use super::*;
use super::items::cac_entries;

    #[test]
    fn the_default_draft_spends_the_base_kit_points() {
        let d = Draft::default();
        // 2 weapons + 2 primary attachments + lethal + tactical + 3 perks = 9; one point free.
        assert_eq!(d.spent(), 9);
        assert_eq!(d.remaining(), 1);
        assert!(d.valid());
        assert_eq!(d.primary_id(), "an94_mp+gl+reflex");
        assert_eq!(d.secondary_id(), "fnp45_mp");
    }

    #[test]
    fn tab_marks_the_personalize_action_without_touching_the_draft() {
        let mut e = Editor::new(Draft::default());
        let before = e.draft.clone();
        let slot = e.slot;
        assert_eq!(e.step(Edges { personalize: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.personalize, Some(PersonalizeKind::Knife), "Tab marks the knife-personalization action");
        assert_eq!(e.draft, before);
        assert_eq!(e.slot, slot);
        // The flag is one-shot: the next frame clears it.
        assert_eq!(e.step(Edges::default()), Outcome::Continue);
        assert_eq!(e.personalize, None);
    }

    #[test]
    fn the_primary_weapon_row_opens_camo_and_an_optic_row_opens_reticles() {
        // `caceditclass.lua:748` `AddCamoButton` on the primary weapon row, `:825`
        // `AddReticleButton` on the primary optic row.
        let mut e = Editor::new(Draft::default());
        assert_eq!(e.step(Edges { camo: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.personalize, Some(PersonalizeKind::Camo));
        assert_eq!(e.step(Edges::default()), Outcome::Continue);
        assert_eq!(e.personalize, None);

        e.slot = Slot::PrimaryAtts;
        e.option = (0..e.options().len())
            .find(|i| e.option_attachment(*i).is_some_and(|s| crate::hud::OPTICS.contains(&s)))
            .expect("the default primary carries an optic");
        assert_eq!(e.step(Edges { reticle: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.personalize, Some(PersonalizeKind::Reticle));
        assert_eq!(e.step(Edges::default()), Outcome::Continue);
        assert_eq!(e.personalize, None);

        // The attachment grid (picking) carries the same reticle outcome.
        e.picking = true;
        assert_eq!(e.step(Edges { reticle: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.personalize, Some(PersonalizeKind::Reticle));
    }

    #[test]
    fn personalize_grids_use_bo2s_column_counts() {
        // `caccamomenu.lua:9`: `CamoGridColumns = 6`; `cacreticles.lua:9`: `GridColumns = 5`.
        assert_eq!(PersonalizeKind::Camo.columns(), 6);
        assert_eq!(PersonalizeKind::Knife.columns(), 6);
        assert_eq!(PersonalizeKind::Reticle.columns(), 5);
        assert_eq!(PersonalizeKind::ReticleColor.columns(), 5);
        // The walk wraps within the row and column exactly as `contentgrid.lua` does.
        assert_eq!(grid_right(5, 12, 6), 0, "the last cell of a row wraps to its first");
        assert_eq!(grid_down(0, 12, 6), 6);
        assert_eq!(grid_up(0, 12, 6), 6, "the first row wraps to the last");
    }

    #[test]
    fn personalize_opens_on_the_current_choice_and_applies_the_confirmed_cell() {
        let mut d = Draft { camo: "gold".into(), ..Draft::default() };
        let choices: Vec<String> = ["none", "gold", "diamond", "beast", "bengal", "blue"].iter().map(|s| s.to_string()).collect();
        let mut p = Personalize::new(PersonalizeKind::Camo, choices, &d);
        assert_eq!(p.highlight, 1, "opens on the draft's current camo");
        // Right moves within the row.
        assert_eq!(p.step(Edges { right: true, ..Default::default() }, &mut d), Outcome::Continue);
        assert_eq!(p.highlight, 2);
        // A single row wraps down back onto itself (`contentgrid.lua`'s back-link).
        assert_eq!(p.step(Edges { down: true, ..Default::default() }, &mut d), Outcome::Continue);
        assert_eq!(p.highlight, 2, "one row wraps down back onto itself");
        // Confirm applies the highlighted choice and leaves the screen.
        assert_eq!(p.step(Edges { confirm: true, ..Default::default() }, &mut d), Outcome::Save);
        assert_eq!(d.camo, "diamond");
    }

    #[test]
    fn personalize_cancel_leaves_the_draft_untouched() {
        let mut d = Draft { reticle: "red_dot".into(), ..Draft::default() };
        let choices: Vec<String> = ["none", "red_dot", "acog"].iter().map(|s| s.to_string()).collect();
        let mut p = Personalize::new(PersonalizeKind::Reticle, choices, &d);
        assert_eq!(p.highlight, 1);
        assert_eq!(p.step(Edges { cancel: true, ..Default::default() }, &mut d), Outcome::Cancel);
        assert_eq!(d.reticle, "red_dot");
    }

    #[test]
    fn personalize_with_no_choices_closes_without_applying() {
        let mut d = Draft::default();
        let mut p = Personalize::new(PersonalizeKind::Knife, Vec::new(), &d);
        assert_eq!(p.step(Edges { confirm: true, ..Default::default() }, &mut d), Outcome::Cancel);
        assert!(d.knife.is_empty());
    }

    #[test]
    fn personalize_editor_moves_the_highlight_and_applies_the_choice() {
        let mut d = Draft { camo: "camo_1".into(), ..Default::default() };
        let choices = vec!["camo_1".into(), "camo_2".into(), "camo_3".into()];
        let mut p = Personalize::new(PersonalizeKind::Camo, choices, &d);
        assert_eq!(p.highlight, 0, "opens on the draft's current camo");
        // Six columns means three choices is one short row: Right walks to the second cell.
        assert_eq!(p.step(Edges { right: true, ..Default::default() }, &mut d), Outcome::Continue);
        assert_eq!(p.highlight, 1);
        // Confirm applies the highlighted choice and closes the screen.
        assert_eq!(p.step(Edges { confirm: true, ..Default::default() }, &mut d), Outcome::Save);
        assert_eq!(d.camo, "camo_2");
        // Cancel closes without applying.
        let mut p = Personalize::new(PersonalizeKind::Reticle, vec!["ret_a".into()], &d);
        d.reticle = "old".into();
        assert_eq!(p.step(Edges { cancel: true, ..Default::default() }, &mut d), Outcome::Cancel);
        assert_eq!(d.reticle, "old");
    }

    #[test]
    fn personalize_editor_wraps_across_rows_like_contentgrid() {
        let mut d = Draft::default();
        let choices: Vec<String> = (0..12).map(|i| format!("camo_{i}")).collect();
        let mut p = Personalize::new(PersonalizeKind::Camo, choices, &d);
        assert_eq!(p.highlight, 0);
        assert_eq!(p.step(Edges { up: true, ..Default::default() }, &mut d), Outcome::Continue);
        assert_eq!(p.highlight, 6, "first row wraps to the last row");
        assert_eq!(p.step(Edges { down: true, ..Default::default() }, &mut d), Outcome::Continue);
        assert_eq!(p.highlight, 0, "last row wraps to the first");
        assert_eq!(p.step(Edges { left: true, ..Default::default() }, &mut d), Outcome::Continue);
        assert_eq!(p.highlight, 5, "first cell wraps to the row's last cell");
    }

    #[test]
    fn every_wildcard_costs_a_point_and_round_trips_through_its_id() {
        for w in Wildcard::ALL {
            assert_eq!(Wildcard::by_id(w.id()), Some(w));
            assert!(!w.name().is_empty());
        }
        assert_eq!(Wildcard::Perk1Greed.greed_tier(), Some(1));
        assert_eq!(Wildcard::Overkill.greed_tier(), None);
    }

    #[test]
    fn weapon_picks_drop_attachments_the_new_weapon_does_not_offer() {
        // The secondary moves from a primary-class gun carrying its attachments to the Tac-45,
        // which offers neither; the pick keeps only what the new weapon can take.
        let mut e = Editor::new(Draft {
            secondary: "an94_mp".into(),
            secondary_atts: vec!["gl".into(), "reflex".into()],
            wildcards: vec![Wildcard::Overkill],
            ..Draft::default()
        });
        e.draft.secondary = "fnp45_mp".into();
        e.keep_offered_atts(false);
        assert!(e.draft.secondary_atts.is_empty(), "{:?}", e.draft.secondary_atts);
    }

    #[test]
    fn weapon_lists_skip_alt_mode_variants_and_repeat_no_display_name() {
        // `gl_an94_mp` is the AN-94's under-barrel launcher, not a class weapon of its own. The
        // built-in table carries it so the assertion is real without a BO2 install.
        let ids = gun_ids(&items::slots_of(true, &[]));
        assert!(!ids.contains(&"gl_an94_mp"), "an alt-mode variant is not in the class list: {ids:?}");
        assert_eq!(ids.iter().filter(|id| **id == "an94_mp").count(), 1, "the base appears once");
        // Every listed id is a base entry (no alt back-pointer, no `+`/`~` variant id), and no
        // display name repeats.
        let entries = cac_entries();
        for id in &ids {
            let d = entries.iter().find(|d| d.id == **id).expect("listed ids are catalog entries");
            assert!(d.alt.is_empty(), "{id} is an alt variant");
            assert!(!d.id.contains(['+', '~']), "{id} is a built variant id");
        }
        let mut names: Vec<&str> = ids.iter().map(|id| weapons::weapon(id).display_name.as_str()).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n, "no weapon is listed twice by display name");
        // The same filter backs the secondary and equipment lists.
        for class in [WeaponClass::Pistol, WeaponClass::Lethal, WeaponClass::Tactical] {
            let list = equipment_ids(class);
            assert!(list.iter().all(|id| weapons::weapon(id).alt.is_empty()), "{class:?}: {list:?}");
        }
    }

    #[test]
    fn attachments_cap_at_two_until_the_gunfighter_wildcard() {
        let mut d = Draft { primary: "an94_mp".into(), primary_atts: vec![], ..Draft::default() };
        let shorts = allowed_shorts(&d.primary);
        assert!(shorts.len() >= 2);
        let mut max = d.primary_att_max();
        toggle_att(&mut d.primary_atts, shorts[0], max, |_, _| true);
        toggle_att(&mut d.primary_atts, shorts[1], max, |_, _| true);
        // A third is refused without the wildcard.
        let third = shorts.get(2).copied();
        if let Some(t) = third {
            toggle_att(&mut d.primary_atts, t, max, |_, _| true);
            assert_eq!(d.primary_atts.len(), 2, "capped at 2");
        }
        d.wildcards.push(Wildcard::PrimaryGunfighter);
        max = d.primary_att_max();
        assert_eq!(max, 3);
        if let Some(t) = third {
            toggle_att(&mut d.primary_atts, t, max, |_, _| true);
            assert_eq!(d.primary_atts.len(), 3);
        }
    }

    #[test]
    fn toggling_off_a_gunfighter_wildcard_truncates_its_extra_attachment() {
        let mut d = Draft { primary: "an94_mp".into(), primary_atts: vec!["gl".into(), "reflex".into(), "stock".into()], wildcards: vec![Wildcard::PrimaryGunfighter], ..Draft::default() };
        assert_eq!(d.spent(), 11);
        assert!(!d.valid());
        d.wildcards.remove(0);
        d.primary_atts.truncate(2);
        assert_eq!(d.spent(), 9);
        assert!(d.valid());
    }

    #[test]
    fn the_budget_refuses_an_over_spend() {
        let mut d = Draft::default();
        // Default is 9; adding a wildcard makes 10 (valid), another makes 11 (invalid).
        d.wildcards.push(Wildcard::Overkill);
        assert_eq!(d.remaining(), 0);
        assert!(d.valid());
        d.wildcards.push(Wildcard::Tactician);
        assert_eq!(d.remaining(), -1);
        assert!(!d.valid());
    }

    #[test]
    fn a_draft_round_trips_through_custom_class() {
        let mut d = Draft { name: "My Rifle".into(), primary: "an94_mp".into(), primary_atts: vec!["reflex".into()], ..Draft::default() };
        d.perks = [Some(Perk::LIGHTWEIGHT), Some(Perk::TOUGHNESS), Some(Perk::DEXTERITY)];
        let c = d.to_custom();
        assert_eq!(c.name, "My Rifle");
        assert_eq!(c.primary, "an94_mp+reflex");
        let back = Draft::from_custom(&c);
        assert_eq!(back.name, "My Rifle");
        assert_eq!(back.primary, "an94_mp");
        assert_eq!(back.primary_atts, vec!["reflex".to_string()]);
        assert_eq!(back.perks, d.perks);
        assert_eq!(back.to_spec(), d.to_spec());
    }

    #[test]
    fn the_spec_applies_over_the_base_kit() {
        let d = Draft { primary: "an94_mp".into(), primary_atts: vec![], perks: [Some(Perk::FLAK_JACKET), None, None], ..Draft::default() };
        let l = crate::weapons::Loadout::default().with_spec(&d.to_spec());
        assert_eq!(l.primary, "an94_mp");
        // The spec carries exactly the set perks; unset tiers fall back to the base kit's slots.
        assert_eq!(l.perks, vec![Perk::FLAK_JACKET]);
    }

    #[test]
    fn slot_navigation_wraps_around_bo2s_order() {
        let e = Editor::new(Draft::default());
        assert_eq!(e.next_slot(Slot::Name), Slot::Primary);
        assert_eq!(e.next_slot(Slot::Primary), Slot::PrimaryAtts);
        // The first row wraps up to the Save row, and Save wraps down to the first row.
        assert_eq!(e.prev_slot(Slot::Name), Slot::Save);
        assert_eq!(e.next_slot(Slot::Wildcards), Slot::Save);
        assert_eq!(e.next_slot(Slot::Save), Slot::Name);
        assert_eq!(e.prev_slot(Slot::Save), Slot::Wildcards);
        // Walk the whole list and come back.
        let mut s = Slot::Name;
        for _ in 0..Slot::ALL.len() {
            s = e.next_slot(s);
        }
        assert_eq!(s, Slot::Name);
    }

    #[test]
    fn the_save_row_confirms_into_a_save_and_has_no_list() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Save;
        // The Save row carries no options, so it can never open a list.
        assert!(e.options().is_empty());
        // Confirming it leaves the editor with a save; a down edge walks off it to the first row.
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Save);
        assert!(!e.picking, "the save row does not enter the list state");
        assert_eq!(e.step(Edges { down: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.slot, Slot::Name);
        // Escape still cancels (without saving).
        e.slot = Slot::Save;
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
    }

    #[test]
    fn the_editor_opens_the_list_on_the_held_option() {
        // The secondary slot opens on the Tac-45, which the built-in table holds.
        let mut e = Editor::new(Draft { secondary: "fnp45_mp".into(), ..Draft::default() });
        e.slot = Slot::Secondary;
        let idx = e.current_option_index();
        assert_eq!(gun_ids(&e.secondary_slots())[idx], "fnp45_mp");
        e.picking = true;
        e.option = idx;
        // Confirming the held weapon leaves the draft unchanged.
        let before = e.draft.clone();
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft, before);
    }

    #[test]
    fn cancel_closes_the_list_then_the_editor() {
        let mut e = Editor::new(Draft::default());
        e.picking = true;
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.picking);
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
    }

    #[test]
    fn perk_picks_land_in_their_tier() {
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Perk2;
        let perks = perk_list(2);
        let scav = perks.iter().position(|p| p.def().id == "Scavenger").expect("Scavenger is tier 2");
        e.picking = true;
        e.option = scav;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.perks[1], Some(Perk::SCAVENGER));
        // Tier 1 and 3 are untouched.
        assert_eq!(e.draft.perks[0], Some(Perk::LIGHTWEIGHT));
        assert_eq!(e.draft.perks[2], Some(Perk::DEXTERITY));
    }

    #[test]
    fn option_chosen_marks_the_drafts_current_elements() {
        // Primary: the held gun's index is the chosen one; no other weapon is.
        let mut e = Editor::new(Draft::default());
        e.slot = Slot::Primary;
        let held = e.current_option_index();
        assert!(e.option_chosen(held));
        assert!(!e.option_chosen(held + 1));
        // Attachments: the AN-94's gl and reflex are on the draft, so their list entries are chosen.
        e.slot = Slot::PrimaryAtts;
        let shorts = attachment_shorts(&e.draft.primary, &e.draft.primary_atts);
        for (i, short) in shorts.iter().enumerate() {
            let on = e.draft.primary_atts.iter().any(|a| a == short);
            assert_eq!(e.option_chosen(i), on, "attachment {short}");
        }
        // Perks: the tier's held perk is chosen.
        e.slot = Slot::Perk1;
        let p1 = perk_list(1).iter().position(|p| *p == Perk::LIGHTWEIGHT).expect("Lightweight is tier 1");
        assert!(e.option_chosen(p1));
        // Wildcards: none are on the default draft.
        e.slot = Slot::Wildcards;
        assert!(!e.option_chosen(0));
    }

    #[test]
    fn slot_values_read_as_the_resulting_class() {
        let d = Draft::default();
        assert_eq!(slot_value(&d, Slot::Primary), weapons::weapon("an94_mp").display_name);
        assert_eq!(slot_value(&d, Slot::Secondary), weapons::weapon("fnp45_mp").display_name);
        // The attachment row lists the display names, not the shorts.
        let atts = slot_value(&d, Slot::PrimaryAtts);
        assert!(atts.contains("Grenade Launcher") && atts.contains("Reflex"), "keeps names: {atts}");
        assert_ne!(atts, "—");
        // The perk rows read the tier's held perk.
        assert_eq!(slot_value(&d, Slot::Perk1), "Lightweight");
        assert_eq!(slot_value(&d, Slot::Wildcards), "—");
    }

    // --- Option 2: the class-name row types text ---

    #[test]
    fn the_name_row_starts_text_entry_and_letters_append() {
        let mut e = Editor::new(Draft { name: String::new(), ..Draft::default() });
        e.slot = Slot::Name;
        // The name row has no option list: Enter starts text entry instead of opening one.
        assert!(e.options().is_empty());
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(e.naming, "Enter on the name row starts text entry");
        assert!(!e.picking, "the name row never opens an option list");
        // Letters append in the order they were pressed.
        e.step(Edges { typed: vec![Key::M, Key::P], ..Default::default() });
        assert_eq!(e.draft.name, "MP");
        // Space types a space; Backspace deletes the last character.
        e.step(Edges { typed: vec![Key::Space, Key::A], ..Default::default() });
        assert_eq!(e.draft.name, "MP A");
        e.step(Edges { backspace: true, ..Default::default() });
        assert_eq!(e.draft.name, "MP ");
        // Enter leaves the field (the text stays); the row stays highlighted and not in the list.
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.naming);
        assert_eq!(e.draft.name, "MP ");
        assert_eq!(e.slot, Slot::Name);
    }

    #[test]
    fn the_name_field_caps_at_bo2s_length() {
        let mut e = Editor::new(Draft { name: String::new(), ..Draft::default() });
        e.slot = Slot::Name;
        e.naming = true;
        // Type far past the cap; the field refuses characters once it is full.
        let keys: Vec<Key> = std::iter::repeat_n(Key::A, NAME_MAX + 5).collect();
        e.step(Edges { typed: keys, ..Default::default() });
        assert_eq!(e.draft.name.chars().count(), NAME_MAX);
        assert_eq!(e.draft.name, "A".repeat(NAME_MAX));
    }

    #[test]
    fn digits_type_into_the_name_field() {
        // BO2's name charset is uppercase letters, digits and the space. The digit row past 1-6
        // was appended to the Key enum in Phase 2; every digit types here.
        for (key, ch) in [
            (Key::Key0, '0'),
            (Key::Key1, '1'),
            (Key::Key2, '2'),
            (Key::Key3, '3'),
            (Key::Key4, '4'),
            (Key::Key5, '5'),
            (Key::Key6, '6'),
            (Key::Key7, '7'),
            (Key::Key8, '8'),
            (Key::Key9, '9'),
        ] {
            assert_eq!(char_of(key), Some(ch));
            assert!(is_typable(key));
        }
        let mut e = Editor::new(Draft { name: String::new(), ..Draft::default() });
        e.slot = Slot::Name;
        e.naming = true;
        e.step(Edges { typed: vec![Key::Key1, Key::Key2, Key::Key7, Key::Key0], ..Default::default() });
        assert_eq!(e.draft.name, "1270");
        // Numbers are part of the spec grammar too: a saved class keeps the typed name.
        let c = e.draft.to_custom();
        assert_eq!(Draft::from_custom(&c).name, "1270");
    }

    #[test]
    fn escape_leaves_the_name_field_without_cancelling_the_editor() {
        let mut e = Editor::new(Draft { name: "Rifle".into(), ..Draft::default() });
        e.slot = Slot::Name;
        e.naming = true;
        // Escape leaves the field, it does not cancel the whole editor while naming.
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.naming);
        assert_eq!(e.draft.name, "Rifle");
        // A second Escape (now out of the field) cancels the editor as usual.
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
    }

    #[test]
    fn the_name_round_trips_through_a_saved_class() {
        let mut d = Draft { name: "My Rifle".into(), ..Draft::default() };
        d.primary = "an94_mp".into();
        let c = d.to_custom();
        assert_eq!(c.name, "My Rifle");
        let back = Draft::from_custom(&c);
        assert_eq!(back.name, "My Rifle");
    }

    // --- Option 3: Greed second perks and Overkill's second primary ---

    #[test]
    fn a_greed_wildcard_splices_in_a_second_perk_row() {
        let mut e = Editor::new(Draft::default());
        // Without the wildcard the second row is absent from the walk.
        assert!(!e.slots().contains(&Slot::Perk1b));
        assert_eq!(e.slots().len(), Slot::ALL.len());
        // Turning on Perk 1 Greed inserts the row right after the base Perk 1 row.
        e.draft.wildcards.push(Wildcard::Perk1Greed);
        let slots = e.slots();
        let base = slots.iter().position(|s| *s == Slot::Perk1).expect("Perk 1 row");
        assert_eq!(slots[base + 1], Slot::Perk1b, "the Greed row follows its base row");
        // It reads as its tier's header and knows its tier.
        assert_eq!(Slot::Perk1b.label(), "PERK 1");
        assert_eq!(Slot::Perk1b.perk_tier(), Some(1));
        assert!(Slot::Perk1b.is_extra_perk());
        assert!(!Slot::Perk1.is_extra_perk());
    }

    #[test]
    fn the_second_perk_row_sets_the_extra_pick_without_touching_the_base() {
        // Six points of kit, so the Greed wildcard and its second perk both fit the ten.
        let d = Draft { primary_atts: vec!["reflex".into()], ..Draft::default() };
        assert_eq!(d.spent(), 8);
        let mut e = Editor::new(d);
        e.draft.wildcards.push(Wildcard::Perk1Greed);
        assert_eq!(e.draft.spent(), 9);
        e.slot = Slot::Perk1b;
        let perks = perk_list(1);
        // Pick a tier-1 perk other than the base pick (Lightweight).
        let choice = perks
            .iter()
            .position(|p| *p != Perk::LIGHTWEIGHT)
            .expect("a second tier-1 perk");
        e.picking = true;
        e.option = choice;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.extra_perks[0], Some(perks[choice]));
        // The base pick is untouched, and both picks resolve into the spec.
        assert_eq!(e.draft.perks[0], Some(Perk::LIGHTWEIGHT));
        let perks_in_order = e.draft.perks_in_order();
        assert_eq!(perks_in_order.len(), 4, "three base perks plus the Greed pick");
        assert!(perks_in_order.contains(&perks[choice]));
    }

    #[test]
    fn turning_off_a_greed_wildcard_drops_its_second_perk() {
        let mut e = Editor::new(Draft::default());
        e.draft.wildcards.push(Wildcard::Perk2Greed);
        e.slot = Slot::Perk2b;
        e.draft.extra_perks[1] = Some(Perk::SCAVENGER);
        // Highlighted on the Greed row, turning the wildcard off falls back to the base row and
        // clears the extra pick.
        e.toggle_wildcard(Wildcard::Perk2Greed);
        assert!(!e.draft.wildcards.contains(&Wildcard::Perk2Greed));
        assert_eq!(e.draft.extra_perks[1], None);
        assert_eq!(e.slot, Slot::Perk2, "the highlight returns to the base row");
        // The Greed row is gone from the walk again.
        assert!(!e.slots().contains(&Slot::Perk2b));
    }

    #[test]
    fn a_greed_perk_costs_its_point_in_the_budget() {
        let mut d = Draft::default();
        let base = d.spent();
        d.extra_perks[0] = Some(Perk::SCAVENGER);
        assert_eq!(d.spent(), base + 1, "the Greed perk costs a point");
        d.wildcards.push(Wildcard::Perk1Greed);
        assert_eq!(d.spent(), base + 2, "the wildcard costs a point too");
    }

    #[test]
    fn overkill_offers_primary_guns_in_the_secondary_list() {
        let mut e = Editor::new(Draft::default());
        // The secondary lists BO2's `secondary` items: the pistols and the launchers.
        let base = gun_ids(&e.secondary_slots());
        assert!(base.contains(&"fnp45_mp") && base.contains(&"smaw_mp"), "{base:?}");
        assert!(!base.contains(&"an94_mp"), "a primary item needs Overkill");
        e.draft.wildcards.push(Wildcard::Overkill);
        let with_overkill = gun_ids(&e.secondary_slots());
        // Every primary item is offered in the secondary slot too.
        for id in gun_ids(&items::slots_of(true, &[])) {
            assert!(with_overkill.contains(&id), "Overkill offers {id}");
        }
        // The option list walks the same items.
        e.slot = Slot::Secondary;
        let options = e.options();
        assert!(options.contains(&weapons::weapon("an94_mp").display_name.to_string()));
    }

    #[test]
    fn each_weapon_slot_lists_bo2s_items_of_that_slot() {
        // The statstable's slot decides, not the weapon class: the SMAW is a `secondary` item, so
        // the primary never lists it (BO2's launchers are secondaries only).
        let primary = gun_ids(&items::slots_of(true, &[]));
        assert!(primary.contains(&"an94_mp") && !primary.contains(&"smaw_mp"), "{primary:?}");
        assert!(primary.iter().all(|id| weapons::weapon(id).cac_slot == items::PRIMARY));
        // A weapon with no slot is no class item: the built-in launcher alt mode and the knife.
        let all: Vec<&str> = cac_entries().iter().map(|d| d.id.as_str()).collect();
        assert!(!all.contains(&"gl_an94_mp") && !all.contains(&"knife_held_mp"), "{all:?}");
    }

    #[test]
    fn overkill_puts_a_second_primary_into_the_secondary_slot() {
        let mut e = Editor::new(Draft::default());
        e.draft.wildcards.push(Wildcard::Overkill);
        e.slot = Slot::Secondary;
        let ids = gun_ids(&e.secondary_slots());
        // Pick the first primary-class gun offered in the secondary list.
        let target = "an94_mp";
        let i = ids.iter().position(|id| *id == target).expect("AN-94 is offered");
        e.picking = true;
        e.option = i;
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft.secondary, target);
        // A round trip resolves it as the secondary and re-derives Overkill.
        let c = e.draft.to_custom();
        assert_eq!(c.secondary, target);
        let back = Draft::from_custom(&c);
        assert_eq!(back.secondary, target);
        assert!(back.wildcards.contains(&Wildcard::Overkill));
    }

