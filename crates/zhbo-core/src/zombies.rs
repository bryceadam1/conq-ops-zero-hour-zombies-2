//! BO2 Zombies' rules that its map scripts hard-code, from `data/ConquerOps/zombies.ini` (each row
//! cites the GSC line it transcribes), and the rule table the install itself carries
//! (`mp/zombiemode.csv`, which the converter ships as is and [`RuleTable`] reads).
//!
//! The weapon roster is TranZit's (`zm_transit.gsc` `include_weapons` / `custom_add_weapons`): which
//! weapons the mode includes, which the Mystery Box offers and how often, what each Pack-a-Punches
//! into, what the wall charges and how many may be out at once.

/// The shipped file (the converter's and the tests' copy; the game reads the same text from
/// `names::ZOMBIES_PATH` in the mod's archive).
pub const SHIPPED: &str = include_str!("../../../data/ConquerOps/zombies.ini");

/// One weapon of the roster ([`parse`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ZmWeapon {
    pub id: String,
    /// `include_weapon`'s `in_box`: the Mystery Box offers it.
    pub in_box: bool,
    /// `add_zombie_weapon`'s `upgrade_name`: what Pack-a-Punch makes of it.
    pub upgrade: Option<String>,
    /// `add_zombie_weapon`'s `cost`, before `mp/zombiemode.csv` overrides it ([`ZmWeapon::cost`]).
    pub script_cost: Option<u32>,
    /// `add_zombie_weapon`'s `ammo_cost`, when the script gives one.
    pub script_ammo_cost: Option<u32>,
    /// `add_limited_weapon`'s amount: how many may be out at once (0: never from the box).
    pub limit: Option<u32>,
    /// The box's weighting: 1 (`default_weighting_func`) or 0.5 (`less_than_normal`).
    pub box_weight: f32,
}

/// BO2's `round_up_to_ten` (`_zm_utility.gsc`): the next multiple of ten, a multiple staying.
pub fn round_up_to_ten(v: i64) -> i64 {
    let r = v - v.rem_euclid(10);
    if r < v {
        r + 10
    } else {
        r
    }
}

impl ZmWeapon {
    /// What the wall charges (`add_zombie_weapon`, `_zm_weapons.gsc:526-531`): the rule table's
    /// first column, rounded up to ten, when it has a row for the weapon, else the script's cost.
    pub fn cost(&self, rules: &RuleTable) -> Option<u32> {
        match rules.int(&self.id, 1) {
            Some(c) => Some(round_up_to_ten(c) as u32),
            None => self.script_cost,
        }
    }

    /// What its ammo costs at the wall (`_zm_weapons.gsc:533-534, 559-560`): the rule table's
    /// second column rounded up to ten, else the script's, else half the cost rounded up to ten.
    pub fn ammo_cost(&self, rules: &RuleTable) -> Option<u32> {
        if let Some(c) = rules.int(&self.id, 2) {
            return Some(round_up_to_ten(c) as u32);
        }
        if let Some(c) = self.script_ammo_cost {
            return Some(c);
        }
        // BO2 halves the script's cost argument (`int( cost * 0.5 )`), not the table's.
        self.script_cost.map(|c| round_up_to_ten((c / 2) as i64) as u32)
    }
}

fn field<T: std::str::FromStr>(s: &str) -> Option<T> {
    let s = s.trim();
    if s == "-" {
        None
    } else {
        s.parse().ok()
    }
}

/// The `[Weapons]` rows of a `zombies.ini` text, in file order; malformed rows are skipped.
pub fn parse(text: &str) -> Vec<ZmWeapon> {
    let mut out = Vec::new();
    let mut in_weapons = false;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_weapons = line.eq_ignore_ascii_case("[Weapons]");
            continue;
        }
        if !in_weapons {
            continue;
        }
        let Some((id, rest)) = line.split_once('=') else { continue };
        let f: Vec<&str> = rest.split(',').collect();
        if f.len() != 6 {
            continue;
        }
        let Some(in_box) = field::<u8>(f[0]) else { continue };
        out.push(ZmWeapon {
            id: id.trim().to_string(),
            in_box: in_box != 0,
            upgrade: field::<String>(f[1]),
            script_cost: field(f[2]),
            script_ammo_cost: field(f[3]),
            limit: field(f[4]),
            box_weight: field(f[5]).unwrap_or(1.0),
        });
    }
    out
}

/// The shipped roster.
pub fn roster() -> Vec<ZmWeapon> {
    parse(SHIPPED)
}

