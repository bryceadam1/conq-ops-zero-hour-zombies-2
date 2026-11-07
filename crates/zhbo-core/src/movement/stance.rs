//! The stance's eye height, its speed scale and the prone body's yaw.
//!
//! * **The eye** (`FUN_008c1120` PM_ViewHeightAdjust): the view height (`ps+0x208`, inches) goes
//!   to the stance's (60 / 40 / 11) one leg at a time — standing to prone passes crouched — each
//!   leg over its time (`FUN_008c1040`: 600 ms into prone, 400 out of it, 200 between standing
//!   and crouched) along the engine's own curve: a waypoint table of (percent of the time, share
//!   of the way), lerped between waypoints (`FUN_008c1070`; tables at `0x00f6ba88`, `0x00f6bad0`,
//!   `0x00f6bb18`, `0x00f6bb70`). Going prone the eye sinks past the floor of the curve and
//!   settles; getting up it dips first. A stance change against the leg under way turns it round
//!   from where it is.
//! * **The speed scale** (`FUN_008c3e00` PM_CmdScaleForStance) reads the same leg: into prone it
//!   blends crouched's 0.65 to prone's 0.15 over the leg, out of prone 0.15 to 0.65; otherwise the
//!   stance's own.
//! * **Prone turning** (`FUN_008c29a0`, `FUN_008c2720`): the prone body has its own yaw. It turns
//!   after the view at 55 degrees a second while the view is more than 40 degrees from it or the
//!   player crawls, and the view never gets more than `bg_prone_yawcap` (45) from it.
//!
//! The tables are engine code data, not dvars or tuning: they are kept here, cited, as the one
//! copy.

use super::{Body, MoveTuning, Stance};

/// A waypoint: percent of the leg's time, share of the way from the leg's `to` height to its
/// `from` height (`FUN_008c1070`'s `{int, float}` entries).
type Waypoint = (i32, f32);

/// Standing to crouched (`0x00f6ba88`): `40 + share x (60 - 40)`.
const STAND_TO_CROUCH: [Waypoint; 8] = [(0, 1.0), (1, 0.975), (4, 0.925), (30, 0.8), (80, 0.2), (90, 0.075), (95, 0.025), (100, 0.0)];
/// Crouched to standing (`0x00f6bad0`): `40 + share x (60 - 40)`.
const CROUCH_TO_STAND: [Waypoint; 8] = [(0, 0.0), (5, 0.025), (10, 0.075), (20, 0.2), (70, 0.8), (96, 0.925), (99, 0.975), (100, 1.0)];
/// Crouched to prone (`0x00f6bb18`): `11 + share x (40 - 11)`; it sinks to 0.138 at half way and
/// comes back up a little before it settles.
const CROUCH_TO_PRONE: [Waypoint; 10] =
    [(0, 1.0), (11, 0.931), (22, 0.759), (34, 0.483), (45, 0.172), (50, 0.138), (55, 0.172), (70, 0.241), (90, 0.207), (100, 0.0)];
/// Prone to crouched (`0x00f6bb70`): `11 + share x (40 - 11)`; it dips below prone first.
const PRONE_TO_CROUCH: [Waypoint; 7] = [(0, 0.0), (5, -0.034), (30, 0.345), (50, 0.483), (67, 0.69), (83, 0.79), (100, 1.0)];

/// The eye's leg under way (`ps+0x20c` start time, `+0x210` the leg's target, `+0x214` going
/// down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewLerp {
    /// When the leg started, the body's clock (seconds).
    pub start: f32,
    pub target: Stance,
    pub down: bool,
}

crate::statehash::state_hash!(struct ViewLerp { start, target, down });

/// A stance's view height, inches.
fn height(t: &MoveTuning, s: Stance) -> f32 {
    match s {
        Stance::Stand => t.view_stand,
        Stance::Crouch => t.view_crouch,
        Stance::Prone => t.view_prone,
    }
}

/// The leg's time, ms (`FUN_008c1040`).
fn duration_ms(t: &MoveTuning, l: &ViewLerp) -> f32 {
    1000.0
        * match (l.target, l.down) {
            (Stance::Prone, _) => t.prone_down_time,
            (Stance::Crouch, false) => t.prone_up_time,
            _ => t.crouch_time,
        }
}

