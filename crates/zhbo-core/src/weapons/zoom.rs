/*
** Conquer Ops - Zero Hour gameplay core: BO2's ADS zoom — the view FOV while aiming in and out.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! BO2's ADS zoom, ported from the t6mp disassembly of `FUN_004a0380` (its decompile loses the
//! x87/SSE return; the function takes the player state in `ESI` and returns the FOV in `XMM0`),
//! called by the view-FOV function `FUN_004a0920`, which then applies `cg_fovMin`/`cg_fovScale`.
//!
//! * **Levels.** `FUN_008ec3f0` resolves `fAdsZoomFov1..3` (the variant's, each overridden by the
//!   last attachment differing from `defaultattachment`). Level 0 reads **fov3**, 1 fov2, 2 fov1,
//!   anything else fov3; the target is `min(level fov, hip fov)`. With
//!   `cg_useWeaponBasedVariableZoom` (default 1) the level is the weapon's inventory slot's, which
//!   only the Variable Zoom's melee press at full ADS steps, `(level + 1) % 2` (`FUN_008de340` →
//!   `FUN_008da3c0`): so a vzoom scope starts on fov3 (30°) and toggles with fov2 (12.5°).
//! * **Level change.** At full ADS on an overlay scope with distinct levels (`FUN_008dae80`), for
//!   125 ms after a change the target blends linearly from the old level's FOV.
//! * **Hybrid alt scope.** A fraction (`cg+0x84440`) rises at `1/altScopeADSTransInTime` while the
//!   held weapon is an ADS-only alt mode (`inventoryType == altmode && bAltWeaponAdsOnly`, the
//!   `dualoptic_` alt), and falls at `1/altScopeADSTransOutTime` otherwise; its frame step is capped
//!   at 0.1 s. The target blends toward the held weapon's fov1 by it.
//! * **The ADS lerp.** At `posFrac` 1 the FOV is the target, at 0 the hip FOV. Between, the
//!   zoom-in or zoom-out fraction (`fAdsZoomInFrac`/`OutFrac`, picked by the direction flag
//!   `cg+0x4ae38`: set at hip, cleared at full ADS, held between) is the share of the ADS lerp the
//!   FOV moves over — the *last* `frac` of it: `z = (posFrac − (1 − frac)) / frac`, the FOV
//!   `hip − (hip − target) · z` when `z > 0`, else the hip FOV. A fraction of 0 snaps at full ADS.
//!   It is the scope overlay's window (`FUN_004b4760`) without the overlay's 0.01 threshold.
//!
//! The FOV is render-only, so the client computes it from the sim's ADS fraction and zoom level;
//! the level (which eats a melee press) is sim state. BO2 draws the viewmodel with the scene FOV
//! (G4), so the gun magnifies with the zoom.

use super::def::WeaponDef;

/// `DAT_00b12968`: the alt-scope fraction's step is at most 0.1 s of frame time.
pub const ALT_SCOPE_DT_MAX: f32 = 0.1;
/// `DAT_00b11258` (125 ms) and `DAT_00b11254` (0.008/ms): a zoom level change blends over 125 ms.
pub const LEVEL_BLEND_TIME: f32 = 0.125;
/// The variable zoom's level count: `FUN_008da3c0` steps `(level + 1) % 2`.
pub const LEVELS: u8 = 2;
/// `ATTACHMENT_VZOOM` (`eAttachment` 0x1d): the weapon whose melee press at full ADS steps the
/// zoom level (`FUN_008de340` tests the combination for it with `FUN_008eb270`).
pub const VZOOM_KIND: u8 = 0x1d;

/// The FOV of zoom `level` (degrees at 4:3): level 0 → fov3, 1 → fov2, 2 → fov1, else fov3. A level
/// the table does not carry (0) falls back to fov1, the weapon's `ads_fov`.
pub fn level_fov(def: &WeaponDef, level: u8) -> f32 {
    let v = match level {
        1 => def.zoom.fov2,
        2 => def.ads_fov,
        _ => def.zoom.fov3,
    };
    if v > 0.0 {
        v
    } else {
        def.ads_fov
    }
}

/// `FUN_008dae00`: the weapon's zoom levels differ (the Variable Zoom; also the Hybrid's 30/50).
pub fn has_levels(def: &WeaponDef) -> bool {
    let (f1, f2, f3) = (def.ads_fov, level_fov(def, 1), level_fov(def, 0));
    f1 != f2 || f2 != f3
}

/// BO2's melee start (`FUN_008e2070`) is refused while aimed at all (`posFrac > 0`) with an overlay
/// reticle or distinct zoom levels.
pub fn melee_blocked(def: &WeaponDef, ads: f32) -> bool {
    ads > 0.0 && (def.overlay_reticle != 0 || has_levels(def))
}

/// The share of the zoom applied at ADS fraction `pos` with zoom fraction `frac` (the in or out
/// one): 0 at the hip, 1 at full ADS, linear over the lerp's last `frac`.
pub fn window(pos: f32, frac: f32) -> f32 {
    if pos >= 1.0 {
        return 1.0;
    }
    if pos <= 0.0 {
        return 0.0;
    }
    let z = pos - (1.0 - frac);
    if z > 0.0 && frac > 0.0 {
        (z / frac).min(1.0)
    } else {
        0.0
    }
}

/// The sim's variable-zoom state for the held weapon: its level, the level it changed from and the
/// seconds since (`ps+0x5d0`, `ps+0x5d4`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Level {
    pub level: u8,
    pub from: u8,
    pub since: f32,
}

crate::statehash::state_hash!(struct Level { level, from, since });

/// The client's render-only zoom state: the hybrid's alt-scope fraction and the zoom direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomView {
    /// `cg+0x84440`.
    pub alt_frac: f32,
    /// `cg+0x4ae38`: zooming in (set at the hip, cleared at full ADS).
    pub zooming_in: bool,
}

impl Default for ZoomView {
    fn default() -> Self {
        Self { alt_frac: 0.0, zooming_in: true }
    }
}

impl ZoomView {
    /// One frame of `dt` seconds: the alt-scope fraction moves toward the ADS-only alt mode
    /// (`on_alt`) or away, and the direction flag follows the lerp's ends.
    pub fn step(&mut self, dt: f32, def: &WeaponDef, on_alt: bool, pos: f32) {
        let dt = dt.min(ALT_SCOPE_DT_MAX);
        if on_alt && def.traits.alt_ads_only {
            if self.alt_frac < 1.0 {
                self.alt_frac = rate_step(self.alt_frac, dt, def.zoom.alt_scope_in_time);
            }
        } else if self.alt_frac > 0.0 {
            self.alt_frac = rate_step(self.alt_frac, -dt, def.zoom.alt_scope_out_time);
        }
        if pos >= 1.0 {
            self.zooming_in = false;
        } else if pos <= 0.0 {
            self.zooming_in = true;
        }
    }

    /// The view FOV (degrees at 4:3) for hip FOV `hip`, ADS fraction `pos` and the sim's zoom
    /// level, before `cg_fovMin` (see [`view_fov`]).
    pub fn fov(&self, hip: f32, def: &WeaponDef, pos: f32, level: Level) -> f32 {
        let mut target = target_fov(def, level.level, hip);
        // The level change blend (`FUN_008dae80`: full ADS, an overlay scope, distinct levels).
        let overlay = !def.overlay_material.is_empty();
        if pos >= 1.0 && overlay && has_levels(def) && level.level != level.from && level.since < LEVEL_BLEND_TIME {
            let old = target_fov(def, level.from, hip);
            let s = (level.since / LEVEL_BLEND_TIME).clamp(0.0, 1.0);
            target = old * (1.0 - s) + target * s;
        }
        if self.alt_frac > 0.0 {
            target = target * (1.0 - self.alt_frac) + target_fov(def, 2, hip) * self.alt_frac;
        }
        let frac = if self.zooming_in { def.zoom.in_frac } else { def.zoom.out_frac };
        hip - (hip - target) * window(pos, frac)
    }
}

/// `min(level fov, hip)`; a weapon without a zoom FOV (none carried) keeps the hip FOV.
fn target_fov(def: &WeaponDef, level: u8, hip: f32) -> f32 {
    let v = level_fov(def, level);
    if v > 0.0 {
        v.min(hip)
    } else {
        hip
    }
}

/// `frac + dt / time`, clamped to 0..1 (a time of 0 moves it at once, as BO2's divide by zero does).
fn rate_step(frac: f32, dt: f32, time: f32) -> f32 {
    let v = if time > 0.0 { frac + dt / time } else { frac + dt.signum() };
    v.clamp(0.0, 1.0)
}

/// The final view FOV (`FUN_004a0920`): `max(cg_fovMin, fov × cg_fovScale)`.
pub fn view_fov(fov: f32, fov_scale: f32, fov_min: f32) -> f32 {
    (fov * fov_scale).max(fov_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(f: impl FnOnce(&mut WeaponDef)) -> WeaponDef {
        let mut d = WeaponDef { id: "test_mp".into(), ads_fov: 50.0, ..Default::default() };
        d.zoom.fov2 = 50.0;
        d.zoom.fov3 = 50.0;
        f(&mut d);
        d
    }

    fn at(v: &ZoomView, d: &WeaponDef, pos: f32) -> f32 {
        v.fov(65.0, d, pos, Level::default())
    }

    #[test]
    fn a_rifle_zooms_over_the_last_half_in_and_the_first_forty_percent_out() {
        // The AN-94's iron sights: 50°, in 0.5, out 0.4.
        let d = def(|d| (d.zoom.in_frac, d.zoom.out_frac) = (0.5, 0.4));
        let mut v = ZoomView::default();
        assert_eq!(at(&v, &d, 0.0), 65.0);
        assert_eq!(at(&v, &d, 0.5), 65.0, "no zoom over the first half of the lerp");
        assert!((at(&v, &d, 0.75) - 57.5).abs() < 1e-4, "halfway through the window: {}", at(&v, &d, 0.75));
        assert_eq!(at(&v, &d, 1.0), 50.0);
        // Full ADS clears the direction flag: aiming out reads the out fraction, so the FOV holds
        // the zoom until the lerp is back to 0.6 and then widens over the rest.
        v.step(0.016, &d, false, 1.0);
        assert!(!v.zooming_in);
        assert!((at(&v, &d, 0.8) - 57.5).abs() < 1e-4, "out: {}", at(&v, &d, 0.8));
        assert_eq!(at(&v, &d, 0.6), 65.0);
        assert_eq!(at(&v, &d, 0.5), 65.0);
        // Back at the hip the flag is set again.
        v.step(0.016, &d, false, 0.0);
        assert!(v.zooming_in);
    }

    #[test]
    fn a_zero_fraction_snaps_at_full_ads_and_a_sniper_fades_out_in_its_last_five_percent() {
        // A base sniper: 15°, in 0, out 0.05.
        let d = def(|d| {
            (d.ads_fov, d.zoom.fov2, d.zoom.fov3) = (15.0, 15.0, 15.0);
            (d.zoom.in_frac, d.zoom.out_frac) = (0.0, 0.05);
        });
        let mut v = ZoomView::default();
        assert_eq!(at(&v, &d, 0.999), 65.0, "the hip FOV until the lerp ends");
        assert_eq!(at(&v, &d, 1.0), 15.0);
        v.step(0.016, &d, false, 1.0);
        assert!((at(&v, &d, 0.975) - 40.0).abs() < 1e-3, "{}", at(&v, &d, 0.975));
        assert_eq!(at(&v, &d, 0.95), 65.0);
    }

    #[test]
    fn the_levels_read_fov3_first_and_the_target_never_widens_the_view() {
        // vzoom: fov1 15 (the variant's), fov2 12.5, fov3 30.
        let d = def(|d| (d.ads_fov, d.zoom.fov2, d.zoom.fov3) = (15.0, 12.5, 30.0));
        assert_eq!(level_fov(&d, 0), 30.0);
        assert_eq!(level_fov(&d, 1), 12.5);
        assert_eq!(level_fov(&d, 2), 15.0);
        assert_eq!(level_fov(&d, 7), 30.0);
        assert!(has_levels(&d));
        let v = ZoomView::default();
        assert_eq!(v.fov(65.0, &d, 1.0, Level { level: 1, from: 1, since: 9.0 }), 12.5);
        // A zoom wider than the hip FOV is the hip FOV (`min`).
        let wide = def(|d| (d.ads_fov, d.zoom.fov2, d.zoom.fov3) = (80.0, 80.0, 80.0));
        assert_eq!(v.fov(65.0, &wide, 1.0, Level::default()), 65.0);
        assert!(!has_levels(&wide));
        // A table without the zoom line falls back to fov1.
        let old = def(|d| (d.zoom.fov2, d.zoom.fov3) = (0.0, 0.0));
        assert_eq!(level_fov(&old, 0), 50.0);
    }

    #[test]
    fn a_level_change_blends_over_125_ms_on_an_overlay_scope() {
        let d = def(|d| {
            (d.ads_fov, d.zoom.fov2, d.zoom.fov3) = (15.0, 12.5, 30.0);
            d.overlay_material = "scope_overlay_dsr50".into();
        });
        let v = ZoomView { zooming_in: false, ..Default::default() };
        let l = |since| Level { level: 1, from: 0, since };
        assert_eq!(v.fov(65.0, &d, 1.0, l(0.0)), 30.0);
        assert!((v.fov(65.0, &d, 1.0, l(0.0625)) - 21.25).abs() < 1e-4);
        assert_eq!(v.fov(65.0, &d, 1.0, l(0.125)), 12.5);
        // No blend below full ADS (`FUN_008dae80` needs posFrac 1) or without an overlay.
        let plain = def(|d| (d.ads_fov, d.zoom.fov2, d.zoom.fov3) = (15.0, 12.5, 30.0));
        assert_eq!(v.fov(65.0, &plain, 1.0, l(0.0)), 12.5);
    }

    #[test]
    fn the_hybrid_alt_scope_blends_to_fov1_at_its_own_times() {
        // The Hybrid: fov1 30 (the alt), fov3 50 (the weapon's own level 0), in 0.15 s, out 0.08 s.
        let d = def(|d| {
            (d.ads_fov, d.zoom.fov2, d.zoom.fov3) = (30.0, 50.0, 50.0);
            (d.zoom.alt_scope_in_time, d.zoom.alt_scope_out_time) = (0.15, 0.08);
            d.traits.alt_ads_only = true;
        });
        let mut v = ZoomView::default();
        assert_eq!(at(&v, &d, 1.0), 50.0, "the main mode aims at fov3");
        v.step(0.075, &d, true, 1.0);
        assert!((v.alt_frac - 0.5).abs() < 1e-5);
        assert!((at(&v, &d, 1.0) - 40.0).abs() < 1e-4);
        // A long frame steps at most 0.1 s.
        v.step(1.0, &d, true, 1.0);
        assert_eq!(v.alt_frac, 1.0);
        assert_eq!(at(&v, &d, 1.0), 30.0);
        v.step(0.04, &d, false, 1.0);
        assert!((v.alt_frac - 0.5).abs() < 1e-5, "out over 0.08 s: {}", v.alt_frac);
        // Only an ADS-only alt mode raises it.
        let mut g = ZoomView::default();
        let gl = def(|d| d.zoom.alt_scope_in_time = 0.15);
        g.step(0.05, &gl, true, 1.0);
        assert_eq!(g.alt_frac, 0.0);
    }

    #[test]
    fn melee_is_refused_while_aiming_a_scope_or_a_multi_level_zoom() {
        let rifle = def(|_| {});
        assert!(!melee_blocked(&rifle, 1.0));
        let sniper = def(|d| d.overlay_reticle = 1);
        assert!(melee_blocked(&sniper, 0.01));
        assert!(!melee_blocked(&sniper, 0.0));
        let vz = def(|d| d.zoom.fov2 = 12.5);
        assert!(melee_blocked(&vz, 0.5));
        assert_eq!(view_fov(5.0, 1.0, 10.0), 10.0);
        assert_eq!(view_fov(50.0, 2.0, 10.0), 100.0);
    }
}
