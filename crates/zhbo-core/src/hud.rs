//! Black Ops 2-style HUD drawn with the engine's 2D primitives after the in-game UI.

use crate::client::{Client, ViewMode, HITMARKER_TIME};
use crate::host::ClientHost;
use crate::movement::Stance;
use crate::sim::{Mode, PlayerSim, FIELD_COMPUTER_RAISE};

const WHITE: u32 = 0xE6FF_FFFF;
const SHADOW: u32 = 0x9000_0000;
const RED: u32 = 0xF0FF_3030;
const AMBER: u32 = 0xF0FF_B020;
const BO2_ORANGE: u32 = 0xF0FF_7A1A;
const PANEL: u32 = 0x7010_1418;
const TERMINAL_GREEN: u32 = 0xF040_FF70;

pub fn draw(host: &dyn ClientHost, client: &Client, player: Option<&PlayerSim>) {
    let (w, h) = host.screen_size();
    let s = (h / 720.0).max(0.5);

    if client.hud.death_banner > 0.0 {
        draw_death_banner(host, w, h, s, client.hud.death_banner);
    }

    let in_fps = matches!(client.view, ViewMode::Fps);
    let Some(p) = player.filter(|_| in_fps) else { return };

    draw_damage_vignette(host, w, h, client.hud.damage_flash, p);

    if let Mode::FieldComputer { t } = p.mode {
        draw_field_computer(host, w, h, s, t / FIELD_COMPUTER_RAISE);
        return;
    }

    draw_crosshair(host, w, h, s, client, p);
    draw_hitmarker(host, w, h, s, client);
    draw_compass(host, w, s, client.yaw);
    draw_ammo(host, w, h, s, p);
    draw_status(host, h, s, p);
    host.text(w * 0.5, h - 30.0 * s, "[F] FIELD COMPUTER", 0xA0FF_FFFF, (11.0 * s) as i32, 1);
}

fn draw_crosshair(host: &dyn ClientHost, w: f32, h: f32, s: f32, client: &Client, p: &PlayerSim) {
    let (cx, cy) = (w * 0.5, h * 0.5);
    let ads = p.arsenal.ads;
    if ads > 0.6 {
        // Red-dot sight.
        host.fill_rect(cx - 2.0 * s, cy - 2.0 * s, 4.0 * s, 4.0 * s, RED);
        return;
    }
    if p.body.sprinting || p.arsenal.is_busy() {
        return;
    }
    let spread = p.arsenal.current().spread_deg(ads).to_radians();
    let half_fov = client.last_hfov * 0.5;
    let gap = (spread.tan() / half_fov.tan() * (w * 0.5)).max(4.0 * s);
    let len = 9.0 * s;
    let alpha = ((1.0 - ads / 0.6) * 230.0) as u32;
    let col = (alpha << 24) | 0x00FF_FFFF;
    let shadow = ((alpha / 2) << 24) & 0xFF00_0000;
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
    let rgb = if client.hud.hitmarker_kill { 0x00FF_3030 } else { 0x00FF_FFFF };
    let col = (a << 24) | rgb;
    let (inner, outer) = (5.0 * s, 12.0 * s);
    for (dx, dy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        host.line(cx + dx * inner, cy + dy * inner, cx + dx * outer, cy + dy * outer, 2.0 * s, col);
    }
}

