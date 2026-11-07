//! zhbo-core: the Rust half of "Conquer Ops", a Zero Hour mod that lets the player build their
//! General and fight as them in first person, Black Ops 2-style.
//!
//! * [`sim`] runs inside Zero Hour's deterministic lockstep logic (movement, weapons, hitscan).
//! * [`client`] runs per rendered frame (mouse look, camera, input messages, HUD).
//! * [`ffi`] is the C ABI the engine glue in `host/zhbo/` calls; [`host`] is the reverse direction.

/// A presentation global the core publishes and the render pass reads (the third-person stowed
/// weapons and torso actions, the viewmodel's overlay, shock and reticle choices). In the game it is
/// one process-wide static. Under `cfg(test)` each test thread gets its own, so a test that publishes
/// and then draws is not raced by another test's client frame publishing on a parallel thread (the
/// body test `the_shield_rides_the_left_hand_and_the_back` failed intermittently that way).
macro_rules! presentation_global {
    ($(#[$meta:meta])* $vis:vis fn $name:ident() -> $ty:ty = $init:expr;) => {
        $(#[$meta])*
        #[cfg(not(test))]
        $vis fn $name() -> &'static $ty {
            static S: $ty = $init;
            &S
        }
        $(#[$meta])*
        #[cfg(test)]
        $vis fn $name() -> &'static $ty {
            thread_local! { static S: &'static $ty = Box::leak(Box::new($init)); }
            S.with(|s| *s)
        }
    };
}

pub mod attachments;
pub mod building;
pub mod classes;
pub mod classicons;
pub mod classmenu;
pub mod classrepair;
pub mod classui;
pub mod classwire;
pub mod client;
pub mod config;
pub mod content;
pub mod conformance;
pub mod damage;
pub mod death;
pub mod enginedamage;
pub mod equipment;
pub mod sensor;
pub mod ffi;
pub mod hitloc;
pub mod hitscan;
pub mod host;
pub mod impacts;
pub mod hud;
pub mod input;
pub mod lui;
pub mod math;
pub mod menu;
pub mod names;
pub mod nativebody;
pub mod nativevm;
pub mod penetration;
pub mod movement;
pub mod profile;
pub mod projectiles;
pub mod sim;
pub mod shellshock;
pub mod shield;
pub mod smoke;
pub mod statehash;
pub mod soundlayers;
pub mod torso;
pub mod sounds;
pub mod strings;
pub mod terrain;
pub mod tuning;
pub mod units;
pub mod viewmodel;
pub mod weapons;

/// The renderer's C ABI (`zhgfx_*`) is part of this static library in wgpu builds.
#[cfg(feature = "renderer")]
pub use zhbo_gfx;
