//! The multiplayer determinism gate on the built-in catalog: the harness's match
//! ([`super::harness`]: the peers, the replicated command stream with Zero Hour's run-ahead, each
//! machine's own render cadence and presentation, a machine that never renders) over rounds that
//! cycle every slot weapon of the catalog through a custom class sent as its wire words. The
//! full-roster gate over a real install is `zhbo-assets`' (`the_install_roster_plays_in_lockstep`).
use super::harness::{capped, play, script, Cmd, Match, Net, Peer, MIDLIFE_PICK, ROUND};
use super::tests::{general, soldier, FakeHost};
use super::*;
use crate::classes::CustomClass;
use crate::client::ClientIn;

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

/// The catalog cycle's picks: round `r`'s class, then round `r + 1`'s mid-life.
fn catalog_picks(r: u32, player: i32, pick: u32) -> Vec<u32> {
    class_for(r + pick, player).to_wire_words()
}

/// The match on three machines: A (player 0's), B (player 1's) and C, which never renders, over
/// `rounds` rounds whose custom classes `picks` sends. Returns how many frames ran and A's and B's
/// render counts.
fn play_match(rounds: u32, picks: &dyn Fn(u32, i32, u32) -> Vec<u32>) -> (u32, u32, u32) {
    let r = play(&Match { players: 2, rounds, picks });
    // Each round's deaths (past the grace period) drop their weapons where the next round's
    // Generals stand: the drops, the walk-overs and the use key's swap ran on every machine.
    for label in ["dropped:", "picked:"] {
        assert!(r.seen.iter().any(|s| s.starts_with(label)), "{label} {:?}", r.seen);
    }
    (r.frames, r.renders[0], r.renders[1])
}

/// The gate: every weapon, every feature, three machines, one result.
#[test]
fn every_machine_agrees_on_every_frame_of_a_long_match() {
    let (frames, ra, rb) = play_match(rounds(), &catalog_picks);
    assert!(frames >= 4 * ROUND);
    assert!(ra > rb, "the machines rendered at different cadences ({ra} vs {rb})");
}

/// The roster's classes on the built-in catalog, three players and their machines: the shape of
/// `zhbo-assets`' full-roster gate (`the_install_roster_plays_in_lockstep`), kept short here (six
/// rounds) — every class the roster builds is legal, and the first eighteen are worn, fought with
/// and agreed on, frame by frame.
#[test]
fn the_roster_plays_in_lockstep_on_three_players_machines() {
    use crate::classmenu::{Draft, PICK_10};
    let classes = super::harness::roster_classes();
    let o = crate::classmenu::offer();
    for item in o.primary.iter().chain(&o.secondary).chain(&o.lethal).chain(&o.tactical) {
        let has = |c: &CustomClass| [&c.primary, &c.secondary].iter().any(|id| crate::classmenu::items::given_parts(id).0 == *item) || c.lethal == *item || c.tactical == *item;
        assert!(classes.iter().any(has), "the roster carries {item}");
    }
    for c in &classes {
        assert!(crate::classrepair::repair_received(&mut c.clone()).is_empty(), "a legal class: {c:?}");
        assert!(Draft::from_custom(c).spent() <= PICK_10);
    }
    let picks = |r: u32, p: i32, pick: u32| classes[((r + pick) as usize * 3 + p as usize) % classes.len()].to_wire_words();
    let report = play(&Match { players: 3, rounds: 6, picks: &picks });
    assert_eq!(report.renders.len(), 4);
    assert_eq!(report.renders[3], 0, "the observer never rendered");
    assert!(report.renders[2] > 0 && report.renders[2] < report.renders[0], "seat 2's machine skipped frames");
    for label in ["worn:", "fired:", "projectile:"] {
        assert!(report.seen.iter().any(|s| s.starts_with(label)), "{label} {:?}", report.seen);
    }
    assert!(report.seen.iter().any(|s| s.starts_with("thrown:") || s.starts_with("planted:")), "{:?}", report.seen);
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
            MIDLIFE_PICK => net.send(frame, 0, Cmd::ChooseClass(g, 0, class_for(r + 1, 0).to_wire_words())),
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

/// R5: at the event cap the buffer's length stops moving, so "the events past the old length"
/// was nothing and `ConquerOps.log` went quiet on a machine that rendered nothing (a minimised
/// window). The log reads a step's events from the emit count instead.
#[test]
fn a_full_event_buffer_still_tells_the_log_this_steps_events() {
    let mut sim = Sim::new();
    for _ in 0..MAX_EVENTS {
        sim.emit(0, EventKind::Swapped);
    }
    let before = sim.emitted;
    assert_eq!(sim.first_event_since(before), MAX_EVENTS, "nothing new yet");
    sim.emit(0, EventKind::Melee);
    sim.emit(0, EventKind::ReloadStarted);
    assert_eq!(sim.events.len(), MAX_EVENTS);
    let first = sim.first_event_since(before);
    let new: Vec<EventKind> = sim.events[first..].iter().map(|e| e.kind).collect();
    assert_eq!(new, vec![EventKind::Melee, EventKind::ReloadStarted]);
    // More new events than the buffer holds: all of it is new.
    let before = sim.emitted;
    for _ in 0..MAX_EVENTS + 5 {
        sim.emit(0, EventKind::Swapped);
    }
    assert_eq!(sim.first_event_since(before), 0);
}

/// A player who leaves a network game hands his assets to a living ally (Zero Hour's
/// `onSelfDestruct` with `transferAssets`), his first-person General among them. The leaver's seat
/// lets go of him at once (his last input would otherwise drive him forever), his equipment goes
/// with him (BO2's disconnect), and the ally can take him into first person as his own.
#[test]
fn a_general_handed_to_an_ally_leaves_the_leavers_seat() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    super::tests::spawn(&mut sim, &host, 0, 1);
    // He was running forward when his player left.
    let run = FpsInput { move_packed: crate::input::pack_move(1.0, 0.0), ..Default::default() };
    for f in 1..=10 {
        sim.input(0, 1, run);
        sim.step(&host, f);
    }
    assert_eq!(*host.controlled.borrow(), vec![1]);
    // He had a Claymore and his Tactical Insertion down; another player a Claymore of his own.
    let claymore: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { equipment_kind: crate::equipment::Kind::Claymore, ..Default::default() }));
    let ti: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { equipment_kind: crate::equipment::Kind::Insertion, ..Default::default() }));
    use crate::equipment::Placed;
    sim.equipment.placed.push(Placed::new(claymore, 0, 1, 77, Vec3::new(50.0, 0.0, 0.0), 0.0, None));
    sim.equipment.placed.push(Placed::new(ti, 0, 1, 78, Vec3::new(0.0, 50.0, 0.0), 0.0, None));
    sim.equipment.placed.push(Placed::new(claymore, 1, 9, 79, Vec3::new(-50.0, 0.0, 0.0), 0.0, None));
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 1) {
        o.owner_player = 1;
    }
    sim.step(&host, 11);
    assert!(sim.player(0).is_none(), "the leaver's seat let go of him");
    assert!(sim.equipment.placed.iter().all(|p| p.owner != 0), "the leaver's equipment went with him (BO2 deletes a disconnecting player's weapon objects)");
    assert!(host.destroyed.borrow().contains(&77) && host.destroyed.borrow().contains(&78), "their objects too: {:?}", host.destroyed.borrow());
    assert_eq!(sim.equipment.placed.len(), 1, "another player's stays");
    assert!(host.controlled.borrow().is_empty(), "his AI runs again");
    let at = host.get(1).pos;
    sim.step(&host, 12);
    assert_eq!(host.get(1).pos, at, "nothing drives him with the leaver's input");
    sim.take_control(&host, 1, 1);
    assert_eq!(sim.player(1).map(|p| p.general), Some(1), "the ally takes him into first person");
    assert_eq!(*host.controlled.borrow(), vec![1]);
}

