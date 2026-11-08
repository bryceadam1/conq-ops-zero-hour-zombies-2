//! Canonical text to a command AST (docs/COMMANDS.md section 4).
//!
//! The parser follows every alternative and keeps every complete parse, so a text that could mean
//! two commands is reported as ambiguous instead of resolved silently; canonical text never is
//! (the round-trip tests check this). It also records the furthest word it reached and what it
//! expected there, for the error message and, later, the valid-completions list.
//!
//! Spelling rules the grammar alone does not show, all so each command has one spelling:
//!
//! * A plural with no determiner means all (`rangers`); `all` is accepted input only (the
//!   normaliser drops it). `the` takes a singular, a number of 2 or more takes a plural, and `1`
//!   is spelled `the`.
//! * A superlative needs `the` or `my`: `the nearest ranger`, `the 3 nearest rangers`,
//!   `my nearest war factory`. A bare number takes no `the` (`3 rangers`).
//! * A mass noun (same singular and plural, like `infantry`) is singular only after `the` or with
//!   a superlative; `my infantry` is all of them.
//! * Adjectives come in a fixed order (`idle damaged veteran elite heroic garrisoned loaded`).
//! * The point after a `near` filter has no filters of its own, so a trailing filter always
//!   belongs to the noun before `near`.
//! * A verb's preposition is left out before `there` and before an offset.

use std::collections::BTreeSet;
use std::fmt;

use super::ast::*;
use super::print::drops_prep;
use super::vocab::{is_word, Noun, TargetSlot, ThingList, VerbDef, Vocabulary};

/// How many expected words a message names.
const SHOWN_EXPECTED: usize = 6;

/// Why a text is not a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The word index the parser could not get past (the text's length: it ended too early).
    pub at: usize,
    /// The word there, if any.
    pub found: Option<String>,
    /// What would have been accepted there: fixed words, or a list in angle brackets
    /// (`<unit type>`, `<place>`, `<number>`).
    pub expected: Vec<String>,
    /// More than one command reads this way (never for canonical text over a vocabulary without
    /// [`Vocabulary::problems`]).
    pub ambiguous: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ambiguous > 1 {
            return write!(f, "the command reads {} ways", self.ambiguous);
        }
        match &self.found {
            Some(w) => write!(f, "didn't expect '{w}'")?,
            None => write!(f, "the command is unfinished")?,
        }
        // A short list for the echo line and the log; the whole list stays in `expected` (the
        // completions read it).
        if !self.expected.is_empty() {
            let shown = &self.expected[..self.expected.len().min(SHOWN_EXPECTED)];
            write!(f, "; expected {}", shown.join(", "))?;
            if self.expected.len() > shown.len() {
                write!(f, " and {} more", self.expected.len() - shown.len())?;
            }
        }
        Ok(())
    }
}

/// Parse canonical command text. Words are split on whitespace and lowercased.
pub fn parse(text: &str, vocab: &Vocabulary) -> Result<Command, ParseError> {
    let toks: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    let mut p = Parser { toks: &toks, vocab, furthest: 0, expected: BTreeSet::new() };
    let mut done: Vec<Command> = Vec::new();
    for (cmd, end) in p.command(0) {
        if end == toks.len() && !done.contains(&cmd) {
            done.push(cmd);
        }
    }
    match done.len() {
        1 => Ok(done.pop().unwrap()),
        0 => Err(ParseError {
            at: p.furthest,
            found: toks.get(p.furthest).cloned(),
            expected: p.expected.into_iter().collect(),
            ambiguous: 0,
        }),
        n => Err(ParseError { at: 0, found: None, expected: Vec::new(), ambiguous: n }),
    }
}

/// What may come next after a partial command (docs/COMMANDS.md section 8): the menu's options and,
/// later, the voice decoder's mask.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completions {
    /// Each word or phrase that keeps the command parseable, in the parser's order of expectation
    /// (names expanded from their lists), no repeats.
    pub next: Vec<String>,
    /// The prefix is itself a whole command.
    pub complete: bool,
}

