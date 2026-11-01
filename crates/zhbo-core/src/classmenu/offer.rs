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

/// Every value the editor can put in each field of a class, each combined with the largest value
/// of every other field: the pick payload's coverage set ([`crate::classwire`]). The payload's
/// fields are sized independently, so the largest of these is the largest legal kit, and every
/// legal value of every field is visited once:
///
/// * the primary: every offered item with every legal attachment set up to Primary Gunfighter's
///   cap (BO2's compatibility, [`super::legal::legal`]), as the weapon the class gets;
/// * the secondary: every secondary item, and every primary under Overkill, likewise up to
///   Secondary Gunfighter's cap;
/// * the lethal slot's items and, under Tactician, the tactical ones; the tactical slot's items;
///   every count from none to `maxGrenades`;
/// * every perk, in a class carrying `maxSpecialtySlots` of them;
/// * every camo the Personalize screens offer each gun and the knife, every reticle of both
///   optics and the colour;
/// * an unnamed class and a name of [`NAME_MAX`] characters.
///
/// The spare-round override ([`CustomClass::reserve`]) is no editor field; the payload carries it
/// at a fixed size whatever its value.
pub fn kit_coverage(mut visit: impl FnMut(&CustomClass)) {
    let o = offer();
    let guns = |items: &[&str], cap: usize| -> Vec<String> {
        let mut out = Vec::new();
        for base in items {
            let shorts = offered_attachments(base);
            let mut picked: Vec<&str> = Vec::new();
            attachment_sets(base, &shorts, 0, cap, &mut picked, &mut out);
        }
        out
    };
    let primaries = guns(&o.primary, o.primary_caps.1);
    let mut secondaries = guns(&o.secondary, o.secondary_caps.1);
    secondaries.extend(guns(&o.primary, o.secondary_caps.1));
    let mut lethals: Vec<String> = Vec::new();
    for id in o.lethal.iter().chain(o.tactical.iter()) {
        if !lethals.iter().any(|l| l == id) {
            lethals.push(id.to_string());
        }
    }
    let tacticals: Vec<String> = o.tactical.iter().map(|s| s.to_string()).collect();
    let counts: Vec<u8> = (0..=classes::max_grenades() as u8).collect();
    let perks: Vec<String> = weapons::perks::PERKS.iter().map(|p| p.id.to_string()).collect();
    let choices = |kind: PersonalizeKind, weapons: &mut dyn Iterator<Item = &str>| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for w in weapons {
            for tab in crate::classui::personalize_tabs_for(kind, Some(w)) {
                for c in tab.choices {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        out
    };
    let bases = |ids: &[String]| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            let base = items::given_parts(id).0;
            if !out.contains(&base) {
                out.push(base);
            }
        }
        out
    };
    let camos = choices(PersonalizeKind::Camo, &mut bases(&primaries).iter().map(String::as_str));
    let secondary_camos = choices(PersonalizeKind::SecondaryCamo, &mut bases(&secondaries).iter().map(String::as_str));
    let knives = choices(PersonalizeKind::Knife, &mut std::iter::once(weapons::KNIFE_HELD));
    let reticles = choices(PersonalizeKind::Reticle, &mut std::iter::once(""));
    let secondary_reticles = choices(PersonalizeKind::SecondaryReticle, &mut std::iter::once(""));
    let colors = choices(PersonalizeKind::ReticleColor, &mut std::iter::once(""));
    let names = vec![String::new(), "W".repeat(NAME_MAX)];

    // The largest value of each field: the one whose payload is longest with only it set.
    let size = |c: &CustomClass| c.to_wire_words().len() * 4;
    let largest = |values: &[String], set: &dyn Fn(&mut CustomClass, &str)| -> String {
        values
            .iter()
            .max_by_key(|v| {
                let mut c = CustomClass::default();
                set(&mut c, v);
                size(&c)
            })
            .cloned()
            .unwrap_or_default()
    };
    type Set = fn(&mut CustomClass, &str);
    let fields: [(&[String], Set); 11] = [
        (&names, |c, v| c.name = v.into()),
        (&primaries, |c, v| c.primary = v.into()),
        (&secondaries, |c, v| c.secondary = v.into()),
        (&lethals, |c, v| c.lethal = v.into()),
        (&tacticals, |c, v| c.tactical = v.into()),
        (&camos, |c, v| c.camo = v.into()),
        (&secondary_camos, |c, v| c.secondary_camo = v.into()),
        (&knives, |c, v| c.knife = v.into()),
        (&reticles, |c, v| c.reticle = v.into()),
        (&colors, |c, v| c.reticle_color = v.into()),
        (&secondary_reticles, |c, v| c.secondary_reticle = v.into()),
    ];
    let mut worst = CustomClass {
        melee: weapons::KNIFE_HELD.into(),
        lethal_count: *counts.last().unwrap_or(&0),
        tactical_count: *counts.last().unwrap_or(&0),
        perks: vec![largest(&perks, &|c, v| c.perks = vec![v.into()]); crate::classwire::max_legal_perks()],
        ..CustomClass::default()
    };
    for (values, set) in &fields {
        set(&mut worst, &largest(values, set));
    }

    visit(&worst);
    for (values, set) in &fields {
        for v in values.iter() {
            let mut c = worst.clone();
            set(&mut c, v);
            visit(&c);
        }
    }
    for p in &perks {
        let mut c = worst.clone();
        c.perks[0] = p.clone();
        visit(&c);
    }
    for (l, t) in counts.iter().flat_map(|l| counts.iter().map(move |t| (*l, *t))) {
        visit(&CustomClass { lethal_count: l, tactical_count: t, ..worst.clone() });
    }
}

/// Every legal attachment set on `base` of at most `cap` shorts from `shorts[from..]`, extending
/// `picked`, pushed to `out` as the weapon the class gets.
fn attachment_sets<'a>(base: &str, shorts: &[&'a str], from: usize, cap: usize, picked: &mut Vec<&'a str>, out: &mut Vec<String>) {
    out.push(draft_weapon_id(base, picked));
    if picked.len() == cap {
        return;
    }
    for i in from..shorts.len() {
        picked.push(shorts[i]);
        if super::legal::legal(base, picked) {
            attachment_sets(base, shorts, i + 1, cap, picked, out);
        }
        picked.pop();
    }
}
