//! First-person commands (docs/COMMANDS.md): the General orders his army with sentences in a
//! canonical English-like language, produced by the scroll-wheel menu, a typed entry and later
//! voice.
//!
//! This is presentation-side code. A command is resolved on the issuing client to object IDs and
//! points, and leaves as the standard engine messages the overhead view sends; nothing here runs
//! in `zhbo_logic_*` or reaches the sim's state.
//!
//! * [`actions`]: how the language covers every engine command type and order message.
//! * [`ast`]: the command AST.
//! * [`cluster`]: the groups `in the <region> group` names.
//! * [`normal`]: the synonyms and aliases, and the normaliser that turns a line into canonical
//!   candidates.
//! * [`vocab`]: the match's word lists and the verbs with the arguments each takes.
//! * [`parse`] and [`print`]: canonical text to the AST and back. Each AST has one spelling, and
//!   `parse(print(ast)) == ast`.
//! * [`world`]: the ABI records of the client services and the snapshot a command resolves against.
//! * [`resolve`]: a command to object ids and engine orders.
//! * [`session`]: the first-person command line (typing, submitting, the echo) and running a
//!   submitted line against the engine.

pub mod actions;
pub mod ast;
pub mod cluster;
pub mod normal;
pub mod parse;
pub mod print;
pub mod resolve;
pub mod session;
pub mod vocab;
pub mod world;

pub use ast::Command;
pub use parse::{parse, ParseError};
pub use print::print;
pub use vocab::{Noun, TargetSlot, ThingList, VerbDef, Vocabulary};

#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod tests;
