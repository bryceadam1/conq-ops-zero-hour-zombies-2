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
/// idle, walk, run, sprint, crouch, prone, ADS) and the weapon in his hand.
///
/// BO2 animates a player by the weapon he holds as much as by his stance: `mp/playeranim.script`
/// matches the weapon's `weapClass` and `playerAnimType` (a pistol walks `pb_combatwalk_forward_
/// loop_pistol`, a launcher `pb_walk_forward_RPG_ads`, the knife `pb_knife_walk_forward`). So a
/// clip id is an **animation set** (the converter's group of weapons the script animates alike,
/// `WeaponDef::body_anims`; set 0 is the rifleman's) and a movement state:
/// `id = set * SET_STRIDE + state`. The state numbers are the manifest's clip ids for set 0,
/// stable across builds.
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
    /// The moving states' other directions (`mp/playeranim.script`'s `direction backward`,
    /// `left`, `right` under `walk`/`run`): each forward gait's backward, left and right, in that
    /// order ([`directional`]).
    pub const STAND_WALK_BACK: u16 = 11;
    pub const STAND_WALK_LEFT: u16 = 12;
    pub const STAND_WALK_RIGHT: u16 = 13;
    pub const STAND_RUN_BACK: u16 = 14;
    pub const STAND_RUN_LEFT: u16 = 15;
    pub const STAND_RUN_RIGHT: u16 = 16;
    pub const CROUCH_WALK_BACK: u16 = 17;
    pub const CROUCH_WALK_LEFT: u16 = 18;
    pub const CROUCH_WALK_RIGHT: u16 = 19;
    pub const PRONE_CRAWL_BACK: u16 = 20;
    pub const PRONE_CRAWL_LEFT: u16 = 21;
    pub const PRONE_CRAWL_RIGHT: u16 = 22;
    /// Moving while aimed (the script's `walk` with `weapon_position ads`: "the player walks when
    /// they are ADS"), standing and crouched, forward then backward, left, right.
    pub const STAND_ADS_WALK: u16 = 23;
    pub const STAND_ADS_WALK_BACK: u16 = 24;
    pub const STAND_ADS_WALK_LEFT: u16 = 25;
    pub const STAND_ADS_WALK_RIGHT: u16 = 26;
    pub const CROUCH_ADS_WALK: u16 = 27;
    pub const CROUCH_ADS_WALK_BACK: u16 = 28;
    pub const CROUCH_ADS_WALK_LEFT: u16 = 29;
    pub const CROUCH_ADS_WALK_RIGHT: u16 = 30;
    /// The last movement state (states are `1..=COUNT`).
    pub const COUNT: u16 = 30;
    /// The states the hit boxes are posed in (`1..=POSED`): the forward gaits and the stances.
    /// A directional state's boxes are its forward gait's ([`fallbacks`]).
    pub const POSED: u16 = 10;
    /// Clip ids per animation set: `id = set * SET_STRIDE + state`.
    pub const SET_STRIDE: u16 = 32;
    /// The most animation sets a body carries: the rifleman's and one per model condition the W3D
    /// body has for a set (`zhbo_core::names::ANIM_SET_CONDITIONS`).
    pub const MAX_SETS: u8 = 9;

    /// The script's `direction` values a moving state carries ([`super::torso::DIRECTIONS`]'s
    /// indices): 1 forward, 2 backward, 3 left, 4 right; 0 (none) reads as forward.
    pub const fn directional(forward: u16, direction: u8) -> u16 {
        let k = match direction {
            2 => 1,
            3 => 2,
            4 => 3,
            _ => return forward,
        };
        match forward {
            STAND_WALK => STAND_WALK_BACK + k - 1,
            STAND_RUN => STAND_RUN_BACK + k - 1,
            CROUCH_WALK => CROUCH_WALK_BACK + k - 1,
            PRONE_CRAWL => PRONE_CRAWL_BACK + k - 1,
            STAND_ADS_WALK => STAND_ADS_WALK + k,
            CROUCH_ADS_WALK => CROUCH_ADS_WALK + k,
            _ => forward,
        }
    }

    /// A state's forward gait and its `direction` index (1 forward, 2 backward, 3 left, 4 right;
    /// 0 for a state that does not move).
    pub const fn split_direction(st: u16) -> (u16, u8) {
        match st {
            STAND_WALK | STAND_RUN | CROUCH_WALK | PRONE_CRAWL | STAND_ADS_WALK | CROUCH_ADS_WALK => (st, 1),
            STAND_WALK_BACK..=STAND_WALK_RIGHT => (STAND_WALK, (st - STAND_WALK_BACK) as u8 + 2),
            STAND_RUN_BACK..=STAND_RUN_RIGHT => (STAND_RUN, (st - STAND_RUN_BACK) as u8 + 2),
            CROUCH_WALK_BACK..=CROUCH_WALK_RIGHT => (CROUCH_WALK, (st - CROUCH_WALK_BACK) as u8 + 2),
            PRONE_CRAWL_BACK..=PRONE_CRAWL_RIGHT => (PRONE_CRAWL, (st - PRONE_CRAWL_BACK) as u8 + 2),
            STAND_ADS_WALK_BACK..=STAND_ADS_WALK_RIGHT => (STAND_ADS_WALK, (st - STAND_ADS_WALK) as u8 + 1),
            CROUCH_ADS_WALK_BACK..=CROUCH_ADS_WALK_RIGHT => (CROUCH_ADS_WALK, (st - CROUCH_ADS_WALK) as u8 + 1),
            _ => (st, 0),
        }
    }

    /// The clip id of movement `state` in animation set `set`.
    pub const fn id(set: u8, state: u16) -> u16 {
        set as u16 * SET_STRIDE + state
    }

    /// A clip id's `(set, state)`.
    pub const fn split(id: u16) -> (u8, u16) {
        ((id / SET_STRIDE) as u8, id % SET_STRIDE)
    }

    /// The movement states to try, in order, when the body has no clip for `state`: the nearest
    /// stance and the nearest gait, so a body missing (say) its crouch walk still crouches while it
    /// moves. A direction falls back to its forward gait's chain.
    pub fn nearest(state: u16) -> &'static [u16] {
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
            STAND_ADS_WALK => &[STAND_ADS_WALK, STAND_WALK, STAND_ADS, STAND_RUN, STAND_IDLE],
            CROUCH_ADS_WALK => &[CROUCH_ADS_WALK, CROUCH_WALK, CROUCH_ADS, CROUCH_IDLE, STAND_WALK, STAND_IDLE],
            STAND_WALK_BACK => &[STAND_WALK_BACK, STAND_RUN_BACK, STAND_WALK, STAND_RUN, STAND_IDLE],
            STAND_WALK_LEFT => &[STAND_WALK_LEFT, STAND_RUN_LEFT, STAND_WALK, STAND_RUN, STAND_IDLE],
            STAND_WALK_RIGHT => &[STAND_WALK_RIGHT, STAND_RUN_RIGHT, STAND_WALK, STAND_RUN, STAND_IDLE],
            STAND_RUN_BACK => &[STAND_RUN_BACK, STAND_WALK_BACK, STAND_RUN, STAND_WALK, STAND_IDLE],
            STAND_RUN_LEFT => &[STAND_RUN_LEFT, STAND_WALK_LEFT, STAND_RUN, STAND_WALK, STAND_IDLE],
            STAND_RUN_RIGHT => &[STAND_RUN_RIGHT, STAND_WALK_RIGHT, STAND_RUN, STAND_WALK, STAND_IDLE],
            CROUCH_WALK_BACK => &[CROUCH_WALK_BACK, CROUCH_WALK, CROUCH_IDLE, STAND_WALK_BACK, STAND_WALK, STAND_IDLE],
            CROUCH_WALK_LEFT => &[CROUCH_WALK_LEFT, CROUCH_WALK, CROUCH_IDLE, STAND_WALK_LEFT, STAND_WALK, STAND_IDLE],
            CROUCH_WALK_RIGHT => &[CROUCH_WALK_RIGHT, CROUCH_WALK, CROUCH_IDLE, STAND_WALK_RIGHT, STAND_WALK, STAND_IDLE],
            PRONE_CRAWL_BACK => &[PRONE_CRAWL_BACK, PRONE_CRAWL, PRONE_IDLE, CROUCH_WALK, STAND_WALK, STAND_IDLE],
            PRONE_CRAWL_LEFT => &[PRONE_CRAWL_LEFT, PRONE_CRAWL, PRONE_IDLE, CROUCH_WALK, STAND_WALK, STAND_IDLE],
            PRONE_CRAWL_RIGHT => &[PRONE_CRAWL_RIGHT, PRONE_CRAWL, PRONE_IDLE, CROUCH_WALK, STAND_WALK, STAND_IDLE],
            STAND_ADS_WALK_BACK => &[STAND_ADS_WALK_BACK, STAND_WALK_BACK, STAND_ADS_WALK, STAND_WALK, STAND_ADS, STAND_IDLE],
            STAND_ADS_WALK_LEFT => &[STAND_ADS_WALK_LEFT, STAND_WALK_LEFT, STAND_ADS_WALK, STAND_WALK, STAND_ADS, STAND_IDLE],
            STAND_ADS_WALK_RIGHT => &[STAND_ADS_WALK_RIGHT, STAND_WALK_RIGHT, STAND_ADS_WALK, STAND_WALK, STAND_ADS, STAND_IDLE],
            CROUCH_ADS_WALK_BACK => &[CROUCH_ADS_WALK_BACK, CROUCH_WALK_BACK, CROUCH_ADS_WALK, CROUCH_WALK, CROUCH_ADS, CROUCH_IDLE, STAND_IDLE],
            CROUCH_ADS_WALK_LEFT => &[CROUCH_ADS_WALK_LEFT, CROUCH_WALK_LEFT, CROUCH_ADS_WALK, CROUCH_WALK, CROUCH_ADS, CROUCH_IDLE, STAND_IDLE],
            CROUCH_ADS_WALK_RIGHT => &[CROUCH_ADS_WALK_RIGHT, CROUCH_WALK_RIGHT, CROUCH_ADS_WALK, CROUCH_WALK, CROUCH_ADS, CROUCH_IDLE, STAND_IDLE],
            _ => &[STAND_IDLE],
        }
    }

    /// The clip ids to try, in order, for clip `id`: each nearer state in the weapon's own set,
    /// then the rifleman's (set 0). The converter leaves out a set's clip that is the rifleman's
    /// own, so a set falls through to set 0 state by state before it trades the gait.
    pub fn fallbacks(id: u16) -> impl Iterator<Item = u16> {
        let (set, st) = split(id);
        nearest(st).iter().flat_map(move |&s| [self::id(set, s), self::id(0, s)].into_iter().take(if set == 0 { 1 } else { 2 }))
    }
}

