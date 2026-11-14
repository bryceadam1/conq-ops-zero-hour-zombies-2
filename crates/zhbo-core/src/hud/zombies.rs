//! The Zombies HUD a survivor General wears (`patch_zm/ui_mp/t6/zombie/*.lua`): the round's chalk
//! bottom-left (`hudroundstatuszombie.lua`) and the d-pad area bottom-right
//! (`huddpadareazombie.lua`): BO2's blood streak, the d-pad circle and the action slots on its
//! four arms (`UpdateActionSlots`: 1 up, 2 down, 3 left, 4 right).
//!
//! On the circle, as TranZit has it (`hud.lua` builds `AmmoAreaZombie` only for the DLC 2-4 maps;
//! TranZit gets the d-pad area): the ammo counter (`hud/ammocounter.lua`), the weapon label
//! (`hud/weaponlabel.lua`) and the offhand icons (`hud/offhandicons.lua`); above it the points
//! list (`hudcompetitivescoreboardzombie.lua`), the points being the player's money.
//!
//! Deliberate divergence: the field computer is no action slot (user's ruling); its key prompt
//! sits under the circle. Not ported yet: the round chalk's animations (the first round's fade
//! and fall, a new round's pulse) and the points list's flying points.

use super::frame::Lui;
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};
use crate::weapons::actionslot::{ActionSlot, Equipment};

/// `CoD.RoundStatus`: the chalk's size and top (bottom-anchored), its left offset (no faction
/// icon outside Grief, `FactionIconLeftOffset`) and colour (`DefaultColor`).
pub const CHALK_SIZE: f32 = 96.0;
pub const CHALK_TOP: f32 = -96.0;
pub const CHALK_LEFT: f32 = 0.0;
pub const CHALK_COLOR: (f32, f32, f32) = (0.21, 0.0, 0.0);
/// The tally marks for one to five (`hud_chalk_1..5`).
pub const CHALKS: [&str; 5] = ["hud_chalk_1", "hud_chalk_2", "hud_chalk_3", "hud_chalk_4", "hud_chalk_5"];

/// `CoD.DPadArea`: the circle's size, the blood streak's box and its tint, the arrow.
pub const CIRCLE_SIZE: f32 = 128.0;
pub const LINES_WIDTH: f32 = 300.0;
pub const LINES_HEIGHT: f32 = 150.0;
/// The streak's bottom below the circle's (`L3_2 = 10`).
pub const LINES_DROP: f32 = 10.0;
pub const LINES_COLOR: (f32, f32, f32) = (0.21, 0.0, 0.0);
pub const BLOOD: &str = "hud_dpad_blood";
pub const CIRCLE: &str = "hud_lui_dpad_circle";

/// `CoD.CompetitiveScoreboard`: a row's size, the list's bottom (not a DLC map) and its bar.
pub const ROW_WIDTH: f32 = 150.0;
pub const ROW_HEIGHT: f32 = 30.0;
pub const ROWS_BOTTOM: f32 = -90.0;
pub const SCORE_BAR: &str = "scorebar_zom_1";
/// `CoD.Zombie.PlayerColors` (`basezombie.lua`), by player.
pub const PLAYER_COLORS: [(f32, f32, f32); 4] = [(1.0, 1.0, 1.0), (0.486, 0.812, 0.933), (0.965, 0.792, 0.314), (0.514, 0.925, 0.533)];
/// `CoD.AmmoCounter`: the counter's box on the circle (left/top anchored), its text height, the
/// low-clip pulse's half period (`LowAmmoFadeTime`) and alpha (`pulse_high`).
pub const AMMO_BOX: (f32, f32, f32, f32) = (-90.0, 10.0, 36.0, 76.0);
pub const AMMO_LOW_FADE_MS: f32 = 500.0;
pub const AMMO_LOW_ALPHA: f32 = 0.5;
/// The weapon label's box on the circle (right/bottom anchored, `huddpadareazombie.lua`), its
/// text height and fade (`CoD.WeaponLabel`).
pub const LABEL_BOX: (f32, f32, f32, f32) = (-231.0, -131.0, -120.0, -88.0);
pub const LABEL_FADE_MS: f32 = 2000.0;
/// `CoD.OffhandIcons`: an icon's side and a slot's width; the lethal's slot ends 1 in from the
/// circle's bottom-right, the tactical's beside it.
pub const OFFHAND_SIZE: f32 = 24.0;
pub const OFFHAND_WIDTH: f32 = OFFHAND_SIZE * 1.5;

