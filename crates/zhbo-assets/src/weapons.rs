//! Black Ops II multiplayer weapons for Conquer Ops: which of BO2's weapons a player can carry, and
//! each one's definition turned into the core's [`WeaponDef`] (Zero Hour units, BO2's display name).
//!
//! The converted table (`Data\ConquerOps\weapons.ini`) holds every player weapon, every attachment
//! and the alt modes they add, so later features (picking up, buying,
//! swapping) can hand out any of them. Viewmodels, sounds and projectile models are converted for
//! the weapons in [`converted`] (the General's loadout with its attachments, then every other weapon; `ZHBO_CONVERT_ALL_WEAPONS=0` limits it to the loadout).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use zhbo_core::attachments::{self, AttachmentDef};
use zhbo_core::units::inches;
use zhbo_core::weapons::{format_table, parse_table, Class, Effect, FireMode, Kick, Loadout, Place, Table, WeaponDef, ALT_CLIPS, MELEE_DAMAGE};
use zhbo_formats::weapon::{self as bo2, Weapon};

use crate::manifest;


/// BO2's create-a-class name of an attachment asset: `silencer_smg` is the `silencer`, `rf_fast` the
/// `rf`.
pub fn short_name(attachment: &str) -> String {
    attachment.split('_').next().unwrap_or(attachment).to_string()
}

/// The core's definition of a BO2 attachment. `names` maps localize keys to text.
pub fn attachment_def(a: &bo2::Attachment, names: &HashMap<String, String>) -> AttachmentDef {
    let fire = a.fire_type.map(|f| match f {
        bo2::fire::FULLAUTO => FireMode::Auto,
        bo2::fire::BURST2 | bo2::fire::BURST3 | bo2::fire::BURST4 | bo2::fire::BURST5 => FireMode::Burst((f - bo2::fire::BURST2 + 2) as u8),
        _ => FireMode::Semi,
    });
    AttachmentDef {
        id: a.name.clone(),
        short: short_name(&a.name),
        display_name: names.get(&a.display_name).map(|s| s.trim().to_string()).unwrap_or_else(|| short_name(&a.name)),
        point: a.point as u8,
        silenced: a.silenced,
        laser: a.laser_sight,
        fire,
        clip: if fire.is_some() { a.clip_size.max(0) as u16 } else { 0 },
        damage_range_scale: a.damage_range_scale,
        ads_fov: if a.ads_zoom_fov > 1.0 { a.ads_zoom_fov } else { 0.0 },
        ads_in_scale: a.ads_in_scale,
        ads_out_scale: a.ads_out_scale,
        ads_move_scale: a.ads_move_scale,
        hip_spread_min_scale: a.hip_spread_min_scale,
        hip_spread_max_scale: a.hip_spread_max_scale,
        fire_time_scale: a.fire_time_scale,
        reload_scale: a.reload_scale,
        reload_empty_scale: a.reload_empty_scale,
        reload_add_scale: a.reload_add_scale,
        clip_scale: a.clip_size_scale,
        recoil_reduction: a.recoil_reduction,
    }
}

/// The whole table: every player weapon, the alt modes their attachments add, and every
/// attachment.
pub fn table(bo2: &[Weapon], atts: &[bo2::Attachment], names: &HashMap<String, String>) -> Table {
    let mut weapons: Vec<WeaponDef> = bo2.iter().filter(|w| is_player_weapon(w)).map(|w| to_def(w, names)).collect();
    let mut extra: Vec<String> = Vec::new();
    for d in &weapons {
        extra.extend(d.alt_modes.iter().map(|(_, alt)| alt.clone()));
    }
    for id in extra {
        if weapons.iter().any(|d| d.id == id) {
            continue;
        }
        if let Some(w) = bo2.iter().find(|w| w.name == id) {
            let mut d = to_def(w, names);
            if w.inventory == bo2::inventory::ALTMODE {
                // A launcher alt mode has its own grenades; select fire and a second optic share
                // the weapon's magazine.
                d.alt_own_ammo = d.class == Class::Launcher;
                // BO2 counts the alt launcher's grenades as rounds, not magazines.
                if d.alt_own_ammo {
                    d.start_reserve = (w.start_ammo - w.clip_size).max(0) as u16;
                    d.max_reserve = ((w.max_ammo - w.clip_size).max(0) as u16).max(d.start_reserve);
                }
            }
            weapons.push(d);
        }
    }
    let attachments = atts.iter().filter(|a| a.kind != 0 && !a.name.starts_with(',')).map(|a| attachment_def(a, names)).collect();
    Table { weapons, attachments, tablet_meshes: 0 }
}

