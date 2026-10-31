/*
** Conquer Ops - converter: the texture pipeline: BO2 textures into Zero Hour maps, occlusion and finishes.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::character;
use crate::generals::MAX_TEXTURE;
use crate::images;
use crate::paks::{shrink, Paks};
use std::collections::BTreeSet;
use zhbo_formats::iwi::Rgba;
use zhbo_formats::{w3d, xmodel};
use std::collections::BTreeMap;


/// How the converted surfaces are drawn, by texture file: how each blends over what is behind it
/// (BO2's own render state), and the ones left out.
#[derive(Default)]

pub(crate) struct Looks {
    pub(crate) blend: BTreeMap<String, w3d::Blend>,
    pub(crate) dropped: BTreeSet<String>,
    /// Opaque surfaces' packed BO2 maps (see [`surface_maps`]), by colour map.
    pub(crate) maps: BTreeMap<String, String>,
    /// Opaque surfaces' specular colour (see [`specular_color`]), by colour map.
    pub(crate) specular: BTreeMap<String, String>,
    /// Surfaces in `dropped` that the native BO2 pass still draws (the W3D path has no shader for
    /// them): the lens reticle, the clan tag and the emblem.
    pub(crate) native_only: BTreeSet<String>,
    /// The lens reticles' projection and constants, by colour map (the reticle image).
    pub(crate) reticle: BTreeMap<String, zhbo_native::model::Reticle>,
    /// The emblem surfaces, by colour map: the core paints the player's faction emblem there.
    pub(crate) emblem: BTreeSet<String>,
}

/// The zone that loads the optics' reticle art in full (the CaC reticle options' materials;
/// `common_mp`'s optic models reference some of their images as stubs): learned by the `Paks`.
pub(crate) const RETICLE_ZONE: &str = "code_post_gfx_mp";

/// BO2's technique sets for the surfaces the engine fills at draw time (`Material::techset`).
const RETICLE_TECHSETS: &str = "sw4_3d_reticle";
/// The projected (parallax-free) lens reticle: reflex, holo, ACOG, the dual optic's halves.
const RETICLE_DYNAMIC: &str = "sw4_3d_reticle_dynamic_";
const CLAN_TAG: &str = "sw4_3d_clan_tag";
const PLAYER_ICON: &str = "sw4_3d_player_icon";

/// BO2's wrist datapad: the campaign map/readout screen on the first-person hands. Multiplayer's
/// watch face is not that screen, and drawn statically the campaign UI reads as placeholder art
/// (its yellow readouts look like caution stripes, its radar like a magenta dot). Draw the screen
/// off instead: the geometry and the watchglass stay, the campaign UI never reaches the hands.
const DATAPAD: &str = "viewarm_gen_datapad";
pub(crate) const DATAPAD_OFF: &str = "zhbo_viewarm_gen_datapad_off.tga";

/// The wrist datapad's screen off: a flat, dark panel (the watchglass over it stays).
fn datapad_off() -> Rgba {
    const SIZE: u32 = 256;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for _ in 0..SIZE * SIZE {
        pixels.extend([16, 18, 20, 255]);
    }
    Rgba { width: SIZE, height: SIZE, pixels }
}

/// Textures for a model's surfaces: the colour map of each, written once per name into `files`.
/// Opaque surfaces get their ambient occlusion in the colour map's alpha and their packed BO2 maps
/// in a second file (see [`surface_maps`]); the others keep BO2's alpha (coverage) in a file of
/// their own. Blend modes and maps go in `looks`.
pub(crate) fn textures(model: &xmodel::Model, paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Vec<Option<String>> {
    model
        .materials
        .iter()
        .map(|m| {
            let m = m.as_ref()?;
            // The surfaces BO2's engine fills at draw time. The W3D path has no shader for any of them
            // and leaves them out (the HUD draws the reticle there); the native pass draws them.
            let engine_filled = if m.techset.contains(RETICLE_DYNAMIC) {
                Some(reticle(m, paks, files, looks))
            } else if m.techset.contains(CLAN_TAG) {
                Some(clan_tag(m, paks, files, looks))
            } else if m.techset.contains(PLAYER_ICON) {
                Some(m.sampler("colorMap").and_then(|img| coverage_texture(m, img, paks, files, looks)).inspect(|name| {
                    looks.emblem.insert(name.clone());
                }))
            } else if m.techset.contains(RETICLE_TECHSETS) {
                // The other reticle techsets (the MMS's static, the dual band's code-drawn ui3d and
                // two-colour) need inputs the mod does not model: left out of both paths.
                Some(None)
            } else {
                None
            };
            if let Some(converted) = engine_filled {
                // A surface whose art or constants are missing is left out, never drawn untextured.
                return Some(converted.unwrap_or_else(|| {
                    let name = character::texture_file(&m.name);
                    looks.dropped.insert(name.clone());
                    name
                }));
            }
            let blend = m.blend();
            let Some(img) = m.color_map() else {
                // A see-through surface drawn only by a special shader: nothing to draw it with.
                if blend != w3d::Blend::Opaque {
                    let name = character::texture_file(&m.name);
                    looks.dropped.insert(name.clone());
                    return Some(name);
                }
                return None;
            };
            let neutral = m.name.contains(DATAPAD);
            let mut name = if neutral { DATAPAD_OFF.to_string() } else { character::texture_file(&img.name) };
            if blend != w3d::Blend::Opaque {
                if !neutral {
                    name = name.replace(".tga", "_a.tga");
                }
                looks.blend.insert(name.clone(), blend);
            }
            let path = format!("Art\\Textures\\{name}");
            if !files.contains_key(&path) {
                let mut rgba = if neutral { datapad_off() } else { shrink(paks.image(img.stream_hash)?, MAX_TEXTURE) };
                if blend == w3d::Blend::Opaque {
                    let map = |semantic| m.image(semantic).and_then(|i| paks.image(i.stream_hash));
                    put_occlusion(&mut rgba, map(xmodel::TS_OCCLUSION_MAP).as_ref());
                    let spec = map(xmodel::TS_SPECULAR_MAP);
                    let maps = surface_maps(rgba.width, rgba.height, map(xmodel::TS_NORMAL_MAP).as_ref(), spec.as_ref());
                    files.insert(format!("Art\\Textures\\{}", maps_file(&name)), images::tga_bytes(&maps));
                    if let Some(spec) = spec {
                        files.insert(format!("Art\\Textures\\{}", specular_file(&name)), images::tga_bytes(&specular_color(spec)));
                    }
                }
                files.insert(path, images::tga_bytes(&rgba));
            }
            if blend == w3d::Blend::Opaque {
                looks.maps.insert(name.clone(), maps_file(&name));
                if files.contains_key(&format!("Art\\Textures\\{}", specular_file(&name))) {
                    looks.specular.insert(name.clone(), specular_file(&name));
                }
            }
            Some(name)
        })
        .collect()
}

/// A surface only the native pass draws, with its own image's colour and coverage (`*_a.tga`),
/// BO2's blend, left out of the W3D conversion.
fn coverage_texture(m: &xmodel::Material, img: &xmodel::Image, paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Option<String> {
    let name = character::texture_file(&img.name).replace(".tga", "_a.tga");
    let path = format!("Art\\Textures\\{name}");
    if let std::collections::btree_map::Entry::Vacant(e) = files.entry(path) {
        e.insert(images::tga_bytes(&shrink(paks.resolve(img)?, MAX_TEXTURE)));
    }
    native_only(m, &name, looks);
    Some(name)
}

fn native_only(m: &xmodel::Material, name: &str, looks: &mut Looks) {
    looks.blend.insert(name.to_string(), m.blend());
    looks.dropped.insert(name.to_string());
    looks.native_only.insert(name.to_string());
}

/// A lens reticle material on its own (a CaC reticle option's), converted as [`textures`] converts
/// one on a model: the reticle image's file name, its constants in `looks`.
pub(crate) fn lens_reticle(m: &xmodel::Material, paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Option<String> {
    m.techset.contains(RETICLE_DYNAMIC).then(|| reticle(m, paks, files, looks)).flatten()
}

/// The lens reticle: its image (`Color_Map`, raw: the channels are shape/glow/background, not
/// colour), its noise (`Detail_Map`) and the shader's constants (see `zhbo_native::model::Reticle`).
fn reticle(m: &xmodel::Material, paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Option<String> {
    let name = coverage_texture(m, m.sampler("Color_Map")?, paks, files, looks)?;
    let noise = m.sampler("Detail_Map").and_then(|img| {
        let file = character::texture_file(&img.name);
        let path = format!("Art\\Textures\\{file}");
        if let std::collections::btree_map::Entry::Vacant(e) = files.entry(path.clone()) {
            e.insert(images::tga_bytes(&shrink(paks.resolve(img)?, MAX_TEXTURE)));
        }
        Some(path)
    });
    looks.reticle.insert(name.clone(), reticle_constants(m, noise)?);
    Some(name)
}

/// The reticle shader's constants from the material (`None` when one is missing: nothing to draw
/// it by).
pub(crate) fn reticle_constants(m: &xmodel::Material, noise: Option<String>) -> Option<zhbo_native::model::Reticle> {
    let c = |n: &str| m.constant(n);
    let rgb = |v: [f32; 4]| [v[0], v[1], v[2]];
    Some(zhbo_native::model::Reticle {
        scale: c("Color_Map_Scale")?[0],
        color: rgb(c("Reticle_Color")?),
        glow: rgb(c("Glow_Color")?),
        glow_falloff: c("Glow_Falloff")?[0],
        emissive: c("Emissive_Amount")?[0],
        noise_scale: c("Noise_Scale")?[0],
        noise_amount: c("Color_Map_Noise")?[0],
        background: c("Background")?[0],
        noise,
    })
    .filter(|r| [r.scale, r.glow_falloff, r.emissive, r.noise_scale, r.noise_amount, r.background].iter().chain(&r.color).chain(&r.glow).all(|v| v.is_finite()))
}

/// The clan tag as BO2 draws it for a player without one. Its shader picks four glyphs out of the
/// font atlas (`ColorMap`, `Row_Chars_` cells a row of `Char_Size` UV each), one per quarter of
/// the surface's U, by the indices code writes to `weaponParam1`; with no tag every index is 0
/// (`t6mp` `FUN_0052a010`), so each quarter shows the atlas's first cell. That cell, four times
/// across, is the surface's texture.
fn clan_tag(m: &xmodel::Material, paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Option<String> {
    let name = character::texture_file(&format!("{}_empty", m.name)).replace(".tga", "_a.tga");
    let path = format!("Art\\Textures\\{name}");
    if let std::collections::btree_map::Entry::Vacant(e) = files.entry(path) {
        let atlas = paks.resolve(m.sampler("colorMap")?)?;
        e.insert(images::tga_bytes(&empty_clan_tag(&atlas, m.constant("Char_Size")?)));
    }
    native_only(m, &name, looks);
    Some(name)
}

/// The atlas's first `char_size` (UV) cell, repeated four times across: the clan tag shader's
/// picture for indices 0, 0, 0, 0.
pub(crate) fn empty_clan_tag(atlas: &Rgba, char_size: [f32; 4]) -> Rgba {
    let cw = ((char_size[0] * atlas.width as f32).round() as u32).clamp(1, atlas.width);
    let ch = ((char_size[1] * atlas.height as f32).round() as u32).clamp(1, atlas.height);
    let (w, h) = (cw * 4, ch);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * atlas.width + x % cw) * 4) as usize;
            pixels.extend_from_slice(&atlas.pixels[i..i + 4]);
        }
    }
    Rgba { width: w, height: h, pixels }
}

/// The packed BO2 maps of the opaque surface whose colour map is `color`.
pub(crate) fn maps_file(color: &str) -> String {
    color.replace(".tga", "_nsg.tga")
}

/// The specular colour of the opaque surface whose colour map is `color` (`_spec`, not BO2's own
/// `_spc` image suffix, so no converted file can be mistaken for a BO2 spec image of that name).
pub(crate) fn specular_file(color: &str) -> String {
    color.replace(".tga", "_spec.tga")
}

/// A BO2 surface's specular colour: its spec map's RGB, raw, at the map's own size (shrunk to the
/// texture cap), alpha opaque. BO2's spec maps are colour maps (`*_spc-rgb`), not levels: a brass
/// cartridge, a red tritium vial, a lens coating and the gold camo — whose colour map is a flat
/// dark and whose whole hue is here — reflect in colour. The packed maps keep the luminance as
/// the level for surfaces drawn without this map.
pub(crate) fn specular_color(spec: Rgba) -> Rgba {
    let mut spec = shrink(spec, MAX_TEXTURE);
    for p in spec.pixels.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    spec
}

/// The texel of `img` at `(x, y)` of a `w` x `h` image (nearest).
pub(crate) fn texel(img: &Rgba, x: u32, y: u32, w: u32, h: u32) -> [u8; 4] {
    let (sx, sy) = (x * img.width / w, y * img.height / h);
    let i = ((sy * img.width + sx) * 4) as usize;
    [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
}

/// A BO2 surface's own maps in one texture the renderer's BO2 shading reads (host patch 0009): the
/// tangent-space normal's X and Y (BO2's BC5 normal map), gloss (the specular map's alpha) and
/// specular level (its colour's luminance: bare metal bright, cloth and polymer dark). Missing maps
/// give a flat, matte surface.
pub(crate) fn surface_maps(w: u32, h: u32, normal: Option<&Rgba>, spec: Option<&Rgba>) -> Rgba {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let n = normal.map_or([128, 128, 255, 255], |n| texel(n, x, y, w, h));
            let s = spec.map_or([0; 4], |s| texel(s, x, y, w, h));
            let level = 0.2126 * s[0] as f32 + 0.7152 * s[1] as f32 + 0.0722 * s[2] as f32;
            pixels.extend([n[0], n[1], s[3], level.round() as u8]);
        }
    }
    Rgba { width: w, height: h, pixels }
}

/// Put BO2's ambient occlusion (its occlusion map's red) in an opaque colour map's alpha, which BO2
/// uses for other data; fully open without one.
pub(crate) fn put_occlusion(color: &mut Rgba, ao: Option<&Rgba>) {
    let (w, h) = (color.width, color.height);
    for y in 0..h {
        for x in 0..w {
            color.pixels[((y * w + x) * 4 + 3) as usize] = ao.map_or(255, |a| texel(a, x, y, w, h)[0]);
        }
    }
}

/// Leave out the dropped surfaces, give each the blend mode of its material and the opaque ones
/// their BO2 maps. Glows are self-lit.
pub(crate) fn finish(meshes: &mut Vec<w3d::SkinMesh>, looks: &Looks) {
    // (`native_only` surfaces are in `dropped` too: the W3D path has no shader for them.)
    meshes.retain(|m| m.texture.as_ref().is_none_or(|t| !looks.dropped.contains(t)));
    for m in meshes.iter_mut() {
        m.blend = m.texture.as_ref().and_then(|t| looks.blend.get(t)).copied().unwrap_or_default();
        if matches!(m.blend, w3d::Blend::Additive { .. }) {
            m.emissive = true;
        }
        m.bo2_maps = m.texture.as_ref().and_then(|t| looks.maps.get(t)).cloned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrist_datapad_screen_is_a_dark_neutral_panel() {
        let off = datapad_off();
        assert_eq!((off.width, off.height), (256, 256));
        assert_eq!(off.pixels.len(), (256 * 256 * 4) as usize);
        assert!(off.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255 && p[0] < 40 && p[1] < 40 && p[2] < 40));
        assert!(DATAPAD_OFF.starts_with("zhbo_") && DATAPAD_OFF.ends_with(".tga"));
    }

    /// The spec map's colour survives as it is (gold's hue), at the map's own size, opaque; the
    /// file sits beside the colour map's packed maps.
    #[test]
    fn the_specular_colour_keeps_the_spec_maps_rgb() {
        let gold = Rgba { width: 4, height: 2, pixels: [158u8, 135, 83, 40].repeat(8) };
        let spec = specular_color(gold);
        assert_eq!((spec.width, spec.height), (4, 2));
        assert!(spec.pixels.as_chunks::<4>().0.iter().all(|p| *p == [158, 135, 83, 255]));
        assert_eq!(specular_file("zhbo_camo_gold_col.tga"), "zhbo_camo_gold_col_spec.tga");
        // The packed maps keep the luminance as the level for draws without the colour.
        let maps = surface_maps(2, 2, None, Some(&Rgba { width: 1, height: 1, pixels: vec![158, 135, 83, 40] }));
        assert_eq!(maps.pixels[2..4], [40, 136]);
    }

    /// No tag: the shader's four glyph indices are all 0, so the surface shows the atlas's first
    /// cell in each quarter of its U.
    #[test]
    fn the_empty_clan_tag_is_the_first_cell_four_times() {
        // A 4x4 atlas whose cells are 2x2 (Char_Size 0.5); the first cell is marked.
        let mut atlas = Rgba { width: 4, height: 4, pixels: vec![0; 64] };
        for (x, y) in [(0u32, 0u32), (1, 0), (0, 1), (1, 1)] {
            atlas.pixels[((y * 4 + x) * 4) as usize..][..4].copy_from_slice(&[x as u8 + 10, y as u8 + 20, 7, 200]);
        }
        let tag = empty_clan_tag(&atlas, [0.5, 0.5, 0.0, 1.0]);
        assert_eq!((tag.width, tag.height), (8, 2));
        for y in 0..2u32 {
            for x in 0..8u32 {
                let i = ((y * 8 + x) * 4) as usize;
                assert_eq!(tag.pixels[i..i + 4], [(x % 2) as u8 + 10, y as u8 + 20, 7, 200], "({x},{y})");
            }
        }
    }

    /// The reticle's numbers are the material's own constants, by the shader's names; a missing
    /// one means nothing to draw the reticle by.
    #[test]
    fn the_reticle_reads_its_shaders_constants() {
        let c = |n: &str, v: [f32; 4]| xmodel::Constant { name_hash: xmodel::sampler_hash(n), value: v };
        let mut m = xmodel::Material {
            name: "mc/mtl_t6_attach_optic_reflex_reticle_01".into(),
            constants: vec![
                c("Reticle_Color", [0.254546, 1.0, 0.0, 1.0]),
                c("Glow_Falloff", [3.01, 0.0, 0.0, 1.0]),
                c("Noise_Scale", [4.75, 0.0, 0.0, 1.0]),
                c("Emissive_Amount", [8.3, 0.0, 0.0, 1.0]),
                c("Glow_Color", [0.0, 1.0, 0.209091, 1.0]),
                c("Background", [0.0, 0.0, 0.0, 1.0]),
                c("Color_Map_Noise", [0.1, 0.0, 0.0, 1.0]),
                c("Color_Map_Scale", [14.6, 0.0, 0.0, 1.0]),
            ],
            ..Default::default()
        };
        let r = reticle_constants(&m, Some("n".into())).expect("all constants present");
        assert_eq!((r.scale, r.color, r.glow, r.glow_falloff, r.emissive), (14.6, [0.254546, 1.0, 0.0], [0.0, 1.0, 0.209091], 3.01, 8.3));
        assert_eq!((r.noise_scale, r.noise_amount, r.background, r.noise.as_deref()), (4.75, 0.1, 0.0, Some("n")));
        m.constants.pop();
        assert!(reticle_constants(&m, None).is_none(), "no Color_Map_Scale, no projection");
    }
}
