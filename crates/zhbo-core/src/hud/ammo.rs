//! BO2's ammo area, bottom-right (`patch_mp/ui_mp/t6/hud/ammoarea.lua`, with `offhandicons.lua`
//! and `huddigit.lua`). Every box is the Lua's, offset from the safe area's bottom-right corner:
//! `CoD.AmmoArea.Right` -28 and `Bottom` -7 (ammoarea.lua:5-6).
//!
//! * The panel art: `hud_mp_vis_right_lower_white` and `hud_mp_vis_right_lower`, 300 x 150 from
//!   x -3 and y 22 up (ammoarea.lua:83-133).
//! * The lethal and tactical counters (`CoD.OffhandIcons`, size 24, width 36): one icon per item
//!   left, stacked leftwards 24 apart; the lethal's box ends 40 left of `Right`, the tactical's 94
//!   (ammoarea.lua:298-324, offhandicons.lua:33-127).
//! * The digits, right to left from `Right - 20`: the reserve small (x0.75, 12 lower, 16.5 apart),
//!   the slash small, then the clip big (22 apart), each digit after the first only when nonzero
//!   above it (ammoarea.lua:930-1267). A low clip pulses red.
//! * The fire-rate icon, 64 x 64 at `Right` and 47 above `Bottom` (ammoarea.lua:372-405,
//!   materials ammoarea.lua:1874-1911).
//! * The weapon label: BO2's three-piece gun title (`hud_mp_vis_gun_title_left/mid/right`, caps 64
//!   wide) with the name in `SmallItalic`, black at alpha 0.5, centred; its width is the text's
//!   less 18 plus 128 (ammoarea.lua:669-842, 1648-1708). Attachment labels follow it for
//!   `WeaponSelectionDelay` 2000 ms; with two or more, `CUSTOMIZED` replaces them.
//! * The alt fire (a grenade launcher's icon and its key, `Condensed` in `CoD.yellowGlow`, right
//!   aligned), 118 left of `Right` (ammoarea.lua:140-279, 847-925).

use super::digit;
use super::frame::{Lui, HUD_ALPHA_FULL, HUD_BASE, YELLOW_GLOW};
use super::Prim;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};
use crate::sim::PlayerSim;
use crate::weapons::{self, FireMode};

pub const RIGHT: f32 = -28.0;
pub const BOTTOM: f32 = -7.0;
pub const OFFHAND_SIZE: f32 = 24.0;
pub const OFFHAND_WIDTH: f32 = OFFHAND_SIZE * 1.5;
pub const OFFHAND_ALPHA_STEP: f32 = 0.75;
pub const WEAPON_SELECTION_DELAY_MS: f32 = 2000.0;
pub const WEAPON_SELECTION_DURATION_MS: f32 = 100.0;
const STENCIL_RIGHT: f32 = RIGHT + (0.0 - 15.0) + 58.0 - 45.0;

pub const MATERIALS: [&str; 12] = [
    "hud_mp_vis_right_lower_white",
    "hud_mp_vis_right_lower",
    "hud_mp_vis_gun_title_left",
    "hud_mp_vis_gun_title_mid",
    "hud_mp_vis_gun_title_right",
    "hud_mp_firerate_single",
    "hud_mp_firerate_burst",
    "hud_mp_firerate_auto",
    "hud_mp_firerate_free",
    "hud_mp_firerate_lock",
    "hud_mp_firerate_pump",
    "hud_mp_firerate_bolt",
];

/// A box offset from the anchor corner (both anchors right/bottom), in LUI units.
fn corner(area: Rect, left: f32, right: f32, top: f32, bottom: f32) -> Rect {
    Place::new((false, true, left.min(right), left.max(right)), (false, true, top.min(bottom), top.max(bottom))).resolve(area)
}