/// Weapons with attachments, and their alt modes, that [`converted`] asks for: built from their base
/// definitions (the core's own rules) and added to the table, so their art can be set on them. An
/// alt mode for a converted weapon is `<alt>~<weapon>` (it shares the weapon's viewmodel).
pub fn add_variants(table: &mut Table, converted: &[String]) {
    for id in converted.iter().filter(|id| id.contains('+')) {
        if table.weapons.iter().any(|d| &d.id == id) {
            continue;
        }
        let (base_id, shorts) = attachments::split_id(id);
        let Some(base) = table.weapons.iter().find(|d| d.id == base_id).cloned() else { continue };
        let atts = attachments::resolve(&base, &shorts, |a| table.attachments.iter().find(|x| x.id == a));
        let mut d = attachments::apply(&base, &atts);
        if let Some((short, alt_id)) = base.alt_modes.iter().find(|(short, _)| d.attachments.contains(short)) {
            if let Some(alt_base) = table.weapons.iter().find(|w| &w.id == alt_id).cloned() {
                // Select fire and the second optic keep the weapon's other attachments.
                let others: Vec<&AttachmentDef> = atts.iter().copied().filter(|a| &a.short != short).collect();
                let mut alt = if alt_base.alt_own_ammo { alt_base.clone() } else { attachments::apply(&alt_base, &others) };
                alt.id = format!("{alt_id}~{}", d.id);
                alt.alt = d.id.clone();
                alt.alt_own_ammo = alt_base.alt_own_ammo;
                alt.display_name = alt_base.display_name.clone();
                d.alt = alt.id.clone();
                table.weapons.push(alt);
            }
        }
        table.weapons.push(d);
    }
}

/// The alt mode entry of a converted weapon, if it has one.
pub fn alt_of<'a>(table: &'a Table, id: &str) -> Option<&'a WeaponDef> {
    let d = table.weapons.iter().find(|d| d.id == id)?;
    table.weapons.iter().find(|a| !d.alt.is_empty() && a.id == d.alt)
}

/// Mark alt-mode clips on a viewmodel: an alt mode's clips sit after its weapon's.
pub fn alt_clip(slot: usize) -> usize {
    slot + ALT_CLIPS as usize
}

/// BO2's base aim-down-sights walking speed, as a share of the normal walk.
const ADS_MOVE_BASE: f32 = 0.35;

/// Where the weapon table goes in the archive (the bridge hands it to the core).
pub use zhbo_core::names::WEAPON_TABLE_PATH as TABLE_PATH;

/// BO2's localized strings for the display names: `zone/<language>/<lang>_code_post_gfx_mp.ff`
/// with `<lang>_patch_mp.ff` over it. Empty if the language zones can't be read.
pub fn names(bo2_dir: &std::path::Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (dir, prefix) in [("english", "en")] {
        for zone in ["code_post_gfx_mp", "patch_mp"] {
            let path = bo2_dir.join("zone").join(dir).join(format!("{prefix}_{zone}.ff"));
            let Ok(file) = std::fs::read(&path) else { continue };
            let Ok(ff) = zhbo_formats::fastfile::read(&file) else { continue };
            let Ok(z) = zhbo_formats::t6zone::load(&ff) else { continue };
            out.extend(zhbo_formats::localize::entries(&z).unwrap_or_default());
        }
    }
    out
}

