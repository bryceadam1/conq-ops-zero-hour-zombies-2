//! Canonical commands drawn from the grammar for the voice model's training set (docs/COMMANDS.md
//! section 11): each over an invented vocabulary (made-up unit, place and power names, never Zero
//! Hour's), so the model learns to copy whatever name it hears instead of memorising a roster, and
//! weighted toward the commands players give most (a selection, a common verb, a target).
//!
//! Every command drawn prints to canonical text that parses back to it ([`Sampler::command`]'s
//! callers check, and the tests hold it over thousands of draws).

use super::ast::*;
use super::vocab::{fixed_verbs, Noun, ThingList, VerbDef, Vocabulary};
use super::world::{plural, CLASSES};
use crate::weapons::rng::Rng;

const SYLLABLES: [&str; 24] = [
    "bra", "kor", "vel", "tan", "dri", "mox", "sel", "rav", "quin", "dor", "fen", "lum", "zar", "pel", "gor", "hal", "nix", "tor", "vas", "wren",
    "cal", "mir", "ost", "bel",
];
const UNIT_NOUNS: [&str; 14] = ["tank", "truck", "trooper", "walker", "gunship", "drone", "tower", "bunker", "factory", "cannon", "launcher", "rover", "jet", "depot"];
const PLACE_NOUNS: [&str; 8] = ["ridge", "pass", "crossing", "hill", "bridge", "valley", "outpost", "quarry"];
const UPGRADE_NOUNS: [&str; 5] = ["armor", "rounds", "engines", "training", "optics"];
const POWER_NOUNS: [&str; 5] = ["strike", "drop", "barrage", "storm", "sweep"];
const SCIENCE_NOUNS: [&str; 3] = ["doctrine", "training", "logistics"];
const MODE_NOUNS: [&str; 3] = ["rounds", "mode", "shells"];
const WEAPON_NOUNS: [&str; 3] = ["cannon", "missile", "mortar"];
const TEAM_NOUNS: [&str; 3] = ["team", "squad", "wing"];

fn pick<T: Copy>(rng: &mut Rng, items: &[T]) -> T {
    items[rng.next_u32() as usize % items.len()]
}

fn chance(rng: &mut Rng, percent: u32) -> bool {
    rng.next_u32() % 100 < percent
}

fn below(rng: &mut Rng, n: usize) -> usize {
    rng.next_u32() as usize % n
}

/// An invented word of two or three syllables.
fn word(rng: &mut Rng) -> String {
    let n = 2 + (chance(rng, 25) as usize);
    (0..n).map(|_| pick(rng, &SYLLABLES)).collect()
}

/// Whether `w` is a word the sampler could have invented (two or three of its syllables), as a
/// plural or possessive too: never an ordinary word, whatever the paraphrases did with it.
pub fn is_invented(w: &str) -> bool {
    let splits = |s: &str| -> bool {
        // The syllables' count along each prefix of `s`, by dynamic programming.
        let mut counts: Vec<Vec<usize>> = vec![Vec::new(); s.len() + 1];
        counts[0].push(0);
        for i in 0..s.len() {
            let here = counts[i].clone();
            for syl in SYLLABLES {
                if s[i..].starts_with(syl) {
                    counts[i + syl.len()].extend(here.iter().map(|n| n + 1));
                }
            }
        }
        counts[s.len()].iter().any(|n| (2..=3).contains(n))
    };
    let w = w.strip_suffix("'s").unwrap_or(w);
    splits(w) || w.strip_suffix("es").is_some_and(splits) || w.strip_suffix('s').is_some_and(splits)
}

