//! `expected.csv`'s create-a-class rows, from the player's install: `mp/statstable.csv` and
//! `mp/attachmenttable.csv` (the items), the compiled UI Lua (`zhbo_formats::havok_lua`: the class
//! caps of `cacutility.lua`, the weapon categories of `cacweaponslot.lua`), the stock allocation of
//! `mp/gamesettings_default.cfg`, and BO2's rules (G15, G16, G17).

use super::combos::{self, kind_of};
use super::install::Install;
use crate::schema::{Row, RowBuilder, Schema};
use crate::weapons::ATTACHMENTS;
use std::collections::BTreeMap;
use zhbo_formats::havok_lua;

/// The class caps of `CoD.CACUtility` that are facts in both worlds (the scorestreak caps,
/// `maxRewards`/`maxRewardSlots`, are the V1 plan's deliberate divergence).
pub const CAPS: [&str; 9] = [
    "maxCustomClass", "maxGrenades", "maxSpecialtySlots", "maxPerks", "maxAttachments", "maxPerkCategories",
    "maxPerksInEachCategory", "maxBonusCards", "maxDefaultClass",
];

/// The UI script that declares the caps, and the one that lists the weapon categories in order.
const CAC_UTILITY: &str = "ui_mp/t6/menus/cacutility.lua";
const WEAPON_SLOT: &str = "ui_mp/t6/menus/cacweaponslot.lua";

fn cell(r: &[String], i: usize) -> String {
    r.get(i).cloned().unwrap_or_default()
}

fn lua(install: &Install, name: &str) -> Result<havok_lua::Function, String> {
    let bytes = havok_lua::rawfile(&install.patch, name).or_else(|| havok_lua::rawfile(&install.patch_ui, name)).ok_or_else(|| format!("{name}: not in patch_mp or patch_ui_mp"))?;
    havok_lua::parse(&bytes).map_err(|e| format!("{name}: {e}"))
}

/// A row of `kind`/`item` with the columns `fill` sets (the rest empty).
pub fn row(schema: &Schema, kind: &str, item: &str, fill: impl FnOnce(&mut RowBuilder)) -> Result<Row, String> {
    let mut b = RowBuilder::new(schema, kind, item);
    fill(&mut b);
    b.finish_sparse()
}

/// G17: a list's order — column 9 ascending, then the item index (`FUN_008d19a0`'s comparator).
fn ranks(rows: &[&Vec<String>]) -> BTreeMap<String, i64> {
    let mut v: Vec<(i64, i64, String)> = rows.iter().map(|r| (cell(r, 9).trim().parse().unwrap_or(0), cell(r, 0).trim().parse().unwrap_or(0), cell(r, 4))).collect();
    v.sort();
    v.into_iter().enumerate().map(|(i, (_, _, k))| (k, i as i64)).collect()
}

