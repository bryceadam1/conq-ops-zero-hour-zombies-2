//! Black Ops II (T6) `XModel` assets, read from a fully loaded zone (`t6zone::Zone`).
//!
//! The zone loader has already resolved every pointer, including data shared with other assets
//! (skeletons, materials, images), so this only reads fields at their offsets. Offsets are the
//! 32-bit PC layout from OpenAssetTools' `T6_Assets.h` (GPL-3.0).

use crate::fastfile::ASSET_XMODEL;
use crate::t6zone::layouts::{DObjAnimMat, GfxPackedVertex, XModel as XM, XModelLodInfo as XLI, XSurface as XS, XSurfaceVertexInfo as XV};
use crate::t6zone::Zone;
use crate::FormatError;

/// DObjSkelMat: vertex bone references are byte offsets of these.
const SKEL_MAT_SIZE: u16 = (DObjAnimMat::SIZE * 2) as u16;
const SURF_QUANTIZED: u16 = 0x1;
const XSURFACE_SIZE: u32 = XS::SIZE;
const VERTEX_SIZE: u32 = GfxPackedVertex::SIZE;

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Relative to the parent (identity for root bones).
    pub local_rotation: [f32; 4],
    pub local_position: [f32; 3],
    /// Model space, bind pose.
    pub rotation: [f32; 4],
    pub position: [f32; 3],
}

#[derive(Clone, Debug, Default)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Up to four (bone index, weight) pairs; weights sum to 1.
    pub weights: Vec<(u16, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Surface {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u16; 3]>,
}

#[derive(Clone, Debug)]
pub struct Lod {
    pub distance: f32,
    pub first_surface: usize,
    pub surface_count: usize,
}

/// T6 texture semantics (`TextureSemantic`).
pub const TS_COLOR_MAP: u8 = 2;
pub const TS_NORMAL_MAP: u8 = 5;
pub const TS_SPECULAR_MAP: u8 = 8;
pub const TS_OCCLUSION_MAP: u8 = 10;
/// The camo layer: `camo_off_solid`/`camo_off_pattern`/`weapon_camo_neutral` sit here on exactly
/// the surfaces a `WeaponCamo` acts on. Inferred, not read from an enum table (research §2.1).
pub const TS_CAMO_MAP: u8 = 22;

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub name: String,
    /// `GfxImage::hash`.
    pub hash: u32,
    /// `streamedParts[0].hash`: the image's entry in the `.ipak` packs.
    pub stream_hash: u32,
    pub streamed_parts: u8,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    /// `MaterialTextureDef::nameHash`: the shader sampler this image feeds, as BO2's
    /// [`sampler_hash`] of its name (`colorMap`, `normalMap`, ...).
    pub name_hash: u32,
    pub semantic: u8,
    pub image: Option<Image>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub textures: Vec<Texture>,
    /// The render state (`GfxStateBits::loadBits`) each technique type draws with, by technique
    /// type (`stateBitsEntry`); `None` where the material has no such technique.
    pub state_bits: Vec<Option<[u32; 2]>>,
    pub sort_key: u8,
}

impl Material {
    /// The image used as the colour map, if it is known.
    pub fn color_map(&self) -> Option<&Image> {
        let is_color = |t: &&Texture| t.semantic == TS_COLOR_MAP || (0xC..=0x1B).contains(&t.semantic);
        self.textures.iter().filter(is_color).find_map(|t| t.image.as_ref())
    }
    /// The image bound to the shader sampler `name` (`colorMap`), matched by the def's name hash.
    /// Style camo materials carry several colour-semantic images (`camo_code_spc`, a spec-rgb
    /// map, the real `_col`); only the sampler name says which one is the colour.
    pub fn sampler(&self, name: &str) -> Option<&Image> {
        let h = sampler_hash(name);
        self.textures.iter().filter(|t| t.name_hash == h).find_map(|t| t.image.as_ref())
    }
    pub fn image(&self, semantic: u8) -> Option<&Image> {
        self.textures.iter().filter(|t| t.semantic == semantic).find_map(|t| t.image.as_ref())
    }

    /// The render state of the material's main technique (the lit one, or the unlit/emissive one
    /// of a glow).
    fn main_state_bits(&self) -> Option<[u32; 2]> {
        [2, 3].iter().find_map(|&t| self.state_bits.get(t).copied().flatten()).or_else(|| self.state_bits.iter().skip(2).find_map(|b| *b))
    }

