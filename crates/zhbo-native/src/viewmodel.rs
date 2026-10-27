//! A viewmodel manifest (`.zbv`): what BO2 composes into one first-person `DObj`.
//!
//! BO2 builds a viewmodel from the faction's arms and the weapon's view model hung on the arms'
//! `tag_weapon`, the magazine on the gun's `j_gun`, and each attachment's `au_*` view model on its
//! `viewModelTag` at the attachment's view offset (`WeaponAttachmentUnique`). The manifest lists
//! those parts in order (a later part may hang from an earlier one's bone), the bones to hide
//! (iron sights under an optic, `hideTags`), the marker tags the converter adds where BO2 has none
//! (`tag_sights` on guns without one), and the animation of each clip id (BO2's
//! `weapAnimFiles_t` slot, the alt mode's after `ALT_CLIPS`, as `zhbo_core::viewmodel::slot`).

use crate::io::{FormatError, Reader, Writer};
use crate::math::Xf;

pub const MAGIC: [u8; 4] = *b"ZBV\0";
pub const VERSION: u32 = 2;

/// The left instance's bit in a clip id's high byte, for a dual-wield pair's split clips. The base
/// (right) instance keeps the plain id; the core's request for a left-gun clip sets this bit
/// (`id | INSTANCE_LEFT`), and the manifest's flat clip table carries both entries. BO2's own slot
/// table tops out at 0x58 + ALT_CLIPS (0x58), well below 0x100, so the bit is free.
pub const INSTANCE_LEFT: u16 = 0x100;

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// The model blob's archive path ([`crate::model_path`]).
    pub model: String,
    /// The bone of an earlier part this one hangs from; `None` for the base (the arms).
    pub tag: Option<String>,
    /// The part's origin in the tag's frame (BO2 inches): an attachment's view offset.
    pub offset: Xf,
    /// Not driven by the clips: it rides its tag as modelled (the reload's incoming magazine on
    /// `tag_clip1`, whose bones share the magazine's names).
    pub rigid: bool,
    /// An ADS model (BO2's `attachViewModel[7]` or an attachment unique's `viewModelADS`): drawn
    /// only while aiming, in place of the hip part that names it in [`Self::ads_swap`]. The W3D
    /// path draws only the hip models.
    pub ads_only: bool,
    /// The index of the [`Self::ads_only`] part BO2 draws instead of this one while aiming (the
    /// holo's `t6_attach_optic_holo_ads` for its `_view`, the M32's slot 7 for its slot 0), or
    /// `None` when this part keeps its model at ADS. Per part: only the paired parts swap (G14 §3,
    /// `FUN_00535170`). Viewmodel (`.zbv`) only: the third-person `.zbw` does not carry it.
    pub ads_swap: Option<u16>,
}

/// The part-record value of [`Part::ads_swap`]'s `None`.
const NO_SWAP: u16 = u16::MAX;

#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub name: String,
    /// An existing bone of the composed skeleton.
    pub parent: String,
    /// In the parent's frame (BO2 inches).
    pub local: Xf,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Viewmodel {
    /// The weapon id (`an94_mp+gl`) this was built for.
    pub id: String,
    /// The faction letter of the arms (`U`, `C`, `G`).
    pub faction: u8,
    pub parts: Vec<Part>,
    pub markers: Vec<Marker>,
    pub hide: Vec<String>,
    /// `(clip id, animation blob path)`.
    pub clips: Vec<(u16, String)>,
}

