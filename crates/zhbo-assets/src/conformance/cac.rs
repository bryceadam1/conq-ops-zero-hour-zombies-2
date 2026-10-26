//! `actual.csv`'s create-a-class rows: what the mod's class editor offers and decides, read from the
//! editor's own lists ([`zhbo_core::classmenu::offer`]), the installed catalog (weapons, attachments,
//! camo and reticle options), the perk table, the wildcards, the class caps, and the converted art's
//! sources (the converter's own sprite tables), in BO2's vocabulary.

use super::actual::Shipped;
use crate::hud::Source;
use zhbo_conformance::bo2::cac::row;
use zhbo_conformance::schema::{Row, Schema};
use zhbo_core::classmenu::{self, draft_weapon_id, offer, offered_attachments, Draft, Wildcard};
use zhbo_core::weapons::{self, Class, Perk, PERKS};

/// `eAttachmentPoint`'s names (the attachment table's column 1).
const POINTS: [&str; 6] = ["none", "top", "bottom", "trigger", "muzzle", "gunperk"];

/// The CaC group a mod class lists under (statstable's groups).
pub fn group_of(c: Class) -> &'static str {
    match c {
        Class::Rifle => "weapon_assault",
        Class::Smg => "weapon_smg",
        Class::Lmg => "weapon_lmg",
        Class::Sniper => "weapon_sniper",
        Class::Shotgun => "weapon_cqb",
        Class::Pistol => "weapon_pistol",
        Class::Launcher => "weapon_launcher",
        Class::Special => "weapon_special",
        Class::Lethal | Class::Tactical | Class::Equipment => "weapon_grenade",
        Class::Melee => "",
    }
}

/// The BO2 material a fixed CaC sprite is converted from (`cac::SPRITES`).
fn sprite_material(name: &str) -> Option<String> {
    crate::cac::SPRITES.iter().find(|s| s.name == name).and_then(|s| match &s.source {
        Source::Material(_, m) => Some(m.to_string()),
        Source::Dynamic(m) => Some(m.clone()),
        Source::Hash(_) => None,
    })
}

/// What one element costs in the editor's allocation (`Draft::spent` with only that element).
fn cost_of(set: impl FnOnce(&mut Draft)) -> i64 {
    // From an empty class: the default draft carries a kit.
    let mut d = Draft::default();
    d.primary.clear();
    d.primary_atts.clear();
    d.secondary.clear();
    d.secondary_atts.clear();
    d.lethal.clear();
    d.tactical.clear();
    d.perks = [None; 3];
    d.extra_perks = [None; 3];
    d.wildcards.clear();
    let empty = d.spent();
    set(&mut d);
    (d.spent() - empty) as i64
}