fn draw_compass(host: &dyn ClientHost, w: f32, s: f32, yaw: f32) {
    // Zero Hour maps put north at +Y; compass heading is clockwise from north.
    let heading = (90.0 - yaw.to_degrees()).rem_euclid(360.0);
    let width = 360.0 * s;
    let x0 = w * 0.5 - width * 0.5;
    let y = 18.0 * s;
    host.fill_rect(x0, y - 2.0 * s, width, 22.0 * s, PANEL);
    let px_per_deg = width / 120.0; // 120 degrees visible
    for mark in (0..360).step_by(15) {
        let mut d = mark as f32 - heading;
        if d > 180.0 {
            d -= 360.0;
        }
        if d < -180.0 {
            d += 360.0;
        }
        if d.abs() > 60.0 {
            continue;
        }
        let x = w * 0.5 + d * px_per_deg;
        let label = match mark {
            0 => Some("N"),
            90 => Some("E"),
            180 => Some("S"),
            270 => Some("W"),
            _ => None,
        };
        match label {
            Some(l) => host.text(x, y, l, if mark == 0 { BO2_ORANGE } else { WHITE }, (12.0 * s) as i32, 1),
            None => host.line(x, y + 4.0 * s, x, y + 12.0 * s, 1.0, 0xA0FF_FFFF),
        }
    }
    host.line(w * 0.5, y + 18.0 * s, w * 0.5, y + 22.0 * s, 2.0 * s, BO2_ORANGE);
}

fn draw_ammo(host: &dyn ClientHost, w: f32, h: f32, s: f32, p: &PlayerSim) {
    let wpn = p.arsenal.current();
    let right = w - 28.0 * s;
    let base = h - 40.0 * s;
    host.fill_rect(right - 200.0 * s, base - 46.0 * s, 210.0 * s, 62.0 * s, PANEL);
    let mag_col = if wpn.ammo == 0 {
        RED
    } else if (wpn.ammo as f32) < wpn.def.mag_size as f32 * 0.25 {
        AMBER
    } else {
        WHITE
    };
    host.text(right - 70.0 * s, base - 42.0 * s, &wpn.ammo.to_string(), mag_col, (26.0 * s) as i32, 2);
    host.text(right, base - 34.0 * s, &format!("| {}", wpn.reserve), WHITE, (16.0 * s) as i32, 2);
    host.text(right, base - 4.0 * s, wpn.def.display_name, BO2_ORANGE, (12.0 * s) as i32, 2);
    if wpn.is_reloading() {
        host.text(w * 0.5, h * 0.5 + 40.0 * s, "RELOADING", WHITE, (14.0 * s) as i32, 1);
    } else if wpn.ammo == 0 && wpn.reserve == 0 {
        host.text(w * 0.5, h * 0.5 + 40.0 * s, "NO AMMO", RED, (14.0 * s) as i32, 1);
    } else if (wpn.ammo as f32) < wpn.def.mag_size as f32 * 0.25 {
        host.text(w * 0.5, h * 0.5 + 40.0 * s, "[R] RELOAD", AMBER, (12.0 * s) as i32, 1);
    }
}

fn draw_status(host: &dyn ClientHost, h: f32, s: f32, p: &PlayerSim) {
    let x = 28.0 * s;
    let base = h - 40.0 * s;
    host.fill_rect(x - 10.0 * s, base - 46.0 * s, 230.0 * s, 62.0 * s, PANEL);
    let frac = if p.max_health > 0.0 { (p.health / p.max_health).clamp(0.0, 1.0) } else { 0.0 };
    let bar_w = 200.0 * s;
    host.text(x, base - 42.0 * s, "GENERAL", BO2_ORANGE, (12.0 * s) as i32, 0);
    host.fill_rect(x, base - 22.0 * s, bar_w, 10.0 * s, SHADOW);
    let col = if frac < 0.35 { RED } else { WHITE };
    host.fill_rect(x, base - 22.0 * s, bar_w * frac, 10.0 * s, col);
    let stamina = p.body.sprint_stamina / crate::movement::MoveTuning::default().sprint_time;
    host.fill_rect(x, base - 8.0 * s, bar_w * stamina.clamp(0.0, 1.0), 3.0 * s, 0x90A0_D0FF);
    let stance = match p.body.stance {
        Stance::Stand => "STANDING",
        Stance::Crouch => "CROUCHED",
        Stance::Prone => "PRONE",
    };
    host.text(x + bar_w, base - 42.0 * s, stance, 0xA0FF_FFFF, (10.0 * s) as i32, 2);
}

