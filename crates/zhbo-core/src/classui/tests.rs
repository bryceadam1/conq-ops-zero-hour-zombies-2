//! The class screens' tests.

use super::*;
use crate::classes::CustomClass;
use crate::classmenu::{Focus, Outcome, Personalize, PersonalizeKind};

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
fn the_lobby_lists_the_sets_five_slots_and_the_match_adds_the_defaults() {
    let _g = lock();
    crate::profile::reset();
    // BO2's CACChooseClass: the five custom slots of the active set, nothing else
    // (`cacchooseclass.lua` `UpdateButtonList`, `a-chooseclass/002`: an untouched set is five
    // `Custom n` slots).
    let rows = pick_rows(true);
    assert_eq!(rows, (0..5).map(PickRow::Empty).collect::<Vec<_>>());
    crate::profile::save(custom("AB", "an94_mp"));
    crate::profile::save(custom("Sniper", "dsr50_mp"));
    let rows = pick_rows(true);
    assert_eq!(rows[..3], [PickRow::Custom(0), PickRow::Custom(1), PickRow::Empty(2)]);
    assert_eq!(rows.len(), 5);
    // The match's changeclass: the same five, then the default classes in its order (SMG, CQB,
    // Assault, LMG, Sniper: `class_smg` 10 .. `class_sniper` 14).
    let rows = pick_rows(false);
    assert_eq!(rows[..5], pick_rows(true)[..]);
    let presets = classes::get();
    let ids: Vec<&str> = rows[5..]
        .iter()
        .map(|r| match r {
            PickRow::Preset(j) => presets[*j].id.as_str(),
            other => panic!("only defaults after the slots: {other:?}"),
        })
        .collect();
    assert_eq!(ids, ["smg", "cqb", "assault", "lmg", "sniper"]);
    crate::profile::reset();
}

#[test]
fn the_pick_list_describes_the_active_class_set() {
    let _g = lock();
    crate::profile::reset();
    let list = pick_list(true);
    assert_eq!((list.set, list.sets), (0, 10));
    assert_eq!(list.set_number(), 1);
    assert_eq!(list.marker(), 0, "the first pip is the marker on set 1");
    assert_eq!(list.set_name, "CLASS SET 1");
    // A class saved in set 2 shows only there, and the header's data follows the set.
    crate::profile::save(custom("Set One", "an94_mp"));
    crate::profile::rename_set(1, "SNIPERS");
    crate::profile::set_active(1);
    crate::profile::save(custom("Set Two", "dsr50_mp"));
    let list = pick_list(true);
    assert_eq!((list.set, list.set_number()), (1, 2));
    assert_eq!(list.set_name, "SNIPERS");
    assert_eq!(list.marker(), 1, "the second pip marks set 2");
    assert_eq!(list.rows[..2], [PickRow::Custom(0), PickRow::Empty(1)], "one custom in this set");
    // BO2's shoulders (`A`/`D` on PC, t6mp `FUN_009376a0`) step the set only in the lobby; a
    // match's fold ignores them. The arrows never page: the class list is a vertical list.
    let mut inp = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    let prev = MenuPrev::default();
    inp.keys.set_down(SHOULDER_R, true);
    assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, 1);
    assert_eq!(pick_frame(&inp, &prev, 0, false).set_step, 0);
    inp.keys.set_down(SHOULDER_R, false);
    inp.keys.set_down(SHOULDER_L, true);
    assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, -1);
    inp.keys.set_down(SHOULDER_L, false);
    for k in [Key::Left, Key::Right] {
        inp.keys.set_down(k, true);
        assert_eq!(pick_frame(&inp, &prev, 0, true).set_step, 0, "{k:?} does not page");
        inp.keys.set_down(k, false);
    }
    crate::profile::reset();
}

/// The pager the shell's shoulders drive: ten steps of +1 walk all ten sets and come back to
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
fn pick_hit_testing_follows_the_luas_button_list() {
    // The lobby list: 30-unit buttons 2 apart under the set header (`cacchooseclass.lua`), 218
    // wide; at 720p one unit is one pixel and the 864-wide menu starts at (1280 - 864) / 2.
    let l = pick_layout(1280.0, 720.0, true);
    assert_eq!(l.frame.menu.x, 208.0);
    assert_eq!(l.list.x, 208.0);
    assert_eq!(l.list.y, 36.0 + 48.0 + 10.0 + 35.0 + 10.0);
    assert_eq!(l.list.w, 218.0);
    let rows: Vec<PickRow> = (0..5).map(PickRow::Empty).collect();
    for i in 0..5 {
        let y = l.list.y + i as f32 * (ROW_H + ROW_SPACING) + 4.0;
        assert_eq!(pick_row_at(&l, l.list.x + 10.0, y, &rows), Some(i), "row {i}");
    }
    assert_eq!(pick_row_at(&l, l.list.x - 5.0, l.list.y + 5.0, &rows), None, "left of the list");
    assert_eq!(pick_row_at(&l, l.list.x + 10.0, l.list.y + ROW_H + 1.0, &rows), None, "the 2-unit spacing");
    assert_eq!(pick_row_at(&l, l.list.x + 10.0, l.list.y - 20.0, &rows), None, "the set header is not a row");
    // The match list: the set's name and a spacer come first.
    let m = pick_layout(1280.0, 720.0, false);
    let rects = row_rects(&m, &rows);
    assert_eq!(rects[0].y, m.list.y + bo2::TEXT_CONDENSED + ROW_SPACING + ROW_H / 2.0 + ROW_SPACING);
}

#[test]
fn the_footer_prompts_are_hit_tested_in_bo2s_order() {
    let l = pick_layout(1280.0, 720.0, true);
    let actions = pick_footer_actions(&l);
    assert_eq!(
        actions.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
        vec![PickFooterAction::Back, PickFooterAction::ClassOptions, PickFooterAction::SetOptions],
        "BO2's footer: ESC Back left, O Class Options rightmost, S Class Set Options beside it"
    );
    assert!(actions[1].1.x > actions[2].1.x, "Class Options is right of Class Set Options");
    for (action, r) in &actions {
        let p = l.frame.px(*r);
        assert_eq!(pick_footer_at(&l, p.x + p.w * 0.5, p.y + p.h * 0.5), Some(*action));
    }
    assert_eq!(pick_footer_at(&l, l.list.x, l.list.y), None, "not a row point");
    let m = pick_layout(1280.0, 720.0, false);
    assert_eq!(pick_footer_actions(&m).iter().map(|(a, _)| *a).collect::<Vec<_>>(), vec![PickFooterAction::Back]);
    assert_eq!((PickFooterAction::SetOptions.key(), PickFooterAction::SetOptions.label()), ("S", "Class Set Options"));
    assert_eq!((PickFooterAction::ClassOptions.key(), PickFooterAction::ClassOptions.label()), ("O", "Class Options"));
}

