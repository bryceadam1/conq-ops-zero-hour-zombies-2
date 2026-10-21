//! The class screens' tests.

use super::*;
use crate::classes::CustomClass;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    crate::profile::lock()
}

fn custom(name: &str, primary: &str) -> CustomClass {
    CustomClass { name: name.into(), primary: primary.into(), ..CustomClass::default() }
}

fn texts(prims: &[Prim]) -> Vec<String> {
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// One frame with `keys` freshly pressed (released the frame after).
fn press(editor: &mut Editor, prev: &mut MenuPrev, keys: &[Key]) -> EditorFrame {
    let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    for k in keys {
        pointer.keys.set_down(*k, true);
    }
    let frame = editor_frame(editor, &pointer, prev);
    prev.update(&pointer);
    pointer.keys.clear();
    editor_frame(editor, &pointer, prev);
    prev.update(&pointer);
    frame
}

#[test]
fn the_shell_pick_list_is_presets_then_customs_then_create_and_the_match_list_has_no_create() {
    let _g = lock();
    crate::profile::reset();
    let presets = classes::get().len();
    let rows = pick_rows(true);
    assert_eq!(rows.len(), presets + 1, "an empty profile: presets + CREATE");
    assert_eq!(rows[presets], PickRow::Create);
    // In a match there is no CREATE A CLASS row (picks only).
    let rows = pick_rows(false);
    assert_eq!(rows.len(), presets, "the in-match list is the presets alone");
    assert!(!rows.contains(&PickRow::Create));
    crate::profile::save(custom("AB", "an94_mp"));
    crate::profile::save(custom("Sniper", "dsr50_mp"));
    let rows = pick_rows(true);
    assert_eq!(rows.len(), presets + 3);
    assert_eq!(rows[presets], PickRow::Custom(0));
    assert_eq!(rows[presets + 1], PickRow::Custom(1));
    assert_eq!(rows[presets + 2], PickRow::Create);
    // The in-match list keeps the customs but drops CREATE.
    let rows = pick_rows(false);
    assert_eq!(rows.len(), presets + 2);
    assert_eq!(rows[presets], PickRow::Custom(0));
    assert!(!rows.contains(&PickRow::Create));
    crate::profile::reset();
}

#[test]
fn the_pick_list_describes_the_active_class_set() {
    let _g = lock();
    crate::profile::reset();
    // Set 1 of ten, BO2's default name, the five defaults and (in the shell) CREATE.
    let presets = classes::get().len();
    let list = pick_list(true);
    assert_eq!((list.set, list.sets), (0, 10));
    assert_eq!(list.set_number(), 1);
    assert_eq!(list.marker(), 0, "the first dot is the marker on set 1");
    assert_eq!(list.set_name, "CLASS SET 1");
    assert_eq!(list.rows.iter().filter(|r| matches!(r, PickRow::Preset(_))).count(), presets);
    assert!(list.rows.iter().all(|r| !matches!(r, PickRow::Custom(_))), "no customs yet");
    assert_eq!(list.rows.last(), Some(&PickRow::Create));
    // A class saved in set 2 shows only there, and the pager's data follows the set.
    crate::profile::save(custom("Set One", "an94_mp"));
    crate::profile::rename_set(1, "SNIPERS");
    crate::profile::set_active(1);
    crate::profile::save(custom("Set Two", "dsr50_mp"));
    let list = pick_list(true);
    assert_eq!((list.set, list.set_number()), (1, 2));
    assert_eq!(list.set_name, "SNIPERS");
    assert_eq!(list.marker(), 1, "the second dot marks set 2");
    assert_eq!(list.rows[presets], PickRow::Custom(0));
    assert_eq!(list.rows[presets + 1], PickRow::Create, "one custom in this set");
    // Left/Right step the set only in the shell; a match's fold ignores them.
    let mut inp = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    let prev = MenuPrev::default();
    inp.keys.set_down(Key::Right, true);
    assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, 1);
    assert_eq!(pick_frame(&inp, &prev, 0, false).set_step, 0);
    inp.keys.set_down(Key::Right, false);
    inp.keys.set_down(Key::Left, true);
    assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, -1);
    crate::profile::reset();
}

