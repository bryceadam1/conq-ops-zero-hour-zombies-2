//! The editor grid's focus links: BO2's `CoD.CustomClass` navigation graph.
//!
//! BO2 does not derive the create-a-class grid's arrow moves from geometry: `AddWidgets` writes an
//! explicit `navigation.up/down/left/right` link on every button (`patch_ui_mp/ui/t6/
//! customclass.lua:863-991`), and LUI moves focus along a link when its direction is pressed
//! (`luielement.lua:714-785`, `handleGamepadButton`). Two `gain_focus` handlers then rewrite links
//! as the player moves (`:1303-1330`), so a column jump returns to the box it came from. [`LINKS`]
//! is that table and [`hook`] those handlers, as data; [`Nav`] is the live graph of one editing
//! session: the table, the mod's own rows spliced in where they are drawn, and the back-links the
//! hooks recorded.
//!
//! The mod's extra rows (BO2 has none of them): the class-name heading above PRIMARY, the SAVE
//! CLASS row under the third bonus card, and the Greed second-perk plates, each its own focus right
//! after its tier (BO2's perk button is one focus opening a two-pick menu). They join the vertical
//! cycles where they sit; a Greed plate shares its tier's Left/Right links and hook.

use super::Slot;
use crate::classes::MAX_BONUS_CARDS;

/// A focus move: LUI's `up`/`down`/`left`/`right` buttons (the arrows, and the mouse wheel for the
/// vertical pair — t6mp `FUN_009376a0` sends `MWHEELUP`/`MWHEELDOWN` as `up`/`down`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// One focusable box of the editor: a slot, and for the bonus-card row which of its three cards
/// (`bonusCardButtons[1..3]`); `card` is 0 for every other slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Focus {
    pub slot: Slot,
    pub card: usize,
}

impl Focus {
    pub const fn slot(slot: Slot) -> Focus {
        Focus { slot, card: 0 }
    }

    pub const fn card(card: usize) -> Focus {
        Focus { slot: Slot::Wildcards, card }
    }
}

const PRIMARY: Focus = Focus::slot(Slot::Primary);
const PRIMARY_ATTS: Focus = Focus::slot(Slot::PrimaryAtts);
const SECONDARY: Focus = Focus::slot(Slot::Secondary);
const SECONDARY_ATTS: Focus = Focus::slot(Slot::SecondaryAtts);
const PERK1: Focus = Focus::slot(Slot::Perk1);
const PERK2: Focus = Focus::slot(Slot::Perk2);
const PERK3: Focus = Focus::slot(Slot::Perk3);
const LETHAL: Focus = Focus::slot(Slot::Lethal);
const TACTICAL: Focus = Focus::slot(Slot::Tactical);
const CARD1: Focus = Focus::card(0);
const CARD2: Focus = Focus::card(1);
const CARD3: Focus = Focus::card(2);

use Dir::{Down, Left, Right, Up};

/// `CoD.CustomClass.AddWidgets`'s links, verbatim and in its order (`customclass.lua:863-991`).
pub const LINKS: [(Focus, Dir, Focus); 43] = [
    (PRIMARY, Up, CARD1),
    (PRIMARY, Down, PRIMARY_ATTS),
    (PRIMARY, Right, PERK1),
    (PRIMARY_ATTS, Up, PRIMARY),
    (PRIMARY_ATTS, Down, SECONDARY),
    (PRIMARY_ATTS, Left, PERK3),
    (PRIMARY_ATTS, Right, PERK3),
    (SECONDARY, Up, PRIMARY_ATTS),
    (SECONDARY, Down, SECONDARY_ATTS),
    (SECONDARY, Right, LETHAL),
    (SECONDARY_ATTS, Up, SECONDARY),
    (SECONDARY_ATTS, Down, CARD1),
    (SECONDARY_ATTS, Right, TACTICAL),
    (PERK1, Up, CARD3),
    (PERK1, Down, PERK2),
    (PERK1, Left, PRIMARY),
    (PERK1, Right, PRIMARY),
    (PERK2, Up, PERK1),
    (PERK2, Down, PERK3),
    (PERK2, Left, PRIMARY),
    (PERK2, Right, PRIMARY),
    (PERK3, Up, PERK2),
    (PERK3, Down, LETHAL),
    (PERK3, Left, PRIMARY_ATTS),
    (PERK3, Right, PRIMARY_ATTS),
    (LETHAL, Up, PERK3),
    (LETHAL, Down, TACTICAL),
    (LETHAL, Left, SECONDARY),
    (LETHAL, Right, SECONDARY),
    (TACTICAL, Up, LETHAL),
    (TACTICAL, Down, CARD3),
    (TACTICAL, Left, SECONDARY_ATTS),
    (TACTICAL, Right, SECONDARY_ATTS),
    (CARD1, Up, SECONDARY_ATTS),
    (CARD1, Down, PRIMARY),
    (CARD1, Right, CARD2),
    (CARD2, Up, SECONDARY_ATTS),
    (CARD2, Down, PRIMARY),
    (CARD2, Left, CARD1),
    (CARD2, Right, CARD3),
    (CARD3, Up, TACTICAL),
    (CARD3, Down, PERK1),
    (CARD3, Left, CARD2),
];

