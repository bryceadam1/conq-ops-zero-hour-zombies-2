// The native BO2 model pass (Track R): GPU-skinned BO2 models lit as BO2's material model does.
//
// The lighting is the engine's bo2Shade (host patch 0009, MODLOG #74), which was tuned in game on
// the W3D path: per-pixel normals with the tangent frame from screen-space derivatives, a
// gloss-driven normalized Blinn-Phong highlight with Fresnel, and a reflection of a sky and ground
// built from the scene's lights. It runs in display (gamma) space, as bo2Shade does and as the rest
// of Zero Hour's scene is lit: colour maps are sampled raw and the lights are Zero Hour's own
// display-encoded values. (A linear-light port washed dark finishes out to grey under Zero Hour's
// bright map ambients: the environment term, tuned in gamma space, is several times stronger in
// linear light. Integrator finding #1, round 2.) Only when the target view is itself sRGB is the
// result decoded first, since the hardware encodes it again (`decode_output`).

const MAX_BONES: u32 = 256u;
const MAX_POWER: f32 = 2048.0;
const LIGHT_SATURATION: f32 = 0.35;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

override decode_output: bool = false;

struct Frame {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    ambient: vec4<f32>,
    light_dir: array<vec4<f32>, 3>,
    light_color: array<vec4<f32>, 3>,
    // x: light count, y: debug view (as ZHBO_BO2_DEBUG: 1 flat normals, 2 normals, 3 diffuse,
    // 4 specular, 5 reflection), z: BO2's fWeaponPosFrac (weaponParam0.w), w: optics knocked out
    // (weaponParam0.y)
    params: vec4<f32>,
    // xy: the target's size in pixels (BO2's renderTargetSize), z: the two-colour reticle's alt
    // lerp (weaponParam0.z)
    screen: vec4<f32>,
    // The MMS screen. optics: x seconds (gameTime), y BO2 inches per world unit, z blip count;
    // forward: the view's forward (world); depth: x, y the target view's projection terms
    // (device depth = x + y / view z); blips: per character (pos.xyz, radius), (height,
    // intensity, -, -)
    optics: vec4<f32>,
    forward: vec4<f32>,
    depth: vec4<f32>,
    blips: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> palette: array<mat4x4<f32>, 256>;

@group(1) @binding(0) var color_map: texture_2d<f32>;
@group(1) @binding(1) var maps_map: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
// x: alpha test (BO2's GE128), y: emissive, z: has packed maps, w: has a camo layer
// camo: offset.xy, scale, gain
// spec: x: has a specular colour map
// reticle (the lens reticle's constants, by BO2's shader names):
//   ret0: Color_Map_Scale, Glow_Falloff, Emissive_Amount, Noise_Scale
//   ret1: Reticle_Color.rgb, Color_Map_Noise
//   ret2: Glow_Color.rgb, Background
//   ret3: Reticle_Alt_Color.rgb, technique (0 dynamic, 1 two-colour, 2 static, 3 ui3d)
//   ret4: Glow_Alt_Color.rgb
struct Material {
    params: vec4<f32>,
    camo: vec4<f32>,
    spec: vec4<f32>,
    ret0: vec4<f32>,
    ret1: vec4<f32>,
    ret2: vec4<f32>,
    ret3: vec4<f32>,
    ret4: vec4<f32>,
    // The MMS screen's constants (zhbo_native::model::Sonar::constants order).
    sonar: array<vec4<f32>, 8>,
}
@group(1) @binding(3) var<uniform> material: Material;
@group(1) @binding(4) var camo_map: texture_2d<f32>;
// BO2's spec map colour (raw). Surfaces without one bind the 1x1 white texture and take the
// packed maps' scalar level instead.
@group(1) @binding(5) var spec_map: texture_2d<f32>;
// The scene's device depth (resolved from the target's depth attachment; 1 is the sky).
@group(2) @binding(0) var scene_z: texture_2d<f32>;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bones: vec4<u32>,
    @location(4) weights: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

fn bone(i: u32) -> mat4x4<f32> {
    return palette[min(i, MAX_BONES - 1u)];
}

@vertex
fn vs(v: VIn) -> VOut {
    let m = bone(v.bones.x) * v.weights.x + bone(v.bones.y) * v.weights.y + bone(v.bones.z) * v.weights.z + bone(v.bones.w) * v.weights.w;
    let world = m * vec4<f32>(v.pos, 1.0);
    var o: VOut;
    o.clip = frame.view_proj * world;
    o.world = world.xyz;
    o.normal = (m * vec4<f32>(v.normal, 0.0)).xyz;
    o.uv = v.uv;
    return o;
}

fn decode(v: vec3<f32>) -> vec3<f32> {
    let c = max(v, vec3<f32>(0.0));
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn finish(c: vec3<f32>, a: f32) -> vec4<f32> {
    if (decode_output) {
        return vec4<f32>(decode(c), a);
    }
    return vec4<f32>(c, a);
}

@fragment
fn fs(i: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Everything that needs uniform control flow first: samples and derivatives.
    var albedo = textureSample(color_map, samp, i.uv);
    let maps = textureSample(maps_map, samp, i.uv);
    // The chosen camo layer. Sampled unconditionally: implicit derivatives need uniform control
    // flow. Surfaces with no camo bind the 1x1 white texture and skip the mix below.
    let camo = textureSample(camo_map, samp, i.uv * material.camo.z + material.camo.xy);
    let spec_color = textureSample(spec_map, samp, i.uv).rgb;
    if (material.params.w > 0.5) {
        // BO2 replaces the surface's camo layer: the layer's colour takes the base colour's place
        // at its own per-pixel coverage. The gain is per set in the blob (camo_uv.w; §5.3).
        albedo = vec4<f32>(mix(albedo.rgb, camo.rgb * material.camo.w, camo.a), albedo.a);
    }
    let dp1 = dpdx(i.world);
    let dp2 = dpdy(i.world);
    let duv1 = dpdx(i.uv);
    let duv2 = dpdy(i.uv);

    if (material.params.x > 0.5 && albedo.a < 0.5) {
        discard;
    }
    if (material.params.y > 0.5) {
        return finish(albedo.rgb, albedo.a);
    }

    let len = length(i.normal);
    var geo = select(vec3<f32>(0.0, 0.0, 1.0), i.normal / len, len > 1e-5);
    if (!front) {
        geo = -geo;
    }
    let debug_mode = frame.params.y;

    // The normal map's tangent-space normal, framed by the surface's position/UV derivatives.
    var n = geo;
    if (material.params.z > 0.5 && !(debug_mode > 0.5 && debug_mode < 1.5)) {
        let xy = maps.rg * 2.0 - 1.0;
        let dp2perp = cross(dp2, geo);
        let dp1perp = cross(geo, dp1);
        let t = dp2perp * duv1.x + dp1perp * duv2.x;
        let b = dp2perp * duv1.y + dp1perp * duv2.y;
        let fr = max(dot(t, t), dot(b, b));
        if (fr > 1e-20) {
            let inv = inverseSqrt(fr);
            let z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
            n = normalize(t * (xy.x * inv) + b * (xy.y * inv) + geo * z);
        }
    }
    let gloss = select(0.0, maps.b, material.params.z > 0.5);
    // Reflectance at normal incidence: BO2's spec map is a colour (`*_spc-rgb`), so a metal's
    // highlight and reflection take its hue (gold: a flat dark colour map, a gold spec map). The
    // raw value is squared as the scalar level always was; a grey map gives the same f0 as before.
    var f0 = vec3<f32>(select(0.0, maps.a * maps.a, material.params.z > 0.5));
    if (material.spec.x > 0.5) {
        f0 = spec_color * spec_color;
    }
    let power = min(exp2(1.0 + gloss * 11.0), MAX_POWER);
    let norm = (power + 8.0) / 8.0;
    // Opaque surfaces carry ambient occlusion in the colour map's alpha; others carry coverage.
    let ao = select(1.0, albedo.a, material.params.z > 0.5);
    let v = normalize(frame.eye.xyz - i.world);

    let ambient = frame.ambient.rgb;
    var direct = vec3<f32>(0.0);
    var spec = vec3<f32>(0.0);
    var sun = vec3<f32>(0.0);
    let count = u32(frame.params.x);
    for (var li = 0u; li < 3u; li = li + 1u) {
        if (li >= count) {
            break;
        }
        let ld = frame.light_dir[li].xyz;
        let l = select(vec3<f32>(0.0, 0.0, 1.0), normalize(ld), length(ld) > 1e-5);
        let raw = frame.light_color[li].rgb;
        if (li == 0u) {
            sun = raw;
        }
        // BO2 lights its weapons near neutral; Zero Hour's accent lights are strongly coloured.
        let light = mix(vec3<f32>(dot(raw, LUMA)), raw, LIGHT_SATURATION);
        let n_dot_l = max(0.0, dot(n, l));
        let facing = clamp(dot(geo, l) * 4.0 + 1.0, 0.0, 1.0);
        direct = direct + light * n_dot_l * facing;
        let h = normalize(l + v);
        let v_dot_h = max(0.0, dot(v, h));
        let fresnel = f0 + (vec3<f32>(1.0) - f0) * (pow(1.0 - v_dot_h, 5.0) * gloss * gloss);
        spec = spec + light * (pow(max(0.0, dot(n, h)), power) * norm * n_dot_l * facing * fresnel);
    }

    let r = reflect(-v, n);
    let blur = 1.0 - gloss;
    let up = smoothstep(-0.25 - blur * 0.5, 0.25 + blur * 0.5, r.z);
    let ambient_level = dot(ambient, LUMA);
    let sun_level = dot(sun, LUMA);
    let sky = vec3<f32>(0.92, 0.97, 1.05) * (ambient_level * 1.4 + sun_level * 0.35);
    let ground = vec3<f32>(1.0, 0.96, 0.9) * (ambient_level * 0.45 + sun_level * 0.08);
    let horizon = exp(-r.z * r.z * (40.0 - blur * 36.0)) * (0.25 + 0.5 * gloss);
    let env = mix(ground, sky, up) + sky * horizon;
    let n_dot_v = max(0.0, dot(n, v));
    let env_fresnel = f0 + (vec3<f32>(1.0) - f0) * (pow(1.0 - n_dot_v, 5.0) * gloss * gloss);

    let diffuse_light = min(ambient * ao + direct, vec3<f32>(1.0));
    var color = albedo.rgb * diffuse_light * (vec3<f32>(1.0) - f0) + spec + env * env_fresnel * ao;
    if (debug_mode > 1.5) {
        if (debug_mode < 2.5) {
            return vec4<f32>(n * 0.5 + 0.5, 1.0);
        } else if (debug_mode < 3.5) {
            color = diffuse_light;
        } else if (debug_mode < 4.5) {
            color = spec;
        } else {
            color = env * env_fresnel * ao;
        }
    }
    let alpha = select(albedo.a, 1.0, material.params.z > 0.5);
    return finish(color, alpha);
}

// BO2's lens reticles, read from the DXBC of their techniques (`pimp_shader_sw4_3d_reticle_*`).
//
// * dynamic (0) and two-colour (1): the vertex shader turns the eye->vertex direction v into the
//   reticle's UV through the surface's tangent frame, `0.5 + Color_Map_Scale * (dot(T, v),
//   -dot(B, v))`: a function of direction only, so the reticle is at infinity and stays on the
//   optic's axis however the gun sways (no parallax). Here the frame comes from the position/UV
//   derivatives (exact on a flat lens) and the projection runs per pixel. Noise at the lens UVs.
// * static (2) and ui3d (3): the image at the lens's own UVs; the noise in screen space (pixel /
//   renderTargetSize * Noise_Scale).
// The pixel shaders: red is the shape (Reticle_Color), green the glow (`green^Glow_Falloff *
// Glow_Color`, times the noise), blue the background's coverage (`* Background`), the shape dimmed
// by `1 - Color_Map_Noise * noise`, the sum times Emissive_Amount. The two-colour technique lerps
// its colours to the alt pair by weaponParam0.z. The ui3d technique reads the code's UI texture:
// red and blue are two shapes (Reticle_Color, Reticle_Alt_Color), green and alpha two glows
// (Glow_Color, Glow_Alt_Color), and it writes alpha 1 into an additive blend with no fade. BO2
// squares the colour, scales by the fade and takes the root (its HDR encode): the colour times
// `sqrt(fade)`. Only the dynamic technique fades in with the aim (`saturate(2 * ads)`); all but
// ui3d fade out while the optics are knocked out (weaponParam0.y).
// The target finder's diamond across, in lens UVs (cg_rangeFinderDiamondSize 50 / the 512-unit
// ui3d window, `FUN_00526780`), and its line's half-width (the atlas sprite is not converted).
const RANGE_FINDER_DIAMOND: f32 = 50.0 / 512.0;
const RANGE_FINDER_LINE: f32 = 0.006;

@fragment
fn fs_reticle(i: VOut) -> @location(0) vec4<f32> {
    let kind = material.ret3.w;
    let projected = kind < 1.5;
    let dp1 = dpdx(i.world);
    let dp2 = dpdy(i.world);
    let duv1 = dpdx(i.uv);
    let duv2 = dpdy(i.uv);
    let len = length(i.normal);
    let n = select(normalize(cross(dp1, dp2) + vec3<f32>(0.0, 0.0, 1e-20)), i.normal / len, len > 1e-5);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    let t = dp2perp * duv1.x + dp1perp * duv2.x;
    let b = dp2perp * duv1.y + dp1perp * duv2.y;
    let tn = t * inverseSqrt(max(dot(t, t), 1e-30));
    let bn = b * inverseSqrt(max(dot(b, b), 1e-30));
    let v = normalize(i.world - frame.eye.xyz);
    // The projected image is clamped (its sampler state's clampU/V), so off the axis it ends.
    let ruv = clamp(vec2<f32>(0.5 + dot(tn, v) * material.ret0.x, 0.5 - dot(bn, v) * material.ret0.x), vec2<f32>(0.0), vec2<f32>(1.0));
    // Every sample in uniform control flow (implicit derivatives); the technique picks one. The
    // projected image takes its top level (the clamp's edge would blow up its derivatives), the
    // lens-UV image and the noise their mips, as BO2 samples them.
    let c = select(textureSample(color_map, samp, i.uv), textureSampleLevel(color_map, samp, ruv, 0.0), projected);
    let screen_uv = i.clip.xy / max(frame.screen.xy, vec2<f32>(1.0));
    let noise = select(textureSample(maps_map, samp, screen_uv * material.ret0.w), textureSample(maps_map, samp, i.uv * material.ret0.w), projected).r;
    let falloff = material.ret0.y;
    let dim = 1.0 - material.ret1.w * noise;
    var rgb: vec3<f32>;
    var alpha: f32;
    var fade = 1.0 - frame.params.w;
    if (kind > 2.5) {
        // The target finder's diamonds (BO2 draws them into the ui3d texture, `FUN_00526780`):
        // each mark where the eye's ray to it crosses the lens, in the lens's UVs, a diamond
        // `RANGE_FINDER_DIAMOND` (cg_rangeFinderDiamondSize 50 of the 512-unit window) across, in
        // the alt shape and glow channels.
        let det = dot(dp1, dp2perp);
        // At least a pixel wide, so a small lens keeps a solid outline.
        let line = max(RANGE_FINDER_LINE, length(abs(duv1) + abs(duv2)));
        var marks = vec2<f32>(0.0);
        let count = u32(frame.optics.z);
        for (var m = 0u; m < 8u; m = m + 1u) {
            if (m >= count || abs(det) < 1e-20) {
                break;
            }
            let to = frame.blips[m * 2u].xyz - frame.eye.xyz;
            let denom = dot(to, n);
            if (abs(denom) < 1e-9) {
                continue;
            }
            let k = dot(i.world - frame.eye.xyz, n) / denom;
            if (k <= 0.0) {
                continue;
            }
            let delta = frame.eye.xyz + to * k - i.world;
            let at = i.uv + vec2<f32>(dot(delta, t), dot(delta, b)) / det;
            let off = abs(abs(i.uv.x - at.x) + abs(i.uv.y - at.y) - RANGE_FINDER_DIAMOND * 0.5);
            marks = marks + vec2<f32>(1.0 - smoothstep(0.0, line, off), exp(-off / (line * 4.0)) * 0.5);
        }
        let code = vec4<f32>(c.r, c.g, min(c.b + marks.x, 1.0), min(c.a + marks.y, 1.0));
        let shape = code.r * material.ret1.rgb + code.b * material.ret3.rgb;
        let glow = pow(max(code.g, 1e-6), falloff) * material.ret2.rgb + pow(max(code.a, 1e-6), falloff) * material.ret4.rgb;
        rgb = (shape * dim + glow * noise) * material.ret0.z;
        alpha = 1.0;
        fade = 1.0;
    } else {
        let alt = select(0.0, frame.screen.z, kind > 0.5 && kind < 1.5);
        let reticle_color = mix(material.ret1.rgb, material.ret3.rgb, alt);
        let glow_color = mix(material.ret2.rgb, material.ret4.rgb, alt);
        let glow = pow(max(c.g, 1e-6), falloff) * glow_color * noise;
        rgb = (c.r * reticle_color * dim + glow) * material.ret0.z;
        alpha = c.b * material.ret2.w;
        if (kind < 0.5) {
            fade = fade * clamp(2.0 * frame.params.z, 0.0, 1.0);
        }
    }
    return finish(min(rgb * sqrt(fade), vec3<f32>(1.0)), select(alpha * fade, 1.0, kind > 2.5));
}

// BO2's millimetre-scanner screen (`sw4_3d_sonar_attachment_lens2`, read from the DXBC of
// `pimp_shader_sw4_3d_sonar_attachment_lens2_c436f53e` VS / `_fc8afb12` PS). It is screen space:
// at its pixel it reads the scene's view depth (BO2's floatZ) and draws
// * the outlines: |depth - mean of four diagonal taps EdgeSize * 0.005 apart| * EdgeIntensity /
//   depth, cubed, between EdgeMinDist and EdgeMaxDist, in EdgeColor, pulsed by the lookup's blue;
// * the pulse: a ring sweeping out from the eye, phase frac(gameTime / PulseInterval) over a
//   wavelength PulseInterval / PulseTime * MaxDepth, profiled by the lookup's red, in PulseColor,
//   faded in over MinDepth/2..MinDepth and out over MinDepth..MaxPulseDepth;
// * the characters the code puts on the scanner (BO2 renders them into its sonar target; here each
//   blip is a capsule the pixel's ray is tested against), only where they stand at least 10" behind
//   the scene (through walls), at their intensity, pulsed by the lookup's green, in EnemiesColor;
// times the background noise (1 - 2 * BackgroundNoise * noise.z, the noise at the mesh UV /
// ScatterSize plus a per-frame random offset), (1 - weaponParam0.y) and hdrAmount, written opaque
// into its additive blend. BO2 squares, fogs (none on the viewmodel) and takes the root: the colour.
fn sonar_depth(p: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(scene_z));
    let d = textureLoad(scene_z, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0).r;
    let denom = d - frame.depth.x;
    // The sky and a target without depth are BO2's "nothing there" (floatZ 0: very far).
    if (d >= 1.0 || abs(denom) < 1e-12 || frame.depth.y == 0.0) {
        return 1e7;
    }
    return max(frame.depth.y / denom, 0.0) * frame.optics.y;
}

fn smooth01(x: f32) -> f32 {
    return x * x * (3.0 - 2.0 * x);
}

// The VS's per-frame offset (`dp4` chain on gameTime): z, w of its output.
fn sonar_jitter(t: f32) -> vec2<f32> {
    let k = vec4<f32>(81.2395, 17.348, 37.3498, 59.3948);
    var r = vec4<f32>(0.0, 0.0, 0.0, t);
    r.x = fract(dot(vec4<f32>(t), k));
    r.y = fract(dot(vec4<f32>(r.x, t, t, t), k));
    r.z = fract(dot(vec4<f32>(r.x, r.y, t, t), k));
    r.w = fract(dot(vec4<f32>(r.x, r.y, r.z, t), k));
    r.x = fract(dot(r, k));
    r.y = fract(dot(r, k));
    r.z = fract(dot(r, k));
    return vec2<f32>(r.z, fract(dot(r, k)));
}

// The nearest capsule (standing on `base`, radius `r`, height `h`) along the ray: its distance,
// or -1 (behind the eye or missed).
fn ray_capsule(o: vec3<f32>, d: vec3<f32>, base: vec3<f32>, r: f32, h: f32) -> f32 {
    let a = base + vec3<f32>(0.0, 0.0, r);
    let b = base + vec3<f32>(0.0, 0.0, max(h - r, r));
    let ba = b - a;
    let oa = o - a;
    let baba = dot(ba, ba);
    let bard = dot(ba, d);
    let baoa = dot(ba, oa);
    let qa = baba - bard * bard;
    let qb = baba * dot(d, oa) - baoa * bard;
    let qc = baba * dot(oa, oa) - baoa * baoa - r * r * baba;
    let disc = qb * qb - qa * qc;
    if (disc < 0.0) {
        return -1.0;
    }
    var y = -1.0;
    if (qa > 1e-9) {
        let t = (-qb - sqrt(disc)) / qa;
        y = baoa + t * bard;
        if (y > 0.0 && y < baba) {
            return t;
        }
    }
    // An end cap (the sphere at the end the body hit lies past).
    let c = select(o - b, oa, y <= 0.0);
    let cb = dot(d, c);
    let h2 = cb * cb - (dot(c, c) - r * r);
    if (h2 > 0.0) {
        return -cb - sqrt(h2);
    }
    return -1.0;
}

@fragment
fn fs_sonar(i: VOut) -> @location(0) vec4<f32> {
    let k0 = material.sonar[0];
    let k1 = material.sonar[1];
    let k2 = material.sonar[2];
    let k3 = material.sonar[3];
    let k4 = material.sonar[4];
    let k5 = material.sonar[5];
    let k6 = material.sonar[6];
    let k7 = material.sonar[7];
    let pulse_interval = max(k0.x, 1e-3);
    let pulse_time = max(k0.y, 1e-3);
    let max_depth = k0.z;
    let max_pulse_depth = k0.w;
    let min_depth = k1.x;
    let line_noise = k1.y;
    let background_noise = k1.z;
    let hdr_amount = k1.w;
    let scatter = max(k2.xy, vec2<f32>(1e-3));
    let edge_size = k2.z;
    let edge_intensity = k2.w;
    let edge_min = k3.x;
    let edge_max = k3.y;
    let pulse_scale = k3.z;
    let player_scale = k3.w;
    let outline_scale = k4.x;
    let min_player = k4.y;
    let enemies = vec4<f32>(k4.zw, k5.xy);
    let pulse_color = vec4<f32>(k5.zw, k6.xy);
    let edge_color = vec4<f32>(k6.zw, k7.xy);

    // The noise, at the mesh UVs plus the frame's random offset (uniform control flow first).
    let noise = textureSample(maps_map, samp, i.uv / scatter + sonar_jitter(frame.optics.x));
    let phase = fract(frame.optics.x / pulse_interval);
    let wave = max(pulse_interval / pulse_time * max_depth, 1e-3);

    // The characters on the scanner: the nearest blip along this pixel's ray, its view depth
    // (inches) and intensity (BO2's sonar colour and depth targets).
    let dir = normalize(i.world - frame.eye.xyz);
    let along = max(dot(dir, normalize(frame.forward.xyz + vec3<f32>(0.0, 0.0, 1e-12))), 1e-4);
    var enemy_t = 1e30;
    var enemy_i = 0.0;
    let count = u32(frame.optics.z);
    for (var b = 0u; b < 8u; b = b + 1u) {
        if (b >= count) {
            break;
        }
        let p = frame.blips[b * 2u];
        let q = frame.blips[b * 2u + 1u];
        let t = ray_capsule(frame.eye.xyz, dir, p.xyz, p.w, q.x);
        if (t > 0.0 && t < enemy_t) {
            enemy_t = t;
            enemy_i = clamp(q.y, 0.0, 1.0);
        }
    }
    // BO2: zNear / sonarDepth, 0 past 10000 (no character there).
    var enemy_depth = 0.0;
    if (enemy_t < 1e29) {
        enemy_depth = enemy_t * along * frame.optics.y;
        if (enemy_depth >= 10000.0) {
            enemy_depth = 0.0;
        }
    }
    let enemy_alpha = select(0.0, 1.0, enemy_t < 1e29);
    let enemy_lum = dot(vec3<f32>(enemy_i), vec3<f32>(0.33));

    let px = vec2<i32>(i.clip.xy);
    let size = max(frame.screen.xy, vec2<f32>(1.0));
    let tap = edge_size * 0.005 * size;
    let center = sonar_depth(px);
    let d_a = sonar_depth(vec2<i32>(i.clip.xy + vec2<f32>(-tap.x, tap.y)));
    let d_b = sonar_depth(vec2<i32>(i.clip.xy + vec2<f32>(-tap.x, -tap.y)));
    let d_c = sonar_depth(vec2<i32>(i.clip.xy + vec2<f32>(tap.x, -tap.y)));
    let d_d = sonar_depth(vec2<i32>(i.clip.xy + vec2<f32>(tap.x, tap.y)));
    let nearest = min(center, min(d_d, min(d_c, min(d_b, min(d_a, 1e7)))));
    let edge = clamp(abs(center - (d_a + d_b + d_c + d_d) * 0.25) * edge_intensity / max(center, 1e-6), 0.0, 1.0);

    // The character, pulsed by its own depth.
    let pe = fract((wave * phase - enemy_depth) / wave + 0.5) - 0.5;
    let player_pulse = textureSampleLevel(color_map, samp, vec2<f32>(pe / max(player_scale, 1e-6) + 0.5, 0.5), 0.0).g;
    let enemy_amount = min(enemy_alpha, max(player_pulse, min_player));
    // Behind the scene by 10" or more (a scene nearer than 10" counts as far).
    let scene_behind = select(center, 100000.0, center <= 10.0) + 10.0;
    let through_wall = select(0.0, 1.0, enemy_depth >= scene_behind);
    let enemy = enemy_lum * enemies.rgb * enemies.w * through_wall * enemy_amount;

    // The pulse ring and the outline, at the nearest of the taps.
    let ps = fract((wave * phase - nearest) / wave + 0.5) - 0.5;
    let ring = textureSampleLevel(color_map, samp, vec2<f32>(ps / max(pulse_scale, 1e-6) + 0.5, 0.5), 0.0).r;
    let outline = textureSampleLevel(color_map, samp, vec2<f32>(ps / max(outline_scale, 1e-6) + 0.5, 0.5), 0.0).b;
    let jitter = (noise.g - 0.5) * line_noise + 1.0;
    let inv_range = 1.0 / max(max_pulse_depth - min_depth, 1e-6);
    let half_min = max(min_depth * 0.5, 1e-6);
    let far_fade = 1.0 - smooth01(clamp((center * jitter - min_depth) * inv_range, 0.0, 1.0));
    let near_fade = smooth01(clamp((center * jitter - half_min) / half_min, 0.0, 1.0));
    let pulse = near_fade * far_fade * ring * pulse_color.rgb * pulse_color.w * select(0.0, 1.0, nearest >= 0.1);
    let enemy_far = 1.0 - smooth01(clamp((enemy_depth - min_depth) * inv_range, 0.0, 1.0));
    let enemy_near = smooth01(clamp((enemy_depth - half_min) / half_min, 0.0, 1.0));
    var color = enemy * enemy_far * enemy_near + pulse;
    var e = edge * edge * edge;
    if (nearest <= edge_min || nearest >= edge_max) {
        e = 0.0;
    }
    color = color + e * edge_color.rgb * edge_color.w * outline;
    color = color * (1.0 - 2.0 * background_noise * noise.b);
    color = color * (1.0 - frame.params.w) * hdr_amount;
    return finish(max(color, vec3<f32>(0.0)), 1.0);
}
