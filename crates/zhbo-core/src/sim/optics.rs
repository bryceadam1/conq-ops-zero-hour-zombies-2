/*
** Conquer Ops - Zero Hour gameplay core: the optic displays' rules: the millimetre scanner's
** characters and the target finder's "enemy spotted".
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! What BO2's code feeds two optics' lens displays, decided in the logic step so a replay draws
//! the same thing (the renderer only draws it: `nativevm` reads [`Optics`] through the client).
//!
//! * **The MMS** (`mmsWeapon`): the characters on the scanner and how strongly each shows
//!   (`t6mp` `FUN_004e7b40`, the per-character sonar pass). Teammates are hidden
//!   (`cg_sonarAttachmentHideFriendlies`); a character's visibility is `1 −` the mean, over the
//!   last `cg_sonarAttachmentSpeedDelay` seconds, of its speed over a threshold that grows as it
//!   comes closer (`cg_sonarAttachmentMaxSpeed + cg_sonarAttachmentDistanceFactor · (1 − d /
//!   mmsMaxDist)`, BO2's speeds per 60 Hz frame), floored at `cg_sonarAttachmentSpeedTimeRatio`;
//!   beyond `mmsMaxDist` a sample counts as full speed. Stationary enemies show, moving ones fade
//!   (BO2's description: "lingering heat signatures from stationary enemies").
//! * **The target finder** (`FUN_00528ec0` / `FUN_00529d80` / `FUN_00525ad0`): only fully aimed,
//!   a trace along the view (8192"); an enemy under the crosshair, within
//!   `cg_rangeFinderMinEnemySpottingDist` feet (−1: no limit), is spotted, and from the next frame
//!   on the reticle takes its alt colour (`weaponParam0.z`). Its display (`FUN_00526780`, drawn
//!   into the code's ui3d texture only above 0.99 ADS) marks every enemy player with a diamond
//!   centred `RangeFinderMarkHeight` above his feet (BO2's 70 − 25", standing): the marks are
//!   [`Optics::blips`] then.

use super::fire::{gather_targets, target_from};
use crate::hitscan::{self, HitResult};
use crate::host::{objflags, LogicHost, ObjInfo};
use crate::math::Vec3;
use crate::units::{inches, LOGIC_DT, LOGIC_FPS};

/// The most characters the scanner shows at once (the renderer's `NATIVE_MAX_BLIPS`).
pub const MAX_BLIPS: usize = 8;

/// One character on the scanner this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blip {
    pub id: u32,
    /// Where it stands, and its capsule (world units).
    pub pos: Vec3,
    pub radius: f32,
    pub height: f32,
    /// BO2's per-character `weaponParam0.x`: 0 hidden .. 1 fully shown.
    pub intensity: f32,
}

/// A character the scanner is following: where it was and its recent speed fractions.
#[derive(Clone, Debug, Default, PartialEq)]
struct Track {
    id: u32,
    last: Vec3,
    /// Speed over the threshold, per logic tick, newest last (at most the window's length).
    fractions: Vec<f32>,
}

/// A player's optic displays' state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Optics {
    tracks: Vec<Track>,
    /// What the lens display shows this tick, nearest first: the scanner's characters with an MMS
    /// in hand, the target finder's marked enemies (a point: `pos` the diamond's centre, no
    /// capsule) with one fully aimed; empty otherwise.
    pub blips: Vec<Blip>,
    /// The target finder: the range under the crosshair in feet (BO2's display digits), when an
    /// enemy is spotted.
    pub spotted_range: Option<i32>,
    /// The tick the current spotting started (BO2's `cg + 0x84430`).
    spotted_since: Option<u32>,
    /// `weaponParam0.z`: 1 once an enemy has been spotted for longer than the tick it started.
    pub alt: f32,
}

/// BO2's frame for its "units/frame" speeds: the sonar's ring holds `SpeedDelay × 60` samples
/// (`0x00b128e4`).
fn bo2_frames_per_tick(t: &crate::tuning::Tuning) -> f32 {
    t.sonar_frames_per_second * LOGIC_DT
}

impl Optics {
    /// Update both displays for the weapon in hand. `eye`/`dir` are the view; `ads` BO2's
    /// `fWeaponPosFrac`.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, host: &dyn LogicHost, me: u32, eye: Vec3, dir: Vec3, ads: f32, def: &crate::weapons::WeaponDef, frame: u32) {
        let t = crate::tuning::get();
        if def.traits.mms && def.mms_max_dist > 0.0 {
            self.step_scanner(host, me, eye, def.mms_max_dist, &t);
        } else {
            self.tracks.clear();
            self.blips.clear();
        }
        self.step_finder(host, me, eye, dir, ads, def.target_finder, frame, &t);
        if def.target_finder && !def.traits.mms {
            self.blips = if ads >= t.range_finder_display_ads { marks(host, me, eye, &t) } else { Vec::new() };
        }
    }

    fn step_scanner(&mut self, host: &dyn LogicHost, me: u32, eye: Vec3, max_dist: f32, t: &crate::tuning::Tuning) {
        let mut objs = Vec::new();
        host.query(eye, inches(max_dist), me, &mut objs);
        let hide_friendlies = t.sonar_hide_friendlies != 0.0;
        let mut seen: Vec<&ObjInfo> = objs
            .iter()
            .filter(|o| o.id != me && o.has(objflags::INFANTRY) && !o.has(objflags::DEAD))
            .filter(|o| !(hide_friendlies && o.has(objflags::ALLY)))
            .collect();
        seen.sort_by_key(|o| o.id);
        let window = ((t.sonar_speed_delay * LOGIC_FPS) as usize).max(1);
        let per_frame = bo2_frames_per_tick(t).max(1e-6);
        let mut tracks = Vec::with_capacity(seen.len());
        let mut blips = Vec::new();
        for o in seen {
            let mut track = self.tracks.iter().position(|k| k.id == o.id).map(|i| self.tracks.swap_remove(i)).unwrap_or(Track { id: o.id, last: o.pos, fractions: Vec::new() });
            // Inches, and inches per BO2 frame.
            let dist = (o.pos - eye).length() / inches(1.0);
            let near = (dist / max_dist).min(1.0);
            let threshold = t.sonar_max_speed + t.sonar_distance_factor * (1.0 - near);
            let speed = if near < 1.0 { (o.pos - track.last).length() / inches(1.0) / per_frame } else { threshold };
            track.last = o.pos;
            track.fractions.push(speed / threshold.max(1e-6));
            if track.fractions.len() > window {
                track.fractions.remove(0);
            }
            // BO2 divides by the whole window: samples it has not taken yet count as still.
            let mean = track.fractions.iter().sum::<f32>() / window as f32;
            let visibility = (1.0 - mean).clamp(0.0, 1.0).max(t.sonar_speed_time_ratio);
            if visibility > 0.0 {
                let shape = target_from(o);
                let (radius, height) = match shape.shape {
                    hitscan::Shape::Cylinder { radius, height } => (radius, height),
                    _ => (o.major_radius, o.height),
                };
                blips.push((dist, Blip { id: o.id, pos: o.pos, radius, height, intensity: visibility }));
            }
            tracks.push(track);
        }
        blips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
        self.blips = blips.into_iter().take(MAX_BLIPS).map(|(_, b)| b).collect();
        self.tracks = tracks;
    }

    #[allow(clippy::too_many_arguments)]
    fn step_finder(&mut self, host: &dyn LogicHost, me: u32, eye: Vec3, dir: Vec3, ads: f32, finder: bool, frame: u32, t: &crate::tuning::Tuning) {
        // BO2 traces only at full ADS (`cg + 0x48288 == 1.0`).
        let range = if finder && ads >= 1.0 { spotted(host, me, eye, dir, t) } else { None };
        let limit = t.range_finder_min_enemy_spotting_dist;
        let within = range.is_some_and(|r| limit < 0.0 || (r as f32) < limit);
        if within {
            self.spotted_since.get_or_insert(frame);
        } else {
            self.spotted_since = None;
        }
        self.spotted_range = range.filter(|_| within);
        self.alt = if self.spotted_since.is_some_and(|s| frame > s) { 1.0 } else { 0.0 };
    }
}

/// The target finder's diamonds: every live enemy soldier in reach (BO2 marks every enemy player
/// it draws), at his chest, the nearest [`MAX_BLIPS`].
fn marks(host: &dyn LogicHost, me: u32, eye: Vec3, t: &crate::tuning::Tuning) -> Vec<Blip> {
    let mut objs = Vec::new();
    host.query(eye, inches(t.range_finder_trace_dist), me, &mut objs);
    let mut out: Vec<(f32, Blip)> = objs
        .iter()
        .filter(|o| o.id != me && o.has(objflags::ENEMY) && o.has(objflags::INFANTRY) && !o.has(objflags::DEAD))
        .map(|o| {
            let pos = o.pos + Vec3::new(0.0, 0.0, inches(t.range_finder_mark_height));
            ((pos - eye).length(), Blip { id: o.id, pos, radius: 0.0, height: 0.0, intensity: 1.0 })
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    out.into_iter().take(MAX_BLIPS).map(|(_, b)| b).collect()
}

/// The range (feet) to an enemy under the crosshair, if the trace hits one.
fn spotted(host: &dyn LogicHost, me: u32, eye: Vec3, dir: Vec3, t: &crate::tuning::Tuning) -> Option<i32> {
    let reach = inches(t.range_finder_trace_dist);
    let targets = gather_targets(host, eye, reach, me);
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    let HitResult::Object { id, distance, .. } = hitscan::trace(eye, dir, reach, &targets, me, &ground) else { return None };
    let o = host.object(id, me)?;
    let enemy = o.has(objflags::ENEMY) && !o.has(objflags::DEAD) && (o.has(objflags::INFANTRY) || o.has(objflags::VEHICLE));
    // Feet: BO2's `|hit − eye| · (1/12)`, truncated (`_DAT_00c73718`).
    enemy.then(|| (distance / inches(1.0) / 12.0) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, soldier, FakeHost};

    fn mms_def() -> crate::weapons::WeaponDef {
        crate::weapons::WeaponDef { traits: crate::weapons::Traits { mms: true, ..Default::default() }, mms_max_dist: 1000.0, ..Default::default() }
    }

    /// Player 2's soldier; the viewer (object 1) is player 1's General.
    fn enemy(id: u32, x: f32) -> ObjInfo {
        soldier(id, 2, x)
    }

    fn world() -> FakeHost {
        let host = FakeHost::default();
        host.add(general(1, 1));
        host
    }

    /// A still enemy shows fully; one running at BO2's run speed fades nearly off the scanner over
    /// the window; a teammate never shows; nothing shows without an MMS in hand.
    #[test]
    fn the_scanner_shows_still_enemies_and_fades_movers() {
        let host = world();
        host.add(enemy(10, 50.0));
        host.add(enemy(11, 60.0));
        host.add(soldier(12, 1, 40.0));
        let mut o = Optics::default();
        let def = mms_def();
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let run = inches(crate::tuning::get().g_speed) * LOGIC_DT;
        for f in 0..40 {
            if let Some(w) = host.objects.borrow_mut().iter_mut().find(|o| o.id == 11) {
                w.pos.x = 60.0 + run * f as f32;
            }
            o.step(&host, 1, eye, Vec3::new(1.0, 0.0, 0.0), 0.0, &def, f);
        }
        let still = o.blips.iter().find(|b| b.id == 10).expect("the still enemy is on the scanner");
        assert_eq!(still.intensity, 1.0);
        // At 15 m a runner (190"/s, 3.2" a frame) is near the threshold there (2.2 + 3 x 0.4):
        // barely a trace left.
        let runner = o.blips.iter().find(|b| b.id == 11).map_or(0.0, |b| b.intensity);
        assert!(runner < 0.2, "the runner faded: {:?}", o.blips);
        assert!(o.blips.iter().all(|b| b.id != 12), "teammates are hidden");
        o.step(&host, 1, eye, Vec3::new(1.0, 0.0, 0.0), 0.0, &crate::weapons::WeaponDef::default(), 41);
        assert!(o.blips.is_empty(), "no MMS, no scanner");
    }

    /// The target finder spots an enemy under the crosshair only fully aimed, and the reticle
    /// turns from the tick after the spotting starts.
    #[test]
    fn the_target_finder_spots_an_enemy_under_the_crosshair_at_full_ads() {
        let host = world();
        host.add(enemy(10, 60.0));
        let def = crate::weapons::WeaponDef { target_finder: true, ..Default::default() };
        let mut o = Optics::default();
        let eye = Vec3::new(0.0, 0.0, 6.0);
        let along = Vec3::new(1.0, 0.0, 0.0);
        o.step(&host, 1, eye, along, 0.9, &def, 1);
        assert_eq!((o.spotted_range, o.alt), (None, 0.0), "not fully aimed");
        o.step(&host, 1, eye, along, 1.0, &def, 2);
        assert!(o.spotted_range.is_some() && o.alt == 0.0, "spotted, the colour waits a tick");
        o.step(&host, 1, eye, along, 1.0, &def, 3);
        assert_eq!(o.alt, 1.0);
        // 60 units at 6" each is 360" = 30 ft, less the hit cylinder's radius.
        assert!((27..=30).contains(&o.spotted_range.unwrap()), "{:?}", o.spotted_range);
        assert_eq!(o.blips.len(), 1, "the enemy is marked on the display");
        assert!((o.blips[0].pos.z - inches(45.0)).abs() < 1e-4, "at his chest");
        o.step(&host, 1, eye, Vec3::new(0.0, 1.0, 0.0), 1.0, &def, 4);
        assert_eq!((o.spotted_range, o.alt), (None, 0.0), "looked away");
        assert_eq!(o.blips.len(), 1, "still marked: the display marks every enemy, not just the aimed one");
        o.step(&host, 1, eye, along, 0.5, &def, 5);
        assert!(o.blips.is_empty(), "the display draws only fully aimed");
    }
}
