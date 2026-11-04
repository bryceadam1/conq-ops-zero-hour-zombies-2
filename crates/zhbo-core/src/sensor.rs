//! The Sensor Grenade's reveal: what its thrower's compass shows. BO2 does it on the client, for
//! the thrower alone (both paths below test the local client against the sensor's owner):
//!
//! * **The scan** (`CG_Missile` `FUN_004bcac0` → `FUN_004bc570`, while the sensor exists): every
//!   800 ms from the throw it sweeps for 400 ms, reaching the enemies within 1000 inches one by
//!   one (`400 / n` ms apart, in client order) and, where its line of sight to the enemy's chest
//!   is clear, pings him on the thrower's compass (`FUN_004ab320` without its flag →
//!   `FUN_004a95f0`): his position then, at most once per `compassStaticImageUpdateTime`, fading
//!   over `compassSatellitePingFadeTime`.
//! * **The detonation** (event 0x4e, raised by `G_SensorGrenadeDetonate` `FUN_005f26c0` as the
//!   sensor goes off; `CG_EntityEvent` `FUN_004c1590`): every enemy within its explosion radius in
//!   clear sight is put on the thrower's compass for 6 s (`FUN_004ab320` with its flag: the
//!   actor's deadline `time + 6000`), where he is.
//!
//! The mod decides both in the sim (the sight lines, who and when), so every machine agrees and
//! the marks are hashed; the minimap only draws them ([`crate::hud`]). The scan's runner and
//! targeting effects are per-viewer presentation BO2 colours by team; the mod does not play them.

use crate::hitscan::{self, HitResult};
use crate::host::{objflags, LogicHost, ObjInfo};
use crate::math::Vec3;
use crate::units::{inches, LOGIC_DT};

/// The sensor numbers from the tuning pack, in Zero Hour units and seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorTuning {
    pub cycle: f32,
    pub sweep: f32,
    pub range: f32,
    pub target_height: f32,
    pub ping_time: f32,
    pub static_update: f32,
    pub ping_fade: f32,
}

impl SensorTuning {
    pub fn from_tuning(t: &crate::tuning::Tuning) -> Self {
        SensorTuning {
            cycle: (t.sensor_scan_cycle_ms / 1000.0).max(LOGIC_DT),
            sweep: (t.sensor_scan_sweep_ms / 1000.0).max(LOGIC_DT),
            range: inches(t.sensor_scan_range),
            target_height: inches(t.sensor_scan_target_height),
            ping_time: t.sensor_ping_time_ms / 1000.0,
            static_update: t.compass_static_image_update_ms / 1000.0,
            ping_fade: t.compass_satellite_ping_fade_time,
        }
    }
}

impl Default for SensorTuning {
    fn default() -> Self {
        SensorTuning::from_tuning(&crate::tuning::Tuning::default())
    }
}

/// An enemy on a thrower's compass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    /// The thrower's seat.
    pub owner: usize,
    /// The enemy's object.
    pub target: u32,
    /// Where he was when pinged (a scan ping stays there; a detonation mark follows him).
    pub pos: Vec3,
    /// Seconds since it was made.
    pub age: f32,
    /// The detonation's mark (shown where he is, for `ping_time`); else a scan ping (shown where
    /// he was, fading over `ping_fade`).
    pub live: bool,
}

crate::statehash::state_hash!(struct Mark { owner, target, pos, age, live });

impl Mark {
    /// How strongly the minimap draws it: 1 for a detonation mark, a scan ping's fade.
    pub fn alpha(&self, t: &SensorTuning) -> f32 {
        if self.live { 1.0 } else { (1.0 - self.age / t.ping_fade.max(1e-3)).clamp(0.0, 1.0) }
    }

    fn expired(&self, t: &SensorTuning) -> bool {
        // Ages are summed frame by frame: within a thousandth of the limit is at it.
        self.age >= (if self.live { t.ping_time } else { t.ping_fade }) - 1e-3
    }
}

