//! Black Ops II (T6) `XAnimParts` animations, read out of a decrypted zone (`fastfile::Zone`).
//!
//! Read from a fully loaded zone (`t6zone::Zone`): the struct's pointers lead to the bone names
//! (script strings), notifies, and the pools `dataByte`, `dataShort`, `dataInt`, `randomDataShort`,
//! `randomDataByte` and the frame `indices`.
//!
//! Bones come grouped by rotation kind (none, half, full, half constant, full constant) in name
//! order; translations name their bone with a byte. Animated tracks are keyframes: a frame index
//! per key and a value per key. Rotations are int16 quaternions (/32767); "half" ones store only
//! Z and W (a turn about Z). Translations are `mins + value * size`, `size` already divided by 255
//! (8-bit) or 65535 (16-bit). Values are local to the parent bone.
//!
//! Layout and decoding from OpenAssetTools (`T6_Assets.h`, `XAnimParts.txt`, `FlatXAnimReader.cpp`,
//! GPL-3.0). The delta part (root motion) is not decoded; the bone tracks are.

use crate::t6zone::Zone;
use crate::FormatError;

// boneCount[] slots (PART_TYPE_*).
const NO_QUAT: usize = 0;
const HALF_QUAT: usize = 1;
const FULL_QUAT: usize = 2;
const HALF_QUAT_NO_SIZE: usize = 3;
const FULL_QUAT_NO_SIZE: usize = 4;
const SMALL_TRANS: usize = 5;
const TRANS: usize = 6;
const TRANS_NO_SIZE: usize = 7;
const NO_TRANS: usize = 8;
const ALL: usize = 9;

/// Keyframes: (frame, value). A single key means the value is constant.
pub type Track<T> = Vec<(u16, T)>;

#[derive(Clone, Debug, Default)]
pub struct BoneTrack {
    pub name: String,
    /// `None`: the animation does not rotate this bone.
    pub rotation: Option<Track<[f32; 4]>>,
    /// `None`: the animation does not move this bone.
    pub translation: Option<Track<[f32; 3]>>,
}

#[derive(Clone, Debug)]
pub struct Notify {
    pub name: String,
    /// 0..1 through the animation.
    pub time: f32,
}

#[derive(Clone, Debug)]
pub struct Anim {
    pub name: String,
    /// Frames are numbered 0..=num_frames.
    pub num_frames: u16,
    pub frame_rate: f32,
    pub looping: bool,
    pub bones: Vec<BoneTrack>,
    pub notifies: Vec<Notify>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}


/// One of the typed data pools, consumed front to back.
struct Pool<'a> {
    d: &'a [u8],
    elem: usize,
    name: &'static str,
}

impl<'a> Pool<'a> {
    fn pop(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let bytes = n * self.elem;
        if self.d.len() < bytes {
            return Err(FormatError::Corrupt(format!("animation {} pool exhausted", self.name)));
        }
        let (head, tail) = self.d.split_at(bytes);
        self.d = tail;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.pop(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16_at(self.pop(1)?, 0))
    }
    fn f32x3(&mut self) -> Result<[f32; 3], FormatError> {
        let b = self.pop(3)?;
        Ok([f32_at(b, 0), f32_at(b, 4), f32_at(b, 8)])
    }
}

struct Pools<'a> {
    byte: Pool<'a>,
    short: Pool<'a>,
    int: Pool<'a>,
    random_short: Pool<'a>,
    random_byte: Pool<'a>,
    indices: Pool<'a>,
    byte_indices: bool,
}

impl Pools<'_> {
    /// Frame indices for a track of `stored + 1` keys (FlatXAnimReader's ReadPackedIndices).
    fn indices(&mut self, stored: u16) -> Result<Vec<u16>, FormatError> {
        let count = stored as usize + 1;
        if self.byte_indices {
            return Ok(self.byte.pop(count)?.to_vec().into_iter().map(u16::from).collect());
        }
        if stored >= 64 {
            let b = self.indices.pop(count)?;
            // The game keeps checkpoints of long index lists in dataShort: skip them.
            self.short.pop((count - 2) / 256 + 2)?;
            return Ok((0..count).map(|i| u16_at(b, 2 * i)).collect());
        }
        let b = self.short.pop(count)?;
        Ok((0..count).map(|i| u16_at(b, 2 * i)).collect())
    }
}

fn quat(v: [i16; 4]) -> [f32; 4] {
    let q = v.map(|c| c as f32 / 32767.0);
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 1e-6 { q.map(|c| c / len) } else { [0.0, 0.0, 0.0, 1.0] }
}

