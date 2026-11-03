//! BO2's text hints over the view: the low-ammo warning and the localized hint text they share.
//!
//! * **The low-ammo warning** (`t6mp` `FUN_0049d3e0`, ownerdraw 123, `weaponinfo.menu`'s
//!   `lowammowarning` item: rect `-320 -8 640 100` aligned 9/9 — the safe area's middle —,
//!   `textalign 9`, `textscale 0.315`, the default font). It is drawn only for a weapon with a clip
//!   counter (`ammoCounterClip`) and no `unlimitedAmmo`, never in the reload states (0x0b-0x12),
//!   and only while the clip is at or under `lowAmmoWarningThreshold` of its size. With rounds in
//!   reserve it is `PLATFORM_RELOAD` ("^3[{+reload}]^7 Reload": the reload key in yellow brackets),
//!   unless the weapon has `bNoPartialReload` or a one-round clip; with none, an empty clip is
//!   `WEAPON_NO_AMMO`, else `PLATFORM_LOW_AMMO_NO_RELOAD` ("Low Ammo"; not for `bNoPartialReload`
//!   or a TV-guided weapon). Each pulses between its two colours
//!   (`lowAmmoWarning[NoReload|NoAmmo]Color1/2`) at 1.7 Hz:
//!   `t = (sin(now_ms · 2π/1000 · 1.7) + 1) · 0.75`, the colour `c1 + (c2 − c1) · t`.
//! * **The hint text.** BO2's localized strings carry the key a command is bound to (`&&1`, or
//!   `{+command}`) and the IW colour codes (`^3` yellow, `^7` back to white); [`runs`] splits one
//!   into coloured runs with the key filled in.

use super::frame::Lui;
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};

/// `lowammowarning`'s rect (480-line space, below the safe area's middle) and text height
/// (`textscale 0.315` of the 48-line font). The text is centred in the rect (`textalign 9`).
const LOW_AMMO_RECT_TOP_480: f32 = -8.0;
const LOW_AMMO_RECT_HEIGHT_480: f32 = 100.0;
const LOW_AMMO_HEIGHT_480: f32 = 48.0 * 0.315;
/// The pulse: `_DAT_00b1106c` (2π/1000 per ms), `_DAT_00b10bf0` (1.7) and `DAT_00c6f688` (0.75).
const LOW_AMMO_PULSE_HZ: f32 = 1.7;
const LOW_AMMO_PULSE_SCALE: f32 = 0.75;

type Rgba = [f32; 4];
/// The warning's colour pairs (the dvars' defaults, registered in `FUN_00499c30`):
/// `lowAmmoWarningColor1/2` white, `NoReloadColor1/2` (0.7, 0.7, 0, 0.8) to yellow,
/// `NoAmmoColor1/2` (0.8, 0, 0, 0.8) to red.
const RELOAD_COLORS: [Rgba; 2] = [[1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]];
const NO_RELOAD_COLORS: [Rgba; 2] = [[0.7, 0.7, 0.0, 0.8], [1.0, 1.0, 0.0, 1.0]];
const NO_AMMO_COLORS: [Rgba; 2] = [[0.8, 0.0, 0.0, 0.8], [1.0, 0.0, 0.0, 1.0]];

/// The IW colour codes the hints use: `^3` yellow, `^7` white (the default).
fn code_rgb(c: char) -> Option<(f32, f32, f32)> {
    match c {
        '3' => Some((1.0, 1.0, 0.0)),
        '7' => Some((1.0, 1.0, 1.0)),
        _ => None,
    }
}

