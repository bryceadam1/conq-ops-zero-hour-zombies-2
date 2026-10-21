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
//! * [`pose`]: BO2's `DObj` composition (attached models share the skeleton by tag), sampling a
//!   clip, layering, notetrack crossing and the skinning palette.
//!
//! Depends on nothing (docs/ARCHITECTURE.md): the converter and the core both depend on it.

pub mod anim;
pub mod io;
pub mod math;
pub mod model;
pub mod pose;
pub mod viewmodel;

pub use io::FormatError;

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

/// A BO2 asset name as a file name: BO2 names may start with `,` or carry `/`.
fn file_stem(name: &str) -> String {
    name.trim_start_matches(',').chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_paths_are_stable_file_names() {
        assert_eq!(model_path("viewmodel_usa_seals_arms"), "Art\\Native\\viewmodel_usa_seals_arms.zbm");
        assert_eq!(anim_path(",viewmodel_an94_reload"), "Art\\Native\\viewmodel_an94_reload.zba");
        assert_eq!(viewmodel_path(3, 'U'), "Art\\Native\\ZV03U.zbv");
        assert_eq!(model_path("a/b c"), "Art\\Native\\a_b_c.zbm");
    }
}
