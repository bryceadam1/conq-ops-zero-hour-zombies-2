//! Fake-host tests for the hip-fire cone end to end (`weapons::aim_spread`): the rounds the sim
//! traces leave the eye inside BO2's cone (`Bullet_Fire`) at the scale BO2's rules give for the
//! player's state, and the crosshair is drawn at the spread the next shot flies with.

use super::tests::{general, run, spawn, wait_until_ready, FakeHost};
use super::*;
use crate::input::{buttons, pack_move};
use crate::weapons::{FireMode, Kick, WeaponDef};

/// A plain one-pellet auto rifle with BO2-shaped spread numbers and no kick (the aim stays put).
fn rifle() -> &'static WeaponDef {
    let base = crate::weapons::weapon("an94_mp");
    Box::leak(Box::new(WeaponDef {
        id: "spread_test_mp".into(),
        hip_spread: [(3.0, 6.75), (2.5, 5.75), (2.0, 5.0)],
        spread_decay: 4.0,
        spread_fire_add: 0.6,
        spread_move_add: 5.0,
        spread_stance_decay: (1.05, 1.1),
        ads_spread: 0.0,
        hip_kick: Kick::default(),
        ads_kick: Kick::default(),
        fire: FireMode::Auto,
        pellets: 1,
        clip: 200,
        ..base.clone()
    }))
}

fn armed(def: &'static WeaponDef) -> (FakeHost, Sim) {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    spawn(&mut sim, &host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let w = sim.players[0].as_mut().unwrap().arsenal.current_mut();
    (w.def, w.main, w.ammo) = (def, def, def.clip);
    (host, sim)
}

/// Each shot's offset from the aim: `tan(offset) / tan(cone)` for a cone of `cone_deg`.
fn shot_shares(sim: &Sim, cone_deg: f32) -> Vec<f32> {
    let p = sim.player(0).unwrap();
    let f = crate::math::dir_from_angles(p.fire_yaw, p.fire_pitch);
    sim.events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Shot { origin, end, .. } => Some((end - origin).normalized_or_zero()),
            _ => None,
        })
        .map(|d| {
            let c = d.dot(f).clamp(-1.0, 1.0);
            (1.0 - c * c).sqrt() / c / cone_deg.to_radians().tan()
        })
        .collect()
}

/// A first shot from rest flies in the stance's minimum cone with BO2's linear radius (half the
/// cone on average, not the two thirds an area-uniform draw gives); running opens it to the
/// maximum, no further.
#[test]
fn the_traced_rounds_fall_in_bo2s_cone_for_the_state() {
    let def = rifle();
    let aim = FpsInput { pitch: 0.1, ..Default::default() };
    let (host, mut sim) = armed(def);
    let mut first = Vec::new();
    for _ in 0..120 {
        // A tap, then rest until the scale is back to 0 (4 shares a second: 0.25 s).
        sim.events.clear();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, ..aim }, 1);
        first.extend(shot_shares(&sim, 3.0));
        run(&mut sim, &host, 0, 1, aim, 12);
        sim.players[0].as_mut().unwrap().arsenal.current_mut().ammo = def.clip;
    }
    assert_eq!(first.len(), 120, "one round a tap");
    assert!(first.iter().all(|r| *r <= 1.0 + 1e-3), "inside the 3° minimum");
    let mean = first.iter().sum::<f32>() / first.len() as f32;
    assert!((mean - 0.5).abs() < 0.08, "BO2's linear radius: mean share {mean}");

    // Running (a move key down past g_speed's threshold): 5 shares a second open it to the
    // 6.75° maximum within a few frames; nothing flies past it.
    let (host, mut sim) = armed(def);
    let run_in = FpsInput { move_packed: pack_move(1.0, 0.0), ..aim };
    run(&mut sim, &host, 0, 1, run_in, 20);
    assert_eq!(sim.player(0).unwrap().arsenal.aim_spread.scale, 255.0, "running opens the scale");
    let mut moving = Vec::new();
    for _ in 0..60 {
        sim.events.clear();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, ..run_in }, 1);
        moving.extend(shot_shares(&sim, 6.75));
        sim.players[0].as_mut().unwrap().arsenal.current_mut().ammo = def.clip;
    }
    assert!(moving.len() >= 20 && moving.iter().all(|r| *r <= 1.0 + 1e-3), "within the maximum: {moving:?}");
    let mean = moving.iter().sum::<f32>() / moving.len() as f32;
    assert!(mean > 0.3, "{mean}");
}

/// The cone blends by the eye height in BO2's inches (`ps->viewHeightCurrent`): crouched, the
/// crouched pair; lowering the stance opens the scale (`bg_ads`); a jump adds `jump_spreadAdd`.
#[test]
fn the_eye_height_picks_the_cone_and_the_moves_open_it() {
    let def = rifle();
    let (host, mut sim) = armed(def);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::CROUCH, ..Default::default() }, 2);
    assert_eq!(sim.player(0).unwrap().arsenal.aim_spread.scale, 255.0, "going down opens it");
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::CROUCH, ..Default::default() }, 40);
    let a = &sim.player(0).unwrap().arsenal;
    assert!((a.view_height() - 40.0).abs() < 1e-2, "crouched eye: {} in", a.view_height());
    assert_eq!(a.spread_cone(a.view_height()), (2.5, 5.75));
    assert_eq!(a.aim_spread.scale, 0.0, "rested");
    assert_eq!(a.spread_now(), 2.5);

    let (host, mut sim) = armed(def);
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::JUMP, ..Default::default() }, 1);
    let s = sim.player(0).unwrap().arsenal.aim_spread.scale;
    assert!((s - 64.0).abs() < 1e-3, "the jump's add: {s}");
    run(&mut sim, &host, 0, 1, FpsInput::default(), 2);
    assert!(sim.player(0).unwrap().arsenal.aim_spread.scale > 64.0, "the air grows it");
}

/// The crosshair's gap is the spread the next shot flies with, through a string of fire and its
/// rest.
#[test]
fn the_crosshair_shows_the_next_shots_spread() {
    let def = rifle();
    let (host, mut sim) = armed(def);
    for f in 0..40 {
        let held = if f < 20 { buttons::FIRE } else { 0 };
        let shown = sim.player(0).unwrap().arsenal.spread_now();
        sim.input(0, 1, FpsInput { held, ..Default::default() });
        let mut flown = None;
        // The shot's spread, straight from the arsenal's action on the same frame.
        let p = sim.players[0].as_mut().unwrap();
        let mut probe = p.arsenal.clone();
        let mut rng = p.rng;
        let mut out = Vec::new();
        probe.step(held, 0, false, 0, 0.0, &mut rng, &mut out);
        for a in out {
            if let crate::weapons::WeaponAction::Fire { spread_deg, pellet: 0, .. } = a {
                flown = Some(spread_deg);
            }
        }
        if let Some(s) = flown {
            assert!((s - shown).abs() < 1e-4, "frame {f}: shown {shown}, flew {s}");
        }
        let n = sim.frame + 1;
        sim.step(&host, n);
        sim.players[0].as_mut().unwrap().arsenal.current_mut().ammo = def.clip;
    }
}
