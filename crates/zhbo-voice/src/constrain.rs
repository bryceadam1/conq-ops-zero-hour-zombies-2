//! The grammar's mask over the model's tokens (docs/COMMANDS.md section 11, "Constrained
//! decoding"): only tokens that keep the translation a valid prefix of a command over the match's
//! vocabulary may be chosen.
//!
//! The grammar works in words and phrases (`zhbo_core::command::parse::completions_with`, the
//! same completions the menu offers, so the mask enforces meaning as well as form: only the
//! match's names, only buildable things after `build`). At each word boundary the allowed phrases
//! are tokenized into a trie (a `Level`). A name the player gives (`name them …`) has no list: there
//! any word may be spelled, token by token, and the grammar decides where it may end.
//!
//! One token can mean two things: `▁` is inside `the torwren crossing` (`▁the ▁ t or…`) and also
//! the start of `korgor tower` after the whole word `the`. So the mask follows every reading at
//! once (a `Path` each), and a step may take any token some reading allows. The end of the
//! translation is allowed only where some reading is a whole command. The tokenizer splits on
//! spaces, so a sentence's tokens are its phrases' tokens in order: T5's sentencepiece marks a
//! word's first token with `▁`; Whisper's byte-level BPE with `Ġ`, the space before the word, so
//! there each phrase is tokenized after a space (the command model learned `" " + label`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use tokenizers::Tokenizer;
use zhbo_core::command::parse::completions_with;
use zhbo_core::command::vocab::is_word;
use zhbo_core::command::Vocabulary;

use crate::Error;

/// The counts a player may say (the menu offers fewer).
pub fn voice_numbers() -> Vec<String> {
    (2..=50).map(|n| n.to_string()).collect()
}

/// The marker sentencepiece puts before a word's first token.
const SPACE: char = '▁';
/// The marker byte-level BPE (GPT-2's, Whisper's) gives a space, so a word's first token after one.
const BYTE_SPACE: char = 'Ġ';

/// How a tokenizer marks a word's start.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    /// Sentencepiece: `▁` comes from the pre-tokenizer, so a phrase is tokenized as it is.
    Sentencepiece,
    /// Byte-level BPE: the space is the text's own, so a phrase is tokenized after one.
    ByteLevel,
}

impl Style {
    fn of(tok: &Tokenizer) -> Style {
        if tok.token_to_id("Ġthe").is_some() { Style::ByteLevel } else { Style::Sentencepiece }
    }

    fn mark(self) -> char {
        match self {
            Style::Sentencepiece => SPACE,
            Style::ByteLevel => BYTE_SPACE,
        }
    }

    /// `phrase` as the tokenizer sees it at a word boundary.
    fn boundary(self, phrase: &str) -> String {
        match self {
            Style::Sentencepiece => phrase.to_string(),
            Style::ByteLevel => format!(" {phrase}"),
        }
    }
}

#[derive(Default)]
struct Node {
    children: BTreeMap<u32, usize>,
    /// The phrase that ends here, if one does.
    phrase: Option<String>,
}

/// What may follow one prefix: its phrases as a token trie, whether the prefix is whole, and
/// whether a name's word may come next.
struct Level {
    nodes: Vec<Node>,
    complete: bool,
    name: bool,
}

impl Level {
    fn build(prefix: &str, vocab: &Vocabulary, tok: &Tokenizer, style: Style, numbers: &[&str]) -> Result<Level, Error> {
        let c = completions_with(prefix, vocab, numbers);
        let mut nodes = vec![Node::default()];
        for phrase in &c.next {
            let ids = tok.encode(style.boundary(phrase), false)?.get_ids().to_vec();
            let mut at = 0;
            for id in ids {
                at = match nodes[at].children.get(&id) {
                    Some(&n) => n,
                    None => {
                        nodes.push(Node::default());
                        let n = nodes.len() - 1;
                        nodes[at].children.insert(id, n);
                        n
                    }
                };
            }
            if at != 0 {
                nodes[at].phrase = Some(phrase.clone());
            }
        }
        Ok(Level { nodes, complete: c.complete, name: c.name })
    }
}