    /// How the material is drawn over what is behind it, from the render state of its main
    /// technique (the lit one, or the unlit/emissive one of a glow).
    pub fn blend(&self) -> crate::w3d::Blend {
        self.main_state_bits().map_or(crate::w3d::Blend::Opaque, |b| blend_of(b[0]))
    }

    /// Which faces the material culls, from the same technique.
    pub fn cull(&self) -> Cull {
        self.main_state_bits().map_or(Cull::Back, |b| cull_of(b[0]))
    }
}

/// The hash BO2 stores in `MaterialTextureDef::nameHash` for a sampler name: `h = 33·h ^ c` over
/// the lower-cased name from 0 (`R_HashString`). Checked against the install: an ordinary gun
/// material's `colorMap` and the style camos' `_col` both carry `0xa0ab1041`.
pub fn sampler_hash(name: &str) -> u32 {
    name.bytes().fold(0u32, |h, c| h.wrapping_mul(33) ^ c.to_ascii_lowercase() as u32)
}

/// Which of a surface's faces draw (`GfxStateBits::loadBits[0]` bits 14-15: BO2's
/// `GFXS0_CULL_*`, as in OAT's T6 `GfxStateBitsEnum` — OpenAssetTools commit 28011d6b64).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cull {
    /// Both (alpha-tested cutouts, glow planes).
    #[default]
    None,
    /// The back faces: BO2's ordinary lit surfaces.
    Back,
    Front,
}

/// `GfxStateBits::loadBits[0]` bits 14-15: 1 `GFXS0_CULL_NONE`, 2 `GFXS0_CULL_BACK`,
/// 3 `GFXS0_CULL_FRONT`. 0 is undefined in the enum (no lit technique sets it): draw
/// double-sided, as surfaces rendered before this was read at all.
pub fn cull_of(bits: u32) -> Cull {
    match (bits >> 14) & 3 {
        2 => Cull::Back,
        3 => Cull::Front,
        _ => Cull::None,
    }
}

/// `GfxStateBits::loadBits[0]`: source and destination colour blend (bits 0-3, 4-7: BO2's
/// `GFXS_BLEND_*`, 1 zero, 2 one, 5 src alpha, 6 1 - src alpha, 10 1 - dest colour) and alpha test
/// (bit 11 set: disabled).
pub fn blend_of(bits: u32) -> crate::w3d::Blend {
    use crate::w3d::Blend;
    const ZERO: u32 = 1;
    const ONE: u32 = 2;
    const SRC_ALPHA: u32 = 5;
    const INV_SRC_ALPHA: u32 = 6;
    let (src, dst) = (bits & 0xF, (bits >> 4) & 0xF);
    let alpha_test = bits & 0x800 == 0;
    match (src, dst) {
        (_, 0) | (_, ZERO) if alpha_test => Blend::AlphaTest,
        (_, 0) | (_, ZERO) => Blend::Opaque,
        // A glow: added (an inverse-destination-colour source, a soft "screen" add, is close enough).
        (src, ONE) => Blend::Additive { alpha: src == SRC_ALPHA },
        (ONE, INV_SRC_ALPHA) => Blend::Premultiplied,
        _ => Blend::Alpha,
    }
}

#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub bones: Vec<Bone>,
    /// All surfaces of all LODs; `lods` says which belong to which.
    pub surfaces: Vec<Surface>,
    pub lods: Vec<Lod>,
    pub radius: f32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// One per surface (same index).
    pub materials: Vec<Option<Material>>,
}

fn vec3(z: &Zone, a: u32) -> [f32; 3] {
    [z.f32(a), z.f32(a + 4), z.f32(a + 8)]
}

fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1F) as i32;
    let man = (h & 0x3FF) as f32;
    sign * match exp {
        0 => man * 2f32.powi(-24),
        31 => f32::INFINITY,
        _ => (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// T6 `PackedUnitVec`: three 10-bit components (OAT's `Vec3UnpackUnitVecThirdBased`).
fn unpack_unit_vec(packed: u32) -> [f32; 3] {
    let c = |shift: u32| {
        let v = (packed >> shift) & 0x3FF;
        let bits = v.wrapping_sub(2 * (v & 0x200)).wrapping_add(0x4040_0000);
        (f32::from_bits(bits) - 3.0) * 8_208.031
    };
    [c(0), c(10), c(20)]
}

/// Find a model by name in a loaded zone.
pub fn find(zone: &Zone, name: &str) -> Result<Model, FormatError> {
    let asset = zone.find(ASSET_XMODEL, name).ok_or_else(|| FormatError::Corrupt(format!("no model named {name} in zone {}", zone.name)))?;
    read(zone, asset.addr)
}

/// Read the model whose struct is at `addr`.
pub fn read(z: &Zone, h: u32) -> Result<Model, FormatError> {
    let name = z.str_ptr(h).trim_start_matches(',').to_string();
    let num_bones = z.try_u8(h + XM::numBones)? as usize;
    let num_root = z.try_u8(h + XM::numRootBones)? as usize;
    let num_surfs = z.try_u8(h + XM::numsurfs)? as usize;
    if num_root > num_bones {
        return Err(FormatError::Corrupt(format!("model {name}: {num_root} root bones of {num_bones}")));
    }
    let (names, parents, quats, trans, base) = (z.try_u32(h + XM::boneNames)?, z.try_u32(h + XM::parentList)?, z.try_u32(h + XM::quats)?, z.try_u32(h + XM::trans)?, z.try_u32(h + XM::baseMat)?);

    let mut bones = Vec::with_capacity(num_bones);
    for i in 0..num_bones {
        let bone_name = if names != 0 { z.script_string(z.u16(names + 2 * i as u32)).to_string() } else { format!("bone{i}") };
        let (parent, local_rotation, local_position) = if i < num_root {
            (None, [0.0, 0.0, 0.0, 1.0], [0.0; 3])
        } else {
            let k = (i - num_root) as u32;
            let p = i as isize - if parents != 0 { z.u8(parents + k) as isize } else { 1 };
            let q = if quats != 0 {
                [0, 1, 2, 3].map(|j| z.i16(quats + 8 * k + 2 * j) as f32 / 32767.0)
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            // Stored with a stride of 4 floats per bone although only 3 are used.
            let t = if trans != 0 { vec3(z, trans + 12 * k) } else { [0.0; 3] };
            ((p >= 0 && (p as usize) < num_bones).then_some(p as usize), q, t)
        };
        let (rotation, position) = if base != 0 {
            let m = base + 32 * i as u32;
            ([z.f32(m), z.f32(m + 4), z.f32(m + 8), z.f32(m + 12)], vec3(z, m + 16))
        } else {
            ([0.0, 0.0, 0.0, 1.0], [0.0; 3])
        };
        bones.push(Bone { name: bone_name, parent, local_rotation, local_position, rotation, position });
    }

    let surfs = z.try_u32(h + XM::surfs)?;
    let surfaces = (0..num_surfs)
        .map(|s| if surfs != 0 { read_surface(z, surfs + XSURFACE_SIZE * s as u32) } else { Ok(Surface::default()) })
        .collect::<Result<Vec<_>, _>>()?;

    let handles = z.try_u32(h + XM::materialHandles)?;
    let materials = (0..num_surfs)
        .map(|s| {
            let m = if handles != 0 { z.u32(handles + 4 * s as u32) } else { 0 };
            (m != 0).then(|| read_material(z, m))
        })
        .collect();

    let num_lods = z.try_u16(h + XM::numLods)? as usize;
    let lods = (0..num_lods.min(4))
        .map(|l| -> Result<Lod, FormatError> {
            let li = h + XM::lodInfo + XLI::SIZE * l as u32;
            Ok(Lod {
                distance: z.try_f32(li + XLI::dist)?,
                surface_count: z.try_u16(li + XLI::numsurfs)? as usize,
                first_surface: z.try_u16(li + XLI::surfIndex)? as usize,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Model { name, bones, surfaces, lods, radius: z.try_f32(h + XM::radius)?, mins: vec3(z, h + XM::mins), maxs: vec3(z, h + XM::maxs), materials })
}

fn read_surface(z: &Zone, sh: u32) -> Result<Surface, FormatError> {
    let vert_list_count = z.try_u8(sh + XS::vertListCount)? as u32;
    let flags = z.try_u16(sh + XS::flags)?;
    let vert_count = z.try_u16(sh + XS::vertCount)? as u32;
    let tri_count = z.try_u16(sh + XS::triCount)? as u32;
    let tris = z.try_u32(sh + XS::triIndices)?;
    // XSurface::vertInfo holds XSurfaceVertexInfo: the blend counts are its vertCount short[4].
    let blend_counts = (0..4usize)
        .map(|i| {
            z.try_u16(sh + XS::vertInfo + XV::vertCount + i as u32 * 2)
                .map_or(0, |v| (v as i16).max(0)) as usize
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or([0usize; 4]);
    let blend = z.try_u32(sh + XS::vertInfo + XV::vertsBlend)?;
    let verts = z.try_u32(sh + XS::verts0)?;
    let vert_list = z.try_u32(sh + XS::vertList)?;
    if flags & SURF_QUANTIZED != 0 || verts == 0 {
        return Ok(Surface::default());
    }

    let mut vertices: Vec<Vertex> = (0..vert_count)
        .map(|i| {
            let v = verts + VERTEX_SIZE * i;
            let tc = z.u32(v + GfxPackedVertex::texCoord);
            Vertex {
                position: vec3(z, v),
                normal: unpack_unit_vec(z.u32(v + GfxPackedVertex::normal)),
                uv: [half_to_f32(tc as u16), half_to_f32((tc >> 16) as u16)],
                weights: Vec::new(),
            }
        })
        .collect();

    // Weights: rigid lists first (one bone for a run of vertices), then blended vertices grouped by
    // influence count. Each blended entry is bone0, then (bone, weight) pairs; weight0 is the rest.
    let mut next = 0usize;
    if vert_list != 0 {
        for l in 0..vert_list_count {
            let e = vert_list + 12 * l;
            let bone = z.u16(e) / SKEL_MAT_SIZE;
            let count = z.u16(e + 2) as usize;
            for v in vertices.iter_mut().skip(next).take(count) {
                v.weights = vec![(bone, 1.0)];
            }
            next += count;
        }
    }
    if blend != 0 {
        let mut o = 0u32;
        let rd = |i: u32| z.u16(blend + 2 * i);
        for (extra, &count) in blend_counts.iter().enumerate() {
            for _ in 0..count {
                // Bone references are byte offsets into the surface's DObjSkelMat array (64 bytes
                // each), like the rigid vertList's; the weights are u16 fixed point (val/65535 —
                // OpenAssetTools' `BoneWeight16`, for IW4 through T6), NOT half floats.
                let mut w = vec![(rd(o) / SKEL_MAT_SIZE, 1.0f32)];
                let mut rest = 1.0f32;
                for k in 0..extra as u32 {
                    let weight = rd(o + 2 + 2 * k) as f32 / 65535.0;
                    rest -= weight;
                    w.push((rd(o + 1 + 2 * k) / SKEL_MAT_SIZE, weight));
                }
                w[0].1 = rest.max(0.0);
                if let Some(v) = vertices.get_mut(next) {
                    v.weights = w;
                }
                next += 1;
                o += 1 + 2 * extra as u32;
            }
        }
    }

    let triangles = if tris != 0 {
        (0..tri_count).map(|t| [z.u16(tris + 6 * t), z.u16(tris + 6 * t + 2), z.u16(tris + 6 * t + 4)]).collect()
    } else {
        Vec::new()
    };
    Ok(Surface { vertices, triangles })
}

/// The `GfxImage` named `name` with real pixel data (not a patch-zone stub): an image asset of its
/// own, or one loaded inline inside another asset (`t6_camo_*` in `common_mp`; research §4.3).
pub fn find_image(z: &Zone, name: &str) -> Option<Image> {
    let sized = |i: &Image| i.width > 0 && i.height > 0 && i.stream_hash != 0;
    crate::fastfile::asset_type_id("image")
        .and_then(|kind| z.find(kind, name))
        .map(|a| read_image(z, a.addr))
        .filter(sized)
        .or_else(|| find_linked(z, "image", name).map(|a| read_image(z, a)).filter(sized))
        .or_else(|| {
            // The image's name pointer is at +72 (`GfxImage::name`), so a pointer-to-name's slot
            // address minus 72 is the image's start. Validate the read before trusting it.
            z.strings(name)
                .into_iter()
                .flat_map(|s| z.pointers_to(s))
                .filter_map(|p| p.checked_sub(72))
                .map(|g| read_image(z, g))
                .find(|i| i.name == name && sized(i))
        })
}

/// A material asset by name (2D HUD art is a material too), or `None` if the zone has none: the
/// zone's own asset, then one nested inside another asset (BO2 registers both in the material
/// pool, `Zone::linked`), then the slow scan for an inline copy.
pub fn find_material(z: &Zone, name: &str) -> Option<Material> {
    find_asset_material(z, name).or_else(|| find_linked_material(z, name)).or_else(|| find_inline_material(z, name))
}

/// A material that is one of the zone's own assets (fast; [`find_material`] also scans for
/// inline ones).
pub fn find_asset_material(z: &Zone, name: &str) -> Option<Material> {
    let kind = crate::fastfile::asset_type_id("material")?;
    let asset = z.find(kind, name).or_else(|| z.find(kind, &format!(",{name}")))?;
    Some(read_material(z, asset.addr))
}

/// A material the zone loads nested inside another asset (an `XModel`'s, a `WeaponCamo`'s): BO2
/// registers it in its pool like a top-level one (`Zone::linked`). A reference stub (`,name`, no
/// textures) is skipped, so a zone that only points at the material answers `None`.
pub fn find_linked_material(z: &Zone, name: &str) -> Option<Material> {
    let kind = crate::fastfile::asset_type_id("material")?;
    z.linked
        .iter()
        .filter(|a| a.kind == kind && z.asset_name(a).as_deref() == Some(name))
        .map(|a| read_material(z, a.addr))
        .find(|m| !m.textures.is_empty())
}

/// The address of the full (non-reference) linked asset of type `kind` named `name`.
fn find_linked(z: &Zone, kind: &str, name: &str) -> Option<u32> {
    let kind = crate::fastfile::asset_type_id(kind)?;
    z.linked.iter().find(|a| a.kind == kind && z.asset_name(a).as_deref() == Some(name)).map(|a| a.addr)
}

/// A material that is not one of the zone's own assets but is loaded inline inside another (a
/// weapon's or menu's), as many HUD icons are (`hud_mp_firerate_*` in `common_mp`): its name
/// string, then a field pointing at it that starts a `Material` with that name and an image.
pub fn find_inline_material(z: &Zone, name: &str) -> Option<Material> {
    z.strings(name).into_iter().flat_map(|s| z.pointers_to(s)).find_map(|m| {
        // A real Material: its texture count fits its table and its images have sizes. A name
        // pointer alone also matches a reference stub or another struct's name field.
        if z.u8(m + 84) == 0 || z.u8(m + 84) > 32 {
            return None;
        }
        let mat = read_material(z, m);
        let sized = |t: &Texture| t.image.as_ref().is_some_and(|i| i.width > 0 && i.height > 0 && i.width <= 8192 && i.height <= 8192);
        (mat.name == name && mat.textures.iter().all(sized)).then_some(mat)
    })
}

pub fn read_material(z: &Zone, m: u32) -> Material {
    let name = z.str_ptr(m).trim_start_matches(',').to_string();
    let count = z.u8(m + 84) as u32;
    let table = z.u32(m + 96);
    let textures = if table == 0 {
        Vec::new()
    } else {
        (0..count)
            .map(|t| {
                let d = table + 16 * t;
                let img = z.u32(d + 12);
                Texture { name_hash: z.u32(d), semantic: z.u8(d + 7), image: (img != 0).then(|| read_image(z, img)) }
            })
            .collect()
    };
    let states = z.u32(m + 104);
    let state_count = z.u8(m + 86) as u32;
    let state_bits = (0..36)
        .map(|t| {
            let i = z.u8(m + 48 + t) as u32;
            (states != 0 && i < state_count).then(|| [z.u32(states + 20 * i), z.u32(states + 20 * i + 4)])
        })
        .collect();
    Material { name, textures, state_bits, sort_key: z.u8(m + 9) }
}

/// Read the `GfxImage` at `g`.
pub fn read_image(z: &Zone, g: u32) -> Image {
    Image {
        name: z.str_ptr(g + 72).trim_start_matches(',').to_string(),
        hash: z.u32(g + 76),
        stream_hash: z.u32(g + 40),
        streamed_parts: z.u8(g + 60),
        width: z.u16(g + 20),
        height: z.u16(g + 22),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_modes_from_render_state() {
        use crate::w3d::Blend;
        // As BO2's materials have them: a gun, the Reflex lens and reticle, the tablet's buttons.
        assert_eq!(blend_of(0x1812_8812), Blend::Opaque);
        // The same words for culling (bits 14-15): the ordinary lit material culls back faces,
        // 1 is double-sided, 3 culls front, and 0 (undefined in BO2's enum) stays double-sided.
        assert_eq!(cull_of(0x1812_8812), Cull::Back);
        assert_eq!(cull_of(0x0000_4065), Cull::None);
        assert_eq!(cull_of(0x0000_8065), Cull::Back);
        assert_eq!(cull_of(0x0000_C065), Cull::Front);
        assert_eq!(cull_of(0x0000_0065), Cull::None);
        assert_eq!(blend_of(0x0000_0065), Blend::Alpha);
        assert_eq!(blend_of(0x0000_0862), Blend::Premultiplied);
        assert_eq!(blend_of(0x0000_082A), Blend::Additive { alpha: false });
        assert_eq!(blend_of(0x0000_0012), Blend::AlphaTest);
    }

    #[test]
    fn unpacks_vertex_fields() {
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0xC000), -2.0);
        let v = unpack_unit_vec(0x1FF);
        assert!(v[0] > 0.9 && v[1].abs() < 0.01 && v[2].abs() < 0.01, "{v:?}");
    }

    /// Blend weights of a synthetic surface: bone references are DObjSkelMat byte offsets (divided
    /// by 64) and secondary weights are u16 fixed point (val/65535), matching OpenAssetTools'
    /// `BoneWeight16`.
    #[test]
    fn reads_synthetic_blend_weights() {
        let mut mem = vec![0u8; 0x400];
        let put16 = |m: &mut [u8], o: usize, v: u16| m[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let put32 = |m: &mut [u8], o: usize, v: u32| m[o..o + 4].copy_from_slice(&v.to_le_bytes());

        let sh = 0x40; // XSurface
        mem[sh] = 0; // vertListCount
        put16(&mut mem, sh + 2, 0); // flags: not quantized
        put16(&mut mem, sh + 4, 3); // vertCount
        put16(&mut mem, sh + 6, 1); // triCount
        put16(&mut mem, sh + 16, 0); // blendCounts[0]
        put16(&mut mem, sh + 18, 2); // blendCounts[1]: two 2-influence vertices
        put16(&mut mem, sh + 20, 1); // blendCounts[2]: one 3-influence vertex
        put16(&mut mem, sh + 22, 0);
        let blend_addr = 0x100;
        let verts_addr = 0x180;
        let tris_addr = 0x200;
        put32(&mut mem, sh + 24, blend_addr as u32);
        put32(&mut mem, sh + 32, verts_addr as u32);
        put32(&mut mem, sh + 40, 0); // no rigid vertList
        put32(&mut mem, sh + 12, tris_addr as u32);

        // Blend stream: [bone0][bone1][w1] for 2-influence, [bone0][bone1][w1][bone2][w2] for 3.
        // Bones: byte offsets (index * 64); weights: u16 fixed point. The stream is u16 slots.
        let fixed = |w: f32| (w * 65536.0).min(65535.0) as u16; // exact for the powers of two below
        let weight_of = |bits: u16| bits as f32 / 65535.0;
        let put_entry = |m: &mut [u8], o: usize, bone0: u16, rest: &[(u16, f32)]| {
            put16(m, o, bone0 * SKEL_MAT_SIZE);
            for (k, (bone, weight)) in rest.iter().enumerate() {
                put16(m, o + 2 * (1 + 2 * k), bone * SKEL_MAT_SIZE);
                put16(m, o + 2 * (2 + 2 * k), fixed(*weight));
            }
        };
        put_entry(&mut mem, blend_addr as usize, 3, &[(7, 0.5)]);
        put_entry(&mut mem, blend_addr as usize + 6, 5, &[(2, 0.25)]);
        put_entry(&mut mem, blend_addr as usize + 12, 1, &[(9, 0.5), (4, 0.125)]);

        // Three vertices at 32-byte stride: position (12), texCoord (at 20), normal (at 24).
        for i in 0..3usize {
            let v = verts_addr + 32 * i;
            let xyz: [f32; 3] = [1.0 + i as f32, 2.0, 3.0];
            for (c, x) in xyz.iter().enumerate() {
                put32(&mut mem, v + 4 * c, x.to_bits());
            }
            put32(&mut mem, v + 20, 0); // tex coords
            put32(&mut mem, v + 24, 0x1FF); // +X normal (unpacked 1.0, 0, 0)
        }
        // One triangle 0,1,2.
        put16(&mut mem, tris_addr, 0);
        put16(&mut mem, tris_addr + 2, 1);
        put16(&mut mem, tris_addr + 4, 2);

        let zone = crate::t6zone::synthetic_zone("t", mem, Vec::new(), Vec::new());
        let s = read_surface(&zone, sh as u32).expect("the synthetic surface reads");
        assert_eq!(s.vertices.len(), 3);
        let (w_half, w_quarter, w_eighth) = (weight_of(0x8000), weight_of(0x4000), weight_of(0x2000));
        assert_eq!(s.vertices[0].weights, vec![(3, 1.0 - w_half), (7, w_half)]);
        assert_eq!(s.vertices[1].weights, vec![(5, 1.0 - w_quarter), (2, w_quarter)]);
        assert_eq!(s.vertices[2].weights, vec![(1, 1.0 - w_half - w_eighth), (9, w_half), (4, w_eighth)]);
        assert_eq!(s.triangles, vec![[0, 1, 2]]);
        assert!((s.vertices[0].position[0] - 1.0).abs() < 1e-5, "{:?}", s.vertices[0].position);
        assert!(s.vertices[0].normal[0] > 0.9, "{:?}", s.vertices[0].normal);
    }

    #[test]
    fn find_image_skips_stubs_and_finds_inline_images() {
        // Two images named "camo_off_solid": a stub (0x0, hash 0) first and a real 32x32 second. The
        // finder must return the real one; the stub is what the patch zone's copies look like.
        let mut mem = vec![0u8; 0x200];
        let put32 = |m: &mut Vec<u8>, at: usize, v: u32| m[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let put16 = |m: &mut Vec<u8>, at: usize, v: u16| m[at..at + 2].copy_from_slice(&v.to_le_bytes());
        let name = 0x20;
        mem[name..name + 15].copy_from_slice(b"camo_off_solid\0");
        // The stub at 0x40: name pointer at +72 -> the name; width/height 0; stream hash 0.
        put32(&mut mem, 0x40 + 72, name as u32);
        // The real image at 0x80: same name, 32x32, a stream hash.
        put32(&mut mem, 0x80 + 72, name as u32);
        put16(&mut mem, 0x80 + 20, 32);
        put16(&mut mem, 0x80 + 22, 32);
        put32(&mut mem, 0x80 + 40, 0x1234);
        let zone = crate::t6zone::synthetic_zone("img", mem, Vec::new(), Vec::new());
        let img = find_image(&zone, "camo_off_solid").expect("the real image");
        assert_eq!((img.width, img.height, img.stream_hash), (32, 32, 0x1234));
    }

    #[test]
    fn sampler_names_hash_as_bo2_stores_them() {
        // From the install: an ordinary gun material's `colorMap`, the style camos' spec-rgb.
        assert_eq!(sampler_hash("colorMap"), 0xa0ab_1041);
        assert_eq!(sampler_hash("SpecularAndGloss"), 0x8c29_7e80);
        assert_eq!(sampler_hash("specularMap"), 0x34ec_ccb3);
    }

    #[test]
    fn nested_materials_resolve_and_their_colour_is_the_colormap_sampler() {
        // A material loaded nested inside another asset (on `linked`, not on `assets`), preceded
        // by a same-named `,` reference stub. Two colour-semantic images: a helper first, then the
        // `colorMap` one; `sampler` must pick by name hash, not semantic order.
        let mut mem = vec![0u8; 0x400];
        let put32 = |m: &mut Vec<u8>, at: usize, v: u32| m[at..at + 4].copy_from_slice(&v.to_le_bytes());
        mem[0x10..0x18].copy_from_slice(b",mat_ab\0");
        mem[0x20..0x27].copy_from_slice(b"mat_ab\0");
        mem[0x30..0x37].copy_from_slice(b"helper\0");
        mem[0x38..0x3e].copy_from_slice(b"color\0");
        // Stub at 0x40 (no textures), real material at 0x100.
        put32(&mut mem, 0x40, 0x10);
        put32(&mut mem, 0x100, 0x20);
        mem[0x100 + 84] = 2;
        put32(&mut mem, 0x100 + 96, 0x200);
        // Texture defs at 0x200: helper (some other hash, semantic 2) then colorMap (semantic 2).
        put32(&mut mem, 0x200, sampler_hash("SpecularAndGloss"));
        mem[0x200 + 7] = TS_COLOR_MAP;
        put32(&mut mem, 0x200 + 12, 0x280);
        put32(&mut mem, 0x210, sampler_hash("colorMap"));
        mem[0x210 + 7] = TS_COLOR_MAP;
        put32(&mut mem, 0x210 + 12, 0x300);
        put32(&mut mem, 0x280 + 72, 0x30);
        put32(&mut mem, 0x300 + 72, 0x38);
        let mut zone = crate::t6zone::synthetic_zone("nested", mem, Vec::new(), Vec::new());
        let asset = |addr| crate::t6zone::Asset { kind: crate::fastfile::asset_type_id("material").unwrap(), structure: "Material", addr };
        zone.linked = vec![asset(0x40), asset(0x100)];
        assert!(find_asset_material(&zone, "mat_ab").is_none(), "not a top-level asset");
        let m = find_linked_material(&zone, "mat_ab").expect("the nested material");
        assert_eq!(m.color_map().map(|i| i.name.as_str()), Some("helper"), "the semantic order picks the helper");
        assert_eq!(m.sampler("colorMap").map(|i| i.name.as_str()), Some("color"));
        assert_eq!(m.sampler("SpecularAndGloss").map(|i| i.name.as_str()), Some("helper"));
        // The general lookup the HUD converter uses finds it too (the stub is skipped).
        assert_eq!(find_material(&zone, "mat_ab").map(|m| m.textures.len()), Some(2));
    }

    /// The load registers every nested asset, as the reference catalog (OAT `Unlinker --list`)
    /// counts them: `common_mp` declares 6,082 top-level assets but loads 9,285, among them all
    /// 1,472 materials, the style camos (`mc/mtl_weapon_camo_gold`) included.
    #[test]
    #[ignore]
    fn common_mp_registers_its_nested_assets() {
        let z = zone("common_mp");
        let count = |k: &str| z.linked.iter().filter(|a| a.kind == crate::fastfile::asset_type_id(k).unwrap()).count();
        assert_eq!((z.linked.len(), count("material"), count("image")), (9285, 1472, 1541));
        let gold = find_linked_material(&z, "mc/mtl_weapon_camo_gold").expect("gold is nested in common_mp");
        // The ADS overlays BO2 ships only nested (in the attachment uniques), with real streamed art.
        for (material, image, hash) in [
            ("scope_overlay_envg_ir", "scope_overlay_envg_1024", 0x1d07_82e5),
            ("scope_overlay_specter", "scope_overlay_specter_1024", 0x1546_91c9),
            ("scope_overlay_vzoom", "scope_overlay_vzoom_1024", 0x1239_abaf),
        ] {
            let m = find_material(&z, material).unwrap_or_else(|| panic!("{material} is nested in common_mp"));
            let i = m.textures.iter().find_map(|t| t.image.as_ref()).expect("an image");
            assert_eq!((i.name.as_str(), i.width, i.height, i.stream_hash), (image, 1024, 1024, hash));
        }
        let col = gold.sampler("colorMap").expect("gold's colorMap");
        assert_eq!(col.name, "~-gcamo_gold_col");
        assert!(find_image(&z, "t6_camo_devgru_solid").is_some_and(|i| i.stream_hash != 0));
    }

    fn zone(name: &str) -> Zone {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join(format!("zone/all/{name}.ff"))).unwrap();
        crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap()
    }

    /// Needs the player's BO2 install (with Multiplayer): `ZHBO_BO2_DIR=... cargo test -p zhbo-formats --release -- --ignored`.
    #[test]
    #[ignore]
    fn reads_an_mp_character() {
        let model = find(&zone("faction_fbi_mp"), "c_usa_mp_fbi_assault_fb").unwrap();
        assert!(model.bones.len() > 20, "a character skeleton");
        assert!(model.bones.iter().any(|b| b.name == "j_head"), "bones are named");
        let lod0 = &model.lods[0];
        let surfs = &model.surfaces[lod0.first_surface..lod0.first_surface + lod0.surface_count];
        assert!(surfs.iter().map(|s| s.triangles.len()).sum::<usize>() > 1000);
        for s in surfs {
            for v in &s.vertices {
                let total: f32 = v.weights.iter().map(|w| w.1).sum();
                assert!((total - 1.0).abs() < 0.01, "weights sum to 1");
                assert!(v.weights.iter().all(|w| (w.0 as usize) < model.bones.len()));
            }
            assert!(s.triangles.iter().flatten().all(|&i| (i as usize) < s.vertices.len()));
        }
        // The secondary weights are u16 fixed point (val/65535): they span the range, they are not
        // one narrow band (reading them as half floats lands many of them past 1.0, some infinite —
        // the sum assert above trips first). Some surfaces are rigid gear (one bone, no blend
        // stream), so check across the whole LOD.
        let secondary: Vec<f32> = surfs.iter().flat_map(|s| s.vertices.iter().flat_map(|v| v.weights.iter().skip(1).map(|w| w.1))).collect();
        assert!(!secondary.is_empty(), "the character has blended vertices");
        let max = secondary.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 0.4, "secondary weights reach at least 0.4 (max {max})");
        assert!(model.maxs[2] > 60.0 && model.maxs[2] < 85.0, "height {:?}", model.maxs);
        for m in &model.materials[lod0.first_surface..lod0.first_surface + lod0.surface_count] {
            assert!(m.is_some(), "material resolved, shared ones included");
        }
    }

    #[test]
    #[ignore]
    fn reads_a_weapon_with_shared_materials() {
        let model = find(&zone("common_mp"), "t6_wpn_ar_an94_view").unwrap();
        let colored = model.materials.iter().flatten().filter(|m| m.color_map().is_some()).count();
        assert!(colored >= model.materials.len() / 2, "{colored} of {} materials have a colour map", model.materials.len());
    }
}
