//! Black Ops II textures from the player's `.ipak` packs: lookup by name hash, decode, PNG/TGA output,
//! and contact sheets for identifying images by eye.

use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::ipak::{Ipak, IpakEntry};
use zhbo_formats::iwi::{self, Rgba};

/// All entries of one image: large images are split over consecutive index entries with the same
/// name hash.
pub fn group_by_name(pak: &Ipak) -> BTreeMap<u32, Vec<IpakEntry>> {
    let mut map: BTreeMap<u32, Vec<IpakEntry>> = BTreeMap::new();
    for e in pak.entries() {
        map.entry(e.name_hash).or_default().push(*e);
    }
    for parts in map.values_mut() {
        parts.sort_by_key(|e| e.data_hash);
    }
    map
}

pub fn load(pak: &Ipak, parts: &[IpakEntry]) -> Result<Rgba, String> {
    let mut data = Vec::new();
    for p in parts {
        data.extend(pak.read(p, None).map_err(|e| e.to_string())?);
    }
    iwi::decode(&data).map_err(|e| e.to_string())
}

pub fn header(pak: &Ipak, parts: &[IpakEntry]) -> Option<iwi::Header> {
    let first = parts.first()?;
    let head = pak.read(first, Some(iwi::HEADER_LEN)).ok()?;
    iwi::parse_header(&head).ok()
}

pub fn write_png(path: &Path, img: &Rgba) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(&img.pixels).map_err(|e| e.to_string())
}

/// Uncompressed 32-bit TGA, bottom-up rows (what Zero Hour's texture loader reads for UI art).
pub fn tga_bytes(img: &Rgba) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + img.pixels.len());
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&(img.width as u16).to_le_bytes());
    out.extend_from_slice(&(img.height as u16).to_le_bytes());
    out.push(32);
    out.push(0x08); // 8 alpha bits, bottom-left origin
    let row = img.width as usize * 4;
    for y in (0..img.height as usize).rev() {
        for p in img.pixels[y * row..(y + 1) * row].chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

/// Nearest-neighbour fit into a `cell`x`cell` box, on a checkerboard so alpha is visible.
fn thumb(img: &Rgba, cell: u32) -> Rgba {
    let scale = (cell as f32 / img.width.max(img.height) as f32).min(1.0);
    let (tw, th) = (((img.width as f32 * scale) as u32).max(1), ((img.height as f32 * scale) as u32).max(1));
    let mut px = vec![0u8; (cell * cell * 4) as usize];
    for y in 0..cell {
        for x in 0..cell {
            let checker = if (x / 8 + y / 8) % 2 == 0 { 70 } else { 110 };
            let mut c = [checker, checker, checker, 255];
            if x < tw && y < th {
                let sx = (x as f32 / scale) as u32;
                let sy = (y as f32 / scale) as u32;
                let i = ((sy.min(img.height - 1) * img.width + sx.min(img.width - 1)) * 4) as usize;
                let s = &img.pixels[i..i + 4];
                let a = s[3] as u32;
                for k in 0..3 {
                    c[k] = ((s[k] as u32 * a + c[k] as u32 * (255 - a)) / 255) as u8;
                }
            }
            let o = ((y * cell + x) * 4) as usize;
            px[o..o + 4].copy_from_slice(&c);
        }
    }
    Rgba { width: cell, height: cell, pixels: px }
}

/// Lay out thumbnails `cols` wide with a 2-pixel gutter.
pub fn sheet(images: &[Rgba], cell: u32, cols: u32) -> Rgba {
    let rows = (images.len() as u32).div_ceil(cols).max(1);
    let pitch = cell + 2;
    let (w, h) = (cols * pitch, rows * pitch);
    let mut px = vec![255u8; (w * h * 4) as usize];
    for (n, img) in images.iter().enumerate() {
        let t = thumb(img, cell);
        let (ox, oy) = ((n as u32 % cols) * pitch, (n as u32 / cols) * pitch);
        for y in 0..cell {
            let src = &t.pixels[(y * cell * 4) as usize..((y + 1) * cell * 4) as usize];
            let o = (((oy + y) * w + ox) * 4) as usize;
            px[o..o + src.len()].copy_from_slice(src);
        }
    }
    Rgba { width: w, height: h, pixels: px }
}
