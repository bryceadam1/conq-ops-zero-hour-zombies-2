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
//! from the player's own install (`zhbo_assets::hud::SPRITES`). The exceptions are Zero Hour's: the
//! team emblem is the player's faction logo, and minimap dots carry each owner's team colour, as on
//! Zero Hour's radar. Without the BO2 art the HUD falls back to drawn shapes and the engine font.

use crate::building;
use crate::client::{Client, ViewMode, HITMARKER_TIME};
use crate::host::{objflags, ClientHost, ObjInfo};
use crate::math::Vec3;
use crate::sim::{Mode, PlayerSim, Sim};

/// MappedImage names produced by `zhbo_assets::hud`.
pub mod sprites {
    pub const DIGITS: [&str; 10] = [
        "ZHBO_BO2_Digit0",
        "ZHBO_BO2_Digit1",
        "ZHBO_BO2_Digit2",
        "ZHBO_BO2_Digit3",
        "ZHBO_BO2_Digit4",
        "ZHBO_BO2_Digit5",
        "ZHBO_BO2_Digit6",
        "ZHBO_BO2_Digit7",
        "ZHBO_BO2_Digit8",
        "ZHBO_BO2_Digit9",
    ];
    pub const SLASH: &str = "ZHBO_BO2_Slash";
    pub const BLOOD: &str = "ZHBO_BO2_Blood";
    pub const RED_DOT: &str = "ZHBO_BO2_RedDot";
    pub const HIT_MARKER: &str = "ZHBO_BO2_HitMarker";
    pub const PLAYER_ARROW: &str = "ZHBO_BO2_PlayerArrow";
    pub const PLAYER_BRACKET: &str = "ZHBO_BO2_PlayerBracket";
    pub const DOT: &str = "ZHBO_BO2_Dot";
    pub const COMPASS: &str = "ZHBO_BO2_Compass";
    pub const LEFT_LOWER: &str = "ZHBO_BO2_LeftLower";
    pub const LEFT_LOWER_BACK: &str = "ZHBO_BO2_LeftLowerBack";
    pub const FACTION_BACKING: &str = "ZHBO_BO2_FactionBacking";
    pub const FACTION_LIGHT: &str = "ZHBO_BO2_FactionLight";
    pub const RIGHT_LOWER: &str = "ZHBO_BO2_RightLower";
    pub const GUN_TITLE: [&str; 3] = ["ZHBO_BO2_GunTitleLeft", "ZHBO_BO2_GunTitleMid", "ZHBO_BO2_GunTitleRight"];
    pub const STREAK_BAR: &str = "ZHBO_BO2_StreakBar";
    pub const STREAK_BAR_BACK: &str = "ZHBO_BO2_StreakBarBack";
    pub const STREAK_BOX: &str = "ZHBO_BO2_StreakBox";
    pub const STREAK_BOX_LIT: &str = "ZHBO_BO2_StreakBoxLit";
    pub const STREAK_GLOW: &str = "ZHBO_BO2_StreakGlow";
    pub const STREAK_VSAT: &str = "ZHBO_BO2_StreakVsat";
    pub const ALL: [&str; 32] = [
        DIGITS[0], DIGITS[1], DIGITS[2], DIGITS[3], DIGITS[4], DIGITS[5], DIGITS[6], DIGITS[7], DIGITS[8],
        DIGITS[9], SLASH, BLOOD, RED_DOT, HIT_MARKER, PLAYER_ARROW, DOT, COMPASS, LEFT_LOWER, LEFT_LOWER_BACK,
        FACTION_BACKING, FACTION_LIGHT, RIGHT_LOWER, GUN_TITLE[0], GUN_TITLE[1], GUN_TITLE[2], STREAK_BAR,
        STREAK_BAR_BACK, STREAK_BOX, STREAK_BOX_LIT, STREAK_GLOW, STREAK_VSAT, PLAYER_BRACKET,
    ];
    /// Not BO2 art: Zero Hour's faction logo for the local player (the bridge resolves it).
    pub const TEAM_LOGO: &str = "ZHBO_TeamLogo";
}

const WHITE: u32 = 0xF2FF_FFFF;
const DIM: u32 = 0xA0FF_FFFF;
const RED: u32 = 0xF0E8_3A2E;
const AMBER: u32 = 0xF0F2_B233;
const SCORE_YELLOW: u32 = 0xFFF5_D53A;
/// BO2's player arrow and ready-streak gold.
const GOLD: u32 = 0xFFF2_C230;
/// Panel fills and outlines.
const PANEL: u32 = 0x9010_1214;
const OUTLINE: u32 = 0xB0C8_CCD0;
/// Horizontal lean of BO2's slanted boxes, as a share of their height.
const SLANT: f32 = 0.35;

