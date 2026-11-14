//! The mod's naming registry (refactor plan 1.1): typed builders for every name the mod invents.
//! Asset names are the ABI between the converter (zhbo-assets, writes them), the engine glue
//! (`host/zhbo/*.cpp`, reads them) and the runtime core (zhbo-core, requests them) — the C++ side
//! of that contract is generated from [`ZHBO_NAMES_H`] into `host/zhbo/ZhboNames.h` (`cargo xtask
//! names`; a test here fails when the checked-in header is stale). See `docs/CONTRACT.md`.

/// Prefix of every General object (`ZHBO_General_USA`, checked by the glue's `isGeneral`).
pub const GENERAL_PREFIX: &str = "ZHBO_General_";
/// Prefix of every survivor General object (also a General: `ZHBO_General_Survivor_Misty`).
pub const SURVIVOR_PREFIX: &str = "ZHBO_General_Survivor_";
/// TranZit's four survivors as Generals: the object, the viewmodel arms letter and the
/// tablet/hammer model code (`ZHBO_VM<code>_TAB`), the same table the bridge's faction list and
/// the converter's survivors carry.
pub const SURVIVORS: [(&str, char, &str); 4] = [
    ("ZHBO_General_Survivor_Misty", 'M', "MIS"),
    ("ZHBO_General_Survivor_Marlton", 'E', "MAR"),
    ("ZHBO_General_Survivor_Russman", 'R', "RUS"),
    ("ZHBO_General_Survivor_Stuhlinger", 'S', "STU"),
];
/// A Zombies map's zombie spawn point (`objflags::SPAWNER`; `zhbo_assets::zombies` writes it).
pub const ZOMBIE_SPAWNER: &str = "ZHBO_ZM_Spawner";
/// Zombies' rule table in the archive (the install's `mp/zombiemode.csv`, `zombies::RuleTable`).
pub const ZOMBIE_RULES_PATH: &str = "Data\\ConquerOps\\zombiemode.csv";
/// Prefix of every zombie object (`ZHBO_Zombie_Walk`; `zhbo_assets::zombies` writes them).
pub const ZOMBIE_PREFIX: &str = "ZHBO_Zombie";
/// The Zombies maps the converter builds from the player's own Zero Hour maps
/// (`zhbo_assets::zombie_map`): each file's path in the archive, then the folder and file name the
/// bridge copies it to under the user-data folder's `Maps` (where the map list finds user maps).
pub const ZOMBIE_MAPS: [(&str, &str, &str); 2] = [
    ("Data\\ConquerOps\\Maps\\Zombies Tournament Desert.map", "Zombies Tournament Desert", "Zombies Tournament Desert.map"),
    ("Data\\ConquerOps\\Maps\\Zombies Tournament Desert.tga", "Zombies Tournament Desert", "Zombies Tournament Desert.tga"),
];
/// Suffix marking a no-shadow copy of a texture (engine patch 0009 strips the shadow pass for it).
pub const NO_SHADOW_SUFFIX: &str = "_nsg";

/// File paths inside the mod's BIG files (the C++ side opens them by the same names; see
/// `ZhboNames.h`).
pub const WEAPON_TABLE_PATH: &str = "Data\\ConquerOps\\weapons.ini";
pub const RTS_FX_PATH: &str = "Data\\ConquerOps\\rts_fx.txt";
/// BO2's impact table, converted (see impacts.rs): `impact`/`flesh` rows of FXLists.
pub const IMPACT_FX_PATH: &str = "Data\\ConquerOps\\impacts.txt";
pub const VIEWMODEL_SOUNDS_PATH: &str = "Data\\ConquerOps\\viewmodel_sounds.txt";
/// The third-person shots' per-layer distance curves (see soundlayers.rs).
pub const SOUND_LAYERS_PATH: &str = "Data\\ConquerOps\\sound_layers.txt";
/// A converted gun's Zero Hour FX weapon (`ZHBO_FX_an94_mp`; `key` is the weapon id with `+`/`~`
/// as `_`, plus `_silenced` for the suppressor's), the weapon the sim fires for its flash.
pub fn fx_weapon(key: &str) -> String {
    format!("{FX_WEAPON_PREFIX}{key}")
}
pub const FX_WEAPON_PREFIX: &str = "ZHBO_FX_";
/// The third-person shot of that gun (`ZHBO_BO2_FireNpc_an94_mp`): what others hear.
pub fn npc_fire_event(key: &str) -> String {
    format!("{NPC_FIRE_PREFIX}{key}")
}
pub const NPC_FIRE_PREFIX: &str = "ZHBO_BO2_FireNpc_";
/// The third-person shot of the gun whose FX weapon is `fx` (the same key).
pub fn npc_fire_for_fx(fx: &str) -> Option<String> {
    fx.strip_prefix(FX_WEAPON_PREFIX).map(npc_fire_event)
}
/// One layer of a converted chain as its own event (`<chain>_L<k>`, k the chain index).
pub fn layer_event(chain: &str, k: usize) -> String {
    format!("{chain}_L{k}")
}

