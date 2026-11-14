//! BO2's action slots: the four per-player slots a script fills with `setactionslot( n, kind,
//! weapon )` and the `+actionslot n` binds press (`buttons_default.cfg`: d-pad up/down/left/right
//! are slots 1/2/3/4). The HUD draws them: the Zombies d-pad (`huddpadareazombie.lua`
//! `UpdateActionSlots`) and, in MP, the alt mode beside the ammo and the scorestreak column.
//!
//! What BO2 puts where:
//! * slot 1: Zombies' equipment (`_zm_equipment.gsc` `equipment_give`: `setactionslot( 1, "weapon",
//!   equipment )`), the buildables' carried part (`_zm_buildables.gsc`);
//! * slot 3: the weapon's alt mode in both modes (`_class.gsc:602`, `_zm.gsc:1320`);
//! * slot 4: Zombies' claymores (`_zm_weap_claymore.gsc:205`); MP's scorestreaks fill 4, 2, 1 in
//!   that order (`_class.gsc` `actionslotorder`).
//!
//! Deliberate divergence: a General's build tool is equipment in slot 2, the slot BO2 Zombies
//! leaves free, so it sits in the same place for every General. The field computer is not a slot:
//! it is the General's own key (it leaves first person, as BO2's scorestreak remotes do).

use crate::input::buttons;

/// The mod's equipment that lives in an action slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Equipment {
    /// The General's build tool (the hammer, `building::Hammer` while it is out).
    BuildTool,
}

crate::statehash::state_hash!(enum Equipment { BuildTool });

/// What one action slot holds (BO2's `setactionslot` kinds the mod uses).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionSlot {
    #[default]
    Empty,
    /// `"altMode"`: the current weapon's alt mode.
    AltMode,
    /// `"weapon"`: an equipment item.
    Equipment { item: Equipment },
}

crate::statehash::state_hash!(enum ActionSlot { Empty, AltMode, Equipment { item } });

/// The input button of each slot (index 0 is slot 1). Slot 3's button is the alt-mode toggle.
pub const BUTTONS: [u32; 4] = [buttons::ACTION_SLOT_1, buttons::ACTION_SLOT_2, buttons::ALT_MODE, buttons::ACTION_SLOT_4];

/// A player's four action slots (index 0 is slot 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionSlots {
    pub slots: [ActionSlot; 4],
}

crate::statehash::state_hash!(struct ActionSlots { slots });

impl ActionSlots {
    /// A General's slots: the alt mode in 3 (as BO2's `_class.gsc` sets it), the build tool in 2.
    pub const GENERAL: ActionSlots = ActionSlots {
        slots: [ActionSlot::Empty, ActionSlot::Equipment { item: Equipment::BuildTool }, ActionSlot::AltMode, ActionSlot::Empty],
    };

    /// The slot (1-4) holding `item`, if any.
    pub fn slot_of(&self, item: Equipment) -> Option<usize> {
        self.slots.iter().position(|s| *s == ActionSlot::Equipment { item }).map(|i| i + 1)
    }

    /// Whether `pressed` holds the press of the slot holding `item`.
    pub fn pressed(&self, item: Equipment, pressed: u32) -> bool {
        self.slot_of(item).is_some_and(|n| pressed & BUTTONS[n - 1] != 0)
    }

    /// Whether any slot holds `item`.
    pub fn holds(&self, item: Equipment) -> bool {
        self.slot_of(item).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_general_carries_the_build_tool_in_slot_two_and_the_alt_mode_in_three() {
        let s = ActionSlots::GENERAL;
        assert_eq!(s.slot_of(Equipment::BuildTool), Some(2));
        assert_eq!(s.slots[2], ActionSlot::AltMode);
        assert!(s.pressed(Equipment::BuildTool, buttons::ACTION_SLOT_2));
        assert!(!s.pressed(Equipment::BuildTool, buttons::ACTION_SLOT_1 | buttons::ACTION_SLOT_4 | buttons::ALT_MODE));
    }

    #[test]
    fn an_empty_slot_set_holds_nothing() {
        let s = ActionSlots::default();
        assert!(!s.holds(Equipment::BuildTool));
        assert!(!s.pressed(Equipment::BuildTool, buttons::ALL));
    }
}
