//! The hip-fire gate over the player's own install: every converted bullet gun of the weapon table
//! (every base weapon, attachment combination and dual-wield pair) fires through the real
//! [`zhbo_core::weapons::Arsenal`] in each state BO2's spread rules tell apart, and each shot's
//! spread and each pellet's place in the cone are held against BO2's rules written out here on
//! their own, from the engine (Ghidra, t6mp):
//!
//! * the cone (`FUN_008e41a0`): the stance's min..max (`fHipSpread*`, already × the attachments'
//!   `fHipSpreadMin/MaxScale` in the table);
//! * the scale (`ps->aimSpreadScale`, 0..255): per frame `FUN_008e0080` — grow by
//!   `(move + air) × dt × 255` when there is something to add, else decay by
//!   `fHipSpreadDecayRate × stance scale × dt × 255`, zeroed fully aimed (`adsZeroSpread`) and by a
//!   weapon with no decay rate; per shot `+ fHipSpreadFireAdd × 255` from the hip (`FUN_008dc710`);
//!   crawling opens it (`FUN_008e4da0`);
//! * the shot (`FUN_0066e590`): `min + (max − min) · s` from the hip, `fAdsSpread + (max −
//!   fAdsSpread) · s` fully aimed, `s` the scale the previous frame left;
//! * the pellet (`Bullet_Fire`, `FUN_005d0c20`): an angle uniform in 0..360° and a radius `r`
//!   uniform in 0..1 on the tangent plane, `tan(offset) = r · tan(spread)`.
//!
//! The weapon values themselves are BO2's: the conformance gate holds every one of them
//! (`hip_spread`, `hip_spread_decay`, `hip_spread_fire_add`, `hip_spread_move_add`,
//! `hip_spread_turn_add`, `hip_spread_stance_decay`, `ads_spread`) to the install's own weapon files.

#[cfg(test)]
mod tests {
    use zhbo_core::input::buttons;
    use zhbo_core::math::Vec3;
    use zhbo_core::tuning::Tuning;
    use zhbo_core::units::{LOGIC_DT, ZH_PER_INCH};
    use zhbo_core::weapons::aim_spread::{cone_dir, SpreadIn};
    use zhbo_core::weapons::{catalog, Arsenal, FireMode, Loadout, Rng, WeaponAction, WeaponDef, WeaponState};

    /// One of the states BO2's rules tell apart.
    #[derive(Clone, Copy, Debug)]
    struct State {
        name: &'static str,
        stance: usize,
        moving: bool,
        airborne: bool,
        aimed: bool,
        /// Frames between trigger pulls: 0 holds the trigger (a string of fire), else each pull is
        /// a first shot from rest.
        tap_gap: usize,
    }

    const STATES: &[State] = &[
        State { name: "first shot from rest", stance: 0, moving: false, airborne: false, aimed: false, tap_gap: 45 },
        State { name: "standing string", stance: 0, moving: false, airborne: false, aimed: false, tap_gap: 0 },
        State { name: "crouched string", stance: 1, moving: false, airborne: false, aimed: false, tap_gap: 0 },
        State { name: "prone string", stance: 2, moving: false, airborne: false, aimed: false, tap_gap: 0 },
        State { name: "running", stance: 0, moving: true, airborne: false, aimed: false, tap_gap: 0 },
        State { name: "crawling", stance: 2, moving: true, airborne: false, aimed: false, tap_gap: 0 },
        State { name: "airborne", stance: 0, moving: false, airborne: true, aimed: false, tap_gap: 0 },
        State { name: "aimed", stance: 0, moving: false, airborne: false, aimed: true, tap_gap: 0 },
    ];

    /// BO2's rules over one state, frame by frame: the scale, and what the shots of a frame fly
    /// with.
    struct Bo2 {
        scale: f32,
    }

    impl Bo2 {
        fn cone(def: &WeaponDef, stance: usize) -> (f32, f32) {
            def.hip_spread[stance]
        }

