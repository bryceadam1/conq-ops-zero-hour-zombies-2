//! BO2's reticle zoom: when a weapon with a 2D scope overlay is "looking through the scope".
//!
//! One rule drives two things (read-only Ghidra, t6mp): the overlay's alpha (`FUN_004b51e0`) and
//! whether the viewmodel is drawn at all (`FUN_005374c0` passes `drawGun = 0` to `FUN_00536f30`,
//! which then skips the DObj's scene add — the whole viewmodel, arms included; the animation and
//! its notetracks keep running). BO2 never drops the scope *part*: the ADS part list keeps it, and
//! the gun as a whole stops drawing under the overlay.

use super::WeaponDef;

/// `DAT_00c73410`: the reticle zoom must exceed this for the overlay to count as up.
pub const RETICLE_ZOOM_MIN: f32 = 0.01;

/// `FUN_004b4760`: the reticle zoom at weapon position fraction `pos_frac`, or `None` when the
/// overlay is not up. Only a weapon with an overlay material or overlay reticle has one. The window
/// is the weapon's `fAdsZoomInFrac` while aiming in ([`zooming_in`] true) and `fAdsZoomOutFrac`
/// otherwise: `z = (pos_frac − (1 − frac)) / frac`, up when `z > 0.01`, clamped to 1. A zero
/// window never opens (the base snipers' in-fraction is 0: their overlay comes up only once the
/// direction flips at full ADS).
pub fn reticle_zoom(def: &WeaponDef, pos_frac: f32, zooming_in: bool) -> Option<f32> {
    if def.overlay_material.is_empty() && def.overlay_reticle == 0 {
        return None;
    }
    if pos_frac == 0.0 {
        return None;
    }
    let frac = if zooming_in { def.ads_zoom_in_frac } else { def.ads_zoom_out_frac };
    let mut z = pos_frac - (1.0 - frac);
    if z > 0.0 {
        z /= frac;
    }
    (z > RETICLE_ZOOM_MIN).then_some(z.min(1.0))
}

/// `FUN_0052e170` (`cg+0x4ae38`): the zoom direction. It becomes "in" when the position fraction is
/// at 0 (hip) and "out" at 1 (full ADS), and holds in between. BO2 updates it after the viewmodel's
/// draw test and before the 2D overlay draws.
pub fn zooming_in(prev: bool, pos_frac: f32) -> bool {
    if pos_frac == 1.0 {
        false
    } else if pos_frac == 0.0 {
        true
    } else {
        prev
    }
}

/// The overlay's alpha at reticle zoom `z` (`FUN_004b51e0`: `z / max(fOverlayAlphaScale, 1)`).
pub fn overlay_alpha(def: &WeaponDef, z: f32) -> f32 {
    z / def.overlay_alpha_scale.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sniper(zin: f32, zout: f32) -> WeaponDef {
        WeaponDef { overlay_material: "scope_overlay_dsr50".into(), ads_zoom_in_frac: zin, ads_zoom_out_frac: zout, overlay_alpha_scale: 1.0, ..Default::default() }
    }

    #[test]
    fn only_overlay_weapons_zoom() {
        let mut d = sniper(0.016, 0.05);
        d.overlay_material.clear();
        assert_eq!(reticle_zoom(&d, 1.0, false), None);
        d.overlay_reticle = 1;
        assert_eq!(reticle_zoom(&d, 1.0, false), Some(1.0), "an overlay reticle alone also counts");
    }

    #[test]
    fn the_base_snipers_open_only_after_the_flip() {
        // dsr50: in 0, out 0.05 (the install's values).
        let d = sniper(0.0, 0.05);
        assert_eq!(reticle_zoom(&d, 0.999, true), None);
        assert_eq!(reticle_zoom(&d, 1.0, true), None, "the in-window is empty: still drawn at the first full-ADS frame");
        let dir = zooming_in(true, 1.0);
        assert!(!dir);
        assert_eq!(reticle_zoom(&d, 1.0, dir), Some(1.0));
        // Aiming out: up through the last 5 %, gone below 0.95 + 0.01 · 0.05.
        assert!(reticle_zoom(&d, 0.96, false).is_some());
        assert_eq!(reticle_zoom(&d, 0.9504, false), None);
        assert!(!zooming_in(false, 0.5), "held between the extremes");
        assert!(zooming_in(false, 0.0));
    }

    #[test]
    fn the_ir_window_fades_in() {
        let d = sniper(0.016, 0.05);
        assert_eq!(reticle_zoom(&d, 0.984, true), None);
        let z = reticle_zoom(&d, 0.992, true).expect("half way through the window");
        assert!((z - 0.5).abs() < 1e-3, "{z}");
        assert!((overlay_alpha(&d, z) - z).abs() < 1e-6);
        let mut d2 = d.clone();
        d2.overlay_alpha_scale = 2.0;
        assert!((overlay_alpha(&d2, z) - z / 2.0).abs() < 1e-6);
        d2.overlay_alpha_scale = 0.5;
        assert!((overlay_alpha(&d2, z) - z).abs() < 1e-6, "the divisor is floored at 1");
        assert_eq!(reticle_zoom(&d, 0.0, true), None);
    }
}
