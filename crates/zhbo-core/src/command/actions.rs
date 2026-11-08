//! How the language covers everything the overhead view can do (docs/COMMANDS.md constraint 1 and
//! section 12): every command-button type (`GUICommandType`, `ControlBar.h`) and every message a
//! player's machine sends the logic (`MessageStream.h`, `MSG_BEGIN_NETWORK_MESSAGES` up to the
//! debug ones), each with the verb or grammar that produces it, or why it is not a player's order.
//!
//! The resolver reads [`button_codes`] from here, so the table is the one source of which button a
//! verb presses. The tests hold both tables to the engine's own headers (when the engine submodule
//! is checked out) and to the verb table.

use super::world::button as b;

/// How the language produces one engine command or message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Covered {
    /// The verb that does it.
    Verb(&'static str),
    /// The grammar does it without a verb of its own (how).
    Grammar(&'static str),
    /// Not a player's order (why): user interface state, a game option, the quit menu.
    NotAnOrder(&'static str),
    /// Never reaches a Zero Hour player (why): compiled out, or sent by nothing.
    Absent(&'static str),
}

use Covered::*;

/// Every `GUICommandType` by its engine name, with the bridge's code for it.
pub const BUTTONS: &[(&str, i32, Covered)] = &[
    ("GUI_COMMAND_NONE", b::NONE, Absent("the invalid command")),
    ("GUI_COMMAND_DOZER_CONSTRUCT", b::DOZER_CONSTRUCT, Verb("build")),
    ("GUI_COMMAND_DOZER_CONSTRUCT_CANCEL", b::DOZER_CONSTRUCT_CANCEL, Verb("cancel construction")),
    ("GUI_COMMAND_UNIT_BUILD", b::UNIT_BUILD, Verb("build")),
    ("GUI_COMMAND_CANCEL_UNIT_BUILD", b::CANCEL_UNIT_BUILD, Verb("cancel")),
    ("GUI_COMMAND_PLAYER_UPGRADE", b::PLAYER_UPGRADE, Verb("upgrade")),
    ("GUI_COMMAND_OBJECT_UPGRADE", b::OBJECT_UPGRADE, Verb("upgrade")),
    ("GUI_COMMAND_CANCEL_UPGRADE", b::CANCEL_UPGRADE, Verb("cancel")),
    ("GUI_COMMAND_ATTACK_MOVE", b::ATTACK_MOVE, Verb("attack-move")),
    ("GUI_COMMAND_GUARD", b::GUARD, Verb("guard")),
    ("GUI_COMMAND_GUARD_WITHOUT_PURSUIT", b::GUARD_WITHOUT_PURSUIT, Verb("guard without pursuit")),
    ("GUI_COMMAND_GUARD_FLYING_UNITS_ONLY", b::GUARD_FLYING_UNITS_ONLY, Verb("guard against aircraft")),
    ("GUI_COMMAND_STOP", b::STOP, Verb("stop")),
    ("GUI_COMMAND_WAYPOINTS", b::WAYPOINTS, Grammar("`then` queues moves (MSG_ADD_WAYPOINT)")),
    ("GUI_COMMAND_EXIT_CONTAINER", b::EXIT_CONTAINER, Verb("unload")),
    ("GUI_COMMAND_EVACUATE", b::EVACUATE, Verb("evacuate")),
    ("GUI_COMMAND_EXECUTE_RAILED_TRANSPORT", b::EXECUTE_RAILED_TRANSPORT, Verb("depart")),
    ("GUI_COMMAND_BEACON_DELETE", b::BEACON_DELETE, Verb("remove beacon")),
    ("GUI_COMMAND_SET_RALLY_POINT", b::SET_RALLY_POINT, Verb("set rally point")),
    ("GUI_COMMAND_SELL", b::SELL, Verb("sell")),
    ("GUI_COMMAND_FIRE_WEAPON", b::FIRE_WEAPON, Verb("fire")),
    ("GUI_COMMAND_SPECIAL_POWER", b::SPECIAL_POWER, Verb("use")),
    ("GUI_COMMAND_PURCHASE_SCIENCE", b::PURCHASE_SCIENCE, Verb("purchase")),
    ("GUI_COMMAND_HACK_INTERNET", b::HACK_INTERNET, Verb("hack internet")),
    ("GUI_COMMAND_TOGGLE_OVERCHARGE", b::TOGGLE_OVERCHARGE, Verb("overcharge")),
    ("GUI_COMMAND_POW_RETURN_TO_PRISON", b::POW_RETURN_TO_PRISON, Absent("ALLOW_SURRENDER is off in Zero Hour's build")),
    ("GUI_COMMAND_COMBATDROP", b::COMBATDROP, Verb("combat drop")),
    ("GUI_COMMAND_SWITCH_WEAPON", b::SWITCH_WEAPON, Verb("switch to")),
    ("GUICOMMANDMODE_HIJACK_VEHICLE", b::HIJACK_VEHICLE, Verb("hijack")),
    ("GUICOMMANDMODE_CONVERT_TO_CARBOMB", b::CONVERT_TO_CARBOMB, Verb("car-bomb")),
    ("GUICOMMANDMODE_SABOTAGE_BUILDING", b::SABOTAGE_BUILDING, Verb("sabotage")),
    ("GUICOMMANDMODE_PICK_UP_PRISONER", b::PICK_UP_PRISONER, Absent("ALLOW_SURRENDER is off in Zero Hour's build")),
    ("GUICOMMANDMODE_PLACE_BEACON", b::PLACE_BEACON, Verb("place beacon")),
    ("GUI_COMMAND_SPECIAL_POWER_FROM_SHORTCUT", b::SPECIAL_POWER_FROM_SHORTCUT, Verb("use")),
    ("GUI_COMMAND_SPECIAL_POWER_FROM_COMMAND_CENTER", b::SPECIAL_POWER_FROM_SHORTCUT, Absent("Generals' legacy name for the shortcut (RTS_GENERALS only)")),
    ("GUI_COMMAND_SPECIAL_POWER_CONSTRUCT", b::SPECIAL_POWER_CONSTRUCT, Verb("build")),
    ("GUI_COMMAND_SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT", b::SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT, Verb("build")),
    ("GUI_COMMAND_SELECT_ALL_UNITS_OF_TYPE", b::SELECT_ALL_UNITS_OF_TYPE, Grammar("a type as the selection (`crusader tanks …`)")),
];

/// Every message a player's machine sends the logic, by its engine name.
pub const MESSAGES: &[(&str, Covered)] = &[
    ("MSG_CREATE_SELECTED_GROUP", Grammar("every command's selection")),
    ("MSG_CREATE_SELECTED_GROUP_NO_SOUND", Grammar("the selection, without the units' voice: the same group")),
    ("MSG_DESTROY_SELECTED_GROUP", NotAnOrder("deselecting everything")),
    ("MSG_REMOVE_FROM_SELECTED_GROUP", Grammar("`except` takes units out of a selection")),
    ("MSG_SELECTED_GROUP_COMMAND", Absent("the logic ignores it (an empty case in GameLogicDispatch)")),
    ("MSG_CREATE_TEAM0", Verb("make them group")),
    ("MSG_CREATE_TEAM1", Verb("make them group")),
    ("MSG_CREATE_TEAM2", Verb("make them group")),
    ("MSG_CREATE_TEAM3", Verb("make them group")),
    ("MSG_CREATE_TEAM4", Verb("make them group")),
    ("MSG_CREATE_TEAM5", Verb("make them group")),
    ("MSG_CREATE_TEAM6", Verb("make them group")),
    ("MSG_CREATE_TEAM7", Verb("make them group")),
    ("MSG_CREATE_TEAM8", Verb("make them group")),
    ("MSG_CREATE_TEAM9", Verb("make them group")),
    ("MSG_SELECT_TEAM0", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM1", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM2", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM3", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM4", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM5", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM6", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM7", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM8", Grammar("`group N` as the selection")),
    ("MSG_SELECT_TEAM9", Grammar("`group N` as the selection")),
    ("MSG_ADD_TEAM0", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM1", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM2", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM3", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM4", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM5", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM6", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM7", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM8", Grammar("`and group N` adds a group to the selection")),
    ("MSG_ADD_TEAM9", Grammar("`and group N` adds a group to the selection")),
    ("MSG_DO_ATTACKSQUAD", Absent("nothing in the engine sends it")),
    ("MSG_DO_WEAPON", Verb("fire")),
    ("MSG_DO_WEAPON_AT_LOCATION", Verb("fire")),
    ("MSG_DO_WEAPON_AT_OBJECT", Verb("fire")),
    ("MSG_DO_SPECIAL_POWER", Verb("use")),
    ("MSG_DO_SPECIAL_POWER_AT_LOCATION", Verb("use")),
    ("MSG_DO_SPECIAL_POWER_AT_OBJECT", Verb("use")),
    ("MSG_SET_RALLY_POINT", Verb("set rally point")),
    ("MSG_PURCHASE_SCIENCE", Verb("purchase")),
    ("MSG_QUEUE_UPGRADE", Verb("upgrade")),
    ("MSG_CANCEL_UPGRADE", Verb("cancel")),
    ("MSG_QUEUE_UNIT_CREATE", Verb("build")),
    ("MSG_CANCEL_UNIT_CREATE", Verb("cancel")),
    ("MSG_DOZER_CONSTRUCT", Verb("build")),
    ("MSG_DOZER_CONSTRUCT_LINE", Verb("build")),
    ("MSG_DOZER_CANCEL_CONSTRUCT", Verb("cancel construction")),
    ("MSG_SELL", Verb("sell")),
    ("MSG_EXIT", Verb("unload")),
    ("MSG_EVACUATE", Verb("evacuate")),
    ("MSG_EXECUTE_RAILED_TRANSPORT", Verb("depart")),
    ("MSG_COMBATDROP_AT_LOCATION", Verb("combat drop")),
    ("MSG_COMBATDROP_AT_OBJECT", Verb("combat drop")),
    ("MSG_AREA_SELECTION_DEPRECATED", Absent("deprecated: nothing sends it")),
    ("MSG_DO_ATTACK_OBJECT", Verb("attack")),
    ("MSG_DO_FORCE_ATTACK_OBJECT", Verb("force attack")),
    ("MSG_DO_FORCE_ATTACK_GROUND", Verb("force attack")),
    ("MSG_GET_REPAIRED", Verb("get repaired")),
    ("MSG_GET_HEALED", Verb("get healed")),
    ("MSG_DO_REPAIR", Verb("repair")),
    ("MSG_RESUME_CONSTRUCTION", Verb("resume building")),
    ("MSG_ENTER", Verb("enter")),
    ("MSG_DOCK", Verb("dock")),
    ("MSG_DO_MOVETO", Verb("move")),
    ("MSG_DO_ATTACKMOVETO", Verb("attack-move")),
    ("MSG_DO_FORCEMOVETO", Verb("force move")),
    ("MSG_ADD_WAYPOINT", Grammar("`then` queues moves")),
    ("MSG_DO_GUARD_POSITION", Verb("guard")),
    ("MSG_DO_GUARD_OBJECT", Verb("guard")),
    ("MSG_DO_STOP", Verb("stop")),
    ("MSG_DO_SCATTER", Verb("scatter")),
    ("MSG_INTERNET_HACK", Verb("hack internet")),
    ("MSG_DO_CHEER", Verb("cheer")),
    ("MSG_TOGGLE_OVERCHARGE", Verb("overcharge")),
    ("MSG_SWITCH_WEAPONS", Verb("switch to")),
    ("MSG_CONVERT_TO_CARBOMB", Verb("car-bomb")),
    ("MSG_CAPTUREBUILDING", Verb("capture")),
    ("MSG_DISABLEVEHICLE_HACK", Verb("disable")),
    ("MSG_STEALCASH_HACK", Verb("steal cash")),
    ("MSG_DISABLEBUILDING_HACK", Verb("disable")),
    ("MSG_SNIPE_VEHICLE", Absent("nothing in the engine sends it (the sniper's shot is a special power: `use`)")),
    ("MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION", Verb("act on")),
    ("MSG_DO_SALVAGE", Verb("salvage")),
    ("MSG_CLEAR_INGAME_POPUP_MESSAGE", NotAnOrder("closing a pop-up message")),
    ("MSG_PLACE_BEACON", Verb("place beacon")),
    ("MSG_REMOVE_BEACON", Verb("remove beacon")),
    ("MSG_SET_BEACON_TEXT", NotAnOrder("a beacon's chat text: typed in the overhead view's text box, not an order to units")),
    ("MSG_SET_REPLAY_CAMERA", NotAnOrder("the replay viewer's camera")),
    ("MSG_SELF_DESTRUCT", NotAnOrder("the quit menu's surrender")),
    ("MSG_CREATE_FORMATION", Verb("form up")),
    ("MSG_LOGIC_CRC", NotAnOrder("the lockstep's own check")),
    ("MSG_SET_MINE_CLEARING_DETAIL", Grammar("sent before a button that uses the mine-clearing weapon set, as the control bar does")),
    ("MSG_ENABLE_RETALIATION_MODE", NotAnOrder("a game option")),
];

/// The button codes a verb presses.
pub fn button_codes(verb: &str) -> Vec<i32> {
    BUTTONS.iter().filter(|(_, _, c)| matches!(c, Verb(v) if *v == verb)).map(|&(_, code, _)| code).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn verbs() -> BTreeSet<String> {
        let mut v: BTreeSet<String> = super::super::vocab::fixed_verbs().into_iter().map(|v| v.words).collect();
        // The orders of the grammar itself (`make them group N`).
        v.insert("make them group".into());
        v
    }

    #[test]
    fn every_verb_the_tables_name_is_in_the_language() {
        let known = verbs();
        for (name, c) in BUTTONS.iter().map(|&(n, _, c)| (n, c)).chain(MESSAGES.iter().copied()) {
            if let Verb(v) = c {
                assert!(known.contains(v), "{name}: '{v}' is not a verb of the language");
            }
        }
    }

    #[test]
    fn every_button_code_is_listed_once() {
        let codes: BTreeSet<i32> = BUTTONS.iter().filter(|(n, _, _)| !n.ends_with("_FROM_COMMAND_CENTER")).map(|&(_, c, _)| c).collect();
        assert_eq!(codes.len(), BUTTONS.len() - 1);
        assert_eq!(codes, (b::NONE..=b::SELECT_ALL_UNITS_OF_TYPE).collect());
    }

    /// The names an enum or section of an engine header declares (identifiers starting with one
    /// of `prefixes`, from the line after `from` up to the line holding `to`).
    fn declared(text: &str, from: &str, to: &str, prefixes: &[&str]) -> BTreeSet<String> {
        let start = text.find(from).unwrap_or_else(|| panic!("no '{from}'"));
        let rest = &text[start + from.len()..];
        let end = rest.find(to).unwrap_or_else(|| panic!("no '{to}' after '{from}'"));
        rest[..end]
            .lines()
            .filter_map(|l| {
                let code = l.split("//").next().unwrap_or("").trim();
                let word: String = code.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                prefixes.iter().any(|p| word.starts_with(p)).then_some(word)
            })
            .collect()
    }

    fn engine(path: &str) -> Option<String> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../host/GeneralsGameCode").join(path);
        match std::fs::read_to_string(&p) {
            Ok(t) => Some(t),
            Err(_) => {
                eprintln!("engine submodule not checked out ({}); skipping", p.display());
                None
            }
        }
    }

    /// Constraint 1, the command bar: every command type the engine has is in [`BUTTONS`], and
    /// [`BUTTONS`] names nothing the engine lacks.
    #[test]
    fn every_engine_command_type_is_covered() {
        let Some(h) = engine("Core/GameEngine/Include/GameClient/ControlBar.h") else { return };
        let engine: BTreeSet<String> = declared(&h, "enum GUICommandType", "GUI_COMMAND_NUM_COMMANDS", &["GUI_COMMAND_", "GUICOMMANDMODE_"]);
        let ours: BTreeSet<String> = BUTTONS.iter().map(|(n, _, _)| n.to_string()).collect();
        assert_eq!(engine.difference(&ours).collect::<Vec<_>>(), Vec::<&String>::new(), "engine command types the language does not cover");
        assert_eq!(ours.difference(&engine).collect::<Vec<_>>(), Vec::<&String>::new(), "listed but not in the engine");
    }

    /// Constraint 1, every order: each message a player's machine sends the logic is in
    /// [`MESSAGES`], and [`MESSAGES`] names nothing the engine lacks.
    #[test]
    fn every_engine_network_message_is_covered() {
        let Some(h) = engine("Core/GameEngine/Include/Common/MessageStream.h") else { return };
        let engine: BTreeSet<String> = declared(&h, "MSG_BEGIN_NETWORK_MESSAGES", "MSG_BEGIN_DEBUG_NETWORK_MESSAGES", &["MSG_"]);
        let ours: BTreeSet<String> = MESSAGES.iter().map(|(n, _)| n.to_string()).collect();
        assert_eq!(engine.difference(&ours).collect::<Vec<_>>(), Vec::<&String>::new(), "engine messages the language does not cover");
        assert_eq!(ours.difference(&engine).collect::<Vec<_>>(), Vec::<&String>::new(), "listed but not in the engine");
    }
}
