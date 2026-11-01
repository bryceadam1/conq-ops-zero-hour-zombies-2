//! BO2's loadout validation and repair, for classes read back from the profile file.
//!
//! BO2 never drops a stored class because one of its items went bad: `checkclasses.lua` walks the
//! class and repairs it in place, slot by slot. This module mirrors that pass on a
//! [`CustomClass`]:
//!
//! * **Order.** `CoD.CheckClasses.CheckItems` (`checkclasses.lua`) walks
//!   `CoD.CACUtility.loadoutSlotNames` once: `primary`, `secondary`, `primarygrenade`,
//!   `specialgrenade`, `specialty1`..`specialty6`, then the bonus cards and rewards. Every slot it
//!   finds invalid is cleared (`ClearSlot`) or, for a weapon, replaced by the matching default
//!   class's item. The extra elements the bonus cards unlock are checked in the same pass, after
//!   their slot: `specialty4` needs Perk 1 Greed, `specialty5` Perk 2 Greed, `specialty6` Perk 3
//!   Greed, `primaryattachment3` Primary Gunfighter, `secondaryattachment2` Secondary Gunfighter,
//!   and a primary-class gun in the secondary slot needs Overkill.
//! * **ClearWeapon first.** `CoD.CheckClasses.ClearWeapon` clears a weapon *and* its
//!   `attachment1`..`attachment3` (plus emblem, camo, tag and reticle) — a weapon that goes can
//!   never leave its attachments behind. [`repair`] does the same.
//! * **Trimming.** BO2 stores items, not points, so its checks only clamp the caps
//!   (`maxAttachments`, `maxSpecialtySlots`, `maxBonusCards`, `maxGrenades`); a hand-edited or
//!   migrated file can still arrive over the ten-point allocation or with a fourth card. The trim
//!   below drops one element at a time in the **reverse** of `CheckItems`' visit order
//!   (`specialty6` first, the primary weapon last), so the elements BO2 checks last are the first
//!   to go and the weapon is the last thing standing. The same order trims a fourth bonus card.
//! * **Grenade counts and the grenade cards.** `CheckItems` clamps a grenade count to `maxGrenades`
//!   (2) and clears the item at 0. A class here carries one lethal and one tactical id and their
//!   counts (`lethal_count`: Danger Close's two, or Tactician's chosen one or two; `tactical_count`:
//!   one or two), clamped the same way and to the item's own cap (`items::offhand_cap`), dropped
//!   when the slot is empty. A tactical item in the lethal slot is Tactician's
//!   (`checkclasses.lua:677-686`) and stays. A held item counts at least one (BO2's count is the
//!   slot's filled boxes), so a 0 (a class saved before the counts) becomes one.
//!
//! * **Camos.** `ClearWeapon` clears a weapon's camo with it, and BO2 stores each camo
//!   (`primarycamo`, `secondarycamo`, `knifecamo`, `checkclasses.lua:875-907`) as an index into the
//!   weapon's own option group, so a class can never hold a camo its weapon does not offer. The mod
//!   stores ids, so the pass checks each against its own weapon's `CamoList`: the primary's, the
//!   secondary's, `knife_held`'s. A camo whose weapon slot is empty (or was cleared above) or that
//!   is not in the list is cleared; a weapon without a converted list (no BO2 data) keeps it.
//!
//! Unknown ids are cleared, not substituted: `catalog::weapon_or_none` drops them loudly (the same
//! rule the arsenal uses), so a corrupt profile shows up in the log instead of quietly turning into
//! some default kit.

use crate::classmenu::items::{given_parts, given_weapon};
use crate::classes::{max_attachments, max_bonus_cards, max_specialty_slots, CustomClass};
use crate::classmenu::{Draft, PICK_10};
use crate::weapons::{self, Class as WeaponClass, Perk};

/// A slot repair emptied, named as `CoD.CACUtility.loadoutSlotNames` names it (`primary`,
/// `specialty4`, …), in the order it was cleared. Empty when the class was already valid.
pub type Cleared = Vec<&'static str>;

/// The slots a draft's three perk tiers map to, base row first then the Greed row, in BO2's names.
const PERK_SLOTS: [&str; 3] = ["specialty1", "specialty2", "specialty3"];
const EXTRA_PERK_SLOTS: [&str; 3] = ["specialty4", "specialty5", "specialty6"];