/// The upper-body (torso) clips a third-person General plays over his movement clip: BO2's
/// `mp/playeranim.script` `EVENTS` (`reload`, `knife_melee`, `fireweapon`, ...), each a `pt_*`
/// animation that tracks only the bones from the lower spine up, so it layers over whatever the
/// legs are doing. A torso clip's id in the body manifest is [`BASE`] plus its index among the
/// body's torso clips; which one an action plays is the converter's table (`zhbo_core::torso`).
pub mod torso {
    /// The first torso clip id (movement clip ids stay below it: `state::MAX_SETS * SET_STRIDE`).
    pub const BASE: u16 = 0x1000;

    /// The actions, as the script's events.
    pub const RELOAD: u8 = 0;
    /// `knife_melee`: the knife swing with the weapon in hand (`meleeattack` for the shield's bash).
    pub const MELEE: u8 = 1;
    /// `knife_melee_charge`: the lunge's strike.
    pub const MELEE_CHARGE: u8 = 2;
    /// `fireweapon` with the offhand as the weapon: the throw.
    pub const THROW: u8 = 3;
    /// `prime_grenade`: the pin pulled, the grenade held.
    pub const PRIME: u8 = 4;
    /// `fireweapon`: a shot's recoil.
    pub const FIRE: u8 = 5;
    /// `raiseweapon`, `dropweapon`, `firstraiseweapon`: the weapon change.
    pub const RAISE: u8 = 6;
    pub const DROP: u8 = 7;
    pub const FIRST_RAISE: u8 = 8;
    /// `flinch`: hurt, from the front (the script keys the flinch on `dmgDirection`: the other
    /// sides are [`FLINCH_BACK`], [`FLINCH_LEFT`], [`FLINCH_RIGHT`]; [`flinch`] picks one).
    pub const FLINCH: u8 = 9;
    /// `jump` and `land`: the script's `legs` clips, whole-body one-shots that take over the
    /// movement clip while they run.
    pub const JUMP: u8 = 10;
    pub const LAND: u8 = 11;
    /// `flinch` from behind, the left and the right.
    pub const FLINCH_BACK: u8 = 12;
    pub const FLINCH_LEFT: u8 = 13;
    pub const FLINCH_RIGHT: u8 = 14;
    pub const ACTIONS: u8 = 15;
    /// The script's event for each action.
    pub const EVENTS: [&str; ACTIONS as usize] = [
        "reload", "knife_melee", "knife_melee_charge", "fireweapon", "prime_grenade", "fireweapon", "raiseweapon", "dropweapon", "firstraiseweapon", "flinch", "jump", "land", "flinch", "flinch",
        "flinch",
    ];
    /// The `dmgDirection` an action is asked with ([`DMG_DIRECTIONS`]): each flinch its side;
    /// every other action reads none of it (`front`).
    pub const fn action_dmg_direction(action: u8) -> &'static str {
        match action {
            FLINCH_BACK => "back",
            FLINCH_LEFT => "left",
            FLINCH_RIGHT => "right",
            _ => "front",
        }
    }
    /// The flinch for a hit from `dmg_dir` ([`DMG_DIRECTIONS`] index: 1 front, 2 back, 3 left,
    /// 4 right; none flinches as from the front).
    pub const fn flinch(dmg_dir: u8) -> u8 {
        match dmg_dir {
            2 => FLINCH_BACK,
            3 => FLINCH_LEFT,
            4 => FLINCH_RIGHT,
            _ => FLINCH,
        }
    }
    /// Whether `action` is one of the four flinches.
    pub const fn is_flinch(action: u8) -> bool {
        matches!(action, FLINCH | FLINCH_BACK | FLINCH_LEFT | FLINCH_RIGHT)
    }

    /// The death: the script's `DEATH` section (whole body), played through and held on the corpse.
    /// Not a per-weapon action: its clip comes from [`death_context`].
    pub const DEATH: u8 = 15;

    /// Whether an action's clip moves the whole body (the legs too).
    pub const fn whole_body(action: u8) -> bool {
        matches!(action, JUMP | LAND | DEATH)
    }

    /// The `DEATH` section's conditions, each in BO2's own value order (t6mp's value tables:
    /// `dmgType` at `0x00f6a6f8`, `dmgDirection` at `0x00f6a6c8`, `direction` and `movestatus`
    /// beside them), as the engine sets them before the death (`BG_AnimScriptEvent`,
    /// `FUN_008acce0`, runs one updater per condition). `dmgDirection` is `none` only for a death
    /// no damage preceded (a suicide on a fresh life): `finishplayerdamage` names a side for every
    /// blow, `back` for one with no direction.
    pub const DMG_TYPES: [&str; 10] = ["normal", "normal_shotgun", "explosive", "melee", "meleebash", "headshot", "fire", "suicide", "gas", "falling"];
    pub const DMG_DIRECTIONS: [&str; 5] = ["none", "front", "back", "left", "right"];
    pub const MOVE_STATUSES: [&str; 3] = ["stationary", "walk", "run"];
    pub const DIRECTIONS: [&str; 5] = ["none", "forward", "backward", "left", "right"];
    pub const DEATH_CONTEXTS: u16 = 3 * 3 * 5 * 10 * 5;
    /// A death's context: stance (0 stand, 1 crouch, 2 prone), a [`MOVE_STATUSES`], a
    /// [`DIRECTIONS`], a [`DMG_TYPES`] and a [`DMG_DIRECTIONS`] index.
    pub const fn death_context(stance: u8, move_status: u8, direction: u8, dmg_type: u8, dmg_dir: u8) -> u16 {
        let stance = if stance > 2 { 2 } else { stance } as u16;
        (((stance * 3 + (move_status % 3) as u16) * 5 + (direction % 5) as u16) * 10 + (dmg_type % 10) as u16) * 5 + (dmg_dir % 5) as u16
    }
    /// A death context's parts, as [`death_context`] takes them.
    pub const fn split_death(c: u16) -> (u8, u8, u8, u8, u8) {
        ((c / 750) as u8, ((c / 250) % 3) as u8, ((c / 50) % 5) as u8, ((c / 5) % 10) as u8, (c % 5) as u8)
    }

    /// The contexts an action is asked in, the conditions the `EVENTS` read as the engine sets
    /// them: stance (0 stand, 1 crouch, 2 prone) × `movestatus` ([`MOVE_STATUSES`]: `reload`
    /// keys on stationary, `jump`/`land` on run, the rest on `moving` = walk or run) ×
    /// `direction` ([`DIRECTIONS`]: `jump`/`land`'s backward, left and right) × aimed.
    pub const CONTEXTS: u8 = 3 * 3 * 5 * 2;
    pub const fn context(stance: u8, move_status: u8, direction: u8, ads: bool) -> u8 {
        let stance = if stance > 2 { 2 } else { stance };
        ((stance * 3 + move_status % 3) * 5 + direction % 5) * 2 + ads as u8
    }
    /// A context's `(stance, movestatus, direction, ads)`, as [`context`] takes them.
    pub const fn split(context: u8) -> (u8, u8, u8, bool) {
        (context / 30, (context / 10) % 3, (context / 2) % 5, context & 1 != 0)
    }
}

