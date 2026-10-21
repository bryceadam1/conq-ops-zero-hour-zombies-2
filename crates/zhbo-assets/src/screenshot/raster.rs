//! Software rasterizer for the class screens' retained prims.
//!
//! It walks a `Vec<hud::Prim>` the way `hud::render` walks it in game — same order, same
//! `ClipBegin`/`ClipEnd` scoping, and the same "a missing image draws its fallback" rule — but
//! paints into an RGBA byte buffer instead of the engine's draw callbacks. Colours are straight
//! alpha over, images are tinted by their prim's ARGB, and text goes through the embedded 5x7
//! font ([`super::font`]). This is the only place the harness interprets prims, so what the PNG
//! shows is what the game's prim list says.

use super::atlas::Atlas;
use super::font;
use std::collections::BTreeSet;
use zhbo_core::hud::Prim;

/// An axis-aligned screen rectangle used for clipping (fractional, like the prims).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl ClipRect {
    /// The overlap of two clips, or `None` when they do not touch.
    fn intersect(self, other: ClipRect) -> Option<ClipRect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        (x1 > x0 && y1 > y0).then_some(ClipRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }
}

/// A top-down RGBA canvas. Alpha is always 255 in the backing store (the screen starts opaque and
/// every blend composites over it), which is what a PNG of a full-screen UI should carry.
pub(crate) struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    /// A canvas filled with one opaque ARGB colour.
    pub(crate) fn new(w: u32, h: u32, argb: u32) -> Canvas {
        let [_, r, g, b] = argb.to_be_bytes();
        let mut px = vec![0u8; (w * h * 4) as usize];
        for p in px.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[r, g, b, 255]);
        }
        Canvas { w, h, px }
    }

    pub(crate) fn width(&self) -> u32 {
        self.w
    }

    pub(crate) fn height(&self) -> u32 {
        self.h
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.px
    }

    /// Blend one pixel with a straight-alpha source colour.
    fn blend(&mut self, x: i64, y: i64, [r, g, b, a]: [u8; 4]) {
        if a == 0 || x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        if a == 255 {
            self.px[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            return;
        }
        let a = a as u32;
        for (k, src) in [r, g, b].into_iter().enumerate() {
            let dst = self.px[i + k] as u32;
            self.px[i + k] = ((src as u32 * a + dst * (255 - a)) / 255) as u8;
        }
        self.px[i + 3] = 255;
    }

    /// A solid (alpha-blended) rectangle, clipped to `clip`.
    pub(crate) fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, argb: u32, clip: Option<ClipRect>) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let [a, r, g, b] = argb.to_be_bytes();
        let (mut x0, mut y0, mut x1, mut y1) = (x, y, x + w, y + h);
        if let Some(c) = clip {
            x0 = x0.max(c.x);
            y0 = y0.max(c.y);
            x1 = x1.min(c.x + c.w);
            y1 = y1.min(c.y + c.h);
        }
        let (x0, y0) = (x0.max(0.0), y0.max(0.0));
        let (x1, y1) = (x1.min(self.w as f32), y1.min(self.h as f32));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for py in y0 as i64..y1.ceil() as i64 {
            for px in x0 as i64..x1.ceil() as i64 {
                self.blend(px, py, [r, g, b, a]);
            }
        }
    }

    fn clipped(&self, clip: Option<ClipRect>) -> ClipRect {
        let all = ClipRect { x: 0.0, y: 0.0, w: self.w as f32, h: self.h as f32 };
        clip.map_or(all, |c| all.intersect(c).unwrap_or(ClipRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }))
    }

    /// A line of the given width (the HUD crosshair's shape; the class screens use none).
    #[allow(clippy::too_many_arguments)] // the prim's own fields, kept in the rasterizer's call shape
    pub(crate) fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, argb: u32, clip: Option<ClipRect>) {
        let [a, r, g, b] = argb.to_be_bytes();
        let clip = self.clipped(clip);
        let half = (width * 0.5).max(0.5);
        let min_x = (x0.min(x1) - half).max(clip.x);
        let max_x = (x0.max(x1) + half).min(clip.x + clip.w);
        let min_y = (y0.min(y1) - half).max(clip.y);
        let max_y = (y0.max(y1) + half).min(clip.y + clip.h);
        if max_x <= min_x || max_y <= min_y {
            return;
        }
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx * dx + dy * dy;
        for py in min_y as i64..max_y.ceil() as i64 {
            for px in min_x as i64..max_x.ceil() as i64 {
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                let t = if len2 > 0.0 { (((cx - x0) * dx + (cy - y0) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let (nx, ny) = (x0 + dx * t, y0 + dy * t);
                if (cx - nx).hypot(cy - ny) <= half {
                    self.blend(px, py, [r, g, b, a]);
                }
            }
        }
    }

    /// Blit an atlas rect into a destination rect, scaled nearest-neighbour and tinted by `argb`
    /// (the `hud::Prim::Image` colour: alpha and RGB both multiply, like the engine's tint).
    #[allow(clippy::too_many_arguments)] // source page, source rect, destination rect, tint, clip
    fn blit(&mut self, src: &[u8], sw: u32, sh: u32, rect: [u32; 4], dx: f32, dy: f32, dw: f32, dh: f32, argb: u32, clip: Option<ClipRect>) {
        if dw <= 0.0 || dh <= 0.0 || sw == 0 || sh == 0 {
            return;
        }
        let [ta, tr, tg, tb] = argb.to_be_bytes();
        let clip = self.clipped(clip);
        let x0 = dx.max(clip.x);
        let y0 = dy.max(clip.y);
        let x1 = (dx + dw).min(clip.x + clip.w);
        let y1 = (dy + dh).min(clip.y + clip.h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (left, top, right, bottom) = (rect[0], rect[1], rect[2], rect[3]);
        let (rw, rh) = (right.saturating_sub(left) + 1, bottom.saturating_sub(top) + 1);
        for py in y0.floor() as i64..y1.ceil() as i64 {
            for px in x0.floor() as i64..x1.ceil() as i64 {
                let u = ((px as f32 + 0.5 - dx) / dw * rw as f32).clamp(0.0, rw as f32 - 1.0) as u32;
                let v = ((py as f32 + 0.5 - dy) / dh * rh as f32).clamp(0.0, rh as f32 - 1.0) as u32;
                let sx = (left + u).min(sw - 1);
                let sy = (top + v).min(sh - 1);
                let i = ((sy * sw + sx) * 4) as usize;
                let [r, g, b, a] = [src[i], src[i + 1], src[i + 2], src[i + 3]];
                let tint = |c: u8, t: u8| ((c as u32 * t as u32) / 255) as u8;
                let out = [tint(r, tr), tint(g, tg), tint(b, tb), tint(a, ta)];
                self.blend(px, py, out);
            }
        }
    }
}

/// Render the prims on a 1280x720-class canvas. `missing` collects the sprite names the atlas did
/// not have (they draw their fallbacks, exactly like `hud::render` in game).
pub(crate) fn render(prims: &[Prim], atlas: &Atlas, w: u32, h: u32, missing: &mut BTreeSet<String>) -> Canvas {
    let mut canvas = Canvas::new(w, h, 0xFF00_0000);
    walk(prims, &mut canvas, atlas, None, missing);
    canvas
}

fn walk(prims: &[Prim], canvas: &mut Canvas, atlas: &Atlas, mut clip: Option<ClipRect>, missing: &mut BTreeSet<String>) {
    for p in prims {
        match p {
            Prim::Rect { x, y, w, h, argb } => canvas.fill_rect(*x, *y, *w, *h, *argb, clip),
            Prim::Line { x0, y0, x1, y1, width, argb } => canvas.line(*x0, *y0, *x1, *y1, *width, *argb, clip),
            Prim::Text { x, y, text, argb, size, align } => font::draw(canvas, *x, *y, text, *argb, *size, *align, clip),
            Prim::Image { name, x, y, w, h, argb, fallback } => match atlas.find(name) {
                Some((page, rect)) => canvas.blit(&page.pixels, page.width, page.height, rect, *x, *y, *w, *h, *argb, clip),
                None => {
                    missing.insert(name.clone());
                    // The same rule as `hud::render`: an absent image draws its fallback prims.
                    walk(fallback, canvas, atlas, clip, missing);
                }
            },
            Prim::ClipBegin { x, y, w, h } => {
                let rect = ClipRect { x: *x, y: *y, w: *w, h: *h };
                clip = Some(match clip {
                    Some(c) => c.intersect(rect).unwrap_or(ClipRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
                    None => rect,
                });
            }
            Prim::ClipEnd => clip = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screenshot::atlas::Atlas;

    fn px(c: &Canvas, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * c.width() + x) * 4) as usize;
        [c.pixels()[i], c.pixels()[i + 1], c.pixels()[i + 2], c.pixels()[i + 3]]
    }

    #[test]
    fn rects_alpha_blend_and_clip() {
        let mut c = Canvas::new(8, 8, 0xFF00_0000);
        c.fill_rect(0.0, 0.0, 8.0, 8.0, 0x80FF_FFFF, None);
        // 0x80/255 of white over black, rounded: each channel lands on 128.
        assert_eq!(px(&c, 0, 0), [128, 128, 128, 255], "half-white over black");
        let mut c = Canvas::new(8, 8, 0xFF00_0000);
        c.fill_rect(0.0, 0.0, 8.0, 8.0, 0xFFFF_0000, Some(ClipRect { x: 2.0, y: 2.0, w: 4.0, h: 4.0 }));
        assert_eq!(px(&c, 3, 3), [255, 0, 0, 255]);
        assert_eq!(px(&c, 0, 0), [0, 0, 0, 255], "outside the clip");
        assert_eq!(px(&c, 7, 7), [0, 0, 0, 255], "outside the clip");
    }

    #[test]
    fn a_missing_image_draws_its_fallback_and_is_reported() {
        let mut missing = BTreeSet::new();
        let prims = [Prim::Image {
            name: "NOT_IN_THE_ATLAS".into(),
            x: 1.0,
            y: 1.0,
            w: 6.0,
            h: 6.0,
            argb: 0xFFFF_FFFF,
            fallback: vec![Prim::Rect { x: 1.0, y: 1.0, w: 6.0, h: 6.0, argb: 0xFFFF_FFFF }],
        }];
        let c = render(&prims, &Atlas::empty(), 8, 8, &mut missing);
        assert_eq!(px(&c, 3, 3), [255, 255, 255, 255]);
        assert!(missing.contains("NOT_IN_THE_ATLAS"));
    }

    #[test]
    fn clip_begin_scopes_until_clip_end() {
        let prims = [
            Prim::ClipBegin { x: 2.0, y: 2.0, w: 2.0, h: 2.0 },
            Prim::Rect { x: 0.0, y: 0.0, w: 8.0, h: 8.0, argb: 0xFF00_FF00 },
            Prim::ClipEnd,
            Prim::Rect { x: 6.0, y: 6.0, w: 1.0, h: 1.0, argb: 0xFF00_00FF },
        ];
        let c = render(&prims, &Atlas::empty(), 8, 8, &mut BTreeSet::new());
        assert_eq!(px(&c, 3, 3), [0, 255, 0, 255]);
        assert_eq!(px(&c, 1, 1), [0, 0, 0, 255], "clipped out");
        assert_eq!(px(&c, 6, 6), [0, 0, 255, 255], "after ClipEnd");
    }
}
