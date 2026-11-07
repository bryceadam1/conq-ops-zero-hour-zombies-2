//! Every General as BO2 shows and plays a third-person player: his upper body's action (the
//! reload, the knife, the throw and the pin, the shot's recoil, the weapon change: `crate::torso`)
//! and what is heard of it at him — the torso clip's sound notetracks (another player's reload),
//! the weapon def's third-person sounds (`WeaponDef::npc_sounds`: the swing, the knife's hit, the
//! raise and putaway, the pin), and his voice when he is hurt and when he dies
//! (`_battlechatter_mp.gsc` `painvox`/`deathvox`, [`sounds::voice_event`]).
//!
//! The torso is tracked for every seat, the presented one included (an overhead General is drawn in
//! third person too; the native body layers it, `nativebody::ffi::publish_torso`). The sounds are
//! for everyone but a presented first-person General, who hears his own 2D versions instead (his
//! viewmodel's notetracks, `*SoundPlayer`); his voice is his too, as BO2 plays it on his own head.
//! Presentation only: read off the sim's events and records, never fed back.

use super::*;
use crate::sim::MAX_PLAYERS;
use crate::torso;
use crate::weapons::{NpcSound, WeaponDef};

/// One General's torso action: the body manifest's clip and how far into it he is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torso {
    pub action: u8,
    pub clip: u16,
    pub t: f32,
    /// A pin or throw's offhand: the lethal (else the tactical).
    pub lethal: bool,
}

#[derive(Clone, Debug, Default)]
struct Seat {
    general: u32,
    /// His eye last frame: where his sounds play (his death's too, after the record is gone).
    eye: Vec3,
    weapon: sounds::WeaponCues,
    torso: Option<Torso>,
    /// Last frame he was on the ground, and held the trigger (his jump, landing and dry fire).
    grounded: bool,
    fire_held: bool,
    /// Overhead: each gun slot's rounds (magazine and spare) last frame: Zero Hour fires his guns
    /// then (`Arsenal::rts_shot` spends them), and a round gone is a shot's recoil.
    rts_rounds: [Option<u32>; 2],
    /// How his General was killed (`EventKind::KilledBy`: BO2's `dmgType`, the blow's way), for
    /// the death that follows.
    killed: Option<(crate::death::DmgType, Vec3)>,
    /// His General died this frame (`EventKind::GeneralDied`): his body falls in the update.
    dying: Option<u32>,
    /// Last frame: his feet, view yaw, stance, BO2's `movestatus` and `direction` of his gait and
    /// keys (`crate::death`), his weapon and its body set.
    pos: Vec3,
    yaw: f32,
    stance: u8,
    move_status: u8,
    direction: u8,
    vm: i32,
    body_anims: u8,
    /// How many chatter moments he has had (`sounds::chatter_roll`'s draw: the same on every
    /// machine, which all see the same events).
    chatter: u32,
}

/// A dead General's body: BO2's death clip played through and held (the corpse).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corpse {
    pub general: u32,
    /// The weapon he held (drawn hidden: BO2 drops it) and his body set's standing clip id.
    pub vm: i32,
    pub state: u16,
    pub clip: u16,
    pub t: f32,
}

/// How long a corpse is drawn: past Zero Hour's General's own `DestructionDelay` (8 s), after
/// which the bridge finds no object to draw it on.
pub const CORPSE_TIME: f32 = 10.0;

/// Every seat's third-person presentation ([`self`]).
#[derive(Clone, Debug, Default)]
pub struct Remote {
    seats: [Seat; MAX_PLAYERS],
    corpses: Vec<Corpse>,
}

/// The torso context of a General now, as the engine sets the script's conditions: his stance,
/// `movestatus` by his speed and `direction` by his movement keys (`crate::death`), whether he aims.
fn context_of(p: &PlayerSim) -> u8 {
    let t = &p.arsenal.tuning;
    let (forward, right) = p.input.move_axes();
    torso::context(p.stance_index() as u8, crate::death::move_status(p.body.horizontal_speed(), t), crate::death::move_direction(forward, right, t), p.arsenal.ads > 0.5)
}

impl Remote {
    /// Forget everyone (a new game).
    pub fn reset(&mut self) {
        *self = Remote::default();
    }

    /// Seat `seat`'s General, dead this frame, falls as the script's `DEATH` says for his stance,
    /// gait, keys and how and from where he was hit (`crate::death`; a death to anything but a
    /// seat's blow is `normal` from no direction); the engine picks one of its alternatives at
    /// random (here by his General's id, the same on every machine).
    fn fall(&mut self, seat: usize) {
        let s = &mut self.seats[seat];
        let Some(general) = s.dying.take() else { return };
        let (dmg, dir) = s.killed.take().unwrap_or_default();
        let ctx = torso::death_context(s.stance, s.move_status, s.direction, dmg as u8, crate::death::dmg_direction(dir, s.yaw));
        let alts = torso::get().death(ctx);
        if let Some(&clip) = alts.get(general as usize % alts.len().max(1)) {
            let state = zhbo_native::thirdperson::state::id(s.body_anims, zhbo_native::thirdperson::state::STAND_IDLE);
            let vm = s.vm;
            self.corpses.retain(|c| c.general != general);
            self.corpses.push(Corpse { general, vm, state, clip, t: 0.0 });
        }
    }

