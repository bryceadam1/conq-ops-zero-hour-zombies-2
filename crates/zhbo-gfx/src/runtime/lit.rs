//! BO2's lit shading, as the native pass's `fs` runs it (`native.wgsl`), in plain Rust: the
//! reference the shader is tested against (`tests/native.rs` renders a flat surface under fixed
//! lights and compares it to [`shade`]) and the place BO2's formulas are pinned by unit tests.
//!
//! Read from the DXBC of BO2's `lit_sun` pixel shaders in the install (MODLOG "BO2's lit shading,
//! from its DXBC"): `pimp_shader_lprobe_bc900d1` (the guns' `lit_sm_r0c0n0s0o0`),
//! `pimp_shader_lprobe_a96c9c79` (`lit_sm_r0c0n0s0`), `pimp_shader_sw4_3d_char_cloth_49ec0efa`
//! and `pimp_shader_sw4_3d_char_skin_7f1d600` (the bodies and the arms). They share one BRDF:
//!
//! * colour space: every colour is squared on the way in (the colour map, the spec map, the light
//!   grid) and the sum is square-rooted on the way out (`sqrt o0.xyz`) — gamma 2;
//! * the normal map: `n = normalize(N + T·x + B·y)` with `x, y = 4.01575·texel − 2.01575` (no z
//!   reconstructed: the map holds the bump at half scale), `B = sign · cross(N, T)`;
//! * the highlight: `pow(N·H, p) · (p/8 + 1/4)` with `p = 2^(13·gloss)`, times the visibility
//!   `N·L / ((L·H)²·k − k + 1.01)` with `k = saturate(gloss + 0.545)`, times the Fresnel
//!   `f0 + (1 − f0)·2^(−10·L·H)` with `f0 = spec²`, times the sun;
//! * the reflection: the probe at `R = reflect(−V, n)`, mip `4 − 4·gloss`, times
//!   `saturate(f0·(0.75 + 0.25·g − a) + a)` with
//!   `a = 1.04167·g·min(2^(−9.28·N·V), 0.475·g) + 0.0182292·g − 0.015625`;
//! * the diffuse: `albedo² · (N·L · sun + grid)`, the grid (BO2's light grid, `modelLightingSampler`)
//!   occluded, the sun not.
//!
//! Zero Hour supplies no light grid, reflection probe, shadow or exposure; each stands in from what
//! it does give (the engine constraint, RENDERER.md "Lit shading"): BO2's one sun is Zero Hour's
//! light 0; the light grid is Zero Hour's own irradiance without its sun (the ambient plus
//! `N·L`-weighted lights 1 and 2, the accent lights its W3D models take); the probe's radiance
//! towards `R` is the same irradiance environment sampled along `R`, sun included (a probe is
//! renormalised to the local light, BO2's `reflectionLightingSH / gridLightingSH`); the sun is
//! unshadowed and the exposure (`hdrControl0.x`) is 1. Zero Hour's light colours are display
//! values and are decoded the way BO2 decodes its colours (squared).

/// BO2's decode of a display colour to light (gamma 2): `c²`.
pub fn decode(c: f32) -> f32 {
    c * c
}

/// The highlight's exponent: `2^(13·gloss)` (gloss is the spec map's alpha).
pub fn specular_power(gloss: f32) -> f32 {
    (13.0 * gloss).exp2()
}

/// The highlight's normalisation, `p/8 + 1/4`.
pub fn specular_normalization(power: f32) -> f32 {
    power * 0.125 + 0.25
}

/// The visibility term, `N·L / ((L·H)²·k − k + 1.01)`, `k = saturate(gloss + 0.545)`.
pub fn visibility(n_dot_l: f32, l_dot_h: f32, gloss: f32) -> f32 {
    let k = (gloss + 0.545).clamp(0.0, 1.0);
    n_dot_l / (l_dot_h * l_dot_h * k - k + 1.01)
}

/// The highlight's Fresnel, `f0 + (1 − f0)·2^(−10·L·H)`.
pub fn fresnel(f0: f32, l_dot_h: f32) -> f32 {
    f0 + (1.0 - f0) * (-10.0 * l_dot_h).exp2()
}

/// The reflection's Fresnel by gloss and `N·V`.
pub fn env_fresnel(f0: f32, n_dot_v: f32, gloss: f32) -> f32 {
    let a = 1.041_67 * gloss * (-9.28 * n_dot_v).exp2().min(0.475 * gloss) + 0.018_229_2 * gloss - 0.015_625;
    (f0 * (0.25 * gloss + 0.75 - a) + a).clamp(0.0, 1.0)
}

/// A normal map texel's tangent-space X or Y as BO2 unpacks it: `4.01575·t − 2.01575`.
pub fn normal_map(t: f32) -> f32 {
    t * 4.015_75 - 2.015_75
}

