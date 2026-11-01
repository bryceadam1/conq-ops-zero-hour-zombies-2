/*
** Conquer Ops - Zero Hour gameplay core: the first-person weapon state machine: raise, fire, reload, offhands, melee.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::catalog::{catalog, weapon, weapon_or_none};
use super::def::{Class, Effect, FireMode, Place, WeaponDef};
use super::loadout::{class_melee, melee_weapon, Loadout};
use super::perks::Perk;
use super::rng::Rng;
use super::specialty::{Specialties, Specialty};
use super::zoom;
use crate::input::buttons;
use crate::tuning::Tuning;
use crate::units::LOGIC_DT;


/// A reload in progress.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reload {
    /// Magazine swap: seconds in, total length, when the rounds go in, whether they have, from
    /// empty, and whether it is Fast Mag's quick reload (the taped second magazine).
    Magazine { t: f32, length: f32, add_at: f32, added: bool, empty: bool, quick: bool },
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
}

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

impl Hand {
    /// A full magazine of `def`'s.
    fn full(def: &WeaponDef) -> Self {
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
        // A gun has its magazine; melee and the offhand classes have no rounds (the knife's table
        // entry carries none, so its default clip would otherwise read as 30 rounds to the RTS
        // ammo report and the unarmed no-fire veto).
        let gun = def.class.is_gun();
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
    fn interrupt(&mut self) {
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
    fn with_alt_ammo(mut self) -> Self {
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

    pub fn can_reload(&self) -> bool {
        !self.is_reloading() && self.ammo < self.def.clip && self.reserve > 0
    }

    fn start_reload(&mut self) {
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
    fn step_rechamber(&mut self, tick: f32) {
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

    fn take(&mut self, n: u16) {
        let take = n.min(self.def.clip - self.ammo).min(self.reserve);
        self.ammo += take;
        self.reserve -= take;
    }

    /// Advance a reload by `dt` of weapon time; returns whether it finished this frame.
    fn step_reload(&mut self, dt: f32) -> bool {
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
    /// [-1, 1), view kick (degrees pitch up, yaw, roll) and the gun kick spring's impulse.
    /// `last`: this round empties the magazine (BO2's last-shot shell eject). `left`: the
    /// dual-wield left gun fired it (BO2's `EV_FIRE_WEAPON_LEFT`, `FUN_008e1b80`).
    Fire { spread_deg: f32, jitter: (f32, f32), view_kick: (f32, f32, f32), gun_kick: (f32, f32), pellet: u16, last: bool, left: bool },
    /// Launch a projectile (launchers, crossbow, ballistic knife); `pellet` numbers the bolts of
    /// a stacked pull by the rounds loaded before each, minus one (the tri-bolt's 2, 1, 0).
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
    /// A swap to a weapon with an empty clip (BO2's empty raise).
    Empty,
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
    /// The held knife: the one weapon of a kit with no gun.
    pub melee: &'static WeaponDef,
    /// The class melee weapon ([`super::loadout::CLASS_MELEE`]): what the melee button strikes with while a gun is
    /// up ([`Arsenal::melee_weapon`]).
    pub class_melee: &'static WeaponDef,
    /// Raising the active weapon (first raise, swap, after an offhand), out of `raise_total`.
    pub raise_left: f32,
    pub raise_total: f32,
    pub raise_kind: RaiseKind,
    /// Putting the previous weapon away before `raise_left` starts, out of `drop_total`.
    pub drop_left: f32,
    pub drop_total: f32,
    /// The weapon being put away.
    pub previous: usize,
    /// The drop under way is an alt toggle's (BO2's state 5): the mode changes when it ends.
    pub alt_pending: bool,
    pub ads: f32,
    /// Seconds since sprinting stopped (weapon comes back up over `sprint_out_time`).
    pub since_sprint: f32,
    pub melee_left: f32,
    pub offhand: Option<OffhandUse>,
    held_last: u32,
    /// Seconds since reload was last pressed (a second press soon after detonates C4).
    since_reload_press: f32,
    /// Real seconds the held grenade has cooked (the fuse burns in real time; the offhand
    /// states' own clock runs at the perk's rate).
    cooked: f32,
    /// The tuning pack's perk multipliers, taken when the kit was made (the pack is loaded once at
    /// init, identically on every client).
    pub tuning: Tuning,
}

/// One of BO2's weapon-state families, each of which a specialty speeds up (Ghidra G3,
/// `FUN_008e0780`: while a state runs, `weaponTime -= msec / mult`, so it lasts `duration x mult`;
/// the client plays the viewmodel animation at `rate / mult` to match, `FUN_0052b1d0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    /// Raise and drop (states 1-5): `fastweaponswitch`.
    Switch,
    /// The reload family (0x0b-0x12): `fastreload`.
    Reload,
    /// Firing and rechambering (6, 7, 8, 0x28): `rof`.
    Fire,
    /// Melee (0x15): `fastmeleerecovery`.
    Melee,
    /// A grenade's or other thrown offhand's states (0x16-0x1d): `fasttoss`.
    Toss,
    /// Equipment's offhand states: `fastequipmentuse`.
    Equipment,
    /// Sprint out (0x26): `sprintrecovery`.
    SprintOut,
}

impl Arsenal {
    pub fn new() -> Self {
        Self::with_loadout(Loadout::default())
    }

    /// The General before a class is picked: knife only, no guns, no grenades. BO2 always has
    /// melee, so the knife is the one weapon; an empty slot in `weapons` is never required because
    /// the knife is there (every consumer can use `current()`), and the offhand counts are zero.
    pub fn unarmed() -> Self {
        let loadout = Loadout::unarmed();
        let melee = weapon(&loadout.melee);
        let mut knife = WeaponState::new(melee);
        knife.raised_once = true;
        Self {
            lethal: Offhand { def: melee, count: 0 },
            tactical: Offhand { def: melee, count: 0 },
            melee,
            class_melee: class_melee(melee),
            loadout,
            weapons: vec![knife],
            active: 0,
            raise_left: 0.0,
            raise_total: 0.0,
            raise_kind: RaiseKind::First,
            drop_left: 0.0,
            drop_total: 0.0,
            previous: 0,
            alt_pending: false,
            ads: 0.0,
            since_sprint: 10.0,
            melee_left: 0.0,
            offhand: None,
            held_last: 0,
            since_reload_press: 10.0,
            cooked: 0.0,
            tuning: crate::tuning::get(),
        }
    }

    /// Whether the General has no gun (the unarmed kit, or a knife-only kit): the HUD hides the
    /// ammo panel, the crosshair and the offhand counters, and firing has nothing to fire.
    pub fn is_unarmed(&self) -> bool {
        !self.weapons.iter().any(|w| w.def.class.is_gun())
    }

    pub fn with_loadout(loadout: Loadout) -> Self {
        // A gun whose id is not in the weapon table is *left out*, loudly (catalog::weapon_or_none
        // records it), rather than silently becoming the catalog's first entry — the AN-94 — which
        // made a failed class application look like "the class picked, but the stock kit stayed".
        let mut weapons: Vec<WeaponState> = Vec::new();
        for (id, reserve) in [(&loadout.primary, loadout.reserve[0]), (&loadout.secondary, loadout.reserve[1])] {
            if id.is_empty() {
                continue;
            }
            let Some(def) = weapon_or_none(id) else { continue };
            let mut w = WeaponState::new(def);
            if let Some(r) = reserve {
                w.reserve = r;
            }
            weapons.push(w);
        }
        if loadout.perks.contains(&Perk::SCAVENGER) {
            for w in &mut weapons {
                w.reserve = (w.reserve + w.def.clip).min(w.def.max_reserve.max(w.reserve));
            }
        }
        // Offhands start with BO2's start count (`start_reserve` for them); an empty or unknown
        // slot has none.
        let knife = || weapon("knife_held_mp");
        let offhand = |id: &str| {
            if id.is_empty() {
                return Offhand { def: knife(), count: 0 };
            }
            let Some(def) = weapon_or_none(id) else { return Offhand { def: knife(), count: 0 } };
            Offhand { def, count: def.start_reserve.max(1) }
        };
        let melee = if loadout.melee.is_empty() { knife() } else { weapon_or_none(&loadout.melee).unwrap_or_else(knife) };
        // A kit with no resolvable gun at all is unarmed (the knife is still there): never leave
        // `weapons` empty, which every consumer reads through `current()`.
        if weapons.is_empty() {
            let mut knife = WeaponState::new(melee);
            knife.raised_once = true;
            weapons.push(knife);
        }
        let raise = weapons[0].def.first_raise_time;
        weapons[0].raised_once = true;
        Self {
            lethal: offhand(&loadout.lethal),
            tactical: offhand(&loadout.tactical),
            melee,
            class_melee: class_melee(melee),
            loadout,
            weapons,
            active: 0,
            raise_left: raise,
            raise_total: raise,
            raise_kind: RaiseKind::First,
            drop_left: 0.0,
            drop_total: 0.0,
            previous: 0,
            alt_pending: false,
            ads: 0.0,
            since_sprint: 10.0,
            melee_left: 0.0,
            offhand: None,
            held_last: 0,
            since_reload_press: 10.0,
            cooked: 0.0,
            tuning: crate::tuning::get(),
        }
    }

    /// The weapon a melee uses now: the held weapon when it is a knife, else the class melee
    /// weapon ([`melee_weapon`]). The held weapon cannot change mid-melee (a swap waits for it).
    pub fn melee_weapon(&self) -> &'static WeaponDef {
        melee_weapon(self.current().def, self.class_melee)
    }

    pub fn has(&self, perk: Perk) -> bool {
        self.loadout.perks.contains(&perk)
    }

    /// The specialties the class's perks grant.
    pub fn class_specialties(&self) -> Specialties {
        self.loadout.perks.iter().fold(Specialties::NONE, |s, p| s.union(p.specialties()))
    }

    /// BO2's current weapon for the specialty test (`FUN_008ddfe0`): the offhand while it is in
    /// use, the melee weapon while meleeing, else the weapon up.
    pub fn current_weapon(&self) -> &'static WeaponDef {
        match self.offhand {
            Some(OffhandUse::Holding { lethal, .. }) | Some(OffhandUse::Throwing { lethal, .. }) => {
                if lethal {
                    self.lethal.def
                } else {
                    self.tactical.def
                }
            }
            None if self.melee_left > 0.0 => self.melee_weapon(),
            None => self.current().def,
        }
    }

    /// BO2's HasSpecialty (Ghidra G1, `FUN_008bf6c0`): the class grants it, or the current weapon
    /// does (its variant's bits with its attachments').
    pub fn has_specialty(&self, s: Specialty) -> bool {
        self.class_specialties().has(s) || self.current_weapon().specialties.has(s)
    }

    /// How long a state of `family` lasts, as a share of its weapon-data duration (1 without the
    /// specialty). Weapon timers advance by `dt / multiplier`; the viewmodel plays at
    /// `1 / multiplier` (see [`Arsenal::anim_rate`]).
    pub fn multiplier(&self, family: Timing) -> f32 {
        let t = &self.tuning;
        let (specialty, m) = match family {
            Timing::Switch => (Specialty::FastWeaponSwitch, t.perk_weap_switch_multiplier),
            Timing::Reload => (Specialty::FastReload, t.perk_weap_reload_multiplier),
            Timing::Fire => (Specialty::Rof, t.perk_weap_rate_multiplier),
            Timing::Melee => (Specialty::FastMeleeRecovery, t.perk_weap_melee_multiplier),
            Timing::Toss => (Specialty::FastToss, t.perk_weap_toss_multiplier),
            Timing::Equipment => (Specialty::FastEquipmentUse, t.perk_weap_equipment_use_multiplier),
            Timing::SprintOut => (Specialty::SprintRecovery, t.perk_sprint_recovery_multiplier),
        };
        if self.has_specialty(specialty) && m > 0.0 {
            m
        } else {
            1.0
        }
    }

    /// Weapon time passed in one logic frame for `family`.
    fn tick(&self, family: Timing) -> f32 {
        LOGIC_DT / self.multiplier(family)
    }

    /// The viewmodel animation's playback rate for what the weapon is doing now (`FUN_0052b1d0`:
    /// `rate / mult`); 1 at rest. The clip times the viewmodel reads are already in weapon time,
    /// so this is for anything timed against the wall clock.
    pub fn anim_rate(&self) -> f32 {
        let family = if let Some(OffhandUse::Holding { lethal, .. } | OffhandUse::Throwing { lethal, .. }) = self.offhand {
            Some(self.offhand_timing(lethal))
        } else if self.melee_left > 0.0 {
            Some(Timing::Melee)
        } else if self.drop_left > 0.0 || self.raise_left > 0.0 {
            Some(Timing::Switch)
        } else if self.current().is_reloading() {
            Some(Timing::Reload)
        } else if self.since_sprint < self.current().def.sprint_out_time {
            Some(Timing::SprintOut)
        } else {
            None
        };
        family.map_or(1.0, |f| 1.0 / self.multiplier(f))
    }

    fn offhand_timing(&self, lethal: bool) -> Timing {
        let def = if lethal { self.lethal.def } else { self.tactical.def };
        if def.class == Class::Equipment {
            Timing::Equipment
        } else {
            Timing::Toss
        }
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

    /// Movement speed multiplier from the held weapon: its `adsMoveSpeedScale` while aiming, else
    /// its `moveSpeedScale` (`FUN_008ed080`; G2). Lightweight is the movement's own
    /// ([`Arsenal::move_faster`]).
    pub fn move_scale(&self) -> f32 {
        let d = self.current().def;
        if self.ads > 0.0 {
            d.ads_move_scale
        } else {
            d.move_scale
        }
    }

    /// What the player's own blasts and stuns do to them: `flakjacket` takes `perk_flakJacket`
    /// percent of explosive damage (`_class.gsc:1023-1032`), `stunprotection` cuts a stun to
    /// x0.1 (`_weapons.gsc:1375-1376`). Checked against the class: by the time the grenade goes
    /// off the hand holds the gun again.
    pub fn self_harm_scales(&self) -> (f32, f32) {
        let class = self.class_specialties();
        let blast = if class.has(Specialty::FlakJacket) { self.tuning.perk_flak_jacket * 0.01 } else { 1.0 };
        let stun = if class.has(Specialty::StunProtection) { self.tuning.stun_protection_scale } else { 1.0 };
        (blast, stun)
    }

    /// The hip spread cone's min and max at an eye height (inches; Ghidra G6): the weapon's
    /// stance values blended by eye height, x0.65 with `bulletaccuracy`.
    pub fn spread_cone(&self, view_height: f32) -> (f32, f32) {
        super::recoil::spread_range(self.current().def, view_height, self.has_specialty(Specialty::BulletAccuracy), &self.tuning)
    }

    /// The spread now (degrees) at a stance (0 stand, 1 crouch, 2 prone) and aim: the cone's
    /// minimum plus the bloom, at most its maximum, blending to the ADS spread as the gun aims.
    pub fn spread_now(&self, stance: usize, ads: f32) -> f32 {
        let t = &self.tuning;
        let height = [t.player_standing_view_height, t.view_height_crouch, t.view_height_prone][stance.min(2)];
        let (min, max) = self.spread_cone(height);
        let w = self.current();
        let hip = (min + w.bloom).min(max);
        hip + (w.def.ads_spread - hip) * ads
    }

    /// `movefaster` (Lightweight): the run speed is `g_speed x perk_speedMultiplier` (G2).
    pub fn move_faster(&self) -> bool {
        self.has_specialty(Specialty::MoveFaster)
    }

    /// Sprint length multiplier: the weapon's `sprintDurationScale`, x `perk_sprintMultiplier`
    /// with `longersprint` (`FUN_008bcfe0`; G2).
    pub fn sprint_scale(&self) -> f32 {
        let d = self.current().def.sprint_duration_scale.max(0.1);
        d * if self.has_specialty(Specialty::LongerSprint) { self.tuning.perk_sprint_multiplier } else { 1.0 }
    }

    /// The weapon is back up from a sprint: `sprintOutTime` of weapon time has passed (which
    /// `sprintrecovery` makes 0.3 of the wall time; G2 `FUN_008e4ad0`).
    fn sprint_ready(&self) -> bool {
        self.since_sprint >= self.current().def.sprint_out_time
    }

    /// Raise and drop take the weapon's own times; `fastweaponswitch` runs them at its rate
    /// ([`Timing::Switch`]) rather than switching to the quick animations (G3). A weapon with an
    /// empty clip (`WeaponState::clip_empty`) drops over its `iEmptyDropTime` (`FUN_008db500`) and raises over its
    /// `iEmptyRaiseTime`, which beats the first raise (`FUN_008df320`; the first raise is spent
    /// either way).
    fn switch_to(&mut self, index: usize) {
        let from = self.current().def;
        let from_empty = self.current().clip_empty();
        self.current_mut().interrupt(); // swapping cancels a reload, like BO2
        self.drop_left = if from_empty { from.empty_drop_time } else { from.drop_time };
        self.drop_total = self.drop_left;
        self.previous = self.active;
        self.active = index;
        let to = self.current().def;
        let first = !self.current().raised_once;
        let empty = self.current().clip_empty();
        self.current_mut().raised_once = true;
        (self.raise_left, self.raise_kind) = if empty {
            (to.empty_raise_time, RaiseKind::Empty)
        } else if first {
            (to.first_raise_time, RaiseKind::First)
        } else {
            (to.raise_time, RaiseKind::Normal)
        };
        self.raise_total = self.raise_left;
        self.ads = 0.0;
    }

    /// The mode an alt toggle of the held weapon goes to: its alt from the weapon, the weapon from
    /// its alt; `None` when it has none.
    fn alt_target(&self) -> Option<&'static WeaponDef> {
        let w = self.current();
        let to = if w.def.id == w.main.id { w.alt_mode? } else { w.main };
        (to.id != w.def.id).then_some(to)
    }

    /// The held mode and the one its toggle goes to are both ADS-only (`FUN_008da850`, each side
    /// `FUN_008ecb00`: the Hybrid optic's weapon and its `dualoptic_*` alt).
    fn ads_only_pair(&self) -> bool {
        self.current().def.traits.alt_ads_only && self.alt_target().is_some_and(|to| to.traits.alt_ads_only)
    }

    /// Which toggle input may switch the held weapon's mode (BO2's client side). The alt-mode
    /// action slot (`FUN_00537ff0` -> `FUN_00537ca0(.., 0)`) refuses an ADS-only target; the
    /// sprint/hold-breath key (`+breath_sprint`, `FUN_0053f380` action 9) while the aim button is
    /// held toggles only an ADS-only target (`FUN_00537dd0` -> `FUN_00537ca0(.., 1)`), and only
    /// once the held ADS-only mode is fully aimed (`FUN_005313a0`: `posFrac == 1`).
    fn alt_toggle_requested(&self, held: u32, pressed: u32) -> bool {
        let Some(to) = self.alt_target() else { return false };
        if to.traits.alt_ads_only {
            pressed & buttons::SPRINT != 0 && held & buttons::ADS != 0 && self.current().def.traits.alt_ads_only && self.ads >= 1.0
        } else {
            pressed & buttons::ALT_MODE != 0
        }
    }

    /// Start toggling the held weapon to its alt mode or back (BO2's weapon-change begin,
    /// `FUN_008db500`, for an alt pair `FUN_008db3f0`): the weapon goes down over the **outgoing**
    /// weapon's alt drop time (`FUN_008ed1e0`, already resolved per combination into
    /// `alt_drop_time`), then [`Self::finish_alt`] changes the mode. A zero drop (every launcher
    /// pair) changes it in the same frame, as `FUN_008e4da0` finishes a zero-time change in the
    /// same pmove. Both run in the raise/drop family (states 5 and 2), so Fast Hands speeds them (G3).
    pub fn toggle_alt(&mut self) {
        if self.alt_target().is_none() {
            return;
        }
        let w = self.current_mut();
        w.reload = None;
        w.burst_left = 0;
        w.string = 0;
        let drop = w.def.alt_drop_time;
        // An ADS-only pair stays aimed through the toggle (`FUN_008daa10` lets an ADS-only weapon
        // aim in the alt drop and raise states, 5 and 2).
        if !self.ads_only_pair() {
            self.ads = 0.0;
        }
        self.previous = self.active;
        self.alt_pending = true;
        self.drop_left = drop;
        self.drop_total = drop;
        if drop <= 0.0 {
            self.finish_alt();
        }
    }

    /// The alt toggle's drop is done: swap to the other mode and raise it over the **incoming**
    /// variant's `iAltRaiseTime` (`FUN_008df320`) — except between two ADS-only modes, where
    /// `FUN_008da850` sets no raise at all and the weapon is ready, still aimed. An alt mode with
    /// its own ammo (a launcher) trades magazines with the weapon; the others share it.
    fn finish_alt(&mut self) {
        self.alt_pending = false;
        let ads_only = self.ads_only_pair();
        let Some(to) = self.alt_target() else { return };
        let w = self.current_mut();
        if to.alt_own_ammo || w.def.alt_own_ammo {
            std::mem::swap(&mut w.ammo, &mut w.alt_ammo);
            std::mem::swap(&mut w.reserve, &mut w.alt_reserve);
        }
        w.def = to;
        w.reload = None;
        w.burst_left = 0;
        w.cooldown = w.cooldown.max(0.0);
        w.string = 0;
        let raise = if ads_only { 0.0 } else { to.alt_raise_time };
        if !ads_only {
            self.ads = 0.0;
        }
        self.raise_left = raise;
        self.raise_total = raise;
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
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, held: u32, pressed: u32, sprinting: bool, stance: usize, speed: f32, rng: &mut Rng, out: &mut Vec<WeaponAction>) {
        let dt = LOGIC_DT;
        let pressed = pressed | (held & !self.held_last);
        let released = self.held_last & !held;
        self.held_last = held;

        // Weapon timers run in weapon-data time: each family at its specialty's rate (G3).
        let (sprint_tick, melee_tick, switch_tick) = (self.tick(Timing::SprintOut), self.tick(Timing::Melee), self.tick(Timing::Switch));
        self.since_sprint = if sprinting { 0.0 } else { self.since_sprint + sprint_tick };
        self.melee_left = (self.melee_left - melee_tick).max(0.0);
        if self.drop_left > 0.0 {
            self.drop_left = (self.drop_left - switch_tick).max(0.0);
            if self.drop_left <= 0.0 && self.alt_pending {
                self.finish_alt();
            }
        } else {
            self.raise_left = (self.raise_left - switch_tick).max(0.0);
        }
        for w in &mut self.weapons {
            let decay = w.def.spread_decay.max(0.1);
            w.bloom = (w.bloom - decay * dt).max(0.0);
            w.since_shot += dt;
        }

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
        if self.alt_toggle_requested(held, pressed) && self.melee_left <= 0.0 && self.drop_left <= 0.0 {
            self.toggle_alt();
            out.push(WeaponAction::AltToggled);
        }

        // Swap.
        // BO2 starts no weapon change during an alt drop (`FUN_008db500` returns in states 3-5).
        if pressed & buttons::SWAP_WEAPON != 0 && self.weapons.len() > 1 && self.melee_left <= 0.0 && !self.alt_pending {
            let next = (self.active + 1) % self.weapons.len();
            self.switch_to(next);
            out.push(WeaponAction::Swapped);
        }

        // The zoom level's change blend runs in real time (the client reads `since`).
        self.current_mut().zoom.since += dt;
        // Aimed, the melee button is the Variable Zoom's level toggle at full ADS (`FUN_008de340`),
        // and BO2's melee start (`FUN_008e2070`) is refused while aimed at all with an overlay
        // reticle or distinct zoom levels.
        let def = self.current().def;
        if pressed & buttons::MELEE != 0 && self.ads >= 1.0 && has_attachment_kind(def, zoom::VZOOM_KIND) {
            self.current_mut().step_zoom_level();
        }
        let melee_refused = zoom::melee_blocked(def, self.ads);
        // Melee interrupts a reload and firing, not a raise.
        if pressed & buttons::MELEE != 0 && !melee_refused && self.melee_left <= 0.0 && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            let m = self.melee_weapon();
            self.melee_left = m.melee_time.max(0.3);
            self.current_mut().interrupt();
            self.ads = 0.0;
            out.push(WeaponAction::Melee { damage: m.melee_damage });
            return;
        }

        // ADS blend (`FUN_008e4470`): the fraction moves at 1/adsTransInTime in and
        // 1/adsTransOutTime out, `fastads` dividing the rate by `perk_weapAdsMultiplier`; never
        // while sprinting or until the weapon is back up from a sprint.
        let def = self.current().def;
        let sprint_ready = self.sprint_ready();
        // Only a gun aims; a knife or a held offhand has no sights (and no ADS zoom). A dual-wield
        // weapon has none either: its ADS button is the left gun's trigger.
        // `FUN_008daa10`: an alt toggle's drop and raise (states 5 and 2) let only an ADS-only
        // weapon aim; every other raise or drop refuses it.
        let switching = self.raise_left > 0.0 || self.drop_left > 0.0;
        let alt_switching = self.alt_pending || (self.raise_left > 0.0 && self.raise_kind == RaiseKind::Alt);
        let switch_ok = !switching || (alt_switching && def.traits.alt_ads_only);
        let wants_ads = def.class.is_gun()
            && !def.is_dual_wield()
            && held & buttons::ADS != 0
            && !sprinting
            && sprint_ready
            && switch_ok
            && !self.current().is_reloading();
        let quickdraw = if self.has_specialty(Specialty::FastAds) && self.tuning.perk_weap_ads_multiplier > 0.0 { self.tuning.perk_weap_ads_multiplier } else { 1.0 };
        self.ads = if wants_ads {
            (self.ads + dt / (def.ads_in_time.max(1e-3) * quickdraw)).min(1.0)
        } else {
            (self.ads - dt / (def.ads_out_time.max(1e-3) * quickdraw)).max(0.0)
        };

        // Each hand runs the reload and trigger rules on its own magazine and timers (BO2's
        // `PM_Weapon`, `FUN_008e4da0`, once per hand). The trigger is `FUN_008dbb20`'s button: the
        // attack button for a single gun and a dual-wield pair's right gun, the ADS button for the
        // left gun. The reload button reaches both hands (each checks it, `FUN_008dfd50`).
        let dual = self.current().left.is_some();
        self.step_hand(held, pressed, sprinting, stance, speed, rng, out, false);
        if dual {
            let trigger = |bits: u32| (bits & !buttons::FIRE) | if bits & buttons::ADS != 0 { buttons::FIRE } else { 0 };
            self.current_mut().swap_hand();
            self.step_hand(trigger(held), trigger(pressed), sprinting, stance, speed, rng, out, true);
            self.current_mut().swap_hand();
        }
    }

    /// One hand's reload progress and requests, then its trigger: `held`/`pressed` carry that
    /// hand's trigger as `FIRE`. The left hand runs with its state swapped in
    /// ([`WeaponState::swap_hand`]); its actions say so (`left`).
    #[allow(clippy::too_many_arguments)]
    fn step_hand(&mut self, held: u32, pressed: u32, sprinting: bool, stance: usize, speed: f32, rng: &mut Rng, out: &mut Vec<WeaponAction>, left: bool) {
        let dt = LOGIC_DT;
        // Reload progress / requests.
        let reload_tick = self.tick(Timing::Reload);
        if self.current_mut().step_reload(reload_tick) {
            out.push(WeaponAction::ReloadFinished);
        }
        if pressed & buttons::RELOAD != 0 && self.current().can_reload() && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            self.current_mut().start_reload();
            out.push(WeaponAction::ReloadStarted);
        }

        let def = self.current().def;
        let sprint_ready = self.sprint_ready();
        if def.class == Class::Melee || def.class.is_offhand() {
            return;
        }

        // Firing.
        let ads = self.ads;
        let blocked = sprinting || !sprint_ready || self.raise_left > 0.0 || self.drop_left > 0.0 || self.melee_left > 0.0;
        let fire_tick = self.tick(Timing::Fire);
        let w = self.current_mut();
        w.cooldown -= fire_tick;
        // BO2 reloads by itself when the magazine runs dry, once the last shot's fire state
        // (`iLastFireTime` or the fire time) is over.
        if w.reload_owed && w.cooldown <= 0.0 {
            w.reload_owed = false;
            if w.ammo == 0 && w.can_reload() {
                w.start_reload();
                out.push(WeaponAction::ReloadStarted);
            }
        }
        w.step_rechamber(fire_tick);
        // Minigun barrels spin up while the trigger is held and down over `spin_down_time`.
        if def.fire == FireMode::Minigun {
            w.spin = if held & buttons::FIRE != 0 && !blocked {
                (w.spin + dt).min(def.spin_up_time)
            } else {
                let down = def.timing.spin_down_time;
                let rate = if down > 0.0 { def.spin_up_time / down } else { f32::INFINITY };
                (w.spin - dt * rate).max(0.0)
            };
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
            FireMode::Stacked => pressed & buttons::FIRE != 0,
            FireMode::Burst(n) => {
                if pressed & buttons::FIRE != 0 && w.burst_left == 0 && w.cooldown <= 0.0 {
                    w.burst_left = n;
                }
                w.burst_left > 0
            }
        };
        if !trigger || blocked || w.is_reloading() || w.rechamber.is_some() {
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
                    w.start_reload();
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
        let spread_now = self.spread_now(stance, ads);
        let w = self.current_mut();
        if !(held & buttons::FIRE != 0) {
            w.string = 0;
        }
        let mut fired_semi = false;
        while w.cooldown <= 0.0 && w.ammo > 0 {
            if matches!(def.fire, FireMode::Semi | FireMode::Stacked) && fired_semi {
                break;
            }
            if def.class == Class::Launcher || (def.projectile_speed > 0.0 && def.class != Class::Shotgun && def.effect != Effect::None) {
                if def.fire == FireMode::Stacked {
                    // A pull empties the stack: once one bolt is out, BO2 keeps firing while the
                    // stack is part-spent (`FUN_008e1320` treats `FUN_008dc0c0` as the trigger)
                    // with the fire delay zeroed (`FUN_008e0780`). Each bolt's event carries the
                    // rounds loaded before it, which picks its place in the fan (`FUN_0066e320`);
                    // `pellet` is that count - 1.
                    for loaded in (1..=w.ammo).rev() {
                        out.push(WeaponAction::Launch { def, pellet: loaded - 1 });
                    }
                } else {
                    out.push(WeaponAction::Launch { def, pellet: 0 });
                }
            } else {
                let spread = spread_now + if ads < 0.5 { def.spread_move_add * (speed / 30.0).min(1.0) } else { 0.0 };
                // BO2's per-shot kick (G5): the view kick's velocity and the gun spring's impulse.
                // A dual-wield pair reads both hands' kicks with their firing states (this hand
                // fires; the other is firing while its fire cycle runs). The left gun's def is
                // the pair's own (`Hand::full`).
                let hands = match &w.left {
                    Some(other) => {
                        let other_firing = other.cooldown > 0.0;
                        super::recoil::Hands::Dual { left: def, right_firing: !left || other_firing, left_firing: left || other_firing }
                    }
                    None => super::recoil::Hands::Single,
                };
                let k = super::recoil::shot_kick_hands(def, hands, ads, w.string, &mut w.recoil_acc, w.since_shot, rng);
                w.since_shot = 0.0;
                for pellet in 0..def.rounds_per_shot() {
                    let jitter = (rng.signed(), rng.signed());
                    let (view_kick, gun_kick) = if pellet == 0 { (k.view, k.gun) } else { ((0.0, 0.0, 0.0), (0.0, 0.0)) };
                    out.push(WeaponAction::Fire { spread_deg: spread, jitter, view_kick, gun_kick, pellet, last: w.ammo == 1, left });
                }
            }
            // The magazine's last round fires over `iLastFireTime` when the weapon has one
            // (`FUN_008dc260` with `FUN_008dc1e0`'s clip < 2 test). A burst's threshold is its
            // round count + 1 there; no BO2 burst weapon authors a last-fire time, so bursts keep
            // the plain fire time.
            let last = w.ammo < 2 && !matches!(def.fire, FireMode::Burst(_));
            let t = &def.timing;
            w.cooldown += if last && t.last_fire_time > 0.0 { t.last_fire_time } else { def.fire_time };
            // A stack spends every loaded round in one pull.
            w.ammo -= if def.fire == FireMode::Stacked { w.ammo } else { 1 };
            w.string = w.string.saturating_add(1);
            w.bloom += def.spread_fire_add;
            // A bolt or pump action owes a rechamber after every shot, or after every
            // `iShotsBeforeRechamber` shots (`FUN_008dc260`).
            if t.bolt_action {
                if t.shots_before_rechamber == 0 {
                    w.rechamber_pending = true;
                } else {
                    w.rechamber_shots = w.rechamber_shots.saturating_add(1);
                    w.rechamber_pending |= w.rechamber_shots >= t.shots_before_rechamber;
                }
                if w.rechamber_pending {
                    break;
                }
            }
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
            w.reload_owed = true;
        }
    }

    /// Lethal (G) and tactical (Q) use. Returns whether it owns this frame.
    fn step_offhand(&mut self, held: u32, pressed: u32, released: u32, out: &mut Vec<WeaponAction>) -> bool {
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
                self.current_mut().interrupt();
                self.ads = 0.0;
                self.offhand = Some(OffhandUse::Holding { lethal, held_for: 0.0 });
                self.cooked = 0.0;
                out.push(WeaponAction::OffhandReady { lethal });
                true
            }
            Some(OffhandUse::Holding { lethal, held_for }) => {
                let held_for = held_for + self.tick(self.offhand_timing(lethal));
                self.cooked += LOGIC_DT;
                let button = if lethal { buttons::LETHAL } else { buttons::TACTICAL };
                let def = if lethal { self.lethal.def } else { self.tactical.def };
                let fuse_left = if def.cook { def.fuse_time - self.cooked } else { def.fuse_time };
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
                let t = t + self.tick(self.offhand_timing(lethal));
                if t >= OFFHAND_THROW {
                    self.offhand = None;
                    let d = self.current().def;
                    (self.raise_left, self.raise_kind) = (d.raise_time, RaiseKind::Normal);
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

/// A magazine reload's length and the moment its rounds go in (seconds of weapon time). The
/// length (`FUN_008dee50`): the quick reload's `iReloadQuick{,Empty}Time` when `quick`, else the
/// normal or empty reload. The add (`FUN_008dec90`): `iReloadQuickAddTime` or `iReloadAddTime`; an
/// empty reload takes `iReloadEmptyAddTime` when it has one, else that same add time; capped at
/// the *normal* (or normal empty) reload time, and never past the end.
fn magazine_times(d: &WeaponDef, empty: bool, quick: bool) -> (f32, f32) {
    let t = &d.timing;
    let normal = if empty { d.reload_empty_time } else { d.reload_time };
    let quick_length = if empty { t.reload_quick_empty_time } else { t.reload_quick_time };
    // A table without the quick times (0) keeps the normal reload.
    let length = if quick && quick_length > 0.0 { quick_length } else { normal };
    let add = if quick { t.reload_quick_add_time } else { d.reload_add_time };
    let add = if empty && d.reload_empty_add_time > 0.0 { d.reload_empty_add_time } else { add };
    let add_at = if add > 0.0 && add < normal { add } else { normal };
    (length, add_at.min(length))
}

/// The combination holds an attachment of BO2's `eAttachment` `kind` (`FUN_008eb270`), resolved
/// through its base weapon's allowed list (a combination's own is cleared) and the catalog's
/// attachment table.
fn has_attachment_kind(def: &WeaponDef, kind: u8) -> bool {
    if def.attachments.is_empty() {
        return false;
    }
    let cat = catalog();
    let Some(base) = cat.get(crate::attachments::split_id(&def.id).0) else { return false };
    let shorts: Vec<&str> = def.attachments.iter().map(String::as_str).collect();
    crate::attachments::resolve(base, &shorts, |id| cat.attachment(id)).iter().any(|a| a.kind == kind)
}
