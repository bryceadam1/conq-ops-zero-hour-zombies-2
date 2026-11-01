use super::*;
use crate::input::buttons;
use crate::units::LOGIC_DT;

fn def(f: impl FnOnce(&mut WeaponDef)) -> &'static WeaponDef {
    let mut d = WeaponDef { id: "test_mp".into(), ..Default::default() };
    f(&mut d);
    Box::leak(Box::new(d))
}

fn arsenal_with(primary: &'static WeaponDef) -> Arsenal {
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
        let mut a = Arsenal::with_loadout(Loadout::test_kit().with_spec(perks));
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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

    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
        let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
        let mut a = Arsenal::with_loadout(Loadout::test_kit().with_spec(perks));
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
    // `Loadout::test_kit()` is still the classes' spec base and the converter's source.
    let base = Loadout::test_kit();
    assert!(!base.primary.is_empty() && !base.secondary.is_empty());
    assert!(!Arsenal::with_loadout(Loadout::test_kit()).is_unarmed(), "Arsenal::new stays the full base kit for the tests/converter");
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
    assert!((thrown_at - (32.0 * LOGIC_DT + def.timing.fire_delay * m)).abs() <= LOGIC_DT * 1.01, "left at {thrown_at}");
    // The fuse burned from the end of the pull-back (BO2's `FUN_008e28a0` sets it there).
    let want = def.fuse_time - (thrown_at - def.timing.hold_fire_time * m);
    assert!((throw - want).abs() <= LOGIC_DT * 1.5, "cooked: {throw} left, want {want}");
    assert_eq!(a.lethal.count, 0);
    // The weapon comes back after the throw.
    frames(&mut a, 0, 60, &mut rng);
    assert!(a.offhand.is_none() && !a.is_busy());
}

#[test]
fn cooking_too_long_goes_off_in_hand() {
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
    a.raise_left = 0.0;
    a.loadout.perks.clear();
    // The secondary's first raise (it has not been up yet), after the primary's drop.
    let expect = ((a.weapons[0].def.drop_time + a.weapons[1].def.first_raise_time) / LOGIC_DT).ceil();
    let slow = swap_frames(&mut a);
    assert_eq!(a.active, 1);
    let mut b = Arsenal::with_loadout(Loadout::test_kit());
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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

/// Scavenger refills the guns (`getweaponslistprimaries`, `_weapons.gsc:1795`): with the
/// under-barrel launcher up, the rifle's spare rounds get a magazine and the launcher's grenades
/// none — as with the rifle up.
#[test]
fn scavenger_refills_the_gun_not_its_launcher_whichever_mode_is_up() {
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
    a.raise_left = 0.0;
    let gun = a.current().main;
    a.current_mut().reserve = 0;
    let gl_reserve = a.current().alt_reserve;
    a.toggle_alt();
    assert!(a.current().alt_up(), "the launcher is up");
    a.scavenge();
    let w = a.current();
    assert_eq!(w.main_rounds().1, gun.clip.min(gun.max_reserve), "the rifle's stock got a magazine");
    assert_eq!(w.reserve, gl_reserve, "the launcher's grenades are not refilled");
    // And the same with the rifle up.
    let mut b = Arsenal::with_loadout(Loadout::test_kit());
    b.current_mut().reserve = 0;
    b.scavenge();
    assert_eq!((b.current().reserve, b.current().alt_reserve), (w.main_rounds().1, gl_reserve));
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
    let mut a = Arsenal::with_loadout(Loadout::test_kit());
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