/// The ammo area's parent: the safe area's right edge, full height (ammoarea.lua:71).
pub fn area(l: &Lui) -> Rect {
    Place::new((false, true, 0.0, 0.0), (true, true, 0.0, 0.0)).resolve(l.screen())
}

/// BO2's fire-rate icon for a weapon (the engine's `fireTypeMaterial`). The weapon table does not
/// carry BO2's fire type yet, so pump and bolt actions read as single.
pub fn fire_rate_material(fire: FireMode) -> &'static str {
    match fire {
        FireMode::Auto | FireMode::Minigun => "hud_mp_firerate_auto",
        FireMode::Semi => "hud_mp_firerate_single",
        FireMode::Burst(_) => "hud_mp_firerate_burst",
    }
}

/// The engine's `lowClip`: under a third of the magazine. The weapon table does not carry BO2's
/// per-weapon warning threshold yet.
pub fn low_clip(ammo: u16, clip: u16) -> bool {
    clip > 0 && (ammo as f32) < clip as f32 / 3.0
}

/// An attachment's display name, from the converted table.
fn attachment_name(short: &str) -> String {
    let d = weapons::catalog();
    d.all()
        .iter()
        .find_map(|w| w.allowed.iter().find(|(s, _)| s == short).map(|(_, asset)| asset.clone()))
        .and_then(|asset| d.attachment(&asset).map(|a| a.display_name.clone()))
        .unwrap_or_else(|| short.to_string())
}

/// The panel art.
pub fn draw_panel(l: &Lui, prims: &mut Vec<Prim>) {
    let a = area(l);
    let r = corner(a, RIGHT + 25.0, RIGHT + 25.0 - 300.0, 22.0 - 150.0, 22.0);
    l.image(prims, r, "hud_mp_vis_right_lower_white", (1.0, 1.0, 1.0), 1.0, Vec::new());
    l.image(prims, r, "hud_mp_vis_right_lower", (1.0, 1.0, 1.0), 1.0, Vec::new());
}

/// One offhand counter: `count` icons of `icon` (a MappedImage), the newest brightest.
pub fn draw_offhand(l: &Lui, prims: &mut Vec<Prim>, right_offset: f32, icon: &str, count: u16) {
    let a = area(l);
    let bx = corner(a, RIGHT - right_offset - OFFHAND_WIDTH, RIGHT - right_offset, BOTTOM - OFFHAND_SIZE, BOTTOM);
    let n = count as i32;
    for i in 1..=n {
        let ofs = -(OFFHAND_SIZE * (i - 1) as f32);
        let r = corner(bx, ofs - OFFHAND_SIZE, ofs, -OFFHAND_SIZE, 0.0);
        let alpha = HUD_ALPHA_FULL * OFFHAND_ALPHA_STEP.powi(n - i);
        let p = l.px(r);
        prims.push(Prim::Image { name: icon.to_string(), x: p.x, y: p.y, w: p.w, h: p.h, argb: super::frame::argb(HUD_BASE, alpha), fallback: Vec::new() });
    }
}

/// The reserve / clip digits, right to left (ammoarea.lua:930-1267). `red` pulses the clip.
pub fn draw_digits(l: &Lui, prims: &mut Vec<Prim>, clip: u16, stock: u16, red: f32) {
    let a = area(l);
    let small_w = digit::WIDTH * digit::SMALL_SCALE;
    let small_step = digit::SPACING * digit::SMALL_SCALE;
    let small_top = digit::BIG_BOTTOM_Y + digit::SMALL_HEIGHT_DIFFERENCE;
    let small_h = digit::BIG_HEIGHT * digit::SMALL_SCALE;
    let mut pen = RIGHT - 20.0;
    let s = digit::digits(stock.min(999) as u32, 3);
    for (k, &d) in s.iter().rev().enumerate() {
        let shown = k == 0 || (k == 1 && (s[0] > 0 || s[1] > 0)) || (k == 2 && s[0] > 0);
        if !shown {
            continue;
        }
        digit::draw(l, prims, corner(a, pen - small_w, pen, small_top, small_top + small_h), d, 0.0, 1.0);
        pen -= small_step;
    }
    digit::draw(l, prims, corner(a, pen - small_w, pen, small_top, small_top + small_h), digit::SLASH, 0.0, 1.0);
    pen -= small_step;
    let c = digit::digits(clip.min(999) as u32, 3);
    for (k, &d) in c.iter().rev().enumerate() {
        let shown = k == 0 || (k == 1 && (c[0] > 0 || c[1] > 0)) || (k == 2 && c[0] > 0);
        if !shown {
            continue;
        }
        let top = digit::BIG_BOTTOM_Y;
        digit::draw(l, prims, corner(a, pen - digit::WIDTH, pen, top, top + digit::BIG_HEIGHT), d, red, 1.0);
        pen -= digit::SPACING;
    }
}