pub fn rows(shipped: &Shipped, bo2_dir: &std::path::Path, schema: &Schema) -> Result<Vec<Row>, String> {
    let o = offer();
    let cat = weapons::catalog();
    let images = crate::cac::item_images(bo2_dir);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Weapons, per slot list; each one's place among its group's entries in that list.
    let lists: [(&str, &Vec<&'static str>); 4] = [("primary", &o.primary), ("secondary", &o.secondary), ("primarygrenade", &o.lethal), ("specialgrenade", &o.tactical)];
    // Every slot list an item appears in (the editor offers the launchers in both weapon slots).
    let slots_of = |id: &str| -> String { lists.iter().filter(|(_, l)| l.contains(&id)).map(|(s, _)| *s).collect::<Vec<_>>().join("+") };
    for (slot, ids) in lists {
        for (i, &id) in ids.iter().enumerate() {
            let Some(d) = cat.get(id) else { continue };
            let reference = id.trim_end_matches("_mp").to_string();
            if !seen.insert(reference.clone()) {
                continue;
            }
            let slots = slots_of(id);
            let group = group_of(d.class);
            let order = ids[..i].iter().filter(|x| cat.get(x).is_some_and(|e| group_of(e.class) == group)).count() as i64;
            let image = shipped.has_sprite(&zhbo_core::names::cac_weapon_image(id)).then(|| images.get(&reference).cloned()).flatten();
            let cost = cost_of(|dr| if slot == "secondary" { dr.secondary = id.to_string() } else if slot == "primary" { dr.primary = id.to_string() } else if slot == "primarygrenade" { dr.lethal = id.to_string() } else { dr.tactical = id.to_string() });
            out.push(row(schema, "weapon", &reference, |b| {
                b.text("name", &d.display_name).opt_text("image", image).text("group", group).text("slot", &slots).int("cost", cost).int("order", order);
            })?);
            // Its attachments, in the editor's order; what each can be combined with is what the
            // editor lets it share the weapon with (`classmenu::legal::compatible`).
            let shorts = offered_attachments(id);
            let att_of = |s: &str| d.allowed.iter().find(|(x, _)| x == s).and_then(|(_, asset)| cat.attachment(asset));
            for (k, &s) in shorts.iter().enumerate() {
                let Some(a) = att_of(s) else { continue };
                let with: Vec<&str> = shorts.iter().copied().filter(|&o| o != s && classmenu::legal::compatible(id, s, o)).collect();
                let image = crate::cac::attachment_material(s).map(|(_, m)| m.to_string());
                let cost = cost_of(|dr| {
                    dr.primary = id.to_string();
                    dr.primary_atts = vec![s.to_string()];
                }) - cost_of(|dr| dr.primary = id.to_string());
                out.push(row(schema, "attachment", &format!("{reference}+{s}"), |b| {
                    // Column 3's localized name is the mod's `full_name` (the attachment menu's
                    // title); `display_name` is the weapon's short label.
                    b.text("name", &a.full_name).text("description", &a.description).opt_text("image", image).text("group", POINTS.get(a.point as usize).copied().unwrap_or("none"));
                    b.int("cost", cost).int("order", k as i64).set("members", &with);
                })?);
            }
            if shorts.contains(&"dw") {
                let given = cat.get(&draft_weapon_id(id, &["dw"])).map(|g| g.id.clone());
                out.push(row(schema, "dualwield", &reference, |b| {
                    b.opt_text("value", given);
                })?);
            }
        }
    }

    // Perks: the perk table, keyed by BO2's statstable reference; the place within the tier.
    for (i, p) in PERKS.iter().enumerate() {
        let perk = Perk::by_id(p.id).ok_or_else(|| format!("perk {} not in the table", p.id))?;
        let order = PERKS[..i].iter().filter(|q| q.tier == p.tier).count() as i64;
        let members: Vec<String> = perk.specialties().iter().map(|s| format!("specialty_{}", s.name())).collect();
        let image = sprite_material(zhbo_core::hud::sprites::PERK_ICONS[i]);
        let cost = cost_of(|dr| dr.perks[(p.tier as usize).clamp(1, 3) - 1] = Some(perk));
        out.push(row(schema, "perk", &p.stats.to_lowercase(), |b| {
            b.text("name", zhbo_core::classui::perk_name(perk)).opt_text("image", image).text("slot", format!("specialty{}", p.tier));
            b.int("cost", cost).int("order", order).set("members", &members);
        })?);
    }

    // Wildcards: keyed by the bonus card their art is converted from.
    for (i, w) in Wildcard::ALL.iter().enumerate() {
        let image = sprite_material(zhbo_core::hud::sprites::WILDCARD_ICONS[*w as usize]);
        let key = image.as_deref().and_then(|m| m.strip_prefix("menu_mp_bonuscard_")).map_or_else(|| w.id().to_string(), str::to_string);
        let cost = cost_of(|dr| dr.wildcards.push(*w));
        out.push(row(schema, "wildcard", &key, |b| {
            b.text("name", w.name()).opt_text("image", image).int("cost", cost).int("order", i as i64);
        })?);
    }

    // Camo options in the order the personalize grid walks a weapon's list (`WeaponDef::camo`, the
    // first primary's: every weapon carries the same table), then the reticles and colours.
    let camos: Vec<&zhbo_core::weapons::CamoOption> = o.primary.first().and_then(|id| cat.get(id)).map(|d| d.camo.iter().filter_map(|c| cat.camo(c)).collect()).unwrap_or_default();
    for (kind, list) in [("camo", camos), ("reticle", cat.reticles().to_vec()), ("reticle_color", cat.reticle_colors().to_vec())] {
        for (i, c) in list.iter().enumerate() {
            out.push(row(schema, kind, &c.id, |b| {
                b.text("name", &c.name).text("image", &c.image).int("order", i as i64);
            })?);
        }
    }

    // The class caps and the allocation.
    let perk_tiers = {
        let mut t: Vec<u8> = PERKS.iter().map(|p| p.tier).collect();
        t.dedup();
        t.len()
    };
    let caps: [(&str, Option<String>); 10] = [
        ("maxCustomClass", Some(zhbo_core::classes::MAX_CUSTOM_CLASSES.to_string())),
        ("maxGrenades", Some(zhbo_core::classes::MAX_GRENADES.to_string())),
        ("maxSpecialtySlots", Some(zhbo_core::classes::MAX_SPECIALTY_SLOTS.to_string())),
        ("maxPerks", None),
        ("maxAttachments", Some(zhbo_core::classes::MAX_ATTACHMENTS.to_string())),
        ("maxPerkCategories", Some(perk_tiers.to_string())),
        ("maxPerksInEachCategory", None),
        ("maxBonusCards", Some(zhbo_core::classes::MAX_BONUS_CARDS.to_string())),
        ("maxDefaultClass", None),
        ("maxAllocation", Some(classmenu::PICK_10.to_string())),
    ];
    for (cap, value) in caps {
        out.push(row(schema, "constant", cap, |b| {
            b.opt_text("value", value);
        })?);
    }

    // The weapon categories: the primary slot's in the editor's order, then the secondary slot's.
    let mut groups: Vec<&str> = Vec::new();
    for c in classmenu::PRIMARY_CLASSES.iter().chain(classmenu::SECONDARY_CLASSES.iter()) {
        let g = group_of(*c);
        if !groups.contains(&g) {
            groups.push(g);
        }
    }
    for (i, g) in groups.iter().enumerate() {
        out.push(row(schema, "category", &i.to_string(), |b| {
            b.text("value", *g);
        })?);
    }
    Ok(out)
}
