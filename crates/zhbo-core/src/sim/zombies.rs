//! BO2 Zombies' round engine (`_zm.gsc` `round_think` / `round_spawning`), on a Zombies map: one
//! with zombie spawn points (`objflags::SPAWNER`, BO2's `zombie_spawn_locations`).
//!
//! Each round: the round's zombie total (`zombies::Vars::zombie_total`) spawns one at a time at
//! a random spawn point (never twice running at the same one, `round_spawning`), every
//! `zombie_spawn_delay` seconds, never more than `zombie_ai_limit` alive; each at the round's
//! health (`ai_calculate_health`) with its run cycle drawn by `set_run_speed`, and sent at the
//! nearest survivor (BO2's `find_flesh`, its closest player). When the total has spawned and every
//! zombie is dead, `round_over` waits `zombie_between_round_time` and the next round begins, its
//! spawn delay and move speed moved on (`round_think`).
//!
//! Kills and hits on zombies pay their player (`_zm_score.gsc` `player_add_points`) into the one
//! wallet Zombies points and Zero Hour cash share ([`LogicHost::add_cash`]).
//!
//! All of it is sim state, stepped once per logic frame in a fixed order and hashed: every
//! machine of a game spawns, sends and pays alike.

use super::*;
use crate::zombies::{RunCycle, Vars};

/// How often an inactive engine looks for spawn points, and an active one re-sends its zombies
/// at the nearest survivor (logic frames).
pub const SCAN_FRAMES: u32 = 30;

/// Where a round is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Phase {
    /// Not a Zombies map (no spawn points seen).
    #[default]
    Off,
    /// `round_over`'s wait before round `round + 1`, `left` seconds to go.
    Between { left: f32 },
    /// Round `round` spawning its total and fighting it out.
    Fighting,
}

crate::statehash::state_hash!(enum Phase { Off, Between { left }, Fighting });

/// The round engine's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Rounds {
    pub phase: Phase,
    /// The round being fought (BO2's `level.round_number`); 0 before the first.
    pub round: u32,
    /// The spawn points (ids, in the engine's order).
    pub spawners: Vec<u32>,
    /// `level.zombie_total`: zombies still to spawn this round.
    pub to_spawn: u32,
    /// The round's zombies alive (ids, spawn order).
    pub alive: Vec<u32>,
    /// Seconds until the next spawn.
    pub next_spawn: f32,
    /// The last spawn point used (`round_spawning`'s `old_spawn`).
    pub last_spawner: u32,
    pub rng: crate::weapons::Rng,
    /// The map is a Zombies map (its objects include a spawn point; the bridge says so as the map
    /// loads, `zhbo_zombies_map_loaded`). Off, the engine never looks for spawn points.
    pub zombies_map: bool,
}

crate::statehash::state_hash!(struct Rounds { phase, round, spawners, to_spawn, alive, next_spawn, last_spawner, rng, zombies_map });

impl Default for Rounds {
    fn default() -> Rounds {
        Rounds { phase: Phase::Off, round: 0, spawners: Vec::new(), to_spawn: 0, alive: Vec::new(), next_spawn: 0.0, last_spawner: 0, rng: crate::weapons::Rng::new(0x5A4D_0001), zombies_map: false }
    }
}

/// BO2's `randomintrange( lo, hi )`: an integer in `[lo, hi)`.
fn random_int_range(rng: &mut crate::weapons::Rng, lo: i32, hi: i32) -> i32 {
    if hi <= lo {
        return lo;
    }
    lo + (rng.next_u32() % (hi - lo) as u32) as i32
}

impl Sim {
    /// The survivors in the game: the seats whose General is a survivor (BO2's `get_players()`).
    fn survivors(&self) -> Vec<&PlayerSim> {
        self.players.iter().flatten().filter(|p| p.survivor).collect()
    }

