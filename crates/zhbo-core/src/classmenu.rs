//! BO2's Create-a-Class editor: the Pick-10 slot editor behind the "CREATE A CLASS" screen.
//!
//! The pick screen (`classui::draw_pick`) is the *quick* choice between shipped presets and the
//! player's saved classes; this module is the full editor BO2 opens from the shell screen's
//! CREATE A CLASS row (a match is pick-only). It is a keyboard- and pointer-driven state machine
//! over the Pick-10 model:
//!
//! * **10 points** per class. Every chosen element — a primary or secondary weapon, each
//!   attachment, the lethal, the tactical, each perk, each wildcard — costs 1. The counter shows
//!   the points left; the editor refuses a choice the budget cannot afford.
//! * **Slots**, in BO2's order: Primary (with its attachments), Secondary (with its), Lethal,
//!   Tactical, the three perk tiers, and the wildcards. The weapon slots list the catalog's guns
//!   of the matching category; the perk slots list the three tiers from `weapons::perks`; the
//!   attachment slots list what the chosen weapon offers (`WeaponDef::allowed`).
//! * **Wildcards** (BO2's eight) unlock a second perk of a tier, a third primary attachment, a
//!   second secondary attachment, a second primary (Overkill), a tactical in the lethal slot
//!   (Tactician), or a second lethal (Danger Close). Each costs a point, and the extra element it
//!   enables costs another.
//!
//! Everything here is pure: the module owns the editing session's state, `step` folds one frame of
//! key edges into it, and `draw` renders it. The caller persists the result through the profile
//! (`Profile`) and applies it to the sim exactly like a shipped-class pick.
//!
//! A class the player finishes is a [`CustomClass`] (`classes.rs`) — the same type the shipped
//! registry's entries resolve to and the same wire format the pick message carries — so a class
//! built here needs no new transport: the sim receives its spec and applies it as-is.

use crate::classes::{self, CustomClass};

/// Which attachments go together: BO2's compatibility lists (its own file: this one is at the
/// module-size limit).
#[path = "classmenu/legal.rs"]
pub mod legal;
/// Which weapons the editor offers in which slot (BO2's statstable items).
#[path = "classmenu/items.rs"]
pub mod items;
use items::{equipment_ids, gun_ids};
use crate::input::Key;
use crate::weapons::{self, Class as WeaponClass, Perk};

/// The Pick-10 budget: ten points per class, exactly BO2's.
pub const PICK_10: i32 = 10;

/// The longest class name the editor's name field accepts (BO2's field is a short one line).
pub const NAME_MAX: usize = 24;

/// The eight BO2 wildcards. Each costs a point and unlocks one extra element (another point).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wildcard {
    /// A second Tier 1 (Perk 1) perk.
    Perk1Greed,
    /// A second Tier 2 (Perk 2) perk.
    Perk2Greed,
    /// A second Tier 3 (Perk 3) perk.
    Perk3Greed,
    /// A second Primary Weapon in the Secondary slot.
    Overkill,
    /// A third attachment on the Primary Weapon.
    PrimaryGunfighter,
    /// A second attachment on the Secondary Weapon.
    SecondaryGunfighter,
    /// A Tactical in the Lethal slot.
    Tactician,
    /// A second Lethal.
    DangerClose,
}

impl Wildcard {
    /// All eight, in BO2's display order (`f-wildcards/001`: the three Greed cards, Overkill,
    /// Secondary Gunfighter, Primary Gunfighter, Tactician, Danger Close). This order is the
    /// bonus-card picker's presentation order; the profile stores the ids, not the index.
    pub const ALL: [Wildcard; 8] = [
        Wildcard::Perk1Greed,
        Wildcard::Perk2Greed,
        Wildcard::Perk3Greed,
        Wildcard::Overkill,
        Wildcard::SecondaryGunfighter,
        Wildcard::PrimaryGunfighter,
        Wildcard::Tactician,
        Wildcard::DangerClose,
    ];

    /// BO2's in-game name.
    pub fn name(self) -> &'static str {
        match self {
            Wildcard::Perk1Greed => "Perk 1 Greed",
            Wildcard::Perk2Greed => "Perk 2 Greed",
            Wildcard::Perk3Greed => "Perk 3 Greed",
            Wildcard::Overkill => "Overkill",
            Wildcard::PrimaryGunfighter => "Primary Gunfighter",
            Wildcard::SecondaryGunfighter => "Secondary Gunfighter",
            Wildcard::Tactician => "Tactician",
            Wildcard::DangerClose => "Danger Close",
        }
    }

    /// The spec id the profile file stores (`perk1greed`).
    pub fn id(self) -> &'static str {
        match self {
            Wildcard::Perk1Greed => "perk1greed",
            Wildcard::Perk2Greed => "perk2greed",
            Wildcard::Perk3Greed => "perk3greed",
            Wildcard::Overkill => "overkill",
            Wildcard::PrimaryGunfighter => "primarygunfighter",
            Wildcard::SecondaryGunfighter => "secondarygunfighter",
            Wildcard::Tactician => "tactician",
            Wildcard::DangerClose => "dangerclose",
        }
    }

    /// The wildcard with this id (the profile's spelling).
    pub fn by_id(id: &str) -> Option<Wildcard> {
        Wildcard::ALL.into_iter().find(|w| w.id().eq_ignore_ascii_case(id))
    }

    /// The perk tier this greed wildcard opens a second slot for, if it is one.
    pub fn greed_tier(self) -> Option<u8> {
        match self {
            Wildcard::Perk1Greed => Some(1),
            Wildcard::Perk2Greed => Some(2),
            Wildcard::Perk3Greed => Some(3),
            _ => None,
        }
    }
}

/// The primary weapon slot's categories, as BO2's create-a-class lists them.
pub const PRIMARY_CLASSES: [WeaponClass; 7] = [
    WeaponClass::Rifle,
    WeaponClass::Smg,
    WeaponClass::Shotgun,
    WeaponClass::Lmg,
    WeaponClass::Sniper,
    WeaponClass::Special,
    WeaponClass::Launcher,
];

/// The secondary weapon slot's categories.
pub const SECONDARY_CLASSES: [WeaponClass; 2] = [WeaponClass::Pistol, WeaponClass::Launcher];

/// One editable slot of the class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// BO2's "Class Name" row at the top: Enter starts text entry on it.
    Name,
    Primary,
    PrimaryAtts,
    Secondary,
    SecondaryAtts,
    Lethal,
    Tactical,
    Perk1,
    Perk2,
    Perk3,
    /// The second row of a perk tier, shown only when the tier's Greed wildcard is on. BO2's create-
    /// a-class adds a duplicate "PERK n" row for the extra pick; the row is dead without the
    /// wildcard, so it is not in [`Slot::ALL`] and only the editor's dynamic list shows it.
    Perk1b,
    Perk2b,
    Perk3b,
    Wildcards,
    /// BO2's final row: confirming it saves the class and leaves the editor. It has no option list.
    Save,
}

impl Slot {
    /// Every slot, in BO2's create-a-class order. `Save` is the row below the elements, so it is
    /// last in the walk and the arrows wrap from it back to the name. The second perk rows
    /// ([`Slot::Perk1b`] and its siblings) are *not* here: they only exist while their Greed wildcard
    /// is on, so the editor's [`Editor::slots`] inserts them into a copy of this list.
    pub const ALL: [Slot; 12] = [
        Slot::Name,
        Slot::Primary,
        Slot::PrimaryAtts,
        Slot::Secondary,
        Slot::SecondaryAtts,
        Slot::Lethal,
        Slot::Tactical,
        Slot::Perk1,
        Slot::Perk2,
        Slot::Perk3,
        Slot::Wildcards,
        Slot::Save,
    ];

