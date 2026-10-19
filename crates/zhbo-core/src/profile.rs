//! The player's create-a-class profile: the custom classes the create-a-class editor saves, kept
//! between sessions in a small text file next to the mod's log (`ConquerOps.profile` in the game's
//! user-data directory). BO2 keeps its custom classes in the player's profile the same way; here the
//! file is one `name\nspec` record per class — the same wire format the choose-class message carries
//! ([`crate::classes::CustomClass::to_wire`]) — records separated by a blank line, so a class whose
//! name or spec is empty still round-trips.
//!
//! The module is pure: [`Profile::to_text`]/[`Profile::from_text`] do the serialisation, and the
//! bridge does the file I/O (`Zhbo_LoadProfile` at init, `Zhbo_SaveProfile` on a save). The registry
//! ([`crate::classes`]) stays the shipped, identical-on-every-client list; the profile's entries are
//! appended to the pick screen's list as the local player's own classes.

use crate::classes::CustomClass;
use std::sync::Mutex;

/// The most classes the profile keeps, matching BO2's ten custom-class slots. A save past the cap
/// replaces the oldest.
pub const MAX_CLASSES: usize = 10;

/// The profile loaded at init; empty until `load` runs.
static PROFILE: Mutex<Option<Profile>> = Mutex::new(None);

/// Load the profile from the file's text (the bridge reads `ConquerOps.profile` from the user-data
/// directory at init). Later loads replace it (a reset may reload).
pub fn load(text: &str) {
    *PROFILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Profile::from_text(text));
}

/// Forget the loaded profile, back to empty (tests: one test's load must not leak into another's).
#[cfg(test)]
pub fn reset() {
    *PROFILE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The profile in force, cloned (the editor and the pick screen read it; the bridge writes it).
pub fn get() -> Profile {
    PROFILE.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Save a class into the global profile. Returns the profile's full text after the save, so the
/// caller (the bridge) can write the whole file (simplest, and immune to a torn append).
pub fn save(class: CustomClass) -> String {
    let mut guard = PROFILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut p = guard.clone().unwrap_or_default();
    p.save(class);
    let text = p.to_text();
    *guard = Some(p);
    text
}

/// The saved custom class with this name, if any.
pub fn by_name(name: &str) -> Option<CustomClass> {
    get().by_name(name).cloned()
}

/// The saved custom classes, in save order.
pub fn classes() -> Vec<CustomClass> {
    get().classes
}

/// The saved custom classes, most recently saved last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub classes: Vec<CustomClass>,
}

impl Profile {
    /// An empty profile (no custom classes).
    pub fn new() -> Profile {
        Profile::default()
    }

    /// Save a class: a class with the same name replaces the matching entry (BO2 overwrites a slot
    /// the player re-edits); a new name appends. Past the cap the oldest is dropped. The name is
    /// trimmed and a blank name is given BO2's default slot name.
    pub fn save(&mut self, mut class: CustomClass) {
        class.name = class.name.trim().to_string();
        if class.name.is_empty() {
            class.name = format!("Custom Class {}", self.classes.len() + 1);
        }
        if let Some(existing) = self.classes.iter_mut().find(|c| c.name.eq_ignore_ascii_case(&class.name)) {
            *existing = class;
            return;
        }
        self.classes.push(class);
        if self.classes.len() > MAX_CLASSES {
            self.classes.remove(0);
        }
    }

    /// The class with this name (case-insensitive), for re-opening it in the editor.
    pub fn by_name(&self, name: &str) -> Option<&CustomClass> {
        self.classes.iter().find(|c| c.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Whether the profile has no custom classes.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    /// Serialise to the profile file's text: one `name\nspec` record per class, records separated
    /// by a blank line. Always ends with a newline (an empty profile is the empty string).
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.classes.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&c.to_wire());
            out.push('\n');
        }
        out
    }

    /// Parse the profile file's text. A record without a spec half is skipped; a malformed file
    /// loses only the broken record, never the whole profile. The leading BOM some editors add is
    /// stripped from the first record's name.
    pub fn from_text(text: &str) -> Profile {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut classes: Vec<CustomClass> = Vec::new();
        for record in text.split("\n\n") {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                continue;
            }
            if let Some(c) = CustomClass::from_wire(record) {
                if classes.len() < MAX_CLASSES && !classes.iter().any(|x| x.name.eq_ignore_ascii_case(&c.name)) {
                    classes.push(c);
                }
            }
        }
        Profile { classes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(name: &str, primary: &str) -> CustomClass {
        CustomClass { name: name.into(), primary: primary.into(), ..CustomClass::default() }
    }

    #[test]
    fn save_appends_and_replaces_by_name() {
        let mut p = Profile::new();
        p.save(class("My Rifle", "an94_mp+reflex"));
        p.save(class("My Sniper", "dsr50_mp+acog"));
        assert_eq!(p.classes.len(), 2);
        // Re-saving the same name replaces, not appends.
        p.save(class("my rifle", "an94_mp+gl"));
        assert_eq!(p.classes.len(), 2);
        assert_eq!(p.by_name("My Rifle").unwrap().primary, "an94_mp+gl");
    }

    #[test]
    fn save_names_a_blank_class() {
        let mut p = Profile::new();
        p.save(class("", "an94_mp"));
        assert_eq!(p.classes[0].name, "Custom Class 1");
        p.save(class("   ", "fnp45_mp"));
        assert_eq!(p.classes[1].name, "Custom Class 2");
    }

    #[test]
    fn save_drops_the_oldest_past_the_cap() {
        let mut p = Profile::new();
        for i in 0..MAX_CLASSES + 2 {
            p.save(class(&format!("Class {i}"), "an94_mp"));
        }
        assert_eq!(p.classes.len(), MAX_CLASSES);
        assert!(p.by_name("Class 0").is_none(), "the oldest went");
        assert!(p.by_name(&format!("Class {}", MAX_CLASSES + 1)).is_some());
    }

    #[test]
    fn round_trips_through_the_text() {
        let mut p = Profile::new();
        p.save(class("My Rifle", "an94_mp+reflex"));
        p.save(CustomClass { name: "Support".into(), primary: "an94_mp".into(), perks: vec!["FlakJacket".into()], ..CustomClass::default() });
        let text = p.to_text();
        let back = Profile::from_text(&text);
        assert_eq!(back, p);
        // An empty profile is the empty string and parses back empty.
        assert_eq!(Profile::new().to_text(), "");
        assert!(Profile::from_text("").is_empty());
    }

    #[test]
    fn from_text_skips_broken_records_and_keeps_the_rest() {
        // A record without a spec half is skipped; the good one survives. A BOM is stripped.
        let text = "\u{feff}Good\nprimary=an94_mp\n\nBroken without a spec\n\n";
        let p = Profile::from_text(text);
        assert_eq!(p.classes.len(), 1);
        assert_eq!(p.classes[0].name, "Good");
    }
}
