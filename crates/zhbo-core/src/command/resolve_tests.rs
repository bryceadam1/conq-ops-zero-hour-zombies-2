//! The resolver, the world snapshot and the command line, over a small fake world: the General at
//! the origin, three Rangers, a dozer, a war factory in control group 3, two enemy rebels and an
//! enemy war factory seen before and now in fog.

use std::cell::RefCell;

use super::session::{run, typed_char, Host, Session, Viewer};
use super::world::{availability as av, button as b, canonical_name, cmdflags as f, context as cx, issued, orderkind as k, playerset, plural, relation as rel, CmdButton, CmdObject, CmdOrder, World};
use crate::input::{Key, KeySet};
use crate::math::Vec3;

const GENERAL: u32 = 1;

fn obj(id: u32, tmpl: u32, flags: u32, relation: i32, x: f32, y: f32) -> CmdObject {
    CmdObject { id, tmpl, flags, relation, pos: Vec3::new(x, y, 0.0), health: 1.0, radius: 10.0, ..CmdObject::default() }
}

fn objects() -> Vec<CmdObject> {
    let ranger = f::SELECTABLE | f::SELECT_ALL | f::INFANTRY | f::IDLE;
    let mut v = vec![
        obj(GENERAL, 900, f::GENERAL | f::INFANTRY, rel::OWN, 0.0, 0.0),
        obj(10, 100, ranger, rel::OWN, 100.0, 0.0),
        obj(11, 100, ranger, rel::OWN, 50.0, 50.0),
        obj(12, 100, ranger, rel::OWN, 0.0, 200.0),
        obj(20, 101, f::SELECTABLE | f::DOZER | f::VEHICLE, rel::OWN, 30.0, -40.0),
        obj(30, 102, f::SELECTABLE | f::STRUCTURE, rel::OWN, -200.0, 0.0),
        obj(40, 200, f::INFANTRY, rel::ENEMY, 300.0, 0.0),
        obj(41, 200, f::INFANTRY, rel::ENEMY, 500.0, 0.0),
        obj(50, 102, f::STRUCTURE | f::GHOST, rel::ENEMY, 0.0, -600.0),
    ];
    v[3].health = 0.5;
    v[2].veterancy = 1;
    v[5].groups = 1 << 3;
    // A second war factory far off with a unit queued, a Humvee with a Ranger riding in it.
    let mut second = obj(31, 102, f::SELECTABLE | f::STRUCTURE, rel::OWN, -900.0, 0.0);
    second.queue = 1;
    v.push(second);
    v.push(obj(21, 105, f::SELECTABLE | f::SELECT_ALL | f::VEHICLE | f::LOADED, rel::OWN, 0.0, -100.0));
    let mut rider = obj(13, 100, f::INFANTRY | f::CONTAINED, rel::OWN, 0.0, -100.0);
    rider.container = 21;
    v.push(rider);
    v
}

fn button(owner: u32, slot: i32, code: i32, tmpl: u32, name: &str) -> CmdButton {
    let mut out = CmdButton { owner, slot, code, tmpl, availability: av::AVAILABLE, ..CmdButton::default() };
    out.name[..name.len()].copy_from_slice(name.as_bytes());
    out
}

/// The war factories build Crusaders and research Composite Armor (unaffordable), the dozer builds
/// a Power Plant, the Rangers switch to flash-bangs, the second factory can be sold, the player has
/// the Carpet Bomb shortcut (aimed at a spot) and a promotion to buy.
fn buttons() -> Vec<CmdButton> {
    let mut armor = button(30, 3, b::PLAYER_UPGRADE, 0, "Composite Armor");
    armor.availability = av::CANT_AFFORD;
    let mut bomb = button(0, 0, b::SPECIAL_POWER_FROM_SHORTCUT, 0, "&Carpet Bomb");
    bomb.set = playerset::SHORTCUTS;
    bomb.options = super::world::option::NEED_TARGET_POS;
    let mut science = button(0, 2, b::PURCHASE_SCIENCE, 0, "Advanced Training");
    science.set = playerset::RANK1;
    let mut v = vec![
        button(30, 0, b::UNIT_BUILD, 103, ""),
        button(31, 0, b::UNIT_BUILD, 103, ""),
        armor,
        button(20, 4, b::DOZER_CONSTRUCT, 104, ""),
        button(31, 9, b::SELL, 0, ""),
        bomb,
        science,
    ];
    for r in [10, 11, 12] {
        v.push(button(r, 1, b::SWITCH_WEAPON, 0, "Flash-Bang Grenades"));
    }
    v
}

