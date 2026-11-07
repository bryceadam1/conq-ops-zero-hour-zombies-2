//! Footsteps, jumps and landings by the surface under the feet: BO2's own sounds for them.
//!
//! **The steps** fall on the bob cycle ([`crate::footsteps`]: two per cycle, the movement's
//! [`Step`]) and play the cell of BO2's footstep tables (`zhbo_formats::footstep`, the MP map
//! zones' `FootstepTableDef`s) for that movement on the surface: t6mp `FUN_004cb480` takes
//! `default_1st_person` for the player's own, `default_3rd_person` for everyone else's, the
//! `_quiet` tables for a player with Dead Silence (`specialty_quieter`) and
//! `default_3rd_person_loud` for an enemy's steps when the listener has Awareness
//! (`specialty_loudenemies`); both perks at once play the plain third-person table.
//!
//! **The jump and the landing** are `clientscripts/mp/_footsteps.csc`'s: `playerjump` plays
//! `fly_step_run_<plr|npc>_<surface>`, `playerland` `fly_land_<plr|npc>_<surface>` (whatever the
//! perks).
//!
//! The converter makes one event per table, movement and surface ([`step_event`]) and per
//! surface for the jump and landing ([`jump_event`], [`land_event`]). Presentation only: read off
//! the sim's records (its bob cycle, stance, speed, perks), and the surface is the sim's own
//! (`LogicHost::surface_at`).

use std::sync::OnceLock;

use crate::footsteps::{self, Step};
use crate::impacts::Surface;
use crate::math::Vec3;
use crate::sim::PlayerSim;
use crate::weapons::specialty::Specialty;

/// BO2's footstep tables the engine registers (`FUN_005031c0`), as the client plays them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepTable {
    First = 0,
    Third = 1,
    FirstQuiet = 2,
    ThirdQuiet = 3,
    ThirdLoud = 4,
}

impl StepTable {
    pub const ALL: [StepTable; 5] = [StepTable::First, StepTable::Third, StepTable::FirstQuiet, StepTable::ThirdQuiet, StepTable::ThirdLoud];

    /// The table's asset name in the map zones.
    pub fn bo2_name(self) -> &'static str {
        ["default_1st_person", "default_3rd_person", "default_1st_person_quiet", "default_3rd_person_quiet", "default_3rd_person_loud"][self as usize]
    }

    /// The player's own (2D), else heard at the stepper.
    pub fn first_person(self) -> bool {
        matches!(self, StepTable::First | StepTable::FirstQuiet)
    }

    fn tag(self) -> &'static str {
        ["1st", "3rd", "1stq", "3rdq", "3rdl"][self as usize]
    }

    /// `FUN_004cb480`'s choice: the stepper's own steps or another's, his Dead Silence, and an
    /// enemy heard with Awareness (`loud`).
    pub fn pick(first_person: bool, quiet: bool, loud: bool) -> StepTable {
        match (first_person, quiet, loud) {
            (true, false, _) => StepTable::First,
            (true, true, _) => StepTable::FirstQuiet,
            (false, false, true) => StepTable::ThirdLoud,
            (false, true, false) => StepTable::ThirdQuiet,
            (false, _, _) => StepTable::Third,
        }
    }
}

/// The surfaces a foot lands on (every [`Surface`] but flesh; a flesh ground steps as default).
pub const SURFACES: [Surface; 11] = [
    Surface::Default,
    Surface::Metal,
    Surface::Concrete,
    Surface::Dirt,
    Surface::Grass,
    Surface::Asphalt,
    Surface::Sand,
    Surface::Snow,
    Surface::Rock,
    Surface::Wood,
    Surface::Water,
];

/// `s` as a footstep surface (flesh steps as default).
pub fn ground(s: Surface) -> Surface {
    if s == Surface::Flesh { Surface::Default } else { s }
}

fn surface_index(s: Surface) -> usize {
    SURFACES.iter().position(|&x| x == ground(s)).unwrap_or(0)
}

/// Every event name, built once: the steps (`table × step × surface`), then the jumps and the
/// landings (`[plr, npc] × surface`).
fn names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut v: Vec<&'static str> = Vec::new();
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        for t in StepTable::ALL {
            for st in Step::ALL {
                for s in SURFACES {
                    v.push(leak(format!("ZHBO_BO2_Step_{}_{}_{}", t.tag(), st.name(), s.name())));
                }
            }
        }
        for kind in ["Jump", "Land"] {
            for who in ["plr", "npc"] {
                for s in SURFACES {
                    v.push(leak(format!("ZHBO_BO2_{kind}_{who}_{}", s.name())));
                }
            }
        }
        v
    })
}

/// The event of a step in `table` of movement `step` on surface `s`.
pub fn step_event(table: StepTable, step: Step, s: Surface) -> &'static str {
    names()[(table as usize * Step::ALL.len() + step as usize) * SURFACES.len() + surface_index(s)]
}

fn csc_event(kind: usize, first_person: bool, s: Surface) -> &'static str {
    let base = StepTable::ALL.len() * Step::ALL.len() * SURFACES.len();
    names()[base + ((kind * 2 + !first_person as usize) * SURFACES.len()) + surface_index(s)]
}