/// `n` distinct names from `make`.
fn names(rng: &mut Rng, n: usize, make: &mut dyn FnMut(&mut Rng) -> String) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut tries = 0;
    while out.len() < n && tries < n * 20 {
        tries += 1;
        let name = make(rng);
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// An invented vocabulary: 8 to 14 unit types (an invented word, alone or before a unit noun), a few
/// places, team names, upgrades, powers, promotions, weapon modes and weapons; the real class
/// words. No name is a grammar word, a verb, or another list's name.
pub fn invented_vocabulary(rng: &mut Rng) -> Vocabulary {
    loop {
        let n_types = 8 + below(rng, 7);
        let types: Vec<Noun> = names(rng, n_types, &mut |r| if chance(r, 60) { format!("{} {}", word(r), pick(r, &UNIT_NOUNS)) } else { word(r) })
            .into_iter()
            .map(|s| Noun::new(&s, &plural(&s)))
            .collect();
        let with = |r: &mut Rng, nouns: &[&str], n: usize| names(r, n, &mut |r| format!("{} {}", word(r), pick(r, nouns)));
        let counts: Vec<usize> = [5, 3, 3, 2, 3, 3, 3].iter().map(|&n| below(rng, n)).collect();
        let places = names(rng, counts[0], &mut |r| if chance(r, 40) { format!("the {} {}", word(r), pick(r, &PLACE_NOUNS)) } else { format!("{} {}", word(r), pick(r, &PLACE_NOUNS)) });
        let upgrades = with(rng, &UPGRADE_NOUNS, 2 + counts[1]);
        let powers = with(rng, &POWER_NOUNS, 1 + counts[2]);
        let sciences = with(rng, &SCIENCE_NOUNS, 1 + counts[3]);
        let modes = with(rng, &MODE_NOUNS, counts[4]);
        let weapons = with(rng, &WEAPON_NOUNS, counts[5]);
        let teams = with(rng, &TEAM_NOUNS, counts[6]);
        let v = Vocabulary {
            types,
            classes: CLASSES.iter().map(|(s, p, _)| Noun::new(s, p)).collect(),
            upgrades,
            powers,
            sciences,
            modes,
            weapons,
            places,
            teams,
            ..Vocabulary::default()
        };
        if v.problems().is_empty() {
            return v;
        }
    }
}

/// How often each verb is drawn (out of the sum), the common orders first.
fn verb_weight(words: &str) -> u32 {
    match words {
        "move" => 14,
        "attack" => 10,
        "attack-move" => 8,
        "build" => 8,
        "stop" | "guard" => 6,
        "act on" => 5,
        "use" => 4,
        "upgrade" | "enter" | "garrison" | "capture" | "repair" => 3,
        "force attack" | "force move" | "purchase" | "set rally point" | "sell" | "evacuate" | "cancel" | "unload" => 2,
        _ => 1,
    }
}

/// Draws canonical commands over one vocabulary.
pub struct Sampler<'a> {
    pub rng: Rng,
    pub vocab: &'a Vocabulary,
}

