//! The shellshock screen layers headless (`runtime/shock.rs`): a blurred shock's trail decays
//! frame by frame, a flashed shock grabs its first frame and adds it and the white over later
//! ones, both on the backbuffer (plain and MSAA) and an offscreen frame buffer, and bad input is
//! refused and counted, never a wgpu error. Set `ZHBO_SHOCK_SHOTS=<dir>` to write each frame as a
//! PNG.

use std::sync::{Arc, Mutex};

use zhbo_gfx::runtime::{BufferKind, InitDesc, Renderer, ShockDraw, ShockLayer, VertexLayout, WindowKind, INVALID};
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
void main()
{
    gl_FragColor = texture2D(s_tex0, v_texcoord0) * v_color0;
}
";

const W: u32 = 96;
const H: u32 = 64;
/// The scene view, and the composite the layer runs before (as the bridge's 1 and 9).
const SCENE: u16 = 0;
const COMPOSITE: u16 = 1;

fn layout() -> VertexLayout {
    let enc = |num: u16, ty: u16, norm: u16| (norm << 7) | (ty << 3) | (num - 1);
    let mut offsets = [0u16; 18];
    let mut attrs = [u16::MAX; 18];
    (offsets[0], attrs[0]) = (0, enc(3, 6, 0));
    (offsets[4], attrs[4]) = (12, enc(4, 1, 1));
    (offsets[10], attrs[10]) = (16, enc(2, 6, 0));
    VertexLayout::from_bgfx(24, &offsets, &attrs)
}

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

fn ortho() -> [f32; 16] {
    [2.0 / W as f32, 0.0, 0.0, 0.0, 0.0, -2.0 / H as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.5, 1.0]
}

type Shot = Arc<Mutex<Option<(u32, u32, Vec<u8>)>>>;

struct Scene {
    r: Renderer,
    log: Arc<Mutex<Vec<String>>>,
    shot: Shot,
    program: u16,
    layout: u16,
    white: u16,
    ib: u16,
    /// The scene's frame buffer (`INVALID`: the backbuffer).
    fb: u16,
}

fn scene(reset: u32, offscreen: bool) -> Option<Scene> {
    let desc = InitDesc { kind: WindowKind::Headless, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: W, height: H, reset, transient_vb_size: 0, transient_ib_size: 0 };
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log2 = log.clone();
    let mut r = match Renderer::new(&desc, Box::new(move |m| {
        eprintln!("{m}");
        log2.lock().unwrap().push(m.to_string());
    })) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no usable adapter, skipping: {e}");
            return None;
        }
    };
    let shot: Shot = Arc::default();
    let shot2 = shot.clone();
    r.set_screenshot_fn(Box::new(move |_, w, h, _, data| *shot2.lock().unwrap() = Some((w, h, data.to_vec()))));
    let varyings = parse_varying_def(VARYINGS);
    let vs = r.create_shader(&blob::build(VS, &varyings, Stage::Vertex, &[]).unwrap().encode());
    let fs = r.create_shader(&blob::build(FS, &varyings, Stage::Fragment, &[]).unwrap().encode());
    let program = r.create_program(vs, fs, true);
    let _ = r.create_uniform("s_tex0", 0, 1);
    let layout = r.register_layout(layout());
    let white = r.create_texture_2d(1, 1, false, 1, state::bgfx_format::RGBA8, 0, Some(&[255; 4]));
    let ib = r.create_index_buffer(&[0u16, 1, 2, 0, 2, 3].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>(), 0);
    let fb = if offscreen {
        let rt = r.create_texture_2d(W, H, false, 1, state::bgfx_format::RGBA8, zhbo_gfx::runtime::TEXTURE_RT, None);
        r.create_frame_buffer(&[rt], false)
    } else {
        INVALID
    };
    for v in [SCENE, COMPOSITE] {
        r.set_view_rect(v, 0, 0, W as u16, H as u16);
        r.set_view_transform(v, None, Some(&ortho()));
    }
    r.set_view_frame_buffer(SCENE, fb);
    Some(Scene { r, log, shot, program, layout, white, ib, fb })
}