/// The field computer's key prompt (no BO2 counterpart): its right edge left of the circle, clear of
/// the ammo counter and weapon label above it, and its text height.
pub const FIELD_COMPUTER_LABEL: (f32, f32) = (-CIRCLE_SIZE - 8.0, 16.0);

/// The last stand's HUD (`_zm_laststand.gsc`, in hud-element units of the 640 × 480 screen):
/// the reviver's progress bar (`createprimaryprogressbar` with the last stand's `primaryprogressbar*`,
/// `:44-47`: 120 × 4, centred 110 below the screen's centre, white `progress_bar_fill` over a
/// `progress_bar_bg` 4 wider and taller in black at half alpha, `_hud_util.gsc` `createbar`), his
/// `ZOMBIE_REVIVING` text (`revive_do_revive`: centred, 113 up from the bottom, `fontscale` 1.8) and
/// the message line (`revive_hud_create`: 160 up from the bottom, `fontscale` 1.5), a font scale
/// being `level.fontheight` 12 units tall (`_hud.gsc:20`).
pub const BAR_480: (f32, f32, f32) = (120.0, 4.0, 110.0);
pub const REVIVING_480: (f32, f32) = (-113.0, 1.8);
pub const MESSAGE_480: (f32, f32) = (-160.0, 1.5);
pub const FONT_HEIGHT_480: f32 = 12.0;
/// The messages' fade (`faderevivemessageover`, `revive_hud_show_n_fade`: 3 s).
pub const MESSAGE_FADE: f32 = 3.0;
pub const BAR_FILL: &str = "progress_bar_fill";
pub const BAR_BACK: &str = "progress_bar_bg";
/// The downed player's icon: the engine's own (t6zm `FUN_004d5e80` loads `waypoint_revive_zm` in
/// Zombies), 30 inches over him as the script's revive icons stand (`_zm_chugabud.gsc:718-725`).
/// Its drawn size is not traced; a fixed `REVIVE_ICON` units.
pub const REVIVE_ICON_MAT: &str = "waypoint_revive_zm";
pub const REVIVE_ICON_HEIGHT: f32 = 30.0;
pub const REVIVE_ICON: f32 = 32.0;

/// `CoD.PowerUps` (`hudpowerupszombie.lua`): the timed power-ups' icons, in its client field order,
/// `IconSize` square, `Spacing` apart, centred at the bottom of the safe area (`UpdatePosition`).
pub const POWERUP_ICON: f32 = 48.0;
pub const POWERUP_SPACING: f32 = 8.0;
pub const POWERUP_ICONS: [(&str, &str); 2] = [("insta_kill", "specialty_instakill_zombies"), ("double_points", "specialty_doublepoints_zombies")];
/// The nuke's white flash (`nuke_flash`: to 0.8 over 0.2 s, held until 0.5 s, gone over 1 s).
pub const NUKE_FLASH: (f32, f32, f32, f32) = (0.8, 0.2, 0.5, 1.0);
/// Max ammo's text (`full_ammo_on_hud`: font scale 2, its top `zombie_timer_offset` less two
/// intervals down, 290 of 480; in over 0.5 s, then up to 270 and out over 1.5 s).
pub const MAX_AMMO_TEXT_480: (f32, f32, f32) = (290.0, 270.0, 2.0);
pub const MAX_AMMO_TIMES: (f32, f32) = (0.5, 1.5);

/// The materials this module draws (for `lui::materials::MATERIALS`).
pub const MATERIALS: [&str; 13] = [CHALKS[0], CHALKS[1], CHALKS[2], CHALKS[3], CHALKS[4], BLOOD, CIRCLE, SCORE_BAR, BAR_FILL, BAR_BACK, REVIVE_ICON_MAT, POWERUP_ICONS[0].1, POWERUP_ICONS[1].1];

/// The chalk a round shows at rest (`UpdateRoundsPlayed`/`StartNewRound`): one to five its tally
/// alone; six to ten a full five and the rest; past ten the number in Morris.
pub fn chalk(round: u32) -> (Vec<&'static str>, Option<String>) {
    match round {
        0 => (Vec::new(), None),
        1..=5 => (vec![CHALKS[round as usize - 1]], None),
        6..=10 => (vec![CHALKS[4], CHALKS[round as usize - 6]], None),
        _ => (Vec::new(), Some(round.to_string())),
    }
}