    /// BO2's header for the slot. The second perk rows read as their tier's header.
    pub fn label(self) -> &'static str {
        match self {
            Slot::Name => "CLASS NAME",
            Slot::Primary => "PRIMARY",
            Slot::PrimaryAtts => "ATTACHMENTS",
            Slot::Secondary => "SECONDARY",
            Slot::SecondaryAtts => "ATTACHMENTS",
            Slot::Lethal => "LETHAL",
            Slot::Tactical => "TACTICAL",
            Slot::Perk1 | Slot::Perk1b => "PERK 1",
            Slot::Perk2 | Slot::Perk2b => "PERK 2",
            Slot::Perk3 | Slot::Perk3b => "PERK 3",
            // BO2 calls the items `bonuscard_*` internally but the screen is `MENU_WILDCARDS_CAPS`
            // (`menus/cacbonuscards.lua`), so the picker titles itself WILDCARDS.
            Slot::Wildcards => "WILDCARDS",
            Slot::Save => "SAVE CLASS",
        }
    }

    /// The perk tier this slot picks, if it is a perk slot (either row of a tier).
    pub fn perk_tier(self) -> Option<u8> {
        match self {
            Slot::Perk1 | Slot::Perk1b => Some(1),
            Slot::Perk2 | Slot::Perk2b => Some(2),
            Slot::Perk3 | Slot::Perk3b => Some(3),
            _ => None,
        }
    }

    /// Whether this is the *second* row of a perk tier (the Greed one).
    pub fn is_extra_perk(self) -> bool {
        matches!(self, Slot::Perk1b | Slot::Perk2b | Slot::Perk3b)
    }

    /// How many cards a row holds when this slot's picker is one of BO2's content grids
    /// (`cacattachmentsmenu.lua:9`: `GridColumns` 6; `cacperksp.lua:7` and
    /// `cacbonuscards.lua:7`: 3). `None` for the weapon carousel (`cacweaponsp.lua`, walked by
    /// category, not by grid links) and the rows without an option list.
    pub fn option_columns(self) -> Option<usize> {
        match self {
            Slot::PrimaryAtts | Slot::SecondaryAtts => Some(6),
            Slot::Lethal
            | Slot::Tactical
            | Slot::Perk1
            | Slot::Perk2
            | Slot::Perk3
            | Slot::Perk1b
            | Slot::Perk2b
            | Slot::Perk3b
            | Slot::Wildcards => Some(3),
            Slot::Name | Slot::Primary | Slot::Secondary | Slot::Save => None,
        }
    }
}

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

// ------------------------------------------------------------------------------------------------
// BO2's content-grid links (`contentgrid.lua`)
// ------------------------------------------------------------------------------------------------

/// BO2's `CoD.ContentGrid.AddButton` wires each new cell as it is added (`contentgrid.lua:91-122`,
/// through `SetNavigation` at `:137-144`):
///
/// * the cell left of a new one gets `right` to it and the new one `left` back (same row);
/// * the new cell's `right` goes to the *first* cell of its row, and that cell's `left` back to
///   the new one — so Left/Right **wrap inside the row**; they never page, and a one-cell row keeps
///   neither link (BO2's `SetNavigation` skips a target that is the button itself);
/// * `up`/`down` link the same column across rows, and the first/last row that holds the column
///   link back to each other, so vertical moves **wrap within the column** (a partially filled last
///   row simply has no cells in the columns it does not reach).
///
/// The helpers below are those links, computed on the option list's flat index. `i` is clamped so a
/// stale highlight (the option list shrank under it) cannot index past the list.
pub fn grid_left(i: usize, n: usize, cols: usize) -> usize {
    if n == 0 || cols == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    let first = row_start(i, cols);
    if i > first {
        i - 1
    } else {
        first + row_len(first, n, cols) - 1
    }
}

/// The cell right of `i`: the next in the row, or the row's first (`contentgrid.lua:100-107`).
pub fn grid_right(i: usize, n: usize, cols: usize) -> usize {
    if n == 0 || cols == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    let first = row_start(i, cols);
    if i + 1 < first + row_len(first, n, cols) {
        i + 1
    } else {
        first
    }
}

/// The cell above `i` in its column, wrapping to the last row that has the column
/// (`contentgrid.lua:108-115` plus the back-links the later rows add).
pub fn grid_up(i: usize, n: usize, cols: usize) -> usize {
    grid_vertical(i, n, cols, -1)
}

/// The cell below `i` in its column, wrapping to the first row
/// (`contentgrid.lua:116-122` plus the back-links).
pub fn grid_down(i: usize, n: usize, cols: usize) -> usize {
    grid_vertical(i, n, cols, 1)
}

/// The first index of `i`'s row.
fn row_start(i: usize, cols: usize) -> usize {
    i - i % cols
}

/// How many cells of the row starting at `first` exist (the last row may be short).
fn row_len(first: usize, n: usize, cols: usize) -> usize {
    (n - first).min(cols)
}

/// Walk the cells of `i`'s column by `step`, wrapping (`contentgrid.lua:108-122`): each row's
/// `up`/`down` is overwritten by the row that neighbours it, and the first row's `up` ends on the
/// last row holding the column (and vice versa).
fn grid_vertical(i: usize, n: usize, cols: usize, step: i32) -> usize {
    if n == 0 || cols == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    let col = i % cols;
    // The rows that hold this column: every full row, plus the last one only if it reaches it.
    let rows = (n - col).div_ceil(cols);
    let row = i / cols;
    let next = (row as i32 + step).rem_euclid(rows as i32) as usize;
    col + next * cols
}

/// The player's class being edited: the Pick-10 choices, resolved into a [`CustomClass`] on save.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    /// What the class is called (`Custom Class 1`).
    pub name: String,
    pub primary: String,
    /// The primary's attachments, by short name (`reflex`), at most 2 without Primary Gunfighter.
    pub primary_atts: Vec<String>,
    pub secondary: String,
    pub secondary_atts: Vec<String>,
    pub lethal: String,
    pub tactical: String,
    /// The three perk tiers' first picks.
    pub perks: [Option<Perk>; 3],
    /// A tier's *second* pick, the one its Greed wildcard opens (BO2's "Perk n Greed"). Dead unless
    /// the matching wildcard is in play; dropped when the wildcard is turned off.
    pub extra_perks: [Option<Perk>; 3],
    /// The wildcards in play.
    pub wildcards: Vec<Wildcard>,
    /// Spare rounds for the primary and secondary; 0 = BO2's default.
    pub reserve: [u16; 2],
    /// The primary's camo id (`none` = the weapon's stock finish), BO2's Personalize → Camouflage.
    /// Empty means "not personalised" and serialises as no key, so older specs keep loading.
    pub camo: String,
    /// The primary optic's reticle id (`none` = the optic's stock reticle), BO2's Personalize →
    /// Reticle. Empty means "not personalised" and serialises as no key.
    pub reticle: String,
    /// The reticle's colour id (`none` = the reticle's stock colour), BO2's Personalize → Reticle
    /// colour. Empty means "not personalised" and serialises as no key.
    pub reticle_color: String,
    /// The melee viewmodel id (`knife_held_mp` = the standard combat knife), BO2's Personalize →
    /// Knife. Empty means "not personalised" and serialises as no key.
    pub knife: String,
}

