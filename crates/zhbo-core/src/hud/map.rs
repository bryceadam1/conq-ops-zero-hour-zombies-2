//! The rotating minimap and the compass strip under it.

use super::*;

/// Rotating minimap: you at the centre facing up, friendlies blue, enemies red (your army acts as
/// a permanent UAV), structures as squares. Returns its bottom edge.
pub(super) fn draw_minimap(host: &dyn ClientHost, prims: &mut Vec<Prim>, s: f32, yaw: f32, p: &PlayerSim, reveal: &dyn Fn(Vec3) -> bool) -> f32 {
    let size = 150.0 * s;
    // BO2 keeps it tight in the corner; just below Zero Hour's message line (EVA notices).
    let (x0, y0) = (14.0 * s, 30.0 * s);
    let (cx, cy) = (x0 + size * 0.5, y0 + size * 0.5);
    prims.push(Prim::Rect { x: x0, y: y0, w: size, h: size, argb: 0xB02A_2E33 });
    // BO2's thin light frame.
    outline_rect(prims, x0, y0, size, size, 1.5 * s, OUTLINE);

    let mut objs: Vec<ObjInfo> = Vec::new();
    host.query(p.body.pos, MINIMAP_RANGE * 1.42, p.general, &mut objs);
    let scale = (size * 0.5) / MINIMAP_RANGE;
    // Rotate world so the view yaw points up the screen.
    let (sn, cs) = (std::f32::consts::FRAC_PI_2 - yaw).sin_cos();
    let to_screen = |pos: Vec3| {
        let (dx, dy) = (pos.x - p.body.pos.x, pos.y - p.body.pos.y);
        let rx = dx * cs - dy * sn;
        let ry = dx * sn + dy * cs;
        (cx + rx * scale, cy - ry * scale)
    };
    let inside = |x: f32, y: f32, r: f32| x - r >= x0 && x + r <= x0 + size && y - r >= y0 && y + r <= y0 + size;
    for o in objs.iter().filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE)) {
        // Zero Hour's radar rule: enemies under your shroud don't show, unless one of your Sensor
        // Grenades sees them, or (BO2's rule) they are firing.
        if o.has(objflags::ENEMY) && o.has(objflags::SHROUDED) && !o.has(objflags::FIRING) && !reveal(o.pos) {
            continue;
        }
        let (x, y) = to_screen(o.pos);
        // Zero Hour's radar rule: everything in its owner's team colour.
        let team = o.color & 0x00FF_FFFF;
        if o.has(objflags::STRUCTURE) {
            let r = (o.major_radius * scale).clamp(3.0 * s, 14.0 * s);
            if inside(x, y, r) {
                prims.push(Prim::Rect { x: x - r, y: y - r, w: r * 2.0, h: r * 2.0, argb: team | 0x9000_0000 });
            }
        } else {
            // BO2's minimap dot, tinted.
            let r = if o.has(objflags::INFANTRY) { 3.5 * s } else { 4.5 * s };
            if inside(x, y, r) {
                prims.push(Prim::Image {
                    name: sprites::DOT.to_string(),
                    x: x - r,
                    y: y - r,
                    w: r * 2.0,
                    h: r * 2.0,
                    argb: team | 0xFF00_0000,
                    fallback: vec![Prim::Rect { x: x - r * 0.6, y: y - r * 0.6, w: r * 1.2, h: r * 1.2, argb: team | 0xFF00_0000 }],
                });
            }
        }
    }
    // You: BO2's player arrow, pointing up.
    let a = 18.0 * s;
    let mut fallback = Vec::new();
    {
        let a = a * 0.4;
        let (tip, l, r) = ((cx, cy - a), (cx - a * 0.8, cy + a * 0.7), (cx + a * 0.8, cy + a * 0.7));
        for ((ax, ay), (bx, by)) in [(tip, l), (l, r), (r, tip)] {
            fallback.push(Prim::Line { x0: ax, y0: ay, x1: bx, y1: by, width: 2.0 * s, argb: GOLD });
        }
    }
    prims.push(Prim::Image {
        name: sprites::PLAYER_ARROW.to_string(),
        x: cx - a * 0.5,
        y: cy - a * 0.5,
        w: a,
        h: a,
        argb: 0xFFFF_FFFF,
        fallback,
    });
    y0 + size
}

/// BO2's compass strip under the minimap (`hud_horizontal_compass_minimap`: a full turn, N at the
/// left), scrolled so the view heading is in the middle of the window.
pub(super) fn draw_compass(prims: &mut Vec<Prim>, s: f32, yaw: f32, top: f32) {
    let (x0, width, height) = (14.0 * s, 150.0 * s, 22.0 * s);
    let y0 = top + 2.0 * s;
    prims.push(Prim::Rect { x: x0, y: y0, w: width, h: height, argb: 0x8018_1A1C });
    let cx = x0 + width * 0.5;
    // Heading in compass degrees: yaw 0 is east (+x), north is +y.
    let heading = (90.0 - yaw.to_degrees()).rem_euclid(360.0);
    // The window shows half a turn; the art's N sits 8/512 in from its left edge.
    let strip = width * 2.0;
    let north_x = cx - heading / 360.0 * strip;
    let left = north_x - strip * 8.0 / 512.0;
    prims.push(Prim::ClipBegin { x: x0, y: y0, w: width, h: height });
    // Fallback: ticks and letters (the art unclips first, like the direct path did).
    let mut fallback = vec![Prim::ClipEnd];
    let px_per_deg = width / 180.0;
    for i in ((heading - 90.0) / 15.0).ceil() as i32..=((heading + 90.0) / 15.0).floor() as i32 {
        let deg = i as f32 * 15.0;
        let x = cx + (deg - heading) * px_per_deg;
        match (deg.rem_euclid(360.0) as i32 + 360) % 360 {
            0 => fallback.push(Prim::Text { x, y: y0 + 4.0 * s, text: "N".to_string(), argb: WHITE, size: (11.0 * s) as i32, align: 1 }),
            90 => fallback.push(Prim::Text { x, y: y0 + 4.0 * s, text: "E".to_string(), argb: WHITE, size: (11.0 * s) as i32, align: 1 }),
            180 => fallback.push(Prim::Text { x, y: y0 + 4.0 * s, text: "S".to_string(), argb: WHITE, size: (11.0 * s) as i32, align: 1 }),
            270 => fallback.push(Prim::Text { x, y: y0 + 4.0 * s, text: "W".to_string(), argb: WHITE, size: (11.0 * s) as i32, align: 1 }),
            _ => fallback.push(Prim::Line { x0: x, y0: y0 + 2.0 * s, x1: x, y1: y0 + 7.0 * s, width: 1.0, argb: DIM }),
        }
    }
    for k in -1..=1 {
        prims.push(Prim::Image { name: sprites::COMPASS.to_string(), x: left + k as f32 * strip, y: y0, w: strip, h: height, argb: 0xFFFF_FFFF, fallback: fallback.clone() });
    }
    prims.push(Prim::ClipEnd);
}
