//! BO2's menu frame and drawing primitives for the class screens, in LUI units.
//!
//! Every class screen is a `CoD.Menu` (`patch_mp/ui/t6/codmenu.lua`): an element `CoD.Menu.Width`
//! x `CoD.Menu.Height` (864 x 648, `codbase.lua`'s `SDSafeWidth`/`SDSafeHeight`) centred in the LUI
//! root, with `addLargePopupBackground`'s frame around it, `addTitle`'s Big heading at its top-left
//! and the two button-prompt bars along its bottom edge. Geometry here is in LUI units (the root is
//! 720 tall, `lui::Root`); [`Frame::px`] turns a box into screen pixels. Colours are `codbase.lua`'s
//! `CoD.*` tables.

use super::*;
use crate::lui::font::{self, Align, Family};
use crate::lui::{materials, Place, Rect, Root};

/// `CoD.Menu.Width` / `CoD.Menu.Height` (`SDSafeWidth` / `SDSafeHeight`).
pub const MENU_W: f32 = 864.0;
pub const MENU_H: f32 = 648.0;

/// `CoD.ButtonPrompt.Height` and its bar spacing (`codmenu.lua` `NewFromState`: `spacing = 10`).
pub const PROMPT_H: f32 = 25.0;
pub const PROMPT_SPACING: f32 = 10.0;
/// `CoD.ButtonPrompt.ButtonToTextSpacing`.
pub const PROMPT_KEY_GAP: f32 = 3.0;

/// `CoD.textSize` in English (`codbase.lua`'s last branch): each equals its font's pixel height.
pub const TEXT_EXTRA_SMALL: f32 = 20.0;
pub const TEXT_DEFAULT: f32 = 25.0;
pub const TEXT_CONDENSED: f32 = 30.0;
pub const TEXT_BIG: f32 = 48.0;

/// An `argb` colour from LUI's 0..1 rgb and alpha.
pub const fn argb(rgb: (f32, f32, f32), a: f32) -> u32 {
    (channel(a) << 24) | (channel(rgb.0) << 16) | (channel(rgb.1) << 8) | channel(rgb.2)
}

const fn channel(v: f32) -> u32 {
    let v = if v < 0.0 { 0.0 } else if v > 1.0 { 1.0 } else { v };
    (v * 255.0 + 0.5) as u32
}

/// `CoD.offWhite` (also `DefaultTextColor` and `ButtonTextColor` in MP).
pub const OFF_WHITE: (f32, f32, f32) = (1.0, 1.0, 1.0);
/// `CoD.BOIIOrange`: the focused button, the allocation bars.
pub const BOII_ORANGE: (f32, f32, f32) = (1.0, 0.4, 0.0);
/// `CoD.trueOrange`: the focused grid button's subtitle.
pub const TRUE_ORANGE: (f32, f32, f32) = (1.0, 0.5, 0.0);
/// `CoD.orange`: the allocation counter under budget.
pub const COD_ORANGE: (f32, f32, f32) = (0.96, 0.58, 0.11);
/// `CoD.yellowGlow`: a button prompt's key.
pub const YELLOW_GLOW: (f32, f32, f32) = (0.9, 0.69, 0.2);
pub const BLACK: (f32, f32, f32) = (0.0, 0.0, 0.0);

/// One class screen's LUI frame: the root and the centred menu element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub root: Root,
    /// The menu element, in root units.
    pub menu: Rect,
}

impl Frame {
    pub fn new(w: f32, h: f32) -> Frame {
        let root = Root::new(w, h);
        let menu = Place::new((false, false, -MENU_W / 2.0, MENU_W / 2.0), (false, false, -MENU_H / 2.0, MENU_H / 2.0)).resolve(root.rect);
        Frame { root, menu }
    }

    /// Screen pixels per LUI unit.
    pub fn s(&self) -> f32 {
        self.root.scale
    }

    /// A box (root units) in screen pixels.
    pub fn px(&self, r: Rect) -> Rect {
        self.root.to_screen(r)
    }

    /// An element placed in the menu, in root units.
    pub fn in_menu(&self, p: Place) -> Rect {
        p.resolve(self.menu)
    }

    /// A point in screen pixels, in root units.
    pub fn unit_point(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.s(), y / self.s())
    }
}