fn draw_damage_vignette(host: &dyn ClientHost, w: f32, h: f32, flash: f32, p: &PlayerSim) {
    let low = if p.max_health > 0.0 { 1.0 - (p.health / p.max_health).clamp(0.0, 1.0) } else { 0.0 };
    let strength = flash.max(low * 0.8).min(1.0);
    if strength <= 0.02 {
        return;
    }
    let band = (h * 0.12).max(24.0);
    for i in 0..4 {
        let a = (strength * 110.0 * (1.0 - i as f32 / 4.0)) as u32;
        let col = (a << 24) | 0x00B0_0000;
        let o = band * i as f32 / 4.0;
        let t = band / 4.0;
        host.fill_rect(0.0, o, w, t, col);
        host.fill_rect(0.0, h - o - t, w, t, col);
        host.fill_rect(o, 0.0, t, h, col);
        host.fill_rect(w - o - t, 0.0, t, h, col);
    }
}

fn draw_field_computer(host: &dyn ClientHost, w: f32, h: f32, s: f32, progress: f32) {
    // The laptop slides up from the bottom of the screen, like BO2's scorestreak tablet.
    let p = crate::math::smoothstep(progress);
    let lw = w * 0.62;
    let lh = h * 0.62;
    let x = (w - lw) * 0.5;
    let y = h - p * (lh + h * 0.12);
    host.fill_rect(x - 14.0 * s, y - 14.0 * s, lw + 28.0 * s, lh + 28.0 * s, 0xF020_2226);
    host.fill_rect(x, y, lw, lh, 0xF0061008);
    host.text(x + 20.0 * s, y + 18.0 * s, "FIELD COMPUTER // COMMAND UPLINK", TERMINAL_GREEN, (14.0 * s) as i32, 0);
    let dots = ".".repeat(((progress * 12.0) as usize) % 4);
    host.text(x + 20.0 * s, y + 44.0 * s, &format!("ESTABLISHING TACTICAL VIEW{dots}"), TERMINAL_GREEN, (12.0 * s) as i32, 0);
    let bar = lw - 40.0 * s;
    host.fill_rect(x + 20.0 * s, y + lh - 40.0 * s, bar, 8.0 * s, 0x6040_FF70);
    host.fill_rect(x + 20.0 * s, y + lh - 40.0 * s, bar * progress.clamp(0.0, 1.0), 8.0 * s, TERMINAL_GREEN);
}

fn draw_death_banner(host: &dyn ClientHost, w: f32, h: f32, s: f32, t: f32) {
    let a = (t.min(1.0) * 230.0) as u32;
    host.fill_rect(0.0, h * 0.4, w, 60.0 * s, (a / 2) << 24);
    host.text(w * 0.5, h * 0.4 + 10.0 * s, "GENERAL DOWN", (a << 24) | 0x00FF_3030, (24.0 * s) as i32, 1);
    host.text(w * 0.5, h * 0.4 + 40.0 * s, "Build a new General at your Command Center", (a << 24) | 0x00FF_FFFF, (11.0 * s) as i32, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientIn;
    use crate::sim::tests::{general, FakeHost};
    use crate::sim::Sim;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Recorder {
        texts: RefCell<Vec<String>>,
        prims: RefCell<usize>,
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
    }

    #[test]
    fn hud_shows_ammo_and_weapon_in_first_person_only() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let rec = Recorder::default();

        draw(&rec, &client, None);
        assert!(rec.texts.borrow().is_empty(), "nothing in RTS mode");

        sim.take_control(&host, 0, 1);
        let mut inp = ClientIn::default();
        for f in 1..80 {
            client.frame(&mut sim, &inp);
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        draw(&rec, &client, sim.player(0));
        let texts = rec.texts.borrow();
        assert!(texts.iter().any(|t| t == "MTAR"));
        assert!(texts.iter().any(|t| t == "30"));
        assert!(texts.iter().any(|t| t == "N" || t == "E" || t == "S" || t == "W"));
        assert!(*rec.prims.borrow() > 4);
    }
}