fn name(tmpl: u32) -> Option<String> {
    match tmpl {
        100 => Some("Ranger".into()),
        101 => Some("Dozer".into()),
        102 => Some("&War Factory".into()),
        103 => Some("Crusader Tank".into()),
        104 => Some("Power Plant".into()),
        105 => Some("Humvee".into()),
        200 => Some("Rebel".into()),
        900 => Some("General".into()),
        _ => None,
    }
}

struct Fake {
    objects: Vec<CmdObject>,
    buttons: Vec<CmdButton>,
    issued: RefCell<Vec<(CmdOrder, Vec<u32>)>>,
    /// The engine's answer to each order kind (sent unless set).
    answers: Vec<(i32, i32)>,
}

impl Fake {
    fn new() -> Fake {
        Fake { objects: objects(), buttons: buttons(), issued: RefCell::new(Vec::new()), answers: Vec::new() }
    }
}

impl Host for Fake {
    fn objects(&self) -> Vec<CmdObject> {
        self.objects.clone()
    }
    fn template_name(&self, tmpl: u32) -> Option<String> {
        name(tmpl)
    }
    fn buttons(&self) -> Vec<CmdButton> {
        self.buttons.clone()
    }
    fn issue(&self, order: &CmdOrder, ids: &[u32]) -> i32 {
        self.issued.borrow_mut().push((*order, ids.to_vec()));
        self.answers.iter().find(|(kind, _)| *kind == order.kind).map_or(issued::SENT, |&(_, code)| code)
    }
    fn ground_height(&self, _x: f32, _y: f32) -> f32 {
        0.0
    }
}

/// The General at the origin facing east (+x), the crosshair level along +x at 5 up.
fn viewer() -> Viewer {
    Viewer { general: GENERAL, feet: Vec3::ZERO, yaw: 0.0, eye: Vec3::new(0.0, 0.0, 5.0), dir: Vec3::new(1.0, 0.0, 0.0) }
}

/// Run `line` in `session`; the orders it sent, or the refusal.
fn command(session: &mut Session, host: &Fake, line: &str) -> Result<Vec<(CmdOrder, Vec<u32>)>, String> {
    host.issued.borrow_mut().clear();
    let log = run(session, line, host, viewer(), &crate::tuning::Tuning::default());
    match log.split_once(" refused: ") {
        Some((_, why)) => Err(why.to_string()),
        None => Ok(host.issued.borrow().clone()),
    }
}

fn one(line: &str) -> (CmdOrder, Vec<u32>) {
    let host = Fake::new();
    let orders = command(&mut Session::default(), &host, line).unwrap_or_else(|e| panic!("'{line}' refused: {e}"));
    assert_eq!(orders.len(), 1, "'{line}': {orders:?}");
    orders[0]
        .clone()
}

fn refusal(line: &str) -> String {
    command(&mut Session::default(), &Fake::new(), line).expect_err(line)
}

