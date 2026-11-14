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

/// English words a renamed vocabulary's names are made of (`granite tower`, `wardens`): a match's
/// names are English (`Ranger`, `War Factory`), and a model that only ever saw invented words in
/// a name's place reads an English one as noise. None is one of Zero Hour's names, a word of the
/// grammar or of the synonym table (`english_name_words_collide_with_nothing`).
const ENGLISH_NAME_WORDS: [&str; 158] = [
    "falcon", "hawk", "kestrel", "osprey", "condor", "heron", "raven", "crow", "magpie", "sparrow", "harrier", "buzzard", "vulture",
    "owl", "wolf", "jackal", "coyote", "badger", "wolverine", "lynx", "cougar", "puma", "jaguar", "leopard", "bobcat", "bear",
    "bison", "buffalo", "mustang", "stallion", "bronco", "ram", "boar", "rhino", "moose", "elk", "stag", "viper", "cobra", "adder",
    "python", "mamba", "gecko", "iguana", "hornet", "wasp", "mantis", "beetle", "locust", "spider", "tarantula", "termite",
    "piranha", "barracuda", "marlin", "shark", "orca", "granite", "basalt", "flint", "slate", "marble", "quartz", "onyx", "jade",
    "garnet", "topaz", "opal", "amber", "cobalt", "copper", "bronze", "nickel", "zinc", "titan", "atlas", "orion", "sirius",
    "vega", "nova", "comet", "meteor", "quasar", "pulsar", "nebula", "zenith", "apex", "summit", "glacier", "tundra", "canyon",
    "mesa", "bayou", "harbor", "ember", "cinder", "blaze", "frost", "sleet", "thunder", "lightning", "tempest", "cyclone",
    "typhoon", "monsoon", "gale", "squall", "sabre", "cutlass", "rapier", "halberd", "lance", "pike", "javelin", "trident",
    "mace", "flail", "glaive", "longbow", "crossbow", "ballista", "catapult", "trebuchet", "warden", "marshal", "templar",
    "vanguard", "warlord", "sheriff", "guardian", "centurion", "legion", "phalanx", "cohort", "brigand", "outlaw", "nomad",
    "drifter", "bandit", "corsair", "buccaneer", "privateer", "musket", "howitzer", "bastion", "citadel", "redoubt", "rampart",
    "pioneer", "voyager", "mariner", "falconer", "lancer", "hussar", "dragoon", "grenadier", "fusilier", "sapper",
];

/// A renaming of a vocabulary's invented words into English ones, each to its own word, applied
/// alike to the vocabulary, a command and a paraphrase (whose names are verbatim).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Renaming(Vec<(String, String)>);

impl Renaming {
    /// A fresh renaming of every invented word in `vocab`'s names.
    pub fn english(rng: &mut Rng, vocab: &Vocabulary) -> Renaming {
        let mut words: Vec<String> = Vec::new();
        let names = vocab.types.iter().map(|n| &n.singular).chain(vocab.places.iter()).chain(&vocab.teams).chain(&vocab.upgrades).chain(&vocab.powers).chain(&vocab.sciences).chain(&vocab.modes).chain(&vocab.weapons);
        for name in names {
            for w in name.split(' ') {
                if is_invented(w) && !words.iter().any(|x| x == w) {
                    words.push(w.to_string());
                }
            }
        }
        let mut pool: Vec<&str> = ENGLISH_NAME_WORDS.to_vec();
        Renaming(
            words
                .into_iter()
                .filter_map(|w| {
                    (!pool.is_empty()).then(|| {
                        let e = pool.swap_remove(below(rng, pool.len()));
                        (w, e.to_string())
                    })
                })
                .collect(),
        )
    }

