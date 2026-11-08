//! The canonical language's tests: the design doc's examples, the spelling rules, errors, the
//! vocabulary check, and the round trip over random commands.

use super::ast::*;
use super::vocab::{fixed_verbs, Noun, ThingList};
use super::parse::completions;
use super::{parse, print, Vocabulary};
use crate::weapons::rng::Rng;

/// A small USA-flavoured match vocabulary with multi-word and mass nouns.
fn vocab() -> Vocabulary {
    let types = [
        ("ranger", "rangers"),
        ("dozer", "dozers"),
        ("humvee", "humvees"),
        ("crusader tank", "crusader tanks"),
        ("war factory", "war factories"),
        ("power plant", "power plants"),
        ("oil derrick", "oil derricks"),
        ("command center", "command centers"),
        ("overlord tank", "overlord tanks"),
    ];
    let classes = [
        ("infantry", "infantry"),
        ("vehicle", "vehicles"),
        ("aircraft", "aircraft"),
        ("building", "buildings"),
    ];
    Vocabulary {
        types: types.iter().map(|(s, p)| Noun::new(s, p)).collect(),
        classes: classes.iter().map(|(s, p)| Noun::new(s, p)).collect(),
        upgrades: vec!["composite armor".into(), "flash-bang grenades".into()],
        powers: vec!["carpet bomb".into(), "paradrop".into()],
        sciences: vec!["advanced training".into()],
        modes: vec!["flash-bang grenades mode".into()],
        weapons: vec!["nuke cannon shell".into()],
        places: vec!["the bridge".into(), "north pass".into()],
        teams: vec!["alpha team".into()],
        ..Vocabulary::default()
    }
}

fn round_trip(text: &str) -> Command {
    let v = vocab();
    let cmd = parse(text, &v).unwrap_or_else(|e| panic!("'{text}' did not parse: {e}"));
    assert_eq!(print(&cmd, &v), text, "'{text}' does not print back the same");
    cmd
}

fn refused(text: &str) {
    assert!(parse(text, &vocab()).is_err(), "'{text}' parsed but is not canonical");
}

fn query(quantity: Quantity, base: Base) -> Query {
    Query { quantity, superlative: None, adjectives: Vec::new(), base, filters: Filters::default() }
}

fn ty(name: &str) -> Base {
    Base::Type(name.to_string())
}

#[test]
fn the_test_vocabulary_has_no_problems() {
    assert_eq!(vocab().problems(), Vec::<String>::new());
}

#[test]
fn the_design_docs_examples_parse_and_print_back() {
    for text in [
        "rangers move to me",
        "units attack-move there",
        "them attack that",
        "the nearest dozer build power plant near my war factory",
        "build 3 crusader tanks",
        "the northernmost dozer repair that",
        "humvees and crusader tanks in the southern group attack the nearest enemy war factory",
        "units except dozers guard the nearest oil derrick",
        "idle rangers near me name them alpha team",
        "alpha team act on that",
        "rangers guard me",
        "rangers move to the bridge then attack-move north of the bridge",
        "use carpet bomb there",
        "purchase advanced training",
        "the nearest dozer build power plant from me to north of me",
        "rangers act on there",
    ] {
        round_trip(text);
    }
}

