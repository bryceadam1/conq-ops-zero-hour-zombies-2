//! RefPack decompression, the codec Zero Hour's `CompressionManager` uses for `EAR\0` files (its
//! shipped `.map` files among them).
//!
//! A port of the engine's `REF_decode` (`Core/Libraries/Source/Compression/EAC/refdecode.cpp`),
//! with bounds checks instead of undefined behaviour on corrupt input. The `EAR\0` container is
//! `CompressionManager`'s: the tag, the uncompressed size (`u32`, little-endian), then the RefPack
//! stream.

use crate::{FormatError, Result};

fn corrupt(msg: &str) -> FormatError {
    FormatError::Corrupt(format!("refpack: {msg}"))
}

/// The file as the engine reads it: an `EAR\0` file decompressed, anything else as it is
/// (`CachedFileInputStream::open`).
pub fn decompress_file(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.get(..4) != Some(b"EAR\0".as_slice()) {
        return Ok(bytes.to_vec());
    }
    let size = bytes.get(4..8).ok_or(FormatError::Truncated("the EAR header"))?;
    let size = u32::from_le_bytes([size[0], size[1], size[2], size[3]]) as usize;
    let out = decode(&bytes[8..])?;
    if out.len() != size {
        return Err(corrupt(&format!("{} bytes out, the header says {size}", out.len())));
    }
    Ok(out)
}

/// Decode one RefPack stream (`REF_decode`).
pub fn decode(src: &[u8]) -> Result<Vec<u8>> {
    let mut s = 0usize;
    let byte = |s: &mut usize| -> Result<usize> {
        let b = *src.get(*s).ok_or_else(|| corrupt("input overrun"))?;
        *s += 1;
        Ok(b as usize)
    };
    let kind = (byte(&mut s)? << 8) | byte(&mut s)?;
    let width = if kind & 0x8000 != 0 { 4 } else { 3 };
    if kind & 0x100 != 0 {
        s += width;
    }
    let mut ulen = 0usize;
    for _ in 0..width {
        ulen = (ulen << 8) | byte(&mut s)?;
    }
    // The size field is the producer's claim; cap the reservation, never trust it for indexing.
    let mut out: Vec<u8> = Vec::with_capacity(ulen.min(1 << 26));
    let literal = |out: &mut Vec<u8>, s: &mut usize, n: usize| -> Result<()> {
        let run = src.get(*s..*s + n).ok_or_else(|| corrupt("literal overrun"))?;
        out.extend_from_slice(run);
        *s += n;
        Ok(())
    };
    let copy = |out: &mut Vec<u8>, back: usize, n: usize| -> Result<()> {
        let from = out.len().checked_sub(back + 1).ok_or_else(|| corrupt("reference before the start"))?;
        for i in 0..n {
            let b = out[from + i];
            out.push(b);
        }
        Ok(())
    };
    loop {
        let first = byte(&mut s)?;
        if first & 0x80 == 0 {
            let second = byte(&mut s)?;
            literal(&mut out, &mut s, first & 3)?;
            copy(&mut out, ((first & 0x60) << 3) + second, ((first & 0x1c) >> 2) + 3)?;
        } else if first & 0x40 == 0 {
            let (second, third) = (byte(&mut s)?, byte(&mut s)?);
            literal(&mut out, &mut s, second >> 6)?;
            copy(&mut out, ((second & 0x3f) << 8) + third, (first & 0x3f) + 4)?;
        } else if first & 0x20 == 0 {
            let (second, third, fourth) = (byte(&mut s)?, byte(&mut s)?, byte(&mut s)?);
            literal(&mut out, &mut s, first & 3)?;
            copy(&mut out, (((first & 0x10) >> 4) << 16) + (second << 8) + third, (((first & 0x0c) >> 2) << 8) + fourth + 5)?;
        } else {
            let run = ((first & 0x1f) << 2) + 4;
            if run <= 112 {
                literal(&mut out, &mut s, run)?;
            } else {
                literal(&mut out, &mut s, first & 3)?;
                break;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-made stream: a literal run, a short back reference into it, and the end code with
    /// two trailing literals.
    #[test]
    fn decodes_literals_back_references_and_the_end() {
        // Header 0x10FB, size 3 bytes = 12.
        let mut src = vec![0x10, 0xFB, 0, 0, 12];
        // Literal run of 4 (first = 0xE0: ((0 & 0x1f) << 2) + 4 = 4).
        src.push(0xE0);
        src.extend_from_slice(b"abcd");
        // Short form: 0 plain, copy 6 (((first & 0x1c) >> 2) + 3 = 6 -> 0x0C), offset 4 back (3 + 1).
        src.extend_from_slice(&[0x0C, 3]);
        // End with 2 literals.
        src.push(0xFC | 2);
        src.extend_from_slice(b"yz");
        assert_eq!(decode(&src).unwrap(), b"abcdabcdabyz");
        let mut file = b"EAR\0".to_vec();
        file.extend_from_slice(&12u32.to_le_bytes());
        file.extend_from_slice(&src);
        assert_eq!(decompress_file(&file).unwrap(), b"abcdabcdabyz");
        assert_eq!(decompress_file(b"CkMp....").unwrap(), b"CkMp....", "an uncompressed file reads as it is");
    }

    #[test]
    fn corrupt_streams_are_errors() {
        assert!(decode(&[0x10]).is_err());
        // A reference before the start of the output.
        assert!(decode(&[0x10, 0xFB, 0, 0, 4, 0x0C, 9]).is_err());
        // No end code.
        assert!(decode(&[0x10, 0xFB, 0, 0, 4, 0xE0, b'a', b'b', b'c', b'd']).is_err());
        let mut file = b"EAR\0".to_vec();
        file.extend_from_slice(&99u32.to_le_bytes());
        file.extend_from_slice(&[0x10, 0xFB, 0, 0, 0, 0xFC]);
        assert!(decompress_file(&file).is_err(), "the size must match the header");
    }
}
