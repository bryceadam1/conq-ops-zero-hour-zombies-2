//! The client's half of the sim's events: what the local player hears and sees of them (his own
//! shots, hits, reloads, throws and the camera transitions; what happens at a place in the world —
//! impacts, blasts, bounces, the deployed shield — whoever's it is; others' shots, launches and
//! throws as heard here), and, while first person is predicted, the matching of the sim's own
//! events against what the predictions already presented (`predict`).

use super::*;
use crate::sim::Event;

/// The hit, in BO2 health, that flashes the screen full (the mod's presentation: an AN-94 round up
/// close, 40 of BO2's 100); smaller hits flash it in proportion and they add up.
const DAMAGE_FLASH_FULL: f32 = 40.0;

impl Client {
    /// The sim's events since the last frame: others' shots and throws as heard here, and the
    /// presented seat's own. `me` is the seat whose view was presented (its events are the
    /// first-person ones), `own` whether that was the local player's own view (a spectator's
    /// followed General gets no death banner, hit marker or score, and its view moves are the
    /// follow logic's, [`super::spectate`]). `owed`, while predicting: what the predictions of the
    /// inputs the sim just took in already presented (`predict::Predictor::reconcile` keeps the
    /// rest).
    pub(super) fn consume_events(&mut self, sim: &mut Sim, inp: &ClientIn, me: i32, own: bool, body_pos: Option<Vec3>, owed: Option<Vec<EventKind>>) {
        // Every General's third-person presentation first: his torso action, its sounds at him,
        // his voice (`remote`).
        for e in &sim.events {
            let Ok(seat) = usize::try_from(e.player) else { continue };
            self.remote.event(seat, &e.kind, sim.player(e.player), e.player != me, &mut self.cues);
        }
        let mut mine: Vec<EventKind> = sim.events.iter().filter(|e| e.player == me).map(|e| e.kind).collect();
        let others: Vec<Event> = sim.events.iter().filter(|e| e.player != me).copied().collect();
        sim.events.clear();
        self.hits_len = 0;
        self.remote_hits.clear();
        if let Some(owed) = owed {
            let r = self.predict.reconcile(mine, owed);
            mine = r.present;
            for headshot in r.kills {
                self.confirm_kill(headshot);
            }
        }
        let auth = sim.player(me).cloned();
        self.present(inp, own, auth.as_ref(), auth.as_ref(), body_pos, mine);
        // Everyone else's events, after the presented seat's own: their impacts take the frame's
        // impact slots the own shots left (`ClientOut.hits`). Each round knows how many of its
        // shooter's rounds this step fired after it (`sounds::remote_round`'s `later`).
        for (i, e) in others.iter().enumerate() {
            let later = if is_round(&e.kind) { others[i + 1..].iter().filter(|o| o.player == e.player && is_round(&o.kind)).count() } else { 0 };
            self.present_remote(sim, *e, body_pos, later);
        }
    }

    /// One of another player's rounds (`right`: how many of his rounds this step fired after it,
    /// `None` for the dual-wield left gun's).
    fn remote_round(&mut self, p: &PlayerSim, at: Vec3, listener: Option<Vec3>, right: Option<usize>) {
        let w = p.arsenal.current();
        let cues = right.map(|later| sounds::remote_round(w.def, w.string, w.ammo, later));
        match cues.and_then(|c| c.shot) {
            Some(last) => self.cues.third_person_cue(last, at, listener),
            None => self.cues.third_person_shot(&w.def.fx_weapon, at, listener),
        }
        if let Some(opening) = cues.and_then(|c| c.opening) {
            self.cues.third_person_cue(opening, at, listener);
        }
    }

