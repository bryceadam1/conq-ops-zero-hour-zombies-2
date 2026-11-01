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
use crate::weapons::{self, WeaponDef};

/// BO2's dual-wield attachment (`ATTACHMENT_DW`), the one that renames the given weapon.
pub const DUAL_WIELD: &str = "dw";

/// The slot names of `mp/statstable.csv` column 13 that hold guns.
pub const PRIMARY: &str = "primary";
pub const SECONDARY: &str = "secondary";
/// The slot names of `mp/statstable.csv` column 13 that hold offhands: the lethal slot's
/// (`primarygrenade`: frag, semtex, … and the placed claymore, Bouncing Betty, C4) and the
/// tactical slot's (`specialgrenade`: concussion, … and the Trophy System, Tactical Insertion,
/// Black Hat). The slot, not the weapon's class, decides — the placed ones are `Equipment`.
pub const PRIMARY_GRENADE: &str = "primarygrenade";
pub const SPECIAL_GRENADE: &str = "specialgrenade";

/// The statstable slot of the lethal (`true`) or the tactical offhand slot.
pub fn offhand_slot(lethal: bool) -> &'static str {
    if lethal {
        PRIMARY_GRENADE
    } else {
        SPECIAL_GRENADE
    }
}

/// The catalog's create-a-class entries: the converted table's base weapons that are BO2 class
/// items (a `CacSlot`), once per display name. The table also carries the alt-mode variants an
/// attachment adds (`gl_an94_mp`) and the converted combinations (`an94_mp+gl`), which are not
/// items of their own. Every returned id is a base id, which is what the converter draws an icon
/// for (`ZHBO_BO2_Icon_<base id>`).
pub(super) fn cac_entries() -> Vec<&'static WeaponDef> {
    let all = weapons::catalog().all();
    let mut seen: Vec<&str> = Vec::new();
    let mut out: Vec<&'static WeaponDef> = all
        .iter()
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
        .collect();
    // BO2's list order (G17, `FUN_008cdde0`): sort key, then item index; stable, so a table without
    // the order (all 0) keeps its own.
    out.sort_by_key(|d| list_key(d));
    out
}

/// A Create-a-Class item's place in its list: the statstable sort key (column 9), then the item
/// index (column 0), [`WeaponDef::cac_sort`]/[`WeaponDef::cac_index`].
pub fn list_key(d: &WeaponDef) -> (i32, i32) {
    (d.cac_sort, d.cac_index)
}

/// The slots whose items a weapon slot lists: the primary its own; the secondary its own, or the
/// primary items only under Overkill (`cacweaponsp.lua:638-720`: with the card the secondary's
/// carousels are the primary groups).
pub(super) fn slots_of(primary: bool, wildcards: &[Wildcard]) -> Vec<&'static str> {
    if primary || wildcards.contains(&Wildcard::Overkill) {
        vec![PRIMARY]
    } else {
        vec![SECONDARY]
    }
}

/// The gun ids a weapon slot lists, in catalog order (the view groups them into categories); the
/// Assault Shield is one of the primary slot's items.
pub(super) fn gun_ids(slots: &[&str]) -> Vec<&'static str> {
    cac_entries().into_iter().filter(|d| d.class.is_slot_weapon() && slots.contains(&d.cac_slot.as_str())).map(|d| d.id.as_str()).collect()
}

/// The offhand ids the lethal (`true`) or the tactical slot lists: the items of its statstable
/// slot ([`offhand_slot`]), in catalog order.
pub(super) fn equipment_ids(lethal: bool) -> Vec<&'static str> {
    offhand_ids(lethal, &[])
}

/// The statstable slot whose items an offhand slot lists with these cards: its own, except that
/// under Tactician the lethal slot lists the tactical slot's items (`cacgrenades.lua:183-195`:
/// `primaryGrenade` with `bonuscard_two_tacticals` reads `GetUnlockablesBySlotName(specialGrenade)`).
pub fn listed_slot(lethal: bool, wildcards: &[Wildcard]) -> &'static str {
    if lethal && wildcards.contains(&Wildcard::Tactician) {
        SPECIAL_GRENADE
    } else {
        offhand_slot(lethal)
    }
}

/// The offhand ids an offhand slot lists with these cards ([`listed_slot`]), in catalog order.
pub(super) fn offhand_ids(lethal: bool, wildcards: &[Wildcard]) -> Vec<&'static str> {
    let slot = listed_slot(lethal, wildcards);
    cac_entries().into_iter().filter(|d| d.class.is_offhand() && d.cac_slot == slot).map(|d| d.id.as_str()).collect()
}

/// Whether `id` is an item the lethal (`true`) or the tactical slot may hold (`CheckItems`).
pub fn fits_offhand(id: &str, lethal: bool) -> bool {
    weapons::catalog().get(id).is_some_and(|d| d.class.is_offhand() && d.cac_slot == offhand_slot(lethal))
}

/// How many of an offhand item one slot may carry: BO2's slot list shows two boxes for a tactical
/// (`cacgrenadesbutton.lua:150-180`: `math.min(2, Engine.GetMaxAmmoForItem(item))`), so an item
/// whose weapon caps its ammo at one (the smoke, the Tactical Insertion) takes one; 0 for none.
/// The two is `maxGrenades` ([`crate::classes::max_grenades()`]), the cap `WeaponDef::max_reserve`
/// (BO2's `iMaxAmmo`).
pub fn offhand_cap(id: &str) -> u8 {
    if id.is_empty() {
        return 0;
    }
    weapons::catalog().get(id).map_or(0, |d| (d.max_reserve.max(1) as usize).min(crate::classes::max_grenades()) as u8)
}

/// Whether the item `base` is one the slot may hold: a gun (or shield) item of the slot's own, or a primary
/// item in the secondary slot (which Overkill allows, see [`super::wildcards_of`]).
pub fn fits_slot(base: &str, primary: bool) -> bool {
    let slots = if primary { vec![PRIMARY] } else { vec![SECONDARY, PRIMARY] };
    weapons::catalog().get(base).is_some_and(|d| d.class.is_slot_weapon() && slots.contains(&d.cac_slot.as_str()))
}

/// Whether the item `base` is a primary item (in the secondary slot it needs Overkill).
pub fn is_primary_item(base: &str) -> bool {
    weapons::catalog().get(base).is_some_and(|d| d.cac_slot == PRIMARY)
}

/// Whether the item `id` is a tactical-slot item (statstable column 13 `specialgrenade`): in the
/// lethal slot it needs Tactician (`checkclasses.lua:677-686`).
pub fn is_tactical_item(id: &str) -> bool {
    !id.is_empty() && weapons::catalog().get(id).is_some_and(|d| d.cac_slot == SPECIAL_GRENADE)
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
