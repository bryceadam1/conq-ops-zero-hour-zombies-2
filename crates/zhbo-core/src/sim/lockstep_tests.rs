//! The multiplayer determinism gate: the peers of one lockstep match, each with its own engine
//! world (a [`FakeHost`]), its own [`Sim`] and its own [`Client`] for its own local player, fed
//! the same replicated command stream with Zero Hour's run-ahead. Each player's machine renders at
//! its own cadence and does its own presentation (its client drains `Sim::events`, plays the
//! tracers for its own viewer, keeps its own camera, look and HUD). Every logic frame the two sims
//! must agree on [`Sim::state_hash`] and on everything they told the engine.
//!
//! The match: two human players, each with a General, against each other and a squad of enemy
//! soldiers, over rounds that cycle every slot weapon of the catalog through a custom class sent
//! as its wire words (the profile is the sender's alone: only the words travel), with the presets
//! picked by index, fire, aim, reload, sprint, both offhands, melee, the alt mode, the weapon
//! swap, stances, a mid-life class change after combat (the class waits for the next spawn), the
//! field computer and the overhead General's RTS report, and a death and a fresh General every
//! round (the chosen class comes in with him).
use super::tests::{general, soldier, FakeHost};
use super::*;
use crate::classes::CustomClass;
use crate::client::{Bindings, Client, ClientIn};
use crate::input::{Key, KeySet};
use std::collections::BTreeMap;

/// Zero Hour's minimum run-ahead (`MIN_RUNAHEAD`): a command sent on logic frame `f` executes
/// on frame `f + RUNAHEAD` on every peer.
const RUNAHEAD: u32 = 4;
/// Logic frames per round.
const ROUND: u32 = 420;

/// One replicated `GameMessage` the mod sends, as the logic dispatcher hands it to the core.
#[derive(Clone, Debug)]
enum Cmd {
    TakeControl(u32),
    Input(u32, FpsInput),
    /// `MSG_ZHBO_CHOOSE_CLASS`: the list index (a preset), or a custom class's wire words.
    ChooseClass(u32, usize, Vec<u32>),
}

/// Zero Hour's lockstep command lists: per execution frame, the commands in a fixed order (by
/// player, then the order each player sent them), the same on every peer.
#[derive(Default)]
struct Net {
    frames: BTreeMap<u32, Vec<(i32, u32, Cmd)>>,
    sent: u32,
}

impl Net {
    fn send(&mut self, now: u32, player: i32, cmd: Cmd) {
        self.sent += 1;
        let list = self.frames.entry(now + RUNAHEAD).or_default();
        list.push((player, self.sent, cmd));
        list.sort_by_key(|(p, n, _)| (*p, *n));
    }

    fn take(&mut self, frame: u32) -> Vec<(i32, Cmd)> {
        self.frames.remove(&frame).unwrap_or_default().into_iter().map(|(p, _, c)| (p, c)).collect()
    }
}

/// One machine of the match.
struct Peer {
    host: FakeHost,
    sim: Sim,
    client: Client,
    /// This machine's player (`ThePlayerList->getLocalPlayer()`).
    local: i32,
    /// This machine's own render cadence (a xorshift of its own).
    seed: u64,
    renders: u32,
}

impl Peer {
    fn new(local: i32, seed: u64) -> Self {
        let host = FakeHost::default();
        // The enemy squad (an RTS player's soldiers) between and around the two Generals.
        for (i, x) in [60.0, 90.0, 120.0, 150.0].into_iter().enumerate() {
            host.add(soldier(500 + i as u32, 2, x));
        }
        Peer { host, sim: Sim::new(), client: Client::default(), local, seed, renders: 0 }
    }

