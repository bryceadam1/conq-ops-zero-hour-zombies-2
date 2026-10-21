//! Third-person manifests (Track R7): a faction's body (`.zbc`) and a weapon's world model with
//! its world attachments (`.zbw`).
//!
//! BO2 draws a player in the world from the faction's MP body (`c_usa_mp_*_fb`, the same 72-inch
//! player skeleton every MP body shares) with the weapon's `world_model` hung on the body's
//! `tag_weapon_right` and the weapon's world attachments on their `worldModelTag`. This module is
//! the mod's own description of that composition: the body manifest names the body's model blob
//! and one animation blob per [`state`]; the weapon manifest lists the world model (the base part)
//! and each attachment (a later part, hanging from a bone of an earlier part, or from the base's
//! origin when BO2 names no tag).
//!
//! The two are composed at run time (the weapon changes with the General's loadout,
//! `zhbo_core::nativebody`), so the body manifest never names a weapon.

use crate::io::{FormatError, Reader, Writer};
use crate::viewmodel::Part;

/// The body clips a third-person General plays, picked from the sim's movement state (Track R7:
/// idle, walk, run, sprint, crouch, prone, ADS; the pistol's own stance). The numbers are the
/// manifest's clip ids, stable across builds.
pub mod state {
    pub const STAND_IDLE: u16 = 1;
    pub const STAND_WALK: u16 = 2;
    pub const STAND_RUN: u16 = 3;
    pub const STAND_SPRINT: u16 = 4;
    pub const STAND_ADS: u16 = 5;
    pub const CROUCH_IDLE: u16 = 6;
    pub const CROUCH_WALK: u16 = 7;
    pub const CROUCH_ADS: u16 = 8;
    pub const PRONE_IDLE: u16 = 9;
    pub const PRONE_CRAWL: u16 = 10;
    pub const STAND_IDLE_PISTOL: u16 = 11;
    pub const STAND_ADS_PISTOL: u16 = 12;
    /// One past the last state (the range the converter writes).
    pub const COUNT: u16 = 12;

    /// The states to try, in order, when the body has no clip for `state`: the nearest stance and
    /// the nearest gait, so a body missing (say) its crouch walk still crouches while it moves.
    pub fn fallbacks(state: u16) -> &'static [u16] {
        match state {
            STAND_WALK => &[STAND_WALK, STAND_RUN, STAND_IDLE],
            STAND_RUN => &[STAND_RUN, STAND_WALK, STAND_IDLE],
            STAND_SPRINT => &[STAND_SPRINT, STAND_RUN, STAND_WALK, STAND_IDLE],
            STAND_ADS => &[STAND_ADS, STAND_IDLE],
            CROUCH_IDLE => &[CROUCH_IDLE, STAND_IDLE],
            CROUCH_WALK => &[CROUCH_WALK, CROUCH_IDLE, STAND_WALK, STAND_RUN, STAND_IDLE],
            CROUCH_ADS => &[CROUCH_ADS, CROUCH_IDLE, STAND_ADS, STAND_IDLE],
            PRONE_IDLE => &[PRONE_IDLE, CROUCH_IDLE, STAND_IDLE],
            PRONE_CRAWL => &[PRONE_CRAWL, PRONE_IDLE, CROUCH_WALK, STAND_WALK, STAND_RUN, STAND_IDLE],
            STAND_IDLE_PISTOL => &[STAND_IDLE_PISTOL, STAND_IDLE],
            STAND_ADS_PISTOL => &[STAND_ADS_PISTOL, STAND_ADS, STAND_IDLE_PISTOL, STAND_IDLE],
            _ => &[STAND_IDLE],
        }
    }
}

pub const BODY_MAGIC: [u8; 4] = *b"ZBC\0";
pub const BODY_VERSION: u32 = 1;
pub const WEAPON_MAGIC: [u8; 4] = *b"ZBW\0";
pub const WEAPON_VERSION: u32 = 1;

/// A faction's third-person body: its model blob and one clip per [`state`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body {
    /// The faction letter (`U`, `C`, `G`).
    pub faction: u8,
    /// The body model blob's archive path ([`crate::model_path`]).
    pub model: String,
    /// The body bone a weapon hangs from (`tag_weapon_right`).
    pub weapon_tag: String,
    /// `(state id, animation blob path)`.
    pub clips: Vec<(u16, String)>,
}