impl Default for Draft {
    /// BO2's default class: the AN-94 with a Reflex sight and a Grenade Launcher, the Tac-45, a
    /// frag, a concussion, and Lightweight/Fast Hands/Dexterity — the base kit (`Loadout::default`).
    fn default() -> Self {
        Draft {
            name: String::new(),
            primary: "an94_mp".into(),
            primary_atts: vec!["gl".into(), "reflex".into()],
            secondary: "fnp45_mp".into(),
            secondary_atts: Vec::new(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            perks: [Some(Perk::LIGHTWEIGHT), Some(Perk::FAST_HANDS), Some(Perk::DEXTERITY)],
            extra_perks: [None, None, None],
            wildcards: Vec::new(),
            reserve: [0, 0],
            camo: String::new(),
            reticle: String::new(),
            reticle_color: String::new(),
            knife: String::new(),
        }
    }
}

impl Draft {
    /// The weapon a class with these choices gets (`an94_mp+gl+reflex`; a pistol with `dw` is its
    /// `_dw_mp` weapon), BO2's `GetLoadoutWeapon` ([`items::given_weapon`]).
    fn weapon_with_atts(base: &str, atts: &[String]) -> String {
        let shorts: Vec<&str> = atts.iter().map(|s| s.as_str()).collect();
        items::given_weapon(base, &shorts)
    }

    /// The primary weapon id plus its attachments.
    pub fn primary_id(&self) -> String {
        Draft::weapon_with_atts(&self.primary, &self.primary_atts)
    }

    /// The secondary weapon id plus its attachments.
    pub fn secondary_id(&self) -> String {
        Draft::weapon_with_atts(&self.secondary, &self.secondary_atts)
    }

    /// How many Pick-10 points the draft has spent. Every element is one point: the two weapons,
    /// every attachment, the lethal, the tactical, each perk, each wildcard.
    pub fn spent(&self) -> i32 {
        let mut n = 0;
        if !self.primary.is_empty() {
            n += 1;
        }
        if !self.secondary.is_empty() {
            n += 1;
        }
        n += self.primary_atts.len() as i32;
        n += self.secondary_atts.len() as i32;
        if !self.lethal.is_empty() {
            n += 1;
        }
        if !self.tactical.is_empty() {
            n += 1;
        }
        n += self.perks.iter().filter(|p| p.is_some()).count() as i32;
        n += self.extra_perks.iter().filter(|p| p.is_some()).count() as i32;
        n += self.wildcards.len() as i32;
        n
    }

    /// The points left of the ten.
    pub fn remaining(&self) -> i32 {
        PICK_10 - self.spent()
    }

    /// Whether the draft's choices fit the budget (never over ten) and the three-card cap.
    pub fn valid(&self) -> bool {
        self.remaining() >= 0 && self.wildcards.len() <= classes::MAX_BONUS_CARDS
    }

    /// Whether every bonus-card slot is taken (`maxBonusCards`), so another card is denied.
    pub fn wildcards_full(&self) -> bool {
        self.wildcards.len() >= classes::MAX_BONUS_CARDS
    }

    /// The most attachments the primary may carry: BO2's two (`maxAttachments`), or three with
    /// Primary Gunfighter.
    pub fn primary_att_max(&self) -> usize {
        if self.wildcards.contains(&Wildcard::PrimaryGunfighter) {
            classes::MAX_ATTACHMENTS + 1
        } else {
            classes::MAX_ATTACHMENTS
        }
    }

    /// The most attachments the secondary may carry: one (BO2's secondary slot takes one fewer),
    /// or two with Secondary Gunfighter.
    pub fn secondary_att_max(&self) -> usize {
        if self.wildcards.contains(&Wildcard::SecondaryGunfighter) {
            classes::MAX_ATTACHMENTS
        } else {
            classes::MAX_ATTACHMENTS - 1
        }
    }

