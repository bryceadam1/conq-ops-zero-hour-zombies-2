//! The centre of the screen: the crosshair, the hit marker and the score popup.
//!
//! * Crosshair (Ghidra G6, `t6mp` 0x004b5d70/0x004b5ea0/0x004b5e40): four side pieces pushed out
//!   from the centre by the weapon's spread, `tan(spread°) · 240 / tan(fovY/2)` in the 480-line
//!   virtual space; alpha `1 − aimSpreadScale`, floored at `cg_crosshairAlphaMin` 0.5; gone once
//!   aiming down the sights. Under the scope overlay the alpha first takes BO2's fade,
//!   `1 − zoom / max(fOverlayAlphaScale, 1)` (`FUN_004b5ea0`), before the floor; the optic reticle
//!   art is not drawn there at all (the viewmodel, which carries the sight, is not drawn either).
//!   The weapon's own `reticleSide` art, `iReticleSideSize` and
//!   `iReticleMinOfs` are not in the weapon table yet, so the pieces are drawn strokes.
//! * Hit marker (G6, `maps/mp/gametypes/_damagefeedback.gsc`): `damage_feedback` 24 x 48 at
//!   (−12, −12) from the centre in the 480-line hudelem space, alpha 1 fading to 0 over 1 s; BO2 has
//!   no separate kill marker.
//! * Score popup (`hud/scorepopup.lua:10-12, 454-523`): "+N" in `Condensed`, its box 40 right of
//!   centre and 75..45 above it, for `DisplayTime` 1000 ms. The additive glow layers
//!   (`CoD.AdditiveTextOverlay`) are not drawn.

use super::frame::{argb, Lui};
use super::*;
use crate::lui::font::Family;
use crate::lui::Place;

/// G6: `cg_crosshairAlphaMin`.
pub const CROSSHAIR_ALPHA_MIN: f32 = 0.5;
/// The hit marker's fade (`fadeovertime(1)`).
pub const HITMARKER_FADE: f32 = 1.0;
/// The score popup's `DisplayTime`.
pub const SCORE_DISPLAY: f32 = 1.0;

/// The crosshair's push from the centre in pixels: G6's 480-line value scaled to the screen.
pub fn crosshair_offset(spread_deg: f32, hfov: f32, w: f32, h: f32) -> f32 {
    let tan_half_v = (hfov * 0.5).tan() * h / w.max(1.0);
    let px480 = spread_deg.to_radians().tan() * 240.0 / tan_half_v.max(1e-4);
    px480 * h / 480.0
}

/// The seven stock reticle colours (`lua-dump/patch_mp/mp/attachmenttable.csv:68-74`; the
/// localized names are Red, Green, Blue, Purple, Teal, Yellow, Orange). The RGB is the grid
/// swatch and `draw_crosshair`'s tint for the optic art.
pub fn reticle_color_rgb(id: &str) -> Option<(f32, f32, f32)> {
    match id {
        "reticle_color_1" => Some((1.0, 0.16, 0.16)),
        "reticle_color_2" => Some((0.2, 1.0, 0.2)),
        "reticle_color_3" => Some((0.25, 0.45, 1.0)),
        "reticle_color_4" => Some((0.65, 0.25, 1.0)),
        "reticle_color_5" => Some((0.2, 1.0, 0.9)),
        "reticle_color_6" => Some((1.0, 0.9, 0.2)),
        "reticle_color_7" => Some((1.0, 0.55, 0.1)),
        _ => None,
    }
}

