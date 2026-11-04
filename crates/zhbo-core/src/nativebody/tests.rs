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
            ViewmodelPart { model: zhbo_native::model_path("gun"), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: None, ads_swap: None },
            ViewmodelPart { model: zhbo_native::model_path("reflex"), tag: Some("tag_reflex".into()), offset: (QUAT_IDENTITY, [0.0, 0.0, 0.0]), rigid: true, ads_only: false, camo: None, ads_swap: None },
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
        assert_eq!(s.frame(&input(7, Some(tag([0.0; 3]))), 0, None), NOT_NATIVE, "off unless asked for");
        assert_eq!(s.frame(&input(7, Some(tag([0.0; 3]))), 1, None), LOADING, "asks for the manifest");
        assert_eq!(answer(s, &f), 1);
        assert_eq!(s.frame(&input(7, Some(tag([0.0; 3]))), 1, None), LOADING, "asks for the models");
        assert_eq!(answer(s, &f), 2);
        s.frame(&input(7, Some(tag([0.0; 3]))), 1, None);
        // The colour map is asked for next when the renderer uploads; absent textures draw
        // white/flat. Without the renderer nothing asks for textures.
        assert_eq!(answer(s, &f), cfg!(feature = "renderer") as usize);
        s.frame(&input(7, Some(tag([0.0; 3]))), 1, None);
        let loaded = s.loaded().expect("composed");
        // The gun's j_gun plus the reflex's bone, on the gun's tag_reflex.
        assert_eq!(loaded.skeleton.bones.len(), 3, "gun bones and the reflex's root");
        // A tag at (10, 20, 30): the gun's origin (j_gun, at 0") lands there; the reflex hangs 2"
        // ahead and 4" up in the gun's frame, all scaled once.
        s.frame(&input(7, Some(tag([10.0, 20.0, 30.0]))), 1, None);
        let m = s.loaded().unwrap().bone_world(&s.posed, "j_gun").expect("the gun bone");
        let want = [10.0, 20.0, 30.0];
        assert!((0..3).all(|k| (m[k][3] - want[k]).abs() < 1e-4), "{:?} vs {want:?}", [m[0][3], m[1][3], m[2][3]]);
        let m = s.loaded().unwrap().bone_world(&s.posed, "j_root").expect("the reflex's root");
        let want = [10.0 + 2.0 * ART_SCALE, 20.0, 30.0 + 4.0 * ART_SCALE];
        assert!((0..3).all(|k| (m[k][3] - want[k]).abs() < 1e-4), "{:?} vs {want:?}", [m[0][3], m[1][3], m[2][3]]);
    });
}

/// Two Generals with different weapons each keep their rig: drawing them alternately composes
/// each once and asks for no file again (one shared slot reloaded both every frame).
#[test]
fn each_general_keeps_his_own_rig() {
    let mut f = fixture();
    let manifest = f.files[0].1.clone();
    f.files.push((zhbo_native::world_weapon_path(8), manifest));
    let at = |general: u32, vm: i32| ZhboNativeBodyIn { general, ..input(vm, Some(tag([0.0; 3]))) };
    with_state(|s| {
        *s = ffi::State::default();
        for _ in 0..6 {
            s.frame(&at(1, 7), 1, None);
            s.frame(&at(2, 8), 1, None);
            answer(s, &f);
        }
        assert_eq!(s.rig_count(), 2);
        let composed = |s: &ffi::State| s.logged().filter(|l| l.contains("composed")).count();
        assert_eq!(composed(s), 2, "{:?}", s.logged().collect::<Vec<_>>());
        for _ in 0..3 {
            s.frame(&at(1, 7), 1, None);
            s.frame(&at(2, 8), 1, None);
            assert_eq!(answer(s, &f), 0, "nothing reloads");
        }
        assert_eq!(s.loaded().map(|l| l.key.1), Some(8), "the last posed is the bone source");
        // A General's weapon change replaces his rig only: while his new one loads (the W3D
        // weapon draws), the other General's stays.
        assert_eq!(s.frame(&at(1, 8), 1, None), LOADING);
        assert_eq!(s.rig_count(), 1);
        for _ in 0..3 {
            s.frame(&at(1, 8), 1, None);
            answer(s, &f);
        }
        assert_eq!(s.rig_count(), 2);
    });
}