/// The jump's event (`playerjump`: `fly_step_run_<plr|npc>_<surface>`).
pub fn jump_event(first_person: bool, s: Surface) -> &'static str {
    csc_event(0, first_person, s)
}

/// The landing's event (`playerland`: `fly_land_<plr|npc>_<surface>`).
pub fn land_event(first_person: bool, s: Surface) -> &'static str {
    csc_event(1, first_person, s)
}

/// The jump and landing events and the alias each plays, as `_footsteps.csc` builds them
/// (`buildandcachesoundalias`: `fly_<step_run|land>_<plr|npc>_<surface>`). The steps' aliases come
/// from the footstep tables, which the converter reads.
pub fn csc_aliases() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for (k, word) in ["step_run", "land"].iter().enumerate() {
        for first in [true, false] {
            for s in SURFACES {
                out.push((csc_event(k, first, s), format!("fly_{word}_{}_{}", if first { "plr" } else { "npc" }, s.name())));
            }
        }
    }
    out
}

/// A landing needs at least this downward speed (Zero Hour units/s) to be heard.
const LAND_SPEED: f32 = 20.0;

/// One General's feet as the client heard them last: his bob cycle, whether he was in the air
/// (and his fastest fall).
#[derive(Clone, Copy, Debug, Default)]
pub struct Feet {
    general: u32,
    bob: Option<f32>,
    airborne: bool,
    fall: f32,
}

impl Feet {
    /// This frame's sounds of `p`'s feet (`table` picks the step sounds), at most a jump or a
    /// landing and a step; `surface` is the ground under him.
    pub fn update(&mut self, p: &PlayerSim, table: StepTable, surface: Surface, out: &mut Vec<&'static str>) {
        if self.general != p.general {
            *self = Feet { general: p.general, airborne: !p.body.on_ground, ..Default::default() };
        }
        let first = table.first_person();
        let cycle = p.gun.bob_cycle();
        if p.body.on_ground {
            if self.airborne && self.fall > LAND_SPEED {
                out.push(land_event(first, surface));
            }
            self.airborne = false;
            self.fall = 0.0;
        } else {
            if !self.airborne && p.body.vel.z > 0.0 {
                out.push(jump_event(first, surface));
            }
            self.airborne = true;
            self.fall = self.fall.max(-p.body.vel.z);
        }
        let t = &p.arsenal.tuning;
        let xyspeed = p.body.horizontal_speed() / crate::units::ZH_PER_INCH;
        if self.bob.is_some_and(|old| footsteps::stepped(old, cycle, xyspeed, t)) {
            out.push(step_event(table, footsteps::step_of(p.body.stance, p.body.sprinting, xyspeed, t), surface));
        }
        self.bob = Some(cycle);
    }
}

/// Other players' footsteps: each first-person General but the presented one, heard at him from
/// the third-person tables (`_footsteps.csc`'s `firstperson = 0`), and his jump and landing too.
#[derive(Clone, Debug, Default)]
pub struct RemoteSteps {
    seats: [Option<Feet>; crate::sim::MAX_PLAYERS],
}

