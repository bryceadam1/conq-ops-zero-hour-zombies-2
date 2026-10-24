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
inline constexpr const char *IMPACT_FX_PATH = "Data\\ConquerOps\\impacts.txt";
inline constexpr const char *VIEWMODEL_SOUNDS_PATH = "Data\\ConquerOps\\viewmodel_sounds.txt";
inline constexpr const char *STRINGS_PATH = "ConquerOps\\strings_en.ini";
inline constexpr const char *TUNING_PATH = "ConquerOps\\tuning.ini";
inline constexpr const char *CLASSES_PATH = "ConquerOps\\classes.ini";
inline constexpr const char *FONTS_PATH = "Data\\ConquerOps\\bo2_fonts.txt";
} // namespace zhbo_names
