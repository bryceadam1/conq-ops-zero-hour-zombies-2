//! File-format readers used by the launcher and asset converter.
//!
//! Everything here reads files from the player's own game installs. Nothing in this crate
//! embeds or redistributes game data.

pub mod big;
pub mod fastfile;
pub mod fx;
pub mod ipak;
pub mod iwi;
pub mod localize;
pub mod lzo;
pub mod sab;
pub mod sndalias;
pub mod t6zone;
pub mod w3d;
pub mod xanim;
pub mod weapon;
pub mod xmodel;

use std::fmt;

#[derive(Debug)]
pub enum FormatError {
    Io(std::io::Error),
    BadMagic { expected: &'static str, found: [u8; 4] },
    Truncated(&'static str),
    Corrupt(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "I/O error: {e}"),
            FormatError::BadMagic { expected, found } => {
                write!(f, "bad magic: expected {expected}, found {found:02x?}")
            }
            FormatError::Truncated(what) => write!(f, "file truncated while reading {what}"),
            FormatError::Corrupt(msg) => write!(f, "corrupt file: {msg}"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(e: std::io::Error) -> Self {
        FormatError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, FormatError>;
