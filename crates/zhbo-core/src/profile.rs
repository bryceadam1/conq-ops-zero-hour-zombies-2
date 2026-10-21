//! The player's create-a-class profile: BO2's class sets, kept between sessions in a small text
//! file next to the mod's log (`ConquerOps.profile` in the game's user-data directory).
//!
//! BO2's model (`cacutility.lua`'s `maxCustomClass` and `cacchooseclass.lua`): **ten class sets**,
//! each with **five custom slots**. The five default classes are shared by every set (they are the
//! class registry's, not the profile's), and the set the player marked active is the one the
//! in-game pick and the command bar show — "You will load into the game with this class set".
//!
//! The file is text and versioned; a v1 file (the old flat list of `name\nspec` records) is
//! migrated on load: its classes land in set 1's slots 1..5, then set 2's slots 1..5, and set 1 is
//! active (the old profile had no sets, so one set holds what used to be the whole list).
//!
//! ```text
//! v2
//! active 1
//! set 1 CLASS SET 1
//! class
//! My Rifle
//! primary=an94_mp+reflex;secondary=fnp45_mp
//! end
//! set 2 CLASS SET 2
//! ```
//!
//! A class record is `class`, its name, its spec (the wire format [`CustomClass::to_wire`] carries
//! on the choose-class message) and `end`; the spec line is optional (an empty spec inherits the
//! base kit). The parser is tolerant: an unknown line, a record without its spec half or a sixth
//! class in one set loses only itself, never the profile. Every class read back is *repaired*
//! ([`crate::classrepair`], BO2's `checkclasses.lua`), not dropped: a stored class whose weapon went
//! unknown is kept with that slot cleared, trimmed back inside the ten-point allocation.
//!
//! The module is pure: [`Profile::to_text`]/[`Profile::from_text`] do the serialisation, and the
//! bridge does the file I/O (`Zhbo_LoadProfile` at init, `Zhbo_SaveProfile` on a save). The registry
//! ([`crate::classes`]) stays the shipped, identical-on-every-client list; the active set's entries
//! join the pick screen and the command bar as the local player's own classes.

use crate::classes::{self, CustomClass};
use std::sync::Mutex;

/// Custom class slots one set holds, BO2's `maxCustomClass`.
pub const MAX_CLASSES: usize = classes::MAX_CUSTOM_CLASSES;

/// The class sets a profile carries, BO2's ten.
pub const MAX_SETS: usize = classes::MAX_CLASS_SETS;

/// The file's version marker: v2 adds the class sets.
const VERSION: &str = "v2";

/// The profile loaded at init; empty until `load` runs.
static PROFILE: Mutex<Option<Profile>> = Mutex::new(None);

/// Load the profile from the file's text (the bridge reads `ConquerOps.profile` from the user-data
/// directory at init). A v1 file is migrated; every class is repaired. Later loads replace it (a
/// reset may reload).
pub fn load(text: &str) {
    *PROFILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Profile::from_text(text));
}

