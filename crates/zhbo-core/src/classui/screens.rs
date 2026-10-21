//! Drawing the screens: the pick list with its kit details, the editor grid and the two pickers.

use super::*;

/// What a host shows on the class screen this frame: the pick list (and its highlighted row) or
/// the open editor, plus the shared deny-rim counter. The in-game `Client` and the shell
/// `MenuState` both produce one; the drawing below never sees the hosts themselves.
pub struct ScreenView<'a> {
    /// The pick list's highlighted row (unused while the editor is up).
    pub highlight: usize,
    /// The open allocation editor, if any (shell only; a match is pick-only).
    pub editor: Option<&'a Editor>,
    /// Seconds left of the deny rim (a refused choice).
    pub deny: f32,
    /// The shell screen: CREATE A CLASS row, set/class options. False in a match.
    pub manage: bool,
}

/// Draw the class screen for a host: the editor while one is open, else the pick list.
pub fn draw_screen(prims: &mut Vec<Prim>, w: f32, h: f32, view: &ScreenView) {
    let s = (h / 720.0).max(0.5);
    match view.editor {
        Some(editor) => draw_editor(prims, w, h, s, editor, view.deny),
        None => draw_pick(prims, w, h, s, view.highlight, view.deny, view.manage),
    }
}

// ------------------------------------------------------------------------------------------------
// The pick screen
// ------------------------------------------------------------------------------------------------

/// BO2's choose-class screen: the heading and the class-set pager across the top, the list of
/// presets, the player's customs and (in the shell) CREATE A CLASS down the left with BO2's hint
/// under it, the highlighted class's kit on the right, the prompts at the bottom.
/// `highlighted` is the host's local row (the in-game client's or the shell menu's); the keys and
/// mouse drive it through [`pick_frame`]. `manage` selects the shell's list and footer actions.
pub fn draw_pick(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, highlighted: usize, deny: f32, manage: bool) {
    background(prims, w, h);
    let l = pick_layout(w, h, s);

    // Snapshot both registries once: the rows, the pager and the details panel must describe the
    // same lists.
    let presets = classes::get();
    let prof = profile::get();
    let customs: &[crate::classes::CustomClass] = prof.customs();
    let list = PickList {
        rows: pick_rows_for(presets.len(), customs.len(), manage),
        set: prof.active_set(),
        sets: profile::MAX_SETS,
        set_name: prof.set_name(prof.active_set()),
    };
    let rows = &list.rows;
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    heading(prims, title_x, l.title_y, s, "CHOOSE CLASS", WHITE);
    if rows.is_empty() {
        // A match with no registry and no customs: draw the chrome, never index an empty list.
        footer_prompts(prims, w, s, l.footer_y, &[Prompt { key: "ESC", label: "Close" }], &[]);
        deny_flash(prims, w, h, s, deny);
        return;
    }
    let active = highlighted.min(rows.len() - 1);

    // The class-set pager over the list (`classsetpager` in BO2): `‹ CLASS SET n ›` with the ten-dot
    // marker under it. The shell steps it with Left/Right; a match shows the active set read-only.
    set_pager(prims, s, l.list_x + l.list_w * 0.5, l.pager_y, &list, manage);
    // The highlighted class's allocation, top right in the heading, with BO2's segmented bar under
    // it (the same readout the editor carries).
    let spent = class_allocation(&rows[active], &presets, customs);
    let (bx, by, bw) = allocation_bar_rect(&l, s);
    draw_allocation_counter(prims, bx + bw, l.title_y + 12.0 * s, s, spent, crate::classmenu::PICK_10);
    draw_allocation(prims, bx, by, bw, s, spent, crate::classmenu::PICK_10);

    panel(prims, l.list_x - 8.0 * s, l.list_y - 8.0 * s, l.list_w + 16.0 * s, l.list_h + 16.0 * s, s);
    let visible = pick_visible(&l);
    let first = pick_first(active, visible);
    for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
        let y = l.list_y + (i - first) as f32 * l.row_h;
        let on = i == active;
        row(prims, l.list_x, y, l.list_w, l.row_h - 6.0 * s, s, on);
        // BO2's slot numeral (1-10 by key), or a plus for the new-class row.
        let (name, primary, primary_id) = match *r {
            PickRow::Preset(j) => {
                let c = &presets[j];
                let kit = classes::loadout_of(c);
                (c.name.clone(), weapons::weapon(&kit.primary).display_name.clone(), kit.primary.clone())
            }
            PickRow::Custom(j) => {
                let c = &customs[j];
                (c.name.clone(), weapons::weapon(&c.primary).display_name.clone(), c.primary.clone())
            }
            PickRow::Create => ("CREATE A CLASS".to_string(), "Ten points of allocation".to_string(), String::new()),
        };
        if matches!(r, PickRow::Create) {
            prims.push(Prim::Text { x: l.list_x + 22.0 * s, y: y + 4.0 * s, text: "+".to_string(), argb: GOLD, size: (22.0 * s) as i32, align: 0 });
        } else {
            hud::draw_number(prims, i as u32 + 1, l.list_x + 44.0 * s, y + 2.0 * s, 24.0 * s, if on { GOLD } else { WHITE });
        }
        let tx = l.list_x + 62.0 * s;
        prims.push(Prim::Text { x: tx, y: y + 3.0 * s, text: name, argb: if on { ORANGE } else { WHITE }, size: (14.0 * s) as i32, align: 0 });
        let icon_x = l.list_x + l.list_w - 48.0 * s;
        let icon_y = y + 3.0 * s;
        if primary_id.is_empty() {
            prims.push(Prim::Text { x: icon_x - 4.0 * s, y: y + 17.0 * s, text: primary, argb: DIM, size: (10.0 * s) as i32, align: 2 });
        } else {
            weapon_icon(prims, &primary_id, icon_x, icon_y, 24.0 * s, if on { WHITE } else { DIM }, &primary);
            prims.push(Prim::Text { x: icon_x - 8.0 * s, y: y + 17.0 * s, text: primary, argb: DIM, size: (10.0 * s) as i32, align: 2 });
        }
    }

    if rows.len() > visible {
        prims.push(Prim::Text {
            x: l.list_x + l.list_w - 6.0 * s,
            y: l.list_y + l.list_h + 2.0 * s,
            text: format!("{} / {}", active + 1, rows.len()),
            argb: DIM,
            size: (10.0 * s) as i32,
            align: 2,
        });
    }
    if manage {
        // BO2's hint under the class list (a-chooseclass/001). It is the shell's alone: a match is
        // already running, so the set is not what the fight loads into.
        chevron(prims, l.list_x + 6.0 * s, l.hint_y + 3.0 * s, 6.0 * s, ORANGE, 1);
        prims.push(Prim::Text {
            x: l.list_x + 20.0 * s,
            y: l.hint_y,
            text: "You will load into the game with this class set.".to_string(),
            argb: DIM,
            size: (12.0 * s) as i32,
            align: 0,
        });
    }
    draw_pick_details(prims, &l, s, &rows[active], &presets, customs);

    if manage {
        // BO2's choose-class prompt strip: `ESC Back` left, `S Class Set Options` and
        // `O Class Options` right (each clickable).
        let actions = [
            PickFooterAction::Back.prompt(),
            PickFooterAction::SetOptions.prompt(),
            PickFooterAction::ClassOptions.prompt(),
        ];
        footer_prompts(prims, w, s, l.footer_y, &actions[..1], &actions[1..]);
    } else {
        // In a match the screen is pick-only: Escape closes it and there is no management.
        footer_prompts(
            prims,
            w,
            s,
            l.footer_y,
            &[Prompt { key: "1-9", label: "Choose" }, Prompt { key: "ENTER", label: "Select" }],
            &[Prompt { key: "ESC", label: "Close" }],
        );
    }
    deny_flash(prims, w, h, s, deny);
}