/// Whether `o` is someone the sensor reveals: an enemy soldier or ground vehicle, alive.
fn revealed(o: &ObjInfo) -> bool {
    o.has(objflags::ENEMY) && !o.has(objflags::DEAD) && !o.has(objflags::AIRBORNE) && (o.has(objflags::INFANTRY) || o.has(objflags::VEHICLE))
}

/// The enemies within `range` of `at` (3D, as BO2 measures it), by object id (BO2's client order).
fn enemies_within(host: &dyn LogicHost, viewer: u32, at: Vec3, range: f32) -> Vec<ObjInfo> {
    let mut objs = Vec::new();
    host.query(at, range, viewer, &mut objs);
    objs.retain(|o| revealed(o) && (o.pos - at).length() < range);
    objs.sort_by_key(|o| o.id);
    objs
}

/// How far above the ground a lying sensor's line of sight starts (Zero Hour units): the
/// grenade's own body.
const SIGHT_LIFT: f32 = inches(4.0);

/// BO2's sight test (`FUN_004bc570`, the 0x4e case): a trace from the sensor to the enemy's chest
/// that reaches him or nothing on the way (terrain and every other solid stop it).
fn in_sight(host: &dyn LogicHost, viewer: u32, from: Vec3, o: &ObjInfo, t: &SensorTuning) -> bool {
    // A sensor lies on the ground: its line leaves from just above it, not from inside the terrain.
    let from = Vec3::new(from.x, from.y, from.z.max(host.ground_height(from.x, from.y)) + SIGHT_LIFT);
    let chest = Vec3::new(o.pos.x, o.pos.y, o.pos.z + t.target_height.min(o.height.max(0.0)));
    let to = chest - from;
    let len = to.length();
    if len < 1e-3 {
        return true;
    }
    let mut objs = Vec::new();
    host.query(from + to * 0.5, len * 0.5 + 1.0, viewer, &mut objs);
    objs.retain(|x| x.id != viewer && !x.has(objflags::DEAD) && !x.has(objflags::NO_COLLIDE));
    objs.sort_by_key(|x| x.id);
    let targets: Vec<_> = objs.iter().map(hitscan::solid).collect();
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    // A solid the line starts inside (someone standing on the sensor) does not stop it, as a
    // trace leaving a brush does not.
    match hitscan::trace_where(from, to * (1.0 / len), len, &targets, viewer, &ground, &|t| !hitscan::contains(t, from)) {
        HitResult::Object { id, .. } => id == o.id,
        HitResult::Terrain { .. } => false,
        HitResult::Nothing { .. } => true,
    }
}

/// Every mark, stepped once per logic frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub marks: Vec<Mark>,
}

crate::statehash::state_hash!(struct Marks { marks });

impl Marks {
    /// Age every mark and drop the expired ones.
    pub fn step(&mut self, t: &SensorTuning) {
        for m in &mut self.marks {
            m.age += LOGIC_DT;
        }
        self.marks.retain(|m| !m.expired(t));
    }

    /// `owner`'s marks.
    pub fn of(&self, owner: usize) -> impl Iterator<Item = &Mark> {
        self.marks.iter().filter(move |m| m.owner == owner)
    }

    /// One scan step of a sensor `since` seconds after its throw (this frame's), at `at`: the
    /// enemy whose turn in the sweep came this frame is pinged if in sight.
    #[allow(clippy::too_many_arguments)]
    pub fn scan(&mut self, host: &dyn LogicHost, viewer: u32, owner: usize, at: Vec3, since: f32, t: &SensorTuning) {
        let now = since.rem_euclid(t.cycle);
        if now >= t.sweep {
            return;
        }
        let prev_since = since - LOGIC_DT;
        let same_cycle = prev_since > 0.0 && (prev_since / t.cycle).floor() == (since / t.cycle).floor();
        let enemies = enemies_within(host, viewer, at, t.range);
        let Some(last) = enemies.len().checked_sub(1) else { return };
        let slot = t.sweep / enemies.len() as f32;
        let k = ((now / slot) as usize).min(last);
        // `FUN_004bc570` pings when a new period starts or the sweep reached the next enemy.
        let reached = !same_cycle || ((prev_since.rem_euclid(t.cycle) / slot) as usize) < k;
        if reached && in_sight(host, viewer, at, &enemies[k], t) {
            self.ping(owner, &enemies[k], t);
        }
    }

