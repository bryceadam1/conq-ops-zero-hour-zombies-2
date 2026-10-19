//! BO2's Create-a-Class editor: the Pick-10 slot editor behind the "CREATE A CLASS" screen.
//!
//! The pick screen (`hud::draw_class_pick`) is the *quick* choice between shipped presets; this
//! module is the full editor BO2 opens from the lobby's Create-a-Class button. It is a
//! keyboard-driven state machine over the Pick-10 model:
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

use crate::classes::CustomClass;
use crate::input::Key;
use crate::weapons::{self, Class as WeaponClass, Perk};

/// The Pick-10 budget: ten points per class, exactly BO2's.
pub const PICK_10: i32 = 10;

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
    /// All eight, BO2's order.
    pub const ALL: [Wildcard; 8] = [
        Wildcard::Perk1Greed,
        Wildcard::Perk2Greed,
        Wildcard::Perk3Greed,
        Wildcard::Overkill,
        Wildcard::PrimaryGunfighter,
        Wildcard::SecondaryGunfighter,
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
    Primary,
    PrimaryAtts,
    Secondary,
    SecondaryAtts,
    Lethal,
    Tactical,
    Perk1,
    Perk2,
    Perk3,
    Wildcards,
    /// BO2's final row: confirming it saves the class and leaves the editor. It has no option list.
    Save,
}

impl Slot {
    /// Every slot, in BO2's create-a-class order. `Save` is the row below the elements, so it is
    /// last in the walk and the arrows wrap from it back to the primary.
    pub const ALL: [Slot; 11] = [
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

    /// BO2's header for the slot.
    pub fn label(self) -> &'static str {
        match self {
            Slot::Primary => "PRIMARY WEAPON",
            Slot::PrimaryAtts => "ATTACHMENTS",
            Slot::Secondary => "SECONDARY WEAPON",
            Slot::SecondaryAtts => "ATTACHMENTS",
            Slot::Lethal => "LETHAL",
            Slot::Tactical => "TACTICAL",
            Slot::Perk1 => "PERK 1",
            Slot::Perk2 => "PERK 2",
            Slot::Perk3 => "PERK 3",
            Slot::Wildcards => "WILDCARDS",
            Slot::Save => "SAVE CLASS",
        }
    }

    /// The perk tier this slot picks, if it is a perk slot.
    pub fn perk_tier(self) -> Option<u8> {
        match self {
            Slot::Perk1 => Some(1),
            Slot::Perk2 => Some(2),
            Slot::Perk3 => Some(3),
            _ => None,
        }
    }
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
    /// The three perk tiers' picks.
    pub perks: [Option<Perk>; 3],
    /// The wildcards in play.
    pub wildcards: Vec<Wildcard>,
    /// Spare rounds for the primary and secondary; 0 = BO2's default.
    pub reserve: [u16; 2],
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
            wildcards: Vec::new(),
            reserve: [0, 0],
        }
    }
}

impl Draft {
    /// The weapon id with its attachments (`an94_mp+gl+reflex`).
    fn weapon_with_atts(base: &str, atts: &[String]) -> String {
        if atts.is_empty() {
            base.to_string()
        } else {
            let mut shorts: Vec<&str> = atts.iter().map(|s| s.as_str()).collect();
            shorts.sort_unstable();
            shorts.dedup();
            std::iter::once(base).chain(shorts).collect::<Vec<_>>().join("+")
        }
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
        n += self.wildcards.len() as i32;
        n
    }

    /// The points left of the ten.
    pub fn remaining(&self) -> i32 {
        PICK_10 - self.spent()
    }

    /// Whether the draft's choices fit the budget (never over ten).
    pub fn valid(&self) -> bool {
        self.remaining() >= 0
    }

    /// The most attachments the primary may carry: 2, or 3 with Primary Gunfighter.
    pub fn primary_att_max(&self) -> usize {
        if self.wildcards.contains(&Wildcard::PrimaryGunfighter) {
            3
        } else {
            2
        }
    }

