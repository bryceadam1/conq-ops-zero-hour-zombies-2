use super::*;
use super::apply::toggle_att;
use super::items::cac_entries;

    #[test]
    fn the_default_draft_spends_the_base_kit_points() {
        let d = Draft::test_kit();
        // 2 weapons + 2 primary attachments + lethal + tactical + 3 perks = 9; one point free.
        assert_eq!(d.spent(), 9);
        assert_eq!(d.remaining(), 1);
        assert!(d.valid());
        assert_eq!(d.primary_id(), "an94_mp+gl+reflex");
        assert_eq!(d.secondary_id(), "fnp45_mp");
    }

    #[test]
    fn tab_marks_the_personalize_action_without_touching_the_draft() {
        let mut e = Editor::new(Draft::test_kit());
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
        let mut e = Editor::new(Draft::test_kit());
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

    /// `customclass.lua`: `PrimaryWeapon_Button_Over` / `SecondaryWeapon_Button_Over` add the `C`
    /// prompt only when the slot holds a weapon, each with its own `weaponSlot`.
    #[test]
    fn c_on_each_weapon_box_opens_that_weapons_camo_page() {
        let mut e = Editor::new(Draft::test_kit());
        e.slot = Slot::Secondary;
        assert_eq!(e.camo_page(), Some(PersonalizeKind::SecondaryCamo));
        assert_eq!(e.step(Edges { camo: true, ..Default::default() }), Outcome::Continue);
        assert_eq!(e.personalize, Some(PersonalizeKind::SecondaryCamo));
        assert_eq!(e.slot, Slot::Secondary, "the focus stays");
        // An empty slot has no prompt and `C` does nothing.
        e.draft.secondary.clear();
        assert_eq!(e.camo_page(), None);
        e.step(Edges { camo: true, ..Default::default() });
        assert_eq!(e.personalize, None);
        e.slot = Slot::Primary;
        e.draft.primary.clear();
        assert_eq!(e.camo_page(), None);
        // Nor anywhere but a weapon box.
        let mut e = Editor::new(Draft::test_kit());
        e.slot = Slot::Lethal;
        assert_eq!(e.camo_page(), None);
        assert_eq!(PersonalizeKind::SecondaryCamo.columns(), 6, "the same `CACCamo` grid");
    }

    /// BO2's `SetClassItem` on a weapon slot with a new value zeroes that slot's camo and reticle
    /// (`FUN_008d0650`); re-picking the same weapon changes nothing, and Remove clears them too.
    #[test]
    fn a_new_weapon_in_a_slot_resets_that_slots_camo() {
        let personalised = Draft {
            camo: "camo_gold".into(),
            reticle: "reticle_2".into(),
            reticle_color: "red".into(),
            secondary_camo: "camo_atacs".into(),
            knife: "camo_diamond".into(),
            ..Draft::test_kit()
        };
        // A different secondary: its camo goes, the primary's options and the knife's stay.
        let mut e = Editor::new(personalised.clone());
        e.slot = Slot::Secondary;
        let ids = e.gun_list(false);
        let other = ids.iter().position(|id| *id != e.draft.secondary).expect("another sidearm");
        let same = ids.iter().position(|id| *id == e.draft.secondary).expect("the current sidearm");
        e.apply_option(same);
        assert_eq!(e.draft.secondary_camo, "camo_atacs", "re-picking the same weapon keeps its camo");
        e.apply_option(other);
        assert!(e.draft.secondary_camo.is_empty());
        assert_eq!((e.draft.camo.as_str(), e.draft.reticle.as_str(), e.draft.knife.as_str()), ("camo_gold", "reticle_2", "camo_diamond"));
        // A different primary: its camo, reticle and reticle colour go; the secondary's stays.
        let mut e = Editor::new(personalised.clone());
        e.slot = Slot::Primary;
        let ids = e.gun_list(true);
        let other = ids.iter().position(|id| *id != e.draft.primary).expect("another primary");
        e.apply_option(other);
        assert!(e.draft.camo.is_empty() && e.draft.reticle.is_empty() && e.draft.reticle_color.is_empty());
        assert_eq!(e.draft.secondary_camo, "camo_atacs");
        // Remove takes the slot's camo with the gun.
        let mut d = personalised.clone();
        assert!(crate::classui::clear_cell(&mut d, Slot::Secondary, 0));
        assert!(d.secondary_camo.is_empty());
        assert_eq!(d.camo, "camo_gold");
    }

    #[test]
    fn the_secondary_camo_page_sets_secondarycamo_only() {
        let mut d = Draft { camo: "camo_gold".into(), secondary_camo: "camo_atacs".into(), ..Draft::test_kit() };
        let choices: Vec<String> = ["camo_none", "camo_atacs", "camo_gold"].iter().map(|s| (*s).to_string()).collect();
        let mut p = Personalize::new(PersonalizeKind::SecondaryCamo, choices, &d);
        assert_eq!((p.equipped.as_str(), p.highlight), ("camo_atacs", 1), "opens on the class's secondarycamo");
        p.highlight = 2;
        assert_eq!(p.step(Edges { confirm: true, ..Default::default() }, &mut d), Outcome::Save);
        assert_eq!((d.camo.as_str(), d.secondary_camo.as_str(), d.knife.as_str()), ("camo_gold", "camo_gold", ""));
        // The draft carries it into the class and back.
        let c = d.to_custom();
        assert_eq!(c.secondary_camo, "camo_gold");
        assert_eq!(Draft::from_custom(&c).secondary_camo, "camo_gold");
        assert!(d.to_spec().contains("secondary_camo=camo_gold"), "{}", d.to_spec());
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
        let mut d = Draft { camo: "gold".into(), ..Draft::test_kit() };
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
        let mut d = Draft { reticle: "red_dot".into(), ..Draft::test_kit() };
        let choices: Vec<String> = ["none", "red_dot", "acog"].iter().map(|s| s.to_string()).collect();
        let mut p = Personalize::new(PersonalizeKind::Reticle, choices, &d);
        assert_eq!(p.highlight, 1);
        assert_eq!(p.step(Edges { cancel: true, ..Default::default() }, &mut d), Outcome::Cancel);
        assert_eq!(d.reticle, "red_dot");
    }

    #[test]
    fn personalize_with_no_choices_closes_without_applying() {
        let mut d = Draft::test_kit();
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
        let mut d = Draft::test_kit();
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
    fn every_wildcard_is_its_tables_card_and_round_trips_through_its_reference() {
        for w in Wildcard::ALL {
            assert_eq!(Wildcard::by_reference(w.reference()), Some(w));
            let card = w.card().expect("the table holds every card the editor has a rule for");
            assert_eq!(w.name(), card.name);
            assert_eq!(w.description(), card.description);
            assert!(!w.description().is_empty() && card.slot.starts_with("bonuscard"), "{card:?}");
            assert_eq!(w.cost(), card.cost);
        }
        // BO2's rows (`mp/statstable.csv:178-185`): Tactician shares Primary Gunfighter's slot.
        assert_eq!(Wildcard::Tactician.card().map(|c| c.slot.as_str()), Some("bonuscard1"));
        assert_eq!(Wildcard::PrimaryGunfighter.card().map(|c| c.slot.as_str()), Some("bonuscard1"));
        assert_eq!(Wildcard::DangerClose.card().map(|c| c.slot.as_str()), Some("bonuscard4"));
        assert_eq!(Wildcard::Perk1Greed.greed_tier(), Some(1));
        assert_eq!(Wildcard::Overkill.greed_tier(), None);
    }

    /// BO2's `SetClassItem` on a weapon slot with a new value zeroes `<slot>attachment1..3`
    /// (`FUN_008d0650`): a different gun starts bare, even when it offers the same attachments;
    /// re-picking the same gun keeps them.
    #[test]
    fn a_new_weapon_in_a_slot_starts_without_attachments() {
        let mut e = Editor::new(Draft::test_kit());
        e.slot = Slot::Primary;
        let ids = e.gun_list(true);
        let same = ids.iter().position(|id| *id == e.draft.primary).expect("the AN-94 is listed");
        e.apply_option(same);
        assert_eq!(e.draft.primary_atts, ["gl", "reflex"], "re-picking the same gun keeps its attachments");
        // Another assault rifle offers the reflex too; BO2 still clears it with the weapon change.
        let other = ids
            .iter()
            .position(|id| *id != e.draft.primary && allowed_shorts(id).contains(&"reflex"))
            .expect("another gun offering the reflex");
        e.apply_option(other);
        assert!(e.draft.primary_atts.is_empty(), "{:?}", e.draft.primary_atts);
        // The secondary slot follows the same rule.
        let mut e = Editor::new(Draft { secondary_atts: vec!["reflex".into()], ..Draft::test_kit() });
        e.slot = Slot::Secondary;
        let ids = e.gun_list(false);
        let other = ids.iter().position(|id| *id != e.draft.secondary).expect("another sidearm");
        e.apply_option(other);
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
        let mut names: Vec<&str> = ids.iter().map(|id| weapons::weapon(id).menu_name()).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n, "no weapon is listed twice by display name");
        // The same filter backs the secondary and equipment lists.
        for lethal in [true, false] {
            let list = equipment_ids(lethal);
            assert!(list.iter().all(|id| weapons::weapon(id).alt.is_empty()), "{lethal}: {list:?}");
        }
    }

    #[test]
    fn attachments_cap_at_two_until_the_gunfighter_wildcard() {
        let mut d = Draft { primary: "an94_mp".into(), primary_atts: vec![], ..Draft::test_kit() };
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
        let mut d = Draft { primary: "an94_mp".into(), primary_atts: vec!["gl".into(), "reflex".into(), "stock".into()], wildcards: vec![Wildcard::PrimaryGunfighter], ..Draft::test_kit() };
        assert_eq!(d.spent(), 11);
        assert!(!d.valid());
        d.wildcards.remove(0);
        d.primary_atts.truncate(2);
        assert_eq!(d.spent(), 9);
        assert!(d.valid());
    }

    #[test]
    fn the_budget_refuses_an_over_spend() {
        let mut d = Draft::test_kit();
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
        let mut d = Draft { name: "My Rifle".into(), primary: "an94_mp".into(), primary_atts: vec!["reflex".into()], ..Draft::test_kit() };
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
        let d = Draft { primary: "an94_mp".into(), primary_atts: vec![], perks: [Some(Perk::FLAK_JACKET), None, None], ..Draft::test_kit() };
        let l = crate::weapons::Loadout::test_kit().with_spec(&d.to_spec());
        assert_eq!(l.primary, "an94_mp");
        // The spec carries exactly the set perks; unset tiers fall back to the base kit's slots.
        assert_eq!(l.perks, vec![Perk::FLAK_JACKET]);
    }

    /// Walk `e` along `dirs` and return where it lands.
    fn walk(e: &mut Editor, dirs: &[Dir]) -> Focus {
        for d in dirs {
            e.navigate(*d);
        }
        e.focus()
    }

    #[test]
    fn the_editor_grid_follows_bo2s_links() {
        // `customclass.lua:863-991`, through the pure editor's key fold, from every box. The grid is
        // BO2's alone: no name or save row is spliced in, so the column cycles close on the cards.
        use Dir::*;
        let s = Focus::slot;
        let cases: &[(Focus, Dir, Focus)] = &[
            (s(Slot::Primary), Up, Focus::card(0)),
            (s(Slot::Primary), Down, s(Slot::PrimaryAtts)),
            (s(Slot::Primary), Right, s(Slot::Perk1)),
            (s(Slot::PrimaryAtts), Down, s(Slot::Secondary)),
            (s(Slot::PrimaryAtts), Left, s(Slot::Perk3)),
            (s(Slot::PrimaryAtts), Right, s(Slot::Perk3)),
            (s(Slot::Secondary), Down, s(Slot::SecondaryAtts)),
            (s(Slot::Secondary), Right, s(Slot::Lethal)),
            (s(Slot::SecondaryAtts), Down, Focus::card(0)),
            (s(Slot::SecondaryAtts), Right, s(Slot::Tactical)),
            (s(Slot::Perk1), Up, Focus::card(2)),
            (s(Slot::Perk1), Down, s(Slot::Perk2)),
            (s(Slot::Perk1), Left, s(Slot::Primary)),
            (s(Slot::Perk2), Right, s(Slot::Primary)),
            (s(Slot::Perk3), Down, s(Slot::Lethal)),
            (s(Slot::Perk3), Left, s(Slot::PrimaryAtts)),
            (s(Slot::Lethal), Left, s(Slot::Secondary)),
            (s(Slot::Tactical), Down, Focus::card(2)),
            (s(Slot::Tactical), Right, s(Slot::SecondaryAtts)),
            (Focus::card(0), Up, s(Slot::SecondaryAtts)),
            (Focus::card(0), Down, s(Slot::Primary)),
            (Focus::card(0), Right, Focus::card(1)),
            (Focus::card(1), Left, Focus::card(0)),
            (Focus::card(1), Up, s(Slot::SecondaryAtts)),
            (Focus::card(2), Up, s(Slot::Tactical)),
            (Focus::card(2), Down, s(Slot::Perk1)),
        ];
        for (from, dir, to) in cases {
            let mut e = Editor::new(Draft::test_kit());
            e.focus_on(*from);
            assert_eq!(e.focus(), *from);
            assert_eq!(walk(&mut e, &[*dir]), *to, "{from:?} {dir:?}");
        }
        // No link, no move: the card row's ends and the left column's Left.
        for (from, dir) in [(Focus::card(0), Left), (Focus::card(2), Right), (s(Slot::Primary), Left)] {
            let mut e = Editor::new(Draft::test_kit());
            e.focus_on(from);
            assert!(!e.navigate(dir), "{from:?} {dir:?} has no link");
            assert_eq!(e.focus(), from);
        }
    }

    #[test]
    fn the_column_jumps_return_to_the_box_they_came_from() {
        use Dir::*;
        // `Perk12Button_GainFocus`: PRIMARY's Right goes back to the perk last focused.
        let mut e = Editor::new(Draft::test_kit());
        assert_eq!(walk(&mut e, &[Right, Down, Left]), Focus::slot(Slot::Primary), "perk 2's Left is PRIMARY");
        assert_eq!(walk(&mut e, &[Right]), Focus::slot(Slot::Perk2), "and PRIMARY's Right now returns to perk 2");
        // `BonusCardButton_GainFocus`: the column above and below a card return to it.
        let mut e = Editor::new(Draft::test_kit());
        e.focus_on(Focus::card(1));
        assert_eq!(walk(&mut e, &[Up, Down]), Focus::card(1), "SECONDARY ATTS' Down returns to card 2");
        assert_eq!(walk(&mut e, &[Down, Up]), Focus::card(1), "the name row's Up returns to card 2");
        // Card 3 belongs to the right column only: TACTICAL never leads to card 2.
        e.focus_on(Focus::slot(Slot::Tactical));
        assert_eq!(walk(&mut e, &[Down]), Focus::card(2));
        // A hover runs the same hook (hover is `gain_focus`).
        let mut e = Editor::new(Draft::test_kit());
        e.step_pointer(PointerEvent { slot: e.slots().iter().position(|s| *s == Slot::Perk2), moved: true, ..Default::default() });
        assert_eq!(e.focus(), Focus::slot(Slot::Perk2));
        e.focus_on(Focus::slot(Slot::Primary));
        assert_eq!(walk(&mut e, &[Right]), Focus::slot(Slot::Perk2));
    }

    #[test]
    fn a_greed_plate_joins_its_tiers_column_and_links() {
        use Dir::*;
        let mut e = Editor::new(Draft { wildcards: vec![Wildcard::Perk1Greed], ..Draft::test_kit() });
        e.focus_on(Focus::slot(Slot::Perk1));
        assert_eq!(walk(&mut e, &[Down]), Focus::slot(Slot::Perk1b));
        assert_eq!(walk(&mut e, &[Down]), Focus::slot(Slot::Perk2));
        assert_eq!(walk(&mut e, &[Up, Up]), Focus::slot(Slot::Perk1));
        e.focus_on(Focus::slot(Slot::Perk1b));
        assert_eq!(walk(&mut e, &[Left]), Focus::slot(Slot::Primary), "the plate shares perk 1's Left");
        assert_eq!(walk(&mut e, &[Right]), Focus::slot(Slot::Perk1b), "and its back-link");
    }

    #[test]
    fn the_wheel_walks_the_grid_links() {
        // LUI's wheel is Up/Down (t6mp `FUN_009376a0`): PRIMARY -> PRIMARY ATTS -> SECONDARY.
        let mut e = Editor::new(Draft::test_kit());
        e.step_pointer(PointerEvent { wheel: 2, ..Default::default() });
        assert_eq!(e.slot, Slot::Secondary);
        e.step_pointer(PointerEvent { wheel: -1, ..Default::default() });
        assert_eq!(e.slot, Slot::PrimaryAtts);
    }

    #[test]
    fn backing_out_of_the_grid_saves_the_class() {
        // BO2 writes each pick into the class as it is made and `CACEditClass.Back` only closes
        // (`caceditclass.lua:472-548`, `:642-653`): Escape on the grid keeps the edits.
        let mut e = Editor::new(Draft::test_kit());
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Save);
        assert!(!e.denied);
        // Escape inside an open list closes the list only (one step back), never the editor.
        let mut e = Editor::new(Draft::test_kit());
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(e.picking);
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.picking);
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Save);
    }

    #[test]
    fn an_over_budget_draft_backs_out_unsaved_with_the_deny() {
        // Every pick that would break the budget is refused, so only a class loaded that way can
        // be over it. It cannot be kept, and the player is not trapped: Escape leaves unsaved.
        let mut d = Draft::test_kit();
        d.wildcards.push(Wildcard::Overkill);
        d.wildcards.push(Wildcard::Tactician);
        let mut e = Editor::new(d);
        assert!(!e.draft.valid());
        assert_eq!(e.step(Edges { cancel: true, ..Default::default() }), Outcome::Cancel);
        assert!(e.denied, "the refused save plays BO2's deny");
    }

    #[test]
    fn the_editor_opens_the_list_on_the_held_option() {
        // The secondary slot opens on the Tac-45, which the built-in table holds.
        let mut e = Editor::new(Draft { secondary: "fnp45_mp".into(), ..Draft::test_kit() });
        e.slot = Slot::Secondary;
        let idx = e.current_option_index();
        assert_eq!(e.gun_list(false)[idx], "fnp45_mp");
        e.picking = true;
        e.option = idx;
        // Confirming the held weapon leaves the draft unchanged.
        let before = e.draft.clone();
        e.step(Edges { confirm: true, ..Default::default() });
        assert_eq!(e.draft, before);
    }

    #[test]
    fn perk_picks_land_in_their_tier() {
        let mut e = Editor::new(Draft::test_kit());
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
        let mut e = Editor::new(Draft::test_kit());
        e.slot = Slot::Primary;
        let held = e.current_option_index();
        assert!(e.option_chosen(held));
        assert!(!e.option_chosen(held + 1));
        // Attachments: the AN-94's gl and reflex are on the draft, so their list entries are chosen.
        e.slot = Slot::PrimaryAtts;
        let shorts = attachment_shorts(&e.draft.primary);
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
        let d = Draft::test_kit();
        assert_eq!(slot_value(&d, Slot::Primary), weapons::weapon("an94_mp").menu_name());
        assert_eq!(slot_value(&d, Slot::Secondary), weapons::weapon("fnp45_mp").menu_name());
        // The attachment row lists the display names, not the shorts.
        let atts = slot_value(&d, Slot::PrimaryAtts);
        assert!(atts.contains("Grenade Launcher") && atts.contains("Reflex"), "keeps names: {atts}");
        assert_ne!(atts, "—");
        // The perk rows read the tier's held perk.
        assert_eq!(slot_value(&d, Slot::Perk1), "Lightweight");
        assert_eq!(slot_value(&d, Slot::Wildcards), "—");
        // A perk whose id is not its name reads as the name.
        let mut d = d;
        d.perks[1] = Some(crate::weapons::Perk::FAST_HANDS);
        assert_eq!(slot_value(&d, Slot::Perk2), "Fast Hands");
    }

    // --- Renaming is the class-options dialog's, not the editor's ---

    #[test]
    fn the_editor_has_no_name_field() {
        // BO2's editor only shows the class name (`caceditclass.lua:177`); the rename is the class
        // options' RENAME (`cacclassoptions.lua:183-192`). Letters are not text entry here: they
        // are the grid's own keys or nothing, and the name never changes.
        let mut e = Editor::new(Draft { name: "RIFLE".into(), ..Draft::test_kit() });
        for k in [Key::M, Key::P, Key::Key1, Key::Space, Key::Backspace] {
            let mut now = crate::input::KeySet::default();
            now.set_down(k, true);
            assert_eq!(e.step(Edges::between(&crate::input::KeySet::default(), &now)), Outcome::Continue);
        }
        assert_eq!(e.draft.name, "RIFLE");
        assert!(!Slot::ALL.iter().any(|s| s.label() == "CLASS NAME"), "no name row");
    }

    #[test]
    fn the_name_round_trips_through_a_saved_class() {
        let mut d = Draft { name: "My Rifle".into(), ..Draft::test_kit() };
        d.primary = "an94_mp".into();
        let c = d.to_custom();
        assert_eq!(c.name, "My Rifle");
        let back = Draft::from_custom(&c);
        assert_eq!(back.name, "My Rifle");
    }

    // --- Option 3: Greed second perks and Overkill's second primary ---

    #[test]
    fn a_greed_wildcard_splices_in_a_second_perk_row() {
        let mut e = Editor::new(Draft::test_kit());
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
        let d = Draft { primary_atts: vec!["reflex".into()], ..Draft::test_kit() };
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
        let mut e = Editor::new(Draft::test_kit());
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
        let mut d = Draft::test_kit();
        let base = d.spent();
        d.extra_perks[0] = Some(Perk::SCAVENGER);
        assert_eq!(d.spent(), base + 1, "the Greed perk costs a point");
        d.wildcards.push(Wildcard::Perk1Greed);
        assert_eq!(d.spent(), base + 2, "the wildcard costs a point too");
    }

    #[test]
    fn overkill_offers_primary_guns_in_the_secondary_list() {
        let mut e = Editor::new(Draft::test_kit());
        // The secondary lists BO2's `secondary` items: the pistols and the launchers.
        let base = e.gun_list(false);
        assert!(base.contains(&"fnp45_mp") && base.contains(&"smaw_mp"), "{base:?}");
        assert!(!base.contains(&"an94_mp"), "a primary item needs Overkill");
        e.draft.wildcards.push(Wildcard::Overkill);
        let with_overkill = e.gun_list(false);
        // Under Overkill both slots list the primary items only, less the gun the other slot holds
        // (`cacweaponsp.lua:638-720`, `AddWeaponGroupCarousel`): no pistols, no second AN-94.
        for id in gun_ids(&items::slots_of(true, &[])) {
            assert_eq!(with_overkill.contains(&id), id != e.draft.primary, "{id}");
        }
        assert!(!with_overkill.contains(&"fnp45_mp") && !with_overkill.contains(&"smaw_mp"), "{with_overkill:?}");
        // The primary list leaves out a primary item the secondary holds.
        e.draft.secondary = with_overkill[0].to_string();
        assert!(!e.gun_list(true).contains(&with_overkill[0]));
        // The option list walks the same items.
        e.slot = Slot::Secondary;
        assert_eq!(e.options().len(), e.gun_list(false).len());
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
        let mut e = Editor::new(Draft::test_kit());
        e.draft.wildcards.push(Wildcard::Overkill);
        e.slot = Slot::Secondary;
        let ids = e.gun_list(false);
        // Pick the first primary-class gun offered in the secondary list (not the AN-94 the
        // primary holds).
        let target = ids[0];
        assert_ne!(target, e.draft.primary);
        let i = 0;
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
    let mut e = Editor::new(Draft::test_kit());
    // Hovering a row moves the highlight without changing state.
    assert_eq!(e.step_pointer(PointerEvent { slot: Some(2), moved: true, ..Default::default() }), Outcome::Continue);
    assert_eq!(e.slot, Slot::Secondary);
    // A *resting* cursor is not a hover: the keyboard highlight stays put.
    e.slot = Slot::Primary;
    e.step_pointer(PointerEvent { slot: Some(3), ..Default::default() });
    assert_eq!(e.slot, Slot::Primary, "no movement, no hover");
    // Clicking the Primary row confirms it (opens its list on the held gun).
    assert_eq!(e.step_pointer(PointerEvent { slot: Some(0), click: true, ..Default::default() }), Outcome::Continue);
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
    let mut e = Editor::new(Draft::test_kit());
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
    let mut d = Draft::test_kit();
    d.wildcards.push(Wildcard::Overkill);
    assert_eq!(d.spent(), PICK_10);
    let mut e = Editor::new(d);
    e.slot = Slot::Wildcards;
    e.picking = true;
    e.option = wildcard_list().iter().position(|w| *w == Wildcard::Tactician).unwrap();
    let before = e.draft.clone();
    assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
    assert!(e.denied, "the refused choice is surfaced");
    assert_eq!(e.draft, before, "the draft is untouched");
    assert!(!e.picking);
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
        ..Draft::test_kit()
    });
    let cards = crate::classes::max_bonus_cards() as i32;
    assert!(e.draft.spent() + cards < PICK_10, "room for every card: {}", e.draft.spent());
    e.slot = Slot::Wildcards;
    for (n, w) in [Wildcard::Perk1Greed, Wildcard::Perk2Greed, Wildcard::Tactician].into_iter().enumerate() {
        let i = wildcard_list().iter().position(|x| *x == w).expect("in the list");
        e.picking = true;
        e.option = i;
        assert_eq!(e.step(Edges { confirm: true, ..Default::default() }), Outcome::Continue);
        assert!(!e.denied, "card {} is within the cap", n + 1);
        assert_eq!(e.draft.wildcards.len(), n + 1);
    }
    assert_eq!(e.draft.wildcards.len(), crate::classes::max_bonus_cards());
    // The fourth: refused with the deny flag, the draft untouched.
    let before = e.draft.clone();
    let overkill = wildcard_list().iter().position(|x| *x == Wildcard::Overkill).expect("in the list");
    let tactician = wildcard_list().iter().position(|x| *x == Wildcard::Tactician).expect("in the list");
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
    assert_eq!(e.draft.wildcards.len(), crate::classes::max_bonus_cards() - 1);
    e.picking = true;
    e.option = overkill;
    e.step(Edges { confirm: true, ..Default::default() });
    assert_eq!(e.draft.wildcards.len(), crate::classes::max_bonus_cards());
    assert!(e.draft.valid());
}