/// The pager the shell's Left/Right drives: ten steps of +1 walk all ten sets and come back to
/// the first, and the marker follows (BO2's pager wraps, `a-chooseclass/011`).
#[test]
fn the_set_pager_steps_through_all_ten_sets_and_wraps() {
    let _g = lock();
    crate::profile::reset();
    let seen: Vec<usize> = (0..profile::MAX_SETS)
        .map(|i| {
            if i > 0 {
                assert!(crate::profile::cycle_active(1).is_some());
            }
            let list = pick_list(true);
            assert_eq!(list.marker(), i.min(profile::MAX_SETS - 1), "the marker follows the active set");
            list.set_number()
        })
        .collect();
    assert_eq!(seen, (1..=profile::MAX_SETS).collect::<Vec<_>>());
    // One more step wraps back to the first set, and a step back wraps to the last.
    crate::profile::cycle_active(1);
    assert_eq!(pick_list(true).set_number(), 1, "the pager wraps forward");
    crate::profile::cycle_active(-1);
    assert_eq!(pick_list(true).set_number(), profile::MAX_SETS, "and backward");
    crate::profile::reset();
}

#[test]
fn pick_hit_testing_finds_every_row_in_the_window() {
    let l = pick_layout(1280.0, 720.0, 1.0);
    let visible = pick_visible(&l);
    for i in 0..visible.min(7) {
        let y = l.list_y + i as f32 * l.row_h + 4.0;
        assert_eq!(pick_row_at(&l, l.list_x + 10.0, y, 7, 0), Some(i), "row {i}");
    }
    // The scroll window: the first shown row is `first`, so the first slot is that row.
    let first = 2;
    assert_eq!(pick_row_at(&l, l.list_x + 10.0, l.list_y + 4.0, 7, first), Some(first));
    assert_eq!(pick_row_at(&l, l.list_x - 5.0, l.list_y + 5.0, 7, 0), None, "left of the list");
    assert_eq!(pick_row_at(&l, l.list_x + 10.0, l.list_y + 7.0 * l.row_h, 7, 0), None, "past the last row");
}

#[test]
fn the_shell_footer_prompts_are_hit_tested_and_absent_in_a_match() {
    let l = pick_layout(1280.0, 720.0, 1.0);
    let actions = pick_footer_actions(&l, 1280.0, 1.0, true);
    assert_eq!(
        actions.iter().map(|(a, ..)| *a).collect::<Vec<_>>(),
        vec![PickFooterAction::Back, PickFooterAction::SetOptions, PickFooterAction::ClassOptions],
        "BO2's choose-class footer: ESC Back, S Class Set Options, O Class Options"
    );
    for (action, x, y, w, h) in &actions {
        assert_eq!(pick_footer_at(&l, 1280.0, 1.0, x + w * 0.5, y + h * 0.5, true), Some(*action));
    }
    assert_eq!(pick_footer_at(&l, 1280.0, 1.0, l.list_x, l.list_y, true), None, "not a row point");
    assert!(pick_footer_actions(&l, 1280.0, 1.0, false).is_empty(), "no management in a match");
    assert_eq!(pick_footer_at(&l, 1280.0, 1.0, 1200.0, l.footer_y + 5.0, false), None);
    // The prompts carry BO2's keys.
    assert_eq!((PickFooterAction::SetOptions.key(), PickFooterAction::SetOptions.label()), ("S", "Class Set Options"));
    assert_eq!((PickFooterAction::ClassOptions.key(), PickFooterAction::ClassOptions.label()), ("O", "Class Options"));
}

#[test]
fn the_choose_class_footer_carries_bo2s_hint_in_the_shell_only() {
    let _g = lock();
    crate::profile::reset();
    let hint = "You will load into the game with this class set.";
    let mut shell = Vec::new();
    draw_pick(&mut shell, 1280.0, 720.0, 1.0, 0, 0.0, true);
    let shell_text = texts(&shell);
    assert!(shell_text.iter().any(|t| t == hint), "the shell shows BO2's hint: {shell_text:?}");
    assert!(shell_text.iter().any(|t| t == "CLASS SET 1"), "the pager names the set");
    assert!(shell_text.iter().any(|t| t == "Class Set Options"));
    assert!(shell_text.iter().any(|t| t == "Class Options"));
    // A match is already running: no hint, and the pager still shows the active set (read-only).
    let mut game = Vec::new();
    draw_pick(&mut game, 1280.0, 720.0, 1.0, 0, 0.0, false);
    let game_text = texts(&game);
    assert!(!game_text.iter().any(|t| t == hint), "no hint in a match: {game_text:?}");
    assert!(game_text.iter().any(|t| t == "CLASS SET 1"), "the read-only pager draws");
    assert!(!game_text.iter().any(|t| t == "Class Set Options"), "nothing to manage in a match");
    crate::profile::reset();
}

