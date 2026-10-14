//! Black Ops II (T6) `XModel` assets, read out of a decrypted zone (`fastfile::Zone`).
//!
//! A zone stores each asset as its fixed-size struct followed by the data of its pointer fields,
//! in field order. A model is found by its name, which is the first data after its 248-byte
//! struct, so this reads one model without walking the rest of the zone. Pointers that are not
//! "follows" are references to data loaded earlier in the zone (for example a skeleton shared with
//! another model); those are not resolved here and make the read fail with a clear error.
//!
//! Layout and loading order from OpenAssetTools (`T6_Assets.h`, `ZoneCode/Game/T6/XAssets/XModel.txt`,
//! GPL-3.0). All structs are the 32-bit PC layout.

use crate::fastfile::{PTR_FOLLOWING, PTR_INSERT};
use crate::FormatError;

const XMODEL_SIZE: usize = 248;
const XSURFACE_SIZE: usize = 80;
const VERTEX_SIZE: usize = 32; // GfxPackedVertex
const RIGID_LIST_SIZE: usize = 12; // XRigidVertList
const COLLISION_TREE_SIZE: usize = 40;
const COLLISION_NODE_SIZE: usize = 16;
const SKEL_MAT_SIZE: u16 = 64; // DObjSkelMat: bone offsets in vertex data are byte offsets of these
const SURF_QUANTIZED: u16 = 0x1;

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
}

