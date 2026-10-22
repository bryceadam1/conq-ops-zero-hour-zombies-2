//! The BO2 materials the ported screens draw, by the name BO2's Lua registers them under
//! (`RegisterMaterial("hud_mp_num_big_0")`). The converter pulls each from the player's install as
//! the MappedImage [`image`] names, so a port keeps BO2's names end to end: add the material here
//! and draw `image("<name>")`. A material the install lacks is reported by the converter; its
//! draw falls back like any missing art.

/// The materials to convert, each with the largest side it is kept at (under 1022: the atlas
/// pages are 1024 px with a pixel of padding round each sprite).
pub const MATERIALS: &[(&str, u32)] = &[
    // The HUD (`hud::materials`; each port lists its own in its module).
    ("hud_mp_num_big_0", 64), ("hud_mp_num_big_1", 64), ("hud_mp_num_big_2", 64), ("hud_mp_num_big_3", 64),
    ("hud_mp_num_big_4", 64), ("hud_mp_num_big_5", 64), ("hud_mp_num_big_6", 64), ("hud_mp_num_big_7", 64),
    ("hud_mp_num_big_8", 64), ("hud_mp_num_big_9", 64), ("hud_mp_num_big_slash", 64), ("hud_mp_num_big_line", 64),
    ("hud_mp_num_big_0_white", 64), ("hud_mp_num_big_1_white", 64), ("hud_mp_num_big_2_white", 64),
    ("hud_mp_num_big_3_white", 64), ("hud_mp_num_big_4_white", 64), ("hud_mp_num_big_5_white", 64),
    ("hud_mp_num_big_6_white", 64), ("hud_mp_num_big_7_white", 64), ("hud_mp_num_big_8_white", 64),
    ("hud_mp_num_big_9_white", 64), ("hud_mp_num_big_slash_white", 64), ("hud_mp_num_big_line_white", 64),
    ("hud_mp_vis_right_lower_white", 512), ("hud_mp_vis_right_lower", 512),
    ("hud_mp_vis_left_lower_back", 512), ("hud_mp_vis_left_lower", 512), ("hud_mp_vis_score_highlight", 256),
    ("hud_mp_vis_gun_title_left", 64), ("hud_mp_vis_gun_title_mid", 64), ("hud_mp_vis_gun_title_right", 64),
    ("hud_mp_firerate_single", 64), ("hud_mp_firerate_burst", 64), ("hud_mp_firerate_auto", 64),
    ("hud_mp_firerate_free", 64), ("hud_mp_firerate_lock", 64), ("hud_mp_firerate_pump", 64), ("hud_mp_firerate_bolt", 64),
    ("hud_mp_vis_ks_dpad_box_back", 128), ("hud_mp_vis_ks_dpad_box_back_lit", 128), ("hud_mp_vis_ks_dpad_box", 128),
    ("hud_mp_vis_ks_dpad_bracket", 128),
    ("overlay_low_health", 512), ("hit_direction", 256), ("damage_feedback", 64), ("hud_horizontal_compass_minimap", 512),
    // Create a class (classui): `codmenu.lua`'s large popup frame, `cacchooseclass.lua`'s set pips,
    // `hinttext.lua`'s arrow, the grid buttons' gradients (`customclass.lua`, `growinggridbutton.lua`)
    // and the lock/restricted badges.
    ("menu_mp_cac_backing", 512),
    // The small popup's own frame slices (`codmenu.lua:1400-1493`: top, stretch, bottom). Sizes
    // like their neighbours; the captures show them as the popup's bevelled edge (`a-chooseclass/021`).
    ("menu_mp_popup_top", 512),
    ("menu_mp_popup_stretch", 512),
    ("menu_mp_popup_bottom", 512),
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
