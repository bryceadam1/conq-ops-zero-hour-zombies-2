//! Posing: BO2's `DObj` composition, sampling a clip, layering, notetracks and the skinning
//! palette.
//!
//! **Composition.** The base model's bones come first; each later part's bones are appended, its
//! root bones hung from the part's tag (an earlier bone, by name) with the part's offset in the
//! tag's frame, exactly as `zhbo_assets::character::Rig` builds the W3D hierarchy (which was checked
//! in game). Animations address bones by name, so a name that occurs twice (the arms' `tag_weapon`
//! and the gun's own root) is driven on both.
//!
//! **Sampling.** A bone the clip has a track for takes the track's rotation, and its translation
//! added to the bone's own local offset (`anim_base`: the model's local position for a child bone,
//! zero for a root, where BO2's track is the full offset); untracked bones keep the bind pose.
//!
//! **Hiding.** BO2 hides a tag (`hideTags`, an optic's iron sights) by collapsing it: the bone and
//! everything under it skin with a zero matrix, so their triangles vanish.

use crate::anim::{Anim, Note};
use crate::math::{apply, compose, inverse, mat4, nlerp, qmul, qconj, qnorm, Mat4, Xf, IDENTITY};
use crate::model::Model;
use crate::viewmodel::Marker;

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Bind pose in the parent's frame.
    pub bind_local: Xf,
    /// Added to a track's translation (see the module docs).
    pub anim_base: [f32; 3],
    /// Bind pose in the composed model's space.
    pub bind: Xf,
    /// Inverse of the bone's bind pose in its own model's space: a vertex of that model, skinned to
    /// this bone, is `pose * inv_bind * vertex`.
    pub inv_bind: Xf,
    pub hidden: bool,
    /// Driven by the clips (false for a rigid part's bones).
    pub animated: bool,
}

/// One part to compose: a model, the bone it hangs from, its offset in that bone's frame, and
/// whether the clips leave it alone (`viewmodel::Part::rigid`).
#[derive(Clone, Copy)]
pub struct PartRef<'a> {
    pub model: &'a Model,
    pub tag: Option<&'a str>,
    pub offset: Xf,
    pub rigid: bool,
}

/// One composed part: where its bones start in the skeleton.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartBones {
    pub first: usize,
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub parts: Vec<PartBones>,
}

