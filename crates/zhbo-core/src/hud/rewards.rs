//! BO2's scorestreak column (`patch_mp/ui_mp/t6/hud/rewardselection.lua`), bottom-right above the
//! ammo. Deliberate divergence: Conquer Ops has no scorestreaks, so its slots hold the field
//! computer and the build menu.
//!
//! The column is a vertical list 64 wide at the safe area's right, its top at
//! `-30 + HeightOffset - SelectorSideLength + Spacing · RewardTotalCount` = -302, the items
//! overlapping by `Spacing` -28; the inventory slot comes first, then a 40 spacer
//! (`-Spacing + 12`), then the three rewards (rewardselection.lua:6-15, 121-164). Each slot is
//! 24 left of the list (`left = right = -24`, both anchored), 64 tall: `hud_mp_vis_ks_dpad_box_back`
//! (`_lit` when it can be used), `hud_mp_vis_ks_dpad_box`, the 64 x 64 icon centred, and the
//! brackets while in use (rewardselection.lua:516-766). A slot's name shows right-aligned at
//! `textLeft` -100 over `textBottom` -100 in `Default` for `RewardSelectionDelay` 1000 ms, then
//! fades over 250 ms (rewardselection.lua:31-35, 828-880).

use super::frame::Lui;
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};

pub const SIDE: f32 = 64.0;
pub const HEIGHT_OFFSET: f32 = -96.0;
pub const SPACING: f32 = -28.0;
pub const TOTAL: f32 = 4.0;
pub const DISABLED_ALPHA: f32 = 0.35;
pub const NAME_MS: f32 = 1000.0;
pub const NAME_FADE_MS: f32 = 250.0;

pub const MATERIALS: [&str; 4] = ["hud_mp_vis_ks_dpad_box_back", "hud_mp_vis_ks_dpad_box_back_lit", "hud_mp_vis_ks_dpad_box", "hud_mp_vis_ks_dpad_bracket"];

/// The top of each slot, from the safe area's bottom: the inventory slot, then rewards 1-3.
pub fn slot_tops() -> [f32; 4] {
    let mut y = -30.0 + HEIGHT_OFFSET - SIDE + SPACING * TOTAL;
    let mut tops = [0.0; 4];
    for (i, top) in tops.iter_mut().enumerate() {
        *top = y;
        y += SIDE + SPACING;
        if i == 0 {
            // The spacer after the inventory slot, with the list's spacing after it too.
            y += -SPACING + 12.0 + SPACING;
        }
    }
    tops
}

/// One slot's box (`slot` 0 is the inventory slot).
pub fn slot_rect(l: &Lui, slot: usize) -> Rect {
    let top = slot_tops()[slot.min(3)];
    Place::new((false, true, -SIDE - 24.0, -24.0), (false, true, top, top + SIDE)).resolve(l.screen())
}

/// One reward slot: its icon (a MappedImage), whether it can be used (lit) and is in use.
pub fn draw_slot(l: &Lui, prims: &mut Vec<Prim>, slot: usize, icon: &str, label: &str, usable: bool, in_use: bool) {
    let r = slot_rect(l, slot);
    let back = if usable { "hud_mp_vis_ks_dpad_box_back_lit" } else { "hud_mp_vis_ks_dpad_box_back" };
    let alpha = if usable { 1.0 } else { DISABLED_ALPHA };
    l.image(prims, r, back, (1.0, 1.0, 1.0), alpha, Vec::new());
    l.image(prims, r, "hud_mp_vis_ks_dpad_box", (1.0, 1.0, 1.0), alpha, Vec::new());
    let ic = Place::new((false, false, -32.0, 32.0), (false, false, -32.0, 32.0)).resolve(r);
    let p = l.px(ic);
    let fallback = vec![Prim::Text { x: p.x + p.w * 0.5, y: p.y + p.h * 0.35, text: label.to_string(), argb: frame_argb(alpha), size: (p.h * 0.22) as i32, align: 1 }];
    prims.push(Prim::Image { name: icon.to_string(), x: p.x, y: p.y, w: p.w, h: p.h, argb: frame_argb(alpha), fallback });
    if in_use {
        l.image(prims, r, "hud_mp_vis_ks_dpad_bracket", (1.0, 1.0, 1.0), 1.0, Vec::new());
    }
}

fn frame_argb(alpha: f32) -> u32 {
    super::frame::argb((1.0, 1.0, 1.0), alpha)
}

/// A slot's name after it comes into use, `since_ms` ago.
pub fn draw_name(l: &Lui, prims: &mut Vec<Prim>, name: &str, since_ms: f32) {
    let fade = if since_ms <= NAME_MS { 1.0 } else { 1.0 - (since_ms - NAME_MS) / NAME_FADE_MS };
    if fade <= 0.0 {
        return;
    }
    let r = Place::new((false, true, -400.0, -100.0), (false, true, -100.0 - 25.0, -100.0)).resolve(l.screen());
    l.text(prims, r, Family::Default, name, Some(Align::Right), (false, true), (1.0, 1.0, 1.0), fade);
}

/// The column: reward 3 (the bottom slot) is the field computer, always usable; reward 2 is the
/// build menu, usable while the General can build. Reward 1 and the inventory slot stay empty.
pub(super) fn draw(l: &Lui, prims: &mut Vec<Prim>, tablet_up: bool, hammer_up: bool, can_build: bool, since_ms: f32) {
    draw_slot(l, prims, 2, sprites::BUILD_ICON, "BUILD", can_build, hammer_up);
    draw_slot(l, prims, 3, sprites::STREAK_VSAT, "FC", true, tablet_up);
    if tablet_up {
        draw_name(l, prims, "FIELD COMPUTER", since_ms);
    } else if hammer_up {
        draw_name(l, prims, "BUILD", since_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_stack_like_the_vertical_list() {
        // -302 for the inventory slot; then -302 + 64 - 28 + 40 - 28 = -254, and 36 apart after.
        assert_eq!(slot_tops(), [-302.0, -254.0, -218.0, -182.0]);
        let l = Lui::new(1280.0, 720.0);
        assert_eq!(slot_rect(&l, 3), Rect { x: 1280.0 - 88.0, y: 720.0 - 182.0, w: 64.0, h: 64.0 });
    }

    #[test]
    fn the_field_computer_slot_is_lit_and_bracketed_in_use() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw(&l, &mut prims, true, false, true, 0.0);
        let names: Vec<_> = prims.iter().filter_map(|p| if let Prim::Image { name, .. } = p { Some(name.as_str()) } else { None }).collect();
        assert_eq!(names.iter().filter(|n| **n == "ZHBO_Mat_hud_mp_vis_ks_dpad_box_back_lit").count(), 2);
        assert!(names.contains(&"ZHBO_Mat_hud_mp_vis_ks_dpad_bracket"));
        assert!(names.contains(&sprites::STREAK_VSAT));
    }
}
