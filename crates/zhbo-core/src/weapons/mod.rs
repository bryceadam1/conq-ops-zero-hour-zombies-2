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

    /// BO2 melees with the class melee weapon (`knife_mp`, given to every class) while a gun is up,
    /// and with the knife itself while the held knife is up.
    #[test]
    fn a_gun_melees_with_the_class_melee_weapon_and_the_held_knife_with_itself() {
        let mut rng = Rng::new(1);
        let class_melee = weapon(CLASS_MELEE);
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        assert!(a.current().def.class.is_gun());
        assert_eq!(a.class_melee.id, CLASS_MELEE);
        let mut out = Vec::new();
        a.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(a.melee_left, class_melee.melee_time);
        let strike = |a: &mut Arsenal, rng: &mut Rng, out: &mut Vec<WeaponAction>| {
            while !out.iter().any(|x| matches!(x, WeaponAction::Melee { .. })) {
                a.step(0, 0, false, 0, 0.0, rng, out);
            }
        };
        strike(&mut a, &mut rng, &mut out);
        assert!(out.iter().any(|x| matches!(x, WeaponAction::Melee { damage } if *damage == class_melee.melee_damage)));
        assert_eq!(a.current_weapon().id, CLASS_MELEE, "the current weapon while meleeing");

        let mut k = Arsenal::unarmed();
        let held = k.current().def;
        assert_eq!(held.id, "knife_held_mp");
        assert_ne!(held.melee_time, class_melee.melee_time, "the stand-ins differ");
        out.clear();
        k.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(k.melee_left, held.melee_time);
        assert_eq!(k.melee_weapon().id, "knife_held_mp");
    }

    /// BO2's melee: the swing (`EV_MELEE_SWIPE`) on the press, the strike (`EV_FIRE_MELEE`) when
    /// the melee weapon's `iMeleeDelay` has run out (`FUN_008dc770` / `FUN_008e4da0`), in plain
    /// time — Fast Melee Recovery does not shorten the delay. A replay strikes on the same frame.
    #[test]
    fn the_strike_lands_after_the_melee_weapons_delay() {
        let class_melee = weapon(CLASS_MELEE);
        assert!(class_melee.melee_delay > LOGIC_DT, "the stand-in has a delay to wait for");
        let swing_and_strike = |perks: &str| {
            let mut rng = Rng::new(1);
            let mut a = Arsenal::with_loadout(Loadout::default().with_spec(perks));
            a.raise_left = 0.0;
            let mut out = Vec::new();
            a.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
            assert!(out.contains(&WeaponAction::MeleeSwing), "the swing starts on the press");
            assert!(!out.iter().any(|x| matches!(x, WeaponAction::Melee { .. })), "no strike on the press");
            let mut frames = 0;
            loop {
                out.clear();
                a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
                frames += 1;
                if out.iter().any(|x| matches!(x, WeaponAction::Melee { .. })) {
                    break;
                }
            }
            frames
        };
        let frames = swing_and_strike("perks=");
        assert_eq!(frames, (class_melee.melee_delay / LOGIC_DT - 1e-4).ceil() as i32, "the strike after the delay");
        assert_eq!(swing_and_strike("perks="), frames, "deterministic");
        assert_eq!(swing_and_strike("perks=Dexterity"), frames, "Fast Melee Recovery leaves the delay alone");
    }

    /// An offhand pressed mid-swing (0x13) takes the state over: the strike still due never lands
    /// (0x16 zeroes `weaponDelay`) and the swing's time left is waited out first; pressed during the
    /// sprint-out (0x26), its time left is.
    #[test]
    fn an_offhand_mid_swing_drops_the_strike_and_waits_the_swing_out() {
        let mut rng = Rng::new(1);
        let mut a = Arsenal::with_loadout(Loadout::default());
        a.raise_left = 0.0;
        let mut out = Vec::new();
        a.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        let swing = a.melee_left;
        assert!(a.melee_delay_left > LOGIC_DT, "the strike is still due");
        out.clear();
        a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
        match a.offhand {
            // The swing ticked once more in plain time before the press.
            Some(OffhandUse::Starting { left, .. }) => assert!((left - (swing - LOGIC_DT)).abs() < 1e-4, "{left} vs {swing}"),
            other => panic!("{other:?}"),
        }
        for _ in 0..120 {
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert!(!out.iter().any(|x| matches!(x, WeaponAction::Melee { .. })), "the strike never lands");
        assert!(matches!(a.offhand, Some(OffhandUse::Holding { .. })), "then the pull-back");

        let mut a = Arsenal::with_loadout(Loadout::default());
        a.raise_left = 0.0;
        a.since_sprint = 0.01;
        let sprint_out = a.current().def.sprint_out_time;
        assert!(sprint_out > 0.05, "the stand-in has a sprint-out");
        a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
        match a.offhand {
            Some(OffhandUse::Starting { left, .. }) => assert!(left > 0.0 && left < sprint_out, "{left}"),
            other => panic!("{other:?}"),
        }
    }

    /// A switch pressed mid-swing is held back, not dropped: it happens once the swing and the
    /// gun's re-raise are done (`FUN_008e0e60` returns at 0x13-0x15 and the command keeps it).
    #[test]
    fn a_switch_during_a_melee_happens_after_it() {
        let mut rng = Rng::new(1);
        let mut a = Arsenal::with_loadout(Loadout::default());
        a.raise_left = 0.0;
        let mut out = Vec::new();
        a.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        a.step(buttons::SWAP_WEAPON, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(a.active, 0, "not during the swing");
        while a.melee_left > 0.0 || a.melee_recover_left > 0.0 {
            out.clear();
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
            assert!(!out.contains(&WeaponAction::Swapped) || (a.melee_left <= 0.0 && a.melee_recover_left <= 0.0));
        }
        a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        assert_eq!(a.active, 1, "switched afterwards");
    }

    /// The weapon-switch key during a non-cancelable offhand (the frag): the switch is kept and
    /// happens once the offhand is done; on a cancelable one it is the cancel, no switch.
    #[test]
    fn a_switch_during_a_frag_happens_after_it() {
        let run = |cancelable: bool| {
            let mut rng = Rng::new(1);
            let mut a = Arsenal::with_loadout(Loadout::default());
            a.raise_left = 0.0;
            assert!(a.weapons.len() > 1);
            let def: &'static WeaponDef = Box::leak(Box::new(WeaponDef { hold_cancelable: cancelable, ..a.lethal.def.clone() }));
            a.lethal = Offhand { def, count: 2 };
            let mut out = Vec::new();
            a.step(buttons::LETHAL, buttons::LETHAL, false, 0, 0.0, &mut rng, &mut out);
            for _ in 0..40 {
                a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
            }
            a.step(buttons::LETHAL | buttons::SWAP_WEAPON, buttons::SWAP_WEAPON, false, 0, 0.0, &mut rng, &mut out);
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
            assert_eq!(a.active, 0, "no switch during the offhand");
            let mut swapped_while_busy = false;
            for _ in 0..200 {
                let busy = a.offhand.is_some();
                let mut step = Vec::new();
                a.step(0, 0, false, 0, 0.0, &mut rng, &mut step);
                swapped_while_busy |= busy && a.offhand.is_some() && step.contains(&WeaponAction::Swapped);
                out.extend(step);
            }
            assert!(!swapped_while_busy);
            (a.active, out.contains(&WeaponAction::Swapped), a.lethal.count)
        };
        assert_eq!(run(false), (1, true, 1), "thrown, then switched");
        assert_eq!(run(true), (0, false, 2), "cancelled, no switch");
    }

    /// BO2 runs the swing's `iMeleeTime` in plain time, then — a gun up, so the class melee
    /// weapon struck — brings the gun back up in its own `quickRaiseTime` (state 0x15,
    /// `FUN_008e1f50`), the one melee state Fast Melee Recovery speeds up (G3). The held knife
    /// strikes with itself and is simply ready again.
    #[test]
    fn after_a_melee_the_gun_comes_back_up_and_fast_melee_recovery_speeds_only_that() {
        let class_melee = weapon(CLASS_MELEE);
        // Frames of the swing, of the re-raise, and whether the trigger did anything meanwhile.
        let melee = |perks: &str| {
            let mut rng = Rng::new(1);
            let mut a = Arsenal::with_loadout(Loadout::default().with_spec(perks));
            a.raise_left = 0.0;
            let mut out = Vec::new();
            a.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
            let (mut swing, mut raise) = (1, 0);
            while a.melee_left > 0.0 {
                a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut out);
                swing += 1;
            }
            let mut fired = shots(&out) > 0;
            while a.melee_recover_left > 0.0 {
                let mut step = Vec::new();
                a.step(buttons::FIRE, 0, false, 0, 0.0, &mut rng, &mut step);
                raise += 1;
                // The frame the re-raise ends the gun is ready and may fire.
                fired |= a.melee_recover_left > 0.0 && shots(&step) > 0;
            }
            (swing, raise, fired, a.multiplier(Timing::Melee), a.current().def.quick_raise_time)
        };
        let (swing, raise, fired, m, quick) = melee("perks=");
        assert_eq!(m, 1.0);
        assert_eq!(swing, 1 + (class_melee.melee_time / LOGIC_DT - 1e-4).ceil() as i32, "the press, then the swing in plain time");
        assert_eq!(raise, (quick / LOGIC_DT - 1e-4).ceil() as i32, "the gun's quick raise");
        assert!(!fired, "the gun is not up until the re-raise ends");
        assert_eq!(melee("perks="), (swing, raise, fired, m, quick), "deterministic");
        let (fast_swing, fast_raise, _, fast, _) = melee("perks=Dexterity");
        assert!(fast < 1.0, "Dexterity grants Fast Melee Recovery");
        assert_eq!(fast_swing, swing, "the swing is not shortened");
        assert_eq!(fast_raise, (quick * fast / LOGIC_DT - 1e-4).ceil() as i32, "only the re-raise is");

        // The held knife: no re-raise.
        let mut rng = Rng::new(1);
        let mut k = Arsenal::unarmed();
        let mut out = Vec::new();
        k.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        while k.melee_left > 0.0 {
            k.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(k.melee_recover_left, 0.0);
        assert!(!k.is_busy());
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
        // Hold (cook) for one second, then let go: the grenade leaves the hand `fire_delay` later.
        for _ in 0..30 {
            a.step(buttons::LETHAL, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        let def = a.lethal.def;
        let mut frames_held = 31;
        let throw = loop {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
            frames_held += 1;
            if let Some(left) = out.iter().find_map(|x| if let WeaponAction::Throw { fuse_left, .. } = x { Some(*fuse_left) } else { None }) {
                break left;
            }
            assert!(frames_held < 60, "never thrown");
        };
        // The release is seen on the first frame the button is up; the offhand states run at the
        // class's Dexterity rate (`fasttoss`, G3).
        let m = a.multiplier(Timing::Toss);
        let thrown_at = frames_held as f32 * LOGIC_DT;
        assert!((thrown_at - (32.0 * LOGIC_DT + def.fire_delay * m)).abs() <= LOGIC_DT * 1.01, "left at {thrown_at}");
        // The fuse burned from the end of the pull-back (BO2's `FUN_008e28a0` sets it there).
        let want = def.fuse_time - (thrown_at - def.hold_fire_time * m);
        assert!((throw - want).abs() <= LOGIC_DT * 1.5, "cooked: {throw} left, want {want}");
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