#[test]
fn a_command_reads_as_its_parts() {
    let cmd = round_trip("the 3 nearest idle rangers in my group move to the bridge then stop");
    let Some(sel) = &cmd.selection else { panic!("no selection") };
    assert_eq!(
        sel.include,
        vec![Group::Query(Query {
            quantity: Quantity::Count(3),
            superlative: Some(Superlative::Nearest),
            adjectives: vec![Adjective::Idle],
            base: ty("ranger"),
            filters: Filters { my_group: true, ..Filters::default() },
        })]
    );
    assert_eq!(cmd.orders.len(), 2);
    assert_eq!(
        cmd.orders[0],
        Order::Verb {
            verb: "move".into(),
            thing: None,
            target: Some(Box::new(Target {
                location: Location { offset: None, point: Point::Place("the bridge".into()) },
                facing: None,
                end: None
            })),
        }
    );

    let cmd = round_trip("build 2 power plants near my command center facing east");
    assert_eq!(cmd.selection, None);
    let Order::Verb { thing, target, .. } = &cmd.orders[0] else { panic!() };
    assert_eq!(thing, &Some(Thing { name: "power plant".into(), count: 2 }));
    let target = target.as_ref().unwrap();
    assert_eq!(target.facing, Some(Direction::East));
    assert_eq!(target.location.offset, Some(Offset::Near));
    assert_eq!(
        target.location.point,
        Point::Object(ObjectRef::Query {
            relation: Relation::Own,
            query: query(Quantity::One, ty("command center"))
        })
    );
}

#[test]
fn quantities_follow_the_determiner_and_the_nouns_number() {
    let sel = |text: &str| match round_trip(text).selection.unwrap().include.remove(0) {
        Group::Query(q) => (q.quantity, q.superlative),
        g => panic!("{g:?}"),
    };
    assert_eq!(sel("rangers stop"), (Quantity::All, None));
    assert_eq!(sel("the ranger stop"), (Quantity::One, None));
    assert_eq!(sel("3 rangers stop"), (Quantity::Count(3), None));
    assert_eq!(sel("the 3 weakest rangers stop"), (Quantity::Count(3), Some(Superlative::Weakest)));
    assert_eq!(sel("half the rangers stop"), (Quantity::Half, None));
    // A mass noun: plural bare, singular after `the`.
    assert_eq!(sel("infantry stop"), (Quantity::All, None));
    assert_eq!(sel("the nearest infantry stop"), (Quantity::One, Some(Superlative::Nearest)));
    assert_eq!(sel("the unit stop"), (Quantity::One, None));

    let target = |text: &str| match &round_trip(text).orders[0] {
        Order::Verb { target: Some(t), .. } => t.location.point.clone(),
        o => panic!("{o:?}"),
    };
    let own = |quantity, superlative| {
        Point::Object(ObjectRef::Query {
            relation: Relation::Own,
            query: Query { superlative, ..query(quantity, ty("war factory")) },
        })
    };
    assert_eq!(target("rangers guard my war factory"), own(Quantity::One, None));
    assert_eq!(target("rangers guard my war factories"), own(Quantity::All, None));
    assert_eq!(target("rangers guard my 2 war factories"), own(Quantity::Count(2), None));
    assert_eq!(target("rangers guard my nearest war factory"), own(Quantity::One, Some(Superlative::Nearest)));
    assert_eq!(target("rangers guard half my war factories"), own(Quantity::Half, None));
}

#[test]
fn non_canonical_spellings_are_refused() {
    // Determiners and numbers.
    refused("the rangers stop");
    refused("1 ranger stop");
    refused("01 rangers stop");
    refused("3 ranger stop");
    refused("ranger stop");
    refused("nearest rangers stop");
    refused("3 nearest rangers stop");
    refused("the 3 rangers stop");
    refused("half the nearest rangers stop");
    refused("my rangers stop"); // a selection is the player's own and never says so
    refused("the nearest war factories stop");
    // Adjectives out of order or repeated.
    refused("damaged idle rangers stop");
    refused("idle idle rangers stop");
    // Prepositions: dropped before `there` and an offset, spoken otherwise.
    refused("rangers move to there");
    refused("rangers move to north of the bridge");
    refused("rangers move the bridge");
    refused("use carpet bomb at there");
    // An object-only verb takes no point.
    refused("rangers attack there");
    refused("rangers attack north of the bridge");
    // Counts: only verbs that take one, never 1.
    refused("build 1 power plant");
    refused("build power plants");
    refused("upgrade 2 composite armor");
    // `facing` only where a building is placed, a line only where one is built.
    refused("rangers move to the bridge facing north");
    refused("rangers move from me to the bridge");
    refused("build power plant from me to there facing north");
    // Unfinished, or extra words.
    refused("rangers move to");
    refused("rangers");
    refused("rangers stop now");
    refused("");
}