    /// One logic frame of the round engine (after the seats have stepped).
    pub(super) fn step_zombies(&mut self, host: &dyn LogicHost) {
        if !self.rounds.zombies_map {
            return;
        }
        let vars = Vars::get();
        let scan = self.frame.is_multiple_of(SCAN_FRAMES);
        if matches!(self.rounds.phase, Phase::Off) {
            if scan {
                // The whole map: its extent's centre and half diagonal (a query far past the map
                // overran Zero Hour's partition grid).
                let (lo, hi) = host.extent();
                let mut objs = Vec::new();
                host.query((lo + hi) * 0.5, (hi - lo).length() * 0.5, 0, &mut objs);
                let spawners: Vec<u32> = objs.iter().filter(|o| o.has(objflags::SPAWNER) && !o.has(objflags::DEAD)).map(|o| o.id).collect();
                if !spawners.is_empty() {
                    host.log(&format!("zombies: {} spawn points; round 1 begins", spawners.len()));
                    self.rounds.spawners = spawners;
                    self.start_round(host, &vars, 1);
                }
            }
            return;
        }
        // The round's dead and the vanished leave the list.
        let mut alive = std::mem::take(&mut self.rounds.alive);
        alive.retain(|&id| host.object(id, id).is_some_and(|o| !o.has(objflags::DEAD)));
        self.rounds.alive = alive;
        match self.rounds.phase {
            Phase::Off => {}
            Phase::Between { left } => {
                let left = left - LOGIC_DT;
                if left <= 0.0 {
                    let next = (self.rounds.round + 1).min(255);
                    self.start_round(host, &vars, next);
                } else {
                    self.rounds.phase = Phase::Between { left };
                }
            }
            Phase::Fighting => {
                self.rounds.next_spawn -= LOGIC_DT;
                if self.rounds.to_spawn > 0 && (self.rounds.alive.len() as u32) < vars.ai_limit && self.rounds.next_spawn <= 0.0 {
                    if self.spawn_zombie(host, &vars) {
                        self.rounds.to_spawn -= 1;
                    }
                    self.rounds.next_spawn = vars.spawn_delay_in(self.rounds.round);
                }
                if scan {
                    self.send_zombies(host);
                }
                if self.rounds.to_spawn == 0 && self.rounds.alive.is_empty() {
                    host.log(&format!("zombies: round {} over", self.rounds.round));
                    self.emit_all(EventKind::ZombieRoundOver { round: self.rounds.round as u8 });
                    self.rounds.phase = Phase::Between { left: vars.between_rounds };
                }
            }
        }
    }

    /// Round `round` begins (`round_spawning`'s setup): its total for the survivors in the game.
    fn start_round(&mut self, host: &dyn LogicHost, vars: &Vars, round: u32) {
        let players = self.survivors().len().max(1) as u32;
        self.rounds.round = round;
        self.rounds.to_spawn = vars.zombie_total(round, players);
        self.rounds.next_spawn = 0.0;
        self.rounds.phase = Phase::Fighting;
        host.log(&format!("zombies: round {round}: {} zombies at {} health", self.rounds.to_spawn, vars.health(round)));
        self.emit_all(EventKind::ZombieRound { round: round as u8 });
    }

    /// One zombie (`round_spawning`'s loop body): at a random spawn point (redrawn once if it is
    /// the last one used), its run cycle drawn, its health the round's, sent at the nearest
    /// survivor. False if the engine made none.
    fn spawn_zombie(&mut self, host: &dyn LogicHost, vars: &Vars) -> bool {
        let r = &mut self.rounds;
        r.spawners.retain(|&id| host.object(id, id).is_some());
        if r.spawners.is_empty() {
            return false;
        }
        let pick = |rng: &mut crate::weapons::Rng, n: usize| (rng.next_u32() % n as u32) as usize;
        let mut spawner = r.spawners[pick(&mut r.rng, r.spawners.len())];
        if spawner == r.last_spawner {
            spawner = r.spawners[pick(&mut r.rng, r.spawners.len())];
        }
        r.last_spawner = spawner;
        let speed = vars.move_speed(r.round);
        let cycle = RunCycle::of(random_int_range(&mut r.rng, speed, speed + 35));
        let Some(at) = host.object(spawner, spawner) else { return false };
        let id = host.spawn(cycle.object(), spawner, at.pos, at.yaw);
        if id == 0 {
            return false;
        }
        host.set_max_health(id, vars.health(r.round) * crate::units::BO2_TO_ZH_DAMAGE);
        r.alive.push(id);
        let target = self.nearest_survivor(host, at.pos);
        if let Some(t) = target {
            host.order_attack(id, t);
        }
        true
    }