#[test]
fn an_over_capped_draft_is_not_valid() {
    // A hand-built draft (or one derived from a stored class) past the cap never saves.
    let d = Draft { wildcards: vec![Wildcard::Perk1Greed, Wildcard::Perk2Greed, Wildcard::Perk3Greed, Wildcard::Overkill], ..Draft::test_kit() };
    assert_eq!(d.wildcards.len(), crate::classes::max_bonus_cards() + 1);
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

    let mut e = Editor::new(Draft::test_kit());
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
    assert_eq!((o.lethal.clone(), o.tactical.clone()), (equipment_ids(true), equipment_ids(false)));
    // BO2's two attachments, three with Primary Gunfighter; one, two with Secondary Gunfighter.
    assert_eq!((o.primary_caps, o.secondary_caps), ((2, 3), (1, 2)));
    let first = o.primary[0];
    assert_eq!(offered_attachments(first), allowed_shorts(first));
    assert_eq!(draft_weapon_id("an94_mp", &["reflex", "grip"]), "an94_mp+grip+reflex");
}

/// BO2's `GetLoadoutWeapon` (G16): attachments join with `+`, but Dual Wield renames the weapon
/// to the underscore form — and the inverse reads a stored id back into the item.
#[test]
fn dual_wield_gives_the_dw_weapon_and_reads_back() {
    assert_eq!(items::given_weapon("an94_mp", &["reflex", "gl"]), "an94_mp+gl+reflex");
    assert_eq!(items::given_weapon("fiveseven_mp", &["dw"]), "fiveseven_dw_mp");
    assert_eq!(items::given_weapon("fiveseven_mp", &[]), "fiveseven_mp");
    assert_eq!(items::given_parts("fiveseven_dw_mp"), ("fiveseven_mp".to_string(), vec!["dw".to_string()]));
    assert_eq!(items::given_parts("an94_mp+gl+reflex"), ("an94_mp".to_string(), vec!["gl".to_string(), "reflex".to_string()]));
    // Only a weapon whose item offers `dw` reads as one: the Tac-45 stand-in offers none.
    assert_eq!(items::given_parts("fnp45_dw_mp"), ("fnp45_dw_mp".to_string(), Vec::new()));
}

