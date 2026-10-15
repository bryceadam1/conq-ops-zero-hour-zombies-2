//! Writes Zero Hour `.w3d` files: hierarchies, skinned meshes with an HLOD, and raw animations.
//!
//! Layouts from the engine's `w3d_file.h`. Rules that matter for skins, from the engine's loaders:
//! * a skin vertex is stored relative to its bone's bind transform (the engine draws it at
//!   `bone_transform * vertex`), one bone per vertex;
//! * pivot transforms are `parent * base * translate(anim t) * rotate(anim q)`, so animation channels
//!   hold deltas from the hierarchy's base pose;
//! * texture V is stored flipped (the loader uses `1 - V`);
//! * names are at most 15 characters (`W3D_NAME_LEN` 16 with the NUL).

pub const NAME_LEN: usize = 16;

const CHUNK_MESH: u32 = 0x0000;
const CHUNK_VERTICES: u32 = 0x0002;
const CHUNK_VERTEX_NORMALS: u32 = 0x0003;
const CHUNK_VERTEX_INFLUENCES: u32 = 0x000E;
const CHUNK_MESH_HEADER3: u32 = 0x001F;
const CHUNK_TRIANGLES: u32 = 0x0020;
const CHUNK_VERTEX_SHADE_INDICES: u32 = 0x0022;
const CHUNK_MATERIAL_INFO: u32 = 0x0028;
const CHUNK_SHADERS: u32 = 0x0029;
const CHUNK_VERTEX_MATERIALS: u32 = 0x002A;
const CHUNK_VERTEX_MATERIAL: u32 = 0x002B;
const CHUNK_VERTEX_MATERIAL_NAME: u32 = 0x002C;
const CHUNK_VERTEX_MATERIAL_INFO: u32 = 0x002D;
const CHUNK_TEXTURES: u32 = 0x0030;
const CHUNK_TEXTURE: u32 = 0x0031;
const CHUNK_TEXTURE_NAME: u32 = 0x0032;
const CHUNK_MATERIAL_PASS: u32 = 0x0038;
const CHUNK_VERTEX_MATERIAL_IDS: u32 = 0x0039;
const CHUNK_SHADER_IDS: u32 = 0x003A;
/// `W3dVertexMaterialStruct::Attributes`: stage 1 texture coordinates from the world-space reflection.
const VERTMAT_STAGE1_MAPPING_WS_ENVIRONMENT: u32 = 0x0000_0D00;
/// `W3DSHADER_DETAILCOLORFUNC_MODALPHAADDCOLOR`: local + localAlpha * other.
const DETAILCOLOR_MODALPHAADDCOLOR: u8 = 12;
const CHUNK_TEXTURE_STAGE: u32 = 0x0048;
const CHUNK_TEXTURE_IDS: u32 = 0x0049;
const CHUNK_STAGE_TEXCOORDS: u32 = 0x004A;
const CHUNK_HIERARCHY: u32 = 0x0100;
const CHUNK_HIERARCHY_HEADER: u32 = 0x0101;
const CHUNK_PIVOTS: u32 = 0x0102;
const CHUNK_ANIMATION: u32 = 0x0200;
const CHUNK_ANIMATION_HEADER: u32 = 0x0201;
const CHUNK_ANIMATION_CHANNEL: u32 = 0x0202;
const CHUNK_HLOD: u32 = 0x0700;
const CHUNK_BOX: u32 = 0x0740;
const CHUNK_HLOD_HEADER: u32 = 0x0701;
const CHUNK_HLOD_LOD_ARRAY: u32 = 0x0702;
const CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER: u32 = 0x0703;
const CHUNK_HLOD_SUB_OBJECT: u32 = 0x0704;

const fn version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}
const MESH_VERSION: u32 = version(4, 2);
const HTREE_VERSION: u32 = version(4, 1);
const HANIM_VERSION: u32 = version(4, 1);
const HLOD_VERSION: u32 = version(1, 0);

const MESH_FLAG_SKIN: u32 = 0x0002_0000;
const VERTEX_CHANNEL_LOCATION: u32 = 0x01;
const VERTEX_CHANNEL_NORMAL: u32 = 0x02;
const VERTEX_CHANNEL_TEXCOORD: u32 = 0x04;
const VERTEX_CHANNEL_BONEID: u32 = 0x10;
const FACE_CHANNEL_FACE: u32 = 0x01;
const ANIM_CHANNEL_X: u16 = 0;
const ANIM_CHANNEL_Q: u16 = 6;

