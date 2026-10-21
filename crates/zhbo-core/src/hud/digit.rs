//! BO2's HUD digit (`patch_mp/ui_mp/t6/hud/huddigit.lua`): a 32 x 64 numeral (`Width`,
//! `BigNumbersHeight`) drawn twice, the `_white` art behind at `CoD.offWhite` and alpha 0.6, the
//! plain art in front (huddigit.lua:41-124; the two tables are bound to background and foreground
//! by `setDigit`'s upvalues 3 and 4, `t6lua.py disasm`). Digits step `Spacing` 22 apart, so they
//! overlap; the small ones (`SmallDigitScale` 0.75) sit `SmallDigitHeightDifference` 12 lower.
//! A low clip pulses both layers between white and red every 500 ms (huddigit.lua:24, 419-464).

use super::frame::{Lui, OFF_WHITE};
use super::Prim;
use crate::lui::Rect;

pub const WIDTH: f32 = 32.0;
pub const SPACING: f32 = 22.0;
pub const BIG_BOTTOM_Y: f32 = -87.0;
pub const BIG_HEIGHT: f32 = 64.0;
pub const SMALL_SCALE: f32 = 0.75;
pub const SMALL_HEIGHT_OFFSET: f32 = 4.0;
pub const SMALL_HEIGHT_DIFFERENCE: f32 = 12.0;
/// `CoD.HUDDigit.Slash` / `Line`: the glyph indices after 0-9.
pub const SLASH: usize = 10;
pub const LINE: usize = 11;
/// The red pulse's half period (huddigit.lua:24, `L3_1 = 500`).
pub const PULSE_MS: f32 = 500.0;

/// The BO2 material of glyph `g` (0-9, [`SLASH`], [`LINE`]), `white` for the background layer.
pub fn material(g: usize, white: bool) -> String {
    let name = match g {
        SLASH => "slash".to_string(),
        LINE => "line".to_string(),
        d => d.min(9).to_string(),
    };
    format!("hud_mp_num_big_{name}{}", if white { "_white" } else { "" })
}

/// Every digit material, for the converter's list.
pub const MATERIALS: [&str; 24] = [
    "hud_mp_num_big_0", "hud_mp_num_big_1", "hud_mp_num_big_2", "hud_mp_num_big_3", "hud_mp_num_big_4",
    "hud_mp_num_big_5", "hud_mp_num_big_6", "hud_mp_num_big_7", "hud_mp_num_big_8", "hud_mp_num_big_9",
    "hud_mp_num_big_slash", "hud_mp_num_big_line",
    "hud_mp_num_big_0_white", "hud_mp_num_big_1_white", "hud_mp_num_big_2_white", "hud_mp_num_big_3_white",
    "hud_mp_num_big_4_white", "hud_mp_num_big_5_white", "hud_mp_num_big_6_white", "hud_mp_num_big_7_white",
    "hud_mp_num_big_8_white", "hud_mp_num_big_9_white", "hud_mp_num_big_slash_white", "hud_mp_num_big_line_white",
];

/// How red a pulsing digit is at `clock` seconds: up to red over 500 ms, back over 500 ms
/// (`beginAnimation("pulse_red", 500)` then `"normal"`, no easing).
pub fn pulse(clock: f32) -> f32 {
    let t = (clock * 1000.0 / PULSE_MS).rem_euclid(2.0);
    if t < 1.0 { t } else { 2.0 - t }
}

/// One digit in box `r` (LUI units); `red` 0..1 is the pulse.
pub fn draw(l: &Lui, prims: &mut Vec<Prim>, r: Rect, g: usize, red: f32, alpha: f32) {
    let tint = |base: (f32, f32, f32)| (base.0, base.1 * (1.0 - red), base.2 * (1.0 - red));
    l.image(prims, r, &material(g, true), tint(OFF_WHITE), 0.6 * alpha, Vec::new());
    let p = l.px(r);
    let glyph = match g {
        SLASH => "/".to_string(),
        LINE => "|".to_string(),
        d => d.to_string(),
    };
    let fallback = vec![Prim::Text { x: p.x + p.w * 0.5, y: p.y + p.h * 0.15, text: glyph, argb: super::frame::argb(tint((1.0, 1.0, 1.0)), alpha), size: (p.h * 0.7) as i32, align: 1 }];
    l.image(prims, r, &material(g, false), tint((1.0, 1.0, 1.0)), alpha, fallback);
}

/// `GetThreeDigits` / `GetFiveDigits`: the decimal digits of `v`, most significant first.
pub fn digits(v: u32, n: usize) -> Vec<usize> {
    (0..n).rev().map(|i| ((v / 10u32.pow(i as u32)) % 10) as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_pulse() {
        assert_eq!(material(3, false), "hud_mp_num_big_3");
        assert_eq!(material(SLASH, true), "hud_mp_num_big_slash_white");
        assert_eq!(digits(507, 3), vec![5, 0, 7]);
        assert_eq!(pulse(0.0), 0.0);
        assert_eq!(pulse(0.5), 1.0);
        assert_eq!(pulse(0.75), 0.5);
        assert!(MATERIALS.iter().all(|m| m.starts_with("hud_mp_num_big_")));
    }

    #[test]
    fn draws_background_then_foreground() {
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw(&l, &mut prims, Rect { x: 10.0, y: 20.0, w: WIDTH, h: BIG_HEIGHT }, 4, 0.0, 1.0);
        let names: Vec<_> = prims.iter().filter_map(|p| if let Prim::Image { name, argb, .. } = p { Some((name.clone(), *argb)) } else { None }).collect();
        assert_eq!(names, vec![("ZHBO_Mat_hud_mp_num_big_4_white".into(), 0x99FF_FFFF), ("ZHBO_Mat_hud_mp_num_big_4".into(), 0xFFFF_FFFF)]);
    }
}
