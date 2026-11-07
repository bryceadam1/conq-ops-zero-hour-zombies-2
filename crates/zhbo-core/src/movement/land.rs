//! The jump and the landing: t6mp's `Jump_*` family and `PM_CrashLand`.
//!
//! * **The jump** (`FUN_008b52f0` CheckJump, `FUN_008b5000` Jump_Start): a standing player on the
//!   ground jumps on a fresh press, no sooner than [`LandTuning::repeat_ms`] after the last jump,
//!   at `sqrt(2 g jump_height / factor)`, where the factor is the slowdown's ([`slowdown_factor`])
//!   while the last landing's timer runs: jumping again at once is lower.
//! * **The jump's landing** (`FUN_008b4e40` Jump_ApplySlowdown, from the walk move while the
//!   jumping flag is up): the velocity is scaled by 0.65 and a 1800 ms timer starts (0.5 and
//!   1200 ms when the feet land a step above the take-off); while it runs, friction's control is
//!   multiplied by the factor (`FUN_008b4f00`, 2.5 easing to 1). The flag goes when the timer
//!   runs out (`FUN_008c20a0` PM_DropTimers).
//! * **Every landing** (`FUN_008c0b20` PM_CrashLand): the fall height is the landing speed's
//!   `v^2 / 2g` (solved from the frame's start like the engine, not the integrator's last step);
//!   past `bg_fallDamageMinHeight` it hurts, a share of 100% up to `bg_fallDamageMaxHeight`
//!   (Lightweight's `fallheight`: none). A landing that hurts slows the player (the hard-landing
//!   timer: weaker friction and acceleration); one that does not, from above 12 inches, scales the
//!   velocity by 0.67. Both dip the view by the event's `landChange` (`FUN_004c1590`).
//!
//! All of it is BO2's integer-millisecond and inch arithmetic, kept in those units; the numbers
//! are the tuning pack's (`crate::tuning`, each key citing its constant).

use super::{Body, MoveTuning};
use crate::tuning::Tuning;
use crate::units::ZH_PER_INCH;

/// The jump's and the landing's numbers, BO2's units (inches, milliseconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandTuning {
    pub slowdown: bool,
    pub land_scale: f32,
    pub land_ms: f32,
    pub land_high_scale: f32,
    pub land_high_ms: f32,
    pub slowdown_max: f32,
    pub slowdown_ramp: f32,
    pub slowdown_ramp_ms: f32,
    pub repeat_ms: f32,
    pub fall_min: f32,
    pub fall_max: f32,
    pub soft_scale: f32,
    pub hard_ms_base: f32,
    pub hard_ms_per_damage: f32,
    pub hard_ms_max: f32,
    pub hard_scale: f32,
    pub hard_scale_drop: f32,
    pub hard_min_scale: f32,
    pub hard_friction: f32,
    pub hard_accel: f32,
    pub dip_min_height: f32,
    pub dip_height_step: f32,
    pub dip_unit: f32,
    pub dip_max: f32,
    pub dip_down_ms: f32,
    pub dip_up_ms: f32,
    pub dip_floor: f32,
}

impl LandTuning {
    pub fn from_tuning(t: &Tuning) -> Self {
        Self {
            slowdown: t.jump_slowdown_enable != 0.0,
            land_scale: t.jump_land_scale,
            land_ms: t.jump_land_time,
            land_high_scale: t.jump_land_high_scale,
            land_high_ms: t.jump_land_high_time,
            slowdown_max: t.jump_slowdown_max,
            slowdown_ramp: t.jump_slowdown_ramp,
            slowdown_ramp_ms: t.jump_slowdown_ramp_time,
            repeat_ms: t.jump_repeat_time,
            fall_min: t.fall_damage_min_height,
            fall_max: t.fall_damage_max_height,
            soft_scale: t.land_scale,
            hard_ms_base: t.hard_land_time_base,
            hard_ms_per_damage: t.hard_land_time_per_damage,
            hard_ms_max: t.hard_land_time_max,
            hard_scale: t.hard_land_scale,
            hard_scale_drop: t.hard_land_scale_drop,
            hard_min_scale: t.hard_land_min_scale,
            hard_friction: t.hard_land_friction_scale,
            hard_accel: t.hard_land_accel_scale,
            dip_min_height: t.land_dip_min_height,
            dip_height_step: t.land_dip_height_step,
            dip_unit: t.land_dip_unit,
            dip_max: t.land_dip_max,
            dip_down_ms: t.land_dip_down_time,
            dip_up_ms: t.land_dip_up_time,
            dip_floor: t.land_dip_view_floor,
        }
    }

