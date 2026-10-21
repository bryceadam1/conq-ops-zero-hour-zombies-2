//! BO2's score panel, bottom-left (`patch_mp/ui_mp/t6/hud/scorebottomleft.lua`). Its boxes are
//! offset from the safe area's bottom-left corner by the file's upvalues, recovered from the
//! bytecode (`t6lua.py disasm`: the `new` closure captures 28, 110, -87, 107, -48 in that order;
//! the decompiler names them all `arg`):
//!
//! * `hud_mp_vis_left_lower_back` 300 x 150 at x 28, tinted the team's faction colour
//!   (scorebottomleft.lua:45-75, 1098-1122), and `hud_mp_vis_left_lower` at x 3, y -128..22.
//! * `hud_mp_vis_score_highlight` 128 x 64 at x 56, y -92..-28.
//! * The faction icon 64 x 64 at x 68, centred 39 above the bottom (scorebottomleft.lua:1134-1144).
//! * The score in HUD digits from x 138, top -87 (`UpdateScores`, upvalues 0 + 1 and 2), each 22
//!   apart, packed from the first slot with leading zeros dropped (scorebottomleft.lua:895-978).
//!
//! Deliberate divergence: the team is the player's Zero Hour army, so the icon is Zero Hour's
//! faction logo, the backing takes that army's radar colour, and the number is the player's funds.
//! Zero Hour has no team score to set against it, no score limit and no round clock, so the enemy
//! digits, the winning status and the game timer are left out.

use super::digit;
use super::frame::Lui;
use super::sprites;
use super::Prim;
use crate::lui::{Place, Rect};

pub const LEFT: f32 = 28.0;
pub const SCORE_X: f32 = 110.0;
/// Five digits (`yourScoreDigits`).
pub const DIGITS: usize = 5;

pub const MATERIALS: [&str; 3] = ["hud_mp_vis_left_lower_back", "hud_mp_vis_left_lower", "hud_mp_vis_score_highlight"];

fn corner(area: Rect, left: f32, right: f32, top: f32, bottom: f32) -> Rect {
    Place::new((true, false, left, right), (false, true, top, bottom)).resolve(area)
}

/// The panel's parent: the safe area's left edge at its bottom (scorebottomleft.lua:23-39).
pub fn area(l: &Lui) -> Rect {
    Place::new((true, false, 0.0, 0.0), (false, true, 0.0, 0.0)).resolve(l.screen())
}

/// An ARGB team colour as LUI's rgb.
fn rgb(argb: u32) -> (f32, f32, f32) {
    let c = |s: u32| ((argb >> s) & 0xFF) as f32 / 255.0;
    (c(16), c(8), c(0))
}

/// The panel: `team_color` is the army's radar colour (ARGB), `score` the number shown.
pub fn draw(l: &Lui, prims: &mut Vec<Prim>, team_color: u32, score: u32) {
    let a = area(l);
    l.image(prims, corner(a, LEFT, LEFT + 300.0, -150.0, 0.0), "hud_mp_vis_left_lower_back", rgb(team_color), 1.0, Vec::new());
    l.image(prims, corner(a, LEFT - 25.0, LEFT - 25.0 + 300.0, -128.0, -128.0 + 150.0), "hud_mp_vis_left_lower", (1.0, 1.0, 1.0), 1.0, Vec::new());
    l.image(prims, corner(a, LEFT + 28.0, LEFT + 28.0 + 128.0, -28.0 - 64.0, -28.0), "hud_mp_vis_score_highlight", (1.0, 1.0, 1.0), 1.0, Vec::new());
    let icon = l.px(corner(a, LEFT + 40.0, LEFT + 40.0 + 64.0, -39.0 - 32.0, -39.0 + 32.0));
    prims.push(Prim::Image { name: sprites::TEAM_LOGO.to_string(), x: icon.x, y: icon.y, w: icon.w, h: icon.h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    // Packed from the first slot: leading zeros take no space (scorebottomleft.lua:907-977).
    let d = digit::digits(score.min(99_999), DIGITS);
    let first = d.iter().position(|&v| v > 0).unwrap_or(DIGITS - 1);
    let mut x = LEFT + SCORE_X;
    for &v in &d[first..] {
        let top = digit::BIG_BOTTOM_Y;
        digit::draw(l, prims, corner(a, x, x + digit::WIDTH, top, top + digit::BIG_HEIGHT), v, 0.0, 1.0);
        x += digit::SPACING;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images(prims: &[Prim]) -> Vec<(String, f32, f32, f32, f32, u32)> {
        prims.iter().filter_map(|p| match p {
            Prim::Image { name, x, y, w, h, argb, .. } => Some((name.clone(), *x, *y, *w, *h, *argb)),
            _ => None,
        }).collect()
    }

    #[test]
    fn panel_and_score_sit_where_the_lua_puts_them() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw(&l, &mut prims, 0xFF33_66CC, 2050);
        let im = images(&prims);
        assert_eq!(im[0], ("ZHBO_Mat_hud_mp_vis_left_lower_back".into(), 28.0, 570.0, 300.0, 150.0, 0xFF33_66CC));
        assert_eq!((im[1].1, im[1].2), (3.0, 592.0));
        assert!(im.iter().any(|i| i.0 == sprites::TEAM_LOGO && (i.1, i.2, i.3) == (68.0, 649.0, 64.0)));
        // 02050: the leading zero is dropped and the rest packs from 138.
        let digits: Vec<_> = im.iter().filter(|i| i.0.starts_with("ZHBO_Mat_hud_mp_num_big_") && !i.0.ends_with("_white")).map(|i| (i.0.clone(), i.1, i.2)).collect();
        assert_eq!(digits[0], ("ZHBO_Mat_hud_mp_num_big_2".into(), 138.0, 633.0));
        assert_eq!(digits.len(), 4);
    }

    #[test]
    fn a_zero_score_shows_one_digit_and_scales() {
        let l = Lui::new(1920.0, 1080.0);
        let mut prims = Vec::new();
        draw(&l, &mut prims, 0xFFFF_FFFF, 0);
        let digits: Vec<_> = images(&prims).into_iter().filter(|i| i.0 == "ZHBO_Mat_hud_mp_num_big_0").collect();
        assert_eq!(digits.len(), 1);
        assert_eq!((digits[0].1, digits[0].2), ((28.0 + 110.0) * 1.5, 1080.0 - 87.0 * 1.5));
    }
}
