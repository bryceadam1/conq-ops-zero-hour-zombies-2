//! The voice model's training set (docs/COMMANDS.md section 11), in two steps:
//!
//! ```text
//! cargo run -p zhbo-core --example voice_corpus -- sample <dir> <count> <batches> <seed>
//! cargo run -p zhbo-core --example voice_corpus -- assemble <dir> <seed>
//! ```
//!
//! `sample` draws `count` canonical commands, each over its own invented vocabulary
//! (`command::sample`), and writes `<dir>/canonical.tsv` (`id`, canonical text, the names a
//! paraphrase must keep, `|`-separated) split into `<dir>/batches/batch_NN.tsv` for the paraphrase
//! pass, and `<dir>/vocab.tsv` (each id's vocabulary, for the constrained decoder's evaluation). The paraphrases come back as `<dir>/paraphrases/batch_NN.tsv` (`id`, paraphrase).
//!
//! `assemble` keeps the paraphrases that pass the meaning checks (every name kept as it is; the
//! counts, group numbers and compass words kept; an exclusion and a sequence still said), adds a
//! speech-recognition-noisy copy of some (homophones, dropped and filler words; never the names),
//! and writes `train.tsv`, `dev.tsv` and `test.tsv` (`input`, canonical label, id) split by command, the
//! canonical lines themselves among the training inputs, and `stats.txt`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use zhbo_core::command::normal::{self, Tables};
use zhbo_core::command::sample::{invented_vocabulary, names_in, Sampler};
use zhbo_core::command::{parse, print};
use zhbo_core::weapons::rng::Rng;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("sample") if args.len() == 5 => sample(Path::new(&args[1]), args[2].parse().unwrap(), args[3].parse().unwrap(), args[4].parse().unwrap()),
        Some("assemble") if args.len() == 3 => assemble(Path::new(&args[1]), args[2].parse().unwrap()),
        _ => Err("usage: voice_corpus sample <dir> <count> <batches> <seed> | assemble <dir> <seed>".into()),
    };
    if let Err(e) = r {
        eprintln!("voice_corpus: {e}");
        std::process::exit(1);
    }
}

