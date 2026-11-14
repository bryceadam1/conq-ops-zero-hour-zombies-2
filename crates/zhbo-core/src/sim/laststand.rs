/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' last stand.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! A survivor downed instead of killed (the engine's damage hook decides it, [`crate::laststand`])
//! lies in last stand (`_zm_laststand.gsc`, rules in `zombies.ini` `[LastStand]`):
//!
//! * **Going down** (`playerlaststand`): he loses `penalty_downed` of his points, rounded up to ten
//!   (`player_reduce_points( "downed" )`, `_zm_score.gsc:292-296`). His weapons stay his but he holds
//!   only his last-stand pistol (`laststand_disable_player_weapons`, `laststand_give_pistol`): the
//!   best pistol he carries ([`crate::zombies::LastStand::best_pistol`]), else the default one; the
//!   default pistol gets two clips of stock, another keeps its clip and up to two clips of its stock
//!   (`last_stand_pistol_swap`, `_zm.gsc:2205-2251`). No weapon change, offhand or melee
//!   (`disableweaponcycling`, the engine's last stand), and he lies in BO2's `LASTSTAND` stance,
//!   drawn and moved as prone (t6zm's `PRONE_TO_LASTSTAND`…; its own crawl speed is not traced yet).
//!   The zombies leave him for the survivors up (`self.ignoreme = 1`).
//! * **Bleeding out** (`laststand_bleedout`): `BleedoutTime` seconds, holding at the end while he is
//!   being revived; then he dies (`bleed_out`) and comes back at the next round (`sim::zombies`).
//! * **The revive** (`revive_trigger_think`, `revive_do_revive`): a survivor up, inside his
//!   `ReviveRadius` trigger and facing him (`can_revive`, `is_facing`) holds the use key for
//!   `ReviveTime` seconds; letting go starts it over. The reviver's gun is down meanwhile (BO2 hands
//!   him the `syrette_zm`). Revived (`revive_success`), he has his weapons back, the pistol's rounds
//!   spent charged to it unless it is the default pistol (`last_stand_restore_pistol_ammo`,
//!   `_zm.gsc:2375-2432`), and full health (`reviveplayer`).
//! * **Everyone down** (`player_damage_override`'s count, `_zm.gsc:4239-4291`): with no survivor up,
//!   the game is over: the downed die with the last one.

use super::*;
use crate::weapons::{Arsenal, Loadout};
use crate::zombies::LastStand;

/// One survivor in last stand.
#[derive(Clone, Debug, PartialEq)]
pub struct Downed {
    pub general: u32,
    /// His Zero Hour player.
    pub player: i32,
    /// Where he lies, as of the last frame (the revive trigger is linked to him).
    pub pos: Vec3,
    /// Seconds of bleeding out left.
    pub bleed: f32,
    /// Seconds of the revive under way.
    pub revive: f32,
    /// This frame's reviver (his General; 0: none), set by the reviver's step.
    pub reviver: u32,
    /// Last frame's reviver, for the HUD.
    pub revived_by: u32,
    /// His weapons as they were, and the rounds his last-stand pistol was given when he carried it
    /// (BO2's `stored_weapon_info[ pistol ].given_amt`).
    pub saved: Option<Arsenal>,
    pub given: u32,
    /// Alone with Quick Revive: seconds until he revives himself (`wait_and_revive`; 0: none).
    pub self_revive: f32,
}

crate::statehash::state_hash!(struct Downed { general, player, pos, bleed, revive, reviver, revived_by, saved, given, self_revive });

/// A weapon's rounds: its clip, a dual-wield's left clip and its stock.
fn total(w: &crate::weapons::WeaponState) -> u32 {
    w.ammo as u32 + w.left_ammo().unwrap_or(0) as u32 + w.reserve as u32
}

/// `last_stand_pistol_swap`: his last-stand arsenal of the weapons `saved`, and the rounds the
/// pistol was given when he carries it (0 when he does not).
pub fn pistol_arsenal(saved: &Arsenal, ls: &LastStand) -> (Arsenal, u32) {
    let pistols: Vec<(&str, u32)> = saved.weapons.iter().filter(|w| w.main.class == crate::weapons::Class::Pistol).map(|w| (w.main.id.as_str(), total(w))).collect();
    let pistol = ls.best_pistol(&pistols).to_string();
    let kit = Loadout { primary: pistol.clone(), melee: saved.loadout.melee.clone(), held_knife: false, ..Loadout::unarmed() };
    let mut down = Arsenal::with_loadout(kit);
    down.lethal.count = 0;
    down.tactical.count = 0;
    let Some(w) = down.weapons.first_mut() else { return (down, 0) };
    let double = 2 * w.main.clip as u32;
    match saved.weapons.iter().find(|s| s.main.id == pistol) {
        Some(s) if pistol != ls.pistol => {
            // Its clip as it was, and up to two clips of its stock.
            w.ammo = s.ammo;
            if let (Some(l), Some(sl)) = (w.left.as_mut(), s.left.as_ref()) {
                l.ammo = sl.ammo;
            }
            w.reserve = (s.reserve as u32).min(double) as u16;
            let given = total(w);
            (down, given)
        }
        _ => {
            // The default pistol (carried or given): a full clip and two of stock.
            w.reserve = double.min(u16::MAX as u32) as u16;
            (down, 0)
        }
    }
}