impl Scene {
    /// One frame: the scene cleared to `bg` (RGBA) with a `fg` box at `x`, the layer, read back.
    fn frame(&mut self, bg: u32, fg: Option<(f32, [u8; 4])>, layer: Option<ShockLayer>) -> Vec<u8> {
        let r = &mut self.r;
        r.set_view_clear(SCENE, state::CLEAR_COLOR | state::CLEAR_DEPTH, bg, 1.0, 0);
        r.touch(SCENE);
        r.touch(COMPOSITE);
        if let Some((x, rgba)) = fg {
            let vb = quad(x, 16.0, x + 24.0, 48.0, rgba);
            let (ptr, start) = r.transient_alloc(false, 4, 24).unwrap();
            unsafe { std::ptr::copy_nonoverlapping(vb.as_ptr(), ptr, vb.len()) };
            r.set_vertex_buffer(0, BufferKind::Transient { index32: false }, self.layout, start, 4, INVALID);
            r.set_index_buffer(BufferKind::Static, self.ib, 0, u32::MAX);
            r.set_texture(0, self.white, u32::MAX);
            r.set_state(0xF | (8 << 4), 0);
            r.submit(SCENE, self.program, 0, 0xFF);
        }
        if let Some(layer) = layer {
            assert!(r.shock_submit(&ShockDraw { target_view: SCENE, before_view: COMPOSITE, layer }));
        }
        r.request_screenshot(self.fb, "shock.png");
        r.frame();
        let (w, h, px) = self.shot.lock().unwrap().take().expect("screenshot");
        assert_eq!((w, h), (W, H));
        px
    }

    fn no_errors(&self) {
        assert_eq!(self.r.stats().errors, 0, "wgpu errors:\n{}", self.log.lock().unwrap().join("\n"));
        assert_eq!(self.r.stats().native_skipped, 0);
    }
}

/// BGR at (x, y) of a BGRA screenshot.
fn at(px: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2]]
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter().zip(&want).all(|(a, b)| a.abs_diff(*b) <= 3)
}

/// Write the frames side by side as `<ZHBO_SHOCK_SHOTS>/<name>.png`.
fn save(name: &str, frames: &[Vec<u8>]) {
    let Ok(dir) = std::env::var("ZHBO_SHOCK_SHOTS") else { return };
    let n = frames.len() as u32;
    let mut rgba = vec![0u8; (W * n * H * 4) as usize];
    for (k, f) in frames.iter().enumerate() {
        for y in 0..H {
            for x in 0..W {
                let s = ((y * W + x) * 4) as usize;
                let d = ((y * W * n + k as u32 * W + x) * 4) as usize;
                rgba[d..d + 4].copy_from_slice(&[f[s + 2], f[s + 1], f[s], 255]);
            }
        }
    }
    let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}.png"))).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W * n, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
}

/// A box sweeping right under a blurred shock: the first frame only saves; each later one lays
/// the saved screen over the live one at `alpha`, so the box leaves a trail that halves each
/// frame at 0.5 (`shellshock`: SRC_ALPHA, INV_SRC_ALPHA, then saved again).
fn blurred_trail(reset: u32, offscreen: bool, name: &str) {
    let Some(mut s) = scene(reset, offscreen) else { return };
    let red = [255, 0, 0, 255];
    let layer = Some(ShockLayer::Blurred { alpha: 0.5 });
    let mut frames = Vec::new();
    for k in 0..4 {
        frames.push(s.frame(0x0000_00ff, Some((8.0 + 20.0 * k as f32, red)), layer));
    }
    save(name, &frames);
    // Frame 0 is the live frame: red box at 8..32, black elsewhere.
    assert!(near(at(&frames[0], 20, 32), [0, 0, 255]), "{:?}", at(&frames[0], 20, 32));
    // Frame 1: the box moved to 28..52; where only the old one was (x = 12), half its red stays.
    assert!(near(at(&frames[1], 12, 32), [0, 0, 128]), "{:?}", at(&frames[1], 12, 32));
    // Frame 2: a quarter there, half at the second box's old place (x = 32 is 0.5·(0.5·red + 0.5·red)).
    assert!(near(at(&frames[2], 12, 32), [0, 0, 64]), "{:?}", at(&frames[2], 12, 32));
    // The live box itself is half its new self and half what was saved there.
    let live = at(&frames[3], 76, 32);
    assert!(live[2] >= 127 && live[0] < 4 && live[1] < 4, "{live:?}");
    s.no_errors();
}

#[test]
fn a_blurred_shock_trails_the_view_on_the_backbuffer() {
    blurred_trail(0, false, "blurred_backbuffer");
}

#[test]
fn a_blurred_shock_trails_the_view_with_msaa() {
    // RESET_MSAA_X4: the target is multisampled and resolves into the copied texture.
    blurred_trail(2 << zhbo_gfx::runtime::RESET_MSAA_SHIFT, false, "blurred_msaa");
}

#[test]
fn a_blurred_shock_trails_an_offscreen_scene() {
    blurred_trail(0, true, "blurred_offscreen");
}

