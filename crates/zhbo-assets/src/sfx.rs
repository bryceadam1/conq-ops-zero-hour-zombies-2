//! Black Ops II sound effects for first person, converted from the player's own BO2 install into
//! `ConquerOps_BO2.big`: WAVs under `Data\Audio\Sounds`, Zero Hour AudioEvents in
//! `Data\INI\SoundEffects\ZHBO_BO2.ini` (the engine loads that folder after `SoundEffects.ini`), and
//! the viewmodel's sound notetracks in `Data\ConquerOps\viewmodel_sounds.txt`.
//!
//! Sounds are found by BO2 alias name (`zhbo_formats::sndalias`); each alias variant becomes one WAV,
//! and a multi-layer event (a gunshot is shot + tail + low end) is mixed into one WAV per variant.
//! zhbo-core plays the events by name (`zhbo_core::sounds`); `ZhboViewmodel.cpp` plays notetracks.

use crate::sounds::wav_bytes;
use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::sab::{SoundBank, FORMAT_FLAC, FORMAT_PCM16, FRAME_RATES};
use zhbo_formats::sndalias::{self, Bank};
use zhbo_formats::t6zone::Zone;
use zhbo_formats::xanim;

/// Where an event's aliases are: the weapon/player bank, or a map bank (footsteps, impacts).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    Common,
    World,
}

pub struct Sfx {
    /// AudioEvent name (zhbo_core::sounds uses the same names).
    pub event: &'static str,
    pub from: From,
    /// Aliases mixed into each variant.
    pub layers: &'static [&'static str],
    pub kind: Kind,
    pub limit: u8,
}

/// An event made at conversion time (a converted weapon's own sounds), from the common bank.
#[derive(Clone, Debug)]
pub struct DynSfx {
    pub event: String,
    /// Aliases mixed into each variant; ones the bank lacks are skipped.
    pub layers: Vec<String>,
    pub kind: Kind,
    pub limit: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The player's own 2D sound.
    Player,
    /// Positional, heard by everyone.
    World,
    /// Positional, heard by everyone except its owner (the owner hears the `Player` version).
    Others,
}

impl Kind {
    fn ini(self) -> &'static str {
        match self {
            Kind::Player => "ui player",
            Kind::World => "world shrouded everyone",
            Kind::Others => "world shrouded allies enemies",
        }
    }
}

