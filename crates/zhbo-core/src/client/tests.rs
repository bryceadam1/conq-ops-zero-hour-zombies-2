    use super::*;
    use crate::sim::tests::{general, FakeHost};

    fn frame_with(client: &mut Client, sim: &mut Sim, host: &FakeHost, inp: &mut ClientIn, steps: usize) -> ClientOut {
        let mut out = ClientOut::default();
        for _ in 0..steps {
            out = client.frame(sim, inp);
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            inp.dt = LOGIC_DT;
            let f = sim.frame + 1;
            sim.step(host, f);
            inp.logic_frame = f;
        }
        out
    }

    #[test]
    fn enter_transition_then_capture_and_send_one_input_per_logic_frame() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);

        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.camera_override, 1);
        assert!(matches!(client.view, ViewMode::Entering { .. }));

        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert_eq!(out.capture_mouse, 1);
        assert_eq!(out.hide_object, 1);
        assert_eq!(out.hide_rts_ui, 1);

        // Same logic frame twice: only the first render frame sends.
        let a = client.frame(&mut sim, &inp);
        let b = client.frame(&mut sim, &inp);
        assert_eq!((a.send_input, b.send_input), (1, 0));
    }

    /// The camera FOV follows BO2's ADS zoom (`weapons::zoom`) on the sim's ADS fraction: the
    /// stand-in AN-94 carries no zoom fractions (0), so it stays at the hip FOV until the lerp
    /// ends, then snaps to its 50° (at 4:3, made Hor+), and back the moment the aim leaves 1.
    #[test]
    fn the_view_fov_follows_the_ads_zoom() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        let aspect = 16.0 / 9.0;
        let hip = horizontal_fov(65.0, aspect);
        let zoomed = horizontal_fov(sim.player(0).unwrap().arsenal.current().def.ads_fov, aspect);
        assert!(zoomed < hip);
        inp.keys.set_down(Key::MouseRight, true);
        let mut seen_full = false;
        for _ in 0..60 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            let ads = sim.player(0).unwrap().arsenal.ads;
            // `frame` reads the sim before this step, so the FOV is the previous step's aim.
            if out.hfov < hip - 1e-4 {
                assert!((out.hfov - zoomed).abs() < 1e-4, "a zero fraction snaps: {} at aim {ads}", out.hfov);
                seen_full = true;
            }
        }
        assert!(seen_full, "the zoom reached the camera");
        inp.keys.set_down(Key::MouseRight, false);
        frame_with(&mut client, &mut sim, &host, &mut inp, 3);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!((out.hfov - hip).abs() < 1e-4, "aiming out leaves the zoom at once: {}", out.hfov);
    }

    #[test]
    fn a_shot_event_for_the_local_player_sets_viewmodel_fired() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        // Another player's shot is not the local viewmodel's effect.
        sim.events.push(crate::sim::Event { player: 1, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO, left: false } });
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.viewmodel_fired, 0);
        // The sim's own `Shot` event is what the bridge's `fired` flag comes from.
        sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO, left: false } });
        let out = client.frame(&mut sim, &inp);
        assert_eq!((out.viewmodel_fired, out.viewmodel_fired_left), (1, 0));
        // A dual-wield pair's left shot sets the left flag only (its flash at `tag_flash1`).
        sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO, left: true } });
        let out = client.frame(&mut sim, &inp);
        assert_eq!((out.viewmodel_fired, out.viewmodel_fired_left), (0, 1));
    }

    /// A dual-wield pair's left gun plays its own `_lh` shot; the right gun the weapon's.
    #[test]
    fn the_left_guns_shot_plays_its_own_sound() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
            class: crate::weapons::Class::Pistol,
            fire_sound: "ZHBO_BO2_Fire_test_dw_mp".into(),
            left_fire_sound: "ZHBO_BO2_FireLeft_test_dw_mp".into(),
            dual_wield_weapon: "test_lh_mp".into(),
            ..Default::default()
        }));
        let p = sim.players[0].as_mut().unwrap();
        let active = p.arsenal.active;
        p.arsenal.weapons[active] = crate::weapons::WeaponState::new(def);
        for (left, want) in [(true, "ZHBO_BO2_FireLeft_test_dw_mp"), (false, "ZHBO_BO2_Fire_test_dw_mp")] {
            client.cues.queue.clear();
            sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO, left } });
            client.frame(&mut sim, &inp);
            let cues: Vec<&str> = client.cues.queue.iter().map(|c| c.event).collect();
            assert!(cues.contains(&want), "left {left}: {cues:?}");
        }
    }

    /// Another player's throw and set-down are heard at them: the offhand's third-person
    /// `fireSound`; the local player's own throw plays its player sound, not that one.
    #[test]
    fn another_players_offhand_is_heard_at_them() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        sim.take_control(&host, 1, 2);
        sim.choose_class(1, 2, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let leak = |d: crate::weapons::WeaponDef| -> &'static crate::weapons::WeaponDef { Box::leak(Box::new(d)) };
        let frag = leak(crate::weapons::WeaponDef { class: crate::weapons::Class::Lethal, npc_fire_sound: "ZHBO_BO2_Snd_wpn_grenade_throw".into(), ..Default::default() });
        let claymore = leak(crate::weapons::WeaponDef {
            class: crate::weapons::Class::Tactical,
            place: crate::weapons::Place::Plant,
            equipment_kind: crate::equipment::Kind::Claymore,
            npc_fire_sound: "ZHBO_BO2_Snd_fly_claymore_plant_npc".into(),
            ..Default::default()
        });
        for p in [0, 1] {
            let a = &mut sim.players[p].as_mut().unwrap().arsenal;
            a.lethal.def = frag;
            a.tactical.def = claymore;
        }
        let at = sim.players[1].as_ref().unwrap().body.eye();
        for (kind, want) in [
            (crate::sim::EventKind::Thrown { lethal: true }, "ZHBO_BO2_Snd_wpn_grenade_throw"),
            (crate::sim::EventKind::Planted { kind: crate::equipment::Kind::Claymore }, "ZHBO_BO2_Snd_fly_claymore_plant_npc"),
        ] {
            client.cues.queue.clear();
            sim.events.push(crate::sim::Event { player: 1, kind });
            client.frame(&mut sim, &inp);
            assert!(client.cues.queue.iter().any(|c| c.event == want && c.at == Some(at)), "{kind:?}: {:?}", client.cues.queue.iter().map(|c| c.event).collect::<Vec<_>>());
            client.cues.queue.clear();
            sim.events.push(crate::sim::Event { player: 0, kind });
            client.frame(&mut sim, &inp);
            assert!(!client.cues.queue.iter().any(|c| c.event == want), "the thrower hears its own sound, not the third-person one");
        }
    }

    /// The third-person layers the remote-sound tests share. `soundlayers` is one process-wide
    /// table that `install` replaces, so every test installs the same rows (tests run in parallel).
    fn install_test_layers() {
        let row = |key: &str| {
            let chain = format!("ZHBO_BO2_FireNpc_{}", key.trim_start_matches("ZHBO_FX_"));
            let layer = crate::soundlayers::Layer { event: Box::leak(chain.clone().into_boxed_str()), min: 0.0, max: 1000.0, min_curve: crate::soundlayers::FLAT, curve: crate::soundlayers::FLAT };
            crate::soundlayers::format_row(key, &chain, &layer)
        };
        crate::soundlayers::install(&(row("ZHBO_FX_test_remote_world") + &row("ZHBO_FX_test_remote_burst")));
    }

    /// The Black Hat fires as its hack lands: the hacker hears its own `fireSoundPlayer`, everyone
    /// else its third-person `fireSound` at him (BO2 plays an offhand's fire for both).
    #[test]
    fn the_black_hat_is_heard_as_its_hack_lands() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        sim.take_control(&host, 1, 2);
        sim.choose_class(1, 2, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let pda: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
            class: crate::weapons::Class::Tactical,
            place: crate::weapons::Place::Hack,
            fire_sound: "ZHBO_BO2_Snd_fly_claymore_plant_plr".into(),
            npc_fire_sound: "ZHBO_BO2_Snd_fly_claymore_plant_npc".into(),
            ..Default::default()
        }));
        for p in [0, 1] {
            sim.players[p].as_mut().unwrap().arsenal.tactical.def = pda;
        }
        let at = sim.players[1].as_ref().unwrap().body.eye();
        client.cues.queue.clear();
        sim.events.push(crate::sim::Event { player: 1, kind: crate::sim::EventKind::Hacked });
        client.frame(&mut sim, &inp);
        assert!(client.cues.queue.iter().any(|c| c.event == "ZHBO_BO2_Snd_fly_claymore_plant_npc" && c.at == Some(at)), "{:?}", client.cues.queue);
        client.cues.queue.clear();
        sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Hacked });
        client.frame(&mut sim, &inp);
        assert!(client.cues.queue.iter().any(|c| c.event == "ZHBO_BO2_Snd_fly_claymore_plant_plr" && c.at.is_none()), "{:?}", client.cues.queue);
        assert!(!client.cues.queue.iter().any(|c| c.event == "ZHBO_BO2_Snd_fly_claymore_plant_npc"), "the hacker hears his own sound");
    }

    /// Another player's burst opens with the def's third-person opening beside the shot, and the
    /// round that empties his magazine plays the def's last-round sound in place of the shot
    /// (`FUN_005320e0`); the rounds between are the FX weapon's chain.
    #[test]
    fn another_players_burst_opens_and_his_last_round_ends_with_the_defs_sounds() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        sim.take_control(&host, 1, 2);
        sim.choose_class(1, 2, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let fx = "ZHBO_FX_test_remote_burst";
        install_test_layers();
        let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
            fx_weapon: fx.into(),
            npc_fire_start_sound: "ZHBO_BO2_Snd_test_fire_npc_burst".into(),
            npc_fire_last_sound: "ZHBO_BO2_Snd_test_fire_npc_last".into(),
            ..crate::weapons::catalog().get("an94_mp").unwrap().clone()
        }));
        let at = Vec3::new(40.0, 30.0, 5.0);
        let shot = crate::sim::EventKind::Shot { origin: at, end: at, left: false };
        // (rounds this frame, the string and the magazine after them) -> what plays.
        let mut hear = |rounds: usize, string: u16, ammo: u16| -> Vec<&'static str> {
            let p = sim.players[1].as_mut().unwrap();
            let active = p.arsenal.active;
            p.arsenal.weapons[active] = crate::weapons::WeaponState::new(def);
            (p.arsenal.weapons[active].string, p.arsenal.weapons[active].ammo) = (string, ammo);
            client.cues.queue.clear();
            for _ in 0..rounds {
                sim.events.push(crate::sim::Event { player: 1, kind: shot });
            }
            client.frame(&mut sim, &inp);
            client.cues.queue.iter().filter(|c| c.at == Some(at)).map(|c| c.event).collect()
        };
        let (chain, open, last) = ("ZHBO_BO2_FireNpc_test_remote_burst", "ZHBO_BO2_Snd_test_fire_npc_burst", "ZHBO_BO2_Snd_test_fire_npc_last");
        assert_eq!(hear(1, 1, 20), [chain, open], "the string's first round");
        assert_eq!(hear(2, 3, 20), [chain, chain], "later rounds");
        assert_eq!(hear(3, 3, 20), [chain, open, chain, chain], "a whole burst in one step opens once, on its first round");
        assert_eq!(hear(2, 2, 0), [chain, open, last], "the second round emptied the magazine");
    }

    /// Every event of another player's, of every kind: what its owner hears or sees at a place in
    /// the world (a pellet's impact and its effect, a blast, a bounce, a dud, a Trophy kill, the
    /// deployed shield) plays the same here, and nothing of his plays as the local player's own
    /// (2D) sound. His shots and launches are his gun's third-person chain at him. (Before, only
    /// others' shots, throws and set-downs played: their impacts, grenades, launchers and shield
    /// were silent and drew no impact on every other machine.)
    #[test]
    fn another_players_world_events_play_here_as_for_their_owner() {
        use crate::impacts::Surface;
        use crate::sim::EventKind as K;
        let at = Vec3::new(40.0, 30.0, 5.0);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let kinds = [
            K::EnteredFps { general: 1 },
            K::ClassChosen { general: 1, class: crate::sim::ClassName::new("x") },
            K::ClassAtNextSpawn { general: 1, class: crate::sim::ClassName::new("x") },
            K::FieldComputerOpened,
            K::ReturnedToRts { general: 1 },
            K::Shot { origin: at, end: at, left: false },
            K::Shot { origin: at, end: at, left: true },
            K::Impact { at, dir, surface: Surface::Concrete, head: false, fatal: false, impact_type: 1, exit: false },
            K::Impact { at, dir, surface: Surface::Flesh, head: true, fatal: true, impact_type: 1, exit: true },
            K::HitMarker { headshot: false, kill: true },
            K::Recoil { gun_kick: (1.0, 1.0) },
            K::Flinch { pitch_deg: 1.0 },
            K::Damaged { amount: 10.0 },
            K::ReloadStarted,
            K::Swapped,
            K::GeneralDied { general: 1 },
            K::Melee,
            K::MeleeHit,
            K::OffhandReady { lethal: true },
            K::Thrown { lethal: true },
            K::CantPlant { lethal: true },
            K::Launched,
            K::Exploded { at, sound: "ZHBO_BO2_Snd_test_blast", impact_type: 7, surface: Surface::Dirt },
            K::Bounced { at, sound: "ZHBO_BO2_Snd_test_bounce" },
            K::Dud { at, sound: "ZHBO_BO2_Snd_test_dud", surface: Surface::Dirt },
            K::Planted { kind: crate::equipment::Kind::Claymore },
            K::Detonated,
            K::Intercepted { at },
            K::DestroyedEquipment,
            K::Hacked,
            K::Hacking { progress: 0.5 },
            K::AltToggled,
            K::Hammer { up: true },
            K::BuildPlaced { template: 1 },
            K::BuildRefused { status: 0 },
            K::Working { kind: 0, percent: 50.0 },
            K::WorkDone { kind: 0 },
            K::ShieldDeployed { at },
            K::ShieldPickedUp,
            K::ShieldDestroyed { by: None, at },
            K::ShieldHit { at, dir, knife: false, impact_type: 1 },
            K::ShieldHit { at, dir, knife: true, impact_type: 1 },
        ];
        // The other player's gun has a third-person chain (`soundlayers`): his shot and launch.
        let fx = "ZHBO_FX_test_remote_world";
        install_test_layers();
        let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { fx_weapon: fx.into(), ..crate::weapons::catalog().get("an94_mp").unwrap().clone() }));
        // What a frame presents of one event of `player`'s: the positional cues, the 2D cues and
        // the impacts.
        let present = |player: i32, kind: K| {
            let host = FakeHost::default();
            host.add(general(1, 0));
            host.add(general(2, 1));
            let mut sim = Sim::new();
            let mut client = Client::default();
            let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
            sim.take_control(&host, 0, 1);
            sim.choose_class(0, 1, 0, &[]);
            sim.take_control(&host, 1, 2);
            sim.choose_class(1, 2, 0, &[]);
            frame_with(&mut client, &mut sim, &host, &mut inp, 60);
            for p in sim.players.iter_mut().flatten() {
                let active = p.arsenal.active;
                p.arsenal.weapons[active] = crate::weapons::WeaponState::new(def);
            }
            client.cues.queue.clear();
            sim.events.push(crate::sim::Event { player, kind });
            let out = client.frame(&mut sim, &inp);
            // A voice is its speaker's own (`sounds::voice_event`: his seat's voice number): the
            // same line, in another player's voice.
            let voice = |e: &'static str| if e.starts_with("ZHBO_BO2_Pain") { &e[..e.len() - 1] } else { e };
            // A chatter line is its speaker's own roll (`sounds::chatter_roll`).
            let mut placed: Vec<(&str, [i32; 3])> = client.cues.queue.iter().filter(|c| !c.event.starts_with("ZHBO_BO2_Bc_")).filter_map(|c| c.at.map(|a| (voice(c.event), [a.x as i32, a.y as i32, a.z as i32]))).collect();
            placed.sort();
            let flat: Vec<&str> = client.cues.queue.iter().filter(|c| c.at.is_none() && !c.event.contains("Step")).map(|c| c.event).collect();
            (placed, flat, out.hits[..out.hits_len as usize].to_vec())
        };
        for kind in kinds {
            let (own_placed, _, own_hits) = present(0, kind);
            let (placed, flat, hits) = present(1, kind);
            for cue in &own_placed {
                assert!(placed.contains(cue), "{kind:?}: the owner hears {cue:?} at its place, so everyone does: {placed:?}");
            }
            assert_eq!(hits, own_hits, "{kind:?}: the same impacts as its owner's");
            assert!(flat.is_empty(), "{kind:?}: another player's event plays nothing as the local player's own: {flat:?}");
            if matches!(kind, K::Shot { .. } | K::Launched) {
                assert!(placed.iter().any(|(e, _)| *e == "ZHBO_BO2_FireNpc_test_remote_world"), "{kind:?}: his gun's third-person chain: {placed:?}");
            }
        }
        // And they are heard: every place the test names plays something for the other player.
        for kind in [kinds[7], kinds[22], kinds[23], kinds[24], kinds[27], kinds[37], kinds[39], kinds[40]] {
            assert!(!present(1, kind).0.is_empty(), "{kind:?} is heard on the other machine");
        }
        assert_eq!(present(1, kinds[7]).2.len(), 1, "another player's impact is drawn here");
    }

    /// The local player's own impacts keep their slots: other players' rounds take only what is
    /// left of the frame's `CLIENT_HITS`.
    #[test]
    fn other_players_impacts_fill_only_the_slots_the_own_ones_leave() {
        use crate::impacts::Surface;
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(general(2, 1));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        sim.take_control(&host, 1, 2);
        sim.choose_class(1, 2, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let hit = |x: f32| crate::sim::EventKind::Impact { at: Vec3::new(x, 0.0, 0.0), dir: Vec3::new(1.0, 0.0, 0.0), surface: Surface::Dirt, head: false, fatal: false, impact_type: 1, exit: false };
        // The other player's shotgun first in the event order, then three of the local player's.
        for i in 0..CLIENT_HITS {
            sim.events.push(crate::sim::Event { player: 1, kind: hit(100.0 + i as f32) });
        }
        for i in 0..3 {
            sim.events.push(crate::sim::Event { player: 0, kind: hit(i as f32) });
        }
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.hits_len as usize, CLIENT_HITS);
        let xs: Vec<i32> = out.hits.iter().map(|h| h.point.x as i32).collect();
        assert_eq!(xs, vec![0, 1, 2, 100, 101, 102, 103, 104]);
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.hits_len, 0, "a frame's impacts are its own");
    }

    #[test]
    fn the_camera_carries_the_sims_view_kick_and_its_roll_without_moving_the_mouse_aim() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        let rest = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(rest.view_roll, 0.0);
        // A kick held at an angle (no velocity, no recentre speed on the stand-in MTAR's pull).
        let k = &mut sim.players[0].as_mut().unwrap().view_kick;
        k.angle = [-2.0, 1.0, 8.0];
        let (yaw, pitch) = (client.yaw, client.pitch);
        // Two logic frames: the interpolation's previous sample catches up too.
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        let out = client.frame(&mut sim, &inp);
        assert_eq!((client.yaw, client.pitch), (yaw, pitch), "the mouse aim never takes the kick");
        let k = sim.player(0).unwrap().view_kick;
        let (ky, kp) = k.aim_offset();
        let want = crate::math::dir_from_angles(yaw + ky, pitch + kp);
        assert!((out.dir - want).length() < 1e-4, "camera = aim + kick: {:?} vs {want:?}", out.dir);
        assert!((out.view_roll - k.roll()).abs() < 1e-6 && out.view_roll > 0.0, "the roll axis reaches the camera");
        assert!((out.viewmodel_angles.z - rest.viewmodel_angles.z - k.roll()).abs() < 0.02, "and the gun rolls with it");
    }

    /// A mid-life pick after he has fought keeps the kit and shows BO2's
    /// `MP_CHANGE_CLASS_NEXT_SPAWN` hint (`menuclass`'s `iprintlnbold`), its install text in the log.
    #[test]
    fn a_pick_after_combat_shows_the_next_spawn_hint() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        sim.players[0].as_mut().unwrap().combat = true;
        sim.choose_class(0, 1, 1, &[]);
        let e = *sim.events.iter().find(|e| matches!(e.kind, crate::sim::EventKind::ClassAtNextSpawn { .. })).expect("the event");
        assert!(crate::sim::describe_event(&e).unwrap().contains(crate::sim::hint_text("MP_CHANGE_CLASS_NEXT_SPAWN")));
        assert_eq!(crate::sim::hint_text("MP_CHANGE_CLASS_NEXT_SPAWN"), "Your class will change the next time you spawn");
        client.frame(&mut sim, &inp);
        assert!(matches!(client.hud.hint, Some(("MP_CHANGE_CLASS_NEXT_SPAWN", _))));
    }

    /// `CantPlant` shows BO2's weapon hint for 1.8 s; the log line carries its text.
    #[test]
    fn cant_plant_shows_the_hint_for_its_time() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let e = crate::sim::Event { player: 0, kind: crate::sim::EventKind::CantPlant { lethal: true } };
        let line = crate::sim::describe_event(&e).unwrap();
        assert!(line.contains(crate::sim::hint_text("WEAPON_CANT_PLANT_EQUIPMENT")), "{line}");
        sim.events.push(e);
        inp.dt = LOGIC_DT;
        client.frame(&mut sim, &inp);
        assert!(matches!(client.hud.hint, Some(("WEAPON_CANT_PLANT_EQUIPMENT", _))));
        let frames = (crate::hud::HINT_TIME / LOGIC_DT).ceil() as usize + 2;
        for _ in 0..frames {
            client.frame(&mut sim, &inp);
        }
        assert!(client.hud.hint.is_none(), "gone after 1.8 s");
    }

    fn impact(player: i32, surface: crate::impacts::Surface, head: bool, fatal: bool) -> crate::sim::Event {
        crate::sim::Event {
            player,
            kind: crate::sim::EventKind::Impact {
                at: Vec3::new(5.0, 0.0, 1.0),
                dir: Vec3::new(0.0, 1.0, 0.0),
                surface,
                head,
                fatal,
                impact_type: 2,
                exit: false,
            },
        }
    }

    #[test]
    fn local_impacts_fill_the_record_and_pick_the_cue() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        client.cues.queue.clear();

        // Another player's impact is the world's: drawn and heard here as well (seat 1 has no
        // record here; the event needs none).
        sim.events.push(impact(1, crate::impacts::Surface::Metal, false, false));
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.hits_len, 1);
        assert!(client.cues.queue.iter().any(|c| c.event == sounds::IMPACT && c.at == Some(Vec3::new(5.0, 0.0, 1.0))), "someone else's hit is heard where it landed");
        client.cues.queue.clear();

        // A blood kill and a wall.
        sim.events.push(impact(0, crate::impacts::Surface::Flesh, true, true));
        sim.events.push(impact(0, crate::impacts::Surface::Concrete, false, false));
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.hits_len, 2);
        assert_eq!(out.hits[0].surface, crate::impacts::Surface::Flesh as i32);
        assert_eq!((out.hits[0].head, out.hits[0].fatal), (1, 1));
        assert_eq!(out.hits[0].impact_type, 2);
        assert_eq!(out.hits[1].surface, crate::impacts::Surface::Concrete as i32);
        let cues: Vec<&str> = client.cues.queue.iter().map(|c| c.event).collect();
        assert!(cues.contains(&sounds::IMPACT_FLESH), "{cues:?}");
        assert!(cues.contains(&sounds::IMPACT), "{cues:?}");

        // A shot on its own no longer plays an impact sound (the pellet's event does).
        client.cues.queue.clear();
        sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO, left: false } });
        client.frame(&mut sim, &inp);
        let cues: Vec<&str> = client.cues.queue.iter().map(|c| c.event).collect();
        assert!(!cues.contains(&sounds::IMPACT) && !cues.contains(&sounds::IMPACT_FLESH), "{cues:?}");
    }

    #[test]
    fn the_record_clamps_at_its_cap() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        for _ in 0..CLIENT_HITS + 1 {
            sim.events.push(impact(0, crate::impacts::Surface::Dirt, false, false));
        }
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.hits_len, CLIENT_HITS as i32);
    }

    #[test]
    fn mouse_right_turns_right_and_w_walks_forward() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        inp.mouse_dx = 100.0;
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(client.yaw < 0.0, "turned clockwise (right): {}", client.yaw);
        inp.mouse_dx = 0.0;
        let yaw = client.yaw;
        inp.keys.set_down(Key::W, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 30);
        let pos = host.get(1).pos;
        let heading = pos.y.atan2(pos.x);
        assert!((heading - yaw).abs() < 0.05, "walked along view yaw {yaw}, heading {heading}");
    }

    #[test]
    fn holding_m_opens_the_map_and_tab_the_scoreboard() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert!(!client.hud.map_open);
        inp.keys.set_down(Key::M, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(client.hud.map_open && !client.hud.scoreboard_open);
        inp.keys.set_down(Key::M, false);
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(!client.hud.map_open);
        inp.keys.set_down(Key::Tab, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(client.hud.scoreboard_open && !client.hud.map_open);
    }

    #[test]
    fn field_computer_key_leaves_first_person() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        frame_with(&mut client, &mut sim, &host, &mut inp, (crate::sim::FIELD_COMPUTER_RAISE / LOGIC_DT) as usize + 5);
        assert!(matches!(client.view, ViewMode::Leaving { .. }), "{:?}", client.view);
        let mut look = None;
        for _ in 0..120 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            if out.rts_look_at_valid != 0 {
                look = Some(out.rts_look_at);
            }
        }
        assert_eq!(client.view, ViewMode::Rts);
        assert!(look.is_some(), "RTS camera recentred on the General");
    }

    /// Entering and leaving first person never jump: every frame the camera moves a little, starting
    /// from (and ending on) the overhead camera's pose and field of view.
    #[test]
    fn transitions_are_smooth_both_ways() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { rts_cam_pos: Vec3::new(-150.0, 60.0, 220.0), rts_hfov: 0.8, ..Default::default() };
        let rts_dir = (Vec3::new(0.0, 0.0, 0.0) - inp.rts_cam_pos).normalized_or_zero();
        inp.rts_cam_dir = rts_dir;
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        let mut outs = Vec::new();
        for _ in 0..200 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            if matches!(client.view, ViewMode::Entering { .. } | ViewMode::Fps) {
                outs.push(out);
            }
            if client.view == ViewMode::Fps && outs.len() > 2 {
                break;
            }
        }
        // While first person owns the camera the view reports the first-person camera, not the
        // overhead one: leaving must not aim for that.
        let overhead_pos = inp.rts_cam_pos;
        inp.rts_cam_pos = client.view_eye;
        inp.rts_cam_dir = client.view_dir;
        inp.rts_hfov = 1.3;
        inp.rts_pivot = client.view_eye + client.view_dir * 100.0;
        // Leave through the field computer.
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        let mut leaving = Vec::new();
        for _ in 0..400 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            if out.camera_override != 0 && matches!(client.view, ViewMode::Leaving { .. }) {
                leaving.push(out);
            }
            if client.view == ViewMode::Rts && !leaving.is_empty() {
                break;
            }
        }
        let check = |outs: &[ClientOut], what: &str| {
            for w in outs.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                assert!((a.eye - b.eye).length() < 25.0, "{what}: eye jumped {:?} -> {:?}", a.eye, b.eye);
                assert!(a.dir.dot(b.dir) > 0.97, "{what}: view turned too fast {:?} -> {:?}", a.dir, b.dir);
                assert!((a.hfov - b.hfov).abs() < 0.05, "{what}: fov jumped {} -> {}", a.hfov, b.hfov);
            }
        };
        check(&outs, "entering");
        check(&leaving, "leaving");
        let first = outs[0];
        assert!((first.eye - overhead_pos).length() < 15.0 && first.dir.dot(rts_dir) > 0.97, "starts at the overhead camera");
        assert!((first.hfov - 0.8).abs() < 0.05);
        let last = leaving.last().unwrap();
        assert!(last.dir.dot(rts_dir) > 0.995, "ends looking the overhead camera's way: {:?}", last.dir);
        assert!((last.hfov - 0.8).abs() < 0.02, "ends at the overhead field of view: {}", last.hfov);
        // The overhead camera pivoted on the origin when first person started; it comes back at the
        // same offset from the General (who has not moved), so nothing snaps when it takes over.
        let ViewMode::Rts = client.view else { panic!("not back overhead") };
        let general_at = host.objects.borrow().iter().find(|o| o.id == 1).unwrap().pos;
        let expected = general_at + overhead_pos;
        assert!((last.eye - expected).length() < 8.0, "ends where the overhead camera will be: {:?} vs {:?}", last.eye, expected);
    }

    /// In a match the class screen is pick-only: there is no key-6 editor path and no
    /// delete/copy; a custom pick is just its row key. The profile is untouched by a pick.
    #[test]
    fn the_in_match_pick_is_selection_only() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::save(crate::classes::CustomClass {
            name: "AB".into(),
            primary: "an94_mp".into(),
            ..crate::classes::CustomClass::default()
        });
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert!(client.class_pick_open);

        // The in-match rows are the set's five slots then the defaults (`changeclass.lua`).
        let rows = crate::classui::pick_rows(false);
        assert_eq!(rows[0], crate::classui::PickRow::Custom(0));
        // Its digit key picks the seeded custom and sends its payload, no profile write.
        let custom = rows.iter().position(|r| matches!(r, crate::classui::PickRow::Custom(0))).expect("the custom row");
        let key = crate::classui::pick_key(custom).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        client.frame(&mut sim, &inp);
        assert!(client.class_pick_open, "the pick has not landed in the sim yet");
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM);
        assert_eq!(out.save_profile, 0, "a pick never rewrites the profile");
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        assert_eq!(crate::classes::CustomClass::from_wire_words(&words).unwrap().name, "AB");
        // No editor can open from the pick screen.
        inp.keys.set_down(Key::Key6, true);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Key6, false);
        client.frame(&mut sim, &inp);
        assert!(client.class_pick_open, "key 6 is just another row key (or nothing)");
        crate::profile::reset();
    }

    /// The in-match gating: the RTS view never auto-opens the pick just because the General is
    /// unclassed; the command bar's force opens it; Escape closes it and keeps playing.
    #[test]
    fn rts_does_not_auto_open_but_the_force_does_and_escape_closes() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();

        // No take-control, no class, overhead view: the screen must stay closed.
        let out = client.frame(&mut sim, &inp);
        assert_eq!(client.view, ViewMode::Rts);
        assert_eq!(out.menu_open, 0, "an unclassed RTS player keeps playing");
        assert!(!client.class_pick_open);

        // The command bar's Create a Class one-shot opens it over the RTS view.
        inp.class_pick_force = 1;
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.menu_open, 1);
        assert!(client.class_pick_open);
        // It stays up while the force flag is gone (the latch), until Escape.
        inp.class_pick_force = 0;
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.menu_open, 1);

        inp.keys.set_down(Key::Escape, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Escape, false);
        let out2 = client.frame(&mut sim, &inp);
        assert_eq!(out.menu_open, 0, "Escape closes the forced screen");
        assert_eq!(out2.menu_open, 0);
        assert!(!client.class_pick_forced, "the force latch cleared");
        // The unclassed RTS player is not re-offered it.
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 5);
        assert_eq!(out.menu_open, 0);
        crate::profile::reset();
    }

    /// The in-match panel opens as a popup, so `CoD.Menu.animateIn`'s 250 ms fade runs once per
    /// open (`codmenu.lua:2300-2326`) and starts over after Escape closes it.
    #[test]
    fn the_in_match_panel_fades_in_and_a_reopen_starts_over() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { dt: 1.0 / 60.0, ..Default::default() };

        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.menu_open, 0);
        assert!(client.class_screen_view().open.is_none(), "no screen, no entrance");
        // The command bar's force opens it over the RTS view.
        inp.class_pick_force = 1;
        client.frame(&mut sim, &inp);
        let open = client.class_screen_view().open.expect("the panel's open starts the fade");
        assert!((0.0..crate::classui::MENU_OPEN_MS / 1000.0).contains(&open), "just opened: {open}");
        inp.class_pick_force = 0;
        for _ in 0..16 {
            client.frame(&mut sim, &inp);
        }
        assert!(client.class_screen_view().open.is_none(), "settled after 250 ms");
        // Escape closes it; the next forced open is a fresh menu and fades again.
        inp.keys.set_down(Key::Escape, true);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Escape, false);
        client.frame(&mut sim, &inp);
        assert!(!client.class_pick_open, "Escape closed the panel");
        assert!(client.class_screen_view().open.is_none());
        inp.class_pick_force = 1;
        client.frame(&mut sim, &inp);
        assert!(client.class_pick_open, "the force reopens it");
        assert!(client.class_screen_view().open.is_some(), "the reopen fades again");
        crate::profile::reset();
    }

    /// First person: an unclassed take-control puts the spawn's class screen up, and it cannot be
    /// played around. Escape declines the spawn (BO2: `back` leaves the player unspawned): the
    /// screen closes, the view holds still with nothing in hand, only the leave press goes out, and
    /// the sim hands the still-unclassed General back overhead. The next take-control puts the
    /// screen up again.
    #[test]
    fn fps_unclassed_escape_declines_the_spawn_and_returns_overhead() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);

        // Reach first person: the unclassed General has the screen up, cursor free, no gun.
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert_eq!(out.menu_open, 1, "the spawn's class screen is up");
        assert_eq!(out.capture_mouse, 0, "the cursor is free for the screen");
        assert_eq!(out.viewmodel, viewmodel::NONE, "nothing in hand before the class spawns him");
        let start = host.get(1).pos;

        // Escape, with W and the mouse held: the screen closes, the view does not turn, nothing
        // but the leave press reaches the sim, and he goes back overhead unclassed.
        inp.keys.set_down(Key::Escape, true);
        inp.keys.set_down(Key::W, true);
        inp.mouse_dx = 50.0;
        let yaw = client.yaw;
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.menu_open, 0, "Escape closed the screen");
        assert!(client.class_pick_declined);
        inp.keys.set_down(Key::Escape, false);
        let mut sent = Vec::new();
        let mut frames = 0;
        while client.view != ViewMode::Rts && frames < 200 {
            let out = client.frame(&mut sim, &inp);
            assert_eq!(out.menu_open, 0, "the screen stays shut while he leaves");
            assert_eq!(out.capture_mouse, 0);
            assert_eq!(out.viewmodel, viewmodel::NONE);
            if out.send_input != 0 {
                sent.push(out.input);
                sim.input(inp.local_player, out.input_general, out.input);
            }
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
            inp.dt = LOGIC_DT;
            frames += 1;
        }
        assert_eq!(client.view, ViewMode::Rts, "back overhead");
        assert_eq!(client.yaw, yaw, "the view held still");
        assert!(!sent.is_empty() && sent[0].pressed == buttons::FIELD_COMPUTER, "the leave press: {sent:?}");
        assert!(sent.iter().all(|i| i.held == 0 && i.move_packed == pack_move(0.0, 0.0)), "no movement: {sent:?}");
        assert!((host.get(1).pos - start).length_xy() < 1e-3, "he never walked");
        let p = sim.player(0).unwrap();
        assert!(!p.is_first_person() && p.class.is_none(), "overhead, still unclassed");
        assert!(host.controlled.borrow().is_empty());
        inp.keys.set_down(Key::W, false);
        inp.mouse_dx = 0.0;

        // Take control again: the screen is up again.
        sim.take_control(&host, 0, 1);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert!(!client.class_pick_declined);
        assert_eq!(out.menu_open, 1, "the next take-control puts the screen up again");
        assert_eq!(out.capture_mouse, 0);
        crate::profile::reset();
    }

    /// Every way into first person: Take Control on a fresh General meets the screen (Escape sends
    /// him back overhead, selected again); a class from the command bar overhead spawns him
    /// straight into play; Escape on a classed General's re-open keeps playing (BO2's mid-life
    /// Back); and after his death the next General meets the screen again (every General is picked
    /// for: the deliberate break from BO2's `pers["class"]` respawn), opened on the last class so
    /// Enter re-picks it, the dead one never selected.
    #[test]
    fn every_entry_meets_the_screen_only_without_a_class_and_every_general_picks() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        // Run until overhead; the General selected on the way, if one was.
        let until_overhead = |client: &mut Client, sim: &mut Sim, inp: &mut ClientIn| {
            let mut selected = Vec::new();
            let mut n = 0;
            while client.view != ViewMode::Rts && n < 200 {
                let out = frame_with(client, sim, &host, inp, 1);
                if out.select_object != 0 {
                    selected.push(out.select_object);
                }
                n += 1;
            }
            selected
        };
        let escape = |client: &mut Client, sim: &mut Sim, inp: &mut ClientIn| {
            inp.keys.set_down(Key::Escape, true);
            frame_with(client, sim, &host, inp, 1);
            inp.keys.set_down(Key::Escape, false);
            until_overhead(client, sim, inp)
        };
        // A fresh General.
        sim.take_control(&host, 0, 1);
        assert_eq!(frame_with(&mut client, &mut sim, &host, &mut inp, 60).menu_open, 1);
        let selected = escape(&mut client, &mut sim, &mut inp);
        assert_eq!(client.view, ViewMode::Rts);
        assert_eq!(selected, [1], "back overhead, selected once, so his class bar is up");
        assert!(sim.player(0).unwrap().class.is_none());
        // His class from the command bar overhead (button 1: the first preset), then Take Control.
        inp.class_button = 1;
        let out = client.frame(&mut sim, &inp);
        inp.class_button = 0;
        assert_eq!(out.menu_open, 0, "the bar picks without the screen");
        sim.choose_class(0, out.choose_class_general, (out.choose_class - 1) as usize, &[]);
        sim.take_control(&host, 0, 1);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!((out.menu_open, out.capture_mouse), (0, 1), "a classed General plays at once");
        assert_ne!(out.viewmodel, viewmodel::NONE, "his gun is up");
        // A classed General's forced re-open: Escape keeps playing.
        inp.class_pick_force = 1;
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        inp.class_pick_force = 0;
        inp.keys.set_down(Key::Escape, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        inp.keys.set_down(Key::Escape, false);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 30);
        assert_eq!(client.view, ViewMode::Fps, "a classed Back stays in first person");
        assert_eq!((out.menu_open, out.capture_mouse), (0, 1));
        // He dies: back overhead, nobody selected.
        host.objects.borrow_mut()[0].flags |= crate::host::objflags::DEAD;
        let selected = until_overhead(&mut client, &mut sim, &mut inp);
        assert!(sim.player(0).is_none());
        assert!(selected.is_empty(), "a dead General is not selected: {selected:?}");
        // The next General is picked for (every General is, `sim/spawn.rs`): the screen is up,
        // opened on the player's last class, and Enter re-picks it in one press.
        host.add(general(2, 0));
        sim.take_control(&host, 0, 2);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!((client.view, out.menu_open, out.capture_mouse), (ViewMode::Fps, 1, 0), "the screen for a new General");
        assert_eq!(out.viewmodel, viewmodel::NONE, "no gun before his pick");
        let last = crate::classes::at(0).unwrap().id;
        let row = crate::classui::pick_rows(false).iter().position(|r| *r == crate::classui::PickRow::Preset(0)).expect("the preset's row");
        assert_eq!(client.class_pick_highlight, row, "opened on the last class, not row 0");
        inp.keys.set_down(Key::Enter, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Enter, false);
        assert_eq!((out.choose_class, out.choose_class_general), (1, 2), "Enter picks the pre-selected class for General 2");
        sim.choose_class(0, out.choose_class_general, (out.choose_class - 1) as usize, &[]);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!((client.view, out.menu_open, out.capture_mouse), (ViewMode::Fps, 0, 1), "playing after the pick");
        assert_ne!(out.viewmodel, viewmodel::NONE, "his gun is up");
        assert_eq!(sim.player(0).unwrap().class.as_deref(), Some(last.as_str()));
        // The field computer hands him back selected.
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        assert_eq!(until_overhead(&mut client, &mut sim, &mut inp), [2]);
        crate::profile::reset();
    }

    #[test]
    fn hor_plus_fov() {
        let f43 = horizontal_fov(65.0, 4.0 / 3.0).to_degrees();
        let f169 = horizontal_fov(65.0, 16.0 / 9.0).to_degrees();
        assert!((f43 - 65.0).abs() < 0.01);
        assert!(f169 > 80.0 && f169 < 82.0, "{f169}");
    }

    /// The `ZHBO_AUTOPILOT=classpick` script exercises the pick-only flow: it Escapes the first
    /// spawn screen (the classless General goes back overhead, never playing the knife), then on
    /// the next take-control picks the seeded custom class by its row key (5 presets + AB), leaves
    /// first person, and after the host takes control again forces the screen open and Escapes it,
    /// then plays on. Then the host kills him and stands a new General in his place: every General
    /// is picked for, so the fourth session meets the spawn screen, opened on AB, and the script's
    /// Enter re-picks it; he walks. A pick never writes the profile.
    #[cfg(feature = "autopilot")]
    #[test]
    fn the_classpick_autopilot_picks_the_seeded_class_and_escapes_a_second_offer() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        // Deliberately not the stock kit: the primary is the pistol and the lethal a claymore.
        crate::profile::load("AB\nprimary=mp7_mp;secondary=an94_mp;lethal=claymore_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Toughness,FastHands,Dexterity");
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client { autopilot: Some(Autopilot { classpick: true, ..Default::default() }), ..Default::default() };
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);

        // Run from the entrance: the script's first action fires 0.3 s into first person, so the
        // capture loop must start before the transition completes.
        let mut reached_fps = false;
        let mut pick: Option<ClientOut> = None;
        let mut any_save = false;
        let mut retakes = 0;
        let mut overhead_since = None;
        let mut declined_unclassed = false;
        let mut reopened = false;
        let mut closed_after_escape = false;
        let mut seat_general = 1;
        let mut third_since = None;
        let mut fourth_screen = false;
        let mut fourth_pick: Option<ClientOut> = None;
        let mut fourth_start = None;
        for i in 0..(30 * 45) {
            // The C++ autopilot takes control again (three times for this run) a while after the
            // General is handed back; the fake host does the same when the script has left first
            // person. In the third session it kills him and stands General 2 in his place.
            if client.view == ViewMode::Rts && i > 60 {
                let since = *overhead_since.get_or_insert(i);
                if retakes < 3 && i - since > 30 && (retakes < 2 || seat_general == 2) {
                    if retakes == 0 {
                        let p = sim.player(0).unwrap();
                        declined_unclassed = p.class.is_none() && !p.is_first_person();
                    }
                    sim.take_control(&host, 0, seat_general);
                    retakes += 1;
                    overhead_since = None;
                }
            }
            if retakes == 2 && seat_general == 1 && sim.player(0).is_some_and(|p| p.is_first_person()) {
                let since = *third_since.get_or_insert(i);
                if i - since > 180 {
                    host.objects.borrow_mut()[0].flags |= crate::host::objflags::DEAD;
                    host.add(crate::host::ObjInfo { pos: host.get(1).pos, ..general(2, 0) });
                    seat_general = 2;
                }
            }
            if retakes == 3 && client.view == ViewMode::Fps {
                if client.class_pick_open && fourth_pick.is_none() {
                    fourth_screen = true;
                    let row = crate::classui::pick_rows(false).iter().position(|r| *r == crate::classui::PickRow::Custom(0)).unwrap();
                    assert_eq!(client.class_pick_highlight, row, "General 2's screen opens on the last class");
                }
                if fourth_start.is_none() && sim.player(0).is_some_and(|p| p.class.is_some()) {
                    fourth_start = Some(host.get(2).pos);
                }
            }
            let out = client.frame(&mut sim, &inp);
            if client.view == ViewMode::Fps {
                reached_fps = true;
            }
            if out.save_profile != 0 {
                any_save = true;
            }
            if out.choose_class != 0 {
                // Deliver the pick like the bridge does: the sim then assumes the class, so the
                // spawn offer closes and the field computer can reach the game.
                if out.choose_class == CHOOSE_CLASS_CUSTOM {
                    let words = &out.choose_class_payload[..out.choose_class_payload_len as usize];
                    sim.choose_class(0, out.choose_class_general, usize::MAX, words);
                } else if out.choose_class > 0 {
                    sim.choose_class(0, out.choose_class_general, (out.choose_class - 1) as usize, &[]);
                }
                if pick.is_none() {
                    pick = Some(out);
                } else if retakes == 3 {
                    fourth_pick = Some(out);
                }
            }
            // The third session's forced re-open (the second session's screen is the pick's).
            if retakes == 2 && out.menu_open != 0 {
                reopened = true;
            }
            if reopened && out.menu_open == 0 && matches!(client.view, ViewMode::Fps) {
                closed_after_escape = true;
            }
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            inp.dt = LOGIC_DT;
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        assert!(reached_fps, "the script runs in first person");
        assert!(declined_unclassed, "the first session's Escape sent the classless General back overhead");
        let out = pick.expect("the script picks the seeded class");
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        let class = crate::classes::CustomClass::from_wire_words(&words).expect("the payload reassembles");
        assert_eq!(class.name, "AB", "the seeded custom is picked by name");
        // The kit, not just the name: this is what the classpick autotest now greps for too.
        let a = &sim.player(0).expect("the General").arsenal;
        assert_eq!(a.weapons[0].main.id, "mp7_mp", "the seeded primary landed");
        assert_eq!(a.lethal.def.id, "claymore_mp", "the seeded lethal landed");
        assert!(a.has(crate::weapons::Perk::TOUGHNESS));
        assert!(reopened, "the script forces the screen open after the next take-control");
        assert!(closed_after_escape, "Escape closes it and first-person play continues");
        assert!(!any_save, "a pick must not write the profile");
        assert_eq!(crate::profile::classes().len(), 1, "the seeded profile is untouched");
        // The fourth session, General 2: the screen, the one-press re-pick, the kit, the walk.
        assert_eq!(retakes, 3, "General 2 was taken into first person");
        assert!(fourth_screen, "General 2 met the spawn screen");
        let out = fourth_pick.expect("the script re-picked for General 2");
        assert_eq!((out.choose_class, out.choose_class_general), (CHOOSE_CLASS_CUSTOM, 2));
        let p = sim.player(0).expect("General 2's record");
        assert_eq!((p.general, p.class.as_deref()), (2, Some("custom:AB")), "he wears the re-pick");
        assert_eq!(p.arsenal.weapons[0].main.id, "mp7_mp");
        let start = fourth_start.expect("he was classed in first person");
        assert!((host.get(2).pos - start).length_xy() > 10.0, "he walked after the pick");
        crate::profile::reset();
    }

    /// The pick screen's number keys follow the rows (key 1 is row 1), and picking a saved custom
    /// sends its payload without touching the profile. While the screen is up the cursor is free
    /// and no first-person input is sent.
    #[test]
    fn number_keys_pick_rows_and_a_profile_re_pick_does_not_save() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::save(crate::classes::CustomClass {
            name: "AB".into(),
            primary: "an94_mp".into(),
            ..crate::classes::CustomClass::default()
        });
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert_eq!(out.menu_open, 1, "the pick screen is up (no class yet)");
        assert_eq!(out.capture_mouse, 0, "the cursor is free for the screen");

        // The match list is the set's five custom slots (AB, then four untouched), then the
        // default classes in `changeclass.lua`'s order (SMG, CQB, Assault, LMG, Sniper): key 6 is
        // the SMG preset, a 1-based index pick (the bridge subtracts one).
        let rows = crate::classui::pick_rows(false);
        assert_eq!(rows[0], crate::classui::PickRow::Custom(0));
        assert_eq!(rows[5], crate::classui::PickRow::Preset(1), "SMG leads the defaults");
        inp.keys.set_down(Key::Key6, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Key6, false);
        client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, 2, "SMG is registry index 1, sent 1-based");
        assert_eq!(out.choose_class_payload_len, 0);
        assert_eq!(out.save_profile, 0, "a pick never writes the profile");

        // Key 1 is the first row: the AB custom, sent as its wire words.
        inp.keys.set_down(Key::Key1, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Key1, false);
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        assert_eq!(crate::classes::CustomClass::from_wire_words(&words).unwrap().name, "AB");
        assert_eq!(out.save_profile, 0, "a profile re-pick must not re-save");

        // Once the pick lands the screen closes: capture returns and the menu flag clears.
        sim.choose_class(0, 1, usize::MAX, &words);
        let f = sim.frame + 1;
        sim.step(&host, f);
        inp.logic_frame = f;
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert_eq!(out.menu_open, 0);
        assert_eq!(out.capture_mouse, 1);
        crate::profile::reset();
    }

    /// Bug 3 regression: a profile custom with a *distinctive* kit must arrive at the sim as that
    /// kit, not the stock one. The classpick autotest seeded a custom that happened to be the
    /// Assault kit, so a pick that never applied the kit still passed the name-only grep. This
    /// drives the real client build of the payload (row key -> `ClientOut` -> wire words) and the
    /// real sim decode, then asserts every arsenal slot, the ammo, and the perks.
    #[test]
    fn a_profile_custom_pick_applies_its_whole_kit() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let custom = crate::classes::CustomClass {
            name: "Loud Kit".into(),
            // Deliberately unlike the stock kit: an SMG primary, the rifle as an Overkill
            // secondary, a claymore, a shorter magazine reserve, and no stock perks.
            primary: "mp7_mp".into(),
            secondary: "an94_mp".into(),
            lethal: "claymore_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            lethal_count: 1,
            tactical_count: 1,
            melee: "knife_held_mp".into(),
            perks: vec!["Toughness".into(), "FlakJacket".into()],
            reserve: [40, 20],
            camo: String::new(),
            reticle: String::new(),
            reticle_color: String::new(),
            knife: String::new(),
            secondary_camo: String::new(),
            secondary_reticle: String::new(),
        };
        // Seed it the way the shell writes it: through the profile file's text.
        crate::profile::load(&format!("{}\n", custom.to_wire()));

        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert!(client.class_pick_open, "the unclassed spawn offers the pick");

        // Press the custom's row key (the row after the presets).
        let rows = crate::classui::pick_rows(false);
        let row = rows.iter().position(|r| matches!(r, crate::classui::PickRow::Custom(0))).expect("the custom row");
        let key = crate::classui::pick_key(row).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM, "the client sends the custom payload");
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        assert_eq!(crate::classes::CustomClass::from_wire_words(&words), Some(custom.clone()), "the wire round-trips every slot");

        // Deliver it exactly as the bridge does: `[len, word...]` into the sim.
        sim.choose_class(0, out.choose_class_general, usize::MAX, &words);
        let f = sim.frame + 1;
        sim.step(&host, f);

        let p = sim.player(0).expect("the General is in first person");
        assert_eq!(p.class.as_deref(), Some("custom:Loud Kit"));
        let a = &p.arsenal;
        assert_eq!(a.weapons.len(), 2, "both guns came from the class");
        assert_eq!(a.weapons[0].main.id, "mp7_mp", "primary is the class's, not the stock AN-94");
        assert_eq!(a.weapons[1].main.id, "an94_mp", "secondary is the class's");
        assert_eq!(a.weapons[0].reserve, 40, "the class's primary reserve");
        assert_eq!(a.weapons[1].reserve, 20, "the class's secondary reserve");
        assert_eq!(a.lethal.def.id, "claymore_mp", "lethal is the class's");
        assert_eq!(a.tactical.def.id, "concussion_grenade_mp");
        assert_eq!(a.melee.id, "knife_held_mp");
        assert!(a.has(crate::weapons::Perk::TOUGHNESS), "the class's perks");
        assert!(a.has(crate::weapons::Perk::FLAK_JACKET), "the class's perks");
        assert!(!a.has(crate::weapons::Perk::LIGHTWEIGHT), "no stock perks leaked through");
        crate::profile::reset();
    }

    /// Dual wield is a weapon of its own (G16): a class whose pistol carries `dw` spawns holding
    /// the `_dw_mp` weapon, not the pistol with a `dw` attachment. The class is seeded the way an
    /// older profile stored it (`fiveseven_mp+dw`), so this also proves the load-time migration.
    #[test]
    fn a_dual_wield_class_spawns_with_the_dw_weapon() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let legacy = crate::classes::CustomClass {
            name: "Akimbo".into(),
            primary: "mp7_mp".into(),
            secondary: "fiveseven_mp+dw".into(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            ..Default::default()
        };
        crate::profile::load(&format!("{}\n", legacy.to_wire()));
        let stored = crate::profile::classes();
        assert_eq!(stored[0].secondary, "fiveseven_dw_mp", "the profile migrates to BO2's weapon");

        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let rows = crate::classui::pick_rows(false);
        let row = rows.iter().position(|r| matches!(r, crate::classui::PickRow::Custom(0))).expect("the custom row");
        let key = crate::classui::pick_key(row).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        client.frame(&mut sim, &inp);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        sim.choose_class(0, out.choose_class_general, usize::MAX, &words);
        let f = sim.frame + 1;
        sim.step(&host, f);

        let a = &sim.player(0).expect("the General is in first person").arsenal;
        assert_eq!(a.weapons[1].main.id, "fiveseven_dw_mp", "the secondary is the dual-wield weapon");
        assert_eq!(a.weapons[1].main.dual_wield_weapon, "fiveseven_lh_mp", "its def names the left gun");

        // Through the event stream: swap to the pair, then the ADS button fires the left gun from
        // its own magazine (BO2's left trigger) and the attack button the right gun.
        let send = |sim: &mut Sim, held: u32, pressed: u32| {
            sim.events.clear();
            sim.input(0, 1, crate::input::FpsInput { held, pressed, ..Default::default() });
            let f = sim.frame + 1;
            sim.step(&host, f);
            sim.events.iter().filter_map(|e| if let crate::sim::EventKind::Shot { left, .. } = e.kind { Some(left) } else { None }).collect::<Vec<bool>>()
        };
        send(&mut sim, 0, buttons::SWAP_WEAPON);
        for _ in 0..120 {
            send(&mut sim, 0, 0);
        }
        let full = sim.player(0).unwrap().arsenal.current().ammo;
        assert_eq!(sim.player(0).unwrap().arsenal.current().left_ammo(), Some(full));
        assert_eq!(send(&mut sim, buttons::ADS, buttons::ADS), vec![true], "the ADS button fires the left gun");
        for _ in 0..30 {
            send(&mut sim, 0, 0);
        }
        assert_eq!(send(&mut sim, buttons::FIRE, buttons::FIRE), vec![false], "the attack button fires the right gun");
        let w = sim.player(0).unwrap().arsenal.current();
        assert_eq!((w.ammo, w.left_ammo()), (full - 1, Some(full - 1)), "one round from each gun's own magazine");
        // (Other tests share the unresolved list; only this class's ids matter here.)
        let missing = crate::weapons::catalog::unresolved();
        assert!(!missing.iter().any(|id| id.contains("fiveseven")), "{missing:?}");
        crate::profile::reset();
    }

    /// BO2's changeclass focuses the class the player is wearing, so the in-match panel opens on
    /// the equipped class rather than on the first row (the command bar's entry can be pressed at
    /// any time, and landing on row 1 would make the player think they wear the default).
    #[test]
    fn the_in_match_panel_opens_on_the_class_the_general_is_wearing() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::save(crate::classes::CustomClass { name: "First".into(), ..Default::default() });
        crate::profile::save(crate::classes::CustomClass { name: "Second".into(), primary: "mp7_mp".into(), ..Default::default() });
        let host = FakeHost::default();
        host.add(general(7, 0));
        let mut sim = Sim::new();
        sim.rts_update(&host, 0, 7, 0, [0, 0, 0], None);
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, class_pick_force: 1, ..Default::default() };
        for _ in 0..2 {
            client.frame(&mut sim, &inp);
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        assert!(client.class_pick_open, "the command bar opened the panel");
        // Pick the SECOND custom by its row key; deliver the one-shot as the bridge does.
        let row = crate::classui::pick_rows(false)
            .iter()
            .position(|r| *r == crate::classui::PickRow::Custom(1))
            .expect("the second custom's row");
        let key = crate::classui::pick_key(row).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        client.frame(&mut sim, &inp);
        let words = &out.choose_class_payload[..out.choose_class_payload_len as usize];
        sim.choose_class(0, out.choose_class_general, usize::MAX, words);
        assert_eq!(sim.player(0).expect("the record").class.as_deref(), Some("custom:Second"));
        // Close the panel, then force it open again: it focuses the class now worn.
        inp.class_pick_force = 0;
        for _ in 0..2 {
            client.frame(&mut sim, &inp);
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        assert!(!client.class_pick_open, "a classed General's panel is closed");
        inp.class_pick_force = 1;
        for _ in 0..2 {
            client.frame(&mut sim, &inp);
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        assert!(client.class_pick_open);
        assert_eq!(client.class_pick_highlight, row, "the panel opens on the worn class, not row 0");
        crate::profile::reset();
    }

    /// Bug 3's other half: a class picked from the command bar while the General is still under
    /// RTS control must target *that* General. The client only learned the General's id on entering
    /// first person, so a fresh General's overhead pick went to id 0 and the sim dropped it — the
    /// screen closed and the General kept the stock kit. The sim's own record (made by
    /// `rts_update` every logic frame) knows the id; the pick must use it.
    #[test]
    fn an_overhead_custom_pick_targets_the_general_under_command() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::save(crate::classes::CustomClass {
            name: "Overhead Kit".into(),
            primary: "mp7_mp".into(),
            secondary: "an94_mp".into(),
            lethal: "claymore_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: vec!["Toughness".into()],
            ..Default::default()
        });
        let host = FakeHost::default();
        host.add(general(7, 0));
        let mut sim = Sim::new();
        // The bridge's `rtsWeapons` call every logic frame: this is the record the client can see.
        sim.rts_update(&host, 0, 7, 0, [0, 0, 0], None);
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, class_pick_force: 1, ..Default::default() };
        for _ in 0..2 {
            client.frame(&mut sim, &inp);
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        assert!(client.class_pick_open, "the command bar's button opens the pick overhead");
        let rows = crate::classui::pick_rows(false);
        let row = rows.iter().position(|r| matches!(r, crate::classui::PickRow::Custom(0))).expect("the custom row");
        let key = crate::classui::pick_key(row).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        sim.choose_class(0, out.choose_class_general, usize::MAX, &words);
        let a = &sim.player(0).expect("the record").arsenal;
        assert_eq!(a.weapons[0].main.id, "mp7_mp", "the overhead pick applied the custom kit");
        assert_eq!(a.lethal.def.id, "claymore_mp");
        assert_eq!(out.choose_class_general, 7, "the pick names the General under command, not 0");
        crate::profile::reset();
    }

    /// The command bar's class buttons ride `ClientIn::class_button`: the engine dispatches a bar
    /// click at a different point in the frame from the client's, so the bridge hands the button
    /// over instead of queueing a pick itself (that queue was written over by the next frame's
    /// fresh `ClientOut` before the bridge read it, so the class never landed — the reported bug).
    /// The client maps the button to the same one-shot the pick screen sends, names the General
    /// under command, and the pick applies on the next logic frame.
    #[test]
    fn a_command_bar_class_button_picks_a_preset_and_the_sim_applies_it() {
        let host = FakeHost::default();
        host.add(general(7, 0));
        let mut sim = Sim::new();
        // The bridge's `rtsWeapons` call every logic frame: the record the client adopts.
        sim.rts_update(&host, 0, 7, 0, [0, 0, 0], None);
        let mut client = Client::default();
        // Button 1 is the first preset; the overhead view (no take-control yet) is where the bar
        // lives.
        let mut inp = ClientIn { class_button: 1, ..Default::default() };

        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, 1, "the first preset, 1-based");
        assert_eq!(out.choose_class_general, 7, "the General under command, not 0");
        assert_eq!(out.choose_class_payload_len, 0, "a preset is an index pick");
        assert_eq!(out.menu_open, 0, "the command bar picks without the overlay screen");
        assert!(!client.class_pick_open);
        assert!(sim.player(0).unwrap().class.is_none(), "nothing applied before the message is dispatched");

        // The bridge turns the one-shot into MSG_ZHBO_CHOOSE_CLASS; the sim dispatches it on its
        // next logic frame.
        sim.choose_class(0, out.choose_class_general, (out.choose_class - 1) as usize, &[]);
        let f = sim.frame + 1;
        sim.step(&host, f);
        inp.logic_frame = f;
        inp.class_button = 0;
        let p = sim.player(0).expect("the record");
        assert_eq!(p.class.as_deref(), Some("assault"));
        assert_eq!(p.arsenal.weapons[0].main.id, "tar21_mp", "the preset's kit landed");
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, 0, "the one-shot is consumed, not repeated");
    }

    /// The bar's custom buttons: the client packs the profile custom's wire words itself (the
    /// same payload the pick screen builds), so the bridge's cached rows can never send a stale
    /// class. The kit lands through the real sim decode; an empty custom slot sends nothing.
    #[test]
    fn a_command_bar_class_button_picks_a_custom_and_an_empty_slot_sends_nothing() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let custom = crate::classes::CustomClass {
            name: "Bar Kit".into(),
            primary: "mp7_mp".into(),
            secondary: "an94_mp".into(),
            lethal: "claymore_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: vec!["Toughness".into()],
            ..Default::default()
        };
        crate::profile::save(custom.clone());
        let host = FakeHost::default();
        host.add(general(7, 0));
        let mut sim = Sim::new();
        sim.rts_update(&host, 0, 7, 0, [0, 0, 0], None);
        let mut client = Client::default();
        // 1-based: the presets come first, so the first custom is button preset_rows() + 1.
        let custom_button = (crate::classicons::preset_rows() + 1) as i32;
        let mut inp = ClientIn { class_button: custom_button, ..Default::default() };

        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM);
        assert_eq!(out.choose_class_general, 7);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        assert_eq!(crate::classes::CustomClass::from_wire_words(&words), Some(custom.clone()));

        sim.choose_class(0, out.choose_class_general, usize::MAX, &words);
        let f = sim.frame + 1;
        sim.step(&host, f);
        inp.logic_frame = f;
        let p = sim.player(0).expect("the record");
        assert_eq!(p.class.as_deref(), Some("custom:Bar Kit"));
        assert_eq!(p.arsenal.weapons[0].main.id, "mp7_mp", "the custom's primary landed");

        // The next custom slot is empty: pressing its button must send nothing at all.
        inp.class_button = custom_button + 1;
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, 0);
        assert_eq!(out.choose_class_payload_len, 0);
        crate::profile::reset();
    }

    /// Every class the editor can build is pickable (`classmenu::kit_coverage`: every legal value
    /// of every field with the largest of the others), from the command bar's button: the client
    /// sends it whole and the sim applies exactly that class. The playtest bug was a personalised
    /// class with an attachment on each gun whose payload outgrew the ABI's 64 words, so the pick
    /// was refused with a dry-fire click.
    #[test]
    fn every_legal_custom_class_is_picked_and_applied() {
        let _g = crate::profile::lock();
        let custom_button = (crate::classicons::preset_rows() + 1) as i32;
        let mut n = 0;
        crate::classmenu::kit_coverage(|class| {
            n += 1;
            crate::profile::reset();
            crate::profile::save(class.clone());
            // The profile names an unnamed class (`Custom 1`); that is the class sent.
            let class = &crate::profile::classes()[0];
            let host = FakeHost::default();
            host.add(general(7, 0));
            let mut sim = Sim::new();
            sim.rts_update(&host, 0, 7, 0, [0, 0, 0], None);
            let mut client = Client::default();
            let out = client.frame(&mut sim, &ClientIn { class_button: custom_button, ..Default::default() });
            assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM, "{class:?} was not sent");
            let words = &out.choose_class_payload[..out.choose_class_payload_len as usize];
            sim.choose_class(0, out.choose_class_general, usize::MAX, words);
            let p = sim.player(0).expect("the record");
            assert_eq!(p.class.as_deref(), Some(format!("custom:{}", class.name).as_str()), "{class:?}");
            assert_eq!(p.camo, class.camo);
            assert_eq!(p.secondary_camo, class.secondary_camo);
            assert_eq!(p.knife_camo, class.knife);
            assert_eq!(p.reticle, class.reticle);
            assert_eq!(p.reticle_color, class.reticle_color);
            let kit = class.loadout();
            assert_eq!(p.arsenal.weapons[0].main.id, crate::weapons::weapon(&kit.primary).id, "{class:?}");
            assert_eq!(p.arsenal.weapons[1].main.id, crate::weapons::weapon(&kit.secondary).id, "{class:?}");
        });
        assert!(n > 50);
        crate::profile::reset();
    }

    /// The playtest report, as played: the second custom class of the set — an attachment on each
    /// gun, gold camo, a gold knife and an Art of War secondary — picked from the class screen's
    /// row. The player's class (`tar21_mp+rangefinder`, `crossbow_mp+ir`) needs the installed
    /// table (zhbo-assets' `every_legal_class_fits_the_pick_payload` replays it); this one is
    /// the same shape over the stand-in table, and its text is as far past the payload. It lands;
    /// before the token payload it was refused with a dry-fire click.
    #[test]
    fn the_playtest_second_custom_class_is_picked_from_the_class_screen() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::load(
            "v2\nactive 1\nset 1 CLASS SET 1\nclass\nSC\n\
             primary=svu_mp;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;lethal_count=1;\
             tactical_count=1;melee=knife_held_mp;perks=Toughness,FastHands,Dexterity\nend\nclass\nCustom 2\n\
             primary=tar21_mp+reflex;secondary=fiveseven_dw_mp;lethal=satchel_charge_mp;tactical=concussion_grenade_mp;\
             lethal_count=1;tactical_count=1;melee=knife_held_mp;perks=Lightweight,Toughness,Dexterity;camo=camo_gold;\
             knife=camo_gold;secondary_camo=camo_artofwar\nend\n",
        );
        let second = crate::profile::classes()[1].clone();
        assert_eq!(second.name, "Custom 2");
        assert_eq!(second.secondary_camo, "camo_artofwar", "the class loads whole");
        assert!(second.to_wire().len() > 64 * 4, "its text is past the payload: {} bytes", second.to_wire().len());
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert!(client.class_pick_open);
        let row = crate::classui::pick_rows(false)
            .iter()
            .position(|r| matches!(r, crate::classui::PickRow::Custom(1)))
            .expect("the second custom is listed");
        let key = crate::classui::pick_key(row).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM, "the pick was sent");
        let words = &out.choose_class_payload[..out.choose_class_payload_len as usize];
        assert_eq!(crate::classes::CustomClass::from_wire_words(words), Some(second.clone()));
        sim.choose_class(0, out.choose_class_general, usize::MAX, words);
        let p = sim.player(0).unwrap();
        assert_eq!(p.class.as_deref(), Some("custom:Custom 2"));
        assert_eq!(p.camo, "camo_gold");
        assert_eq!(p.secondary_camo, "camo_artofwar");
        crate::profile::reset();
    }

    /// The standard scripted run starts on an unarmed General, and the autotest seeds the profile
    /// with one custom class first (`scripts/autotest-mac.sh`): the AN-94 with its under-barrel
    /// grenade launcher. BO2's premades carry no attachments, so without that seed the script's
    /// alt-mode step has nothing to toggle (the "grenade launcher" check). The run picks the
    /// custom by its computed row key — the presets, then the custom — and the kit, not just the
    /// name, must land: the script fires, melees and throws the frag and concussion that follow.
    #[cfg(feature = "autopilot")]
    #[test]
    fn the_standard_autopilot_picks_the_seeded_gl_custom_only_while_the_spawn_pick_is_up() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        // The script's seed, through the v2 file format the bridge reads.
        crate::profile::load(
            "v2\nactive 1\nset 1 CLASS SET 1\nclass\nGL Test\n\
             primary=an94_mp+gl;secondary=fnp45_mp;lethal=frag_grenade_mp;\
             tactical=concussion_grenade_mp;melee=knife_held_mp;\
             perks=Lightweight,FastHands,Dexterity\nend\n",
        );
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client { autopilot: Some(Autopilot::default()), ..Default::default() };
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        let mut pick: Option<ClientOut> = None;
        for _ in 0..(30 * 3) {
            let out = client.frame(&mut sim, &inp);
            if out.choose_class != 0 && pick.is_none() {
                pick = Some(out);
                // Deliver it as the bridge does: a custom carries its wire words.
                let words = &out.choose_class_payload[..out.choose_class_payload_len as usize];
                sim.choose_class(0, out.choose_class_general, usize::MAX, words);
            }
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            inp.dt = LOGIC_DT;
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        let pick = pick.expect("the standard run picks a class at spawn");
        assert_eq!(pick.choose_class, CHOOSE_CLASS_CUSTOM, "the seeded custom, not a premade");
        let words: Vec<u32> = pick.choose_class_payload[..pick.choose_class_payload_len as usize].to_vec();
        let class = crate::classes::CustomClass::from_wire_words(&words).expect("the payload reassembles");
        assert_eq!(class.name, "GL Test", "the seeded custom is picked by row key");
        // The row is derived from the live list: the set's custom slots lead the match list
        // (`changeclass.lua`), then the defaults, so the seeded custom is row 1.
        let rows = crate::classui::pick_rows(false);
        assert_eq!(
            rows.iter().position(|r| matches!(r, crate::classui::PickRow::Custom(0))),
            Some(0),
            "the custom leads the match list"
        );
        assert!(rows[1..].iter().take(4).all(|r| matches!(r, crate::classui::PickRow::Empty(_))), "the set's untouched slots follow");
        let a = &sim.player(0).unwrap().arsenal;
        assert!(!a.is_unarmed(), "the class armed the General");
        assert_eq!(a.weapons[0].main.id, "an94_mp+gl", "the seeded primary (with its launcher) landed");
        assert_eq!(a.weapons[0].main.alt, "gl_an94_mp", "its alt mode is the grenade launcher");
        assert_eq!(a.weapons[1].main.id, "fnp45_mp", "the secondary landed");
        assert_eq!(a.lethal.def.id, "frag_grenade_mp", "the script's lethal throw");
        assert_eq!(a.tactical.def.id, "concussion_grenade_mp", "the script's tactical throw");
        assert_eq!(a.melee.id, "knife_held_mp", "the script's melee");

        // A second first-person session on the already classed General (the RTS run does this):
        // no pick is offered, and the pick window must not turn into a weapon swap.
        sim.take_control(&host, 0, 1);
        sim.events.clear();
        let mut swapped = false;
        for _ in 0..(30 * 2) {
            let out = client.frame(&mut sim, &inp);
            assert_eq!(out.choose_class, 0, "a classed General is not offered the pick");
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            let f = sim.frame + 1;
            sim.step(&host, f);
            swapped |= sim.events.iter().any(|e| matches!(e.kind, crate::sim::EventKind::Swapped));
            sim.events.clear();
            inp.logic_frame = f;
        }
        assert!(!swapped, "the pick key was not pressed as a weapon swap");
        crate::profile::reset();
    }

    /// With no custom in the profile the standard run still reaches first person: it picks the
    /// first default class in the match list's order (SMG, `changeclass.lua`). A re-entry on an
    /// already classed General must not turn the pick key into gameplay input (the RTS run's
    /// first-person phase reuses this script and checks the pistol is still in hand).
    #[cfg(feature = "autopilot")]
    #[test]
    fn the_standard_autopilot_falls_back_to_the_first_preset_without_a_custom() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client { autopilot: Some(Autopilot::default()), ..Default::default() };
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        let mut pick: Option<ClientOut> = None;
        for _ in 0..(30 * 3) {
            let out = client.frame(&mut sim, &inp);
            if out.choose_class != 0 && pick.is_none() {
                pick = Some(out);
                sim.choose_class(0, out.choose_class_general, (out.choose_class - 1) as usize, &[]);
            }
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            inp.dt = LOGIC_DT;
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        let pick = pick.expect("the standard run picks a class at spawn");
        assert_eq!(pick.choose_class, 2, "the first default class in the match list's order (SMG, registry index 1 sent 1-based)");
        assert_eq!(pick.choose_class_payload_len, 0, "a preset is an index pick");
        assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].main.id, "mp7_mp");

        // A second session on the classed General: the Key1 pick window must stay inert.
        sim.take_control(&host, 0, 1);
        sim.events.clear();
        let mut swapped = false;
        for _ in 0..(30 * 2) {
            let out = client.frame(&mut sim, &inp);
            assert_eq!(out.choose_class, 0, "a classed General is not offered the pick");
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            let f = sim.frame + 1;
            sim.step(&host, f);
            swapped |= sim.events.iter().any(|e| matches!(e.kind, crate::sim::EventKind::Swapped));
            sim.events.clear();
            inp.logic_frame = f;
        }
        assert!(!swapped, "the pick key was not pressed as a weapon swap");
        crate::profile::reset();
    }

    /// A custom class whose wire words do not fit the ABI payload is denied, never truncated: a
    /// truncated payload fails to decode in the sim, which would ignore the pick and leave the
    /// General on his old kit (the silent "the pick did nothing" bug).
    #[test]
    fn an_oversized_custom_pick_is_denied_not_truncated() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::save(crate::classes::CustomClass {
            name: "Too Big".into(),
            primary: "x".repeat(300),
            ..crate::classes::CustomClass::default()
        });
        assert!(crate::profile::classes()[0].to_wire_words().len() > 64, "the seed is really oversized");
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert!(client.class_pick_open);
        // The seeded custom leads the match list (`changeclass.lua`): press its row key.
        let row = crate::classui::pick_rows(false)
            .iter()
            .position(|r| matches!(r, crate::classui::PickRow::Custom(0)))
            .expect("the seeded custom is listed");
        let key = crate::classui::pick_key(row).expect("a digit key");
        inp.keys.set_down(key, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(key, false);
        assert_eq!(out.choose_class, 0, "nothing was sent");
        assert_eq!(out.choose_class_payload_len, 0);
        assert!(client.class_pick_open, "the screen stays up: the pick was denied");
        crate::profile::reset();
    }

