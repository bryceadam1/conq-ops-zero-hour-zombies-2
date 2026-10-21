//! The native BO2 model pass without a window: skinning moves vertices with their bones, the
//! colour survives the sRGB decode/encode round trip, lighting lights, the pass runs before the
//! view it is queued before (or at the end of the frame when that view never comes), and bad
//! input is refused and counted, never fatal.
//!
//! Skips on machines without a usable adapter (set ZHBO_GFX_FALLBACK=1 for the software one).

use std::sync::{Arc, Mutex};

use zhbo_gfx::runtime::{InitDesc, NativeBlend, NativeDraw, NativeFrame, NativeLight, NativeSurface, NativeVertex, Renderer, WindowKind, INVALID};
use zhbo_gfx::state;

const W: u32 = 64;
const H: u32 = 64;
const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

type Shot = Arc<Mutex<Option<Vec<u8>>>>;
type Log = Arc<Mutex<Vec<String>>>;

fn renderer() -> Option<(Renderer, Log, Shot)> {
    let desc = InitDesc { kind: WindowKind::Headless, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: W, height: H, reset: 0, transient_vb_size: 0, transient_ib_size: 0 };
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
    r.set_screenshot_fn(Box::new(move |_, _, _, _, data| *shot2.lock().unwrap() = Some(data.to_vec())));
    for v in [1u16, 2] {
        r.set_view_rect(v, 0, 0, W as u16, H as u16);
    }
    r.set_view_clear(1, state::CLEAR_COLOR | state::CLEAR_DEPTH, 0x000000ff, 1.0, 0);
    r.touch(1);
    Some((r, log, shot))
}

/// Pixels (y down) to clip space, column-major, z = 0.5.
fn ortho() -> [f32; 16] {
    let mut m = IDENTITY;
    m[0] = 2.0 / W as f32;
    m[5] = -2.0 / H as f32;
    m[10] = 0.0;
    m[12] = -1.0;
    m[13] = 1.0;
    m[14] = 0.5;
    m
}

fn translate(x: f32, y: f32) -> [f32; 16] {
    let mut m = IDENTITY;
    m[12] = x;
    m[13] = y;
    m
}

/// A quad over pixels (x0, y0)-(x1, y1), facing the viewer, skinned wholly to `bone`.
fn quad(x0: f32, y0: f32, x1: f32, y1: f32, bone: u16) -> [NativeVertex; 4] {
    [(x0, y0), (x0, y1), (x1, y1), (x1, y0)].map(|(x, y)| NativeVertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, -1.0],
        uv: [0.5, 0.5],
        bones: [bone, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
    })
}

fn frame(lights: u32) -> NativeFrame {
    NativeFrame {
        view_proj: ortho(),
        eye: [32.0, 32.0, -100.0],
        ambient: [0.1, 0.1, 0.1],
        lights: [NativeLight { direction: [0.0, 0.0, -1.0], color: [1.0, 1.0, 1.0] }; 3],
        light_count: lights,
        debug: 0.0,
    }
}

fn texture(r: &mut Renderer, rgba: [u8; 4]) -> u16 {
    let data: Vec<u8> = std::iter::repeat_n(rgba, 16).flatten().collect();
    r.native_create_texture(4, 4, &data, true)
}

fn surface(color: u16, emissive: bool, count: u32) -> NativeSurface {
    NativeSurface { first_index: 0, index_count: count, blend: NativeBlend::Opaque, emissive, color, maps: INVALID, sort_key: 0 }
}

/// BGR at (x, y) of the last screenshot.
fn pixel(shot: &Shot, x: u32, y: u32) -> [u8; 3] {
    let px = shot.lock().unwrap().clone().expect("a screenshot");
    let i = ((y * W + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2]]
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(&b).all(|(x, y)| x.abs_diff(*y) <= 3)
}

#[test]
fn skins_by_bone_and_keeps_the_colour() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let color = texture(&mut r, [128, 64, 32, 255]);
    // Left quad on bone 0, right quad on bone 1; the palette moves bone 1 down by 32 pixels.
    let mut verts = quad(0.0, 0.0, 16.0, 16.0, 0).to_vec();
    verts.extend(quad(32.0, 0.0, 48.0, 16.0, 1));
    let indices = [0u16, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    let mesh = r.native_create_mesh(&verts, &indices, &[surface(color, true, 12)]);
    assert_ne!(mesh, INVALID);
    let palette = [IDENTITY, translate(0.0, 32.0)];
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &palette, frame: frame(0) }));
    r.request_screenshot(INVALID, "native.png");
    r.frame();
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
    assert!(close(pixel(&shot, 8, 8), [32, 64, 128]), "bone 0's quad, colour round-tripped through sRGB: {:?}", pixel(&shot, 8, 8));
    assert!(close(pixel(&shot, 40, 40), [32, 64, 128]), "bone 1's quad moved down: {:?}", pixel(&shot, 40, 40));
    assert!(close(pixel(&shot, 40, 8), [0, 0, 0]), "nothing left where bone 1's quad was: {:?}", pixel(&shot, 40, 8));
    assert_eq!(r.native_live(), (1, 1));
}

