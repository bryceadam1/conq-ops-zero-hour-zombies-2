//! BO2's weapon pickup hint: the cursor hint of a dropped weapon in the player's use list (t6mp
//! `FUN_004dbbe0`, hint type 7, with the text of `FUN_004db920`).
//!
//! * **Where:** `common_mp`'s `cursorhints.menu`: menu `rect 0 70 40 40 9 9` (the safe area's
//!   centre, 70 lines below it), its one item ownerdraw 72 at `rect -20 0 40 40`, `textscale
//!   0.315`. The text is centred on the item with its pen half a text height below the item's
//!   top; the icon under it, `textHeight × 2 − rect.h × 0.5 × heightScale` below the top.
//! * **What:** `PLATFORM_PICKUPNEWWEAPON` ("Press ^3&&1^7 to pick up") with fewer than two
//!   primaries, else `PLATFORM_SWAPWEAPONS` ("Press ^3&&1^7 for"), then the weapon's name and each
//!   attachment's (`FUN_008eb300`), the use key's name for `&&1`, `^3` yellow and `^7` white.
//! * **The icon:** the weapon's `hudIcon` at the item's size, twice as wide when its
//!   `hudIconRatio` is 2:1 or 4:1 and half as tall at 4:1.

use super::frame::{argb, Lui};
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};
use crate::weapons::WeaponDef;

/// `cursorhints.menu`'s rect y (480 lines below the centre), size and text height.
const HINT_Y_480: f32 = 70.0;
const HINT_SIZE_480: f32 = 40.0;
const HINT_TEXT_480: f32 = 48.0 * 0.315;

/// The icon's width and height scales for a `hudIconRatio` (`FUN_004dbbe0`: 2 and 1 for 2:1, 2 and
/// 0.5 for 4:1).
pub fn icon_scale(ratio: u8) -> (f32, f32) {
    match ratio {
        0 => (1.0, 1.0),
        1 => (2.0, 1.0),
        _ => (2.0, 0.5),
    }
}

/// The name the hint gives the weapon: its name, then each attachment's.
pub fn pickup_name(def: &WeaponDef) -> String {
    let cat = crate::weapons::catalog();
    let base = cat.get(crate::weapons::base_of(&def.id)).unwrap_or(def);
    let mut name = base.display_name.clone();
    for short in &def.attachments {
        let att = base.allowed.iter().find(|(s, _)| s == short).and_then(|(_, asset)| cat.attachment(asset));
        if let Some(a) = att.filter(|a| !a.display_name.is_empty()) {
            name.push(' ');
            name.push_str(&a.display_name);
        }
    }
    name
}

/// A localized hint as coloured runs: `&&1` is `key`, `^3` turns yellow, `^7` (and any other code)
/// white.
pub fn coloured_runs(text: &str, key: &str) -> Vec<(String, (f32, f32, f32))> {
    let text = text.replace("&&1", key);
    let mut out: Vec<(String, (f32, f32, f32))> = Vec::new();
    let mut rgb = (1.0, 1.0, 1.0);
    let mut run = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(|d| d.is_ascii_digit()) {
            let code = chars.next().unwrap_or('7');
            if !run.is_empty() {
                out.push((std::mem::take(&mut run), rgb));
            }
            rgb = if code == '3' { (1.0, 1.0, 0.0) } else { (1.0, 1.0, 1.0) };
            continue;
        }
        run.push(c);
    }
    if !run.is_empty() {
        out.push((run, rgb));
    }
    out
}

