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
    // 4 specular, 5 reflection)
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> palette: array<mat4x4<f32>, 256>;

@group(1) @binding(0) var color_map: texture_2d<f32>;
@group(1) @binding(1) var maps_map: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
// x: alpha test (BO2's GE128), y: emissive, z: has packed maps
@group(1) @binding(3) var<uniform> material: vec4<f32>;

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
    let albedo = textureSample(color_map, samp, i.uv);
    let maps = textureSample(maps_map, samp, i.uv);
    let dp1 = dpdx(i.world);
    let dp2 = dpdy(i.world);
    let duv1 = dpdx(i.uv);
    let duv2 = dpdy(i.uv);

    if (material.x > 0.5 && albedo.a < 0.5) {
        discard;
    }
    if (material.y > 0.5) {
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
    if (material.z > 0.5 && !(debug_mode > 0.5 && debug_mode < 1.5)) {
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
    let gloss = select(0.0, maps.b, material.z > 0.5);
    let f0 = select(0.0, maps.a * maps.a, material.z > 0.5);
    let power = min(exp2(1.0 + gloss * 11.0), MAX_POWER);
    let norm = (power + 8.0) / 8.0;
    // Opaque surfaces carry ambient occlusion in the colour map's alpha; others carry coverage.
    let ao = select(1.0, albedo.a, material.z > 0.5);
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
        let fresnel = f0 + (1.0 - f0) * pow(1.0 - v_dot_h, 5.0) * gloss * gloss;
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
    let env_fresnel = f0 + (1.0 - f0) * pow(1.0 - n_dot_v, 5.0) * gloss * gloss;

    let diffuse_light = min(ambient * ao + direct, vec3<f32>(1.0));
    var color = albedo.rgb * diffuse_light * (1.0 - f0) + spec + env * env_fresnel * ao;
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
    let alpha = select(albedo.a, 1.0, material.z > 0.5);
    return finish(color, alpha);
}
