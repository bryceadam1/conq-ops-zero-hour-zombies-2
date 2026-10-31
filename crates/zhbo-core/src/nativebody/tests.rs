//! The native third person without a GPU: a world weapon built in memory, provided through the
//! request/answer loop, placed on an engine tag transform and posed; the body clip selection from
//! the sim's movement facts.

use super::ffi::{with_state, ZhboNativeBodyIn, LOADING, NOT_NATIVE};
use super::*;
use zhbo_native::math::{QUAT_IDENTITY, IDENTITY};
use zhbo_native::model::{Bone, Lod, Material, Model, Surface};
use zhbo_native::thirdperson::Weapon;
use zhbo_native::viewmodel::Part as ViewmodelPart;

fn bone(name: &str, parent: Option<u32>, pos: [f32; 3]) -> Bone {
    Bone { name: name.into(), parent, local_rotation: QUAT_IDENTITY, local_position: pos, rotation: QUAT_IDENTITY, position: pos }
}

/// A one-triangle model whose vertices all ride its last bone.
fn model(name: &str, bones: Vec<Bone>) -> Model {
    let last = bones.len() as u16 - 1;
    let v = |p: [f32; 3]| zhbo_native::model::Vertex { position: p, normal: [0.0, 0.0, 1.0], uv: [0.0; 2], bones: [last, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] };
    let at = bones.last().unwrap().position;
    Model {
        name: name.into(),
        bones,
        materials: vec![Material { name: "m".into(), color: Some("Art\\Textures\\c.tga".into()), ..Default::default() }],
        lods: vec![Lod { distance: 0.0, first_surface: 0, surface_count: 1 }],
        surfaces: vec![Surface { material: 0, vertices: vec![v(at), v([at[0], at[1] + 5.0, at[2]]), v([at[0], at[1], at[2] + 5.0])], indices: vec![0, 1, 2] }],
        ..Default::default()
    }
}

struct Fixture {
    files: Vec<(String, Vec<u8>)>,
}

/// A world rifle 30" long (its `j_gun` at 3") with a reflex 2" ahead on its `tag_reflex`, and a
/// hidden iron sight.
fn fixture() -> Fixture {
    let gun = model("gun", vec![bone("j_gun", None, [0.0; 3]), bone("tag_reflex", Some(0), [2.0, 0.0, 4.0])]);
    let reflex = model("reflex", vec![bone("j_root", None, [0.0; 3])]);
    let manifest = Weapon {
        id: "test_mp+reflex".into(),
        parts: vec![
            ViewmodelPart { model: zhbo_native::model_path("gun"), tag: None, offset: IDENTITY, rigid: true, ads_only: false },
            ViewmodelPart { model: zhbo_native::model_path("reflex"), tag: Some("tag_reflex".into()), offset: (QUAT_IDENTITY, [0.0, 0.0, 0.0]), rigid: true, ads_only: false },
        ],
        hide: vec!["tag_iron".into()],
    };
    Fixture { files: vec![(zhbo_native::world_weapon_path(7), manifest.to_bytes()), (zhbo_native::model_path("gun"), gun.to_bytes()), (zhbo_native::model_path("reflex"), reflex.to_bytes())] }
}

fn answer(s: &mut ffi::State, f: &Fixture) -> usize {
    let wanted: Vec<String> = s.files.wanted().map(str::to_string).collect();
    for w in &wanted {
        let bytes = f.files.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice());
        s.files.provide(w, bytes);
    }
    wanted.len()
}

/// A tag at `t` with no rotation, its rows in W3D's row-vector order.
fn tag(t: [f32; 3]) -> [[f32; 4]; 3] {
    [[1.0, 0.0, 0.0, t[0]], [0.0, 1.0, 0.0, t[1]], [0.0, 0.0, 1.0, t[2]]]
}