#[test]
fn the_lobby_shows_the_set_header_and_hint_and_the_match_the_sets_name() {
    let _g = lock();
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    crate::profile::reset();
    let mut lobby = Vec::new();
    draw_pick(&mut lobby, 1280.0, 720.0, &ScreenView { highlight: 0, editor: None, personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None });
    let lobby_text = texts(&lobby);
    for want in ["CHOOSE CLASS", "CLASS SET 1", "Custom 1", "Custom 5", "Class Set Options", "Class Options", "PRIMARY", "WILDCARDS"] {
        assert!(lobby_text.iter().any(|t| t == want), "the lobby shows {want:?}: {lobby_text:?}");
    }
    // The hint wraps onto lines of the list's width.
    assert!(lobby_text.iter().any(|t| SET_HINT.starts_with(t.as_str()) && !t.is_empty()), "the hint: {lobby_text:?}");
    let mut game = Vec::new();
    draw_pick(&mut game, 1280.0, 720.0, &ScreenView { highlight: 0, editor: None, personalize: None, deny: 0.0, manage: false, now: 0.0, set_change: None, open: None });
    let game_text = texts(&game);
    assert!(!game_text.iter().any(|t| t.starts_with("You will load")), "no hint in a match: {game_text:?}");
    assert!(game_text.iter().any(|t| t == "CLASS SET 1"), "the match list names the set");
    assert!(!game_text.iter().any(|t| t == "Class Set Options"), "nothing to manage in a match");
    crate::profile::reset();
}

#[test]
fn a_class_set_switch_slides_the_list_in_and_fades_the_preview() {
    let _g = lock();
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    crate::profile::reset();
    let view = |settle: f32| {
        let mut prims = Vec::new();
        draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: None, personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: Some(settle * pick::SET_SWITCH_MS / 1000.0), open: None });
        prims
    };
    let started = view(0.0);
    let landed = view(1.0);
    assert_eq!(pick::SET_SWITCH_MS, 250.0, "cacchooseclass.lua's beginAnimation duration");
    // The list starts a list-width plus 20 to the left of its resting place (`setLeftRight` aims its
    // right edge at -320), so the same row's text is further left at settle 0.
    let lobby = pick_layout(1280.0, 720.0, true);
    let row_pen = |prims: &[Prim]| {
        prims
            .iter()
            .find_map(|p| match p {
                Prim::Text { text, x, .. } if text == "Custom 1" => Some(*x),
                _ => None,
            })
            .expect("the first row's label")
    };
    let travel = (lobby.list.w + 20.0) * lobby.frame.s();
    assert!((row_pen(&landed) - row_pen(&started) - travel).abs() < 1.0, "the list slides in by its width + 20");
    // The preview fades in over the same 250 ms (`fade_in` with an ease-in): at 0 nothing of it is
    // visible, and by the end it is at full alpha. TACTICAL is the preview's own slot label.
    let alpha_of = |prims: &[Prim], want: &str| -> Option<u32> {
        prims.iter().find_map(|q| match q {
            Prim::Text { text, argb, .. } if text == want => Some(*argb >> 24),
            _ => None,
        })
    };
    // The preview draws its slot labels at 0.5 alpha, so full here is 0x80, not 0xFF.
    assert_eq!(alpha_of(&started, "TACTICAL"), Some(0), "the preview starts invisible");
    assert_eq!(alpha_of(&landed, "TACTICAL"), Some(bo2::argb(bo2::OFF_WHITE, 0.5) >> 24), "and lands at its full alpha");
    // A settled screen has no clip of its own (the clip belongs to the slide).
    assert!(!landed.iter().any(|p| matches!(p, Prim::ClipBegin { .. })), "settled draws no clip");
    assert!(started.iter().any(|p| matches!(p, Prim::ClipBegin { .. })), "the slide clips the list to the menu");
    crate::profile::reset();
}

#[test]
fn a_menu_fades_in_over_the_luas_250ms() {
    let _g = lock();
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    crate::profile::reset();
    // `CoD.Menu.animateIn` (`codmenu.lua:2300-2326`): `beginAnimation("animate_popup_in", 250,
    // false, true)` — no ease-in, an ease-out, from alpha 0 to 1. `openPopup` runs it for the
    // lobby's choose-class screen and for every popup (`codmenu.lua:1985-1990`).
    assert_eq!(MENU_OPEN_MS, 250.0, "codmenu.lua's animateIn duration");
    let draw = |open: Option<f32>, editor: Option<&Editor>| {
        let mut prims = Vec::new();
        draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor, personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open });
        prims
    };
    let alpha = |prims: &[Prim], want: &str| -> Option<u32> {
        prims.iter().find_map(|q| match q {
            Prim::Text { text, argb, .. } if text == want => Some(*argb >> 24),
            _ => None,
        })
    };
    // The screen starts invisible and lands at its own alpha (the title is off-white, full).
    assert_eq!(alpha(&draw(Some(0.0), None), "CHOOSE CLASS"), Some(0), "opens at alpha 0");
    // Halfway through with easeOut: `(1 - cos(pi/2)) / 2` is exactly 0.5, so a 255 alpha is 127.
    assert_eq!(alpha(&draw(Some(MENU_OPEN_MS / 2000.0), None), "CHOOSE CLASS"), Some(127), "the ease-out midpoint");
    let landed = draw(None, None);
    assert_eq!(alpha(&landed, "CHOOSE CLASS"), Some(255), "settled at full alpha");
    // No time and the full duration both draw exactly the static screen (no fade group, no clip).
    assert_eq!(draw(Some(MENU_OPEN_MS / 1000.0), None), landed, "the landed fade is the static draw");
    // The editor is a menu too, and its picker is a submenu: both fade in the same way.
    let e = Editor::new(Draft::default());
    assert_eq!(alpha(&draw(Some(0.0), Some(&e)), "SAVE CLASS"), Some(0));
    assert_eq!(alpha(&draw(None, Some(&e)), "SAVE CLASS"), Some(255));
    let mut picker = Editor::new(Draft::default());
    picker.slot = Slot::Perk2;
    picker.picking = true;
    assert_eq!(alpha(&draw(Some(0.0), Some(&picker)), "Scavenger"), Some(0), "the submenu opens from 0");
    assert_eq!(alpha(&draw(None, Some(&picker)), "Scavenger"), Some(255));
    crate::profile::reset();
}

