//! Black Ops II multiplayer-style HUD, drawn after Zero Hour's in-game UI.
//!
//! Layout follows BO2: minimap top-left, score/funds bottom-left, weapon name and ammo in BO2's
//! slanted numerals bottom-right, a four-line crosshair that opens with spread, four-stroke hit
//! markers, a "+100" kill popup, and blood at the screen edges when hurt. There is no health bar.
//!
//! BO2 art comes from `ConquerOps_BO2.big`, which the launcher builds from the player's own install
//! (`zhbo_assets::hud::SPRITES`). Every sprite has a drawn fallback, so the HUD works without it.

use crate::client::{Client, ViewMode, HITMARKER_TIME};
use crate::host::{objflags, ClientHost, ObjInfo};
use crate::math::Vec3;
use crate::sim::{Mode, PlayerSim, FIELD_COMPUTER_RAISE};

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
    pub const BRACKET: &str = "ZHBO_BO2_Bracket";
    pub const ALL: [&str; 14] = [
        DIGITS[0], DIGITS[1], DIGITS[2], DIGITS[3], DIGITS[4], DIGITS[5], DIGITS[6], DIGITS[7], DIGITS[8],
        DIGITS[9], SLASH, BLOOD, RED_DOT, BRACKET,
    ];
}

const WHITE: u32 = 0xF2FF_FFFF;
const DIM: u32 = 0xA0FF_FFFF;
const RED: u32 = 0xF0E8_3A2E;
const AMBER: u32 = 0xF0F2_B233;
const SCORE_YELLOW: u32 = 0xFFF5_D53A;
const FRIEND: u32 = 0xF04F_C3F7;
const ENEMY: u32 = 0xF0E8_3A2E;

/// Minimap radius in world units.
const MINIMAP_RANGE: f32 = 320.0;

pub fn draw(host: &dyn ClientHost, client: &Client, player: Option<&PlayerSim>, local_player: i32) {
    let (w, h) = host.screen_size();
    let s = (h / 720.0).max(0.5);

    if client.hud.death_banner > 0.0 {
        draw_death_banner(host, w, h, s, client.hud.death_banner);
    }

    let in_fps = matches!(client.view, ViewMode::Fps);
    let Some(p) = player.filter(|_| in_fps) else { return };

    draw_damage(host, w, h, client.hud.damage_flash, p);

    if let Mode::FieldComputer { t } = p.mode {
        draw_field_computer(host, w, h, s, t / FIELD_COMPUTER_RAISE);
        return;
    }

    draw_crosshair(host, w, h, s, client, p);
    draw_hitmarker(host, w, h, s, client);
    draw_score_popup(host, w, h, s, client);
    draw_minimap(host, s, client.yaw, p);
    draw_funds(host, h, s, local_player);
    draw_ammo(host, w, h, s, p);
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
    if ads > 0.6 {
        let size = 22.0 * s;
        if !host.image(sprites::RED_DOT, cx - size * 0.5, cy - size * 0.5, size, size, 0xFFFF_FFFF) {
            host.fill_rect(cx - 2.0 * s, cy - 2.0 * s, 4.0 * s, 4.0 * s, RED);
        }
        return;
    }
    if p.body.sprinting || p.arsenal.is_busy() {
        return;
    }
    let spread = p.arsenal.current().spread_deg(ads).to_radians();
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
/// a permanent UAV), structures as squares.
fn draw_minimap(host: &dyn ClientHost, s: f32, yaw: f32, p: &PlayerSim) {
    let size = 176.0 * s;
    // Below Zero Hour's message line (EVA notices print at the top left).
    let (x0, y0) = (22.0 * s, 52.0 * s);
    let (cx, cy) = (x0 + size * 0.5, y0 + size * 0.5);
    host.fill_rect(x0, y0, size, size, 0x9010_1418);
    // Hairline frame with BO2's slightly brighter corners.
    for (ax, ay, bx, by) in [(x0, y0, x0 + size, y0), (x0, y0 + size, x0 + size, y0 + size), (x0, y0, x0, y0 + size), (x0 + size, y0, x0 + size, y0 + size)] {
        host.line(ax, ay, bx, by, 1.0, 0x50FF_FFFF);
    }
    let corner = 10.0 * s;
    for (x, y, dx, dy) in [(x0, y0, 1.0, 1.0), (x0 + size, y0, -1.0, 1.0), (x0, y0 + size, 1.0, -1.0), (x0 + size, y0 + size, -1.0, -1.0)] {
        host.line(x, y, x + dx * corner, y, 2.0 * s, DIM);
        host.line(x, y, x, y + dy * corner, 2.0 * s, DIM);
    }

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
        let (x, y) = to_screen(o.pos);
        let col = if o.has(objflags::ENEMY) { ENEMY } else if o.has(objflags::ALLY) { FRIEND } else { 0x90C8_C8C8 };
        if o.has(objflags::STRUCTURE) {
            let r = (o.major_radius * scale).clamp(3.0 * s, 14.0 * s);
            if inside(x, y, r) {
                host.fill_rect(x - r, y - r, r * 2.0, r * 2.0, (col & 0x00FF_FFFF) | 0x6000_0000);
            }
        } else {
            let r = if o.has(objflags::INFANTRY) { 2.0 * s } else { 3.0 * s };
            if inside(x, y, r) {
                host.fill_rect(x - r, y - r, r * 2.0, r * 2.0, col);
            }
        }
    }
    // You: a white chevron pointing up.
    let a = 7.0 * s;
    host.line(cx, cy - a, cx - a * 0.7, cy + a * 0.7, 2.0 * s, WHITE);
    host.line(cx, cy - a, cx + a * 0.7, cy + a * 0.7, 2.0 * s, WHITE);
    // North marker on the rim.
    let (nx, ny) = to_screen(p.body.pos + Vec3::new(0.0, MINIMAP_RANGE * 0.88, 0.0));
    if inside(nx, ny, 6.0 * s) {
        host.text(nx, ny - 7.0 * s, "N", DIM, (11.0 * s) as i32, 1);
    }
}