#[test]
fn a_weapon_without_its_tag_keeps_the_w3d_model() {
    with_state(|s| {
        *s = ffi::State::default();
        assert_eq!(s.frame(&input(7, None), 1, None), NOT_NATIVE, "no tag, no native draw");
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
        assert_eq!(s.frame(&input(9, Some(tag([0.0; 3]))), 1, None), NOT_NATIVE);
        assert!(s.logged().any(|l| l.contains("ZW09") && l.contains("reconvert")), "{:?}", s.logged().collect::<Vec<_>>());
    });
}

#[test]
fn the_body_clip_follows_the_stance_gait_and_sights() {
    let run = 31.7; // the sim's g_speed, world units/s
    use zhbo_native::thirdperson::state;
    // Standing still, then each gait.
    assert_eq!(state_of(0, 0.0, false, 0.0, 0, false, run), state::STAND_IDLE);
    assert_eq!(state_of(0, 4.0, false, 0.0, 0, false, run), state::STAND_WALK);
    assert_eq!(state_of(0, run, false, 0.0, 0, false, run), state::STAND_RUN);
    assert_eq!(state_of(0, run, true, 0.0, 0, false, run), state::STAND_SPRINT);
    // ADS, and the held weapon's own animation set for every state.
    assert_eq!(state_of(0, 0.0, false, 1.0, 0, false, run), state::STAND_ADS);
    assert_eq!(state_of(0, 0.0, false, 1.0, 3, false, run), state::id(3, state::STAND_ADS));
    assert_eq!(state_of(0, 4.0, false, 0.0, 3, false, run), state::id(3, state::STAND_WALK), "a pistol walks its own walk");
    assert_eq!(state_of(2, 4.0, false, 0.0, 5, false, run), state::id(5, state::PRONE_CRAWL));
    // Crouch: idle, walk, ADS.
    assert_eq!(state_of(1, 0.0, false, 0.0, 0, false, run), state::CROUCH_IDLE);
    assert_eq!(state_of(1, 4.0, false, 0.0, 0, false, run), state::CROUCH_WALK);
    assert_eq!(state_of(1, 0.0, false, 1.0, 0, false, run), state::CROUCH_ADS);
    // Prone: idle and crawl.
    assert_eq!(state_of(2, 0.0, false, 0.0, 0, false, run), state::PRONE_IDLE);
    assert_eq!(state_of(2, 4.0, false, 0.0, 0, false, run), state::PRONE_CRAWL);
    // Under RTS the bridge's "moving" runs even though the sim's speed is zero.
    assert_eq!(state_of(0, 0.0, false, 0.0, 0, true, run), state::STAND_WALK);
}

#[test]
fn the_body_root_is_the_object_yawed_about_z() {
    // Feet at (1, 2, 3), yawed +90° about Z: the head stands at the General's drawn size (the W3D
    // body's `Scale`, `units::DRAWN_ZH_PER_INCH`) and the body's +X runs along +Y.
    let root = root_from_object([1.0, 2.0, 3.0], std::f32::consts::FRAC_PI_2);
    let head = zhbo_native::math::mat4_point(&root, [0.0, 0.0, 72.0]);
    let want = [1.0, 2.0, 3.0 + crate::units::drawn_inches(72.0)];
    assert!((0..3).all(|k| (head[k] - want[k]).abs() < 1e-4), "{head:?} vs {want:?}");
    let side = zhbo_native::math::mat4_point(&root, [72.0, 0.0, 0.0]);
    let want = [1.0, 2.0 + crate::units::drawn_inches(72.0), 3.0];
    assert!((0..3).all(|k| (side[k] - want[k]).abs() < 1e-4), "{side:?} vs {want:?}");
}