#[test]
fn the_focused_button_pulses_like_cod9button() {
    // 650 ms down to 0.5, 650 ms back up, eased (`cod9button.lua` `pulse_low`/`pulse_high`).
    assert_eq!(pulse_alpha(0.0), 1.0);
    assert!((pulse_alpha(0.65) - 0.5).abs() < 1e-4);
    assert!((pulse_alpha(1.3) - 1.0).abs() < 1e-4);
    assert!(pulse_alpha(0.3) > 0.5 && pulse_alpha(0.3) < 1.0);
}

#[test]
fn the_allocation_bar_lights_one_segment_per_point_and_the_counter_says_n_of_ten() {
    for spent in 0..=10 {
        assert_eq!(allocation_lit(spent, 10), spent as usize);
    }
    assert_eq!(allocation_lit(-2, 10), 0, "never negative");
    assert_eq!(allocation_lit(14, 10), 10, "clamped to the budget");
    let mut prims = Vec::new();
    let f = bo2::Frame::new(1280.0, 720.0);
    preview::allocation_bar(&mut prims, &f, crate::lui::Rect { x: 0.0, y: 0.0, w: 200.0, h: 10.0 }, 7, 10);
    let lit = prims
        .iter()
        .filter(|p| matches!(p, Prim::Rect { argb, .. } if *argb == bo2::argb(bo2::BOII_ORANGE, 1.0)))
        .count();
    assert_eq!(lit, 7, "seven of the ten blocks are spent");
}

#[test]
fn the_editor_grid_hit_tests_every_box() {
    let e = Editor::new(Draft::default());
    let l = editor_layout(1280.0, 720.0);
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
        let p = l.frame.px(c.rect);
        let (cx, cy) = (p.x + p.w * 0.5, p.y + p.h * 0.5);
        assert_eq!(editor_cell_at(&l, &cells, cx, cy), Some(i), "{:?} box {i}", c.slot);
    }
    // The attachment boxes are one per attachment the gun may carry.
    let atts = cells.iter().filter(|c| c.slot == Slot::PrimaryAtts).count();
    assert_eq!(atts, e.draft.primary_att_max());
    // A point off the grid hits nothing.
    assert_eq!(editor_cell_at(&l, &cells, 5.0, 5.0), None);
}

/// A box's middle in screen pixels, as a pointer reports it.
fn centre(l: &EditorLayout, r: crate::lui::Rect) -> (f32, f32) {
    let p = l.frame.px(r);
    (p.x + p.w * 0.5, p.y + p.h * 0.5)
}

#[test]
fn the_weapon_picker_hit_tests_the_card_and_the_tiles_it_draws() {
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Primary;
    e.picking = true;
    let l = editor_layout(1280.0, 720.0);
    let cats = weapon_categories(&e);
    let opts = cats[category_of(&cats, e.option)].options.clone();
    let highlighted = opts.iter().position(|i| *i == e.option).expect("the held gun is in its category");
    // BO2's carousel keeps the highlighted gun as its grown card and streams its successors right,
    // so the window starts at the highlight (`cacweaponsp.lua`'s `HighlightedItemWidth`).
    let (first, shown) = picker_window(e.slot, highlighted);
    assert_eq!(first, highlighted, "the carousel's window starts at the highlighted card");
    assert!(shown >= 1, "at least one tile fits");
    let (cx, cy) = centre(&l, carousel_card(&l));
    assert_eq!(picker_option_at(&l, e.slot, cx, cy, first, opts.len()), Some(highlighted), "the card is the highlight");
    for j in 0..shown {
        let i = highlighted + 1 + j;
        if i >= opts.len() {
            break;
        }
        let (tx, ty) = centre(&l, carousel_tile(&l, j));
        assert_eq!(picker_option_at(&l, e.slot, tx, ty, first, opts.len()), Some(i), "tile {j} is the next gun");
    }
    // A point off the carousel hits nothing (the category column below is not an option).
    assert_eq!(picker_option_at(&l, e.slot, 2.0, 2.0, first, opts.len()), None);
}

#[test]
fn an_option_grid_hit_tests_every_card_it_draws() {
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Perk2;
    e.picking = true;
    let l = editor_layout(1280.0, 720.0);
    let opts = e.options();
    assert!(opts.len() > 1, "tier 2 lists its perks");
    // The highlighted cell's row is the window's bottom row (BO2's grids scroll a row at a time).
    for highlighted in [0, 1, opts.len() - 1] {
        let (first, shown) = picker_window(e.slot, highlighted);
        assert_eq!(first % grid_columns(e.slot), 0, "the window starts on a column");
        assert!(highlighted >= first && highlighted < first + shown, "the highlight {highlighted} is in view");
        for j in 0..shown {
            let i = first + j;
            if i >= opts.len() {
                break;
            }
            let (x, y) = centre(&l, grid_cell(&l, e.slot, j));
            assert_eq!(picker_option_at(&l, e.slot, x, y, first, opts.len()), Some(i), "cell {j} maps to its option");
        }
    }
    // A point off the grid hits nothing.
    assert_eq!(picker_option_at(&l, e.slot, 2.0, 2.0, 0, opts.len()), None);
    // Rows walk three at a time, so a highlight on the last row of a full page keeps the page.
    let cols = grid_columns(e.slot);
    assert_eq!(cols, 3, "the perk grid is three wide (`CoD.CACPerksP.GridColumns`)");
}

