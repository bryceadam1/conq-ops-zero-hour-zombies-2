//! Translate bgfx `.sc` shaders (as used by the engine's render backend) into GLSL 450 that naga
//! compiles for wgpu, and reflect the layout the runtime needs.
//!
//! bgfx shaders are GLSL with a preprocessor dialect: `$input`/`$output` lines naming entries in
//! `varying.def.sc`, `SAMPLER2D(name, slot)` declarations, `texture2D()`-style sampling, loose
//! `uniform vec4/mat4` declarations, `mul()`, `vecN_splat()`, `gl_FragColor`, and predefined view
//! and model uniforms. This module rewrites that dialect into Vulkan-style GLSL 450:
//!
//! * every loose uniform (predefined or user) goes into one std140 block per stage, at
//!   `@group(0) @binding(0)` for the vertex stage and `@binding(1)` for the fragment stage (bgfx
//!   gives each stage its own constant buffer, so the two layouts differ);
//! * `SAMPLER2D(name, slot)` becomes a texture at `@group(1) @binding(slot * 2)` and a sampler at
//!   `@binding(slot * 2 + 1)`, and the sampling calls are rewritten to combine them;
//! * vertex attributes get locations from their semantic, varyings from their `varying.def.sc` order.
//!
//! `BGFX_SHADER_LANGUAGE_GLSL` is defined as 0: wgpu follows the D3D/Metal conventions (depth 0..1,
//! texture origin top-left), which is what bgfx's non-GLSL branches expect.

use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaryingDef {
    pub ty: String,
    pub name: String,
    pub semantic: String,
}

/// Parse `varying.def.sc` (`vec4 v_color0 : COLOR0 = vec4(...);`).
pub fn parse_varying_def(text: &str) -> Vec<VaryingDef> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let Some((lhs, rhs)) = line.split_once(':') else { continue };
        let mut lhs = lhs.split_whitespace();
        let (Some(ty), Some(name)) = (lhs.next(), lhs.next()) else { continue };
        let semantic = rhs.split(['=', ';']).next().unwrap_or("").trim().to_string();
        if !semantic.is_empty() {
            out.push(VaryingDef { ty: ty.into(), name: name.into(), semantic });
        }
    }
    out
}

pub use crate::state::attribute_location;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniformKind {
    Vec4,
    Mat4,
}