    /// A scan ping (`FUN_004a95f0`): his position now, unless he was pinged too recently.
    fn ping(&mut self, owner: usize, o: &ObjInfo, t: &SensorTuning) {
        match self.marks.iter_mut().find(|m| m.owner == owner && m.target == o.id && !m.live) {
            Some(m) if m.age < t.static_update => {}
            Some(m) => {
                m.pos = o.pos;
                m.age = 0.0;
            }
            None => self.marks.push(Mark { owner, target: o.id, pos: o.pos, age: 0.0, live: false }),
        }
    }

    /// The detonation's ping (event 0x4e): every enemy within `radius` of `at` in sight goes on
    /// `owner`'s compass for `ping_time`.
    pub fn detonated(&mut self, host: &dyn LogicHost, viewer: u32, owner: usize, at: Vec3, radius: f32, t: &SensorTuning) {
        for o in enemies_within(host, viewer, at, radius) {
            if !in_sight(host, viewer, at, &o, t) {
                continue;
            }
            match self.marks.iter_mut().find(|m| m.owner == owner && m.target == o.id && m.live) {
                Some(m) => {
                    m.pos = o.pos;
                    m.age = 0.0;
                }
                None => self.marks.push(Mark { owner, target: o.id, pos: o.pos, age: 0.0, live: true }),
            }
        }
    }