#[test]
fn the_allocation_bar_lights_one_segment_per_point_and_the_counter_says_n_of_ten() {
    for spent in 0..=10 {
        assert_eq!(allocation_lit(spent, 10), spent as usize);
    }
    assert_eq!(allocation_lit(-2, 10), 0, "never negative");
    assert_eq!(allocation_lit(14, 10), 10, "clamped to the budget");
    let mut prims = Vec::new();
    draw_allocation(&mut prims, 0.0, 0.0, 200.0, 1.0, 7, 10);
    let lit = prims
        .iter()
        .filter(|p| matches!(p, Prim::Rect { argb, .. } if *argb == ORANGE))
        .count();
    assert_eq!(lit, 7, "seven of the ten blocks are spent");
    assert_eq!(prims.iter().filter(|p| matches!(p, Prim::Rect { .. })).count(), 10, "ten blocks");
}

#[test]
fn pick_hit_testing_still_maps_the_set_pager_row() {
    // The pager sits above the list, so a click on it is not a row and the list starts below it.
    let l = pick_layout(1280.0, 720.0, 1.0);
    assert!(l.list_y > l.pager_y + l.pager_h, "the list starts under the pager");
    assert_eq!(pick_row_at(&l, l.list_x + 10.0, l.pager_y + 4.0, 7, 0), None, "the pager is not a row");
}

#[test]
fn the_editor_grid_hit_tests_every_box() {
    let e = Editor::new(Draft::default());
    let l = editor_layout(1280.0, 720.0, 1.0);
    let cells = editor_cells(&l, &e);
    // Every box of the walk is on screen, and its middle maps back to it. A weapon that offers
    // no attachments has no attachment boxes (BO2's "Attachments not available").
    for slot in e.slots() {
        let offered = match slot {
            Slot::PrimaryAtts => offers_attachments(&e.draft.primary),
            Slot::SecondaryAtts => offers_attachments(&e.draft.secondary),
            _ => true,
        };
        assert_eq!(cells.iter().any(|c| c.slot == slot), offered, "{slot:?} box presence");
    }
    for (i, c) in cells.iter().enumerate() {
        let (cx, cy) = c.center();
        assert_eq!(editor_cell_at(&cells, cx, cy), Some(i), "{:?} box {i}", c.slot);
    }
    // The attachment boxes are one per attachment the gun may carry.
    let atts = cells.iter().filter(|c| c.slot == Slot::PrimaryAtts).count();
    assert_eq!(atts, e.draft.primary_att_max());
    // A point off the grid hits nothing.
    assert_eq!(editor_cell_at(&cells, 5.0, 5.0), None);
}

#[test]
fn the_editor_screens_draw_their_boxes_and_prompts() {
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Perk2;
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), deny: 0.0, manage: true });
    let t = texts(&prims);
    for label in ["PERK 1", "PERK 2", "PERK 3", "LETHAL", "TACTICAL", "ATTACHMENTS", "BONUS CARDS"] {
        assert!(t.iter().any(|x| x == label), "editor label {label}: {t:?}");
    }
    assert!(t.iter().any(|x| x == "Remove"), "the R Remove prompt: {t:?}");
    assert!(t.iter().any(|x| x.contains("AN-94")), "the held primary in its box");
    // The allocation readout replaced the old POINTS LEFT counter.
    assert!(!t.iter().any(|x| x == "POINTS LEFT"), "no Pick-10 wording: {t:?}");
    // A perk list shows the tier's grid.
    e.picking = true;
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), deny: 0.0, manage: true });
    let t = texts(&prims);
    assert!(t.iter().any(|x| x == "Scavenger"), "tier 2 perks are listed: {t:?}");
    assert!(t.iter().any(|x| x == "FastHands"), "tier 2 perks are listed");
}

#[test]
fn a_launcher_shows_attachments_not_available() {
    let mut d = Draft::default();
    d.secondary = "smaw_mp".into();
    let e = Editor::new(d);
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), deny: 0.0, manage: true });
    let t = texts(&prims);
    assert!(t.iter().any(|x| x == "Attachments not available"), "BO2's launcher line: {t:?}");
}