/// An empty-or-not test for a root-unit box against a screen point.
pub fn hit(f: &Frame, r: Rect, x: f32, y: f32) -> bool {
    let p = f.px(r);
    x >= p.x && x <= p.right() && y >= p.y && y <= p.bottom()
}

/// A solid box (root units).
pub fn fill(prims: &mut Vec<Prim>, f: &Frame, r: Rect, argb: u32) {
    let p = f.px(r);
    if p.w > 0.0 && p.h > 0.0 {
        prims.push(Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb });
    }
}

/// A BO2 material stretched over a box (`LUI.UIImage` + `setImage(RegisterMaterial(name))`),
/// tinted like `setRGB`/`setAlpha`. `fallback` draws when the converted archive lacks it.
pub fn image(prims: &mut Vec<Prim>, f: &Frame, material: &str, r: Rect, argb: u32, fallback: Vec<Prim>) {
    let p = f.px(r);
    if p.w > 0.0 && p.h > 0.0 {
        prims.push(Prim::Image { name: materials::image(material), x: p.x, y: p.y, w: p.w, h: p.h, argb, fallback });
    }
}

/// An image under a converter-chosen name (weapon art, perk icons) over a box.
pub fn sprite(prims: &mut Vec<Prim>, f: &Frame, name: &str, r: Rect, argb: u32, fallback: Vec<Prim>) {
    let p = f.px(r);
    if p.w > 0.0 && p.h > 0.0 {
        prims.push(Prim::Image { name: name.to_string(), x: p.x, y: p.y, w: p.w, h: p.h, argb, fallback });
    }
}

/// `LUI.UIText` in a box: the box height is the text size, the pen line its bottom edge (G9).
pub fn text(prims: &mut Vec<Prim>, f: &Frame, family: Family, s: &str, r: Rect, align: Align, argb: u32) {
    let p = f.px(r);
    let x = match align {
        Align::Left => p.x,
        Align::Center => p.x + p.w * 0.5,
        Align::Right => p.right(),
    };
    font::text(prims, family, s, x, p.y, p.h, align, argb);
}

/// The width a text takes at `height` units, in units.
pub fn text_w(family: Family, s: &str, height: f32) -> f32 {
    font::width(family, s, height)
}

/// Scale a group of prims' alpha (LUI's `alphaMultiplier`, the way an element animates in or out
/// without anything under it changing). Image fallbacks fade with their image.
pub fn fade(prims: &mut [Prim], a: f32) {
    let k = a.clamp(0.0, 1.0);
    for p in prims.iter_mut() {
        match p {
            Prim::Rect { argb, .. } | Prim::Line { argb, .. } | Prim::Text { argb, .. } | Prim::Image { argb, .. } | Prim::Quad { argb, .. } => {
                let alpha = ((*argb >> 24) as f32 * k) as u32;
                *argb = (alpha << 24) | (*argb & 0x00FF_FFFF);
            }
            Prim::ClipBegin { .. } | Prim::ClipEnd => {}
        }
        if let Prim::Image { fallback, .. } | Prim::Quad { fallback, .. } = p {
            fade(fallback, k);
        }
    }
}

/// `CoD.Border.new(thickness, r, g, b, alpha)`: four edge strips inside a box (the thickness is
/// at least 2, as the widget clamps it).
pub fn border(prims: &mut Vec<Prim>, f: &Frame, r: Rect, thickness: f32, argb: u32) {
    let t = thickness.max(2.0);
    fill(prims, f, Rect { x: r.x, y: r.y, w: r.w, h: t }, argb);
    fill(prims, f, Rect { x: r.x, y: r.bottom() - t, w: r.w, h: t }, argb);
    fill(prims, f, Rect { x: r.x, y: r.y + t, w: t, h: r.h - 2.0 * t }, argb);
    fill(prims, f, Rect { x: r.right() - t, y: r.y + t, w: t, h: r.h - 2.0 * t }, argb);
}

/// The screen behind a class menu: BO2 shows its menus over the blurred, darkened lobby scene; the
/// mod's shell and match have no such scene, so a near-black fill (the capture's average,
/// `a-chooseclass/001`) stands in for it.
pub const BACKDROP: u32 = 0xFF1B_1B1B;

