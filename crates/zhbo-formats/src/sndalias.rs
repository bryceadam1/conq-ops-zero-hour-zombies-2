//! Black Ops II (T6) sound aliases, read from a fully loaded zone (`t6zone::Zone`).
//!
//! A zone's `SndBank` asset (e.g. `mpl_common.all` in `common_mp.ff`) names every sound: an alias
//! such as `wpn_an94_fire_plr` lists one or more variants, each pointing by `assetId` at an entry of
//! the matching `sound/<bank>.sabs`/`.sabl` file (or of the bank's `loadedAssets`, kept in the zone),
//! with its volume, pitch and distance ranges. Layout from OpenAssetTools' `T6_Assets.h`, sizes as
//! its ZoneCodeGenerator computes them (32-bit PC).

use crate::t6zone::layouts::{SndAlias as SA, SndAliasList as SL, SndBank as SB};
use crate::t6zone::Zone;
use crate::FormatError;

const ALIAS_LIST_SIZE: u32 = SL::SIZE;
const ALIAS_SIZE: u32 = SA::SIZE;
const LOADED_ASSETS: u32 = SB::loadedAssets;
const ASSET_ENTRY_SIZE: u32 = 20;
pub const ASSET_SOUND: u32 = 9;

#[derive(Clone, Debug)]
pub struct Variant {
    /// Entry id in the bank's `.sabs`/`.sabl`, or in `Bank::loaded`.
    pub asset_id: u32,
    pub file: String,
    /// 0..1.
    pub volume: (f32, f32),
    /// Playback rate multiplier.
    pub pitch: (f32, f32),
    /// Dry `(distMin, distMax)` in inches. BO2 stores every alias distance halved: the sound thread
    /// multiplies the u16 by 2.0 (`DAT_00c6f6ac`) at each use (`FUN_0082acf0`, `FUN_0082b9f0`).
    pub distance: (f32, f32),
    /// The voice waits this long after the play call before it sounds (`FUN_0082c470` stores
    /// `startDelay` with the voice); each layer of a secondary chain has its own.
    pub start_delay_ms: u16,
    /// How BO2's mixer levels it: bus, volume group, pan, distance curves ([`Mix`]).
    pub mix: Mix,
    /// `secondaryName`: the alias BO2 starts together with this one (a gunshot's low-end layer,
    /// whose own secondary is the tail: `wpn_an94_fire_plr` -> `wpn_an94_lfe` -> `_fire_plr_decay`).
    pub secondary: Option<String>,
}

/// The mixer fields of an alias (`SndAliasFlags` bit layout from OpenAssetTools' `T6_Assets.h`):
/// indices into `SndDriverGlobals` (`crate::sndglobals`) and the bus enum (`SOUND_BUS_IDS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mix {
    /// `busType` (flags0 bits 11-14): 0 reverb, 1 fx, 2 voice, 3 pfutz, 4 hdrfx, 5 ui, 6 music,
    /// 7 movie, 8 reference ([`crate::sndglobals::Bus`]).
    pub bus: u8,
    /// `volumeGroup` (flags0 bits 17-21): index into the driver globals' groups.
    pub group: u8,
    /// `pan` (+92): index into the driver globals' pans.
    pub pan: u8,
    /// `panType` (flags0 bit 1): 3D (positional) rather than 2D.
    pub positional: bool,
    /// `volumeFalloffCurve` (flags1 bits 2-7): the dry curve from distMin to distMax.
    pub falloff_curve: u8,
    /// `volumeMinFalloffCurve` (flags1 bits 14-19): the dry curve from 0 to distMin.
    pub min_falloff_curve: u8,
}

