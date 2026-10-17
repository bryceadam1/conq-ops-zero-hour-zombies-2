//! End-to-end check of the renderer without a window: shaders through the build-time blob path,
//! transient and static buffers, BC1 and packed-565 textures, model transforms, a partial view
//! clear, alpha blending and the screenshot readback, verified pixel by pixel.
//!
//! Runs on whatever adapter wgpu finds (set ZHBO_GFX_FALLBACK=1 for the software one); skips if
//! there is none.

use std::sync::{Arc, Mutex};

use zhbo_gfx::runtime::{BufferKind, InitDesc, Renderer, VertexLayout, WindowKind, INVALID};
use zhbo_gfx::shader::{parse_varying_def, Stage};
use zhbo_gfx::{blob, state};

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
void main()
{
    gl_Position = mul(u_modelViewProj, vec4(a_position, 1.0));
    v_color0 = a_color0;
    v_texcoord0 = a_texcoord0;
}
";

const FS: &str = "\
$input v_color0, v_texcoord0
#include <bgfx_shader.sh>
SAMPLER2D(s_tex0, 0);
uniform vec4 u_tint;
void main()
{
    gl_FragColor = texture2D(s_tex0, v_texcoord0) * v_color0 * u_tint;
}
";

const W: u32 = 64;
const H: u32 = 64;

fn layout() -> VertexLayout {
    // bgfx's attribute encoding: (normalized << 7) | (type << 3) | (num - 1).
    let enc = |num: u16, ty: u16, norm: u16| (norm << 7) | (ty << 3) | (num - 1);
    let mut offsets = [0u16; 18];
    let mut attrs = [u16::MAX; 18];
    (offsets[0], attrs[0]) = (0, enc(3, 6, 0)); // POSITION float3
    (offsets[4], attrs[4]) = (12, enc(4, 1, 1)); // COLOR0 uint8x4 normalized
    (offsets[10], attrs[10]) = (16, enc(2, 6, 0)); // TEXCOORD0 float2
    VertexLayout::from_bgfx(24, &offsets, &attrs)
}

/// A quad from (x0, y0) to (x1, y1) in pixels, colour as RGBA bytes.
fn quad(x0: f32, y0: f32, x1: f32, y1: f32, rgba: [u8; 4]) -> Vec<u8> {
    let mut v = Vec::new();
    for (x, y, u, t) in [(x0, y0, 0.0f32, 0.0f32), (x1, y0, 1.0, 0.0), (x1, y1, 1.0, 1.0), (x0, y1, 0.0, 1.0)] {
        for f in [x, y, 0.0f32] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&rgba);
        for f in [u, t] {
            v.extend_from_slice(&f.to_le_bytes());
        }
    }
    v
}

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

/// Row-vector ortho: pixels (y down) to NDC, z = 0.5.
fn ortho() -> [f32; 16] {
    [2.0 / W as f32, 0.0, 0.0, 0.0, 0.0, -2.0 / H as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.5, 1.0]
}

