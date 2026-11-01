//! BO2's personalize screens (camo, reticle, reticle colour, knife): the pure grid editor.

use super::*;

/// BO2's personalize screens, in the order the create-a-class screen offers them: camouflage,
/// reticle, reticle colour and the knife prompt (`caceditclass.lua:52-63`'s `knifePersonalization`,
/// its `AddCamoButton`/`AddReticleButton` handlers and `caccamomenu.lua`/`cacreticles.lua`). The
/// camo and knife grids are six wide (`caccamomenu.lua:9`: `CamoGridColumns = 6`); the reticle
/// grids are five (`cacreticles.lua:9`: `GridColumns = 5`). The pure editor only describes the
/// rows and columns; the class UI draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonalizeKind {
    /// The primary's camo page (BO2's `CACCamo` with `weaponSlot = "primary"`).
    Camo,
    Reticle,
    ReticleColor,
    Knife,
    /// The secondary's camo page: the same `CACCamo` with `weaponSlot = "secondary"`
    /// (`customclass.lua`'s `SecondaryWeapon_Button_Over` sets it; the pick lands in
    /// `secondarycamo`, `caccamomenu.lua`'s `SetClassItem(weaponSlot .. "camo")`).
    SecondaryCamo,
}

impl PersonalizeKind {
    /// The grid's column count, BO2's own for each screen.
    pub fn columns(self) -> usize {
        match self {
            PersonalizeKind::Camo | PersonalizeKind::Knife | PersonalizeKind::SecondaryCamo => 6,
            PersonalizeKind::Reticle | PersonalizeKind::ReticleColor => 5,
        }
    }
}

/// The pure personalize editor: one screen's choices and highlighted cell. The host fills the
/// choices (the converted camo list, the reticle list) and the class UI draws the grid; `step`
/// only moves the highlight and applies the chosen value to the draft. The row order is the
/// choices' order, exactly the order the host builds from BO2's camo/reticle lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Personalize {
    pub kind: PersonalizeKind,
    /// The current tab's cells (`tabs[tab].choices`).
    pub choices: Vec<String>,
    pub highlight: usize,
    /// BO2's `TabManager` tabs (`caccamomenu.lua:274-341`: CAMO / DLC / DLC2; `cacreticles.lua`:
    /// RETICLE / DLC / DLC2), each its own content grid. A screen without tabs has one.
    pub tabs: Vec<PersonalizeTab>,
    pub tab: usize,
    /// The draft's value when the screen opened: the cell BO2 focuses first in each tab
    /// (`AddCamos`: the equipped cell, else the first) and the `Equipped` preview's option.
    pub equipped: String,
}

/// One personalize tab: its label (`MPUI_CAMO_CAPS`, `MPUI_MAP_CATEGORY_DLC_CAPS`,
/// `MPUI_DLC2_CAPS`) and its cells in BO2's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalizeTab {
    pub label: String,
    pub choices: Vec<String>,
}

impl Personalize {
    /// Open a one-tab screen on the draft's current choice (the first cell when the draft has none).
    pub fn new(kind: PersonalizeKind, choices: Vec<String>, draft: &Draft) -> Personalize {
        Personalize::with_tabs(kind, vec![PersonalizeTab { label: String::new(), choices }], draft)
    }

    /// Open a tabbed screen on its first tab (BO2's `setCurrentTab()` with no argument), the
    /// draft's current choice focused when that tab holds it.
    pub fn with_tabs(kind: PersonalizeKind, tabs: Vec<PersonalizeTab>, draft: &Draft) -> Personalize {
        let equipped = match kind {
            PersonalizeKind::Camo => &draft.camo,
            PersonalizeKind::Reticle => &draft.reticle,
            PersonalizeKind::ReticleColor => &draft.reticle_color,
            PersonalizeKind::Knife => &draft.knife,
            PersonalizeKind::SecondaryCamo => &draft.secondary_camo,
        }
        .clone();
        let tabs = if tabs.is_empty() { vec![PersonalizeTab { label: String::new(), choices: Vec::new() }] } else { tabs };
        let mut p = Personalize { kind, choices: Vec::new(), highlight: 0, tabs, tab: 0, equipped };
        p.open_tab(0);
        p
    }

    /// Show tab `i`: its cells, the equipped one focused (else the first), as `AddCamos` does
    /// when `SetupCamoTabContainer` rebuilds the grid.
    fn open_tab(&mut self, i: usize) {
        self.tab = i.min(self.tabs.len().saturating_sub(1));
        self.choices = self.tabs.get(self.tab).map(|t| t.choices.clone()).unwrap_or_default();
        self.highlight = self.choices.iter().position(|c| *c == self.equipped).unwrap_or(0);
    }

    /// `shoulderl`/`shoulderr` (`A`/`D` on PC): the previous/next tab, wrapping at either end
    /// (`tabmanagerlite.lua:9-96`, `ButtonLeftShoulder`/`ButtonRightShoulder`). Returns whether the
    /// tab changed.
    pub fn step_tab(&mut self, step: i32) -> bool {
        let n = self.tabs.len() as i32;
        if n < 2 || step == 0 {
            return false;
        }
        let i = (self.tab as i32 + step).rem_euclid(n) as usize;
        self.open_tab(i);
        true
    }

    /// A click on tab `i` (`TabManager.TabClicked`): that tab. Returns whether the tab changed.
    pub fn select_tab(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() || i == self.tab {
            return false;
        }
        self.open_tab(i);
        true
    }

    /// Move the highlight and confirm the choice. `Outcome::Save` means the host closes the
    /// personalize screen and keeps the draft; `Cancel` means it closes without applying.
    pub fn step(&mut self, edges: Edges, draft: &mut Draft) -> Outcome {
        let n = self.choices.len();
        if n == 0 {
            return if edges.cancel || edges.confirm { Outcome::Cancel } else { Outcome::Continue };
        }
        let cols = self.kind.columns();
        if edges.left {
            self.highlight = grid_left(self.highlight, n, cols);
        }
        if edges.right {
            self.highlight = grid_right(self.highlight, n, cols);
        }
        if edges.up {
            self.highlight = grid_up(self.highlight, n, cols);
        }
        if edges.down {
            self.highlight = grid_down(self.highlight, n, cols);
        }
        if edges.confirm {
            if let Some(v) = self.choices.get(self.highlight) {
                match self.kind {
                    PersonalizeKind::Camo => draft.camo = v.clone(),
                    PersonalizeKind::Reticle => draft.reticle = v.clone(),
                    PersonalizeKind::ReticleColor => draft.reticle_color = v.clone(),
                    PersonalizeKind::Knife => draft.knife = v.clone(),
                    PersonalizeKind::SecondaryCamo => draft.secondary_camo = v.clone(),
                }
            }
            return Outcome::Save;
        }
        if edges.cancel {
            return Outcome::Cancel;
        }
        Outcome::Continue
    }
}
