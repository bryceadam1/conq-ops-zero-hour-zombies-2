//! What the editor offers, read by the conformance emitter.

use super::*;

/// What the editor offers, as the conformance emitter (`zhbo-assets::conformance`) reads it: each
/// slot's weapon ids in list order, and per weapon the attachment shorts and the slot's caps. It is
/// the editor's own lists ([`gun_ids`], [`equipment_ids`], [`allowed_shorts`], [`Draft`]'s caps),
/// read-only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer {
    /// The primary slot's weapons (BO2's `primary` items, [`items`]).
    pub primary: Vec<&'static str>,
    /// The secondary slot's weapons without Overkill (BO2's `secondary` items).
    pub secondary: Vec<&'static str>,
    pub lethal: Vec<&'static str>,
    pub tactical: Vec<&'static str>,
    /// The most attachments a primary takes without, then with, Primary Gunfighter.
    pub primary_caps: (usize, usize),
    /// The most attachments a secondary takes without, then with, Secondary Gunfighter.
    pub secondary_caps: (usize, usize),
}

/// The editor's offer over the installed catalog.
pub fn offer() -> Offer {
    let caps = |card: Wildcard, primary: bool| {
        let mut d = Draft::default();
        let base = if primary { d.primary_att_max() } else { d.secondary_att_max() };
        d.wildcards.push(card);
        (base, if primary { d.primary_att_max() } else { d.secondary_att_max() })
    };
    Offer {
        primary: gun_ids(&items::slots_of(true, &[])),
        secondary: gun_ids(&items::slots_of(false, &[])),
        lethal: equipment_ids(true),
        tactical: equipment_ids(false),
        primary_caps: caps(Wildcard::PrimaryGunfighter, true),
        secondary_caps: caps(Wildcard::SecondaryGunfighter, false),
    }
}

/// The attachment shorts the editor offers on `base` ([`allowed_shorts`]).
pub fn offered_attachments(base: &str) -> Vec<&'static str> {
    allowed_shorts(base)
}

/// The weapon id a draft with these attachments asks the catalog for ([`Draft::primary_id`]).
pub fn draft_weapon_id(base: &str, shorts: &[&str]) -> String {
    Draft::weapon_with_atts(base, &shorts.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}