/// The counts the menu offers for `<number>`, and the team names it suggests for `name them`.
pub const MENU_NUMBERS: [&str; 8] = ["2", "3", "4", "5", "6", "8", "10", "12"];
pub const TEAM_SUGGESTIONS: [&str; 6] = ["alpha team", "bravo team", "charlie team", "delta team", "echo team", "foxtrot team"];

/// The completions of `prefix` (canonical text, possibly empty). An unparseable prefix has none.
pub fn completions(prefix: &str, vocab: &Vocabulary) -> Completions {
    let toks: Vec<String> = prefix.split_whitespace().map(str::to_lowercase).collect();
    let (reached, complete, expected) = reach(&toks, vocab);
    if reached < toks.len() {
        return Completions { next: Vec::new(), complete: false };
    }
    // Each expectation as groups of candidates that the grammar treats alike: a list's names are
    // atomic phrases, so one representative's fate is the group's. Only a noun's number tells its
    // forms apart (singulars, plurals, mass nouns that are both).
    let nouns = |list: &[Noun]| -> Vec<Vec<String>> {
        let mut singular = Vec::new();
        let mut plural = Vec::new();
        let mut mass = Vec::new();
        for n in list {
            if n.is_mass() {
                mass.push(n.singular.clone());
            } else {
                singular.push(n.singular.clone());
                plural.push(n.plural.clone());
            }
        }
        vec![singular, plural, mass]
    };
    let mut next: Vec<String> = Vec::new();
    for e in expected {
        let groups: Vec<Vec<String>> = match e.as_str() {
            "<unit type>" => nouns(&vocab.types),
            "<class>" => nouns(&vocab.classes),
            "<place>" => vec![vocab.places.clone()],
            "<team>" => vec![vocab.teams.clone()],
            "<upgrade>" => vec![vocab.upgrades.clone()],
            "<power>" => vec![vocab.powers.clone()],
            "<science>" => vec![vocab.sciences.clone()],
            "<weapon mode>" => vec![vocab.modes.clone()],
            "<weapon>" => vec![vocab.weapons.clone()],
            "<number>" => vec![MENU_NUMBERS.iter().map(|s| s.to_string()).collect()],
            "<group number>" => vec![(0..10).map(|d| d.to_string()).collect()],
            "<name>" => vec![TEAM_SUGGESTIONS.iter().filter(|t| !vocab.teams.iter().any(|n| n == *t)).map(|s| s.to_string()).collect()],
            fixed => vec![vec![fixed.to_string()]],
        };
        for group in groups {
            let Some(first) = group.first() else { continue };
            // Kept only if the command still parses through it (a plural after `the` does not).
            let mut longer = toks.clone();
            longer.extend(first.split_whitespace().map(str::to_string));
            let (r, done, _) = reach(&longer, vocab);
            if r >= longer.len() || done {
                for c in group {
                    if !next.contains(&c) {
                        next.push(c);
                    }
                }
            }
        }
    }
    Completions { next, complete }
}

/// How far the parser gets through `toks`: the furthest word it reached, whether some parse is a
/// whole command, and what it expected at the furthest word.
fn reach(toks: &[String], vocab: &Vocabulary) -> (usize, bool, BTreeSet<String>) {
    let mut p = Parser { toks, vocab, furthest: 0, expected: BTreeSet::new() };
    let complete = p.command(0).iter().any(|(_, end)| *end == toks.len());
    let reached = if complete { toks.len().max(p.furthest) } else { p.furthest };
    (reached, complete, p.expected)
}

/// Every parse of a part of the text: the value and the word index after it.
type Alts<T> = Vec<(T, usize)>;

struct Parser<'a> {
    toks: &'a [String],
    vocab: &'a Vocabulary,
    furthest: usize,
    expected: BTreeSet<String>,
}