/// Sound events and Zero Hour FX weapons for the converted weapons. Each gun gets its own player
/// gunshot (BO2's `_plr` alias with its tail and low end) and third-person gunshot, played by its
/// own FX weapon (`ZHBO_FX_<id>`); the loadout's offhands and knife supply the throw, pin, blast and
/// swing sounds. Sets each definition's `fire_sound` and `fx_weapon`; returns the events and the
/// FX weapon INI.
pub fn sounds_and_fx(bo2: &[Weapon], atts: &[bo2::Attachment], defs: &mut [WeaponDef], converted: &[String]) -> (Vec<crate::sfx::DynSfx>, String) {
    use crate::sfx::{DynSfx, Kind};
    let mut events = Vec::new();
    let mut ini = String::from("; Generated by zhbo-assets: each BO2 gun's third-person shot (zhbo-core plays these).\n\n");
    let layers = |base: &str| -> Vec<String> {
        let stem = base.trim_end_matches("_fire_plr").trim_end_matches("_fire_npc").to_string();
        vec![base.to_string(), format!("{base}_decay"), format!("{stem}_lfe"), format!("{stem}_LFE")]
    };
    let l = Loadout::default();
    // The converted weapons and their alt modes.
    let mut ids: Vec<String> = converted.to_vec();
    for id in converted {
        if let Some(alt) = defs.iter().find(|d| &d.id == id).map(|d| d.alt.clone()).filter(|a| !a.is_empty()) {
            ids.push(alt);
        }
    }
    for id in &ids {
        let base = crate::generals::base_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(d) = defs.iter_mut().find(|d| &d.id == id) else { continue };
        // Attachments can change the shot (a suppressor).
        let (_, shorts) = attachments::split_id(id.split('~').next().unwrap_or(id));
        let c = crate::generals::combo(w, atts, &shorts);
        // Zero Hour names: no `+` or `~`.
        let key = id.replace(['+', '~'], "_");
        if d.class.is_gun() {
            if let Some(plr) = &c.fire_sound_player {
                let event = format!("ZHBO_BO2_Fire_{key}");
                events.push(DynSfx { event: event.clone(), layers: layers(plr), kind: Kind::Player, limit: 3 });
                d.fire_sound = event;
            }
            let npc_event = format!("ZHBO_BO2_FireNpc_{key}");
            if let Some(npc) = &c.fire_sound {
                events.push(DynSfx { event: npc_event.clone(), layers: vec![npc.clone(), format!("{npc}_decay")], kind: Kind::Others, limit: 3 });
            }
            let fx = format!("ZHBO_FX_{key}");
            let launcher = d.class == Class::Launcher;
            ini.push_str(&format!(
                "Weapon {fx}\n  PrimaryDamage           = 0.0\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = 1000.0\n  \
                 DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = {}\n  FireSound               = {npc_event}\n  \
                 RadiusDamageAffects     = ENEMIES\n  DelayBetweenShots       = 0\n  ClipSize                = 0\nEnd\n\n",
                if launcher { "WeaponFX_RocketBuggyMissileFire" } else if d.silenced { "NONE" } else { "WeaponFX_GenericMachineGunFire" }
            ));
            d.fx_weapon = fx;
        }
        let general = |event: &str, alias: &Option<String>, kind: Kind| alias.as_ref().map(|a| DynSfx { event: event.into(), layers: vec![a.clone()], kind, limit: 2 });
        if *id == l.lethal {
            events.extend(general(zhbo_core::sounds::THROW, &w.fire_sound_player, Kind::Player));
            events.extend(general(zhbo_core::sounds::PULL_PIN, &w.pullback_sound_player, Kind::Player));
            events.extend(general(zhbo_core::sounds::EXPLOSION, &w.projectile_explosion_sound, Kind::World));
        }
        if *id == l.melee {
            events.extend(general(zhbo_core::sounds::MELEE_SWING, &w.melee_swipe_sound_player, Kind::Player));
        }
    }
    (events, ini)
}

/// A weapon a player can carry in BO2 multiplayer (not killstreak, turret or alt-mode entries).
pub fn is_player_weapon(w: &Weapon) -> bool {
    use manifest::{EXCLUDED_WEAPONS, LETHALS, TACTICALS};
    if w.name == "knife_held_mp" {
        return true;
    }
    if w.name.starts_with("remote_") || w.name.contains("killstreak") || w.name == "knife_mp" {
        return false;
    }
    if LETHALS.contains(&w.name.as_str()) || TACTICALS.contains(&w.name.as_str()) {
        return true;
    }
    w.is_gun() && !w.name.contains("wager") && !w.name.starts_with("inventory_") && w.view_model.is_some() && !EXCLUDED_WEAPONS.contains(&w.name.as_str())
}

fn class_of(w: &Weapon) -> Class {
    use manifest::{LETHALS, TACTICALS};
    let name = w.name.as_str();
    if LETHALS.contains(&name) {
        return if matches!(name, "bouncingbetty_mp" | "claymore_mp" | "satchel_charge_mp") { Class::Equipment } else { Class::Lethal };
    }
    if TACTICALS.contains(&name) {
        return if matches!(name, "trophy_system_mp" | "tactical_insertion_mp" | "pda_hack_mp") { Class::Equipment } else { Class::Tactical };
    }
    if w.weap_type == bo2::kind::MELEE {
        return Class::Melee;
    }
    if w.view_model.as_deref().is_some_and(|m| m.contains("_sniper_")) {
        return Class::Sniper;
    }
    match w.class {
        bo2::class::MG => Class::Lmg,
        bo2::class::SMG => Class::Smg,
        bo2::class::SPREAD => Class::Shotgun,
        bo2::class::PISTOL | bo2::class::PISTOL_SPREAD => Class::Pistol,
        bo2::class::ROCKETLAUNCHER | bo2::class::GRENADE => Class::Launcher,
        bo2::class::RIFLE if w.weap_type == bo2::kind::PROJECTILE => Class::Special,
        bo2::class::RIFLE => Class::Rifle,
        _ => Class::Special,
    }
}

