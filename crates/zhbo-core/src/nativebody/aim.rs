//! The third-person body's aim pitch: BO2's bone controllers (t6mp `BG_PlayerAngles`
//! `FUN_008a9a70` and `BG_Player_DoControllers` `FUN_008a9ed0`, run by `FUN_008aa870` for each
//! client entity, `CG_Player_PreControllers` `FUN_004b7960`).
//!
//! **The torso pitch.** BO2 swings the torso's pitch toward twice the view pitch
//! (`BG_SwingAngles` `FUN_008a97d0`: no swing tolerance, never more than 45° behind, at
//! `|delta| × 0.05` (at least 0.5) × 0.15 degrees per millisecond). Prone, only half of an upward
//! look reaches the torso and none of a downward one; a riot shield halves it.
//!
//! **The controllers.** The torso pitch `P` is spread over the spine by fixed shares: `pelvis`
//! 0.4, `back_mid` 0.1, `back_up` 0.5 (`back_low` takes none), and the neck turns 0.3 of what the
//! head still lacks (`view − P`). Each controller eases toward its target at 0.3° per millisecond.
//! The MP bodies have no `back_up` bone (all three factions), so BO2 drops that share
//! (`DObjGetBoneIndex` finds nothing): the spine below the arms turns `0.5 P` = the view pitch,
//! which is why the swing aims at twice it, and the gun in the hands points where the player looks.
//!
//! The rotations are about the body's own lateral axis (BO2's controller angles are pitch about Y
//! in the model frame), at each bone, so the legs below the pelvis follow its share as BO2's do.
//! Presentation only: the sim's hit boxes stay on their posed states.

/// The controller bones and each one's share of the torso pitch and of the head's remainder
/// (`FUN_008a9ed0`'s pitch column: `back_low` none, `back_mid` 0.1, `back_up` 0.5, `neck` 0.3 of
/// the head, `pelvis` 0.4), in skeleton order (parents first).
pub const CONTROLLERS: [(&str, f32, f32); 5] = [("pelvis", 0.4, 0.0), ("back_low", 0.0, 0.0), ("back_mid", 0.1, 0.0), ("back_up", 0.5, 0.0), ("neck", 0.0, 0.3)];

/// The torso pitch's target as a multiple of the view pitch (`FUN_008a9a70`: `_DAT_00c6f6ac`).
const TORSO_PITCH_SCALE: f32 = 2.0;
/// `BG_SwingAngles`' clamp (degrees the torso may trail its target), speed and scale.
const SWING_CLAMP: f32 = 45.0;
const SWING_SPEED: f32 = 0.15;
const SWING_SCALE: f32 = 0.05;
const SWING_MIN_SCALE: f32 = 0.5;
/// Degrees per millisecond a controller moves toward its target (`FUN_008aa870`, `DAT_00bc6698`).
const CONTROLLER_RATE: f32 = 0.3;

/// One General's aim state: the swung torso pitch and each controller's eased pitch (degrees,
/// BO2's sign: positive looking down).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aim {
    torso: f32,
    controllers: [f32; CONTROLLERS.len()],
    started: bool,
}

/// What the body knows of a General's aim this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AimIn {
    /// The view pitch, degrees, BO2's sign (positive down).
    pub view_pitch: f32,
    pub prone: bool,
    pub shield: bool,
}

impl Aim {
    /// Advance `ms` milliseconds toward `i`; the controllers' pitches (degrees, BO2's sign), in
    /// [`CONTROLLERS`] order.
    pub fn step(&mut self, i: AimIn, ms: f32) -> [f32; CONTROLLERS.len()] {
        let dest = i.view_pitch * TORSO_PITCH_SCALE;
        if !self.started {
            // A General first seen takes his pose at once.
            self.torso = dest;
        } else {
            let delta = dest - self.torso;
            let scale = (delta.abs() * SWING_SCALE).max(SWING_MIN_SCALE);
            let step = ms * scale * SWING_SPEED;
            self.torso = if delta.abs() <= step { dest } else { self.torso + step * delta.signum() };
            self.torso = self.torso.clamp(dest - SWING_CLAMP, dest + SWING_CLAMP);
        }
        let mut p = self.torso;
        if i.prone {
            p = if p < 0.0 { p * 0.5 } else { 0.0 };
        }
        if i.shield {
            p *= 0.5;
        }
        for (k, (_, torso_share, head_share)) in CONTROLLERS.iter().enumerate() {
            let target = p * torso_share + (i.view_pitch - p) * head_share;
            let c = &mut self.controllers[k];
            *c = if !self.started { target } else { *c + (target - *c).clamp(-CONTROLLER_RATE * ms, CONTROLLER_RATE * ms) };
        }
        self.started = true;
        self.controllers
    }
}