/// The editor picks Dual Wield like any attachment; the class it saves holds BO2's weapon, and
/// re-opening it shows the pistol with Dual Wield equipped.
#[test]
fn picking_dual_wield_saves_the_dw_weapon() {
    let mut e = Editor::new(Draft { secondary: "fiveseven_mp".into(), ..Draft::test_kit() });
    e.slot = Slot::SecondaryAtts;
    let i = attachment_shorts(&e.draft.secondary).iter().position(|s| *s == "dw").expect("Dual Wield is offered");
    assert!(e.apply_option(i));
    assert_eq!(e.draft.secondary_id(), "fiveseven_dw_mp");
    let c = e.draft.to_custom();
    assert_eq!(c.secondary, "fiveseven_dw_mp");
    assert!(e.draft.to_spec().contains("secondary=fiveseven_dw_mp"));
    let back = Draft::from_stored(&c);
    assert_eq!((back.secondary.as_str(), back.secondary_atts.clone()), ("fiveseven_mp", vec!["dw".to_string()]));
    assert_eq!(back.spent(), e.draft.spent(), "Dual Wield costs its attachment point");
    // The `_dw` weapon is no item of its own: the secondary list never shows it.
    assert!(!gun_ids(&[items::SECONDARY]).contains(&"fiveseven_dw_mp"));
}

