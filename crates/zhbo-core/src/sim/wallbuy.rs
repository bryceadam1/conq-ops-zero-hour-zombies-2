/*
** Conquer Ops - Zero Hour gameplay core: BO2 Zombies' wall buys.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! TranZit's wall buys (`zombies.ini` `[WallBuys]`, `zhbo_core::zombies::WallBuys`) on a Zombies
//! map. The round engine stands them as it starts: the converter's prop of each one's wall model
//! (`names::wall_buy_object`), the whole set on a ring around each player start (`[Layout]`, the
//! mod's own map: its survivors start apart, where BO2's co-op starts together). The sim keeps which is which (`Rounds::wall_buys`, hashed).
//!
//! The use key on one in a survivor's use list (as a dropped weapon's: within `player_useRadius`,
//! the best of distance and facing) is BO2's `weapon_spawn_think` (`_zm_weapons.gsc:1969-2181`):
//!
//! * **Without the weapon** (nor its upgrade): its price from the wallet, then `weapon_give`
//!   (`:2327-2476`): a gun comes with its start ammo and is switched to, taking the place of the
//!   one up when he carries his limit (two); a lethal grenade takes the lethal slot, a melee
//!   weapon the melee (`change_melee_weapon`), the Claymore the tactical slot (BO2 keeps it in
//!   its own placeable-mine slot, which the mod's arsenal does not have).
//! * **With it**: ammo at its ammo price, or `UpgradedAmmoCost` for the upgrade, when he is short
//!   of it (`ammo_give`, `:2523-2565`: the stock below the start ammo, then `givemaxammo`, its
//!   alt mode's too). A melee buy sells nothing twice.
//! * Short of points, nothing (BO2's `no_purchase`).

use super::*;
use crate::weapons::{Carried, WeaponOptions};
use crate::zombies::{WallBuy, WallBuys};

/// One wall buy on the map.
#[derive(Clone, Debug, PartialEq)]
pub struct WallBuyAt {
    /// Its row in [`WallBuys`]'s list.
    pub index: u8,
    pub pos: Vec3,
    pub yaw: f32,
    /// Its prop (0: none stood).
    pub object: u32,
}

crate::statehash::state_hash!(struct WallBuyAt { index, pos, yaw, object });

/// What a wall buy offers `p` now: the weapon, or ammo for the weapon he holds of it.
#[derive(Clone, Debug, PartialEq)]
pub enum Offer {
    Weapon { cost: u32 },
    Ammo { cost: u32 },
    /// A melee weapon he has: nothing.
    Nothing,
}

/// The held weapon of `p` that is `id` or its upgrade (BO2's `has_weapon_or_upgrade`, by base).
fn held(p: &PlayerSim, b: &WallBuy) -> Option<usize> {
    let upgrade = b.upgrade();
    p.arsenal.weapons.iter().position(|w| {
        let base = crate::weapons::base_of(&w.main.id);
        base == b.weapon || upgrade.as_deref() == Some(base)
    })
}

fn is_melee(def: &crate::weapons::WeaponDef) -> bool {
    def.class == crate::weapons::Class::Melee
}

/// What `b` offers `p` (`wall_weapon_update_prompt`, `weapon_spawn_think`).
pub fn offer(p: &PlayerSim, b: &WallBuy) -> Offer {
    let Some(def) = crate::weapons::weapon_or_none(&b.weapon) else { return Offer::Nothing };
    let has = if is_melee(def) {
        p.arsenal.class_melee.id == b.weapon
    } else if def.class == crate::weapons::Class::Lethal {
        p.arsenal.lethal.def.id == b.weapon
    } else if def.class == crate::weapons::Class::Equipment || def.class == crate::weapons::Class::Tactical {
        p.arsenal.tactical.def.id == b.weapon
    } else {
        held(p, b).is_some()
    };
    if !has {
        return Offer::Weapon { cost: b.cost() };
    }
    if is_melee(def) {
        return Offer::Nothing;
    }
    let upgraded = held(p, b).is_some_and(|i| crate::weapons::base_of(&p.arsenal.weapons[i].main.id) != b.weapon);
    Offer::Ammo { cost: if upgraded { WallBuys::get().upgraded_ammo_cost } else { b.ammo_cost() } }
}

/// `weapon_give`: `def` into `p`'s arsenal as a wall buy gives it.
pub fn give(p: &mut PlayerSim, def: &'static crate::weapons::WeaponDef) {
    match def.class {
        crate::weapons::Class::Melee => {
            p.arsenal.class_melee = def;
            p.arsenal.loadout.melee = def.id.clone();
        }
        crate::weapons::Class::Lethal => {
            p.arsenal.lethal = crate::weapons::Offhand { def, count: def.start_reserve.max(1) };
            p.arsenal.loadout.lethal = def.id.clone();
        }
        crate::weapons::Class::Tactical | crate::weapons::Class::Equipment => {
            p.arsenal.tactical = crate::weapons::Offhand { def, count: def.start_reserve.max(1) };
            p.arsenal.loadout.tactical = def.id.clone();
        }
        _ => {
            let item = Carried { def, clip: def.clip, stock: def.start_reserve, alt_clip: 0, alt_stock: 0, options: WeaponOptions::default() };
            let _ = p.arsenal.pick_up(item, |w| w.options.clone().unwrap_or_default());
        }
    }
}

/// `ammo_give`: whether `p` was short of ammo for his weapon `i` (its stock under
/// `weaponstartammo`, BO2's whole start count), filling it and its alt mode's (`givemaxammo`).
pub fn give_ammo(p: &mut PlayerSim, i: usize) -> bool {
    let w = &mut p.arsenal.weapons[i];
    // While the alt mode is up the main mode's stock waits in `alt_reserve` (`WeaponState`).
    let main_up = std::ptr::eq(w.def, w.main);
    let stock = if main_up { w.reserve } else { w.alt_reserve };
    if stock as u32 >= w.main.start_reserve as u32 + w.main.clip as u32 {
        return false;
    }
    let alt_max = w.alt_mode.filter(|a| a.alt_own_ammo).map(|a| a.max_reserve);
    if main_up {
        w.reserve = w.main.max_reserve;
        if let Some(a) = alt_max {
            w.alt_reserve = a;
        }
    } else {
        w.alt_reserve = w.main.max_reserve;
        if let Some(a) = alt_max {
            w.reserve = a;
        }
    }
    true
}

impl Sim {
    /// Stand the wall buys (the round engine's start, on a Zombies map): every one around each
    /// survivor's start, evenly on the `ring`, facing it, owned as `owner` (a spawn point: the
    /// map's neutral side).
    pub(super) fn place_wall_buys(&mut self, host: &dyn LogicHost, owner: u32) {
        let w = WallBuys::get();
        let starts: Vec<Vec3> = self.living_survivors(host).iter().map(|o| o.pos).collect();
        if starts.is_empty() || !self.rounds.wall_buys.is_empty() {
            return;
        }
        let n = starts.len();
        for start in &starts {
            for (i, b) in w.buys.iter().enumerate() {
                let a = i as f32 * std::f32::consts::TAU / w.buys.len() as f32;
                let (s, c) = crate::math::det::sin_cos(a);
                let pos = *start + Vec3::new(c, s, 0.0) * w.ring;
                let pos = Vec3::new(pos.x, pos.y, host.ground_height(pos.x, pos.y));
                let yaw = crate::math::wrap_angle(a + std::f32::consts::PI);
                let object = host.spawn(&crate::names::wall_buy_object(&b.weapon), owner, pos, yaw);
                self.rounds.wall_buys.push(WallBuyAt { index: i as u8, pos, yaw, object });
            }
        }
        let stood = self.rounds.wall_buys.iter().filter(|b| b.object != 0).count();
        host.log(&format!("zombies: {} wall buys around each of {n} starts, {stood} props stood", w.buys.len()));
    }

    /// The wall buy in `p`'s use list now, if any (the HUD's prompt asks the same).
    pub fn wall_buy_target(&self, p: &PlayerSim) -> Option<usize> {
        if !p.survivor || self.is_downed(p.general) {
            return None;
        }
        let t = crate::tuning::get();
        let eye = p.body.eye();
        let dir = dir_from_angles(p.yaw, p.pitch);
        let mut best: Option<(usize, f32)> = None;
        for (i, b) in self.rounds.wall_buys.iter().enumerate() {
            let to = b.pos + Vec3::new(0.0, 0.0, crate::units::inches(t.dropped_item_half_size)) - eye;
            let dist = to.length();
            if dist / crate::units::inches(1.0) > t.player_use_radius {
                continue;
            }
            let dot = if dist > 0.0 { dir.dot(to) / dist } else { 1.0 };
            let score = dist / crate::units::inches(1.0) + t.use_list_dot_scale * (1.0 - (dot + 1.0) * 0.5);
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The use key on a wall buy: the purchase. True when the press was the wall buy's.
    pub(super) fn wall_buy_input(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, input: &mut FpsInput) -> bool {
        if input.pressed & buttons::FIELD_COMPUTER == 0 {
            return false;
        }
        let Some(i) = self.wall_buy_target(p) else { return false };
        input.pressed &= !buttons::FIELD_COMPUTER;
        let b = &WallBuys::get().buys[self.rounds.wall_buys[i].index as usize];
        let Some(def) = crate::weapons::weapon_or_none(&b.weapon) else { return true };
        let Some(player) = host.object(p.general, p.general).map(|o| o.owner_player) else { return true };
        let money = host.money(player).max(0) as u32;
        let (cost, bought) = match offer(p, b) {
            Offer::Weapon { cost } if money >= cost => {
                give(p, def);
                (cost, true)
            }
            Offer::Ammo { cost } if money >= cost => match held(p, b) {
                Some(w) if give_ammo(p, w) => (cost, true),
                _ => (0, false),
            },
            _ => (0, false),
        };
        if bought {
            host.add_cash(player, -(cost as i32));
            self.emit(idx, EventKind::ZombiePurchase { cost: cost.min(u16::MAX as u32) as u16 });
            host.log(&format!("zombies: General {} bought {} for {cost}", p.general, b.weapon));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buy_offers_the_weapon_then_its_ammo_and_a_melee_buy_only_once() {
        let mut sim = Sim::new();
        let mut p = sim.new_record(1, Mode::Fps, None);
        p.arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout { primary: "fnp45_mp".into(), held_knife: false, ..crate::weapons::Loadout::unarmed() });
        let gun = WallBuy { weapon: "an94_mp".into(), model: String::new(), cost: Some(1200), hint: None };
        assert_eq!(offer(&p, &gun), Offer::Weapon { cost: 1200 });
        give(&mut p, crate::weapons::weapon("an94_mp"));
        assert_eq!(p.arsenal.weapons.len(), 2, "a second gun beside the first");
        assert_eq!(p.arsenal.current().main.id, "an94_mp", "switched to");
        assert!(matches!(offer(&p, &gun), Offer::Ammo { .. }));
        let i = p.arsenal.weapons.iter().position(|w| w.main.id == "an94_mp").unwrap();
        // BO2 compares the stock with the whole start count: a fresh wall gun can take ammo (to
        // its max) at once, and then has nothing to buy.
        assert!(give_ammo(&mut p, i));
        assert_eq!(p.arsenal.weapons[i].reserve, p.arsenal.weapons[i].main.max_reserve);
        assert!(!give_ammo(&mut p, i), "full: nothing to sell");
        // A third gun takes the place of the one up.
        give(&mut p, crate::weapons::weapon("mp7_mp"));
        assert_eq!(p.arsenal.weapons.iter().map(|w| w.main.id.as_str()).collect::<Vec<_>>(), vec!["fnp45_mp", "mp7_mp"]);
        let knife = WallBuy { weapon: "knife_mp".into(), model: String::new(), cost: Some(3000), hint: None };
        if crate::weapons::weapon_or_none("knife_mp").is_some_and(is_melee) {
            give(&mut p, crate::weapons::weapon("knife_mp"));
            assert_eq!(offer(&p, &knife), Offer::Nothing);
        }
        sim.players[0] = Some(p);
    }
}
