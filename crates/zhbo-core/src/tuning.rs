//! The gameplay tuning pack (refactor plan 3.1): the numbers BO2/bridge code used to scatter
//! across ~10 files, in one place. Loaded at init from `Data/ConquerOps/tuning.ini` (part of the
//! mod's BIG file, like weapons.ini — every player must have the identical file); the built-in
//! defaults below keep tests and the fallback working when the file is absent. This plan moves
//! numbers; it must not change them.

/*
** Conquer Ops - Zero Hour gameplay core: the gameplay tuning pack.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use std::collections::HashMap;
use std::sync::Mutex;

/// The gameplay tuning: movement, health regeneration, building rates, projectile physics and the
/// field computer's raise time. Defaults are the values the mod has always used.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    pub walk_speed: f32,
    pub sprint_time: f32,
    pub regen_delay: f32,
    pub regen_full_time: f32,
    pub building_rate: f32,
    pub projectile_bounce: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            walk_speed: crate::units::inches(190.0),
            sprint_time: 4.0,
            regen_delay: 5.0,
            regen_full_time: 3.0,
            building_rate: 1.0,
            projectile_bounce: 0.3,
        }
    }
}

/// The tuning loaded at init; the defaults until `load` runs.
static TUNING: Mutex<Option<Tuning>> = Mutex::new(None);

/// Load the tuning pack (the bridge passes `data/ConquerOps/tuning.ini`'s text at init). The
/// values must be identical on every client: the file is part of the mod.
pub fn load(text: &str) {
    let mut t = Tuning::default();
    let nums: HashMap<String, f32> = text
        .lines()
        .map(|l| l.split(';').next().unwrap_or("").trim())
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_ascii_lowercase().replace('_', ""), v.trim().parse().ok()?))
        })
        .collect();
    if let Some(v) = nums.get("walkspeed") { t.walk_speed = *v; }
    if let Some(v) = nums.get("sprinttime") { t.sprint_time = *v; }
    if let Some(v) = nums.get("regendelay") { t.regen_delay = *v; }
    if let Some(v) = nums.get("regenfulltime") { t.regen_full_time = *v; }
    if let Some(v) = nums.get("buildingrate") { t.building_rate = *v; }
    if let Some(v) = nums.get("projectilebounce") { t.projectile_bounce = *v; }
    *TUNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
}

/// The tuning in force; the built-in defaults before init (tests, the shell).
pub fn get() -> Tuning {
    TUNING.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_tuning_file() {
        load("; tuning.ini\nWalkSpeed = 380\nSprintTime = 3\nRegenDelay = 4 ");
        let t = get();
        assert_eq!(t.walk_speed, 380.0);
        assert_eq!(t.sprint_time, 3.0);
        assert_eq!(t.regen_delay, 4.0);
        // Unknown keys are tolerated; an absent file keeps the defaults.
        assert_eq!(t.projectile_bounce, 0.3);
    }

    #[test]
    fn defaults_hold_without_the_file() {
        let t = Tuning::default();
        assert_eq!(t.regen_delay, 5.0);
        assert_eq!(t.regen_full_time, 3.0);
    }
}
