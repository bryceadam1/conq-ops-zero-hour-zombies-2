/*
** Conquer Ops - Zero Hour gameplay core: how BO2 picks a player's death animation.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! A dead player's body falls by `mp/playeranim.script`'s `DEATH` section, the first entry whose
//! conditions all hold ("the first match found is the one used"), one of its alternatives. The
//! engine sets every condition from the player as he dies (`BG_AnimScriptEvent`, t6mp
//! `FUN_008acce0`, runs the 25 condition updaters of the table at `0x00f6afe8` before it picks);
//! the `DEATH` section reads five of them, each worked out here as the engine does:
//!
//! * `stance` (`FUN_008adf50`): prone, crouch or stand.
//! * `movestatus` (`FUN_008ad980`): his horizontal speed against `player_runThreshhold` (110 in/s,
//!   run) and `player_moveThreshhold` (2 in/s, walk), else stationary ([`move_status`]).
//! * `direction` (`FUN_008adb70`): his movement keys, not his velocity: the forward/right move
//!   normalised, forward past `player_strafeAnimCosAngle` (0.5) forward, past -0.5 backward, else
//!   the strafe's side, none without keys ([`move_direction`]).
//! * `dmgType` (`FUN_008ade60`, from the value `finishplayerdamage` `FUN_005ed360` and `player_die`
//!   `FUN_005effe0` store): by the killing blow's means of death and weapon ([`DmgType::of`]).
//! * `dmgDirection` (`FUN_008add10`, stored by `finishplayerdamage`): where the blow came from in
//!   his view, by its direction (`vDir`) against his view yaw ([`dmg_direction`]).
//!
//! What the script does with them (this install's `DEATH`): prone dies by
//! `pb_prone_death_quickdeath` whatever hit him; an explosive, a shotgun, a headshot, the shield's
//! bash and the knife each have a clip per side (crouched ones too, except the explosive);
//! anything else falls by the stance and gait: crouched running, crouched, running backward, left
//! or right, running, standing (8 alternatives). The converter asks the script in every context
//! ([`crate::torso::death_context`]); the client picks from the answers.
//!
//! **Presentation only.** The sim reports the killing blow (`EventKind::KilledBy`); the client
//! reads stance, gait and keys off the seat's last record. Nothing here feeds the sim.

use crate::damage::MeansOfDeath;
use crate::math::Vec3;
use crate::weapons::{WeapClass, WeapType, WeaponDef};

/// BO2's `dmgType` condition values, in the engine's order (`torso::DMG_TYPES`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DmgType {
    #[default]
    Normal,
    NormalShotgun,
    Explosive,
    Melee,
    MeleeBash,
    HeadShot,
    Fire,
    Suicide,
    Gas,
    Falling,
}

impl DmgType {
    pub const ALL: [DmgType; 10] = [
        DmgType::Normal,
        DmgType::NormalShotgun,
        DmgType::Explosive,
        DmgType::Melee,
        DmgType::MeleeBash,
        DmgType::HeadShot,
        DmgType::Fire,
        DmgType::Suicide,
        DmgType::Gas,
        DmgType::Falling,
    ];

    /// The script's name for it.
    pub fn name(self) -> &'static str {
        crate::torso::DMG_TYPES[self as usize]
    }

    /// The `dmgType` of a killing blow by means `how` with `weapon` (`player_die`, t6mp
    /// `FUN_005effe0`, the last to set it):
    ///
    /// * `MOD_BURNED`: fire;
    /// * `MOD_GRENADE`, `_GRENADE_SPLASH`, `_PROJECTILE`, `_PROJECTILE_SPLASH`, `_EXPLOSIVE`:
    ///   explosive;
    /// * `MOD_MELEE`, `_BAYONET`: the shield's bash (weapType `riotshield`) meleebash, else melee;
    /// * `MOD_HEAD_SHOT`: headshot (a shotgun's headshot too: the means is tested first);
    /// * `MOD_SUICIDE`: a grenade's (weapType `grenade`) explosive, else suicide;
    /// * `MOD_FALLING` with no ground under him (his ground entity, `ps + 0xa4`, is `0x3ff`, none:
    ///   what the movement's ground trace `FUN_008c8420` sets in the air): falling;
    /// * anything else (bullets, impacts, crushes, ...): a spread weapon's (`weapClass` spread or
    ///   pistol spread) normal_shotgun, else normal. `MOD_GAS` is never told apart: gas is none of
    ///   the engine's branches.
    pub fn of(how: MeansOfDeath, weapon: Option<&WeaponDef>, airborne: bool) -> DmgType {
        use MeansOfDeath as M;
        let weap_type = weapon.map(|w| w.traits.weap_type);
        match how {
            M::Burned => DmgType::Fire,
            M::Grenade | M::GrenadeSplash | M::Projectile | M::ProjectileSplash | M::Explosive => DmgType::Explosive,
            M::Melee | M::Bayonet if weap_type == Some(WeapType::RiotShield) => DmgType::MeleeBash,
            M::Melee | M::Bayonet => DmgType::Melee,
            M::HeadShot => DmgType::HeadShot,
            M::Suicide if weap_type == Some(WeapType::Grenade) => DmgType::Explosive,
            M::Suicide => DmgType::Suicide,
            M::Falling if airborne => DmgType::Falling,
            _ if weapon.is_some_and(|w| matches!(w.weap_class, WeapClass::Spread | WeapClass::PistolSpread)) => DmgType::NormalShotgun,
            _ => DmgType::Normal,
        }
    }
}

/// BO2's `dmgDirection` (`torso::DMG_DIRECTIONS`: 1 front, 2 back, 3 left, 4 right; 0 none is no
/// blow's, [`NO_DIRECTION`]) of a blow that
/// travelled along `dir` (BO2's `vDir`) into a player whose view yaw is `view_yaw` (radians,
/// counter-clockwise from +X, as BO2's yaw): `finishplayerdamage` (t6mp `FUN_005ed360`) takes
/// `vectoyaw(vDir)` less his whole-degree view yaw into [0, 360): [135, 225) front, [45, 135) right,
/// [225, 315) left, else back — and back for a blow with no direction.
pub fn dmg_direction(dir: Vec3, view_yaw: f32) -> u8 {
    const FRONT: u8 = 1;
    const BACK: u8 = 2;
    const LEFT: u8 = 3;
    const RIGHT: u8 = 4;
    if dir.x * dir.x + dir.y * dir.y < 1e-12 {
        return BACK;
    }
    let view = view_yaw.to_degrees().rem_euclid(360.0).trunc();
    let f = (crate::math::det::atan2(dir.y, dir.x).to_degrees() - view).rem_euclid(360.0);
    if !(45.0..315.0).contains(&f) {
        BACK
    } else if (135.0..225.0).contains(&f) {
        FRONT
    } else if f < 135.0 {
        RIGHT
    } else {
        LEFT
    }
}

/// `dmgDirection` none: a death no damage named a side for (BO2: a suicide before any damage).
pub const NO_DIRECTION: u8 = 0;

/// BO2's `movestatus` index (`torso::MOVE_STATUSES`) of a horizontal speed in Zero Hour units per
/// second (`FUN_008ad980`, the tuning pack's `player_runThreshhold` and `player_moveThreshhold`).
pub fn move_status(speed: f32, t: &crate::tuning::Tuning) -> u8 {
    let speed = speed / crate::units::ZH_PER_INCH;
    if speed > t.player_run_threshhold {
        2
    } else if speed > t.player_move_threshhold {
        1
    } else {
        0
    }
}

/// BO2's `direction` index (`torso::DIRECTIONS`: none, forward, backward, left, right) of the
/// movement keys, forward and right in [-1, 1] (`FUN_008adb70`, `player_strafeAnimCosAngle`).
pub fn move_direction(forward: f32, right: f32, t: &crate::tuning::Tuning) -> u8 {
    let len = (forward * forward + right * right).sqrt();
    let len = if len < 1e-6 { 1.0 } else { len };
    let (f, r) = (forward / len, right / len);
    let cos = t.player_strafe_anim_cos_angle;
    if f > cos {
        1
    } else if f < -cos {
        2
    } else if r > 0.0 {
        4
    } else if r < 0.0 {
        3
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::WeapType;

    fn def(class: WeapClass, ty: WeapType) -> &'static WeaponDef {
        let mut traits = WeaponDef::default().traits;
        traits.weap_type = ty;
        Box::leak(Box::new(WeaponDef { weap_class: class, traits, ..Default::default() }))
    }

    /// Every means of death BO2 has, by every weapon class and type, against `player_die`'s rule
    /// written out case by case; and every `dmgType` the script can be asked is reached by some.
    #[test]
    fn every_means_and_weapon_dies_by_player_dies_dmg_type() {
        use DmgType as D;
        use MeansOfDeath as M;
        let mut reached = std::collections::HashSet::new();
        for how in M::ALL {
            for class in WeapClass::ALL {
                for ty in WeapType::ALL {
                    for airborne in [false, true] {
                        let w = def(class, ty);
                        let spread = matches!(class, WeapClass::Spread | WeapClass::PistolSpread);
                        let want = match how {
                            M::Burned => D::Fire,
                            M::Grenade | M::GrenadeSplash | M::Projectile | M::ProjectileSplash | M::Explosive => D::Explosive,
                            M::Melee | M::Bayonet => {
                                if ty == WeapType::RiotShield {
                                    D::MeleeBash
                                } else {
                                    D::Melee
                                }
                            }
                            M::HeadShot => D::HeadShot,
                            M::Suicide => {
                                if ty == WeapType::Grenade {
                                    D::Explosive
                                } else {
                                    D::Suicide
                                }
                            }
                            M::Falling if airborne => D::Falling,
                            _ if spread => D::NormalShotgun,
                            _ => D::Normal,
                        };
                        let got = DmgType::of(how, Some(w), airborne);
                        assert_eq!(got, want, "{how:?} by {class:?}/{ty:?} airborne {airborne}");
                        reached.insert(got);
                    }
                }
            }
            // No weapon (a death to the world): never a shotgun or a bash.
            assert!(!matches!(DmgType::of(how, None, false), D::NormalShotgun | D::MeleeBash));
        }
        let unreached: Vec<DmgType> = DmgType::ALL.into_iter().filter(|d| !reached.contains(d)).collect();
        assert_eq!(unreached, [D::Gas], "gas is in the script's table, set by no branch");
        for d in DmgType::ALL {
            assert_eq!(DmgType::ALL[d as usize], d);
            assert!(!d.name().is_empty());
        }
    }

    /// The blow's direction against his view yaw: every 45-degree sector, both sides of each edge.
    #[test]
    fn the_blows_direction_names_the_side_it_came_from() {
        let dir = |deg: f32| Vec3::new(deg.to_radians().cos(), deg.to_radians().sin(), 0.0);
        // Facing +X: a blow travelling -X came from in front.
        assert_eq!(dmg_direction(dir(180.0), 0.0), 1, "front");
        assert_eq!(dmg_direction(dir(0.0), 0.0), 2, "back");
        assert_eq!(dmg_direction(dir(270.0), 0.0), 3, "travelling -Y: from his left (+Y)");
        assert_eq!(dmg_direction(dir(90.0), 0.0), 4, "travelling +Y: from his right");
        assert_eq!(dmg_direction(Vec3::ZERO, 1.0), 2, "no direction: back");
        for (deg, want) in [(44.0, 2), (46.0, 4), (134.0, 4), (136.0, 1), (224.0, 1), (226.0, 3), (314.0, 3), (316.0, 2)] {
            assert_eq!(dmg_direction(dir(deg), 0.0), want, "{deg}");
            // Turned: the same blow in his frame.
            assert_eq!(dmg_direction(dir(deg + 90.0), std::f32::consts::FRAC_PI_2), want, "{deg} turned");
        }
    }

    #[test]
    fn gait_and_keys_read_as_the_engine_reads_them() {
        let t = crate::tuning::Tuning::default();
        let zh = |inches: f32| crate::units::inches(inches);
        assert_eq!(move_status(zh(0.0), &t), 0);
        assert_eq!(move_status(zh(2.5), &t), 1);
        assert_eq!(move_status(zh(110.0), &t), 1);
        assert_eq!(move_status(zh(111.0), &t), 2);
        assert_eq!(move_direction(0.0, 0.0, &t), 0);
        assert_eq!(move_direction(1.0, 0.0, &t), 1);
        assert_eq!(move_direction(-1.0, 0.0, &t), 2);
        assert_eq!(move_direction(0.0, -1.0, &t), 3);
        assert_eq!(move_direction(0.0, 1.0, &t), 4);
        // Diagonals: forward-right is forward (cos 45 > 0.5), back-left backward.
        assert_eq!(move_direction(1.0, 1.0, &t), 1);
        assert_eq!(move_direction(-1.0, -1.0, &t), 2);
        assert_eq!(move_direction(0.4, 1.0, &t), 4);
    }
}
