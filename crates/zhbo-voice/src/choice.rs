//! Choosing the next phrase as a multiple-choice question (docs/COMMANDS.md section 11,
//! "Multiple choice"): the grammar's options for the next step, the order as heard and the
//! command so far go into the encoder together, and the model answers with an option's letter.
//! The encoder sees every candidate beside the transcript, so it can compare them (`korgor` or
//! `korvel`?), which a decoder scoring one candidate at a time cannot.
//!
//! Asking costs an encoder pass per step, so the generator asks only where it is unsure
//! ([`crate::options::OptionDecoder::decode_choice`]), and only about its own likeliest options.

/// The most options a question offers, lettered `A` onwards.
pub const MAX_OPTIONS: usize = 8;
/// The letters, in order.
pub const LETTERS: [&str; MAX_OPTIONS] = ["A", "B", "C", "D", "E", "F", "G", "H"];
/// How the end of the command reads as an option.
pub const END: &str = "(end of command)";

/// The question for the step after `so_far` (canonical phrases), with `options` (`""` is the end
/// of the command) in the order given.
pub fn question(heard: &str, so_far: &[String], options: &[String]) -> String {
    let so_far = if so_far.is_empty() { "(nothing yet)".to_string() } else { format!("\"{}\"", so_far.join(" ")) };
    let lines: Vec<String> = options.iter().zip(LETTERS).map(|(o, l)| format!("{l}. {}", if o.is_empty() { END } else { o.as_str() })).collect();
    format!("Order: \"{heard}\"\nCommand so far: {so_far}\nWhat comes next in the command?\nOPTIONS:\n{}\nAnswer:", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_letters_its_options() {
        let q = question("rangers go to the bridge", &["rangers".into()], &["move to".into(), "guard".into(), String::new()]);
        assert_eq!(q, "Order: \"rangers go to the bridge\"\nCommand so far: \"rangers\"\nWhat comes next in the command?\nOPTIONS:\nA. move to\nB. guard\nC. (end of command)\nAnswer:");
        assert!(question("x", &[], &["units".into()]).contains("Command so far: (nothing yet)"));
    }
}
