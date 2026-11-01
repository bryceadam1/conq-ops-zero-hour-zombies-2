//! Black Ops II sound effects for first person, converted from the player's own BO2 install into
//! `ConquerOps_BO2.big`: WAVs under `Data\Audio\Sounds`, Zero Hour AudioEvents in
//! `Data\INI\SoundEffects\ZHBO_BO2.ini` (the engine loads that folder after `SoundEffects.ini`), and
//! the viewmodel's sound notetracks in `Data\ConquerOps\viewmodel_sounds.txt`.
//!
//! Sounds are found by BO2 alias name (`zhbo_formats::sndalias`); each alias variant becomes one WAV,
//! and a multi-layer event (a gunshot is shot + tail + low end) is mixed into one WAV per variant.
//! zhbo-core plays the events by name (`zhbo_core::sounds`); `ZhboViewmodel.cpp` plays notetracks.

use crate::mixer::{Layer, Mixer};
use crate::sounds::wav_bytes;
use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::sab::{SoundBank, FORMAT_FLAC, FORMAT_PCM16, FRAME_RATES};
use zhbo_formats::sndalias::{self, Alias, Bank};
use zhbo_formats::t6zone::Zone;
use zhbo_formats::xanim;

/// Where an event's aliases are: the weapon/player bank, a map bank (footsteps, impacts), or the
/// menu bank (`code_post_gfx_mp`'s `mpl_code_post_gfx`, where BO2 keeps the CaC cues).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    Common,
    World,
}

pub struct Sfx {
    /// AudioEvent name (zhbo_core::sounds uses the same names).
    pub event: &'static str,
    pub from: From,
    /// Aliases mixed into each variant, each with its `secondaryName` chain ([`secondary_chain`]).
    pub layers: &'static [&'static str],
    pub kind: Kind,
    pub limit: u8,
}