/// Where one reading stands within its level.
#[derive(Clone, PartialEq, Eq, Hash)]
enum At {
    /// On the trie, at this node (0: the word boundary itself).
    Node(usize),
    /// Spelling a name's word, so far.
    Word(String),
}

/// One reading of the tokens so far: the whole phrases before it, and where it is in the next.
#[derive(Clone)]
struct Path {
    words: Vec<String>,
    level: Rc<Level>,
    at: At,
}

/// The tokens a name's word may be spelled with: those that start a word, and those that go on.
struct Spelling {
    starts: BTreeMap<u32, String>,
    goes_on: BTreeMap<u32, String>,
}

impl Spelling {
    fn new(tok: &Tokenizer, style: Style) -> Spelling {
        let mut starts = BTreeMap::new();
        let mut goes_on = BTreeMap::new();
        for (piece, id) in tok.get_vocab(false) {
            match piece.strip_prefix(style.mark()) {
                // The marker alone starts a word whose letters follow in other tokens.
                Some(rest) if rest.is_empty() || is_word(rest) => {
                    starts.insert(id, rest.to_string());
                }
                Some(_) => {}
                None if is_word(&piece) => {
                    goes_on.insert(id, piece);
                }
                None => {}
            }
        }
        Spelling { starts, goes_on }
    }
}

/// The mask's state through one translation. A clone shares the clone's tables (the spelling,
/// the levels built so far), so a beam can carry one per hypothesis.
#[derive(Clone)]
pub struct Mask<'a> {
    vocab: &'a Vocabulary,
    tok: &'a Tokenizer,
    style: Style,
    numbers: Vec<String>,
    spelling: Rc<Spelling>,
    /// Each prefix's level, built once.
    levels: Rc<std::cell::RefCell<HashMap<String, Rc<Level>>>>,
    paths: Vec<Path>,
}

impl<'a> Mask<'a> {
    pub fn new(vocab: &'a Vocabulary, tok: &'a Tokenizer) -> Result<Mask<'a>, Error> {
        let style = Style::of(tok);
        let mut mask = Mask { vocab, tok, style, numbers: voice_numbers(), spelling: Rc::new(Spelling::new(tok, style)), levels: Rc::default(), paths: Vec::new() };
        let level = mask.level(&[])?;
        mask.paths.push(Path { words: Vec::new(), level, at: At::Node(0) });
        Ok(mask)
    }

    fn level(&mut self, words: &[String]) -> Result<Rc<Level>, Error> {
        let prefix = words.join(" ");
        if let Some(l) = self.levels.borrow().get(&prefix) {
            return Ok(l.clone());
        }
        let refs: Vec<&str> = self.numbers.iter().map(String::as_str).collect();
        let level = Rc::new(Level::build(&prefix, self.vocab, self.tok, self.style, &refs)?);
        self.levels.borrow_mut().insert(prefix, level.clone());
        Ok(level)
    }

    /// The phrase a path has just finished, if it stands at the end of one.
    fn finished(path: &Path) -> Option<&str> {
        match &path.at {
            At::Node(n) => path.level.nodes[*n].phrase.as_deref(),
            At::Word(w) if !w.is_empty() => Some(w),
            At::Word(_) => None,
        }
    }

    /// The word boundary after a path's finished phrase.
    fn after(&mut self, path: &Path) -> Result<Option<Path>, Error> {
        let Some(phrase) = Self::finished(path) else { return Ok(None) };
        let mut words = path.words.clone();
        words.push(phrase.to_string());
        let level = self.level(&words)?;
        Ok(Some(Path { words, level, at: At::Node(0) }))
    }

    /// The tokens allowed next (`eos` where the command may end).
    pub fn allowed(&mut self, eos: u32) -> Result<Vec<u32>, Error> {
        let mut out = BTreeSet::new();
        let mut boundaries: Vec<Rc<Level>> = Vec::new();
        for path in self.paths.clone() {
            match &path.at {
                At::Node(n) => out.extend(path.level.nodes[*n].children.keys().copied()),
                At::Word(_) => out.extend(self.spelling.goes_on.keys().copied()),
            }
            if path.at == At::Node(0) {
                boundaries.push(path.level.clone());
            }
            if let Some(boundary) = self.after(&path)? {
                boundaries.push(boundary.level);
            }
        }
        for level in boundaries {
            out.extend(level.nodes[0].children.keys().copied());
            if level.name {
                out.extend(self.spelling.starts.keys().copied());
            }
            if level.complete {
                out.insert(eos);
            }
        }
        Ok(out.into_iter().collect())
    }

    /// Where `token` takes a reading that stands at a word boundary.
    fn enter(&self, boundary: &Path, token: u32, out: &mut Vec<Path>) {
        if let Some(&n) = boundary.level.nodes[0].children.get(&token) {
            out.push(Path { at: At::Node(n), ..boundary.clone() });
        }
        if boundary.level.name {
            if let Some(start) = self.spelling.starts.get(&token) {
                out.push(Path { at: At::Word(start.clone()), ..boundary.clone() });
            }
        }
    }

    /// Take `token`: along each reading's phrase, or past its end into what follows it.
    pub fn advance(&mut self, token: u32) -> Result<(), Error> {
        let mut next = Vec::new();
        for path in std::mem::take(&mut self.paths) {
            match &path.at {
                At::Node(n) => {
                    if let Some(&c) = path.level.nodes[*n].children.get(&token) {
                        next.push(Path { at: At::Node(c), ..path.clone() });
                    }
                }
                At::Word(w) => {
                    if let Some(more) = self.spelling.goes_on.get(&token) {
                        next.push(Path { at: At::Word(format!("{w}{more}")), ..path.clone() });
                    }
                }
            }
            if path.at == At::Node(0) {
                self.enter(&path, token, &mut next);
            }
            if let Some(boundary) = self.after(&path)? {
                self.enter(&boundary, token, &mut next);
            }
        }
        // Two readings that agree on everything are one.
        let mut seen = BTreeSet::new();
        next.retain(|p| seen.insert((p.words.clone(), at_key(&p.at))));
        if next.is_empty() {
            return Err(format!("token {token} is outside the grammar's mask").into());
        }
        self.paths = next;
        Ok(())
    }
}