// --- Pointer events, deny feedback ---

#[test]
fn pointer_hover_click_and_wheel_drive_the_editor() {
    let mut e = Editor::new(Draft::default());
    // Hovering a row moves the highlight without changing state.
    assert_eq!(e.step_pointer(PointerEvent { slot: Some(0), moved: true, ..Default::default() }), Outcome::Continue);
    assert_eq!(e.slot, Slot::Name);
    // A *resting* cursor is not a hover: the keyboard highlight stays put.
    e.slot = Slot::Primary;
    e.step_pointer(PointerEvent { slot: Some(3), ..Default::default() });
    assert_eq!(e.slot, Slot::Primary, "no movement, no hover");
    // Clicking the Primary row confirms it (opens its list on the held gun).
    assert_eq!(e.step_pointer(PointerEvent { slot: Some(1), click: true, ..Default::default() }), Outcome::Continue);
    assert_eq!(e.slot, Slot::Primary);
    assert!(e.picking);
    // A click with a resting cursor still picks the option under it (the common hover-then-
    // click sequence: the move happened on the previous frame).
    let held = e.option;
    e.step_pointer(PointerEvent { option: Some(held), click: true, ..Default::default() });
    assert!(!e.picking);
    assert_eq!(e.draft.primary, "an94_mp", "the held gun was confirmed");
    // The wheel walks the slot rows (Primary -> PrimaryAtts) even when the cursor rests on one.
    e.step_pointer(PointerEvent { slot: Some(5), wheel: 1, ..Default::default() });
    assert_eq!(e.slot, Slot::PrimaryAtts);
}

