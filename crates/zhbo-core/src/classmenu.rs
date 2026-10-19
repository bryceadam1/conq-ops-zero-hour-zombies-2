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
            Slot::Primary => "PRIMARY WEAPON",
            Slot::PrimaryAtts => "ATTACHMENTS",
            Slot::Secondary => "SECONDARY WEAPON",
            Slot::SecondaryAtts => "ATTACHMENTS",
            Slot::Lethal => "LETHAL",
            Slot::Tactical => "TACTICAL",
            Slot::Perk1 | Slot::Perk1b => "PERK 1",
            Slot::Perk2 | Slot::Perk2b => "PERK 2",
            Slot::Perk3 | Slot::Perk3b => "PERK 3",
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
        n += self.extra_perks.iter().filter(|p| p.is_some()).count() as i32;
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
        }
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
        }
    }
}

/// Split a weapon id into its base and attachment shorts (`an94_mp+gl+reflex`).
fn split_atts(id: &str) -> (String, Vec<String>) {
    let mut parts = id.split('+');
    let base = parts.next().unwrap_or("").to_string();
    (base, parts.map(|s| s.to_string()).collect())
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
    /// Whether the class-name row has text entry (BO2's name field): printable keys edit the name
    /// instead of moving the highlight, and Enter/Escape leave it.
    pub naming: bool,
}