impl Viewmodel {
    pub fn clip(&self, id: u16) -> Option<&str> {
        self.clips.iter().find(|c| c.0 == id).map(|c| c.1.as_str())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(MAGIC, VERSION);
        w.str(&self.id);
        w.u8(self.faction);
        w.count(self.parts.len());
        for p in &self.parts {
            w.str(&p.model);
            w.opt_str(p.tag.as_deref());
            w.f32s(&p.offset.0);
            w.f32s(&p.offset.1);
            w.u8(p.rigid as u8);
            w.u8(p.ads_only as u8);
            w.u16(p.ads_swap.unwrap_or(NO_SWAP));
        }
        w.count(self.markers.len());
        for m in &self.markers {
            w.str(&m.name);
            w.str(&m.parent);
            w.f32s(&m.local.0);
            w.f32s(&m.local.1);
        }
        w.count(self.hide.len());
        for h in &self.hide {
            w.str(h);
        }
        w.count(self.clips.len());
        for (id, path) in &self.clips {
            w.u16(*id);
            w.str(path);
        }
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Viewmodel, FormatError> {
        let mut r = Reader::open(data, MAGIC, "viewmodel", VERSION)?;
        let id = r.str()?;
        let faction = r.u8()?;
        let np = r.count(36)?;
        let mut parts = Vec::with_capacity(np);
        for i in 0..np {
            let model = r.str()?;
            let tag = r.opt_str()?;
            if (i == 0) != tag.is_none() {
                return Err(FormatError::Corrupt(format!("part {i}: only the base part has no tag")));
            }
            let (offset, rigid, ads_only) = ((r.f32n()?, r.f32n()?), r.u8()? != 0, r.u8()? != 0);
            let ads_swap = Some(r.u16()?).filter(|&s| s != NO_SWAP);
            parts.push(Part { model, tag, offset, rigid, ads_only, ads_swap });
        }
        // A swap names an ADS-only part; an ADS-only part never swaps itself.
        for (i, p) in parts.iter().enumerate() {
            if let Some(s) = p.ads_swap {
                if s as usize == i || !parts.get(s as usize).is_some_and(|t| t.ads_only) || p.ads_only {
                    return Err(FormatError::Corrupt(format!("part {i}: ADS swap {s} is not an ADS-only part")));
                }
            }
        }
        let nm = r.count(36)?;
        let mut markers = Vec::with_capacity(nm);
        for _ in 0..nm {
            markers.push(Marker { name: r.str()?, parent: r.str()?, local: (r.f32n()?, r.f32n()?) });
        }
        let nh = r.count(4)?;
        let mut hide = Vec::with_capacity(nh);
        for _ in 0..nh {
            hide.push(r.str()?);
        }
        let nc = r.count(6)?;
        let mut clips = Vec::with_capacity(nc);
        for _ in 0..nc {
            clips.push((r.u16()?, r.str()?));
        }
        r.finish()?;
        Ok(Viewmodel { id, faction, parts, markers, hide, clips })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::IDENTITY;

    #[test]
    fn round_trips() {
        let v = Viewmodel {
            id: "an94_mp+reflex".into(),
            faction: b'U',
            parts: vec![
                Part { model: "Art\\Native\\arms.zbm".into(), tag: None, offset: IDENTITY, rigid: false, ads_only: false, ads_swap: None },
                Part { model: "Art\\Native\\gun.zbm".into(), tag: Some("tag_weapon".into()), offset: IDENTITY, rigid: false, ads_only: false, ads_swap: None },
                Part { model: "Art\\Native\\holo_view.zbm".into(), tag: Some("tag_weapon".into()), offset: ([0.0, 0.0, 0.0, 1.0], [3.0, 0.0, 4.5]), rigid: true, ads_only: false, ads_swap: Some(3) },
                Part { model: "Art\\Native\\holo_ads.zbm".into(), tag: Some("tag_weapon".into()), offset: ([0.0, 0.0, 0.0, 1.0], [3.0, 0.0, 4.5]), rigid: true, ads_only: true, ads_swap: None },
            ],
            markers: vec![Marker { name: "tag_sights".into(), parent: "tag_weapon".into(), local: ([0.0, 0.0, 0.0, 1.0], [2.0, 0.0, 5.0]) }],
            hide: vec!["tag_sight_on".into()],
            clips: vec![(1, "Art\\Native\\idle.zba".into()), (0x10, "Art\\Native\\reload.zba".into())],
        };
        let bytes = v.to_bytes();
        assert_eq!(Viewmodel::from_bytes(&bytes).unwrap(), v);
        assert_eq!(v.clip(0x10), Some("Art\\Native\\reload.zba"));
        for n in 0..bytes.len() {
            assert!(Viewmodel::from_bytes(&bytes[..n]).is_err());
        }
        let mut no_base = v.clone();
        no_base.parts[0].tag = Some("x".into());
        assert!(Viewmodel::from_bytes(&no_base.to_bytes()).is_err());
        // A swap must name an ADS-only part: not a hip part, not itself, not out of range.
        for bad in [1u16, 2, 9] {
            let mut v2 = v.clone();
            v2.parts[2].ads_swap = Some(bad);
            assert!(Viewmodel::from_bytes(&v2.to_bytes()).is_err(), "swap to {bad}");
        }
    }
}
