//! The command bar's class buttons: which classes the bar offers, on which page, with which
//! labels and which kit icons.
//!
//! The rows are the ACTIVE class set's, in BO2's order (`cacchooseclass.lua`): Take Control, the
//! five default classes (shared by every set), then that set's five custom slots — ten class
//! buttons plus Take Control and Back, one page. The command bar builds the command set from them
//! (host/zhbo/ZhboControlBar.cpp):
//! page one carries Take Control, the classes and Back (there is no second page any more). This
//! module is the single source for that composition: the same registry and profile the class pick
//! screen reads (`classui::pick_list`), so the bar and the screen can never disagree about which
//! classes exist. The engine's command set has 14 user-facing slots (`MAX_COMMANDS_PER_SET` is 18,
//! 14 UI), and Take Control/Back take one each, leaving twelve class slots on page one — one more
//! than a full set needs.
//!
//! [`BUTTONS`] stays at the ABI's fifteen `ZhboClassButton` rows (the bridge iterates all fifteen
//! and skips the absent ones); only [`ROWS`] carry a class.

use crate::hud::sprites;
use crate::{classes, profile};

/// The five default classes, shared by every class set (BO2's `maxDefaultClass`).
pub const PRESETS: usize = classes::MAX_CUSTOM_CLASSES;
/// The active set's custom slots (BO2's `maxCustomClass`).
pub const CUSTOMS: usize = profile::MAX_CLASSES;
/// The buttons that carry a class: the five defaults then the active set's five custom slots.
pub const ROWS: usize = PRESETS + CUSTOMS;
/// The command bar's `ZhboClassButton` capacity, which the bridge iterates (unchanged ABI); the
/// rows past [`ROWS`] are absent, and the bridge skips them.
pub const BUTTONS: usize = 15;

/// Class slots on the first page: 14 UI slots less Take Control (slot 0) and Back (slot 13). A
/// full set (ten classes) fits, so the bar is one page.
pub const PAGE1_CLASS_SLOTS: usize = 12;

/// One class button's kit: the names rendered over it, and its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassIcons {
    /// The class's display name (the preset's, or the player's custom name).
    pub name: String,
    /// The primary weapon's converted HUD icon (`ZHBO_BO2_Icon_<id>`).
    pub primary: String,
    /// The secondary's icon.
    pub secondary: String,
    /// The lethal's icon.
    pub lethal: String,
    /// The tactical's icon.
    pub tactical: String,
    /// The perk icons, in pick order (BO2's three tiers, plus any Greed picks).
    pub perks: Vec<&'static str>,
}

/// The per-weapon icon the converter emits (`ZHBO_BO2_Icon_<id>`); an attached variant uses its
/// base gun's icon (`an94_mp+reflex` -> `ZHBO_BO2_Icon_an94_mp`), exactly like the class screen.
pub(crate) fn weapon_icon(id: &str) -> String {
    let base = id.split('+').next().unwrap_or(id);
    format!("ZHBO_BO2_Icon_{base}")
}

/// All [`BUTTONS`] rows, in button order: the five default classes first, then the ACTIVE class
/// set's customs. An absent class (a registry with fewer than five presets, or a set with fewer
/// than five classes) is `None`, and so are the rows past [`ROWS`]. The registries are snapshotted
/// once, so one call describes one consistent list.
pub fn icons() -> Vec<Option<ClassIcons>> {
    let presets = classes::get();
    let customs = profile::classes();
    let mut out: Vec<Option<ClassIcons>> = Vec::with_capacity(BUTTONS);
    for c in presets.iter().take(PRESETS) {
        let kit = classes::loadout_of(c);
        out.push(Some(ClassIcons {
            name: c.name.clone(),
            primary: weapon_icon(&kit.primary),
            secondary: weapon_icon(&kit.secondary),
            lethal: weapon_icon(&kit.lethal),
            tactical: weapon_icon(&kit.tactical),
            perks: kit.perks.iter().map(|p| sprites::perk_icon(*p)).collect(),
        }));
    }
    while out.len() < PRESETS {
        out.push(None);
    }
    for (i, c) in customs.iter().take(CUSTOMS).enumerate() {
        // The kit the sim applies: the entry's spec over the base kit, so an unset slot shows the
        // gun/equipment it actually inherits rather than an empty icon.
        let kit = c.loadout();
        out.push(Some(ClassIcons {
            // A hand-edited profile can carry a blank name; the button still needs a label.
            name: if c.name.trim().is_empty() { format!("Custom Class {}", i + 1) } else { c.name.clone() },
            primary: weapon_icon(&kit.primary),
            secondary: weapon_icon(&kit.secondary),
            lethal: weapon_icon(&kit.lethal),
            tactical: weapon_icon(&kit.tactical),
            perks: kit.perks.iter().map(|p| sprites::perk_icon(*p)).collect(),
        }));
    }
    out.resize(BUTTONS, None);
    out
}