#[test]
fn a_click_outside_the_list_does_not_confirm_the_highlighted_row() {
    let mut e = Editor::new(Draft::default());
    e.picking = true;
    e.slot = Slot::Primary;
    // No hovered slot or option: a click on empty space is ignored.
    let before = e.draft.clone();
    e.step_pointer(PointerEvent { click: true, ..Default::default() });
    assert!(e.picking, "the list stays open");
    assert!(!e.options().is_empty(), "the list is still there");
    assert_eq!(e.draft, before, "nothing was picked");
}

#[test]
fn the_budget_denies_an_eleventh_point_and_leaves_the_draft() {
    let mut d = Draft::default();
    d.wildcards.push(Wildcard::Overkill);
    assert_eq!(d.spent(), PICK_10);
    let mut e = Editor::new(d);
    e.slot = Slot::Wildcards;
    e.picking = true;
    e.option = Wildcard::ALL.iter().position(|w| *w == Wildcard::Tactician).unwrap();
    let before = e.draft.clone();
    assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
    assert!(e.denied, "the refused choice is surfaced");
    assert_eq!(e.draft, before, "the draft is untouched");
    assert!(!e.picking);
}

#[test]
fn an_over_budget_save_is_denied_instead_of_closing() {
    let mut d = Draft::default();
    d.wildcards.push(Wildcard::Overkill);
    d.wildcards.push(Wildcard::Tactician);
    assert!(!d.valid());
    let mut e = Editor::new(d);
    e.slot = Slot::Save;
    assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
    assert!(e.denied);
}