    /// The perks in pick order for the spec/wire: each tier's first pick, then its Greed pick, in
    /// tier order. Empty tiers are skipped. Duplicates are impossible (the editor refuses them).
    pub fn perks_in_order(&self) -> Vec<Perk> {
        let mut out: Vec<Perk> = Vec::new();
        for t in 0..3 {
            if let Some(p) = self.perks[t] {
                out.push(p);
            }
            if let Some(p) = self.extra_perks[t] {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// The kit spec (`Loadout::with_spec` grammar), exactly like a shipped class's. `primary` and
    /// `secondary` carry their attachments; the perks join in tier order.
    pub fn to_spec(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut push = |key: &str, value: &str| {
            if !value.is_empty() {
                parts.push(format!("{key}={value}"));
            }
        };
        push("primary", &self.primary_id());
        push("secondary", &self.secondary_id());
        push("lethal", &self.lethal);
        push("tactical", &self.tactical);
        push("melee", "knife_held_mp");
        let perks: Vec<&str> = self.perks_in_order().iter().map(|p| p.def().id).collect();
        push("perks", &perks.join(","));
        push("camo", &self.camo);
        push("reticle", &self.reticle);
        push("reticle_color", &self.reticle_color);
        push("knife", &self.knife);
        if self.reserve[0] > 0 {
            push("primary_reserve", &self.reserve[0].to_string());
        }
        if self.reserve[1] > 0 {
            push("secondary_reserve", &self.reserve[1].to_string());
        }
        parts.join(";")
    }

    /// The finished class as a [`CustomClass`] (the profile entry and the pick message's payload).
    pub fn to_custom(&self) -> CustomClass {
        CustomClass {
            name: self.name.clone(),
            primary: self.primary_id(),
            secondary: self.secondary_id(),
            lethal: self.lethal.clone(),
            tactical: self.tactical.clone(),
            melee: "knife_held_mp".into(),
            perks: self.perks_in_order().iter().map(|p| p.def().id.to_string()).collect(),
            reserve: self.reserve,
            camo: self.camo.clone(),
            reticle: self.reticle.clone(),
            reticle_color: self.reticle_color.clone(),
            knife: self.knife.clone(),
            ..CustomClass::default()
        }
    }

    /// Build a draft from a *stored* class, repaired first: profile entries are loaded repaired
    /// ([`crate::profile`]) and a class can also arrive from an older build or a hand edit, so
    /// re-opening one in the editor shows exactly the class the game would apply — never a draft
    /// holding an unknown weapon or more than ten points (`classrepair`, BO2's `checkclasses.lua`).
    pub fn from_stored(c: &CustomClass) -> Draft {
        let mut c = c.clone();
        let _ = crate::classrepair::repair(&mut c);
        Draft::from_custom(&c)
    }

    /// Build a draft from a saved class (re-opening a profile entry in the editor).
    pub fn from_custom(c: &CustomClass) -> Draft {
        let (primary, primary_atts) = split_atts(&c.primary);
        let (secondary, secondary_atts) = split_atts(&c.secondary);
        let (perks, extra_perks) = perks_of(&c.perks);
        Draft {
            name: c.name.clone(),
            primary,
            primary_atts,
            secondary,
            secondary_atts,
            lethal: c.lethal.clone(),
            tactical: c.tactical.clone(),
            perks,
            extra_perks,
            wildcards: wildcards_of(c),
            reserve: c.reserve,
            camo: c.camo.clone(),
            reticle: c.reticle.clone(),
            reticle_color: c.reticle_color.clone(),
            knife: c.knife.clone(),
        }
    }
}

/// Split a given weapon id into its item and attachment shorts (`an94_mp+gl+reflex`;
/// `fnp45_dw_mp` is the Tac-45 with `dw`), [`items::given_parts`].
fn split_atts(id: &str) -> (String, Vec<String>) {
    items::given_parts(id)
}

/// The perks of a saved class into the three tiers' first and second picks: the first perk in a
/// tier is its primary row, the second its Greed row. A second in a tier needs the matching Greed
/// wildcard, which [`wildcards_of`] restores.
fn perks_of(ids: &[String]) -> ([Option<Perk>; 3], [Option<Perk>; 3]) {
    let mut first: [Option<Perk>; 3] = [None, None, None];
    let mut second: [Option<Perk>; 3] = [None, None, None];
    for id in ids {
        let Some(p) = Perk::by_id(id) else { continue };
        let tier = (p.def().tier as usize).clamp(1, 3) - 1;
        if first[tier].is_none() {
            first[tier] = Some(p);
        } else if second[tier].is_none() {
            second[tier] = Some(p);
        }
    }
    (first, second)
}

/// The wildcards a saved class implies: it can only have the ones its choices need. A saved class
/// carries no explicit wildcard list (the spec grammar has no slot for one), so the wildcards are
/// re-derived: a greed wildcard when a tier has a second perk, the gunfighters when the attachment
/// counts exceed the base maxima, Overkill when the secondary is a primary-class gun, Tactician or
/// Danger Close from the slot contents.
fn wildcards_of(c: &CustomClass) -> Vec<Wildcard> {
    let mut out: Vec<Wildcard> = Vec::new();
    // Second perk in a tier: count each tier's perks.
    let mut tiers = [0u8; 3];
    for id in &c.perks {
        if let Some(p) = Perk::by_id(id) {
            tiers[(p.def().tier as usize).clamp(1, 3) - 1] += 1;
        }
    }
    if tiers[0] > 1 {
        out.push(Wildcard::Perk1Greed);
    }
    if tiers[1] > 1 {
        out.push(Wildcard::Perk2Greed);
    }
    if tiers[2] > 1 {
        out.push(Wildcard::Perk3Greed);
    }
    // Attachment counts.
    let (_, pri_atts) = split_atts(&c.primary);
    let (_, sec_atts) = split_atts(&c.secondary);
    if pri_atts.len() > 2 {
        out.push(Wildcard::PrimaryGunfighter);
    }
    if sec_atts.len() > 1 {
        out.push(Wildcard::SecondaryGunfighter);
    }
    // The secondary holding a primary item is Overkill.
    if items::is_primary_item(&split_atts(&c.secondary).0) {
        out.push(Wildcard::Overkill);
    }
    out
}

/// The live editing session. `None` when the editor is closed.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    /// The class being edited.
    pub draft: Draft,
    /// The highlighted slot.
    pub slot: Slot,
    /// The highlighted option within the slot's list.
    pub option: usize,
    /// Whether the slot's option list is open (picking) or the slot row is just highlighted.
    pub picking: bool,
    /// Whether the class-name row has text entry (BO2's name field): printable keys edit the name
    /// instead of moving the highlight, and Enter/Escape leave it.
    pub naming: bool,
    /// One-shot: the last confirm was refused (over budget, or an attachment at its cap). The
    /// caller plays BO2's deny cue and clears this; the state itself stays pure.
    pub denied: bool,
    /// One-shot: the player pressed Tab on the class row (`caceditclass.lua:52-63`'s
    /// `knifePersonalization` prompt) or `C`/`V` on the primary weapon/optic row. The caller opens
    /// the personalize screen and clears this; the pure editor only records the action.
    pub personalize: Option<PersonalizeKind>,
}

/// The editor's answer to one pointer frame, already hit-tested by the view (the editor knows
/// nothing about pixels). Hovering moves the highlight; a click confirms the hovered row, exactly
/// as Enter does on it; the wheel walks the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerEvent {
    /// Hovered slot row (an index into [`Editor::slots`]).
    pub slot: Option<usize>,
    /// Hovered option row (an index into [`Editor::options`]) while a list is open.
    pub option: Option<usize>,
    /// The left button went down this frame.
    pub click: bool,
    /// Wheel notches this frame (positive scrolls down the list).
    pub wheel: i32,
    /// The cursor moved this frame. Only then does a hover move the highlight (a resting cursor
    /// must not fight the keyboard); a click always acts on the row under it.
    pub moved: bool,
}

impl PointerEvent {
    /// Whether this frame carries a pointer action at all (click or wheel).
    pub fn is_action(&self) -> bool {
        self.click || self.wheel != 0
    }
}

/// What [`Editor::apply`] did with an option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyResult {
    /// The draft changed.
    Changed,
    /// The option was already the draft's value (picking it again).
    NoChange,
    /// The choice would break the budget or a cap: the draft is untouched.
    Denied,
}

impl Editor {
    pub fn new(draft: Draft) -> Editor {
        Editor { draft, slot: Slot::Primary, option: 0, picking: false, naming: false, denied: false, personalize: None }
    }

    /// The rows to draw and walk, in order: [`Slot::ALL`] with a second perk row spliced in after a
    /// tier whose Greed wildcard is on (BO2's duplicate "PERK n" row).
    pub fn slots(&self) -> Vec<Slot> {
        let mut out: Vec<Slot> = Vec::with_capacity(Slot::ALL.len() + 3);
        for s in Slot::ALL {
            out.push(s);
            if let Some(tier) = s.perk_tier() {
                if !s.is_extra_perk() && self.draft.wildcards.contains(&greed_wildcard(tier)) {
                    out.push(extra_perk_slot(tier));
                }
            }
        }
        out
    }

    /// The highlighted slot's index in [`Editor::slots`], for the view's scrolling.
    pub fn slot_index(&self) -> usize {
        self.slots().iter().position(|s| *s == self.slot).unwrap_or(0)
    }

    /// The option list for the highlighted slot: display strings, in the order the arrows walk.
    /// The lists are rebuilt on demand so a weapon change re-derives the attachment list.
    pub fn options(&self) -> Vec<String> {
        match self.slot {
            Slot::Name => Vec::new(),
            Slot::Primary => weapon_options(&items::slots_of(true, &self.draft.wildcards)),
            Slot::PrimaryAtts => attachment_options(&self.draft.primary, &self.draft.primary_atts),
            Slot::Secondary => weapon_options(&self.secondary_slots()),
            Slot::SecondaryAtts => attachment_options(&self.draft.secondary, &self.draft.secondary_atts),
            Slot::Lethal => equipment_options(WeaponClass::Lethal),
            Slot::Tactical => equipment_options(WeaponClass::Tactical),
            Slot::Perk1 | Slot::Perk1b => perk_options(1),
            Slot::Perk2 | Slot::Perk2b => perk_options(2),
            Slot::Perk3 | Slot::Perk3b => perk_options(3),
            Slot::Wildcards => Wildcard::ALL.iter().map(|w| w.name().to_string()).collect(),
            Slot::Save => Vec::new(),
        }
    }

    /// The item slots the secondary lists: its own, plus the primary items when Overkill puts a
    /// second primary there.
    fn secondary_slots(&self) -> Vec<&'static str> {
        items::slots_of(false, &self.draft.wildcards)
    }

