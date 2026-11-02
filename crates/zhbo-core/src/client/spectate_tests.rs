//! The spectating client's tests: it never sends, never captures, follows the sim's aim smoothly,
//! switches, goes overhead and back, moves on when the followed General dies, and takes over from
//! a player whose General fell as he became a spectator.
use super::*;
use crate::client::ClientOut;
use crate::host::objflags;
use crate::input::{FpsInput, Key};
use crate::math::{dir_from_angles, Vec3};
use crate::sim::tests::{general, spawn, FakeHost};
use crate::units::LOGIC_DT;

/// The observer's own seat: no General.
const OBSERVER: i32 = 5;

/// Two players' Generals in first person, each with a class.
fn match_of_two() -> (FakeHost, Sim) {
    let host = FakeHost::default();
    let mut g1 = general(1, 0);
    g1.pos = Vec3::new(0.0, 0.0, 0.0);
    let mut g2 = general(2, 1);
    g2.pos = Vec3::new(200.0, 50.0, 0.0);
    host.add(g1);
    host.add(g2);
    let mut sim = Sim::new();
    spawn(&mut sim, &host, 0, 1);
    spawn(&mut sim, &host, 1, 2);
    (host, sim)
}

fn spectator_in() -> ClientIn {
    ClientIn { local_player: OBSERVER, spectating: 1, input_allowed: 1, ..Default::default() }
}

/// What a spectating frame must never do.
fn assert_silent(out: &ClientOut) {
    assert_eq!(out.send_input, 0, "a spectator sends no input");
    assert_eq!(out.choose_class, 0, "a spectator picks no class");
    assert_eq!(out.select_object, 0, "a spectator selects nothing");
    assert_eq!(out.capture_mouse, 0, "a spectator never takes the mouse");
    assert_eq!(out.menu_open, 0, "a spectator has no class screen");
    assert_eq!(out.save_profile, 0);
}

/// One logic frame: the players' inputs (what the network delivered), `renders` spectator frames
/// across it, then the step.
fn tick(client: &mut Client, sim: &mut Sim, host: &FakeHost, inp: &mut ClientIn, inputs: &[(i32, u32, FpsInput)], renders: u32) -> Vec<ClientOut> {
    let mut outs = Vec::new();
    for r in 0..renders {
        inp.dt = LOGIC_DT / renders as f32;
        let out = client.frame(sim, inp);
        assert_silent(&out);
        outs.push(out);
        // Press edges are a frame long.
        if r == 0 {
            inp.keys = Default::default();
            inp.mouse_buttons = 0;
        }
    }
    for &(player, g, input) in inputs {
        sim.input(player, g, input);
    }
    let f = sim.frame + 1;
    sim.step(host, f);
    inp.logic_frame = f;
    outs
}

fn settle(client: &mut Client, sim: &mut Sim, host: &FakeHost, inp: &mut ClientIn, frames: u32) -> ClientOut {
    let mut last = ClientOut::default();
    for _ in 0..frames {
        last = *tick(client, sim, host, inp, &[], 1).last().unwrap();
    }
    last
}

#[test]
fn a_spectator_follows_a_first_person_general_and_sends_nothing() {
    let (host, mut sim) = match_of_two();
    let mut client = Client::default();
    let mut inp = spectator_in();
    // Everything a player would press, held and moved, and the bar's one-shots.
    inp.keys.set_down(Key::W, true);
    inp.keys.set_down(Key::Shift, true);
    inp.mouse_dx = 40.0;
    inp.mouse_dy = -15.0;
    inp.class_button = 1;
    inp.class_pick_force = 1;
    let held = inp;
    for _ in 0..90 {
        let frame = inp.logic_frame;
        inp = ClientIn { logic_frame: frame, ..held };
        tick(&mut client, &mut sim, &host, &mut inp, &[], 2);
    }
    inp = ClientIn { logic_frame: inp.logic_frame, ..held };
    let out = client.frame(&mut sim, &inp);
    assert_silent(&out);
    assert_eq!(client.view, ViewMode::Fps, "it flew into the first candidate's eyes");
    assert_eq!(out.hide_object, 1, "the followed General's body is hidden, as his own client hides it");
    assert_eq!(client.viewed, 0);
    assert_eq!(out.hide_rts_ui, 1);
    assert!(out.viewmodel >= 0, "his gun is drawn");
    assert!(!client.class_pick_open);
    // The mouse never turned the view: it is the General's own aim (his input is the default 0).
    let p = sim.player(0).unwrap();
    let want = dir_from_angles(p.input.yaw, p.input.pitch);
    assert!((out.dir - want).length() < 1e-4, "{:?} vs {want:?}", out.dir);
}

