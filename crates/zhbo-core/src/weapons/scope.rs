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
    let frac = if zooming_in { def.zoom.in_frac } else { def.zoom.out_frac };
    let mut z = pos_frac - (1.0 - frac);
    if z > 0.0 {
        z /= frac;
    }
    (z > RETICLE_ZOOM_MIN).then_some(z.min(1.0))
}

/// `MISSILE_GUIDANCE_TVGUIDED` (`guidedMissileType_t`): a TV-guided weapon never sets the HUD's
/// scoped bit.
pub const MISSILE_GUIDANCE_TVGUIDED: u32 = 6;

/// `CoD.BIT_IS_SCOPED` (visibility bit 28), as `FUN_0055a420` sets it: the overlay is up
/// (`FUN_005388d0` → [`reticle_zoom`]; its turret/vehicle branch has no counterpart here) and the
/// weapon is not TV-guided (`WeaponDef+0x770`, `guidedMissileType`). The HUD's minimap and compass
/// strip read it (`hud/compass.lua` `UpdateMinimapVisibility`: alpha 0/1, no fade).
pub fn scoped(def: &WeaponDef, pos_frac: f32, zooming_in: bool) -> bool {
    def.guided_missile_type != MISSILE_GUIDANCE_TVGUIDED && reticle_zoom(def, pos_frac, zooming_in).is_some()
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
    z / def.zoom.overlay_alpha_scale.max(1.0)
}

/// BO2's virtual screen, the space `overlayWidth`/`overlayHeight` are given in.
pub const VIRTUAL_W: f32 = 640.0;
pub const VIRTUAL_H: f32 = 480.0;

/// `FUN_004b4c70`'s quarter-art threshold (`DAT_00b12d84`, `_DAT_00b0c434`): an overlay no larger
/// than 320 × 240 is the top-left quarter of the scope, drawn four times mirrored about the centre.
pub const QUARTER_MAX: (f32, f32) = (320.0, 240.0);

/// The overlay size to draw `def` at, and whether it fell back: a weapon table without
/// `OverlaySize` (one written before the key existed) reads 0 × 0, which BO2's rule would draw as
/// nothing but fill — a black screen while scoped. A zero size counts as "not carried" and takes
/// the tuning pack's `ScopeOverlayWidth`/`ScopeOverlayHeight` (BO2's 480 × 480).
pub fn overlay_size(def: &WeaponDef) -> ((f32, f32), bool) {
    let (w, h) = def.overlay_size;
    if w > 0.0 && h > 0.0 {
        return ((w, h), false);
    }
    let t = crate::tuning::get();
    ((t.scope_overlay_width, t.scope_overlay_height), true)
}

/// Where the scope overlay goes on a `screen_w` × `screen_h` screen (`FUN_004b4c70`).
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayPlacement {
    /// Each draw of the art: the screen points of the image's top-left, top-right, bottom-right
    /// and bottom-left corners (`ClientHost::image_quad`'s order).
    pub quads: Vec<[(f32, f32); 4]>,
    /// The art's bounds `(x0, y0, x1, y1)`; `FUN_004b4840` fills the screen outside it.
    pub bounds: (f32, f32, f32, f32),
}

