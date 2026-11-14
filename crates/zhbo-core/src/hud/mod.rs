//! Black Ops II's multiplayer HUD, drawn after Zero Hour's in-game UI, ported element by element
//! from BO2's own HUD Lua (`~/Documents/BO2-reference/lua-dump/decompiled/patch_mp/ui_mp/t6/hud/`)
//! on the LUI layer ([`crate::lui`]): every box keeps the Lua's numbers and anchors against the
//! 720-unit root, every image is the BO2 material the Lua registers (converted under its own name,
//! [`crate::lui::materials`]), and the code-drawn parts follow the engine (Ghidra G6/G7).
//!
//! * Bottom-right: the ammo area ([`ammo`]: panel, offhand counters, HUD digits, weapon label,
//!   fire-rate icon, alt fire) and above it the scorestreak column ([`rewards`]).
//! * Bottom-left: the score panel ([`score`]).
//! * Top-left: the minimap and the compass strip ([`map`]).
//! * Centre: crosshair, hit marker, score popup ([`center`]); damage feedback ([`damage`]); the
//!   grenade indicator and the allies' names ([`grenade`]).
//! * Left, above the score panel: the kill feed ([`obituary`]).
//! * Tab: the scoreboard ([`scoreboard`]); the full-map radar moved to M.
//!
//! Deliberate divergences: no scorestreaks (the column holds the field computer and the build
//! menu); the team is the player's Zero Hour army (its logo, radar colour and funds on the score
//! panel, team colours on the minimap). Without the converted BO2 art the HUD falls back to drawn
//! shapes and the engine font.

use crate::building::{self, work_text};
use crate::client::{Client, ViewMode};
use crate::host::{objflags, ClientHost, ObjInfo};
use crate::input::Key;
use crate::math::Vec3;
use crate::sim::{Mode, PlayerSim, Sim};

pub mod ammo;
pub mod center;
pub mod command;
pub mod damage;
pub mod digit;
pub mod feedback;
pub mod frame;
pub mod grenade;
pub mod hints;
pub mod map;
pub mod obituary;
mod panels;
pub mod pickup;
pub mod rewards;
pub mod score;
pub mod scoreboard;
mod shapes;
pub mod spectate;
pub mod sprites;
pub mod state;
pub mod zombies;
#[cfg(test)]
mod tests;

use center::*;
pub use center::{reticle_color_rgb, HINT_TIME, SCAVENGER_ICON_FADE};
use damage::*;
use map::*;
use panels::*;
pub(crate) use shapes::*;

/// One retained HUD primitive: element functions build these (so the layout is unit-testable
/// without a drawing host), and [`render`] walks them into the engine's draw callbacks.
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    /// A solid rectangle.
    Rect { x: f32, y: f32, w: f32, h: f32, argb: u32 },
    /// A line of the given width.
    Line { x0: f32, y0: f32, x1: f32, y1: f32, width: f32, argb: u32 },
    /// UTF-8 text at size (height in pixels), aligned 0 left / 1 centre / 2 right at x.
    Text { x: f32, y: f32, text: String, argb: u32, size: i32, align: i32 },
    /// A named sprite from the converted BO2 art, tinted by `argb`. If the art is missing, the
    /// fallback prims draw instead (the no-BO2-art paths).
    Image { name: String, x: f32, y: f32, w: f32, h: f32, argb: u32, fallback: Vec<Prim> },
    /// A named sprite on a quad (`ClientHost::image_quad`'s corner order), or the fallback.
    Quad { name: String, corners: [(f32, f32); 4], argb: u32, fallback: Vec<Prim> },
    /// Intersect subsequent prims with this screen rectangle until [`Prim::ClipEnd`].
    ClipBegin { x: f32, y: f32, w: f32, h: f32 },
    ClipEnd,
}

/// Walk the prims and call the engine's drawing callbacks.
pub fn render(prims: &[Prim], host: &dyn ClientHost) {
    for p in prims {
        match p {
            Prim::Rect { x, y, w, h, argb } => host.fill_rect(*x, *y, *w, *h, *argb),
            Prim::Line { x0, y0, x1, y1, width, argb } => host.line(*x0, *y0, *x1, *y1, *width, *argb),
            Prim::Text { x, y, text, argb, size, align } => host.text(*x, *y, text, *argb, *size, *align),
            Prim::Image { name, x, y, w, h, argb, fallback } => {
                if !host.image(name, *x, *y, *w, *h, *argb) && !fallback.is_empty() {
                    render(fallback, host);
                }
            }
            Prim::Quad { name, corners, argb, fallback } => {
                if !host.image_quad(name, *corners, *argb) && !fallback.is_empty() {
                    render(fallback, host);
                }
            }
            Prim::ClipBegin { x, y, w, h } => host.clip(Some((*x, *y, *w, *h))),
            Prim::ClipEnd => host.clip(None),
        }
    }
}