        /// The spreads of a frame's shots (degrees; `hands` says which hand fired each, `true` the
        /// left), then the frame's update and adds. Every shot of a frame flies with the scale the
        /// last frame left (`currentAimSpreadScale`); a hand's later shot in the same frame stands
        /// for BO2's next command and has its earlier shot's add. Both guns of a pair fire on one
        /// command, so the left gun's first shot has none of the right's.
        fn frame(&mut self, def: &WeaponDef, s: &State, ads_before: f32, ads_at_fire: f32, hands: &[bool], t: &Tuning) -> Vec<f32> {
            let (min, max) = Self::cone(def, s.stance);
            let n = hands.len();
            let mut shots = Vec::new();
            let mut at = [self.scale; 2];
            for &left in hands {
                let at = &mut at[left as usize];
                let k = (*at / 255.0).clamp(0.0, 1.0);
                shots.push(if ads_at_fire >= 1.0 { def.ads_spread + (max - def.ads_spread) * k } else { min + (max - min) * k });
                if ads_at_fire < 1.0 {
                    *at = (*at + def.spread_fire_add * 255.0).min(255.0);
                }
            }
            // FUN_008e0080 on the scale the last frame left.
            let dt = LOGIC_DT;
            if def.spread_decay == 0.0 {
                self.scale = 0.0;
            } else {
                let decay = def.spread_decay
                    * if s.airborne {
                        t.aim_spread_air_decay_scale
                    } else {
                        [1.0, def.spread_stance_decay.0, def.spread_stance_decay.1][s.stance]
                    };
                let mut add = 0.0;
                if ads_before >= 1.0 {
                    self.scale = 0.0;
                } else {
                    let speed = t.g_speed;
                    if s.moving && def.spread_move_add != 0.0 && speed > t.bg_aim_spread_move_speed_threshold {
                        add += speed * def.spread_move_add / t.g_speed;
                    }
                    if s.airborne {
                        add += t.aim_spread_air_add;
                    }
                }
                self.scale = if add * dt > 0.0 { self.scale + add * dt * 255.0 } else { self.scale - decay * dt * 255.0 }.clamp(0.0, 255.0);
            }
            // The weapon frame: crawling opens it; then the shots' adds.
            if s.stance == 2 && s.moving && ads_before.max(ads_at_fire) < 1.0 {
                self.scale = 255.0;
            }
            if ads_at_fire < 1.0 {
                for _ in 0..n {
                    self.scale = (self.scale + def.spread_fire_add * 255.0).min(255.0);
                }
            }
            shots
        }
    }

    /// A pellet's place: its radius as a share of the cone's tangent (`r`, BO2's uniform draw)
    /// and its angle around the aim (radians).
    fn place(dir: Vec3, yaw: f32, pitch: f32, spread_deg: f32) -> (f32, f32) {
        let f = zhbo_core::math::dir_from_angles(yaw, pitch);
        let (_, right) = zhbo_core::math::basis_xy(yaw);
        let up = right.cross(f);
        let along = dir.dot(f);
        let (x, y) = (dir.dot(right) / along, dir.dot(up) / along);
        let r = (x * x + y * y).sqrt() / spread_deg.to_radians().tan().max(1e-9);
        (r, y.atan2(x))
    }

    fn arsenal(def: &'static WeaponDef) -> Arsenal {
        let mut a = Arsenal::with_loadout(Loadout::unarmed());
        a.weapons = vec![WeaponState::new(def)];
        a.active = 0;
        a.raise_left = 0.0;
        a.drop_left = 0.0;
        a
    }

    /// The guns BO2 fires bullets from (a cone of `Bullet_Fire` pellets): not a launcher, a stack,
    /// the shield or the knives.
    fn bullet_gun(d: &WeaponDef) -> bool {
        d.class.is_gun() && !d.launches() && d.fire != FireMode::Stacked && d.hip_spread.iter().any(|p| p.1 > 0.0)
    }

