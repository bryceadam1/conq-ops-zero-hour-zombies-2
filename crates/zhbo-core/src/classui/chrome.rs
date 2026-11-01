//! The class screens' shared chrome: the deny rim, the allocation clamp, element names and the
//! weapon stat readout's values.

use super::*;

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

/// BO2's four menu bars, each 0..1: the attribute table's 0..100 values over 100.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponStats {
    pub damage: f32,
    pub range: f32,
    pub fire_rate: f32,
    pub accuracy: f32,
}

/// BO2's bar labels' localize keys, in the order the weapon screens list them (`c-primary/001`:
/// FIRE RATE, DAMAGE, RANGE, ACCURACY).
pub const STAT_LABELS: [&str; 4] = ["MPUI_FIRE_RATE_CAPS", "MPUI_DAMAGE_CAPS", "MPUI_RANGE_CAPS", "MPUI_ACCURACY_CAPS"];

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

/// One weapon's bars: BO2's own attribute-table values (`WeaponDef::attributes`, 0..100), as
/// `weaponattributelist.lua` reads them.
pub fn weapon_stats(def: &WeaponDef) -> WeaponStats {
    from_attributes(def.attributes)
}

/// The bars with one attachment's deltas on this weapon added (the attachment picker's
/// comparison; `UpdateWeaponAttributes` with an attachment index). An attachment without a row
/// changes nothing.
pub fn weapon_stats_with(def: &WeaponDef, short: &str) -> WeaponStats {
    let delta = def.attachment_attributes.iter().find(|(s, _)| s == short).map_or([0.0; 4], |(_, a)| *a);
    let mut a = def.attributes;
    for (v, d) in a.iter_mut().zip(delta) {
        *v += d;
    }
    from_attributes(a)
}

fn from_attributes(a: [f32; 4]) -> WeaponStats {
    let bar = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    WeaponStats { fire_rate: bar(a[0]), damage: bar(a[1]), range: bar(a[2]), accuracy: bar(a[3]) }
}
