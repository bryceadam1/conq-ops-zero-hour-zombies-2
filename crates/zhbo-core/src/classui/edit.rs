//! Clearing a box or a whole slot of a draft (BO2's `R Remove`).

use super::*;

/// Clear one box of a slot: the attachment in that box, the bonus card in that box, or the whole
/// single-value slot. False when the box was already empty.
pub fn clear_cell(d: &mut Draft, slot: Slot, index: usize) -> bool {
    match slot {
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
        // An offhand box: one of the slot's count goes (BO2's `GrenadesChosen` on a filled box
        // lowers `count` by one); the item goes with its last one.
        Slot::Lethal | Slot::Tactical => {
            let lethal = slot == Slot::Lethal;
            let k = usize::from(!lethal);
            let held = if lethal { &mut d.lethal } else { &mut d.tactical };
            if held.is_empty() {
                return false;
            }
            if std::mem::take(&mut d.second_offhand[k]) && d.offhand_boxes(lethal) > 1 {
                return true;
            }
            let held = if lethal { &mut d.lethal } else { &mut d.tactical };
            held.clear();
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
                // A tactical in the lethal slot needs Tactician (`checkclasses.lua:677-686`).
                Wildcard::Tactician => {
                    if crate::classmenu::items::is_tactical_item(&d.lethal) {
                        d.lethal.clear();
                    }
                }
                _ => {
                    if let Some(tier) = w.greed_tier() {
                        d.extra_perks[(tier as usize) - 1] = None;
                    }
                }
            }
            true
        }
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
        Slot::Lethal | Slot::Tactical => {
            let had = clear_cell(d, slot, 0);
            while clear_cell(d, slot, 0) {}
            had
        }
        other => clear_cell(d, other, 0),
    }
}

// ------------------------------------------------------------------------------------------------
// The hosts' shared input fold
// ------------------------------------------------------------------------------------------------