#[test]
fn an_attachment_at_its_cap_is_refused() {
    let mut atts = vec!["reflex".to_string(), "gl".to_string()];
    assert!(!toggle_att(&mut atts, "stock", 2, |_, _| true), "no room for a third");
    assert_eq!(atts, vec!["reflex".to_string(), "gl".to_string()]);
    assert!(toggle_att(&mut atts, "reflex", 2, |_, _| true), "removing one frees a slot");
}

/// BO2's attachment grid: picking an attachment unequips the equipped ones it is mutually
/// exclusive with, so a forbidden pair is never built (`cacattachmentsmenu.lua`).
#[test]
fn picking_an_exclusive_attachment_replaces_the_one_it_conflicts_with() {
    // reflex–acog are exclusive (two optics); grip goes with both.
    let ok = |a: &str, b: &str| !matches!((a, b), ("reflex", "acog") | ("acog", "reflex"));
    let mut atts = vec!["reflex".to_string(), "grip".to_string()];
    assert!(toggle_att(&mut atts, "acog", 2, ok), "at the cap, but the conflict frees a slot");
    assert_eq!(atts, vec!["grip".to_string(), "acog".to_string()]);
    // At the cap with nothing exclusive, the pick is refused untouched.
    assert!(!toggle_att(&mut atts, "fmj", 2, ok));
    assert_eq!(atts, vec!["grip".to_string(), "acog".to_string()]);
}