// --- The wildcard grid from the table ---

/// The grid lists the table's bonus cards by sort key, then index (`GetUnlockablesByGroupName`
/// with `UnlockablesComparisonFunction`, G17): `f-wildcards/001`'s order.
#[test]
fn the_wildcard_grid_is_the_tables_cards_in_sort_order() {
    assert_eq!(
        wildcard_list(),
        vec![
            Wildcard::Perk1Greed,
            Wildcard::Perk2Greed,
            Wildcard::Perk3Greed,
            Wildcard::Overkill,
            Wildcard::SecondaryGunfighter,
            Wildcard::PrimaryGunfighter,
            Wildcard::Tactician,
            Wildcard::DangerClose,
        ]
    );
    let sorts: Vec<(i32, i32)> = weapons::catalog().bonus_cards().iter().map(|c| (c.sort, c.index)).collect();
    assert!(sorts.windows(2).all(|w| w[0] <= w[1]), "{sorts:?}");
    let mut e = Editor::new(Draft::test_kit());
    e.slot = Slot::Wildcards;
    assert_eq!(e.options(), wildcard_list().iter().map(|w| w.name().to_string()).collect::<Vec<_>>());
    for (i, w) in wildcard_list().into_iter().enumerate() {
        assert_eq!(e.option_wildcard(i), Some(w));
    }
}

