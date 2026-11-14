//! The Zombies HUD a survivor General wears (`patch_zm/ui_mp/t6/zombie/*.lua`): the round's chalk
//! bottom-left (`hudroundstatuszombie.lua`) and the d-pad area bottom-right
//! (`huddpadareazombie.lua`): BO2's blood streak, the d-pad circle and the action slots on its
//! four arms (`UpdateActionSlots`: 1 up, 2 down, 3 left, 4 right).
//!
//! Deliberate divergence: the field computer is no action slot (user's ruling); its key prompt
//! sits under the circle. Not ported yet: the round chalk's animations (the first round's fade
//! and fall, a new round's pulse), and BO2's classic points list and Zombies ammo counter, whose
//! layouts are not in the reference dump (`AmmoAreaZombie` is required but was not dumped, the
//! classic score is a `ui_zm` menuDef); the MP ammo area and score panel stand in for them.

use super::frame::Lui;
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};
use crate::weapons::actionslot::{ActionSlot, Equipment};

/// `CoD.RoundStatus`: the chalk's size and top (bottom-anchored), its left offset (no faction
/// icon outside Grief, `FactionIconLeftOffset`) and colour (`DefaultColor`).
pub const CHALK_SIZE: f32 = 96.0;
pub const CHALK_TOP: f32 = -96.0;
pub const CHALK_LEFT: f32 = 0.0;
pub const CHALK_COLOR: (f32, f32, f32) = (0.21, 0.0, 0.0);
/// The tally marks for one to five (`hud_chalk_1..5`).
pub const CHALKS: [&str; 5] = ["hud_chalk_1", "hud_chalk_2", "hud_chalk_3", "hud_chalk_4", "hud_chalk_5"];

/// `CoD.DPadArea`: the circle's size, the blood streak's box and its tint, the arrow.
pub const CIRCLE_SIZE: f32 = 128.0;
pub const LINES_WIDTH: f32 = 300.0;
pub const LINES_HEIGHT: f32 = 150.0;
/// The streak's bottom below the circle's (`L3_2 = 10`).
pub const LINES_DROP: f32 = 10.0;
pub const LINES_COLOR: (f32, f32, f32) = (0.21, 0.0, 0.0);
pub const BLOOD: &str = "hud_dpad_blood";
pub const CIRCLE: &str = "hud_lui_dpad_circle";

/// The materials this module draws (for `lui::materials::MATERIALS`).
pub const MATERIALS: [&str; 7] = [CHALKS[0], CHALKS[1], CHALKS[2], CHALKS[3], CHALKS[4], BLOOD, CIRCLE];

/// The chalk a round shows at rest (`UpdateRoundsPlayed`/`StartNewRound`): one to five its tally
/// alone; six to ten a full five and the rest; past ten the number in Morris.
pub fn chalk(round: u32) -> (Vec<&'static str>, Option<String>) {
    match round {
        0 => (Vec::new(), None),
        1..=5 => (vec![CHALKS[round as usize - 1]], None),
        6..=10 => (vec![CHALKS[4], CHALKS[round as usize - 6]], None),
        _ => (Vec::new(), Some(round.to_string())),
    }
}

/// The round's chalk, bottom-left: each tally 96 square side by side from the left (`roundChalk1`,
/// `roundChalk2`), or the number in the first's box.
pub fn draw_round(l: &Lui, prims: &mut Vec<Prim>, round: u32) {
    let (tallies, number) = chalk(round);
    for (k, material) in tallies.iter().enumerate() {
        let left = CHALK_LEFT + k as f32 * CHALK_SIZE;
        let r = Place::new((true, false, left, left + CHALK_SIZE), (false, true, CHALK_TOP, CHALK_TOP + CHALK_SIZE)).resolve(l.screen());
        l.image(prims, r, material, CHALK_COLOR, 1.0, Vec::new());
    }
    if let Some(n) = number {
        let r = Place::new((true, false, CHALK_LEFT, CHALK_LEFT + CHALK_SIZE), (false, true, CHALK_TOP, CHALK_TOP + CHALK_SIZE)).resolve(l.screen());
        l.text(prims, r, Family::Morris, &n, Some(Align::Left), (true, false), CHALK_COLOR, 1.0);
    }
}

/// The circle's box, bottom-right.
pub fn circle_rect(l: &Lui) -> Rect {
    Place::new((false, true, -CIRCLE_SIZE, 0.0), (false, true, -CIRCLE_SIZE, 0.0)).resolve(l.screen())
}

/// Action slot `n`'s (1-4) icon box in the circle (`UpdateActionSlots`): `CircleSize / 4` tall,
/// `aspect` times as wide, a quarter of the circle in from its arm's edge.
pub fn slot_rect(circle: Rect, n: usize, aspect: f32) -> Rect {
    let h = CIRCLE_SIZE / 4.0;
    let w = h * aspect;
    let q = CIRCLE_SIZE / 4.0;
    let place = match n {
        1 => Place::new((false, false, -w / 2.0, w / 2.0), (true, false, q - h / 2.0, q + h / 2.0)),
        2 => Place::new((false, false, -w / 2.0, w / 2.0), (false, true, -q - h / 2.0, -q + h / 2.0)),
        3 => Place::new((true, false, q - w / 2.0, q + w / 2.0), (false, false, -h / 2.0, h / 2.0)),
        _ => Place::new((false, true, -q - w / 2.0, -q + w / 2.0), (false, false, -h / 2.0, h / 2.0)),
    };
    place.resolve(circle)
}