struct Cursor<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], FormatError> {
        let s = self.d.get(self.pos..self.pos + n).ok_or(FormatError::Truncated(what))?;
        self.pos += n;
        Ok(s)
    }
    fn cstr(&mut self, what: &'static str) -> Result<String, FormatError> {
        let rest = self.d.get(self.pos..).ok_or(FormatError::Truncated(what))?;
        let len = rest.iter().position(|&b| b == 0).ok_or(FormatError::Truncated(what))?;
        self.pos += len + 1;
        Ok(String::from_utf8_lossy(&rest[..len]).into_owned())
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn i16_at(b: &[u8], at: usize) -> i16 {
    u16_at(b, at) as i16
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}
fn vec3_at(b: &[u8], at: usize) -> [f32; 3] {
    [f32_at(b, at), f32_at(b, at + 4), f32_at(b, at + 8)]
}

/// Whether a pointer's data follows in the stream; errors on references to earlier data.
fn follows(ptr: u32, field: &str) -> Result<bool, FormatError> {
    match ptr {
        0 => Ok(false),
        PTR_FOLLOWING | PTR_INSERT => Ok(true),
        _ => Err(FormatError::Corrupt(format!(
            "{field} refers to data shared with an earlier asset ({ptr:#x}); not supported yet"
        ))),
    }
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

/// Find a model by name in a decrypted zone. `script_strings` is the zone's string table
/// (`fastfile::ZoneIndex::script_strings`), which holds the bone names.
pub fn find(data: &[u8], script_strings: &[String], name: &str) -> Result<Model, FormatError> {
    let mut needle = name.as_bytes().to_vec();
    needle.push(0);
    let mut start = 0;
    while let Some(rel) = data[start..].windows(needle.len()).position(|w| w == needle.as_slice()) {
        let at = start + rel;
        start = at + 1;
        if at < XMODEL_SIZE {
            continue;
        }
        let header = &data[at - XMODEL_SIZE..at];
        let num_bones = header[4] as usize;
        let num_root = header[5] as usize;
        let num_surfs = header[6] as usize;
        if u32_at(header, 0) != PTR_FOLLOWING || num_bones == 0 || num_root > num_bones || num_surfs == 0 {
            continue;
        }
        if u32_at(header, 32) != PTR_FOLLOWING {
            continue; // the surfaces must follow
        }
        let mut c = Cursor { d: data, pos: at };
        let model_name = c.cstr("model name")?;
        return read_body(&mut c, header, model_name, script_strings);
    }
    Err(FormatError::Corrupt(format!("no model named {name} in this zone")))
}

fn read_body(c: &mut Cursor, h: &[u8], name: String, strings: &[String]) -> Result<Model, FormatError> {
    let num_bones = h[4] as usize;
    let num_root = h[5] as usize;
    let num_surfs = h[6] as usize;
    let non_root = num_bones - num_root;

    let bone_names: Vec<String> = if follows(u32_at(h, 8), "boneNames")? {
        let b = c.take(2 * num_bones, "bone names")?;
        (0..num_bones)
            .map(|i| strings.get(u16_at(b, 2 * i) as usize).cloned().unwrap_or_else(|| format!("bone{i}")))
            .collect()
    } else {
        (0..num_bones).map(|i| format!("bone{i}")).collect()
    };
    let parents = if follows(u32_at(h, 12), "parentList")? { c.take(non_root, "parent list")?.to_vec() } else { vec![0; non_root] };
    let quats = if follows(u32_at(h, 16), "quats")? { c.take(8 * non_root, "bone rotations")? } else { &[][..] };
    // Stored with a stride of 4 floats per bone although only 3 are used.
    let trans = if follows(u32_at(h, 20), "trans")? { c.take(16 * non_root, "bone offsets")? } else { &[][..] };
    if follows(u32_at(h, 24), "partClassification")? {
        c.take(num_bones, "part classification")?;
    }
    let base = if follows(u32_at(h, 28), "baseMat")? { c.take(32 * num_bones, "base pose")? } else { &[][..] };

    let mut bones = Vec::with_capacity(num_bones);
    for i in 0..num_bones {
        let (parent, local_rotation, local_position) = if i < num_root {
            (None, [0.0, 0.0, 0.0, 1.0], [0.0; 3])
        } else {
            let k = i - num_root;
            let p = i as isize - parents[k] as isize;
            let q = |j: usize| if quats.is_empty() { if j == 3 { 1.0 } else { 0.0 } } else { i16_at(quats, 8 * k + 2 * j) as f32 / 32767.0 };
            let t = if trans.is_empty() { [0.0; 3] } else { vec3_at(trans, 16 * k) };
            ((p >= 0 && (p as usize) < num_bones).then_some(p as usize), [q(0), q(1), q(2), q(3)], t)
        };
        let (rotation, position) = if base.is_empty() {
            ([0.0, 0.0, 0.0, 1.0], [0.0; 3])
        } else {
            let m = &base[32 * i..32 * i + 32];
            ([f32_at(m, 0), f32_at(m, 4), f32_at(m, 8), f32_at(m, 12)], vec3_at(m, 16))
        };
        bones.push(Bone { name: bone_names[i].clone(), parent, local_rotation, local_position, rotation, position });
    }

    // Surfaces: the whole array of structs first, then each surface's data.
    let surf_structs = c.take(XSURFACE_SIZE * num_surfs, "surfaces")?;
    let mut surfaces = Vec::with_capacity(num_surfs);
    for s in 0..num_surfs {
        let sh = &surf_structs[XSURFACE_SIZE * s..XSURFACE_SIZE * (s + 1)];
        surfaces.push(read_surface(c, sh, s)?);
    }

    let mut lods = Vec::new();
    let num_lods = u16_at(h, 196) as usize;
    for l in 0..num_lods.min(4) {
        let li = &h[40 + 28 * l..40 + 28 * (l + 1)];
        lods.push(Lod { distance: f32_at(li, 0), surface_count: u16_at(li, 4) as usize, first_surface: u16_at(li, 6) as usize });
    }
    Ok(Model { name, bones, surfaces, lods, radius: f32_at(h, 168), mins: vec3_at(h, 172), maxs: vec3_at(h, 184) })
}

fn read_surface(c: &mut Cursor, sh: &[u8], index: usize) -> Result<Surface, FormatError> {
    let vert_list_count = sh[1] as usize;
    let flags = u16_at(sh, 2);
    let vert_count = u16_at(sh, 4) as usize;
    let tri_count = u16_at(sh, 6) as usize;
    let tri_ptr = u32_at(sh, 12);
    let blend_counts = [i16_at(sh, 16), i16_at(sh, 18), i16_at(sh, 20), i16_at(sh, 22)].map(|n| n.max(0) as usize);
    let blend_ptr = u32_at(sh, 24);
    let tension_ptr = u32_at(sh, 28);
    let verts_ptr = u32_at(sh, 32);
    let vert_list_ptr = u32_at(sh, 40);
    if flags & SURF_QUANTIZED != 0 {
        return Err(FormatError::Corrupt(format!("surface {index} uses quantized vertices; not supported")));
    }

    // Loading order (XModel.txt "reorder"): vertInfo, verts0, vertList, triIndices.
    let blend = if follows(blend_ptr, "vertsBlend")? {
        let n = blend_counts[0] + 3 * blend_counts[1] + 5 * blend_counts[2] + 7 * blend_counts[3];
        Some(c.take(2 * n, "vertex weights")?)
    } else {
        None
    };
    if follows(tension_ptr, "tensionData")? {
        c.take(4 * blend_counts.iter().sum::<usize>(), "tension data")?;
    }
    let verts = if follows(verts_ptr, "verts0")? { c.take(VERTEX_SIZE * vert_count, "vertices")? } else { &[][..] };
    let mut rigid = Vec::new();
    if follows(vert_list_ptr, "vertList")? {
        let lists = c.take(RIGID_LIST_SIZE * vert_list_count, "rigid vertex lists")?;
        for l in 0..vert_list_count {
            let e = &lists[RIGID_LIST_SIZE * l..RIGID_LIST_SIZE * (l + 1)];
            rigid.push((u16_at(e, 0) / SKEL_MAT_SIZE, u16_at(e, 2) as usize));
            if follows(u32_at(e, 8), "collisionTree")? {
                let tree = c.take(COLLISION_TREE_SIZE, "collision tree")?;
                let (node_count, nodes_ptr) = (u32_at(tree, 24) as usize, u32_at(tree, 28));
                let (leaf_count, leafs_ptr) = (u32_at(tree, 32) as usize, u32_at(tree, 36));
                if follows(nodes_ptr, "collision nodes")? {
                    c.take(COLLISION_NODE_SIZE * node_count, "collision nodes")?;
                }
                if follows(leafs_ptr, "collision leafs")? {
                    c.take(2 * leaf_count, "collision leafs")?;
                }
            }
        }
    }
    let tris = if follows(tri_ptr, "triIndices")? { c.take(6 * tri_count, "triangles")? } else { &[][..] };

    let mut vertices: Vec<Vertex> = (0..verts.len() / VERTEX_SIZE)
        .map(|i| {
            let v = &verts[VERTEX_SIZE * i..VERTEX_SIZE * (i + 1)];
            let tc = u32_at(v, 20);
            Vertex {
                position: vec3_at(v, 0),
                normal: unpack_unit_vec(u32_at(v, 24)),
                uv: [half_to_f32(tc as u16), half_to_f32((tc >> 16) as u16)],
                weights: Vec::new(),
            }
        })
        .collect();

    // Weights: rigid lists first (one bone for a run of vertices), then blended vertices grouped
    // by influence count. Each blended entry is bone0, then (bone, weight) pairs; weight0 is the rest.
    let mut next = 0usize;
    for (bone, count) in rigid {
        for v in vertices.iter_mut().skip(next).take(count) {
            v.weights = vec![(bone, 1.0)];
        }
        next += count;
    }
    if let Some(b) = blend {
        let mut o = 0usize;
        for (extra, &count) in blend_counts.iter().enumerate() {
            for _ in 0..count {
                let mut w = vec![(u16_at(b, 2 * o) / SKEL_MAT_SIZE, 1.0f32)];
                let mut rest = 1.0f32;
                for k in 0..extra {
                    let bone = u16_at(b, 2 * (o + 1 + 2 * k)) / SKEL_MAT_SIZE;
                    let weight = u16_at(b, 2 * (o + 2 + 2 * k)) as f32 / 65535.0;
                    rest -= weight;
                    w.push((bone, weight));
                }
                w[0].1 = rest.max(0.0);
                if let Some(v) = vertices.get_mut(next) {
                    v.weights = w;
                }
                next += 1;
                o += 1 + 2 * extra;
            }
        }
    }

    let triangles = (0..tris.len() / 6).map(|t| [u16_at(tris, 6 * t), u16_at(tris, 6 * t + 2), u16_at(tris, 6 * t + 4)]).collect();
    Ok(Surface { vertices, triangles })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_vertex_fields() {
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0xC000), -2.0);
        // 10-bit components: 0x1FF is the largest positive, 0 is zero.
        let v = unpack_unit_vec(0x1FF);
        assert!(v[0] > 0.9 && v[1].abs() < 0.01 && v[2].abs() < 0.01, "{v:?}");
    }

    /// Needs the player's BO2 install (with Multiplayer): `ZHBO_BO2_DIR=... cargo test -p zhbo-formats -- --ignored`.
    #[test]
    #[ignore]
    fn reads_an_mp_character() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/faction_fbi_mp.ff")).unwrap();
        let zone = crate::fastfile::read(&file).unwrap();
        let index = zone.index().unwrap();
        let model = find(&zone.data, &index.script_strings, "c_usa_mp_fbi_assault_fb").unwrap();
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
        // About 72 inches tall, standing on the origin.
        assert!(model.maxs[2] > 60.0 && model.maxs[2] < 85.0, "height {:?}", model.maxs);
    }
}
