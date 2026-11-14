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
    /// `add_zombie_weapon`'s hint: its wall buy's prompt key (`ZOMBIE_WEAPON_M14`).
    pub hint: Option<String>,
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
        if f.len() != 6 && f.len() != 7 {
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
            hint: f.get(6).and_then(|h| field::<String>(h)),
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

/// A `zombies.ini` text's `[Zombie]` `MeleeDamage` (BO2's `self.meleedamage`), or 0 when missing.
pub fn parse_melee_damage(text: &str) -> f32 {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[Zombie]");
            continue;
        }
        if let Some((k, v)) = line.split_once('=').filter(|_| in_section) {
            if k.trim() == "MeleeDamage" {
                return v.trim().parse().unwrap_or(0.0);
            }
        }
    }
    0.0
}

/// A zombie's swipe on a player, BO2 health (the shipped `[Zombie]` `MeleeDamage`).
pub fn melee_damage() -> f32 {
    parse_melee_damage(SHIPPED)
}

/// The shipped survivor kit.
pub fn survivor_kit() -> crate::weapons::Loadout {
    parse_survivor_kit(SHIPPED)
}

/// One wall buy: its weapon, the model shown on its wall, and the cost and prompt a melee buy names
/// itself (the rest are the roster's).
#[derive(Clone, Debug, PartialEq)]
pub struct WallBuy {
    pub weapon: String,
    pub model: String,
    pub cost: Option<u32>,
    pub hint: Option<String>,
}

impl WallBuy {
    /// Its price (the melee buy's own, else the roster's with the rule table's override).
    pub fn cost(&self) -> u32 {
        self.cost.or_else(|| roster().into_iter().find(|w| w.id == self.weapon).and_then(|w| w.cost(&rules()))).unwrap_or(0)
    }

    /// Its ammo price: the roster's, with the rule table's override.
    pub fn ammo_cost(&self) -> u32 {
        roster().into_iter().find(|w| w.id == self.weapon).and_then(|w| w.ammo_cost(&rules())).unwrap_or(0)
    }

    /// Its prompt's key (`wall_weapon_update_prompt`'s `get_weapon_hint`).
    pub fn hint(&self) -> Option<String> {
        self.hint.clone().or_else(|| roster().into_iter().find(|w| w.id == self.weapon).and_then(|w| w.hint))
    }

    /// The Pack-a-Punched weapon of its weapon (a held upgrade counts as having it).
    pub fn upgrade(&self) -> Option<String> {
        roster().into_iter().find(|w| w.id == self.weapon).and_then(|w| w.upgrade)
    }
}

/// TranZit's wall buys (`zombies.ini` `[WallBuys]`) and where the Zombies map stands them
/// (`[Layout]`).
#[derive(Clone, Debug, PartialEq)]
pub struct WallBuys {
    /// The wall buys, in the map's order.
    pub buys: Vec<WallBuy>,
    pub upgraded_ammo_cost: u32,
    /// Zero Hour units from each player start.
    pub ring: f32,
}

impl WallBuys {
    pub fn parse(text: &str) -> WallBuys {
        let mut w = WallBuys { buys: Vec::new(), upgraded_ammo_cost: 0, ring: 0.0 };
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') {
                section = line.to_ascii_lowercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match (section.as_str(), key) {
                ("[wallbuys]", "UpgradedAmmoCost") => w.upgraded_ammo_cost = value.parse().unwrap_or(0),
                ("[wallbuys]", _) => {
                    let f: Vec<&str> = value.split(',').map(str::trim).collect();
                    w.buys.push(WallBuy { weapon: key.to_string(), model: f[0].to_string(), cost: f.get(1).and_then(|c| c.parse().ok()), hint: f.get(2).map(|h| h.to_string()) });
                }
                ("[layout]", "WallBuyRing") => w.ring = value.parse().unwrap_or(0.0),
                _ => {}
            }
        }
        w
    }

    /// The shipped `zombies.ini`'s.
    pub fn get() -> &'static WallBuys {
        static W: std::sync::OnceLock<WallBuys> = std::sync::OnceLock::new();
        W.get_or_init(|| WallBuys::parse(SHIPPED))
    }
}