    /// The scoreboard's tallies (`Client::seats`), from every seat's events since the last frame
    /// (before they are presented and drained), and each first-person General's army and side as
    /// the engine's logic reports it relative to the local player's General (`host`: the real
    /// client always has it).
    pub(super) fn tally_seats(&mut self, sim: &Sim, host: Option<&dyn LogicHost>, local: i32) {
        for e in &sim.events {
            let Some(t) = usize::try_from(e.player).ok().and_then(|i| self.seats.get_mut(i)) else { continue };
            match e.kind {
                EventKind::HitMarker { kill: true, .. } => t.kills += 1,
                EventKind::GeneralDied { .. } => t.deaths += 1,
                EventKind::Assist { level, tactical } => t.assist_points += crate::client::assist_points(level, tactical),
                _ => {}
            }
            t.seen = true;
        }
        for (t, &n) in self.seats.iter_mut().zip(sim.score.assists.iter()) {
            t.assists = n;
        }
        let viewer = sim.player(local).map(|p| p.general);
        for (seat, p) in sim.players.iter().enumerate() {
            let (Some(p), Some(t)) = (p, self.seats.get_mut(seat)) else { continue };
            t.seen = true;
            let Some(info) = host.and_then(|h| h.object(p.general, viewer.unwrap_or(p.general))) else { continue };
            t.color = info.color;
            // Only the local player's own General knows the sides; while he has none the last
            // answer stands.
            if seat as i32 == local {
                t.ally = true;
            } else if viewer.is_some() {
                t.ally = info.has(crate::host::objflags::ALLY);
            }
        }
    }

    /// Another player's event as this machine hears and sees it. What happens at a place in the
    /// world (a pellet's impact, a blast, a bounce, a dud, a Trophy System's kill, the deployed
    /// shield's plant, hits and destruction) BO2 plays for every client, so it plays here exactly
    /// as it does for its owner ([`Self::world_event`]). His shots, launches, throws and set-downs
    /// play their third-person sounds at him. The rest is his own first person (his view, kick,
    /// flinch, viewmodel, 2D sounds, hit marker, HUD) and plays only on his machine; the match is
    /// exhaustive so that a new event has to say who sees it.
    fn present_remote(&mut self, sim: &Sim, e: Event, listener: Option<Vec3>, later: usize) {
        if self.world_event(e.kind, false) {
            return;
        }
        let Some(p) = sim.player(e.player) else { return };
        match e.kind {
            // BO2's third-person chain, layer by layer at the listener's distance. A launcher's
            // shot is the same chain (its FX weapon's `_npc` layers), from the eye it left. The
            // round that empties the magazine plays the def's last-round sound instead and a fire
            // string's first round adds its opening (`FUN_005320e0`, `sounds::remote_round`); the
            // left gun's rounds are the plain shot.
            EventKind::Shot { origin, left, .. } => self.remote_round(p, origin, listener, (!left).then_some(later)),
            EventKind::Launched => self.remote_round(p, p.body.eye(), listener, Some(later)),
            // An offhand leaving another player's hand: its third-person `fireSound` at them
            // (`sounds::remote_offhand`), as BO2 plays it for everyone but the thrower.
            EventKind::Thrown { lethal } => {
                let def = p.arsenal.offhand_def(lethal);
                if let Some(cue) = sounds::remote_offhand(def) {
                    self.cues.play_at(cue, p.body.eye());
                }
            }
            // The Black Hat fires as the hack lands: what others hear of any offhand firing.
            EventKind::Hacked => {
                let tool = [p.arsenal.lethal.def, p.arsenal.tactical.def].into_iter().find(|d| d.place == crate::weapons::Place::Hack);
                if let Some(cue) = tool.and_then(sounds::remote_offhand) {
                    self.cues.play_at(cue, p.body.eye());
                }
            }
            EventKind::Planted { kind } => {
                let set_down = [p.arsenal.lethal.def, p.arsenal.tactical.def].into_iter().find(|d| d.equipment_kind == kind && d.place == crate::weapons::Place::Plant);
                if let Some(cue) = set_down.and_then(sounds::remote_offhand) {
                    self.cues.play_at(cue, p.body.eye());
                }
            }
            // A pickup's third-person sound where the weapon lay (`pickupSound`/`ammoPickupSound`).
            EventKind::WeaponPickedUp { def, at } => {
                if let Some(cue) = sounds::some(def.npc_pickup_sound.as_str()) {
                    self.cues.play_at(cue, at);
                }
            }
            EventKind::AmmoPickedUp { def, at } => {
                if let Some(cue) = sounds::some(def.npc_ammo_pickup_sound.as_str()) {
                    self.cues.play_at(cue, at);
                }
            }
            // `scavenger_think`: `player playsound( "fly_equipment_pickup_npc" )`, heard at him.
            EventKind::ScavengerPickedUp { at } => self.cues.play_at(sounds::SCAVENGER_PICKUP_NPC, at),
            EventKind::WeaponDropped { .. } => {}
            // His alone, or heard through `remote` (the swing, the knife's hit, the pin, his reload's
            // notetracks, his voice: the third-person versions). The C4 trigger is his `*Player`
            // sound.
            EventKind::EnteredFps { .. }
            | EventKind::ClassChosen { .. }
            | EventKind::ClassAtNextSpawn { .. }
            | EventKind::FieldComputerOpened
            | EventKind::ReturnedToRts { .. }
            | EventKind::HitMarker { .. }
            | EventKind::Recoil { .. }
            | EventKind::Flinch { .. }
            | EventKind::Damaged { .. }
            | EventKind::ReloadStarted
            | EventKind::Swapped
            | EventKind::GeneralDied { .. }
            | EventKind::Melee
            | EventKind::MeleeHit
            | EventKind::Assist { .. }
            | EventKind::KilledBy { .. }
            | EventKind::OffhandReady { .. }
            | EventKind::CantPlant { .. }
            | EventKind::Detonated
            | EventKind::DestroyedEquipment
            | EventKind::Hacking { .. }
            | EventKind::AltToggled
            | EventKind::Hammer { .. }
            | EventKind::BuildPlaced { .. }
            | EventKind::BuildRefused { .. }
            | EventKind::Working { .. }
            | EventKind::WorkDone { .. }
            | EventKind::ShieldPickedUp => {}
            // The world's: `world_event` took them above.
            EventKind::Impact { .. }
            | EventKind::Exploded { .. }
            | EventKind::Bounced { .. }
            | EventKind::Dud { .. }
            | EventKind::Intercepted { .. }
            | EventKind::ShieldDeployed { .. }
            | EventKind::ShieldDestroyed { .. }
            | EventKind::ShieldHit { .. } => {}
        }
    }

