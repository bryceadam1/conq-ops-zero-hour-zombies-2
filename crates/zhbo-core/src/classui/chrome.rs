//! The class screens' shared chrome: the footer prompt strip, the deny rim, the allocation clamp,
//! element names and the weapon stat readout's values.

use super::*;

/// The prompt width at `s` (the engine font's advance is close enough for the layout and the
/// prompt's clickable box).
pub(super) fn prompt_width(p: &Prompt, s: f32) -> f32 {
    (p.key.chars().count() + p.label.chars().count() + 1) as f32 * PROMPT_SIZE * 6.0 / 7.0 * s
}

/// The footer strip: a slanted gold-edged command line, BO2's prompt strip.
pub(super) fn footer_bar(prims: &mut Vec<Prim>, w: f32, s: f32, y: f32) {
    let x = w * 0.05;
    prims.extend(hud::slant_box(x, y, w - 2.0 * x, 26.0 * s, 0x8010_1214, Some(GOLD)));
}

/// One footer prompt: BO2's gold key then the white label. Returns the prompt's width.
pub(super) fn draw_prompt(prims: &mut Vec<Prim>, x: f32, y: f32, s: f32, p: &Prompt) -> f32 {
    let size = (PROMPT_SIZE * s) as i32;
    let adv = PROMPT_SIZE * 6.0 / 7.0 * s;
    prims.push(Prim::Text { x, y, text: p.key.to_string(), argb: GOLD, size, align: 0 });
    let tx = x + (p.key.chars().count() as f32 + 1.0) * adv;
    prims.push(Prim::Text { x: tx, y, text: p.label.to_string(), argb: WHITE, size, align: 0 });
    prompt_width(p, s)
}

/// The prompt strip: `left` prompts from the left margin, `right` prompts ending at the right one.
pub(super) fn footer_prompts(prims: &mut Vec<Prim>, w: f32, s: f32, y: f32, left: &[Prompt], right: &[Prompt]) {
    footer_bar(prims, w, s, y);
    let ty = y + 7.0 * s;
    let gap = 24.0 * s;
    let mut x = w * 0.05 + 12.0 * s;
    for p in left {
        x += draw_prompt(prims, x, ty, s, p) + gap;
    }
    let right_edge = w * 0.95 - 12.0 * s;
    let mut x = right_edge - right.iter().map(|p| prompt_width(p, s) + gap).sum::<f32>() + gap;
    for p in right {
        x += draw_prompt(prims, x, ty, s, p) + gap;
        let _ = x;
    }
}

/// A red rim while a denied choice's flash is up.
pub(super) fn deny_flash(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, flash: f32) {
    if flash <= 0.0 {
        return;
    }
    let a = ((flash / DENY_FLASH).min(1.0) * 150.0) as u32;
    let col = (a << 24) | (RED & 0x00FF_FFFF);
    let t = 5.0 * s;
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h: t, argb: col });
    prims.push(Prim::Rect { x: 0.0, y: h - t, w, h: t, argb: col });
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w: t, h, argb: col });
    prims.push(Prim::Rect { x: w - t, y: 0.0, w: t, h, argb: col });
}

// ------------------------------------------------------------------------------------------------
// The allocation readout
// ------------------------------------------------------------------------------------------------

/// The allocation bar's lit segments: BO2 lights one per point spent (`allocationbar.lua`'s
/// `AddBars`, clamped to `Engine.GetMaxAllocation`).
pub fn allocation_lit(spent: i32, total: i32) -> usize {
    spent.clamp(0, total.max(0)) as usize
}

// ------------------------------------------------------------------------------------------------
// Element names and icons
// ------------------------------------------------------------------------------------------------

/// A weapon id's base gun (the icon and catalog entry without its attachments).
pub(super) fn base_id(id: &str) -> &str {
    id.split('+').next().unwrap_or(id)
}

/// An attachment short as the catalog names it.
pub(super) fn att_display(short: &str) -> String {
    let d = weapons::catalog();
    d.all()
        .iter()
        .find_map(|w| w.allowed.iter().find(|(s, _)| s == short).map(|(_, asset)| asset.clone()))
        .and_then(|asset| d.attachment(&asset).map(|a| a.display_name.clone()))
        .unwrap_or_else(|| short.to_string())
}

// ------------------------------------------------------------------------------------------------
// Weapon stat bars
// ------------------------------------------------------------------------------------------------

/// BO2's four menu bars, each 0..1. They are normalised against the gun catalog's own extremes, so
/// no number is hand-copied here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponStats {
    pub damage: f32,
    pub range: f32,
    pub fire_rate: f32,
    pub accuracy: f32,
}

/// BO2's bar labels, in the order the weapon screens list them (`c-primary/001`: FIRE RATE, DAMAGE,
/// RANGE, ACCURACY).
pub const STAT_LABELS: [&str; 4] = ["FIRE RATE", "DAMAGE", "RANGE", "ACCURACY"];

/// The bars' segments per row, as BO2's are (`c-primary/001`).
pub const STAT_SEGMENTS: usize = 18;

/// One bar's value by row index, in [`STAT_LABELS`] order.
pub fn stat_value(stats: &WeaponStats, row: usize) -> f32 {
    match row {
        0 => stats.fire_rate,
        1 => stats.damage,
        2 => stats.range,
        _ => stats.accuracy,
    }
}

/// How many of a bar's segments are lit.
pub fn stat_lit(value: f32) -> usize {
    (value.clamp(0.0, 1.0) * STAT_SEGMENTS as f32).round() as usize
}

/// The catalog's gun extremes, in one pass (no allocation).
pub(super) fn gun_bounds() -> (f32, f32, f32, f32, f32) {
    let guns = || weapons::catalog().all().iter().copied().filter(|d| d.class.is_gun());
    let max_damage = guns().map(|d| d.damage[0]).fold(0.0, f32::max);
    let max_range = guns().map(|d| d.max_range).fold(0.0, f32::max);
    let max_rpm = guns().map(|d| d.rounds_per_minute()).fold(0.0, f32::max);
    let min_spread = guns().map(|d| d.hip_spread[0].0).fold(f32::MAX, f32::min);
    let max_spread = guns().map(|d| d.hip_spread[0].0).fold(0.0, f32::max);
    (max_damage, max_range, max_rpm, min_spread, max_spread)
}

/// One gun's bars, derived only from its table fields: first damage point, max range, rounds per
/// minute, and the standing hip spread (tighter is more accurate).
pub fn weapon_stats(def: &WeaponDef) -> WeaponStats {
    let (max_damage, max_range, max_rpm, min_spread, max_spread) = gun_bounds();
    let ratio = |v: f32, max: f32| if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    let accuracy = if max_spread > min_spread {
        1.0 - ((def.hip_spread[0].0 - min_spread) / (max_spread - min_spread)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    WeaponStats {
        damage: ratio(def.damage[0], max_damage),
        range: ratio(def.max_range, max_range),
        fire_rate: ratio(def.rounds_per_minute(), max_rpm),
        accuracy,
    }
}