/// A survivor's last stand (`zombies.ini` `[LastStand]`, `_zm_laststand.gsc`).
#[derive(Clone, Debug, PartialEq)]
pub struct LastStand {
    pub bleedout_time: f32,
    pub revive_time: f32,
    /// In BO2 inches.
    pub revive_radius: f32,
    pub revive_facing: f32,
    pub pistol: String,
    pub pistol_values: Vec<String>,
}

impl LastStand {
    pub fn parse(text: &str) -> LastStand {
        let mut ls = LastStand { bleedout_time: 0.0, revive_time: 0.0, revive_radius: 0.0, revive_facing: 1.0, pistol: String::new(), pistol_values: Vec::new() };
        let mut in_section = false;
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') {
                in_section = line.eq_ignore_ascii_case("[LastStand]");
                continue;
            }
            let Some((key, value)) = line.split_once('=').filter(|_| in_section) else { continue };
            let (key, value) = (key.trim(), value.trim());
            let num = || value.parse::<f32>().unwrap_or(0.0);
            match key {
                "BleedoutTime" => ls.bleedout_time = num(),
                "ReviveTime" => ls.revive_time = num(),
                "ReviveRadius" => ls.revive_radius = num(),
                "ReviveFacing" => ls.revive_facing = num(),
                "Pistol" => ls.pistol = value.to_string(),
                "PistolValues" => ls.pistol_values = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                _ => {}
            }
        }
        ls
    }

    /// The shipped `zombies.ini`'s.
    pub fn get() -> &'static LastStand {
        static LS: std::sync::OnceLock<LastStand> = std::sync::OnceLock::new();
        LS.get_or_init(|| LastStand::parse(SHIPPED))
    }

    /// `last_stand_best_pistol` (co-op): of the pistols `held` (id, rounds left), the one ranked
    /// last in `pistol_values` (an unranked one counts as the first), skipping an empty one unless
    /// it is the default pistol; none: the default pistol.
    pub fn best_pistol<'a>(&'a self, held: &[(&'a str, u32)]) -> &'a str {
        let value = |id: &str| self.pistol_values.iter().position(|v| v == id).unwrap_or(0);
        let mut best: Option<(&str, usize)> = None;
        for &(id, rounds) in held {
            if rounds == 0 && id != self.pistol {
                continue;
            }
            // `last_stand_compare_pistols` keeps the first of equal values.
            if best.is_none_or(|(_, v)| value(id) > v) {
                best = Some((id, value(id)));
            }
        }
        best.map_or(self.pistol.as_str(), |(id, _)| id)
    }
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

/// The rule table the game loaded (`zhbo_load_zombie_rules`, from the archive); empty until then.
static RULES: std::sync::RwLock<Option<RuleTable>> = std::sync::RwLock::new(None);

/// Install the archive's rule table (the bridge reads `names::ZOMBIE_RULES_PATH`). Returns its row
/// count.
pub fn install_rules(csv: &str) -> usize {
    let t = RuleTable::parse(csv);
    let n = t.rows.len();
    *RULES.write().unwrap_or_else(|e| e.into_inner()) = Some(t);
    n
}

/// The installed rule table (empty if none was loaded).
pub fn rules() -> RuleTable {
    RULES.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// A `[Rounds]` key of a `zombies.ini` text.
fn rounds_value(text: &str, key: &str) -> Option<f32> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[Rounds]");
            continue;
        }
        if let Some((k, v)) = line.split_once('=').filter(|_| in_section) {
            if k.trim() == key {
                return v.trim().parse().ok();
            }
        }
    }
    None
}