fn draw_funds(host: &dyn ClientHost, h: f32, s: f32, local_player: i32) {
    let (x, y) = (22.0 * s, h - 64.0 * s);
    host.fill_rect(x, y, 176.0 * s, 40.0 * s, 0x7010_1418);
    host.fill_rect(x, y, 3.0 * s, 40.0 * s, FRIEND);
    host.text(x + 12.0 * s, y + 3.0 * s, "FUNDS", DIM, (9.0 * s) as i32, 0);
    let cash = host.cash(local_player).max(0) as u32;
    host.text(x + 12.0 * s, y + 14.0 * s, &format!("$ {cash}"), WHITE, (17.0 * s) as i32, 0);
}

// ------------------------------------------------------------------------------------------------
// Ammo (bottom right)
// ------------------------------------------------------------------------------------------------

fn draw_ammo(host: &dyn ClientHost, w: f32, h: f32, s: f32, p: &PlayerSim) {
    let wpn = p.arsenal.current();
    let right = w - 34.0 * s;
    let base = h - 34.0 * s;
    let big = 58.0 * s;
    let small = 28.0 * s;

    let low = (wpn.ammo as f32) < wpn.def.mag_size as f32 * 0.3;
    let mag_col = if wpn.ammo == 0 { RED } else if low { AMBER } else { WHITE };

    // Reserve (small numerals) at the far right, magazine (big) to its left, BO2 order.
    let reserve_left = draw_number(host, wpn.reserve as u32, right, base - small, small, DIM);
    let slash_w = small * 0.5;
    if !host.image(sprites::SLASH, reserve_left - slash_w - 2.0 * s, base - small, slash_w, small, DIM) {
        host.text(reserve_left - slash_w * 0.5, base - small, "/", DIM, (small * 0.75) as i32, 1);
    }
    let mag_left = draw_number(host, wpn.ammo as u32, reserve_left - slash_w - 6.0 * s, base - big, big, mag_col);

    // Weapon name above, and a hairline under the whole block.
    host.text(right, base - big - 22.0 * s, wpn.def.display_name, WHITE, (13.0 * s) as i32, 2);
    host.fill_rect(mag_left, base + 5.0 * s, right - mag_left, 1.5 * s, 0x60FF_FFFF);

    // Field computer, shown like a ready scorestreak above the weapon name.
    host.text(right, base - big - 46.0 * s, "[F]  FIELD COMPUTER", 0xC0F2_B233, (11.0 * s) as i32, 2);

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

fn draw_field_computer(host: &dyn ClientHost, w: f32, h: f32, s: f32, progress: f32) {
    // A rugged tablet slides up from the bottom, like BO2's scorestreak remote.
    let p = crate::math::smoothstep(progress);
    let lw = w * 0.56;
    let lh = h * 0.6;
    let x = (w - lw) * 0.5;
    let y = h - p * (lh + h * 0.14);
    host.fill_rect(x - 18.0 * s, y - 18.0 * s, lw + 36.0 * s, lh + 36.0 * s, 0xF21B_1D1F);
    host.fill_rect(x - 18.0 * s, y - 18.0 * s, lw + 36.0 * s, 4.0 * s, 0xF23A_3E42);
    host.fill_rect(x, y, lw, lh, 0xF206_0B0E);
    let green = 0xF05C_FFB0;
    host.text(x + 18.0 * s, y + 14.0 * s, "FIELD COMPUTER", green, (14.0 * s) as i32, 0);
    host.text(x + lw - 18.0 * s, y + 14.0 * s, "SECURE LINK", 0x905C_FFB0, (11.0 * s) as i32, 2);
    let b = lh * 0.42;
    if !host.image(sprites::BRACKET, x + (lw - b) * 0.5, y + (lh - b) * 0.5, b, b, 0xFFFF_FFFF) {
        host.line(x + lw * 0.4, y + lh * 0.5, x + lw * 0.6, y + lh * 0.5, 2.0 * s, green);
    }
    let dots = ".".repeat(((progress * 12.0) as usize) % 4);
    host.text(x + lw * 0.5, y + lh * 0.5 + b * 0.6, &format!("Establishing tactical view{dots}"), green, (12.0 * s) as i32, 1);
    let bar = lw - 36.0 * s;
    host.fill_rect(x + 18.0 * s, y + lh - 34.0 * s, bar, 6.0 * s, 0x605C_FFB0);
    host.fill_rect(x + 18.0 * s, y + lh - 34.0 * s, bar * progress.clamp(0.0, 1.0), 6.0 * s, green);
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
        for f in 1..80 {
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
        draw(&rec, &client, sim.player(0), 0);
        let images = rec.images.borrow();
        assert!(images.contains(&"ZHBO_BO2_Digit3".to_string()), "30 rounds -> '3' numeral");
        assert!(images.contains(&"ZHBO_BO2_Digit0".to_string()));
        assert!(images.contains(&sprites::SLASH.to_string()));
        let texts = rec.texts.borrow();
        assert!(texts.iter().any(|t| t == "MTAR"));
        assert!(texts.iter().any(|t| t == "$ 10000"));
        assert!(!texts.iter().any(|t| t == "3" || t == "0"), "no font fallback when art exists");
    }

    #[test]
    fn falls_back_to_text_without_the_art() {
        let (sim, client) = fps_state();
        let rec = Recorder::default();
        draw(&rec, &client, sim.player(0), 0);
        let texts = rec.texts.borrow();
        assert!(texts.iter().any(|t| t == "3") && texts.iter().any(|t| t == "0"));
    }

    #[test]
    fn nothing_in_rts_view() {
        let rec = Recorder::default();
        draw(&rec, &Client::default(), None, 0);
        assert!(rec.texts.borrow().is_empty() && *rec.prims.borrow() == 0);
    }
}
