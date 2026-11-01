/*
** Conquer Ops - native viewmodel: where the drawn gun sits relative to the eye.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The hold metric: the gun the native pass drew last frame, measured in the eye's frame.
//!
//! A gun is "held" when the eye is behind and to the left of it: the camera sits outside the gun's
//! geometry and the gun fills the lower right of the screen. The XPR-50 once rendered with the eye
//! *inside* its receiver (its hip rig root is authored in the ADS layer, which the hip skipped), and
//! no image-free check could see it. [`measure`] skins every drawn vertex of the gun's parts (every
//! manifest part but the arms, part 0) by the last pose, exactly as the pass's palette does, and
//! returns the eye-frame bounds in inches; [`Hold::eye_inside`] is the failure the sweep and the
//! install test flag. Development and test tool: on no frame path.

use super::{Loaded, Mesh, Posed};
use zhbo_native::math::Mat4;

/// One composed part's drawn extent in the eye frame (+X forward, +Y left, +Z up), in inches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartBox {
    /// The manifest part (composition index; 0 is the arms, never measured).
    pub part: u16,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub vertices: usize,
    /// Which of the six axis rays from the eye (+X, −X, +Y, −Y, +Z, −Z) cross one of the part's
    /// drawn triangles, as bits.
    pub hits: u8,
}

impl PartBox {
    /// The eye is inside this box on all three axes at once, by more than `margin` inches.
    pub fn contains_eye(&self, margin: f32) -> bool {
        (0..3).all(|k| self.mins[k] < -margin && self.maxs[k] > margin)
    }

    /// The eye is enclosed: inside the box, and the part's triangles lie across at least five of the
    /// six axis rays from it ([`ENCLOSED_RAYS`]). A receiver open at one end still surrounds the
    /// eye (the XPR-50's did, blocking five), while a long part merely passing beside the head
    /// (a shoulder-fired tube, whose box spans it) blocks one, and a part whose box is stretched by
    /// a stray vertex far away blocks none.
    pub fn encloses_eye(&self, margin: f32) -> bool {
        self.contains_eye(margin) && self.hits.count_ones() >= ENCLOSED_RAYS
    }
}

/// The drawn gun in the eye frame: every gun part's box, the union, and the vertex nearest the eye.
#[derive(Clone, Debug, PartialEq)]
pub struct Hold {
    pub parts: Vec<PartBox>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// The gun vertex nearest the eye, and its distance (inches).
    pub nearest: [f32; 3],
    pub nearest_distance: f32,
    /// How many gun vertices were measured, and how many lie behind the eye (x < 0).
    pub vertices: usize,
    pub behind: usize,
}

impl Hold {
    /// The first gun part that encloses the eye ([`PartBox::encloses_eye`]) — the camera inside the
    /// receiver, a magazine or an optic. Per part, not the union: a stray part elsewhere in the
    /// frame would make a union box swallow the eye without any geometry near it. At the hip a held
    /// gun hangs in front, to the right and below, so no part surrounds the eye.
    pub fn eye_inside(&self, margin: f32) -> Option<&PartBox> {
        self.parts.iter().find(|p| p.encloses_eye(margin))
    }
}

/// Measure the gun `posed` drew from `loaded`: the hip mesh below the optic switch, the aimed
/// mesh from it (the pass's own choice, `gpu::draw`). `eye`/`dir` are the frame the pose was
/// placed in (`ZhboNativeIn`). `None` when the composite has no gun part with a drawn vertex.
pub fn measure(loaded: &Loaded, posed: &Posed, ads: f32, eye: [f32; 3], dir: [f32; 3]) -> Option<Hold> {
    let mesh: &Mesh = if ads >= super::OPTIC_VIEWMODEL_SWITCH { loaded.mesh_aimed.as_ref().unwrap_or(&loaded.mesh) } else { &loaded.mesh };
    let frame = super::eye_frame(eye, dir);
    let axes = [[frame[0], frame[1], frame[2]], [frame[4], frame[5], frame[6]], [frame[8], frame[9], frame[10]]];
    let inch = 1.0 / zhbo_native::ART_SCALE;
    let mut eye_space: Vec<Option<[f32; 3]>> = vec![None; mesh.vertices.len()];
    let mut hold: Option<Hold> = None;
    for s in mesh.surfaces.iter().filter(|s| s.part != 0) {
        let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
        for &i in mesh.indices.get(range).unwrap_or(&[]) {
            let i = i as usize;
            if eye_space[i].is_some() {
                continue;
            }
            let v = &mesh.vertices[i];
            let world = skin(&posed.palette, v.position, &v.bones, &v.weights);
            let rel = [world[0] - eye[0], world[1] - eye[1], world[2] - eye[2]];
            let p = axes.map(|a| (a[0] * rel[0] + a[1] * rel[1] + a[2] * rel[2]) * inch);
            eye_space[i] = Some(p);
            let d = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            let h = hold.get_or_insert(Hold { parts: Vec::new(), mins: p, maxs: p, nearest: p, nearest_distance: d, vertices: 0, behind: 0 });
            let part = match h.parts.iter().position(|b| b.part == s.part) {
                Some(k) => k,
                None => {
                    h.parts.push(PartBox { part: s.part, mins: p, maxs: p, vertices: 0, hits: 0 });
                    h.parts.len() - 1
                }
            };
            let b = &mut h.parts[part];
            b.vertices += 1;
            for (k, &c) in p.iter().enumerate() {
                b.mins[k] = b.mins[k].min(c);
                b.maxs[k] = b.maxs[k].max(c);
                h.mins[k] = h.mins[k].min(c);
                h.maxs[k] = h.maxs[k].max(c);
            }
            if d < h.nearest_distance {
                (h.nearest, h.nearest_distance) = (p, d);
            }
            h.vertices += 1;
            h.behind += usize::from(p[0] < 0.0);
        }
    }
    // The ray pass: which axis rays from the eye each part's triangles cross.
    if let Some(h) = &mut hold {
        for s in mesh.surfaces.iter().filter(|s| s.part != 0) {
            let Some(b) = h.parts.iter_mut().find(|b| b.part == s.part) else { continue };
            let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
            for tri in mesh.indices.get(range).unwrap_or(&[]).as_chunks::<3>().0 {
                if b.hits == 0b11_1111 {
                    break;
                }
                let Some([a, c, d]) = tri.map(|i| eye_space[i as usize]).into_iter().collect::<Option<Vec<_>>>().map(|v| [v[0], v[1], v[2]]) else { continue };
                for (bit, dir) in RAYS.iter().enumerate() {
                    if b.hits & (1 << bit) == 0 && ray_hits(*dir, a, c, d) {
                        b.hits |= 1 << bit;
                    }
                }
            }
        }
    }
    hold
}

/// How many of the six axis rays a part must block to enclose the eye ([`PartBox::encloses_eye`]).
/// Measured over the whole catalog: the XPR-50 inside its receiver blocked 5; every part whose box
/// merely held the eye blocked at most 1 (the SMAW/FHJ-18 tubes 1, the HAMR's stretched box 0).
pub const ENCLOSED_RAYS: u32 = 5;

/// The six axis rays from the eye, in [`PartBox::hits`] bit order.
const RAYS: [[f32; 3]; 6] = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];

/// Does the ray from the origin along `dir` cross triangle `(a, b, c)` (Möller–Trumbore, either
/// winding: the art's back faces count, an enclosure is geometry, not shading)?
fn ray_hits(dir: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    let sub = |p: [f32; 3], q: [f32; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let cross = |p: [f32; 3], q: [f32; 3]| [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
    let dot = |p: [f32; 3], q: [f32; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let (e1, e2) = (sub(b, a), sub(c, a));
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-9 {
        return false;
    }
    let inv = 1.0 / det;
    let t = [-a[0], -a[1], -a[2]];
    let u = dot(t, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(t, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    dot(e2, q) * inv > 0.0
}

/// The last native frame's gun, measured ([`measure`]) — what a sweep or install test reads after
/// driving `zhbo_native_viewmodel` with the same `eye`/`dir`/`ads` it passed.
pub fn last(ads: f32, eye: [f32; 3], dir: [f32; 3]) -> Option<Hold> {
    super::ffi::with_loaded(|loaded, posed| measure(loaded, posed, ads, eye, dir)).flatten()
}

/// Load, compose and pose the viewmodel `inp` names on a private state — the game's own frame
/// (`ffi::State::frame`), the ADS layer and placement included — answering every file request from
/// `files` (`None`: the archive lacks it), then measure that frame. The process-wide native state
/// is untouched, so an install test can walk the catalog. `Err` names why nothing was posed.
pub fn hold_of(inp: &super::ffi::ZhboNativeIn, files: impl Fn(&str) -> Option<Vec<u8>>) -> Result<Hold, String> {
    let mut s = super::ffi::State::default();
    // Each round answers every outstanding request; the clips arrive a round after the models.
    // Two quiet rounds in a row (nothing asked) means the frame posed with everything loaded.
    let mut quiet = 0;
    for _ in 0..64 {
        s.frame(inp, true);
        let wanted: Vec<String> = s.files.wanted().map(str::to_string).collect();
        if wanted.is_empty() {
            quiet += 1;
            if quiet == 2 {
                break;
            }
            continue;
        }
        quiet = 0;
        for w in &wanted {
            let bytes = files(w);
            s.files.provide(w, bytes.as_deref());
        }
    }
    let Some(loaded) = s.loaded.as_ref() else {
        return Err(s.log.iter().cloned().collect::<Vec<_>>().join("; "));
    };
    measure(loaded, &s.posed, inp.ads, inp.eye, inp.dir).ok_or_else(|| "no gun part drew a vertex".to_string())
}

/// One vertex skinned by the palette, as the pass's vertex shader does (weights renormalized by
/// the composer; a zero-weight slot contributes nothing).
fn skin(palette: &[Mat4], p: [f32; 3], bones: &[u16; 4], weights: &[f32; 4]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    let mut total = 0.0;
    for (&b, &w) in bones.iter().zip(weights) {
        if w <= 0.0 {
            continue;
        }
        let Some(m) = palette.get(b as usize) else { continue };
        for (r, o) in out.iter_mut().enumerate() {
            *o += w * (m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r]);
        }
        total += w;
    }
    if total > 0.0 && (total - 1.0).abs() > 1e-4 {
        out = out.map(|c| c / total);
    }
    out
}