fn effect_of(w: &Weapon) -> Effect {
    if let Some(e) = manifest::named_effect(&w.name) {
        return e;
    }
    match w.weap_type {
        bo2::kind::PROJECTILE | bo2::kind::GRENADE | bo2::kind::MINE => Effect::Explode,
        _ => Effect::None,
    }
}

fn fire_mode(w: &Weapon) -> FireMode {
    match w.fire_type {
        bo2::fire::FULLAUTO | bo2::fire::STACKED => FireMode::Auto,
        bo2::fire::MINIGUN => FireMode::Minigun,
        bo2::fire::BURST2 | bo2::fire::BURST3 | bo2::fire::BURST4 | bo2::fire::BURST5 => FireMode::Burst(w.burst_count() as u8),
        _ => FireMode::Semi,
    }
}

fn kick(k: bo2::Kick) -> Kick {
    Kick { pitch: k.pitch, yaw: k.yaw }
}

/// The core definition for a BO2 weapon. `names` maps localize keys to text.
///
/// In debug builds the result is round-tripped through the table schema (format then parse), so
/// a schema line that `to_def` does not write — or one whose value does not survive the text
/// form — fails here instead of silently drifting (refactor plan 1.4 / 3.2).
pub fn to_def(w: &Weapon, names: &HashMap<String, String>) -> WeaponDef {
    let class = class_of(w);
    let clip = w.clip_size.max(0) as u16;
    let (start_reserve, max_reserve) = if class.is_offhand() {
        // Offhands: how many you spawn with, and carry at most.
        (w.start_ammo.max(1) as u16, w.max_ammo.max(w.start_ammo).max(1) as u16)
    } else if w.ammo_clip_relative || (w.start_ammo > 0 && w.start_ammo < w.clip_size) {
        // BO2 multiplayer counts gun ammo in magazines (the loaded one included): the AN-94's 4/8
        // is 30/90 at spawn and 210 spare at most.
        (((w.start_ammo - 1).max(0) * w.clip_size) as u16, ((w.max_ammo - 1).max(0) * w.clip_size) as u16)
    } else {
        ((w.start_ammo - w.clip_size).max(0) as u16, (w.max_ammo - w.clip_size).max(0) as u16)
    };
    let range = |r: f32| if r >= 999_999.0 { inches(100_000.0) } else { inches(r) };
    let key = manifest::display_name_key(w.display_name.trim_start_matches("WEAPON_"));
    let display_name = names
        .get(key)
        .or_else(|| names.get(&format!("WEAPON_{key}")))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| w.name.trim_end_matches("_mp").to_uppercase());
    let effect = effect_of(w);
    let sticky = manifest::STICKY.contains(&w.name.as_str());
    let mut damage = [0.0; 6];
    let mut damage_range = [0.0; 6];
    for i in 0..6 {
        damage[i] = w.damage[i] as f32;
        damage_range[i] = if w.damage_range[i] > 0.0 { range(w.damage_range[i]) } else { 0.0 };
    }
    let mult = |i: usize| w.location_multipliers.get(i).copied().filter(|m| *m > 0.0).unwrap_or(1.0);
    let shells = w.reload_start_time > 0.0 && w.reload_ammo_add > 0;
    let d = WeaponDef {
        id: w.name.clone(),
        equipment_kind: manifest::equipment_kind_of(&w.name),
        display_name,
        class,
        fire: fire_mode(w),
        fire_time: w.fire_time.max(0.01),
        burst_delay: w.burst_delay,
        spin_up_time: w.spin_up_time,
        pellets: w.shot_count.max(1) as u16,
        clip,
        start_reserve,
        max_reserve: max_reserve.max(start_reserve),
        reload_time: w.reload_time,
        reload_empty_time: if w.reload_empty_time > 0.0 { w.reload_empty_time } else { w.reload_time },
        reload_add_time: w.reload_add_time,
        reload_empty_add_time: if w.reload_empty_add_time > 0.0 { w.reload_empty_add_time } else { w.reload_add_time },
        reload_start_time: if shells { w.reload_start_time } else { 0.0 },
        reload_end_time: if shells { w.reload_end_time } else { 0.0 },
        reload_ammo_add: if shells { w.reload_ammo_add.max(1) as u16 } else { 0 },
        raise_time: w.raise_time,
        first_raise_time: if w.first_raise_time > 0.0 { w.first_raise_time } else { w.raise_time },
        drop_time: w.drop_time,
        quick_raise_time: if w.quick_raise_time > 0.0 { w.quick_raise_time } else { w.raise_time * 0.6 },
        quick_drop_time: if w.quick_drop_time > 0.0 { w.quick_drop_time } else { w.drop_time * 0.6 },
        sprint_out_time: w.sprint_out_time,
        ads_in_time: w.ads_in_time.max(0.05),
        ads_out_time: w.ads_out_time.max(0.05),
        ads_fov: w.ads_zoom_fov,
        damage,
        damage_range,
        head_multiplier: mult(bo2::HIT_LOCATION_HEAD),
        max_range: if w.max_range > 0.0 { inches(w.max_range) } else { inches(8000.0) },
        hip_spread: [w.hip_spread_stand, w.hip_spread_ducked, w.hip_spread_prone],
        spread_decay: w.hip_spread_decay,
        spread_fire_add: w.hip_spread_fire_add,
        spread_move_add: w.hip_spread_move_add,
        ads_spread: w.ads_spread,
        ads_kick: kick(w.ads_view_kick),
        hip_kick: kick(w.hip_view_kick),
        move_scale: if w.move_speed_scale > 0.0 { w.move_speed_scale } else { 1.0 },
        // BO2's adsMoveSpeedScale multiplies the base ADS walk speed (rifles 1, LMGs 0.8, SMGs,
        // pistols and shotguns 2); the base itself is engine tuning, about a third of a walk.
        ads_move_scale: (ADS_MOVE_BASE * if w.ads_move_speed_scale > 0.0 { w.ads_move_speed_scale } else { 1.0 }).min(1.0),
        sprint_duration_scale: if w.sprint_duration_scale > 0.0 { w.sprint_duration_scale } else { 1.0 },
        melee_damage: if w.melee_damage > 0 { w.melee_damage as f32 } else { MELEE_DAMAGE },
        melee_time: if w.melee_time > 0.0 { w.melee_time } else { 0.6 },
        effect,
        fuse_time: w.fuse_time,
        cook: w.cook_off_hold,
        explosion_radius: inches(w.explosion_radius as f32),
        explosion_radius_min: inches(w.explosion_radius_min as f32),
        explosion_inner_damage: w.explosion_inner_damage as f32,
        explosion_outer_damage: w.explosion_outer_damage as f32,
        projectile_speed: inches(w.projectile_speed as f32),
        projectile_speed_up: inches(w.projectile_speed_up as f32),
        projectile_gravity: w.class != bo2::class::ROCKETLAUNCHER,
        impact_explode: w.projectile_impact_explode,
        sticky,
        fx_weapon: String::new(),
        explosion_fx: match effect {
            Effect::Explode => "FX_BoobyTrapExplosion",
            Effect::Stun => "WeaponFX_RangerFlashBangGrenadeDetonation",
            Effect::Emp => "WeaponFX_EMPPulseImpact",
            Effect::Smoke => "FX_OverlordSmoke",
            Effect::Impact | Effect::None => "",
        }
        .to_string(),
        projectile_object: String::new(),
        fire_sound: String::new(),
        explode_sound: String::new(),
        viewmodel: -1,
        anims: w.anims.iter().enumerate().take(128).filter(|(_, a)| a.is_some()).fold(0u128, |m, (i, _)| m | (1u128 << i)),
        clip_base: 0,
        world_meshes: 0,
        attachments: Vec::new(),
        allowed: w.attachment_variants.iter().map(|(_, id)| (short_name(id), id.clone())).collect(),
        alt_modes: w
            .attachment_uniques
            .iter()
            .filter(|u| u.combined == 0)
            .filter_map(|u| {
                let alt = u.alt_weapon.clone()?;
                let (_, att) = w.attachment_variants.iter().find(|(k, _)| *k == u.kind)?;
                Some((short_name(att), alt))
            })
            .collect(),
        alt: if w.inventory == bo2::inventory::ALTMODE { w.alt_weapon.clone().unwrap_or_default() } else { String::new() },
        alt_own_ammo: false,
        silenced: w.silenced,
        laser: false,
        fmj: false,
        target_finder: false,
        recoil_reduction: (0.0, 0.0),
        place: manifest::place_of(&w.name),
    };
    #[cfg(debug_assertions)]
    {
        let text = format_table(std::slice::from_ref(&d));
        match parse_table(&text) {
            Ok(back) => debug_assert_eq!(back.as_slice(), std::slice::from_ref(&d), "to_def's output must round-trip through the weapon table schema"),
            Err(e) => panic!("to_def's output does not parse back ({}): {}", w.name, e),
        }
    }
    d
}

