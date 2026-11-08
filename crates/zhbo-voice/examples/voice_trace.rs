//! One translation step by step (docs/COMMANDS.md section 11): what the model wanted at each
//! token and what the grammar's mask let it take.
//!
//! ```text
//! cargo run --release -p zhbo-voice --example voice_trace -- <model dir> <vocabulary line> <heard text>
//! ```

use std::path::Path;

use zhbo_core::command::Vocabulary;
use zhbo_voice::Translator;

fn main() -> Result<(), zhbo_voice::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: voice_trace <model dir> <vocabulary line> <heard text>".into());
    }
    let vocab = Vocabulary::from_line(&args[1]).ok_or("a vocabulary line needs eight lists")?;
    let mut model = Translator::load(Path::new(&args[0]), candle_core::Device::Cpu)?;
    let mut steps = Vec::new();
    let t = model.translate_traced(&args[2], Some(&vocab), &mut |s| steps.push(s))?;
    let tok = model.tokenizer();
    let name = |id: u32| tok.id_to_token(id).unwrap_or_default();
    for s in &steps {
        let mark = if s.wanted == s.took { "  " } else { "!!" };
        println!("{mark} wanted {:>14} {:.3}   took {:>14} {:.3}", name(s.wanted), s.wanted_p, name(s.took), s.took_p);
    }
    println!("=> {} (masked {:.3})", t.text, t.masked_max);
    Ok(())
}
