//! BO2's slanted numerals (the ammo counter's digits).

use super::*;

/// Width of one BO2 numeral at height `height` (the art is 32x64, glyphs overlap slightly).
pub(crate) fn digit_advance(height: f32) -> f32 {
    height * 0.40
}

/// Draw a number in BO2 numerals, right-aligned at `right`. Returns the left edge.
pub(crate) fn draw_number(prims: &mut Vec<Prim>, value: u32, right: f32, top: f32, height: f32, argb: u32) -> f32 {
    let text = value.to_string();
    let adv = digit_advance(height);
    let left = right - adv * text.len() as f32;
    let mut x = left;
    for ch in text.chars() {
        let d = ch.to_digit(10).unwrap_or(0) as usize;
        let cell_w = height * 0.5;
        // Fallback: the engine font, sized to roughly match.
        let fallback = vec![Prim::Text { x: x + adv * 0.5, y: top + height * 0.05, text: ch.to_string(), argb, size: (height * 0.75) as i32, align: 1 }];
        prims.push(Prim::Image { name: sprites::DIGITS[d].to_string(), x: x - (cell_w - adv) * 0.5, y: top, w: cell_w, h: height, argb, fallback });
        x += adv;
    }
    left
}