/// Repair `c` in place, BO2's way (see the module docs). Returns every slot it cleared.
pub fn repair(c: &mut CustomClass) -> Cleared {
    let mut cleared: Cleared = Vec::new();
    repair_weapon(c, true, &mut cleared);
    repair_weapon(c, false, &mut cleared);
    // A tactical in the lethal slot is Tactician's (re-derived from it, `classmenu::wildcards_of`).
    if !crate::classmenu::items::is_tactical_item(&c.lethal) {
        repair_offhand(c, true, &mut cleared);
    }
    repair_offhand(c, false, &mut cleared);
    repair_counts(c);
    if !c.melee.is_empty() && !resolves_as(&c.melee, WeaponClass::Melee) {
        c.melee.clear();
        cleared.push("melee");
    }
    repair_perks(c, &mut cleared);
    trim(c, &mut cleared);
    repair_camos(c, |id| weapons::catalog::weapon_or_none(id).map(|d| d.camo.clone()), &mut cleared);
    cleared
}

/// Each weapon's camo against that weapon's own list (see the module docs): `list` is a weapon's
/// `CamoList`, `None` for an unknown id. Runs after the trim, which can clear a weapon.
fn repair_camos(c: &mut CustomClass, list: impl Fn(&str) -> Option<Vec<String>>, cleared: &mut Cleared) {
    let knife = weapons::KNIFE_HELD.to_string();
    for (slot, weapon, camo) in [
        ("primarycamo", &c.primary, &mut c.camo),
        ("secondarycamo", &c.secondary, &mut c.secondary_camo),
        ("knifecamo", &knife, &mut c.knife),
    ] {
        if camo.is_empty() {
            continue;
        }
        let keep = !weapon.is_empty() && list(crate::attachments::split_id(weapon).0).is_some_and(|l| l.is_empty() || l.contains(camo));
        if !keep {
            camo.clear();
            cleared.push(slot);
        }
    }
}

/// A weapon slot: an unknown id, a non-gun or an item of another slot clears the weapon and its attachments together
/// (`ClearWeapon`); attachments the weapon does not offer are dropped (the per-weapon
/// `GetNumAttachments` check), and so are the ones past the slot's cap (`maxAttachments`, plus the
/// Gunfighter card's extra on the primary) and the ones BO2's compatibility lists forbid together.
/// An empty slot means "inherit the base kit" and stays empty.
fn repair_weapon(c: &mut CustomClass, primary: bool, cleared: &mut Cleared) {
    let (slot, id) = if primary { ("primary", &mut c.primary) } else { ("secondary", &mut c.secondary) };
    if id.is_empty() {
        return;
    }
    // The stored id is the weapon BO2 gives (`fnp45_dw_mp` for a pistol with `dw`); read it back
    // into the item and its attachments.
    let (base, shorts) = given_parts(id);
    let (base, shorts): (&str, Vec<&str>) = (&base, shorts.iter().map(String::as_str).collect());
    let Some(def) = weapons::catalog::weapon_or_none(base) else {
        id.clear();
        cleared.push(slot);
        return;
    };
    // A non-gun (the shield counts as one of the slot's weapons), or a gun BO2 does not offer in this slot (a `secondary` item in the primary, a
    // weapon that is no class item at all), is not a valid item for it (`CheckItems`).
    if !holds(def, crate::classmenu::items::fits_slot(base, primary)) {
        id.clear();
        cleared.push(slot);
        return;
    }
    let max = if primary { max_attachments() + 1 } else { max_attachments() };
    let offered: Vec<&str> = shorts.iter().copied().filter(|s| def.allowed.iter().any(|(a, _)| a == s)).take(max).collect();
    // A pair BO2's compatibility lists forbid is cut down by its conflicting-attachment check
    // (`FUN_008eb870`, `classmenu::legal::sanitize`).
    let offered = crate::classmenu::legal::sanitize(base, &offered);
    if offered.len() != shorts.len() {
        cleared.push(if primary { "primaryattachment" } else { "secondaryattachment" });
    }
    // Stored as the weapon BO2 gives: an older profile's `fnp45_mp+dw` migrates to `fnp45_dw_mp`
    // (no slot was cleared, so nothing is reported).
    let given = given_weapon(base, &offered);
    if *id != given {
        *id = given;
    }
}

