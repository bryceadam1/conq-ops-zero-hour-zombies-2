//! The accepted input around the canonical language (docs/COMMANDS.md section 4, "Synonyms and
//! aliases"): the tables of `data/ConquerOps/commands.ini` (and a player's own
//! `ConquerOps.commands.ini`), and the normaliser that turns a line into canonical candidates.
//!
//! The normaliser is a table lookup, longest phrase first. A phrase with several readings yields
//! one candidate per combination, in the tables' order, so the session can try each in turn
//! ([`super::session::run`]: the first that parses and resolves wins, and two that resolve to
//! different commands are put to the player). An alias for several types stands for all of them
//! where a bare plural goes.

use std::sync::Mutex;

use super::vocab::Vocabulary;

/// The shipped tables: the same file the mod's BIG carries, so the built-in default and the game
/// never differ.
pub const SHIPPED: &str = include_str!("../../../../data/ConquerOps/commands.ini");

/// The most candidates one line may yield (each combination of its ambiguous phrases' readings).
pub const MAX_CANDIDATES: usize = 32;

/// The words that make a following alias something other than a bare plural.
const DETERMINERS: [&str; 4] = ["the", "my", "half", "group"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tables {
    /// Each phrase (its words) and its readings (canonical words; empty: dropped).
    pub words: Vec<(Vec<String>, Vec<String>)>,
    /// Each alias and the canonical plurals it stands for.
    pub aliases: Vec<(String, Vec<String>)>,
}

/// A line's words as the normaliser reads them: lowercase, punctuation as spaces (a hyphen or an
/// apostrophe inside a word stays).
pub fn words(line: &str) -> Vec<String> {
    let cleaned: String = line.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '\'' { c } else { ' ' }).collect();
    cleaned.split_whitespace().map(|w| w.trim_matches(|c| c == '-' || c == '\'').to_string()).filter(|w| !w.is_empty()).collect()
}

impl Tables {
    /// The tables of one or more files, read in order: a later file's entry for a phrase or alias
    /// replaces an earlier one's.
    pub fn parse(texts: &[&str]) -> Tables {
        let mut t = Tables::default();
        for text in texts {
            let mut section = "";
            for line in text.lines() {
                let line = line.split(';').next().unwrap_or("").trim();
                if line.is_empty() {
                    continue;
                }
                if line.starts_with('[') && line.ends_with(']') {
                    section = if line.eq_ignore_ascii_case("[words]") { "words" } else if line.eq_ignore_ascii_case("[aliases]") { "aliases" } else { "" };
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else { continue };
                let key = words(key);
                if key.is_empty() {
                    continue;
                }
                match section {
                    "words" => {
                        let readings: Vec<String> = value.split('|').map(|r| r.trim()).map(|r| if r == "<none>" { String::new() } else { words(r).join(" ") }).collect();
                        t.words.retain(|(k, _)| *k != key);
                        t.words.push((key, readings));
                    }
                    "aliases" => {
                        let name = key.join(" ");
                        let members: Vec<String> = value.split(',').map(|m| words(m).join(" ")).filter(|m| !m.is_empty()).collect();
                        t.aliases.retain(|(k, _)| *k != name);
                        t.aliases.push((name, members));
                    }
                    _ => {}
                }
            }
        }
        // Longest phrases first, so `a couple of` wins over `a`.
        t.words.sort_by_key(|w| std::cmp::Reverse(w.0.len()));
        t
    }

    /// The canonical candidates for a line: the line as typed first, then each combination of its
    /// phrases' readings, with aliases expanded; no repeats, at most [`MAX_CANDIDATES`] + 1.
    pub fn candidates(&self, line: &str, vocab: &Vocabulary) -> Vec<String> {
        let toks = words(line);
        let mut out = vec![toks.join(" ")];
        // The match's names, longest first: a synonym never shadows one (a team called `my boys`, a
        // place called `the bridge`).
        let mut names: Vec<Vec<String>> = vocab
            .types
            .iter()
            .chain(&vocab.classes)
            .flat_map(|n| [n.singular.clone(), n.plural.clone()])
            .chain(vocab.places.iter().cloned())
            .chain(vocab.teams.iter().cloned())
            .map(|n| n.split(' ').map(str::to_string).collect())
            .collect();
        names.sort_by_key(|n: &Vec<String>| std::cmp::Reverse(n.len()));
        // Each stretch of the line: its readings (one, for a word the tables do not know or a name).
        let mut segments: Vec<&[String]> = Vec::new();
        let mut owned: Vec<Vec<String>> = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            if let Some(name) = names.iter().find(|n| toks[i..].starts_with(n)) {
                owned.push(vec![name.join(" ")]);
                i += name.len();
                continue;
            }
            match self.words.iter().find(|(k, _)| toks[i..].starts_with(k)) {
                Some((k, readings)) => {
                    owned.push(readings.clone());
                    i += k.len();
                }
                None => {
                    owned.push(vec![toks[i].clone()]);
                    i += 1;
                }
            }
        }
        for o in &owned {
            segments.push(o);
        }
        let mut combos: Vec<Vec<&str>> = vec![Vec::new()];
        for seg in segments {
            let mut next = Vec::new();
            for c in &combos {
                for r in seg {
                    if next.len() >= MAX_CANDIDATES {
                        break;
                    }
                    let mut c2 = c.clone();
                    if !r.is_empty() {
                        c2.push(r.as_str());
                    }
                    next.push(c2);
                }
            }
            combos = next;
        }
        for c in combos {
            let text = self.expand_aliases(&c.join(" "), vocab);
            if !out.contains(&text) {
                out.push(text);
            }
        }
        out
    }

