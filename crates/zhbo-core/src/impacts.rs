/*
** Conquer Ops - Zero Hour gameplay core: what a shot hit, and the BO2 impact effect that belongs
** there.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The mod's surface vocabulary and the converted BO2 impact table.
//!
//! BO2 has no per-weapon impact effect: the weapon carries a bucket (`WeaponDef.impactType`) and
//! the engine reads the global `FxImpactTable` — one entry per bucket, 32 non-flesh surfaces plus
//! four flesh cells (body/head × nonfatal/fatal). The converter writes the cells the mod can
//! reach as `Data\ConquerOps\impacts.txt` ([`crate::names::IMPACT_FX_PATH`]); the bridge asks
//! [`fx_name`] through `zhbo_impact_fx` and plays the returned FXList at the hit point.
//!
//! The same file carries BO2's impact *sounds* for the projectile impact types (`sound
//! <impactType> <surface> <event>`): a blast plays the type's `<family>_<surface>` alias
//! ([`sound_name`], `zhbo_formats::fx::ImpactKind`), the way it plays the type's FX cell.
//!
//! A surface comes from what was hit: objects from their kind flags ([`Surface::of_object`], the
//! same three-way split the overhead General's hits use) and the ground from Zero Hour's own
//! terrain class through [`crate::host::LogicHost::surface_at`]. **Surface data is presentation
//! only** — it never feeds damage, movement or any other sim state (docs/CONTRACT.md).

use crate::host::{objflags, ObjInfo};
use std::sync::{OnceLock, RwLock};

/// What a shot hit. The discriminants are the ABI's `ZHBO_SURF_*` (`host/zhbo/zhbo_core.h`).
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Surface {
    /// Unknown, or a hit BO2's table only has a default cell for.
    #[default]
    Default = 0,
    /// A soldier: the four-way blood table (see [`flesh_variant`]).
    Flesh = 1,
    Metal = 2,
    Concrete = 3,
    Dirt = 4,
    Grass = 5,
    Asphalt = 6,
    Sand = 7,
    Snow = 8,
    Rock = 9,
    Wood = 10,
    Water = 11,
}

impl Surface {
    /// Every slot, in ABI order (a test pins the discriminants to this order).
    pub const ALL: [Surface; 12] = [
        Surface::Default,
        Surface::Flesh,
        Surface::Metal,
        Surface::Concrete,
        Surface::Dirt,
        Surface::Grass,
        Surface::Asphalt,
        Surface::Sand,
        Surface::Snow,
        Surface::Rock,
        Surface::Wood,
        Surface::Water,
    ];

    /// The slot's name, as the impact table and the log spell it.
    pub fn name(self) -> &'static str {
        match self {
            Surface::Default => "default",
            Surface::Flesh => "flesh",
            Surface::Metal => "metal",
            Surface::Concrete => "concrete",
            Surface::Dirt => "dirt",
            Surface::Grass => "grass",
            Surface::Asphalt => "asphalt",
            Surface::Sand => "sand",
            Surface::Snow => "snow",
            Surface::Rock => "rock",
            Surface::Wood => "wood",
            Surface::Water => "water",
        }
    }

    /// A slot by its name (the impact table's rows), or `None`.
    pub fn parse(s: &str) -> Option<Surface> {
        Surface::ALL.into_iter().find(|x| x.name() == s)
    }

    /// A slot from the ABI value; an unknown value is [`Surface::Default`].
    pub fn from_i32(v: i32) -> Surface {
        Surface::ALL.get(usize::try_from(v).unwrap_or(usize::MAX)).copied().unwrap_or(Surface::Default)
    }

    /// What an object hit is: a soldier bleeds, a vehicle or aircraft sparks, a structure is hard.
    /// The same split the overhead General's hits use (`host/zhbo/ZhboRtsWeapons.cpp`'s
    /// `playRtsHit`), from the same flags.
    pub fn of_object(o: Option<&ObjInfo>) -> Surface {
        let Some(o) = o else { return Surface::Default };
        if o.has(objflags::INFANTRY) {
            Surface::Flesh
        } else if o.has(objflags::VEHICLE) || o.has(objflags::AIRCRAFT) {
            Surface::Metal
        } else if o.has(objflags::STRUCTURE) {
            Surface::Concrete
        } else {
            Surface::Default
        }
    }
}

/// BO2's `FxFleshType` for a hit: 0 body nonfatal, 1 body fatal, 2 head nonfatal, 3 head fatal.
pub fn flesh_variant(head: bool, fatal: bool) -> usize {
    ((head as usize) << 1) | fatal as usize
}

/// One converted impact cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// BO2's `WeaponDef.impactType` (the table entry the shot uses).
    pub impact_type: i32,
    /// The surface; [`Surface::Flesh`] rows are keyed by `variant` instead.
    pub surface: Surface,
    /// `Some(0..3)` for a flesh row (BO2's `FxFleshType`), `None` for a non-flesh row.
    pub variant: Option<usize>,
    /// The converted Zero Hour FXList.
    pub fx: String,
}

/// Read the converted table text: `impact <impactType> <surface> <fxlist>`,
/// `flesh <impactType> <variant> <fxlist>`; `;` starts a comment. Malformed lines are skipped (the
/// converter writes this file, and a bad row must not take the game down).
pub fn parse(text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        let mut w = line.split_whitespace();
        match (w.next(), w.next(), w.next(), w.next(), w.next()) {
            (Some("impact"), Some(t), Some(s), Some(fx), None) => {
                let (Ok(t), Some(surface)) = (t.parse(), Surface::parse(s)) else { continue };
                rows.push(Row { impact_type: t, surface, variant: None, fx: fx.to_string() });
            }
            (Some("flesh"), Some(t), Some(v), Some(fx), None) => {
                let (Ok(t), Ok(v)) = (t.parse(), v.parse::<usize>()) else { continue };
                if v < 4 {
                    rows.push(Row { impact_type: t, surface: Surface::Flesh, variant: Some(v), fx: fx.to_string() });
                }
            }
            _ => {}
        }
    }
    rows
}

/// The cell for `impact_type` on `surface`: a non-flesh row, falling back to the same bucket's
/// default cell (BO2's `SURF_TYPE_DEFAULT` rule, `fx::impact`); for [`Surface::Flesh`] the blood
/// variant `head << 1 | fatal`, falling back to body-nonfatal. `None` = nothing converted.
pub fn find(rows: &[Row], impact_type: i32, surface: Surface, head: bool, fatal: bool) -> Option<&str> {
    if surface == Surface::Flesh {
        let variant = flesh_variant(head, fatal);
        return [Some(variant), Some(0)].into_iter().flatten().find_map(|v| {
            rows.iter()
                .find(|r| r.impact_type == impact_type && r.variant == Some(v))
                .map(|r| r.fx.as_str())
        });
    }
    let cell = |s: Surface| rows.iter().find(|r| r.impact_type == impact_type && r.variant.is_none() && r.surface == s).map(|r| r.fx.as_str());
    cell(surface).or_else(|| cell(Surface::Default))
}

/// One converted impact sound: the event a blast of `impact_type` on `surface` plays.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundRow {
    pub impact_type: i32,
    pub surface: Surface,
    /// The converted AudioEvent (`ZHBO_BO2_Snd_<family>_<surface>`).
    pub event: String,
}

/// The `sound <impactType> <surface> <event>` rows of the converted table (malformed ones skipped).
pub fn parse_sounds(text: &str) -> Vec<SoundRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        let mut w = line.split_whitespace();
        if let (Some("sound"), Some(t), Some(s), Some(event), None) = (w.next(), w.next(), w.next(), w.next(), w.next()) {
            if let (Ok(t), Some(surface)) = (t.parse(), Surface::parse(s)) {
                rows.push(SoundRow { impact_type: t, surface, event: event.to_string() });
            }
        }
    }
    rows
}

/// The sound for `impact_type` on `surface`: the exact cell only. BO2 hashes `<family>_<surface>`
/// and a hash no loaded bank holds plays nothing — `FUN_008352b0` (the sound thread's play command)
/// asks `FUN_0082df80`, whose miss (negative-cached, `FUN_0082dec0` walks every bank's hash chain)
/// returns 0 and calls the empty `FUN_00a06580`; there is no default-surface fallback.
/// So `wpn_rocket_explode_flesh`, absent from the install, is silent as in BO2.
pub fn find_sound<S: AsRef<str>>(rows: &[(i32, Surface, S)], impact_type: i32, surface: Surface) -> Option<&S> {
    rows.iter().find(|r| r.0 == impact_type && r.1 == surface).map(|r| &r.2)
}

/// Whether `impact_type` has any sound cell (its family converted): a blast of a type without one
/// (an unconverted archive, a mod-only def) falls back to the generic blast, one with one plays
/// BO2's cell or nothing.
pub fn has_sounds<S>(rows: &[(i32, Surface, S)], impact_type: i32) -> bool {
    rows.iter().any(|r| r.0 == impact_type)
}

/// The table itself: installed once before play from the converted file. The empty table holds
/// until then, so a missing/stale archive plays no impact FX (the impact sound still plays).
static TABLE: OnceLock<RwLock<Vec<Row>>> = OnceLock::new();

fn table() -> &'static RwLock<Vec<Row>> {
    TABLE.get_or_init(|| RwLock::new(Vec::new()))
}

/// The installed sound cells. The events are interned (`'static`, leaked once per install — the
/// table is installed once per archive load) so a cue can name them like every other sound.
type SoundCell = (i32, Surface, &'static str);
static SOUNDS: OnceLock<RwLock<Vec<SoundCell>>> = OnceLock::new();

fn sounds() -> &'static RwLock<Vec<SoundCell>> {
    SOUNDS.get_or_init(|| RwLock::new(Vec::new()))
}

/// Install the converted table (`Data\ConquerOps\impacts.txt`); returns the row count (FX and
/// sound cells).
pub fn install(text: &str) -> usize {
    let rows = parse(text);
    let cells: Vec<SoundCell> =
        parse_sounds(text).into_iter().map(|r| (r.impact_type, r.surface, &*Box::leak(r.event.into_boxed_str()))).collect();
    let n = rows.len() + cells.len();
    *table().write().unwrap_or_else(|e| e.into_inner()) = rows;
    *sounds().write().unwrap_or_else(|e| e.into_inner()) = cells;
    n
}

/// The converted sound event for a blast of `impact_type` on `surface` (`None`: no cell).
pub fn sound_name(impact_type: i32, surface: Surface) -> Option<&'static str> {
    let cells = sounds().read().unwrap_or_else(|e| e.into_inner());
    find_sound(&cells, impact_type, surface).copied()
}

/// Whether the installed table has sound cells for `impact_type` ([`has_sounds`]).
pub fn type_has_sounds(impact_type: i32) -> bool {
    has_sounds(&sounds().read().unwrap_or_else(|e| e.into_inner()), impact_type)
}

/// The converted FXList for a hit through the installed table (the ABI's lookup).
pub fn fx_name(impact_type: i32, surface: Surface, head: bool, fatal: bool) -> Option<String> {
    let rows = table().read().unwrap_or_else(|e| e.into_inner());
    find(&rows, impact_type, surface, head, fatal).map(str::to_string)
}

/// The one table the in-process tests install (the table is global, so every test that installs
/// one installs this same text): blast type 77 and bullet type 78, on dirt and concrete, with water
/// absent (`NONE`, BO2's hash miss).
#[cfg(test)]
pub const TEST_TABLE: &str = "\
impact 77 default FX_D
impact 77 concrete FX_C
sound 77 dirt SND_D
sound 77 concrete SND_C
sound 77 water NONE
sound 78 dirt HIT_DIRT
sound 78 concrete HIT_CONCRETE
sound 78 flesh HIT_FLESH
sound 78 water NONE
";

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
; impactType surface fxlist
impact 2 default ZHBO_BO2FX_b2_dirt
impact 2 concrete ZHBO_BO2FX_b2_concrete
flesh 2 0 ZHBO_BO2FX_b2_flesh_body
flesh 2 3 ZHBO_BO2FX_b2_flesh_head_fatal
sound 7 default ZHBO_BO2_Snd_wpn_grenade_explode_default
sound 7 concrete ZHBO_BO2_Snd_wpn_grenade_explode_concrete
sound 7 bogus ZHBO_BO2_Snd_x
garbage line
";

    #[test]
    fn the_slots_are_numbered_as_the_abi_says() {
        // host/zhbo/zhbo_core.h's ZHBO_SURF_* in the same order; `from_i32` is the ABI's door.
        for (i, s) in Surface::ALL.iter().enumerate() {
            assert_eq!(*s as i32, i as i32, "{s:?}");
            assert_eq!(Surface::from_i32(i as i32), *s);
            assert_eq!(Surface::from_i32(i as i32 + 100), Surface::Default);
            assert_eq!(Surface::parse(s.name()), Some(*s));
        }
        assert_eq!(Surface::ALL.len(), 12);
    }

    #[test]
    fn a_hit_picks_its_cell_and_falls_back_like_bo2() {
        let rows = parse(TABLE);
        assert_eq!(rows.len(), 4, "the malformed line is skipped");
        // The exact cell.
        assert_eq!(find(&rows, 2, Surface::Concrete, false, false), Some("ZHBO_BO2FX_b2_concrete"));
        // A surface the bucket has no cell for: its default cell (BO2's SURF_TYPE_DEFAULT rule).
        assert_eq!(find(&rows, 2, Surface::Metal, false, false), Some("ZHBO_BO2FX_b2_dirt"));
        // A bucket with no rows at all.
        assert_eq!(find(&rows, 9, Surface::Concrete, false, false), None);
        // Blood: the four-way table, and a missing variant falls back to body-nonfatal.
        assert_eq!(find(&rows, 2, Surface::Flesh, false, false), Some("ZHBO_BO2FX_b2_flesh_body"));
        assert_eq!(find(&rows, 2, Surface::Flesh, true, true), Some("ZHBO_BO2FX_b2_flesh_head_fatal"));
        assert_eq!(find(&rows, 2, Surface::Flesh, true, false), Some("ZHBO_BO2FX_b2_flesh_body"));
    }

    #[test]
    fn a_blast_picks_its_exact_sound_cell_or_none_like_bo2() {
        let rows: Vec<_> = parse_sounds(TABLE).into_iter().map(|r| (r.impact_type, r.surface, r.event)).collect();
        assert_eq!(rows.len(), 2, "the unknown surface is skipped");
        assert_eq!(parse(TABLE).len(), 4, "sound rows are not FX rows");
        assert_eq!(find_sound(&rows, 7, Surface::Concrete).map(String::as_str), Some("ZHBO_BO2_Snd_wpn_grenade_explode_concrete"));
        assert_eq!(find_sound(&rows, 7, Surface::Water), None, "no alias for the surface: BO2 plays nothing");
        assert!(has_sounds(&rows, 7));
        assert_eq!(find_sound(&rows, 9, Surface::Concrete), None);
        assert!(!has_sounds(&rows, 9));
    }

    #[test]
    fn fatal_and_head_are_bo2s_flesh_bits() {
        assert_eq!(flesh_variant(false, false), 0);
        assert_eq!(flesh_variant(false, true), 1);
        assert_eq!(flesh_variant(true, false), 2);
        assert_eq!(flesh_variant(true, true), 3);
    }

    #[test]
    fn objects_classify_by_their_kind_flags() {
        let o = |f: u32| ObjInfo { flags: f, ..Default::default() };
        assert_eq!(Surface::of_object(Some(&o(objflags::INFANTRY))), Surface::Flesh);
        assert_eq!(Surface::of_object(Some(&o(objflags::VEHICLE))), Surface::Metal);
        assert_eq!(Surface::of_object(Some(&o(objflags::AIRCRAFT))), Surface::Metal);
        assert_eq!(Surface::of_object(Some(&o(objflags::STRUCTURE))), Surface::Concrete);
        assert_eq!(Surface::of_object(Some(&o(0))), Surface::Default);
        assert_eq!(Surface::of_object(None), Surface::Default);
        // A structure that is also a vehicle (a building with a gun) sparks: order is deliberate.
        assert_eq!(Surface::of_object(Some(&o(objflags::STRUCTURE | objflags::VEHICLE))), Surface::Metal);
    }
}