impl Editor {
    pub fn new(draft: Draft) -> Editor {
        Editor { draft, slot: Slot::Primary, option: 0, picking: false, naming: false }
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

    /// The option list for the highlighted slot: display strings, in the order the arrows walk.
    /// The lists are rebuilt on demand so a weapon change re-derives the attachment list.
    pub fn options(&self) -> Vec<String> {
        match self.slot {
            Slot::Name => Vec::new(),
            Slot::Primary => weapon_options(&PRIMARY_CLASSES),
            Slot::PrimaryAtts => attachment_options(&self.draft.primary, &self.draft.primary_atts),
            Slot::Secondary => weapon_options(&self.secondary_classes()),
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

    /// The secondary slot's categories: BO2's pistols and launchers, plus the primary-class guns
    /// when Overkill puts a second primary there.
    fn secondary_classes(&self) -> Vec<WeaponClass> {
        let mut out: Vec<WeaponClass> = SECONDARY_CLASSES.to_vec();
        if self.draft.wildcards.contains(&Wildcard::Overkill) {
            for c in PRIMARY_CLASSES {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }

    /// Whether option `i` of the highlighted slot is already in the draft (so the list can mark it).
    /// A weapon/equipment/perk/wildcard option is "chosen" when it is the current value; an
    /// attachment option is chosen when the weapon carries its short.
    pub fn option_chosen(&self, i: usize) -> bool {
        match self.slot {
            Slot::Name => false,
            Slot::Primary => gun_ids(&PRIMARY_CLASSES).get(i).is_some_and(|id| *id == self.draft.primary),
            Slot::Secondary => gun_ids(&self.secondary_classes()).get(i).is_some_and(|id| *id == self.draft.secondary),
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
/// `typed` is the fresh presses of the printable keys (A-Z and Space), in `Key::ALL` order, so the
/// name row can append them; it is empty outside text entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
    pub backspace: bool,
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
            typed,
        }
    }
}

/// Whether a key types a character into the class-name field: A-Z and Space. `char_of` gives it.
fn is_typable(k: Key) -> bool {
    char_of(k).is_some()
}

/// The character a key types into the name field: A-Z as uppercase letters, Space as a space.
fn char_of(k: Key) -> Option<char> {
    match k {
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
        if self.naming {
            return self.step_naming(edges);
        }
        if self.picking {
            return self.step_picking(edges);
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
            // The Save row has no list to open: confirming it leaves the editor, the caller saving
            // the class (it gates on the draft's validity).
            if self.slot == Slot::Save {
                return Outcome::Save;
            }
            // The name row opens text entry instead of an option list.
            if self.slot == Slot::Name {
                self.naming = true;
                return Outcome::Continue;
            }
            self.picking = true;
            self.option = self.current_option_index();
        }
        if edges.cancel {
            return Outcome::Cancel;
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
            Slot::Name => 0,
            Slot::Primary => weapon_index(&PRIMARY_CLASSES, &self.draft.primary),
            Slot::Secondary => weapon_index(&self.secondary_classes(), &self.draft.secondary),
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

    /// Apply option `i` of the highlighted slot to the draft. Weapon picks drop attachments the new
    /// weapon does not offer; attachment picks toggle; perk picks set the tier row; wildcard picks
    /// toggle.
    fn apply(&mut self, i: usize) {
        match self.slot {
            Slot::Name => {}
            Slot::Primary => {
                let ids = gun_ids(&PRIMARY_CLASSES);
                if let Some(id) = ids.get(i).copied() {
                    self.draft.primary = id.to_string();
                    self.keep_offered_atts(true);
                }
            }
            Slot::Secondary => {
                let ids = gun_ids(&self.secondary_classes());
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
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 | Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
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
    /// attachment, the second secondary attachment, a tier's Greed perk).
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

/// The base (non-Greed) row of a perk tier.
fn base_perk_slot(tier: u8) -> Slot {
    match tier {
        2 => Slot::Perk2,
        3 => Slot::Perk3,
        _ => Slot::Perk1,
    }
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
        let e = Editor::new(Draft::default());
        assert_eq!(e.next_slot(Slot::Name), Slot::Primary);
        assert_eq!(e.next_slot(Slot::Primary), Slot::PrimaryAtts);
        // The first row wraps up to the Save row, and Save wraps down to the first row.
        assert_eq!(e.prev_slot(Slot::Name), Slot::Save);
        assert_eq!(e.next_slot(Slot::Wildcards), Slot::Save);
        assert_eq!(e.next_slot(Slot::Save), Slot::Name);
        assert_eq!(e.prev_slot(Slot::Save), Slot::Wildcards);
        // Walk the whole list and come back.
        let mut s = Slot::Name;
        for _ in 0..Slot::ALL.len() {
            s = e.next_slot(s);
        }
        assert_eq!(s, Slot::Name);
    }

    #[test]
    fn the_save_row_confirms_into_a_save_and_has_no_list() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Save;
        // The Save row carries no options, so it can never open a list.
        assert!(e.options().is_empty());
        // Confirming it leaves the editor with a save; a down edge walks off it to the first row.
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Save);
        assert!(!e.picking, "the save row does not enter the list state");
        assert_eq!(e.step(Edges { down: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.slot, Slot::Name);
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

    // --- Option 2: the class-name row types text ---

    #[test]
    fn the_name_row_starts_text_entry_and_letters_append() {
        let mut e = Editor::new(Draft { name: String::new(), ..Draft::default() });
        e.slot = Slot::Name;
        // The name row has no option list: Enter starts text entry instead of opening one.
        assert!(e.options().is_empty());
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(e.naming, "Enter on the name row starts text entry");
        assert!(!e.picking, "the name row never opens an option list");
        // Letters append in the order they were pressed.
        e.step(Edges { typed: vec![Key::M, Key::P], ..Default::default() });
        assert_eq!(e.draft.name, "MP");
        // Space types a space; Backspace deletes the last character.
        e.step(Edges { typed: vec![Key::Space, Key::A], ..Default::default() });
        assert_eq!(e.draft.name, "MP A");
        e.step(Edges { backspace: true, ..Default::default() });
        assert_eq!(e.draft.name, "MP ");
        // Enter leaves the field (the text stays); the row stays highlighted and not in the list.
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.naming);
        assert_eq!(e.draft.name, "MP ");
        assert_eq!(e.slot, Slot::Name);
    }

    #[test]
    fn the_name_field_caps_at_bo2s_length() {
        let mut e = Editor::new(Draft { name: String::new(), ..Draft::default() });
        e.slot = Slot::Name;
        e.naming = true;
        // Type far past the cap; the field refuses characters once it is full.
        let keys: Vec<Key> = std::iter::repeat(Key::A).take(NAME_MAX + 5).collect();
        e.step(Edges { typed: keys, ..Default::default() });
        assert_eq!(e.draft.name.chars().count(), NAME_MAX);
        assert_eq!(e.draft.name, "A".repeat(NAME_MAX));
    }

    #[test]
    fn escape_leaves_the_name_field_without_cancelling_the_editor() {
        let mut e = Editor::new(Draft { name: "Rifle".into(), ..Draft::default() });
        e.slot = Slot::Name;
        e.naming = true;
        // Escape leaves the field, it does not cancel the whole editor while naming.
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.naming);
        assert_eq!(e.draft.name, "Rifle");
        // A second Escape (now out of the field) cancels the editor as usual.
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
    }

    #[test]
    fn the_name_round_trips_through_a_saved_class() {
        let mut d = Draft { name: "My Rifle".into(), ..Draft::default() };
        d.primary = "an94_mp".into();
        let c = d.to_custom();
        assert_eq!(c.name, "My Rifle");
        let back = Draft::from_custom(&c);
        assert_eq!(back.name, "My Rifle");
    }

    // --- Option 3: Greed second perks and Overkill's second primary ---

    #[test]
    fn a_greed_wildcard_splices_in_a_second_perk_row() {
        let mut e = Editor::new(Draft::default());
        // Without the wildcard the second row is absent from the walk.
        assert!(!e.slots().contains(&Slot::Perk1b));
        assert_eq!(e.slots().len(), Slot::ALL.len());
        // Turning on Perk 1 Greed inserts the row right after the base Perk 1 row.
        e.draft.wildcards.push(Wildcard::Perk1Greed);
        let slots = e.slots();
        let base = slots.iter().position(|s| *s == Slot::Perk1).expect("Perk 1 row");
        assert_eq!(slots[base + 1], Slot::Perk1b, "the Greed row follows its base row");
        // It reads as its tier's header and knows its tier.
        assert_eq!(Slot::Perk1b.label(), "PERK 1");
        assert_eq!(Slot::Perk1b.perk_tier(), Some(1));
        assert!(Slot::Perk1b.is_extra_perk());
        assert!(!Slot::Perk1.is_extra_perk());
    }

    #[test]
    fn the_second_perk_row_sets_the_extra_pick_without_touching_the_base() {
        let mut e = Editor::new(Draft::default());
        e.draft.wildcards.push(Wildcard::Perk1Greed);
        e.slot = Slot::Perk1b;
        let perks = perk_list(1);
        // Pick a tier-1 perk other than the base pick (Lightweight).
        let choice = perks
            .iter()
            .position(|p| *p != Perk::LIGHTWEIGHT)
            .expect("a second tier-1 perk");
        e.picking = true;
        e.option = choice;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.extra_perks[0], Some(perks[choice]));
        // The base pick is untouched, and both picks resolve into the spec.
        assert_eq!(e.draft.perks[0], Some(Perk::LIGHTWEIGHT));
        let perks_in_order = e.draft.perks_in_order();
        assert_eq!(perks_in_order.len(), 4, "three base perks plus the Greed pick");
        assert!(perks_in_order.contains(&perks[choice]));
    }

    #[test]
    fn turning_off_a_greed_wildcard_drops_its_second_perk() {
        let mut e = Editor::new(Draft::default());
        e.draft.wildcards.push(Wildcard::Perk2Greed);
        e.slot = Slot::Perk2b;
        e.draft.extra_perks[1] = Some(Perk::SCAVENGER);
        // Highlighted on the Greed row, turning the wildcard off falls back to the base row and
        // clears the extra pick.
        e.toggle_wildcard(Wildcard::Perk2Greed);
        assert!(!e.draft.wildcards.contains(&Wildcard::Perk2Greed));
        assert_eq!(e.draft.extra_perks[1], None);
        assert_eq!(e.slot, Slot::Perk2, "the highlight returns to the base row");
        // The Greed row is gone from the walk again.
        assert!(!e.slots().contains(&Slot::Perk2b));
    }

    #[test]
    fn a_greed_perk_costs_its_point_in_the_budget() {
        let mut d = Draft::default();
        let base = d.spent();
        d.extra_perks[0] = Some(Perk::SCAVENGER);
        assert_eq!(d.spent(), base + 1, "the Greed perk costs a point");
        d.wildcards.push(Wildcard::Perk1Greed);
        assert_eq!(d.spent(), base + 2, "the wildcard costs a point too");
    }

    #[test]
    fn overkill_offers_primary_guns_in_the_secondary_list() {
        let mut e = Editor::new(Draft::default());
        let base = e.secondary_classes();
        assert_eq!(base, SECONDARY_CLASSES.to_vec(), "pistols and launchers only");
        e.draft.wildcards.push(Wildcard::Overkill);
        let with_overkill = e.secondary_classes();
        // The primary-class guns are offered in the secondary slot.
        for c in PRIMARY_CLASSES {
            assert!(with_overkill.contains(&c), "Overkill offers {c:?}");
        }
        // The option list walks the same categories.
        e.slot = Slot::Secondary;
        let options = e.options();
        assert!(options.contains(&weapons::weapon("an94_mp").display_name.to_string()));
    }

    #[test]
    fn overkill_puts_a_second_primary_into_the_secondary_slot() {
        let mut e = Editor::new(Draft::default());
        e.draft.wildcards.push(Wildcard::Overkill);
        e.slot = Slot::Secondary;
        let classes = e.secondary_classes();
        let ids = gun_ids(&classes);
        // Pick the first primary-class gun offered in the secondary list.
        let target = "an94_mp";
        let i = ids.iter().position(|id| *id == target).expect("AN-94 is offered");
        e.picking = true;
        e.option = i;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.secondary, target);
        // A round trip resolves it as the secondary and re-derives Overkill.
        let c = e.draft.to_custom();
        assert_eq!(c.secondary, target);
        let back = Draft::from_custom(&c);
        assert_eq!(back.secondary, target);
        assert!(back.wildcards.contains(&Wildcard::Overkill));
    }
}
