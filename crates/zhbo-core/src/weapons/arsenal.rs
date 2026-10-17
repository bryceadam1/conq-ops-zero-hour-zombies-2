/*
** Conquer Ops - Zero Hour gameplay core: the first-person weapon state machine: raise, fire, reload, offhands, melee.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::catalog::{catalog, weapon};
use super::def::{Class, Effect, FireMode, Place, WeaponDef, ALT_CLIPS, KICK_TO_DEGREES, MELEE_DAMAGE, MELEE_FX_WEAPON, MELEE_RANGE};
use super::table::{fire_name, parse_fire};
use super::loadout::Loadout;
use super::perks::Perk;
use super::rng::Rng;
use crate::attachments;
use crate::input::buttons;
use crate::units::{inches, LOGIC_DT};


/// A reload in progress.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reload {
    /// Magazine swap: seconds in, total length, when the rounds go in, whether they have.
    Magazine { t: f32, length: f32, add_at: f32, added: bool, empty: bool },
    /// Tube shotgun: start, then one cycle per batch of shells, then end.
    Shells { phase: ShellPhase, t: f32, stop: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellPhase {
    Start,
    Insert,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponState {
    /// What is up now: the weapon, or its alt mode.
    pub def: &'static WeaponDef,
    /// The weapon itself (with its attachments), whichever mode is up.
    pub main: &'static WeaponDef,
    pub ammo: u16,
    pub reserve: u16,
    /// Seconds until the next round may fire (can go negative within a frame for high RPM).
    pub cooldown: f32,
    pub reload: Option<Reload>,
    pub burst_left: u8,
    /// Hip spread bloom above the stance minimum, degrees.
    pub bloom: f32,
    /// Minigun barrel spin, seconds toward `spin_up_time`.
    pub spin: f32,
    /// Has been raised before (the first raise is BO2's longer "first raise").
    pub raised_once: bool,
    /// The alt mode's own ammo while the main mode is up (and the main mode's while the alt is).
    pub alt_ammo: u16,
    pub alt_reserve: u16,
    /// Shots in the current string of fire (the grip's recoil reduction builds over it).
    pub string: u16,
}

impl WeaponState {
    pub fn new(def: &'static WeaponDef) -> Self {
        Self {
            def,
            main: def,
            ammo: def.clip,
            reserve: def.start_reserve,
            cooldown: 0.0,
            reload: None,
            burst_left: 0,
            bloom: 0.0,
            spin: 0.0,
            raised_once: false,
            alt_ammo: 0,
            alt_reserve: 0,
            string: 0,
        }
        .with_alt_ammo()
    }

    /// Load the alt mode's own magazine (an under-barrel launcher's grenades).
    fn with_alt_ammo(mut self) -> Self {
        if let Some(alt) = catalog().get(&self.def.alt).filter(|a| a.alt_own_ammo) {
            self.alt_ammo = alt.clip;
            self.alt_reserve = alt.start_reserve;
        }
        self
    }

    pub fn is_reloading(&self) -> bool {
        self.reload.is_some()
    }

    /// Seconds left on a magazine reload (0 when none; shell reloads report their current phase).
    pub fn reload_left(&self) -> f32 {
        match self.reload {
            Some(Reload::Magazine { t, length, .. }) => (length - t).max(0.0),
            Some(Reload::Shells { .. }) => self.def.reload_time,
            None => 0.0,
        }
    }

    pub fn can_reload(&self) -> bool {
        !self.is_reloading() && self.ammo < self.def.clip && self.reserve > 0
    }

    fn start_reload(&mut self, speed: f32) {
        if !self.can_reload() {
            return;
        }
        let d = self.def;
        self.burst_left = 0;
        self.reload = Some(if d.shell_by_shell() {
            Reload::Shells { phase: ShellPhase::Start, t: 0.0, stop: false }
        } else {
            let empty = self.ammo == 0;
            let (length, add_at) = if empty { (d.reload_empty_time, d.reload_empty_add_time) } else { (d.reload_time, d.reload_add_time) };
            let add_at = if add_at > 0.0 { add_at } else { length };
            Reload::Magazine { t: 0.0, length: length / speed, add_at: add_at.min(length) / speed, added: false, empty }
        });
    }

    fn take(&mut self, n: u16) {
        let take = n.min(self.def.clip - self.ammo).min(self.reserve);
        self.ammo += take;
        self.reserve -= take;
    }

    /// Advance a reload; returns whether it finished this frame.
    fn step_reload(&mut self, dt: f32) -> bool {
        let d = self.def;
        let Some(r) = self.reload else { return false };
        match r {
            Reload::Magazine { t, length, add_at, added, empty } => {
                let t = t + dt;
                let mut added = added;
                if !added && t >= add_at {
                    self.take(d.clip);
                    added = true;
                }
                if t >= length {
                    self.reload = None;
                    return true;
                }
                self.reload = Some(Reload::Magazine { t, length, add_at, added, empty });
            }
            Reload::Shells { phase, t, stop } => {
                let t = t + dt;
                match phase {
                    ShellPhase::Start if t >= d.reload_start_time => self.reload = Some(Reload::Shells { phase: ShellPhase::Insert, t: 0.0, stop }),
                    ShellPhase::Insert if t >= d.reload_time => {
                        self.take(d.reload_ammo_add.max(1));
                        let full = self.ammo >= d.clip || self.reserve == 0;
                        let next = if full || stop { ShellPhase::End } else { ShellPhase::Insert };
                        self.reload = Some(Reload::Shells { phase: next, t: 0.0, stop });
                    }
                    ShellPhase::End if t >= d.reload_end_time => {
                        self.reload = None;
                        return true;
                    }
                    _ => self.reload = Some(Reload::Shells { phase, t, stop }),
                }
            }
        }
        false
    }

    /// Hip cone half-angle (degrees) for a stance (0 stand, 1 crouch, 2 prone) and ADS fraction.
    pub fn spread_deg(&self, stance: usize, ads: f32) -> f32 {
        let (min, max) = self.def.hip_spread[stance.min(2)];
        let hip = (min + self.bloom).min(max);
        hip + (self.def.ads_spread - hip) * ads
    }
}

/// A grenade or other offhand item and how many are left.
#[derive(Clone, Debug, PartialEq)]
pub struct Offhand {
    pub def: &'static WeaponDef,
    pub count: u16,
}

/// What the weapon layer wants the logic to do this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponAction {
    /// Fire one round (one pellet for shotguns) with the given cone (degrees), random offsets in
    /// [-1, 1) and view kick (degrees pitch up, yaw).
    Fire { spread_deg: f32, jitter: (f32, f32), recoil: (f32, f32), pellet: u16 },
    /// Launch a projectile (launchers, crossbow, ballistic knife); `pellet` numbers the bolts of
    /// one shot (the tri-bolt's three).
    Launch { def: &'static WeaponDef, pellet: u16 },
    /// Throw an offhand item; `fuse_left` is what remains after cooking.
    Throw { def: &'static WeaponDef, fuse_left: f32 },
    /// Cooked too long: it goes off in hand.
    CookedOff { def: &'static WeaponDef },
    /// Pin pulled (the throw animation's wind-up starts).
    OffhandReady { lethal: bool },
    /// Set equipment down in front of the player (Claymore, Bouncing Betty, Trophy System,
    /// Tactical Insertion).
    Plant { def: &'static WeaponDef },
    /// Set off the player's C4 (double-tap reload, or the lethal button with none left in hand).
    Detonate,
    /// Switched to or from the weapon's alt mode.
    AltToggled,
    Melee { damage: f32 },
    DryFire,
    ReloadStarted,
    ReloadFinished,
    Swapped,
}

/// Which raise animation is playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaiseKind {
    First,
    Normal,
    Quick,
    /// Into or out of an alt mode (BO2's alt raise).
    Alt,
}

/// Offhand use in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OffhandUse {
    /// Pin pulled / wound up and held (`cooked` seconds of fuse used for cooking grenades).
    Holding { lethal: bool, held_for: f32 },
    /// Released: the throw plays out before the weapon comes back.
    Throwing { lethal: bool, t: f32 },
}

/// A second reload press within this many seconds sets off C4.
pub const DETONATE_DOUBLE_TAP: f32 = 0.35;

/// Seconds of BO2's offhand pull-back before a throw can leave the hand, and the throw itself.
pub const OFFHAND_PULLBACK: f32 = 0.35;
pub const OFFHAND_THROW: f32 = 0.45;

/// BO2 melee: a lunge up to this range (inches 64 plus the lunge), one hit on infantry.

#[derive(Clone, Debug, PartialEq)]
pub struct Arsenal {
    pub loadout: Loadout,
    pub weapons: Vec<WeaponState>,
    pub active: usize,
    pub lethal: Offhand,
    pub tactical: Offhand,
    pub melee: &'static WeaponDef,
    /// Raising the active weapon (first raise, swap, after an offhand), out of `raise_total`.
    pub raise_left: f32,
    pub raise_total: f32,
    pub raise_kind: RaiseKind,
    /// Putting the previous weapon away before `raise_left` starts, out of `drop_total`.
    pub drop_left: f32,
    pub drop_total: f32,
    /// The weapon being put away.
    pub previous: usize,
    pub ads: f32,
    /// Seconds since sprinting stopped (weapon comes back up over `sprint_out_time`).
    pub since_sprint: f32,
    pub melee_left: f32,
    pub offhand: Option<OffhandUse>,
    held_last: u32,
    /// Seconds since reload was last pressed (a second press soon after detonates C4).
    since_reload_press: f32,
}

impl Arsenal {
    pub fn new() -> Self {
        Self::with_loadout(Loadout::default())
    }

    pub fn with_loadout(loadout: Loadout) -> Self {
        let mut weapons: Vec<WeaponState> = [&loadout.primary, &loadout.secondary].iter().map(|id| WeaponState::new(weapon(id))).collect();
        for (w, r) in weapons.iter_mut().zip(loadout.reserve) {
            if let Some(r) = r {
                w.reserve = r;
            }
        }
        if loadout.perks.contains(&Perk::SCAVENGER) {
            for w in &mut weapons {
                w.reserve = (w.reserve + w.def.clip).min(w.def.max_reserve.max(w.reserve));
            }
        }
        // Offhands start with BO2's start count (`start_reserve` for them).
        let offhand = |id: &str| {
            let def = weapon(id);
            Offhand { def, count: def.start_reserve.max(1) }
        };
        let raise = weapons[0].def.first_raise_time;
        weapons[0].raised_once = true;
        Self {
            lethal: offhand(&loadout.lethal),
            tactical: offhand(&loadout.tactical),
            melee: weapon(&loadout.melee),
            loadout,
            weapons,
            active: 0,
            raise_left: raise,
            raise_total: raise,
            raise_kind: RaiseKind::First,
            drop_left: 0.0,
            drop_total: 0.0,
            previous: 0,
            ads: 0.0,
            since_sprint: 10.0,
            melee_left: 0.0,
            offhand: None,
            held_last: 0,
            since_reload_press: 10.0,
        }
    }

    pub fn has(&self, perk: Perk) -> bool {
        self.loadout.perks.contains(&perk)
    }

    pub fn current(&self) -> &WeaponState {
        &self.weapons[self.active]
    }

    pub fn current_mut(&mut self) -> &mut WeaponState {
        &mut self.weapons[self.active]
    }

    /// The weapon can't shoot right now (raising, putting away, reloading, meleeing, throwing).
    pub fn is_busy(&self) -> bool {
        self.raise_left > 0.0 || self.drop_left > 0.0 || self.current().is_reloading() || self.melee_left > 0.0 || self.offhand.is_some()
    }

    /// Movement speed multiplier from the held weapon, ADS and perks.
    pub fn move_scale(&self) -> f32 {
        let d = self.current().def;
        let base = d.move_scale + (d.ads_move_scale - d.move_scale) * self.ads;
        base * if self.has(Perk::LIGHTWEIGHT) { Perk::LIGHTWEIGHT.magnitude() } else { 1.0 }
    }

    /// Sprint duration multiplier from the held weapon and perks.
    pub fn sprint_scale(&self) -> f32 {
        let d = self.current().def.sprint_duration_scale.max(0.1);
        d * if self.has(Perk::EXTREME_CONDITIONING) { Perk::EXTREME_CONDITIONING.magnitude() } else { 1.0 }
    }

    /// Field of view multiplier when fully aimed (relative to BO2's 65 degrees).
    pub fn ads_fov_scale(&self) -> f32 {
        let fov = self.current().def.ads_fov;
        if fov > 0.0 {
            (fov / 65.0).clamp(0.1, 1.0)
        } else {
            1.0
        }
    }

    fn sprint_out_time(&self) -> f32 {
        self.current().def.sprint_out_time * if self.has(Perk::DEXTERITY) { Perk::DEXTERITY.magnitude() } else { 1.0 }
    }

    fn switch_to(&mut self, index: usize) {
        let fast = self.has(Perk::FAST_HANDS);
        let from = self.current().def;
        self.current_mut().reload = None; // swapping cancels a reload, like BO2
        self.current_mut().burst_left = 0;
        self.drop_left = if fast { from.quick_drop_time } else { from.drop_time };
        self.drop_total = self.drop_left;
        self.previous = self.active;
        self.active = index;
        let to = self.current().def;
        let first = !self.current().raised_once;
        self.current_mut().raised_once = true;
        (self.raise_left, self.raise_kind) = if first {
            (to.first_raise_time, RaiseKind::First)
        } else if fast {
            (to.quick_raise_time, RaiseKind::Quick)
        } else {
            (to.raise_time, RaiseKind::Normal)
        };
        self.raise_total = self.raise_left;
        self.ads = 0.0;
    }

    /// Switch the held weapon to its alt mode or back. An alt mode with its own ammo (a launcher)
    /// trades magazines with the weapon; the others share it.
    pub fn toggle_alt(&mut self) {
        let fast = self.has(Perk::FAST_HANDS);
        let w = self.current_mut();
        let to = if w.def.id == w.main.id {
            match catalog().get(&w.main.alt) {
                Some(alt) => alt,
                None => return,
            }
        } else {
            w.main
        };
        if to.id == w.def.id {
            return;
        }
        if to.alt_own_ammo || w.def.alt_own_ammo {
            std::mem::swap(&mut w.ammo, &mut w.alt_ammo);
            std::mem::swap(&mut w.reserve, &mut w.alt_reserve);
        }
        w.def = to;
        w.reload = None;
        w.burst_left = 0;
        w.cooldown = w.cooldown.max(0.0);
        w.string = 0;
        self.ads = 0.0;
        // BO2 raises an alt toggle in max(quickRaiseTime, raiseTime / 2) — the quick raise when it
        // is longer, never a full raise (refactor audit 0.4, docs/REFACTOR-PLAN.md); Fast Hands
        // switches use quickRaiseTime outright, like every other switch in switch_to().
        let t = if fast { to.quick_raise_time } else { to.quick_raise_time.max(to.raise_time * 0.5) };
        self.raise_left = t;
        self.raise_total = t;
        self.raise_kind = RaiseKind::Alt;
    }

    /// A shot from the Zero Hour weapon in `slot` (0 primary, 1 secondary, 2 lethal) while the
    /// General is under RTS control: it comes out of the magazine, which Zero Hour's own clip
    /// reload refills from the spare rounds.
    pub fn rts_shot(&mut self, slot: usize) {
        if slot == 2 {
            self.lethal.count = self.lethal.count.saturating_sub(1);
            return;
        }
        let Some(w) = self.weapons.get_mut(slot) else { return };
        w.reload = None;
        if w.ammo == 0 {
            let take = w.def.clip.min(w.reserve);
            w.ammo += take;
            w.reserve -= take;
        }
        w.ammo = w.ammo.saturating_sub(1);
    }

    /// Rounds in the magazine and spare for RTS slot `slot` (grenades for the lethal).
    pub fn rts_ammo(&self, slot: usize) -> (u16, u16) {
        match slot {
            2 => (self.lethal.count, 0),
            s => self.weapons.get(s).map_or((0, 0), |w| (w.ammo, w.reserve)),
        }
    }

    /// Give a magazine (Scavenger) to every gun.
    pub fn scavenge(&mut self) {
        for w in &mut self.weapons {
            w.reserve = (w.reserve + w.def.clip).min(w.def.max_reserve);
        }
        self.lethal.count = self.lethal.count.max(1);
    }

    /// Advance one logic frame. `stance` is 0 stand, 1 crouch, 2 prone; `speed` the ground speed.
    pub fn step(&mut self, held: u32, pressed: u32, sprinting: bool, stance: usize, speed: f32, rng: &mut Rng, out: &mut Vec<WeaponAction>) {
        let dt = LOGIC_DT;
        let pressed = pressed | (held & !self.held_last);
        let released = self.held_last & !held;
        self.held_last = held;

        self.since_sprint = if sprinting { 0.0 } else { self.since_sprint + dt };
        self.melee_left = (self.melee_left - dt).max(0.0);
        if self.drop_left > 0.0 {
            self.drop_left = (self.drop_left - dt).max(0.0);
        } else {
            self.raise_left = (self.raise_left - dt).max(0.0);
        }
        for w in &mut self.weapons {
            let decay = w.def.spread_decay.max(0.1);
            w.bloom = (w.bloom - decay * dt).max(0.0);
        }
        let reload_speed = 1.0;

        // Offhands: hold to pull back (and cook), release to throw.
        if self.step_offhand(held, pressed, released, out) {
            return;
        }

        // C4: a quick second press of reload sets it off (BO2's double tap).
        self.since_reload_press += dt;
        if pressed & buttons::RELOAD != 0 {
            if self.since_reload_press < DETONATE_DOUBLE_TAP {
                out.push(WeaponAction::Detonate);
            }
            self.since_reload_press = 0.0;
        }

        // Alt mode (select fire, grenade launcher, second optic).
        if pressed & buttons::ALT_MODE != 0 && !self.current().main.alt.is_empty() && self.melee_left <= 0.0 && self.drop_left <= 0.0 {
            self.toggle_alt();
            out.push(WeaponAction::AltToggled);
        }

        // Swap.
        if pressed & buttons::SWAP_WEAPON != 0 && self.weapons.len() > 1 && self.melee_left <= 0.0 {
            let next = (self.active + 1) % self.weapons.len();
            self.switch_to(next);
            out.push(WeaponAction::Swapped);
        }

        // Melee interrupts a reload and firing, not a raise.
        if pressed & buttons::MELEE != 0 && self.melee_left <= 0.0 && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            self.melee_left = self.melee.melee_time.max(0.3);
            let w = self.current_mut();
            w.burst_left = 0;
            w.reload = None;
            self.ads = 0.0;
            out.push(WeaponAction::Melee { damage: self.melee.melee_damage });
            return;
        }

        // Reload progress / requests.
        if self.current_mut().step_reload(dt) {
            out.push(WeaponAction::ReloadFinished);
        }
        if pressed & buttons::RELOAD != 0 && self.current().can_reload() && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            self.current_mut().start_reload(reload_speed);
            out.push(WeaponAction::ReloadStarted);
        }

        // ADS blend: BO2 aims in and out at the weapon's own rates, and not while sprinting or
        // until the weapon is back up from a sprint.
        let def = self.current().def;
        let sprint_ready = self.since_sprint >= self.sprint_out_time();
        let wants_ads = held & buttons::ADS != 0 && !sprinting && sprint_ready && self.raise_left <= 0.0 && self.drop_left <= 0.0 && !self.current().is_reloading();
        self.ads = if wants_ads {
            (self.ads + dt / def.ads_in_time.max(1e-3)).min(1.0)
        } else {
            (self.ads - dt / def.ads_out_time.max(1e-3)).max(0.0)
        };

        if def.class == Class::Melee || def.class.is_offhand() {
            return;
        }

        // Firing.
        let ads = self.ads;
        let blocked = sprinting || !sprint_ready || self.raise_left > 0.0 || self.drop_left > 0.0 || self.melee_left > 0.0;
        let w = self.current_mut();
        w.cooldown -= dt;
        // Minigun barrels spin while the trigger is held.
        if def.fire == FireMode::Minigun {
            w.spin = if held & buttons::FIRE != 0 && !blocked { (w.spin + dt).min(def.spin_up_time) } else { (w.spin - dt).max(0.0) };
        }
        // A tube-shotgun reload stops after the current shell when the trigger is pressed.
        if pressed & buttons::FIRE != 0 {
            if let Some(Reload::Shells { phase, t, .. }) = w.reload {
                if w.ammo > 0 {
                    w.reload = Some(Reload::Shells { phase, t, stop: true });
                }
            }
        }
        let trigger = match def.fire {
            FireMode::Auto => held & buttons::FIRE != 0,
            FireMode::Minigun => held & buttons::FIRE != 0 && w.spin >= def.spin_up_time,
            FireMode::Semi => pressed & buttons::FIRE != 0,
            FireMode::Burst(n) => {
                if pressed & buttons::FIRE != 0 && w.burst_left == 0 && w.cooldown <= 0.0 {
                    w.burst_left = n;
                }
                w.burst_left > 0
            }
        };
        if !trigger || blocked || w.is_reloading() {
            if w.cooldown < 0.0 {
                w.cooldown = 0.0;
            }
            if sprinting || w.is_reloading() {
                w.burst_left = 0;
            }
            return;
        }
        if w.ammo == 0 {
            w.burst_left = 0;
            if pressed & buttons::FIRE != 0 {
                if w.can_reload() {
                    w.start_reload(reload_speed);
                    out.push(WeaponAction::ReloadStarted);
                } else {
                    out.push(WeaponAction::DryFire);
                }
            }
            if w.cooldown < 0.0 {
                w.cooldown = 0.0;
            }
            return;
        }
        let kick = if ads > 0.5 { def.ads_kick } else { def.hip_kick };
        if !(held & buttons::FIRE != 0) {
            w.string = 0;
        }
        let mut fired_semi = false;
        while w.cooldown <= 0.0 && w.ammo > 0 {
            if def.fire == FireMode::Semi && fired_semi {
                break;
            }
            if def.class == Class::Launcher || (def.projectile_speed > 0.0 && def.class != Class::Shotgun && def.effect != Effect::None) {
                for pellet in 0..def.pellets.max(1) {
                    out.push(WeaponAction::Launch { def, pellet });
                }
            } else {
                let spread = w.spread_deg(stance, ads) + if ads < 0.5 { def.spread_move_add * (speed / 30.0).min(1.0) } else { 0.0 };
                // The grip: aimed kick shrinks with each shot of a string, down to its limit.
                let (rate, limit) = def.recoil_reduction;
                let reduce = if ads > 0.5 { 1.0 - (rate * w.string as f32).min(limit) } else { 1.0 };
                for pellet in 0..def.pellets.max(1) {
                    let jitter = (rng.signed(), rng.signed());
                    let recoil = if pellet == 0 {
                        (rng.range(kick.pitch) * KICK_TO_DEGREES * reduce, rng.range(kick.yaw) * KICK_TO_DEGREES * reduce)
                    } else {
                        (0.0, 0.0)
                    };
                    out.push(WeaponAction::Fire { spread_deg: spread, jitter, recoil, pellet });
                }
            }
            w.ammo -= 1;
            w.string = w.string.saturating_add(1);
            w.bloom += def.spread_fire_add;
            w.cooldown += def.fire_time;
            fired_semi = true;
            if let FireMode::Burst(_) = def.fire {
                w.burst_left = w.burst_left.saturating_sub(1);
                if w.burst_left == 0 {
                    w.cooldown += def.burst_delay;
                    break;
                }
            }
        }
        if w.ammo == 0 && w.reserve > 0 {
            // BO2 reloads by itself when the magazine runs dry.
            w.start_reload(reload_speed);
            out.push(WeaponAction::ReloadStarted);
        }
    }

    /// Lethal (G) and tactical (Q) use. Returns whether it owns this frame.
    fn step_offhand(&mut self, held: u32, pressed: u32, released: u32, out: &mut Vec<WeaponAction>) -> bool {
        let dt = LOGIC_DT;
        match self.offhand {
            None => {
                let lethal = pressed & buttons::LETHAL != 0;
                let tactical = pressed & buttons::TACTICAL != 0;
                // BO2 lets an offhand interrupt a raise, not a swap's put-away or a melee.
                if !(lethal || tactical) || self.drop_left > 0.0 || self.melee_left > 0.0 {
                    return false;
                }
                let slot = if lethal { &self.lethal } else { &self.tactical };
                if slot.count == 0 {
                    // Out of C4 in hand: the button is the detonator.
                    if slot.def.sticky && slot.def.place == Place::Throw && slot.def.class == Class::Equipment {
                        out.push(WeaponAction::Detonate);
                    }
                    return false;
                }
                let w = self.current_mut();
                w.reload = None;
                w.burst_left = 0;
                self.ads = 0.0;
                self.offhand = Some(OffhandUse::Holding { lethal, held_for: 0.0 });
                out.push(WeaponAction::OffhandReady { lethal });
                true
            }
            Some(OffhandUse::Holding { lethal, held_for }) => {
                let held_for = held_for + dt;
                let button = if lethal { buttons::LETHAL } else { buttons::TACTICAL };
                let def = if lethal { self.lethal.def } else { self.tactical.def };
                let fuse_left = if def.cook { def.fuse_time - held_for } else { def.fuse_time };
                if def.cook && fuse_left <= 0.0 {
                    // Cooked too long.
                    self.use_offhand(lethal);
                    self.offhand = Some(OffhandUse::Throwing { lethal, t: 0.0 });
                    out.push(WeaponAction::CookedOff { def });
                    return true;
                }
                let let_go = released & button != 0 || held & button == 0;
                if def.place == Place::Hack {
                    // The Black Hat stays up while held; the logic works out what it is aimed at.
                    if let_go {
                        self.offhand = Some(OffhandUse::Throwing { lethal, t: 0.0 });
                    } else {
                        self.offhand = Some(OffhandUse::Holding { lethal, held_for });
                    }
                } else if let_go && held_for >= OFFHAND_PULLBACK {
                    self.use_offhand(lethal);
                    self.offhand = Some(OffhandUse::Throwing { lethal, t: 0.0 });
                    out.push(match def.place {
                        Place::Plant => WeaponAction::Plant { def },
                        _ => WeaponAction::Throw { def, fuse_left },
                    });
                } else {
                    self.offhand = Some(OffhandUse::Holding { lethal, held_for });
                }
                true
            }
            Some(OffhandUse::Throwing { lethal, t }) => {
                let t = t + dt;
                if t >= OFFHAND_THROW {
                    self.offhand = None;
                    let d = self.current().def;
                    (self.raise_left, self.raise_kind) =
                        if self.has(Perk::FAST_HANDS) { (d.quick_raise_time, RaiseKind::Quick) } else { (d.raise_time, RaiseKind::Normal) };
                    self.raise_total = self.raise_left;
                } else {
                    self.offhand = Some(OffhandUse::Throwing { lethal, t });
                }
                true
            }
        }
    }

    /// The Black Hat is up and aimed (seconds held), if it is.
    pub fn hacking(&self) -> Option<f32> {
        match self.offhand {
            Some(OffhandUse::Holding { lethal, held_for }) => {
                let def = if lethal { self.lethal.def } else { self.tactical.def };
                (def.place == Place::Hack).then_some(held_for)
            }
            _ => None,
        }
    }

    /// The Black Hat finished a hack: it uses one charge and goes down.
    pub fn hack_done(&mut self) {
        if let Some(OffhandUse::Holding { lethal, .. }) = self.offhand {
            self.use_offhand(lethal);
            self.offhand = Some(OffhandUse::Throwing { lethal, t: 0.0 });
        }
    }

    pub fn use_offhand(&mut self, lethal: bool) {
        let slot = if lethal { &mut self.lethal } else { &mut self.tactical };
        slot.count = slot.count.saturating_sub(1);
    }
}

impl Default for Arsenal {
    fn default() -> Self {
        Self::new()
    }
}