fn at_key(at: &At) -> String {
    match at {
        At::Node(n) => format!("n{n}"),
        At::Word(w) => format!("w{w}"),
    }
}

#[cfg(test)]
mod tests {
    use tokenizers::models::unigram::Unigram;
    use tokenizers::pre_tokenizers::metaspace::{Metaspace, PrependScheme};
    use zhbo_core::command::Noun;

    use super::*;

    /// A sentencepiece-like tokenizer small enough to reason about: `korgor` and `zarwren` both
    /// start with a bare `▁`, as unfamiliar words do in T5's.
    fn tokenizer() -> Tokenizer {
        let mut pieces: Vec<(String, f64)> = vec![("<unk>".into(), 0.0), ("</s>".into(), 0.0)];
        for p in ["▁move", "▁to", "▁the", "▁then", "▁name", "▁them", "▁tower", "▁towers", "▁crossing", "▁wing", "▁", "or", "gor", "wren"] {
            pieces.push((p.to_string(), -2.0));
        }
        pieces.extend(('a'..='z').map(|c| (c.to_string(), -10.0)));
        let mut tok = Tokenizer::new(Unigram::from(pieces, Some(0), false).unwrap());
        tok.with_pre_tokenizer(Some(Metaspace::new(SPACE, PrependScheme::Always, true)));
        tok
    }