/// How a query starts (before the superlative).
#[derive(Clone, Copy, PartialEq)]
enum Det {
    None,
    The,
    TheCount(u32),
    Count(u32),
    Half { own: bool },
    My,
    MyCount(u32),
}

impl Det {
    fn own(self) -> bool {
        matches!(self, Det::My | Det::MyCount(_) | Det::Half { own: true })
    }
}

impl Parser<'_> {
    fn tok(&self, i: usize) -> Option<&str> {
        self.toks.get(i).map(String::as_str)
    }

    fn expect(&mut self, at: usize, what: &str) {
        if at > self.furthest {
            self.furthest = at;
            self.expected.clear();
        }
        if at == self.furthest {
            self.expected.insert(what.to_string());
        }
    }

    /// Match `phrase`'s words at `i`. On a miss, the next word wanted is recorded where it was
    /// wanted, as `label` if the miss is on the first word.
    fn phrase_as(&mut self, i: usize, phrase: &str, label: &str) -> Option<usize> {
        let mut j = i;
        for w in phrase.split_whitespace() {
            if self.tok(j) == Some(w) {
                j += 1;
            } else {
                self.expect(j, if j == i { label } else { w });
                return None;
            }
        }
        Some(j)
    }

    fn word(&mut self, i: usize, phrase: &str) -> Option<usize> {
        self.phrase_as(i, phrase, phrase)
    }

    /// A number of 2 or more (a count; 1 is spelled `the`).
    fn count(&mut self, i: usize) -> Option<(u32, usize)> {
        match self.tok(i).and_then(number) {
            Some(n) if n >= 2 => Some((n, i + 1)),
            _ => {
                self.expect(i, "<number>");
                None
            }
        }
    }

    fn digit(&mut self, i: usize) -> Option<(u8, usize)> {
        match self.tok(i) {
            Some(t) if t.len() == 1 && t.as_bytes()[0].is_ascii_digit() => Some((t.as_bytes()[0] - b'0', i + 1)),
            _ => {
                self.expect(i, "<group number>");
                None
            }
        }
    }

    /// Every name of `names` that matches at `i`.
    fn names(&mut self, i: usize, names: &[String], label: &str) -> Alts<String> {
        let mut out = Vec::new();
        for n in names {
            if let Some(j) = self.phrase_as(i, n, label) {
                out.push((n.clone(), j));
            }
        }
        out
    }

    // --------------------------------------------------------------------------------------------
    // Command, selection
    // --------------------------------------------------------------------------------------------

    fn command(&mut self, i: usize) -> Alts<Command> {
        let mut sels: Alts<Option<Selection>> = vec![(None, i)];
        sels.extend(self.selection(i).into_iter().map(|(s, j)| (Some(s), j)));
        let mut out = Vec::new();
        for (selection, j) in sels {
            for (orders, k) in self.orders(j) {
                out.push((Command { selection: selection.clone(), orders }, k));
            }
        }
        out
    }

    fn orders(&mut self, i: usize) -> Alts<Vec<Order>> {
        let mut out = Vec::new();
        for (order, j) in self.order(i) {
            out.push((vec![order.clone()], j));
            if let Some(k) = self.word(j, "then") {
                for (rest, m) in self.orders(k) {
                    let mut v = vec![order.clone()];
                    v.extend(rest);
                    out.push((v, m));
                }
            }
        }
        out
    }

    fn selection(&mut self, i: usize) -> Alts<Selection> {
        let mut out = Vec::new();
        for (include, j) in self.group_list(i) {
            out.push((Selection { include: include.clone(), exclude: Vec::new() }, j));
            if let Some(k) = self.word(j, "except") {
                for (exclude, m) in self.group_list(k) {
                    out.push((Selection { include: include.clone(), exclude }, m));
                }
            }
        }
        out
    }

    fn group_list(&mut self, i: usize) -> Alts<Vec<Group>> {
        let mut out = Vec::new();
        for (g, j) in self.group(i) {
            out.push((vec![g.clone()], j));
            if let Some(k) = self.word(j, "and") {
                for (rest, m) in self.group_list(k) {
                    let mut v = vec![g.clone()];
                    v.extend(rest);
                    out.push((v, m));
                }
            }
        }
        out
    }

    fn group(&mut self, i: usize) -> Alts<Group> {
        let mut out = Vec::new();
        if let Some(j) = self.word(i, "group") {
            if let Some((n, k)) = self.digit(j) {
                out.push((Group::ControlGroup(n), k));
            }
        }
        if let Some(j) = self.word(i, "them") {
            out.push((Group::Them, j));
        }
        if let Some(j) = self.word(i, "that") {
            out.push((Group::That, j));
        }
        let vocab = self.vocab;
        out.extend(self.names(i, &vocab.teams, "<team>").into_iter().map(|(n, j)| (Group::Team(n), j)));
        for ((_, q), j) in self.query(i, false, true) {
            out.push((Group::Query(q), j));
        }
        out
    }

    // --------------------------------------------------------------------------------------------
    // Queries
    // --------------------------------------------------------------------------------------------

    /// `[determiner] [superlative] {adjective} [relation] noun {filter}`. `target`: a target's
    /// query, which may say whose (`my`, `enemy`, …); a selection's never does. `filters`: whether
    /// filters may follow (not inside a `near` filter).
    fn query(&mut self, i: usize, target: bool, filters: bool) -> Alts<(Relation, Query)> {
        let mut dets: Alts<Det> = vec![(Det::None, i)];
        if let Some(j) = self.word(i, "the") {
            dets.push((Det::The, j));
            if let Some((n, k)) = self.count(j) {
                dets.push((Det::TheCount(n), k));
            }
        }
        if let Some((n, j)) = self.count(i) {
            dets.push((Det::Count(n), j));
        }
        if let Some(j) = self.word(i, "half") {
            if let Some(k) = self.word(j, "the") {
                dets.push((Det::Half { own: false }, k));
            }
            if target {
                if let Some(k) = self.word(j, "my") {
                    dets.push((Det::Half { own: true }, k));
                }
            }
        }
        if target {
            if let Some(j) = self.word(i, "my") {
                dets.push((Det::My, j));
                if let Some((n, k)) = self.count(j) {
                    dets.push((Det::MyCount(n), k));
                }
            }
        }

        let mut out = Vec::new();
        for (det, j) in dets {
            let mut sups: Alts<Option<Superlative>> = Vec::new();
            if !matches!(det, Det::TheCount(_)) {
                sups.push((None, j));
            }
            if matches!(det, Det::The | Det::TheCount(_) | Det::My | Det::MyCount(_)) {
                for s in Superlative::ALL {
                    if let Some(k) = self.word(j, s.word()) {
                        sups.push((Some(s), k));
                    }
                }
            }
            for (superlative, k) in sups {
                for (adjectives, m) in self.adjectives(k) {
                    let mut rels: Alts<Relation> = vec![(if det.own() { Relation::Own } else { Relation::Any }, m)];
                    if target && !det.own() {
                        for r in Relation::WORDS {
                            if let Some(n) = self.word(m, r.word().unwrap()) {
                                rels.push((r, n));
                            }
                        }
                    }
                    for (relation, n) in rels {
                        for ((base, forms), p) in self.noun(n) {
                            let Some(quantity) = quantity(det, superlative, forms) else { continue };
                            let fs: Alts<Filters> =
                                if filters { self.filters(p) } else { vec![(Filters::default(), p)] };
                            for (f, r) in fs {
                                let query = Query {
                                    quantity,
                                    superlative,
                                    adjectives: adjectives.clone(),
                                    base: base.clone(),
                                    filters: f,
                                };
                                out.push(((relation, query), r));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Every run of adjectives in canonical order at `i`, the empty run included.
    fn adjectives(&mut self, i: usize) -> Alts<Vec<Adjective>> {
        let mut out = vec![(Vec::new(), i)];
        let mut idx = 0;
        while idx < out.len() {
            let (run, j) = out[idx].clone();
            for a in Adjective::ALL {
                if run.last().is_some_and(|&l| a <= l) {
                    continue;
                }
                if let Some(k) = self.word(j, a.word()) {
                    let mut r = run.clone();
                    r.push(a);
                    out.push((r, k));
                }
            }
            idx += 1;
        }
        out
    }

    /// Every noun at `i`, with which forms the words matched.
    fn noun(&mut self, i: usize) -> Alts<(Base, Forms)> {
        let mut out = Vec::new();
        let vocab = self.vocab;
        for (list, label, make) in [
            (&vocab.types, "<unit type>", Base::Type as fn(String) -> Base),
            (&vocab.classes, "<class>", Base::Class as fn(String) -> Base),
        ] {
            for n in list {
                for (form, plural) in noun_forms(n) {
                    if let Some(j) = self.phrase_as(i, form, label) {
                        let forms = Forms { singular: !plural || n.is_mass(), plural: plural || n.is_mass(), mass: n.is_mass() };
                        out.push(((make(n.singular.clone()), forms), j));
                    }
                }
            }
        }
        if let Some(j) = self.word(i, "unit") {
            out.push(((Base::Units, Forms { singular: true, plural: false, mass: false }), j));
        }
        if let Some(j) = self.word(i, "units") {
            out.push(((Base::Units, Forms { singular: false, plural: true, mass: false }), j));
        }
        out
    }

    fn filters(&mut self, i: usize) -> Alts<Filters> {
        let mut nears: Alts<Option<Box<Point>>> = vec![(None, i)];
        if let Some(j) = self.word(i, "near") {
            for (p, k) in self.point(j, false, true) {
                nears.push((Some(Box::new(p)), k));
            }
        }
        let mut out = Vec::new();
        for (near, j) in nears {
            let mut regions: Alts<Option<Region>> = vec![(None, j)];
            if let Some(k) = self.word(j, "in the") {
                for r in Region::ALL {
                    if let Some(m) = self.word(k, r.word()) {
                        if let Some(n) = self.word(m, "group") {
                            regions.push((Some(r), n));
                        }
                    }
                }
            }
            for (region, k) in regions {
                out.push((Filters { near: near.clone(), region, my_group: false }, k));
                if let Some(m) = self.word(k, "in my group") {
                    out.push((Filters { near: near.clone(), region, my_group: true }, m));
                }
            }
        }
        out
    }

    // --------------------------------------------------------------------------------------------
    // Points and locations
    // --------------------------------------------------------------------------------------------

    /// `me`, `there`, a place, `that` or an object query. `filters`: whether an object query may
    /// carry filters. `there`: whether `there` may stand here (not after a spoken preposition).
    fn point(&mut self, i: usize, filters: bool, there: bool) -> Alts<Point> {
        let mut out = Vec::new();
        if let Some(j) = self.word(i, "me") {
            out.push((Point::Me, j));
        }
        if there {
            if let Some(j) = self.word(i, "there") {
                out.push((Point::There, j));
            }
        }
        if let Some(j) = self.word(i, "that") {
            out.push((Point::Object(ObjectRef::That), j));
        }
        let vocab = self.vocab;
        out.extend(self.names(i, &vocab.places, "<place>").into_iter().map(|(n, j)| (Point::Place(n), j)));
        for ((relation, query), j) in self.query(i, true, filters) {
            out.push((Point::Object(ObjectRef::Query { relation, query }), j));
        }
        out
    }

    /// A verb's target: `there` or an offset location without the preposition, otherwise the
    /// preposition (if the verb has one) and a point.
    fn target(&mut self, i: usize, verb: &VerbDef, slot: TargetSlot) -> Alts<Option<Target>> {
        // Each candidate location, with whether the verb's preposition was spoken before it.
        let mut locs: Alts<(Location, bool)> = Vec::new();
        for o in Offset::all() {
            if let Some(j) = self.word(i, o.words()) {
                for (point, k) in self.point(j, true, true) {
                    locs.push(((Location { offset: Some(o), point }, false), k));
                }
            }
        }
        let after_prep = match &verb.prep {
            Some(prep) => self.word(i, prep),
            None => Some(i),
        };
        if let Some(j) = after_prep {
            for (point, k) in self.point(j, true, verb.prep.is_none()) {
                locs.push(((Location { offset: None, point }, verb.prep.is_some()), k));
            }
        }
        if verb.prep.is_some() {
            // `there` takes no preposition (with none, the point list above already has it).
            if let Some(j) = self.word(i, "there") {
                locs.push(((Location { offset: None, point: Point::There }, false), j));
            }
        }

        let mut out: Alts<Option<Target>> = Vec::new();
        if !slot.required {
            out.push((None, i));
        }
        for ((location, with_prep), j) in locs {
            // Canonical: a verb's preposition is spoken exactly when it is not dropped.
            if verb.prep.is_some() && with_prep == drops_prep(&location) {
                continue;
            }
            if !fits(slot, &location) {
                continue;
            }
            out.push((Some(Target { location: location.clone(), facing: None, end: None }), j));
            if slot.facing {
                if let Some(k) = self.word(j, "facing") {
                    for d in Direction::ALL {
                        if let Some(m) = self.word(k, d.word()) {
                            out.push((Some(Target { location: location.clone(), facing: Some(d), end: None }), m));
                        }
                    }
                }
            }
        }
        // A line: `from <location> to <location>`, its two ends (no preposition of the verb's).
        if slot.line {
            if let Some(j) = self.word(i, "from") {
                for (start, k) in self.bare_location(j) {
                    if let Some(m) = self.word(k, "to") {
                        for (end, n) in self.bare_location(m) {
                            out.push((Some(Target { location: start.clone(), facing: None, end: Some(end) }), n));
                        }
                    }
                }
            }
        }
        out
    }

    /// `[offset] point`, with no preposition before it (a line's ends).
    fn bare_location(&mut self, i: usize) -> Alts<Location> {
        let mut out = Vec::new();
        for o in Offset::all() {
            if let Some(j) = self.word(i, o.words()) {
                for (point, k) in self.point(j, true, true) {
                    out.push((Location { offset: Some(o), point }, k));
                }
            }
        }
        for (point, k) in self.point(i, true, true) {
            out.push((Location { offset: None, point }, k));
        }
        out
    }

    // --------------------------------------------------------------------------------------------
    // Orders
    // --------------------------------------------------------------------------------------------

    fn order(&mut self, i: usize) -> Alts<Order> {
        let mut out = Vec::new();
        if let Some(j) = self.word(i, "name them") {
            let mut k = j;
            while self.tok(k).is_some_and(|w| w != "then" && is_word(w)) {
                k += 1;
            }
            if k > j {
                out.push((Order::NameThem(self.toks[j..k].join(" ")), k));
            } else {
                self.expect(j, "<name>");
            }
        }
        if let Some(j) = self.word(i, "make them group") {
            if let Some((n, k)) = self.digit(j) {
                out.push((Order::MakeGroup(n), k));
            }
        }
        let vocab = self.vocab;
        for verb in &vocab.verbs {
            let Some(j) = self.phrase_as(i, &verb.words, &verb.words) else { continue };
            let things: Alts<Option<Thing>> = match verb.thing {
                None => vec![(None, j)],
                Some((list, counted)) => self.thing(j, list, counted).into_iter().map(|(t, k)| (Some(t), k)).collect(),
            };
            for (thing, k) in things {
                let targets: Alts<Option<Target>> = match verb.target {
                    None => vec![(None, k)],
                    Some(slot) => self.target(k, verb, slot),
                };
                for (target, m) in targets {
                    out.push((Order::Verb { verb: verb.words.clone(), thing: thing.clone(), target: target.map(Box::new) }, m));
                }
            }
        }
        out
    }

    fn thing(&mut self, i: usize, list: ThingList, counted: bool) -> Alts<Thing> {
        let mut out = Vec::new();
        if list == ThingList::Production {
            out.extend(self.thing(i, ThingList::Types, false));
            out.extend(self.thing(i, ThingList::Upgrades, false));
            return out;
        }
        if list == ThingList::Types {
            let types = &self.vocab.types;
            for n in types {
                if let Some(j) = self.phrase_as(i, &n.singular, "<unit type>") {
                    out.push((Thing { name: n.singular.clone(), count: 1 }, j));
                }
            }
            if counted {
                if let Some((count, j)) = self.count(i) {
                    for n in types {
                        if let Some(k) = self.phrase_as(j, &n.plural, "<unit type>") {
                            out.push((Thing { name: n.singular.clone(), count }, k));
                        }
                    }
                }
            }
            return out;
        }
        let vocab = self.vocab;
        let (names, label) = match list {
            ThingList::Upgrades => (&vocab.upgrades, "<upgrade>"),
            ThingList::Powers => (&vocab.powers, "<power>"),
            ThingList::Modes => (&vocab.modes, "<weapon mode>"),
            ThingList::Weapons => (&vocab.weapons, "<weapon>"),
            _ => (&vocab.sciences, "<science>"),
        };
        self.names(i, names, label).into_iter().map(|(name, j)| (Thing { name, count: 1 }, j)).collect()
    }
}

/// Which number a noun's words can be.
#[derive(Clone, Copy)]
struct Forms {
    singular: bool,
    plural: bool,
    mass: bool,
}

/// A noun's distinct written forms, each with whether it is the plural.
fn noun_forms(n: &Noun) -> Vec<(&str, bool)> {
    if n.is_mass() {
        vec![(n.singular.as_str(), true)]
    } else {
        vec![(n.singular.as_str(), false), (n.plural.as_str(), true)]
    }
}

/// The quantity a determiner, superlative and noun form spell, or `None` if they don't agree.
fn quantity(det: Det, superlative: Option<Superlative>, f: Forms) -> Option<Quantity> {
    let sup = superlative.is_some();
    match det {
        Det::None if !sup && f.plural => Some(Quantity::All),
        Det::The if f.singular => Some(Quantity::One),
        Det::TheCount(n) if sup && f.plural => Some(Quantity::Count(n)),
        Det::Count(n) if !sup && f.plural => Some(Quantity::Count(n)),
        Det::Half { .. } if !sup && f.plural => Some(Quantity::Half),
        Det::MyCount(n) if f.plural => Some(Quantity::Count(n)),
        // `my`: a singular is one, a plural all; a mass noun is all unless a superlative makes it one.
        Det::My if f.mass => Some(if sup { Quantity::One } else { Quantity::All }),
        Det::My if f.singular => Some(Quantity::One),
        Det::My if f.plural && !sup => Some(Quantity::All),
        _ => None,
    }
}

/// Whether a location suits a verb's target slot: an object-only slot takes an object (or the
/// General) with no offset.
fn fits(slot: TargetSlot, loc: &Location) -> bool {
    let is_object = loc.offset.is_none() && matches!(loc.point, Point::Me | Point::Object(_));
    (slot.object && is_object) || slot.point
}

/// A count or group number: ASCII digits with no leading zero.
fn number(t: &str) -> Option<u32> {
    if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) || (t.len() > 1 && t.starts_with('0')) {
        return None;
    }
    t.parse().ok()
}
