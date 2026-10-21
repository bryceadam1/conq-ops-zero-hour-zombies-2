//! The BO2 materials the ported screens draw, by the name BO2's Lua registers them under
//! (`RegisterMaterial("hud_mp_num_big_0")`). The converter pulls each from the player's install as
//! the MappedImage [`image`] names, so a port keeps BO2's names end to end: add the material here
//! and draw `image("<name>")`. A material the install lacks is reported by the converter; its
//! draw falls back like any missing art.

/// The materials to convert, each with the largest side it is kept at (under 1022: the atlas
/// pages are 1024 px with a pixel of padding round each sprite).
pub const MATERIALS: &[(&str, u32)] = &[];

/// The MappedImage a BO2 material converts to.
pub fn image(material: &str) -> String {
    format!("ZHBO_Mat_{material}")
}
