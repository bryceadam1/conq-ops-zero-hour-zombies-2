//! The multiplayer determinism harness: the peers of one lockstep match, each with its own engine
//! world (a [`FakeHost`]), its own [`Sim`] and its own [`Client`] for its own local player, fed
//! the same replicated command stream with Zero Hour's run-ahead. Each player's machine renders at
//! its own cadence and does its own presentation (its client drains `Sim::events`, plays the
//! tracers for its own viewer, keeps its own camera, look and HUD), one more machine never
//! renders, and a spectating observer machine (an observer seat, a replay viewer:
//! `ClientIn::spectating`) renders, follows the Generals in first person, switches and goes
//! overhead, and must never send a thing. Every logic frame every sim must agree on [`Sim::state_hash`] and on everything it told
//! the engine.
//!
//! The match: two to four human players, each with a General, against each other and a squad of
//! enemy soldiers, over rounds whose custom classes the caller's [`Picks`] send as wire words (the
//! profile is the sender's alone: only the words travel), with the presets picked by index in round
//! 0; fire, aim, reload, sprint, both offhands, melee, the alt mode, the weapon swap, stances, a
//! mid-life class change after combat (the class waits for the next spawn), the field computer and
//! the overhead General's RTS report, and a death and a fresh General every round (the chosen class
//! comes in with him).
//!
//! The gate on the built-in catalog is `sim/lockstep_tests.rs`; the `harness` feature exposes this
//! module so `zhbo-assets` can run it over a real install's whole roster ([`roster_classes`]).
use super::fakehost::{general, soldier, FakeHost};
use super::*;
use crate::classes::CustomClass;
use crate::client::{Bindings, Client, ClientIn};
use crate::input::{mouse_buttons, Key, KeySet};
use std::collections::{BTreeMap, BTreeSet};

/// Zero Hour's minimum run-ahead (`MIN_RUNAHEAD`): a command sent on logic frame `f` executes
/// on frame `f + RUNAHEAD` on every peer.
pub const RUNAHEAD: u32 = 4;
/// The run-ahead over the match, as Zero Hour adapts it (`Network::processRunAheadCommand`, one
/// value for every peer from the frame it is set): 30 as a game starts, then down to what the
/// latency needs and back up, so commands pile onto one frame (it shrank: the last input wins,
/// every press counts) or leave a frame without any (it grew: the last one holds). It changes once a
/// round, at [`RUNAHEAD_CHANGE`]: the players are in first person and sending input every frame,
/// and no beat of the script is pressed yet, so every change meets a stream of inputs while the
/// beats keep the room they are spaced for (the round's body stays at 12 or less: a 30-frame delay
/// late in a round would push the swap and the field computer into the round's end).
pub fn runahead_at(frame: u32) -> u32 {
    const BODY: [u32; 6] = [12, 4, 8, 4, 6, 12];
    let f = frame.saturating_sub(1);
    if f < RUNAHEAD_CHANGE {
        return 30;
    }
    let (r, t) = (f / ROUND, f % ROUND);
    let body = if t >= RUNAHEAD_CHANGE { r } else { r - 1 };
    BODY[body as usize % BODY.len()]
}
/// The round frame the run-ahead changes ([`runahead_at`]): after the take-control has landed at
/// any run-ahead, before the script's first press (frame 120).
pub const RUNAHEAD_CHANGE: u32 = 90;
/// Logic frames per round.
pub const ROUND: u32 = 660;
/// The round frame of the mid-life class pick (after the round's fighting: it waits for the next
/// spawn, or lands at once in round 0's grace period).
pub const MIDLIFE_PICK: u32 = 610;
/// The round frame the script opens the field computer (back overhead).
pub const FIELD_COMPUTER: u32 = 625;