    /// The longest timer a jump's landing starts: past it the timer is not the jump's
    /// (`pm_time < 0x709` in `FUN_008b4e40`/`FUN_008b4f00`/`FUN_008b5000`).
    fn jump_window(&self) -> f32 {
        self.land_ms.max(self.land_high_ms)
    }

    /// The fall damage of a landing from `h` inches, percent (`FUN_008c0b20`): none up to the
    /// minimum height, all of it from the maximum, `trunc((h - min) / (max - min) x 100)` between.
    pub fn fall_damage(&self, h: f32) -> u8 {
        if h <= self.fall_min {
            0
        } else if h >= self.fall_max {
            100
        } else {
            let d = ((h - self.fall_min) / (self.fall_max - self.fall_min) * 100.0).trunc();
            if d >= 100.0 {
                100
            } else if d < 1.0 {
                0
            } else {
                d as u8
            }
        }
    }

    /// The landing event's view dip from a fall of `h` inches (`FUN_008c0b20`'s event parameter,
    /// `FUN_004c1590`'s `landChange` is its negative): `(int)(((h - 12) / 26 + 1) x 4)`, at most 24;
    /// none from 12 inches or less.
    pub fn dip(&self, h: f32) -> f32 {
        if h <= self.dip_min_height {
            return 0.0;
        }
        (((h - self.dip_min_height) / self.dip_height_step + 1.0) * self.dip_unit).trunc().min(self.dip_max)
    }

    /// A hurting landing's view dip (`FUN_004c1590`, event `EV_LANDING_PAIN`): the damage turned
    /// back into a height, `min + damage / 100 x (max - min)`, then [`LandTuning::dip`]; under one
    /// inch none.
    pub fn pain_dip(&self, damage: u8) -> f32 {
        let h = self.fall_min + f32::from(damage) * 0.01 * (self.fall_max - self.fall_min);
        let d = self.dip(h);
        if d < 1.0 {
            0.0
        } else {
            d
        }
    }

    /// The view's landing offset (inches, BO2's `landChange` signed) `ms` after the landing
    /// (`FUN_0051f760`): down over 150 ms, back over the next 300.
    pub fn view_dip(&self, land_change: f32, ms: f32) -> f32 {
        if ms <= 0.0 {
            0.0
        } else if ms < self.dip_down_ms {
            land_change * ms / self.dip_down_ms
        } else if ms < self.dip_down_ms + self.dip_up_ms {
            land_change * (1.0 - (ms - self.dip_down_ms) / self.dip_up_ms)
        } else {
            0.0
        }
    }
}

/// What a landing did (`FUN_008c0b20`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    /// The fall height, inches.
    pub height: f32,
    /// Fall damage, percent of max health (0..=100; BO2's `EV_LANDING_PAIN` parameter).
    pub damage: u8,
}

/// `PM_DropTimers` (`FUN_008c20a0`): the movement timer runs down; when it runs out, the jumping
/// flag (and the take-off height with it, `FUN_008b4d60`) and the hard landing go.
pub(super) fn drop_timers(body: &mut Body, dt: f32) {
    if body.pm_time <= 0.0 {
        return;
    }
    let ms = dt * 1000.0;
    if ms < body.pm_time {
        body.pm_time -= ms;
    } else {
        clear_jump(body);
        body.hard_landing = false;
        body.pm_time = 0.0;
    }
}

/// `Jump_ClearState` (`FUN_008b4d60`).
fn clear_jump(body: &mut Body) {
    body.jumping = false;
    body.jump_origin_z = 0.0;
}

