//! An animation blob (`.zba`): one BO2 `XAnimParts` as keyframe tracks.
//!
//! Tracks are BO2's own (zhbo_formats::xanim decodes them): rotations replace the bone's local
//! rotation, translations are offsets added to the bone's local offset (the rule the W3D path was
//! verified with in game; `zhbo_assets::character::animate`). A bone the clip has no track for keeps
//! its bind pose. Notetracks are BO2's notifies, at 0..1 through the clip.

use crate::io::{FormatError, Reader, Writer};
use crate::math::{lerp3, nlerp, Quat, Vec3};

pub const MAGIC: [u8; 4] = *b"ZBA\0";
pub const VERSION: u32 = 1;

/// Keyframes `(frame, value)`, frames ascending. One key: constant.
pub type Track<T> = Vec<(u16, T)>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoneTrack {
    pub name: String,
    pub rotation: Option<Track<Quat>>,
    pub translation: Option<Track<Vec3>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub name: String,
    /// 0..1 through the clip.
    pub time: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anim {
    pub name: String,
    /// Frames are numbered `0..=num_frames`.
    pub num_frames: u16,
    pub frame_rate: f32,
    pub looping: bool,
    /// BO2's `bDelta`: the clip carries root motion (not applied to viewmodels).
    pub delta: bool,
    /// BO2's `assetType` byte, kept for the additive/relative clips the player layers.
    pub asset_type: u8,
    pub bones: Vec<BoneTrack>,
    pub notes: Vec<Note>,
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

impl BoneTrack {
    pub fn rotation_at(&self, frame: f32) -> Option<Quat> {
        self.rotation.as_ref().map(|t| sample(t, frame, nlerp))
    }
    pub fn translation_at(&self, frame: f32) -> Option<Vec3> {
        self.translation.as_ref().map(|t| sample(t, frame, lerp3))
    }
}

impl Anim {
    /// Seconds from the first frame to the last.
    pub fn duration(&self) -> f32 {
        self.num_frames as f32 / self.frame_rate.max(1e-3)
    }

    /// The frame (fractional) at `time` seconds: wrapped for a loop, held at the end otherwise
    /// (the bridge's W3D rule, `ZhboViewmodel.cpp` `loops`).
    pub fn frame_at(&self, time: f32, looping: bool) -> f32 {
        let frames = self.num_frames as f32;
        let f = time.max(0.0) * self.frame_rate;
        if frames <= 0.0 {
            0.0
        } else if looping {
            f % frames
        } else {
            f.min(frames)
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(MAGIC, VERSION);
        w.str(&self.name);
        w.u16(self.num_frames);
        w.f32(self.frame_rate);
        w.u8(self.looping as u8);
        w.u8(self.delta as u8);
        w.u8(self.asset_type);
        w.count(self.bones.len());
        for b in &self.bones {
            w.str(&b.name);
            match &b.rotation {
                Some(t) => {
                    w.count(t.len());
                    for (f, q) in t {
                        w.u16(*f);
                        w.f32s(q);
                    }
                }
                None => w.count(0),
            }
            match &b.translation {
                Some(t) => {
                    w.count(t.len());
                    for (f, p) in t {
                        w.u16(*f);
                        w.f32s(p);
                    }
                }
                None => w.count(0),
            }
        }
        w.count(self.notes.len());
        for n in &self.notes {
            w.str(&n.name);
            w.f32(n.time);
        }
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Anim, FormatError> {
        let mut r = Reader::open(data, MAGIC, "animation", VERSION)?;
        let name = r.str()?;
        let num_frames = r.u16()?;
        let frame_rate = r.f32()?;
        if frame_rate <= 0.0 {
            return Err(FormatError::Corrupt(format!("frame rate {frame_rate}")));
        }
        let looping = r.u8()? != 0;
        let delta = r.u8()? != 0;
        let asset_type = r.u8()?;
        let check = |frames: &[u16], what: &str| -> Result<(), FormatError> {
            if frames.windows(2).any(|w| w[0] >= w[1]) || frames.iter().any(|&f| f > num_frames) {
                return Err(FormatError::Corrupt(format!("{what} keys out of order or past frame {num_frames}")));
            }
            Ok(())
        };
        let nb = r.count(12)?;
        let mut bones = Vec::with_capacity(nb);
        for _ in 0..nb {
            let name = r.str()?;
            let nr = r.count(18)?;
            let mut rot = Vec::with_capacity(nr);
            for _ in 0..nr {
                rot.push((r.u16()?, r.f32n::<4>()?));
            }
            let nt = r.count(14)?;
            let mut tr = Vec::with_capacity(nt);
            for _ in 0..nt {
                tr.push((r.u16()?, r.f32n::<3>()?));
            }
            check(&rot.iter().map(|k| k.0).collect::<Vec<_>>(), "rotation")?;
            check(&tr.iter().map(|k| k.0).collect::<Vec<_>>(), "translation")?;
            bones.push(BoneTrack { name, rotation: (!rot.is_empty()).then_some(rot), translation: (!tr.is_empty()).then_some(tr) });
        }
        let nn = r.count(8)?;
        let mut notes = Vec::with_capacity(nn);
        for _ in 0..nn {
            notes.push(Note { name: r.str()?, time: r.f32()? });
        }
        r.finish()?;
        Ok(Anim { name, num_frames, frame_rate, looping, delta, asset_type, bones, notes })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::math::axis_angle;

    /// Turns `j_arm` a quarter turn about Z over 10 frames and slides it 2" up; one notetrack.
    pub(crate) fn bend() -> Anim {
        Anim {
            name: "bend".into(),
            num_frames: 10,
            frame_rate: 10.0,
            looping: false,
            delta: false,
            asset_type: 0,
            bones: vec![BoneTrack {
                name: "j_arm".into(),
                rotation: Some(vec![(0, [0.0, 0.0, 0.0, 1.0]), (10, axis_angle([0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2))]),
                translation: Some(vec![(0, [0.0; 3]), (10, [0.0, 0.0, 2.0])]),
            }],
            notes: vec![Note { name: "sound_wpn_mag_out".into(), time: 0.5 }],
        }
    }

    #[test]
    fn round_trips_and_refuses_bad_keys() {
        let a = bend();
        let bytes = a.to_bytes();
        assert_eq!(Anim::from_bytes(&bytes).unwrap(), a);
        for n in 0..bytes.len() {
            assert!(Anim::from_bytes(&bytes[..n]).is_err(), "prefix {n}");
        }
        let mut late = bend();
        late.bones[0].rotation.as_mut().unwrap()[1].0 = 11;
        assert!(Anim::from_bytes(&late.to_bytes()).is_err());
    }

    #[test]
    fn samples_between_keys_and_holds_or_wraps() {
        let a = bend();
        let t = a.bones[0].translation_at(5.0).unwrap();
        assert!((t[2] - 1.0).abs() < 1e-5);
        assert_eq!(a.frame_at(2.0, false), 10.0, "held at the end");
        assert!((a.frame_at(1.25, true) - 2.5).abs() < 1e-4, "wrapped");
        assert!((a.duration() - 1.0).abs() < 1e-6);
    }
}
