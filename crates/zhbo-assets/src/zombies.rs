//! BO2 Zombies for Conquer Ops: TranZit's weapons (the only Zombies map of a base install) and
//! Zombies' rule table, from the player's own install, beside the multiplayer weapons.
//!
//! Which weapons is TranZit's roster (`zhbo_core::zombies`, transcribed from `zm_transit.gsc`); what
//! each one is comes from its definition in TranZit's pool (`zhbo_formats::weapon::ZM_TRANSIT_POOL`),
//! turned into the core's [`WeaponDef`] by the same [`crate::weapons::to_def`] as every MP weapon.
//! The art passes treat them as the rest of the converted base set: they come after the MP
//! weapons in the converted list, so each keeps the MP viewmodel numbering intact.

use std::collections::HashMap;
use std::path::Path;

use zhbo_core::weapons::WeaponDef;
use zhbo_formats::weapon::Weapon;

use crate::paks::ZmPool;

/// Where the converter writes Zombies' rule table in the archive (`zhbo_core::zombies::RuleTable`).
pub const RULES_PATH: &str = "Data\\ConquerOps\\zombiemode.csv";

/// TranZit's pool and what the converter takes from it.
pub(crate) struct Zombies {
    pub(crate) pool: ZmPool,
    /// Every weapon of the pool (the art passes look a weapon up by name here).
    pub(crate) weapons: Vec<Weapon>,
    /// The converted weapons: the roster's, then the alt modes and spawned grenades they reach, in
    /// that order, each once.
    pub(crate) ids: Vec<String>,
}

/// BO2's localized strings for the Zombies display names: every Zombies language zone TranZit's
/// classic mode loads, later ones over earlier (the patches last).
pub(crate) fn names(bo2_dir: &Path) -> HashMap<String, String> {
    zhbo_formats::localize::install_strings(bo2_dir, &["code_post_gfx_zm", "common_zm", "ui_zm", "zm_transit", "so_zclassic_zm_transit", "patch_zm", "patch_ui_zm"])
}

impl Zombies {
    /// TranZit's pool from the install, with the roster's weapons and what they reach. A roster
    /// weapon the pool lacks is left out (and named in the summary by [`Zombies::missing`]).
    pub(crate) fn load(bo2_dir: &Path) -> Result<Zombies, String> {
        let pool = ZmPool::load(bo2_dir)?;
        // TranZit's attachments are its own (no Create-a-Class): its weapons carry theirs built in.
        let (weapons, _) = pool.weapons()?;
        let mut ids: Vec<String> = zhbo_core::zombies::roster().into_iter().map(|w| w.id).filter(|id| weapons.iter().any(|w| &w.name == id)).collect();
        // The alt modes their attachments add (the Pack-a-Punched M16's launcher) and the grenades
        // their projectiles become, as the MP table reaches them (`weapons::table_with_metadata`).
        let mut k = 0;
        while k < ids.len() {
            if let Some(w) = weapons.iter().find(|w| w.name == ids[k]) {
                let d = crate::weapons::to_def(w, &HashMap::new());
                let own_alt = (!d.alt.is_empty()).then(|| d.alt.clone());
                for extra in d.alt_modes.iter().map(|(_, alt)| alt.clone()).chain(own_alt).chain((!d.spawns.is_empty()).then(|| d.spawns.clone())) {
                    if !ids.contains(&extra) && weapons.iter().any(|w| w.name == extra) {
                        ids.push(extra);
                    }
                }
            }
            k += 1;
        }
        Ok(Zombies { pool, weapons, ids })
    }

    /// The converted weapons whose art the install cannot supply (`has_art`: the view model is
    /// found and its textures convert, as the MP items are locked without it).
    pub(crate) fn lacking_art(&self, has_art: impl Fn(&Weapon) -> bool) -> Vec<String> {
        self.ids.iter().filter(|id| self.weapons.iter().find(|w| &w.name == *id).is_some_and(|w| w.view_model.is_some() && !has_art(w))).cloned().collect()
    }

