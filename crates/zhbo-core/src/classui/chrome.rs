//! BO2's screen chrome: background, heading band, rows, panels, footer prompts, the allocation bar, icons and the weapon stat bars.

use super::*;

/// The full-screen BO2 menu: background art, backdrop, grid and glow, with the dark fallback.
///
/// BO2's create-a-class screens sit on a near-black panel (`b-editor/001`): the art shows through a
/// dark scrim, so the boxes, rows and text read the same at any map brightness.
pub(super) fn background(prims: &mut Vec<Prim>, w: f32, h: f32) {
    prims.push(Prim::Image {
        name: sprites::MENU_BACKGROUND.to_string(),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: 0xFFFF_FFFF,
        fallback: vec![Prim::Rect { x: 0.0, y: 0.0, w, h, argb: 0xF010_1214 }],
    });
    prims.push(Prim::Image {
        name: sprites::MENU_BACKDROP.to_string(),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: 0xE0FF_FFFF,
        fallback: Vec::new(),
    });
    prims.push(Prim::Image {
        name: sprites::MENU_GRID.to_string(),
        x: 0.0,
        y: 0.0,
        w,
        h,
        argb: 0x30FF_FFFF,
        fallback: Vec::new(),
    });
    prims.push(Prim::Rect { x: 0.0, y: 0.0, w, h, argb: 0xD814_1618 });
}

