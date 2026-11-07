//! The centre of the screen: the crosshair, the hit marker and the score popup.
//!
//! * Crosshair (Ghidra G6; [`crosshair`]): the weapon's `reticleSide` quads at `iReticleSideSize`
//!   and its `reticleCenter`, pushed out by the spread the next shot would fly with
//!   ([`crate::weapons::aim_spread`]), at least `iReticleMinOfs`; shrinking to half through the
//!   `fAdsCrosshairInFrac`/`OutFrac` window and gone at full ADS unless `bKeepCrosshairWhenADS`;
//!   alpha `1 − aimSpreadScale` times the overlay's fade, floored at `cg_crosshairAlphaMin`; the
//!   team colour over a target with `crosshairColorChange` ([`super::feedback::Target`]).
//! * Hit marker (G6, `maps/mp/gametypes/_damagefeedback.gsc`): `damage_feedback` 24 x 48 at
//!   (−12, −12) from the centre in the 480-line hudelem space, alpha 1 fading to 0 over 1 s, or its
//!   `_flak`/`_tac` variant when the victim's perk took the hit; BO2 has no separate kill marker.
//! * Score popup (`hud/scorepopup.lua:10-12, 454-523`): "+N" in `Condensed`, its box 40 right of
//!   centre and 75..45 above it, for `DisplayTime` 1000 ms. The additive glow layers
//!   (`CoD.AdditiveTextOverlay`) are not drawn.

use super::frame::{argb, Lui};
use super::*;
use crate::lui::font::Family;
use crate::lui::{Place, Rect};

/// G6: `cg_crosshairAlphaMin`.
pub const CROSSHAIR_ALPHA_MIN: f32 = 0.5;
/// The hit marker's fade (`fadeovertime(1)`).
pub const HITMARKER_FADE: f32 = 1.0;
/// The score popup's `DisplayTime`.
pub const SCORE_DISPLAY: f32 = 1.0;
/// How long the weapon hint stays (`FUN_004ddc80`: cleared at its start + 0x708 ms).
pub const HINT_TIME: f32 = 1.8;
/// The hint's fade-in cycle (`FUN_004ddc80`: alpha `((now - start) % 600) / 600`).
pub const HINT_CYCLE: f32 = 0.6;
/// `invalidcmdhint.menu` (`common_mp.ff`'s `ui_mp/hud_*.txt`): `rect 0 40` (480-line space) with
/// align 9/9 and the item's `textscale 0.275` in `normalFont` (`textfont 1`), which IW menus draw
/// `48 * scale` lines tall. Align 9 is the middle of the user safe area on both axes
/// (`FUN_0057d900`, the menus' screen placement: case 9 is `(safeLeft + safeRight) / 2`, 8 the safe
/// left/top, 10 the safe right/bottom) — the screen centre for a centred safe area.
const HINT_BELOW_CENTRE_480: f32 = 40.0;
const HINT_HEIGHT_480: f32 = 48.0 * 0.275;

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
    // The W3D fallback has no lens reticle: aiming through an optic, the optic's art on the line of
    // sight stands in (the native viewmodel draws BO2's own on the lens, projected to infinity, and
    // BO2 has no 2D copy over it; the overlay replaces the sight).
    let optic = wpn.def.attachments.iter().any(|a| OPTICS.contains(&a.as_str()));
    if ads > 0.6 && optic && !crate::nativevm::lens_reticle_drawn() && zoom.is_none() {
        let size = 30.0 * s;
        // The held slot's chosen reticle through the optic (`reticle_3` -> `reflex_3`, the grid
        // cell's art; the Hybrid Optic's first sight), tinted by the chosen colour
        // (`reticle_color_1` is Red). Without a choice, the red dot.
        let reticle = p.held_reticle();
        let art = crate::weapons::optic_of(&wpn.def.attachments).and_then(|o| crate::weapons::reticle_art(o, reticle).into_iter().next());
        let rgb = reticle_color_rgb(p.held_reticle_color()).unwrap_or((1.0, 1.0, 1.0));
        let col = argb(rgb, 1.0);
        let name = art.unwrap_or_else(|| sprites::RED_DOT.to_string());
        prims.push(Prim::Image {
            name,
            x: cx - size * 0.5,
            y: cy - size * 0.5,
            w: size,
            h: size,
            argb: if reticle.is_empty() { 0xFFFF_FFFF } else { col },
            fallback: vec![Prim::Rect { x: cx - 2.0 * s, y: cy - 2.0 * s, w: 4.0 * s, h: 4.0 * s, argb: col }],
        });
    }
    if p.body.sprinting || p.arsenal.is_busy() {
        return;
    }
    let a = &p.arsenal;
    let look = Crosshair {
        def: wpn.def,
        ads,
        zooming_in: client.hud.lui.ads_zoom_in,
        aim_spread: a.aim_spread.fraction(),
        // The cone a hip shot would fly with now (`FUN_004b5d70`: the hip lerp by
        // `aimSpreadScale` over `BG_GetSpreadForWeapon`'s stance-blended, perk-scaled min..max) —
        // the same numbers the sim fires with ([`crate::weapons::aim_spread`]).
        spread: crate::weapons::aim_spread::AimSpread::shot_spread(a.aim_spread.scale, a.spread_cone(a.view_height()), wpn.def, 0.0),
        hfov: client.last_hfov,
        zoom,
        rgb: if wpn.def.reticle.color_change { client.feedback.target.colour(client.hud.lui.clock) } else { (1.0, 1.0, 1.0) },
    };
    crosshair(prims, w, h, &look);
}

