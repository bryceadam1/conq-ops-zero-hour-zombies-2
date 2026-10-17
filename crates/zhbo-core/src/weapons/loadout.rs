/*
** Conquer Ops - Zero Hour gameplay core: the loadout: the player's kit spec and how it resolves into an Arsenal.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::catalog::weapon;
use super::perks::Perk;
use crate::config;
use std::collections::HashMap;


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
}

impl Loadout {
    /// The converted viewmodels this kit can show: its guns and their alt modes, its offhands and
    /// its melee weapon, each once.
    pub fn viewmodels(&self) -> Vec<i32> {
        let mut out = Vec::new();
        for id in [&self.primary, &self.secondary, &self.lethal, &self.tactical, &self.melee] {
            let d = weapon(id);
            let alt = (!d.alt.is_empty()).then(|| weapon(&d.alt));
            for vm in std::iter::once(d.viewmodel).chain(alt.map(|a| a.viewmodel)) {
                if vm >= 0 && !out.contains(&vm) {
                    out.push(vm);
                }
            }
        }
        out
    }
}

impl Default for Loadout {
    /// The General's kit: the AN-94 with a Reflex sight and a Grenade Launcher, the Tac-45, a frag,
    /// a concussion, the knife; Lightweight, Fast Hands and Dexterity. `ZHBO_LOADOUT` changes it (see [`Loadout::with_spec`]).
    fn default() -> Self {
        let l = Loadout {
            primary: "an94_mp+gl+reflex".into(),
            secondary: "fnp45_mp".into(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: vec![Perk::LIGHTWEIGHT, Perk::FAST_HANDS, Perk::DEXTERITY],
            reserve: [None, None],
        };
        // The loadout spec from the config registry (ZHBO_LOADOUT, read once at init).
        match crate::config::get().loadout.as_deref() {
            Some(spec) => l.with_spec(spec),
            None => l,
        }
    }
}

impl Loadout {
    /// Change slots from a spec like `lethal=claymore_mp;tactical=trophy_system_mp;primary=
    /// dsr50_mp+acog;perks=Hardline,Engineer`. For trying other kits (and
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
                _ => {}
            }
        }
        self
    }
}