    /// A byte-level BPE like Whisper's: each listed piece is built up a letter at a time by its
    /// own merges, so unfamiliar words fall apart into `Ġ`, letters and the shorter pieces.
    fn byte_tokenizer() -> Tokenizer {
        let mut vocab: HashMap<String, u32> = HashMap::new();
        let mut merges: Vec<(String, String)> = Vec::new();
        let add = |piece: &str, vocab: &mut HashMap<String, u32>| {
            let n = vocab.len() as u32;
            vocab.entry(piece.to_string()).or_insert(n);
        };
        add("<|endoftext|>", &mut vocab);
        for c in ('a'..='z').chain(['Ġ']) {
            add(&c.to_string(), &mut vocab);
        }
        for p in ["Ġmove", "Ġto", "Ġthe", "Ġthen", "Ġname", "Ġthem", "Ġtower", "Ġtowers", "Ġcrossing", "Ġwing", "or", "gor", "wren"] {
            let chars: Vec<char> = p.chars().collect();
            let mut built = chars[0].to_string();
            for c in &chars[1..] {
                let next = format!("{built}{c}");
                if !vocab.contains_key(&next) {
                    merges.push((built.clone(), c.to_string()));
                    add(&next, &mut vocab);
                }
                built = next;
            }
        }
        // As Whisper's tokenizer.json has it.
        let spec = serde_json::json!({
            "version": "1.0", "truncation": null, "padding": null, "added_tokens": [], "normalizer": null,
            "pre_tokenizer": {"type": "ByteLevel", "add_prefix_space": false, "trim_offsets": true, "use_regex": true},
            "post_processor": null, "decoder": {"type": "ByteLevel", "add_prefix_space": true, "trim_offsets": true, "use_regex": true},
            "model": {"type": "BPE", "vocab": vocab, "merges": merges},
        });
        spec.to_string().parse().unwrap()
    }

    fn vocab() -> Vocabulary {
        Vocabulary { types: vec![Noun::new("korgor tower", "korgor towers")], places: vec!["the zarwren crossing".into()], ..Vocabulary::default() }
    }

    /// Walk `text`'s tokens through the mask, then its end.
    fn walk(text: &str) -> Result<(), String> {
        walk_with(&tokenizer(), "</s>", text)
    }

    /// The same under the byte-level tokenizer, the text after a space as the command model writes it.
    fn walk_bytes(text: &str) -> Result<(), String> {
        walk_with(&byte_tokenizer(), "<|endoftext|>", &format!(" {text}"))
    }

    fn walk_with(tok: &Tokenizer, end: &str, text: &str) -> Result<(), String> {
        let v = vocab();
        let eos = tok.token_to_id(end).unwrap();
        let mut mask = Mask::new(&v, tok).unwrap();
        for t in tok.encode(text, false).unwrap().get_ids().iter().copied() {
            if !mask.allowed(eos).unwrap().contains(&t) {
                return Err(format!("{:?} refused", tok.id_to_token(t)));
            }
            mask.advance(t).unwrap();
        }
        mask.allowed(eos).unwrap().contains(&eos).then_some(()).ok_or_else(|| "cannot end".to_string())
    }

    #[test]
    fn a_token_inside_one_phrase_may_start_the_next() {
        let tok = tokenizer();
        // The case: `▁` goes on into `the zarwren crossing` and also starts `korgor` after `the`.
        assert_eq!(tok.encode("the korgor tower", false).unwrap().get_tokens()[..3], ["▁the", "▁", "k"]);
        assert_eq!(tok.encode("the zarwren crossing", false).unwrap().get_tokens()[..3], ["▁the", "▁", "z"]);
        assert_eq!(walk("move to the korgor tower"), Ok(()));
        assert_eq!(walk("move to the zarwren crossing"), Ok(()));
        assert!(walk("move to the korwren tower").is_err());
        assert!(walk("move to the").is_err());
    }

    #[test]
    fn a_name_may_be_any_words() {
        assert_eq!(walk("korgor towers name them selhal wing"), Ok(()));
        assert_eq!(walk("korgor towers name them selhal wing then move to the korgor tower"), Ok(()));
        assert!(walk("korgor towers name them").is_err());
    }

    #[test]
    fn byte_level_tokens_walk_the_same_commands() {
        let tok = byte_tokenizer();
        assert_eq!(Style::of(&tok), Style::ByteLevel);
        assert_eq!(Style::of(&tokenizer()), Style::Sentencepiece);
        assert_eq!(tok.encode(" the korgor tower", false).unwrap().get_tokens()[..3], ["Ġthe", "Ġ", "k"]);
        assert_eq!(walk_bytes("move to the korgor tower"), Ok(()));
        assert_eq!(walk_bytes("move to the zarwren crossing"), Ok(()));
        assert!(walk_bytes("move to the korwren tower").is_err());
        assert!(walk_bytes("move to the").is_err());
        assert_eq!(walk_bytes("korgor towers name them selhal wing then move to the korgor tower"), Ok(()));
        assert!(walk_bytes("korgor towers name them").is_err());
    }
}