/// The round's chalk, bottom-left: each tally 96 square side by side from the left (`roundChalk1`,
/// `roundChalk2`), or the number in the first's box.
pub fn draw_round(l: &Lui, prims: &mut Vec<Prim>, round: u32) {
    let (tallies, number) = chalk(round);
    for (k, material) in tallies.iter().enumerate() {
        let left = CHALK_LEFT + k as f32 * CHALK_SIZE;
        let r = Place::new((true, false, left, left + CHALK_SIZE), (false, true, CHALK_TOP, CHALK_TOP + CHALK_SIZE)).resolve(l.screen());
        l.image(prims, r, material, CHALK_COLOR, 1.0, Vec::new());
    }
    if let Some(n) = number {
        let r = Place::new((true, false, CHALK_LEFT, CHALK_LEFT + CHALK_SIZE), (false, true, CHALK_TOP, CHALK_TOP + CHALK_SIZE)).resolve(l.screen());
        l.text(prims, r, Family::Morris, &n, Some(Align::Left), (true, false), CHALK_COLOR, 1.0);
    }
}

/// The circle's box, bottom-right.
pub fn circle_rect(l: &Lui) -> Rect {
    Place::new((false, true, -CIRCLE_SIZE, 0.0), (false, true, -CIRCLE_SIZE, 0.0)).resolve(l.screen())
}

/// Action slot `n`'s (1-4) icon box in the circle (`UpdateActionSlots`): `CircleSize / 4` tall,
/// `aspect` times as wide, a quarter of the circle in from its arm's edge.
pub fn slot_rect(circle: Rect, n: usize, aspect: f32) -> Rect {
    let h = CIRCLE_SIZE / 4.0;
    let w = h * aspect;
    let q = CIRCLE_SIZE / 4.0;
    let place = match n {
        1 => Place::new((false, false, -w / 2.0, w / 2.0), (true, false, q - h / 2.0, q + h / 2.0)),
        2 => Place::new((false, false, -w / 2.0, w / 2.0), (false, true, -q - h / 2.0, -q + h / 2.0)),
        3 => Place::new((true, false, q - w / 2.0, q + w / 2.0), (false, false, -h / 2.0, h / 2.0)),
        _ => Place::new((false, true, -q - w / 2.0, -q + w / 2.0), (false, false, -h / 2.0, h / 2.0)),
    };
    place.resolve(circle)
}

/// One player on the points list: his points and his colour's index (`PLAYER_COLORS`), and
/// whether he is the local player (only his row shows the bar, `CompetitiveScoreShowSelf`).
pub struct Points {
    pub points: i32,
    pub color: usize,
    pub own: bool,
}

/// The points list (`CompetitiveScoreboard`): rows of `ROW_WIDTH` by `ROW_HEIGHT` up from
/// `ROWS_BOTTOM` at the right, the first player at the top (`Update`: score k goes to row n - k + 1),
/// each score left in its row past a 10-unit inset in its player's colour, the local player's row
/// on the bar tinted (0.21, 0, 0).
pub fn draw_points(l: &Lui, prims: &mut Vec<Prim>, players: &[Points]) {
    let n = players.len();
    for (k, p) in players.iter().enumerate() {
        let row = n - k;
        let bottom = ROWS_BOTTOM - ROW_HEIGHT * row as f32;
        let r = Place::new((false, true, -ROW_WIDTH, 0.0), (false, true, bottom - ROW_HEIGHT, bottom)).resolve(l.screen());
        if p.own {
            l.image(prims, r, SCORE_BAR, (0.21, 0.0, 0.0), 1.0, Vec::new());
        }
        let text = Place::new((true, false, 10.0, ROW_WIDTH), (true, true, 0.0, 0.0)).resolve(r);
        l.text(prims, text, Family::Default, &p.points.to_string(), Some(Align::Left), (true, false), PLAYER_COLORS[p.color % PLAYER_COLORS.len()], 1.0);
    }
}

/// The low-clip pulse (`Ammo_PulseHigh`/`Ammo_PulseLow`): the text fades to half and back, each
/// way `LowAmmoFadeTime`.
pub fn ammo_pulse(clock: f32) -> f32 {
    let half = AMMO_LOW_FADE_MS / 1000.0;
    let t = (clock / half).rem_euclid(2.0);
    let k = if t < 1.0 { t } else { 2.0 - t };
    1.0 - (1.0 - AMMO_LOW_ALPHA) * k
}