/// Body mode: a world part with no tag (the magazine — BO2's default `j_gun` is not a bone of the
/// world models) hangs from the weapon's root on the hand, not from the body's origin.
#[test]
fn a_tagless_world_part_rides_the_weapon_in_body_mode() {
    let body = model("body", vec![bone("tag_origin", None, [0.0; 3]), bone("tag_weapon_right", Some(0), [10.0, -28.0, 40.0])]);
    let gun = model("gun", vec![bone("tag_weapon", None, [0.0; 3])]);
    let mag = model("mag", vec![bone("tag_clip", None, [0.0; 3])]);
    let manifest = Weapon {
        id: "test_mp".into(),
        parts: vec![
            ViewmodelPart { model: zhbo_native::model_path("gun"), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: None, ads_swap: None },
            ViewmodelPart { model: zhbo_native::model_path("mag"), tag: None, offset: (QUAT_IDENTITY, [-11.0, 0.0, -4.0]), rigid: true, ads_only: false, camo: None, ads_swap: None },
        ],
        hide: vec![],
    };
    let body_manifest = zhbo_native::thirdperson::Body { faction: b'U', model: zhbo_native::model_path("body"), weapon_tag: "tag_weapon_right".into(), clips: vec![] };
    let files_in = [
        (zhbo_native::world_weapon_path(7), manifest.to_bytes()),
        (zhbo_native::model_path("gun"), gun.to_bytes()),
        (zhbo_native::model_path("mag"), mag.to_bytes()),
        (zhbo_native::body_path('U'), body_manifest.to_bytes()),
        (zhbo_native::model_path("body"), body.to_bytes()),
    ];
    let req = Request { vm: 7, faction: b'U', mode: Mode::Body, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1, offhands: [-1; 2] };
    let mut files = Files::default();
    let mut loaded = None;
    for _ in 0..8 {
        if let Some(l) = load(&mut files, &req, None).unwrap() {
            loaded = Some(l);
            break;
        }
        let wanted: Vec<String> = files.wanted().map(str::to_string).collect();
        for w in &wanted {
            files.provide(w, files_in.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice()));
        }
    }
    let l = loaded.expect("the body rig loads");
    let clip = l.skeleton.bone("tag_clip").expect("the magazine's bone");
    let at = l.skeleton.bones[clip].bind.1;
    assert!((at[0] - -1.0).abs() < 1e-4 && (at[1] - -28.0).abs() < 1e-4 && (at[2] - 36.0).abs() < 1e-4, "the magazine sits on the hand's weapon: {at:?}");
}

/// Third-person camo is per part, as in first person (`FUN_00529600`, G16): the world gun wears
/// the def's `WeaponCamo`, the optic its own unique's; in body mode the body (part 0) wears none.
#[test]
fn each_world_part_wears_its_own_camo_asset() {
    let camo_model = |name: &str, bones: Vec<Bone>| {
        let mut m = model(name, bones);
        m.materials[0].camo = zhbo_native::model::CamoLayer::Pattern;
        m
    };
    let body = camo_model("body", vec![bone("tag_origin", None, [0.0; 3]), bone("tag_weapon_right", Some(0), [10.0, -28.0, 40.0])]);
    let gun = camo_model("gun", vec![bone("tag_weapon", None, [0.0; 3]), bone("tag_reflex", Some(0), [2.0, 0.0, 4.0])]);
    let optic = camo_model("optic", vec![bone("j_root", None, [0.0; 3])]);
    let blob = |name: &str, image: &str| zhbo_native::camo::Camo {
        name: name.into(),
        solid_base: None,
        pattern_base: None,
        sets: vec![zhbo_native::camo::Set { solid: None, pattern: Some(image.into()), offset: [0.0; 2], scale: 1.0, gain: 2.0 }],
        styles: vec![],
        materials: vec![],
        options: vec![zhbo_native::camo::CamoOption { id: "camo_atacs".into(), pattern: Some(0), style: None }],
    };
    let manifest = Weapon {
        id: "test_mp+reflex".into(),
        parts: vec![
            ViewmodelPart { model: zhbo_native::model_path("gun"), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: Some("camo_gun".into()), ads_swap: None },
            ViewmodelPart { model: zhbo_native::model_path("optic"), tag: Some("tag_reflex".into()), offset: IDENTITY, rigid: true, ads_only: false, camo: Some("camo_optic".into()), ads_swap: None },
        ],
        hide: vec![],
    };
    let files_in = [
        (zhbo_native::world_weapon_path(7), manifest.to_bytes()),
        (zhbo_native::model_path("gun"), gun.to_bytes()),
        (zhbo_native::model_path("optic"), optic.to_bytes()),
        (zhbo_native::body_path('U'), zhbo_native::thirdperson::Body { faction: b'U', model: zhbo_native::model_path("body"), weapon_tag: "tag_weapon_right".into(), clips: vec![] }.to_bytes()),
        (zhbo_native::model_path("body"), body.to_bytes()),
        (zhbo_native::camo_path("camo_gun"), blob("camo_gun", "gun_atacs.tga").to_bytes()),
        (zhbo_native::camo_path("camo_optic"), blob("camo_optic", "optic_atacs.tga").to_bytes()),
    ];
    for mode in [Mode::Weapon, Mode::Body, Mode::Lying] {
        let req = Request { vm: 7, faction: b'U', mode, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1, offhands: [-1; 2] };
        let mut files = Files::default();
        let mut loaded = None;
        for _ in 0..8 {
            if let Some(l) = load(&mut files, &req, Some("camo_atacs")).unwrap() {
                loaded = Some(l);
                break;
            }
            let wanted: Vec<String> = files.wanted().map(str::to_string).collect();
            for w in &wanted {
                files.provide(w, files_in.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice()));
            }
        }
        let l = loaded.expect("the rig loads");
        let camos: Vec<Option<&str>> = l.mesh.surfaces.iter().map(|s| s.camo.as_deref()).collect();
        let want: &[Option<&str>] = match mode {
            Mode::Weapon | Mode::Lying => &[Some("gun_atacs.tga"), Some("optic_atacs.tga")],
            Mode::Body => &[None, Some("gun_atacs.tga"), Some("optic_atacs.tga")],
        };
        assert_eq!(camos, want, "{mode:?}");
        assert_eq!(l.camo.as_deref(), Some("camo_atacs"));
    }
}

