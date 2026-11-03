//! The converter's rules for each BO2 effect element (`fx::particle_system` and friends), one test
//! per rule.

use super::*;
use zhbo_formats::fx::{Range, VelSample, VisSample};

fn sprite() -> Elem {
    Elem {
        kind: kind::SPRITE_BILLBOARD,
        flags: SPAWN_OFFSET_SPHERE,
        count: Range { base: 5.0, amplitude: 2.0 },
        life_msec: Range { base: 1000.0, amplitude: 500.0 },
        delay_msec: Range { base: 100.0, amplitude: 0.0 },
        offset_radius: Range { base: 0.0, amplitude: 12.0 },
        gravity: Range { base: 1.0, amplitude: 0.0 },
        velocity: ([0.3, 0.0, 0.0], [0.1, 0.0, 0.0]),
        vis: vec![
            VisSample { color: [200, 180, 160, 255], color_amplitude: [200, 180, 160, 255], size: [6.0, 6.0], ..Default::default() },
            VisSample { color: [100, 90, 80, 0], color_amplitude: [100, 90, 80, 0], size: [30.0, 30.0], ..Default::default() },
        ],
        ..Default::default()
    }
}

#[test]
fn an_element_keeps_its_timing_size_and_fade() {
    let ini = particle_system("ZHBO_test_0", &sprite(), "zhbo_fx_smoke.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 15.0, role: Role::Blast, orient: false }).unwrap();
    // 1 to 1.5 s, plus the frame Zero Hour counts down before the first draw.
    assert!(ini.contains("Lifetime = 31 46"), "{ini}");
    // A whole random count of 5..=7 (Zero Hour truncates the burst count).
    assert!(ini.contains("BurstCount = 5.000 7.999"), "{ini}");
    assert!(ini.contains("InitialDelay = 2 2"));
    assert!(ini.contains("Alpha1 = 1.00 1.00 0") && ini.contains("Alpha2 = 0.00 0.00 39"), "{ini}");
    assert!(ini.contains("Shader = ALPHA") && ini.contains("VolumeType = SPHERE"));
    assert!(ini.contains("Priority = CRITICAL"), "{ini}");
    // Out along the normal (up), falling.
    assert!(ini.contains("VelOrthoZ = 1.667 2.222"), "{ini}");
    assert!(ini.contains("Gravity = -0.1481"), "{ini}");
}

#[test]
fn each_role_takes_zero_hours_priority_for_its_kind() {
    let ini = |role| particle_system("ZHBO_test_r", &sprite(), "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 12.0, role, orient: false }).unwrap();
    for (role, p) in [(Role::Blast, "CRITICAL"), (Role::Impact, "CRITICAL"), (Role::Flash, "CRITICAL"), (Role::Shell, "WEAPON_EXPLOSION"), (Role::Dust, "WEAPON_EXPLOSION"), (Role::Area, "AREA_EFFECT")] {
        assert!(ini(role).contains(&format!("Priority = {p}\n")), "{role:?}");
    }
    // The lowest priority is only for what may vanish under load.
    let model = Elem { kind: kind::MODEL, count: Range { base: 1.0, amplitude: 0.0 }, ..Default::default() };
    assert!(model_particle_system("ZHBO_test_s", &model, "s.tga", Spawn::START, Play { looping_secs: 1.0, role: Role::Shell, orient: true }).unwrap().contains("Priority = WEAPON_EXPLOSION"));
}

#[test]
fn an_effect_in_two_roles_takes_the_higher_priority() {
    let light = Elem { kind: kind::OMNI_LIGHT, ..Default::default() };
    let e = Effect { name: "test/both".into(), elems: vec![light], ..Default::default() };
    let mut c = Converted::default();
    let name = c.add_as(&e, Role::Dust, &|_| None);
    assert_eq!(c.add_as(&e, Role::Blast, &|_| None), name);
    assert_eq!(c.roles.get(&name), Some(&Role::Blast));
    // and never lowered
    c.add_as(&e, Role::Shell, &|_| None);
    assert_eq!(c.roles.get(&name), Some(&Role::Blast));
}

#[test]
fn looping_elements_spawn_one_per_interval_count_times() {
    // Every 3 s, five times: one particle each 90 frames for 450 frames.
    let slow = Elem { looping: true, interval_msec: 3000, ..sprite() };
    let ini = particle_system("ZHBO_test_1", &slow, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 15.0, role: Role::Blast, orient: false }).unwrap();
    assert!(ini.contains("SystemLifetime = 450") && ini.contains("BurstDelay = 89 89") && ini.contains("BurstCount = 1.000 1.000"), "{ini}");
    // Every 3 msec, twenty times (the grenade's sparks, BO2's 57 msec looping life): eleven
    // or twelve a frame for two frames (11.1 a frame on average: 33.3 / 3), not twenty a frame for a second.
    let fast = Elem { looping: true, interval_msec: 3, count: Range { base: 20.0, amplitude: 0.0 }, ..sprite() };
    let ini = particle_system("ZHBO_test_2", &fast, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 15.0, role: Role::Blast, orient: false }).unwrap();
    assert!(ini.contains("SystemLifetime = 2") && ini.contains("BurstDelay = 0 0") && ini.contains("BurstCount = 11.111 12.110"), "{ini}");
    // Count 0: for as long as the effect loops.
    let forever = Elem { looping: true, interval_msec: 1000, count: Range::default(), ..sprite() };
    let ini = particle_system("ZHBO_test_3", &forever, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 15.0, role: Role::Blast, orient: false }).unwrap();
    assert!(ini.contains("SystemLifetime = 450"), "{ini}");
}