/// BO2's sight attachments (a reticle on the line of sight).
const OPTICS: &[&str] = &["reflex", "acog", "holo", "rangefinder", "dualoptic", "mms", "ir", "vzoom"];

/// Minimap radius in world units.
const MINIMAP_RANGE: f32 = 320.0;

pub fn draw(host: &dyn ClientHost, client: &Client, sim: &Sim, local_player: i32) {
    let player = sim.player(local_player);
    let (w, h) = host.screen_size();
    let s = (h / 720.0).max(0.5);

    if client.hud.death_banner > 0.0 {
        draw_death_banner(host, w, h, s, client.hud.death_banner);
    }

    let in_fps = matches!(client.view, ViewMode::Fps);
    let Some(p) = player.filter(|_| in_fps) else {
        host.ghost(0, Vec3::ZERO, 0.0, false);
        return;
    };

    draw_damage(host, w, h, client.hud.damage_flash, p);

    // Using the field computer, the tablet is up (zhbo_core::viewmodel): no crosshair, and its
    // name shows bottom-left like a BO2 scorestreak in use.
    let tablet_up = matches!(p.mode, Mode::FieldComputer { .. });
    let map_open = client.hud.map_open;
    if !tablet_up && !map_open {
        draw_marks(host, w, h, s, client, sim, p);
        draw_crosshair(host, w, h, s, client, p);
        draw_hitmarker(host, w, h, s, client);
        draw_score_popup(host, w, h, s, client);
        draw_hack(host, w, h, s, client);
    }
    draw_build(host, w, h, s, client, p, !tablet_up && !map_open);
    let reveal = |pos: Vec3| sim.reveals(local_player, pos);
    let map_bottom = draw_minimap(host, s, client.yaw, p, &reveal);
    draw_compass(host, s, client.yaw, map_bottom);
    draw_team_panel(host, h, s, local_player, tablet_up);
    draw_streaks(host, w, h, s, tablet_up);
    draw_ammo(host, w, h, s, p, !tablet_up && !map_open && p.hammer.is_none() && !p.building);
    draw_offhands(host, w, h, s, p);
    if map_open {
        draw_map(host, w, h, s, p);
    }
}

