//! The native viewmodel without a GPU: a two-part viewmodel built in memory, its files provided
//! through the request/answer loop, posed by its clip, slid onto the sights when aiming.

use super::ffi::{with_state, ZhboNativeIn, LOADING, NOT_NATIVE};
use super::*;
use zhbo_native::anim::BoneTrack;
use zhbo_native::math::{QUAT_IDENTITY, IDENTITY};
use zhbo_native::model::{Bone, Lod, Material, Surface};
use zhbo_native::viewmodel::{Marker, Part};

#[test]
fn viewmodel_geometry_uses_the_sims_inch() {
    // The viewmodel-space boundary: BO2 geometry is scaled by the same factor the sim uses, so a
    // gun's 20" offset and a 72" body are 20/6 and 12 Zero Hour units — first person and third
    // person agree (Track R7).
    assert_eq!(ART_SCALE, crate::units::ZH_PER_INCH);
    assert!((72.0 * ART_SCALE - crate::units::inches(72.0)).abs() < 1e-6);
    assert!((20.0 * ART_SCALE - crate::units::inches(20.0)).abs() < 1e-6);
}

fn bone(name: &str, parent: Option<u32>, pos: [f32; 3]) -> Bone {
    Bone { name: name.into(), parent, local_rotation: QUAT_IDENTITY, local_position: pos, rotation: QUAT_IDENTITY, position: pos }
}

/// A one-triangle model whose vertices all ride its last bone.
fn model(name: &str, bones: Vec<Bone>) -> Model {
    let last = bones.len() as u16 - 1;
    let v = |p: [f32; 3]| Vertex { position: p, normal: [0.0, 0.0, 1.0], uv: [0.0; 2], bones: [last, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] };
    let at = bones.last().unwrap().position;
    Model {
        name: name.into(),
        bones,
        materials: vec![Material { name: "m".into(), color: Some("Art\\Textures\\c.tga".into()), ..Default::default() }],
        lods: vec![Lod { distance: 0.0, first_surface: 0, surface_count: 1 }],
        // Facing the eye (in the Y-Z plane), 5" on a side.
        surfaces: vec![Surface { material: 0, vertices: vec![v(at), v([at[0], at[1] + 5.0, at[2]]), v([at[0], at[1], at[2] + 5.0])], indices: vec![0, 1, 2] }],
        ..Default::default()
    }
}

#[test]
fn hidden_bones_omit_their_triangles_and_weights() {
    // A body bone and a sight bone: a triangle on each, and a triangle blended between them.
    let bones = vec![bone("j_gun", None, [0.0; 3]), bone("tag_sights", Some(0), [0.0, 0.0, 2.0])];
    let v = |p: [f32; 3], b: u16, w: [f32; 4]| Vertex { position: p, normal: [0.0, 0.0, 1.0], uv: [0.0; 2], bones: [b, 0, 0, 0], weights: w };
    let model = Model {
        name: "gun".into(),
        bones,
        materials: vec![Material { name: "m".into(), ..Default::default() }],
        lods: vec![Lod { distance: 0.0, first_surface: 0, surface_count: 1 }],
        surfaces: vec![Surface {
            material: 0,
            vertices: vec![
                v([0.0, 0.0, 0.0], 0, [1.0, 0.0, 0.0, 0.0]),
                v([1.0, 0.0, 0.0], 0, [1.0, 0.0, 0.0, 0.0]),
                v([0.0, 1.0, 0.0], 0, [1.0, 0.0, 0.0, 0.0]),
                v([0.0, 0.0, 2.0], 1, [1.0, 0.0, 0.0, 0.0]),
                v([1.0, 0.0, 2.0], 1, [1.0, 0.0, 0.0, 0.0]),
                v([0.0, 1.0, 2.0], 1, [1.0, 0.0, 0.0, 0.0]),
                // Mostly on the body, partly on the sight: the triangle stays, but the sight's
                // weight goes so the palette's zero matrix cannot drag the vertex to the origin.
                Vertex { position: [0.0, 0.0, 1.0], normal: [0.0, 0.0, 1.0], uv: [0.0; 2], bones: [0, 1, 0, 0], weights: [0.75, 0.25, 0.0, 0.0] },
                Vertex { position: [1.0, 0.0, 1.0], normal: [0.0, 0.0, 1.0], uv: [0.0; 2], bones: [0, 1, 0, 0], weights: [0.75, 0.25, 0.0, 0.0] },
                Vertex { position: [0.0, 1.0, 1.0], normal: [0.0, 0.0, 1.0], uv: [0.0; 2], bones: [0, 1, 0, 0], weights: [0.75, 0.25, 0.0, 0.0] },
            ],
            // The blended vertices name bone 0 first and bone 1 second.
            indices: vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
        }],
        ..Default::default()
    };
    let parts = [PartRef { model: &model, tag: None, offset: IDENTITY, rigid: false }];
    let (skel, problems) = Skeleton::compose(&parts, &[], &["tag_sights".into()]);
    assert!(problems.is_empty());
    assert!(skel.bones[1].hidden);
    let mesh = build_mesh(&skel, [&model]).unwrap();
    // The sight triangle is left out; the body and blended triangles stay.
    assert_eq!(mesh.indices.len(), 6);
    assert_eq!(mesh.surfaces[0].index_count, 6);
    let blended = mesh.vertices[6];
    assert_eq!(blended.weights, [1.0, 0.0, 0.0, 0.0], "the hidden bone's weight is dropped and the visible one renormalized");
}

