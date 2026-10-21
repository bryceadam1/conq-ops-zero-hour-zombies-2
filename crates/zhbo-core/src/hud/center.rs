//! The centre of the screen: crosshair, hit marker and the kill popup.

use super::*;

pub(super) fn draw_crosshair(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client, p: &PlayerSim) {
    // Unarmed (knife only): there is no gun to aim, so no crosshair.
    if p.arsenal.is_unarmed() {
        return;
    }
    let (cx, cy) = (w * 0.5, h * 0.5);
    let ads = p.arsenal.ads;
    // Aiming through an optic: BO2's red-dot glow, translucent, on the line of sight (BO2 projects
    // the reticle to infinity, so it stays centred). Iron sights have none.
    let optic = p.arsenal.current().def.attachments.iter().any(|a| OPTICS.contains(&a.as_str()));
    if ads > 0.6 && !optic {
        return;
    }
    if ads > 0.6 {
        let size = 30.0 * s;
        prims.push(Prim::Image {
            name: sprites::RED_DOT.to_string(),
            x: cx - size * 0.5,
            y: cy - size * 0.5,
            w: size,
            h: size,
            argb: 0xFFFF_FFFF,
            fallback: vec![Prim::Rect { x: cx - 2.0 * s, y: cy - 2.0 * s, w: 4.0 * s, h: 4.0 * s, argb: RED }],
        });
        return;
    }
    if p.body.sprinting || p.arsenal.is_busy() {
        return;
    }
    let stance = match p.body.stance {
        crate::movement::Stance::Stand => 0,
        crate::movement::Stance::Crouch => 1,
        crate::movement::Stance::Prone => 2,
    };
    let spread = p.arsenal.current().spread_deg(stance, ads).to_radians();
    let half_fov = client.last_hfov * 0.5;
    let gap = (spread.tan() / half_fov.tan() * (w * 0.5)).max(5.0 * s);
    let len = 10.0 * s;
    let alpha = ((1.0 - ads / 0.6) * 235.0) as u32;
    let col = (alpha << 24) | 0x00FF_FFFF;
    let shadow = ((alpha / 3) << 24) & 0xFF00_0000;
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let (x0, y0) = (cx + dx * gap, cy + dy * gap);
        let (x1, y1) = (cx + dx * (gap + len), cy + dy * (gap + len));
        prims.push(Prim::Line { x0: x0 + 1.0, y0: y0 + 1.0, x1: x1 + 1.0, y1: y1 + 1.0, width: 2.0 * s, argb: shadow });
        prims.push(Prim::Line { x0, y0, x1, y1, width: 2.0 * s, argb: col });
    }
}

pub(super) fn draw_hitmarker(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client) {
    let t = client.hud.hitmarker;
    if t <= 0.0 {
        return;
    }
    let (cx, cy) = (w * 0.5, h * 0.5);
    let a = ((t / HITMARKER_TIME).min(1.0) * 255.0) as u32;
    let rgb = if client.hud.hitmarker_kill { 0x00E8_3A2E } else { 0x00FF_FFFF };
    let col = (a << 24) | rgb;
    // BO2's `damage_feedback`: the X fills the top half of the image.
    let size = 30.0 * s;
    let (inner, outer) = (6.0 * s, 14.0 * s);
    let mut fallback = Vec::new();
    for (dx, dy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        fallback.push(Prim::Line { x0: cx + dx * inner, y0: cy + dy * inner, x1: cx + dx * outer, y1: cy + dy * outer, width: 2.0 * s, argb: col });
    }
    prims.push(Prim::Image {
        name: sprites::HIT_MARKER.to_string(),
        x: cx - size * 0.5,
        y: cy - size * 0.5,
        w: size,
        h: size * 2.0,
        argb: col,
        fallback,
    });
}

/// BO2's yellow "+100" score popup above-right of the crosshair after a kill.
pub(super) fn draw_score_popup(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client) {
    let t = client.hud.score_popup;
    if t <= 0.0 || client.hud.score_points == 0 {
        return;
    }
    let a = (t.min(0.4) / 0.4 * 255.0) as u32;
    let col = (a << 24) | (SCORE_YELLOW & 0x00FF_FFFF);
    let height = 26.0 * s;
    let rise = (1.0 - t / crate::client::SCORE_POPUP_TIME).max(0.0) * 12.0 * s;
    let top = h * 0.5 - 70.0 * s - rise;
    let right = w * 0.5 + 40.0 * s + digit_advance(height) * client.hud.score_points.to_string().len() as f32;
    let left = draw_number(prims, client.hud.score_points, right, top, height, col);
    // The plus sign.
    let (px, py, arm) = (left - 9.0 * s, top + height * 0.5, 5.0 * s);
    prims.push(Prim::Rect { x: px - arm, y: py - 1.5 * s, w: arm * 2.0, h: 3.0 * s, argb: col });
    prims.push(Prim::Rect { x: px - 1.5 * s, y: py - arm, w: 3.0 * s, h: arm * 2.0, argb: col });
}
