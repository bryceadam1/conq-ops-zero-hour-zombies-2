/*
** Conquer Ops - Zero Hour gameplay core: the weapons: definitions, the converted table, the catalog, loadouts and the first-person state machine.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

pub mod arsenal;
pub mod catalog;
pub mod def;
pub mod feel;
pub mod gunpos;
pub mod loadout;
pub mod perks;
pub mod recoil;
pub mod rng;
pub mod specialty;
pub mod table;

pub use arsenal::*;
pub use catalog::*;
pub use def::*;
pub use feel::{GunKick, Reticle, Sway, ViewPos};
pub use loadout::*;
pub use perks::*;
pub use rng::*;
pub use specialty::*;
pub use table::*;

pub use crate::attachments::AttachmentDef;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::buttons;
    use crate::units::LOGIC_DT;

    fn def(f: impl FnOnce(&mut WeaponDef)) -> &'static WeaponDef {
        let mut d = WeaponDef { id: "test_mp".into(), ..Default::default() };
        f(&mut d);
        Box::leak(Box::new(d))
    }

    fn arsenal_with(primary: &'static WeaponDef) -> Arsenal {
        let mut a = Arsenal::new();
        a.weapons[0] = WeaponState::new(primary);
        a.raise_left = 0.0;
        a
    }

    fn frames(a: &mut Arsenal, held: u32, n: usize, rng: &mut Rng) -> Vec<WeaponAction> {
        let mut out = Vec::new();
        for _ in 0..n {
            a.step(held, 0, false, 0, 0.0, rng, &mut out);
        }
        out
    }

    fn shots(actions: &[WeaponAction]) -> usize {
        actions.iter().filter(|a| matches!(a, WeaponAction::Fire { pellet: 0, .. })).count()
    }

    /// An unknown weapon id is never silently substituted in a kit: the slot is left out and the id
    /// is recorded (and printed), so a class whose id does not resolve cannot pass for the stock
    /// AN-94 kit. `weapon()` (display-only) still falls back, but loudly.
    #[test]
    fn an_unknown_weapon_id_is_loud_and_never_becomes_the_first_gun() {
        catalog::clear_unresolved();
        assert!(catalog::weapon_or_none("definitely_not_a_weapon_mp").is_none());
        assert!(catalog::unresolved().iter().any(|s| s == "definitely_not_a_weapon_mp"), "{:?}", catalog::unresolved());
        // The display fallback keeps its old behaviour but is recorded.
        assert_eq!(weapon("definitely_not_a_weapon_mp").id, catalog().all()[0].id);
        // A kit with that primary is unarmed (the knife), not the catalog's first entry.
        let a = Arsenal::with_loadout(Loadout { primary: "definitely_not_a_weapon_mp".into(), ..Loadout::unarmed() });
        assert!(a.is_unarmed(), "the bad id did not borrow the AN-94");
        assert_eq!(a.weapons.len(), 1);
        assert_eq!(a.weapons[0].def.class, Class::Melee);
        catalog::clear_unresolved();
    }

    #[test]
    fn the_unarmed_kit_is_knife_only_and_the_base_kit_is_untouched() {
        let a = Arsenal::unarmed();
        assert!(a.is_unarmed());
        assert_eq!(a.weapons.len(), 1);
        assert_eq!(a.current().def.id, "knife_held_mp");
        assert_eq!((a.lethal.count, a.tactical.count), (0, 0));
        assert!(a.loadout.perks.is_empty());
        // `Loadout::default()` is still the classes' spec base and the converter's source.
        let base = Loadout::default();
        assert!(!base.primary.is_empty() && !base.secondary.is_empty());
        assert!(!Arsenal::new().is_unarmed(), "Arsenal::new stays the full base kit for the tests/converter");
    }

    #[test]
    fn table_round_trips() {
        let defs: Vec<WeaponDef> = catalog().all().iter().map(|d| (*d).clone()).collect();
        let again = parse_table(&format_table(&defs)).unwrap();
        assert_eq!(defs, again);
        assert!(catalog().get("an94_mp").is_some() && catalog().get("frag_grenade_mp").is_some());
    }

    #[test]
    fn auto_fire_matches_fire_time() {
        let d = def(|d| d.fire_time = 0.08); // 750 rpm
        let mut a = arsenal_with(d);
        let fired = shots(&frames(&mut a, buttons::FIRE, 60, &mut Rng::new(1))); // 2 s
        assert!((24..=26).contains(&fired), "fired {fired}");
    }

    #[test]
    fn magazine_reload_adds_rounds_at_the_add_time() {
        let d = def(|d| {
            d.clip = 30;
            d.start_reserve = 90;
            d.reload_empty_time = 2.0;
            d.reload_empty_add_time = 1.0;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(2);
        let out = frames(&mut a, buttons::FIRE, 120, &mut rng);
        assert_eq!(shots(&out), 30);
        assert!(out.contains(&WeaponAction::ReloadStarted), "auto reload when empty");
        // Rounds land at the add time, before the reload animation ends.
        let mut a2 = a.clone();
        frames(&mut a2, 0, 1, &mut rng);
        assert!(a.current().is_reloading());
        let mut t = 0;
        while a.current().ammo == 0 {
            frames(&mut a, 0, 1, &mut rng);
            t += 1;
        }
        assert!(a.current().is_reloading(), "still animating after the rounds went in");
        assert_eq!(a.current().ammo, 30);
        assert_eq!(a.current().reserve, 60);
        assert!(t < 40);
    }

    #[test]
    fn bursts_fire_n_per_press() {
        let d = def(|d| {
            d.fire = FireMode::Burst(3);
            d.fire_time = 0.06;
            d.burst_delay = 0.2;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(3);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..20 {
            a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(shots(&out), 3, "one burst while holding");
    }

    #[test]
    fn shotguns_fire_pellets_and_reload_shell_by_shell() {
        let d = def(|d| {
            d.class = Class::Shotgun;
            d.fire = FireMode::Semi;
            d.pellets = 8;
            d.clip = 8;
            d.start_reserve = 16;
            d.reload_time = 0.5;
            d.reload_start_time = 0.3;
            d.reload_end_time = 0.3;
            d.reload_ammo_add = 1;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(4);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        let pellets = out.iter().filter(|x| matches!(x, WeaponAction::Fire { .. })).count();
        assert_eq!(pellets, 8);
        // Reload one shell, then the trigger stops it after that shell.
        a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
        frames(&mut a, 0, 30, &mut rng); // start (0.3) + one shell (0.5)
        assert_eq!(a.current().ammo, 8, "one shell went in");
    }

    #[test]
    fn grenades_cook_and_throw() {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        let mut rng = Rng::new(5);
        let mut out = Vec::new();
        a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
        assert!(out.contains(&WeaponAction::OffhandReady { lethal: true }));
        // Hold (cook) for one second, then let go.
        for _ in 0..30 {
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        let throw = out.iter().find_map(|x| if let WeaponAction::Throw { fuse_left, .. } = x { Some(*fuse_left) } else { None });
        let fuse = a.lethal.def.fuse_time;
        let left = throw.expect("thrown");
        assert!((left - (fuse - 1.0)).abs() < 0.1, "cooked: {left} of {fuse}");
        assert_eq!(a.lethal.count, 0);
        // The weapon comes back after the throw.
        frames(&mut a, 0, 60, &mut rng);
        assert!(a.offhand.is_none() && !a.is_busy());
    }

    #[test]
    fn cooking_too_long_goes_off_in_hand() {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        let mut out = Vec::new();
        let mut rng = Rng::new(6);
        a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..200 {
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert!(out.iter().any(|x| matches!(x, WeaponAction::CookedOff { .. })));
    }

    #[test]
    fn sprinting_blocks_fire_and_ads_until_sprint_out() {
        let d = def(|d| d.sprint_out_time = 0.3);
        let mut a = arsenal_with(d);
        a.loadout.perks.clear();
        let mut rng = Rng::new(7);
        let mut out = Vec::new();
        for _ in 0..30 {
            a.step(buttons::FIRE | buttons::ADS, 0, true, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(shots(&out), 0);
        assert_eq!(a.ads, 0.0);
        // Right after the sprint: still coming up.
        a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(shots(&out), 0, "sprint-out time");
        frames(&mut a, buttons::FIRE, 12, &mut rng);
        let mut out = Vec::new();
        a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        assert!(shots(&frames(&mut a, buttons::FIRE, 5, &mut rng)) > 0);
    }

    #[test]
    fn swap_drops_then_raises_faster_with_fast_hands() {
        // G3: Fast Hands keeps the weapon's own drop/raise times and runs the countdown at
        // 1 / perk_weapSwitchMultiplier (the swap takes half the frames), not the quick times.
        let swap_frames = |a: &mut Arsenal| {
            let mut out = Vec::new();
            let mut rng = Rng::new(8);
            a.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
            let mut n = 1;
            while a.drop_left > 0.0 || a.raise_left > 0.0 {
                a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
                n += 1;
            }
            n
        };
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        a.loadout.perks.clear();
        // The secondary's first raise (it has not been up yet), after the primary's drop.
        let expect = ((a.weapons[0].def.drop_time + a.weapons[1].def.first_raise_time) / LOGIC_DT).ceil();
        let slow = swap_frames(&mut a);
        assert_eq!(a.active, 1);
        let mut b = Arsenal::new();
        b.raise_left = 0.0;
        assert!(b.loadout.perks.contains(&Perk::FAST_HANDS));
        let fast = swap_frames(&mut b);
        assert_eq!(b.raise_kind, RaiseKind::First, "the same animation, run faster");
        // Two frames of overhead either way: the press frame and the frame the drop ends on.
        assert!((slow as f32 - 2.0 - expect).abs() <= 1.0, "{slow} frames vs {expect}");
        assert!((fast as f32 - 2.0 - expect * 0.5).abs() <= 1.0, "fast hands {fast} frames vs {}", expect * 0.5);
    }

    #[test]
    fn damage_curve_and_explosion_falloff() {
        let d = weapon("an94_mp");
        assert_eq!(d.damage_at(10.0), 40.0);
        assert_eq!(d.damage_at(1000.0), 33.0);
        let g = weapon("frag_grenade_mp");
        assert!(g.explosion_damage_at(0.0) > g.explosion_damage_at(g.explosion_radius * 0.9));
        assert_eq!(g.explosion_damage_at(g.explosion_radius + 1.0), 0.0);
    }

    #[test]
    fn perks_change_movement() {
        let mut a = Arsenal::new();
        assert!(a.move_faster(), "Lightweight grants movefaster");
        a.loadout.perks.retain(|p| *p != Perk::LIGHTWEIGHT);
        assert!(!a.move_faster());
        let base = a.sprint_scale();
        a.loadout.perks.push(Perk::EXTREME_CONDITIONING);
        assert!((a.sprint_scale() - 2.0 * base).abs() < 1e-4, "longersprint doubles the sprint");
    }
}

#[cfg(test)]
mod loadout_tests {
    use super::*;

    #[test]
    fn a_spec_changes_slots_and_perks() {
        let l = Loadout::default().with_spec("lethal=claymore_mp; perks=Hardline,Engineer");
        assert_eq!(l.lethal, "claymore_mp");
        assert_eq!(l.perks, vec![Perk::HARDLINE, Perk::ENGINEER]);
        assert_eq!(l.primary, Loadout::default().primary);
    }
}

#[cfg(test)]
mod swap_tests {
    use super::*;
    use crate::input::buttons;

    #[test]
    fn swapping_mid_reload_before_the_add_time_keeps_the_magazine_empty() {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        a.loadout.perks.clear();
        let mut rng = Rng::new(9);
        let mut out = Vec::new();
        a.weapons[0].ammo = 1;
        // Fire the last round: the empty reload starts by itself.
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        assert!(a.current().is_reloading());
        for _ in 0..10 {
            a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        a.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..60 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        a.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..60 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(a.active, 0);
        assert_eq!(a.current().ammo, 0, "{:?}", a.current().reload);
    }
}

