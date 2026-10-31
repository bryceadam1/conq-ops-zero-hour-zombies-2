//! The option lists and row values the editor shows, and the indices it opens on.

use super::*;

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
pub(super) fn att_names(atts: &[String]) -> String {
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
pub(super) fn weapon_options(slots: &[&str]) -> Vec<String> {
    gun_ids(slots).iter().map(|id| weapons::weapon(id).display_name.clone()).collect()
}

/// The attachment display names the weapon offers, in its list order ([`attachment_shorts`]); the
/// ones already on it are listed in place so they can be removed.
pub(super) fn attachment_options(base: &str) -> Vec<String> {
    let d = weapons::weapon(base);
    d.allowed.iter().map(|(_, asset)| weapons::catalog().attachment(asset).map(|a| a.display_name.clone()).unwrap_or_else(|| asset.clone())).collect()
}

/// The display names of an offhand slot's listed items, in catalog order. The editor opens on the
/// held one through [`Editor::current_option_index`].
pub(super) fn equipment_options(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| weapons::weapon(id).display_name.clone()).collect()
}

/// The display names of the perks in a tier.
pub(super) fn perk_options(tier: u8) -> Vec<String> {
    perk_list(tier).into_iter().map(Perk::name).collect()
}

/// The attachment shorts the weapon offers.
pub(super) fn allowed_shorts(base: &str) -> Vec<&'static str> {
    weapons::weapon(base).allowed.iter().map(|(s, _)| s.as_str()).collect()
}

/// The attachment grid's shorts: the weapon's offer in BO2's list order. BO2's attachment grid
/// adds one button per attachment number of the item, `1 .. GetNumAttachments - 1`
/// (`cacattachmentsmenu.lua:858-879`), whatever is equipped; the item's attachment numbers follow
/// its statstable column 8 (G17), the order the converter gives `WeaponDef::allowed`. `apply` maps
/// an option index to a short through this.
pub(super) fn attachment_shorts(base: &str) -> Vec<&'static str> {
    allowed_shorts(base)
}

/// The perks of a tier, in table order.
pub(super) fn perk_list(tier: u8) -> Vec<Perk> {
    weapons::perks::PERKS.iter().enumerate().filter(|(_, p)| p.tier == tier).map(|(i, _)| Perk(i as u8)).collect()
}

/// The index of the held weapon in the category list.
pub(super) fn weapon_index(slots: &[&str], current: &str) -> usize {
    gun_ids(slots).iter().position(|id| *id == current).unwrap_or(0)
}

/// The grid index of a weapon's first equipped attachment (the first cell when it has none).
pub(super) fn attachment_index(base: &str, current: &[String]) -> usize {
    current.first().and_then(|c| attachment_shorts(base).iter().position(|s| s == c)).unwrap_or(0)
}

/// The index of the held offhand in its slot's list.
pub(super) fn equipment_index(ids: &[&str], current: &str) -> usize {
    ids.iter().position(|id| *id == current).unwrap_or(0)
}

/// The index of the perk in its tier's list (the first when unset).
pub(super) fn perk_index(tier: u8, p: Option<Perk>) -> usize {
    let list = perk_list(tier);
    p.and_then(|p| list.iter().position(|x| *x == p)).unwrap_or(0)
}
