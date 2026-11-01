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
pub mod handling;
pub mod idle_sway;
pub mod loadout;
pub mod perks;
pub mod recoil;
pub mod rng;
pub mod scope;
pub mod specialty;
pub mod table;
pub mod view_kick;
pub mod zoom;

pub use arsenal::*;
pub use catalog::*;
pub use def::*;
pub use handling::{AntiQuickScope, CycleTimes, OffhandClass, Penetrate, Traits, WeapType, Zoom};
pub use feel::{optic_of, reticle_art, GunKick, Reticle, Sway, ViewPos};
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

    /// Frames until the trigger fires again, holding it from a fresh shot.
    fn frames_between_shots(a: &mut Arsenal, rng: &mut Rng) -> Vec<usize> {
        let mut at = Vec::new();
        for f in 0..(4.0 / LOGIC_DT) as usize {
            let out = frames(a, buttons::FIRE, 1, rng);
            if shots(&out) > 0 {
                at.push(f);
            }
        }
        at.windows(2).map(|w| w[1] - w[0]).collect()
    }

    /// A bolt action rechambers after every shot: the next round waits the fire time and then
    /// `iRechamberTime` (`FUN_008dc260` sets the flag, `FUN_008db0d0` runs the rechamber).
    #[test]
    fn a_bolt_action_rechambers_between_shots() {
        let d = def(|d| {
            d.fire_time = 0.2;
            d.timing.bolt_action = true;
            d.timing.rechamber_time = 1.0;
        });
        let mut a = arsenal_with(d);
        let gaps = frames_between_shots(&mut a, &mut Rng::new(3));
        let want = (1.2 / LOGIC_DT).round() as usize;
        assert!(!gaps.is_empty() && gaps.iter().all(|g| g.abs_diff(want) <= 1), "gaps {gaps:?}, want {want}");
        // Without the flag the same times fire at the fire time.
        let plain = def(|d| {
            d.fire_time = 0.2;
            d.timing.rechamber_time = 1.0;
        });
        let mut a = arsenal_with(plain);
        let gaps = frames_between_shots(&mut a, &mut Rng::new(3));
        assert!(gaps.iter().all(|g| g.abs_diff((0.2 / LOGIC_DT).round() as usize) <= 1), "gaps {gaps:?}");
    }

    /// `iShotsBeforeRechamber`: the SRM's four rounds, then a rechamber.
    #[test]
    fn shots_before_rechamber_counts_rounds() {
        let d = def(|d| {
            d.fire_time = 0.128;
            d.timing.bolt_action = true;
            d.timing.shots_before_rechamber = 4;
            d.timing.rechamber_time = 0.55;
        });
        let mut a = arsenal_with(d);
        let gaps = frames_between_shots(&mut a, &mut Rng::new(4));
        let short = (0.128 / LOGIC_DT).round() as usize;
        let long = ((0.128 + 0.55) / LOGIC_DT).round() as usize;
        assert!(gaps.len() >= 4, "{gaps:?}");
        for (i, g) in gaps.iter().take(8).enumerate() {
            let want = if i % 4 == 3 { long } else { short };
            assert!(g.abs_diff(want) <= 1, "gap {i} = {g}, want {want}: {gaps:?}");
        }
    }

    /// Starting a reload drops the owed rechamber (`FUN_008e1510`).
    #[test]
    fn a_reload_drops_the_owed_rechamber() {
        let d = def(|d| {
            d.fire_time = 0.2;
            d.timing.bolt_action = true;
            d.timing.rechamber_time = 1.0;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(5);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        assert!(a.current().rechamber_pending);
        a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
        assert!(a.current().is_reloading());
        assert!(!a.current().rechamber_pending && a.current().rechamber.is_none());
    }

    /// The magazine's last round fires over `iLastFireTime` (the pistols' slide lock, the pump's
    /// last shell): the empty reload starts when it ends.
    #[test]
    fn the_last_round_uses_the_last_fire_time() {
        let d = def(|d| {
            d.fire_time = 0.1;
            d.clip = 1;
            d.timing.last_fire_time = 0.5;
        });
        let mut a = arsenal_with(d);
        a.weapons[0].ammo = 1;
        let mut rng = Rng::new(6);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        let mut n: usize = 0;
        while !a.current().is_reloading() && n < 100 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
            n += 1;
        }
        assert!(n.abs_diff((0.5 / LOGIC_DT).round() as usize) <= 1, "the reload waits out the last-fire time: {n} frames");
    }

    /// The minigun barrels spin down over `iSpinDownTime`, not at the spin-up rate.
    #[test]
    fn the_minigun_spins_down_over_its_spin_down_time() {
        let d = def(|d| {
            d.fire = FireMode::Minigun;
            d.spin_up_time = 1.0;
            d.timing.spin_down_time = 2.0;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(7);
        frames(&mut a, buttons::FIRE, (1.0 / LOGIC_DT) as usize + 1, &mut rng);
        assert_eq!(a.current().spin, 1.0);
        frames(&mut a, 0, (1.0 / LOGIC_DT).round() as usize, &mut rng);
        assert!((a.current().spin - 0.5).abs() < 0.02, "half spun down after 1 s of 2: {}", a.current().spin);
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

    /// BO2 adds an empty reload's rounds at `iReloadEmptyAddTime`, or at the reload add time when
    /// that is 0 (most rifles; `FUN_008dec90`).
    #[test]
    fn an_empty_reload_adds_at_its_own_add_time_else_the_reload_add_time() {
        let add_frames = |empty_add: f32| {
            let d = def(|d| {
                d.reload_empty_time = 2.0;
                d.reload_add_time = 0.8;
                d.reload_empty_add_time = empty_add;
            });
            let mut a = arsenal_with(d);
            a.weapons[0].ammo = 0;
            let mut rng = Rng::new(2);
            let mut out = Vec::new();
            a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
            assert!(a.current().is_reloading());
            let mut n = 0;
            while a.current().ammo == 0 {
                frames(&mut a, 0, 1, &mut rng);
                n += 1;
            }
            assert!(a.current().is_reloading(), "the empty reload is still playing");
            n as f32 * LOGIC_DT
        };
        assert!((add_frames(0.0) - 0.8).abs() <= 2.0 * LOGIC_DT, "0: the reload add time");
        assert!((add_frames(1.5) - 1.5).abs() <= 2.0 * LOGIC_DT, "its own add time");
    }

    /// Only a bullet weapon plays the empty reload; a launcher at clip 0 reloads over its plain
    /// reload time (`FUN_008dec90`: clip 0 and `weapType` bullet).
    #[test]
    fn only_bullet_weapons_play_the_empty_reload() {
        let length = |weap_type: WeapType| {
            let d = def(|d| {
                d.clip = 1;
                d.start_reserve = 3;
                d.reload_time = 1.0;
                d.reload_empty_time = 2.0;
                d.traits.weap_type = weap_type;
            });
            let mut a = arsenal_with(d);
            a.weapons[0].ammo = 0;
            let mut rng = Rng::new(2);
            let mut out = Vec::new();
            a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
            match a.current().reload {
                Some(Reload::Magazine { length, empty, .. }) => (length, empty),
                r => panic!("{r:?}"),
            }
        };
        assert_eq!(length(WeapType::Bullet), (2.0, true));
        assert_eq!(length(WeapType::Projectile), (1.0, false));
    }

    /// The reload is segmented by BO2's flag, not by its times: without `bSegmentedReload` a
    /// weapon that authors start/end times and rounds per cycle reloads its whole magazine.
    #[test]
    fn segmented_reloads_follow_the_flag() {
        let reload = |segmented: bool| {
            let d = def(|d| {
                d.clip = 4;
                d.start_reserve = 8;
                d.reload_start_time = 0.3;
                d.reload_end_time = 0.3;
                d.reload_ammo_add = 1;
                d.traits.segmented_reload = segmented;
            });
            let mut a = arsenal_with(d);
            a.weapons[0].ammo = 2;
            let mut rng = Rng::new(2);
            let mut out = Vec::new();
            a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
            a.current().reload
        };
        assert!(matches!(reload(true), Some(Reload::Shells { .. })));
        assert!(matches!(reload(false), Some(Reload::Magazine { .. })));
    }

    /// A swap puts an empty-clipped weapon away over its `iEmptyDropTime` and raises an empty one
    /// over its `iEmptyRaiseTime`, even on its first raise (`FUN_008db500`, `FUN_008df320`).
    #[test]
    fn an_empty_gun_drops_and_raises_over_its_empty_times() {
        let p = def(|d| {
            d.drop_time = 0.4;
            d.empty_drop_time = 0.9;
        });
        let s = def(|d| {
            d.first_raise_time = 0.7;
            d.raise_time = 0.5;
            d.empty_raise_time = 1.3;
        });
        let mut a = arsenal_with(p);
        a.loadout.perks.clear();
        a.weapons[1] = WeaponState::new(s);
        a.weapons[0].ammo = 0;
        a.weapons[1].ammo = 0;
        let mut rng = Rng::new(8);
        let mut out = Vec::new();
        a.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(a.active, 1);
        assert_eq!((a.drop_total, a.raise_total, a.raise_kind), (0.9, 1.3, RaiseKind::Empty));
        let mut n = 1;
        while a.drop_left > 0.0 || a.raise_left > 0.0 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
            n += 1;
        }
        let expect = ((0.9 + 1.3) / LOGIC_DT).ceil();
        assert!((n as f32 - 2.0 - expect).abs() <= 1.0, "{n} frames vs {expect}");
        // Loaded, the same pair swaps over the plain times (the first raise is spent).
        let mut b = arsenal_with(p);
        b.loadout.perks.clear();
        b.weapons[1] = WeaponState::new(s);
        b.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!((b.drop_total, b.raise_total, b.raise_kind), (0.4, 0.7, RaiseKind::First));
    }

    /// `unlimitedAmmo` keeps an empty clip from reading as empty: the plain drop and raise play.
    #[test]
    fn unlimited_ammo_never_takes_the_empty_raise_or_drop() {
        let unlimited = |d: &mut WeaponDef| {
            d.drop_time = 0.4;
            d.empty_drop_time = 0.9;
            d.first_raise_time = 0.7;
            d.empty_raise_time = 1.3;
            d.traits.unlimited_ammo = true;
        };
        let (p, s) = (def(unlimited), def(unlimited));
        let mut a = arsenal_with(p);
        a.loadout.perks.clear();
        a.weapons[1] = WeaponState::new(s);
        a.weapons[0].ammo = 0;
        a.weapons[1].ammo = 0;
        let mut out = Vec::new();
        a.step(0, buttons::SWAP_WEAPON, false, 0, 0.0, &mut Rng::new(8), &mut out);
        assert_eq!((a.drop_total, a.raise_total, a.raise_kind), (0.4, 0.7, RaiseKind::First));
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

    /// `Bullet_Fire`'s range rule: a spread class (by BO2's `weapClass`, not the mod's class)
    /// traces to its curve's last range; anything else to `bulletrange`.
    #[test]
    fn the_bullet_range_follows_bo2s_weap_class() {
        let shotgun = def(|d| {
            d.weap_class = WeapClass::Spread;
            d.damage_range = [10.0, 0.0, 0.0, 20.0, 21.0, 30.0];
        });
        assert_eq!(shotgun.bullet_range(500.0), crate::units::inches(30.0));
        let judge = def(|d| {
            d.class = Class::Pistol;
            d.weap_class = WeapClass::PistolSpread;
            d.damage_range = [10.0, 11.0, 0.0, 0.0, 0.0, 25.0];
        });
        assert_eq!(judge.bullet_range(500.0), crate::units::inches(25.0));
        let rifle = def(|d| d.damage_range = [10.0, 11.0, 0.0, 0.0, 0.0, 25.0]);
        assert_eq!(rifle.bullet_range(500.0), 500.0);
    }

    /// BO2's `shotCount` only counts for a spread class: a rifle authored with 8 fires one.
    #[test]
    fn only_a_spread_class_fires_its_shot_count() {
        let d = def(|d| {
            d.fire = FireMode::Semi;
            d.pellets = 8;
        });
        let mut a = arsenal_with(d);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut Rng::new(4), &mut out);
        assert_eq!(out.iter().filter(|x| matches!(x, WeaponAction::Fire { .. })).count(), 1);
    }

    /// The tri-bolt (`stacked`): one pull launches the whole three-bolt magazine, numbered by the
    /// rounds loaded before each bolt; a part-loaded stack fires what it holds.
    #[test]
    fn a_stack_fires_its_full_magazine_at_once() {
        let d = def(|d| {
            d.class = Class::Special;
            d.fire = FireMode::Stacked;
            d.projectile_speed = 100.0;
            d.effect = Effect::Explode;
            d.pellets = 0;
            d.clip = 3;
            d.start_reserve = 3;
        });
        let mut a = arsenal_with(d);
        let mut rng = Rng::new(4);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        let bolts: Vec<u16> = out.iter().filter_map(|x| if let WeaponAction::Launch { pellet, .. } = x { Some(*pellet) } else { None }).collect();
        assert_eq!(bolts, [2, 1, 0], "{out:?}");
        assert_eq!(a.current().ammo, 0);
        let mut b = arsenal_with(d);
        b.weapons[0].ammo = 2;
        b.weapons[0].reserve = 0;
        let mut out = Vec::new();
        b.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(out.iter().filter(|x| matches!(x, WeaponAction::Launch { .. })).count(), 2, "a part-loaded stack fires its two");
    }

    #[test]
    fn shotguns_fire_pellets_and_reload_shell_by_shell() {
        let d = def(|d| {
            d.class = Class::Shotgun;
            d.weap_class = WeapClass::Spread;
            d.fire = FireMode::Semi;
            d.pellets = 8;
            d.clip = 8;
            d.start_reserve = 16;
            d.reload_time = 0.5;
            d.reload_start_time = 0.3;
            d.reload_end_time = 0.3;
            d.reload_ammo_add = 1;
            d.traits.segmented_reload = true;
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

    /// Frames from an alt-mode press until the toggle's drop and raise are both done, and whether
    /// the mode had already changed on the press frame.
    fn alt_toggle_frames(a: &mut Arsenal) -> (usize, bool) {
        let mut out = Vec::new();
        let mut rng = Rng::new(3);
        let before = a.current().def.id.clone();
        a.step(0, buttons::ALT_MODE, false, 0, 0.0, &mut rng, &mut out);
        let swapped_now = a.current().def.id != before;
        let mut n = 1;
        while a.drop_left > 0.0 || a.raise_left > 0.0 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
            n += 1;
        }
        (n, swapped_now)
    }

    #[test]
    fn a_launcher_toggle_raises_the_incoming_mode_over_its_alt_raise_time() {
        // G16 / FUN_008df320: the alt raise is the incoming variant's iAltRaiseTime; the launcher
        // pair's alt drop is 0, so the mode changes on the press frame (FUN_008e4da0).
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        a.loadout.perks.clear();
        let gl = weapon("gl_an94_mp");
        let (ammo, gl_ammo) = (a.current().ammo, a.current().alt_ammo);
        let (n, now) = alt_toggle_frames(&mut a);
        assert!(now, "a zero alt drop changes the mode at once");
        assert_eq!(a.current().def.id, "gl_an94_mp");
        assert_eq!((a.current().ammo, a.current().alt_ammo), (gl_ammo, ammo), "the launcher trades magazines");
        let expect = (gl.alt_raise_time / LOGIC_DT).ceil() as usize;
        assert!(n.abs_diff(expect + 1) <= 1, "{n} frames vs {expect}");
        // And back: the weapon's own alt raise.
        let (n, now) = alt_toggle_frames(&mut a);
        assert!(now);
        assert!(a.current().def.id.starts_with("an94_mp"));
        assert_eq!(a.raise_kind, RaiseKind::Alt);
        let expect = (a.current().def.alt_raise_time / LOGIC_DT).ceil() as usize;
        assert!(n.abs_diff(expect + 1) <= 1, "{n} frames vs {expect}");
    }

    #[test]
    fn an_alt_toggle_drops_the_outgoing_weapon_first_and_fast_hands_speeds_both() {
        // FUN_008db500: an alt pair's change starts with the OUTGOING weapon's alt drop
        // (FUN_008ed1e0 — select fire's unique gives `an94_mp+sf` 500 ms), the mode changes when it
        // ends, then the incoming mode raises over its own iAltRaiseTime (0 for `sf_*`). Both are
        // raise/drop-family states, so Fast Hands halves them (G3).
        let sf_like = || -> &'static WeaponDef {
            let mut d = weapon("an94_mp+gl+reflex").clone();
            d.alt_drop_time = 0.5;
            Box::leak(Box::new(d))
        };
        let run = |fast: bool| {
            let mut a = arsenal_with(sf_like());
            if !fast {
                a.loadout.perks.clear();
            }
            let before = a.current().def.id.clone();
            let mut out = Vec::new();
            let mut rng = Rng::new(3);
            a.step(buttons::FIRE, buttons::ALT_MODE, false, 0, 0.0, &mut rng, &mut out);
            assert!(a.alt_pending && a.drop_left > 0.0);
            assert_eq!(a.current().def.id, before, "the outgoing mode is held through the drop");
            let mut drop_frames: usize = 1;
            while a.alt_pending {
                out.clear();
                a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
                assert!(a.alt_pending || a.current().def.id == "gl_an94_mp", "the mode changes as the drop ends");
                if a.alt_pending {
                    assert_eq!(shots(&out), 0, "no shot during the alt drop");
                }
                drop_frames += 1;
            }
            assert_eq!(a.raise_total, weapon("gl_an94_mp").alt_raise_time, "the incoming mode's alt raise");
            drop_frames
        };
        let slow = run(false);
        let fast = run(true);
        let expect = (0.5 / LOGIC_DT).ceil() as usize;
        assert!(slow.abs_diff(expect + 1) <= 1, "{slow} frames vs {expect}");
        assert!(fast.abs_diff(expect / 2 + 1) <= 1, "fast hands {fast} frames vs {}", expect / 2);
    }

    /// The Hybrid optic's pair as BO2's data has it: `an94_mp+dualoptic` (the attachment's
    /// `bAltWeaponAdsOnly`) and its `dualoptic_*` alt (the def's own flag), both ADS-only, each with
    /// its own alt drop and raise times (the raise must go unused).
    fn hybrid() -> Arsenal {
        let mut main = weapon("an94_mp").clone();
        main.id = "an94_mp+dualoptic".into();
        main.alt = "dualoptic_an94_mp".into();
        main.traits.alt_ads_only = true;
        main.alt_drop_time = 0.2;
        main.alt_raise_time = 0.4;
        main.ads_fov = 30.0;
        let main: &'static WeaponDef = Box::leak(Box::new(main));
        let mut alt = main.clone();
        alt.id = "dualoptic_an94_mp".into();
        alt.alt = main.id.clone();
        alt.alt_drop_time = 0.3;
        alt.alt_raise_time = 0.5;
        alt.ads_fov = 50.0;
        let alt: &'static WeaponDef = Box::leak(Box::new(alt));
        let mut a = arsenal_with(main);
        a.weapons[0].alt_mode = Some(alt);
        a.loadout.perks.clear();
        a
    }

    /// Step until the arsenal is fully aimed with nothing in progress.
    fn aim(a: &mut Arsenal, rng: &mut Rng) {
        let mut out = Vec::new();
        for _ in 0..200 {
            a.step(buttons::ADS, 0, false, 0, 0.0, rng, &mut out);
            if a.ads >= 1.0 && a.drop_left <= 0.0 && a.raise_left <= 0.0 {
                return;
            }
        }
        panic!("never fully aimed");
    }

    /// Hold the aim and press the sprint/hold-breath key, then step until the toggle is done;
    /// returns the frames it took and the lowest ADS fraction seen.
    fn hybrid_toggle(a: &mut Arsenal, rng: &mut Rng) -> (usize, f32) {
        let mut out = Vec::new();
        a.step(buttons::ADS | buttons::SPRINT, buttons::SPRINT, false, 0, 0.0, rng, &mut out);
        assert!(out.contains(&WeaponAction::AltToggled), "the toggle starts");
        let (mut n, mut low) = (1, a.ads);
        while a.alt_pending || a.drop_left > 0.0 || a.raise_left > 0.0 {
            a.step(buttons::ADS, 0, false, 0, 0.0, rng, &mut out);
            low = low.min(a.ads);
            n += 1;
            assert!(n < 200);
        }
        (n, low)
    }

    #[test]
    fn the_hybrid_optic_toggles_aimed_with_no_raise() {
        // BO2: the sprint/hold-breath key while aimed toggles an ADS-only alt (FUN_0053f380
        // action 9 -> FUN_00537dd0, posFrac >= 1); the drop is the outgoing weapon's alt drop
        // (FUN_008db500) and the weapon stays aimed through it (FUN_008daa10, states 5/2); two
        // ADS-only modes set no raise (FUN_008df320 -> FUN_008da850).
        let mut a = hybrid();
        let mut rng = Rng::new(5);
        aim(&mut a, &mut rng);
        let (n, low) = hybrid_toggle(&mut a, &mut rng);
        assert_eq!(a.current().def.id, "dualoptic_an94_mp");
        assert_eq!(low, 1.0, "the aim holds through the toggle");
        assert_eq!((a.raise_left, a.raise_total), (0.0, 0.0), "no raise between ADS-only modes");
        let expect = (0.2 / LOGIC_DT).ceil() as usize;
        assert!(n.abs_diff(expect + 1) <= 1, "{n} frames vs the 0.2 s drop");
        // And back over the alt's own drop, still with no raise.
        let (n, low) = hybrid_toggle(&mut a, &mut rng);
        assert_eq!(a.current().def.id, "an94_mp+dualoptic");
        assert_eq!((low, a.raise_total), (1.0, 0.0));
        let expect = (0.3 / LOGIC_DT).ceil() as usize;
        assert!(n.abs_diff(expect + 1) <= 1, "{n} frames vs the 0.3 s drop");
    }

    #[test]
    fn the_hybrid_optic_keeps_its_mode_when_unaimed_and_refuses_the_alt_key() {
        let mut a = hybrid();
        let mut rng = Rng::new(6);
        let mut out = Vec::new();
        // The alt-mode key refuses an ADS-only target (FUN_00537ca0(.., 0)), aimed or not.
        a.step(0, buttons::ALT_MODE, false, 0, 0.0, &mut rng, &mut out);
        aim(&mut a, &mut rng);
        a.step(buttons::ADS, buttons::ALT_MODE, false, 0, 0.0, &mut rng, &mut out);
        assert!(!a.alt_pending && a.current().def.id == "an94_mp+dualoptic" && !out.contains(&WeaponAction::AltToggled));
        // Sprint without the aim, or before the aim is full (FUN_005313a0: posFrac == 1), does nothing.
        let mut b = hybrid();
        b.step(0, buttons::SPRINT, false, 0, 0.0, &mut rng, &mut out);
        b.step(buttons::ADS, 0, false, 0, 0.0, &mut rng, &mut out);
        assert!(b.ads > 0.0 && b.ads < 1.0);
        b.step(buttons::ADS | buttons::SPRINT, buttons::SPRINT, false, 0, 0.0, &mut rng, &mut out);
        assert!(!b.alt_pending && !out.contains(&WeaponAction::AltToggled));
        // Toggle, then let go of the aim: BO2 has no revert, the red dot stays up for the next aim.
        hybrid_toggle(&mut a, &mut rng);
        for _ in 0..60 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!((a.ads, a.current().def.id.as_str()), (0.0, "dualoptic_an94_mp"));
        aim(&mut a, &mut rng);
        assert_eq!(a.current().def.id, "dualoptic_an94_mp");
        // ads-zoom replaced the arsenal's ads_fov_scale with the zoom module's level FOV.
        let fov = zoom::level_fov(a.current().def, 2);
        assert!((fov - 50.0).abs() < 1e-6, "aims through the alt's own optic: {fov}");
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
    fn dual(f: impl FnOnce(&mut WeaponDef)) -> &'static WeaponDef {
        def(|d| {
            d.id = "test_dw_mp".into();
            d.class = Class::Pistol;
            d.fire = FireMode::Semi;
            d.fire_time = 0.1;
            d.clip = 10;
            d.start_reserve = 80;
            d.max_reserve = 160;
            d.reload_time = 1.9;
            d.reload_empty_time = 2.4;
            d.reload_add_time = 1.3;
            d.reload_empty_add_time = 1.3;
            d.dual_wield_weapon = "test_lh_mp".into();
            f(d)
        })
    }

    fn fired(actions: &[WeaponAction], left: bool) -> usize {
        actions.iter().filter(|a| matches!(a, WeaponAction::Fire { pellet: 0, left: l, .. } if *l == left)).count()
    }

    /// Press `buttons` once (one frame), then release for `n` frames.
    fn tap(a: &mut Arsenal, buttons: u32, n: usize, rng: &mut Rng) -> Vec<WeaponAction> {
        let mut out = frames(a, buttons, 1, rng);
        out.extend(frames(a, 0, n, rng));
        out
    }

    /// BO2 fires a dual-wield weapon as two guns (`PM_Weapon` once per hand): the attack button
    /// fires the right gun from its magazine, the ADS button the left gun from its own; the spare
    /// rounds are shared.
    #[test]
    fn each_hand_fires_its_own_magazine() {
        let mut a = arsenal_with(dual(|_| {}));
        let mut rng = Rng::new(3);
        assert_eq!(a.current().left_ammo(), Some(10), "the left gun starts with a full magazine");
        let out = tap(&mut a, buttons::FIRE, 10, &mut rng);
        assert_eq!((fired(&out, false), fired(&out, true)), (1, 0));
        assert_eq!((a.current().ammo, a.current().left_ammo()), (9, Some(10)));
        let out = tap(&mut a, buttons::ADS, 10, &mut rng);
        assert_eq!((fired(&out, false), fired(&out, true)), (0, 1), "the ADS button is the left trigger");
        assert_eq!((a.current().ammo, a.current().left_ammo()), (9, Some(9)));
        // Both triggers at once: one round from each gun, on independent timers.
        let out = tap(&mut a, buttons::FIRE | buttons::ADS, 10, &mut rng);
        assert_eq!((fired(&out, false), fired(&out, true)), (1, 1));
        assert_eq!((a.current().ammo, a.current().left_ammo(), a.current().reserve), (8, Some(8), 80));
    }

    /// A dual-wield weapon has no sights (`aimDownSight` 0): holding ADS never raises the aim.
    #[test]
    fn a_dual_wield_weapon_never_aims() {
        let mut a = arsenal_with(dual(|d| d.ads_in_time = 0.1));
        let mut rng = Rng::new(4);
        frames(&mut a, buttons::ADS, 30, &mut rng);
        assert_eq!(a.ads, 0.0);
    }

    /// The reload button reaches both hands (`FUN_008dfd50` per hand), refilling each magazine
    /// from the shared spare rounds; an empty hand reloads by itself while the other keeps firing.
    #[test]
    fn reload_refills_both_hands_and_an_empty_hand_reloads_alone() {
        let mut a = arsenal_with(dual(|_| {}));
        let mut rng = Rng::new(5);
        for _ in 0..3 {
            tap(&mut a, buttons::FIRE | buttons::ADS, 8, &mut rng);
        }
        assert_eq!((a.current().ammo, a.current().left_ammo()), (7, Some(7)));
        let out = tap(&mut a, buttons::RELOAD, 200, &mut rng);
        assert_eq!(out.iter().filter(|x| **x == WeaponAction::ReloadStarted).count(), 2, "both guns reload");
        assert_eq!((a.current().ammo, a.current().left_ammo(), a.current().reserve), (10, Some(10), 74));

        // Empty the left gun only: it reloads on its own; the right keeps firing meanwhile.
        for _ in 0..10 {
            tap(&mut a, buttons::ADS, 8, &mut rng);
        }
        assert!(a.current().left.as_ref().unwrap().is_reloading(), "the empty left gun reloads by itself");
        assert!(!a.current().is_reloading());
        let out = tap(&mut a, buttons::FIRE, 8, &mut rng);
        assert_eq!(fired(&out, false), 1, "the right gun fires during the left reload");
        frames(&mut a, 0, 200, &mut rng);
        assert_eq!((a.current().ammo, a.current().left_ammo(), a.current().reserve), (9, Some(10), 64));
    }

    /// A single gun keeps one magazine and the ADS button aims, never fires.
    #[test]
    fn a_single_gun_has_no_left_hand() {
        let mut a = arsenal_with(def(|d| {
            d.class = Class::Pistol;
            d.fire = FireMode::Semi;
            d.clip = 10;
            d.start_reserve = 40;
        }));
        let mut rng = Rng::new(6);
        assert_eq!(a.current().left_ammo(), None);
        let out = tap(&mut a, buttons::ADS, 5, &mut rng);
        assert_eq!(shots(&out), 0);
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
        // Fire the last round: the empty reload starts by itself once its fire state ends.
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        for _ in 0..10 {
            if a.current().is_reloading() {
                break;
            }
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
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


#[cfg(test)]
mod zoom_tests {
    use super::*;
    use crate::input::buttons;

    fn sniper() -> &'static WeaponDef {
        let mut d = WeaponDef { id: "test_sniper_mp".into(), class: Class::Sniper, ads_fov: 15.0, ads_in_time: 0.1, ads_out_time: 0.1, overlay_reticle: 1, ..Default::default() };
        (d.zoom.fov2, d.zoom.fov3) = (15.0, 15.0);
        Box::leak(Box::new(d))
    }

    fn aimed(def: &'static WeaponDef) -> (Arsenal, Rng) {
        let mut a = Arsenal::new();
        a.weapons[0] = WeaponState::new(def);
        a.raise_left = 0.0;
        let mut rng = Rng::new(3);
        let mut out = Vec::new();
        for _ in 0..20 {
            a.step(buttons::ADS, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(a.ads, 1.0);
        (a, rng)
    }

    #[test]
    fn melee_is_refused_while_a_scope_is_aimed_and_works_from_the_hip() {
        let (mut a, mut rng) = aimed(sniper());
        let mut out = Vec::new();
        a.step(buttons::ADS | buttons::MELEE, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        assert!(!out.iter().any(|x| matches!(x, WeaponAction::Melee { .. })), "{out:?}");
        assert_eq!(a.ads, 1.0, "the aim holds");
        for _ in 0..20 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        out.clear();
        a.step(buttons::MELEE, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        assert!(out.iter().any(|x| matches!(x, WeaponAction::Melee { .. })), "{out:?}");
    }

    #[test]
    fn the_zoom_level_toggles_between_two_and_its_blend_clock_runs_in_the_sim() {
        let (mut a, mut rng) = aimed(sniper());
        assert_eq!(a.current().zoom.level, 0);
        a.weapons[0].step_zoom_level();
        assert_eq!((a.current().zoom.level, a.current().zoom.from, a.current().zoom.since), (1, 0, 0.0));
        let mut out = Vec::new();
        a.step(buttons::ADS, 0, false, 0, 0.0, &mut rng, &mut out);
        assert!((a.current().zoom.since - crate::units::LOGIC_DT).abs() < 1e-6);
        a.weapons[0].step_zoom_level();
        assert_eq!((a.current().zoom.level, a.current().zoom.from), (0, 1), "(level + 1) % 2");
    }
}

#[cfg(test)]
mod quick_reload_tests {
    use super::*;
    use crate::input::buttons;

    /// The AN-94 with Fast Mag, as converted: reload 1.67/2.095 (add 0.877), quick 1.67/2.095
    /// (add 0.9).
    fn fast_mag(dual_mag: bool) -> &'static WeaponDef {
        let mut d = WeaponDef { id: "test_fastmag_mp".into(), class: Class::Rifle, clip: 30, start_reserve: 300, max_reserve: 300, ..Default::default() };
        (d.reload_time, d.reload_empty_time, d.reload_add_time, d.reload_empty_add_time) = (1.67, 2.095, 0.877, 0.0);
        (d.timing.reload_quick_time, d.timing.reload_quick_empty_time, d.timing.reload_quick_add_time) = (1.2, 1.5, 0.9);
        d.traits.dual_mag = dual_mag;
        Box::leak(Box::new(d))
    }

    fn reload(a: &mut Arsenal, rng: &mut Rng) -> Reload {
        let mut out = Vec::new();
        a.step(0, buttons::RELOAD, false, 0, 0.0, rng, &mut out);
        let r = a.current().reload.expect("reloading");
        while a.current().is_reloading() {
            a.step(0, 0, false, 0, 0.0, rng, &mut out);
        }
        r
    }

    fn kit(def: &'static WeaponDef) -> (Arsenal, Rng) {
        let mut a = Arsenal::new();
        a.loadout.perks.clear();
        a.weapons[0] = WeaponState::new(def);
        a.raise_left = 0.0;
        (a, Rng::new(5))
    }

    #[test]
    fn fast_mag_alternates_the_quick_swap_with_a_full_reload() {
        let (mut a, mut rng) = kit(fast_mag(true));
        let mut kinds = Vec::new();
        for _ in 0..4 {
            a.weapons[0].ammo = 10;
            let Reload::Magazine { quick, length, add_at, .. } = reload(&mut a, &mut rng) else { panic!() };
            kinds.push((quick, length, add_at));
            assert_eq!(a.current().ammo, 30);
        }
        assert_eq!(kinds, vec![(true, 1.2, 0.9), (false, 1.67, 0.877), (true, 1.2, 0.9), (false, 1.67, 0.877)], "the first reload is the quick one (FUN_008ddce0)");
        // From empty: the quick empty length; no empty add time, so the add time (capped at the
        // normal empty reload).
        a.weapons[0].ammo = 0;
        let Reload::Magazine { quick, length, add_at, empty, .. } = reload(&mut a, &mut rng) else { panic!() };
        assert!(quick && empty);
        assert_eq!((length, add_at), (1.5, 0.9));
    }

    #[test]
    fn an_interrupted_reload_before_the_add_keeps_the_flag() {
        let (mut a, mut rng) = kit(fast_mag(true));
        a.weapons[0].ammo = 10;
        let mut out = Vec::new();
        a.step(0, buttons::RELOAD, false, 0, 0.0, &mut rng, &mut out);
        a.weapons[0].reload = None; // swapped away / meleed before the rounds went in
        assert!(a.current().quick_reload);
        let Reload::Magazine { quick, .. } = reload(&mut a, &mut rng) else { panic!() };
        assert!(quick);
        assert!(!a.current().quick_reload);
    }

    #[test]
    fn without_a_dual_mag_every_reload_is_normal_and_an_empty_one_adds_at_the_add_time() {
        let (mut a, mut rng) = kit(fast_mag(false));
        for _ in 0..2 {
            a.weapons[0].ammo = 0;
            let Reload::Magazine { quick, length, add_at, .. } = reload(&mut a, &mut rng) else { panic!() };
            assert!(!quick);
            // BO2 (`FUN_008dec90`): no `iReloadEmptyAddTime` → the rounds go in at `iReloadAddTime`.
            assert_eq!((length, add_at), (2.095, 0.877));
        }
    }
}