#[test]
fn the_world_names_its_types_and_leaves_out_grammar_words() {
    let mut objs = objects();
    objs.push(obj(60, 300, f::SELECTABLE, rel::OWN, 0.0, 0.0));
    let world = World::new(objs, buttons(), &|t| if t == 300 { Some("Stop".into()) } else { name(t) });
    let names: Vec<&str> = world.types.iter().map(|(n, _)| n.singular.as_str()).collect();
    // The types on the map and the ones the buttons build (the Crusader, the Power Plant).
    assert_eq!(names, ["crusader tank", "dozer", "general", "humvee", "power plant", "ranger", "rebel", "war factory"]);
    assert_eq!(world.templates("war factory"), [102]);
    assert_eq!(world.refused_names, ["'stop': a word of the grammar"]);
    // The class words stay, but none a type took.
    let vocab = world.vocabulary(&[]);
    let classes: Vec<&str> = vocab.classes.iter().map(|n| n.singular.as_str()).collect();
    assert_eq!(classes, ["infantry", "vehicle", "aircraft", "building", "builder", "harvester"]);
    // The buttons' names, each in its list.
    assert_eq!(vocab.upgrades, ["composite armor"]);
    assert_eq!(vocab.powers, ["carpet bomb"]);
    assert_eq!(vocab.sciences, ["advanced training"]);
    assert_eq!(vocab.modes, ["flash-bang grenades"]);
    assert_eq!(vocab.problems(), Vec::<String>::new());
}

#[test]
fn a_power_may_share_a_units_name_but_an_upgrade_may_not() {
    // The Scud Storm is a building and its power; an upgrade named like a unit would clash with it
    // after `cancel`.
    let mut objs = objects();
    objs.push(obj(70, 106, f::SELECTABLE | f::STRUCTURE, rel::OWN, 0.0, 300.0));
    let mut storm = button(70, 0, b::SPECIAL_POWER, 0, "Scud Storm");
    storm.options = super::world::option::NEED_TARGET_POS;
    let world = World::new(objs.clone(), vec![storm, button(30, 5, b::PLAYER_UPGRADE, 0, "Ranger")], &|t| if t == 106 { Some("Scud Storm".into()) } else { name(t) });
    let vocab = world.vocabulary(&[]);
    assert_eq!(vocab.powers, ["scud storm"]);
    assert!(vocab.upgrades.is_empty());
    assert_eq!(world.refused_names, ["'ranger': an upgrade with a unit's name"]);
    assert_eq!(vocab.problems(), Vec::<String>::new());
    let mut host = Fake::new();
    host.objects = objs;
    host.buttons = vec![storm];
    let mut s = Session::default();
    let (o, ids) = command(&mut s, &host, "use scud storm north of me").unwrap()[0].clone();
    assert_eq!((o.kind, ids), (k::BUTTON, vec![70]));
}

#[test]
fn display_names_become_canonical_words_with_regular_plurals() {
    assert_eq!(canonical_name("&War Factory").as_deref(), Some("war factory"));
    assert_eq!(canonical_name("  Crusader   Tank ").as_deref(), Some("crusader tank"));
    assert_eq!(canonical_name("MISSING: 'OBJECT:Foo'"), None);
    assert_eq!(canonical_name(""), None);
    // Punctuation is a word break; a hyphen inside a word stays.
    assert_eq!(canonical_name("Upgrade: Flash-Bang (Rangers)").as_deref(), Some("upgrade flash-bang rangers"));
    assert_eq!(canonical_name("A-10 Strike!").as_deref(), Some("a-10 strike"));
    assert_eq!(plural("war factory"), "war factories");
    assert_eq!(plural("humvee"), "humvees");
    assert_eq!(plural("supply truck"), "supply trucks");
    assert_eq!(plural("bus"), "buses");
    assert_eq!(plural("patriot missile battery"), "patriot missile batteries");
    assert_eq!(plural("key"), "keys");
}

#[test]
fn selections_resolve_nearest_first() {
    let (o, ids) = one("rangers move to me");
    assert_eq!((o.kind, o.pos), (k::MOVE, Vec3::ZERO));
    assert_eq!(ids, [11, 10, 12]);
    // `units` is Zero Hour's select-all: no dozer, no building, never the General (the Humvee, at
    // the same distance as Ranger 10, after him by id).
    assert_eq!(one("units stop").1, [11, 10, 21, 12]);
    assert_eq!(one("builders stop").1, [20]);
    assert_eq!(one("buildings stop").1, [30, 31]);
    assert_eq!(one("group 3 stop").1, [30]);
    assert_eq!(refusal("group 4 stop"), "group 4 is empty");
}