    /// `text` with each invented word renamed: as itself, its plural, or its possessive, with any
    /// punctuation around it (a paraphrase's `korgors,`) kept.
    pub fn text(&self, text: &str) -> String {
        let one = |w: &str| -> String {
            for (from, to) in &self.0 {
                if w == from {
                    return to.clone();
                }
                if w == plural(from) {
                    return plural(to);
                }
                if w == format!("{from}'s") {
                    return format!("{to}'s");
                }
            }
            w.to_string()
        };
        text.split(' ')
            .map(|t| {
                let start = t.find(|c: char| c.is_alphanumeric()).unwrap_or(t.len());
                let end = t.rfind(|c: char| c.is_alphanumeric() || c == '\'').map_or(start, |i| i + 1).max(start);
                format!("{}{}{}", &t[..start], one(&t[start..end]), &t[end..])
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `vocab` with every name renamed.
    pub fn vocabulary(&self, vocab: &Vocabulary) -> Vocabulary {
        let list = |l: &[String]| l.iter().map(|n| self.text(n)).collect::<Vec<_>>();
        Vocabulary {
            types: vocab.types.iter().map(|n| Noun::new(&self.text(&n.singular), &self.text(&n.plural))).collect(),
            places: list(&vocab.places),
            teams: list(&vocab.teams),
            upgrades: list(&vocab.upgrades),
            powers: list(&vocab.powers),
            sciences: list(&vocab.sciences),
            modes: list(&vocab.modes),
            weapons: list(&vocab.weapons),
            ..vocab.clone()
        }
    }
}

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

/// A vocabulary for the large training corpus: [`invented_vocabulary`] with more kinds of name.
/// About half are renamed into English words ([`Renaming`]); some get a sound-alike pair (a type
/// whose invented word differs from another's by one syllable, so the model must hear the
/// difference); some carry a digit or a letter acronym (`mk2 korgor tank`, `ecm tank`), as mods'
/// and real games' names do. Returned with the renaming applied, never with grammar collisions.
pub fn corpus_vocabulary(rng: &mut Rng) -> Vocabulary {
    loop {
        let mut v = invented_vocabulary(rng);
        // A sound-alike: one type's invented word with one syllable swapped, as another type.
        if chance(rng, 50) {
            if let Some(n) = v.types.iter().find(|n| n.singular.split(' ').any(is_invented)).cloned() {
                let word = n.singular.split(' ').find(|w| is_invented(w)).unwrap_or("").to_string();
                let syl: Vec<&str> = SYLLABLES.iter().copied().filter(|s| word.contains(s)).collect();
                if let Some(from) = syl.first() {
                    let to = pick(rng, &SYLLABLES);
                    let alike = n.singular.replacen(from, to, 1);
                    if alike != n.singular && !v.types.iter().any(|t| t.singular == alike) {
                        v.types.push(Noun::new(&alike, &plural(&alike)));
                    }
                }
            }
        }
        // A marked variant: a digit (`mk2`) or a letter acronym before a unit noun.
        if chance(rng, 15) {
            let mark = if chance(rng, 50) { format!("mk{}", 2 + below(rng, 3)) } else { ["ecm", "sam", "apc", "emp", "rpg"][below(rng, 5)].to_string() };
            let name = format!("{mark} {}", pick(rng, &UNIT_NOUNS));
            if !v.types.iter().any(|t| t.singular == name) {
                v.types.push(Noun::new(&name, &plural(&name)));
            }
        }
        if chance(rng, 50) {
            let r = Renaming::english(rng, &v);
            v = r.vocabulary(&v);
        }
        if v.problems().is_empty() {
            return v;
        }
    }
}

/// What a command contains, as tags the corpus balances on: its verbs (`verb:<words>`), and
/// `implied` (no selection), `multi` (several groups), `except`, `then`, `them`, `that`, `team`,
/// `group`, `class`, `units`, `count`, `half`, `superlative`, `adjective`, `near`, `region`,
/// `my group`, `place`, `me`, `there`, `offset`, `facing`, `line`, `thing count`, `name them`,
/// `make group`, `enemy`, `own`, `allied`, `neutral`.
pub fn features(cmd: &Command) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut tag = |t: &str| {
        if !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    };
    fn query(q: &Query, tag: &mut dyn FnMut(&str)) {
        match q.base {
            Base::Class(_) => tag("class"),
            Base::Units => tag("units"),
            Base::Type(_) => {}
        }
        match q.quantity {
            Quantity::Count(_) => tag("count"),
            Quantity::Half => tag("half"),
            _ => {}
        }
        if q.superlative.is_some() {
            tag("superlative");
        }
        if !q.adjectives.is_empty() {
            tag("adjective");
        }
        if let Some(p) = &q.filters.near {
            tag("near");
            point(p, tag);
        }
        if q.filters.region.is_some() {
            tag("region");
        }
        if q.filters.my_group {
            tag("my group");
        }
    }
    fn point(p: &Point, tag: &mut dyn FnMut(&str)) {
        match p {
            Point::Me => tag("me"),
            Point::There => tag("there"),
            Point::Place(_) => tag("place"),
            Point::Object(ObjectRef::That) => tag("that"),
            Point::Object(ObjectRef::Query { relation, query: q }) => {
                match relation {
                    Relation::Enemy => tag("enemy"),
                    Relation::Own => tag("own"),
                    Relation::Allied => tag("allied"),
                    Relation::Neutral | Relation::Civilian => tag("neutral"),
                    Relation::Any => {}
                }
                query(q, tag);
            }
        }
    }
    match &cmd.selection {
        None => tag("implied"),
        Some(sel) => {
            if sel.include.len() > 1 {
                tag("multi");
            }
            if !sel.exclude.is_empty() {
                tag("except");
            }
            for g in sel.include.iter().chain(&sel.exclude) {
                match g {
                    Group::Query(q) => query(q, &mut tag),
                    Group::ControlGroup(_) => tag("group"),
                    Group::Team(_) => tag("team"),
                    Group::Them => tag("them"),
                    Group::That => tag("that"),
                }
            }
        }
    }
    if cmd.orders.len() > 1 {
        tag("then");
    }
    for o in &cmd.orders {
        match o {
            Order::NameThem(_) => tag("name them"),
            Order::MakeGroup(_) => tag("make group"),
            Order::Verb { verb, thing, target } => {
                tag(&format!("verb:{verb}"));
                if thing.as_ref().is_some_and(|t| t.count > 1) {
                    tag("thing count");
                }
                if let Some(t) = target {
                    if t.location.offset.is_some() {
                        tag("offset");
                    }
                    point(&t.location.point, &mut tag);
                    if t.facing.is_some() {
                        tag("facing");
                    }
                    if let Some(end) = &t.end {
                        tag("line");
                        point(&end.point, &mut tag);
                    }
                }
            }
        }
    }
    out
}

/// The corpus's balance: each verb's share (the sampler's weights, but no verb under 1%), each
/// length's share (in words, most commands short), and a floor for every rarer feature. A drawn
/// command is accepted while its verb and length are under their share; near the end, only
/// commands that bring a feature still under its floor.
pub struct Quotas {
    target: usize,
    verbs: std::collections::BTreeMap<String, f64>,
    lengths: [f64; 5],
    floors: Vec<(&'static str, f64)>,
    pub verb_counts: std::collections::BTreeMap<String, usize>,
    pub length_counts: [usize; 5],
    pub feature_counts: std::collections::BTreeMap<String, usize>,
    pub accepted: usize,
}

impl Quotas {
    pub fn new(target: usize) -> Quotas {
        let verbs = fixed_verbs();
        let total: f64 = verbs.iter().map(|v| verb_weight(&v.words) as f64).sum();
        let mut shares: std::collections::BTreeMap<String, f64> = verbs.iter().map(|v| (v.words.clone(), (verb_weight(&v.words) as f64 / total).max(0.01))).collect();
        let sum: f64 = shares.values().sum();
        shares.values_mut().for_each(|s| *s /= sum);
        Quotas {
            target,
            verbs: shares,
            lengths: [0.15, 0.35, 0.30, 0.15, 0.05],
            floors: vec![
                ("implied", 0.15),
                ("then", 0.10),
                ("superlative", 0.10),
                ("count", 0.08),
                ("place", 0.08),
                ("adjective", 0.08),
                ("multi", 0.08),
                ("except", 0.06),
                ("group", 0.06),
                ("them", 0.06),
                ("that", 0.06),
                ("near", 0.06),
                ("offset", 0.06),
                ("team", 0.04),
                ("region", 0.04),
                ("half", 0.03),
                ("my group", 0.03),
                ("facing", 0.02),
                ("line", 0.01),
                ("name them", 0.01),
                ("make group", 0.01),
            ],
            verb_counts: Default::default(),
            length_counts: [0; 5],
            feature_counts: Default::default(),
            accepted: 0,
        }
    }

    fn bucket(words: usize) -> usize {
        match words {
            0..=3 => 0,
            4..=6 => 1,
            7..=10 => 2,
            11..=15 => 3,
            _ => 4,
        }
    }

    /// Why a command with these `features` and `words` would be refused, if it would.
    pub fn verdict(&self, features: &[String], words: usize) -> Result<(), &'static str> {
        let left = self.target.saturating_sub(self.accepted);
        if left == 0 {
            return Err("full");
        }
        // A command bringing a feature still under its floor may overshoot its verb's and length's
        // share (a building's facing is only ever `build`'s): floors come first.
        let missing: Vec<&str> = self.floors.iter().filter(|(f, share)| (*self.feature_counts.get(*f).unwrap_or(&0) as f64) < share * self.target as f64).map(|(f, _)| *f).collect();
        let helps = features.iter().any(|f| missing.contains(&f.as_str()));
        let (verb_slack, length_slack) = if helps { (1.6, 1.3) } else { (1.05, 1.05) };
        let b = Self::bucket(words);
        if self.length_counts[b] as f64 >= self.lengths[b] * self.target as f64 * length_slack {
            return Err("length");
        }
        for f in features.iter().filter_map(|f| f.strip_prefix("verb:")) {
            let share = self.verbs.get(f).copied().unwrap_or(0.01);
            if *self.verb_counts.get(f).unwrap_or(&0) as f64 >= share * self.target as f64 * verb_slack {
                return Err("verb");
            }
        }
        // Once what is still missing nears the room left, take only commands that help.
        let deficit: f64 = self.floors.iter().map(|(f, share)| (share * self.target as f64 - *self.feature_counts.get(*f).unwrap_or(&0) as f64).max(0.0)).sum();
        if deficit >= left as f64 * 0.5 && !helps {
            return Err("floors");
        }
        Ok(())
    }

    /// Whether to keep a command with these `features` and `words` (and count it if so).
    pub fn offer(&mut self, features: &[String], words: usize) -> bool {
        if self.verdict(features, words).is_err() {
            return false;
        }
        let b = Self::bucket(words);
        self.accepted += 1;
        self.length_counts[b] += 1;
        for f in features {
            match f.strip_prefix("verb:") {
                Some(v) => *self.verb_counts.entry(v.to_string()).or_default() += 1,
                None => *self.feature_counts.entry(f.clone()).or_default() += 1,
            }
        }
        true
    }

    /// The feature the sampler should aim for next (`Sampler::focus`): of those it can aim for,
    /// the one furthest behind its floor's pace (its count against its share of what has been
    /// accepted so far), if any is behind.
    pub fn focus(&self) -> Vec<String> {
        let pace = |f: &str| -> f64 {
            let share = self.floors.iter().find(|(g, _)| *g == f).map_or(0.0, |(_, s)| *s);
            let want = share * (self.accepted.max(20)) as f64;
            *self.feature_counts.get(f).unwrap_or(&0) as f64 / want.max(1e-9)
        };
        // Short commands lag behind their length's share as plain draws seldom make them; once
        // their length is full, aiming at them would only be refused.
        let short_full = self.length_counts[0] as f64 >= self.lengths[0] * self.target as f64;
        let short_pace = if short_full { f64::INFINITY } else { self.length_counts[0] as f64 / (self.lengths[0] * self.accepted.max(20) as f64) };
        ["facing", "line", "name them", "make group", "then", "place"]
            .iter()
            .map(|f| (pace(f), *f))
            .chain(std::iter::once((short_pace, "short")))
            .filter(|(p, _)| *p < 0.9)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, f)| vec![f.to_string()])
            .unwrap_or_default()
    }