/// The slowdown's factor (`FUN_008b4f00` Jump_ReduceFriction; `FUN_008b5000` inlines it): 2.5 while
/// more than 1700 ms are left on the timer, then `1 + pm_time x 1.5 / 1700` down to 1. A timer
/// longer than any jump landing's is not the jump's: the jump state goes and the factor is 1.
pub(super) fn slowdown_factor(body: &mut Body, t: &LandTuning) -> f32 {
    if body.pm_time > t.jump_window() {
        clear_jump(body);
        return 1.0;
    }
    if !t.slowdown {
        1.0
    } else if body.pm_time >= t.slowdown_ramp_ms {
        t.slowdown_max
    } else {
        body.pm_time * t.slowdown_ramp / t.slowdown_ramp_ms + 1.0
    }
}

/// Friction's control multiplier on the ground (`FUN_008bfea0`): a hard landing's, else the jump
/// slowdown's while the jumping flag is up.
pub(super) fn friction_scale(body: &mut Body, t: &LandTuning) -> f32 {
    if body.hard_landing {
        t.hard_friction
    } else if body.jumping {
        slowdown_factor(body, t)
    } else {
        1.0
    }
}

/// `Jump_ApplySlowdown` (`FUN_008b4e40`), each walk frame while the jumping flag is up: on the
/// first (no timer yet) it scales the velocity and starts the timer by where the feet landed
/// against the take-off; a timer that is not the jump's ends the jump state and scales by 0.65.
pub(super) fn apply_slowdown(body: &mut Body, t: &LandTuning, step: f32) {
    let mut scale = 1.0;
    if body.pm_time <= t.jump_window() {
        if body.pm_time == 0.0 {
            if body.jump_origin_z + step <= body.pos.z {
                body.pm_time = t.land_high_ms;
                scale = t.land_high_scale;
            } else {
                body.pm_time = t.land_ms;
                scale = t.land_scale;
            }
        }
    } else {
        clear_jump(body);
        scale = t.land_scale;
    }
    if t.slowdown {
        body.vel = body.vel * scale;
    }
}

/// `CheckJump` and `Jump_Start` (`FUN_008b52f0`, `FUN_008b5000`): the jump velocity, or `None` when
/// the last jump was too recent. Sets the jump state (the flag, the take-off height and time; the
/// timer and a hard landing cleared, the sprint button released).
pub(super) fn try_jump(body: &mut Body, t: &MoveTuning) -> Option<f32> {
    if body.jump_time.is_some_and(|j| (body.time - j) * 1000.0 < t.land.repeat_ms) {
        return None;
    }
    let factor = if body.jumping && body.pm_time <= t.land.jump_window() { slowdown_factor(body, &t.land) } else { 1.0 };
    let vz = (2.0 * t.gravity * t.jump_height / factor).sqrt();
    body.jump_time = Some(body.time);
    body.jump_origin_z = body.pos.z;
    body.pm_time = 0.0;
    body.jumping = true;
    body.hard_landing = false;
    body.sprint_button_up = false;
    Some(vz)
}