/// Held map key (Tab, BO2's scoreboard key): Zero Hour's own radar over the middle of the view,
/// with the whole map, shroud, team colours and radar events, and BO2's player bracket on you. Zero
/// Hour's rule applies: no radar, no map.
fn draw_map(host: &dyn ClientHost, w: f32, h: f32, s: f32, p: &PlayerSim) {
    let side = (h * 0.74).min(w * 0.8);
    let (x, y) = ((w - side) * 0.5, (h - side) * 0.5 + 8.0 * s);
    host.fill_rect(x - 6.0 * s, y - 30.0 * s, side + 12.0 * s, side + 36.0 * s, 0xD010_1214);
    outline_rect(host, x - 6.0 * s, y - 30.0 * s, side + 12.0 * s, side + 36.0 * s, 1.5 * s, OUTLINE);
    host.text(x, y - 24.0 * s, "TACTICAL MAP", WHITE, (14.0 * s) as i32, 0);
    host.text(x + side, y - 22.0 * s, "RELEASE  [TAB]", DIM, (11.0 * s) as i32, 2);
    match host.radar(x, y, side, side, p.body.pos) {
        Some((px, py)) => {
            let (bw, bh) = (30.0 * s, 15.0 * s);
            host.image(sprites::PLAYER_BRACKET, px - bw * 0.5, py - bh * 0.5, bw, bh, 0xFFF2_C230);
        }
        None => {
            host.text(x + side * 0.5, y + side * 0.45, "NO RADAR", WHITE, (20.0 * s) as i32, 1);
            host.text(x + side * 0.5, y + side * 0.45 + 28.0 * s, "Your side has no radar to show the map", DIM, (12.0 * s) as i32, 1);
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Numerals
// ------------------------------------------------------------------------------------------------

/// Width of one BO2 numeral at height `height` (the art is 32x64, glyphs overlap slightly).
fn digit_advance(height: f32) -> f32 {
    height * 0.40
}

/// Draw a number in BO2 numerals, right-aligned at `right`. Returns the left edge.
fn draw_number(host: &dyn ClientHost, value: u32, right: f32, top: f32, height: f32, argb: u32) -> f32 {
    let text = value.to_string();
    let adv = digit_advance(height);
    let left = right - adv * text.len() as f32;
    let mut x = left;
    for ch in text.chars() {
        let d = ch.to_digit(10).unwrap_or(0) as usize;
        let cell_w = height * 0.5;
        if !host.image(sprites::DIGITS[d], x - (cell_w - adv) * 0.5, top, cell_w, height, argb) {
            // Fallback: the engine font, sized to roughly match.
            host.text(x + adv * 0.5, top + height * 0.05, &ch.to_string(), argb, (height * 0.75) as i32, 1);
        }
        x += adv;
    }
    left
}

// ------------------------------------------------------------------------------------------------
// Crosshair, hit marker, kill popup
// ------------------------------------------------------------------------------------------------

fn draw_crosshair(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client, p: &PlayerSim) {
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
        if !host.image(sprites::RED_DOT, cx - size * 0.5, cy - size * 0.5, size, size, 0xFFFF_FFFF) {
            host.fill_rect(cx - 2.0 * s, cy - 2.0 * s, 4.0 * s, 4.0 * s, RED);
        }
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
        host.line(x0 + 1.0, y0 + 1.0, x1 + 1.0, y1 + 1.0, 2.0 * s, shadow);
        host.line(x0, y0, x1, y1, 2.0 * s, col);
    }
}

fn draw_hitmarker(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client) {
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
    if host.image(sprites::HIT_MARKER, cx - size * 0.5, cy - size * 0.5, size, size * 2.0, col) {
        return;
    }
    let (inner, outer) = (6.0 * s, 14.0 * s);
    for (dx, dy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        host.line(cx + dx * inner, cy + dy * inner, cx + dx * outer, cy + dy * outer, 2.0 * s, col);
    }
}

/// BO2's yellow "+100" score popup above-right of the crosshair after a kill.
fn draw_score_popup(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client) {
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
    let left = draw_number(host, client.hud.score_points, right, top, height, col);
    // The plus sign.
    let (px, py, arm) = (left - 9.0 * s, top + height * 0.5, 5.0 * s);
    host.fill_rect(px - arm, py - 1.5 * s, arm * 2.0, 3.0 * s, col);
    host.fill_rect(px - 1.5 * s, py - arm, 3.0 * s, arm * 2.0, col);
}

// ------------------------------------------------------------------------------------------------
// Minimap and funds (left side)
// ------------------------------------------------------------------------------------------------

/// Rotating minimap: you at the centre facing up, friendlies blue, enemies red (your army acts as
/// a permanent UAV), structures as squares. Returns its bottom edge.
fn draw_minimap(host: &dyn ClientHost, s: f32, yaw: f32, p: &PlayerSim, reveal: &dyn Fn(Vec3) -> bool) -> f32 {
    let size = 150.0 * s;
    // BO2 keeps it tight in the corner; just below Zero Hour's message line (EVA notices).
    let (x0, y0) = (14.0 * s, 30.0 * s);
    let (cx, cy) = (x0 + size * 0.5, y0 + size * 0.5);
    host.fill_rect(x0, y0, size, size, 0xB02A_2E33);
    // BO2's thin light frame.
    outline_rect(host, x0, y0, size, size, 1.5 * s, OUTLINE);

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
                host.fill_rect(x - r, y - r, r * 2.0, r * 2.0, team | 0x9000_0000);
            }
        } else {
            // BO2's minimap dot, tinted.
            let r = if o.has(objflags::INFANTRY) { 3.5 * s } else { 4.5 * s };
            if inside(x, y, r) && !host.image(sprites::DOT, x - r, y - r, r * 2.0, r * 2.0, team | 0xFF00_0000) {
                host.fill_rect(x - r * 0.6, y - r * 0.6, r * 1.2, r * 1.2, team | 0xFF00_0000);
            }
        }
    }
    // You: BO2's player arrow, pointing up.
    let a = 18.0 * s;
    if !host.image(sprites::PLAYER_ARROW, cx - a * 0.5, cy - a * 0.5, a, a, 0xFFFF_FFFF) {
        let a = a * 0.4;
        let (tip, l, r) = ((cx, cy - a), (cx - a * 0.8, cy + a * 0.7), (cx + a * 0.8, cy + a * 0.7));
        for ((ax, ay), (bx, by)) in [(tip, l), (l, r), (r, tip)] {
            host.line(ax, ay, bx, by, 2.0 * s, GOLD);
        }
    }
    y0 + size
}