    /// Whether option `i` of the highlighted slot is already in the draft (so the list can mark it).
    /// A weapon/equipment/perk/wildcard option is "chosen" when it is the current value; an
    /// attachment option is chosen when the weapon carries its short.
    pub fn option_chosen(&self, i: usize) -> bool {
        match self.slot {
            Slot::Name => false,
            Slot::Primary => gun_ids(&items::slots_of(true, &self.draft.wildcards)).get(i).is_some_and(|id| *id == self.draft.primary),
            Slot::Secondary => gun_ids(&self.secondary_slots()).get(i).is_some_and(|id| *id == self.draft.secondary),
            Slot::PrimaryAtts => {
                attachment_shorts(&self.draft.primary, &self.draft.primary_atts).get(i).is_some_and(|s| self.draft.primary_atts.iter().any(|a| a == s))
            }
            Slot::SecondaryAtts => {
                attachment_shorts(&self.draft.secondary, &self.draft.secondary_atts).get(i).is_some_and(|s| self.draft.secondary_atts.iter().any(|a| a == s))
            }
            Slot::Lethal => equipment_ids(WeaponClass::Lethal).get(i).is_some_and(|id| *id == self.draft.lethal),
            Slot::Tactical => equipment_ids(WeaponClass::Tactical).get(i).is_some_and(|id| *id == self.draft.tactical),
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 | Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
                // The tier's list marks a perk when *either* row of the tier holds it.
                let tier = self.slot.perk_tier().unwrap_or(1);
                let list = perk_list(tier);
                let idx = (tier as usize).clamp(1, 3) - 1;
                list.get(i).is_some_and(|p| self.draft.perks[idx] == Some(*p) || self.draft.extra_perks[idx] == Some(*p))
            }
            Slot::Wildcards => Wildcard::ALL.get(i).is_some_and(|w| self.draft.wildcards.contains(w)),
            Slot::Save => false,
        }
    }

    /// The weapon option `i` of the highlighted slot selects, for the view's icon and stat bars.
    /// `None` when the slot does not list guns (perks, attachments, wildcards).
    pub fn option_weapon(&self, i: usize) -> Option<&'static crate::weapons::WeaponDef> {
        let id = match self.slot {
            Slot::Primary => gun_ids(&items::slots_of(true, &self.draft.wildcards)).get(i).copied(),
            Slot::Secondary => gun_ids(&self.secondary_slots()).get(i).copied(),
            Slot::Lethal => equipment_ids(WeaponClass::Lethal).get(i).copied(),
            Slot::Tactical => equipment_ids(WeaponClass::Tactical).get(i).copied(),
            _ => None,
        }?;
        Some(weapons::weapon(id))
    }

    /// The attachment short option `i` of an attachment slot, for its icon.
    pub fn option_attachment(&self, i: usize) -> Option<&'static str> {
        match self.slot {
            Slot::PrimaryAtts => attachment_shorts(&self.draft.primary, &self.draft.primary_atts).get(i).copied(),
            Slot::SecondaryAtts => attachment_shorts(&self.draft.secondary, &self.draft.secondary_atts).get(i).copied(),
            _ => None,
        }
    }

    /// Whether the highlighted primary attachment is one BO2 personalizes the reticle of.
    fn option_is_optic(&self) -> bool {
        self.option_attachment(self.option).is_some_and(|s| crate::hud::OPTICS.contains(&s))
    }

    /// The perk option `i` of a perk row, for its icon.
    pub fn option_perk(&self, i: usize) -> Option<Perk> {
        self.slot.perk_tier().and_then(|tier| perk_list(tier).get(i).copied())
    }

    /// The wildcard option `i` of the wildcard slot, for its icon.
    pub fn option_wildcard(&self, i: usize) -> Option<Wildcard> {
        (self.slot == Slot::Wildcards).then(|| Wildcard::ALL.get(i).copied()).flatten()
    }
}

/// The Greed wildcard that opens a tier's second perk row.
fn greed_wildcard(tier: u8) -> Wildcard {
    match tier {
        2 => Wildcard::Perk2Greed,
        3 => Wildcard::Perk3Greed,
        _ => Wildcard::Perk1Greed,
    }
}

/// The second perk row of a tier (the Greed one).
fn extra_perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2b,
        3 => Slot::Perk3b,
        _ => Slot::Perk1b,
    }
}

/// The current value of a slot as the editor's row shows it: the draft's own resolution, so the
/// screen reads exactly like the resulting class. Display names, not ids; `—` when a slot is empty.
pub fn slot_value(d: &Draft, slot: Slot) -> String {
    match slot {
        Slot::Name => if d.name.is_empty() { "—".into() } else { d.name.clone() },
        Slot::Primary => weapons::weapon(&d.primary).display_name.clone(),
        Slot::Secondary => weapons::weapon(&d.secondary).display_name.clone(),
        Slot::PrimaryAtts => att_names(&d.primary_atts),
        Slot::SecondaryAtts => att_names(&d.secondary_atts),
        Slot::Lethal => weapons::weapon(&d.lethal).display_name.clone(),
        Slot::Tactical => weapons::weapon(&d.tactical).display_name.clone(),
        Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
            let tier = slot.perk_tier().unwrap_or(1) as usize;
            d.perks[tier - 1].map(|p| p.def().id.to_string()).unwrap_or_else(|| "—".into())
        }
        Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            let tier = slot.perk_tier().unwrap_or(1) as usize;
            d.extra_perks[tier - 1].map(|p| p.def().id.to_string()).unwrap_or_else(|| "—".into())
        }
        Slot::Wildcards => {
            if d.wildcards.is_empty() {
                "—".into()
            } else {
                d.wildcards.iter().map(|w| w.name()).collect::<Vec<_>>().join(", ")
            }
        }
        Slot::Save => String::new(),
    }
}