/// BO2's `level.zombie_vars` the round engine and the score read: each `set_zombie_var( name,
/// default, is_float, column )` (`_zm_utility.gsc:2428-2453`) — the rule table's cell in the
/// difficulty's column when it has one (`int()` unless `is_float`), else the script's default
/// (`_zm.gsc:855-875`) — and the `[Rounds]` constants the script hard-codes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vars {
    pub health_start: f32,
    pub health_increase: f32,
    pub health_increase_multiplier: f32,
    pub spawn_delay: f32,
    pub move_speed_multiplier: i32,
    pub max_ai: i32,
    pub ai_per_player: i32,
    pub ai_limit: u32,
    pub between_rounds: f32,
    pub spawn_delay_floor: f32,
    pub spawn_delay_decay: f32,
    /// `zombie_score_kill_<n>player`, n = 1..4.
    pub score_kill: [i32; 4],
    pub score_damage: i32,
    pub score_bonus_melee: i32,
    pub score_bonus_head: i32,
    pub score_bonus_neck: i32,
    pub score_bonus_torso: i32,
    pub start_round: u32,
    pub starting_points_per_round: i32,
    pub respawn_floor_round: u32,
    pub respawn_floor_points: i32,
    /// `penalty_downed`: the share of his points a player loses going down (`player_reduce_points`,
    /// `_zm_score.gsc:292-296`, rounded up to ten).
    pub penalty_downed: f32,
}

impl Vars {
    /// The vars over `rules` and the `zombies.ini` text `ini`.
    pub fn of(rules: &RuleTable, ini: &str) -> Vars {
        let difficulty = rounds_value(ini, "Difficulty").unwrap_or(1.0) as usize;
        let column = difficulty + 1;
        // `set_zombie_var` without a column reads column 1 (`_zm_utility.gsc:2433-2434`).
        let int = |name: &str, default: i32, col: usize| rules.cell(name, col).and_then(|c| c.parse::<f64>().ok()).map_or(default, |v| v as i32);
        let float = |name: &str, default: f32, col: usize| rules.cell(name, col).and_then(|c| c.parse::<f32>().ok()).unwrap_or(default);
        Vars {
            health_start: int("zombie_health_start", 150, column) as f32,
            health_increase: int("zombie_health_increase", 100, column) as f32,
            health_increase_multiplier: float("zombie_health_increase_multiplier", 0.1, column),
            spawn_delay: float("zombie_spawn_delay", 2.0, column),
            move_speed_multiplier: int("zombie_move_speed_multiplier", 8, column),
            max_ai: int("zombie_max_ai", 24, column),
            ai_per_player: int("zombie_ai_per_player", 6, column),
            ai_limit: rounds_value(ini, "AiLimit").unwrap_or(24.0) as u32,
            between_rounds: float("zombie_between_round_time", rounds_value(ini, "BetweenRounds").unwrap_or(10.0), 1),
            spawn_delay_floor: rounds_value(ini, "SpawnDelayFloor").unwrap_or(0.08),
            spawn_delay_decay: rounds_value(ini, "SpawnDelayDecay").unwrap_or(0.95),
            score_kill: [1, 2, 3, 4].map(|n| int(&format!("zombie_score_kill_{n}player"), 50, 1)),
            score_damage: int("zombie_score_damage_normal", 10, 1),
            score_bonus_melee: int("zombie_score_bonus_melee", 80, 1),
            score_bonus_head: int("zombie_score_bonus_head", 50, 1),
            score_bonus_neck: int("zombie_score_bonus_neck", 20, 1),
            score_bonus_torso: int("zombie_score_bonus_torso", 10, 1),
            start_round: rounds_value(ini, "StartRound").unwrap_or(1.0) as u32,
            starting_points_per_round: rounds_value(ini, "StartingPointsPerRound").unwrap_or(500.0) as i32,
            respawn_floor_round: rounds_value(ini, "RespawnFloorRound").unwrap_or(6.0) as u32,
            respawn_floor_points: rounds_value(ini, "RespawnFloorPoints").unwrap_or(1500.0) as i32,
            penalty_downed: float("penalty_downed", 0.05, column),
        }
    }

    /// Each player's points at the start (`level.player_starting_points`, `_zm.gsc:839`).
    pub fn starting_points(&self) -> i32 {
        self.start_round as i32 * self.starting_points_per_round
    }

    /// The vars over the installed rules and the shipped `zombies.ini`.
    pub fn get() -> Vars {
        Vars::of(&rules(), SHIPPED)
    }

    /// `ai_calculate_health( round )` (`_zm.gsc:3566-3584`): BO2 health of the round's zombies.
    pub fn health(&self, round: u32) -> f32 {
        let mut h = self.health_start as i64;
        for i in 2..=round.max(1) {
            if i >= 10 {
                let old = h;
                h += (h as f32 * self.health_increase_multiplier) as i64;
                if h < old {
                    return old as f32;
                }
            } else {
                h = (h as f32 + self.health_increase) as i64;
            }
        }
        h as f32
    }