/// `FUN_004b4c70`'s placement of a `size` overlay (the def's `overlayWidth`, `overlayHeight`):
/// centred on the screen, both alignments `2` (centre), through `ScrPlace` (`FUN_0057d900`), whose
/// virtual-to-real scale is one factor for both axes, the 640 × 480 screen fitted inside the real
/// one. So the art keeps its aspect at every resolution: a 480 × 480 sniper scope is a square as
/// tall as the screen, never stretched to its width. A size within [`QUARTER_MAX`] draws its
/// quarter four times, the texture's (0, 0) corner outermost and (1, 1) at the centre
/// (`s0,t0,s1,t1` = `0,0,1,1` / `1,0,0,1` / `0,1,1,0` / `1,1,0,0`), spanning twice the size.
pub fn overlay_placement(size: (f32, f32), screen_w: f32, screen_h: f32) -> OverlayPlacement {
    let k = (screen_w / VIRTUAL_W).min(screen_h / VIRTUAL_H);
    let (w, h) = (size.0 * k, size.1 * k);
    let (cx, cy) = (screen_w * 0.5, screen_h * 0.5);
    let corners = |at: &dyn Fn(f32, f32) -> (f32, f32)| [at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0)];
    if size.0 <= QUARTER_MAX.0 && size.1 <= QUARTER_MAX.1 {
        let quads = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .iter()
            .map(|&(sx, sy): &(f32, f32)| corners(&|s, t| (cx + sx * w * (1.0 - s), cy + sy * h * (1.0 - t))))
            .collect();
        OverlayPlacement { quads, bounds: (cx - w, cy - h, cx + w, cy + h) }
    } else {
        let (x0, y0) = (cx - w * 0.5, cy - h * 0.5);
        OverlayPlacement { quads: vec![corners(&|s, t| (x0 + s * w, y0 + t * h))], bounds: (x0, y0, x0 + w, y0 + h) }
    }
}