/// The pages' class button indices for a registry of `presets` presets and `customs` custom
/// classes. A class set is at most ten classes, and page one holds up to twelve (after Take
/// Control and before Back), so the second page can never fill; the two-element shape stays
/// because [`button_page_for`] and its callers read a (page, slot) pair.
pub fn pages_for(presets: usize, customs: usize) -> [Vec<usize>; 2] {
    let presets = presets.min(PRESETS);
    let customs = customs.min(CUSTOMS);
    let mut all: Vec<usize> = (0..presets).collect();
    all.extend((PRESETS..PRESETS + customs).collect::<Vec<_>>());
    let page2 = all.split_off(all.len().min(PAGE1_CLASS_SLOTS));
    [all, page2]
}

/// A button's `(page, slot)` in the command set, or `None` when the button carries no class.
/// Page-one slots start at 1 (0 is Take Control); page-two slots start at 0.
pub fn button_page_for(button: usize, presets: usize, customs: usize) -> Option<(u8, u8)> {
    let [page1, page2] = pages_for(presets, customs);
    if let Some(i) = page1.iter().position(|b| *b == button) {
        return Some((1, 1 + i as u8));
    }
    page2.iter().position(|b| *b == button).map(|i| (2, i as u8))
}

/// The button labels, in button order. A duplicate (a custom class named like a preset, or two
/// customs whose names differ only by case) gets a numeric suffix so each button reads uniquely;
/// a blank name falls back to `Custom Class N`.
pub fn labels() -> Vec<Option<String>> {
    let icons = icons();
    let mut out: Vec<Option<String>> = Vec::with_capacity(BUTTONS);
    for c in &icons {
        let Some(c) = c else {
            out.push(None);
            continue;
        };
        let base = c.name.trim().replace(['\n', '\r'], " ");
        let mut label = base.clone();
        let mut n = 1;
        while out.iter().flatten().any(|l| l.eq_ignore_ascii_case(&label)) {
            n += 1;
            label = format!("{base} {n}");
        }
        out.push(Some(label));
    }
    out
}