fn shorts(b: &[u8], n: usize, at: usize) -> Vec<i16> {
    (0..n).map(|i| u16_at(b, at + 2 * i) as i16).collect()
}

/// Find an animation by name in a loaded zone.
pub fn find(zone: &Zone, name: &str) -> Result<Anim, FormatError> {
    let asset = zone
        .find(crate::fastfile::ASSET_XANIM, name)
        .ok_or_else(|| FormatError::Corrupt(format!("no animation named {name} in zone {}", zone.name)))?;
    read(zone, asset.addr)
}

/// Read the animation whose struct is at `h`.
pub fn read(z: &Zone, h: u32) -> Result<Anim, FormatError> {
    let name = z.str_ptr(h).trim_start_matches(',').to_string();
    let counts: Vec<usize> = (0..10).map(|i| z.u8(h + 24 + i) as usize).collect();
    let data_byte_count = z.u16(h + 4) as usize;
    let data_short_count = z.u16(h + 6) as usize;
    let data_int_count = z.u16(h + 8) as usize;
    let random_byte_count = z.u16(h + 10) as usize;
    let num_frames = z.u16(h + 14);
    let looping = z.u8(h + 16) != 0;
    let notify_count = z.u8(h + 34) as u32;
    let random_short_count = z.u32(h + 40) as usize;
    let index_count = z.u32(h + 44) as usize;
    let frame_rate = z.f32(h + 48);
    let byte_indices = num_frames < 256;
    let bone_count = counts[ALL];

    let names_ptr = z.u32(h + 64);
    let names: Vec<String> = (0..bone_count as u32)
        .map(|i| if names_ptr != 0 { z.script_string(z.u16(names_ptr + 2 * i)).to_string() } else { String::new() })
        .collect();
    let notify_ptr = z.u32(h + 96);
    let notifies = if notify_ptr == 0 {
        Vec::new()
    } else {
        (0..notify_count)
            .map(|i| Notify { name: z.script_string(z.u16(notify_ptr + 8 * i)).to_string(), time: z.f32(notify_ptr + 8 * i + 4) })
            .collect()
    };
    let pool = |ptr_at: u32, count: usize, elem: usize, what: &'static str| -> Pool {
        let ptr = z.u32(h + ptr_at);
        let d = if ptr != 0 { z.bytes(ptr, count * elem) } else { &[][..] };
        Pool { d, elem, name: what }
    };
    let mut p = Pools {
        byte: pool(68, data_byte_count, 1, "dataByte"),
        short: pool(72, data_short_count, 2, "dataShort"),
        int: pool(76, data_int_count, 4, "dataInt"),
        random_short: pool(80, random_short_count, 2, "randomDataShort"),
        random_byte: pool(84, random_byte_count, 1, "randomDataByte"),
        indices: pool(92, index_count, if byte_indices { 1 } else { 2 }, "indices"),
        byte_indices,
    };
    let mut bones: Vec<BoneTrack> = names.into_iter().map(|name| BoneTrack { name, ..Default::default() }).collect();
    let mut b = 0usize;
    b += counts[NO_QUAT];
    for kind in [HALF_QUAT, FULL_QUAT] {
        let comps = if kind == HALF_QUAT { 2 } else { 4 };
        for _ in 0..counts[kind] {
            let stored = p.short.u16()?;
            let frames = p.indices(stored)?;
            let v = p.random_short.pop(comps * frames.len())?;
            let s = shorts(v, comps * frames.len(), 0);
            bones[b].rotation = Some(
                frames
                    .iter()
                    .enumerate()
                    .map(|(k, &f)| {
                        let q = if comps == 2 { [0, 0, s[2 * k], s[2 * k + 1]] } else { [s[4 * k], s[4 * k + 1], s[4 * k + 2], s[4 * k + 3]] };
                        (f, quat(q))
                    })
                    .collect(),
            );
            b += 1;
        }
    }
    for kind in [HALF_QUAT_NO_SIZE, FULL_QUAT_NO_SIZE] {
        let comps = if kind == HALF_QUAT_NO_SIZE { 2 } else { 4 };
        for _ in 0..counts[kind] {
            let s = shorts(p.short.pop(comps)?, comps, 0);
            let q = if comps == 2 { [0, 0, s[0], s[1]] } else { [s[0], s[1], s[2], s[3]] };
            bones[b].rotation = Some(vec![(0, quat(q))]);
            b += 1;
        }
    }
    let bone_index = |p: &mut Pools, bones: &[BoneTrack]| -> Result<usize, FormatError> {
        let i = p.byte.u8()? as usize;
        if i >= bones.len() {
            return Err(FormatError::Corrupt(format!("translation for bone {i} of {}", bones.len())));
        }
        Ok(i)
    };
    for kind in [SMALL_TRANS, TRANS] {
        for _ in 0..counts[kind] {
            let i = bone_index(&mut p, &bones)?;
            let stored = p.short.u16()?;
            let mins = p.int.f32x3()?;
            let size = p.int.f32x3()?;
            let frames = p.indices(stored)?;
            let n = frames.len();
            let values: Vec<[f32; 3]> = if kind == SMALL_TRANS {
                let v = p.random_byte.pop(3 * n)?;
                (0..n).map(|k| [0, 1, 2].map(|c| mins[c] + v[3 * k + c] as f32 * size[c])).collect()
            } else {
                let v = p.random_short.pop(3 * n)?;
                (0..n).map(|k| [0, 1, 2].map(|c| mins[c] + u16_at(v, 2 * (3 * k + c)) as f32 * size[c])).collect()
            };
            bones[i].translation = Some(frames.into_iter().zip(values).collect());
        }
    }
    for _ in 0..counts[TRANS_NO_SIZE] {
        let i = bone_index(&mut p, &bones)?;
        bones[i].translation = Some(vec![(0, p.int.f32x3()?)]);
    }
    for _ in 0..counts[NO_TRANS] {
        bone_index(&mut p, &bones)?;
    }
    // Every pool must be used up exactly; anything else means the layout was misread.
    for pool in [&p.byte, &p.short, &p.int, &p.random_short, &p.random_byte, &p.indices] {
        if !pool.d.is_empty() {
            return Err(FormatError::Corrupt(format!("animation {name}: {} bytes left in {}", pool.d.len(), pool.name)));
        }
    }
    Ok(Anim { name, num_frames, frame_rate, looping, bones, notifies })
}