fn tracer_def() -> &'static crate::weapons::WeaponDef {
    let def = crate::weapons::WeaponDef { tracer_fx_weapon: "TRACER".into(), enemy_tracer_fx_weapon: "TRACER_ENEMY".into(), ..Default::default() };
    Box::leak(Box::new(def))
}

#[test]
fn each_viewer_sees_the_enemy_tracer_of_shooters_not_on_his_side() {
    // BO2's Tracer_Spawn: FUN_004bbf10 (same team as the local client) picks the friendly
    // tracer, any other shooter the enemyTracerType walk's (FUN_008eb420). The viewer here is
    // player 0 commanding General 1; General 2 is his own second soldier, 4 an enemy's, 6 gone.
    use crate::client::tracers::{ViewTracers, Viewer};
    let play = |host: &FakeHost, viewer: &Viewer, shots: &mut Vec<crate::sim::ShotTracer>| ViewTracers { viewer: *viewer, ..Default::default() }.play(host, shots);
    use crate::sim::ShotTracer;
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(general(2, 0));
    host.add(general(4, 1));
    let def = tracer_def();
    let shot = |shooter| ShotTracer { shooter, def, target: 0, end: Vec3::new(100.0, 0.0, 0.0), left: false };
    let viewer = Viewer { player: 0, general: 1, first_person: 0 };
    let mut shots = vec![shot(1), shot(2), shot(4), shot(6)];
    play(&host, &viewer, &mut shots);
    assert!(shots.is_empty(), "the queue is drained");
    let seen: Vec<(u32, String)> = host.fx.borrow().iter().map(|(s, w, _)| (*s, w.clone())).collect();
    assert_eq!(seen, [(1, "TRACER".into()), (2, "TRACER".into()), (4, "TRACER_ENEMY".into()), (6, "TRACER_ENEMY".into())]);

    // Another machine's viewer (player 1, General 4) sees the same bullets the other way round:
    // the sim queued one viewer-agnostic record, the asset is each machine's.
    host.fx.borrow_mut().clear();
    let mut shots = vec![shot(1), shot(4)];
    play(&host, &Viewer { player: 1, general: 4, first_person: 0 }, &mut shots);
    let seen: Vec<String> = host.fx.borrow().iter().map(|(_, w, _)| w.clone()).collect();
    assert_eq!(seen, ["TRACER_ENEMY", "TRACER"]);

    // Zero Hour's alliance makes a friend of another player's soldier: the fake host's relationship
    // is by owner, so a viewer whose player index differs but whose General shares the owner sees
    // General 2 as an ally (the ALLY flag, not ownership, decides).
    host.fx.borrow_mut().clear();
    let mut shots = vec![shot(2)];
    play(&host, &Viewer { player: 7, general: 1, first_person: 0 }, &mut shots);
    assert_eq!(host.fx.borrow()[0].1, "TRACER");
    // Before he commands a General, only his own player's soldiers are friends.
    host.fx.borrow_mut().clear();
    let mut shots = vec![shot(2), shot(4)];
    play(&host, &Viewer { player: 0, general: 0, first_person: 0 }, &mut shots);
    let seen: Vec<String> = host.fx.borrow().iter().map(|(_, w, _)| w.clone()).collect();
    assert_eq!(seen, ["TRACER", "TRACER_ENEMY"]);
}