/// The sun's highlight scalar (before its colour, the spec colour's Fresnel and the scales).
pub fn highlight(n_dot_h: f32, n_dot_l: f32, l_dot_h: f32, gloss: f32) -> f32 {
    let p = specular_power(gloss);
    // BO2: `exp(log(N·H)·p)`: 0 at N·H = 0.
    let d = if n_dot_h > 0.0 { (n_dot_h.log2() * p).exp2() } else { 0.0 };
    d * specular_normalization(p) * visibility(n_dot_l, l_dot_h, gloss)
}

/// One Zero Hour light as the pass takes it: towards the light, display colour.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub dir: [f32; 3],
    pub color: [f32; 3],
}

/// What one pixel is shaded with: the surface (normal, colour, spec colour and gloss, occlusion),
/// the material's scales (`zhbo_native::model::Lighting` order: specular, reflection, ambient,
/// diffuse normal) and the scene (view direction, Zero Hour's ambient and lights, light 0 the sun).
#[derive(Clone, Copy, Debug)]
pub struct Pixel<'a> {
    /// The shading normal (the normal map applied), unit.
    pub n: [f32; 3],
    /// The diffuse normal (the bump scaled by `Diffuse_Normal_Height_Facing`), unit.
    pub n_diffuse: [f32; 3],
    /// Towards the eye, unit.
    pub v: [f32; 3],
    /// Raw texels (display values).
    pub albedo: [f32; 3],
    /// The spec map's colour, or `None` (no spec map: BO2's technique has no highlight or
    /// reflection).
    pub spec: Option<[f32; 3]>,
    pub gloss: f32,
    pub ao: f32,
    pub scales: [f32; 4],
    pub ambient: [f32; 3],
    pub lights: &'a [Light],
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 1e-12 { [a[0] / l, a[1] / l, a[2] / l] } else { [0.0, 0.0, 1.0] }
}

/// Zero Hour's irradiance towards `n` without its sun, decoded: BO2's light grid stand-in.
pub fn grid(n: [f32; 3], ambient: [f32; 3], lights: &[Light]) -> [f32; 3] {
    let mut e = ambient.map(decode);
    for l in lights.iter().skip(1) {
        let w = dot(n, normalize(l.dir)).max(0.0);
        for (c, k) in e.iter_mut().zip(l.color) {
            *c += w * decode(k);
        }
    }
    e
}

/// The reflection probe's stand-in towards `r`: Zero Hour's irradiance along `r`, sun included.
pub fn probe(r: [f32; 3], ambient: [f32; 3], lights: &[Light]) -> [f32; 3] {
    let mut e = grid(r, ambient, lights);
    if let Some(sun) = lights.first() {
        let w = dot(r, normalize(sun.dir)).max(0.0);
        for (c, k) in e.iter_mut().zip(sun.color) {
            *c += w * decode(k);
        }
    }
    e
}