/// A card is charged its row's cost (BO2's column 12).
#[test]
fn a_wildcard_costs_its_cards_cost() {
    let mut d = Draft::test_kit();
    let before = d.spent();
    d.wildcards.push(Wildcard::Overkill);
    assert_eq!(d.spent() - before, Wildcard::Overkill.card().map(|c| c.cost).unwrap());
}

/// BO2's grid marks Tactician and Danger Close mutually exclusive (`bonuscardgridbutton.lua`):
/// with one equipped, the other is denied untouched; turning the first off frees it.
#[test]
fn tactician_and_danger_close_exclude_each_other() {
    let mut e = Editor::new(Draft { perks: [None; 3], ..Draft::test_kit() });
    e.slot = Slot::Wildcards;
    let at = |w: Wildcard| wildcard_list().iter().position(|x| *x == w).unwrap();
    let pick = |e: &mut Editor, w: Wildcard| {
        e.picking = true;
        e.option = at(w);
        e.denied = false;
        e.step(Edges { confirm: true, ..Default::default() });
    };
    pick(&mut e, Wildcard::Tactician);
    assert_eq!(e.draft.wildcards, vec![Wildcard::Tactician]);
    let before = e.draft.clone();
    pick(&mut e, Wildcard::DangerClose);
    assert!(e.denied, "Danger Close is refused beside Tactician");
    assert_eq!(e.draft, before);
    pick(&mut e, Wildcard::Tactician);
    assert!(e.draft.wildcards.is_empty());
    pick(&mut e, Wildcard::DangerClose);
    assert_eq!(e.draft.wildcards, vec![Wildcard::DangerClose]);
    pick(&mut e, Wildcard::Tactician);
    assert!(e.denied, "and the other way round");
    assert_eq!(Wildcard::Overkill.excludes(), None);
}