pub(crate) const WHITE: u32 = 0xF2FF_FFFF;
pub(crate) const DIM: u32 = 0xA0FF_FFFF;
pub(crate) const RED: u32 = 0xF0E8_3A2E;
/// BO2's player arrow and ready-streak gold.
pub(crate) const GOLD: u32 = 0xFFF2_C230;
/// Panel fills and outlines.
pub(crate) const PANEL: u32 = 0x9010_1214;
pub(crate) const OUTLINE: u32 = 0xB0C8_CCD0;

/// BO2's sight attachments (a reticle on the line of sight).
pub(crate) const OPTICS: &[&str] = &["reflex", "acog", "holo", "rangefinder", "dualoptic", "mms", "ir", "vzoom"];

/// Minimap radius in world units.
pub(super) const MINIMAP_RANGE: f32 = 320.0;

/// The snipers' full-screen ADS overlay materials (`scope_overlay_<weapon>`), each kept at 1024 px
/// as its own standalone page (not packed into the atlas).
pub const OVERLAY_MATERIALS: &[(&str, u32)] = &[
    ("scope_overlay_ballista", 1024),
    ("scope_overlay_dsr50", 1024),
    ("scope_overlay_envg_ir", 1024),
    ("scope_overlay_specter", 1024),
    ("scope_overlay_svu", 1024),
    ("scope_overlay_vzoom", 1024),
    ("scope_overlay_xpr50", 1024),
];

/// Every BO2 material the HUD draws, for the converter (`zhbo_core::lui::materials`).
pub fn materials() -> impl Iterator<Item = &'static str> {
    digit::MATERIALS
        .iter()
        .chain(ammo::MATERIALS.iter())
        .chain(score::MATERIALS.iter())
        .chain(rewards::MATERIALS.iter())
        .chain(damage::MATERIALS.iter())
        .chain(spectate::MATERIALS.iter())
        .chain(["damage_feedback", "damage_feedback_flak", "damage_feedback_tac", "hud_horizontal_compass_minimap", "hud_grenadeicon", "hud_grenadepointer"].iter())
        .chain(["killicondied", "killiconheadshot", "hud_obit_knife", "hud_obit_death_suicide", "hud_obit_death_crush", "hud_obit_death_falling", "hud_obit_death_grenade_round"].iter())
        .copied()
}

/// The local player's army colour, from the General's own object (Zero Hour's radar colour).
/// The scoreboard's sides: the local player's (his own row from the HUD's tally, his allies')
/// under `own_color`, then everyone else who has played in first person, each sorted by score.
fn scoreboard_blocks(host: &dyn ClientHost, client: &Client, own_color: u32, local_player: i32) -> Vec<scoreboard::Block> {
    let st = &client.hud.lui;
    let points = crate::client::KILL_POINTS;
    let name = |seat: i32| host.player_name(seat).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| if seat == local_player { "General".to_string() } else { format!("Player {}", seat + 1) });
    let mine = usize::try_from(local_player).ok().and_then(|i| client.seats.get(i)).copied().unwrap_or_default();
    let own_row = scoreboard::Row { name: name(local_player), values: [st.kills * points + mine.assist_points, st.kills, st.deaths, mine.assists], you: true };
    let mut own = scoreboard::Block { color: own_color, rows: vec![own_row] };
    let mut enemies = scoreboard::Block { color: 0, rows: Vec::new() };
    for (seat, t) in client.seats.iter().enumerate().filter(|(seat, t)| t.seen && *seat as i32 != local_player) {
        let row = scoreboard::Row { name: name(seat as i32), values: [t.kills * points + t.assist_points, t.kills, t.deaths, t.assists], you: false };
        if t.ally {
            own.rows.push(row);
        } else {
            if enemies.rows.is_empty() {
                enemies.color = t.color;
            }
            enemies.rows.push(row);
        }
    }
    let mut blocks = vec![own];
    if !enemies.rows.is_empty() {
        blocks.push(enemies);
    }
    for b in &mut blocks {
        b.rows.sort_by(|a, b| b.values[0].cmp(&a.values[0]));
    }
    blocks
}

