//! zhbo-native: Conquer Ops' own blobs for BO2 art drawn natively, without the W3D conversion.
//!
//! The converter (`zhbo-assets`) reads BO2's models and animations from the player's install and
//! writes them here in the mod's own little-endian, versioned formats; the core (`zhbo-core`) reads
//! them back, poses them as BO2 does and hands the skinned meshes to the renderer (`zhbo-gfx`).
//! Nothing in these files is BO2's own data layout, and nothing converted is ever shipped.
//!
//! * [`model`]: a skinned model (`.zbm`): skeleton, LODs, surfaces with four-bone skin weights, and
//!   materials (blend from BO2's render state, colour map, packed normal/gloss/specular map).
//! * [`anim`]: an animation (`.zba`): per-bone keyframe tracks and notetracks.
//! * [`viewmodel`]: a first-person viewmodel (`.zbv`): which models hang from which tags, the extra
//!   tags the converter adds (`tag_sights`), the bones to hide, and the clip of each BO2 slot.
//! * [`thirdperson`]: a third-person body (`.zbc`: the faction's MP body and one clip per movement
//!   state) and a weapon's world model with its world attachments (`.zbw`).
//! * [`pose`]: BO2's `DObj` composition (attached models share the skeleton by tag), sampling a
//!   clip, layering, notetrack crossing and the skinning palette.
//!
//! Depends on nothing (docs/ARCHITECTURE.md): the converter and the core both depend on it.

pub mod anim;
pub mod io;
pub mod math;
pub mod model;
pub mod pose;
pub mod tga;
pub mod thirdperson;
pub mod viewmodel;

pub use io::FormatError;

/// Zero Hour world units per Black Ops II inch: **1/6**, six inches to a unit, so BO2's 72" player
/// is 12 units tall (Zero Hour's infantry `GeometryHeight = 12`; CONTRACT.md, "Units"). This is the
/// one definition of the two games' scale: the art paths draw with [`ART_SCALE`] and the sim moves
/// and aims with `zhbo_core::units::ZH_PER_INCH`, both reading this number. It lives here because
/// this crate is the leaf the converter and the core both depend on (docs/ARCHITECTURE.md).
pub const ZH_PER_INCH: f32 = 1.0 / 6.0;

/// BO2 inches to the art's world units: [`ZH_PER_INCH`] itself. Art and sim must agree — a BO2
/// body's eye at its inch height and the sim's first-person eye are the same world point, which is
/// what puts a native model on the General in third person (Track R7). An alias, not a second
/// scale.
pub const ART_SCALE: f32 = ZH_PER_INCH;

/// The marker the converter places on every native viewmodel for aiming down the sights: BO2's
/// `tag_sights`, an optic's window, or the gun's front sight (`zhbo_assets::native::aim_marker`).
/// A name of its own, so it never collides with BO2's bones.
pub const AIM_TAG: &str = "tag_zhbo_aim";

/// Where the converter puts the native blobs in the mod's archive (`ConquerOps_BO2.big`).
pub const DIR: &str = "Art\\Native\\";

/// The archive path of a model blob.
pub fn model_path(model: &str) -> String {
    format!("{DIR}{}.zbm", file_stem(model))
}

/// The archive path of an animation blob.
pub fn anim_path(anim: &str) -> String {
    format!("{DIR}{}.zba", file_stem(anim))
}

/// The archive path of viewmodel `vm`'s manifest for faction letter `faction` (`U`, `C`, `G`).
pub fn viewmodel_path(vm: i32, faction: char) -> String {
    format!("{DIR}ZV{vm:02}{faction}.zbv")
}

/// The archive path of faction `faction`'s third-person body manifest (`U`, `C`, `G`).
pub fn body_path(faction: char) -> String {
    format!("{DIR}ZB{faction}.zbc")
}

/// The archive path of converted weapon `vm`'s world-model manifest.
pub fn world_weapon_path(vm: i32) -> String {
    format!("{DIR}ZW{vm:02}.zbw")
}

/// A BO2 asset name as a file name: BO2 names may start with `,` or carry `/`.
fn file_stem(name: &str) -> String {
    name.trim_start_matches(',').chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_factor_for_bo2_inches_and_zh_units() {
        // The sim's contract: six inches to a Zero Hour unit, a 72" player at ZH infantry height.
        assert_eq!(ZH_PER_INCH, 1.0 / 6.0);
        assert_eq!(ART_SCALE, ZH_PER_INCH, "the art scale is the sim's scale, not a second one");
        assert!((72.0 * ART_SCALE - 12.0).abs() < 1e-6, "a 72-inch soldier is 12 units");
        assert!((60.0 * ART_SCALE - 10.0).abs() < 1e-6, "BO2's 60-inch eye is 10 units");
        // Measured on the player's install (`zhbo-assets model ... c_usa_mp_seal6_assault_fb`:
        // bounds z -0.12"..72.15"), the body the converter draws is a 72-inch body.
        assert!((72.14641 * ART_SCALE - 12.0244).abs() < 1e-3);
    }

    #[test]
    fn archive_paths_are_stable_file_names() {
        assert_eq!(model_path("viewmodel_usa_seals_arms"), "Art\\Native\\viewmodel_usa_seals_arms.zbm");
        assert_eq!(anim_path(",viewmodel_an94_reload"), "Art\\Native\\viewmodel_an94_reload.zba");
        assert_eq!(viewmodel_path(3, 'U'), "Art\\Native\\ZV03U.zbv");
        assert_eq!(body_path('U'), "Art\\Native\\ZBU.zbc");
        assert_eq!(world_weapon_path(7), "Art\\Native\\ZW07.zbw");
        assert_eq!(model_path("a/b c"), "Art\\Native\\a_b_c.zbm");
    }
}
