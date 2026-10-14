//! Unit conversion between Black Ops 2 (inches, 100 HP soldiers) and Zero Hour world units.
//!
//! Zero Hour infantry use `GeometryHeight = 12`, and a BO2 player stands 72 inches tall, so one ZH
//! world unit is treated as 6 inches. Every BO2 distance/speed in this crate passes through [`inches`].

/// Zero Hour world units per BO2 inch.
pub const ZH_PER_INCH: f32 = 1.0 / 6.0;

/// Zero Hour's fixed logic rate (`LOGICFRAMES_PER_SECOND`).
pub const LOGIC_FPS: f32 = 30.0;
pub const LOGIC_DT: f32 = 1.0 / LOGIC_FPS;

/// Convert a BO2 length (inches) or speed (inches/s) to Zero Hour units.
pub const fn inches(v: f32) -> f32 {
    v * ZH_PER_INCH
}

/// BO2 damage is authored against 100 HP players; a Zero Hour Ranger has 180 HP (`HumanArmor`).
/// Scaling by 1.5 keeps BO2's shots-to-kill (an MTAR body-shot kill takes 3 rounds up close) while
/// ZH armor (SMALL_ARMS against tank armor, etc.) still decides what rifles can hurt.
pub const BO2_TO_ZH_DAMAGE: f32 = 1.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soldier_height_matches_zh_infantry() {
        assert!((inches(72.0) - 12.0).abs() < 1e-4);
    }
}
