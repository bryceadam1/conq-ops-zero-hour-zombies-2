/*
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
