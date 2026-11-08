//! Respelling what Whisper heard into the match's words (docs/COMMANDS.md section 11, "Snapping").
//!
//! Whisper writes what a name sounds like: `velgal team` for `belcal team`, `enami` for `enemy`,
//! `play spican` for `place beacon`. The translator copies names and knows the grammar's words,
//! so a respelt one leaves it guessing (and the mask then picks some other name). The snapper
//! puts each word it does not know back into the nearest-sounding phrase it does: the match's
//! names, the grammar's words and verbs, the synonym table's phrases and aliases.
//!
//! A word is known if it is one of those phrases' words, a number, or an ordinary word (one the
//! translator was trained on outside names: `words.txt`, written by `voice_corpus assemble`
//! beside the training set and shipped with the model). Only spans with an unknown word are
//! respelt, so what the player said plainly stays as said.

use std::collections::BTreeSet;

use zhbo_core::command::normal::Tables;
use zhbo_core::command::vocab::fixed_words;
use zhbo_core::command::Vocabulary;

/// The longest span respelt, in words (the longest names have three or four).
const MAX_SPAN: usize = 4;
/// The largest distance, per letter of the span's unknown words, that still counts as the same
/// phrase. Dividing by the unknown letters (not the whole span's) means an unknown word must be
/// explained by the phrase, not dropped beside a long name it happens to sit next to.
pub const CUTOFF: f32 = 0.34;

pub struct Snapper {
    ordinary: BTreeSet<String>,
    /// The synonym table's phrases and aliases: words a player says that the grammar reads.
    table_phrases: Vec<String>,
}

impl Snapper {
    pub fn new(ordinary: impl IntoIterator<Item = String>, tables: &Tables) -> Snapper {
        let mut table_phrases: Vec<String> = tables.words.iter().map(|(k, _)| k.join(" ")).collect();
        table_phrases.extend(tables.aliases.iter().map(|(name, _)| name.clone()));
        Snapper { ordinary: ordinary.into_iter().collect(), table_phrases }
    }

