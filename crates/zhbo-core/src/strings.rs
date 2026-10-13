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
        "Fight as your General in first person. Press F to open your field computer and return to command.",
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

/// NUL-terminated UTF-16 text for a label (case-insensitive, like the CSF lookup), valid for the
/// lifetime of the process.
pub fn lookup(label: &str) -> Option<&'static [u16]> {
    table().get(&label.to_ascii_lowercase()).map(|v| v.as_slice())
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
    }
}