fn team_color(host: &dyn ClientHost, p: &PlayerSim) -> u32 {
    let mut objs = Vec::new();
    host.query(p.body.pos, 2.0, p.general, &mut objs);
    objs.iter().find(|o| o.id == p.general).map_or(0xFFFF_FFFF, |o| o.color)
}

/// The scope's 2D screen-space ADS overlay (`FUN_004b51e0`). It draws while BO2's reticle zoom is
/// up ([`crate::weapons::scope::reticle_zoom`], in the zoom direction as updated this frame), white
/// at alpha `zoom / max(fOverlayAlphaScale, 1)`; over the same window the viewmodel is not drawn
/// (`HudLui::gun_under_overlay`, the native viewmodel). The art keeps its own aspect, centred
/// ([`crate::weapons::scope::overlay_placement`]: a sniper's 480 × 480 scope is a square as tall
/// as the screen), and `FUN_004b4840` fills the rest of the screen with the art's edge, which is
/// opaque black in every overlay BO2 ships (the converter's install-gated test checks it), so the
/// fills are black at the overlay's alpha.
fn draw_scope_overlay(prims: &mut Vec<Prim>, w: f32, h: f32, p: &PlayerSim, zooming_in: bool) {
    let def = p.arsenal.current_weapon();
    let Some(zoom) = crate::weapons::scope::reticle_zoom(def, p.arsenal.ads, zooming_in) else { return };
    if def.overlay_material.is_empty() {
        return;
    }
    let a = (crate::weapons::scope::overlay_alpha(def, zoom).clamp(0.0, 1.0) * 255.0).round() as u32;
    scope_overlay(prims, def, a, w, h);
}

/// Set once the missing-`OverlaySize` fallback has been logged (once per run).
pub(crate) static SCOPE_SIZE_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The overlay's prims for `def` at alpha `a` (0–255) on a `w` × `h` screen (`FUN_004b4c70`);
/// the screenshot harness renders the converted overlays through it.
pub fn scope_overlay(prims: &mut Vec<Prim>, def: &crate::weapons::WeaponDef, a: u32, w: f32, h: f32) {
    use crate::weapons::scope::{overlay_fills, overlay_placement};
    // The converted MappedImage name. If the install lacks the art, the empty fallback draws
    // nothing (log-or-skip); the converter reports the missing material and the install-gated
    // test catches a sniper whose overlay did not convert.
    let name = crate::lui::materials::image(&def.overlay_material);
    let argb = (a << 24) | (WHITE & 0x00FF_FFFF);
    let (size, fell_back) = crate::weapons::scope::overlay_size(def);
    // To the process log, like an unknown weapon id (`weapons::catalog`): the autotest captures it.
    if fell_back && !SCOPE_SIZE_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("zhbo: scope overlay of {:?} has no OverlaySize (a stale weapon table?); drawing the tuning pack's ScopeOverlayWidth/Height", def.id);
    }
    let place = overlay_placement(size, w, h);
    for q in &place.quads {
        let axis_aligned = q[0].0 < q[1].0 && q[0].1 < q[3].1 && q[0].1 == q[1].1 && q[0].0 == q[3].0;
        if axis_aligned {
            prims.push(Prim::Image { name: name.clone(), x: q[0].0, y: q[0].1, w: q[1].0 - q[0].0, h: q[3].1 - q[0].1, argb, fallback: Vec::new() });
        } else {
            prims.push(Prim::Quad { name: name.clone(), corners: *q, argb, fallback: Vec::new() });
        }
    }
    for (x, y, fw, fh) in overlay_fills(place.bounds, w, h) {
        prims.push(Prim::Rect { x, y, w: fw, h: fh, argb: a << 24 });
    }
}