/// The fire-rate icon (shown once the weapon-selection labels settle).
pub fn draw_fire_rate(l: &Lui, prims: &mut Vec<Prim>, material: &str, alpha: f32) {
    let a = area(l);
    let r = corner(a, RIGHT - 64.0, RIGHT, BOTTOM - 47.0 - 64.0, BOTTOM - 47.0);
    l.image(prims, r, material, (1.0, 1.0, 1.0), alpha, Vec::new());
}

/// The label boxes' geometry: (full width, visible width, trim) for a text (ammoarea.lua:1648-1708).
pub fn label_widths(l: &Lui, text: &str) -> (f32, f32, f32) {
    let w = (l.text_width(Family::SmallItalic, text, 20.0) - 18.0).max(0.0);
    let full = w + 128.0;
    let visible = w + 30.0;
    (full, visible, (full - visible) * 0.5)
}

/// One gun-title label whose right edge is `x` from the stencil's right.
fn draw_label(l: &Lui, prims: &mut Vec<Prim>, stencil: Rect, x: f32, text: &str, alpha: f32) {
    let (full, _, _) = label_widths(l, text);
    let c = corner(stencil, x - full, x, -64.0, 0.0);
    let cap = |left: bool| Place::new(if left { (true, false, 0.0, 64.0) } else { (false, true, -64.0, 0.0) }, (true, true, 0.0, 0.0)).resolve(c);
    l.image(prims, cap(true), "hud_mp_vis_gun_title_left", (1.0, 1.0, 1.0), alpha, Vec::new());
    l.image(prims, Place::new((true, true, 64.0, -64.0), (true, true, 0.0, 0.0)).resolve(c), "hud_mp_vis_gun_title_mid", (1.0, 1.0, 1.0), alpha, Vec::new());
    l.image(prims, cap(false), "hud_mp_vis_gun_title_right", (1.0, 1.0, 1.0), alpha, Vec::new());
    let t = Place::new((true, true, -1.0, -1.0), (false, false, -10.0, 10.0)).resolve(c);
    l.text(prims, t, Family::SmallItalic, text, None, (true, true), (0.0, 0.0, 0.0), 0.5 * alpha);
}