/// `FUN_008c1070`: the share at `frac` percent along `table`, lerped between waypoints.
fn table_share(table: &[Waypoint], frac: i32) -> f32 {
    let mut prev = table[0];
    if frac <= prev.0 {
        return prev.1;
    }
    for &w in &table[1..] {
        if frac == w.0 {
            return w.1;
        }
        if frac < w.0 {
            return prev.1 + (w.1 - prev.1) * (frac - prev.0) as f32 / (w.0 - prev.0) as f32;
        }
        prev = w;
    }
    // Past the table's end the engine's loop leaves the share at 0.
    0.0
}

/// The view height `frac` percent into leg `l` (`FUN_008c1120`'s calls of `FUN_008c1070`).
fn leg_height(t: &MoveTuning, l: &ViewLerp, frac: i32) -> f32 {
    let (crouch, stand, prone) = (t.view_crouch, t.view_stand, t.view_prone);
    match (l.target, l.down) {
        (Stance::Prone, _) => prone + table_share(&CROUCH_TO_PRONE, frac) * (crouch - prone),
        (Stance::Crouch, false) => prone + table_share(&PRONE_TO_CROUCH, frac) * (crouch - prone),
        (Stance::Crouch, true) => crouch + table_share(&STAND_TO_CROUCH, frac) * (stand - crouch),
        (Stance::Stand, _) => crouch + table_share(&CROUCH_TO_STAND, frac) * (stand - crouch),
    }
}

/// The percent of the way leg `l` is at the body's time (`(now - start) x 100 / time`, integer).
fn leg_frac(body: &Body, t: &MoveTuning, l: &ViewLerp) -> i32 {
    let elapsed_ms = ((body.time - l.start) * 1000.0).round();
    (elapsed_ms * 100.0 / duration_ms(t, l)).floor() as i32
}

/// `FUN_008c1120`: one frame of the eye toward the stance's height. Sets `view_height` (inches)
/// and `eye_height` (Zero Hour units, at the General's drawn scale).
pub(super) fn adjust_view_height(body: &mut Body, t: &MoveTuning) {
    let target = body.stance;
    let target_h = height(t, target);
    if body.view_height == 0.0 {
        body.view_height = target_h;
    }
    let mut frac = 0;
    if let Some(l) = body.view_lerp {
        frac = leg_frac(body, t, &l).max(0);
        if frac >= 100 {
            body.view_height = height(t, l.target);
            body.view_lerp = None;
        } else {
            body.view_height = leg_height(t, &l, frac);
        }
    }
    match body.view_lerp {
        None if body.view_height != target_h => {
            let crouch = t.view_crouch;
            let (leg, down) = match target {
                Stance::Prone => (if body.view_height > crouch { Stance::Crouch } else { Stance::Prone }, true),
                Stance::Crouch => (Stance::Crouch, body.view_height > crouch),
                Stance::Stand => (if body.view_height >= crouch { Stance::Stand } else { Stance::Crouch }, false),
            };
            body.view_lerp = Some(ViewLerp { start: body.time, target: leg, down });
        }
        Some(mut l) if target != l.target => {
            // The stance went against the leg: it turns round from where it is.
            let (th, lh) = (target_h, height(t, l.target));
            if (th < lh && !l.down) || (th > lh && l.down) {
                l.down = !l.down;
                l.target = match (l.down, l.target) {
                    (false, Stance::Prone) => Stance::Crouch,
                    (false, _) => Stance::Stand,
                    (true, Stance::Stand) => Stance::Crouch,
                    (true, _) => Stance::Prone,
                };
                if frac == 0 {
                    body.view_lerp = None;
                    body.view_height = height(t, l.target);
                } else {
                    l.start = body.time - duration_ms(t, &l) * (100 - frac) as f32 * 0.01 / 1000.0;
                    body.view_lerp = Some(l);
                }
            }
        }
        _ => {}
    }
    body.eye_height = body.view_height * t.eye_scale;
}

