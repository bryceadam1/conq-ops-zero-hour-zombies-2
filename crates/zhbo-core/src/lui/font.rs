//! BO2's UI fonts, converted from the player's own install (`zhbo_assets::fonts`): the glyph
//! metrics arrive as text at init (`zhbo_load_fonts`) and each glyph is a MappedImage over the
//! converted atlas, so text draws as [`Prim::Image`]s. Without the converted archive the text
//! falls back to the engine font ([`Prim::Text`]), like every other BO2 art fallback.
//!
//! `CoD.fonts` in `patch_mp/ui/t6/codbase.lua` names the families; in English each family's
//! `CoD.textSize` equals its font's pixel height (Default 25 = `smallFont`, Condensed 30 =
//! `normalFont`, Big 48, Morris 60), so LUI draws its text at the font's own size. A glyph's box is
//! `x0, y0, width, height` from the pen, with `y0` negative (above the pen line); the pen advances
//! by `dx`. All in font pixels, scaled by `height / pixel_height` to the requested size.

use crate::hud::Prim;
use std::collections::HashMap;
use std::sync::Mutex;

/// A `CoD.fonts` family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// `smallFont`.
    Default,
    /// `normalFont`.
    Condensed,
    /// `bigFont`.
    Big,
    /// `extraBigFont`.
    Morris,
    /// `extraSmallFont` (also `CoD.fonts.Tiny`).
    ExtraSmall,
    /// `italicFont`.
    Italic,
    /// `smallItalicFont`.
    SmallItalic,
}

impl Family {
    /// The font file's name in `fonts/<resolution>/`.
    pub const fn file(self) -> &'static str {
        match self {
            Family::Default => "smallFont",
            Family::Condensed => "normalFont",
            Family::Big => "bigFont",
            Family::Morris => "extraBigFont",
            Family::ExtraSmall => "extraSmallFont",
            Family::Italic => "italicFont",
            Family::SmallItalic => "smallItalicFont",
        }
    }

    pub const ALL: [Family; 7] =
        [Family::Default, Family::Condensed, Family::Big, Family::Morris, Family::ExtraSmall, Family::Italic, Family::SmallItalic];
}

/// LUI's horizontal text alignment (`LUI.Alignment.Left/Center/Right`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub x0: f32,
    pub y0: f32,
    pub dx: f32,
    pub width: f32,
    pub height: f32,
    /// The MappedImage drawing it.
    pub image: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Font {
    pub pixel_height: f32,
    pub glyphs: HashMap<char, Glyph>,
}

static FONTS: Mutex<Option<HashMap<String, Font>>> = Mutex::new(None);

/// Parse the converter's metrics file (`font <file> <pixel height>` lines, each followed by its
/// `g <code point> <x0> <y0> <dx> <width> <height> <image>` lines).
pub fn parse(text: &str) -> HashMap<String, Font> {
    let mut fonts: HashMap<String, Font> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.as_slice() {
            ["font", name, height] => {
                let pixel_height = height.parse().unwrap_or(0.0);
                fonts.insert(name.to_string(), Font { pixel_height, glyphs: HashMap::new() });
                current = Some(name.to_string());
            }
            ["g", code, x0, y0, dx, w, h, image] => {
                let (Some(font), Some(c)) = (current.as_ref().and_then(|n| fonts.get_mut(n)), code.parse().ok().and_then(char::from_u32)) else {
                    continue;
                };
                let num = |s: &str| s.parse::<f32>().unwrap_or(0.0);
                font.glyphs.insert(c, Glyph { x0: num(x0), y0: num(y0), dx: num(dx), width: num(w), height: num(h), image: image.to_string() });
            }
            _ => {}
        }
    }
    fonts.retain(|_, f| f.pixel_height > 0.0 && !f.glyphs.is_empty());
    fonts
}

/// Load the converted metrics (the bridge passes the archive's file; empty when there is none).
pub fn load(text: &str) {
    let fonts = parse(text);
    *FONTS.lock().unwrap_or_else(|e| e.into_inner()) = (!fonts.is_empty()).then_some(fonts);
}

/// Tests that load or read the font table take this lock (the `strings::lock` pattern).
#[cfg(test)]
pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