/// The ammo counter (`AmmoCounter.UpdateAmmo`): `clip/stock`, a dual-wield's left clip before
/// it (`left | clip/stock`), right-aligned in Big in its box on the circle, pulsing on a low clip.
/// Hidden for a melee weapon (`ShouldHideAmmoCounter`).
pub fn draw_ammo(l: &Lui, prims: &mut Vec<Prim>, p: &PlayerSim, clock: f32) {
    let w = p.arsenal.current();
    if w.def.class == crate::weapons::Class::Melee || p.arsenal.is_unarmed() {
        return;
    }
    let mut text = format!("{}/{}", w.ammo, w.reserve);
    if let Some(left) = w.left_ammo() {
        text = format!("{left} | {text}");
    }
    let low = super::ammo::low_clip(w.ammo, w.def) || w.left_ammo().is_some_and(|n| super::ammo::low_clip(n, w.def));
    let alpha = if low { ammo_pulse(clock) } else { 1.0 };
    let (left, right, top, bottom) = AMMO_BOX;
    let b = Place::new((true, false, left, right), (true, false, top, bottom)).resolve(circle_rect(l));
    let r = Place::new((false, true, -1.0, 0.0), (true, true, -4.0, 4.0)).resolve(b);
    let r = Rect { x: b.x, w: r.right() - b.x, ..r };
    l.text(prims, r, Family::Big, &text, Some(Align::Right), (false, true), super::frame::HUD_BASE, alpha);
}

/// The weapon label (`WeaponLabel.UpdateWeapon`): the weapon's name right-aligned in Big beside the
/// circle after a weapon change, fading out over `FadeTime`.
pub fn draw_weapon_label(l: &Lui, prims: &mut Vec<Prim>, p: &PlayerSim, since_ms: f32) {
    let alpha = 1.0 - (since_ms / LABEL_FADE_MS).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let (left, right, top, bottom) = LABEL_BOX;
    let r = Place::new((false, true, left, right), (false, true, top, bottom)).resolve(circle_rect(l));
    l.text(prims, r, Family::Big, &p.arsenal.current().def.display_name, Some(Align::Right), (false, true), super::frame::HUD_BASE, alpha);
}

/// The offhand icons (`OffhandIcons.SetMaterialAndQuantity`): one `OFFHAND_SIZE` icon per grenade,
/// right to left from its slot's right edge, the k-th of n at alpha k/n; the lethal's slot ends 1
/// in from the circle's bottom-right, the tactical's left of it.
pub fn draw_offhands(l: &Lui, prims: &mut Vec<Prim>, p: &PlayerSim) {
    let circle = circle_rect(l);
    for (k, slot) in [&p.arsenal.lethal, &p.arsenal.tactical].into_iter().enumerate() {
        let n = slot.count as usize;
        if n == 0 {
            continue;
        }
        let right = -1.0 - OFFHAND_WIDTH * k as f32;
        let area = Place::new((false, true, right - OFFHAND_WIDTH, right), (false, true, -1.0 - OFFHAND_SIZE, -1.0)).resolve(circle);
        let icon = slot.def.hud_icon_image();
        for i in 0..n {
            let x = -(i as f32) * OFFHAND_SIZE;
            let r = Place::new((false, true, x - OFFHAND_SIZE, x), (false, true, -OFFHAND_SIZE, 0.0)).resolve(area);
            let px = l.px(r);
            let alpha = (n - i) as f32 / n as f32;
            prims.push(Prim::Image { name: icon.clone(), x: px.x, y: px.y, w: px.w, h: px.h, argb: super::frame::argb(super::frame::HUD_BASE, alpha), fallback: Vec::new() });
        }
    }
}

