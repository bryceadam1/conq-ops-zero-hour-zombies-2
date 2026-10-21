//! Black Ops II (T6) bitmap fonts, read from a fully loaded zone such as
//! `zone/english/en_code_post_gfx_mp.ff` (`fonts/720/normalFont`, `bigFont`, ...). A font is a
//! glyph table over one atlas material: each glyph's atlas rectangle (`s0,t0`-`s1,t1`), its box
//! relative to the pen (`x0`, `y0`, `pixelWidth`, `pixelHeight`) and its advance (`dx`), all at the
//! font's `pixelHeight`. Kerning pairs adjust the advance between two letters.

use crate::t6zone::layouts::{Font_s as F, Glyph as G, KerningPairs as K};
use crate::t6zone::Zone;
use crate::xmodel::{read_material, Material};
use crate::FormatError;

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub letter: u16,
    pub x0: i8,
    pub y0: i8,
    pub dx: u8,
    pub width: u8,
    pub height: u8,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
}

#[derive(Clone, Debug)]
pub struct Font {
    pub name: String,
    pub pixel_height: i32,
    pub glyphs: Vec<Glyph>,
    /// `(first, second, amount)`.
    pub kerning: Vec<(u16, u16, i32)>,
    pub material: Option<Material>,
}

/// Every font in a loaded zone.
pub fn all(z: &Zone) -> Result<Vec<Font>, FormatError> {
    let Some(kind) = crate::fastfile::asset_type_id("font") else { return Ok(Vec::new()) };
    z.assets.iter().filter(|a| a.kind == kind).map(|a| read(z, a.addr)).collect()
}

/// The `Font_s` at `f`.
pub fn read(z: &Zone, f: u32) -> Result<Font, FormatError> {
    let count = z.try_u32(f + F::glyphCount)?;
    let table = z.try_u32(f + F::glyphs)?;
    let mut glyphs = Vec::with_capacity(count.min(4096) as usize);
    for i in 0..if table == 0 { 0 } else { count } {
        let g = table + G::SIZE * i;
        glyphs.push(Glyph {
            letter: z.try_u16(g + G::letter)?,
            x0: z.try_u8(g + G::x0)? as i8,
            y0: z.try_u8(g + G::y0)? as i8,
            dx: z.try_u8(g + G::dx)?,
            width: z.try_u8(g + G::pixelWidth)?,
            height: z.try_u8(g + G::pixelHeight)?,
            s0: z.try_f32(g + G::s0)?,
            t0: z.try_f32(g + G::t0)?,
            s1: z.try_f32(g + G::s1)?,
            t1: z.try_f32(g + G::t1)?,
        });
    }
    let pairs = z.try_u32(f + F::kerningPairsCount)?;
    let ktable = z.try_u32(f + F::kerningPairs)?;
    let mut kerning = Vec::new();
    for i in 0..if ktable == 0 { 0 } else { pairs } {
        let k = ktable + K::SIZE * i;
        kerning.push((z.try_u16(k + K::wFirst)?, z.try_u16(k + K::wSecond)?, z.try_u32(k + K::iKernAmount)? as i32));
    }
    let material = z.try_u32(f + F::material)?;
    Ok(Font {
        name: z.try_str_ptr(f + F::fontName)?,
        pixel_height: z.try_u32(f + F::pixelHeight)? as i32,
        glyphs,
        kerning,
        material: (material != 0).then(|| read_material(z, material)),
    })
}