/// The weapon label row: the weapon's name, then its attachments for the selection delay (or
/// `CUSTOMIZED` after it when there are two or more), the fire-rate icon at the right once settled.
/// `since_ms` is how long ago the weapon came up.
pub fn draw_weapon_label(l: &Lui, prims: &mut Vec<Prim>, name: &str, attachments: &[String], fire: &str, since_ms: f32) {
    let a = area(l);
    let stencil = corner(a, STENCIL_RIGHT - 500.0, STENCIL_RIGHT, BOTTOM - 47.0 - 64.0, BOTTOM - 47.0);
    let settled = since_ms >= WEAPON_SELECTION_DELAY_MS;
    let names: Vec<String> = attachments.iter().take(3).map(|s| attachment_name(s).to_uppercase()).collect();
    let customized = settled && names.len() >= 2;
    let shown: Vec<String> = if customized {
        vec!["CUSTOMIZED".to_string()]
    } else {
        names
    };
    // Right to left: the fire-rate icon's 64 once settled, then the attachment labels, then the gun.
    // `v` is the visible right edge; a label's box reaches `trim` past it (the art's soft caps).
    let name = name.to_uppercase();
    let mut v = if settled { -64.0 } else { 0.0 };
    for t in shown.iter().rev() {
        let (_, vis, trim) = label_widths(l, t);
        draw_label(l, prims, stencil, v + trim, t, 1.0);
        v -= vis;
    }
    let (_, _, trim) = label_widths(l, &name);
    draw_label(l, prims, stencil, v + trim, &name, 1.0);
    if settled {
        // The fire-rate icon fades in over `WeaponSelectionDuration * 2` (easeIn), ammoarea.lua:1807.
        let t = ((since_ms - WEAPON_SELECTION_DELAY_MS) / (WEAPON_SELECTION_DURATION_MS * 2.0)).clamp(0.0, 1.0);
        draw_fire_rate(l, prims, fire, crate::lui::anim::ease(t, true, false));
    }
}

