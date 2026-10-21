//! BO2's `ClassPreview` (`patch_mp/ui/t6/classpreview.lua`, `AddWidgets` and `UpdateClass`): the
//! highlighted class's kit beside the choose-class list, in LUI units relative to the preview.
//!
//! With the preview `W` wide: the allocation counter (Condensed, right-aligned, 100 wide, bottom at
//! 5) over the ten-bar allocation strip (5..15), then from y = 30 a left column `(W - 4) * 0.53`
//! wide and a right one `(W - 4) * 0.47` wide 4 apart. Left: the primary block (half the column's
//! width tall), its attachment row, the secondary and its attachment row (71 tall each, 4 apart).
//! Right: three perk rows sharing the same height
//! (`((primary + 4 + 71) - 8) / 3` each, 4 apart), then LETHAL and TACTICAL (71 tall, 4 apart),
//! then the WILDCARDS label and three cards across the whole width. Every block is a black 0.1
//! plate with its slot name in ExtraSmall at alpha 0.5 and the item's name in off-white.

use super::bo2::{self, argb, Frame, BLACK, COD_ORANGE, OFF_WHITE};
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::Rect;

/// The block spacing (`L8_2`), the secondary/grenade height (`L9_2`), the badge size (`L10_2`).
const GAP: f32 = 4.0;
const SMALL_H: f32 = 71.0;
/// The first block row's top (`L24_2 = 5 + 10 + 15`).
const TOP: f32 = 30.0;
/// The allocation bar's top and height (`L7_2`, `L6_2`).
const BAR_TOP: f32 = 5.0;
const BAR_H: f32 = 10.0;
/// `CoD.AllocationStatus.Width`.
const STATUS_W: f32 = 100.0;

/// The preview's column geometry for a preview `w` wide.
struct Cols {
    left_w: f32,
    right_x: f32,
    right_w: f32,
    primary_h: f32,
    perk_h: f32,
}

fn cols(w: f32) -> Cols {
    let left_w = (w - GAP) * 0.53;
    let right_w = (w - GAP) * 0.47;
    let primary_h = left_w / 2.0;
    Cols { left_w, right_x: left_w + GAP, right_w, primary_h, perk_h: ((primary_h + GAP + SMALL_H) - GAP * 2.0) / 3.0 }
}

/// A block plate: black at alpha 0.1.
fn plate(prims: &mut Vec<Prim>, f: &Frame, r: Rect) {
    bo2::fill(prims, f, r, argb(BLACK, 0.1));
}

/// A slot's name (`MPUI_PRIMARY_CAPS`...) at a block's top-left, ExtraSmall at alpha 0.5.
fn slot_label(prims: &mut Vec<Prim>, f: &Frame, x: f32, y: f32, w: f32, s: &str) {
    bo2::text(prims, f, Family::ExtraSmall, s, Rect { x: x + 3.0, y, w, h: bo2::TEXT_EXTRA_SMALL }, Align::Left, argb(OFF_WHITE, 0.5));
}