/// The followed view is the input angles the sim stepped on, interpolated across the logic frame
/// like the eye: at a logic frame's start it is the previous frame's aim, then it turns toward the
/// current one, never past it.
#[test]
fn the_followed_aim_is_the_sims_smoothed_between_logic_frames() {
    let (host, mut sim) = match_of_two();
    let mut client = Client::default();
    let mut inp = spectator_in();
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!(client.view, ViewMode::Fps);
    let step = 0.02;
    let mut yaw = 0.0f32;
    for _ in 0..20 {
        let prev = sim.player(0).unwrap().input.yaw;
        yaw += step;
        let outs = tick(&mut client, &mut sim, &host, &mut inp, &[(0, 1, FpsInput { yaw, ..Default::default() })], 4);
        // The frames rendered across this logic frame turned from the aim two steps back to the
        // last one the sim stepped on, monotonically.
        let yaws: Vec<f32> = outs.iter().map(|o| o.dir.y.atan2(o.dir.x)).collect();
        assert!(yaws.windows(2).all(|w| w[1] >= w[0] - 1e-6), "smooth: {yaws:?}");
        assert!(yaws.iter().all(|&y| y <= prev + 1e-4), "never ahead of the sim: {yaws:?} vs {prev}");
    }
    // Once the aim stops, the view lands exactly on it.
    let out = settle(&mut client, &mut sim, &host, &mut inp, 3);
    assert!((out.dir.y.atan2(out.dir.x) - yaw).abs() < 1e-4);
}

/// BO2's switch bar: the attack button the previous player, the aim button the next, cut at once;
/// the view toggle leaves for the overhead view and comes back.
#[test]
fn the_spectator_switches_goes_overhead_and_comes_back() {
    let (host, mut sim) = match_of_two();
    let mut client = Client::default();
    let mut inp = spectator_in();
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!(client.viewed, 0);

    inp.keys.set_down(Key::MouseRight, true);
    let out = tick(&mut client, &mut sim, &host, &mut inp, &[], 1)[0];
    assert_eq!((client.viewed, out.hide_object), (1, 2), "the next General, at once");
    assert_eq!(client.view, ViewMode::Fps, "a cut, not a fly-out");
    let eye = sim.player(1).unwrap().body.eye();
    assert!((out.eye - eye).length() < 1e-3, "his eyes, not a blend from the last ones");
    settle(&mut client, &mut sim, &host, &mut inp, 2);
    inp.keys.set_down(Key::MouseLeft, true);
    tick(&mut client, &mut sim, &host, &mut inp, &[], 1);
    assert_eq!(client.viewed, 0, "the previous, wrapping");

    // The view toggle (the field-computer key): overhead.
    inp.keys.set_down(Key::F, true);
    tick(&mut client, &mut sim, &host, &mut inp, &[], 1);
    assert!(matches!(client.view, ViewMode::Leaving { .. }));
    let out = settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!((client.view, out.camera_override, client.viewed), (ViewMode::Rts, 0, -1));
    assert_eq!(out.rts_look_at_valid, 0, "the landing frame is past");
    // It stays overhead while both play on, and its events are third person only.
    settle(&mut client, &mut sim, &host, &mut inp, 30);
    assert_eq!(client.view, ViewMode::Rts);
    // The middle button brings it back to the one it followed.
    inp.mouse_buttons = crate::input::mouse_buttons::MIDDLE;
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!((client.view, client.viewed), (ViewMode::Fps, 0));
}

