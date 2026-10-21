//! Unit conversion between Black Ops 2 (inches, 100 HP soldiers) and Zero Hour world units.
//!
//! Zero Hour infantry use `GeometryHeight = 12`, and a BO2 player stands 72 inches tall, so one ZH
//! world unit is treated as 6 inches. Every BO2 distance/speed in this crate passes through [`inches`].
//!
//! The factor itself is defined once in `zhbo_native::ZH_PER_INCH`, the leaf crate both the
//! converter and this sim depend on: the art scale is the same number (`ART_SCALE`), so a converted
//! body and the sim's eye height land on the same world point (Track R7's third person).

/// Zero Hour world units per BO2 inch (one source: `zhbo_native::ZH_PER_INCH`).
pub const ZH_PER_INCH: f32 = zhbo_native::ZH_PER_INCH;

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

/// The General is drawn this much bigger than BO2's scale (`Scale` in `ZZZ_ZHBO_General.ini`) so
/// they stand as tall as Zero Hour's infantry (12 units × 1.15 ≈ 13.8; stock infantry models run
/// 14.3–16 units to the top of the head over a 12–13 `GeometryHeight` cylinder); the first-person
/// eye is raised to match, so the camera stays on the converted body's eye line.
pub const GENERAL_SCALE: f32 = 1.15;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soldier_height_matches_zh_infantry() {
        assert!((inches(72.0) - 12.0).abs() < 1e-4);
    }

    #[test]
    fn the_sim_and_the_art_share_one_factor() {
        // The world-space boundary: a BO2 length through the sim and through the converter's art
        // scale is the same Zero Hour length — no ~9% gap for third person to trip over.
        assert_eq!(ZH_PER_INCH, zhbo_native::ART_SCALE);
        assert!((inches(72.0) - 72.0 * zhbo_native::ART_SCALE).abs() < 1e-6);
        // BO2's standing eye (the tuning pack's 60") and the model's eye are the same point: the
        // first-person camera sits on a converted body's eye line, not above it.
        let eye = inches(60.0) * GENERAL_SCALE;
        let body_eye = 60.0 * zhbo_native::ART_SCALE * GENERAL_SCALE;
        assert!((eye - body_eye).abs() < 1e-5);
        assert!((eye - 11.5).abs() < 1e-4, "60 in at 1/6, the General's 1.15: {eye}");
    }
}

