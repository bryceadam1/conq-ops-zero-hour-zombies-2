//! What the resolver sees of the world, and what it sends back (docs/COMMANDS.md sections 2, 3 and
//! 7): the ABI records of the client services `command_objects`, `command_buttons`,
//! `template_name` and `issue_order`, and the snapshot a command is resolved against.
//!
//! The bridge lists every object the local player's overhead view would let them click: their own
//! live objects, others' visible ones, and the ghosts of buildings seen before and now in fog. It
//! never lists units in fog or shroud. It also lists the player's own passengers (not clickable,
//! but `unload` names them) and every button of the player's command sets.

use std::collections::BTreeMap;

use super::vocab::{fixed_verbs, fixed_words, Noun, Vocabulary};
use crate::math::Vec3;

/// One object the local player could click overhead (`ZhboCmdObject`, 52 bytes).
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
    /// How many entries its production queue holds (a factory's units and upgrades).
    pub queue: i32,
    /// The object it rides in ([`cmdflags::CONTAINED`]), else 0.
    pub container: u32,
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
    /// One of the player's own passengers, inside [`super::CmdObject::container`]: never selectable,
    /// only `unload` names it.
    pub const CONTAINED: u32 = 1 << 12;
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

/// One button of the player's command sets (`ZhboCmdButton`, 124 bytes): an object's current
/// command set (what the control bar shows when it is selected), or the player's own sets (the
/// Generals' promotions, the special-power shortcuts).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CmdButton {
    /// The object whose command set holds it; 0 for the player's own sets.
    pub owner: u32,
    /// [`playerset`] for `owner` 0, else 0.
    pub set: i32,
    /// Its place in the set (`CommandSet::getCommandButton`).
    pub slot: i32,
    /// [`button`]: the bridge's stable code for the button's `GUICommandType`.
    pub code: i32,
    /// The button's `CommandOption` bits ([`option`]).
    pub options: u32,
    /// The thing template it builds or names, else 0.
    pub tmpl: u32,
    /// [`availability`]: `ControlBar::getCommandAvailability` now.
    pub availability: i32,
    /// What it is about, as display text (UTF-8, NUL-terminated): the upgrade's or science's name,
    /// the button's label for a power or a weapon mode; empty for a build (the template names it).
    pub name: [u8; 48],
    /// The button's label (UTF-8, NUL-terminated), for the log.
    pub label: [u8; 48],
}

impl Default for CmdButton {
    fn default() -> Self {
        CmdButton { owner: 0, set: 0, slot: 0, code: 0, options: 0, tmpl: 0, availability: 0, name: [0; 48], label: [0; 48] }
    }
}

impl CmdButton {
    pub fn name_text(&self) -> String {
        c_text(&self.name)
    }

    pub fn label_text(&self) -> String {
        c_text(&self.label)
    }

    /// The target the button asks for: (an object, a point).
    pub fn needs(&self) -> (bool, bool) {
        let object = self.options & (option::NEED_TARGET_ENEMY_OBJECT | option::NEED_TARGET_NEUTRAL_OBJECT | option::NEED_TARGET_ALLY_OBJECT) != 0
            || self.options & option::CONTEXTMODE_COMMAND != 0;
        (object, self.options & option::NEED_TARGET_POS != 0)
    }
}

fn c_text(b: &[u8]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..n]).into_owned()
}

/// One of the map's named areas (`ZhboCmdPlace`, 60 bytes): a polygon trigger's name (UTF-8,
/// NUL-terminated, the map's identifier as written) and its centre.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CmdPlace {
    pub name: [u8; 48],
    pub pos: Vec3,
}

impl Default for CmdPlace {
    fn default() -> Self {
        CmdPlace { name: [0; 48], pos: Vec3::ZERO }
    }
}

impl CmdPlace {
    pub fn name_text(&self) -> String {
        c_text(&self.name)
    }
}

/// [`CmdButton::set`] for the player's own sets (`ZHBO_PLAYERSET_*`).
pub mod playerset {
    /// The player template's special-power shortcuts (`SpecialPowerShortcutCommandSet`).
    pub const SHORTCUTS: i32 = 1;
    /// The Generals' promotions (`PurchaseScienceCommandSetRank1/3/8`).
    pub const RANK1: i32 = 2;
    pub const RANK3: i32 = 3;
    pub const RANK8: i32 = 4;
}