    /// Seat `seat`'s General's torso, if he is in an action.
    pub fn torso(&self, seat: usize) -> Option<Torso> {
        self.seats.get(seat).and_then(|s| s.torso)
    }

    /// Every General in a torso action, and every corpse in its death, for the body pass.
    pub fn torsos(&self) -> impl Iterator<Item = crate::nativebody::ffi::TorsoAction> + '_ {
        let live = self.seats.iter().filter_map(|s| s.torso.map(|t| crate::nativebody::ffi::TorsoAction { general: s.general, action: t.action, clip: t.clip, t: t.t, lethal: t.lethal }));
        live.chain(self.corpses.iter().map(|c| crate::nativebody::ffi::TorsoAction { general: c.general, action: torso::DEATH, clip: c.clip, t: c.t, lethal: false }))
    }

    /// The corpses, for the body pass's poses (`zhbo_client_native_poses`).
    pub fn corpses(&self) -> &[Corpse] {
        &self.corpses
    }

    fn start(&mut self, seat: usize, def: &WeaponDef, action: u8, context: u8) {
        self.start_offhand(seat, def, action, context, false);
    }

    fn start_offhand(&mut self, seat: usize, def: &WeaponDef, action: u8, context: u8, lethal: bool) {
        let Some(s) = self.seats.get_mut(seat) else { return };
        if let Some(clip) = torso::pick_for(def, action, context) {
            s.torso = Some(Torso { action, clip, t: 0.0, lethal });
        }
    }

    /// Whether seat `seat` is in an action a shot's recoil or a flinch does not interrupt (his
    /// reload, knife, throw, weapon change, jump or landing).
    fn busy(&self, seat: usize) -> bool {
        self.seats.get(seat).and_then(|s| s.torso).is_some_and(|t| !(t.action == torso::FIRE || torso::is_flinch(t.action)))
    }

    /// Seat `seat`'s battle chatter for `kind` (`_battlechatter_mp.gsc`): BO2's line for a reload
    /// or for the offhand thrown or set down, at his own odds, in his voice, on his head for
    /// everyone (`mpsaylocalsound` → `dosound` → `playsoundontag`); never with Dead Silence.
    fn chatter(&mut self, seat: usize, kind: &EventKind, p: &PlayerSim, cues: &mut Cues) {
        let a = &p.arsenal;
        let def = match *kind {
            EventKind::ReloadStarted => a.current().def,
            EventKind::Thrown { lethal } => if lethal { a.lethal.def } else { a.tactical.def },
            EventKind::Planted { kind } => match [a.lethal.def, a.tactical.def].into_iter().find(|d| d.equipment_kind == kind) {
                Some(d) => d,
                None => return,
            },
            _ => return,
        };
        let Some(s) = self.seats.get_mut(seat) else { return };
        let Some((line, odds)) = sounds::chatter_for(kind, def) else { return };
        s.chatter = s.chatter.wrapping_add(1);
        if p.arsenal.has_specialty(crate::weapons::specialty::Specialty::Quieter) || sounds::chatter_roll(seat, s.chatter) < odds {
            return;
        }
        if let Some(e) = sounds::chatter_event(crate::nativebody::ffi::faction_of(p.general), seat, line) {
            cues.play_at(e, p.body.eye());
        }
    }

    /// One of seat `seat`'s sim events, before they are presented and drained (`p`: his record).
    /// `heard`: another player's (his third-person sounds play here at him).
    pub fn event(&mut self, seat: usize, kind: &EventKind, p: Option<&PlayerSim>, heard: bool, cues: &mut Cues) {
        // His voice: BO2 plays it on his head for everyone, himself included.
        match *kind {
            // Hurt while not at full health already (BO2 notifies `snd_pain_player` only when
            // `self.health != self.maxhealth` as the damage arrives, `_globallogic_player.gsc:790`).
            EventKind::Damaged { amount, .. } => {
                if let Some(p) = p.filter(|p| p.health + amount < p.max_health - 1e-3) {
                    cues.play_at(sounds::voice_event(sounds::Voice::Pain, crate::nativebody::ffi::faction_of(p.general), seat), p.body.eye());
                }
            }
            // A callout: `reloadtracking`/`grenadetracking`'s line (`sounds::chatter_for`).
            EventKind::ReloadStarted | EventKind::Thrown { .. } | EventKind::Planted { .. } => {
                if let Some(p) = p {
                    self.chatter(seat, kind, p, cues);
                }
            }
            EventKind::KilledBy { dmg, dir, .. } => {
                if let Some(s) = self.seats.get_mut(seat) {
                    s.killed = Some((dmg, dir));
                }
            }
            EventKind::GeneralDied { general } => {
                // Where he was last seen: the record is gone with him.
                if let Some(s) = self.seats.get(seat).filter(|s| s.general != 0) {
                    let general = if general != 0 { general } else { s.general };
                    cues.play_at(sounds::voice_event(sounds::Voice::Death, crate::nativebody::ffi::faction_of(general), seat), s.eye);
                    // His body falls in this frame's update ([`Remote::fall`]): the report of the
                    // blow that killed him may come after this (the sim scores the step's blows
                    // after every seat's step, and his record ends in his own).
                    self.seats[seat].dying = Some(general);
                }
                if let Some(s) = self.seats.get_mut(seat) {
                    s.torso = None;
                }
            }
            _ => {}
        }
        let Some(p) = p else { return };
        let a = &p.arsenal;
        let held = a.current().def;
        let ctx = context_of(p);
        let at = p.body.eye();
        let offhand = |lethal: bool| if lethal { a.lethal.def } else { a.tactical.def };
        let npc = |cues: &mut Cues, def: &'static WeaponDef, m: NpcSound| {
            if heard {
                if let Some(e) = sounds::some(def.npc_sound(m)) {
                    cues.play_at(e, at);
                }
            }
        };
        match *kind {
            EventKind::ReloadStarted => self.start(seat, held, torso::RELOAD, ctx),
            EventKind::Melee => {
                self.start(seat, held, if p.charge.is_some() { torso::MELEE_CHARGE } else { torso::MELEE }, ctx);
                npc(cues, a.melee_weapon(), NpcSound::MeleeSwipe);
            }
            EventKind::MeleeHit => npc(cues, a.melee_weapon(), NpcSound::MeleeHit),
            EventKind::OffhandReady { lethal } => {
                self.start_offhand(seat, offhand(lethal), torso::PRIME, ctx, lethal);
                npc(cues, offhand(lethal), NpcSound::Pullback);
            }
            EventKind::Thrown { lethal } => self.start_offhand(seat, offhand(lethal), torso::THROW, ctx, lethal),
            // Hurt: the flinch for the side the blow came from (the script's `flinch` by
            // `dmgDirection`, `crate::death::dmg_direction`), unless a longer action is on.
            EventKind::Damaged { dir, .. } if !self.busy(seat) => self.start(seat, held, torso::flinch(crate::death::dmg_direction(dir, p.yaw)), ctx),
            // A shot's recoil, unless a longer action is on (BO2 fires from none of them).
            EventKind::Shot { left: false, .. } | EventKind::Launched if !self.busy(seat) => self.start(seat, held, torso::FIRE, ctx),
            _ => {}
        }
    }

    /// This frame: every seat's torso runs on (its notetracks heard at him, except for the
    /// presented first-person seat), his weapon change starts its torso and plays its
    /// third-person sounds, and where his eye is is remembered.
    pub fn update(&mut self, players: &[Option<PlayerSim>], presented: i32, dt: f32, cues: &mut Cues) {
        let table = torso::get();
        for seat in 0..self.seats.len() {
            self.fall(seat);
        }
        // The corpses fall and lie until Zero Hour removes the body.
        for c in &mut self.corpses {
            c.t += dt;
        }
        self.corpses.retain(|c| c.t < CORPSE_TIME);
        for (seat, s) in self.seats.iter_mut().enumerate() {
            let Some(p) = players.get(seat).and_then(|p| p.as_ref()) else {
                s.torso = None;
                s.general = 0;
                continue;
            };
            if p.general != s.general {
                *s = Seat { general: p.general, grounded: p.body.on_ground, chatter: s.chatter, ..Default::default() };
            }
            s.eye = p.body.eye();
            s.pos = p.body.pos;
            s.yaw = p.yaw;
            s.stance = p.stance_index() as u8;
            let tuning = &p.arsenal.tuning;
            s.move_status = crate::death::move_status(p.body.horizontal_speed(), tuning);
            let (forward, right) = p.input.move_axes();
            s.direction = crate::death::move_direction(forward, right, tuning);
            s.vm = p.arsenal.current().def.world_index(); // content: presentation (the corpse's draw)
            s.body_anims = p.arsenal.current().def.body_anims;
            let heard = !(seat as i32 == presented && p.is_first_person());
            // The weapon change: the putaway's and the raise's torso and third-person sounds.
            let ctx = context_of(p);
            let mut started = None;
            let at = s.eye;
            s.weapon.step(&p.arsenal, |m| {
                use crate::weapons::arsenal::RaiseKind;
                let (def, action, sound) = match m {
                    sounds::Moment::Putaway(d) => (d, torso::DROP, Some(NpcSound::Putaway)),
                    sounds::Moment::Raise(d, RaiseKind::First) => (d, torso::FIRST_RAISE, Some(NpcSound::FirstRaise)),
                    sounds::Moment::Raise(d, RaiseKind::Normal | RaiseKind::Quick | RaiseKind::Empty) => (d, torso::RAISE, Some(NpcSound::Raise)),
                    sounds::Moment::Raise(_, RaiseKind::Alt) | sounds::Moment::AltSwitch(_) | sounds::Moment::AdsZoom(_) => return,
                };
                if let Some(clip) = table.pick(def.torso_anims, action, ctx) {
                    started = Some(Torso { action, clip, t: 0.0, lethal: false });
                }
                if heard {
                    if let Some(e) = sound.and_then(|m| sounds::some(def.npc_sound(m))) {
                        cues.play_at(e, at);
                    }
                }
            });
            if started.is_some() {
                s.torso = started;
            }
            // His jump and landing (the script's whole-body `jump`/`land`), first person only: the
            // sim moves no overhead General's body.
            if p.is_first_person() {
                let held = p.arsenal.current().def;
                let action = match (s.grounded, p.body.on_ground) {
                    (true, false) if p.body.vel.z > 0.0 => Some(torso::JUMP),
                    (false, true) => Some(torso::LAND),
                    _ => None,
                };
                if let Some(clip) = action.and_then(|a| table.pick(held.torso_anims, a, ctx).map(|c| (a, c))) {
                    s.torso = Some(Torso { action: clip.0, clip: clip.1, t: 0.0, lethal: false });
                }
                // The trigger pulled on an empty magazine: his `emptyFireSound` at him.
                let w = p.arsenal.current();
                let fire = p.input.held & crate::input::buttons::FIRE != 0;
                if heard && fire && !s.fire_held && w.ammo == 0 && !w.is_reloading() {
                    if let Some(e) = sounds::some(w.def.npc_sound(NpcSound::EmptyFire)) {
                        cues.play_at(e, at);
                    }
                }
                s.fire_held = fire;
            }
            s.grounded = p.body.on_ground;
            // Overhead, Zero Hour fires his guns (no sim shot event): a round spent is the gun's
            // `fireweapon` torso, as BO2 plays it for every shot.
            if !p.is_first_person() {
                for (slot, last) in s.rts_rounds.iter_mut().enumerate() {
                    let Some(w) = p.arsenal.weapons.get(slot) else {
                        *last = None;
                        continue;
                    };
                    let (ammo, spare) = w.main_rounds();
                    let now = ammo as u32 + spare as u32;
                    let fired = last.is_some_and(|l| now < l);
                    *last = Some(now);
                    let busy = s.torso.is_some_and(|t| !(t.action == torso::FIRE || torso::is_flinch(t.action)));
                    if fired && !busy {
                        if let Some(clip) = table.pick(w.def.torso_anims, torso::FIRE, ctx) {
                            s.torso = Some(Torso { action: torso::FIRE, clip, t: 0.0, lethal: false });
                        }
                    }
                }
            } else {
                s.rts_rounds = [None; 2];
            }
            // The torso runs on; its sound notetracks play as it passes them.
            if let Some(t) = &mut s.torso {
                let from = t.t;
                t.t += dt;
                match table.clip(t.clip) {
                    Some(c) => {
                        if heard {
                            for &(when, event) in &c.notes {
                                if (when > from || (from == 0.0 && when == 0.0)) && when <= t.t {
                                    cues.play_at(event, at);
                                }
                            }
                        }
                        if t.t >= c.length {
                            s.torso = None;
                        }
                    }
                    None => s.torso = None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::WeaponState;

    /// Torso set 7's actions: a 2 s reload with two sound notetracks, a knife swipe, a throw.
    const TABLE: &str = "torso 7 0 0 4096\ntorso 7 1 0 4097\ntorso 7 3 0 4098\ntorso 7 6 0 4099\n\
        torso_clip 4096 2.0 pt_reload_stand_rifle\ntorso_note 4096 0.1 ZHBO_NTN_mag_out\ntorso_note 4096 1.0 ZHBO_NTN_mag_in\n\
        torso_clip 4097 0.9 p_knife_stand_rifle_swipe\ntorso_clip 4098 0.9 pt_grenade_stand_throw\ntorso_clip 4099 0.5 pt_rifle_stand_raise\n\
        torso 7 9 0 4100\ntorso 7 10 0 4101\ntorso 7 11 0 4102\ntorso 7 5 0 4103\n\
        torso_clip 4100 0.4 pt_stand_flinch_forward\ntorso_clip 4101 0.3 pb_standjump_takeoff\ntorso_clip 4102 0.4 pb_standjump_land\ntorso_clip 4103 0.3 pt_stand_shoot\n\
        torso 7 12 0 4104\ntorso 7 13 0 4105\ntorso_clip 4104 0.4 pt_stand_flinch_back\ntorso_clip 4105 0.4 pt_stand_flinch_left\n";

    fn def() -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef {
            id: "test_remote_mp".into(),
            torso_anims: 7,
            npc_sounds: vec![
                ("melee_swipe".into(), "ZHBO_BO2_SndNpc_wpn_melee_whoosh_npc".into()),
                ("melee_hit".into(), "ZHBO_BO2_SndNpc_wpn_melee_knife_hit_body".into()),
                ("pullback".into(), "ZHBO_BO2_SndNpc_wpn_semtex_charge".into()),
                ("raise".into(), "ZHBO_BO2_SndNpc_fly_generic_raise_npc".into()),
                ("empty_fire".into(), "ZHBO_BO2_SndNpc_wpn_generic_dryfire_npc".into()),
            ],
            ..Default::default()
        }))
    }

    fn soldier(general: u32) -> PlayerSim {
        let mut p = PlayerSim::spawn_for_tests();
        p.general = general;
        p.body.pos = Vec3::new(100.0 * general as f32, 0.0, 0.0);
        let d = def();
        for w in p.arsenal.weapons.iter_mut() {
            *w = WeaponState::new(d);
        }
        p.arsenal.melee = d;
        p.arsenal.class_melee = d;
        p.arsenal.lethal.def = d;
        p
    }

    fn played(c: &mut Cues) -> Vec<(&'static str, Option<Vec3>)> {
        c.queue.drain(..).map(|r| (r.event, r.at)).collect()
    }

    /// Another player's reload is his torso clip, heard at him through its notetracks; the
    /// presented first-person General's own is drawn but not heard (his viewmodel plays its 2D
    /// foley). His knife plays its torso and BO2's third-person swing and hit at him; his pin its
    /// pullback; his throw its torso.
    #[test]
    fn another_players_actions_play_their_torso_and_third_person_sounds_at_him() {
        let _g = crate::ffi::state_lock();
        crate::torso::install(TABLE);
        let players = vec![Some(soldier(1)), Some(soldier(2))];
        let mut r = Remote::default();
        let mut c = Cues::default();
        r.update(&players, 0, 0.0, &mut c);
        c.queue.clear();
        // His reload.
        r.event(1, &EventKind::ReloadStarted, players[1].as_ref(), true, &mut c);
        assert_eq!(r.torso(1), Some(Torso { action: torso::RELOAD, clip: 4096, t: 0.0, lethal: false }));
        let mut heard = Vec::new();
        for _ in 0..80 {
            r.update(&players, 0, 1.0 / 30.0, &mut c);
            heard.extend(played(&mut c));
        }
        let eye = players[1].as_ref().unwrap().body.eye();
        assert_eq!(heard, [("ZHBO_NTN_mag_out", Some(eye)), ("ZHBO_NTN_mag_in", Some(eye))], "his reload foley, at him, once each");
        assert_eq!(r.torso(1), None, "the torso ends with its clip");
        // The presented first-person General's own reload: drawn, not heard.
        r.event(0, &EventKind::ReloadStarted, players[0].as_ref(), false, &mut c);
        assert!(r.torso(0).is_some());
        for _ in 0..80 {
            r.update(&players, 0, 1.0 / 30.0, &mut c);
        }
        assert!(played(&mut c).is_empty(), "his own foley is his viewmodel's");
        // His knife, the hit, the pin and the throw.
        r.event(1, &EventKind::Melee, players[1].as_ref(), true, &mut c);
        assert_eq!(r.torso(1).map(|t| t.clip), Some(4097));
        r.event(1, &EventKind::MeleeHit, players[1].as_ref(), true, &mut c);
        r.event(1, &EventKind::OffhandReady { lethal: true }, players[1].as_ref(), true, &mut c);
        let sounds: Vec<&str> = played(&mut c).into_iter().map(|(e, at)| {
            assert_eq!(at, Some(eye));
            e
        }).collect();
        assert_eq!(sounds, ["ZHBO_BO2_SndNpc_wpn_melee_whoosh_npc", "ZHBO_BO2_SndNpc_wpn_melee_knife_hit_body", "ZHBO_BO2_SndNpc_wpn_semtex_charge"]);
        r.event(1, &EventKind::Thrown { lethal: true }, players[1].as_ref(), true, &mut c);
        assert_eq!(r.torso(1).map(|t| t.clip), Some(4098));
        // The presented seat's own swing is his 2D one: nothing here.
        r.event(0, &EventKind::Melee, players[0].as_ref(), false, &mut c);
        assert!(played(&mut c).is_empty());
        assert!(r.torsos().any(|a| a.general == 2 && a.clip == 4098 && a.action == torso::THROW), "published for the body pass");
    }

    /// A weapon change: his raise's torso and BO2's third-person raise sound at him.
    #[test]
    fn another_players_weapon_change_plays_its_torso_and_raise() {
        let _g = crate::ffi::state_lock();
        crate::torso::install(TABLE);
        let mut players = vec![None, Some(soldier(2))];
        // Settled: no raise or drop running.
        let a = &mut players[1].as_mut().unwrap().arsenal;
        (a.raise_left, a.drop_left) = (0.0, 0.0);
        let mut r = Remote::default();
        let mut c = Cues::default();
        r.update(&players, 0, 0.0, &mut c);
        c.queue.clear();
        let a = &mut players[1].as_mut().unwrap().arsenal;
        a.raise_left = 0.4;
        a.raise_kind = crate::weapons::arsenal::RaiseKind::Normal;
        r.update(&players, 0, 1.0 / 30.0, &mut c);
        assert_eq!(played(&mut c).into_iter().map(|(e, _)| e).collect::<Vec<_>>(), ["ZHBO_BO2_SndNpc_fly_generic_raise_npc"]);
        assert_eq!(r.torso(1).map(|t| t.clip), Some(4099));
    }

    /// His voice: pain when hurt from below full health (BO2's first hit from full health is
    /// silent), his death where he fell, each in his seat's voice.
    #[test]
    fn pain_and_death_voices_play_at_him() {
        let mut p = soldier(2);
        p.max_health = 100.0;
        p.health = 70.0;
        let players = vec![None, Some(p.clone())];
        let mut r = Remote::default();
        let mut c = Cues::default();
        r.update(&players, 0, 0.0, &mut c);
        c.queue.clear();
        let eye = p.body.eye();
        r.event(1, &EventKind::Damaged { amount: 30.0, dir: Vec3::ZERO }, Some(&p), true, &mut c);
        assert!(played(&mut c).is_empty(), "from full health: no pain");
        r.event(1, &EventKind::Damaged { amount: 10.0, dir: Vec3::ZERO }, Some(&p), true, &mut c);
        assert_eq!(played(&mut c), [(sounds::voice_event(sounds::Voice::Pain, crate::nativebody::ffi::faction_of(2), 1), Some(eye))]);
        r.event(1, &EventKind::GeneralDied { general: 2 }, None, true, &mut c);
        let death = played(&mut c);
        assert_eq!(death, [(sounds::voice_event(sounds::Voice::Death, crate::nativebody::ffi::faction_of(2), 1), Some(eye))]);
        assert!(death[0].0.starts_with("ZHBO_BO2_PainDeath_") && death[0].0.ends_with('1'), "seat 1's voice");
    }

    /// The torso's context is the script's conditions as the engine sets them: `movestatus` walk
    /// or run by his speed (110 in/s), `direction` by his keys, not "run" for any movement.
    #[test]
    fn the_torso_asks_by_walk_or_run_and_the_keys() {
        let mut p = soldier(2);
        let at = |p: &PlayerSim| torso::split(context_of(p));
        assert_eq!(at(&p), (0, 0, 0, false), "standing still, no keys");
        p.body.vel = Vec3::new(crate::units::inches(60.0), 0.0, 0.0);
        p.input.move_packed = crate::input::pack_move(1.0, 0.0);
        assert_eq!(at(&p), (0, 1, 1, false), "walking forward");
        p.body.vel = Vec3::new(-crate::units::inches(150.0), 0.0, 0.0);
        p.input.move_packed = crate::input::pack_move(-1.0, 0.0);
        assert_eq!(at(&p), (0, 2, 2, false), "running backward");
        p.input.move_packed = crate::input::pack_move(0.0, -1.0);
        assert_eq!(at(&p).2, 3, "strafing left");
    }

    /// His jump and landing play the script's whole-body clips; a hit his flinch (not over his
    /// reload); his trigger on an empty magazine his dry fire at him, once per pull.
    #[test]
    fn jumps_landings_flinches_and_dry_fire() {
        let _g = crate::ffi::state_lock();
        crate::torso::install(TABLE);
        let mut players = vec![None, Some(soldier(2))];
        {
            let p = players[1].as_mut().unwrap();
            p.body.on_ground = true;
            let a = &mut p.arsenal;
            (a.raise_left, a.drop_left) = (0.0, 0.0);
        }
        let mut r = Remote::default();
        let mut c = Cues::default();
        r.update(&players, 0, 0.0, &mut c);
        {
            let b = &mut players[1].as_mut().unwrap().body;
            b.on_ground = false;
            b.vel.z = 100.0;
        }
        r.update(&players, 0, 1.0 / 30.0, &mut c);
        assert_eq!(r.torso(1).map(|t| t.action), Some(torso::JUMP));
        players[1].as_mut().unwrap().body.on_ground = true;
        r.update(&players, 0, 1.0 / 30.0, &mut c);
        assert_eq!(r.torso(1).map(|t| t.action), Some(torso::LAND));
        // A flinch does not cut the landing; after it ends it plays.
        let p = players[1].clone().unwrap();
        r.event(1, &EventKind::Damaged { amount: 1.0, dir: Vec3::ZERO }, Some(&p), true, &mut c);
        assert_eq!(r.torso(1).map(|t| t.action), Some(torso::LAND));
        for _ in 0..20 {
            r.update(&players, 0, 1.0 / 30.0, &mut c);
        }
        // The flinch by the side the blow came from (`dmgDirection`): from in front (it travels
        // against his facing), from the left, and with no direction from behind
        // (`finishplayerdamage`'s rule).
        let yaw = p.yaw;
        let along = |deg: f32| Vec3::new((yaw + deg.to_radians()).cos(), (yaw + deg.to_radians()).sin(), 0.0);
        for (dir, want) in [(along(180.0), torso::FLINCH), (along(270.0), torso::FLINCH_LEFT), (Vec3::ZERO, torso::FLINCH_BACK)] {
            for _ in 0..20 {
                r.update(&players, 0, 1.0 / 30.0, &mut c);
            }
            r.event(1, &EventKind::Damaged { amount: 1.0, dir }, Some(&p), true, &mut c);
            assert_eq!(r.torso(1).map(|t| t.action), Some(want), "{dir:?}");
        }
        c.queue.clear();
        // Dry fire: once per pull, only on an empty magazine.
        {
            let p = players[1].as_mut().unwrap();
            let w = p.arsenal.current_mut();
            w.ammo = 0;
            p.input.held |= crate::input::buttons::FIRE;
        }
        for _ in 0..3 {
            r.update(&players, 0, 1.0 / 30.0, &mut c);
        }
        let dry: Vec<_> = played(&mut c).into_iter().filter(|(e, _)| e.contains("dryfire")).collect();
        assert_eq!(dry.len(), 1, "{dry:?}");
    }

    /// BO2's battle chatter (`_battlechatter_mp.gsc`): a reload says `reloading_gen` at its odds
    /// (not with a launcher), a frag `attack_grenade`, a Betty at the trophy odds; never with Dead
    /// Silence; every machine rolls the same.
    #[test]
    fn battle_chatter_lines_and_odds() {
        let d = |id: &str| WeaponDef { id: id.into(), ..Default::default() };
        assert_eq!(sounds::chatter_for(&EventKind::ReloadStarted, &d("an94_mp+reflex")), Some(("reloading_gen", sounds::CHATTER_RELOAD)));
        assert_eq!(sounds::chatter_for(&EventKind::ReloadStarted, &d("smaw_mp")), None);
        assert_eq!(sounds::chatter_for(&EventKind::Thrown { lethal: true }, &d("frag_grenade_mp")), Some(("attack_grenade", sounds::CHATTER_TOSS)));
        assert_eq!(sounds::chatter_for(&EventKind::Planted { kind: crate::equipment::Kind::Betty }, &d("bouncingbetty_mp")), Some(("attack_c4", sounds::CHATTER_TROPHY)));
        assert!(sounds::THROW_CHATTER.iter().all(|t| sounds::CHATTER_LINES.contains(&t.1)));
        // The reload line plays about 40 % of the time (`>= 60` of 0..100), the same on every machine.
        let said = (0..1000).filter(|&n| sounds::chatter_roll(1, n) >= sounds::CHATTER_RELOAD).count();
        assert!((340..460).contains(&said), "{said}");
        assert_eq!(sounds::chatter_roll(3, 17), sounds::chatter_roll(3, 17));
        let mut p = soldier(2);
        p.arsenal.weapons[0] = WeaponState::new(Box::leak(Box::new(WeaponDef { id: "an94_mp".into(), ..Default::default() })));
        p.arsenal.active = 0;
        let mut r = Remote::default();
        let mut c = Cues::default();
        for _ in 0..50 {
            r.event(1, &EventKind::ReloadStarted, Some(&p), true, &mut c);
        }
        let lines: Vec<_> = played(&mut c).into_iter().filter(|(e, _)| e.starts_with("ZHBO_BO2_Bc_")).collect();
        assert!(!lines.is_empty() && lines.len() < 50 && lines.iter().all(|(e, _)| e.ends_with("_reloading_gen")), "{lines:?}");
        p.arsenal.loadout.perks.push(crate::weapons::Perk::by_id("DeadSilence").expect("Dead Silence"));
        for _ in 0..50 {
            r.event(1, &EventKind::ReloadStarted, Some(&p), true, &mut c);
        }
        assert!(played(&mut c).iter().all(|(e, _)| !e.starts_with("ZHBO_BO2_Bc_")), "Dead Silence says nothing");
    }

    /// Overhead, Zero Hour fires his gun and the sim spends the round: the gun's shot torso.
    #[test]
    fn an_overhead_generals_spent_round_is_a_shot() {
        let _g = crate::ffi::state_lock();
        crate::torso::install(TABLE);
        let mut players = vec![None, Some(soldier(2))];
        players[1].as_mut().unwrap().mode = crate::sim::Mode::Rts;
        let mut r = Remote::default();
        let mut c = Cues::default();
        r.update(&players, 0, 0.0, &mut c);
        r.update(&players, 0, 1.0 / 30.0, &mut c);
        assert_eq!(r.torso(1), None, "no round spent, no shot");
        players[1].as_mut().unwrap().arsenal.rts_shot(0);
        r.update(&players, 0, 1.0 / 30.0, &mut c);
        assert_eq!(r.torso(1).map(|t| (t.action, t.clip)), Some((torso::FIRE, 4103)));
    }

    /// His death: the script's `DEATH` clip for how he was hit and from where, his gait and keys,
    /// played on his body through to the corpse (held, the gun dropped), until Zero Hour removes
    /// the body.
    #[test]
    fn a_killed_general_falls_by_the_scripts_death() {
        let _g = crate::ffi::state_lock();
        use crate::death::DmgType;
        let head_front = torso::death_context(0, 0, 0, DmgType::HeadShot as u8, 1);
        let shotgun_left = torso::death_context(0, 0, 0, DmgType::NormalShotgun as u8, 3);
        let bash_back = torso::death_context(0, 0, 0, DmgType::MeleeBash as u8, 2);
        let run_back = torso::death_context(0, 2, 2, DmgType::Normal as u8, 2);
        let default = torso::death_context(0, 0, 0, DmgType::Normal as u8, 2);
        crate::torso::install(&format!(
            "death {head_front} 8000 8001\ndeath {shotgun_left} 8003\ndeath {bash_back} 8004\ndeath {run_back} 8005\ndeath {default} 8002\n\
             torso_clip 8000 1.5 pb_death_headshot_front\ntorso_clip 8001 1.5 pb_death_headshot_front_v2\ntorso_clip 8002 2.0 pb_stand_death_neckdeath\n\
             torso_clip 8003 1.5 pb_death_shotgun_left\ntorso_clip 8004 1.5 pb_death_shieldbash_back\ntorso_clip 8005 1.5 pb_death_run_back\n"
        ));
        let mut p = soldier(2);
        p.yaw = 0.0;
        let players = vec![None, Some(p.clone())];
        // The sim reports the blow after the death when his seat steps after his killer's (the
        // step's blows are scored once every seat has stepped): either order falls the same.
        let fall_in = |players: &[Option<PlayerSim>], killed: Option<EventKind>, report_first: bool| {
            let mut r = Remote::default();
            let mut c = Cues::default();
            r.update(players, 0, 0.0, &mut c);
            let report = |r: &mut Remote, c: &mut Cues| {
                if let Some(k) = killed {
                    r.event(1, &k, None, true, c);
                }
            };
            if report_first {
                report(&mut r, &mut c);
            }
            r.event(1, &EventKind::GeneralDied { general: 2 }, None, true, &mut c);
            if !report_first {
                report(&mut r, &mut c);
            }
            r.update(&[None, None], 0, 0.0, &mut c);
            r
        };
        let fall = |players: &[Option<PlayerSim>], killed: Option<EventKind>| {
            let (a, b) = (fall_in(players, killed, true), fall_in(players, killed, false));
            assert_eq!(a.corpses(), b.corpses(), "{killed:?}");
            a
        };
        // Shot in the head by a General in front of him (+X): the blow travels -X.
        let r = fall(&players, Some(EventKind::KilledBy { dmg: DmgType::HeadShot, dir: Vec3::new(-1.0, 0.0, 0.0), by: None, how: crate::damage::MeansOfDeath::Unknown, weapon: None }));
        let corpse = r.corpses()[0];
        assert!(corpse.clip == 8000 || corpse.clip == 8001, "{corpse:?}");
        assert!(r.torsos().any(|a| a.general == 2 && a.action == torso::DEATH));
        // A shotgun from his left (+Y: the pellets travel -Y), the shield's bash from behind.
        let shot = fall(&players, Some(EventKind::KilledBy { dmg: DmgType::NormalShotgun, dir: Vec3::new(0.0, -1.0, 0.0), by: None, how: crate::damage::MeansOfDeath::Unknown, weapon: None }));
        assert_eq!(shot.corpses()[0].clip, 8003);
        let bash = fall(&players, Some(EventKind::KilledBy { dmg: DmgType::MeleeBash, dir: Vec3::new(1.0, 0.0, 0.0), by: None, how: crate::damage::MeansOfDeath::Unknown, weapon: None }));
        assert_eq!(bash.corpses()[0].clip, 8004);
        // A death to anything else: the stance's default.
        assert_eq!(fall(&players, None).corpses()[0].clip, 8002);
        // Running backward (BO2 reads the keys for `direction`, the speed for `movestatus`).
        let mut runner = p.clone();
        runner.body.vel = Vec3::new(-crate::units::inches(150.0), 0.0, 0.0);
        runner.input.move_packed = crate::input::pack_move(-1.0, 0.0);
        let r = fall(&[None, Some(runner)], None);
        assert_eq!(r.corpses()[0].clip, 8005);
        // The corpse lies until Zero Hour's body is gone.
        let mut r = r;
        let mut c = Cues::default();
        let none: Vec<Option<PlayerSim>> = vec![None, None];
        for _ in 0..(CORPSE_TIME * 10.0) as usize + 2 {
            r.update(&none, 0, 0.1, &mut c);
        }
        assert!(r.corpses().is_empty());
    }
}