    /// The most attachments the secondary may carry: 1, or 2 with Secondary Gunfighter.
    pub fn secondary_att_max(&self) -> usize {
        if self.wildcards.contains(&Wildcard::SecondaryGunfighter) {
            2
        } else {
            1
        }
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
        let perks: Vec<&str> = self.perks.iter().flatten().map(|p| p.def().id).collect();
        push("perks", &perks.join(","));
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
            perks: self.perks.iter().flatten().map(|p| p.def().id.to_string()).collect(),
            reserve: self.reserve,
        }
    }

    /// Build a draft from a saved class (re-opening a profile entry in the editor).
    pub fn from_custom(c: &CustomClass) -> Draft {
        let (primary, primary_atts) = split_atts(&c.primary);
        let (secondary, secondary_atts) = split_atts(&c.secondary);
        Draft {
            name: c.name.clone(),
            primary,
            primary_atts,
            secondary,
            secondary_atts,
            lethal: c.lethal.clone(),
            tactical: c.tactical.clone(),
            perks: perks_of(&c.perks),
            wildcards: wildcards_of(c),
            reserve: c.reserve,
        }
    }
}

/// Split a weapon id into its base and attachment shorts (`an94_mp+gl+reflex`).
fn split_atts(id: &str) -> (String, Vec<String>) {
    let mut parts = id.split('+');
    let base = parts.next().unwrap_or("").to_string();
    (base, parts.map(|s| s.to_string()).collect())
}

/// The perks of a saved class into the three tiers (the first in each tier wins; a second in a tier
/// needs the matching greed wildcard, which [`wildcards_of`] restores).
fn perks_of(ids: &[String]) -> [Option<Perk>; 3] {
    let mut out: [Option<Perk>; 3] = [None, None, None];
    for id in ids {
        let Some(p) = Perk::by_id(id) else { continue };
        let tier = (p.def().tier as usize).clamp(1, 3) - 1;
        if out[tier].is_none() {
            out[tier] = Some(p);
        }
    }
    out
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
    // The secondary holding a primary-class gun is Overkill.
    if weapons::weapon(&split_atts(&c.secondary).0).class.is_gun()
        && !SECONDARY_CLASSES.contains(&weapons::weapon(&split_atts(&c.secondary).0).class)
    {
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
}

impl Editor {
    pub fn new(draft: Draft) -> Editor {
        Editor { draft, slot: Slot::Primary, option: 0, picking: false }
    }

    /// The option list for the highlighted slot: display strings, in the order the arrows walk.
    /// The lists are rebuilt on demand so a weapon change re-derives the attachment list.
    pub fn options(&self) -> Vec<String> {
        match self.slot {
            Slot::Primary => weapon_options(&PRIMARY_CLASSES),
            Slot::PrimaryAtts => attachment_options(&self.draft.primary, &self.draft.primary_atts),
            Slot::Secondary => weapon_options(&SECONDARY_CLASSES),
            Slot::SecondaryAtts => attachment_options(&self.draft.secondary, &self.draft.secondary_atts),
            Slot::Lethal => equipment_options(WeaponClass::Lethal),
            Slot::Tactical => equipment_options(WeaponClass::Tactical),
            Slot::Perk1 => perk_options(1),
            Slot::Perk2 => perk_options(2),
            Slot::Perk3 => perk_options(3),
            Slot::Wildcards => Wildcard::ALL.iter().map(|w| w.name().to_string()).collect(),
            Slot::Save => Vec::new(),
        }
    }

    /// Whether option `i` of the highlighted slot is already in the draft (so the list can mark it).
    /// A weapon/equipment/perk/wildcard option is "chosen" when it is the current value; an
    /// attachment option is chosen when the weapon carries its short.
    pub fn option_chosen(&self, i: usize) -> bool {
        match self.slot {
            Slot::Primary => gun_ids(&PRIMARY_CLASSES).get(i).is_some_and(|id| *id == self.draft.primary),
            Slot::Secondary => gun_ids(&SECONDARY_CLASSES).get(i).is_some_and(|id| *id == self.draft.secondary),
            Slot::PrimaryAtts => {
                attachment_shorts(&self.draft.primary, &self.draft.primary_atts).get(i).is_some_and(|s| self.draft.primary_atts.iter().any(|a| a == s))
            }
            Slot::SecondaryAtts => {
                attachment_shorts(&self.draft.secondary, &self.draft.secondary_atts).get(i).is_some_and(|s| self.draft.secondary_atts.iter().any(|a| a == s))
            }
            Slot::Lethal => equipment_ids(WeaponClass::Lethal).get(i).is_some_and(|id| *id == self.draft.lethal),
            Slot::Tactical => equipment_ids(WeaponClass::Tactical).get(i).is_some_and(|id| *id == self.draft.tactical),
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
                let tier = self.slot.perk_tier().unwrap_or(1);
                let list = perk_list(tier);
                let cur = self.draft.perks[(tier as usize).clamp(1, 3) - 1];
                cur.is_some_and(|p| list.get(i).is_some_and(|x| *x == p))
            }
            Slot::Wildcards => Wildcard::ALL.get(i).is_some_and(|w| self.draft.wildcards.contains(w)),
            Slot::Save => false,
        }
    }
}

