//! Converters that read Black Ops II content from the player's own install.
//!
//! Output is private to the player: it is written into their local Conquer Ops folder and must never
//! be redistributed. Sound banks and `.ipak` image packs are only compressed; fastfiles are
//! decrypted by `zhbo_formats::fastfile` from the player's own install.

pub mod cac;
pub mod camo;
pub mod conformance;
pub mod hud;
pub mod images;
pub mod sounds;
pub mod xf;
pub mod character;
pub mod generals;
pub mod impacts;
mod hipfire_gate;
mod lockstep_gate;
mod health_gate;
pub mod ini;
pub mod emblems;
pub mod paks;
pub mod rangefinder;
pub mod reticles;
pub mod textures;
pub mod viewmodels;
pub mod fonts;
pub mod native;
pub mod native_audit;
pub mod thirdperson;
pub mod playeranim;
pub mod viewmodel_sweep;
pub mod flicker;
pub mod fx;
pub mod detail;
pub mod fx_preview;
pub mod shotfx;
pub mod mixer;
pub mod sfx;
pub mod manifest;
pub mod screenshot;
pub mod shield;
pub mod props;
pub mod weapons;
pub mod toughness;
#[cfg(test)]
mod content_tests;
#[cfg(test)]
mod test_catalog;
