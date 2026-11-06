/*
** Conquer Ops - Zero Hour gameplay core: the weapons: definitions, the converted table, the catalog, loadouts and the first-person state machine.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

pub mod aim_spread;
pub mod arsenal;
pub mod weapon_state;
pub mod catalog;
pub mod def;
pub mod feel;
pub mod gunpos;
pub mod handling;
pub mod idle_sway;
pub mod loadout;
pub mod perks;
pub mod pickup;
pub mod recoil;
pub mod rng;
pub mod scope;
pub mod specialty;
pub mod table;
pub mod view_kick;
pub mod zoom;

pub use arsenal::*;
pub use weapon_state::*;
pub use catalog::*;
pub use def::*;
pub use handling::{AntiQuickScope, CycleTimes, OffhandClass, Penetrate, Traits, WeapType, Zoom};
pub use feel::{optic_of, reticle_art, reticle_index, reticle_name_key, GunKick, Reticle, Sway, ViewPos, RETICLE_OPTICS};
pub use loadout::*;
pub use perks::*;
pub use pickup::{base_of, droppable, AmmoTaken, Carried, SwapRefused, WeaponOptions};
pub use rng::*;
pub use specialty::*;
pub use table::*;

pub use crate::attachments::AttachmentDef;

#[cfg(test)]
mod tests;

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

    #[test]
    fn the_assault_shield_is_a_primary_that_melees_with_itself_and_fires_nothing() {
        let shield = catalog().get("riotshield_mp").expect("the built-in shield");
        assert!(shield.is_shield() && shield.shield.is_some());
        assert!(!shield.class.is_gun() && shield.class.is_slot_weapon());
        assert_eq!(shield.cac_class(), Class::Special, "BO2 lists it under SPECIALS");
        // `_scoreevents.gsc:375`: the shield's melee is its own, not the class knife's stab.
        let knife = catalog().get(CLASS_MELEE).unwrap();
        assert_eq!(melee_weapon(shield, knife).id, "riotshield_mp");
        let mut a = Arsenal::new();
        a.weapons[0] = WeaponState::new(shield);
        a.raise_left = 0.0;
        assert_eq!(a.current().ammo, 1, "its one round is the deploy");
        let mut rng = Rng::new(1);
        let mut out = Vec::new();
        a.step(buttons::FIRE, buttons::FIRE, false, 0, 0.0, &mut rng, &mut out);
        assert!(!out.iter().any(|w| matches!(w, WeaponAction::Fire { .. } | WeaponAction::Launch { .. })), "{out:?}");
        // It does not aim: BO2's `riotshield_mp` has `aimDownSight` 0, so `FUN_008e4470` keeps
        // the ADS fraction at 0 (its `ads_down` clip is only the hip pose's layer).
        assert!(!shield.aims);
        for _ in 0..30 {
            a.step(buttons::ADS, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        assert_eq!(a.ads, 0.0);
        assert_eq!(a.move_scale(), shield.move_scale);
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
        // A melee start is the swing (the strike lands after the melee weapon's delay).
        let melee = |x: &WeaponAction| matches!(x, WeaponAction::Melee { .. } | WeaponAction::MeleeSwing);
        a.step(buttons::ADS | buttons::MELEE, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        assert!(!out.iter().any(melee), "{out:?}");
        assert_eq!(a.ads, 1.0, "the aim holds");
        for _ in 0..20 {
            a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
        }
        out.clear();
        a.step(buttons::MELEE, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        assert!(out.iter().any(melee), "{out:?}");
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