/// A survivor General's kit at spawn, from a `zombies.ini` text's `[Survivor]` section (BO2's
/// `spawned_player`): the start weapon alone (no second gun, no held knife), the lethal with
/// its count, the melee weapon. A key the section lacks stays empty.
pub fn parse_survivor_kit(text: &str) -> crate::weapons::Loadout {
    let mut kit = crate::weapons::Loadout::unarmed();
    kit.melee.clear();
    kit.held_knife = false;
    let mut in_section = false;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[Survivor]");
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_section) else { continue };
        let value = value.trim().to_string();
        match key.trim() {
            "StartWeapon" => kit.primary = value,
            "Lethal" => kit.lethal = value,
            "LethalCount" => kit.lethal_count = value.parse().ok(),
            "Tactical" => kit.tactical = value,
            "Melee" => kit.melee = value,
            _ => {}
        }
    }
    kit
}

/// The shipped survivor kit.
pub fn survivor_kit() -> crate::weapons::Loadout {
    parse_survivor_kit(SHIPPED)
}

/// BO2's `mp/zombiemode.csv` (`common_zm`): Zombies' rule table, a name per row then one value per
/// difficulty (`NAME,EASY,MEDIUM,HARD,VETERAN`). Scripts read it with `tablelookup( table, 0,
/// name, column )`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleTable {
    rows: Vec<Vec<String>>,
}

impl RuleTable {
    pub fn parse(csv: &str) -> RuleTable {
        RuleTable { rows: csv.lines().map(|l| l.split(',').map(|c| c.trim().to_string()).collect()).filter(|r: &Vec<String>| !r.is_empty() && !r[0].is_empty()).collect() }
    }

    /// `tablelookup( table, 0, name, column )`: the cell, `None` when the row is missing or the cell
    /// empty (BO2's `""`).
    pub fn cell(&self, name: &str, column: usize) -> Option<&str> {
        self.rows.iter().find(|r| r[0] == name).and_then(|r| r.get(column)).map(|s| s.as_str()).filter(|s| !s.is_empty())
    }

    /// The cell as GSC's `int()` reads it.
    pub fn int(&self, name: &str, column: usize) -> Option<i64> {
        self.cell(name, column).and_then(|s| s.parse::<f64>().ok()).map(|v| v as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_roster_parses_every_row() {
        let r = roster();
        let lines = SHIPPED.lines().filter(|l| { let l = l.trim(); !l.is_empty() && !l.starts_with(';') && !l.starts_with('[') }).count();
        assert_eq!(r.len(), lines, "a row the parser skipped");
        let ray = r.iter().find(|w| w.id == "ray_gun_zm").unwrap();
        assert!(ray.in_box);
        assert_eq!((ray.upgrade.as_deref(), ray.script_cost, ray.limit), (Some("ray_gun_upgraded_zm"), Some(10000), Some(4)));
        // Every upgrade is itself a row.
        for w in r.iter().filter_map(|w| w.upgrade.as_ref()) {
            assert!(r.iter().any(|x| &x.id == w), "{w}");
        }
    }

    #[test]
    fn a_survivor_spawns_with_the_start_weapon_alone_and_an_empty_lethal() {
        let kit = survivor_kit();
        assert_eq!((kit.primary.as_str(), kit.secondary.as_str()), ("m1911_zm", ""));
        assert_eq!((kit.lethal.as_str(), kit.lethal_count), ("frag_grenade_zm", Some(0)));
        assert_eq!((kit.tactical.as_str(), kit.melee.as_str()), ("", "knife_zm"));
        assert!(!kit.held_knife, "Zombies gives no held knife");
    }

    #[test]
    fn the_rule_table_overrides_the_scripts_cost_as_add_zombie_weapon_does() {
        let rules = RuleTable::parse("NAME,EASY,MEDIUM,HARD,VETERAN\npython_zm,2200,,,\nm16_zm,1195,605,,\n");
        let w = |id: &str, cost: Option<u32>, ammo: Option<u32>| ZmWeapon { id: id.into(), in_box: true, upgrade: None, script_cost: cost, script_ammo_cost: ammo, limit: None, box_weight: 1.0 };
        assert_eq!(w("python_zm", Some(50), None).cost(&rules), Some(2200));
        assert_eq!(w("m16_zm", Some(1200), None).cost(&rules), Some(1200), "rounded up to ten");
        assert_eq!(w("m16_zm", Some(1200), None).ammo_cost(&rules), Some(610));
        assert_eq!(w("galil_zm", Some(50), None).cost(&rules), Some(50), "no row: the script's");
        assert_eq!(w("mp5k_zm", Some(1000), None).ammo_cost(&rules), Some(500), "half the script's cost");
        assert_eq!(w("frag_grenade_zm", Some(250), Some(250)).ammo_cost(&rules), Some(250));
        assert_eq!(round_up_to_ten(1195), 1200);
        assert_eq!(round_up_to_ten(1200), 1200);
    }
}
