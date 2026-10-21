//! Damage feedback and the death banner.
//!
//! * The low-health overlay (Ghidra G7, `t6mp` 0x004dabe0/0x004daf40): `overlay_low_health`
//!   full screen at the pulse state machine's alpha ([`super::state::LowHealth`]).
//! * The damage-direction indicators (G7, 0x004ade10): `hit_direction` on a 128 x 64 band 64..128
//!   units out from the centre (480-line space), turned to `viewYaw − hitYaw`, opaque for the first
//!   half of its 2 s and fading after. The core does not learn where damage came from yet, so no
//!   slot is filled until the engine reports it (`HudLui::damage_from`).

use super::state::HudLui;
use super::*;
use crate::lui::materials;

pub const MATERIALS: [&str; 2] = ["overlay_low_health", "hit_direction"];

/// The low-health overlay at `alpha`.
pub(super) fn draw_low_health(prims: &mut Vec<Prim>, w: f32, h: f32, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
    // Without the art: a red rim like BO2's.
    let band = (h * 0.08).max(16.0);
    let col = ((a * 3 / 5) << 24) | 0x0090_0000;
    let fallback = vec![
        Prim::Rect { x: 0.0, y: 0.0, w, h: band, argb: col },
        Prim::Rect { x: 0.0, y: h - band, w, h: band, argb: col },
        Prim::Rect { x: 0.0, y: band, w: band, h: h - 2.0 * band, argb: col },
        Prim::Rect { x: w - band, y: band, w: band, h: h - 2.0 * band, argb: col },
    ];
    prims.push(Prim::Image { name: materials::image("overlay_low_health"), x: 0.0, y: 0.0, w, h, argb: (a << 24) | 0x00FF_FFFF, fallback });
}

/// The corners of one indicator: the 128 x 64 band above the centre, turned by `angle_deg`
/// (clockwise on screen), in pixels.
pub fn indicator_corners(w: f32, h: f32, angle_deg: f32) -> [(f32, f32); 4] {
    let u = h / 480.0;
    let (cx, cy) = (w * 0.5, h * 0.5);
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let turn = |x: f32, y: f32| (cx + (x * cos - y * sin) * u, cy + (x * sin + y * cos) * u);
    [turn(-64.0, -128.0), turn(64.0, -128.0), turn(64.0, -64.0), turn(-64.0, -64.0)]
}

pub(super) fn draw_indicators(prims: &mut Vec<Prim>, w: f32, h: f32, view_yaw_deg: f32, state: &HudLui) {
    for (yaw, alpha) in state.live_indicators() {
        let corners = indicator_corners(w, h, view_yaw_deg - yaw);
        let a = (alpha * 255.0) as u32;
        let col = (0xFF << 24) | (a << 16) | (a << 8) | a;
        let edge = (a << 24) | 0x00E0_2020;
        let fallback = (0..4).map(|i| {
            let (p, q) = (corners[i], corners[(i + 1) % 4]);
            Prim::Line { x0: p.0, y0: p.1, x1: q.0, y1: q.1, width: 2.0, argb: edge }
        }).collect();
        prims.push(Prim::Quad { name: materials::image("hit_direction"), corners, argb: col, fallback });
    }
}

pub(super) fn draw_death_banner(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, t: f32) {
    let a = (t.min(1.0) * 230.0) as u32;
    prims.push(Prim::Rect { x: 0.0, y: h * 0.4, w, h: 64.0 * s, argb: (a / 2) << 24 });
    prims.push(Prim::Text { x: w * 0.5, y: h * 0.4 + 10.0 * s, text: "GENERAL DOWN".to_string(), argb: (a << 24) | (RED & 0x00FF_FFFF), size: (24.0 * s) as i32, align: 1 });
    prims.push(Prim::Text { x: w * 0.5, y: h * 0.4 + 42.0 * s, text: "Build a new General at your Command Center".to_string(), argb: (a << 24) | 0x00FF_FFFF, size: (12.0 * s) as i32, align: 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_indicator_from_ahead_sits_above_the_centre() {
        // 640 x 480: one unit a pixel; straight ahead is the band 64..128 above the centre.
        let c = indicator_corners(640.0, 480.0, 0.0);
        assert_eq!(c[0], (256.0, 112.0));
        assert_eq!(c[2], (384.0, 176.0));
        // From the right (90° clockwise): the band moves to the right of the centre.
        let r = indicator_corners(640.0, 480.0, 90.0);
        assert!((r[0].0 - 448.0).abs() < 1e-3 && (r[0].1 - 176.0).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn the_overlay_is_full_screen() {
        let mut prims = Vec::new();
        draw_low_health(&mut prims, 1280.0, 720.0, 0.5);
        assert!(matches!(&prims[0], Prim::Image { name, w, h, argb, .. } if name == "ZHBO_Mat_overlay_low_health" && *w == 1280.0 && *h == 720.0 && *argb == 0x7FFF_FFFF));
    }
}