/// The followed General dies: the view moves on to the next one in first person; with nobody
/// left it flies out overhead, and follows the next General who enters.
#[test]
fn a_followed_generals_death_moves_on_then_goes_overhead() {
    let (host, mut sim) = match_of_two();
    let mut client = Client::default();
    let mut inp = spectator_in();
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!(client.viewed, 0);
    let kill = |id: u32| {
        for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
            o.flags |= objflags::DEAD;
        }
    };
    kill(1);
    settle(&mut client, &mut sim, &host, &mut inp, 2);
    assert_eq!((client.viewed, client.view), (1, ViewMode::Fps), "cut to the other General");
    assert_eq!(client.hud.death_banner, 0.0, "no GENERAL DOWN for someone else's General");
    kill(2);
    settle(&mut client, &mut sim, &host, &mut inp, 2);
    assert!(matches!(client.view, ViewMode::Leaving { .. }), "nobody left: overhead");
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!(client.view, ViewMode::Rts);
    // A fresh General enters first person: he is followed.
    let mut g3 = general(3, 1);
    g3.pos = Vec3::new(50.0, 0.0, 0.0);
    host.add(g3);
    spawn(&mut sim, &host, 1, 3);
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!((client.view, client.viewed), (ViewMode::Fps, 1));
    assert_eq!(client.general, 3);
}

/// The local player's General falls and the same frame he is out of the game (defeated, he stays
/// to watch): his own death plays out — the banner and the fly-out — then the client follows the
/// General still in first person, sending nothing from then on.
#[test]
fn a_player_whose_general_dies_as_he_becomes_a_spectator_sees_his_death_then_follows() {
    let (host, mut sim) = match_of_two();
    let mut client = Client::default();
    let mut inp = ClientIn { local_player: 0, input_allowed: 1, dt: LOGIC_DT, ..Default::default() };
    // His own client: first person, sending.
    let mut sent = 0;
    for _ in 0..60 {
        let out = client.frame(&mut sim, &inp);
        sent += out.send_input;
        if out.send_input != 0 {
            sim.input(0, out.input_general, out.input);
        }
        let f = sim.frame + 1;
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    assert!(sent > 0);
    assert_eq!(client.view, ViewMode::Fps);
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 1) {
        o.flags |= objflags::DEAD;
    }
    let f = sim.frame + 1;
    sim.step(&host, f);
    inp.logic_frame = f;
    // Defeated: the engine now says he only watches (his seat stays the local player's).
    inp.spectating = 1;
    let out = client.frame(&mut sim, &inp);
    assert_silent(&out);
    assert!(client.hud.death_banner > 0.0, "his own GENERAL DOWN");
    assert!(matches!(client.view, ViewMode::Leaving { .. }), "his death's fly-out");
    let out = settle(&mut client, &mut sim, &host, &mut inp, 120);
    assert_eq!((client.view, client.viewed, out.hide_object), (ViewMode::Fps, 1, 2), "then he follows the other General");
}

/// The seat the engine observes (Zero Hour's observer list) is adopted when it changes: followed
/// when he is in first person.
#[test]
fn the_engines_observed_seat_is_followed() {
    let (host, mut sim) = match_of_two();
    let mut client = Client::default();
    let mut inp = spectator_in();
    settle(&mut client, &mut sim, &host, &mut inp, 60);
    assert_eq!(client.viewed, 0);
    inp.local_player = 1;
    settle(&mut client, &mut sim, &host, &mut inp, 2);
    assert_eq!(client.viewed, 1);
}

#[test]
fn cycling_wraps_and_survives_a_missing_seat() {
    assert_eq!(cycle(&[0, 2, 4], Some(2), 1), Some(4));
    assert_eq!(cycle(&[0, 2, 4], Some(4), 1), Some(0));
    assert_eq!(cycle(&[0, 2, 4], Some(0), -1), Some(4));
    assert_eq!(cycle(&[0, 2, 4], Some(3), -1), Some(2), "from a seat that left");
    assert_eq!(cycle(&[0, 2, 4], None, -1), Some(4));
    assert_eq!(cycle(&[], Some(1), 1), None);
}
