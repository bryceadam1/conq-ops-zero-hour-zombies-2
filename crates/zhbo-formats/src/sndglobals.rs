//! Black Ops II's sound mixer tables: the `SndDriverGlobals` asset (`code_post_gfx_mp.ff`) — volume
//! groups, falloff curves, pans and the master snapshots (bus gains, compressor, limiter). An alias
//! (`crate::sndalias::Mix`) indexes these. Layout from OpenAssetTools' `T6_Assets.h`; how the sound
//! thread and the DSP use each field is transcribed in `zhbo_assets::mixer`, with the t6mp
//! functions cited there.

use crate::t6zone::layouts::{
    SndCurve as SC, SndDriverGlobals as SG, SndMaster as SM, SndPan as SP, SndVolumeGroup as SV,
};
use crate::t6zone::Zone;
use crate::FormatError;

pub const ASSET_SND_DRIVER_GLOBALS: u32 = 32;

/// BO2's buses (`busType`, OpenAssetTools' `SOUND_BUS_IDS`), the order of the DSP's bus buffers
/// and of the snapshot's `bus*G` gains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Bus {
    Reverb,
    Fx,
    Voice,
    Pfutz,
    Hdrfx,
    Ui,
    Music,
    Movie,
    Reference,
}

impl Bus {
    pub const ALL: [Bus; 9] =
        [Bus::Reverb, Bus::Fx, Bus::Voice, Bus::Pfutz, Bus::Hdrfx, Bus::Ui, Bus::Music, Bus::Movie, Bus::Reference];

    pub fn from_index(i: u8) -> Option<Bus> {
        Bus::ALL.get(i as usize).copied()
    }
}

#[derive(Clone, Debug)]
pub struct Group {
    pub name: String,
    /// Index of the parent group, `None` at the root (`parentIndex` -1).
    pub parent: Option<usize>,
    /// `attenuationMp` (what t6mp reads, +0x4e) as a 0..1 gain.
    pub attenuation_mp: f32,
    pub attenuation_sp: f32,
}

#[derive(Clone, Debug)]
pub struct Curve {
    pub name: String,
    /// Eight (x, y) points, x rising 0..1 (evaluated by `zhbo_core::soundlayers::curve_eval`).
    pub points: [(f32, f32); 8],
}

#[derive(Clone, Debug)]
pub struct Pan {
    pub name: String,
    pub front: f32,
    pub back: f32,
    pub center: f32,
    pub lfe: f32,
    pub left: f32,
    pub right: f32,
}

/// One dynamics stage of the master (`comp*`/`limit*`): `[E, PG, MG, T, R, TA, TR]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dynamics {
    pub enabled: bool,
    pub pre_gain: f32,
    pub makeup_gain: f32,
    pub threshold: f32,
    pub ratio: f32,
    pub attack: f32,
    pub release: f32,
}

/// The master EQ's band shapes, in the snapshot's order (`FUN_006f5040`: `FUN_00833e50` low
/// shelf, `FUN_00833fc2` peak twice, `FUN_00833ce8` high shelf — RBJ cookbook biquads).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandKind {
    LowShelf,
    Peak,
    HighShelf,
}

/// One EQ band: `[E, G (dB), F (Hz), Q]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub kind: BandKind,
    pub enabled: bool,
    pub gain_db: f32,
    pub freq: f32,
    pub q: f32,
}

#[derive(Clone, Debug)]
pub struct Master {
    pub name: String,
    /// `low`, `peak1`, `peak2`, `hi`.
    pub eq: [Band; 4],
    /// `eqG`: the linear gain the EQ applies first.
    pub eq_gain: f32,
    pub compressor: Dynamics,
    pub limiter: Dynamics,
    /// `bus*G` gated by `bus*E` (a disabled bus is silent, `FUN_0082fc60`), in [`Bus`] order for
    /// reverb..movie; the reference bus has no gain field and is 1.
    pub bus_gain: [f32; 9],
    /// `hdrfxCompE`: the HDR bus joins before the compressor rather than after it.
    pub hdrfx_compressed: bool,
    pub voice_eq: bool,
    pub voice_compressed: bool,
}

impl Master {
    pub fn bus(&self, bus: Bus) -> f32 {
        self.bus_gain[bus as usize]
    }
}