#[test]
fn light_brightens_a_lit_surface() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let color = texture(&mut r, [200, 200, 200, 255]);
    let mut verts = quad(0.0, 0.0, 16.0, 16.0, 0).to_vec();
    verts.extend(quad(32.0, 0.0, 48.0, 16.0, 0));
    let left = r.native_create_mesh(&verts[..4], &[0, 1, 2, 0, 2, 3], &[surface(color, false, 6)]);
    let right = r.native_create_mesh(&verts[4..], &[0, 1, 2, 0, 2, 3], &[surface(color, false, 6)]);
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh: left, palette: &[IDENTITY], frame: frame(0) }));
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh: right, palette: &[IDENTITY], frame: frame(1) }));
    r.request_screenshot(INVALID, "lit.png");
    r.frame();
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
    let (dark, lit) = (pixel(&shot, 8, 8), pixel(&shot, 40, 8));
    assert!(lit[0] > dark[0] + 40, "lit {lit:?} vs ambient only {dark:?}");
    assert!(dark[0] > 0, "ambient still shows: {dark:?}");
}

fn draw(mesh: u16, palette: &[[f32; 16]], view: u16) -> NativeDraw<'_> {
    NativeDraw { target_view: view, before_view: 2, mesh, palette, frame: frame(0) }
}

#[test]
fn refuses_bad_input_without_errors() {
    let Some((mut r, log, _)) = renderer() else { return };
    let verts = quad(0.0, 0.0, 16.0, 16.0, 3);
    assert_eq!(r.native_create_mesh(&verts, &[0, 1, 9], &[surface(INVALID, true, 3)]), INVALID, "index past the vertices");
    assert_eq!(r.native_create_mesh(&verts, &[0, 1, 2], &[surface(INVALID, true, 6)]), INVALID, "surface past the indices");
    assert_eq!(r.native_create_mesh(&verts, &[0, 1, 2], &[surface(0x1234, true, 3)]), INVALID, "a texture that never was");
    let mut bad = verts;
    bad[0].bones[0] = 300;
    assert_eq!(r.native_create_mesh(&bad, &[0, 1, 2], &[surface(INVALID, true, 3)]), INVALID, "bone past the palette's cap");
    assert_eq!(r.native_create_texture(4, 4, &[0; 10], true), INVALID, "short texture data");
    let mesh = r.native_create_mesh(&verts, &[0, 1, 2, 0, 2, 3], &[surface(INVALID, true, 6)]);
    assert_ne!(mesh, INVALID);
    assert!(!r.native_submit(&draw(mesh, &[IDENTITY; 3], 1)), "skinned to bone 3, palette of 3");
    assert!(!r.native_submit(&draw(mesh, &[IDENTITY; 4], 999)), "view out of range");
    let mut nan = IDENTITY;
    nan[3] = f32::NAN;
    assert!(!r.native_submit(&draw(mesh, &[nan; 4], 1)), "non-finite palette");
    r.native_destroy_mesh(mesh);
    assert!(!r.native_submit(&draw(mesh, &[IDENTITY; 4], 1)), "destroyed mesh");
    r.frame();
    assert_eq!(r.stats().native_skipped, 4);
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
}

#[test]
fn runs_before_its_view_or_at_the_end() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let red = texture(&mut r, [255, 0, 0, 255]);
    let mesh = r.native_create_mesh(&quad(0.0, 0.0, 64.0, 64.0, 0), &[0, 1, 2, 0, 2, 3], &[surface(red, true, 6)]);
    // View 2 clears the left half blue after the native pass that runs before it; the native
    // pass queued before an unused view runs at the end, over everything.
    r.set_view_rect(2, 0, 0, 32, H as u16);
    r.set_view_clear(2, state::CLEAR_COLOR, 0x0000ffff, 1.0, 0);
    r.touch(2);
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &[IDENTITY], frame: frame(0) }));
    r.request_screenshot(INVALID, "order.png");
    r.frame();
    assert!(close(pixel(&shot, 8, 8), [255, 0, 0]), "view 2 drew after the pass: {:?}", pixel(&shot, 8, 8));
    assert!(close(pixel(&shot, 48, 8), [0, 0, 255]), "the pass drew outside view 2: {:?}", pixel(&shot, 48, 8));

    r.touch(1);
    r.touch(2);
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 200, mesh, palette: &[IDENTITY], frame: frame(0) }));
    r.request_screenshot(INVALID, "end.png");
    r.frame();
    assert!(close(pixel(&shot, 8, 8), [0, 0, 255]), "the end-of-frame pass drew last: {:?}", pixel(&shot, 8, 8));
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
}