/// The shipped localised strings (see strings.rs).
pub const STRINGS_PATH: &str = "ConquerOps\\strings_en.ini";
/// The gameplay tuning pack (see tuning.rs).
pub const TUNING_PATH: &str = "ConquerOps\\tuning.ini";
/// The class presets (see classes.rs).
pub const CLASSES_PATH: &str = "ConquerOps\\classes.ini";
/// The first-person commands' synonyms and aliases (see command/normal.rs).
pub const COMMANDS_PATH: &str = "ConquerOps\\commands.ini";
/// The player's own command synonyms, beside the profile in Zero Hour's user-data folder.
pub const PLAYER_COMMANDS_FILE: &str = "ConquerOps.commands.ini";
/// The converted BO2 HUD/texture archive and its version marker.
pub const BO2_ARCHIVE_NAME: &str = "ConquerOps_BO2.big";
pub const BO2_ARCHIVE_VERSION_PATH: &str = "Data\\ConquerOps\\bo2_hud_version.txt";
/// The BO2 font metrics in the converted archive (see lui/font.rs).
pub const FONTS_PATH: &str = "Data\\ConquerOps\\bo2_fonts.txt";
/// A BO2 font glyph's MappedImage (`ZHBO_Glyph_bigFont_65`, the converted atlas).
pub fn font_glyph(font: &str, code: u32) -> String {
    format!("ZHBO_Glyph_{font}_{code}")
}