/// One replicated `GameMessage` the mod sends, as the logic dispatcher hands it to the core.
#[derive(Clone, Debug)]
pub enum Cmd {
    TakeControl(u32),
    /// The input and the frame its machine showed as it was made (`ClientOut::input_seen_frame`).
    Input(u32, FpsInput, u32),
    /// `MSG_ZHBO_CHOOSE_CLASS`: the list index (a preset), or a custom class's wire words.
    ChooseClass(u32, usize, Vec<u32>),
}

/// Zero Hour's lockstep command lists: per execution frame, the commands in a fixed order (by
/// player, then the order each player sent them), the same on every peer.
pub struct Net {
    frames: BTreeMap<u32, Vec<(i32, u32, Cmd)>>,
    sent: u32,
    /// The run-ahead in force (Zero Hour's `m_runAhead`); a fixed one when `schedule` is off.
    pub runahead: u32,
    schedule: bool,
    /// `getExecutionFrame`'s floor: an execution frame never goes back (`m_lastExecutionFrame`).
    last_execution: u32,
}

impl Default for Net {
    fn default() -> Self {
        Net { frames: BTreeMap::new(), sent: 0, runahead: RUNAHEAD, schedule: false, last_execution: 0 }
    }
}

impl Net {
    /// A fixed run-ahead (the latency harness's).
    pub fn with_runahead(runahead: u32) -> Self {
        Net { runahead, ..Default::default() }
    }

    /// The run-ahead changing over the match ([`runahead_at`]), as in a network game.
    pub fn scheduled() -> Self {
        Net { schedule: true, ..Default::default() }
    }

    pub fn send(&mut self, now: u32, player: i32, cmd: Cmd) {
        self.sent += 1;
        let runahead = if self.schedule { runahead_at(now) } else { self.runahead };
        // `Network::getExecutionFrame`: now plus the run-ahead, never before the last one.
        let at = (now + runahead).max(self.last_execution);
        self.last_execution = at;
        let list = self.frames.entry(at).or_default();
        list.push((player, self.sent, cmd));
        list.sort_by_key(|(p, n, _)| (*p, *n));
    }

    pub fn take(&mut self, frame: u32) -> Vec<(i32, Cmd)> {
        self.frames.remove(&frame).unwrap_or_default().into_iter().map(|(p, _, c)| (p, c)).collect()
    }
}

/// One machine of the match.
pub struct Peer {
    pub host: FakeHost,
    pub sim: Sim,
    pub client: Client,
    /// This machine's player (`ThePlayerList->getLocalPlayer()`).
    pub local: i32,
    /// This machine's own render cadence (a xorshift of its own).
    seed: u64,
    pub renders: u32,
}

impl Peer {
    pub fn new(local: i32, seed: u64) -> Self {
        let host = FakeHost::default();
        // The enemy squad (an RTS player's soldiers) between and around the Generals.
        for (i, x) in [60.0, 90.0, 120.0, 150.0].into_iter().enumerate() {
            host.add(soldier(500 + i as u32, 2, x));
        }
        Peer { host, sim: Sim::new(), client: Client::default(), local, seed, renders: 0 }
    }

    /// A machine with an empty world (no squad): the two-instance script's test, whose Generals
    /// face each other across open ground as the bridge's walk leaves them.
    pub fn bare(local: i32, seed: u64) -> Self {
        Peer { host: FakeHost::default(), sim: Sim::new(), client: Client::default(), local, seed, renders: 0 }
    }

