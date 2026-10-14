//! A tiny software rasterizer for checking converted models by eye: LOD 0, front and side views,
//! flat-shaded, side by side in one PNG. BO2 models are Z-up with X forward.
//!
//! `pose` skins models with an animation frame, so animation decoding can be checked the same way.

use std::path::Path;
use std::rc::Rc;
use zhbo_formats::iwi::Rgba;
use zhbo_formats::xanim::Anim;
use zhbo_formats::xmodel::Model;

const H: usize = 512;
const W: usize = H * 2;

/// One surface, ready to draw.
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u16; 3]>,
    pub texture: Option<Rc<Rgba>>,
}

/// Colour map per surface of a model (same index as `Model::surfaces`), if known.
pub type Textures = Vec<Option<Rc<Rgba>>>;

fn lod0(model: &Model) -> &[zhbo_formats::xmodel::Surface] {
    let lod = &model.lods[0];
    &model.surfaces[lod.first_surface..lod.first_surface + lod.surface_count]
}

pub fn render(model: &Model, textures: &Textures, out: &Path) -> std::io::Result<()> {
    let first = model.lods[0].first_surface;
    let meshes: Vec<Mesh> = lod0(model)
        .iter()
        .enumerate()
        .map(|(i, s)| Mesh {
            positions: s.vertices.iter().map(|v| v.position).collect(),
            uvs: s.vertices.iter().map(|v| v.uv).collect(),
            triangles: s.triangles.clone(),
            texture: textures.get(first + i).cloned().flatten(),
        })
        .collect();
    render_meshes(&meshes, out)
}

use zhbo_assets::xf::{apply, compose, inverse, Xf};

/// Skin `models` with `anim` at `frame`. Each model may hang its root bone from a named bone of
/// an earlier model (a gun's `j_gun` on the arms' `tag_weapon`).
pub fn pose(models: &[(&Model, &Textures, Option<&str>)], anim: &Anim, frame: f32) -> Vec<Mesh> {
    let mut meshes = Vec::new();
    let mut placed: Vec<(String, Xf)> = Vec::new(); // posed world transforms of bones so far
    for &(model, textures, attach) in models {
        let attach_xf = attach.and_then(|name| placed.iter().find(|(n, _)| n == name).map(|p| p.1));
        let mut world: Vec<Xf> = Vec::with_capacity(model.bones.len());
        for b in &model.bones {
            let track = anim.bones.iter().find(|t| t.name == b.name);
            let rot = track.and_then(|t| t.rotation_at(frame));
            let pos = track.and_then(|t| t.translation_at(frame));
            let xf = match b.parent {
                // Animated translations are offsets from the bone's local offset; rotations replace.
                Some(p) => compose(
                    world[p],
                    (rot.unwrap_or(b.local_rotation), pos.map_or(b.local_position, |t| [0, 1, 2].map(|k| b.local_position[k] + t[k]))),
                ),
                None => match attach_xf {
                    // An attached root follows its tag, plus whatever the animation does to it.
                    Some(tag) => compose(tag, (rot.unwrap_or([0.0, 0.0, 0.0, 1.0]), pos.unwrap_or([0.0; 3]))),
                    None => (rot.unwrap_or(b.rotation), pos.unwrap_or(b.position)),
                },
            };
            world.push(xf);
        }
        let skin: Vec<Xf> = model.bones.iter().zip(&world).map(|(b, &w)| compose(w, inverse((b.rotation, b.position)))).collect();
        let first = model.lods[0].first_surface;
        for (i, s) in lod0(model).iter().enumerate() {
            let positions = s
                .vertices
                .iter()
                .map(|v| {
                    v.weights.iter().fold([0.0f32; 3], |acc, &(bone, w)| {
                        let p = apply(skin[bone as usize], v.position);
                        [acc[0] + p[0] * w, acc[1] + p[1] * w, acc[2] + p[2] * w]
                    })
                })
                .collect();
            meshes.push(Mesh {
                positions,
                uvs: s.vertices.iter().map(|v| v.uv).collect(),
                triangles: s.triangles.clone(),
                texture: textures.get(first + i).cloned().flatten(),
            });
        }
        placed.extend(model.bones.iter().zip(&world).map(|(b, &w)| (b.name.clone(), w)));
    }
    meshes
}