/// The installed catalog's compatibility lists drive the editor (the built-in table: the AN-94's
/// reflex and grenade launcher list each other).
#[test]
fn the_catalog_decides_compatibility() {
    assert!(legal::compatible("an94_mp", "reflex", "gl"));
    assert!(legal::legal("an94_mp", &["gl", "reflex"]));
    assert!(!legal::compatible("an94_mp", "reflex", "reflex"));
    assert!(!legal::compatible("an94_mp", "reflex", "stock"), "an attachment the weapon lacks goes with nothing");
    assert!(legal::exclusive_with("an94_mp", "gl", &["reflex".to_string()]).is_empty());
    assert_eq!(legal::sanitize("an94_mp", &["reflex", "gl"]), vec!["gl", "reflex"]);
}

// --- The bonus-card cap (BO2's maxBonusCards) ---

#[test]
fn a_fourth_bonus_card_is_denied() {
    // A cheap class (five points) leaves budget for five cards, so only the three-card cap can
    // refuse the fourth: BO2's `maxBonusCards`.
    let mut e = Editor::new(Draft {
        primary: "an94_mp".into(),
        primary_atts: Vec::new(),
        secondary: "fnp45_mp".into(),
        secondary_atts: Vec::new(),
        perks: [Some(Perk::LIGHTWEIGHT), None, None],
        ..Draft::default()
    });
    let cards = crate::classes::MAX_BONUS_CARDS as i32;
    assert!(e.draft.spent() + cards < PICK_10, "room for every card: {}", e.draft.spent());
    e.slot = Slot::Wildcards;
    for (n, w) in [Wildcard::Perk1Greed, Wildcard::Tactician, Wildcard::DangerClose].into_iter().enumerate() {
        let i = Wildcard::ALL.iter().position(|x| *x == w).expect("in the list");
        e.picking = true;
        e.option = i;
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.denied, "card {} is within the cap", n + 1);
        assert_eq!(e.draft.wildcards.len(), n + 1);
    }
    assert_eq!(e.draft.wildcards.len(), crate::classes::MAX_BONUS_CARDS);
    // The fourth: refused with the deny flag, the draft untouched.
    let before = e.draft.clone();
    let overkill = Wildcard::ALL.iter().position(|x| *x == Wildcard::Overkill).expect("in the list");
    let tactician = Wildcard::ALL.iter().position(|x| *x == Wildcard::Tactician).expect("in the list");
    e.picking = true;
    e.option = overkill;
    e.step(Edges { confirm: true, ..Default::default() });
    assert!(e.denied, "the fourth card is surfaced as a denial");
    assert_eq!(e.draft, before, "the draft is untouched");
    assert!(!e.picking);
    // A card already in play can still be turned off, and then another can go on.
    e.picking = true;
    e.option = tactician;
    e.step(Edges { confirm: true, ..Default::default() });
    assert_eq!(e.draft.wildcards.len(), crate::classes::MAX_BONUS_CARDS - 1);
    e.picking = true;
    e.option = overkill;
    e.step(Edges { confirm: true, ..Default::default() });
    assert_eq!(e.draft.wildcards.len(), crate::classes::MAX_BONUS_CARDS);
    assert!(e.draft.valid());
}

