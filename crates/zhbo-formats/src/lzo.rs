//! LZO1X decompression (the codec Black Ops II uses inside `.ipak` image packs).
//!
//! A straight port of the reference decoder's state machine (`lzo1x_d.ch`), with bounds checks
//! instead of undefined behaviour on corrupt input.

use crate::{FormatError, Result};

#[derive(Clone, Copy)]
enum State {
    Instruction,
    FirstLiteralRun,
    Match(usize),
    MatchDone,
    MatchNext(usize),
}

fn corrupt(msg: &str) -> FormatError {
    FormatError::Corrupt(format!("lzo1x: {msg}"))
}

/// Decompress one LZO1X stream. Returns the output and the number of input bytes consumed.
pub fn decompress(src: &[u8], size_hint: usize) -> Result<(Vec<u8>, usize)> {
    let mut out: Vec<u8> = Vec::with_capacity(size_hint);
    let mut ip = 0usize;

    let byte = |ip: &mut usize| -> Result<usize> {
        let b = *src.get(*ip).ok_or_else(|| corrupt("input overrun"))?;
        *ip += 1;
        Ok(b as usize)
    };
    let le16 = |ip: &mut usize| -> Result<usize> {
        let lo = *src.get(*ip).ok_or_else(|| corrupt("input overrun"))? as usize;
        let hi = *src.get(*ip + 1).ok_or_else(|| corrupt("input overrun"))? as usize;
        *ip += 2;
        Ok(lo | (hi << 8))
    };
    let literals = |out: &mut Vec<u8>, ip: &mut usize, n: usize| -> Result<()> {
        let s = src.get(*ip..*ip + n).ok_or_else(|| corrupt("literal overrun"))?;
        out.extend_from_slice(s);
        *ip += n;
        Ok(())
    };
    // Back-references may overlap their own output, so copy byte by byte.
    let back_copy = |out: &mut Vec<u8>, dist: usize, n: usize| -> Result<()> {
        if dist == 0 || dist > out.len() {
            return Err(corrupt("match distance outside output"));
        }
        let start = out.len() - dist;
        for k in 0..n {
            let b = out[start + k];
            out.push(b);
        }
        Ok(())
    };
    // Lengths extended by runs of zero bytes.
    let extended = |ip: &mut usize, base: usize| -> Result<usize> {
        let mut t = 0usize;
        loop {
            let b = byte(ip)?;
            if b != 0 {
                return Ok(t + base + b);
            }
            t += 255;
        }
    };

    let mut state = State::Instruction;
    if let Some(&first) = src.first() {
        if first > 17 {
            ip = 1;
            let t = first as usize - 17;
            if t < 4 {
                state = State::MatchNext(t);
            } else {
                literals(&mut out, &mut ip, t)?;
                state = State::FirstLiteralRun;
            }
        }
    }

    loop {
        state = match state {
            State::Instruction => {
                let t = byte(&mut ip)?;
                if t >= 16 {
                    State::Match(t)
                } else {
                    let n = if t == 0 { extended(&mut ip, 15)? } else { t };
                    literals(&mut out, &mut ip, n + 3)?;
                    State::FirstLiteralRun
                }
            }
            State::FirstLiteralRun => {
                let t = byte(&mut ip)?;
                if t >= 16 {
                    State::Match(t)
                } else {
                    let dist = 1 + 0x0800 + (t >> 2) + (byte(&mut ip)? << 2);
                    back_copy(&mut out, dist, 3)?;
                    State::MatchDone
                }
            }
            State::Match(t) => {
                if t >= 64 {
                    let dist = 1 + ((t >> 2) & 7) + (byte(&mut ip)? << 3);
                    back_copy(&mut out, dist, (t >> 5) + 1)?;
                } else if t >= 32 {
                    let n = if t & 31 == 0 { extended(&mut ip, 31)? } else { t & 31 };
                    let dist = 1 + (le16(&mut ip)? >> 2);
                    back_copy(&mut out, dist, n + 2)?;
                } else if t >= 16 {
                    let high = (t & 8) << 11;
                    let n = if t & 7 == 0 { extended(&mut ip, 7)? } else { t & 7 };
                    let low = le16(&mut ip)? >> 2;
                    if high == 0 && low == 0 {
                        return Ok((out, ip)); // end-of-stream marker
                    }
                    back_copy(&mut out, high + low + 0x4000, n + 2)?;
                } else {
                    let dist = 1 + (t >> 2) + (byte(&mut ip)? << 2);
                    back_copy(&mut out, dist, 2)?;
                }
                State::MatchDone
            }
            State::MatchDone => {
                let t = (src[ip - 2] & 3) as usize;
                if t == 0 {
                    State::Instruction
                } else {
                    State::MatchNext(t)
                }
            }
            State::MatchNext(t) => {
                literals(&mut out, &mut ip, t)?;
                State::Match(byte(&mut ip)?)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_only_stream() {
        // 21 = 17 + 4 literals, then the end-of-stream marker (0x11 0x00 0x00).
        let src = [21, b'a', b'b', b'c', b'd', 0x11, 0, 0];
        let (out, used) = decompress(&src, 0).unwrap();
        assert_eq!(out, b"abcd");
        assert_eq!(used, src.len());
    }

    #[test]
    fn overlapping_back_reference() {
        // Literal "ab", then an M2 match (t >= 64): length 3+... copying from distance 2 repeats "ab".
        // t = 0b010_000_00 | ((dist-1)&7)<<2 = 64 + (1<<2); len = (t>>5)+1 = 3; next byte = (dist-1)>>3 = 0.
        let src = [19, b'a', b'b', 64 | (1 << 2), 0, 0x11, 0, 0];
        let (out, _) = decompress(&src, 0).unwrap();
        assert_eq!(out, b"ababa");
    }

    #[test]
    fn corrupt_input_is_an_error_not_a_panic() {
        assert!(decompress(&[0x40, 0xff], 0).is_err());
        assert!(decompress(&[], 0).is_err());
    }
}
