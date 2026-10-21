//! Black Hat's progress, building, the world marks and the reload prompt.

use super::*;

/// The Black Hat's progress over the crosshair.
pub(super) fn draw_hack(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client) {
    if client.hud.hack_shown <= 0.0 {
        return;
    }
    let (bw, bh) = (180.0 * s, 8.0 * s);
    let (x, y) = (w * 0.5 - bw * 0.5, h * 0.5 + 60.0 * s);
    prims.push(Prim::Rect { x, y, w: bw, h: bh, argb: PANEL });
    prims.push(Prim::Rect { x, y, w: bw * client.hud.hack_progress.clamp(0.0, 1.0), h: bh, argb: 0xF020_D040 });
    outline_rect(prims, x, y, bw, bh, 1.0 * s, OUTLINE);
    prims.push(Prim::Text { x: w * 0.5, y: y - 18.0 * s, text: "HACKING".to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 1 });
}

/// Building: with the hammer up, the build menu (right of centre) and the placement ghost; at a
/// structure to build or repair, BO2 Zombies' "Hold F" prompt and, while working, its progress.
#[allow(clippy::too_many_arguments)]
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
                prims.push(Prim::Rect { x, y: top, w: pw, h: row * (options.len() as f32 + 2.6), argb: PANEL });
                prims.push(Prim::Text { x: x + 10.0 * s, y: top + 3.0 * s, text: "BUILD".to_string(), argb: GOLD, size, align: 0 });
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
                    prims.push(Prim::Text { x: x + 10.0 * s, y: y + 2.0 * s, text: o.name().to_string(), argb: col, size, align: 0 });
                    prims.push(Prim::Text { x: x + pw - 10.0 * s, y: y + 2.0 * s, text: format!("${}", o.cost), argb: col, size, align: 2 });
                }
                let y = top + row * (options.len() as f32 + 1.4);
                prims.push(Prim::Text { x: x + pw * 0.5, y, text: "[RMB] next   [Wheel] turn   [Fire] place".to_string(), argb: DIM, size: (11.0 * s) as i32, align: 1 });
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
            prims.push(Prim::Rect { x, y, w: bw, h: bh, argb: PANEL });
            prims.push(Prim::Rect { x, y, w: bw * (client.hud.build_percent / 100.0).clamp(0.0, 1.0), h: bh, argb: 0xF0F2_C230 });
            outline_rect(prims, x, y, bw, bh, 1.0 * s, OUTLINE);
            let label = format!("{} {}  {:.0}%", work_text(site.kind).progress, site.name().to_uppercase(), client.hud.build_percent);
            prims.push(Prim::Text { x: w * 0.5, y: y - 18.0 * s, text: label.to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 1 });
        } else {
            let text = format!("Hold [F] to {} {} ({:.0}%)", work_text(site.kind).prompt, site.name(), site.percent);
            prims.push(Prim::Text { x: w * 0.5, y: h * 0.5 + 60.0 * s, text: text.to_string(), argb: WHITE, size, align: 1 });
        }
    }
    if client.hud.build_note_time > 0.0 {
        prims.push(Prim::Text { x: w * 0.5, y: h * 0.5 + 84.0 * s, text: client.hud.build_note.to_string(), argb: WHITE, size, align: 1 });
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
    (x.abs() <= 1.0 && y.abs() <= 1.0).then_some((w * 0.5 * (1.0 + x), h * 0.5 * (1.0 - y)))
}

/// Marks over the world: the target finder's red diamonds on enemies while aiming, and Engineer's
/// outline on enemy equipment.
#[allow(clippy::too_many_arguments)]
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

/// BO2's reload hint under the crosshair when the clip runs low.
pub(super) fn draw_reload_prompt(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, p: &PlayerSim) {
    if p.arsenal.is_unarmed() {
        return;
    }
    let wpn = p.arsenal.current();
    let low = super::ammo::low_clip(wpn.ammo, wpn.def.clip);
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
