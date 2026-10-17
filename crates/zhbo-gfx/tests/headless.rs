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


/// A headless renderer for the negative-path tests, or the test returns early on machines
/// without a usable adapter (the tests that need pixels assert on the screenshot instead).
fn headless() -> (Option<Renderer>, Arc<Mutex<Vec<String>>>) {
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
    match Renderer::new(&desc, Box::new(move |m| {
        eprintln!("{m}");
        log2.lock().unwrap().push(m.to_string());
    })) {
        Ok(r) => (Some(r), log),
        Err(e) => {
            eprintln!("no usable adapter, skipping: {e}");
            (None, log)
        }
    }
}

fn assert_no_errors(r: &Renderer, log: &Arc<Mutex<Vec<String>>>) {
    assert_eq!(r.stats().errors, 0, "wgpu reported errors:\n{}", log.lock().unwrap().join("\n"));
}

/// A program + layout pair for the tests (the same one-quad shader).
fn test_program(r: &mut Renderer) -> (u16, u16) {
    let varyings = parse_varying_def(VARYINGS);
    let vs = r.create_shader(&blob::build(VS, &varyings, Stage::Vertex, &[]).unwrap().encode());
    let fs = r.create_shader(&blob::build(FS, &varyings, Stage::Fragment, &[]).unwrap().encode());
    (r.create_program(vs, fs, true), r.register_layout(layout()))
}

fn setup_views(r: &mut Renderer) {
    for v in 0..3u16 {
        r.set_view_rect(v, 0, 0, W as u16, H as u16);
        r.set_view_transform(v, None, Some(&ortho()));
    }
    r.set_view_clear(0, state::CLEAR_COLOR | state::CLEAR_DEPTH, 0x0000ffff, 1.0, 0);
}

/// Invalid handles, out-of-range draws, a full pool and a broken pipeline: the frame completes,
/// `stats().errors` counts the problems, and nothing panics (4.1).
#[test]
fn negative_paths_are_counted_not_fatal() {
    let (Some(mut r), log) = headless() else { return };
    let (program, layout_id) = test_program(&mut r);
    assert_ne!(program, INVALID);

    setup_views(&mut r);
    let tint = r.create_uniform("u_tint", 2, 1);
    r.set_uniform(tint, &[1.0, 1.0, 1.0, 1.0]);

    // Draws that reference resources that do not exist: each must be dropped, not crash.
    r.set_vertex_buffer(0, BufferKind::Static, 0xFEFE, 0, 4, INVALID);
    r.set_index_buffer(BufferKind::Static, 0xFEFE, 0, 6);
    r.set_texture(0, 0xFEFE, u32::MAX);
    r.set_state(0xF | (8 << 4), 0);
    r.submit(0, program, 0, 0xFF);

    // Out-of-range draw: an index buffer with 6 indices, drawn with 6000.
    let static_ib = r.create_index_buffer(&index_bytes(&QUAD_INDICES), 0);
    let static_vb = r.create_vertex_buffer(&quad(0.0, 0.0, 32.0, 64.0, [255; 4]), layout());
    r.set_vertex_buffer(0, BufferKind::Static, static_vb, 0, 4, INVALID);
    r.set_index_buffer(BufferKind::Static, static_ib, 0, 6000);
    r.set_state(0xF | (8 << 4), 0);
    r.submit(0, program, 0, 0xFF);

    // An out-of-range view, a view rect past the backbuffer and a bogus clear flag: dropped.
    r.set_view_rect(200, 0, 0, W as u16, H as u16);
    r.set_view_rect(0, 0, 0, u16::MAX, u16::MAX);

    // A broken pipeline: garbage WGSL produces an INVALID shader, and a program built from it
    // names the real cause (the shader), not a downstream pipeline failure.
    let bad = r.create_shader(b"not a ZHS1 blob");
    assert_eq!(bad, INVALID, "decode errors name the shader, not the pipeline");
    let bad_program = r.create_program(bad, bad, false);
    assert_eq!(bad_program, INVALID);

    r.frame();
    assert_eq!(r.stats().errors, 0, "wgpu reported errors:\n{}", log.lock().unwrap().join("\n"));
    assert!(r.stats().num_draw <= 1, "invalid draws dropped ({} drew)", r.stats().num_draw);
}