/// BO2's compass strip under the minimap (`hud_horizontal_compass_minimap`: a full turn, N at the
/// left), scrolled so the view heading is in the middle of the window.
fn draw_compass(host: &dyn ClientHost, s: f32, yaw: f32, top: f32) {
    let (x0, width, height) = (14.0 * s, 150.0 * s, 22.0 * s);
    let y0 = top + 2.0 * s;
    host.fill_rect(x0, y0, width, height, 0x8018_1A1C);
    let cx = x0 + width * 0.5;
    // Heading in compass degrees: yaw 0 is east (+x), north is +y.
    let heading = (90.0 - yaw.to_degrees()).rem_euclid(360.0);
    // The window shows half a turn; the art's N sits 8/512 in from its left edge.
    let strip = width * 2.0;
    let north_x = cx - heading / 360.0 * strip;
    let left = north_x - strip * 8.0 / 512.0;
    host.clip(Some((x0, y0, width, height)));
    let mut drew = false;
    for k in -1..=1 {
        drew |= host.image(sprites::COMPASS, left + k as f32 * strip, y0, strip, height, 0xFFFF_FFFF);
    }
    host.clip(None);
    if drew {
        return;
    }
    // Fallback: ticks and letters.
    let px_per_deg = width / 180.0;
    for i in ((heading - 90.0) / 15.0).ceil() as i32..=((heading + 90.0) / 15.0).floor() as i32 {
        let deg = i as f32 * 15.0;
        let x = cx + (deg - heading) * px_per_deg;
        match (deg.rem_euclid(360.0) as i32 + 360) % 360 {
            0 => host.text(x, y0 + 4.0 * s, "N", WHITE, (11.0 * s) as i32, 1),
            90 => host.text(x, y0 + 4.0 * s, "E", WHITE, (11.0 * s) as i32, 1),
            180 => host.text(x, y0 + 4.0 * s, "S", WHITE, (11.0 * s) as i32, 1),
            270 => host.text(x, y0 + 4.0 * s, "W", WHITE, (11.0 * s) as i32, 1),
            _ => host.line(x, y0 + 2.0 * s, x, y0 + 7.0 * s, 1.0, DIM),
        }
    }
}

// ------------------------------------------------------------------------------------------------
// BO2 shapes
// ------------------------------------------------------------------------------------------------

fn outline_rect(host: &dyn ClientHost, x: f32, y: f32, w: f32, h: f32, t: f32, argb: u32) {
    host.fill_rect(x, y, w, t, argb);
    host.fill_rect(x, y + h - t, w, t, argb);
    host.fill_rect(x, y, t, h, argb);
    host.fill_rect(x + w - t, y, t, h, argb);
}

/// BO2's slanted box: a parallelogram leaning right, `x` its bottom-left corner's left edge.
fn slant_box(host: &dyn ClientHost, x: f32, y: f32, w: f32, h: f32, fill: u32, edge: Option<u32>) {
    let lean = h * SLANT;
    let rows = h.ceil().max(1.0) as i32;
    for i in 0..rows {
        let f = i as f32 / rows as f32;
        host.fill_rect(x + lean * (1.0 - f), y + i as f32, w, 1.0, fill);
    }
    if let Some(c) = edge {
        let (tl, tr, bl, br) = ((x + lean, y), (x + lean + w, y), (x, y + h), (x + w, y + h));
        for ((ax, ay), (bx, by)) in [(tl, tr), (bl, br), (tl, bl), (tr, br)] {
            host.line(ax, ay, bx, by, 1.5, c);
        }
    }
}