/// A hostile or broken client's pick payloads (R1): random words, a tagged body of noise, an
/// illegal class (every rule broken at once), and a legal class with one field corrupted. `k`
/// seeds the choice, so every machine of a match replays the same stream.
pub(super) fn fuzz_payload(k: u64) -> Vec<u32> {
    let mut s = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let cat = crate::weapons::catalog();
    let ids: Vec<&str> = cat.all().iter().map(|d| d.id.as_str()).collect();
    let any = |n: u64| ids[(n % ids.len() as u64) as usize].to_string();
    match next() % 4 {
        0 => (0..next() % 80).map(|_| next() as u32).collect(),
        1 => {
            let len = (next() % 200) as u16;
            let mut bytes = vec![crate::classwire::TAG];
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend((0..len).map(|_| next() as u8));
            crate::classwire::pack(&bytes)
        }
        2 => {
            // Every rule at once: a secondary item and five attachments (some not offered) in the
            // primary, an offhand in the secondary, a tactical in the tactical's place and a gun
            // in the lethal's, nine of each grenade, six perks of one tier and a stranger, foreign
            // camos and a magazine of spare rounds.
            let c = CustomClass {
                name: "Hacked".into(),
                primary: format!("{}+gl+reflex+grip+acog+silencer", any(next())),
                secondary: any(next()),
                lethal: any(next()),
                tactical: any(next()),
                lethal_count: (next() % 256) as u8,
                tactical_count: (next() % 256) as u8,
                melee: any(next()),
                perks: ["Lightweight", "Lightweight", "Hardline", "Ghost", "FlakJacket", "Toughness", "NotAPerk"].iter().map(|p| p.to_string()).collect(),
                reserve: [u16::MAX, (next() % 65_536) as u16],
                camo: "camo_gold".into(),
                knife: "camo_nope".into(),
                secondary_camo: "camo_dragon".into(),
                ..Default::default()
            };
            c.to_wire_words()
        }
        _ => {
            let mut c = class_for(next() as u32 % 16, (next() % 2) as i32);
            match next() % 6 {
                0 => c.primary = any(next()),
                1 => c.secondary = format!("{}+{}", c.secondary, ["gl", "dw", "mk", "ft", "acog"][(next() % 5) as usize]),
                2 => c.lethal_count = 9,
                3 => c.perks.extend(["Ghost", "Engineer", "Dexterity", "Scavenger"].iter().map(|p| p.to_string())),
                4 => c.tactical = c.primary.clone(),
                _ => c.reserve = [9_999, 9_999],
            }
            c.to_wire_words()
        }
    }
}

/// R1 through the lockstep: every pick of a long match is a hostile payload. Every machine mends
/// (or drops) each the same way, so they still agree on every frame, and what the players wear is
/// legal.
#[test]
fn hostile_class_payloads_are_mended_alike_on_every_machine() {
    let picks = |r: u32, player: i32, pick: u32| fuzz_payload(((r as u64) << 8) | ((player as u64) << 1) | pick as u64);
    let (frames, _, _) = play_match(6, &picks);
    assert_eq!(frames, 6 * ROUND);
}

/// A DLC item and a DLC camo are the sender's to own (BO2's `Engine.HasDLCForItem` runs on the
/// owner's controller): the received pass reads neither the receiving machine's locks nor its camo
/// lists, so a peer without the pack wears the class exactly as one with it does, and the
/// picker's own pass is the one that refuses a locked item.
#[test]
fn a_received_class_ignores_the_receivers_dlc() {
    let mut c = class_for(0, 0);
    c.camo = "camo_dlc_pack_only".into();
    c.secondary_camo = "camo_dlc_pack_only".into();
    let mut received = c.clone();
    let cleared = crate::classrepair::repair_received(&mut received);
    assert!(!cleared.iter().any(|s| s.ends_with("camo")), "{cleared:?}");
    assert_eq!((received.camo.as_str(), received.secondary_camo.as_str()), ("camo_dlc_pack_only", "camo_dlc_pack_only"));
    let words = received.to_wire_words();
    let chosen = ChosenClass::decode(0, &words).unwrap();
    assert_eq!(chosen.camo, "camo_dlc_pack_only");
}