impl Mix {
    /// Decode the two flag words.
    pub fn from_flags(flags0: u32, flags1: u32, pan: u8) -> Mix {
        Mix {
            bus: ((flags0 >> 11) & 0xf) as u8,
            group: ((flags0 >> 17) & 0x1f) as u8,
            pan,
            positional: (flags0 >> 1) & 1 != 0,
            falloff_curve: ((flags1 >> 2) & 0x3f) as u8,
            min_falloff_curve: ((flags1 >> 14) & 0x3f) as u8,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Alias {
    pub name: String,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug)]
pub struct LoadedEntry {
    pub id: u32,
    pub size: u32,
    pub offset: u32,
    pub frame_count: u32,
    pub rate_index: u8,
    pub channels: u8,
    pub looping: bool,
    pub format: u8,
}

#[derive(Clone, Debug)]
pub struct Bank {
    pub name: String,
    pub aliases: Vec<Alias>,
    /// Sounds kept in the zone itself (`loadedAssets`), with their data.
    pub loaded: Vec<LoadedEntry>,
    pub loaded_data: Vec<u8>,
}

impl Bank {
    /// The alias named `name`, ignoring ASCII case: BO2 looks aliases up by a hash of the
    /// lowercased name (`FUN_0083c1d0`: seed 0x1505, ×0x1003f + `tolower(c)`), so a reference
    /// spelt `wpn_sig556_silencer_LFE` plays the alias `wpn_sig556_silencer_lfe`.
    pub fn alias(&self, name: &str) -> Option<&Alias> {
        self.aliases.iter().find(|a| a.name.eq_ignore_ascii_case(name))
    }
}

/// u16 volume/pitch fields: 0..65535 as 0..1 (volume) or 0..2 (pitch, BO2 stores rate / 2).
fn unit(v: u16) -> f32 {
    v as f32 / 65535.0
}

/// BO2's distance unit: the stored u16 is half the distance in inches (`DAT_00c6f6ac` = 2.0).
pub const DISTANCE_SCALE: f32 = 2.0;

/// Every sound bank in a loaded zone.
pub fn banks(zone: &Zone) -> Result<Vec<Bank>, FormatError> {
    zone.assets.iter().filter(|a| a.kind == ASSET_SOUND).map(|a| read_bank(zone, a.addr)).collect()
}

fn read_bank(z: &Zone, b: u32) -> Result<Bank, FormatError> {
    let name = z.try_str_ptr(b)?;
    let count = z.try_u32(b + SB::aliasCount)?;
    let lists = z.try_u32(b + SB::alias)?;
    let mut aliases = Vec::new();
    if lists != 0 {
        for i in 0..count {
            let l = lists + ALIAS_LIST_SIZE * i;
            let head = z.u32(l + SL::head);
            let n = z.u32(l + SL::count).min(256);
            let variants = (0..n)
                .filter(|_| head != 0)
                .map(|j| {
                    let a = head + ALIAS_SIZE * j;
                    Variant {
                        asset_id: z.u32(a + SA::assetId),
                        file: z.str_ptr(a + 20),
                        start_delay_ms: z.u16(a + 54),
                        secondary: Some(z.str_ptr(a + SA::secondaryName)).filter(|s| !s.is_empty()),
                        volume: (unit(z.u16(a + 60)), unit(z.u16(a + 62))),
                        pitch: (unit(z.u16(a + 64)) * 2.0, unit(z.u16(a + 66)) * 2.0),
                        distance: (z.u16(a + SA::distMin) as f32 * DISTANCE_SCALE, z.u16(a + SA::distMax) as f32 * DISTANCE_SCALE),
                        mix: Mix::from_flags(z.u32(a + SA::flags), z.u32(a + SA::flags + 4), z.u8(a + SA::pan)),
                    }
                })
                .collect();
            aliases.push(Alias { name: z.str_ptr(l), variants });
        }
    }
    let la = b + LOADED_ASSETS;
    let entry_count = z.u32(la + 12);
    let entries = z.u32(la + 16);
    let data_size = z.u32(la + 20);
    let data = z.u32(la + 24);
    let loaded = if entries == 0 {
        Vec::new()
    } else {
        (0..entry_count.min(1 << 16))
            .map(|i| {
                let e = entries + ASSET_ENTRY_SIZE * i;
                LoadedEntry {
                    id: z.u32(e),
                    size: z.u32(e + 4),
                    offset: z.u32(e + 8),
                    frame_count: z.u32(e + 12),
                    rate_index: z.u8(e + 16),
                    channels: z.u8(e + 17),
                    looping: z.u8(e + 18) != 0,
                    format: z.u8(e + 19),
                }
            })
            .collect()
    };
    let loaded_data = if data != 0 { z.bytes(data, data_size as usize).to_vec() } else { Vec::new() };
    Ok(Bank { name, aliases, loaded, loaded_data })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the player's BO2 install (with Multiplayer).
    #[test]
    #[ignore]
    fn reads_weapon_aliases() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/common_mp.ff")).unwrap();
        let zone = crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap();
        let banks = banks(&zone).unwrap();
        let common = banks.iter().find(|b| b.name == "mpl_common.all").expect("mpl_common.all");
        assert!(common.aliases.len() > 500, "{} aliases", common.aliases.len());
        let fire = common.aliases.iter().find(|a| a.name.contains("an94") && a.name.contains("fire")).expect("an94 fire");
        assert!(!fire.variants.is_empty() && fire.variants.iter().all(|v| v.asset_id != 0));
        assert!(fire.variants.iter().all(|v| v.volume.1 > 0.0 && v.volume.1 <= 1.0), "{fire:?}");
    }

    /// The mixer fields against OpenAssetTools' CSV dump of the same bank (`Bus`, `VolumeGroup`,
    /// `PanType`, `DistMin`/`DistMaxDry` (×2), `DryMinCurve`/`DryMaxCurve`): the AN-94's shot is a 2D
    /// fx-bus `grp_weapon` alias, its third-person close layer a 3D one fading out by `cosdelay`
    /// into `alloff`, its distant layer fading in by `sindelay`, its low end on the HDR bus.
    #[test]
    #[ignore]
    fn reads_mixer_fields() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/common_mp.ff")).unwrap();
        let zone = crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap();
        let banks = banks(&zone).unwrap();
        let common = banks.iter().find(|b| b.name == "mpl_common.all").expect("mpl_common.all");
        let v = |n: &str| common.alias(n).unwrap_or_else(|| panic!("{n}")).variants[0].clone();
        // OAT's SOUND_GROUPS / SOUND_CURVES order: grp_weapon 15, grp_wpn_lfe 2; default 0,
        // allon 2, alloff 3, sindelay 11, cosdelay 12.
        let plr = v("wpn_an94_fire_plr");
        assert_eq!((plr.mix.bus, plr.mix.group, plr.mix.positional), (1, 15, false), "{plr:?}");
        let lfe = v("wpn_an94_lfe");
        assert_eq!((lfe.mix.bus, lfe.mix.group), (4, 2), "{lfe:?}");
        let npc = v("wpn_an94_fire_npc");
        assert!(npc.mix.positional);
        assert_eq!((npc.mix.min_falloff_curve, npc.mix.falloff_curve), (12, 3));
        assert_eq!(npc.distance, (1800.0, 1800.0));
        let dist = v("wpn_an94_fire_npc_dist");
        assert_eq!((dist.mix.min_falloff_curve, dist.mix.falloff_curve), (11, 0));
        assert_eq!(dist.distance, (1800.0, 12000.0));
    }
}
