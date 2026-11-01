//! BO2's Create-a-Class editor: the Pick-10 slot editor behind the "CREATE A CLASS" screen.
//!
//! The pick screen (`classui::draw_pick`) is the *quick* choice between shipped presets and the
//! player's saved classes; this module is the full editor BO2 opens from the shell screen's
//! CREATE A CLASS row (a match is pick-only). It is a keyboard- and pointer-driven state machine
//! over the Pick-10 model:
//!
//! * **10 points** per class. Every chosen element — a primary or secondary weapon, each
//!   attachment, the lethal, the tactical, each perk, each wildcard — costs 1. The counter shows
//!   the points left; the editor refuses a choice the budget cannot afford.
//! * **Slots**, in BO2's order: Primary (with its attachments), Secondary (with its), Lethal,
//!   Tactical, the three perk tiers, and the wildcards. The weapon slots list the catalog's guns
//!   of the matching category; the perk slots list the three tiers from `weapons::perks`; the
//!   attachment slots list what the chosen weapon offers (`WeaponDef::allowed`).
//! * **Wildcards** (BO2's bonus cards, the weapon table's `BonusCard` rows from the player's
//!   statstable: name, description, slot, cost and grid order are data) unlock a second perk of a tier, a third primary attachment, a
//!   second secondary attachment, a second primary (Overkill), a tactical in the lethal slot
//!   (Tactician), or a second lethal (Danger Close). Each costs its card's cost (BO2: one), the
//!   extra element it enables another; Tactician and Danger Close exclude each other.
//!
//! Everything here is pure: the module owns the editing session's state, `step` folds one frame of
//! key edges into it, and `draw` renders it. The caller persists the result through the profile
//! (`Profile`) and applies it to the sim exactly like a shipped-class pick.
//!
//! A class the player finishes is a [`CustomClass`] (`classes.rs`) — the same type the shipped
//! registry's entries resolve to and the same wire format the pick message carries — so a class
//! built here needs no new transport: the sim receives its spec and applies it as-is.

use crate::classes::{self, CustomClass};
use crate::input::Key;
use crate::weapons::{self, Perk};
use items::{equipment_ids, gun_ids};

/// Which attachments go together: BO2's compatibility lists.
pub mod legal;
/// Which weapons the editor offers in which slot (BO2's statstable items).
pub mod items;

// The editor, one concern per file; everything public is re-exported here, so callers keep
// naming `classmenu::Draft`, `classmenu::Editor` and the rest.
/// The bonus cards' rules ([`Wildcard`]) and the grid's order.
mod wildcard;
/// The editor's rows ([`Slot`]) and the weapon slots' categories.
mod slot;
/// The personalize screens' grid editor ([`Personalize`]).
mod personalize;
/// BO2's content-grid links ([`grid_left`] and its siblings).
mod grid;
/// The class being edited ([`Draft`]) and its conversion to and from a [`CustomClass`].
mod draft;
/// The editing session's state and queries ([`Editor`]).
mod editor;
/// The per-frame key edges ([`Edges`]).
mod keys;
/// The editor's key and pointer fold (`Editor::step`, `Editor::step_pointer`).
mod step;
/// Applying an option to the draft (`Editor::apply` and the offhand/wildcard rules).
mod apply;
/// The option lists and row values ([`slot_value`]).
mod options;
/// What the editor offers, for the conformance emitter ([`offer`]).
mod offer;

pub use draft::*;
pub use editor::*;
pub use grid::*;
pub use keys::*;
pub use offer::*;
pub use options::*;
pub use personalize::*;
pub use slot::*;
pub use wildcard::*;

/// The Pick-10 budget: ten points per class, exactly BO2's.
pub const PICK_10: i32 = 10;

/// The longest class name the editor's name field accepts (BO2's field is a short one line).
pub const NAME_MAX: usize = 24;

/// The editor's tests live in their own file.
#[cfg(test)]
mod tests;
