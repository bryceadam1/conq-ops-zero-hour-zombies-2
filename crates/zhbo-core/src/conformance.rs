//! Does a weapon play by its BO2 definition? Runs the real arsenal with one weapon and measures:
//! fire rate (BO2's fire time), the fire mode (semi, auto, n-round burst, minigun spin-up), pellets
//! per shot, magazine size before the automatic reload, reload timing (rounds in at the add time, done
//! at the reload time; shell by shell for tube shotguns), ADS time, the damage curve, and for
//! offhands the fuse and cooking. Used by zhbo-assets' test over every multiplayer weapon in the
//! player's own BO2 install, and by this crate's tests on the built-in table.

use crate::input::buttons;
use crate::units::LOGIC_DT;
use crate::weapons::{Arsenal, Class, FireMode, Offhand, Place, Rng, WeaponAction, WeaponDef, WeaponState, OFFHAND_PULLBACK};

/// Frames for `seconds`, rounded up.
fn frames(seconds: f32) -> usize {
    (seconds / LOGIC_DT).ceil() as usize
}

fn arsenal(def: &'static WeaponDef) -> Arsenal {
    let mut a = Arsenal::new();
    a.loadout.perks.clear();
    a.weapons[0] = WeaponState::new(def);
    a.weapons[0].reserve = def.clip.max(1) * 4;
    a.raise_left = 0.0;
    a
}

fn step(a: &mut Arsenal, held: u32, pressed: u32, out: &mut Vec<WeaponAction>, rng: &mut Rng) {
    a.step(held, pressed, false, 0, 0.0, rng, out);
}

fn shots(out: &[WeaponAction]) -> usize {
    out.iter().filter(|x| matches!(x, WeaponAction::Fire { pellet: 0, .. } | WeaponAction::Launch { pellet: 0, .. })).count()
}

/// Whether `got` is within one logic frame (plus `slack` frames) of `want` seconds.
fn near(got: usize, want: f32, slack: usize) -> bool {
    let want = want / LOGIC_DT;
    (got as f32 - want).abs() <= 1.01 + slack as f32
}