/// A chunk under construction; `sub` marks chunks that contain other chunks.
struct Chunk {
    id: u32,
    data: Vec<u8>,
    sub: bool,
}

impl Chunk {
    fn new(id: u32) -> Self {
        Chunk { id, data: Vec::new(), sub: false }
    }
    fn wrapper(id: u32) -> Self {
        Chunk { id, data: Vec::new(), sub: true }
    }
    fn child(&mut self, c: Chunk) -> &mut Self {
        c.write_to(&mut self.data);
        self
    }
    fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_le_bytes());
        let size = self.data.len() as u32 | if self.sub { 0x8000_0000 } else { 0 };
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&self.data);
    }
    fn u8(&mut self, v: u8) -> &mut Self {
        self.data.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn vec3(&mut self, v: [f32; 3]) -> &mut Self {
        self.f32(v[0]).f32(v[1]).f32(v[2])
    }
    fn name(&mut self, s: &str, len: usize) -> &mut Self {
        let mut b = vec![0u8; len];
        let n = s.len().min(len - 1);
        b[..n].copy_from_slice(&s.as_bytes()[..n]);
        self.data.extend_from_slice(&b);
        self
    }
    fn cstr(&mut self, s: &str) -> &mut Self {
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0);
        self
    }
}

#[derive(Clone, Debug)]
pub struct Pivot {
    pub name: String,
    /// Index of the parent pivot; `None` for pivot 0 only.
    pub parent: Option<usize>,
    /// Base pose, relative to the parent.
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

/// A hierarchy (`<name>.w3d`, conventionally `XXX_SKL`). Pivot 0 must be the root transform.
pub fn hierarchy(name: &str, pivots: &[Pivot]) -> Vec<u8> {
    let mut h = Chunk::wrapper(CHUNK_HIERARCHY);
    let mut header = Chunk::new(CHUNK_HIERARCHY_HEADER);
    header.u32(HTREE_VERSION).name(name, NAME_LEN).u32(pivots.len() as u32).vec3([0.0; 3]);
    h.child(header);
    let mut p = Chunk::new(CHUNK_PIVOTS);
    for pv in pivots {
        p.name(&pv.name, NAME_LEN)
            .u32(pv.parent.map_or(u32::MAX, |i| i as u32))
            .vec3(pv.translation)
            .vec3([0.0; 3]) // Euler angles: unused by the engine when the quaternion is present
            .f32(pv.rotation[0])
            .f32(pv.rotation[1])
            .f32(pv.rotation[2])
            .f32(pv.rotation[3]);
    }
    h.child(p);
    let mut out = Vec::new();
    h.write_to(&mut out);
    out
}

/// How a mesh is drawn over what is behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    #[default]
    Opaque,
    /// Opaque where the texture's alpha is over half, clear elsewhere (straps, hair).
    AlphaTest,
    /// Mixed by the texture's alpha (glass, decals).
    Alpha,
    /// Colour already weighted by its alpha: added, with the background dimmed by the alpha.
    Premultiplied,
    /// Added over the background (glows): black is clear. `alpha` weights it by the texture's alpha.
    Additive { alpha: bool },
}

/// One skinned mesh: vertices relative to their bone, one texture.
#[derive(Clone, Debug, Default)]
pub struct SkinMesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Direct3D convention (V down); flipped when written.
    pub uvs: Vec<[f32; 2]>,
    /// Pivot index per vertex.
    pub bones: Vec<u16>,
    pub triangles: Vec<[u32; 3]>,
    /// Texture file name (e.g. `zhbo_x.tga`), or none for an untextured mesh.
    pub texture: Option<String>,
    /// Blended meshes don't write depth; Zero Hour sorts them back to front after the opaque ones.
    pub blend: Blend,
    /// A world-space environment map added over the texture, weighted by the texture's alpha (its
    /// shine): `colour + alpha * environment`. Opaque meshes only (the others' alpha is coverage).
    pub env_map: Option<String>,
    /// BO2's own surface maps packed into one texture (`*_nsg.tga`: normal X and Y, gloss,
    /// specular level), on stage 1 with the mesh's UVs. The renderer lights such a mesh as BO2 does
    /// (host patch 0009); the fixed-function stage is off. Takes the place of `env_map`.
    pub bo2_maps: Option<String>,
    /// Self-lit at full brightness, whatever the scene's lights (screens, glowing buttons).
    pub emissive: bool,
}

