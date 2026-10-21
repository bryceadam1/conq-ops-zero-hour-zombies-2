//! The screenshots' embedded 5x7 bitmap font.
//!
//! The game draws text through Zero Hour's font, which a headless harness cannot reach. A tiny
//! embedded ASCII font keeps the shots legible without a new dependency or a shipped font file:
//! glyphs are drawn `size` pixels tall (nearest-neighbour from the 5x7 grid) and the prim's
//! alignment (0 left, 1 centre, 2 right) is honoured exactly like `hud::Prim::Text`.
//!
//! The font is uppercase; lowercase input folds to uppercase (the class screens' labels are
//! uppercase anyway, and player class names stay readable).

use super::raster::{Canvas, ClipRect};

/// The glyph grid: five columns by seven rows.
const GLYPH_W: f32 = 5.0;
const GLYPH_H: f32 = 7.0;

/// One glyph's rows, top to bottom, five bits each (bit 4 is the leftmost column).
fn glyph(c: char) -> Option<[u8; 7]> {
    let rows = match c.to_ascii_uppercase() {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110, 0b00100],
        ':' => [0b00000, 0b00110, 0b00110, 0b00000, 0b00110, 0b00110, 0b00000],
        ';' => [0b00000, 0b00110, 0b00110, 0b00000, 0b00110, 0b00110, 0b00100],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
        '\'' => [0b00100, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        '"' => [0b01010, 0b01010, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '*' => [0b00000, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0b00000],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        '\\' => [0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '[' => [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110],
        ']' => [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110],
        '<' => [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010],
        '>' => [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000],
        '=' => [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000],
        '%' => [0b11000, 0b11001, 0b00010, 0b00100, 0b01000, 0b10011, 0b00011],
        '&' => [0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101],
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        '@' => [0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01111],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
        '|' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        '^' => [0b00100, 0b01010, 0b10001, 0b00000, 0b00000, 0b00000, 0b00000],
        '~' => [0b00000, 0b00000, 0b01001, 0b10110, 0b00000, 0b00000, 0b00000],
        '$' => [0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100],
        '`' => [0b01000, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        ' ' => [0; 7],
        _ => return None,
    };
    Some(rows)
}

/// The pixel distance from one glyph's left edge to the next.
fn advance(size: f32) -> f32 {
    // The 5x7 grid plus a one-column gap, scaled to `size`.
    size * (GLYPH_W + 1.0) / GLYPH_H
}

/// The width of `text` at `size`, for the prim's alignment.
pub(crate) fn text_width(text: &str, size: i32) -> f32 {
    let n = text.chars().count();
    if n == 0 {
        return 0.0;
    }
    let size = size.max(1) as f32;
    advance(size) * n as f32 - size / GLYPH_H
}

/// Draw `text` with its top edge at `y`, aligned at `x` (0 left, 1 centre, 2 right).
#[allow(clippy::too_many_arguments)] // mirrors `hud::Prim::Text`'s fields plus the canvas and clip
pub(crate) fn draw(canvas: &mut Canvas, x: f32, y: f32, text: &str, argb: u32, size: i32, align: i32, clip: Option<ClipRect>) {
    let size = size.max(1) as f32;
    let adv = advance(size);
    let width = text_width(text, size as i32);
    let start = match align {
        1 => x - width * 0.5,
        2 => x - width,
        _ => x,
    };
    let scale = size / GLYPH_H;
    let mut gx = start;
    for c in text.chars() {
        if let Some(rows) = glyph(c) {
            // Pixel boundaries for the 5x7 cells, so adjacent blocks meet without seams.
            let cols: Vec<f32> = (0..=5).map(|i| (gx + i as f32 * scale).round()).collect();
            let lines: Vec<f32> = (0..=7).map(|i| (y + i as f32 * scale).round()).collect();
            for (r, bits) in rows.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        let x0 = cols[col];
                        let y0 = lines[r];
                        canvas.fill_rect(x0, y0, cols[col + 1] - x0, lines[r + 1] - y0, argb, clip);
                    }
                }
            }
        }
        gx += adv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_width_tracks_the_glyph_advance_and_alignment_stays_inside() {
        // A 14-character label at 14 px is about 12 px per glyph.
        let w = text_width("CREATE A CLASS", 14);
        assert!((w - 165.0).abs() < 2.0, "{w}");
        assert_eq!(text_width("", 12), 0.0);
        // Every needed character has a glyph (the screens' labels and prompts).
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,:;!?'\"-+/*()<>[]%&#@_=|^~$`\\".chars() {
            assert!(glyph(c).is_some(), "missing glyph {c:?}");
        }
    }

    #[test]
    fn drawing_marks_pixels_and_honours_clipping() {
        let mut c = Canvas::new(40, 20, 0xFF00_0000);
        draw(&mut c, 0.0, 0.0, "AB", 0xFFFF_FFFF, 14, 0, None);
        assert!(c.pixels().as_chunks::<4>().0.iter().any(|p| p[0] > 200), "the glyphs drew");
        let before: Vec<u8> = c.pixels().to_vec();
        draw(&mut c, 0.0, 0.0, "AB", 0xFFFF_0000, 14, 0, Some(ClipRect { x: 100.0, y: 0.0, w: 10.0, h: 10.0 }));
        assert_eq!(c.pixels(), &before[..], "a clip away from the glyphs draws nothing");
    }
}