/// R1, the rule: whatever words arrive, the class the sim wears is the repaired one — legal under
/// every check the editor applies (Pick 10, the card cap, each slot's items and attachments, the
/// grenade counts, the perk tiers, the camos, the spare rounds) — or the pick is dropped.
#[test]
fn a_received_class_is_worn_legal_or_not_at_all() {
    use crate::classmenu::{Draft, PICK_10};
    let mut worn = 0;
    for k in 0..4000u64 {
        let words = capped(fuzz_payload(k));
        if words.is_empty() {
            // No words is the preset pick by index, not a custom class.
            continue;
        }
        let Some(chosen) = ChosenClass::decode(0, &words) else {
            assert!(CustomClass::from_wire_words(&words).is_none(), "only an undecodable payload is dropped");
            continue;
        };
        worn += 1;
        let mut c = CustomClass::from_wire_words(&words).unwrap();
        let _ = crate::classrepair::repair_received(&mut c);
        assert_eq!(chosen.loadout, c.loadout(), "the sim wears the repaired class (payload {k})");
        assert!(crate::classrepair::repair_received(&mut c.clone()).is_empty(), "the repaired class is legal: {c:?}");
        let d = Draft::from_custom(&c);
        assert!(d.spent() <= PICK_10 && d.wildcards.len() <= crate::classes::max_bonus_cards(), "{c:?}");
        for (id, primary) in [(&c.primary, true), (&c.secondary, false)] {
            if !id.is_empty() {
                let (base, _) = crate::classmenu::items::given_parts(id);
                assert!(crate::classmenu::items::fits_slot(&base, primary) || (!primary && crate::classmenu::items::is_primary_item(&base)), "{id} in the {} slot", if primary { "primary" } else { "secondary" });
            }
        }
        assert!(c.lethal_count as usize <= crate::classes::max_grenades() && c.tactical_count as usize <= crate::classes::max_grenades());
        for (i, id) in [&c.primary, &c.secondary].into_iter().enumerate() {
            if c.reserve[i] > 0 {
                assert!(c.reserve[i] <= crate::weapons::weapon(id).max_reserve, "{c:?}");
            }
        }
    }
    assert!(worn > 1000, "the fuzz reached the repair ({worn} classes worn)");
}