/// A flashed shellshock's whiteout over the view (`crate::shellshock::Screen::white`, t6mp
/// `FUN_005105c0`), under the HUD's own elements — only when the renderer's shock layer did not
/// take this frame (`nativevm::shock_layer_drawn`): that layer adds the white and the frozen
/// screengrab (and a blurred shock's trail) as BO2 does. This is the fallback without it (the DX8
/// build, a scene the viewmodel draw did not reach).
fn draw_shellshock(prims: &mut Vec<Prim>, w: f32, h: f32, screen: crate::shellshock::Screen) {
    if crate::nativevm::shock_layer_drawn() {
        return;
    }
    let a = (screen.white.clamp(0.0, 1.0) * 255.0).round() as u32;
    if a > 0 {
        prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h, argb: (a << 24) | 0x00ff_ffff });
    }
}

pub fn draw(host: &dyn ClientHost, prims: &mut Vec<Prim>, client: &Client, sim: &Sim, local_player: i32) {
    // First person is drawn from the client's prediction of the local player while it runs (the
    // ammo leaves the counter as the shot leaves the gun), else from the sim's record.
    let player = client.predict.view().or_else(|| sim.player(local_player));
    let (w, h) = host.screen_size();
    let s = (h / 720.0).max(0.5);
    let l = frame::Lui::new(w, h);
    let st = &client.hud.lui;

    if client.hud.death_banner > 0.0 {
        draw_death_banner(prims, w, h, s, client.hud.death_banner);
    }

    // The create-a-class screen can be up in the overhead view too (only when the command bar's
    // "Create a Class" button forces it), so draw it before the first-person-only overlays below.
    // The client decides (`class_pick_open`): an unclassed General in the overhead view is *not*
    // forced to pick, which used to swallow every click from match start.
    let in_fps = matches!(client.view, ViewMode::Fps);
    if !in_fps && client.class_pick_open {
        crate::classui::draw_screen(prims, w, h, &client.class_screen_view());
    }

    // A spectator overhead with a General to follow: the way back into first person.
    let spectating = client.spectator.is_some();
    if spectating && !in_fps && client.spectator.as_ref().is_some_and(|s| s.overhead) && !crate::client::spectate::candidates(sim).is_empty() {
        spectate::draw_overhead_prompt(&l, prims, crate::input::key_name(client.cfg.bindings.field_computer));
    }

    let Some(p) = player.filter(|_| in_fps) else {
        host.ghost(0, Vec3::ZERO, 0.0, false);
        return;
    };

    draw_low_health(prims, w, h, st.low_health.alpha(st.clock));
    draw_shellshock(prims, w, h, p.shock.screen());
    // The command line and its echo (a spectator types nothing).
    if !spectating {
        command::draw(prims, w, h, s, &client.command);
    }
    // Under the scope overlay (BO2's reticle zoom, in this frame's zoom direction) BO2 hides the
    // damage indicators (`FUN_004adda0`), and the scoped bit hides the minimap and the compass
    // strip ([`crate::weapons::scope::scoped`]). Both switch at once, without a fade. The ammo
    // area, score panel, scorestreak column, hit marker and score popup stay.
    let def = p.arsenal.current_weapon();
    let zoom = crate::weapons::scope::reticle_zoom(def, p.arsenal.ads, st.ads_zoom_in);
    let scoped = crate::weapons::scope::scoped(def, p.arsenal.ads, st.ads_zoom_in);
    if zoom.is_none() {
        draw_indicators(prims, w, h, client.yaw.to_degrees(), st);
    }

    // The kill feed: up whatever else is (BO2 hides it only behind a menu and in the killcam).
    let names = |seat: u8| host.player_name(seat as i32).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Player {}", seat + 1));
    let ally = |seat: u8| seat as i32 == local_player || client.seats.get(seat as usize).is_some_and(|t| t.ally);
    let suicide = crate::weapons::catalog().ui_text("CGAME_QUAKE_SUICIDE").unwrap_or("");
    obituary::draw(&l, prims, &client.feedback.obits, st.clock, &obituary::Names { name: &names, ally: &ally, suicide });

    // Using the field computer, the tablet is up (zhbo_core::viewmodel): no crosshair.
    let tablet_up = matches!(p.mode, Mode::FieldComputer { .. });
    let map_open = client.hud.map_open;
    // A Shock Charge's zap hides his HUD until its exit shock (`setclientuivisibilityflag(
    // "hud_visible", 0 )`, `_proximity_grenade.gsc:191-207`; `sim::shock::Taser`).
    let tazed = p.taser.is_some_and(|z| z.hides_hud());
    // BO2 hides its HUD under the scoreboard (`BIT_SCOREBOARD_OPEN` in every panel's visibility).
    if tazed {
        host.ghost(0, Vec3::ZERO, 0.0, false);
    } else if client.hud.scoreboard_open {
        scoreboard::draw(&l, prims, &scoreboard_blocks(host, client, team_color(host, p), local_player));
        host.ghost(0, Vec3::ZERO, 0.0, false);
    } else {
        if !tablet_up && !map_open {
            draw_marks(host, prims, w, h, s, client, sim, p);
            draw_names(prims, w, h, client, host);
            // The grenade indicator: not under the scope overlay (`FUN_004aea40` tests
            // `FUN_004b4760`).
            if zoom.is_none() {
                if let Ok(me) = usize::try_from(local_player) {
                    let seats = client.seats;
                    let ally = |seat: usize| seats.get(seat).is_some_and(|t| t.ally);
                    let shown = grenade::collect(sim, me, p, &ally);
                    grenade::draw(prims, w, h, client.view_eye, client.yaw, &shown, st.clock);
                }
            }
            // BO2's scope overlay: up while the reticle zoom is, when the viewmodel is not drawn.
            draw_scope_overlay(prims, w, h, p, st.ads_zoom_in);
            draw_crosshair(prims, w, h, s, client, p, zoom);
            // What is the player's alone — his hit marker, score popup, hints, hack bar and the
            // prompts naming his keys — a spectator does not see (BO2's script HUD elements belong
            // to the player; the spectator's own prompts are the dead-spectate panel's).
            if !spectating {
                draw_hitmarker(prims, w, h, client);
                center::draw_scavenger_icon(prims, w, h, client);
                draw_score_popup(&l, prims, client);
                draw_hint(&l, prims, client);
                pickup::draw(&l, prims, sim, p, crate::input::key_name(client.cfg.bindings.field_computer));
                draw_hack(prims, w, h, s, client);
            }
            if p.hammer.is_none() && !p.building && !spectating {
                hints::draw_low_ammo(&l, prims, p, crate::input::key_name(client.cfg.bindings.reload), client.hud.lui.clock);
                draw_breath_hint(&l, prims, p, crate::input::key_name(client.cfg.bindings.sprint));
            }
        }
        if spectating {
            host.ghost(0, Vec3::ZERO, 0.0, false);
        } else {
            draw_build(host, prims, w, h, s, client, p, !tablet_up && !map_open);
        }
        if !scoped {
            let marks = sim.sensor_marks(local_player);
            draw_minimap(host, prims, &l, client.yaw, p, &marks);
            draw_compass(prims, &l, client.yaw);
        }
        // A survivor's points are on the Zombies points list (`hud::zombies`).
        if !p.survivor {
            score::draw(&l, prims, team_color(host, p), host.cash(local_player).max(0) as u32);
        }
        if spectating {
            let name = host.player_name(local_player).unwrap_or_default();
            let b = &client.cfg.bindings;
            let panel = spectate::Panel { name: &name, prev_key: crate::input::key_name(Key::MouseLeft), next_key: crate::input::key_name(Key::MouseRight), view_key: crate::input::key_name(b.field_computer) };
            spectate::draw(&l, prims, &panel);
        } else if p.survivor {
            // A survivor wears the Zombies HUD (`hud::zombies`): the round's chalk and the d-pad
            // area with his action slots, the field computer's key under it.
            let b = &client.cfg.bindings;
            let keys = [b.action_slot_1, b.action_slot_2, b.alt_mode, b.action_slot_4].map(crate::input::key_name);
            let can_build = !host.build_options(p.general).is_empty();
            zombies::draw_round(&l, prims, sim.rounds.round);
            zombies::draw_dpad(&l, prims, &zombies::slots(p, can_build, keys), crate::input::key_name(b.field_computer));
            zombies::draw_ammo(&l, prims, p, st.clock);
            zombies::draw_weapon_label(&l, prims, p, st.weapon_ms());
            zombies::draw_offhands(&l, prims, p);
            // The players in the game, in the order the round engine met them (yours at least).
            let mut order: Vec<i32> = sim.rounds.players.clone();
            if !order.contains(&local_player) {
                order.push(local_player);
            }
            let points: Vec<zombies::Points> = order.iter().enumerate().map(|(k, &pl)| zombies::Points { points: host.cash(pl), color: k, own: pl == local_player }).collect();
            zombies::draw_points(&l, prims, &points);
            let use_key = crate::input::key_name(b.field_computer);
            let name = |player: i32| host.player_name(player).unwrap_or_default();
            zombies::draw_last_stand(&l, prims, sim, p, use_key, &name, &|at| panels::project(client, w, h, at));
            if sim.revive_target(p).is_none() && !zombies::draw_magic_box(&l, prims, sim, p, use_key) && !zombies::draw_perk_machine(&l, prims, sim, p, use_key)
                && !zombies::draw_pack_a_punch(&l, prims, sim, p, use_key)
            {
                zombies::draw_wall_buy(&l, prims, sim, p, use_key);
            }
            zombies::draw_perks(&l, prims, sim, p);
            zombies::draw_powerups(&l, prims, sim, sim.frame as f32);
        } else {
            // The scorestreak column is the player's own commands: the tablet, and the build tool
            // when an action slot holds it.
            let build = p.action_slots.holds(crate::weapons::actionslot::Equipment::BuildTool).then(|| !host.build_options(p.general).is_empty());
            rewards::draw(&l, prims, tablet_up, p.hammer.is_some(), build, st.weapon_ms());
        }
        let alt_key = crate::input::key_name(client.cfg.bindings.alt_mode);
        if !p.survivor {
            ammo::draw(&l, prims, p, st.weapon_ms(), st.clock, alt_key);
        }
        if map_open {
            draw_map(host, prims, w, h, s, p, crate::input::key_name(client.cfg.bindings.map));
        }
    }
    if client.class_pick_open {
        crate::classui::draw_screen(prims, w, h, &client.class_screen_view());
    }
}