impl Sampler<'_> {
    fn chance(&mut self, percent: u32) -> bool {
        chance(&mut self.rng, percent)
    }

    fn below(&mut self, n: usize) -> usize {
        below(&mut self.rng, n)
    }

    pub fn command(&mut self) -> Command {
        let selection = self.chance(75).then(|| self.selection());
        let first = self.order();
        let mut orders = vec![first];
        if self.chance(12) {
            // Mostly a queued move after the first (the resolver queues moves only).
            let next = if self.chance(70) { self.verb_order(&fixed_verbs().into_iter().find(|v| v.words == "move").unwrap()) } else { self.order() };
            orders.push(next);
        }
        Command { selection, orders }
    }

    fn selection(&mut self) -> Selection {
        let n = match self.below(100) {
            0..=84 => 1,
            85..=97 => 2,
            _ => 3,
        };
        let include = (0..n).map(|_| self.group()).collect();
        let exclude = if self.chance(8) { vec![self.group()] } else { Vec::new() };
        Selection { include, exclude }
    }

    fn group(&mut self) -> Group {
        match self.below(100) {
            0..=74 => Group::Query(self.query(false, true).1),
            75..=82 => Group::Them,
            83..=90 => Group::That,
            91..=95 if !self.vocab.teams.is_empty() => Group::Team(self.vocab.teams[self.below(self.vocab.teams.len())].clone()),
            _ => Group::ControlGroup(self.below(10) as u8),
        }
    }

    fn query(&mut self, target: bool, filters: bool) -> (Relation, Query) {
        let relation = if target {
            match self.below(100) {
                0..=34 => Relation::Any,
                35..=74 => Relation::Enemy,
                75..=89 => Relation::Own,
                90..=94 => Relation::Allied,
                95..=97 => Relation::Neutral,
                _ => Relation::Civilian,
            }
        } else {
            Relation::Any
        };
        let (base, mass) = match self.below(100) {
            0..=69 => (Base::Type(self.vocab.types[self.below(self.vocab.types.len())].singular.clone()), false),
            70..=84 => {
                let n = &self.vocab.classes[self.below(self.vocab.classes.len())];
                (Base::Class(n.singular.clone()), n.is_mass())
            }
            _ => (Base::Units, false),
        };
        let quantity = if target {
            match self.below(100) {
                0..=69 => Quantity::One,
                70..=89 => Quantity::All,
                90..=94 => Quantity::Count(2 + self.below(5) as u32),
                _ => Quantity::Half,
            }
        } else {
            match self.below(100) {
                0..=54 => Quantity::All,
                55..=79 => Quantity::One,
                80..=94 => Quantity::Count(2 + self.below(8) as u32),
                _ => Quantity::Half,
            }
        };
        let mut superlative = match quantity {
            Quantity::One | Quantity::Count(_) if self.chance(45) => Some(Superlative::ALL[self.below(Superlative::ALL.len())]),
            _ => None,
        };
        // `my <mass noun>` alone is all of them: one of them needs a superlative.
        if relation == Relation::Own && mass && quantity == Quantity::One && superlative.is_none() {
            superlative = Some(Superlative::Nearest);
        }
        let adjectives = Adjective::ALL.iter().copied().filter(|_| chance(&mut self.rng, 6)).collect();
        let mut f = Filters::default();
        if filters {
            if self.chance(8) {
                f.near = Some(Box::new(self.point(false, true)));
            }
            if self.chance(5) {
                f.region = Some(Region::ALL[self.below(Region::ALL.len())]);
            }
            f.my_group = self.chance(5);
        }
        (relation, Query { quantity, superlative, adjectives, base, filters: f })
    }

    fn object(&mut self, filters: bool) -> ObjectRef {
        if self.chance(18) {
            ObjectRef::That
        } else {
            let (relation, query) = self.query(true, filters);
            ObjectRef::Query { relation, query }
        }
    }

    fn point(&mut self, filters: bool, there: bool) -> Point {
        let roll = self.below(100);
        match roll {
            0..=21 => Point::Me,
            22..=43 if there => Point::There,
            44..=55 if !self.vocab.places.is_empty() => Point::Place(self.vocab.places[self.below(self.vocab.places.len())].clone()),
            _ => Point::Object(self.object(filters)),
        }
    }

    fn order(&mut self) -> Order {
        match self.below(100) {
            0..=1 => Order::NameThem(format!("{} {}", word(&mut self.rng), pick(&mut self.rng, &TEAM_NOUNS))),
            2..=3 => Order::MakeGroup(self.below(10) as u8),
            _ => {
                let verbs: Vec<VerbDef> = fixed_verbs().into_iter().filter(|v| v.thing.is_none_or(|(list, _)| !self.vocab.things(list).is_empty())).collect();
                let total: u32 = verbs.iter().map(|v| verb_weight(&v.words)).sum();
                let mut roll = self.rng.next_u32() % total;
                let verb = verbs.iter().find(|v| {
                    let w = verb_weight(&v.words);
                    if roll < w {
                        true
                    } else {
                        roll -= w;
                        false
                    }
                });
                self.verb_order(verb.unwrap_or(&verbs[0]))
            }
        }
    }

    fn verb_order(&mut self, verb: &VerbDef) -> Order {
        let thing = verb.thing.map(|(list, counted)| {
            let names = self.vocab.things(list);
            let name = names[self.below(names.len())].to_string();
            let count = if counted && list != ThingList::Production && self.chance(30) { 2 + self.below(5) as u32 } else { 1 };
            Thing { name, count }
        });
        let target = verb.target.and_then(|slot| {
            if !slot.required && self.chance(45) {
                return None;
            }
            if slot.line && self.chance(5) {
                let start = Location { offset: None, point: self.point(true, true) };
                let end = Location { offset: None, point: self.point(true, true) };
                return Some(Box::new(Target { location: start, facing: None, end: Some(end) }));
            }
            let location = if slot.point && self.chance(75) {
                let offset = if self.chance(25) {
                    Some(match self.below(4) {
                        0 => Offset::Near,
                        1 => Offset::Behind,
                        2 => Offset::InFrontOf,
                        _ => Offset::Of(Direction::ALL[self.below(8)]),
                    })
                } else {
                    None
                };
                Location { offset, point: self.point(true, true) }
            } else {
                let point = if self.chance(15) { Point::Me } else { Point::Object(self.object(true)) };
                Location { offset: None, point }
            };
            let facing = (slot.facing && self.chance(10)).then(|| Direction::ALL[self.below(8)]);
            Some(Box::new(Target { location, facing, end: None }))
        });
        Order::Verb { verb: verb.words.clone(), thing, target }
    }
}