/// Zero Hour units always hit; per-shot RTS damage keeps BO2's ratios between weapons, scaled so a
/// 40-damage rifle round (the AN-94's) hits for the 15 the General's rifle always had.
const RTS_DAMAGE: f32 = 15.0 / 40.0;

/// How far the overhead General engages with a gun of `class` (Zero Hour units, edge to edge).
fn rts_range(class: Class) -> f32 {
    match class {
        Class::Sniper => 300.0,
        Class::Shotgun => 100.0,
        Class::Pistol => 125.0,
        _ => 175.0,
    }
}

/// One RTS shot's damage: BO2's first damage point (every pellet of a shotgun), or the blast of a
/// launcher's rocket, in [`RTS_DAMAGE`]'s scale.
fn rts_damage(d: &WeaponDef) -> f32 {
    if d.class == Class::Launcher && d.explosion_inner_damage > 0.0 {
        return d.explosion_inner_damage * RTS_DAMAGE;
    }
    d.damage[0] * RTS_DAMAGE * d.pellets.max(1) as f32
}

/// The General's Zero Hour weapons under RTS control (`ZHBO_GeneralRifle`, `_Pistol`, `_Grenade`)
/// with the loadout's BO2 numbers: (weapon, key, value) to set in `ZHBO_Weapons.ini`. These are
/// the stand-ins a General holds before his class lands; [`rts_weapons_ini`] writes the ones a
/// class swaps in.
pub fn rts_weapon_edits(defs: &[WeaponDef]) -> Vec<(String, String, String)> {
    let l = Loadout::default();
    let find = |id: &str| defs.iter().find(|d| d.id == id);
    let mut out = Vec::new();
    let mut set = |w: &str, k: &str, v: String| out.push((w.to_string(), k.to_string(), v));
    for (weapon, id) in [("ZHBO_GeneralRifle", &l.primary), ("ZHBO_GeneralPistol", &l.secondary)] {
        let Some(d) = find(id) else { continue };
        set(weapon, "PrimaryDamage", format!("{:.1}", rts_damage(d)));
        set(weapon, "AttackRange", format!("{:.1}", rts_range(d.class)));
        set(weapon, "DelayBetweenShots", format!("{:.0}", d.fire_time * 1000.0));
        set(weapon, "ClipSize", d.clip.max(1).to_string());
        set(weapon, "ClipReloadTime", format!("{:.0}", d.reload_time * 1000.0));
        let key = id.replace(['+', '~'], "_");
        set(weapon, "FireSound", format!("ZHBO_BO2_FireNpc_{key}"));
    }
    // The third slot only stands in for the throw (zhbo-core throws the BO2 grenade).
    set("ZHBO_GeneralGrenade", "FireSound", zhbo_core::sounds::THROW.to_string());
    out
}

