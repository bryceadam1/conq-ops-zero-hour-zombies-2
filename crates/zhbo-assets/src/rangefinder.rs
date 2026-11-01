/*
** Conquer Ops - converter: the target finder's lens display, from the player's install.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The target finder's ui3d display (`zhbo_native::display`). BO2's code (`t6mp` `FUN_00526780`)
//! draws it from a sprite atlas, `mtl_t6_attach_optic_rangefinder_ui3d_background`, and a table of
//! sprite quads it builds on the stack from constants in `t6mp.exe`'s `.rdata`. The numbers are
//! **read from the player's own `t6mp.exe`** (its `.rdata` is not encrypted; only file bytes are
//! read): [`IDLE`] and [`SPOTTED`] name, per table slot, the address its value is loaded from (the
//! function's `MOV [ESP+…]` stores; `None` is a literal 0), so nothing is hand-copied. The two
//! sprite sets are baked into window-sized frames; the atlas and the diamond's rectangle go to the
//! renderer, which places the diamonds per frame.

use std::collections::BTreeMap;
use std::path::Path;

use crate::character;
use crate::images;
use crate::paks::Paks;
use zhbo_formats::iwi::Rgba;
use zhbo_formats::{t6zone::Zone, w3d, xmodel};
use zhbo_native::display::RangeFinderDisplay;

/// The atlas material BO2's code draws the display with (`FUN_004d4980` registers it).
const ATLAS: &str = "mtl_t6_attach_optic_rangefinder_ui3d_background";

/// One table slot: the `.rdata` address its value is loaded from, `None` for a literal 0.
type Slot = Option<u32>;

/// `FUN_00526780`'s idle table (`local_260`..): six quads of `x, y, w, h, s, t, ds, dt, rotation`
/// (window units; atlas texels; degrees).
#[rustfmt::skip]
const IDLE: [[Slot; 9]; 6] = [
    [Some(0x00b26e54), Some(0x00b26e54), Some(0x00b26e58), Some(0x00b26e58), None, None, Some(0x00b26e58), Some(0x00b26e58), None],
    [Some(0x00b26d64), Some(0x00c73428), Some(0x00c73564), Some(0x00c73578), None, Some(0x00b26e58), Some(0x00c73564), Some(0x00c73578), None],
    [Some(0x00b26d64), Some(0x00b26e5c), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26de4), None, Some(0x00bd08c8), Some(0x00bd08c8), None],
    [Some(0x00b26e4c), Some(0x00b26e5c), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26de4), None, Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b12e30)],
    [Some(0x00b26d64), Some(0x00b26e50), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26de4), None, Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00c733e0)],
    [Some(0x00b26e4c), Some(0x00b26e50), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26de4), None, Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00c734ec)],
];

/// The spotted table (`local_188`..): ten quads, drawn while `FUN_00525ad0` (an enemy spotted).
#[rustfmt::skip]
const SPOTTED: [[Slot; 9]; 10] = [
    [Some(0x00b26e54), Some(0x00b26e54), Some(0x00b26e58), Some(0x00b26e58), Some(0x00b26e58), None, Some(0x00b26e58), Some(0x00b26e58), None],
    [Some(0x00b26d64), Some(0x00c73428), Some(0x00c73564), Some(0x00c73578), None, Some(0x00b26dc0), Some(0x00c73564), Some(0x00c73578), None],
    [Some(0x00b26d64), Some(0x00b26e5c), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26e48), None, Some(0x00bd08c8), Some(0x00bd08c8), None],
    [Some(0x00b26e4c), Some(0x00b26e5c), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26e48), None, Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b12e30)],
    [Some(0x00b26d64), Some(0x00b26e50), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26e48), None, Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00c733e0)],
    [Some(0x00b26e4c), Some(0x00b26e50), Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00b26e48), None, Some(0x00bd08c8), Some(0x00bd08c8), Some(0x00c734ec)],
    [Some(0x00b0c434), Some(0x00c73428), Some(0x00b26e44), Some(0x00b26e44), Some(0x00c73564), Some(0x00b26e58), Some(0x00b26e44), Some(0x00b26e44), None],
    [Some(0x00b26d64), Some(0x00b0c434), Some(0x00b26e44), Some(0x00b26e44), Some(0x00c73564), Some(0x00b26e58), Some(0x00b26e44), Some(0x00b26e44), Some(0x00c733e0)],
    [Some(0x00b26e4c), Some(0x00b0c434), Some(0x00b26e44), Some(0x00b26e44), Some(0x00c73564), Some(0x00b26e58), Some(0x00b26e44), Some(0x00b26e44), Some(0x00b12e30)],
    [Some(0x00b0c434), Some(0x00b26e40), Some(0x00b26e44), Some(0x00b26e44), Some(0x00c73564), Some(0x00b26e58), Some(0x00b26e44), Some(0x00b26e44), Some(0x00c734ec)],
];

/// The atlas texel scales (`s · 0x00c73484`, `t · 0x00b26e3c`), degrees to radians (`0x00c73660`),
/// the window's size (`0x00b11498`), the diamond's atlas rectangle (`0x00b26e2c`..`0x00b26e38`),
/// `cg_rangeFinderDiamondSize`'s default (`0x00c735b8`, `FUN_004cc260`) and its window scale
/// (`0x00c735e0`).
const TEXEL_S: u32 = 0x00c73484;
const TEXEL_T: u32 = 0x00b26e3c;
const DEG: u32 = 0x00c73660;
const WINDOW: u32 = 0x00b11498;
const DIAMOND_UV: [u32; 4] = [0x00b26e2c, 0x00b26e30, 0x00b26e34, 0x00b26e38];
const DIAMOND_SIZE: u32 = 0x00c735b8;
const DIAMOND_SCALE: u32 = 0x00c735e0;

/// `t6mp.exe`'s bytes, read at virtual addresses through its section table.
pub(crate) struct Exe {
    bytes: Vec<u8>,
    /// (virtual address, raw size, file offset) per section.
    sections: Vec<(u32, u32, u32)>,
}

impl Exe {
    pub(crate) fn parse(bytes: Vec<u8>) -> Option<Exe> {
        let u16_at = |o: usize| bytes.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let pe = u32_at(0x3c)? as usize;
        if bytes.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let count = u16_at(pe + 6)? as usize;
        let optional = u16_at(pe + 20)? as usize;
        let base = u32_at(pe + 24 + 28)?;
        let mut sections = Vec::with_capacity(count);
        for i in 0..count {
            let o = pe + 24 + optional + 40 * i;
            sections.push((base.wrapping_add(u32_at(o + 12)?), u32_at(o + 16)?, u32_at(o + 20)?));
        }
        Some(Exe { bytes, sections })
    }

    pub(crate) fn f32(&self, va: u32) -> Option<f32> {
        let &(start, _, raw) = self.sections.iter().find(|(s, n, _)| (*s..s.wrapping_add(*n)).contains(&va))?;
        let o = (raw + (va - start)) as usize;
        self.bytes.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn quads<const N: usize>(&self, table: &[[Slot; 9]; N]) -> Option<Vec<[f32; 9]>> {
        table.iter().map(|q| q.iter().map(|s| s.map_or(Some(0.0), |a| self.f32(a))).collect::<Option<Vec<f32>>>().map(|v| std::array::from_fn(|i| v[i]))).collect()
    }
}

/// The numbers the display is drawn by, from the exe.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layout {
    pub idle: Vec<[f32; 9]>,
    pub spotted: Vec<[f32; 9]>,
    pub texel: [f32; 2],
    pub deg: f32,
    pub window: f32,
    pub diamond_uv: [f32; 4],
    pub diamond_size: f32,
}

impl Layout {
    pub(crate) fn read(exe: &Exe) -> Option<Layout> {
        let l = Layout {
            idle: exe.quads(&IDLE)?,
            spotted: exe.quads(&SPOTTED)?,
            texel: [exe.f32(TEXEL_S)?, exe.f32(TEXEL_T)?],
            deg: exe.f32(DEG)?,
            window: exe.f32(WINDOW)?,
            diamond_uv: DIAMOND_UV.map(|a| exe.f32(a)).iter().copied().collect::<Option<Vec<f32>>>().map(|v| [v[0], v[1], v[2], v[3]])?,
            diamond_size: exe.f32(DIAMOND_SIZE)? * exe.f32(DIAMOND_SCALE)?,
        };
        let finite = l.idle.iter().chain(&l.spotted).flatten().chain(&l.texel).chain(&l.diamond_uv).chain([&l.deg, &l.window, &l.diamond_size]).all(|v| v.is_finite());
        (finite && l.window >= 1.0 && l.window <= 4096.0).then_some(l)
    }
}

/// A quad's atlas mapping as `FUN_00526780` builds its vertices: the UV at the quad's top-left
/// corner and the UV steps along its width and height (the texel rectangle turned by `rotation`
/// about its centre).
pub(crate) fn quad_uv(q: &[f32; 9], texel: [f32; 2], deg: f32) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let (s, t, ds, dt, rot) = (q[4], q[5], q[6], q[7], q[8]);
    let ([ks, kt], [hs, ht]) = (texel, [ds * 0.5, dt * 0.5]);
    if rot == 0.0 {
        return ([s * ks, t * kt], [ds * ks, 0.0], [0.0, dt * kt]);
    }
    let (sn, c) = (rot * deg).sin_cos();
    ([(s + hs - hs * c + ht * sn) * ks, (ht + t - hs * sn - ht * c) * kt], [ds * c * ks, ds * sn * kt], [-dt * sn * ks, dt * c * kt])
}

fn sample(atlas: &Rgba, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (atlas.width as f32, atlas.height as f32);
    let (x, y) = (u * w - 0.5, v * h - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let px = |xi: f32, yi: f32| {
        let xi = xi.clamp(0.0, w - 1.0) as u32;
        let yi = yi.clamp(0.0, h - 1.0) as u32;
        let i = ((yi * atlas.width + xi) * 4) as usize;
        [0, 1, 2, 3].map(|k| atlas.pixels[i + k] as f32 / 255.0)
    };
    let (a, b, c, d) = (px(x0, y0), px(x0 + 1.0, y0), px(x0, y0 + 1.0), px(x0 + 1.0, y0 + 1.0));
    [0, 1, 2, 3].map(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy)
}

/// The window with `quads` drawn from the atlas (white, BO2's colour for them), blended as the
/// atlas material blends, one texel per window unit.
pub(crate) fn bake(atlas: &Rgba, quads: &[[f32; 9]], l: &Layout, blend: w3d::Blend) -> Rgba {
    let n = l.window.round() as u32;
    let mut out = vec![0.0f32; (n * n * 4) as usize];
    for q in quads {
        let (o, ex, ey) = quad_uv(q, l.texel, l.deg);
        let (x0, y0, w, h) = (q[0], q[1], q[2].max(1e-3), q[3].max(1e-3));
        for py in (y0.floor().max(0.0) as u32)..((y0 + h).ceil().min(n as f32) as u32) {
            for px in (x0.floor().max(0.0) as u32)..((x0 + w).ceil().min(n as f32) as u32) {
                let (a, b) = ((px as f32 + 0.5 - x0) / w, (py as f32 + 0.5 - y0) / h);
                if !(0.0..=1.0).contains(&a) || !(0.0..=1.0).contains(&b) {
                    continue;
                }
                let src = sample(atlas, o[0] + a * ex[0] + b * ey[0], o[1] + a * ex[1] + b * ey[1]);
                let i = ((py * n + px) * 4) as usize;
                let dst = &mut out[i..i + 4];
                for k in 0..4 {
                    dst[k] = match blend {
                        w3d::Blend::Additive { alpha: false } => dst[k] + src[k],
                        w3d::Blend::Additive { alpha: true } => dst[k] + src[k] * src[3],
                        w3d::Blend::Premultiplied => src[k] + dst[k] * (1.0 - src[3]),
                        w3d::Blend::Alpha | w3d::Blend::AlphaTest => src[k] * src[3] + dst[k] * (1.0 - src[3]),
                        w3d::Blend::Opaque => src[k],
                    }
                    .clamp(0.0, 1.0);
                }
            }
        }
    }
    Rgba { width: n, height: n, pixels: out.iter().map(|v| (v * 255.0).round() as u8).collect() }
}

/// Convert the display: `None` (logged by the caller) when the exe, the atlas or a number is missing.
pub(crate) fn display(bo2_dir: &Path, zones: &[&Zone], paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>) -> Result<RangeFinderDisplay, String> {
    let exe = std::fs::read(bo2_dir.join("t6mp.exe")).map_err(|e| format!("t6mp.exe: {e}"))?;
    let exe = Exe::parse(exe).ok_or("t6mp.exe: not a PE image")?;
    let layout = Layout::read(&exe).ok_or("t6mp.exe: the display's numbers are not where FUN_00526780 reads them")?;
    let m = zones.iter().find_map(|z| xmodel::find_asset_material(z, ATLAS)).ok_or(format!("{ATLAS}: not in the zones"))?;
    let img = m.color_map().ok_or(format!("{ATLAS}: no colour map"))?;
    let atlas = paks.resolve(img).ok_or(format!("{ATLAS}: image {} not found", img.name))?;
    let blend = m.blend();
    let path = |name: &str| format!("Art\\Textures\\{}", character::texture_file(name));
    let (idle, spotted, atlas_path) = (path("rangefinder_idle"), path("rangefinder_spotted"), path(&img.name));
    files.insert(idle.clone(), images::tga_bytes(&bake(&atlas, &layout.idle, &layout, blend)));
    files.insert(spotted.clone(), images::tga_bytes(&bake(&atlas, &layout.spotted, &layout, blend)));
    files.insert(atlas_path.clone(), images::tga_bytes(&atlas));
    Ok(RangeFinderDisplay { idle, spotted, atlas: atlas_path, diamond_uv: layout.diamond_uv, diamond_size: layout.diamond_size })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unrotated quad maps its texel rectangle straight; a quarter turn turns it about its
    /// centre (the top-left corner takes the rectangle's bottom-left texel).
    #[test]
    fn quads_map_their_texels_as_bo2_builds_the_vertices() {
        let texel = [1.0 / 256.0, 1.0 / 128.0];
        let deg = std::f32::consts::PI / 180.0;
        let (o, ex, ey) = quad_uv(&[0.0, 0.0, 35.0, 35.0, 106.0, 0.0, 35.0, 35.0, 0.0], texel, deg);
        assert_eq!((o, ex, ey), ([106.0 / 256.0, 0.0], [35.0 / 256.0, 0.0], [0.0, 35.0 / 128.0]));
        let (o, ex, ey) = quad_uv(&[0.0, 0.0, 35.0, 35.0, 106.0, 0.0, 35.0, 35.0, 90.0], texel, deg);
        assert!((o[0] - 141.0 / 256.0).abs() < 1e-5 && o[1].abs() < 1e-5, "{o:?}");
        assert!(ex[0].abs() < 1e-5 && (ex[1] - 35.0 / 128.0).abs() < 1e-5 && (ey[0] + 35.0 / 256.0).abs() < 1e-5, "{ex:?} {ey:?}");
    }

    /// The bake draws a quad's texels into its window rectangle and nothing outside it.
    #[test]
    fn the_bake_draws_each_quad_in_its_rectangle() {
        let atlas = Rgba { width: 2, height: 2, pixels: [[255u8, 0, 0, 255]; 4].concat() };
        let l = Layout { idle: Vec::new(), spotted: Vec::new(), texel: [0.5, 0.5], deg: 0.0, window: 16.0, diamond_uv: [0.0; 4], diamond_size: 0.1 };
        let img = bake(&atlas, &[[4.0, 4.0, 4.0, 4.0, 0.0, 0.0, 2.0, 2.0, 0.0]], &l, w3d::Blend::Additive { alpha: false });
        let at = |x: u32, y: u32| img.pixels[((y * 16 + x) * 4) as usize];
        assert_eq!((at(5, 5), at(2, 2), at(9, 5)), (255, 0, 0));
    }

    /// The player's install: the numbers come out of `t6mp.exe` as BO2 draws them (the centre
    /// sprite 53 units square at 230, the window 512, the diamond 50 / 512), and the display
    /// converts with its atlas.
    #[test]
    #[ignore]
    fn the_installs_target_finder_display_converts() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        let exe = Exe::parse(std::fs::read(dir.join("t6mp.exe")).unwrap()).unwrap();
        let l = Layout::read(&exe).expect("the numbers");
        assert_eq!((l.idle.len(), l.spotted.len(), l.window), (6, 10, 512.0));
        assert_eq!(&l.idle[0][..4], &[230.0, 230.0, 53.0, 53.0]);
        assert!((l.diamond_size - 50.0 / 512.0).abs() < 1e-6);
        let zone = crate::paks::load_zone(&dir, crate::textures::RETICLE_ZONE).unwrap();
        let mut paks = Paks::open(&dir).unwrap();
        paks.learn(&zone);
        let mut files = BTreeMap::new();
        let d = display(&dir, &[&zone], &paks, &mut files).expect("the display converts");
        let idle = zhbo_native::tga::decode(&files[&d.idle]).unwrap();
        assert_eq!((idle.width, idle.height), (512, 512));
        assert!(idle.rgba.chunks(4).any(|p| p[..3].iter().any(|&c| c > 64)), "the sprites drew");
        assert!(files.contains_key(&d.atlas));
        if let Some(out) = std::env::var_os("ZHBO_DUMP_DIR") {
            for p in [&d.idle, &d.spotted, &d.atlas] {
                let name = p.rsplit('\\').next().unwrap();
                std::fs::write(std::path::Path::new(&out).join(name), &files[p]).unwrap();
            }
        }
    }
}
