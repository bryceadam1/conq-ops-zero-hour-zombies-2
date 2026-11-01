//! Applying an option to the draft: weapons, attachments, offhands, perks and wildcards.

use super::*;

impl Editor {
    /// Apply option `i` of the highlighted slot to the draft. A choice that would spend an
    /// eleventh Pick-10 point, or add an attachment past the gun's cap, is refused and the draft
    /// is left exactly as it was (BO2's deny feedback reads this).
    pub(super) fn apply(&mut self, i: usize) -> ApplyResult {
        let before = self.draft.clone();
        if !self.apply_option(i) {
            return ApplyResult::Denied;
        }
        if !self.draft.valid() {
            self.draft = before;
            return ApplyResult::Denied;
        }
        if self.draft == before {
            ApplyResult::NoChange
        } else {
            ApplyResult::Changed
        }
    }

    /// Apply option `i` without the budget check; false means the option itself was refused (an
    /// attachment at its cap). Weapon picks drop attachments the new weapon does not offer;
    /// attachment picks toggle; perk picks set the tier row; wildcard picks toggle.
    pub(super) fn apply_option(&mut self, i: usize) -> bool {
        match self.slot {
            Slot::Primary => {
                let ids = self.gun_list(true);
                if let Some(id) = ids.get(i).copied() {
                    self.draft.primary = id.to_string();
                    self.keep_offered_atts(true);
                }
            }
            Slot::Secondary => {
                let ids = self.gun_list(false);
                if let Some(id) = ids.get(i).copied() {
                    self.draft.secondary = id.to_string();
                    self.keep_offered_atts(false);
                }
            }
            Slot::PrimaryAtts => {
                let shorts = attachment_shorts(&self.draft.primary);
                let max = self.draft.primary_att_max();
                if let Some(short) = shorts.get(i).copied() {
                    let base = self.draft.primary.clone();
                    if !toggle_att(&mut self.draft.primary_atts, short, max, |a, b| legal::compatible(&base, a, b)) {
                        return false;
                    }
                }
            }
            Slot::SecondaryAtts => {
                let shorts = attachment_shorts(&self.draft.secondary);
                let max = self.draft.secondary_att_max();
                if let Some(short) = shorts.get(i).copied() {
                    let base = self.draft.secondary.clone();
                    if !toggle_att(&mut self.draft.secondary_atts, short, max, |a, b| legal::compatible(&base, a, b)) {
                        return false;
                    }
                }
            }
            Slot::Lethal => {
                if let Some(id) = self.offhand_list(true).get(i).copied() {
                    self.pick_offhand(true, id);
                }
            }
            Slot::Tactical => {
                if let Some(id) = self.offhand_list(false).get(i).copied() {
                    self.pick_offhand(false, id);
                }
            }
            Slot::Perk1 | Slot::Perk2 | Slot::Perk3 | Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
                let tier = self.slot.perk_tier().unwrap_or(1);
                let perks: Vec<Perk> = perk_list(tier);
                if let Some(p) = perks.get(i).copied() {
                    self.set_perk(tier, p);
                }
            }
            Slot::Wildcards => {
                if let Some(w) = wildcard_list().get(i).copied() {
                    // A fourth bonus card (BO2's `maxBonusCards`) or a mutually exclusive one is
                    // refused, so the editor turns it into the deny cue and leaves the draft alone.
                    if !self.toggle_wildcard(w) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Pick an offhand slot's item. Another item replaces the held one at a count of one (BO2's
    /// `CACGrenades.Equip` sets `count` 1); picking the held item again takes or returns its second
    /// box when the slot has two ([`Draft::offhand_boxes`]: BO2's second box equips another of the
    /// held item, `caceditclass.lua:506-566`, and selecting a filled box unequips it).
    pub(super) fn pick_offhand(&mut self, lethal: bool, id: &str) {
        let k = usize::from(!lethal);
        let held = if lethal { &mut self.draft.lethal } else { &mut self.draft.tactical };
        if held == id {
            if self.draft.offhand_boxes(lethal) > 1 {
                self.draft.second_offhand[k] = !self.draft.second_offhand[k];
            }
        } else {
            *held = id.to_string();
            self.draft.second_offhand[k] = false;
        }
    }

    /// Take the second box of an offhand slot (BO2's click on the slot list's empty second box):
    /// false when the slot holds nothing, has one box, or already carries two. The caller checks
    /// the budget ([`Editor::apply_second_offhand`]).
    pub(super) fn take_second_offhand(&mut self, lethal: bool) -> bool {
        let k = usize::from(!lethal);
        let held = if lethal { &self.draft.lethal } else { &self.draft.tactical };
        if held.is_empty() || self.draft.offhand_boxes(lethal) < 2 || self.draft.second_offhand[k] {
            return false;
        }
        self.draft.second_offhand[k] = true;
        true
    }

    /// The pointer's click on an offhand button's empty second box: a second of the held item, one
    /// point, refused (the deny cue, the draft untouched) over budget or when there is no box.
    pub fn apply_second_offhand(&mut self, lethal: bool) -> ApplyResult {
        let before = self.draft.clone();
        if !self.take_second_offhand(lethal) {
            return ApplyResult::NoChange;
        }
        if !self.draft.valid() {
            self.draft = before;
            self.denied = true;
            return ApplyResult::Denied;
        }
        ApplyResult::Changed
    }

    /// Set the highlighted row's perk in a tier. The base row and the Greed row hold separate picks;
    /// the tier's option list refuses a perk already in the tier, so a tier's two picks differ.
    pub(super) fn set_perk(&mut self, tier: u8, p: Perk) {
        let idx = (tier as usize).clamp(1, 3) - 1;
        if self.slot.is_extra_perk() {
            self.draft.extra_perks[idx] = Some(p);
        } else {
            self.draft.perks[idx] = Some(p);
        }
    }

    /// Toggle a wildcard on or off; turning one off drops the element it enabled (the third primary
    /// attachment, the second secondary attachment, a tier's Greed perk, Tactician's tactical in the
    /// lethal slot, Danger Close's second lethal). False when a fourth card
    /// is refused (BO2 allows `maxBonusCards`, 3, per class), or a card [`Wildcard::excludes`] an
    /// equipped one: the option is denied untouched.
    pub(super) fn toggle_wildcard(&mut self, w: Wildcard) -> bool {
        if let Some(pos) = self.draft.wildcards.iter().position(|x| *x == w) {
            self.draft.wildcards.remove(pos);
            match w {
                Wildcard::PrimaryGunfighter => {
                    self.draft.primary_atts.truncate(2);
                }
                Wildcard::SecondaryGunfighter => {
                    self.draft.secondary_atts.truncate(1);
                }
                // Without Tactician a tactical item in the lethal slot is illegal: BO2's
                // `checkclasses.lua:677-686` clears the slot (`ClearSlot`).
                Wildcard::Tactician => {
                    if items::is_tactical_item(&self.draft.lethal) {
                        self.draft.lethal.clear();
                        self.draft.second_offhand[0] = false;
                    }
                }
                // Without Danger Close a lethal item's button has one box again: BO2's
                // `BonusCardUnequipped.bonuscard_danger_close` lowers `primarygrenadecount` 2 to 1
                // and clears `status2` (`bonuscardbutton.lua:490-545`).
                Wildcard::DangerClose => {
                    if !items::is_tactical_item(&self.draft.lethal) {
                        self.draft.second_offhand[0] = false;
                    }
                }
                _ => {
                    if let Some(tier) = w.greed_tier() {
                        let idx = (tier as usize).clamp(1, 3) - 1;
                        self.draft.extra_perks[idx] = None;
                        // The Greed row is gone; if it was highlighted, fall back to the base row.
                        if self.slot.is_extra_perk() && self.slot.perk_tier() == Some(tier) {
                            self.slot = base_perk_slot(tier);
                        }
                    }
                }
            }
            true
        } else if self.draft.wildcards_full() || w.excludes().is_some_and(|x| self.draft.wildcards.contains(&x)) {
            // A fourth card, or one the grid marks mutually exclusive with an equipped card.
            false
        } else {
            self.draft.wildcards.push(w);
            true
        }
    }

    /// Drop attachments the (new) weapon does not offer, keeping the offered ones in order.
    pub(super) fn keep_offered_atts(&mut self, primary: bool) {
        let base = if primary { self.draft.primary.clone() } else { self.draft.secondary.clone() };
        let allowed = allowed_shorts(&base);
        let max = if primary { self.draft.primary_att_max() } else { self.draft.secondary_att_max() };
        let atts = if primary { &mut self.draft.primary_atts } else { &mut self.draft.secondary_atts };
        atts.retain(|s| allowed.iter().any(|a| a == s));
        // BO2's compatibility is per attachment kind, so a kept set stays legal; the check is the
        // same one a stored class gets.
        let kept: Vec<String> = legal::sanitize(&base, &atts.iter().map(String::as_str).collect::<Vec<_>>()).into_iter().map(str::to_string).collect();
        atts.retain(|s| kept.contains(s));
        atts.truncate(max);
    }
}

/// Toggle an attachment short in the list, capped at `max`. Adding one first unequips the equipped
/// ones it is mutually exclusive with (`compatible` false; BO2's `ContentButtonSelected`, see
/// [`legal`]), then needs room under the cap. False means it could not be added (the cap is
/// reached): the caller turns that into the editor's deny cue and the list is left as it was.
pub(super) fn toggle_att(atts: &mut Vec<String>, short: &str, max: usize, compatible: impl Fn(&str, &str) -> bool) -> bool {
    if let Some(pos) = atts.iter().position(|s| s == short) {
        atts.remove(pos);
        return true;
    }
    let kept: Vec<String> = atts.iter().filter(|s| compatible(s, short)).cloned().collect();
    if kept.len() < max {
        *atts = kept;
        atts.push(short.to_string());
        true
    } else {
        false
    }
}
