//! The client's half of the sim's events: what the local player hears and sees of them (his own
//! shots, hits, reloads, throws and the camera transitions; others' shots and throws as heard
//! here), and, while first person is predicted, the matching of the sim's own events against what
//! the predictions already presented (`predict`).

use super::*;

impl Client {
    /// The sim's events since the last frame: others' shots and throws as heard here, and the
    /// local player's own. `owed`, while predicting: what the predictions of the inputs the sim
    /// just took in already presented (`predict::Predictor::reconcile` keeps the rest).
    pub(super) fn consume_events(&mut self, sim: &mut Sim, inp: &ClientIn, body_pos: Option<Vec3>, owed: Option<Vec<EventKind>>) {
        let me = inp.local_player;
        let mut mine: Vec<EventKind> = sim.events.iter().filter(|e| e.player == me).map(|e| e.kind).collect();
        // Other players' shots: BO2's third-person chain, layer by layer at the listener's distance.
        for e in sim.events.iter().filter(|e| e.player != me) {
            let Some(p) = sim.player(e.player) else { continue };
            match e.kind {
                EventKind::Shot { origin, .. } => self.cues.third_person_shot(&p.arsenal.current().def.fx_weapon, origin, body_pos),
                // An offhand leaving another player's hand: its third-person `fireSound` at them
                // (`sounds::remote_offhand`), as BO2 plays it for everyone but the thrower.
                EventKind::Thrown { lethal } => {
                    let def = if lethal { p.arsenal.lethal.def } else { p.arsenal.tactical.def };
                    if let Some(cue) = sounds::remote_offhand(def) {
                        self.cues.play_at(cue, p.body.eye());
                    }
                }
                EventKind::Planted { kind } => {
                    let set_down = [p.arsenal.lethal.def, p.arsenal.tactical.def].into_iter().find(|d| d.equipment_kind == kind && d.place == crate::weapons::Place::Plant);
                    if let Some(cue) = set_down.and_then(sounds::remote_offhand) {
                        self.cues.play_at(cue, p.body.eye());
                    }
                }
                _ => {}
            }
        }
        sim.events.clear();
        self.hits_len = 0;
        if let Some(owed) = owed {
            let r = self.predict.reconcile(mine, owed);
            mine = r.present;
            for headshot in r.kills {
                self.confirm_kill(headshot);
            }
        }
        let auth = sim.player(me).cloned();
        self.present(inp, auth.as_ref(), auth.as_ref(), body_pos, mine);
    }

    /// The events this frame's prediction brought (`predict::Advance`): the own gun's tracers for
    /// the viewmodel pass (friendly to their own shooter, `tracers::choose`), then the events as
    /// the predicted record's weapon presents them.
    pub(super) fn present_predicted(&mut self, inp: &ClientIn, view: Option<&PlayerSim>, adv: predict::Advance) {
        for t in adv.tracers {
            let fx_weapon = tracers::choose(t.def, false);
            if !fx_weapon.is_empty() {
                self.tracers.pending.push(tracers::OwnTracer { fx_weapon, end: t.end, left: t.left });
            }
        }
        self.present(inp, None, view, None, adv.events);
    }

    /// A predicted hit marker the sim says killed: the kill's feedback (the marker's kill style,
    /// the kill count and the score popup) without a second hit sound.
    fn confirm_kill(&mut self, headshot: bool) {
        self.hud.hitmarker = HITMARKER_TIME;
        self.hud.hitmarker_kill = true;
        self.hud.hitmarker_head |= headshot;
        self.count_kill();
    }

    fn count_kill(&mut self) {
        self.hud.lui.kills += 1;
        if self.hud.score_popup <= 0.0 {
            self.hud.score_points = 0;
        }
        self.hud.score_points += KILL_POINTS;
        self.hud.score_popup = SCORE_POPUP_TIME;
    }

