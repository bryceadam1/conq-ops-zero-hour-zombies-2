//! BO2's assists: who else hurt a player before he was killed.
//!
//! BO2 keeps, per player and per life, every enemy player who damaged him and how much
//! (`_globallogic_player.gsc` `trackattackerdamage`, called from the damage callback for an enemy
//! player's damage only; the list is reset on spawn, `resetattackerlist`). When an enemy player
//! kills him, every other attacker on the killer's team is credited an assist
//! (`playerkilled_awardassists`, only in a team game: `processassist` returns unless
//! `level.teambased`, and never for the victim's own team), scored by the damage he did:
//! `assist_<25|50|75>` for `ceil(damage / 25)` clamped to 1..3 (`_globallogic_score.gsc:1213`).
//! A suicide, a teamkill or a death to the world credits nobody (their `lpattackteam` is the
//! victim's own team or `world`). While the victim's health regenerates, every attacker's damage
//! decays by the health regained (`_healthoverlay.gsc` `decayplayerdamages`), so an old wound
//! scores the lowest assist.
//!
//! Zero Hour's teams are its alliances: an attacker is on the killer's team when Zero Hour calls
//! his General the killer's ally, and a free-for-all (nobody allied) credits no assists, as BO2's
//! free-for-all does not.
//!
//! **Sim state.** The damage is the sim's own (`LogicHost::damage` as a seat's General, recorded
//! by [`Tally`] around the step), so every machine records the same hits in the same order; the
//! attacker lists and each seat's assist count are hashed (`Sim::fingerprint`'s world part).
//! An attacker's last weapon decides the score event (`processassist`'s `switch ( weapon )`):
//! a concussion, flashbang, EMP grenade or Shock Charge as his last hit scores `assist_concussion`,
//! `assist_flash`, `assist_emp` or `assist_proximity` ([`Tactical`]). Those hit a first-person
//! General through the sim's shellshock (`sim::guard`'s shock hits, which name the weapon and its
//! owner: BO2's 1-damage tactical blast, `idamage` floored at 1); every other hit is a plain one.
use super::{EventKind, Sim, MAX_PLAYERS};
use crate::host::{objflags, LogicHost, ObjInfo};
use crate::impacts::Surface;
use crate::math::Vec3;
use std::cell::RefCell;

/// The tactical a hit came from, for the score event it makes ([`Tactical::of`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tactical {
    Concussion,
    Flash,
    Emp,
    Proximity,
}

crate::statehash::state_hash!(enum Tactical { Concussion, Flash, Emp, Proximity });

impl Tactical {
    /// BO2's names (`processassist`): `concussion_grenade_mp`, `flash_grenade_mp`,
    /// `emp_grenade_mp`, `proximity_grenade(_aoe)_mp` — here the def's stun kind: the EMP's
    /// effect, a placed stun (the Shock Charge), the two thrown ones by name (as `sim::shock`).
    pub fn of(def: &crate::weapons::WeaponDef, thrown: bool) -> Option<Tactical> {
        use crate::weapons::Effect;
        match def.effect {
            Effect::Emp => Some(Tactical::Emp),
            Effect::Stun if !thrown || def.equipment_kind == crate::weapons::Kind::Shock => Some(Tactical::Proximity),
            Effect::Stun if def.id == super::shock::FLASH_WEAPON => Some(Tactical::Flash),
            Effect::Stun if def.id == super::shock::CONCUSSION_WEAPON => Some(Tactical::Concussion),
            _ => None,
        }
    }
}

/// One enemy's damage to a General this life (BO2's `attackerdamage[clientid]`, in BO2 health).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attacker {
    pub seat: u8,
    /// The General he hurt him with (his own may have died since: a grenade still in the air).
    pub general: u32,
    pub damage: f32,
    /// His last hit's tactical (BO2 keeps the last weapon): `None` for any other weapon.
    pub tactical: Option<Tactical>,
}

crate::statehash::state_hash!(struct Attacker { seat, general, damage, tactical });