/// The last stand as survivor `p` sees it: each downed survivor's icon over him (`project` places a
/// world point on screen, in pixels), the hint over one he may revive, his revive's bar and text,
/// and the message line: a fresh down's `ZOMBIE_PLAYER_NEEDS_TO_BE_REVIVED`, or his reviver's
/// `ZOMBIE_PLAYER_IS_REVIVING_YOU`, each fading over `MESSAGE_FADE`.
pub fn draw_last_stand(l: &Lui, prims: &mut Vec<Prim>, sim: &Sim, p: &PlayerSim, use_key: &str, name: &dyn Fn(i32) -> String, project: &dyn Fn(crate::math::Vec3) -> Option<(f32, f32)>) {
    let ls = crate::zombies::LastStand::get();
    let size = l.px(Rect { x: 0.0, y: 0.0, w: REVIVE_ICON, h: REVIVE_ICON }).w;
    for d in sim.rounds.downed.iter().filter(|d| d.general != p.general) {
        let Some((x, y)) = project(d.pos + crate::math::Vec3::new(0.0, 0.0, crate::units::inches(REVIVE_ICON_HEIGHT))) else { continue };
        prims.push(Prim::Image { name: crate::lui::materials::image(REVIVE_ICON_MAT), x: x - size * 0.5, y: y - size * 0.5, w: size, h: size, argb: super::frame::argb((1.0, 1.0, 1.0), 1.0), fallback: Vec::new() });
    }
    let scale = |v: f32| v * 1.5;
    let line = |prims: &mut Vec<Prim>, bottom_480: f32, fontscale: f32, text: &str, alpha: f32| {
        let h = scale(FONT_HEIGHT_480 * fontscale);
        let y = scale(bottom_480);
        let r = Place::new((true, true, 0.0, 0.0), (false, true, y - h * 0.5, y + h * 0.5)).resolve(l.screen());
        l.text(prims, r, Family::Default, text, Some(Align::Center), (true, true), (1.0, 1.0, 1.0), alpha);
    };
    if let Some(d) = sim.rounds.downed.iter().find(|d| d.revived_by == p.general) {
        let (w, h, below) = BAR_480;
        let (w, h, below) = (scale(w), scale(h), scale(below));
        let bar = Place::new((false, false, -w * 0.5, w * 0.5), (false, false, below - h * 0.5, below + h * 0.5)).resolve(l.screen());
        let back = Rect { x: bar.x - scale(2.0), y: bar.y - scale(2.0), w: bar.w + scale(4.0), h: bar.h + scale(4.0) };
        l.image(prims, back, BAR_BACK, (0.0, 0.0, 0.0), 0.5, Vec::new());
        let frac = (d.revive / ls.revive_time.max(1e-3)).clamp(0.0, 1.0);
        l.image(prims, Rect { w: bar.w * frac, ..bar }, BAR_FILL, (1.0, 1.0, 1.0), 1.0, Vec::new());
        let (bottom, fontscale) = REVIVING_480;
        line(prims, bottom, fontscale, crate::sim::hint_text("ZOMBIE_REVIVING"), 1.0);
    } else if sim.revive_target(p).is_some() {
        super::pickup::draw_runs(l, prims, &super::pickup::coloured_runs(crate::sim::hint_text("ZOMBIE_BUTTON_TO_REVIVE_PLAYER"), use_key));
    }
    let (bottom, fontscale) = MESSAGE_480;
    if let Some(d) = sim.rounds.downed.iter().find(|d| d.general == p.general && d.revived_by != 0) {
        // `revive_hud_show_n_fade( 3.0 )` as the revive starts.
        let alpha = 1.0 - d.revive / MESSAGE_FADE;
        if alpha > 0.0 {
            let reviver = sim.seat_of(d.revived_by).map_or_else(String::new, |s| name(s as i32));
            line(prims, bottom, fontscale, &crate::sim::hint_text("ZOMBIE_PLAYER_IS_REVIVING_YOU").replace("&&1", &reviver), alpha);
        }
    } else if !sim.is_downed(p.general) {
        // `revive_hud_think`: the latest down, to every survivor up, fading as it shows.
        if let Some(d) = sim.rounds.downed.iter().rev().find(|d| d.general != p.general) {
            let alpha = 1.0 - (ls.bleedout_time - d.bleed) / MESSAGE_FADE;
            if alpha > 0.0 {
                line(prims, bottom, fontscale, &crate::sim::hint_text("ZOMBIE_PLAYER_NEEDS_TO_BE_REVIVED").replace("&&1", &name(d.player)), alpha);
            }
        }
    }
}

/// `set_clientfield_powerups`: a timed power-up's icon state with `left` seconds of it (1 on, 2 off
/// and 3 on while flashing), from `powerup_hud_monitor`'s table: from 10 s down, steps of 0.2 s
/// (0.1 s under 5), the off steps 0.05 s longer.
pub fn powerup_state(left: f32) -> u8 {
    if left >= 10.0 {
        return 1;
    }
    let mut table = Vec::new();
    let (mut timer, mut on) = (10.0f32, false);
    while timer >= 0.15 {
        let delta = if timer < 5.0 { 0.1 } else { 0.2 };
        if on {
            timer -= delta + 0.05;
            table.push((timer, 2u8));
        } else {
            timer -= delta;
            table.push((timer, 3u8));
        }
        on = !on;
    }
    for &(t, v) in table.iter().skip(1).rev() {
        if left < t {
            return v;
        }
    }
    3
}