    pub fn cadence(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    /// The logic frame: the frame's commands in the engine's dispatch order, the overhead
    /// Generals' RTS report (the bridge's `rtsWeapons`, before the step), the step, and this
    /// machine's tracers (`zhbo_logic_step` hands them to the viewer right after the step).
    pub fn logic(&mut self, frame: u32, cmds: &[(i32, Cmd)], rts_shots: [u32; 3]) {
        for (player, cmd) in cmds {
            match cmd {
                Cmd::TakeControl(g) => self.sim.take_control(&self.host, *player, *g),
                Cmd::Input(g, input, seen) => self.sim.input_seen(*player, *g, *input, *seen),
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
    pub fn render(&mut self, net: &mut Net, frame: u32, frames: u32, keys: KeySet, look: (f32, f32)) {
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
            // The engine's logic at hand, as in the game: the client predicts its own player
            // (`client::predict`), which must leave the sim and the engine untouched.
            let out = self.client.frame_with(&mut self.sim, Some(&self.host), &inp);
            if out.send_input != 0 {
                net.send(frame, self.local, Cmd::Input(out.input_general, out.input, out.input_seen_frame));
            }
            self.renders += 1;
        }
    }

    /// A spectating machine's rendered frames (an observer seat, a replay's playback, a defeated
    /// player who stayed): `observed` is the seat the engine observes, `keys`/`buttons` the
    /// spectator's own switch and view presses. It follows first-person Generals and must never
    /// send a thing or predict — every `ClientOut` it makes is checked, and nothing reaches the
    /// network. The engine's logic is at hand, as for a player's machine, so the predictor's own
    /// guard is what keeps it off.
    pub fn spectate(&mut self, frame: u32, frames: u32, observed: i32, keys: KeySet, buttons: u32) -> bool {
        let mut followed = false;
        for _ in 0..frames {
            let inp = ClientIn {
                dt: LOGIC_DT / frames as f32,
                local_player: observed,
                logic_frame: frame,
                keys,
                mouse_dx: 25.0,
                mouse_buttons: buttons,
                input_allowed: 1,
                screen_w: 1280.0,
                screen_h: 720.0,
                class_button: 1,
                class_pick_force: 1,
                spectating: 1,
                ..Default::default()
            };
            let out = self.client.frame_with(&mut self.sim, Some(&self.host), &inp);
            let at = format!("the spectator at frame {frame}");
            assert_eq!(out.send_input, 0, "{at} sent input");
            assert_eq!(out.choose_class, 0, "{at} picked a class");
            assert_eq!(out.select_object, 0, "{at} selected an object");
            assert_eq!(out.capture_mouse, 0, "{at} captured the mouse");
            assert!(!self.client.predict.active(), "{at} predicted");
            followed |= self.client.is_following();
            self.renders += 1;
        }
        followed
    }
}

/// What a player does on frame `t` of a round: the keys held and the mouse moved.
pub fn script(t: u32, rng: u64) -> (KeySet, (f32, f32)) {
    let b = Bindings::default();
    let mut keys = KeySet::default();
    let mut hold = |k: Key| keys.set_down(k, true);
    // Small aim corrections, both ways.
    let jitter = |shift: u32| ((rng >> shift) % 9) as f32 - 4.0;
    let look = (jitter(3), jitter(11) * 0.5);
    // The beats are spaced for the install's slowest items (a frag is ready ~1.6 s after the press,
    // a launcher's first raise is 1.5 s, an LMG reloads in ~3 s), so each one lands on every class.
    match t {
        // Fire aimed bursts, then from the hip, and a reload that is over before anything else
        // can cancel it (the alt mode shares the magazine).
        120..=150 => {
            hold(b.ads);
            hold(b.fire);
        }
        151..=160 => hold(b.fire),
        165..=170 => hold(b.reload),
        260..=279 => {
            hold(b.forward);
            hold(b.sprint);
        }
        // Cook the lethal, let go; then, once its throw is done, the tactical.
        282..=305 => hold(b.lethal),
        350..=373 => hold(b.tactical),
        // Knife taps (the last swing is over by the alt press).
        380..=395 if t.is_multiple_of(7) => hold(b.melee),
        // The alt mode in (standing, the knife done), a burst once it is up (a real launcher's
        // raise is ~1 s), and back out.
        420 => hold(b.alt_mode),
        445..=460 => hold(b.fire),
        470 => hold(b.alt_mode),
        // Aimed, the hold-breath key: the Hybrid optic's ADS-only toggle (`dualoptic_*`, which the
        // alt-mode key refuses), and a few aimed shots in it (any other gun just holds its breath).
        490..=512 => {
            hold(b.ads);
            if t == 504 {
                hold(b.sprint);
            }
            if t >= 506 {
                hold(b.fire);
            }
        }
        // The swap to the second weapon, and shots with it once it is up: the trigger let go every
        // third frame, so a semi-automatic launcher or the shield, up only after its 1.5 s first
        // raise, still sees a fresh press.
        515 => hold(b.swap),
        545..=590 if !t.is_multiple_of(3) => hold(b.fire),
        // Stances and a jump, strafing.
        592..=597 => {
            hold(b.crouch);
            hold(b.left);
        }
        598..=600 => hold(b.jump),
        601..=604 => {
            hold(b.prone);
            hold(b.right);
        }
        // The field computer: back overhead.
        FIELD_COMPUTER => hold(b.field_computer),
        _ => {}
    }
    (keys, look)
}

/// The custom class words player `player`'s UI sends in round `r`: `pick` 0 is the round's opening
/// pick, 1 the mid-life one.
pub type Picks<'a> = &'a dyn Fn(u32, i32, u32) -> Vec<u32>;

/// What the engine's message can carry of a payload: [`crate::classwire::PAYLOAD_WORDS`] words
/// (the bridge's `payload[64]`).
pub fn capped(mut words: Vec<u32>) -> Vec<u32> {
    words.truncate(crate::classwire::PAYLOAD_WORDS);
    words
}

/// One match: `players` playing seats (2 to 4), each with its own machine and a General per round,
/// plus one machine that never renders (an observer's seat, or a headless replay check).
pub struct Match<'a> {
    pub players: usize,
    pub rounds: u32,
    pub picks: Picks<'a>,
}

/// What a match did, as the first player's machine saw it.
#[derive(Debug, Default)]
pub struct Report {
    pub frames: u32,
    /// Each machine's rendered frames (the players' in seat order, then the observer's: 0).
    pub renders: Vec<u32>,
    /// What the match reached, as labels: `fired:<weapon>` (a shot or a launch, with the weapon up
    /// then, its alt mode's id when the alt was up), `melee:<weapon>` (a melee with it up),
    /// `deployed:<shield>` (a deployed Assault Shield), `thrown:<item>` (the offhand),
    /// `projectile:<def>` (every projectile in flight, the spawned ones among them), `equipment:<def>` (every piece of placed equipment), `carried:<item>` (an
    /// offhand of a class a player wore), `died:<round frame>` (a General's death, by 30 frames),
    /// `shock:<name>` (a shellshock on a General), `worn:<weapon>` (a slot weapon of a class a
    /// player wore).
    pub seen: BTreeSet<String>,
}

/// A harness General's health (Zero Hour units): the whole round's fire from two others.
const GENERAL_HEALTH: f32 = 1.0e6;

/// The seats' Generals: two facing each other across the squad, a third and fourth facing in
/// from the flanks.
const SEATS: [(f32, f32, f32); 4] = [
    (0.0, 10.0, 0.0),
    (200.0, -10.0, std::f32::consts::PI),
    (100.0, 150.0, -std::f32::consts::FRAC_PI_2),
    (100.0, -150.0, std::f32::consts::FRAC_PI_2),
];

/// Play `m` on every machine, asserting every frame that they agree (see the module doc).
pub fn play(m: &Match) -> Report {
    assert!((2..=SEATS.len()).contains(&m.players), "{} players", m.players);
    let seeds = [0x0A11_CE5E_ED00_0001u64, 0x0B0B_5EED_0000_0002, 0x0D0D_5EED_0000_0004, 0x0E0E_5EED_0000_0005];
    let mut peers: Vec<Peer> = (0..m.players).map(|p| Peer::new(p as i32, seeds[p])).collect();
    // A seat with no General of its own, past the players'.
    let observer = m.players.max(3) as i32;
    peers.push(Peer::new(observer, 0x0C0C_5EED_0000_0003));
    // A spectating observer seat that renders and follows the Generals in first person.
    let mut spectator = Peer::new(observer + 1, 0x0F0F_5EED_0000_0006);
    let mut followed = 0;
    // The run-ahead changes over the match as Zero Hour adapts it, under the players' predicting
    // clients.
    let mut net = Net::scheduled();
    // The players' own hands: one script stream per player (each machine only ever reads its
    // own player's).
    let mut hand = [0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888, 0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0001];
    let mut next = |p: usize| {
        let s = &mut hand[p];
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    };
    let mut report = Report::default();
    let total = m.rounds * ROUND;
    let mut frame = 0;
    while frame < total {
        frame += 1;
        let (r, t) = ((frame - 1) / ROUND, (frame - 1) % ROUND);
        let ids: Vec<u32> = (0..m.players).map(|p| 100 + m.players as u32 * r + p as u32).collect();
        // The engine's own logic, the same on every machine: a fresh General per player each
        // round, and the round's end kills them all.
        for peer in peers.iter_mut().chain(std::iter::once(&mut spectator)) {
            if t == 0 {
                for (p, &id) in ids.iter().enumerate() {
                    let mut g = general(id, p as i32);
                    // Tough enough to live out the round's script under the others' fire: a
                    // General downed in the firefight would never reach the offhands, the alt mode
                    // or the swap (the damage still lands, and is hashed, every frame).
                    (g.health, g.max_health) = (GENERAL_HEALTH, GENERAL_HEALTH);
                    let (x, y, yaw) = SEATS[p];
                    g.pos = Vec3::new(x, y, 0.0);
                    g.yaw = yaw;
                    peer.host.add(g);
                }
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
                1 => net.send(frame, player, Cmd::ChooseClass(g, 0, capped((m.picks)(r, player, 0)))),
                3 => net.send(frame, player, Cmd::TakeControl(g)),
                // Mid-life after the fight: the class waits for the next spawn (or, in the grace
                // period of round 0, before any combat, it lands at once).
                MIDLIFE_PICK => net.send(frame, player, Cmd::ChooseClass(g, 0, capped((m.picks)(r, player, 1)))),
                _ => {}
            }
        }
        // Overhead, the General's Zero Hour weapon fires now and then (the RTS report's shots).
        let rts_shots = if t > FIELD_COMPUTER + 5 && t.is_multiple_of(5) { [1, 0, 0] } else { [0; 3] };
        let cmds = net.take(frame);
        for peer in peers.iter_mut().chain(std::iter::once(&mut spectator)) {
            peer.logic(frame, &cmds, rts_shots);
        }

        let (first, rest) = peers.split_first().expect("machines");
        // The fingerprint Zero Hour's CRC carries (`zhbo_logic_crc`): its hash and, on a failure,
        // the per-seat parts that name where the state went apart.
        let h = first.sim.fingerprint();
        // (Inactive only until the first picks, sent on frame 2, execute.)
        assert!(h.active || frame < 2 + runahead_at(2), "frame {frame}: the match holds sim state");
        for (i, peer) in rest.iter().chain(std::iter::once(&spectator)).enumerate() {
            let at = format!("machine {} (seat {}) diverged from seat 0's at frame {frame} (round {r}, t {t})", i + 1, peer.local);
            assert_eq!(peer.sim.fingerprint(), h, "{at}: the sim state");
            assert_eq!(peer.host.objects, first.host.objects, "{at}: the engine objects");
            assert_eq!(peer.host.damage_log, first.host.damage_log, "{at}: damage");
            assert_eq!(peer.host.controlled, first.host.controlled, "{at}: control");
            assert_eq!(*peer.host.spawned.borrow(), *first.host.spawned.borrow(), "{at}: spawns");
            assert_eq!(*peer.host.destroyed.borrow(), *first.host.destroyed.borrow(), "{at}: destroys");
            assert_eq!(*peer.host.disabled.borrow(), *first.host.disabled.borrow(), "{at}: disables");
        }
        observe(&first.sim, &mut report.seen);

        // Each player's machine renders its own frames: seat 0's at a ragged 1-3 per logic frame,
        // seat 1's at one, seats 2 and 3 two every other frame and none between; the observer's
        // never.
        for p in 0..m.players {
            let (keys, look) = script(t, next(p));
            let frames = match p {
                0 => 1 + (peers[0].cadence() % 3) as u32,
                1 => 1,
                _ => 2 * (frame % 2),
            };
            peers[p].render(&mut net, frame, frames, keys, look);
        }
        // The spectator: its own seat, then the engine observes seat 1 for a while; it switches
        // with the attack and aim buttons and goes overhead and back with the view key.
        let observed = if (ROUND / 2..ROUND).contains(&t) && r % 2 == 1 { 1 } else { spectator.local };
        let mut keys = KeySet::default();
        let mut buttons = 0;
        match t {
            150 => keys.set_down(Key::MouseRight, true),
            170 => keys.set_down(Key::MouseLeft, true),
            250 => keys.set_down(Bindings::default().field_computer, true),
            330 => buttons = mouse_buttons::MIDDLE,
            _ => {}
        }
        let frames = 1 + (spectator.cadence() % 2) as u32;
        followed += spectator.spectate(frame, frames, observed, keys, buttons) as u32;
    }
    assert!(m.rounds < 2 || followed > ROUND, "the spectator followed the Generals in first person ({followed} frames)");
    let observer = peers.last().expect("the observer");
    assert_eq!(observer.sim.events.len(), MAX_EVENTS, "the machine that never renders sat at the event cap");
    // The players' machines predicted their own players all match long, through every run-ahead
    // change, and still agreed with the one that predicts nothing.
    // (Seats 2 and 3 render on every other logic frame only, so they predict half as often.)
    for peer in &peers[..m.players] {
        let s = peer.client.predict.stats;
        let floor = if peer.local < 2 { frame / 2 } else { frame / 4 };
        assert!(s.acked > floor, "seat {}'s machine predicted ({s:?})", peer.local);
    }
    report.frames = frame;
    report.renders = peers.iter().map(|p| p.renders).collect();
    report
}

/// The coverage labels of one logic frame (see [`Report::seen`]): seat 0's machine drains its
/// events every frame it renders, so what it holds now is this step's.
fn observe(sim: &Sim, seen: &mut BTreeSet<String>) {
    for e in &sim.events {
        if let EventKind::GeneralDied { .. } = e.kind {
            seen.insert(format!("died:{}", (sim.frame - 1) % ROUND / 30 * 30));
        }
        let Some(p) = usize::try_from(e.player).ok().and_then(|i| sim.players.get(i)).and_then(|p| p.as_ref()) else { continue };
        let held = p.arsenal.current_weapon().id.as_str();
        let label = match e.kind {
            EventKind::Shot { .. } | EventKind::Launched => "fired",
            // A melee with the weapon up (the Assault Shield's attack is its bash).
            EventKind::Melee => {
                seen.insert(format!("melee:{}", p.arsenal.current().def.id));
                continue;
            }
            EventKind::Thrown { lethal } => {
                seen.insert(format!("thrown:{}", if lethal { &p.arsenal.lethal.def.id } else { &p.arsenal.tactical.def.id }));
                continue;
            }
            _ => continue,
        };
        seen.insert(format!("{label}:{held}"));
    }
    for (_, proj) in &sim.projectiles {
        if !seen.contains(&format!("projectile:{}", proj.def.id)) {
            seen.insert(format!("projectile:{}", proj.def.id));
        }
    }
    // The Assault Shield's trigger deploys it (the held weapon is the next one by then).
    for d in &sim.deployed {
        if !seen.contains(&format!("deployed:{}", d.def.id)) {
            seen.insert(format!("deployed:{}", d.def.id));
        }
    }
    for e in &sim.equipment.placed {
        if !seen.contains(&format!("equipment:{}", e.def.id)) {
            seen.insert(format!("equipment:{}", e.def.id));
        }
    }
    for p in sim.players.iter().flatten() {
        if let Some(a) = &p.shock.active {
            if !seen.contains(&format!("shock:{}", a.def.name)) {
                seen.insert(format!("shock:{}", a.def.name));
            }
        }
    }
    for c in sim.chosen.iter().flatten() {
        for id in [&c.loadout.primary, &c.loadout.secondary] {
            if !id.is_empty() && !seen.contains(&format!("worn:{id}")) {
                seen.insert(format!("worn:{id}"));
            }
        }
        for id in [&c.loadout.lethal, &c.loadout.tactical] {
            if !id.is_empty() && !seen.contains(&format!("carried:{id}")) {
                seen.insert(format!("carried:{id}"));
            }
        }
    }
}

/// Every item the class editor offers, as legal classes: each primary item with its offered
/// attachments packed into compatible pairs (a weapon with none, once bare), each secondary item
/// with each of its attachments alone (and once bare), every lethal and tactical item in turn,
/// and one perk of each tier in turn — a primary, two attachments, a secondary, one attachment,
/// two offhands and three perks: Pick 10 exactly, no bonus card. Each class is what the sim wears
/// (it is repaired the way the sim repairs a received one, which leaves a legal class alone). DLC
/// items stay in, locked or not on this machine: a peer who owns the pack sends them.
pub fn roster_classes() -> Vec<CustomClass> {
    use crate::classmenu::{items::given_weapon, legal, offer, offered_attachments};
    let o = offer();
    let sets = |items: &[&str], per: usize| -> Vec<String> {
        let mut out = Vec::new();
        for base in items {
            let mut left: Vec<&str> = offered_attachments(base);
            // A combination has one alt mode (the first of the weapon's that its attachments
            // name), so two alt-mode attachments never share a set: each one's alt is reached.
            let alts: Vec<&str> = crate::weapons::catalog().get(base).map_or(Vec::new(), |d| d.alt_modes.iter().map(|(a, _)| a.as_str()).collect());
            out.push(given_weapon(base, &[]));
            while !left.is_empty() {
                let mut set: Vec<&str> = Vec::new();
                left.retain(|a| {
                    let second_alt = alts.contains(a) && set.iter().any(|s| alts.contains(s));
                    if set.len() < per && !second_alt && legal::legal(base, &[set.as_slice(), &[*a]].concat()) {
                        set.push(a);
                        false
                    } else {
                        true
                    }
                });
                if set.is_empty() {
                    // Nothing left that any set can take (cannot happen: a lone attachment is
                    // always legal), stop rather than loop.
                    break;
                }
                out.push(given_weapon(base, &set));
            }
        }
        out
    };
    let primaries = sets(&o.primary, crate::classes::max_attachments());
    let secondaries = sets(&o.secondary, 1);
    let perks = |tier: u8| -> Vec<&'static str> { crate::weapons::perks::PERKS.iter().filter(|p| p.tier == tier).map(|p| p.id).collect() };
    let tiers = [perks(1), perks(2), perks(3)];
    let n = primaries.len().max(secondaries.len()).max(o.lethal.len()).max(o.tactical.len());
    (0..n)
        .map(|k| {
            let pick = |list: &[String]| list[k % list.len()].clone();
            let item = |list: &[&str]| list.get(k % list.len().max(1)).map_or(String::new(), |s| s.to_string());
            let mut c = CustomClass {
                name: format!("Roster {k}"),
                primary: pick(&primaries),
                secondary: pick(&secondaries),
                lethal: item(&o.lethal),
                tactical: item(&o.tactical),
                lethal_count: 1,
                tactical_count: 1,
                melee: crate::weapons::KNIFE_HELD.to_string(),
                perks: tiers.iter().map(|t| t[k % t.len()].to_string()).collect(),
                ..Default::default()
            };
            let _ = crate::classrepair::repair_received(&mut c);
            c
        })
        .collect()
}