#[test]
fn a_def_without_an_enemy_tracer_draws_its_own_for_everyone() {
    use crate::client::tracers::choose;
    let def = crate::weapons::WeaponDef { tracer_fx_weapon: "TRACER".into(), ..Default::default() };
    let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
    assert_eq!(choose(def, true), "TRACER");
    assert_eq!(choose(tracer_def(), true), "TRACER_ENEMY");
    assert_eq!(choose(tracer_def(), false), "TRACER");
}

#[test]
fn the_client_frame_records_the_viewer() {
    let mut client = Client::default();
    let mut sim = Sim::new();
    let host = FakeHost::default();
    host.add(general(1, 2));
    sim.take_control(&host, 2, 1);
    let mut inp = ClientIn { local_player: 2, ..Default::default() };
    frame_with(&mut client, &mut sim, &host, &mut inp, 1);
    assert_eq!((client.tracers.viewer.player, client.tracers.viewer.general), (2, 1));
}

#[test]
fn the_own_first_person_tracer_is_drawn_from_the_viewmodel_on_the_same_countdown() {
    // BO2 draws your own bullets' tracers too (FUN_00539800 sets FUN_00538e40's draw flag for
    // every shooter; cg_firstPersonTracerChance is never read), starting at the viewmodel's flash
    // tag (FUN_00535f80): the own General's tracer is not fired from his hidden body but handed to
    // the viewmodel pass for the next rendered frame, others' still go through fire_fx.
    use crate::client::tracers::{OwnTracer, ViewTracers, Viewer};
    use crate::sim::ShotTracer;
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(general(4, 1));
    let def = tracer_def();
    let end = Vec3::new(50.0, 0.0, 0.0);
    let mut view = ViewTracers { viewer: Viewer { player: 0, general: 1, first_person: 1 }, ..Default::default() };
    let mut shots = vec![ShotTracer { shooter: 1, def, target: 0, end, left: false }, ShotTracer { shooter: 4, def, target: 0, end, left: false }];
    view.play(&host, &mut shots);
    let fired: Vec<(u32, String)> = host.fx.borrow().iter().map(|(s, w, _)| (*s, w.clone())).collect();
    assert_eq!(fired, [(4, "TRACER_ENEMY".into())], "only the other soldier's through fire_fx");
    assert!(view.frame.is_empty(), "not before the next rendered frame");
    view.begin_frame(true);
    assert_eq!(view.frame, [OwnTracer { fx_weapon: "TRACER", end, left: false }], "his own side's tracer");
    view.begin_frame(true);
    assert!(view.frame.is_empty(), "drawn once");
    // Out of first person there is no viewmodel to draw from: the queue is dropped.
    view.play(&host, &mut vec![ShotTracer { shooter: 1, def, target: 0, end, left: false }]);
    view.begin_frame(false);
    view.begin_frame(true);
    assert!(view.frame.is_empty());
}