/// A sim with every part of the state in use: two seats from a real match (mid-fight: a reload,
/// the gun's pose, the RNG and the timers moving), then every optional piece of a player record
/// set, a projectile, equipment that is down, a smoke screen and a deployed shield.
fn populated_sim() -> Sim {
    // Legal classes (the roster's), so the receipt's repair keeps every slot of both.
    let roster = super::harness::roster_classes();
    let mut a = Peer::new(0, 1);
    let mut net = Net::default();
    for frame in 1..=200 {
        let t = frame - 1;
        if t == 0 {
            a.host.add(general(100, 0));
            // Seat 1 behind seat 0's fire (the squad is ahead, +x), so both records live on.
            let mut g1 = general(101, 1);
            g1.pos = Vec3::new(-200.0, 0.0, 0.0);
            g1.yaw = std::f32::consts::PI;
            a.host.add(g1);
        }
        match t {
            1 => {
                net.send(frame, 0, Cmd::ChooseClass(100, 0, roster[0].to_wire_words()));
                net.send(frame, 1, Cmd::ChooseClass(101, 0, roster[1].to_wire_words()));
            }
            3 => {
                net.send(frame, 0, Cmd::TakeControl(100));
                net.send(frame, 1, Cmd::TakeControl(101));
            }
            _ => {}
        }
        let cmds = net.take(frame);
        a.logic(frame, &cmds, [0; 3]);
        let (keys, look) = script(t, frame as u64 * 0x9E37_79B9);
        a.render(&mut net, frame, 1, keys, look);
    }
    let mut sim = a.sim;
    let cat = crate::weapons::catalog();
    let def = |id: &str| cat.get(id).unwrap_or_else(|| panic!("{id} in the table"));
    let p = sim.players[0].as_mut().expect("seat 0 in first person");
    p.mode = Mode::FieldComputer { t: 0.25 };
    p.hack = Some((500, 0.5));
    p.hammer = Some(crate::building::Hammer { choice: 1, picked: true, raise: 0.5, turn: 0.25, at: Vec3::new(1.0, 2.0, 3.0), angle: 0.5 });
    p.site = Some(crate::building::Site { id: 9, template: 3, kind: 1, percent: 40.0, name: [b'x'; 48] });
    p.charge = Some(fire::MeleeCharge { target: 501, left: 0.25 });
    p.optics = optics::Optics::filled_for_test();
    let shock: &'static crate::shellshock::ShockDef = Box::leak(Box::new(crate::shellshock::ShockDef { name: "concussion_grenade".into(), ..Default::default() }));
    p.shock.active = Some(crate::shellshock::Active { def: shock, duration: 2.0, left: 1.5, serial: 3 });
    p.shock.queued = Some((0.5, "flashbang", 1.0));
    p.body.sprint_started = Some(1.5);
    p.body.stance_lerp = Some((1.0, 0.5, 1.0, 0.25));
    p.arsenal.offhand = Some(crate::weapons::OffhandUse::Throwing { lethal: true, t: 0.25, fired: false });
    let hand = crate::weapons::Hand { ammo: 5, cooldown: 0.1, reload: Some(crate::weapons::Reload::Magazine { t: 0.5, length: 2.0, add_at: 1.0, added: false, empty: true, quick: false }), burst_left: 2, string: 3, recoil_acc: 0.5, since_shot: 1.0, rechamber_pending: true, rechamber_shots: 1, rechamber: Some(0.25), reload_owed: true };
    let w = &mut p.arsenal.weapons[0];
    w.alt_mode = Some(def("gl_an94_mp"));
    w.reload = Some(crate::weapons::Reload::Shells { phase: crate::weapons::ShellPhase::Insert, t: 0.5, stop: false });
    w.rechamber = Some(0.5);
    w.left = Some(hand);
    let frag = def("frag_grenade_mp");
    let flight = crate::projectiles::Flight { gravity: 1.0, bounce_up: 0.5, bounce_along: 0.5, rest_speed: 2.0 };
    sim.projectiles.push((
        1,
        crate::projectiles::Projectile { def: frag, owner: 101, pos: Vec3::new(1.0, 2.0, 3.0), vel: Vec3::new(4.0, 5.0, 6.0), fuse: 2.0, age: 0.5, object: 900, stuck_to: Some(501), resting: false, self_blast_scale: 1.0, self_stun_scale: 1.0, lands: true, travelled: 10.0, bounced: Some((Vec3::new(1.0, 1.0, 0.0), crate::impacts::Surface::Dirt)), struck: Some(true), last_struck: Some(502), flight },
    ));
    let mut placed = crate::equipment::Placed::new(def("claymore_mp"), 0, 100, 901, Vec3::new(5.0, 5.0, 0.0), 0.5, Some(503));
    placed.triggered = Some(0.25);
    sim.equipment.placed.push(placed);
    sim.smokes.push(crate::smoke::Smoke { at: Vec3::new(9.0, 9.0, 0.0), born: 7 });
    let shield = def("riotshield_mp");
    let bearer = crate::shield::Bearer::of(0, Vec3::new(3.0, 3.0, 0.0), 0.5, shield, true).expect("the shield's box");
    sim.deployed.push(deploy::Deployed { owner: 1, general: 101, def: shield, object: 902, bearer, damage_taken: 10.0, destroyed: Some(1.0), weapon: (1, crate::weapons::WeaponState::new(shield), true) });
    let options = crate::weapons::WeaponOptions { camo: "camo_gold".into(), reticle: "reticle_3".into(), reticle_color: "reticle_color_2".into() };
    let weapon = crate::weapons::Carried { def: def("an94_mp"), clip: 20, stock: 60, alt_clip: 1, alt_stock: 2, options: options.clone() };
    sim.drops.push(drops::Dropped { weapon, pos: Vec3::new(7.0, 8.0, 1.0), vel: Vec3::new(1.0, 2.0, -3.0), resting: false, yaw: 0.5, expires: Some(1800), owner: 100, owner_until: 40, object: 903 });
    sim.kills.push(drops::Kill { victim: 101, friendly: true });
    // A picked-up weapon wears its own options.
    sim.players[0].as_mut().unwrap().arsenal.weapons[1].options = Some(options);
    sim.score.attacked.push(score::Attacked { victim: 100, attackers: vec![score::Attacker { seat: 1, general: 101, damage: 30.0, tactical: Some(score::Tactical::Flash) }] });
    sim.score.assists[1] = 2;
    sim.score.generals[0] = [0, 0, 90, 100];
    sim
}

/// Change the `n`-th leaf of the hashed state (the parts [`Sim::fingerprint`] streams).
fn perturb_state(sim: &mut Sim, n: &mut usize) -> bool {
    use crate::statehash::StateHash;
    // A seat's class wait is hashed with the seat, so only the seats in use (a record or a class).
    let in_use: Vec<usize> = (0..MAX_PLAYERS).filter(|&i| sim.players[i].is_some() || sim.chosen[i].is_some()).collect();
    sim.frame.perturb(n) || sim.players.perturb(n) || sim.chosen.perturb(n) || in_use.into_iter().any(|i| sim.class_waits[i].perturb(n)) || sim.projectiles.perturb(n) || sim.equipment.perturb(n) || sim.smokes.perturb(n) || sim.deployed.perturb(n) || sim.drops.perturb(n) || sim.kills.perturb(n) || sim.score.perturb(n) || sim.history.perturb(n)
}

/// Sensitivity: every leaf of the state (every scalar, string and definition of every player
/// record, chosen class, projectile, piece of equipment, smoke and deployed shield) changes the
/// hash, and changes exactly the part that holds it: its own seat's, or the world's.
#[test]
fn every_field_of_the_state_changes_the_hash_and_its_own_part() {
    let sim = populated_sim();
    let base = sim.fingerprint();
    assert!(base.active);
    let mut leaves = 0;
    loop {
        let mut s = sim.clone();
        let mut n = leaves;
        if !perturb_state(&mut s, &mut n) {
            break;
        }
        let f = s.fingerprint();
        assert_ne!(f.hash, base.hash, "leaf {leaves} did not change the hash");
        let seats: Vec<usize> = (0..MAX_PLAYERS).filter(|&i| f.seats[i] != base.seats[i]).collect();
        let world = f.world != base.world;
        let frame = f.frame != base.frame;
        assert_eq!(seats.len() + world as usize + frame as usize, 1, "leaf {leaves} changed one part: seats {seats:?}, world {world}, frame {frame}");
        leaves += 1;
    }
    // Two full records, two classes and the world: a few hundred leaves, all reached.
    assert!(leaves > 500, "the walk reached the state ({leaves} leaves)");
}