/// [`CmdButton::code`] (`ZHBO_BUTTON_*`): the bridge maps each `GUICommandType` to one of these, so
/// the numbering does not depend on the engine's build flags (`ALLOW_SURRENDER` shifts its enum).
pub mod button {
    pub const NONE: i32 = 0;
    pub const DOZER_CONSTRUCT: i32 = 1;
    pub const DOZER_CONSTRUCT_CANCEL: i32 = 2;
    pub const UNIT_BUILD: i32 = 3;
    pub const CANCEL_UNIT_BUILD: i32 = 4;
    pub const PLAYER_UPGRADE: i32 = 5;
    pub const OBJECT_UPGRADE: i32 = 6;
    pub const CANCEL_UPGRADE: i32 = 7;
    pub const ATTACK_MOVE: i32 = 8;
    pub const GUARD: i32 = 9;
    pub const GUARD_WITHOUT_PURSUIT: i32 = 10;
    pub const GUARD_FLYING_UNITS_ONLY: i32 = 11;
    pub const STOP: i32 = 12;
    pub const WAYPOINTS: i32 = 13;
    pub const EXIT_CONTAINER: i32 = 14;
    pub const EVACUATE: i32 = 15;
    pub const EXECUTE_RAILED_TRANSPORT: i32 = 16;
    pub const BEACON_DELETE: i32 = 17;
    pub const SET_RALLY_POINT: i32 = 18;
    pub const SELL: i32 = 19;
    pub const FIRE_WEAPON: i32 = 20;
    pub const SPECIAL_POWER: i32 = 21;
    pub const PURCHASE_SCIENCE: i32 = 22;
    pub const HACK_INTERNET: i32 = 23;
    pub const TOGGLE_OVERCHARGE: i32 = 24;
    pub const POW_RETURN_TO_PRISON: i32 = 25;
    pub const COMBATDROP: i32 = 26;
    pub const SWITCH_WEAPON: i32 = 27;
    pub const HIJACK_VEHICLE: i32 = 28;
    pub const CONVERT_TO_CARBOMB: i32 = 29;
    pub const SABOTAGE_BUILDING: i32 = 30;
    pub const PICK_UP_PRISONER: i32 = 31;
    pub const PLACE_BEACON: i32 = 32;
    pub const SPECIAL_POWER_FROM_SHORTCUT: i32 = 33;
    pub const SPECIAL_POWER_CONSTRUCT: i32 = 34;
    pub const SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT: i32 = 35;
    pub const SELECT_ALL_UNITS_OF_TYPE: i32 = 36;
}

/// `CommandOption` bits (`ControlBar.h`, explicit values there).
pub mod option {
    pub const NEED_TARGET_ENEMY_OBJECT: u32 = 0x0000_0001;
    pub const NEED_TARGET_NEUTRAL_OBJECT: u32 = 0x0000_0002;
    pub const NEED_TARGET_ALLY_OBJECT: u32 = 0x0000_0004;
    pub const NEED_TARGET_POS: u32 = 0x0000_0020;
    pub const CONTEXTMODE_COMMAND: u32 = 0x0000_0200;
    pub const SCRIPT_ONLY: u32 = 0x0008_0000;
}

/// [`CmdButton::availability`]: `CommandAvailability` (`ControlBar.h`).
pub mod availability {
    pub const RESTRICTED: i32 = 0;
    pub const AVAILABLE: i32 = 1;
    pub const ACTIVE: i32 = 2;
    pub const HIDDEN: i32 = 3;
    pub const NOT_READY: i32 = 4;
    pub const CANT_AFFORD: i32 = 5;
}