/// The kit `draft` describes in the preview `p` (root units); `None` is an empty slot: the plates,
/// labels and a zero allocation, nothing equipped.
pub fn draw(prims: &mut Vec<Prim>, f: &Frame, p: Rect, draft: Option<&Draft>) {
    let at = |x: f32, y: f32, w: f32, h: f32| Rect { x: p.x + x, y: p.y + y, w, h };
    let c = cols(p.w);
    let spent = draft.map_or(0, Draft::spent);
    let max = crate::classmenu::PICK_10;

    // AllocationStatus: `n/10`, Condensed, right-aligned, `CoD.orange` under budget.
    bo2::text(prims, f, Family::Condensed, &format!("{spent}/{max}"), at(p.w - STATUS_W, BAR_TOP - bo2::TEXT_CONDENSED, STATUS_W, bo2::TEXT_CONDENSED), Align::Right, argb(COD_ORANGE, 1.0));
    allocation_bar(prims, f, at(0.0, BAR_TOP, p.w, BAR_H), spent, max);

    // The primary block.
    let pr = at(0.0, TOP, c.left_w, c.primary_h);
    plate(prims, f, pr);
    slot_label(prims, f, pr.x, pr.y, c.left_w, "PRIMARY");
    // The primary's attachment row, the secondary block and its attachment row.
    let pa = at(0.0, TOP + c.primary_h + GAP, c.left_w, SMALL_H);
    let sr = at(0.0, pa.y - p.y + SMALL_H + GAP, c.left_w, SMALL_H);
    let sa = at(0.0, sr.y - p.y + SMALL_H + GAP, c.left_w, SMALL_H);
    plate(prims, f, pa);
    plate(prims, f, sr);
    plate(prims, f, sa);
    slot_label(prims, f, sr.x, sr.y, c.left_w, "SECONDARY");
    // The three perk rows.
    let perk_rows: Vec<Rect> = (0..3).map(|k| at(c.right_x, TOP + k as f32 * (c.perk_h + GAP), c.right_w, c.perk_h)).collect();
    for (k, r) in perk_rows.iter().enumerate() {
        plate(prims, f, *r);
        slot_label(prims, f, r.x, r.y, c.right_w, &format!("PERK {}", k + 1));
    }
    // LETHAL and TACTICAL under the perks.
    let lethal = at(c.right_x, TOP + 3.0 * (c.perk_h + GAP), c.right_w, SMALL_H);
    let tactical = at(c.right_x, lethal.y - p.y + SMALL_H + GAP, c.right_w, SMALL_H);
    for (r, label) in [(lethal, "LETHAL"), (tactical, "TACTICAL")] {
        plate(prims, f, r);
        slot_label(prims, f, r.x, r.y, c.right_w, label);
    }
    // WILDCARDS: the label then three plates across the preview.
    let wild_label_y = tactical.bottom() + GAP;
    slot_label(prims, f, p.x, wild_label_y, p.w, "WILDCARDS");
    let card_w = (p.w - GAP * 2.0) / 3.0;
    let cards: Vec<Rect> = (0..3).map(|i| Rect { x: p.x + i as f32 * (card_w + GAP), y: wild_label_y + bo2::TEXT_EXTRA_SMALL, w: card_w, h: SMALL_H }).collect();
    for r in &cards {
        plate(prims, f, *r);
    }

    let Some(d) = draft else { return };
    let name = |r: Rect, s: &str, family: Family, h: f32, y: f32| {
        let mut out = Vec::new();
        bo2::text(&mut out, f, family, s, Rect { x: r.x + 3.0, y, w: r.w, h }, Align::Left, argb(OFF_WHITE, 1.0));
        out
    };
    // Weapons: the `_big` picture right of centre, below the slot label, and the name in
    // Condensed at the block's bottom.
    for (r, id, pic_h, right_pad, pic_top) in [
        (pr, d.primary.as_str(), c.primary_h * 0.9, 50.0, pr.y + bo2::TEXT_EXTRA_SMALL),
        (sr, d.secondary.as_str(), SMALL_H * 0.8, 80.0, sr.y + bo2::TEXT_EXTRA_SMALL),
    ] {
        if id.is_empty() {
            continue;
        }
        let def = weapons::weapon(id);
        let pic_w = pic_h * if r == pr { 1.8 } else { 2.0 };
        let pic = Rect { x: r.right() - right_pad - pic_w, y: pic_top, w: pic_w, h: pic_h };
        bo2::sprite(prims, f, &crate::names::cac_weapon_image(base_id(&def.id)), pic, argb(OFF_WHITE, 1.0), Vec::new());
        prims.extend(name(r, &def.display_name, Family::Condensed, bo2::TEXT_CONDENSED, r.bottom() - bo2::TEXT_CONDENSED));
    }
    // Attachments: up to three 61-unit icons from the row's right, 4 apart, the names stacked in
    // ExtraSmall at the left (`primaryWeaponAttachmentsImage1..3`).
    for (r, atts) in [(pa, &d.primary_atts), (sa, &d.secondary_atts)] {
        let icon = SMALL_H - 10.0;
        let n = atts.len().min(3);
        for (i, short) in atts.iter().take(3).enumerate() {
            let k = (n - i) as f32;
            let rect = Rect { x: r.right() - icon * k - GAP * k, y: r.y + 5.0, w: icon, h: icon };
            if let Some(name) = sprites::attachment_icon(short) {
                bo2::sprite(prims, f, name, rect, argb(OFF_WHITE, 1.0), Vec::new());
            }
            prims.extend(name(r, &att_display(short), Family::ExtraSmall, bo2::TEXT_EXTRA_SMALL, r.y + bo2::TEXT_EXTRA_SMALL * i as f32));
        }
    }
    // Perks: the tier's picks as squares right-anchored in the row (the last pick flush right,
    // `classpreview.lua`'s `perkImage1/2`), the names under the slot label, clipped short of them.
    for (k, r) in perk_rows.iter().enumerate() {
        let tier = k as u8 + 1;
        let picks: Vec<crate::weapons::Perk> = d.perks_in_order().into_iter().filter(|p| p.def().tier == tier).collect();
        let n = picks.len();
        for (i, perk) in picks.iter().enumerate() {
            let size = c.perk_h;
            let x = r.right() - (n - i) as f32 * (size + GAP);
            bo2::sprite(prims, f, sprites::perk_icon(*perk), Rect { x, y: r.y, w: size, h: size }, argb(OFF_WHITE, 1.0), Vec::new());
            let name_r = Rect { x: r.x + 3.0, y: r.y + bo2::TEXT_EXTRA_SMALL * (i as f32 + 1.0), w: c.right_w - n as f32 * (size + GAP) - 8.0, h: bo2::TEXT_EXTRA_SMALL };
            bo2::text(prims, f, Family::ExtraSmall, &perk_name(*perk), name_r, Align::Left, argb(OFF_WHITE, 1.0));
        }
    }
    // Grenades: the icon at three quarters of the row's height, from the row's right in 71-wide
    // cells (`classpreview.lua`'s `primarygrenadeImage1/2`); the name clipped short of it.
    for (r, id) in [(lethal, d.lethal.as_str()), (tactical, d.tactical.as_str())] {
        if id.is_empty() {
            continue;
        }
        let def = weapons::weapon(id);
        let s = SMALL_H * 0.75;
        let x = r.right() - SMALL_H * 2.0 + SMALL_H / 2.0 - s / 2.0;
        bo2::sprite(prims, f, &format!("ZHBO_BO2_Icon_{}", def.id), Rect { x, y: r.y + SMALL_H / 2.0 - s / 2.0, w: s, h: s }, argb(OFF_WHITE, 1.0), Vec::new());
        let name_r = Rect { x: r.x + 3.0, y: r.y + bo2::TEXT_EXTRA_SMALL, w: c.right_w - 2.0 * SMALL_H - 8.0, h: bo2::TEXT_EXTRA_SMALL };
        bo2::text(prims, f, Family::ExtraSmall, &def.display_name, name_r, Align::Left, argb(OFF_WHITE, 1.0));
    }
    // Wildcards: each card's art and name in its plate.
    for (r, wildcard) in cards.iter().zip(d.wildcards.iter()) {
        let s = r.h;
        bo2::sprite(prims, f, sprites::wildcard_icon(*wildcard), Rect { x: r.right() - s * 1.3, y: r.y, w: s * 1.3, h: s }, argb(OFF_WHITE, 1.0), Vec::new());
        prims.extend(name(*r, wildcard.name(), Family::ExtraSmall, bo2::TEXT_EXTRA_SMALL, r.y + 3.0));
    }
}

