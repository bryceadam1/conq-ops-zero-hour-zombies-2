//! The General as a builder. Under RTS control he builds and repairs like a dozer or worker (his
//! faction's build menu, a dozer's AI), only more slowly. In first person he places a structure with
//! his hammer and builds it, or repairs a damaged one, by holding the use key at it, as a BO2
//! Zombies player rebuilds a barrier.

use crate::math::Vec3;

/// His building and repairing speed under RTS control, as a share of a dozer's (Zero Hour's
/// `DozerAIUpdate`; repairs at his own `RepairHealthPercentPerSecond`).
pub const GENERAL_BUILD_RATE: f32 = 0.5;
/// In first person, holding the use key at it: a dozer's speed (he is right there, exposed).
pub const FPS_BUILD_RATE: f32 = 1.0;
/// How far from a structure's footprint he can build it (Zero Hour units, from his centre: his
/// body is 10 across the radius, so 15 to spare).
pub const REACH: f32 = 25.0;
/// How far ahead the hammer places a structure (its centre): no nearer than `PLACE_MIN`, and
/// `PLACE_CLEAR` clear of its footprint, so it is not on top of him.
pub const PLACE_RANGE: f32 = 220.0;
pub const PLACE_MIN: f32 = 50.0;
pub const PLACE_CLEAR: f32 = 15.0;
/// Seconds to bring the hammer up (BO2's knife pullout) before it can place.
pub const HAMMER_RAISE: f32 = crate::viewmodel::HAMMER_PULLOUT_TIME;
/// How far one notch of the mouse wheel turns the structure being placed (radians: 15 degrees).
pub const ROTATE_STEP: f32 = std::f32::consts::PI / 12.0;

/// One structure the General can build (his build menu's), as the host describes it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildOption {
    /// Zero Hour's template id.
    pub template: u32,
    pub cost: i32,
    /// [`status`].
    pub status: i32,
    /// Its footprint's bounding radius.
    pub radius: f32,
    /// Display name, UTF-8, NUL-terminated.
    pub name: [u8; 48],
}

impl Default for BuildOption {
    fn default() -> Self {
        BuildOption { template: 0, cost: 0, status: status::LOCKED, radius: 0.0, name: [0; 48] }
    }
}

impl BuildOption {
    pub fn name(&self) -> String {
        name_of(&self.name)
    }
}

/// Whether a [`BuildOption`] can be placed now.
pub mod status {
    pub const OK: i32 = 0;
    /// Not enough money.
    pub const NO_MONEY: i32 = 1;
    /// Missing a prerequisite, or otherwise not buildable.
    pub const LOCKED: i32 = 2;
    /// (A refused placement only) no room there.
    pub const BLOCKED: i32 = 3;
    /// (Work only) another builder is already repairing it.
    pub const BUSY: i32 = 4;
}

/// What he can do at a [`Site`].
pub mod work {
    /// Build a structure under construction.
    pub const BUILD: i32 = 0;
    /// Repair a damaged structure.
    pub const REPAIR: i32 = 1;
}

/// How the HUD words each kind of [`work`].
pub struct WorkText {
    /// "Hold [F] to {prompt} ...".
    pub prompt: &'static str,
    /// Over the progress bar.
    pub progress: &'static str,
    pub done: &'static str,
}

/// By [`work`] kind.
pub const WORK_TEXT: [WorkText; 2] = [
    WorkText { prompt: "build", progress: "BUILDING", done: "Construction complete" },
    WorkText { prompt: "repair", progress: "REPAIRING", done: "Repair complete" },
];

pub fn work_text(kind: i32) -> &'static WorkText {
    usize::try_from(kind).ok().and_then(|k| WORK_TEXT.get(k)).unwrap_or(&WORK_TEXT[0])
}

/// A structure in his reach he can work on, as the host describes it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub id: u32,
    pub template: u32,
    /// [`work`].
    pub kind: i32,
    /// How far along: built, or (repairing) its health, 0..100.
    pub percent: f32,
    /// Display name, UTF-8, NUL-terminated.
    pub name: [u8; 48],
}

impl Default for Site {
    fn default() -> Self {
        Site { id: 0, template: 0, kind: work::BUILD, percent: 0.0, name: [0; 48] }
    }
}

impl Site {
    pub fn name(&self) -> String {
        name_of(&self.name)
    }
}

fn name_of(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// The hammer, out in first person: which of his build options is chosen, and how far it is up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hammer {
    pub choice: usize,
    /// The choice was made (it starts on the first structure he can build now).
    pub picked: bool,
    pub raise: f32,
    /// How far the mouse wheel turned it from facing him (radians).
    pub turn: f32,
    /// Where the chosen structure would go now, and which way it would face.
    pub at: Vec3,
    pub angle: f32,
}

/// Where the hammer places a structure: the ground the line of sight meets (from `eye` along unit
/// `dir`), [`PLACE_MIN`] to [`PLACE_RANGE`] ahead and clear of a footprint of `radius`; looking up
/// past that range places it at the range's end.
pub fn place_point(eye: Vec3, dir: Vec3, radius: f32, ground: impl Fn(f32, f32) -> f32) -> Vec3 {
    const STEP: f32 = 4.0;
    let flat = Vec3::new(dir.x, dir.y, 0.0).normalized_or_zero();
    let at = |ahead: f32| {
        let near = PLACE_MIN.max(radius + PLACE_CLEAR);
        let p = eye + flat * ahead.clamp(near, PLACE_RANGE.max(near));
        Vec3::new(p.x, p.y, ground(p.x, p.y))
    };
    let mut t = STEP;
    while t <= PLACE_RANGE * 2.0 {
        let p = eye + dir * t;
        if p.z <= ground(p.x, p.y) {
            return at((p - eye).length_xy());
        }
        t += STEP;
    }
    at(PLACE_RANGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_where_the_line_of_sight_meets_the_ground() {
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let p = place_point(eye, Vec3::new(1.0, 0.0, -0.1).normalized_or_zero(), 0.0, |_, _| 0.0);
        assert!((p.x - 100.0).abs() < 5.0 && p.z == 0.0, "{p:?}");
        let far = place_point(eye, Vec3::new(1.0, 0.0, 0.2).normalized_or_zero(), 0.0, |_, _| 0.0);
        assert!((far.x - PLACE_RANGE).abs() < 1.0, "looking up: the range's end {far:?}");
        let near = place_point(eye, Vec3::new(0.1, 0.0, -1.0).normalized_or_zero(), 0.0, |_, _| 0.0);
        assert!((near.x - PLACE_MIN).abs() < 0.01, "looking down: not on top of him {near:?}");
        let big = place_point(eye, Vec3::new(0.1, 0.0, -1.0).normalized_or_zero(), 80.0, |_, _| 0.0);
        assert!((big.x - 80.0 - PLACE_CLEAR).abs() < 0.01, "a big one clear of him {big:?}");
    }
}
