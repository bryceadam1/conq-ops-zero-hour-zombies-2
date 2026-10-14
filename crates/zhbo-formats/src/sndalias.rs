//! Black Ops II (T6) sound aliases, read from a fully loaded zone (`t6zone::Zone`).
//!
//! A zone's `SndBank` asset (e.g. `mpl_common.all` in `common_mp.ff`) names every sound: an alias
//! such as `wpn_an94_fire_plr` lists one or more variants, each pointing by `assetId` at an entry of
//! the matching `sound/<bank>.sabs`/`.sabl` file (or of the bank's `loadedAssets`, kept in the zone),
//! with its volume, pitch and distance ranges. Layout from OpenAssetTools' `T6_Assets.h`, sizes as
//! its ZoneCodeGenerator computes them (32-bit PC).

use crate::t6zone::Zone;

const ALIAS_LIST_SIZE: u32 = 20;
const ALIAS_SIZE: u32 = 96;
const LOADED_ASSETS: u32 = 4708; // SndBank::loadedAssets
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
    /// Inches.
    pub distance: (f32, f32),
    pub start_delay_ms: u16,
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
    pub fn alias(&self, name: &str) -> Option<&Alias> {
        self.aliases.iter().find(|a| a.name == name)
    }
}

/// u16 volume/pitch fields: 0..65535 as 0..1 (volume) or 0..2 (pitch, BO2 stores rate / 2).
fn unit(v: u16) -> f32 {
    v as f32 / 65535.0
}

/// Every sound bank in a loaded zone.
pub fn banks(zone: &Zone) -> Vec<Bank> {
    zone.assets.iter().filter(|a| a.kind == ASSET_SOUND).map(|a| read_bank(zone, a.addr)).collect()
}

fn read_bank(z: &Zone, b: u32) -> Bank {
    let name = z.str_ptr(b);
    let count = z.u32(b + 4);
    let lists = z.u32(b + 8);
    let mut aliases = Vec::new();
    if lists != 0 {
        for i in 0..count {
            let l = lists + ALIAS_LIST_SIZE * i;
            let head = z.u32(l + 8);
            let n = z.u32(l + 12).min(256);
            let variants = (0..n)
                .filter(|_| head != 0)
                .map(|j| {
                    let a = head + ALIAS_SIZE * j;
                    Variant {
                        asset_id: z.u32(a + 16),
                        file: z.str_ptr(a + 20),
                        start_delay_ms: z.u16(a + 54),
                        volume: (unit(z.u16(a + 60)), unit(z.u16(a + 62))),
                        pitch: (unit(z.u16(a + 64)) * 2.0, unit(z.u16(a + 66)) * 2.0),
                        distance: (z.u16(a + 68) as f32, z.u16(a + 70) as f32),
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
    Bank { name, aliases, loaded, loaded_data }
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
        let banks = banks(&zone);
        let common = banks.iter().find(|b| b.name == "mpl_common.all").expect("mpl_common.all");
        assert!(common.aliases.len() > 500, "{} aliases", common.aliases.len());
        let fire = common.aliases.iter().find(|a| a.name.contains("an94") && a.name.contains("fire")).expect("an94 fire");
        assert!(!fire.variants.is_empty() && fire.variants.iter().all(|v| v.asset_id != 0));
        assert!(fire.variants.iter().all(|v| v.volume.1 > 0.0 && v.volume.1 <= 1.0), "{fire:?}");
    }
}
