//! The words a match's commands are made of (docs/COMMANDS.md section 5): the run-time lists
//! (types, classes, upgrades, powers, sciences, place names, team names) and the verbs with the
//! arguments each takes.
//!
//! The lists are filled from data by the caller (the world query, a later step). The verbs start as
//! the fixed table below: the context verbs (one per `ActionManager::can*` query), the orders bound
//! to hotkeys and the verbs with their own shapes (`build`, `use`, …). The verbs built from each
//! object's command set extend it in a later step.

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
}

impl TargetSlot {
    pub const OBJECT: TargetSlot = TargetSlot { object: true, point: false, required: true, facing: false };
    pub const POINT: TargetSlot = TargetSlot { object: false, point: true, required: true, facing: false };
    pub const EITHER: TargetSlot = TargetSlot { object: true, point: true, required: true, facing: false };
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

/// The verbs the language has before any command-set data is read (docs/COMMANDS.md section 7).
pub fn fixed_verbs() -> Vec<VerbDef> {
    use TargetSlot as T;
    let optional_either = TargetSlot { required: false, ..T::EITHER };
    let placement = TargetSlot { object: false, point: true, required: false, facing: true };
    let mut v = vec![
        // Orders with no target.
        VerbDef::new("stop", None, None, None),
        VerbDef::new("evacuate", None, None, None),
        VerbDef::new("sell", None, None, None),
        VerbDef::new("scatter", None, None, None),
        VerbDef::new("form up", None, None, None),
        VerbDef::new("cheer", None, None, None),
        // Movement and attack.
        VerbDef::new("move", Some("to"), None, Some(T::POINT)),
        VerbDef::new("force move", Some("to"), None, Some(T::POINT)),
        VerbDef::new("attack-move", Some("to"), None, Some(T::POINT)),
        VerbDef::new("attack", None, None, Some(T::OBJECT)),
        VerbDef::new("force attack", None, None, Some(T::EITHER)),
        VerbDef::new("guard", None, None, Some(optional_either)),
        // The right-click, and one verb per ActionManager::can* query.
        VerbDef::new("act", Some("on"), None, Some(T::OBJECT)),
        VerbDef::new("enter", None, None, Some(T::OBJECT)),
        VerbDef::new("garrison", None, None, Some(T::OBJECT)),
        VerbDef::new("capture", None, None, Some(T::OBJECT)),
        VerbDef::new("repair", None, None, Some(T::OBJECT)),
        VerbDef::new("resume building", None, None, Some(T::OBJECT)),
        VerbDef::new("get repaired", Some("at"), None, Some(T::OBJECT)),
        VerbDef::new("get healed", Some("at"), None, Some(T::OBJECT)),
        VerbDef::new("dock", Some("at"), None, Some(T::OBJECT)),
        VerbDef::new("deliver supplies", Some("to"), None, Some(T::OBJECT)),
        VerbDef::new("hijack", None, None, Some(T::OBJECT)),
        VerbDef::new("car-bomb", None, None, Some(T::OBJECT)),
        VerbDef::new("snipe", None, None, Some(T::OBJECT)),
        VerbDef::new("disable", None, None, Some(T::OBJECT)),
        VerbDef::new("steal cash", Some("from"), None, Some(T::OBJECT)),
        VerbDef::new("sabotage", None, None, Some(T::OBJECT)),
        VerbDef::new("cut power", Some("to"), None, Some(T::OBJECT)),
        VerbDef::new("bribe", None, None, Some(T::OBJECT)),
        VerbDef::new("convert", None, None, Some(T::OBJECT)),
        VerbDef::new("pick up", None, None, Some(T::OBJECT)),
        VerbDef::new("salvage", None, None, Some(T::OBJECT)),
        // Points the overhead view clicks.
        VerbDef::new("set rally point", Some("at"), None, Some(T::POINT)),
        VerbDef::new("place beacon", Some("at"), None, Some(T::POINT)),
    ];
    // The verbs with things.
    v.push(VerbDef::new("build", Some("at"), Some((ThingList::Types, true)), Some(placement)));
    v.push(VerbDef::new("upgrade", None, Some((ThingList::Upgrades, false)), None));
    v.push(VerbDef::new("use", Some("at"), Some((ThingList::Powers, false)), Some(optional_either)));
    v.push(VerbDef::new("purchase", None, Some((ThingList::Sciences, false)), None));
    v
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
        "in", "of", "behind", "front", "facing", "name", "make", "units", "unit",
    ];
    w.extend(Superlative::ALL.iter().map(|s| s.word()));
    w.extend(Adjective::ALL.iter().map(|a| a.word()));
    w.extend(Relation::WORDS.iter().filter_map(|r| r.word()));
    w.extend(Direction::ALL.iter().map(|d| d.word()));
    w.extend(Region::ALL.iter().map(|r| r.word()));
    w
}

impl Vocabulary {
    pub fn verb(&self, words: &str) -> Option<&VerbDef> {
        self.verbs.iter().find(|v| v.words == words)
    }

    pub fn things(&self, list: ThingList) -> Vec<&str> {
        match list {
            ThingList::Types => self.types.iter().map(|n| n.singular.as_str()).collect(),
            ThingList::Upgrades => self.upgrades.iter().map(String::as_str).collect(),
            ThingList::Powers => self.powers.iter().map(String::as_str).collect(),
            ThingList::Sciences => self.sciences.iter().map(String::as_str).collect(),
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
            match names[..i].iter().find(|&&(_, n)| n == name) {
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