/// What is not state stays out: the presentation buffers and the step's scratch copies.
#[test]
fn presentation_and_scratch_do_not_enter_the_hash() {
    let mut sim = populated_sim();
    let h = sim.state_hash();
    sim.emit(0, EventKind::Swapped);
    sim.events.clear();
    sim.emit(1, EventKind::Melee);
    let def = crate::weapons::catalog().get("an94_mp").unwrap();
    sim.tracers.push(ShotTracer { shooter: 100, def, target: 0, end: Vec3::new(1.0, 2.0, 3.0), left: false });
    sim.shields.clear();
    sim.equipment.shields.clear();
    sim.equipment.guards.clear();
    sim.combat_seen = 0b101;
    sim.emitted += 7;
    sim.antilag_hits.push((100, 101, 3));
    assert_eq!(sim.state_hash(), h);
}

/// Stability: the hash is a function of the values alone. An equal state built apart (a clone, a
/// definition at another address with the same id) hashes the same, and so does the same state
/// after a reset and the same steps.
#[test]
fn equal_states_hash_equal_wherever_they_live() {
    let sim = populated_sim();
    assert_eq!(sim.clone().fingerprint(), sim.fingerprint());
    let mut moved = sim.clone();
    let p = moved.players[0].as_mut().unwrap();
    let def = p.arsenal.weapons[0].def;
    p.arsenal.weapons[0].def = Box::leak(Box::new(def.clone()));
    assert!(!std::ptr::eq(moved.players[0].as_ref().unwrap().arsenal.weapons[0].def, def));
    assert_eq!(moved.state_hash(), sim.state_hash(), "a definition is hashed by its id, not its address");
    assert_eq!(populated_sim().fingerprint(), sim.fingerprint(), "the same match twice");
}

/// An idle sim adds nothing to Zero Hour's CRC (a game nobody played in first person CRCs as
/// stock Zero Hour); a class, a General in first person or anything in the world makes it active.
#[test]
fn only_a_sim_that_holds_state_joins_the_crc() {
    let mut sim = Sim::new();
    assert!(!sim.fingerprint().active);
    sim.frame = 300;
    assert!(!sim.fingerprint().active, "the frame alone is no state");
    sim.smokes.push(crate::smoke::Smoke { at: Vec3::ZERO, born: 0 });
    assert!(sim.fingerprint().active, "a smoke screen");
    let mut sim = Sim::new();
    let weapon = crate::weapons::Carried { def: crate::weapons::weapon("an94_mp"), clip: 1, stock: 0, alt_clip: 0, alt_stock: 0, options: Default::default() };
    sim.drops.push(drops::Dropped { weapon, pos: Vec3::ZERO, vel: Vec3::ZERO, resting: true, yaw: 0.0, expires: None, owner: 0, owner_until: 0, object: 0 });
    assert!(sim.fingerprint().active, "a dropped weapon");
    let mut sim = populated_sim();
    assert!(sim.fingerprint().active);
    sim.reset();
    assert!(!sim.fingerprint().active, "a new game");
    let host = FakeHost::default();
    host.add(general(1, 0));
    sim.choose_class(4, 1, 0, &[]);
    let f = sim.fingerprint();
    assert!(f.active && f.seats[4].is_some() && f.seats.iter().filter(|s| s.is_some()).count() == 1, "a class picked overhead: {f:?}");
}