    /// Present the local player's events: `auth` is the sim's record (the camera transitions read
    /// it), `holder` the record whose weapon in hand picks the sounds (the sim's, or the
    /// prediction's for its own events).
    fn present(&mut self, inp: &ClientIn, auth: Option<&PlayerSim>, holder: Option<&PlayerSim>, body_pos: Option<Vec3>, mine: Vec<EventKind>) {
        // The weapon in hand after the step: its sounds for this frame's shots, swing and throws.
        let held = holder.map(|p| (p.arsenal.current().def, p.arsenal.current().ammo, p.arsenal.current().string, p.arsenal.lethal.def, p.arsenal.tactical.def));
        let fire = held.map_or(sounds::FIRE, |(def, ..)| sounds::fire_for(def));
        let left_fire = holder.map_or(sounds::FIRE, |p| sounds::left_fire_for(p.arsenal.current().def));
        let swing = holder.map_or(sounds::MELEE_SWING, |p| sounds::melee_swing_for(p.arsenal.melee_weapon()));
        let shots = mine.iter().filter(|k| matches!(k, EventKind::Shot { left: false, .. })).count();
        // A dual-wield pair's left gun plays its own `_lh` shot.
        for _ in mine.iter().filter(|k| matches!(k, EventKind::Shot { left: true, .. })) {
            self.cues.play(left_fire);
        }
        if let Some((def, ammo, string, ..)) = held {
            // A fire string's opening (`fireStartSoundPlayer`: a burst's recording) plays once, on
            // the frame its first round leaves — the trigger string was empty before this step.
            if let Some(start) = sounds::some(def.fire_start_sound.as_str()).filter(|_| shots > 0 && string as usize <= shots) {
                self.cues.play(start);
            }
            for i in 0..shots {
                // The gunshot is per shot; the impact is per pellet and comes from its own event (a
                // shot into the air no longer plays a hard-surface sound). The last one this frame
                // left the magazine at `ammo`.
                self.cues.play(if i + 1 == shots { sounds::shot_for(def, ammo, fire) } else { fire });
            }
        }
        for kind in mine {
            match kind {
                EventKind::EnteredFps { general } => {
                    if let Some(p) = auth {
                        self.start_entering(inp, general, p.yaw, p.body.eye());
                    }
                }
                EventKind::ReturnedToRts { general } => {
                    self.general = general;
                    self.begin_leaving(inp, body_pos.unwrap_or(self.eye_curr));
                }
                EventKind::GeneralDied { .. } => {
                    self.hud.death_banner = DEATH_BANNER_TIME;
                    self.hud.lui.deaths += 1;
                    if !matches!(self.view, ViewMode::Rts) {
                        self.begin_leaving(inp, self.eye_curr - Vec3::new(0.0, 0.0, 10.0));
                    }
                }
                EventKind::HitMarker { headshot, kill } => {
                    self.cues.play(sounds::HIT_MARKER);
                    self.hud.hitmarker = HITMARKER_TIME;
                    self.hud.hitmarker_kill = kill;
                    self.hud.hitmarker_head = headshot;
                    if kill {
                        self.count_kill();
                    }
                }
                EventKind::Recoil { gun_kick } => {
                    // The view kick is the sim's and never moves the mouse aim (BO2 keeps it out of
                    // `cl.viewangles`). The gun itself kicks on G5's spring, not with the camera.
                    self.vm_kick = Some(gun_kick);
                    self.hud.muzzle = 0.05;
                }
                EventKind::Flinch { pitch_deg } => self.hud.flinch_pitch += pitch_deg,
                EventKind::Damaged { amount } => {
                    self.cues.play(sounds::HURT);
                    self.hud.damage_flash = (self.hud.damage_flash + amount / 60.0).min(1.0);
                }
                // A pellet landed: BO2's impact FX (the bridge plays it from `ClientOut.hits`) and
                // the impact table's sound for the gun's type and the surface here (`sounds::impact_cue`).
                EventKind::Impact { at, dir, surface, head, fatal, impact_type, exit } => {
                    let cue = if exit { sounds::exit_cue(impact_type, surface) } else { sounds::impact_cue(impact_type, surface) };
                    if let Some(cue) = cue {
                        self.cues.play_at(cue, at);
                    }
                    if self.hits_len < CLIENT_HITS {
                        self.hits[self.hits_len] = ClientHit {
                            point: at,
                            dir,
                            surface: surface as i32,
                            head: head as i32,
                            fatal: fatal as i32,
                            impact_type,
                            exit: exit as i32,
                        };
                        self.hits_len += 1;
                    }
                }
                EventKind::Shot { left: false, .. } => self.vm_events.shot = true,
                EventKind::Shot { left: true, .. } => self.vm_events.left_shot = true,
                EventKind::ReloadStarted => self.vm_events.reload_started = true,
                EventKind::Swapped => self.vm_events.swapped = true,
                EventKind::FieldComputerOpened => {}
                EventKind::ClassChosen { .. } => {
                    // No cue here: BO2's class change applies silently on spawn. The menu already
                    // played the equip when the pick was made (and the command bar does too).
                    // A pick landed: the forced screen closes, and the next spawn's pick returns.
                    self.class_pick_forced = false;
                }
                EventKind::Melee => self.cues.play(swing),
                EventKind::MeleeHit => {
                    if let Some((def, ..)) = held.filter(|(def, ..)| !def.melee_hit_sound.is_empty()) {
                        self.cues.play(def.melee_hit_sound.as_str());
                    }
                }
                EventKind::OffhandReady { lethal } => {
                    self.cues.play(held.map_or(sounds::PULL_PIN, |(_, _, _, l, t)| sounds::pullback_for(if lethal { l } else { t })))
                }
                EventKind::Thrown { lethal } => self.cues.play(held.map_or(sounds::THROW, |(_, _, _, l, t)| sounds::throw_for(if lethal { l } else { t }))),
                EventKind::Launched => self.cues.play(held.map_or(sounds::FIRE_LAUNCHER, |(def, ammo, ..)| sounds::shot_for(def, ammo, sounds::launch_for(def)))),
                EventKind::Exploded { at, sound, impact_type, surface } => {
                    for cue in sounds::explosion_cues(sound, impact_type, surface).into_iter().flatten() {
                        self.cues.play_at(cue, at);
                    }
                }
                EventKind::Bounced { at, sound } => {
                    if let Some(cue) = sounds::some(sound) {
                        self.cues.play_at(cue, at);
                    }
                }
                EventKind::Dud { at, sound, surface } => {
                    for cue in sounds::dud_cues(sound, surface).into_iter().flatten() {
                        self.cues.play_at(cue, at);
                    }
                }
                EventKind::Intercepted { at } => self.cues.play_at(sounds::EXPLOSION, at),
                EventKind::Hacking { progress } => {
                    self.hud.hack_progress = progress;
                    self.hud.hack_shown = 0.15;
                }
                EventKind::Hacked => self.hud.hack_shown = 0.0,
                EventKind::AltToggled => self.vm_events.swapped = true,
                EventKind::Planted { kind } => {
                    use crate::equipment::Kind;
                    if matches!(kind, Kind::Claymore | Kind::Betty | Kind::Trophy | Kind::Insertion) {
                        // The planted item's own sound: the player's lethal or tactical of that kind.
                        let def = held.and_then(|(_, _, _, l, t)| [l, t].into_iter().find(|d| d.equipment_kind == kind));
                        self.cues.play(def.map_or(sounds::PLANT, sounds::plant_for));
                    }
                }
                EventKind::Detonated => self.cues.play(sounds::DETONATE),
                // The shield going down plays the plant (BO2's own `wpn_riotshield_plant` is a
                // notetrack sound the mod does not convert yet).
                EventKind::ShieldDeployed { at } => self.cues.play_at(sounds::SHIELD_PLANT, at),
                EventKind::ShieldPickedUp => self.vm_events.swapped = true,
                EventKind::ShieldDestroyed { at, .. } => self.cues.play_at(sounds::SHIELD_DESTROY, at),
                // A shield hit: the shield's own impact sound and a metal spark (no blood).
                EventKind::ShieldHit { at, dir, knife, impact_type } => {
                    self.cues.play_at(if knife { sounds::SHIELD_HIT_KNIFE } else { sounds::SHIELD_HIT_BULLET }, at);
                    if self.hits_len < CLIENT_HITS {
                        let surface = crate::impacts::Surface::Metal as i32;
                        self.hits[self.hits_len] = ClientHit { point: at, dir, surface, head: 0, fatal: 0, impact_type, exit: 0 };
                        self.hits_len += 1;
                    }
                }
                EventKind::DestroyedEquipment => {}
                // BO2's weapon hint ("Cannot Plant Here"): set afresh, drawn by the HUD.
                // BO2's `iprintlnbold` hints: the plant refusal, and a mid-life class pick that
                // waits for the next spawn (`MP_CHANGE_CLASS_NEXT_SPAWN`).
                EventKind::CantPlant { .. } | EventKind::ClassAtNextSpawn { .. } => self.hud.hint = crate::sim::event_hint(&kind).map(|k| (k, 0.0)),
                EventKind::Hammer { .. } => self.vm_events.swapped = true,
                EventKind::BuildPlaced { .. } => {
                    self.cues.play(sounds::PLANT);
                    self.note("Structure placed: hold [F] at it to build");
                }
                EventKind::BuildRefused { status } => {
                    self.cues.play(sounds::DRY_FIRE);
                    self.note(match status {
                        crate::building::status::NO_MONEY => "Not enough money",
                        crate::building::status::LOCKED => "Not available yet",
                        crate::building::status::BUSY => "Someone else is repairing it",
                        _ => "Can't build there",
                    });
                }
                EventKind::Working { percent, .. } => {
                    self.hud.build_percent = percent;
                    if self.hud.build_shown <= 0.0 {
                        self.hud.build_blow = 0.0;
                    }
                    self.hud.build_shown = 0.15;
                }
                EventKind::WorkDone { kind } => {
                    self.hud.build_shown = 0.0;
                    self.cues.play(sounds::PLANT);
                    self.note(crate::building::work_text(kind).done);
                }
            }
        }
    }
}
