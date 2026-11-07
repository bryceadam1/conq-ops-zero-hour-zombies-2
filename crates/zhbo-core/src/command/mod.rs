//! First-person commands (docs/COMMANDS.md): the General orders his army with sentences in a
//! canonical English-like language, produced by the scroll-wheel menu, a typed entry and later
//! voice.
//!
//! This is presentation-side code. A command is resolved on the issuing client to object IDs and
//! points, and leaves as the standard engine messages the overhead view sends; nothing here runs
//! in `zhbo_logic_*` or reaches the sim's state.
//!
//! * [`ast`]: the command AST.
//! * [`vocab`]: the match's word lists and the verbs with the arguments each takes.
//! * [`parse`] and [`print`]: canonical text to the AST and back. Each AST has one spelling, and
//!   `parse(print(ast)) == ast`.

pub mod ast;
pub mod parse;
pub mod print;
pub mod vocab;

pub use ast::Command;
pub use parse::{parse, ParseError};
pub use print::print;
pub use vocab::{Noun, TargetSlot, ThingList, VerbDef, Vocabulary};

#[cfg(test)]
mod tests;