pub const SFX: &[Sfx] = &[
    Sfx { event: "ZHBO_BO2_Fire", from: From::Common, layers: &["wpn_an94_fire_plr", "wpn_an94_fire_plr_decay", "wpn_an94_lfe"], kind: Kind::Player, limit: 3 },
    Sfx { event: "ZHBO_BO2_FirePistol", from: From::Common, layers: &["wpn_beretta93r_fire_plr", "wpn_beretta93r_fire_plr_decay", "wpn_beretta93r_LFE"], kind: Kind::Player, limit: 3 },
    Sfx { event: "ZHBO_BO2_DryFire", from: From::Common, layers: &["wpn_an94_dryfire_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_HitMarker", from: From::Common, layers: &["mpl_hit_alert"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_Heartbeat", from: From::Common, layers: &["mpl_player_heartbeat"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Hurt", from: From::World, layers: &["prj_bullet_impact_large_player"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_Step", from: From::World, layers: &["fly_step_run_grass"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_SprintStep", from: From::World, layers: &["fly_step_sprint_grass", "fly_gear_sprint_plr"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_Land", from: From::World, layers: &["fly_dtp_land_plr_grass"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Impact", from: From::World, layers: &["prj_bullet_impact_small_dirt"], kind: Kind::World, limit: 4 },
    Sfx { event: "ZHBO_BO2_ImpactFlesh", from: From::World, layers: &["prj_bullet_impact_large_player"], kind: Kind::World, limit: 4 },
    // Third-person gunshots: the FX weapons fire these for everyone but the shooter, the RTS rifle
    // for everyone (ZHBO_Weapons.ini's FireSound, rewritten below).
    Sfx { event: "ZHBO_BO2_FireNpc", from: From::Common, layers: &["wpn_an94_fire_npc", "wpn_an94_fire_npc_decay"], kind: Kind::Others, limit: 3 },
    Sfx { event: "ZHBO_BO2_FirePistolNpc", from: From::Common, layers: &["wpn_beretta93r_fire_npc", "wpn_beretta93r_fire_npc_decay"], kind: Kind::Others, limit: 3 },
    Sfx { event: "ZHBO_BO2_FireRts", from: From::Common, layers: &["wpn_an94_fire_npc", "wpn_an94_fire_npc_decay"], kind: Kind::World, limit: 3 },
    // Scorestreaks earned (each streak's own sting), the UAV coming online, equipment planted, and
    // the C4 detonator.
    Sfx { event: "ZHBO_BO2_Streak_RADAR", from: From::Common, layers: &["mpl_killstreak_radar"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Streak_AUTO_TURRET", from: From::Common, layers: &["mpl_killstreak_auto_turret"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Streak_MINIGUN", from: From::Common, layers: &["mpl_killstreak_minigun"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Streak_M32", from: From::Common, layers: &["mpl_killstreak_generic"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_UavOnline", from: From::Common, layers: &["mpl_killstreak_uav"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Plant", from: From::Common, layers: &["fly_claymore_plant_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Detonate", from: From::Common, layers: &["wpn_c4detpack_trigger_plr"], kind: Kind::Player, limit: 1 },
];

/// The mod's weapon INI with BO2 gunshots: (weapon, fire sound event).
const WEAPON_INI: &str = include_str!("../../../data/Data/INI/Weapon/ZHBO_Weapons.ini");
const WEAPON_INI_PATH: &str = "Data\\INI\\Weapon\\ZHBO_Weapons.ini";
const WEAPON_SOUNDS: &[(&str, &str)] =
    &[("ZHBO_MTAR_FX", "ZHBO_BO2_FireNpc"), ("ZHBO_B23R_FX", "ZHBO_BO2_FirePistolNpc"), ("ZHBO_GeneralRifle", "ZHBO_BO2_FireRts")];

/// Replace each weapon's `FireSound` in the weapon INI.
pub fn weapon_ini(base: &str, sounds: &[(&str, &str)]) -> Result<String, String> {
    let mut out = String::new();
    let mut current: Option<&str> = None;
    let mut done = 0;
    for line in base.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("Weapon ") {
            current = sounds.iter().find(|s| s.0 == name.trim()).map(|s| s.1);
        } else if t == "End" {
            current = None;
        }
        if let (Some(event), true) = (current, t.starts_with("FireSound")) {
            out.push_str(&format!("  FireSound               = {event}\n"));
            done += 1;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if done != sounds.len() {
        return Err(format!("rewrote {done} of {} weapon fire sounds", sounds.len()));
    }
    Ok(out)
}

/// The map whose bank supplies footsteps and impacts.
pub const WORLD_ZONE: &str = "mp_raid";
/// BO2's player gunshot is 0.45; scale BO2 volumes so it plays at Zero Hour's 100.
const VOLUME_GAIN: f32 = 100.0 / 0.45;
const NOTETRACK_PREFIX: &str = "sndnt#";

/// 16-bit PCM.
#[derive(Clone)]
struct Pcm {
    samples: Vec<i16>,
    channels: u16,
    rate: u32,
}

fn pcm_from(format: u8, data: &[u8], channels: u8, rate_index: u8) -> Option<Pcm> {
    match format {
        FORMAT_PCM16 => Some(Pcm {
            samples: data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
            channels: channels.max(1) as u16,
            rate: *FRAME_RATES.get(rate_index as usize)?,
        }),
        FORMAT_FLAC => {
            let mut reader = claxon::FlacReader::new(std::io::Cursor::new(data)).ok()?;
            let info = reader.streaminfo();
            let shift = info.bits_per_sample as i32 - 16;
            let samples = reader
                .samples()
                .map_while(Result::ok)
                .map(|s| (if shift >= 0 { s >> shift } else { s << -shift }).clamp(i16::MIN as i32, i16::MAX as i32) as i16)
                .collect();
            Some(Pcm { samples, channels: info.channels as u16, rate: info.sample_rate })
        }
        _ => None,
    }
}

/// Every `.sabl`/`.sabs` bank in the install, indexed by entry id. Aliases in one zone point at
/// sounds in any bank (most player sounds are in the shared `cmn_root.all.sabl`).
struct Library {
    banks: Vec<SoundBank>,
    index: std::collections::HashMap<u32, (usize, usize)>,
}

impl Library {
    fn open(bo2_dir: &Path) -> Result<Library, String> {
        let dir = bo2_dir.join("sound");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "sabl" || x == "sabs"))
            .collect();
        // Loaded banks first: they hold the short effects.
        paths.sort_by_key(|p| (p.extension().is_some_and(|x| x == "sabs"), p.clone()));
        let mut lib = Library { banks: Vec::new(), index: Default::default() };
        for p in paths {
            let Ok(bank) = SoundBank::open(&p) else { continue };
            let b = lib.banks.len();
            for (i, e) in bank.entries().iter().enumerate() {
                lib.index.entry(e.id).or_insert((b, i));
            }
            lib.banks.push(bank);
        }
        Ok(lib)
    }

    fn decode(&self, asset_id: u32) -> Option<Pcm> {
        let &(b, i) = self.index.get(&asset_id)?;
        let bank = &self.banks[b];
        let e = &bank.entries()[i];
        let data = bank.read(e).ok()?;
        pcm_from(e.format, &data, e.channels, e.frame_rate_index)
    }
}

/// A zone's sound aliases, decoded through the library.
struct Source<'a> {
    bank: Bank,
    lib: &'a Library,
}

impl Source<'_> {
    fn decode(&self, asset_id: u32) -> Option<Pcm> {
        self.lib.decode(asset_id)
    }
}

/// Resample (linear) and remix to `rate` Hz, `channels` channels.
fn convert(p: &Pcm, rate: u32, channels: u16) -> Vec<i16> {
    let src_ch = p.channels.max(1) as usize;
    let frames = p.samples.len() / src_ch;
    let out_frames = (frames as u64 * rate as u64 / p.rate.max(1) as u64) as usize;
    let mut out = Vec::with_capacity(out_frames * channels as usize);
    for i in 0..out_frames {
        let pos = i as f64 * p.rate as f64 / rate as f64;
        let i0 = (pos as usize).min(frames.saturating_sub(1));
        let i1 = (i0 + 1).min(frames.saturating_sub(1));
        let t = pos - i0 as f64;
        for c in 0..channels as usize {
            let sc = c.min(src_ch - 1);
            let a = p.samples[i0 * src_ch + sc] as f64;
            let b = p.samples[i1 * src_ch + sc] as f64;
            out.push((a + (b - a) * t) as i16);
        }
    }
    out
}

/// Mix layers (each scaled by its BO2 volume relative to the loudest).
fn mix(layers: &[(Pcm, f32)]) -> Pcm {
    let rate = layers.iter().map(|l| l.0.rate).max().unwrap_or(48000);
    let channels = layers.iter().map(|l| l.0.channels).max().unwrap_or(1);
    let loudest = layers.iter().map(|l| l.1).fold(0.0f32, f32::max).max(1e-3);
    let mut acc: Vec<f32> = Vec::new();
    for (p, vol) in layers {
        let s = convert(p, rate, channels);
        if s.len() > acc.len() {
            acc.resize(s.len(), 0.0);
        }
        for (a, v) in acc.iter_mut().zip(s) {
            *a += v as f32 * (vol / loudest);
        }
    }
    Pcm { samples: acc.into_iter().map(|v| v.clamp(-32768.0, 32767.0) as i16).collect(), channels, rate }
}

fn pcm_wav(p: &Pcm) -> Vec<u8> {
    let bytes: Vec<u8> = p.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    wav_bytes(&bytes, p.channels, p.rate)
}

fn audio_event(name: &str, sounds: &[String], volume: f32, kind: Kind, limit: u8) -> String {
    let volume = (volume * VOLUME_GAIN).round().clamp(5.0, 100.0) as u32;
    let kind = match kind {
        Kind::Player => kind.ini().to_string(),
        _ => format!("{}\n  MinRange    = 40\n  MaxRange    = 600", kind.ini()),
    };
    format!(
        "AudioEvent {name}\n  Sounds      = {}\n  Control     = interrupt random\n  Priority    = high\n  \
         Volume      = {volume}\n  Limit       = {limit}\n  Type        = {kind}\nEnd\n\n",
        sounds.join(" ")
    )
}

fn sound_file(event: &str, variant: usize) -> String {
    format!("{}_{variant}", event.to_ascii_lowercase())
}

/// Convert the events and `extra` (per-weapon) events; `viewmodel_clips` are the viewmodel
/// animations by (viewmodel, clip), whose sound notetracks become `ZHBO_NT_*` events. Returns the
/// archive files and a summary.
pub fn files(
    common: &Zone,
    world: &Zone,
    bo2_dir: &Path,
    viewmodel_clips: &[(i32, i32, xanim::Anim)],
    extra: &[DynSfx],
) -> Result<(Vec<(String, Vec<u8>)>, String), String> {
    let lib = Library::open(bo2_dir)?;
    let pick = |zone: &Zone, prefix: &str| -> Option<Source> {
        let bank = sndalias::banks(zone).into_iter().find(|b| b.name.starts_with(prefix))?;
        Some(Source { bank, lib: &lib })
    };
    let common_src = pick(common, "mpl_common").ok_or("no mpl_common sound bank in common_mp")?;
    let world_src = pick(world, "mpl_").ok_or_else(|| format!("no sound bank in {WORLD_ZONE}"))?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut ini = String::from("; Generated by Conquer Ops from the player's own Black Ops II install. Do not redistribute.\n\n");
    let mut missing: Vec<String> = Vec::new();

    let mut add = |event: &str, src: &Source, layers: &[&str], kind: Kind, limit: u8, out: &mut BTreeMap<String, Vec<u8>>| -> bool {
        let mut aliases: Vec<_> = layers.iter().filter_map(|n| src.bank.alias(n)).collect();
        aliases.dedup_by(|a, b| a.name == b.name);
        if aliases.is_empty() {
            return false;
        }
        // One output variant per variant of the first layer; other layers use their first variant.
        let base = aliases[0];
        let mut sounds = Vec::new();
        for (i, v) in base.variants.iter().enumerate().take(8) {
            let mut parts = Vec::new();
            for (k, a) in aliases.iter().enumerate() {
                let var = if k == 0 { v } else { &a.variants[0] };
                if let Some(p) = src.decode(var.asset_id) {
                    parts.push((p, var.volume.1));
                }
            }
            if parts.is_empty() {
                continue;
            }
            let name = sound_file(event, i);
            out.insert(format!("Data\\Audio\\Sounds\\{name}.wav"), pcm_wav(&mix(&parts)));
            sounds.push(name);
        }
        if sounds.is_empty() {
            return false;
        }
        let volume = base.variants.iter().map(|v| v.volume.1).fold(0.0, f32::max);
        ini.push_str(&audio_event(event, &sounds, volume, kind, limit));
        true
    };

    for s in SFX {
        let src = if s.from == From::Common { &common_src } else { &world_src };
        if !add(s.event, src, s.layers, s.kind, s.limit, &mut out) {
            missing.push(s.event.to_string());
        }
    }

    let mut extra_made = 0;
    for e in extra {
        let layers: Vec<&str> = e.layers.iter().map(String::as_str).collect();
        if add(&e.event, &common_src, &layers, e.kind, e.limit, &mut out) {
            extra_made += 1;
        } else {
            missing.push(format!("weapon sound {}", e.event));
        }
    }

    // Viewmodel notetracks: "sndnt#<alias>" notifies, at their time into each clip.
    let mut notes = String::from("; viewmodel clip seconds event (zhbo_core::viewmodel: weapon viewmodel, BO2 slot or tablet clip)\n");
    let mut nt_events: BTreeMap<String, bool> = BTreeMap::new();
    for (vm, clip, anim) in viewmodel_clips {
        let length = anim.num_frames as f32 / anim.frame_rate.max(1.0);
        for n in &anim.notifies {
            let Some(alias) = n.name.strip_prefix(NOTETRACK_PREFIX) else { continue };
            let event = format!("ZHBO_NT_{alias}");
            // Most foley is in the common bank; some (gear rustle) only in the map's.
            let ok = *nt_events
                .entry(event.clone())
                .or_insert_with(|| add(&event, &common_src, &[alias], Kind::Player, 2, &mut out) || add(&event, &world_src, &[alias], Kind::Player, 2, &mut out));
            if ok {
                notes.push_str(&format!("{vm} {clip} {:.3} {event}\n", n.time * length));
            } else if !missing.iter().any(|m| m.ends_with(alias)) {
                // Not in any multiplayer bank: BO2 plays nothing for it either.
                missing.push(format!("notetrack {alias}"));
            }
        }
    }
    out.insert("Data\\INI\\SoundEffects\\ZHBO_BO2.ini".into(), ini.into_bytes());
    out.insert(WEAPON_INI_PATH.into(), weapon_ini(WEAPON_INI, WEAPON_SOUNDS)?.into_bytes());
    out.insert("Data\\ConquerOps\\viewmodel_sounds.txt".into(), notes.into_bytes());
    // Zero Hour allows only 4 simultaneous 2D sounds; automatic fire plus footsteps and reload
    // foley need more. The engine loads this folder after AudioSettings.ini.
    out.insert(
        "Data\\INI\\AudioSettings\\ZHBO_BO2.ini".into(),
        b"AudioSettings\n  SampleCount2D = 16\nEnd\n".to_vec(),
    );
    let wavs = out.keys().filter(|k| k.ends_with(".wav")).count();
    let summary = format!(
        "{wavs} sounds, {} events, {} notetrack events{}",
        SFX.len() + extra_made - missing.iter().filter(|m| SFX.iter().any(|s| s.event == m.as_str())).count(),
        nt_events.values().filter(|v| **v).count(),
        if missing.is_empty() { String::new() } else { format!(", missing {missing:?}") }
    );
    Ok((out.into_iter().collect(), summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_and_resamples() {
        let a = Pcm { samples: vec![1000; 48], channels: 1, rate: 24000 };
        let b = Pcm { samples: vec![500; 96], channels: 2, rate: 48000 };
        let m = mix(&[(a, 1.0), (b, 0.5)]);
        assert_eq!((m.channels, m.rate), (2, 48000));
        // 48 mono frames at 24 kHz become 96 stereo frames (192 samples) at 48 kHz; the second
        // layer (48 frames at half volume) covers the first half.
        assert_eq!(m.samples.len(), 192);
        assert!(m.samples[..96].iter().all(|&s| (1240..=1260).contains(&s)), "{:?}", &m.samples[..4]);
        assert!(m.samples[96..].iter().all(|&s| (990..=1010).contains(&s)));
    }

    #[test]
    fn events_are_well_formed() {
        let e = audio_event("ZHBO_BO2_Fire", &["zhbo_bo2_fire_0".into()], 0.45, Kind::Player, 3);
        assert!(e.contains("Volume      = 100") && e.contains("Type        = ui player") && e.ends_with("End\n\n"));
        let w = audio_event("ZHBO_BO2_Impact", &["x".into()], 0.2, Kind::World, 4);
        assert!(w.contains("world shrouded everyone") && w.contains("MinRange"));
        let o = audio_event("ZHBO_BO2_FireNpc", &["x".into()], 0.2, Kind::Others, 4);
        assert!(o.contains("allies enemies") && !o.contains("player"));
    }

    #[test]
    fn rewrites_weapon_fire_sounds() {
        let ini = weapon_ini(WEAPON_INI, WEAPON_SOUNDS).unwrap();
        assert!(ini.contains("FireSound               = ZHBO_BO2_FireNpc"));
        assert!(ini.contains("FireSound               = ZHBO_BO2_FireRts"));
        assert!(!ini.contains("RangerWeapon"), "no Zero Hour rifle sound left");
        assert!(ini.contains("HeroUSAKnifeAttack"), "weapons not listed keep their sound");
    }
}
