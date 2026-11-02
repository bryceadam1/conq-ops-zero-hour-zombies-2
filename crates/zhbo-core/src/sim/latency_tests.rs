//! The first-person latency harness: one machine of a lockstep match driven at an artificial
//! run-ahead (Zero Hour's: 1 in a skirmish, whose command list is read on the next logic frame; 4 its
//! minimum on a LAN; 8 and 12 online; 30 as a game starts), measuring how many logic frames pass
//! between a key going down and the view answering it — the camera moving, the muzzle flash and
//! the fire sound, the HUD's ammo, the hit marker, the zoom, the reload's clip — with the client's
//! prediction (`client::predict`) and without it, plus how often a prediction was wrong; and the
//! antilag's hit rate on a strafing General tracked exactly as the shooter saw him.
//!
//! `cargo test -p zhbo-core latency -- --nocapture` prints the table.
use super::harness::{Cmd, Net, Peer};
use super::fakehost::general;
use super::*;
use crate::client::{Bindings, ClientIn, ClientOut, ViewMode};
use crate::input::{Key, KeySet};

/// What one rendered frame showed.
struct Shown {
    out: ClientOut,
    hitmarker: f32,
    fire_sound: bool,
    /// The magazine the HUD counts (the record first person is drawn from).
    ammo: u16,
}

/// One machine (player 0's), at run-ahead `runahead`, predicting or not.
struct Rig {
    peer: Peer,
    net: Net,
    frame: u32,
    predict: bool,
    /// Rendered frames per logic frame.
    renders: u32,
    /// The rendered frame within the logic frame.
    sub: u32,
}

impl Rig {
    fn new(runahead: u32, predict: bool) -> Rig {
        let peer = Peer::new(0, 7);
        let g = general(100, 0);
        peer.host.add(g);
        // The squad soaks every shot: the measures fire at it again and again.
        for o in peer.host.objects.borrow_mut().iter_mut().filter(|o| o.id >= 500) {
            o.health = 1.0e9;
            o.max_health = 1.0e9;
        }
        let mut rig = Rig { peer, net: Net::with_runahead(runahead), frame: 0, predict, renders: 1, sub: 0 };
        // The class by preset index from the command bar, then first person.
        rig.net.send(0, 0, Cmd::ChooseClass(100, 0, vec![]));
        rig.net.send(1, 0, Cmd::TakeControl(100));
        // The camera flies in and the weapon comes up.
        rig.run(KeySet::default(), 240);
        assert!(matches!(rig.peer.client.view, ViewMode::Fps), "first person");
        rig
    }

    /// One rendered frame with `keys` down, the logic frame first when one is due (`renders`
    /// rendered frames per logic frame).
    fn tick(&mut self, keys: KeySet) -> Shown {
        if self.sub == 0 {
            self.frame += 1;
            let cmds = self.net.take(self.frame);
            self.peer.logic(self.frame, &cmds, [0; 3]);
        }
        self.sub = (self.sub + 1) % self.renders;
        let dt = LOGIC_DT / self.renders as f32;
        let inp = ClientIn { dt, local_player: 0, logic_frame: self.frame, keys, input_allowed: 1, ..Default::default() };
        let host: Option<&dyn LogicHost> = if self.predict { Some(&self.peer.host) } else { None };
        let out = self.peer.client.frame_with(&mut self.peer.sim, host, &inp);
        if out.send_input != 0 {
            self.net.send(self.frame, 0, Cmd::Input(out.input_general, out.input, out.input_seen_frame));
        }
        let fire = self.peer.sim.player(0).map(|p| crate::sounds::fire_for(p.arsenal.current().def));
        let fire_sound = self.peer.client.cues.queue.drain(..).any(|r| Some(r.event) == fire || r.event == crate::sounds::FIRE);
        let shown = self.peer.client.predict.view().or_else(|| self.peer.sim.player(0));
        let ammo = shown.map_or(0, |p| p.arsenal.current().ammo);
        Shown { out, hitmarker: self.peer.client.hud.hitmarker, fire_sound, ammo }
    }

