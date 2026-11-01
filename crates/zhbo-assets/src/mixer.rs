//! Black Ops II's mixer, evaluated at conversion: what level each alias of a sound comes out of
//! BO2's speakers at, so a converted AudioEvent carries BO2's relative weight (the scope and the
//! Ghidra trail: MODLOG "BO2's mixer levels every converted sound").
//!
//! Per voice (t6mp sound thread): `gain = volume × Π group attenuationMp (up the parents,
//! FUN_0082b9f0) × distance curve (FUN_0082acf0 → FUN_0083d060)`, each layer of a secondary chain
//! its own voice (FUN_0082c470 starts the secondary with the same call) after its own `startDelay`.
//! Per bus (the DSP, FUN_006f5310, with the snapshot's gains from FUN_0082fc60):
//!
//! ```text
//! fx = COMP(fx + reverb)          (+ voice / hdrfx first when voiceCompE / hdrfxCompE)
//! fx = EQ(fx + hdrfx)             (HDR joins after the compressor: hdrfxCompE = 0 in every snapshot)
//! out = LIMIT(fx + voice + ui) + music + movie + reference
//! ```
//!
//! COMP and LIMIT are one kernel (FUN_00834623): sidechain `PG·x` highpassed (`y = x − x₋₁ + 0.995·y₋₁`),
//! peak envelope `e += a·max(|y| − e, 0) − r·e`, gain `max(e, T)^(1/R − 1)`, output `y·gain·MG·0.5`.
//! Here it is evaluated statically for the sound alone: the gain at the envelope's peak over the
//! sound (in game the envelope is shared with everything else playing, so the per-sample pumping of
//! a lone sound is not BO2's either). The master EQ (FUN_006f5040) is `eqG` then four RBJ biquads
//! (low shelf, two peaks, high shelf: FUN_00833e50/00833fc2/00833ce8) on what has passed the
//! compressor and the HDR bus. The reverb send, futz and the voice side-chain duck are not modelled.

use zhbo_formats::sndalias::Variant;
use zhbo_formats::sndglobals::{Band, BandKind, Bus, DriverGlobals, Dynamics, Master};

/// The DSP's sample rate (`DAT_00b81514`): the kernel's time constants are in samples at it.
const DSP_RATE: f32 = 48000.0;
/// The kernel's sidechain highpass (`DAT_00b93390`).
const HIGHPASS: f32 = 0.995;
/// The kernel's output scale with the make-up gain (`DAT_00c733cc`).
const KERNEL_OUTPUT: f32 = 0.5;
/// The 5.1 → stereo fold of a rear or centre channel into a side (-3 dB; the LFE channel is not
/// folded, as XAudio2's default matrix).
const FOLD: f32 = std::f32::consts::FRAC_1_SQRT_2;

pub struct Mixer<'a> {
    pub globals: &'a DriverGlobals,
    pub master: &'a Master,
    /// The sample rate the EQ's filters are designed at (the rendered sound's).
    pub rate: f32,
}

/// One layer to render: interleaved samples (−1..1) at the event's rate and channel count.
pub struct Layer<'a> {
    pub samples: &'a [f32],
    pub start_frame: usize,
    pub bus: Bus,
    pub gain: f32,
}