/// How many of the ten points the highlighted row's class spends (BO2's counter beside the pager).
pub(super) fn class_allocation(row: &PickRow, presets: &[classes::ClassDef], customs: &[crate::classes::CustomClass]) -> i32 {
    match *row {
        PickRow::Preset(j) => {
            let stored = crate::classes::CustomClass::from_spec(&presets[j].spec);
            Draft::from_stored(&stored).spent()
        }
        PickRow::Custom(j) => Draft::from_stored(&customs[j]).spent(),
        PickRow::Create => 0,
    }
}

/// BO2's class-set pager: `‹ CLASS SET n ›` with a ten-dot bar under it marking the position within
/// the ten sets (`a-chooseclass/001`..`/010`). In the shell the chevrons are lit (Left/Right step
/// the sets); in a match they are dim, because the set is picked in the lobby.
pub(super) fn set_pager(prims: &mut Vec<Prim>, s: f32, cx: f32, y: f32, list: &PickList, manage: bool) {
    let col = if manage { ORANGE } else { DIM };
    let chev = 11.0 * s;
    chevron(prims, cx - 74.0 * s, y + 6.0 * s, chev, col, -1);
    chevron(prims, cx + 74.0 * s - chev * 0.6, y + 6.0 * s, chev, col, 1);
    prims.push(Prim::Text {
        x: cx,
        y: y + 2.0 * s,
        text: list.set_name.clone(),
        argb: WHITE,
        size: (13.0 * s) as i32,
        align: 1,
    });
    // The ten-dot set marker, BO2's diamond on the active set.
    let n = list.sets.max(1);
    let step = 15.0 * s;
    let left = cx - step * (n as f32 - 1.0) * 0.5;
    let my = y + 23.0 * s;
    for i in 0..n {
        let mx = left + i as f32 * step;
        if i == list.marker() {
            diamond(prims, mx, my, 5.0 * s, ORANGE);
        } else {
            prims.push(Prim::Rect { x: mx - 1.5 * s, y: my - 1.5 * s, w: 3.0 * s, h: 3.0 * s, argb: DIM });
        }
    }
}