/// What one slot shows: its icon (a MappedImage), whether it can be used, and its key.
pub struct Slot<'a> {
    pub icon: Option<&'a str>,
    pub usable: bool,
    pub key: &'a str,
}

/// The d-pad area: the streak, the circle, each filled slot's icon (full alpha when usable, BO2's
/// `HUDAlphaEmpty` else) with its key beside it, and the field computer's key under the circle.
pub fn draw_dpad(l: &Lui, prims: &mut Vec<Prim>, slots: &[Slot; 4], field_computer_key: &str) {
    let circle = circle_rect(l);
    let lines = Place::new((false, true, -LINES_WIDTH, 0.0), (false, true, LINES_DROP - LINES_HEIGHT, LINES_DROP)).resolve(l.screen());
    l.image(prims, lines, BLOOD, LINES_COLOR, 1.0, Vec::new());
    l.image(prims, circle, CIRCLE, super::frame::HUD_BASE, 1.0, Vec::new());
    for (k, slot) in slots.iter().enumerate() {
        let Some(icon) = slot.icon else { continue };
        let r = slot_rect(circle, k + 1, 1.0);
        let alpha = if slot.usable { super::frame::HUD_ALPHA_FULL } else { super::frame::HUD_ALPHA_EMPTY };
        let p = l.px(r);
        prims.push(Prim::Image { name: icon.to_string(), x: p.x, y: p.y, w: p.w, h: p.h, argb: super::frame::argb(super::frame::HUD_BASE, alpha), fallback: Vec::new() });
        if !slot.key.is_empty() {
            let key = Place::new((true, true, 0.0, 0.0), (false, true, -10.0, 0.0)).resolve(r);
            l.text(prims, key, Family::Default, slot.key, Some(Align::Center), (true, true), super::frame::HUD_BASE, alpha);
        }
    }
    if !field_computer_key.is_empty() {
        let r = Place::new((false, true, -CIRCLE_SIZE, 0.0), (false, true, 2.0, 16.0)).resolve(l.screen());
        let label = format!("[{field_computer_key}] {}", crate::strings::text("HUD:ZHBO_FieldComputer"));
        l.text(prims, r, Family::Default, &label, Some(Align::Center), (true, true), super::frame::HUD_BASE, super::frame::HUD_ALPHA_FULL);
    }
}

/// The four slots of a survivor `p` as the d-pad shows them: the build tool's icon (usable while
/// he can build), the alt mode's (its weapon's `dpadIcon`, BO2's slot 3).
pub fn slots<'a>(p: &'a PlayerSim, can_build: bool, keys: [&'a str; 4]) -> [Slot<'a>; 4] {
    let alt = super::ammo::alt_slot_icon(p.arsenal.current().def, crate::weapons::catalog().get(&p.arsenal.current().def.alt));
    std::array::from_fn(|k| {
        let (icon, usable) = match p.action_slots.slots[k] {
            ActionSlot::Equipment { item: Equipment::BuildTool } => (Some(sprites::BUILD_ICON), can_build),
            ActionSlot::AltMode => (alt, true),
            ActionSlot::Empty => (None, false),
        };
        Slot { icon, usable, key: keys[k] }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chalk_counts_tallies_to_ten_then_the_number() {
        assert_eq!(chalk(1), (vec!["hud_chalk_1"], None));
        assert_eq!(chalk(5), (vec!["hud_chalk_5"], None));
        assert_eq!(chalk(7), (vec!["hud_chalk_5", "hud_chalk_2"], None));
        assert_eq!(chalk(11), (Vec::new(), Some("11".to_string())));
    }

    #[test]
    fn the_slots_sit_on_the_circles_arms() {
        let l = Lui::new(1280.0, 720.0);
        let c = circle_rect(&l);
        let centre = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (cx, cy) = centre(c);
        let q = CIRCLE_SIZE / 4.0;
        for (n, want) in [(1, (cx, c.y + q)), (2, (cx, c.bottom() - q)), (3, (c.x + q, cy)), (4, (c.right() - q, cy))] {
            let got = centre(slot_rect(c, n, 1.0));
            assert!((got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3, "slot {n}: {got:?} {want:?}");
        }
        assert_eq!((c.w, c.h), (CIRCLE_SIZE, CIRCLE_SIZE));
    }

    #[test]
    fn the_round_draws_its_tallies_in_dark_red() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw_round(&l, &mut prims, 7);
        let names: Vec<&str> = prims.iter().filter_map(|p| if let Prim::Image { name, .. } = p { Some(name.as_str()) } else { None }).collect();
        assert_eq!(names, ["ZHBO_Mat_hud_chalk_5", "ZHBO_Mat_hud_chalk_2"]);
    }
}