impl RemoteSteps {
    /// This frame's footfalls of every seat's first-person General but `presented`'s. `listener`
    /// is the presented General with his Awareness; `enemy(stepper, listener)` says whether a
    /// General is the listener's enemy; `surface` is the ground at a point.
    pub fn update(&mut self, players: &[Option<PlayerSim>], presented: i32, enemy: &dyn Fn(u32, u32) -> bool, surface: &dyn Fn(Vec3) -> Surface, cues: &mut crate::sounds::Cues) {
        let listener = usize::try_from(presented).ok().and_then(|i| players.get(i)).and_then(|p| p.as_ref());
        let aware = listener.is_some_and(|l| l.arsenal.has_specialty(Specialty::LoudEnemies));
        let mut heard = Vec::new();
        for (seat, slot) in self.seats.iter_mut().enumerate() {
            let p = players.get(seat).and_then(|p| p.as_ref()).filter(|p| seat as i32 != presented && p.is_first_person());
            let Some(p) = p else {
                *slot = None;
                continue;
            };
            let quiet = p.arsenal.has_specialty(Specialty::Quieter);
            let loud = aware && listener.is_some_and(|l| enemy(p.general, l.general));
            let feet = slot.get_or_insert_with(Feet::default);
            heard.clear();
            feet.update(p, StepTable::pick(false, quiet, loud), surface(p.body.pos), &mut heard);
            for e in &heard {
                cues.play_at(e, p.body.pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every event name is distinct, and each lookup lands on its own.
    #[test]
    fn the_events_are_one_per_table_step_and_surface() {
        let mut all: Vec<&str> = names().to_vec();
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n);
        assert_eq!(step_event(StepTable::ThirdQuiet, Step::CrouchRun, Surface::Concrete), "ZHBO_BO2_Step_3rdq_crouch_run_concrete");
        assert_eq!(step_event(StepTable::First, Step::Sprint, Surface::Flesh), "ZHBO_BO2_Step_1st_sprint_default");
        assert_eq!(land_event(true, Surface::Metal), "ZHBO_BO2_Land_plr_metal");
        assert_eq!(jump_event(false, Surface::Water), "ZHBO_BO2_Jump_npc_water");
        assert!(csc_aliases().contains(&("ZHBO_BO2_Land_plr_metal", "fly_land_plr_metal".to_string())), "the landing is fly_land_plr_<surface>, not the dive-to-prone's");
    }

    /// A walker on the ground with the forward key held, his bob cycle advanced by the sim's own
    /// rule (`viewmodel::Motion`) each 30 Hz frame.
    fn walk(p: &mut PlayerSim, frames: usize, mut each: impl FnMut(&PlayerSim)) {
        p.input.move_packed = crate::input::pack_move(1.0, 0.0);
        p.body.vel = Vec3::new(crate::units::inches(190.0), 0.0, 0.0);
        for _ in 0..frames {
            let mut gun = std::mem::take(&mut p.gun);
            gun.update(1.0 / 30.0, 0.0, 0.0, 65.0, p, None);
            p.gun = gun;
            each(p);
        }
    }

    /// The own steps fall on the bob cycle (BO2's 2.6 a second at a full run), each the run's cell
    /// of the first-person table on the ground's surface; Dead Silence takes the quiet table.
    #[test]
    fn the_own_steps_follow_the_bob_cycle_and_the_surface() {
        let mut p = PlayerSim::spawn_for_tests();
        let mut feet = Feet::default();
        let mut heard = Vec::new();
        walk(&mut p, 300, |p| feet.update(p, StepTable::First, Surface::Metal, &mut heard));
        assert_eq!(heard.len(), 26, "10 s of running: {heard:?}");
        assert!(heard.iter().all(|e| *e == step_event(StepTable::First, Step::Run, Surface::Metal)), "{heard:?}");
        let mut p = PlayerSim::spawn_for_tests();
        p.arsenal.loadout.perks.push(crate::weapons::Perk::by_id("DeadSilence").expect("Dead Silence"));
        let mut c = crate::sounds::Cues::default();
        walk(&mut p, 30, |p| c.update(1.0 / 30.0, p, false, Surface::Grass));
        assert!(c.queue.iter().any(|r| r.event == step_event(StepTable::FirstQuiet, Step::Run, Surface::Grass)), "{:?}", c.queue);
    }

    /// Another player's steps are heard at him from the third-person tables: plain, quiet with his
    /// Dead Silence, loud to a listener with Awareness when he is an enemy.
    #[test]
    fn remote_steps_pick_the_listeners_table() {
        let mut listener = PlayerSim::spawn_for_tests();
        listener.general = 1;
        let mut walker = PlayerSim::spawn_for_tests();
        walker.general = 2;
        let run = |listener: &PlayerSim, walker: &mut PlayerSim, enemies: bool| {
            let mut steps = RemoteSteps::default();
            let mut c = crate::sounds::Cues::default();
            walk(walker, 30, |w| steps.update(&[Some(listener.clone()), Some(w.clone())], 0, &|_, _| enemies, &|_| Surface::Concrete, &mut c));
            c.queue.iter().map(|r| r.event).collect::<Vec<_>>()
        };
        let plain = run(&listener, &mut walker.clone(), true);
        assert!(!plain.is_empty() && plain.iter().all(|e| *e == step_event(StepTable::Third, Step::Run, Surface::Concrete)), "{plain:?}");
        listener.arsenal.loadout.perks.push(crate::weapons::Perk::by_id("Awareness").expect("Awareness"));
        assert!(run(&listener, &mut walker.clone(), true).iter().all(|e| *e == step_event(StepTable::ThirdLoud, Step::Run, Surface::Concrete)));
        assert!(run(&listener, &mut walker.clone(), false).iter().all(|e| *e == step_event(StepTable::Third, Step::Run, Surface::Concrete)), "an ally's steps stay plain");
        walker.arsenal.loadout.perks.push(crate::weapons::Perk::by_id("DeadSilence").expect("Dead Silence"));
        assert!(run(&listener, &mut walker.clone(), true).iter().all(|e| *e == step_event(StepTable::Third, Step::Run, Surface::Concrete)), "both perks: plain");
        listener.arsenal.loadout.perks.clear();
        assert!(run(&listener, &mut walker.clone(), true).iter().all(|e| *e == step_event(StepTable::ThirdQuiet, Step::Run, Surface::Concrete)));
    }

    /// `FUN_004cb480`'s table choice.
    #[test]
    fn the_perks_pick_the_table() {
        assert_eq!(StepTable::pick(true, false, true), StepTable::First);
        assert_eq!(StepTable::pick(true, true, false), StepTable::FirstQuiet);
        assert_eq!(StepTable::pick(false, false, false), StepTable::Third);
        assert_eq!(StepTable::pick(false, true, false), StepTable::ThirdQuiet);
        assert_eq!(StepTable::pick(false, false, true), StepTable::ThirdLoud);
        assert_eq!(StepTable::pick(false, true, true), StepTable::Third, "Dead Silence against Awareness: the plain steps");
    }
}