/// `last_stand_restore_pistol_ammo`: his weapons back, the rounds his carried pistol spent
/// (`given` less what is left in `down`) taken from its stock, then its clip.
pub fn restore_arsenal(mut saved: Arsenal, down: &Arsenal, given: u32, ls: &LastStand) -> Arsenal {
    let Some(d) = down.weapons.first() else { return saved };
    if given == 0 || d.main.id == ls.pistol {
        return saved;
    }
    if let Some(s) = saved.weapons.iter_mut().find(|s| s.main.id == d.main.id) {
        let mut used = given.saturating_sub(total(d));
        let from_stock = used.min(s.reserve as u32);
        s.reserve -= from_stock as u16;
        used -= from_stock;
        s.ammo = s.ammo.saturating_sub(used.min(u16::MAX as u32) as u16);
    }
    saved
}

impl Sim {
    /// The seat whose record drives `general`.
    pub fn seat_of(&self, general: u32) -> Option<usize> {
        self.players.iter().position(|p| p.as_ref().is_some_and(|p| p.general == general))
    }

    /// Survivor `general` goes down (queued by the engine's damage hook).
    pub fn go_down(&mut self, host: &dyn LogicHost, general: u32) {
        if general == 0 || self.rounds.downed.iter().any(|d| d.general == general) {
            return;
        }
        let Some(info) = host.object(general, general) else { return };
        let ls = LastStand::get();
        let player = info.owner_player;
        let points = host.money(player).max(0) as i64;
        let lost = crate::zombies::round_up_to_ten((points as f32 * crate::zombies::Vars::get().penalty_downed) as i64).min(points);
        if lost > 0 {
            host.add_cash(player, -(lost as i32));
        }
        let self_revive = if self.self_revives(general) { crate::zombies::Perks::get().solo_revive_time } else { 0.0 };
        let mut downed = Downed { general, player, pos: info.pos, bleed: ls.bleedout_time, revive: 0.0, reviver: 0, revived_by: 0, saved: None, given: 0, self_revive };
        if let Some(idx) = self.seat_of(general) {
            if let Some(mut p) = self.players[idx].take() {
                let (down, given) = pistol_arsenal(&p.arsenal, ls);
                let mut saved = std::mem::replace(&mut p.arsenal, down);
                // `perk_think`: his perks go as he goes down.
                self.lose_perks(general, &mut saved);
                downed.saved = Some(saved);
                downed.given = given;
                p.body.stance = Stance::Prone;
                p.charge = None;
                p.hammer = None;
                self.players[idx] = Some(p);
            }
            self.emit(idx, EventKind::Downed { general, points: lost as u16 });
        }
        host.log(&format!("zombies: General {general} is down, {lost} points lost, {:.0} s to bleed out", ls.bleedout_time));
        self.rounds.downed.push(downed);
        self.rounds.up = self.rounds.up.saturating_sub(1);
        if !self.rounds.alive.is_empty() {
            self.send_zombies(host);
        }
    }

    /// Whether `general` lies in last stand.
    pub fn is_downed(&self, general: u32) -> bool {
        self.rounds.downed.iter().any(|d| d.general == general)
    }

    /// A downed survivor's input: no stance change, jump, sprint, weapon change, offhand, melee,
    /// alt mode, action slot or field computer; he lies prone.
    pub(super) fn downed_input(p: &mut PlayerSim, input: &mut FpsInput) {
        let blocked = buttons::JUMP
            | buttons::SPRINT
            | buttons::CROUCH
            | buttons::PRONE
            | buttons::SWAP_WEAPON
            | buttons::MELEE
            | buttons::LETHAL
            | buttons::TACTICAL
            | buttons::ALT_MODE
            | buttons::FIELD_COMPUTER
            | buttons::ACTION_SLOT_1
            | buttons::ACTION_SLOT_2
            | buttons::ACTION_SLOT_4;
        input.held &= !blocked;
        input.pressed &= !blocked;
        p.body.stance = Stance::Prone;
    }