pub const BODY_MAGIC: [u8; 4] = *b"ZBC\0";
pub const BODY_VERSION: u32 = 1;
pub const WEAPON_MAGIC: [u8; 4] = *b"ZBW\0";
pub const WEAPON_VERSION: u32 = 3;

/// A faction's third-person body: its model blob and one clip per [`state`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body {
    /// The faction letter (`U`, `C`, `G`).
    pub faction: u8,
    /// The body model blob's archive path ([`crate::model_path`]).
    pub model: String,
    /// The body bone a weapon hangs from (`tag_weapon_right`).
    pub weapon_tag: String,
    /// `(clip id, animation blob path)`: [`state::id`] of an animation set and a movement state.
    pub clips: Vec<(u16, String)>,
}

impl Body {
    /// Torso clip `id` ([`torso::BASE`] on), exactly: a torso clip has no fallback.
    pub fn torso_clip(&self, id: u16) -> Option<&str> {
        (id >= torso::BASE).then(|| self.clips.iter().find(|c| c.0 == id).map(|c| c.1.as_str())).flatten()
    }

    /// The clip for clip id `id`: its own, else the nearest one the body has ([`state::fallbacks`]).
    pub fn clip(&self, id: u16) -> Option<&str> {
        state::fallbacks(id).find_map(|s| self.clips.iter().find(|c| c.0 == s).map(|c| c.1.as_str()))
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
            w.u8(p.ads_only as u8);
            // The part's `WeaponCamo` (`FUN_00529600`, G16): the class camo's asset on it.
            w.opt_str(p.camo.as_deref());
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
        for _ in 0..n {
            let model = r.str()?;
            // A base part's tag, when present, names the body bone it hangs from instead of the
            // body's `weapon_tag` (the Assault Shield on `tag_weapon_left`, a stowed weapon on
            // `tag_stowed_back`); the others' tags are bones of the parts before them.
            let tag = r.opt_str()?;
            parts.push(Part { model, tag, offset: (r.f32n()?, r.f32n()?), rigid: r.u8()? != 0, ads_only: r.u8()? != 0, camo: r.opt_str()?, ads_swap: None });
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

    #[test]
    fn every_action_context_splits_back_into_its_parts() {
        use torso::*;
        let mut seen = vec![false; CONTEXTS as usize];
        for stance in 0..3 {
            for ms in 0..3 {
                for dir in 0..5 {
                    for ads in [false, true] {
                        let c = context(stance, ms, dir, ads);
                        assert_eq!(split(c), (stance, ms, dir, ads));
                        assert!(!std::mem::replace(&mut seen[c as usize], true));
                    }
                }
            }
        }
        assert!(seen.iter().all(|&s| s));
        // Each side's flinch is asked with its own side and played for it.
        for (dir, name) in DMG_DIRECTIONS.iter().enumerate().skip(1) {
            let a = flinch(dir as u8);
            assert!(is_flinch(a) && EVENTS[a as usize] == "flinch");
            assert_eq!(action_dmg_direction(a), *name);
        }
        assert_eq!(flinch(0), FLINCH);
        assert!(DEATH >= ACTIONS && !is_flinch(DEATH));
    }

    #[test]
    fn every_death_context_splits_back_into_its_parts() {
        use torso::*;
        let mut seen = vec![false; DEATH_CONTEXTS as usize];
        for stance in 0..3 {
            for ms in 0..MOVE_STATUSES.len() as u8 {
                for dir in 0..DIRECTIONS.len() as u8 {
                    for ty in 0..DMG_TYPES.len() as u8 {
                        for dd in 0..DMG_DIRECTIONS.len() as u8 {
                            let c = death_context(stance, ms, dir, ty, dd);
                            assert_eq!(split_death(c), (stance, ms, dir, ty, dd));
                            assert!(!std::mem::replace(&mut seen[c as usize], true));
                        }
                    }
                }
            }
        }
        assert!(seen.iter().all(|&s| s));
    }

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
        // A weapon's set takes its own clip, else the rifleman's same state before a nearer one.
        let mut pistol = b.clone();
        pistol.clips.push((state::id(2, state::STAND_IDLE), "Art\\Native\\pb_stand_alert_pistol.zba".into()));
        assert_eq!(pistol.clip(state::id(2, state::STAND_IDLE)), Some("Art\\Native\\pb_stand_alert_pistol.zba"));
        assert_eq!(pistol.clip(state::id(2, state::STAND_RUN)), Some("Art\\Native\\pb_combatrun_forward_loop.zba"), "the rifleman's run");
        assert_eq!(pistol.clip(state::id(2, state::STAND_WALK)), Some("Art\\Native\\pb_combatrun_forward_loop.zba"), "the rifleman's run, not the set's idle");
        assert_eq!(pistol.clip(state::id(2, state::STAND_ADS)), Some("Art\\Native\\pb_stand_alert_pistol.zba"), "no ADS anywhere: the set's idle");
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
                Part { model: "Art\\Native\\t6_wpn_ar_an94_world.zbm".into(), tag: None, offset: IDENTITY, rigid: true, ads_only: false, camo: Some("camo_an94".into()), ads_swap: None },
                Part { model: "Art\\Native\\au_an94_reflex_world.zbm".into(), tag: Some("tag_reflex".into()), offset: ([0.0, 0.0, 0.0, 1.0], [2.0, 0.0, 3.0]), rigid: true, ads_only: false, camo: Some("camo_reflex".into()), ads_swap: None },
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
        // A base part may name the body bone it hangs from (the Assault Shield's left hand).
        let mut on_body_tag = w.clone();
        on_body_tag.parts[0].tag = Some("tag_weapon_left".into());
        assert_eq!(Weapon::from_bytes(&on_body_tag.to_bytes()).unwrap(), on_body_tag);
    }
}