    /// Leave out the weapons [`Zombies::lacking_art`] found, each with what only it brought along
    /// (an upgrade's alt mode). Returns the weapons left out.
    pub(crate) fn leave_out(&mut self, lacking: Vec<String>) -> Vec<String> {
        // An alt mode or spawned grenade only reached through a weapon left out goes too.
        let reached_only_by = |id: &str| self.weapons.iter().filter(|w| lacking.contains(&w.name)).any(|w| w.alt_weapon.as_deref() == Some(id) || w.spawned_grenade_weapon.as_deref() == Some(id));
        let mut dropped: Vec<String> = lacking.clone();
        dropped.extend(self.ids.iter().filter(|id| !lacking.contains(id) && reached_only_by(id) && !zhbo_core::zombies::roster().iter().any(|w| &w.id == *id)).cloned());
        self.ids.retain(|id| !dropped.contains(id));
        dropped
    }

    /// The roster's weapons the install's pool does not define.
    pub(crate) fn missing(&self) -> Vec<String> {
        zhbo_core::zombies::roster().into_iter().map(|w| w.id).filter(|id| !self.weapons.iter().any(|w| &w.name == id)).collect()
    }

    /// The core's definitions of the converted weapons, named from `names` (the Zombies strings
    /// over the multiplayer ones).
    pub(crate) fn defs(&self, names: &HashMap<String, String>) -> Vec<WeaponDef> {
        self.ids.iter().filter_map(|id| self.weapons.iter().find(|w| &w.name == id)).map(|w| crate::weapons::to_def(w, names)).collect()
    }

    /// Zombies' rule table as the install has it (`mp/zombiemode.csv`, `common_zm`), back as CSV.
    pub(crate) fn rule_table(bo2_dir: &Path) -> Result<String, String> {
        let zone = crate::paks::load_zone(bo2_dir, "common_zm")?;
        let rows = zhbo_formats::stringtable::rows(&zone, "mp/zombiemode.csv").map_err(|e| e.to_string())?.ok_or("common_zm has no mp/zombiemode.csv")?;
        Ok(rows.iter().map(|r| r.join(",")).collect::<Vec<_>>().join("\n") + "\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Over the player's install: every roster weapon converts to a definition with BO2's display
    /// name, the upgrades and their alt modes are reached, and the rule table prices the walls as
    /// `add_zombie_weapon` does.
    #[test]
    #[ignore]
    fn tranzits_roster_converts_from_the_install() {
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        let zm = Zombies::load(&bo2).unwrap();
        assert!(zm.missing().is_empty(), "the pool lacks {:?}", zm.missing());
        let mut names = crate::weapons::names(&bo2);
        names.extend(super::names(&bo2));
        let defs = zm.defs(&names);
        assert_eq!(defs.len(), zm.ids.len());
        // Every display name the install has text for is that text (the monkey's and the EMP's keys
        // have none in any language zone, and the knife's def names nothing: those stay as BO2 has
        // them).
        let lost: Vec<&str> = defs
            .iter()
            .filter(|d| zm.weapons.iter().find(|w| w.name == d.id).is_some_and(|w| names.get(&w.display_name).is_some_and(|t| t.trim() != d.display_name)))
            .map(|d| d.id.as_str())
            .collect();
        assert!(lost.is_empty(), "lost display names: {lost:?}");
        eprintln!("converted: {}", zm.ids.join(" "));
        assert!(zm.ids.iter().any(|id| id == "gl_m16_upgraded_zm"), "the upgraded M16's launcher");
        let rules = zhbo_core::zombies::RuleTable::parse(&Zombies::rule_table(&bo2).unwrap());
        let roster = zhbo_core::zombies::roster();
        let price = |id: &str| roster.iter().find(|w| w.id == id).unwrap().cost(&rules);
        eprintln!("walls: python {:?} m16 {:?} galil {:?} ray gun {:?}", price("python_zm"), price("m16_zm"), price("galil_zm"), price("ray_gun_zm"));
        assert_eq!(rules.int("zombie_health_start", 2), Some(150));
        for d in &defs {
            eprintln!("{} \"{}\" clip {} reserve {}/{}", d.id, d.display_name, d.clip, d.start_reserve, d.max_reserve);
        }
    }
}

