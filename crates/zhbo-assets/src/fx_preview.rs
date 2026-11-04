//! A headless look at the converted effects: the converter's Zero Hour particle systems played by
//! Zero Hour's own particle rules (`ParticleSys.cpp`: bursts, logic-frame lifetimes, per-frame
//! velocity damping and gravity, size rates, alpha and colour key frames), and the BO2 effect they
//! came from played by BO2's (`FxElemDef`: spawn counts, delays and loops, the velocity graph's
//! integrated delta, the visual-state graph, runners), both drawn as billboards from one camera.
//!
//! [`metrics`] reduces a played effect to numbers the sweep compares: how much it covers over its
//! life (`mass`: width² × opacity, summed over particles and time), its peak coverage and its reach.
//! The contact sheets (`zhbo-assets fx-sheet`) put BO2's row over Zero Hour's for each effect.
//! Nothing here reaches the game: it reads the converter's output and the parsed BO2 effect.

use std::collections::BTreeMap;
use zhbo_formats::fx::{kind, Effect, Elem};
use zhbo_formats::iwi::Rgba;
use zhbo_formats::w3d::Blend;

/// Milliseconds per Zero Hour logic frame.
const MS_PER_FRAME: f32 = 1000.0 / 30.0;

/// One drawn particle: where (Zero Hour units, Z up), its full width, colour and opacity.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub pos: [f32; 3],
    pub size: f32,
    /// RGB 0..1, multiplied with the texture.
    pub color: [f32; 3],
    pub alpha: f32,
    pub additive: bool,
    pub texture: String,
    /// The flipbook cell drawn: the grid's side and the cell (row by row from the top left);
    /// `(1, 0)` is the whole texture.
    pub cell: (u32, u32),
}

impl Sprite {
    /// How much the particle shows: its alpha when blended, its brightness when added.
    pub fn opacity(&self) -> f32 {
        if self.additive {
            self.color.iter().fold(0.0f32, |a, &c| a.max(c)) * self.alpha
        } else {
            self.alpha
        }
    }
}

/// Deterministic randomness for both players.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

// ------------------------------------------------------------------------------------------------
// Zero Hour: the converter's INI text, played by ParticleSys.cpp's rules
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ZhSystem {
    pub additive: bool,
    pub texture: String,
    pub lifetime: (f32, f32),
    pub system_lifetime: f32,
    pub size: (f32, f32),
    pub size_rate: (f32, f32),
    pub size_rate_damping: (f32, f32),
    pub alpha: Vec<(f32, f32, u32)>,
    pub color: Vec<([f32; 3], u32)>,
    pub burst_delay: (f32, f32),
    pub burst_count: (f32, f32),
    pub initial_delay: (f32, f32),
    pub vel: [(f32, f32); 3],
    pub vel_damping: (f32, f32),
    pub gravity: f32,
    /// POINT, SPHERE (radius), BOX (half sizes), CYLINDER (radius, length).
    pub volume: String,
    pub sphere: f32,
    pub box_half: [f32; 3],
    pub cylinder: (f32, f32),
    /// Host patch 0028's flipbook: `FrameGrid`, `FrameCount`, `FrameStartType`, `FrameStart`,
    /// `FrameRate`, `FrameOverLife`, `FrameLoops`.
    pub frame_grid: u32,
    pub frame_count: u32,
    pub frame_start_type: String,
    pub frame_start: u32,
    pub frame_rate: f32,
    pub frame_over_life: bool,
    pub frame_loops: u32,
    /// Host patch 0028's distance ranges from the camera (`SpawnRange`, `FarFade`, `NearFade`).
    pub spawn_range: (f32, f32),
    pub far_fade: (f32, f32),
    pub near_fade: (f32, f32),
}

/// Where the sheets and the sweep look from ([`sheet_camera`] at 40 feet): the distance ranges are
/// measured from it on both sides.
pub fn eye() -> [f32; 3] {
    sheet_camera(zhbo_core::units::inches(SHEET_DISTANCE_INCHES)).eye
}

/// How far the sheets' camera stands from the effect (inches).
pub const SHEET_DISTANCE_INCHES: f32 = 480.0;

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Whether something `d` from the camera is in a spawn range `(min, max)` (max 0: no range).
pub fn in_spawn_range(range: (f32, f32), d: f32) -> bool {
    range.1 <= 0.0 || (d >= range.0 && d <= range.1)
}

/// How much shows `d` from the camera under a far fade (gone past its end) and a near fade (gone
/// nearer than its start): host patch 0028's `ParticleSystemInfo::getDistanceFade`.
pub fn distance_fade(far: (f32, f32), near: (f32, f32), d: f32) -> f32 {
    let mut f = 1.0;
    if far.1 > far.0 {
        f *= 1.0 - ((d - far.0) / (far.1 - far.0)).clamp(0.0, 1.0);
    }
    if near.1 > near.0 {
        f *= ((d - near.0) / (near.1 - near.0)).clamp(0.0, 1.0);
    }
    f
}

impl ZhSystem {
    /// The cell a particle shows (`Particle::getFrame`, host patch 0028): `personality` is its
    /// spawn index in the system, `age` the updates it has had of its `life`.
    pub fn frame(&self, personality: u32, age: u32, life: u32) -> u32 {
        let mut log2 = 0;
        while log2 < 4 && (2u32 << log2) <= self.frame_grid {
            log2 += 1;
        }
        let count = self.frame_count.min(1 << (2 * log2));
        if count <= 1 {
            return 0;
        }
        let start = match self.frame_start_type.as_str() {
            "RANDOM" => (personality.wrapping_add(1).wrapping_mul(2654435761)) >> 8,
            "INDEXED" => personality,
            _ => self.frame_start,
        } % count;
        let step = if self.frame_over_life {
            if life > 0 { (age as u64 * count as u64 / life as u64) as u32 } else { 0 }
        } else if self.frame_rate > 0.0 {
            (age as f32 * self.frame_rate / 30.0) as u32
        } else {
            0
        };
        let step = if self.frame_loops > 0 { step.min(self.frame_loops * count - 1) } else { step };
        (start + step) % count
    }

    /// The grid's side as W3D draws it (a power of two, at most 16).
    pub fn grid_side(&self) -> u32 {
        let mut side = 1;
        while side < 16 && side * 2 <= self.frame_grid {
            side *= 2;
        }
        side
    }
}

