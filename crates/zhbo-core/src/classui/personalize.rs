//! BO2's personalize screens: camouflage, reticles, reticle colour and knife.
//!
//! The pure editor lives in `classmenu.rs` ([`crate::classmenu::Personalize`]); this module owns
//! the drawing and pointer hit-testing. The geometry is the Lua's own (`caccamomenu.lua:7-24`,
//! `cacreticles.lua:7-24`): the grid starts `GridTop` 60 units under the menu's top, its cells are
//! `GridItemSpacing` 8 apart, and each cell is `50 + TitleHeight + 20` tall. The camo and knife
//! grids are six columns wide (`CACCamo.CamoGridColumns = 6`); the reticle and reticle-colour
//! grids are five (`CACReticles.GridColumns = 5`).

use super::bo2::{self, argb, Frame, BOII_ORANGE, OFF_WHITE};
use super::*;
use crate::classmenu::{Draft, Edges, Outcome, Personalize, PersonalizeKind};
use crate::lui::font::{Align, Family};
use crate::lui::Rect;

/// `CoD.CACCamo.GridTop` / `CoD.CACReticles.GridTop` (`caccamomenu.lua:7`, `cacreticles.lua:7`).
pub const PERSONALIZE_GRID_TOP: f32 = 60.0;
/// `CoD.CACCamo.GridItemSpacing` / `CoD.CACReticles.GridItemSpacing`.
pub const GRID_ITEM_SPACING: f32 = 8.0;
/// `CamoGridItemHeight` / `GridItemHeight`: `50 + ContentGridButton.TitleHeight + 20`
/// (`caccamomenu.lua:20-24`, `cacreticles.lua:20-24`; `TitleHeight` is the ExtraSmall text size).
pub const GRID_ITEM_H: f32 = 50.0 + bo2::TEXT_EXTRA_SMALL + 20.0;

/// One personalize screen's geometry, in LUI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PersonalizeLayout {
    pub frame: Frame,
    /// The title row, menu-relative.
    pub title: Rect,
    /// The grid's first cell's top-left corner, menu-relative.
    pub grid: Rect,
    /// The grid's column count, BO2's own for this screen.
    pub columns: usize,
}

impl PersonalizeLayout {
    /// The grid's cell width: `(Menu.Width - Spacing * (Columns - 1)) / Columns`
    /// (`caccamomenu.lua:11-19`, `cacreticles.lua:11-19`).
    pub fn cell_w(&self) -> f32 {
        (bo2::MENU_W - GRID_ITEM_SPACING * (self.columns as f32 - 1.0)) / self.columns as f32
    }

    /// The cell's box in menu-relative units.
    pub fn cell_rect(&self, i: usize) -> Rect {
        let w = self.cell_w();
        let col = i % self.columns;
        let row = i / self.columns;
        Rect {
            x: self.grid.x + col as f32 * (w + GRID_ITEM_SPACING),
            y: self.grid.y + row as f32 * (GRID_ITEM_H + GRID_ITEM_SPACING),
            w,
            h: GRID_ITEM_H,
        }
    }

    /// The cell under a screen point, as its index in the choices list. Gaps between cells miss.
    pub fn cell_at(&self, choices: usize, x: f32, y: f32) -> Option<usize> {
        if self.columns == 0 || choices == 0 {
            return None;
        }
        let p = self.frame.px(self.grid);
        let cw = self.cell_w() * self.frame.s();
        let ch = GRID_ITEM_H * self.frame.s();
        let gap = GRID_ITEM_SPACING * self.frame.s();
        if x < p.x || y < p.y {
            return None;
        }
        let col = ((x - p.x) / (cw + gap)) as usize;
        let row = ((y - p.y) / (ch + gap)) as usize;
        if col >= self.columns {
            return None;
        }
        let i = row * self.columns + col;
        if i >= choices {
            return None;
        }
        let cell_x = p.x + col as f32 * (cw + gap);
        let cell_y = p.y + row as f32 * (ch + gap);
        if x > cell_x + cw || y > cell_y + ch {
            return None;
        }
        Some(i)
    }
}

