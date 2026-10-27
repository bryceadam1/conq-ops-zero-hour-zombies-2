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
use crate::classmenu::{Draft, Edges, Outcome, Personalize, PersonalizeKind, PersonalizeTab};
use crate::weapons::{Catalog, OptionTab};
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

/// The fallback choice lists: the captured camo/knife names, `None` first (`i-personalize/001`,
/// `002`), and the stock reticle/colour options. [`personalize_choices_for`] replaces the camo list
/// with the weapon's converted `CamoList` when the catalog has one.
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

/// The host's choices for a personalize screen: the weapon's converted `CamoList`, the converted
/// reticle/colour tables, or the captured fallback when the catalog has no BO2 data. The first tab's
/// cells for a tabbed screen ([`personalize_tabs_for`]).
pub fn personalize_choices_for(kind: PersonalizeKind, weapon: Option<&str>) -> Vec<String> {
    personalize_tabs_for(kind, weapon).into_iter().next().map(|t| t.choices).unwrap_or_default()
}

/// `MPUI_CAMO_CAPS` / `MPUI_RETICLES_CAPS`, `MPUI_MAP_CATEGORY_DLC_CAPS`, `MPUI_DLC2_CAPS`.
fn tab_label(kind: PersonalizeKind, tab: OptionTab) -> &'static str {
    match (kind, tab) {
        (PersonalizeKind::Reticle, OptionTab::Base) => "RETICLES",
        (_, OptionTab::Base) => "CAMO",
        (_, OptionTab::Dlc) => "DLC",
        (_, OptionTab::Dlc2) => "DLC2",
    }
}

/// The screen's tabs as BO2 builds them. Camo (`caccamomenu.lua:274-341`) and reticles
/// (`cacreticles.lua:222-262`): the base tab, then DLC (always added), then DLC2 only when it has a
/// row (`ShouldDisplayDlc2Tab`). Each tab is option 0 (`camo_none`, `reticle_0`) followed by its
/// rows (`AddCamos` inserts index 0 first): the base and DLC tabs in table order, DLC2 by its sort
/// key (column 5). The weapon's converted list decides which rows exist (the converter drops the DLC
/// rows this install has no art for); EMBLEM AND TAG is not ported (the mod has no emblems or clan
/// tags). The colour and knife screens have one untitled tab.
pub fn personalize_tabs_for(kind: PersonalizeKind, weapon: Option<&str>) -> Vec<PersonalizeTab> {
    personalize_tabs_in(crate::weapons::catalog(), kind, weapon)
}

/// [`personalize_tabs_for`] over a given catalog.
pub fn personalize_tabs_in(cat: &Catalog, kind: PersonalizeKind, weapon: Option<&str>) -> Vec<PersonalizeTab> {
    let ids: Vec<String> = match kind {
        PersonalizeKind::Camo => weapon.and_then(|w| cat.get(w)).map(|d| d.camo.clone()).unwrap_or_default(),
        PersonalizeKind::Reticle => cat.reticles().iter().map(|r| r.id.clone()).collect(),
        PersonalizeKind::ReticleColor => {
            let ids: Vec<String> = cat.reticle_colors().iter().map(|c| c.id.clone()).collect();
            let choices = if ids.is_empty() { personalize_choices(kind) } else { ids };
            return vec![PersonalizeTab { label: String::new(), choices }];
        }
        PersonalizeKind::Knife => return vec![PersonalizeTab { label: String::new(), choices: personalize_choices(kind) }],
    };
    if ids.is_empty() {
        return vec![PersonalizeTab { label: tab_label(kind, OptionTab::Base).into(), choices: personalize_choices(kind) }];
    }
    let first = ids[0].clone();
    let rows = |tab: OptionTab| -> Vec<(u32, String)> {
        ids.iter()
            .skip(1)
            .filter_map(|id| {
                let o = option_of(cat, kind, id);
                (o.map_or(OptionTab::Base, |o| o.tab) == tab).then(|| (o.map_or(0, |o| o.sort), id.clone()))
            })
            .collect()
    };
    let mut tabs = Vec::new();
    for tab in [OptionTab::Base, OptionTab::Dlc, OptionTab::Dlc2] {
        let mut r = rows(tab);
        if tab == OptionTab::Dlc2 {
            if r.is_empty() {
                continue;
            }
            r.sort_by_key(|(sort, _)| *sort);
        }
        let mut choices = vec![first.clone()];
        choices.extend(r.into_iter().map(|(_, id)| id));
        tabs.push(PersonalizeTab { label: tab_label(kind, tab).into(), choices });
    }
    tabs
}