/// Forget the loaded profile, back to empty (tests: one test's load must not leak into another's).
#[cfg(test)]
pub fn reset() {
    *PROFILE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The global profile is shared: every test that touches it (here and in `client`/`classui`) takes
/// this lock so parallel test threads cannot see each other's saves.
#[cfg(test)]
pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

/// The profile in force, cloned (the editor and the pick screen read it; the bridge writes it).
pub fn get() -> Profile {
    PROFILE.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Apply `change` to the global profile and return its result with the file's new text, or `None`
/// when the change was refused (nothing was written). The caller (the bridge) writes the whole
/// file: simplest, and immune to a torn append.
fn update<T>(change: impl FnOnce(&mut Profile) -> Option<T>) -> Option<(T, String)> {
    let mut guard = PROFILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut p = guard.clone().unwrap_or_default();
    let value = change(&mut p)?;
    let text = p.to_text();
    *guard = Some(p);
    Some((value, text))
}

/// Save a class into the active set. Returns the profile's full text after the save (even when the
/// set was full and the save was refused, in which case the text is unchanged).
pub fn save(class: CustomClass) -> String {
    let mut guard = PROFILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut p = guard.clone().unwrap_or_default();
    p.save(class);
    let text = p.to_text();
    *guard = Some(p);
    text
}

/// Save a class into one slot of the active set (`slot` is 0-based; `slot == len` appends). Returns
/// the new text, or `None` when the set is full or the slot is past the end.
pub fn save_in_slot(slot: usize, class: CustomClass) -> Option<String> {
    update(|p| p.save_in_slot(p.active_set, slot, class).then_some(())).map(|(_, text)| text)
}

/// Save a class into one slot of a named set (`slot == len` appends to its end). Returns the new
/// text, or `None` when the set is unknown, full, or the slot is past its end.
pub fn save_in_set(set: usize, slot: usize, class: CustomClass) -> Option<String> {
    update(|p| p.save_in_slot(set, slot, class).then_some(())).map(|(_, text)| text)
}

/// Remove a class from the active set by name. Returns the new text, or `None` when it was not
/// there.
pub fn remove(name: &str) -> Option<String> {
    update(|p| p.remove(name).then_some(())).map(|(_, text)| text)
}

/// Remove the active set's class in `slot` (BO2's delete/reset of a custom slot).
pub fn remove_in_slot(slot: usize) -> Option<String> {
    update(|p| p.remove_in_slot(p.active_set, slot).then_some(())).map(|(_, text)| text)
}

/// Duplicate a class into a free slot of the active set. Returns the copy's new name and the new
/// text, or `None` when the source is missing or the set is full.
pub fn copy(name: &str) -> Option<(String, String)> {
    update(|p| p.copy(name))
}

/// Duplicate the active set's class in `slot` into the set's first free slot.
pub fn copy_in_slot(slot: usize) -> Option<(String, String)> {
    update(|p| p.copy_in_slot(slot))
}

/// Rename a class set (0-based). Returns the new text, or `None` for an unknown set.
pub fn rename_set(set: usize, name: &str) -> Option<String> {
    update(|p| p.rename_set(set, name).then_some(())).map(|(_, text)| text)
}

/// Empty a class set's custom slots (BO2's "reset class set"), keeping its name.
pub fn reset_set(set: usize) -> Option<String> {
    update(|p| p.reset_set(set).then_some(())).map(|(_, text)| text)
}

/// Make a class set the active one. Returns the new text, or `None` for an unknown set.
pub fn set_active(set: usize) -> Option<String> {
    update(|p| p.set_active(set).then_some(())).map(|(_, text)| text)
}

/// Move the active set by `step` (wrapping, as BO2's shoulder buttons do). Returns the new text.
pub fn cycle_active(step: i32) -> Option<String> {
    update(|p| p.cycle_active(step).then_some(())).map(|(_, text)| text)
}

/// The saved custom classes of the active set, in slot order (the list the pick screen and the
/// command bar show).
pub fn classes() -> Vec<CustomClass> {
    get().customs().to_vec()
}

/// The class with this name in the active set, if any.
pub fn by_name(name: &str) -> Option<CustomClass> {
    get().by_name(name).cloned()
}

/// The active set's index (0-based).
pub fn active_set() -> usize {
    get().active_set()
}

/// A set's display name (never empty: a set the player never renamed shows `CLASS SET n`).
pub fn set_name(set: usize) -> String {
    get().set_name(set)
}

/// Pack UTF-8 text into the `u32` words the client ABI carries (four bytes little-endian per
/// word). Used for the profile write one-shot, which must carry the whole file.
pub fn pack_text(text: &str) -> Vec<u32> {
    let bytes = text.as_bytes();
    let mut words = Vec::with_capacity(bytes.len().div_ceil(4));
    for chunk in bytes.chunks(4) {
        let mut w = 0u32;
        for (i, b) in chunk.iter().enumerate() {
            w |= (*b as u32) << (8 * i);
        }
        words.push(w);
    }
    words
}

/// Undo [`pack_text`]; `None` when the bytes are not UTF-8 (a truncated or corrupt payload).
pub fn unpack_text(words: &[u32]) -> Option<String> {
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8(bytes[..len].to_vec()).ok()
}

/// One class set: its name and its custom slots, in slot order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassSet {
    /// What the set is called; a blank name falls back to `CLASS SET n`.
    pub name: String,
    /// The custom classes, at most [`MAX_CLASSES`].
    pub customs: Vec<CustomClass>,
}

impl ClassSet {
    /// BO2's default set name (`CLASS_SET1`..`CLASS_SET10` in `reset_public_class_sets.cfg`).
    pub fn default_name(index: usize) -> String {
        format!("CLASS SET {}", index + 1)
    }

    /// The name the set shows: its own, or the default when the player cleared it.
    pub fn name_or_default(&self, index: usize) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            ClassSet::default_name(index)
        } else {
            name.to_string()
        }
    }

    /// Whether the set holds no custom classes.
    pub fn is_empty(&self) -> bool {
        self.customs.is_empty()
    }
}