#[test]
fn the_shared_fold_walks_a_content_grid_with_all_four_arrows() {
    // The perk picker's grid is three wide (`cacperksp.lua:7`). The shared fold passes the arrows
    // to the pure editor now, so the hosts get BO2's ContentGrid links without their own code.
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Perk2;
    e.picking = true;
    e.option = 0;
    let n = e.options().len();
    assert!(n >= 4, "tier 2 has a second row to walk: {n}");
    let mut prev = MenuPrev::default();

    let f = press(&mut e, &mut prev, &[Key::Right]);
    assert_eq!(e.option, 1, "Right stays in the row");
    assert_eq!(f.cue, Some(cues::GRID_NAV), "a grid move plays cac_grid_nav");
    press(&mut e, &mut prev, &[Key::Left]);
    assert_eq!(e.option, 0);
    press(&mut e, &mut prev, &[Key::Left]);
    assert_eq!(e.option, 2, "Left off the row's edge wraps to its last cell");
    press(&mut e, &mut prev, &[Key::Right]);
    assert_eq!(e.option, 0, "Right off the row's edge wraps to its first");
    press(&mut e, &mut prev, &[Key::Down]);
    assert_eq!(e.option, 3, "Down is one row, not the next flat option");
    press(&mut e, &mut prev, &[Key::Up]);
    assert_eq!(e.option, 0);
    assert!(e.picking, "the arrows never close the list");
}

#[test]
fn the_option_grid_labels_the_option_under_each_cell_and_highlights_the_chosen_one() {
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Perk3;
    e.picking = true;
    e.option = e.options().len() - 1;
    let opts = e.options();
    let (first, shown) = picker_window(e.slot, e.option);
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None });
    let t = texts(&prims);
    for name in &opts[first..(first + shown).min(opts.len())] {
        assert!(t.iter().any(|x| x == name), "option {name} is labelled: {t:?}");
    }
    // The highlight is on the selected option's cell, and only there (BOIIOrange, `cod9button.lua`).
    let orange: Vec<String> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, argb, .. } if *argb == bo2::argb(bo2::BOII_ORANGE, 1.0) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(orange, vec![opts[e.option].clone()], "one highlighted cell, on the chosen perk");
}

#[test]
fn the_editor_screens_draw_their_boxes_and_prompts() {
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Perk2;
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None });
    let t = texts(&prims);
    for label in ["PERK 1", "PERK 2", "PERK 3", "LETHAL", "TACTICAL", "ATTACHMENTS", "WILDCARDS"] {
        assert!(t.iter().any(|x| x == label), "editor label {label}: {t:?}");
    }
    assert!(t.iter().any(|x| x == "Remove"), "the R Remove prompt: {t:?}");
    assert!(t.iter().any(|x| x == "Personalize Knife"), "the Tab prompt from b-editor/002: {t:?}");
    assert!(!t.iter().any(|x| x == "Open"), "no invented Enter Open prompt: {t:?}");
    assert!(t.iter().any(|x| x.contains("AN-94")), "the held primary in its box");
    // The allocation readout replaced the old POINTS LEFT counter.
    assert!(!t.iter().any(|x| x == "POINTS LEFT"), "no Pick-10 wording: {t:?}");
    // A perk list shows the tier's grid.
    e.picking = true;
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None });
    let t = texts(&prims);
    assert!(t.iter().any(|x| x == "Scavenger"), "tier 2 perks are listed: {t:?}");
    assert!(t.iter().any(|x| x == "FastHands"), "tier 2 perks are listed");
    assert!(t.iter().any(|x| x == "Back"), "the picker's only prompt from d-attach/002: {t:?}");
    assert!(!t.iter().any(|x| x == "Choose"), "no invented Enter Choose prompt: {t:?}");
}

#[test]
fn a_launcher_shows_attachments_not_available() {
    let d = Draft { secondary: "smaw_mp".into(), ..Draft::default() };
    let e = Editor::new(d);
    let mut prims = Vec::new();
    draw_screen(&mut prims, 1280.0, 720.0, &ScreenView { highlight: 0, editor: Some(&e), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None });
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
fn up_down_walk_the_categories_and_left_right_the_guns_inside_one() {
    let cats = two_categories();
    let mut e = Editor::new(Draft::default());
    e.slot = Slot::Primary;
    e.picking = true;
    e.option = 1;
    // Left/Right stay inside the SMG category and stop at its ends (the edge bounce,
    // `cardcarousel.lua:254-381`).
    assert!(step_within_category(&mut e, &cats, 1));
    assert_eq!(e.option, 2);
    assert!(!step_within_category(&mut e, &cats, 1), "Right at the last card bounces");
    assert_eq!(e.option, 2, "and does not wrap");
    e.option = 0;
    assert!(!step_within_category(&mut e, &cats, -1), "Left at the first card bounces");
    assert_eq!(e.option, 0);
    // Up/Down step the category and wrap across the column (`ScrollList`, `cardcarousellist.lua:723-772`).
    e.option = 1;
    step_category(&mut e, &cats, 1);
    assert_eq!(e.option, 3, "an unvisited category opens on its first gun");
    step_within_category(&mut e, &cats, 1);
    assert_eq!(e.option, 4);
    step_category(&mut e, &cats, 1);
    assert_eq!(e.option, 1, "the last category wraps to the first, back on the card it was left on");
    step_category(&mut e, &cats, -1);
    assert_eq!(e.option, 4, "and each category keeps its own card");
    // Through the fold: Down moves to the next category, Left/Right walk the guns.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Enter]);
    let cats = weapon_categories(&e);
    let before = category_of(&cats, e.option);
    press(&mut e, &mut prev, &[Key::Down]);
    assert_eq!(category_of(&cats, e.option), (before + 1) % cats.len(), "Down walks the categories");
    press(&mut e, &mut prev, &[Key::Up]);
    assert_eq!(category_of(&cats, e.option), before, "and Up back");
    press(&mut e, &mut prev, &[Key::Up]);
    assert_eq!(category_of(&cats, e.option), (before + cats.len() - 1) % cats.len(), "Up wraps");
    press(&mut e, &mut prev, &[Key::Down]);
    // Right never leaves the category, however often it is pressed.
    for _ in 0..12 {
        press(&mut e, &mut prev, &[Key::Right]);
        assert_eq!(category_of(&cats, e.option), before, "Left/Right stay inside the active category");
    }
    let last = *cats[before].options.last().expect("a category has guns");
    assert_eq!(e.option, last, "Right ends on the category's last gun");
    // The wheel is Up/Down (t6mp `FUN_009376a0`): a notch is a category.
    let mut inp = MenuPointer { wheel: 1.0, ..Default::default() };
    editor_frame(&mut e, &inp, &prev);
    assert_eq!(category_of(&cats, e.option), (before + 1) % cats.len(), "a wheel notch down is the next category");
    inp.wheel = -1.0;
    editor_frame(&mut e, &inp, &prev);
    assert_eq!(e.option, last, "a notch up returns, onto the remembered card");
}

#[test]
fn the_carousel_mouse_clicks_to_focus_and_clicks_the_focused_card_to_equip() {
    // `cardcarousel.lua`: the cards ignore hover (`setHandleMouseMove(false)`, :853-857); a click on
    // another card focuses it, a click on the focused card is its action (`CardMouseClick`,
    // :1576-1643).
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Enter]);
    let cats = weapon_categories(&e);
    let opts = cats[category_of(&cats, e.option)].options.clone();
    // Start on the category's first gun so its successor is drawn as the first tile.
    e.option = opts[0];
    assert!(opts.len() > 1, "the held gun's category has more than one gun");
    let l = editor_layout(1280.0, 720.0);
    let (tx, ty) = centre(&l, carousel_tile(&l, 0));
    let hover = MenuPointer { screen_w: 1280.0, screen_h: 720.0, mouse_x: tx, mouse_y: ty, ..Default::default() };
    editor_frame(&mut e, &hover, &prev);
    prev.update(&hover);
    assert_eq!(e.option, opts[0], "hovering a tile does not move the highlight");
    let click = MenuPointer { mouse_buttons: crate::input::mouse_buttons::LEFT, ..hover };
    let f = editor_frame(&mut e, &click, &prev);
    prev.update(&click);
    assert_eq!(e.option, opts[1], "a click on a tile focuses that gun");
    assert!(e.picking, "without equipping it");
    assert_eq!(f.cue, Some(cues::SLIDE_LR), "with the carousel's LR slide (carouselLRSFX)");
    let up = MenuPointer { mouse_buttons: 0, ..hover };
    editor_frame(&mut e, &up, &prev);
    prev.update(&up);
    let want = e.option_weapon(opts[1]).expect("a gun").id.clone();
    let (cx, cy) = centre(&l, carousel_card(&l));
    let on_card = MenuPointer { mouse_x: cx, mouse_y: cy, ..up };
    editor_frame(&mut e, &on_card, &prev);
    prev.update(&on_card);
    let f = editor_frame(&mut e, &MenuPointer { mouse_buttons: crate::input::mouse_buttons::LEFT, ..on_card }, &prev);
    assert!(!e.picking, "a click on the focused card equips it");
    assert_eq!(e.draft.primary, want);
    assert_eq!(f.cue, Some(cues::SLIDE_EQUIP));
}

