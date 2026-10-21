//! Reading the converter's textures back: TGA, uncompressed (type 2) or run-length (type 10),
//! 24 or 32 bits, either origin, into top-down RGBA8 rows (the order the renderer uploads, and the
//! one BO2's UVs address: `zhbo_assets::images::tga_bytes` writes bottom-up, and the W3D path's
//! `1 - v` undid that).

use crate::io::FormatError;

pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGBA8, top row first.
    pub rgba: Vec<u8>,
}

/// The largest side accepted (BO2's own maps are at most 4096).
const MAX_SIDE: u32 = 8192;

pub fn decode(data: &[u8]) -> Result<Image, FormatError> {
    let bad = |m: &str| FormatError::Corrupt(format!("tga: {m}"));
    if data.len() < 18 {
        return Err(bad("short header"));
    }
    let id_len = data[0] as usize;
    let (cmap, kind) = (data[1], data[2]);
    let width = u16::from_le_bytes([data[12], data[13]]) as u32;
    let height = u16::from_le_bytes([data[14], data[15]]) as u32;
    let bpp = data[16] as usize / 8;
    let top_down = data[17] & 0x20 != 0;
    if cmap != 0 || !(kind == 2 || kind == 10) || !(bpp == 3 || bpp == 4) {
        return Err(bad(&format!("type {kind}, colour map {cmap}, {} bits", bpp * 8)));
    }
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(bad(&format!("{width}x{height}")));
    }
    let n = (width * height) as usize;
    let mut src = data.get(18 + id_len..).ok_or_else(|| bad("short id"))?;
    let mut px: Vec<[u8; 4]> = Vec::with_capacity(n);
    let read = |s: &[u8]| [s[2], s[1], s[0], if bpp == 4 { s[3] } else { 255 }];
    if kind == 2 {
        if src.len() < n * bpp {
            return Err(bad("short pixels"));
        }
        px.extend(src[..n * bpp].chunks_exact(bpp).map(read));
    } else {
        while px.len() < n {
            let (&head, rest) = src.split_first().ok_or_else(|| bad("short packet"))?;
            let count = (head & 0x7F) as usize + 1;
            if px.len() + count > n {
                return Err(bad("run past the image"));
            }
            if head & 0x80 != 0 {
                let p = rest.get(..bpp).ok_or_else(|| bad("short run"))?;
                px.extend(std::iter::repeat_n(read(p), count));
                src = &rest[bpp..];
            } else {
                let raw = rest.get(..count * bpp).ok_or_else(|| bad("short raw packet"))?;
                px.extend(raw.chunks_exact(bpp).map(read));
                src = &rest[count * bpp..];
            }
        }
    }
    let row = width as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    for y in 0..height as usize {
        let sy = if top_down { y } else { height as usize - 1 - y };
        for p in &px[sy * row..(sy + 1) * row] {
            rgba.extend_from_slice(p);
        }
    }
    Ok(Image { width, height, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `zhbo_assets::images::tga_bytes` writes: 32-bit, bottom-left origin, BGRA.
    fn bottom_up(w: u16, h: u16, rows_top_first: &[[u8; 4]]) -> Vec<u8> {
        let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&[32, 0x08]);
        for y in (0..h as usize).rev() {
            for p in &rows_top_first[y * w as usize..(y + 1) * w as usize] {
                out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        out
    }

    #[test]
    fn reads_the_converters_files_top_down() {
        let px = [[255, 0, 0, 255], [0, 255, 0, 128], [0, 0, 255, 255], [9, 9, 9, 0]];
        let img = decode(&bottom_up(2, 2, &px)).unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(img.rgba, px.concat());
    }

    #[test]
    fn reads_run_length_and_refuses_damage() {
        // 3x1, 24-bit, top-left origin: a run of two reds then one raw blue.
        let mut d = vec![0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 1, 0, 24, 0x20];
        d.extend_from_slice(&[0x81, 0, 0, 255, 0x00, 255, 0, 0]);
        let img = decode(&d).unwrap();
        assert_eq!(img.rgba, [255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255]);
        for n in 0..d.len() {
            assert!(decode(&d[..n]).is_err(), "prefix {n}");
        }
        let mut overrun = d.clone();
        overrun[18] = 0x85;
        assert!(decode(&overrun).is_err());
    }
}