    /// Fire one gun in one state for `frames` frames; returns (shots checked, pellets' places).
    fn run(def: &'static WeaponDef, s: &State, frames: usize, rng: &mut Rng, places: &mut Vec<(f32, f32)>) -> Result<usize, String> {
        let t = Tuning::default();
        let mut a = arsenal(def);
        let dual = a.current().left.is_some();
        if s.aimed && (dual || !def.aims) {
            return Ok(0);
        }
        let mut bo2 = Bo2 { scale: 0.0 };
        let (yaw, pitch) = (0.4f32, 0.1f32);
        let speed = if s.moving { t.g_speed * ZH_PER_INCH } else { 0.0 };
        // Up on the sights first (the scale zeroes there), then the string.
        let aim = if s.aimed { buttons::ADS } else { 0 };
        let lead = if s.aimed { (def.ads_in_time / LOGIC_DT).ceil() as usize + 2 } else { 0 };
        let mut checked = 0;
        for f in 0..lead + frames {
            let trigger = f >= lead && (s.tap_gap == 0 || (f - lead) % s.tap_gap == 0);
            let fire = if trigger { if dual { buttons::FIRE | buttons::ADS } else { buttons::FIRE } } else { 0 };
            for w in a.weapons.iter_mut() {
                w.ammo = w.def.clip.max(1);
                if let Some(h) = w.left.as_mut() {
                    h.ammo = w.def.clip.max(1);
                }
            }
            a.spread_in = SpreadIn { view: (yaw, pitch), moving: s.moving, run_speed: t.g_speed, airborne: s.airborne, ..Default::default() };
            let ads_before = a.ads;
            let mut out = Vec::new();
            a.step(aim | fire, 0, false, s.stance, speed, rng, &mut out);
            let fires: Vec<(f32, (f32, f32), u16, bool)> = out
                .iter()
                .filter_map(|x| match x {
                    WeaponAction::Fire { spread_deg, jitter, pellet, left, .. } => Some((*spread_deg, *jitter, *pellet, *left)),
                    _ => None,
                })
                .collect();
            let hands: Vec<bool> = fires.iter().filter(|x| x.2 == 0).map(|x| x.3).collect();
            let want = bo2.frame(def, s, ads_before, a.ads, &hands, &t);
            let mut shot = 0usize;
            for (spread, (u0, u1), pellet, _) in fires {
                if pellet == 0 {
                    shot += 1;
                }
                let k = shot.saturating_sub(1);
                let w = want[k];
                if (spread - w).abs() > 1e-3 + w * 1e-4 {
                    return Err(format!("{} {}: frame {f} shot {k}: spread {spread:.4}° != BO2's {w:.4}°", def.id, s.name));
                }
                if spread > 1e-3 {
                    let dir = cone_dir(yaw, pitch, spread, u0, u1);
                    places.push(place(dir, yaw, pitch, spread));
                }
                if pellet == 0 {
                    checked += 1;
                }
            }
        }
        Ok(checked)
    }

    /// The pellets' places against BO2's law: `r` uniform in 0..1 (a Kolmogorov-Smirnov bound),
    /// the angle uniform (its mean vector near 0).
    fn check_law(what: &str, places: &[(f32, f32)]) -> Result<(), String> {
        let n = places.len();
        if n < 200 {
            return Ok(());
        }
        let mut r: Vec<f32> = places.iter().map(|p| p.0).collect();
        r.sort_by(f32::total_cmp);
        if r[n - 1] > 1.0 + 1e-3 {
            return Err(format!("{what}: a pellet left its cone (r = {})", r[n - 1]));
        }
        let ks = r.iter().enumerate().map(|(i, x)| ((i + 1) as f32 / n as f32 - x).abs().max((x - i as f32 / n as f32).abs())).fold(0.0f32, f32::max);
        let bound = 1.95 / (n as f32).sqrt();
        if ks > bound {
            return Err(format!("{what}: radius law KS {ks:.4} > {bound:.4} over {n} pellets (BO2's radius is uniform; mean r {:.3}, BO2's 0.5)", r.iter().sum::<f32>() / n as f32));
        }
        let (c, s) = places.iter().fold((0.0f32, 0.0f32), |a, p| (a.0 + p.1.cos(), a.1 + p.1.sin()));
        let m = (c * c + s * s).sqrt() / n as f32;
        if m > 4.0 / (n as f32).sqrt() {
            return Err(format!("{what}: the angle leans ({m:.4})"));
        }
        Ok(())
    }

    /// Every converted bullet gun, in every state: each shot's spread is BO2's and the pellets fall
    /// by BO2's law. `--nocapture` prints a line per state.
    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR); converts the weapon table and fires the roster (release)"]
    fn the_install_roster_hip_fires_as_bo2() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        crate::test_catalog::install_shipped(&bo2_dir);
        let guns: Vec<&'static WeaponDef> = catalog().all().iter().copied().filter(|d| bullet_gun(d)).collect();
        assert!(guns.len() > 300, "the roster with its combinations: {}", guns.len());
        let mut failures = Vec::new();
        let mut rng = Rng::new(0x5eed_5ead);
        for s in STATES {
            let (mut shots, mut places) = (0, Vec::new());
            for &def in &guns {
                let mut own = Vec::new();
                match run(def, s, 60, &mut rng, &mut own) {
                    Ok(n) => shots += n,
                    Err(e) => failures.push(e),
                }
                places.extend(own);
            }
            if let Err(e) = check_law(s.name, &places) {
                failures.push(e);
            }
            println!("{:<22} {} guns, {shots} shots, {} pellets", s.name, guns.len(), places.len());
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}