pub fn render_meshes(meshes: &[Mesh], out: &Path) -> std::io::Result<()> {
    let mut rgb = vec![40u8; W * H * 3];
    let mut depth = vec![f32::NEG_INFINITY; W * H];

    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in meshes.iter().flat_map(|m| &m.positions) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let extent = (0..3).map(|k| hi[k] - lo[k]).fold(1e-3f32, f32::max) * 1.1;
    let scale = H as f32 / extent;
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];

    // view 0: from the front (looking down -X): screen x = Y, depth = X.
    // view 1: from the side (looking down +Y): screen x = X, depth = -Y.
    for view in 0..2 {
        let project = |p: [f32; 3]| -> [f32; 3] {
            let (sx, d) = if view == 0 { (p[1] - center[1], p[0]) } else { (p[0] - center[0], -p[1]) };
            let sy = -(p[2] - center[2]);
            [(view * H) as f32 + H as f32 / 2.0 + sx * scale, H as f32 / 2.0 + sy * scale, d]
        };
        for m in meshes {
            for t in &m.triangles {
                let [a, b, c] = t.map(|i| m.positions[i as usize]);
                let uv = t.map(|i| m.uvs.get(i as usize).copied().unwrap_or([0.0, 0.0]));
                let n = cross(sub(b, a), sub(c, a));
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                let light = [0.6f32, -0.5, 0.62];
                let shade = ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len).abs() * 0.75 + 0.2;
                fill(&mut rgb, &mut depth, view, [project(a), project(b), project(c)], shade, uv, m.texture.as_deref());
            }
        }
    }
    let file = std::fs::File::create(out)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(&rgb).map_err(std::io::Error::other)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn fill(rgb: &mut [u8], depth: &mut [f32], view: usize, p: [[f32; 3]; 3], shade: f32, uv: [[f32; 2]; 3], tex: Option<&Rgba>) {
    let x0 = (p.iter().map(|v| v[0]).fold(f32::MAX, f32::min).floor().max((view * H) as f32)) as usize;
    let x1 = (p.iter().map(|v| v[0]).fold(f32::MIN, f32::max).ceil().min(((view + 1) * H - 1) as f32)) as isize;
    let y0 = (p.iter().map(|v| v[1]).fold(f32::MAX, f32::min).floor().max(0.0)) as usize;
    let y1 = (p.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil().min((H - 1) as f32)) as isize;
    let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    if area.abs() < 1e-6 || x1 < x0 as isize || y1 < y0 as isize {
        return;
    }
    for y in y0..=y1 as usize {
        for x in x0..=x1 as usize {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((p[1][0] - px) * (p[2][1] - py) - (p[2][0] - px) * (p[1][1] - py)) / area;
            let w1 = ((p[2][0] - px) * (p[0][1] - py) - (p[0][0] - px) * (p[2][1] - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let d = w0 * p[0][2] + w1 * p[1][2] + w2 * p[2][2];
            let i = y * W + x;
            if d > depth[i] {
                depth[i] = d;
                let base = match tex {
                    Some(t) if t.width > 0 && t.height > 0 => {
                        let u = w0 * uv[0][0] + w1 * uv[1][0] + w2 * uv[2][0];
                        let v = w0 * uv[0][1] + w1 * uv[1][1] + w2 * uv[2][1];
                        let tx = ((u.rem_euclid(1.0) * t.width as f32) as u32).min(t.width - 1);
                        let ty = ((v.rem_euclid(1.0) * t.height as f32) as u32).min(t.height - 1);
                        let o = 4 * (ty * t.width + tx) as usize;
                        [t.pixels[o] as f32, t.pixels[o + 1] as f32, t.pixels[o + 2] as f32]
                    }
                    _ => [255.0, 255.0, 230.0],
                };
                let k = shade.clamp(0.0, 1.0) * 0.6 + 0.4;
                rgb[3 * i..3 * i + 3].copy_from_slice(&base.map(|c| (c * k).min(255.0) as u8));
            }
        }
    }
}