/// How far a thrown offhand flies at the speed zhbo-core throws it with (`Sim::rts_update`): the
/// farthest throw, at 45 degrees.
pub fn throw_range(d: &WeaponDef) -> f32 {
    let speed = if d.projectile_speed > 0.0 { d.projectile_speed } else { inches(800.0) };
    speed * speed / zhbo_core::projectiles::GRAVITY
}

/// The Zero Hour weapon of every player weapon the overhead General can hold
/// (`zhbo_core::names::rts_weapon`), which the bridge swaps into his slots as his class and his
/// orders change (`zhbo_core::sim::rts`):
///
/// * a **gun** fires with its BO2 fire rate, magazine and reload, hits for [`rts_damage`] out to
///   [`rts_range`], and sounds and flashes as BO2's does to others (`flashes` by weapon id;
///   `has_event` says which sound events were converted);
/// * an **offhand** is an order fired once: a throw reaches as far as the grenade flies
///   ([`throw_range`]) but never into its own blast, and a planted item walks him up to the spot
///   (`PLANT_DISTANCE`). zhbo-core throws or plants the BO2 item itself; Zero Hour only aims, so
///   these carry a token of damage (Zero Hour won't attack with a weapon that does none);
/// * the **knife** reaches `MELEE_RANGE`, edge to edge as Zero Hour measures it.
///
/// Attachments and alt modes share their base weapon's, so only base weapons get one, once each.
pub fn rts_weapons_ini(defs: &[WeaponDef], flashes: &BTreeMap<String, String>, has_event: &dyn Fn(&str) -> bool) -> String {
    let mut ini = String::from("; The overhead General's weapons, one per BO2 player weapon (zhbo-assets weapons::rts_weapons_ini).\n\n");
    let mut seen = BTreeSet::new();
    for d in defs.iter().filter(|d| !d.id.contains(['+', '~'])) {
        let name = zhbo_core::names::rts_weapon(&d.id);
        if !seen.insert(name.clone()) {
            continue;
        }
        let body = if d.class.is_gun() {
            let launcher = d.class == Class::Launcher;
            let npc = format!("ZHBO_BO2_FireNpc_{}", d.id.replace(['+', '~'], "_"));
            let flash = flashes.get(&d.id).cloned().unwrap_or_else(|| {
                if launcher { "WeaponFX_RocketBuggyMissileFire" } else if d.silenced { "NONE" } else { "WeaponFX_GenericMachineGunFire" }.to_string()
            });
            format!(
                "  PrimaryDamage           = {:.1}\n  PrimaryDamageRadius     = {:.1}\n  AttackRange             = {:.1}\n  \
                 DamageType              = {}\n  DeathType               = {}\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  FireFX                  = {flash}\n  FireSound               = {}\n  \
                 RadiusDamageAffects     = ENEMIES NEUTRALS\n  DelayBetweenShots       = {:.0}\n  ClipSize                = {}\n  \
                 ClipReloadTime          = {:.0}\n",
                rts_damage(d),
                if launcher { d.explosion_radius } else { 0.0 },
                rts_range(d.class),
                if launcher { "EXPLOSION" } else { "SMALL_ARMS" },
                if launcher { "EXPLODED" } else { "NORMAL" },
                if has_event(&npc) { npc.as_str() } else { "NONE" },
                d.fire_time * 1000.0,
                d.clip.max(1),
                d.reload_time * 1000.0,
            )
        } else if d.class.is_offhand() {
            let throw = d.place == Place::Throw;
            let (range, min) = if throw {
                (throw_range(d), if d.effect == Effect::Explode { d.explosion_radius } else { 0.0 })
            } else {
                (zhbo_core::sim::PLANT_DISTANCE, 0.0)
            };
            let sound = if throw && has_event(zhbo_core::sounds::THROW) { zhbo_core::sounds::THROW } else { "NONE" };
            format!(
                "  PrimaryDamage           = 0.01\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = {range:.1}\n  \
                 MinimumAttackRange      = {min:.1}\n  DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  \
                 WeaponSpeed             = 999999.0\n  ProjectileObject        = NONE\n  RadiusDamageAffects     = ENEMIES NEUTRALS\n  \
                 FireSound               = {sound}\n  PreAttackDelay          = 450\n  PreAttackType           = PER_SHOT\n  \
                 DelayBetweenShots       = 0\n  ClipSize                = 1\n  ClipReloadTime          = 1500\n  AutoReloadsClip         = Yes\n"
            )
        } else {
            let sound = if has_event(zhbo_core::sounds::MELEE_SWING) { zhbo_core::sounds::MELEE_SWING } else { "NONE" };
            format!(
                "  PrimaryDamage           = 0.01\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = {:.1}\n  \
                 DamageType              = MELEE\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
                 ProjectileObject        = NONE\n  RadiusDamageAffects     = ENEMIES NEUTRALS\n  FireSound               = {sound}\n  \
                 DelayBetweenShots       = {:.0}\n  ClipSize                = 1\n  ClipReloadTime          = 0\n",
                zhbo_core::weapons::MELEE_RANGE,
                d.melee_time.max(0.0) * 1000.0,
            )
        };
        ini.push_str(&format!("Weapon {name}\n{body}End\n\n"));
    }
    ini
}