fn mesh_chunk(container: &str, m: &SkinMesh) -> Chunk {
    let mut c = Chunk::wrapper(CHUNK_MESH);
    let textured = m.texture.is_some();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &m.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if m.positions.is_empty() {
        (lo, hi) = ([0.0; 3], [0.0; 3]);
    }
    let center = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
    let radius = m.positions.iter().map(|p| (0..3).map(|k| (p[k] - center[k]).powi(2)).sum::<f32>().sqrt()).fold(0.0, f32::max);

    let mut header = Chunk::new(CHUNK_MESH_HEADER3);
    header
        .u32(MESH_VERSION)
        .u32(MESH_FLAG_SKIN)
        .name(&m.name, NAME_LEN)
        .name(container, NAME_LEN)
        .u32(m.triangles.len() as u32)
        .u32(m.positions.len() as u32)
        .u32(1) // materials
        .u32(0) // damage stages
        .u32(0) // sort level
        .u32(0) // prelit version
        .u32(0) // future
        .u32(VERTEX_CHANNEL_LOCATION | VERTEX_CHANNEL_NORMAL | VERTEX_CHANNEL_TEXCOORD | VERTEX_CHANNEL_BONEID)
        .u32(FACE_CHANNEL_FACE)
        .vec3(lo)
        .vec3(hi)
        .vec3(center)
        .f32(radius);
    c.child(header);

    let mut v = Chunk::new(CHUNK_VERTICES);
    m.positions.iter().for_each(|p| {
        v.vec3(*p);
    });
    c.child(v);
    let mut n = Chunk::new(CHUNK_VERTEX_NORMALS);
    m.normals.iter().for_each(|p| {
        n.vec3(*p);
    });
    c.child(n);

    let mut t = Chunk::new(CHUNK_TRIANGLES);
    for tri in &m.triangles {
        let [a, b, cc] = tri.map(|i| m.positions[i as usize]);
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
        let mut nrm = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if len > 0.0 {
            nrm = nrm.map(|x| x / len);
        }
        let dist = nrm[0] * a[0] + nrm[1] * a[1] + nrm[2] * a[2];
        t.u32(tri[0]).u32(tri[1]).u32(tri[2]).u32(0).vec3(nrm).f32(dist);
    }
    c.child(t);

    let mut shade = Chunk::new(CHUNK_VERTEX_SHADE_INDICES);
    (0..m.positions.len() as u32).for_each(|i| {
        shade.u32(i);
    });
    c.child(shade);

    let mut inf = Chunk::new(CHUNK_VERTEX_INFLUENCES);
    for &b in &m.bones {
        inf.u16(b).u16(0).u16(0).u16(0);
    }
    c.child(inf);

    let bo2_maps = m.bo2_maps.as_ref().filter(|_| textured);
    let env = m.env_map.as_ref().filter(|_| textured && m.blend == Blend::Opaque && bo2_maps.is_none());
    let stage1 = bo2_maps.or(env);
    let mut info = Chunk::new(CHUNK_MATERIAL_INFO);
    info.u32(1).u32(1).u32(1).u32(textured as u32 + stage1.is_some() as u32);
    c.child(info);

    let mut vms = Chunk::wrapper(CHUNK_VERTEX_MATERIALS);
    let mut vm = Chunk::wrapper(CHUNK_VERTEX_MATERIAL);
    let mut vm_name = Chunk::new(CHUNK_VERTEX_MATERIAL_NAME);
    vm_name.cstr("zhbo");
    vm.child(vm_name);
    let mut vm_info = Chunk::new(CHUNK_VERTEX_MATERIAL_INFO);
    vm_info
        .u32(if env.is_some() { VERTMAT_STAGE1_MAPPING_WS_ENVIRONMENT } else { 0 }) // attributes
        // Untextured surfaces (BO2 materials drawn by special shaders, such as screens) get a dark
        // grey instead of plain white.
        .u32(if m.emissive { 0 } else if textured { 0xFFFF_FFFF } else { 0x0040_4040 }) // ambient (RGB + pad)
        .u32(if m.emissive { 0 } else if textured { 0xFFFF_FFFF } else { 0x0040_4040 }) // diffuse
        .u32(0) // specular
        .u32(if m.emissive { 0xFFFF_FFFF } else { 0 }) // emissive
        .f32(1.0) // shininess
        .f32(1.0) // opacity
        .f32(0.0); // translucency
    vm.child(vm_info);
    vms.child(vm);
    c.child(vms);

    // ShaderClass's SrcBlend (zero, one, src alpha, 1 - src alpha) and DestBlend (zero, one, src
    // colour, 1 - src colour, src alpha, 1 - src alpha).
    let (src_blend, dst_blend, depth_write) = match m.blend {
        Blend::Opaque | Blend::AlphaTest => (1, 0, true),
        Blend::Alpha => (2, 5, false),
        Blend::Premultiplied => (1, 5, false),
        Blend::Additive { alpha } => (if alpha { 2 } else { 1 }, 1, false),
    };
    let mut shaders = Chunk::new(CHUNK_SHADERS);
    shaders
        .u8(3) // DepthCompare: PASS_LEQUAL
        .u8(depth_write as u8) // DepthMask
        .u8(0) // ColorMask (obsolete)
        .u8(dst_blend) // DestBlend
        .u8(0) // FogFunc (obsolete)
        .u8(1) // PriGradient: modulate
        .u8(0) // SecGradient: disable
        .u8(src_blend) // SrcBlend
        .u8(textured as u8) // Texturing
        .u8(if env.is_some() { DETAILCOLOR_MODALPHAADDCOLOR } else { 0 }) // DetailColorFunc
        .u8(0) // DetailAlphaFunc
        .u8(0) // ShaderPreset
        .u8((m.blend == Blend::AlphaTest) as u8) // AlphaTest
        .u8(0)
        .u8(0)
        .u8(0);
    c.child(shaders);

    if let Some(tex) = &m.texture {
        let mut texs = Chunk::wrapper(CHUNK_TEXTURES);
        let mut tx = Chunk::wrapper(CHUNK_TEXTURE);
        let mut tn = Chunk::new(CHUNK_TEXTURE_NAME);
        tn.cstr(tex);
        tx.child(tn);
        texs.child(tx);
        if let Some(second) = stage1 {
            let mut tx = Chunk::wrapper(CHUNK_TEXTURE);
            let mut tn = Chunk::new(CHUNK_TEXTURE_NAME);
            tn.cstr(second);
            tx.child(tn);
            texs.child(tx);
        }
        c.child(texs);
    }

    let mut pass = Chunk::wrapper(CHUNK_MATERIAL_PASS);
    let mut vmids = Chunk::new(CHUNK_VERTEX_MATERIAL_IDS);
    vmids.u32(0);
    pass.child(vmids);
    let mut sids = Chunk::new(CHUNK_SHADER_IDS);
    sids.u32(0);
    pass.child(sids);
    if textured {
        let mut stage = Chunk::wrapper(CHUNK_TEXTURE_STAGE);
        let mut tids = Chunk::new(CHUNK_TEXTURE_IDS);
        tids.u32(0);
        stage.child(tids);
        let mut uv = Chunk::new(CHUNK_STAGE_TEXCOORDS);
        for t in &m.uvs {
            uv.f32(t[0]).f32(1.0 - t[1]);
        }
        stage.child(uv);
        pass.child(stage);
    }
    if stage1.is_some() {
        // The BO2 maps use the mesh's UVs; for the environment the mapper generates this stage's
        // coordinates, and the mesh's own are written for a uniform vertex layout.
        let mut stage = Chunk::wrapper(CHUNK_TEXTURE_STAGE);
        let mut tids = Chunk::new(CHUNK_TEXTURE_IDS);
        tids.u32(1);
        stage.child(tids);
        let mut uv = Chunk::new(CHUNK_STAGE_TEXCOORDS);
        for t in &m.uvs {
            uv.f32(t[0]).f32(1.0 - t[1]);
        }
        stage.child(uv);
        pass.child(stage);
    }
    c.child(pass);
    c
}

