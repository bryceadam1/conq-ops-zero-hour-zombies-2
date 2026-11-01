/*
** Conquer Ops - Zero Hour gameplay core: BO2's hit locations and how a trace ranks them.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's `hitLocation_t` and the player trace's ranking of them (t6mp `FUN_007837a0`).
//!
//! A bullet's trace against a player tests every hit box of the posed body
//! ([`crate::hitscan::Shape::Body`]) and keeps, of the boxes the ray crosses, the one whose
//! location ranks highest, the nearest among equals. The ranking is the engine's own table, passed
//! by the bullet trace (`FUN_005cf2f0`): a rifle bullet (`bRifleBullet`) ranks the head first, then
//! down the body to the feet; any other bullet ranks every body location the same, so the nearest
//! box wins. A location ranked under 2 (none, the gun) is never hit. The damage is then multiplied
//! by the weapon's [`crate::weapons::WeaponDef::location_multipliers`] for the location
//! (`FUN_005f05c0`).

use crate::math::Vec3;
use crate::units::inches;
use zhbo_native::thirdperson::state;

/// `hitLocation_t` count (`HITLOC_NUM`): the names at t6mp `0x00c8bc90`.
pub const COUNT: usize = 21;

pub const NONE: u8 = 0;
pub const HELMET: u8 = 1;
pub const HEAD: u8 = 2;
pub const NECK: u8 = 3;
pub const TORSO_UPPER: u8 = 4;
pub const TORSO_MID: u8 = 5;
pub const TORSO_LOWER: u8 = 6;
pub const RIGHT_LEG_UPPER: u8 = 13;
pub const LEFT_FOOT: u8 = 18;
pub const GUN: u8 = 19;
pub const RIOTSHIELD: u8 = 20;

/// BO2's location names, in `hitLocation_t` order (t6mp `0x00c8bc90`).
pub const NAMES: [&str; COUNT] = [
    "none", "helmet", "head", "neck", "torso_upper", "torso_mid", "torso_lower", "right_arm_upper", "left_arm_upper",
    "right_arm_lower", "left_arm_lower", "right_hand", "left_hand", "right_leg_upper", "left_leg_upper", "right_leg_lower",
    "left_leg_lower", "right_foot", "left_foot", "gun", "riotshield",
];

/// The trace's rank of each location for a non-rifle bullet (t6mp `0x00c8bce4`).
const RANK: [u8; COUNT] = [1, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 101];
/// The same for a rifle bullet (t6mp `0x00c8bcfc`): head first.
const RANK_RIFLE: [u8; COUNT] = [1, 11, 11, 11, 10, 9, 8, 7, 7, 6, 6, 5, 5, 4, 4, 4, 4, 3, 3, 0, 101];
/// The least `XBoneInfo.radiusSquared` (sq in) of a bone the trace tests (t6mp `0x00ca6094`).
pub const MIN_RADIUS_SQUARED: f32 = 4.0;
/// The lowest rank a box needs to be hit at all (`FUN_007837a0` starts its best at 2).
pub const MIN_RANK: u8 = 2;
/// Ranks of 101 and up are the riot shield's collision-map path, which the mod does not carry.
const SHIELD_RANK: u8 = 101;

/// How the trace ranks `location` for a rifle bullet or not; `None` when a box of that location is
/// never hit (none, the gun, the riot shield).
pub fn rank(location: u8, rifle: bool) -> Option<u8> {
    let table = if rifle { &RANK_RIFLE } else { &RANK };
    table.get(location as usize).copied().filter(|r| (MIN_RANK..SHIELD_RANK).contains(r))
}

/// A head hit: BO2's headshot is the head or the helmet.
pub fn is_head(location: u8) -> bool {
    location == HEAD || location == HELMET
}

/// A location by its BO2 name.
pub fn parse(name: &str) -> Option<u8> {
    NAMES.iter().position(|n| *n == name).map(|i| i as u8)
}

/// One hit box of the soldier's body as the converter wrote it: the bone's `XBoneInfo` bounds in
/// the bone's frame, the bone's model-space transform in the pose of one body state, and its
/// location (`partClassification`). BO2 inches, model space (+X forward, +Z up, feet at the origin).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitBox {
    /// The body state this pose is (`zhbo_native::thirdperson::state`): the first frame of that
    /// state's body clip.
    pub state: u16,
    pub location: u8,
    pub bone: String,
    /// The bone's model-space rotation (x, y, z, w) and position.
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

impl HitBox {
    /// The weapon table's
    /// `HitBox = <state> <location> <bone> <rotation 4> <position 3> <mins 3> <maxs 3>`.
    pub fn format(&self) -> String {
        let n = |v: &[f32]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ");
        format!("{} {} {} {} {} {} {}", self.state, NAMES[self.location as usize], self.bone, n(&self.rotation), n(&self.position), n(&self.mins), n(&self.maxs))
    }

    pub fn parse(value: &str) -> Result<HitBox, String> {
        let w: Vec<&str> = value.split_whitespace().collect();
        if w.len() != 16 {
            return Err(format!("hit box needs a body state, a location, a bone and 13 numbers: {value}"));
        }
        let state = w[0].parse::<u16>().map_err(|e| format!("body state {}: {e}", w[0]))?;
        let location = parse(w[1]).ok_or_else(|| format!("unknown hit location {}", w[1]))?;
        let f: Vec<f32> = w[3..].iter().map(|x| x.parse::<f32>().map_err(|e| format!("{x}: {e}"))).collect::<Result<_, _>>()?;
        Ok(HitBox { state, location, bone: w[2].to_string(), rotation: [f[0], f[1], f[2], f[3]], position: [f[4], f[5], f[6]], mins: [f[7], f[8], f[9]], maxs: [f[10], f[11], f[12]] })
    }
}

/// A box ready to trace: Zero Hour units, in the soldier's frame (yaw 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PosedBox {
    pub location: u8,
    pub origin: Vec3,
    /// The bone's axes in the soldier's frame (rows).
    pub axes: [Vec3; 3],
    pub mins: Vec3,
    pub maxs: Vec3,
}

/// The soldier's hit boxes in one pose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body {
    pub boxes: Vec<PosedBox>,
}

impl Body {
    pub fn new(boxes: &[HitBox]) -> Body {
        let v = |a: [f32; 3]| Vec3::new(inches(a[0]), inches(a[1]), inches(a[2]));
        let boxes = boxes
            .iter()
            .map(|b| {
                let [x, y, z, w] = b.rotation;
                // The rotation matrix's columns are the bone's axes in model space.
                let ax = Vec3::new(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w));
                let ay = Vec3::new(2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w));
                let az = Vec3::new(2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y));
                PosedBox { location: b.location, origin: v(b.position), axes: [ax, ay, az], mins: v(b.mins), maxs: v(b.maxs) }
            })
            .collect();
        Body { boxes }
    }

    /// BO2's pick along a ray in the soldier's frame (`FUN_007837a0`): of the boxes the ray enters
    /// within `max_t`, the one whose location ranks highest, the nearest among equals. Returns the
    /// distance and the location.
    pub fn pick(&self, o: Vec3, d: Vec3, max_t: f32, rifle: bool) -> Option<(f32, u8)> {
        let mut best: Option<(u8, f32, u8)> = None;
        for b in &self.boxes {
            let Some(r) = rank(b.location, rifle) else { continue };
            let Some(t) = b.entry(o, d).filter(|t| *t <= max_t) else { continue };
            if best.is_none_or(|(br, bt, _)| r > br || (r == br && t < bt)) {
                best = Some((r, t, b.location));
            }
        }
        best.map(|(_, t, l)| (t, l))
    }

    /// Whether `p` (soldier frame) is inside any box.
    pub fn contains(&self, p: Vec3) -> bool {
        self.boxes.iter().any(|b| {
            let l = b.local(p);
            (b.mins.x..=b.maxs.x).contains(&l.x) && (b.mins.y..=b.maxs.y).contains(&l.y) && (b.mins.z..=b.maxs.z).contains(&l.z)
        })
    }
}

/// The soldier's hit boxes in every body state the converter posed. BO2 traces a player's DObj as
/// the server animated it (`FUN_006f2910` -> `FUN_00649b50`, G_DObjCalcPose: the entity's
/// controller, then the skeleton from its animation tree — stance, gait, ADS); the mod poses the
/// boxes from the sim's own deterministic body state ([`crate::nativebody::state_of`], the same
/// state the third-person body draws), at the first frame of that state's clip. Zero Hour's
/// infantry have no BO2 animation state and stand ([`state::STAND_IDLE`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poses {
    by_state: Vec<(u16, Body)>,
}

impl Poses {
    /// Group the table's boxes by body state.
    pub fn new(boxes: &[HitBox]) -> Poses {
        let mut states: Vec<u16> = boxes.iter().map(|b| b.state).collect();
        states.sort_unstable();
        states.dedup();
        let by_state = states
            .into_iter()
            .map(|s| {
                let set: Vec<HitBox> = boxes.iter().filter(|b| b.state == s).cloned().collect();
                (s, Body::new(&set))
            })
            .collect();
        Poses { by_state }
    }

    /// One pose for every state (tests, and a table posed only standing).
    pub fn single(body: Body) -> Poses {
        Poses { by_state: vec![(state::STAND_IDLE, body)] }
    }

    /// The boxes for body state `s`: its own pose, else the nearest the table has (the same
    /// fallbacks the body clips use), else any.
    pub fn get(&self, s: u16) -> Option<&Body> {
        let find = |s: u16| self.by_state.iter().find(|(k, _)| *k == s).map(|(_, b)| b);
        state::fallbacks(s).find_map(find).or_else(|| self.by_state.first().map(|(_, b)| b))
    }

    /// The standing pose (Zero Hour's infantry).
    pub fn stand(&self) -> Option<&Body> {
        self.get(state::STAND_IDLE)
    }

    pub fn is_empty(&self) -> bool {
        self.by_state.is_empty()
    }
}

impl PosedBox {
    fn local(&self, p: Vec3) -> Vec3 {
        let r = p - self.origin;
        Vec3::new(r.dot(self.axes[0]), r.dot(self.axes[1]), r.dot(self.axes[2]))
    }

    /// Where a ray from `o` along `d` (soldier frame) enters the box (0 when it starts inside).
    pub fn entry(&self, o: Vec3, d: Vec3) -> Option<f32> {
        let lo = self.local(o);
        let ld = Vec3::new(d.dot(self.axes[0]), d.dot(self.axes[1]), d.dot(self.axes[2]));
        crate::hitscan::ray_aabb(lo, ld, self.mins, self.maxs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rifle_ranks_the_head_over_the_body_and_others_rank_them_alike() {
        assert!(rank(HEAD, true) > rank(TORSO_UPPER, true));
        assert!(rank(TORSO_UPPER, true) > rank(RIGHT_LEG_UPPER, true));
        assert_eq!(rank(HEAD, false), rank(LEFT_FOOT, false));
        for never in [NONE, GUN, RIOTSHIELD] {
            assert_eq!(rank(never, true), None);
            assert_eq!(rank(never, false), None);
        }
        assert_eq!(parse("torso_mid"), Some(TORSO_MID));
        assert_eq!(NAMES[NECK as usize], "neck");
    }

    fn cube(location: u8, x: f32, half: f32) -> HitBox {
        HitBox { state: state::STAND_IDLE, location, bone: "b".into(), rotation: [0.0, 0.0, 0.0, 1.0], position: [x, 0.0, 40.0], mins: [-half; 3], maxs: [half; 3] }
    }

    #[test]
    fn a_rifle_picks_the_head_behind_a_hand_and_others_the_nearest() {
        // A hand in front of the head along +X (inches): the rifle's rank takes the head.
        let body = Body::new(&[cube(11, -10.0, 2.0), cube(HEAD, 0.0, 4.0)]);
        let o = Vec3::new(inches(-40.0), 0.0, inches(40.0));
        let x = Vec3::new(1.0, 0.0, 0.0);
        let (t, loc) = body.pick(o, x, 1000.0, true).unwrap();
        assert_eq!(loc, HEAD);
        assert!((t - inches(36.0)).abs() < 1e-4, "{t}");
        assert_eq!(body.pick(o, x, 1000.0, false).unwrap().1, 11, "any other bullet: the nearest");
        // A head past the trace's end does not count.
        assert_eq!(body.pick(o, x, inches(33.0), true).unwrap().1, 11);
        assert!(body.contains(Vec3::new(0.0, 0.0, inches(42.0))));
        assert!(!body.contains(Vec3::new(0.0, 0.0, inches(50.0))));
    }

    #[test]
    fn a_box_turns_with_its_bone_and_the_table_line_round_trips() {
        // A 20 x 2 x 2 box along the bone's X, the bone turned 90 degrees about Z: it lies along Y.
        let h = (0.5f32).sqrt();
        let b = HitBox { state: state::PRONE_IDLE, location: TORSO_MID, bone: "back_mid".into(), rotation: [0.0, 0.0, h, h], position: [0.0, 0.0, 0.0], mins: [-10.0, -1.0, -1.0], maxs: [10.0, 1.0, 1.0] };
        let body = Body::new(std::slice::from_ref(&b));
        assert!(body.contains(Vec3::new(0.0, inches(9.0), 0.0)));
        assert!(!body.contains(Vec3::new(inches(9.0), 0.0, 0.0)));
        assert_eq!(HitBox::parse(&b.format()).unwrap(), b);
    }

    #[test]
    fn a_state_without_its_own_pose_takes_the_nearest_stance() {
        let at = |s: u16, z: f32| HitBox { state: s, position: [0.0, 0.0, z], ..cube(HEAD, 0.0, 2.0) };
        let poses = Poses::new(&[at(state::STAND_IDLE, 60.0), at(state::CROUCH_IDLE, 40.0), at(state::PRONE_IDLE, 8.0)]);
        let z = |s: u16| poses.get(s).unwrap().boxes[0].origin.z / inches(1.0);
        assert!((z(state::CROUCH_WALK) - 40.0).abs() < 1e-3, "a crouch walk crouches");
        assert!((z(state::PRONE_CRAWL) - 8.0).abs() < 1e-3);
        assert!((z(state::STAND_SPRINT) - 60.0).abs() < 1e-3);
        assert!((poses.stand().unwrap().boxes[0].origin.z / inches(1.0) - 60.0).abs() < 1e-3);
        assert!(Poses::default().get(state::STAND_IDLE).is_none());
    }
}