/// The hint for the dropped weapon `p`'s use key would take now, if any.
pub(super) fn draw(l: &Lui, prims: &mut Vec<Prim>, sim: &Sim, p: &PlayerSim, key: &str) {
    let Some(i) = sim.use_target(p) else { return };
    let def = sim.drops[i].weapon.def;
    let text_key = if p.arsenal.primaries() < 2 { "PLATFORM_PICKUPNEWWEAPON" } else { "PLATFORM_SWAPWEAPONS" };
    let mut runs = coloured_runs(crate::sim::hint_text(text_key), key);
    runs.push((format!(" {}", pickup_name(def)), (1.0, 1.0, 1.0)));
    // LUI's 720-line space is 1.5 x the menus' 480.
    let (top, size, height) = (HINT_Y_480 * 1.5, HINT_SIZE_480 * 1.5, HINT_TEXT_480 * 1.5);
    let pen = top + height * 0.5;
    let widths: Vec<f32> = runs.iter().map(|(t, _)| l.text_width(Family::Condensed, t, height)).collect();
    let total: f32 = widths.iter().sum();
    let row = Place::new((true, true, 0.0, 0.0), (false, false, pen - height, pen)).resolve(l.screen());
    let mut x = row.x + row.w * 0.5 - total * 0.5;
    for ((text, rgb), w) in runs.iter().zip(&widths) {
        let r = Rect { x, w: *w, ..row };
        l.text(prims, Rect { x: x + 1.5, y: r.y + 1.5, ..r }, Family::Condensed, text, Some(Align::Left), (true, false), (0.0, 0.0, 0.0), 1.0);
        l.text(prims, r, Family::Condensed, text, Some(Align::Left), (true, false), *rgb, 1.0);
        x += w;
    }
    // BO2's hint draws the weapon's `hudIcon` by its `hudIconRatio`: both the base gun's (an attached
    // variant names its gun's icon, `classicons::weapon_icon`).
    let base = crate::weapons::weapon_or_none(def.id.split('+').next().unwrap_or(&def.id)).unwrap_or(def);
    let (ws, hs) = icon_scale(base.hud_icon_ratio);
    let (iw, ih) = (size * ws, size * hs);
    let icon_top = top + height * 2.0 - size * 0.5 * hs;
    let r = Place::new((false, false, -iw * 0.5, iw * 0.5), (false, false, icon_top, icon_top + ih)).resolve(l.screen());
    let px = l.px(r);
    prims.push(Prim::Image { name: base.hud_icon_image(), x: px.x, y: px.y, w: px.w, h: px.h, argb: argb((1.0, 1.0, 1.0), 1.0), fallback: Vec::new() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_text_colours_the_key_and_the_icon_keeps_its_ratio() {
        let runs = coloured_runs("Press ^3&&1^7 for", "F");
        assert_eq!(runs, vec![("Press ".to_string(), (1.0, 1.0, 1.0)), ("F".to_string(), (1.0, 1.0, 0.0)), (" for".to_string(), (1.0, 1.0, 1.0))]);
        assert_eq!(icon_scale(0), (1.0, 1.0));
        assert_eq!(icon_scale(1), (2.0, 1.0));
        assert_eq!(icon_scale(2), (2.0, 0.5));
    }

    #[test]
    fn the_hint_names_the_weapon_and_its_attachments() {
        let def = crate::weapons::catalog().get("an94_mp+reflex").expect("a combination");
        let cat = crate::weapons::catalog();
        let reflex = cat.all().iter().find_map(|w| w.allowed.iter().find(|(s, _)| s == "reflex")).and_then(|(_, a)| cat.attachment(a)).expect("the reflex sight");
        assert_eq!(pickup_name(def), format!("{} {}", cat.get("an94_mp").unwrap().display_name, reflex.display_name));
    }

    #[test]
    fn the_hint_shows_for_the_targeted_weapon_only() {
        use crate::sim::drops::Dropped;
        use crate::weapons::{weapon, Carried, Loadout};
        let mut sim = Sim::new();
        let mut p = PlayerSim::spawn_for_tests();
        p.arsenal = crate::weapons::Arsenal::with_loadout(Loadout::unarmed().with_spec("primary=an94_mp;secondary=fnp45_mp"));
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw(&l, &mut prims, &sim, &p, "F");
        assert!(prims.is_empty(), "nothing in reach");
        let weapon = Carried { def: weapon("svu_mp"), clip: 1, stock: 0, alt_clip: 0, alt_stock: 0, options: Default::default() };
        sim.drops.push(Dropped { weapon, pos: Vec3::new(crate::units::inches(50.0), 0.0, 0.0), vel: Vec3::ZERO, resting: true, yaw: 0.0, expires: None, owner: 0, owner_until: 0, object: 0 });
        draw(&l, &mut prims, &sim, &p, "F");
        // The def's HUD icon (`HudIcon`), the sprite the class screens draw for it.
        let svu_icon = crate::weapons::weapon("svu_mp").hud_icon_image();
        assert!(!svu_icon.is_empty() && svu_icon == crate::classicons::weapon_icon("svu_mp"));
        let icon = prims.iter().find_map(|q| match q {
            Prim::Image { name, x, y, w, h, .. } if *name == svu_icon => Some((*x, *y, *w, *h)),
            _ => None,
        });
        let (x, y, w, h) = icon.expect("the icon");
        // Centred under the text, the item's 40 x 40 (480 lines) times the ratio's scales.
        let (ws, hs) = icon_scale(crate::weapons::weapon("svu_mp").hud_icon_ratio);
        let unit = 720.0 / 480.0;
        assert!((w - 40.0 * ws * unit).abs() < 0.5 && (h - 40.0 * hs * unit).abs() < 0.5, "{w} x {h}");
        assert!((x + w * 0.5 - 640.0).abs() < 0.5 && y > 360.0, "centred below the middle: {x}, {y}");
        assert!(!prims.is_empty());
    }
}
