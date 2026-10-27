//! Which weapons Create-a-Class offers and which attachment combinations a class can carry on each
//! (Ghidra G15, `~/Documents/BO2-reference/ghidra/answers/G15.md`).
//!
//! * The items: `mp/statstable.csv` rows whose group (column 2) is `weapon_*` and whose allocation
//!   cost (column 12) is not negative (`_bot_loadout.gsc:506-509` skips the negative ones); the
//!   loadout slot is column 13, the attachments column 8 (`WeaponAttachment` asset names).
//! * A token counts only when the weapon's variant has that attachment kind (`FUN_008e7fd0`).
//! * Two attachments go together only when each lists the other in `mp/attachmenttable.csv`'s
//!   `compatibleAttachments` (column 11, `FUN_008e70f0`; the pairwise rule `FUN_008e6b20`).
//! * Counts (`checkclasses.lua:735-790`): a primary takes 2 attachments and a third with Primary
//!   Gunfighter; a secondary 1 and a second with Secondary Gunfighter. Grenades take none.

use super::raw::{Attachment, Variant, KINDS};
use crate::weapons::ATTACHMENTS;
use std::collections::{BTreeMap, HashMap};

/// One Create-a-Class weapon item.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Column 4, `an94`.
    pub reference: String,
    /// The variant BO2 gives for it, `an94_mp`.
    pub variant: String,
    /// Column 2, `weapon_assault`.
    pub group: String,
    /// Column 13, `primary`, `secondary`, `primarygrenade`, `specialgrenade`.
    pub slot: String,
    /// Column 12.
    pub cost: i32,
    /// Column 8's tokens (`silencer_shotgun`).
    pub attachments: Vec<String>,
    /// Column 0, the item index.
    pub index: i32,
}

/// The CaC weapon items of a statstable.
pub fn items(statstable: &[Vec<String>]) -> Vec<Item> {
    let cell = |r: &[String], i: usize| r.get(i).cloned().unwrap_or_default();
    statstable
        .iter()
        .filter(|r| cell(r, 2).starts_with("weapon_"))
        .filter_map(|r| {
            let cost: i32 = cell(r, 12).trim().parse().ok()?;
            let reference = cell(r, 4);
            (cost >= 0 && !reference.is_empty() && reference != "weapon_null").then(|| Item {
                variant: format!("{reference}_mp"),
                reference,
                group: cell(r, 2),
                slot: cell(r, 13),
                cost,
                attachments: cell(r, 8).split_whitespace().map(str::to_string).collect(),
                index: cell(r, 0).trim().parse().unwrap_or(-1),
            })
        })
        .collect()
}

/// `mp/attachmenttable.csv`'s `attachment` rows: each kind's attach point and compatible set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// `eAttachment` → (attach point name, compatible kinds as bits).
    pub rows: HashMap<usize, (String, u64)>,
}

/// The `eAttachment` index of a reference name, or `None` (BO2's "unknown", 30).
pub fn kind_of(name: &str) -> Option<usize> {
    ATTACHMENTS.iter().position(|a| *a == name)
}

pub fn table(attachmenttable: &[Vec<String>]) -> Table {
    let cell = |r: &[String], i: usize| r.get(i).cloned().unwrap_or_default();
    let mut rows = HashMap::new();
    for r in attachmenttable.iter().filter(|r| cell(r, 2) == "attachment") {
        let Some(kind) = kind_of(&cell(r, 4)) else { continue };
        // An unknown name sets no bit ("Invalid attachment name '%s' in table '%s'").
        let compat = cell(r, 11).split_whitespace().filter_map(kind_of).fold(0u64, |m, k| m | 1 << k);
        rows.insert(kind, (cell(r, 1), compat));
    }
    Table { rows }
}

impl Table {
    /// `FUN_008e6b20`: `none` goes with anything; otherwise both rows must exist and list each other.
    pub fn compatible(&self, a: usize, b: usize) -> bool {
        if a == 0 || b == 0 {
            return true;
        }
        match (self.rows.get(&a), self.rows.get(&b)) {
            (Some((_, ma)), Some((_, mb))) => ma & (1 << b) != 0 && mb & (1 << a) != 0,
            _ => false,
        }
    }
}