#[test]
fn a_click_beside_an_open_picker_opens_nothing_else() {
    // The picker owns the screen: the editor's boxes are not drawn, so they are not hit-tested.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    e.slot = Slot::Perk1;
    press(&mut e, &mut prev, &[Key::Enter]);
    assert!(e.picking);
    let l = editor_layout(1280.0, 720.0);
    let (x, y) = centre(&l, l.wildcards[2]);
    let p = MenuPointer { screen_w: 1280.0, screen_h: 720.0, mouse_x: x, mouse_y: y, ..Default::default() };
    editor_frame(&mut e, &p, &prev);
    prev.update(&p);
    let p = MenuPointer { mouse_buttons: crate::input::mouse_buttons::LEFT, ..p };
    editor_frame(&mut e, &p, &prev);
    assert_eq!(e.slot, Slot::Perk1, "still the perk picker");
}

#[test]
fn enter_picks_the_highlighted_gun_of_the_active_category() {
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    press(&mut e, &mut prev, &[Key::Enter]);
    let cats = weapon_categories(&e);
    let active = category_of(&cats, e.option);
    // Walk within the category (from its first gun, so Right always has somewhere to go), then
    // choose.
    e.option = cats[active].options[0];
    press(&mut e, &mut prev, &[Key::Right]);
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

/// Press Down until the editor's slot is `want` (the left column's cycle: PRIMARY, its attachments,
/// SECONDARY, its attachments, card 1, the name), returning the frames' cues.
fn walk_to_slot(e: &mut Editor, prev: &mut MenuPrev, want: Slot) -> Vec<Option<&'static str>> {
    let mut cues = Vec::new();
    for _ in 0..e.slots().len() {
        if e.slot == want {
            break;
        }
        cues.push(press(e, prev, &[Key::Down]).cue);
    }
    assert_eq!(e.slot, want, "the walk reaches {want:?}");
    cues
}

#[test]
fn the_editor_plays_the_luas_cue_at_each_call_site() {
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    // Opening the primary slot is CACEditClass's WeaponChosen: the weapon picker's select cue.
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::WEAPON_SELECT));
    assert!(e.picking);
    // The carousel's moves are the slide cues (`cardcarousellist.lua`), not the grid's. From the
    // category's first gun Right always moves (an end card bounces silently).
    let cats = weapon_categories(&e);
    e.option = cats[category_of(&cats, e.option)].options[0];
    assert_eq!(press(&mut e, &mut prev, &[Key::Right]).cue, Some(cues::SLIDE_LR));
    assert_eq!(press(&mut e, &mut prev, &[Key::Down]).cue, Some(cues::SLIDE_DOWN));
    assert_eq!(press(&mut e, &mut prev, &[Key::Up]).cue, Some(cues::SLIDE_UP));
    // Choosing a gun is the carousel's equip.
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::SLIDE_EQUIP));
    assert!(!e.picking);
    // Backing out of a picker without choosing is the menu's back-out.
    press(&mut e, &mut prev, &[Key::Enter]);
    assert!(e.picking);
    assert_eq!(press(&mut e, &mut prev, &[Key::Escape]).cue, Some(cues::BACKOUT));
    assert!(!e.picking);
    // A content grid's move and choice are the grid's nav and equip (a perk, not a gun).
    e.slot = Slot::Perk1;
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::SUBMENU_SELECT));
    // A grid's cell walk (here Down, a column link) is the grid's nav.
    assert_eq!(press(&mut e, &mut prev, &[Key::Down]).cue, Some(cues::GRID_NAV));
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::GRID_EQUIP));
    // An attachment is `cac_equipment_add` (`attachmentgridbutton.lua`'s actionSFX). A fresh
    // editor: the default draft carries two attachments at 9/10, so the earlier perk left no room.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    e.slot = Slot::PrimaryAtts;
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::SUBMENU_SELECT));
    // The first option is the held Grenade Launcher: choosing it unequips it.
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::EQUIPMENT_ADD));
    // Removing a filled slot is `cac_cmn_item_remove`.
    assert_eq!(press(&mut e, &mut prev, &[Key::R]).cue, Some(cues::ITEM_REMOVE));
    // A refused choice is the deny alone: the host surfaces `denied` (its cue and the rim) and no
    // second cue plays, as BO2's handlers pick denySFX *instead of* the action sound.
    let mut e = Editor::new(Draft { wildcards: vec![Wildcard::Tactician, Wildcard::Overkill], ..Draft::default() });
    let mut prev = MenuPrev::default();
    e.slot = Slot::Save;
    let frame = press(&mut e, &mut prev, &[Key::Enter]);
    assert!(frame.denied, "eleven points will not save");
    assert_eq!(frame.cue, None, "the deny is the only sound");
}

