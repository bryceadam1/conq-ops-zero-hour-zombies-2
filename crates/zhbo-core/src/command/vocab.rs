//! The words a match's commands are made of (docs/COMMANDS.md section 5): the run-time lists
//! (types, classes, upgrades, powers, sciences, place names, team names) and the verbs with the
//! arguments each takes.
//!
//! The lists are filled from data by the caller ([`super::world::World::vocabulary`]: the objects'
//! display names and the names their command buttons give). The verbs are the fixed table below:
//! the right-click verbs (each a way `CommandTranslator::evaluateContextCommand` can act), the
//! orders bound to hotkeys, and the verbs of the command buttons (`build`, `upgrade`, `use`, …),
//! whose things come from the buttons.

use super::ast::{Adjective, Direction, Region, Relation, Superlative};

/// A noun's two forms (`crusader tank` / `crusader tanks`). A mass noun (`infantry`, `aircraft`)
/// has the same word for both: it is singular only after `the` or with a superlative, and plural
/// otherwise (section 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Noun {
    pub singular: String,
    pub plural: String,
}

impl Noun {
    pub fn new(singular: &str, plural: &str) -> Self {
        Noun { singular: singular.to_string(), plural: plural.to_string() }
    }

    pub fn is_mass(&self) -> bool {
        self.singular == self.plural
    }
}

/// Which list a verb's thing comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThingList {
    /// Thing templates (`build power plant`), with a count when the verb allows it.
    Types,
    Upgrades,
    Powers,
    Sciences,
    /// Weapon modes (`SWITCH_WEAPON` buttons' labels).
    Modes,
    /// Weapons fired by button (`FIRE_WEAPON` buttons' labels).
    Weapons,
    /// What a producer can have queued: a type or an upgrade (`cancel`).
    Production,
}

/// What a verb's target may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetSlot {
    /// An object (`attack the enemy war factory`, `guard me`).
    pub object: bool,
    /// A point or a place (`move to the bridge`, `force attack there`). An object named here
    /// stands for its position.
    pub point: bool,
    pub required: bool,
    /// `facing <direction>` may follow (verbs that place a building).
    pub facing: bool,
    /// `from <place> to <place>` may stand instead (verbs that build a line, like a wall).
    pub line: bool,
}

impl TargetSlot {
    pub const OBJECT: TargetSlot = TargetSlot { object: true, point: false, required: true, facing: false, line: false };
    pub const POINT: TargetSlot = TargetSlot { object: false, point: true, required: true, facing: false, line: false };
    pub const EITHER: TargetSlot = TargetSlot { object: true, point: true, required: true, facing: false, line: false };
}

/// One verb: its words, the preposition before its target, and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerbDef {
    /// The verb's words, the key in [`super::ast::Order::Verb`] (`move`, `set rally point`).
    pub words: String,
    /// The preposition before a target (`to`, `at`, `on`). It is left out before `there` and
    /// before an offset (`move there`, `move north of the bridge`).
    pub prep: Option<String>,
    /// `Some((list, counted))`: the verb names a thing from `list`, with a count if `counted`.
    pub thing: Option<(ThingList, bool)>,
    pub target: Option<TargetSlot>,
}

impl VerbDef {
    fn new(words: &str, prep: Option<&str>, thing: Option<(ThingList, bool)>, target: Option<TargetSlot>) -> Self {
        VerbDef { words: words.to_string(), prep: prep.map(str::to_string), thing, target }
    }
}

