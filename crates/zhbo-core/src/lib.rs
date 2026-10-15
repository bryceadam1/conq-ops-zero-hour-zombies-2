//! zhbo-core: the Rust half of "Conquer Ops", a Zero Hour mod that lets the player build their
//! General and fight as them in first person, Black Ops 2-style.
//!
//! * [`sim`] runs inside Zero Hour's deterministic lockstep logic (movement, weapons, hitscan).
//! * [`client`] runs per rendered frame (mouse look, camera, input messages, HUD).
//! * [`ffi`] is the C ABI the engine glue in `host/zhbo/` calls; [`host`] is the reverse direction.

pub mod attachments;
pub mod client;
pub mod conformance;
pub mod equipment;
pub mod ffi;
pub mod hitscan;
pub mod host;
pub mod hud;
pub mod input;
pub mod math;
pub mod movement;
pub mod projectiles;
pub mod sim;
pub mod sounds;
pub mod strings;
pub mod units;
pub mod viewmodel;
pub mod weapons;

/// The renderer's C ABI (`zhgfx_*`) is part of this static library in wgpu builds.
#[cfg(feature = "renderer")]
pub use zhbo_gfx;
