/*
** Conquer Ops - Zero Hour gameplay core: the first-person weapon state machine: raise, fire, reload, offhands, melee.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::catalog::{catalog, weapon, weapon_or_none};
use super::def::{Class, FireMode, Place, WeaponDef};
use super::loadout::{class_melee, melee_weapon, Loadout};
use super::perks::Perk;
use super::rng::Rng;
use super::specialty::{Specialties, Specialty};
use super::zoom;
use crate::input::buttons;
use crate::tuning::Tuning;
use crate::units::LOGIC_DT;
use super::weapon_state::{Reload, WeaponState};


/// A grenade or other offhand item and how many are left.
#[derive(Clone, Debug, PartialEq)]
pub struct Offhand {
    pub def: &'static WeaponDef,
    pub count: u16,
}

crate::statehash::state_hash!(struct Offhand { def, count });

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
    Launch { def: &'static WeaponDef, pellet: u16, spread_deg: f32, jitter: (f32, f32) },
    /// Throw an offhand item; `fuse_left` is what remains after cooking.
    /// `lethal`: thrown from the lethal slot (whatever the item's class: C4 is equipment, and
    /// Tactician puts a tactical there).
    Throw { def: &'static WeaponDef, fuse_left: f32, lethal: bool },
    /// Cooked too long: it goes off in hand.
    CookedOff { def: &'static WeaponDef },
    /// Pin pulled (the throw animation's wind-up starts).
    OffhandReady { lethal: bool },
    /// BO2's plant check refused a `plantable` item (event 0xd5, "Cannot Plant Here"); nothing is
    /// used.
    CantPlant { lethal: bool },
    /// Set equipment down in front of the player (Claymore, Bouncing Betty, Trophy System,
    /// Tactical Insertion).
    Plant { def: &'static WeaponDef },
    /// Set off the player's C4 (double-tap reload, or the lethal button with none left in hand).
    Detonate,
    /// The Assault Shield's trigger: plant it as cover (`deploy_riotshield`); its one round is
    /// spent, the sim refills it if the placement is refused.
    DeployShield { def: &'static WeaponDef },
    /// Switched to or from the weapon's alt mode.
    AltToggled,
    /// The melee swing starts (BO2's `EV_MELEE_SWIPE`, event 0x27 in `FUN_008dc770`): the swipe sound.
    MeleeSwing,
    /// The melee strike lands (`EV_FIRE_MELEE`, 0x28): when the melee weapon's `melee_delay` has run
    /// out after the swing (`FUN_008e4da0`, state 0x13 -> 0x14).
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

crate::statehash::state_hash!(enum RaiseKind { First, Normal, Quick, Empty, Alt });

/// Offhand use in progress: BO2's offhand states 0x16-0x1d (Ghidra `FUN_008e2bb0` starts them for
/// any offhand-slot weapon, grenade or equipment alike; MODLOG "Offhands run BO2's offhand states").
/// Every `t` is weapon time, which runs at `fasttoss`/`fastequipmentuse`'s rate (G3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OffhandUse {
    /// Pressed while another weapon state ran (0x16, `FUN_008dcb70`): that state ends, but its
    /// `weaponTime` stays and runs out first — the weapon frame (`FUN_008e4da0`) switches state
    /// only once `weaponTime` and `weaponDelay` are 0, and 0x16 zeroes only the delay — at the
    /// offhand's rate (0x16 is one of G3's offhand states). `left` is that time; at 0 the
    /// pull-back or the detonator starts (`FUN_008e2710`).
    Starting { lethal: bool, left: f32 },
    /// The pull-back (state 0x17, the item's `hold_fire_time`), then held (0x1a) for as long as
    /// the button is (`FUN_008e28e0`). A cooking grenade's fuse burns from the end of the pull-back.
    Holding { lethal: bool, held_for: f32 },
    /// Released (0x18, `FUN_008e2ac0`): the item leaves the hand at its `fire_delay` (`fired`),
    /// the state lasts its `fire_time`.
    Throwing { lethal: bool, t: f32, fired: bool },
    /// A `hold_use` item (the Black Hat) put away in its `quick_drop_time` (`FUN_008dcb00`).
    Dropping { lethal: bool, t: f32 },
    /// The detonator (0x19, `FUN_008e2710`/`FUN_008dcd20`): a `has_detonator` item's button with
    /// none left in hand; the charges go off at `detonate_delay`, the state lasts `detonate_time`
    /// and the held weapon is simply back (no raise).
    Detonating { lethal: bool, t: f32, fired: bool },
    /// The held weapon back up in its `quick_raise_time` (0x1d, `FUN_008dcbe0`), still at the
    /// offhand's rate; another offhand may start now (`FUN_008e2bb0` allows 0x1d).
    Raising { lethal: bool, t: f32 },
}

crate::statehash::state_hash!(enum OffhandUse {
    Starting { lethal, left },
    Holding { lethal, held_for },
    Throwing { lethal, t, fired },
    Dropping { lethal, t },
    Detonating { lethal, t, fired },
    Raising { lethal, t },
});

impl OffhandUse {
    /// Whether the lethal slot's item (else the tactical's) is the one in use.
    pub fn lethal(self) -> bool {
        match self {
            OffhandUse::Starting { lethal, .. }
            | OffhandUse::Holding { lethal, .. }
            | OffhandUse::Throwing { lethal, .. }
            | OffhandUse::Dropping { lethal, .. }
            | OffhandUse::Detonating { lethal, .. }
            | OffhandUse::Raising { lethal, .. } => lethal,
        }
    }
}

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
    /// The sights are wanted this frame (BO2's `PMF_SIGHT_AIMING`, `0x10`): the aim button down
    /// and nothing refusing the aim ([`Arsenal::step`]'s `wants_ads`). The walk reads it
    /// ([`Arsenal::move_scale`]).
    pub sights: bool,
    /// Seconds since sprinting stopped (weapon comes back up over `sprint_out_time`).
    pub since_sprint: f32,
    /// The melee weapon's `melee_time` left (states 0x13/0x14), in plain time.
    pub melee_left: f32,
    /// The held gun coming back up after a melee with the class melee weapon (state 0x15,
    /// `FUN_008e1f50`): its `quick_raise_time` left, which Fast Melee Recovery runs faster.
    pub melee_recover_left: f32,
    /// Seconds until the swing's strike lands (`ps->weaponDelay` in state 0x13); 0 when none is due.
    pub melee_delay_left: f32,
    pub offhand: Option<OffhandUse>,
    /// Whether BO2's plant check (`FUN_008e1780`) passes for the lethal and the tactical slot's
    /// item this frame; the sim sets it from the world before [`Arsenal::step`]
    /// ([`crate::sim::can_plant`]). Only `plantable` items read it.
    pub plant_ok: [bool; 2],
    /// BO2's aim spread scale (`ps->aimSpreadScale`, [`super::aim_spread`]): one per player, both
    /// hands of a dual-wield pair and every weapon share it.
    pub aim_spread: super::aim_spread::AimSpread,
    /// What this frame's spread update reads; the sim sets it from the world before
    /// [`Arsenal::step`] (the stance and the ground speed come in through `step` itself).
    pub spread_in: super::aim_spread::SpreadIn,
    /// The scale this frame's shots start from: the one the last frame left.
    shot_scale: f32,
    /// Weapon-switch presses made during an offhand, applied when it is done.
    pub swaps_pending: u8,
    held_last: u32,
    /// Seconds since reload was last pressed (a second press soon after detonates C4).
    since_reload_press: f32,
    /// Real seconds the held grenade has cooked (the fuse burns in real time; the offhand
    /// states' own clock runs at the perk's rate).
    cooked: f32,
    /// A live grenade picked up to throw back (BO2's pick-up `FUN_005d2140` makes it the offhand
    /// weapon, its fuse the grenade's time left): the lethal offhand use in progress is this
    /// grenade, not the slot's item, until it leaves the hand ([`Arsenal::throw_back`]).
    pub throwback: Option<&'static WeaponDef>,
    /// Specialties given outside the class (Zombies' perks, `sim::perks`).
    pub granted: Specialties,
    /// The tuning pack's perk multipliers, taken when the kit was made (the pack is loaded once at
    /// init, identically on every client).
    pub tuning: Tuning,
}

// `tuning` is the tuning pack, loaded once at init and the same on every machine (the content
// CRC covers it).
crate::statehash::state_hash!(struct Arsenal {
    loadout, weapons, active, lethal, tactical, melee, class_melee, raise_left, raise_total, raise_kind, drop_left,
    drop_total, previous, alt_pending, ads, sights, since_sprint, melee_left, melee_recover_left, melee_delay_left, offhand,
    plant_ok, aim_spread, spread_in, shot_scale, swaps_pending, held_last, since_reload_press, cooked, throwback, granted,
} skip { tuning });

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
    /// The re-raise after a melee (0x15): `fastmeleerecovery` (the swing itself, 0x13/0x14, runs
    /// in plain time).
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
            sights: false,
            since_sprint: 10.0,
            melee_left: 0.0,
            melee_recover_left: 0.0,
            melee_delay_left: 0.0,
            offhand: None,
            plant_ok: [true, true],
            aim_spread: Default::default(),
            spread_in: Default::default(),
            shot_scale: 0.0,
            swaps_pending: 0,
            held_last: 0,
            since_reload_press: 10.0,
            cooked: 0.0,
            throwback: None,
            granted: Specialties::NONE,
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
                // Saturating: a class's own reserve (a hand-edited profile) can sit at the type's top.
                w.reserve = w.reserve.saturating_add(w.def.clip).min(w.def.max_reserve.max(w.reserve));
            }
        }
        // Offhands start with BO2's start count (`start_reserve` for them); an empty or unknown
        // slot has none.
        let knife = || weapon("knife_held_mp");
        // Each offhand slot carries the class's count when it sets one (BO2's `primarygrenadecount`
        // — Danger Close's two — and `specialgrenadecount`); whatever item the lethal slot holds, a
        // tactical under Tactician too.
        let offhand = |id: &str, count: Option<u16>| {
            if id.is_empty() {
                return Offhand { def: knife(), count: 0 };
            }
            let Some(def) = weapon_or_none(id) else { return Offhand { def: knife(), count: 0 } };
            Offhand { def, count: count.unwrap_or(def.start_reserve.max(1)) }
        };
        let melee = if loadout.melee.is_empty() { knife() } else { weapon_or_none(&loadout.melee).unwrap_or_else(knife) };
        // BO2 gives the held knife as a weapon when the class has fewer than two
        // (`_class.gsc` `giveloadout`: `if ( initialweaponcount < 2 ) giveweapon( "knife_held_mp" )`),
        // after the guns, and spawns holding it only when there is no gun at all (`initialweaponcount
        // == 0`). So a one-gun class switches between the gun and the knife, and a kit with no
        // resolvable gun is unarmed but never leaves `weapons` empty (every consumer reads
        // `current()`).
        // Zombies gives no held knife (`Loadout::held_knife`); the weapons are never left empty.
        if (weapons.len() < 2 && loadout.held_knife) || weapons.is_empty() {
            let mut knife = WeaponState::new(melee);
            knife.raised_once = weapons.is_empty();
            weapons.push(knife);
        }
        let raise = weapons[0].def.first_raise_time;
        weapons[0].raised_once = true;
        Self {
            lethal: offhand(&loadout.lethal, loadout.lethal_count),
            tactical: offhand(&loadout.tactical, loadout.tactical_count),
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
            sights: false,
            since_sprint: 10.0,
            melee_left: 0.0,
            melee_recover_left: 0.0,
            melee_delay_left: 0.0,
            offhand: None,
            plant_ok: [true, true],
            aim_spread: Default::default(),
            spread_in: Default::default(),
            shot_scale: 0.0,
            swaps_pending: 0,
            held_last: 0,
            since_reload_press: 10.0,
            cooked: 0.0,
            throwback: None,
            granted: Specialties::NONE,
            tuning: crate::tuning::get(),
        }
    }

    /// The weapon a melee uses now: the held weapon when it is a knife, else the class melee
    /// weapon ([`melee_weapon`]). The held weapon cannot change mid-melee (a swap waits for it).
    pub fn melee_weapon(&self) -> &'static WeaponDef {
        melee_weapon(self.current().def, self.class_melee)
    }

    /// The melee just started is a charge (the lunge): BO2 times it with the melee weapon's
    /// charge time and delay instead (`FUN_008dc770`, +0x488/+0x438), where the def has them.
    pub fn charge_melee(&mut self) {
        let m = self.melee_weapon();
        if m.melee_charge_time > 0.0 {
            self.melee_left = m.melee_charge_time;
        }
        if m.melee_charge_delay > 0.0 && self.melee_delay_left > 0.0 {
            self.melee_delay_left = m.melee_charge_delay;
        }
    }

    pub fn has(&self, perk: Perk) -> bool {
        self.loadout.perks.contains(&perk)
    }

    /// The specialties the class's perks grant.
    pub fn class_specialties(&self) -> Specialties {
        self.loadout.perks.iter().fold(self.granted, |s, p| s.union(p.specialties()))
    }

    /// BO2's current weapon for the specialty test (`FUN_008ddfe0`): the offhand while it is in
    /// use, the melee weapon while meleeing, else the weapon up.
    pub fn current_weapon(&self) -> &'static WeaponDef {
        match self.offhand {
            Some(u) => self.offhand_def(u.lethal()),
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
        let family = if let Some(u) = self.offhand {
            Some(self.offhand_timing(u.lethal()))
        } else if self.melee_left > 0.0 {
            None
        } else if self.melee_recover_left > 0.0 {
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

    /// The item in the lethal or the tactical slot (the lethal use is a picked-up grenade's while
    /// one is in hand, [`Arsenal::throwback`]).
    pub fn offhand_def(&self, lethal: bool) -> &'static WeaponDef {
        if let (true, Some(def)) = (lethal, self.throwback) {
            return def;
        }
        if lethal {
            self.lethal.def
        } else {
            self.tactical.def
        }
    }

    /// The specialty that speeds an offhand's states (G3): `fastequipmentuse` for BO2's offhand
    /// slot 3 — `Class::Equipment` is that slot (the converter's `class_of`), whichever
    /// Create-a-Class slot lists the item (the Sensor Grenade is a tactical there) — else `fasttoss`.
    fn offhand_timing(&self, lethal: bool) -> Timing {
        let def = self.offhand_def(lethal);
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

    /// A weapon change asked for now waits (`FUN_008e0e60` holds the request back): an offhand's
    /// states (0x16-0x1d), the melee states (0x13-0x15, the strike's delay included) and an alt
    /// toggle under way (`FUN_008db500` returns in states 3-5). The field computer is held to it.
    pub fn holds_switch_back(&self) -> bool {
        self.offhand.is_some() || self.melee_left > 0.0 || self.melee_recover_left > 0.0 || self.melee_delay_left > 0.0 || self.alt_pending
    }

    /// The General leaves first person (the field computer): the sights come down and a reload in
    /// progress is cut short (both hands), as a swap cuts it (`switch_to`).
    pub fn put_away(&mut self) {
        self.current_mut().interrupt();
        self.ads = 0.0;
        self.sights = false;
    }

    /// Pick up a live grenade of `def` with `fuse_left` seconds on it to throw back (BO2's
    /// pick-up, t6mp `FUN_005d2140`: the offhand weapon becomes the grenade, `grenadeTimeLeft` its
    /// time left; `FUN_00607730` then throws it as the picker's). It starts as a lethal press would
    /// (`FUN_008e2bb0`, no count needed: it is not the slot's), its pull-back then its release —
    /// the throw button is not held, so it leaves at once — and its fuse keeps burning from the
    /// pick-up: held too long it goes off in hand. Refused (false) with another offhand in hand,
    /// while the weapon is put away, or with a grenade already picked up.
    pub fn throw_back(&mut self, def: &'static WeaponDef, fuse_left: f32, out: &mut Vec<WeaponAction>) -> bool {
        if self.throwback.is_some() || self.drop_left > 0.0 || matches!(self.offhand, Some(u) if !matches!(u, OffhandUse::Raising { .. })) {
            return false;
        }
        self.throwback = Some(def);
        self.start_offhand_use(true, out);
        // Fast Hands' `pin_back`: what is left is bumped up to `perk_grenadeTossBackTimer`
        // (t6mp `FUN_005d2140`: `grenadeTimeLeft = max(left, timer)` with specialty 0x2e, else
        // the grenade's own time left).
        let fuse_left = if self.has_specialty(crate::weapons::specialty::Specialty::PinBack) { fuse_left.max(self.tuning.perk_grenade_toss_back_timer) } else { fuse_left };
        self.cooked = (def.fuse_time - fuse_left).max(0.0);
        true
    }

    /// Whether the weapon-switch key has another weapon to cycle to. BO2's client cycles its
    /// selection over the weapons held (`FUN_00537c20` → `FUN_005378d0`) and requests a change only
    /// when that lands on a different one; with a single weapon it lands on the same and nothing
    /// is sent, so there is no switch to hold back or to carry.
    pub fn can_cycle(&self) -> bool {
        self.weapons.len() > 1
    }

    /// The weapon can't shoot right now (raising, putting away, reloading, meleeing, throwing).
    pub fn is_busy(&self) -> bool {
        self.raise_left > 0.0 || self.drop_left > 0.0 || self.current().is_reloading() || self.melee_left > 0.0 || self.melee_recover_left > 0.0 || self.offhand.is_some()
    }

    /// Movement speed multiplier from the held weapon (`FUN_008c4110` PM_CmdScale_Walk). Aiming
    /// is BO2's `PMF_WALKING` (`FUN_008c5310`: the sights wanted, not prone — the reload states
    /// already refuse them) or, prone, an ADS fraction above 0: the speed takes `AdsWalkScale`
    /// (0.4) and the weapon's `adsMoveSpeedScale` (`FUN_008ed080`: the def's, an attachment's
    /// replacing it); otherwise its `moveSpeedScale`, and a def without one takes the ADS scale
    /// alone. Lightweight is the movement's own ([`Arsenal::move_faster`]).
    pub fn move_scale(&self, prone: bool) -> f32 {
        let d = self.current().def;
        let aiming = if prone { self.ads > 0.0 } else { self.sights };
        let walk = if aiming { self.tuning.ads_walk_scale } else { 1.0 };
        if d.move_scale > 0.0 && !aiming {
            walk * d.move_scale
        } else {
            walk * d.ads_move_scale
        }
    }

    /// What the player's own blasts and stuns do to them. Flak Jacket does not cover his own
    /// blasts: `cac_modified_damage` returns the damage unchanged when the victim is the attacker
    /// (`_class.gsc:957-958`; another General's blasts go through [`crate::sim::guard`]), so the
    /// blast share is 1. `stunprotection` cuts a stun to x0.1 whoever threw it
    /// (`_weapons.gsc:1375-1376`), the thrower's own included: the guard applies it to every
    /// victim, so the own share is 1 here too.
    pub fn self_harm_scales(&self) -> (f32, f32) {
        (1.0, 1.0)
    }

    /// The hip spread cone's min and max at an eye height (inches; Ghidra G6): the weapon's
    /// stance values blended by eye height, x0.65 with `bulletaccuracy`.
    pub fn spread_cone(&self, view_height: f32) -> (f32, f32) {
        super::recoil::spread_range(self.current().def, view_height, self.has_specialty(Specialty::BulletAccuracy), &self.tuning)
    }

    /// The eye height (inches) the cone blends by: the sim's ([`super::aim_spread::SpreadIn`]),
    /// else the stance's resting height.
    pub fn view_height(&self) -> f32 {
        let t = &self.tuning;
        let i = &self.spread_in;
        i.view_height.unwrap_or([t.player_standing_view_height, t.view_height_crouch, t.view_height_prone][i.stance.min(2)])
    }

    /// The spread a shot would fly with now (degrees): the aim spread scale's place in the cone
    /// (`FUN_0066e590`, [`super::aim_spread::AimSpread::shot_spread`]).
    pub fn spread_now(&self) -> f32 {
        super::aim_spread::AimSpread::shot_spread(self.aim_spread.scale, self.spread_cone(self.view_height()), self.current().def, self.ads)
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

    /// Take the weapon up away (BO2's `takeweapon` on the current weapon, as Pack-a-Punch takes
    /// it): the next weapon comes up, or the melee weapon held when it was the only one.
    pub fn take_current(&mut self) -> WeaponState {
        let i = self.active;
        if self.weapons.len() == 1 {
            let mut knife = WeaponState::new(self.class_melee);
            knife.raised_once = true;
            return std::mem::replace(&mut self.weapons[0], knife);
        }
        let taken = self.weapons.remove(i);
        self.active = 0;
        self.previous = 0;
        self.current_mut().raised_once = true;
        (self.raise_left, self.raise_kind) = (self.current().def.raise_time, RaiseKind::Normal);
        self.raise_total = self.raise_left;
        self.drop_left = 0.0;
        self.drop_total = 0.0;
        self.ads = 0.0;
        taken
    }

    /// Raise and drop take the weapon's own times; `fastweaponswitch` runs them at its rate
    /// ([`Timing::Switch`]) rather than switching to the quick animations (G3). A weapon with an
    /// empty clip (`WeaponState::clip_empty`) drops over its `iEmptyDropTime` (`FUN_008db500`) and raises over its
    /// `iEmptyRaiseTime`, which beats the first raise (`FUN_008df320`; the first raise is spent
    /// either way).
    pub(super) fn switch_to(&mut self, index: usize) {
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
        self.sights = false;
        // The raise opens the spread scale (`FUN_008df320`), once the drop is done.
        if self.drop_left <= 0.0 {
            self.aim_spread.open();
        }
    }

    /// A shield was deployed: the engine's `deployriotshield` takes the weapon (it lies on the
    /// ground as the shield's retrieve item) and the script brings up the last other weapon, giving
    /// `knife_held_mp` when there is none (`_riotshield.gsc` `watchriotshielddeploy`). Returns the
    /// slot it held, its state, and whether the held knife was given.
    pub fn take_shield(&mut self) -> Option<(usize, WeaponState, bool)> {
        let slot = self.weapons.iter().position(|w| w.def.is_shield())?;
        let state = self.weapons.remove(slot);
        let gave_knife = self.weapons.is_empty();
        if gave_knife {
            self.weapons.push(WeaponState::new(self.melee));
        }
        let fix = |i: usize| if i > slot { i - 1 } else { i };
        let back = fix(self.previous).min(self.weapons.len() - 1);
        // The shield's own deploy animation put it down: the next weapon only raises.
        self.active = if self.weapons[back].def.class.is_slot_weapon() || gave_knife { back } else { 0 };
        self.previous = self.active;
        self.drop_left = 0.0;
        let to = self.current().def;
        let first = !self.current().raised_once;
        self.current_mut().raised_once = true;
        (self.raise_left, self.raise_kind) = if first { (to.first_raise_time, RaiseKind::First) } else { (to.raise_time, RaiseKind::Normal) };
        self.raise_total = self.raise_left;
        self.ads = 0.0;
        self.sights = false;
        self.aim_spread.open();
        Some((slot, state, gave_knife))
    }

    /// The deployed shield was picked up (its retrieve item): the weapon returns to its slot, the
    /// held knife given for it goes (`riotshieldtakeweapon`), and the shield comes up.
    pub fn return_shield(&mut self, slot: usize, mut state: WeaponState, gave_knife: bool) {
        if gave_knife {
            if let Some(k) = self.weapons.iter().position(|w| w.def.id == self.melee.id) {
                self.weapons.remove(k);
            }
        }
        state.ammo = state.def.clip;
        let slot = slot.min(self.weapons.len());
        self.weapons.insert(slot, state);
        if self.weapons.len() == 1 {
            (self.active, self.previous) = (0, 0);
            self.raise_left = self.current().def.raise_time;
            self.raise_total = self.raise_left;
            return;
        }
        let shift = |i: usize| if i >= slot { i + 1 } else { i };
        (self.active, self.previous) = (shift(self.active).min(self.weapons.len() - 1), shift(self.previous).min(self.weapons.len() - 1));
        self.switch_to(slot);
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
            self.sights = false;
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
        if w.alt_owns_ammo() {
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
            self.sights = false;
        }
        self.raise_left = raise;
        self.raise_total = raise;
        self.raise_kind = RaiseKind::Alt;
        // An alt pair's raise keeps at least half the spread scale (`FUN_008df320`, `FUN_008db3f0`);
        // two ADS-only modes skip the state setup that sets it (`FUN_008da850`).
        if !ads_only {
            self.aim_spread.alt_raised(&self.tuning);
        }
    }

    /// A shot from the Zero Hour weapon in `slot` (0 primary, 1 secondary, 2 lethal) while the
    /// General is under RTS control: it comes out of the magazine, which Zero Hour's own clip
    /// reload refills from the spare rounds. The Zero Hour weapon is the gun's own (its main mode),
    /// so the rounds are too, even when the General left first person with the alt mode up.
    pub fn rts_shot(&mut self, slot: usize) {
        if slot == 2 {
            self.lethal.count = self.lethal.count.saturating_sub(1);
            return;
        }
        let Some(w) = self.weapons.get_mut(slot) else { return };
        w.reload = None;
        let clip = w.main.clip;
        let (ammo, reserve) = w.main_rounds_mut();
        if *ammo == 0 {
            let take = clip.min(*reserve);
            *ammo += take;
            *reserve -= take;
        }
        *ammo = ammo.saturating_sub(1);
    }

    /// Rounds in the magazine and spare for RTS slot `slot` (grenades for the lethal): the gun's
    /// own, as [`Arsenal::rts_shot`] spends them.
    pub fn rts_ammo(&self, slot: usize) -> (u16, u16) {
        match slot {
            2 => (self.lethal.count, 0),
            s => self.weapons.get(s).map_or((0, 0), WeaponState::main_rounds),
        }
    }

    /// A Scavenger pack picked up (`scavenger_think`, `_weapons.gsc:1791-1890`).
    ///
    /// * Every gun (`getweaponslistprimaries()`, `:1795`): `int( clip * scavenger_clip_multiplier )`
    ///   more rounds in its stock, capped at its most (`:1863-1888`); the weapon's own clip into its
    ///   own stock, never an alt mode's (an under-barrel launcher is in the alts list, which only
    ///   refills the offhands it names), whichever mode is up.
    /// * Each offhand the switch names (`:1816-1851`) gets one more, up to the loadout's count for
    ///   the class's own item (`primarygrenadecount`/`specialgrenadecount`: Danger Close's two
    ///   lethals refill to two). Not the Tactical Insertion (not in the switch); not the C4 while one
    ///   of the player's is in the world (`c4_down`, `anyobjectsinworld`); a lethal item (statstable
    ///   slot `primarygrenade`) not when the class gives no lethal (`grenadetypeprimarycount < 1`),
    ///   and — the switch falls through — no item when the class holds a tactical but gives none
    ///   of it (`grenadetypesecondarycount < 1`).
    pub fn scavenge(&mut self, c4_down: bool) {
        let multiplier = self.tuning.scavenger_clip_multiplier;
        for w in &mut self.weapons {
            let (clip, max) = (w.main.clip, w.main.max_reserve);
            let add = (f32::from(clip) * multiplier).max(0.0) as u16;
            let (_, reserve) = w.main_rounds_mut();
            *reserve = reserve.saturating_add(add).min(max);
        }
        let (lethal, tactical) = (self.class_count(true), self.class_count(false));
        let tactical_held = !self.loadout.tactical.is_empty();
        for is_lethal_slot in [true, false] {
            let o = if is_lethal_slot { &self.lethal } else { &self.tactical };
            let kind = o.def.equipment_kind;
            let lethal_item = o.def.cac_slot == "primarygrenade";
            let skip = o.def.id.is_empty()
                || kind == crate::weapons::Kind::Insertion
                || (kind == crate::weapons::Kind::C4 && c4_down)
                || (lethal_item && lethal < 1)
                || (tactical_held && tactical < 1);
            if skip {
                continue;
            }
            let max = if is_lethal_slot { lethal } else { tactical };
            let o = if is_lethal_slot { &mut self.lethal } else { &mut self.tactical };
            if o.count < max {
                o.count += 1;
            }
        }
    }

    /// How many of an offhand the class gives (the count [`Arsenal::with_loadout`] starts it with):
    /// the loadout's lethal or tactical count, else the item's start count; none for an empty slot.
    pub fn class_count(&self, lethal: bool) -> u16 {
        let (id, count) = if lethal { (&self.loadout.lethal, self.loadout.lethal_count) } else { (&self.loadout.tactical, self.loadout.tactical_count) };
        if id.is_empty() {
            return 0;
        }
        weapon_or_none(id).map_or(0, |d| count.unwrap_or(d.start_reserve.max(1)))
    }

    /// Advance one logic frame. `stance` is 0 stand, 1 crouch, 2 prone; `speed` the ground speed.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, held: u32, pressed: u32, sprinting: bool, stance: usize, speed: f32, rng: &mut Rng, out: &mut Vec<WeaponAction>) {
        let dt = LOGIC_DT;
        let pressed = pressed | (held & !self.held_last);
        let released = self.held_last & !held;
        self.held_last = held;
        // BO2's aim spread update is the frame's first step (`FUN_008e0080`, at the top of
        // `Pmove`); this frame's shots fly with the scale the last frame left
        // (`currentAimSpreadScale`, taken before `Pmove`).
        self.spread_in.stance = stance;
        self.spread_in.speed = speed / crate::units::ZH_PER_INCH;
        self.shot_scale = self.aim_spread.scale;
        let held_def = self.current().def;
        self.aim_spread.update(&self.spread_in, held_def, self.ads, &self.tuning, dt);

        // Weapon timers run in weapon-data time: each family at its specialty's rate (G3).
        let (sprint_tick, melee_tick, switch_tick) = (self.tick(Timing::SprintOut), self.tick(Timing::Melee), self.tick(Timing::Switch));
        self.since_sprint = if sprinting { 0.0 } else { self.since_sprint + sprint_tick };
        // The swing runs its `melee_time` in plain time (0x13/0x14); when it ends with a gun up (the
        // class melee weapon struck, `FUN_008e1f50`: held != melee weapon) the gun comes back up in
        // its own `quick_raise_time` (0x15), the one melee state Fast Melee Recovery speeds up.
        self.melee_recover_left = (self.melee_recover_left - melee_tick).max(0.0);
        if self.melee_left > 0.0 {
            self.melee_left = (self.melee_left - dt).max(0.0);
            if self.melee_left <= 1e-6 {
                self.melee_left = 0.0;
                let held = self.current().def;
                if held.id != self.melee_weapon().id {
                    self.melee_recover_left = held.quick_raise_time;
                }
            }
        }
        // The strike lands when the swing's delay runs out. BO2 counts the delay in plain time:
        // state 0x13 takes no specialty scaling (`FUN_008e0780`; Fast Melee Recovery scales 0x15).
        if self.melee_delay_left > 0.0 {
            self.melee_delay_left -= dt;
            if self.melee_delay_left <= 1e-6 {
                self.melee_delay_left = 0.0;
                out.push(WeaponAction::Melee { damage: self.melee_weapon().melee_damage });
            }
        }
        if self.drop_left > 0.0 {
            self.drop_left = (self.drop_left - switch_tick).max(0.0);
            if self.drop_left <= 0.0 {
                if self.alt_pending {
                    self.finish_alt();
                } else {
                    // The new weapon's raise begins: the spread scale opens (`FUN_008df320`).
                    self.aim_spread.open();
                }
            }
        } else {
            self.raise_left = (self.raise_left - switch_tick).max(0.0);
        }
        for w in &mut self.weapons {
            w.since_shot += dt;
        }

        // Offhands: hold to pull back (and cook), release to throw.
        // The weapon-switch key during an offhand: BO2's client cycles its weapon selection anyway
        // (`FUN_00537c20` → `FUN_005378d0`) unless it sends a cancelable item's cancel instead
        // (states 0x16-0x1b), and the server's switch check (`FUN_008e0e60`) only holds the
        // request while the offhand states run — the command keeps carrying it, so the switch
        // happens once the offhand is done. Each press cycles once more. With no other weapon to
        // cycle to (the knife alone, a shield deployed) the selection stays put and nothing is
        // requested ([`Arsenal::can_cycle`]): the press is not kept for later.
        if pressed & buttons::SWAP_WEAPON != 0 && self.can_cycle() {
            if let Some(u) = self.offhand {
                let cancels = self.offhand_def(u.lethal()).hold_cancelable && !matches!(u, OffhandUse::Raising { .. });
                if !cancels {
                    self.swaps_pending = self.swaps_pending.wrapping_add(1);
                }
            }
        }
        if self.step_offhand(held, pressed, released, out) {
            return;
        }

        // C4: a quick second press of reload sets it off (BO2's double tap).
        self.since_reload_press += dt;
        if pressed & buttons::RELOAD != 0 {
            if self.since_reload_press < self.tuning.detonate_double_tap {
                out.push(WeaponAction::Detonate);
            }
            self.since_reload_press = 0.0;
        }

        // Alt mode (select fire, grenade launcher, second optic).
        let meleeing = self.melee_left > 0.0 || self.melee_recover_left > 0.0;
        if self.alt_toggle_requested(held, pressed) && !meleeing && self.drop_left <= 0.0 {
            self.toggle_alt();
            out.push(WeaponAction::AltToggled);
        }

        // Swap (held back during the melee states 0x13-0x15, `FUN_008e0e60`).
        // The melee states hold a switch back the same way (`FUN_008e0e60` returns at 0x13-0x15
        // without touching the request): it happens once the swing and re-raise are done.
        // So does a shot's own time (states 6/0x28 with `weaponTime` left; the check runs after the
        // frame's tick, so this frame's fire tick counts first). The reload and rechamber states
        // (0x0b-0x12, 7, 8) do not hold it: a switch cuts them short. The function's other
        // hold-backs are states the mod has no counterpart of (0x1e-0x22, 0x2a/0x2b, 0x31-0x33) or a
        // running `weaponDelay`, which here is the melee strike's or an offhand's, both covered.
        // BO2 starts no weapon change during an alt drop (`FUN_008db500` returns in states 3-5).
        let shot_running = !self.current().is_reloading() && self.current().cooldown - self.tick(Timing::Fire) > 1e-6;
        let held_back = meleeing || shot_running;
        if pressed & buttons::SWAP_WEAPON != 0 && held_back && self.can_cycle() {
            self.swaps_pending = self.swaps_pending.wrapping_add(1);
        }
        if !self.can_cycle() {
            self.swaps_pending = 0;
        }
        let swaps = if self.swaps_pending > 0 { self.swaps_pending as usize } else { (pressed & buttons::SWAP_WEAPON != 0) as usize };
        if swaps > 0 && self.weapons.len() > 1 && !held_back && !self.alt_pending {
            self.swaps_pending = 0;
            let next = (self.active + swaps) % self.weapons.len();
            if next != self.active {
                self.switch_to(next);
                out.push(WeaponAction::Swapped);
            }
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
        if pressed & buttons::MELEE != 0 && !melee_refused && !meleeing && self.raise_left <= 0.0 && self.drop_left <= 0.0 {
            // The swing starts now and the strike lands after the melee weapon's delay
            // (`FUN_008dc770`: weaponTime = iMeleeTime, weaponDelay = iMeleeDelay, state 0x13).
            let m = self.melee_weapon();
            self.melee_left = m.melee_time.max(0.3);
            self.current_mut().interrupt();
            self.ads = 0.0;
            self.sights = false;
            out.push(WeaponAction::MeleeSwing);
            if m.melee_delay > 0.0 {
                self.melee_delay_left = m.melee_delay;
            } else {
                out.push(WeaponAction::Melee { damage: m.melee_damage });
            }
            return;
        }

        // ADS blend (`FUN_008e4470`): the fraction moves at 1/adsTransInTime in and
        // 1/adsTransOutTime out, `fastads` dividing the rate by `perk_weapAdsMultiplier`; never
        // while sprinting or until the weapon is back up from a sprint.
        let def = self.current().def;
        let sprint_ready = self.sprint_ready();
        // Only a def with `aimDownSight` aims (`FUN_008e4470` pins the fraction at 0 for any
        // other): a knife, a held offhand, the Assault Shield, the Ballistic Knife and a launcher
        // mode have no sights (and no ADS zoom); the shield's `ads_down` is its hip pose. A
        // dual-wield weapon has none either: its ADS button is the left gun's trigger.
        // `FUN_008daa10`: an alt toggle's drop and raise (states 5 and 2) let only an ADS-only
        // weapon aim; every other raise or drop refuses it.
        let switching = self.raise_left > 0.0 || self.drop_left > 0.0;
        let alt_switching = self.alt_pending || (self.raise_left > 0.0 && self.raise_kind == RaiseKind::Alt);
        let switch_ok = !switching || (alt_switching && def.traits.alt_ads_only);
        let wants_ads = def.class.is_slot_weapon()
            && def.aims
            && !def.is_dual_wield()
            && held & buttons::ADS != 0
            && !sprinting
            && sprint_ready
            && switch_ok
            && self.melee_recover_left <= 0.0
            && !self.current().is_reloading();
        let quickdraw = if self.has_specialty(Specialty::FastAds) && self.tuning.perk_weap_ads_multiplier > 0.0 { self.tuning.perk_weap_ads_multiplier } else { 1.0 };
        self.sights = wants_ads;
        self.ads = if wants_ads {
            (self.ads + dt / (def.ads_in_time.max(1e-3) * quickdraw)).min(1.0)
        } else if !def.aims {
            // `FUN_008e4470` sets the fraction straight to 0 for a def that does not aim.
            0.0
        } else {
            (self.ads - dt / (def.ads_out_time.max(1e-3) * quickdraw)).max(0.0)
        };

        // Each hand runs the reload and trigger rules on its own magazine and timers (BO2's
        // `PM_Weapon`, `FUN_008e4da0`, once per hand). The trigger is `FUN_008dbb20`'s button: the
        // attack button for a single gun and a dual-wield pair's right gun, the ADS button for the
        // left gun. The reload button reaches both hands (each checks it, `FUN_008dfd50`).
        let dual = self.current().left.is_some();
        self.step_hand(held, pressed, sprinting, rng, out, false);
        if dual {
            let trigger = |bits: u32| (bits & !buttons::FIRE) | if bits & buttons::ADS != 0 { buttons::FIRE } else { 0 };
            self.current_mut().swap_hand();
            self.step_hand(trigger(held), trigger(pressed), sprinting, rng, out, true);
            self.current_mut().swap_hand();
        }
        // The weapon frame's own maxes (`FUN_008e4da0`): crawling (prone with a move key down, not
        // fully aimed) and the melee states 0x13-0x15.
        let crawling = self.spread_in.stance == 2 && self.spread_in.moving && self.ads < 1.0;
        if crawling || self.melee_left > 0.0 || self.melee_recover_left > 0.0 {
            self.aim_spread.open();
        }
    }

    /// One hand's reload progress and requests, then its trigger: `held`/`pressed` carry that
    /// hand's trigger as `FIRE`. The left hand runs with its state swapped in
    /// ([`WeaponState::swap_hand`]); its actions say so (`left`).
    #[allow(clippy::too_many_arguments)]
    fn step_hand(&mut self, held: u32, pressed: u32, sprinting: bool, rng: &mut Rng, out: &mut Vec<WeaponAction>, left: bool) {
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
        // The Assault Shield fires no round: its trigger deploys it (the sim's shield placement),
        // once per press, after the weapon is up and its last deploy's time has run.
        if def.class == Class::Shield {
            let blocked = sprinting || !sprint_ready || self.raise_left > 0.0 || self.drop_left > 0.0 || self.melee_left > 0.0 || self.melee_recover_left > 0.0;
            let fire_tick = self.tick(Timing::Fire);
            let w = self.current_mut();
            w.cooldown = (w.cooldown - fire_tick).max(0.0);
            if pressed & buttons::FIRE != 0 && !blocked && w.cooldown <= 0.0 {
                if w.ammo > 0 {
                    w.ammo -= 1;
                    w.cooldown = def.fire_time;
                    out.push(WeaponAction::DeployShield { def });
                } else {
                    out.push(WeaponAction::DryFire);
                }
            }
            return;
        }
        if def.class == Class::Melee || def.class.is_offhand() {
            return;
        }

        // Firing.
        let ads = self.ads;
        let blocked = sprinting || !sprint_ready || self.raise_left > 0.0 || self.drop_left > 0.0 || self.melee_left > 0.0 || self.melee_recover_left > 0.0;
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
        let cone = self.spread_cone(self.view_height());
        let mut shot_scale = self.shot_scale;
        let mut shots = 0u32;
        let w = self.current_mut();
        if !(held & buttons::FIRE != 0) {
            w.string = 0;
        }
        let mut fired_semi = false;
        while w.cooldown <= 0.0 && w.ammo > 0 {
            if matches!(def.fire, FireMode::Semi | FireMode::Stacked) && fired_semi {
                break;
            }
            if def.launches() {
                // The rocket's spread is the shot's, its disc drawn as a bullet's (`FUN_0066e590`
                // -> `FUN_0066d710` -> `FUN_0066ce90`: the angle, then the radius).
                let spread_deg = super::aim_spread::AimSpread::shot_spread(shot_scale, cone, def, ads);
                if def.fire == FireMode::Stacked {
                    // A pull empties the stack: once one bolt is out, BO2 keeps firing while the
                    // stack is part-spent (`FUN_008e1320` treats `FUN_008dc0c0` as the trigger)
                    // with the fire delay zeroed (`FUN_008e0780`). Each bolt's event carries the
                    // rounds loaded before it, which picks its place in the fan (`FUN_0066e320`);
                    // `pellet` is that count - 1.
                    for loaded in (1..=w.ammo).rev() {
                        let jitter = (rng.next_f32(), rng.next_f32());
                        out.push(WeaponAction::Launch { def, pellet: loaded - 1, spread_deg, jitter });
                    }
                } else {
                    let jitter = (rng.next_f32(), rng.next_f32());
                    out.push(WeaponAction::Launch { def, pellet: 0, spread_deg, jitter });
                }
            } else {
                let spread = super::aim_spread::AimSpread::shot_spread(shot_scale, cone, def, ads);
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
                    // `Bullet_Fire`'s two draws per pellet: the angle, then the radius.
                    let jitter = (rng.next_f32(), rng.next_f32());
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
            shot_scale = super::aim_spread::AimSpread::after_shot(shot_scale, def, ads);
            shots += 1;
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
        // Each shot opens the scale (`FUN_008dc710`); a later shot of this hand in the same frame
        // already flew with it (`shot_scale`), as BO2's next command's would.
        for _ in 0..shots {
            self.aim_spread.fired(def, ads);
        }
    }

    /// Lethal (G) and tactical (Q) use: BO2's offhand states (see [`OffhandUse`]). Returns whether
    /// it owns this frame.
    fn step_offhand(&mut self, held: u32, pressed: u32, released: u32, out: &mut Vec<WeaponAction>) -> bool {
        // A picked-up grenade is the offhand only until it has left the hand.
        if matches!(self.offhand, None | Some(OffhandUse::Raising { .. })) {
            self.throwback = None;
        }
        // A new offhand may start from rest or while the gun comes back up after one (0x1d).
        if matches!(self.offhand, None | Some(OffhandUse::Raising { .. })) && self.start_offhand(pressed, out) {
            return true;
        }
        let Some(use_) = self.offhand else { return false };
        let lethal = use_.lethal();
        let def = self.offhand_def(lethal);
        let tick = self.tick(self.offhand_timing(lethal));
        match use_ {
            OffhandUse::Starting { left, .. } => {
                // A picked-up grenade's fuse burns from the pick-up.
                if lethal && self.throwback.is_some() {
                    self.cooked += LOGIC_DT;
                }
                let left = left - tick;
                if left <= 1e-6 {
                    self.begin_offhand(lethal, out);
                } else {
                    self.offhand = Some(OffhandUse::Starting { lethal, left });
                }
            }
            OffhandUse::Holding { held_for, .. } => {
                let held_for = held_for + tick;
                let pulled_back = held_for >= def.timing.hold_fire_time - 1e-6;
                // The fuse is set when the pull-back ends (`FUN_008e28a0`) and burns in plain time
                // (`FUN_008e2e40`).
                let thrown_back = lethal && self.throwback.is_some();
                if pulled_back || thrown_back {
                    self.cooked += LOGIC_DT;
                }
                if (def.cook || thrown_back) && def.fuse_time - self.cooked <= 0.0 {
                    // Cooked too long.
                    self.use_offhand(lethal);
                    self.offhand = Some(OffhandUse::Throwing { lethal, t: def.timing.fire_delay, fired: true });
                    out.push(WeaponAction::CookedOff { def });
                    return true;
                }
                let button = if lethal { buttons::LETHAL } else { buttons::TACTICAL };
                // Held after the pull-back with the button still down (0x1a, `FUN_008e28e0`), an
                // `offhandHoldIsCancelable` item is put away unused when the weapon-switch button
                // comes up (the client sends the cancel from weapnext's release, `FUN_00537c20`,
                // instead of cycling): the gun's quick raise (`FUN_008dcbe0`), the count untouched
                // and the fuse dropped (it is set afresh at the next pull-back).
                // In the held state the plant check runs first (`FUN_008e28e0`): refused, the item
                // is put away unused (`FUN_008dcbe0`).
                if pulled_back && !self.plant_allowed(lethal) {
                    self.cooked = 0.0;
                    self.offhand = Some(OffhandUse::Raising { lethal, t: 0.0 });
                    out.push(WeaponAction::CantPlant { lethal });
                    return true;
                }
                if pulled_back && def.hold_cancelable && held & button != 0 && released & buttons::SWAP_WEAPON != 0 {
                    self.cooked = 0.0;
                    self.offhand = Some(OffhandUse::Raising { lethal, t: 0.0 });
                    return true;
                }
                let let_go = released & button != 0 || held & button == 0;
                self.offhand = Some(if !pulled_back || !let_go {
                    OffhandUse::Holding { lethal, held_for }
                } else if def.hold_use {
                    OffhandUse::Dropping { lethal, t: 0.0 }
                } else {
                    OffhandUse::Throwing { lethal, t: 0.0, fired: false }
                });
                // An item with no delay leaves the hand on the release.
                if let Some(OffhandUse::Throwing { t, fired: false, .. }) = self.offhand {
                    self.step_throw(lethal, t, out);
                }
            }
            OffhandUse::Throwing { t, .. } => self.step_throw(lethal, t + tick, out),
            OffhandUse::Dropping { t, .. } => {
                let t = t + tick;
                self.offhand = Some(if t >= def.quick_drop_time { OffhandUse::Raising { lethal, t: 0.0 } } else { OffhandUse::Dropping { lethal, t } });
            }
            OffhandUse::Detonating { t, fired, .. } => {
                let t = t + tick;
                let fired = fired || {
                    let due = t >= def.detonate_delay - 1e-6;
                    if due {
                        out.push(WeaponAction::Detonate);
                    }
                    due
                };
                // The detonate state ends with the held weapon simply ready (`FUN_008dc9d0`).
                self.offhand = if t >= def.timing.detonate_time.max(def.detonate_delay) { None } else { Some(OffhandUse::Detonating { lethal, t, fired }) };
            }
            OffhandUse::Raising { t, .. } => {
                let t = t + tick;
                if t >= self.current().def.quick_raise_time {
                    self.offhand = None;
                } else {
                    self.offhand = Some(OffhandUse::Raising { lethal, t });
                }
            }
        }
        true
    }

    /// The release state at `t`: the item leaves the hand at its `fire_delay` (its count goes down
    /// then, `FUN_008e2ac0`), and the held weapon starts back up after its `fire_time`.
    fn step_throw(&mut self, lethal: bool, t: f32, out: &mut Vec<WeaponAction>) {
        let def = self.offhand_def(lethal);
        let fired = matches!(self.offhand, Some(OffhandUse::Throwing { fired: true, .. }));
        if !fired && t >= def.timing.fire_delay - 1e-6 && !self.plant_allowed(lethal) {
            // Refused at the release's delay too (`FUN_008e2ac0`): put away, not used.
            self.offhand = Some(OffhandUse::Raising { lethal, t: 0.0 });
            out.push(WeaponAction::CantPlant { lethal });
            return;
        }
        if !fired && t >= def.timing.fire_delay - 1e-6 {
            self.use_offhand(lethal);
            let thrown_back = lethal && self.throwback.is_some();
            let fuse_left = if def.cook || thrown_back { def.fuse_time - self.cooked } else { def.fuse_time };
            out.push(match def.place {
                Place::Plant => WeaponAction::Plant { def },
                _ => WeaponAction::Throw { def, fuse_left, lethal },
            });
        } else if !fired && (def.cook || (lethal && self.throwback.is_some())) {
            // Still in the hand: the fuse burns on.
            self.cooked += LOGIC_DT;
        }
        let fired = fired || t >= def.timing.fire_delay - 1e-6;
        self.offhand = Some(if t >= def.fire_time.max(def.timing.fire_delay) - 1e-6 {
            self.raise_left = 0.0;
            OffhandUse::Raising { lethal, t: 0.0 }
        } else {
            OffhandUse::Throwing { lethal, t, fired }
        });
    }

    /// The offhand buttons (`FUN_008e2bb0`): the slot's item starts its pull-back, or, a
    /// `has_detonator` item with none left in hand, its detonator — at once from rest or from the
    /// offhand's own re-raise (0x1d), else once the interrupted state's time has run out
    /// ([`OffhandUse::Starting`]). Returns whether one was started.
    fn start_offhand(&mut self, pressed: u32, out: &mut Vec<WeaponAction>) -> bool {
        let lethal = pressed & buttons::LETHAL != 0;
        let tactical = pressed & buttons::TACTICAL != 0;
        // BO2 refuses the press only in the put-away states (3-5) and its own offhand states
        // (`FUN_008e2bb0`); a raise, a shot, a reload, a melee swing (0x13/0x14) and the sprint-out
        // (0x26) are waited out instead.
        if !(lethal || tactical) || self.drop_left > 0.0 {
            return false;
        }
        let slot = if lethal { &self.lethal } else { &self.tactical };
        if slot.count == 0 && !slot.def.has_detonator {
            return false;
        }
        // The press itself is refused, nothing starting (`FUN_008e2bb0`: event 0xd5, return).
        if slot.count > 0 && !self.plant_allowed(lethal) {
            out.push(WeaponAction::CantPlant { lethal });
            return false;
        }
        self.start_offhand_use(lethal, out);
        true
    }

    /// The offhand use the press (or a throwback's pick-up) starts: the interrupted state's time
    /// waited out first ([`OffhandUse::Starting`]), or at once.
    fn start_offhand_use(&mut self, lethal: bool, out: &mut Vec<WeaponAction>) {
        // `FUN_008e2bb0` goes straight to `FUN_008e2710` only from 0x1d; any other state is
        // replaced by 0x16 with its `weaponTime` kept.
        let left = if matches!(self.offhand, Some(OffhandUse::Raising { .. })) { 0.0 } else { self.state_time_left() };
        let w = self.current_mut();
        // Both hands of a dual-wield pair stop (`interrupt`).
        w.interrupt();
        w.cooldown = 0.0;
        self.ads = 0.0;
        self.sights = false;
        self.cooked = 0.0;
        self.raise_left = 0.0;
        // An offhand may start while the gun comes back up from a melee (0x15 is not among
        // `FUN_008e2bb0`'s exclusions); what is left of that re-raise is waited out first.
        self.melee_recover_left = 0.0;
        // Mid-swing (0x13) the strike was due when `weaponDelay` ran out; 0x16 zeroes the delay,
        // so a strike not yet landed never does. The swing's time is waited out.
        self.melee_left = 0.0;
        self.melee_delay_left = 0.0;
        // The sprint-out (0x26) is the interrupted state too.
        self.since_sprint = self.since_sprint.max(self.current().def.sprint_out_time);
        if left > 1e-6 {
            self.offhand = Some(OffhandUse::Starting { lethal, left });
        } else {
            self.begin_offhand(lethal, out);
        }
    }

    /// BO2's plant check for the slot's item: always passes unless the item is `plantable`.
    fn plant_allowed(&self, lethal: bool) -> bool {
        !self.offhand_def(lethal).plantable || self.plant_ok[if lethal { 0 } else { 1 }]
    }

    /// The weapon time BO2's current state still holds (`ps->weaponTime`), which an offhand press
    /// keeps: the raise (states 1/2), the re-raise after a melee (0x15), the reload (the whole
    /// reload's time for a magazine, whose `weaponDelay` is the ammo add; the current part for a
    /// shell-by-shell one), or the shot's fire time (6/7). Every timer here has already been
    /// counted down this frame, as `FUN_008e0780` runs before the offhand check.
    fn state_time_left(&self) -> f32 {
        let w = self.current();
        if self.raise_left > 0.0 {
            self.raise_left
        } else if self.melee_recover_left > 0.0 {
            self.melee_recover_left
        } else if self.melee_left > 0.0 {
            // The swing (0x13/0x14): its `iMeleeTime` left.
            self.melee_left
        } else if w.is_reloading() {
            w.reload_part_left()
        } else if self.since_sprint > 0.0 && self.since_sprint < w.def.sprint_out_time {
            // The sprint-out (0x26) after a sprint that ended before this frame.
            w.def.sprint_out_time - self.since_sprint
        } else {
            // The fire block counts the cooldown down after the offhands; this frame's tick
            // first.
            (w.cooldown - self.tick(Timing::Fire)).max(0.0)
        }
    }

    /// `FUN_008e2710`: the pull-back (0x17) or, a `has_detonator` item with none left, the
    /// detonator (0x19).
    fn begin_offhand(&mut self, lethal: bool, out: &mut Vec<WeaponAction>) {
        let slot = if lethal { &self.lethal } else { &self.tactical };
        if slot.count == 0 && slot.def.has_detonator && !(lethal && self.throwback.is_some()) {
            self.offhand = Some(OffhandUse::Detonating { lethal, t: 0.0, fired: false });
        } else {
            self.offhand = Some(OffhandUse::Holding { lethal, held_for: 0.0 });
            out.push(WeaponAction::OffhandReady { lethal });
        }
    }

    /// The Black Hat is up and aimed (seconds held), if it is.
    pub fn hacking(&self) -> Option<f32> {
        match self.offhand {
            Some(OffhandUse::Holding { lethal, held_for }) => (self.offhand_def(lethal).place == Place::Hack).then_some(held_for),
            _ => None,
        }
    }

    /// The hacking tool in hand while [`Arsenal::hacking`] is `Some`.
    pub fn hacking_def(&self) -> Option<&'static WeaponDef> {
        match self.offhand {
            Some(OffhandUse::Holding { lethal, .. }) => Some(self.offhand_def(lethal)).filter(|d| d.place == Place::Hack),
            _ => None,
        }
    }

    /// The Black Hat finished a hack: it uses one charge and is put away.
    pub fn hack_done(&mut self) {
        if let Some(OffhandUse::Holding { lethal, .. }) = self.offhand {
            self.use_offhand(lethal);
            self.offhand = Some(OffhandUse::Dropping { lethal, t: 0.0 });
        }
    }

    pub fn use_offhand(&mut self, lethal: bool) {
        // A picked-up grenade is not the slot's: the slot keeps its count.
        if lethal && self.throwback.is_some() {
            return;
        }
        let slot = if lethal { &mut self.lethal } else { &mut self.tactical };
        slot.count = slot.count.saturating_sub(1);
    }
}

impl Default for Arsenal {
    fn default() -> Self {
        Self::new()
    }
}


/// The combination holds an attachment of BO2's `eAttachment` `kind` (`FUN_008eb270`), resolved
/// through its base weapon's allowed list (a combination's own is cleared) and the catalog's
/// attachment table.
pub(crate) fn has_attachment_kind(def: &WeaponDef, kind: u8) -> bool {
    if def.attachments.is_empty() {
        return false;
    }
    let cat = catalog();
    let Some(base) = cat.get(crate::attachments::split_id(&def.id).0) else { return false };
    let shorts: Vec<&str> = def.attachments.iter().map(String::as_str).collect();
    crate::attachments::resolve(base, &shorts, |id| cat.attachment(id)).iter().any(|a| a.kind == kind)
}
