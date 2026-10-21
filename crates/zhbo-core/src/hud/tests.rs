//! The HUD's tests.

use super::*;

/// Build the HUD's prims and walk them into the recorder (the tests assert on both).
fn draw_hud(rec: &Recorder, client: &Client, sim: &Sim) -> Vec<Prim> {
    let mut prims = Vec::new();
    draw(rec, &mut prims, client, sim, 0);
    render(&prims, rec);
    prims
}
use crate::client::ClientIn;
use crate::sim::tests::{general, soldier, FakeHost};
use crate::sim::Sim;
use std::cell::RefCell;

#[derive(Default)]
struct Recorder {
    texts: RefCell<Vec<String>>,
    images: RefCell<Vec<String>>,
    prims: RefCell<usize>,
    lines: RefCell<Vec<(f32, f32, f32, f32)>>,
    has_art: bool,
    objects: Vec<ObjInfo>,
}

impl ClientHost for Recorder {
    fn screen_size(&self) -> (f32, f32) {
        (1280.0, 720.0)
    }
    fn fill_rect(&self, _x: f32, _y: f32, _w: f32, _h: f32, _argb: u32) {
        *self.prims.borrow_mut() += 1;
    }
    fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, _width: f32, _argb: u32) {
        *self.prims.borrow_mut() += 1;
        self.lines.borrow_mut().push((x0, y0, x1, y1));
    }
    fn text(&self, _x: f32, _y: f32, text: &str, _argb: u32, _size: i32, _align: i32) {
        self.texts.borrow_mut().push(text.to_string());
    }
    fn image(&self, name: &str, _x: f32, _y: f32, _w: f32, _h: f32, _argb: u32) -> bool {
        self.images.borrow_mut().push(name.to_string());
        self.has_art
    }
    fn query(&self, _c: Vec3, _r: f32, _v: u32, out: &mut Vec<ObjInfo>) {
        out.extend_from_slice(&self.objects);
    }
    fn cash(&self, _player: i32) -> i32 {
        10_000
    }
}