/// One pixel, as `native.wgsl` `fs` writes it (display values, before any sRGB target decode).
pub fn shade(p: &Pixel) -> [f32; 3] {
    let [specular, reflection, ambient_scale, _] = p.scales;
    let (n, v) = (p.n, p.v);
    let sun = p.lights.first().copied();
    let mut out = [0.0f32; 3];
    let albedo = p.albedo.map(decode);
    let sun_color = sun.map_or([0.0; 3], |s| s.color.map(decode));
    let l = sun.map_or([0.0, 0.0, 1.0], |s| normalize(s.dir));
    let n_dot_l_diffuse = if sun.is_some() { dot(p.n_diffuse, l).max(0.0) } else { 0.0 };
    let g = grid(p.n_diffuse, p.ambient, p.lights);
    for c in 0..3 {
        out[c] = albedo[c] * (n_dot_l_diffuse * sun_color[c] + g[c] * p.ao * ambient_scale);
    }
    if let Some(spec) = p.spec {
        let f0 = spec.map(decode);
        if sun.is_some() {
            let h = normalize([l[0] + v[0], l[1] + v[1], l[2] + v[2]]);
            let (n_dot_h, l_dot_h, n_dot_l) = (dot(n, h).clamp(0.0, 1.0), dot(h, l).clamp(0.0, 1.0), dot(n, l).clamp(0.0, 1.0));
            let s = highlight(n_dot_h, n_dot_l, l_dot_h, p.gloss) * specular * p.ao;
            for c in 0..3 {
                out[c] += s * fresnel(f0[c], l_dot_h) * sun_color[c];
            }
        }
        let n_dot_v = dot(n, v).clamp(0.0, 1.0);
        let k = 2.0 * dot(n, v);
        let r = [n[0] * k - v[0], n[1] * k - v[1], n[2] * k - v[2]];
        let e = probe(r, p.ambient, p.lights);
        for c in 0..3 {
            out[c] += e[c] * env_fresnel(f0[c], n_dot_v, p.gloss) * reflection * p.ao;
        }
    }
    out.map(|c| c.max(0.0).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4 * b.abs().max(1.0)
    }

    /// BO2's gloss runs the exponent from 1 (gloss 0) to 8192 (gloss 1), normalised `p/8 + 1/4`.
    #[test]
    fn gloss_maps_to_bo2s_power() {
        assert!(near(specular_power(0.0), 1.0));
        assert!(near(specular_power(1.0), 8192.0));
        assert!(near(specular_power(0.5), 2f32.powf(6.5)));
        assert!(near(specular_normalization(8.0), 1.25));
        // On the mirror direction the lobe's peak is the normalisation times the visibility.
        let g = 0.6;
        let p = specular_power(g);
        assert!(near(highlight(1.0, 1.0, 1.0, g), specular_normalization(p) * visibility(1.0, 1.0, g)));
        assert_eq!(highlight(0.0, 1.0, 1.0, g), 0.0, "N·H = 0: BO2's exp(log 0 · p) is 0");
    }

    /// The visibility at L·H = 1 is `N·L / 1.01`; at grazing L·H it divides by `1.01 − k`.
    #[test]
    fn the_visibility_is_bo2s() {
        assert!(near(visibility(1.0, 1.0, 0.3), 1.0 / 1.01));
        let k = 0.3f32 + 0.545;
        assert!(near(visibility(0.5, 0.0, 0.3), 0.5 / (1.01 - k)));
        // k saturates: gloss 0.9 and 1.0 see the same k = 1.
        assert!(near(visibility(1.0, 0.0, 0.9), visibility(1.0, 0.0, 1.0)));
        assert!(near(visibility(1.0, 0.0, 1.0), 100.0));
    }

    /// The highlight's Fresnel runs from `f0 + (1 − f0)` at L·H = 0 to `f0 + (1 − f0)/1024` head on.
    #[test]
    fn the_highlights_fresnel_is_bo2s() {
        assert!(near(fresnel(0.04, 0.0), 1.0));
        assert!(near(fresnel(0.04, 1.0), 0.04 + 0.96 / 1024.0));
        assert!(near(fresnel(1.0, 0.3), 1.0));
    }

    /// The reflection's Fresnel: a rough surface (gloss 0) reflects `0.75·f0 + 0.015625` at most,
    /// a glossy one head on about f0, and every glossy one goes to 1 at grazing for a white f0.
    #[test]
    fn the_reflections_fresnel_is_bo2s() {
        assert!(near(env_fresnel(0.5, 1.0, 0.0), 0.5 * (0.75 + 0.015_625) - 0.015_625));
        let head_on = env_fresnel(0.04, 1.0, 1.0);
        let a = 1.041_67 * (-9.28f32).exp2() + 0.018_229_2 - 0.015_625;
        assert!(near(head_on, 0.04 * (1.0 - a) + a));
        let grazing = env_fresnel(0.04, 0.0, 1.0);
        assert!(near(grazing, 0.04 * (1.0 - (1.041_67 * 0.475 + 0.002_604_2)) + 1.041_67 * 0.475 + 0.002_604_2));
        assert!(grazing > head_on * 5.0, "the reflection rises at grazing angles");
        assert_eq!(env_fresnel(0.0, 1.0, 0.0), 0.0, "clamped at 0: a black spec map reflects nothing");
    }

    /// The normal map's mid texel (128) is flat; full scale is about ±2 (the half-scale bump).
    #[test]
    fn the_normal_map_unpacks_as_bo2s() {
        assert!(normal_map(128.0 / 255.0).abs() < 1e-4);
        assert!(near(normal_map(1.0), 2.0));
        assert!(near(normal_map(0.0), -2.015_75));
    }

    /// A white surface lit by an ambient alone shows the ambient (gamma 2 round trip); with no spec
    /// map there is no highlight or reflection at all; with one, both add.
    #[test]
    fn shade_round_trips_the_colour_space_and_adds_the_spec_terms() {
        let up = [0.0, 0.0, 1.0];
        let mut p = Pixel { n: up, n_diffuse: up, v: up, albedo: [1.0; 3], spec: None, gloss: 0.5, ao: 1.0, scales: [1.0; 4], ambient: [0.4; 3], lights: &[] };
        assert!(shade(&p).iter().all(|c| near(*c, 0.4)));
        let sun = [Light { dir: [0.0, 0.0, 1.0], color: [0.8; 3] }];
        p.lights = &sun;
        // sqrt(0.16 + 0.64)
        let lit = shade(&p);
        assert!(lit.iter().all(|c| near(*c, 0.8f32.sqrt())), "{lit:?}");
        p.spec = Some([0.5; 3]);
        assert!(shade(&p)[0] > lit[0], "the highlight and reflection add");
        p.scales = [0.0, 0.0, 1.0, 1.0];
        assert!(near(shade(&p)[0], lit[0]), "the material's scales turn them off");
    }
}