/// A blurred shock that ends and another that starts later: the new one's first frame lays nothing
/// stale over the view (its trail restarts), as the old saved screen is not the frame before.
#[test]
fn a_new_blurred_shock_starts_a_fresh_trail() {
    let Some(mut s) = scene(0, false) else { return };
    let red = Some((8.0, [255, 0, 0, 255]));
    s.frame(0x0000_00ff, red, Some(ShockLayer::Blurred { alpha: 0.5 }));
    s.frame(0x0000_00ff, None, None);
    let px = s.frame(0x0000_00ff, None, Some(ShockLayer::Blurred { alpha: 0.9 }));
    assert!(near(at(&px, 20, 32), [0, 0, 0]), "no stale red: {:?}", at(&px, 20, 32));
    s.no_errors();
}

/// A flashed shock: the first frame is grabbed and left as it is; later frames add the grab at
/// `shot` and the white (`shellshock_flashed`: ONE, ONE, `grab · shot + white`), so the frozen
/// image of the first frame ghosts over the live view while both fade.
#[test]
fn a_flashed_shock_grabs_its_first_frame_and_adds_it_and_the_white() {
    let Some(mut s) = scene(0, false) else { return };
    let green = [0, 255, 0, 255];
    let mut frames = Vec::new();
    frames.push(s.frame(0x2020_20ff, Some((8.0, green)), Some(ShockLayer::Grab)));
    // The grab frame draws nothing.
    assert!(near(at(&frames[0], 20, 32), [0, 255, 0]) && near(at(&frames[0], 60, 32), [32, 32, 32]));
    for (white, shot) in [(1.0, 1.0), (0.5, 0.75), (0.2, 0.5), (0.0, 0.25)] {
        frames.push(s.frame(0x2020_20ff, Some((60.0, [0, 0, 255, 255])), Some(ShockLayer::Flashed { white, shot })));
    }
    save("flashed", &frames);
    let w = |v: f32| (v * 255.0).round() as u8;
    // Full white saturates.
    assert!(near(at(&frames[1], 20, 32), [255, 255, 255]));
    // Grey background + 0.75·grey grab + 0.5 white; the old green box ghosts over grey.
    let g = 32u8;
    let px = at(&frames[2], 20, 32);
    assert!(near(px, [g + w(0.5), 255, g + w(0.5)]), "{px:?}");
    // The live blue box (x 60..84) under the grab's grey and the white.
    let px = at(&frames[3], 70, 32);
    let grey = (g as f32 * 0.5).round() as u8 + w(0.2);
    assert!(near(px, [255, grey, grey]), "{px:?}");
    // No white left: only a quarter of the frozen frame added.
    let px = at(&frames[4], 20, 32);
    assert!(near(px, [g, g + w(0.25), g]), "{px:?}");
    s.no_errors();
}

/// Refusals: a non-finite or out-of-range value, a view out of range; each counted as skipped,
/// none reaching wgpu, and a frame with no layer queued draws nothing extra.
#[test]
fn bad_shock_layers_are_refused_not_fatal() {
    let Some(mut s) = scene(0, false) else { return };
    let r = &mut s.r;
    for layer in [ShockLayer::Blurred { alpha: f32::NAN }, ShockLayer::Blurred { alpha: 1.5 }, ShockLayer::Flashed { white: -0.1, shot: 0.0 }, ShockLayer::Flashed { white: 0.0, shot: f32::INFINITY }] {
        assert!(!r.shock_submit(&ShockDraw { target_view: SCENE, before_view: COMPOSITE, layer }));
    }
    assert!(!r.shock_submit(&ShockDraw { target_view: 300, before_view: COMPOSITE, layer: ShockLayer::Grab }));
    assert!(!r.shock_submit(&ShockDraw { target_view: INVALID, before_view: COMPOSITE, layer: ShockLayer::Grab }));
    r.touch(SCENE);
    r.frame();
    assert_eq!(r.stats().native_skipped, 6, "every refusal counted");
    assert_eq!(r.stats().errors, 0);
    assert_eq!(r.shock_drawn(), 0, "nothing ran");
    // A layer whose view has only a depth target is skipped at encode, without a wgpu error.
    let depth = r.create_texture_2d(W, H, false, 1, state::bgfx_format::D24S8, zhbo_gfx::runtime::TEXTURE_RT, None);
    let fb = r.create_frame_buffer(&[depth], false);
    r.set_view_frame_buffer(2, fb);
    r.set_view_rect(2, 0, 0, W as u16, H as u16);
    r.touch(2);
    assert!(r.shock_submit(&ShockDraw { target_view: 2, before_view: COMPOSITE, layer: ShockLayer::Grab }));
    r.touch(COMPOSITE);
    r.frame();
    assert_eq!(r.stats().native_skipped, 1);
    assert_eq!(r.stats().errors, 0, "{}", s.log.lock().unwrap().join("\n"));
}