#[test]
fn the_slot_walk_plays_the_loadout_nav_and_the_wildcard_row_its_own() {
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    let cues = walk_to_slot(&mut e, &mut prev, Slot::Wildcards);
    let (last, rest) = cues.split_last().expect("the walk moved");
    assert!(rest.iter().all(|c| *c == Some(cues::LOADOUT_NAV)), "every slot row is cac_loadout_nav: {cues:?}");
    // The strip that lands on the bonus cards has its own focus cue (`CACUtility.WildcardGainFocusSFX`).
    assert_eq!(*last, Some(cues::WILDCARD_NAV));
}

#[test]
fn a_bonus_card_plays_its_own_equip_alias() {
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    e.slot = Slot::Wildcards;
    assert_eq!(press(&mut e, &mut prev, &[Key::Enter]).cue, Some(cues::SUBMENU_SELECT));
    // Walk to a known card and equip it: the alias belongs to that card (`bonuscardbutton.lua`).
    e.option = Wildcard::ALL.iter().position(|w| *w == Wildcard::Overkill).expect("Overkill is a card");
    let frame = press(&mut e, &mut prev, &[Key::Enter]);
    assert_eq!(frame.cue, Some(cues::wildcard_equip(Wildcard::Overkill)), "the equipped card's alias");
    assert!(e.draft.wildcards.contains(&Wildcard::Overkill));
    // Overkill is BO2's third card: `cac_wildcard_equip_3`.
    assert_eq!(frame.cue, Some("ZHBO_UI_cac_wildcard_equip_3"));
}

#[test]
fn a_pickers_title_is_the_luas_heading() {
    let mut e = Editor::new(Draft::default());
    // BO2 titles the weapon screens `PRIMARY`/`SECONDARY` (`MPUI_<slot>_CAPS`), the wildcard screen
    // `MENU_WILDCARDS_CAPS` (menus/cacbonuscards.lua), the others by their slot.
    for (slot, want) in [(Slot::Primary, "PRIMARY"), (Slot::Secondary, "SECONDARY"), (Slot::Wildcards, "WILDCARDS"), (Slot::Perk2, "PERK 2")] {
        e.slot = slot;
        assert_eq!(picker_title(&e), want, "{slot:?}'s picker title");
    }
    // The attachment screen names the gun it edits (`<WEAPON> ATTACHMENTS`).
    e.slot = Slot::PrimaryAtts;
    assert!(picker_title(&e).ends_with(" ATTACHMENTS"), "{}", picker_title(&e));
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
    let d = Draft { primary_atts: vec!["gl".into(), "reflex".into()], ..Draft::default() };
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
    press(&mut e, &mut prev, &[Key::Right]); // Primary -> Perk 1 (`customclass.lua:863-991`)
    assert_eq!(e.slot, Slot::Perk1);
    let frame = press(&mut e, &mut prev, &[Key::R]);
    assert!(frame.removed);
    assert!(e.draft.perks[0].is_none(), "tier 1 is empty");
    assert!(e.draft.perks[1].is_some(), "tier 2 is untouched");
    // R on a bonus card clears that card alone: each card is its own button.
    let d = Draft { primary_atts: vec![], wildcards: vec![Wildcard::Perk1Greed, Wildcard::Overkill], ..Draft::default() };
    let mut e = Editor::new(d);
    let mut prev = MenuPrev::default();
    e.focus_on(Focus::card(1));
    assert!(press(&mut e, &mut prev, &[Key::R]).removed);
    assert_eq!(e.draft.wildcards, vec![Wildcard::Perk1Greed], "only the second card went");
}