/// A seat's General and the enemies who hurt him, in the order they first did.
#[derive(Clone, Debug, PartialEq)]
pub struct Attacked {
    pub victim: u32,
    pub attackers: Vec<Attacker>,
}

crate::statehash::state_hash!(struct Attacked { victim, attackers });

/// The match's assist bookkeeping.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    /// Each seat's General who has been hurt by an enemy seat this life.
    pub attacked: Vec<Attacked>,
    /// Each seat's assists so far (BO2's `pers["assists"]`): what the scoreboard's ASSISTS shows.
    pub assists: [u32; MAX_PLAYERS],
    /// The Generals each seat has had lately, newest last: a hit from a General whose seat record
    /// is gone (his grenade landed after he died) is still his seat's, as BO2's grenade stays its
    /// thrower's.
    pub generals: [[u32; GENERALS_KEPT]; MAX_PLAYERS],
}

/// How many of a seat's Generals [`Score::generals`] remembers.
pub const GENERALS_KEPT: usize = 4;

crate::statehash::state_hash!(struct Score { attacked, assists, generals });

/// BO2's assist score event for `damage` (BO2 health): `assist_25`, `assist_50` or `assist_75`,
/// as 1, 2 or 3 (`int(ceil(damage / 25))` clamped to 1..3).
pub fn assist_level(damage: f32) -> u8 {
    (damage / 25.0).ceil().clamp(1.0, 3.0) as u8
}

/// One call of `LogicHost::damage` the step made, in call order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub shooter: u32,
    pub victim: u32,
    /// Zero Hour damage.
    pub amount: f32,
    pub kill: bool,
    /// BO2's means of death, weapon and direction (the presentation of the death:
    /// `EventKind::KilledBy`).
    pub blow: crate::damage::Blow,
}

impl Score {
    /// The seat whose General `general` is (now or lately).
    fn seat_of(&self, players: &[Option<super::PlayerSim>], general: u32) -> Option<usize> {
        if general == 0 {
            return None;
        }
        players.iter().position(|p| p.as_ref().is_some_and(|p| p.general == general)).or_else(|| self.generals.iter().position(|g| g.contains(&general)))
    }

    /// Remember each seat's current General.
    fn note_generals(&mut self, players: &[Option<super::PlayerSim>]) {
        for (seat, p) in players.iter().enumerate().take(MAX_PLAYERS) {
            let Some(p) = p else { continue };
            let kept = &mut self.generals[seat];
            if p.general != 0 && kept[GENERALS_KEPT - 1] != p.general {
                kept.rotate_left(1);
                kept[GENERALS_KEPT - 1] = p.general;
            }
        }
    }

    /// Seat `seat`'s General `shooter` hit `victim` for `damage` (BO2 health) with `tactical`:
    /// onto the victim's list, if he is an enemy (BO2's friendly-fire branch tracks none).
    fn track(&mut self, host: &dyn LogicHost, seat: usize, shooter: u32, victim: u32, damage: f32, tactical: Option<Tactical>) {
        if host.object(victim, shooter).is_some_and(|o| o.has(objflags::ALLY)) {
            return;
        }
        let list = match self.attacked.iter().position(|a| a.victim == victim) {
            Some(i) => &mut self.attacked[i],
            None => {
                self.attacked.push(Attacked { victim, attackers: Vec::new() });
                self.attacked.last_mut().expect("just pushed")
            }
        };
        match list.attackers.iter_mut().find(|a| a.seat as usize == seat) {
            Some(a) => {
                a.damage += damage;
                a.general = shooter;
                a.tactical = tactical;
            }
            None => list.attackers.push(Attacker { seat: seat as u8, general: shooter, damage, tactical }),
        }
    }

    /// The victim's health came back by `bo2_health`: each attacker's damage decays by it.
    pub fn decay(&mut self, victim: u32, bo2_health: f32) {
        if let Some(a) = self.attacked.iter_mut().find(|a| a.victim == victim) {
            for at in &mut a.attackers {
                at.damage = (at.damage - bo2_health).max(0.0);
            }
        }
    }

