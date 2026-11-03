//! A third-person General's upper body: what BO2 plays over his movement clip as he reloads,
//! swings the knife, throws or primes a grenade, fires and changes weapon
//! (`mp/playeranim.script`'s `EVENTS`: `reload`, `knife_melee`, `fireweapon`, ...), and the sounds
//! the clip's notetracks carry — another player's reload is heard through his torso clip's
//! `sndnt#fly_assault_reload_npc_mag_out`, as BO2 plays it (no MP weapon names a `reloadSound`).
//!
//! The converter (`zhbo_assets::thirdperson::torso_table`) asks the script for every weapon's
//! clip per action ([`zhbo_native::thirdperson::torso`]) and context (stance, moving, aimed),
//! groups the weapons into torso sets (`WeaponDef::torso_anims`) and writes the table into
//! [`crate::names::SOUND_LAYERS_PATH`] next to the shot layers, with each clip's length and its
//! sound notetracks (positional events, `names::npc_notetrack_event`):
//!
//! * `torso <set> <action> <context> <clip id>`
//! * `torso_clip <clip id> <seconds> <name>`
//! * `torso_note <clip id> <seconds> <event>`
//!
//! The clips themselves are in the faction body manifest under the same ids (the native body
//! layers them, `nativebody`). Presentation only: the client picks and times the action from the
//! sim's events and records, never the other way round.

use std::collections::HashMap;
use std::sync::RwLock;

pub use zhbo_native::thirdperson::torso::{context, whole_body, ACTIONS, CONTEXTS, DROP, FIRE, FIRST_RAISE, FLINCH, JUMP, LAND, MELEE, MELEE_CHARGE, PRIME, RAISE, RELOAD, THROW};

/// A torso clip: its length and its sound notetracks (seconds in, event).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub length: f32,
    pub name: String,
    pub notes: Vec<(f32, &'static str)>,
}

/// The installed table.
#[derive(Clone, Debug, Default)]
pub struct Table {
    picks: HashMap<(u16, u8, u8), u16>,
    clips: HashMap<u16, Clip>,
}

impl Table {
    /// The clip torso set `set` plays for `action` in `context`.
    pub fn pick(&self, set: u16, action: u8, context: u8) -> Option<u16> {
        self.picks.get(&(set, action, context)).copied()
    }

    pub fn clip(&self, id: u16) -> Option<&Clip> {
        self.clips.get(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.picks.is_empty()
    }
}

/// Parse the torso rows (other rows and malformed ones are skipped).
pub fn parse(text: &str) -> Table {
    let mut t = Table::default();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("torso") => {
                let row = (|| Some(((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?), it.next()?.parse().ok()?)))();
                if let Some((k, clip)) = row {
                    t.picks.insert(k, clip);
                }
            }
            Some("torso_clip") => {
                let row = (|| Some((it.next()?.parse::<u16>().ok()?, it.next()?.parse::<f32>().ok()?, it.next()?.to_string())))();
                if let Some((id, length, name)) = row {
                    let c = t.clips.entry(id).or_default();
                    c.length = length;
                    c.name = name;
                }
            }
            Some("torso_note") => {
                let row = (|| Some((it.next()?.parse::<u16>().ok()?, it.next()?.parse::<f32>().ok()?, it.next()?.to_string())))();
                if let Some((id, at, event)) = row {
                    t.clips.entry(id).or_default().notes.push((at, Box::leak(event.into_boxed_str())));
                }
            }
            _ => {}
        }
    }
    t
}

fn table() -> &'static RwLock<&'static Table> {
    static TABLE: std::sync::OnceLock<RwLock<&'static Table>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(Box::leak(Box::default())))
}

/// Install the converted rows (replacing any earlier ones). Returns the number of picks.
pub fn install(text: &str) -> usize {
    let t = parse(text);
    let n = t.picks.len();
    *table().write().unwrap_or_else(|e| e.into_inner()) = Box::leak(Box::new(t));
    n
}

/// The installed table.
pub fn get() -> &'static Table {
    *table().read().unwrap_or_else(|e| e.into_inner())
}

/// The clip `def` plays for `action` with the General in `context` (its torso set's), if any.
pub fn pick_for(def: &crate::weapons::WeaponDef, action: u8, context: u8) -> Option<u16> {
    get().pick(def.torso_anims, action, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_parse_into_picks_clips_and_notes() {
        let t = parse(
            "; comment\nlayer x y z 1 2\ntorso 3 0 0 4096\ntorso 3 0 4 4097\ntorso_clip 4096 2.3 pt_reload_stand_rifle\n\
             torso_note 4096 0.0322 ZHBO_NTN_fly_assault_reload_npc_mag_out\ntorso_note 4096 0.966 ZHBO_NTN_fly_assault_reload_npc_mag_in\ntorso bad\n",
        );
        assert_eq!(t.pick(3, RELOAD, 0), Some(4096));
        assert_eq!(t.pick(3, RELOAD, context(1, false, false)), Some(4097));
        assert_eq!(t.pick(3, MELEE, 0), None);
        let c = t.clip(4096).unwrap();
        assert_eq!((c.length, c.name.as_str()), (2.3, "pt_reload_stand_rifle"));
        assert_eq!(c.notes, [(0.0322, "ZHBO_NTN_fly_assault_reload_npc_mag_out"), (0.966, "ZHBO_NTN_fly_assault_reload_npc_mag_in")]);
    }
}