/// The profile stores no wildcard list (a class's cards are re-derived from what it holds), so a
/// profile written before the cards came from the table loads with the same cards: a v2 record
/// with two Tier 1 perks re-opens with Perk 1 Greed, charged its card's cost.
#[test]
fn an_old_profiles_class_reloads_with_its_cards() {
    let _g = crate::profile::lock();
    let tier1 = perk_list(1);
    let (a, b) = (tier1[0].def().id, tier1[1].def().id);
    let text = format!("v2\nactive 1\nset 1 CLASS SET 1\nclass\nGreedy\nprimary=an94_mp;perks={a},{b}\nend\n");
    let p = crate::profile::Profile::from_text(&text);
    let c = p.customs().first().expect("the class loads").clone();
    let d = Draft::from_stored(&c);
    assert_eq!(d.wildcards, vec![Wildcard::Perk1Greed]);
    assert_eq!(d.extra_perks[0].map(|p| p.def().id), Some(b));
    // Saved back, the class is the same record.
    assert_eq!(d.to_custom().perks, c.perks);
    let mut again = crate::profile::Profile::new();
    assert!(again.save(d.to_custom()));
    let back = crate::profile::Profile::from_text(&again.to_text());
    assert_eq!(Draft::from_stored(&back.customs()[0]).wildcards, vec![Wildcard::Perk1Greed]);
}

