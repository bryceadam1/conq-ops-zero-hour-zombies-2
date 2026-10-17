//! Localised strings for the mod's INI labels, served to `GameTextManager::fetch` when the label is
//! missing from the game's own string table. This avoids shipping a replacement `generals.csf`.
//!
//! `Data/ConquerOps/strings_en.ini` (shipped in the mod's BIG file, one `label = text` line per
//! entry) is consulted first; the built-in English table below is the fallback so tests and a
//! missing file keep working. The structure is ready for `strings_<lang>.ini` files later.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The shipped strings file, loaded like the weapon table (the bridge reads it into
/// `zhbo_load_strings` at init). `None` until then.
static SHIPPED: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Load the shipped strings table (the bridge passes `Data/ConquerOps/strings_en.ini`'s text).
/// Later loads replace it (a reset may reload the mod's BIG file).
pub fn load(text: &str) {
    let table: HashMap<String, String> = text
        .lines()
        .map(|l| l.split(';').next().unwrap_or("").trim())
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect();
    *SHIPPED.lock().unwrap_or_else(|e| e.into_inner()) = Some(table);
}

/// The label's text from the shipped file, if loaded and present.
fn shipped(label: &str) -> Option<String> {
    SHIPPED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|t| t.get(&label.to_ascii_lowercase()).cloned())
}

const ENGLISH: &[(&str, &str)] = &[
    ("OBJECT:ZHBO_General", "General"),
    ("CONTROLBAR:ZHBO_ConstructGeneral", "Deploy &General"),
    (
        "CONTROLBAR:ToolTipZHBO_ConstructGeneral",
        "Put yourself on the battlefield. Select your General and use Take Control to fight in first person.",
    ),
    ("CONTROLBAR:ZHBO_TakeControl", "&Take Control"),
    (
        "CONTROLBAR:ToolTipZHBO_TakeControl",
        "Fight as your General in first person, with the selected weapon and the ammo left. Press F to open your field computer and return to command.",
    ),
    ("CONTROLBAR:ZHBO_BuildMenu", "&Build"),
    (
        "CONTROLBAR:ToolTipZHBO_BuildMenu",
        "Build structures, like a dozer or worker (more slowly). In first person, press B for your hammer, place with fire, then hold F at the site to build.",
    ),
    ("CONTROLBAR:ZHBO_BuildBack", "&Back"),
    ("CONTROLBAR:ToolTipZHBO_BuildBack", "Back to the General's commands."),
];

fn table() -> &'static HashMap<String, Vec<u16>> {
    static TABLE: OnceLock<HashMap<String, Vec<u16>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        ENGLISH
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.encode_utf16().chain(std::iter::once(0)).collect()))
            .collect()
    })
}

/// The General's weapon buttons, named after the loadout's weapons. Built the first time one is
/// asked for: the control bar asks after the converted weapon table is installed.
fn weapon_labels() -> &'static HashMap<String, Vec<u16>> {
    static TABLE: OnceLock<HashMap<String, Vec<u16>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let l = crate::weapons::Loadout::default();
        let w = |id: &str| crate::weapons::weapon(id);
        let (p, s, g, t) = (w(&l.primary), w(&l.secondary), w(&l.lethal), w(&l.tactical));
        let rounds = |d: &crate::weapons::WeaponDef| format!("{} rounds a magazine, {} spare", d.clip, d.start_reserve);
        let entries = [
            ("CONTROLBAR:ZHBO_Primary".to_string(), format!("&{}", p.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Primary".to_string(), format!("Use the {} ({}). Ammo is limited: the number on the button is what is left.", p.display_name, rounds(p))),
            ("CONTROLBAR:ZHBO_Secondary".to_string(), format!("&{}", s.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Secondary".to_string(), format!("Use the {} ({}). The General switches to it when the {} is empty.", s.display_name, rounds(s), p.display_name)),
            ("CONTROLBAR:ZHBO_Lethal".to_string(), format!("&{}", g.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Lethal".to_string(), format!("Throw {} at the target ({} carried). Only used when selected.", g.display_name, g.start_reserve.max(1))),
            ("CONTROLBAR:ZHBO_Tactical".to_string(), format!("&{}", t.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Tactical".to_string(), format!("Throw {} at the target ({} carried). Only used when selected.", t.display_name, t.start_reserve.max(1))),
        ];
        entries.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v.encode_utf16().chain(std::iter::once(0)).collect())).collect()
    })
}

/// NUL-terminated UTF-16 text for a label (case-insensitive, like the CSF lookup), valid for the
/// lifetime of the process.
pub fn lookup(label: &str) -> Option<&'static [u16]> {
    let key = label.to_ascii_lowercase();
    // The shipped file wins; its values live for the process (leaked once per load).
    if let Some(text) = shipped(&key) {
        let leaked: &'static [u16] = Box::leak(text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice());
        return Some(leaked);
    }
    if key.contains("zhbo_primary") || key.contains("zhbo_secondary") || key.contains("zhbo_lethal") {
        return weapon_labels().get(&key).map(|v| v.as_slice());
    }
    table().get(&key).map(|v| v.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shipped_strings_file() {
        load("; strings_en.ini\nOBJECT:ZHBO_General = Commander\nCONTROLBAR:ZHBO_BuildBack = &Back ");
        assert_eq!(shipped("object:zhbo_general").as_deref(), Some("Commander"));
        assert_eq!(shipped("controlbar:zhbo_buildback").as_deref(), Some("&Back"));
        assert_eq!(shipped("object:other"), None);
        // lookup consults it before the built-in table, and the text is NUL-terminated UTF-16.
        let s = lookup("OBJECT:ZHBO_General").unwrap();
        assert_eq!(String::from_utf16(&s[..s.len() - 1]).unwrap(), "Commander");
    }

    #[test]
    fn lookup_is_case_insensitive_and_nul_terminated() {
        // The other test loaded a shipped table that overrides the built-in English: reset it
        // so this one exercises the fallback.
        load("");
        let s = lookup("object:zhbo_general").unwrap();
        assert_eq!(*s.last().unwrap(), 0);
        assert_eq!(String::from_utf16(&s[..s.len() - 1]).unwrap(), "General");
        assert!(lookup("OBJECT:NotOurs").is_none());
        let rifle = lookup("CONTROLBAR:ZHBO_Primary").unwrap();
        assert_eq!(String::from_utf16(&rifle[..rifle.len() - 1]).unwrap(), "&AN-94");
    }
}