/// A localized hint as coloured runs: `&&1` and any `{+command}` become `key`, and a colour code
/// (`^3`, `^7`) starts a run of its colour; text before the first code is `base`.
pub fn runs(text: &str, key: &str, base: (f32, f32, f32)) -> Vec<(String, (f32, f32, f32))> {
    let mut filled = text.replace("&&1", key);
    while let Some(start) = filled.find("{+") {
        let Some(end) = filled[start..].find('}') else { break };
        filled.replace_range(start..start + end + 1, key);
    }
    let mut out: Vec<(String, (f32, f32, f32))> = Vec::new();
    let mut rgb = base;
    let mut chars = filled.chars().peekable();
    let mut cur = String::new();
    while let Some(c) = chars.next() {
        if c == '^' {
            if let Some(next) = chars.peek().copied().filter(|d| d.is_ascii_digit()) {
                chars.next();
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), rgb));
                }
                rgb = code_rgb(next).map_or(base, |c| if next == '7' { base } else { c });
                continue;
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push((cur, rgb));
    }
    out
}

/// Which warning the weapon in hand shows (`FUN_0049d3e0`): its localize key and colour pair.
pub fn low_ammo_warning(p: &PlayerSim) -> Option<(&'static str, [Rgba; 2])> {
    if p.arsenal.is_unarmed() {
        return None;
    }
    let w = p.arsenal.current();
    let d = w.def;
    if !d.ammo_counter || d.traits.unlimited_ammo || w.is_reloading() {
        return None;
    }
    if !super::ammo::low_clip(w.ammo, d) {
        return None;
    }
    let clip = d.clip.min(999);
    if w.reserve > 0 {
        if d.no_partial_reload || clip == 1 {
            return None;
        }
        Some(("PLATFORM_RELOAD", RELOAD_COLORS))
    } else if w.ammo == 0 {
        Some(("WEAPON_NO_AMMO", NO_AMMO_COLORS))
    } else if d.no_partial_reload || d.guided_missile_type == 6 {
        None
    } else {
        Some(("PLATFORM_LOW_AMMO_NO_RELOAD", NO_RELOAD_COLORS))
    }
}

/// The pulse's colour at `clock` seconds.
pub fn pulse(colors: [Rgba; 2], clock: f32) -> Rgba {
    let t = ((clock * 1000.0 * std::f32::consts::TAU / 1000.0 * LOW_AMMO_PULSE_HZ).sin() + 1.0) * LOW_AMMO_PULSE_SCALE;
    let [a, b] = colors;
    [0, 1, 2, 3].map(|k| (a[k] + (b[k] - a[k]) * t).clamp(0.0, 1.0))
}