#[derive(Clone, Debug)]
pub struct DriverGlobals {
    pub name: String,
    pub groups: Vec<Group>,
    pub curves: Vec<Curve>,
    pub pans: Vec<Pan>,
    pub masters: Vec<Master>,
}

impl DriverGlobals {
    /// The product of `attenuationMp` from `group` up through its parents (`FUN_0082b9f0`: the walk
    /// stops at index -1, or after 100 steps).
    pub fn group_gain(&self, group: usize) -> f32 {
        let mut gain = 1.0;
        let mut at = Some(group);
        for _ in 0..100 {
            let Some(g) = at.and_then(|i| self.groups.get(i)) else { break };
            gain *= g.attenuation_mp;
            at = g.parent;
        }
        gain
    }

    pub fn curve(&self, index: u8) -> Option<&Curve> {
        self.curves.get(index as usize)
    }

    pub fn pan(&self, index: u8) -> Option<&Pan> {
        self.pans.get(index as usize)
    }

    /// The snapshot the sound system starts in (`FUN_0082a9d0` looks up `"default"`), else the
    /// first.
    pub fn default_master(&self) -> Option<&Master> {
        self.masters.iter().find(|m| m.name == "default").or(self.masters.first())
    }
}

fn fixed_str(z: &Zone, addr: u32, len: usize) -> String {
    let b = z.bytes(addr, len);
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..n]).into_owned()
}

fn unit(v: u16) -> f32 {
    v as f32 / 65535.0
}