/// Danger Close gives the lethal button a second box (`cacgrenadesbutton.lua:130-180`): the card
/// costs its own point, the second lethal is filled like the tactical's second box and costs
/// another, the class carries the count (`primarygrenadecount` 2) and re-opening the stored class
/// gives the card back (`checkclasses.lua:688-700`). Taking the card off drops the second lethal
/// (`bonuscardbutton.lua:490-545`).
#[test]
fn danger_close_opens_a_second_lethal_box_that_costs_a_point() {
    let plain = Draft::test_kit();
    let mut e = Editor::new(Draft { perks: [None, None, None], ..Draft::test_kit() });
    let base = e.draft.spent();
    assert_eq!(e.draft.offhand_boxes(true), 1, "one box without the card");
    assert_eq!(e.apply_second_offhand(true), ApplyResult::NoChange, "no second box to fill");
    e.slot = Slot::Wildcards;
    let dc = wildcard_list().iter().position(|w| *w == Wildcard::DangerClose).expect("the card is listed");
    assert_eq!(e.apply(dc), ApplyResult::Changed);
    assert_eq!((e.draft.offhand_boxes(true), e.draft.lethal_count(), e.draft.spent()), (2, 1, base + 1), "the card alone");
    assert_eq!(e.apply_second_offhand(true), ApplyResult::Changed);
    assert_eq!((e.draft.lethal_count(), e.draft.offhand_count(true), e.draft.spent()), (2, 2, base + 2), "the second lethal");
    let c = e.draft.to_custom();
    assert_eq!(c.lethal_count, crate::classes::max_grenades() as u8);
    assert_eq!(c.loadout().lethal_count, Some(2));
    assert!(c.to_spec().contains("lethal_count=2"), "{}", c.to_spec());
    let back = Draft::from_custom(&c);
    assert_eq!((back.wildcards.clone(), back.lethal_count()), (vec![Wildcard::DangerClose], 2));
    // Re-picking the held lethal toggles the second box, as the tactical slot does.
    e.slot = Slot::Lethal;
    let frag = e.offhand_list(true).iter().position(|id| *id == e.draft.lethal).expect("the frag is listed");
    assert_eq!(e.apply(frag), ApplyResult::Changed);
    assert_eq!(e.draft.lethal_count(), 1);
    assert_eq!(e.apply(frag), ApplyResult::Changed);
    assert_eq!(e.draft.lethal_count(), 2);
    // The card off: one box, one lethal.
    e.slot = Slot::Wildcards;
    assert_eq!(e.apply(dc), ApplyResult::Changed);
    assert_eq!((e.draft.offhand_boxes(true), e.draft.lethal_count(), e.draft.spent()), (1, 1, base));
    // No lethal, no second one: the card alone.
    let bare = Draft { lethal: String::new(), wildcards: vec![Wildcard::DangerClose], ..Draft::test_kit() };
    assert_eq!(bare.to_custom().lethal_count, 0);
    assert_eq!(plain.to_custom().lethal_count, 1);
}

#[test]
fn the_offhand_slots_list_their_statstable_items_placed_equipment_included() {
    // BO2 lists the offhands by statstable slot (column 13): the claymore is a `primarygrenade`
    // item and the Trophy System a `specialgrenade` one, though both are placed equipment
    // (`Class::Equipment`), not grenades.
    let (lethal, tactical) = (equipment_ids(true), equipment_ids(false));
    assert_eq!(weapons::weapon("claymore_mp").class, crate::weapons::Class::Equipment);
    assert_eq!(weapons::weapon("trophy_system_mp").class, crate::weapons::Class::Equipment);
    assert!(lethal.contains(&"frag_grenade_mp") && lethal.contains(&"claymore_mp"), "{lethal:?}");
    assert!(tactical.contains(&"concussion_grenade_mp") && tactical.contains(&"trophy_system_mp"), "{tactical:?}");
    assert!(!lethal.contains(&"trophy_system_mp") && !tactical.contains(&"claymore_mp"));
    // Picking it from the list puts it in the draft.
    let mut e = Editor::new(Draft::test_kit());
    e.slot = Slot::Lethal;
    let i = lethal.iter().position(|id| *id == "claymore_mp").unwrap();
    assert_eq!(e.options()[i], "Claymore");
    assert!(e.apply_option(i));
    assert_eq!(e.draft.lethal, "claymore_mp");
    assert_eq!(e.current_option_index(), i);
}

#[test]
fn the_primary_slot_offers_the_assault_shield_and_repair_keeps_it() {
    let ids = gun_ids(&items::slots_of(true, &[]));
    assert!(ids.contains(&"riotshield_mp"), "{ids:?}");
    assert!(!gun_ids(&items::slots_of(false, &[])).contains(&"riotshield_mp"), "a primary item only");
    assert!(items::fits_slot("riotshield_mp", true));
}

/// Under Tactician the lethal slot lists the tactical items (`cacgrenades.lua:183-195`), so the
/// card works end to end: pick it, put a tactical in the lethal slot, save, and the reopened class
/// has the card back (re-derived from the slot, `checkclasses.lua:677-686`) and the same list.
/// Turning the card off clears the tactical from the lethal slot, as BO2's `ClearSlot` does.
#[test]
fn tactician_lists_the_tactical_items_in_the_lethal_slot() {
    let at = |w: Wildcard| wildcard_list().iter().position(|x| *x == w).unwrap();
    let mut e = Editor::new(Draft { perks: [None; 3], ..Draft::test_kit() });
    e.slot = Slot::Lethal;
    assert_eq!(e.options(), equipment_ids(true).iter().map(|id| weapons::weapon(id).display_name.clone()).collect::<Vec<_>>());
    e.slot = Slot::Wildcards;
    assert!(e.apply_option(at(Wildcard::Tactician)));
    e.slot = Slot::Lethal;
    let tactical = equipment_ids(false);
    assert_eq!(e.options(), equipment_options(&tactical), "the lethal slot lists the tactical slot's items");
    assert!(!e.options().contains(&weapons::weapon("frag_grenade_mp").display_name));
    let i = tactical.iter().position(|id| *id == "concussion_grenade_mp").unwrap();
    assert_eq!(e.option_weapon(i).map(|d| d.id.as_str()), Some("concussion_grenade_mp"));
    assert_eq!(e.apply(i), ApplyResult::Changed);
    assert_eq!(e.draft.lethal, "concussion_grenade_mp");
    assert!(e.option_chosen(i));
    // Saved and reopened (through the profile repair): the card and the listing come back.
    let back = Draft::from_stored(&e.draft.to_custom());
    assert_eq!(back.lethal, "concussion_grenade_mp");
    assert!(back.wildcards.contains(&Wildcard::Tactician), "{:?}", back.wildcards);
    let mut again = Editor::new(back);
    again.slot = Slot::Lethal;
    assert_eq!(again.current_option_index(), i);
    // Without the card the tactical cannot stay in the lethal slot.
    again.slot = Slot::Wildcards;
    assert!(again.apply_option(at(Wildcard::Tactician)));
    assert!(again.draft.lethal.is_empty());
    again.slot = Slot::Lethal;
    assert_eq!(again.options(), equipment_options(&equipment_ids(true)));
}