    /// The nearest living survivor General to `pos` (BO2's `find_flesh`: the closest player).
    fn nearest_survivor(&self, host: &dyn LogicHost, pos: Vec3) -> Option<u32> {
        self.survivors()
            .into_iter()
            .filter_map(|p| host.object(p.general, p.general).filter(|o| !o.has(objflags::DEAD)))
            .min_by(|a, b| (a.pos - pos).length().total_cmp(&(b.pos - pos).length()))
            .map(|o| o.id)
    }

    /// Send every zombie of the round at its nearest survivor again (BO2's zombies re-target as
    /// players move; `find_flesh` runs on).
    fn send_zombies(&mut self, host: &dyn LogicHost) {
        for &id in &self.rounds.alive {
            let Some(z) = host.object(id, id) else { continue };
            if let Some(t) = self.nearest_survivor(host, z.pos) {
                host.order_attack(id, t);
            }
        }
    }

    fn emit_all(&mut self, kind: EventKind) {
        for idx in 0..self.players.len() {
            if self.players[idx].as_ref().is_some_and(|p| p.survivor) {
                self.emit(idx, kind);
            }
        }
    }

    /// A hit of the step on a zombie, paid to its player (`player_add_points`): `"damage"` for a
    /// hit that leaves it standing, `"death"` with the kill bonus for the killing one (melee's,
    /// `player_add_points_kill_bonus`). Hit locations' bonuses need the hit's location, which the
    /// step's hit record does not carry yet.
    pub(super) fn zombie_points(&mut self, host: &dyn LogicHost, shooter: u32, victim: u32, kill: bool, how: crate::damage::MeansOfDeath) {
        if !host.object(victim, victim).is_some_and(|o| o.has(objflags::ZOMBIE)) {
            return;
        }
        let Some(seat) = self.players.iter().position(|p| p.as_ref().is_some_and(|p| p.general == shooter)) else { return };
        let Some(owner) = host.object(shooter, shooter).map(|o| o.owner_player) else { return };
        let vars = Vars::get();
        let players = self.survivors().len().clamp(1, 4);
        let points = if kill {
            let bonus = if how == crate::damage::MeansOfDeath::Melee { vars.score_bonus_melee } else { 0 };
            vars.score_kill[players - 1] + bonus
        } else {
            vars.score_damage
        };
        host.add_cash(owner, points);
        self.emit(seat, EventKind::ZombiePoints { points: points as u16 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::fakehost::{general, FakeHost};

    fn zombies_map() -> (Sim, FakeHost) {
        let host = FakeHost::default();
        host.spawn_objects.set(true);
        host.add(ObjInfo { flags: objflags::INFANTRY | objflags::GENERAL | objflags::SURVIVOR, ..general(1, 0) });
        host.add(ObjInfo { id: 50, owner_player: 7, flags: objflags::SPAWNER | objflags::NO_COLLIDE, pos: Vec3::new(300.0, 0.0, 0.0), ..Default::default() });
        host.add(ObjInfo { id: 51, owner_player: 7, flags: objflags::SPAWNER | objflags::NO_COLLIDE, pos: Vec3::new(-300.0, 0.0, 0.0), ..Default::default() });
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        sim.take_control(&host, 0, 1);
        (sim, host)
    }

    fn step(sim: &mut Sim, host: &FakeHost, frames: u32) {
        for _ in 0..frames {
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    #[test]
    fn a_map_without_spawn_points_has_no_rounds() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.rounds.zombies_map = true;
        step(&mut sim, &host, SCAN_FRAMES * 2);
        assert_eq!(sim.rounds.phase, Phase::Off);
        assert!(host.spawned.borrow().is_empty());
    }

    #[test]
    fn an_ordinary_map_never_looks_for_spawn_points() {
        let (mut sim, host) = zombies_map();
        sim.rounds.zombies_map = false;
        step(&mut sim, &host, SCAN_FRAMES * 3);
        assert_eq!(sim.rounds.phase, Phase::Off, "not a Zombies map: spawn points or not, no rounds");
    }

    #[test]
    fn round_one_spawns_its_total_at_the_spawn_points_and_sends_them_at_the_survivor() {
        let (mut sim, host) = zombies_map();
        step(&mut sim, &host, SCAN_FRAMES + 1);
        assert_eq!((sim.rounds.round, sim.rounds.phase), (1, Phase::Fighting));
        let vars = Vars::get();
        let total = vars.zombie_total(1, 1);
        // One every spawn delay.
        step(&mut sim, &host, (vars.spawn_delay / LOGIC_DT) as u32 * total + 2);
        let spawned = host.spawned.borrow().clone();
        assert_eq!(spawned.len() as u32, total);
        assert!(host.spawn_owners.borrow().iter().all(|&o| o == 50 || o == 51), "at the spawn points, on their side");
        assert!(spawned.iter().all(|(_, t)| t == "ZHBO_Zombie_Walk"), "round 1: speed 8, every draw under 35 walks");
        let health = Vars::get().health(1) * crate::units::BO2_TO_ZH_DAMAGE;
        assert!(host.healths.borrow().iter().all(|&(_, h)| h == health), "the round's health");
        assert!(host.orders.borrow().iter().all(|&(_, target)| target == 1), "at the survivor");
        assert_eq!(sim.rounds.to_spawn, 0);
    }

    #[test]
    fn the_next_round_follows_the_last_zombies_death_after_the_wait() {
        let (mut sim, host) = zombies_map();
        let vars = Vars::get();
        step(&mut sim, &host, SCAN_FRAMES + 1 + (vars.spawn_delay / LOGIC_DT) as u32 * vars.zombie_total(1, 1) + 2);
        for (id, _) in host.spawned.borrow().iter() {
            host.objects.borrow_mut().iter_mut().filter(|o| o.id == *id).for_each(|o| o.flags |= objflags::DEAD);
        }
        step(&mut sim, &host, 2);
        assert!(matches!(sim.rounds.phase, Phase::Between { .. }));
        step(&mut sim, &host, (vars.between_rounds / LOGIC_DT) as u32 + 2);
        assert_eq!((sim.rounds.round, sim.rounds.phase), (2, Phase::Fighting));
    }

    #[test]
    fn a_kill_and_a_hit_on_a_zombie_pay_the_shooters_player() {
        let (mut sim, host) = zombies_map();
        host.add(ObjInfo { id: 90, owner_player: 7, flags: objflags::INFANTRY | objflags::ZOMBIE, ..Default::default() });
        sim.zombie_points(&host, 1, 90, false, crate::damage::MeansOfDeath::RifleBullet);
        sim.zombie_points(&host, 1, 90, true, crate::damage::MeansOfDeath::Melee);
        let vars = Vars::get();
        assert_eq!(*host.cash.borrow(), vec![(0, vars.score_damage), (0, vars.score_kill[0] + vars.score_bonus_melee)]);
        // A hit on anything else pays nothing.
        sim.zombie_points(&host, 1, 50, true, crate::damage::MeansOfDeath::RifleBullet);
        assert_eq!(host.cash.borrow().len(), 2);
    }
}