    /// The floors not met, as `feature have/want`.
    pub fn short(&self) -> Vec<String> {
        self.floors
            .iter()
            .filter_map(|(f, share)| {
                let have = *self.feature_counts.get(*f).unwrap_or(&0);
                let want = (share * self.target as f64) as usize;
                (have < want).then(|| format!("{f} {have}/{want}"))
            })
            .collect()
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
    /// Features to aim for (`facing`, `line`, `name them`, `make group`, `then`, `place`): verbs that take them
    /// and better odds for them, when a corpus is short of them. Empty, the draws are as always.
    pub focus: Vec<String>,
    /// The chance, in percent, that a query carries each adjective (6 by default; the corpus
    /// draws fewer: real commands rarely stack `idle damaged veteran`).
    pub adjective_percent: u32,
}

impl<'a> Sampler<'a> {
    pub fn new(rng: Rng, vocab: &'a Vocabulary) -> Sampler<'a> {
        Sampler { rng, vocab, focus: Vec::new(), adjective_percent: 6 }
    }

    fn focused(&self, feature: &str) -> bool {
        self.focus.iter().any(|f| f == feature)
    }

    fn chance(&mut self, percent: u32) -> bool {
        chance(&mut self.rng, percent)
    }

    fn below(&mut self, n: usize) -> usize {
        below(&mut self.rng, n)
    }

    pub fn command(&mut self) -> Command {
        if self.focused("short") {
            return self.short();
        }
        let selection = self.chance(75).then(|| self.selection());
        let first = self.order();
        let mut orders = vec![first];
        if self.focused("then") || self.chance(12) {
            // Mostly a queued move after the first (the resolver queues moves only).
            let next = if self.chance(70) { self.verb_order(&fixed_verbs().into_iter().find(|v| v.words == "move").unwrap()) } else { self.order() };
            orders.push(next);
        }
        Command { selection, orders }
    }

    /// A short command, as players give most: no selection or a bare one, a verb, at most a
    /// plain target (`tanks attack that`, `stop`, `units move to me`).
    fn short(&mut self) -> Command {
        let selection = (!self.chance(40)).then(|| {
            let g = match self.below(10) {
                0..=5 => {
                    let n = &self.vocab.types[self.below(self.vocab.types.len())];
                    let _ = n;
                    Group::Query(Query {
                        quantity: Quantity::All,
                        superlative: None,
                        adjectives: Vec::new(),
                        base: if self.chance(70) { Base::Type(self.vocab.types[self.below(self.vocab.types.len())].singular.clone()) } else { Base::Units },
                        filters: Filters::default(),
                    })
                }
                6 => Group::Them,
                7 => Group::That,
                _ => Group::ControlGroup(self.below(10) as u8),
            };
            Selection { include: vec![g], exclude: Vec::new() }
        });
        let verbs: Vec<VerbDef> = fixed_verbs().into_iter().filter(|v| v.thing.is_none() && v.target.is_none_or(|t| !t.line)).collect();
        let total: u32 = verbs.iter().map(|v| verb_weight(&v.words)).sum();
        let mut roll = self.rng.next_u32() % total.max(1);
        let verb = verbs
            .iter()
            .find(|v| {
                let w = verb_weight(&v.words);
                if roll < w {
                    true
                } else {
                    roll -= w;
                    false
                }
            })
            .unwrap_or(&verbs[0])
            .clone();
        let target = verb.target.and_then(|slot| {
            if !slot.required && self.chance(50) {
                return None;
            }
            let point = match self.below(3) {
                0 if slot.point => Point::Me,
                1 if slot.point => Point::There,
                _ => Point::Object(ObjectRef::That),
            };
            Some(Box::new(Target { location: Location { offset: None, point }, facing: None, end: None }))
        });
        Command { selection, orders: vec![Order::Verb { verb: verb.words.clone(), thing: None, target }] }
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
        let percent = self.adjective_percent;
        let adjectives = Adjective::ALL.iter().copied().filter(|_| chance(&mut self.rng, percent)).collect();
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
            _ if self.focused("place") && !self.vocab.places.is_empty() => Point::Place(self.vocab.places[self.below(self.vocab.places.len())].clone()),
            44..=55 if !self.vocab.places.is_empty() => Point::Place(self.vocab.places[self.below(self.vocab.places.len())].clone()),
            _ => Point::Object(self.object(filters)),
        }
    }