/// What BO2's crosshair draw reads this frame.
pub struct Crosshair<'a> {
    pub def: &'a crate::weapons::WeaponDef,
    /// The weapon position fraction and BO2's zoom direction (`cg+0x4ae38`).
    pub ads: f32,
    pub zooming_in: bool,
    /// `aimSpreadScale / 255` and the cone it gives now (degrees).
    pub aim_spread: f32,
    pub spread: f32,
    /// The horizontal field of view (radians).
    pub hfov: f32,
    /// The overlay's reticle zoom (`FUN_004b4760`), `None` when it is not up.
    pub zoom: Option<f32>,
    /// The team colour (`FUN_004b52a0`), white without `crosshairColorChange`.
    pub rgb: (f32, f32, f32),
}

/// The scale and the downward slide (480-line units) the ADS window gives the crosshair
/// (`FUN_004b5ba0`): through the weapon's `fAdsCrosshairInFrac` (aiming in) or `OutFrac` window
/// the pieces shrink to half and slide by the view's `fAdsAimPitch`; none with
/// `bKeepCrosshairWhenADS` (`FUN_004b64a0` skips it).
pub fn ads_shrink(c: &Crosshair) -> (f32, f32) {
    let r = &c.def.reticle;
    if c.ads == 0.0 || r.keep_when_ads {
        return (1.0, 0.0);
    }
    let frac = if c.zooming_in { r.ads_in_frac } else { r.ads_out_frac };
    let t = c.ads - (1.0 - frac);
    if t <= 0.0 || frac <= 0.0 {
        return (1.0, 0.0);
    }
    let t = t / frac;
    let tan_half_v = (c.hfov * 0.5).tan() * 0.75;
    (1.0 - t * 0.5, c.def.view_pos.ads_aim_pitch.to_radians().tan() * (t * 240.0) / tan_half_v.max(1e-4))
}