/// `FUN_008c3e00`: the stance's speed scale. Into prone it blends crouched to prone over the leg;
/// out of prone (the crouch leg going up) prone to crouched; otherwise the stance's own.
pub(super) fn speed_scale(body: &Body, t: &MoveTuning) -> f32 {
    if let Some(l) = body.view_lerp {
        let blend = |from: f32, to: f32| {
            let f = ((body.time - l.start) * 1000.0 / duration_ms(t, &l)).clamp(0.0, 1.0);
            (1.0 - f) * from + f * to
        };
        match (l.target, l.down) {
            (Stance::Prone, _) => return blend(t.crouch_scale, t.prone_scale),
            (Stance::Crouch, false) => return blend(t.prone_scale, t.crouch_scale),
            _ => {}
        }
    }
    match body.stance {
        Stance::Stand => 1.0,
        Stance::Crouch => t.crouch_scale,
        Stance::Prone => t.prone_scale,
    }
}

/// `AngleDelta`: `a - b` wrapped to (-pi, pi].
fn angle_delta(a: f32, b: f32) -> f32 {
    crate::math::wrap_angle(a - b)
}

/// `FUN_008c2720`'s clamp: `view_yaw` brought to within `cap` of the prone body's `prone_yaw`
/// (radians).
pub fn cap_prone_yaw(prone_yaw: f32, view_yaw: f32, cap: f32) -> f32 {
    let d = angle_delta(prone_yaw, view_yaw);
    if d > cap {
        crate::math::wrap_angle(prone_yaw - cap)
    } else if d < -cap {
        crate::math::wrap_angle(prone_yaw + cap)
    } else {
        view_yaw
    }
}