    /// `frames` logic frames' worth of rendered frames.
    fn run(&mut self, keys: KeySet, frames: u32) {
        for _ in 0..frames * self.renders {
            self.tick(keys);
        }
    }

    /// Milliseconds from `keys` going down (held `hold` logic frames; pressed on a logic frame's
    /// first rendered frame, the worst case for an input sent once a logic frame is the next
    /// rendered frame's) until `answered` says the view showed it (0: the same rendered frame),
    /// or `None` within `limit` logic frames.
    fn latency(&mut self, keys: KeySet, hold: u32, limit: u32, answered: &mut dyn FnMut(&Shown) -> bool) -> Option<f32> {
        while self.sub != 0 {
            self.tick(KeySet::default());
        }
        let dt = LOGIC_DT / self.renders as f32;
        for i in 0..limit * self.renders {
            let shown = self.tick(if i < hold * self.renders { keys } else { KeySet::default() });
            if answered(&shown) {
                return Some(i as f32 * dt * 1000.0);
            }
        }
        None
    }
}

fn keys(list: &[Key]) -> KeySet {
    let mut k = KeySet::default();
    for key in list {
        k.set_down(*key, true);
    }
    k
}

/// The latencies of one run-ahead, predicting or not (logic frames; `None`: never within the
/// window), and the prediction's stats.
#[derive(Debug)]
struct Measured {
    camera: Option<f32>,
    flash: Option<f32>,
    fire_sound: Option<f32>,
    ammo: Option<f32>,
    hit_marker: Option<f32>,
    ads: Option<f32>,
    reload: Option<f32>,
    stats: crate::client::predict::Stats,
}

fn measure(runahead: u32, predict: bool, renders: u32) -> Measured {
    let b = Bindings::default();
    let mut rig = Rig::new(runahead, predict);
    rig.renders = renders;
    let limit = runahead + 12;
    // Aim at the squad's first soldier (60 units ahead on the x axis).
    let aim_yaw = 0.0f32;
    let aim_pitch = -(4.0f32 / 60.0).atan();
    rig.peer.client.yaw = aim_yaw;
    rig.peer.client.pitch = aim_pitch;
    rig.run(KeySet::default(), runahead + 4);

    let base = rig.tick(KeySet::default()).out;
    let fwd = keys(&[b.forward]);
    let camera = rig.latency(fwd, 6, limit, &mut |s| (s.out.eye - base.eye).length() > 1e-3);
    // Back where he aims from: walk back, settle.
    rig.run(keys(&[b.back]), 6);
    rig.run(KeySet::default(), runahead + 30);

    let flash = rig.latency(keys(&[b.fire]), 1, limit, &mut |s| s.out.viewmodel_fired != 0);
    rig.run(KeySet::default(), runahead + 40);
    let fire_sound = rig.latency(keys(&[b.fire]), 1, limit, &mut |s| s.fire_sound);
    rig.run(KeySet::default(), runahead + 40);
    let before = rig.tick(KeySet::default()).ammo;
    let ammo = rig.latency(keys(&[b.fire]), 1, limit, &mut |s| s.ammo < before);
    rig.run(KeySet::default(), runahead + 60);
    rig.peer.client.hud.hitmarker = 0.0;
    let hit_marker = rig.latency(keys(&[b.fire]), 1, limit, &mut |s| s.hitmarker > 0.0);
    rig.run(KeySet::default(), runahead + 40);

    let ads = rig.latency(keys(&[b.ads]), limit, limit, &mut |s| s.out.viewmodel_ads > 0.0);
    rig.run(KeySet::default(), runahead + 40);
    let idle = rig.tick(KeySet::default()).out.viewmodel;
    let reload = rig.latency(keys(&[b.reload]), 1, limit, &mut |s| s.out.viewmodel != idle);
    rig.run(KeySet::default(), 90);
    Measured { camera, flash, fire_sound, ammo, hit_marker, ads, reload, stats: rig.peer.client.predict.stats }
}