#[test]
fn a_trailing_filter_belongs_to_the_noun_before_near() {
    let cmd = round_trip("rangers near the war factory in the northern group stop");
    let Group::Query(q) = &cmd.selection.unwrap().include[0] else { panic!() };
    assert_eq!(q.filters.region, Some(Region::Northern));
    assert_eq!(
        q.filters.near.as_deref(),
        Some(&Point::Object(ObjectRef::Query { relation: Relation::Any, query: query(Quantity::One, ty("war factory")) }))
    );
}

#[test]
fn errors_say_where_and_what_was_expected() {
    let v = vocab();
    let e = parse("rangers move to", &v).unwrap_err();
    assert_eq!((e.at, e.found.as_deref()), (3, None));
    for want in ["me", "that", "<place>", "<unit type>", "the", "my"] {
        assert!(e.expected.iter().any(|x| x == want), "expected {want} in {:?}", e.expected);
    }
    assert!(!e.expected.iter().any(|x| x == "there"), "`there` takes no preposition: {:?}", e.expected);

    let e = parse("rangers move to me now", &v).unwrap_err();
    assert_eq!((e.at, e.found.as_deref()), (4, Some("now")));
    assert!(e.expected.contains(&"then".to_string()), "{:?}", e.expected);

    let e = parse("rangres stop", &v).unwrap_err();
    assert_eq!(e.at, 0);
    assert!(e.expected.len() > 6);
    let msg = e.to_string();
    assert!(msg.starts_with("didn't expect 'rangres'; expected "), "{msg}");
    assert!(msg.ends_with(&format!(" and {} more", e.expected.len() - 6)), "only six are named: {msg}");
}

#[test]
fn the_vocabulary_check_names_each_problem() {
    let mut v = vocab();
    v.types.push(Noun::new("stop", "stops"));
    v.places.push("ranger".into());
    v.teams.push("bravo then charlie".into());
    v.powers.push("Carpet Bomb".into());
    v.sciences.push("advanced training".into());
    let problems = v.problems();
    for want in [
        "type 'stop' is a word of the grammar",
        "'ranger' is both a type and a place",
        "team 'bravo then charlie' contains 'then'",
        "power 'Carpet Bomb': 'Carpet' is not lowercase letters, digits, '-' or '''",
        "science 'advanced training' is listed twice",
        "'ranger' is both a type and a place",
    ] {
        assert!(problems.iter().any(|p| p == want), "missing '{want}' in {problems:#?}");
    }
}

#[test]
fn every_fixed_verb_round_trips() {
    let v = vocab();
    for verb in fixed_verbs() {
        let mut text = String::from("rangers ");
        text.push_str(&verb.words);
        match verb.thing {
            Some((ThingList::Types, _)) => text.push_str(" power plant"),
            Some((list, _)) => {
                text.push(' ');
                text.push_str(v.things(list)[0]);
            }
            None => {}
        }
        if let Some(slot) = verb.target {
            if slot.required {
                if let Some(prep) = &verb.prep {
                    text.push(' ');
                    text.push_str(prep);
                }
                text.push_str(if slot.object { " that" } else { " the bridge" });
            }
        }
        round_trip(&text);
    }
}

// ------------------------------------------------------------------------------------------------
// The round trip over random commands
// ------------------------------------------------------------------------------------------------

struct Gen<'a> {
    rng: Rng,
    vocab: &'a Vocabulary,
}