/// Turn each controller bone of `skel` by its pitch (degrees, BO2's sign) about the model's
/// lateral (+Y) axis at the bone, over the local pose `local` (`model` is scratch). A bone the
/// skeleton lacks is skipped, as BO2's controller is.
pub fn apply(skel: &zhbo_native::pose::Skeleton, bones: &[Option<usize>; CONTROLLERS.len()], pitches: &[f32; CONTROLLERS.len()], local: &mut [zhbo_native::math::Xf], model: &mut Vec<zhbo_native::math::Xf>) {
    use zhbo_native::math::{axis_angle, qconj, qmul, qnorm};
    for (k, bone) in bones.iter().enumerate() {
        let (Some(b), pitch) = (*bone, pitches[k]) else { continue };
        if pitch.abs() < 1e-4 {
            continue;
        }
        skel.model_space(local, model);
        let parent = skel.bones[b].parent.map_or([0.0, 0.0, 0.0, 1.0], |p| model[p].0);
        let r = axis_angle([0.0, 1.0, 0.0], pitch.to_radians());
        local[b].0 = qnorm(qmul(qmul(qmul(qconj(parent), r), parent), local[b].0));
    }
}

/// The controller bones' indices in `skel`.
pub fn bones(skel: &zhbo_native::pose::Skeleton) -> [Option<usize>; CONTROLLERS.len()] {
    CONTROLLERS.map(|(name, _, _)| skel.bone(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Held still, the spine below the arms (pelvis + back_mid) turns the view pitch, the head
    /// 0.7 of it; prone looks down not at all; the shield halves it.
    #[test]
    fn the_controllers_split_bo2s_torso_pitch() {
        let mut a = Aim::default();
        let c = a.step(AimIn { view_pitch: -30.0, ..Default::default() }, 16.0);
        assert!((c[0] + c[2] - -30.0).abs() < 1e-3, "pelvis + back_mid = the view: {c:?}");
        assert!((c[3] - -30.0).abs() < 1e-3, "back_up's 0.5 share (the bone MP bodies lack)");
        assert!((c[4] - 9.0).abs() < 1e-3, "the neck: 0.3 x (view - 2 view)");
        let mut prone = Aim::default();
        assert_eq!(prone.step(AimIn { view_pitch: 20.0, prone: true, ..Default::default() }, 16.0), [0.0, 0.0, 0.0, 0.0, 6.0], "prone, a look down turns only the neck");
        let mut shield = Aim::default();
        let s = shield.step(AimIn { view_pitch: -30.0, shield: true, ..Default::default() }, 16.0);
        assert!((s[0] + s[2] - -15.0).abs() < 1e-3);
    }

    /// After the first frame the torso swings and the controllers ease: a sudden look up is not
    /// taken at once, but is within a quarter second.
    #[test]
    fn the_pitch_swings_and_eases() {
        let mut a = Aim::default();
        a.step(AimIn::default(), 16.0);
        let first = a.step(AimIn { view_pitch: -40.0, ..Default::default() }, 16.0);
        assert!(first[0] > -0.4 * 80.0 + 1.0, "{first:?}");
        let mut c = first;
        for _ in 0..20 {
            c = a.step(AimIn { view_pitch: -40.0, ..Default::default() }, 16.0);
        }
        assert!((c[0] + c[2] - -40.0).abs() < 0.5, "{c:?}");
    }
}
