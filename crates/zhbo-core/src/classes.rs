//! The class registry (BO2 Create-a-Class): named loadout presets the General can assume. The
//! five basic BO2 classes are built in; `Data/ConquerOps/classes.ini` (shipped in the mod's BIG
//! file, like weapons.ini — every player must have the identical file, so an index sent over the
//! lockstep message stream means the same kit everywhere) overrides and extends the list. Custom
//! classes live in the player's own profile (local; see `client::menu`), picked by sending their
//! spec through the choose-class message.
/*
** Conquer Ops - Zero Hour gameplay core: the class registry (Create-a-Class presets).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use std::sync::Mutex;

/// One pickable class: a display name and the kit spec (`Loadout::with_spec`) it applies.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassDef {
    /// Unique key (`assault`). Basic classes keep theirs so shipped files can extend the list.
    pub id: String,
    /// What the pick menu and the HUD show (`Assault`).
    pub name: String,
    /// The kit spec: every slot the spec sets replaces the base kit's (see `loadout_of`).
    pub spec: String,
}

/// The five basic BO2 classes as `(id, name, spec)` triples: the defaults when no file is loaded
/// (tests, the fallback). Complete specs, so an INI file must set every slot it wants different
/// from the base kit.
pub const BASIC: &[(&str, &str, &str)] = &[
    ("assault", "Assault", "primary=an94_mp+gl+reflex;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity"),
    ("marksman", "Marksman", "primary=dsr50_mp;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=ExtremeConditioning,FastHands,Dexterity"),
    ("pointman", "Pointman", "primary=an94_mp+reflex;secondary=fnp45_mp;lethal=claymore_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,Scavenger,Dexterity"),
    ("breacher", "Breacher", "primary=an94_mp+reflex;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=FlakJacket,Toughness,Dexterity"),
    ("engineer", "Engineer", "primary=an94_mp+reflex;secondary=fnp45_mp;lethal=claymore_mp;tactical=trophy_system_mp;melee=knife_held_mp;perks=Engineer,Lightweight,Dexterity"),
];

/// The basic triples as `ClassDef`s (each access builds fresh ones; tests mutate nothing).
pub fn basic() -> Vec<ClassDef> {
    BASIC.iter().map(|(id, name, spec)| ClassDef { id: (*id).into(), name: (*name).into(), spec: (*spec).into() }).collect()
}

/// The classes loaded at init; the basics until `load` runs.
static CLASSES: Mutex<Option<Vec<ClassDef>>> = Mutex::new(None);

/// Load the class list (the bridge passes `data/ConquerOps/classes.ini`'s text at init). The
/// list must be identical on every client: the file ships in the mod's BIG file. Later loads
/// replace the list (a reset may reload the mod's BIG file).
pub fn load(text: &str) {
    let list = parse(text);
    *CLASSES.lock().unwrap_or_else(|e| e.into_inner()) = Some(list);
}

/// Forget the loaded list, back to the built-in basics (tests: one test's load must not leak
/// into another's).
#[cfg(test)]
pub fn reset() {
    *CLASSES.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Parse `Class <id>` blocks: `Name =` and `Spec =` fields between `Class` and `End`, `;`-comments
/// like the rest of the mod's data files. A block with an empty `Spec` or a duplicate id is
/// skipped (a broken block must not lose the whole list).
pub fn parse(text: &str) -> Vec<ClassDef> {
    let mut out: Vec<ClassDef> = Vec::new();
    let mut current: Option<ClassDef> = None;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("Class ") {
            let id = rest.trim().to_ascii_lowercase();
            if !id.is_empty() {
                current = Some(ClassDef { id, name: String::new(), spec: String::new() });
            }
        } else if line.eq_ignore_ascii_case("End") {
            if let Some(c) = current.take() {
                if !c.spec.is_empty() && !out.iter().any(|d| d.id == c.id) {
                    out.push(c);
                }
            }
        } else if let Some(c) = current.as_mut() {
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim().to_string();
                match key.trim().to_ascii_lowercase().as_str() {
                    "name" => c.name = value,
                    "spec" => c.spec = value,
                    _ => {}
                }
            }
        }
    }
    // Unterminated last block (missing `End`): keep it if it is complete.
    if let Some(c) = current {
        if !c.spec.is_empty() && !out.iter().any(|d| d.id == c.id) {
            out.push(c);
        }
    }
    out
}

/// The class list in force: the five basics before init (tests, the shell).
pub fn get() -> Vec<ClassDef> {
    CLASSES.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(basic)
}

/// The class with this id (`assault`), from the list in force.
pub fn by_id(id: &str) -> Option<ClassDef> {
    get().into_iter().find(|c| c.id == id)
}

/// The class at this list index (a choose-class message carries the index; the list must be
/// identical on every client, which the shipped file guarantees).
pub fn at(index: usize) -> Option<ClassDef> {
    get().get(index).cloned()
}

/// The class's kit: the spec over the base kit (`Loadout::default()`; a complete spec replaces
/// every slot, which the basic classes carry and the shipped file is written to).
pub fn loadout_of(c: &ClassDef) -> crate::weapons::Loadout {
    crate::weapons::Loadout::default().with_spec(&c.spec)
}

/// A player-authored Create-a-Class entry. Unlike [`ClassDef`] (the shipped registry, known to
/// every client ahead of time), a custom class exists only on its author's client until the pick
/// message carries its spec to the sim. The fields map 1:1 to the kit's slots so the
/// create-a-class menu can edit each one directly; `to_spec`/`from_spec` round-trip through the
/// same `Loadout::with_spec` grammar the shipped registry uses, and `to_wire`/`from_wire` pack the
/// whole entry into a compact string for the pick message and the profile file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomClass {
    /// What the menu and the HUD show (the player's own naming, `My Sniper`).
    pub name: String,
    /// Primary slot: the catalog id plus `+`-joined attachment shorts (`an94_mp+gl+reflex`).
    pub primary: String,
    /// Secondary slot (`fnp45_mp`).
    pub secondary: String,
    /// Lethal offhand (`frag_grenade_mp`).
    pub lethal: String,
    /// Tactical offhand (`concussion_grenade_mp`).
    pub tactical: String,
    /// Melee slot (`knife_held_mp`).
    pub melee: String,
    /// The perk spec ids in pick order (`Lightweight`, `FastHands`, `Dexterity`).
    pub perks: Vec<String>,
    /// Spare rounds for the primary/secondary, overriding BO2's defaults (0 = unset).
    pub reserve: [u16; 2],
}

impl CustomClass {
    /// The kit this entry applies: the spec over the base kit, exactly like a shipped class.
    pub fn loadout(&self) -> crate::weapons::Loadout {
        crate::weapons::Loadout::default().with_spec(&self.to_spec())
    }

    /// The spec string in `Loadout::with_spec`'s grammar: every slot the entry sets, `;`-joined.
    /// Empty weapon slots are skipped, so they leave the base kit's slot in place (a Create-a-Class
    /// entry inherits what the player didn't touch). The round-trip through
    /// [`CustomClass::from_spec`] preserves every set field.
    pub fn to_spec(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut push = |key: &str, value: &str| {
            if !value.is_empty() {
                parts.push(format!("{key}={value}"));
            }
        };
        push("primary", &self.primary);
        push("secondary", &self.secondary);
        push("lethal", &self.lethal);
        push("tactical", &self.tactical);
        push("melee", &self.melee);
        push("perks", &self.perks.join(","));
        if self.reserve[0] > 0 {
            push("primary_reserve", &self.reserve[0].to_string());
        }
        if self.reserve[1] > 0 {
            push("secondary_reserve", &self.reserve[1].to_string());
        }
        parts.join(";")
    }

    /// Parse a spec string back into an entry (the same grammar [`crate::weapons::Loadout::with_spec`]
    /// reads; keys it doesn't know are ignored). The name is not part of a spec: set it after.
    pub fn from_spec(spec: &str) -> CustomClass {
        let mut c = CustomClass::default();
        for part in spec.split(';') {
            let Some((key, value)) = part.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "primary" => c.primary = value.into(),
                "secondary" => c.secondary = value.into(),
                "lethal" => c.lethal = value.into(),
                "tactical" => c.tactical = value.into(),
                "melee" => c.melee = value.into(),
                "perks" => c.perks = value.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
                "primary_reserve" => c.reserve[0] = value.parse().unwrap_or(0),
                "secondary_reserve" => c.reserve[1] = value.parse().unwrap_or(0),
                _ => {}
            }
        }
        c
    }

    /// Wire format for the pick message and the profile file: `name\nspec`, where `name` may not
    /// contain a newline (the menu trims it). The sim receives the spec half through its
    /// choose-class path, and the profile round-trips the whole line.
    pub fn to_wire(&self) -> String {
        let name = self.name.replace('\n', " ");
        format!("{name}\n{}", self.to_spec())
    }

    /// Undo [`CustomClass::to_wire`]; `None` if the payload has no spec half.
    pub fn from_wire(wire: &str) -> Option<CustomClass> {
        let (name, spec) = wire.split_once('\n')?;
        Some(CustomClass { name: name.to_string(), ..CustomClass::from_spec(spec) })
    }

    /// The pick message's payload: the wire format's UTF-8 bytes packed little-endian, four per
    /// `u32`. The engine's `GameMessage` stream carries only integers, so the bytes ride as words;
    /// the bridge appends them in order and the sim reassembles them ([`CustomClass::from_wire_words`]).
    pub fn to_wire_words(&self) -> Vec<u32> {
        let wire = self.to_wire();
        let bytes = wire.as_bytes();
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

    /// Undo [`CustomClass::to_wire_words`]; `None` if the words don't reassemble into valid UTF-8
    /// with a spec half (a malformed or truncated payload).
    pub fn from_wire_words(words: &[u32]) -> Option<CustomClass> {
        let mut bytes = Vec::with_capacity(words.len() * 4);
        for w in words {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        // A trailing NUL is possible if the payload was padded; stop at the first NUL inside the
        // last word, since the words themselves carry no length.
        let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        let wire = String::from_utf8(bytes[..len].to_vec()).ok()?;
        CustomClass::from_wire(&wire)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// The global list is shared; tests that touch it run one at a time.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn the_five_basics_are_the_defaults() {
        let _g = lock();
        reset();
        let list = get();
        assert_eq!(list.len(), 5);
        assert_eq!(list[0].id, "assault");
        assert_eq!(list[4].id, "engineer");
        // Every basic class carries a complete kit: every slot set over the base kit.
        for c in &list {
            let l = loadout_of(c);
            assert!(!l.primary.is_empty() && !l.secondary.is_empty() && !l.melee.is_empty());
            assert!(!l.lethal.is_empty() && !l.tactical.is_empty());
            assert!(!l.perks.is_empty());
        }
        // The assault class is the default loadout's kit.
        let default = crate::weapons::Loadout::default();
        assert_eq!(loadout_of(&list[0]), default, "assault: the base kit");
    }

    #[test]
    fn parses_the_classes_file() {
        let _g = lock();
        // An empty file replaces the list with nothing: the shipped file always carries the
        // five basics, so the built-in defaults below are only for tests and a missing file.
        load("");
        assert!(get().is_empty());
        let text = "; classes.ini\nClass assault\n  Name = Infantry\n  Spec = primary=dsr50_mp\nEnd\n\nClass sniper\n  Name = Sniper\n  Spec = primary=an94_mp\nEnd\n";
        load(text);
        let list = get();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], ClassDef { id: "assault".into(), name: "Infantry".into(), spec: "primary=dsr50_mp".into() });
        assert_eq!(by_id("sniper").unwrap().name, "Sniper");
        assert!(by_id("marksman").is_none(), "the file replaces the list");
        // Restore for the other tests.
        reset();
    }

    #[test]
    fn broken_blocks_are_skipped() {
        let text = "; no spec\nClass broken\n  Name = Broken\nEnd\nClass good\nName = Good\nSpec = primary=an94_mp\nEnd\n; duplicate id\nClass good\nName = Again\nSpec = primary=fnp45_mp\nEnd\n";
        let list = parse(text);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "good");
        assert_eq!(list[0].name, "Good");
    }

    #[test]
    fn unterminated_block_is_kept_when_complete() {
        let list = parse("Class last\nName = Last\nSpec = primary=an94_mp\n");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "last");
    }

    #[test]
    fn list_index_round_trips() {
        let _g = lock();
        let list = get();
        for (i, c) in list.iter().enumerate() {
            assert_eq!(at(i).as_ref(), Some(c));
        }
        assert!(at(5).is_none());
    }

    #[test]
    fn custom_class_round_trips_through_the_spec_grammar() {
        let c = CustomClass {
            name: "My Sniper".into(),
            primary: "dsr50_mp+acog".into(),
            secondary: "fnp45_mp".into(),
            lethal: "claymore_mp".into(),
            tactical: String::new(),
            melee: "knife_held_mp".into(),
            perks: vec!["Lightweight".into(), "Dexterity".into()],
            reserve: [60, 0],
        };
        let spec = c.to_spec();
        // The empty tactical is skipped (it inherits the base kit's slot).
        assert_eq!(spec, "primary=dsr50_mp+acog;secondary=fnp45_mp;lethal=claymore_mp;melee=knife_held_mp;perks=Lightweight,Dexterity;primary_reserve=60");
        // Round-tripping through the spec grammar preserves every set field (the name is not
        // part of a spec, so the parsed entry is nameless).
        assert_eq!(CustomClass::from_spec(&spec), CustomClass { name: String::new(), ..c.clone() });
        // The wire format is name + spec, and round-trips too.
        let wire = c.to_wire();
        assert_eq!(wire, "My Sniper\nprimary=dsr50_mp+acog;secondary=fnp45_mp;lethal=claymore_mp;melee=knife_held_mp;perks=Lightweight,Dexterity;primary_reserve=60");
        assert_eq!(CustomClass::from_wire(&wire), Some(c));
        // A newline in the name would break the wire framing; `to_wire` replaces it.
        let named = CustomClass { name: "bad\nname".into(), ..CustomClass::default() };
        assert!(!named.to_wire().contains("bad\nname"));
    }

    #[test]
    fn custom_class_loadout_applies_over_the_base_kit() {
        // The entry sets the primary and the perks; the other slots stay the base kit's
        // (`Loadout::default()`'s an94+gl, the Tac-45, the frag, the concussion, the knife,
        // and the reserve from `Loadout::default()`).
        let c = CustomClass {
            name: "Shotgun".into(),
            primary: "m1216_mp".into(),
            perks: vec!["FlakJacket".into()],
            ..CustomClass::default()
        };
        let base = crate::weapons::Loadout::default();
        let l = c.loadout();
        assert_eq!(l.primary, "m1216_mp");
        assert_eq!(l.secondary, base.secondary);
        assert_eq!(l.lethal, base.lethal);
        assert_eq!(l.tactical, base.tactical);
        assert_eq!(l.melee, base.melee);
        assert_eq!(l.perks, vec![crate::weapons::Perk::by_id("FlakJacket").unwrap()]);
        // Unknown perk ids are dropped (the spec grammar's parser ignores them).
        let c2 = CustomClass { perks: vec!["NotAPerk".into()], ..CustomClass::default() };
        assert!(c2.loadout().perks.is_empty());
    }

    #[test]
    fn from_wire_rejects_a_payload_without_a_spec() {
        assert!(CustomClass::from_wire("Just a name").is_none());
        assert!(CustomClass::from_wire("").is_none());
        // The spec half alone is fine (an unnamed class).
        assert_eq!(CustomClass::from_wire("\nprimary=an94_mp").unwrap().primary, "an94_mp");
    }
}