/// One order for the bridge to send as the overhead view would (`ZhboCmdOrder`, 56 bytes): the
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
    /// The kind's number: the control group ([`orderkind::MAKE_GROUP`]), the guard mode
    /// ([`orderkind::GUARD_POSITION`], [`orderkind::GUARD_OBJECT`]), the allowed outcomes
    /// ([`orderkind::CONTEXT`], [`context`] bits).
    pub arg: i32,
    /// The thing template ([`orderkind::CONSTRUCT`], [`orderkind::CANCEL`] of a unit).
    pub tmpl: u32,
    /// The button's slot ([`orderkind::BUTTON`]; [`orderkind::CANCEL`] of an upgrade: the slot of
    /// the owner's button for it), else -1.
    pub slot: i32,
    /// The building's angle, radians ([`orderkind::CONSTRUCT`]).
    pub angle: f32,
    /// [`CmdButton::set`] of the button ([`orderkind::BUTTON`]).
    pub set: i32,
    /// A line's far end ([`orderkind::CONSTRUCT`] with `arg` 1: a wall from `pos` to `end`).
    pub end: Vec3,
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
    /// `MSG_DO_GUARD_POSITION` at `pos`, guard mode `arg`.
    pub const GUARD_POSITION: i32 = 8;
    /// `MSG_DO_GUARD_OBJECT` on `target`, guard mode `arg`.
    pub const GUARD_OBJECT: i32 = 9;
    /// `MSG_CREATE_TEAM<arg>`: the selection becomes control group `arg`.
    pub const MAKE_GROUP: i32 = 10;
    /// Press button `slot` of the selected object's command set (or the player's set `set`, the
    /// ids then naming nobody), as the control bar does (`ControlBar::processCommandUI`); a button
    /// that needs a target is finished at `target`/`pos` as the world click would
    /// (`CommandTranslator::evaluateContextCommand` with the button pending).
    pub const BUTTON: i32 = 11;
    /// `MSG_DOZER_CONSTRUCT` of `tmpl` at the nearest legal place to `pos`, at `angle`; for a
    /// line structure (`LINEBUILD`), `MSG_DOZER_CONSTRUCT_LINE` from `pos` to `end` (`arg` 1) or to
    /// `pos`. A special power's construction (`SPECIAL_POWER_CONSTRUCT` buttons) goes through
    /// [`BUTTON`] instead.
    pub const CONSTRUCT: i32 = 12;
    /// Cancel the last queued `tmpl` (`MSG_CANCEL_UNIT_CREATE`), or the upgrade of button `slot`
    /// (`MSG_CANCEL_UPGRADE`), in the selected producer.
    pub const CANCEL: i32 = 13;
    /// `MSG_EXIT` of passenger `target` from the selected container.
    pub const UNLOAD: i32 = 14;
    /// What a right-click on `target` (or at `pos` when `target` is 0) does
    /// (`CommandTranslator::evaluateContextCommand`), only if its outcome is one of `arg`'s
    /// [`context`] bits.
    pub const CONTEXT: i32 = 15;
    /// `MSG_DO_SCATTER`.
    pub const SCATTER: i32 = 16;
    /// `MSG_CREATE_FORMATION`.
    pub const FORM_UP: i32 = 17;
    /// `MSG_DO_CHEER`.
    pub const CHEER: i32 = 18;
    /// `MSG_PLACE_BEACON` at `pos`.
    pub const PLACE_BEACON: i32 = 19;
    /// `MSG_REMOVE_BEACON` (the selected beacon).
    pub const REMOVE_BEACON: i32 = 20;
}

/// What a right-click does, as the bridge classifies `evaluateContextCommand`'s message
/// (`ZHBO_CTX_*` bits, [`orderkind::CONTEXT`]).
pub mod context {
    pub const MOVE: i32 = 1 << 0;
    pub const ATTACK: i32 = 1 << 1;
    pub const ENTER: i32 = 1 << 2;
    pub const CAPTURE: i32 = 1 << 3;
    pub const REPAIR: i32 = 1 << 4;
    pub const RESUME_CONSTRUCTION: i32 = 1 << 5;
    pub const GET_REPAIRED: i32 = 1 << 6;
    pub const GET_HEALED: i32 = 1 << 7;
    pub const DOCK: i32 = 1 << 8;
    pub const CARBOMB: i32 = 1 << 9;
    pub const HACK: i32 = 1 << 10;
    pub const SALVAGE: i32 = 1 << 11;
    pub const SPECIAL_POWER: i32 = 1 << 12;
    pub const RALLY_POINT: i32 = 1 << 13;
    pub const OTHER: i32 = 1 << 14;
    pub const ANY: i32 = (1 << 15) - 1;
}

/// [`issue_order`](crate::host::ClientHost::issue_order)'s answer (`ZHBO_ISSUED_*`).
pub mod issued {
    pub const SENT: i32 = 1;
    /// None of the ids is the player's.
    pub const NOBODY: i32 = 0;
    pub const UNAVAILABLE: i32 = -1;
    pub const CANT_AFFORD: i32 = -2;
    pub const NOT_READY: i32 = -3;
    pub const QUEUE_FULL: i32 = -4;
    pub const NO_PLACE: i32 = -5;
    /// The target does not suit it (the engine's GUI command or right-click refused it).
    pub const BAD_TARGET: i32 = -6;
    /// The right-click there does something else.
    pub const OTHER_ACTION: i32 = -7;
    /// Nothing queued of that kind to cancel, or no such passenger.
    pub const NOT_FOUND: i32 = -8;

