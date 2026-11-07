//! BO2's kill feed (the obituaries): one line per General's death, from the sim's kill events, so
//! every machine of a match shows the same lines in the same order.
//!
//! * The line (t6mp CG_Obituary `FUN_004c0080`, built by `FUN_005490a0`): `^c<attacker>^7 <icon>
//!   ^c<victim>`, the names in the team colour code `FUN_004ef380` gives (`^8` the local player's
//!   side, `^9` the other); with no attacker (a suicide, or a death to the world: Zero Hour's own
//!   weapons) the attacker part is left out and ` CGAME_QUAKE_SUICIDE` follows the victim.
//! * The icon ([`icon`]): the server's `obituary` builtin (`FUN_00628c80`) sends the weapon unless
//!   the means is `MOD_HEAD_SHOT`, `MOD_SUICIDE`, `MOD_FALLING`, `MOD_CRUSH`, a melee not by an
//!   Assault Shield or an impact by a weapon whose `impactType` is not 15; the client draws the
//!   weapon's `killIcon` (`hud_obit_death_suicide` when it has none) at `killIconRatio`, mirrored by
//!   `flipKillIcon`, else the means' own icon. The icon is 1.4 font heights square, twice as wide
//!   for 2:1 and 4:1 and half as tall for 4:1, each side rounded to 32nds (`FUN_00547770`).
//! * The window (game message window 0, `FUN_00548a80`'s defaults): 4 lines, each for 5 s, fading
//!   in over 0.25 s and out over the last 0.5 s, the stack sliding up over 0.25 s as a line comes
//!   in. LUI draws it (`FUN_0069f0e0`) bottom up at its font's pixel height (`CoD.fonts.ExtraSmall`,
//!   `CoD.textSize.ExtraSmall` 20 in English) in `hud.lua:827`'s box: left 13..277 of the safe
//!   area, bottom-anchored −249..−149.

use super::frame::Lui;
use super::Prim;
use crate::damage::MeansOfDeath;
use crate::lui::font::{Align, Family};
use crate::lui::Rect;
use crate::weapons::{WeapType, WeaponDef};
use std::collections::VecDeque;

/// `con_gameMsgWindow0MsgTime`, `LineCount`, `ScrollTime`, `FadeInTime`, `FadeOutTime`.
pub const MSG_TIME: f32 = 5.0;
pub const LINE_COUNT: usize = 4;
pub const SCROLL_TIME: f32 = 0.25;
pub const FADE_IN: f32 = 0.25;
pub const FADE_OUT: f32 = 0.5;
/// The icon's base side in font heights (`_DAT_00b1347c`).
pub const ICON_SIZE: f32 = 1.4;
/// `CoD.textSize.ExtraSmall` (English): the window's line height.
pub const LINE_HEIGHT: f32 = 20.0;
/// `hud.lua:827`'s box (LUI units in the safe area).
pub const BOX_LEFT: f32 = 13.0;
pub const BOX_RIGHT: f32 = 277.0;
pub const BOX_BOTTOM: f32 = -149.0;
/// `g_TeamColor_MyTeam` and `g_TeamColor_EnemyTeam` (G6): the `^8` / `^9` names.
pub const MY_TEAM: (f32, f32, f32) = (0.4, 0.7, 0.4);
pub const ENEMY_TEAM: (f32, f32, f32) = (1.0, 0.315, 0.35);
/// The weapon `impactType` whose `MOD_IMPACT` keeps the weapon's icon (`FUN_00628c80`).
const IMPACT_TYPE_WEAPON_ICON: i32 = 15;

/// One icon: the material, its size in font heights and whether it is mirrored.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub material: String,
    pub w: f32,
    pub h: f32,
    pub flip: bool,
}

/// `FUN_00547770`: a side in font heights, as the text's icon code carries it (32nds, 16..127
/// before the code's offset).
fn side(v: f32) -> f32 {
    ((v * 32.0).round().clamp(16.0, 127.0)) / 32.0
}

