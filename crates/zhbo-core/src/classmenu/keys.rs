//! The editor's per-frame key edges and the name field's typed characters.

use super::*;

/// A key edge this frame (fresh press): the editor's input is edge-driven like the pick screen.
/// `typed` is the fresh presses of the printable keys (A-Z, 0-9 and Space), in `Key::ALL` order, so
/// the name row can append them; it is empty outside text entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
    pub backspace: bool,
    /// Tab on the class row: BO2's `knifePersonalization` prompt (`caceditclass.lua:52-63`).
    pub personalize: bool,
    /// `C` on the primary weapon row: BO2's `AddCamoButton` (`caceditclass.lua:748`).
    pub camo: bool,
    /// `V` on the primary optic row: BO2's `AddReticleButton` (`caceditclass.lua:825`).
    pub reticle: bool,
    pub typed: Vec<Key>,
}

impl Edges {
    /// The edges in `now` that were not in `prev`.
    pub fn between(prev: &crate::input::KeySet, now: &crate::input::KeySet) -> Edges {
        let edge = |k: Key| now.is_down(k) && !prev.is_down(k);
        let typed = Key::ALL.iter().copied().filter(|k| is_typable(*k) && edge(*k)).collect();
        Edges {
            up: edge(Key::Up),
            down: edge(Key::Down),
            left: edge(Key::Left),
            right: edge(Key::Right),
            confirm: edge(Key::Enter),
            cancel: edge(Key::Escape),
            backspace: edge(Key::Backspace),
            personalize: edge(Key::Tab),
            camo: edge(Key::C),
            reticle: edge(Key::V),
            typed,
        }
    }
}

/// Whether a key types a character into the class-name field: A-Z, 0-9 and Space. `char_of` gives it.
pub(super) fn is_typable(k: Key) -> bool {
    char_of(k).is_some()
}

/// The character a key types into the name field. BO2's class names are uppercase letters, digits
/// and the space: A-Z as uppercase letters, 0-9, and Space.
pub(super) fn char_of(k: Key) -> Option<char> {
    match k {
        Key::Key0 => Some('0'),
        Key::Key1 => Some('1'),
        Key::Key2 => Some('2'),
        Key::Key3 => Some('3'),
        Key::Key4 => Some('4'),
        Key::Key5 => Some('5'),
        Key::Key6 => Some('6'),
        Key::Key7 => Some('7'),
        Key::Key8 => Some('8'),
        Key::Key9 => Some('9'),
        Key::A => Some('A'),
        Key::B => Some('B'),
        Key::C => Some('C'),
        Key::D => Some('D'),
        Key::E => Some('E'),
        Key::F => Some('F'),
        Key::G => Some('G'),
        Key::H => Some('H'),
        Key::I => Some('I'),
        Key::J => Some('J'),
        Key::K => Some('K'),
        Key::L => Some('L'),
        Key::M => Some('M'),
        Key::N => Some('N'),
        Key::O => Some('O'),
        Key::P => Some('P'),
        Key::Q => Some('Q'),
        Key::R => Some('R'),
        Key::S => Some('S'),
        Key::T => Some('T'),
        Key::U => Some('U'),
        Key::V => Some('V'),
        Key::W => Some('W'),
        Key::X => Some('X'),
        Key::Y => Some('Y'),
        Key::Z => Some('Z'),
        Key::Space => Some(' '),
        _ => None,
    }
}