fn index_bytes(i: &[u16]) -> Vec<u8> {
    i.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[test]
fn draws_a_frame_and_reads_it_back() {
    let desc = InitDesc {
        kind: WindowKind::Headless,
        window: std::ptr::null_mut(),
        display: std::ptr::null_mut(),
        width: W,
        height: H,
        reset: 0,
        transient_vb_size: 0,
        transient_ib_size: 0,
    };
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log2 = log.clone();
    let mut r = match Renderer::new(&desc, Box::new(move |m| {
        eprintln!("{m}");
        log2.lock().unwrap().push(m.to_string());
    })) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no usable adapter, skipping: {e}");
            return;
        }
    };
    let shot: Arc<Mutex<Option<(u32, u32, Vec<u8>)>>> = Arc::default();
    let shot2 = shot.clone();
    r.set_screenshot_fn(Box::new(move |_, w, h, _, data| *shot2.lock().unwrap() = Some((w, h, data.to_vec()))));

    let varyings = parse_varying_def(VARYINGS);
    let vs = r.create_shader(&blob::build(VS, &varyings, Stage::Vertex, &[]).unwrap().encode());
    let fs = r.create_shader(&blob::build(FS, &varyings, Stage::Fragment, &[]).unwrap().encode());
    let program = r.create_program(vs, fs, true);
    assert_ne!(program, INVALID);
    let s_tex0 = r.create_uniform("s_tex0", 0, 1);
    let u_tint = r.create_uniform("u_tint", 2, 1);
    let _ = s_tex0;

    // 2x2 packed R5G6B5 red, and a 4x4 BC1 block of solid green.
    let red565: Vec<u8> = std::iter::repeat_n(0xF800u16.to_le_bytes(), 4).flatten().collect();
    let red = r.create_texture_2d(2, 2, false, 1, state::bgfx_format::R5G6B5, 0, Some(&red565));
    let green_bc1 = [0xE0, 0x07, 0xE0, 0x07, 0, 0, 0, 0];
    let green = r.create_texture_2d(4, 4, false, 1, state::bgfx_format::BC1, 0, Some(&green_bc1));
    assert!(red != INVALID && green != INVALID);

    let layout_id = r.register_layout(layout());
    let static_vb = r.create_vertex_buffer(&quad(0.0, 0.0, 32.0, 64.0, [255; 4]), layout());
    let static_ib = r.create_index_buffer(&index_bytes(&QUAD_INDICES), 0);

    for v in 0..3u16 {
        r.set_view_rect(v, 0, 0, W as u16, H as u16);
        r.set_view_transform(v, None, Some(&ortho()));
    }
    r.set_view_clear(0, state::CLEAR_COLOR | state::CLEAR_DEPTH, 0x0000ffff, 1.0, 0);
    r.set_uniform(u_tint, &[1.0, 1.0, 1.0, 1.0]);

    // View 0, left half: red texture through transient buffers.
    let vb = quad(0.0, 0.0, 32.0, 64.0, [255; 4]);
    let (ptr, start) = r.transient_alloc(false, 4, 24).unwrap();
    unsafe { std::ptr::copy_nonoverlapping(vb.as_ptr(), ptr, vb.len()) };
    let ib = index_bytes(&QUAD_INDICES);
    let (iptr, istart) = r.transient_alloc(true, 6, 2).unwrap();
    unsafe { std::ptr::copy_nonoverlapping(ib.as_ptr(), iptr, ib.len()) };
    r.set_vertex_buffer(0, BufferKind::Transient { index32: false }, layout_id, start, 4, INVALID);
    r.set_index_buffer(BufferKind::Transient { index32: false }, 0, istart, 6);
    r.set_texture(0, red, u32::MAX);
    r.set_state(0xF | (8 << 4), 0); // RGBA writes, depth test ALWAYS, no cull
    r.submit(0, program, 0, 0xFF);

    // View 0, right half: the same static quad moved by a model transform, green BC1 texture.
    let mut model = zhbo_gfx::runtime::math::IDENTITY;
    model[12] = 32.0;
    let t = r.alloc_transform(&[model]);
    r.set_transform_cached(t, 1);
    r.set_vertex_buffer(0, BufferKind::Static, static_vb, 0, u32::MAX, INVALID);
    r.set_index_buffer(BufferKind::Static, static_ib, 0, u32::MAX);
    r.set_texture(0, green, u32::MAX);
    r.set_state(0xF | (8 << 4), 0);
    r.submit(0, program, 0, 0xFF);

    // View 1: a partial clear to yellow over the red half.
    r.set_view_rect(1, 8, 8, 16, 16);
    r.set_view_clear(1, state::CLEAR_COLOR, 0xffff00ff, 1.0, 0);
    r.touch(1);

    // View 2: half-transparent black alpha-blended over the green half.
    let blend = (5u64 << 12) | (6 << 16) | (5 << 20) | (6 << 24);
    let vb = quad(40.0, 40.0, 56.0, 56.0, [0, 0, 0, 128]);
    let (ptr, start) = r.transient_alloc(false, 4, 24).unwrap();
    unsafe { std::ptr::copy_nonoverlapping(vb.as_ptr(), ptr, vb.len()) };
    r.set_vertex_buffer(0, BufferKind::Transient { index32: false }, layout_id, start, 4, INVALID);
    r.set_index_buffer(BufferKind::Static, static_ib, 0, u32::MAX);
    r.set_texture(0, green, u32::MAX);
    r.set_state(0x7 | (8 << 4) | blend, 0);
    r.submit(2, program, 0, 0xFF);

    r.request_screenshot(INVALID, "frame.png");
    r.frame();

    let (w, h, px) = shot.lock().unwrap().take().expect("screenshot callback ran");
    assert_eq!((w, h), (W, H));
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2]] // B, G, R
    };
    let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(&b).all(|(x, y)| x.abs_diff(*y) <= 3);
    let cases = [
        ("red half", (2, 40), [0, 0, 255]),
        ("yellow partial clear", (16, 16), [0, 255, 255]),
        ("red outside the clear rect", (28, 4), [0, 0, 255]),
        ("green BC1 half", (48, 8), [0, 255, 0]),
        ("50% black over green", (48, 48), [0, 127, 0]),
    ];
    let mut failed = Vec::new();
    for (name, (x, y), want) in cases {
        let got = at(x, y);
        if !close(got, want) {
            failed.push(format!("{name} at ({x},{y}): got BGR {got:?}, want {want:?}"));
        }
    }
    assert!(failed.is_empty(), "{}\nlog:\n{}", failed.join("\n"), log.lock().unwrap().join("\n"));
    assert_eq!(r.stats().errors, 0, "wgpu reported errors:\n{}", log.lock().unwrap().join("\n"));
    assert_eq!(r.stats().num_draw, 3);
}