/// The obituary's icon for a death by `how` with `weapon` (see the module doc).
pub fn icon(how: MeansOfDeath, weapon: Option<&WeaponDef>) -> Icon {
    use MeansOfDeath as M;
    let weapon = weapon.filter(|w| match how {
        M::Impact => w.impact_type == IMPACT_TYPE_WEAPON_ICON,
        M::Melee => w.traits.weap_type == WeapType::RiotShield,
        M::HeadShot | M::Suicide | M::Falling | M::Crush => false,
        _ => true,
    });
    let plain = |m: &str| Icon { material: m.to_string(), w: side(ICON_SIZE), h: side(ICON_SIZE), flip: false };
    match weapon {
        Some(w) if w.kill_icon.material.is_empty() => plain("hud_obit_death_suicide"),
        Some(w) => {
            let k = &w.kill_icon;
            let wide = if k.ratio != 0 { 2.0 } else { 1.0 };
            let tall = if k.ratio >= 2 { 0.5 } else { 1.0 };
            Icon { material: k.material.clone(), w: side(ICON_SIZE * wide), h: side(ICON_SIZE * tall), flip: k.flip }
        }
        None => plain(match how {
            M::Melee => "hud_obit_knife",
            M::HeadShot => "killiconheadshot",
            M::Crush => "hud_obit_death_crush",
            M::Falling => "hud_obit_death_falling",
            M::Suicide | M::TriggerHurt => "hud_obit_death_suicide",
            M::Impact => "hud_obit_death_grenade_round",
            _ => "killicondied",
        }),
    }
}

/// One line: the attacker's seat (`None`: no attacker part), the victim's, the icon and when it
/// came in (the HUD clock, seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub attacker: Option<u8>,
    pub victim: u8,
    pub icon: Icon,
    pub start: f32,
}

/// The feed: its lines, oldest first, and the seats whose death already has a line (their
/// `KilledBy` came before the `GeneralDied`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Feed {
    pub lines: VecDeque<Line>,
    reported: [bool; crate::sim::MAX_PLAYERS],
}

impl Feed {
    /// Seat `victim`'s General was killed by seat `by` (his own: a suicide) with `how`/`weapon`.
    pub fn killed_by(&mut self, victim: u8, by: Option<u8>, how: MeansOfDeath, weapon: Option<&WeaponDef>, now: f32) {
        if let Some(r) = self.reported.get_mut(victim as usize) {
            *r = true;
        }
        let attacker = by.filter(|&b| b != victim);
        self.push(Line { attacker, victim, icon: icon(how, weapon), start: now });
    }

    /// Seat `victim`'s General died: a line of its own unless his `KilledBy` gave one (a death no
    /// blow explains is the world's, `MOD_UNKNOWN`: `killicondied`).
    pub fn died(&mut self, victim: u8, now: f32) {
        match self.reported.get_mut(victim as usize) {
            Some(r) if *r => *r = false,
            _ => self.push(Line { attacker: None, victim, icon: icon(MeansOfDeath::Unknown, None), start: now }),
        }
    }

    fn push(&mut self, line: Line) {
        self.lines.push_back(line);
        while self.lines.len() > LINE_COUNT {
            self.lines.pop_front();
        }
    }

    /// The lines still on screen at `now`, newest first, each with its alpha and its offset below
    /// its resting place (LUI units, the slide-in).
    pub fn shown(&self, now: f32) -> Vec<(&Line, f32, f32)> {
        let live: Vec<&Line> = self.lines.iter().rev().filter(|l| now - l.start < MSG_TIME && now >= l.start).collect();
        // `FUN_0069f0e0`: every line younger than the scroll time pushes the stack down by the
        // rest of its slide.
        let slide: f32 = live.iter().map(|l| (1.0 - (now - l.start) / SCROLL_TIME).clamp(0.0, 1.0) * LINE_HEIGHT).sum();
        live.into_iter()
            .map(|l| {
                let age = now - l.start;
                let end = MSG_TIME - age;
                let mut a = if end < FADE_OUT { end / FADE_OUT } else { 1.0 };
                if age < FADE_IN {
                    a *= age / FADE_IN;
                }
                (l, a.clamp(0.0, 1.0), slide)
            })
            .collect()
    }
}

