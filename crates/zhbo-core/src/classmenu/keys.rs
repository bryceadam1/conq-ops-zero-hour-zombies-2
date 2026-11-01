//! The editor's per-frame key edges.

use super::*;

/// A key edge this frame (fresh press): the editor's input is edge-driven like the pick screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
    /// Tab on the class row: BO2's `knifePersonalization` prompt (`caceditclass.lua:52-63`).
    pub personalize: bool,
    /// `C` on a weapon box: BO2's `AddCamoButton` (`caceditclass.lua:748`).
    pub camo: bool,
    /// `V` on the primary optic row: BO2's `AddReticleButton` (`caceditclass.lua:825`).
    pub reticle: bool,
}

impl Edges {
    /// The edges in `now` that were not in `prev`.
    pub fn between(prev: &crate::input::KeySet, now: &crate::input::KeySet) -> Edges {
        let edge = |k: Key| now.is_down(k) && !prev.is_down(k);
        Edges {
            up: edge(Key::Up),
            down: edge(Key::Down),
            left: edge(Key::Left),
            right: edge(Key::Right),
            confirm: edge(Key::Enter),
            cancel: edge(Key::Escape),
            personalize: edge(Key::Tab),
            camo: edge(Key::C),
            reticle: edge(Key::V),
        }
    }
}
