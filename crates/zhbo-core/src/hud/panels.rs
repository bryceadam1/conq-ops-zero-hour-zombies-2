//! The bottom-right column and world marks: the field computer streak, Black Hat, building, ammo and offhands.

use super::*;

/// BO2's scorestreak column. Conquer Ops' one streak is the field computer (BO2's orbital VSAT icon),
/// always ready, so its box is lit; its name sits beside it like BO2's selected streak, with the key.
pub(super) fn draw_streaks(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, tablet_up: bool) {
    // The box art is a wide slanted box across the middle half of a square image.
    let b = 64.0 * s;
    let x = w - 14.0 * s - b;
    let y = h - 206.0 * s;
    // The bar to the left of the column.
    let (bw, bh) = (24.0 * s, 48.0 * s);
    let bx = x - bw + 6.0 * s;
    let by = y + (b - bh) * 0.5;
    prims.push(Prim::Image { name: sprites::STREAK_BAR_BACK.to_string(), x: bx, y: by, w: bw, h: bh, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    let bar_fallback = vec![Prim::Rect { x: bx + bw * 0.5, y: y, w: 2.0 * s, h: b, argb: 0x90FF_FFFF }];
    prims.push(Prim::Image { name: sprites::STREAK_BAR.to_string(), x: bx, y: by, w: bw, h: bh, argb: 0xFFFF_FFFF, fallback: bar_fallback });
    let glow = b * 1.1;
    prims.push(Prim::Image { name: sprites::STREAK_GLOW.to_string(), x: x + (b - glow) * 0.5, y: y + (b - glow) * 0.5, w: glow, h: glow, argb: if tablet_up { 0xC0FF_FFFF } else { 0x70FF_FFFF }, fallback: Vec::new() });
    let lit_fallback = slant_box(x, y + b * 0.2, b, b * 0.6, 0xB0A8_8228, Some(GOLD));
    prims.push(Prim::Image { name: sprites::STREAK_BOX_LIT.to_string(), x: x, y: y, w: b, h: b, argb: 0xFFFF_FFFF, fallback: lit_fallback });
    let icon = b * 0.46;
    prims.push(Prim::Image { name: sprites::STREAK_VSAT.to_string(), x: x + (b - icon) * 0.5, y: y + (b - icon) * 0.5, w: icon, h: icon, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Image { name: sprites::STREAK_BOX.to_string(), x: x, y: y, w: b, h: b, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Text { x: bx - 4.0 * s, y: y + b * 0.5 - 7.0 * s, text: "FIELD COMPUTER  [F]".to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 2 });
}

/// The Black Hat's progress over the crosshair.
pub(super) fn draw_hack(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client) {
    if client.hud.hack_shown <= 0.0 {
        return;
    }
    let (bw, bh) = (180.0 * s, 8.0 * s);
    let (x, y) = (w * 0.5 - bw * 0.5, h * 0.5 + 60.0 * s);
    prims.push(Prim::Rect { x: x, y: y, w: bw, h: bh, argb: PANEL });
    prims.push(Prim::Rect { x: x, y: y, w: bw * client.hud.hack_progress.clamp(0.0, 1.0), h: bh, argb: 0xF020_D040 });
    outline_rect(prims, x, y, bw, bh, 1.0 * s, OUTLINE);
    prims.push(Prim::Text { x: w * 0.5, y: y - 18.0 * s, text: "HACKING".to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 1 });
}

/// Building: with the hammer up, the build menu (right of centre) and the placement ghost; at a
/// structure to build or repair, BO2 Zombies' "Hold F" prompt and, while working, its progress.
pub(super) fn draw_build(host: &dyn ClientHost, prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client, p: &PlayerSim, show: bool) {
    let options = if p.hammer.is_some() { host.build_options(p.general) } else { Vec::new() };
    match p.hammer {
        Some(hm) if !options.is_empty() => {
            let choice = hm.choice.min(options.len() - 1);
            let o = options[choice];
            let legal = o.status == building::status::OK && host.can_place(p.general, o.template, hm.at, hm.angle);
            host.ghost(o.template, hm.at, hm.angle, legal);
            if show {
                let size = (13.0 * s) as i32;
                let row = 20.0 * s;
                let (pw, x) = (230.0 * s, w - 250.0 * s);
                let top = h * 0.5 - row * (options.len() as f32 * 0.5 + 1.0);
                prims.push(Prim::Rect { x: x, y: top, w: pw, h: row * (options.len() as f32 + 2.6), argb: PANEL });
                prims.push(Prim::Text { x: x + 10.0 * s, y: top + 3.0 * s, text: "BUILD".to_string(), argb: GOLD, size: size, align: 0 });
                prims.push(Prim::Text { x: x + pw - 10.0 * s, y: top + 3.0 * s, text: "[B] put away".to_string(), argb: DIM, size: (11.0 * s) as i32, align: 2 });
                for (i, o) in options.iter().enumerate() {
                    let y = top + row * (i as f32 + 1.2);
                    if i == choice {
                        outline_rect(prims, x + 4.0 * s, y - 1.0 * s, pw - 8.0 * s, row, 1.0 * s, GOLD);
                    }
                    let col = match o.status {
                        building::status::OK => WHITE,
                        building::status::NO_MONEY => RED,
                        _ => DIM,
                    };
                    prims.push(Prim::Text { x: x + 10.0 * s, y: y + 2.0 * s, text: o.name().to_string(), argb: col, size: size, align: 0 });
                    prims.push(Prim::Text { x: x + pw - 10.0 * s, y: y + 2.0 * s, text: format!("${}", o.cost), argb: col, size: size, align: 2 });
                }
                let y = top + row * (options.len() as f32 + 1.4);
                prims.push(Prim::Text { x: x + pw * 0.5, y: y, text: "[RMB] next   [Wheel] turn   [Fire] place".to_string(), argb: DIM, size: (11.0 * s) as i32, align: 1 });
            }
        }
        _ => host.ghost(0, Vec3::ZERO, 0.0, false),
    }
    if !show {
        return;
    }
    let size = (13.0 * s) as i32;
    if let Some(site) = p.site {
        if client.hud.build_shown > 0.0 {
            let (bw, bh) = (180.0 * s, 8.0 * s);
            let (x, y) = (w * 0.5 - bw * 0.5, h * 0.5 + 60.0 * s);
            prims.push(Prim::Rect { x: x, y: y, w: bw, h: bh, argb: PANEL });
            prims.push(Prim::Rect { x: x, y: y, w: bw * (client.hud.build_percent / 100.0).clamp(0.0, 1.0), h: bh, argb: 0xF0F2_C230 });
            outline_rect(prims, x, y, bw, bh, 1.0 * s, OUTLINE);
            let label = format!("{} {}  {:.0}%", work_text(site.kind).progress, site.name().to_uppercase(), client.hud.build_percent);
            prims.push(Prim::Text { x: w * 0.5, y: y - 18.0 * s, text: label.to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 1 });
        } else {
            let text = format!("Hold [F] to {} {} ({:.0}%)", work_text(site.kind).prompt, site.name(), site.percent);
            prims.push(Prim::Text { x: w * 0.5, y: h * 0.5 + 60.0 * s, text: text.to_string(), argb: WHITE, size: size, align: 1 });
        }
    }
    if client.hud.build_note_time > 0.0 {
        prims.push(Prim::Text { x: w * 0.5, y: h * 0.5 + 84.0 * s, text: client.hud.build_note.to_string(), argb: WHITE, size: size, align: 1 });
    }
}

/// Where a world point is on screen, if it is in front of the camera.
pub(super) fn project(client: &Client, w: f32, h: f32, at: Vec3) -> Option<(f32, f32)> {
    let fwd = client.view_dir.normalized_or_zero();
    let right = fwd.cross(Vec3::UP).normalized_or_zero();
    let up = right.cross(fwd);
    let d = at - client.view_eye;
    let z = d.dot(fwd);
    if z < 1.0 {
        return None;
    }
    let half = (client.last_hfov * 0.5).tan();
    let x = d.dot(right) / z / half;
    let y = d.dot(up) / z / half * (w / h);
    (x.abs() <= 1.0 && y.abs() <= 1.0).then(|| (w * 0.5 * (1.0 + x), h * 0.5 * (1.0 - y)))
}

/// Marks over the world: the target finder's red diamonds on enemies while aiming, and Engineer's
/// outline on enemy equipment.
pub(super) fn draw_marks(host: &dyn ClientHost, prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client, sim: &Sim, p: &PlayerSim) {
    let def = p.arsenal.current().def;
    let finder = def.target_finder && p.arsenal.ads > 0.6;
    let engineer = p.arsenal.has(crate::weapons::Perk::ENGINEER);
    if !finder && !engineer {
        return;
    }
    let mut objs = Vec::new();
    host.query(p.body.pos, 400.0, p.general, &mut objs);
    objs.sort_by_key(|o| o.id);
    for o in objs.iter().filter(|o| o.has(objflags::ENEMY) && !o.has(objflags::DEAD)) {
        let equipment = sim.equipment.placed.iter().any(|e| e.object == o.id);
        let mark = if equipment { engineer } else { finder && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE) };
        if !mark {
            continue;
        }
        let at = Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height.max(2.0) * 0.6);
        if let Some((x, y)) = project(client, w, h, at) {
            let r = 7.0 * s;
            let col = if equipment { AMBER } else { RED };
            for ((ax, ay), (bx, by)) in [((x, y - r), (x + r, y)), ((x + r, y), (x, y + r)), ((x, y + r), (x - r, y)), ((x - r, y), (x, y - r))] {
                prims.push(Prim::Line { x0: ax, y0: ay, x1: bx, y1: by, width: 2.0 * s, argb: col });
            }
        }
    }
}

pub(super) fn draw_ammo(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, p: &PlayerSim, prompts: bool) {
    // Knife only: no magazine, no reserve and no "No ammo" prompt to draw (the knife is not a
    // gun that ran dry). Unarmed firing is simply nothing.
    if p.arsenal.is_unarmed() {
        return;
    }
    let wpn = p.arsenal.current();
    // BO2's weapon panel frame, bottom-right.
    let (fw, fh) = (176.0 * s, 88.0 * s);
    let (fx, ftop) = (w - fw - 8.0 * s, h - fh - 4.0 * s);
    prims.push(Prim::Image { name: sprites::RIGHT_LOWER.to_string(), x: fx, y: ftop, w: fw, h: fh, argb: 0xFFFF_FFFF, fallback: Vec::new() });

    let right = w - 24.0 * s;
    let base = h - 16.0 * s;
    // BO2 draws the magazine and the reserve nearly the same size.
    let big = 38.0 * s;
    let small = 32.0 * s;
    let low = (wpn.ammo as f32) < wpn.def.clip as f32 * 0.3;
    let mag_col = if wpn.ammo == 0 { RED } else if low { AMBER } else { WHITE };
    let reserve_left = draw_number(prims, wpn.reserve as u32, right, base - small, small, WHITE);
    let slash_w = small * 0.5;
    let slash_fallback = vec![Prim::Text { x: reserve_left - slash_w * 0.5, y: base - small, text: "/".to_string(), argb: WHITE, size: (small * 0.75) as i32, align: 1 }];
    prims.push(Prim::Image { name: sprites::SLASH.to_string(), x: reserve_left - slash_w - 1.0 * s, y: base - small, w: slash_w, h: small, argb: WHITE, fallback: slash_fallback });
    draw_number(prims, wpn.ammo as u32, reserve_left - slash_w - 3.0 * s, base - big, big, mag_col);

    // The weapon's name in BO2's title box (its box is rows 22-40 of the 64-pixel art), with the
    // fire mode beside it.
    let mode = wpn.def.fire_mode_name();
    let row = ftop + 4.0 * s; // the box's top
    prims.push(Prim::Text { x: right, y: row + 1.0 * s, text: mode.to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 2 });
    let art = 20.0 * s / (18.0 / 64.0); // art height for a 20-pixel box
    let art_top = row - art * 22.0 / 64.0;
    // The box stretches for longer names (an alt mode's "Grenade Launcher").
    let text_w = (wpn.def.display_name.chars().count() as f32 * 7.2).max(44.0) * s;
    let box_right = right - 82.0 * s;
    let mid_left = box_right - art * 0.5 - text_w;
    let _titled = prims.push(Prim::Image { name: sprites::GUN_TITLE[1].to_string(), x: mid_left, y: art_top, w: text_w, h: art, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[0].to_string(), x: mid_left - art, y: art_top, w: art, h: art, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[2].to_string(), x: mid_left + text_w, y: art_top, w: art, h: art, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Text { x: mid_left + text_w * 0.5, y: row + 3.0 * s, text: wpn.def.display_name.to_string(), argb: WHITE, size: (13.0 * s) as i32, align: 1 });

    if !prompts {
        return;
    }
    let prompt = if wpn.is_reloading() {
        Some(("Reloading...", DIM))
    } else if wpn.ammo == 0 && wpn.reserve == 0 {
        Some(("No ammo", RED))
    } else if low {
        Some(("Press  R  to reload", WHITE))
    } else {
        None
    };
    if let Some((text, col)) = prompt {
        prims.push(Prim::Text { x: w * 0.5, y: h * 0.5 + 46.0 * s, text: text.to_string(), argb: col, size: (13.0 * s) as i32, align: 1 });
    }
}

/// BO2's lethal and tactical counters: each offhand's own HUD icon with how many are left, in the
/// weapon panel to the left of the ammo.
pub(super) fn draw_offhands(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, p: &PlayerSim) {
    // Unarmed: no lethal/tactical counters either (the empty kit has none).
    if p.arsenal.is_unarmed() {
        return;
    }
    let size = 20.0 * s;
    let y = h - 34.0 * s;
    let mut x = w - 182.0 * s;
    for slot in [&p.arsenal.lethal, &p.arsenal.tactical] {
        let col = if slot.count == 0 { 0x60FF_FFFF } else { 0xFFFF_FFFF };
        // No art: BO2's short name.
        let short: String = slot.def.display_name.chars().take(3).collect();
        let fallback = vec![Prim::Text { x: x + size * 0.5, y: y + 3.0 * s, text: short.clone(), argb: col, size: (10.0 * s) as i32, align: 1 }];
        prims.push(Prim::Image { name: format!("ZHBO_BO2_Icon_{}", slot.def.id), x, y, w: size, h: size, argb: col, fallback });
        prims.push(Prim::Text { x: x + size + 2.0 * s, y: y + 6.0 * s, text: slot.count.to_string(), argb: col, size: (11.0 * s) as i32, align: 0 });
        x += size + 18.0 * s;
    }
}