/// BO2's team panel, bottom-left: its frame, the emblem plate, and the team's numbers. The emblem is
/// Zero Hour's logo for the player's faction, and the number is the player's funds. Above it, BO2
/// names the scorestreak in use.
fn draw_team_panel(host: &dyn ClientHost, h: f32, s: f32, local_player: i32, tablet_up: bool) {
    let (fw, fh) = (176.0 * s, 88.0 * s);
    let (x, top) = (8.0 * s, h - fh - 4.0 * s);
    let framed = host.image(sprites::LEFT_LOWER_BACK, x, top, fw, fh, 0xC0FF_FFFF);
    host.image(sprites::LEFT_LOWER, x, top, fw, fh, 0xFFFF_FFFF);
    if !framed {
        slant_box(host, x + 10.0 * s, top + 20.0 * s, fw - 20.0 * s, fh - 26.0 * s, PANEL, None);
    }
    // The emblem plate and the faction logo on it.
    let plate = 62.0 * s;
    let (px, py) = (x + 14.0 * s, top + fh - plate - 4.0 * s);
    host.image(sprites::FACTION_LIGHT, px, py, plate, plate, 0x90FF_FFFF);
    host.image(sprites::FACTION_BACKING, px, py, plate, plate, 0xFFFF_FFFF);
    let logo = plate * 0.68;
    host.image(sprites::TEAM_LOGO, px + (plate - logo) * 0.5, py + (plate - logo) * 0.5, logo, logo, 0xFFFF_FFFF);

    // Funds in BO2's numerals on the frame's upper line, labelled under it.
    let cash = host.cash(local_player).max(0) as u32;
    let right = x + fw - 14.0 * s;
    let height = 30.0 * s;
    let num_top = top + 22.0 * s;
    let left = draw_number(host, cash, right, num_top, height, WHITE);
    host.text(left - 9.0 * s, num_top + height * 0.22, "$", WHITE, (16.0 * s) as i32, 2);
    host.text(right, num_top + height + 4.0 * s, "FUNDS", DIM, (10.0 * s) as i32, 2);

    // The scorestreak in use, BO2's big bold name with its status under it.
    if tablet_up {
        host.text(x + 6.0 * s, top - 50.0 * s, "FIELD COMPUTER", WHITE, (30.0 * s) as i32, 0);
        host.text(x + 8.0 * s, top - 12.0 * s, "Establishing tactical view", WHITE, (12.0 * s) as i32, 0);
    }
}

// ------------------------------------------------------------------------------------------------
// Scorestreaks (bottom right, above the weapon)
// ------------------------------------------------------------------------------------------------

/// BO2's scorestreak column. Conquer Ops' one streak is the field computer (BO2's orbital VSAT icon),
/// always ready, so its box is lit; its name sits beside it like BO2's selected streak, with the key.
fn draw_streaks(host: &dyn ClientHost, w: f32, h: f32, s: f32, tablet_up: bool) {
    // The box art is a wide slanted box across the middle half of a square image.
    let b = 64.0 * s;
    let x = w - 14.0 * s - b;
    let y = h - 206.0 * s;
    // The bar to the left of the column.
    let (bw, bh) = (24.0 * s, 48.0 * s);
    let bx = x - bw + 6.0 * s;
    let by = y + (b - bh) * 0.5;
    host.image(sprites::STREAK_BAR_BACK, bx, by, bw, bh, 0xFFFF_FFFF);
    if !host.image(sprites::STREAK_BAR, bx, by, bw, bh, 0xFFFF_FFFF) {
        host.fill_rect(bx + bw * 0.5, y, 2.0 * s, b, 0x90FF_FFFF);
    }
    let glow = b * 1.1;
    host.image(sprites::STREAK_GLOW, x + (b - glow) * 0.5, y + (b - glow) * 0.5, glow, glow, if tablet_up { 0xC0FF_FFFF } else { 0x70FF_FFFF });
    let lit = host.image(sprites::STREAK_BOX_LIT, x, y, b, b, 0xFFFF_FFFF);
    let icon = b * 0.46;
    host.image(sprites::STREAK_VSAT, x + (b - icon) * 0.5, y + (b - icon) * 0.5, icon, icon, 0xFFFF_FFFF);
    host.image(sprites::STREAK_BOX, x, y, b, b, 0xFFFF_FFFF);
    if !lit {
        slant_box(host, x, y + b * 0.2, b, b * 0.6, 0xB0A8_8228, Some(GOLD));
    }
    host.text(bx - 4.0 * s, y + b * 0.5 - 7.0 * s, "FIELD COMPUTER  [F]", WHITE, (12.0 * s) as i32, 2);
}

/// The Black Hat's progress over the crosshair.
fn draw_hack(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client) {
    if client.hud.hack_shown <= 0.0 {
        return;
    }
    let (bw, bh) = (180.0 * s, 8.0 * s);
    let (x, y) = (w * 0.5 - bw * 0.5, h * 0.5 + 60.0 * s);
    host.fill_rect(x, y, bw, bh, PANEL);
    host.fill_rect(x, y, bw * client.hud.hack_progress.clamp(0.0, 1.0), bh, 0xF020_D040);
    outline_rect(host, x, y, bw, bh, 1.0 * s, OUTLINE);
    host.text(w * 0.5, y - 18.0 * s, "HACKING", WHITE, (12.0 * s) as i32, 1);
}