/// Only the class primary inherits the class camo; another weapon in hand, or no camo, is stock.
#[test]
fn the_class_camo_follows_the_primary_into_third_person() {
    let primary = crate::weapons::WeaponDef { id: "mp7_mp".into(), viewmodel: 5, camo_material: "camo_mp7".into(), ..Default::default() };
    assert_eq!(camo_for(&primary, "camo_atacs", 5).as_deref(), Some("camo_atacs"));
    assert_eq!(camo_for(&primary, "camo_atacs", 6), None, "the secondary");
    assert_eq!(camo_for(&primary, "camo_none", 5), None);
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
    let part = |m: &str, tag: Option<&str>| ViewmodelPart { model: zhbo_native::model_path(m), tag: tag.map(str::to_string), offset: IDENTITY, rigid: true, ads_only: false, camo: None, ads_swap: None };
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
        let m = s.loaded().unwrap().bone_world(&s.posed, bone).unwrap_or_else(|| panic!("no {bone}"));
        [m[0][3], m[1][3], m[2][3]]
    };
    let close = |a: [f32; 3], inches: [f32; 3]| (0..3).all(|k| (a[k] - crate::units::drawn_inches(inches[k])).abs() < 1e-3);
    let compose = |s: &mut ffi::State, vm: i32| {
        for _ in 0..20 {
            s.frame(&body_input(vm), 2, None);
            answer(s, &f);
        }
        s.frame(&body_input(vm), 2, None);
    };
    with_state(|s| {
        *s = ffi::State::default();
        ffi::publish_stowed(&[]);
        compose(s, 9);
        assert!(s.loaded().is_some(), "{:?}", s.logged().collect::<Vec<_>>());
        assert!(close(at(s, "j_shield"), [12.0, 8.0, 44.0]), "held on the left hand: {:?}", at(s, "j_shield"));
        // The rifle up, the shield on the back.
        ffi::publish_stowed(&[(1, 9)]);
        compose(s, 3);
        assert!(s.loaded().is_some_and(|l| l.key.1 == 3), "{:?}", s.logged().collect::<Vec<_>>());
        assert!(close(at(s, "j_gun"), [10.0, -8.0, 40.0]), "the rifle in the right hand");
        assert!(close(at(s, "j_stow"), [-6.0, 0.0, 50.0]), "stowed on the back: {:?}", at(s, "j_stow"));
        // He dies: his record (and the stowed shield it published) is gone, but his corpse keeps the
        // rig he died with rather than recomposing (the W3D body would show while it loaded).
        ffi::publish_stowed(&[]);
        ffi::publish_torso(std::iter::once(ffi::TorsoAction { general: 1, action: zhbo_native::thirdperson::torso::DEATH, clip: 0x1000, t: 0.5, lethal: false }));
        assert_eq!(s.frame(&body_input(3), 2, None), s.frame(&body_input(3), 2, None));
        assert!(s.loaded().is_some_and(|l| l.key.3 == 9), "the corpse's rig is the one he died with");
        ffi::publish_torso(std::iter::empty());
        // The weapon-only mode is placed by the engine's right-hand tag: the shield is not in that
        // hand, so it is refused (once) and the W3D weapon, skinned on tag_weapon_left, draws it.
        // The rifle still draws there.
        *s = ffi::State::default();
        let weapon_input = |vm: i32| input(vm, Some(tag([0.0; 3])));
        for _ in 0..20 {
            assert_ne!(s.frame(&weapon_input(9), 1, None), ffi::DRAWN, "the shield is never drawn on the right-hand tag");
            answer(s, &f);
        }
        assert!(s.logged().any(|l| l.contains("held on tag_weapon_left")), "{:?}", s.logged().collect::<Vec<_>>());
        for _ in 0..20 {
            s.frame(&weapon_input(3), 1, None);
            answer(s, &f);
        }
        assert!(s.loaded().is_some_and(|l| l.key.1 == 3), "the rifle composes in the weapon-only mode: {:?}", s.logged().collect::<Vec<_>>());
    });
}