/// A skinned model (`<name>.w3d`, conventionally `XXX_SKN`): its meshes and an HLOD binding them to
/// the hierarchy `htree`.
pub fn skin_model(name: &str, htree: &str, meshes: &[SkinMesh]) -> Vec<u8> {
    skin_model_with_box(name, htree, meshes, None)
}

/// An oriented box on the model's root, `(centre, half extents)`. Zero Hour picks units by casting
/// rays at their models, and a skin's own vertices sit bone-relative (unposed, by the feet), so
/// infantry carry a box (`BOUNDINGBOX`) to be clicked on.
pub type PickBox = ([f32; 3], [f32; 3]);

/// [`skin_model`] with a box to select the unit by.
pub fn skin_model_with_box(name: &str, htree: &str, meshes: &[SkinMesh], pick: Option<PickBox>) -> Vec<u8> {
    let mut out = Vec::new();
    for m in meshes {
        mesh_chunk(name, m).write_to(&mut out);
    }
    let box_name = format!("{name}.BOUNDINGBOX");
    if let Some((centre, extent)) = pick {
        let mut b = Chunk::new(CHUNK_BOX);
        // Oriented; physical and projectile collisions (rays hit it). Not drawn.
        b.u32(version(1, 0)).u32(0x0000_0001 | 0x0000_0010 | 0x0000_0020).name(&box_name, NAME_LEN * 2).u32(0);
        b.f32(centre[0]).f32(centre[1]).f32(centre[2]).f32(extent[0]).f32(extent[1]).f32(extent[2]);
        b.write_to(&mut out);
    }
    let mut hlod = Chunk::wrapper(CHUNK_HLOD);
    let mut header = Chunk::new(CHUNK_HLOD_HEADER);
    header.u32(HLOD_VERSION).u32(1).name(name, NAME_LEN).name(htree, NAME_LEN);
    hlod.child(header);
    let mut lod = Chunk::wrapper(CHUNK_HLOD_LOD_ARRAY);
    let mut lod_header = Chunk::new(CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER);
    lod_header.u32(meshes.len() as u32 + pick.is_some() as u32).f32(-1.0); // NO_MAX_SCREEN_SIZE
    lod.child(lod_header);
    for m in meshes {
        let mut sub = Chunk::new(CHUNK_HLOD_SUB_OBJECT);
        sub.u32(0).name(&format!("{name}.{}", m.name), NAME_LEN * 2);
        lod.child(sub);
    }
    if pick.is_some() {
        let mut sub = Chunk::new(CHUNK_HLOD_SUB_OBJECT);
        sub.u32(0).name(&box_name, NAME_LEN * 2);
        lod.child(sub);
    }
    hlod.child(lod);
    hlod.write_to(&mut out);
    out
}

