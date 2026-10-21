//! Damage blood, the field computer's screen and the death banner.

use super::*;

pub(super) fn draw_damage(prims: &mut Vec<Prim>, w: f32, h: f32, flash: f32, p: &PlayerSim, blood_art: bool) {
    let low = if p.max_health > 0.0 { 1.0 - (p.health / p.max_health).clamp(0.0, 1.0) } else { 0.0 };
    let strength = flash.max(low * 0.9).min(1.0);
    if strength <= 0.02 {
        return;
    }
    // BO2: blood creeps in from the corners, and the edges redden.
    let a = (strength * 220.0) as u32;
    let size = h * 0.55;
    let tint = (a << 24) | 0x00FF_FFFF;
    let corners = [(-size * 0.2, -size * 0.2), (w - size * 0.8, -size * 0.25), (-size * 0.25, h - size * 0.8), (w - size * 0.8, h - size * 0.8)];
    for (x, y) in corners {
        prims.push(Prim::Image { name: sprites::BLOOD.to_string(), x: x, y: y, w: size, h: size, argb: tint, fallback: Vec::new() });
    }
    let band = (h * 0.10).max(20.0);
    let edge = if blood_art { 70.0 } else { 120.0 };
    for i in 0..4 {
        let ea = (strength * edge * (1.0 - i as f32 / 4.0)) as u32;
        let col = (ea << 24) | 0x0090_0000;
        let o = band * i as f32 / 4.0;
        let t = band / 4.0;
        prims.push(Prim::Rect { x: 0.0, y: o, w: w, h: t, argb: col });
        prims.push(Prim::Rect { x: 0.0, y: h - o - t, w: w, h: t, argb: col });
        prims.push(Prim::Rect { x: o, y: 0.0, w: t, h: h, argb: col });
        prims.push(Prim::Rect { x: w - o - t, y: 0.0, w: t, h: h, argb: col });
    }
}

pub(super) fn draw_death_banner(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, t: f32) {
    let a = (t.min(1.0) * 230.0) as u32;
    prims.push(Prim::Rect { x: 0.0, y: h * 0.4, w: w, h: 64.0 * s, argb: (a / 2) << 24 });
    prims.push(Prim::Text { x: w * 0.5, y: h * 0.4 + 10.0 * s, text: "GENERAL DOWN".to_string(), argb: (a << 24) | (RED & 0x00FF_FFFF), size: (24.0 * s) as i32, align: 1 });
    prims.push(Prim::Text { x: w * 0.5, y: h * 0.4 + 42.0 * s, text: "Build a new General at your Command Center".to_string(), argb: (a << 24) | 0x00FF_FFFF, size: (12.0 * s) as i32, align: 1 });
}
