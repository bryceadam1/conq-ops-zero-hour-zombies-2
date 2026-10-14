//! Treyarch `IWi` images, version 27 (Black Ops II), decoded to RGBA8.
//!
//! ```text
//! 0x00 "IWi", u8 version (27)
//! 0x04 u8 format, u8 flags, u16 width, u16 height, u16 depth
//! 0x20 u32[8] end offsets of mip levels, largest first (mip 0 ends at the first value)
//! 0x40 mip data, smallest level first
//! ```

use crate::{FormatError, Result};

pub const HEADER_LEN: usize = 0x40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Argb32,
    Rgb24,
    Dxt1,
    Dxt3,
    Dxt5,
    /// Two-channel BC5 (normal maps).
    Bc5,
    Other(u8),
}

impl Format {
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => Format::Argb32,
            2 => Format::Rgb24,
            11 => Format::Dxt1,
            12 => Format::Dxt3,
            13 => Format::Dxt5,
            14 => Format::Bc5,
            o => Format::Other(o),
        }
    }

    /// Bytes for one mip level.
    pub fn level_size(self, w: usize, h: usize) -> Option<usize> {
        let blocks = w.div_ceil(4).max(1) * h.div_ceil(4).max(1);
        match self {
            Format::Argb32 => Some(w * h * 4),
            Format::Rgb24 => Some(w * h * 3),
            Format::Dxt1 => Some(blocks * 8),
            Format::Dxt3 | Format::Dxt5 | Format::Bc5 => Some(blocks * 16),
            Format::Other(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub format: Format,
    pub flags: u8,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub mip0_end: u32,
}

pub fn parse_header(data: &[u8]) -> Result<Header> {
    if data.len() < HEADER_LEN {
        return Err(FormatError::Truncated("IWi header"));
    }
    if &data[0..3] != b"IWi" {
        return Err(FormatError::BadMagic { expected: "IWi", found: data[0..4].try_into().unwrap() });
    }
    let u16_at = |o: usize| u16::from_le_bytes(data[o..o + 2].try_into().unwrap());
    Ok(Header {
        version: data[3],
        format: Format::from_code(data[4]),
        flags: data[5],
        width: u16_at(6),
        height: u16_at(8),
        depth: u16_at(10),
        mip0_end: u32::from_le_bytes(data[0x20..0x24].try_into().unwrap()),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Decode the largest mip level of a whole IWi file.
pub fn decode(data: &[u8]) -> Result<Rgba> {
    let h = parse_header(data)?;
    let (w, hh) = (h.width as usize, h.height as usize);
    let size = h.format.level_size(w, hh).ok_or_else(|| FormatError::Corrupt(format!("unsupported IWi format {:?}", h.format)))?;
    let end = (h.mip0_end as usize).min(data.len());
    let start = end.checked_sub(size).filter(|&s| s >= HEADER_LEN).ok_or(FormatError::Truncated("IWi mip 0"))?;
    let src = &data[start..end];
    let pixels = match h.format {
        Format::Argb32 => src.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect(),
        Format::Rgb24 => src.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0], 255]).collect(),
        f => decode_blocks(src, w, hh, f),
    };
    Ok(Rgba { width: w as u32, height: hh as u32, pixels })
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
}

fn color_block(b: &[u8], out: &mut [[u8; 4]; 16], dxt1: bool) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u32, wb: u32| ((a as u32 * wa + b as u32 * wb) / (wa + wb)) as u8;
    let mut pal = [[p0[0], p0[1], p0[2], 255], [p1[0], p1[1], p1[2], 255], [0; 4], [0; 4]];
    if c0 > c1 || !dxt1 {
        pal[2] = [mix(p0[0], p1[0], 2, 1), mix(p0[1], p1[1], 2, 1), mix(p0[2], p1[2], 2, 1), 255];
        pal[3] = [mix(p0[0], p1[0], 1, 2), mix(p0[1], p1[1], 1, 2), mix(p0[2], p1[2], 1, 2), 255];
    } else {
        pal[2] = [mix(p0[0], p1[0], 1, 1), mix(p0[1], p1[1], 1, 1), mix(p0[2], p1[2], 1, 1), 255];
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, px) in out.iter_mut().enumerate() {
        *px = pal[((bits >> (2 * i)) & 3) as usize];
    }
}

/// BC3/BC4-style interpolated 8-bit channel block (8 bytes).
fn alpha_block(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut pal = [0u32; 8];
    pal[0] = a0;
    pal[1] = a1;
    if a0 > a1 {
        for i in 1..7 {
            pal[i + 1] = ((7 - i as u32) * a0 + i as u32 * a1) / 7;
        }
    } else {
        for i in 1..5 {
            pal[i + 1] = ((5 - i as u32) * a0 + i as u32 * a1) / 5;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let bits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | x as u64);
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (3 * i)) & 7) as usize] as u8;
    }
    out
}

