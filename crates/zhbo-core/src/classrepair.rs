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
//! * **Grenade counts.** `CheckItems` clamps a grenade count to `maxGrenades` (2) and clears the
//!   item at 0. A class here carries one lethal and one tactical id, and the carried count is the
//!   weapon table's `Clip` (the concussion's is 2), so there is no count to repair.
//!
//! Unknown ids are cleared, not substituted: `catalog::weapon_or_none` drops them loudly (the same
//! rule the arsenal uses), so a corrupt profile shows up in the log instead of quietly turning into
//! some default kit.

use crate::attachments::{split_id, variant_id};
use crate::classes::{CustomClass, MAX_ATTACHMENTS, MAX_BONUS_CARDS, MAX_SPECIALTY_SLOTS};
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
    repair_offhand(c, WeaponClass::Lethal, true, &mut cleared);
    repair_offhand(c, WeaponClass::Tactical, false, &mut cleared);
    if !c.melee.is_empty() && !resolves_as(&c.melee, WeaponClass::Melee) {
        c.melee.clear();
        cleared.push("melee");
    }
    repair_perks(c, &mut cleared);
    trim(c, &mut cleared);
    cleared
}

/// A weapon slot: an unknown id or a non-gun clears the weapon and its attachments together
/// (`ClearWeapon`); attachments the weapon does not offer are dropped (the per-weapon
/// `GetNumAttachments` check), and so are the ones past the slot's cap (`maxAttachments`, plus the
/// Gunfighter card's extra on the primary). An empty slot means "inherit the base kit" and stays
/// empty.
fn repair_weapon(c: &mut CustomClass, primary: bool, cleared: &mut Cleared) {
    let (slot, id) = if primary { ("primary", &mut c.primary) } else { ("secondary", &mut c.secondary) };
    if id.is_empty() {
        return;
    }
    let (base, shorts) = split_id(id);
    let Some(def) = weapons::catalog::weapon_or_none(base) else {
        id.clear();
        cleared.push(slot);
        return;
    };
    if !def.class.is_gun() {
        id.clear();
        cleared.push(slot);
        return;
    }
    let max = if primary { MAX_ATTACHMENTS + 1 } else { MAX_ATTACHMENTS };
    let offered: Vec<&str> = shorts.iter().copied().filter(|s| def.allowed.iter().any(|(a, _)| a == s)).take(max).collect();
    if offered.len() != shorts.len() {
        cleared.push(if primary { "primaryattachment" } else { "secondaryattachment" });
        *id = variant_id(base, &offered);
    }
}

/// An offhand slot the class cannot hold is cleared (`ClearSlot`): the lethal slot takes the lethal
/// class, the tactical slot the tactical class, exactly the two slots `loadoutSlotNames` names.
fn repair_offhand(c: &mut CustomClass, class: WeaponClass, lethal: bool, cleared: &mut Cleared) {
    let (slot, id) = if lethal { ("primarygrenade", &mut c.lethal) } else { ("specialgrenade", &mut c.tactical) };
    if id.is_empty() || resolves_as(id, class) {
        return;
    }
    id.clear();
    cleared.push(slot);
}

/// Whether the id resolves to a weapon of `class`.
fn resolves_as(id: &str, class: WeaponClass) -> bool {
    weapons::catalog::weapon_or_none(id).is_some_and(|d| d.class == class)
}

/// The perks: unknown ids go (they would silently vanish in `Loadout::with_spec` anyway), a repeat
/// goes, and a tier keeps at most two picks (`maxPerksInEachCategory`: BO2's two rows per tier).
/// Six slots in all (`maxSpecialtySlots`), which three tiers of two reach exactly.
fn repair_perks(c: &mut CustomClass, cleared: &mut Cleared) {
    let mut kept: Vec<Perk> = Vec::with_capacity(MAX_SPECIALTY_SLOTS);
    let mut tiers = [0usize; 3];
    for id in &c.perks {
        let Some(p) = Perk::by_id(id) else {
            cleared.push("specialty");
            continue;
        };
        let tier = (p.def().tier as usize).clamp(1, 3) - 1;
        tiers[tier] += 1;
        if kept.contains(&p) || tiers[tier] > 2 || kept.len() >= MAX_SPECIALTY_SLOTS {
            cleared.push(if tiers[tier] > 2 { EXTRA_PERK_SLOTS[tier] } else { "specialty" });
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
        if d.spent() <= PICK_10 && d.wildcards.len() <= MAX_BONUS_CARDS {
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
    if !c.tactical.is_empty() {
        c.tactical.clear();
        cleared.push("specialgrenade");
        return true;
    }
    if !c.lethal.is_empty() {
        c.lethal.clear();
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
    let (base, shorts) = split_id(id);
    if shorts.is_empty() {
        return false;
    }
    let kept = &shorts[..shorts.len() - 1];
    *id = variant_id(base, kept);
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
            melee: "knife_held_mp".into(),
            perks: perks.iter().map(|p| (*p).to_string()).collect(),
            reserve: [0, 0],
        }
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
        let mut c = class("an94_mp", &[]);
        c.lethal = "concussion_grenade_mp".into();
        c.tactical = "frag_grenade_mp".into();
        let cleared = repair(&mut c);
        assert_eq!(cleared, vec!["primarygrenade", "specialgrenade"], "CheckItems' slot order");
        assert!(c.lethal.is_empty() && c.tactical.is_empty());
        // Unknown ids clear too (and are logged by the catalog).
        let mut c = class("an94_mp", &[]);
        c.lethal = "nope_mp".into();
        assert_eq!(repair(&mut c), vec!["primarygrenade"]);
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
        assert_eq!(d.wildcards.len(), MAX_BONUS_CARDS, "and at the card cap: {:?}", d.wildcards);
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
        assert!(d.wildcards.len() > MAX_BONUS_CARDS, "{:?}", d.wildcards);
        let cleared = repair(&mut c);
        let after = Draft::from_custom(&c);
        assert!(after.wildcards.len() <= MAX_BONUS_CARDS, "{:?} after {cleared:?}", after.wildcards);
        assert!(Draft::from_custom(&c).spent() <= PICK_10);
        assert!(!cleared.is_empty());
    }

    #[test]
    fn an_empty_class_is_already_valid() {
        // Empty slots mean "inherit the base kit", not "invalid": a fresh entry repairs to itself.
        let mut c = CustomClass { name: "Fresh".into(), ..CustomClass::default() };
        assert!(repair(&mut c).is_empty());
        assert_eq!(c.name, "Fresh", "repair never drops the class itself");
    }
}