    /// The downed survivor `p` may revive now (`can_revive`): another, inside his trigger (as wide
    /// as it is tall) and in front of `p` (`is_facing`). The HUD's hint asks the same.
    pub fn revive_target(&self, p: &PlayerSim) -> Option<usize> {
        if !p.survivor || self.is_downed(p.general) {
            return None;
        }
        let ls = LastStand::get();
        let radius = crate::units::inches(ls.revive_radius);
        let fwd = Vec3::new(crate::math::det::cos(p.yaw), crate::math::det::sin(p.yaw), 0.0);
        self.rounds.downed.iter().position(|d| {
            if d.general == p.general {
                return false;
            }
            let to = d.pos - p.body.pos;
            let flat = Vec3::new(to.x, to.y, 0.0);
            let dist = flat.length();
            let facing = dist <= 1e-3 || fwd.dot(flat) / dist > ls.revive_facing;
            dist <= radius && to.z.abs() <= radius && facing
        })
    }

    /// The use key held over a downed survivor (`is_reviving`): `p` revives him this frame, with his
    /// gun down and the press kept from the field computer. True when he is reviving.
    pub(super) fn revive_input(&mut self, p: &PlayerSim, input: &mut FpsInput) -> bool {
        if (input.held | input.pressed) & buttons::FIELD_COMPUTER == 0 {
            return false;
        }
        let Some(i) = self.revive_target(p) else { return false };
        self.rounds.downed[i].reviver = p.general;
        let blocked = buttons::FIELD_COMPUTER | buttons::FIRE | buttons::ADS | buttons::RELOAD | buttons::MELEE | buttons::LETHAL | buttons::TACTICAL | buttons::SWAP_WEAPON;
        input.held &= !blocked;
        input.pressed &= !blocked;
        true
    }

    /// The last stands' frame (on a Zombies map, after the players' steps): bleeding out, revives,
    /// and the end when nobody is up.
    pub(super) fn step_last_stand(&mut self, host: &dyn LogicHost) {
        let ls = LastStand::get();
        // The dead (a bleed-out, the end) and the vanished leave the list.
        let mut downed = std::mem::take(&mut self.rounds.downed);
        downed.retain_mut(|d| match host.object(d.general, d.general).filter(|o| !o.has(objflags::DEAD)) {
            Some(o) => {
                d.pos = o.pos;
                true
            }
            None => false,
        });
        self.rounds.downed = downed;
        let living = self.living_survivors(host);
        self.rounds.up = living.iter().filter(|o| !self.is_downed(o.id)).count() as u32;
        // Alone with Quick Revive he waits on himself (`wait_and_revive`), not the end.
        if self.rounds.up == 0 && !self.rounds.downed.is_empty() && self.rounds.downed.iter().all(|d| d.self_revive <= 0.0) {
            host.log("zombies: no survivor is up; the downed die with the last");
            for d in std::mem::take(&mut self.rounds.downed) {
                host.kill(d.general);
            }
            return;
        }
        let mut i = 0;
        while i < self.rounds.downed.len() {
            let d = &mut self.rounds.downed[i];
            // Alone with Quick Revive he gets up by himself (`wait_and_revive`).
            if d.self_revive > 0.0 {
                d.self_revive -= LOGIC_DT;
                if d.self_revive <= 0.0 {
                    let mut d = self.rounds.downed.remove(i);
                    d.revived_by = d.general;
                    self.revive(host, d);
                    continue;
                }
            }
            d.revived_by = d.reviver;
            d.reviver = 0;
            if d.revived_by != 0 {
                d.revive += LOGIC_DT;
            } else {
                d.revive = 0.0;
            }
            // `revive_do_revive`: a reviver with Quick Revive takes half the time.
            let reviver_qr = d.revived_by != 0 && self.rounds.perks.has(d.revived_by, "specialty_quickrevive");
            let revive_time = ls.revive_time * if reviver_qr { crate::zombies::Perks::get().revive_scale } else { 1.0 };
            let d = &mut self.rounds.downed[i];
            if d.revived_by != 0 && d.revive >= revive_time {
                let d = self.rounds.downed.remove(i);
                self.revive(host, d);
                continue;
            }
            d.bleed = (d.bleed - LOGIC_DT).max(0.0);
            if d.bleed <= 0.0 && d.revived_by == 0 {
                let d = self.rounds.downed.remove(i);
                host.log(&format!("zombies: General {} bled out", d.general));
                if let Some(idx) = self.seat_of(d.general) {
                    self.emit(idx, EventKind::BledOut { general: d.general });
                }
                host.kill(d.general);
                continue;
            }
            i += 1;
        }
    }

