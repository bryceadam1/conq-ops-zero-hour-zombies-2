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
        sim.events.push(crate::sim::Event { player: 1, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO } });
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.viewmodel_fired, 0);
        // The sim's own `Shot` event is what the bridge's `fired` flag comes from.
        sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO } });
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.viewmodel_fired, 1);
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

        // Another player's impact is not the local player's FX.
        sim.events.push(impact(1, crate::impacts::Surface::Metal, false, false));
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.hits_len, 0);
        let cues: Vec<&str> = client.cues.queue.iter().map(|c| c.event).collect();
        assert!(!cues.contains(&sounds::IMPACT) && !cues.contains(&sounds::IMPACT_FLESH), "no cue for someone else's hit: {cues:?}");

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
        sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Shot { origin: Vec3::ZERO, end: Vec3::ZERO } });
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

    /// First person: an unclassed take-control offers the pick; Escape leaves the General unarmed
    /// (knife only) and plays on, and the auto-offer stays dismissed until the *next* take-control.
    #[test]
    fn fps_unclassed_offer_escape_dismisses_until_the_next_take_control() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);

        // Reach first person: the unclassed General is offered the pick, cursor free.
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert_eq!(out.menu_open, 1, "the spawn offer is up");
        assert_eq!(out.capture_mouse, 0, "the cursor is free for the screen");

        // Escape: the screen closes, first-person input/capture resume, and the General stays
        // unarmed (knife only) — Escape no longer means "keep the base kit".
        inp.keys.set_down(Key::Escape, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Escape, false);
        let out2 = client.frame(&mut sim, &inp);
        assert_eq!(out.menu_open, 0);
        assert_eq!(out2.menu_open, 0);
        assert_eq!(out2.capture_mouse, 1, "first-person play continues");
        assert!(client.class_pick_dismissed);
        assert!(sim.player(0).unwrap().arsenal.is_unarmed(), "Escape stays unarmed, not the stock kit");
        // It stays dismissed for this life.
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 20);
        assert_eq!(out.menu_open, 0);

        // Return to RTS (field computer) and take control again: the next take-control re-offers.
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        let mut frames = 0;
        while client.view != ViewMode::Rts && frames < 400 {
            frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            frames += 1;
        }
        assert_eq!(client.view, ViewMode::Rts, "back overhead");
        sim.take_control(&host, 0, 1);
        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert_eq!(out.menu_open, 1, "the next take-control re-offers the pick");
        assert_eq!(out.capture_mouse, 0);
        crate::profile::reset();
    }

    #[test]
    fn hor_plus_fov() {
        let f43 = horizontal_fov(65.0, 4.0 / 3.0).to_degrees();
        let f169 = horizontal_fov(65.0, 16.0 / 9.0).to_degrees();
        assert!((f43 - 65.0).abs() < 0.01);
        assert!(f169 > 80.0 && f169 < 82.0, "{f169}");
    }

    /// The `ZHBO_AUTOPILOT=classpick` script now exercises the pick-only flow: it picks the seeded
    /// custom class by its row key (5 presets + AB), leaves first person, and after the host takes
    /// control again forces the screen open and Escapes it, then plays on. A pick never writes the
    /// profile.
    #[cfg(feature = "autopilot")]
    #[test]
    fn the_classpick_autopilot_picks_the_seeded_class_and_escapes_a_second_offer() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        // Deliberately not the stock kit: the primary is the pistol and the lethal a claymore.
        crate::profile::load("AB\nprimary=fnp45_mp;secondary=an94_mp;lethal=claymore_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Toughness,FastHands,Dexterity");
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
        let mut retook = false;
        let mut reopened = false;
        let mut closed_after_escape = false;
        for i in 0..(30 * 20) {
            // The C++ autopilot takes control again after the field computer hands the General
            // back; the fake host does the same when the script has left first person.
            if client.view == ViewMode::Rts && !retook && i > 120 {
                sim.take_control(&host, 0, 1);
                retook = true;
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
                }
            }
            if retook && out.menu_open != 0 {
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
        let out = pick.expect("the script picks the seeded class");
        assert_eq!(out.choose_class, CHOOSE_CLASS_CUSTOM);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        let class = crate::classes::CustomClass::from_wire_words(&words).expect("the payload reassembles");
        assert_eq!(class.name, "AB", "the seeded custom is picked by name");
        // The kit, not just the name: this is what the classpick autotest now greps for too.
        let a = &sim.player(0).expect("the General").arsenal;
        assert_eq!(a.weapons[0].main.id, "fnp45_mp", "the seeded primary landed");
        assert_eq!(a.lethal.def.id, "claymore_mp", "the seeded lethal landed");
        assert!(a.has(crate::weapons::Perk::TOUGHNESS));
        assert!(reopened, "the script forces the screen open after the next take-control");
        assert!(closed_after_escape, "Escape closes it and first-person play continues");
        assert!(!any_save, "a pick must not write the profile");
        assert_eq!(crate::profile::classes().len(), 1, "the seeded profile is untouched");
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
            // Deliberately the reverse of the stock kit: the pistol as primary, the rifle as
            // secondary, a claymore, a shorter magazine reserve, and no stock perks.
            primary: "fnp45_mp".into(),
            secondary: "an94_mp".into(),
            lethal: "claymore_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: vec!["Toughness".into(), "FlakJacket".into()],
            reserve: [40, 20],
            camo: String::new(),
            reticle: String::new(),
            reticle_color: String::new(),
            knife: String::new(),
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
        assert_eq!(a.weapons[0].main.id, "fnp45_mp", "primary is the class's, not the stock AN-94");
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

    /// BO2's changeclass focuses the class the player is wearing, so the in-match panel opens on
    /// the equipped class rather than on the first row (the command bar's entry can be pressed at
    /// any time, and landing on row 1 would make the player think they wear the default).
    #[test]
    fn the_in_match_panel_opens_on_the_class_the_general_is_wearing() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        crate::profile::save(crate::classes::CustomClass { name: "First".into(), ..Default::default() });
        crate::profile::save(crate::classes::CustomClass { name: "Second".into(), primary: "fnp45_mp".into(), ..Default::default() });
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
            primary: "fnp45_mp".into(),
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
        assert_eq!(a.weapons[0].main.id, "fnp45_mp", "the overhead pick applied the custom kit");
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
            primary: "fnp45_mp".into(),
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
        // 1-based: the presets come first, so the first custom is button PRESETS + 1.
        let custom_button = (crate::classicons::PRESETS + 1) as i32;
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
        assert_eq!(p.arsenal.weapons[0].main.id, "fnp45_mp", "the custom's primary landed");

        // The next custom slot is empty: pressing its button must send nothing at all.
        inp.class_button = custom_button + 1;
        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.choose_class, 0);
        assert_eq!(out.choose_class_payload_len, 0);
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