/// An event made at conversion time (a converted weapon's own sounds), from the common bank.
#[derive(Clone, Debug)]
pub struct DynSfx {
    pub event: String,
    /// Aliases mixed into each variant, each with its `secondaryName` chain; ones the bank lacks
    /// are skipped.
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
    Sfx { event: "ZHBO_BO2_Fire", from: From::Common, layers: &["wpn_an94_fire_plr"], kind: Kind::Player, limit: 3 },
    Sfx { event: "ZHBO_BO2_FirePistol", from: From::Common, layers: &["wpn_beretta93r_fire_plr"], kind: Kind::Player, limit: 3 },
    Sfx { event: "ZHBO_BO2_DryFire", from: From::Common, layers: &["wpn_an94_dryfire_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_DryFireGeneric", from: From::Common, layers: &["wpn_generic_dryfire_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_DryFireGlRifle", from: From::Common, layers: &["wpn_gl_rifle_dryfire_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_DryFireCrossbow", from: From::Common, layers: &["wpn_crossbow_dryfire_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_DryFireRpg", from: From::Common, layers: &["wpn_rpg_dryfire_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_DryFireBeretta", from: From::Common, layers: &["wpn_beretta93r_dryfire_plr"], kind: Kind::Player, limit: 1 },
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
    Sfx { event: "ZHBO_BO2_FireNpc", from: From::Common, layers: &["wpn_an94_fire_npc"], kind: Kind::Others, limit: 3 },
    Sfx { event: "ZHBO_BO2_FirePistolNpc", from: From::Common, layers: &["wpn_beretta93r_fire_npc"], kind: Kind::Others, limit: 3 },
    Sfx { event: "ZHBO_BO2_FireRts", from: From::Common, layers: &["wpn_an94_fire_npc"], kind: Kind::World, limit: 3 },
    // Equipment planted, and the C4 detonator.
    Sfx { event: "ZHBO_BO2_Plant", from: From::Common, layers: &["fly_claymore_plant_plr"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Detonate", from: From::Common, layers: &["wpn_c4detpack_trigger_plr"], kind: Kind::Player, limit: 1 },
];

/// The mod's weapon INI with BO2 gunshots: (weapon, fire sound event).
const WEAPON_INI: &str = include_str!("../../../data/Data/INI/Weapon/ZHBO_Weapons.ini");
pub const WEAPON_INI_PATH: &str = "Data\\INI\\Weapon\\ZHBO_Weapons.ini";
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
/// The zone whose `mpl_code_post_gfx` bank holds the create-a-class/menu aliases.
/// The zones whose banks hold BO2's menu aliases (`zhbo_core::sounds::UiCue`), searched in order:
/// the shell's own bank first, then the patches and the common bank for the few that live there.
pub const UI_ZONES: &[(&str, &str)] =
    &[("code_post_gfx_mp", "mpl_code_post_gfx"), ("patch_ui_mp", "mpl_patch_ui"), ("patch_mp", "mpl_patch")];
/// The zone whose `SndDriverGlobals` holds BO2's mixer tables (groups, curves, pans, snapshots).
pub const MIXER_ZONE: &str = "code_post_gfx_mp";
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

/// A zone's sound aliases, decoded through the library. A source holds every bank whose name
/// starts with its prefix, in the zone's order: the first bank's aliases win, later ones supply
/// the aliases the first lacks (common_mp splits its player sounds across `mpl_common*` banks).
struct Source<'a> {
    banks: Vec<Bank>,
    lib: &'a Library,
}

impl Source<'_> {
    fn alias(&self, name: &str) -> Option<&Alias> {
        self.banks.iter().find_map(|b| b.alias(name))
    }

    /// `layers` with every alias BO2 starts alongside each one: its `secondaryName` chain.
    fn chained(&self, layers: &[&str]) -> Vec<&Alias> {
        secondary_chain(layers, |n| self.alias(n))
    }

    fn decode(&self, asset_id: u32) -> Option<Pcm> {
        self.lib.decode(asset_id)
    }
}

/// The aliases BO2 plays for `layers`: each one, then its `secondaryName`, that one's secondary, …
/// (`wpn_sig556_fire_plr` -> `wpn_sig556_LFE` -> `wpn_sig556_fire_plr_decay`: the shot, its low
/// end, its tail). Missing aliases end their chain; each alias once (a cycle ends too), first-seen
/// order.
pub fn secondary_chain<'a>(layers: &[&str], alias: impl Fn(&str) -> Option<&'a Alias>) -> Vec<&'a Alias> {
    let mut out: Vec<&Alias> = Vec::new();
    for name in layers {
        let mut next = alias(name);
        while let Some(a) = next {
            if out.iter().any(|o| o.name == a.name) {
                break;
            }
            out.push(a);
            next = a.variants.first().and_then(|v| v.secondary.as_deref()).and_then(&alias);
        }
    }
    out
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

/// Interleaved −1..1 samples at `rate` Hz, `channels` channels.
fn to_float(p: &Pcm, rate: u32, channels: u16) -> Vec<f32> {
    convert(p, rate, channels).into_iter().map(|s| s as f32 / 32768.0).collect()
}

/// One variant of an event as BO2's mixer outputs it ([`crate::mixer`]): every layer at its own
/// gain (volume × group × bus × pan fold, and for a 3D alias its distance curve at the source,
/// `d = 0`), from its own start delay, through the buses and master. Layers the mix silences (an
/// LFE-only pan in stereo, the far layer of a crossfade up close) are not decoded. `None` when
/// nothing is audible.
fn render(mixer: &Mixer, layers: &[(&sndalias::Variant, Pcm)]) -> Option<(Vec<f32>, u16, u32)> {
    let rate = layers.iter().map(|l| l.1.rate).max()?;
    let channels = layers.iter().map(|l| l.1.channels).max()?;
    let floats: Vec<Vec<f32>> = layers.iter().map(|(_, p)| to_float(p, rate, channels)).collect();
    let mix: Vec<Layer> = layers
        .iter()
        .zip(&floats)
        .map(|((v, _), samples)| Layer {
            samples,
            start_frame: (v.start_delay_ms as u64 * rate as u64 / 1000) as usize,
            bus: mixer.bus(v),
            gain: mixer.gain(v, 0.0),
        })
        .collect();
    let out = mixer.render(&mix, channels as usize);
    out.iter().any(|s| *s != 0.0).then_some((out, channels, rate))
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |a, s| a.max(s.abs()))
}

/// 16-bit samples of `samples` scaled so `peak` is full scale.
fn quantize(samples: &[f32], peak: f32) -> Vec<i16> {
    samples.iter().map(|s| (s / peak * 32767.0).round().clamp(-32768.0, 32767.0) as i16).collect()
}

fn pcm_wav(samples: &[i16], scale: f32, channels: u16, rate: u32) -> Vec<u8> {
    let bytes: Vec<u8> = samples.iter().flat_map(|&s| ((s as f32 * scale).round() as i16).to_le_bytes()).collect();
    wav_bytes(&bytes, channels, rate)
}

/// Zero Hour falloff for a 3D chain, in Zero Hour units: `MaxRange` where BO2's chain is last
/// heard (the farthest layer's `distMax`), `MinRange` half the distance at which the chain's summed
/// curves fall to half their level at the source — Zero Hour's OpenAL falloff is `MinRange / d`
/// (inverse distance, clamped), so the two meet at −6 dB. `None` for a chain with no 3D layer.
fn falloff(mixer: &Mixer, chain: &[&sndalias::Variant]) -> Option<(f32, f32)> {
    let max = mixer.audible_range(chain);
    if max <= 0.0 {
        return None;
    }
    const STEPS: usize = 400;
    let at = |i: usize| max * i as f32 / STEPS as f32;
    let source = mixer.chain_gain(chain, 0.0);
    let reference = if source > 0.0 { source } else { (0..=STEPS).map(|i| mixer.chain_gain(chain, at(i))).fold(0.0, f32::max) };
    let half = (0..=STEPS).map(at).find(|&d| mixer.chain_gain(chain, d) < 0.5 * reference).unwrap_or(max);
    let units = zhbo_core::units::inches;
    Some((units(half * 0.5).max(1.0), units(max)))
}

fn audio_event(name: &str, sounds: &[String], volume: u32, kind: Kind, limit: u8, range: Option<(f32, f32)>) -> String {
    let kind = match (kind, range) {
        (Kind::Player, _) => kind.ini().to_string(),
        (_, Some((min, max))) => format!("{}\n  MinRange    = {min:.0}\n  MaxRange    = {max:.0}", kind.ini()),
        (_, None) => format!("{}\n  MinRange    = 40\n  MaxRange    = 600", kind.ini()),
    };
    format!(
        "AudioEvent {name}\n  Sounds      = {}\n  Control     = interrupt random\n  Priority    = high\n  \
         Volume      = {volume}\n  Limit       = {limit}\n  Type        = {kind}\nEnd\n\n",
        sounds.join(" ")
    )
}

/// An event rendered through BO2's mixer, waiting for the one normalization every event shares.
struct Pending {
    event: String,
    kind: Kind,
    limit: u8,
    range: Option<(f32, f32)>,
    /// (sound file, mixer output normalized to its own peak, that peak, channels, rate).
    variants: Vec<(String, Vec<i16>, f32, u16, u32)>,
}

impl Pending {
    fn level(&self) -> f32 {
        self.variants.iter().map(|v| v.2).fold(0.0, f32::max)
    }
}

/// Zero Hour's `Volume` for an event at `level` when the loudest event is at `loudest`: the
/// percentage rounded up (so the WAV only ever scales down) and at least 1.
fn volume_percent(level: f32, loudest: f32) -> u32 {
    ((100.0 * level / loudest).ceil() as u32).clamp(1, 100)
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
        let mut banks = sndalias::banks(zone).ok()?;
        banks.retain(|b| b.name.starts_with(prefix));
        (!banks.is_empty()).then_some(Source { banks, lib: &lib })
    };
    let common_src = pick(common, "mpl_common").ok_or("no mpl_common sound bank in common_mp")?;
    let world_src = pick(world, "mpl_").ok_or_else(|| format!("no sound bank in {WORLD_ZONE}"))?;
    // The menu banks are optional: without one its UI events stay on the missing list and the
    // cursor is silent rather than the whole archive failing.
    let ui_zones: Vec<(Zone, &str)> =
        UI_ZONES.iter().filter_map(|(z, bank)| Some((crate::generals::load_zone(bo2_dir, z).ok()?, *bank))).collect();
    let ui_banks: Vec<Source> = ui_zones.iter().filter_map(|(zone, bank)| pick(zone, bank)).collect();
    let ui_srcs: Vec<&Source> = ui_banks.iter().chain(std::iter::once(&common_src)).collect();
    let mixer_zone = ui_zones.iter().map(|(z, _)| z).find(|z| z.name.contains(MIXER_ZONE));
    let loaded;
    let mixer_zone = match mixer_zone {
        Some(z) => z,
        None => {
            loaded = crate::generals::load_zone(bo2_dir, MIXER_ZONE)?;
            &loaded
        }
    };
    let globals = zhbo_formats::sndglobals::read(mixer_zone)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no SndDriverGlobals in {MIXER_ZONE}"))?;
    let mixer = Mixer::new(&globals).ok_or("SndDriverGlobals has no master snapshot")?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut pending: Vec<Pending> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    let mut add = |event: &str, src: &Source, layers: &[&str], kind: Kind, limit: u8| -> bool {
        let aliases = src.chained(layers);
        if aliases.is_empty() {
            return false;
        }
        // One output variant per variant of the first layer; other layers use their first variant.
        let base = aliases[0];
        let mut variants = Vec::new();
        for (i, v) in base.variants.iter().enumerate().take(8) {
            let parts: Vec<(&sndalias::Variant, Pcm)> = aliases
                .iter()
                .enumerate()
                .map(|(k, a)| if k == 0 { v } else { &a.variants[0] })
                .filter(|var| mixer.gain(var, 0.0) > 0.0)
                .filter_map(|var| Some((var, src.decode(var.asset_id)?)))
                .collect();
            let Some((samples, channels, rate)) = render(&mixer, &parts) else { continue };
            let level = peak(&samples);
            variants.push((sound_file(event, i), quantize(&samples, level), level, channels, rate));
        }
        if variants.is_empty() {
            return false;
        }
        let chain: Vec<&sndalias::Variant> = aliases.iter().filter_map(|a| a.variants.first()).collect();
        let range = if kind == Kind::Player { None } else { falloff(&mixer, &chain) };
        pending.push(Pending { event: event.to_string(), kind, limit, range, variants });
        true
    };

    for s in SFX {
        let src = match s.from {
            From::Common => &common_src,
            From::World => &world_src,
        };
        // Some aliases BO2 keeps only in the map's bank (the generic dry fire): fall back to it
        // rather than dropping the event. Common first, so aliases that already resolve do not change.
        let ok = add(s.event, src, s.layers, s.kind, s.limit)
            || (s.from == From::Common && add(s.event, &world_src, s.layers, s.kind, s.limit));
        if !ok {
            missing.push(s.event.to_string());
        }
    }

    let mut ui_made = 0;
    for cue in zhbo_core::sounds::UiCue::ALL {
        if ui_srcs.iter().any(|src| add(cue.event(), src, &[cue.alias()], Kind::Player, 2)) {
            ui_made += 1;
        } else {
            missing.push(format!("menu sound {}", cue.alias()));
        }
    }

    let mut extra_made = 0;
    for e in extra {
        let layers: Vec<&str> = e.layers.iter().map(String::as_str).collect();
        // As the fixed events: some aliases live only in the map's bank (gear pull-outs).
        if add(&e.event, &common_src, &layers, e.kind, e.limit) || add(&e.event, &world_src, &layers, e.kind, e.limit) {
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
            let event = zhbo_core::names::notetrack_event(alias);
            // Most foley is in the common bank; some (gear rustle) only in the map's.
            let ok = *nt_events
                .entry(event.clone())
                .or_insert_with(|| add(&event, &common_src, &[alias], Kind::Player, 2) || add(&event, &world_src, &[alias], Kind::Player, 2));
            if ok {
                notes.push_str(&format!("{vm} {clip} {:.3} {event}\n", n.time * length));
            } else if !missing.iter().any(|m| m.ends_with(alias)) {
                // Not in any multiplayer bank: BO2 plays nothing for it either.
                missing.push(format!("notetrack {alias}"));
            }
        }
    }
    // One normalization for every event: the loudest converted sound is Zero Hour's 100, every
    // other keeps its BO2 level relative to it. The rounded-up percentage carries the level and the
    // WAV the remainder (a scale <= 1).
    let loudest = pending.iter().map(Pending::level).fold(0.0f32, f32::max).max(f32::MIN_POSITIVE);
    let mut ini = String::from("; Generated by Conquer Ops from the player's own Black Ops II install. Do not redistribute.\n\n");
    for p in &pending {
        let volume = volume_percent(p.level(), loudest);
        for (name, samples, level, channels, rate) in &p.variants {
            let scale = level / loudest / (volume as f32 / 100.0);
            out.insert(format!("Data\\Audio\\Sounds\\{name}.wav"), pcm_wav(samples, scale, *channels, *rate));
        }
        let sounds: Vec<String> = p.variants.iter().map(|v| v.0.clone()).collect();
        ini.push_str(&audio_event(&p.event, &sounds, volume, p.kind, p.limit, p.range));
    }
    let loudest_event = pending.iter().max_by(|a, b| a.level().total_cmp(&b.level())).map_or("", |p| p.event.as_str());
    out.insert("Data\\INI\\SoundEffects\\ZHBO_BO2.ini".into(), ini.into_bytes());
    out.insert(WEAPON_INI_PATH.into(), weapon_ini(WEAPON_INI, WEAPON_SOUNDS)?.into_bytes());
    out.insert(zhbo_core::names::VIEWMODEL_SOUNDS_PATH.into(), notes.into_bytes());
    // Zero Hour allows only 4 simultaneous 2D sounds; automatic fire plus footsteps and reload
    // foley need more. The engine loads this folder after AudioSettings.ini.
    out.insert(
        "Data\\INI\\AudioSettings\\ZHBO_BO2.ini".into(),
        b"AudioSettings\n  SampleCount2D = 16\nEnd\n".to_vec(),
    );
    let wavs = out.keys().filter(|k| k.ends_with(".wav")).count();
    let summary = format!(
        "{wavs} sounds, {} events, {ui_made} menu events, {} notetrack events, loudest {loudest_event}{}",
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
    fn resamples_and_remixes() {
        let a = Pcm { samples: vec![1000; 48], channels: 1, rate: 24000 };
        // 48 mono frames at 24 kHz become 96 stereo frames (192 samples) at 48 kHz.
        let f = to_float(&a, 48000, 2);
        assert_eq!(f.len(), 192);
        assert!(f.iter().all(|&s| (s - 1000.0 / 32768.0).abs() < 1e-4));
    }

    /// The loudest event is 100; others keep their ratio, rounded up so the WAV only scales down.
    #[test]
    fn volumes_share_one_normalization() {
        assert_eq!(volume_percent(2.0, 2.0), 100);
        assert_eq!(volume_percent(1.0, 2.0), 50);
        assert_eq!(volume_percent(0.501, 2.0), 26);
        assert_eq!(volume_percent(1e-6, 2.0), 1);
        let samples = quantize(&[0.5, -0.25], 0.5);
        assert_eq!(samples, [32767, -16384]);
    }

    /// BO2 starts an alias's `secondaryName` with it, and that one's secondary, …: the burst
    /// SIG556's per-shot alias is its low end, whose secondary is the tail; the full shot chains
    /// to the low end. Each alias once (a cycle ends the chain); a missing one ends its chain.
    #[test]
    fn layers_follow_the_secondary_chain() {
        let alias = |name: &str, secondary: Option<&str>| Alias {
            name: name.into(),
            variants: vec![sndalias::Variant {
                asset_id: 1,
                file: String::new(),
                volume: (1.0, 1.0),
                pitch: (1.0, 1.0),
                distance: (0.0, 0.0),
                start_delay_ms: 0,
                secondary: secondary.map(String::from),
                mix: Default::default(),
            }],
        };
        let bank = [
            alias("wpn_sig556_fire_plr", Some("wpn_sig556_LFE")),
            alias("wpn_sig556_LFE", Some("wpn_sig556_fire_plr_decay")),
            alias("wpn_sig556_fire_plr_decay", None),
            alias("loop_a", Some("loop_b")),
            alias("loop_b", Some("loop_a")),
            alias("dangling", Some("not_in_any_bank")),
        ];
        let find = |n: &str| bank.iter().find(|a| a.name.eq_ignore_ascii_case(n));
        let names = |layers: &[&str]| secondary_chain(layers, find).iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&["wpn_sig556_fire_plr"]), ["wpn_sig556_fire_plr", "wpn_sig556_LFE", "wpn_sig556_fire_plr_decay"]);
        assert_eq!(names(&["wpn_sig556_LFE"]), ["wpn_sig556_LFE", "wpn_sig556_fire_plr_decay"], "the burst gun's shot: low end and tail");
        assert_eq!(names(&["wpn_sig556_fire_plr", "wpn_sig556_fire_plr_decay"]), ["wpn_sig556_fire_plr", "wpn_sig556_LFE", "wpn_sig556_fire_plr_decay"]);
        assert_eq!(names(&["loop_a"]), ["loop_a", "loop_b"]);
        assert_eq!(names(&["dangling", "missing"]), ["dangling"]);
    }

    /// Needs the player's BO2 install. The AN-94's third-person chain layer by layer as BO2's
    /// mixer has it: up close the close shot and its tail, not the low end (LFE-only pan) nor the
    /// distant layer (its `sindelay` min curve is 0 at the source); past 1,800" only the distant
    /// layer, out to 12,000". The converted events carry those levels and ranges.
    #[test]
    #[ignore]
    fn the_installs_chains_follow_bo2s_mixer() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::generals::load_zone(&dir, "common_mp").unwrap();
        let world = crate::generals::load_zone(&dir, WORLD_ZONE).unwrap();
        let gfx = crate::generals::load_zone(&dir, MIXER_ZONE).unwrap();
        let globals = zhbo_formats::sndglobals::read(&gfx).unwrap().unwrap();
        let mixer = Mixer::new(&globals).unwrap();
        let banks = sndalias::banks(&common).unwrap();
        let find = |n: &str| banks.iter().find_map(|b| b.alias(n));
        let chain = secondary_chain(&["wpn_an94_fire_npc"], find);
        let names: Vec<&str> = chain.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["wpn_an94_fire_npc", "wpn_an94_LFE_npc", "wpn_an94_fire_npc_dist", "wpn_an94_fire_npc_decay"]);
        let g = |i: usize, d: f32| mixer.gain(&chain[i].variants[0], d);
        assert!(g(0, 0.0) > 0.0 && g(3, 0.0) > 0.0, "close shot and tail at the source");
        assert_eq!(g(1, 0.0), 0.0, "the low end is LFE-only: not in stereo");
        assert_eq!(g(2, 0.0), 0.0, "the distant layer is silent at the source");
        assert!(g(2, 3000.0) > 0.0 && g(0, 3000.0) == 0.0, "past 1800 only the distant layer");
        assert_eq!(g(2, 12000.0), 0.0);
        // The player's shot outweighs its own tail; a footstep's foley is far below the shot
        // before the compressor, which makes quiet fx up.
        let shot = mixer.gain(&find("wpn_an94_fire_plr").unwrap().variants[0], 0.0);
        let tail = mixer.gain(&find("wpn_an94_fire_plr_decay").unwrap().variants[0], 0.0);
        assert!(shot > 2.0 * tail, "{shot} {tail}");