struct Fixture {
    files: Vec<(String, Vec<u8>)>,
}

/// Arms with `tag_weapon` 20" ahead; a gun on it whose sights sit 4" up and 2" left; an idle clip
/// that lifts the arms' `tag_weapon` 1" and a reload that lowers it 3".
fn fixture() -> Fixture {
    let arms = model("arms", vec![bone("j_root", None, [0.0; 3]), bone("tag_weapon", Some(0), [20.0, 0.0, -5.0])]);
    let gun = model("gun", vec![bone("tag_weapon", None, [0.0; 3]), bone("j_gun", Some(0), [3.0, 0.0, 0.0])]);
    let clip = |name: &str, z: f32| Anim {
        name: name.into(),
        num_frames: 10,
        frame_rate: 10.0,
        looping: false,
        delta: false,
        asset_type: 0,
        bones: vec![BoneTrack { name: "tag_weapon".into(), rotation: None, translation: Some(vec![(0, [0.0, 0.0, z]), (10, [0.0, 0.0, z])]) }],
        notes: vec![],
    };
    let manifest = Viewmodel {
        id: "test_mp".into(),
        faction: b'U',
        parts: vec![
            Part { model: zhbo_native::model_path("arms"), tag: None, offset: IDENTITY, rigid: false },
            Part { model: zhbo_native::model_path("gun"), tag: Some("tag_weapon".into()), offset: IDENTITY, rigid: false },
        ],
        markers: vec![Marker { name: AIM_TAG.into(), parent: "tag_weapon".into(), local: (QUAT_IDENTITY, [0.0, 2.0, 4.0]) }],
        hide: vec![],
        clips: vec![(1, zhbo_native::anim_path("idle")), (0x10, zhbo_native::anim_path("reload"))],
    };
    Fixture {
        files: vec![
            (zhbo_native::viewmodel_path(7, 'U'), manifest.to_bytes()),
            (zhbo_native::model_path("arms"), arms.to_bytes()),
            (zhbo_native::model_path("gun"), gun.to_bytes()),
            (zhbo_native::anim_path("idle"), clip("idle", 1.0).to_bytes()),
            (zhbo_native::anim_path("reload"), clip("reload", -3.0).to_bytes()),
        ],
    }
}

fn input(clip: i32, ads: f32) -> ZhboNativeIn {
    ZhboNativeIn { vm: 7, faction: b'U' as i32, clip, ads, rate: 1.0, dir: [1.0, 0.0, 0.0], ..Default::default() }
}

/// Answer every outstanding request from the fixture (absent files: "missing").
fn answer(s: &mut ffi::State, f: &Fixture) -> usize {
    let wanted: Vec<String> = s.files.wanted().map(str::to_string).collect();
    for w in &wanted {
        let bytes = f.files.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice());
        s.files.provide(w, bytes);
    }
    wanted.len()
}

fn aim_in_eye(s: &ffi::State) -> [f32; 3] {
    let m = s.loaded.as_ref().unwrap().bone_world(&s.posed, AIM_TAG).expect("the aim marker");
    [m[0][3], m[1][3], m[2][3]]
}