/// The alt fire: the alt mode's icon and its key on PC.
pub fn draw_alt_fire(l: &Lui, prims: &mut Vec<Prim>, icon: &str, key: &str) {
    let a = area(l);
    let r = corner(a, RIGHT - 118.0 - 64.0, RIGHT - 118.0, BOTTOM + 22.0 - 64.0, BOTTOM + 22.0);
    let p = l.px(r);
    prims.push(Prim::Image { name: icon.to_string(), x: p.x, y: p.y, w: p.w, h: p.h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    let k = corner(a, RIGHT - 118.0 - 64.0 - 160.0, RIGHT - 118.0 - 64.0, BOTTOM + 6.0 - 3.0 - 25.0 + 4.0, BOTTOM + 6.0 - 3.0);
    l.text(prims, k, Family::Condensed, key, Some(Align::Right), (false, true), YELLOW_GLOW, 1.0);
}

/// The whole ammo area for a player. `since_ms` is the time since the current weapon came up and
/// `clock` the HUD's clock (seconds) for the low-clip pulse.
pub fn draw(l: &Lui, prims: &mut Vec<Prim>, p: &PlayerSim, since_ms: f32, clock: f32, alt_key: &str) {
    draw_panel(l, prims);
    if p.arsenal.is_unarmed() {
        return;
    }
    for (slot, ofs) in [(&p.arsenal.lethal, 40.0), (&p.arsenal.tactical, 94.0)] {
        draw_offhand(l, prims, ofs, &format!("ZHBO_BO2_Icon_{}", slot.def.id), slot.count);
    }
    let w = p.arsenal.current();
    let red = if low_clip(w.ammo, w.def.clip) { digit::pulse(clock) } else { 0.0 };
    draw_digits(l, prims, w.ammo, w.reserve, red);
    draw_weapon_label(l, prims, &w.def.display_name, &w.main.attachments, fire_rate_material(w.def.fire), since_ms);
    if !w.main.alt.is_empty() {
        draw_alt_fire(l, prims, &format!("ZHBO_BO2_Icon_{}", w.main.alt), alt_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rects(prims: &[Prim], name: &str) -> Vec<(f32, f32, f32, f32)> {
        prims.iter().filter_map(|p| match p {
            Prim::Image { name: n, x, y, w, h, .. } if n == name => Some((*x, *y, *w, *h)),
            _ => None,
        }).collect()
    }

    #[test]
    fn digits_follow_the_lua_pen() {
        // 1280 x 720: LUI units are pixels. Reserve 90: '0' at -48..-72 then '9' 16.5 left of it;
        // the slash; the clip 30: '0' then '3' 22 apart (ammoarea.lua:948-1143).
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw_digits(&l, &mut prims, 30, 90, 0.0);
        let zeros = rects(&prims, "ZHBO_Mat_hud_mp_num_big_0");
        assert_eq!(zeros[0], (1280.0 - 48.0 - 24.0, 720.0 - 75.0, 24.0, 48.0), "reserve units digit, small");
        let nine = rects(&prims, "ZHBO_Mat_hud_mp_num_big_9")[0];
        assert_eq!(nine.0, 1280.0 - 48.0 - 16.5 - 24.0);
        let slash = rects(&prims, "ZHBO_Mat_hud_mp_num_big_slash")[0];
        assert_eq!(slash.0, 1280.0 - 48.0 - 33.0 - 24.0);
        // The clip's units digit, full size, 16.5 after the slash: right edge -48 - 49.5.
        assert_eq!(zeros[1], (1280.0 - 97.5 - 32.0, 720.0 - 87.0, 32.0, 64.0));
        let three = rects(&prims, "ZHBO_Mat_hud_mp_num_big_3")[0];
        assert_eq!(three.0, 1280.0 - 97.5 - 22.0 - 32.0);
    }

    #[test]
    fn scales_with_the_screen_height() {
        // 2560 x 1080 (21:9): 1.5 px a unit, anchored to the right edge.
        let l = Lui::new(2560.0, 1080.0);
        let mut prims = Vec::new();
        draw_digits(&l, &mut prims, 5, 0, 0.0);
        let five = rects(&prims, "ZHBO_Mat_hud_mp_num_big_5")[0];
        assert_eq!(five, (2560.0 + (-81.0 - 32.0) * 1.5, 1080.0 - 87.0 * 1.5, 48.0, 96.0), "one reserve digit and the slash: the clip ends at -81");
    }

    #[test]
    fn offhands_stack_left_and_brighten() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw_offhand(&l, &mut prims, 40.0, "ZHBO_BO2_Icon_frag_grenade_mp", 2);
        let r: Vec<_> = prims.iter().filter_map(|p| if let Prim::Image { x, y, argb, .. } = p { Some((*x, *y, *argb)) } else { None }).collect();
        // The box ends at Right - 40 = -68; icon 1 fills its right 24, icon 2 the 24 left of that.
        assert_eq!(r, vec![(1280.0 - 68.0 - 24.0, 720.0 - 31.0, 0xBFFF_FFFF), (1280.0 - 68.0 - 48.0, 720.0 - 31.0, 0xFFFF_FFFF)]);
    }

    #[test]
    fn label_and_fire_rate_settle_after_the_delay() {
        let l = Lui::new(1280.0, 720.0);
        let atts = vec!["reflex".to_string(), "gl".to_string()];
        let mut before = Vec::new();
        draw_weapon_label(&l, &mut before, "an-94", &atts, "hud_mp_firerate_auto", 500.0);
        assert!(rects(&before, "ZHBO_Mat_hud_mp_firerate_auto").is_empty(), "no fire-rate icon while selecting");
        assert_eq!(rects(&before, "ZHBO_Mat_hud_mp_vis_gun_title_left").len(), 3, "the gun and two attachments");
        let mut after = Vec::new();
        draw_weapon_label(&l, &mut after, "an-94", &atts, "hud_mp_firerate_auto", 2500.0);
        assert_eq!(rects(&after, "ZHBO_Mat_hud_mp_vis_gun_title_left").len(), 2, "the gun and CUSTOMIZED");
        let icon = rects(&after, "ZHBO_Mat_hud_mp_firerate_auto")[0];
        assert_eq!(icon, (1280.0 - 28.0 - 64.0, 720.0 - 7.0 - 47.0 - 64.0, 64.0, 64.0));
    }

    #[test]
    fn low_clip_is_a_third() {
        assert!(low_clip(9, 30));
        assert!(!low_clip(10, 30));
        assert!(!low_clip(0, 0));
    }
}
