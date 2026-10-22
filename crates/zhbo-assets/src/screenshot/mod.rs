//! Headless screenshots of the create-a-class screens.
//!
//! `zhbo-assets screenshot [outdir] [--archive <big>]` renders the screens through the *real*
//! drawing code (`classui::draw_screen`/`draw_dialog` and the `ScreenView` both game hosts produce)
//! and writes:
//!
//! * `cac-browse.png` — the shell choose-class screen: the set pager, the allocation readout, the
//!   class rows with BO2's hint, the kit panel, and the `ESC Back / S Class Set Options /
//!   O Class Options` prompt strip;
//! * `cac-set-pager.png` — the same screen on an untouched class set, so the pager's ten-dot marker
//!   sits mid-row (BO2's `a-chooseclass/002`);
//! * `cac-pick.png` — the in-match pick list (presets and customs, the pager read-only);
//! * `cac-editor.png` — the boxed allocation editor on a populated draft (`b-editor/028`);
//! * `cac-editor-empty.png` — BO2's pristine empty class (`b-editor/002`: 0/10, "Attachments not
//!   available" under both weapons, no `R Remove`);
//! * `cac-editor-weapons.png` — the primary weapon screen with the category column and the
//!   segmented stat bars (`c-primary/001`);
//! * `cac-editor-attachments.png` — the attachment picker;
//! * `cac-dialog-set.png` / `cac-dialog-class.png` — the shell's `S`/`O` dialogs
//!   (`a-chooseclass/021`, `/023`).
//!
//! Sprites come from the player's locally converted `ConquerOps_BO2.big`: the harness reads the
//! generated MappedImage INI and decodes its TGA pages, and the weapon catalog comes from the
//! `Data\ConquerOps\weapons.ini` the converter puts in the same archive (the built-in stand-in
//! table otherwise). Missing art draws the prims' fallbacks exactly like `hud::render` in game,
//! and is listed at the end so the next art pass knows what the archive lacks.
//!
//! This is a development tool on no shipped path. It never launches or connects to Black Ops II,
//! and it reads only the archives already on this machine. Screenshots are output, not source:
//! the default `target/cac-shots/` is inside the ignored build directory.

mod atlas;
mod contact;
mod font;
mod raster;

use crate::images;
use atlas::Atlas;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use zhbo_core::classes::CustomClass;
use zhbo_core::classmenu::{Draft, Editor, Personalize, PersonalizeKind, Slot, Wildcard};
use zhbo_core::classui::{self, Dialog, MenuPointer, MenuPrev, PickRow, ScreenView};
use zhbo_core::hud::Prim;
use zhbo_core::input::{Key, KeySet};
use zhbo_core::names;
use zhbo_core::profile;
use zhbo_core::weapons::{self, Perk};
use zhbo_formats::big::BigArchive;
use zhbo_formats::iwi::Rgba;

/// BO2's menu resolution: the layout is written for 720p and scales from there.
pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;

/// Render the three screens into `out`.
///
/// `archive` overrides the `ConquerOps_BO2.big` to read (the `--archive` flag); otherwise the
/// `ZHBO_BO2_ARCHIVE` environment variable, then the standard launcher dist locations, then
/// `build/dist` are tried.
pub fn run(out: &Path, archive: Option<&Path>, size: Option<(u32, u32)>) -> Result<(), String> {
    let archive = resolve_archive(archive)?;
    let atlas = Atlas::open(&archive)?;
    install_weapon_catalog(&archive);
    install_fonts(&archive);
    println!(
        "screenshot: {} ({} sprites on {} atlas pages)",
        archive.display(),
        atlas.len(),
        atlas.page_count()
    );
    seed_profile();
    let (w, h) = size.unwrap_or((WIDTH, HEIGHT));
    let result = write_shots(&atlas, out, w, h);
    clear_profile();
    result
}

/// Render every MappedImage in `archive` to one labelled contact sheet (plus the raw pages): the
/// art-QA view that shows a rect pointing at the wrong place, or a stale archive, at a glance.
pub fn run_contact(out: &Path, archive: Option<&Path>, cols: u32) -> Result<(), String> {
    contact::run(out, archive, cols)
}