/// Build the layout for one personalize screen. `kind` selects the grid's column count and title.
pub fn personalize_layout(w: f32, h: f32, kind: PersonalizeKind) -> PersonalizeLayout {
    let frame = Frame::new(w, h);
    let title = Rect { x: frame.menu.x, y: frame.menu.y + 20.0, w: bo2::MENU_W, h: bo2::TEXT_BIG };
    let grid = Rect { x: frame.menu.x, y: frame.menu.y + PERSONALIZE_GRID_TOP, w: bo2::MENU_W, h: bo2::MENU_H - PERSONALIZE_GRID_TOP };
    PersonalizeLayout { frame, title, grid, columns: kind.columns() }
}

/// The title BO2's Lua shows for each screen (`caccamomenu.lua:304`: `MPUI_CAMO_CAPS`;
/// `cacreticles.lua:224`: `MPUI_RETICLES_CAPS`; the colour and knife prompts are the class editor's
/// own wording, `MPUI_PERSONALIZE_KNIFE` at `caceditclass.lua:50-56`).
pub fn personalize_title(kind: PersonalizeKind) -> &'static str {
    match kind {
        PersonalizeKind::Camo => "CAMO",
        PersonalizeKind::Reticle => "RETICLES",
        PersonalizeKind::ReticleColor => "RETICLE COLOR",
        PersonalizeKind::Knife => "KNIFE",
    }
}

/// The host's choice lists for the personalize screens. The camo and knife lists are BO2's
/// captured names, `None` first as the grid's first cell (`i-personalize/001`, `002`); the
/// reticle and colour lists are the stock options until the converter supplies the weapon's own
/// set (M2).
pub fn personalize_choices(kind: PersonalizeKind) -> Vec<String> {
    let names: &[&str] = match kind {
        PersonalizeKind::Camo | PersonalizeKind::Knife => &[
            "None",
            "Gold",
            "Diamond",
            "Beast",
            "Bengal",
            "Blue",
            "Cherry",
            "Carbon",
            "Kryptek Typhon",
            "Ghostex Delta 6",
            "Bloodshot",
            "Blue Tiger",
            "Art of War",
            "Ronin",
            "Skulls",
        ],
        PersonalizeKind::Reticle => &["None", "Reflex", "ACOG", "EOTech", "MARS"],
        PersonalizeKind::ReticleColor => &["Red", "Green", "Blue", "Yellow", "Purple"],
    };
    names.iter().map(|s| (*s).to_string()).collect()
}

/// A cell's background: the same near-black plate as the editor's grid buttons.
fn button_bg(prims: &mut Vec<Prim>, f: &Frame, r: Rect) {
    let p = f.px(r);
    let fallback = vec![Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb: argb(BUTTON_TINT, BUTTON_ALPHA + 0.1) }];
    bo2::image(prims, f, "menu_mp_cac_grad_stretch", r, argb(BUTTON_TINT, BUTTON_ALPHA + 0.35), fallback);
}

