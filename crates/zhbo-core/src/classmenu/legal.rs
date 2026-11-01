//! Which attachments a class may carry together on a weapon: BO2's compatibility lists, read from
//! the installed catalog (`AttachmentDef::compatible`, converted from `mp/attachmenttable.csv`).
//!
//! * The pairwise rule is [`attachments::compatible`] (`FUN_008e6b20`).
//! * The editor never builds a forbidden set: picking an attachment first unequips every equipped
//!   one it is mutually exclusive with (`cacattachmentsmenu.lua` `ContentButtonSelected` →
//!   `UnequipAttachment` over the button's `mutuallyExclusiveIndices`, which
//!   `attachmentgridbutton.lua` fills from `Engine.AreAttachmentsCompatible`).
//! * A stored or hand-written id is cut down by BO2's own check ([`attachments::sanitize`],
//!   `FUN_008eb870`) — the class repair and the catalog both apply it.
//!
//! The count caps (Gunfighter) stay with the draft; this module is only the pairing rule, so a
//! later rule (dual wield as its own weapon, alt-weapon bases) extends one place.

use crate::attachments::{self, AttachmentDef};
use crate::weapons;

/// The attachment `short` resolves to on `base` (the weapon's offered variant), if it offers it.
fn def_of(base: &str, short: &str) -> Option<&'static AttachmentDef> {
    let d = weapons::catalog().get(base)?;
    d.allowed.iter().find(|(s, _)| s == short).and_then(|(_, asset)| weapons::catalog().attachment(asset))
}

/// Whether two attachments may share `base` (`AreAttachmentsCompatible`). An attachment the weapon
/// does not offer goes with nothing (BO2: a missing row is "unknown", never compatible).
pub fn compatible(base: &str, a: &str, b: &str) -> bool {
    matches!((def_of(base, a), def_of(base, b)), (Some(x), Some(y)) if attachments::compatible(x, y))
}

/// The equipped attachments `short` is mutually exclusive with on `base`, in `current`'s order —
/// what BO2's attachment grid marks on the button and unequips when it is picked.
pub fn exclusive_with<'a>(base: &str, short: &str, current: &'a [String]) -> Vec<&'a str> {
    current.iter().map(String::as_str).filter(|&c| c != short && !compatible(base, c, short)).collect()
}

/// Whether every pair of `shorts` may share `base`.
pub fn legal(base: &str, shorts: &[&str]) -> bool {
    shorts.iter().enumerate().all(|(i, a)| shorts[i + 1..].iter().all(|b| compatible(base, a, b)))
}

/// `shorts` after BO2's conflicting-attachment check ([`attachments::sanitize`]), in canonical
/// order; shorts `base` does not offer are dropped first.
pub fn sanitize<'a>(base: &str, shorts: &[&'a str]) -> Vec<&'a str> {
    let defs: Vec<&AttachmentDef> = shorts.iter().filter_map(|s| def_of(base, s)).collect();
    let kept = attachments::sanitize(&defs);
    let mut out: Vec<&str> = shorts.iter().copied().filter(|s| kept.iter().any(|k| k.short == *s)).collect();
    out.sort_unstable();
    out.dedup();
    out
}