/// A cell's background: the same near-black plate as the editor's grid buttons.
fn button_bg(prims: &mut Vec<Prim>, f: &Frame, r: Rect) {
    let p = f.px(r);
    let fallback = vec![Prim::Rect { x: p.x, y: p.y, w: p.w, h: p.h, argb: argb(BUTTON_TINT, BUTTON_ALPHA + 0.1) }];
    bo2::image(prims, f, "menu_mp_cac_grad_stretch", r, argb(BUTTON_TINT, BUTTON_ALPHA + 0.35), fallback);
}

/// The option row behind a cell (`camo`, `reticle`, `reticle_color`).
fn option_of(cat: &Catalog, kind: PersonalizeKind, choice: &str) -> Option<&'static crate::weapons::CamoOption> {
    match kind {
        PersonalizeKind::Camo | PersonalizeKind::Knife => cat.camo(choice),
        PersonalizeKind::Reticle => cat.reticle(choice),
        PersonalizeKind::ReticleColor => cat.reticle_color(choice),
    }
}

/// The label a cell shows: the camo/reticle/colour option's localized name, or the choice itself
/// for the fallback lists.
fn choice_label(cat: &Catalog, kind: PersonalizeKind, choice: &str) -> String {
    option_of(cat, kind, choice).map(|c| c.name.clone()).unwrap_or_else(|| choice.to_string())
}

/// A camo option's thumbnail (`UIExpression.GetWeaponOptionImage`: column 6, `menu_camo_*_pattern`).
/// The converter writes it under that very name (`zhbo-assets` `hud.rs`), so it is drawn as a sprite,
/// not through the `ZHBO_Mat_` material namespace.
pub fn camo_image(cat: &Catalog, id: &str) -> Option<&'static str> {
    cat.camo(id).map(|c| c.image.as_str()).filter(|s| !s.is_empty())
}

/// What [`camo_swatch`] draws for `weapon` wearing `camo`: the sprite and its size (the 140 x 70
/// thumbnail, or a store option's 32-unit `<image>_32`), `None` for option 0 or an unknown id.
pub fn swatch_art(cat: &Catalog, weapon: &str, camo: &str) -> Option<(String, f32, f32)> {
    let d = cat.get(weapon)?;
    if camo.is_empty() || d.camo.first().is_some_and(|c| c == camo) {
        return None;
    }
    let o = cat.camo(camo).filter(|o| !o.image.is_empty())?;
    Some(if o.mtx {
        (format!("{}_32", o.image), 32.0, 32.0)
    } else if o.image.contains("_128") {
        (o.image.clone(), 32.0, 32.0)
    } else {
        (o.image.clone(), 140.0, 70.0)
    })
}

/// The camo swatch a weapon button carries for its class camo (`CACWeaponButton.GetCamoSwatch`,
/// `ClassPreview.GetCamoSwatch`): a stencilled `container` showing the bottom-right corner of the
/// option's 140 x 70 thumbnail; a store option draws its whole `<image>_32` in a 32-unit square at
/// the corner instead (`SetupMTXCamoSwatch`), as does a `_128` image. Option 0 (the weapon's first
/// row, `camo_none`) and an unknown id draw nothing (`if camo and camo ~= 0`).
pub fn camo_swatch(prims: &mut Vec<Prim>, f: &Frame, container: Rect, weapon: &str, camo: &str) {
    let Some((name, w, h)) = swatch_art(crate::weapons::catalog(), weapon, camo) else { return };
    let p = f.px(container);
    prims.push(Prim::ClipBegin { x: p.x, y: p.y, w: p.w, h: p.h });
    bo2::sprite(prims, f, &name, Rect { x: container.right() - w, y: container.bottom() - h, w, h }, argb(OFF_WHITE, 1.0), Vec::new());
    prims.push(Prim::ClipEnd);
}