#[test]
fn the_card_row_highlights_and_hover_focuses_one_card() {
    let d = Draft { primary_atts: vec![], wildcards: vec![Wildcard::Perk1Greed], ..Draft::default() };
    let mut e = Editor::new(d);
    let mut prev = MenuPrev::default();
    let l = editor_layout(1280.0, 720.0);
    // Hovering the third card focuses that card (hover is `gain_focus`, `luielement.lua:316-348`).
    let (x, y) = centre(&l, l.wildcards[2]);
    let p = MenuPointer { screen_w: 1280.0, screen_h: 720.0, mouse_x: x, mouse_y: y, ..Default::default() };
    let f = editor_frame(&mut e, &p, &prev);
    prev.update(&p);
    assert_eq!(e.focus(), Focus::card(2));
    assert_eq!(f.cue, Some(cues::WILDCARD_NAV));
    let cells = editor_cells(&l, &e);
    let focused: Vec<usize> = cells.iter().filter(|c| cell_focused(&e, c)).map(|c| c.index).collect();
    assert_eq!(focused, vec![2], "only the hovered card is outlined");
    // Left walks the strip, and moving between cards is the strip's own cue.
    assert_eq!(press(&mut e, &mut prev, &[Key::Left]).cue, Some(cues::WILDCARD_NAV));
    assert_eq!(e.focus(), Focus::card(1));
    // The wheel follows the links too: card 2's Up is SECONDARY ATTS.
    // (The cursor rests where `press` left it, off the grid, so no hover competes.)
    let w = MenuPointer { wheel: -1.0, screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    editor_frame(&mut e, &w, &prev);
    assert_eq!(e.slot, Slot::SecondaryAtts);
}

#[test]
fn the_grids_wheel_moves_a_row() {
    // The wheel is LUI's Up/Down: a whole grid row (`contentgrid.lua`'s column links), not one cell.
    let mut e = Editor::new(Draft::default());
    let mut prev = MenuPrev::default();
    e.slot = Slot::Perk3;
    press(&mut e, &mut prev, &[Key::Enter]);
    assert!(e.picking);
    e.option = 1;
    let w = MenuPointer { wheel: 1.0, ..Default::default() };
    editor_frame(&mut e, &w, &prev);
    assert_eq!(e.option, 1 + grid_columns(Slot::Perk3), "a notch down is the cell below");
    editor_frame(&mut e, &MenuPointer { wheel: -1.0, ..Default::default() }, &prev);
    assert_eq!(e.option, 1);
}

#[test]
fn the_circled_x_clears_only_the_selected_filled_box() {
    let d = Draft { primary_atts: vec!["gl".into(), "reflex".into()], ..Draft::default() };
    let mut e = Editor::new(d);
    let l = editor_layout(1280.0, 720.0);
    // Select the attachment strip, then click the second attachment's X.
    e.slot = Slot::PrimaryAtts;
    let cells = editor_cells(&l, &e);
    let second = cells.iter().find(|c| c.slot == Slot::PrimaryAtts && c.index == 1).expect("the second box");
    let r = l.frame.px(remove_rect(second));
    assert_eq!(editor_remove_at(&l, &cells, &e, r.x + r.w * 0.5, r.y + r.h * 0.5), Some((Slot::PrimaryAtts, 1)));
    // The X is not live on an unselected slot (BO2 draws it on the highlighted one).
    e.slot = Slot::Primary;
    assert_eq!(editor_remove_at(&l, &cells, &e, r.x + r.w * 0.5, r.y + r.h * 0.5), None);
    // Clicking it removes exactly that attachment.
    let mut e = Editor::new(Draft { primary: "an94_mp".into(), primary_atts: vec!["gl".into(), "reflex".into()], ..Draft::default() });
    e.slot = Slot::PrimaryAtts;
    let cells = editor_cells(&l, &e);
    let second = cells.iter().find(|c| c.slot == Slot::PrimaryAtts && c.index == 1).cloned().expect("the second box");
    let r = l.frame.px(remove_rect(&second));
    let mut prev = MenuPrev::default();
    let mut pointer = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    pointer.mouse_x = r.x + r.w * 0.5;
    pointer.mouse_y = r.y + r.h * 0.5;
    let _ = editor_frame(&mut e, &pointer, &prev);
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
fn the_attachment_grid_and_lower_panel_follow_the_luas_derived_geometry() {
    // The declared `GridTop = 60` (line 8) is unused; the captures put the selected cell's top
    // border at y≈305 at 3024×1890, i.e. GRID_TOP 80 (`d-attach/002`).
    assert_eq!(GRID_TOP, 80.0);
    assert_eq!(ATTACH_GRID_H, 94.0);
    let l = editor_layout(3024.0, 1890.0);
    let first = l.frame.px(grid_cell(&l, Slot::PrimaryAtts, 0));
    assert!((first.y - 304.5).abs() <= 1.0, "the first cell starts at the captured y≈305: {first:?}");
    assert!((first.w - 360.5).abs() <= 1.0, "GridItemWidth = (864 - 8*5)/6 LUI: {first:?}");
    assert!((first.h - 246.75).abs() <= 1.0, "GridItemHeight = 64 + 20 + 10 LUI: {first:?}");

    // `cacattachmentsmenu.lua:238-247` fixes the lower panel at 83 + 3*94 + 2*8 + 3 + 25 + 5.
    assert_eq!(ATTACH_PANEL_TOP, 414.0);
    let panel = l.frame.px(attachment_panel(&l));
    assert!((panel.y - 1181.25).abs() <= 1.0, "the lower panel starts at the captured y≈1181: {panel:?}");
    let title = l.frame.px(attachment_title(&l));
    assert!((title.y - panel.y).abs() <= 0.01, "the Big title is at the panel's top");
    let stats = l.frame.px(attachment_stats(&l));
    assert!((stats.y - 1464.75).abs() <= 1.0, "the stats start at Big + Default*2 + 10: {stats:?}");
    // The `Equipped` box is `SlotListSize + 10` tall, `Prompt.Height + 30` above the menu bottom.
    let box_ = l.frame.px(attachment_slot_box(&l, 2));
    assert!((box_.w - 441.0).abs() <= 1.0, "two 64-unit dips and 20-unit spacing: {box_:?}");
    assert!((box_.y - 1456.875).abs() <= 1.0, "the box sits 55 LUI above the menu bottom: {box_:?}");
    // The attachment picker shows three rows; the fourth is behind the lower panel.
    assert_eq!(picker_window(Slot::PrimaryAtts, 0).1, 18);
}

#[test]
fn the_attachment_panel_consumes_the_bo2_metadata() {
    // `mp/attachmenttable.csv` row 22 and `mp/attributestable.csv`'s reflex row: the lower
    // title is the full name, the description is the localized description key, and the four
    // bars use the attribute deltas (damage, range, accuracy, mobility, fire rate).
    let reflex = weapons::catalog().attachment("reflex").expect("reflex in the built-in table");
    assert_eq!(reflex.full_name, "Reflex Sight");
    assert_eq!(reflex.description, "Precision red dot sight.");
    assert_eq!(reflex.attribute_deltas, [0.0, 5.0, 15.0, 0.0, 0.0]);
    let base = WeaponStats { damage: 0.5, range: 0.5, fire_rate: 0.5, accuracy: 0.5 };
    let modified = WeaponStats {
        damage: (base.damage + reflex.attribute_deltas[0] / 100.0).clamp(0.0, 1.0),
        range: (base.range + reflex.attribute_deltas[1] / 100.0).clamp(0.0, 1.0),
        accuracy: (base.accuracy + reflex.attribute_deltas[2] / 100.0).clamp(0.0, 1.0),
        fire_rate: (base.fire_rate + reflex.attribute_deltas[4] / 100.0).clamp(0.0, 1.0),
    };
    assert_eq!(modified.range, 0.55, "the reflex's +5 range");
    assert_eq!(modified.accuracy, 0.65, "the reflex's +15 accuracy");
    assert_eq!(modified.damage, base.damage);
    assert_eq!(modified.fire_rate, base.fire_rate);
}

#[test]
fn the_small_popup_uses_bo2s_448_wide_body_and_its_entries_start_at_93() {
    // `codmenu.lua:35-36` and `cacclassoptions.lua:15-18`: 448×256, and 271 for the class dialog.
    let (x, y, w, h) = super::dialog::dialog_box(3024.0, 1890.0, 2.625, DialogKind::SetOptions);
    assert!((w - 448.0 * 2.625).abs() <= 0.01);
    assert!((h - 256.0 * 2.625).abs() <= 0.01);
    assert!((x + w * 0.5 - 1512.0).abs() <= 0.01, "the popup is centred");
    assert!((y + h * 0.5 - 945.0).abs() <= 0.01, "the popup is centred");
    let (_, _, _, class_h) = super::dialog::dialog_box(3024.0, 1890.0, 2.625, DialogKind::ClassOptions);
    assert!((class_h - 271.0 * 2.625).abs() <= 0.01);
    // The list starts at Big + Default + 20 (`cacclasssetoptions.lua:120-134`).
    let d = Dialog::set_options("CLASS SET 1");
    let (_, by, _, _) = super::dialog::dialog_box(3024.0, 1890.0, 2.625, d.kind);
    assert_eq!(super::dialog::dialog_item_at(&d, 3024.0, 1890.0, 2.625, 950.0, by + 94.0 * 2.625), Some(0));
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

#[test]
fn the_personalize_screen_draws_bo2s_grid_and_labels() {
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    let draft = Draft::default();
    let choices: Vec<String> = ["NONE", "GOLD", "DIAMOND", "BEAST", "BENGAL", "BLUE", "CHERRY", "CARBON"].iter().map(|s| s.to_string()).collect();
    let p = Personalize::new(PersonalizeKind::Camo, choices, &draft);
    let mut prims = Vec::new();
    draw_personalize(&mut prims, 1280.0, 720.0, &p, None, 0.0, 0.0);
    let t = texts(&prims);
    assert!(t.iter().any(|s| s == "CAMO"), "the title: {t:?}");
    assert!(t.iter().any(|s| s == "Equipped"), "the Equipped box: {t:?}");
    for name in ["NONE", "GOLD", "DIAMOND", "BEAST", "BENGAL", "BLUE"] {
        assert!(t.iter().any(|s| s == name), "the choice {name:?}: {t:?}");
    }
    assert!(t.iter().any(|s| s == "Back"), "the ESC Back prompt: {t:?}");
    assert!(t.iter().any(|s| s == "Select"), "the ENTER Select prompt: {t:?}");
}

#[test]
fn the_personalize_screen_moves_and_selects_with_arrows() {
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    let mut draft = Draft::default();
    let choices: Vec<String> = ["NONE", "GOLD", "DIAMOND", "BEAST", "BENGAL", "BLUE"].iter().map(|s| s.to_string()).collect();
    let mut p = Personalize::new(PersonalizeKind::Camo, choices, &draft);
    let mut prev = MenuPrev::default();
    let mut inp = MenuPointer { screen_w: 1280.0, screen_h: 720.0, ..Default::default() };
    inp.keys.set_down(Key::Right, true);
    let frame = personalize_frame(&mut p, &mut draft, &inp, &prev);
    assert_eq!(frame.outcome, Outcome::Continue);
    assert_eq!(p.highlight, 1, "Right walks the grid");
    assert_eq!(frame.cue, Some(cues::GRID_NAV));
    prev.update(&inp);
    inp.keys.clear();
    personalize_frame(&mut p, &mut draft, &inp, &prev);
    prev.update(&inp);
    inp.keys.set_down(Key::Enter, true);
    let frame = personalize_frame(&mut p, &mut draft, &inp, &prev);
    assert_eq!(frame.outcome, Outcome::Save);
    assert_eq!(draft.camo, "GOLD", "Enter equips the highlighted cell");
    assert_eq!(frame.cue, Some(cues::GRID_EQUIP));
    let mut prims = Vec::new();
    draw_personalize(&mut prims, 1280.0, 720.0, &p, None, 0.0, 0.0);
    assert!(texts(&prims).iter().any(|s| s == "GOLD"));
}

#[test]
fn a_dialog_takes_hover_focus_and_the_wheel() {
    // The options dialogs are `CoD.ButtonList`s: a moving cursor focuses an entry and the wheel is
    // Up/Down, wrapping like the arrows (`cacclassoptions.lua:157`).
    let mut d = Dialog::class_options("Custom 5");
    let prev = MenuPrev::default();
    let s = 1.0;
    let (x, y) = (0..720)
        .map(|y| (640.0, y as f32))
        .find(|(x, y)| dialog_item_at(&d, 1280.0, 720.0, s, *x, *y) == Some(2))
        .expect("the third entry is on screen");
    let hover = MenuPointer { screen_w: 1280.0, screen_h: 720.0, mouse_x: x, mouse_y: y, ..Default::default() };
    dialog_step(&mut d, &prev, &hover);
    assert_eq!(d.highlight, 2, "hover focuses the entry under the cursor");
    let mut at_rest = prev;
    at_rest.update(&hover);
    let wheel = MenuPointer { wheel: 1.0, ..hover };
    dialog_step(&mut d, &at_rest, &wheel);
    assert_eq!(d.highlight, 3, "a notch down is the next entry");
    dialog_step(&mut d, &at_rest, &wheel);
    assert_eq!(d.highlight, 0, "and wraps");
}

#[test]
fn the_personalize_wheel_moves_a_row() {
    let mut draft = Draft::default();
    let choices: Vec<String> = (0..15).map(|i| format!("C{i}")).collect();
    let mut p = Personalize::new(PersonalizeKind::Camo, choices, &draft);
    let prev = MenuPrev::default();
    let w = MenuPointer { wheel: 1.0, ..Default::default() };
    personalize_frame(&mut p, &mut draft, &w, &prev);
    assert_eq!(p.highlight, PersonalizeKind::Camo.columns(), "a notch down is the cell below (6 wide)");
    let w = MenuPointer { wheel: -1.0, ..Default::default() };
    personalize_frame(&mut p, &mut draft, &w, &prev);
    assert_eq!(p.highlight, 0);
}