#[test]
fn the_velocity_graph_sets_how_far_it_goes() {
    // Starts at 0.44 in/ms and slows to nothing: the graph's delta says it covers half the
    // distance of the starting speed held for its life.
    let s = |v: f32, d: f32| VelSample { local_velocity: ([v, 0.0, 0.0], [0.0; 3]), local_delta: ([d, 0.0, 0.0], [0.0; 3]), ..Default::default() };
    let el = Elem { vel: vec![s(0.44, 0.0), s(0.0, 0.22)], ..sprite() };
    let ini = particle_system("ZHBO_test_4", &el, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    let damping: f32 = ini.lines().find_map(|l| l.trim().strip_prefix("VelocityDamping = ")).unwrap().split_whitespace().next().unwrap().parse().unwrap();
    // Zero Hour's travel over the mean life (37.5 frames) matches BO2's.
    let frames = 37.5f32;
    let v0 = units(0.44) * MS_PER_FRAME;
    let zh = v0 * (1.0 - damping.powf(frames)) / (1.0 - damping);
    let bo2 = units(0.22 * 1250.0);
    assert!((zh - bo2).abs() / bo2 < 0.01, "damping {damping}: {zh} vs {bo2}");
    assert_eq!(fitted_damping(1.0, 30.0), 1.0f32.min(fitted_damping(1.0, 30.0)));
    assert!((fitted_damping(1.0, 30.0) - 1.0).abs() < 1e-3);
}

#[test]
fn short_lives_keep_strictly_increasing_key_frames() {
    // Five samples over a 2-frame life: keys on frames 0, 1 (rounded 0.5 -> 1 is skipped as
    // a repeat where it lands on the same frame), 2 — never a second key on frame 0.
    let v = |a: u8| VisSample { color: [255, 255, 255, a], color_amplitude: [255, 255, 255, a], size: [4.0, 4.0], ..Default::default() };
    let el = Elem { life_msec: Range { base: 66.0, amplitude: 0.0 }, vis: vec![v(255), v(200), v(150), v(100), v(0)], ..sprite() };
    let ini = particle_system("ZHBO_test_5", &el, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    let frames: Vec<u32> = ini.lines().filter(|l| l.trim().starts_with("Alpha")).map(|l| l.split_whitespace().last().unwrap().parse().unwrap()).collect();
    assert!(frames.windows(2).all(|w| w[1] > w[0]), "{frames:?}\n{ini}");
    assert_eq!(frames[0], 0);
}

#[test]
fn an_element_bo2_spawns_none_of_is_not_drawn() {
    let none = Elem { count: Range::default(), ..sprite() };
    let mut c = Converted::default();
    let e = Effect { name: "test/none".into(), elems: vec![none], ..Default::default() };
    let name = c.add(&e, &|_| None);
    assert_eq!(c.body(&name), Some(""));
}

#[test]
fn runners_start_their_effect_and_lights_pulse() {
    // A runner 10 inches up its X starting an effect whose only element is an omni light: the
    // list carries the light, from BO2's colour, radius and life.
    let light = Elem {
        kind: kind::OMNI_LIGHT,
        count: Range { base: 1.0, amplitude: 0.0 },
        life_msec: Range { base: 100.0, amplitude: 0.0 },
        vis: vec![VisSample { color: [250, 200, 150, 255], color_amplitude: [250, 200, 150, 255], size: [120.0, 0.0], ..Default::default() }],
        ..Default::default()
    };
    let child = Effect { name: "test/flash".into(), elems: vec![light], ..Default::default() };
    let runner = Elem { kind: kind::RUNNER, count: Range { base: 1.0, amplitude: 0.0 }, origin: [Range { base: 10.0, amplitude: 0.0 }, Range::default(), Range::default()], children: vec![child.clone()], ..Default::default() };
    // A runner BO2 spawns none of starts nothing.
    let idle = Elem { count: Range::default(), ..runner.clone() };
    let e = Effect { name: "test/blast".into(), elems: vec![runner, idle], ..Default::default() };
    let mut c = Converted::default();
    let name = c.add(&e, &|_| None);
    let body = c.body(&name).unwrap();
    assert_eq!(body.matches("LightPulse").count(), 1, "{body}");
    assert!(body.contains("Color = R:250 G:200 B:150") && body.contains("Radius = 20.0") && body.contains("DecreaseTime = 100\n"), "{body}");
}

#[test]
fn spawn_offsets_go_on_the_nugget_in_the_lists_frame() {
    let el = Elem { origin: [Range { base: 20.0, amplitude: 160.0 }, Range { base: -6.0, amplitude: 12.0 }, Range::default()], ..sprite() };
    // The middle of the box, 100 inches up the normal: up in a world list, X in a framed one.
    let world = frame_vec(false, origin_centre(&el));
    let framed = frame_vec(true, origin_centre(&el));
    assert!((world[2] - units(100.0)).abs() < 1e-4 && world[0].abs() < 1e-4, "{world:?}");
    assert!((framed[0] - units(100.0)).abs() < 1e-4, "{framed:?}");
    let text = fxlist_particles(&[Placed { system: "S".into(), offset: world }], false);
    assert!(text.contains("Offset = X:0.00 Y:0.00 Z:16.67"), "{text}");
    // The box's half extent spawns around it.
    let ini = particle_system("ZHBO_test_6", &Elem { flags: 0, ..el }, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    assert!(ini.contains("VolumeType = BOX") && ini.contains("VolBoxHalfSize = X:1.00 Y:0.00 Z:13.33"), "{ini}");
}

#[test]
fn a_model_element_becomes_a_shell_sprite() {
    let el = Elem { kind: kind::MODEL, count: Range { base: 1.0, amplitude: 0.0 }, models: vec!["fx_rifle_shell".into()], ..Default::default() };
    let ini = model_particle_system("ZHBO_test_shell", &el, "zhbo_fx_shell.tga", Spawn::START, Play { looping_secs: 1.0, role: Role::Shell, orient: false }).unwrap();
    assert!(ini.contains("ParticleSystem ZHBO_test_shell"), "{ini}");
    assert!(ini.contains("ParticleName = zhbo_fx_shell.tga"));
    // BO2's model element has no size: a shell-sized default life and sprite.
    assert!(ini.contains("Lifetime = 22 28"), "{ini}");
    assert!(ini.contains("Size = 0.17 0.17"), "{ini}");
}

#[test]
fn a_shell_fxlist_orients_to_the_tag_with_bo2_axes() {
    let el = Elem { kind: kind::MODEL, count: Range { base: 1.0, amplitude: 0.0 }, velocity: ([0.3, 0.0, 0.0], [0.1, 0.0, 0.0]), ..Default::default() };
    let ini = model_particle_system("ZHBO_test_shell", &el, "zhbo_fx_shell.tga", Spawn::START, Play { looping_secs: 1.0, role: Role::Shell, orient: true }).unwrap();
    // The tag's X is the eject direction: identity axes, nothing on Z (the world effects' map
    // puts it there, and the existing test still pins that).
    assert!(ini.contains("VelOrthoX = 1.667 2.222"), "{ini}");
    assert!(ini.contains("VelOrthoY = 0.000 0.000") && ini.contains("VelOrthoZ = 0.000 0.000"), "{ini}");
    let systems = vec![Placed { system: "ZHBO_test_shell".to_string(), offset: [0.0; 3] }];
    assert!(fxlist_particles(&systems, true).contains("    Name = ZHBO_test_shell\n    OrientToObject = Yes\n"), "shell");
    assert!(!fxlist_particles(&systems, false).contains("OrientToObject"), "world");
}

#[test]
fn a_flipbook_is_laid_out_on_zero_hours_square_grid() {
    use zhbo_formats::iwi::Rgba;
    // An 8 x 4 atlas of 32 frames, each cell filled with its own index.
    let (cw, ch) = (4u32, 4u32);
    let mut img = Rgba { width: 8 * cw, height: 4 * ch, pixels: vec![0; (8 * cw * 4 * ch * 4) as usize] };
    for y in 0..img.height {
        for x in 0..img.width {
            let i = ((y * img.width + x) * 4) as usize;
            img.pixels[i] = ((y / ch) * 8 + x / cw) as u8;
            img.pixels[i + 3] = 255;
        }
    }
    let grid = Grid::of(32);
    assert_eq!(grid, Grid { side: 8, frames: 32 });
    let out = flipbook_grid(&img, (8, 4), grid);
    assert_eq!((out.width, out.height), (8 * cw, 8 * ch));
    // Frame n sits at row n / 8, column n % 8 of the grid (W3D's frame UV table order).
    for n in [0u32, 7, 8, 20, 31] {
        let (gx, gy) = (n % 8, n / 8);
        let i = (((gy * ch + 1) * out.width + gx * cw + 1) * 4) as usize;
        assert_eq!(out.pixels[i] as u32, n, "frame {n}");
    }
    // Past the last frame the grid is empty.
    let i = (((7 * ch + 1) * out.width + 1) * 4) as usize;
    assert_eq!(out.pixels[i + 3], 0);
    // One frame: the first cell alone, as before.
    assert_eq!(flipbook_grid(&img, (8, 4), Grid::NONE).width, cw);
    assert_eq!((Grid::of(2).side, Grid::of(5).side, Grid::of(64).side, Grid::of(300).frames), (2, 4, 8, 256));
    assert_eq!(texture_file("fxt_fire", Grid::of(32)), "zhbo_fx_fxt_fire_f32.tga");
}

#[test]
fn a_flipbook_element_plays_its_frames() {
    use zhbo_formats::fx::Flipbook;
    // BO2's damage smoke: a random start, every frame once over the life (behaviour 5).
    let el = Elem { flipbook: Flipbook { behavior: 5, index: 0, fps: 0, loop_count: 1, frames: 32 }, atlas: (8, 4), ..sprite() };
    let ini = particle_system("ZHBO_test_fb", &el, "t.tga", false, Grid::of(32), Spawn::START, Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    for f in ["FrameGrid = 8\n", "FrameCount = 32\n", "FrameStartType = RANDOM\n", "FrameOverLife = Yes\n", "FrameLoops = 0\n"] {
        assert!(ini.contains(f), "{f}\n{ini}");
    }
    // A fixed start at 30 fps, held after two passes.
    let el = Elem { flipbook: Flipbook { behavior: 8, index: 3, fps: 30, loop_count: 2, frames: 16 }, atlas: (4, 4), ..sprite() };
    let ini = particle_system("ZHBO_test_fb2", &el, "t.tga", false, Grid::of(16), Spawn::START, Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    for f in ["FrameGrid = 4\n", "FrameStartType = FIXED\n", "FrameStart = 3\n", "FrameRate = 30.00\n", "FrameOverLife = No\n", "FrameLoops = 2\n"] {
        assert!(ini.contains(f), "{f}\n{ini}");
    }
    // A plain texture writes none of it (Zero Hour's stock fields only).
    let ini = particle_system("ZHBO_test_fb3", &sprite(), "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    assert!(!ini.contains("Frame"), "{ini}");
}

#[test]
fn a_looping_runner_starts_its_effect_every_interval() {
    // BO2's sentry damage state: a runner every 100 msec, forever, of a one-shot smoke puff.
    let puff = Elem { count: Range { base: 1.0, amplitude: 0.0 }, delay_msec: Range::default(), ..sprite() };
    let runner = Elem { kind: kind::RUNNER, looping: true, interval_msec: 100, count: Range { base: 2.147_483_6e9, amplitude: 0.0 }, ..Default::default() };
    let at = Spawn::START.run(&runner, 6.0);
    assert_eq!(at.repeat, Some((100.0, 6000.0)));
    assert!((at.times - 1.0).abs() < 1e-4, "{at:?}");
    let ini = particle_system("ZHBO_test_run", &puff, "t.tga", false, Grid::NONE, at, Play { looping_secs: 6.0, role: Role::Blast, orient: false }).unwrap();
    // A puff every three frames (100 msec) for the six seconds, not one puff.
    assert!(ini.contains("BurstDelay = 2 2") && ini.contains("SystemLifetime = 180") && ini.contains("BurstCount = 1.000 1.999"), "{ini}");
    // A one-shot runner of count 3 stands for three of its effect.
    let three = Elem { kind: kind::RUNNER, count: Range { base: 3.0, amplitude: 0.0 }, ..Default::default() };
    assert_eq!(Spawn::START.run(&three, 1.0).times, 3.0);
    let ini = particle_system("ZHBO_test_run3", &puff, "t.tga", false, Grid::NONE, Spawn::START.run(&three, 1.0), Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    assert!(ini.contains("BurstCount = 3.000 3.999"), "{ini}");
    // A looping element under a looping runner: 4 runs x 1 particle, not 4 a burst twice.
    let looped = Elem { looping: true, interval_msec: 500, count: Range { base: 1.0, amplitude: 0.0 }, ..sprite() };
    let four = Elem { kind: kind::RUNNER, looping: true, interval_msec: 50, count: Range { base: 4.0, amplitude: 0.0 }, ..Default::default() };
    let ini = particle_system("ZHBO_test_run4", &looped, "t.tga", false, Grid::NONE, Spawn::START.run(&four, 1.0), Play { looping_secs: 1.0, role: Role::Blast, orient: false }).unwrap();
    let get = |k: &str| -> f32 { ini.lines().find_map(|l| l.trim().strip_prefix(k)).unwrap().split_whitespace().next().unwrap().parse().unwrap() };
    let bursts = (get("SystemLifetime = ") / (get("BurstDelay = ") + 1.0)).ceil();
    assert!((bursts * get("BurstCount = ") - 4.0).abs() < 0.5, "{ini}");
}

#[test]
fn the_size_follows_bo2s_whole_graph() {
    // The grenade's trail puff swells from 2.5 to 3.5 inches and back to nothing while it
    // fades: matching the first slope and the end shrank it from the start.
    let sizes = [2.5f32, 3.0, 3.5, 3.5, 3.5, 3.0, 2.4, 1.8, 0.0].map(|s| units(2.0 * s));
    let shows = [0.58f32, 0.43, 0.31, 0.22, 0.15, 0.11, 0.06, 0.03, 0.0];
    let (rate, damping) = fit_size(&sizes, &shows, sizes[0], 26.0);
    // While it is most seen (the first third) it grows, as BO2's does.
    let at = |a: u32| sizes[0] + rate * (0..a).map(|k| damping.powi(k as i32)).sum::<f32>();
    assert!(at(4) > sizes[0] && at(8) > sizes[0], "rate {rate} damping {damping}");
    assert!((at(8) - sizes[2]).abs() < 0.25 * sizes[2], "{} vs {}", at(8), sizes[2]);
    // A straight graph is fitted straight.
    let line = [1.0f32, 2.0, 3.0, 4.0, 5.0];
    let (r, d) = fit_size(&line, &[1.0; 5], 1.0, 40.0);
    assert!((d - 1.0).abs() < 0.01 && (r - 0.1).abs() < 0.01, "{r} {d}");
}

#[test]
fn a_particle_shorter_than_a_frame_shows_fainter() {
    // An 18 msec gas flash is drawn for one 33 msec frame: at 18/33 of BO2's opacity.
    let flash = Elem { life_msec: Range { base: 18.0, amplitude: 0.0 }, ..sprite() };
    let ini = particle_system("ZHBO_test_gas", &flash, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 1.0, role: Role::Flash, orient: false }).unwrap();
    assert!(ini.contains("Alpha1 = 0.54 0.54 0"), "{ini}");
    // A long one is untouched.
    let ini = particle_system("ZHBO_test_long", &sprite(), "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 1.0, role: Role::Flash, orient: false }).unwrap();
    assert!(ini.contains("Alpha1 = 1.00 1.00 0"), "{ini}");
}

#[test]
fn a_projectile_trail_rides_its_object_as_a_streak() {
    let trail = Elem { kind: kind::TRAIL, looping: true, interval_msec: 100, count: Range { base: 50.0, amplitude: 0.0 }, ..sprite() };
    let ini = particle_system("ZHBO_test_tr", &trail, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 5.0, role: Role::Trail, orient: true }).unwrap();
    assert!(ini.contains("Type = STREAK") && ini.contains("Priority = WEAPON_TRAIL"), "{ini}");
    // The same element in a blast stays sprites.
    let ini = particle_system("ZHBO_test_tr2", &trail, "t.tga", false, Grid::NONE, Spawn::START, Play { looping_secs: 5.0, role: Role::Blast, orient: false }).unwrap();
    assert!(ini.contains("Type = PARTICLE"), "{ini}");
    let text = "Object ZHBO_Proj_a\n  Draw = W3DModelDraw ModuleTag_01\n    DefaultConditionState\n      Model = A\n    End\n  End\nEnd\n\nObject ZHBO_Proj_b\n  Draw = W3DModelDraw ModuleTag_01\n    DefaultConditionState\n      Model = B\n    End\n  End\nEnd\n";
    let out = crate::props::with_trails(text, &[("ZHBO_Proj_b".to_string(), vec!["S0".to_string(), "S1".to_string()])]);
    assert!(out.contains("      Model = B\n      ParticleSysBone = None S0\n      ParticleSysBone = None S1\n"), "{out}");
    assert!(out.contains("      Model = A\n    End"), "{out}");
}


#[test]
fn a_blasts_decal_is_a_scorch_mark() {
    // The grenade's scorch: 45 + 15 inches, once.
    let decal = Elem { kind: kind::DECAL, count: Range { base: 1.0, amplitude: 0.0 }, vis: vec![VisSample { size: [45.0, 0.0], size_amplitude: [15.0, 0.0], ..Default::default() }], ..Default::default() };
    let e = Effect { name: "test/scorched".into(), elems: vec![decal], ..Default::default() };
    let mut c = Converted::default();
    let blast = c.add(&e, &|_| None);
    assert!(c.body(&blast).unwrap().contains(&format!("  TerrainScorch\n    Type = RANDOM\n    Radius = {:.1}\n  End\n", units(52.5))));
    // A bullet's hole is not put on the ground.
    let hit = c.add_framed(&e, Role::Impact, &|_| None);
    assert!(!c.body(&hit).unwrap().contains("TerrainScorch"));
}


#[test]
fn a_particle_starts_its_death_effect_where_and_when_it_dies() {
    // A wisp 5 inches up, falling for 2 s (gravity 0.5: 400 in/s^2 -> 800 inches): it lands
    // (an upright list stops it at the ground) and its death effect, a light, starts there 2 s on.
    let light = Elem { kind: kind::SPRITE_BILLBOARD, count: Range { base: 1.0, amplitude: 0.0 }, life_msec: Range { base: 100.0, amplitude: 0.0 }, vis: sprite().vis, materials: vec![], ..Default::default() };
    let after = Effect { name: "test/after".into(), elems: vec![light], ..Default::default() };
    let wisp = Elem {
        count: Range { base: 1.0, amplitude: 0.0 },
        life_msec: Range { base: 2000.0, amplitude: 0.0 },
        delay_msec: Range::default(),
        origin: [Range { base: 5.0, amplitude: 0.0 }, Range::default(), Range::default()],
        gravity: Range { base: 0.5, amplitude: 0.0 },
        flags: 0,
        on_death: Some(Box::new(after)),
        ..sprite()
    };
    let t = death_travel(&wisp, true);
    assert!((t[0] + 5.0).abs() < 1e-4, "{t:?}");
    assert!((death_travel(&wisp, false)[0] + 800.0).abs() < 1e-2);
    let at = Spawn::START.run(&wisp, 1.0);
    assert_eq!(at.times, 1.0);
}
