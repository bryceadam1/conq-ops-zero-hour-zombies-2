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

/// Give the held weapon BO2's common crosshair (`reticle_side_small` at 8, the rifles' 0/1/0.2
/// fractions): the built-in table carries no crosshair art.
fn with_bo2_crosshair(sim: &mut Sim) {
    let w = sim.players[0].as_mut().expect("the player").arsenal.current_mut();
    let mut d = w.def.clone();
    d.reticle = crate::weapons::Reticle { side: "reticle_side_small".into(), center_size: 4.0, side_size: 8.0, ads_in_frac: 1.0, ads_out_frac: 0.2, ..Default::default() };
    w.def = Box::leak(Box::new(d));
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
    let (mut sim, mut client) = fps_state();
    with_bo2_crosshair(&mut sim);
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
fn the_crosshair_uses_the_classs_reticle_and_colour() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    let mut client = Client::default();
    sim.take_control(&host, 0, 1);
    let custom = crate::classes::CustomClass {
        name: "Reticle Kit".into(),
        primary: "an94_mp+reflex".into(),
        reticle: "reticle_3".into(),
        reticle_color: "reticle_color_2".into(),
        ..Default::default()
    };
    sim.choose_class(0, 1, usize::MAX, &custom.to_wire_words());
    let mut inp = ClientIn::default();
    for f in 1..160 {
        client.frame(&mut sim, &inp);
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    let p = sim.players[0].as_mut().expect("the player");
    assert_eq!(p.reticle, "reticle_3");
    assert_eq!(p.reticle_color, "reticle_color_2");
    p.arsenal.ads = 1.0;
    let rec = Recorder { has_art: true, ..Default::default() };
    let prims = draw_hud(&rec, &client, &sim);
    let argb = prims
        .iter()
        .find_map(|prim| match prim {
            Prim::Image { name, argb, .. } if name == "reflex_3" => Some(*argb),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the class's reticle art draws: {prims:?}"));
    // `reticle_color_2` is Green: the tint's green channel dominates red and blue.
    assert!(((argb >> 8) & 0xFF) > ((argb >> 16) & 0xFF), "{argb:#010x}");
    assert!(((argb >> 8) & 0xFF) > (argb & 0xFF), "{argb:#010x}");
}

/// BO2 has one optic reticle, on the lens: while the native viewmodel draws it there, the HUD's
/// 2D stand-in (the W3D fallback's) stays out.
#[test]
fn the_hud_reticle_gives_way_to_the_lens_reticle() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    let mut client = Client::default();
    sim.take_control(&host, 0, 1);
    let custom = crate::classes::CustomClass { name: "Reflex".into(), primary: "an94_mp+reflex".into(), ..Default::default() };
    sim.choose_class(0, 1, usize::MAX, &custom.to_wire_words());
    let mut inp = ClientIn::default();
    for f in 1..160 {
        client.frame(&mut sim, &inp);
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    sim.players[0].as_mut().expect("the player").arsenal.ads = 1.0;
    let centre_images = |prims: &[Prim]| prims.iter().filter(|p| matches!(p, Prim::Image { name, .. } if name == sprites::RED_DOT || name.contains("_reticle"))).count();
    let rec = Recorder { has_art: true, ..Default::default() };
    assert_eq!(centre_images(&draw_hud(&rec, &client, &sim)), 1, "the W3D fallback: the 2D reticle");
    crate::nativevm::set_lens_reticle_drawn(true);
    let rec = Recorder { has_art: true, ..Default::default() };
    let n = centre_images(&draw_hud(&rec, &client, &sim));
    crate::nativevm::set_lens_reticle_drawn(false);
    assert_eq!(n, 0, "the lens reticle draws: no 2D copy");
}

#[test]
fn the_scope_overlay_follows_bo2s_reticle_zoom() {
    // `FUN_004b51e0`: the overlay draws while the reticle zoom is up, at alpha zoom / max(scale, 1).
    // A base sniper's in-window is empty (0): nothing while aiming in, even at full ADS, until the
    // direction flips (`HudLui::tick`); aiming out it fades over the last 5 %.
    let (mut sim, mut client) = fps_state();
    let def = {
        let mut d = sim.players[0].as_ref().expect("the player").arsenal.current_weapon().clone();
        d.overlay_material = "scope_overlay_xpr50".into();
        d.overlay_size = (480.0, 480.0);
        (d.zoom.in_frac, d.zoom.out_frac, d.zoom.overlay_alpha_scale) = (0.0, 0.05, 1.0);
        Box::leak(Box::new(d))
    };
    sim.players[0].as_mut().expect("the player").arsenal.current_mut().def = def;
    let name = crate::lui::materials::image("scope_overlay_xpr50");
    let overlay_alpha = |client: &Client, sim: &Sim| {
        let rec = Recorder { has_art: true, ..Default::default() };
        draw_hud(&rec, client, sim).iter().find_map(|p| match p {
            Prim::Image { name: n, argb, .. } if n == &name => Some(argb >> 24),
            _ => None,
        })
    };
    let set = |sim: &mut Sim, client: &mut Client, ads: f32| {
        let p = sim.players[0].as_mut().expect("the player");
        p.arsenal.ads = ads;
        client.hud.lui.tick(0.016, Some(p));
    };
    set(&mut sim, &mut client, 0.0);
    set(&mut sim, &mut client, 0.99);
    assert_eq!(overlay_alpha(&client, &sim), None, "aiming in: no overlay");
    assert!(!client.hud.lui.gun_under_overlay);
    set(&mut sim, &mut client, 1.0);
    assert!(!client.hud.lui.gun_under_overlay, "the first full-ADS frame still draws the gun (tested before the flip)");
    assert_eq!(overlay_alpha(&client, &sim), Some(255), "the overlay is up once the direction flips");
    set(&mut sim, &mut client, 1.0);
    assert!(client.hud.lui.gun_under_overlay, "held: the viewmodel is not drawn");
    set(&mut sim, &mut client, 0.975);
    assert_eq!(overlay_alpha(&client, &sim), Some(128), "aiming out: half way through the 5 % window");
    set(&mut sim, &mut client, 0.94);
    assert_eq!(overlay_alpha(&client, &sim), None);
    assert!(!client.hud.lui.gun_under_overlay);
}

#[test]
fn the_scope_overlay_draws_a_round_scope_at_every_aspect() {
    // `FUN_004b4c70`: the 480 x 480 sniper art is drawn square in pixels, centred, and the screen
    // around it is filled black at the overlay's alpha (`FUN_004b4840`) — never the art stretched
    // over the whole screen (the ellipse the HUD once drew).
    let (mut sim, _client) = fps_state();
    let def = {
        let mut d = sim.players[0].as_ref().expect("the player").arsenal.current_weapon().clone();
        d.overlay_material = "scope_overlay_xpr50".into();
        d.overlay_size = (480.0, 480.0);
        (d.zoom.in_frac, d.zoom.out_frac, d.zoom.overlay_alpha_scale) = (0.0, 0.05, 1.0);
        Box::leak(Box::new(d))
    };
    let p = sim.players[0].as_mut().expect("the player");
    p.arsenal.current_mut().def = def;
    p.arsenal.ads = 1.0;
    let name = crate::lui::materials::image("scope_overlay_xpr50");
    for (w, h) in [(1280.0, 720.0), (1920.0, 1200.0), (1024.0, 768.0), (3440.0, 1440.0), (720.0, 1280.0)] {
        let mut prims = Vec::new();
        draw_scope_overlay(&mut prims, w, h, sim.players[0].as_ref().unwrap(), false);
        let art: Vec<_> = prims.iter().filter_map(|q| match q {
            Prim::Image { name: n, x, y, w, h, argb, .. } if *n == name => Some((*x, *y, *w, *h, *argb)),
            _ => None,
        }).collect();
        assert_eq!(art.len(), 1, "{w}x{h}");
        let (x, y, aw, ah, argb) = art[0];
        assert!((aw - ah).abs() < 1e-3, "{w}x{h}: the scope is {aw} x {ah} px, not round");
        assert!((x + aw * 0.5 - w * 0.5).abs() < 1e-3 && (y + ah * 0.5 - h * 0.5).abs() < 1e-3);
        let fill: f32 = prims.iter().map(|q| match q {
            Prim::Rect { w, h, argb: a, .. } => {
                assert_eq!(*a, argb & 0xFF00_0000, "black at the overlay's alpha");
                w * h
            }
            _ => 0.0,
        }).sum();
        assert!((fill + aw * ah - w * h).abs() < 1.0, "{w}x{h}: the fills cover the rest of the screen");
    }
}

#[test]
fn a_stale_table_without_overlay_size_still_draws_the_scope() {
    // A weapon table written before `OverlaySize` reads 0 x 0; BO2's rule would then draw only the
    // fills (a black screen). The HUD falls back to the tuning pack's square and logs once.
    let mut d = crate::weapons::WeaponDef { overlay_material: "scope_overlay_xpr50".into(), ..Default::default() };
    d.zoom.overlay_alpha_scale = 1.0;
    assert_eq!(d.overlay_size, (0.0, 0.0));
    let mut prims = Vec::new();
    scope_overlay(&mut prims, &d, 255, 1280.0, 720.0);
    let name = crate::lui::materials::image("scope_overlay_xpr50");
    let art: Vec<_> = prims.iter().filter_map(|p| match p {
        Prim::Image { name: n, x, y, w, h, .. } if *n == name => Some((*x, *y, *w, *h)),
        _ => None,
    }).collect();
    assert_eq!(art, vec![(280.0, 0.0, 720.0, 720.0)], "the 480 x 480 height-fit square");
    assert!(SCOPE_SIZE_WARNED.load(std::sync::atomic::Ordering::Relaxed), "the fallback is logged");
}

#[test]
fn the_scoped_hud_hides_the_map_compass_and_indicators() {
    // `CoD.BIT_IS_SCOPED` (`FUN_0055a420` → the reticle zoom) hides the minimap container, which
    // holds the map and the compass strip; the damage indicators' gate (`FUN_004adda0`) is the
    // reticle zoom itself. All three switch at once. The ammo area stays; an IR sniper's optic
    // draws no 2D reticle over the overlay.
    let (mut sim, mut client) = fps_state();
    let def = {
        let mut d = sim.players[0].as_ref().expect("the player").arsenal.current_weapon().clone();
        d.overlay_material = "scope_overlay_xpr50".into();
        d.attachments = vec!["ir".into()];
        (d.zoom.in_frac, d.zoom.out_frac, d.zoom.overlay_alpha_scale) = (0.0, 0.05, 1.0);
        Box::leak(Box::new(d))
    };
    sim.players[0].as_mut().expect("the player").arsenal.current_mut().def = def;
    client.hud.lui.damage_from(90.0, 0.0);
    #[derive(Debug, PartialEq)]
    struct Seen {
        compass: bool,
        indicator: bool,
        ammo: bool,
        reticle: bool,
    }
    let seen = |client: &Client, sim: &Sim| {
        let rec = Recorder { has_art: true, ..Default::default() };
        let prims = draw_hud(&rec, client, sim);
        let has = |pred: &dyn Fn(&Prim) -> bool| prims.iter().any(pred);
        let hit = crate::lui::materials::image("hit_direction");
        let digit = crate::lui::materials::image("hud_mp_num_big_slash");
        Seen {
            compass: has(&|p| matches!(p, Prim::Image { name, .. } if name == sprites::COMPASS)),
            indicator: has(&|p| matches!(p, Prim::Quad { name, .. } if *name == hit)),
            ammo: has(&|p| matches!(p, Prim::Image { name, .. } if *name == digit)),
            reticle: has(&|p| matches!(p, Prim::Image { name, .. } if name == sprites::RED_DOT || name.starts_with("mtl_t6_attach_optic"))),
        }
    };
    let set = |sim: &mut Sim, client: &mut Client, ads: f32| {
        let p = sim.players[0].as_mut().expect("the player");
        p.arsenal.ads = ads;
        client.hud.lui.tick(0.016, Some(p));
    };
    let shown = Seen { compass: true, indicator: true, ammo: true, reticle: true };
    let scoped = Seen { compass: false, indicator: false, ammo: true, reticle: false };
    set(&mut sim, &mut client, 0.0);
    assert_eq!(seen(&client, &sim), Seen { reticle: false, ..shown }, "hip: everything, no optic reticle");
    set(&mut sim, &mut client, 0.99);
    assert_eq!(seen(&client, &sim), shown, "aiming in below the (empty) window: the sight's reticle, not scoped");
    set(&mut sim, &mut client, 1.0);
    assert_eq!(seen(&client, &sim), scoped, "the direction flipped at full ADS: scoped");
    set(&mut sim, &mut client, 0.975);
    assert_eq!(seen(&client, &sim), scoped, "aiming out inside the last 5 %: still scoped");
    set(&mut sim, &mut client, 0.94);
    assert_eq!(seen(&client, &sim), shown, "aiming out below the window: back at once");

    // An in-window (IR's 0.016): scoped while aiming in through it.
    let mut d = def.clone();
    d.zoom.in_frac = 0.016;
    sim.players[0].as_mut().expect("the player").arsenal.current_mut().def = Box::leak(Box::new(d.clone()));
    set(&mut sim, &mut client, 0.0);
    set(&mut sim, &mut client, 0.98);
    assert_eq!(seen(&client, &sim), shown, "aiming in, below the window");
    set(&mut sim, &mut client, 0.995);
    assert_eq!(seen(&client, &sim), scoped, "aiming in, inside the window");

    // TV-guided: the overlay hides the indicators, but the scoped bit (map, compass) stays clear.
    d.guided_missile_type = crate::weapons::scope::MISSILE_GUIDANCE_TVGUIDED;
    sim.players[0].as_mut().expect("the player").arsenal.current_mut().def = Box::leak(Box::new(d));
    set(&mut sim, &mut client, 0.995);
    assert_eq!(seen(&client, &sim), Seen { compass: true, ..scoped });
}

#[test]
fn the_crosshair_fades_under_the_overlay_to_the_floor() {
    // `FUN_004b5ea0`: under the overlay the side pieces take `1 − zoom / max(scale, 1)` before
    // `FUN_004b5e40`'s floor, `cg_crosshairAlphaMin` (`DAT_01062db4`, 0.5).
    let (mut sim, client) = fps_state();
    with_bo2_crosshair(&mut sim);
    let p = sim.players[0].as_ref().expect("the player");
    let alpha = |zoom: Option<f32>| {
        let mut prims = Vec::new();
        draw_crosshair(&mut prims, 1280.0, 720.0, 1.0, &client, p, zoom);
        prims.iter().find_map(|q| match q {
            Prim::Quad { argb, name, .. } if name.ends_with("reticle_side_small") => Some(argb >> 24),
            _ => None,
        })
    };
    assert_eq!(alpha(None), Some(255));
    assert_eq!(alpha(Some(0.25)), Some(191), "0.75 of full");
    assert_eq!(alpha(Some(1.0)), Some(128), "floored at cg_crosshairAlphaMin");
}

#[test]
fn falls_back_to_text_without_the_art() {
    let (sim, client) = fps_state();
    let rec = Recorder::default();
    let _ = draw_hud(&rec, &client, &sim);
    let texts = rec.texts.borrow();
    assert!(texts.iter().any(|t| t == "3") && texts.iter().any(|t| t == "0"));
}

/// The held knife of a one-gun class in the hand: BO2 hides the ammo digits for a `melee` weapType
/// (`CoD.AmmoArea.UpdateWeapon` -> `hideAmmo`) and keeps the weapon label and offhand counters.
#[test]
fn the_held_knife_hides_the_ammo_digits_only() {
    let (mut sim, _) = fps_state();
    let mut a = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout { secondary: String::new(), ..crate::weapons::Loadout::test_kit() });
    a.raise_left = 0.0;
    sim.players[0].as_mut().unwrap().arsenal = a;
    let digits = |sim: &Sim| {
        let mut prims = Vec::new();
        ammo::draw(&super::frame::Lui::new(1280.0, 720.0), &mut prims, sim.player(0).unwrap(), 0.0, 0.0, "");
        let names: Vec<String> = prims.iter().filter_map(|p| if let Prim::Image { name, .. } = p { Some(name.clone()) } else { None }).collect();
        (names.iter().filter(|n| n.contains("hud_mp_num_big_")).count(), names.iter().any(|n| n == "ZHBO_Mat_hud_grenadeicon"))
    };
    let (gun, nade) = digits(&sim);
    assert!(gun > 0 && nade, "the gun counts its rounds ({gun})");
    sim.players[0].as_mut().unwrap().arsenal.active = 1;
    assert_eq!(sim.player(0).unwrap().arsenal.current().def.id, "knife_held_mp");
    let (knife, nade) = digits(&sim);
    assert_eq!(knife, 0, "no digits for the knife");
    assert!(nade, "the offhand counters stay");
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
    // An attachment's icon is its table image's converted art (`attachmenttable.csv` column 6).
    assert_eq!(sprites::attachment_icon("reflex").as_deref(), Some("ZHBO_Mat_cac_mods_red_dot"));
    assert_eq!(sprites::attachment_icon("gl").as_deref(), Some("ZHBO_Mat_hud_grenade_launcher"));
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

/// The board lists the other player too (a two-player match): his name, his kills and deaths as
/// this machine saw the sim's events, under the enemies' block; the local player's own row first.
/// (Before, the board had one hard-coded "General" row.)
#[test]
fn the_scoreboard_lists_every_player_in_the_match() {
    let _f = crate::lui::font::lock();
    crate::lui::font::load("");
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(general(2, 1));
    let mut sim = Sim::new();
    let mut client = Client::default();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    sim.take_control(&host, 1, 2);
    sim.choose_class(1, 2, 0, &[]);
    let mut inp = ClientIn::default();
    for f in 1..160 {
        if f == 100 {
            // Seat 1 kills twice and dies once; the local player kills once.
            for kind in [
                crate::sim::EventKind::HitMarker { headshot: false, kill: true, alert: crate::damage::HitAlert::Player },
                crate::sim::EventKind::HitMarker { headshot: true, kill: true, alert: crate::damage::HitAlert::Player },
                crate::sim::EventKind::HitMarker { headshot: false, kill: false, alert: crate::damage::HitAlert::Player },
                crate::sim::EventKind::GeneralDied { general: 2 },
            ] {
                sim.events.push(crate::sim::Event { player: 1, kind });
            }
            sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::HitMarker { headshot: false, kill: true, alert: crate::damage::HitAlert::Player } });
        }
        client.frame_with(&mut sim, Some(&host), &inp);
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    let t = client.seats[1];
    assert_eq!((t.seen, t.kills, t.deaths, t.ally), (true, 2, 1, false));
    client.hud.scoreboard_open = true;
    let rec = Recorder { has_art: true, ..Default::default() };
    let _prims = draw_hud(&rec, &client, &sim);
    let texts = rec.texts.borrow();
    let at = |want: &str| texts.iter().position(|t| t == want).unwrap_or_else(|| panic!("{want} in {texts:?}"));
    // The local row (the HUD's tally: 1 kill, 100) comes before the other player's (200, 2, 1).
    // (The kill feed names him first: his death's line.)
    assert!(at("General") < texts.iter().rposition(|t| t == "Player 2").expect("his row"), "{texts:?}");
    for want in ["100", "200", "2"] {
        at(want);
    }
}

/// A flashed shellshock whites the view out (`FUN_005105c0`: `min(left / whiteFade, 1)`), a full
/// screen rectangle; none without a shock.
#[test]
fn a_flashed_shellshock_whites_the_view_out() {
    let (mut sim, client) = fps_state();
    let rec = Recorder::default();
    let white = |prims: &[Prim]| prims.iter().find_map(|p| match p {
        Prim::Rect { x, y, w, h, argb } if *x == 0.0 && *y == 0.0 && *w == 1280.0 && *h == 720.0 && argb & 0x00ff_ffff == 0x00ff_ffff => Some(argb >> 24),
        _ => None,
    });
    assert_eq!(white(&draw_hud(&rec, &client, &sim)), None);
    sim.players[0].as_mut().unwrap().shock.play(crate::shellshock::names::FLASHBANG, 6.0);
    assert_eq!(white(&draw_hud(&rec, &client, &sim)), Some(255), "a full whiteout");
    sim.players[0].as_mut().unwrap().shock.active.as_mut().unwrap().left = 1.75;
    assert_eq!(white(&draw_hud(&rec, &client, &sim)), Some(128), "half way down its 3.5 s fade (ease(0.5) = 0.5)");
    // When the renderer's shock layer takes the frame it adds the white itself (with the grab).
    crate::nativevm::set_shock_layer_drawn(true);
    assert_eq!(white(&draw_hud(&rec, &client, &sim)), None, "the renderer's layer, not the HUD's");
    // The next client frame resets it until the scene's draw queues the layer again.
    crate::nativevm::set_local_shock(None);
    assert_eq!(white(&draw_hud(&rec, &client, &sim)), Some(128));
}

/// A spectator following a General sees his HUD reduced to what BO2's spectator sees: his ammo,
/// crosshair, compass and score, and the dead-spectate panel naming the switch keys; not the
/// player's own hit marker, scorestreak column or key prompts.
#[test]
fn a_spectators_hud_is_the_followed_generals_reduced() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    crate::sim::tests::spawn(&mut sim, &host, 0, 1);
    let mut client = Client::default();
    let mut inp = ClientIn { local_player: 5, spectating: 1, dt: crate::units::LOGIC_DT, ..Default::default() };
    for f in 1..80 {
        client.frame(&mut sim, &inp);
        sim.step(&host, f);
        inp.logic_frame = f;
    }
    assert!(client.is_following());
    client.hud.hitmarker = 0.5;
    let rec = Recorder { has_art: true, ..Default::default() };
    let mut prims = Vec::new();
    draw(&rec, &mut prims, &client, &sim, client.viewed);
    render(&prims, &rec);
    let images = rec.images.borrow();
    assert!(images.iter().any(|i| i == "ZHBO_Mat_hud_shoutcasting_change_tab"), "the panel");
    assert!(images.iter().any(|i| i.starts_with("ZHBO_Mat_hud_mp_vis_left_lower")), "his score panel");
    assert!(!images.iter().any(|i| i.contains("damage_feedback")), "no hit marker");
    assert!(!images.iter().any(|i| i.contains("hud_mp_vis_ks_dpad")), "no scorestreak column");
    let texts = rec.texts.borrow();
    assert!(texts.iter().any(|t| t.contains("[LMB]") && t.contains("[RMB]") && t.contains("Spectating")), "{texts:?}");
}

/// Every hurt with a way fills a damage-direction slot pointing back at its source (G7
/// `FUN_004e8ab0`), the overhead General's too, and the slot draws as `hit_direction`.
#[test]
fn a_hurt_points_the_indicator_back_at_its_source() {
    let (mut sim, mut client) = fps_state();
    client.yaw = 0.0;
    // A blow travelling +X: it came from behind (yaw 180), give or take the 10° jitter.
    sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Damaged { amount: 10.0, dir: Vec3::new(1.0, 0.0, 0.0) } });
    // And one with no way (BO2's yaw byte 255): no slot.
    sim.events.push(crate::sim::Event { player: 0, kind: crate::sim::EventKind::Damaged { amount: 10.0, dir: Vec3::ZERO } });
    client.frame(&mut sim, &ClientIn::default());
    let live = client.hud.lui.live_indicators();
    assert_eq!(live.len(), 1, "{live:?}");
    assert!((live[0].0 - 180.0).abs() <= 10.0, "{live:?}");
    let rec = Recorder { has_art: true, ..Default::default() };
    let prims = draw_hud(&rec, &client, &sim);
    assert!(prims.iter().any(|q| matches!(q, Prim::Quad { name, .. } if *name == crate::lui::materials::image("hit_direction"))));
}