/// BO2's crosshair (Ghidra G6; `FUN_004b64a0` → `FUN_004b5ea0`, `FUN_004b5c40`): the weapon's four
/// `reticleSide` quads at `iReticleSideSize`, turned 0/90/180/270° about their centres and pushed
/// out by the spread, at least `iReticleMinOfs`, less `size · fHipReticleSidePos`; its
/// `reticleCenter` at `iReticleCenterSize`; alpha `(1 − aimSpreadScale/255) · fade`, floored at
/// `cg_crosshairAlphaMin`, `fade` the overlay's (`1 − zoom / max(fOverlayAlphaScale, 1)`). At full
/// ADS it is gone unless the weapon has `bKeepCrosshairWhenADS`. All in the 480-line virtual space.
pub fn crosshair(prims: &mut Vec<Prim>, w: f32, h: f32, c: &Crosshair) {
    let r = &c.def.reticle;
    if c.ads >= 1.0 && !r.keep_when_ads {
        return;
    }
    let u = h / 480.0;
    let (scale, slide) = ads_shrink(c);
    let (cx, cy) = (w * 0.5, h * 0.5 + slide * u);
    let fade = c.zoom.map_or(1.0, |z| 1.0 - crate::weapons::scope::overlay_alpha(c.def, z));
    let alpha = ((1.0 - c.aim_spread) * fade).max(CROSSHAIR_ALPHA_MIN);
    let col = argb(c.rgb, alpha);
    let tan_half_v = (c.hfov * 0.5).tan() * h / w.max(1.0);
    let size = r.side_size * scale;
    let px = ((c.spread * scale).to_radians().tan() * 240.0 / tan_half_v.max(1e-4)).max(r.min_ofs);
    let ofs = (px - size * r.side_pos) * u;
    let (sz, e) = (size * u, size * 0.125 * u);
    if !r.side.is_empty() && sz > 0.0 {
        let name = crate::lui::materials::image(&r.side);
        // (left, top, rotation): `FUN_004b5ea0`'s placements, each with its eighth-size nudge.
        let quads = [
            (cx - sz * 0.5, cy - sz - ofs - e, 0.0f32),
            (cx + ofs + e, cy - sz * 0.5 + e, 90.0),
            (cx - sz * 0.5 - e, cy + ofs + e, 180.0),
            (cx - sz - ofs - e, cy - sz * 0.5, 270.0),
        ];
        for (x, y, rot) in quads {
            let (mx, my) = (x + sz * 0.5, y + sz * 0.5);
            let (sin, cos) = rot.to_radians().sin_cos();
            let turn = |a: f32, b: f32| (mx + (a * cos - b * sin) * sz * 0.5, my + (a * sin + b * cos) * sz * 0.5);
            let corners = [turn(-1.0, -1.0), turn(1.0, -1.0), turn(1.0, 1.0), turn(-1.0, 1.0)];
            // Without the art: a stroke along the piece's long axis, pointing at the centre.
            let (a0, a1) = (turn(0.0, -1.0), turn(0.0, 1.0));
            let fallback = vec![Prim::Line { x0: a0.0, y0: a0.1, x1: a1.0, y1: a1.1, width: 2.0 * u, argb: col }];
            prims.push(Prim::Quad { name: name.clone(), corners, argb: col, fallback });
        }
    }
    if !r.center.is_empty() && r.center_size > 0.0 {
        let cs = r.center_size * scale * u;
        prims.push(Prim::Image { name: crate::lui::materials::image(&r.center), x: cx - cs * 0.5, y: cy - cs * 0.5, w: cs, h: cs, argb: col, fallback: Vec::new() });
    }
}

/// The Scavenger icon's fade (`scavenger_think`: alpha 1, `fadeovertime( 2.5 )` to 0,
/// `_weapons.gsc:1799-1801`).
pub const SCAVENGER_ICON_FADE: f32 = 2.5;

