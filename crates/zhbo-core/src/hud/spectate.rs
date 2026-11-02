//! BO2's dead-spectate panel (`patch_mp/ui_mp/t6/hud/deadspectate.lua`), the spectating client's
//! one addition to the followed General's HUD: a 300 × 60 box centred 120 above the safe area's
//! bottom (`Width` 300, `Height` = `SwitchPlayerBarHeight` 22 + `EmblemSideLength` 38,
//! `Bottom` -120), holding
//!
//! * `hud_shoutcasting_change_tab` at x -76..150 from the centre, y 0..62 (deadspectate.lua:62-80);
//! * the switch-player bar, x 64..237 from the left, y -3..29: `DualButtonPrompt("shoulderl",
//!   MPUI_SPECTATING, "shoulderr", …, "mouse1", "mouse2")` — the attack button follows the previous
//!   player, the aim button the next (lua:81-150);
//! * `hud_shoutcasting_viewing_box_dead` at x -146..146, y `BodyStart` - 4 .. 68 (lua:151-178);
//! * the followed player's name, centred in the body (`BodyStart` + body/2 - `TextSize`/2), at
//!   `TextSize` (`CoD.textSize.Default`) (lua:179-205);
//! * the spectate-mode button below it (`ButtonPrompt("alt2", …, "mouse3")`, `TextSize` × 0.75 tall,
//!   `CoD.fonts.ExtraSmall`): BO2's third/first-person toggle, here the overhead view (lua:206-250).
//!
//! The panel shows only while following (`BIT_SPECTATING_CLIENT`, not in a killcam or a menu,
//! `UpdateVisibility`). Overhead, the spectator gets the one prompt back in: `MPUI_FIRST_PERSON_VIEW`
//! on the same key, while there is a General to follow.
//!
//! The button glyphs are the mod's key names in brackets (the HUD's other prompts' style); the
//! overhead view's label is the mod's own (BO2's toggle names its third-person camera).

use super::frame::{Lui, OFF_WHITE};
use super::Prim;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};

pub const WIDTH: f32 = 300.0;
pub const SWITCH_BAR_HEIGHT: f32 = 22.0;
pub const EMBLEM_SIDE: f32 = 38.0;
pub const HEIGHT: f32 = SWITCH_BAR_HEIGHT + EMBLEM_SIDE;
pub const BOTTOM: f32 = -120.0;
/// `CoD.textSize.Default`.
pub const TEXT_SIZE: f32 = 25.0;

pub const MATERIALS: [&str; 2] = ["hud_shoutcasting_change_tab", "hud_shoutcasting_viewing_box_dead"];

/// What the panel says: the followed player's name, and the keys (the switch pair and the view
/// toggle), as the HUD names them.
pub struct Panel<'a> {
    pub name: &'a str,
    pub prev_key: &'a str,
    pub next_key: &'a str,
    pub view_key: &'a str,
}

/// The panel's box: centred, its bottom 120 above the safe area's.
pub fn area(l: &Lui) -> Rect {
    Place::new((false, false, -WIDTH / 2.0, WIDTH / 2.0), (false, true, BOTTOM - HEIGHT, BOTTOM)).resolve(l.screen())
}

/// The panel while following.
pub fn draw(l: &Lui, prims: &mut Vec<Prim>, panel: &Panel) {
    let hud = area(l);
    let at = |h: (bool, bool, f32, f32), v: (bool, bool, f32, f32)| Place::new(h, v).resolve(hud);
    let fill = |r: Rect, argb: u32| {
        let p = l.px(r);
        vec![Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb }]
    };
    let tab = at((false, false, -76.0, 150.0), (true, false, 0.0, 62.0));
    l.image(prims, tab, MATERIALS[0], OFF_WHITE, 1.0, fill(tab, 0x8010_1214));
    let body = at((false, false, -146.0, 146.0), (true, false, SWITCH_BAR_HEIGHT - 4.0, 68.0));
    l.image(prims, body, MATERIALS[1], OFF_WHITE, 1.0, fill(body, 0xA010_1214));

    // The switch-player bar: [prev] Spectating [next], centred on the bar.
    let bar = at((true, false, 64.0, 237.0), (true, false, -3.0, 29.0));
    let label = crate::weapons::catalog().label("MPUI_SPECTATING");
    let row = Place::new((true, true, 0.0, 0.0), (false, false, -10.0, 10.0)).resolve(bar);
    let switch = format!("[{}]  {}  [{}]", panel.prev_key, label, panel.next_key);
    l.text(prims, row, Family::ExtraSmall, &switch, Some(Align::Center), (true, true), OFF_WHITE, 1.0);

    // The name, centred in the body.
    let top = SWITCH_BAR_HEIGHT + (HEIGHT - SWITCH_BAR_HEIGHT) / 2.0 - TEXT_SIZE / 2.0;
    let name = at((false, false, -WIDTH / 2.0, WIDTH / 2.0), (true, false, top, top + TEXT_SIZE));
    l.text(prims, name, Family::Default, panel.name, Some(Align::Center), (false, false), OFF_WHITE, 1.0);

    // The view button under it.
    let below = top + EMBLEM_SIDE;
    let mode = at((false, false, -WIDTH / 2.0, WIDTH / 2.0), (true, false, below, below + TEXT_SIZE * 0.75));
    l.text(prims, mode, Family::ExtraSmall, &format!("[{}] Overhead View", panel.view_key), Some(Align::Center), (false, false), OFF_WHITE, 1.0);
}

/// Overhead with someone to follow: the way back in, where the panel's view button sits.
pub fn draw_overhead_prompt(l: &Lui, prims: &mut Vec<Prim>, view_key: &str) {
    let hud = area(l);
    let below = SWITCH_BAR_HEIGHT + (HEIGHT - SWITCH_BAR_HEIGHT) / 2.0 - TEXT_SIZE / 2.0 + EMBLEM_SIDE;
    let mode = Place::new((false, false, -WIDTH / 2.0, WIDTH / 2.0), (true, false, below, below + TEXT_SIZE * 0.75)).resolve(hud);
    let text = format!("[{}] {}", view_key, crate::weapons::catalog().label("MPUI_FIRST_PERSON_VIEW"));
    l.text(prims, mode, Family::ExtraSmall, &text, Some(Align::Center), (false, false), OFF_WHITE, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_sits_where_deadspectate_puts_it() {
        let l = Lui::new(1280.0, 720.0);
        assert_eq!(area(&l), Rect { x: 490.0, y: 540.0, w: 300.0, h: 60.0 });
        let mut prims = Vec::new();
        draw(&l, &mut prims, &Panel { name: "Bryce", prev_key: "LMB", next_key: "RMB", view_key: "F" });
        let images: Vec<_> = prims.iter().filter_map(|p| if let Prim::Image { name, x, y, w, h, .. } = p { Some((name.clone(), *x, *y, *w, *h)) } else { None }).collect();
        assert!(images.contains(&("ZHBO_Mat_hud_shoutcasting_change_tab".into(), 640.0 - 76.0, 540.0, 226.0, 62.0)), "{images:?}");
        assert!(images.contains(&("ZHBO_Mat_hud_shoutcasting_viewing_box_dead".into(), 640.0 - 146.0, 558.0, 292.0, 50.0)), "{images:?}");
    }
}