#[test]
fn superlatives_adjectives_filters_and_set_operations() {
    assert_eq!(one("the nearest ranger stop").1, [11]);
    assert_eq!(one("the farthest ranger stop").1, [12]);
    assert_eq!(one("the northernmost ranger stop").1, [12]);
    assert_eq!(one("the easternmost ranger stop").1, [10]);
    // The weakest first; the tie at full health goes to the nearer.
    assert_eq!(one("the 2 weakest rangers stop").1, [12, 11]);
    assert_eq!(one("half the rangers stop").1, [11, 10]);
    assert_eq!(one("veteran rangers stop").1, [11]);
    assert_eq!(one("damaged rangers stop").1, [12]);
    // Within CommandNearRadius (150) of the dozer at (30, -40).
    assert_eq!(one("rangers near the dozer stop").1, [11, 10]);
    assert_eq!(one("rangers except the farthest ranger stop").1, [11, 10]);
    assert_eq!(one("the dozer and the nearest ranger stop").1, [20, 11]);
    assert_eq!(refusal("rangers except rangers stop"), "nobody is left to command");
    assert_eq!(refusal("elite rangers stop"), "you have no elite rangers");
}

#[test]
fn targets_resolve_to_objects_and_points() {
    let (o, ids) = one("the nearest ranger attack the nearest enemy rebel");
    assert_eq!((o.kind, o.target, ids), (k::ATTACK, 40, vec![11]));
    // A building seen before and now in fog is still a target (rule 1).
    assert_eq!(one("rangers attack the enemy war factory").0.target, 50);
    assert_eq!(one("rangers guard my war factory").0, CmdOrder { kind: k::GUARD_OBJECT, target: 30, pos: Vec3::new(-200.0, 0.0, 0.0), slot: -1, ..CmdOrder::default() });
    assert_eq!(one("rangers guard me").0.target, GENERAL);
    // Offsets: north is +y on the radar; behind me is against his facing (east).
    assert_eq!(one("rangers move north of me").0.pos, Vec3::new(0.0, 150.0, 0.0));
    assert_eq!(one("rangers move behind me").0.pos, Vec3::new(-150.0, 0.0, 0.0));
    // In front of the dozer: from it, back along the line from him to it (0.6, -0.8).
    let p = one("rangers move in front of the dozer").0.pos;
    assert!((p - Vec3::new(-60.0, 80.0, 0.0)).length() < 1e-3, "{p:?}");
    assert_eq!(one("rangers force attack north of me").0.kind, k::FORCE_ATTACK_GROUND);
    assert_eq!(one("rangers force attack the nearest enemy rebel").0.kind, k::FORCE_ATTACK);
    assert_eq!(refusal("rangers attack the enemy dozer"), "can't see the enemy dozer");
}

#[test]
fn the_crosshair_names_that_and_there() {
    // The crosshair runs along +x: the Ranger at (100, 0) is the first thing on it.
    assert_eq!(one("that move to me").1, [10]);
    assert_eq!(one("rangers move to that").0.target, 0, "a move takes the object's place");
    assert_eq!(one("rangers move to that").0.pos, Vec3::new(100.0, 0.0, 0.0));
    let mut host = Fake::new();
    host.objects.retain(|o| o.id != 10);
    let mut s = Session::default();
    let orders = command(&mut s, &host, "rangers attack that").unwrap();
    assert_eq!(orders[0].0.target, 40, "with that Ranger gone, the rebel behind him");
    assert_eq!(command(&mut s, &host, "that stop").unwrap_err(), "that isn't one of your units");
}