    fn order(&mut self) -> Order {
        if self.focused("name them") {
            return Order::NameThem(format!("{} {}", word(&mut self.rng), pick(&mut self.rng, &TEAM_NOUNS)));
        }
        if self.focused("make group") {
            return Order::MakeGroup(self.below(10) as u8);
        }
        match self.below(100) {
            0..=1 => Order::NameThem(format!("{} {}", word(&mut self.rng), pick(&mut self.rng, &TEAM_NOUNS))),
            2..=3 => Order::MakeGroup(self.below(10) as u8),
            _ => {
                let (facing, line) = (self.focused("facing"), self.focused("line"));
                let verbs: Vec<VerbDef> = fixed_verbs()
                    .into_iter()
                    .filter(|v| v.thing.is_none_or(|(list, _)| !self.vocab.things(list).is_empty()))
                    .filter(|v| (!facing || v.target.is_some_and(|t| t.facing)) && (!line || v.target.is_some_and(|t| t.line)))
                    .collect();
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
            let (facing, line) = (self.focused("facing"), self.focused("line"));
            if !slot.required && !(facing || line) && self.chance(45) {
                return None;
            }
            if slot.line && self.chance(if line { 70 } else { 5 }) {
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
            let facing = (slot.facing && self.chance(if facing { 70 } else { 10 })).then(|| Direction::ALL[self.below(8)]);
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

    /// Zero Hour's names' words (units, structures, Generals), which no renamed name may use.
    const ZERO_HOUR_WORDS: [&str; 52] = [
        "ranger", "crusader", "humvee", "paladin", "overlord", "comanche", "raptor", "aurora", "dragon", "scorpion", "marauder",
        "avenger", "chinook", "battlemaster", "inferno", "helix", "spectre", "tomahawk", "ambulance", "dozer", "rebel", "hijacker",
        "saboteur", "hacker", "lotus", "jarmen", "kell", "burton", "pathfinder", "technical", "toxin", "quad", "gattling", "stinger",
        "palace", "firebase", "nuke", "nuclear", "scud", "tunnel", "propaganda", "barracks", "airfield", "minigunner", "sentry",
        "colonel", "nationalist", "microwave", "laser", "listening", "demo", "carpet",
    ];

    #[test]
    fn english_name_words_collide_with_nothing() {
        let tables = super::super::normal::Tables::parse(&[super::super::normal::SHIPPED]);
        let mut taken: Vec<String> = super::super::vocab::fixed_words().iter().map(|w| w.to_string()).collect();
        taken.extend(fixed_verbs().iter().flat_map(|v| v.words.split(' ').map(str::to_string).collect::<Vec<_>>()));
        taken.extend(tables.words.iter().flat_map(|(k, r)| k.iter().cloned().chain(r.iter().flat_map(|x| x.split(' ').map(str::to_string)))));
        taken.extend(tables.aliases.iter().flat_map(|(a, m)| a.split(' ').chain(m.iter().flat_map(|x| x.split(' '))).map(str::to_string).collect::<Vec<_>>()));
        taken.extend(CLASSES.iter().flat_map(|(s, p, _)| [s.to_string(), p.to_string()]));
        for w in ENGLISH_NAME_WORDS {
            assert!(!taken.iter().any(|t| t == w), "{w} is a word of the grammar or the synonym table");
            assert!(!ZERO_HOUR_WORDS.iter().any(|z| w.contains(z)), "{w} is one of Zero Hour's names");
            assert!(!is_invented(w), "{w} reads as invented");
        }
        let unique: std::collections::BTreeSet<&str> = ENGLISH_NAME_WORDS.iter().copied().collect();
        assert_eq!(unique.len(), ENGLISH_NAME_WORDS.len());
    }

    #[test]
    fn a_renamed_vocabulary_reads_its_renamed_commands() {
        let mut rng = Rng::new(11);
        for i in 0..300 {
            let v = invented_vocabulary(&mut rng);
            let r = Renaming::english(&mut rng, &v);
            let e = r.vocabulary(&v);
            assert!(e.problems().is_empty(), "#{i}: {:?}", e.problems());
            let mut s = Sampler::new(Rng::new(rng.next_u32()), &v);
            let text = print(&s.command(), &v);
            let renamed = r.text(&text);
            let back = parse(&renamed, &e).unwrap_or_else(|err| panic!("#{i}: '{renamed}' (from '{text}'): {err}"));
            assert_eq!(print(&back, &e), renamed);
            assert!(e.types.iter().all(|n| !n.singular.split(' ').any(is_invented)), "#{i}: {:?}", e.types);
            if let Some(n) = v.types.first() {
                let said = format!("uh, {}, go there.", n.plural);
                assert_eq!(r.text(&said), format!("uh, {}, go there.", e.types[0].plural));
            }
        }
    }

    #[test]
    fn corpus_vocabularies_have_the_new_name_styles() {
        let mut rng = Rng::new(21);
        let (mut english, mut marked, mut alike) = (0, 0, 0);
        for _ in 0..200 {
            let v = corpus_vocabulary(&mut rng);
            assert!(v.problems().is_empty(), "{:?}", v.problems());
            english += v.types.iter().all(|n| !n.singular.split(' ').any(is_invented)) as usize;
            marked += v.types.iter().any(|n| n.singular.starts_with("mk") || ["ecm ", "sam ", "apc ", "emp ", "rpg "].iter().any(|m| n.singular.starts_with(m))) as usize;
            alike += v.types.iter().enumerate().any(|(i, a)| v.types.iter().skip(i + 1).any(|b| a.singular.len() == b.singular.len() && a.singular.chars().zip(b.singular.chars()).filter(|(x, y)| x != y).count() <= 4 && a.singular != b.singular)) as usize;
        }
        assert!(english > 60 && marked > 10 && alike > 40, "english {english}, marked {marked}, alike {alike}");
    }

    #[test]
    fn the_quotas_balance_a_draw() {
        let mut rng = Rng::new(31);
        let mut q = Quotas::new(3000);
        let mut refused: std::collections::BTreeMap<(String, &str, bool), usize> = Default::default();
        let mut tries = 0;
        while q.accepted < 3000 && tries < 400_000 {
            tries += 1;
            let v = invented_vocabulary(&mut rng);
            let mut s = Sampler::new(Rng::new(rng.next_u32()), &v);
            s.focus = q.focus();
            let focus = s.focus.clone();
            let cmd = s.command();
            let words = print(&cmd, &v).split(' ').count();
            let f = features(&cmd);
            if let (Some(want), Err(why)) = (focus.first(), q.verdict(&f, words)) {
                *refused.entry((want.clone(), why, f.iter().any(|x| x == want))).or_insert(0usize) += 1;
            }
            q.offer(&f, words);
        }
        assert_eq!(q.accepted, 3000, "short of features: {:?}; focused draws refused (feature, why, had it): {refused:?}; lengths {:?}", q.short(), q.length_counts);
        assert!(q.short().is_empty(), "{:?}", q.short());
        // No verb dominates: the commonest is at most its share and a little.
        let top = q.verb_counts.values().max().copied().unwrap_or(0);
        assert!(top < 3000 / 6, "{:?}", q.verb_counts);
    }

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
            let mut s = Sampler::new(Rng::new(rng.next_u32()), &v);
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