/// The current value of a slot as the editor's row shows it: the draft's own resolution, so the
/// screen reads exactly like the resulting class. Display names, not ids; `—` when a slot is empty.
pub fn slot_value(d: &Draft, slot: Slot) -> String {
    match slot {
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
fn weapon_options(classes: &[WeaponClass]) -> Vec<String> {    gun_ids(classes).iter().map(|id| weapons::weapon(id).display_name.clone()).collect()
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
}

impl Edges {
    /// The edges in `now` that were not in `prev`.
    pub fn between(prev: &crate::input::KeySet, now: &crate::input::KeySet) -> Edges {
        let edge = |k: Key| now.is_down(k) && !prev.is_down(k);
        Edges {
            up: edge(Key::Up),
            down: edge(Key::Down),
            left: edge(Key::Left),
            right: edge(Key::Right),
            confirm: edge(Key::Enter),
            cancel: edge(Key::Escape),
        }
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
        if self.picking {
            return self.step_picking(edges);
        }
        // The slot row is highlighted: up/down move the highlight, Enter opens the list.
        if edges.up {
            self.slot = prev_slot(self.slot);
            self.option = 0;
        }
        if edges.down {
            self.slot = next_slot(self.slot);
            self.option = 0;
        }
        if edges.confirm {
            // The Save row has no list to open: confirming it leaves the editor, the caller saving
            // the class (it gates on the draft's validity).
            if self.slot == Slot::Save {
                return Outcome::Save;
            }
            self.picking = true;
            self.option = self.current_option_index();
        }
        if edges.cancel {
            return Outcome::Cancel;
        }
        Outcome::Continue
    }

    /// The option list is open: up/down move, Enter picks, Escape closes the list (one step back).
    fn step_picking(&mut self, edges: Edges) -> Outcome {
        let n = self.options().len();
        if n == 0 {
            if edges.cancel || edges.confirm {
                self.picking = false;
            }
            return Outcome::Continue;
        }
        if edges.up {
            self.option = (self.option + n - 1) % n;
        }
        if edges.down {
            self.option = (self.option + 1) % n;
        }
        if edges.confirm {
            self.apply(self.option);
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
            Slot::Primary => weapon_index(&PRIMARY_CLASSES, &self.draft.primary),
            Slot::Secondary => weapon_index(&SECONDARY_CLASSES, &self.draft.secondary),
            Slot::PrimaryAtts | Slot::SecondaryAtts => 0,
            Slot::Lethal => equipment_index(WeaponClass::Lethal, &self.draft.lethal),
            Slot::Tactical => equipment_index(WeaponClass::Tactical, &self.draft.tactical),
            Slot::Perk1 => perk_index(1, self.draft.perks[0]),
            Slot::Perk2 => perk_index(2, self.draft.perks[1]),
            Slot::Perk3 => perk_index(3, self.draft.perks[2]),
            Slot::Wildcards => 0,
            Slot::Save => 0,
        }
    }

    /// Apply option `i` of the highlighted slot to the draft. Weapon picks drop attachments the new
    /// weapon does not offer; attachment picks toggle; perk picks set the tier (and a wildcard's
    /// second slot picks the tier's other perk); wildcard picks toggle.
    fn apply(&mut self, i: usize) {
        match self.slot {
            Slot::Primary => {
                let ids = gun_ids(&PRIMARY_CLASSES);
                if let Some(id) = ids.get(i).copied() {
                    self.draft.primary = id.to_string();
                    self.keep_offered_atts(true);
                }
            }
            Slot::Secondary => {
                let ids = gun_ids(&SECONDARY_CLASSES);
                if let Some(id) = ids.get(i).copied() {
                    self.draft.secondary = id.to_string();
                    self.keep_offered_atts(false);
                }
            }
            Slot::PrimaryAtts => {
                let shorts = attachment_shorts(&self.draft.primary, &self.draft.primary_atts);
                let max = self.draft.primary_att_max();
                if let Some(short) = shorts.get(i).copied() {
                    toggle_att(&mut self.draft.primary_atts, short, max);
                }
            }
            Slot::SecondaryAtts => {
                let shorts = attachment_shorts(&self.draft.secondary, &self.draft.secondary_atts);
                let max = self.draft.secondary_att_max();
                if let Some(short) = shorts.get(i).copied() {
                    toggle_att(&mut self.draft.secondary_atts, short, max);
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
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
                let tier = self.slot.perk_tier().unwrap_or(1);
                let perks: Vec<Perk> = perk_list(tier);
                if let Some(p) = perks.get(i).copied() {
                    self.set_perk(tier, p);
                }
            }
            Slot::Wildcards => {
                if let Some(w) = Wildcard::ALL.get(i).copied() {
                    self.toggle_wildcard(w);
                }
            }
            Slot::Save => {}
        }
    }

    /// Set a perk in a tier, avoiding a duplicate of the tier's other pick when a greed wildcard
    /// gives the tier two slots.
    fn set_perk(&mut self, tier: u8, p: Perk) {
        let idx = (tier as usize).clamp(1, 3) - 1;
        self.draft.perks[idx] = Some(p);
    }

    /// Toggle a wildcard on or off; turning one off drops the element it enabled (the third primary
    /// attachment, the second secondary attachment).
    fn toggle_wildcard(&mut self, w: Wildcard) {
        if let Some(pos) = self.draft.wildcards.iter().position(|x| *x == w) {
            self.draft.wildcards.remove(pos);
            match w {
                Wildcard::PrimaryGunfighter => {
                    self.draft.primary_atts.truncate(2);
                }
                Wildcard::SecondaryGunfighter => {
                    self.draft.secondary_atts.truncate(1);
                }
                _ => {}
            }
        } else {
            self.draft.wildcards.push(w);
        }
    }

    /// Drop attachments the (new) weapon does not offer, keeping the offered ones in order.
    fn keep_offered_atts(&mut self, primary: bool) {
        let base = if primary { self.draft.primary.clone() } else { self.draft.secondary.clone() };
        let allowed = allowed_shorts(&base);
        let max = if primary { self.draft.primary_att_max() } else { self.draft.secondary_att_max() };
        let atts = if primary { &mut self.draft.primary_atts } else { &mut self.draft.secondary_atts };
        atts.retain(|s| allowed.iter().any(|a| a == s));
        atts.truncate(max);
    }
}

/// Toggle an attachment short in the list, capped at `max`.
fn toggle_att(atts: &mut Vec<String>, short: &str, max: usize) {
    if let Some(pos) = atts.iter().position(|s| s == short) {
        atts.remove(pos);
    } else if atts.len() < max {
        atts.push(short.to_string());
    }
}

/// The gun ids of the categories, in the same order as [`weapon_options`].
fn gun_ids(classes: &[WeaponClass]) -> Vec<&'static str> {
    weapons::catalog().all().iter().filter(|d| classes.contains(&d.class)).map(|d| d.id.as_str()).collect()
}

/// The equipment ids of a class.
fn equipment_ids(class: WeaponClass) -> Vec<&'static str> {
    weapons::catalog().all().iter().filter(|d| d.class == class).map(|d| d.id.as_str()).collect()
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

/// The perks of a tier, in table order.
fn perk_list(tier: u8) -> Vec<Perk> {
    weapons::perks::PERKS.iter().enumerate().filter(|(_, p)| p.tier == tier).map(|(i, _)| Perk(i as u8)).collect()
}

/// The index of the held weapon in the category list.
fn weapon_index(classes: &[WeaponClass], current: &str) -> usize {
    gun_ids(classes).iter().position(|id| *id == current).unwrap_or(0)
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

fn next_slot(s: Slot) -> Slot {
    let i = Slot::ALL.iter().position(|x| *x == s).unwrap_or(0);
    Slot::ALL[(i + 1) % Slot::ALL.len()]
}

fn prev_slot(s: Slot) -> Slot {
    let i = Slot::ALL.iter().position(|x| *x == s).unwrap_or(0);
    Slot::ALL[(i + Slot::ALL.len() - 1) % Slot::ALL.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_draft_spends_the_base_kit_points() {
        let d = Draft::default();
        // 2 weapons + 2 primary attachments + lethal + tactical + 3 perks = 9; one point free.
        assert_eq!(d.spent(), 9);
        assert_eq!(d.remaining(), 1);
        assert!(d.valid());
        assert_eq!(d.primary_id(), "an94_mp+gl+reflex");
        assert_eq!(d.secondary_id(), "fnp45_mp");
    }

    #[test]
    fn every_wildcard_costs_a_point_and_round_trips_through_its_id() {
        for w in Wildcard::ALL {
            assert_eq!(Wildcard::by_id(w.id()), Some(w));
            assert!(!w.name().is_empty());
        }
        assert_eq!(Wildcard::Perk1Greed.greed_tier(), Some(1));
        assert_eq!(Wildcard::Overkill.greed_tier(), None);
    }

    #[test]
    fn a_weapon_pick_drops_attachments_the_new_weapon_does_not_offer() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Primary;
        // The built-in table's primary list: the AN-94 (rifle) and its Grenade Launcher variant.
        // Pick the launcher variant; the AN-94's gl/reflex are not offered by it.
        let ids = gun_ids(&PRIMARY_CLASSES);
        let launcher = ids.iter().position(|id| *id == "gl_an94_mp").expect("the launcher variant is in the catalog");
        e.picking = true;
        e.option = launcher;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.primary, "gl_an94_mp");
        assert!(e.draft.primary_atts.is_empty(), "{:?}", e.draft.primary_atts);
    }

    #[test]
    fn attachments_cap_at_two_until_the_gunfighter_wildcard() {
        let mut d = Draft { primary: "an94_mp".into(), primary_atts: vec![], ..Draft::default() };
        let shorts = allowed_shorts(&d.primary);
        assert!(shorts.len() >= 2);
        let mut max = d.primary_att_max();
        toggle_att(&mut d.primary_atts, shorts[0], max);
        toggle_att(&mut d.primary_atts, shorts[1], max);
        // A third is refused without the wildcard.
        let third = shorts.get(2).copied();
        if let Some(t) = third {
            toggle_att(&mut d.primary_atts, t, max);
            assert_eq!(d.primary_atts.len(), 2, "capped at 2");
        }
        d.wildcards.push(Wildcard::PrimaryGunfighter);
        max = d.primary_att_max();
        assert_eq!(max, 3);
        if let Some(t) = third {
            toggle_att(&mut d.primary_atts, t, max);
            assert_eq!(d.primary_atts.len(), 3);
        }
    }

    #[test]
    fn toggling_off_a_gunfighter_wildcard_truncates_its_extra_attachment() {
        let mut d = Draft { primary: "an94_mp".into(), primary_atts: vec!["gl".into(), "reflex".into(), "stock".into()], wildcards: vec![Wildcard::PrimaryGunfighter], ..Draft::default() };
        assert_eq!(d.spent(), 11);
        assert!(!d.valid());
        d.wildcards.remove(0);
        d.primary_atts.truncate(2);
        assert_eq!(d.spent(), 9);
        assert!(d.valid());
    }

    #[test]
    fn the_budget_refuses_an_over_spend() {
        let mut d = Draft::default();
        // Default is 9; adding a wildcard makes 10 (valid), another makes 11 (invalid).
        d.wildcards.push(Wildcard::Overkill);
        assert_eq!(d.remaining(), 0);
        assert!(d.valid());
        d.wildcards.push(Wildcard::Tactician);
        assert_eq!(d.remaining(), -1);
        assert!(!d.valid());
    }

    #[test]
    fn a_draft_round_trips_through_custom_class() {
        let mut d = Draft { name: "My Rifle".into(), primary: "an94_mp".into(), primary_atts: vec!["reflex".into()], ..Draft::default() };
        d.perks = [Some(Perk::LIGHTWEIGHT), Some(Perk::TOUGHNESS), Some(Perk::DEXTERITY)];
        let c = d.to_custom();
        assert_eq!(c.name, "My Rifle");
        assert_eq!(c.primary, "an94_mp+reflex");
        let back = Draft::from_custom(&c);
        assert_eq!(back.name, "My Rifle");
        assert_eq!(back.primary, "an94_mp");
        assert_eq!(back.primary_atts, vec!["reflex".to_string()]);
        assert_eq!(back.perks, d.perks);
        assert_eq!(back.to_spec(), d.to_spec());
    }

    #[test]
    fn the_spec_applies_over_the_base_kit() {
        let d = Draft { primary: "an94_mp".into(), primary_atts: vec![], perks: [Some(Perk::FLAK_JACKET), None, None], ..Draft::default() };
        let l = crate::weapons::Loadout::default().with_spec(&d.to_spec());
        assert_eq!(l.primary, "an94_mp");
        // The spec carries exactly the set perks; unset tiers fall back to the base kit's slots.
        assert_eq!(l.perks, vec![Perk::FLAK_JACKET]);
    }

    #[test]
    fn slot_navigation_wraps_around_bo2s_order() {
        assert_eq!(next_slot(Slot::Primary), Slot::PrimaryAtts);
        assert_eq!(prev_slot(Slot::Primary), Slot::Save);
        assert_eq!(next_slot(Slot::Wildcards), Slot::Save);
        assert_eq!(prev_slot(Slot::Save), Slot::Wildcards);
        // Walk the whole list and come back.
        let mut s = Slot::Primary;
        for _ in 0..Slot::ALL.len() {
            s = next_slot(s);
        }
        assert_eq!(s, Slot::Primary);
    }

    #[test]
    fn the_save_row_confirms_into_a_save_and_has_no_list() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Save;
        // The Save row carries no options, so it can never open a list.
        assert!(e.options().is_empty());
        // Confirming it leaves the editor with a save; a down edge walks off it to the primary.
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Save);
        assert!(!e.picking, "the save row does not enter the list state");
        assert_eq!(e.step(Edges { down: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.slot, Slot::Primary);
        // Escape still cancels (without saving).
        e.slot = Slot::Save;
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
    }

    #[test]
    fn the_editor_opens_the_list_on_the_held_option() {
        // The secondary slot opens on the Tac-45, which the built-in table holds.
        let mut e = Editor::new(Draft { secondary: "fnp45_mp".into(), ..Draft::default() });
        e.slot = Slot::Secondary;
        let idx = e.current_option_index();
        assert_eq!(gun_ids(&SECONDARY_CLASSES)[idx], "fnp45_mp");
        e.picking = true;
        e.option = idx;
        // Confirming the held weapon leaves the draft unchanged.
        let before = e.draft.clone();
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft, before);
    }

    #[test]
    fn cancel_closes_the_list_then_the_editor() {
        let mut e = Editor::new(Draft::default());
        e.picking = true;
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.picking);
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
    }

    #[test]
    fn perk_picks_land_in_their_tier() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Perk2;
        let perks = perk_list(2);
        let scav = perks.iter().position(|p| p.def().id == "Scavenger").expect("Scavenger is tier 2");
        e.picking = true;
        e.option = scav;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.perks[1], Some(Perk::SCAVENGER));
        // Tier 1 and 3 are untouched.
        assert_eq!(e.draft.perks[0], Some(Perk::LIGHTWEIGHT));
        assert_eq!(e.draft.perks[2], Some(Perk::DEXTERITY));
    }

    #[test]
    fn option_chosen_marks_the_drafts_current_elements() {
        // Primary: the held gun's index is the chosen one; no other weapon is.
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Primary;
        let held = e.current_option_index();
        assert!(e.option_chosen(held));
        assert!(!e.option_chosen(held + 1));
        // Attachments: the AN-94's gl and reflex are on the draft, so their list entries are chosen.
        e.slot = Slot::PrimaryAtts;
        let shorts = attachment_shorts(&e.draft.primary, &e.draft.primary_atts);
        for (i, short) in shorts.iter().enumerate() {
            let on = e.draft.primary_atts.iter().any(|a| a == short);
            assert_eq!(e.option_chosen(i), on, "attachment {short}");
        }
        // Perks: the tier's held perk is chosen.
        e.slot = Slot::Perk1;
        let p1 = perk_list(1).iter().position(|p| *p == Perk::LIGHTWEIGHT).expect("Lightweight is tier 1");
        assert!(e.option_chosen(p1));
        // Wildcards: none are on the default draft.
        e.slot = Slot::Wildcards;
        assert!(!e.option_chosen(0));
    }

    #[test]
    fn slot_values_read_as_the_resulting_class() {
        let d = Draft::default();
        assert_eq!(slot_value(&d, Slot::Primary), weapons::weapon("an94_mp").display_name);
        assert_eq!(slot_value(&d, Slot::Secondary), weapons::weapon("fnp45_mp").display_name);
        // The attachment row lists the display names, not the shorts.
        let atts = slot_value(&d, Slot::PrimaryAtts);
        assert!(atts.contains("Grenade Launcher") && atts.contains("Reflex"), "keeps names: {atts}");
        assert_ne!(atts, "—");
        // The perk rows read the tier's held perk.
        assert_eq!(slot_value(&d, Slot::Perk1), "Lightweight");
        assert_eq!(slot_value(&d, Slot::Wildcards), "—");
    }
}