/// The table, and the bar: predicted, every cue answers within the rendered frame after its
/// input went out, at every run-ahead; unpredicted, it waits out the run-ahead. Rendered at 120 Hz
/// (four rendered frames a logic frame) and at 30 Hz (one).
#[test]
fn latency_with_and_without_prediction() {
    for renders in [4u32, 1] {
        eprintln!("{} Hz render, milliseconds from the key going down:", 30 * renders);
        eprintln!("run-ahead | predicted: camera flash sound ammo hit ads reload (mispredicted/acked) | unpredicted: camera flash sound ammo hit ads reload");
        let frame_ms = LOGIC_DT * 1000.0;
        for runahead in [1u32, 4, 8, 12, 30] {
            let p = measure(runahead, true, renders);
            let u = measure(runahead, false, renders);
            let f = |x: Option<f32>| x.map_or("-".to_string(), |v| format!("{v:.0}"));
            eprintln!(
                "{runahead:>9} | {:>6} {:>5} {:>5} {:>4} {:>3} {:>3} {:>6} ({}/{}) | {:>6} {:>5} {:>5} {:>4} {:>3} {:>3} {:>6}",
                f(p.camera), f(p.flash), f(p.fire_sound), f(p.ammo), f(p.hit_marker), f(p.ads), f(p.reload), p.stats.mispredicted, p.stats.acked,
                f(u.camera), f(u.flash), f(u.fire_sound), f(u.ammo), f(u.hit_marker), f(u.ads), f(u.reload),
            );
            // The camera is drawn at the render clock between the last two predicted frames, so it
            // moves on the rendered frame after the input went out; every cue is presented as it is
            // predicted, on the input's own frame.
            let bars = [("camera", p.camera, frame_ms / renders as f32 + 0.5), ("flash", p.flash, 0.5), ("fire sound", p.fire_sound, 0.5), ("ammo", p.ammo, 0.5), ("hit marker", p.hit_marker, 0.5), ("ads", p.ads, frame_ms / renders as f32 + 0.5), ("reload", p.reload, 0.5)];
            for (name, v, bar) in bars {
                assert!(v.is_some_and(|v| v <= bar), "{renders} renders, run-ahead {runahead}: predicted {name} answered in {v:?} ms (bar {bar:.0})");
            }
            for (name, v) in [("camera", u.camera), ("flash", u.flash), ("ads", u.ads)] {
                assert!(v.is_some_and(|v| v + 0.5 >= (runahead as f32 - 1.0) * frame_ms), "run-ahead {runahead}: unpredicted {name} answered in {v:?} ms, before the run-ahead");
            }
            assert!(p.stats.acked > 100, "the sim took the inputs in ({p:?})");
            assert_eq!(p.stats.mispredicted, 0, "run-ahead {runahead}: a still world is predicted exactly ({:?})", p.stats);
            assert_eq!(p.stats.late, 0, "run-ahead {runahead}: every sim weapon event was predicted ({:?})", p.stats);
        }
    }
}

/// Zero Hour moves the run-ahead while a game runs (it starts at 30 and adapts to the latency):
/// the frame it shrinks two inputs land on one frame, the frame it grows none does. The prediction
/// is wrong for those few and heals: the view snaps nothing, most inputs are predicted exactly.
#[test]
fn a_changing_runahead_mispredicts_a_little_and_recovers() {
    let b = Bindings::default();
    let mut rig = Rig::new(30, true);
    for round in 0..12u32 {
        rig.net.runahead = [30, 12, 4, 8, 4, 6][round as usize % 6];
        rig.run(keys(&[b.forward, b.right]), 20);
        rig.run(keys(&[b.fire]), 10);
        rig.run(keys(&[b.left]), 20);
    }
    let s = rig.peer.client.predict.stats;
    eprintln!("changing run-ahead: {s:?}");
    assert!(s.acked > 300);
    assert_eq!(s.snaps, 0, "nothing snapped");
}

