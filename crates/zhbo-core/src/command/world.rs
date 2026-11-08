//! What the resolver sees of the world, and what it sends back (docs/COMMANDS.md sections 2 and 3):
//! the ABI records of the client services `command_objects`, `template_name` and `issue_order`,
//! and the snapshot a command is resolved against.
//!
//! The bridge lists every object the local player's overhead view would let them click: their own
//! live objects, others' visible ones, and the ghosts of buildings seen before and now in fog. It
//! never lists units in fog or shroud.

use std::collections::BTreeMap;

use super::vocab::{fixed_words, Noun, Vocabulary};
use crate::math::Vec3;

/// One object the local player could click overhead (`ZhboCmdObject`, 44 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CmdObject {
    pub id: u32,
    /// Its thing template's id (`ThingTemplate::getTemplateID`), named by `template_name`.
    pub tmpl: u32,
    /// [`cmdflags`] bits.
    pub flags: u32,
    /// [`relation`]: whose it is, from the local player's side.
    pub relation: i32,
    pub pos: Vec3,
    /// Health as a fraction of its maximum (1 at full).
    pub health: f32,
    /// Its bounding sphere's radius (the crosshair's `that` tests it).
    pub radius: f32,
    /// Veterancy: 0 none, 1 veteran, 2 elite, 3 heroic.
    pub veterancy: i32,
    /// The player's control groups it is in: bit N for group N (0-9).
    pub groups: u32,
}

/// [`CmdObject::flags`] (`ZHBO_CMD_*`).
pub mod cmdflags {
    pub const STRUCTURE: u32 = 1 << 0;
    pub const INFANTRY: u32 = 1 << 1;
    pub const VEHICLE: u32 = 1 << 2;
    pub const AIRCRAFT: u32 = 1 << 3;
    pub const DOZER: u32 = 1 << 4;
    pub const HARVESTER: u32 = 1 << 5;
    /// What Zero Hour's select-all key would select (`CommandXlat`'s `MSG_META_SELECT_ALL`:
    /// own, not a dozer, harvester or `IGNORES_SELECT_ALL`, mass-selectable, not contained, alive,
    /// on the map). Never the General.
    pub const SELECT_ALL: u32 = 1 << 6;
    /// The player's own and selectable (`Object::isSelectable`, not contained, alive). Never the
    /// General.
    pub const SELECTABLE: u32 = 1 << 7;
    /// A building seen before and now in fog: its ghost is what the player sees.
    pub const GHOST: u32 = 1 << 8;
    /// No orders (`AIUpdateInterface::isIdle`).
    pub const IDLE: u32 = 1 << 9;
    /// It has passengers or a garrison.
    pub const LOADED: u32 = 1 << 10;
    /// A General (`ZHBO_General_*`).
    pub const GENERAL: u32 = 1 << 11;
}

/// [`CmdObject::relation`] (`ZHBO_REL_*`).
pub mod relation {
    pub const OWN: i32 = 0;
    pub const ALLY: i32 = 1;
    pub const ENEMY: i32 = 2;
    pub const NEUTRAL: i32 = 3;
    /// The neutral side's civilian player (its template's side is `Civilian`).
    pub const CIVILIAN: i32 = 4;
}

/// One order for the bridge to send as the overhead view would (`ZhboCmdOrder`, 28 bytes): the
/// selection as `MSG_CREATE_SELECTED_GROUP`, then the order's message.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CmdOrder {
    /// [`orderkind`].
    pub kind: i32,
    /// The target object, for the kinds that take one.
    pub target: u32,
    /// The target point, for the kinds that take one.
    pub pos: Vec3,
    /// 1: queue behind the current order (`MSG_ADD_WAYPOINT` for a move).
    pub queued: i32,
    /// The control group for [`orderkind::MAKE_GROUP`].
    pub arg: i32,
}

/// [`CmdOrder::kind`] (`ZHBO_ORDER_*`), each the message the bridge sends.
pub mod orderkind {
    /// `MSG_DO_MOVETO` at `pos` (queued: `MSG_ADD_WAYPOINT`).
    pub const MOVE: i32 = 1;
    /// `MSG_DO_FORCEMOVETO` at `pos`.
    pub const FORCE_MOVE: i32 = 2;
    /// `MSG_DO_ATTACKMOVETO` at `pos`.
    pub const ATTACK_MOVE: i32 = 3;
    /// `MSG_DO_STOP`.
    pub const STOP: i32 = 4;
    /// `MSG_DO_ATTACK_OBJECT` on `target`.
    pub const ATTACK: i32 = 5;
    /// `MSG_DO_FORCE_ATTACK_OBJECT` on `target`.
    pub const FORCE_ATTACK: i32 = 6;
    /// `MSG_DO_FORCE_ATTACK_GROUND` at `pos`.
    pub const FORCE_ATTACK_GROUND: i32 = 7;
    /// `MSG_DO_GUARD_POSITION` at `pos` (`GUARDMODE_NORMAL`).
    pub const GUARD_POSITION: i32 = 8;
    /// `MSG_DO_GUARD_OBJECT` on `target` (`GUARDMODE_NORMAL`).
    pub const GUARD_OBJECT: i32 = 9;
    /// `MSG_CREATE_TEAM<arg>`: the selection becomes control group `arg`.
    pub const MAKE_GROUP: i32 = 10;
}

