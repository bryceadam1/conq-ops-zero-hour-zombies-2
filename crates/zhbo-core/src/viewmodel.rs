//! The first-person viewmodel (BO2 arms and weapon): which animation plays, and how far into it.
//!
//! The engine side (`host/zhbo/ZhboViewmodel.cpp`) owns the model and the clips; this decides from
//! the player's state and this frame's events. Idle and sprint loop; the others play once and hold
//! their last frame until the state moves on.

use crate::sim::PlayerSim;

pub const NONE: i32 = -1;
pub const IDLE: i32 = 0;
pub const FIRE: i32 = 1;
pub const RELOAD: i32 = 2;
pub const RELOAD_EMPTY: i32 = 3;
pub const SPRINT: i32 = 4;
pub const RAISE: i32 = 5;

/// How long the fire clip owns the viewmodel after a shot before idle resumes.
const FIRE_HOLD: f32 = 0.3;

/// What happened to the local player's weapon this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    pub shot: bool,
    pub reload_started: bool,
    pub swapped: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Viewmodel {
    pub anim: i32,
    pub t: f32,
}

impl Default for Viewmodel {
    fn default() -> Self {
        Viewmodel { anim: RAISE, t: 0.0 }
    }
}

impl Viewmodel {
    fn play(&mut self, anim: i32) {
        self.anim = anim;
        self.t = 0.0;
    }

    /// Advance by `dt` and return (clip, seconds into it).
    pub fn update(&mut self, dt: f32, p: &PlayerSim, ev: Events) -> (i32, f32) {
        self.t += dt;
        let w = p.arsenal.current();
        if ev.swapped {
            self.play(RAISE);
        }
        if ev.reload_started {
            self.play(if w.ammo == 0 { RELOAD_EMPTY } else { RELOAD });
        }
        if ev.shot {
            self.play(FIRE);
        }
        let done = match self.anim {
            RELOAD | RELOAD_EMPTY => w.reload_left <= 0.0 && self.t > 0.1,
            FIRE => self.t > FIRE_HOLD,
            RAISE => p.arsenal.raise_left <= 0.0 && self.t > 0.1,
            _ => false,
        };
        if done {
            self.play(IDLE);
        }
        if self.anim == IDLE || self.anim == SPRINT {
            let want = if p.body.sprinting && p.arsenal.ads < 0.5 { SPRINT } else { IDLE };
            if want != self.anim {
                self.play(want);
            }
        }
        (self.anim, self.t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Mode, PlayerSim};

    fn player() -> PlayerSim {
        PlayerSim::spawn_for_tests()
    }

    #[test]
    fn plays_the_right_clips() {
        let mut p = player();
        p.mode = Mode::Fps;
        p.arsenal.raise_left = 0.0;
        let mut vm = Viewmodel::default();
        // Raise ends, then idle.
        assert_eq!(vm.update(0.2, &p, Events::default()).0, IDLE);
        // A shot restarts the fire clip; idle comes back after it.
        assert_eq!(vm.update(0.05, &p, Events { shot: true, ..Default::default() }), (FIRE, 0.0));
        assert_eq!(vm.update(0.1, &p, Events::default()).0, FIRE);
        assert_eq!(vm.update(0.3, &p, Events::default()).0, IDLE);
        // Sprinting switches to the sprint loop, unless aiming.
        p.body.sprinting = true;
        assert_eq!(vm.update(0.05, &p, Events::default()).0, SPRINT);
        p.arsenal.ads = 1.0;
        assert_eq!(vm.update(0.05, &p, Events::default()).0, IDLE);
        p.arsenal.ads = 0.0;
        p.body.sprinting = false;
        // An empty reload plays the empty clip and holds while the reload runs.
        p.arsenal.weapons[p.arsenal.active].ammo = 0;
        p.arsenal.weapons[p.arsenal.active].reload_left = 2.0;
        assert_eq!(vm.update(0.05, &p, Events { reload_started: true, ..Default::default() }).0, RELOAD_EMPTY);
        assert_eq!(vm.update(1.0, &p, Events::default()).0, RELOAD_EMPTY);
        p.arsenal.weapons[p.arsenal.active].reload_left = 0.0;
        assert_eq!(vm.update(0.05, &p, Events::default()).0, IDLE);
    }
}
