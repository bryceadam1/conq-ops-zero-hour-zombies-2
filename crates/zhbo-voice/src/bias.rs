//! Contextual biasing for Whisper (docs/COMMANDS.md section 11, "Biasing"): the phrases a match
//! expects to hear (its names, the grammar's words) made likelier while Whisper writes, without
//! ruling anything else out.
//!
//! The phrases go into a tree over Whisper's tokens, each written as Whisper writes a word (a
//! leading space) in lower case, capitalised and title case. A token that starts a phrase or
//! carries one on gets its phrase's weight added to its log-probability. A phrase left half-way
//! gives back what it gained, so the boost can't be farmed by starting names (the retraction of
//! Google's contextual biasing, Zhao et al. 2019); a finished one keeps it.

use std::collections::HashMap;

use tokenizers::Tokenizer;

use crate::Error;

struct Node {
    next: HashMap<u32, usize>,
    /// The weight a token reaching this node earns: the heaviest phrase through it.
    weight: f32,
    /// A phrase ends here.
    end: bool,
}

pub struct Bias {
    nodes: Vec<Node>,
}

/// Where one hypothesis stands in the tree: its node (0, the root, outside any phrase) and what
/// the phrase it is in has gained so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    node: usize,
    gained: f32,
}

impl Bias {
    /// The tree of `phrases`, each with its weight (natural log units).
    pub fn new(tokenizer: &Tokenizer, phrases: &[(String, f32)]) -> Result<Bias, Error> {
        let mut bias = Bias { nodes: vec![Node { next: HashMap::new(), weight: 0.0, end: false }] };
        for (phrase, weight) in phrases {
            let lower = phrase.trim().to_lowercase();
            if lower.is_empty() || *weight <= 0.0 {
                continue;
            }
            let mut forms = vec![lower.clone(), capitalise(&lower), lower.split(' ').map(capitalise).collect::<Vec<_>>().join(" ")];
            forms.dedup();
            for form in forms {
                let ids = tokenizer.encode(format!(" {form}"), false)?.get_ids().to_vec();
                bias.insert(&ids, *weight);
            }
        }
        Ok(bias)
    }

    fn insert(&mut self, ids: &[u32], weight: f32) {
        let mut at = 0;
        for &t in ids {
            at = match self.nodes[at].next.get(&t) {
                Some(&n) => n,
                None => {
                    self.nodes.push(Node { next: HashMap::new(), weight: 0.0, end: false });
                    let n = self.nodes.len() - 1;
                    self.nodes[at].next.insert(t, n);
                    n
                }
            };
            self.nodes[at].weight = self.nodes[at].weight.max(weight);
        }
        self.nodes[at].end = true;
    }

    /// What every token leaving `state` would get, as the change for most tokens (giving back an
    /// unfinished phrase's gain) and the tokens that do better (those carrying the phrase on, or
    /// starting another), each with its own change.
    pub fn boosts(&self, state: State) -> (f32, Vec<(u32, f32)>) {
        let node = &self.nodes[state.node];
        let back = if node.end { 0.0 } else { -state.gained };
        let mut better: HashMap<u32, f32> = HashMap::new();
        for (&t, &n) in &self.nodes[0].next {
            better.insert(t, back + self.nodes[n].weight);
        }
        if state.node != 0 {
            for (&t, &n) in &node.next {
                better.insert(t, self.nodes[n].weight);
            }
        }
        (back, better.into_iter().collect())
    }

    /// The state after `token`.
    pub fn advance(&self, state: State, token: u32) -> State {
        if state.node != 0 {
            if let Some(&n) = self.nodes[state.node].next.get(&token) {
                return State { node: n, gained: state.gained + self.nodes[n].weight };
            }
        }
        match self.nodes[0].next.get(&token) {
            Some(&n) => State { node: n, gained: self.nodes[n].weight },
            None => State::default(),
        }
    }
}

fn capitalise(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A word-level tokenizer: each space-led word one token (`Ġword`), enough to walk the tree.
    fn tokenizer() -> Tokenizer {
        let words = ["Ġostzar", "ĠOstzar", "Ġfactories", "ĠFactories", "Ġoscar", "Ġthe", "Ġbridge", "ĠBridge", "Ġost", "ĠOst"];
        let vocab: serde_json::Map<String, serde_json::Value> = words.iter().enumerate().map(|(i, w)| (w.to_string(), serde_json::json!(i))).collect();
        let spec = serde_json::json!({
            "version": "1.0",
            "pre_tokenizer": {"type": "ByteLevel", "add_prefix_space": false, "trim_offsets": true, "use_regex": true},
            "model": {"type": "WordLevel", "vocab": vocab, "unk_token": "Ġthe"},
            "decoder": {"type": "ByteLevel", "add_prefix_space": true, "trim_offsets": true, "use_regex": true}
        });
        spec.to_string().parse().unwrap()
    }

    #[test]
    fn a_phrase_earns_its_weight_and_a_half_phrase_gives_it_back() {
        let tok = tokenizer();
        let id = |w: &str| tok.token_to_id(w).unwrap();
        let bias = Bias::new(&tok, &[("ostzar factories".into(), 2.0), ("the bridge".into(), 1.0)]).unwrap();
        // Outside a phrase, only phrase starts do better, by their weight; nothing is given back.
        let (rest, better) = bias.boosts(State::default());
        assert_eq!(rest, 0.0);
        let get = |b: &[(u32, f32)], t: u32| b.iter().find(|x| x.0 == t).map(|x| x.1);
        assert_eq!(get(&better, id("Ġostzar")), Some(2.0));
        assert_eq!(get(&better, id("ĠOstzar")), Some(2.0));
        assert_eq!(get(&better, id("Ġoscar")), None);
        // Half-way, carrying on earns more, and leaving gives back the 2.0 gained.
        let s = bias.advance(State::default(), id("Ġostzar"));
        let (rest, better) = bias.boosts(s);
        assert_eq!(rest, -2.0);
        assert_eq!(get(&better, id("Ġfactories")), Some(2.0));
        // Leaving for another phrase's start: its weight, less what this one gained.
        assert_eq!(get(&better, id("Ġthe")), Some(-1.0));
        // A finished phrase keeps what it earned.
        let s = bias.advance(s, id("Ġfactories"));
        assert_eq!(bias.boosts(s).0, 0.0);
        assert_eq!(bias.advance(s, id("Ġoscar")), State::default());
    }
}