/// A chevron (`‹`/`›`) drawn as two strokes: `dir` −1 points left, +1 right. `x, y` is its top-left.
pub(super) fn chevron(prims: &mut Vec<Prim>, x: f32, y: f32, size: f32, argb: u32, dir: i32) {
    let mid = y + size * 0.5;
    let (x0, x1) = if dir < 0 { (x + size * 0.5, x) } else { (x, x + size * 0.5) };
    prims.push(Prim::Line { x0, y0: y, x1, y1: mid, width: 2.0, argb });
    prims.push(Prim::Line { x0: x1, y0: mid, x1: x0, y1: y + size, width: 2.0, argb });
}

/// A filled diamond (the pager's active-set marker, BO2's little orange lozenge).
pub(super) fn diamond(prims: &mut Vec<Prim>, cx: f32, cy: f32, r: f32, argb: u32) {
    for i in 0..(r * 2.0).max(1.0) as i32 {
        let f = i as f32 / (r * 2.0).max(1.0);
        let half = r * (1.0 - (f * 2.0 - 1.0).abs());
        prims.push(Prim::Rect { x: cx - half, y: cy - r + i as f32, w: half * 2.0, h: 1.0, argb });
    }
}

/// BO2's circled `X` remove affordance, on the selected box's top-right corner (`b-editor/028`).
pub(super) fn remove_glyph(prims: &mut Vec<Prim>, cell: &EditorCell, s: f32) {
    let (x, y, d, _) = remove_rect(cell, s);
    let (cx, cy) = (x + d * 0.5, y + d * 0.5);
    let r = d * 0.5;
    let seg = 12;
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
        prims.push(Prim::Line {
            x0: cx + r * a0.cos(),
            y0: cy + r * a0.sin(),
            x1: cx + r * a1.cos(),
            y1: cy + r * a1.sin(),
            width: 1.5,
            argb: WHITE,
        });
    }
    let k = r * 0.42;
    prims.push(Prim::Line { x0: cx - k, y0: cy - k, x1: cx + k, y1: cy + k, width: 1.8, argb: WHITE });
    prims.push(Prim::Line { x0: cx - k, y0: cy + k, x1: cx + k, y1: cy - k, width: 1.8, argb: WHITE });
}