/// A mod-only divergence, the kind Zero Hour's object CRC cannot see (a round of ammo, a class
/// change, a shellshock's clock, a cooldown), changes the hash on the next CRC frame, and the
/// per-seat parts name the seat it is in.
#[test]
fn a_mod_only_divergence_is_seen_and_names_its_seat() {
    let a = populated_sim();
    let base = a.fingerprint();
    type Edit = (&'static str, fn(&mut PlayerSim));
    let edits: [Edit; 4] = [
        ("ammo", |p| p.arsenal.weapons[1].ammo -= 1),
        ("class", |p| p.class = Some("custom:other".into())),
        ("shellshock", |p| p.shock.active.as_mut().unwrap().left -= 0.5),
        ("cooldown", |p| p.arsenal.weapons[0].cooldown += 0.1),
    ];
    for (what, edit) in edits {
        let mut b = a.clone();
        edit(b.players[0].as_mut().unwrap());
        let f = b.fingerprint();
        assert_ne!(f.hash, base.hash, "{what}");
        assert_ne!(f.seats[0], base.seats[0], "{what}: seat 0's part");
        assert_eq!((f.seats[1], f.world), (base.seats[1], base.world), "{what}: nothing else");
    }
}

/// The two-instance test's script (`client::Autopilot::mp_input`, `rounds` lives each) on three
/// machines, with the bridge's part played here: the take-controls, a dead General's
/// replacement, and in the long run a General back overhead taken again. Every frame's
/// fingerprint must agree. Returns C's events, the scripts' log lines and whether any General was
/// stunned (shellshocked, or the engine's disable).
#[cfg(feature = "autopilot")]
fn mp_autopilot_match(rounds: u32, frames: u32) -> (Vec<Event>, Vec<String>, bool) {
    use crate::client::Autopilot;
    let peer = |local: i32, seed: u64, shooter: Option<bool>| {
        let mut p = Peer::bare(local, seed);
        p.client.autopilot = shooter.map(|mp_shooter| Autopilot { mp: true, mp_shooter, mp_rounds: rounds, ..Default::default() });
        p
    };
    let mut a = peer(0, 0x0A11_CE5E_ED00_0001, Some(true));
    let mut b = peer(1, 0x0B0B_5EED_0000_0002, Some(false));
    let mut c = peer(3, 0x0C0C_5EED_0000_0003, None);
    let mut net = Net::default();
    // Standing at the meeting point, facing each other (the bridge's walk ends 120 apart).
    for p in [&mut a, &mut b, &mut c] {
        p.host.add(general(100, 0));
        let mut g = general(101, 1);
        g.pos = Vec3::new(120.0, 0.0, 0.0);
        g.yaw = std::f32::consts::PI;
        p.host.add(g);
    }
    let mut generals = [100u32, 101u32];
    let mut next_id = 102;
    let mut seen: Vec<Event> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    // The long run's bridge: every take-control counted, and one back overhead alive taken again
    // a second later while lives are left (the target's declined spawn is one more; the host
    // plays on).
    let limit = |i: usize| match (rounds > 1, i) {
        (false, _) => 0,
        (true, 1) => rounds as usize + 1,
        (true, _) => usize::MAX,
    };
    let mut takes = [0usize; 2];
    let mut overhead_since = [None::<u32>; 2];
    let mut shocked = false;
    for frame in 1..=frames {
        // The bridge: take control a little apart (each machine's walk ends on its own), and a
        // dead General's replacement from the Command Center a few seconds later.
        if frame == 10 {
            net.send(frame, 0, Cmd::TakeControl(generals[0]));
            takes[0] += 1;
        }
        if frame == 40 {
            net.send(frame, 1, Cmd::TakeControl(generals[1]));
            takes[1] += 1;
        }
        for (i, g) in generals.into_iter().enumerate() {
            let dead = a.host.objects.borrow().iter().any(|o| o.id == g && o.has(objflags::DEAD));
            if dead {
                let id = next_id;
                next_id += 1;
                generals[i] = id;
                for p in [&mut a, &mut b, &mut c] {
                    let mut fresh = general(id, i as i32);
                    fresh.pos = Vec3::new(if i == 0 { -60.0 } else { 180.0 }, 0.0, 0.0);
                    p.host.add(fresh);
                }
                net.send(frame + 90, i as i32, Cmd::TakeControl(id));
                takes[i] += 1;
                overhead_since[i] = None;
            } else if frame > 120 && takes[i] < limit(i) && a.sim.player(i as i32).is_some_and(|p| p.general == g && !p.is_first_person()) {
                let since = *overhead_since[i].get_or_insert(frame);
                if frame == since + 30 {
                    net.send(frame, i as i32, Cmd::TakeControl(g));
                    takes[i] += 1;
                    overhead_since[i] = None;
                }
            } else {
                overhead_since[i] = None;
            }
        }
        let cmds = net.take(frame);
        for p in [&mut a, &mut b, &mut c] {
            p.logic(frame, &cmds, [0; 3]);
        }
        let h = a.sim.fingerprint();
        assert_eq!(b.sim.fingerprint(), h, "B diverged from A at frame {frame}");
        assert_eq!(c.sim.fingerprint(), h, "C diverged from A at frame {frame}");
        seen.append(&mut c.sim.events);
        // A concussion's stun reached a General (the built-in table has no `Shock` blocks, so
        // headless the stun is the engine's disable; in game the shellshock rides with it).
        shocked |= c.sim.players.iter().flatten().any(|p| p.shock.active.is_some()) || c.host.disabled.borrow().iter().any(|&(id, _, _)| generals.contains(&id));
        // Each player's machine: its client runs the autopilot and the bridge sends what it says.
        for p in [&mut a, &mut b] {
            let inp = ClientIn { dt: LOGIC_DT, local_player: p.local, logic_frame: frame, input_allowed: 1, screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
            let out = p.client.frame_with(&mut p.sim, Some(&p.host), &inp);
            if out.choose_class != 0 {
                let words = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
                let index = if words.is_empty() { (out.choose_class - 1) as usize } else { 0 };
                net.send(frame, p.local, Cmd::ChooseClass(out.choose_class_general, index, words));
            }
            if out.send_input != 0 {
                net.send(frame, p.local, Cmd::Input(out.input_general, out.input, out.input_seen_frame));
            }
            lines.append(&mut p.client.autopilot.as_mut().unwrap().log);
        }
    }
    (seen, lines, shocked)
}

/// The two-instance in-engine test's script (`ZHBO_AUTOPILOT=mp`, `scripts/mptest-mac.sh`), headless
/// on the same lockstep harness: two machines, each running the multiplayer autopilot for its own
/// player only (the shooter and the target), a third that never renders, and the engine side the
/// bridge plays in game (take control once each General stands at the meeting point; a fresh
/// General from the Command Center when one dies). The script must reach every step it claims —
/// the picks, the sidearm, the throw, the fight, a death, the respawned General coming back with
/// his class and no pick, the field computer — with every machine agreeing on every frame.
#[cfg(feature = "autopilot")]
#[test]
fn the_multiplayer_autopilot_fights_in_lockstep() {
    let _g = crate::profile::lock();
    crate::profile::reset();
    let (seen, lines, _) = mp_autopilot_match(1, 30 * 45);
    let count = |player: i32, f: &dyn Fn(&EventKind) -> bool| seen.iter().filter(|e| e.player == player && f(&e.kind)).count();
    for player in [0, 1] {
        assert!(count(player, &|k| matches!(k, EventKind::ClassChosen { .. })) >= 1, "player {player} picked a class");
        assert!(count(player, &|k| matches!(k, EventKind::Shot { .. })) >= 2, "player {player} fired");
        assert!(count(player, &|k| matches!(k, EventKind::Swapped)) >= 2, "player {player} swapped to the sidearm and back");
        assert!(count(player, &|k| matches!(k, EventKind::Thrown { lethal: true })) >= 1, "player {player} threw the lethal");
    }
    assert!(count(1, &|k| matches!(k, EventKind::GeneralDied { .. })) >= 1, "the shooter killed the target's General");
    assert_eq!(count(0, &|k| matches!(k, EventKind::GeneralDied { .. })), 0, "the target's short burst did not kill the shooter");
    assert!(count(0, &|k| matches!(k, EventKind::FieldComputerOpened)) >= 1, "the shooter left through the field computer");
    // The target's next General came in with his class (a pick is only offered to the classless).
    assert_eq!(count(1, &|k| matches!(k, EventKind::EnteredFps { .. })), 2, "the target's second General was taken into first person");
    assert_eq!(count(1, &|k| matches!(k, EventKind::ClassChosen { .. })), 1, "his class came back with him: no second pick");
    assert!(count(1, &|k| matches!(k, EventKind::FieldComputerOpened)) >= 1, "the respawned General left through the field computer");
    assert!(lines.iter().any(|l| l.contains("the fight starts (the other General is in first person)")), "the fight waited for the other General: {lines:?}");
    // One line per General at least (headless every viewmodel index is -1, so a swap logs nothing
    // new; in game each weapon change logs its own).
    assert!(lines.iter().filter(|l| l.contains("in hand")).count() >= 3, "each side logged what it holds: {lines:?}");
    crate::profile::reset();
}

/// The long two-instance run (`ZHBO_AUTOPILOT_ROUNDS`, `mptest-mac.sh` with
/// `ZHBO_MPTEST_ROUNDS`), headless: the target declines his first spawn and is taken again, both
/// play rounds of tacticals (a shellshock), bursts, reloads and knives, the target changes class
/// mid-life after combat and his next General wears it — every machine agreeing on every frame.
#[cfg(feature = "autopilot")]
#[test]
fn the_long_multiplayer_autopilot_run_fights_in_lockstep() {
    let _g = crate::profile::lock();
    crate::profile::reset();
    let (seen, lines, shocked) = mp_autopilot_match(4, 30 * 200);
    let count = |player: i32, f: &dyn Fn(&EventKind) -> bool| seen.iter().filter(|e| e.player == player && f(&e.kind)).count();
    // The decline: back overhead before any class, then taken again and picked.
    let first_return = seen.iter().position(|e| e.player == 1 && matches!(e.kind, EventKind::ReturnedToRts { .. }));
    let first_pick = seen.iter().position(|e| e.player == 1 && matches!(e.kind, EventKind::ClassChosen { .. }));
    assert!(matches!((first_return, first_pick), (Some(r), Some(c)) if r < c), "the target declined his spawn, then picked: {lines:?}");
    assert!(lines.iter().filter(|l| l.contains("round") && l.contains("starts")).count() >= 2, "rounds were played: {lines:?}");
    for player in [0, 1] {
        assert!(count(player, &|k| matches!(k, EventKind::Thrown { lethal: false })) >= 1, "player {player} threw his tactical");
        assert!(count(player, &|k| matches!(k, EventKind::ReloadStarted)) >= 1, "player {player} reloaded");
    }
    assert!(shocked, "a concussion stunned a General");
    assert!(count(1, &|k| matches!(k, EventKind::ClassAtNextSpawn { .. })) >= 1, "the target's mid-life change waited for his next spawn: {lines:?}");
    let change = lines.iter().position(|l| l.contains("changing class mid-life")).expect("the change was made");
    assert!(lines[change..].iter().any(|l| l.contains("first-person session") && l.contains("(class cqb)")), "his next General wore it: {lines:?}");
    // And the sim said so for that General, on every machine: the waiting class came in with a
    // ClassChosen naming it.
    let waited = seen.iter().rposition(|e| e.player == 1 && matches!(e.kind, EventKind::ClassAtNextSpawn { .. })).unwrap();
    let EventKind::ClassAtNextSpawn { general: old, class } = seen[waited].kind else { unreachable!() };
    assert!(seen[waited..].iter().any(|e| e.player == 1 && matches!(e.kind, EventKind::ClassChosen { general, class: c } if general != old && c == class)), "the next General's ClassChosen for {class}");
    crate::profile::reset();
}

/// The long run is meaningful for any number of lives from two: the target's class change falls in
/// a life that has a next one ([`crate::client::Autopilot::mp_change_life`]: the first fight for two
/// lives, the first round for three), and his next General wears it, on every machine.
#[test]
fn the_long_run_changes_class_in_a_life_with_a_next_one() {
    let _g = crate::profile::lock();
    for rounds in [2u32, 3] {
        crate::profile::reset();
        let (seen, lines, _) = mp_autopilot_match(rounds, 30 * 150);
        let change = lines.iter().position(|l| l.contains("changing class mid-life")).unwrap_or_else(|| panic!("{rounds} lives: the change was made: {lines:?}"));
        assert!(lines[change..].iter().any(|l| l.contains("first-person session") && l.contains("(class cqb)")), "{rounds} lives: his next General wore it: {lines:?}");
        let waited = seen.iter().rposition(|e| e.player == 1 && matches!(e.kind, EventKind::ClassAtNextSpawn { .. })).unwrap_or_else(|| panic!("{rounds} lives: the change waited"));
        assert!(seen[waited..].iter().any(|e| e.player == 1 && matches!(e.kind, EventKind::ClassChosen { .. })), "{rounds} lives: the next General's ClassChosen");
    }
    crate::profile::reset();
}

/// In a network game the other players' Generals are in first person on their own machines; here
/// they must still show their class weapon in their hands (the native third person, the only one
/// with the attachments and the camo). Only the General this machine looks through is left out:
/// his body is hidden and the viewmodel draws his arms. (The pass once skipped every first-person
/// General, which is the same thing in single player and wrong for everyone else's.)
#[test]
fn a_remote_first_person_general_keeps_his_third_person() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(general(2, 1));
    let mut sim = Sim::new();
    super::tests::spawn(&mut sim, &host, 0, 1);
    super::tests::spawn(&mut sim, &host, 1, 2);
    assert!(sim.player(0).unwrap().is_first_person() && sim.player(1).unwrap().is_first_person());
    let posed = |viewer: Option<u32>| sim.native_poses(None, viewer).iter().map(|p| p.general).collect::<Vec<_>>();
    assert_eq!(posed(Some(1)), vec![2], "player 0's machine draws player 1's General, not its own");
    assert_eq!(posed(Some(2)), vec![1], "and player 1's machine the other way round");
    assert_eq!(posed(None), vec![1, 2], "an overhead viewer sees both");
}

/// A weapon drop on two players' machines: player 1's General dies (past the grace period) and
/// drops his rifle; player 0 steps onto the spot and takes its ammo (the same weapon), then a
/// second death drops another gun, which player 0 swaps for with the use key, putting his own
/// down. Every frame both machines (each predicting its own player) and a third that never
/// renders hold the same fingerprint, objects, spawns and destroys.
#[test]
fn a_dropped_weapon_plays_alike_on_every_machine() {
    let mut peers = [Peer::bare(0, 7), Peer::bare(1, 9), Peer::bare(2, 11)];
    let mut net = Net::default();
    let rifle = CustomClass { name: "Rifle".into(), primary: "an94_mp".into(), secondary: "fnp45_mp".into(), melee: "knife_held_mp".into(), ..Default::default() };
    let sniper = CustomClass { name: "Sniper".into(), primary: "svu_mp".into(), secondary: "fnp45_mp".into(), melee: "knife_held_mp".into(), ..Default::default() };
    let late = 500;
    let mut seen: Vec<EventKind> = Vec::new();
    for frame in 1..=late + 400 {
        // The engine's logic, alike everywhere: player 1's first General dies at `late`, his
        // second (a sniper) at `late + 150`; player 0 is moved onto the first drop.
        for peer in &mut peers {
            match frame {
                1 => {
                    let mut g0 = general(100, 0);
                    g0.pos = Vec3::new(0.0, 0.0, 0.0);
                    let mut g1 = general(101, 1);
                    g1.pos = Vec3::new(0.0, 40.0, 0.0);
                    peer.host.add(g0);
                    peer.host.add(g1);
                }
                f if f == late || f == late + 150 => {
                    let id = if f == late { 101 } else { 102 };
                    for o in peer.host.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                        o.flags |= objflags::DEAD;
                    }
                }
                f if f == late + 60 => {
                    let at = peer.sim.drops.first().map(|d| d.pos).expect("the first drop lies there");
                    for o in peer.host.objects.borrow_mut().iter_mut().filter(|o| o.id == 100) {
                        o.pos = Vec3::new(at.x, at.y, 0.0);
                    }
                }
                f if f == late + 100 => {
                    // Before player 0, where he stands now.
                    let at = peer.host.get(100).pos;
                    let mut g = general(102, 1);
                    g.pos = at + Vec3::new(8.0, 0.0, 0.0);
                    peer.host.add(g);
                }
                _ => {}
            }
        }
        match frame {
            2 => {
                net.send(frame, 0, Cmd::ChooseClass(100, 0, rifle.to_wire_words()));
                net.send(frame, 1, Cmd::ChooseClass(101, 0, rifle.to_wire_words()));
            }
            4 => {
                net.send(frame, 0, Cmd::TakeControl(100));
                net.send(frame, 1, Cmd::TakeControl(101));
            }
            // Mid-life past the grace period: his next General wears it (`MP_CHANGE_CLASS_NEXT_SPAWN`).
            f if f == late - 50 => net.send(frame, 1, Cmd::ChooseClass(101, 0, sniper.to_wire_words())),
            f if f == late + 104 => net.send(frame, 1, Cmd::TakeControl(102)),
            _ => {}
        }
        let cmds = net.take(frame);
        for peer in &mut peers {
            peer.logic(frame, &cmds, [0; 3]);
        }
        let h = peers[0].sim.fingerprint();
        for peer in &peers[1..] {
            let at = format!("seat {}'s machine at frame {frame}", peer.local);
            assert_eq!(peer.sim.fingerprint(), h, "{at}: the sim state");
            assert_eq!(peer.host.objects, peers[0].host.objects, "{at}: the objects");
            assert_eq!(*peer.host.spawned.borrow(), *peers[0].host.spawned.borrow(), "{at}: spawns");
            assert_eq!(*peer.host.destroyed.borrow(), *peers[0].host.destroyed.borrow(), "{at}: destroys");
        }
        seen.extend(peers[2].sim.events.drain(..).filter(|e| !matches!(e.kind, EventKind::Shot { .. } | EventKind::Recoil { .. } | EventKind::Impact { .. })).map(|e| e.kind));
        // Player 0 fires and reloads early (room for spare rounds), and presses the use key once
        // the second drop lies before him.
        let b = crate::client::Bindings::default();
        let mut keys = crate::input::KeySet::default();
        match frame {
            60..=120 => keys.set_down(b.fire, true),
            130 => keys.set_down(b.reload, true),
            f if f == late + 200 => keys.set_down(b.field_computer, true),
            _ => {}
        }
        
        peers[0].render(&mut net, frame, 1 + (frame % 2), keys, (0.0, 0.0));
        peers[1].render(&mut net, frame, 1, crate::input::KeySet::default(), (0.0, 0.0));
    }
    let has = |f: &dyn Fn(&EventKind) -> bool| seen.iter().any(f);
    assert!(has(&|k| matches!(k, EventKind::WeaponDropped { def, .. } if def.id == "an94_mp")), "the first death dropped the rifle");
    assert!(has(&|k| matches!(k, EventKind::AmmoPickedUp { def, .. } if def.id == "an94_mp")), "player 0 took its ammo");
    assert!(has(&|k| matches!(k, EventKind::WeaponDropped { def, .. } if def.id == "svu_mp")), "the second death dropped the sniper rifle: {seen:?}");
    assert!(has(&|k| matches!(k, EventKind::WeaponPickedUp { def, .. } if def.id == "svu_mp")), "player 0 swapped for it: {:?}", peers[0].sim.drops.iter().map(|d| (d.weapon.def.id.clone(), d.pos)).collect::<Vec<_>>());
}
