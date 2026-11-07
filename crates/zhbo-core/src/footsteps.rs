//! BO2's bob cycle and footsteps (`PM_Footsteps`, t6mp `FUN_008c4820`).
//!
//! **The cycle.** `ps->bobCycle` is a byte the move advances each frame it runs on the ground with
//! a movement key held: `msec × (xyspeed / top) × factor`, where `top` is the fastest the keys,
//! stance, weapon and sprint allow (`FUN_008c4690`, [`crate::movement::bob_top_speed`]) and the
//! factor comes from the engine's table at `0x00bd265c` by stance and walking (a second half for
//! running backwards) or, sprinting upright, `player_sprintCameraBob` (and the weapon's
//! `fSprintCycleScale`, `FUN_008c1d00`). Below one inch a second the cycle resets (`FUN_008c1a20`);
//! in the air and with no movement key it holds. The weapon bob, the view bob and the footsteps all
//! read it (G4's `FUN_008f0160`/`FUN_008f0ac0`), so walking and stepping keep one cadence.
//!
//! **The steps.** A footstep plays when the cycle crosses a multiple of 64 into its "loud" quarter
//! (`FUN_008c16c0` with `FUN_008c1760`: `(cycle & 0x7f) > 0x3f` at `player_footstepsThreshhold`
//! or faster): two per cycle. Which sound is the movement's ([`Step`], `FUN_008c0a10`) on the
//! surface under the feet (`FUN_008c08e0`), from BO2's footstep tables
//! (`zhbo_formats::footstep`).
//!
//! The cycle is sim state (the gun's bob angles an aimed shot follows read it, `viewmodel::Motion`);
//! the steps are presentation, read off it by the client.

use crate::movement::Stance;
use crate::tuning::Tuning;

/// A footstep's movement: the footstep table's columns (`FUN_008c0a10`'s events `0x80..0x85`,
/// the tables' column order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Sprint = 0,
    Run = 1,
    Walk = 2,
    CrouchRun = 3,
    CrouchWalk = 4,
    Prone = 5,
}

impl Step {
    pub const ALL: [Step; 6] = [Step::Sprint, Step::Run, Step::Walk, Step::CrouchRun, Step::CrouchWalk, Step::Prone];

    /// The movement's word in BO2's aliases (`fly_step_<name>_plr_<surface>`).
    pub fn name(self) -> &'static str {
        ["sprint", "run", "walk", "crouch_run", "crouch_walk", "prone"][self as usize]
    }
}

/// What the move knows this frame, in BO2's inches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gait {
    pub stance: Stance,
    /// Ground speed, inches/s.
    pub xyspeed: f32,
    pub on_ground: bool,
    /// The sprint flag (`pm_flags & 0x8000`).
    pub sprinting: bool,
    /// Movement keys, -1..1.
    pub forward: f32,
    pub right: f32,
    /// [`crate::movement::bob_top_speed`] in inches/s.
    pub top_speed: f32,
    /// The weapon's `fSprintCycleScale` (0 reads as 1).
    pub sprint_cycle_scale: f32,
}

/// Whether the bob walks (`FUN_008c4820`'s walking flag: the hold-breath walk `pm_flags & 0x40`
/// or a lean, which the mod does not model, or standing at `player_runbkThreshhold` or slower).
pub fn walking(g: &Gait, t: &Tuning) -> bool {
    g.stance == Stance::Stand && g.xyspeed <= t.player_runbk_threshhold
}

/// Whether the cycle runs as a sprint: the flag, and upright the speed to match.
fn sprint_bob(g: &Gait, t: &Tuning) -> bool {
    g.sprinting && (g.stance != Stance::Stand || g.xyspeed >= t.player_sprint_threshhold)
}

/// The stance and walking factor (`0x00bd265c`: `[stance × 2 + walking]`, stance 0 stand, 1 prone,
/// 2 crouch; three more rows running backwards, `pm_flags & 0x20`).
fn factor(g: &Gait, walk: bool, t: &Tuning) -> f32 {
    let back = g.forward < 0.0;
    let pick = |run: f32, walk_f: f32| if walk { walk_f } else { run };
    match (g.stance, back) {
        (Stance::Stand, false) => pick(t.bob_factor_stand, t.bob_factor_stand_walk),
        (Stance::Prone, false) => pick(t.bob_factor_prone, t.bob_factor_prone_walk),
        (Stance::Crouch, false) => pick(t.bob_factor_crouch, t.bob_factor_crouch_walk),
        (Stance::Stand, true) => pick(t.bob_factor_back_stand, t.bob_factor_back_stand_walk),
        (Stance::Prone, true) => pick(t.bob_factor_back_prone, t.bob_factor_back_prone_walk),
        (Stance::Crouch, true) => pick(t.bob_factor_back_crouch, t.bob_factor_back_crouch_walk),
    }
}

