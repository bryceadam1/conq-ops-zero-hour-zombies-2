//! What Create-a-Class offers: BO2's weapon items and the weapon a class gets for one.
//!
//! * **The items** are `mp/statstable.csv`'s weapon rows with a non-negative allocation cost, each
//!   in its column-13 slot (`primary`, `secondary`, `primarygrenade`, `specialgrenade`; Ghidra
//!   G15). The converter carries the slot on the def ([`WeaponDef::cac_slot`]); a weapon with none
//!   is not a class item (the `_dw` pistols, the War Machine, the held knife).
//! * **The slots**: the primary lists the `primary` items; the secondary the `secondary` items,
//!   and the `primary` ones too under Overkill (`MPUI_BONUSCARD_OVERKILL`: a second primary).
//! * **The given weapon** (`GetLoadoutWeapon`, `FUN_00631780`, G16): `<ref>_mp` with `+name` per
//!   attachment, except that the dual-wield attachment (`eAttachment` 4, `dw`) switches to the
//!   underscore form `<ref>_dw[_name…]_mp` — a pistol with `dw` is the separate weapon
//!   `fnp45_dw_mp`, whose def names its left gun. [`given_weapon`] builds that id and
//!   [`given_parts`] reads one back into the item and its attachments.

use super::Wildcard;
use crate::attachments::variant_id;
use crate::weapons::{self, Class as WeaponClass, WeaponDef};

/// BO2's dual-wield attachment (`ATTACHMENT_DW`), the one that renames the given weapon.
pub const DUAL_WIELD: &str = "dw";

/// The slot names of `mp/statstable.csv` column 13 that hold guns.
pub const PRIMARY: &str = "primary";
pub const SECONDARY: &str = "secondary";

/// The catalog's create-a-class entries: the converted table's base weapons that are BO2 class
/// items (a `CacSlot`), once per display name. The table also carries the alt-mode variants an
/// attachment adds (`gl_an94_mp`) and the converted combinations (`an94_mp+gl`), which are not
/// items of their own. Every returned id is a base id, which is what the converter draws an icon
/// for (`ZHBO_BO2_Icon_<base id>`).
pub(super) fn cac_entries() -> Vec<&'static WeaponDef> {
    let all = weapons::catalog().all();
    let mut seen: Vec<&str> = Vec::new();
    all.iter()
        .copied()
        .filter(|d| !d.cac_slot.is_empty() && d.attachments.is_empty() && d.alt.is_empty())
        .filter(|d| !all.iter().any(|base| base.alt_modes.iter().any(|(_, alt)| alt == &d.id)))
        .filter(|d| {
            // Safety net: no weapon may appear twice under one display name.
            let name = d.display_name.as_str();
            if seen.contains(&name) {
                false
            } else {
                seen.push(name);
                true
            }
        })
        .collect()
}

/// The slots whose items a weapon slot lists: the primary its own; the secondary its own, plus the
/// primary items under Overkill.
pub(super) fn slots_of(primary: bool, wildcards: &[Wildcard]) -> Vec<&'static str> {
    if primary {
        vec![PRIMARY]
    } else if wildcards.contains(&Wildcard::Overkill) {
        vec![SECONDARY, PRIMARY]
    } else {
        vec![SECONDARY]
    }
}

/// The gun ids a weapon slot lists, in catalog order (the view groups them into categories).
pub(super) fn gun_ids(slots: &[&str]) -> Vec<&'static str> {
    cac_entries().into_iter().filter(|d| d.class.is_gun() && slots.contains(&d.cac_slot.as_str())).map(|d| d.id.as_str()).collect()
}

/// The equipment ids of a class.
pub(super) fn equipment_ids(class: WeaponClass) -> Vec<&'static str> {
    cac_entries().into_iter().filter(|d| d.class == class).map(|d| d.id.as_str()).collect()
}

/// Whether the item `base` is one the slot may hold: a gun item of the slot's own, or a primary
/// item in the secondary slot (which Overkill allows, see [`super::wildcards_of`]).
pub fn fits_slot(base: &str, primary: bool) -> bool {
    weapons::catalog().get(base).is_some_and(|d| d.class.is_gun() && slots_of(primary, &[Wildcard::Overkill]).contains(&d.cac_slot.as_str()))
}

/// Whether the item `base` is a primary item (in the secondary slot it needs Overkill).
pub fn is_primary_item(base: &str) -> bool {
    weapons::catalog().get(base).is_some_and(|d| d.cac_slot == PRIMARY)
}

/// The weapon a class with item `base` and attachments `shorts` gets (`GetLoadoutWeapon`): the
/// `+` form (`an94_mp+gl+reflex`, sorted) or, with `dw`, the underscore form (`fnp45_dw_mp`).
pub fn given_weapon(base: &str, shorts: &[&str]) -> String {
    if !shorts.contains(&DUAL_WIELD) {
        return variant_id(base, shorts);
    }
    let reference = base.strip_suffix("_mp").unwrap_or(base);
    let mut others: Vec<&str> = shorts.iter().copied().filter(|s| *s != DUAL_WIELD && !s.is_empty()).collect();
    others.sort_unstable();
    others.dedup();
    std::iter::once(reference).chain(std::iter::once(DUAL_WIELD)).chain(others).collect::<Vec<_>>().join("_") + "_mp"
}

/// The item and attachment shorts of a given weapon id ([`given_weapon`]'s inverse): `an94_mp+gl`
/// is `an94_mp` with `gl`; `fnp45_dw_mp` is `fnp45_mp` with `dw` — only when the catalog's
/// `fnp45_mp` offers `dw`, so no other name with a `_dw` in it is mistaken for one.
pub fn given_parts(id: &str) -> (String, Vec<String>) {
    let mut parts = id.split('+');
    let base = parts.next().unwrap_or("");
    let shorts: Vec<String> = parts.map(str::to_string).collect();
    if shorts.is_empty() {
        if let Some((reference, rest)) = base.strip_suffix("_mp").and_then(|b| b.split_once(&format!("_{DUAL_WIELD}"))) {
            let item = format!("{reference}_mp");
            let others: Vec<&str> = rest.split('_').filter(|s| !s.is_empty()).collect();
            let offers = |s: &str| weapons::catalog().get(&item).is_some_and(|d| d.allowed.iter().any(|(a, _)| a == s));
            if (rest.is_empty() || rest.starts_with('_')) && offers(DUAL_WIELD) && others.iter().all(|s| offers(s)) {
                return (item, std::iter::once(DUAL_WIELD).chain(others).map(str::to_string).collect());
            }
        }
    }
    (base.to_string(), shorts)
}