#[test]
fn loads_through_requests_then_poses_and_aims() {
    let f = fixture();
    with_state(|s| {
        *s = ffi::State::default();
        assert_eq!(s.frame(&input(1, 0.0), false), NOT_NATIVE, "off unless enabled");
        assert_eq!(s.frame(&input(1, 0.0), true), LOADING, "asks for the manifest");
        assert_eq!(answer(s, &f), 1);
        assert_eq!(s.frame(&input(1, 0.0), true), LOADING, "asks for the models");
        assert_eq!(answer(s, &f), 2);
        s.frame(&input(1, 0.0), true);
        // The idle clip and the colour map were requested; the texture is absent (draws white).
        assert_eq!(answer(s, &f), 1 + cfg!(feature = "renderer") as usize);
        s.frame(&input(1, 0.0), true);
        let loaded = s.loaded.as_ref().expect("composed");
        assert_eq!(loaded.skeleton.bones.len(), 5);
        assert_eq!(s.posed.palette.len(), 5);
        // Hip: the marker at the gun's sights, 20" ahead, 5" down + 1" (idle) + 4", 2" left; scaled.
        let hip = aim_in_eye(s);
        let want = [20.0 * ART_SCALE, 2.0 * ART_SCALE, 0.0];
        assert!((0..3).all(|k| (hip[k] - want[k]).abs() < 1e-4), "{hip:?} vs {want:?}");
        // Fully aimed: the marker slides onto the view axis, its distance kept.
        s.frame(&input(1, 1.0), true);
        let aimed = aim_in_eye(s);
        assert!(aimed[1].abs() < 1e-4 && aimed[2].abs() < 1e-4 && (aimed[0] - want[0]).abs() < 1e-4, "{aimed:?}");
    });
}

/// Perpendicular distance from `p` to the ray `(eye, dir)` — the shot's own line.
fn distance_to_ray(p: [f32; 3], eye: [f32; 3], dir: [f32; 3]) -> f32 {
    let d = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
    let along = d[0] * dir[0] + d[1] * dir[1] + d[2] * dir[2];
    let perp = [d[0] - dir[0] * along, d[1] - dir[1] * along, d[2] - dir[2] * along];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt()
}

#[test]
fn aiming_puts_the_sight_on_the_ray_the_shot_takes() {
    use crate::math::dir_from_angles;
    use crate::movement::{Body, MoveTuning};
    let f = fixture();
    // The eye and axis of the shot, built as `sim::fire` and `Body::eye()` build them: the eye off
    // the world origin, the view angles the sim fires along.
    let eye = Body::new(crate::math::Vec3::new(120.0, -30.0, 4.0), &MoveTuning::default()).eye();
    let (yaw, pitch) = (0.9f32, -0.2f32);
    let dir = dir_from_angles(yaw, pitch);
    let (eye, dir) = ([eye.x, eye.y, eye.z], [dir.x, dir.y, dir.z]);
    let shot = |s: &mut ffi::State, ads: f32| {
        let mut inp = input(1, ads);
        (inp.eye, inp.dir) = (eye, dir);
        for _ in 0..8 {
            s.frame(&inp, true);
            answer(s, &f);
        }
        distance_to_ray(aim_in_eye(s), eye, dir)
    };
    with_state(|s| {
        *s = ffi::State::default();
        // At the hip the sight is where the animation and the def's offsets put it: 2" left of the
        // axis in this fixture (its forward distance does not matter to the distance).
        let hip = shot(s, 0.0);
        let half = shot(s, 0.5);
        let aimed = shot(s, 1.0);
        assert!((hip - 2.0 * ART_SCALE).abs() < 1e-3, "hip {hip}");
        assert!(aimed < 1e-4, "fully aimed, the sight is on the ray: {aimed}");
        assert!(hip > half && half > aimed, "the slide is monotone: {hip} {half} {aimed}");
        // The sight keeps its authored distance ahead of the eye all the way.
        let along = |p: [f32; 3]| (p[0] - eye[0]) * dir[0] + (p[1] - eye[1]) * dir[1] + (p[2] - eye[2]) * dir[2];
        assert!((along(aim_in_eye(s)) - 20.0 * ART_SCALE).abs() < 1e-3, "{}", along(aim_in_eye(s)));
    });
}