/// The player's class sets and which one the match uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// BO2's ten sets (`CLASS SET 1`..`10`), each with five custom slots.
    pub sets: [ClassSet; MAX_SETS],
    /// The set the in-game pick and the command bar show; 0-based.
    pub active_set: usize,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            sets: std::array::from_fn(|i| ClassSet { name: ClassSet::default_name(i), customs: Vec::new() }),
            active_set: 0,
        }
    }
}

impl Profile {
    /// An empty profile: ten empty sets, the first active.
    pub fn new() -> Profile {
        Profile::default()
    }

    /// The active set's index, clamped into range (a hand-edited file can name set 99).
    pub fn active_set(&self) -> usize {
        self.active_set.min(MAX_SETS - 1)
    }

    /// The set a match loads into.
    pub fn active(&self) -> &ClassSet {
        &self.sets[self.active_set()]
    }

    /// The active set's custom classes, in slot order.
    pub fn customs(&self) -> &[CustomClass] {
        &self.active().customs
    }

    /// A set's customs by index (`None` past the tenth set).
    pub fn customs_in(&self, set: usize) -> Option<&[CustomClass]> {
        self.sets.get(set).map(|s| s.customs.as_slice())
    }

    /// A set's display name (`CLASS SET n` when it was never renamed).
    pub fn set_name(&self, set: usize) -> String {
        self.sets.get(set).map(|s| s.name_or_default(set)).unwrap_or_default()
    }

    /// Make `set` the active one (BO2: "You will load into the game with this class set").
    pub fn set_active(&mut self, set: usize) -> bool {
        if set >= MAX_SETS {
            return false;
        }
        self.active_set = set;
        true
    }

    /// Move the active set by `step`, wrapping through all ten (BO2's shoulder buttons wrap).
    pub fn cycle_active(&mut self, step: i32) -> bool {
        if step == 0 {
            return false;
        }
        let n = MAX_SETS as i32;
        self.active_set = (self.active_set() as i32 + step).rem_euclid(n) as usize;
        true
    }

    /// Rename a set (`CLASS SET 3`); a blank name resets it to the default.
    pub fn rename_set(&mut self, set: usize, name: &str) -> bool {
        let Some(s) = self.sets.get_mut(set) else { return false };
        let name = name.trim();
        s.name = if name.is_empty() { ClassSet::default_name(set) } else { name.to_string() };
        true
    }

    /// Empty a set's custom slots; its name stays.
    pub fn reset_set(&mut self, set: usize) -> bool {
        let Some(s) = self.sets.get_mut(set) else { return false };
        s.customs.clear();
        true
    }

    /// Save a class into the active set: a class with the same name replaces the matching slot
    /// (the editor re-saving a class), a new name appends to the first free slot. False when the
    /// set is full. The name is trimmed and a blank name is given BO2's default slot name.
    pub fn save(&mut self, class: CustomClass) -> bool {
        let set = self.active_set();
        let name = class.name.trim();
        let slot = if name.is_empty() {
            None
        } else {
            self.sets[set].customs.iter().position(|c| c.name.trim().eq_ignore_ascii_case(name))
        };
        let slot = slot.unwrap_or(self.sets[set].customs.len());
        self.save_in_slot(set, slot, class)
    }