    /// An event that happens at a place in the world, played the same for whoever's it is: its
    /// sounds at the place and its impact effect (`own`: the presented seat's, whose impacts come
    /// first in `ClientOut.hits`; another player's fill what is left). False for any other event.
    fn world_event(&mut self, kind: EventKind, own: bool) -> bool {
        match kind {
            // A pellet landed: BO2's impact FX (the bridge plays it from `ClientOut.hits`) and
            // the impact table's sound for the gun's type and the surface here (`sounds::impact_cue`).
            EventKind::Impact { at, dir, surface, head, fatal, impact_type, exit } => {
                let cue = if exit { sounds::exit_cue(impact_type, surface) } else { sounds::impact_cue(impact_type, surface) };
                if let Some(cue) = cue {
                    self.cues.play_at(cue, at);
                }
                let hit = ClientHit { point: at, dir, surface: surface as i32, head: head as i32, fatal: fatal as i32, impact_type, exit: exit as i32 };
                self.push_hit(hit, own);
            }
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
            // The shield going down plays the plant (BO2's own `wpn_riotshield_plant` is a
            // notetrack sound the mod does not convert yet).
            EventKind::ShieldDeployed { at } => self.cues.play_at(sounds::SHIELD_PLANT, at),
            EventKind::ShieldDestroyed { at, .. } => self.cues.play_at(sounds::SHIELD_DESTROY, at),
            // A shield hit: the shield's own impact sound and a metal spark (no blood).
            EventKind::ShieldHit { at, dir, knife, impact_type } => {
                self.cues.play_at(if knife { sounds::SHIELD_HIT_KNIFE } else { sounds::SHIELD_HIT_BULLET }, at);
                let surface = crate::impacts::Surface::Metal as i32;
                self.push_hit(ClientHit { point: at, dir, surface, head: 0, fatal: 0, impact_type, exit: 0 }, own);
            }
            _ => return false,
        }
        true
    }