/// A transient buffer set with "all remaining" (num == u32::MAX, legal in bgfx) must draw exactly
/// what the engine allocated, not a garbage count (0.1a).
#[test]
fn transient_all_remaining_draws() {
    let desc = InitDesc {
        kind: WindowKind::Headless,
        window: std::ptr::null_mut(),
        display: std::ptr::null_mut(),
        width: W,
        height: H,
        reset: 0,
        transient_vb_size: 0,
        transient_ib_size: 0,
    };
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log2 = log.clone();
    let mut r = match Renderer::new(&desc, Box::new(move |m| {
        eprintln!("{m}");
        log2.lock().unwrap().push(m.to_string());
    })) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no usable adapter, skipping: {e}");
            return;
        }
    };

    let varyings = parse_varying_def(VARYINGS);
    let vs = r.create_shader(&blob::build(VS, &varyings, Stage::Vertex, &[]).unwrap().encode());
    let fs = r.create_shader(&blob::build(FS, &varyings, Stage::Fragment, &[]).unwrap().encode());
    let program = r.create_program(vs, fs, true);
    assert_ne!(program, INVALID);
    let red565: Vec<u8> = std::iter::repeat_n(0xF800u16.to_le_bytes(), 4).flatten().collect();
    let red = r.create_texture_2d(2, 2, false, 1, state::bgfx_format::R5G6B5, 0, Some(&red565));
    assert_ne!(red, INVALID);
    let layout_id = r.register_layout(layout());

    r.set_view_rect(0, 0, 0, W as u16, H as u16);
    r.set_view_transform(0, None, Some(&ortho()));
    r.set_view_clear(0, state::CLEAR_COLOR | state::CLEAR_DEPTH, 0x0000ffff, 1.0, 0);
    r.set_state(0xF | (8 << 4), 0);

    // A 4-vertex quad and 6 indices, both set with num == u32::MAX ("all remaining").
    let vb = quad(0.0, 0.0, W as f32, H as f32, [255; 4]);
    let (ptr, start) = r.transient_alloc(false, 4, 24).unwrap();
    unsafe { std::ptr::copy_nonoverlapping(vb.as_ptr(), ptr, vb.len()) };
    let ib = index_bytes(&QUAD_INDICES);
    let (iptr, istart) = r.transient_alloc(true, 6, 2).unwrap();
    unsafe { std::ptr::copy_nonoverlapping(ib.as_ptr(), iptr, ib.len()) };
    r.set_vertex_buffer(0, BufferKind::Transient { index32: false }, layout_id, start, u32::MAX, INVALID);
    r.set_index_buffer(BufferKind::Transient { index32: false }, 0, istart, u32::MAX);
    r.set_texture(0, red, u32::MAX);
    r.submit(0, program, 0, 0xFF);

    // A stale transient range past the arena's used bytes is clamped, not drawn with garbage.
    r.set_vertex_buffer(0, BufferKind::Transient { index32: false }, layout_id, start + 4, u32::MAX, INVALID);
    r.set_index_buffer(BufferKind::Transient { index32: false }, 0, istart + 6, u32::MAX);
    r.set_texture(0, red, u32::MAX);
    r.submit(0, program, 0, 0xFF);

    r.frame();
    assert_eq!(r.stats().num_draw, 1, "only the in-range draw survives");
    assert_eq!(r.stats().errors, 0, "wgpu reported errors:\n{}", log.lock().unwrap().join("\n"));
}

/// A blit whose clamped region or layer range is empty is skipped with a report, never sent to
/// wgpu as a zero-size or out-of-range copy (0.1e).
#[test]
fn empty_blits_are_skipped() {
    let desc = InitDesc {
        kind: WindowKind::Headless,
        window: std::ptr::null_mut(),
        display: std::ptr::null_mut(),
        width: W,
        height: H,
        reset: 0,
        transient_vb_size: 0,
        transient_ib_size: 0,
    };
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log2 = log.clone();
    let mut r = match Renderer::new(&desc, Box::new(move |m| {
        eprintln!("{m}");
        log2.lock().unwrap().push(m.to_string());
    })) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no usable adapter, skipping: {e}");
            return;
        }
    };
    let a = r.create_texture_2d(8, 8, false, 2, state::bgfx_format::RGBA8, 0, None);
    let b = r.create_texture_2d(8, 8, false, 2, state::bgfx_format::RGBA8, 0, None);
    assert!(a != INVALID && b != INVALID);

    // Source layer range empty (sz at the last layer), zero size, and a zero-width copy.
    r.blit(0, b, 0, 0, 0, 0, a, 0, 0, 0, 1, 4, 4, 1); // valid, to keep the blit path exercised
    r.blit(0, b, 0, 0, 0, 0, a, 0, 0, 0, 1, 4, 4, 0); // zero depth
    r.blit(0, b, 0, 0, 0, 0, a, 0, 0, 0, 1, 0, 4, 1); // zero width
    r.blit(0, b, 0, 0, 0, 0, a, 0, 0, 0, 2, 4, 4, 1); // source layer out of range
    r.blit(0, b, 0, 0, 0, 0, a, 0, 8, 0, 0, 4, 4, 1); // source x past the mip
    r.frame();
    assert_eq!(r.stats().num_blit, 1, "only the valid blit is recorded");
    assert_eq!(r.stats().errors, 0, "wgpu reported errors:\n{}", log.lock().unwrap().join("\n"));
}