    /// Save a class into one slot of a set: `slot < len` replaces that slot (BO2 edits a slot, it
    /// does not reorder the set), `slot == len` appends. False when the set is full, the slot is
    /// past the end, or the set does not exist.
    pub fn save_in_slot(&mut self, set: usize, slot: usize, mut class: CustomClass) -> bool {
        if set >= MAX_SETS {
            return false;
        }
        let customs = &mut self.sets[set].customs;
        if slot > customs.len() || (slot == customs.len() && customs.len() >= MAX_CLASSES) {
            return false;
        }
        class.name = class.name.trim().to_string();
        if class.name.is_empty() {
            class.name = crate::classui::empty_slot_name(slot);
        }
        if slot < customs.len() {
            customs[slot] = class;
        } else {
            customs.push(class);
        }
        true
    }

    /// Remove a class by name from the active set; returns whether it was there.
    pub fn remove(&mut self, name: &str) -> bool {
        let set = self.active_set();
        let Some(slot) = self.sets[set].customs.iter().position(|c| c.name.eq_ignore_ascii_case(name.trim())) else {
            return false;
        };
        self.sets[set].customs.remove(slot);
        true
    }

    /// Remove a set's class by slot; returns whether it was there.
    pub fn remove_in_slot(&mut self, set: usize, slot: usize) -> bool {
        let Some(s) = self.sets.get_mut(set) else { return false };
        if slot >= s.customs.len() {
            return false;
        }
        s.customs.remove(slot);
        true
    }

    /// Duplicate the active set's class named `name` into its first free slot, named `<name> COPY`,
    /// `<name> COPY 2`, … (BO2's copy class). Returns the new name.
    pub fn copy(&mut self, name: &str) -> Option<String> {
        let set = self.active_set();
        let slot = self.sets[set].customs.iter().position(|c| c.name.eq_ignore_ascii_case(name.trim()))?;
        self.copy_in_slot(slot)
    }

    /// Duplicate a set's class by slot into the set's first free slot.
    pub fn copy_in_slot(&mut self, slot: usize) -> Option<String> {
        let set = self.active_set();
        if self.sets[set].customs.len() >= MAX_CLASSES {
            return None;
        }
        let source = self.sets[set].customs.get(slot)?.clone();
        let copy_name = self.free_copy_name(&source.name);
        let copy = CustomClass { name: copy_name.clone(), ..source };
        self.save_in_slot(set, self.sets[set].customs.len(), copy).then_some(copy_name)
    }

    /// A copy name not already in the active set.
    fn free_copy_name(&self, base: &str) -> String {
        let base = base.trim();
        let stem = if base.is_empty() { "Custom Class" } else { base };
        let taken = |n: &str| self.by_name(n).is_some();
        let first = format!("{stem} COPY");
        if !taken(&first) {
            return first;
        }
        for n in 2..=MAX_CLASSES * 2 {
            let name = format!("{stem} COPY {n}");
            if !taken(&name) {
                return name;
            }
        }
        format!("{stem} COPY {}", self.customs().len() + 1)
    }

    /// The active set's class with this name (case-insensitive), for re-opening it in the editor.
    pub fn by_name(&self, name: &str) -> Option<&CustomClass> {
        self.active().customs.iter().find(|c| c.name.eq_ignore_ascii_case(name.trim()))
    }

    /// A set's class in one slot.
    pub fn class_in_slot(&self, slot: usize) -> Option<&CustomClass> {
        self.active().customs.get(slot)
    }

    /// Whether the active set has no custom classes.
    pub fn is_empty(&self) -> bool {
        self.active().is_empty()
    }