fn sample<T: Copy>(track: &Track<T>, frame: f32, lerp: impl Fn(T, T, f32) -> T) -> T {
    let i = track.partition_point(|k| (k.0 as f32) <= frame);
    if i == 0 {
        return track[0].1;
    }
    if i >= track.len() {
        return track[track.len() - 1].1;
    }
    let (f0, a) = track[i - 1];
    let (f1, b) = track[i];
    lerp(a, b, (frame - f0 as f32) / (f1 as f32 - f0 as f32).max(1e-6))
}

pub fn nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    let q: [f32; 4] = [0, 1, 2, 3].map(|i| a[i] * (1.0 - t) + b[i] * s * t);
    let len = q.iter().map(|c| c * c).sum::<f32>().sqrt().max(1e-9);
    q.map(|c| c / len)
}

impl BoneTrack {
    /// Local rotation at `frame` (fractional), if this bone's rotation is animated.
    pub fn rotation_at(&self, frame: f32) -> Option<[f32; 4]> {
        self.rotation.as_ref().map(|t| sample(t, frame, nlerp))
    }
    /// Local translation at `frame`, if this bone's translation is animated.
    pub fn translation_at(&self, frame: f32) -> Option<[f32; 3]> {
        self.translation.as_ref().map(|t| sample(t, frame, |a, b, s| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_keyframes() {
        let t = BoneTrack { name: "b".into(), rotation: None, translation: Some(vec![(0, [0.0; 3]), (10, [10.0, 0.0, 0.0])]) };
        assert_eq!(t.translation_at(5.0), Some([5.0, 0.0, 0.0]));
        assert_eq!(t.translation_at(20.0), Some([10.0, 0.0, 0.0]));
        let q = nlerp([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0], 0.5);
        assert!((q[2] - q[3]).abs() < 1e-6);
    }

    /// Needs the player's BO2 install (with Multiplayer).
    #[test]
    #[ignore]
    fn reads_a_viewmodel_animation() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/common_mp.ff")).unwrap();
        let zone = crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap();
        let anim = find(&zone, "viewmodel_mp7_reload").unwrap();
        assert!(anim.num_frames > 30 && anim.frame_rate > 0.0, "{} frames at {}", anim.num_frames, anim.frame_rate);
        assert!(anim.bones.iter().any(|b| b.name == "j_gun"));
        assert!(anim.bones.iter().any(|b| b.rotation.as_ref().is_some_and(|r| r.len() > 1)), "something is keyed");
        for b in &anim.bones {
            for t in [0.0, anim.num_frames as f32 / 2.0, anim.num_frames as f32] {
                if let Some(q) = b.rotation_at(t) {
                    assert!(q.iter().all(|c| c.is_finite()));
                }
                if let Some(p) = b.translation_at(t) {
                    assert!(p.iter().all(|c| c.abs() < 200.0), "{} {p:?}", b.name);
                }
            }
        }
    }
}