    /// The phrases a heard span may be respelt into, for one match.
    fn targets(&self, vocab: &Vocabulary) -> Vec<String> {
        let mut out: Vec<String> = fixed_words().iter().map(|w| w.to_string()).collect();
        out.extend(vocab.verbs.iter().map(|v| v.words.clone()));
        out.extend(self.table_phrases.iter().cloned());
        for n in vocab.types.iter().chain(&vocab.classes) {
            out.push(n.singular.clone());
            out.push(n.plural.clone());
        }
        for list in [&vocab.places, &vocab.teams, &vocab.upgrades, &vocab.powers, &vocab.sciences, &vocab.modes, &vocab.weapons] {
            for name in list {
                out.push(name.clone());
                // `the torwren crossing` is often said, and heard, without its article.
                if let Some(bare) = name.strip_prefix("the ") {
                    out.push(bare.to_string());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// `heard` (tidied: lower case, words and commas) with each unknown word's span respelt.
    pub fn snap(&self, heard: &str, vocab: &Vocabulary) -> String {
        let targets = self.targets(vocab);
        let mut known: BTreeSet<&str> = self.ordinary.iter().map(String::as_str).collect();
        known.extend(targets.iter().flat_map(|t| t.split_whitespace()));
        let unknown = |w: &str| !w.is_empty() && !known.contains(w) && !w.bytes().all(|b| b.is_ascii_digit());
        // Each word with what follows it (a comma) kept apart. Whisper joins words with hyphens
        // (`a-8-korselvases`); an unknown hyphenated word is read as its parts (`attack-move`,
        // a known word, stays whole).
        let mut tokens: Vec<(&str, &str)> = Vec::new();
        for t in heard.split_whitespace() {
            let core = t.trim_end_matches(',');
            let punct = &t[core.len()..];
            if unknown(core) && core.contains('-') {
                let parts: Vec<&str> = core.split('-').filter(|p| !p.is_empty()).collect();
                for (i, p) in parts.iter().enumerate() {
                    tokens.push((p, if i + 1 == parts.len() { punct } else { "" }));
                }
            } else {
                tokens.push((core, punct));
            }
        }
        // Every span around an unknown word, at its best target.
        let mut found: Vec<(f32, usize, usize, &str)> = Vec::new();
        for u in (0..tokens.len()).filter(|&u| unknown(tokens[u].0)) {
            let mut best: Option<(f32, usize, usize, &str)> = None;
            for start in u.saturating_sub(MAX_SPAN - 1)..=u {
                for end in u + 1..=(start + MAX_SPAN).min(tokens.len()) {
                    // A comma inside the span splits it: what the player paused at is two things.
                    if tokens[start..end - 1].iter().any(|t| !t.1.is_empty()) {
                        continue;
                    }
                    let span: String = tokens[start..end].iter().map(|t| t.0).collect();
                    let unknown_letters: usize = tokens[start..end].iter().filter(|t| unknown(t.0)).map(|t| t.0.len()).sum();
                    for t in &targets {
                        let target: String = t.split_whitespace().collect();
                        let d = distance(&span, &target) / unknown_letters.max(1) as f32;
                        let better = match best {
                            None => true,
                            // Ties go to the longer span: more of what was heard explained.
                            Some((bd, bs, be, _)) => d < bd || (d == bd && end - start > be - bs),
                        };
                        if better {
                            best = Some((d, start, end, t));
                        }
                    }
                }
            }
            if let Some(b) = best.filter(|b| b.0 <= CUTOFF) {
                found.push(b);
            }
        }
        // The closest first, each word respelt at most once.
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut taken = vec![false; tokens.len()];
        let mut at: Vec<Option<(usize, &str)>> = vec![None; tokens.len()];
        for (_, start, end, t) in found {
            if taken[start..end].iter().any(|&x| x) {
                continue;
            }
            taken[start..end].iter_mut().for_each(|x| *x = true);
            at[start] = Some((end, t));
        }
        let mut out: Vec<String> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            match at[i] {
                Some((end, t)) => {
                    out.push(format!("{t}{}", tokens[end - 1].1));
                    i = end;
                }
                None => {
                    out.push(format!("{}{}", tokens[i].0, tokens[i].1));
                    i += 1;
                }
            }
        }
        out.join(" ")
    }
}

/// How a letter sounds, coarsely: letters in one class are cheap to confuse.
fn class(c: u8) -> u8 {
    match c {
        b'a' | b'e' | b'i' | b'o' | b'u' | b'y' => 1,
        b'b' | b'p' | b'v' | b'f' => 2,
        b'c' | b'k' | b'q' | b'g' | b'j' | b's' | b'z' | b'x' => 3,
        b'd' | b't' => 4,
        b'm' | b'n' => 5,
        b'l' | b'r' => 6,
        b'h' | b'w' => 7,
        _ => 0,
    }
}

/// An edit distance weighted by sound: a letter for one that sounds alike, or a vowel, `h` or `w`
/// put in or left out, costs half (`velgal` is 1.0 from `belcal`, `renbra` 0.5 from `wrenbra`).
/// Spaces, hyphens and apostrophes are not letters here.
pub fn distance(a: &str, b: &str) -> f32 {
    let a: Vec<u8> = a.bytes().filter(u8::is_ascii_alphanumeric).collect();
    let b: Vec<u8> = b.bytes().filter(u8::is_ascii_alphanumeric).collect();
    let gap = |c: u8| if matches!(class(c), 1 | 7) { 0.5 } else { 1.0 };
    let swap = |x: u8, y: u8| {
        if x == y {
            0.0
        } else if class(x) != 0 && class(x) == class(y) {
            0.5
        } else {
            1.0
        }
    };
    let mut row: Vec<f32> = std::iter::once(0.0).chain(b.iter().scan(0.0, |s, &c| {
        *s += gap(c);
        Some(*s)
    })).collect();
    for &x in &a {
        let mut prev = row[0];
        row[0] += gap(x);
        for (j, &y) in b.iter().enumerate() {
            let here = (prev + swap(x, y)).min(row[j + 1] + gap(x)).min(row[j] + gap(y));
            prev = row[j + 1];
            row[j + 1] = here;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use zhbo_core::command::normal;
    use zhbo_core::command::Noun;

    use super::*;

    fn vocab() -> Vocabulary {
        Vocabulary {
            types: vec![Noun::new("korgor tower", "korgor towers"), Noun::new("fengor", "fengors"), Noun::new("tortan factory", "tortan factories")],
            places: vec!["the wrenbra crossing".into()],
            teams: vec!["belcal team".into()],
            ..Vocabulary::default()
        }
    }

    fn snapper() -> Snapper {
        let ordinary = ["go", "to", "over", "head", "get", "please", "guys", "those", "build", "a", "after", "play", "finger"].map(String::from);
        Snapper::new(ordinary, &Tables::parse(&[normal::SHIPPED]))
    }

    #[test]
    fn sounds_alike_cost_less() {
        assert_eq!(distance("velgal", "belcal"), 1.0);
        assert_eq!(distance("renbra", "wrenbra"), 0.5);
        assert_eq!(distance("cornixcal", "kornixcal"), 0.5);
        assert!(distance("tank", "fengor") > 3.0);
    }

    #[test]
    fn what_was_heard_is_respelt_into_the_matchs_words() {
        let (s, v) = (snapper(), vocab());
        // Names: one word, several, and a name heard as two words.
        assert_eq!(s.snap("them and velgal team attack-move to renbra crossing", &v), "them and belcal team attack-move to wrenbra crossing");
        assert_eq!(s.snap("go to the core gore tower", &v), "go to the korgor tower");
        // The grammar's words, and a phrase the synonym table reads.
        assert_eq!(s.snap("half the enami tautan factories", &v), "half the enemy tortan factories");
        // What was said plainly stays, commas included; an ordinary word is never respelt.
        assert_eq!(s.snap("those guys, head over to me please", &v), "those guys, head over to me please");
        assert_eq!(s.snap("build a finger", &v), "build a finger");
        // Nothing near enough: left for the translator.
        assert_eq!(s.snap("go to xylophone", &v), "go to xylophone");
        // An unknown word is explained, never dropped beside a name; hyphens Whisper put in split.
        assert_eq!(s.snap("tortan factories zup", &v), "tortan factories zup");
        assert_eq!(s.snap("them and belcal team-build fengar", &v), "them and belcal team build fengor");
    }
}
