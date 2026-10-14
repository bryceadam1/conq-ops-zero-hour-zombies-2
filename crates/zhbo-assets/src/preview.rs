//! A tiny software rasterizer for checking converted models by eye: LOD 0, front and side views,
//! flat-shaded, side by side in one PNG. BO2 models are Z-up with X forward.

use std::path::Path;
use zhbo_formats::xmodel::Model;

const H: usize = 512;
const W: usize = H * 2;

pub fn render(model: &Model, out: &Path) -> std::io::Result<()> {
    let mut rgb = vec![40u8; W * H * 3];
    let mut depth = vec![f32::NEG_INFINITY; W * H];
    let lod = &model.lods[0];
    let surfs = &model.surfaces[lod.first_surface..lod.first_surface + lod.surface_count];

    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in surfs.iter().flat_map(|s| &s.vertices) {
        for k in 0..3 {
            lo[k] = lo[k].min(v.position[k]);
            hi[k] = hi[k].max(v.position[k]);
        }
    }
    let extent = (0..3).map(|k| hi[k] - lo[k]).fold(1e-3f32, f32::max) * 1.1;
    let scale = H as f32 / extent;
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];

    // view 0: from the front (looking down -X): screen x = -Y, depth = X.
    // view 1: from the side (looking down +Y): screen x = X, depth = -Y.
    for view in 0..2 {
        let project = |p: [f32; 3]| -> [f32; 3] {
            let (sx, d) = if view == 0 { (-(p[1] - center[1]), p[0]) } else { (p[0] - center[0], -p[1]) };
            let sy = -(p[2] - center[2]);
            [(view * H) as f32 + H as f32 / 2.0 + sx * scale, H as f32 / 2.0 + sy * scale, d]
        };
        for s in surfs {
            for t in &s.triangles {
                let [a, b, c] = t.map(|i| s.vertices[i as usize].position);
                let n = cross(sub(b, a), sub(c, a));
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                let light = [0.6f32, -0.5, 0.62];
                let shade = ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len).abs() * 0.75 + 0.2;
                fill(&mut rgb, &mut depth, view, [project(a), project(b), project(c)], shade);
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

fn fill(rgb: &mut [u8], depth: &mut [f32], view: usize, p: [[f32; 3]; 3], shade: f32) {
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
                let c = (shade.clamp(0.0, 1.0) * 255.0) as u8;
                rgb[3 * i..3 * i + 3].copy_from_slice(&[c, c, (c as f32 * 0.9) as u8]);
            }
        }
    }
}