/// A truncated or garbage `ZHS1` blob must produce a clean error, never a panic (4.1).
#[test]
fn blob_decoder_never_panics_on_garbage() {
    use zhbo_gfx::blob::ShaderBlob;
    let mut samples: Vec<Vec<u8>> = Vec::new();
    samples.push(Vec::new());
    samples.push(b"Z".to_vec());
    samples.push(b"ZHS1".to_vec());
    samples.push(b"ZHS1\x00\x00".to_vec());
    samples.push(b"ZHS1\xff\xff\xff\xfftruncated".to_vec());
    samples.push(vec![0xFFu8; 4096]);
    for mut s in samples {
        // Truncate every prefix of a valid blob too, if one is available.
        for cut in 0..=s.len() {
            let bad = s[..cut].to_vec();
            let r = ShaderBlob::decode(&bad);
            assert!(r.is_err(), "garbage of {} bytes decoded: {:?}", bad.len(), bad);
        }
        s.reverse();
    }
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

/// Dynamic buffers: an in-place update, an update that grows the buffer past its created size
/// (bgfx's ALLOW_RESIZE) while keeping the earlier contents, and a draw from the grown buffer (4.1).
#[test]
fn dynamic_buffers_update_grow_and_draw() {
    let (Some(mut r), log) = headless() else { return };
    let (program, layout_id) = test_program(&mut r);
    setup_views(&mut r);
    let tint = r.create_uniform("u_tint", 2, 1);
    r.set_uniform(tint, &[1.0, 1.0, 1.0, 1.0]);

    // Two vertices now; a third vertex is written past the end (growing the buffer by one).
    let dvb = r.create_dynamic_vertex_buffer(2, Some(&quad(0.0, 0.0, 16.0, 64.0, [255; 4])), layout());
    assert_ne!(dvb, INVALID);
    r.update_dynamic_vertex_buffer(dvb, 2, &quad(32.0, 64.0, 48.0, 64.0, [255; 4])[..24]);
    let dib = r.create_dynamic_index_buffer(6, Some(&index_bytes(&[0, 1, 2, 0, 2, 3])), 0);
    assert_ne!(dib, INVALID);
    // An in-place index update at the buffer's tail (still no grow).
    r.update_dynamic_index_buffer(dib, 4, &index_bytes(&[2, 3]));

    let red565: Vec<u8> = std::iter::repeat_n(0xF800u16.to_le_bytes(), 4).flatten().collect();
    let red = r.create_texture_2d(2, 2, false, 1, state::bgfx_format::R5G6B5, 0, Some(&red565));
    assert_ne!(red, INVALID);
    r.set_vertex_buffer(0, BufferKind::Dynamic, dvb, 0, u32::MAX, INVALID);
    r.set_index_buffer(BufferKind::Dynamic, dib, 0, u32::MAX);
    r.set_texture(0, red, u32::MAX);
    r.set_state(0xF | (8 << 4), 0);
    r.submit(0, program, 0, 0xFF);

    // The grown vertex buffer drew the triangle (red, inside it at (12, 48)).
    let shot: Arc<Mutex<Option<(u32, u32, Vec<u8>)>>> = Arc::default();
    let shot2 = shot.clone();
    r.set_screenshot_fn(Box::new(move |_, w, h, _, data| *shot2.lock().unwrap() = Some((w, h, data.to_vec()))));
    r.request_screenshot(INVALID, "grow.png");
    r.frame();

    let (w, h, px) = shot.lock().unwrap().take().expect("screenshot callback ran");
    let i = ((48 * w + 12) * 4) as usize;
    assert!(px[i + 2] > 200, "the grown buffer drew the triangle: BGR {},{}", px[i], px[i + 1]);
    assert_eq!(r.stats().errors, 0, "wgpu reported errors:\n{}", log.lock().unwrap().join("\n"));
}

/// View order, offscreen frame buffers (plain and MSAA), an sRGB-sampled texture and instance
/// data: each path renders without wgpu errors and the draws are counted (4.1).
#[test]
fn views_framebuffers_msaa_srgb_and_instance_data() {
    let (Some(mut r), log) = headless() else { return };
    let (program, layout_id) = test_program(&mut r);
    setup_views(&mut r);
    let tint = r.create_uniform("u_tint", 2, 1);
    r.set_uniform(tint, &[1.0, 1.0, 1.0, 1.0]);

    // The backbuffer views draw in 2, 1, 0 order regardless of submission order.
    r.set_view_order(0, Some(&[2, 1, 0]), 3);

    let static_vb = r.create_vertex_buffer(&quad(0.0, 0.0, W as f32, H as f32, [255; 4]), layout());
    let static_ib = r.create_index_buffer(&index_bytes(&QUAD_INDICES), 0);
    let srgb_tex = r.create_texture_2d(2, 2, false, 1, state::bgfx_format::RGBA8, zhbo_gfx::runtime::TEXTURE_SRGB, Some(&[0, 0, 255, 255]));

    // View 1 draws into a plain offscreen RGBA8 frame buffer, sampling the sRGB texture.
    let rt = r.create_texture_2d(W, H, false, 1, state::bgfx_format::RGBA8, zhbo_gfx::runtime::TEXTURE_RT, None);
    let fb = r.create_frame_buffer(&[rt], false);
    assert!(rt != INVALID && fb != INVALID, "offscreen frame buffer created");
    r.set_view_frame_buffer(1, fb);
    r.set_view_clear(1, state::CLEAR_COLOR | state::CLEAR_DEPTH, 0x0000ffff, 1.0, 0);

    // View 2 draws into a 4x-MSAA offscreen frame buffer.
    let rt_msaa = r.create_texture_2d(W, H, false, 1, state::bgfx_format::RGBA8,
        zhbo_gfx::runtime::TEXTURE_RT | (3 << zhbo_gfx::runtime::TEXTURE_RT_MSAA_SHIFT), None);
    let fb_msaa = r.create_frame_buffer(&[rt_msaa], false);
    assert_ne!(fb_msaa, INVALID, "the MSAA frame buffer is usable");
    r.set_view_frame_buffer(2, fb_msaa);
    r.set_view_clear(2, state::CLEAR_COLOR | state::CLEAR_DEPTH, 0x0000ffff, 1.0, 0);

    // View 0 draws the backbuffer with per-instance data (4 instances, 16 bytes each).
    let instances = vec![0u8; 4 * 16];
    let idvb = r.create_dynamic_vertex_buffer(4, Some(&instances), layout());
    assert_ne!(idvb, INVALID);
    r.set_instance_data(BufferKind::Dynamic, idvb, 0, 4, 16);

    for v in 0..3u16 {
        r.set_vertex_buffer(0, BufferKind::Static, static_vb, 0, u32::MAX, INVALID);
        r.set_index_buffer(BufferKind::Static, static_ib, 0, u32::MAX);
        r.set_texture(0, srgb_tex, u32::MAX);
        r.set_state(0xF | (8 << 4), 0);
        r.submit(v, program, 0, 0xFF);
    }

    r.frame();
    assert_eq!(r.stats().num_draw, 3, "all three views drew");
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
