//! The native BO2 model pass without a window: skinning moves vertices with their bones, an
//! emissive colour comes through exactly, dark finishes keep their hue and stay dark under Zero
//! Hour's lights, lighting lights, the pass runs before the view it is queued before (or at the
//! end of the frame when that view never comes), and bad input is refused and counted, never
//! fatal.
//!
//! Skips on machines without a usable adapter (set ZHBO_GFX_FALLBACK=1 for the software one).

use std::sync::{Arc, Mutex};

use zhbo_gfx::runtime::{InitDesc, NativeBlend, NativeCull, NativeDraw, NativeFrame, NativeLight, NativeSurface, NativeVertex, Renderer, WindowKind, INVALID};
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
/// Index it `[0, 2, 1, 0, 3, 2]`: the pass keeps BO2's index order, whose front faces are
/// clockwise in the framebuffer (the engine's content is the other way, `character.rs`).
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
        view_proj: Some(ortho()),
        eye: [32.0, 32.0, -100.0],
        ambient: [0.1, 0.1, 0.1],
        lights: [NativeLight { direction: [0.0, 0.0, -1.0], color: [1.0, 1.0, 1.0] }; 3],
        light_count: lights,
        debug: 0.0,
    }
}

fn texture(r: &mut Renderer, rgba: [u8; 4]) -> u16 {
    let data: Vec<u8> = std::iter::repeat_n(rgba, 16).flatten().collect();
    r.native_create_texture(4, 4, &data, false)
}

fn surface(color: u16, emissive: bool, count: u32) -> NativeSurface {
    NativeSurface { first_index: 0, index_count: count, blend: NativeBlend::Opaque, emissive, color, maps: INVALID, camo: INVALID, camo_uv: [0.0, 0.0, 1.0, 1.0], sort_key: 0, cull: NativeCull::None }
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
    let indices = [0u16, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6];
    let mesh = r.native_create_mesh(&verts, &indices, &[surface(color, true, 12)]);
    assert_ne!(mesh, INVALID);
    let palette = [IDENTITY, translate(0.0, 32.0)];
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &palette, frame: frame(0) }));
    r.request_screenshot(INVALID, "native.png");
    r.frame();
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
    assert!(close(pixel(&shot, 8, 8), [32, 64, 128]), "bone 0's quad, its colour exact: {:?}", pixel(&shot, 8, 8));
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
    let left = r.native_create_mesh(&verts[..4], &[0, 2, 1, 0, 3, 2], &[surface(color, false, 6)]);
    let right = r.native_create_mesh(&verts[4..], &[0, 2, 1, 0, 3, 2], &[surface(color, false, 6)]);
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

/// A dark olive BO2 finish (the AN-94's colour map averages RGB 42/39/35) with its packed maps
/// (flat normal, gloss 72, specular level 61), lit by Zero Hour's day lights, keeps its hue and
/// stays dark: the integrator's finding #1 was the native gun washing out to light grey.
#[test]
fn a_dark_finish_keeps_its_hue_under_zero_hour_light() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let olive = texture(&mut r, [70, 80, 40, 255]);
    let data: Vec<u8> = std::iter::repeat_n([128u8, 128, 72, 61], 16).flatten().collect();
    let maps = r.native_create_texture(4, 4, &data, false);
    let mut s = surface(olive, false, 6);
    s.maps = maps;
    let mesh = r.native_create_mesh(&quad(0.0, 0.0, 64.0, 64.0, 0), &[0, 2, 1, 0, 3, 2], &[s]);
    let mut f = frame(1);
    f.ambient = [0.45, 0.45, 0.47];
    f.lights[0] = NativeLight { direction: [-0.5, 0.3, -0.8], color: [1.0, 0.97, 0.9] };
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &[IDENTITY], frame: f }));
    r.request_screenshot(INVALID, "olive.png");
    r.frame();
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
    let [b, g, rr] = pixel(&shot, 32, 32);
    eprintln!("olive lit: RGB {rr} {g} {b}");
    assert!(g > b + 10 && rr > b + 5, "kept its olive hue: RGB {rr} {g} {b}");
    assert!(g < 160, "stayed a dark finish, not washed out: RGB {rr} {g} {b}");
}

/// Under a bright map ambient (Zero Hour maps go well past the GameData defaults) a dark finish
/// stays dark: the environment term must not wash it out (finding #1's real cause).
#[test]
fn a_dark_finish_stays_dark_under_a_bright_ambient() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let dark = texture(&mut r, [42, 39, 35, 213]);
    let data: Vec<u8> = std::iter::repeat_n([128u8, 126, 72, 61], 16).flatten().collect();
    let maps = r.native_create_texture(4, 4, &data, false);
    let mut s = surface(dark, false, 6);
    s.maps = maps;
    let mesh = r.native_create_mesh(&quad(0.0, 0.0, 64.0, 64.0, 0), &[0, 2, 1, 0, 3, 2], &[s]);
    let mut f = frame(1);
    f.ambient = [1.2, 1.2, 1.2];
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &[IDENTITY], frame: f }));
    r.request_screenshot(INVALID, "bright.png");
    r.frame();
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
    let [b, g, rr] = pixel(&shot, 32, 32);
    eprintln!("dark finish, bright ambient: RGB {rr} {g} {b}");
    assert!(rr.max(g).max(b) < 110, "not washed out to grey: RGB {rr} {g} {b}");
}