/// Building: with the hammer up, the build menu (right of centre) and the placement ghost; at a
/// site, BO2 Zombies' "Hold F" prompt and, while building, its progress.
fn draw_build(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client, p: &PlayerSim, show: bool) {
    let options = if p.hammer.is_some() || p.site.is_some() { host.build_options(p.general) } else { Vec::new() };
    let name_of = |template: u32| options.iter().find(|o| o.template == template).map_or("structure".to_string(), |o| o.name());
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
                host.fill_rect(x, top, pw, row * (options.len() as f32 + 2.6), PANEL);
                host.text(x + 10.0 * s, top + 3.0 * s, "BUILD", GOLD, size, 0);
                host.text(x + pw - 10.0 * s, top + 3.0 * s, "[B] put away", DIM, (11.0 * s) as i32, 2);
                for (i, o) in options.iter().enumerate() {
                    let y = top + row * (i as f32 + 1.2);
                    if i == choice {
                        outline_rect(host, x + 4.0 * s, y - 1.0 * s, pw - 8.0 * s, row, 1.0 * s, GOLD);
                    }
                    let col = match o.status {
                        building::status::OK => WHITE,
                        building::status::NO_MONEY => RED,
                        _ => DIM,
                    };
                    host.text(x + 10.0 * s, y + 2.0 * s, &o.name(), col, size, 0);
                    host.text(x + pw - 10.0 * s, y + 2.0 * s, &format!("${}", o.cost), col, size, 2);
                }
                let y = top + row * (options.len() as f32 + 1.4);
                host.text(x + pw * 0.5, y, "[Wheel] choose   [Fire] place", DIM, (11.0 * s) as i32, 1);
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
            host.fill_rect(x, y, bw, bh, PANEL);
            host.fill_rect(x, y, bw * (client.hud.build_percent / 100.0).clamp(0.0, 1.0), bh, 0xF0F2_C230);
            outline_rect(host, x, y, bw, bh, 1.0 * s, OUTLINE);
            let label = format!("BUILDING {}  {:.0}%", name_of(site.template).to_uppercase(), client.hud.build_percent);
            host.text(w * 0.5, y - 18.0 * s, &label, WHITE, (12.0 * s) as i32, 1);
        } else {
            let text = format!("Hold [F] to build {} ({:.0}%)", name_of(site.template), site.percent);
            host.text(w * 0.5, h * 0.5 + 60.0 * s, &text, WHITE, size, 1);
        }
    }
    if client.hud.build_note_time > 0.0 {
        host.text(w * 0.5, h * 0.5 + 84.0 * s, client.hud.build_note, WHITE, size, 1);
    }
}

/// Where a world point is on screen, if it is in front of the camera.
fn project(client: &Client, w: f32, h: f32, at: Vec3) -> Option<(f32, f32)> {
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
fn draw_marks(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client, sim: &Sim, p: &PlayerSim) {
    let def = p.arsenal.current().def;
    let finder = def.target_finder && p.arsenal.ads > 0.6;
    let engineer = p.arsenal.has(crate::weapons::Perk::Engineer);
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
                host.line(ax, ay, bx, by, 2.0 * s, col);
            }
        }
    }
}