impl<'a> Mixer<'a> {
    pub fn new(globals: &'a DriverGlobals) -> Option<Mixer<'a>> {
        Some(Mixer { globals, master: globals.default_master()?, rate: DSP_RATE })
    }

    pub fn bus(&self, v: &Variant) -> Bus {
        Bus::from_index(v.mix.bus).unwrap_or(Bus::Fx)
    }

    /// How much of the alias reaches a stereo pair: a 2D alias plays its pan's per-speaker gains,
    /// folded (front + 0.707 × (back + centre), the sides averaged); a 3D alias is panned by its
    /// position and keeps unit gain — unless its pan sends it to the LFE channel only (the
    /// `wpn_*_lfe` layers), which stereo does not carry.
    pub fn stereo_fold(&self, v: &Variant) -> f32 {
        let Some(p) = self.globals.pan(v.mix.pan) else { return 1.0 };
        let speakers = p.front + FOLD * (p.back + p.center);
        if speakers <= 0.0 {
            return 0.0;
        }
        if v.mix.positional {
            return 1.0;
        }
        speakers * 0.5 * (p.left + p.right)
    }

    /// The dry distance gain at `d` inches (FUN_0083d060): inside `distMin` the min curve at
    /// `d / distMin`, beyond it the falloff curve over `distMin..distMax`. A 2D alias is 1.
    pub fn distance_gain(&self, v: &Variant, d: f32) -> f32 {
        if !v.mix.positional {
            return 1.0;
        }
        let (min, max) = v.distance;
        let eval = |c: u8, t: f32| self.globals.curve(c).map_or(1.0, |c| c.eval(t));
        if d < min {
            eval(v.mix.min_falloff_curve, d / min)
        } else {
            let t = if max > min { (d - min) / (max - min) } else { 1.0 };
            eval(v.mix.falloff_curve, t)
        }
    }

    /// The linear gain of the variant into its bus at distance `d`: volume × group chain × bus ×
    /// stereo fold × distance curve.
    pub fn gain(&self, v: &Variant, d: f32) -> f32 {
        v.volume.1 * self.globals.group_gain(v.mix.group as usize) * self.master.bus(self.bus(v)) * self.stereo_fold(v) * self.distance_gain(v, d)
    }

    /// The farthest distance any of `variants` is heard at (its `distMax`; 0 for 2D only).
    pub fn audible_range(&self, variants: &[&Variant]) -> f32 {
        variants.iter().filter(|v| v.mix.positional).map(|v| v.distance.1.max(v.distance.0)).fold(0.0, f32::max)
    }

    /// The chain's loudness against distance: the sum of every layer's gain at `d`.
    pub fn chain_gain(&self, variants: &[&Variant], d: f32) -> f32 {
        variants.iter().map(|v| self.gain(v, d)).sum()
    }

    /// Mix the layers through BO2's buses and master. Returns interleaved output samples.
    pub fn render(&self, layers: &[Layer], channels: usize) -> Vec<f32> {
        let len = layers.iter().map(|l| l.start_frame * channels + l.samples.len()).max().unwrap_or(0);
        let mut bus = vec![Vec::new(); Bus::ALL.len()];
        for l in layers {
            let b = &mut bus[l.bus as usize];
            if b.is_empty() {
                b.resize(len, 0.0);
            }
            for (o, s) in b[l.start_frame * channels..].iter_mut().zip(l.samples) {
                *o += s * l.gain;
            }
        }
        let take = |bus: &mut Vec<Vec<f32>>, b: Bus| std::mem::take(&mut bus[b as usize]);
        let add = |into: &mut Vec<f32>, from: Vec<f32>| {
            if into.is_empty() {
                *into = from;
            } else {
                for (a, b) in into.iter_mut().zip(from) {
                    *a += b;
                }
            }
        };
        let m = self.master;
        let mut fx = take(&mut bus, Bus::Fx);
        add(&mut fx, take(&mut bus, Bus::Reverb));
        let voice = take(&mut bus, Bus::Voice);
        let hdr = take(&mut bus, Bus::Hdrfx);
        let (mut voice, mut hdr) = (Some(voice), Some(hdr));
        if m.voice_compressed {
            add(&mut fx, voice.take().unwrap_or_default());
        }
        if m.hdrfx_compressed {
            add(&mut fx, hdr.take().unwrap_or_default());
        }
        dynamics(&m.compressor, &mut fx, channels);
        if m.voice_eq {
            add(&mut fx, voice.take().unwrap_or_default());
        }
        add(&mut fx, hdr.take().unwrap_or_default());
        equalize(m, &mut fx, channels, self.rate);
        add(&mut fx, voice.take().unwrap_or_default());
        add(&mut fx, take(&mut bus, Bus::Ui));
        add(&mut fx, take(&mut bus, Bus::Pfutz));
        dynamics(&m.limiter, &mut fx, channels);
        for b in [Bus::Music, Bus::Movie, Bus::Reference] {
            add(&mut fx, take(&mut bus, b));
        }
        fx.resize(len, 0.0);
        fx
    }
}

/// The RBJ-cookbook biquad of one EQ band at `rate`, normalized: `[b0, b1, b2, a1, a2]`.
pub fn biquad(b: &Band, rate: f32) -> [f32; 5] {
    let a = 10f32.powf(b.gain_db / 40.0);
    let w = std::f32::consts::PI * (2.0 * b.freq / rate).clamp(1e-6, 1.0);
    let (sin, cos) = w.sin_cos();
    let alpha = sin / (2.0 * b.q.max(1e-3));
    let sq = 2.0 * a.sqrt() * alpha;
    let (b0, b1, b2, a0, a1, a2) = match b.kind {
        BandKind::Peak => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
        BandKind::LowShelf => (
            a * ((a + 1.0) - (a - 1.0) * cos + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - sq),
            (a + 1.0) + (a - 1.0) * cos + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - sq,
        ),
        BandKind::HighShelf => (
            a * ((a + 1.0) + (a - 1.0) * cos + sq),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - sq),
            (a + 1.0) - (a - 1.0) * cos + sq,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - sq,
        ),
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// The master EQ on `x` (interleaved): `eqG`, then each enabled band per channel.
fn equalize(m: &Master, x: &mut [f32], channels: usize, rate: f32) {
    if x.is_empty() {
        return;
    }
    for s in x.iter_mut() {
        *s *= m.eq_gain;
    }
    let ch = channels.max(1);
    for band in m.eq.iter().filter(|b| b.enabled) {
        let [b0, b1, b2, a1, a2] = biquad(band, rate);
        for c in 0..ch {
            let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for s in x.iter_mut().skip(c).step_by(ch) {
                let y = b0 * *s + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                (x2, x1, y2, y1) = (x1, *s, y1, y);
                *s = y;
            }
        }
    }
}

/// The kernel's smoothing coefficient for a time constant (FUN_008345d8):
/// `(sqrt(4N² + 1) − 1) / (2N²)`, N = seconds × 48000 (floored at 1e-6 s).
pub fn coefficient(seconds: f32) -> f32 {
    let n = seconds.max(1e-6) * DSP_RATE;
    let n2 = n * n;
    ((4.0 * n2 + 1.0).sqrt() - 1.0) / (2.0 * n2)
}

/// The kernel's static gain for envelope `e`: `max(e, T)^(1/R − 1) · MG · 0.5`, times the
/// sidechain's pre-gain (the output is the pre-gained signal).
pub fn static_gain(d: &Dynamics, e: f32) -> f32 {
    e.max(d.threshold).powf(1.0 / d.ratio - 1.0) * d.makeup_gain * KERNEL_OUTPUT * d.pre_gain
}

/// Apply one dynamics stage to `x` (interleaved, `channels`): highpass, then the static gain at
/// the peak of the kernel's own envelope over the sound.
fn dynamics(d: &Dynamics, x: &mut [f32], channels: usize) {
    if !d.enabled || x.is_empty() {
        return;
    }
    let (a, r) = (coefficient(d.attack), coefficient(d.release));
    let mut peak = 0.0f32;
    for c in 0..channels.max(1) {
        let (mut prev, mut y, mut e) = (0.0f32, 0.0f32, 0.0f32);
        for s in x.iter_mut().skip(c).step_by(channels.max(1)) {
            y = *s - prev + y * HIGHPASS;
            prev = *s;
            *s = y;
            let level = (y * d.pre_gain).abs();
            e += a * (level - e).max(0.0) - r * e;
            peak = peak.max(e);
        }
    }
    let g = static_gain(d, peak);
    for s in x.iter_mut() {
        *s *= g;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhbo_formats::sndalias::Mix;
    use zhbo_formats::sndglobals::{Curve, Group, Pan};

    fn dyn_(pg: f32, mg: f32, t: f32, r: f32) -> Dynamics {
        Dynamics { enabled: true, pre_gain: pg, makeup_gain: mg, threshold: t, ratio: r, attack: 0.0117, release: 0.2118 }
    }

    /// BO2's tables, as the install has them (the OAT dump's values): enough to level the three
    /// reference aliases.
    fn globals() -> DriverGlobals {
        let g = |name: &str, parent: Option<usize>, a: u16| Group { name: name.into(), parent, attenuation_mp: a as f32 / 65535.0, attenuation_sp: 0.0 };
        let flat = |name: &str, y: f32| Curve { name: name.into(), points: [(0.0, y), (1.0, y), (1.0, y), (1.0, y), (1.0, y), (1.0, y), (1.0, y), (1.0, 0.0)] };
        let pan = |name: &str, front: f32, back: f32, lfe: f32| Pan { name: name.into(), front, back, center: 0.0, lfe, left: 1.0, right: 1.0 };
        DriverGlobals {
            name: "default".into(),
            groups: vec![g("grp_reference", None, 65535), g("grp_master", None, 36853), g("grp_weapon", Some(1), 36853), g("grp_voice", Some(1), 65535), g("grp_foley", Some(1), 36853), g("grp_wpn_lfe", Some(0), 52056)],
            curves: vec![
                Curve { name: "default".into(), points: [(0.0, 1.0), (0.142857, 0.630957), (0.285714, 0.398107), (0.428571, 0.251189), (0.571429, 0.177828), (0.714286, 0.112202), (0.857143, 0.070795), (1.0, 0.0)] },
                flat("allon", 1.0),
                flat("alloff", 0.0),
            ],
            pans: vec![pan("default", 1.0, 0.0, 0.0), pan("front_mostly", 1.0, 0.281838, 0.0), pan("lfe", 0.0, 0.0, 1.0)],
            masters: vec![Master {
                name: "default".into(),
                eq: [
                    Band { kind: BandKind::LowShelf, enabled: true, gain_db: 5.0, freq: 132.3088, q: 1.168478 },
                    Band { kind: BandKind::Peak, enabled: true, gain_db: 5.0, freq: 357.8922, q: 0.221001 },
                    Band { kind: BandKind::Peak, enabled: true, gain_db: -8.0, freq: 2772.543, q: 9.795492 },
                    Band { kind: BandKind::HighShelf, enabled: true, gain_db: 3.0, freq: 4621.938, q: 1.754375 },
                ],
                eq_gain: 1.0,
                compressor: dyn_(3.431997, 0.26298, 0.013275, 22.36621),
                limiter: dyn_(1.104993, 0.816788, 0.066117, 1.290039),
                bus_gain: [0.202214, 0.152477, 5.615831, 0.642844, 2.88864, 2.735035, 2.258948, 1.0, 1.0],
                hdrfx_compressed: false,
                voice_eq: false,
                voice_compressed: false,
            }],
        }
    }

    fn db(dbspl: f32) -> f32 {
        10f32.powf((dbspl - 100.0) / 20.0)
    }

    fn variant(vol_dbspl: f32, bus: u8, group: u8, pan: u8, positional: bool) -> Variant {
        Variant {
            asset_id: 1,
            file: String::new(),
            volume: (db(vol_dbspl), db(vol_dbspl)),
            pitch: (1.0, 1.0),
            distance: (0.0, 10000.0),
            start_delay_ms: 0,
            secondary: None,
            mix: Mix { bus, group, pan, positional, falloff_curve: 0, min_falloff_curve: 1 },
        }
    }

    /// The three reference aliases' BO2 gains into the master: the AN-94's shot (`93` dBSPL, fx bus,
    /// `grp_weapon`, `front_mostly`), the sniper hold-breath line (`75`, voice bus, `grp_voice`) and
    /// a footstep's foley (`86`, fx bus, `grp_foley`). Volume alone ranks the shot 18 dB over the
    /// breath; BO2's groups and buses put the breath 18 dB over the shot.
    #[test]
    fn groups_and_buses_level_the_reference_aliases() {
        let dg = globals();
        let m = Mixer::new(&dg).unwrap();
        let shot = m.gain(&variant(93.0, 1, 2, 1, false), 0.0);
        let breath = m.gain(&variant(75.0, 2, 3, 0, false), 0.0);
        let step = m.gain(&variant(86.0, 1, 4, 0, false), 0.0);
        // 0.447 × 0.316 × 0.152 × 1.2.
        assert!((shot - 0.447 * 0.3162 * 0.152477 * (1.0 + FOLD * 0.281838)).abs() < 1e-3, "{shot}");
        // 0.0562 × 0.562 × 5.616.
        assert!((breath - 0.05623 * 0.5623 * 5.615831).abs() < 1e-3, "{breath}");
        assert!(breath > 6.0 * shot, "breath {breath} shot {shot}");
        assert!((shot / step - 1.2 * db(93.0) / db(86.0)).abs() < 0.05, "same bus and group depth: only volume and pan differ");
        // The LFE layer sends to the LFE channel only: stereo does not carry it.
        assert_eq!(m.gain(&variant(86.0, 4, 5, 2, false), 0.0), 0.0);
    }

    /// The compressor squashes the fx bus (ratio 22: a ten-fold louder shot comes out barely
    /// louder) and makes quiet fx up by ~×28; the HDR and voice buses join after it untouched.
    #[test]
    fn the_compressor_squashes_fx_but_not_hdr_or_voice() {
        let dg = globals();
        let m = Mixer::new(&dg).unwrap();
        let tone = |amp: f32| (0..9600).flat_map(|i| [amp * (i as f32 * 0.05).sin(); 1]).collect::<Vec<f32>>();
        let level = |bus: Bus, amp: f32| {
            let s = tone(amp);
            let out = m.render(&[Layer { samples: &s, start_frame: 0, bus, gain: 1.0 }], 1);
            out[4800..].iter().fold(0.0f32, |a, s| a.max(s.abs()))
        };
        let loud = level(Bus::Fx, 0.2);
        let soft = level(Bus::Fx, 0.02);
        assert!(loud / soft < 1.3, "ratio 22 above threshold: {loud} vs {soft}");
        let quiet = level(Bus::Fx, 0.0005);
        assert!(quiet / 0.0005 > 15.0, "below threshold the kernel's constant make-up: {}", quiet / 0.0005);
        let hdr_loud = level(Bus::Hdrfx, 0.2);
        let hdr_soft = level(Bus::Hdrfx, 0.02);
        assert!(hdr_loud / hdr_soft > 5.0, "the HDR bus skips the compressor: {hdr_loud} vs {hdr_soft}");
    }

    /// Start delays shift a layer; a 3D alias follows its curves.
    #[test]
    fn layers_start_late_and_fall_off() {
        let dg = globals();
        let m = Mixer::new(&dg).unwrap();
        let one = [1.0f32; 4];
        let out = m.render(&[Layer { samples: &one, start_frame: 6, bus: Bus::Reference, gain: 1.0 }], 1);
        assert_eq!(out.len(), 10);
        assert!(out[..6].iter().all(|&s| s == 0.0) && out[6..].iter().all(|&s| s == 1.0));
        let mut v = variant(100.0, 8, 0, 0, true);
        v.distance = (100.0, 1100.0);
        assert_eq!(m.distance_gain(&v, 50.0), 1.0, "allon inside distMin");
        assert!((m.distance_gain(&v, 100.0 + 1000.0 * 0.142857) - 0.630957).abs() < 1e-3);
        assert_eq!(m.distance_gain(&v, 5000.0), 0.0);
        assert_eq!(m.audible_range(&[&v]), 1100.0);
    }

    /// The master EQ shapes what passes the compressor (the 132 Hz shelf lifts the lows, the
    /// 2.8 kHz notch cuts) and leaves the voice bus alone.
    #[test]
    fn the_master_eq_shapes_fx_but_not_voice() {
        let dg = globals();
        let mut flat_dg = globals();
        flat_dg.masters[0].eq.iter_mut().for_each(|b| b.enabled = false);
        let amp = |dg: &DriverGlobals, hz: f32, bus: Bus| {
            let m = Mixer::new(dg).unwrap();
            let s: Vec<f32> = (0..48000).map(|i| 0.05 * (std::f32::consts::TAU * hz * i as f32 / 48000.0).sin()).collect();
            let out = m.render(&[Layer { samples: &s, start_frame: 0, bus, gain: 1.0 }], 1);
            out[24000..].iter().fold(0.0f32, |a, s| a.max(s.abs()))
        };
        let db = |hz: f32, bus: Bus| 20.0 * (amp(&dg, hz, bus) / amp(&flat_dg, hz, bus)).log10();
        assert!(db(60.0, Bus::Hdrfx) > 4.0, "the 132 Hz shelf lifts the lows: {}", db(60.0, Bus::Hdrfx));
        assert!(db(2772.5, Bus::Hdrfx) < -6.0, "the 2.8 kHz peak cuts: {}", db(2772.5, Bus::Hdrfx));
        assert!(db(60.0, Bus::Voice).abs() < 0.01 && db(2772.5, Bus::Voice).abs() < 0.01, "voice skips the EQ");
        // A 0 dB band is flat.
        let flat = biquad(&Band { kind: BandKind::Peak, enabled: true, gain_db: 0.0, freq: 1000.0, q: 1.0 }, 48000.0);
        assert!((flat[0] - 1.0).abs() < 1e-6 && (flat[1] - flat[3]).abs() < 1e-6 && (flat[2] - flat[4]).abs() < 1e-6);
    }

    #[test]
    fn kernel_coefficients() {
        // ~1/N for a long time constant.
        assert!((coefficient(0.2118) - 1.0 / (0.2118 * 48000.0)).abs() < 1e-6);
        let c = dyn_(3.431997, 0.26298, 0.013275, 22.36621);
        // Continuous at the threshold, constant below it.
        assert_eq!(static_gain(&c, 0.001), static_gain(&c, c.threshold));
        assert!(static_gain(&c, 0.1) < static_gain(&c, 0.02));
    }
}