/// The power-ups on the HUD: each timed one running (its icon, flashing near its end), the
/// nuke's flash and max ammo's text.
pub fn draw_powerups(l: &Lui, prims: &mut Vec<Prim>, sim: &Sim, clock_frames: f32) {
    let s = &sim.rounds.powerups;
    let active: Vec<(&str, f32)> = POWERUP_ICONS
        .iter()
        .filter_map(|&(name, mat)| {
            let left = match name {
                "insta_kill" => s.insta_kill,
                _ => s.double_points,
            };
            (left > 0.0).then_some((mat, left))
        })
        .collect();
    let n = active.len() as f32;
    let mut x = -(POWERUP_ICON * 0.5 * n + POWERUP_SPACING * 0.5 * (n - 1.0));
    for (mat, left) in active {
        let r = Place::new((false, false, x, x + POWERUP_ICON), (false, true, -POWERUP_ICON, 0.0)).resolve(l.screen());
        let alpha = if matches!(powerup_state(left), 1 | 3) { 1.0 } else { 0.0 };
        if alpha > 0.0 {
            l.image(prims, r, mat, (1.0, 1.0, 1.0), alpha, Vec::new());
        }
        x += POWERUP_ICON + POWERUP_SPACING;
    }
    let since = |frame: u32| (frame != 0).then_some((clock_frames - frame as f32) * crate::units::LOGIC_DT).filter(|e| *e >= 0.0);
    if let Some(e) = since(s.nuke_frame) {
        let (peak, up, hold, down) = NUKE_FLASH;
        let alpha = if e < up {
            peak * e / up
        } else if e < hold {
            peak
        } else {
            peak * (1.0 - (e - hold) / down)
        };
        if alpha > 0.0 {
            let (w, h) = (l.px(l.screen()).w, l.px(l.screen()).h);
            prims.push(Prim::Rect { x: 0.0, y: 0.0, w: w * 2.0, h: h * 2.0, argb: super::frame::argb((1.0, 1.0, 1.0), alpha) });
        }
    }
    if let Some(e) = since(s.max_ammo_frame) {
        let (top, rise_to, fontscale) = MAX_AMMO_TEXT_480;
        let (fade_in, move_out) = MAX_AMMO_TIMES;
        let (y, alpha) = if e < fade_in { (top, e / fade_in) } else { let k = ((e - fade_in) / move_out).min(1.0); (top + (rise_to - top) * k, 1.0 - k) };
        if alpha > 0.0 {
            let h = FONT_HEIGHT_480 * fontscale * 1.5;
            let r = Place::new((true, true, 0.0, 0.0), (true, false, y * 1.5, y * 1.5 + h)).resolve(l.screen());
            l.text(prims, r, Family::Condensed, crate::sim::hint_text("ZOMBIE_POWERUP_MAX_AMMO"), Some(Align::Center), (true, true), (1.0, 1.0, 1.0), alpha);
        }
    }
}

/// A Zombies trigger's prompt from its localized text: `[{+activate}]` is the use key and `&&1`
/// the price (`sethintstring( hint, cost )`), as coloured runs.
pub fn trigger_prompt(text: &str, use_key: &str, cost: u32) -> Vec<(String, (f32, f32, f32))> {
    let text = text.replace("[{+activate}]", use_key).replace("&&1", &cost.to_string());
    super::pickup::coloured_runs(&text, "")
}

/// The wall buy in `p`'s use list: its prompt (`wall_weapon_update_prompt`): the weapon's own hint
/// with its price without it, `ZOMBIE_WEAPONAMMOONLY` with the ammo price with it, nothing for a
/// melee weapon he has.
pub fn draw_wall_buy(l: &Lui, prims: &mut Vec<Prim>, sim: &Sim, p: &PlayerSim, use_key: &str) {
    let Some(i) = sim.wall_buy_target(p) else { return };
    let Some(b) = crate::zombies::WallBuys::get().buys.get(sim.rounds.wall_buys[i].index as usize) else { return };
    let (key, cost) = match crate::sim::wallbuy::offer(p, b) {
        crate::sim::wallbuy::Offer::Weapon { cost } => (b.hint().unwrap_or_default(), cost),
        crate::sim::wallbuy::Offer::Ammo { cost } => ("ZOMBIE_WEAPONAMMOONLY".to_string(), cost),
        crate::sim::wallbuy::Offer::Nothing => return,
    };
    super::pickup::draw_runs(l, prims, &trigger_prompt(crate::sim::hint_text(&key), use_key, cost));
}

/// What one slot shows: its icon (a MappedImage), the name drawn where the icon has no art, whether
/// it can be used, and its key.
pub struct Slot<'a> {
    pub icon: Option<&'a str>,
    pub label: String,
    pub usable: bool,
    pub key: &'a str,
}