/// The verbs of the language (docs/COMMANDS.md section 7).
pub fn fixed_verbs() -> Vec<VerbDef> {
    use TargetSlot as T;
    let optional_either = TargetSlot { required: false, ..T::EITHER };
    let optional_point = TargetSlot { required: false, ..T::POINT };
    let placement = TargetSlot { object: false, point: true, required: false, facing: true, line: true };
    let none = |w: &str| VerbDef::new(w, None, None, None);
    let on = |w: &str, prep: Option<&str>, slot: TargetSlot| VerbDef::new(w, prep, None, Some(slot));
    let thing = |w: &str, prep: Option<&str>, list: ThingList, counted: bool, slot: Option<TargetSlot>| VerbDef::new(w, prep, Some((list, counted)), slot);
    vec![
        // Orders with no target: hotkeys (`MSG_META_SCATTER`, `MSG_META_CREATE_FORMATION`,
        // `MSG_META_ALL_CHEER`, `MSG_META_REMOVE_BEACON`) and command buttons.
        none("stop"),
        none("scatter"),
        none("form up"),
        none("cheer"),
        none("sell"),
        none("overcharge"),
        none("hack internet"),
        none("depart"),
        none("cancel construction"),
        none("remove beacon"),
        on("evacuate", Some("at"), optional_point),
        // Movement and attack.
        on("move", Some("to"), T::POINT),
        on("force move", Some("to"), T::POINT),
        on("attack-move", Some("to"), T::POINT),
        on("attack", None, T::OBJECT),
        on("force attack", None, T::EITHER),
        on("guard", None, optional_either),
        on("guard without pursuit", None, optional_either),
        on("guard against aircraft", None, optional_either),
        // The right-click, and each way `CommandTranslator::evaluateContextCommand` can act on a
        // target.
        // `act on` a spot is the right-click on open ground (a move, a rally point, a special
        // power's new destination).
        on("act on", None, T::EITHER),
        on("enter", None, T::OBJECT),
        on("garrison", None, T::OBJECT),
        on("capture", None, T::OBJECT),
        on("repair", None, T::OBJECT),
        on("resume building", None, T::OBJECT),
        on("get repaired", Some("at"), T::OBJECT),
        on("get healed", Some("at"), T::OBJECT),
        on("dock", Some("at"), T::OBJECT),
        on("deliver supplies", Some("to"), T::OBJECT),
        on("hijack", None, T::OBJECT),
        on("car-bomb", None, T::OBJECT),
        on("sabotage", None, T::OBJECT),
        on("salvage", None, T::OBJECT),
        on("disable", None, T::OBJECT),
        on("steal cash", Some("from"), T::OBJECT),
        // Points the overhead view clicks.
        on("set rally point", Some("at"), T::POINT),
        on("place beacon", Some("at"), T::POINT),
        on("combat drop", Some("on"), T::EITHER),
        // The command buttons with things.
        thing("build", Some("at"), ThingList::Types, true, Some(placement)),
        thing("upgrade", None, ThingList::Upgrades, false, None),
        thing("use", Some("at"), ThingList::Powers, false, Some(optional_either)),
        thing("purchase", None, ThingList::Sciences, false, None),
        thing("cancel", None, ThingList::Production, false, None),
        thing("unload", None, ThingList::Types, true, None),
        thing("switch to", None, ThingList::Modes, false, None),
        thing("fire", Some("at"), ThingList::Weapons, false, Some(optional_either)),
    ]
}

/// Every word list of one match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vocabulary {
    /// Thing templates by display name, the player's own and others' alike.
    pub types: Vec<Noun>,
    /// Class words (KindOf).
    pub classes: Vec<Noun>,
    pub upgrades: Vec<String>,
    pub powers: Vec<String>,
    pub sciences: Vec<String>,
    pub modes: Vec<String>,
    pub weapons: Vec<String>,
    /// The map's place names.
    pub places: Vec<String>,
    /// Team names saved this match.
    pub teams: Vec<String>,
    pub verbs: Vec<VerbDef>,
}

impl Default for Vocabulary {
    fn default() -> Self {
        Vocabulary {
            types: Vec::new(),
            classes: Vec::new(),
            upgrades: Vec::new(),
            powers: Vec::new(),
            sciences: Vec::new(),
            modes: Vec::new(),
            weapons: Vec::new(),
            places: Vec::new(),
            teams: Vec::new(),
            verbs: fixed_verbs(),
        }
    }
}

/// The grammar's own words, which no run-time name may equal.
pub fn fixed_words() -> Vec<&'static str> {
    let mut w = vec![
        "then", "and", "except", "the", "my", "half", "group", "them", "that", "me", "there", "near",
        "in", "of", "behind", "front", "facing", "from", "name", "make", "units", "unit",
    ];
    w.extend(Superlative::ALL.iter().map(|s| s.word()));
    w.extend(Adjective::ALL.iter().map(|a| a.word()));
    w.extend(Relation::WORDS.iter().filter_map(|r| r.word()));
    w.extend(Direction::ALL.iter().map(|d| d.word()));
    w.extend(Region::ALL.iter().map(|r| r.word()));
    w
}

impl Vocabulary {
    /// The names as one line, for the voice decoder in its own process (docs/COMMANDS.md section
    /// 11) and the training set's `vocab.tsv`: the types (`singular>plural`), then the places,
    /// teams, upgrades, powers, sciences, weapon modes and weapons, each `|`-separated, tab between
    /// lists, and last the class words (`singular>plural`), which a match narrows. The verbs are
    /// always the grammar's own.
    pub fn to_line(&self) -> String {
        let nouns = |list: &[Noun]| list.iter().map(|n| format!("{}>{}", n.singular, n.plural)).collect::<Vec<_>>().join("|");
        [
            nouns(&self.types),
            self.places.join("|"),
            self.teams.join("|"),
            self.upgrades.join("|"),
            self.powers.join("|"),
            self.sciences.join("|"),
            self.modes.join("|"),
            self.weapons.join("|"),
            nouns(&self.classes),
        ]
        .join("\t")
    }

