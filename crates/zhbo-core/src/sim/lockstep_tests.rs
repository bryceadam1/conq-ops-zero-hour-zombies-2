//! The multiplayer determinism gate on the built-in catalog: the harness's match
//! ([`super::harness`]: the peers, the replicated command stream with Zero Hour's run-ahead, each
//! machine's own render cadence and presentation, a machine that never renders) over rounds that
//! cycle every slot weapon of the catalog through a custom class sent as its wire words. The
//! full-roster gate over a real install is `zhbo-assets`' (`the_install_roster_plays_in_lockstep`).
use super::harness::{capped, play, script, Cmd, Match, Net, Peer, MIDLIFE_PICK, ROUND};
use super::tests::{general, soldier, FakeHost};
use super::*;
use crate::classes::CustomClass;

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
/// lets go of him at once (his last input would otherwise drive him forever) and the ally can take
/// him into first person as his own.
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
    for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 1) {
        o.owner_player = 1;
    }
    sim.step(&host, 11);
    assert!(sim.player(0).is_none(), "the leaver's seat let go of him");
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
