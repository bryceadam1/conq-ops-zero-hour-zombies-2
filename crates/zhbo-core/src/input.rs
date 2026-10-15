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
    pub const ALL: u32 = (1 << 12) - 1;
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