/// One combination a class can carry: its kinds in `eAttachment` order, and the wildcards it needs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Combo {
    pub kinds: Vec<usize>,
    pub requires: Vec<&'static str>,
}

impl Combo {
    /// The row key: the kinds' names joined by `+` (`""` for none).
    pub fn key(&self) -> String {
        self.kinds.iter().map(|&k| ATTACHMENTS[k]).collect::<Vec<_>>().join("+")
    }
}

/// `ATTACHMENT_DW`, the attachment that renames the given weapon.
pub const DW: usize = 4;

/// The weapon `GetLoadoutWeapon` (`FUN_00631780`, G16) gives a class with `item` and `combo`, as
/// a row key: the variant and the `+`-joined attachment key. The `+` form is the item's variant
/// with the combination (`an94_mp`, `gl+reflex`); the dual-wield attachment switches to the
/// underscore form, `<ref>_dw[_name…]_mp`, a separate variant with no `+` part (`fnp45_dw_mp`, `""`).
pub fn given(item: &Item, combo: &Combo) -> (String, String) {
    if !combo.kinds.contains(&DW) {
        return (item.variant.clone(), combo.key());
    }
    let others = combo.kinds.iter().filter(|&&k| k != DW).map(|&k| ATTACHMENTS[k]);
    let name = std::iter::once(item.reference.as_str()).chain(std::iter::once("dw")).chain(others).collect::<Vec<_>>().join("_") + "_mp";
    (name, String::new())
}