    /// Forget `owner`'s marks (he left the match).
    pub fn clear_owner(&mut self, owner: usize) {
        self.marks.retain(|m| m.owner != owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, soldier, FakeHost};

    /// The thrower's General 1 (player 0) with the sensor at the origin, and `enemies` of player 1
    /// at the given distances (Zero Hour units), each on his own bearing (none in another's way).
    fn scene(enemies: &[(u32, f32)]) -> FakeHost {
        let host = FakeHost::default();
        host.add(general(1, 0));
        for (k, &(id, d)) in enemies.iter().enumerate() {
            let a = k as f32 * 0.9;
            host.add(ObjInfo { pos: Vec3::new(d * a.cos(), d * a.sin(), 0.0), ..soldier(id, 1, 0.0) });
        }
        host
    }

    const AT: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    fn frames(seconds: f32) -> usize {
        (seconds / LOGIC_DT).round() as usize
    }

    /// Run the scan from the throw for `seconds`, logging (frame, target) of each new ping.
    fn run(host: &FakeHost, marks: &mut Marks, t: &SensorTuning, seconds: f32) -> Vec<(usize, u32)> {
        let mut pinged = Vec::new();
        for f in 1..=frames(seconds) {
            marks.step(t);
            let before: Vec<(u32, f32)> = marks.marks.iter().map(|m| (m.target, m.age)).collect();
            marks.scan(host, 1, 0, AT, f as f32 * LOGIC_DT, t);
            for m in &marks.marks {
                if m.age == 0.0 && !before.iter().any(|&(id, age)| id == m.target && age == 0.0) {
                    pinged.push((f, m.target));
                }
            }
        }
        pinged
    }

    /// BO2's scan (`FUN_004bc570`): every 800 ms it reaches the enemies within 1000 inches one by
    /// one across its first 400 ms (`400 / n` apart, in order), pings each for the thrower where he
    /// stands, never one out of range or a friend, and pings the same enemy again only after
    /// `compassStaticImageUpdateTime`; a ping fades over `compassSatellitePingFadeTime`.
    #[test]
    fn the_scan_reaches_the_enemies_in_range_one_by_one_every_period() {
        let t = SensorTuning::default();
        let host = scene(&[(7, 20.0), (8, 40.0), (9, t.range + 20.0)]);
        host.add(soldier(10, 0, 30.0)); // a friend
        let mut marks = Marks::default();
        let pinged = run(&host, &mut marks, &t, 0.8);
        // Two enemies in range: 0.2 s apart in the first 0.4 s, by id.
        assert_eq!(pinged.iter().map(|p| p.1).collect::<Vec<_>>(), vec![7, 8], "{pinged:?}");
        // The first the frame after the throw, the second where the sweep reaches him (200 ms).
        assert_eq!((pinged[0].0, pinged[1].0), (1, frames(t.sweep / 2.0)), "{pinged:?}");
        assert!(marks.marks.iter().all(|m| !m.live && m.owner == 0));
        // Within the 4 s the pings stay where they were made; then each is made again.
        let pinged = run_from(&host, &mut marks, &t, 0.8, t.static_update - 0.8 - LOGIC_DT);
        assert!(pinged.is_empty(), "{pinged:?}");
        let pinged = run_from(&host, &mut marks, &t, t.static_update, 1.0);
        assert_eq!(pinged.iter().map(|p| p.1).collect::<Vec<_>>(), vec![7, 8], "{pinged:?}");
        // A ping fades out and is gone after the fade time.
        let mut m = Mark { owner: 0, target: 7, pos: Vec3::ZERO, age: t.ping_fade * 0.5, live: false };
        assert!((m.alpha(&t) - 0.5).abs() < 1e-4);
        m.age = t.ping_fade;
        assert!(m.expired(&t));
    }

    fn run_from(host: &FakeHost, marks: &mut Marks, t: &SensorTuning, from: f32, seconds: f32) -> Vec<(usize, u32)> {
        let mut pinged = Vec::new();
        let start = frames(from);
        for f in start + 1..=start + frames(seconds) {
            marks.step(t);
            let before: Vec<(u32, f32)> = marks.marks.iter().map(|m| (m.target, m.age)).collect();
            marks.scan(host, 1, 0, AT, f as f32 * LOGIC_DT, t);
            for m in &marks.marks {
                if m.age == 0.0 && !before.iter().any(|&(id, age)| id == m.target && age == 0.0) {
                    pinged.push((f, m.target));
                }
            }
        }
        pinged
    }

    /// The scan's line of sight: a hillside between the sensor and the enemy hides him.
    #[test]
    fn the_scan_needs_a_clear_line_to_the_enemy() {
        let t = SensorTuning::default();
        let host = scene(&[(7, 60.0)]);
        host.hill.set(Some((25.0, 2.0)));
        let mut marks = Marks::default();
        assert!(run(&host, &mut marks, &t, 0.8).is_empty());
        host.hill.set(None);
        assert_eq!(run(&host, &mut marks, &t, 0.8).len(), 1);
    }

    /// The detonation (event 0x4e): every enemy within the blast radius in sight is marked live
    /// on the thrower's compass for 6 s (`FUN_004ab320`: `time + 6000`); one beyond it is not.
    #[test]
    fn the_detonation_marks_the_enemies_in_its_blast_for_six_seconds() {
        let t = SensorTuning::default();
        let radius = 50.0;
        let host = scene(&[(7, 20.0), (8, 45.0), (9, 80.0)]);
        let mut marks = Marks::default();
        marks.detonated(&host, 1, 0, AT, radius, &t);
        let mut ids: Vec<u32> = marks.of(0).filter(|m| m.live).map(|m| m.target).collect();
        ids.sort();
        assert_eq!(ids, vec![7, 8]);
        assert!(marks.of(1).next().is_none(), "the thrower's alone");
        for _ in 0..frames(t.ping_time) - 1 {
            marks.step(&t);
        }
        assert_eq!(marks.marks.len(), 2, "still up just before 6 s");
        marks.step(&t);
        assert!(marks.marks.is_empty(), "gone at 6 s");
    }
}
