//! Localised strings for the mod's INI labels, served to `GameTextManager::fetch` when the label is
//! missing from the game's own string table. This avoids shipping a replacement `generals.csf`.

use std::collections::HashMap;
use std::sync::OnceLock;

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
        let (p, s, g) = (w(&l.primary), w(&l.secondary), w(&l.lethal));
        let rounds = |d: &crate::weapons::WeaponDef| format!("{} rounds a magazine, {} spare", d.clip, d.start_reserve);
        let entries = [
            ("CONTROLBAR:ZHBO_Primary".to_string(), format!("&{}", p.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Primary".to_string(), format!("Use the {} ({}). Ammo is limited: the number on the button is what is left.", p.display_name, rounds(p))),
            ("CONTROLBAR:ZHBO_Secondary".to_string(), format!("&{}", s.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Secondary".to_string(), format!("Use the {} ({}). The General switches to it when the {} is empty.", s.display_name, rounds(s), p.display_name)),
            ("CONTROLBAR:ZHBO_Lethal".to_string(), format!("&{}", g.display_name)),
            ("CONTROLBAR:ToolTipZHBO_Lethal".to_string(), format!("Throw {} at the target ({} carried). Only used when selected.", g.display_name, g.start_reserve.max(1))),
        ];
        entries.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v.encode_utf16().chain(std::iter::once(0)).collect())).collect()
    })
}

/// NUL-terminated UTF-16 text for a label (case-insensitive, like the CSF lookup), valid for the
/// lifetime of the process.
pub fn lookup(label: &str) -> Option<&'static [u16]> {
    let key = label.to_ascii_lowercase();
    if key.contains("zhbo_primary") || key.contains("zhbo_secondary") || key.contains("zhbo_lethal") {
        return weapon_labels().get(&key).map(|v| v.as_slice());
    }
    table().get(&key).map(|v| v.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive_and_nul_terminated() {
        let s = lookup("object:zhbo_general").unwrap();
        assert_eq!(*s.last().unwrap(), 0);
        assert_eq!(String::from_utf16(&s[..s.len() - 1]).unwrap(), "General");
        assert!(lookup("OBJECT:NotOurs").is_none());
        let rifle = lookup("CONTROLBAR:ZHBO_Primary").unwrap();
        assert_eq!(String::from_utf16(&rifle[..rifle.len() - 1]).unwrap(), "&AN-94");
    }
}