#[test]
fn the_own_tracers_follow_the_sims_countdown_deterministically() {
    // The own-gun tracers come from the sim's countdown, not a client random roll: two runs of
    // the same input queue the same bullets.
    let run = || {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        for f in 1..=120 {
            sim.step(&host, f);
        }
        let w = sim.players[0].as_mut().unwrap().arsenal.current_mut();
        let def = crate::weapons::WeaponDef { tracer_fx_weapon: "TRACER".into(), tracer_interval: 2, pellets: 1, fire: crate::weapons::FireMode::Auto, ..w.def.clone() };
        let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
        (w.def, w.main, w.ammo, w.reserve) = (def, def, 9, 0);
        let mut view = ViewTracers { viewer: Viewer { player: 0, general: 1, first_person: 1 }, ..Default::default() };
        let mut drawn = Vec::new();
        for f in 121..=300 {
            sim.input(0, 1, crate::input::FpsInput { held: buttons::FIRE, ..Default::default() });
            sim.step(&host, f);
            view.play(&host, &mut sim.tracers);
            view.begin_frame(true);
            drawn.extend(view.frame.iter().map(|t| (f, t.end)));
        }
        drawn
    };
    use crate::client::tracers::{ViewTracers, Viewer};
    let a = run();
    assert_eq!(a.len(), 4, "every 2nd of 9 rounds: {a:?}");
    assert_eq!(a, run());
}

    /// A shellshock with look control caps the turn rate (`maxyawspeed` deg/s) and scales the
    /// sensitivity (t6mp `FUN_005109f0`); the concussion's is 22°/s at full strength.
    #[test]
    fn a_shellshock_kicks_the_view_not_the_aim_and_rings() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        let calm = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        let aim = (client.yaw, client.pitch);
        let looks = |o: &ClientOut| (o.dir.y.atan2(o.dir.x), o.dir.z.asin());
        // The flashbang: a 0.05 kick radius and BO2 MP's one ringing loop.
        sim.players[0].as_mut().unwrap().shock.play(crate::shellshock::names::FLASHBANG, 3.0);
        client.cues.loops.clear();
        let mut moved = 0.0f32;
        for _ in 0..30 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            let (y, p) = (looks(&out), looks(&calm));
            moved = moved.max(crate::math::wrap_angle(y.0 - p.0).abs()).max((y.1 - p.1).abs());
        }
        assert!(moved > 1e-3 && moved < 0.1, "the camera wobbles within the radius: {moved}");
        assert_eq!((client.yaw, client.pitch), aim, "the aim the input carries is untouched");
        let ring = crate::shellshock::loop_event("chr_flashbang_tinnitus_loop");
        let gains: Vec<f32> = client.cues.loops.iter().filter(|(e, _)| *e == ring).map(|(_, g)| *g).collect();
        assert!(gains.len() == 30 && gains[0] > 0.0 && gains[0] < 1.0 && *gains.last().unwrap() == 1.0, "fades in over 0.25 s: {gains:?}");
        // Leaving first person stops the ringing at once.
        client.cues.loops.clear();
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        frame_with(&mut client, &mut sim, &host, &mut inp, (crate::sim::FIELD_COMPUTER_RAISE / LOGIC_DT) as usize + 5);
        assert!(!matches!(client.view, ViewMode::Fps));
        assert_eq!(client.cues.loops.iter().rfind(|(e, _)| *e == ring), Some(&(ring, 0.0)), "{:?}", client.cues.loops);
    }

    #[test]
    fn a_shellshock_slows_the_look() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        let turn = |client: &mut Client, sim: &mut Sim, inp: &mut ClientIn| {
            let before = client.yaw;
            inp.mouse_dx = -2000.0;
            client.frame(sim, inp);
            inp.mouse_dx = 0.0;
            crate::math::wrap_angle(client.yaw - before).abs()
        };
        let free = turn(&mut client, &mut sim, &mut inp);
        sim.players[0].as_mut().unwrap().shock.play(crate::shellshock::names::CONCUSSION, 6.0);
        let shocked = turn(&mut client, &mut sim, &mut inp);
        let cap = 22f32.to_radians() * inp.dt.clamp(0.0, 0.25);
        assert!(free > cap * 2.0, "{free}");
        assert!((shocked - cap).abs() < 1e-5, "{shocked} vs {cap}");
    }