/// Every shot, in the order it is printed.
fn write_shots(atlas: &Atlas, out: &Path, w: u32, h: u32) -> Result<(), String> {
    SIZE.with(|s| s.set((w as f32, h as f32)));
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut missing = BTreeSet::new();
    let shots = [
        ("cac-browse", browse_prims()),
        ("cac-set-pager", pager_prims()),
        ("cac-pick", pick_prims()),
        ("cac-editor", editor_prims()),
        ("cac-editor-empty", empty_editor_prims()),
        ("cac-editor-weapons", weapon_picker_prims()),
        ("cac-editor-attachments", attachment_picker_prims()),
        ("cac-personalize", personalize_prims()),
        ("cac-dialog-set", set_dialog_prims()),
        ("cac-dialog-class", class_dialog_prims()),
    ];
    for (name, prims) in shots {
        let canvas = raster::render(&prims, atlas, w, h, &mut missing);
        let file = out.join(format!("{name}.png"));
        let (width, height) = (canvas.width(), canvas.height());
        images::write_png(&file, &Rgba { width, height, pixels: canvas.pixels().to_vec() })?;
        println!("wrote {}", file.display());
    }
    if missing.is_empty() {
        println!("art: every sprite the screens asked for resolved");
    } else {
        println!("art: {} sprite name(s) missing from the archive (their fallbacks drew):", missing.len());
        for name in &missing {
            println!("  {name}");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// The views
// ------------------------------------------------------------------------------------------------

/// The shell choose-class screen: the full list and the management footer (`manage: true`).
fn browse_prims() -> Vec<Prim> {
    let rows = classui::pick_rows(true);
    // Highlight the first custom row: the shot then shows the player's own classes and their kit,
    // not just the shipped presets.
    let highlight = rows.iter().position(|r| matches!(r, PickRow::Custom(0))).unwrap_or(0);
    draw(&ScreenView { highlight, editor: None, personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None })
}

/// The shell screen on an untouched class set: the pager's marker sits mid-row, as BO2's does on
/// the empty sets 2-10 (`a-chooseclass/002`).
fn pager_prims() -> Vec<Prim> {
    let set = profile::active_set();
    profile::set_active(3);
    let prims = draw(&ScreenView { highlight: 0, editor: None, personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None });
    profile::set_active(set);
    prims
}

/// The in-match pick screen: presets and the player's customs, no CREATE A CLASS row, no
/// management and a read-only pager (`manage: false`, what `Client::class_screen_view` produces).
fn pick_prims() -> Vec<Prim> {
    let rows = classui::pick_rows(false);
    let highlight = rows.iter().position(|r| matches!(r, PickRow::Custom(1))).unwrap_or(0);
    draw(&ScreenView { highlight, editor: None, personalize: None, deny: 0.0, manage: false, now: 0.0, set_change: None, open: None })
}

/// The allocation editor on a populated draft, its boxed grid closed (`b-editor/028`).
fn editor_prims() -> Vec<Prim> {
    let mut editor = editor_state();
    editor.picking = false;
    draw(&ScreenView { highlight: 0, editor: Some(&editor), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None })
}

/// The personalize screen: BO2's six-column camo grid with `None` first (`i-personalize/001`).
fn personalize_prims() -> Vec<Prim> {
    let mut editor = editor_state();
    editor.picking = false;
    let choices = classui::personalize_choices_for(PersonalizeKind::Camo, Some(editor.draft.primary.as_str()));
    let p = Personalize::new(PersonalizeKind::Camo, choices, &editor.draft);
    draw(&ScreenView { highlight: 0, editor: Some(&editor), personalize: Some(&p), deny: 0.0, manage: true, now: 0.0, set_change: None, open: None })
}

/// BO2's pristine empty class (`b-editor/002`): nothing chosen, so the attachment rows read
/// "Attachments not available" and the footer has no `R Remove`.
fn empty_editor_prims() -> Vec<Prim> {
    let draft = Draft {
        name: "Custom 1".into(),
        primary: String::new(),
        primary_atts: Vec::new(),
        secondary: String::new(),
        secondary_atts: Vec::new(),
        lethal: String::new(),
        tactical: String::new(),
        perks: [None, None, None],
        extra_perks: [None, None, None],
        wildcards: Vec::new(),
        reserve: [0, 0],
        camo: String::new(),
        reticle: String::new(),
        reticle_color: String::new(),
        knife: String::new(),
    };
    let editor = Editor::new(draft);
    draw(&ScreenView { highlight: 0, editor: Some(&editor), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None })
}

/// The primary weapon screen: the category column, the category's tiles and the segmented bars
/// (`c-primary/001`).
fn weapon_picker_prims() -> Vec<Prim> {
    let editor = editor_state();
    draw(&ScreenView { highlight: 0, editor: Some(&editor), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None })
}

/// The attachment picker of the primary (`d-attach/001`).
fn attachment_picker_prims() -> Vec<Prim> {
    let mut editor = editor_state();
    editor.picking = false;
    editor.slot = Slot::PrimaryAtts;
    let mut prev = MenuPrev::default();
    // Enter opens the attachment strip's list through the real edge fold.
    let mut pointer = MenuPointer { screen_w: size().0, screen_h: size().1, ..Default::default() };
    pointer.keys.set_down(Key::Enter, true);
    classui::editor_frame(&mut editor, &pointer, &prev);
    prev.update(&pointer);
    pointer.keys.set_down(Key::Enter, false);
    classui::editor_frame(&mut editor, &pointer, &prev);
    if let Some(i) = editor.options().iter().position(|o| o == "Reflex") {
        editor.option = i;
    }
    draw(&ScreenView { highlight: 0, editor: Some(&editor), personalize: None, deny: 0.0, manage: true, now: 0.0, set_change: None, open: None })
}

/// The shell's `S Class Set Options` dialog over the choose-class screen (`a-chooseclass/021`).
fn set_dialog_prims() -> Vec<Prim> {
    let mut prims = browse_prims();
    let dialog = Dialog::set_options(&profile::get().set_name(profile::active_set()));
    classui::draw_dialog(&mut prims, size().0, size().1, &dialog, None);
    prims
}

/// The shell's `O Class Options` dialog over the choose-class screen (`a-chooseclass/023`).
fn class_dialog_prims() -> Vec<Prim> {
    let mut prims = browse_prims();
    let name = first_custom_name();
    let dialog = Dialog::class_options(&name);
    classui::draw_dialog(&mut prims, size().0, size().1, &dialog, None);
    prims
}

/// The first custom class's name, for the class dialog's subtitle.
fn first_custom_name() -> String {
    profile::classes().first().map(|c| c.name.clone()).unwrap_or_else(|| "Custom 1".into())
}

fn draw(view: &ScreenView<'_>) -> Vec<Prim> {
    let mut prims = Vec::new();
    classui::draw_screen(&mut prims, size().0, size().1, view);
    prims
}

/// A realistic draft: a named SMG class at exactly ten points, with Perk 1 Greed on so the extra
/// PERK 1 box and the bonus card are visible. The option list is opened through the real edge fold
/// (`classui::editor_frame`), not by setting the editor's fields by hand.
fn editor_state() -> Editor {
    let draft = Draft {
        name: "BLACKOUT".into(),
        primary: "mp7_mp".into(),
        primary_atts: vec!["reflex".into()],
        secondary: "fnp45_mp".into(),
        secondary_atts: Vec::new(),
        lethal: "frag_grenade_mp".into(),
        tactical: "concussion_grenade_mp".into(),
        perks: [Some(Perk::LIGHTWEIGHT), Some(Perk::FAST_HANDS), Some(Perk::DEXTERITY)],
        extra_perks: [Some(Perk::FLAK_JACKET), None, None],
        wildcards: vec![Wildcard::Perk1Greed],
        reserve: [0, 0],
        camo: String::new(),
        reticle: String::new(),
        reticle_color: String::new(),
        knife: String::new(),
    };
    let mut editor = Editor::new(draft);
    let mut prev = MenuPrev::default();
    let mut pointer = MenuPointer { screen_w: size().0, screen_h: size().1, ..Default::default() };
    // Enter opens the Primary list; the two Downs walk it to a different gun, whose stat bars the
    // view then shows.
    for key in [Key::Enter, Key::Down, Key::Down] {
        pointer.keys = KeySet::default();
        pointer.keys.set_down(key, true);
        classui::editor_frame(&mut editor, &pointer, &prev);
        prev.update(&pointer);
        pointer.keys.set_down(key, false);
        classui::editor_frame(&mut editor, &pointer, &prev);
        prev.update(&pointer);
    }
    debug_assert!(editor.picking, "the option list is open");
    debug_assert_eq!(editor.slot, Slot::Primary);
    editor
}

// ------------------------------------------------------------------------------------------------
// State and archive resolution
// ------------------------------------------------------------------------------------------------

/// Seed the global profile with a couple of saved classes, so the shots show real custom rows
/// instead of an empty list. The profile is process-local (no file is read or written).
fn seed_profile() {
    profile::load("");
    for (name, primary, perks) in [
        ("BLACKOUT", "mp7_mp+reflex", &["Lightweight", "FastHands", "Dexterity"][..]),
        ("OVERWATCH", "dsr50_mp", &["FlakJacket", "Toughness", "Dexterity"][..]),
        ("RUNNER", "mp7_mp+extclip", &["Lightweight", "ExtremeConditioning", "Dexterity"][..]),
    ] {
        profile::save(CustomClass {
            name: name.into(),
            primary: primary.into(),
            secondary: "fnp45_mp".into(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: perks.iter().map(|p| (*p).to_string()).collect(),
            reserve: [0, 0],
            camo: String::new(),
            reticle: String::new(),
            reticle_color: String::new(),
            knife: String::new(),
        });
    }
}

fn clear_profile() {
    profile::load("");
}

thread_local! {
    /// The size the views draw at (`--size`; BO2's 720p by default).
    static SIZE: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((WIDTH as f32, HEIGHT as f32)) };
}

/// The views' screen size.
fn size() -> (f32, f32) {
    SIZE.with(|s| s.get())
}

/// Load BO2's UI font metrics (`names::FONTS_PATH`) from the archive, as the bridge does at init;
/// without them the text draws in the harness's own font.
fn install_fonts(archive: &Path) {
    let text = BigArchive::open(archive)
        .ok()
        .and_then(|big| big.find(names::FONTS_PATH).and_then(|e| big.read(e).ok()))
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default();
    zhbo_core::lui::font::load(&text);
    println!("fonts: {}", if zhbo_core::lui::font::loaded() { "BO2's UI fonts" } else { "none in the archive (engine-font stand-in)" });
}

/// Install the converted weapon table (names, icons, the editor's option lists) from the
/// `Data\ConquerOps\weapons.ini` the converter puts in the BO2 archive (the next `ConquerOps.big`
/// is tried as a fallback). Without it the core's built-in stand-in table is used, which only has
/// the default kit, so the shot still renders but lists far fewer guns.
fn install_weapon_catalog(archive: &Path) {
    let mut archives: Vec<PathBuf> = vec![archive.to_path_buf()];
    if let Some(dir) = archive.parent() {
        archives.push(dir.join("ConquerOps.big"));
    }
    archives.push(PathBuf::from("build/dist/ConquerOps.big"));
    for path in archives {
        let Ok(big) = BigArchive::open(&path) else { continue };
        let Some(entry) = big.find(names::WEAPON_TABLE_PATH) else { continue };
        let Ok(bytes) = big.read(entry) else { continue };
        let Ok(text) = String::from_utf8(bytes) else { continue };
        match weapons::install_catalog(&text) {
            Ok(n) => println!("weapon table: {n} weapons from {}", path.display()),
            Err(e) => println!("weapon table: {}: {e} (using the built-in table)", path.display()),
        }
        return;
    }
    println!("weapon table: no {} found; using the built-in stand-in table", names::WEAPON_TABLE_PATH);
}

/// The archive to read: `--archive`, then `ZHBO_BO2_ARCHIVE`, then the launcher's dist folder,
/// then the dev build tree. The error says how to produce the archive, in the launcher's words.
pub(crate) fn resolve_archive(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if path.is_file() { Ok(path.to_path_buf()) } else { Err(format!("{}: no such archive", path.display())) };
    }
    if let Some(path) = std::env::var_os("ZHBO_BO2_ARCHIVE").map(PathBuf::from) {
        return if path.is_file() { Ok(path) } else { Err(format!("ZHBO_BO2_ARCHIVE={}: no such archive", path.display())) };
    }
    let candidates = archive_candidates();
    for path in &candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    let looked: Vec<String> = candidates.iter().map(|p| format!("  {}", p.display())).collect();
    Err(format!(
        "ConquerOps_BO2.big not found. Looked at:\n{}\n\
         Build it by launching the mod once through its launcher (`cargo xtask run`, or run the\n\
         `conquerops` launcher in the dist folder): the launcher converts the BO2 HUD art from your\n\
         own Black Ops II install and writes ConquerOps_BO2.big next to itself. To use an archive\n\
         built elsewhere, pass `--archive <path>` or set ZHBO_BO2_ARCHIVE.",
        looked.join("\n")
    ))
}

fn archive_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        v.push(home.join("Library/Application Support/ConquerOps/dist").join(names::BO2_ARCHIVE_NAME));
        v.push(home.join(".local/share/ConquerOps/dist").join(names::BO2_ARCHIVE_NAME));
    }
    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        v.push(appdata.join("ConquerOps").join("dist").join(names::BO2_ARCHIVE_NAME));
    }
    v.push(PathBuf::from("build/dist").join(names::BO2_ARCHIVE_NAME));
    v.push(PathBuf::from("dist").join(names::BO2_ARCHIVE_NAME));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(prims: &[Prim]) -> Vec<&str> {
        prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_screen_draws_from_the_real_prims_without_an_archive() {
        seed_profile();
        let browse = browse_prims();
        let browse_text = texts(&browse);
        assert!(browse_text.contains(&"CHOOSE CLASS"), "the heading draws: {browse_text:?}");
        assert!(browse_text.contains(&"BLACKOUT"), "the profile's custom row draws: {browse_text:?}");
        assert!(browse_text.contains(&"Custom 4") && browse_text.contains(&"Custom 5"), "the set's untouched slots draw: {browse_text:?}");
        // BO2's hint wraps to the list's width (`hinttext.lua`).
        assert!(browse_text.iter().any(|t| classui::SET_HINT.starts_with(*t) && !t.is_empty()), "BO2's hint draws: {browse_text:?}");
        assert!(browse_text.contains(&"Class Set Options") && browse_text.contains(&"Class Options"), "the S/O prompts draw");

        // The pager shot is on an untouched set: the marker sits mid-row and the list has no customs.
        let pager = pager_prims();
        let pager_text = texts(&pager);
        assert!(pager_text.contains(&"CLASS SET 4"), "the pager names the browsed set: {pager_text:?}");
        assert!(!pager_text.contains(&"BLACKOUT"), "another set's classes are not listed");

        let pick = pick_prims();
        let pick_text = texts(&pick);
        assert!(pick_text.contains(&"OVERWATCH"), "the in-match list includes customs");
        assert!(!pick_text.contains(&"Class Set Options"), "nothing to manage in a match");

        let editor = editor_prims();
        let editor_text = texts(&editor);
        assert_eq!(editor_text.iter().filter(|t| **t == "PERK 1").count(), 1, "the perk rows draw once each: {editor_text:?}");
        assert!(editor_text.contains(&"BLACKOUT"), "the draft's name is the heading");
        // With the converted table the gun offers attachments (the rows draw); with the stand-in
        // table an unknown gun offers none (BO2's "Attachments not available").
        assert!(
            editor_text.contains(&"ATTACHMENTS") || editor_text.contains(&"Attachments not available"),
            "the attachment row draws: {editor_text:?}"
        );
        assert!(editor_text.contains(&"Remove"), "the R Remove prompt draws for a filled slot");

        // BO2's pristine empty class says so under both weapons, and has no R Remove.
        let empty = empty_editor_prims();
        let empty_text = texts(&empty);
        assert_eq!(empty_text.iter().filter(|t| **t == "Attachments not available").count(), 2, "{empty_text:?}");
        assert!(!empty_text.contains(&"Remove"), "nothing to remove in an empty class");

        // The weapon screen carries BO2's category column and bar labels. The column lists the
        // categories the weapon table has, in BO2's order (rotated to put the active one on top;
        // the shot opens on the first category, so the order reads as BO2's).
        let weapons = weapon_picker_prims();
        let weapon_text = texts(&weapons);
        for label in ["FIRE RATE", "DAMAGE", "RANGE", "ACCURACY"] {
            assert!(weapon_text.contains(&label), "stat bar {label}: {weapon_text:?}");
        }
        let bo2 = ["SUBMACHINE GUNS", "ASSAULT RIFLES", "SHOTGUNS", "LIGHT MACHINE GUNS", "SNIPER RIFLES", "SPECIALS", "LAUNCHERS"];
        let drawn: Vec<usize> = weapon_text.iter().filter_map(|t| bo2.iter().position(|b| b == t)).collect();
        assert!(drawn.len() >= 2, "the category column draws: {weapon_text:?}");
        assert!(drawn.windows(2).all(|w| w[0] < w[1]), "BO2's category order: {drawn:?}");

        // The attachment picker's lower half carries the highlighted entry, the four bars and
        // the `Equipped` box (`cacattachmentsmenu.lua:268-536`, `d-attach/002`).
        let att = attachment_picker_prims();
        let att_text = texts(&att);
        assert!(att_text.contains(&"Equipped"), "the slot-list title draws: {att_text:?}");
        for label in ["FIRE RATE", "DAMAGE", "RANGE", "ACCURACY"] {
            assert!(att_text.contains(&label), "the attachment panel's {label} bar draws: {att_text:?}");
        }
        assert!(att_text.contains(&"Reflex Sight"), "the full attachment title draws: {att_text:?}");
        assert!(att_text.contains(&"Precision red dot sight."), "the attachment description draws: {att_text:?}");

        // The personalize screen carries BO2's title and the six-column grid, `None` first.
        let personalize = personalize_prims();
        let personalize_text = texts(&personalize);
        assert!(personalize_text.contains(&"CAMO"), "the personalize title draws: {personalize_text:?}");
        assert!(personalize_text.contains(&"None"), "the first camo cell draws: {personalize_text:?}");
        assert!(personalize_text.contains(&"EQUIPPED"), "the Equipped label draws: {personalize_text:?}");
        assert!(personalize_text.contains(&"MP7"), "the weapon preview draws: {personalize_text:?}");
        assert!(personalize_text.contains(&"Back"), "the ESC Back prompt draws: {personalize_text:?}");

        // The dialogs carry BO2's titles and entries.
        let set_prims = set_dialog_prims();
        let set_dialog = texts(&set_prims);
        assert!(set_dialog.contains(&"CLASS SET OPTIONS"), "{set_dialog:?}");
        assert!(set_dialog.contains(&"RENAME") && set_dialog.contains(&"RESET SET"));
        let class_prims = class_dialog_prims();
        let class_dialog = texts(&class_prims);
        assert!(class_dialog.contains(&"CLASS OPTIONS"));
        assert!(class_dialog.contains(&"CLEAR LOADOUT"));

        // Every shot rasterizes into a screen with actual content (not one flat fill), with every
        // image falling back exactly as `hud::render` would.
        let atlas = Atlas::empty();
        let mut missing = BTreeSet::new();
        for prims in [&browse, &pager, &pick, &editor, &empty, &weapons, &att, &personalize] {
            let canvas = raster::render(prims, &atlas, WIDTH, HEIGHT, &mut missing);
            let distinct: BTreeSet<[u8; 4]> = canvas.pixels().as_chunks::<4>().0.iter().copied().collect();
            // The empty class and the personalize grid are legitimately sparse (no icons): a
            // handful of chrome shades.
            assert!(distinct.len() > 6, "only {} colours drawn", distinct.len());
        }
        assert!(!missing.is_empty(), "the fallbacks were exercised");
        clear_profile();
    }
}