/// The d-pad area: the streak, the circle, each filled slot's icon (full alpha when usable, BO2's
/// `HUDAlphaEmpty` else) with its key beside it, and the field computer's key under the circle.
pub fn draw_dpad(l: &Lui, prims: &mut Vec<Prim>, slots: &[Slot; 4], field_computer_key: &str) {
    let circle = circle_rect(l);
    let lines = Place::new((false, true, -LINES_WIDTH, 0.0), (false, true, LINES_DROP - LINES_HEIGHT, LINES_DROP)).resolve(l.screen());
    l.image(prims, lines, BLOOD, LINES_COLOR, 1.0, Vec::new());
    l.image(prims, circle, CIRCLE, super::frame::HUD_BASE, 1.0, Vec::new());
    for (k, slot) in slots.iter().enumerate() {
        let Some(icon) = slot.icon else { continue };
        let r = slot_rect(circle, k + 1, 1.0);
        let alpha = if slot.usable { super::frame::HUD_ALPHA_FULL } else { super::frame::HUD_ALPHA_EMPTY };
        let p = l.px(r);
        let argb = super::frame::argb(super::frame::HUD_BASE, alpha);
        let fallback = vec![Prim::Text { x: p.x + p.w * 0.5, y: p.y + p.h * 0.35, text: slot.label.clone(), argb, size: (p.h * 0.35) as i32, align: 1 }];
        prims.push(Prim::Image { name: icon.to_string(), x: p.x, y: p.y, w: p.w, h: p.h, argb, fallback });
        if !slot.key.is_empty() {
            let key = Place::new((true, true, 0.0, 0.0), (false, true, -10.0, 0.0)).resolve(r);
            l.text(prims, key, Family::Default, slot.key, Some(Align::Center), (true, true), super::frame::HUD_BASE, alpha);
        }
    }
    if !field_computer_key.is_empty() {
        let (right, height) = FIELD_COMPUTER_LABEL;
        let r = Place::new((false, true, right - 300.0, right), (false, true, -2.0 - height, -2.0)).resolve(circle);
        let label = format!("[{field_computer_key}] {}", crate::strings::text("HUD:ZHBO_FieldComputer"));
        l.text(prims, r, Family::Default, &label, Some(Align::Right), (false, true), super::frame::HUD_BASE, super::frame::HUD_ALPHA_FULL);
    }
}