#[test]
fn an_over_capped_draft_is_not_valid() {
    // A hand-built draft (or one derived from a stored class) past the cap never saves.
    let d = Draft { wildcards: vec![Wildcard::Perk1Greed, Wildcard::Perk2Greed, Wildcard::Perk3Greed, Wildcard::Overkill], ..Draft::default() };
    assert_eq!(d.wildcards.len(), crate::classes::MAX_BONUS_CARDS + 1);
    assert!(!d.valid(), "over the card cap");
}

#[test]
fn a_stored_over_budget_class_opens_repaired() {
    // `from_stored` runs BO2's checkclasses.lua repair first, so the editor never opens on a
    // draft the game could not apply.
    let c = CustomClass {
        name: "Fat".into(),
        primary: "an94_mp+gl+reflex".into(),
        secondary: "dsr50_mp".into(),
        perks: vec!["Lightweight".into(), "Hardline".into(), "Toughness".into(), "Scavenger".into(), "Dexterity".into()],
        ..CustomClass::default()
    };
    let raw = Draft::from_custom(&c);
    assert!(raw.spent() > PICK_10, "the stored class is over budget: {}", raw.spent());
    let repaired = Draft::from_stored(&c);
    assert!(repaired.spent() <= PICK_10, "the editor opens within the allocation: {}", repaired.spent());
    assert!(repaired.valid());
    assert_eq!(repaired.name, "Fat", "the class's identity survives the repair");
}

