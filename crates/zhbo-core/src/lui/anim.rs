//! LUI's animation states: an element registers named states (`registerAnimationState("show",
//! {alphaMultiplier = 1})`) and moves to one over a duration (`animateToState("show", ms, easeIn,
//! easeOut)`, or `beginAnimation` + setters), with a `transition_complete_<state>` event when it
//! lands (`patch_mp/ui/lui/luielement.lua`). A [`Tween`] is one such move of one value, sampled by
//! the screen's own clock (menu and HUD time, never the sim's).
//!
//! The curve for each `easeIn`/`easeOut` combination is engine-side (Ghidra query G8); until it
//! is answered [`ease`] is linear for every combination, and that one function is all G8 changes.

/// One value moving between two states.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    /// When the move started, in the caller's clock (seconds).
    pub start: f32,
    /// LUI's duration (milliseconds; 0 snaps).
    pub duration_ms: f32,
    pub ease_in: bool,
    pub ease_out: bool,
}

impl Tween {
    /// A value resting at `v`.
    pub const fn at(v: f32) -> Tween {
        Tween { from: v, to: v, start: 0.0, duration_ms: 0.0, ease_in: false, ease_out: false }
    }

    /// Move from wherever this tween is at `now` to `to` over `duration_ms` (LUI's
    /// `animateToState` interrupting a running move starts from the current value).
    pub fn animate_to(&mut self, now: f32, to: f32, duration_ms: f32, ease_in: bool, ease_out: bool) {
        *self = Tween { from: self.value(now), to, start: now, duration_ms, ease_in, ease_out };
    }

    /// Progress 0..1 at `now`.
    pub fn progress(&self, now: f32) -> f32 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }
        ((now - self.start) * 1000.0 / self.duration_ms).clamp(0.0, 1.0)
    }

    pub fn value(&self, now: f32) -> f32 {
        let t = ease(self.progress(now), self.ease_in, self.ease_out);
        self.from + (self.to - self.from) * t
    }

    /// Whether the move has landed (LUI's `transition_complete_<state>`).
    pub fn done(&self, now: f32) -> bool {
        self.progress(now) >= 1.0
    }
}

/// LUI's easing for a progress `t` in 0..1. Linear for every flag combination until G8 gives
/// the engine's curve (see the module doc).
pub fn ease(t: f32, _ease_in: bool, _ease_out: bool) -> f32 {
    t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_over_the_duration_and_lands() {
        // `hud/ammocounter.lua`: the low-ammo pulse runs 500 ms between alpha 1 and 0.5.
        let mut a = Tween::at(1.0);
        a.animate_to(10.0, 0.5, 500.0, false, false);
        assert_eq!(a.value(10.0), 1.0);
        assert_eq!(a.value(10.25), 0.75);
        assert!(!a.done(10.4));
        assert_eq!(a.value(10.6), 0.5);
        assert!(a.done(10.5));
    }

    #[test]
    fn an_interrupted_move_starts_where_it_is() {
        let mut a = Tween::at(0.0);
        a.animate_to(0.0, 1.0, 1000.0, false, false);
        a.animate_to(0.5, 0.0, 1000.0, false, false);
        assert_eq!(a.value(0.5), 0.5);
        assert_eq!(a.value(1.0), 0.25);
        let mut snap = Tween::at(0.2);
        snap.animate_to(3.0, 0.9, 0.0, true, true);
        assert_eq!(snap.value(3.0), 0.9, "zero duration snaps");
    }
}