fn draw_ammo(host: &dyn ClientHost, w: f32, h: f32, s: f32, p: &PlayerSim, prompts: bool) {
    let wpn = p.arsenal.current();
    // BO2's weapon panel frame, bottom-right.
    let (fw, fh) = (176.0 * s, 88.0 * s);
    let (fx, ftop) = (w - fw - 8.0 * s, h - fh - 4.0 * s);
    host.image(sprites::RIGHT_LOWER, fx, ftop, fw, fh, 0xFFFF_FFFF);

    let right = w - 24.0 * s;
    let base = h - 16.0 * s;
    // BO2 draws the magazine and the reserve nearly the same size.
    let big = 38.0 * s;
    let small = 32.0 * s;
    let low = (wpn.ammo as f32) < wpn.def.clip as f32 * 0.3;
    let mag_col = if wpn.ammo == 0 { RED } else if low { AMBER } else { WHITE };
    let reserve_left = draw_number(host, wpn.reserve as u32, right, base - small, small, WHITE);
    let slash_w = small * 0.5;
    if !host.image(sprites::SLASH, reserve_left - slash_w - 1.0 * s, base - small, slash_w, small, WHITE) {
        host.text(reserve_left - slash_w * 0.5, base - small, "/", WHITE, (small * 0.75) as i32, 1);
    }
    draw_number(host, wpn.ammo as u32, reserve_left - slash_w - 3.0 * s, base - big, big, mag_col);

    // The weapon's name in BO2's title box (its box is rows 22-40 of the 64-pixel art), with the
    // fire mode beside it.
    let mode = wpn.def.fire_mode_name();
    let row = ftop + 4.0 * s; // the box's top
    host.text(right, row + 1.0 * s, mode, WHITE, (12.0 * s) as i32, 2);
    let art = 20.0 * s / (18.0 / 64.0); // art height for a 20-pixel box
    let art_top = row - art * 22.0 / 64.0;
    // The box stretches for longer names (an alt mode's "Grenade Launcher").
    let text_w = (wpn.def.display_name.chars().count() as f32 * 7.2).max(44.0) * s;
    let box_right = right - 82.0 * s;
    let mid_left = box_right - art * 0.5 - text_w;
    let titled = host.image(sprites::GUN_TITLE[1], mid_left, art_top, text_w, art, 0xFFFF_FFFF);
    host.image(sprites::GUN_TITLE[0], mid_left - art, art_top, art, art, 0xFFFF_FFFF);
    host.image(sprites::GUN_TITLE[2], mid_left + text_w, art_top, art, art, 0xFFFF_FFFF);
    if !titled {
        slant_box(host, mid_left - 10.0 * s, row, text_w + 20.0 * s, 20.0 * s, 0x8030_3438, Some(OUTLINE));
    }
    host.text(mid_left + text_w * 0.5, row + 3.0 * s, &wpn.def.display_name, WHITE, (13.0 * s) as i32, 1);

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
        host.text(w * 0.5, h * 0.5 + 46.0 * s, text, col, (13.0 * s) as i32, 1);
    }
}

/// BO2's lethal and tactical counters: each offhand's own HUD icon with how many are left, in the
/// weapon panel to the left of the ammo.
fn draw_offhands(host: &dyn ClientHost, w: f32, h: f32, s: f32, p: &PlayerSim) {
    let size = 20.0 * s;
    let y = h - 34.0 * s;
    let mut x = w - 182.0 * s;
    for slot in [&p.arsenal.lethal, &p.arsenal.tactical] {
        let col = if slot.count == 0 { 0x60FF_FFFF } else { 0xFFFF_FFFF };
        if !host.image(&format!("ZHBO_BO2_Icon_{}", slot.def.id), x, y, size, size, col) {
            // No art: BO2's short name.
            let short: String = slot.def.display_name.chars().take(3).collect();
            host.text(x + size * 0.5, y + 3.0 * s, &short, col, (10.0 * s) as i32, 1);
        }
        host.text(x + size + 2.0 * s, y + 6.0 * s, &slot.count.to_string(), col, (11.0 * s) as i32, 0);
        x += size + 18.0 * s;
    }
}

// ------------------------------------------------------------------------------------------------
// Damage, field computer, death
// ------------------------------------------------------------------------------------------------

fn draw_damage(host: &dyn ClientHost, w: f32, h: f32, flash: f32, p: &PlayerSim) {
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
    let mut drew = false;
    for (x, y) in corners {
        drew |= host.image(sprites::BLOOD, x, y, size, size, tint);
    }
    let band = (h * 0.10).max(20.0);
    let edge = if drew { 70.0 } else { 120.0 };
    for i in 0..4 {
        let ea = (strength * edge * (1.0 - i as f32 / 4.0)) as u32;
        let col = (ea << 24) | 0x0090_0000;
        let o = band * i as f32 / 4.0;
        let t = band / 4.0;
        host.fill_rect(0.0, o, w, t, col);
        host.fill_rect(0.0, h - o - t, w, t, col);
        host.fill_rect(o, 0.0, t, h, col);
        host.fill_rect(w - o - t, 0.0, t, h, col);
    }
}