/// A short list of attachment shorts as their display names, joined. The names come from the
/// catalog's attachment table, so they read as BO2's; an unknown short falls back to itself.
fn att_names(atts: &[String]) -> String {
    if atts.is_empty() {
        return "—".into();
    }
    let d = weapons::catalog();
    atts.iter()
        .map(|s| {
            d.all()
                .iter()
                .find_map(|w| w.allowed.iter().find(|(short, _)| short == s).map(|(_, asset)| asset.clone()))
                .and_then(|asset| d.attachment(&asset).map(|a| a.display_name.clone()))
                .unwrap_or_else(|| s.clone())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The display names of the catalog's guns in the given categories, in catalog order. The editor
/// opens on the held weapon through [`Editor::current_option_index`], not by reordering the list.
fn weapon_options(slots: &[&str]) -> Vec<String> {
    gun_ids(slots).iter().map(|id| weapons::weapon(id).display_name.clone()).collect()
}

/// The attachment display names the weapon offers (`WeaponDef::allowed`); the ones already on it
/// are included so they can be removed. The chosen attachments come first so the slot opens on them.
fn attachment_options(base: &str, current: &[String]) -> Vec<String> {
    let d = weapons::weapon(base);
    let name_of = |short: &str| -> String {
        d.allowed
            .iter()
            .find(|(s, _)| s == short)
            .map(|(_, asset)| weapons::catalog().attachment(asset).map(|a| a.display_name.clone()).unwrap_or_else(|| asset.clone()))
            .unwrap_or_else(|| short.to_string())
    };
    let mut out: Vec<String> = current.iter().map(|s| name_of(s)).collect();
    for (short, asset) in &d.allowed {
        if current.iter().any(|s| s == short) {
            continue;
        }
        out.push(weapons::catalog().attachment(asset).map(|a| a.display_name.clone()).unwrap_or_else(|| asset.clone()));
    }
    out
}

/// The display names of the equipment of a class, in catalog order. The editor opens on the held
/// one through [`Editor::current_option_index`].
fn equipment_options(class: WeaponClass) -> Vec<String> {
    equipment_ids(class).iter().map(|id| weapons::weapon(id).display_name.clone()).collect()
}

/// The display names of the perks in a tier.
fn perk_options(tier: u8) -> Vec<String> {
    weapons::perks::PERKS.iter().filter(|p| p.tier == tier).map(|p| p.id.to_string()).collect()
}

/// A key edge this frame (fresh press): the editor's input is edge-driven like the pick screen.
/// `typed` is the fresh presses of the printable keys (A-Z, 0-9 and Space), in `Key::ALL` order, so
/// the name row can append them; it is empty outside text entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
    pub backspace: bool,
    /// Tab on the class row: BO2's `knifePersonalization` prompt (`caceditclass.lua:52-63`).
    pub personalize: bool,
    /// `C` on the primary weapon row: BO2's `AddCamoButton` (`caceditclass.lua:748`).
    pub camo: bool,
    /// `V` on the primary optic row: BO2's `AddReticleButton` (`caceditclass.lua:825`).
    pub reticle: bool,
    pub typed: Vec<Key>,
}

impl Edges {
    /// The edges in `now` that were not in `prev`.
    pub fn between(prev: &crate::input::KeySet, now: &crate::input::KeySet) -> Edges {
        let edge = |k: Key| now.is_down(k) && !prev.is_down(k);
        let typed = Key::ALL.iter().copied().filter(|k| is_typable(*k) && edge(*k)).collect();
        Edges {
            up: edge(Key::Up),
            down: edge(Key::Down),
            left: edge(Key::Left),
            right: edge(Key::Right),
            confirm: edge(Key::Enter),
            cancel: edge(Key::Escape),
            backspace: edge(Key::Backspace),
            personalize: edge(Key::Tab),
            camo: edge(Key::C),
            reticle: edge(Key::V),
            typed,
        }
    }
}

/// Whether a key types a character into the class-name field: A-Z, 0-9 and Space. `char_of` gives it.
fn is_typable(k: Key) -> bool {
    char_of(k).is_some()
}

/// The character a key types into the name field. BO2's class names are uppercase letters, digits
/// and the space: A-Z as uppercase letters, 0-9, and Space.
fn char_of(k: Key) -> Option<char> {
    match k {
        Key::Key0 => Some('0'),
        Key::Key1 => Some('1'),
        Key::Key2 => Some('2'),
        Key::Key3 => Some('3'),
        Key::Key4 => Some('4'),
        Key::Key5 => Some('5'),
        Key::Key6 => Some('6'),
        Key::Key7 => Some('7'),
        Key::Key8 => Some('8'),
        Key::Key9 => Some('9'),
        Key::A => Some('A'),
        Key::B => Some('B'),
        Key::C => Some('C'),
        Key::D => Some('D'),
        Key::E => Some('E'),
        Key::F => Some('F'),
        Key::G => Some('G'),
        Key::H => Some('H'),
        Key::I => Some('I'),
        Key::J => Some('J'),
        Key::K => Some('K'),
        Key::L => Some('L'),
        Key::M => Some('M'),
        Key::N => Some('N'),
        Key::O => Some('O'),
        Key::P => Some('P'),
        Key::Q => Some('Q'),
        Key::R => Some('R'),
        Key::S => Some('S'),
        Key::T => Some('T'),
        Key::U => Some('U'),
        Key::V => Some('V'),
        Key::W => Some('W'),
        Key::X => Some('X'),
        Key::Y => Some('Y'),
        Key::Z => Some('Z'),
        Key::Space => Some(' '),
        _ => None,
    }
}

/// The editor's outcome for one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The editor stays open; the caller keeps drawing it.
    Continue,
    /// The player confirmed a valid class: the caller saves it to the profile and closes.
    Save,
    /// The player cancelled: the caller closes without saving.
    Cancel,
}

impl Editor {
    /// Fold one frame's key edges into the session. `valid` gates the save (the caller's own check
    /// of the budget, so the UI can also show it).
    pub fn step(&mut self, edges: Edges) -> Outcome {
        self.denied = false;
        self.personalize = None;
        if self.naming {
            return self.step_naming(edges);
        }
        if self.picking {
            return self.step_picking(edges);
        }
        // BO2's `TAB Personalize Knife` prompt opens the personalize screen without moving the
        // row highlight or touching the draft (`caceditclass.lua:52-63`).
        if edges.personalize {
            self.personalize = Some(PersonalizeKind::Knife);
            return Outcome::Continue;
        }
        // BO2's `AddCamoButton` on the primary weapon row (`caceditclass.lua:748`).
        if edges.camo && self.slot == Slot::Primary {
            self.personalize = Some(PersonalizeKind::Camo);
            return Outcome::Continue;
        }
        // BO2's `AddReticleButton` on the primary optic row (`caceditclass.lua:825`).
        if edges.reticle && self.slot == Slot::PrimaryAtts && self.option_is_optic() {
            self.personalize = Some(PersonalizeKind::Reticle);
            return Outcome::Continue;
        }
        // The slot row is highlighted: up/down move the highlight, Enter opens the list.
        if edges.up {
            self.slot = self.prev_slot(self.slot);
            self.option = 0;
        }
        if edges.down {
            self.slot = self.next_slot(self.slot);
            self.option = 0;
        }
        if edges.confirm {
            return self.confirm();
        }
        if edges.cancel {
            return Outcome::Cancel;
        }
        Outcome::Continue
    }

    /// Confirm the highlighted row, the shared body of Enter and a click on a row: Save leaves the
    /// editor (only when the draft fits the budget), the name row starts text entry, everything else
    /// opens its option list on the current choice.
    fn confirm(&mut self) -> Outcome {
        if self.slot == Slot::Save {
            // BO2 refuses an over-budget save; surface it rather than closing silently.
            if self.draft.valid() {
                return Outcome::Save;
            }
            self.denied = true;
            return Outcome::Continue;
        }
        if self.slot == Slot::Name {
            self.naming = true;
            return Outcome::Continue;
        }
        self.picking = true;
        self.option = self.current_option_index();
        Outcome::Continue
    }

    /// Fold one pointer frame (hover/click/wheel, already hit-tested by the view) into the session.
    /// Pure like [`Editor::step`]; a click on a row is Enter on it. A *hover* only moves the
    /// highlight on a real cursor move, so keyboard navigation is never overridden by the mouse
    /// resting somewhere; a click always acts on the row under the cursor.
    pub fn step_pointer(&mut self, pointer: PointerEvent) -> Outcome {
        self.denied = false;
        if self.naming {
            // Clicking away leaves the name field; it never cancels the editor.
            if pointer.click {
                self.naming = false;
            }
            return Outcome::Continue;
        }
        if self.picking {
            let n = self.options().len();
            if pointer.wheel != 0 && n > 0 {
                self.option = (self.option as i32 + pointer.wheel).rem_euclid(n as i32) as usize;
            }
            if pointer.moved {
                if let Some(i) = pointer.option {
                    if i < n {
                        self.option = i;
                    }
                }
            }
            if pointer.click {
                if let Some(i) = pointer.option {
                    if i < n {
                        if self.apply(i) == ApplyResult::Denied {
                            self.denied = true;
                        }
                        self.picking = false;
                    }
                } else if let Some(i) = pointer.slot {
                    // A click outside the open list returns to the row walk and confirms that row.
                    if let Some(slot) = self.slots().get(i).copied() {
                        self.slot = slot;
                        self.option = 0;
                        self.picking = false;
                        return self.confirm();
                    }
                }
            }
            return Outcome::Continue;
        }
        if pointer.wheel != 0 {
            let list = self.slots();
            let i = list.iter().position(|s| *s == self.slot).unwrap_or(0) as i32;
            self.slot = list[(i + pointer.wheel).rem_euclid(list.len() as i32) as usize];
            self.option = 0;
        }
        if pointer.moved || pointer.click {
            if let Some(i) = pointer.slot {
                if let Some(slot) = self.slots().get(i).copied() {
                    self.slot = slot;
                    self.option = 0;
                }
            }
        }
        if pointer.click && pointer.slot.is_some() {
            return self.confirm();
        }
        Outcome::Continue
    }

    /// The name row has text entry: printable keys append, Backspace deletes, Enter/Escape leave
    /// the field (the text stays either way; Escape does not cancel the whole editor while naming).
    fn step_naming(&mut self, edges: Edges) -> Outcome {
        for key in &edges.typed {
            if let Some(c) = char_of(*key) {
                if self.draft.name.chars().count() < NAME_MAX {
                    self.draft.name.push(c);
                }
            }
        }
        if edges.backspace {
            self.draft.name.pop();
        }
        if edges.confirm || edges.cancel {
            self.naming = false;
        }
        Outcome::Continue
    }

    /// The rows to walk, in order: [`Editor::slots`].
    fn next_slot(&self, s: Slot) -> Slot {
        let list = self.slots();
        let i = list.iter().position(|x| *x == s).unwrap_or(0);
        list[(i + 1) % list.len()]
    }

    fn prev_slot(&self, s: Slot) -> Slot {
        let list = self.slots();
        let i = list.iter().position(|x| *x == s).unwrap_or(0);
        list[(i + list.len() - 1) % list.len()]
    }

    /// The option list is open: the arrows move, Enter picks, Escape closes the list (one step
    /// back). A content grid is two-dimensional (`Slot::option_columns`, `contentgrid.lua`): Left/
    /// Right walk the row and wrap at its ends, Up/Down walk the column and wrap at the first/last
    /// row. The weapon carousel keeps the plain list walk here; its four arrows are consumed by the
    /// shared fold (`classui::editor_frame`) because they steer its categories.
    fn step_picking(&mut self, edges: Edges) -> Outcome {
        // BO2's `AddReticleButton` on the primary optic row (`caceditclass.lua:825`).
        if edges.reticle && self.slot == Slot::PrimaryAtts && self.option_is_optic() {
            self.personalize = Some(PersonalizeKind::Reticle);
            return Outcome::Continue;
        }
        let n = self.options().len();
        if n == 0 {
            if edges.cancel || edges.confirm {
                self.picking = false;
            }
            return Outcome::Continue;
        }
        match self.slot.option_columns() {
            Some(cols) => {
                if edges.left {
                    self.option = grid_left(self.option, n, cols);
                }
                if edges.right {
                    self.option = grid_right(self.option, n, cols);
                }
                if edges.up {
                    self.option = grid_up(self.option, n, cols);
                }
                if edges.down {
                    self.option = grid_down(self.option, n, cols);
                }
            }
            None => {
                if edges.up {
                    self.option = (self.option + n - 1) % n;
                }
                if edges.down {
                    self.option = (self.option + 1) % n;
                }
            }
        }
        if edges.confirm {
            if self.apply(self.option) == ApplyResult::Denied {
                self.denied = true;
            }
            self.picking = false;
        }
        if edges.cancel {
            self.picking = false;
        }
        Outcome::Continue
    }

    /// The index in the option list of the slot's current value (so the list opens on it).
    fn current_option_index(&self) -> usize {
        match self.slot {
            Slot::Name => 0,
            Slot::Primary => weapon_index(&items::slots_of(true, &self.draft.wildcards), &self.draft.primary),
            Slot::Secondary => weapon_index(&self.secondary_slots(), &self.draft.secondary),
            Slot::PrimaryAtts | Slot::SecondaryAtts => 0,
            Slot::Lethal => equipment_index(WeaponClass::Lethal, &self.draft.lethal),
            Slot::Tactical => equipment_index(WeaponClass::Tactical, &self.draft.tactical),
            Slot::Perk1 | Slot::Perk1b => perk_index(1, self.perk_held(1)),
            Slot::Perk2 | Slot::Perk2b => perk_index(2, self.perk_held(2)),
            Slot::Perk3 | Slot::Perk3b => perk_index(3, self.perk_held(3)),
            Slot::Wildcards => 0,
            Slot::Save => 0,
        }
    }

    /// The perk the highlighted row holds: the tier's first pick on the base row, its Greed pick on
    /// the second row.
    fn perk_held(&self, tier: u8) -> Option<Perk> {
        let idx = (tier as usize).clamp(1, 3) - 1;
        if self.slot.is_extra_perk() {
            self.draft.extra_perks[idx]
        } else {
            self.draft.perks[idx]
        }
    }

    /// Apply option `i` of the highlighted slot to the draft. A choice that would spend an
    /// eleventh Pick-10 point, or add an attachment past the gun's cap, is refused and the draft
    /// is left exactly as it was (BO2's deny feedback reads this).
    fn apply(&mut self, i: usize) -> ApplyResult {
        let before = self.draft.clone();
        if !self.apply_option(i) {
            return ApplyResult::Denied;
        }
        if !self.draft.valid() {
            self.draft = before;
            return ApplyResult::Denied;
        }
        if self.draft == before {
            ApplyResult::NoChange
        } else {
            ApplyResult::Changed
        }
    }

    /// Apply option `i` without the budget check; false means the option itself was refused (an
    /// attachment at its cap). Weapon picks drop attachments the new weapon does not offer;
    /// attachment picks toggle; perk picks set the tier row; wildcard picks toggle.
    fn apply_option(&mut self, i: usize) -> bool {
        match self.slot {
            Slot::Name => {}
            Slot::Primary => {
                let ids = gun_ids(&items::slots_of(true, &self.draft.wildcards));
                if let Some(id) = ids.get(i).copied() {
                    self.draft.primary = id.to_string();
                    self.keep_offered_atts(true);
                }
            }
            Slot::Secondary => {
                let ids = gun_ids(&self.secondary_slots());
                if let Some(id) = ids.get(i).copied() {
                    self.draft.secondary = id.to_string();
                    self.keep_offered_atts(false);
                }
            }
            Slot::PrimaryAtts => {
                let shorts = attachment_shorts(&self.draft.primary, &self.draft.primary_atts);
                let max = self.draft.primary_att_max();
                if let Some(short) = shorts.get(i).copied() {
                    let base = self.draft.primary.clone();
                    if !toggle_att(&mut self.draft.primary_atts, short, max, |a, b| legal::compatible(&base, a, b)) {
                        return false;
                    }
                }
            }
            Slot::SecondaryAtts => {
                let shorts = attachment_shorts(&self.draft.secondary, &self.draft.secondary_atts);
                let max = self.draft.secondary_att_max();
                if let Some(short) = shorts.get(i).copied() {
                    let base = self.draft.secondary.clone();
                    if !toggle_att(&mut self.draft.secondary_atts, short, max, |a, b| legal::compatible(&base, a, b)) {
                        return false;
                    }
                }
            }
            Slot::Lethal => {
                if let Some(id) = equipment_ids(WeaponClass::Lethal).get(i).copied() {
                    self.draft.lethal = id.to_string();
                }
            }
            Slot::Tactical => {
                if let Some(id) = equipment_ids(WeaponClass::Tactical).get(i).copied() {
                    self.draft.tactical = id.to_string();
                }
            }
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 | Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
                let tier = self.slot.perk_tier().unwrap_or(1);
                let perks: Vec<Perk> = perk_list(tier);
                if let Some(p) = perks.get(i).copied() {
                    self.set_perk(tier, p);
                }
            }
            Slot::Wildcards => {
                if let Some(w) = Wildcard::ALL.get(i).copied() {
                    // A fourth bonus card is refused (BO2's `maxBonusCards`), so the editor turns it
                    // into the deny cue and leaves the draft alone.
                    if !self.toggle_wildcard(w) {
                        return false;
                    }
                }
            }
            Slot::Save => {}
        }
        true
    }

    /// Set the highlighted row's perk in a tier. The base row and the Greed row hold separate picks;
    /// the tier's option list refuses a perk already in the tier, so a tier's two picks differ.
    fn set_perk(&mut self, tier: u8, p: Perk) {
        let idx = (tier as usize).clamp(1, 3) - 1;
        if self.slot.is_extra_perk() {
            self.draft.extra_perks[idx] = Some(p);
        } else {
            self.draft.perks[idx] = Some(p);
        }
    }

    /// Toggle a wildcard on or off; turning one off drops the element it enabled (the third primary
    /// attachment, the second secondary attachment, a tier's Greed perk). False when a fourth card
    /// is refused: BO2 allows `maxBonusCards` (3) per class, so the option is denied untouched.
    fn toggle_wildcard(&mut self, w: Wildcard) -> bool {
        if let Some(pos) = self.draft.wildcards.iter().position(|x| *x == w) {
            self.draft.wildcards.remove(pos);
            match w {
                Wildcard::PrimaryGunfighter => {
                    self.draft.primary_atts.truncate(2);
                }
                Wildcard::SecondaryGunfighter => {
                    self.draft.secondary_atts.truncate(1);
                }
                _ => {
                    if let Some(tier) = w.greed_tier() {
                        let idx = (tier as usize).clamp(1, 3) - 1;
                        self.draft.extra_perks[idx] = None;
                        // The Greed row is gone; if it was highlighted, fall back to the base row.
                        if self.slot.is_extra_perk() && self.slot.perk_tier() == Some(tier) {
                            self.slot = base_perk_slot(tier);
                        }
                    }
                }
            }
            true
        } else if self.draft.wildcards_full() {
            false
        } else {
            self.draft.wildcards.push(w);
            true
        }
    }

    /// Drop attachments the (new) weapon does not offer, keeping the offered ones in order.
    fn keep_offered_atts(&mut self, primary: bool) {
        let base = if primary { self.draft.primary.clone() } else { self.draft.secondary.clone() };
        let allowed = allowed_shorts(&base);
        let max = if primary { self.draft.primary_att_max() } else { self.draft.secondary_att_max() };
        let atts = if primary { &mut self.draft.primary_atts } else { &mut self.draft.secondary_atts };
        atts.retain(|s| allowed.iter().any(|a| a == s));
        // BO2's compatibility is per attachment kind, so a kept set stays legal; the check is the
        // same one a stored class gets.
        let kept: Vec<String> = legal::sanitize(&base, &atts.iter().map(String::as_str).collect::<Vec<_>>()).into_iter().map(str::to_string).collect();
        atts.retain(|s| kept.contains(s));
        atts.truncate(max);
    }
}