#[test]
fn the_weapon_list_opens_on_the_held_guns_category() {
    // The default draft's primary (the AN-94) is a rifle, so the list opens on that category
    // and the highlight is one of its guns.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Enter]);
    assert!(e.picking, "Enter opens the primary list");
    let cats = weapon_categories(&e);
    assert!(!cats.is_empty());
    let active = category_of(&cats, e.option);
    assert_eq!(cats[active].name, "ASSAULT RIFLES", "the list opens on the held gun's category");
    assert_eq!(cats[active].options[0], e.option, "and on the held gun itself");
}

/// A two-category list, as BO2's primary list is: the helpers walk it without the catalog.
fn two_categories() -> Vec<Category> {
    vec![
        Category { class: WeaponClass::Smg, name: "SUBMACHINE GUNS", options: vec![0, 1, 2] },
        Category { class: WeaponClass::Rifle, name: "ASSAULT RIFLES", options: vec![3, 4] },
    ]
}

#[test]
fn left_right_walk_the_categories_and_up_down_the_guns_inside_one() {
    let cats = two_categories();
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Primary;
    e.picking = true;
    e.option = 1;
    // Down/Up stay inside the SMG category and wrap inside it.
    assert!(step_within_category(&mut e, &cats, 1));
    assert_eq!(e.option, 2);
    step_within_category(&mut e, &cats, 1);
    assert_eq!(e.option, 0, "Down at the end of a category wraps to its first gun");
    step_within_category(&mut e, &cats, -1);
    assert_eq!(e.option, 2, "Up at the start wraps to its last");
    // Right/Left move to the neighbouring category's first gun and wrap across the column.
    step_category(&mut e, &cats, 1);
    assert_eq!(e.option, 3, "Right lands on the next category's first gun");
    step_category(&mut e, &cats, 1);
    assert_eq!(e.option, 0, "the last category wraps back to the first");
    step_category(&mut e, &cats, -1);
    assert_eq!(e.option, 3, "and back to the last");
    // Through the fold: a right press moves the highlight to the next category.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Enter]);
    let cats = weapon_categories(&e);
    let before = category_of(&cats, e.option);
    press(&mut e, &mut prev, &[Key::Right]);
    assert_eq!(category_of(&cats, e.option), (before + 1) % cats.len(), "the fold walks the categories");
    press(&mut e, &mut prev, &[Key::Left]);
    assert_eq!(category_of(&cats, e.option), before, "and back");
    // Down never leaves the category, however long the list is.
    press(&mut e, &mut prev, &[Key::Down]);
    assert_eq!(category_of(&cats, e.option), before, "Up/Down stay inside the active category");
}

#[test]
fn enter_picks_the_highlighted_gun_of_the_active_category() {
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Enter]);
    let cats = weapon_categories(&e);
    let active = category_of(&cats, e.option);
    // Walk within the category, then choose.
    press(&mut e, &mut prev, &[Key::Down]);
    assert_eq!(category_of(&cats, e.option), active);
    let target = e.option_weapon(e.option).expect("a gun under the highlight").id.clone();
    press(&mut e, &mut prev, &[Key::Enter]);
    assert!(!e.picking);
    assert_eq!(e.draft.primary, target, "the highlighted gun is the draft's primary");
}

#[test]
fn the_category_column_rotates_so_the_active_one_is_on_top() {
    let d = Draft::default();
    let mut e = Editor::new(d);
    e.slot = Slot::Primary;
    e.picking = true;
    let cats = weapon_categories(&e);
    let column = category_column(&cats, 2);
    assert_eq!(column[0], 2, "the active category is on top");
    assert_eq!(column.len(), cats.len(), "every category is listed once");
    let mut sorted = column.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..cats.len()).collect::<Vec<_>>(), "no duplicates");
}

#[test]
fn the_secondary_slot_lists_pistols_first() {
    let d = Draft { secondary: "fnp45_mp".into(), ..Draft::default() };
    let mut e = Editor::new(d);
    e.slot = Slot::Secondary;
    e.picking = true;
    let cats = weapon_categories(&e);
    assert_eq!(cats[0].name, "PISTOLS", "BO2's secondary order starts with pistols");
    assert!(cats.iter().any(|c| c.name == "LAUNCHERS"), "and lists launchers");
}