/// Whether a weapon slot may hold `def` (`fits`: it is an item of that slot): a gun of the slot,
/// and not locked — an item whose art the install lacks (a DLC weapon without its pack,
/// [`crate::weapons::WeaponDef::cac_locked`]) is listed but never held.
fn holds(def: &crate::weapons::WeaponDef, fits: bool) -> bool {
    def.class.is_slot_weapon() && fits && !def.cac_locked
}

/// An offhand slot holding an item that is not its own is cleared (`ClearSlot`): the lethal slot
/// takes BO2's `primarygrenade` items (the claymore, Betty and C4 among them), the tactical slot the
/// `specialgrenade` ones — the statstable slot the editor lists them by, not the weapon's class.
fn repair_offhand(c: &mut CustomClass, lethal: bool, cleared: &mut Cleared) {
    let slot = crate::classmenu::items::offhand_slot(lethal);
    let id = if lethal { &mut c.lethal } else { &mut c.tactical };
    if id.is_empty() || (weapons::catalog::weapon_or_none(id).is_some() && crate::classmenu::items::fits_offhand(id, lethal)) {
        return;
    }
    id.clear();
    cleared.push(slot);
}

/// The stored grenade counts (`CheckItems` clamps a count to `maxGrenades`, and the slot list caps
/// it at the item's own max ammo, [`crate::classmenu::items::offhand_cap`]). A held item counts at
/// least one: BO2's count is the slot's filled boxes and an equipped item fills the first
/// (`cacgrenadesbutton.lua:130-230`), so a 0 (a class saved before the counts) is one. A lethal
/// item's second box is Danger Close's (the card is re-derived from that count); Tactician's
/// tactical and the tactical slot store their chosen one or two; each is capped at the item's own
/// max ammo, as the button's boxes are; an empty slot stores none.
fn repair_counts(c: &mut CustomClass) {
    use crate::classmenu::items;
    let clamp = |n: u8, id: &str| n.clamp(1, items::offhand_cap(id).max(1));
    c.lethal_count = if c.lethal.is_empty() { 0 } else { clamp(c.lethal_count, &c.lethal) };
    c.tactical_count = if c.tactical.is_empty() { 0 } else { clamp(c.tactical_count, &c.tactical) };
}

/// Whether the id resolves to a weapon of `class`.
fn resolves_as(id: &str, class: WeaponClass) -> bool {
    weapons::catalog::weapon_or_none(id).is_some_and(|d| d.class == class)
}

/// The perks: unknown ids go (they would silently vanish in `Loadout::with_spec` anyway), a repeat
/// goes, and a tier keeps at most two picks (`maxPerksInEachCategory`: BO2's two rows per tier).
/// Six slots in all (`maxSpecialtySlots`), which three tiers of two reach exactly.
fn repair_perks(c: &mut CustomClass, cleared: &mut Cleared) {
    let mut kept: Vec<Perk> = Vec::with_capacity(max_specialty_slots());
    let mut tiers = [0usize; 3];
    for id in &c.perks {
        let Some(p) = Perk::by_id(id) else {
            cleared.push("specialty");
            continue;
        };
        let tier = (p.def().tier as usize).clamp(1, 3) - 1;
        tiers[tier] += 1;
        let per_tier = crate::classes::max_perks_in_each_category();
        if kept.contains(&p) || tiers[tier] > per_tier || kept.len() >= max_specialty_slots() {
            cleared.push(if tiers[tier] > per_tier { EXTRA_PERK_SLOTS[tier] } else { "specialty" });
            continue;
        }
        kept.push(p);
    }
    if kept.len() != c.perks.len() {
        c.perks = kept.iter().map(|p| p.def().id.to_string()).collect();
    }
}

/// Drop elements until the class fits the ten-point allocation and the three-card cap, in the
/// reverse of `CheckItems`' visit order.
fn trim(c: &mut CustomClass, cleared: &mut Cleared) {
    loop {
        let d = Draft::from_custom(c);
        if d.spent() <= PICK_10 && d.wildcards.len() <= max_bonus_cards() {
            return;
        }
        if !drop_one(c, cleared) {
            return;
        }
    }
}