/// Per-frame pose deltas for one pivot (relative to its base pose).
#[derive(Clone, Debug)]
pub struct Channel {
    pub pivot: u16,
    pub translations: Vec<[f32; 3]>,
    pub rotations: Vec<[f32; 4]>,
}

/// A raw animation (`<name>.w3d`), known to the game as `<htree>.<name>`.
pub fn animation(name: &str, htree: &str, frames: u32, frame_rate: u32, channels: &[Channel]) -> Vec<u8> {
    let mut a = Chunk::wrapper(CHUNK_ANIMATION);
    let mut header = Chunk::new(CHUNK_ANIMATION_HEADER);
    header.u32(HANIM_VERSION).name(name, NAME_LEN).name(htree, NAME_LEN).u32(frames).u32(frame_rate);
    a.child(header);
    for ch in channels {
        for axis in 0..3u16 {
            if ch.translations.iter().all(|t| t[axis as usize].abs() < 1e-5) {
                continue;
            }
            let mut c = Chunk::new(CHUNK_ANIMATION_CHANNEL);
            c.u16(0).u16(ch.translations.len() as u16 - 1).u16(1).u16(ANIM_CHANNEL_X + axis).u16(ch.pivot).u16(0);
            ch.translations.iter().for_each(|t| {
                c.f32(t[axis as usize]);
            });
            a.child(c);
        }
        if ch.rotations.iter().any(|q| (q[3].abs() - 1.0).abs() > 1e-5) {
            let mut c = Chunk::new(CHUNK_ANIMATION_CHANNEL);
            c.u16(0).u16(ch.rotations.len() as u16 - 1).u16(4).u16(ANIM_CHANNEL_Q).u16(ch.pivot).u16(0);
            for q in &ch.rotations {
                c.f32(q[0]).f32(q[1]).f32(q[2]).f32(q[3]);
            }
            a.child(c);
        }
    }
    let mut out = Vec::new();
    a.write_to(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(d: &[u8]) -> Vec<(u32, bool, usize)> {
        let mut out = Vec::new();
        let mut p = 0;
        while p + 8 <= d.len() {
            let id = u32::from_le_bytes(d[p..p + 4].try_into().unwrap());
            let sz = u32::from_le_bytes(d[p + 4..p + 8].try_into().unwrap());
            out.push((id, sz & 0x8000_0000 != 0, (sz & 0x7FFF_FFFF) as usize));
            p += 8 + (sz & 0x7FFF_FFFF) as usize;
        }
        assert_eq!(p, d.len(), "chunks tile the file");
        out
    }

    #[test]
    fn hierarchy_layout() {
        let pivots = vec![
            Pivot { name: "ROOTTRANSFORM".into(), parent: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0] },
            Pivot { name: "a_very_long_bone_name".into(), parent: Some(0), translation: [1.0, 2.0, 3.0], rotation: [0.0, 0.0, 0.0, 1.0] },
        ];
        let d = hierarchy("TEST_SKL", &pivots);
        let top = chunks(&d);
        assert_eq!(top, vec![(CHUNK_HIERARCHY, true, d.len() - 8)]);
        let inner = chunks(&d[8..]);
        assert_eq!(inner[0], (CHUNK_HIERARCHY_HEADER, false, 4 + 16 + 4 + 12));
        assert_eq!(inner[1], (CHUNK_PIVOTS, false, 2 * 60), "W3dPivotStruct is 60 bytes");
        assert_eq!(&d[8 + 8 + 36 + 8 + 60..8 + 8 + 36 + 8 + 60 + 16], b"a_very_long_bon\0", "names are cut to 15 characters");
    }

    #[test]
    fn skin_model_layout() {
        let m = SkinMesh {
            name: "BODY".into(),
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            bones: vec![1, 1, 2],
            triangles: vec![[0, 1, 2]],
            texture: Some("t.tga".into()),
            blend: Blend::Opaque,
            env_map: None,
            bo2_maps: None,
            emissive: false,
        };
        let d = skin_model("TEST_SKN", "TEST_SKL", &[m.clone()]);
        let top = chunks(&d);
        assert_eq!(top.iter().map(|c| c.0).collect::<Vec<_>>(), vec![CHUNK_MESH, CHUNK_HLOD]);
        let mesh = chunks(&d[8..8 + top[0].2]);
        let ids: Vec<u32> = mesh.iter().map(|c| c.0).collect();
        assert_eq!(ids[0], CHUNK_MESH_HEADER3);
        assert_eq!(mesh[0].2, 116, "W3dMeshHeader3Struct is 116 bytes");
        let tri = mesh.iter().find(|c| c.0 == CHUNK_TRIANGLES).unwrap();
        assert_eq!(tri.2, 32, "W3dTriangleStruct is 32 bytes");
        let inf = mesh.iter().find(|c| c.0 == CHUNK_VERTEX_INFLUENCES).unwrap();
        assert_eq!(inf.2, 3 * 8, "W3dVertInfStruct is 8 bytes");

        // A box to click: a 68-byte W3dBoxStruct, and one more sub-object.
        let d = skin_model_with_box("TEST_SKN", "TEST_SKL", &[m.clone()], Some(([0.0, 0.0, 6.0], [4.0, 4.0, 6.0])));
        let top = chunks(&d);
        assert_eq!(top.iter().map(|c| c.0).collect::<Vec<_>>(), vec![CHUNK_MESH, CHUNK_BOX, CHUNK_HLOD]);
        assert_eq!(top[1].2, 68);
        assert!(d.windows(20).any(|w| w == b"TEST_SKN.BOUNDINGBO"[..].iter().chain(b"X").copied().collect::<Vec<u8>>().as_slice()));

        // BO2 maps: a second texture on a second stage, with the detail stage off.
        let bo2 = SkinMesh { bo2_maps: Some("t_nsg.tga".into()), env_map: Some("env.tga".into()), ..m };
        let d = skin_model("TEST_SKN", "TEST_SKL", &[bo2]);
        let mesh = chunks(&d[8..8 + chunks(&d)[0].2]);
        let pass = mesh.iter().find(|c| c.0 == CHUNK_MATERIAL_PASS).unwrap();
        let start = 8 + 8 + mesh.iter().take_while(|c| c.0 != CHUNK_MATERIAL_PASS).map(|c| 8 + c.2).sum::<usize>();
        let stages = chunks(&d[start..start + pass.2]).iter().filter(|c| c.0 == CHUNK_TEXTURE_STAGE).count();
        assert_eq!(stages, 2);
        assert!(d.windows(9).any(|w| w == b"t_nsg.tga"));
        assert!(!d.windows(7).any(|w| w == b"env.tga"), "the BO2 maps take the environment's place");
    }
}
