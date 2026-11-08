//! What the voice examples share: each row's vocabulary from `vocab.tsv` (as `voice_corpus`
//! writes it), the same-meaning comparison, and speech from macOS's `say`.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use zhbo_core::command::{parse, Vocabulary};

/// Every vocabulary in `<data dir>/vocab.tsv`, by id.
pub fn vocabularies(data: &Path) -> std::io::Result<BTreeMap<String, Vocabulary>> {
    Ok(std::fs::read_to_string(data.join("vocab.tsv"))?.lines().filter_map(vocab).collect())
}

/// `<data dir>/test.tsv`'s rows: input, label, vocabulary id.
pub fn test_rows(data: &Path, limit: usize) -> std::io::Result<Vec<(String, String, String)>> {
    Ok(std::fs::read_to_string(data.join("test.tsv"))?
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 3).then(|| (f[0].to_string(), f[1].to_string(), f[2].to_string()))
        })
        .take(limit)
        .collect())
}

/// Whether two texts are the same command: both parse, and the selection's groups match as sets
/// (`a and b` orders its groups freely; nothing else about the command does).
pub fn same_meaning(a: &str, b: &str, v: &Vocabulary) -> bool {
    let (Ok(mut x), Ok(mut y)) = (parse(a, v), parse(b, v)) else { return false };
    for cmd in [&mut x, &mut y] {
        if let Some(sel) = &mut cmd.selection {
            sel.include.sort_by_key(|g| format!("{g:?}"));
            sel.exclude.sort_by_key(|g| format!("{g:?}"));
        }
    }
    x == y
}

fn vocab(line: &str) -> Option<(String, Vocabulary)> {
    let (id, rest) = line.split_once('\t')?;
    Some((id.to_string(), Vocabulary::from_line(rest)?))
}


/// `text` spoken by macOS's `say` in `voice` (at `rate` words a minute, or the voice's own) as
/// 16 kHz mono samples.
pub fn speak(text: &str, voice: &str, rate: Option<u32>, scratch: &Path) -> Result<Vec<f32>, zhbo_voice::Error> {
    let wav = scratch.join(format!("zhbo_voice_{}.wav", std::process::id()));
    let mut say = Command::new("say");
    say.args(["-v", voice, "--data-format=LEF32@16000", "-o"]).arg(&wav);
    if let Some(r) = rate {
        say.arg("-r").arg(r.to_string());
    }
    let ok = say.arg(text).status()?.success();
    if !ok {
        return Err(format!("say failed for {voice:?}").into());
    }
    zhbo_voice::audio::read_wav(&std::fs::read(&wav)?)
}

