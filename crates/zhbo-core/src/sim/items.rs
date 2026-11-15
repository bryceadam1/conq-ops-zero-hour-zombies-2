/*
** Conquer Ops - Zero Hour gameplay core: the weapons Zombies' scripts hand a survivor for a moment.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2 Zombies plays its drinks and hand animations as weapons the script gives and takes
//! (`zombies::script_weapons`), each one's raise being the animation:
//!
//! * **A perk's bottle** (`vending_trigger_post_think`): `perk_give_bottle_begin` switches to it,
//!   the perk comes on `weapon_change_complete` (the drink done), and `perk_give_bottle_end` takes
//!   it and brings the gun back. Going down meanwhile, no perk.
//! * **Pack-a-Punch's knuckle crack** (`do_knuckle_crack`): in the place of the weapon taken,
//!   played out before his next gun comes up.
//! * **The reviver's syrette** (`revive_trigger_think`): held while he revives, then his gun back
//!   (`revive_give_back_weapons`).
//! * **The builder** (`buildable_use_hold_think_internal`): held while he builds at a bench, then
//!   his gun back (`switch_back_primary_weapon`).
//!
//! Meanwhile he may not aim, sprint, go prone or melee (`disable_player_move_states`), nor fire,
//! throw, change weapon or use anything (`is_drinking`).

use super::*;

/// What the script weapon in hand is for.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// Perk `perk` (its row in `zombies::Perks`) comes when the drink is done.
    Drink { perk: u8 },
    KnuckleCrack,
    Syrette,
    /// A melee buy's flourish: `weapon` (an id) is his when it is done.
    Flourish { weapon: String },
    /// The builder at a buildable's bench, held while he builds (`sim::buildables`).
    Build,
}

crate::statehash::state_hash!(enum ItemKind { Drink { perk }, KnuckleCrack, Syrette, Flourish { weapon }, Build });

/// A script weapon in hand: what for, its id and the gun to bring back (by id).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemUse {
    pub kind: ItemKind,
    pub item: String,
    pub back: String,
}

crate::statehash::state_hash!(struct ItemUse { kind, item, back });

/// What a script weapon blocks (`disable_player_move_states`, `is_drinking`): all but walking,
/// crouching and jumping.
const ITEM_BLOCKED: u32 = buttons::FIRE
    | buttons::ADS
    | buttons::SPRINT
    | buttons::PRONE
    | buttons::RELOAD
    | buttons::SWAP_WEAPON
    | buttons::MELEE
    | buttons::LETHAL
    | buttons::TACTICAL
    | buttons::ALT_MODE
    | buttons::ACTION_SLOT_1
    | buttons::ACTION_SLOT_2
    | buttons::ACTION_SLOT_4;

impl Sim {
    /// Hand `p` the script weapon `item` for `kind` (in the place of the weapon up with `replace`).
    /// False when the weapon table lacks it (the caller then goes straight on).
    pub(super) fn start_item(p: &mut PlayerSim, kind: ItemKind, item: &str, replace: bool) -> bool {
        let Some(def) = crate::weapons::weapon_or_none(item) else { return false };
        let back = p.arsenal.current().main.id.clone();
        p.arsenal.raise_item(def, replace);
        if p.body.stance == Stance::Prone {
            // `disable_player_move_states( 1 )`: prone goes to crouch.
            p.body.stance = Stance::Crouch;
        }
        p.zm_item = Some(ItemUse { kind, item: item.to_string(), back });
        true
    }

    /// Take the script weapon in `p`'s hand back and bring his gun up.
    pub(super) fn end_item(p: &mut PlayerSim) {
        let Some(use_) = p.zm_item.take() else { return };
        if let Some(def) = crate::weapons::weapon_or_none(&use_.item) {
            p.arsenal.drop_item(def, &use_.back);
        }
    }

    /// The script weapon's frame (first person, before the input acts): a drink or knuckle crack
    /// done ends, a drink's perk coming; and while one is in hand, the input it blocks.
    pub(super) fn step_item(&mut self, host: &dyn LogicHost, p: &mut PlayerSim, input: &mut FpsInput) {
        let Some(use_) = p.zm_item.clone() else { return };
        let raised = crate::weapons::weapon_or_none(&use_.item).is_none_or(|def| p.arsenal.item_raised(def));
        match use_.kind {
            ItemKind::Drink { perk } if raised => {
                Self::end_item(p);
                self.grant_perk(host, p, perk);
            }
            ItemKind::KnuckleCrack if raised => Self::end_item(p),
            ItemKind::Flourish { weapon } if raised => {
                Self::end_item(p);
                if let Some(def) = crate::weapons::weapon_or_none(&weapon) {
                    super::wallbuy::give(p, def);
                }
            }
            _ => {}
        }
        if p.zm_item.as_ref().is_some_and(|u| u.kind != ItemKind::Syrette && u.kind != ItemKind::Build) {
            input.held &= !(ITEM_BLOCKED | buttons::FIELD_COMPUTER);
            input.pressed &= !(ITEM_BLOCKED | buttons::FIELD_COMPUTER);
        } else if p.zm_item.is_some() {
            input.held &= !ITEM_BLOCKED;
            input.pressed &= !ITEM_BLOCKED;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_weapon_raises_and_gives_the_gun_back() {
        let mut sim = Sim::new();
        let mut p = sim.new_record(1, Mode::Fps, None);
        p.arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout { primary: "an94_mp".into(), secondary: "fnp45_mp".into(), held_knife: false, ..crate::weapons::Loadout::unarmed() });
        p.arsenal.raise_left = 0.0;
        // A stand-in for a bottle: any weapon of the table.
        assert!(Sim::start_item(&mut p, ItemKind::KnuckleCrack, "mp7_mp", false));
        assert_eq!(p.arsenal.current().main.id, "mp7_mp");
        let def = crate::weapons::weapon("mp7_mp");
        assert!(!p.arsenal.item_raised(def), "the gun goes down, then the item comes up");
        let mut rng = crate::weapons::Rng::new(1);
        let mut out = Vec::new();
        for _ in 0..200 {
            p.arsenal.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert!(p.arsenal.item_raised(def));
        let host = crate::sim::fakehost::FakeHost::default();
        let mut input = FpsInput { pressed: buttons::FIRE, ..Default::default() };
        sim.step_item(&host, &mut p, &mut input);
        assert!(p.zm_item.is_none(), "done: taken back");
        assert_eq!(p.arsenal.current().main.id, "an94_mp", "his gun back up");
        assert_eq!(p.arsenal.weapons.len(), 2);
        sim.players[0] = Some(p);
    }

    #[test]
    fn while_drinking_he_cannot_fire_aim_sprint_or_use() {
        let sim = Sim::new();
        let mut p = sim.new_record(1, Mode::Fps, None);
        p.zm_item = Some(ItemUse { kind: ItemKind::Drink { perk: 0 }, item: "zombie_perk_bottle_jugg".into(), back: String::new() });
        let mut sim = sim;
        let host = crate::sim::fakehost::FakeHost::default();
        let mut input = FpsInput { held: buttons::FIRE | buttons::ADS | buttons::SPRINT | buttons::CROUCH, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
        // The bottle is not in the built-in table: it counts as drunk at once.
        sim.step_item(&host, &mut p, &mut input);
        assert!(p.zm_item.is_none());
        p.zm_item = Some(ItemUse { kind: ItemKind::Syrette, item: "syrette_zm".into(), back: String::new() });
        let mut input = FpsInput { held: buttons::FIRE | buttons::FIELD_COMPUTER | buttons::CROUCH, ..Default::default() };
        sim.step_item(&host, &mut p, &mut input);
        assert_eq!(input.held, buttons::FIELD_COMPUTER | buttons::CROUCH, "the syrette keeps the use key held, nothing else");
    }
}
