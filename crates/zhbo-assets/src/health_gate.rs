//! The General's health over the players' own installs (real-install, `--ignored`):
//!
//! * **BO2's weapons** — every damage path of every converted weapon of the roster (each gun's
//!   round at every band of its range curve, body and head; the melee blow; every launcher's,
//!   lethal's and equipment's impact and blast at the centre and the edge) — kill a General in
//!   BO2's own shots against `playerMaxHealth` (`ceil(100 / idamage)`, BO2's integer hit written
//!   out here from its engine: t6mp `FUN_008edb80` range curve `(int)`, `FUN_005cfc60`
//!   `(int)(damage * mult)`, `FUN_005f05c0` location multiplier floored at 1, `FUN_005f2130`
//!   radius damage `(int)`), at a hero's health, his veterancy's and the old 500, through the core's
//!   own conversion ([`zhbo_core::damage::bullet_damage`], [`zhbo_core::damage::to_zh`]).
//! * **Zero Hour's weapons** — every `Weapon` of the install's `Weapon.ini` (its units, its
//!   structures, its superweapons) kills the converted General in as many hits as each of Zero
//!   Hour's heroes (`zhbo_core::names::TOUGHNESS_HEROES`), by each one's health and armour.
//!
//! `ZHBO_HEALTH_TABLE=<dir>` writes both sweeps as Markdown tables (old and new shots-to-kill).

#[cfg(test)]
mod tests {
    use crate::toughness::{self, Armor};
    use std::fmt::Write as _;
    use zhbo_core::damage::{bullet_damage, idamage, to_zh};
    use zhbo_core::host::{objflags, ObjInfo};
    use zhbo_core::hitloc;
    use zhbo_core::tuning::Tuning;
    use zhbo_core::units::{inches, BO2_TO_ZH_DAMAGE};
    use zhbo_core::weapons::{catalog, WeaponDef};

    /// The General's health before this change: `MaxHealth 500`, every BO2 hit x 1.5 unrounded.
    const OLD_HEALTH: f32 = 500.0;