impl Skeleton {
    /// Compose `parts` (a model, the tag it hangs from, its offset in the tag's frame) in order,
    /// then the marker tags, then hide `hide` and everything under it. A part whose tag is
    /// missing hangs from the base's origin (logged by the caller: BO2 would fail the attach).
    pub fn compose(parts: &[PartRef<'_>], markers: &[Marker], hide: &[String]) -> (Skeleton, Vec<String>) {
        let mut s = Skeleton::default();
        let mut problems = Vec::new();
        for &PartRef { model, tag, offset, rigid } in parts {
            let attach = tag.and_then(|t| {
                let at = s.bone(t);
                if at.is_none() {
                    problems.push(format!("{}: no tag {t} to hang from", model.name));
                }
                at
            });
            let origin = compose(attach.map_or(IDENTITY, |i| s.bones[i].bind), offset);
            let first = s.bones.len();
            for b in &model.bones {
                let own = (b.rotation, b.position);
                let world = compose(origin, own);
                let parent = match b.parent {
                    Some(p) => Some(first + p as usize),
                    None => attach,
                };
                let bind_local = compose(inverse(parent.map_or(IDENTITY, |p| s.bones[p].bind)), world);
                let anim_base = if b.parent.is_some() { b.local_position } else { [0.0; 3] };
                s.bones.push(Bone { name: b.name.clone(), parent, bind_local, anim_base, bind: world, inv_bind: inverse(own), hidden: false, animated: !rigid });
            }
            s.parts.push(PartBones { first, count: model.bones.len() });
        }
        for m in markers {
            let Some(parent) = s.bone(&m.parent) else {
                problems.push(format!("marker {}: no parent {}", m.name, m.parent));
                continue;
            };
            let bind = compose(s.bones[parent].bind, m.local);
            s.bones.push(Bone { name: m.name.clone(), parent: Some(parent), bind_local: m.local, anim_base: m.local.1, bind, inv_bind: inverse(bind), hidden: false, animated: true });
        }
        for i in 0..s.bones.len() {
            let hidden_here = hide.iter().any(|h| h == &s.bones[i].name);
            let under_hidden = s.bones[i].parent.is_some_and(|p| s.bones[p].hidden);
            s.bones[i].hidden = hidden_here || under_hidden;
        }
        (s, problems)
    }

    /// The first bone named `name`.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// The bind pose, local.
    pub fn bind_pose(&self) -> Vec<Xf> {
        self.bones.iter().map(|b| b.bind_local).collect()
    }

    /// `anim` at `frame`, local. `out` is reused (no per-frame allocation once sized).
    pub fn sample(&self, anim: &Anim, frame: f32, out: &mut Vec<Xf>) {
        out.clear();
        for b in &self.bones {
            let mut local = b.bind_local;
            if let Some(t) = anim.bones.iter().find(|t| b.animated && t.name == b.name) {
                if let Some(q) = t.rotation_at(frame) {
                    local.0 = q;
                }
                if let Some(p) = t.translation_at(frame) {
                    local.1 = [b.anim_base[0] + p[0], b.anim_base[1] + p[1], b.anim_base[2] + p[2]];
                }
            }
            out.push(local);
        }
    }

    /// Model-space transforms of a local pose (parents precede children).
    pub fn model_space(&self, local: &[Xf], out: &mut Vec<Xf>) {
        out.clear();
        for (i, b) in self.bones.iter().enumerate() {
            let l = local.get(i).copied().unwrap_or(b.bind_local);
            out.push(match b.parent {
                Some(p) => compose(out[p], l),
                None => l,
            });
        }
    }

    /// The skinning palette: `root * pose * inv_bind` per bone (hidden bones collapse to zero),
    /// with `root` placing the model in the world.
    pub fn palette(&self, model_space: &[Xf], root: &Mat4, out: &mut Vec<Mat4>) {
        out.clear();
        for (i, b) in self.bones.iter().enumerate() {
            if b.hidden {
                out.push([0.0; 16]);
                continue;
            }
            let x = compose(model_space.get(i).copied().unwrap_or(b.bind), b.inv_bind);
            out.push(crate::math::mat4_mul(root, &mat4(x)));
        }
    }

    /// Where bone `i` is in model space for a pose.
    pub fn bone_point(&self, model_space: &[Xf], i: usize) -> [f32; 3] {
        apply(model_space[i], [0.0; 3])
    }
}

/// Crossfade `from` into `to` by `t` (0: all `from`), per bone: BO2's blend between the outgoing
/// and incoming clips.
pub fn blend(from: &[Xf], to: &mut [Xf], t: f32) {
    let t = t.clamp(0.0, 1.0);
    for (a, b) in from.iter().zip(to.iter_mut()) {
        b.0 = nlerp(a.0, b.0, t);
        b.1 = [0, 1, 2].map(|k| a.1[k] + (b.1[k] - a.1[k]) * t);
    }
}

/// Add `anim`'s change since its first frame on top of `base` (an additive layer, weighted).
pub fn add(skel: &Skeleton, base: &mut [Xf], anim: &Anim, frame: f32, weight: f32) {
    for (i, b) in skel.bones.iter().enumerate() {
        let Some(t) = anim.bones.iter().find(|t| b.animated && t.name == b.name) else { continue };
        if let (Some(q0), Some(q)) = (t.rotation_at(0.0), t.rotation_at(frame)) {
            let delta = nlerp([0.0, 0.0, 0.0, 1.0], qmul(q, qconj(q0)), weight);
            base[i].0 = qnorm(qmul(delta, base[i].0));
        }
        if let (Some(p0), Some(p)) = (t.translation_at(0.0), t.translation_at(frame)) {
            for k in 0..3 {
                base[i].1[k] += (p[k] - p0[k]) * weight;
            }
        }
    }
}

/// The notetracks a clip crosses going from `from` to `to` seconds (`from` excluded): each note
/// fires once per pass. A loop that wrapped fires the notes past `from` and those before `to`;
/// `from < 0` means the clip (re)started, so a note at its very start fires too.
pub fn notes_crossed<'a>(anim: &'a Anim, from: f32, to: f32, looping: bool) -> impl Iterator<Item = &'a Note> + 'a {
    let d = anim.duration().max(1e-6);
    let norm = |t: f32| if looping { (t / d).rem_euclid(1.0) } else { (t / d).min(1.0) };
    let start = if from < 0.0 { -1.0 } else { norm(from) };
    let end = norm(to.max(0.0));
    let wrapped = looping && from >= 0.0 && (to / d).floor() > (from / d).floor();
    anim.notes.iter().filter(move |n| if wrapped { n.time > start || n.time <= end } else { n.time > start && n.time <= end })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::tests::bend;
    use crate::math::{mat4_point, MAT4_IDENTITY};
    use crate::model::tests::strip;

    fn part<'a>(model: &'a Model, tag: Option<&'a str>, offset: Xf) -> PartRef<'a> {
        PartRef { model, tag, offset, rigid: false }
    }

    #[test]
    fn rigid_parts_ignore_the_clips() {
        let m = strip();
        let (skel, _) = Skeleton::compose(&[PartRef { model: &m, tag: None, offset: IDENTITY, rigid: true }], &[], &[]);
        let mut local = Vec::new();
        skel.sample(&bend(), 10.0, &mut local);
        assert_eq!(local, skel.bind_pose());
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < 1e-3)
    }

    fn skin(skel: &Skeleton, model: &Model, part: usize, pal: &[Mat4], v: usize) -> [f32; 3] {
        let vert = &model.surfaces[0].vertices[v];
        let mut out = [0.0; 3];
        for k in 0..4 {
            let m = &pal[skel.parts[part].first + vert.bones[k] as usize];
            let p = mat4_point(m, vert.position);
            for c in 0..3 {
                out[c] += p[c] * vert.weights[k];
            }
        }
        out
    }

    #[test]
    fn bind_pose_skins_vertices_where_bo2_put_them() {
        let m = strip();
        let (skel, problems) = Skeleton::compose(&[part(&m, None, IDENTITY)], &[], &[]);
        assert!(problems.is_empty());
        let (mut ms, mut pal) = (Vec::new(), Vec::new());
        skel.model_space(&skel.bind_pose(), &mut ms);
        skel.palette(&ms, &MAT4_IDENTITY, &mut pal);
        for v in 0..3 {
            assert!(close(skin(&skel, &m, 0, &pal, v), m.surfaces[0].vertices[v].position), "vertex {v}");
        }
    }

    #[test]
    fn a_clip_bends_the_child_and_its_translation_adds_to_the_local_offset() {
        let m = strip();
        let (skel, _) = Skeleton::compose(&[part(&m, None, IDENTITY)], &[], &[]);
        let a = bend();
        let (mut local, mut ms, mut pal) = (Vec::new(), Vec::new(), Vec::new());
        skel.sample(&a, 10.0, &mut local);
        assert!(close(local[1].1, [10.0, 0.0, 2.0]), "local offset 10\" plus the track's 2\": {:?}", local[1].1);
        skel.model_space(&local, &mut ms);
        skel.palette(&ms, &MAT4_IDENTITY, &mut pal);
        // The tip (10" past the arm bone) turns a quarter turn about Z at the raised arm.
        assert!(close(skin(&skel, &m, 0, &pal, 2), [10.0, 10.0, 2.0]), "{:?}", skin(&skel, &m, 0, &pal, 2));
    }

    #[test]
    fn attached_parts_follow_their_tag_and_hidden_tags_collapse() {
        let arms = strip();
        let mut gun = strip();
        gun.name = "gun".into();
        gun.bones[0].name = "tag_weapon_root".into();
        gun.bones[1].name = "tag_sight_on".into();
        let offset = (crate::math::QUAT_IDENTITY, [0.0, 0.0, 1.0]);
        let markers = [Marker { name: "tag_sights".into(), parent: "tag_weapon_root".into(), local: (crate::math::QUAT_IDENTITY, [5.0, 0.0, 3.0]) }];
        let (skel, problems) = Skeleton::compose(&[part(&arms, None, IDENTITY), part(&gun, Some("j_arm"), offset)], &markers, &["tag_sight_on".to_string()]);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(skel.bones.len(), 5);
        let (mut local, mut ms, mut pal) = (Vec::new(), Vec::new(), Vec::new());
        skel.sample(&bend(), 10.0, &mut local);
        skel.model_space(&local, &mut ms);
        // The gun's root rides the arm: arm at (10,0,2) turned 90° about Z, plus the 1" offset.
        assert!(close(skel.bone_point(&ms, 2), [10.0, 0.0, 3.0]), "{:?}", skel.bone_point(&ms, 2));
        let sights = skel.bone("tag_sights").unwrap();
        assert!(close(skel.bone_point(&ms, sights), [10.0, 5.0, 6.0]), "{:?}", skel.bone_point(&ms, sights));
        skel.palette(&ms, &MAT4_IDENTITY, &mut pal);
        assert!(skel.bones[3].hidden && pal[3] == [0.0; 16], "the hidden tag collapses");
        assert!(!skel.bones[2].hidden);
        let (_, missing) = Skeleton::compose(&[part(&arms, None, IDENTITY), part(&gun, Some("nope"), IDENTITY)], &[], &[]);
        assert_eq!(missing.len(), 1);
    }

    #[test]
    fn notetracks_fire_once_per_pass() {
        let a = bend(); // one note at 0.5 of a 1 s clip
        assert_eq!(notes_crossed(&a, 0.4, 0.6, false).count(), 1);
        assert_eq!(notes_crossed(&a, 0.5, 0.6, false).count(), 0, "already fired at 0.5");
        assert_eq!(notes_crossed(&a, -1.0, 0.5, false).count(), 1, "a restart fires from the start");
        assert_eq!(notes_crossed(&a, 0.9, 1.6, true).count(), 1, "a wrapped loop fires past its seam");
        assert_eq!(notes_crossed(&a, 0.6, 1.4, true).count(), 0);
    }

    #[test]
    fn blend_and_add_layers() {
        let m = strip();
        let (skel, _) = Skeleton::compose(&[part(&m, None, IDENTITY)], &[], &[]);
        let bind = skel.bind_pose();
        let mut bent = Vec::new();
        skel.sample(&bend(), 10.0, &mut bent);
        let mut half = bent.clone();
        blend(&bind, &mut half, 0.5);
        assert!((half[1].1[2] - 1.0).abs() < 1e-4, "halfway up");
        let mut layered = bind.clone();
        add(&skel, &mut layered, &bend(), 10.0, 1.0);
        assert!(close(layered[1].1, [10.0, 0.0, 2.0]));
    }
}