/// A weapon's create-a-class picture (BO2's `<statstable image>_big`, `ZHBO_CaC_Weapon_mp7_mp`; the
/// knife's is its statstable image as is, `combat_tab_knife`).
pub fn cac_weapon_image(base_id: &str) -> String {
    format!("ZHBO_CaC_Weapon_{base_id}")
}
/// A weapon viewmodel's hierarchy (`ZV03_SKL` for viewmodel 3).
pub fn viewmodel_skl(vm: u8) -> String {
    format!("ZV{vm:02}_SKL")
}
/// A weapon viewmodel's model. `letter` picks the arms variant (`S` shared, `C`/`G`/`U` faction).
pub fn viewmodel_skn(vm: u8, letter: char) -> String {
    format!("ZV{vm:02}{letter}_SKN")
}
/// A weapon viewmodel's clip for BO2 animation `slot` (`ZV03_A04`).
pub fn viewmodel_anim(vm: u8, slot: u8) -> String {
    format!("ZV{vm:02}_A{slot:02X}")
}
/// A projectile model (`ZPR02_SKN`).
pub fn projectile_model(vm: u8) -> String {
    format!("ZPR{vm:02}_SKN")
}
/// A dropped weapon's model (`ZDR003_SKN`), numbered in conversion order.
pub fn drop_model(n: u16) -> String {
    format!("ZDR{n:03}_SKN")
}
/// BO2's Scavenger pack: the weapon `dropscavengeritem` drops (`_weapons.gsc:1934`), whose world
/// model the converter turns into [`SCAVENGER_PACK_OBJECT`].
pub const SCAVENGER_ITEM: &str = "scavenger_item_mp";
/// The Zero Hour object a Scavenger pack lies as (`sim::scavenger`).
pub const SCAVENGER_PACK_OBJECT: &str = "ZHBO_ScavengerPack";
/// The Zero Hour object a dropped weapon of base weapon `base` lies as (`ZHBO_Drop_an94_mp`).
pub fn drop_object(base: &str) -> String {
    format!("ZHBO_Drop_{base}")
}
/// The Zero Hour object a Zombies perk machine of specialty `specialty` stands as (its model,
/// `zombies.ini` `[Perks]`), made by the converter (`ZHBO_ZM_Perk_specialty_armorvest`).
pub fn perk_machine_object(specialty: &str) -> String {
    format!("ZHBO_ZM_Perk_{specialty}")
}
/// The W3D model of the converter's `n`-th perk machine prop.
pub fn perk_machine_model(n: u16) -> String {
    format!("ZPK{n:03}_SKN")
}
/// The Zero Hour object Pack-a-Punch stands as: its model (`zombies.ini` `[PackAPunch]`), made by
/// the converter.
pub fn pack_a_punch_object() -> String {
    "ZHBO_ZM_PackAPunch".to_string()
}
/// The W3D model of the converter's Pack-a-Punch prop.
pub fn pack_a_punch_model(n: u16) -> String {
    format!("ZPP{n:03}_SKN")
}
/// The Zero Hour object the Mystery Box stands as: its model (`zombies.ini` `[MagicBox]`), made by
/// the converter.
pub fn magic_box_object() -> String {
    "ZHBO_ZM_MagicBox".to_string()
}
/// The W3D model of the converter's Mystery Box prop.
pub fn magic_box_model(n: u16) -> String {
    format!("ZMB{n:03}_SKN")
}
/// The Zero Hour object a Zombies power-up `name` lies as (`ZHBO_ZM_PowerUp_nuke`): its model
/// (`zombies.ini` `[PowerUps]`), made by the converter.
pub fn powerup_object(name: &str) -> String {
    format!("ZHBO_ZM_PowerUp_{name}")
}
/// The W3D model of the converter's `n`-th Zombies power-up prop.
pub fn powerup_model(n: u16) -> String {
    format!("ZPU{n:03}_SKN")
}
/// The W3D model of the converter's `n`-th Zombies wall buy prop.
pub fn wall_buy_model(n: u16) -> String {
    format!("ZWB{n:03}_SKN")
}
/// The Zero Hour object a Zombies wall buy of weapon `weapon` stands as: its wall model
/// (`zombies.ini` `[WallBuys]`), made by the converter (`ZHBO_ZM_WallBuy_m14_zm`).
pub fn wall_buy_object(weapon: &str) -> String {
    format!("ZHBO_ZM_WallBuy_{weapon}")
}
/// The Zero Hour projectile object for weapon `id`.
pub fn projectile_object(id: &str) -> String {
    format!("ZHBO_Proj_{id}")
}
/// The Zero Hour weapon the overhead General fights with when he holds BO2 weapon `id`
/// (`ZHBO_GeneralWeapon_mp7_mp`): one per player weapon, written by the converter, swapped into
/// his weapon slots by the bridge as his class and his third-slot order change (`sim/rts.rs`).
/// Attachments and alt modes share the base weapon's (`an94_mp+gl` → `an94_mp`).
pub const RTS_WEAPON_PREFIX: &str = "ZHBO_GeneralWeapon_";
/// Longest RTS weapon name the ABI carries (`ZhboRtsWeapons::templates`, NUL included).
pub const RTS_WEAPON_NAME_CAP: usize = 48;
pub fn rts_weapon(id: &str) -> String {
    let base = id.split(['+', '~']).next().unwrap_or(id);
    format!("{RTS_WEAPON_PREFIX}{base}")
}
/// A third-person (torso) clip's notetrack sound event (`sndnt#fly_assault_reload_npc_mag_out`
/// plays `ZHBO_NTN_fly_assault_reload_npc_mag_out`): positional, heard at the General
/// (`crate::torso`), apart from the viewmodel's 2D `ZHBO_NT_*` events.
pub const NPC_NOTETRACK_PREFIX: &str = "ZHBO_NTN_";
pub fn npc_notetrack_event(alias: &str) -> String {
    format!("{NPC_NOTETRACK_PREFIX}{alias}")
}
/// A viewmodel notetrack sound event (`sndnt#reload` plays `ZHBO_NT_reload`).
pub const NOTETRACK_PREFIX: &str = "ZHBO_NT_";
pub fn notetrack_event(alias: &str) -> String {
    format!("ZHBO_NT_{alias}")
}