    /// `round_spawning`'s zombie total (`_zm.gsc:2914-2935`, `default_max_zombie_func`,
    /// `:2872-2894`) for `players` players.
    pub fn zombie_total(&self, round: u32, players: u32) -> u32 {
        let mut max = self.max_ai as f32;
        let mut multiplier = round as f32 / 5.0;
        if multiplier < 1.0 {
            multiplier = 1.0;
        }
        if round >= 10 {
            multiplier *= round as f32 * 0.15;
        }
        if players <= 1 {
            max += (0.5 * self.ai_per_player as f32 * multiplier) as i32 as f32;
        } else {
            max += ((players - 1) as f32 * self.ai_per_player as f32 * multiplier) as i32 as f32;
        }
        let max = max as i32;
        let share = match round {
            0 | 1 => 0.25,
            2 => 0.3,
            3 => 0.5,
            4 => 0.7,
            5 => 0.9,
            _ => return max.max(0) as u32,
        };
        ((max as f32 * share) as i32).max(0) as u32
    }

    /// `level.zombie_move_speed` in round `round`: `round * zombie_move_speed_multiplier` set at
    /// the start (`_zm.gsc:904-906`) and, at each round's end, from the round just played before it
    /// counts up (`round_think`, `:3503-3508`).
    pub fn move_speed(&self, round: u32) -> i32 {
        round.saturating_sub(1).max(1) as i32 * self.move_speed_multiplier
    }

    /// `zombie_spawn_delay` in round `round`: shrunk by the decay at each round's end while above
    /// the floor, else set to it (`round_think`, `:3496-3501`).
    pub fn spawn_delay_in(&self, round: u32) -> f32 {
        let mut d = self.spawn_delay;
        for _ in 1..round.max(1) {
            if d > self.spawn_delay_floor {
                d *= self.spawn_delay_decay;
            } else if d < self.spawn_delay_floor {
                d = self.spawn_delay_floor;
            }
        }
        d
    }
}

/// A zombie's run cycle (`set_run_speed`, `_zm_utility.gsc:214-224`): `rand` drawn in
/// `[move_speed, move_speed + 35)`; up to 35 walks, up to 70 runs, beyond sprints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunCycle {
    Walk,
    Run,
    Sprint,
}

impl RunCycle {
    pub fn of(rand: i32) -> RunCycle {
        if rand <= 35 {
            RunCycle::Walk
        } else if rand <= 70 {
            RunCycle::Run
        } else {
            RunCycle::Sprint
        }
    }