/// Draw the open personalize screen: its title, the choices as a grid of buttons, the highlighted
/// cell in BO2's orange, and the `ESC Back` / `ENTER Select` footer.
pub fn draw_personalize(prims: &mut Vec<Prim>, w: f32, h: f32, p: &Personalize, weapon: Option<&str>, deny: f32, _now: f32) {
    let l = personalize_layout(w, h, p.kind);
    let f = &l.frame;
    bo2::backdrop(prims, f, w, h);
    bo2::title(prims, f, personalize_title(p.kind));
    // BO2's `Equipped` label and the chosen camo's name (`caccamomenu.lua`'s `CamoEquip`), at
    // the title's right.
    bo2::text(
        prims,
        f,
        Family::ExtraSmall,
        "EQUIPPED",
        Rect { x: f.menu.right() - 300.0, y: f.menu.y + 20.0, w: 300.0, h: bo2::TEXT_EXTRA_SMALL },
        Align::Left,
        argb(OFF_WHITE, 0.5),
    );
    if let Some(name) = p.choices.get(p.highlight) {
        bo2::text(
            prims,
            f,
            Family::Condensed,
            name,
            Rect { x: f.menu.right() - 300.0, y: f.menu.y + 20.0 + bo2::TEXT_EXTRA_SMALL, w: 300.0, h: bo2::TEXT_CONDENSED },
            Align::Left,
            argb(OFF_WHITE, 1.0),
        );
    }
    // The weapon preview: the class editor's gun, bottom-right under the grid.
    if let Some(id) = weapon {
        let def = crate::weapons::weapon(id);
        let preview = Rect { x: f.menu.right() - 260.0, y: f.menu.bottom() - 200.0, w: 260.0, h: 120.0 };
        bo2::text(prims, f, Family::Big, &def.display_name, preview, Align::Right, argb(OFF_WHITE, 1.0));
    }
    for (i, name) in p.choices.iter().enumerate() {
        let r = l.cell_rect(i);
        button_bg(prims, f, r);
        if i == p.highlight {
            bo2::border(prims, f, r, 2.0, argb(BOII_ORANGE, 1.0));
        }
        bo2::text(
            prims,
            f,
            Family::Condensed,
            name,
            Rect { x: r.x + 3.0, y: r.bottom() - bo2::TEXT_CONDENSED - 2.0, w: r.w, h: bo2::TEXT_CONDENSED },
            Align::Left,
            argb(if i == p.highlight { BOII_ORANGE } else { OFF_WHITE }, 1.0),
        );
        // The chosen cell carries BO2's `*` marker in the corner, as the editor's grids do.
        let chosen = p.choices.get(p.highlight).is_some_and(|v| v == name);
        if chosen {
            bo2::text(
                prims,
                f,
                Family::Condensed,
                "*",
                Rect { x: r.right() - 20.0, y: r.y + 2.0, w: 20.0, h: bo2::TEXT_CONDENSED },
                Align::Center,
                argb(bo2::YELLOW_GLOW, 1.0),
            );
        }
    }
    if p.choices.is_empty() {
        bo2::text(
            prims,
            f,
            Family::Condensed,
            "NOTHING TO CHOOSE",
            Rect { x: f.menu.x, y: f.menu.y + PERSONALIZE_GRID_TOP, w: 400.0, h: bo2::TEXT_CONDENSED },
            Align::Left,
            argb(OFF_WHITE, 0.5),
        );
    }
    // The screen is a submenu of the editor: BO2's personalize screens carry `ESC Back` and the
    // select prompt, matching the other content grids (`c-primary/001`, `d-attach/002`).
    let boxes = bo2::prompt_boxes(f, &[("ESC", "Back"), ("ENTER", "Select")], &[]);
    bo2::draw_prompts(prims, f, &boxes);
    deny_flash(prims, w, h, f.s(), deny);
}

/// One frame of the personalize screen: pointer hover/click and key edges folded into the pure
/// editor. The returned outcome tells the host whether the screen closed and whether the draft
/// changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonalizeFrame {
    pub outcome: Outcome,
    pub denied: bool,
    pub cue: Option<&'static str>,
}

/// Fold one frame. The pointer's hover moves the highlight only on a real move; a click confirms
/// the cell under it. Escape backs out without applying.
pub fn personalize_frame(personalize: &mut Personalize, draft: &mut Draft, inp: &MenuPointer, prev: &MenuPrev) -> PersonalizeFrame {
    let mut edges = Edges::between(&prev.keys, &inp.keys);
    let l = personalize_layout(inp.screen_w, inp.screen_h, personalize.kind);
    let hover = l.cell_at(personalize.choices.len(), inp.mouse_x, inp.mouse_y);
    if prev.moved(inp) {
        if let Some(i) = hover {
            personalize.highlight = i;
        }
    }
    if prev.clicked(inp) {
        if let Some(i) = hover {
            personalize.highlight = i;
            edges.confirm = true;
        }
    }
    let outcome = personalize.step(edges, draft);
    let navigated = prev.edge(inp, Key::Up)
        || prev.edge(inp, Key::Down)
        || prev.edge(inp, Key::Left)
        || prev.edge(inp, Key::Right);
    let cue = match outcome {
        Outcome::Save => Some(cues::GRID_EQUIP),
        Outcome::Cancel => Some(cues::BACKOUT),
        Outcome::Continue if navigated => Some(cues::GRID_NAV),
        Outcome::Continue => None,
    };
    PersonalizeFrame { outcome, denied: false, cue }
}