fn input(vm: i32, tag: Option<[[f32; 4]; 3]>) -> ZhboNativeBodyIn {
    let mut inp = ZhboNativeBodyIn { general: 1, mode: 1, vm, faction: b'U' as i32, state: 1, time: 0.0, rate: 1.0, pos: [1.0, 2.0, 3.0], yaw: 0.0, ..Default::default() };
    if let Some(t) = tag {
        inp.tag_valid = 1;
        inp.tag = t.iter().flatten().copied().collect::<Vec<f32>>().try_into().unwrap();
    }
    inp
}

#[test]
fn the_tag_matrix_is_scaled_once() {
    // A tag at (10, 20, 30), no rotation: the gun's origin lands at the tag's translation and its
    // +X (inches) runs along the tag's X basis, scaled.
    let root = root_from_tag(&tag([10.0, 20.0, 30.0]));
    let p = zhbo_native::math::mat4_point(&root, [3.0, 0.0, 0.0]);
    let want = [10.0 + 3.0 * ART_SCALE, 20.0, 30.0];
    assert!((0..3).all(|k| (p[k] - want[k]).abs() < 1e-5), "{p:?} vs {want:?}");
    // A tag yawed +90° about Z sends the gun's +X to +Y. W3D's Matrix3D is column-vector with the
    // basis in the columns: row 0 = (X.x, Y.x, Z.x, Tx), so the +X image is (row0[0], row1[0],
    // row2[0]) = (0, 1, 0).
    let mut r = tag([0.0, 0.0, 0.0]);
    r[0] = [0.0, -1.0, 0.0, 0.0];
    r[1] = [1.0, 0.0, 0.0, 0.0];
    let p = zhbo_native::math::mat4_point(&root_from_tag(&r), [2.0, 0.0, 0.0]);
    assert!((p[0]).abs() < 1e-5 && (p[1] - 2.0 * ART_SCALE).abs() < 1e-5 && p[2].abs() < 1e-5, "{p:?}");
}

#[test]
fn a_world_weapon_loads_through_requests_and_rides_the_tag() {
    let f = fixture();
    with_state(|s| {
        *s = ffi::State::default();
        assert_eq!(s.frame(&input(7, Some(tag([0.0; 3]))), 0), NOT_NATIVE, "off unless asked for");
        assert_eq!(s.frame(&input(7, Some(tag([0.0; 3]))), 1), LOADING, "asks for the manifest");
        assert_eq!(answer(s, &f), 1);
        assert_eq!(s.frame(&input(7, Some(tag([0.0; 3]))), 1), LOADING, "asks for the models");
        assert_eq!(answer(s, &f), 2);
        s.frame(&input(7, Some(tag([0.0; 3]))), 1);
        // The colour map is asked for next when the renderer uploads; absent textures draw
        // white/flat. Without the renderer nothing asks for textures.
        assert_eq!(answer(s, &f), cfg!(feature = "renderer") as usize);
        s.frame(&input(7, Some(tag([0.0; 3]))), 1);
        let loaded = s.loaded.as_ref().expect("composed");
        // The gun's j_gun plus the reflex's bone, on the gun's tag_reflex.
        assert_eq!(loaded.skeleton.bones.len(), 3, "gun bones and the reflex's root");
        // A tag at (10, 20, 30): the gun's origin (j_gun, at 0") lands there; the reflex hangs 2"
        // ahead and 4" up in the gun's frame, all scaled once.
        s.frame(&input(7, Some(tag([10.0, 20.0, 30.0]))), 1);
        let m = s.loaded.as_ref().unwrap().bone_world(&s.posed, "j_gun").expect("the gun bone");
        let want = [10.0, 20.0, 30.0];
        assert!((0..3).all(|k| (m[k][3] - want[k]).abs() < 1e-4), "{:?} vs {want:?}", [m[0][3], m[1][3], m[2][3]]);
        let m = s.loaded.as_ref().unwrap().bone_world(&s.posed, "j_root").expect("the reflex's root");
        let want = [10.0 + 2.0 * ART_SCALE, 20.0, 30.0 + 4.0 * ART_SCALE];
        assert!((0..3).all(|k| (m[k][3] - want[k]).abs() < 1e-4), "{:?} vs {want:?}", [m[0][3], m[1][3], m[2][3]]);
    });
}

