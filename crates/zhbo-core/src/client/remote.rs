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

    /// Every General in a torso action: `(General, clip, seconds in)`.
    pub fn torsos(&self) -> impl Iterator<Item = (u32, u16, f32)> + '_ {
        self.seats.iter().filter_map(|s| s.torso.map(|t| (s.general, t.clip, t.t)))
    }

    fn start(&mut self, seat: usize, def: &WeaponDef, action: u8, context: u8) {
        let Some(s) = self.seats.get_mut(seat) else { return };
        if let Some(clip) = torso::pick_for(def, action, context) {
            s.torso = Some(Torso { clip, t: 0.0 });
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
                    started = Some(Torso { clip, t: 0.0 });
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