    fn general(max_health: f32) -> ObjInfo {
        ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL, health: max_health, max_health, ..Default::default() }
    }

    /// Hits of `zh` Zero Hour damage until `max_health` is gone (Zero Hour's body dies at 0).
    fn hits(max_health: f32, zh: f32) -> u32 {
        if zh <= 0.0 {
            return 0;
        }
        let (mut health, mut n) = (max_health, 0);
        while health > 0.0 && n < 100_000 {
            health -= zh;
            n += 1;
        }
        n
    }

    /// BO2's shots against its 100-health player for a whole hit (none for a hit under 1).
    fn bo2_shots(t: &Tuning, hit: f32) -> u32 {
        if hit < 1.0 {
            0
        } else {
            (t.player_max_health / hit).ceil() as u32
        }
    }

    /// The old model's shots: the unrounded hit x 1.5 against 500.
    fn old_shots(bo2: f32) -> u32 {
        hits(OLD_HEALTH, bo2 * BO2_TO_ZH_DAMAGE)
    }

    /// The bands of a gun's range curve: each used point's start, and the midpoint to the next
    /// (inches).
    fn bands(d: &WeaponDef) -> Vec<(String, f32)> {
        let used: Vec<f32> = (0..6).filter(|&i| i == 0 || d.damage_range[i] > 0.0).map(|i| d.damage_range[i]).collect();
        let mut out = Vec::new();
        for (k, &r) in used.iter().enumerate() {
            out.push((format!("{r:.0}in"), r));
            if let Some(&next) = used.get(k + 1) {
                if next > r {
                    out.push((format!("{:.0}in", (r + next) / 2.0), (r + next) / 2.0));
                }
            }
        }
        out
    }

    /// The healths a General may have: the converted hero health (the install's, else the stock
    /// heroes' 200), Zero Hour's veterancy bonuses (`GameData.ini` `HealthBonus_*` 120/130/150 %),
    /// and the old 500.
    fn healths() -> Vec<f32> {
        let hero = std::env::var("ZHBO_ZH_DIR").ok().and_then(|zh| toughness::from_install(std::path::Path::new(&zh), crate::generals::base_ini()).ok()).map_or(200.0, |t| t.health);
        vec![hero, hero * 1.2, hero * 1.3, hero * 1.5, OLD_HEALTH]
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR; ZHBO_ZH_DIR for the converted health); converts the weapon table"]
    fn the_install_roster_kills_a_general_in_bo2s_shots() {
        let bo2_dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        crate::test_catalog::install_shipped(&bo2_dir);
        let t = Tuning::default();
        let healths = healths();
        let mut table = String::from("| weapon | path | where | BO2 hit | old shots (500, x1.5) | new shots | BO2 shots |\n|---|---|---|---|---|---|---|\n");
        let (mut checked, mut failures) = (0usize, Vec::new());
        let mut check = |id: &str, path: &str, at: &str, bo2_hit: f32, old: u32, zh: &dyn Fn(&ObjInfo) -> f32| {
            let want = bo2_shots(&t, bo2_hit);
            for &h in &healths {
                let got = hits(h, zh(&general(h)));
                checked += 1;
                if got != want {
                    failures.push(format!("{id} {path} {at}: {got} shots at {h} health, BO2 {want} (hit {bo2_hit})"));
                }
            }
            let _ = writeln!(table, "| {id} | {path} | {at} | {bo2_hit} | {old} | {} | {want} |", hits(healths[0], zh(&general(healths[0]))));
        };
        let defs: Vec<&'static WeaponDef> = catalog().all().to_vec();
        let mut weapons = 0;
        for d in &defs {
            let mut any = false;
            if d.class.is_gun() && d.damage[0] > 0.0 && d.explosion_inner_damage <= 0.0 {
                any = true;
                for (name, r) in bands(d) {
                    let range = d.damage_at(inches(r));
                    for (loc_name, loc) in [("body", hitloc::TORSO_UPPER), ("head", hitloc::HEAD)] {
                        let at = d.location_multipliers[loc as usize];
                        // BO2's integer hit: the range curve's (under 1 nothing reaches the
                        // callback, `FUN_005f05c0`), then the location's (at least 1).
                        let hit = if idamage(range) >= 1.0 { (idamage(range) * at).trunc().max(1.0) } else { 0.0 };
                        check(&d.id, "round", &format!("{name} {loc_name}"), hit, old_shots(range * at), &|g| bullet_damage(&t, Some(g), range, 1.0, at, 1.0, 1.0));
                    }
                }
            }
            // The blow of whatever is the melee weapon (`Arsenal::melee_weapon`: the class knife,
            // the shield's bash, a gun's own melee def).
            if d.melee_damage > 0.0 {
                any = true;
                check(&d.id, "melee", "blow", idamage(d.melee_damage), old_shots(d.melee_damage), &|g| to_zh(&t, g, d.melee_damage));
            }
            if d.explosion_inner_damage > 0.0 {
                any = true;
                if d.damage[0] > 0.0 {
                    check(&d.id, "impact", "direct", idamage(d.damage[0]), old_shots(d.damage[0]), &|g| to_zh(&t, g, d.damage[0]));
                }
                let edge = d.explosion_damage_at(d.explosion_radius);
                for (at, v) in [("blast centre", d.explosion_inner_damage), ("blast edge", edge)] {
                    check(&d.id, "blast", at, idamage(v), old_shots(v), &|g| to_zh(&t, g, v));
                }
            } else if !d.class.is_gun() && d.damage[0] > 0.0 {
                // A thrown blade or a stuck piece striking (`MOD_IMPACT`).
                any = true;
                check(&d.id, "impact", "strike", idamage(d.damage[0]), old_shots(d.damage[0]), &|g| to_zh(&t, g, d.damage[0]));
            }
            weapons += any as usize;
        }
        if let Ok(dir) = std::env::var("ZHBO_HEALTH_TABLE") {
            std::fs::write(std::path::Path::new(&dir).join("bo2-shots-to-kill.md"), &table).unwrap();
        }
        eprintln!("{weapons} weapons, {checked} cases at {healths:?} health");
        assert!(weapons > 50, "the roster: {weapons}");
        assert!(failures.is_empty(), "{} of {checked}:\n{}", failures.len(), failures.join("\n"));
    }

    /// One `Weapon` of `Weapon.ini`: its name, `PrimaryDamage` and `DamageType`.
    fn zh_weapons(weapon_ini: &str) -> Vec<(String, f32, String)> {
        let mut out = Vec::new();
        let mut cur: Option<(String, f32, String)> = None;
        for line in weapon_ini.lines() {
            let l = line.split(';').next().unwrap_or("").trim();
            if !line.starts_with([' ', '\t']) {
                if let Some(name) = l.strip_prefix("Weapon ") {
                    cur = Some((name.trim().to_string(), 0.0, "EXPLOSION".into()));
                    continue;
                }
                if l == "End" {
                    out.extend(cur.take());
                    continue;
                }
            }
            let Some(c) = cur.as_mut() else { continue };
            if let Some((k, v)) = l.split_once('=') {
                match k.trim() {
                    "PrimaryDamage" => c.1 = v.trim().parse().unwrap_or(0.0),
                    "DamageType" => c.2 = v.trim().to_string(),
                    _ => {}
                }
            }
        }
        out
    }

    #[test]
    #[ignore = "reads this machine's Zero Hour install (ZHBO_ZH_DIR)"]
    fn zero_hours_weapons_kill_the_general_as_they_kill_its_heroes() {
        let zh = std::path::PathBuf::from(std::env::var("ZHBO_ZH_DIR").expect("ZHBO_ZH_DIR"));
        let tough = toughness::from_install(&zh, crate::generals::base_ini()).expect("the heroes");
        eprintln!("{}", tough.summary());
        assert_eq!(tough.heroes.iter().map(|h| h.object.as_str()).collect::<Vec<_>>(), zhbo_core::names::TOUGHNESS_HEROES);
        let mean = tough.heroes.iter().map(|h| h.max_health).sum::<f32>() / tough.heroes.len() as f32;
        assert_eq!(tough.health, mean * Tuning::default().general_hero_toughness);
        // The converter writes it into every General object.
        let objects: Vec<&str> = crate::generals::FACTIONS.iter().map(|f| f.object).collect();
        let ini = toughness::apply(crate::generals::base_ini(), &objects, tough.health).unwrap();
        for o in &objects {
            assert_eq!(toughness::max_health(&toughness::object_block(&ini, "Object", o).unwrap()), Some(tough.health), "{o}");
        }
        let inis = toughness::install_inis(&zh, "data\\ini\\").unwrap();
        let text = |name: &str| inis.iter().find(|(p, _)| p.eq_ignore_ascii_case(name)).map(|(_, t)| t.clone()).unwrap_or_else(|| panic!("{name}"));
        let armor_ini = text("Data\\INI\\Armor.ini");
        let general_armor = Armor::parse(&armor_ini, &tough.armor).expect("the General's armour");
        let hero_armor: Vec<Armor> = tough.heroes.iter().map(|h| Armor::parse(&armor_ini, &h.armor).expect("a hero's armour")).collect();
        let weapons = zh_weapons(&text("Data\\INI\\Weapon.ini"));
        assert!(weapons.len() > 200, "{}", weapons.len());
        let mut table = format!("| Zero Hour weapon | damage | type | old General (500) | General ({}) | {} |\n|---|---|---|---|---|---|\n", tough.health, tough.heroes.iter().map(|h| format!("{} ({})", h.object, h.max_health)).collect::<Vec<_>>().join(" | "));
        table = table.replacen("|---|---|---|---|---|---|", &format!("|---|---|---|---|---|{}", "---|".repeat(tough.heroes.len())), 1);
        let mut compared = 0;
        for (name, damage, kind) in &weapons {
            let g = general_armor.share(kind) * damage;
            if g <= 0.0 {
                continue;
            }
            compared += 1;
            let general = hits(tough.health, g);
            let heroes: Vec<u32> = tough.heroes.iter().zip(&hero_armor).map(|(h, a)| hits(h.max_health, a.share(kind) * damage)).collect();
            if (tough.scale - 1.0).abs() < 1e-6 {
                assert!(heroes.iter().all(|&n| n == general), "{name} ({damage} {kind}): General {general}, heroes {heroes:?}");
            }
            let _ = writeln!(table, "| {name} | {damage} | {kind} | {} | {general} | {} |", hits(OLD_HEALTH, g), heroes.iter().map(u32::to_string).collect::<Vec<_>>().join(" | "));
        }
        if let Ok(dir) = std::env::var("ZHBO_HEALTH_TABLE") {
            std::fs::write(std::path::Path::new(&dir).join("zh-hits-to-kill.md"), &table).unwrap();
        }
        eprintln!("{compared} Zero Hour weapons compared");
        assert!(compared > 200);
    }
}