/// The four slots of a survivor `p` as the d-pad shows them: the build tool's icon (usable while
/// he can build), the alt mode's (its weapon's `dpadIcon`, BO2's slot 3).
pub fn slots<'a>(p: &'a PlayerSim, can_build: bool, keys: [&'a str; 4]) -> [Slot<'a>; 4] {
    let build = crate::strings::text("HUD:ZHBO_Build");
    let alt = super::ammo::alt_slot_icon(p.arsenal.current().def, crate::weapons::catalog().get(&p.arsenal.current().def.alt));
    std::array::from_fn(|k| {
        let (icon, label, usable) = match p.action_slots.slots[k] {
            ActionSlot::Equipment { item: Equipment::BuildTool } => (Some(sprites::BUILD_ICON), build.clone(), can_build),
            ActionSlot::AltMode => (alt, String::new(), true),
            ActionSlot::Empty => (None, String::new(), false),
        };
        Slot { icon, label, usable, key: keys[k] }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chalk_counts_tallies_to_ten_then_the_number() {
        assert_eq!(chalk(1), (vec!["hud_chalk_1"], None));
        assert_eq!(chalk(5), (vec!["hud_chalk_5"], None));
        assert_eq!(chalk(7), (vec!["hud_chalk_5", "hud_chalk_2"], None));
        assert_eq!(chalk(11), (Vec::new(), Some("11".to_string())));
    }

    #[test]
    fn the_slots_sit_on_the_circles_arms() {
        let l = Lui::new(1280.0, 720.0);
        let c = circle_rect(&l);
        let centre = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (cx, cy) = centre(c);
        let q = CIRCLE_SIZE / 4.0;
        for (n, want) in [(1, (cx, c.y + q)), (2, (cx, c.bottom() - q)), (3, (c.x + q, cy)), (4, (c.right() - q, cy))] {
            let got = centre(slot_rect(c, n, 1.0));
            assert!((got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3, "slot {n}: {got:?} {want:?}");
        }
        assert_eq!((c.w, c.h), (CIRCLE_SIZE, CIRCLE_SIZE));
    }

    #[test]
    fn the_points_list_puts_the_first_player_on_top_and_bars_only_your_row() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw_points(&l, &mut prims, &[Points { points: 500, color: 0, own: false }, Points { points: 750, color: 1, own: true }]);
        let bars = prims.iter().filter(|p| matches!(p, Prim::Image { name, .. } if name == "ZHBO_Mat_scorebar_zom_1")).count();
        assert_eq!(bars, 1);
        let y_of = |t: &str| prims.iter().find_map(|p| if let Prim::Text { text, y, .. } = p { (text == t).then_some(*y) } else { None }).unwrap();
        assert!(y_of("500") < y_of("750"), "the first player above");
    }

    #[test]
    fn the_dpad_stays_on_screen_and_names_an_icon_without_art() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        let empty = || Slot { icon: None, label: String::new(), usable: false, key: "" };
        let build = Slot { icon: Some(sprites::BUILD_ICON), label: "Build".into(), usable: true, key: "B" };
        draw_dpad(&l, &mut prims, &[empty(), build, empty(), empty()], "F");
        for p in &prims {
            let (x, y) = match p {
                Prim::Image { x, y, .. } | Prim::Text { x, y, .. } => (*x, *y),
                _ => continue,
            };
            assert!((0.0..1280.0).contains(&x) && (0.0..720.0).contains(&y), "{p:?} off screen");
        }
        let fallback = prims.iter().find_map(|p| match p {
            Prim::Image { name, fallback, .. } if name == sprites::BUILD_ICON => Some(fallback),
            _ => None,
        });
        assert!(matches!(fallback.map(|f| f.as_slice()), Some([Prim::Text { text, .. }]) if text == "Build"));
    }

    #[test]
    fn a_reviver_sees_the_bar_fill_and_the_downed_see_who_revives_them() {
        use crate::sim::laststand::Downed;
        let mut sim = Sim::new();
        let reviver = sim.new_record(2, crate::sim::Mode::Fps, None);
        sim.players[1] = Some(reviver.clone());
        let downed = sim.new_record(1, crate::sim::Mode::Fps, None);
        sim.players[0] = Some(downed.clone());
        sim.rounds.downed.push(Downed { general: 1, player: 0, pos: crate::math::Vec3::ZERO, bleed: 40.0, revive: 1.5, reviver: 0, revived_by: 2, saved: None, given: 0 });
        let l = Lui::new(1280.0, 720.0);
        let name = |pl: i32| format!("P{pl}");
        let mut prims = Vec::new();
        draw_last_stand(&l, &mut prims, &sim, &reviver, "F", &name, &|_| Some((640.0, 300.0)));
        let fill = prims.iter().find_map(|p| match p {
            Prim::Image { name, w, .. } if name == "ZHBO_Mat_progress_bar_fill" => Some(*w),
            _ => None,
        });
        assert!((fill.unwrap() - 120.0 * 1.5 * 0.5).abs() < 1.0, "half way through three seconds");
        assert!(prims.iter().any(|p| matches!(p, Prim::Image { name, .. } if name == "ZHBO_Mat_waypoint_revive_zm")), "his icon over him");
        let mut prims = Vec::new();
        draw_last_stand(&l, &mut prims, &sim, &downed, "F", &name, &|_| None);
        // Without the converted table the line is its key (the install's has `&&1 is reviving you`).
        assert!(prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if text.contains("ZOMBIE_PLAYER_IS_REVIVING_YOU"))), "tells him he is being revived");
    }

    #[test]
    fn a_timed_power_up_flashes_in_its_last_ten_seconds() {
        assert_eq!(powerup_state(20.0), 1);
        let states: Vec<u8> = (0..100).map(|i| powerup_state(9.99 - i as f32 * 0.099)).collect();
        assert!(states.contains(&2) && states.contains(&3), "{states:?}");
    }

    #[test]
    fn a_trigger_prompt_names_the_key_and_the_price() {
        let runs = trigger_prompt("Hold ^3[{+activate}]^7 for M14 [Cost: &&1]", "F", 500);
        let text: String = runs.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(text, "Hold F for M14 [Cost: 500]");
        assert_eq!(runs[1], ("F".to_string(), (1.0, 1.0, 0.0)), "the key in yellow");
    }

    #[test]
    fn the_low_clip_pulse_runs_between_full_and_half() {
        assert!((ammo_pulse(0.0) - 1.0).abs() < 1e-5);
        assert!((ammo_pulse(0.5) - 0.5).abs() < 1e-5);
        assert!((ammo_pulse(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_round_draws_its_tallies_in_dark_red() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw_round(&l, &mut prims, 7);
        let names: Vec<&str> = prims.iter().filter_map(|p| if let Prim::Image { name, .. } = p { Some(name.as_str()) } else { None }).collect();
        assert_eq!(names, ["ZHBO_Mat_hud_chalk_5", "ZHBO_Mat_hud_chalk_2"]);
    }
}