    /// The General's life ended: his attackers are forgotten (BO2 clears `attackers` at death and
    /// the rest at the next spawn).
    pub fn forget(&mut self, victim: u32) {
        self.attacked.retain(|a| a.victim != victim);
    }
}

impl Sim {
    /// Run `step` with the damage it deals recorded, then score it ([`Sim::score_hits`]).
    pub(super) fn scored<R>(&mut self, host: &dyn LogicHost, step: impl FnOnce(&mut Sim, &dyn LogicHost) -> R) -> R {
        let tally = Tally { host, hits: RefCell::new(Vec::new()) };
        let out = step(self, &tally);
        let hits = tally.hits.into_inner();
        self.score_hits(host, &hits);
        out
    }

    /// The step's hits, in order: an enemy seat's damage to a seat's General goes on his attacker
    /// list; a seat's killing blow credits the assists and ends the list.
    pub(super) fn score_hits(&mut self, host: &dyn LogicHost, hits: &[Hit]) {
        self.score.note_generals(&self.players);
        let t = crate::tuning::get();
        for h in hits {
            // Zombies: a hit on a zombie pays its player (`sim/zombies.rs`).
            self.zombie_points(host, h.shooter, h.victim, h.kill, h.blow.how);
            // A player's General: a seat's record holds him (first person or overhead), or held
            // him this step (his seat may have been stepped after the blow that killed him).
            let Some(victim_seat) = self.score.seat_of(&self.players, h.victim) else { continue };
            let shooter = self.score.seat_of(&self.players, h.shooter);
            // Where it came from and what it was, for his hurt and death (presentation).
            let airborne = self.players[victim_seat].as_ref().is_some_and(|p| !p.body.on_ground);
            let dmg = crate::death::DmgType::of(h.blow.how, h.blow.weapon, airborne);
            self.blows.push(super::Struck { victim: h.victim, dir: h.blow.dir, dmg, how: h.blow.how, engine: false });
            // A tactical's own radius damage (the concussion's, the flashbang's, the Shock Charge's 1,
            // t6mp `FUN_0060a480`) is that tactical's hit: BO2 keeps the weapon of each attacker's
            // damage, and `processassist` reads it.
            let tactical = h.blow.weapon.and_then(|w| Tactical::of(w, true));
            if let Some(seat) = shooter.filter(|&s| s != victim_seat) {
                // In BO2 health: his share of `playerMaxHealth` (`trackattackerdamage`'s
                // `idamage`).
                let bo2 = match host.object(h.victim, h.shooter) {
                    Some(o) => crate::damage::to_bo2(&t, &o, h.amount),
                    None => crate::damage::general_bo2(&t, h.amount, self.players[victim_seat].as_ref().map_or(0.0, |p| p.max_health)),
                };
                self.score.track(host, seat, h.shooter, h.victim, bo2, tactical);
                // His Flak Jacket took an explosion: the attacker's marker shows it
                // (`doperkfeedback`, `_globallogic_player.gsc:1166-1179`; presentation).
                let flak = self.players[victim_seat].as_ref().is_some_and(|p| p.arsenal.class_specialties().has(crate::weapons::Specialty::FlakJacket));
                if flak && h.blow.how.is_explosive_damage() {
                    self.emit(seat, EventKind::PerkFeedback { perk: crate::damage::PerkFeedback::FlakJacket });
                }
            }
            if h.kill {
                if let Some(killer) = shooter.filter(|&s| s != victim_seat) {
                    self.award_assists(host, killer, h.shooter, h.victim);
                }
                self.score.forget(h.victim);
                // How he died, for his death animation (presentation: the script's `DEATH`).
                // And for the kill feed: who, with what and how (BO2's `obituary( victim, attacker,
                // weapon, means )`, `_globallogic_player.gsc:1318`).
                self.emit(victim_seat, EventKind::KilledBy { dmg, dir: h.blow.dir, by: shooter.map(|s| s as u8), how: h.blow.how, weapon: h.blow.weapon });
            }
        }
    }