/// The cycle after `dt` seconds of gait `g`, from `cycle` (0..256, kept fractional: BO2 truncates
/// to its byte each move, which at the display rate would drop up to a third of the cycle).
pub fn advance(cycle: f32, g: &Gait, t: &Tuning, dt: f32) -> f32 {
    if !g.on_ground {
        return cycle;
    }
    if g.xyspeed < t.player_move_threshhold {
        // `FUN_008c1a20`: standing still (under an inch a second) restarts the cycle.
        return if g.xyspeed < 1.0 { 0.0 } else { cycle };
    }
    if g.forward == 0.0 && g.right == 0.0 {
        return cycle;
    }
    let sprint = sprint_bob(g, t);
    let walk = walking(g, t);
    let back = g.forward < 0.0;
    let bob = if g.stance == Stance::Stand && !back && sprint { t.player_sprint_camera_bob } else { factor(g, walk, t) };
    let mut rate = g.xyspeed / g.top_speed.max(1e-3) * bob;
    // `FUN_008c1d00`: a sprint's cycle runs at the weapon's own scale.
    if g.sprinting && g.stance == Stance::Stand && g.sprint_cycle_scale > 0.0 {
        rate *= g.sprint_cycle_scale;
    }
    (cycle + dt * 1000.0 * rate).rem_euclid(256.0)
}

/// Whether moving the cycle from `old` to `new` put a foot down (`FUN_008c16c0`: a multiple of 64
/// crossed, into the loud quarters `64..128` and `192..256`, at `player_footstepsThreshhold`).
pub fn stepped(old: f32, new: f32, xyspeed: f32, t: &Tuning) -> bool {
    let (o, n) = (old as u32 & 0xff, new as u32 & 0xff);
    (o ^ n) & !0x3f != 0 && (n & 0x7f) > 0x3f && xyspeed >= t.player_footsteps_threshhold
}

/// The footstep's movement (`FUN_008c0a10`): prone; crouched, running at
/// `player_runThreshhold` or faster; upright, sprinting, running, else walking.
pub fn step_of(stance: Stance, sprinting: bool, xyspeed: f32, t: &Tuning) -> Step {
    let run = xyspeed >= t.player_run_threshhold;
    match stance {
        Stance::Prone => Step::Prone,
        Stance::Crouch => if run { Step::CrouchRun } else { Step::CrouchWalk },
        Stance::Stand if sprinting => Step::Sprint,
        Stance::Stand => if run { Step::Run } else { Step::Walk },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gait(speed: f32) -> Gait {
        Gait { stance: Stance::Stand, xyspeed: speed, on_ground: true, sprinting: false, forward: 1.0, right: 0.0, top_speed: 190.0, sprint_cycle_scale: 1.0 }
    }

    /// At full run speed the standing cycle advances 0.335 per millisecond: 256 in 0.764 s, two
    /// steps per cycle (BO2's 2.6 steps a second, 72" apart at 190"/s).
    #[test]
    fn a_run_steps_at_bo2s_cadence() {
        let t = Tuning::default();
        let (mut c, mut steps) = (0.0, 0);
        for _ in 0..300 {
            let n = advance(c, &gait(190.0), &t, 1.0 / 30.0);
            steps += stepped(c, n, 190.0, &t) as u32;
            c = n;
        }
        // 10 s: 3350 cycle units / 128 per step.
        assert_eq!(steps, 26, "steps in 10 s");
        assert!((advance(0.0, &gait(190.0), &t, 0.1) - 33.5).abs() < 1e-3);
    }

    /// Still, airborne and keyless: the cycle resets, holds, holds.
    #[test]
    fn the_cycle_holds_or_resets() {
        let t = Tuning::default();
        assert_eq!(advance(40.0, &gait(0.5), &t, 0.1), 0.0);
        assert_eq!(advance(40.0, &Gait { on_ground: false, ..gait(190.0) }, &t, 0.1), 40.0);
        assert_eq!(advance(40.0, &Gait { forward: 0.0, ..gait(190.0) }, &t, 0.1), 40.0);
    }

    /// The factor table: walking upright slow, backwards, crouched, sprinting with the weapon's scale.
    #[test]
    fn the_factor_follows_stance_and_direction() {
        let t = Tuning::default();
        let r = |g: Gait| advance(0.0, &g, &t, 0.001) / (g.xyspeed / g.top_speed);
        assert!((r(gait(50.0)) - 0.28).abs() < 1e-4, "standing at 60\"/s or under walks");
        assert!((r(Gait { forward: -1.0, ..gait(100.0) }) - 0.36).abs() < 1e-4);
        assert!((r(Gait { stance: Stance::Crouch, ..gait(100.0) }) - 0.34).abs() < 1e-4);
        assert!((r(Gait { sprinting: true, sprint_cycle_scale: 1.2, ..gait(285.0) }) - 0.6).abs() < 1e-4);
    }

    /// Two loud crossings per cycle; the quiet ones and slow feet are silent.
    #[test]
    fn steps_fall_on_the_loud_quarters() {
        let t = Tuning::default();
        assert!(stepped(60.0, 66.0, 100.0, &t));
        assert!(stepped(190.0, 194.0, 100.0, &t));
        assert!(!stepped(126.0, 130.0, 100.0, &t));
        assert!(!stepped(250.0, 2.0, 100.0, &t));
        assert!(!stepped(60.0, 66.0, 5.0, &t));
        assert_eq!(step_of(Stance::Stand, false, 120.0, &t), Step::Run);
        assert_eq!(step_of(Stance::Stand, false, 80.0, &t), Step::Walk);
        assert_eq!(step_of(Stance::Crouch, false, 80.0, &t), Step::CrouchWalk);
        assert_eq!(step_of(Stance::Prone, false, 30.0, &t), Step::Prone);
        assert_eq!(step_of(Stance::Stand, true, 280.0, &t), Step::Sprint);
    }
}