/// What a box's `gain_focus` handler does to the links (`customclass.lua`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    /// `Button_GainFocus` (`:1287-1300`): records the box for `RestoreFocus`, no link change.
    None,
    /// `Perk12Button_GainFocus` (`:1303-1312`, perk 1 `:267` and perk 2 `:326`):
    /// `navigation.left.navigation.right = self`, so PRIMARY's Right returns to the perk last on.
    LeftBack,
    /// `BonusCardButton_GainFocus` (`:1315-1330`, every card `:802`): `navigation.up.navigation.down
    /// = self` and `navigation.down.navigation.up = self`, so the column above and below return to
    /// the card last on.
    VerticalBack,
}

/// A box's hook. A Greed plate is part of its tier's button, so it carries the tier's.
pub fn hook(f: Focus) -> Hook {
    match f.slot {
        Slot::Perk1 | Slot::Perk2 | Slot::Perk1b | Slot::Perk2b => Hook::LeftBack,
        Slot::Wildcards => Hook::VerticalBack,
        _ => Hook::None,
    }
}

/// The focusable boxes of an editor whose walk rows are `slots` (`Editor::slots`): every row, with
/// the bonus-card row expanded to its `maxBonusCards` cards.
pub fn boxes(slots: &[Slot]) -> Vec<Focus> {
    let mut out = Vec::with_capacity(slots.len() + MAX_BONUS_CARDS);
    for s in slots {
        if *s == Slot::Wildcards {
            out.extend((0..MAX_BONUS_CARDS).map(Focus::card));
        } else {
            out.push(Focus::slot(*s));
        }
    }
    out
}

/// One editing session's navigation state: the back-links the hooks wrote, applied over the
/// table each time the graph is built (the rows change with the Greed wildcards, so the graph is
/// rebuilt from its parts rather than stored).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Nav {
    back: Vec<(Focus, Dir, Focus)>,
}

/// A built graph: `(from, dir, to)` links.
pub type Graph = Vec<(Focus, Dir, Focus)>;

fn get(g: &Graph, from: Focus, dir: Dir) -> Option<Focus> {
    g.iter().find(|(f, d, _)| *f == from && *d == dir).map(|(_, _, t)| *t)
}

fn set(g: &mut Graph, from: Focus, dir: Dir, to: Focus) {
    match g.iter_mut().find(|(f, d, _)| *f == from && *d == dir) {
        Some(link) => link.2 = to,
        None => g.push((from, dir, to)),
    }
}

/// Put `row` directly above `below` in its column: every Down into `below` now enters `row`,
/// `row` goes on down to `below`, and its Up is the one `below` had.
fn insert_above(g: &mut Graph, below: Focus, row: Focus) {
    let up = get(g, below, Up);
    for link in g.iter_mut() {
        if link.1 == Down && link.2 == below {
            link.2 = row;
        }
    }
    if let Some(up) = up {
        set(g, row, Up, up);
    }
    set(g, row, Down, below);
    set(g, below, Up, row);
}

impl Nav {
    /// The graph over the boxes present: BO2's table, the mod's rows spliced in, then the recorded
    /// back-links (those whose ends still exist).
    pub fn graph(&self, present: &[Focus]) -> Graph {
        let has = |f: &Focus| present.contains(f);
        let mut g: Graph = LINKS.iter().copied().filter(|(f, _, t)| has(f) && has(t)).collect();
        // The Greed plates: under their tier in the right column, sharing its Left/Right.
        for (base, extra) in [(Slot::Perk1, Slot::Perk1b), (Slot::Perk2, Slot::Perk2b), (Slot::Perk3, Slot::Perk3b)] {
            let (base, extra) = (Focus::slot(base), Focus::slot(extra));
            if !has(&extra) || !has(&base) {
                continue;
            }
            if let Some(below) = get(&g, base, Down) {
                insert_above(&mut g, below, extra);
            }
            for dir in [Left, Right] {
                if let Some(t) = get(&g, base, dir) {
                    set(&mut g, extra, dir, t);
                }
            }
        }
        // The class-name heading above PRIMARY; SAVE CLASS under the third card (above PERK 1 in
        // the right column's cycle).
        if has(&Focus::slot(Slot::Name)) && has(&PRIMARY) {
            insert_above(&mut g, PRIMARY, Focus::slot(Slot::Name));
        }
        if has(&Focus::slot(Slot::Save)) && has(&PERK1) {
            insert_above(&mut g, PERK1, Focus::slot(Slot::Save));
        }
        for (f, d, t) in &self.back {
            if has(f) && has(t) {
                set(&mut g, *f, *d, *t);
            }
        }
        g
    }

    /// Where `dir` leads from `from` (`None`: no link, the press does nothing).
    pub fn target(&self, present: &[Focus], from: Focus, dir: Dir) -> Option<Focus> {
        get(&self.graph(present), from, dir)
    }

    /// `f` gained focus (a key move or a hover): run its hook, recording the back-links it writes.
    pub fn focused(&mut self, present: &[Focus], f: Focus) {
        let g = self.graph(present);
        let mut write = |from: Focus, dir: Dir| {
            match self.back.iter_mut().find(|(bf, bd, _)| *bf == from && *bd == dir) {
                Some(link) => link.2 = f,
                None => self.back.push((from, dir, f)),
            }
        };
        match hook(f) {
            Hook::None => {}
            Hook::LeftBack => {
                if let Some(left) = get(&g, f, Left) {
                    write(left, Right);
                }
            }
            Hook::VerticalBack => {
                if let Some(up) = get(&g, f, Up) {
                    write(up, Down);
                }
                if let Some(down) = get(&g, f, Down) {
                    write(down, Up);
                }
            }
        }
    }
}
