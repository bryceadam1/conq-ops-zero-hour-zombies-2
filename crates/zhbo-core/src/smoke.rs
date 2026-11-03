//! The smoke screen: BO2's smoke grenade (`willy_pete_mp`) as sim state, and the sight it blocks.
//!
//! BO2 (`_smokegrenade.gsc`, `watchsmokegrenadedetonation`): where the grenade goes off the script
//! spawns a timed effect entity for 12 seconds (`spawntimedfx( "smoke_center_mp", position, up, 12 )`;
//! t6mp's `spawntimedfx`, `FUN_00623ed0`, frees it at `time + 12000`) and makes it a visibility
//! blocker (`blocksight`: `fxblocksight( ent, 64 )`, then every 0.75 s
//! `clamp( radius * 1.5, 10, 150 )`). The engine keeps the blockers in a table of entity and radius
//! (`SV_FX_SetVisBlocker`, `FUN_006e3020`) and its sight test (`FUN_006e2f90`) says two points see
//! each other when they are closer than 80 inches, else when the segment between them passes no
//! blocker within its radius of the entity's origin (`FUN_006e2e10`: the segment's closest point).
//! What asks it in t6mp: the turrets' target sight (`FUN_0067d3a0` from `FUN_0067d490`), a lock-on
//! think (`FUN_0064f580`), an unreferenced sight helper (`FUN_005d7cd0`) and `bullettracepassed`'s
//! optional sixth argument, which no MP script passes — the trophy, sensor, Black Hat and claymore
//! traces do not consult it, and nor does the compass. So the screen blocks the sight of what
//! BO2's engine aims by itself, which the mod's Zero Hour units are: they neither acquire nor keep
//! a target the line to which passes through it (host patch 0024, `WeaponSet`), and path around it
//! while it stands. First-person players see the cloud (presentation); their rounds pass, as in
//! BO2.
//!
//! The numbers are the tuning pack's (`[Smoke]`); the screen's state is its centre and the logic
//! frame it went off on, both lockstep, so every peer's engine asks the same question and gets the
//! same answer.
use crate::math::Vec3;
use crate::tuning::Tuning;
use crate::units::{inches, LOGIC_DT};

/// A smoke screen standing in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Smoke {
    /// Where the grenade went off (Zero Hour units): the blocker's centre.
    pub at: Vec3,
    /// The logic frame it went off on.
    pub born: u32,
}

crate::statehash::state_hash!(struct Smoke { at, born });

/// The tuning pack's `[Smoke]` values in the sim's units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Logic frames the screen stands.
    pub frames: u32,
    /// The blocker's first radius, its growth per step, the step (logic frames), its floor and cap
    /// (Zero Hour units).
    pub radius_start: f32,
    pub growth: f32,
    pub step_frames: f32,
    pub radius_min: f32,
    pub radius_max: f32,
    /// Closer than this, two points always see each other (Zero Hour units).
    pub min_distance: f32,
}

impl Params {
    pub fn from_tuning(t: &Tuning) -> Self {
        Params {
            frames: (t.smoke_duration / LOGIC_DT).round().max(0.0) as u32,
            radius_start: inches(t.smoke_sight_radius_start),
            growth: t.smoke_sight_radius_growth,
            step_frames: (t.smoke_sight_radius_interval / LOGIC_DT).max(f32::MIN_POSITIVE),
            radius_min: inches(t.smoke_sight_radius_min),
            radius_max: inches(t.smoke_sight_radius_max),
            min_distance: inches(t.smoke_sight_min_distance),
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Params::from_tuning(&Tuning::default())
    }
}

impl Smoke {
    /// Logic frames since it went off.
    pub fn age(&self, frame: u32) -> u32 {
        frame.saturating_sub(self.born)
    }

    /// Whether it still stands on `frame`.
    pub fn stands(&self, frame: u32, p: &Params) -> bool {
        self.age(frame) < p.frames
    }

    /// The blocker's radius on `frame`: `blocksight`'s start, grown once per elapsed step and
    /// clamped each time (the script's loop, step by step).
    pub fn radius(&self, frame: u32, p: &Params) -> f32 {
        let steps = (self.age(frame) as f32 / p.step_frames).floor() as u32;
        let mut r = p.radius_start;
        for _ in 0..steps.min(64) {
            r = (r * p.growth).clamp(p.radius_min, p.radius_max);
        }
        r
    }
}

/// Whether a smoke grenade going off on `surface` raises its screen: BO2's script returns before
/// the timed effect and the blocker when the grenade went off in water (`level.water_duds`, unset
/// in MP, counts as on; `_smokegrenade.gsc:25-29`).
pub fn raised_on(surface: crate::impacts::Surface) -> bool {
    surface != crate::impacts::Surface::Water
}

/// The squared distance from `c` to the segment `a`–`b` (BO2's `FUN_006e2e10`: the closest point
/// of the segment, its ends included).
pub fn segment_distance_sq(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let ab = b - a;
    let len_sq = ab.dot(ab);
    let t = if len_sq > 0.0 { ((c - a).dot(ab) / len_sq).clamp(0.0, 1.0) } else { 0.0 };
    let d = a + ab * t - c;
    d.dot(d)
}

/// A screen as the engine's questions see it: centre, radius now, the frame it went off on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blocker {
    pub at: Vec3,
    pub radius: f32,
    pub born: u32,
}

