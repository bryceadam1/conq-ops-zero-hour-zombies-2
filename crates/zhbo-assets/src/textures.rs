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
            // An optic's reticle: BO2 projects it to infinity (it stays on the line of sight,
            // wherever the eye is), which a surface on the glass can't do; the HUD draws BO2's
            // red-dot glow at the centre instead (`zhbo_core::hud`). Leave it out.
            // The clan tag and emblem: BO2 fills these in from the player's own tag and emblem
            // (picking glyphs out of a font atlas), so drawn as they are they show the whole atlas.
            if m.name.contains("reticle") || m.name.contains("clan_tag") || m.name.contains("player_icon") {
                let name = character::texture_file(&m.name);
                looks.dropped.insert(name.clone());
                return Some(name);
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
            let mut name = character::texture_file(&img.name);
            if blend != w3d::Blend::Opaque {
                name = name.replace(".tga", "_a.tga");
                looks.blend.insert(name.clone(), blend);
            }
            let path = format!("Art\\Textures\\{name}");
            if !files.contains_key(&path) {
                let mut rgba = shrink(paks.image(img.stream_hash)?, MAX_TEXTURE);
                if blend == w3d::Blend::Opaque {
                    let map = |semantic| m.image(semantic).and_then(|i| paks.image(i.stream_hash));
                    put_occlusion(&mut rgba, map(xmodel::TS_OCCLUSION_MAP).as_ref());
                    let maps = surface_maps(rgba.width, rgba.height, map(xmodel::TS_NORMAL_MAP).as_ref(), map(xmodel::TS_SPECULAR_MAP).as_ref());
                    files.insert(format!("Art\\Textures\\{}", maps_file(&name)), images::tga_bytes(&maps));
                }
                files.insert(path, images::tga_bytes(&rgba));
            }
            if blend == w3d::Blend::Opaque {
                looks.maps.insert(name.clone(), maps_file(&name));
            }
            Some(name)
        })
        .collect()
}

/// The packed BO2 maps of the opaque surface whose colour map is `color`.
pub(crate) fn maps_file(color: &str) -> String {
    color.replace(".tga", "_nsg.tga")
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
    meshes.retain(|m| m.texture.as_ref().is_none_or(|t| !looks.dropped.contains(t)));
    for m in meshes.iter_mut() {
        m.blend = m.texture.as_ref().and_then(|t| looks.blend.get(t)).copied().unwrap_or_default();
        if matches!(m.blend, w3d::Blend::Additive { .. }) {
            m.emissive = true;
        }
        m.bo2_maps = m.texture.as_ref().and_then(|t| looks.maps.get(t)).cloned();
    }
}