    /// `revive_success`: `d` is up again with his weapons and full health.
    fn revive(&mut self, host: &dyn LogicHost, d: Downed) {
        let ls = LastStand::get();
        if let Some(idx) = self.seat_of(d.general) {
            if let Some(mut p) = self.players[idx].take() {
                if let Some(saved) = d.saved {
                    p.arsenal = restore_arsenal(saved, &p.arsenal, d.given, ls);
                }
                self.players[idx] = Some(p);
            }
            self.emit(idx, EventKind::Revived { general: d.general, by: d.revived_by });
        }
        if let Some(seat) = self.seat_of(d.revived_by) {
            self.emit(seat, EventKind::Revived { general: d.general, by: d.revived_by });
        }
        if let Some(o) = host.object(d.general, d.general) {
            host.heal(d.general, (o.max_health - o.health).max(0.0));
        }
        host.log(&format!("zombies: General {} revived by General {}", d.general, d.revived_by));
        self.rounds.up += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};
    use crate::weapons::WeaponState;

    fn rules() -> LastStand {
        LastStand { bleedout_time: 45.0, revive_time: 3.0, revive_radius: 75.0, revive_facing: 0.9, pistol: "fnp45_mp".into(), pistol_values: vec!["fnp45_mp".into(), "fiveseven_mp".into()] }
    }

    fn kit(guns: &[(&str, u16, u16)]) -> Arsenal {
        let mut a = Arsenal::with_loadout(Loadout { primary: "an94_mp".into(), held_knife: false, ..Loadout::unarmed() });
        for &(id, ammo, reserve) in guns {
            let mut w = WeaponState::new(crate::weapons::catalog().get(id).unwrap());
            (w.ammo, w.reserve) = (ammo, reserve);
            a.weapons.push(w);
        }
        a
    }

    #[test]
    fn the_default_pistol_comes_with_two_clips_of_stock_and_costs_nothing_back() {
        let ls = rules();
        let saved = kit(&[]);
        let (down, given) = pistol_arsenal(&saved, &ls);
        let w = &down.weapons[0];
        assert_eq!((w.main.id.as_str(), w.ammo, w.reserve, given), ("fnp45_mp", w.main.clip, 2 * w.main.clip, 0));
        assert_eq!((down.weapons.len(), down.lethal.count, down.tactical.count), (1, 0, 0), "the pistol alone");
        assert_eq!(restore_arsenal(saved.clone(), &down, given, &ls), saved);
    }

    #[test]
    fn a_carried_pistol_keeps_its_clip_and_pays_its_rounds_spent_from_stock_then_clip() {
        let ls = rules();
        let saved = kit(&[("fiveseven_mp", 5, 100)]);
        let (mut down, given) = pistol_arsenal(&saved, &ls);
        let clip = down.weapons[0].main.clip;
        assert_eq!((down.weapons[0].main.id.as_str(), down.weapons[0].ammo, down.weapons[0].reserve), ("fiveseven_mp", 5, 2 * clip));
        assert_eq!(given, 5 + 2 * clip as u32);
        // He fires the clip and reloads once: a clip and five spent.
        down.weapons[0].ammo = 0;
        down.weapons[0].reserve = clip;
        let back = restore_arsenal(saved.clone(), &down, given, &ls);
        let w = back.weapons.iter().find(|w| w.main.id == "fiveseven_mp").unwrap();
        assert_eq!((w.ammo, w.reserve), (5, 100 - (5 + clip)));
        assert_eq!(back.weapons.len(), saved.weapons.len(), "every weapon back");
    }

    #[test]
    fn an_empty_carried_pistol_gives_way_to_the_default() {
        let ls = rules();
        let (down, _) = pistol_arsenal(&kit(&[("fiveseven_mp", 0, 0)]), &ls);
        assert_eq!(down.weapons[0].main.id, "fnp45_mp");
    }

    fn co_op() -> (Sim, FakeHost) {
        let host = FakeHost::default();
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        host.add(ObjInfo { id: 2, flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, pos: Vec3::new(10.0, 0.0, 0.0), ..general(2, 1) });
        host.wallet.borrow_mut().push((0, 1000));
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.take_control(&host, 1, 2);
        (sim, host)
    }