/// A custom button's pick payload: its wire words (the same the pick screen sends). Shipped
/// presets pick by registry index and carry no payload, so this is empty for them.
pub fn custom_payload(button: usize) -> Vec<u32> {
    let index = button.checked_sub(PRESETS);
    match index {
        Some(i) => profile::classes().get(i).map(|c| c.to_wire_words()).unwrap_or_default(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::CustomClass;

    /// Hold both globals these tests read (the profile's customs and the class registry's presets).
    /// Always in this order — profile, then registry — so a test thread in `classes`/`profile` can
    /// never hold the other one and deadlock. `let _g = lock()` keeps both guards to scope end.
    fn lock() -> (std::sync::MutexGuard<'static, ()>, std::sync::MutexGuard<'static, ()>) {
        (crate::profile::lock(), crate::classes::lock())
    }

    fn custom(name: &str, primary: &str) -> CustomClass {
        CustomClass { name: name.into(), primary: primary.into(), ..CustomClass::default() }
    }

    #[test]
    fn a_full_class_set_fits_one_page() {
        // Five defaults + five customs is ten rows, and page one holds twelve class slots.
        let [p1, p2] = pages_for(PRESETS, 0);
        assert_eq!(p1, vec![0, 1, 2, 3, 4]);
        assert!(p2.is_empty(), "no customs: one page");

        let [p1, p2] = pages_for(PRESETS, 1);
        assert_eq!(p1, vec![0, 1, 2, 3, 4, 5]);
        assert!(p2.is_empty());

        let [p1, p2] = pages_for(PRESETS, CUSTOMS);
        assert_eq!(p1, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "a full set is one page");
        assert!(p2.is_empty(), "there is never a second page any more");
        // A registry with fewer presets leaves the custom indices untouched.
        let [p1, p2] = pages_for(2, CUSTOMS);
        assert_eq!(&p1[..2], [0, 1]);
        assert_eq!(p1[2], PRESETS, "customs keep their own button indices");
        assert!(p2.is_empty());
        assert_eq!(pages_for(0, 0), [Vec::new(), Vec::new()]);
    }

    #[test]
    fn three_customs_still_fit_one_page() {
        const { assert!(ROWS <= PAGE1_CLASS_SLOTS, "a full active set must fit page one's class slots") };
        let [p1, p2] = pages_for(PRESETS, 3);
        assert_eq!(p1, vec![0, 1, 2, 3, 4, 5, 6, 7], "the five presets, then the three customs");
        assert!(p2.is_empty(), "three customs never spill onto a second page");
        assert_eq!(button_page_for(PRESETS + 2, PRESETS, 3), Some((1, 8)), "the last custom is the last class slot");
        assert_eq!(button_page_for(PRESETS + 3, PRESETS, 3), None, "the first free custom slot carries no class");
    }

    #[test]
    fn the_rows_past_the_active_sets_classes_are_absent() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("One", "an94_mp"));
        crate::profile::save(custom("Two", "mp7_mp"));
        crate::profile::save(custom("Three", "svu_mp"));
        let all = icons();
        assert_eq!(all.len(), BUTTONS, "the bridge always gets the ABI's fifteen rows");
        assert!(all[PRESETS..PRESETS + 3].iter().all(|c| c.is_some()), "three customs, three rows");
        // The two free custom slots and the ABI's padding rows come back present=0: the bridge
        // skips them when it builds the set and when it draws kits over the visible buttons.
        assert!(all[PRESETS + 3..].iter().all(|c| c.is_none()));
        assert!(labels()[PRESETS + 3..].iter().all(|l| l.is_none()));
        for b in PRESETS + 3..BUTTONS {
            assert_eq!(button_page_for(b, PRESETS, 3), None, "button {b} is absent");
            assert!(custom_payload(b).is_empty(), "button {b} has no payload");
        }
        crate::profile::reset();
    }

    #[test]
    fn switching_the_active_set_rederives_labels_icons_and_payloads() {
        let _g = lock();
        crate::profile::reset();
        // The second set (index 1) carries three customs; the first (index 0) five different
        // ones. The bar describes whichever is active, and re-derives on every call (the bridge
        // rebuilds at game start, when the shell may have switched sets).
        assert!(crate::profile::set_active(1).is_some());
        for (name, primary) in [("Alpha", "an94_mp"), ("Bravo", "mp7_mp"), ("Charlie", "svu_mp")] {
            crate::profile::save(custom(name, primary));
        }
        let set_two = icons();
        assert_eq!(set_two[PRESETS].as_ref().map(|c| c.name.as_str()), Some("Alpha"));
        assert_eq!(set_two[PRESETS].as_ref().unwrap().primary, "ZHBO_BO2_Icon_an94_mp");
        assert_eq!(labels()[PRESETS], Some("Alpha".to_string()));
        assert_eq!(custom_payload(PRESETS), custom("Alpha", "an94_mp").to_wire_words());
        assert!(set_two[PRESETS + 3..].iter().all(|c| c.is_none()), "set 2 carries three classes");

        assert!(crate::profile::set_active(0).is_some());
        for i in 1..=CUSTOMS {
            crate::profile::save(custom(&format!("Set One {i}"), "mp7_mp"));
        }
        assert!(crate::profile::set_active(1).is_some());
        let back_to_two = icons();
        assert_eq!(back_to_two[PRESETS].as_ref().map(|c| c.name.as_str()), Some("Alpha"));
        assert_eq!(labels()[PRESETS], Some("Alpha".to_string()));
        assert_eq!(custom_payload(PRESETS), custom("Alpha", "an94_mp").to_wire_words());
        assert!(back_to_two[PRESETS + 3..].iter().all(|c| c.is_none()), "the other set's five classes are not on the bar");

        assert!(crate::profile::set_active(0).is_some());
        let set_one = icons();
        assert_eq!(set_one[PRESETS].as_ref().map(|c| c.name.as_str()), Some("Set One 1"));
        assert_eq!(set_one[PRESETS + CUSTOMS - 1].as_ref().map(|c| c.name.clone()), Some(format!("Set One {CUSTOMS}")));
        assert_eq!(labels()[PRESETS + CUSTOMS - 1], Some(format!("Set One {CUSTOMS}")));
        assert!(set_one[PRESETS + CUSTOMS..].iter().all(|c| c.is_none()));
        crate::profile::reset();
    }

    #[test]
    fn button_pages_and_slots_match_the_command_set_layout() {
        // Page one: Take Control is slot 0, so the first class button lands in slot 1 and a full
        // set's last custom button in slot 10; Back sits in slot 13.
        assert_eq!(button_page_for(0, PRESETS, CUSTOMS), Some((1, 1)));
        assert_eq!(button_page_for(4, PRESETS, CUSTOMS), Some((1, 5)));
        assert_eq!(button_page_for(5, PRESETS, CUSTOMS), Some((1, 6)));
        assert_eq!(button_page_for(ROWS - 1, PRESETS, CUSTOMS), Some((1, 10)));
        // With no customs the custom buttons carry no class.
        assert_eq!(button_page_for(5, PRESETS, 0), None);
        assert_eq!(button_page_for(11, PRESETS, 0), None);
        // The padded ABI rows carry no class either.
        assert_eq!(button_page_for(ROWS, PRESETS, CUSTOMS), None);
        assert_eq!(button_page_for(BUTTONS - 1, PRESETS, CUSTOMS), None);
    }

    #[test]
    fn icons_come_from_the_active_set_and_the_shared_defaults() {
        let _g = lock();
        crate::profile::reset();
        let presets = classes::get();
        let Some(first) = presets.first() else { return }; // another test swapped the registry out
        let all_icons = icons();
        assert_eq!(all_icons.len(), BUTTONS, "the ABI's fifteen rows");
        assert_eq!(all_icons[0].as_ref().map(|c| c.name.as_str()), Some(first.name.as_str()));
        // The first default class is BO2's Assault: the MTAR, no attachments.
        let assault = classes::loadout_of(first);
        assert_eq!(all_icons[0].as_ref().unwrap().primary, weapon_icon(&assault.primary));
        assert_eq!(all_icons[0].as_ref().unwrap().primary, "ZHBO_BO2_Icon_tar21_mp");
        assert_eq!(all_icons[0].as_ref().unwrap().perks.len(), assault.perks.len());
        // No customs: the custom buttons and the padded rows carry no class.
        assert!(all_icons[PRESETS..].iter().all(|c| c.is_none()));

        crate::profile::save(custom("My Sniper", "dsr50_mp+acog"));
        let icons = icons();
        let sniper = icons[PRESETS].as_ref().expect("the saved class is on the first custom button");
        assert_eq!(sniper.name, "My Sniper");
        assert_eq!(sniper.primary, "ZHBO_BO2_Icon_dsr50_mp", "the base gun's icon, not the attachment's");
        assert_eq!(sniper.secondary, "ZHBO_BO2_Icon_fnp45_mp");
        assert_eq!(sniper.lethal, "ZHBO_BO2_Icon_frag_grenade_mp");
        assert_eq!(sniper.tactical, "ZHBO_BO2_Icon_concussion_grenade_mp");
        assert_eq!(sniper.perks, vec![sprites::perk_icon(crate::weapons::Perk::LIGHTWEIGHT), sprites::perk_icon(crate::weapons::Perk::FAST_HANDS), sprites::perk_icon(crate::weapons::Perk::DEXTERITY)]);
        assert!(icons[PRESETS + 1..].iter().all(|c| c.is_none()));
        crate::profile::reset();
    }

    #[test]
    fn the_bar_shows_only_the_active_sets_customs() {
        let _g = lock();
        crate::profile::reset();
        crate::profile::save(custom("Set One", "an94_mp"));
        assert!(crate::profile::set_active(1).is_some());
        crate::profile::save(custom("Set Two", "dsr50_mp"));
        let icons = icons();
        assert_eq!(icons[PRESETS].as_ref().map(|c| c.name.as_str()), Some("Set Two"));
        assert!(icons[PRESETS + 1..].iter().all(|c| c.is_none()), "set 1's class is not on the bar");
        // Switching back swaps the rows: the bar always describes the set a match would load.
        assert!(crate::profile::set_active(0).is_some());
        assert_eq!(self::icons()[PRESETS].as_ref().map(|c| c.name.as_str()), Some("Set One"));
        crate::profile::reset();
    }

    #[test]
    fn labels_disambiguate_duplicates_and_name_blanks() {
        let _g = lock();
        crate::profile::reset();
        let preset_labels = labels();
        assert!(preset_labels[PRESETS..].iter().all(|l| l.is_none()), "no customs, no custom labels");
        let Some(first_preset) = icons()[0].as_ref().map(|c| c.name.clone()) else { return }; // registry swapped by another test
        assert!(preset_labels[0].is_some());
        crate::profile::save(custom(&first_preset, "an94_mp"));
        // "Sniper" is one of BO2's five default classes now, so that name is taken too.
        crate::profile::save(custom("Sniper", "an94_mp"));
        let custom_labels = labels();
        assert_eq!(custom_labels[PRESETS], Some(format!("{first_preset} 2")), "a name like a default class gets a suffix");
        assert_eq!(custom_labels[PRESETS + 1], Some("Sniper 2".to_string()));

        // A blank name (a hand-edited profile) still gets a readable label.
        crate::profile::reset();
        crate::profile::load("   \nprimary=an94_mp\n");
        let blank_labels = labels();
        assert_eq!(blank_labels[PRESETS], Some("Custom Class 1".to_string()));
        crate::profile::reset();
    }

    #[test]
    fn a_custom_buttons_payload_is_its_wire_words() {
        let _g = lock();
        crate::profile::reset();
        let c = custom("Sniper", "dsr50_mp+acog");
        crate::profile::save(c.clone());
        assert!(custom_payload(0).is_empty(), "a preset picks by index");
        assert_eq!(custom_payload(PRESETS), c.to_wire_words());
        assert!(custom_payload(PRESETS + 1).is_empty(), "an empty custom slot has no payload");
        assert_eq!(CustomClass::from_wire_words(&custom_payload(PRESETS)), Some(c));
        crate::profile::reset();
    }
}