#[test]
fn the_content_grid_links_match_bo2s_contentgrid() {
    // `contentgrid.lua:91-122` with 7 cells, 3 wide (the perk/wildcard shape):
    //   row 0: 0 1 2   row 1: 3 4 5   row 2 (short): 6
    // Left/Right wrap inside the row (never page); a one-cell row keeps no link (a no-op move).
    assert_eq!(grid_right(0, 7, 3), 1);
    assert_eq!(grid_right(1, 7, 3), 2);
    assert_eq!(grid_right(2, 7, 3), 0, "the row's last cell links to its first");
    assert_eq!(grid_left(0, 7, 3), 2, "and the first back to the last");
    assert_eq!(grid_left(1, 7, 3), 0);
    assert_eq!(grid_left(6, 7, 3), 6, "a one-cell row has no left link");
    assert_eq!(grid_right(6, 7, 3), 6, "nor a right one");
    // Up/Down walk the cells that hold the column and wrap between the first and last of them:
    // cells 1, 4, 7 are column 1 but the short last row has none, so 1 <-> 4 wrap.
    assert_eq!(grid_down(1, 7, 3), 4);
    assert_eq!(grid_down(4, 7, 3), 1, "the last row holding the column links back");
    assert_eq!(grid_up(4, 7, 3), 1);
    assert_eq!(grid_up(1, 7, 3), 4);
    // Column 0 has all three rows: 0 -> 3 -> 6 -> 0, and back.
    assert_eq!(grid_down(0, 7, 3), 3);
    assert_eq!(grid_down(3, 7, 3), 6);
    assert_eq!(grid_down(6, 7, 3), 0);
    assert_eq!(grid_up(0, 7, 3), 6);
    assert_eq!(grid_up(6, 7, 3), 3);
    // A full grid is the plain rectangle; a one-row grid never leaves it (BO2's self-links no-op).
    for i in 0..6 {
        assert_eq!(grid_down(i, 6, 3), (i + 3) % 6);
        assert_eq!(grid_up(i, 6, 3), (i + 3) % 6);
    }
    assert_eq!(grid_right(5, 6, 3), 3);
    assert_eq!(grid_left(3, 6, 3), 5);
    for i in 0..3 {
        assert_eq!(grid_down(i, 3, 3), i);
        assert_eq!(grid_up(i, 3, 3), i);
    }
    // The attachment grid is six wide (`cacattachmentsmenu.lua:9`).
    assert_eq!(grid_right(5, 13, 6), 0);
    assert_eq!(grid_left(0, 13, 6), 5);
    assert_eq!(grid_down(5, 13, 6), 11, "the last row holding column 5 is row 1");
    assert_eq!(grid_down(11, 13, 6), 5);
    assert_eq!(grid_down(6, 13, 6), 12);
    assert_eq!(grid_down(12, 13, 6), 0);
    assert_eq!(grid_up(0, 13, 6), 12, "the short last row's own cell links back");
}