impl Gen<'_> {
    fn below(&mut self, n: usize) -> usize {
        self.rng.next_u32() as usize % n
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())].clone()
    }

    fn query(&mut self, target: bool, filters: bool) -> (Relation, Query) {
        let relation = if target {
            self.pick(&[Relation::Any, Relation::Own, Relation::Enemy, Relation::Allied, Relation::Neutral, Relation::Civilian])
        } else {
            Relation::Any
        };
        let (base, mass) = match self.below(5) {
            0 => {
                let n = self.pick(&self.vocab.classes);
                (Base::Class(n.singular.clone()), n.is_mass())
            }
            1 => (Base::Units, false),
            _ => (Base::Type(self.pick(&self.vocab.types).singular), false),
        };
        let (quantity, superlative) = loop {
            let quantity = match self.below(4) {
                0 => Quantity::All,
                1 => Quantity::One,
                2 => Quantity::Count(2 + self.below(11) as u32),
                _ => Quantity::Half,
            };
            let superlative = match quantity {
                Quantity::One | Quantity::Count(_) if self.chance(50) => Some(self.pick(&Superlative::ALL)),
                _ => None,
            };
            // `my <mass noun>` is all of them; one of them needs a superlative.
            if relation == Relation::Own && mass && quantity == Quantity::One && superlative.is_none() {
                continue;
            }
            break (quantity, superlative);
        };
        let adjectives = Adjective::ALL.iter().copied().filter(|_| self.chance(15)).collect();
        let mut f = Filters::default();
        if filters {
            if self.chance(25) {
                f.near = Some(Box::new(self.point(false)));
            }
            if self.chance(20) {
                f.region = Some(self.pick(&Region::ALL));
            }
            f.my_group = self.chance(15);
        }
        (relation, Query { quantity, superlative, adjectives, base, filters: f })
    }

    fn object(&mut self, filters: bool) -> ObjectRef {
        if self.chance(20) {
            ObjectRef::That
        } else {
            let (relation, query) = self.query(true, filters);
            ObjectRef::Query { relation, query }
        }
    }

    fn point(&mut self, filters: bool) -> Point {
        match self.below(5) {
            0 => Point::Me,
            1 => Point::There,
            2 => Point::Place(self.pick(&self.vocab.places)),
            _ => Point::Object(self.object(filters)),
        }
    }

    fn group(&mut self) -> Group {
        match self.below(8) {
            0 => Group::ControlGroup(self.below(10) as u8),
            1 => Group::Team(self.pick(&self.vocab.teams)),
            2 => Group::Them,
            3 => Group::That,
            _ => Group::Query(self.query(false, true).1),
        }
    }

    fn groups(&mut self, min: usize) -> Vec<Group> {
        (0..min + self.below(3)).map(|_| self.group()).collect()
    }

    fn order(&mut self) -> Order {
        match self.below(12) {
            0 => {
                let words = ["bravo", "squad", "2", "north", "tank", "hunters"];
                let n = 1 + self.below(3);
                Order::NameThem((0..n).map(|_| self.pick(&words)).collect::<Vec<_>>().join(" "))
            }
            1 => Order::MakeGroup(self.below(10) as u8),
            _ => {
                let verb = self.pick(&self.vocab.verbs);
                let thing = verb.thing.map(|(list, counted)| {
                    let name = self.pick(&self.vocab.things(list)).to_string();
                    let count = if counted && self.chance(40) { 2 + self.below(8) as u32 } else { 1 };
                    Thing { name, count }
                });
                let target = verb.target.and_then(|slot| {
                    if !slot.required && self.chance(30) {
                        return None;
                    }
                    let location = if slot.point && self.chance(70) {
                        let offset = if self.chance(40) { Some(self.pick(&Offset::all().collect::<Vec<_>>())) } else { None };
                        Location { offset, point: self.point(true) }
                    } else {
                        let point = if self.chance(20) { Point::Me } else { Point::Object(self.object(true)) };
                        Location { offset: None, point }
                    };
                    if slot.line && self.chance(20) {
                        let end = Location { offset: None, point: self.point(true) };
                        return Some(Target { location, facing: None, end: Some(end) });
                    }
                    let facing = if slot.facing && self.chance(40) { Some(self.pick(&Direction::ALL)) } else { None };
                    Some(Target { location, facing, end: None })
                });
                Order::Verb { verb: verb.words, thing, target: target.map(Box::new) }
            }
        }
    }

    fn command(&mut self) -> Command {
        let selection = if self.chance(80) {
            Some(Selection { include: self.groups(1), exclude: if self.chance(25) { self.groups(1) } else { Vec::new() } })
        } else {
            None
        };
        let orders = (0..1 + self.below(3)).map(|_| self.order()).collect();
        Command { selection, orders }
    }
}

