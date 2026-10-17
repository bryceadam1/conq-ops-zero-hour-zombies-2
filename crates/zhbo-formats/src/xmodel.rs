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
    pub fn image(&self, semantic: u8) -> Option<&Image> {
        self.textures.iter().filter(|t| t.semantic == semantic).find_map(|t| t.image.as_ref())
    }

    /// How the material is drawn over what is behind it, from the render state of its main
    /// technique (the lit one, or the unlit/emissive one of a glow).
    pub fn blend(&self) -> crate::w3d::Blend {
        let main = [2, 3].iter().find_map(|&t| self.state_bits.get(t).copied().flatten());
        let main = main.or_else(|| self.state_bits.iter().skip(2).find_map(|b| *b));
        main.map_or(crate::w3d::Blend::Opaque, |b| blend_of(b[0]))
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
        (f32::from_bits(bits) - 3.0) * 8208.0312
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
                // each), like the rigid vertList's; the weights are half floats (OpenAssetTools'
                // XModel dumpers read both the same way for IW4 through T6).
                let mut w = vec![(rd(o) / SKEL_MAT_SIZE, 1.0f32)];
                let mut rest = 1.0f32;
                for k in 0..extra as u32 {
                    let weight = half_to_f32(rd(o + 2 + 2 * k));
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

/// A material asset by name (2D HUD art is a material too), or `None` if the zone has none.
pub fn find_material(z: &Zone, name: &str) -> Option<Material> {
    let kind = crate::fastfile::asset_type_id("material")?;
    let asset = z.find(kind, name).or_else(|| z.find(kind, &format!(",{name}")))?;
    Some(read_material(z, asset.addr))
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
                Texture { semantic: z.u8(d + 7), image: (img != 0).then(|| read_image(z, img)) }
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
    /// by 64) and secondary weights are half floats, matching OpenAssetTools' XModel readers.
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
        // Bones: byte offsets (index * 64); weights: half floats. The stream is u16 slots.
        let put_entry = |m: &mut [u8], o: usize, bone0: u16, rest: &[(u16, f32)]| {
            put16(m, o, bone0 * SKEL_MAT_SIZE);
            for (k, (bone, weight)) in rest.iter().enumerate() {
                put16(m, o + 2 * (1 + 2 * k), bone * SKEL_MAT_SIZE);
                // 0.25, 0.5 and 0.125 as half floats.
                let bits = match weight {
                    w if *w == 0.25 => 0x3400u16,
                    w if *w == 0.5 => 0x3800,
                    w if *w == 0.125 => 0x3000,
                    _ => unreachable!(),
                };
                put16(m, o + 2 * (2 + 2 * k), bits);
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
        assert_eq!(s.vertices[0].weights, vec![(3, 0.5), (7, 0.5)]);
        assert_eq!(s.vertices[1].weights, vec![(5, 0.75), (2, 0.25)]);
        assert_eq!(s.vertices[2].weights, vec![(1, 0.375), (9, 0.5), (4, 0.125)]);
        assert_eq!(s.triangles, vec![[0, 1, 2]]);
        assert!((s.vertices[0].position[0] - 1.0).abs() < 1e-5, "{:?}", s.vertices[0].position);
        assert!(s.vertices[0].normal[0] > 0.9, "{:?}", s.vertices[0].normal);
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
            // The secondary weights are half floats: they span the range, they are not one narrow
            // band (reading the half-float bits as /65535 lands every secondary near 0.22).
            let secondary: Vec<f32> = s
                .vertices
                .iter()
                .flat_map(|v| v.weights.iter().skip(1).map(|w| w.1))
                .collect();
            assert!(!secondary.is_empty(), "the character has blended vertices");
            let max = secondary.iter().cloned().fold(0.0f32, f32::max);
            assert!(max > 0.4, "secondary weights reach at least 0.4 (max {max})");
            assert!(s.triangles.iter().flatten().all(|&i| (i as usize) < s.vertices.len()));
        }
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