/// `CoD.Menu.TitleHeight`: the personalize grid sits `GridTop = TitleHeight + 10` down.
pub const MENU_TITLE_H: f32 = PERSONALIZE_GRID_TOP - 10.0;
/// `CoD.TabManager.DefaultSpacing` and `SpaceBetweenTabsAndButtons` (`tabmanagerlite.lua:5-6`).
const TAB_SPACING: f32 = 20.0;
const TAB_TO_BUTTON: f32 = 10.0;
/// `CoD.gray`, the inactive tab's text: defined outside the decompiled Lua; measured off the
/// capture's inactive `DLC` tab (`i-personalize/001`, RGB 99/97/92).
pub const GRAY: (f32, f32, f32) = (0.39, 0.38, 0.36);

/// The tab strip's boxes: the `shoulderl` prompt, each tab, the `shoulderr` prompt.
#[derive(Clone, Debug, PartialEq)]
pub struct TabStrip {
    pub prev: Rect,
    pub tabs: Vec<Rect>,
    pub next: Rect,
}

/// `CoD.TabManager` right-aligned over the title row (`caccamomenu.lua:274-300`: top
/// `TitleHeight - ButtonPrompt.Height`, bottom `TitleHeight`): from the right, the `shoulderr`
/// prompt, 10 units, the tabs last to first 20 apart, 10 units, the `shoulderl` prompt
/// (`ResetLayout`). A tab is its Default-size label plus 5 a side, `Default + 4` tall, on the
/// strip's bottom (`AddTab`). `None` for a screen without tabs.
pub fn tab_strip(l: &PersonalizeLayout, labels: &[String]) -> Option<TabStrip> {
    if labels.len() < 2 {
        return None;
    }
    let f = &l.frame;
    let bottom = f.menu.y + MENU_TITLE_H;
    let a = bo2::PROMPT_H;
    let mut x = f.menu.right() - a;
    let next = Rect { x, y: bottom - a, w: a, h: a };
    x -= TAB_TO_BUTTON;
    let h = bo2::TEXT_DEFAULT + 4.0;
    let mut tabs = vec![Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; labels.len()];
    for (i, label) in labels.iter().enumerate().rev() {
        let w = bo2::text_w(Family::Default, label, bo2::TEXT_DEFAULT) + 10.0;
        x -= w;
        tabs[i] = Rect { x, y: bottom - h, w, h };
        if i > 0 {
            x -= TAB_SPACING;
        }
    }
    x -= TAB_TO_BUTTON + a;
    Some(TabStrip { prev: Rect { x, y: bottom - a, w: a, h: a }, tabs, next })
}

fn draw_tabs(prims: &mut Vec<Prim>, l: &PersonalizeLayout, p: &Personalize) {
    let labels: Vec<String> = p.tabs.iter().map(|t| t.label.clone()).collect();
    let Some(strip) = tab_strip(l, &labels) else { return };
    let f = &l.frame;
    super::pick::arrow(prims, f, strip.prev, -1);
    super::pick::arrow(prims, f, strip.next, 1);
    for (i, (r, label)) in strip.tabs.iter().zip(&labels).enumerate() {
        let current = i == p.tab;
        if current {
            // `HighlightCurrentTab`: the plate (`menu_mp_tab`, top -1 / bottom +4) and its 4-unit
            // end cap at alpha 1, the label in the selection colour (off-white).
            let plate = Rect { x: r.x, y: r.y - 1.0, w: r.w, h: r.h + 5.0 };
            let px = f.px(plate);
            bo2::image(prims, f, "menu_mp_tab", plate, argb(OFF_WHITE, 1.0), vec![Prim::Rect { x: px.x, y: px.y, w: px.w, h: px.h, argb: argb(bo2::BLACK, 1.0) }]);
            bo2::image(prims, f, "menu_mp_tab_end", Rect { x: r.right() - 4.0, y: plate.y, w: 4.0, h: plate.h }, argb(OFF_WHITE, 1.0), Vec::new());
        }
        let text = Rect { x: r.x, y: r.y + (r.h - bo2::TEXT_DEFAULT) / 2.0, w: r.w, h: bo2::TEXT_DEFAULT };
        bo2::text(prims, f, Family::Default, label, text, Align::Center, argb(if current { OFF_WHITE } else { GRAY }, 1.0));
    }
}