#[derive(Clone, Debug, Default)]
pub struct ZhNugget {
    pub system: String,
    pub offset: [f32; 3],
    pub orient: bool,
    /// The nugget's `InitialDelay` (msec), when set.
    pub delay_ms: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct ZhLight {
    pub color: [f32; 3],
    pub radius: f32,
    pub increase: u32,
    pub decrease: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ZhList {
    pub systems: Vec<ZhNugget>,
    pub lights: Vec<ZhLight>,
}

fn pair(v: &str) -> (f32, f32) {
    let mut it = v.split_whitespace().filter_map(|t| t.parse::<f32>().ok());
    let a = it.next().unwrap_or(0.0);
    (a, it.next().unwrap_or(a))
}

fn xyz(v: &str) -> [f32; 3] {
    let mut out = [0.0; 3];
    for t in v.split_whitespace() {
        let (k, n) = t.split_once(':').unwrap_or(("", ""));
        let n = n.parse().unwrap_or(0.0);
        match k {
            "X" | "R" => out[0] = n,
            "Y" | "G" => out[1] = n,
            "Z" | "B" => out[2] = n,
            _ => {}
        }
    }
    out
}

/// The `ParticleSystem` blocks of an INI text, by name.
pub fn parse_systems(text: &str) -> BTreeMap<String, ZhSystem> {
    let mut out = BTreeMap::new();
    let mut cur: Option<(String, ZhSystem)> = None;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix("ParticleSystem ") {
            cur = Some((name.trim().to_string(), ZhSystem { vel_damping: (1.0, 1.0), size_rate_damping: (1.0, 1.0), ..Default::default() }));
            continue;
        }
        if line == "End" {
            if let Some((n, s)) = cur.take() {
                out.insert(n, s);
            }
            continue;
        }
        let Some((_, s)) = cur.as_mut() else { continue };
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "Shader" => s.additive = v == "ADDITIVE",
            "ParticleName" => s.texture = v.to_string(),
            "Lifetime" => s.lifetime = pair(v),
            "SystemLifetime" => s.system_lifetime = pair(v).0,
            "Size" => s.size = pair(v),
            "SizeRate" => s.size_rate = pair(v),
            "SizeRateDamping" => s.size_rate_damping = pair(v),
            "BurstDelay" => s.burst_delay = pair(v),
            "BurstCount" => s.burst_count = pair(v),
            "InitialDelay" => s.initial_delay = pair(v),
            "VelOrthoX" => s.vel[0] = pair(v),
            "VelOrthoY" => s.vel[1] = pair(v),
            "VelOrthoZ" => s.vel[2] = pair(v),
            "VelocityDamping" => s.vel_damping = pair(v),
            "Gravity" => s.gravity = pair(v).0,
            "VolumeType" => s.volume = v.to_string(),
            "VolSphereRadius" => s.sphere = pair(v).0,
            "VolBoxHalfSize" => s.box_half = xyz(v),
            "VolCylinderRadius" => s.cylinder.0 = pair(v).0,
            "VolCylinderLength" => s.cylinder.1 = pair(v).0,
            "FrameGrid" => s.frame_grid = pair(v).0 as u32,
            "FrameCount" => s.frame_count = pair(v).0 as u32,
            "FrameStartType" => s.frame_start_type = v.to_string(),
            "FrameStart" => s.frame_start = pair(v).0 as u32,
            "FrameRate" => s.frame_rate = pair(v).0,
            "FrameOverLife" => s.frame_over_life = v == "Yes",
            "FrameLoops" => s.frame_loops = pair(v).0 as u32,
            "SpawnRange" => s.spawn_range = pair(v),
            "FarFade" => s.far_fade = pair(v),
            "NearFade" => s.near_fade = pair(v),
            k if k.starts_with("Alpha") => {
                let n: Vec<f32> = v.split_whitespace().filter_map(|t| t.parse().ok()).collect();
                if n.len() == 3 {
                    s.alpha.push((n[0], n[1], n[2] as u32));
                }
            }
            k if k.starts_with("Color") && k != "ColorScale" => {
                let frame = v.split_whitespace().last().and_then(|t| t.parse().ok()).unwrap_or(0);
                let c = xyz(v);
                s.color.push(([c[0] / 255.0, c[1] / 255.0, c[2] / 255.0], frame));
            }
            _ => {}
        }
    }
    out
}