    /// One impact for `ClientOut.hits`: the presented seat's own while there is room, another
    /// player's into what the own ones leave (`Client::frame_with` merges them).
    fn push_hit(&mut self, hit: ClientHit, own: bool) {
        if !own {
            if self.remote_hits.len() < CLIENT_HITS {
                self.remote_hits.push(hit);
            }
        } else if self.hits_len < CLIENT_HITS {
            self.hits[self.hits_len] = hit;
            self.hits_len += 1;
        }
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
        self.present(inp, true, None, view, None, adv.events);
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
    fn present(&mut self, inp: &ClientIn, own: bool, auth: Option<&PlayerSim>, holder: Option<&PlayerSim>, body_pos: Option<Vec3>, mine: Vec<EventKind>) {
        // The weapon in hand after the step: its sounds for this frame's shots, swing and throws.
        let held = holder.map(|p| (p.arsenal.current().def, p.arsenal.current().ammo, p.arsenal.current().string, p.arsenal.offhand_def(true), p.arsenal.offhand_def(false)));
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
            // Following someone else: his first-person presentation (gun, sounds, impacts,
            // flinch) plays, but nothing that is the player's own — his view moves (the follow
            // logic owns the camera), his death banner, hit markers, score and hints.
            if !own && !spectated_event(&kind) {
                continue;
            }
            // What happens at a place in the world plays the same for everyone (`world_event`).
            if self.world_event(kind, true) {
                continue;
            }
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
                // BO2's assist: its score popup (`processscoreevent( assist_level )`).
                EventKind::Assist { level, tactical } => {
                    if self.hud.score_popup <= 0.0 {
                        self.hud.score_points = 0;
                    }
                    self.hud.score_points += crate::client::assist_points(level, tactical);
                    self.hud.score_popup = SCORE_POPUP_TIME;
                }
                EventKind::Recoil { gun_kick } => {
                    // The view kick is the sim's and never moves the mouse aim (BO2 keeps it out of
                    // `cl.viewangles`). The gun itself kicks on G5's spring, not with the camera.
                    self.vm_kick = Some(gun_kick);
                    self.hud.muzzle = 0.05;
                }
                EventKind::Flinch { yaw_deg, pitch_deg } => {
                    // BO2 replaces the kick per hit (`CG_DamageFeedback`), never adds to it.
                    self.hud.flinch.hit(yaw_deg, pitch_deg, crate::tuning::get().flinch_time);
                }
                // The hurt cue and the screen's flash are the first-person view's (an overhead
                // General's hurt is his body's flinch and voice alone).
                EventKind::Damaged { .. } if !auth.is_some_and(PlayerSim::is_first_person) => {}
                EventKind::Damaged { amount, .. } => {
                    self.cues.play(sounds::HURT);
                    // By the hit in BO2 health (his share of `playerMaxHealth`).
                    let bo2 = auth.map_or(0.0, |p| crate::damage::general_bo2(&crate::tuning::get(), amount, p.max_health));
                    self.hud.damage_flash = (self.hud.damage_flash + bo2 / DAMAGE_FLASH_FULL).min(1.0);
                }
                EventKind::Shot { left: false, .. } => self.vm_events.shot = true,
                EventKind::Shot { left: true, .. } => self.vm_events.left_shot = true,
                EventKind::ReloadStarted => self.vm_events.reload_started = true,
                EventKind::Swapped => self.vm_events.swapped = true,
                EventKind::FieldComputerOpened => {}
                // His death animation is the third person's (`remote`).
                EventKind::KilledBy { .. } => {}
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
                    if let Some(cue) = held.and_then(|(_, _, _, l, t)| sounds::pullback_for(if lethal { l } else { t })) {
                        self.cues.play(cue);
                    }
                }
                EventKind::Thrown { lethal } => self.cues.play(held.map_or(sounds::THROW, |(_, _, _, l, t)| sounds::throw_for(if lethal { l } else { t }))),
                EventKind::Launched => self.cues.play(held.map_or(sounds::FIRE_LAUNCHER, |(def, ammo, ..)| sounds::shot_for(def, ammo, sounds::launch_for(def)))),
                EventKind::Hacking { progress } => {
                    self.hud.hack_progress = progress;
                    self.hud.hack_shown = 0.15;
                }
                EventKind::Hacked => {
                    self.hud.hack_shown = 0.0;
                    // The Black Hat fires as the hack lands: its own fire sound (`fireSoundPlayer`).
                    let tool = held.and_then(|(_, _, _, l, t)| [l, t].into_iter().find(|d| d.place == crate::weapons::Place::Hack));
                    if let Some(cue) = tool.and_then(sounds::hack_fire_for) {
                        self.cues.play(cue);
                    }
                }
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
                EventKind::ShieldPickedUp => self.vm_events.swapped = true,
                // BO2's EV_ITEM_PICKUP: the weapon's `pickupSoundPlayer`; the new weapon's first
                // raise is the arsenal's (EV_RAISE_PICKUP_WEAPON plays nothing, `FUN_00534f20`).
                EventKind::WeaponPickedUp { def, .. } => {
                    self.vm_events.swapped = true;
                    if let Some(cue) = sounds::some(def.pickup_sound.as_str()) {
                        self.cues.play(cue);
                    }
                }
                // EV_AMMO_PICKUP: `ammoPickupSoundPlayer`.
                EventKind::AmmoPickedUp { def, .. } => {
                    if let Some(cue) = sounds::some(def.ammo_pickup_sound.as_str()) {
                        self.cues.play(cue);
                    }
                }
                // `scavenger_think`: `playlocalsound( "fly_equipment_pickup_plr" )` and the icon.
                EventKind::ScavengerPickedUp { .. } => {
                    self.cues.play(sounds::SCAVENGER_PICKUP);
                    self.hud.scavenger_icon = crate::hud::SCAVENGER_ICON_FADE;
                }
                EventKind::WeaponDropped { .. } => {}
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
                // The world's: `world_event` played them above.
                EventKind::Impact { .. }
                | EventKind::Exploded { .. }
                | EventKind::Bounced { .. }
                | EventKind::Dud { .. }
                | EventKind::Intercepted { .. }
                | EventKind::ShieldDeployed { .. }
                | EventKind::ShieldDestroyed { .. }
                | EventKind::ShieldHit { .. } => {}
            }
        }
    }
}

/// Whether a spectator presents the followed General's event: his gun, sounds, impacts and the
/// shots' kick and flinch are what BO2 shows a spectator (the followed player's own first-person
/// view); his camera moves, death banner, hit marker, score popup, hints and work bars are his
/// alone (BO2's script HUD elements belong to the player, not his spectators).
fn spectated_event(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::EnteredFps { .. }
            | EventKind::ReturnedToRts { .. }
            | EventKind::GeneralDied { .. }
            | EventKind::HitMarker { .. }
            | EventKind::ClassChosen { .. }
            | EventKind::ClassAtNextSpawn { .. }
            | EventKind::CantPlant { .. }
            | EventKind::Hacking { .. }
            | EventKind::Hacked
            | EventKind::BuildPlaced { .. }
            | EventKind::BuildRefused { .. }
            | EventKind::Working { .. }
            | EventKind::WorkDone { .. }
    )
}

/// A round leaving another player's gun: what `sounds::remote_round` counts.
fn is_round(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Shot { left: false, .. } | EventKind::Launched)
}