/// `CoD.Menu.addLargePopupBackground` (MP): the dotted backing (`menu_mp_cac_backing`, alpha 0.7,
/// tiled 8 times across), and the two `menu_mp_backing_frame` rules (alpha 0.65) 16 units tall,
/// above and below a 605-unit band shifted 19 units up from the menu's centre. The bottom rule is
/// the top one mirrored (its LUI top is below its bottom); the converter writes that mirrored copy.
pub fn backdrop(prims: &mut Vec<Prim>, f: &Frame, w: f32, h: f32) {
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h, argb: BACKDROP });
    let band = 605.0;
    let lift = 19.0;
    let rule = 16.0;
    // The dots: 8 tiles of the 8 x 1024 strip across the 1280-unit backing, centred.
    let dots = f.in_menu(Place::new((false, false, -640.0, 640.0), (false, false, -512.0, 512.0)));
    let tile = dots.w / 8.0;
    for i in 0..8 {
        image(prims, f, "menu_mp_cac_backing", Rect { x: dots.x + i as f32 * tile, y: dots.y, w: tile, h: dots.h }, argb(OFF_WHITE, 0.7), Vec::new());
    }
    let top = f.in_menu(Place::new((false, false, -640.0, 640.0), (false, false, -band / 2.0 - lift - rule, -band / 2.0 - lift)));
    image(prims, f, "menu_mp_backing_frame", top, argb(OFF_WHITE, 0.65), rule_fallback(f, top));
    let bottom = f.in_menu(Place::new((false, false, -640.0, 640.0), (false, false, band / 2.0 - lift, band / 2.0 - lift + rule)));
    image(prims, f, FRAME_FLIPPED, bottom, argb(OFF_WHITE, 0.65), rule_fallback(f, bottom));
}

/// The converter's vertically mirrored `menu_mp_backing_frame` (`addLargePopupBackground`'s bottom
/// rule).
pub const FRAME_FLIPPED: &str = "menu_mp_backing_frame_flipv";

fn rule_fallback(f: &Frame, r: Rect) -> Vec<Prim> {
    let p = f.px(r);
    vec![Prim::Rect { x: p.x, y: p.y + p.h * 0.5, w: p.w, h: f.s().max(1.0), argb: argb(OFF_WHITE, 0.25) }]
}

/// `CoD.Menu.addTitle`: Big (48) off-white text at the menu's top-left, `Menu.Width` wide.
pub fn title(prims: &mut Vec<Prim>, f: &Frame, s: &str) {
    let r = f.in_menu(Place::new((true, false, 0.0, MENU_W), (true, false, 0.0, TEXT_BIG)));
    text(prims, f, Family::Big, s, r, Align::Left, argb(OFF_WHITE, 1.0));
}

/// A button prompt's box in the menu's bottom bars (root units), with its key and label.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptBox {
    pub key: String,
    pub label: String,
    pub rect: Rect,
}

/// `CoD.ButtonPrompt`'s width: the key, `ButtonToTextSpacing`, the label, both ExtraSmall.
pub fn prompt_width(key: &str, label: &str) -> f32 {
    let k = text_w(Family::ExtraSmall, key, TEXT_EXTRA_SMALL);
    let l = text_w(Family::ExtraSmall, label, TEXT_EXTRA_SMALL);
    if label.is_empty() {
        k
    } else {
        k + PROMPT_KEY_GAP + l
    }
}

/// The two prompt bars (`leftButtonPromptBar`/`rightButtonPromptBar`): `ButtonPrompt.Height` tall
/// at the menu's bottom, `spacing = 10`, the left one packed from the left edge, the right one from
/// the right edge (its first prompt rightmost, `addRightButtonPrompt` order).
pub fn prompt_boxes(f: &Frame, left: &[(&str, &str)], right: &[(&str, &str)]) -> Vec<PromptBox> {
    let bar = f.in_menu(Place::new((true, true, 0.0, 0.0), (false, true, -PROMPT_H, 0.0)));
    let mut out = Vec::new();
    let mut x = bar.x;
    for (key, label) in left {
        let w = prompt_width(key, label);
        out.push(PromptBox { key: key.to_string(), label: label.to_string(), rect: Rect { x, y: bar.y, w, h: bar.h } });
        x += w + PROMPT_SPACING;
    }
    let mut x = bar.right();
    for (key, label) in right {
        let w = prompt_width(key, label);
        x -= w;
        out.push(PromptBox { key: key.to_string(), label: label.to_string(), rect: Rect { x, y: bar.y, w, h: bar.h } });
        x -= PROMPT_SPACING;
    }
    out
}

