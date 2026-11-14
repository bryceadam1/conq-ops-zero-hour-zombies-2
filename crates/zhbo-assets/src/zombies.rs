//! BO2 Zombies for Conquer Ops: TranZit's weapons (the only Zombies map of a base install) and
//! Zombies' rule table, from the player's own install, beside the multiplayer weapons.
//!
//! Which weapons is TranZit's roster (`zhbo_core::zombies`, transcribed from `zm_transit.gsc`); what
//! each one is comes from its definition in TranZit's pool (`zhbo_formats::weapon::ZM_TRANSIT_POOL`),
//! turned into the core's [`WeaponDef`] by the same [`crate::weapons::to_def`] as every MP weapon.
//! The art passes treat them as the rest of the converted base set: they come after the MP
//! weapons in the converted list, so each keeps the MP viewmodel numbering intact.

use std::collections::HashMap;
use std::path::Path;

use zhbo_core::weapons::WeaponDef;
use zhbo_formats::weapon::Weapon;

use crate::paks::ZmPool;

/// Where the converter writes Zombies' rule table in the archive (`zhbo_core::zombies::RuleTable`).
pub const RULES_PATH: &str = "Data\\ConquerOps\\zombiemode.csv";

/// TranZit's pool and what the converter takes from it.
pub(crate) struct Zombies {
    pub(crate) pool: ZmPool,
    /// Every weapon of the pool (the art passes look a weapon up by name here).
    pub(crate) weapons: Vec<Weapon>,
    /// The converted weapons: the roster's, then the alt modes and spawned grenades they reach, in
    /// that order, each once.
    pub(crate) ids: Vec<String>,
}

/// BO2's localized strings for the Zombies display names: every Zombies language zone TranZit's
/// classic mode loads, later ones over earlier (the patches last).
pub(crate) fn names(bo2_dir: &Path) -> HashMap<String, String> {
    zhbo_formats::localize::install_strings(bo2_dir, &["code_post_gfx_zm", "common_zm", "ui_zm", "zm_transit", "so_zclassic_zm_transit", "patch_zm", "patch_ui_zm"])
}

impl Zombies {
    /// TranZit's pool from the install, with the roster's weapons and what they reach. A roster
    /// weapon the pool lacks is left out (and named in the summary by [`Zombies::missing`]).
    pub(crate) fn load(bo2_dir: &Path) -> Result<Zombies, String> {
        let pool = ZmPool::load(bo2_dir)?;
        // TranZit's attachments are its own (no Create-a-Class): its weapons carry theirs built in.
        let (weapons, _) = pool.weapons()?;
        let mut ids: Vec<String> = zhbo_core::zombies::roster().into_iter().map(|w| w.id).filter(|id| weapons.iter().any(|w| &w.name == id)).collect();
        // The alt modes their attachments add (the Pack-a-Punched M16's launcher) and the grenades
        // their projectiles become, as the MP table reaches them (`weapons::table_with_metadata`).
        let mut k = 0;
        while k < ids.len() {
            if let Some(w) = weapons.iter().find(|w| w.name == ids[k]) {
                let d = crate::weapons::to_def(w, &HashMap::new());
                let own_alt = (!d.alt.is_empty()).then(|| d.alt.clone());
                for extra in d.alt_modes.iter().map(|(_, alt)| alt.clone()).chain(own_alt).chain((!d.spawns.is_empty()).then(|| d.spawns.clone())) {
                    if !ids.contains(&extra) && weapons.iter().any(|w| w.name == extra) {
                        ids.push(extra);
                    }
                }
            }
            k += 1;
        }
        Ok(Zombies { pool, weapons, ids })
    }

    /// The converted weapons whose art the install cannot supply (`has_art`: the view model is
    /// found and its textures convert, as the MP items are locked without it).
    pub(crate) fn lacking_art(&self, has_art: impl Fn(&Weapon) -> bool) -> Vec<String> {
        self.ids.iter().filter(|id| self.weapons.iter().find(|w| &w.name == *id).is_some_and(|w| w.view_model.is_some() && !has_art(w))).cloned().collect()
    }

