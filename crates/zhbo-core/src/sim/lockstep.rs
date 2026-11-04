//! The sim's lockstep fingerprint: one 64-bit hash of every piece of state a logic step reads and
//! writes, so the peers of a match fed the same replicated message stream can prove they agree,
//! frame by frame. The same hash is the sim's share of Zero Hour's own logic CRC
//! (`zhbo_logic_crc`, host patch 0020: `GameLogic::getCRC` after `MARKER:TheAI`), so a mod-only
//! divergence (ammo, a class, a shellshock, a cooldown) shows as Zero Hour's desync on the next CRC
//! frame instead of only once it moves or hurts an object; and it is what the multiplayer gate
//! (`lockstep_tests.rs`) compares.
//!
//! What is hashed is the deterministic state only, streamed field by field
//! ([`crate::statehash`]): each seat's player record and chosen class (and whether it waits for
//! his next General), the projectiles, the equipment that is down, the smoke screens, the deployed
//! shields, the dropped weapons and the kills not yet read by a death, the assist bookkeeping
//! (`score`: every General's attackers and each seat's assists) and the antilag history (the
//! first-person Generals and shields of the last frames, which decide what a shot hits), plus the
//! frame. What is
//! **not** hashed, each named in its `skip` list:
//! * presentation that legitimately differs per machine: [`Sim::events`] (each client drains it
//!   at its own render cadence, and a headless replay never does) and [`Sim::tracers`] (drained
//!   right after each step and drawn per viewer), and `emitted`, the count of every event ever
//!   emitted (the log's bookkeeping across the event cap: it only grows with `events`, which a
//!   client drains, and nothing in the step reads it), and `antilag_hits`, the log's record of
//!   rewound hits (drained after every step, read by nothing in it);
//! * per-step scratch rebuilt from the hashed state before it is read: [`Sim::shields`] and the
//!   equipment's copies of it and of the guards, the equipment's shock hits, `combat_seen`;
//! * the installed data, which never changes during a game and which the content CRC (the
//!   bridge's mod-content CRC folded into the INI CRC) already proves equal: the tuning,
//!   penetration table, bullet range and hit bodies, the smoke screen's numbers, and the antilag rewind cap (`antilag_frames`,
//!   from the tuning pack).
//!
//! The hash is built from per-seat sub-hashes and one for the shared world, which the CRC log
//! prints, so a desync names the seat (or the world) whose state went apart.
use super::{Sim, MAX_PLAYERS};
use crate::statehash::{Hasher, StateHash};

/// The fingerprint and its parts (`ZhboLogicCrc` on the C side).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// [`Sim::state_hash`]: the frame, every seat's sub-hash and the world's.
    pub hash: u64,
    /// Each seat's player record and chosen class; `None` for a seat with neither.
    pub seats: [Option<u64>; MAX_PLAYERS],
    /// The shared state: projectiles, equipment that is down, smoke screens, deployed shields, the antilag history.
    pub world: u64,
    pub frame: u32,
    /// The sim holds state: a seat is in use or something is in the world. An idle sim (no seat holds a
    /// record or a class: no General was deployed — an overhead General's RTS report is a seat
    /// record too — and nobody picked a class) adds nothing to Zero Hour's CRC, so such a game CRCs
    /// exactly as stock Zero Hour does.
    pub active: bool,
}

impl Sim {
    /// The deterministic state's fingerprint (see the module doc). Two sims that stepped the same
    /// messages agree on it; anything a local presentation call could leave behind does not
    /// enter it.
    pub fn state_hash(&self) -> u64 {
        self.fingerprint().hash
    }

    /// The fingerprint with its per-seat and world parts.
    pub fn fingerprint(&self) -> Fingerprint {
        // Every field by name (no `..`): a new one does not compile until it is hashed or skipped
        // here with its reason (see the module doc).
        let Sim {
            players,
            projectiles,
            equipment,
            smokes,
            smoke: _,
            frame,
            chosen,
            deployed,
            drops,
            kills,
            history,
            score,
            tuning: _,
            bullet_range: _,
            penetration: _,
            hit_body: _,
            events: _,
            tracers: _,
            shields: _,
            combat_seen: _,
            emitted: _,
            antilag_hits: _,
            antilag_frames: _,
        } = self;
        let mut seats = [None; MAX_PLAYERS];
        for (i, seat) in seats.iter_mut().enumerate() {
            let (Some(p), Some(c)) = (players.get(i), chosen.get(i)) else { continue };
            if p.is_some() || c.is_some() {
                let mut h = Hasher::new();
                p.hash(&mut h);
                c.hash(&mut h);
                *seat = Some(h.finish());
            }
        }
        let things = world_hash(projectiles, equipment, smokes, deployed, drops, kills, score);
        // The antilag history joins the world's part, but not the idle test: every step records
        // a frame, an empty one when nobody is in first person.
        let world = {
            let mut h = Hasher::new();
            things.hash(&mut h);
            history.hash(&mut h);
            h.finish()
        };
        let mut h = Hasher::new();
        frame.hash(&mut h);
        seats.hash(&mut h);
        world.hash(&mut h);
        let active = seats.iter().any(Option::is_some) || things != empty_world();
        Fingerprint { hash: h.finish(), seats, world, frame: *frame, active }
    }
}

fn world_hash(
    projectiles: &[(usize, crate::projectiles::Projectile)],
    equipment: &crate::equipment::Equipment,
    smokes: &[crate::smoke::Smoke],
    deployed: &[super::deploy::Deployed],
    drops: &[super::drops::Dropped],
    kills: &[super::drops::Kill],
    score: &super::score::Score,
) -> u64 {
    let mut h = Hasher::new();
    projectiles.hash(&mut h);
    equipment.hash(&mut h);
    smokes.hash(&mut h);
    deployed.hash(&mut h);
    drops.hash(&mut h);
    kills.hash(&mut h);
    score.hash(&mut h);
    h.finish()
}

/// The world's hash with nothing in it.
fn empty_world() -> u64 {
    static EMPTY: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *EMPTY.get_or_init(|| world_hash(&[], &crate::equipment::Equipment::default(), &[], &[], &[], &[], &super::score::Score::default()))
}