/// A weapon def's own BO2 sound (`raiseSoundPlayer`, `meleeHitSound`, …) as its converted event:
/// one per alias, shared by every def that names it (`fly_generic_raise_plr` plays
/// `ZHBO_BO2_Snd_fly_generic_raise_plr`).
pub const DEF_SOUND_PREFIX: &str = "ZHBO_BO2_Snd_";
pub fn def_sound_event(alias: &str) -> String {
    format!("{DEF_SOUND_PREFIX}{alias}")
}
/// A def-level sound's third-person event (`ZHBO_BO2_SndNpc_<alias>`): what everyone hears at
/// another player (`WeaponDef::npc_sounds`), positional, apart from the owner's own 2D event even
/// where BO2 names one alias for both (the shield's swing).
pub const NPC_DEF_SOUND_PREFIX: &str = "ZHBO_BO2_SndNpc_";
pub fn npc_def_sound_event(alias: &str) -> String {
    format!("{NPC_DEF_SOUND_PREFIX}{alias}")
}
/// The BO2 alias a def-sound event (own or third-person) plays, if `event` is one.
pub fn def_sound_alias(event: &str) -> Option<&str> {
    event.strip_prefix(DEF_SOUND_PREFIX).or_else(|| event.strip_prefix(NPC_DEF_SOUND_PREFIX))
}

/// A converted BO2 effect's Zero Hour FXList (`ZHBO_BO2FX_weapon_shellejects_fx_rifle`): BO2's
/// effect name with every non-alphanumeric rune replaced by `_`.
pub fn bo2_fx(effect: &str) -> String {
    let clean: String = effect.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("ZHBO_BO2FX_{clean}")
}

/// A weapon's third-person meshes on the soldier model (`W0302`: viewmodel 3, mesh 2).
pub fn world_mesh(vm: u8, k: u8) -> String {
    format!("W{vm:02}{k:02}")
}
/// The Zero Hour model conditions the W3D soldier body plays third-person animation sets 1.. by
/// (`WeaponDef::body_anims`; set 0, the rifleman's, has none): the bridge raises
/// `ANIM_SET_CONDITIONS[set - 1]` while a weapon of that set is in hand and the converter's Draw
/// module has that set's idle, run, aim, fire and reload under it. Zero Hour sets the `RIDER`
/// conditions only on a vehicle whose `RiderChangeContain` holds a rider, never on infantry.
pub const ANIM_SET_CONDITIONS: [&str; 8] = ["RIDER1", "RIDER2", "RIDER3", "RIDER4", "RIDER5", "RIDER6", "RIDER7", "RIDER8"];

/// The scorestreak tablet's third-person meshes (`TB01`).
pub fn tablet_mesh(k: u8) -> String {
    format!("TB{k:02}")
}
/// A faction viewmodel's model (`ZHBO_VMUSA_SKN`).
pub fn faction_viewmodel(faction: &str) -> String {
    format!("ZHBO_VM{faction}_SKN")
}
/// The three factions' viewmodels and General objects, as constants for const tables (the
/// builders above must agree with them; the test below checks that).
pub const FACTION_VIEWMODEL_USA: &str = "ZHBO_VMUSA_SKN";
pub const FACTION_VIEWMODEL_CHN: &str = "ZHBO_VMCHN_SKN";
pub const FACTION_VIEWMODEL_GLA: &str = "ZHBO_VMGLA_SKN";
/// The survivors' viewmodels ([`SURVIVORS`]' codes).
pub const FACTION_VIEWMODEL_MIS: &str = "ZHBO_VMMIS_SKN";
pub const FACTION_VIEWMODEL_MAR: &str = "ZHBO_VMMAR_SKN";
pub const FACTION_VIEWMODEL_RUS: &str = "ZHBO_VMRUS_SKN";
pub const FACTION_VIEWMODEL_STU: &str = "ZHBO_VMSTU_SKN";
pub const GENERAL_USA: &str = "ZHBO_General_USA";
pub const GENERAL_CHN: &str = "ZHBO_General_CHN";
pub const GENERAL_GLA: &str = "ZHBO_General_GLA";
/// The tablet/hammer viewmodel hierarchies and their clips (the converter binds BO2 aliases to
/// these names; the clips' slots live in `zhbo_core::viewmodel`).
pub const TABLET_HIERARCHY: &str = "ZHBO_VMT_SKL";
pub const TABLET_PULLOUT: &str = "ZHBO_VMT_PUL";
pub const TABLET_IDLE: &str = "ZHBO_VMT_IDL";
pub const TABLET_PUTAWAY: &str = "ZHBO_VMT_PUT";
pub const HAMMER_HIERARCHY: &str = "ZHBO_VMH_SKL";
pub const HAMMER_PULLOUT: &str = "ZHBO_VMH_PUL";
pub const HAMMER_IDLE: &str = "ZHBO_VMH_IDL";
pub const HAMMER_SWING: &str = "ZHBO_VMH_SWG";
pub const HAMMER_PUTAWAY: &str = "ZHBO_VMH_PUT";