    /// Leave out the weapons [`Zombies::lacking_art`] found, each with what only it brought along
    /// (an upgrade's alt mode). Returns the weapons left out.
    pub(crate) fn leave_out(&mut self, lacking: Vec<String>) -> Vec<String> {
        // An alt mode or spawned grenade only reached through a weapon left out goes too.
        let reached_only_by = |id: &str| self.weapons.iter().filter(|w| lacking.contains(&w.name)).any(|w| w.alt_weapon.as_deref() == Some(id) || w.spawned_grenade_weapon.as_deref() == Some(id));
        let mut dropped: Vec<String> = lacking.clone();
        dropped.extend(self.ids.iter().filter(|id| !lacking.contains(id) && reached_only_by(id) && !zhbo_core::zombies::roster().iter().any(|w| &w.id == *id)).cloned());
        self.ids.retain(|id| !dropped.contains(id));
        dropped
    }

    /// The roster's weapons the install's pool does not define.
    pub(crate) fn missing(&self) -> Vec<String> {
        zhbo_core::zombies::roster().into_iter().map(|w| w.id).filter(|id| !self.weapons.iter().any(|w| &w.name == id)).collect()
    }

    /// The core's definitions of the converted weapons, named from `names` (the Zombies strings
    /// over the multiplayer ones).
    pub(crate) fn defs(&self, names: &HashMap<String, String>) -> Vec<WeaponDef> {
        self.ids.iter().filter_map(|id| self.weapons.iter().find(|w| &w.name == id)).map(|w| crate::weapons::to_def(w, names)).collect()
    }

    /// Zombies' rule table as the install has it (`mp/zombiemode.csv`, `common_zm`), back as CSV.
    pub(crate) fn rule_table(bo2_dir: &Path) -> Result<String, String> {
        let zone = crate::paks::load_zone(bo2_dir, "common_zm")?;
        let rows = zhbo_formats::stringtable::rows(&zone, "mp/zombiemode.csv").map_err(|e| e.to_string())?.ok_or("common_zm has no mp/zombiemode.csv")?;
        Ok(rows.iter().map(|r| r.join(",")).collect::<Vec<_>>().join("\n") + "\n")
    }
}

/// TranZit's zombie as Zero Hour units (`ZHBO_Zombie_{Walk,Run,Sprint}`): BO2's classic zombie
/// (`character/c_zom_zombie1_01.gsc`: `c_zom_zombie1_body01` with the first head of
/// `xmodelalias/c_zom_zombie_head_als`, attached on the body's own bones as `attach( head, "", 1 )`
/// does) skinned as one W3D model, with the clips of its animtree (`aitype/zm_transit_basic_01.gsc`).
/// BO2 moves a zombie by its clip's root motion, so each run cycle's Zero Hour speed is its clip's
/// (`zhbo_formats::xanim::root_motion`, at the General's drawn scale). BO2's zombies die as
/// ragdolls; the mod has none, so they play the animtree's standing `ch_dazed_a_death`.
pub mod unit {
    /// The body, its head and the W3D names (at most 15 characters).
    pub const BODY: &str = "c_zom_zombie1_body01";
    pub const HEAD: &str = "c_zom_zombie_head_k";
    pub const HIERARCHY: &str = "ZHBO_ZOM_SKL";
    pub const MODEL: &str = "ZHBO_ZOM_SKN";
    /// Each clip: its W3D name and BO2's.
    pub const IDLE: (&str, &str) = ("ZHBO_ZOM_IDL", "ai_zombie_idle_v1_delta");
    pub const ATTACK: (&str, &str) = ("ZHBO_ZOM_ATK", "ai_zombie_attack_v1");
    pub const DEATH: (&str, &str) = ("ZHBO_ZOM_DTH", "ch_dazed_a_death");
    /// The run cycles BO2's `set_zombie_run_cycle` picks between ("walk", "run", "sprint"): the
    /// object, its locomotor, and its clip.
    pub const CYCLES: [(&str, &str, (&str, &str)); 3] = [
        ("ZHBO_Zombie_Walk", "ZHBO_ZombieWalkLocomotor", ("ZHBO_ZOM_WLK", "ai_zombie_walk_v1")),
        ("ZHBO_Zombie_Run", "ZHBO_ZombieRunLocomotor", ("ZHBO_ZOM_RUN", "ai_zombie_run_v2")),
        ("ZHBO_Zombie_Sprint", "ZHBO_ZombieSprintLocomotor", ("ZHBO_ZOM_SPR", "ai_zombie_sprint_v1")),
    ];
    pub const WEAPON: &str = "ZHBO_ZombieSwipe";
    /// The archive's generated INIs.
    pub const OBJECT_INI: &str = "Data\\INI\\Object\\ZZZ_ZHBO_Zombie.ini";
    pub const LOCOMOTOR_INI: &str = "Data\\INI\\Locomotor\\ZZZ_ZHBO_Zombie.ini";
    pub const WEAPON_INI: &str = "Data\\INI\\Weapon\\ZZZ_ZHBO_Zombie.ini";
    /// BO2's AI melee reach (`ai_meleeRange`'s 64 inches, the IW engines' default; the dogs set
    /// the same, `animscripts/dog_init.gsc:60`).
    pub const MELEE_RANGE: f32 = 64.0;
}