#[test]
fn a_weapon_without_its_tag_keeps_the_w3d_model() {
    with_state(|s| {
        *s = ffi::State::default();
        assert_eq!(s.frame(&input(7, None), 1), NOT_NATIVE, "no tag, no native draw");
    });
    // The bridge asks this before it bothers with the draw; it takes no state lock itself.
    assert_eq!(unsafe { ffi::zhbo_native_body_ready(&input(7, None)) }, 0);
    assert_eq!(unsafe { ffi::zhbo_native_body_ready(&input(7, Some(tag([0.0; 3])))) }, 1);
}

#[test]
fn a_missing_manifest_is_refused_once() {
    with_state(|s| {
        *s = ffi::State::default();
        s.files.provide(&zhbo_native::world_weapon_path(9), None);
        assert_eq!(s.frame(&input(9, Some(tag([0.0; 3]))), 1), NOT_NATIVE);
        assert!(s.logged().any(|l| l.contains("ZW09") && l.contains("reconvert")), "{:?}", s.logged().collect::<Vec<_>>());
    });
}

#[test]
fn the_body_clip_follows_the_stance_gait_and_sights() {
    let run = 31.7; // the sim's g_speed, world units/s
    use zhbo_native::thirdperson::state;
    // Standing still, then each gait.
    assert_eq!(state_of(0, 0.0, false, 0.0, false, false, run), state::STAND_IDLE);
    assert_eq!(state_of(0, 4.0, false, 0.0, false, false, run), state::STAND_WALK);
    assert_eq!(state_of(0, run, false, 0.0, false, false, run), state::STAND_RUN);
    assert_eq!(state_of(0, run, true, 0.0, false, false, run), state::STAND_SPRINT);
    // ADS and the pistol's own stance.
    assert_eq!(state_of(0, 0.0, false, 1.0, false, false, run), state::STAND_ADS);
    assert_eq!(state_of(0, 0.0, false, 1.0, true, false, run), state::STAND_ADS_PISTOL);
    assert_eq!(state_of(0, 0.0, false, 0.0, true, false, run), state::STAND_IDLE_PISTOL);
    // Crouch: idle, walk, ADS.
    assert_eq!(state_of(1, 0.0, false, 0.0, false, false, run), state::CROUCH_IDLE);
    assert_eq!(state_of(1, 4.0, false, 0.0, false, false, run), state::CROUCH_WALK);
    assert_eq!(state_of(1, 0.0, false, 1.0, false, false, run), state::CROUCH_ADS);
    // Prone: idle and crawl.
    assert_eq!(state_of(2, 0.0, false, 0.0, false, false, run), state::PRONE_IDLE);
    assert_eq!(state_of(2, 4.0, false, 0.0, false, false, run), state::PRONE_CRAWL);
    // Under RTS the bridge's "moving" runs even though the sim's speed is zero.
    assert_eq!(state_of(0, 0.0, false, 0.0, false, true, run), state::STAND_WALK);
}

#[test]
fn the_body_root_is_the_object_yawed_about_z() {
    // Feet at (1, 2, 3), yawed +90° about Z: the head stays 12 units up and the body's +X (its
    // right side in the model's frame) runs along +Y.
    let root = root_from_object([1.0, 2.0, 3.0], std::f32::consts::FRAC_PI_2);
    let head = zhbo_native::math::mat4_point(&root, [0.0, 0.0, 72.0]);
    let want = [1.0, 2.0, 3.0 + 72.0 * ART_SCALE];
    assert!((0..3).all(|k| (head[k] - want[k]).abs() < 1e-4), "{head:?} vs {want:?}");
    let side = zhbo_native::math::mat4_point(&root, [72.0, 0.0, 0.0]);
    let want = [1.0, 2.0 + 72.0 * ART_SCALE, 3.0];
    assert!((0..3).all(|k| (side[k] - want[k]).abs() < 1e-4), "{side:?} vs {want:?}");
}