/// The class words (docs/COMMANDS.md section 5): grammar words, each tied to an object flag. A
/// unit type's own display name wins over a class word it equals.
pub const CLASSES: [(&str, &str, u32); 6] = [
    ("infantry", "infantry", cmdflags::INFANTRY),
    ("vehicle", "vehicles", cmdflags::VEHICLE),
    ("aircraft", "aircraft", cmdflags::AIRCRAFT),
    ("building", "buildings", cmdflags::STRUCTURE),
    ("builder", "builders", cmdflags::DOZER),
    ("harvester", "harvesters", cmdflags::HARVESTER),
];

/// The world a command is resolved against: the clickable objects, their names, the General and
/// where the crosshair points.
#[derive(Clone, Debug, Default)]
pub struct World {
    pub objects: Vec<CmdObject>,
    /// Each named type: its noun and the templates with that display name (faction variants share
    /// one name).
    pub types: Vec<(Noun, Vec<u32>)>,
    /// The General: his object id, his feet and his facing (radians).
    pub general: u32,
    pub general_pos: Vec3,
    pub general_yaw: f32,
    /// The crosshair: the eye and the view's direction.
    pub eye: Vec3,
    pub dir: Vec3,
    /// Names the vocabulary refused, with why (logged once per command).
    pub refused_names: Vec<String>,
}

impl World {
    /// The snapshot from the objects and a template namer. Each name is the display name lowercased,
    /// with the `&` hotkey markers taken out; a template with no usable name is not nameable.
    pub fn new(objects: Vec<CmdObject>, name_of: &dyn Fn(u32) -> Option<String>) -> World {
        let mut by_name: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut refused = Vec::new();
        let fixed = fixed_words();
        let verbs: Vec<String> = super::vocab::fixed_verbs().into_iter().map(|v| v.words).collect();
        let mut seen = std::collections::BTreeSet::new();
        for o in &objects {
            if !seen.insert(o.tmpl) {
                continue;
            }
            let Some(raw) = name_of(o.tmpl) else { continue };
            let Some(name) = canonical_name(&raw) else {
                refused.push(format!("'{raw}': not plain words"));
                continue;
            };
            if fixed.contains(&name.as_str()) || verbs.contains(&name) || name.split(' ').any(|w| w == "then") {
                refused.push(format!("'{name}': a word of the grammar"));
                continue;
            }
            by_name.entry(name).or_default().push(o.tmpl);
        }
        let types = by_name.into_iter().map(|(name, tmpls)| (Noun::new(&name, &plural(&name)), tmpls)).collect();
        World { objects, types, refused_names: refused, ..World::default() }
    }

    /// The vocabulary of this world: its type names, the class words no type name took, and the
    /// team names given.
    pub fn vocabulary(&self, teams: &[String]) -> Vocabulary {
        let types: Vec<Noun> = self.types.iter().map(|(n, _)| n.clone()).collect();
        let taken = |w: &str| types.iter().any(|n| n.singular == w || n.plural == w);
        let classes = CLASSES.iter().filter(|(s, p, _)| !taken(s) && !taken(p)).map(|(s, p, _)| Noun::new(s, p)).collect();
        Vocabulary { types, classes, teams: teams.to_vec(), ..Vocabulary::default() }
    }

    /// The templates named `singular`.
    pub fn templates(&self, singular: &str) -> &[u32] {
        self.types.iter().find(|(n, _)| n.singular == singular).map_or(&[], |(_, t)| t.as_slice())
    }

    pub fn object(&self, id: u32) -> Option<&CmdObject> {
        self.objects.iter().find(|o| o.id == id)
    }
}

/// A display name as canonical words: lowercase, `&` markers out, whitespace collapsed; `None` when
/// a word is not letters, digits, `-` or `'` after that (a missing string, punctuation).
pub fn canonical_name(raw: &str) -> Option<String> {
    let lower = raw.replace('&', "").to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    if words.is_empty() || !words.iter().all(|w| super::vocab::is_word(w)) {
        return None;
    }
    Some(words.join(" "))
}

/// The plural of a name by English's regular rules, on its last word: `-y` after a consonant to
/// `-ies`, `-s`, `-x`, `-z`, `-ch`, `-sh` add `-es`, the rest add `-s`.
pub fn plural(name: &str) -> String {
    let (head, last) = match name.rsplit_once(' ') {
        Some((h, l)) => (format!("{h} "), l),
        None => (String::new(), name),
    };
    let consonant_y = last.len() > 1 && last.ends_with('y') && !matches!(last.as_bytes()[last.len() - 2], b'a' | b'e' | b'i' | b'o' | b'u');
    let p = if consonant_y {
        format!("{}ies", &last[..last.len() - 1])
    } else if ["s", "x", "z", "ch", "sh"].iter().any(|e| last.ends_with(e)) {
        format!("{last}es")
    } else {
        format!("{last}s")
    };
    head + &p
}