    /// `victim` was killed by seat `killer`'s General `by`: every other attacker on the killer's
    /// side, and not on the victim's, scores an assist.
    fn award_assists(&mut self, host: &dyn LogicHost, killer: usize, by: u32, victim: u32) {
        let Some(list) = self.score.attacked.iter().find(|a| a.victim == victim) else { return };
        let mut credited = Vec::new();
        for a in list.attackers.iter().filter(|a| a.seat as usize != killer) {
            // His General now, else the one he hurt the victim with.
            let his = self.players.get(a.seat as usize).and_then(|p| p.as_ref()).map_or(a.general, |p| p.general);
            let side = |viewer: u32| host.object(his, viewer).or_else(|| host.object(a.general, viewer));
            let with_killer = side(by).is_some_and(|o| o.has(objflags::ALLY));
            let with_victim = side(victim).is_some_and(|o| o.has(objflags::ALLY));
            if with_killer && !with_victim {
                credited.push((a.seat as usize, assist_level(a.damage), a.tactical));
            }
        }
        for (seat, level, tactical) in credited {
            self.score.assists[seat] += 1;
            self.emit(seat, EventKind::Assist { level, tactical });
        }
    }

    /// A tactical's hit on a first-person General (`sim::guard`'s shock hit: the weapon and its
    /// owner) that dealt no damage of its own (the EMP grenade's 0): on his attacker list as that
    /// weapon. One with radius damage was already tracked by it ([`Sim::score_hits`]).
    pub(super) fn score_shock(&mut self, host: &dyn LogicHost, owner: u32, victim: u32, def: &crate::weapons::WeaponDef, thrown: bool) {
        let Some(tactical) = Tactical::of(def, thrown) else { return };
        if def.explosion_inner_damage != 0.0 {
            return;
        }
        if !self.players.iter().flatten().any(|p| p.general == victim) {
            return;
        }
        if let Some(seat) = self.score.seat_of(&self.players, owner).filter(|&s| self.players.get(s).and_then(|p| p.as_ref()).is_none_or(|p| p.general != victim)) {
            self.score.track(host, seat, owner, victim, 1.0, Some(tactical));
            // A flash, concussion or Shock Charge his Tactical Mask took (`doperkfeedback`'s
            // `isflashorstundamage && hastacticalmask`, `_class.gsc:1087-1090`): the attacker's
            // marker shows it (presentation).
            use crate::weapons::Specialty as S;
            let masked = self.players.iter().flatten().find(|p| p.general == victim).is_some_and(|p| {
                let sp = p.arsenal.class_specialties();
                sp.has(S::StunProtection) || sp.has(S::FlashProtection) || sp.has(S::ProximityProtection)
            });
            if masked && tactical != Tactical::Emp {
                self.emit(seat, EventKind::PerkFeedback { perk: crate::damage::PerkFeedback::TacticalMask });
            }
        }
    }
}

/// The step's host with every damage call recorded ([`Hit`]); everything else passes through.
struct Tally<'a> {
    host: &'a dyn LogicHost,
    hits: RefCell<Vec<Hit>>,
}