/// Draw the low-ammo warning, `reload_key` the reload binding's name, `clock` the HUD's.
pub(super) fn draw_low_ammo(l: &Lui, prims: &mut Vec<Prim>, p: &PlayerSim, reload_key: &str, clock: f32) {
    let Some((key, colors)) = low_ammo_warning(p) else { return };
    let [r, g, b, alpha] = pulse(colors, clock);
    let text = crate::weapons::catalog().label(key);
    let parts = runs(text, reload_key, (r, g, b));
    let height = LOW_AMMO_HEIGHT_480 * 1.5;
    let widths: Vec<f32> = parts.iter().map(|(t, _)| l.text_width(Family::Default, t, height)).collect();
    let total: f32 = widths.iter().sum();
    // Centred in the item's rect, on both axes.
    let mid = (LOW_AMMO_RECT_TOP_480 + LOW_AMMO_RECT_HEIGHT_480 * 0.5) * 1.5;
    let row = Place::new((true, true, 0.0, 0.0), (false, false, mid - height * 0.5, mid + height * 0.5)).resolve(l.screen());
    let mut x = row.x + row.w * 0.5 - total * 0.5;
    for ((text, rgb), w) in parts.iter().zip(&widths) {
        l.text(prims, Rect { x, w: *w, ..row }, Family::Default, text, Some(Align::Left), (true, false), *rgb, alpha);
        x += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hint_fills_in_the_key_and_splits_at_the_colour_codes() {
        let white = (1.0, 1.0, 1.0);
        let r = runs("^3[{+reload}]^7 Reload", "R", white);
        assert_eq!(r, vec![("[R]".to_string(), (1.0, 1.0, 0.0)), (" Reload".to_string(), white)]);
        let r = runs("Press ^3&&1^7 and hold to steady", "SHIFT", white);
        assert_eq!(r.iter().map(|(t, _)| t.as_str()).collect::<String>(), "Press SHIFT and hold to steady");
        assert_eq!(r[1], ("SHIFT".to_string(), (1.0, 1.0, 0.0)));
        // No codes: one run in the base colour.
        assert_eq!(runs("Low Ammo", "R", (1.0, 1.0, 0.0)), vec![("Low Ammo".to_string(), (1.0, 1.0, 0.0))]);
    }

    /// `FUN_0049d3e0`'s cases, per weapon flags and ammo.
    #[test]
    fn the_low_ammo_warning_follows_bo2s_cases() {
        let mut p = PlayerSim::spawn_for_tests();
        let base = crate::weapons::WeaponDef { clip: 30, low_ammo_threshold: 0.33, ammo_counter: true, ..p.arsenal.current().def.clone() };
        let set = |p: &mut PlayerSim, d: crate::weapons::WeaponDef, ammo: u16, reserve: u16| {
            let w = p.arsenal.active;
            p.arsenal.weapons[w].def = Box::leak(Box::new(d));
            p.arsenal.weapons[w].ammo = ammo;
            p.arsenal.weapons[w].reserve = reserve;
            p.arsenal.weapons[w].reload = None;
        };
        let key = |p: &PlayerSim| low_ammo_warning(p).map(|(k, _)| k);
        // 30 x 0.33 = 9.9: nine warns, ten does not.
        set(&mut p, base.clone(), 10, 90);
        assert_eq!(key(&p), None);
        set(&mut p, base.clone(), 9, 90);
        assert_eq!(key(&p), Some("PLATFORM_RELOAD"));
        // An LMG's 0.2 of 100: 25 left is not low.
        set(&mut p, crate::weapons::WeaponDef { clip: 100, low_ammo_threshold: 0.2, ..base.clone() }, 25, 200);
        assert_eq!(key(&p), None, "an LMG's own threshold");
        set(&mut p, base.clone(), 9, 0);
        assert_eq!(key(&p), Some("PLATFORM_LOW_AMMO_NO_RELOAD"), "nothing to reload with");
        set(&mut p, base.clone(), 0, 0);
        assert_eq!(key(&p), Some("WEAPON_NO_AMMO"));
        set(&mut p, crate::weapons::WeaponDef { ammo_counter: false, ..base.clone() }, 0, 0);
        assert_eq!(key(&p), None, "no clip counter (the shield, the knife)");
        set(&mut p, crate::weapons::WeaponDef { clip: 1, ..base.clone() }, 0, 3);
        assert_eq!(key(&p), None, "a one-round clip prompts no reload");
        set(&mut p, crate::weapons::WeaponDef { no_partial_reload: true, ..base.clone() }, 5, 30);
        assert_eq!(key(&p), None);
        // Reloading: nothing (BO2 draws none in the reload states).
        set(&mut p, base.clone(), 0, 90);
        let w = p.arsenal.active;
        p.arsenal.weapons[w].reload = Some(crate::weapons::weapon_state::Reload::Magazine { t: 0.0, length: 2.0, add_at: 1.5, added: false, empty: true, quick: false });
        assert!(p.arsenal.current().is_reloading());
        assert_eq!(key(&p), None, "not while reloading");
    }

    #[test]
    fn the_warning_pulses_between_its_colours() {
        let lo = pulse(NO_AMMO_COLORS, 0.75 / LOW_AMMO_PULSE_HZ);
        assert!((lo[0] - 0.8).abs() < 1e-3 && (lo[3] - 0.8).abs() < 1e-3, "the trough is colour 1: {lo:?}");
        let hi = pulse(NO_AMMO_COLORS, 0.25 / LOW_AMMO_PULSE_HZ);
        assert_eq!(hi, [1.0, 0.0, 0.0, 1.0], "the crest overshoots colour 2 and clamps");
        assert_eq!(pulse(RELOAD_COLORS, 0.3), [1.0; 4], "the reload prompt stays white");
    }
}
