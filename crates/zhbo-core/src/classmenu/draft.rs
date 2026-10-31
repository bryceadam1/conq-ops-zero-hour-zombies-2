//! The class being edited (the Pick-10 choices) and its conversion to and from a [`CustomClass`].

use super::*;

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
    /// Whether the lethal (`[0]`) and the tactical (`[1]`) slot carry a second of their item, each
    /// its own element (BO2's second slot box, `caceditclass.lua:506-566`'s `EquipGrenade` raising
    /// the slot's `count`). The tactical slot may; the lethal slot with Tactician's tactical or
    /// Danger Close's second lethal ([`Draft::offhand_boxes`]). Ignored while the slot's button
    /// has one box.
    pub second_offhand: [bool; 2],
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
            second_offhand: [false; 2],
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
    pub(super) fn weapon_with_atts(base: &str, atts: &[String]) -> String {
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

    /// How many Pick-10 points the draft has spent. Every element is one point — the two weapons,
    /// every attachment, the lethal, the tactical, each perk — and each wildcard its card's cost.
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
        // A second lethal (Danger Close's, or Tactician's second tactical) is its own element (BO2
        // charges each filled box), as is a second tactical.
        if self.lethal_count() > 1 {
            n += 1;
        }
        if !self.tactical.is_empty() {
            n += 1;
        }
        if self.tactical_count() > 1 {
            n += 1;
        }
        n += self.perks.iter().filter(|p| p.is_some()).count() as i32;
        n += self.extra_perks.iter().filter(|p| p.is_some()).count() as i32;
        n += self.wildcards.iter().map(|w| w.cost()).sum::<i32>();
        n
    }

    /// The points left of the ten.
    pub fn remaining(&self) -> i32 {
        PICK_10 - self.spent()
    }

    /// Whether the draft's choices fit the budget (never over ten) and the three-card cap.
    pub fn valid(&self) -> bool {
        self.remaining() >= 0 && self.wildcards.len() <= classes::max_bonus_cards()
    }

    /// Whether every bonus-card slot is taken (`maxBonusCards`), so another card is denied.
    pub fn wildcards_full(&self) -> bool {
        self.wildcards.len() >= classes::max_bonus_cards()
    }

    /// How many lethals the class carries: BO2's `primarygrenadecount`, the filled boxes of the
    /// slot's button (`cacgrenadesbutton.lua:130-230` counts the `statusN` boxes), which
    /// `_class.gsc:718-739` gives as the item's clip — so one lethal is one, whatever the item's
    /// own start count (one C4, one Trophy System), and two when the second box is filled (Danger
    /// Close's second lethal, Tactician's second tactical, [`Draft::offhand_boxes`]); 0 for an
    /// empty slot.
    pub fn lethal_count(&self) -> u8 {
        if self.lethal.is_empty() {
            0
        } else {
            self.chosen_count(true)
        }
    }

    /// How many tacticals the class carries: BO2's `specialgrenadecount`, one, or two when the
    /// slot's second box is taken (a point of its own); 0 for an empty slot.
    pub fn tactical_count(&self) -> u8 {
        if self.tactical.is_empty() {
            0
        } else {
            self.chosen_count(false)
        }
    }

    /// How many of its item an offhand slot holds (0 empty): the given count. The button's boxes
    /// fill to it and its name reads `x2` past one.
    pub fn offhand_count(&self, lethal: bool) -> u8 {
        if lethal {
            self.lethal_count()
        } else {
            self.tactical_count()
        }
    }

    /// One, or two when the slot's second box is taken and the button has one.
    pub(super) fn chosen_count(&self, lethal: bool) -> u8 {
        if self.second_offhand[usize::from(!lethal)] && self.offhand_boxes(lethal) > 1 {
            2
        } else {
            1
        }
    }

    /// How many boxes an offhand slot's button has for its item (`cacgrenadesbutton.lua:130-180`):
    /// two in the tactical slot and for Tactician's tactical in the lethal slot, one for a lethal
    /// item plus a second with Danger Close — each capped at the item's own
    /// ([`items::offhand_cap`]: `math.min(boxes, Engine.GetMaxAmmoForItem(item))`), at least one.
    pub fn offhand_boxes(&self, lethal: bool) -> u8 {
        let id = if lethal { &self.lethal } else { &self.tactical };
        if lethal && !items::is_tactical_item(id) && !self.wildcards.contains(&Wildcard::DangerClose) {
            1
        } else {
            items::offhand_cap(id).max(1)
        }
    }

    /// The most attachments the primary may carry: BO2's two (`maxAttachments`), or three with
    /// Primary Gunfighter.
    pub fn primary_att_max(&self) -> usize {
        if self.wildcards.contains(&Wildcard::PrimaryGunfighter) {
            classes::max_attachments() + 1
        } else {
            classes::max_attachments()
        }
    }

    /// The most attachments the secondary may carry: one (BO2's secondary slot takes one fewer),
    /// or two with Secondary Gunfighter.
    pub fn secondary_att_max(&self) -> usize {
        if self.wildcards.contains(&Wildcard::SecondaryGunfighter) {
            classes::max_attachments()
        } else {
            classes::max_attachments() - 1
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
        if self.lethal_count() > 0 {
            push("lethal_count", &self.lethal_count().to_string());
        }
        if self.tactical_count() > 0 {
            push("tactical_count", &self.tactical_count().to_string());
        }
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
            lethal_count: self.lethal_count(),
            tactical_count: self.tactical_count(),
            melee: "knife_held_mp".into(),
            perks: self.perks_in_order().iter().map(|p| p.def().id.to_string()).collect(),
            reserve: self.reserve,
            camo: self.camo.clone(),
            reticle: self.reticle.clone(),
            reticle_color: self.reticle_color.clone(),
            knife: self.knife.clone(),
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
            second_offhand: [c.lethal_count > 1, c.tactical_count > 1],
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
pub(super) fn split_atts(id: &str) -> (String, Vec<String>) {
    items::given_parts(id)
}

/// The perks of a saved class into the three tiers' first and second picks: the first perk in a
/// tier is its primary row, the second its Greed row. A second in a tier needs the matching Greed
/// wildcard, which [`wildcards_of`] restores.
pub(super) fn perks_of(ids: &[String]) -> ([Option<Perk>; 3], [Option<Perk>; 3]) {
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
pub(super) fn wildcards_of(c: &CustomClass) -> Vec<Wildcard> {
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
    // The lethal slot holding a tactical item is Tactician; carrying two lethals is Danger Close
    // (`checkclasses.lua:677-700`: each needs its card).
    if items::is_tactical_item(&c.lethal) {
        out.push(Wildcard::Tactician);
    } else if !c.lethal.is_empty() && c.lethal_count > 1 {
        out.push(Wildcard::DangerClose);
    }
    out
}