/// The kill feed is built from the sim's events alone, so two machines given the same events in the
/// same order show the same lines (the lockstep sim gives every machine the same events).
#[test]
fn the_kill_feed_is_the_same_on_every_machine() {
    let rifle = crate::weapons::catalog().all().first().copied();
    let events = [
        crate::sim::Event { player: 1, kind: crate::sim::EventKind::KilledBy { dmg: crate::death::DmgType::Normal, dir: Vec3::new(1.0, 0.0, 0.0), by: Some(0), how: crate::damage::MeansOfDeath::RifleBullet, weapon: rifle } },
        crate::sim::Event { player: 1, kind: crate::sim::EventKind::GeneralDied { general: 2 } },
        crate::sim::Event { player: 0, kind: crate::sim::EventKind::GeneralDied { general: 1 } },
    ];
    let feed = |local: i32| {
        let (mut sim, mut client) = fps_state();
        sim.events.extend(events);
        client.frame(&mut sim, &ClientIn { local_player: local, ..Default::default() });
        client.feedback.obits.lines.iter().map(|l| (l.attacker, l.victim, l.icon.material.clone())).collect::<Vec<_>>()
    };
    let a = feed(0);
    let b = feed(1);
    assert_eq!(a, b);
    assert_eq!(a.len(), 2, "one line per death: {a:?}");
    assert_eq!(a[1], (None, 0, "killicondied".to_string()), "a death no blow explains is the world's");
}

