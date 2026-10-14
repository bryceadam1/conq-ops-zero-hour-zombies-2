//! Black Ops II (T6) `XModel` assets, read from a fully loaded zone (`t6zone::Zone`).
//!
//! The zone loader has already resolved every pointer, including data shared with other assets
//! (skeletons, materials, images), so this only reads fields at their offsets. Offsets are the
//! 32-bit PC layout from OpenAssetTools' `T6_Assets.h` (GPL-3.0).

use crate::fastfile::ASSET_XMODEL;
use crate::t6zone::Zone;
use crate::FormatError;

const SKEL_MAT_SIZE: u16 = 64; // DObjSkelMat: vertex bone references are byte offsets of these
const SURF_QUANTIZED: u16 = 0x1;
const XSURFACE_SIZE: u32 = 80;
const VERTEX_SIZE: u32 = 32;

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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct Texture {
    pub semantic: u8,
    pub image: Option<Image>,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub name: String,
    pub textures: Vec<Texture>,
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
    let num_bones = z.u8(h + 4) as usize;
    let num_root = z.u8(h + 5) as usize;
    let num_surfs = z.u8(h + 6) as usize;
    if num_root > num_bones {
        return Err(FormatError::Corrupt(format!("model {name}: {num_root} root bones of {num_bones}")));
    }
    let (names, parents, quats, trans, base) = (z.u32(h + 8), z.u32(h + 12), z.u32(h + 16), z.u32(h + 20), z.u32(h + 28));

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
            let t = if trans != 0 { vec3(z, trans + 16 * k) } else { [0.0; 3] };
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

    let surfs = z.u32(h + 32);
    let surfaces = (0..num_surfs).map(|s| if surfs != 0 { read_surface(z, surfs + XSURFACE_SIZE * s as u32) } else { Surface::default() }).collect();

    let handles = z.u32(h + 36);
    let materials = (0..num_surfs)
        .map(|s| {
            let m = if handles != 0 { z.u32(handles + 4 * s as u32) } else { 0 };
            (m != 0).then(|| read_material(z, m))
        })
        .collect();

    let num_lods = z.u16(h + 196) as usize;
    let lods = (0..num_lods.min(4))
        .map(|l| {
            let li = h + 40 + 28 * l as u32;
            Lod { distance: z.f32(li), surface_count: z.u16(li + 4) as usize, first_surface: z.u16(li + 6) as usize }
        })
        .collect();
    Ok(Model { name, bones, surfaces, lods, radius: z.f32(h + 168), mins: vec3(z, h + 172), maxs: vec3(z, h + 184), materials })
}

fn read_surface(z: &Zone, sh: u32) -> Surface {
    let vert_list_count = z.u8(sh + 1) as u32;
    let flags = z.u16(sh + 2);
    let vert_count = z.u16(sh + 4) as u32;
    let tri_count = z.u16(sh + 6) as u32;
    let tris = z.u32(sh + 12);
    let blend_counts = [16, 18, 20, 22].map(|o| z.i16(sh + o).max(0) as usize);
    let blend = z.u32(sh + 24);
    let verts = z.u32(sh + 32);
    let vert_list = z.u32(sh + 40);
    if flags & SURF_QUANTIZED != 0 || verts == 0 {
        return Surface::default();
    }

    let mut vertices: Vec<Vertex> = (0..vert_count)
        .map(|i| {
            let v = verts + VERTEX_SIZE * i;
            let tc = z.u32(v + 20);
            Vertex {
                position: vec3(z, v),
                normal: unpack_unit_vec(z.u32(v + 24)),
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
    Surface { vertices, triangles }
}

fn read_material(z: &Zone, m: u32) -> Material {
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
    Material { name, textures }
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
    fn unpacks_vertex_fields() {
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0xC000), -2.0);
        let v = unpack_unit_vec(0x1FF);
        assert!(v[0] > 0.9 && v[1].abs() < 0.01 && v[2].abs() < 0.01, "{v:?}");
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