/// The ids whose art is converted: the loadout's (with its attachments) first, then every other player weapon unless `ZHBO_CONVERT_ALL_WEAPONS=0`.
pub fn converted(all: &[Weapon]) -> Vec<String> {
    let l = Loadout::default();
    let mut ids = vec![l.primary, l.secondary, l.lethal, l.tactical, l.melee];
    // Every weapon's art, so any of them can be picked up, bought or swapped to; `=0` converts only
    // the loadout's (quicker while developing).
    if !std::env::var("ZHBO_CONVERT_ALL_WEAPONS").is_ok_and(|v| v == "0") {
        for w in all.iter().filter(|w| is_player_weapon(w)) {
            if !ids.contains(&w.name) {
                ids.push(w.name.clone());
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_bo2_types_to_classes() {
        let rifle = Weapon { name: "an94_mp".into(), class: bo2::class::RIFLE, view_model: Some("t6_wpn_ar_an94_view".into()), ..Default::default() };
        assert_eq!(class_of(&rifle), Class::Rifle);
        let sniper = Weapon { name: "dsr50_mp".into(), class: bo2::class::RIFLE, view_model: Some("t6_wpn_sniper_dsr50_view".into()), ..Default::default() };
        assert_eq!(class_of(&sniper), Class::Sniper);
        let frag = Weapon { name: "frag_grenade_mp".into(), weap_type: bo2::kind::GRENADE, ..Default::default() };
        assert_eq!((class_of(&frag), effect_of(&frag)), (Class::Lethal, Effect::Explode));
        let conc = Weapon { name: "concussion_grenade_mp".into(), weap_type: bo2::kind::GRENADE, ..Default::default() };
        assert_eq!((class_of(&conc), effect_of(&conc)), (Class::Tactical, Effect::Stun));
    }

    #[test]
    fn clip_relative_ammo_counts_magazines() {
        let w = Weapon { name: "an94_mp".into(), clip_size: 30, start_ammo: 4, max_ammo: 8, ammo_clip_relative: false, fire_time: 0.096, ..Default::default() };
        let d = to_def(&w, &HashMap::new());
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (30, 90, 210));
        assert_eq!(d.display_name, "AN94");
    }

    /// The `Weapon` block named `name` in `ini`, and one key's value in it.
    fn block<'a>(ini: &'a str, name: &str) -> &'a str {
        let at = ini.find(&format!("Weapon {name}\n")).unwrap_or_else(|| panic!("no {name} in\n{ini}"));
        &ini[at..at + ini[at..].find("\nEnd").unwrap()]
    }
    fn value<'a>(block: &'a str, key: &str) -> &'a str {
        let line = block.lines().find(|l| l.trim_start().starts_with(key)).unwrap_or_else(|| panic!("no {key} in\n{block}"));
        line.split('=').nth(1).unwrap().trim()
    }

    #[test]
    fn every_base_weapon_gets_the_overhead_generals_weapon() {
        let rifle = WeaponDef { id: "an94_mp".into(), class: Class::Rifle, clip: 30, fire_time: 0.1, reload_time: 2.5, damage: [40.0; 6], ..Default::default() };
        let variant = WeaponDef { id: "an94_mp+gl".into(), clip: 99, ..rifle.clone() };
        let frag = WeaponDef {
            id: "frag_grenade_mp".into(),
            class: Class::Lethal,
            effect: Effect::Explode,
            place: Place::Throw,
            explosion_radius: 40.0,
            projectile_speed: inches(900.0),
            ..Default::default()
        };
        let claymore = WeaponDef { id: "claymore_mp".into(), class: Class::Equipment, place: Place::Plant, ..Default::default() };
        let knife = WeaponDef { id: "knife_held_mp".into(), class: Class::Melee, melee_time: 0.5, ..Default::default() };
        let defs = [rifle, variant, frag.clone(), claymore, knife];
        let flashes = BTreeMap::from([("an94_mp".to_string(), "ZHBO_BO2FX_Flash".to_string())]);
        let ini = rts_weapons_ini(&defs, &flashes, &|e| e == zhbo_core::sounds::THROW);
        assert_eq!(ini.matches("\nWeapon ").count() + usize::from(ini.starts_with("Weapon ")), 4, "attachments share their base weapon's:\n{ini}");

        let gun = block(&ini, "ZHBO_GeneralWeapon_an94_mp");
        assert_eq!((value(gun, "ClipSize"), value(gun, "DelayBetweenShots"), value(gun, "ClipReloadTime")), ("30", "100", "2500"), "the base weapon's numbers");
        assert_eq!(value(gun, "FireFX"), "ZHBO_BO2FX_Flash");
        assert_eq!(value(gun, "FireSound"), "NONE", "no converted NPC sound, no event");
        assert_eq!(value(gun, "PrimaryDamage"), format!("{:.1}", 40.0 * RTS_DAMAGE));

        let throw = block(&ini, "ZHBO_GeneralWeapon_frag_grenade_mp");
        assert_eq!(value(throw, "AttackRange"), format!("{:.1}", throw_range(&frag)));
        assert_eq!(value(throw, "MinimumAttackRange"), "40.0", "never into its own blast");
        assert_eq!(value(throw, "FireSound"), zhbo_core::sounds::THROW);
        let plant = block(&ini, "ZHBO_GeneralWeapon_claymore_mp");
        assert_eq!(value(plant, "AttackRange"), format!("{:.1}", zhbo_core::sim::PLANT_DISTANCE));
        let melee = block(&ini, "ZHBO_GeneralWeapon_knife_held_mp");
        assert_eq!((value(melee, "AttackRange"), value(melee, "DamageType")), (format!("{:.1}", zhbo_core::weapons::MELEE_RANGE).as_str(), "MELEE"));
    }

    #[test]
    fn a_throws_reach_is_its_flight_at_45_degrees() {
        let frag = WeaponDef { projectile_speed: inches(800.0), ..Default::default() };
        // v² / g with BO2's gravity (800 in/s²): 800 inches.
        assert!((throw_range(&frag) - inches(800.0)).abs() < 1e-3);
    }
}