#[test]
fn r_clears_the_highlighted_slot_and_an_empty_one_is_a_no_op() {
    let mut d = Draft::default();
    d.primary_atts = vec!["gl".into(), "reflex".into()];
    let mut e = Editor::new(d);
    let mut prev = MenuPrev::default();
    // R on the primary clears the gun and its attachments (BO2's ClearWeapon).
    assert_eq!(e.slot, Slot::Primary);
    let frame = press(&mut e, &mut prev, &[Key::R]);
    assert!(frame.removed, "the remove fired");
    assert!(e.draft.primary.is_empty(), "the gun went");
    assert!(e.draft.primary_atts.is_empty(), "and its attachments");
    // A second R on the now-empty slot does nothing (no deny, no remove).
    let frame = press(&mut e, &mut prev, &[Key::R]);
    assert!(!frame.removed, "nothing left to remove");
    assert!(!frame.denied);
    // R on a perk tier clears that tier only.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Down]); // Primary -> PrimaryAtts
    press(&mut e, &mut prev, &[Key::Down]); // -> Secondary
    press(&mut e, &mut prev, &[Key::Down]); // -> SecondaryAtts
    press(&mut e, &mut prev, &[Key::Down]); // -> Lethal
    press(&mut e, &mut prev, &[Key::Down]); // -> Tactical
    press(&mut e, &mut prev, &[Key::Down]); // -> Perk 1
    assert_eq!(e.slot, Slot::Perk1);
    let frame = press(&mut e, &mut prev, &[Key::R]);
    assert!(frame.removed);
    assert!(e.draft.perks[0].is_none(), "tier 1 is empty");
    assert!(e.draft.perks[1].is_some(), "tier 2 is untouched");
}

#[test]
fn the_circled_x_clears_only_the_selected_filled_box() {
    let mut d = Draft::default();
    d.primary_atts = vec!["gl".into(), "reflex".into()];
    let mut e = Editor::new(d);
    let l = editor_layout(1280.0, 720.0, 1.0);
    // Select the attachment strip, then click the second attachment's X.
    e.slot = Slot::PrimaryAtts;
    let cells = editor_cells(&l, &e);
    let second = cells.iter().find(|c| c.slot == Slot::PrimaryAtts && c.index == 1).expect("the second box");
    let (rx, ry, rw, rh) = remove_rect(second, 1.0);
    assert_eq!(editor_remove_at(&cells, &e, 1.0, rx + rw * 0.5, ry + rh * 0.5), Some((Slot::PrimaryAtts, 1)));
    // The X is not live on an unselected slot (BO2 draws it on the highlighted one).
    e.slot = Slot::Primary;
    assert_eq!(editor_remove_at(&cells, &e, 1.0, rx + rw * 0.5, ry + rh * 0.5), None);
    // Clicking it removes exactly that attachment.
    let mut e = Editor::new(Draft { primary: "an94_mp".into(), primary_atts: vec!["gl".into(), "reflex".into()], ..Draft::default() });
    e.slot = Slot::PrimaryAtts;
    let cells = editor_cells(&l, &e);
    let second = cells.iter().find(|c| c.slot == Slot::PrimaryAtts && c.index == 1).cloned().expect("the second box");
    let (rx, ry, rw, rh) = remove_rect(&second, 1.0);
    let mut prev = MenuPrev::default();
    let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    pointer.mouse_x = rx + rw * 0.5;
    pointer.mouse_y = ry + rh * 0.5;
    let frame = editor_frame(&mut e, &pointer, &prev);
    prev.update(&pointer);
    pointer.mouse_buttons = crate::input::mouse_buttons::LEFT;
    let frame = editor_frame(&mut e, &pointer, &prev);
    assert!(frame.removed, "the click cleared the box");
    assert_eq!(e.draft.primary_atts, vec!["gl".to_string()], "the first attachment stays");
    assert!(!e.picking, "clearing does not open the list");
}

#[test]
fn the_perk_lists_follow_bo2s_grid_order() {
    // e-perks/001, /006, /011: the grids read left to right, top to bottom.
    let expected = [
        vec!["Lightweight", "Hardline", "BlindEye", "FlakJacket", "Ghost"],
        vec!["Toughness", "ColdBlooded", "FastHands", "HardWired", "Scavenger"],
        vec!["Dexterity", "ExtremeConditioning", "Engineer", "TacticalMask", "DeadSilence", "Awareness"],
    ];
    let mut e = Editor::new(Draft::default());
    for (tier, want) in expected.iter().enumerate() {
        e.slot = perk_slot(tier as u8 + 1);
        assert_eq!(e.options(), *want, "perk tier {}", tier + 1);
    }
}

