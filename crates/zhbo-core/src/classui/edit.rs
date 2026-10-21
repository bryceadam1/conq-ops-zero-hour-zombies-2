//! Clearing a box or a whole slot of a draft (BO2's `R Remove`).

use super::*;

/// Clear one box of a slot: the attachment in that box, the bonus card in that box, or the whole
/// single-value slot. False when the box was already empty.
pub fn clear_cell(d: &mut Draft, slot: Slot, index: usize) -> bool {
    match slot {
        Slot::Name => {
            if d.name.is_empty() {
                return false;
            }
            d.name.clear();
            true
        }
        Slot::Primary => {
            if d.primary.is_empty() {
                return false;
            }
            // BO2's `ClearWeapon` takes the attachments with the gun.
            d.primary.clear();
            d.primary_atts.clear();
            true
        }
        Slot::Secondary => {
            if d.secondary.is_empty() {
                return false;
            }
            d.secondary.clear();
            d.secondary_atts.clear();
            true
        }
        Slot::PrimaryAtts => {
            if index >= d.primary_atts.len() {
                return false;
            }
            d.primary_atts.remove(index);
            true
        }
        Slot::SecondaryAtts => {
            if index >= d.secondary_atts.len() {
                return false;
            }
            d.secondary_atts.remove(index);
            true
        }
        Slot::Lethal => {
            if d.lethal.is_empty() {
                return false;
            }
            d.lethal.clear();
            true
        }
        Slot::Tactical => {
            if d.tactical.is_empty() {
                return false;
            }
            d.tactical.clear();
            true
        }
        Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
            let tier = (slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.perks[tier].take().is_some()
        }
        Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            let tier = (slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.extra_perks[tier].take().is_some()
        }
        Slot::Wildcards => {
            if index >= d.wildcards.len() {
                return false;
            }
            let w = d.wildcards.remove(index);
            // Turning the card off drops what it enabled, exactly like un-picking it in the editor.
            match w {
                Wildcard::PrimaryGunfighter => d.primary_atts.truncate(2),
                Wildcard::SecondaryGunfighter => d.secondary_atts.truncate(1),
                _ => {
                    if let Some(tier) = w.greed_tier() {
                        d.extra_perks[(tier as usize) - 1] = None;
                    }
                }
            }
            true
        }
        Slot::Save => false,
    }
}

/// Clear the whole slot: every attachment of the strip, every bonus card, or the single value.
pub fn clear_slot(d: &mut Draft, slot: Slot) -> bool {
    match slot {
        Slot::PrimaryAtts => !std::mem::take(&mut d.primary_atts).is_empty(),
        Slot::SecondaryAtts => !std::mem::take(&mut d.secondary_atts).is_empty(),
        Slot::Wildcards => {
            let had = !d.wildcards.is_empty();
            while !d.wildcards.is_empty() {
                clear_cell(d, Slot::Wildcards, 0);
            }
            had
        }
        other => clear_cell(d, other, 0),
    }
}

// ------------------------------------------------------------------------------------------------
// The hosts' shared input fold
// ------------------------------------------------------------------------------------------------
