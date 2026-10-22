//! BO2's box shapes: outlined rectangles.

use super::*;

pub(crate) fn outline_rect(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, h: f32, t: f32, argb: u32) {
    prims.push(Prim::Rect { x, y, w, h: t, argb });
    prims.push(Prim::Rect { x, y: y + h - t, w, h: t, argb });
    prims.push(Prim::Rect { x, y, w: t, h, argb });
    prims.push(Prim::Rect { x: x + w - t, y, w: t, h, argb });
}