/// Toggle an attachment short in the list, capped at `max`. Adding one first unequips the equipped
/// ones it is mutually exclusive with (`compatible` false; BO2's `ContentButtonSelected`, see
/// [`legal`]), then needs room under the cap. False means it could not be added (the cap is
/// reached): the caller turns that into the editor's deny cue and the list is left as it was.
fn toggle_att(atts: &mut Vec<String>, short: &str, max: usize, compatible: impl Fn(&str, &str) -> bool) -> bool {
    if let Some(pos) = atts.iter().position(|s| s == short) {
        atts.remove(pos);
        return true;
    }
    let kept: Vec<String> = atts.iter().filter(|s| compatible(s, short)).cloned().collect();
    if kept.len() < max {
        *atts = kept;
        atts.push(short.to_string());
        true
    } else {
        false
    }
}

/// The attachment shorts the weapon offers.
fn allowed_shorts(base: &str) -> Vec<&'static str> {
    weapons::weapon(base).allowed.iter().map(|(s, _)| s.as_str()).collect()
}

/// The attachment shorts in the same order as [`attachment_options`]: the chosen ones first, then
/// the rest of what the weapon offers. `apply` maps an option index to a short through this.
fn attachment_shorts(base: &str, current: &[String]) -> Vec<&'static str> {
    let allowed = allowed_shorts(base);
    let mut out: Vec<&'static str> = current.iter().filter_map(|s| allowed.iter().copied().find(|a| a == s)).collect();
    for s in allowed {
        if !current.iter().any(|c| c == s) {
            out.push(s);
        }
    }
    out
}