    /// Why an order did not go out, for the echo.
    pub fn reason(code: i32) -> &'static str {
        match code {
            NOBODY => "none of them is yours",
            UNAVAILABLE => "it isn't available",
            CANT_AFFORD => "you can't afford it",
            NOT_READY => "it isn't ready",
            QUEUE_FULL => "the queue is full",
            NO_PLACE => "there's no room to build it there",
            BAD_TARGET => "it can't be used on that",
            OTHER_ACTION => "that would do something else",
            NOT_FOUND => "there's nothing like that to cancel or unload",
            _ => "the engine refused it",
        }
    }
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

/// The world a command is resolved against: the clickable objects, the buttons, their names, the
/// General and where the crosshair points.
#[derive(Clone, Debug, Default)]
pub struct World {
    pub objects: Vec<CmdObject>,
    pub buttons: Vec<CmdButton>,
    /// Each named type: its noun and the templates with that display name (faction variants share
    /// one name). Types on the map and types the buttons build.
    pub types: Vec<(Noun, Vec<u32>)>,
    /// The other names the buttons give (upgrades, powers, sciences, weapon modes, weapons), each
    /// with the buttons' indices in [`World::buttons`].
    pub names: BTreeMap<String, Vec<usize>>,
    /// The General: his object id, his feet and his facing (radians).
    pub general: u32,
    pub general_pos: Vec3,
    pub general_yaw: f32,
    /// The crosshair: the eye and the view's direction.
    pub eye: Vec3,
    pub dir: Vec3,
    /// The map's named areas (their canonical names) and their centres ([`World::set_places`]).
    pub places: Vec<(String, Vec3)>,
    /// The map's centre and half its smaller side (the groups' compass regions).
    pub map_centre: Vec3,
    pub map_radius: f32,
    /// Names the vocabulary refused, with why (logged with each command).
    pub refused_names: Vec<String>,
}

/// Which list a button's name joins, by its code.
fn name_list(code: i32) -> Option<super::vocab::ThingList> {
    use super::vocab::ThingList as L;
    match code {
        button::PLAYER_UPGRADE | button::OBJECT_UPGRADE => Some(L::Upgrades),
        button::SPECIAL_POWER | button::SPECIAL_POWER_FROM_SHORTCUT => Some(L::Powers),
        button::PURCHASE_SCIENCE => Some(L::Sciences),
        button::SWITCH_WEAPON => Some(L::Modes),
        button::FIRE_WEAPON => Some(L::Weapons),
        _ => None,
    }
}