impl Body {
    /// The clip for `state`: its own, else the nearest one the body has ([`state::fallbacks`]).
    pub fn clip(&self, state: u16) -> Option<&str> {
        state::fallbacks(state).iter().find_map(|&s| self.clips.iter().find(|c| c.0 == s).map(|c| c.1.as_str()))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(BODY_MAGIC, BODY_VERSION);
        w.u8(self.faction);
        w.str(&self.model);
        w.str(&self.weapon_tag);
        w.count(self.clips.len());
        for (state, path) in &self.clips {
            w.u16(*state);
            w.str(path);
        }
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Body, FormatError> {
        let mut r = Reader::open(data, BODY_MAGIC, "body", BODY_VERSION)?;
        let faction = r.u8()?;
        let model = r.str()?;
        let weapon_tag = r.str()?;
        let n = r.count(6)?;
        let mut clips = Vec::with_capacity(n);
        for _ in 0..n {
            clips.push((r.u16()?, r.str()?));
        }
        r.finish()?;
        Ok(Body { faction, model, weapon_tag, clips })
    }
}

/// A weapon as other players see it: the `world_model` (the base part) and its world attachments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weapon {
    /// The weapon id (`an94_mp+gl`) this was built for.
    pub id: String,
    /// The world model first (no tag), then each world attachment in order.
    pub parts: Vec<Part>,
    /// The bones BO2 hides for this combination (`hideTags`: iron sights under an optic, a
    /// magazine a fast mag replaces); the core collapses them when it composes.
    pub hide: Vec<String>,
}

impl Weapon {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(WEAPON_MAGIC, WEAPON_VERSION);
        w.str(&self.id);
        w.count(self.parts.len());
        for p in &self.parts {
            w.str(&p.model);
            w.opt_str(p.tag.as_deref());
            w.f32s(&p.offset.0);
            w.f32s(&p.offset.1);
            w.u8(p.rigid as u8);
        }
        w.count(self.hide.len());
        for h in &self.hide {
            w.str(h);
        }
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Weapon, FormatError> {
        let mut r = Reader::open(data, WEAPON_MAGIC, "world weapon", WEAPON_VERSION)?;
        let id = r.str()?;
        let n = r.count(34)?;
        let mut parts = Vec::with_capacity(n);
        for i in 0..n {
            let model = r.str()?;
            let tag = r.opt_str()?;
            if i == 0 && tag.is_some() {
                return Err(FormatError::Corrupt(format!("part {i}: only the base part has no tag")));
            }
            parts.push(Part { model, tag, offset: (r.f32n()?, r.f32n()?), rigid: r.u8()? != 0 });
        }
        let nh = r.count(4)?;
        let mut hide = Vec::with_capacity(nh);
        for _ in 0..nh {
            hide.push(r.str()?);
        }
        r.finish()?;
        Ok(Weapon { id, parts, hide })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::IDENTITY;

    fn body() -> Body {
        Body {
            faction: b'U',
            model: "Art\\Native\\c_usa_mp_seal6_assault_fb.zbm".into(),
            weapon_tag: "tag_weapon_right".into(),
            clips: vec![(state::STAND_IDLE, "Art\\Native\\pb_stand_alert.zba".into()), (state::STAND_RUN, "Art\\Native\\pb_combatrun_forward_loop.zba".into())],
        }
    }

    #[test]
    fn body_round_trips_and_falls_back_to_a_nearer_state() {
        let b = body();
        assert_eq!(Body::from_bytes(&b.to_bytes()).unwrap(), b);
        assert_eq!(b.clip(state::STAND_RUN), Some("Art\\Native\\pb_combatrun_forward_loop.zba"));
        // A missing state takes its nearest stance and gait.
        assert_eq!(b.clip(state::STAND_SPRINT), Some("Art\\Native\\pb_combatrun_forward_loop.zba"), "sprint runs when there is no sprint");
        assert_eq!(b.clip(state::CROUCH_WALK), Some("Art\\Native\\pb_combatrun_forward_loop.zba"), "a crouch walk walks when there is no crouch");
        assert_eq!(b.clip(state::PRONE_IDLE), Some("Art\\Native\\pb_stand_alert.zba"), "prone idles standing");
        // Every prefix is refused, and a bad magic or version is named.
        let bytes = b.to_bytes();
        for n in 0..bytes.len() {
            assert!(Body::from_bytes(&bytes[..n]).is_err(), "{n} bytes parsed");
        }
        assert!(matches!(Body::from_bytes(&[0; 8]), Err(FormatError::Magic { .. })));
    }

    fn weapon() -> Weapon {
        Weapon {
            id: "an94_mp+reflex".into(),
            parts: vec![
                Part { model: "Art\\Native\\t6_wpn_ar_an94_world.zbm".into(), tag: None, offset: IDENTITY, rigid: true },
                Part { model: "Art\\Native\\au_an94_reflex_world.zbm".into(), tag: Some("tag_reflex".into()), offset: ([0.0, 0.0, 0.0, 1.0], [2.0, 0.0, 3.0]), rigid: true },
            ],
            hide: vec!["tag_sight_on".into()],
        }
    }

    #[test]
    fn world_weapon_round_trips() {
        let w = weapon();
        assert_eq!(Weapon::from_bytes(&w.to_bytes()).unwrap(), w);
        let bytes = w.to_bytes();
        for n in 0..bytes.len() {
            assert!(Weapon::from_bytes(&bytes[..n]).is_err(), "{n} bytes parsed");
        }
        let mut no_base = w.clone();
        no_base.parts[0].tag = Some("x".into());
        assert!(Weapon::from_bytes(&no_base.to_bytes()).is_err());
    }
}