#[test]
fn there_is_where_the_crosshair_meets_the_ground() {
    let host = Fake::new();
    let mut v = viewer();
    v.eye = Vec3::new(0.0, 0.0, 100.0);
    v.dir = Vec3::new(1.0, 0.0, -1.0) * std::f32::consts::FRAC_1_SQRT_2;
    let mut s = Session::default();
    run(&mut s, "rangers move there", &host, v, &crate::tuning::Tuning::default());
    let (o, _) = host.issued.borrow()[0];
    assert!((o.pos.x - 100.0).abs() < 4.0 && o.pos.y.abs() < 1e-3 && o.pos.z == 0.0, "{o:?}");
    v.dir = Vec3::new(0.0, 0.0, 1.0);
    let log = run(&mut s, "rangers move there", &host, v, &crate::tuning::Tuning::default());
    assert!(log.ends_with("refused: the crosshair isn't on the ground"), "{log}");
}

#[test]
fn them_teams_groups_and_then() {
    let host = Fake::new();
    let mut s = Session::default();
    assert_eq!(command(&mut s, &host, "stop").unwrap_err(), "say who: no units were commanded yet");
    command(&mut s, &host, "the 2 nearest rangers move to me").unwrap();
    // No selection, or `them`: the last one.
    assert_eq!(command(&mut s, &host, "stop").unwrap()[0].1, [11, 10]);
    assert_eq!(command(&mut s, &host, "them and the dozer stop").unwrap()[0].1, [11, 10, 20]);
    // A team name joins the vocabulary.
    command(&mut s, &host, "veteran rangers name them alpha team").unwrap();
    assert_eq!(command(&mut s, &host, "alpha team stop").unwrap()[0].1, [11]);
    assert_eq!(command(&mut s, &host, "rangers make them group 2").unwrap()[0].0, CmdOrder { kind: k::MAKE_GROUP, arg: 2, slot: -1, ..CmdOrder::default() });
    // `then` queues moves.
    let orders = command(&mut s, &host, "rangers move to me then move to the dozer").unwrap();
    assert_eq!(orders.iter().map(|(o, _)| (o.kind, o.queued)).collect::<Vec<_>>(), [(k::MOVE, 0), (k::MOVE, 1)]);
    assert_eq!(command(&mut s, &host, "rangers move to me then stop").unwrap_err(), "only a move can follow 'then' for now, not 'stop'");
    // The right-click verbs now go to the engine to check (here: the Ranger under the crosshair).
    assert_eq!(command(&mut s, &host, "rangers capture that").unwrap()[0].0.kind, k::CONTEXT);
}

#[test]
fn a_refused_line_changes_nothing_and_says_why() {
    let host = Fake::new();
    let mut s = Session::default();
    command(&mut s, &host, "rangers move to me").unwrap();
    host.issued.borrow_mut().clear();
    let log = run(&mut s, "rangres stop", &host, viewer(), &crate::tuning::Tuning::default());
    assert!(log.starts_with("command: 'rangres stop' refused: didn't expect 'rangres'"), "{log}");
    assert!(host.issued.borrow().is_empty());
    assert_eq!(s.memory.last, [11, 10, 12]);
    let echo = s.echo.as_ref().unwrap();
    assert!(!echo.ok && echo.text.starts_with("rangres stop: didn't expect 'rangres'"), "{echo:?}");
    let log = run(&mut s, "rangers move to me", &host, viewer(), &crate::tuning::Tuning::default());
    assert_eq!(log, "command: 'rangers move to me' -> 3 units: rangers move to me, 1 order(s)");
    assert!(s.echo.as_ref().unwrap().ok);
}

fn press(s: &mut Session, keys: &[Key]) -> bool {
    let mut held = KeySet::default();
    for &key in keys {
        held.set_down(key, true);
    }
    let a = s.keys(&held, true);
    let b = s.keys(&KeySet::default(), true);
    a || b
}