/// The torso layer (`Loaded::pose`): a torso clip replaces the bones it tracks (the spine and
/// up) over the movement clip, fading in and out over `TORSO_BLEND`, and leaves the legs on the
/// movement clip; the weapon-only mode ignores it.
#[test]
fn a_torso_clip_moves_the_upper_body_over_the_legs() {
    use zhbo_native::anim::{Anim, BoneTrack};
    use zhbo_native::math::axis_angle;
    let body = model("body", vec![bone("tag_origin", None, [0.0; 3]), bone("j_hip_le", Some(0), [0.0, 4.0, 36.0]), bone("j_spinelower", Some(0), [0.0, 0.0, 40.0]), bone("tag_weapon_right", Some(2), [10.0, -8.0, 10.0])]);
    let gun = model("gun", vec![bone("tag_weapon", None, [0.0; 3])]);
    let manifest = Weapon { id: "test_mp".into(), parts: vec![ViewmodelPart { model: zhbo_native::model_path("gun"), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: None, ads_swap: None }], hide: vec![] };
    let body_manifest = zhbo_native::thirdperson::Body { faction: b'U', model: zhbo_native::model_path("body"), weapon_tag: "tag_weapon_right".into(), clips: vec![] };
    let files_in = [
        (zhbo_native::world_weapon_path(7), manifest.to_bytes()),
        (zhbo_native::model_path("gun"), gun.to_bytes()),
        (zhbo_native::body_path('U'), body_manifest.to_bytes()),
        (zhbo_native::model_path("body"), body.to_bytes()),
    ];
    let req = Request { vm: 7, faction: b'U', mode: Mode::Body, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1, offhands: [-1; 2] };
    let mut files = Files::default();
    let mut loaded = None;
    for _ in 0..8 {
        if let Some(l) = load(&mut files, &req, None).unwrap() {
            loaded = Some(l);
            break;
        }
        let wanted: Vec<String> = files.wanted().map(str::to_string).collect();
        for w in &wanted {
            files.provide(w, files_in.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice()));
        }
    }
    let l = loaded.expect("the body rig loads");
    // The legs' clip bends the hip; the torso clip (one second) turns the lower spine a quarter
    // turn and also names... nothing of the legs.
    let turn = axis_angle([0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2);
    let hip = axis_angle([1.0, 0.0, 0.0], 0.5);
    let track = |name: &str, q| BoneTrack { name: name.into(), rotation: Some(vec![(0, q), (30, q)]), translation: None };
    let legs = Anim { name: "pb_walk".into(), num_frames: 30, frame_rate: 30.0, looping: true, delta: false, asset_type: 0, bones: vec![track("j_hip_le", hip)], notes: vec![] };
    let torso = Anim { name: "pt_reload".into(), num_frames: 30, frame_rate: 30.0, looping: false, delta: false, asset_type: 0, bones: vec![track("j_spinelower", turn)], notes: vec![] };
    let (spine, hip_bone) = (l.skeleton.bone("j_spinelower").unwrap(), l.skeleton.bone("j_hip_le").unwrap());
    let mut out = Posed::default();
    let close = |a: [f32; 4], b: [f32; 4]| (0..4).all(|k| (a[k] - b[k]).abs() < 1e-4) || (0..4).all(|k| (a[k] + b[k]).abs() < 1e-4);
    // Mid-clip: the spine is the torso's, the hip the legs'.
    l.pose(&req, Some(&legs), Some((&torso, 0.5, false)), &mut out);
    assert!(close(out.local[spine].0, turn), "the torso turns the spine: {:?}", out.local[spine].0);
    assert!(close(out.local[hip_bone].0, hip), "the legs keep walking: {:?}", out.local[hip_bone].0);
    // At its very start and end it is the movement clip's (it fades in and out).
    l.pose(&req, Some(&legs), Some((&torso, 0.0, false)), &mut out);
    assert!(close(out.local[spine].0, QUAT_IDENTITY), "no torso at its first frame");
    l.pose(&req, Some(&legs), Some((&torso, 1.0, false)), &mut out);
    assert!(close(out.local[spine].0, QUAT_IDENTITY), "nor at its end");
    assert_eq!(torso_weight(TORSO_BLEND * 0.5, 1.0), 0.5);
    // A death is held at its end (the corpse), long past the clip.
    l.pose(&req, Some(&legs), Some((&torso, 5.0, true)), &mut out);
    assert!(close(out.local[spine].0, turn), "the corpse keeps the last frame");
    // A pin or a throw: the gun is not drawn (BO2's one entity weapon is the grenade then); the body
    // still is.
    l.pose(&req, Some(&legs), Some((&torso, 0.5, false)), &mut out);
    l.hide_weapon(&mut out);
    let gun_bone = l.skeleton.bone("tag_weapon").unwrap();
    assert_eq!(out.palette[gun_bone], [0.0; 16], "the gun collapses");
    assert_ne!(out.palette[spine], [0.0; 16], "the body draws");
    // Weapon-only mode is rigid: no torso.
    let rigid = Request { mode: Mode::Weapon, ..req };
    l.pose(&rigid, None, Some((&torso, 0.5, false)), &mut out);
    assert!(out.local.iter().all(|x| x.0 != turn));
}