    fn next(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    /// The logic frame: the frame's commands in the engine's dispatch order, the overhead
    /// Generals' RTS report (the bridge's `rtsWeapons`, before the step), the step, and this
    /// machine's tracers (`zhbo_logic_step` hands them to the viewer right after the step).
    fn logic(&mut self, frame: u32, cmds: &[(i32, Cmd)], rts_shots: [u32; 3]) {
        for (player, cmd) in cmds {
            match cmd {
                Cmd::TakeControl(g) => self.sim.take_control(&self.host, *player, *g),
                Cmd::Input(g, input) => self.sim.input(*player, *g, *input),
                Cmd::ChooseClass(g, index, words) => self.sim.choose_class(*player, *g, *index, words),
            }
        }
        let generals: Vec<(i32, u32)> = self.host.objects.borrow().iter().filter(|o| o.id < 500 && !o.has(objflags::DEAD)).map(|o| (o.owner_player, o.id)).collect();
        for (player, g) in generals {
            self.sim.rts_update(&self.host, player, g, 0, rts_shots, None);
        }
        self.sim.step(&self.host, frame);
        self.client.tracers.play(&self.host, &mut self.sim.tracers);
    }

    /// This machine's rendered frames for one logic frame (`frames` of them; 0 = it rendered
    /// none). The local player's client turns `keys`/`look` into its input and the bridge sends
    /// it; everything else the client does is this machine's own presentation.
    fn render(&mut self, net: &mut Net, frame: u32, frames: u32, keys: KeySet, look: (f32, f32)) {
        for _ in 0..frames {
            let inp = ClientIn {
                dt: LOGIC_DT / frames as f32,
                local_player: self.local,
                logic_frame: frame,
                keys,
                mouse_dx: look.0 / frames as f32,
                mouse_dy: look.1 / frames as f32,
                input_allowed: 1,
                screen_w: 1280.0,
                screen_h: 720.0,
                ..Default::default()
            };
            let out = self.client.frame(&mut self.sim, &inp);
            if out.send_input != 0 {
                net.send(frame, self.local, Cmd::Input(out.input_general, out.input));
            }
            self.renders += 1;
        }
    }
}

/// What a player does on frame `t` of a round: the keys held and the mouse moved.
fn script(t: u32, rng: u64) -> (KeySet, (f32, f32)) {
    let b = Bindings::default();
    let mut keys = KeySet::default();
    let mut hold = |k: Key| keys.set_down(k, true);
    // Small aim corrections, both ways.
    let jitter = |shift: u32| ((rng >> shift) % 9) as f32 - 4.0;
    let look = (jitter(3), jitter(11) * 0.5);
    match t {
        // Fire aimed bursts, then from the hip.
        120..=169 => {
            hold(b.ads);
            hold(b.fire);
        }
        170..=189 => hold(b.fire),
        190..=199 => hold(b.reload),
        200..=219 => {
            hold(b.forward);
            hold(b.sprint);
        }
        // Cook the lethal, let go; then the tactical.
        222..=245 => hold(b.lethal),
        252..=275 => hold(b.tactical),
        // Knife taps.
        280..=300 if t.is_multiple_of(7) => hold(b.melee),
        // Stances and a jump, strafing.
        301..=306 => {
            hold(b.crouch);
            hold(b.left);
        }
        307..=309 => hold(b.jump),
        310..=313 => {
            hold(b.prone);
            hold(b.right);
        }
        // The alt mode in, a press to fire it, and back out.
        318 => hold(b.alt_mode),
        322..=330 => hold(b.fire),
        335 => hold(b.alt_mode),
        // The swap to the second weapon and a few shots with it.
        340 => hold(b.swap),
        348..=362 => hold(b.fire),
        // The field computer: back overhead.
        390 => hold(b.field_computer),
        _ => {}
    }
    (keys, look)
}

/// Round `r`'s custom class for a player: the catalog's slot weapons in turn as the primary, with
/// an offhand pair and perks that cycle too.
fn class_for(r: u32, player: i32) -> CustomClass {
    let cat = crate::weapons::catalog();
    let slot: Vec<&str> = cat.all().iter().filter(|d| d.class.is_slot_weapon() && d.id.ends_with("_mp")).map(|d| d.id.as_str()).collect();
    let lethals: Vec<&str> = cat.all().iter().filter(|d| d.class == Class::Lethal).map(|d| d.id.as_str()).collect();
    let tacticals: Vec<&str> = cat.all().iter().filter(|d| d.class == Class::Tactical).map(|d| d.id.as_str()).collect();
    let k = (r as usize) * 2 + player as usize;
    let pick = |list: &[&str], i: usize| list.get(i % list.len().max(1)).copied().unwrap_or_default().to_string();
    CustomClass {
        name: format!("Round {r} P{player}"),
        primary: pick(&slot, k),
        secondary: pick(&slot, k + 7),
        lethal: pick(&lethals, k),
        tactical: pick(&tacticals, k),
        lethal_count: 1,
        tactical_count: 1,
        melee: "knife_held_mp".into(),
        perks: [["Lightweight", "FastHands", "Dexterity"], ["Scavenger", "Toughness", "Hardline"], ["Ghost", "FlakJacket", "Engineer"]][k % 3].iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// How many rounds cover every slot weapon of the catalog as a primary (two a round).
fn rounds() -> u32 {
    let n = crate::weapons::catalog().all().iter().filter(|d| d.class.is_slot_weapon() && d.id.ends_with("_mp")).count() as u32;
    n.div_ceil(2).max(4)
}

/// The match on three machines: A (player 0's), B (player 1's) and C, which never renders.
/// Returns how many frames ran and A's and B's render counts.
fn play_match() -> (u32, u32, u32) {
    let mut a = Peer::new(0, 0x0A11_CE5E_ED00_0001);
    let mut b = Peer::new(1, 0x0B0B_5EED_0000_0002);
    // An observer seat with no General of its own.
    let mut c = Peer::new(3, 0x0C0C_5EED_0000_0003);
    let mut net = Net::default();
    // The players' own hands: one script stream per player (each machine only ever reads its
    // own player's).
    let mut hand = [0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888u64];
    let mut next = |p: usize| {
        let s = &mut hand[p];
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    };
    let total = rounds() * ROUND;
    let mut frame = 0;
    while frame < total {
        frame += 1;
        let (r, t) = ((frame - 1) / ROUND, (frame - 1) % ROUND);
        let ids = [100 + 2 * r, 101 + 2 * r];
        // The engine's own logic, the same on every machine: a fresh General per player each
        // round (facing each other across the squad), and the round's end kills both.
        for peer in [&mut a, &mut b, &mut c] {
            if t == 0 {
                let mut g0 = general(ids[0], 0);
                g0.pos = Vec3::new(0.0, 10.0, 0.0);
                let mut g1 = general(ids[1], 1);
                g1.pos = Vec3::new(200.0, -10.0, 0.0);
                g1.yaw = std::f32::consts::PI;
                peer.host.add(g0);
                peer.host.add(g1);
            }
            if t == ROUND - 2 {
                for o in peer.host.objects.borrow_mut().iter_mut().filter(|o| ids.contains(&o.id)) {
                    o.flags |= objflags::DEAD;
                }
            }
        }
        // The picks and take-controls each player's own UI sends for him.
        for (p, &g) in ids.iter().enumerate() {
            let player = p as i32;
            match t {
                // Round 0 by preset index from the overhead command bar, then customs.
                1 if r == 0 => net.send(frame, player, Cmd::ChooseClass(g, p, vec![])),
                1 => net.send(frame, player, Cmd::ChooseClass(g, 0, class_for(r, player).to_wire_words())),
                3 => net.send(frame, player, Cmd::TakeControl(g)),
                // Mid-life after the fight: the class waits for the next spawn (or, in the grace
                // period of round 0, before any combat, it lands at once).
                365 => net.send(frame, player, Cmd::ChooseClass(g, 0, class_for(r + 1, player).to_wire_words())),
                _ => {}
            }
        }
        // Overhead, the General's Zero Hour weapon fires now and then (the RTS report's shots).
        let rts_shots = if t > 395 && t.is_multiple_of(5) { [1, 0, 0] } else { [0; 3] };
        let cmds = net.take(frame);
        for peer in [&mut a, &mut b, &mut c] {
            peer.logic(frame, &cmds, rts_shots);
        }

        let h = a.sim.state_hash();
        for (name, peer) in [("B", &b), ("C", &c)] {
            let at = format!("machine {name} diverged from A at frame {frame} (round {r}, t {t})");
            assert_eq!(peer.sim.state_hash(), h, "{at}: the sim state");
            assert_eq!(peer.host.objects, a.host.objects, "{at}: the engine objects");
            assert_eq!(peer.host.damage_log, a.host.damage_log, "{at}: damage");
            assert_eq!(peer.host.controlled, a.host.controlled, "{at}: control");
            assert_eq!(*peer.host.spawned.borrow(), *a.host.spawned.borrow(), "{at}: spawns");
            assert_eq!(*peer.host.destroyed.borrow(), *a.host.destroyed.borrow(), "{at}: destroys");
            assert_eq!(*peer.host.disabled.borrow(), *a.host.disabled.borrow(), "{at}: disables");
        }

        // Each player's machine renders its own frames: A at a ragged 1-3 per logic frame, B at
        // one; C never.
        let (ka, la) = script(t, next(0));
        let (kb, lb) = script(t, next(1));
        let fa = 1 + (a.next() % 3) as u32;
        a.render(&mut net, frame, fa, ka, la);
        b.render(&mut net, frame, 1, kb, lb);
    }
    assert_eq!(c.sim.events.len(), MAX_EVENTS, "the machine that never renders sat at the event cap");
    (frame, a.renders, b.renders)
}

/// The gate: every weapon, every feature, three machines, one result.
#[test]
fn every_machine_agrees_on_every_frame_of_a_long_match() {
    let (frames, ra, rb) = play_match();
    assert!(frames >= 4 * ROUND);
    assert!(ra > rb, "the machines rendered at different cadences ({ra} vs {rb})");
}

/// The match really fought: the gate is only worth something if the scripted rounds reach the
/// systems it claims to cover (checked on a single peer that never drains its events).
#[test]
fn the_lockstep_match_exercises_the_sim() {
    let mut a = Peer::new(0, 1);
    let mut net = Net::default();
    let mut seen: Vec<EventKind> = Vec::new();
    for frame in 1..=2 * ROUND {
        let (r, t) = ((frame - 1) / ROUND, (frame - 1) % ROUND);
        let g = 100 + 2 * r;
        if t == 0 {
            a.host.add(general(g, 0));
        }
        match t {
            1 => net.send(frame, 0, Cmd::ChooseClass(g, 0, class_for(r, 0).to_wire_words())),
            3 => net.send(frame, 0, Cmd::TakeControl(g)),
            365 => net.send(frame, 0, Cmd::ChooseClass(g, 0, class_for(r + 1, 0).to_wire_words())),
            _ => {}
        }
        if t == ROUND - 2 {
            for o in a.host.objects.borrow_mut().iter_mut().filter(|o| o.id == g) {
                o.flags |= objflags::DEAD;
            }
        }
        let cmds = net.take(frame);
        a.logic(frame, &cmds, [0; 3]);
        seen.extend(a.sim.events.drain(..).map(|e| e.kind));
        let (keys, look) = script(t, frame as u64 * 0x9E37_79B9);
        // Render without the client draining the events (they were taken above).
        a.render(&mut net, frame, 1, keys, look);
    }
    let has = |f: &dyn Fn(&EventKind) -> bool| seen.iter().any(f);
    assert!(has(&|k| matches!(k, EventKind::ClassChosen { .. })), "a class landed");
    assert!(has(&|k| matches!(k, EventKind::EnteredFps { .. })), "took control");
    assert!(has(&|k| matches!(k, EventKind::Shot { .. })), "fired");
    assert!(has(&|k| matches!(k, EventKind::ReloadStarted)), "reloaded");
    assert!(has(&|k| matches!(k, EventKind::Thrown { .. }) || matches!(k, EventKind::Planted { .. })), "used an offhand");
    assert!(has(&|k| matches!(k, EventKind::Melee)), "swung the knife");
    assert!(has(&|k| matches!(k, EventKind::ClassAtNextSpawn { .. })), "a mid-life pick after combat waited for the next spawn");
    assert!(has(&|k| matches!(k, EventKind::GeneralDied { .. })), "a General died and the next came in");
    assert!(has(&|k| matches!(k, EventKind::FieldComputerOpened)), "opened the field computer");
}

/// The bug the gate caught: the step decided `hasdonecombat` from an index window into
/// [`Sim::events`], which the client drains per render frame and the cap trims from the front, so
/// a machine that rendered nothing for a while (a minimised window, a headless replay check) lost
/// the step's own combat events and let a mid-life class change through that every other machine
/// held for the next spawn. The flag now comes from the emit itself.
#[test]
fn combat_is_seen_even_when_the_event_buffer_is_full() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(soldier(2, 1, 60.0));
    let mut sim = Sim::new();
    super::tests::spawn(&mut sim, &host, 0, 1);
    // A full buffer nobody drained.
    for _ in 0..MAX_EVENTS {
        sim.emit(0, EventKind::Swapped);
    }
    let p = sim.players[0].as_mut().unwrap();
    p.arsenal = Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
    p.arsenal.raise_left = 0.0;
    let mut fired = false;
    for f in 1..=30 {
        sim.input(0, 1, FpsInput { held: buttons::FIRE, pressed: if f == 1 { buttons::FIRE } else { 0 }, ..Default::default() });
        sim.step(&host, f);
        fired |= sim.events.iter().any(|e| matches!(e.kind, EventKind::Shot { .. }));
    }
    assert!(fired, "the General fired");
    assert_eq!(sim.events.len(), MAX_EVENTS, "the buffer stayed at its cap");
    assert!(sim.player(0).unwrap().combat, "the shot counted as combat with the buffer full");
}
