//! Black Ops II sound bank entries to 16-bit PCM WAV (what Zero Hour's Miles audio plays).

use zhbo_formats::sab::{SabEntry, SoundBank, FORMAT_FLAC, FORMAT_PCM16};

/// Decode one bank entry to WAV; `None` for codecs we do not handle.
pub fn entry_to_wav(bank: &SoundBank, e: &SabEntry) -> Result<Option<Vec<u8>>, String> {
    let data = bank.read(e).map_err(|err| err.to_string())?;
    Ok(match e.format {
        FORMAT_FLAC => Some(flac_to_wav(&data)?),
        FORMAT_PCM16 => Some(wav_bytes(&data, e.channels as u16, e.sample_rate().unwrap_or(48000))),
        _ => None,
    })
}

/// Decode a FLAC stream to 16-bit PCM WAV (Zero Hour's Miles audio plays 16-bit WAV).
pub fn flac_to_wav(flac: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(flac)).map_err(|e| e.to_string())?;
    let info = reader.streaminfo();
    let channels = info.channels as u16;
    let rate = info.sample_rate;
    let shift = info.bits_per_sample as i32 - 16;
    let mut pcm: Vec<u8> = Vec::new();
    for s in reader.samples() {
        let s = s.map_err(|e| e.to_string())?;
        let v = if shift >= 0 { s >> shift } else { s << -shift };
        pcm.extend_from_slice(&(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes());
    }
    Ok(wav_bytes(&pcm, channels, rate))
}

pub fn wav_bytes(pcm: &[u8], channels: u16, rate: u32) -> Vec<u8> {
    let mut w = Vec::with_capacity(44 + pcm.len());
    let block = channels * 2;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * block as u32).to_le_bytes());
    w.extend_from_slice(&block.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_well_formed() {
        let w = wav_bytes(&[0, 0, 1, 0], 1, 48000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 4);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 48000);
        assert_eq!(w.len(), 48);
    }
}