fn decode_blocks(src: &[u8], w: usize, h: usize, f: Format) -> Vec<u8> {
    let bw = w.div_ceil(4).max(1);
    let bh = h.div_ceil(4).max(1);
    let bsize = if f == Format::Dxt1 { 8 } else { 16 };
    let mut out = vec![0u8; w * h * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let o = (by * bw + bx) * bsize;
            let Some(b) = src.get(o..o + bsize) else { continue };
            let mut px = [[0u8; 4]; 16];
            match f {
                Format::Dxt1 => color_block(b, &mut px, true),
                Format::Dxt3 => {
                    color_block(&b[8..], &mut px, false);
                    for (i, p) in px.iter_mut().enumerate() {
                        let nib = (b[i / 2] >> ((i % 2) * 4)) & 15;
                        p[3] = nib * 17;
                    }
                }
                Format::Dxt5 => {
                    color_block(&b[8..], &mut px, false);
                    let a = alpha_block(&b[0..8]);
                    for (p, a) in px.iter_mut().zip(a) {
                        p[3] = a;
                    }
                }
                Format::Bc5 => {
                    let (r, g) = (alpha_block(&b[0..8]), alpha_block(&b[8..16]));
                    for i in 0..16 {
                        px[i] = [r[i], g[i], 255, 255];
                    }
                }
                _ => {}
            }
            for (i, p) in px.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                if x < w && y < h {
                    out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(p);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iwi(format: u8, w: u16, h: u16, mip0: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; HEADER_LEN];
        d[0..4].copy_from_slice(b"IWi\x1b");
        d[4] = format;
        d[6..8].copy_from_slice(&w.to_le_bytes());
        d[8..10].copy_from_slice(&h.to_le_bytes());
        d[10..12].copy_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(mip0);
        let end = d.len() as u32;
        d[0x20..0x24].copy_from_slice(&end.to_le_bytes());
        d
    }

    #[test]
    fn argb_is_swizzled_to_rgba() {
        let img = decode(&iwi(1, 1, 1, &[10, 20, 30, 40])).unwrap();
        assert_eq!(img.pixels, vec![30, 20, 10, 40]);
    }

    #[test]
    fn dxt1_solid_block() {
        // c0 = pure red (0xF800), c1 = black, all indices 0 -> every pixel red.
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let img = decode(&iwi(11, 4, 4, &block)).unwrap();
        assert!(img.pixels.chunks(4).all(|p| p == [255, 0, 0, 255]));
    }

    #[test]
    fn dxt5_alpha_endpoints() {
        // Alpha 255/0 with all indices 1 -> alpha 0; colour white.
        let mut block = vec![255, 0];
        block.extend_from_slice(&[0b0100_1001, 0b1001_0010, 0b0010_0100, 0b0100_1001, 0b1001_0010, 0b0010_0100]);
        block.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
        let img = decode(&iwi(13, 4, 4, &block)).unwrap();
        assert!(img.pixels.chunks(4).all(|p| p[3] == 0 && p[0] == 255));
    }

    #[test]
    fn level_sizes() {
        assert_eq!(Format::Dxt5.level_size(512, 128), Some(65536));
        assert_eq!(Format::Dxt1.level_size(2, 2), Some(8));
    }
}
