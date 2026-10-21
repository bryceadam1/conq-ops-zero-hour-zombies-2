//! BO2's scoreboard, held on Tab (`patch_mp/ui_mp/t6/hud/scoreboard.lua`,
//! `scoreboardplayerrow.lua`). BO2's columns come from the engine per gametype
//! (`Engine.GetScoreBoardColumnName`); Conquer Ops shows Team Deathmatch's set: SCORE, KILLS,
//! DEATHS, ASSISTS (`MENU_SCORE_CAPS`, `MENU_KILLS_CAPS`, `MENU_DEATHS_CAPS`, `MPUI_ASSISTS_CAPS`
//! in `en_code_post_gfx_mp`).
//!
//! Structure as BO2's: the `MENU_SCOREBOARD_CAPS` title, then a block per team with the faction
//! icon, the team score in `Big` and the faction name in `ExtraSmall` 38 below its top
//! (scoreboard.lua:1269-1311), the column headers, and one row per player on a white plate at alpha
//! 0.06 (scoreboard.lua `0.0599` at the row plate). The dimensions (row 27 tall and 2 apart, the
//! 232-wide name column, 82-wide value columns) are the file's top-level constants as the
//! bytecode lists them; their binding to each box is not fully decoded, so the widths are
//! approximate.
//!
//! Deliberate divergence: the core knows only the local player's own tally (score = kills ×
//! `KILL_POINTS`), so the board shows the player's row under their Zero Hour army; other players
//! need the engine to list them.

use super::frame::Lui;
use super::*;
use crate::lui::font::{Align, Family};
use crate::lui::{Place, Rect};

pub const ROW_H: f32 = 27.0;
pub const ROW_GAP: f32 = 2.0;
pub const NAME_W: f32 = 232.0;
pub const COL_W: f32 = 82.0;
pub const RANK_W: f32 = 66.0;
pub const HEADER_H: f32 = 100.0;
pub const COLUMNS: [&str; 4] = ["SCORE", "KILLS", "DEATHS", "ASSISTS"];

/// One scoreboard row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub values: [u32; 4],
    pub you: bool,
}

/// The board's box, centred.
pub fn board(l: &Lui) -> Rect {
    let w = RANK_W + NAME_W + COL_W * COLUMNS.len() as f32;
    Place::new((false, false, -w * 0.5, w * 0.5), (true, false, 80.0, 640.0)).resolve(l.screen())
}

pub fn draw(l: &Lui, prims: &mut Vec<Prim>, team_color: u32, rows: &[Row]) {
    let b = board(l);
    let title = Rect { x: b.x, y: b.y, w: b.w, h: 25.0 };
    l.text(prims, title, Family::Default, "SCOREBOARD", Some(Align::Left), (true, false), (1.0, 1.0, 1.0), 1.0);
    // The team header: the icon, the team's score, the army's colour as the plate.
    let header = Rect { x: b.x, y: b.y + 30.0, w: b.w, h: HEADER_H };
    let hp = l.px(header);
    prims.push(Prim::Rect { x: hp.x, y: hp.y, w: hp.w, h: hp.h, argb: (team_color & 0x00FF_FFFF) | 0x4000_0000 });
    let icon = l.px(Rect { x: header.x + 18.0, y: header.y + 18.0, w: 64.0, h: 64.0 });
    prims.push(Prim::Image { name: sprites::TEAM_LOGO.to_string(), x: icon.x, y: icon.y, w: icon.w, h: icon.h, argb: 0xFFFF_FFFF, fallback: Vec::new() });
    let total: u32 = rows.iter().map(|r| r.values[0]).sum();
    l.text(prims, Rect { x: header.x + 100.0, y: header.y - 4.0, w: 300.0, h: 48.0 }, Family::Big, &total.to_string(), Some(Align::Left), (true, false), (1.0, 1.0, 1.0), 1.0);
    // The column headers.
    let heads = header.y + HEADER_H + 4.0;
    let mut x = b.x + RANK_W + NAME_W;
    for c in COLUMNS {
        l.text(prims, Rect { x, y: heads, w: COL_W, h: 20.0 }, Family::ExtraSmall, c, Some(Align::Center), (true, false), (1.0, 1.0, 1.0), 0.7);
        x += COL_W;
    }
    // The rows.
    let mut y = heads + 24.0;
    for (i, r) in rows.iter().enumerate() {
        let rp = l.px(Rect { x: b.x, y, w: b.w, h: ROW_H });
        prims.push(Prim::Rect { x: rp.x, y: rp.y, w: rp.w, h: rp.h, argb: if r.you { 0x26FF_FFFF } else { 0x0FFF_FFFF } });
        let text_box = |x: f32, w: f32| Rect { x, y: y + 2.0, w, h: ROW_H - 4.0 };
        l.text(prims, text_box(b.x, RANK_W), Family::Condensed, &(i + 1).to_string(), Some(Align::Center), (true, false), (1.0, 1.0, 1.0), 1.0);
        l.text(prims, text_box(b.x + RANK_W, NAME_W), Family::Condensed, &r.name, Some(Align::Left), (true, false), (1.0, 1.0, 1.0), 1.0);
        let mut x = b.x + RANK_W + NAME_W;
        for v in r.values {
            l.text(prims, text_box(x, COL_W), Family::Condensed, &v.to_string(), Some(Align::Center), (true, false), (1.0, 1.0, 1.0), 1.0);
            x += COL_W;
        }
        y += ROW_H + ROW_GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_the_tdm_columns_and_the_players_row() {
        let _f = crate::lui::font::lock();
        crate::lui::font::load("");
        let l = Lui::new(1280.0, 720.0);
        let mut prims = Vec::new();
        draw(&l, &mut prims, 0xFF22_44AA, &[Row { name: "General".into(), values: [300, 3, 1, 0], you: true }]);
        let texts: Vec<_> = prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.as_str()) } else { None }).collect();
        for want in ["SCOREBOARD", "SCORE", "KILLS", "DEATHS", "ASSISTS", "General", "300", "3", "1"] {
            assert!(texts.contains(&want), "{want} in {texts:?}");
        }
        let b = board(&l);
        assert_eq!(b.w, 66.0 + 232.0 + 328.0);
        assert_eq!(b.x, 640.0 - b.w * 0.5);
    }
}
