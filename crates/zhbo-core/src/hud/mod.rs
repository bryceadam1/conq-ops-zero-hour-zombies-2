//! Black Ops II multiplayer-style HUD, drawn after Zero Hour's in-game UI.
//!
//! Layout follows BO2's multiplayer HUD: the minimap top-left with the compass strip under it; the
//! team panel bottom-left (emblem and stacked numbers: here, funds), with the active scorestreak's
//! name above it; bottom-right, the scorestreak column (the field computer is the ready streak),
//! the weapon name in BO2's title box with its fire mode, and the ammo in BO2's slanted numerals. In
//! the middle, a four-line crosshair that opens with spread, four-stroke hit markers, a "+100"
//! kill popup, and blood at the screen edges when hurt. There is no health bar. While the tablet is
//! up the HUD stays, as in BO2, without the crosshair.
//!
//! Every piece is BO2's own art where BO2 has it, from `ConquerOps_BO2.big`, which the launcher builds
//! from the player's own install (`zhbo_assets::hud::SPRITES` and `zhbo_assets::cac::SPRITES`). The
//! exceptions are Zero Hour's: the
//! team emblem is the player's faction logo, and minimap dots carry each owner's team colour, as on
//! Zero Hour's radar. Without the BO2 art the HUD falls back to drawn shapes and the engine font.

use crate::building::{self, work_text};
use crate::client::{Client, ViewMode, HITMARKER_TIME};
use crate::host::{objflags, ClientHost, ObjInfo};
use crate::math::Vec3;
use crate::sim::{Mode, PlayerSim, Sim};

mod center;
mod damage;
mod map;
mod numerals;
mod panels;
mod shapes;
pub mod sprites;
mod team;
#[cfg(test)]
mod tests;

use center::*;
use damage::*;
use map::*;
pub(crate) use numerals::*;
use panels::*;
pub(crate) use shapes::*;
use team::*;

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
            Prim::ClipBegin { x, y, w, h } => host.clip(Some((*x, *y, *w, *h))),
            Prim::ClipEnd => host.clip(None),
        }
    }
}


pub(crate) const WHITE: u32 = 0xF2FF_FFFF;
pub(crate) const DIM: u32 = 0xA0FF_FFFF;
pub(crate) const RED: u32 = 0xF0E8_3A2E;
pub(crate) const AMBER: u32 = 0xF0F2_B233;
pub(super) const SCORE_YELLOW: u32 = 0xFFF5_D53A;
/// BO2's player arrow and ready-streak gold.
pub(crate) const GOLD: u32 = 0xFFF2_C230;
/// Panel fills and outlines.
pub(crate) const PANEL: u32 = 0x9010_1214;
pub(crate) const OUTLINE: u32 = 0xB0C8_CCD0;
/// Horizontal lean of BO2's slanted boxes, as a share of their height.
pub(super) const SLANT: f32 = 0.35;

/// BO2's sight attachments (a reticle on the line of sight).
pub(super) const OPTICS: &[&str] = &["reflex", "acog", "holo", "rangefinder", "dualoptic", "mms", "ir", "vzoom"];

/// Minimap radius in world units.
pub(super) const MINIMAP_RANGE: f32 = 320.0;

pub fn draw(host: &dyn ClientHost, prims: &mut Vec<Prim>, client: &Client, sim: &Sim, local_player: i32) {
    let player = sim.player(local_player);
    let (w, h) = host.screen_size();
    let s = (h / 720.0).max(0.5);

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

    let blood_art = host.image(sprites::BLOOD, 0.0, 0.0, 0.0, 0.0, 0);
        draw_damage(prims, w, h, client.hud.damage_flash, p, blood_art);

    // Using the field computer, the tablet is up (zhbo_core::viewmodel): no crosshair, and its
    // name shows bottom-left like a BO2 scorestreak in use.
    let tablet_up = matches!(p.mode, Mode::FieldComputer { .. });
    let map_open = client.hud.map_open;
    if !tablet_up && !map_open {
        draw_marks(host, prims, w, h, s, client, sim, p);
        draw_crosshair(prims, w, h, s, client, p);
        draw_hitmarker(prims, w, h, s, client);
        draw_score_popup(prims, w, h, s, client);
        draw_hack(prims, w, h, s, client);
    }
    draw_build(host, prims, w, h, s, client, p, !tablet_up && !map_open);
    let reveal = |pos: Vec3| sim.reveals(local_player, pos);
    let map_bottom = draw_minimap(host, prims, s, client.yaw, p, &reveal);
    draw_compass(prims, s, client.yaw, map_bottom);
    draw_team_panel(host, prims, h, s, local_player, tablet_up);
    draw_streaks(prims, w, h, s, tablet_up);
    draw_ammo(prims, w, h, s, p, !tablet_up && !map_open && p.hammer.is_none() && !p.building);
    draw_offhands(prims, w, h, s, p);
    if map_open {
        draw_map(host, prims, w, h, s, p);
    }
    if client.class_pick_open {
        crate::classui::draw_screen(prims, w, h, &client.class_screen_view());
    }
}

/// Held map key (Tab, BO2's scoreboard key): Zero Hour's own radar over the middle of the view,
/// with the whole map, shroud, team colours and radar events, and BO2's player bracket on you. Zero
/// Hour's rule applies: no radar, no map.
pub(super) fn draw_map(host: &dyn ClientHost, prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, p: &PlayerSim) {
    let side = (h * 0.74).min(w * 0.8);
    let (x, y) = ((w - side) * 0.5, (h - side) * 0.5 + 8.0 * s);
    prims.push(Prim::Rect { x: x - 6.0 * s, y: y - 30.0 * s, w: side + 12.0 * s, h: side + 36.0 * s, argb: 0xD010_1214 });
    outline_rect(prims, x - 6.0 * s, y - 30.0 * s, side + 12.0 * s, side + 36.0 * s, 1.5 * s, OUTLINE);
    prims.push(Prim::Text { x: x, y: y - 24.0 * s, text: "TACTICAL MAP".to_string(), argb: WHITE, size: (14.0 * s) as i32, align: 0 });
    prims.push(Prim::Text { x: x + side, y: y - 22.0 * s, text: "RELEASE  [TAB]".to_string(), argb: DIM, size: (11.0 * s) as i32, align: 2 });
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