        let (files, summary) = files(&common, &world, &dir, &[], &[]).unwrap();
        println!("{summary}");
        let ini = files.iter().find(|f| f.0 == "Data\\INI\\SoundEffects\\ZHBO_BO2.ini").map(|f| String::from_utf8_lossy(&f.1).into_owned()).unwrap();
        if let Ok(out) = std::env::var("ZHBO_SFX_DUMP") {
            std::fs::write(out, &ini).unwrap();
        }
        let event = |name: &str| {
            let at = ini.find(&format!("AudioEvent {name}\n")).unwrap_or_else(|| panic!("no {name}"));
            ini[at..].split("End\n").next().unwrap().to_string()
        };
        let field = |name: &str, f: &str| -> f32 {
            event(name).lines().find_map(|l| l.trim().strip_prefix(f).map(|v| v.trim_start_matches([' ', '=']).trim().parse().unwrap())).unwrap_or_else(|| panic!("{name} {f}"))
        };
        let (min, max) = (field("ZHBO_BO2_FireNpc", "MinRange"), field("ZHBO_BO2_FireNpc", "MaxRange"));
        assert_eq!(max, zhbo_core::units::inches(12000.0).round(), "heard as far as the distant layer");
        assert!(min > 0.0 && min < max / 4.0, "{min} {max}");
        let vol = |name: &str| field(name, "Volume");
        assert!(vol("ZHBO_BO2_Fire") > vol("ZHBO_BO2_Step"), "the shot over a footstep");
    }

    #[test]
    fn events_are_well_formed() {
        let e = audio_event("ZHBO_BO2_Fire", &["zhbo_bo2_fire_0".into()], 100, Kind::Player, 3, None);
        assert!(e.contains("Volume      = 100") && e.contains("Type        = ui player") && e.ends_with("End\n\n"));
        let w = audio_event("ZHBO_BO2_Impact", &["x".into()], 40, Kind::World, 4, Some((120.0, 900.0)));
        assert!(w.contains("world shrouded everyone") && w.contains("MinRange    = 120") && w.contains("MaxRange    = 900"));
        let o = audio_event("ZHBO_BO2_FireNpc", &["x".into()], 40, Kind::Others, 4, None);
        assert!(o.contains("allies enemies") && !o.contains("player"));
    }

    #[test]
    fn menu_cues_are_distinct_bo2_aliases() {
        use zhbo_core::sounds::UiCue;
        let mut aliases: Vec<&str> = UiCue::ALL.iter().map(|c| c.alias()).collect();
        aliases.sort_unstable();
        aliases.dedup();
        assert_eq!(aliases.len(), UiCue::ALL.len(), "one cue per alias");
        for c in UiCue::ALL {
            assert!(c.alias().starts_with("cac_") || c.alias().starts_with("uin_"), "{}", c.alias());
            assert_eq!(c.event(), format!("ZHBO_UI_{}", c.alias()));
        }
        // The create-a-class screens' own cues are among the converted ones.
        use zhbo_core::classui::cues;
        for event in [cues::ENTER_CAC, cues::DENY, cues::WEAPON_SELECT] {
            assert!(UiCue::ALL.iter().any(|c| c.event() == event), "{event}");
        }
    }

    #[test]
    fn rewrites_weapon_fire_sounds() {
        let ini = weapon_ini(WEAPON_INI, WEAPON_SOUNDS).unwrap();
        assert!(ini.contains("FireSound               = ZHBO_BO2_FireNpc"));
        assert!(ini.contains("FireSound               = ZHBO_BO2_FireRts"));
        assert!(!ini.contains("RangerWeapon"), "no Zero Hour rifle sound left");
        assert!(ini.contains("HeroUSAKnifeAttack"), "weapons not listed keep their sound");
    }

    #[test]
    fn the_dry_fire_events_cover_the_five_bo2_aliases() {
        for alias in ["wpn_generic_dryfire_plr", "wpn_gl_rifle_dryfire_plr", "wpn_crossbow_dryfire_plr", "wpn_rpg_dryfire_plr", "wpn_beretta93r_dryfire_plr"] {
            assert!(SFX.iter().any(|s| s.layers.contains(&alias)), "no event for {alias}");
        }
    }
}
