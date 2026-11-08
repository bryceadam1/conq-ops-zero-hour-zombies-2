//! Audio into what Whisper takes: 16 kHz mono `f32`.

use crate::Error;

/// Interleaved `channels`-channel audio at `rate` Hz as 16 kHz mono: the channels averaged, then
/// each output sample the mean of the input samples it spans (a box low-pass that keeps speech's
/// band and drops what would alias), linearly interpolated when the input is slower.
pub fn to_whisper(samples: &[f32], channels: usize, rate: u32) -> Vec<f32> {
    let channels = channels.max(1);
    let mono: Vec<f32> = samples.chunks_exact(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect();
    let target = crate::hear::SAMPLE_RATE as f64;
    if rate as f64 == target || mono.is_empty() {
        return mono;
    }
    let step = rate as f64 / target;
    let n = (mono.len() as f64 / step).floor() as usize;
    (0..n)
        .map(|i| {
            let (from, to) = (i as f64 * step, (i + 1) as f64 * step);
            if step > 1.0 {
                let (a, b) = (from.floor() as usize, (to.ceil() as usize).min(mono.len()));
                mono[a..b].iter().sum::<f32>() / (b - a).max(1) as f32
            } else {
                let a = from.floor() as usize;
                let t = (from - a as f64) as f32;
                let next = mono.get(a + 1).copied().unwrap_or(mono[a]);
                mono[a] * (1.0 - t) + next * t
            }
        })
        .collect()
}

/// A WAV file (16-bit PCM or 32-bit float, any rate and channel count) as 16 kHz mono.
pub fn read_wav(bytes: &[u8]) -> Result<Vec<f32>, Error> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut format = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let len = u32_at(at + 4) as usize;
        let body = at + 8;
        let end = (body + len).min(bytes.len());
        match &bytes[at..at + 4] {
            b"fmt " if len >= 16 => format = Some((u16_at(body), u16_at(body + 2) as usize, u32_at(body + 4), u16_at(body + 14))),
            b"data" => {
                let Some((kind, channels, rate, bits)) = format else { return Err("WAV data before its format".into()) };
                let data = &bytes[body..end];
                let samples: Vec<f32> = match (kind, bits) {
                    (1, 16) => data.as_chunks::<2>().0.iter().map(|&b| i16::from_le_bytes(b) as f32 / 32768.0).collect(),
                    (3, 32) => data.as_chunks::<4>().0.iter().map(|&b| f32::from_le_bytes(b)).collect(),
                    _ => return Err(format!("WAV format {kind} at {bits} bits is not read").into()),
                };
                return Ok(to_whisper(&samples, channels, rate));
            }
            _ => {}
        }
        at = body + len + (len & 1);
    }
    Err("no data in the WAV file".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_becomes_16_khz_mono() {
        // 48 kHz stereo: a second of a 440 Hz tone in both ears.
        let tone: Vec<f32> = (0..48000).flat_map(|i| {
            let s = (i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin() * 0.5;
            [s, s]
        }).collect();
        let out = to_whisper(&tone, 2, 48000);
        assert_eq!(out.len(), 16000);
        // The tone survives: its loudness is about the input's (0.5 / sqrt 2).
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!((rms - 0.354).abs() < 0.02, "rms {rms}");
        // 8 kHz up to 16 kHz doubles the samples.
        assert_eq!(to_whisper(&[0.0, 1.0, 0.0, 1.0], 1, 8000).len(), 8);
    }

    #[test]
    fn a_wav_file_reads() {
        let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&32000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(&16384i16.to_le_bytes());
        wav.extend_from_slice(&(-32768i16).to_le_bytes());
        assert_eq!(read_wav(&wav).unwrap(), vec![0.5, -1.0]);
        assert!(read_wav(b"nonsense").is_err());
    }
}
