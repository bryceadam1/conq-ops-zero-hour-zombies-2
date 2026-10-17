//! First-person input as it crosses the lockstep network: one [`FpsInput`] per logic frame per
//! player, carried by `GameMessage::MSG_ZHBO_FPS_INPUT`.
//!
//! The move axes are quantised to i8 so every client reconstructs identical floats.

pub mod buttons {
    pub const FIRE: u32 = 1 << 0;
    pub const ADS: u32 = 1 << 1;
    pub const SPRINT: u32 = 1 << 2;
    pub const CROUCH: u32 = 1 << 3;
    pub const PRONE: u32 = 1 << 4;
    pub const JUMP: u32 = 1 << 5;
    pub const RELOAD: u32 = 1 << 6;
    pub const SWAP_WEAPON: u32 = 1 << 7;
    pub const MELEE: u32 = 1 << 8;
    pub const FIELD_COMPUTER: u32 = 1 << 9;
    pub const LETHAL: u32 = 1 << 10;
    pub const TACTICAL: u32 = 1 << 11;
    /// Toggle the weapon's alt mode (select fire, under-barrel launcher, second optic).
    pub const ALT_MODE: u32 = 1 << 12;
    /// Bring the hammer up or put it away (building).
    pub const HAMMER: u32 = 1 << 13;
    /// With the hammer up: turn the structure being placed (the mouse wheel; ADS picks the next one).
    pub const ROTATE_LEFT: u32 = 1 << 14;
    pub const ROTATE_RIGHT: u32 = 1 << 15;
    pub const ALL: u32 = (1 << 16) - 1;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FpsInput {
    /// Buttons held at the time of sending.
    pub held: u32,
    /// Buttons that went down at least once since the previous message (edge latch).
    pub pressed: u32,
    /// Packed move axes, see [`pack_move`].
    pub move_packed: i32,
    /// Absolute view yaw (radians, ZH convention).
    pub yaw: f32,
    /// Absolute view pitch (radians, + is up).
    pub pitch: f32,
}

impl FpsInput {
    pub fn move_axes(&self) -> (f32, f32) {
        unpack_move(self.move_packed)
    }

    pub fn sanitized(mut self) -> Self {
        self.held &= buttons::ALL;
        self.pressed &= buttons::ALL;
        if !self.yaw.is_finite() {
            self.yaw = 0.0;
        }
        if !self.pitch.is_finite() {
            self.pitch = 0.0;
        }
        self.yaw = crate::math::wrap_angle(self.yaw);
        self.pitch = self.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        self
    }
}

/// BO2 clamps view pitch to roughly +-85 degrees.
pub const MAX_PITCH: f32 = 85.0 * std::f32::consts::PI / 180.0;

fn quantize(v: f32) -> i8 {
    (v.clamp(-1.0, 1.0) * 127.0).round() as i8
}

/// Pack forward/right axes in [-1, 1] into one integer message argument.
pub fn pack_move(forward: f32, right: f32) -> i32 {
    let f = quantize(forward) as u8 as i32;
    let r = quantize(right) as u8 as i32;
    (f << 8) | r
}

pub fn unpack_move(packed: i32) -> (f32, f32) {
    let f = ((packed >> 8) & 0xff) as u8 as i8;
    let r = (packed & 0xff) as u8 as i8;
    (f as f32 / 127.0, r as f32 / 127.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_round_trips_exactly_after_quantisation() {
        for &(f, r) in &[(1.0, -1.0), (0.0, 0.0), (-0.5, 0.25), (0.7071, 0.7071)] {
            let (f2, r2) = unpack_move(pack_move(f, r));
            let (f3, r3) = unpack_move(pack_move(f2, r2));
            assert_eq!((f2, r2), (f3, r3), "stable after one round trip");
            assert!((f2 - f).abs() < 0.01 && (r2 - r).abs() < 0.01);
        }
    }

    #[test]
    fn sanitize_clamps_hostile_values() {
        let i = FpsInput { held: u32::MAX, pitch: 10.0, yaw: f32::NAN, ..Default::default() }.sanitized();
        assert_eq!(i.held, buttons::ALL);
        assert_eq!(i.yaw, 0.0);
        assert!((i.pitch - MAX_PITCH).abs() < 1e-6);
    }
}

/// Platform-neutral physical keys (named physical keys, mouse buttons and the wheel). The client
/// input carries a bitset of these instead of Windows virtual-key codes, so the ABI has no
/// platform in it and a platform layer maps its native scancodes straight to `Key` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Space,
    Tab,
    Shift,
    Control,
    MouseLeft,
    MouseRight,
}

impl Key {
    /// Every `Key`, in enum order (the tests walk this to check the default bindings).
    pub const ALL: &'static [Key] = &[
        Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L,
        Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X,
        Key::Y, Key::Z, Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5, Key::Space, Key::Tab,
        Key::Shift, Key::Control, Key::MouseLeft, Key::MouseRight,
    ];

    /// The bit in a `ClientIn::keys` word for this key.
    pub fn bit(self) -> u64 {
        1u64 << self as u8 as usize
    }
}

/// The 256-bit (4x64) key-down set that crosses the ABI.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct KeySet {
    pub bits: [u64; 4],
}

impl KeySet {
    pub fn is_down(&self, k: Key) -> bool {
        self.bits[(k as u8 as usize) / 64] & k.bit() != 0
    }
    pub fn set_down(&mut self, k: Key, down: bool) {
        let bit = k.bit();
        let w = (k as u8 as usize) / 64;
        if down {
            self.bits[w] |= bit;
        } else {
            self.bits[w] &= !bit;
        }
    }
    pub fn clear(&mut self) {
        self.bits = [0; 4];
    }
}

/// The player-facing name of a key (HUD prompts: "[F]", "[TAB]").
pub fn key_name(k: Key) -> &'static str {
    match k {
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::Key1 => "1",
        Key::Key2 => "2",
        Key::Key3 => "3",
        Key::Key4 => "4",
        Key::Key5 => "5",
        Key::Space => "SPACE",
        Key::Tab => "TAB",
        Key::Shift => "SHIFT",
        Key::Control => "CTRL",
        Key::MouseLeft => "LMB",
        Key::MouseRight => "RMB",
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn key_set_round_trips() {
        let mut ks = KeySet::default();
        assert!(!ks.is_down(Key::A));
        ks.set_down(Key::A, true);
        assert!(ks.is_down(Key::A));
        ks.set_down(Key::A, false);
        assert!(!ks.is_down(Key::A));
        ks.set_down(Key::MouseRight, true);
        assert!(ks.is_down(Key::MouseRight));
        ks.clear();
        assert!(!ks.is_down(Key::MouseRight));
    }

    #[test]
    fn key_names_are_distinct() {
        let names: std::collections::HashSet<_> = Key::ALL.iter().map(|k| key_name(*k)).collect();
        assert_eq!(names.len(), Key::ALL.len(), "every key has a unique name");
    }
}