/// What the draw needs of the match: each seat's name and whether he is on the local player's
/// side.
pub struct Names<'a> {
    pub name: &'a dyn Fn(u8) -> String,
    pub ally: &'a dyn Fn(u8) -> bool,
    /// `CGAME_QUAKE_SUICIDE`, localized.
    pub suicide: &'a str,
}

/// Draw the feed.
pub fn draw(l: &Lui, prims: &mut Vec<Prim>, feed: &Feed, now: f32, names: &Names) {
    let screen = l.screen();
    let bottom = screen.bottom() + BOX_BOTTOM;
    let colour = |seat: u8| if (names.ally)(seat) { MY_TEAM } else { ENEMY_TEAM };
    for (i, (line, alpha, slide)) in feed.shown(now).into_iter().enumerate() {
        if alpha <= 0.0 {
            continue;
        }
        let top = bottom - (i as f32 + 1.0) * LINE_HEIGHT + slide;
        let mut x = screen.x + BOX_LEFT;
        let run = |prims: &mut Vec<Prim>, text: &str, rgb: (f32, f32, f32), x: &mut f32| {
            let w = l.text_width(Family::ExtraSmall, text, LINE_HEIGHT);
            l.text(prims, Rect { x: *x, y: top, w, h: LINE_HEIGHT }, Family::ExtraSmall, text, Some(Align::Left), (true, false), rgb, alpha);
            *x += w;
        };
        if let Some(a) = line.attacker {
            run(prims, &(names.name)(a), colour(a), &mut x);
            run(prims, " ", (1.0, 1.0, 1.0), &mut x);
        }
        let (iw, ih) = (line.icon.w * LINE_HEIGHT, line.icon.h * LINE_HEIGHT);
        let r = l.px(Rect { x, y: top + (LINE_HEIGHT - ih) * 0.5, w: iw, h: ih });
        let argb = super::frame::argb((1.0, 1.0, 1.0), alpha);
        let name = crate::lui::materials::image(&line.icon.material);
        if line.icon.flip {
            let corners = [(r.right(), r.y), (r.x, r.y), (r.x, r.bottom()), (r.right(), r.bottom())];
            prims.push(Prim::Quad { name, corners, argb, fallback: Vec::new() });
        } else {
            prims.push(Prim::Image { name, x: r.x, y: r.y, w: r.w, h: r.h, argb, fallback: Vec::new() });
        }
        x += iw;
        run(prims, " ", (1.0, 1.0, 1.0), &mut x);
        let victim = colour(line.victim);
        run(prims, &(names.name)(line.victim), victim, &mut x);
        if line.attacker.is_none() && !names.suicide.is_empty() {
            run(prims, &format!(" {}", names.suicide), victim, &mut x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::KillIcon;

    fn def(material: &str, ratio: u8) -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef { kill_icon: KillIcon { material: material.into(), ratio, flip: false }, ..Default::default() }))
    }

    #[test]
    fn the_icon_follows_the_servers_weapon_rule_and_the_means() {
        let rifle = def("hud_an94", 1);
        // A rifle round: the weapon's 2:1 icon, 2.8 x 1.4 font heights in 32nds.
        let i = icon(MeansOfDeath::RifleBullet, Some(rifle));
        assert_eq!((i.material.as_str(), i.w, i.h), ("hud_an94", 90.0 / 32.0, 45.0 / 32.0));
        // A headshot never sends the weapon: the headshot icon replaces it.
        assert_eq!(icon(MeansOfDeath::HeadShot, Some(rifle)).material, "killiconheadshot");
        // A knife (not a shield) is the means' knife; a 4:1 icon is half as tall.
        assert_eq!(icon(MeansOfDeath::Melee, Some(rifle)).material, "hud_obit_knife");
        assert_eq!(icon(MeansOfDeath::GrenadeSplash, Some(def("hud_frag", 2))).h, 22.0 / 32.0);
        // No weapon: by the means; a weapon without an icon draws the suicide one.
        assert_eq!(icon(MeansOfDeath::Unknown, None).material, "killicondied");
        assert_eq!(icon(MeansOfDeath::Crush, None).material, "hud_obit_death_crush");
        assert_eq!(icon(MeansOfDeath::RifleBullet, Some(def("", 0))).material, "hud_obit_death_suicide");
        // An impact keeps the weapon only for impactType 15.
        assert_eq!(icon(MeansOfDeath::Impact, Some(rifle)).material, "hud_obit_death_grenade_round");
    }

    #[test]
    fn one_line_per_death_four_at_most_each_for_five_seconds() {
        let mut f = Feed::default();
        f.killed_by(1, Some(0), MeansOfDeath::RifleBullet, None, 0.0);
        f.died(1, 0.1);
        assert_eq!(f.lines.len(), 1, "the KilledBy's line, none for its GeneralDied");
        f.died(2, 0.2);
        assert_eq!(f.lines.back().map(|l| (l.attacker, l.icon.material.as_str())), Some((None, "killicondied")), "a world death");
        f.killed_by(3, Some(3), MeansOfDeath::Suicide, None, 0.3);
        assert_eq!(f.lines.back().unwrap().attacker, None, "a suicide has no attacker part");
        for k in 0..3 {
            f.died(4, 0.4 + k as f32);
        }
        assert_eq!(f.lines.len(), LINE_COUNT);
        let shown = f.shown(3.0);
        assert_eq!(shown[0].0.start, 0.4 + 2.0, "newest first");
        // Fading: in over 0.25 s, out over the last 0.5 s.
        let mut g = Feed::default();
        g.died(0, 0.0);
        assert!((g.shown(0.125)[0].1 - 0.5).abs() < 1e-5);
        assert!((g.shown(4.75)[0].1 - 0.5).abs() < 1e-5);
        assert!(g.shown(5.0).is_empty());
        // The slide: a new line starts one line low and rises over 0.25 s.
        assert_eq!(g.shown(0.0)[0].2, LINE_HEIGHT);
        assert_eq!(g.shown(0.3)[0].2, 0.0);
    }

    #[test]
    fn the_line_reads_attacker_icon_victim_in_team_colours() {
        let _f = crate::lui::font::lock();
        crate::lui::font::load("");
        let mut f = Feed::default();
        f.killed_by(1, Some(0), MeansOfDeath::RifleBullet, Some(def("hud_an94", 1)), 0.0);
        let l = Lui::new(1280.0, 720.0);
        let name = |s: u8| format!("P{s}");
        let ally = |s: u8| s == 0;
        let mut prims = Vec::new();
        draw(&l, &mut prims, &f, 1.0, &Names { name: &name, ally: &ally, suicide: "" });
        let texts: Vec<(String, u32)> = prims.iter().filter_map(|p| if let Prim::Text { text, argb, .. } = p { Some((text.clone(), *argb)) } else { None }).filter(|(t, _)| t.trim() != "").collect();
        assert_eq!(texts, vec![("P0".into(), super::super::frame::argb(MY_TEAM, 1.0)), ("P1".into(), super::super::frame::argb(ENEMY_TEAM, 1.0))]);
        let icon = prims.iter().find_map(|p| if let Prim::Image { name, y, h, w, .. } = p { Some((name.clone(), *y, *w, *h)) } else { None }).unwrap();
        assert_eq!(icon.0, "ZHBO_Mat_hud_an94");
        // 720 lines: the newest line's box is 20 tall, its bottom 149 above the screen's bottom.
        assert!((icon.1 + icon.3 * 0.5 - (720.0 - 149.0 - 10.0)).abs() < 0.01, "{icon:?}");
        assert!((icon.2 - 90.0 / 32.0 * 20.0).abs() < 0.01);
    }
}