/// The Scavenger pickup icon (`scavenger_hud_create`, `_weapons.gsc:1892-1912`): 48 x 24 virtual
/// (640x480) pixels, centred across, its top 16 below the middle, fading out over
/// [`SCAVENGER_ICON_FADE`].
pub(super) fn draw_scavenger_icon(prims: &mut Vec<Prim>, w: f32, h: f32, client: &Client) {
    let t = client.hud.scavenger_icon;
    if t <= 0.0 {
        return;
    }
    let k = h / 480.0;
    let (iw, ih) = (48.0 * k, 24.0 * k);
    let alpha = (t / SCAVENGER_ICON_FADE).min(1.0);
    prims.push(Prim::Image { name: super::sprites::SCAVENGER_PICKUP.to_string(), x: w * 0.5 - iw * 0.5, y: h * 0.5 + 16.0 * k, w: iw, h: ih, argb: argb((1.0, 1.0, 1.0), alpha), fallback: Vec::new() });
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
    // The perk variants (`_damagefeedback.gsc`: the last `setshader` stands).
    let material = match client.feedback.marker {
        Some(crate::damage::PerkFeedback::FlakJacket) => "damage_feedback_flak",
        Some(crate::damage::PerkFeedback::TacticalMask) => "damage_feedback_tac",
        None => "damage_feedback",
    };
    prims.push(Prim::Image { name: crate::lui::materials::image(material), x: cx - 12.0 * u, y: cy - 12.0 * u, w: 24.0 * u, h: 48.0 * u, argb: col, fallback });
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

/// BO2's hold-breath hint (ownerdraw 71 in `common_mp`'s `holdbreathhint.menu`: rect `0 80`, align
/// 9/8 = the safe area's horizontal centre and top, `textfont 1`, `textscale 0.315`, `textstyle 3`),
/// drawn by `FUN_004dc750` centred on the rect's x (`x - width * 0.5`) with its pen line 80 lines
/// below the top. The text is `PLATFORM_HOLD_BREATH`, "Press ^3&&1^7 and hold to steady": the
/// breath key's name in yellow (`^3`), the rest white, all over the shadowed style's black copy.
pub(super) fn draw_breath_hint(l: &Lui, prims: &mut Vec<Prim>, p: &PlayerSim, key: &str) {
    if !super::panels::breath_hint_shown(p) {
        return;
    }
    let (pen, height) = (BREATH_HINT_BELOW_TOP_480 * 1.5, BREATH_HINT_HEIGHT_480 * 1.5);
    let parts = super::hints::runs(crate::weapons::catalog().label("PLATFORM_HOLD_BREATH"), key, (1.0, 1.0, 1.0));
    let parts: Vec<(&str, (f32, f32, f32))> = parts.iter().map(|(t, c)| (t.as_str(), *c)).collect();
    let widths: Vec<f32> = parts.iter().map(|(t, _)| l.text_width(Family::Condensed, t, height)).collect();
    let total: f32 = widths.iter().sum();
    let row = Place::new((true, true, 0.0, 0.0), (true, false, pen - height, pen)).resolve(l.screen());
    let mut x = row.x + row.w * 0.5 - total * 0.5;
    let shadow = HINT_SHADOW_480 * 1.5;
    for ((text, rgb), w) in parts.iter().zip(&widths) {
        let r = Rect { x, w: *w, ..row };
        l.text(prims, Rect { x: x + shadow, y: r.y + shadow, ..r }, Family::Condensed, text, Some(crate::lui::font::Align::Left), (true, false), (0.0, 0.0, 0.0), 1.0);
        l.text(prims, r, Family::Condensed, text, Some(crate::lui::font::Align::Left), (true, false), *rgb, 1.0);
        x += w;
    }
}

/// `holdbreathhint.menu`'s rect y (480 lines below the safe area's top) and its text height
/// (`textscale 0.315` of the 48-line font).
const BREATH_HINT_BELOW_TOP_480: f32 = 80.0;
const BREATH_HINT_HEIGHT_480: f32 = 48.0 * 0.315;
/// BO2's weapon hint (ownerdraw 113, `FUN_004ddc80`): the localized text centred on the screen's
/// middle, its pen line 40 lines below the centre, white, fading in over each 600 ms for 1.8 s.
/// `textstyle 3` is IW's shadowed style: the text in black one virtual unit down and right, under
/// the text, at the same alpha.
pub(super) fn draw_hint(l: &Lui, prims: &mut Vec<Prim>, client: &Client) {
    let Some((key, t)) = client.hud.hint else { return };
    let alpha = (t % HINT_CYCLE) / HINT_CYCLE;
    // LUI's 720-line space is 1.5 x the menus' 480.
    let (pen, height) = (HINT_BELOW_CENTRE_480 * 1.5, HINT_HEIGHT_480 * 1.5);
    let r = Place::new((true, true, 0.0, 0.0), (false, false, pen - height, pen)).resolve(l.screen());
    let text = crate::sim::hint_text(key);
    let shadow = Rect { x: r.x + HINT_SHADOW_480 * 1.5, y: r.y + HINT_SHADOW_480 * 1.5, ..r };
    l.text(prims, shadow, Family::Condensed, text, None, (true, true), (0.0, 0.0, 0.0), alpha);
    l.text(prims, r, Family::Condensed, text, None, (true, true), (1.0, 1.0, 1.0), alpha);
}

/// The shadowed style's offset, in 480-line units.
const HINT_SHADOW_480: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_breath_hint_sits_80_lines_below_the_top_with_the_key_in_yellow() {
        let _f = crate::lui::font::lock();
        crate::lui::font::load("");
        let l = Lui::new(1280.0, 720.0);
        let mut p = PlayerSim::spawn_for_tests();
        let mut d = crate::weapons::WeaponDef { class: crate::weapons::Class::Sniper, overlay_reticle: 1, ..Default::default() };
        d.zoom.hold_breath = true;
        let w = p.arsenal.active;
        p.arsenal.weapons[w].def = Box::leak(Box::new(d));
        let mut prims = Vec::new();
        draw_breath_hint(&l, &mut prims, &p, "SHIFT");
        assert!(prims.is_empty(), "not aimed: no hint");
        p.arsenal.ads = 1.0;
        draw_breath_hint(&l, &mut prims, &p, "SHIFT");
        assert_eq!(prims.len(), 6, "three runs, each over its shadow");
        let texts: Vec<_> = prims.iter().filter_map(|q| if let Prim::Text { text, argb, y, size, .. } = q { Some((text.clone(), *argb, *y, *size)) } else { None }).collect();
        assert_eq!(texts[3].0, "SHIFT");
        assert_eq!(texts[3].1, super::super::frame::argb((1.0, 1.0, 0.0), 1.0), "^3 yellow");
        assert_eq!(texts[1].1, super::super::frame::argb((1.0, 1.0, 1.0), 1.0));
        // The pen line at 80 of 480 lines (120 of 720), the text 0.315 x 48 lines tall above it.
        let top = texts[1].2;
        assert!((top - (120.0 - 48.0 * 0.315 * 1.5)).abs() < 1.0, "{top}");
        p.idle.breath.holding = true;
        prims.clear();
        draw_breath_hint(&l, &mut prims, &p, "SHIFT");
        assert!(prims.is_empty(), "holding: no hint");
    }

    #[test]
    fn the_hint_sits_below_the_centre_and_fades_in_each_600_ms() {
        let _f = crate::lui::font::lock();
        crate::lui::font::load("");
        let l = Lui::new(1280.0, 720.0);
        let mut client = Client::default();
        let mut prims = Vec::new();
        draw_hint(&l, &mut prims, &client);
        assert!(prims.is_empty(), "no hint, nothing drawn");
        client.hud.hint = Some(("WEAPON_CANT_PLANT_EQUIPMENT", 0.3));
        draw_hint(&l, &mut prims, &client);
        // The black shadow first, one 480-line unit (1.5 LUI) down-right; then the text, centred.
        let (shadow, text) = (&prims[0], &prims[1]);
        assert!(matches!(text, Prim::Text { x, text, .. } if *x == 640.0 && !text.is_empty()), "{text:?}");
        let (Prim::Text { x: sx, y: sy, argb: sa, .. }, Prim::Text { y: ty, argb: ta, .. }) = (shadow, text) else { panic!() };
        assert_eq!((*sx, *sy - *ty), (641.5, 1.5));
        // Half way through the first cycle: half alpha.
        assert_eq!(super::super::frame::argb((1.0, 1.0, 1.0), 0.5), *ta);
        assert_eq!(super::super::frame::argb((0.0, 0.0, 0.0), 0.5), *sa);
    }

    fn bo2_def(keep: bool) -> &'static crate::weapons::WeaponDef {
        let reticle = crate::weapons::Reticle { side: "reticle_side_small".into(), side_size: 8.0, min_ofs: 6.0, ads_in_frac: 1.0, ads_out_frac: 0.2, keep_when_ads: keep, color_change: true, ..Default::default() };
        Box::leak(Box::new(crate::weapons::WeaponDef { reticle, ..Default::default() }))
    }

    fn look(def: &'static crate::weapons::WeaponDef, ads: f32, spread: f32) -> Crosshair<'static> {
        Crosshair { def, ads, zooming_in: true, aim_spread: 0.0, spread, hfov: 90f32.to_radians(), zoom: None, rgb: (1.0, 1.0, 1.0) }
    }

    fn quads(c: &Crosshair) -> Vec<[(f32, f32); 4]> {
        let mut prims = Vec::new();
        crosshair(&mut prims, 640.0, 480.0, c);
        prims.iter().filter_map(|p| if let Prim::Quad { corners, .. } = p { Some(*corners) } else { None }).collect()
    }

    #[test]
    fn the_side_pieces_sit_where_bo2_puts_them_at_least_the_min_offset_out() {
        // 640 x 480, a 90° horizontal FOV at 4:3: tan(vfov/2) = 0.75. No spread: the min offset 6.
        let q = quads(&look(bo2_def(false), 0.0, 0.0));
        assert_eq!(q.len(), 4);
        let centre = |c: &[(f32, f32); 4]| ((c[0].0 + c[2].0) * 0.5, (c[0].1 + c[2].1) * 0.5);
        // Top: centred on x, its box from 240 - 8 - 6 - 1 down to 240 - 6 - 1.
        let (tx, ty) = centre(&q[0]);
        assert!((tx - 320.0).abs() < 1e-3 && (ty - (240.0 - 6.0 - 1.0 - 4.0)).abs() < 1e-3, "{tx} {ty}");
        // Right: turned 90°, pushed out by 6 + 1, its eighth nudge down.
        let (rx, ry) = centre(&q[1]);
        assert!((rx - (320.0 + 7.0 + 4.0)).abs() < 1e-3 && (ry - 241.0).abs() < 1e-3, "{rx} {ry}");
        // A 3° spread pushes past the minimum: tan(3°) * 240 / 0.75.
        let q = quads(&look(bo2_def(false), 0.0, 3.0));
        let px = 3f32.to_radians().tan() * 240.0 / 0.75;
        assert!((centre(&q[0]).1 - (240.0 - px - 1.0 - 4.0)).abs() < 1e-2);
    }

    #[test]
    fn aiming_shrinks_the_crosshair_to_half_and_full_ads_hides_it_unless_kept() {
        let half = quads(&look(bo2_def(false), 0.5, 0.0));
        // In-fraction 1: half way in is t 0.5, scale 0.75: the side 6 wide.
        let w = (half[0][1].0 - half[0][0].0).abs();
        assert!((w - 6.0).abs() < 1e-3, "{w}");
        assert!(quads(&look(bo2_def(false), 1.0, 0.0)).is_empty(), "full ADS: gone");
        let kept = quads(&look(bo2_def(true), 1.0, 0.0));
        assert_eq!(kept.len(), 4, "bKeepCrosshairWhenADS");
        assert!(((kept[0][1].0 - kept[0][0].0).abs() - 8.0).abs() < 1e-3, "kept at full size");
    }

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