/// The attachment kinds an item lists, in its column-8 order: each token's text before the first
/// `_` against the `eAttachment` names (`FUN_008cdfb0`'s `strtok(token, "_")`), so `grip_smg` is
/// the grip whether or not an attachment asset of that name exists.
pub fn listed(item: &Item) -> Vec<usize> {
    let mut out = Vec::new();
    for t in &item.attachments {
        if let Some(k) = kind_of(t.split('_').next().unwrap_or(t)).filter(|&k| k > 0) {
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// The kinds an item lists that its variant has (`FUN_008e7fd0` drops one it lacks), ascending.
pub fn offered(item: &Item, v: &Variant, _atts: &BTreeMap<String, Attachment>) -> Vec<usize> {
    let mut kinds: Vec<usize> = listed(item).into_iter().filter(|&k| k < KINDS && v.attachment(k).is_some()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// Every combination a class can carry on `item` (the bare weapon first): pairwise compatible, at
/// most the slot's count, each with the wildcards its size needs.
pub fn combos(item: &Item, kinds: &[usize], t: &Table) -> Vec<Combo> {
    let (base, extra, card) = match item.slot.as_str() {
        "primary" => (2, 1, "primarygunfighter"),
        "secondary" => (1, 1, "secondarygunfighter"),
        _ => (0, 0, ""),
    };
    let max = base + extra;
    let mut out = vec![Combo { kinds: Vec::new(), requires: Vec::new() }];
    let mut pick = Vec::new();
    fn walk(kinds: &[usize], from: usize, max: usize, t: &Table, pick: &mut Vec<usize>, found: &mut Vec<Vec<usize>>) {
        for i in from..kinds.len() {
            let k = kinds[i];
            if pick.iter().all(|&p| t.compatible(p, k)) {
                pick.push(k);
                found.push(pick.clone());
                if pick.len() < max {
                    walk(kinds, i + 1, max, t, pick, found);
                }
                pick.pop();
            }
        }
    }
    let mut found = Vec::new();
    if max > 0 {
        walk(kinds, 0, max, t, &mut pick, &mut found);
    }
    for kinds in found {
        let requires = if kinds.len() > base { vec![card] } else { Vec::new() };
        out.push(Combo { kinds, requires });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| s.to_string()).collect()
    }

    fn att(kind: &str, point: &str, compat: &str) -> Vec<String> {
        let mut r = vec![String::new(); 20];
        r[1] = point.into();
        r[2] = "attachment".into();
        r[4] = kind.into();
        r[11] = compat.into();
        r
    }

    #[test]
    fn items_skip_negative_costs_and_non_weapons() {
        let mut a = vec![String::new(); 19];
        a[0] = "26".into();
        a[2] = "weapon_assault".into();
        a[4] = "an94".into();
        a[8] = "reflex acog".into();
        a[12] = "1".into();
        a[13] = "primary".into();
        let mut dw = a.clone();
        dw[4] = "kard_dw".into();
        dw[12] = "-1".into();
        let perk = row(&["160", "", "specialty", "", "PERKS_X", "", "", "", "", "", "", "", "1", "specialty1"]);
        let got = items(&[a, dw, perk]);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].variant.as_str(), got[0].slot.as_str()), ("an94_mp", "primary"));
        assert_eq!(got[0].attachments, vec!["reflex", "acog"]);
    }

    #[test]
    fn compatibility_must_be_mutual_and_counts_follow_the_slot() {
        let t = table(&[
            att("acog", "top", "grip gl"),
            att("reflex", "top", "grip gl"),
            att("grip", "bottom", "acog reflex"),
            // gl lists acog but not grip: grip+gl is refused although grip lists... nothing.
            att("gl", "bottom", "acog"),
        ]);
        let (acog, reflex, grip, gl) = (1, 20, 12, 11);
        assert!(t.compatible(acog, grip) && t.compatible(acog, gl));
        assert!(!t.compatible(reflex, gl), "reflex lists gl but gl does not list reflex");
        assert!(!t.compatible(acog, reflex) && !t.compatible(grip, gl));
        let item = Item { reference: "x".into(), variant: "x_mp".into(), group: "weapon_assault".into(), slot: "primary".into(), cost: 1, attachments: vec![], index: 1 };
        let keys: Vec<String> = combos(&item, &[acog, gl, grip, reflex], &t).iter().map(|c| c.key()).collect();
        assert_eq!(keys, vec!["", "acog", "acog+gl", "acog+grip", "gl", "grip", "grip+reflex", "reflex"]);
        let sec = Item { slot: "secondary".into(), ..item.clone() };
        let c = combos(&sec, &[acog, grip], &t);
        assert_eq!(c.iter().map(|c| (c.key(), c.requires.clone())).collect::<Vec<_>>(), vec![
            (String::new(), vec![]),
            ("acog".into(), vec![]),
            ("acog+grip".into(), vec!["secondarygunfighter"]),
            ("grip".into(), vec![]),
        ]);
        let nade = Item { slot: "primarygrenade".into(), ..item };
        assert_eq!(combos(&nade, &[acog], &t).len(), 1);
    }

    #[test]
    fn a_token_is_its_text_before_the_first_underscore() {
        let item = Item {
            reference: "mp7".into(),
            variant: "mp7_mp".into(),
            group: "weapon_smg".into(),
            slot: "primary".into(),
            cost: 1,
            attachments: ["reflex", "grip_smg", "silencer_smg", "rf_fast", "bogus_x", "grip"].map(String::from).to_vec(),
            index: 13,
        };
        // grip (12) once, in its first place; an unknown name is no attachment.
        assert_eq!(listed(&item), vec![20, 12, 23, 21]);
    }

    #[test]
    fn dual_wield_gives_the_underscore_variant() {
        let item = Item { reference: "fnp45".into(), variant: "fnp45_mp".into(), group: "weapon_pistol".into(), slot: "secondary".into(), cost: 1, attachments: vec![], index: 3 };
        let dw = Combo { kinds: vec![DW], requires: vec![] };
        assert_eq!(given(&item, &dw), ("fnp45_dw_mp".to_string(), String::new()));
        let fmj = Combo { kinds: vec![10], requires: vec![] };
        assert_eq!(given(&item, &fmj), ("fnp45_mp".to_string(), "fmj".to_string()));
    }

    #[test]
    fn a_primary_third_attachment_needs_primary_gunfighter() {
        let t = table(&[att("acog", "top", "grip fmj"), att("grip", "bottom", "acog fmj"), att("fmj", "gunperk", "acog grip")]);
        let item = Item { reference: "x".into(), variant: "x_mp".into(), group: "weapon_assault".into(), slot: "primary".into(), cost: 1, attachments: vec![], index: 1 };
        let c = combos(&item, &[1, 10, 12], &t);
        let three = c.iter().find(|c| c.kinds.len() == 3).unwrap();
        assert_eq!((three.key(), three.requires.clone()), ("acog+fmj+grip".to_string(), vec!["primarygunfighter"]));
    }
}