#[test]
fn random_commands_print_and_parse_back_unchanged() {
    let v = vocab();
    let mut g = Gen { rng: Rng::new(0x5eed_c0de), vocab: &v };
    for i in 0..4000 {
        let cmd = g.command();
        let text = print(&cmd, &v);
        match parse(&text, &v) {
            Ok(back) => assert_eq!(back, cmd, "#{i}: '{text}' parsed to another command"),
            Err(e) => panic!("#{i}: '{text}' did not parse: {e}\n{cmd:#?}"),
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Completions
// ------------------------------------------------------------------------------------------------

#[test]
fn completions_offer_only_what_keeps_the_command_parseable() {
    let v = vocab();
    let c = completions("", &v);
    assert!(!c.complete);
    // A command may start with a selection or a verb (an implied selection).
    for want in ["rangers", "the", "units", "group", "them", "that", "alpha team", "build", "stop", "move", "purchase"] {
        assert!(c.next.iter().any(|n| n == want), "'{want}' missing from {:?}", c.next);
    }
    // After `the`, singulars only.
    let c = completions("the", &v);
    assert!(c.next.iter().any(|n| n == "ranger") && !c.next.iter().any(|n| n == "rangers"), "{:?}", c.next);
    assert!(c.next.iter().any(|n| n == "nearest"));
    // After the selection: verbs. `rangers move to` wants a point, not `there` (no preposition there).
    let c = completions("rangers move to", &v);
    assert!(c.next.iter().any(|n| n == "me") && c.next.iter().any(|n| n == "the bridge"));
    assert!(!c.next.iter().any(|n| n == "there"), "{:?}", c.next);
    // A whole command is complete, and may go on.
    let c = completions("rangers move to me", &v);
    assert!(c.complete);
    assert!(c.next.iter().any(|n| n == "then"));
    // `build` offers counts and types; `name them` offers fresh team names.
    let c = completions("build", &v);
    assert!(c.next.iter().any(|n| n == "power plant") && c.next.iter().any(|n| n == "3"));
    let c = completions("rangers name them", &v);
    assert!(c.next.iter().any(|n| n == "bravo team") && !c.next.iter().any(|n| n == "alpha team"), "{:?}", c.next);
    // An unparseable prefix has none.
    assert_eq!(completions("rangres", &v), super::parse::Completions::default());
    // Every offer extends the prefix into something that parses on.
    for prefix in ["", "the nearest", "rangers", "rangers guard", "build 3"] {
        for n in completions(prefix, &v).next {
            let longer = format!("{prefix} {n}");
            assert!(completions(&longer, &v).complete || !completions(&longer, &v).next.is_empty(), "'{longer}' is a dead end");
        }
    }
}

#[test]
fn completions_stay_quick_over_a_large_vocabulary() {
    // A full match's vocabulary: hundreds of type names (the map's and the buildable ones).
    let mut v = vocab();
    for i in 0..300 {
        v.types.push(Noun::new(&format!("unit kind {i}"), &format!("unit kinds {i}")));
    }
    let start = std::time::Instant::now();
    for prefix in ["", "the", "rangers", "rangers move to", "build"] {
        assert!(!completions(prefix, &v).next.is_empty());
    }
    let took = start.elapsed();
    eprintln!("five menu refreshes over {} types: {took:?}", v.types.len());
    // A refresh runs once per pick, not per frame; even unoptimised it stays well under a second.
    assert!(took.as_secs_f32() < 1.0, "{took:?}");
}