/// The highlighted class's kit on the right, in BO2's order (`a-chooseclass/001`): PRIMARY,
/// SECONDARY, PERK 1, PERK 2, PERK 3, LETHAL, TACTICAL, BONUS CARDS.
pub(super) fn draw_pick_details(prims: &mut Vec<Prim>, l: &PickLayout, s: f32, row: &PickRow, presets: &[classes::ClassDef], customs: &[crate::classes::CustomClass]) {
    let x = l.panel_x + 14.0 * s;
    let w = l.panel_w - 28.0 * s;
    panel(prims, l.panel_x, l.panel_y, l.panel_w, l.panel_h, s);
    let mut y = l.panel_y + 12.0 * s;
    if matches!(*row, PickRow::Create) {
        prims.push(Prim::Text { x, y, text: "CREATE A CLASS".to_string(), argb: ORANGE, size: (18.0 * s) as i32, align: 0 });
        y += 30.0 * s;
        for line in ["Name it, pick ten points of kit,", "then save it to your profile.", "", "It joins this list and can be", "picked again in any match."] {
            prims.push(Prim::Text { x, y, text: (*line).to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
            y += 18.0 * s;
        }
        return;
    }
    let (primary, secondary, lethal, tactical, perks, wildcards) = match *row {
        PickRow::Preset(j) => {
            let kit = classes::loadout_of(&presets[j]);
            (kit.primary.clone(), kit.secondary.clone(), kit.lethal.clone(), kit.tactical.clone(), kit.perks.clone(), Vec::new())
        }
        PickRow::Custom(j) => {
            let c = &customs[j];
            // The repaired draft: the details panel shows the class the game would actually apply.
            let draft = Draft::from_stored(c);
            (draft.primary_id(), draft.secondary_id(), draft.lethal.clone(), draft.tactical.clone(), draft.perks_in_order(), draft.wildcards.clone())
        }
        PickRow::Create => unreachable!("handled above"),
    };
    // The rows' spacing follows the panel's height, so the kit spreads down it as BO2's does.
    let unit = l.panel_h / 15.0;
    // The two weapons as BO2's icon-and-name blocks.
    for (label, id) in [("PRIMARY", primary.as_str()), ("SECONDARY", secondary.as_str())] {
        let def = weapons::weapon(id);
        let size = 64.0 * s;
        weapon_icon(prims, &def.id, x, y, size, WHITE, &def.display_name);
        prims.push(Prim::Text { x: x + size + 12.0 * s, y: y + 2.0 * s, text: label.to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        prims.push(Prim::Text { x: x + size + 12.0 * s, y: y + 15.0 * s, text: def.display_name.clone(), argb: WHITE, size: (15.0 * s) as i32, align: 0 });
        // The weapon's attachments as their own little icons on the right.
        let mut ax = x + w - 22.0 * s;
        for short in def.attachments.iter().rev() {
            icon(prims, sprites::attachment_icon(short), ax, y + 8.0 * s, 20.0 * s, DIM, short);
            ax -= 24.0 * s;
        }
        y += (size + unit).max(unit * 2.0);
    }
    // PERK 1-3: each tier's pick, with the Greed rows' second pick beside it.
    for tier in 1..=3u8 {
        let picks: Vec<&crate::weapons::Perk> = perks.iter().filter(|p| p.def().tier == tier).collect();
        let size = 28.0 * s;
        let mut px = x;
        for perk in &picks {
            icon(prims, Some(sprites::perk_icon(**perk)), px, y, size, WHITE, perk.def().id);
            px += size + 4.0 * s;
        }
        prims.push(Prim::Text { x: px + 6.0 * s, y: y + 2.0 * s, text: format!("PERK {tier}"), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        let names: Vec<&str> = picks.iter().map(|p| p.def().id).collect();
        prims.push(Prim::Text { x: px + 6.0 * s, y: y + 15.0 * s, text: if names.is_empty() { "—".into() } else { names.join(", ") }, argb: WHITE, size: (12.0 * s) as i32, align: 0 });
        y += size + unit * 0.5;
    }
    for (label, id) in [("LETHAL", lethal.as_str()), ("TACTICAL", tactical.as_str())] {
        let def = weapons::weapon(id);
        let size = 28.0 * s;
        weapon_icon(prims, &def.id, x, y, size, WHITE, &def.display_name);
        prims.push(Prim::Text { x: x + size + 8.0 * s, y: y + 2.0 * s, text: label.to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        prims.push(Prim::Text { x: x + size + 8.0 * s, y: y + 15.0 * s, text: def.display_name.clone(), argb: WHITE, size: (12.0 * s) as i32, align: 0 });
        y += size + unit * 0.5;
    }
    if !wildcards.is_empty() {
        prims.push(Prim::Text { x, y, text: "BONUS CARDS".to_string(), argb: DIM, size: (10.0 * s) as i32, align: 0 });
        y += 15.0 * s;
        for wildcard in &wildcards {
            icon(prims, Some(sprites::wildcard_icon(*wildcard)), x, y, 18.0 * s, WHITE, wildcard.name());
            prims.push(Prim::Text { x: x + 24.0 * s, y: y + 2.0 * s, text: wildcard.name().to_string(), argb: DIM, size: (11.0 * s) as i32, align: 0 });
            y += 21.0 * s;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The editor
// ------------------------------------------------------------------------------------------------

/// The highlighted weapon whose bars the editor shows: the option being hovered while a list is
/// open, else the slot's own gun.
pub(super) fn editor_stats_weapon(editor: &Editor) -> Option<&'static WeaponDef> {
    let def = if editor.picking {
        editor.option_weapon(editor.option)?
    } else {
        match editor.slot {
            Slot::Primary => weapons::weapon(&editor.draft.primary),
            Slot::Secondary => weapons::weapon(&editor.draft.secondary),
            _ => return None,
        }
    };
    def.class.is_gun().then_some(def)
}

/// The icon of a box's current value.
pub(super) fn cell_icon(editor: &Editor, cell: &EditorCell) -> Option<String> {
    let d = &editor.draft;
    match cell.slot {
        Slot::Primary => (!d.primary.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.primary))),
        Slot::Secondary => (!d.secondary.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.secondary))),
        Slot::PrimaryAtts => d.primary_atts.get(cell.index).and_then(|a| sprites::attachment_icon(a)).map(str::to_string),
        Slot::SecondaryAtts => d.secondary_atts.get(cell.index).and_then(|a| sprites::attachment_icon(a)).map(str::to_string),
        Slot::Lethal => (!d.lethal.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.lethal))),
        Slot::Tactical => (!d.tactical.is_empty()).then(|| format!("ZHBO_BO2_Icon_{}", base_id(&d.tactical))),
        Slot::Perk1 | Slot::Perk2 | Slot::Perk3 => {
            let tier = (cell.slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.perks[tier].map(|p| sprites::perk_icon(p).to_string())
        }
        Slot::Perk1b | Slot::Perk2b | Slot::Perk3b => {
            let tier = (cell.slot.perk_tier().unwrap_or(1) as usize) - 1;
            d.extra_perks[tier].map(|p| sprites::perk_icon(p).to_string())
        }
        Slot::Wildcards => d.wildcards.get(cell.index).map(|w| sprites::wildcard_icon(*w).to_string()),
        Slot::Name | Slot::Save => None,
    }
}

/// What a box shows as its value.
pub(super) fn cell_value(editor: &Editor, cell: &EditorCell) -> String {
    let d = &editor.draft;
    match cell.slot {
        Slot::PrimaryAtts => d.primary_atts.get(cell.index).map(|s| att_display(s)).unwrap_or_default(),
        Slot::SecondaryAtts => d.secondary_atts.get(cell.index).map(|s| att_display(s)).unwrap_or_default(),
        Slot::Wildcards => d.wildcards.get(cell.index).map(|w| w.name().to_string()).unwrap_or_default(),
        other => crate::classmenu::slot_value(d, other),
    }
}

/// An attachment short as the catalog names it.
pub(super) fn att_display(short: &str) -> String {
    let d = weapons::catalog();
    d.all()
        .iter()
        .find_map(|w| w.allowed.iter().find(|(s, _)| s == short).map(|(_, asset)| asset.clone()))
        .and_then(|asset| d.attachment(&asset).map(|a| a.display_name.clone()))
        .unwrap_or_else(|| short.to_string())
}

/// Whether a weapon offers any attachments at all (BO2's "Attachments not available", `b-editor/002`).
pub(super) fn offers_attachments(id: &str) -> bool {
    !id.is_empty() && !weapons::weapon(id).allowed.is_empty()
}

/// BO2's allocation editor: the class name as the heading with the allocation bar under it, the
/// boxed grid down the screen, and the prompts at the bottom. A weapon picker or an option list
/// takes the screen over while one is open, as BO2's do.
pub fn draw_editor(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, editor: &Editor, deny: f32) {
    background(prims, w, h);
    let l = editor_layout(w, h, s);
    match editor.slot {
        Slot::Primary | Slot::Secondary if editor.picking => draw_weapon_picker(prims, w, h, s, &l, editor),
        _ if editor.picking => draw_option_picker(prims, w, h, s, &l, editor),
        _ => draw_editor_grid(prims, w, h, s, &l, editor),
    }
    deny_flash(prims, w, h, s, deny);
}

/// The editor's boxed grid (`b-editor/001`, `/028`).
pub(super) fn draw_editor_grid(prims: &mut Vec<Prim>, w: f32, _h: f32, s: f32, l: &EditorLayout, editor: &Editor) {
    let d = &editor.draft;
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    let name = if d.name.trim().is_empty() { "CREATE A CLASS".to_string() } else { d.name.clone() };
    let name_on = editor.slot == Slot::Name;
    prims.push(Prim::Text {
        x: title_x,
        y: l.title_y + 10.0 * s,
        text: if editor.naming { format!("{name}_") } else { name },
        argb: if name_on { ORANGE } else { WHITE },
        size: (24.0 * s) as i32,
        align: 0,
    });
    // The allocation readout, BO2's bar across the content's width with the n/10 counter above its
    // right end.
    draw_allocation_counter(prims, l.content_x + l.content_w, l.title_y + 12.0 * s, s, d.spent(), crate::classmenu::PICK_10);
    draw_allocation(prims, l.content_x, l.bar_y, l.content_w, s, d.spent(), crate::classmenu::PICK_10);

    // The group labels BO2 sets beside the box rows. A weapon that offers no attachments says so in
    // place of the label and boxes (b-editor/002).
    if offers_attachments(&d.primary) {
        group_label(prims, l.left_x, l.atts_y + 12.0 * s, s, "ATTACHMENTS");
    } else {
        prims.push(Prim::Text { x: l.left_x + 12.0 * s, y: l.atts_y + 12.0 * s, text: "Attachments not available".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
    }
    if offers_attachments(&d.secondary) {
        group_label(prims, l.left_x, l.atts2_y + 12.0 * s, s, "ATTACHMENTS");
    } else {
        prims.push(Prim::Text { x: l.left_x + 12.0 * s, y: l.atts2_y + 12.0 * s, text: "Attachments not available".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
    }
    group_label(prims, l.left_x, l.wild_y - 18.0 * s, s, "BONUS CARDS");
    for tier in 1..=3u8 {
        let y = l.top + (tier as f32 - 1.0) * (l.perk_h + l.gap);
        group_label(prims, l.right_x, y + 14.0 * s, s, if tier == 1 { "PERK 1" } else if tier == 2 { "PERK 2" } else { "PERK 3" });
        let pick = d.perks[(tier as usize) - 1].map(|p| p.def().id.to_string());
        prims.push(Prim::Text {
            x: l.right_x,
            y: y + 30.0 * s,
            text: pick.unwrap_or_else(|| "—".into()),
            argb: if d.perks[(tier as usize) - 1].is_some() { WHITE } else { DIM },
            size: (13.0 * s) as i32,
            align: 0,
        });
    }
    group_label(prims, l.right_x, l.lethal_y + 14.0 * s, s, "LETHAL");
    prims.push(Prim::Text {
        x: l.right_x,
        y: l.lethal_y + 30.0 * s,
        text: if d.lethal.is_empty() { "—".to_string() } else { weapons::weapon(&d.lethal).display_name.clone() },
        argb: if d.lethal.is_empty() { DIM } else { WHITE },
        size: (13.0 * s) as i32,
        align: 0,
    });
    let tac_y = l.tactical_y + 14.0 * s;
    group_label(prims, l.right_x, tac_y, s, "TACTICAL");
    prims.push(Prim::Text {
        x: l.right_x,
        y: tac_y + 16.0 * s,
        text: if d.tactical.is_empty() { "—".to_string() } else { weapons::weapon(&d.tactical).display_name.clone() },
        argb: if d.tactical.is_empty() { DIM } else { WHITE },
        size: (13.0 * s) as i32,
        align: 0,
    });

    let cells = editor_cells(l, editor);
    for cell in &cells {
        draw_cell(prims, s, editor, cell);
    }

    let mut left = vec![Prompt { key: "ESC", label: "Back" }, Prompt { key: "ENTER", label: "Open" }];
    if editor.naming {
        left = vec![Prompt { key: "TYPE", label: "Name" }, Prompt { key: "BKSP", label: "Delete" }, Prompt { key: "ENTER", label: "Done" }];
    }
    // BO2 shows `R Remove` only while the highlighted slot holds something (`b-editor/002` has
    // none, `/028` has it).
    let removable = cells.iter().any(|c| c.slot == editor.slot && cell_filled(editor, c));
    let right: &[Prompt] = if removable && !editor.naming { &[Prompt { key: "R", label: "Remove" }] } else { &[] };
    footer_prompts(prims, w, s, l.footer_y, &left, right);
}

/// A small group label beside the box rows (BO2's grey caps).
pub(super) fn group_label(prims: &mut Vec<Prim>, x: f32, y: f32, s: f32, text: &str) {
    prims.push(Prim::Text { x, y, text: text.to_string(), argb: DIM, size: (11.0 * s) as i32, align: 0 });
}

/// Whether a box itself draws its value. BO2 writes the weapons' and bonus cards' names inside
/// their boxes (`MP7` in the primary box, the card names in the strip), while the perk and offhand
/// rows read beside their icon (`b-editor/028`) and the heading already is the class name.
pub(super) fn cell_draws_value(slot: Slot) -> bool {
    matches!(slot, Slot::Primary | Slot::Secondary | Slot::PrimaryAtts | Slot::SecondaryAtts | Slot::Wildcards)
}

/// One boxed cell: BO2's tile with the value and the icon, an orange outline and the circled X when
/// it is the highlighted slot.
pub(super) fn draw_cell(prims: &mut Vec<Prim>, s: f32, editor: &Editor, cell: &EditorCell) {
    let on = cell.slot == editor.slot;
    let filled = cell_filled(editor, cell);
    // BO2's boxed tiles: a dark plate with a thin border, orange when the box is selected. The
    // converted TILE art is a small framed tile that smears when stretched to a big box.
    prims.push(Prim::Rect { x: cell.x, y: cell.y, w: cell.w, h: cell.h, argb: CELL_FILL });
    hud::outline_rect(prims, cell.x, cell.y, cell.w, cell.h, if on { 2.0 } else { 1.0 } * s, if on { ORANGE } else { CELL_EDGE });
    if cell.slot == Slot::Save {
        prims.push(Prim::Text {
            x: cell.x + cell.w * 0.5,
            y: cell.y + cell.h * 0.28,
            text: "SAVE CLASS".to_string(),
            argb: if on { ORANGE } else { WHITE },
            size: (13.0 * s) as i32,
            align: 1,
        });
        return;
    }
    let icon_size = (cell.h * 0.62).min(cell.w * 0.55);
    if let Some(art) = cell_icon(editor, cell) {
        let value = cell_value(editor, cell);
        icon(prims, Some(&art), cell.x + cell.w * 0.5 - icon_size * 0.5 - cell.w * 0.12, cell.y + (cell.h - icon_size) * 0.4, icon_size, if on { WHITE } else { DIM }, &value);
    }
    if !filled {
        return;
    }
    if cell_draws_value(cell.slot) {
        let value = cell_value(editor, cell);
        let size = if cell.h > 60.0 * s { 15.0 * s } else { 11.0 * s };
        prims.push(Prim::Text {
            x: cell.x + 8.0 * s,
            y: cell.y + cell.h - size - 6.0 * s,
            text: value,
            argb: if on { ORANGE } else { WHITE },
            size: size as i32,
            align: 0,
        });
    }
    if on {
        remove_glyph(prims, cell, s);
    }
}

/// BO2's weapon screen (`c-primary/001`, `d-attach/037`): the slot's title, the allocation readout,
/// the highlighted gun's segmented bars, the category column at the bottom left with the active
/// category on top, and the category's guns as tiles on the right.
pub(super) fn draw_weapon_picker(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, l: &EditorLayout, editor: &Editor) {
    let d = &editor.draft;
    let slot = editor.slot;
    let title = if slot == Slot::Secondary { "SECONDARY" } else { "PRIMARY" };
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    heading(prims, title_x, l.title_y, s, title, WHITE);
    draw_allocation_counter(prims, l.content_x + l.content_w, l.title_y + 12.0 * s, s, d.spent(), crate::classmenu::PICK_10);
    draw_allocation(prims, l.content_x, l.bar_y, l.content_w, s, d.spent(), crate::classmenu::PICK_10);

    let cats = weapon_categories(editor);
    let active = category_of(&cats, editor.option);
    // The highlighted gun's name and segmented bars, BO2's left column.
    if let Some(def) = editor.option_weapon(editor.option) {
        prims.push(Prim::Text { x: l.left_x, y: l.top + 6.0 * s, text: def.display_name.clone(), argb: if def.class.is_gun() { WHITE } else { ORANGE }, size: (26.0 * s) as i32, align: 0 });
        weapon_icon(prims, &def.id, l.left_x, l.top + 44.0 * s, 96.0 * s, WHITE, &def.display_name);
        if def.class.is_gun() {
            stat_bars(prims, l.left_x, l.top + 152.0 * s, l.left_w, s, &weapon_stats(def));
        }
    }
    // BO2's category column: the active category orange on top, the rest following it, with its rule
    // under the active entry.
    let cat_y = l.footer_y - 40.0 * s - cats.len() as f32 * 26.0 * s;
    let column = category_column(&cats, active);
    for (i, ci) in column.iter().enumerate() {
        let cat = &cats[*ci];
        let y = cat_y + i as f32 * 26.0 * s;
        let on = *ci == active;
        prims.push(Prim::Text {
            x: l.left_x,
            y,
            text: cat.name.to_string(),
            argb: if on { ORANGE } else { WHITE },
            size: if on { (17.0 * s) as i32 } else { (13.0 * s) as i32 },
            align: 0,
        });
        if on {
            prims.push(Prim::Rect { x: l.left_x, y: y + 22.0 * s, w: l.left_w * 0.9, h: 1.0 * s, argb: 0x60FF_FFFF });
        }
    }
    // The category's guns as tiles, the highlighted one outlined in orange.
    let (px, py, pw, _) = picker_rect(l, slot);
    let opts = cats.get(active).map(|c| c.options.clone()).unwrap_or_default();
    let highlighted = opts.iter().position(|i| *i == editor.option).unwrap_or(0);
    let (first, shown) = picker_window(l, slot, highlighted);
    for (row_i, opt) in opts.iter().enumerate().skip(first).take(shown) {
        let y = py + (row_i - first) as f32 * l.row_h;
        let on = *opt == editor.option;
        let Some(def) = editor.option_weapon(*opt) else { continue };
        tile(prims, s, px, y, pw, l.row_h - 6.0 * s, on, def.id.as_str(), &def.display_name, editor.option_chosen(*opt));
    }
    if opts.len() > shown {
        prims.push(Prim::Text {
            x: px + pw - 6.0 * s,
            y: py + shown as f32 * l.row_h + 2.0 * s,
            text: format!("{} / {}", highlighted + 1, opts.len()),
            argb: DIM,
            size: (10.0 * s) as i32,
            align: 2,
        });
    }
    let _ = (w, h);
    footer_prompts(
        prims,
        w,
        s,
        l.footer_y,
        &[Prompt { key: "ESC", label: "Back" }],
        &[
            Prompt { key: "LEFT/RIGHT", label: "Category" },
            Prompt { key: "UP/DOWN", label: "Gun" },
            Prompt { key: "ENTER", label: "Choose" },
        ],
    );
}

/// One weapon tile of the picker (`c-primary/001`'s strip, drawn as a row in our column).
#[allow(clippy::too_many_arguments)] // a tile's rect, its state and its label parts
pub(super) fn tile(prims: &mut Vec<Prim>, s: f32, x: f32, y: f32, w: f32, h: f32, on: bool, id: &str, name: &str, chosen: bool) {
    prims.push(Prim::Rect { x, y, w, h, argb: if on { ROW_ACTIVE } else { CELL_FILL } });
    hud::outline_rect(prims, x, y, w, h, if on { 2.0 } else { 1.0 } * s, if on { ORANGE } else { CELL_EDGE });
    let icon_size = (h - 8.0 * s).min(w * 0.45);
    weapon_icon(prims, id, x + 6.0 * s, y + 4.0 * s, icon_size, WHITE, name);
    prims.push(Prim::Text {
        x: x + icon_size + 16.0 * s,
        y: y + h * 0.5 - 8.0 * s,
        text: name.to_string(),
        argb: if on { ORANGE } else { WHITE },
        size: (14.0 * s) as i32,
        align: 0,
    });
    if chosen {
        prims.push(Prim::Text { x: x + w - 14.0 * s, y: y + h * 0.5 - 8.0 * s, text: "*".to_string(), argb: GOLD, size: (16.0 * s) as i32, align: 2 });
    }
}

/// The attachments/perks/bonus-card/offhand picker: BO2's list screen, the entry rows over the
/// title of the slot they fill.
pub(super) fn draw_option_picker(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, l: &EditorLayout, editor: &Editor) {
    let d = &editor.draft;
    let title_x = banner(prims, l.content_x, l.content_w, s, l.title_y, l.header_h);
    heading(prims, title_x, l.title_y, s, editor.slot.label(), WHITE);
    draw_allocation_counter(prims, l.content_x + l.content_w, l.title_y + 12.0 * s, s, d.spent(), crate::classmenu::PICK_10);
    draw_allocation(prims, l.content_x, l.bar_y, l.content_w, s, d.spent(), crate::classmenu::PICK_10);

    let opts = editor.options();
    let (px, py, pw, ph) = picker_rect(l, editor.slot);
    panel(prims, px - 8.0 * s, py - 8.0 * s, pw + 16.0 * s, ph + 16.0 * s, s);
    let (start, shown) = picker_window(l, editor.slot, editor.option);
    for (j, name) in opts.iter().enumerate().skip(start).take(shown) {
        let y = py + (j - start) as f32 * l.row_h;
        let on = j == editor.option;
        let chosen = editor.option_chosen(j);
        row(prims, px, y, pw, l.row_h - 5.0 * s, s, on);
        if let Some(art) = option_icon(editor, j) {
            icon(prims, Some(&art), px + 8.0 * s, y + 3.0 * s, 24.0 * s, if on || chosen { WHITE } else { DIM }, name);
        }
        if chosen {
            prims.push(Prim::Text { x: px + pw - 14.0 * s, y: y + 6.0 * s, text: "*".to_string(), argb: GOLD, size: (14.0 * s) as i32, align: 2 });
        }
        prims.push(Prim::Text {
            x: px + 40.0 * s,
            y: y + 4.0 * s,
            text: name.clone(),
            argb: if on { ORANGE } else if chosen { WHITE } else { DIM },
            size: (12.0 * s) as i32,
            align: 0,
        });
    }
    if opts.is_empty() {
        prims.push(Prim::Text { x: px + 10.0 * s, y: py + 8.0 * s, text: "NOTHING TO CHOOSE".to_string(), argb: DIM, size: (12.0 * s) as i32, align: 0 });
    }
    if opts.len() > shown {
        prims.push(Prim::Text {
            x: px + pw - 10.0 * s,
            y: py + shown as f32 * l.row_h + 4.0 * s,
            text: format!("{} / {}", editor.option + 1, opts.len()),
            argb: DIM,
            size: (11.0 * s) as i32,
            align: 2,
        });
    }
    // The hovered gun's bars, for the weapon-derived lists (lethal/tactical are not guns, so this
    // is only the secondary under Overkill's primary categories).
    if let Some(def) = editor_stats_weapon(editor) {
        let by = py + ph - 96.0 * s;
        if by > py {
            prims.push(Prim::Text { x: px, y: by, text: def.display_name.clone(), argb: WHITE, size: (12.0 * s) as i32, align: 0 });
            stat_bars(prims, px, by + 16.0 * s, pw, s, &weapon_stats(def));
        }
    }
    let _ = (w, h);
    footer_prompts(
        prims,
        w,
        s,
        l.footer_y,
        &[Prompt { key: "ESC", label: "Back" }],
        &[Prompt { key: "UP/DOWN", label: "Select" }, Prompt { key: "ENTER", label: "Choose" }],
    );
}

/// The icon of an option row in the editor's list.
pub(super) fn option_icon(editor: &Editor, i: usize) -> Option<String> {
    if let Some(def) = editor.option_weapon(i) {
        return Some(format!("ZHBO_BO2_Icon_{}", base_id(&def.id)));
    }
    if let Some(short) = editor.option_attachment(i) {
        return sprites::attachment_icon(short).map(str::to_string);
    }
    if let Some(perk) = editor.option_perk(i) {
        return Some(sprites::perk_icon(perk).to_string());
    }
    editor.option_wildcard(i).map(|w| sprites::wildcard_icon(w).to_string())
}

// ------------------------------------------------------------------------------------------------
// The shell's dialogs
// ------------------------------------------------------------------------------------------------
