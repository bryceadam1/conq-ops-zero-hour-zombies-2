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
    /// The language zone's voice bank (`zone/english/en_common_mp.ff`, `mpl_common.english`):
    /// the player's own breathing.
    Voice,
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
    /// Positional, heard by everyone except its owner (the owner hears the `Player` version): the
    /// engine plays it with the owning player (an RTS weapon's `FireSound`), and Zero Hour's
    /// player-relative type needs that owner.
    Others,
    /// Positional, heard by everyone, for the sounds the core plays of another player's first-person
    /// General (his gun's third-person chain and its layers, `cues::third_person_shot`): the core
    /// already picks who hears it (never the shooter), and `ClientHost::sound` names no owner,
    /// which a player-relative type asserts on and refuses.
    Remote,
    /// The player's own 2D loop, played until the core stops it (`ClientHost::sound_loop`: a
    /// shellshock's ringing).
    PlayerLoop,
}

impl Kind {
    fn ini(self) -> &'static str {
        match self {
            Kind::Player | Kind::PlayerLoop => "ui player",
            Kind::World | Kind::Remote => "world shrouded everyone",
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
    // `_damagefeedback.gsc`'s other alerts: a hit by an inflictor with no `soundmod` (a grenade,
    // a rocket, a launched bolt) and the Shock Charge's (`taser_spike`).
    Sfx { event: "ZHBO_BO2_HitMarkerLow", from: From::Common, layers: &["mpl_hit_alert_low"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_HitMarkerTaser", from: From::Common, layers: &["mpl_hit_alert_taser_spike"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_Heartbeat", from: From::Common, layers: &["mpl_player_heartbeat"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_Hurt", from: From::World, layers: &["prj_bullet_impact_large_player"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_Step", from: From::World, layers: &["fly_step_run_grass"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_SprintStep", from: From::World, layers: &["fly_step_sprint_grass", "fly_gear_sprint_plr"], kind: Kind::Player, limit: 2 },
    Sfx { event: "ZHBO_BO2_Land", from: From::World, layers: &["fly_dtp_land_plr_grass"], kind: Kind::Player, limit: 1 },
    // Other players' footsteps, heard at them (`zhbo_core::sounds::RemoteSteps`): BO2's
    // third-person aliases (`_footsteps.csc`, `firstperson = 0`), positional for everyone.
    Sfx { event: "ZHBO_BO2_StepNpc", from: From::World, layers: &["fly_step_run_npc_grass"], kind: Kind::World, limit: 4 },
    Sfx { event: "ZHBO_BO2_SprintStepNpc", from: From::World, layers: &["fly_step_sprint_npc_grass"], kind: Kind::World, limit: 4 },
    // ... and their landings (`_footsteps.csc` `playerland`, `fly_land_npc_<surface>`).
    Sfx { event: "ZHBO_BO2_LandNpc", from: From::World, layers: &["fly_land_npc_grass"], kind: Kind::World, limit: 4 },
    // Hold breath: `clientscripts/mp/_audio.csc` plays `<teamclientprefix>_sinper_*` (`vox_gen` by
    // default) on the engine's `playerbreath*sound` notifies (`FUN_0052e3d0`).
    Sfx { event: "ZHBO_BO2_BreathIn", from: From::Voice, layers: &["vox_gen_sinper_hold"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_BreathOut", from: From::Voice, layers: &["vox_gen_sinper_exhale"], kind: Kind::Player, limit: 1 },
    Sfx { event: "ZHBO_BO2_BreathGasp", from: From::Voice, layers: &["vox_gen_sinper_gasp"], kind: Kind::Player, limit: 1 },
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

/// The language zone whose `mpl_common.<language>` bank holds the player's voice (breathing).
pub const VOICE_ZONE: &str = "common_mp";

/// A language zone (`zone/<language>/<prefix><name>.ff`, `zhbo_formats::t6zone::language`).
fn load_language_zone(bo2_dir: &Path, name: &str) -> Result<Zone, String> {
    zhbo_formats::t6zone::load_file(&zhbo_formats::t6zone::language_path(bo2_dir, name))
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
pub(crate) const NOTETRACK_PREFIX: &str = "sndnt#";

/// The team's voice alias for voice number `n` ending in `suffix` in a faction's voice bank:
/// `vox_<team><n><suffix>` (the bank holds one team's).
fn voice_alias(src: &Source, n: usize, suffix: &str) -> Option<String> {
    let mut names: Vec<&str> = src
        .banks
        .iter()
        .flat_map(|b| b.aliases.iter().map(|a| a.name.as_str()))
        .filter(|a| a.starts_with("vox_") && a.ends_with(suffix))
        .filter(|a| a[..a.len() - suffix.len()].ends_with(&n.to_string()))
        .filter(|a| a[4..a.len() - suffix.len() - 1].chars().all(|c| c.is_ascii_alphabetic()))
        .collect();
    names.sort_unstable();
    names.first().map(|s| s.to_string())
}

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
            samples: data.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
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
fn render(mixer: &Mixer, layers: &[(&sndalias::Variant, Pcm, f32)]) -> Option<(Vec<f32>, u16, u32)> {
    let rate = layers.iter().map(|l| l.1.rate).max()?;
    let mixer = Mixer { rate: rate as f32, ..*mixer };
    let channels = layers.iter().map(|l| l.1.channels).max()?;
    let floats: Vec<Vec<f32>> = layers.iter().map(|(_, p, _)| to_float(p, rate, channels)).collect();
    let mix: Vec<Layer> = layers
        .iter()
        .zip(&floats)
        .map(|((v, _, gain), samples)| Layer {
            samples,
            start_frame: (v.start_delay_ms as u64 * rate as u64 / 1000) as usize,
            bus: mixer.bus(v),
            gain: *gain,
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

/// One alias of a third-person chain as a `soundlayers` row: its distances in Zero Hour units and
/// its two dry curves; a 2D alias is flat (BO2 does not attenuate it).
fn layer_row(mixer: &Mixer, v: &sndalias::Variant, event: &str) -> zhbo_core::soundlayers::Layer {
    use zhbo_core::soundlayers::{Layer, FLAT};
    let event: &'static str = Box::leak(event.to_string().into_boxed_str());
    let curve = |c: u8| mixer.globals.curve(c).map_or(FLAT, |c| c.points);
    if !v.mix.positional {
        return Layer { event, min: 0.0, max: 1e9, min_curve: FLAT, curve: FLAT };
    }
    let units = zhbo_core::units::inches;
    Layer { event, min: units(v.distance.0), max: units(v.distance.1), min_curve: curve(v.mix.min_falloff_curve), curve: curve(v.mix.falloff_curve) }
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
    // A loop plays until removed (Zero Hour's permanent sound: `LOOP` with `LoopCount = 0`);
    // everything else is a one-shot that may cut its oldest instance.
    let control = if kind == Kind::PlayerLoop { "loop\n  LoopCount   = 0" } else { "interrupt random" };
    let kind = match (kind, range) {
        (Kind::Player | Kind::PlayerLoop, _) => kind.ini().to_string(),
        (_, Some((min, max))) => format!("{}\n  MinRange    = {min:.0}\n  MaxRange    = {max:.0}", kind.ini()),
        (_, None) => format!("{}\n  MinRange    = 40\n  MaxRange    = 600", kind.ini()),
    };
    format!(
        "AudioEvent {name}\n  Sounds      = {}\n  Control     = {control}\n  Priority    = high\n  \
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
    factions: &[(u8, &str)],
) -> Result<(crate::generals::Files, String), String> {
    let lib = Library::open(bo2_dir)?;
    let pick = |zone: &Zone, prefix: &str| -> Option<Source> {
        let mut banks = sndalias::banks(zone).ok()?;
        banks.retain(|b| b.name.starts_with(prefix));
        (!banks.is_empty()).then_some(Source { banks, lib: &lib })
    };
    let common_src = pick(common, "mpl_common").ok_or("no mpl_common sound bank in common_mp")?;
    let world_src = pick(world, "mpl_").ok_or_else(|| format!("no sound bank in {WORLD_ZONE}"))?;
    // The voice bank is optional like the menu banks: without it the breath events stay missing.
    let voice_zone = load_language_zone(bo2_dir, VOICE_ZONE).ok();
    let voice_src = voice_zone.as_ref().and_then(|z| pick(z, "mpl_common"));
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
    let mut layer_rows = String::from("; Third-person shots layer by layer (zhbo_core::soundlayers): layer <fx weapon> <chain> <event> <min> <max> <min curve> <curve>\n");

    let mut add = |event: &str, src: &Source, layers: &[&str], kind: Kind, limit: u8| -> bool {
        let aliases = src.chained(layers);
        if aliases.is_empty() {
            return false;
        }
        // One output variant per variant of the first layer; other layers use their first variant.
        let base = aliases[0];
        let mut variants = Vec::new();
        for (i, v) in base.variants.iter().enumerate().take(8) {
            let parts: Vec<(&sndalias::Variant, Pcm, f32)> = aliases
                .iter()
                .enumerate()
                .map(|(k, a)| if k == 0 { v } else { &a.variants[0] })
                .map(|var| (var, mixer.gain(var, 0.0)))
                .filter(|(_, g)| *g > 0.0)
                .filter_map(|(var, g)| Some((var, src.decode(var.asset_id)?, g)))
                .collect();
            let Some((samples, channels, rate)) = render(&mixer, &parts) else { continue };
            let level = peak(&samples);
            variants.push((sound_file(event, i), quantize(&samples, level), level, channels, rate));
        }
        if variants.is_empty() {
            return false;
        }
        let chain: Vec<&sndalias::Variant> = aliases.iter().filter_map(|a| a.variants.first()).collect();
        let range = if matches!(kind, Kind::Player | Kind::PlayerLoop) { None } else { falloff(&mixer, &chain) };
        pending.push(Pending { event: event.to_string(), kind, limit, range, variants });
        // A converted gun's third-person shot also ships layer by layer for the client to level by
        // BO2's distance curves (`zhbo_core::soundlayers`), keyed by the gun's FX weapon; another
        // remote cue (a burst's opening, the last round: `WeaponDef::npc_fire_start_sound`) by its
        // own event.
        if kind == Kind::Remote {
            let fx = event.strip_prefix(zhbo_core::names::NPC_FIRE_PREFIX).map_or_else(|| event.to_string(), zhbo_core::names::fx_weapon);
            let reach = zhbo_core::units::inches(mixer.audible_range(&chain).max(1.0));
            for (k, a) in aliases.iter().enumerate() {
                let Some(first) = a.variants.first() else { continue };
                if mixer.dry_gain(first) <= 0.0 {
                    continue;
                }
                let name = zhbo_core::names::layer_event(event, k);
                let variants: Vec<_> = a
                    .variants
                    .iter()
                    .take(8)
                    .enumerate()
                    .filter_map(|(i, var)| {
                        let (samples, channels, rate) = render(&mixer, &[(var, src.decode(var.asset_id)?, mixer.dry_gain(var))])?;
                        let level = peak(&samples);
                        Some((sound_file(&name, i), quantize(&samples, level), level, channels, rate))
                    })
                    .collect();
                if variants.is_empty() {
                    continue;
                }
                let layer = layer_row(&mixer, first, &name);
                // No Zero Hour falloff inside its reach: the client applies BO2's curve.
                let r = if first.mix.positional { layer.max.max(1.0) } else { reach };
                layer_rows.push_str(&zhbo_core::soundlayers::format_row(&fx, event, &layer));
                pending.push(Pending { event: name, kind, limit, range: Some((r, r)), variants });
            }
        }
        true
    };

    for s in SFX {
        let src = match s.from {
            From::Common => &common_src,
            From::World => &world_src,
            From::Voice => match &voice_src {
                Some(v) => v,
                None => {
                    missing.push(s.event.to_string());
                    continue;
                }
            },
        };
        // Some aliases BO2 keeps only in the map's bank (the generic dry fire): fall back to it
        // rather than dropping the event. Common first, so aliases that already resolve do not change.
        let ok = add(s.event, src, s.layers, s.kind, s.limit)
            || (s.from == From::Common && add(s.event, &world_src, s.layers, s.kind, s.limit));
        if !ok {
            missing.push(s.event.to_string());
        }
    }

    // Each faction's voice (`zhbo_core::sounds::voice_event`): its language zone's `mpl_vox_*`
    // bank names the team's aliases `<teamprefix><n>_pain_small` / `_pain_death` for voice
    // numbers 0..2 (`_battlechatter_mp.gsc`; the prefix is the team's, `vox_st` for the SEALs).
    let mut voices_made = 0;
    for &(letter, zone) in factions {
        let Some(f) = zhbo_core::sounds::VOICE_FACTIONS.iter().position(|&x| x == letter) else { continue };
        let lang = load_language_zone(bo2_dir, zone).ok();
        let src = lang.as_ref().and_then(|z| pick(z, "mpl_vox"));
        for (v, suffix) in [(zhbo_core::sounds::Voice::Pain, "_pain_small"), (zhbo_core::sounds::Voice::Death, "_pain_death")] {
            for n in 0..zhbo_core::sounds::VOICES {
                let event = zhbo_core::sounds::VOICE_EVENTS[v as usize][f][n];
                let alias = src.as_ref().and_then(|s| voice_alias(s, n, suffix));
                if alias.is_some_and(|a| add(event, src.as_ref().expect("an alias has a bank"), &[a.as_str()], Kind::World, 2)) {
                    voices_made += 1;
                } else {
                    missing.push(event.to_string());
                }
            }
        }
        // The battle chatter's lines (`zhbo_core::sounds::CHATTER_LINES`), each voice's own.
        for n in 0..zhbo_core::sounds::VOICES {
            for line in zhbo_core::sounds::CHATTER_LINES {
                let event = zhbo_core::sounds::chatter_event_name(letter, n, line);
                let alias = src.as_ref().and_then(|s| voice_alias(s, n, &format!("_{line}")));
                if alias.is_some_and(|a| add(&event, src.as_ref().expect("an alias has a bank"), &[a.as_str()], Kind::World, 2)) {
                    voices_made += 1;
                } else {
                    missing.push(event);
                }
            }
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
    out.insert(zhbo_core::names::SOUND_LAYERS_PATH.into(), layer_rows.into_bytes());
    // Zero Hour allows only 4 simultaneous 2D sounds; automatic fire plus footsteps and reload
    // foley need more. The engine loads this folder after AudioSettings.ini.
    out.insert(
        "Data\\INI\\AudioSettings\\ZHBO_BO2.ini".into(),
        b"AudioSettings\n  SampleCount2D = 16\nEnd\n".to_vec(),
    );
    let wavs = out.keys().filter(|k| k.ends_with(".wav")).count();
    let summary = format!(
        "{wavs} sounds, {} events, {ui_made} menu events, {voices_made} voice events, {} notetrack events, loudest {loudest_event}{}",
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

        let extra = [DynSfx { event: zhbo_core::names::npc_fire_event("an94_mp"), layers: vec!["wpn_an94_fire_npc".into()], kind: Kind::Remote, limit: 3 }];
        let factions: Vec<(u8, &str)> = crate::generals::FACTIONS.iter().map(|f| (f.letter as u8, f.zone)).collect();
        let (files, summary) = files(&common, &world, &dir, &[], &extra, &factions).unwrap();
        // The layers ship keyed by the gun's FX weapon: the close shot, the distant layer and the
        // tail (not the LFE-only low end), each with its own curves; the client's rule on them
        // crossfades as BO2's does.
        let rows = files.iter().find(|f| f.0 == zhbo_core::names::SOUND_LAYERS_PATH).map(|f| String::from_utf8_lossy(&f.1).into_owned()).unwrap();
        let table = zhbo_core::soundlayers::parse(&rows);
        let shot = &table[&zhbo_core::names::fx_weapon("an94_mp")];
        assert_eq!(shot.chain, "ZHBO_BO2_FireNpc_an94_mp");
        let events: Vec<&str> = shot.layers.iter().map(|l| l.event).collect();
        assert_eq!(events, ["ZHBO_BO2_FireNpc_an94_mp_L0", "ZHBO_BO2_FireNpc_an94_mp_L2", "ZHBO_BO2_FireNpc_an94_mp_L3"]);
        let ft = zhbo_core::units::inches;
        assert_eq!((shot.layers[0].gain(0.0), shot.layers[1].gain(0.0)), (1.0, 0.0), "point-blank: the close shot, no distant layer");
        assert_eq!(shot.layers[0].gain(ft(3000.0)), 0.0);
        assert!(shot.layers[1].gain(ft(3000.0)) > 0.5, "across the map: the distant layer");
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
        assert_eq!(field("ZHBO_BO2_FireNpc_an94_mp_L2", "MinRange"), field("ZHBO_BO2_FireNpc_an94_mp_L2", "MaxRange"), "no Zero Hour falloff on a layer");
        let vol = |name: &str| field(name, "Volume");
        assert!(vol("ZHBO_BO2_Fire") > vol("ZHBO_BO2_Step"), "the shot over a footstep");
        // Every faction's pain and death voice, for every voice number (`_battlechatter_mp.gsc`):
        // positional, heard at BO2's 150..1200" (the alias's own distances).
        for v in zhbo_core::sounds::VOICE_EVENTS.iter().flatten().flatten() {
            assert!(event(v).contains("world shrouded everyone"), "{v}");
            assert_eq!(field(v, "MaxRange"), zhbo_core::units::inches(1200.0).round(), "{v}");
        }
        // ... and every battle chatter line in each voice (`_battlechatter_mp.gsc`).
        for &f in &zhbo_core::sounds::VOICE_FACTIONS {
            for n in 0..zhbo_core::sounds::VOICES {
                for line in zhbo_core::sounds::CHATTER_LINES {
                    let e = zhbo_core::sounds::chatter_event_name(f, n, line);
                    assert!(event(&e).contains("world shrouded everyone"), "{e}");
                }
            }
        }
        assert!(event("ZHBO_BO2_LandNpc").contains("world shrouded everyone"));
    }

    /// Needs the player's BO2 install. The menu cues the converter leaves missing are in no bank
    /// of any zone t6mp loads: its zone list (`FUN_00597ed0`) appends the mode suffix `_mp` to
    /// `code_pre_gfx`, `code_post_gfx`, `patch`, `patch_ui`, `common`, and the shell adds `ui_mp`;
    /// SP's `code_post_gfx` holds seven of them (`spl_code_post_gfx`), the rest exist in no zone of
    /// the install at all. BO2's MP menus hash-miss them and play nothing, as the mod does.
    #[test]
    #[ignore]
    fn the_missing_menu_cues_are_in_no_mp_zone() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let zones = ["code_pre_gfx_mp", "code_post_gfx_mp", "patch_mp", "patch_ui_mp", "common_mp", "ui_mp", "common_patch_mp"];
        let banks: Vec<Bank> = zones
            .iter()
            .filter_map(|z| crate::generals::load_zone(&dir, z).ok())
            .flat_map(|z| sndalias::banks(&z).unwrap_or_default())
            .collect();
        assert!(banks.iter().any(|b| b.name.starts_with("mpl_code_post_gfx")), "the MP shell bank loaded");
        let present = |alias: &str| banks.iter().any(|b| b.alias(alias).is_some_and(|a| !a.variants.is_empty()));
        let missing: Vec<&str> = zhbo_core::sounds::UiCue::ALL.iter().map(|c| c.alias()).filter(|a| !present(a)).collect();
        assert!(missing.contains(&"uin_main_nav") && missing.contains(&"cac_cmn_item_remove"), "{missing:?}");
        let sp = crate::generals::load_zone(&dir, "code_post_gfx").unwrap();
        let sp_banks = sndalias::banks(&sp).unwrap();
        assert!(sp_banks.iter().any(|b| b.name.starts_with("spl_") && b.alias("uin_main_nav").is_some()), "SP-only");
        println!("{} menu cues in no MP zone: {missing:?}", missing.len());
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_breath_sounds_decode_from_the_voice_bank() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let lib = Library::open(&dir).unwrap();
        let zone = load_language_zone(&dir, VOICE_ZONE).unwrap();
        let mut banks = sndalias::banks(&zone).unwrap();
        banks.retain(|b| b.name.starts_with("mpl_common"));
        let src = Source { banks, lib: &lib };
        for s in SFX.iter().filter(|s| s.from == From::Voice) {
            let a = src.alias(s.layers[0]).unwrap_or_else(|| panic!("{} missing", s.layers[0]));
            assert!(src.decode(a.variants[0].asset_id).is_some_and(|p| !p.samples.is_empty()), "{} decodes", s.event);
        }
        for e in [zhbo_core::sounds::BREATH_IN, zhbo_core::sounds::BREATH_OUT, zhbo_core::sounds::BREATH_GASP] {
            assert!(SFX.iter().any(|s| s.event == e), "{e}");
        }
    }

    #[test]
    fn events_are_well_formed() {
        let e = audio_event("ZHBO_BO2_Fire", &["zhbo_bo2_fire_0".into()], 100, Kind::Player, 3, None);
        assert!(e.contains("Volume      = 100") && e.contains("Type        = ui player") && e.ends_with("End\n\n"));
        let w = audio_event("ZHBO_BO2_Impact", &["x".into()], 40, Kind::World, 4, Some((120.0, 900.0)));
        assert!(w.contains("world shrouded everyone") && w.contains("MinRange    = 120") && w.contains("MaxRange    = 900"));
        let o = audio_event("ZHBO_BO2_FireNpc", &["x".into()], 40, Kind::Others, 4, None);
        // A shellshock's ringing loops until the core stops it, as a 2D player sound.
        let l = audio_event("ZHBO_BO2_Shock_chr_flashbang_tinnitus_loop", &["x".into()], 30, Kind::PlayerLoop, 1, None);
        assert!(l.contains("Control     = loop\n  LoopCount   = 0\n") && l.contains("Type        = ui player\n") && !l.contains("MinRange"), "{l}");
        assert!(!e.contains("loop"), "{e}");
        assert!(o.contains("allies enemies") && !o.contains("player"));
        // The core plays another player's shot with no owner: a player-relative type would assert.
        let r = audio_event("ZHBO_BO2_FireNpc_an94_mp_L0", &["x".into()], 40, Kind::Remote, 3, Some((900.0, 900.0)));
        assert!(r.contains("world shrouded everyone") && !r.contains("allies") && !r.contains("enemies"), "{r}");
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
