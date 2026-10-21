//! BO2's box shapes: outlined and slanted rectangles.

use super::*;

pub(crate) fn outline_rect(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, h: f32, t: f32, argb: u32) {
    prims.push(Prim::Rect { x: x, y: y, w: w, h: t, argb: argb });
    prims.push(Prim::Rect { x: x, y: y + h - t, w: w, h: t, argb: argb });
    prims.push(Prim::Rect { x: x, y: y, w: t, h: h, argb: argb });
    prims.push(Prim::Rect { x: x + w - t, y: y, w: t, h: h, argb: argb });
}

/// BO2's slanted box: a parallelogram leaning right, `x` its bottom-left corner's left edge.
pub(crate) fn slant_box(x: f32, y: f32, w: f32, h: f32, fill: u32, edge: Option<u32>) -> Vec<Prim> {
    let mut prims = Vec::new();
    let lean = h * SLANT;
    let rows = h.ceil().max(1.0) as i32;
    for i in 0..rows {
        let f = i as f32 / rows as f32;
        prims.push(Prim::Rect { x: x + lean * (1.0 - f), y: y + i as f32, w, h: 1.0, argb: fill });
    }
    if let Some(c) = edge {
        let (tl, tr, bl, br) = ((x + lean, y), (x + lean + w, y), (x, y + h), (x + w, y + h));
        for ((ax, ay), (bx, by)) in [(tl, tr), (bl, br), (tl, bl), (tr, br)] {
            prims.push(Prim::Line { x0: ax, y0: ay, x1: bx, y1: by, width: 1.5, argb: c });
        }
    }
    prims
}