#[test]
fn the_command_line_types_submits_and_cancels() {
    let mut s = Session::default();
    assert!(!press(&mut s, &[Key::W]), "closed, a key belongs to first person");
    assert!(press(&mut s, &[Key::Enter]) && s.open);
    for key in [Key::A, Key::T, Key::T, Key::Minus, Key::X, Key::Backspace, Key::Space, Key::Key3, Key::Key0] {
        press(&mut s, &[key]);
    }
    assert_eq!(s.line, "att- 30");
    assert!(press(&mut s, &[Key::Enter]));
    assert!(!s.open && s.swallow);
    assert_eq!(s.take_submitted().as_deref(), Some("att- 30"));
    assert_eq!(s.take_submitted(), None);

    press(&mut s, &[Key::Enter]);
    press(&mut s, &[Key::S]);
    press(&mut s, &[Key::Escape]);
    assert!(!s.open);
    assert_eq!(s.take_submitted(), None, "Escape submits nothing");

    // Out of first person (or behind a menu) the line closes.
    press(&mut s, &[Key::Enter]);
    assert!(s.open);
    assert!(!s.keys(&KeySet::default(), false));
    assert!(!s.open);
    assert_eq!(typed_char(Key::Key1), Some('1'));
    assert_eq!(typed_char(Key::Key6), Some('6'));
    assert_eq!(typed_char(Key::Enter), None);
}

#[test]
fn build_queues_at_the_shortest_queue_and_constructs_with_the_nearest_idle_builder() {
    let host = Fake::new();
    let mut s = Session::default();
    // Factory 30 (empty, near) twice, then factory 31 (one queued) once the queues tie and the
    // nearer one has taken its turn.
    let orders = command(&mut s, &host, "build 3 crusader tanks").unwrap();
    assert_eq!(orders.iter().map(|(o, ids)| (o.kind, o.slot, ids[0])).collect::<Vec<_>>(), [(k::BUTTON, 0, 30), (k::BUTTON, 0, 30), (k::BUTTON, 0, 31)]);
    // No units named: the ones that acted are `them`.
    assert_eq!(s.memory.last, [30, 31]);
    assert_eq!(command(&mut s, &host, "the war factory build crusader tank").unwrap()[0].1, [30]);

    // Construction: the dozer, at the place named; `facing east` is angle 0.
    let (o, ids) = command(&mut s, &host, "the dozer build power plant near me facing east").unwrap()[0].clone();
    assert_eq!((o.kind, o.tmpl, o.pos, o.angle, ids), (k::CONSTRUCT, 104, Vec3::ZERO, 0.0, vec![20]));
    // Implied builder, no place: at the builder, the template's own angle (NaN for the bridge).
    let (o, ids) = command(&mut s, &host, "build power plant").unwrap()[0].clone();
    assert_eq!((o.pos, ids), (Vec3::new(30.0, -40.0, 0.0), vec![20]));
    assert!(o.angle.is_nan());
    assert_eq!(command(&mut s, &host, "build 2 power plants").unwrap_err(), "build one building at a time");
    // A line (a wall): both ends go to the bridge, which sends the line message for a LINEBUILD
    // template and refuses one for anything else.
    let (o, _) = command(&mut s, &host, "the dozer build power plant from me to north of me").unwrap()[0].clone();
    assert_eq!((o.pos, o.end, o.arg), (Vec3::ZERO, Vec3::new(0.0, 150.0, 0.0), 1));
    assert_eq!(command(&mut s, &host, "build crusader tank from me to north of me").unwrap_err(), "only a building is built from one place to another, not build crusader tank");
    assert_eq!(command(&mut s, &host, "rangers build crusader tank").unwrap_err(), "none of them can build crusader tank");
}