/// `FUN_008c29a0`: one frame of prone turning for a view at `view_yaw` (radians), `moving` with a
/// move key down. The body turns after the view (at most `ProneSwingSpeed` a second) while the
/// view is past `bg_prone_yawcap - ProneSwingMargin` from it or the player moves; the view comes
/// back within the cap. Returns the view yaw to use. A body not prone keeps its prone yaw on the
/// view (it lies down facing where it looks).
pub fn prone_view_yaw(body: &mut Body, view_yaw: f32, moving: bool, t: &MoveTuning, dt: f32) -> f32 {
    if body.stance != Stance::Prone {
        body.prone_yaw = view_yaw;
        return view_yaw;
    }
    let d = angle_delta(body.prone_yaw, view_yaw);
    if d.abs() > t.prone_yaw_cap - t.prone_swing_margin || (moving && d != 0.0) {
        let step = t.prone_swing_speed * dt;
        body.prone_yaw = if step <= d.abs() {
            crate::math::wrap_angle(if d <= 0.0 { body.prone_yaw + step } else { body.prone_yaw - step })
        } else {
            view_yaw
        };
    }
    cap_prone_yaw(body.prone_yaw, view_yaw, t.prone_yaw_cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    fn body_at(stance: Stance) -> (Body, MoveTuning) {
        let t = MoveTuning::default();
        let mut b = Body::new(Vec3::ZERO, &t);
        b.stance = stance;
        (b, t)
    }

    /// Steps the eye at 30 Hz for `seconds`, returning the heights (inches) each frame.
    fn run(b: &mut Body, t: &MoveTuning, seconds: f32) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..(seconds / crate::units::LOGIC_DT).round() as i32 {
            b.time += crate::units::LOGIC_DT;
            adjust_view_height(b, t);
            out.push(b.view_height);
        }
        out
    }

    #[test]
    fn the_engine_tables_lerp_between_waypoints() {
        assert_eq!(table_share(&STAND_TO_CROUCH, 0), 1.0);
        assert!((table_share(&STAND_TO_CROUCH, 55) - 0.5).abs() < 1e-6, "half-way between 30 (0.8) and 80 (0.2)");
        assert_eq!(table_share(&CROUCH_TO_PRONE, 50), 0.138);
        assert_eq!(table_share(&PRONE_TO_CROUCH, 100), 1.0);
    }

    #[test]
    fn crouching_takes_200_ms_along_bo2s_curve() {
        let (mut b, t) = body_at(Stance::Stand);
        b.stance = Stance::Crouch;
        let h = run(&mut b, &t, 0.3);
        // Frame 1 starts the leg; frame 2 is 33 ms (16%) in: 0.8 + (0.925-0.8)·(30-16)/26 of the way.
        assert_eq!(h[0], t.view_stand, "the leg starts from where the eye is");
        assert!(h[1] < t.view_stand && h[1] > 55.0, "{h:?}");
        assert!(h.iter().skip(7).all(|&v| v == t.view_crouch), "down by 200 ms: {h:?}");
        assert!((b.eye_height - t.view_crouch * t.eye_scale).abs() < 1e-5);
    }

    #[test]
    fn standing_to_prone_passes_crouched_then_sinks_and_settles() {
        let (mut b, t) = body_at(Stance::Stand);
        b.stance = Stance::Prone;
        let h = run(&mut b, &t, 1.2);
        // 200 ms to crouched, then 600 ms into prone along the dip-and-settle curve.
        let low = h.iter().cloned().fold(f32::MAX, f32::min);
        assert_eq!(low, t.view_prone, "{h:?}");
        let first_crouch = h.iter().position(|&v| v == t.view_crouch).expect("passes crouched");
        let settled = h.iter().position(|&v| v == t.view_prone).unwrap();
        let mid = &h[first_crouch..settled];
        // The curve's own wobble: it sinks to 0.138 of the way, comes back up to ~0.24, settles.
        assert!(mid.windows(2).any(|w| w[1] > w[0]), "comes back up after the trough: {mid:?}");
        assert_eq!(*h.last().unwrap(), t.view_prone);
        assert!(h.len() - h.iter().position(|&v| v == t.view_prone).unwrap() > 5, "settled by ~0.8 s: {h:?}");
    }

    #[test]
    fn a_reversed_stance_turns_the_leg_round_from_where_it_is() {
        let (mut b, t) = body_at(Stance::Stand);
        b.stance = Stance::Crouch;
        run(&mut b, &t, 0.1);
        let at = b.view_height;
        b.stance = Stance::Stand;
        let h = run(&mut b, &t, 0.3);
        // The turning frame still steps the old leg (the engine computes it first), then the
        // eye climbs from there without a jump.
        assert!(h[0] <= at && h[0] > t.view_crouch, "{at} -> {}", h[0]);
        assert!(h.windows(2).all(|w| w[1] >= w[0]), "climbs back: {h:?}");
        assert_eq!(*h.last().unwrap(), t.view_stand);
    }

    #[test]
    fn the_speed_scale_follows_the_prone_leg() {
        let (mut b, t) = body_at(Stance::Crouch);
        b.view_height = t.view_crouch;
        b.stance = Stance::Prone;
        b.time += 0.001;
        adjust_view_height(&mut b, &t);
        b.time += 0.3;
        assert!((speed_scale(&b, &t) - (t.crouch_scale + t.prone_scale) * 0.5).abs() < 0.01, "half-way at 300 of 600 ms");
        // Standing to prone: the stand-crouch leg first, at prone's scale (the stance's own).
        let (mut s, t) = body_at(Stance::Stand);
        s.stance = Stance::Prone;
        s.time += 0.001;
        adjust_view_height(&mut s, &t);
        assert_eq!(speed_scale(&s, &t), t.prone_scale);
    }

    #[test]
    fn the_prone_body_turns_after_the_view_and_caps_it() {
        let (mut b, t) = body_at(Stance::Prone);
        b.prone_yaw = 0.0;
        let dt = crate::units::LOGIC_DT;
        // Within 40 degrees and still: the body stays, the view is free.
        let v = prone_view_yaw(&mut b, 0.5, false, &t, dt);
        assert_eq!((v, b.prone_yaw), (0.5, 0.0));
        // 90 degrees: the view is held at 45 and the body swings 55 degrees a second after it.
        let v = prone_view_yaw(&mut b, 90f32.to_radians(), false, &t, dt);
        assert!((b.prone_yaw - 55f32.to_radians() * dt).abs() < 1e-5, "{}", b.prone_yaw.to_degrees());
        assert!((v - (b.prone_yaw + 45f32.to_radians())).abs() < 1e-5, "{}", v.to_degrees());
        // Crawling, the body comes round to the view.
        for _ in 0..60 {
            prone_view_yaw(&mut b, 0.3, true, &t, dt);
        }
        assert!((b.prone_yaw - 0.3).abs() < 1e-5);
        assert_eq!(cap_prone_yaw(0.0, -1.2, 45f32.to_radians()), -45f32.to_radians());
    }
}