/// The antilag: a General strafes across in front; the shooter's machine aims exactly at him as
/// it shows him every frame and holds the trigger. With the rewind (the shots carry the frame he
/// was seen at) nearly every bullet lands, at every run-ahead up to the cap; without it, they miss
/// behind him by the run-ahead's worth of his run.
#[test]
fn shots_land_where_the_shooter_saw_the_target() {
    eprintln!("run-ahead | hit rate with antilag | without");
    for runahead in [4u32, 8, 12] {
        let with = strafe_hit_rate(runahead, true);
        let without = strafe_hit_rate(runahead, false);
        eprintln!("{runahead:>9} | {:>20.2} | {:.2}", with, without);
        assert!(with >= 0.9, "run-ahead {runahead}: {with:.2} of the shots landed with the antilag");
        assert!(without < with - 0.3, "run-ahead {runahead}: without it {without:.2} (with {with:.2})");
    }
}

fn strafe_hit_rate(runahead: u32, antilag: bool) -> f32 {
    let b = Bindings::default();
    let mut rig = Rig::new(runahead, true);
    if !antilag {
        rig.peer.sim.antilag_frames = 0;
    }
    // The enemy General: player 1, 150 units ahead, facing the shooter, a lot of health.
    let mut enemy = general(300, 1);
    enemy.pos = Vec3::new(150.0, 0.0, 0.0);
    enemy.yaw = std::f32::consts::PI;
    enemy.health = 1.0e9;
    enemy.max_health = 1.0e9;
    rig.peer.host.add(enemy);
    // Out of the soldiers' way.
    rig.peer.host.objects.borrow_mut().retain(|o| !(500..600).contains(&o.id));
    rig.net.send(rig.frame, 1, Cmd::ChooseClass(300, 0, vec![]));
    rig.net.send(rig.frame + 1, 1, Cmd::TakeControl(300));
    rig.run(KeySet::default(), 120);
    let (mut shots, mut hits) = (0u32, 0u32);
    for t in 0..240u32 {
        // His machine: strafing right and left, his inputs arriving through the same run-ahead.
        let side = if (t / 24) % 2 == 0 { 1.0 } else { -1.0 };
        let input = FpsInput { move_packed: crate::input::pack_move(0.0, side), yaw: std::f32::consts::PI, ..Default::default() };
        let seen = rig.peer.sim.frame;
        rig.net.send(rig.frame, 1, Cmd::Input(300, input, seen));
        // Ours: aim at him exactly as this machine shows him (the sim's frame on screen).
        let me = rig.peer.client.view_eye;
        if let Some(him) = rig.peer.sim.player(1) {
            let chest = him.body.pos + Vec3::new(0.0, 0.0, 6.0);
            let d = chest - me;
            rig.peer.client.yaw = d.y.atan2(d.x);
            rig.peer.client.pitch = (d.z / d.length_xy().max(1e-3)).atan();
        }
        let before = rig.peer.host.damage_log.borrow().iter().filter(|(_, v, _)| *v == 300).count();
        let fired_before = rig.peer.sim.player(0).map(|p| p.arsenal.current().ammo);
        rig.tick(if t > 20 { keys(&[b.fire, b.ads]) } else { keys(&[b.ads]) });
        let after = rig.peer.host.damage_log.borrow().iter().filter(|(_, v, _)| *v == 300).count();
        let fired_after = rig.peer.sim.player(0).map(|p| p.arsenal.current().ammo);
        if let (Some(a), Some(b)) = (fired_before, fired_after) {
            if b < a {
                shots += (a - b) as u32;
            }
        }
        hits += (after - before) as u32;
        // Keep the magazine full: the rate is about aim, not reloads.
        if let Some(p) = rig.peer.sim.players[0].as_mut() {
            let w = p.arsenal.current_mut();
            if w.ammo < 3 {
                w.ammo = w.def.clip.max(1);
            }
        }
    }
    assert!(shots > 20, "fired ({shots})");
    hits as f32 / shots as f32
}