/// Every way `def` departs from its BO2 definition in the simulation (empty if none).
pub fn check(def: &'static WeaponDef) -> Vec<String> {
    let mut fails = Vec::new();
    let mut fail = |s: String| fails.push(format!("{}: {s}", def.id));
    let mut rng = Rng::new(7);

    if def.class.is_gun() {
        if def.clip == 0 || def.fire_time <= 0.0 {
            fail(format!("no clip ({}) or fire time ({})", def.clip, def.fire_time));
            return fails;
        }
        // Damage curve: BO2's damage at each used point (a point sharing its range with a later one
        // is a step: the later damage applies beyond it), and the last point's damage beyond.
        // (Launched weapons hit with their projectile, not this curve.)
        let bullets = def.class != Class::Launcher && def.projectile_speed <= 0.0;
        let used: Vec<usize> = (0..6).filter(|&i| i == 0 || def.damage_range[i] > 0.0).collect();
        for &i in used.iter().filter(|_| bullets) {
            let r = def.damage_range[i];
            if used.iter().any(|&j| j != i && (def.damage_range[j] - r).abs() < 1e-3) {
                continue;
            }
            if (def.damage_at(r) - def.damage[i]).abs() > 1e-3 {
                fail(format!("damage {} at range point {i}, BO2 says {}", def.damage_at(r), def.damage[i]));
            }
        }
        let last = *used.last().unwrap();
        if bullets && (def.damage_at(def.damage_range[last] + 100.0) - def.damage[last]).abs() > 1e-3 {
            fail(format!("damage beyond the curve {}, BO2 says {}", def.damage_at(def.damage_range[last] + 100.0), def.damage[last]));
        }

        // Fire mode and rate, over one magazine.
        let mut a = arsenal(def);
        let mut out = Vec::new();
        let spin = frames(def.spin_up_time);
        match def.fire {
            FireMode::Auto | FireMode::Minigun => {
                // Hold: the magazine empties at BO2's fire time.
                let mut first = None;
                let mut last = 0;
                for f in 0..frames(def.fire_time * def.clip as f32 + def.spin_up_time + 1.0) {
                    let before = shots(&out);
                    step(&mut a, buttons::FIRE, if f == 0 { buttons::FIRE } else { 0 }, &mut out, &mut rng);
                    if out.contains(&WeaponAction::ReloadStarted) {
                        // Count one magazine: stop when it reloads by itself.
                        if shots(&out) > before {
                            last = f;
                        }
                        break;
                    }
                    if shots(&out) > before {
                        first.get_or_insert(f);
                        last = f;
                    }
                }
                let fired = shots(&out);
                if fired != def.clip as usize {
                    fail(format!("held fire emptied {fired} of {} rounds", def.clip));
                }
                if def.fire == FireMode::Minigun && first.unwrap_or(0) + 1 < spin {
                    fail(format!("minigun fired before spinning up ({first:?} < {spin} frames)"));
                }
                if fired > 2 {
                    let span = last - first.unwrap_or(0);
                    if !near(span, def.fire_time * (fired - 1) as f32, fired / 10) {
                        fail(format!("{fired} rounds took {span} frames, BO2 says {:.1}", def.fire_time * (fired - 1) as f32 / LOGIC_DT));
                    }
                }
            }
            FireMode::Semi => {
                // Holding fires once.
                for f in 0..frames(1.0) {
                    step(&mut a, buttons::FIRE, if f == 0 { buttons::FIRE } else { 0 }, &mut out, &mut rng);
                }
                if shots(&out) != 1 {
                    fail(format!("semi-auto fired {} rounds on one press", shots(&out)));
                }
                // Mashing fires no faster than the fire time (one magazine, no reload).
                let mut a = arsenal(def);
                a.weapons[0].reserve = 0;
                let mut out = Vec::new();
                let n = frames(def.fire_time * 3.0 + 0.01);
                for _ in 0..n {
                    step(&mut a, buttons::FIRE, buttons::FIRE, &mut out, &mut rng);
                }
                let max = (n as f32 * LOGIC_DT / def.fire_time).floor() as usize + 1;
                if shots(&out) > max.min(def.clip as usize) {
                    fail(format!("mashing fired {} rounds in {n} frames, at most {max}", shots(&out)));
                }
            }
            FireMode::Burst(k) => {
                for f in 0..frames(def.fire_time * k as f32 + 0.5) {
                    step(&mut a, buttons::FIRE, if f == 0 { buttons::FIRE } else { 0 }, &mut out, &mut rng);
                }
                let want = (k as usize).min(def.clip as usize);
                if shots(&out) != want {
                    fail(format!("one press fired {} rounds, BO2 bursts {want}", shots(&out)));
                }
            }
        }
        // Pellets (or bolts) per shot. A weapon faster than a logic frame fires several shots in
        // one; count per shot.
        let mut a = arsenal(def);
        let mut out = Vec::new();
        step(&mut a, buttons::FIRE, buttons::FIRE, &mut out, &mut rng);
        let projectiles = out.iter().filter(|x| matches!(x, WeaponAction::Fire { .. } | WeaponAction::Launch { .. })).count();
        let per_shot = projectiles / shots(&out).max(1);
        if per_shot != def.pellets.max(1) as usize {
            fail(format!("{per_shot} pellets per shot, BO2 has {}", def.pellets));
        }

        // Reloads (when there is more than the loaded round).
        if def.clip > 1 {
            for empty in [false, true] {
                let mut a = arsenal(def);
                a.weapons[0].ammo = if empty { 0 } else { 1 };
                let mut out = Vec::new();
                step(&mut a, 0, buttons::RELOAD, &mut out, &mut rng);
                if !a.current().is_reloading() {
                    fail(format!("reload ({}) did not start", if empty { "empty" } else { "partial" }));
                    continue;
                }
                let start = a.current().ammo;
                let (mut added_at, mut done_at) = (None, None);
                let longest = def.reload_start_time + def.reload_end_time + def.reload_time.max(def.reload_empty_time) * (def.clip as f32 + 2.0);
                for f in 1..frames(longest + 2.0) {
                    step(&mut a, 0, 0, &mut out, &mut rng);
                    if added_at.is_none() && a.current().ammo > start {
                        added_at = Some(f);
                    }
                    if !a.current().is_reloading() {
                        done_at = Some(f);
                        break;
                    }
                }
                let Some(done) = done_at else {
                    fail(format!("reload ({}) never finished: {:?}, {} rounds, reload {} start {} end {} add {}", if empty { "empty" } else { "partial" }, a.current().reload, a.current().ammo, def.reload_time, def.reload_start_time, def.reload_end_time, def.reload_ammo_add));
                    continue;
                };
                if def.shell_by_shell() {
                    let shells = (def.clip - start) as f32 / def.reload_ammo_add.max(1) as f32;
                    let want = def.reload_start_time + shells.ceil() * def.reload_time + def.reload_end_time;
                    if !near(done, want, shells.ceil() as usize) {
                        fail(format!("shell reload took {done} frames, BO2 says {:.1}", want / LOGIC_DT));
                    }
                } else {
                    let (length, add) = if empty { (def.reload_empty_time, def.reload_empty_add_time) } else { (def.reload_time, def.reload_add_time) };
                    let add = if add > 0.0 { add.min(length) } else { length };
                    if !near(done, length, 0) {
                        fail(format!("{} reload took {done} frames, BO2 says {:.1}", if empty { "empty" } else { "partial" }, length / LOGIC_DT));
                    }
                    if !added_at.is_some_and(|f| near(f, add, 0)) {
                        fail(format!("rounds went in at frame {added_at:?}, BO2 says {:.1}", add / LOGIC_DT));
                    }
                }
                if a.current().ammo != def.clip {
                    fail(format!("reload left {} of {} rounds", a.current().ammo, def.clip));
                }
            }
        }

        // ADS time.
        let mut a = arsenal(def);
        let mut out = Vec::new();
        let mut f = 0;
        while a.ads < 1.0 && f < 300 {
            step(&mut a, buttons::ADS, 0, &mut out, &mut rng);
            f += 1;
        }
        if !near(f, def.ads_in_time, 0) {
            fail(format!("aiming took {f} frames, BO2 says {:.1}", def.ads_in_time / LOGIC_DT));
        }
    }

    if def.class.is_offhand() {
        let mut a = Arsenal::new();
        a.loadout.perks.clear();
        a.raise_left = 0.0;
        a.lethal = Offhand { def, count: 1 };
        let mut out = Vec::new();
        match def.place {
            // Equipment set down: planted after the pull-back, and one used.
            Place::Plant => {
                for f in 0..frames(OFFHAND_PULLBACK) {
                    a.step(buttons::LETHAL, if f == 0 { buttons::LETHAL } else { 0 }, false, 0, 0.0, &mut rng, &mut out);
                }
                a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
                if !out.iter().any(|x| matches!(x, WeaponAction::Plant { .. })) {
                    fail("never planted".into());
                }
                if a.lethal.count != 0 {
                    fail("planting used none".into());
                }
            }
            // The Black Hat: up while held, put away on release without using a charge.
            Place::Hack => {
                for f in 0..frames(1.0) {
                    a.step(buttons::LETHAL, if f == 0 { buttons::LETHAL } else { 0 }, false, 0, 0.0, &mut rng, &mut out);
                }
                if a.hacking().is_none() {
                    fail("not held up".into());
                }
                for _ in 0..frames(1.0) {
                    a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
                }
                if a.offhand.is_some() || a.lethal.count != 1 {
                    fail("letting go of the Black Hat used it or left it up".into());
                }
            }
            // Thrown after the pull-back; a cooking grenade's fuse runs from the pin.
            Place::Throw => {
                let hold = OFFHAND_PULLBACK.max(if def.cook { (def.fuse_time * 0.5).min(1.0) } else { 0.0 });
                for f in 0..frames(hold) {
                    a.step(buttons::LETHAL, if f == 0 { buttons::LETHAL } else { 0 }, false, 0, 0.0, &mut rng, &mut out);
                }
                for _ in 0..frames(0.2) {
                    a.step(0, 0, false, 0, 0.0, &mut rng, &mut out);
                }
                match out.iter().find_map(|x| if let WeaponAction::Throw { fuse_left, .. } = x { Some(*fuse_left) } else { None }) {
                    None => fail("never thrown".into()),
                    Some(left) => {
                        let want = if def.cook { def.fuse_time - frames(hold) as f32 * LOGIC_DT } else { def.fuse_time };
                        if (left - want).abs() > LOGIC_DT * 1.5 {
                            fail(format!("fuse {left:.2} s left, BO2 says {want:.2}"));
                        }
                    }
                }
            }
        }
        if def.explosion_radius > 0.0 && def.explosion_damage_at(0.0) < def.explosion_damage_at(def.explosion_radius) {
            fail("explosion grows with distance".into());
        }
    }

    if def.class == Class::Melee {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        a.melee = def;
        let mut out = Vec::new();
        a.step(0, buttons::MELEE, false, 0, 0.0, &mut rng, &mut out);
        if !out.iter().any(|x| matches!(x, WeaponAction::Melee { damage } if (*damage - def.melee_damage).abs() < 1e-3)) {
            fail(format!("melee did not deal {}", def.melee_damage));
        }
    }
    fails
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::catalog;

    #[test]
    fn built_in_weapons_play_by_their_definitions() {
        let fails: Vec<String> = catalog().all().iter().flat_map(|d| check(d)).collect();
        assert!(fails.is_empty(), "{fails:#?}");
    }
}