#[test]
fn falls_back_to_the_idle_and_plays_at_the_rate() {
    let f = fixture();
    with_state(|s| {
        *s = ffi::State::default();
        for _ in 0..6 {
            s.frame(&input(0x10, 0.0), true);
            answer(s, &f);
        }
        let l = s.loaded.as_ref().unwrap();
        assert_eq!(l.clip_path(0x17), l.clip_path(1), "a slot the gun lacks holds the idle");
        assert_eq!(l.clip_path(0x10).unwrap(), zhbo_native::anim_path("reload"));
        let z = aim_in_eye(s)[2];
        assert!((z - (-3.0 - 5.0 + 4.0) * ART_SCALE).abs() < 1e-4, "the reload lowers the gun: {z}");
        let mut req = ffi::request(&input(0x10, 0.0));
        req.rate = 0.0;
        assert!(req.rate == 0.0 && ffi::request(&ZhboNativeIn { rate: f32::NAN, ..input(1, 0.0) }).rate == 1.0);
    });
}

#[test]
fn a_missing_manifest_gives_up_once() {
    with_state(|s| {
        *s = ffi::State::default();
        let mut inp = input(1, 0.0);
        inp.vm = 99;
        assert_eq!(s.frame(&inp, true), LOADING);
        let wanted: Vec<String> = s.files.wanted().map(str::to_string).collect();
        for w in wanted {
            s.files.provide(&w, None);
        }
        assert_eq!(s.frame(&inp, true), NOT_NATIVE);
        assert_eq!(s.frame(&inp, true), NOT_NATIVE, "not retried");
        assert_eq!(s.log.len(), 1, "logged once: {:?}", s.log);
    });
}

#[test]
fn gun_pose_matches_the_w3d_rotation_order() {
    // A yaw of 90° turns the gun's forward (X) to the left (Y); a pitch raises the muzzle.
    let yaw = GunPose::from_offset_angles([0.0; 3], [std::f32::consts::FRAC_PI_2, 0.0, 0.0]);
    let p = zhbo_native::math::apply(yaw.0, [1.0, 0.0, 0.0]);
    assert!((p[1] - 1.0).abs() < 1e-5, "{p:?}");
    let pitch = GunPose::from_offset_angles([0.0; 3], [0.0, 0.3, 0.0]);
    assert!(zhbo_native::math::apply(pitch.0, [1.0, 0.0, 0.0])[2] > 0.2, "muzzle up");
}

