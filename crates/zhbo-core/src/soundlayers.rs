//! Third-person shots as BO2 plays them: every layer of the alias's secondary chain is its own
//! voice whose dry level follows its own distance curves (t6mp `FUN_0082c470` starts each layer,
//! `FUN_0082acf0` → `FUN_0083d060` levels it). The AN-94's close shot fades out by 1,800" while
//! its `_npc_dist` layer fades in from silence at the source and carries to 12,000".
//!
//! The converter (`zhbo_assets::sfx`) renders each layer as its own AudioEvent with no Zero Hour
//! falloff and writes the layer rows to [`crate::names::SOUND_LAYERS_PATH`]; the bridge passes the
//! file to `zhbo_load_sound_layers`. The client plays another player's shot as its layers, each at
//! the gain [`gain`] gives at the listener's distance (`ClientHost::sound_gain`). This module owns
//! the distance rule; the converter's mixer evaluates its curves through it too.
//!
//! Row: `layer <FX weapon> <chain event> <layer event> <distMin> <distMax> <min curve x0 y0 … x7 y7>
//! <curve …>`, distances in Zero Hour units, keyed by the FX weapon the sim fires for that gun
//! (`WeaponDef::fx_weapon`, the suppressor's swapped in). A 2D layer is written flat (gain 1).

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// An 8-point BO2 `SndCurve` (x rising 0..1).
pub type Curve = [(f32, f32); 8];

/// Piecewise-linear value of `curve` at `t` (clamped to 0..1): the first segment containing `t`.
pub fn curve_eval(curve: &Curve, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    for w in curve.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if t <= x1 {
            if x1 <= x0 {
                return y1;
            }
            return y0 + (y1 - y0) * ((t - x0) / (x1 - x0)).clamp(0.0, 1.0);
        }
    }
    curve[7].1
}

/// BO2's dry distance gain (`FUN_0083d060`): inside `min` the min curve at `d / min`, beyond it
/// the falloff curve over `min..max`.
pub fn distance_gain(min: f32, max: f32, min_curve: &Curve, curve: &Curve, d: f32) -> f32 {
    if d < min {
        curve_eval(min_curve, d / min)
    } else {
        let t = if max > min { (d - min) / (max - min) } else { 1.0 };
        curve_eval(curve, t)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub event: &'static str,
    pub min: f32,
    pub max: f32,
    pub min_curve: Curve,
    pub curve: Curve,
}

impl Layer {
    pub fn gain(&self, d: f32) -> f32 {
        distance_gain(self.min, self.max, &self.min_curve, &self.curve, d)
    }
}

/// A gun's third-person shot: its chain's event (the converted close mix) and the layers.
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub chain: &'static str,
    pub layers: Vec<Layer>,
}

/// A curve that is 1 everywhere: a 2D layer's row.
pub const FLAT: Curve = [(0.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0)];

static LAYERS: OnceLock<RwLock<HashMap<String, Shot>>> = OnceLock::new();

fn layers() -> &'static RwLock<HashMap<String, Shot>> {
    LAYERS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn parse_curve(it: &mut dyn Iterator<Item = &str>) -> Option<Curve> {
    let mut c = [(0.0, 0.0); 8];
    for p in c.iter_mut() {
        *p = (it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    }
    Some(c)
}

/// Parse layer rows (malformed rows are skipped).
pub fn parse(text: &str) -> HashMap<String, Shot> {
    let mut out: HashMap<String, Shot> = HashMap::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() != Some("layer") {
            continue;
        }
        let row = (|| {
            let fx = it.next()?.to_string();
            let chain = it.next()?.to_string();
            let event = it.next()?.to_string();
            let min = it.next()?.parse().ok()?;
            let max = it.next()?.parse().ok()?;
            let min_curve = parse_curve(&mut it)?;
            let curve = parse_curve(&mut it)?;
            Some((fx, chain, Layer { event: Box::leak(event.into_boxed_str()), min, max, min_curve, curve }))
        })();
        if let Some((fx, chain, layer)) = row {
            out.entry(fx).or_insert_with(|| Shot { chain: Box::leak(chain.into_boxed_str()), layers: Vec::new() }).layers.push(layer);
        }
    }
    out
}

/// Install the converted rows (replacing any earlier ones). Returns the number of layers.
pub fn install(text: &str) -> usize {
    let table = parse(text);
    let n = table.values().map(|s| s.layers.len()).sum();
    *layers().write().unwrap_or_else(|e| e.into_inner()) = table;
    n
}

/// One row as the converter writes it.
pub fn format_row(fx: &str, chain: &str, l: &Layer) -> String {
    let pts = |c: &Curve| c.iter().map(|(x, y)| format!("{x} {y}")).collect::<Vec<_>>().join(" ");
    format!("layer {fx} {chain} {} {} {} {} {}\n", l.event, l.min, l.max, pts(&l.min_curve), pts(&l.curve))
}