/// Draw prompts: the key in `CoD.yellowGlow`, the label off-white, both ExtraSmall centred in the
/// 25-unit element.
pub fn draw_prompts(prims: &mut Vec<Prim>, f: &Frame, boxes: &[PromptBox]) {
    for b in boxes {
        let top = b.rect.y + (PROMPT_H - TEXT_EXTRA_SMALL) / 2.0;
        let kw = text_w(Family::ExtraSmall, &b.key, TEXT_EXTRA_SMALL);
        text(prims, f, Family::ExtraSmall, &b.key, Rect { x: b.rect.x, y: top, w: kw, h: TEXT_EXTRA_SMALL }, Align::Left, argb(YELLOW_GLOW, 1.0));
        if !b.label.is_empty() {
            let x = b.rect.x + kw + PROMPT_KEY_GAP;
            text(prims, f, Family::ExtraSmall, &b.label, Rect { x, y: top, w: b.rect.right() - x, h: TEXT_EXTRA_SMALL }, Align::Left, argb(OFF_WHITE, 1.0));
        }
    }
}

/// `CoD.HintText`: the `ui_arrow_right` arrow (Default height - 8 square, at x -5) and the hint in
/// Default (25) off-white at alpha 0.75, left-aligned and wrapped to the box (`hinttext.lua`).
pub fn hint(prims: &mut Vec<Prim>, f: &Frame, r: Rect, s: &str) {
    let a = TEXT_DEFAULT - 8.0;
    let arrow = Rect { x: r.x - 5.0, y: r.y + (TEXT_DEFAULT - a) / 2.0, w: a, h: a };
    let colour = argb(OFF_WHITE, 0.75);
    let p = f.px(arrow);
    let tri = vec![
        Prim::Line { x0: p.x + p.w * 0.3, y0: p.y + p.h * 0.2, x1: p.x + p.w * 0.7, y1: p.y + p.h * 0.5, width: (p.w * 0.18).max(1.0), argb: colour },
        Prim::Line { x0: p.x + p.w * 0.7, y0: p.y + p.h * 0.5, x1: p.x + p.w * 0.3, y1: p.y + p.h * 0.8, width: (p.w * 0.18).max(1.0), argb: colour },
    ];
    image(prims, f, "ui_arrow_right", arrow, colour, tri);
    let x = r.x - 5.0 + a;
    for (i, line) in wrap(Family::Default, s, TEXT_DEFAULT, r.right() - x).iter().enumerate() {
        text(prims, f, Family::Default, line, Rect { x, y: r.y + i as f32 * TEXT_DEFAULT, w: r.right() - x, h: TEXT_DEFAULT }, Align::Left, colour);
    }
}

/// Word-wrap `s` to `width` units (G9: an aligned UIText wraps on whitespace to its box width).
pub fn wrap(family: Family, s: &str, height: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && text_w(family, &candidate, height) > width {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_is_centred_sd_safe() {
        // 16:10 like the captures: the root is 1152 wide, so the menu's left edge sits at 144 and
        // its top at 36 (`a-chooseclass/001`'s title starts 144 units in).
        let f = Frame::new(3024.0, 1890.0);
        assert_eq!(f.menu, Rect { x: 144.0, y: 36.0, w: 864.0, h: 648.0 });
        assert!((f.s() - 2.625).abs() < 1e-4);
    }

    #[test]
    fn colours_are_lui_rgb() {
        assert_eq!(argb(BOII_ORANGE, 1.0), 0xFFFF_6600);
        assert_eq!(argb(OFF_WHITE, 0.5), 0x80FF_FFFF);
    }

    #[test]
    fn prompt_bars_pack_from_their_edges() {
        let _l = crate::lui::font::lock();
        crate::lui::font::load("");
        let f = Frame::new(1280.0, 720.0);
        let b = prompt_boxes(&f, &[("ESC", "Back")], &[("O", "Class Options"), ("S", "Class Set Options")]);
        assert_eq!(b[0].rect.x, f.menu.x, "Back starts at the menu's left edge");
        assert_eq!(b[0].rect.bottom(), f.menu.bottom());
        assert_eq!(b[1].rect.right(), f.menu.right(), "the first right prompt is rightmost");
        assert!(b[2].rect.right() <= b[1].rect.x - PROMPT_SPACING + 1e-3);
    }
}