/// BO2's heading band: the three-piece GunTitle art across the content's width (BO2 puts the heading
/// at the top left, above a rule — `a-chooseclass/001`, `b-editor/001`).
pub(super) fn banner(prims: &mut Vec<Prim>, x: f32, w: f32, s: f32, y: f32, header_h: f32) -> f32 {
    let cap = header_h * 0.5;
    let mid_w = (w - 2.0 * cap).max(40.0 * s);
    let mut fallback = hud::slant_box(x + cap, y, mid_w, header_h, PANEL, Some(OUTLINE));
    fallback.push(Prim::Rect { x: x + cap + 4.0 * s, y: y + 6.0 * s, w: 3.0 * s, h: header_h - 12.0 * s, argb: GOLD });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[0].to_string(), x, y, w: cap, h: header_h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[1].to_string(), x: x + cap, y, w: mid_w, h: header_h, argb: 0xFFFF_FFFF, fallback });
    prims.push(Prim::Image { name: sprites::GUN_TITLE[2].to_string(), x: x + cap + mid_w, y, w: cap, h: header_h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    x + cap + 16.0 * s
}

/// A screen's heading in the band.
pub(super) fn heading(prims: &mut Vec<Prim>, x: f32, y: f32, s: f32, text: &str, argb: u32) {
    prims.push(Prim::Text { x, y: y + 10.0 * s, text: text.to_string(), argb, size: (24.0 * s) as i32, align: 0 });
}

/// A slanted list row in BO2's slate: a ROW_FILL/ROW_HIGHLIGHT bar with a ROW_LINE under it.
///
/// The converted row art is a whole framed tile, so stretching it to a list row's width smears its
/// border across the row (the drawn bar reads as BO2's plain row instead); the thin ROW_LINE art is
/// a line and stretches correctly.
pub(super) fn row(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, h: f32, s: f32, active: bool) {
    let fill = if active { ROW_ACTIVE } else { ROW_FILL };
    let edge = if active { Some(GOLD) } else { Some(ROW_EDGE) };
    prims.extend(hud::slant_box(x, y, w, h, fill, edge));
    if active {
        prims.push(Prim::Rect { x: x + 4.0 * s, y: y + 4.0 * s, w: 3.0 * s, h: h - 8.0 * s, argb: GOLD });
    }
    prims.push(Prim::Image {
        name: sprites::ROW_LINE.to_string(),
        x,
        y: y + h - 2.0 * s,
        w,
        h: 1.5 * s,
        argb: 0x80FF_FFFF,
        fallback: vec![Prim::Rect { x, y: y + h - 2.0 * s, w, h: 1.5 * s, argb: ROW_EDGE }],
    });
}

/// A panel: BO2's dark plate. The converted `PANEL_FILL`/`PANEL_LINE` art are framed tiles (an
/// orange border and a zigzag rule) that smear when stretched to a panel, so the plate draws its own
/// fill and outline, exactly like the missing-art fallbacks did.
pub(super) fn panel(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, h: f32, s: f32) {
    prims.push(Prim::Rect { x, y, w, h, argb: PANEL_FILL });
    hud::outline_rect(prims, x, y, w, h, 1.5 * s, OUTLINE);
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

/// BO2's segmented allocation bar: ten blocks, one per point, `spent` of them orange.
pub fn draw_allocation(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, s: f32, spent: i32, total: i32) {
    let n = total.max(1) as usize;
    let gap = 3.0 * s;
    let seg_w = ((w - gap * (n as f32 - 1.0)) / n as f32).max(2.0);
    let lit = allocation_lit(spent, total);
    for i in 0..n {
        prims.push(Prim::Rect {
            x: x + i as f32 * (seg_w + gap),
            y,
            w: seg_w,
            h: 6.0 * s,
            argb: if i < lit { ORANGE } else { BAR_FREE },
        });
    }
}

/// BO2's `n/10` counter, right-aligned at `right` (BO2's numerals are its digit art; a plain text
/// fallback keeps it legible when the art is small or missing).
pub fn draw_allocation_counter(prims: &mut Vec<Prim>, right: f32, y: f32, s: f32, spent: i32, total: i32) {
    let col = if spent > total { RED } else { ORANGE };
    prims.push(Prim::Text {
        x: right,
        y,
        text: format!("{}/{}", spent.max(0), total.max(0)),
        argb: col,
        size: (20.0 * s) as i32,
        align: 2,
    });
}

// ------------------------------------------------------------------------------------------------
// Element icons
// ------------------------------------------------------------------------------------------------

/// A weapon/perk/attachment/wildcard icon with a text fallback (the first letters of its name).
/// `size` is already scaled; the fallback text scales with it.
pub(super) fn icon(prims: &mut Vec<Prim>, name: Option<&str>, x: f32, y: f32, size: f32, argb: u32, fallback: &str) {
    let short: String = fallback.chars().filter(|c| !c.is_whitespace()).take(3).collect();
    let fallback = vec![Prim::Text { x: x + size * 0.5, y: y + size * 0.28, text: short, argb, size: (size * 0.38) as i32, align: 1 }];
    match name {
        Some(n) => prims.push(Prim::Image { name: n.to_string(), x, y, w: size, h: size, argb, fallback }),
        None => prims.extend(fallback),
    }
}

/// The per-weapon HUD icon the converter emits (`ZHBO_BO2_Icon_<id>`); an attached variant uses its
/// base gun's icon (`an94_mp+grip` -> `ZHBO_BO2_Icon_an94_mp`).
pub(super) fn weapon_icon(prims: &mut Vec<Prim>, id: &str, x: f32, y: f32, size: f32, argb: u32, label: &str) {
    let base = id.split('+').next().unwrap_or(id);
    let name = format!("ZHBO_BO2_Icon_{base}");
    icon(prims, Some(&name), x, y, size, argb, label);
}

/// A weapon id's base gun (the icon and catalog entry without its attachments).
pub(super) fn base_id(id: &str) -> &str {
    id.split('+').next().unwrap_or(id)
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

/// BO2's segmented stat readout: four rows of [`STAT_SEGMENTS`] blocks, FIRE RATE first. Spent
/// blocks are white, free ones BO2's dark slots (`c-primary/001`).
pub(super) fn stat_bars(prims: &mut Vec<Prim>, x: f32, y: f32, w: f32, s: f32, stats: &WeaponStats) {
    let row_h = 19.0 * s;
    let label_w = 78.0 * s;
    let bar_x = x + label_w;
    let bar_w = (w - label_w).max(40.0 * s);
    let gap = 3.0 * s;
    let seg_w = ((bar_w - gap * (STAT_SEGMENTS as f32 - 1.0)) / STAT_SEGMENTS as f32).max(1.0);
    for (i, label) in STAT_LABELS.iter().enumerate() {
        let top = y + i as f32 * row_h;
        prims.push(Prim::Text { x, y: top + 2.0 * s, text: (*label).to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        let lit = stat_lit(stat_value(stats, i));
        for k in 0..STAT_SEGMENTS {
            prims.push(Prim::Rect {
                x: bar_x + k as f32 * (seg_w + gap),
                y: top + 3.0 * s,
                w: seg_w,
                h: 8.0 * s,
                argb: if k < lit { 0xD8FF_FFFF } else { 0x4810_1214 },
            });
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The hosts' shared view
// ------------------------------------------------------------------------------------------------