/// A General object's name (`ZHBO_General_USA`).
pub fn general(faction: &str) -> String {
    format!("{GENERAL_PREFIX}{faction}")
}
/// Whether `name` is a General object (the same test everywhere: the full prefix).
pub fn is_general(name: &str) -> bool {
    name.starts_with(GENERAL_PREFIX)
}
/// Whether `name` is a zombie object.
pub fn is_zombie(name: &str) -> bool {
    name.starts_with(ZOMBIE_PREFIX)
}
/// Whether `name` is a survivor General object.
pub fn is_survivor(name: &str) -> bool {
    name.starts_with(SURVIVOR_PREFIX)
}
/// Zero Hour's heroes, by their stock objects: the General is as tough as they are against Zero
/// Hour's own weapons (the tuning pack's `GeneralHeroToughness`; the converter reads their health
/// and armour from the player's `INIZH.big`, `zhbo_assets::generals::general_health`).
pub const TOUGHNESS_HEROES: [&str; 3] = ["AmericaInfantryColonelBurton", "ChinaInfantryBlackLotus", "GLAInfantryJarmenKell"];
/// The faction a General object belongs to (`ZHBO_General_USA` → `USA`).
pub fn general_faction(name: &str) -> Option<&str> {
    name.strip_prefix(GENERAL_PREFIX).filter(|f| !f.is_empty())
}

/// `gun.tga` → `gun_nsg.tga`: the no-shadow copy of a texture.
pub fn with_no_shadow(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((base, ext)) => format!("{base}{NO_SHADOW_SUFFIX}.{ext}"),
        None => format!("{name}{NO_SHADOW_SUFFIX}"),
    }
}

/// The world-mesh name parsed back: `W0302` → `(vm 3, k 2)`.
pub fn parse_world_mesh(name: &str) -> Option<(u8, u8)> {
    let rest = name.strip_prefix('W')?;
    if rest.len() != 4 || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((rest[..2].parse().ok()?, rest[2..].parse().ok()?))
}

/// `ZV03USKN`-shaped viewmodel names parsed back: `ZV03U_SKN` → `(vm 3, 'U')`.
pub fn parse_viewmodel_skn(name: &str) -> Option<(u8, char)> {
    let rest = name.strip_prefix("ZV")?.strip_suffix("_SKN")?;
    let (vm, letter) = rest.split_at(rest.len().checked_sub(1)?);
    if vm.len() != 2 || !vm.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((vm.parse().ok()?, letter.chars().next()?))
}

/// The C++ side of the registry: `host/zhbo/ZhboNames.h`, written by `cargo xtask names` and
/// checked in. A test here fails when the checked-in copy drifts from this text. Only the names
/// the glue uses are in it; the rest of this module is Rust-only.
pub const ZHBO_NAMES_H: &str = r#"/*
** Conquer Ops - the mod's invented names, shared with the converter and the runtime core
** (crates/zhbo-core/src/names.rs, which generates this file: `cargo xtask names`).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
#pragma once

#include <stdint.h>
#include <stdio.h>
#include <string.h>

