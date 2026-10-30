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
//! * Centre: crosshair, hit marker, score popup ([`center`]); damage feedback ([`damage`]).
//! * Tab: the scoreboard ([`scoreboard`]); the full-map radar moved to M.
//!
//! Deliberate divergences: no scorestreaks (the column holds the field computer and the build
//! menu); the team is the player's Zero Hour army (its logo, radar colour and funds on the score
//! panel, team colours on the minimap). Without the converted BO2 art the HUD falls back to drawn
//! shapes and the engine font.

use crate::building::{self, work_text};
use crate::client::{Client, ViewMode};
use crate::host::{objflags, ClientHost, ObjInfo};
use crate::math::Vec3;
use crate::sim::{Mode, PlayerSim, Sim};

pub mod ammo;
pub mod center;
pub mod damage;
pub mod digit;
pub mod frame;
pub mod map;
mod panels;
pub mod rewards;
pub mod score;
pub mod scoreboard;
mod shapes;
pub mod sprites;
pub mod state;
#[cfg(test)]
mod tests;

use center::*;
pub use center::reticle_color_rgb;
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
pub(crate) const AMBER: u32 = 0xF0F2_B233;
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
        .chain(["damage_feedback", "hud_horizontal_compass_minimap"].iter())
        .copied()
}

/// The local player's army colour, from the General's own object (Zero Hour's radar colour).
fn team_color(host: &dyn ClientHost, p: &PlayerSim) -> u32 {
    let mut objs = Vec::new();
    host.query(p.body.pos, 2.0, p.general, &mut objs);
    objs.iter().find(|o| o.id == p.general).map_or(0xFFFF_FFFF, |o| o.color)
}

/// The scope's 2D screen-space ADS overlay (`FUN_004b51e0`). It draws while BO2's reticle zoom is
/// up ([`crate::weapons::scope::reticle_zoom`], in the zoom direction as updated this frame), white
/// at alpha `zoom / max(fOverlayAlphaScale, 1)`; over the same window the viewmodel is not drawn
/// (`HudLui::gun_under_overlay`, the native viewmodel). The material's own art carries the black
/// surround and the clear lens centre; a full-screen image needs no scissor of its own.
fn draw_scope_overlay(prims: &mut Vec<Prim>, w: f32, h: f32, p: &PlayerSim, zooming_in: bool) {
    let def = p.arsenal.current_weapon();
    let Some(zoom) = crate::weapons::scope::reticle_zoom(def, p.arsenal.ads, zooming_in) else { return };
    if def.overlay_material.is_empty() {
        return;
    }
    let a = (crate::weapons::scope::overlay_alpha(def, zoom).clamp(0.0, 1.0) * 255.0).round() as u32;
    // The converted MappedImage name. If the install lacks the art, the empty fallback draws
    // nothing (log-or-skip); the converter reports the missing material and the install-gated
    // test catches a sniper whose overlay did not convert.
    prims.push(Prim::Image {
        name: crate::lui::materials::image(&def.overlay_material),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: (a << 24) | (WHITE & 0x00FF_FFFF),
        fallback: Vec::new(),
    });
}

pub fn draw(host: &dyn ClientHost, prims: &mut Vec<Prim>, client: &Client, sim: &Sim, local_player: i32) {
    let player = sim.player(local_player);
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

    let Some(p) = player.filter(|_| in_fps) else {
        host.ghost(0, Vec3::ZERO, 0.0, false);
        return;
    };

    draw_low_health(prims, w, h, st.low_health.alpha(st.clock));
    draw_indicators(prims, w, h, client.yaw.to_degrees(), st);

    // Using the field computer, the tablet is up (zhbo_core::viewmodel): no crosshair.
    let tablet_up = matches!(p.mode, Mode::FieldComputer { .. });
    let map_open = client.hud.map_open;
    // BO2 hides its HUD under the scoreboard (`BIT_SCOREBOARD_OPEN` in every panel's visibility).
    if client.hud.scoreboard_open {
        let row = scoreboard::Row {
            name: "General".to_string(),
            values: [st.kills * crate::client::KILL_POINTS, st.kills, st.deaths, 0],
            you: true,
        };
        scoreboard::draw(&l, prims, team_color(host, p), &[row]);
        host.ghost(0, Vec3::ZERO, 0.0, false);
    } else {
        if !tablet_up && !map_open {
            draw_marks(host, prims, w, h, s, client, sim, p);
            // BO2's scope overlay: up while the reticle zoom is, when the viewmodel is not drawn.
            draw_scope_overlay(prims, w, h, p, st.ads_zoom_in);
            draw_crosshair(prims, w, h, s, client, p);
            draw_hitmarker(prims, w, h, client);
            draw_score_popup(&l, prims, client);
            draw_hack(prims, w, h, s, client);
            if p.hammer.is_none() && !p.building {
                draw_reload_prompt(prims, w, h, s, p);
            }
        }
        draw_build(host, prims, w, h, s, client, p, !tablet_up && !map_open);
        let reveal = |pos: Vec3| sim.reveals(local_player, pos);
        draw_minimap(host, prims, &l, client.yaw, p, &reveal);
        draw_compass(prims, &l, client.yaw);
        score::draw(&l, prims, team_color(host, p), host.cash(local_player).max(0) as u32);
        let can_build = !host.build_options(p.general).is_empty();
        rewards::draw(&l, prims, tablet_up, p.hammer.is_some(), can_build, st.weapon_ms());
        let alt_key = crate::input::key_name(client.cfg.bindings.alt_mode);
        ammo::draw(&l, prims, p, st.weapon_ms(), st.clock, alt_key);
        if map_open {
            draw_map(host, prims, w, h, s, p, crate::input::key_name(client.cfg.bindings.map));
        }
    }
    if client.class_pick_open {
        crate::classui::draw_screen(prims, w, h, &client.class_screen_view());
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