    /// Repair every stored class ([`crate::classrepair`], BO2's `checkclasses.lua`); returns how
    /// many were changed. Loading does this, and so does re-opening a class in the editor.
    pub fn repair(&mut self) -> usize {
        let mut changed = 0;
        for set in &mut self.sets {
            for class in &mut set.customs {
                if !crate::classrepair::repair(class).is_empty() {
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Serialise to the profile file's text (see the module docs). Always ends with a newline and
    /// always carries all ten sets, so the file shows the sets the player has not filled yet.
    pub fn to_text(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        out.push_str(&format!("active {}\n", self.active_set() + 1));
        for (i, set) in self.sets.iter().enumerate() {
            out.push_str(&format!("set {} {}\n", i + 1, set.name_or_default(i)));
            for c in &set.customs {
                out.push_str("class\n");
                out.push_str(&c.to_wire());
                out.push('\n');
                out.push_str("end\n");
            }
        }
        out
    }

    /// Parse the profile file's text, migrating v1 and repairing every class (see the module docs).
    /// A malformed record loses only itself, never the whole profile; a leading BOM is stripped.
    pub fn from_text(text: &str) -> Profile {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut p = if is_v2(text) { Profile::from_v2(text) } else { Profile::from_v1(text) };
        p.repair();
        p
    }

    /// A v1 file: `name\nspec` records separated by a blank line. Migration puts the first five
    /// classes in set 1, the next five in set 2, and makes set 1 active — the old file had one flat
    /// list, and the first set is what the old build would have loaded into a match.
    fn from_v1(text: &str) -> Profile {
        let mut p = Profile::new();
        let mut parsed: Vec<CustomClass> = Vec::new();
        for record in text.split("\n\n") {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                continue;
            }
            if let Some(c) = CustomClass::from_wire(record) {
                if parsed.len() < MAX_SETS * MAX_CLASSES {
                    parsed.push(c);
                }
            }
        }
        for (i, c) in parsed.into_iter().enumerate() {
            p.save_in_slot(i / MAX_CLASSES, i % MAX_CLASSES, c);
        }
        p.active_set = 0;
        p
    }

    /// A v2 file: `v2`, `active n`, then `set n name` blocks of `class`/name/spec/`end` records.
    fn from_v2(text: &str) -> Profile {
        let mut p = Profile::new();
        let mut set = 0usize;
        let mut lines = text.lines();
        while let Some(raw) = lines.next() {
            let line = raw.trim();
            if line.is_empty() || line == VERSION {
                continue;
            }
            if let Some(rest) = line.strip_prefix("active ") {
                if let Ok(n) = rest.trim().parse::<usize>() {
                    if (1..=MAX_SETS).contains(&n) {
                        p.active_set = n - 1;
                    }
                }
            } else if let Some(rest) = line.strip_prefix("set ") {
                let (num, name) = rest.split_once(' ').unwrap_or((rest, ""));
                if let Ok(n) = num.trim().parse::<usize>() {
                    if (1..=MAX_SETS).contains(&n) {
                        set = n - 1;
                        let name = name.trim();
                        p.sets[set].name = if name.is_empty() { ClassSet::default_name(set) } else { name.to_string() };
                    }
                }
            } else if line.eq_ignore_ascii_case("class") {
                // The record is its name line and its spec line; `end` closes it, and a record
                // with a name but no spec inherits the base kit (the spec line is optional).
                let name = lines.next().unwrap_or("").trim().to_string();
                let mut spec = String::new();
                if let Some(next) = lines.next() {
                    if next.trim().eq_ignore_ascii_case("end") {
                        if name.is_empty() {
                            continue;
                        }
                    } else {
                        spec = next.trim().to_string();
                        if lines.clone().next().is_some_and(|l| l.trim().eq_ignore_ascii_case("end")) {
                            lines.next();
                        }
                    }
                }
                if name.is_empty() && spec.is_empty() {
                    continue;
                }
                let mut c = CustomClass::from_spec(&spec);
                c.name = name;
                let slot = p.sets[set].customs.len();
                if slot < MAX_CLASSES {
                    p.save_in_slot(set, slot, c);
                }
            }
        }
        p
    }
}

/// Whether the text is a v2 profile: the version marker, then `active` or a `set` line (the writer
/// always emits both). A v1 file whose first class happens to be called `v2` — `v2\nprimary=…` —
/// is not mistaken for a version-2 file and lost.
fn is_v2(text: &str) -> bool {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    lines.next() == Some(VERSION) && lines.next().is_some_and(|l| l.starts_with("active ") || l.starts_with("set "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(name: &str, primary: &str) -> CustomClass {
        CustomClass { name: name.into(), primary: primary.into(), ..CustomClass::default() }
    }

    #[test]
    fn a_new_profile_has_ten_empty_sets_and_the_first_active() {
        let p = Profile::new();
        assert_eq!(p.sets.len(), MAX_SETS);
        assert_eq!(p.active_set, 0);
        assert!(p.sets.iter().all(|s| s.is_empty()));
        assert_eq!(p.set_name(0), "CLASS SET 1");
        assert_eq!(p.set_name(9), "CLASS SET 10");
        // The empty profile's text is the version line, the active set and the ten sets.
        let text = p.to_text();
        assert!(text.starts_with("v2\nactive 1\nset 1 CLASS SET 1\n"));
        assert_eq!(Profile::from_text(&text), p);
    }

    #[test]
    fn save_fills_five_slots_then_refuses() {
        let _g = lock();
        let mut p = Profile::new();
        for i in 0..MAX_CLASSES {
            assert!(p.save(class(&format!("Class {i}"), "an94_mp")), "slot {i} is free");
        }
        assert_eq!(p.customs().len(), 5);
        // A sixth class is refused, not silently dropped over an older one (BO2 replaces the slot
        // the player edits; it never evicts).
        assert!(!p.save(class("Sixth", "an94_mp")));
        assert_eq!(p.customs().len(), 5);
        assert!(p.by_name("Class 0").is_some(), "nothing was evicted");
        // Re-saving a stored name replaces its slot.
        assert!(p.save(class("class 0", "dsr50_mp")));
        assert_eq!(p.customs().len(), 5);
        assert_eq!(p.by_name("Class 0").unwrap().primary, "dsr50_mp");
        // Slot saves: an existing slot is replaced in place, a past-the-end slot refused.
        assert!(p.save_in_slot(0, 2, class("Replaced", "fnp45_mp")));
        assert_eq!(p.customs()[2].name, "Replaced");
        assert!(!p.save_in_slot(0, 9, class("Too far", "an94_mp")));
        assert!(!p.save_in_slot(MAX_SETS, 0, class("No set", "an94_mp")));
    }

    #[test]
    fn save_names_a_blank_class_and_writes_the_other_sets_independently() {
        let _g = lock();
        let mut p = Profile::new();
        p.save(class("", "an94_mp"));
        assert_eq!(p.customs()[0].name, "Custom 1", "BO2's untouched slot name (`a-chooseclass/002`)");
        p.set_active(2);
        p.save(class("Third Set", "an94_mp"));
        assert_eq!(p.customs_in(0).unwrap().len(), 1, "set 1 kept its class");
        assert_eq!(p.customs_in(2).unwrap().len(), 1, "set 3 got the new one");
        assert!(p.customs_in(1).unwrap().is_empty());
    }

    #[test]
    fn set_management_renames_resets_and_moves_the_active_set() {
        let _g = lock();
        let mut p = Profile::new();
        p.save(class("Rifle", "an94_mp"));
        assert!(p.rename_set(0, "  My Set  "));
        assert_eq!(p.set_name(0), "My Set");
        assert!(!p.rename_set(MAX_SETS, "nope"));
        // A blank rename falls back to the default (BO2's set names are always shown).
        p.rename_set(0, "   ");
        assert_eq!(p.set_name(0), "CLASS SET 1");
        // Reset empties the slots but keeps the name.
        p.rename_set(1, "Spare");
        p.set_active(1);
        p.save(class("Spare Class", "an94_mp"));
        assert!(p.reset_set(1));
        assert!(p.customs_in(1).unwrap().is_empty());
        assert_eq!(p.set_name(1), "Spare");
        assert!(!p.reset_set(MAX_SETS));
        // The active set moves and wraps through all ten.
        assert!(p.set_active(9));
        assert_eq!(p.active_set(), 9);
        assert!(p.cycle_active(1));
        assert_eq!(p.active_set(), 0, "past the last set wraps to the first");
        assert!(p.cycle_active(-1));
        assert_eq!(p.active_set(), 9);
        assert!(!p.set_active(MAX_SETS));
        // A hand-edited out-of-range active set clamps instead of panicking.
        let mut p = Profile::new();
        p.active_set = 99;
        assert_eq!(p.active_set(), MAX_SETS - 1);
        assert!(p.customs().is_empty());
    }

    #[test]
    fn round_trips_through_the_text_with_sets_and_names() {
        let _g = lock();
        let mut p = Profile::new();
        p.save(class("My Rifle", "an94_mp+reflex"));
        p.save(CustomClass {
            name: "Support".into(),
            primary: "an94_mp".into(),
            perks: vec!["FlakJacket".into()],
            ..CustomClass::default()
        });
        p.rename_set(1, "SNIPERS");
        p.set_active(1);
        p.save(class("Big Gun", "tar21_mp"));
        let text = p.to_text();
        let back = Profile::from_text(&text);
        assert_eq!(back, p);
        // Every class's spec round-trips too.
        assert_eq!(back.customs()[0].primary, "tar21_mp");
        assert_eq!(back.set_name(1), "SNIPERS");
        assert_eq!(back.active_set(), 1);
        // Every set is written, named, so a hand edit can see them.
        assert!(text.contains("set 10 CLASS SET 10\n"));
    }

    #[test]
    fn a_v1_profile_migrates_into_sets_one_and_two() {
        let _g = lock();
        // Seven v1 records: the old file was one flat list of up to ten classes.
        let mut text = String::new();
        for i in 0..7 {
            text.push_str(&format!("Class {i}\nprimary=an94_mp\n\n"));
        }
        let p = Profile::from_text(&text);
        assert_eq!(p.active_set(), 0, "a migrated profile loads into the first set");
        assert_eq!(p.customs_in(0).unwrap().len(), 5, "the first five stay in set 1");
        assert_eq!(p.customs_in(1).unwrap().len(), 2, "the rest land in set 2");
        assert_eq!(p.customs_in(0).unwrap()[0].name, "Class 0");
        assert_eq!(p.customs_in(1).unwrap()[1].name, "Class 6");
        // The migrated text is v2 from then on, and round-trips.
        let v2 = p.to_text();
        assert!(v2.starts_with("v2\n"));
        assert_eq!(Profile::from_text(&v2), p);
        // A v1 record without a spec half is still skipped; a BOM is stripped.
        let p = Profile::from_text("\u{feff}Good\nprimary=an94_mp\n\nBroken without a spec\n\n");
        assert_eq!(p.customs().len(), 1);
        assert_eq!(p.customs()[0].name, "Good");
        // A v1 class called "v2" is not mistaken for a version marker and lost.
        let p = Profile::from_text("v2\nprimary=fnp45_mp\n");
        assert_eq!(p.customs().len(), 1);
        assert_eq!(p.customs()[0].name, "v2");
        assert_eq!(p.customs()[0].primary, "fnp45_mp");
    }

    #[test]
    fn from_text_is_tolerant_of_junk_and_extra_slots() {
        let text = "\u{feff}v2\nactive 2\n\
                    set 1 One\nclass\nA\nprimary=an94_mp\nend\n\
                    class\n\nend\n\
                    set 2 Two\nclass\nB\nprimary=tar21_mp\nend\nclass\nC\nprimary=fnp45_mp\nend\n\
                    class\nD\nprimary=an94_mp\nend\nclass\nE\nprimary=an94_mp\nend\nclass\nF\nprimary=an94_mp\nend\n\
                    class\nG\nprimary=an94_mp\nend\n\
                    set 99 Not a set\nnonsense line\n";
        let p = Profile::from_text(text);
        assert_eq!(p.active_set(), 1, "active 2");
        assert_eq!(p.set_name(0), "One");
        assert_eq!(p.set_name(1), "Two");
        // The empty `class`/``/`` record is skipped; a sixth class in set 2 is dropped.
        assert_eq!(p.customs_in(0).unwrap().len(), 1);
        assert_eq!(p.customs_in(1).unwrap().len(), 5);
        assert!(p.sets.iter().enumerate().all(|(i, s)| !s.name_or_default(i).is_empty()), "no set is nameless");
    }

    #[test]
    fn loading_repairs_an_invalid_stored_class_instead_of_dropping_it() {
        let _g = lock();
        // A class whose weapon is not in the catalog: the class is kept, its weapon cleared.
        let text = "v2\nactive 1\nset 1 CLASS SET 1\nclass\nBad Gun\nprimary=not_a_weapon_mp+reflex\nend\n";
        let p = Profile::from_text(text);
        assert_eq!(p.customs().len(), 1, "the class survived");
        assert_eq!(p.customs()[0].name, "Bad Gun");
        assert!(p.customs()[0].primary.is_empty(), "the unknown weapon was cleared");
        // An over-budget class is trimmed to the allocation, not dropped.
        let mut c = class("Fat", "an94_mp+gl+reflex");
        c.perks = vec!["Lightweight".into(), "Hardline".into(), "Toughness".into(), "Scavenger".into(), "Dexterity".into()];
        c.secondary = "tar21_mp".into();
        let mut p = Profile::new();
        p.save(c);
        let text = p.to_text();
        let back = Profile::from_text(&text);
        assert_eq!(back.customs().len(), 1);
        let lifted = crate::classmenu::Draft::from_custom(&back.customs()[0]);
        assert!(lifted.spent() <= crate::classmenu::PICK_10, "repaired to {} points", lifted.spent());
    }

    #[test]
    fn remove_and_copy_work_inside_the_active_set() {
        let _g = lock();
        let mut p = Profile::new();
        p.save(class("Rifle", "an94_mp"));
        p.save(class("Sniper", "tar21_mp"));
        assert!(p.remove("rifle"), "case-insensitive");
        assert_eq!(p.customs().len(), 1);
        assert_eq!(p.customs()[0].name, "Sniper");
        assert!(!p.remove("Rifle"), "already gone");
        // Copy names are unique inside the set and capped at five slots.
        assert!(p.save(class("Rifle", "an94_mp")));
        let first = p.copy("Rifle").expect("room for a copy");
        assert_eq!(first, "Rifle COPY");
        let second = p.copy("Rifle").expect("room for another");
        assert_eq!(second, "Rifle COPY 2");
        assert!(p.copy("Missing").is_none());
        assert_eq!(p.customs().len(), 4);
        assert!(p.save(class("Filler", "an94_mp")));
        assert!(p.copy("Rifle").is_none(), "a full set refuses the copy");
        // Copies go into the set being browsed, not into a global list.
        p.set_active(4);
        assert!(p.copy("Rifle").is_none(), "the other set has no Rifle");
        assert!(p.save(class("Rifle", "fnp45_mp")));
        assert_eq!(p.copy_in_slot(0).as_deref(), Some("Rifle COPY"));
        assert_eq!(p.customs().len(), 2);
        assert_eq!(p.customs_in(0).unwrap().len(), 5, "set 1 is untouched");
    }

    #[test]
    fn text_words_round_trip_the_whole_profile() {
        let mut p = Profile::new();
        p.save(class("My Rifle", "an94_mp+reflex"));
        p.save(class("Support", "fnp45_mp"));
        let text = p.to_text();
        let words = pack_text(&text);
        assert_eq!(unpack_text(&words).as_deref(), Some(text.as_str()));
        assert_eq!(Profile::from_text(&unpack_text(&words).unwrap()), p);
        // A trailing NUL (padding) stops the decode; corrupt bytes decode to None.
        assert_eq!(unpack_text(&[0]).as_deref(), Some(""));
        assert_eq!(unpack_text(&[]).as_deref(), Some(""), "an empty profile is zero words");
        assert!(unpack_text(&[0x00FF_FFFF]).is_none());
    }
}