/// The horizontal speed of clip `name`'s root motion, inches a second (0 without root motion).
fn root_speed(zone: &zhbo_formats::t6zone::Zone, name: &str) -> Result<f32, String> {
    let anim = zhbo_formats::xanim::find(zone, name).map_err(|e| e.to_string())?;
    let keys = zhbo_formats::xanim::root_motion(zone, name).map_err(|e| e.to_string())?;
    let (Some(a), Some(b)) = (keys.first(), keys.last()) else { return Ok(0.0) };
    let secs = (b.0 - a.0) as f32 / anim.frame_rate.max(1.0);
    if secs <= 0.0 {
        return Ok(0.0);
    }
    Ok(((b.1[0] - a.1[0]).powi(2) + (b.1[1] - a.1[1]).powi(2)).sqrt() / secs)
}

/// The zombie units' art and INIs into `out`; returns a summary. `zone` holds the classic mode's
/// characters and clips (`generals::SURVIVOR_ZONE`), `materials` the zones its materials resolve
/// in.
pub(crate) fn zombie_unit(
    zone: &zhbo_formats::t6zone::Zone,
    materials: &[&zhbo_formats::t6zone::Zone],
    paks: &crate::paks::Paks,
    out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    rules: &zhbo_core::zombies::RuleTable,
) -> Result<String, String> {
    use crate::character::{self, Rig};
    use zhbo_formats::{w3d, xanim, xmodel};
    let mut body = xmodel::find(zone, unit::BODY).map_err(|e| e.to_string())?;
    let mut head = xmodel::find(zone, unit::HEAD).map_err(|e| e.to_string())?;
    let mut pool = vec![zone];
    pool.extend_from_slice(materials);
    crate::viewmodels::resolve_materials(&pool, &mut body);
    crate::viewmodels::resolve_materials(&pool, &mut head);
    let rig = Rig::new(&body, &[]);
    let mut looks = crate::textures::Looks::default();
    let body_tex = crate::textures::textures(&body, paks, out, &mut looks);
    let head_tex = crate::textures::textures(&head, paks, out, &mut looks);
    let mut meshes = character::skin(&rig, &body, "B", &body_tex);
    meshes.extend(character::skin(&rig, &head, "H", &head_tex));
    crate::textures::finish(&mut meshes, &looks);
    out.insert(format!("Art\\W3D\\{}.w3d", unit::MODEL), w3d::skin_model_with_box(unit::MODEL, unit::HIERARCHY, &meshes, None));
    out.insert(format!("Art\\W3D\\{}.w3d", unit::HIERARCHY), rig.hierarchy_file(unit::HIERARCHY));
    let mut clips: Vec<(&str, &str)> = vec![unit::IDLE, unit::ATTACK, unit::DEATH];
    clips.extend(unit::CYCLES.iter().map(|c| c.2));
    let mut attack_ms = 0.0;
    for (w3d_name, bo2_name) in &clips {
        let anim = xanim::find(zone, bo2_name).map_err(|e| e.to_string())?;
        if *bo2_name == unit::ATTACK.1 {
            attack_ms = anim.num_frames as f32 / anim.frame_rate.max(1.0) * 1000.0;
        }
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, unit::HIERARCHY, frames, fps, &channels));
    }
    // Health: BO2's round-one zombie (`zombie_health_start`, the rule table's "original" column,
    // `_zm_spawner.gsc:225`) at the exchange rate BO2 weapons hit Zero Hour units by, so BO2's own
    // shots kill it; the round engine raises it per round.
    let health = rules.int("zombie_health_start", 2).unwrap_or(0) as f32 * zhbo_core::units::BO2_TO_ZH_DAMAGE;
    let mut locos = String::from("; Conquer Ops - the zombies' locomotors, written by zhbo_assets::zombies::zombie_unit.\n");
    let mut objects = String::from("; Conquer Ops - BO2's zombies as Zero Hour units, written by zhbo_assets::zombies::zombie_unit.\n");
    let mut speeds = Vec::new();
    for (object, loco, (clip, bo2_clip)) in unit::CYCLES {
        let speed = root_speed(zone, bo2_clip)? * zhbo_core::units::DRAWN_ZH_PER_INCH;
        speeds.push(format!("{object} {speed:.1}"));
        locos.push_str(&format!(
            "\nLocomotor {loco}\n  Surfaces = GROUND RUBBLE\n  Speed = {speed:.2}\n  SpeedDamaged = {speed:.2}\n  TurnRate = 500\n  TurnRateDamaged = 500\n  Acceleration = 100\n  AccelerationDamaged = 100\n  Braking = 100\n  MinTurnSpeed = 0\n  ZAxisBehavior = NO_Z_MOTIVE_FORCE\n  Appearance = TWO_LEGS\n  StickToGround = Yes\nEnd\n"
        ));
        objects.push_str(&format!(
            "\nObject {object}\n  Draw = W3DModelDraw ModuleTag_01\n    OkToChangeModelColor = No\n    DefaultConditionState\n      Model = {model}\n      IdleAnimation = {h}.{idle}\n      AnimationMode = ONCE\n    End\n    ConditionState = MOVING\n      Animation = {h}.{clip}\n      AnimationMode = LOOP\n      Flags = RANDOMSTART\n    End\n    ConditionState = FIRING_A\n      Animation = {h}.{atk}\n      AnimationMode = ONCE\n    End\n    AliasConditionState = MOVING FIRING_A\n    ConditionState = DYING\n      Animation = {h}.{dth}\n      AnimationMode = ONCE\n    End\n  End\n  DisplayName = OBJECT:ZHBO_Zombie\n  Side = Civilian\n  EditorSorting = INFANTRY\n  VisionRange = 400\n  ShroudClearingRange = 0\n  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY {weapon}\n  End\n  ArmorSet\n    Conditions = None\n    Armor = HumanArmor\n  End\n  KindOf = CAN_ATTACK INFANTRY SCORE\n  Body = ActiveBody ModuleTag_02\n    MaxHealth = {health:.1}\n    InitialHealth = {health:.1}\n  End\n  Behavior = AIUpdateInterface ModuleTag_03\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  Locomotor = SET_NORMAL {loco}\n  Behavior = PhysicsBehavior ModuleTag_04\n    Mass = 5.0\n  End\n  Behavior = SlowDeathBehavior ModuleTag_05\n    DeathTypes = ALL\n    SinkDelay = 3000\n    SinkRate = 0.5\n    DestructionDelay = 8000\n  End\n  Geometry = CYLINDER\n  GeometryMajorRadius = 10.0\n  GeometryMinorRadius = 10.0\n  GeometryHeight = 14.0\n  Scale = {scale}\n  GeometryIsSmall = Yes\n  Shadow = SHADOW_DECAL\n  ShadowSizeX = 14\n  ShadowSizeY = 14\n  ShadowTexture = ShadowI\nEnd\n",
            model = unit::MODEL,
            h = unit::HIERARCHY,
            idle = unit::IDLE.0,
            atk = unit::ATTACK.0,
            dth = unit::DEATH.0,
            weapon = unit::WEAPON,
            scale = zhbo_core::units::GENERAL_SCALE,
        ));
    }
    // The swipe: BO2's meleedamage at the exchange rate on Zero Hour's units and structures (on a
    // General the bridge deals BO2's own, `zhbo_zombie_hit_damage`), one per attack clip.
    let weapon = format!(
        "; Conquer Ops - the zombies' swipe, written by zhbo_assets::zombies::zombie_unit.\n\nWeapon {}\n  PrimaryDamage = {:.1}\n  PrimaryDamageRadius = 0\n  AttackRange = {:.1}\n  DamageType = MELEE\n  DeathType = NORMAL\n  WeaponSpeed = 99999.0\n  ProjectileObject = NONE\n  RadiusDamageAffects = ENEMIES\n  DelayBetweenShots = {:.0}\n  ClipSize = 0\n  AcceptableAimDelta = 180\nEnd\n",
        unit::WEAPON,
        zhbo_core::zombies::melee_damage() * zhbo_core::units::BO2_TO_ZH_DAMAGE,
        unit::MELEE_RANGE * zhbo_core::units::DRAWN_ZH_PER_INCH,
        attack_ms
    );
    out.insert(unit::OBJECT_INI.to_string(), objects.into_bytes());
    out.insert(unit::LOCOMOTOR_INI.to_string(), locos.into_bytes());
    out.insert(unit::WEAPON_INI.to_string(), weapon.into_bytes());
    Ok(format!("zombie units: {} meshes, {} clips, speeds {}", meshes.len(), clips.len(), speeds.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Over the player's install: every roster weapon converts to a definition with BO2's display
    /// name, the upgrades and their alt modes are reached, and the rule table prices the walls as
    /// `add_zombie_weapon` does.
    #[test]
    #[ignore]
    fn tranzits_roster_converts_from_the_install() {
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        let zm = Zombies::load(&bo2).unwrap();
        assert!(zm.missing().is_empty(), "the pool lacks {:?}", zm.missing());
        let mut names = crate::weapons::names(&bo2);
        names.extend(super::names(&bo2));
        let defs = zm.defs(&names);
        assert_eq!(defs.len(), zm.ids.len());
        // Every display name the install has text for is that text (the monkey's and the EMP's keys
        // have none in any language zone, and the knife's def names nothing: those stay as BO2 has
        // them).
        let lost: Vec<&str> = defs
            .iter()
            .filter(|d| zm.weapons.iter().find(|w| w.name == d.id).is_some_and(|w| names.get(&w.display_name).is_some_and(|t| t.trim() != d.display_name)))
            .map(|d| d.id.as_str())
            .collect();
        assert!(lost.is_empty(), "lost display names: {lost:?}");
        eprintln!("converted: {}", zm.ids.join(" "));
        assert!(zm.ids.iter().any(|id| id == "gl_m16_upgraded_zm"), "the upgraded M16's launcher");
        let rules = zhbo_core::zombies::RuleTable::parse(&Zombies::rule_table(&bo2).unwrap());
        let roster = zhbo_core::zombies::roster();
        let price = |id: &str| roster.iter().find(|w| w.id == id).unwrap().cost(&rules);
        eprintln!("walls: python {:?} m16 {:?} galil {:?} ray gun {:?}", price("python_zm"), price("m16_zm"), price("galil_zm"), price("ray_gun_zm"));
        assert_eq!(rules.int("zombie_health_start", 2), Some(150));
        for d in &defs {
            eprintln!("{} \"{}\" clip {} reserve {}/{}", d.id, d.display_name, d.clip, d.start_reserve, d.max_reserve);
        }
    }
}


#[cfg(test)]
mod root_motion_probe {
    /// Over the player's install: the zombie clips' root motion decodes, forward and steady.
    #[test]
    #[ignore]
    fn the_zombie_clips_carry_root_motion() {
        let bo2 = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        let zone = crate::paks::load_zone(&bo2, crate::generals::SURVIVOR_ZONE).unwrap();
        for name in ["ai_zombie_walk_v1", "ai_zombie_run_v2", "ai_zombie_sprint_v1", "ai_zombie_attack_v1", "ai_zombie_idle_v1_delta"] {
            let anim = zhbo_formats::xanim::find(&zone, name).unwrap();
            let keys = zhbo_formats::xanim::root_motion(&zone, name).unwrap();
            let secs = anim.num_frames as f32 / anim.frame_rate;
            let (first, last) = (keys.first().map(|k| k.1).unwrap_or_default(), keys.last().map(|k| k.1).unwrap_or_default());
            let d = [last[0] - first[0], last[1] - first[1], last[2] - first[2]];
            eprintln!("{name}: {} frames at {} fps, delta {}, {} keys, moved {d:?} = {:.1} in/s", anim.num_frames, anim.frame_rate, anim.delta, keys.len(), (d[0] * d[0] + d[1] * d[1]).sqrt() / secs);
        }
    }
}
