//! Spoken commands over a match's real kind of names (docs/COMMANDS.md section 11): English words
//! like Zero Hour's (`rangers`, `war factory`), which the training set's invented names are not.
//! An evaluation only; none of it is trained on.
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_match -- <voice dir>
//! ```
//!
//! macOS only: each line is spoken by `say` (four voices in turn) and run through the whole
//! pipeline (`<voice dir>` as the launcher fills it). Reports each answer, what the game would do
//! with it (`command::voice::act` at the shipped guard), and the count right.

mod common;

use std::path::Path;

use zhbo_core::command::voice::{act, Act, Heard};
use zhbo_core::command::Vocabulary;
use zhbo_voice::pipeline::Pipeline;

const VOICES: [&str; 4] = ["Samantha", "Daniel", "Karen", "Moira"];

/// The match: Zero Hour's kind of names (types `singular>plural`, then a place).
const VOCAB: &str = "ranger>rangers|humvee>humvees|crusader tank>crusader tanks|missile defender>missile defenders|barracks>barracks|war factory>war factories|construction dozer>construction dozers|supply center>supply centers\tthe bridge\t\t\t\t\t\t";

/// What is said, and the command it means.
const LINES: [(&str, &str); 16] = [
    ("Rangers, come over to me.", "rangers move to me"),
    ("All humvees, attack the nearest enemy humvee.", "humvees attack the nearest enemy humvee"),
    ("Crusader tanks, guard the war factory.", "crusader tanks guard the war factory"),
    ("Build two rangers.", "build 2 rangers"),
    ("Everyone, follow me!", "units guard me"),
    ("Dozers, build a barracks near me.", "builders build barracks near me"),
    ("Missile defenders, stop.", "missile defenders stop"),
    ("Send the humvees to the bridge.", "humvees move to the bridge"),
    ("Rangers and missile defenders, attack-move to the bridge.", "rangers and missile defenders attack-move to the bridge"),
    ("The damaged crusader tanks, go back to the war factory.", "the damaged crusader tanks move to the war factory"),
    ("Half the rangers, guard me.", "half the rangers guard me"),
    ("Group three, attack that.", "group 3 attack that"),
    ("Humvees, move north of the bridge.", "humvees move north of the bridge"),
    ("The nearest ranger, capture that.", "the nearest ranger capture that"),
    ("Sell the barracks.", "the barracks sell"),
    ("Crusader tanks, attack the enemy war factory.", "crusader tanks attack the enemy war factory"),
];

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().ok_or("usage: voice_match <voice dir>")?;
    let vocab = Vocabulary::from_line(VOCAB).ok_or("the vocabulary line")?;
    let tuning = zhbo_core::tuning::Tuning::default();
    let mut pipeline = Pipeline::load(Path::new(dir), &Pipeline::shipped_tables(), candle_core::Device::Cpu)?;
    let scratch = std::env::temp_dir();
    let (mut right, mut ran_wrong) = (0usize, 0usize);
    for (i, (said, want)) in LINES.iter().enumerate() {
        let pcm = common::speak(said, VOICES[i % VOICES.len()], None, &scratch)?;
        let o = pipeline.run(&pcm, &vocab)?;
        let heard = Heard { transcript: o.heard.clone(), command: o.command.clone(), masked: o.masked_max, stuck: o.stuck, no_speech: o.no_speech };
        let what = match act(&heard, tuning.command_voice_guard, tuning.command_voice_silence) {
            Act::Run(_) => "runs",
            Act::Confirm(_) => "asks",
            Act::Nothing => "nothing",
        };
        let ok = common::same_meaning(&o.command, want, &vocab);
        right += ok as usize;
        ran_wrong += (!ok && what == "runs") as usize;
        println!("{} {what:<7} {:<55} heard '{}' -> '{}' (mask {:.2})", if ok { "OK" } else { "--" }, want, o.heard, o.command, o.masked_max);
    }
    println!("{right} of {} right; {ran_wrong} wrong ones would run unasked", LINES.len());
    Ok(())
}