/// BO2's sight test (`FUN_006e2f90`): whether the line from `a` to `b` is blocked by a screen.
pub fn blocks_sight(blockers: &[Blocker], min_distance: f32, a: Vec3, b: Vec3) -> bool {
    let ab = b - a;
    if ab.dot(ab) < min_distance * min_distance {
        return false;
    }
    blockers.iter().any(|s| segment_distance_sq(a, b, s.at) < s.radius * s.radius)
}

/// Whether the ground point `(x, y)` is under a screen (its radius, seen from above): the
/// pathfinder's extra cost there.
pub fn covers(blockers: &[Blocker], x: f32, y: f32) -> bool {
    blockers.iter().any(|s| {
        let (dx, dy) = (x - s.at.x, y - s.at.y);
        dx * dx + dy * dy < s.radius * s.radius
    })
}

/// Whether the ground segment `(ax, ay)`–`(bx, by)` passes under a screen that went off after
/// `since` (a path computed before it stood): the move repaths once around it.
pub fn crosses_since(blockers: &[Blocker], ax: f32, ay: f32, bx: f32, by: f32, since: u32) -> bool {
    let flat = |s: &Blocker| Vec3::new(s.at.x, s.at.y, 0.0);
    blockers.iter().filter(|s| s.born > since).any(|s| segment_distance_sq(Vec3::new(ax, ay, 0.0), Vec3::new(bx, by, 0.0), flat(s)) < s.radius * s.radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params::default()
    }

    #[test]
    fn it_stands_for_the_scripts_twelve_seconds() {
        let p = params();
        assert_eq!(p.frames, 360);
        let s = Smoke { at: Vec3::ZERO, born: 100 };
        assert!(s.stands(100, &p) && s.stands(459, &p));
        assert!(!s.stands(460, &p));
    }

    #[test]
    fn the_blocker_grows_as_blocksight_does() {
        // 64, then x1.5 every 0.75 s (22.5 frames), capped at 150 inches.
        let p = params();
        let s = Smoke { at: Vec3::ZERO, born: 0 };
        let r = |f| s.radius(f, &p) / inches(1.0);
        assert!((r(0) - 64.0).abs() < 1e-3 && (r(22) - 64.0).abs() < 1e-3);
        assert!((r(23) - 96.0).abs() < 1e-3);
        assert!((r(45) - 144.0).abs() < 1e-3);
        assert!((r(68) - 150.0).abs() < 1e-3 && (r(359) - 150.0).abs() < 1e-3);
    }

    #[test]
    fn a_line_through_the_screen_is_blocked_and_one_past_it_is_not() {
        let p = params();
        let s = [Blocker { at: Vec3::ZERO, radius: p.radius_max, born: 0 }];
        let far = inches(600.0);
        // Straight through, from outside to outside.
        assert!(blocks_sight(&s, p.min_distance, Vec3::new(-far, 0.0, 0.0), Vec3::new(far, 0.0, 0.0)));
        // Passing beside it, outside the radius.
        let side = inches(160.0);
        assert!(!blocks_sight(&s, p.min_distance, Vec3::new(-far, side, 0.0), Vec3::new(far, side, 0.0)));
        // Over it (the blocker is a sphere around the grenade).
        assert!(!blocks_sight(&s, p.min_distance, Vec3::new(-far, 0.0, inches(200.0)), Vec3::new(far, 0.0, inches(200.0))));
        // Into it: a target inside is hidden from outside, and sees nothing out.
        assert!(blocks_sight(&s, p.min_distance, Vec3::new(-far, 0.0, 0.0), Vec3::new(inches(20.0), 0.0, 0.0)));
        // Ending short of it.
        assert!(!blocks_sight(&s, p.min_distance, Vec3::new(-far, 0.0, 0.0), Vec3::new(-inches(200.0), 0.0, 0.0)));
        // Within 80 inches of each other, always seen (even inside).
        assert!(!blocks_sight(&s, p.min_distance, Vec3::new(-inches(30.0), 0.0, 0.0), Vec3::new(inches(30.0), 0.0, 0.0)));
    }

    #[test]
    fn the_ground_under_it_and_paths_through_it() {
        let s = [Blocker { at: Vec3::new(100.0, 100.0, 5.0), radius: 25.0, born: 50 }];
        assert!(covers(&s, 110.0, 90.0) && !covers(&s, 130.0, 100.0));
        // A path computed before it went off crosses it; one computed after does not repath.
        assert!(crosses_since(&s, 0.0, 100.0, 200.0, 100.0, 49));
        assert!(!crosses_since(&s, 0.0, 100.0, 200.0, 100.0, 50));
        assert!(!crosses_since(&s, 0.0, 0.0, 200.0, 0.0, 0), "beside it");
    }
}