/// What the editor offers, as the conformance emitter (`zhbo-assets::conformance`) reads it: each
/// slot's weapon ids in list order, and per weapon the attachment shorts and the slot's caps. It is
/// the editor's own lists ([`gun_ids`], [`equipment_ids`], [`allowed_shorts`], [`Draft`]'s caps),
/// read-only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer {
    /// The primary slot's weapons (BO2's `primary` items, [`items`]).
    pub primary: Vec<&'static str>,
    /// The secondary slot's weapons without Overkill (BO2's `secondary` items).
    pub secondary: Vec<&'static str>,
    pub lethal: Vec<&'static str>,
    pub tactical: Vec<&'static str>,
    /// The most attachments a primary takes without, then with, Primary Gunfighter.
    pub primary_caps: (usize, usize),
    /// The most attachments a secondary takes without, then with, Secondary Gunfighter.
    pub secondary_caps: (usize, usize),
}

/// The editor's offer over the installed catalog.
pub fn offer() -> Offer {
    let caps = |card: Wildcard, primary: bool| {
        let mut d = Draft::default();
        let base = if primary { d.primary_att_max() } else { d.secondary_att_max() };
        d.wildcards.push(card);
        (base, if primary { d.primary_att_max() } else { d.secondary_att_max() })
    };
    Offer {
        primary: gun_ids(&items::slots_of(true, &[])),
        secondary: gun_ids(&items::slots_of(false, &[])),
        lethal: equipment_ids(WeaponClass::Lethal),
        tactical: equipment_ids(WeaponClass::Tactical),
        primary_caps: caps(Wildcard::PrimaryGunfighter, true),
        secondary_caps: caps(Wildcard::SecondaryGunfighter, false),
    }
}

/// The attachment shorts the editor offers on `base` ([`allowed_shorts`]).
pub fn offered_attachments(base: &str) -> Vec<&'static str> {
    allowed_shorts(base)
}

/// The weapon id a draft with these attachments asks the catalog for ([`Draft::primary_id`]).
pub fn draft_weapon_id(base: &str, shorts: &[&str]) -> String {
    Draft::weapon_with_atts(base, &shorts.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

/// The perks of a tier, in table order.
fn perk_list(tier: u8) -> Vec<Perk> {
    weapons::perks::PERKS.iter().enumerate().filter(|(_, p)| p.tier == tier).map(|(i, _)| Perk(i as u8)).collect()
}

/// The index of the held weapon in the category list.
fn weapon_index(slots: &[&str], current: &str) -> usize {
    gun_ids(slots).iter().position(|id| *id == current).unwrap_or(0)
}

/// The index of the held equipment in the class list.
fn equipment_index(class: WeaponClass, current: &str) -> usize {
    equipment_ids(class).iter().position(|id| *id == current).unwrap_or(0)
}

/// The index of the perk in its tier's list (the first when unset).
fn perk_index(tier: u8, p: Option<Perk>) -> usize {
    let list = perk_list(tier);
    p.and_then(|p| list.iter().position(|x| *x == p)).unwrap_or(0)
}

/// The base (non-Greed) row of a perk tier.
fn base_perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2,
        3 => Slot::Perk3,
        _ => Slot::Perk1,
    }
}

/// The editor's tests live in their own file: `classmenu.rs` is at the module-size limit.
#[cfg(test)]
#[path = "classmenu/tests.rs"]
mod tests;
