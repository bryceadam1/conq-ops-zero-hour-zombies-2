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
}

#[derive(Clone, Debug, Default)]
struct Seat {
    general: u32,
    /// His eye last frame: where his sounds play (his death's too, after the record is gone).
    eye: Vec3,
    weapon: sounds::WeaponCues,
    torso: Option<Torso>,
}

/// Every seat's third-person presentation ([`self`]).
#[derive(Clone, Debug, Default)]
pub struct Remote {
    seats: [Seat; MAX_PLAYERS],
}

/// The torso context of a General now: his stance, whether he is moving, whether he aims.
fn context_of(p: &PlayerSim) -> u8 {
    torso::context(p.stance_index() as u8, p.body.horizontal_speed() > 0.2, p.arsenal.ads > 0.5)
}

impl Remote {
    /// Forget everyone (a new game).
    pub fn reset(&mut self) {
        *self = Remote::default();
    }

    /// Seat `seat`'s General's torso, if he is in an action.
    pub fn torso(&self, seat: usize) -> Option<Torso> {
        self.seats.get(seat).and_then(|s| s.torso)
    }

    /// Every General in a torso action, for the body pass.
    pub fn torsos(&self) -> impl Iterator<Item = crate::nativebody::ffi::TorsoAction> + '_ {
        self.seats.iter().filter_map(|s| s.torso.map(|t| crate::nativebody::ffi::TorsoAction { general: s.general, action: t.action, clip: t.clip, t: t.t }))
    }

    fn start(&mut self, seat: usize, def: &WeaponDef, action: u8, context: u8) {
        let Some(s) = self.seats.get_mut(seat) else { return };
        if let Some(clip) = torso::pick_for(def, action, context) {
            s.torso = Some(Torso { action, clip, t: 0.0 });
        }
    }

    /// One of seat `seat`'s sim events, before they are presented and drained (`p`: his record).
    /// `heard`: another player's (his third-person sounds play here at him).
    pub fn event(&mut self, seat: usize, kind: &EventKind, p: Option<&PlayerSim>, heard: bool, cues: &mut Cues) {
        // His voice: BO2 plays it on his head for everyone, himself included.
        match *kind {
            // Hurt while not at full health already (BO2 notifies `snd_pain_player` only when
            // `self.health != self.maxhealth` as the damage arrives, `_globallogic_player.gsc:790`).
            EventKind::Damaged { amount } => {
                if let Some(p) = p.filter(|p| p.health + amount < p.max_health - 1e-3) {
                    cues.play_at(sounds::voice_event(sounds::Voice::Pain, crate::nativebody::ffi::faction_of(p.general), seat), p.body.eye());
                }
            }
            EventKind::GeneralDied { general } => {
                // Where he was last seen: the record is gone with him.
                if let Some(s) = self.seats.get(seat).filter(|s| s.general != 0) {
                    let general = if general != 0 { general } else { s.general };
                    cues.play_at(sounds::voice_event(sounds::Voice::Death, crate::nativebody::ffi::faction_of(general), seat), s.eye);
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
                self.start(seat, offhand(lethal), torso::PRIME, ctx);
                npc(cues, offhand(lethal), NpcSound::Pullback);
            }
            EventKind::Thrown { lethal } => self.start(seat, offhand(lethal), torso::THROW, ctx),
            // A shot's recoil, unless a longer action is on (BO2 fires from none of them).
            EventKind::Shot { left: false, .. } | EventKind::Launched => {
                let busy = self.seats.get(seat).and_then(|s| s.torso).is_some_and(|t| torso::get().pick(held.torso_anims, torso::FIRE, ctx) != Some(t.clip));
                if !busy {
                    self.start(seat, held, torso::FIRE, ctx);
                }
            }
            _ => {}
        }
    }

    /// This frame: every seat's torso runs on (its notetracks heard at him, except for the
    /// presented first-person seat), his weapon change starts its torso and plays its
    /// third-person sounds, and where his eye is is remembered.
    pub fn update(&mut self, players: &[Option<PlayerSim>], presented: i32, dt: f32, cues: &mut Cues) {
        let table = torso::get();
        for (seat, s) in self.seats.iter_mut().enumerate() {
            let Some(p) = players.get(seat).and_then(|p| p.as_ref()) else {
                s.torso = None;
                s.general = 0;
                continue;
            };
            if p.general != s.general {
                *s = Seat { general: p.general, ..Default::default() };
            }
            s.eye = p.body.eye();
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
                    started = Some(Torso { action, clip, t: 0.0 });
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
        torso_clip 4097 0.9 p_knife_stand_rifle_swipe\ntorso_clip 4098 0.9 pt_grenade_stand_throw\ntorso_clip 4099 0.5 pt_rifle_stand_raise\n";

    fn def() -> &'static WeaponDef {
        Box::leak(Box::new(WeaponDef {
            id: "test_remote_mp".into(),
            torso_anims: 7,
            npc_sounds: vec![
                ("melee_swipe".into(), "ZHBO_BO2_SndNpc_wpn_melee_whoosh_npc".into()),
                ("melee_hit".into(), "ZHBO_BO2_SndNpc_wpn_melee_knife_hit_body".into()),
                ("pullback".into(), "ZHBO_BO2_SndNpc_wpn_semtex_charge".into()),
                ("raise".into(), "ZHBO_BO2_SndNpc_fly_generic_raise_npc".into()),
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
        assert_eq!(r.torso(1), Some(Torso { action: torso::RELOAD, clip: 4096, t: 0.0 }));
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
        r.event(1, &EventKind::Damaged { amount: 30.0 }, Some(&p), true, &mut c);
        assert!(played(&mut c).is_empty(), "from full health: no pain");
        r.event(1, &EventKind::Damaged { amount: 10.0 }, Some(&p), true, &mut c);
        assert_eq!(played(&mut c), [(sounds::voice_event(sounds::Voice::Pain, crate::nativebody::ffi::faction_of(2), 1), Some(eye))]);
        r.event(1, &EventKind::GeneralDied { general: 2 }, None, true, &mut c);
        let death = played(&mut c);
        assert_eq!(death, [(sounds::voice_event(sounds::Voice::Death, crate::nativebody::ffi::faction_of(2), 1), Some(eye))]);
        assert!(death[0].0.starts_with("ZHBO_BO2_PainDeath_") && death[0].0.ends_with('1'), "seat 1's voice");
    }
}
