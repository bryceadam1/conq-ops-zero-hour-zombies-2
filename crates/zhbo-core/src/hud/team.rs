//! The team panel bottom-left: the faction emblem and the funds.

use super::*;

/// BO2's team panel, bottom-left: its frame, the emblem plate, and the team's numbers. The emblem is
/// Zero Hour's logo for the player's faction, and the number is the player's funds. Above it, BO2
/// names the scorestreak in use.
pub(super) fn draw_team_panel(host: &dyn ClientHost, prims: &mut Vec<Prim>, h: f32, s: f32, local_player: i32, tablet_up: bool) {
    let (fw, fh) = (176.0 * s, 88.0 * s);
    let (x, top) = (8.0 * s, h - fh - 4.0 * s);
    prims.push(Prim::Image { name: sprites::LEFT_LOWER_BACK.to_string(), x: x, y: top, w: fw, h: fh, argb: 0xC0FF_FFFF, fallback: slant_box(x + 10.0 * s, top + 20.0 * s, fw - 20.0 * s, fh - 26.0 * s, PANEL, None) });
    prims.push(Prim::Image { name: sprites::LEFT_LOWER.to_string(), x: x, y: top, w: fw, h: fh, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    // The emblem plate and the faction logo on it.
    let plate = 62.0 * s;
    let (px, py) = (x + 14.0 * s, top + fh - plate - 4.0 * s);
    prims.push(Prim::Image { name: sprites::FACTION_LIGHT.to_string(), x: px, y: py, w: plate, h: plate, argb: 0x90FF_FFFF, fallback: Vec::new() });
    prims.push(Prim::Image { name: sprites::FACTION_BACKING.to_string(), x: px, y: py, w: plate, h: plate, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    let logo = plate * 0.68;
    prims.push(Prim::Image { name: sprites::TEAM_LOGO.to_string(), x: px + (plate - logo) * 0.5, y: py + (plate - logo) * 0.5, w: logo, h: logo, argb: 0xFFFF_FFFF, fallback: Vec::new() });

    // Funds in BO2's numerals on the frame's upper line, labelled under it.
    let cash = host.cash(local_player).max(0) as u32;
    let right = x + fw - 14.0 * s;
    let height = 30.0 * s;
    let num_top = top + 22.0 * s;
    let left = draw_number(prims, cash, right, num_top, height, WHITE);
    prims.push(Prim::Text { x: left - 9.0 * s, y: num_top + height * 0.22, text: "$".to_string(), argb: WHITE, size: (16.0 * s) as i32, align: 2 });
    prims.push(Prim::Text { x: right, y: num_top + height + 4.0 * s, text: "FUNDS".to_string(), argb: DIM, size: (10.0 * s) as i32, align: 2 });

    // The scorestreak in use, BO2's big bold name with its status under it.
    if tablet_up {
        prims.push(Prim::Text { x: x + 6.0 * s, y: top - 50.0 * s, text: "FIELD COMPUTER".to_string(), argb: WHITE, size: (30.0 * s) as i32, align: 0 });
        prims.push(Prim::Text { x: x + 8.0 * s, y: top - 12.0 * s, text: "Establishing tactical view".to_string(), argb: WHITE, size: (12.0 * s) as i32, align: 0 });
    }
}