fn sample(dir: &Path, count: usize, batches: usize, seed: u32) -> Result<(), String> {
    fs::create_dir_all(dir.join("batches")).map_err(|e| e.to_string())?;
    let mut rng = Rng::new(seed);
    let mut lines: Vec<String> = Vec::new();
    let mut vocabs: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while lines.len() < count {
        let v = invented_vocabulary(&mut rng);
        let mut s = Sampler { rng: Rng::new(rng.next_u32()), vocab: &v };
        let cmd = s.command();
        let text = print(&cmd, &v);
        // Canonical by construction; held to it anyway.
        if parse(&text, &v).ok().as_ref() != Some(&cmd) {
            return Err(format!("'{text}' does not round-trip"));
        }
        if !seen.insert(text.clone()) {
            continue;
        }
        let names = names_in(&text, &v).join("|");
        vocabs.push(format!("{:05}\t{}", lines.len(), vocab_line(&v)));
        lines.push(format!("{:05}\t{text}\t{names}", lines.len()));
    }
    fs::write(dir.join("canonical.tsv"), lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    fs::write(dir.join("vocab.tsv"), vocabs.join("\n") + "\n").map_err(|e| e.to_string())?;
    let per = count.div_ceil(batches.max(1));
    for (b, chunk) in lines.chunks(per).enumerate() {
        fs::write(dir.join("batches").join(format!("batch_{b:02}.tsv")), chunk.join("\n") + "\n").map_err(|e| e.to_string())?;
    }
    eprintln!("voice_corpus: {} canonical commands in {} batches", lines.len(), lines.len().div_ceil(per));
    Ok(())
}

/// A vocabulary as one `vocab.tsv` line after its id: the types (`singular>plural`), then the
/// places, teams, upgrades, powers, sciences, weapon modes and weapons, each `|`-separated, tab
/// between lists (the class words are the grammar's own).
fn vocab_line(v: &zhbo_core::command::Vocabulary) -> String {
    let types: Vec<String> = v.types.iter().map(|n| format!("{}>{}", n.singular, n.plural)).collect();
    [types.join("|"), v.places.join("|"), v.teams.join("|"), v.upgrades.join("|"), v.powers.join("|"), v.sciences.join("|"), v.modes.join("|"), v.weapons.join("|")].join("\t")
}

/// The number words a count may be said as.
const NUMBER_WORDS: [(&str, &str); 12] =
    [("0", "zero"), ("1", "one"), ("2", "two"), ("3", "three"), ("4", "four"), ("5", "five"), ("6", "six"), ("7", "seven"), ("8", "eight"), ("9", "nine"), ("10", "ten"), ("12", "twelve")];
const COMPASS: [&str; 4] = ["north", "south", "east", "west"];
const EXCLUSION: [&str; 9] = ["except", "but", "not", "other than", "excluding", "besides", "apart", "without", "leave out"];
const SEQUENCE: [&str; 6] = ["then", "after", "afterwards", "next", "once", "and"];

/// Why a paraphrase does not say what its label says, if it does not.
fn drift(label: &str, names: &[&str], para: &str) -> Option<String> {
    let p = format!(" {} ", normal::words(para).join(" "));
    for n in names {
        if !p.contains(&format!(" {n} ")) {
            return Some(format!("lost the name '{n}'"));
        }
    }
    let label_words: Vec<&str> = label.split(' ').collect();
    for w in &label_words {
        if w.bytes().all(|b| b.is_ascii_digit()) && names.iter().all(|n| !n.split(' ').any(|x| x == *w)) {
            let word = NUMBER_WORDS.iter().find(|(d, _)| d == w).map(|(_, n)| *n);
            if !p.contains(&format!(" {w} ")) && word.is_none_or(|n| !p.contains(&format!(" {n} "))) {
                return Some(format!("lost the number {w}"));
            }
        }
    }
    let in_names = |c: &str| names.iter().any(|n| n.contains(c));
    for c in COMPASS {
        if label.contains(c) && !in_names(c) && !p.contains(c) {
            return Some(format!("lost '{c}'"));
        }
    }
    if label_words.contains(&"except") && !EXCLUSION.iter().any(|e| p.contains(&format!(" {e} "))) {
        return Some("lost the exclusion".into());
    }
    if label_words.contains(&"then") && !SEQUENCE.iter().any(|e| p.contains(&format!(" {e} "))) {
        return Some("lost the sequence".into());
    }
    None
}

/// Words that carry no command: a phrasing that is its label plus only these is trivial.
const CHAFF: [&str; 14] = ["uh", "um", "okay", "ok", "alright", "right", "hey", "please", "now", "quick", "quickly", "right now", "everyone", "so"];

/// Whether `para` is just `label` with filler around it (at most one such phrasing per command is
/// kept: it teaches nothing the canonical line does not).
fn trivial(label: &str, para: &str) -> bool {
    let words: Vec<String> = normal::words(para).into_iter().filter(|w| !CHAFF.contains(&w.as_str())).collect();
    words.join(" ") == label
}

/// Words a speech recogniser confuses, as (heard, said) pairs read either way.
const HOMOPHONES: [(&str, &str); 14] = [
    ("for", "four"),
    ("to", "two"),
    ("too", "to"),
    ("there", "their"),
    ("here", "hear"),
    ("right", "write"),
    ("one", "won"),
    ("eight", "ate"),
    ("by", "buy"),
    ("ranger", "danger"),
    ("guard", "god"),
    ("attack", "a tack"),
    ("north", "nor"),
    ("move", "moo"),
];
const FILLERS: [&str; 5] = ["uh", "um", "okay", "alright", "like"];

/// A recognised version of `para`: lowercase, no punctuation, a homophone or two, a filler, a small
/// word dropped; the names left as they are.
fn noisy(rng: &mut Rng, para: &str, names: &[&str]) -> String {
    let mut words = normal::words(para);
    let protected = |i: usize, words: &[String]| names.iter().any(|n| {
        let n: Vec<&str> = n.split(' ').collect();
        (0..n.len()).any(|k| i >= k && words.len() >= i - k + n.len() && words[i - k..i - k + n.len()].iter().map(String::as_str).eq(n.iter().copied()))
    });
    for i in 0..words.len() {
        if protected(i, &words) || rng.next_u32() % 100 >= 15 {
            continue;
        }
        if let Some(&(a, b)) = HOMOPHONES.iter().find(|(a, b)| words[i] == *a || words[i] == *b) {
            words[i] = if words[i] == a { b.to_string() } else { a.to_string() };
        }
    }
    if rng.next_u32() % 100 < 30 {
        let at = rng.next_u32() as usize % (words.len() + 1);
        words.insert(at, FILLERS[rng.next_u32() as usize % FILLERS.len()].to_string());
    }
    if rng.next_u32() % 100 < 25 {
        if let Some(i) = (0..words.len()).find(|&i| (words[i] == "the" || words[i] == "a") && !protected(i, &words)) {
            words.remove(i);
        }
    }
    words.join(" ")
}

fn assemble(dir: &Path, seed: u32) -> Result<(), String> {
    let canonical = fs::read_to_string(dir.join("canonical.tsv")).map_err(|e| format!("canonical.tsv: {e}"))?;
    let mut labels: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for line in canonical.lines() {
        let mut f = line.split('\t');
        let (Some(id), Some(text)) = (f.next(), f.next()) else { continue };
        let names = f.next().unwrap_or("").split('|').filter(|n| !n.is_empty()).map(str::to_string).collect();
        labels.insert(id.to_string(), (text.to_string(), names));
    }
    let mut pairs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
    let mut read = 0usize;
    let mut unknown = 0usize;
    let mut entries: Vec<_> = fs::read_dir(dir.join("paraphrases")).map_err(|e| format!("paraphrases: {e}"))?.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        for line in text.lines() {
            let Some((id, para)) = line.split_once('\t') else { continue };
            let para = para.trim();
            if para.is_empty() {
                continue;
            }
            read += 1;
            let Some((label, names)) = labels.get(id.trim()) else {
                unknown += 1;
                continue;
            };
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let kept = pairs.entry(id.trim().to_string()).or_default();
            if trivial(label, para) && kept.iter().any(|k| trivial(label, k)) {
                *dropped.entry("only filler around the label (one kept per command)".into()).or_default() += 1;
                continue;
            }
            match drift(label, &names, para) {
                Some(why) => *dropped.entry(why.split('\'').next().unwrap_or(&why).trim().to_string()).or_default() += 1,
                None => kept.push(para.to_string()),
            }
        }
    }
    // How many the normaliser alone already reads back to the label (the model's job is the rest).
    let tables = Tables::parse(&[normal::SHIPPED]);
    let mut rng = Rng::new(seed);
    let (mut train, mut dev, mut test) = (Vec::new(), Vec::new(), Vec::new());
    let mut kept = 0usize;
    let mut noisy_count = 0usize;
    let mut normalised = 0usize;
    for (id, (label, names)) in &labels {
        // Split by command: 90 / 5 / 5 on the id's hash.
        let h = id.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619)) % 100;
        let out = match h {
            0..=89 => &mut train,
            90..=94 => &mut dev,
            _ => &mut test,
        };
        if h < 90 {
            out.push(format!("{label}\t{label}\t{id}"));
        }
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        for para in pairs.get(id).into_iter().flatten() {
            kept += 1;
            let words = normal::words(para).join(" ");
            if tables.candidates(&words, &zhbo_core::command::Vocabulary::default()).iter().any(|c| c == label) {
                normalised += 1;
            }
            out.push(format!("{para}\t{label}\t{id}"));
            if rng.next_u32() % 100 < 40 {
                noisy_count += 1;
                out.push(format!("{}\t{label}\t{id}", noisy(&mut rng, para, &names)));
            }
        }
    }
    for (name, rows) in [("train.tsv", &train), ("dev.tsv", &dev), ("test.tsv", &test)] {
        fs::write(dir.join(name), rows.join("\n") + "\n").map_err(|e| e.to_string())?;
    }
    let mut stats = format!(
        "canonical commands: {}\nparaphrases read: {read} (unknown ids: {unknown})\nkept: {kept}\nnoisy copies: {noisy_count}\nread back by the normaliser alone: {normalised}\nrows: train {}, dev {}, test {}\ndropped:\n",
        labels.len(),
        train.len(),
        dev.len(),
        test.len()
    );
    for (why, n) in &dropped {
        stats.push_str(&format!("  {why}: {n}\n"));
    }
    fs::write(dir.join("stats.txt"), &stats).map_err(|e| e.to_string())?;
    eprint!("{stats}");
    Ok(())
}