/// BO2's `specialgrenadecount`: the tactical slot carries one or two of its item (each a point;
/// `math.min(2, GetMaxAmmoForItem)` boxes), picking the held item again takes or returns the
/// second, another item starts at one, and the saved class gives and re-opens with the count.
#[test]
fn a_second_tactical_costs_a_point_and_round_trips() {
    let mut e = Editor::new(Draft::test_kit());
    let base = e.draft.spent();
    assert_eq!(e.draft.tactical_count(), 1);
    assert_eq!(e.draft.to_custom().tactical_count, 1, "the class's own count, not the item's start count");
    e.slot = Slot::Tactical;
    let i = e.offhand_list(false).iter().position(|id| *id == "concussion_grenade_mp").unwrap();
    assert_eq!(items::offhand_cap("concussion_grenade_mp"), crate::classes::max_grenades() as u8);
    assert_eq!(e.apply(i), ApplyResult::Changed, "re-picking the held item takes its second box");
    assert_eq!((e.draft.tactical_count(), e.draft.spent()), (2, base + 1));
    let c = e.draft.to_custom();
    assert_eq!((c.tactical_count, c.loadout().tactical_count), (2, Some(2)));
    assert!(c.to_spec().contains("tactical_count=2"), "{}", c.to_spec());
    let back = Draft::from_stored(&c);
    assert_eq!((back.tactical_count(), back.spent()), (2, base + 1));
    assert_eq!(e.apply(i), ApplyResult::Changed, "and again returns it");
    assert_eq!(e.draft.tactical_count(), 1);
    // Over budget, the second is denied and the draft untouched.
    let mut full = Editor::new(Draft { wildcards: vec![Wildcard::Perk1Greed], extra_perks: [Some(Perk::HARDLINE), None, None], ..Draft::test_kit() });
    full.draft.secondary_atts = vec![];
    while full.draft.remaining() > 0 {
        full.draft.primary_atts.push("x".into());
    }
    full.slot = Slot::Tactical;
    let before = full.draft.clone();
    assert_eq!(full.apply(i), ApplyResult::Denied);
    assert_eq!(full.draft, before);
    // A stored count is clamped to the item's cap by the repair.
    let mut c = Draft::test_kit().to_custom();
    c.tactical_count = 7;
    let _ = crate::classrepair::repair(&mut c);
    assert_eq!(c.tactical_count, items::offhand_cap(&c.tactical));
}

/// BO2 orders a CaC list by the statstable sort key, then the item index (G17, `FUN_008cdde0`):
/// the editor's entries follow the table's `CacOrder`, and a table carries it through its text.
#[test]
fn the_cac_lists_follow_the_sort_key_then_the_index() {
    let entries = cac_entries();
    assert!(entries.windows(2).all(|w| items::list_key(w[0]) <= items::list_key(w[1])));
    let mut defs = vec![
        weapons::WeaponDef { id: "b_mp".into(), display_name: "B".into(), cac_slot: "primary".into(), cac_sort: 1, cac_index: 9, ..Default::default() },
        weapons::WeaponDef { id: "a_mp".into(), display_name: "A".into(), cac_slot: "primary".into(), cac_sort: 1, cac_index: 3, ..Default::default() },
        weapons::WeaponDef { id: "c_mp".into(), display_name: "C".into(), cac_slot: "primary".into(), cac_sort: 0, cac_index: 20, ..Default::default() },
    ];
    let back = weapons::table::parse_table(&weapons::table::format_table(&defs)).unwrap();
    assert_eq!(back.iter().map(items::list_key).collect::<Vec<_>>(), defs.iter().map(items::list_key).collect::<Vec<_>>());
    defs.sort_by_key(items::list_key);
    assert_eq!(defs.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["c_mp", "a_mp", "b_mp"]);
}

#[test]
fn the_attachment_grid_keeps_the_weapons_order_whatever_is_equipped() {
    // BO2's grid adds one button per attachment number (`cacattachmentsmenu.lua:858-879`): an
    // equipped attachment stays in its place, and the grid opens on the first one equipped.
    let mut e = Editor::new(Draft::test_kit());
    let offered = offered_attachments(&e.draft.primary);
    e.slot = Slot::PrimaryAtts;
    for (i, short) in offered.iter().enumerate() {
        assert_eq!(e.option_attachment(i), Some(*short));
    }
    assert_eq!(e.options().len(), offered.len());
    let first = offered.iter().position(|s| *s == e.draft.primary_atts[0]).expect("the draft's attachment is offered");
    assert_eq!(e.current_option_index(), first);
}

/// A weapon list that does not offer the held gun opens on the first gun of its first category
/// (BO2's carousel list keeps its first carousel's first card, `cardcarousellist.lua:893`): a
/// pistol secondary under Overkill opens on a submachine gun, not the catalog's first gun.
#[test]
fn a_list_without_the_held_gun_opens_on_its_first_category() {
    let mut e = Editor::new(Draft::test_kit());
    e.draft.wildcards.push(Wildcard::Overkill);
    e.slot = Slot::Secondary;
    assert!(!e.gun_list(false).contains(&e.draft.secondary.as_str()), "the pistol is not offered");
    let i = e.current_option_index();
    let first = crate::classui::category_order(Slot::Secondary).into_iter().find(|c| e.gun_list(false).iter().any(|id| weapons::weapon(id).cac_class() == *c)).unwrap();
    assert_eq!(e.option_weapon(i).unwrap().cac_class(), first);
    let ids = e.gun_list(false);
    assert_eq!(ids.iter().position(|id| weapons::weapon(id).cac_class() == first), Some(i), "its first gun");
    // The held gun, when offered, is still where the list opens.
    e.slot = Slot::Primary;
    assert_eq!(e.option_weapon(e.current_option_index()).unwrap().id, e.draft.primary);
}