pub fn rows(install: &Install, schema: &Schema) -> Result<Vec<Row>, String> {
    let stats = install.table("mp/statstable.csv")?;
    let atable = install.table("mp/attachmenttable.csv")?;
    let compat = combos::table(&atable);
    let variants = install.variants();
    let atts = install.attachments();
    let loc = |k: &str| if k.is_empty() { String::new() } else { install.localize(k) };
    let mut out = Vec::new();

    // Weapons: the CaC items, ranked within their group.
    let items = combos::items(&stats);
    let weapon_rows: Vec<&Vec<String>> = stats.iter().filter(|r| cell(r, 2).starts_with("weapon_") && cell(r, 12).trim().parse::<i32>().is_ok_and(|c| c >= 0)).collect();
    let mut by_group: BTreeMap<String, Vec<&Vec<String>>> = BTreeMap::new();
    for r in &weapon_rows {
        by_group.entry(cell(r, 2)).or_default().push(r);
    }
    let rank: BTreeMap<String, i64> = by_group.values().flat_map(|g| ranks(g)).collect();
    let att_row = |kind: usize| atable.iter().find(|r| cell(r, 2) == "attachment" && kind_of(&cell(r, 4)) == Some(kind));
    for item in &items {
        let Some(r) = stats.iter().find(|r| cell(r, 4) == item.reference) else { continue };
        out.push(row(schema, "weapon", &item.reference, |b| {
            b.text("name", loc(&cell(r, 3))).text("description", loc(&cell(r, 7))).text("image", cell(r, 6)).text("group", &item.group).text("slot", &item.slot).int("cost", item.cost as i64);
            b.int("order", rank.get(&item.reference).copied().unwrap_or(-1));
        })?);
        // The item's attachments, in its column-8 order (`FUN_008ce960` walks them in that order).
        let Some(v) = variants.get(&item.variant).copied() else { continue };
        let offered = combos::offered(item, &v, &atts);
        let in_order = combos::listed(item);
        for (i, &k) in in_order.iter().enumerate() {
            let Some(ar) = att_row(k) else { continue };
            let with: Vec<&str> = offered.iter().copied().filter(|&o| o != k && compat.compatible(k, o)).map(|o| ATTACHMENTS[o]).collect();
            out.push(row(schema, "attachment", &format!("{}+{}", item.reference, ATTACHMENTS[k]), |b| {
                b.text("name", loc(&cell(ar, 3))).text("description", loc(&cell(ar, 7))).text("image", cell(ar, 6)).text("group", cell(ar, 1));
                // The class editor's cost of an item's attachment: 1 when the item has it
                // (`GetItemAttachmentAllocationCost` with two arguments, `FUN_008cfad0`, G17).
                b.int("cost", 1).int("order", i as i64).set("members", &with);
            })?);
        }
        // Dual wield: the weapon a class with this pistol and `dw` gets (G16).
        if offered.contains(&4) {
            let name = format!("{}_dw_mp", item.reference);
            out.push(row(schema, "dualwield", &item.reference, |b| {
                b.opt_text("value", variants.contains_key(&name).then_some(name.clone()));
            })?);
        }
    }

    // Perks and wildcards: the statstable's specialty and bonus-card rows.
    for (group, kind, prefix) in [("specialty", "perk", "PERKS_"), ("bonuscard", "wildcard", "")] {
        let rows: Vec<&Vec<String>> = stats.iter().filter(|r| cell(r, 2) == group && cell(r, 12).trim().parse::<i32>().is_ok_and(|c| c >= 0)).collect();
        let mut by_slot: BTreeMap<String, Vec<&Vec<String>>> = BTreeMap::new();
        for r in &rows {
            by_slot.entry(cell(r, 13)).or_default().push(r);
        }
        let rank: BTreeMap<String, i64> = by_slot.values().flat_map(|g| ranks(g)).collect();
        for r in rows {
            let item = if kind == "perk" { cell(r, 3).trim_start_matches(prefix).to_lowercase() } else { cell(r, 4).trim_start_matches("bonuscard_").to_string() };
            let members: Vec<String> = if kind == "perk" { cell(r, 4).split('|').filter(|s| !s.is_empty()).map(str::to_string).collect() } else { Vec::new() };
            out.push(row(schema, kind, &item, |b| {
                b.text("name", loc(&cell(r, 3))).text("description", loc(&cell(r, 7))).text("image", cell(r, 6)).text("slot", cell(r, 13));
                b.int("cost", cell(r, 12).trim().parse().unwrap_or(0)).int("order", rank.get(&cell(r, 4)).copied().unwrap_or(-1));
                if kind == "perk" {
                    b.set("members", &members);
                }
            })?);
        }
    }

    // Camo, reticle and reticle-colour options: a group's rows in table order
    // (`caccamomenu.lua:1205-1213` lists group index 0..N-1).
    for group in ["camo", "reticle", "reticle_color"] {
        for (i, r) in atable.iter().filter(|r| cell(r, 1) == group && cell(r, 2) == "weaponoption").enumerate() {
            out.push(row(schema, group, &cell(r, 4), |b| {
                b.text("name", loc(&cell(r, 3))).text("image", cell(r, 6)).int("order", i as i64);
            })?);
        }
    }

    // The class caps (`cacutility.lua`'s main chunk) and the stock allocation.
    let caps: BTreeMap<String, String> = lua(install, CAC_UTILITY)?.stores().into_iter().filter(|s| s.path == "CoD.CACUtility").map(|s| (s.field, s.value.text())).collect();
    for cap in CAPS {
        out.push(row(schema, "constant", cap, |b| {
            b.opt_text("value", caps.get(cap).cloned());
        })?);
    }
    let cfg = havok_lua::rawfile(&install.patch, "mp/gamesettings_default.cfg").map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
    let alloc = cfg.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        (w.next() == Some("gametype_setting") && w.next() == Some("maxAllocation")).then(|| w.next().map(str::to_string)).flatten()
    });
    out.push(row(schema, "constant", "maxAllocation", |b| {
        b.opt_text("value", alloc);
    })?);

    // The weapon categories in the order the weapon-type list adds them (`cacweaponslot.lua`).
    let slot = lua(install, WEAPON_SLOT)?;
    let groups: Vec<String> = slot.all().iter().flat_map(|f| f.stores()).filter(|s| s.field == "weaponGroup").map(|s| s.value.text()).collect();
    for (i, g) in groups.iter().enumerate() {
        out.push(row(schema, "category", &i.to_string(), |b| {
            b.text("value", g);
        })?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: &str, reference: &str, sort: &str) -> Vec<String> {
        let mut r = vec![String::new(); 14];
        r[0] = index.into();
        r[4] = reference.into();
        r[9] = sort.into();
        r
    }

    #[test]
    fn a_list_orders_by_column_nine_then_the_item_index() {
        let (a, b, c, d) = (item("26", "an94", "9"), item("32", "tar21", "1"), item("31", "hk416", "5"), item("24", "xm8", "5"));
        let r = ranks(&[&a, &b, &c, &d]);
        // tar21 (1) first; xm8 and hk416 share 5, the lower index (24) first; an94 (9) last.
        assert_eq!((r["tar21"], r["xm8"], r["hk416"], r["an94"]), (0, 1, 2, 3));
    }

    #[test]
    fn a_sparse_row_leaves_the_unset_columns_empty() {
        let s = crate::cac::schema();
        let r = row(&s, "constant", "maxAttachments", |b| {
            b.text("value", "2");
        })
        .unwrap();
        let v = s.index("value").unwrap();
        assert_eq!(r.cells[v], "2");
        assert!(r.cells.iter().enumerate().all(|(i, c)| i == v || c.is_empty()));
    }
}