/// Body mode with the Assault Shield: in hand it rides `tag_weapon_left` (its manifest's own base
/// tag), carried with another weapon up it hangs on `tag_stowed_back` (the stowed manifest).
#[test]
fn the_shield_rides_the_left_hand_and_the_back() {
    let body = model("body", vec![bone("j_root", None, [0.0; 3]), bone("tag_weapon_right", Some(0), [10.0, -8.0, 40.0]), bone("tag_weapon_left", Some(0), [12.0, 8.0, 44.0]), bone("tag_stowed_back", Some(0), [-6.0, 0.0, 50.0])]);
    let bmanifest = zhbo_native::thirdperson::Body { faction: b'U', model: zhbo_native::model_path("body"), weapon_tag: "tag_weapon_right".into(), clips: Vec::new() };
    let shield = model("shield", vec![bone("j_shield", None, [0.0; 3])]);
    let stow = model("stow", vec![bone("j_stow", None, [0.0; 3])]);
    let rifle = model("rifle", vec![bone("j_gun", None, [0.0; 3])]);
    let part = |m: &str, tag: Option<&str>| ViewmodelPart { model: zhbo_native::model_path(m), tag: tag.map(str::to_string), offset: IDENTITY, rigid: true, ads_only: false };
    let f = Fixture {
        files: vec![
            (zhbo_native::body_path('U'), bmanifest.to_bytes()),
            (zhbo_native::model_path("body"), body.to_bytes()),
            (zhbo_native::world_weapon_path(9), Weapon { id: "riotshield_mp".into(), parts: vec![part("shield", Some("tag_weapon_left"))], hide: Vec::new() }.to_bytes()),
            (zhbo_native::stowed_weapon_path(9), Weapon { id: "riotshield_mp_stowed".into(), parts: vec![part("stow", Some("tag_stowed_back"))], hide: Vec::new() }.to_bytes()),
            (zhbo_native::world_weapon_path(3), Weapon { id: "rifle_mp".into(), parts: vec![part("rifle", None)], hide: Vec::new() }.to_bytes()),
            (zhbo_native::model_path("shield"), shield.to_bytes()),
            (zhbo_native::model_path("stow"), stow.to_bytes()),
            (zhbo_native::model_path("rifle"), rifle.to_bytes()),
        ],
    };
    let body_input = |vm: i32| ZhboNativeBodyIn { mode: 2, pos: [0.0; 3], ..input(vm, None) };
    let at = |s: &ffi::State, bone: &str| {
        let m = s.loaded.as_ref().unwrap().bone_world(&s.posed, bone).unwrap_or_else(|| panic!("no {bone}"));
        [m[0][3], m[1][3], m[2][3]]
    };
    let close = |a: [f32; 3], inches: [f32; 3]| (0..3).all(|k| (a[k] - inches[k] * ART_SCALE).abs() < 1e-3);
    let compose = |s: &mut ffi::State, vm: i32| {
        for _ in 0..20 {
            s.frame(&body_input(vm), 2);
            answer(s, &f);
        }
        s.frame(&body_input(vm), 2);
    };
    with_state(|s| {
        *s = ffi::State::default();
        ffi::publish_stowed(&[]);
        compose(s, 9);
        assert!(s.loaded.is_some(), "{:?}", s.logged().collect::<Vec<_>>());
        assert!(close(at(s, "j_shield"), [12.0, 8.0, 44.0]), "held on the left hand: {:?}", at(s, "j_shield"));
        // The rifle up, the shield on the back.
        ffi::publish_stowed(&[(1, 9)]);
        compose(s, 3);
        assert!(s.loaded.as_ref().is_some_and(|l| l.key.1 == 3), "{:?}", s.logged().collect::<Vec<_>>());
        assert!(close(at(s, "j_gun"), [10.0, -8.0, 40.0]), "the rifle in the right hand");
        assert!(close(at(s, "j_stow"), [-6.0, 0.0, 50.0]), "stowed on the back: {:?}", at(s, "j_stow"));
        ffi::publish_stowed(&[]);
    });
}