    fn step(sim: &mut Sim, host: &FakeHost, frames: u32) {
        for _ in 0..frames {
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    #[test]
    fn going_down_costs_five_percent_and_bleeds_out_unrevived() {
        let (mut sim, host) = co_op();
        step(&mut sim, &host, 1);
        assert_eq!(sim.rounds.up, 2);
        let before = sim.players[0].as_ref().unwrap().arsenal.clone();
        sim.go_down(&host, 1);
        assert_eq!(*host.cash.borrow(), vec![(0, -50)], "5% of 1000, rounded up to ten");
        let p = sim.players[0].as_ref().unwrap();
        assert_eq!(p.body.stance, Stance::Prone);
        assert_eq!(sim.rounds.downed[0].saved.as_ref(), Some(&before));
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::Downed { general: 1, points: 50 })));
        step(&mut sim, &host, (45.0 / LOGIC_DT) as u32 - 2);
        assert!(host.killed.borrow().is_empty());
        step(&mut sim, &host, 4);
        assert_eq!(*host.killed.borrow(), vec![1], "bled out after 45 s");
        assert!(sim.rounds.downed.is_empty());
    }

    #[test]
    fn a_teammate_holding_use_over_him_revives_him_in_three_seconds() {
        let (mut sim, host) = co_op();
        step(&mut sim, &host, 1);
        let before = sim.players[0].as_ref().unwrap().arsenal.clone();
        host.objects.borrow_mut().iter_mut().filter(|o| o.id == 1).for_each(|o| o.health = 1.0);
        sim.go_down(&host, 1);
        // Player 1 faces him (he lies at the origin, 10 along -x: 60 in, inside the 75 in trigger) and
        // holds the use key.
        let hold = FpsInput { held: buttons::FIELD_COMPUTER, pressed: buttons::FIELD_COMPUTER, yaw: std::f32::consts::PI, ..Default::default() };
        for _ in 0..(3.0 / LOGIC_DT) as u32 + 2 {
            sim.input(1, 2, hold);
            step(&mut sim, &host, 1);
        }
        assert!(sim.rounds.downed.is_empty(), "revived");
        assert_eq!(sim.players[0].as_ref().unwrap().arsenal.weapons.len(), before.weapons.len());
        assert_eq!(host.get(1).health, host.get(1).max_health, "full health");
        assert!(matches!(sim.players[1].as_ref().unwrap().mode, Mode::Fps), "the press never opened his field computer");
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::Revived { general: 1, by: 2 })));
    }

    #[test]
    fn letting_go_starts_the_revive_over_and_facing_away_does_nothing() {
        let (mut sim, host) = co_op();
        step(&mut sim, &host, 1);
        sim.go_down(&host, 1);
        let hold = FpsInput { held: buttons::FIELD_COMPUTER, yaw: std::f32::consts::PI, ..Default::default() };
        for _ in 0..30 {
            sim.input(1, 2, hold);
            step(&mut sim, &host, 1);
        }
        assert!(sim.rounds.downed[0].revive > 0.9);
        sim.input(1, 2, FpsInput { yaw: std::f32::consts::PI, ..Default::default() });
        step(&mut sim, &host, 1);
        assert_eq!(sim.rounds.downed[0].revive, 0.0);
        for _ in 0..120 {
            sim.input(1, 2, FpsInput { held: buttons::FIELD_COMPUTER, yaw: 0.0, ..Default::default() });
            step(&mut sim, &host, 1);
        }
        assert_eq!(sim.rounds.downed.len(), 1, "his back to him: no revive");
    }

    #[test]
    fn alone_quick_revive_gets_him_up_after_ten_seconds_and_is_spent() {
        let host = FakeHost::default();
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        sim.rounds.players = vec![0];
        let qr = crate::zombies::Perks::get().index("specialty_quickrevive").unwrap() as u8;
        sim.rounds.perks.owned.push((1, vec![qr]));
        sim.go_down(&host, 1);
        assert!(sim.rounds.perks.of(1).is_empty(), "his perks go as he goes down");
        step(&mut sim, &host, (9.8 / LOGIC_DT) as u32);
        assert_eq!(sim.rounds.downed.len(), 1, "alone and down, but not dead");
        assert!(host.killed.borrow().is_empty());
        step(&mut sim, &host, 10);
        assert!(sim.rounds.downed.is_empty(), "up again by himself");
    }

    #[test]
    fn with_nobody_up_the_downed_die_with_the_last() {
        let (mut sim, host) = co_op();
        step(&mut sim, &host, 1);
        sim.go_down(&host, 1);
        host.kill(2);
        step(&mut sim, &host, 1);
        assert_eq!(*host.killed.borrow(), vec![2, 1]);
    }
}