    /// An alias where a bare plural goes, as the union of the match's types and classes it names
    /// (`idle tanks` → `idle crusaders and idle paladins`); after a determiner it stays as said.
    fn expand_aliases(&self, text: &str, vocab: &Vocabulary) -> String {
        let toks: Vec<&str> = text.split_whitespace().collect();
        let adjectives: Vec<&str> = super::ast::Adjective::ALL.iter().map(|a| a.word()).collect();
        let plurals: Vec<&str> = vocab.types.iter().chain(&vocab.classes).map(|n| n.plural.as_str()).collect();
        let mut out: Vec<String> = Vec::new();
        for (i, tok) in toks.iter().enumerate() {
            let Some((_, members)) = self.aliases.iter().find(|(k, _)| k == tok) else {
                out.push(tok.to_string());
                continue;
            };
            // The adjectives just before it go with each member.
            let mut start = i;
            while start > 0 && adjectives.contains(&toks[start - 1]) {
                start -= 1;
            }
            let determined = start > 0 && (DETERMINERS.contains(&toks[start - 1]) || toks[start - 1].bytes().all(|b| b.is_ascii_digit()));
            let present: Vec<&String> = members.iter().filter(|m| plurals.contains(&m.as_str())).collect();
            if determined || present.is_empty() {
                out.push(tok.to_string());
                continue;
            }
            let adj: Vec<&str> = toks[start..i].to_vec();
            out.truncate(out.len() - adj.len());
            let parts: Vec<String> = present.iter().map(|m| adj.iter().copied().chain(std::iter::once(m.as_str())).collect::<Vec<_>>().join(" ")).collect();
            out.push(parts.join(" and "));
        }
        out.join(" ")
    }
}

static TABLES: Mutex<Option<Tables>> = Mutex::new(None);

/// Load the mod's tables and the player's own (empty if he has none); until this runs the shipped
/// tables hold.
pub fn load(mod_text: &str, player_text: &str) {
    let t = Tables::parse(&[mod_text, player_text]);
    *TABLES.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
}

