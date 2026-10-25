//! BO2 parity conformance: one shared CSV schema ([`schema`]), the expected side computed from the
//! player's own Black Ops II install by BO2's own rules ([`bo2`], transcribed from the engine —
//! Ghidra answers G14/G15 — and the game's Lua), and the differ ([`diff`]) that joins it with the
//! mod's side (`zhbo-assets::conformance`) and clusters every mismatch.
//!
//! This crate depends on `zhbo-formats` only, so the expected side cannot call the converter or the
//! gameplay core: a rule the mod got wrong cannot leak into the oracle.
//!
//! Development tool only: it reads the player's install and never launches or connects to BO2.

pub mod bo2;
pub mod csv;
pub mod diff;
pub mod schema;
pub mod weapons;
