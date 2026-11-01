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
    Camo,
    Reticle,
    ReticleColor,
    Knife,
}

impl PersonalizeKind {
    /// The grid's column count, BO2's own for each screen.
    pub fn columns(self) -> usize {
        match self {
            PersonalizeKind::Camo | PersonalizeKind::Knife => 6,
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
    pub choices: Vec<String>,
    pub highlight: usize,
}

impl Personalize {
    /// Open the screen on the draft's current choice (the first cell when the draft has none).
    pub fn new(kind: PersonalizeKind, choices: Vec<String>, draft: &Draft) -> Personalize {
        let current = match kind {
            PersonalizeKind::Camo => &draft.camo,
            PersonalizeKind::Reticle => &draft.reticle,
            PersonalizeKind::ReticleColor => &draft.reticle_color,
            PersonalizeKind::Knife => &draft.knife,
        };
        let highlight = choices.iter().position(|c| c == current).unwrap_or(0);
        Personalize { kind, choices, highlight }
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
