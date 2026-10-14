//! Converters that read Black Ops II content from the player's own install.
//!
//! Output is private to the player: it is written into their local Conquer Ops folder and must never
//! be redistributed. Nothing here decrypts anything: sound banks and `.ipak` image packs are only
//! compressed, and the encrypted fastfiles are not read.

pub mod hud;
pub mod images;
pub mod sounds;