impl World {
    /// The snapshot from the objects, the buttons and a template namer. Each name is canonical
    /// words ([`canonical_name`]); a name equal to a grammar word or verb is refused (and logged).
    pub fn new(objects: Vec<CmdObject>, buttons: Vec<CmdButton>, name_of: &dyn Fn(u32) -> Option<String>) -> World {
        let mut refused = Vec::new();
        let fixed = fixed_words();
        let verbs: Vec<String> = fixed_verbs().into_iter().map(|v| v.words).collect();
        let accept = |raw: &str, refused: &mut Vec<String>| -> Option<String> {
            let Some(name) = canonical_name(raw) else {
                refused.push(format!("'{raw}': not plain words"));
                return None;
            };
            if fixed.contains(&name.as_str()) || verbs.contains(&name) || name.split(' ').any(|w| w == "then") {
                refused.push(format!("'{name}': a word of the grammar"));
                return None;
            }
            Some(name)
        };
        let mut by_name: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut seen = std::collections::BTreeSet::new();
        let tmpls = objects.iter().map(|o| o.tmpl).chain(buttons.iter().map(|b| b.tmpl)).filter(|&t| t != 0);
        for tmpl in tmpls {
            if !seen.insert(tmpl) {
                continue;
            }
            let Some(raw) = name_of(tmpl) else { continue };
            if let Some(name) = accept(&raw, &mut refused) {
                by_name.entry(name).or_default().push(tmpl);
            }
        }
        let types: Vec<(Noun, Vec<u32>)> = by_name.into_iter().map(|(name, tmpls)| (Noun::new(&name, &plural(&name)), tmpls)).collect();
        let mut names: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, b) in buttons.iter().enumerate() {
            if name_list(b.code).is_none() || b.options & option::SCRIPT_ONLY != 0 {
                continue;
            }
            let raw = b.name_text();
            if raw.trim().is_empty() {
                continue;
            }
            if let Some(name) = accept(&raw, &mut refused) {
                // An upgrade may not share a unit's name: `cancel` reads either. The other names
                // are only read after their own verbs (`use scud storm` beside the Scud Storm).
                let upgrade = matches!(b.code, button::PLAYER_UPGRADE | button::OBJECT_UPGRADE);
                if upgrade && types.iter().any(|(n, _)| n.singular == name || n.plural == name) {
                    refused.push(format!("'{name}': an upgrade with a unit's name"));
                    continue;
                }
                names.entry(name).or_default().push(i);
            }
        }
        refused.sort();
        refused.dedup();
        World { objects, buttons, types, names, refused_names: refused, ..World::default() }
    }

    /// The map's named areas from their raw names (`MapAreaName`, `North_Pass`): split into words at
    /// underscores and case changes, made canonical; a name that is a grammar word, a unit's or
    /// another area's is refused (and logged).
    pub fn set_places(&mut self, raw: Vec<(String, Vec3)>) {
        let fixed = fixed_words();
        let verbs: Vec<String> = fixed_verbs().into_iter().map(|v| v.words).collect();
        for (name, at) in raw {
            let Some(words) = canonical_name(&split_words(&name)) else { continue };
            let clash = fixed.contains(&words.as_str())
                || verbs.contains(&words)
                || words.split(' ').any(|w| w == "then")
                || self.types.iter().any(|(n, _)| n.singular == words || n.plural == words)
                || CLASSES.iter().any(|(s, p, _)| *s == words || *p == words);
            if clash {
                self.refused_names.push(format!("place '{words}': a word of the grammar or a unit's name"));
            } else if self.places.iter().any(|(n, _)| *n == words) {
                self.refused_names.push(format!("place '{words}': two areas share the name"));
            } else {
                self.places.push((words, at));
            }
        }
        self.places.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// The vocabulary of this world: its type names, the class words no type name took, the names
    /// the buttons give, the map's places, and the team names given.
    pub fn vocabulary(&self, teams: &[String]) -> Vocabulary {
        use super::vocab::ThingList as L;
        let types: Vec<Noun> = self.types.iter().map(|(n, _)| n.clone()).collect();
        let taken = |w: &str| types.iter().any(|n| n.singular == w || n.plural == w);
        let classes = CLASSES.iter().filter(|(s, p, _)| !taken(s) && !taken(p)).map(|(s, p, _)| Noun::new(s, p)).collect();
        let list = |want: L| -> Vec<String> {
            self.names.iter().filter(|(_, idx)| idx.iter().any(|&i| name_list(self.buttons[i].code) == Some(want))).map(|(n, _)| n.clone()).collect()
        };
        Vocabulary {
            types,
            classes,
            upgrades: list(L::Upgrades),
            powers: list(L::Powers),
            sciences: list(L::Sciences),
            modes: list(L::Modes),
            weapons: list(L::Weapons),
            places: self.places.iter().map(|(n, _)| n.clone()).collect(),
            teams: teams.to_vec(),
            ..Vocabulary::default()
        }
    }

    /// The templates named `singular`.
    pub fn templates(&self, singular: &str) -> &[u32] {
        self.types.iter().find(|(n, _)| n.singular == singular).map_or(&[], |(_, t)| t.as_slice())
    }

    pub fn object(&self, id: u32) -> Option<&CmdObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// The buttons a name stands for.
    pub fn named_buttons(&self, name: &str) -> impl Iterator<Item = &CmdButton> {
        self.names.get(name).into_iter().flatten().map(|&i| &self.buttons[i])
    }
}

/// A display name as canonical words: lowercase, `&` hotkey markers out, other punctuation as
/// spaces, whitespace collapsed. `None` for a missing string (the engine's `MISSING: '…'`) or one
/// with no word left.
pub fn canonical_name(raw: &str) -> Option<String> {
    if raw.trim_start().starts_with("MISSING") {
        return None;
    }
    let cleaned: String = raw
        .replace('&', "")
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '\'' { c } else { ' ' })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().map(|w| w.trim_matches(|c| c == '-' || c == '\'')).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return None;
    }
    Some(words.join(" "))
}

/// A map's identifier as words: underscores and case changes split it (`NorthPass_2` → `North Pass
/// 2`).
pub fn split_words(raw: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in raw.chars() {
        let boundary = match prev {
            Some(p) => (p.is_ascii_lowercase() && c.is_ascii_uppercase()) || (p.is_ascii_alphabetic() && c.is_ascii_digit()) || (p.is_ascii_digit() && c.is_ascii_alphabetic()),
            None => false,
        };
        if c == '_' {
            out.push(' ');
        } else {
            if boundary {
                out.push(' ');
            }
            out.push(c);
        }
        prev = Some(c);
    }
    out
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
