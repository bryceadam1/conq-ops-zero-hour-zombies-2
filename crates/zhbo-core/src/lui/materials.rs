//! The BO2 materials the ported screens draw, by the name BO2's Lua registers them under
//! (`RegisterMaterial("hud_mp_num_big_0")`). The converter pulls each from the player's install as
//! the MappedImage [`image`] names, so a port keeps BO2's names end to end: add the material here
//! and draw `image("<name>")`. A material the install lacks is reported by the converter; its
//! draw falls back like any missing art.

/// The materials to convert, each with the largest side it is kept at (under 1022: the atlas
/// pages are 1024 px with a pixel of padding round each sprite).
pub const MATERIALS: &[(&str, u32)] = &[
    // Create a class (classui): `codmenu.lua`'s large popup frame, `cacchooseclass.lua`'s set pips,
    // `hinttext.lua`'s arrow, the grid buttons' gradients (`customclass.lua`, `growinggridbutton.lua`)
    // and the lock/restricted badges.
    ("menu_mp_cac_backing", 512),
    ("menu_mp_backing_frame", 512),
    ("menu_mp_backing_frame_flipv", 512),
    ("menu_vis_carousel_pip", 32),
    ("menu_vis_carousel_pip_fill", 32),
    ("ui_arrow_right", 32),
    ("menu_mp_cac_grad_stretch", 64),
    ("menu_mp_cac_grad_stretch_add", 64),
    ("menu_mp_cac_grad_stretch_add_v2", 64),
    ("menu_mp_cac_grid_pattern", 16),
    ("lui_bracket", 16),
    ("cac_restricted", 32),
    ("menu_mp_lobby_locked", 32),
    ("menu_mp_lobby_locked_big", 128),
];

/// A [`MATERIALS`] name ending in this is its base material mirrored top to bottom (LUI draws an
/// image upside down when its top edge is below its bottom; the converter writes the mirror).
pub const FLIP_V: &str = "_flipv";

/// The MappedImage a BO2 material converts to.
pub fn image(material: &str) -> String {
    format!("ZHBO_Mat_{material}")
}