/// What a click on the tab strip hit: a shoulder prompt (−1/+1) or a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabHit {
    Step(i32),
    Tab(usize),
}

pub fn tab_at(l: &PersonalizeLayout, p: &Personalize, x: f32, y: f32) -> Option<TabHit> {
    let labels: Vec<String> = p.tabs.iter().map(|t| t.label.clone()).collect();
    let strip = tab_strip(l, &labels)?;
    if bo2::hit(&l.frame, strip.prev, x, y) {
        return Some(TabHit::Step(-1));
    }
    if bo2::hit(&l.frame, strip.next, x, y) {
        return Some(TabHit::Step(1));
    }
    strip.tabs.iter().position(|r| bo2::hit(&l.frame, *r, x, y)).map(TabHit::Tab)
}

/// The cell's picture box (`CamoGridButton.SetupButtonImages`): 8 in from the sides and the top,
/// its bottom `TitleHeight + 4 + 7` above the cell's; option 0 (`None`) a centred square that tall.
pub fn cell_image_rect(cell: Rect, first: bool) -> Rect {
    let h = cell.h - 8.0 - (bo2::TEXT_EXTRA_SMALL + 4.0) - 7.0;
    if first {
        Rect { x: cell.x + (cell.w - h) / 2.0, y: cell.y + 8.0, w: h, h }
    } else {
        Rect { x: cell.x + 8.0, y: cell.y + 8.0, w: cell.w - 16.0, h }
    }
}

/// The camo screen's lower panel (`SetupCamoTabContainer`), menu units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CamoPanel {
    /// `camoTitle`: the focused cell's name in Big.
    pub title: Rect,
    /// `weaponImage`: the weapon's `_big` picture, 280 x 140.
    pub weapon: Rect,
    /// The preview box (220 x 130) under its `Equipped` heading.
    pub preview: Rect,
    /// `camoImage` inside the box (20 in, 12 down, 70 tall).
    pub image: Rect,
}

/// The panel's geometry. The info column starts under four grid rows
/// (`-30 + GridTop + 4 * ItemHeight + 3 * Spacing + 3 + HintText.Height + 5`); the weapon picture
/// hangs off the right half's right edge (`-280 + 10 .. 10`, `-70 .. 70`); the box is anchored to
/// the menu's bottom (`-ButtonPrompt.Height - 22`), 80 left of the right half's start.
pub fn camo_panel(l: &PersonalizeLayout) -> CamoPanel {
    let m = l.frame.menu;
    let top = m.y + PERSONALIZE_GRID_TOP - 30.0 + 4.0 * GRID_ITEM_H + 3.0 * GRID_ITEM_SPACING + 3.0 + bo2::TEXT_DEFAULT + 5.0;
    let title = Rect { x: m.x, y: top, w: 1.0 + bo2::MENU_W / 2.0, h: bo2::TEXT_BIG };
    let weapon = Rect { x: m.right() - 270.0, y: top - 70.0, w: 280.0, h: 140.0 };
    let half = m.right() - bo2::MENU_W / 2.0 + 10.0;
    let bottom = m.bottom() - bo2::PROMPT_H - 22.0;
    let preview = Rect { x: half - 80.0, y: bottom - 130.0, w: 220.0, h: 130.0 };
    let image = Rect { x: preview.x + 20.0, y: preview.y + 12.0, w: preview.w - 40.0, h: 70.0 };
    CamoPanel { title, weapon, preview, image }
}

/// The option the `Equipped` box shows: the draft's, or option 0 when the draft has none
/// (BO2's class always holds an index; 0 is `camo_none`).
fn equipped_choice(p: &Personalize) -> &str {
    if !p.equipped.is_empty() {
        return &p.equipped;
    }
    p.tabs.first().and_then(|t| t.choices.first()).map_or("", String::as_str)
}