fn fps_state() -> (Sim, Client) {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    let mut client = Client::default();
    sim.take_control(&host, 0, 1);
    // A class is chosen (the pick screen only shows while the class is unset).
    sim.choose_class(0, 1, 0, &[]);
    let mut inp = ClientIn::default();
    for f in 1..160 {
        client.frame(&mut sim, &inp);
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    (sim, client)
}

#[test]
fn uses_bo2_numerals_when_the_art_is_installed() {
    let (sim, client) = fps_state();
    let rec = Recorder { has_art: true, objects: vec![soldier(5, 1, 40.0)], ..Default::default() };
    let _ = draw_hud(&rec, &client, &sim);
    let images = rec.images.borrow();
    let mat = |m: &str| crate::lui::materials::image(m);
    assert!(images.contains(&mat("hud_mp_num_big_3")), "30 rounds -> '3' digit");
    assert!(images.contains(&mat("hud_mp_num_big_0")));
    assert!(images.contains(&mat("hud_mp_num_big_slash")));
    assert!(images.contains(&mat("hud_mp_num_big_1")), "funds (10000) in HUD digits");
    let texts = rec.texts.borrow();
    assert!(texts.iter().any(|t| t == "MTAR"), "BO2's Assault class carries the MTAR: {texts:?}");
    for art in [
        sprites::COMPASS,
        sprites::PLAYER_ARROW,
        sprites::DOT,
        sprites::STREAK_VSAT,
        &mat("hud_mp_vis_left_lower"),
        &mat("hud_mp_vis_right_lower"),
        &mat("hud_mp_vis_gun_title_mid"),
        &mat("hud_mp_vis_ks_dpad_box_back_lit"),
    ] {
        assert!(images.contains(&art.to_string()), "BO2 art {art}");
    }
    assert!(images.contains(&sprites::TEAM_LOGO.to_string()), "Zero Hour's faction logo");
    assert!(!texts.iter().any(|t| t == "3" || t == "0"), "no font fallback when art exists");
}

#[test]
fn map_key_shows_the_radar_or_says_there_is_none() {
    // A crosshair line touches (640±gap, 360) with a gap of about 40; the map's own radar box
    // keeps its edges more than 260 px from the centre, so this window only sees the crosshair.
    let near_centre = |lines: &[(f32, f32, f32, f32)]| {
        lines
            .iter()
            .filter(|&&(x0, y0, x1, y1)| {
                [(x0, y0), (x1, y1)]
                    .iter()
                    .any(|&(x, y)| (x - 640.0).abs() < 70.0 && (y - 360.0).abs() < 70.0)
            })
            .count()
    };
    let (sim, mut client) = fps_state();
    client.hud.map_open = true;
    let rec = Recorder { has_art: true, ..Default::default() };
    let _ = draw_hud(&rec, &client, &sim);
    let texts = rec.texts.borrow();
    assert!(texts.iter().any(|t| t == "TACTICAL MAP"));
    assert!(texts.iter().any(|t| t == "NO RADAR"), "the fake host has no radar");
    // The same frame's map_open: no crosshair while the map is up (the ammo panel and the
    // minimap stay, like BO2's scoreboard).
    assert_eq!(near_centre(&rec.lines.borrow()), 0, "no crosshair under the map");
    drop(texts);
    client.hud.map_open = false;
    let rec = Recorder { has_art: true, ..Default::default() };
    let _ = draw_hud(&rec, &client, &sim);
    assert!(near_centre(&rec.lines.borrow()) >= 4, "the crosshair is back with the map closed");
}

#[test]
fn falls_back_to_text_without_the_art() {
    let (sim, client) = fps_state();
    let rec = Recorder::default();
    let _ = draw_hud(&rec, &client, &sim);
    let texts = rec.texts.borrow();
    assert!(texts.iter().any(|t| t == "3") && texts.iter().any(|t| t == "0"));
}

/// Bug 1's HUD sweep: an unarmed General (knife only) draws no ammo panel ("0/0", "No ammo"),
/// no offhand counters and no gun crosshair. It must not panic on the empty gun slots.
#[test]
fn unarmed_draws_no_ammo_panel_or_crosshair() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    let mut client = Client::default();
    sim.take_control(&host, 0, 1);
    let mut inp = ClientIn::default();
    for f in 1..160 {
        client.frame(&mut sim, &inp);
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    assert!(matches!(client.view, crate::client::ViewMode::Fps));
    assert!(sim.player(0).unwrap().arsenal.is_unarmed());
    // Draw the play view (the spawn offer normally covers it).
    client.class_pick_open = false;
    let rec = Recorder { has_art: true, ..Default::default() };
    let _ = draw_hud(&rec, &client, &sim);
    let texts = rec.texts.borrow();
    assert!(!texts.iter().any(|t| t == "No ammo"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Knife"), "no weapon panel for the knife: {texts:?}");
    drop(texts);
    let near_centre = rec.lines.borrow().iter().filter(|&&(x0, y0, x1, y1)| {
        [(x0, y0), (x1, y1)].iter().any(|&(x, y)| (x - 640.0).abs() < 70.0 && (y - 360.0).abs() < 70.0)
    }).count();
    assert_eq!(near_centre, 0, "no crosshair without a gun");
}

#[test]
fn nothing_in_rts_view_unless_the_pick_is_open() {
    // No General in the sim, and no forced pick: the overhead view draws nothing (an unclassed
    // General must not get the pick screen over the RTS view).
    let rec = Recorder::default();
    let client = Client::default();
    let _ = draw_hud(&rec, &client, &Sim::new());
    assert!(rec.texts.borrow().is_empty(), "nothing draws: {:?}", rec.texts.borrow());

    // The command bar forces it: the pick screen draws, and it is pick-only (no CREATE row,
    // no delete/copy affordances).
    let mut client = Client::default();
    client.class_pick_open = true;
    let rec = Recorder::default();
    let _ = draw_hud(&rec, &client, &Sim::new());
    let texts = rec.texts.borrow();
    assert!(texts.iter().any(|t| t == "CHOOSE CLASS"), "the pick screen drew: {texts:?}");
    assert!(!texts.iter().any(|t| t == "Ten points of allocation"), "no CREATE A CLASS row in a match");
    assert!(texts.iter().any(|t| t == "Back"), "the in-match footer closes: {texts:?}");
    // The pager is a read-only display of the active set in a match: no management prompts.
    assert!(texts.iter().any(|t| t == "CLASS SET 1"), "the active set shows");
    assert!(!texts.iter().any(|t| t == "Class Set Options"), "no set options in a match");
}

#[test]
fn cac_sprite_names_follow_the_core_tables() {
    use crate::classmenu::Wildcard;
    use crate::weapons::{Perk, PERKS};
    for (i, def) in PERKS.iter().enumerate() {
        assert_eq!(sprites::perk_icon(Perk::ALL[i]), sprites::PERK_ICONS[i]);
        assert_eq!(sprites::PERK_ICONS[i], format!("ZHBO_BO2_Perk_{}", def.id));
    }
    for w in Wildcard::ALL {
        assert_eq!(sprites::wildcard_icon(w), format!("ZHBO_BO2_Wildcard_{w:?}"), "the icon follows the wildcard's name");
    }
    let mut shorts: Vec<&str> = sprites::ATTACHMENT_ICONS.iter().map(|(short, _)| *short).collect();
    shorts.sort_unstable();
    shorts.dedup();
    assert_eq!(shorts.len(), sprites::ATTACHMENT_ICONS.len(), "attachment shorts are unique");
    for (short, name) in sprites::ATTACHMENT_ICONS {
        assert_eq!(sprites::attachment_icon(short), Some(*name));
    }
    assert!(sprites::attachment_icon("not-an-attachment").is_none());
    // The converter's fixed-name list must not repeat a name (the atlas keys images by it).
    let mut all = sprites::ALL.to_vec();
    let n = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), n, "every MappedImage name is unique");
}

#[test]
fn every_hud_material_is_converted() {
    for m in materials() {
        assert!(crate::lui::materials::MATERIALS.iter().any(|(n, _)| *n == m), "{m} is missing from lui::materials::MATERIALS");
    }
}

#[test]
fn tab_shows_the_scoreboard_and_hides_the_hud() {
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    let (sim, mut client) = fps_state();
    client.hud.scoreboard_open = true;
    client.hud.lui.kills = 2;
    let rec = Recorder { has_art: true, ..Default::default() };
    let _prims = draw_hud(&rec, &client, &sim);
    let texts = rec.texts.borrow();
    for want in ["SCOREBOARD", "KILLS", "DEATHS", "200"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    let images = rec.images.borrow();
    assert!(!images.contains(&crate::lui::materials::image("hud_mp_vis_right_lower")), "the ammo area hides under the scoreboard");
}