/// `FUN_004b4840`: the screen outside the overlay's bounds, as `(x, y, w, h)` strips — left and
/// right full height, then top and bottom between them — each skipped when empty. BO2 draws them
/// with the overlay's own material and colour, sampling its edge (the sides stretch texel column
/// `s = 0`, top and bottom texel row `t = 0`).
pub fn overlay_fills(bounds: (f32, f32, f32, f32), screen_w: f32, screen_h: f32) -> Vec<(f32, f32, f32, f32)> {
    let (x0, y0, x1, y1) = bounds;
    let mut out = Vec::new();
    if x0 > 0.0 {
        out.push((0.0, 0.0, x0, screen_h));
    }
    if x1 < screen_w {
        out.push((x1, 0.0, screen_w - x1, screen_h));
    }
    if y0 > 0.0 {
        out.push((x0, 0.0, x1 - x0, y0));
    }
    if y1 < screen_h {
        out.push((x0, y1, x1 - x0, screen_h - y1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sniper(zin: f32, zout: f32) -> WeaponDef {
        WeaponDef { overlay_material: "scope_overlay_dsr50".into(), zoom: crate::weapons::Zoom { in_frac: zin, out_frac: zout, overlay_alpha_scale: 1.0, ..Default::default() }, ..Default::default() }
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
        d2.zoom.overlay_alpha_scale = 2.0;
        assert!((overlay_alpha(&d2, z) - z / 2.0).abs() < 1e-6);
        d2.zoom.overlay_alpha_scale = 0.5;
        assert!((overlay_alpha(&d2, z) - z).abs() < 1e-6, "the divisor is floored at 1");
        assert_eq!(reticle_zoom(&d, 0.0, true), None);
    }

    #[test]
    fn scoped_follows_the_zoom_except_tv_guided() {
        let mut d = sniper(0.0, 0.05);
        assert!(!scoped(&d, 1.0, true), "the first full-ADS frame: not yet");
        assert!(scoped(&d, 1.0, false));
        assert!(scoped(&d, 0.96, false));
        assert!(!scoped(&d, 0.9504, false));
        d.guided_missile_type = MISSILE_GUIDANCE_TVGUIDED;
        assert!(!scoped(&d, 1.0, false), "TV-guided: the overlay is up but the bit stays clear");
        assert!(reticle_zoom(&d, 1.0, false).is_some());
    }

    /// 16:9, 16:10, 4:3, 5:4, 21:9 and 32:9.
    const SCREENS: [(f32, f32); 7] = [(1280.0, 720.0), (1920.0, 1080.0), (3024.0, 1890.0), (1024.0, 768.0), (1280.0, 1024.0), (3440.0, 1440.0), (5120.0, 1440.0)];

    #[test]
    fn a_sniper_scope_is_round_at_every_aspect() {
        // The snipers' overlayWidth/Height are 480 x 480: one quad, square in pixels, centred, as
        // tall as the screen where the screen is at least 4:3. It once filled the whole screen,
        // stretching the scope's circle into an ellipse on anything but a square screen.
        for (w, h) in SCREENS {
            let p = overlay_placement((480.0, 480.0), w, h);
            assert_eq!(p.quads.len(), 1);
            let q = p.quads[0];
            let (qw, qh) = (q[1].0 - q[0].0, q[3].1 - q[0].1);
            assert!((qw - qh).abs() < 1e-3, "{w}x{h}: the circle's box is {qw} x {qh}");
            assert!(((q[0].0 + q[1].0) * 0.5 - w * 0.5).abs() < 1e-3 && ((q[0].1 + q[3].1) * 0.5 - h * 0.5).abs() < 1e-3, "centred");
            assert!((qh - h.min(w * 0.75)).abs() < 1e-3, "{w}x{h}: the 480-unit square fits the 640 x 480 screen");
            assert_eq!(p.bounds, (q[0].0, q[0].1, q[1].0, q[3].1));
        }
    }

    #[test]
    fn a_table_without_the_size_falls_back_to_bo2s_square() {
        // A stale weapon table reads 0 x 0: not carried, so the tuning pack's 480 x 480, never an
        // all-fill black screen.
        let mut d = sniper(0.0, 0.05);
        assert_eq!(overlay_size(&d), ((480.0, 480.0), true));
        let p = overlay_placement(overlay_size(&d).0, 1280.0, 720.0);
        assert_eq!(p.bounds, (280.0, 0.0, 1000.0, 720.0), "a height-fit square, not nothing");
        d.overlay_size = (320.0, 240.0);
        assert_eq!(overlay_size(&d), ((320.0, 240.0), false), "a carried size is used as is");
    }

    #[test]
    fn the_fills_cover_exactly_the_screen_outside_the_art() {
        for (w, h) in SCREENS {
            for size in [(480.0, 480.0), (640.0, 480.0), (320.0, 240.0), (200.0, 150.0)] {
                let p = overlay_placement(size, w, h);
                let (x0, y0, x1, y1) = p.bounds;
                let fills: f32 = overlay_fills(p.bounds, w, h).iter().map(|f| f.2 * f.3).sum();
                assert!((fills + (x1 - x0) * (y1 - y0) - w * h).abs() < 1.0, "{w}x{h} {size:?}");
            }
        }
        let full = overlay_placement((640.0, 480.0), 1024.0, 768.0);
        assert!(overlay_fills(full.bounds, 1024.0, 768.0).is_empty(), "a 640 x 480 overlay at 4:3 leaves nothing to fill");
    }

    #[test]
    fn a_quarter_overlay_mirrors_about_the_centre() {
        // `FUN_004b4c70`: 320 x 240 or less draws four times, the texture's (0, 0) corner at the
        // screen's outer corner and (1, 1) at the centre, uniformly scaled.
        let p = overlay_placement((200.0, 200.0), 1920.0, 1080.0);
        let k = 1080.0 / 480.0;
        let (cx, cy, r) = (960.0, 540.0, 200.0 * k);
        assert_eq!(p.quads.len(), 4);
        let origins: Vec<(f32, f32)> = p.quads.iter().map(|q| q[0]).collect();
        assert_eq!(origins, vec![(cx - r, cy - r), (cx + r, cy - r), (cx - r, cy + r), (cx + r, cy + r)]);
        for q in &p.quads {
            assert_eq!(q[2], (cx, cy), "every quarter's (1, 1) corner meets at the centre");
        }
        assert_eq!(p.bounds, (cx - r, cy - r, cx + r, cy + r));
    }
}