fn draw_death_banner(host: &dyn ClientHost, w: f32, h: f32, s: f32, t: f32) {
    let a = (t.min(1.0) * 230.0) as u32;
    host.fill_rect(0.0, h * 0.4, w, 64.0 * s, (a / 2) << 24);
    host.text(w * 0.5, h * 0.4 + 10.0 * s, "GENERAL DOWN", (a << 24) | (RED & 0x00FF_FFFF), (24.0 * s) as i32, 1);
    host.text(w * 0.5, h * 0.4 + 42.0 * s, "Build a new General at your Command Center", (a << 24) | 0x00FF_FFFF, (12.0 * s) as i32, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientIn;
    use crate::sim::tests::{general, soldier, FakeHost};
    use crate::sim::Sim;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Recorder {
        texts: RefCell<Vec<String>>,
        images: RefCell<Vec<String>>,
        prims: RefCell<usize>,
        has_art: bool,
        objects: Vec<ObjInfo>,
    }

    impl ClientHost for Recorder {
        fn screen_size(&self) -> (f32, f32) {
            (1280.0, 720.0)
        }
        fn fill_rect(&self, _x: f32, _y: f32, _w: f32, _h: f32, _argb: u32) {
            *self.prims.borrow_mut() += 1;
        }
        fn line(&self, _x0: f32, _y0: f32, _x1: f32, _y1: f32, _width: f32, _argb: u32) {
            *self.prims.borrow_mut() += 1;
        }
        fn text(&self, _x: f32, _y: f32, text: &str, _argb: u32, _size: i32, _align: i32) {
            self.texts.borrow_mut().push(text.to_string());
        }
        fn image(&self, name: &str, _x: f32, _y: f32, _w: f32, _h: f32, _argb: u32) -> bool {
            self.images.borrow_mut().push(name.to_string());
            self.has_art
        }
        fn query(&self, _c: Vec3, _r: f32, _v: u32, out: &mut Vec<ObjInfo>) {
            out.extend_from_slice(&self.objects);
        }
        fn cash(&self, _player: i32) -> i32 {
            10_000
        }
    }

    fn fps_state() -> (Sim, Client) {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        sim.take_control(&host, 0, 1);
        let mut inp = ClientIn::default();
        for f in 1..160 {
            client.frame(&mut sim, &inp);
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        (sim, client)
    }

    #[test]
    fn uses_bo2_numerals_when_the_art_is_installed() {
        let (sim, client) = fps_state();
        let rec = Recorder { has_art: true, objects: vec![soldier(5, 1, 40.0)], ..Default::default() };
        draw(&rec, &client, &sim, 0);
        let images = rec.images.borrow();
        assert!(images.contains(&"ZHBO_BO2_Digit3".to_string()), "30 rounds -> '3' numeral");
        assert!(images.contains(&"ZHBO_BO2_Digit0".to_string()));
        assert!(images.contains(&sprites::SLASH.to_string()));
        let texts = rec.texts.borrow();
        assert!(texts.iter().any(|t| t == "AN-94"));
        assert!(texts.iter().any(|t| t == "FULL-AUTO"), "BO2 fire mode beside the weapon name");
        assert!(texts.iter().any(|t| t == "FUNDS") && images.contains(&"ZHBO_BO2_Digit1".to_string()), "funds in BO2 numerals");
        assert!(texts.iter().any(|t| t.starts_with("FIELD COMPUTER")), "the ready scorestreak");
        for art in [sprites::COMPASS, sprites::PLAYER_ARROW, sprites::DOT, sprites::LEFT_LOWER, sprites::RIGHT_LOWER, sprites::STREAK_VSAT, sprites::GUN_TITLE[1]] {
            assert!(images.contains(&art.to_string()), "BO2 art {art}");
        }
        assert!(images.contains(&sprites::TEAM_LOGO.to_string()), "Zero Hour's faction logo");
        assert!(!texts.iter().any(|t| t == "3" || t == "0"), "no font fallback when art exists");
    }

    #[test]
    fn map_key_shows_the_radar_or_says_there_is_none() {
        let (sim, mut client) = fps_state();
        client.hud.map_open = true;
        let rec = Recorder { has_art: true, ..Default::default() };
        draw(&rec, &client, &sim, 0);
        let texts = rec.texts.borrow();
        assert!(texts.iter().any(|t| t == "TACTICAL MAP"));
        assert!(texts.iter().any(|t| t == "NO RADAR"), "the fake host has no radar");
    }

    #[test]
    fn falls_back_to_text_without_the_art() {
        let (sim, client) = fps_state();
        let rec = Recorder::default();
        draw(&rec, &client, &sim, 0);
        let texts = rec.texts.borrow();
        assert!(texts.iter().any(|t| t == "3") && texts.iter().any(|t| t == "0"));
    }

    #[test]
    fn nothing_in_rts_view() {
        let rec = Recorder::default();
        draw(&rec, &Client::default(), &Sim::new(), 0);
        assert!(rec.texts.borrow().is_empty() && *rec.prims.borrow() == 0);
    }
}
