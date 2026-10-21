//! Localised strings for the mod's INI labels, served to `GameTextManager::fetch` when the label is
//! missing from the game's own string table. This avoids shipping a replacement `generals.csf`.
//!
//! `Data/ConquerOps/strings_en.ini` (shipped in the mod's BIG file, one `label = text` line per
//! entry) is consulted first; the built-in English table below is the fallback so tests and a
//! missing file keep working. The structure is ready for `strings_<lang>.ini` files later.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::classicons;

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

/// The shipped strings table is shared: every test that loads or reads it takes this lock so a
/// parallel test's `load` cannot swap the table mid-assertion (the same pattern as
/// `profile::lock`/`classes::lock`).
#[cfg(test)]
pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

/// The label's text from the shipped file, if loaded and present.
fn shipped(label: &str) -> Option<String> {
    SHIPPED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|t| t.get(&label.to_ascii_lowercase()).cloned())
}

/// Served text, interned by content: a distinct string is leaked once and then reused. The table
/// never shrinks, so a pointer handed to the engine (`zhbo_string`) stays valid for the life of the
/// process — a strings-file reload or a class-list rebuild can never invalidate a pointer the engine
/// still holds — and a hot label (a tooltip, fetched every time it is built) is not re-leaked on
/// every fetch.
fn intern(text: &str) -> &'static [u16] {
    static INTERNED: Mutex<Option<HashMap<String, &'static [u16]>>> = Mutex::new(None);
    let mut guard = INTERNED.lock().unwrap_or_else(|e| e.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    if let Some(served) = table.get(text).copied() {
        return served;
    }
    let served: &'static [u16] = Box::leak(text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice());
    table.insert(text.to_string(), served);
    served
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
    ("CONTROLBAR:ZHBO_ChooseClass", "Create a &Class"),
    (
        "CONTROLBAR:ToolTipZHBO_ChooseClass",
        "Pick a class for your General. The kit replaces his base one and takes effect the next time he takes the field.",
    ),
    // The General's guns and orders under RTS control (sim/rts.rs). Named for BO2's loadout slots,
    // not the weapons: his class is the player's, so the bridge draws each kit's BO2 icon over the
    // button instead (zhbo_client_rts_icons) and the number on it is what is left.
    ("CONTROLBAR:ZHBO_Primary", "&Primary"),
    ("CONTROLBAR:ToolTipZHBO_Primary", "Fight with your primary weapon. Ammo is limited: the number on the button is what is left."),
    ("CONTROLBAR:ZHBO_Secondary", "S&econdary"),
    (
        "CONTROLBAR:ToolTipZHBO_Secondary",
        "Fight with your secondary weapon. The General switches to it on his own when the primary is empty.",
    ),
    ("CONTROLBAR:ZHBO_Lethal", "&Lethal"),
    (
        "CONTROLBAR:ToolTipZHBO_Lethal",
        "Use your lethal where you click: a grenade is thrown there, equipment is put down facing it. The number is how many are left.",
    ),
    ("CONTROLBAR:ZHBO_Tactical", "Tact&ical"),
    (
        "CONTROLBAR:ToolTipZHBO_Tactical",
        "Use your tactical where you click: a grenade is thrown there, equipment is put down facing it. The Black Hat needs first person.",
    ),
    ("CONTROLBAR:ZHBO_Melee", "&Knife"),
    ("CONTROLBAR:ToolTipZHBO_Melee", "Close in on the enemy you click and knife him."),
    ("CONTROLBAR:ZHBO_Reload", "&Reload"),
    ("CONTROLBAR:ToolTipZHBO_Reload", "Reload the weapon in hand from its spare rounds."),
    // The one-page class command set (ZhboControlBar): its navigation and the class buttons'
    // shared tooltip. The class buttons' own labels are derived from the live class list
    // (`CONTROLBAR:ZHBO_ClassButton01`..`15`, see class_label).
    ("CONTROLBAR:ZHBO_ClassNext", "&Next"),
    ("CONTROLBAR:ToolTipZHBO_ClassNext", "Show the rest of your classes."),
    ("CONTROLBAR:ZHBO_ClassBack", "&Back"),
    ("CONTROLBAR:ToolTipZHBO_ClassBack", "Back to the General's commands."),
    (
        "CONTROLBAR:ToolTipZHBO_ClassButton",
        "Pick this class for your General. The kit replaces his base one and is ready the next time he takes the field.",
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

/// A command bar class-button label, named after the live class list (the presets from the
/// registry, the customs from the profile). The label set is rebuilt when that list changes: the
/// profile is static within a match, but the shell's create-a-class screen changes it between
/// matches. The texts go through [`intern`], so a rebuild reuses unchanged labels and never frees
/// the text of a label the engine may still hold.
fn class_label(key: &str) -> Option<&'static [u16]> {
    let n: usize = key.strip_prefix("controlbar:zhbo_classbutton")?.parse().ok()?;
    if n == 0 || n > classicons::BUTTONS {
        return None;
    }
    let wanted = format!("controlbar:zhbo_classbutton{n:02}");
    struct Cache {
        signature: String,
        labels: HashMap<String, &'static [u16]>,
    }
    static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
    let labels = classicons::labels();
    let signature: String = labels.iter().map(|l| l.as_deref().unwrap_or("")).collect::<Vec<_>>().join("\u{1}");
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let stale = guard.as_ref().is_none_or(|c| c.signature != signature);
    if stale {
        let mut table: HashMap<String, &'static [u16]> = HashMap::new();
        for (i, label) in labels.iter().enumerate() {
            if let Some(label) = label {
                let slot = format!("controlbar:zhbo_classbutton{:02}", i + 1);
                table.insert(slot, intern(label));
            }
        }
        *guard = Some(Cache { signature, labels: table });
    }
    guard.as_ref().and_then(|c| c.labels.get(&wanted).copied())
}

/// NUL-terminated UTF-16 text for a label (case-insensitive, like the CSF lookup), valid for the
/// lifetime of the process.
pub fn lookup(label: &str) -> Option<&'static [u16]> {
    let key = label.to_ascii_lowercase();
    // The shipped file wins; its text is interned, so the pointer survives a reload.
    if let Some(text) = shipped(&key) {
        return Some(intern(&text));
    }
    if key.starts_with("controlbar:zhbo_classbutton") {
        return class_label(&key);
    }
    table().get(&key).map(|v| v.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shipped_strings_file() {
        let _g = lock();
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
        let _g = lock();
        // Reset the table so this one exercises the built-in English fallback (the two tests share
        // the global table, so they take the lock one at a time).
        load("");
        let s = lookup("object:zhbo_general").unwrap();
        assert_eq!(*s.last().unwrap(), 0);
        assert_eq!(String::from_utf16(&s[..s.len() - 1]).unwrap(), "General");
        assert!(lookup("OBJECT:NotOurs").is_none());
        let primary = lookup("CONTROLBAR:ZHBO_Primary").unwrap();
        assert_eq!(String::from_utf16(&primary[..primary.len() - 1]).unwrap(), "&Primary", "BO2's slot name: the button's icon is the kit's");
    }

    fn read(served: &[u16]) -> String {
        String::from_utf16(&served[..served.len() - 1]).unwrap()
    }

    fn text(label: &str) -> Option<String> {
        lookup(label).map(read)
    }

    fn custom(name: &str) -> crate::classes::CustomClass {
        crate::classes::CustomClass { name: name.into(), primary: "an94_mp".into(), ..crate::classes::CustomClass::default() }
    }

    /// Every gun and order button's label and tooltip resolves (a missing one renders as "???";
    /// `zhbo_tactical` once did, while the other slots were fine).
    #[test]
    fn every_weapon_slot_label_and_tooltip_resolves() {
        let _g = lock();
        load("");
        for slot in ["Primary", "Secondary", "Lethal", "Tactical", "Melee", "Reload"] {
            let label = format!("CONTROLBAR:ZHBO_{slot}");
            assert!(text(&label).is_some_and(|t| !t.is_empty()), "{label} is served");
            let tooltip = format!("CONTROLBAR:ToolTipZHBO_{slot}");
            assert!(text(&tooltip).is_some_and(|t| !t.is_empty()), "{tooltip} is served");
        }
    }

    /// Every label the mod's command-button INI references resolves through `lookup` — the same
    /// path the engine's GameText hook takes — so an INI typo cannot ship as "???" on the bar.
    #[test]
    fn every_command_button_label_in_the_ini_resolves() {
        // Reads the profile (for the class slots) and the class registry, in the shared order.
        let _g = crate::profile::lock();
        let _c = crate::classes::lock();
        let _s = lock();
        load("");
        crate::profile::reset();
        // Every row that carries a class has a class when all five custom slots are filled: the
        // five defaults plus the active set's five. The ABI's remaining rows ([`classicons::ROWS`]
        // up to `BUTTONS`) are absent, so the bridge never places their buttons.
        for i in 0..crate::classicons::CUSTOMS {
            crate::profile::save(custom(&format!("Custom {i}")));
        }
        for i in 1..=crate::classicons::ROWS {
            let label = format!("CONTROLBAR:ZHBO_ClassButton{i:02}");
            assert!(text(&label).is_some_and(|t| !t.is_empty()), "{label} must serve text");
        }
        for i in crate::classicons::ROWS + 1..=crate::classicons::BUTTONS {
            assert_eq!(text(&format!("CONTROLBAR:ZHBO_ClassButton{i:02}")), None, "row {i} carries no class");
        }
        let ini = include_str!("../../../data/Data/INI/CommandButton/ZHBO_CommandButtons.ini");
        let mut checked = 0;
        for line in ini.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let Some((field, label)) = line.split_once('=') else { continue };
            if !matches!(field.trim(), "TextLabel" | "DescriptLabel") {
                continue;
            }
            let label = label.trim();
            // The INI still declares the ABI's fifteen class buttons; only the ten the core fills
            // carry a label (the bar is one page of Take Control + ten classes + Back).
            if let Some(n) = label.strip_prefix("CONTROLBAR:ZHBO_ClassButton").and_then(|n| n.parse::<usize>().ok()) {
                if n > crate::classicons::ROWS {
                    continue;
                }
            }
            assert!(text(label).is_some_and(|t| !t.is_empty()), "{label} must serve text ({line})");
            checked += 1;
        }
        assert!(checked >= 40, "the INI's label references were parsed (found {checked})");
        crate::profile::reset();
    }

    /// The ABI hands the engine a bare pointer (`zhbo_string`), so the served buffer must never move
    /// or be freed: a reload of the strings file, or a class-list rebuild, must leave a pointer the
    /// engine still holds reading its old text. The text is interned too, so fetching a label twice
    /// cannot leak a second copy (tooltips are fetched every time they are built).
    #[test]
    fn served_text_outlives_a_reload() {
        let _g = lock();
        load("OBJECT:ZHBO_General = Commander");
        let first = lookup("OBJECT:ZHBO_General").unwrap();
        assert_eq!(read(first), "Commander");
        assert_eq!(lookup("OBJECT:ZHBO_General").unwrap().as_ptr(), first.as_ptr(), "one copy per distinct text");
        // The bridge may reload the mod's BIG between games; the old pointer keeps its own bytes.
        load("OBJECT:ZHBO_General = Marshal");
        assert_eq!(read(first), "Commander", "a reload does not free or rewrite the served text");
        assert_eq!(text("OBJECT:ZHBO_General").as_deref(), Some("Marshal"), "the reload serves the new text");
        load("");
    }

    #[test]
    fn class_label_pointers_survive_a_rebuild() {
        let _g = crate::profile::lock();
        let _c = crate::classes::lock();
        let _s = lock();
        crate::profile::reset();
        crate::profile::save(custom("Mine"));
        let mine = lookup("CONTROLBAR:ZHBO_ClassButton06").unwrap();
        assert_eq!(read(mine), "Mine");
        // A shell edit between matches rebuilds the label cache. The engine may still hold the
        // pointer it fetched for the last tooltip, so the old text must stay readable.
        crate::profile::reset();
        crate::profile::save(custom("Other"));
        assert_eq!(read(mine), "Mine", "the rebuilt cache does not free the old label");
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton06").as_deref(), Some("Other"));
        // A rebuild that keeps a name reuses that label's interned buffer.
        crate::profile::reset();
        crate::profile::save(custom("Mine"));
        assert_eq!(lookup("CONTROLBAR:ZHBO_ClassButton06").unwrap().as_ptr(), mine.as_ptr());
        crate::profile::reset();
    }

    #[test]
    fn class_button_labels_follow_the_live_class_list() {
        // This one reads all three globals. Keep the shared order (profile -> registry -> strings)
        // so it can never cycle with the tests that take only one of them.
        let _g = crate::profile::lock();
        let _c = crate::classes::lock();
        let _s = lock();
        crate::profile::reset();
        let first = crate::classes::get().first().map(|c| c.name.clone()).unwrap_or_default();
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton01").as_deref(), Some(first.as_str()));
        // No customs: the custom slots have no labels, and out-of-range labels are ignored.
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton06"), None);
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton16"), None);
        let custom = |name: &str| crate::classes::CustomClass { name: name.into(), primary: "an94_mp".into(), ..crate::classes::CustomClass::default() };
        crate::profile::save(custom("Mine"));
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton06").as_deref(), Some("Mine"));
        // A shell edit between matches refreshes the labels (the cache tracks the live list).
        crate::profile::reset();
        crate::profile::save(custom("Other"));
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton06").as_deref(), Some("Other"));
        // A custom named like a preset is disambiguated on its own button.
        crate::profile::save(custom(&first));
        assert_eq!(text("CONTROLBAR:ZHBO_ClassButton07").as_deref(), Some(format!("{first} 2").as_str()));
        crate::profile::reset();
    }
}