/// The class's lethal and tactical ride `tag_inhand` (t6mp `FUN_0052a470`), drawn only while one
/// is primed or thrown; a missing offhand manifest is no grenade, not a failed rig.
#[test]
fn the_primed_offhand_rides_the_hand_tag() {
    let body = model("body", vec![bone("tag_origin", None, [0.0; 3]), bone("tag_weapon_right", Some(0), [10.0, -8.0, 40.0]), bone("tag_inhand", Some(0), [12.0, -6.0, 42.0])]);
    let gun = model("gun", vec![bone("tag_weapon", None, [0.0; 3])]);
    let nade = model("nade", vec![bone("j_nade", None, [0.0; 3])]);
    let part = |m: &str| ViewmodelPart { model: zhbo_native::model_path(m), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: None, ads_swap: None };
    let files_in = [
        (zhbo_native::world_weapon_path(7), Weapon { id: "gun_mp".into(), parts: vec![part("gun")], hide: vec![] }.to_bytes()),
        (zhbo_native::world_weapon_path(8), Weapon { id: "frag_grenade_mp".into(), parts: vec![part("nade")], hide: vec![] }.to_bytes()),
        (zhbo_native::model_path("gun"), gun.to_bytes()),
        (zhbo_native::model_path("nade"), nade.to_bytes()),
        (zhbo_native::body_path('U'), zhbo_native::thirdperson::Body { faction: b'U', model: zhbo_native::model_path("body"), weapon_tag: "tag_weapon_right".into(), clips: vec![] }.to_bytes()),
        (zhbo_native::model_path("body"), body.to_bytes()),
    ];
    let req = Request { vm: 7, faction: b'U', mode: Mode::Body, state: 0, time: 0.0, rate: 1.0, pos: [0.0; 3], yaw: 0.0, tag: None, stowed: -1, offhands: [8, 9] };
    let mut files = Files::default();
    let mut loaded = None;
    for _ in 0..12 {
        if let Some(l) = load(&mut files, &req, None).unwrap() {
            loaded = Some(l);
            break;
        }
        let wanted: Vec<String> = files.wanted().map(str::to_string).collect();
        for w in &wanted {
            files.provide(w, files_in.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice()));
        }
    }
    let l = loaded.expect("the rig loads without the tactical's manifest");
    assert!(l.offhand_parts[0].is_some() && l.offhand_parts[1].is_none());
    let nade_bone = l.skeleton.bone("j_nade").unwrap();
    let at = l.skeleton.bones[nade_bone].bind.1;
    assert!((at[0] - 12.0).abs() < 1e-4 && (at[2] - 42.0).abs() < 1e-4, "on tag_inhand: {at:?}");
    let mut out = Posed::default();
    l.pose(&req, None, None, &mut out);
    l.show_offhand(&mut out, None);
    assert_eq!(out.palette[nade_bone], [0.0; 16], "not in the hand until primed");
    l.pose(&req, None, None, &mut out);
    l.show_offhand(&mut out, Some(0));
    assert_ne!(out.palette[nade_bone], [0.0; 16], "the lethal in the hand");
}

