//! Converters that read Black Ops II content from the player's own install.
//!
//! Output is private to the player: it is written into their local Conquer Ops folder and must never
//! be redistributed. Sound banks and `.ipak` image packs are only compressed; fastfiles are
//! decrypted by `zhbo_formats::fastfile` from the player's own install.

pub mod hud;
pub mod images;
pub mod sounds;
pub mod xf;
pub mod character;
pub mod generals;
pub mod ini;
pub mod paks;
pub mod textures;
pub mod viewmodels;
pub mod fx;
pub mod sfx;
pub mod weapons;