fn with_font<R>(family: Family, f: impl FnOnce(Option<&Font>) -> R) -> R {
    let guard = FONTS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.as_ref().and_then(|t| t.get(family.file())))
}

/// Whether the converted fonts are loaded (else text uses the engine font).
pub fn loaded() -> bool {
    FONTS.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

fn advance(font: &Font, text: &str, scale: f32) -> f32 {
    text.chars().map(|c| font.glyphs.get(&c).map_or(0.0, |g| g.dx)).sum::<f32>() * scale
}

/// The width `text` takes at `height` pixels. The engine-font estimate when no fonts are loaded.
pub fn width(family: Family, text: &str, height: f32) -> f32 {
    with_font(family, |font| match font {
        Some(font) => advance(font, text, height / font.pixel_height),
        None => text.chars().count() as f32 * height * 0.5,
    })
}

/// Draw `text` in a LUI text box: `x` is the box's left, centre or right edge per `align`, `top`
/// its top, `height` its height (the text size). The pen line is the box's bottom edge: a glyph's
/// `y0` is measured up from it.
pub fn text(prims: &mut Vec<Prim>, family: Family, text: &str, x: f32, top: f32, height: f32, align: Align, argb: u32) {
    with_font(family, |font| {
        let Some(font) = font else {
            let (x, a) = match align {
                Align::Left => (x, 0),
                Align::Center => (x, 1),
                Align::Right => (x, 2),
            };
            prims.push(Prim::Text { x, y: top, text: text.to_string(), argb, size: height.round() as i32, align: a });
            return;
        };
        let scale = height / font.pixel_height;
        let w = advance(font, text, scale);
        let mut pen = match align {
            Align::Left => x,
            Align::Center => x - w * 0.5,
            Align::Right => x - w,
        };
        let base = top + height;
        for c in text.chars() {
            let Some(g) = font.glyphs.get(&c) else { continue };
            if g.width > 0.0 && g.height > 0.0 {
                prims.push(Prim::Image {
                    name: g.image.clone(),
                    x: pen + g.x0 * scale,
                    y: base + g.y0 * scale,
                    w: g.width * scale,
                    h: g.height * scale,
                    argb,
                    fallback: Vec::new(),
                });
            }
            pen += g.dx * scale;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: &str = "; test\nfont bigFont 48\ng 65 1 -39 18 17 31 ZHBO_Glyph_bigFont_65\ng 32 0 0 10 0 0 ZHBO_Glyph_bigFont_32\n";

    #[test]
    fn parses_the_converters_metrics() {
        let f = parse(METRICS);
        let big = &f["bigFont"];
        assert_eq!(big.pixel_height, 48.0);
        assert_eq!(big.glyphs[&'A'], Glyph { x0: 1.0, y0: -39.0, dx: 18.0, width: 17.0, height: 31.0, image: "ZHBO_Glyph_bigFont_65".into() });
    }

    #[test]
    fn lays_out_glyphs_from_the_pen_line() {
        let _l = lock();
        load(METRICS);
        let mut prims = Vec::new();
        // At 24 px (half of 48) the advance halves: "A A" is 9 + 5 + 9 wide.
        text(&mut prims, Family::Big, "A A", 100.0, 10.0, 24.0, Align::Right, 0xFFFF_FFFF);
        assert_eq!(width(Family::Big, "A A", 24.0), 23.0);
        let images: Vec<_> = prims.iter().filter_map(|p| match p {
            Prim::Image { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
            _ => None,
        }).collect();
        // The space has no box; each A sits at its x0, y0 up from the bottom edge (10 + 24).
        assert_eq!(images, vec![(77.5, 14.5, 8.5, 15.5), (91.5, 14.5, 8.5, 15.5)]);
        load("");
        assert!(!loaded());
    }

    #[test]
    fn falls_back_to_the_engine_font() {
        let _l = lock();
        load("");
        let mut prims = Vec::new();
        text(&mut prims, Family::Default, "AMMO", 5.0, 6.0, 25.0, Align::Center, 0xFF00_0000);
        assert_eq!(prims, vec![Prim::Text { x: 5.0, y: 6.0, text: "AMMO".into(), argb: 0xFF00_0000, size: 25, align: 1 }]);
    }
}
