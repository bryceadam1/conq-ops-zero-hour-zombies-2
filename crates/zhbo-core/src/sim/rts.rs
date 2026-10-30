/*
** Conquer Ops - Zero Hour gameplay core: the overhead General: RTS weapon tracking, ammo, offhands and orders.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The General under RTS control. Zero Hour's AI moves, aims and fires his three weapon slots; the
//! core owns what those shots are:
//!
//! * **His guns are his class's.** Each slot holds the Zero Hour weapon converted from the BO2
//!   weapon in that place of his kit (`names::rts_weapon`: its fire rate, magazine, reload, reach
//!   and sound). [`RtsWeapons::templates`] names them every frame and the bridge swaps them in when
//!   they change (a class pick, the third slot's order). The ammo is the same arsenal first person
//!   uses: every shot is counted here, an empty gun can't fire, and a dry gun hands over.
//! * **The third slot is an order** (the command bar's Lethal, Tactical and Melee buttons): Zero
//!   Hour walks him into the order's reach (the template's range) and fires it once; the core then
//!   does what first person does — throws the BO2 grenade on its arc to the target, plants the
//!   equipment in front of him facing it, or swings the knife ([`Sim::melee_strike`]).
//! * **Reload** is an order too ([`Sim::rts_reload`]): the gun in hand tops its magazine up from
//!   its spare rounds, and the bridge starts Zero Hour's reload for the time.
//!
//! Orders arrive as game messages (`MSG_ZHBO_TAKE_CONTROL` with an action), so every peer applies
//! them on the same logic frame.

use super::*;
use crate::sim::fire::MeleeReach;
use crate::weapons::{Place, WeaponDef};

/// What the General's third Zero Hour slot does when it fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Third {
    #[default]
    Lethal,
    Tactical,
    /// One knife strike, then back to the offhand that was chosen.
    Melee,
}

impl Third {
    /// The order's wire value (`zhbo_logic_rts_offhand`'s `choice`, `RtsWeapons::third`).
    pub fn from_wire(v: i32) -> Third {
        match v {
            1 => Third::Tactical,
            2 => Third::Melee,
            _ => Third::Lethal,
        }
    }

    pub fn wire(self) -> i32 {
        match self {
            Third::Lethal => 0,
            Third::Tactical => 1,
            Third::Melee => 2,
        }
    }
}

/// A General's RTS state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtsState {
    /// The Zero Hour weapon slot to lock to (the weapon that was in hand when first person ended, or
    /// one with ammo when the current one ran dry), or -1.
    pub lock: i32,
    /// Whether the offhand orders throw the tactical (else the lethal); kept while a melee order
    /// borrows the slot.
    pub tactical: bool,
    /// A melee order is pending on the third slot.
    pub melee: bool,
    /// The gun slot to go back to after the third slot's order.
    pub gun: i32,
}

impl Default for RtsState {
    fn default() -> Self {
        RtsState { lock: -1, tactical: false, melee: false, gun: 0 }
    }
}

impl RtsState {
    pub fn third(&self) -> Third {
        if self.melee {
            Third::Melee
        } else if self.tactical {
            Third::Tactical
        } else {
            Third::Lethal
        }
    }
}

impl Sim {
    pub fn rts_update(&mut self, host: &dyn LogicHost, player: i32, general: u32, current_slot: i32, shots: [u32; 3], target: Option<Vec3>) -> RtsWeapons {
        let Ok(idx) = usize::try_from(player) else { return RtsWeapons::default() };
        if idx >= MAX_PLAYERS {
            return RtsWeapons::default();
        }
        if self.players[idx].as_ref().is_none_or(|p| p.general != general && !p.is_first_person()) {
            let Some(info) = host.object(general, general) else { return RtsWeapons::default() };
            let frame = self.frame;
            self.players[idx] = Some(PlayerSim {
                general,
                class: None,
                reticle: String::new(),
                reticle_color: String::new(),
                camo: String::new(),
                mode: Mode::Rts,
                body: Body::new(info.pos, &self.tuning),
                yaw: info.yaw,
                pitch: 0.0,
                // A classless General has no guns to fire: report nothing usable until a class
                // lands (the bridge's empty-weapon veto stops Zero Hour's own firing).
                arsenal: Arsenal::unarmed(),
                rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                input: FpsInput { yaw: info.yaw, ..Default::default() },
                pressed_accum: 0,
                health: info.health,
                max_health: info.max_health,
                regen_wait: 0.0,
                hack: None,
                rts: RtsState::default(),
                hammer: None,
                site: None,
                building: false,
                tracer_count: 0,
            });
        }
        let Some(mut p) = self.players[idx].take() else { return RtsWeapons::default() };
        if p.general != general {
            self.players[idx] = Some(p);
            return RtsWeapons::default();
        }
        if p.mode != Mode::Rts {
            let out = rts_weapons(&p.arsenal, -1, &p.rts);
            self.players[idx] = Some(p);
            return out;
        }
        for (slot, &n) in shots.iter().enumerate().take(2) {
            for _ in 0..n {
                p.arsenal.rts_shot(slot);
            }
        }
        // The third slot carries out the order it was given (one shot per order: the buttons fire
        // it once).
        let mut uses: Vec<&'static WeaponDef> = Vec::new();
        let mut strikes = 0;
        for _ in 0..shots[2] {
            if p.rts.melee {
                strikes += 1;
                continue;
            }
            let lethal = !p.rts.tactical;
            let off = if lethal { &p.arsenal.lethal } else { &p.arsenal.tactical };
            // The Black Hat needs its target held in view: no overhead order uses it.
            if off.count > 0 && off.def.place != Place::Hack {
                uses.push(off.def);
                p.arsenal.use_offhand(lethal);
            }
        }
        if (0..2).contains(&current_slot) {
            p.rts.gun = current_slot;
        }
        if (0..p.arsenal.weapons.len() as i32).contains(&current_slot) {
            p.arsenal.active = current_slot as usize;
        }
        let mut lock = std::mem::replace(&mut p.rts.lock, -1);
        if !uses.is_empty() || strikes > 0 {
            lock = p.rts.gun; // back to the gun after the order
        }
        if strikes > 0 {
            p.rts.melee = false;
        }
        let a = &p.arsenal;
        let current = if lock >= 0 { lock } else { current_slot };
        let empty = |s: i32| if s == 2 { third_left(a, &p.rts) == 0 } else { a.rts_ammo(s as usize).0 + a.rts_ammo(s as usize).1 == 0 };
        if (0..3).contains(&current) && empty(current) {
            // Out of ammo: the first gun that has some (the third slot only by order).
            lock = (0..a.weapons.len().min(2)).find(|&s| a.rts_ammo(s).0 + a.rts_ammo(s).1 > 0).map_or(-1, |s| s as i32);
        }
        if let Some(info) = host.object(general, general) {
            // He acts from where Zero Hour put him, facing what he was ordered at.
            p.body.pos = info.pos;
            p.yaw = target.map_or(info.yaw, |t| {
                let to = t - info.pos;
                if to.length_xy() > 1e-3 { to.y.atan2(to.x) } else { info.yaw }
            });
            for _ in 0..strikes {
                let damage = p.arsenal.melee.melee_damage;
                self.melee_strike(host, idx, &mut p, damage, MeleeReach::Footprint(info.major_radius));
            }
            for def in uses {
                self.rts_offhand_use(host, idx, &mut p, def, target);
            }
        }
        let out = rts_weapons(&p.arsenal, lock, &p.rts);
        self.players[idx] = Some(p);
        out
    }

    /// One offhand from the third slot: a grenade thrown on its arc to the target, as first person
    /// throws it (its fuse, bounces and blast), or equipment planted in front of him facing it.
    fn rts_offhand_use(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, def: &'static WeaponDef, target: Option<Vec3>) {
        if def.place == Place::Plant {
            self.apply_weapon_action(host, idx, p, WeaponAction::Plant { def });
            return;
        }
        let from = p.body.pos + Vec3::new(0.0, 0.0, p.body.eye_height * 0.9);
        let to = target.unwrap_or(from);
        let speed = if def.projectile_speed > 0.0 { def.projectile_speed } else { crate::units::inches(800.0) };
        let vel = throw_velocity(from, to, speed);
        let mut proj = Projectile::launch(def, p.general, from, vel.normalized_or_zero(), def.fuse_time, host, p.yaw);
        proj.vel = vel;
        (proj.self_blast_scale, proj.self_stun_scale) = p.arsenal.self_harm_scales();
        self.projectiles.push((idx, proj));
        self.emit(idx, EventKind::Thrown { lethal: def.class == Class::Lethal });
    }

    /// Under RTS control: what the third slot's next shot does — `choice` is [`Third`]'s wire value
    /// (0 the lethal, 1 the tactical, 2 one knife strike). Lockstep: from a game message.
    pub fn rts_offhand(&mut self, player: i32, general: u32, choice: i32) {
        let Some(p) = usize::try_from(player).ok().and_then(|i| self.players.get_mut(i)).and_then(|p| p.as_mut()) else { return };
        if p.general != general {
            return;
        }
        match Third::from_wire(choice) {
            Third::Melee => p.rts.melee = true,
            t => {
                p.rts.melee = false;
                p.rts.tactical = t == Third::Tactical;
            }
        }
    }

    /// Under RTS control: reload the gun in hand (the slot Zero Hour reports, or the one he goes
    /// back to). Tops its magazine up from its spare rounds, as a first-person reload ends, and
    /// returns the slot reloaded, or -1 when there was nothing to do (full, or no spare rounds).
    /// Lockstep: from a game message; the bridge starts Zero Hour's reload for the time.
    pub fn rts_reload(&mut self, player: i32, general: u32, current_slot: i32) -> i32 {
        let Some(p) = usize::try_from(player).ok().and_then(|i| self.players.get_mut(i)).and_then(|p| p.as_mut()) else { return -1 };
        if p.general != general || p.mode != Mode::Rts {
            return -1;
        }
        let slot = if (0..2).contains(&current_slot) { current_slot } else { p.rts.gun };
        let Some(w) = p.arsenal.weapons.get_mut(slot as usize).filter(|w| w.def.class.is_gun()) else { return -1 };
        let take = w.def.clip.saturating_sub(w.ammo).min(w.reserve);
        if take == 0 {
            return -1;
        }
        w.ammo += take;
        w.reserve -= take;
        w.reload = None;
        slot
    }

    /// The ammo a General's weapons have, for the control bar (the loadout's when the General has
    /// no arsenal yet).
    pub fn rts_ammo(&self, general: u32) -> RtsWeapons {
        match self.players.iter().flatten().find(|p| p.general == general) {
            Some(p) => rts_weapons(&p.arsenal, -1, &p.rts),
            // No record yet: the General is still classless, so he has no usable weapon (before
            // the first `rts_update` created the record). Never report the stock kit.
            None => rts_weapons(&Arsenal::unarmed(), -1, &RtsState::default()),
        }
    }
}

/// The BO2 icons of a General's kit for his command bar, by [`RtsIcons::PRIMARY`] .. [`RtsIcons::KNIFE`]
/// (`classicons::weapon_icon` names; empty while he is classless). The bridge draws them over the
/// plain tiles of his gun and order buttons each frame: his class is the player's, so no
/// conversion-time button art can show it (the same reason the class buttons draw their kits).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtsIcons {
    pub icons: [[u8; crate::names::RTS_WEAPON_NAME_CAP]; 5],
}

impl RtsIcons {
    pub const PRIMARY: usize = 0;
    pub const SECONDARY: usize = 1;
    pub const LETHAL: usize = 2;
    pub const TACTICAL: usize = 3;
    pub const KNIFE: usize = 4;

    pub fn icon(&self, which: usize) -> Option<&str> {
        let t = self.icons.get(which)?;
        let len = t.iter().position(|&b| b == 0).unwrap_or(t.len());
        std::str::from_utf8(&t[..len]).ok().filter(|s| !s.is_empty())
    }
}

impl Default for RtsIcons {
    fn default() -> Self {
        RtsIcons { icons: [[0; crate::names::RTS_WEAPON_NAME_CAP]; 5] }
    }
}

impl Sim {
    /// The kit icons for a General's command bar ([`RtsIcons`]).
    pub fn rts_icons(&self, general: u32) -> RtsIcons {
        let mut out = RtsIcons::default();
        let Some(a) = self.players.iter().flatten().find(|p| p.general == general).map(|p| &p.arsenal).filter(|a| !a.is_unarmed()) else {
            return out;
        };
        let gun = |s: usize| a.weapons.get(s).map(|w| w.def).filter(|d| d.class.is_gun());
        let defs = [gun(0), gun(1), Some(a.lethal.def), Some(a.tactical.def), Some(a.melee)];
        for (slot, def) in defs.into_iter().enumerate() {
            let Some(def) = def else { continue };
            let name = crate::classicons::weapon_icon(&def.id);
            if name.len() < crate::names::RTS_WEAPON_NAME_CAP {
                out.icons[slot][..name.len()].copy_from_slice(name.as_bytes());
            }
        }
        out
    }
}

/// What the third slot has left for its current order: the chosen offhand's count, or one knife.
fn third_left(a: &Arsenal, rts: &RtsState) -> u16 {
    match rts.third() {
        Third::Melee => 1,
        Third::Tactical => a.tactical.count,
        Third::Lethal => a.lethal.count,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtsWeapons {
    pub ammo: [i32; 3],
    pub reserve: [i32; 3],
    pub lock: i32,
    /// Tactical grenades left, and whether the offhand orders throw them (else the lethal).
    pub tactical: i32,
    pub tactical_selected: i32,
    /// What the third slot does now ([`Third`]'s wire value). Appended in ABI 22.
    pub third: i32,
    /// Lethals left (`ammo[2]` counts whichever offhand the third slot holds). Appended in ABI 22.
    pub lethal: i32,
    /// Which third-slot orders can be given now, by [`Third`] wire value: bit 0 the lethal, 1 the
    /// tactical, 2 the knife (an offhand with some left that an overhead order can use; the knife
    /// once classed). The command bar greys the others out. Appended in ABI 22.
    pub orders: i32,
    /// The Zero Hour weapon each slot must hold (`names::rts_weapon`, NUL-terminated), or empty to
    /// keep the one it has (a classless General keeps the stand-ins). Appended in ABI 22.
    pub templates: [[u8; crate::names::RTS_WEAPON_NAME_CAP]; 3],
}

impl Default for RtsWeapons {
    fn default() -> Self {
        RtsWeapons {
            ammo: [0; 3],
            reserve: [0; 3],
            lock: 0,
            tactical: 0,
            tactical_selected: 0,
            third: 0,
            lethal: 0,
            orders: 0,
            templates: [[0; crate::names::RTS_WEAPON_NAME_CAP]; 3],
        }
    }
}

impl RtsWeapons {
    /// The weapon slot `slot` must hold, if the core names one.
    pub fn template(&self, slot: usize) -> Option<&str> {
        let t = self.templates.get(slot)?;
        let len = t.iter().position(|&b| b == 0).unwrap_or(t.len());
        std::str::from_utf8(&t[..len]).ok().filter(|s| !s.is_empty())
    }
}

fn rts_weapons(a: &Arsenal, lock: i32, rts: &RtsState) -> RtsWeapons {
    let mut out = RtsWeapons {
        lock,
        tactical: a.tactical.count as i32,
        tactical_selected: rts.tactical as i32,
        third: rts.third().wire(),
        lethal: a.lethal.count as i32,
        ..Default::default()
    };
    for s in 0..2 {
        let (ammo, reserve) = a.rts_ammo(s);
        out.ammo[s] = ammo as i32;
        out.reserve[s] = reserve as i32;
    }
    out.ammo[2] = third_left(a, rts) as i32;
    // A classless General (knife only) keeps Zero Hour's stand-ins, which the empty-ammo veto holds,
    // and has no orders to give.
    if a.is_unarmed() {
        return out;
    }
    let usable = |o: &crate::weapons::Offhand| o.count > 0 && o.def.place != Place::Hack;
    out.orders = (usable(&a.lethal) as i32) << Third::Lethal.wire() | (usable(&a.tactical) as i32) << Third::Tactical.wire() | 1 << Third::Melee.wire();
    let third = match rts.third() {
        Third::Melee => a.melee,
        Third::Tactical => a.tactical.def,
        Third::Lethal => a.lethal.def,
    };
    let guns = (0..2).map(|s| a.weapons.get(s).map(|w| w.def).filter(|d| d.class.is_gun()));
    for (slot, def) in guns.chain(std::iter::once(Some(third))).enumerate() {
        let Some(def) = def else { continue };
        let name = crate::names::rts_weapon(&def.id);
        if name.len() < crate::names::RTS_WEAPON_NAME_CAP {
            out.templates[slot][..name.len()].copy_from_slice(name.as_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::Kind as EquipKind;
    use crate::sim::tests::{general, soldier, FakeHost};
    use crate::weapons::{Effect, Offhand, MELEE_DAMAGE};

    /// A General (id 1, player 0) under RTS control with the first preset class.
    fn classed(host: &FakeHost) -> Sim {
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.rts_update(host, 0, 1, 0, [0, 0, 0], None);
        sim.choose_class(0, 1, 0, &[]);
        sim
    }

    fn arsenal(sim: &mut Sim) -> &mut Arsenal {
        &mut sim.players[0].as_mut().unwrap().arsenal
    }

    #[test]
    fn each_slot_names_the_zero_hour_weapon_of_the_kit() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let bare = sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        assert!((0..3).all(|s| bare.template(s).is_none()), "a classless General keeps the stand-ins");
        sim.choose_class(0, 1, 0, &[]);
        let a = arsenal(&mut sim).clone();
        let r = sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        assert_eq!(r.template(0), Some(crate::names::rts_weapon(&a.weapons[0].def.id).as_str()));
        assert_eq!(r.template(1), Some(crate::names::rts_weapon(&a.weapons[1].def.id).as_str()));
        assert_eq!(r.template(2), Some(crate::names::rts_weapon(&a.lethal.def.id).as_str()), "the lethal by default");
        sim.rts_offhand(0, 1, Third::Tactical.wire());
        assert_eq!(sim.rts_ammo(1).template(2), Some(crate::names::rts_weapon(&a.tactical.def.id).as_str()));
        sim.rts_offhand(0, 1, Third::Melee.wire());
        let r = sim.rts_ammo(1);
        assert_eq!((r.third, r.template(2)), (Third::Melee.wire(), Some(crate::names::rts_weapon(&a.melee.id).as_str())));
        assert_eq!(r.ammo[2], 1, "the knife never runs dry, so the bridge's empty veto leaves it alone");
    }

    #[test]
    fn attachments_share_the_base_weapons_rts_weapon() {
        assert_eq!(crate::names::rts_weapon("an94_mp+gl+reflex"), "ZHBO_GeneralWeapon_an94_mp");
        assert_eq!(crate::names::rts_weapon("an94_mp+gl~gl"), "ZHBO_GeneralWeapon_an94_mp");
    }

    #[test]
    fn a_lethal_order_throws_once_and_goes_back_to_the_gun() {
        let host = FakeHost::default();
        let mut sim = classed(&host);
        let before = sim.rts_ammo(1);
        let r = sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(Vec3::new(120.0, 0.0, 0.0)));
        assert_eq!(r.ammo[2], before.ammo[2] - 1);
        assert_eq!(r.lock, 0, "back to the primary after the throw");
        assert_eq!(sim.projectiles.len(), 1);
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::Thrown { lethal: true })), "thrown as first person throws");
        let p = sim.player(0).unwrap();
        assert!(p.yaw.abs() < 1e-4, "he faces what he threw at");
    }

    #[test]
    fn a_planted_lethal_goes_down_in_front_of_him_facing_the_order() {
        let host = FakeHost::default();
        let mut sim = classed(&host);
        let claymore: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            id: "claymore_mp".into(),
            class: Class::Equipment,
            effect: Effect::Explode,
            place: Place::Plant,
            equipment_kind: EquipKind::Claymore,
            ..Default::default()
        }));
        arsenal(&mut sim).lethal = Offhand { def: claymore, count: 2 };
        assert_eq!(sim.rts_ammo(1).template(2), Some("ZHBO_GeneralWeapon_claymore_mp"));
        let r = sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(Vec3::new(0.0, 50.0, 0.0)));
        assert_eq!(r.ammo[2], 1);
        assert!(sim.projectiles.is_empty(), "planted, not thrown");
        let placed = &sim.equipment.placed[0];
        assert_eq!(placed.kind, EquipKind::Claymore);
        assert!((placed.pos - Vec3::new(0.0, PLANT_DISTANCE, 0.0)).length() < 1e-3, "in front of him: {:?}", placed.pos);
        assert!((placed.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "facing the order");
    }

    #[test]
    fn a_melee_order_strikes_the_victim_zero_hour_closed_in_on() {
        let host = FakeHost::default();
        let mut sim = classed(&host);
        // Zero Hour's reach is edge to edge: the knife's range plus both footprints (10 each).
        let reach = MELEE_RANGE + 10.0 + 10.0;
        host.add(soldier(2, 1, reach - 0.5));
        let grenades = sim.rts_ammo(1).ammo[2];
        sim.rts_offhand(0, 1, Third::Melee.wire());
        let r = sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(host.get(2).pos));
        assert_eq!(host.damage_log.borrow().as_slice(), &[(1, 2, MELEE_DAMAGE * BO2_TO_ZH_DAMAGE)]);
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::Melee)));
        assert_eq!((r.third, r.ammo[2], r.lock), (Third::Lethal.wire(), grenades, 0), "one strike, then the lethal again; no grenade spent");
    }

    #[test]
    fn a_melee_order_out_of_reach_hits_nothing() {
        let host = FakeHost::default();
        let mut sim = classed(&host);
        host.add(soldier(2, 1, MELEE_RANGE + 10.0 + 10.0 + 5.0));
        sim.rts_offhand(0, 1, Third::Melee.wire());
        sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(host.get(2).pos));
        assert!(host.damage_log.borrow().is_empty());
    }

    #[test]
    fn the_reload_order_tops_up_the_gun_in_hand() {
        let host = FakeHost::default();
        let mut sim = classed(&host);
        let full = sim.rts_ammo(1);
        sim.rts_update(&host, 0, 1, 0, [5, 0, 0], None);
        assert_eq!(sim.rts_reload(0, 1, 0), 0);
        let r = sim.rts_ammo(1);
        assert_eq!((r.ammo[0], r.reserve[0]), (full.ammo[0], full.reserve[0] - 5));
        assert_eq!(sim.rts_reload(0, 1, 0), -1, "a full magazine has nothing to reload");
        // The third slot is not a gun: the order reloads the gun he goes back to.
        sim.rts_update(&host, 0, 1, 1, [0, 3, 0], None);
        assert_eq!(sim.rts_reload(0, 1, 2), 1);
    }

    #[test]
    fn the_command_bar_offers_only_the_orders_he_can_carry_out() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        assert_eq!(sim.rts_ammo(1).orders, 0, "no orders before a class");
        sim.choose_class(0, 1, 0, &[]);
        let r = sim.rts_ammo(1);
        assert_eq!((r.orders, r.lethal), (0b111, arsenal(&mut sim).lethal.count as i32));
        arsenal(&mut sim).lethal.count = 0;
        let black_hat: &'static WeaponDef = Box::leak(Box::new(WeaponDef { id: "pda_hack_mp".into(), class: Class::Tactical, place: Place::Hack, ..Default::default() }));
        arsenal(&mut sim).tactical = Offhand { def: black_hat, count: 1 };
        assert_eq!(sim.rts_ammo(1).orders, 1 << Third::Melee.wire(), "no lethals left, and the Black Hat needs first person");
    }

    #[test]
    fn the_command_bar_draws_his_own_kit() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        assert!((0..5).all(|i| sim.rts_icons(1).icon(i).is_none()), "nothing before a class");
        sim.choose_class(0, 1, 0, &[]);
        let a = arsenal(&mut sim).clone();
        let icons = sim.rts_icons(1);
        let want = |id: &str| crate::classicons::weapon_icon(id);
        assert_eq!(icons.icon(RtsIcons::PRIMARY), Some(want(&a.weapons[0].def.id).as_str()));
        assert_eq!(icons.icon(RtsIcons::SECONDARY), Some(want(&a.weapons[1].def.id).as_str()));
        assert_eq!(icons.icon(RtsIcons::LETHAL), Some(want(&a.lethal.def.id).as_str()));
        assert_eq!(icons.icon(RtsIcons::TACTICAL), Some(want(&a.tactical.def.id).as_str()));
        assert_eq!(icons.icon(RtsIcons::KNIFE), Some(want(&a.melee.id).as_str()));
    }

    #[test]
    fn orders_for_another_general_are_ignored() {
        let host = FakeHost::default();
        let mut sim = classed(&host);
        sim.rts_offhand(0, 7, Third::Melee.wire());
        assert_eq!(sim.rts_ammo(1).third, Third::Lethal.wire());
        assert_eq!(sim.rts_reload(0, 7, 0), -1);
    }
}