/// Drop one element, the first that exists, walking BO2's slot order backwards: `specialty6` down
/// to `specialty1`, the tactical, the lethal, the secondary's attachments then the secondary, the
/// primary's attachments, the primary. False when there is nothing left to drop.
fn drop_one(c: &mut CustomClass, cleared: &mut Cleared) -> bool {
    // The Greed rows first (`specialty4`..`specialty6`): they are what the bonus-card checks clear.
    let mut tiers = [Vec::new(), Vec::new(), Vec::new()];
    for id in &c.perks {
        if let Some(p) = Perk::by_id(id) {
            let tier = (p.def().tier as usize).clamp(1, 3) - 1;
            tiers[tier].push(p.def().id);
        }
    }
    for tier in (0..3).rev() {
        if tiers[tier].len() > 1 {
            let last = *tiers[tier].last().unwrap();
            if let Some(i) = c.perks.iter().rposition(|p| p == last) {
                c.perks.remove(i);
                cleared.push(EXTRA_PERK_SLOTS[tier]);
                return true;
            }
        }
    }
    // The base perk rows, tier 3 first.
    for tier in (0..3).rev() {
        if let Some(id) = tiers[tier].first() {
            if let Some(i) = c.perks.iter().position(|p| p == *id) {
                c.perks.remove(i);
                cleared.push(PERK_SLOTS[tier]);
                return true;
            }
        }
    }
    // A second tactical goes before the first (`specialgrenadecount` back to one).
    if c.tactical_count > 1 {
        c.tactical_count = 1;
        cleared.push("specialgrenadecount");
        return true;
    }
    if !c.tactical.is_empty() {
        c.tactical.clear();
        c.tactical_count = 0;
        cleared.push("specialgrenade");
        return true;
    }
    // A second lethal (Danger Close's, Tactician's second tactical) goes before the first
    // (`primarygrenadecount` back to one).
    if c.lethal_count > 1 {
        c.lethal_count = 1;
        cleared.push("primarygrenadecount");
        return true;
    }
    if !c.lethal.is_empty() {
        c.lethal.clear();
        c.lethal_count = 0;
        cleared.push("primarygrenade");
        return true;
    }
    if pop_attachment(&mut c.secondary) {
        cleared.push("secondaryattachment");
        return true;
    }
    if !c.secondary.is_empty() {
        c.secondary.clear();
        cleared.push("secondary");
        return true;
    }
    if pop_attachment(&mut c.primary) {
        cleared.push("primaryattachment");
        return true;
    }
    if !c.primary.is_empty() {
        c.primary.clear();
        cleared.push("primary");
        return true;
    }
    false
}

