//! Canonical text from a command AST: the one spelling of each command (docs/COMMANDS.md section
//! 4). [`super::parse`] reads exactly this text back.

use super::ast::*;
use super::vocab::Vocabulary;

/// The canonical text of `cmd`. The vocabulary supplies the plural forms and the verbs'
/// prepositions; a name it does not hold is printed as stored (the parser would then refuse it).
pub fn print(cmd: &Command, vocab: &Vocabulary) -> String {
    let mut p = Printer { vocab, words: Vec::new() };
    p.command(cmd);
    p.words.join(" ")
}

/// One selection group's canonical text (`idle rangers`, `group 3`), for messages.
pub fn print_group(g: &Group, vocab: &Vocabulary) -> String {
    let mut p = Printer { vocab, words: Vec::new() };
    p.groups(std::slice::from_ref(g));
    p.words.join(" ")
}

/// A point's canonical text (`the nearest enemy war factory`), for messages.
pub fn print_point(pt: &Point, vocab: &Vocabulary) -> String {
    let mut p = Printer { vocab, words: Vec::new() };
    p.point(pt);
    p.words.join(" ")
}

/// One order's canonical text, for messages.
pub fn print_order(order: &Order, vocab: &Vocabulary) -> String {
    let mut p = Printer { vocab, words: Vec::new() };
    p.order(order);
    p.words.join(" ")
}

struct Printer<'a> {
    vocab: &'a Vocabulary,
    words: Vec<String>,
}

impl Printer<'_> {
    fn push(&mut self, w: &str) {
        self.words.push(w.to_string());
    }

    fn command(&mut self, cmd: &Command) {
        if let Some(sel) = &cmd.selection {
            self.selection(sel);
        }
        for (i, order) in cmd.orders.iter().enumerate() {
            if i > 0 {
                self.push("then");
            }
            self.order(order);
        }
    }

    fn selection(&mut self, sel: &Selection) {
        self.groups(&sel.include);
        if !sel.exclude.is_empty() {
            self.push("except");
            self.groups(&sel.exclude);
        }
    }

    fn groups(&mut self, groups: &[Group]) {
        for (i, g) in groups.iter().enumerate() {
            if i > 0 {
                self.push("and");
            }
            match g {
                Group::Query(q) => self.query(Relation::Any, q),
                Group::ControlGroup(n) => {
                    self.push("group");
                    self.push(&n.to_string());
                }
                Group::Team(name) => self.push(name),
                Group::Them => self.push("them"),
                Group::That => self.push("that"),
            }
        }
    }

    /// `[determiner] [superlative] {adjective} [relation] noun {filter}`. The determiner carries
    /// the quantity and, for [`Relation::Own`], the relation (`my`).
    fn query(&mut self, relation: Relation, q: &Query) {
        let own = relation == Relation::Own;
        match q.quantity {
            Quantity::All => {
                if own {
                    self.push("my");
                }
            }
            Quantity::One => self.push(if own { "my" } else { "the" }),
            Quantity::Count(n) => {
                if own {
                    self.push("my");
                } else if q.superlative.is_some() {
                    self.push("the");
                }
                self.push(&n.to_string());
            }
            Quantity::Half => {
                self.push("half");
                self.push(if own { "my" } else { "the" });
            }
        }
        if let Some(s) = q.superlative {
            self.push(s.word());
        }
        for a in &q.adjectives {
            self.push(a.word());
        }
        if let Some(w) = relation.word() {
            self.push(w);
        }
        let singular = q.quantity == Quantity::One;
        let vocab = self.vocab;
        match &q.base {
            Base::Type(name) => self.noun(&vocab.types, name, singular),
            Base::Class(name) => self.noun(&vocab.classes, name, singular),
            Base::Units => self.push(if singular { "unit" } else { "units" }),
        }
        self.filters(&q.filters);
    }

    fn noun(&mut self, list: &[super::vocab::Noun], singular_form: &str, singular: bool) {
        let word = match list.iter().find(|n| n.singular == singular_form) {
            Some(n) if !singular => n.plural.clone(),
            _ => singular_form.to_string(),
        };
        self.words.push(word);
    }

    fn filters(&mut self, f: &Filters) {
        if let Some(p) = &f.near {
            self.push("near");
            self.point(p);
        }
        if let Some(r) = f.region {
            self.push("in");
            self.push("the");
            self.push(r.word());
            self.push("group");
        }
        if f.my_group {
            self.push("in");
            self.push("my");
            self.push("group");
        }
    }

    fn point(&mut self, p: &Point) {
        match p {
            Point::Me => self.push("me"),
            Point::There => self.push("there"),
            Point::Object(ObjectRef::That) => self.push("that"),
            Point::Object(ObjectRef::Query { relation, query }) => self.query(*relation, query),
            Point::Place(name) => self.push(name),
        }
    }

    fn location(&mut self, loc: &Location) {
        if let Some(o) = loc.offset {
            self.push(o.words());
        }
        self.point(&loc.point);
    }

    fn order(&mut self, order: &Order) {
        match order {
            Order::NameThem(name) => {
                self.push("name");
                self.push("them");
                self.push(name);
            }
            Order::MakeGroup(n) => {
                self.push("make");
                self.push("them");
                self.push("group");
                self.push(&n.to_string());
            }
            Order::Verb { verb, thing, target } => {
                self.push(verb);
                if let Some(t) = thing {
                    if t.count > 1 {
                        self.push(&t.count.to_string());
                        let vocab = self.vocab;
                        self.noun(&vocab.types, &t.name, false);
                    } else {
                        self.push(&t.name);
                    }
                }
                if let Some(t) = target {
                    if !drops_prep(&t.location) {
                        if let Some(prep) = self.vocab.verb(verb).and_then(|v| v.prep.as_deref()) {
                            self.push(prep);
                        }
                    }
                    self.location(&t.location);
                    if let Some(d) = t.facing {
                        self.push("facing");
                        self.push(d.word());
                    }
                }
            }
        }
    }
}

/// A verb's preposition is left out before `there` and before an offset, which is a preposition
/// itself (`move there`, `move north of the bridge`, `build power plant near my war factory`).
pub fn drops_prep(loc: &Location) -> bool {
    loc.offset.is_some() || loc.point == Point::There
}
