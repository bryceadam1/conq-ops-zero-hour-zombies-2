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
    // Aiming through an optic: BO2's reticle art on the line of sight (BO2 projects the reticle
    // to infinity, so it stays centred). Iron sights have none.
    let optic = wpn.def.attachments.iter().any(|a| OPTICS.contains(&a.as_str()));
    if ads > 0.6 && !optic {
        return;
    }
    if ads > 0.6 && crate::nativevm::lens_reticle_drawn() {
        // The native viewmodel draws BO2's own reticle on the lens, projected to infinity
        // (`native.wgsl` `fs_reticle`): BO2 has that one and no 2D copy over it.
        return;
    }
    if ads > 0.6 {
        // The W3D fallback has no lens reticle: the optic's art on the line of sight stands in.
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
    let parts = [("Press ", (1.0, 1.0, 1.0)), (key, (1.0, 1.0, 0.0)), (" and hold to steady", (1.0, 1.0, 1.0))];
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