namespace zhbo_names
{
// Prefix of every General object; the glue's isGeneral checks exactly this.
inline constexpr const char *GENERAL_PREFIX = "ZHBO_General_";
inline bool isGeneral(const char *name)
{
	return name && strncmp(name, GENERAL_PREFIX, strlen(GENERAL_PREFIX)) == 0;
}
// Prefix of every survivor General object (a General too); the glue's isSurvivor checks it.
inline constexpr const char *SURVIVOR_PREFIX = "ZHBO_General_Survivor_";
inline bool isSurvivor(const char *name)
{
	return name && strncmp(name, SURVIVOR_PREFIX, strlen(SURVIVOR_PREFIX)) == 0;
}
// Prefix of every zombie object (written by the converter); the glue's isZombie checks it.
inline constexpr const char *ZOMBIE_PREFIX = "ZHBO_Zombie";
inline bool isZombie(const char *name)
{
	return name && strncmp(name, ZOMBIE_PREFIX, strlen(ZOMBIE_PREFIX)) == 0;
}
// A Zombies map's zombie spawn point (the round engine's, zhbo_core::sim::zombies).
inline constexpr const char *ZOMBIE_SPAWNER = "ZHBO_ZM_Spawner";
inline bool isZombieSpawner(const char *name)
{
	return name && strcmp(name, ZOMBIE_SPAWNER) == 0;
}
// TranZit's survivors as Generals, by player slot on a Zombies map (names.rs SURVIVORS).
inline constexpr const char *SURVIVOR_OBJECTS[4] = {"ZHBO_General_Survivor_Misty", "ZHBO_General_Survivor_Marlton", "ZHBO_General_Survivor_Russman", "ZHBO_General_Survivor_Stuhlinger"};
// The Zombies maps the converter built (names.rs ZOMBIE_MAPS): the archive path, then the folder
// and file under the user-data folder's Maps.
struct ZombieMapFile
{
	const char *archive;
	const char *folder;
	const char *file;
};
inline constexpr ZombieMapFile ZOMBIE_MAP_FILES[2] = {
	{"Data\\ConquerOps\\Maps\\Zombies Tournament Desert.map", "Zombies Tournament Desert", "Zombies Tournament Desert.map"},
	{"Data\\ConquerOps\\Maps\\Zombies Tournament Desert.tga", "Zombies Tournament Desert", "Zombies Tournament Desert.tga"},
};
// Zombies' rule table in the archive (the install's mp/zombiemode.csv).
inline constexpr const char *ZOMBIE_RULES_PATH = "Data\\ConquerOps\\zombiemode.csv";
// A weapon's third-person meshes on the soldier model (viewmodel vm, mesh k).
inline void worldMesh(char *out, size_t cap, int vm, int k)
{
	snprintf(out, cap, "W%02d%02d", vm, k);
}
// The scorestreak tablet's third-person meshes.
inline void tabletMesh(char *out, size_t cap, int k)
{
	snprintf(out, cap, "TB%02d", k);
}
// The model condition of third-person animation set k (1..8) is ANIM_SET_CONDITIONS[k - 1]; set 0,
// the rifleman's, has none (ZhboRtsWeapons applyWorldPoses raises it while the set is in hand).
inline constexpr const char *ANIM_SET_CONDITIONS[8] = {"RIDER1", "RIDER2", "RIDER3", "RIDER4", "RIDER5", "RIDER6", "RIDER7", "RIDER8"};
// File paths inside the mod's BIG files (written by zhbo-assets, opened here).
inline constexpr const char *WEAPON_TABLE_PATH = "Data\\ConquerOps\\weapons.ini";
inline constexpr const char *RTS_FX_PATH = "Data\\ConquerOps\\rts_fx.txt";
inline constexpr const char *IMPACT_FX_PATH = "Data\\ConquerOps\\impacts.txt";
inline constexpr const char *VIEWMODEL_SOUNDS_PATH = "Data\\ConquerOps\\viewmodel_sounds.txt";
inline constexpr const char *SOUND_LAYERS_PATH = "Data\\ConquerOps\\sound_layers.txt";
inline constexpr const char *STRINGS_PATH = "ConquerOps\\strings_en.ini";
inline constexpr const char *TUNING_PATH = "ConquerOps\\tuning.ini";
inline constexpr const char *CLASSES_PATH = "ConquerOps\\classes.ini";
inline constexpr const char *COMMANDS_PATH = "ConquerOps\\commands.ini";
inline constexpr const char *PLAYER_COMMANDS_FILE = "ConquerOps.commands.ini";
inline constexpr const char *FONTS_PATH = "Data\\ConquerOps\\bo2_fonts.txt";
} // namespace zhbo_names
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headers_survivor_and_map_tables_match_the_registry() {
        for (object, _, _) in SURVIVORS {
            assert!(ZHBO_NAMES_H.contains(&format!("\"{object}\"")), "{object}");
        }
        for (archive, folder, file) in ZOMBIE_MAPS {
            let row = format!("{{\"{}\", \"{folder}\", \"{file}\"}}", archive.replace('\\', "\\\\"));
            assert!(ZHBO_NAMES_H.contains(&row), "{row}");
        }
    }

    #[test]
    fn builders_match_the_converted_layout() {
        assert_eq!(world_mesh(3, 2), "W0302");
        assert_eq!(parse_world_mesh("W0302"), Some((3, 2)));
        assert_eq!(parse_world_mesh("W3"), None);
        assert_eq!(parse_world_mesh("W030a"), None);
        assert_eq!(tablet_mesh(1), "TB01");
        assert_eq!(viewmodel_skl(3), "ZV03_SKL");
        assert_eq!(viewmodel_skn(3, 'U'), "ZV03U_SKN");
        assert_eq!(parse_viewmodel_skn("ZV03U_SKN"), Some((3, 'U')));
        assert_eq!(parse_viewmodel_skn("ZV3U_SKN"), None);
        assert_eq!(viewmodel_anim(3, 0x04), "ZV03_A04");
        assert_eq!(viewmodel_anim(12, 0x3D), "ZV12_A3D");
        assert_eq!(projectile_model(2), "ZPR02_SKN");
        assert_eq!(projectile_object("frag_grenade_mp"), "ZHBO_Proj_frag_grenade_mp");
        assert_eq!(notetrack_event("reload"), "ZHBO_NT_reload");
        assert_eq!(bo2_fx("weapon/shellejects/fx_rifle"), "ZHBO_BO2FX_weapon_shellejects_fx_rifle");
        assert_eq!(general("USA"), GENERAL_USA);
        assert_eq!(faction_viewmodel("GLA"), FACTION_VIEWMODEL_GLA);
        assert_eq!(faction_viewmodel("CHN"), FACTION_VIEWMODEL_CHN);
    }

    #[test]
    fn the_anim_set_conditions_are_the_headers_and_cover_every_set() {
        let quoted: Vec<String> = ANIM_SET_CONDITIONS.iter().map(|c| format!("\"{c}\"")).collect();
        assert!(ZHBO_NAMES_H.contains(&format!("ANIM_SET_CONDITIONS[{}] = {{{}}};", ANIM_SET_CONDITIONS.len(), quoted.join(", "))));
        assert_eq!(ANIM_SET_CONDITIONS.len() + 1, zhbo_native::thirdperson::state::MAX_SETS as usize, "a condition per set but the rifleman's");
    }

    #[test]
    fn the_general_prefix_test_is_the_single_source() {
        assert!(is_general("ZHBO_General_USA"));
        assert!(is_general("ZHBO_General_X"));
        assert!(!is_general("ZHBO_General"), "the shorter prefix (the old 12-char test) must not match");
        assert!(!is_general("AmericaInfantryRanger"));
        assert_eq!(general_faction("ZHBO_General_GLA"), Some("GLA"));
        assert_eq!(general_faction("ZHBO_General"), None);
    }

    #[test]
    fn no_shadow_suffix_marks_texture_copies() {
        assert_eq!(with_no_shadow("gun.tga"), "gun_nsg.tga");
        assert_eq!(with_no_shadow("gun"), "gun_nsg");
    }

    #[test]
    fn the_generated_header_matches_the_checked_in_file() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../host/zhbo/ZhboNames.h");
        let checked_in = std::fs::read_to_string(path).expect("host/zhbo/ZhboNames.h is checked in");
        assert_eq!(checked_in, ZHBO_NAMES_H, "run `cargo xtask names` to regenerate host/zhbo/ZhboNames.h");
    }

    #[test]
    fn the_abis_rts_name_slots_match_the_c_header() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../host/zhbo/zhbo_core.h");
        let header = std::fs::read_to_string(path).expect("host/zhbo/zhbo_core.h is checked in");
        assert!(header.contains(&format!("#define ZHBO_RTS_WEAPON_NAME_CAP {RTS_WEAPON_NAME_CAP}\n")), "zhbo_core.h's ZHBO_RTS_WEAPON_NAME_CAP must be {RTS_WEAPON_NAME_CAP}");
    }
}