/// The grenade indicator collects the player's own and enemies' resting grenades in range, never
/// an ally's or a flying one.
#[test]
fn the_grenade_indicator_shows_own_and_enemy_grenades_at_rest_in_range() {
    let (mut sim, _client) = fps_state();
    let p = sim.players[0].clone().expect("the player");
    let mut def = crate::weapons::WeaponDef { timed_detonation: true, explosion_radius: crate::units::inches(256.0), ..Default::default() };
    def.traits.weap_type = crate::weapons::WeapType::Grenade;
    let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
    let host = FakeHost::default();
    let at = p.body.pos + Vec3::new(crate::units::inches(100.0), 0.0, 0.0);
    let mut g = crate::projectiles::Projectile::launch(def, p.general, at, Vec3::new(1.0, 0.0, 0.0), 3.0, &host, 0.0);
    g.vel = Vec3::ZERO;
    g.pos = at;
    sim.projectiles = vec![(0, g.clone()), (1, g.clone()), (2, g.clone())];
    let ally = |seat: usize| seat == 2;
    let shown = crate::hud::grenade::collect(&sim, 0, &p, &ally);
    assert_eq!(shown.len(), 2, "own and the enemy's, not the ally's");
    assert_eq!(shown[0].icon, "hud_grenadeicon");
    // Flying fast: not shown; far beyond its 256-inch range: not shown.
    let mut fast = g.clone();
    fast.vel = Vec3::new(crate::units::inches(400.0), 0.0, 0.0);
    let mut far = g;
    far.pos = p.body.pos + Vec3::new(crate::units::inches(400.0), 0.0, 0.0);
    sim.projectiles = vec![(0, fast), (0, far)];
    assert!(crate::hud::grenade::collect(&sim, 0, &p, &ally).is_empty());
}