    /// The zombie object of this cycle (`zhbo_assets::zombies::unit::CYCLES`).
    pub fn object(self) -> &'static str {
        match self {
            RunCycle::Walk => "ZHBO_Zombie_Walk",
            RunCycle::Run => "ZHBO_Zombie_Run",
            RunCycle::Sprint => "ZHBO_Zombie_Sprint",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wall_buys_are_tranzits_ten_with_their_prompts_and_prices() {
        let w = WallBuys::get();
        assert_eq!(w.buys.len(), 10);
        assert_eq!((w.buys[3].weapon.as_str(), w.buys[3].model.as_str(), w.buys[3].cost()), ("m14_zm", "t6_wpn_ar_m14_world", 500));
        assert_eq!(w.upgraded_ammo_cost, 4500);
        assert!(w.ring > 0.0);
        for b in &w.buys {
            assert!(b.hint().is_some_and(|h| h.starts_with("ZOMBIE_WEAPON_")), "{}'s prompt", b.weapon);
            assert!(b.cost() > 0, "{}'s price", b.weapon);
        }
        let bowie = w.buys.iter().find(|b| b.weapon == "bowie_knife_zm").unwrap();
        assert_eq!((bowie.cost(), bowie.hint().as_deref()), (3000, Some("ZOMBIE_WEAPON_BOWIE_BUY")));
    }

    #[test]
    fn the_last_stand_reads_its_rules_and_picks_the_best_pistol_with_ammo() {
        let ls = LastStand::get();
        assert_eq!((ls.bleedout_time, ls.revive_time, ls.revive_radius, ls.pistol.as_str()), (45.0, 3.0, 75.0, "m1911_zm"));
        assert_eq!(ls.pistol_values.len(), 32);
        assert_eq!(ls.best_pistol(&[]), "m1911_zm");
        assert_eq!(ls.best_pistol(&[("m1911_zm", 0), ("python_zm", 6)]), "python_zm");
        assert_eq!(ls.best_pistol(&[("python_zm", 0)]), "m1911_zm", "an empty pistol is passed over");
        assert_eq!(ls.best_pistol(&[("ray_gun_zm", 20), ("judge_zm", 5)]), "ray_gun_zm");
        assert!((Vars::get().penalty_downed - 0.05).abs() < 1e-6);
    }

    #[test]
    fn the_shipped_roster_parses_every_row() {
        let r = roster();
        let weapons = SHIPPED.split("[Weapons]").nth(1).unwrap_or("");
        let weapons = weapons.split("\n[").next().unwrap_or("");
        let lines = weapons.lines().filter(|l| { let l = l.trim(); !l.is_empty() && !l.starts_with(';') }).count();
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
    fn the_rounds_follow_bo2s_formulas() {
        let csv = "NAME,EASY,MEDIUM,HARD,VETERAN\nzombie_health_start,80,150,150,150\nzombie_health_increase,80,100,100,100\nzombie_health_increase_multiplier,0.075,0.1,0.1,0.1\nzombie_max_ai,24,24,24,24\nzombie_ai_per_player,6,6,6,6\nzombie_move_speed_multiplier,6,8,8,8\nzombie_spawn_delay,2.0,2.0,2.0,2.0\nzombie_score_kill_1player,50,,,\n";
        let v = Vars::of(&RuleTable::parse(csv), SHIPPED);
        // The "original" column: 150, +100 a round to 9, then +10% a round.
        assert_eq!((v.health(1), v.health(2), v.health(9)), (150.0, 250.0, 950.0));
        assert_eq!(v.health(10), 1045.0);
        // Round 1 alone: 24 + int(0.5 * 6 * 1) = 27, a quarter of it.
        assert_eq!(v.zombie_total(1, 1), 6);
        assert_eq!(v.zombie_total(1, 4), 10, "24 + 3 * 6 = 42, a quarter");
        assert_eq!(v.zombie_total(6, 1), 27, "round 6: multiplier 1.2: 24 + int(3.6) = 27, all of it");
        assert_eq!((v.move_speed(1), v.move_speed(2), v.move_speed(5)), (8, 8, 32));
        assert!((v.spawn_delay_in(1) - 2.0).abs() < 1e-6 && (v.spawn_delay_in(3) - 2.0 * 0.95 * 0.95).abs() < 1e-5);
        assert_eq!((RunCycle::of(35), RunCycle::of(36), RunCycle::of(71)), (RunCycle::Walk, RunCycle::Run, RunCycle::Sprint));
        assert_eq!((v.score_kill[0], v.score_damage, v.score_bonus_melee), (50, 10, 80));
        assert_eq!(v.ai_limit, 24);
        assert_eq!(v.starting_points(), 500);
        assert_eq!((v.respawn_floor_round, v.respawn_floor_points), (6, 1500));
    }

    #[test]
    fn a_zombies_swipe_is_bo2s_sixty() {
        assert_eq!(melee_damage(), 60.0);
        // On a General it is that share of BO2's player health of his own (damage::general_zh).
        let t = crate::tuning::Tuning::default();
        let hit = crate::damage::general_zh(&t, melee_damage(), 300.0);
        assert!(hit > 0.6 * 300.0 && hit < 0.61 * 300.0, "{hit}");
    }

    #[test]
    fn the_rule_table_overrides_the_scripts_cost_as_add_zombie_weapon_does() {
        let rules = RuleTable::parse("NAME,EASY,MEDIUM,HARD,VETERAN\npython_zm,2200,,,\nm16_zm,1195,605,,\n");
        let w = |id: &str, cost: Option<u32>, ammo: Option<u32>| ZmWeapon { id: id.into(), in_box: true, upgrade: None, script_cost: cost, script_ammo_cost: ammo, limit: None, box_weight: 1.0, hint: None };
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