#[test]
fn a_grid_picker_walks_two_dimensions_and_a_wide_slot_says_so() {
    // The perk picker's list is three wide (`cacperksp.lua:7`); `Editor::step` folds the arrows.
    assert_eq!(Slot::Perk2.option_columns(), Some(3));
    assert_eq!(Slot::PrimaryAtts.option_columns(), Some(6), "`cacattachmentsmenu.lua:9`");
    assert_eq!(Slot::Wildcards.option_columns(), Some(3), "`cacbonuscards.lua:7`");
    assert_eq!(Slot::Primary.option_columns(), None, "the carousel walks categories, not a grid");
    assert_eq!(Slot::Name.option_columns(), None);

    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Perk2;
    e.picking = true;
    e.option = 0;
    e.step(Edges { right: true, ..Default::default() });
    assert_eq!(e.option, 1, "Right stays in the row");
    e.step(Edges { left: true, ..Default::default() });
    assert_eq!(e.option, 0);
    e.step(Edges { left: true, ..Default::default() });
    let row_end = e.options().len().min(3) - 1;
    assert_eq!(e.option, row_end, "Left off the row's edge wraps to its last cell");
    e.step(Edges { right: true, ..Default::default() });
    assert_eq!(e.option, 0, "Right off the row's edge wraps to its first");
    // Down moves a row (three cells), not to the next flat item.
    let n = e.options().len();
    e.step(Edges { down: true, ..Default::default() });
    assert_eq!(e.option, if n > 3 { 3 } else { 0 });
    e.step(Edges { up: true, ..Default::default() });
    assert_eq!(e.option, 0);
}

#[test]
fn the_offer_is_the_editors_own_lists_and_caps() {
    let o = offer();
    assert_eq!(o.primary, gun_ids(&[items::PRIMARY]));
    assert_eq!(o.secondary, gun_ids(&[items::SECONDARY]));
    assert_eq!((o.lethal.clone(), o.tactical.clone()), (equipment_ids(WeaponClass::Lethal), equipment_ids(WeaponClass::Tactical)));
    // BO2's two attachments, three with Primary Gunfighter; one, two with Secondary Gunfighter.
    assert_eq!((o.primary_caps, o.secondary_caps), ((2, 3), (1, 2)));
    let first = o.primary[0];
    assert_eq!(offered_attachments(first), allowed_shorts(first));
    assert_eq!(draft_weapon_id("an94_mp", &["reflex", "grip"]), "an94_mp+grip+reflex");
}