/// R4: a surface culls the faces its material says to. The pass keeps BO2's own index order and
/// treats it as clockwise-front (the engine's convention is the opposite, `character.rs`): two
/// quads, one wound each way — `Back` leaves the front one, `Front` the back one, `None` both.
#[test]
fn culls_the_faces_the_surface_says() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let green = texture(&mut r, [0, 200, 0, 255]);
    let mut verts = quad(0.0, 0.0, 16.0, 16.0, 0).to_vec();
    verts.extend(quad(32.0, 0.0, 48.0, 16.0, 0));
    // The left quad BO2-wound (clockwise = front here); the right one the other way.
    let indices = [0u16, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7];
    for (cull, left, right) in [(NativeCull::Back, true, false), (NativeCull::Front, false, true), (NativeCull::None, true, true)] {
        let mut s = surface(green, true, indices.len() as u32);
        s.cull = cull;
        let mesh = r.native_create_mesh(&verts, &indices, &[s]);
        assert_ne!(mesh, INVALID);
        assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &[IDENTITY], frame: frame(0) }));
        r.request_screenshot(INVALID, "cull.png");
        r.frame();
        assert_eq!(pixel(&shot, 8, 8) == [0, 200, 0], left, "{cull:?}: the front quad drawn == {left}");
        assert_eq!(pixel(&shot, 40, 8) == [0, 200, 0], right, "{cull:?}: the back quad drawn == {right}");
        r.native_destroy_mesh(mesh);
        r.touch(1);
    }
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
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
    let mesh = r.native_create_mesh(&verts, &[0, 2, 1, 0, 3, 2], &[surface(INVALID, true, 6)]);
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
fn skips_a_surface_with_no_indices() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let green = texture(&mut r, [0, 200, 0, 255]);
    // One index buffer, two surfaces: the left quad draws, and the second surface covers the right
    // quad's indices but has none — a fully hidden surface, a no-op, not a refusal.
    let mut verts = quad(0.0, 0.0, 16.0, 16.0, 0).to_vec();
    verts.extend(quad(32.0, 0.0, 48.0, 16.0, 0));
    let indices = [0u16, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6];
    let mut empty = surface(green, true, 0);
    empty.first_index = 6;
    let mesh = r.native_create_mesh(&verts, &indices, &[surface(green, true, 6), empty]);
    assert_ne!(mesh, INVALID, "an empty surface does not refuse the mesh");
    assert!(r.native_submit(&draw(mesh, &[IDENTITY], 1)));
    r.request_screenshot(INVALID, "empty-surface.png");
    r.frame();
    assert!(close(pixel(&shot, 8, 8), [0, 200, 0]), "the surface with indices still draws: {:?}", pixel(&shot, 8, 8));
    assert!(close(pixel(&shot, 40, 8), [0, 0, 0]), "the empty surface draws nothing: {:?}", pixel(&shot, 40, 8));
    r.native_destroy_mesh(mesh);
    // Nothing drawable at all: refused, so the caller falls back (the core then draws W3D).
    let mut none = surface(green, true, 0);
    none.first_index = 0;
    assert_eq!(r.native_create_mesh(&verts, &indices, &[none]), INVALID, "a mesh with no drawable surface");
    let logged = log.lock().unwrap().join("\n");
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{logged}");
    assert!(logged.contains("has no indices"), "the skip is logged:\n{logged}");
}

#[test]
fn takes_the_target_views_transform_when_given_none() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let red = texture(&mut r, [255, 0, 0, 255]);
    let mesh = r.native_create_mesh(&quad(0.0, 0.0, 16.0, 16.0, 0), &[0, 2, 1, 0, 3, 2], &[surface(red, true, 6)]);
    // The view's projection (bx order, the same bytes as the column-major ortho) and a view
    // matrix that moves the world 32 pixels right.
    r.set_view_transform(1, Some(&translate(32.0, 0.0)), Some(&ortho()));
    let mut f = frame(0);
    f.view_proj = None;
    assert!(r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &[IDENTITY], frame: f }));
    r.request_screenshot(INVALID, "view.png");
    r.frame();
    assert!(close(pixel(&shot, 40, 8), [0, 0, 255]), "drawn through the view's transform: {:?}", pixel(&shot, 40, 8));
    assert!(close(pixel(&shot, 8, 8), [0, 0, 0]), "not at the untransformed spot: {:?}", pixel(&shot, 8, 8));
    assert_eq!(r.stats().errors, 0, "wgpu errors:\n{}", log.lock().unwrap().join("\n"));
}

#[test]
fn runs_before_its_view_or_at_the_end() {
    let Some((mut r, log, shot)) = renderer() else { return };
    let red = texture(&mut r, [255, 0, 0, 255]);
    let mesh = r.native_create_mesh(&quad(0.0, 0.0, 64.0, 64.0, 0), &[0, 2, 1, 0, 3, 2], &[surface(red, true, 6)]);
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