/// The `FXList` blocks of an INI text, by name.
pub fn parse_lists(text: &str) -> BTreeMap<String, ZhList> {
    #[derive(PartialEq)]
    enum In {
        None,
        System,
        Light,
        Other,
    }
    let mut out = BTreeMap::new();
    let mut cur: Option<(String, ZhList)> = None;
    let mut nugget = In::None;
    for raw in text.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix("FXList ") {
            cur = Some((name.trim().to_string(), ZhList::default()));
            continue;
        }
        let Some((_, list)) = cur.as_mut() else { continue };
        if line == "End" {
            if nugget == In::None {
                let (n, l) = cur.take().unwrap();
                out.insert(n, l);
            }
            nugget = In::None;
            continue;
        }
        if nugget == In::None {
            nugget = match line {
                "ParticleSystem" => {
                    list.systems.push(ZhNugget::default());
                    In::System
                }
                "LightPulse" => {
                    list.lights.push(ZhLight::default());
                    In::Light
                }
                _ => In::Other,
            };
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match nugget {
            In::System => {
                let n = list.systems.last_mut().unwrap();
                match k {
                    "Name" => n.system = v.to_string(),
                    "Offset" => n.offset = xyz(v),
                    "OrientToObject" => n.orient = v == "Yes",
                    "InitialDelay" => n.delay_ms = Some(pair(v).0),
                    _ => {}
                }
            }
            In::Light => {
                let l = list.lights.last_mut().unwrap();
                match k {
                    "Color" => l.color = xyz(v).map(|c| c / 255.0),
                    "Radius" => l.radius = pair(v).0,
                    "IncreaseTime" => l.increase = pair(v).0 as u32,
                    "DecreaseTime" => l.decrease = pair(v).0 as u32,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

struct ZhParticle {
    pos: [f32; 3],
    vel: [f32; 3],
    damping: f32,
    size: f32,
    size_rate: f32,
    size_damping: f32,
    life: u32,
    age: u32,
    alpha: Vec<(f32, u32)>,
    /// Its spawn index in the system (Zero Hour's personality) and the life it started with.
    personality: u32,
    lifetime: u32,
}

/// A value of key frames at `age` (linear between keys; the first holds until its frame, a key
/// past the first with frame 0 ends the sequence).
fn keyed<T: Copy>(keys: &[(T, u32)], age: u32, lerp: impl Fn(T, T, f32) -> T) -> Option<T> {
    let first = keys.first()?;
    let mut prev = *first;
    for (i, k) in keys.iter().enumerate().skip(1) {
        if k.1 == 0 && i > 0 {
            break;
        }
        if age <= k.1 {
            let span = k.1.saturating_sub(prev.1).max(1) as f32;
            let t = (age.saturating_sub(prev.1)) as f32 / span;
            return Some(lerp(prev.0, k.0, t.clamp(0.0, 1.0)));
        }
        prev = *k;
    }
    Some(prev.0)
}

/// Play the FXList `list` for `frames` logic frames and return what is drawn after the last one.
/// An oriented list is shown as if played on the ground (its X along the normal: up).
pub fn play_zh(list: &ZhList, systems: &BTreeMap<String, ZhSystem>, frames: u32, seed: u64) -> Vec<Sprite> {
    let mut out = Vec::new();
    play_zh_each(list, systems, frames, seed, |f, s| {
        if f + 1 == frames as usize {
            out.push(s);
        }
    });
    out
}

/// The most particles one system keeps (a guard for the sweep, far above any converted effect).
const MAX_PARTICLES: usize = 20_000;

/// [`play_zh`], handing every frame's sprites to `visit` (frame index from 0: after frame 1).
pub fn play_zh_each(list: &ZhList, systems: &BTreeMap<String, ZhSystem>, frames: u32, seed: u64, mut visit: impl FnMut(usize, Sprite)) {
    for (i, nugget) in list.systems.iter().enumerate() {
        // Each system its own stream: a shorter play is a prefix of a longer one.
        let mut rng = Rng((seed.max(1) + i as u64 * 7919).wrapping_mul(0x9E3779B97F4A7C15) | 1);
        let Some(sys) = systems.get(&nugget.system) else { continue };
        // The frame the caller passes for an oriented list: X (BO2's normal) up, Y and Z level.
        let frame = |v: [f32; 3]| if nugget.orient { [v[1], v[2], v[0]] } else { v };
        let delay = match nugget.delay_ms {
            Some(ms) if ms >= 0.0 => (ms / MS_PER_FRAME).ceil() as u32,
            _ => rng.range(sys.initial_delay.0, sys.initial_delay.1) as u32,
        };
        let forever = sys.system_lifetime <= 0.0;
        let mut life_left = sys.system_lifetime as u32;
        let mut burst_left = 0u32;
        let mut parts: Vec<ZhParticle> = Vec::new();
        let mut personality = 0u32;
        // ParticleSystem::update, once per logic frame; the delay counts down first.
        let mut delay_left = delay;
        for fi in 0..frames as usize {
            if delay_left > 0 {
                delay_left -= 1;
                continue;
            }
            if forever || life_left > 0 {
                if burst_left == 0 {
                    let n = rng.range(sys.burst_count.0, sys.burst_count.1) as u32;
                    // None outside the spawn range (the system stands at its offset).
                    let n = if in_spawn_range(sys.spawn_range, distance(frame(nugget.offset), eye())) { n } else { 0 };
                    for _ in 0..n.min((MAX_PARTICLES - parts.len().min(MAX_PARTICLES)) as u32) {
                        let local = match sys.volume.as_str() {
                            "SPHERE" => {
                                let r = rng.range(0.0, sys.sphere);
                                let (u, w) = (rng.range(-1.0, 1.0), rng.range(0.0, std::f32::consts::TAU));
                                let s = (1.0 - u * u).sqrt();
                                [r * s * w.cos(), r * s * w.sin(), r * u]
                            }
                            "BOX" => [rng.range(-1.0, 1.0) * sys.box_half[0], rng.range(-1.0, 1.0) * sys.box_half[1], rng.range(-1.0, 1.0) * sys.box_half[2]],
                            "CYLINDER" => {
                                let (r, a) = (rng.range(0.0, sys.cylinder.0), rng.range(0.0, std::f32::consts::TAU));
                                [r * a.cos(), r * a.sin(), rng.range(-0.5, 0.5) * sys.cylinder.1]
                            }
                            _ => [0.0; 3],
                        };
                        let vel = [rng.range(sys.vel[0].0, sys.vel[0].1), rng.range(sys.vel[1].0, sys.vel[1].1), rng.range(sys.vel[2].0, sys.vel[2].1)];
                        let off = frame(nugget.offset);
                        let local = frame(local);
                        parts.push(ZhParticle {
                            pos: [local[0] + off[0], local[1] + off[1], local[2] + off[2]],
                            vel: frame(vel),
                            damping: rng.range(sys.vel_damping.0, sys.vel_damping.1),
                            size: rng.range(sys.size.0, sys.size.1),
                            size_rate: rng.range(sys.size_rate.0, sys.size_rate.1),
                            size_damping: rng.range(sys.size_rate_damping.0, sys.size_rate_damping.1),
                            life: rng.range(sys.lifetime.0, sys.lifetime.1).max(1.0) as u32,
                            age: 0,
                            alpha: sys.alpha.iter().map(|&(a, b, f)| (rng.range(a, b), f)).collect(),
                            personality,
                            lifetime: 0,
                        });
                        personality += 1;
                        let p = parts.last_mut().unwrap();
                        p.lifetime = p.life;
                    }
                    burst_left = rng.range(sys.burst_delay.0, sys.burst_delay.1) as u32;
                } else {
                    burst_left -= 1;
                }
            }
            // Particle::update then the draw's integration.
            parts.retain_mut(|p| {
                p.life -= 1;
                if p.life == 0 {
                    return false;
                }
                p.vel[2] += sys.gravity;
                for v in &mut p.vel {
                    *v *= p.damping;
                }
                if p.age > 0 {
                    p.size_rate *= p.size_damping;
                }
                for i in 0..3 {
                    p.pos[i] += p.vel[i];
                }
                p.size += p.size_rate;
                p.age += 1;
                true
            });
            if !forever && life_left > 0 {
                life_left -= 1;
            }
            for p in &parts {
            let alpha = if sys.additive { p.alpha.first().map_or(1.0, |a| a.0) } else { keyed(&p.alpha, p.age, |a, b, t| a + (b - a) * t).unwrap_or(1.0) };
            let color = keyed(&sys.color, p.age, |a, b, t| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]).unwrap_or([1.0; 3]);
            // ADDITIVE draws texture × colour; its alpha keys are not played.
            let alpha = if sys.additive { 1.0 } else { alpha.clamp(0.0, 1.0) };
            // The distance fades: an additive particle darkens, the others fade.
            let fade = distance_fade(sys.far_fade, sys.near_fade, distance(p.pos, eye()));
            let (color, alpha) = if sys.additive { (color.map(|c| c * fade), alpha) } else { (color, alpha * fade) };
            // The cell after this update: the life counted down (`m_lifetime - m_lifetimeLeft`).
            let cell = (sys.grid_side(), sys.frame(p.personality, p.lifetime - p.life, p.lifetime));
            visit(fi, Sprite { pos: p.pos, size: p.size.max(0.0), color, alpha, additive: sys.additive, texture: sys.texture.clone(), cell });
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Black Ops II: the parsed effect, played by FxElemDef's rules
// ------------------------------------------------------------------------------------------------

/// The texture file the converter writes for an element's image and flipbook
/// (`fx::Converted::texture`), and its grid.
pub fn texture_file(el: &Elem, image: &str) -> (String, crate::fx::Grid) {
    let grid = crate::fx::Grid::of(el.flipbook.frames.min(el.atlas.0.max(1) * el.atlas.1.max(1)));
    (crate::fx::texture_file(image, grid), grid)
}

/// The cell BO2 draws of an element's flipbook (`FxElemAtlas`): the start (`behavior & 3`: the
/// fixed index, a random cell — `random` 0..1 — or the spawn order `index`), then every cell once
/// over the life (bit 2) or `fps` cells a second, held on the last after `loopCount` passes
/// (bit 3).
pub fn bo2_frame(el: &Elem, frames: u32, random: f32, index: u32, age_ms: f32, life_ms: f32) -> u32 {
    use zhbo_formats::fx::Flipbook as F;
    if frames <= 1 {
        return 0;
    }
    let fb = el.flipbook;
    let start = match fb.behavior & F::START_MASK {
        F::START_RANDOM => (random * frames as f32) as u32,
        F::START_INDEXED => index,
        _ => fb.index as u32,
    } % frames;
    let step = if fb.behavior & F::PLAY_OVER_LIFE != 0 {
        ((age_ms / life_ms.max(1.0)).clamp(0.0, 0.9999) * frames as f32) as u32
    } else {
        (age_ms.max(0.0) * fb.fps as f32 / 1000.0) as u32
    };
    let step = if fb.behavior & F::LOOP_N_TIMES != 0 { step.min(fb.loop_count.max(1) as u32 * frames - 1) } else { step };
    (start + step) % frames
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Play `effect` and return what is drawn `t_ms` after it starts, in Zero Hour units with the
/// effect's X (the surface normal) up. `looping_ms` is how long looping elements keep spawning.
pub fn play_bo2(effect: &Effect, t_ms: f32, looping_ms: f32, seed: u64) -> Vec<Sprite> {
    let mut rng = Rng(seed.max(1).wrapping_mul(0x2545F4914F6CDD1D) | 1);
    let mut out = Vec::new();
    play_bo2_at(effect, [0.0; 3], t_ms, looping_ms, &mut rng, &mut out, 3);
    out
}

fn play_bo2_at(effect: &Effect, at: [f32; 3], t_ms: f32, looping_ms: f32, rng: &mut Rng, out: &mut Vec<Sprite>, depth: u32) {
    let u = zhbo_core::units::ZH_PER_INCH;
    for el in &effect.elems {
        if el.emission {
            continue;
        }
        // Spawn times.
        let mut spawns: Vec<f32> = Vec::new();
        if el.looping {
            let loops = if el.count.base > 0.0 { el.count.base as u32 } else { (looping_ms / el.interval_msec.max(1) as f32) as u32 };
            // Only the spawns up to the moment asked for (a fast loop, every msec for a whole
            // trail's life, used to be cut at 2,000 spawns: two seconds).
            let due = (t_ms.max(0.0) / el.interval_msec.max(1) as f32) as u32 + 1;
            for i in 0..loops.min(due) {
                spawns.push(i as f32 * el.interval_msec.max(1) as f32);
            }
        } else {
            let n = el.count.base as i32 + (rng.next() * (el.count.amplitude + 1.0)) as i32;
            for _ in 0..n.max(0) {
                spawns.push(el.delay_msec.base + rng.next() * el.delay_msec.amplitude);
            }
        }
        for (spawn_index, &born) in spawns.iter().enumerate() {
            // One draw from the effect's stream per spawn, so what a spawn looks like does not
            // depend on the moment asked for.
            let mut own = Rng(((rng.next() * 16_777_216.0) as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15) | 1);
            let rng = &mut own;
            let age = t_ms - born;
            let life = el.life_msec.base + rng.next() * el.life_msec.amplitude;
            // The draw at `born` (runners start their effect then, wherever they spawned).
            let origin = [
                el.origin[0].base + rng.next() * el.origin[0].amplitude,
                el.origin[1].base + rng.next() * el.origin[1].amplitude,
                el.origin[2].base + rng.next() * el.origin[2].amplitude,
            ];
            let mut origin = origin;
            let radius = el.offset_radius.base + rng.next() * el.offset_radius.amplitude;
            match el.flags & 0x30 {
                0x10 => {
                    let (z, w) = (rng.range(-1.0, 1.0), rng.range(0.0, std::f32::consts::TAU));
                    let s = (1.0 - z * z).sqrt();
                    origin = [origin[0] + radius * z, origin[1] + radius * s * w.cos(), origin[2] + radius * s * w.sin()];
                }
                0x20 => {
                    let w = rng.range(0.0, std::f32::consts::TAU);
                    let h = el.offset_height.base + rng.next() * el.offset_height.amplitude;
                    origin = [origin[0] + h, origin[1] + radius * w.cos(), origin[2] + radius * w.sin()];
                }
                _ => {}
            }
            if el.kind == kind::RUNNER {
                if age >= 0.0 && depth > 0 {
                    let p = [at[0] + origin[1] * u, at[1] + origin[2] * u, at[2] + origin[0] * u];
                    for c in &el.children {
                        play_bo2_at(c, p, age, looping_ms, rng, out, depth - 1);
                    }
                }
                continue;
            }
            // Dead: the effect it starts where it died (`effectOnDeath`), from the mean path the
            // converter uses (`fx::death_travel`).
            if age >= life && depth > 0 {
                if let Some(death) = &el.on_death {
                    let t = crate::fx::death_travel(el, true);
                    let p = [at[0] + (origin[1] + t[1]) * u, at[1] + (origin[2] + t[2]) * u, at[2] + (origin[0] + t[0]) * u];
                    play_bo2_at(death, p, age - life, looping_ms, rng, out, depth - 1);
                }
            }
            if age < 0.0 || age >= life || !kind::is_sprite(el.kind) || el.vis.is_empty() {
                continue;
            }
            let Some(m) = el.materials.first() else { continue };
            if m.name.contains("distortion") {
                continue;
            }
            let Some(img) = m.color_map() else { continue };
            let tn = (age / life).clamp(0.0, 1.0);
            // The velocity graph's integrated delta (inches per msec of life), base plus random.
            let mut d = [0.0f32; 3];
            if el.vel.len() > 1 {
                let r = [rng.next(), rng.next(), rng.next()];
                let x = tn * (el.vel.len() - 1) as f32;
                let i = (x as usize).min(el.vel.len() - 2);
                let f = x - i as f32;
                for frame_local in [true, false] {
                    let get = |k: usize| {
                        let v = &el.vel[k];
                        let (b, a) = if frame_local { v.local_delta } else { v.world_delta };
                        [b[0] + a[0] * r[0], b[1] + a[1] * r[1], b[2] + a[2] * r[2]]
                    };
                    let p = lerp3(get(i), get(i + 1), f);
                    if frame_local {
                        for k in 0..3 {
                            d[k] += p[k] * life;
                        }
                    } else {
                        // World axes: Z up, mapped back into the effect's frame (X up).
                        d[0] += p[2] * life;
                        d[1] += p[0] * life;
                        d[2] += p[1] * life;
                    }
                }
            }
            let secs = age / 1000.0;
            let grav = -0.5 * 800.0 * (el.gravity.base + rng.next() * el.gravity.amplitude) * secs * secs;
            let local = [origin[0] + d[0] + grav, origin[1] + d[1], origin[2] + d[2]];
            // The visual state graph.
            let (rs, rc) = (rng.next(), rng.next());
            let x = tn * (el.vis.len() - 1) as f32;
            let i = (x as usize).min(el.vis.len().saturating_sub(2));
            let f = if el.vis.len() > 1 { x - i as f32 } else { 0.0 };
            // A stretched sprite (a tail, a line, a non-uniform size) is drawn as the square of its
            // area, as the converter draws it: Zero Hour's particles are square.
            let stretched = matches!(el.kind, kind::TAIL | kind::LINE | kind::TRAIL) || el.flags & 0x1000_0000 != 0;
            let at_k = |k: usize| {
                let v = el.vis[k.min(el.vis.len() - 1)];
                let w = v.size[0] + v.size_amplitude[0] * rs;
                let size = if stretched { (w.max(0.0) * (v.size[1] + v.size_amplitude[1] * rs).max(0.0)).sqrt() } else { w };
                let c = |j: usize| (v.color[j] as f32 + (v.color_amplitude[j] as f32 - v.color[j] as f32) * rc) / 255.0;
                (size, [c(0), c(1), c(2)], c(3))
            };
            let (s0, c0, a0) = at_k(i);
            let (s1, c1, a1) = at_k(i + 1);
            let size = s0 + (s1 - s0) * f;
            let color = lerp3(c0, c1, f);
            let alpha = a0 + (a1 - a0) * f;
            let additive = matches!(m.blend(), Blend::Additive { .. });
            let (texture, grid) = texture_file(el, &img.name);
            let cell = (grid.side, bo2_frame(el, grid.frames, rng.next(), spawn_index as u32, age, life));
            let pos = [at[0] + local[1] * u, at[1] + local[2] * u, at[2] + local[0] * u];
            // BO2's distance ranges from the camera: the spawn range at the spawn point, the fades
            // where the particle is (`fadeInRange` the far end, `fadeOutRange` the near one).
            let spawned_at = [at[0] + origin[1] * u, at[1] + origin[2] * u, at[2] + origin[0] * u];
            let span = |r: zhbo_formats::fx::Range| (r.base.max(0.0) * u, (r.base + r.amplitude).max(0.0) * u);
            if !in_spawn_range(span(el.spawn_range), distance(spawned_at, eye())) {
                continue;
            }
            let fade = distance_fade(span(el.fade_in_range), span(el.fade_out_range), distance(pos, eye()));
            let (color, alpha) = if additive { (color.map(|c| c * fade), alpha) } else { (color, alpha * fade) };
            out.push(Sprite {
                pos,
                size: (2.0 * size * u).max(0.0),
                color,
                alpha,
                additive,
                texture,
                cell,
            });
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Numbers and pictures
// ------------------------------------------------------------------------------------------------

/// What a played effect amounts to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// Σ width² × opacity over particles and time (units² · s).
    pub mass: f32,
    /// The largest Σ width² × opacity at one moment (units²).
    pub peak: f32,
    /// The farthest a visible particle's edge reaches from the effect's origin (units).
    pub reach: f32,
    /// The most particles alive at once.
    pub count: usize,
    /// How long anything is visible (s).
    pub span: f32,
    /// [`Metrics::mass`] and [`Metrics::peak`] of the additive particles alone: the flash and the
    /// fire, which a blast's long smoke would otherwise outweigh.
    pub glow_mass: f32,
    pub glow_peak: f32,
}

/// One moment's totals: covered area (width² × opacity), reach, particles.
#[derive(Clone, Copy, Debug, Default)]
struct Moment {
    area: f32,
    glow: f32,
    reach: f32,
    count: usize,
}

impl Moment {
    fn add(&mut self, s: &Sprite) {
        self.count += 1;
        let o = s.opacity();
        if o < 0.01 || s.size <= 0.0 {
            return;
        }
        self.area += s.size * s.size * o;
        if s.additive {
            self.glow += s.size * s.size * o;
        }
        let r = (s.pos[0] * s.pos[0] + s.pos[1] * s.pos[1] + s.pos[2] * s.pos[2]).sqrt() + s.size * 0.5;
        self.reach = self.reach.max(r);
    }
}

fn fold(moments: &[Moment], dt: f32, t0: f32) -> Metrics {
    let mut m = Metrics::default();
    for (i, x) in moments.iter().enumerate() {
        if x.area > 0.0 {
            m.span = m.span.max(t0 + i as f32 * dt);
        }
        m.mass += x.area * dt;
        m.glow_mass += x.glow * dt;
        m.glow_peak = m.glow_peak.max(x.glow);
        m.peak = m.peak.max(x.area);
        m.reach = m.reach.max(x.reach);
        m.count = m.count.max(x.count);
    }
    m
}

/// Fold one moment's sprites into `m` (`dt` seconds long, at `t`).
pub fn accumulate(m: &mut Metrics, sprites: &[Sprite], t: f32, dt: f32) {
    let mut x = Moment::default();
    sprites.iter().for_each(|s| x.add(s));
    let one = fold(&[x], dt, t);
    m.mass += one.mass;
    m.peak = m.peak.max(one.peak);
    m.reach = m.reach.max(one.reach);
    m.count = m.count.max(one.count);
    m.span = m.span.max(one.span);
    m.glow_mass += one.glow_mass;
    m.glow_peak = m.glow_peak.max(one.glow_peak);
}

/// The metrics of a converted FXList, played frame by frame for `secs`.
pub fn metrics_zh(list: &ZhList, systems: &BTreeMap<String, ZhSystem>, secs: f32) -> Metrics {
    let frames = (secs * 30.0) as u32;
    let mut moments = vec![Moment::default(); frames as usize];
    play_zh_each(list, systems, frames, 7, |f, s| moments[f].add(&s));
    fold(&moments, 1.0 / 30.0, 1.0 / 30.0)
}

/// The metrics of a BO2 effect, sampled every Zero Hour frame for `secs`.
pub fn metrics_bo2(effect: &Effect, looping_ms: f32, secs: f32) -> Metrics {
    let mut m = Metrics::default();
    let frames = (secs * 30.0) as u32;
    // Mid-frame samples: an element shorter than a frame (a muzzle flash's 18 msec) still shows.
    for f in 1..=frames {
        let t = (f as f32 - 0.5) / 30.0;
        let s = play_bo2(effect, t * 1000.0, looping_ms, 7);
        accumulate(&mut m, &s, t, 1.0 / 30.0);
    }
    m
}

/// A camera: eye, target, vertical field of view (degrees).
pub struct Camera {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub fov: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-6);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Draw `sprites` as camera-facing quads over a sky/ground backdrop (`size` pixels square).
/// `textures` holds the converter's textures (decoded) by file name; a missing one draws as a
/// soft disc.
pub fn render(sprites: &[Sprite], textures: &BTreeMap<String, Rgba>, cam: &Camera, size: u32) -> Rgba {
    let (w, h) = (size as usize, size as usize);
    let fwd = norm(sub(cam.target, cam.eye));
    let right = norm(cross(fwd, [0.0, 0.0, 1.0]));
    let up = cross(right, fwd);
    let f = 1.0 / (cam.fov.to_radians() * 0.5).tan();
    let mut px = vec![[0f32; 3]; w * h];
    // Backdrop: the ground (z < 0) darker than the sky.
    for y in 0..h {
        for x in 0..w {
            let nx = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let ny = 1.0 - (y as f32 + 0.5) / h as f32 * 2.0;
            let dir = norm([fwd[0] * f + right[0] * nx + up[0] * ny, fwd[1] * f + right[1] * nx + up[1] * ny, fwd[2] * f + right[2] * nx + up[2] * ny]);
            let ground = dir[2] < 0.0 && cam.eye[2] / -dir[2] < 400.0;
            px[y * w + x] = if ground { [0.22, 0.24, 0.20] } else { [0.36, 0.42, 0.50] };
        }
    }
    // Back to front.
    let mut order: Vec<(f32, &Sprite)> = sprites.iter().map(|s| (dot(sub(s.pos, cam.eye), fwd), s)).filter(|(d, _)| *d > 0.1).collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (depth, s) in order {
        let rel = sub(s.pos, cam.eye);
        let sx = dot(rel, right) / depth * f;
        let sy = dot(rel, up) / depth * f;
        let half = s.size * 0.5 / depth * f;
        let (cx, cy) = ((sx + 1.0) * 0.5 * w as f32, (1.0 - sy) * 0.5 * h as f32);
        let rad = half * 0.5 * w as f32;
        if rad < 0.3 {
            continue;
        }
        let tex = textures.get(&s.texture);
        let (x0, x1) = (((cx - rad).floor().max(0.0)) as usize, ((cx + rad).ceil().min(w as f32)) as usize);
        let (y0, y1) = (((cy - rad).floor().max(0.0)) as usize, ((cy + rad).ceil().min(h as f32)) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                let u = ((x as f32 + 0.5 - (cx - rad)) / (2.0 * rad)).clamp(0.0, 0.999);
                let v = ((y as f32 + 0.5 - (cy - rad)) / (2.0 * rad)).clamp(0.0, 0.999);
                // The particle's cell of a flipbook grid (W3D's frame UV table: row by row).
                let (side, idx) = (s.cell.0.max(1) as f32, s.cell.1 as f32);
                let (u, v) = (((idx % side).floor() + u) / side, ((idx / side).floor() + v) / side);
                let (tc, ta) = match tex {
                    Some(t) if t.width > 0 => {
                        let i = (((v * t.height as f32) as usize) * t.width as usize + (u * t.width as f32) as usize) * 4;
                        let p = &t.pixels[i..i + 4];
                        ([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0], p[3] as f32 / 255.0)
                    }
                    _ => {
                        let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
                        ([1.0; 3], (1.0 - d).max(0.0))
                    }
                };
                let dst = &mut px[y * w + x];
                if s.additive {
                    for k in 0..3 {
                        dst[k] += tc[k] * s.color[k] * s.alpha * if tex.is_some() { 1.0 } else { ta };
                    }
                } else {
                    let a = (ta * s.alpha).clamp(0.0, 1.0);
                    for k in 0..3 {
                        dst[k] = dst[k] * (1.0 - a) + tc[k] * s.color[k] * a;
                    }
                }
            }
        }
    }
    let mut pixels = Vec::with_capacity(w * h * 4);
    for p in px {
        pixels.extend(p.iter().map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8));
        pixels.push(255);
    }
    Rgba { width: size, height: size, pixels }
}

/// The camera the sheets use: a soldier's eye 40 feet from the blast.
pub fn sheet_camera(distance: f32) -> Camera {
    let eye_h = zhbo_core::units::inches(60.0);
    Camera { eye: [0.0, -distance, eye_h], target: [0.0, 0.0, eye_h * 0.5], fov: 65.0 }
}

/// Decode the converter's TGA textures (`Art\Textures\<file>` → bytes) by file name.
pub fn decode_textures(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Rgba> {
    files
        .iter()
        .filter_map(|(k, v)| {
            let name = k.rsplit('\\').next()?.to_string();
            let img = zhbo_native::tga::decode(v).ok()?;
            Some((name, Rgba { width: img.width, height: img.height, pixels: img.rgba }))
        })
        .collect()
}

/// The elements of an effect, its runners' effects included (for counting what converts).
pub fn all_elems(effect: &Effect) -> Vec<&Elem> {
    let mut out = Vec::new();
    for el in &effect.elems {
        out.push(el);
        for c in &el.children {
            out.extend(all_elems(c));
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// The sweep and the contact sheets (`zhbo-assets fx-sheet`)
// ------------------------------------------------------------------------------------------------

/// The effects the sheets show by default: every explosion the impact table and the defs play,
/// the flash-type grenades, the bullet impacts and two muzzle flashes.
pub const SHEET_EFFECTS: &[&str] = &[
    "explosions/fx_grenadeexp_dirt",
    "explosions/fx_grenadeexp_concrete",
    "weapon/rocket/fx_rocket_exp_dirt_mp",
    "weapon/rocket/fx_rocket_exp_concrete_mp",
    "weapon/satchel/fx_explosion_satchel_generic",
    "weapon/bouncing_betty/fx_betty_explosion",
    "explosions/fx_exp_equipment_lg",
    "explosions/fx_exp_equipment",
    "explosions/fx_flashbang",
    "weapon/emp/fx_emp_explosion_equip",
    "weapon/grenade/fx_prox_grenade_exp",
    "impacts/fx_dud_m203",
    "impacts/fx_small_dirt",
    "impacts/fx_large_concrete",
    "impacts/fx_xtreme_dirthit_mp",
    "impacts/fx_flesh_hit_body_nonfatal",
    "weapon/muzzleflashes/fx_muz_ar_flash_1p",
    "weapon/muzzleflashes/fx_muz_ar_flash_3p",
];

/// The moments a sheet shows (msec after the effect starts).
pub const SHEET_TIMES: [f32; 6] = [50.0, 150.0, 300.0, 600.0, 1200.0, 2500.0];

fn csv_row(name: &str, b: &Metrics, z: &Metrics) -> String {
    let r = |a: f32, b: f32| if b > 0.0 { a / b } else { 0.0 };
    format!(
        "{name},{:.2},{:.2},{:.1},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.1},{},{:.2},{:.2},{:.2},{:.3},{:.3},{:.3},{:.3},{:.3}\n",
        b.mass, b.peak, b.reach, b.count, b.span, b.glow_mass, b.glow_peak, z.mass, z.peak, z.reach, z.count, z.span, z.glow_mass, z.glow_peak,
        r(z.mass, b.mass), r(z.peak, b.peak), r(z.reach, b.reach), r(z.glow_mass, b.glow_mass), r(z.glow_peak, b.glow_peak)
    )
}

/// The sweep's CSV header.
pub const CSV_HEADER: &str = "effect,bo2_mass,bo2_peak,bo2_reach,bo2_count,bo2_span,bo2_glow_mass,bo2_glow_peak,zh_mass,zh_peak,zh_reach,zh_count,zh_span,zh_glow_mass,zh_glow_peak,mass_ratio,peak_ratio,reach_ratio,glow_mass_ratio,glow_peak_ratio\n";

/// Convert `effect` alone and return the converter's FXList, systems and decoded textures, plus
/// the textures BO2's own elements draw with (for the BO2 row).
pub fn convert_one(effect: &Effect, pool: &[&zhbo_formats::t6zone::Zone], paks: &dyn Fn(zhbo_formats::ipak::ImageKey) -> Option<Rgba>) -> (ZhList, BTreeMap<String, ZhSystem>, BTreeMap<String, Rgba>) {
    let (list, systems, textures, _) = convert_one_text(effect, pool, paks);
    (list, systems, textures)
}

/// [`convert_one`], with the converter's INI text (the particle systems, then the FXLists).
pub fn convert_one_text(effect: &Effect, pool: &[&zhbo_formats::t6zone::Zone], paks: &dyn Fn(zhbo_formats::ipak::ImageKey) -> Option<Rgba>) -> (ZhList, BTreeMap<String, ZhSystem>, BTreeMap<String, Rgba>, String) {
    let mut c = crate::fx::Converted::with_pool(pool);
    let name = c.add(effect, paks);
    let systems = parse_systems(&c.particles);
    let list = parse_lists(&c.fxlists).remove(&name).unwrap_or_default();
    // The BO2 row draws every element, converted or not: give it each element's first cell.
    for el in all_elems(effect) {
        if let Some(img) = el.materials.first().and_then(|m| m.color_map()) {
            c.preview_texture(&img.name, img.key(), el.atlas, el.flipbook.frames, paks);
        }
    }
    let textures = decode_textures(&c.textures);
    let text = format!("; images missing from the packs: {:?}\n{}{}", c.missing, c.particles, c.fxlists);
    (list, systems, textures, text)
}

/// The metrics of every effect in `zone` (BO2's and the converted one's) as CSV rows, and for
/// each name in `sheets` a contact sheet in `out`: BO2's row over Zero Hour's at [`SHEET_TIMES`].
pub fn sweep(zone: &zhbo_formats::t6zone::Zone, pool: &[&zhbo_formats::t6zone::Zone], paks: &dyn Fn(zhbo_formats::ipak::ImageKey) -> Option<Rgba>, out: &std::path::Path, sheets: &[&str], all: bool) -> Result<String, String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let kind = zhbo_formats::fastfile::asset_type_id("fx").ok_or("no fx asset type")?;
    let mut names: Vec<String> = if all {
        zone.assets.iter().filter(|a| a.kind == kind).map(|a| zone.str_ptr(a.addr).trim_start_matches(',').to_string()).collect()
    } else {
        Vec::new()
    };
    names.extend(sheets.iter().map(|s| s.to_string()));
    names.sort();
    names.dedup();
    let mut csv = String::from(CSV_HEADER);
    let cam = sheet_camera(zhbo_core::units::inches(480.0));
    for name in &names {
        let Some(mut effect) = zhbo_formats::fx::find(zone, name) else { continue };
        // BO2 draws the full material a stub names; so does its row here.
        zhbo_formats::fx::bind_materials(&mut effect, pool);
        let looping_ms = (effect.looping_life_msec as f32).clamp(1000.0, 20000.0);
        let (list, systems, textures, text) = convert_one_text(&effect, pool, paks);
        let secs = 6.0;
        let b = metrics_bo2(&effect, looping_ms, secs);
        let z = metrics_zh(&list, &systems, secs);
        csv.push_str(&csv_row(name, &b, &z));
        if sheets.contains(&name.as_str()) {
            let cell = 192;
            let mut cells = Vec::new();
            for &t in &SHEET_TIMES {
                cells.push(render(&play_bo2(&effect, t, looping_ms, 7), &textures, &cam, cell));
            }
            for &t in &SHEET_TIMES {
                let frames = (t / MS_PER_FRAME).round().max(1.0) as u32;
                cells.push(render(&play_zh(&list, &systems, frames, 7), &textures, &cam, cell));
            }
            let sheet = crate::images::sheet(&cells, cell, SHEET_TIMES.len() as u32);
            let file = out.join(format!("{}.png", name.replace('/', "__")));
            crate::images::write_png(&file, &sheet)?;
            std::fs::write(file.with_extension("ini"), &text).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(out.join("metrics.csv"), &csv).map_err(|e| e.to_string())?;
    Ok(csv)
}

/// [`sweep`] over the player's install: `common_mp`'s effects.
pub fn sweep_install(bo2_dir: &std::path::Path, out: &std::path::Path, sheets: &[&str], all: bool) -> Result<String, String> {
    let zone = crate::paks::load_zone(bo2_dir, "common_mp")?;
    let patch = crate::paks::load_zone(bo2_dir, "patch_mp")?;
    // As the archive build resolves them: stubs by name from the weapon pool's zones.
    let paks = crate::paks::Paks::open_for(bo2_dir, &[&patch, &zone])?;
    sweep(&zone, &[&patch, &zone], &|k| paks.image(k), out, sheets, all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flipbook_advances_as_the_engine_and_bo2_play_it() {
        // Host patch 0028's Particle::getFrame: over the life, at a rate, held after its loops.
        let over = ZhSystem { frame_grid: 8, frame_count: 32, frame_over_life: true, ..Default::default() };
        assert_eq!((over.frame(0, 0, 40), over.frame(0, 20, 40), over.frame(0, 39, 40)), (0, 16, 31));
        let rate = ZhSystem { frame_grid: 4, frame_count: 16, frame_start: 3, frame_rate: 15.0, frame_loops: 1, ..Default::default() };
        // 15 cells a second at 30 updates a second: one every other update, from cell 3.
        assert_eq!((rate.frame(0, 0, 90), rate.frame(0, 4, 90)), (3, 5));
        // One pass then held on the cell before the start.
        assert_eq!(rate.frame(0, 89, 90), 2);
        let random = ZhSystem { frame_grid: 2, frame_count: 2, frame_start_type: "RANDOM".into(), ..Default::default() };
        let cells: std::collections::BTreeSet<u32> = (0..16).map(|p| random.frame(p, 0, 10)).collect();
        assert_eq!(cells.len(), 2, "both cells are used");
        assert_eq!(ZhSystem::default().frame(5, 5, 10), 0);
        // BO2's side of the same rules.
        use zhbo_formats::fx::Flipbook;
        let el = Elem { flipbook: Flipbook { behavior: 4, frames: 32, ..Default::default() }, ..Default::default() };
        assert_eq!((bo2_frame(&el, 32, 0.0, 0, 0.0, 1000.0), bo2_frame(&el, 32, 0.0, 0, 500.0, 1000.0)), (0, 16));
        let el = Elem { flipbook: Flipbook { behavior: 2, fps: 10, frames: 4, ..Default::default() }, ..Default::default() };
        assert_eq!(bo2_frame(&el, 4, 0.0, 3, 250.0, 1000.0), 1);
    }

    #[test]
    fn a_converted_system_plays_by_zero_hours_rules() {
        let ini = "ParticleSystem T\n  Shader = ALPHA\n  ParticleName = t.tga\n  Lifetime = 10 10\n  SystemLifetime = 1\n  Size = 2.00 2.00\n  SizeRate = 0.5 0.5\n  Alpha1 = 1.00 1.00 0\n  Alpha2 = 0.00 0.00 10\n  BurstDelay = 0 0\n  BurstCount = 3 3\n  InitialDelay = 0 0\n  VelOrthoX = 0 0\n  VelOrthoY = 0 0\n  VelOrthoZ = 1 1\n  VelocityDamping = 1 1\n  Gravity = 0\n  VolumeType = POINT\nEnd\n";
        let systems = parse_systems(ini);
        let lists = parse_lists("FXList L\n  ParticleSystem\n    Name = T\n  End\nEnd\n");
        let s = play_zh(&lists["L"], &systems, 5, 1);
        assert_eq!(s.len(), 3);
        assert!((s[0].pos[2] - 5.0).abs() < 1e-4, "{:?}", s[0].pos);
        assert!((s[0].size - 4.5).abs() < 1e-4, "{}", s[0].size);
        assert!((s[0].alpha - 0.5).abs() < 1e-4, "{}", s[0].alpha);
        // Dead after its ten frames.
        assert!(play_zh(&lists["L"], &systems, 11, 1).is_empty());
        let m = metrics_zh(&lists["L"], &systems, 1.0);
        assert_eq!(m.count, 3);
        assert!(m.mass > 0.0 && m.reach > 5.0);
    }
}

/// The whole install's effects against their conversion: the regression gate for the causes that
/// made BO2's explosions small in Zero Hour (runners never resolved, stub images and materials
/// unresolved, one-frame particles never drawn, looping and one-shot counts misread).
#[cfg(test)]
mod install_tests {
    use super::*;
    use zhbo_formats::fx::kind;

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_bo2_effect_converts_to_its_size() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let common = crate::paks::load_zone(&bo2_dir, "common_mp").unwrap();
        let patch = crate::paks::load_zone(&bo2_dir, "patch_mp").unwrap();
        let pool = [&patch, &common];
        let paks = crate::paks::Paks::open_for(&bo2_dir, &pool).unwrap();
        let kind_fx = zhbo_formats::fastfile::asset_type_id("fx").unwrap();
        let names: Vec<String> = common.assets.iter().filter(|a| a.kind == kind_fx).map(|a| common.str_ptr(a.addr).trim_start_matches(',').to_string()).collect();
        let (mut runners, mut within, mut measured) = (0, 0, 0);
        let mut failures = Vec::new();
        for name in &names {
            let Some(mut e) = zhbo_formats::fx::find(&common, name) else { continue };
            // Every runner starts the effect it names when the zone defines it (the reader once
            // dereferenced the name twice and dropped every runner's effect).
            for el in all_elems(&e).into_iter().filter(|el| el.kind == kind::RUNNER) {
                let reachable = el.runs.iter().filter(|n| n.contains('/') && zhbo_formats::fx::find(&common, n).is_some()).count();
                runners += 1;
                if el.children.len() != reachable {
                    failures.push(format!("{name}: runner of {:?} started {} effects", el.runs, el.children.len()));
                }
            }
            zhbo_formats::fx::bind_materials(&mut e, &pool);
            let (list, systems, _, text) = convert_one_text(&e, &pool, &|k| paks.image(k));
            if !text.starts_with("; images missing from the packs: {}") {
                failures.push(format!("{name}: {}", text.lines().next().unwrap_or_default()));
            }
            let looping_ms = (e.looping_life_msec as f32).clamp(1000.0, 20000.0);
            let b = metrics_bo2(&e, looping_ms, 6.0);
            if b.mass <= 0.0 {
                continue;
            }
            let z = metrics_zh(&list, &systems, 6.0);
            measured += 1;
            let ratio = z.mass / b.mass;
            if (0.5..=2.0).contains(&ratio) {
                within += 1;
            }
            // The explosions, flashes and impacts the mod plays stay near BO2's size: not a fraction
            // (the missing flash and fire) nor a multiple (the Betty's looping parts emitting a
            // whole count every frame for a second covered 8.6x). The band is wide because the
            // metric weighs a long thin smoke like a short fireball.
            if SHEET_EFFECTS.contains(&name.as_str()) && !(0.4..=4.0).contains(&ratio) {
                failures.push(format!("{name}: covers {ratio:.2} of BO2's (mass {:.0} vs {:.0})", z.mass, b.mass));
            }
        }
        assert!(runners > 100, "{runners} runners: the install's effects did not parse");
        // Measured 2026-11-03: 305 of 371 (82 %); before the fixes 112 (30 %), 81 effects drawing
        // nothing at all. Then (fx-flipbook) 351 of 367 (96 %): looping and counted runners repeat
        // their effect, the size is fitted to BO2's whole graph, sub-frame particles show for their
        // share of the frame, and the distance ranges are kept (four effects show nothing at the
        // sheets' 40 feet in BO2 either).
        assert!(within * 100 >= measured * 95, "only {within} of {measured} effects cover 0.5-2x BO2's");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
