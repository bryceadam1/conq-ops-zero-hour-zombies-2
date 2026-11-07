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
    prims.push(Prim::Text { x: w * 0.5, y: y - 18.0 * s, text: crate::strings::text("HUD:ZHBO_Hacking"), argb: WHITE, size: (12.0 * s) as i32, align: 1 });
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
                prims.push(Prim::Text { x: x + 10.0 * s, y: top + 3.0 * s, text: crate::strings::text("HUD:ZHBO_Build"), argb: GOLD, size, align: 0 });
                prims.push(Prim::Text { x: x + pw - 10.0 * s, y: top + 3.0 * s, text: crate::strings::text("HUD:ZHBO_PutAway"), argb: DIM, size: (11.0 * s) as i32, align: 2 });
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

/// Draws a diamond mark centred on a screen point in a colour.
type Diamond<'a> = &'a dyn Fn(&mut Vec<Prim>, f32, f32, u32);

/// Marks over the world: the target finder's red diamonds on enemies while aiming, and
/// Engineer's view of enemy equipment.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_marks(host: &dyn ClientHost, prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client, sim: &Sim, p: &PlayerSim) {
    let def = p.arsenal.current().def;
    // The lens draws the target finder's own diamonds (BO2's display, `sim::optics`) when the
    // native viewmodel drew its reticle; the W3D fallback keeps these.
    let finder = def.target_finder && p.arsenal.ads > 0.6 && !crate::nativevm::lens_reticle_drawn();
    let diamond = |prims: &mut Vec<Prim>, x: f32, y: f32, col: u32| {
        let r = 7.0 * s;
        for ((ax, ay), (bx, by)) in [((x, y - r), (x + r, y)), ((x + r, y), (x, y + r)), ((x, y + r), (x - r, y)), ((x - r, y), (x, y - r))] {
            prims.push(Prim::Line { x0: ax, y0: ay, x1: bx, y1: by, width: 2.0 * s, argb: col });
        }
    };
    if finder {
        let mut objs = Vec::new();
        host.query(p.body.pos, 400.0, p.general, &mut objs);
        objs.sort_by_key(|o| o.id);
        let placed = |id: u32| sim.equipment.placed.iter().any(|e| e.object == id);
        for o in objs.iter().filter(|o| o.has(objflags::ENEMY) && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE) && !placed(o.id)) {
            let at = Vec3::new(o.pos.x, o.pos.y, o.pos.z + o.height.max(2.0) * 0.6);
            if let Some((x, y)) = project(client, w, h, at) {
                diamond(prims, x, y, RED);
            }
        }
    }
    draw_recon(host, prims, w, h, s, client, sim, p, &diamond);
}

/// Engineer's `specialty_showenemyequipment` (`gametypes/_weaponobjects.gsc`
/// `resetreconmodelvisibility`, `:2253-2300`; `_riotshield.gsc:160-190`): every enemy weapon
/// object whose watcher has a recon model — the Bouncing Betty, C4, Claymore, Sensor Grenade,
/// Tactical Insertion, Shock Charge and Trophy System — and every enemy deployed Assault Shield
/// shows to him through walls at any distance (`setforcenocull`, visible only to him). BO2 draws the
/// item's `_detect` model in its detectable material; here its outline: the box of its projectile
/// model (BO2's own bounds, `WeaponDef::projectile_box`) drawn over everything, a diamond where the
/// def carries none.
#[allow(clippy::too_many_arguments)]
fn draw_recon(host: &dyn ClientHost, prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client, sim: &Sim, p: &PlayerSim, diamond: Diamond) {
    if !p.arsenal.has_specialty(crate::weapons::Specialty::ShowEnemyEquipment) {
        return;
    }
    let enemy = |id: u32, at: Vec3| {
        let mut objs = Vec::new();
        host.query(at, 4.0, p.general, &mut objs);
        objs.iter().any(|o| o.id == id && o.has(objflags::ENEMY) && !o.has(objflags::DEAD))
    };
    for e in sim.equipment.placed.iter().filter(|e| e.object != 0 && recon(e.kind) && enemy(e.object, e.pos)) {
        match e.def.projectile_box() {
            Some((c, half)) => {
                let corner = |i: usize| e.pos + c + Vec3::new(if i & 1 == 0 { -half.x } else { half.x }, if i & 2 == 0 { -half.y } else { half.y }, if i & 4 == 0 { -half.z } else { half.z });
                let pts: Vec<Option<(f32, f32)>> = (0..8).map(|i| project(client, w, h, corner(i))).collect();
                for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                    if let (Some((ax, ay)), Some((bx, by))) = (pts[a], pts[b]) {
                        prims.push(Prim::Line { x0: ax, y0: ay, x1: bx, y1: by, width: 2.0 * s, argb: RED });
                    }
                }
            }
            None => {
                if let Some((x, y)) = project(client, w, h, e.pos) {
                    diamond(prims, x, y, RED);
                }
            }
        }
    }
    for d in sim.deployed.iter().filter(|d| d.object != 0 && d.destroyed.is_none()) {
        let at = Vec3::new(d.bearer.pos.x, d.bearer.pos.y, d.bearer.pos.z + crate::units::inches(crate::tuning::get().riotshield_placement_zoffset));
        if enemy(d.object, at) {
            if let Some((x, y)) = project(client, w, h, at) {
                diamond(prims, x, y, RED);
            }
        }
    }
}

/// Whether a piece of `kind` has a recon model (its watcher's `reconmodel`): everything that is
/// placed but the mod's `Other` (the Black Hat never stays down).
pub(super) fn recon(kind: crate::equipment::Kind) -> bool {
    kind != crate::equipment::Kind::Other
}

/// Whether BO2 draws the hold-breath hint (`FUN_004dc750`, `cg_drawBreathHint`): fully aimed
/// through an overlay scope whose def has `bHoldBreathToSteady`, while the breath is not held.
pub(super) fn breath_hint_shown(p: &PlayerSim) -> bool {
    let def = p.arsenal.current().def;
    !p.idle.breath.holding
        && def.class.is_gun()
        && def.zoom.hold_breath
        && crate::weapons::idle_sway::sways_view(def)
        && p.arsenal.ads >= 1.0
}