#[test]
fn the_bonus_card_list_follows_bo2s_grid_order() {
    // f-wildcards/001: Perk 1 Greed, Perk 2 Greed, Perk 3 Greed / Overkill, Secondary Gunfighter,
    // Primary Gunfighter / Tactician, Danger Close.
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Wildcards;
    assert_eq!(
        e.options(),
        vec![
            "Perk 1 Greed",
            "Perk 2 Greed",
            "Perk 3 Greed",
            "Overkill",
            "Secondary Gunfighter",
            "Primary Gunfighter",
            "Tactician",
            "Danger Close"
        ]
    );
}

#[test]
fn the_stat_bars_are_in_bo2s_order_and_segmented() {
    assert_eq!(STAT_LABELS, ["FIRE RATE", "DAMAGE", "RANGE", "ACCURACY"]);
    let an94 = weapons::weapon("an94_mp");
    let stats = weapon_stats(an94);
    assert_eq!(stat_value(&stats, 0), stats.fire_rate);
    assert_eq!(stat_value(&stats, 1), stats.damage);
    assert_eq!(stat_value(&stats, 2), stats.range);
    assert_eq!(stat_value(&stats, 3), stats.accuracy);
    assert_eq!(stat_lit(0.0), 0);
    assert_eq!(stat_lit(1.0), STAT_SEGMENTS);
    let (max_damage, _, _, _, _) = gun_bounds();
    let strongest = weapons::catalog().all().iter().copied().find(|d| d.class.is_gun() && (d.damage[0] - max_damage).abs() < 1e-4).expect("a max-damage gun");
    assert_eq!(weapon_stats(strongest).damage, 1.0);
    assert_eq!(weapon_stats(&WeaponDef { damage: [max_damage, 1.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() }).damage, 1.0);
}

#[test]
fn the_dialogs_follow_bo2s_entries_and_escape_backs_out() {
    let set = Dialog::set_options("CLASS SET 1");
    assert_eq!(set.kind.title(), "CLASS SET OPTIONS");
    assert_eq!(set.items.iter().map(|i| i.label()).collect::<Vec<_>>(), vec!["RENAME", "RESET SET"]);
    let class = Dialog::class_options("Custom 5");
    assert_eq!(class.kind.title(), "CLASS OPTIONS");
    assert_eq!(
        class.items.iter().map(|i| i.label()).collect::<Vec<_>>(),
        vec!["RENAME", "COPY", "RESET TO DEFAULT", "CLEAR LOADOUT"],
        "BO2's class dialog: a-chooseclass/023"
    );
    let mut d = class.clone();
    let mut prev = MenuPrev::default();
    let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    pointer.keys.set_down(Key::Down, true);
    assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Continue);
    assert_eq!(d.highlight, 1, "Down walks the entries");
    prev.update(&pointer);
    pointer.keys.set_down(Key::Enter, true);
    assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Confirm(DialogItem::Copy));
    prev.update(&pointer);
    pointer.keys.clear();
    pointer.keys.set_down(Key::Escape, true);
    assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Back);
}

#[test]
fn a_rename_types_into_the_dialog_field_and_commits_on_enter() {
    let mut d = Dialog::set_options("CLASS SET 1");
    d.begin_rename("CLASS SET 1");
    let mut prev = MenuPrev::default();
    let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    for k in [Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace, Key::Backspace] {
        pointer.keys.set_down(k, true);
        dialog_step(&mut d, &prev, &pointer);
        prev.update(&pointer);
        pointer.keys.set_down(k, false);
        dialog_step(&mut d, &prev, &pointer);
        prev.update(&pointer);
    }
    assert!(d.naming);
    for k in [Key::S, Key::N, Key::Key1] {
        pointer.keys.set_down(k, true);
        dialog_step(&mut d, &prev, &pointer);
        prev.update(&pointer);
        pointer.keys.set_down(k, false);
        dialog_step(&mut d, &prev, &pointer);
        prev.update(&pointer);
    }
    assert_eq!(d.buffer, "SN1");
    pointer.keys.set_down(Key::Enter, true);
    assert_eq!(dialog_step(&mut d, &prev, &pointer), DialogOutcome::Confirm(DialogItem::Rename));
    assert!(!d.naming, "Enter closes the field");
}
