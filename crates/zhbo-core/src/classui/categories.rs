//! The weapon pickers' categories: BO2's names and column order, and walking them.

use super::*;

/// One category of a weapon slot's list: BO2's display name and the option indices it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    pub class: WeaponClass,
    pub name: &'static str,
    /// The option indices (into `Editor::options`) of the guns in this category, in list order.
    pub options: Vec<usize>,
}

/// The primary slot's category order, BO2's (`c-primary/001`: SUBMACHINE GUNS, ASSAULT RIFLES,
/// SHOTGUNS, LIGHT MACHINE GUNS, SNIPER RIFLES, SPECIALS).
pub(super) const PRIMARY_CATEGORY_ORDER: [WeaponClass; 7] = [
    WeaponClass::Smg,
    WeaponClass::Rifle,
    WeaponClass::Shotgun,
    WeaponClass::Lmg,
    WeaponClass::Sniper,
    WeaponClass::Special,
    WeaponClass::Launcher,
];

/// The secondary slot's category order, BO2's (`d-attach/037`: PISTOLS, LAUNCHERS, SPECIALS).
pub(super) const SECONDARY_CATEGORY_ORDER: [WeaponClass; 3] = [WeaponClass::Pistol, WeaponClass::Launcher, WeaponClass::Special];

/// BO2's display name for a weapon category.
pub fn category_name(class: WeaponClass) -> &'static str {
    match class {
        WeaponClass::Smg => "SUBMACHINE GUNS",
        WeaponClass::Rifle => "ASSAULT RIFLES",
        WeaponClass::Shotgun => "SHOTGUNS",
        WeaponClass::Lmg => "LIGHT MACHINE GUNS",
        WeaponClass::Sniper => "SNIPER RIFLES",
        WeaponClass::Special => "SPECIALS",
        WeaponClass::Launcher => "LAUNCHERS",
        WeaponClass::Pistol => "PISTOLS",
        _ => "OTHER",
    }
}

/// The category order of a weapon slot: its own table first, then the other slot's (the secondary
/// gains the primary classes under Overkill).
pub(super) fn category_order(slot: Slot) -> Vec<WeaponClass> {
    let mut out: Vec<WeaponClass> = match slot {
        Slot::Secondary => SECONDARY_CATEGORY_ORDER.to_vec(),
        _ => PRIMARY_CATEGORY_ORDER.to_vec(),
    };
    for c in PRIMARY_CATEGORY_ORDER.into_iter().chain(SECONDARY_CATEGORY_ORDER) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The weapon slot's categories, in BO2's column order. Empty for a slot that does not list guns.
pub fn weapon_categories(editor: &Editor) -> Vec<Category> {
    if !matches!(editor.slot, Slot::Primary | Slot::Secondary) {
        return Vec::new();
    }
    let order = category_order(editor.slot);
    let mut out: Vec<Category> = Vec::new();
    for i in 0..editor.options().len() {
        let Some(def) = editor.option_weapon(i) else { continue };
        match out.iter_mut().find(|c| c.class == def.class) {
            Some(c) => c.options.push(i),
            None => out.push(Category { class: def.class, name: category_name(def.class), options: vec![i] }),
        }
    }
    out.sort_by_key(|c| order.iter().position(|x| *x == c.class).unwrap_or(usize::MAX));
    out
}

/// Which category holds the highlighted option (0 when it holds none).
pub fn category_of(cats: &[Category], option: usize) -> usize {
    cats.iter().position(|c| c.options.contains(&option)).unwrap_or(0)
}

/// The category column as BO2 draws it (`s10-snipercat`): the active category on top, then the rest
/// in their own order, wrapping (`SNIPER RIFLES` first, `SPECIALS` … after it).
pub fn category_column(cats: &[Category], active: usize) -> Vec<usize> {
    if cats.is_empty() {
        return Vec::new();
    }
    let active = active.min(cats.len() - 1);
    (0..cats.len()).map(|i| (active + i) % cats.len()).collect()
}

/// Move the highlight to the neighbouring category: BO2's Up/Down (and the wheel) on the card
/// carousel list (`cardcarousellist.lua:723-772`, `ScrollList`), wrapping past either end. Each
/// category is its own `CardCarousel` that keeps its card while the menu is open, so the move lands
/// on the card the category was left on (its first gun when it was never visited); the card being
/// left is remembered in [`Editor::carousel`].
pub fn step_category(editor: &mut Editor, cats: &[Category], step: i32) -> bool {
    if cats.is_empty() || step == 0 {
        return false;
    }
    let active = category_of(cats, editor.option);
    let next = (active as i32 + step).rem_euclid(cats.len() as i32) as usize;
    if editor.carousel.len() < cats.len() {
        editor.carousel.resize(cats.len(), None);
    }
    editor.carousel[active] = Some(editor.option);
    let target = editor.carousel[next].filter(|o| cats[next].options.contains(o)).unwrap_or(cats[next].options[0]);
    let moved = target != editor.option;
    editor.option = target;
    moved
}

/// Move the highlight within the active category: BO2's Left/Right on the carousel. The first and
/// last cards do not wrap — `CoD_CardCarousel_PerformEdgeBounce` eats the press with a bounce
/// (`cardcarousel.lua:254-381`) — so a press at an end is refused (`false`).
pub fn step_within_category(editor: &mut Editor, cats: &[Category], step: i32) -> bool {
    if cats.is_empty() || step == 0 {
        return false;
    }
    let cat = &cats[category_of(cats, editor.option)];
    if cat.options.is_empty() {
        return false;
    }
    let pos = cat.options.iter().position(|i| *i == editor.option).unwrap_or(0) as i32;
    let next = pos + step.signum();
    if next < 0 || next >= cat.options.len() as i32 {
        return false;
    }
    editor.option = cat.options[next as usize];
    true
}

/// The highlighted gun's position inside its category (the carousel's `m_currentCardIndex`, 0-based).
pub fn position_in_category(cats: &[Category], option: usize) -> usize {
    cats.get(category_of(cats, option)).and_then(|c| c.options.iter().position(|i| *i == option)).unwrap_or(0)
}

// ------------------------------------------------------------------------------------------------
// Removing a slot (BO2's `R Remove` and the circled X)
// ------------------------------------------------------------------------------------------------
