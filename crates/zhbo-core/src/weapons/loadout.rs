/*
** Conquer Ops - Zero Hour gameplay core: the loadout: the player's kit spec and how it resolves into an Arsenal.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::catalog::{weapon, weapon_or_none};
use super::def::{Class, WeaponDef};
use super::perks::Perk;

/// The class melee weapon: BO2 gives every class `knife_mp` (`_class.gsc:586`, inventory type
/// melee), the weapon the melee button strikes with whatever gun is up. The held knife
/// (`knife_held_mp`, the loadout's `melee`) is a weapon in the hand, BO2's primary-inventory knife.
pub const CLASS_MELEE: &str = "knife_mp";

/// The class melee weapon's definition; the held knife when the table has none (an archive from
/// before it was converted).
pub fn class_melee(held_knife: &'static WeaponDef) -> &'static WeaponDef {
    weapon_or_none(CLASS_MELEE).unwrap_or(held_knife)
}

/// The weapon a melee uses with `held` up: a melee weapon or the Assault Shield strikes with itself,
/// anything else with the class melee weapon (`_scoreevents.gsc:375`: every `MOD_MELEE` kill but
/// the riot shield's is a knife stab; the conformance oracle's rule, `bo2/rows.rs`).
pub fn melee_weapon<'a>(held: &'a WeaponDef, class_melee: &'a WeaponDef) -> &'a WeaponDef {
    if held.class == Class::Melee || held.is_shield() {
        held
    } else {
        class_melee
    }
}

/// The held knife: BO2's `knife_held` (`mp/statstable.csv`, the loadout's `melee`), the knife every
/// class carries in the hand and the one its knife personalize page dresses (`caceditclass.lua`'s
/// `OpenKnifePersonalization`; `caccamomenu.lua` reads `knife_held`'s row). The melee button
/// strikes with [`CLASS_MELEE`].
pub const KNIFE_HELD: &str = "knife_held_mp";

/// What the General carries. Weapons are catalog ids, so they can later be picked up, swapped or
/// bought by changing the loadout.

#[derive(Clone, Debug, PartialEq)]
pub struct Loadout {
    pub primary: String,
    pub secondary: String,
    pub lethal: String,
    pub tactical: String,
    pub melee: String,
    pub perks: Vec<Perk>,
    /// Spare rounds the primary and secondary start with, instead of BO2's (tests run a gun dry).
    pub reserve: [Option<u16>; 2],
    /// How many of the lethal the class carries: BO2's loadout `primarygrenadecount`
    /// (`_class.gsc:718`, the clip `setweaponammoclip` gives at `:739`): one per filled box, so one
    /// lethal is one (a C4 too), two with Danger Close (`cacgrenadesbutton.lua:130-230`). `None` (a
    /// spec without the key: the shipped presets) gives the item's own start count.
    pub lethal_count: Option<u16>,
    /// How many of the tactical the class carries: BO2's loadout `specialgrenadecount`
    /// (`_class.gsc:724`, the clip `setweaponammoclip` gives), one or two (a second tactical costs
    /// a point, `caceditclass.lua:506-566`). `None` gives the item's own start count.
    pub tactical_count: Option<u16>,
    /// MP's `giveloadout` rule: a class with fewer than two guns also gets the held knife as a
    /// weapon (`_class.gsc`). Zombies gives none (`give_start_weapon` gives the start weapon
    /// alone, `_zm_utility.gsc:3524-3531`): a survivor with one gun holds just it.
    pub held_knife: bool,
}

crate::statehash::state_hash!(struct Loadout { primary, secondary, lethal, tactical, melee, perks, reserve, lethal_count, tactical_count, held_knife });

impl Loadout {
    /// The converted viewmodels this kit can show: its guns and their alt modes, its offhands, its
    /// held knife and the class melee weapon, each once.
    pub fn viewmodels(&self) -> Vec<i32> {
        let mut out = Vec::new();
        let class_melee = CLASS_MELEE.to_string();
        for id in [&self.primary, &self.secondary, &self.lethal, &self.tactical, &self.melee, &class_melee] {
            // An empty slot is legitimate (the unarmed kit); an unknown one resolves to nothing
            // rather than borrowing the catalog's first weapon's viewmodel.
            if id.is_empty() {
                continue;
            }
            let Some(d) = weapon_or_none(id) else { continue };
            let alt = (!d.alt.is_empty()).then(|| weapon(&d.alt));
            for vm in std::iter::once(d.viewmodel).chain(alt.map(|a| a.viewmodel)) { // content: presentation (viewmodels(): the client's preload)
                if vm >= 0 && !out.contains(&vm) {
                    out.push(vm);
                }
            }
        }
        out
    }

    /// The kit before a class is picked: no guns and no grenades, the melee knife, no perks (BO2
    /// always has melee). Deliberately not [`Default`] — that is the classes' spec base and the
    /// converter's source, and must stay the full base kit.
    pub fn unarmed() -> Loadout {
        Loadout {
            primary: String::new(),
            secondary: String::new(),
            lethal: String::new(),
            tactical: String::new(),
            melee: KNIFE_HELD.into(),
            perks: Vec::new(),
            reserve: [None, None],
            lethal_count: None,
            tactical_count: None,
            held_knife: true,
        }
    }
}

impl Loadout {
    /// A test fixture kit with attachments and an alt mode (BO2's premades carry neither): the
    /// AN-94 with a Reflex sight and a Grenade Launcher, the Tac-45, a frag, a concussion, the
    /// knife; Lightweight, Fast Hands and Dexterity.
    #[cfg(test)]
    pub fn test_kit() -> Loadout {
        Loadout::unarmed().with_spec(TEST_KIT)
    }
}

/// The General's base kit — before a class lands, and what a new custom class starts from
/// (`classmenu::Draft::default`) — as one spec, the one source of both: BO2's default class, the
/// Assault premade (`level.defaultclass = "CLASS_ASSAULT"`, `_class.gsc:36`; `reset_classes.cfg`'s
/// `equipdefaultclass 0 class_custom_assault`; [`crate::classes::BASIC`]`[0]`, from
/// `mp/statstable.csv`'s default-class column): the MTAR, the SMAW, a frag, a concussion, the knife;
/// Lightweight, Toughness, Dexterity.
pub const BASE_KIT: &str = crate::classes::BASIC[0].2;

/// The test fixture kit's spec ([`Loadout::test_kit`]).
#[cfg(test)]
pub const TEST_KIT: &str = "primary=an94_mp+gl+reflex;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity";

impl Default for Loadout {
    /// The base kit ([`BASE_KIT`]); `ZHBO_LOADOUT` changes it (see [`Loadout::with_spec`]).
    fn default() -> Self {
        let l = Loadout::unarmed().with_spec(BASE_KIT);
        // The loadout spec from the config registry (ZHBO_LOADOUT, read once at init).
        match crate::config::get().loadout.as_deref() {
            Some(spec) => l.with_spec(spec),
            None => l,
        }
    }
}

impl Loadout {
    /// Change slots from a spec like `lethal=claymore_mp;tactical=trophy_system_mp;primary=
    /// dsr50_mp+acog;perks=Hardline,Engineer` (`lethal_count=2`: Danger Close's second lethal; `tactical_count=2`: a second tactical). For trying other kits (and
    /// test runs): the converter reads the same variable so their art is converted, and every
    /// player in a game must use the same one.
    pub fn with_spec(mut self, spec: &str) -> Loadout {
        for part in spec.split(';') {
            let Some((key, value)) = part.split_once('=') else { continue };
            let value = value.trim().to_string();
            match key.trim() {
                "primary" => self.primary = value,
                "secondary" => self.secondary = value,
                "lethal" => self.lethal = value,
                "tactical" => self.tactical = value,
                "melee" => self.melee = value,
                "perks" => self.perks = value.split(',').filter_map(|p| Perk::by_id(p.trim())).collect(),
                "primary_reserve" => self.reserve[0] = value.parse().ok(),
                "secondary_reserve" => self.reserve[1] = value.parse().ok(),
                "lethal_count" => self.lethal_count = value.parse().ok(),
                "tactical_count" => self.tactical_count = value.parse().ok(),
                _ => {}
            }
        }
        self
    }
}