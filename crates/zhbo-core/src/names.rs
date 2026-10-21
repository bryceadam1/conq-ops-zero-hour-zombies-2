//! The mod's naming registry (refactor plan 1.1): typed builders for every name the mod invents.
//! Asset names are the ABI between the converter (zhbo-assets, writes them), the engine glue
//! (`host/zhbo/*.cpp`, reads them) and the runtime core (zhbo-core, requests them) — the C++ side
//! of that contract is generated from [`ZHBO_NAMES_H`] into `host/zhbo/ZhboNames.h` (`cargo xtask
//! names`; a test here fails when the checked-in header is stale). See `docs/CONTRACT.md`.

/// Prefix of every General object (`ZHBO_General_USA`, checked by the glue's `isGeneral`).
pub const GENERAL_PREFIX: &str = "ZHBO_General_";
/// Suffix marking a no-shadow copy of a texture (engine patch 0009 strips the shadow pass for it).
pub const NO_SHADOW_SUFFIX: &str = "_nsg";

/// File paths inside the mod's BIG files (the C++ side opens them by the same names; see
/// `ZhboNames.h`).
pub const WEAPON_TABLE_PATH: &str = "Data\\ConquerOps\\weapons.ini";
pub const RTS_FX_PATH: &str = "Data\\ConquerOps\\rts_fx.txt";
pub const VIEWMODEL_SOUNDS_PATH: &str = "Data\\ConquerOps\\viewmodel_sounds.txt";
/// The shipped localised strings (see strings.rs).
pub const STRINGS_PATH: &str = "ConquerOps\\strings_en.ini";
/// The gameplay tuning pack (see tuning.rs).
pub const TUNING_PATH: &str = "ConquerOps\\tuning.ini";
/// The class presets (see classes.rs).
pub const CLASSES_PATH: &str = "ConquerOps\\classes.ini";
/// The converted BO2 HUD/texture archive and its version marker.
pub const BO2_ARCHIVE_NAME: &str = "ConquerOps_BO2.big";
pub const BO2_ARCHIVE_VERSION_PATH: &str = "Data\\ConquerOps\\bo2_hud_version.txt";

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
/// A viewmodel notetrack sound event (`sndnt#reload` plays `ZHBO_NT_reload`).
pub const NOTETRACK_PREFIX: &str = "ZHBO_NT_";
pub fn notetrack_event(alias: &str) -> String {
    format!("ZHBO_NT_{alias}")
}

/// A weapon's third-person meshes on the soldier model (`W0302`: viewmodel 3, mesh 2).
pub fn world_mesh(vm: u8, k: u8) -> String {
    format!("W{vm:02}{k:02}")
}
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
// File paths inside the mod's BIG files (written by zhbo-assets, opened here).
inline constexpr const char *WEAPON_TABLE_PATH = "Data\\ConquerOps\\weapons.ini";
inline constexpr const char *RTS_FX_PATH = "Data\\ConquerOps\\rts_fx.txt";
inline constexpr const char *VIEWMODEL_SOUNDS_PATH = "Data\\ConquerOps\\viewmodel_sounds.txt";
inline constexpr const char *STRINGS_PATH = "ConquerOps\\strings_en.ini";
inline constexpr const char *TUNING_PATH = "ConquerOps\\tuning.ini";
inline constexpr const char *CLASSES_PATH = "ConquerOps\\classes.ini";
} // namespace zhbo_names
"#;

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(general("USA"), GENERAL_USA);
        assert_eq!(faction_viewmodel("GLA"), FACTION_VIEWMODEL_GLA);
        assert_eq!(faction_viewmodel("CHN"), FACTION_VIEWMODEL_CHN);
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