/// `PM_CrashLand` (`FUN_008c0b20`): the feet touched down this frame, having started it at height
/// `z0` (Zero Hour units) going up at `vz0` (units/s). `fall_height_perk` is Lightweight's
/// `fallheight` (`ps+0x548` bit 14): no fall damage. `None` when the landing is out of the fall's
/// reach (the feet came up onto something higher than the frame started: the engine returns).
pub(super) fn crash_land(body: &mut Body, z0: f32, vz0: f32, fall_height_perk: bool, t: &MoveTuning) -> Option<Landing> {
    let l = &t.land;
    let g = t.gravity;
    // The engine's quadratic: when, from the frame's start, the feet reached the ground, and the
    // speed there.
    let a = -g * 0.5;
    let c = z0 - body.pos.z;
    let den = vz0 * vz0 - c * a * 4.0;
    if den < 0.0 {
        return None;
    }
    let time = (-vz0 - den.sqrt()) / (a * 2.0);
    let v = -(time * -g + vz0);
    let height = v * v / (g * 2.0) / ZH_PER_INCH;
    let damage = if fall_height_perk { 0 } else { l.fall_damage(height) };
    if damage == 0 {
        // A soft landing: from above 12 inches it slows and dips (the shorter ones are only the
        // footstep events 0x81/0x82).
        if height > l.dip_min_height {
            body.vel = body.vel * l.soft_scale;
            body.land_change = -l.dip(height);
            body.land_time = body.time;
        }
    } else {
        let mut scale = l.soft_scale;
        if damage < 100 {
            let hard = l.hard_ms_base + f32::from(damage) * l.hard_ms_per_damage;
            scale = if hard > l.hard_ms_max {
                body.pm_time = l.hard_ms_max;
                l.hard_min_scale
            } else {
                body.pm_time = hard;
                if hard >= l.hard_ms_base + 1000.0 {
                    l.hard_min_scale
                } else {
                    l.hard_scale - (hard - l.hard_ms_base) * 0.001 * l.hard_scale_drop
                }
            };
            body.hard_landing = true;
        }
        body.vel = body.vel * scale;
        let dip = l.pain_dip(damage);
        if dip > 0.0 {
            body.land_change = -dip;
            body.land_time = body.time;
        }
    }
    Some(Landing { height, damage })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bo2s_fall_damage_curve() {
        let l = LandTuning::from_tuning(&Tuning::default());
        assert_eq!(l.fall_damage(128.0), 0, "up to the minimum: none");
        assert_eq!(l.fall_damage(129.0), 0, "under one percent rounds to none");
        assert_eq!(l.fall_damage(214.0), 50, "(214 - 128) / 172 x 100 = 50");
        assert_eq!(l.fall_damage(299.9), 99);
        assert_eq!(l.fall_damage(300.0), 100);
        assert_eq!(l.fall_damage(1000.0), 100);
    }

    #[test]
    fn bo2s_landing_dip_sizes() {
        let l = LandTuning::from_tuning(&Tuning::default());
        assert_eq!(l.dip(12.0), 0.0);
        assert_eq!(l.dip(12.5), 4.0, "just past 12 inches: 4");
        assert_eq!(l.dip(39.0), 8.0, "a jump's own height: (27/26 + 1) x 4 = 8.15 -> 8");
        assert_eq!(l.dip(1000.0), 24.0, "capped at 24");
        assert_eq!(l.pain_dip(100), 24.0);
        // 50%: 214 inches back -> (202/26 + 1) x 4 = 35 -> 24.
        assert_eq!(l.pain_dip(50), 24.0);
        assert_eq!(l.pain_dip(1), l.dip(129.72));
        // The view: down over 150 ms, back by 450.
        assert_eq!(l.view_dip(-8.0, 75.0), -4.0);
        assert_eq!(l.view_dip(-8.0, 150.0), -8.0);
        assert_eq!(l.view_dip(-8.0, 300.0), -4.0);
        assert_eq!(l.view_dip(-8.0, 450.0), 0.0);
    }

    #[test]
    fn the_slowdown_factor_is_flat_then_ramps_to_one() {
        let l = LandTuning::from_tuning(&Tuning::default());
        let mut b = Body::new(crate::math::Vec3::ZERO, &MoveTuning::default());
        b.jumping = true;
        for (pm, want) in [(1800.0, 2.5), (1700.0, 2.5), (850.0, 1.75), (0.0, 1.0)] {
            b.pm_time = pm;
            assert!((slowdown_factor(&mut b, &l) - want).abs() < 1e-5, "{pm} ms: {want}");
        }
        b.pm_time = 2000.0;
        assert_eq!(slowdown_factor(&mut b, &l), 1.0, "a timer longer than a jump's is not the jump's");
        assert!(!b.jumping, "and it ends the jump state");
        let off = LandTuning { slowdown: false, ..l };
        b.jumping = true;
        b.pm_time = 1800.0;
        assert_eq!(slowdown_factor(&mut b, &off), 1.0, "jump_slowdownEnable 0");
    }
}