pub fn get() -> Tables {
    TABLES.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| Tables::parse(&[SHIPPED]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::vocab::Noun;

    fn vocab() -> Vocabulary {
        Vocabulary {
            types: vec![Noun::new("ranger", "rangers"), Noun::new("crusader", "crusaders"), Noun::new("paladin", "paladins")],
            classes: vec![Noun::new("builder", "builders"), Noun::new("infantry", "infantry")],
            ..Vocabulary::default()
        }
    }

    #[test]
    fn the_shipped_tables_parse_and_every_reading_is_canonical_words() {
        let t = Tables::parse(&[SHIPPED]);
        assert!(t.words.len() > 150 && !t.aliases.is_empty());
        for (k, readings) in &t.words {
            assert!(!readings.is_empty(), "{k:?}");
            for r in readings {
                assert!(r.is_empty() || r.split(' ').all(crate::command::vocab::is_word), "{k:?} = {r:?}");
            }
        }
        // Longest first.
        assert!(t.words.windows(2).all(|w| w[0].0.len() >= w[1].0.len()));
    }

    #[test]
    fn candidates_try_the_line_then_each_reading() {
        let t = Tables::parse(&[SHIPPED]);
        let v = vocab();
        let c = t.candidates("Everyone, move here!", &v);
        assert_eq!(c[0], "everyone move here");
        assert!(c.contains(&"units move to me".to_string()), "{c:?}");
        // `hold` is stop or guard: both are tried, stop first.
        let c = t.candidates("rangers hold", &v);
        assert_eq!(&c[1..], ["rangers stop", "rangers guard"]);
        // A phrase beats its first word; numbers become digits.
        let c = t.candidates("the closest ranger", &v);
        assert!(c.contains(&"the nearest ranger".to_string()), "{c:?}");
        let c = t.candidates("build three crusaders", &v);
        assert!(c.contains(&"build 3 crusaders".to_string()), "{c:?}");
        // `all` drops out: a bare plural is all of them.
        assert!(t.candidates("all rangers stop", &v).contains(&"rangers stop".to_string()));
    }

    #[test]
    fn an_alias_is_the_matchs_members_where_a_bare_plural_goes() {
        let t = Tables::parse(&[SHIPPED]);
        let v = vocab();
        let c = t.candidates("tanks move to me", &v);
        assert!(c.contains(&"crusaders and paladins move to me".to_string()), "{c:?}");
        let c = t.candidates("idle tanks stop", &v);
        assert!(c.contains(&"idle crusaders and idle paladins stop".to_string()), "{c:?}");
        // After a determiner it stays (the grammar has no one-of-several noun).
        assert!(t.candidates("the nearest tank stop", &v).iter().all(|c| c.contains("tank")));
        let c = t.candidates("dozers stop", &v);
        assert!(c.contains(&"builders stop".to_string()), "{c:?}");
    }

    #[test]
    fn a_synonym_never_shadows_one_of_the_matchs_names() {
        let t = Tables::parse(&[SHIPPED]);
        // A team called `the strongest` and a place called `here` stay themselves.
        let v = Vocabulary { teams: vec!["the strongest".into()], places: vec!["here".into()], ..vocab() };
        let c = t.candidates("the strongest, move to here", &v);
        assert!(c.iter().all(|c| c.starts_with("the strongest") && c.ends_with("to here")), "{c:?}");
    }

    #[test]
    fn a_players_file_adds_and_overrides() {
        let t = Tables::parse(&[SHIPPED, "[Words]\nmy boys = rangers\nhold = guard\n[Aliases]\ntanks = paladins\n"]);
        let v = vocab();
        assert!(t.candidates("my boys hold", &v).contains(&"rangers guard".to_string()));
        assert!(!t.candidates("my boys hold", &v).contains(&"rangers stop".to_string()));
        assert!(t.candidates("tanks stop", &v).contains(&"paladins stop".to_string()));
    }
}