/// The Generals' nameplates over their heads ([`grenade::draw_name`]), each in its owner's house
/// colour at its fade ([`feedback::Nameplate`]).
fn draw_names(prims: &mut Vec<Prim>, w: f32, h: f32, client: &Client, host: &dyn ClientHost) {
    let now = client.hud.lui.clock;
    for plate in &client.feedback.names {
        let alpha = plate.fade.alpha(now);
        if alpha <= 0.0 {
            continue;
        }
        let Some((x, y)) = project(client, w, h, plate.at) else { continue };
        let name = host.player_name(plate.owner).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Player {}", plate.owner + 1));
        grenade::draw_name(prims, h, x, y, (plate.at - client.view_eye).length(), &name, frame::rgb_of(plate.color), alpha);
    }
}

/// Held map key (M; Tab is BO2's scoreboard): Zero Hour's own radar over the middle of the view,
/// with the whole map, shroud, team colours and radar events, and BO2's player bracket on you. Zero
/// Hour's rule applies: no radar, no map.
pub(super) fn draw_map(host: &dyn ClientHost, prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, p: &PlayerSim, key: &str) {
    let side = (h * 0.74).min(w * 0.8);
    let (x, y) = ((w - side) * 0.5, (h - side) * 0.5 + 8.0 * s);
    prims.push(Prim::Rect { x: x - 6.0 * s, y: y - 30.0 * s, w: side + 12.0 * s, h: side + 36.0 * s, argb: 0xD010_1214 });
    outline_rect(prims, x - 6.0 * s, y - 30.0 * s, side + 12.0 * s, side + 36.0 * s, 1.5 * s, OUTLINE);
    prims.push(Prim::Text { x, y: y - 24.0 * s, text: "TACTICAL MAP".to_string(), argb: WHITE, size: (14.0 * s) as i32, align: 0 });
    prims.push(Prim::Text { x: x + side, y: y - 22.0 * s, text: format!("RELEASE  [{key}]"), argb: DIM, size: (11.0 * s) as i32, align: 2 });
    match host.radar(x, y, side, side, p.body.pos) {
        Some((px, py)) => {
            let (bw, bh) = (30.0 * s, 15.0 * s);
            prims.push(Prim::Image { name: sprites::PLAYER_BRACKET.to_string(), x: px - bw * 0.5, y: py - bh * 0.5, w: bw, h: bh, argb: 0xFFF2_C230, fallback: Vec::new() });
        }
        None => {
            prims.push(Prim::Text { x: x + side * 0.5, y: y + side * 0.45, text: "NO RADAR".to_string(), argb: WHITE, size: (20.0 * s) as i32, align: 1 });
            prims.push(Prim::Text { x: x + side * 0.5, y: y + side * 0.45 + 28.0 * s, text: "Your side has no radar to show the map".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 1 });
        }
    }
}