#[test]
fn powers_promotions_upgrades_and_modes_press_their_buttons() {
    let host = Fake::new();
    let mut s = Session::default();
    let (o, ids) = command(&mut s, &host, "use carpet bomb north of me").unwrap()[0].clone();
    assert_eq!((o.kind, o.set, o.slot, o.pos, ids.len()), (k::BUTTON, playerset::SHORTCUTS, 0, Vec3::new(0.0, 150.0, 0.0), 0));
    assert_eq!(command(&mut s, &host, "use carpet bomb").unwrap_err(), "'use' needs a target (a spot or a unit)");
    let (o, _) = command(&mut s, &host, "purchase advanced training").unwrap()[0].clone();
    assert_eq!((o.set, o.slot), (playerset::RANK1, 2));
    assert_eq!(command(&mut s, &host, "upgrade composite armor").unwrap_err(), "you can't afford to upgrade composite armor");
    let orders = command(&mut s, &host, "rangers switch to flash-bang grenades").unwrap();
    assert_eq!(orders.iter().map(|(_, ids)| ids[0]).collect::<Vec<_>>(), [11, 10, 12]);
    assert_eq!(command(&mut s, &host, "rangers sell").unwrap_err(), "none of them can sell");
    assert_eq!(command(&mut s, &host, "war factories sell").unwrap()[0].1, [31]);
    assert!(command(&mut s, &host, "rangers switch to flash-bang grenades there").unwrap_err().starts_with("didn't expect 'there'"));
}

#[test]
fn right_click_verbs_say_what_they_allow() {
    let (o, ids) = one("rangers capture the enemy war factory");
    assert_eq!((o.kind, o.target, o.arg, ids), (k::CONTEXT, 50, cx::CAPTURE | cx::SPECIAL_POWER, vec![11, 10, 12]));
    assert_eq!(one("rangers act on that").0.arg, cx::ANY);
    // On open ground: the right-click at the spot, no target object.
    let (o, _) = one("rangers act on north of me");
    assert_eq!((o.kind, o.target, o.arg, o.pos), (k::CONTEXT, 0, cx::ANY, Vec3::new(0.0, 150.0, 0.0)));
    assert_eq!(one("the dozer repair my war factory").0.arg, cx::REPAIR);
    let (o, _) = one("the war factory set rally point north of me");
    assert_eq!((o.kind, o.target, o.arg, o.pos), (k::CONTEXT, 0, cx::RALLY_POINT, Vec3::new(0.0, 150.0, 0.0)));
    assert_eq!(one("rangers guard without pursuit me").0.arg, 1);
    assert_eq!(one("rangers guard against aircraft north of me").0, CmdOrder { kind: k::GUARD_POSITION, pos: Vec3::new(0.0, 150.0, 0.0), arg: 2, slot: -1, ..CmdOrder::default() });
    assert_eq!(one("rangers scatter").0.kind, k::SCATTER);
    assert_eq!(one("rangers place beacon north of me").0.kind, k::PLACE_BEACON);
}

#[test]
fn cancel_and_unload_name_what_is_queued_and_who_rides() {
    let (o, ids) = one("cancel crusader tank");
    assert_eq!((o.kind, o.tmpl, ids), (k::CANCEL, 103, vec![30, 31]));
    let (o, ids) = one("the humvee unload ranger");
    assert_eq!((o.kind, o.target, ids), (k::UNLOAD, 13, vec![21]));
    assert_eq!(refusal("the dozer unload ranger"), "no ranger riding in them");
    // A passenger is never selected or targeted: three Rangers, not four.
    assert_eq!(one("rangers stop").1, [11, 10, 12]);
}

#[test]
fn what_the_engine_refuses_is_said_and_a_command_none_of_whose_orders_went_out_changes_nothing() {
    let mut host = Fake::new();
    host.answers = vec![(k::CONSTRUCT, issued::NO_PLACE), (k::BUTTON, issued::QUEUE_FULL)];
    let mut s = Session::default();
    assert_eq!(command(&mut s, &host, "build power plant").unwrap_err(), "there's no room to build it there");
    assert!(s.memory.last.is_empty());
    host.answers = vec![(k::CONTEXT, issued::OTHER_ACTION)];
    assert_eq!(command(&mut s, &host, "rangers capture the enemy war factory").unwrap_err(), "that would do something else");
    // Part of a command going out is said too.
    host.answers.clear();
    let log = run(&mut s, "rangers move to me", &host, viewer(), &crate::tuning::Tuning::default());
    assert!(log.contains("3 order(s)") || log.contains("1 order(s)"), "{log}");
}