    /// A vocabulary from [`Vocabulary::to_line`]'s line. Without the class list (the training
    /// set's lines), all the grammar's classes.
    pub fn from_line(line: &str) -> Option<Vocabulary> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            return None;
        }
        let list = |s: &str| s.split('|').filter(|x| !x.is_empty()).map(str::to_string).collect::<Vec<_>>();
        let nouns = |s: &str| s.split('|').filter_map(|t| t.split_once('>')).map(|(s, p)| Noun::new(s, p)).collect::<Vec<_>>();
        Some(Vocabulary {
            types: nouns(f[0]),
            places: list(f[1]),
            teams: list(f[2]),
            upgrades: list(f[3]),
            powers: list(f[4]),
            sciences: list(f[5]),
            modes: list(f[6]),
            weapons: list(f[7]),
            classes: match f.get(8) {
                Some(c) => nouns(c),
                None => super::world::CLASSES.iter().map(|(s, p, _)| Noun::new(s, p)).collect(),
            },
            ..Vocabulary::default()
        })
    }

    pub fn verb(&self, words: &str) -> Option<&VerbDef> {
        self.verbs.iter().find(|v| v.words == words)
    }

    pub fn things(&self, list: ThingList) -> Vec<&str> {
        match list {
            ThingList::Types => self.types.iter().map(|n| n.singular.as_str()).collect(),
            ThingList::Upgrades => self.upgrades.iter().map(String::as_str).collect(),
            ThingList::Powers => self.powers.iter().map(String::as_str).collect(),
            ThingList::Sciences => self.sciences.iter().map(String::as_str).collect(),
            ThingList::Modes => self.modes.iter().map(String::as_str).collect(),
            ThingList::Weapons => self.weapons.iter().map(String::as_str).collect(),
            ThingList::Production => self.types.iter().map(|n| n.singular.as_str()).chain(self.upgrades.iter().map(String::as_str)).collect(),
        }
    }

    /// The vocabulary's problems, empty when commands over it parse unambiguously: a name that is
    /// a fixed word or a verb, a name with `then` in it (it would split the command), a name in two
    /// lists or twice in one, a word that is not lowercase letters, digits, `-` or `'`.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let fixed = fixed_words();
        let verbs: Vec<&str> = self.verbs.iter().map(|v| v.words.as_str()).collect();
        let mut names: Vec<(&str, &str)> = Vec::new();
        for n in &self.types {
            names.push(("type", &n.singular));
            if !n.is_mass() {
                names.push(("type", &n.plural));
            }
        }
        for n in &self.classes {
            names.push(("class", &n.singular));
            if !n.is_mass() {
                names.push(("class", &n.plural));
            }
        }
        for (list, v) in [
            ("upgrade", &self.upgrades),
            ("power", &self.powers),
            ("science", &self.sciences),
            ("mode", &self.modes),
            ("weapon", &self.weapons),
            ("place", &self.places),
            ("team", &self.teams),
        ] {
            names.extend(v.iter().map(|s| (list, s.as_str())));
        }
        for (i, &(list, name)) in names.iter().enumerate() {
            if name.split_whitespace().next().is_none() {
                out.push(format!("an empty {list} name"));
                continue;
            }
            if let Some(bad) = name.split_whitespace().find(|w| !is_word(w)) {
                out.push(format!("{list} '{name}': '{bad}' is not lowercase letters, digits, '-' or '''"));
            }
            if fixed.contains(&name) || verbs.contains(&name) {
                out.push(format!("{list} '{name}' is a word of the grammar"));
            }
            if name.split_whitespace().any(|w| w == "then") {
                out.push(format!("{list} '{name}' contains 'then'"));
            }
            // Two lists clash only where the parser reads them in the same place: the noun lists
            // (types, classes, places, teams) together, and types with upgrades (`cancel` takes
            // either). A power, science, weapon mode or weapon is only read after its own verb, so
            // it may share a unit's name (the Scud Storm is a building and a power).
            const NOUNS: [&str; 4] = ["type", "class", "place", "team"];
            let clash = |a: &str, b: &str| a == b || (NOUNS.contains(&a) && NOUNS.contains(&b)) || matches!((a, b), ("type", "upgrade") | ("upgrade", "type"));
            match names[..i].iter().find(|&&(other, n)| n == name && clash(other, list)) {
                Some(&(other, _)) if other == list => out.push(format!("{list} '{name}' is listed twice")),
                Some(&(other, _)) => out.push(format!("'{name}' is both a {other} and a {list}")),
                None => {}
            }
        }
        out
    }
}

/// A canonical word: lowercase ASCII letters, digits, `-` and `'`.
pub fn is_word(w: &str) -> bool {
    !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'\'')
}