/// The title: `MPUI_PERSONALIZE_N` with the weapon's name for camo (`caccamomenu.lua:158-215`),
/// the screen's own heading otherwise.
fn screen_title(cat: &Catalog, kind: PersonalizeKind, weapon: Option<&str>) -> String {
    match (kind, weapon.and_then(|w| cat.get(w))) {
        (PersonalizeKind::Camo, Some(d)) => format!("PERSONALIZE {}", d.display_name.to_uppercase()),
        _ => personalize_title(kind).to_string(),
    }
}

/// Draw the open personalize screen: its title and tabs, the choices as a grid of buttons (the
/// equipped one filled orange, the focused one framed), the camo screen's lower panel, and the
/// `ESC Back` / `ENTER Select` footer.
pub fn draw_personalize(prims: &mut Vec<Prim>, w: f32, h: f32, p: &Personalize, weapon: Option<&str>, deny: f32, now: f32) {
    draw_personalize_in(crate::weapons::catalog(), prims, w, h, p, weapon, deny, now);
}

/// [`draw_personalize`] over a given catalog.
#[allow(clippy::too_many_arguments)]
pub fn draw_personalize_in(cat: &Catalog, prims: &mut Vec<Prim>, w: f32, h: f32, p: &Personalize, weapon: Option<&str>, deny: f32, _now: f32) {
    let l = personalize_layout(w, h, p.kind);
    let f = &l.frame;
    bo2::backdrop(prims, f, w, h);
    bo2::title(prims, f, &screen_title(cat, p.kind, weapon));
    draw_tabs(prims, &l, p);
    for (i, choice) in p.choices.iter().enumerate() {
        let r = l.cell_rect(i);
        button_bg(prims, f, r);
        // `UpdateClass`: the equipped cell's `selectedFill` (BOIIOrange at 0.2, 2/1 in).
        if *choice == equipped_choice(p) {
            bo2::fill(prims, f, Rect { x: r.x + 2.0, y: r.y + 1.0, w: r.w - 4.0, h: r.h - 2.0 }, argb(BOII_ORANGE, 0.2));
        }
        if i == p.highlight {
            bo2::border(prims, f, r, 2.0, argb(BOII_ORANGE, 1.0));
        }
        let pic = cell_image_rect(r, i == 0);
        match p.kind {
            PersonalizeKind::Camo => {
                if let Some(image) = camo_image(cat, choice) {
                    bo2::sprite(prims, f, image, pic, argb(OFF_WHITE, 1.0), Vec::new());
                }
            }
            PersonalizeKind::Reticle => {
                if let Some(image) = weapon
                    .and_then(|w| cat.get(w))
                    .and_then(|d| crate::weapons::optic_of(&d.attachments).and_then(|o| crate::weapons::reticle_art(o, choice)))
                {
                    // The reticle art is a converter-named sprite too (`hud.rs`'s reticle grid).
                    bo2::sprite(prims, f, &image, pic, argb(OFF_WHITE, 1.0), Vec::new());
                }
            }
            PersonalizeKind::ReticleColor => {
                if let Some(rgb) = crate::hud::reticle_color_rgb(choice) {
                    let sw = pic.h.max(8.0);
                    bo2::fill(prims, f, Rect { x: pic.x + (pic.w - sw) * 0.5, y: pic.y, w: sw, h: sw }, argb(rgb, 1.0));
                }
            }
            PersonalizeKind::Knife => {}
        }
        bo2::text(
            prims,
            f,
            Family::Condensed,
            &choice_label(cat, p.kind, choice),
            Rect { x: r.x + 3.0, y: r.bottom() - bo2::TEXT_CONDENSED - 2.0, w: r.w, h: bo2::TEXT_CONDENSED },
            Align::Left,
            argb(if i == p.highlight { BOII_ORANGE } else { OFF_WHITE }, 1.0),
        );
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
    if p.kind == PersonalizeKind::Camo {
        draw_camo_panel(cat, prims, &l, p, weapon);
    }
    // The screen is a submenu of the editor: BO2's personalize screens carry `ESC Back` and the
    // select prompt, matching the other content grids (`c-primary/001`, `d-attach/002`).
    let boxes = bo2::prompt_boxes(f, &[("ESC", "Back"), ("ENTER", "Select")], &[]);
    bo2::draw_prompts(prims, f, &boxes);
    deny_flash(prims, w, h, f.s(), deny);
}

/// `CamoButtonHighlighted` + `UpdateCamoPreview` for an unlocked cell: the focused cell's name, the
/// weapon's stock `_big` picture (BO2's personalize screen never re-renders the gun), and the
/// `Equipped` box with the equipped option's thumbnail (`currentlyEquippedCamoImageName`; `None`'s
/// `menu_theater_nodata` is a centred 70-unit square).
fn draw_camo_panel(cat: &Catalog, prims: &mut Vec<Prim>, l: &PersonalizeLayout, p: &Personalize, weapon: Option<&str>) {
    let f = &l.frame;
    let panel = camo_panel(l);
    if let Some(choice) = p.choices.get(p.highlight) {
        bo2::text(prims, f, Family::Big, &choice_label(cat, p.kind, choice), panel.title, Align::Left, argb(OFF_WHITE, 1.0));
    }
    if let Some(d) = weapon.and_then(|w| cat.get(w)) {
        bo2::sprite(prims, f, &crate::names::cac_weapon_image(base_id(&d.id)), panel.weapon, argb(OFF_WHITE, 1.0), Vec::new());
    }
    bo2::text(
        prims,
        f,
        Family::Default,
        "Equipped",
        Rect { x: panel.preview.x, y: panel.preview.y - bo2::TEXT_DEFAULT, w: 300.0, h: bo2::TEXT_DEFAULT },
        Align::Left,
        argb(OFF_WHITE, 1.0),
    );
    bo2::border(prims, f, panel.preview, 1.0, argb(OFF_WHITE, 0.05));
    let equipped = equipped_choice(p);
    if let Some(image) = camo_image(cat, equipped) {
        let first = p.tabs.first().and_then(|t| t.choices.first()).is_some_and(|c| c == equipped);
        let r = if first { Rect { x: panel.image.x + (panel.image.w - 70.0) / 2.0, w: 70.0, ..panel.image } } else { panel.image };
        bo2::sprite(prims, f, image, r, argb(OFF_WHITE, 1.0), Vec::new());
    }
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
/// the cell under it. `A`/`D` (and clicks on a tab or its shoulder prompts) switch tabs. Escape
/// backs out without applying.
pub fn personalize_frame(personalize: &mut Personalize, draft: &mut Draft, inp: &MenuPointer, prev: &MenuPrev) -> PersonalizeFrame {
    let mut edges = Edges::between(&prev.keys, &inp.keys);
    let l = personalize_layout(inp.screen_w, inp.screen_h, personalize.kind);
    let nav = LuiNav::read(prev, inp);
    let mut tabbed = personalize.step_tab(nav.shoulder());
    if prev.clicked(inp) {
        match tab_at(&l, personalize, inp.mouse_x, inp.mouse_y) {
            Some(TabHit::Step(s)) => tabbed |= personalize.step_tab(s),
            Some(TabHit::Tab(i)) => tabbed |= personalize.select_tab(i),
            None => {}
        }
    }
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
    // The wheel is LUI's Up/Down (t6mp `FUN_009376a0`): one grid row per notch, like the arrows.
    let notches = wheel_notches(inp);
    for _ in 0..notches.unsigned_abs() {
        let step = if notches > 0 { Edges { down: true, ..Default::default() } } else { Edges { up: true, ..Default::default() } };
        personalize.step(step, draft);
    }
    let outcome = personalize.step(edges, draft);
    let navigated = notches != 0
        || prev.edge(inp, Key::Up)
        || prev.edge(inp, Key::Down)
        || prev.edge(inp, Key::Left)
        || prev.edge(inp, Key::Right);
    let cue = match outcome {
        Outcome::Save => Some(cues::GRID_EQUIP),
        Outcome::Cancel => Some(cues::BACKOUT),
        Outcome::Continue if navigated || tabbed => Some(cues::GRID_NAV),
        Outcome::Continue => None,
    };
    PersonalizeFrame { outcome, denied: false, cue }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::{CamoOption, Table, WeaponDef};

    fn opt(id: &str, image: &str, tab: OptionTab, sort: u32, mtx: bool) -> CamoOption {
        CamoOption { id: id.into(), name: id.to_uppercase(), image: image.into(), sort, tab, mtx }
    }

    /// A one-gun catalog: `camo_none` first, a base row, a `digital_unlock` row and a store row on
    /// DLC, and two DLC2 rows out of their sort order.
    fn catalog() -> Catalog {
        let camo = vec![
            opt("camo_none", "menu_theater_nodata", OptionTab::Base, 0, false),
            opt("camo_atacs", "menu_camo_atacs_pattern", OptionTab::Base, 0, false),
            opt("camo_dragon", "menu_camo_mtx_dragon", OptionTab::Dlc2, 2, true),
            opt("camo_elite1", "menu_camo_bo2_elite_pattern", OptionTab::Dlc, 0, false),
            opt("camo_mtp", "menu_camo_mtx_mil", OptionTab::Dlc, 0, true),
            opt("camo_ukpunk", "menu_camo_mtx_punk", OptionTab::Dlc2, 1, true),
        ];
        let gun = WeaponDef {
            id: "an94_mp".into(),
            display_name: "AN-94".into(),
            camo: camo.iter().map(|c| c.id.clone()).collect(),
            ..Default::default()
        };
        Catalog::from_table(Table { weapons: vec![gun], camo, ..Default::default() })
    }

    fn images(prims: &[Prim]) -> Vec<String> {
        prims.iter().filter_map(|p| if let Prim::Image { name, .. } = p { Some(name.clone()) } else { None }).collect()
    }

    #[test]
    fn the_camo_tabs_follow_addcamos() {
        let tabs = personalize_tabs_in(&catalog(), PersonalizeKind::Camo, Some("an94_mp"));
        let labels: Vec<&str> = tabs.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["CAMO", "DLC", "DLC2"]);
        assert_eq!(tabs[0].choices, ["camo_none", "camo_atacs"]);
        assert_eq!(tabs[1].choices, ["camo_none", "camo_elite1", "camo_mtp"], "digital unlocks and store rows, table order");
        assert_eq!(tabs[2].choices, ["camo_none", "camo_ukpunk", "camo_dragon"], "DLC2 by its sort key");
    }

    #[test]
    fn dlc2_shows_only_with_a_row_and_dlc_always() {
        let cat_table = Table {
            camo: vec![opt("camo_none", "", OptionTab::Base, 0, false), opt("camo_atacs", "", OptionTab::Base, 0, false)],
            weapons: vec![WeaponDef { id: "an94_mp".into(), camo: vec!["camo_none".into(), "camo_atacs".into()], ..Default::default() }],
            ..Default::default()
        };
        let tabs = personalize_tabs_in(&Catalog::from_table(cat_table), PersonalizeKind::Camo, Some("an94_mp"));
        let labels: Vec<&str> = tabs.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["CAMO", "DLC"], "`ShouldDisplayDlc2Tab` is false; DLC is always added");
        assert_eq!(tabs[1].choices, ["camo_none"]);
    }

    #[test]
    fn the_tiles_draw_the_converted_thumbnails_by_their_own_names() {
        // The converter writes `menu_camo_*_pattern` under that name (`hud.rs`); drawing it through
        // the `ZHBO_Mat_` namespace looked up an image that never existed, so every tile was blank.
        let _f = crate::lui::font::lock();
        crate::lui::font::load("");
        let cat = catalog();
        let draft = Draft { camo: "camo_atacs".into(), ..Default::default() };
        let p = Personalize::with_tabs(PersonalizeKind::Camo, personalize_tabs_in(&cat, PersonalizeKind::Camo, Some("an94_mp")), &draft);
        assert_eq!(p.highlight, 1, "the equipped cell is focused");
        let mut prims = Vec::new();
        draw_personalize_in(&cat, &mut prims, 1280.0, 720.0, &p, Some("an94_mp"), 0.0, 0.0);
        let names = images(&prims);
        for want in ["menu_theater_nodata", "menu_camo_atacs_pattern"] {
            assert!(names.iter().any(|n| n == want), "{want} drawn: {names:?}");
        }
        assert!(!names.iter().any(|n| n.starts_with("ZHBO_Mat_menu_camo")), "no material-namespaced thumbnails: {names:?}");
        let t: Vec<String> = prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None }).collect();
        assert!(t.iter().any(|s| s == "PERSONALIZE AN-94"), "MPUI_PERSONALIZE_N: {t:?}");
        assert!(t.iter().any(|s| s == "Equipped"), "the preview box heading: {t:?}");
        for tab in ["CAMO", "DLC", "DLC2"] {
            assert!(t.iter().any(|s| s == tab), "tab {tab}: {t:?}");
        }
        // The Equipped box shows the equipped option's thumbnail: two atacs images (tile + box).
        assert_eq!(names.iter().filter(|n| *n == "menu_camo_atacs_pattern").count(), 2);
        assert!(names.iter().any(|n| n == &crate::names::cac_weapon_image("an94_mp")), "the weapon's `_big` picture");
    }

    #[test]
    fn the_shoulders_and_tab_clicks_switch_tabs() {
        let cat = catalog();
        let mut draft = Draft { camo: "camo_mtp".into(), ..Default::default() };
        let mut p = Personalize::with_tabs(PersonalizeKind::Camo, personalize_tabs_in(&cat, PersonalizeKind::Camo, Some("an94_mp")), &draft);
        assert_eq!((p.tab, p.highlight), (0, 0), "opens on CAMO; the equipped DLC row is not here");
        let prev = MenuPrev::default();
        let mut inp = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
        inp.keys.set_down(crate::classui::SHOULDER_R, true);
        let frame = personalize_frame(&mut p, &mut draft, &inp, &prev);
        assert_eq!(frame.outcome, Outcome::Continue);
        assert_eq!(p.tab, 1, "`D` is shoulderr");
        assert_eq!(p.choices, ["camo_none", "camo_elite1", "camo_mtp"]);
        assert_eq!(p.highlight, 2, "the equipped cell");
        assert!(p.step_tab(1) && p.tab == 2);
        assert!(p.step_tab(1) && p.tab == 0, "wraps last -> first");
        assert!(p.step_tab(-1) && p.tab == 2, "and first -> last");
        // A click on the CAMO tab.
        let l = personalize_layout(1280.0, 720.0, PersonalizeKind::Camo);
        let labels: Vec<String> = p.tabs.iter().map(|t| t.label.clone()).collect();
        let strip = tab_strip(&l, &labels).unwrap();
        let r = l.frame.px(strip.tabs[0]);
        let mut at_rest = MenuPrev::default();
        let still = MenuPointer { screen_w: 1280.0, screen_h: 720.0, mouse_x: r.x + r.w / 2.0, mouse_y: r.y + r.h / 2.0, ..Default::default() };
        at_rest.update(&still);
        let click = MenuPointer { mouse_buttons: 1, ..still };
        personalize_frame(&mut p, &mut draft, &click, &at_rest);
        assert_eq!(p.tab, 0);
        assert_eq!(draft.camo, "camo_mtp", "switching tabs equips nothing");
    }

    #[test]
    fn the_class_swatch_is_the_thumbnail_corner_or_a_store_32() {
        let cat = catalog();
        assert_eq!(swatch_art(&cat, "an94_mp", "camo_none"), None, "option 0 has no swatch");
        assert_eq!(swatch_art(&cat, "an94_mp", ""), None);
        assert_eq!(swatch_art(&cat, "an94_mp", "camo_atacs"), Some(("menu_camo_atacs_pattern".into(), 140.0, 70.0)));
        assert_eq!(swatch_art(&cat, "an94_mp", "camo_mtp"), Some(("menu_camo_mtx_mil_32".into(), 32.0, 32.0)));
    }

    #[test]
    fn a_weapon_without_a_converted_camo_list_keeps_the_captured_fallback() {
        assert_eq!(personalize_choices_for(PersonalizeKind::Camo, Some("an94_mp")), personalize_choices(PersonalizeKind::Camo));
        assert_eq!(personalize_choices_for(PersonalizeKind::Camo, None), personalize_choices(PersonalizeKind::Camo));
        assert_eq!(personalize_choices_for(PersonalizeKind::Reticle, Some("an94_mp")), personalize_choices(PersonalizeKind::Reticle));
    }
}
