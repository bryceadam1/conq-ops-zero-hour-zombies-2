//! Where the gun sits in the view (Track R6, ADR: "ADS alignment").
//!
//! BO2 draws the first-person weapon at the eye's frame, turned by the frame's gun-position angles,
//! moved by the weapon def's offsets (`weapons::gunpos`, Ghidra G4), and — while aiming — with the
//! sight on the view axis. There is no viewmodel FOV: the pass takes the scene's own projection
//! (G4, `docs/CONTRACT.md` "The native viewmodel").
//!
//! **The axis is the sim's.** The bridge hands over the eye and direction the client wrote from the
//! sim's `Body::eye()` and view angles (`client::Client::frame`); that is the eye→aim axis
//! `sim::fire` traces shots along, so placement never invents a second one.
//!
//! **The sight is the converter's aim marker.** `zhbo_native::AIM_TAG` is BO2's `tag_sights`, an
//! optic's window when one is mounted, or the front sight the converter found
//! (`zhbo_assets::native::aim_marker`): the same point the W3D path slides. BO2 itself reaches the
//! aimed pose through the weapon animation and the def's `fAdsAimPitch`; until an ADS animation
//! layer exists, the aim fraction also slides the marker onto the axis, which is what
//! `fWeaponPosFrac` blends in BO2 (`FUN_008efa70`).

use zhbo_native::math::{apply, compose, mat4, mat4_mul, Mat4, Xf, MAT4_IDENTITY, QUAT_IDENTITY};

use super::{eye_frame, ART_SCALE};

/// The sight's point in the eye's frame (X along the aim, Y left, Z up; world units), for the
/// frame's `gun` transform (eye space, art units) and the posed `aim` marker (the composed model's
/// space, BO2 inches).
pub fn sight(gun: Xf, aim: Xf) -> [f32; 3] {
    apply(gun, apply(aim, [0.0; 3]).map(|c| c * ART_SCALE))
}

/// `gun` slid so the sight sits on the view axis, by the aim fraction `ads`.
///
/// BO2's `fWeaponPosFrac` (which the arsenal moves at the weapon's own ADS in/out times) scales
/// the slide: at 1 the sight's sideways and vertical offsets are gone and its forward distance is
/// kept, at 0 the pose is the hip pose untouched, in between it is linear — the same share the
/// def's offsets fade by (`max(0, 1 - 2 * ads)`, `FUN_008f0700`).
pub fn aligned(gun: Xf, aim: Xf, ads: f32) -> Xf {
    let k = ads.clamp(0.0, 1.0);
    if k <= 0.0 {
        return gun;
    }
    let at = sight(gun, aim);
    // The offset is applied in the eye's frame, before the gun's own rotation: a translation of
    // the whole gun, so the sight keeps the orientation the animation gave it.
    compose((QUAT_IDENTITY, [0.0, -at[1] * k, -at[2] * k]), gun)
}

/// The composed model's to-world matrix: the eye's frame, the gun's transform (with the ADS slide
/// in it) and the art's scale (the converter's `character::SCALE`).
pub fn root(eye: [f32; 3], dir: [f32; 3], gun: Xf) -> Mat4 {
    let scale = {
        let mut s = MAT4_IDENTITY;
        s[0] = ART_SCALE;
        s[5] = ART_SCALE;
        s[10] = ART_SCALE;
        s
    };
    mat4_mul(&mat4_mul(&eye_frame(eye, dir), &mat4(gun)), &scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhbo_native::math::axis_angle;

    /// A gun turned away from the view: yaw left, muzzle up, rolled, and pushed right.
    fn turned() -> Xf {
        let q = zhbo_native::math::qmul(axis_angle([0.0, 0.0, 1.0], 0.12), axis_angle([0.0, 1.0, 0.0], -0.08));
        (zhbo_native::math::qnorm(q), [1.0, 0.4, -0.15])
    }

    /// An aim marker 20 inches ahead, 2 left and 4 up of the gun's root.
    fn aim() -> Xf {
        (QUAT_IDENTITY, [20.0, 2.0, 4.0])
    }

    #[test]
    fn aiming_takes_the_sights_lateral_and_vertical_offset_to_zero() {
        let gun = turned();
        let hip = sight(gun, aim());
        assert!(hip[1].abs() > 0.01 && hip[2].abs() > 0.01, "the hip pose has an offset: {hip:?}");
        let half = sight(aligned(gun, aim(), 0.5), aim());
        let full = sight(aligned(gun, aim(), 1.0), aim());
        // Half-way is half the hip offset, and the forward distance is the authored one all along.
        assert!((half[1] - hip[1] * 0.5).abs() < 1e-5 && (half[2] - hip[2] * 0.5).abs() < 1e-5, "{half:?}");
        assert!(full[1].abs() < 1e-5 && full[2].abs() < 1e-5, "{full:?}");
        assert!((full[0] - hip[0]).abs() < 1e-5 && full[0] > 0.0, "{full:?} vs {hip:?}");
        // Monotone from hip to aimed.
        assert!(hip[1].abs() > half[1].abs() && half[1].abs() > full[1].abs());
        // The alignment is a translation: the gun keeps the animation's orientation.
        assert_eq!(aligned(gun, aim(), 1.0).0, gun.0);
    }

    #[test]
    fn the_hip_pose_is_left_alone() {
        let gun = turned();
        assert_eq!(aligned(gun, aim(), 0.0), gun);
        assert_eq!(aligned(gun, aim(), -1.0), gun, "an out-of-range fraction cannot flip the slide");
    }

    #[test]
    fn the_eye_frame_puts_the_axis_on_the_direction() {
        let eye = [12.0, -3.0, 5.0];
        let dir = [0.0, 0.6, 0.8];
        let m = root(eye, dir, (QUAT_IDENTITY, [0.0; 3]));
        // The model's +X (inches) lands along the view direction from the eye, scaled to art units.
        let p = zhbo_native::math::mat4_point(&m, [10.0, 0.0, 0.0]);
        assert!((p[0] - eye[0]).abs() < 1e-5 && (p[1] - (eye[1] + dir[1] * 10.0 * ART_SCALE)).abs() < 1e-5, "{p:?}");
        assert!((p[2] - (eye[2] + dir[2] * 10.0 * ART_SCALE)).abs() < 1e-5, "{p:?}");
    }
}