/// End to end on a headless renderer (wgpu builds): the core composes, uploads and submits the
/// fixture, and the frame's pixels show it in front of the cleared scene.
#[cfg(feature = "renderer")]
#[test]
fn draws_into_a_headless_renderer() {
    use std::sync::{Arc, Mutex};
    use zhbo_gfx::ffi::{with_renderer, zhgfx_init, zhgfx_shutdown, ZhgfxInit};
    use zhbo_gfx::runtime::INVALID;
    let init = ZhgfxInit { kind: 0, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: 64, height: 64, reset: 0, transient_vb_size: 0, transient_ib_size: 0 };
    if !unsafe { zhgfx_init(&init) } {
        eprintln!("no usable adapter, skipping");
        return;
    }
    let shot: Arc<Mutex<Option<Vec<u8>>>> = Arc::default();
    let shot2 = shot.clone();
    with_renderer((), |r| {
        r.set_screenshot_fn(Box::new(move |_, _, _, _, d| *shot2.lock().unwrap() = Some(d.to_vec())));
        r.set_view_rect(1, 0, 0, 64, 64);
        r.set_view_clear(1, zhbo_gfx::state::CLEAR_COLOR, 0x0000ffff, 1.0, 0);
        // The eye at the origin looking down +X, Z up (bx row-vector order): X forward -> z,
        // Y left -> -x, Z up -> y; a 90° perspective with depth 0..1.
        let view = [0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let (n, f) = (0.1f32, 100.0f32);
        let proj = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
        r.set_view_transform(1, Some(&view), Some(&proj));
        r.touch(1);
    });
    let f = fixture();
    let drawn = with_state(|s| {
        *s = ffi::State::default();
        let mut inp = input(1, 1.0);
        inp.target_view = 1;
        inp.before_view = 2;
        let mut code = LOADING;
        for _ in 0..8 {
            code = s.frame(&inp, true);
            answer(s, &f);
        }
        code
    });
    assert_eq!(drawn, ffi::DRAWN);
    with_renderer((), |r| {
        r.request_screenshot(INVALID, "vm.png");
        r.frame();
        assert_eq!(r.stats().errors, 0);
        assert_eq!(r.stats().native_skipped, 0);
    });
    let px = shot.lock().unwrap().take().expect("a screenshot");
    // Cleared blue (BGRA 255,0,0); the unlit (no lights, no ambient) viewmodel is dark on it.
    let covered = px.as_chunks::<4>().0.iter().filter(|p| p[0] < 128).count();
    assert!(covered > 0, "the viewmodel's triangles reached the screen");
    with_state(|s| s.gpu.release());
    zhgfx_shutdown();
}

#[cfg(feature = "renderer")]
#[test]
#[ignore = "diagnostic: ZHBO_DIAG_DIR holds an94_col.tga/an94_nsg.tga from the archive"]
fn diag_real_maps() {
    use std::sync::{Arc, Mutex};
    use zhbo_gfx::ffi::{with_renderer, zhgfx_init, zhgfx_shutdown, ZhgfxInit};
    use zhbo_gfx::runtime::*;
    let dir = std::env::var("ZHBO_DIAG_DIR").unwrap();
    let col = zhbo_native::tga::decode(&std::fs::read(format!("{dir}/an94_col.tga")).unwrap()).unwrap();
    let nsg = zhbo_native::tga::decode(&std::fs::read(format!("{dir}/an94_nsg.tga")).unwrap()).unwrap();
    let init = ZhgfxInit { kind: 0, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: 64, height: 64, reset: 0, transient_vb_size: 0, transient_ib_size: 0 };
    assert!(unsafe { zhgfx_init(&init) });
    let shot: Arc<Mutex<Option<Vec<u8>>>> = Arc::default();
    let shot2 = shot.clone();
    with_renderer((), |r| {
        r.set_screenshot_fn(Box::new(move |_, _, _, _, d| *shot2.lock().unwrap() = Some(d.to_vec())));
        r.set_view_rect(1, 0, 0, 64, 64);
        r.touch(1);
        let c = r.native_create_texture(col.width, col.height, &col.rgba, false);
        let m = r.native_create_texture(nsg.width, nsg.height, &nsg.rgba, false);
        let v = |x: f32, y: f32, u: f32, w: f32| NativeVertex { position: [x, y, 0.0], normal: [0.0, 0.0, -1.0], uv: [u, w], bones: [0; 4], weights: [1.0, 0.0, 0.0, 0.0] };
        let verts = [v(0.0, 0.0, 0.0, 0.0), v(0.0, 64.0, 0.0, 1.0), v(64.0, 64.0, 1.0, 1.0), v(64.0, 0.0, 1.0, 0.0)];
        for (maps, label) in [(INVALID, "no maps"), (m, "maps")] {
            let mesh = r.native_create_mesh(&verts, &[0, 1, 2, 0, 2, 3], &[NativeSurface { first_index: 0, index_count: 6, blend: NativeBlend::Opaque, emissive: false, color: c, maps, sort_key: 0, cull: NativeCull::None }]);
            let mut ortho = [0.0f32; 16];
            ortho[0] = 2.0 / 64.0; ortho[5] = -2.0 / 64.0; ortho[12] = -1.0; ortho[13] = 1.0; ortho[14] = 0.5; ortho[15] = 1.0;
            let mut lights = [NativeLight::default(); 3];
            lights[0] = NativeLight { direction: [0.81, -0.38, 0.45], color: [1.0, 1.0, 1.0] };
            lights[1] = NativeLight { direction: [-0.79, -0.62, 0.0], color: [0.24, 0.24, 0.31] };
            let f = NativeFrame { view_proj: Some(ortho), eye: [32.0, 32.0, -100.0], ambient: [0.22, 0.2, 0.17], lights, light_count: 2, debug: 0.0 };
            r.native_submit(&NativeDraw { target_view: 1, before_view: 2, mesh, palette: &[zhbo_native::math::MAT4_IDENTITY], frame: f });
            r.request_screenshot(INVALID, "x");
            r.frame();
            let px = shot.lock().unwrap().take().unwrap();
            let n = (px.len() / 4) as f32;
            let (px, _) = px.as_chunks::<4>();
            let avg: Vec<f32> = (0..3).map(|c| px.iter().map(|p| p[2 - c] as f32).sum::<f32>() / n).collect();
            eprintln!("{label}: avg RGB {avg:?}");
        }
    });
    zhgfx_shutdown();
}

#[cfg(feature = "renderer")]
#[test]
#[ignore = "diagnostic: ZHBO_DIAG_BIG = ConquerOps_BO2.big, ZHBO_DIAG_OUT = out.ppm"]
fn diag_real_viewmodel() {
    use std::sync::{Arc, Mutex};
    use zhbo_gfx::ffi::{with_renderer, zhgfx_init, zhgfx_shutdown, ZhgfxInit};
    use zhbo_gfx::runtime::INVALID;
    let big = std::fs::read(std::env::var("ZHBO_DIAG_BIG").unwrap()).unwrap();
    let be = |a: usize| u32::from_be_bytes(big[a..a + 4].try_into().unwrap()) as usize;
    let mut index = std::collections::HashMap::new();
    let mut at = 16;
    for _ in 0..be(8) {
        let (off, size) = (be(at), be(at + 4));
        let end = at + 8 + big[at + 8..].iter().position(|&b| b == 0).unwrap();
        index.insert(String::from_utf8_lossy(&big[at + 8..end]).to_lowercase(), (off, size));
        at = end + 1;
    }
    let (w, h) = (320u32, 200u32);
    let init = ZhgfxInit { kind: 0, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: w, height: h, reset: 0, transient_vb_size: 0, transient_ib_size: 0 };
    assert!(unsafe { zhgfx_init(&init) });
    let shot: Arc<Mutex<Option<Vec<u8>>>> = Arc::default();
    let shot2 = shot.clone();
    with_renderer((), |r| {
        r.set_screenshot_fn(Box::new(move |_, _, _, _, d| *shot2.lock().unwrap() = Some(d.to_vec())));
        r.set_view_rect(1, 0, 0, w as u16, h as u16);
        r.set_view_clear(1, zhbo_gfx::state::CLEAR_COLOR, 0x6080a0ff, 1.0, 0);
        let view = [0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let (n, f) = (0.3f32, 100.0f32);
        let t = 1.0 / (65f32.to_radians() / 2.0).tan();
        let proj = [t * h as f32 / w as f32, 0.0, 0.0, 0.0, 0.0, t, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
        r.set_view_transform(1, Some(&view), Some(&proj));
        r.touch(1);
    });
    let vm: i32 = std::env::var("ZHBO_DIAG_VM").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let code = with_state(|s| {
        *s = ffi::State::default();
        let mut inp = input(1, 0.0);
        inp.vm = vm;
        inp.target_view = 1;
        inp.before_view = 2;
        let amb: f32 = std::env::var("ZHBO_DIAG_AMB").ok().and_then(|v| v.parse().ok()).unwrap_or(0.2);
        inp.ambient = [amb; 3];
        inp.lights[0] = ffi::ZhboNativeLight { dir: [0.81, -0.38, 0.45], color: [1.0, 1.0, 1.0] };
        inp.lights[1] = ffi::ZhboNativeLight { dir: [-0.79, -0.62, 0.0], color: [0.24, 0.24, 0.31] };
        inp.light_count = 2;
        let mut code = 0;
        for _ in 0..12 {
            code = s.frame(&inp, true);
            let wanted: Vec<String> = s.files.wanted().map(str::to_string).collect();
            for p in wanted {
                let bytes = index.get(&p.to_lowercase()).map(|&(o, n)| &big[o..o + n]);
                s.files.provide(&p, bytes);
            }
        }
        while let Some(l) = s.log.pop_front() {
            eprintln!("{l}");
        }
        code
    });
    eprintln!("code {code}");
    with_renderer((), |r| {
        r.request_screenshot(INVALID, "x");
        r.frame();
        eprintln!("errors {} skipped {} live {:?}", r.stats().errors, r.stats().native_skipped, r.native_live());
    });
    let px = shot.lock().unwrap().take().unwrap();
    let mut ppm = format!("P6 {w} {h} 255\n").into_bytes();
    let (px, _) = px.as_chunks::<4>();
    for p in px {
        ppm.extend_from_slice(&[p[2], p[1], p[0]]);
    }
    std::fs::write(std::env::var("ZHBO_DIAG_OUT").unwrap(), ppm).unwrap();
    zhgfx_shutdown();
}