/// The vocabulary's names that occur in `text` as whole words, longest first: the words a
/// paraphrase must keep as they are.
pub fn names_in(text: &str, vocab: &Vocabulary) -> Vec<String> {
    let mut all: Vec<String> = vocab
        .types
        .iter()
        .flat_map(|n| [n.singular.clone(), n.plural.clone()])
        .chain(vocab.places.iter().cloned())
        .chain(vocab.teams.iter().cloned())
        .chain(vocab.upgrades.iter().cloned())
        .chain(vocab.powers.iter().cloned())
        .chain(vocab.sciences.iter().cloned())
        .chain(vocab.modes.iter().cloned())
        .chain(vocab.weapons.iter().cloned())
        .collect();
    all.sort_by_key(|n| std::cmp::Reverse(n.len()));
    all.dedup();
    let padded = format!(" {text} ");
    let mut found: Vec<String> = Vec::new();
    for n in all {
        if padded.contains(&format!(" {n} ")) && !found.iter().any(|f| f.contains(&n)) {
            found.push(n);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{parse, print};

    #[test]
    fn invented_words_are_told_from_ordinary_ones() {
        for w in ["korgor", "nixveldor", "moxzar", "fenpels", "torhal's", "tandorvases", "wrenravfen"] {
            assert!(is_invented(w), "{w}");
        }
        for w in ["tower", "kor", "korgorvelbra", "near", "garrison", "salvage", "tortoise"] {
            assert!(!is_invented(w), "{w}");
        }
    }

    #[test]
    fn invented_vocabularies_have_no_problems_and_no_zero_hour_names() {
        let mut rng = Rng::new(7);
        for _ in 0..200 {
            let v = invented_vocabulary(&mut rng);
            assert!(v.problems().is_empty());
            for n in &v.types {
                assert!(!["ranger", "crusader", "dozer", "overlord"].iter().any(|z| n.singular.contains(z)), "{n:?}");
            }
        }
    }

    #[test]
    fn drawn_commands_are_canonical_and_round_trip() {
        let mut rng = Rng::new(0x0bad_5eed);
        let mut verbs = std::collections::BTreeSet::new();
        for i in 0..3000 {
            let v = invented_vocabulary(&mut rng);
            let mut s = Sampler { rng: Rng::new(rng.next_u32()), vocab: &v };
            let cmd = s.command();
            let text = print(&cmd, &v);
            let back = parse(&text, &v).unwrap_or_else(|e| panic!("#{i}: '{text}' did not parse: {e}"));
            assert_eq!(back, cmd, "#{i}: '{text}'");
            for o in &cmd.orders {
                if let Order::Verb { verb, .. } = o {
                    verbs.insert(verb.clone());
                }
            }
        }
        // Every verb of the language turns up.
        let all: std::collections::BTreeSet<String> = fixed_verbs().into_iter().map(|v| v.words).collect();
        assert_eq!(all.difference(&verbs).collect::<Vec<_>>(), Vec::<&String>::new());
    }

    #[test]
    fn names_in_finds_whole_names_longest_first() {
        let v = Vocabulary { types: vec![Noun::new("korvel tank", "korvel tanks"), Noun::new("korvel", "korvels")], places: vec!["the dorfen pass".into()], ..Vocabulary::default() };
        assert_eq!(names_in("korvel tanks move to the dorfen pass", &v), ["the dorfen pass", "korvel tanks"]);
        assert_eq!(names_in("the korvel stop", &v), ["korvel"]);
    }
}
