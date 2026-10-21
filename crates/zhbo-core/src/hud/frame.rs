//! Drawing LUI elements into prims: boxes are resolved in LUI units against the 720-unit root
//! ([`crate::lui`]) and converted to pixels only when drawn, so every element keeps the Lua's
//! numbers. Colours follow LUI's `setRGB`/`setAlpha` (0..1 floats).

use super::Prim;
use crate::lui::font::{self, Align, Family};
use crate::lui::{materials, Place, Rect, Root};

/// `CoD.offWhite`, `CoD.yellowGlow` and the HUD base colour (`patch_mp/ui/t6/codbase.lua`).
pub const OFF_WHITE: (f32, f32, f32) = (1.0, 1.0, 1.0);
pub const YELLOW_GLOW: (f32, f32, f32) = (0.9, 0.69, 0.2);
pub const HUD_BASE: (f32, f32, f32) = (1.0, 1.0, 1.0);
/// `CoD.HUDAlphaFull` / `CoD.HUDAlphaEmpty`.
pub const HUD_ALPHA_FULL: f32 = 1.0;
pub const HUD_ALPHA_EMPTY: f32 = 0.6;

/// An LUI colour as the engine's ARGB.
pub fn argb(rgb: (f32, f32, f32), alpha: f32) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (c(alpha) << 24) | (c(rgb.0) << 16) | (c(rgb.1) << 8) | c(rgb.2)
}

/// One screen's LUI context: the root and, for convenience, its full box in LUI units.
#[derive(Clone, Copy, Debug)]
pub struct Lui {
    pub root: Root,
}

impl Lui {
    pub fn new(w: f32, h: f32) -> Lui {
        Lui { root: Root::new(w, h) }
    }

    /// The whole screen (the HUD's safe area: on PC `safeAreaTweakable_*` is 1, Ghidra G9).
    pub fn screen(&self) -> Rect {
        self.root.rect
    }

    /// `place` inside `parent` (both in LUI units).
    pub fn at(&self, parent: Rect, place: Place) -> Rect {
        place.resolve(parent)
    }

    pub fn px(&self, r: Rect) -> Rect {
        self.root.to_screen(r)
    }

    /// A `LUI.UIImage` showing a BO2 material (converted as `materials::image`).
    pub fn image(&self, prims: &mut Vec<Prim>, r: Rect, material: &str, rgb: (f32, f32, f32), alpha: f32, fallback: Vec<Prim>) {
        if alpha <= 0.0 {
            return;
        }
        let p = self.px(r);
        prims.push(Prim::Image { name: materials::image(material), x: p.x, y: p.y, w: p.w, h: p.h, argb: argb(rgb, alpha), fallback });
    }

    /// A `LUI.UIText` in box `r`: the text is the box's height and its pen line is the box's bottom
    /// (G9). `align` is the element's `setAlignment` (Left/Center/Right on the box), or `None` for
    /// LUI's default: centred when both or neither horizontal anchors are set, else on the
    /// anchored edge (G9's Alignment.None rule) — `anchor` gives (leftAnchor, rightAnchor).
    #[allow(clippy::too_many_arguments)]
    pub fn text(&self, prims: &mut Vec<Prim>, r: Rect, family: Family, text: &str, align: Option<Align>, anchor: (bool, bool), rgb: (f32, f32, f32), alpha: f32) {
        if alpha <= 0.0 || text.is_empty() {
            return;
        }
        let p = self.px(r);
        let align = align.unwrap_or(match anchor {
            (true, false) => Align::Left,
            (false, true) => Align::Right,
            _ => Align::Center,
        });
        let x = match align {
            Align::Left => p.x,
            Align::Center => p.x + p.w * 0.5,
            Align::Right => p.right(),
        };
        font::text(prims, family, text, x, p.y, p.h, align, argb(rgb, alpha));
    }

    /// The width a text takes at `height` LUI units, in LUI units (`GetTextDimensions`).
    pub fn text_width(&self, family: Family, text: &str, height: f32) -> f32 {
        font::width(family, text, height * self.root.scale) / self.root.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_and_boxes() {
        assert_eq!(argb((1.0, 0.0, 0.0), 0.5), 0x80FF_0000);
        let l = Lui::new(1920.0, 1080.0);
        let r = l.at(l.screen(), Place::new((false, true, -28.0, 0.0), (false, true, -7.0, 0.0)));
        assert_eq!(r, Rect { x: 1252.0, y: 713.0, w: 28.0, h: 7.0 });
        assert_eq!(l.px(r), Rect { x: 1878.0, y: 1069.5, w: 42.0, h: 10.5 });
    }
}