/// The zone's `SndDriverGlobals`, if it has one.
pub fn read(zone: &Zone) -> Result<Option<DriverGlobals>, FormatError> {
    let Some(asset) = zone.assets.iter().find(|a| a.kind == ASSET_SND_DRIVER_GLOBALS) else { return Ok(None) };
    let b = asset.addr;
    let name = zone.try_str_ptr(b + SG::name)?;
    let list = |count: u32, ptr: u32, size: u32| -> Result<Vec<u32>, FormatError> {
        let n = zone.try_u32(b + count)?.min(1024);
        let p = zone.try_u32(b + ptr)?;
        Ok(if p == 0 { Vec::new() } else { (0..n).map(|i| p + i * size).collect() })
    };
    let groups = list(SG::groupCount, SG::groups, SV::SIZE)?
        .into_iter()
        .map(|g| {
            let parent = zone.u32(g + SV::parentIndex) as i32;
            Group {
                name: fixed_str(zone, g + SV::name, 32),
                parent: (parent >= 0).then_some(parent as usize),
                attenuation_mp: unit(zone.u16(g + SV::attenuationMp)),
                attenuation_sp: unit(zone.u16(g + SV::attenuationSp)),
            }
        })
        .collect();
    let curves = list(SG::curveCount, SG::curves, SC::SIZE)?
        .into_iter()
        .map(|c| {
            let mut points = [(0.0, 0.0); 8];
            for (i, p) in points.iter_mut().enumerate() {
                let a = c + SC::points + 8 * i as u32;
                *p = (zone.f32(a), zone.f32(a + 4));
            }
            Curve { name: fixed_str(zone, c + SC::name, 32), points }
        })
        .collect();
    let pans = list(SG::panCount, SG::pans, SP::SIZE)?
        .into_iter()
        .map(|p| Pan {
            name: fixed_str(zone, p + SP::name, 32),
            front: zone.f32(p + SP::front),
            back: zone.f32(p + SP::back),
            center: zone.f32(p + SP::center),
            lfe: zone.f32(p + SP::lfe),
            left: zone.f32(p + SP::left),
            right: zone.f32(p + SP::right),
        })
        .collect();
    let masters = list(SG::masterCount, SG::masters, SM::SIZE)?
        .into_iter()
        .map(|m| {
            let dynamics = |e: u32| Dynamics {
                enabled: zone.u32(m + e) != 0,
                pre_gain: zone.f32(m + e + 4),
                makeup_gain: zone.f32(m + e + 8),
                threshold: zone.f32(m + e + 12),
                ratio: zone.f32(m + e + 16),
                attack: zone.f32(m + e + 20),
                release: zone.f32(m + e + 24),
            };
            let mut bus_gain = [1.0; 9];
            for (i, g) in bus_gain.iter_mut().take(8).enumerate() {
                let on = zone.u32(m + SM::busReverbE + 4 * i as u32) != 0;
                *g = if on { zone.f32(m + SM::busReverbG + 4 * i as u32) } else { 0.0 };
            }
            let band = |e: u32, kind| Band {
                kind,
                enabled: zone.u32(m + e) != 0,
                gain_db: zone.f32(m + e + 4),
                freq: zone.f32(m + e + 8),
                q: zone.f32(m + e + 12),
            };
            Master {
                name: fixed_str(zone, m + SM::name, 32),
                eq: [
                    band(SM::lowE, BandKind::LowShelf),
                    band(SM::peak1E, BandKind::Peak),
                    band(SM::peak2E, BandKind::Peak),
                    band(SM::hiE, BandKind::HighShelf),
                ],
                eq_gain: zone.f32(m + SM::eqG),
                compressor: dynamics(SM::compE),
                limiter: dynamics(SM::limitE),
                bus_gain,
                hdrfx_compressed: zone.u32(m + SM::hdrfxCompE) != 0,
                voice_eq: zone.u32(m + SM::voiceEqE) != 0,
                voice_compressed: zone.u32(m + SM::voiceCompE) != 0,
            }
        })
        .collect();
    Ok(Some(DriverGlobals { name, groups, curves, pans, masters }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_gain_walks_the_parents() {
        let g = |name: &str, parent: Option<usize>, a: f32| Group { name: name.into(), parent, attenuation_mp: a, attenuation_sp: a };
        let dg = DriverGlobals {
            name: String::new(),
            groups: vec![g("grp_reference", None, 1.0), g("grp_master", None, 0.5623), g("grp_weapon", Some(1), 0.5623), g("loop", Some(3), 0.5)],
            curves: vec![],
            pans: vec![],
            masters: vec![],
        };
        assert!((dg.group_gain(2) - 0.3162).abs() < 1e-3);
        assert_eq!(dg.group_gain(0), 1.0);
        assert!(dg.group_gain(3) < 1e-20, "a cycle stops after 100 steps");
    }

    /// Needs the player's BO2 install: the tables match OpenAssetTools' CSV dump of the asset
    /// (`code_post_gfx_mp/soundbank/globals/*.csv`).
    #[test]
    #[ignore]
    fn reads_the_driver_globals() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/code_post_gfx_mp.ff")).unwrap();
        let zone = crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap();
        let dg = read(&zone).unwrap().expect("SndDriverGlobals in code_post_gfx_mp");
        let group = |n: &str| dg.groups.iter().position(|g| g.name == n).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(dg.groups.len(), 26);
        assert_eq!(group("grp_weapon"), 15);
        assert_eq!(dg.groups[group("grp_weapon")].parent, Some(group("grp_master")));
        assert!((dg.group_gain(group("grp_weapon")) - (36853.0f32 / 65535.0).powi(2)).abs() < 1e-4);
        assert!((dg.group_gain(group("grp_wpn_lfe")) - 52056.0 / 65535.0).abs() < 1e-4);
        assert_eq!(dg.curves[11].name, "sindelay");
        assert_eq!(dg.curves[12].name, "cosdelay");
        let lfe = dg.pans.iter().find(|p| p.name == "lfe").unwrap();
        assert_eq!((lfe.front, lfe.back, lfe.center, lfe.lfe), (0.0, 0.0, 0.0, 1.0));
        let m = dg.default_master().unwrap();
        assert_eq!(m.name, "default");
        assert!((m.bus(Bus::Fx) - 0.152477).abs() < 1e-5 && (m.bus(Bus::Voice) - 5.615831).abs() < 1e-5);
        assert!((m.bus(Bus::Hdrfx) - 2.88864).abs() < 1e-5);
        assert!(m.compressor.enabled && (m.compressor.ratio - 22.36621).abs() < 1e-4);
        assert!(!m.hdrfx_compressed && !m.voice_compressed && !m.voice_eq);
        // master.csv: low +5 dB at 132.3 Hz, peak1 +5 at 357.9, peak2 -8 at 2772.5 (Q 9.80), hi +3.
        let eq: Vec<(bool, f32, f32)> = m.eq.iter().map(|b| (b.enabled, b.gain_db, b.freq.round())).collect();
        assert_eq!(eq, [(true, 5.0, 132.0), (true, 5.0, 358.0), (true, -8.0, 2773.0), (true, 3.0, 4622.0)]);
        assert_eq!(m.eq_gain, 1.0);
    }
}