impl LogicHost for Tally<'_> {
    fn log(&self, msg: &str) {
        self.host.log(msg)
    }
    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.host.ground_height(x, y)
    }
    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.host.is_walkable(x, y)
    }
    fn surface_at(&self, x: f32, y: f32) -> Surface {
        self.host.surface_at(x, y)
    }
    fn extent(&self) -> (Vec3, Vec3) {
        self.host.extent()
    }
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
        self.host.object(id, viewer)
    }
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        self.host.query(center, radius, viewer, out)
    }
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, moving: bool) {
        self.host.set_pose(id, pos, yaw, moving)
    }
    fn set_controlled(&self, id: u32, controlled: bool) {
        self.host.set_controlled(id, controlled)
    }
    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, pos: Vec3) {
        self.host.fire_fx(shooter, fx_weapon, target, pos)
    }
    fn damage(&self, shooter: u32, victim: u32, amount: f32, blow: crate::damage::Blow) -> bool {
        let kill = self.host.damage(shooter, victim, amount, blow);
        self.hits.borrow_mut().push(Hit { shooter, victim, amount, kill, blow });
        kill
    }
    fn last_blow(&self, victim: u32) -> Option<crate::damage::Blow> {
        self.hits.borrow().iter().rev().find(|h| h.victim == victim).map(|h| h.blow)
    }
    fn heal(&self, id: u32, amount: f32) {
        self.host.heal(id, amount)
    }
    fn sim_eye(&self, id: u32) -> Option<f32> {
        self.host.sim_eye(id)
    }
    fn spawn(&self, template: &str, owner: u32, pos: Vec3, yaw: f32) -> u32 {
        self.host.spawn(template, owner, pos, yaw)
    }
    fn destroy(&self, id: u32) {
        self.host.destroy(id)
    }
    fn order_attack(&self, id: u32, target: u32) {
        self.host.order_attack(id, target)
    }
    fn set_max_health(&self, id: u32, health: f32) {
        self.host.set_max_health(id, health)
    }
    fn add_cash(&self, player: i32, amount: i32) {
        self.host.add_cash(player, amount)
    }
    fn spawn_survivor(&self, player: i32, pos: Vec3, yaw: f32) -> u32 {
        self.host.spawn_survivor(player, pos, yaw)
    }
    fn money(&self, player: i32) -> i32 {
        self.host.money(player)
    }
    fn disable(&self, id: u32, kind: i32, seconds: f32) {
        self.host.disable(id, kind, seconds)
    }
    fn fx(&self, name: &str, pos: Vec3) {
        self.host.fx(name, pos)
    }
    fn capture(&self, id: u32, new_owner: u32) {
        self.host.capture(id, new_owner)
    }
    fn smoke(&self, center: Vec3, radius: f32) {
        self.host.smoke(center, radius)
    }
    fn build_options(&self, general: u32) -> Vec<crate::building::BuildOption> {
        self.host.build_options(general)
    }
    fn can_place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> bool {
        self.host.can_place(general, template, pos, angle)
    }
    fn place(&self, general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
        self.host.place(general, template, pos, angle)
    }
    fn work_site(&self, general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        self.host.work_site(general, pos, reach)
    }
    fn work(&self, general: u32, site: u32, rate: f32) -> f32 {
        self.host.work(general, site, rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, spawn, FakeHost};

    /// Four players, two teams of two: seats 0 and 1 against 2 and 3. Generals 100 + seat.
    fn match_of_four() -> (Sim, FakeHost) {
        let host = FakeHost::default();
        for seat in 0..4 {
            host.add(general(100 + seat as u32, seat));
        }
        *host.teams.borrow_mut() = vec![(0, 0), (1, 0), (2, 1), (3, 1)];
        let mut sim = Sim::new();
        for seat in 0..4 {
            spawn(&mut sim, &host, seat, 100 + seat as u32);
        }
        (sim, host)
    }

    fn hit(sim: &mut Sim, host: &FakeHost, shooter: u32, victim: u32, bo2_damage: f32) {
        // A General takes a BO2 hit as its share of his health (`damage::to_zh`).
        let amount = host.object(victim, shooter).map_or(bo2_damage * crate::units::BO2_TO_ZH_DAMAGE, |o| crate::damage::to_zh(&crate::tuning::get(), &o, bo2_damage));
        let blow = crate::damage::Blow { how: crate::damage::MeansOfDeath::RifleBullet, weapon: None, dir: Vec3::new(1.0, 0.0, 0.0) };
        let kill = host.damage(shooter, victim, amount, blow);
        sim.score_hits(host, &[Hit { shooter, victim, amount, kill, blow }]);
    }

    fn assists(sim: &Sim) -> Vec<(i32, u8)> {
        sim.events.iter().filter_map(|e| match e.kind {
            EventKind::Assist { level, .. } => Some((e.player, level)),
            _ => None,
        }).collect()
    }

    /// BO2's rule (`playerkilled_awardassists`, `processassist`): the killer's teammates who hurt
    /// the victim each score one, by `ceil(damage / 25)` clamped to 1..3; the killer does not.
    #[test]
    fn the_killers_teammates_who_hurt_the_victim_score_an_assist() {
        let (mut sim, host) = match_of_four();
        let hp = crate::damage::to_bo2(&crate::tuning::get(), &host.get(102), host.get(102).health);
        hit(&mut sim, &host, 101, 102, 60.0);
        hit(&mut sim, &host, 100, 102, 10.0);
        assert!(assists(&sim).is_empty(), "nobody scores while he lives");
        hit(&mut sim, &host, 100, 102, hp);
        assert_eq!(assists(&sim), [(1, 3)], "seat 1's 60 is assist_75; the killer scores none");
        assert_eq!(sim.score.assists, { let mut a = [0; MAX_PLAYERS]; a[1] = 1; a });
        assert!(sim.score.attacked.iter().all(|a| a.victim != 102), "his list ends with him");
        // His death's presentation: BO2's dmgType of the blow and the way it travelled, and for the
        // kill feed the killer's seat and the means (`obituary`).
        let killed = EventKind::KilledBy { dmg: crate::death::DmgType::Normal, dir: Vec3::new(1.0, 0.0, 0.0), by: Some(0), how: crate::damage::MeansOfDeath::RifleBullet, weapon: None };
        assert!(sim.events.iter().any(|e| e.player == 2 && e.kind == killed), "{:?}", sim.events);
        assert_eq!([assist_level(0.0), assist_level(25.0), assist_level(25.5), assist_level(50.0), assist_level(51.0), assist_level(500.0)], [1, 1, 2, 2, 3, 3]);
    }

    /// No assist for a teammate's damage, the victim's own team, a free-for-all, a suicide or a
    /// death to something that is not a player.
    #[test]
    fn friendly_damage_suicides_and_free_for_alls_score_nothing() {
        // A teammate's damage to his own side is not tracked.
        let (mut sim, host) = match_of_four();
        hit(&mut sim, &host, 103, 102, 40.0);
        assert!(sim.score.attacked.is_empty(), "a teammate's damage is not an attacker's");
        // The victim kills himself: his attackers score nothing.
        hit(&mut sim, &host, 100, 102, 40.0);
        hit(&mut sim, &host, 102, 102, 1000.0);
        assert!(assists(&sim).is_empty(), "a suicide credits nobody");
        // Killed by a non-player (Zero Hour's own tank, id 900): nobody.
        let (mut sim, host) = match_of_four();
        hit(&mut sim, &host, 100, 102, 40.0);
        hit(&mut sim, &host, 900, 102, 1000.0);
        assert!(assists(&sim).is_empty(), "a death to the world credits nobody");
        // A free-for-all: every player is his own team.
        let (mut sim, host) = match_of_four();
        host.teams.borrow_mut().clear();
        hit(&mut sim, &host, 101, 102, 40.0);
        hit(&mut sim, &host, 100, 102, 1000.0);
        assert!(assists(&sim).is_empty(), "nobody is on the killer's team");
    }

    /// Regeneration decays every attacker's damage (`decayplayerdamages`): an old wound scores the
    /// lowest assist. A hit from a General who has since died is still his seat's.
    #[test]
    fn regenerated_health_decays_the_damage_and_a_dead_attackers_hit_counts() {
        let (mut sim, host) = match_of_four();
        hit(&mut sim, &host, 101, 102, 70.0);
        sim.score.decay(102, 60.0);
        // Seat 1's General dies; his seat takes a new one (Generals 101 then 111).
        sim.players[1].as_mut().unwrap().general = 111;
        host.add(general(111, 1));
        sim.score_hits(&host, &[]);
        hit(&mut sim, &host, 100, 102, 1000.0);
        assert_eq!(assists(&sim), [(1, 1)], "70 decayed by 60: assist_25, credited to his seat");
    }

    /// Whole again, he forgets who hurt him (`_healthoverlay.gsc:172-175`: `newhealth >= 1.0` ->
    /// `resetattackerlist`): seat 1's 60 is regenerated away, very hurt a step at a time to full,
    /// and the kill seat 0 lands after it credits nobody. A kill before he is whole still credits
    /// the decayed damage.
    #[test]
    fn regenerating_to_full_forgets_his_attackers() {
        let t = crate::tuning::get();
        let regen = |frames: u32| {
            let (mut sim, host) = match_of_four();
            hit(&mut sim, &host, 101, 102, 60.0);
            assert!(sim.score.attacked.iter().any(|a| a.victim == 102), "seat 1 hurt him");
            for _ in 0..frames {
                let f = sim.frame + 1;
                sim.step(&host, f);
            }
            let whole = host.get(102).health >= host.get(102).max_health - 1e-3;
            sim.events.clear();
            hit(&mut sim, &host, 100, 102, 1000.0);
            (whole, assists(&sim))
        };
        // The delay (at least `veryhurttime` once very hurt), then 10 % a step from 40 %: six steps.
        let full = ((t.regen_delay.max(t.very_hurt_time) + 7.0 * t.regen_step) / crate::units::LOGIC_DT).ceil() as u32 + 2;
        assert_eq!(regen(full), (true, vec![]), "whole again: no assist");
        // Two steps in (60 % of him): the 60 decayed by the 20 regained still scores assist_50.
        let partway = ((t.regen_delay.max(t.very_hurt_time) + 1.5 * t.regen_step) / crate::units::LOGIC_DT).ceil() as u32;
        let (whole, got) = regen(partway);
        assert!(!whole);
        assert_eq!(got, [(1, 2)], "not yet whole: the decayed damage still counts");
    }

    /// A tactical as the attacker's last hit scores its own event (`processassist`'s switch): the
    /// concussion's own radius damage, then the kill by his teammate; a later plain hit makes it a
    /// plain one. A tactical without damage (the EMP's) is tracked by its shock.
    #[test]
    fn a_tactical_as_the_last_hit_scores_its_own_assist() {
        let (mut sim, host) = match_of_four();
        let conc = crate::weapons::catalog().get(super::super::shock::CONCUSSION_WEAPON).expect("the concussion");
        let stun_blow = crate::damage::Blow::new(crate::damage::MeansOfDeath::GrenadeSplash, conc, Vec3::new(1.0, 0.0, 0.0));
        sim.scored(&host, |_, h| {
            h.damage(101, 102, crate::units::BO2_TO_ZH_DAMAGE, stun_blow);
        });
        hit(&mut sim, &host, 100, 102, 1000.0);
        let got: Vec<_> = sim.events.iter().filter_map(|e| match e.kind { EventKind::Assist { level, tactical } => Some((e.player, level, tactical)), _ => None }).collect();
        assert_eq!(got, [(1, 1, Some(Tactical::Concussion))]);
        let (mut sim, host) = match_of_four();
        sim.scored(&host, |_, h| {
            h.damage(101, 102, crate::units::BO2_TO_ZH_DAMAGE, stun_blow);
        });
        hit(&mut sim, &host, 101, 102, 30.0);
        hit(&mut sim, &host, 100, 102, 1000.0);
        assert!(sim.events.iter().any(|e| matches!(e.kind, EventKind::Assist { level: 2, tactical: None })), "his last hit was the gun");
        // A teammate's stun is not tracked; the EMP's shock is; the classification by BO2's names.
        let emp: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { effect: crate::weapons::Effect::Emp, id: "emp_grenade_mp".into(), ..Default::default() }));
        let (mut sim, host) = match_of_four();
        sim.score_shock(&host, 103, 102, emp, true);
        assert!(sim.score.attacked.is_empty());
        sim.score_shock(&host, 101, 102, emp, true);
        assert!(sim.score.attacked.iter().any(|a| a.victim == 102 && a.attackers.iter().any(|x| x.tactical == Some(Tactical::Emp))));
        let def = |effect, id: &str| crate::weapons::WeaponDef { effect, id: id.into(), ..Default::default() };
        use crate::weapons::Effect;
        assert_eq!(Tactical::of(&def(Effect::Stun, "flash_grenade_mp"), true), Some(Tactical::Flash));
        assert_eq!(Tactical::of(&def(Effect::Stun, "proximity_grenade_mp"), false), Some(Tactical::Proximity));
        assert_eq!(Tactical::of(&def(Effect::Emp, "emp_grenade_mp"), true), Some(Tactical::Emp));
        assert_eq!(Tactical::of(&def(Effect::Explode, "frag_grenade_mp"), true), None);
    }

    /// The step's damage reaches the scoring through its host ([`Sim::scored`]), in call order.
    #[test]
    fn the_steps_damage_calls_are_scored() {
        let (mut sim, host) = match_of_four();
        sim.scored(&host, |_, h| {
            let zh = |bo2: f32| crate::damage::to_zh(&crate::tuning::get(), &h.object(102, 0).unwrap(), bo2);
            h.damage(101, 102, zh(40.0), crate::damage::Blow::bare(crate::damage::MeansOfDeath::RifleBullet));
            h.damage(100, 102, zh(1000.0), crate::damage::Blow::bare(crate::damage::MeansOfDeath::GrenadeSplash));
        });
        assert_eq!(assists(&sim), [(1, 2)]);
    }

    /// A kill in the step itself scores whichever seat is stepped first: the victim's record ends
    /// in his own seat's step, which comes after his killer's when his seat is the later one, and
    /// his attacker list must outlive it until the step's blows are scored.
    #[test]
    fn a_kill_in_the_step_scores_when_the_victim_steps_after_his_killer() {
        let (mut sim, host) = match_of_four();
        // Seat 1 hurt seat 2's General (60: assist_75); he now stands at seat 0's knife, nearly dead.
        hit(&mut sim, &host, 101, 102, 60.0);
        for o in host.objects.borrow_mut().iter_mut().filter(|o| o.id == 102) {
            o.pos = Vec3::new(8.0, 0.0, 0.0);
            o.health = 1.0;
        }
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        p.arsenal.raise_left = 0.0;
        sim.events.clear();
        for i in 0..20 {
            let melee = if i == 0 { crate::input::buttons::MELEE } else { 0 };
            sim.input(0, 100, crate::input::FpsInput { pressed: melee, ..Default::default() });
            let f = sim.frame + 1;
            sim.step(&host, f);
        }
        assert!(sim.events.iter().any(|e| e.player == 2 && matches!(e.kind, EventKind::GeneralDied { .. })), "he died in the step");
        assert_eq!(assists(&sim), [(1, 3)], "seat 1's assist");
        assert!(sim.events.iter().any(|e| e.player == 2 && matches!(e.kind, EventKind::KilledBy { dmg: crate::death::DmgType::Melee, .. })));
        assert!(sim.score.attacked.iter().all(|a| a.victim != 102), "his list ends with him");
    }

    /// Every machine scores the same hits the same way: two sims fed one script agree on the
    /// events and the hash.
    #[test]
    fn two_machines_score_the_same_assists() {
        let run = || {
            let (mut sim, host) = match_of_four();
            for (s, v, d) in [(101, 102, 30.0), (100, 103, 20.0), (101, 103, 20.0), (100, 102, 1000.0), (101, 103, 1000.0)] {
                hit(&mut sim, &host, s, v, d);
            }
            (assists(&sim), sim.state_hash())
        };
        let (a, b) = (run(), run());
        assert_eq!(a, b);
        assert_eq!(a.0, [(1, 2), (0, 1)]);
    }
}