/// What another player's shot through `fx` sounds like at listener distance `d` (Zero Hour
/// units; `None` with no listener): `Ok` the audible layers and their gains, `Err` the chain's
/// close mix to play with Zero Hour's falloff. `None` when no gun converted with that FX weapon
/// (the engine's FX weapon plays its own `FireSound`).
pub fn shot(fx: &str, d: Option<f32>) -> Option<Result<Vec<(&'static str, f32)>, &'static str>> {
    let table = layers().read().unwrap_or_else(|e| e.into_inner());
    let s = table.get(fx)?;
    Some(match d {
        Some(d) => Ok(s.layers.iter().map(|l| (l.event, l.gain(d))).filter(|(_, g)| *g > 0.0).collect()),
        None => Err(s.chain),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLON: Curve = [(0.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 0.0)];
    const ALLOFF: Curve = [(0.0, 0.0), (1.0, 0.0), (1.0, 0.0), (1.0, 0.0), (1.0, 0.0), (1.0, 0.0), (1.0, 0.0), (1.0, 0.0)];
    const COSDELAY: Curve = [(0.0, 1.0), (0.3, 1.0), (0.541137, 0.707946), (0.645471, 0.562341), (0.728955, 0.398107), (0.802613, 0.281838), (0.870886, 0.141254), (1.0, 0.0)];
    const SINDELAY: Curve = [(0.0, 0.0), (0.3, 0.0), (0.429114, 0.141254), (0.497387, 0.281838), (0.571045, 0.398107), (0.654529, 0.562341), (0.758863, 0.707946), (1.0, 1.0)];
    const DEFAULT: Curve = [(0.0, 1.0), (0.142857, 0.630957), (0.285714, 0.398107), (0.428571, 0.251189), (0.571429, 0.177828), (0.714286, 0.112202), (0.857143, 0.070795), (1.0, 0.0)];

    #[test]
    fn curves_are_piecewise_linear() {
        assert_eq!(curve_eval(&COSDELAY, 0.2), 1.0);
        assert!((curve_eval(&COSDELAY, 0.4205) - 0.854).abs() < 0.01);
        assert_eq!(curve_eval(&COSDELAY, 1.0), 0.0);
        assert_eq!(curve_eval(&ALLON, 0.5), 1.0);
        assert_eq!(curve_eval(&DEFAULT, 7.0), 0.0, "clamped");
    }

    /// The AN-94's third person (Zero Hour units at 6 per foot: 1,800" = 300, 12,000" = 2,000): the
    /// close shot alone at the source, both through the crossfade, only the distant layer past it.
    #[test]
    fn the_chain_crossfades_with_distance() {
        let text = [
            Layer { event: "close", min: 300.0, max: 300.0, min_curve: COSDELAY, curve: ALLOFF },
            Layer { event: "dist", min: 300.0, max: 2000.0, min_curve: SINDELAY, curve: DEFAULT },
        ]
        .iter()
        .map(|l| format_row("ZHBO_FX_x", "ZHBO_BO2_FireNpc_x", l))
        .collect::<String>();
        let table = parse(&text);
        assert_eq!(table["ZHBO_FX_x"].chain, "ZHBO_BO2_FireNpc_x");
        let l = &table["ZHBO_FX_x"].layers;
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].gain(0.0), l[1].gain(0.0)), (1.0, 0.0), "point-blank: no distant layer");
        assert!(l[0].gain(200.0) > 0.0 && l[1].gain(200.0) > 0.0, "both through the crossfade");
        assert_eq!(l[0].gain(500.0), 0.0);
        assert!(l[1].gain(500.0) > 0.5);
        assert_eq!(l[1].gain(2000.0), 0.0, "silent at distMax");
        install(&text);
        assert_eq!(shot("ZHBO_FX_x", Some(0.0)), Some(Ok(vec![("close", 1.0)])));
        assert_eq!(shot("ZHBO_FX_x", Some(500.0)).unwrap().unwrap().len(), 1);
        assert_eq!(shot("ZHBO_FX_x", None), Some(Err("ZHBO_BO2_FireNpc_x")), "no listener: the close mix");
        assert!(shot("ZHBO_FX_none", Some(0.0)).is_none());
        // The client's cue (`sounds::Cues::third_person_shot`): the layers at their gains from
        // the shooter's position, the close mix when there is no first-person listener, nothing for
        // an FX weapon without layers.
        use crate::math::Vec3;
        let mut cues = crate::sounds::Cues::default();
        let at = Vec3::new(500.0, 0.0, 0.0);
        cues.third_person_shot("ZHBO_FX_x", at, Some(Vec3::new(0.0, 0.0, 0.0)));
        cues.third_person_shot("ZHBO_FX_x", at, None);
        cues.third_person_shot("ZHBO_FX_none", at, Some(Vec3::default()));
        let q: Vec<(&str, f32)> = cues.queue.iter().map(|r| (r.event, r.gain)).collect();
        assert_eq!(q.len(), 2, "{q:?}");
        assert_eq!(q[0].0, "dist");
        assert!((q[0].1 - l[1].gain(500.0)).abs() < 1e-6 && q[0].1 < 1.0);
        assert_eq!(q[1], ("ZHBO_BO2_FireNpc_x", 1.0));
        assert!(cues.queue.iter().all(|r| r.at == Some(at)));
        assert!(parse("layer a b c 1 2 3\nnoise\n").is_empty(), "malformed rows skipped");
        let flat = Layer { event: "2d", min: 0.0, max: 1e9, min_curve: FLAT, curve: FLAT };
        assert_eq!(flat.gain(5000.0), 1.0, "a 2D layer is not attenuated");
    }
}
