/*
** Conquer Ops - Zero Hour gameplay core: a weapon off a player, and the arsenal's side of BO2's
** weapon drops and pickups: which weapon a death drops, the ammo a walk-over takes, the swap.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's rules, each from its source:
//! * **What drops** (`_weapons.gsc:342-450`, `maydropweapon` and `dropweaponfordeath`): the last
//!   weapon of the `primary` inventory held (an alt mode is its weapon's), never a held killstreak
//!   gun (`isheldkillstreakweapon`; the data marks the same guns `bIsCarriedKillstreakWeapon`), and
//!   only with rounds in its magazine or stock; the stock is capped at the weapon's maximum.
//! * **A walk-over** (t6mp `FUN_005faa60` → `FUN_005fa880`, per mode of the item: the weapon and
//!   its alt): a player holding the exact weapon is offered its stock and magazine, one holding a
//!   weapon of the same base (`FUN_008dda40`) its stock only; what he can carry moves to his stock
//!   (`FUN_005fa6a0`, capped at the maximum), taken from the item's stock first, then its
//!   magazine. Holding the exact weapon, any ammo taken takes the item away.
//! * **The swap** (use; `FUN_005fb7d0` → `FUN_005fb450`): with fewer than two primaries (a carried
//!   killstreak gun not counted, `FUN_008dd9e0`) the weapon is simply given; otherwise one is put
//!   down — a held melee-type primary first (the held knife, `FUN_008eb4d0`), else the weapon up
//!   (`FUN_005f9050`) — where the item lay, with its rounds and options, if it has any rounds
//!   (`FUN_005f93b0`; a clip-only weapon's empty magazine is none), and the new one is given with
//!   the item's options, magazine and stock (`FUN_005fa9b0`) and switched to (`FUN_0066f4f0`).

use super::catalog::catalog;
use super::def::{Inventory, WeaponDef};
use super::handling::WeapType;
use super::weapon_state::WeaponState;
use super::{Arsenal, RaiseKind};

/// What a weapon wears that is not its class's: the options it was dropped with (BO2's item
/// carries the weapon's options, `FUN_005fb0e0` → `+0x8c`, and the pickup gives them,
/// `FUN_00670460`): its camo, optic reticle and reticle colour.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponOptions {
    pub camo: String,
    pub reticle: String,
    pub reticle_color: String,
}

crate::statehash::state_hash!(struct WeaponOptions { camo, reticle, reticle_color });

/// One weapon off a player (BO2's dropped item): the weapon (its main mode, with its attachments),
/// the rounds of each mode (`itemweaponsetammo`: the weapon's, and its alt mode's when that keeps
/// its own) and its options.
#[derive(Clone, Debug, PartialEq)]
pub struct Carried {
    pub def: &'static WeaponDef,
    pub clip: u16,
    pub stock: u16,
    pub alt_clip: u16,
    pub alt_stock: u16,
    pub options: WeaponOptions,
}

crate::statehash::state_hash!(struct Carried { def, clip, stock, alt_clip, alt_stock, options });

/// A weapon's base (`an94_mp+gl+reflex` → `an94_mp`, `gl_an94_mp~an94_mp+gl` → `gl_an94_mp`):
/// what BO2's same-weapon test compares (`FUN_008dda40`).
pub fn base_of(id: &str) -> &str {
    id.split(['+', '~']).next().unwrap_or(id)
}

/// Whether BO2 ever drops `def` (`maydropweapon`: the `primary` inventory; not a held killstreak
/// gun, `isheldkillstreakweapon`/`bIsCarriedKillstreakWeapon`).
pub fn droppable(def: &WeaponDef) -> bool {
    def.inventory == Inventory::Primary && !def.drop.carried_killstreak
}

/// One of the primaries a pickup swaps against (`FUN_008dd9e0`): the `primary` inventory, a
/// carried killstreak gun aside.
fn counts_as_primary(def: &WeaponDef) -> bool {
    def.inventory == Inventory::Primary && !def.drop.carried_killstreak
}

impl Carried {
    /// The weapon in `w` with its rounds, as it leaves the player (`dropweaponfordeath`: the
    /// stock capped at the weapon's maximum).
    pub fn from_state(w: &WeaponState, options: WeaponOptions) -> Carried {
        let (clip, stock) = w.main_rounds();
        let (alt_clip, alt_stock) = match w.alt_mode.filter(|_| w.alt_owns_ammo()) {
            Some(alt) => {
                let (c, s) = if w.alt_up() { (w.ammo, w.reserve) } else { (w.alt_ammo, w.alt_reserve) };
                (c, s.min(alt.max_reserve))
            }
            None => (0, 0),
        };
        Carried { def: w.main, clip, stock: stock.min(w.main.max_reserve), alt_clip, alt_stock, options }
    }

    /// Rounds left in any mode (`anyammoforweaponmodes`, then the clip-and-stock test).
    pub fn has_rounds(&self) -> bool {
        self.clip as u32 + self.stock as u32 + self.alt_clip as u32 + self.alt_stock as u32 > 0
    }

    /// The weapon's alt mode when it keeps rounds of its own (an under-barrel launcher).
    fn own_ammo_alt(&self) -> Option<&'static WeaponDef> {
        catalog().get(&self.def.alt).filter(|a| a.alt_own_ammo || self.def.alt_own_ammo)
    }

    /// A new weapon state holding it: its magazine and stock, its alt mode's, a dual-wield left
    /// gun full (BO2 gives the weapon and sets only the item's clip, `FUN_005fa9b0`), its options.
    pub fn into_state(self) -> WeaponState {
        let mut w = WeaponState::new(self.def);
        w.ammo = self.clip.min(self.def.clip);
        w.reserve = self.stock;
        if self.own_ammo_alt().is_some() {
            w.alt_ammo = self.alt_clip;
            w.alt_reserve = self.alt_stock;
        }
        w.options = Some(self.options);
        w
    }
}

/// What a walk-over did (`FUN_005fa880`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmmoTaken {
    /// Some ammo moved (BO2 adds EV_AMMO_PICKUP).
    pub any: bool,
    /// The item goes: the player holds the exact weapon and took some.
    pub remove: bool,
}

/// Why a swap was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapRefused {
    /// Two primaries and none to put down (`GAME_CANT_GET_PRIMARY_WEAP_MESSAGE`).
    NoPrimaryToDrop,
}

impl Arsenal {
    /// The weapon a death drops (`dropweaponfordeath`): the weapon up, as its main mode, when BO2
    /// drops it and it has rounds. Its index in `weapons`.
    pub fn death_drop(&self) -> Option<usize> {
        let w = self.current();
        let (clip, stock) = w.main_rounds();
        (droppable(w.main) && clip as u32 + stock.min(w.main.max_reserve) as u32 > 0).then_some(self.active)
    }

    /// BO2's primaries count (`FUN_008dd9e0`): the hint says "pick up" under two, "swap" at two.
    pub fn primaries(&self) -> usize {
        self.weapons.iter().filter(|w| counts_as_primary(w.main)).count()
    }

    /// The player holds `def` itself (`FUN_0049efb0`): the swap is refused, a walk-over is offered
    /// the magazine too.
    pub fn holds_exact(&self, def: &WeaponDef) -> bool {
        self.weapons.iter().any(|w| w.main.id == def.id)
    }

    /// The weapon state matching `def` for its ammo: the exact weapon, else one of its base
    /// (`FUN_005f9120`).
    fn ammo_match(&self, def: &WeaponDef) -> Option<usize> {
        self.weapons.iter().position(|w| w.main.id == def.id).or_else(|| self.weapons.iter().position(|w| base_of(&w.main.id) == base_of(&def.id)))
    }

    /// Walk over `item` (`FUN_005faa60`): take the ammo the player can carry from it.
    pub fn take_ammo(&mut self, item: &mut Carried) -> AmmoTaken {
        let Some(i) = self.ammo_match(item.def) else { return AmmoTaken::default() };
        let exact = self.weapons[i].main.id == item.def.id;
        let mut any = false;
        // The weapon's own mode.
        {
            let w = &mut self.weapons[i];
            let max = w.main.max_reserve;
            let (_, reserve) = w.main_rounds_mut();
            any |= give(reserve, max, &mut item.stock, &mut item.clip, exact);
        }
        // Its alt mode's own rounds (an under-barrel launcher), into the matching weapon's alt.
        if let (Some(alt), Some(w_alt)) = (item.own_ammo_alt(), self.weapons[i].alt_mode) {
            if base_of(&alt.id) == base_of(&w_alt.id) && self.weapons[i].alt_owns_ammo() {
                let w = &mut self.weapons[i];
                let reserve = if w.alt_up() { &mut w.reserve } else { &mut w.alt_reserve };
                any |= give(reserve, w_alt.max_reserve, &mut item.alt_stock, &mut item.alt_clip, exact);
            }
        }
        AmmoTaken { any, remove: exact && any }
    }

    /// The swap's slot: the weapon put down (or given beside, `None`). Refused with two primaries
    /// and none to put down.
    fn swap_slot(&self) -> Result<Option<usize>, SwapRefused> {
        if self.primaries() < 2 {
            return Ok(None);
        }
        let melee = self.weapons.iter().position(|w| counts_as_primary(w.main) && w.main.traits.weap_type == WeapType::Melee);
        let up = Some(self.active).filter(|&i| counts_as_primary(self.weapons[i].main));
        melee.or(up).map(Some).ok_or(SwapRefused::NoPrimaryToDrop)
    }

    /// Use `item` (the swap, see the module doc): the weapon given and switched to, and the one put
    /// down (with `drop_options`, its options) if it had rounds.
    pub fn pick_up(&mut self, item: Carried, drop_options: impl Fn(&WeaponState) -> WeaponOptions) -> Result<Option<Carried>, SwapRefused> {
        let slot = self.swap_slot()?;
        let new = item.into_state();
        let (index, put_down) = match slot {
            None => {
                self.weapons.push(new);
                (self.weapons.len() - 1, None)
            }
            Some(i) => {
                let old = std::mem::replace(&mut self.weapons[i], new);
                let carried = Carried::from_state(&old, drop_options(&old));
                let rounds = if old.main.traits.clip_only { carried.clip > 0 } else { carried.has_rounds() };
                (i, rounds.then_some(carried))
            }
        };
        if index == self.active {
            // The weapon up was taken: the new one comes straight up (nothing to put away).
            self.current_mut().interrupt();
            self.drop_left = 0.0;
            self.drop_total = 0.0;
            self.ads = 0.0;
            let to = self.current().def;
            let empty = self.current().clip_empty();
            self.current_mut().raised_once = true;
            (self.raise_left, self.raise_kind) = if empty { (to.empty_raise_time, RaiseKind::Empty) } else { (to.first_raise_time, RaiseKind::First) };
            self.raise_total = self.raise_left;
        } else {
            self.switch_to(index);
        }
        Ok(put_down)
    }
}

/// Move what `reserve` can hold (up to `max`) from the item's stock (and, `with_clip`, its
/// magazine after); whether any moved (`FUN_005fa6a0`/`FUN_005fa880`).
fn give(reserve: &mut u16, max: u16, stock: &mut u16, clip: &mut u16, with_clip: bool) -> bool {
    let offered = *stock as u32 + if with_clip { *clip as u32 } else { 0 };
    let room = max.saturating_sub(*reserve) as u32;
    let taken = offered.min(room);
    if taken == 0 {
        return false;
    }
    *reserve += taken as u16;
    let from_stock = taken.min(*stock as u32);
    *stock -= from_stock as u16;
    *clip -= (taken - from_stock) as u16;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::{weapon, Loadout};

    fn kit(spec: &str) -> Arsenal {
        let mut a = Arsenal::with_loadout(Loadout::unarmed().with_spec(spec));
        a.raise_left = 0.0;
        a
    }

    fn item(id: &str, clip: u16, stock: u16) -> Carried {
        Carried { def: weapon(id), clip, stock, alt_clip: 0, alt_stock: 0, options: WeaponOptions { camo: "camo_gold".into(), ..Default::default() } }
    }

    #[test]
    fn a_death_drops_the_weapon_up_with_rounds_and_never_the_knife() {
        let a = kit("primary=an94_mp;secondary=fnp45_mp");
        assert_eq!(a.death_drop(), Some(0));
        let mut dry = a.clone();
        dry.weapons[0].ammo = 0;
        dry.weapons[0].reserve = 0;
        assert_eq!(dry.death_drop(), None, "no rounds: nothing drops");
        let knife = kit("melee=knife_held_mp");
        assert_eq!(knife.death_drop(), None, "the held knife has no rounds");
    }

    #[test]
    fn a_walk_over_takes_the_stock_and_with_the_exact_weapon_the_magazine_and_the_item() {
        let mut a = kit("primary=an94_mp;secondary=fnp45_mp");
        let max = a.weapons[0].main.max_reserve;
        a.weapons[0].reserve = max - 10;
        let mut it = item("an94_mp", 30, 4);
        let t = a.take_ammo(&mut it);
        assert_eq!(t, AmmoTaken { any: true, remove: true });
        assert_eq!(a.weapons[0].reserve, max);
        assert_eq!((it.stock, it.clip), (0, 24), "the stock first, then the magazine");

        // Same base, other attachments: the stock only, and the item stays.
        let mut b = kit("primary=an94_mp+reflex;secondary=fnp45_mp");
        b.weapons[0].reserve = 0;
        let mut it = item("an94_mp", 30, 40);
        let t = b.take_ammo(&mut it);
        assert_eq!(t, AmmoTaken { any: true, remove: false });
        assert_eq!(b.weapons[0].reserve, 40);
        assert_eq!((it.stock, it.clip), (0, 30));

        // Full: nothing moves.
        let mut full = kit("primary=an94_mp;secondary=fnp45_mp");
        full.weapons[0].reserve = max;
        let mut it = item("an94_mp", 30, 4);
        assert_eq!(full.take_ammo(&mut it), AmmoTaken::default());
    }

    #[test]
    fn the_swap_replaces_the_held_knife_first_then_the_weapon_up() {
        // One gun: the knife is the second primary and goes for the new gun.
        let mut a = kit("primary=an94_mp");
        assert_eq!(a.primaries(), 2);
        let put = a.pick_up(item("svu_mp", 5, 10), |_| WeaponOptions::default()).unwrap();
        assert!(put.is_none(), "the knife has no rounds and is not put down");
        assert_eq!(a.weapons.iter().map(|w| w.main.id.as_str()).collect::<Vec<_>>(), ["an94_mp", "svu_mp"]);
        assert_eq!(a.active, 1, "switched to the new weapon");
        assert_eq!(a.raise_kind, RaiseKind::First);
        assert_eq!((a.current().ammo, a.current().reserve), (5, 10));
        assert_eq!(a.current().options.as_ref().unwrap().camo, "camo_gold");

        // Two guns: the weapon up is put down, where it was, with its rounds.
        let mut b = kit("primary=an94_mp;secondary=fnp45_mp");
        let put = b.pick_up(item("svu_mp", 5, 10), |_| WeaponOptions { camo: "camo_dot".into(), ..Default::default() }).unwrap().unwrap();
        assert_eq!(put.def.id, "an94_mp");
        assert_eq!(put.options.camo, "camo_dot");
        assert_eq!(b.weapons[0].main.id, "svu_mp");
        assert_eq!(b.active, 0);
        assert!(b.drop_left == 0.0 && b.raise_left > 0.0, "nothing to put away: the new one comes up");
    }

    #[test]
    fn droppable_follows_the_inventory_and_the_killstreak_flag() {
        assert!(droppable(weapon("an94_mp")));
        assert!(!droppable(weapon("frag_grenade_mp")));
        let mut ks = weapon("an94_mp").clone();
        ks.drop.carried_killstreak = true;
        assert!(!droppable(&ks));
    }
}