impl UniformKind {
    pub fn size(self) -> u32 {
        match self {
            UniformKind::Vec4 => 16,
            UniformKind::Mat4 => 64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniformInfo {
    pub name: String,
    pub kind: UniformKind,
    pub count: u32,
    /// Byte offset in the std140 uniform block.
    pub offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamplerInfo {
    pub name: String,
    pub slot: u32,
    pub array: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeInfo {
    pub name: String,
    pub semantic: String,
    pub location: u32,
}

#[derive(Clone, Debug)]
pub struct Translated {
    pub stage: Stage,
    pub glsl: String,
    pub uniforms: Vec<UniformInfo>,
    pub uniform_block_size: u32,
    pub samplers: Vec<SamplerInfo>,
    pub attributes: Vec<AttributeInfo>,
}

/// bgfx's predefined uniforms (`bgfx_shader.sh`). Only those a shader references are emitted.
const PREDEFINED: &[(&str, UniformKind, u32)] = &[
    ("u_viewRect", UniformKind::Vec4, 1),
    ("u_viewTexel", UniformKind::Vec4, 1),
    ("u_view", UniformKind::Mat4, 1),
    ("u_invView", UniformKind::Mat4, 1),
    ("u_proj", UniformKind::Mat4, 1),
    ("u_invProj", UniformKind::Mat4, 1),
    ("u_viewProj", UniformKind::Mat4, 1),
    ("u_invViewProj", UniformKind::Mat4, 1),
    ("u_model", UniformKind::Mat4, 1),
    ("u_modelView", UniformKind::Mat4, 1),
    ("u_modelViewProj", UniformKind::Mat4, 1),
    ("u_alphaRef4", UniformKind::Vec4, 1),
];

const PRELUDE_MACROS: &str = "\
#define mul(a, b) ((a) * (b))
#define vec2_splat(x) vec2(x)
#define vec3_splat(x) vec3(x)
#define vec4_splat(x) vec4(x)
#define saturate(x) clamp(x, 0.0, 1.0)
#define lerp(a, b, t) mix(a, b, t)
#define atan2(y, x) atan(y, x)
#define instMul(a, b) ((a) * (b))
#define mtxFromCols(a, b, c, d) mat4(a, b, c, d)
#define mtxFromRows(a, b, c, d) transpose(mat4(a, b, c, d))
#define BGFX_SHADER_LANGUAGE_GLSL 0
#define BGFX_SHADER_LANGUAGE_HLSL 0
#define BGFX_SHADER_LANGUAGE_METAL 0
#define BGFX_SHADER_LANGUAGE_SPIRV 1
";

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Replace whole-word occurrences of `word` with `with`.
fn replace_word(src: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let bytes = src.as_bytes();
    while let Some(pos) = src[i..].find(word) {
        let start = i + pos;
        let end = start + word.len();
        let before_ok = start == 0 || !is_ident(bytes[start - 1] as char);
        let after_ok = end >= src.len() || !is_ident(bytes[end] as char);
        out.push_str(&src[i..start]);
        out.push_str(if before_ok && after_ok { with } else { word });
        i = end;
    }
    out.push_str(&src[i..]);
    out
}

/// Rewrite `fname(sampler, ...)` into `glsl_fn(sampler2D(sampler, sampler_smp), ...)` for every
/// declared sampler.
fn rewrite_sampling(src: &str, fname: &str, glsl_fn: &str, samplers: &[SamplerInfo]) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    let open = format!("{fname}(");
    while let Some(pos) = rest.find(&open) {
        let before_ok = pos == 0 || !is_ident(rest.as_bytes()[pos - 1] as char);
        out.push_str(&rest[..pos]);
        let after = &rest[pos + open.len()..];
        let arg_end = after.find([',', ')']).unwrap_or(0);
        let name = after[..arg_end].trim();
        match samplers.iter().find(|s| s.name == name).filter(|_| before_ok) {
            Some(s) => {
                let ctor = if s.array { "sampler2DArray" } else { "sampler2D" };
                let _ = write!(out, "{glsl_fn}({ctor}({name}, {name}_smp)");
                rest = &after[arg_end..];
            }
            None => {
                out.push_str(&open);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Split the top-level comma-separated arguments of a call starting just after `(`.
/// Returns the argument strings and the index of the closing `)`.
fn split_args(s: &str) -> Option<(Vec<String>, usize)> {
    let mut depth = 0i32;
    let mut args = Vec::new();
    let mut cur = String::new();
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                cur.push(c);
            }
            ')' => {
                args.push(cur);
                return Some((args, i));
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    None
}

/// Rewrite `T f(sampler2D smp, ...)` into `T f(texture2D smp, sampler smp_smp, ...)` and every
/// call `f(s_tex, ...)` into `f(s_tex, s_tex_smp, ...)`. Returns the new source and the sampler
/// parameter names (which the sampling rewrite must also recognise).
fn split_sampler_parameters(src: &str) -> (String, Vec<String>) {
    // (function name, parameter indices that are samplers)
    let mut funcs: Vec<(String, Vec<usize>)> = Vec::new();
    let mut param_names = Vec::new();
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let mut new_line = line.to_string();
        if let Some(open) = line.find('(') {
            let head: Vec<&str> = line[..open].split_whitespace().collect();
            let is_definition = head.len() == 2 && !line.trim_start().starts_with('#');
            if is_definition {
                if let Some((args, close)) = split_args(&line[open + 1..]) {
                    let mut idx = Vec::new();
                    let mut new_args = Vec::new();
                    for (i, a) in args.iter().enumerate() {
                        let parts: Vec<&str> = a.split_whitespace().collect();
                        if parts.len() == 2 && parts[0] == "sampler2D" {
                            idx.push(i);
                            param_names.push(parts[1].to_string());
                            new_args.push(format!("texture2D {0}, sampler {0}_smp", parts[1]));
                        } else {
                            new_args.push(a.trim().to_string());
                        }
                    }
                    if !idx.is_empty() {
                        funcs.push((head[1].to_string(), idx));
                        new_line = format!("{}({}){}", &line[..open], new_args.join(", "), &line[open + 1 + close + 1..]);
                    }
                }
            }
        }
        out.push_str(&new_line);
        out.push('\n');
    }
    // Call sites (including inside macros); rewritten definitions are skipped.
    for (name, idx) in &funcs {
        let open = format!("{name}(");
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find(&open) {
            let before_ok = pos == 0 || !is_ident(rest.as_bytes()[pos - 1] as char);
            result.push_str(&rest[..pos + open.len()]);
            let after = &rest[pos + open.len()..];
            match split_args(after) {
                Some((args, close)) if before_ok && !args.iter().any(|a| a.contains("texture2D ")) => {
                    let mut new_args = Vec::new();
                    for (i, a) in args.iter().enumerate() {
                        new_args.push(a.trim().to_string());
                        if idx.contains(&i) {
                            new_args.push(format!("{}_smp", a.trim()));
                        }
                    }
                    result.push_str(&new_args.join(", "));
                    result.push(')');
                    rest = &after[close + 1..];
                }
                _ => rest = after,
            }
        }
        result.push_str(rest);
        out = result;
    }
    (out, param_names)
}

fn list_after(line: &str, directive: &str) -> Vec<String> {
    line[directive.len()..].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Translate one bgfx shader. `defines` are extra `NAME=VALUE` pairs (shaderc `--define`).
pub fn translate(src: &str, varyings: &[VaryingDef], stage: Stage, defines: &[(&str, &str)]) -> Result<Translated, String> {
    let mut inputs: Vec<String> = Vec::new();
    let mut outputs: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut samplers: Vec<SamplerInfo> = Vec::new();
    let mut user_uniforms: Vec<(String, UniformKind, u32)> = Vec::new();
    // Integer #defines, for uniform array sizes such as `u_swayTable[MAX_SWAY_TYPES_PLUS1]`.
    let int_defines: BTreeMap<String, u32> = src
        .lines()
        .filter_map(|l| {
            let mut p = l.trim().strip_prefix("#define")?.split_whitespace();
            let (name, value) = (p.next()?, p.next()?);
            Some((name.to_string(), value.parse().ok()?))
        })
        .collect();

    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("$input") {
            inputs = list_after(t, "$input");
            continue;
        }
        if t.starts_with("$output") {
            outputs = list_after(t, "$output");
            continue;
        }
        if t.starts_with("#include") {
            if t.contains("bgfx_shader.sh") {
                continue;
            }
            return Err(format!("unsupported include: {t}"));
        }
        if let Some(decl) = t.strip_prefix("SAMPLER2DARRAY(").or_else(|| t.strip_prefix("SAMPLER2D(")) {
            let array = t.starts_with("SAMPLER2DARRAY(");
            let inner = decl.split(')').next().unwrap_or("");
            let mut parts = inner.split(',');
            let name = parts.next().unwrap_or("").trim().to_string();
            let slot: u32 = parts.next().unwrap_or("").trim().parse().map_err(|_| format!("bad sampler slot: {t}"))?;
            if !samplers.iter().any(|s| s.name == name) {
                samplers.push(SamplerInfo { name, slot, array });
            }
            continue;
        }
        if let Some(decl) = t.strip_prefix("uniform ") {
            let decl = decl.split(';').next().unwrap_or("").trim();
            let mut parts = decl.split_whitespace();
            let ty = parts.next().unwrap_or("");
            let rest: String = parts.collect();
            let kind = match ty {
                "vec4" => UniformKind::Vec4,
                "mat4" => UniformKind::Mat4,
                other => return Err(format!("unsupported uniform type {other} in: {t}")),
            };
            let (name, count) = match rest.split_once('[') {
                Some((n, c)) => {
                    let c = c.trim_end_matches(']').trim();
                    let count = c.parse::<u32>().ok().or_else(|| int_defines.get(c).copied());
                    (n.to_string(), count.ok_or_else(|| format!("bad array size: {t}"))?)
                }
                None => (rest.clone(), 1),
            };
            user_uniforms.push((name, kind, count));
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }

    // Helper functions that take a combined `sampler2D` parameter: split it into a texture and a
    // sampler (Vulkan GLSL has no combined parameter type) and pass both at every call site.
    let (rewritten, param_samplers) = split_sampler_parameters(&body);
    body = rewritten;
    let mut sampling_names = samplers.clone();
    sampling_names.extend(param_samplers.iter().map(|n| SamplerInfo { name: n.clone(), slot: u32::MAX, array: false }));

    // Sampling calls.
    for (f, g) in [
        ("texture2DLod", "textureLod"),
        ("texture2DGrad", "textureGrad"),
        ("texture2DArray", "texture"),
        ("texture2D", "texture"),
    ] {
        body = rewrite_sampling(&body, f, g, &sampling_names);
    }
    if stage == Stage::Fragment {
        body = replace_word(&body, "gl_FragColor", "zhbo_FragColor");
    }

    // Uniform block: predefined uniforms the shader uses, then user uniforms, in declaration order.
    let mut uniforms: Vec<UniformInfo> = Vec::new();
    let mut offset = 0u32;
    let mut push_uniform = |name: &str, kind: UniformKind, count: u32| {
        uniforms.push(UniformInfo { name: name.to_string(), kind, count, offset });
        offset += kind.size() * count;
    };
    for &(name, kind, count) in PREDEFINED {
        if replace_word(&body, name, "\u{1}") != body {
            push_uniform(name, kind, count);
        }
    }
    for (name, kind, count) in &user_uniforms {
        push_uniform(name, *kind, *count);
    }
    let block_size = offset.max(16);

    let mut glsl = String::from("#version 450\n");
    for (k, v) in defines {
        let _ = writeln!(glsl, "#define {k} {v}");
    }
    glsl.push_str(PRELUDE_MACROS);

    if !uniforms.is_empty() {
        let _ = writeln!(glsl, "layout(set = 0, binding = {}, std140) uniform ZhboUniforms {{", uniform_binding(stage));
        for u in &uniforms {
            let ty = match u.kind {
                UniformKind::Vec4 => "vec4",
                UniformKind::Mat4 => "mat4",
            };
            if u.count > 1 || u.name == "u_model" {
                let _ = writeln!(glsl, "    {ty} {}[{}];", u.name, u.count);
            } else {
                let _ = writeln!(glsl, "    {ty} {};", u.name);
            }
        }
        glsl.push_str("};\n");
    }
    for s in &samplers {
        let tex = if s.array { "texture2DArray" } else { "texture2D" };
        let _ = writeln!(glsl, "layout(set = 1, binding = {}) uniform {tex} {};", s.slot * 2, s.name);
        let _ = writeln!(glsl, "layout(set = 1, binding = {}) uniform sampler {}_smp;", s.slot * 2 + 1, s.name);
    }

    let varying_location = |name: &str| varyings.iter().filter(|v| v.name.starts_with("v_")).position(|v| v.name == name);
    let find = |name: &str| varyings.iter().find(|v| v.name == name);
    let mut attributes = Vec::new();
    match stage {
        Stage::Vertex => {
            for name in &inputs {
                let v = find(name).ok_or_else(|| format!("$input {name} not in varying.def.sc"))?;
                let loc = attribute_location(&v.semantic).ok_or_else(|| format!("unknown semantic {}", v.semantic))?;
                let _ = writeln!(glsl, "layout(location = {loc}) in {} {};", v.ty, v.name);
                attributes.push(AttributeInfo { name: v.name.clone(), semantic: v.semantic.clone(), location: loc });
            }
            for name in &outputs {
                let v = find(name).ok_or_else(|| format!("$output {name} not in varying.def.sc"))?;
                let loc = varying_location(name).ok_or_else(|| format!("{name} is not a v_ varying"))?;
                let _ = writeln!(glsl, "layout(location = {loc}) out {} {};", v.ty, v.name);
            }
        }
        Stage::Fragment => {
            for name in &inputs {
                let v = find(name).ok_or_else(|| format!("$input {name} not in varying.def.sc"))?;
                let loc = varying_location(name).ok_or_else(|| format!("{name} is not a v_ varying"))?;
                let _ = writeln!(glsl, "layout(location = {loc}) in {} {};", v.ty, v.name);
            }
            glsl.push_str("layout(location = 0) out vec4 zhbo_FragColor;\n");
        }
    }
    glsl.push_str("#line 1\n");
    glsl.push_str(&body);

    samplers.sort_by_key(|s| s.slot);
    Ok(Translated { stage, glsl, uniforms, uniform_block_size: block_size, samplers, attributes })
}

/// The `@group(0)` binding of a stage's uniform block.
pub fn uniform_binding(stage: Stage) -> u32 {
    match stage {
        Stage::Vertex => 0,
        Stage::Fragment => 1,
    }
}

/// Compile translated GLSL with naga and validate it.
pub fn compile(t: &Translated) -> Result<(naga::Module, naga::valid::ModuleInfo), String> {
    let stage = match t.stage {
        Stage::Vertex => naga::ShaderStage::Vertex,
        Stage::Fragment => naga::ShaderStage::Fragment,
    };
    let mut frontend = naga::front::glsl::Frontend::default();
    let module = frontend
        .parse(&naga::front::glsl::Options::from(stage), &t.glsl)
        .map_err(|e| format!("GLSL parse failed: {}", e.emit_to_string(&t.glsl)))?;
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .map_err(|e| format!("validation failed: {}", e.emit_to_string(&t.glsl)))?;
    Ok((module, info))
}

/// Uniform name -> (offset, kind, count), for `setUniform`.
pub fn uniform_map(t: &Translated) -> BTreeMap<String, (u32, UniformKind, u32)> {
    t.uniforms.iter().map(|u| (u.name.clone(), (u.offset, u.kind, u.count))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARYINGS: &str = "\
vec4 v_color0    : COLOR0    = vec4(1.0, 1.0, 1.0, 1.0);
vec2 v_texcoord0 : TEXCOORD0 = vec2(0.0, 0.0);
vec3 a_position  : POSITION;
vec4 a_color0    : COLOR0;
vec2 a_texcoord0 : TEXCOORD0;
";

    const VS: &str = "\
$input a_position, a_color0, a_texcoord0
$output v_color0, v_texcoord0
#include <bgfx_shader.sh>
uniform vec4 u_tint;
void main()
{
    gl_Position = mul(u_modelViewProj, vec4(a_position, 1.0));
    v_color0 = a_color0 * u_tint;
    v_texcoord0 = a_texcoord0;
}
";

    const FS: &str = "\
$input v_color0, v_texcoord0
#include <bgfx_shader.sh>
SAMPLER2D(s_tex0, 0);
uniform vec4 u_params[2];
void main()
{
    vec4 c = texture2D(s_tex0, v_texcoord0) * v_color0;
    if (c.a < u_params[1].x) discard;
    gl_FragColor = vec4(saturate(c.rgb), c.a);
}
";

    #[test]
    fn translates_and_validates_a_small_pair() {
        let v = parse_varying_def(VARYINGS);
        assert_eq!(v.len(), 5);
        let vs = translate(VS, &v, Stage::Vertex, &[]).unwrap();
        compile(&vs).unwrap_or_else(|e| panic!("{e}\n{}", vs.glsl));
        assert_eq!(vs.uniforms[0].name, "u_modelViewProj");
        assert_eq!(vs.uniforms[1].offset, 64, "u_tint after the mat4");
        assert_eq!(vs.attributes.iter().map(|a| a.location).collect::<Vec<_>>(), vec![0, 4, 10]);

        let fs = translate(FS, &v, Stage::Fragment, &[]).unwrap();
        compile(&fs).unwrap_or_else(|e| panic!("{e}\n{}", fs.glsl));
        assert_eq!(fs.samplers, vec![SamplerInfo { name: "s_tex0".into(), slot: 0, array: false }]);
        assert_eq!(uniform_map(&fs)["u_params"], (0, UniformKind::Vec4, 2));
    }

    #[test]
    fn rewrites_only_declared_samplers_and_whole_words() {
        let s = vec![SamplerInfo { name: "s_a".into(), slot: 3, array: false }];
        let out = rewrite_sampling("x = texture2D(s_a, uv) + mytexture2D(s_a, uv);", "texture2D", "texture", &s);
        assert_eq!(out, "x = texture(sampler2D(s_a, s_a_smp), uv) + mytexture2D(s_a, uv);");
        assert_eq!(replace_word("gl_FragColor = gl_FragColorX;", "gl_FragColor", "o"), "o = gl_FragColorX;");
    }

    #[test]
    fn splits_sampler_parameters_and_their_calls() {
        let src = "#define SAMPLE(uv) blur(s_tex0, uv)\nvec4 blur(sampler2D smp, vec2 uv)\n{ return texture2D(smp, uv); }\n";
        let (out, names) = split_sampler_parameters(src);
        assert_eq!(names, vec!["smp"]);
        assert!(out.contains("vec4 blur(texture2D smp, sampler smp_smp, vec2 uv)"), "{out}");
        assert!(out.contains("blur(s_tex0, s_tex0_smp, uv)"), "{out}");
    }

    /// Every shader the engine's render backend uses, from the pinned engine checkout.
    #[test]
    fn engine_shaders_translate_and_validate() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../host/GeneralsGameCode/Core/Libraries/Source/WWVegas/WW3D2/shaders");
        let Ok(def) = std::fs::read_to_string(dir.join("varying.def.sc")) else {
            eprintln!("engine submodule not checked out; skipping");
            return;
        };
        let varyings = parse_varying_def(&def);
        let mut failures = Vec::new();
        let mut count = 0;
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".sc") || name == "varying.def.sc" {
                continue;
            }
            let stage = if name.starts_with("vs_") { Stage::Vertex } else { Stage::Fragment };
            let src = std::fs::read_to_string(&path).unwrap();
            let mut variants: Vec<Vec<(&str, &str)>> = vec![vec![]];
            if name == "fs_uber.sc" {
                variants.push(vec![("GGC_UBER_STAGE0_ARRAY", "1")]);
                variants.push(vec![("GGC_UBER_FRAME_TEXTURE", "1")]);
            }
            if name == "vs_uber.sc" {
                variants.push(vec![("GGC_UBER_STAGE0_ARRAY", "1")]);
            }
            for defs in variants {
                count += 1;
                let result = translate(&src, &varyings, stage, &defs).and_then(|t| {
                    let (module, info) = compile(&t)?;
                    // Code generation must work too, not just validation.
                    naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
                        .map(|_| ())
                        .map_err(|e| format!("WGSL generation failed: {e}"))
                });
                if let Err(e) = result {
                    failures.push(format!("{name} {defs:?}: {e}"));
                }
            }
        }
        assert!(count >= 25, "found only {count} shader variants");
        assert!(failures.is_empty(), "{} of {count} failed:\n{}", failures.len(), failures.join("\n\n"));
    }
}