/// A dropped weapon (`Mode::Lying`): the world weapon alone, on its side — its length along the
/// drop's yaw, its height (model z) across, its width (model y) up — at the General's drawn scale,
/// its lowest point on the ground at `pos`.
#[test]
fn a_dropped_weapon_lies_on_its_side_on_the_ground() {
    let gun = model("gun", vec![bone("tag_weapon", None, [0.0; 3])]);
    let manifest = Weapon { id: "test_mp".into(), parts: vec![ViewmodelPart { model: zhbo_native::model_path("gun"), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: None, ads_swap: None }], hide: vec![] };
    let files_in = [(zhbo_native::world_weapon_path(7), manifest.to_bytes()), (zhbo_native::model_path("gun"), gun.to_bytes())];
    let req = Request { vm: 7, faction: b'U', mode: Mode::Lying, state: 0, time: 0.0, rate: 1.0, pos: [100.0, 50.0, 3.0], yaw: std::f32::consts::FRAC_PI_2, tag: None, stowed: -1, offhands: [-1; 2] };
    let mut files = Files::default();
    let mut loaded = None;
    for _ in 0..8 {
        if let Some(l) = load(&mut files, &req, None).unwrap() {
            loaded = Some(l);
            break;
        }
        let wanted: Vec<String> = files.wanted().map(str::to_string).collect();
        for w in &wanted {
            files.provide(w, files_in.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice()));
        }
    }
    let l = loaded.expect("the weapon rig loads");
    assert!(l.body.is_none(), "no body");
    let mut out = Posed::default();
    l.pose(&req, None, None, &mut out);
    assert!((l.lowest(&out.palette) - 3.0).abs() < 1e-4, "on the ground at pos");
    // The triangle (0,0,0), (0,5,0), (0,0,5) in inches: model y (the side) points up, model z
    // (the height) across; facing +Y (yaw 90°), model +z goes to world +X.
    let k = ART_SCALE * crate::units::GENERAL_SCALE;
    let at = |p: [f32; 3]| {
        let m = &out.palette[0];
        [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
    };
    let (o, side, up) = (at([0.0; 3]), at([0.0, 5.0, 0.0]), at([0.0, 0.0, 5.0]));
    assert!((side[2] - o[2] - 5.0 * k).abs() < 1e-3, "its side points up: {o:?} {side:?}");
    assert!((up[0] - o[0] - 5.0 * k).abs() < 1e-3 && (up[2] - o[2]).abs() < 1e-3, "its height lies across: {up:?}");
    assert!((o[0] - 100.0).abs() < 1e-3 && (o[1] - 50.0).abs() < 1e-3, "where it lies");
}

/// A dropped weapon's rig goes with the drop (each drop is a new object): `retain_dropped` keeps
/// the lying rigs still listed and a General's whatever it is given.
#[test]
fn a_gone_drops_rig_is_released() {
    let f = fixture();
    with_state(|s| {
        for (general, mode) in [(1u32, 1), (900, super::ffi::LYING), (901, super::ffi::LYING)] {
            let mut inp = ZhboNativeBodyIn { general, mode, vm: 7, faction: b'U' as i32, rate: 1.0, tag_valid: 1, ..Default::default() };
            inp.tag[0] = 1.0;
            inp.tag[5] = 1.0;
            inp.tag[10] = 1.0;
            for _ in 0..8 {
                if s.frame(&inp, 1, None) != LOADING {
                    break;
                }
                let wanted: Vec<String> = s.files.wanted().map(str::to_string).collect();
                for w in &wanted {
                    s.files.provide(w, f.files.iter().find(|(p, _)| p == w).map(|(_, b)| b.as_slice()));
                }
            }
        }
        let before = s.rig_count();
        s.retain_dropped(&[901]);
        assert_eq!(s.rig_count(), before - 1, "the drop that went is released, the General and the lying one stay");
    });
}
