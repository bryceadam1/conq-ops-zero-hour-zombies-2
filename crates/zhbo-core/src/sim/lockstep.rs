//! The sim's lockstep fingerprint: one 64-bit hash of every piece of state a logic step reads and
//! writes, so two peers fed the same replicated message stream can prove they agree, frame by
//! frame (the two-peer harness in `lockstep_tests.rs`, and the in-engine CRC hook that
//! docs/CONTRACT.md "Multiplayer" proposes).
//!
//! What is hashed is the deterministic state only: the player records, the chosen classes, the
//! projectiles, the equipment, the smoke screens and the deployed shields, plus the frame. What is
//! **not** hashed is presentation that legitimately differs per machine: [`Sim::events`] (each
//! client drains it at its own render cadence, and a headless replay never does) and
//! [`Sim::tracers`] (drained right after each step and drawn per viewer). The tuning, penetration
//! table and hit bodies come from the installed data and never change during a game; the content
//! check (the bridge's mod-content CRC) covers them.
//!
//! The hash runs over the state's `Debug` form, so a field added to any of these types is covered
//! without anyone remembering to add it here; `f32`'s `Debug` prints the shortest string that
//! round-trips the exact bits, so two states hash equal only when every float is bit-identical
//! (`-0.0` and `0.0` differ; NaN payloads do not, which no sim value carries).
use super::Sim;
use std::fmt::Write;

/// FNV-1a over the bytes written to it.
struct Fnv(u64);

impl Write for Fnv {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
        Ok(())
    }
}

impl Sim {
    /// The deterministic state's fingerprint (see the module doc). Two sims that stepped the same
    /// messages agree on it; anything a local presentation call could leave behind does not
    /// enter it.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        // A write to the hasher cannot fail.
        let _ = write!(
            h,
            "{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.frame,
            self.players,
            self.chosen,
            self.projectiles,
            self.equipment.placed,
            self.smokes,
            self.deployed,
            self.shields,
        );
        h.0
    }
}
