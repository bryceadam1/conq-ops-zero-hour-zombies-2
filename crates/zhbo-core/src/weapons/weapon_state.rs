/*
** Conquer Ops - Zero Hour gameplay core: one weapon's own state: ammo, the fire cycle's timers,
** the reload in progress, the dual-wield hands.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::catalog::catalog;
use super::def::WeaponDef;
use super::zoom;

/// A reload in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reload {
    /// Magazine swap: seconds in, total length, when the rounds go in, whether they have, from
    /// empty, and whether it is Fast Mag's quick reload (the taped second magazine).
    Magazine { t: f32, length: f32, add_at: f32, added: bool, empty: bool, quick: bool },
    /// Tube shotgun: start, then one cycle per batch of shells, then end.
    Shells { phase: ShellPhase, t: f32, stop: bool },
}

crate::statehash::state_hash!(enum Reload { Magazine { t, length, add_at, added, empty, quick }, Shells { phase, t, stop } });

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellPhase {
    Start,
    Insert,
    End,
}

crate::statehash::state_hash!(enum ShellPhase { Start, Insert, End });

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponState {
    /// What is up now: the weapon, or its alt mode.
    pub def: &'static WeaponDef,
    /// The weapon itself (with its attachments), whichever mode is up.
    pub main: &'static WeaponDef,
    /// The weapon's alt mode (`main.alt`, resolved once), if it has one.
    pub alt_mode: Option<&'static WeaponDef>,
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
    /// Shots in the current string of fire (the reduced-kick window counts it).
    pub string: u16,
    /// The grip's recoil-reduction accumulator, and seconds since the last shot (G5).
    pub recoil_acc: f32,
    pub since_shot: f32,
    /// A bolt or pump action owes a rechamber (BO2's per-weapon flag, set by the shot in
    /// `FUN_008dc260`, cleared when the rechamber ends `FUN_008db020` or a reload starts
    /// `FUN_008e1510`).
    pub rechamber_pending: bool,
    /// Shots since the last rechamber, for `shots_before_rechamber` (`ps+0x4f0`).
    pub rechamber_shots: u16,
    /// Seconds into the rechamber in progress (BO2's weapon state 7/8), or `None`.
    pub rechamber: Option<f32>,
    /// The magazine ran dry: the reload starts once the last shot's fire state ends.
    pub reload_owed: bool,
    /// The variable zoom's level (BO2's inventory slot `+0x10`), the level it changed from and
    /// the seconds since (`ps+0x5d0`/`ps+0x5d4`; `weapons::zoom`).
    pub zoom: zoom::Level,
    /// BO2's per-weapon quick-reload flag (inventory slot `+0x17`): set when a dual-mag weapon is
    /// given (`FUN_008ddce0`), flipped by each of its reloads' ammo transfer (`FUN_008dbc00`), so
    /// Fast Mag alternates the quick swap to the taped magazine with a full reload.
    pub quick_reload: bool,
    /// A dual-wield weapon's left gun (BO2 runs `PM_Weapon` once per hand, `FUN_008c8420` with
    /// `ps+0x71` the hand): its own magazine and timers. The spare rounds are shared — both guns
    /// name the same `szAmmoName` — so `reserve` serves both. `None` for a single gun.
    pub left: Option<Hand>,
    /// A weapon picked up off the ground wears the options it was dropped with (its camo and
    /// reticle, [`super::WeaponOptions`]); `None`: a class weapon, which wears the class's.
    pub options: Option<super::WeaponOptions>,
}

crate::statehash::state_hash!(struct WeaponState {
    def, main, alt_mode, ammo, reserve, cooldown, reload, burst_left, bloom, spin, raised_once, alt_ammo, alt_reserve,
    string, recoil_acc, since_shot, rechamber_pending, rechamber_shots, rechamber, reload_owed, zoom, quick_reload, left,
    options,
});

/// One gun's own magazine and timers: what BO2 keeps per hand (`ammoCount[hand]`, the left hand's
/// weapon state `ps+0x1d4` and times `ps+0x54`/`+0x58`). The right hand's live in [`WeaponState`]
/// itself; [`WeaponState::swap_hand`] trades them so one set of rules runs either hand.
#[derive(Clone, Debug, PartialEq)]
pub struct Hand {
    pub ammo: u16,
    pub cooldown: f32,
    pub reload: Option<Reload>,
    pub burst_left: u8,
    pub string: u16,
    pub recoil_acc: f32,
    pub since_shot: f32,
    /// The hand's own fire-cycle states (BO2's per-hand weapon state: the rechamber states 7/8
    /// and the dry magazine's owed reload), as [`WeaponState`]'s fields of the same names.
    pub rechamber_pending: bool,
    pub rechamber_shots: u16,
    pub rechamber: Option<f32>,
    pub reload_owed: bool,
}

crate::statehash::state_hash!(struct Hand {
    ammo, cooldown, reload, burst_left, string, recoil_acc, since_shot, rechamber_pending, rechamber_shots, rechamber,
    reload_owed,
});

impl Hand {
    /// A full magazine of `def`'s.
    pub(super) fn full(def: &WeaponDef) -> Self {
        Hand {
            ammo: def.clip,
            cooldown: 0.0,
            reload: None,
            burst_left: 0,
            string: 0,
            recoil_acc: 0.0,
            since_shot: 10.0,
            rechamber_pending: false,
            rechamber_shots: 0,
            rechamber: None,
            reload_owed: false,
        }
    }

    pub fn is_reloading(&self) -> bool {
        self.reload.is_some()
    }
}

impl WeaponState {
    pub fn new(def: &'static WeaponDef) -> Self {
        // A gun has its magazine (the Assault Shield's one round is its deploy); melee and the
        // offhand classes have no rounds (the knife's table entry carries none, so its default
        // clip would otherwise read as 30 rounds to the RTS ammo report and the unarmed no-fire
        // veto).
        let gun = def.class.is_slot_weapon();
        Self {
            def,
            main: def,
            alt_mode: catalog().get(&def.alt),
            ammo: if gun { def.clip } else { 0 },
            reserve: if gun { def.start_reserve } else { 0 },
            cooldown: 0.0,
            reload: None,
            burst_left: 0,
            bloom: 0.0,
            spin: 0.0,
            raised_once: false,
            alt_ammo: 0,
            alt_reserve: 0,
            string: 0,
            recoil_acc: 0.0,
            since_shot: 10.0,
            rechamber_pending: false,
            rechamber_shots: 0,
            rechamber: None,
            reload_owed: false,
            zoom: zoom::Level { since: zoom::LEVEL_BLEND_TIME, ..Default::default() },
            quick_reload: def.traits.dual_mag,
            // The left gun's magazine is the dual-wield weapon's clip size: BO2's left-hand def
            // (`szDualWieldWeaponName`) carries the same `iClipSize` and reload times as its pair
            // for every dual-wield pistol, and is not a weapon the table converts.
            left: (gun && def.is_dual_wield()).then(|| Hand::full(def)),
            options: None,
        }
        .with_alt_ammo()
    }

    /// Trade the right hand's magazine and timers with the left gun's (no-op for a single gun), so
    /// the reload and trigger rules run the left hand on its own state. Call it again to trade back.
    pub fn swap_hand(&mut self) {
        let Some(h) = self.left.as_mut() else { return };
        std::mem::swap(&mut self.ammo, &mut h.ammo);
        std::mem::swap(&mut self.cooldown, &mut h.cooldown);
        std::mem::swap(&mut self.reload, &mut h.reload);
        std::mem::swap(&mut self.burst_left, &mut h.burst_left);
        std::mem::swap(&mut self.string, &mut h.string);
        std::mem::swap(&mut self.recoil_acc, &mut h.recoil_acc);
        std::mem::swap(&mut self.since_shot, &mut h.since_shot);
        std::mem::swap(&mut self.rechamber_pending, &mut h.rechamber_pending);
        std::mem::swap(&mut self.rechamber_shots, &mut h.rechamber_shots);
        std::mem::swap(&mut self.rechamber, &mut h.rechamber);
        std::mem::swap(&mut self.reload_owed, &mut h.reload_owed);
    }

    /// Stop both hands' reloads and bursts (a swap, melee or offhand interrupts them).
    pub(super) fn interrupt(&mut self) {
        self.reload = None;
        self.burst_left = 0;
        if let Some(h) = self.left.as_mut() {
            h.reload = None;
            h.burst_left = 0;
        }
    }

    /// The left gun's rounds in its magazine, for a dual-wield weapon.
    pub fn left_ammo(&self) -> Option<u16> {
        self.left.as_ref().map(|h| h.ammo)
    }

    /// Load the alt mode's own magazine (an under-barrel launcher's grenades).
    pub(super) fn with_alt_ammo(mut self) -> Self {
        if let Some(alt) = self.alt_mode.filter(|a| a.alt_own_ammo) {
            self.alt_ammo = alt.clip;
            self.alt_reserve = alt.start_reserve;
        }
        self
    }

    pub fn is_reloading(&self) -> bool {
        self.reload.is_some()
    }

    /// The alt mode is up (BO2's held weapon is the `altmode` inventory entry).
    pub fn alt_up(&self) -> bool {
        self.def.id != self.main.id
    }

    /// One side of the alt pair keeps a magazine of its own (an under-barrel launcher): the alt
    /// toggle trades it with the other's ([`super::Arsenal`]'s `finish_alt`); every other alt shares
    /// the weapon's.
    pub fn alt_owns_ammo(&self) -> bool {
        self.main.alt_own_ammo || self.alt_mode.is_some_and(|a| a.alt_own_ammo)
    }

    /// The weapon's own magazine and spare rounds (its main mode's: BO2's inventory entry for the
    /// weapon, apart from its `altmode` entry), whichever mode is up — Scavenger's refill and the
    /// overhead General's gun use these. With an own-ammo alt up they sit in `alt_ammo` and
    /// `alt_reserve`.
    pub fn main_rounds_mut(&mut self) -> (&mut u16, &mut u16) {
        if self.alt_up() && self.alt_owns_ammo() {
            (&mut self.alt_ammo, &mut self.alt_reserve)
        } else {
            (&mut self.ammo, &mut self.reserve)
        }
    }

    /// [`Self::main_rounds_mut`]'s magazine and spare rounds, read.
    pub fn main_rounds(&self) -> (u16, u16) {
        if self.alt_up() && self.alt_owns_ammo() {
            (self.alt_ammo, self.alt_reserve)
        } else {
            (self.ammo, self.reserve)
        }
    }

    /// The Variable Zoom's melee press at full ADS (`FUN_008de340` → `FUN_008da3c0`, weapon-based
    /// levels): step to the next of [`zoom::LEVELS`] and start the change's blend.
    pub fn step_zoom_level(&mut self) {
        self.zoom = zoom::Level { level: (self.zoom.level + 1) % zoom::LEVELS, from: self.zoom.level, since: 0.0 };
    }

    /// Seconds left on a magazine reload (0 when none; shell reloads report their current phase).
    pub fn reload_left(&self) -> f32 {
        match self.reload {
            Some(Reload::Magazine { t, length, .. }) => (length - t).max(0.0),
            Some(Reload::Shells { .. }) => self.def.reload_time,
            None => 0.0,
        }
    }

    /// BO2's empty-clip test for the empty raise and drop (`FUN_008e6770`): clip 0 on a weapon
    /// without `unlimitedAmmo`.
    pub fn clip_empty(&self) -> bool {
        self.ammo == 0 && !self.def.traits.unlimited_ammo
    }

    /// Weapon time left in the reload's current state: a magazine reload is one state for its
    /// whole length (the ammo add is its `weaponDelay`); a shell reload's start, each insert and
    /// its end are states of their own.
    pub fn reload_part_left(&self) -> f32 {
        let d = self.def;
        match self.reload {
            Some(Reload::Magazine { t, length, .. }) => (length - t).max(0.0),
            Some(Reload::Shells { phase, t, .. }) => {
                let part = match phase {
                    ShellPhase::Start => d.reload_start_time,
                    ShellPhase::Insert => d.reload_time,
                    ShellPhase::End => d.reload_end_time,
                };
                (part - t).max(0.0)
            }
            None => 0.0,
        }
    }

    pub fn can_reload(&self) -> bool {
        !self.is_reloading() && self.ammo < self.def.clip && self.reserve > 0
    }

    pub(super) fn start_reload(&mut self) {
        if !self.can_reload() {
            return;
        }
        let d = self.def;
        self.burst_left = 0;
        // Starting a reload drops an owed rechamber (`FUN_008e1510`).
        self.rechamber_pending = false;
        self.rechamber = None;
        self.reload = Some(if d.shell_by_shell() {
            Reload::Shells { phase: ShellPhase::Start, t: 0.0, stop: false }
        } else {
            let empty = d.empty_reload(self.ammo);
            let quick = self.quick_reload && d.traits.dual_mag;
            let (length, add_at) = magazine_times(d, empty, quick);
            Reload::Magazine { t: 0.0, length, add_at, added: false, empty, quick }
        });
    }

    /// The rechamber (`FUN_008db0d0`): once the shot's fire time has run out, an owed rechamber
    /// runs for `iRechamberTime` and the weapon cannot fire until it ends (`FUN_008db020`). The
    /// `rof` tick scales it like the fire state (G3: states 6, 7, 8).
    pub(super) fn step_rechamber(&mut self, tick: f32) {
        let t = &self.def.timing;
        if let Some(e) = self.rechamber {
            let e = e + tick;
            if e >= t.rechamber_time {
                self.rechamber = None;
                self.rechamber_pending = false;
                // The time past the rechamber's end carries into the next shot's wait (the
                // cooldown's own count-down during the rechamber is discarded).
                self.cooldown = -(e - t.rechamber_time);
            } else {
                self.rechamber = Some(e);
            }
            return;
        }
        if self.rechamber_pending && t.bolt_action && self.cooldown <= 0.0 && !self.is_reloading() {
            // The rechamber starts where the fire state ended, mid-frame.
            self.rechamber_shots = 0;
            let carried = -self.cooldown;
            self.cooldown = 0.0;
            self.rechamber = Some(0.0);
            self.step_rechamber(carried);
        }
    }

    pub(super) fn take(&mut self, n: u16) {
        let take = n.min(self.def.clip - self.ammo).min(self.reserve);
        self.ammo += take;
        self.reserve -= take;
    }

    /// Advance a reload by `dt` of weapon time; returns whether it finished this frame.
    pub(super) fn step_reload(&mut self, dt: f32) -> bool {
        let d = self.def;
        let Some(r) = self.reload else { return false };
        match r {
            Reload::Magazine { t, length, add_at, added, empty, quick } => {
                let t = t + dt;
                let mut added = added;
                if !added && t >= add_at {
                    self.take(d.clip);
                    added = true;
                    // The ammo transfer flips a dual-mag weapon's quick flag (`FUN_008dbc00`).
                    if d.traits.dual_mag {
                        self.quick_reload = !quick;
                    }
                }
                if t >= length {
                    self.reload = None;
                    return true;
                }
                self.reload = Some(Reload::Magazine { t, length, add_at, added, empty, quick });
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

/// A magazine reload's length and the moment its rounds go in (seconds of weapon time). The
/// length (`FUN_008dee50`): the quick reload's `iReloadQuick{,Empty}Time` when `quick`, else the
/// normal or empty reload. The add (`FUN_008dec90`): `iReloadQuickAddTime` or `iReloadAddTime`; an
/// empty reload takes `iReloadEmptyAddTime` when it has one, else that same add time; capped at
/// the *normal* (or normal empty) reload time, and never past the end.
pub(super) fn magazine_times(d: &WeaponDef, empty: bool, quick: bool) -> (f32, f32) {
    let t = &d.timing;
    // A def without an empty reload time (0: the converted offhands and the shield) reloads over
    // the normal one.
    let normal = if empty && d.reload_empty_time > 0.0 { d.reload_empty_time } else { d.reload_time };
    let quick_length = if empty { t.reload_quick_empty_time } else { t.reload_quick_time };
    // A table without the quick times (0) keeps the normal reload.
    let length = if quick && quick_length > 0.0 { quick_length } else { normal };
    let add = if quick { t.reload_quick_add_time } else { d.reload_add_time };
    let add = if empty && d.reload_empty_add_time > 0.0 { d.reload_empty_add_time } else { add };
    let add_at = if add > 0.0 && add < normal { add } else { normal };
    (length, add_at.min(length))
}