/// `CoD.AllocationBar` (`allocationbar.lua`): ten bars `BarSpacing` 5 apart across the strip, each
/// `BarHeight * 2` tall centred: `BOIIOrange` at 0.35 behind, the spent ones filled solid with the
/// white 0.5 highlight one unit in along their top (`allocation_change`).
pub fn allocation_bar(prims: &mut Vec<Prim>, f: &Frame, r: Rect, spent: i32, max: i32) {
    let n = max.max(1) as usize;
    let lit = allocation_lit(spent, max);
    let spacing = 5.0;
    let bar_w = (r.w - spacing * (n as f32 - 1.0)) / n as f32;
    let bar_h = 8.0;
    let y = r.y + r.h / 2.0 - bar_h / 2.0;
    for i in 0..n {
        let b = Rect { x: r.x + i as f32 * (bar_w + spacing), y, w: bar_w, h: bar_h };
        bo2::fill(prims, f, b, argb(bo2::BOII_ORANGE, 0.35));
        if i < lit {
            bo2::fill(prims, f, b, argb(bo2::BOII_ORANGE, 1.0));
            bo2::fill(prims, f, Rect { x: b.x + 1.0, y: b.y + 1.0, w: b.w - 2.0, h: (b.h - 6.0).max(0.0) }, argb(OFF_WHITE, 0.5));
        }
    }
}

/// A perk's display name: the perk table's ids are BO2's English names run together
/// (`FastHands` -> `Fast Hands`).
pub fn perk_name(p: crate::weapons::Perk) -> String {
    let id = p.def().id;
    let mut out = String::with_capacity(id.len() + 2);
    for (i, ch) in id.chars().enumerate() {
        if i > 0 && ch.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_columns_match_the_lua() {
        // The lobby's preview is 864 - (864 / 3 - 50) = 626 wide (`a-chooseclass/001`).
        let c = cols(626.0);
        assert!((c.left_w - 329.66).abs() < 0.01);
        assert!((c.right_x - 333.66).abs() < 0.01);
        assert!((c.primary_h - 164.83).abs() < 0.01);
        assert!((c.perk_h - 77.28).abs() < 0.01);
        // The three perk rows end where the secondary block does.
        let perks_end = TOP + 3.0 * c.perk_h + 2.0 * GAP;
        let secondary_end = TOP + c.primary_h + GAP + SMALL_H;
        assert!((perks_end - secondary_end).abs() < 1e-3);
    }

    #[test]
    fn perk_names_read_like_bo2() {
        assert_eq!(perk_name(crate::weapons::Perk::FAST_HANDS), "Fast Hands");
        assert_eq!(perk_name(crate::weapons::Perk::EXTREME_CONDITIONING), "Extreme Conditioning");
    }
}