/// `zoom`: BO2's reticle zoom this frame ([`crate::weapons::scope::reticle_zoom`]), `None` when
/// the scope overlay is not up.
pub(super) fn draw_crosshair(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, client: &Client, p: &PlayerSim, zoom: Option<f32>) {
    // Unarmed (knife only): there is no gun to aim, so no crosshair.
    if p.arsenal.is_unarmed() {
        return;
    }
    let (cx, cy) = (w * 0.5, h * 0.5);
    let ads = p.arsenal.ads;
    let wpn = p.arsenal.current();
    // Aiming through an optic: BO2's reticle art on the line of sight (BO2 projects the reticle
    // to infinity, so it stays centred). Iron sights have none.
    let optic = wpn.def.attachments.iter().any(|a| OPTICS.contains(&a.as_str()));
    if ads > 0.6 && !optic {
        return;
    }
    if ads > 0.6 {
        // The overlay replaces the sight: an IR or VZoom sniper's optic draws no 2D reticle on it.
        if zoom.is_some() {
            return;
        }
        let size = 30.0 * s;
        // The chosen stock reticle through the optic (`reticle_3` -> the optic's `_reticle_04`),
        // tinted by the chosen colour (`reticle_color_1` is Red). Without a choice, the red dot.
        let art = crate::weapons::optic_of(&wpn.def.attachments)
            .and_then(|o| crate::weapons::reticle_art(o, &p.reticle));
        let rgb = reticle_color_rgb(&p.reticle_color).unwrap_or((1.0, 1.0, 1.0));
        let col = argb(rgb, 1.0);
        let name = art.unwrap_or_else(|| sprites::RED_DOT.to_string());
        prims.push(Prim::Image {
            name,
            x: cx - size * 0.5,
            y: cy - size * 0.5,
            w: size,
            h: size,
            argb: if p.reticle.is_empty() { 0xFFFF_FFFF } else { col },
            fallback: vec![Prim::Rect { x: cx - 2.0 * s, y: cy - 2.0 * s, w: 4.0 * s, h: 4.0 * s, argb: col }],
        });
        return;
    }
    if p.body.sprinting || p.arsenal.is_busy() {
        return;
    }
    let stance = match p.body.stance {
        crate::movement::Stance::Stand => 0,
        crate::movement::Stance::Crouch => 1,
        crate::movement::Stance::Prone => 2,
    };
    let spread = wpn.spread_deg(stance, ads);
    let gap = crosshair_offset(spread, client.last_hfov, w, h);
    // aimSpreadScale: how far the bloom has opened the cone between its stance min and max.
    let (min, max) = wpn.def.hip_spread[stance];
    let aim_spread = if max > min { (wpn.bloom / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let fade = zoom.map_or(1.0, |z| 1.0 - crate::weapons::scope::overlay_alpha(wpn.def, z));
    let alpha = (fade * (1.0 - aim_spread)).max(CROSSHAIR_ALPHA_MIN);
    let col = argb((1.0, 1.0, 1.0), alpha);
    let len = 10.0 * s;
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let (x0, y0) = (cx + dx * gap, cy + dy * gap);
        let (x1, y1) = (cx + dx * (gap + len), cy + dy * (gap + len));
        prims.push(Prim::Line { x0, y0, x1, y1, width: 2.0 * s, argb: col });
    }
}

/// The hit marker: `t` counts down from [`HITMARKER_FADE`].
pub(super) fn draw_hitmarker(prims: &mut Vec<Prim>, w: f32, h: f32, client: &Client) {
    let t = client.hud.hitmarker;
    if t <= 0.0 {
        return;
    }
    let u = h / 480.0;
    let (cx, cy) = (w * 0.5, h * 0.5);
    let col = argb((1.0, 1.0, 1.0), (t / HITMARKER_FADE).min(1.0));
    let mut fallback = Vec::new();
    let (inner, outer) = (4.0 * u, 9.0 * u);
    for (dx, dy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        fallback.push(Prim::Line { x0: cx + dx * inner, y0: cy + dy * inner, x1: cx + dx * outer, y1: cy + dy * outer, width: 1.5 * u, argb: col });
    }
    prims.push(Prim::Image { name: crate::lui::materials::image("damage_feedback"), x: cx - 12.0 * u, y: cy - 12.0 * u, w: 24.0 * u, h: 48.0 * u, argb: col, fallback });
}

/// The score popup over the centre.
pub(super) fn draw_score_popup(l: &Lui, prims: &mut Vec<Prim>, client: &Client) {
    let t = client.hud.score_popup;
    if t <= 0.0 || client.hud.score_points == 0 {
        return;
    }
    let r = Place::new((true, true, 40.0, 40.0), (false, false, -75.0, -75.0 + 30.0)).resolve(l.screen());
    l.text(prims, r, Family::Condensed, &format!("+{}", client.hud.score_points), None, (true, true), (1.0, 1.0, 1.0), 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosshair_offset_is_g6s_480_line_value() {
        // A 65° horizontal FOV at 4:3: tan(vfov/2) = tan(32.5°) * 0.75. A 3° spread pushes
        // tan(3°) * 240 / that = 37.7 lines, scaled to 720 px.
        let px = crosshair_offset(3.0, 65f32.to_radians(), 960.0, 720.0);
        let expect = 3f32.to_radians().tan() * 240.0 / (32.5f32.to_radians().tan() * 0.75) * 1.5;
        assert!((px - expect).abs() < 1e-3, "{px} vs {expect}");
    }

    #[test]
    fn score_popup_sits_right_of_and_above_the_centre() {
        let _f = crate::lui::font::lock();
        crate::lui::font::load("");
        let l = Lui::new(1280.0, 720.0);
        let mut client = Client::default();
        client.hud.score_popup = 0.5;
        client.hud.score_points = 100;
        let mut prims = Vec::new();
        draw_score_popup(&l, &mut prims, &client);
        // No fonts loaded in the test: the engine-font fallback, centred at 640 + 40.
        assert!(matches!(&prims[0], Prim::Text { x, y, text, .. } if *x == 680.0 && *y == 285.0 && text == "+100"));
    }
}