/// Remove the last attachment from a weapon id, if it has one.
fn pop_attachment(id: &mut String) -> bool {
    let (base, shorts) = given_parts(id);
    if shorts.is_empty() {
        return false;
    }
    let kept: Vec<&str> = shorts[..shorts.len() - 1].iter().map(String::as_str).collect();
    *id = given_weapon(&base, &kept);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(primary: &str, perks: &[&str]) -> CustomClass {
        CustomClass {
            name: "Stored".into(),
            primary: primary.into(),
            secondary: "fnp45_mp".into(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            lethal_count: 1,
            tactical_count: 1,
            melee: "knife_held_mp".into(),
            perks: perks.iter().map(|p| (*p).to_string()).collect(),
            reserve: [0, 0],
            camo: String::new(),
            reticle: String::new(),
            reticle_color: String::new(),
            knife: String::new(),
            secondary_camo: String::new(),
        }
    }

    #[test]
    fn a_held_offhand_counts_at_least_one() {
        // BO2's count is the slot's filled boxes (`cacgrenadesbutton.lua:130-230`): a class saved
        // before the counts (0) carries one of each, never the item's own start count.
        let mut c = class("an94_mp", &["Lightweight"]);
        (c.lethal_count, c.tactical_count) = (0, 0);
        let _ = repair(&mut c);
        assert_eq!((c.lethal_count, c.tactical_count), (1, 1));
        assert_eq!((c.loadout().lethal_count, c.loadout().tactical_count), (Some(1), Some(1)));
    }

    #[test]
    fn a_locked_item_is_not_held() {
        // The Peacekeeper without Revolution: listed locked, so a stored class drops it.
        let mut d = weapons::weapon("mp7_mp").clone();
        assert!(holds(&d, true));
        d.cac_locked = true;
        assert!(!holds(&d, true));
    }

    #[test]
    fn a_valid_class_is_left_alone() {
        let mut c = class("an94_mp+gl+reflex", &["Lightweight", "Toughness", "Dexterity"]);
        let before = c.clone();
        assert!(repair(&mut c).is_empty());
        assert_eq!(c, before);
    }

    #[test]
    fn an_unknown_weapon_clears_the_weapon_and_its_attachments() {
        // `ClearWeapon`: the gun and the attachments on it go together, and the class survives.
        let mut c = class("not_a_weapon_mp+reflex", &["Lightweight"]);
        let cleared = repair(&mut c);
        assert!(c.primary.is_empty(), "{}", c.primary);
        assert_eq!(cleared, vec!["primary"]);
        assert_eq!(c.lethal, "frag_grenade_mp", "the rest of the class stays");
        // A weapon of the wrong kind (a grenade in a weapon slot) goes the same way.
        let mut c = class("frag_grenade_mp", &[]);
        assert_eq!(repair(&mut c), vec!["primary"]);
        assert!(c.primary.is_empty());
    }

    #[test]
    fn a_stored_dual_wield_attachment_migrates_to_the_dw_weapon() {
        // Older profiles stored the pistol with a `dw` attachment; BO2's class gives the separate
        // `_dw_mp` weapon (G16). The migration clears nothing.
        let mut c = class("an94_mp", &[]);
        c.secondary = "fiveseven_mp+dw".into();
        assert!(repair(&mut c).is_empty());
        assert_eq!(c.secondary, "fiveseven_dw_mp");
        // The new form is already valid.
        let before = c.clone();
        assert!(repair(&mut c).is_empty());
        assert_eq!(c, before);
        // The trim drops Dual Wield as the secondary's attachment, before the pistol itself.
        let mut id = c.secondary.clone();
        assert!(pop_attachment(&mut id));
        assert_eq!(id, "fiveseven_mp");
    }

    #[test]
    fn a_weapon_of_the_other_slot_is_cleared() {
        // The SMAW is a `secondary` item: BO2's primary never holds it.
        let mut c = class("smaw_mp", &[]);
        assert_eq!(repair(&mut c), vec!["primary"]);
        assert!(c.primary.is_empty());
        // A primary item in the secondary is Overkill's, a valid class (the card is re-derived).
        let mut c = class("an94_mp", &[]);
        c.secondary = "mk48_mp".into();
        assert!(repair(&mut c).is_empty());
    }

    #[test]
    fn an_attachment_the_weapon_does_not_offer_is_dropped() {
        // `primaryattachment3` and friends are checked against the weapon's own list in BO2.
        let mut c = class("an94_mp+gl+reflex+grip", &[]);
        let cleared = repair(&mut c);
        assert_eq!(cleared, vec!["primaryattachment"]);
        assert_eq!(c.primary, "an94_mp+gl+reflex");
        // The secondary carries its own list.
        let mut c = class("an94_mp", &[]);
        c.secondary = "fnp45_mp+gl".into();
        assert_eq!(repair(&mut c), vec!["secondaryattachment"]);
        assert_eq!(c.secondary, "fnp45_mp");
    }

    /// `CheckItems`' order over `loadoutSlotNames`: the primary, the secondary, then the grenades,
    /// then the perk slots — a repair report reads in that order.
    #[test]
    fn an_offhand_in_the_wrong_slot_is_cleared() {
        // (A tactical in the lethal slot is Tactician's, not a wrong slot: see below.)
        let mut c = class("an94_mp", &[]);
        c.lethal = "fnp45_mp".into();
        c.tactical = "frag_grenade_mp".into();
        let cleared = repair(&mut c);
        assert_eq!(cleared, vec!["primarygrenade", "specialgrenade"], "CheckItems' slot order");
        assert!(c.lethal.is_empty() && c.tactical.is_empty());
        // Unknown ids clear too (and are logged by the catalog).
        let mut c = class("an94_mp", &[]);
        c.lethal = "nope_mp".into();
        assert_eq!(repair(&mut c), vec!["primarygrenade"]);
    }

    /// `checkclasses.lua:677-700`: a tactical in the lethal slot needs Tactician and a lethal count
    /// of two Danger Close; the cards are re-derived from them, so both stay (and are charged). A
    /// count is clamped to `maxGrenades`, means nothing without a lethal item, and its second
    /// lethal is the first thing over-budget trimming takes from the slot.
    #[test]
    fn tactician_and_danger_close_classes_keep_their_grenades() {
        let mut c = class("an94_mp", &[]);
        c.lethal = "concussion_grenade_mp".into();
        c.lethal_count = 2;
        assert_eq!(repair(&mut c), Cleared::new());
        assert_eq!((c.lethal.as_str(), c.lethal_count), ("concussion_grenade_mp", 2), "Tactician's second tactical stays");
        assert!(Draft::from_custom(&c).wildcards.contains(&crate::classmenu::Wildcard::Tactician));
        let mut c = class("an94_mp", &[]);
        c.lethal_count = 5;
        assert_eq!(repair(&mut c), Cleared::new());
        assert_eq!(c.lethal_count, crate::classes::max_grenades() as u8);
        let d = Draft::from_custom(&c);
        assert!(d.wildcards.contains(&crate::classmenu::Wildcard::DangerClose));
        // Trimming walks the slots backwards: the tactical, then the second lethal, then the lethal.
        let mut cleared = Cleared::new();
        while drop_one(&mut c, &mut cleared) && cleared.len() < 3 {}
        assert_eq!(cleared, vec!["specialgrenade", "primarygrenadecount", "primarygrenade"]);
    }

    /// The placed equipment is the slot's own item (statstable column 13), not a stray class: a
    /// stored claymore and Trophy System stay. Swapped, the claymore leaves the tactical slot like
    /// any other wrong-slot item, while the Trophy System in the lethal slot is Tactician's
    /// (`checkclasses.lua:677-686`: a `specialgrenade` item there needs the card, which the class
    /// re-derives) and stays.
    #[test]
    fn placed_equipment_stays_in_its_statstable_slot() {
        let mut c = class("an94_mp", &[]);
        c.lethal = "claymore_mp".into();
        c.tactical = "trophy_system_mp".into();
        assert_eq!(repair(&mut c), Cleared::new());
        assert_eq!((c.lethal.as_str(), c.tactical.as_str()), ("claymore_mp", "trophy_system_mp"));
        c.lethal = "trophy_system_mp".into();
        c.tactical = "claymore_mp".into();
        assert_eq!(repair(&mut c), vec!["specialgrenade"]);
        assert_eq!(c.lethal, "trophy_system_mp");
    }

    #[test]
    fn a_tier_keeps_two_perks_and_unknowns_go() {
        let mut c = class("an94_mp", &["Lightweight", "Hardline", "Ghost", "NotAPerk", "Dexterity"]);
        let cleared = repair(&mut c);
        assert_eq!(c.perks, vec!["Lightweight", "Hardline", "Dexterity"]);
        assert!(cleared.contains(&"specialty"), "{cleared:?}");
    }

    /// The over-budget precedence: `CoD.CheckClasses.CheckItems` (`checkclasses.lua`) visits the
    /// slots in `CoD.CACUtility.loadoutSlotNames` order, so the trim drops them in reverse —
    /// `specialty6` down to `specialty1`, the offhands, then the weapons last.
    #[test]
    fn an_over_budget_class_is_trimmed_last_slot_first() {
        // 1 primary + 2 attachments + 1 secondary + 2 offhands + 6 perks + 3 Greed cards is 15
        // points: five over, and exactly at the card cap. The Greed rows are `specialty6` and
        // `specialty5`, and the trim drops them before anything the check pass walks earlier.
        let mut c = class(
            "an94_mp+gl+reflex",
            &["Lightweight", "FlakJacket", "Toughness", "Scavenger", "Dexterity", "ExtremeConditioning"],
        );
        let d = Draft::from_custom(&c);
        assert!(d.spent() > PICK_10, "the seed is over budget: {}", d.spent());
        assert_eq!(d.wildcards.len(), max_bonus_cards(), "and at the card cap: {:?}", d.wildcards);
        let cleared = repair(&mut c);
        assert!(Draft::from_custom(&c).spent() <= PICK_10);
        assert_eq!(&cleared[..2], ["specialty6", "specialty5"], "{cleared:?}");
        // The weapon is the last thing standing: the primary and its attachments survive.
        assert_eq!(c.primary, "an94_mp+gl+reflex");
        assert!(c.perks.len() <= 4);
    }

    /// `maxBonusCards` (cacutility.lua) against the cards `CheckItems`/`GetEquippedBonusCards`
    /// gate (`bonuscard_perk_1_greed` … `bonuscard_overkill`).
    #[test]
    fn a_fourth_bonus_card_is_trimmed() {
        // Three Greed picks, both Gunfighters and Overkill: six cards. The cap is three, and the
        // trim order drops the Greed rows first.
        let mut c = class(
            "an94_mp+gl+reflex+grip",
            &["Lightweight", "Hardline", "Toughness", "Scavenger", "Dexterity", "ExtremeConditioning"],
        );
        // A rifle in the secondary slot is Overkill's second primary.
        c.secondary = "an94_mp+gl".into();
        let d = Draft::from_custom(&c);
        assert!(d.wildcards.len() > max_bonus_cards(), "{:?}", d.wildcards);
        let cleared = repair(&mut c);
        let after = Draft::from_custom(&c);
        assert!(after.wildcards.len() <= max_bonus_cards(), "{:?} after {cleared:?}", after.wildcards);
        assert!(Draft::from_custom(&c).spent() <= PICK_10);
        assert!(!cleared.is_empty());
    }

    /// Each weapon's camo is checked against its own weapon's list: the secondary's
    /// `secondarycamo` against the sidearm's `CamoList`, not the primary's; a camo whose weapon
    /// went (`ClearWeapon`) goes with it.
    #[test]
    fn each_camo_is_checked_against_its_own_weapons_list() {
        let lists = |id: &str| -> Option<Vec<String>> {
            let l: &[&str] = match id {
                "an94_mp" => &["camo_none", "camo_gold", "camo_dragon"],
                "fnp45_mp" => &["camo_none", "camo_gold"],
                "knife_held_mp" => &["camo_none", "camo_diamond"],
                "svu_mp" => &[],
                _ => return None,
            };
            Some(l.iter().map(|s| (*s).to_string()).collect())
        };
        let mut c = class("an94_mp+reflex", &[]);
        c.camo = "camo_dragon".into();
        c.secondary_camo = "camo_gold".into();
        c.knife = "camo_diamond".into();
        let before = c.clone();
        let mut cleared = Vec::new();
        repair_camos(&mut c, lists, &mut cleared);
        assert!(cleared.is_empty(), "{cleared:?}");
        assert_eq!(c, before);
        // The primary's DLC camo is not in the sidearm's list: the secondary loses it, the primary
        // keeps its own.
        c.secondary_camo = "camo_dragon".into();
        c.knife = "camo_gold".into();
        repair_camos(&mut c, lists, &mut cleared);
        assert_eq!(cleared, vec!["secondarycamo", "knifecamo"]);
        assert_eq!((c.camo.as_str(), c.secondary_camo.as_str(), c.knife.as_str()), ("camo_dragon", "", ""));
        // No secondary: its camo goes with it. A weapon without a converted list keeps its camo.
        let mut c = class("svu_mp", &[]);
        c.camo = "camo_anything".into();
        c.secondary.clear();
        c.secondary_camo = "camo_gold".into();
        let mut cleared = Vec::new();
        repair_camos(&mut c, lists, &mut cleared);
        assert_eq!(cleared, vec!["secondarycamo"]);
        assert_eq!(c.camo, "camo_anything");
        // The full pass: an unknown secondary is cleared, and so is its camo.
        let mut c = class("an94_mp", &[]);
        c.secondary = "nope_mp".into();
        c.secondary_camo = "camo_gold".into();
        let cleared = repair(&mut c);
        assert_eq!(cleared, vec!["secondary", "secondarycamo"]);
        assert!(c.secondary_camo.is_empty());
    }

    #[test]
    fn an_empty_class_is_already_valid() {
        // Empty slots mean "inherit the base kit", not "invalid": a fresh entry repairs to itself.
        let mut c = CustomClass { name: "Fresh".into(), ..CustomClass::default() };
        assert!(repair(&mut c).is_empty());
        assert_eq!(c.name, "Fresh", "repair never drops the class itself");
    }
}
