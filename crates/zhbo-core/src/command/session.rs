//! The first-person command line and command menu (docs/COMMANDS.md section 8) and running a
//! submitted command against the engine.
//!
//! The menu (its key, Z by default) builds a command a word or phrase at a time from the
//! completions the grammar allows next ([`super::parse::completions`]), with live counts: the wheel
//! (or Up/Down) walks the options, the left button (or Enter) picks one, the right button (or
//! Backspace) takes the last pick back, and `(done)` runs the command. While it is up it borrows
//! the wheel and the mouse buttons; movement and look stay with first person.
//!
//! In first person, Enter opens the line; letters, digits, space and `-` type, Backspace deletes,
//! Enter submits and Escape cancels. While it is open it holds the keyboard: the client sends no
//! movement or look, and the bridge eats Escape (`menu_open`). A submitted line is run after the
//! client frame by [`run`], which has the engine's services at hand: it is parsed, resolved
//! against a fresh [`World`] snapshot and sent as engine orders, and the echo line says what was
//! done or why not.

use super::resolve::{resolve, Memory, Settings};
use super::world::{issued, CmdButton, CmdObject, CmdOrder, World};
use crate::input::{Key, KeySet};
use crate::math::Vec3;

/// How long the echo line stays up, seconds.
pub const ECHO_TIME: f32 = 5.0;
/// The longest line the entry takes, characters.
pub const LINE_MAX: usize = 120;

/// The menu's option that runs the command built so far.
pub const DONE: &str = "(done)";

/// One option of the command menu: a word or phrase, and how many of the player's objects it
/// names, where it names some.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuOption {
    pub text: String,
    pub count: Option<usize>,
}

/// The command menu while it is up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    /// The words and phrases picked so far: the command's canonical text, in order.
    pub picks: Vec<String>,
    pub options: Vec<MenuOption>,
    pub highlight: usize,
    /// The picks changed: the options are recomputed after the frame ([`refresh_menu`]).
    pub dirty: bool,
}

impl Menu {
    pub fn text(&self) -> String {
        self.picks.join(" ")
    }
}

/// What the last command did, for the echo line.
#[derive(Clone, Debug, PartialEq)]
pub struct Echo {
    pub text: String,
    pub ok: bool,
    pub age: f32,
}

/// The command line's state, kept by the client.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    /// The line is up and holds the keyboard.
    pub open: bool,
    pub line: String,
    pub memory: Memory,
    pub echo: Option<Echo>,
    /// A line submitted this frame, for [`run`].
    submitted: Option<String>,
    /// The keys down last frame, for edges.
    prev: KeySet,
    /// Keys down when the line closed: ignored until released, so the Enter or the last letter
    /// does not reach first person.
    pub(crate) swallow: bool,
    /// The command menu, while it is up.
    pub menu: Option<Menu>,
    /// The map's places were logged (once per game, with the first command).
    places_logged: bool,
    /// The keys down last frame, for the menu's edges.
    menu_prev: KeySet,
    /// The push-to-talk key is held (docs/COMMANDS.md section 11).
    voice_held: bool,
    /// It went down (true) or came up (false) this frame, for the voice link.
    voice_edge: Option<bool>,
}

/// The character a key types into the line, if any.
pub fn typed_char(k: Key) -> Option<char> {
    match k as u8 {
        0..=25 => Some((b'a' + k as u8) as char),
        26..=31 => Some((b'1' + (k as u8 - 26)) as char),
        45..=48 => Some(['7', '8', '9', '0'][(k as u8 - 45) as usize]),
        _ if k == Key::Space => Some(' '),
        _ if k == Key::Minus => Some('-'),
        _ => None,
    }
}

impl Session {
    /// One frame of keys while the player is in first person and free to type (`free`: no class
    /// screen, field computer or spectating). Returns whether the line holds the keyboard this
    /// frame (the frame it closes included).
    pub fn keys(&mut self, now: &KeySet, free: bool) -> bool {
        let prev = std::mem::replace(&mut self.prev, *now);
        let edge = |k: Key| now.is_down(k) && !prev.is_down(k);
        if !free {
            self.open = false;
            return false;
        }
        if !self.open {
            if edge(Key::Enter) {
                self.open = true;
                self.line.clear();
                return true;
            }
            return false;
        }
        if edge(Key::Escape) {
            self.close();
            return true;
        }
        if edge(Key::Enter) {
            let line = self.line.trim().to_string();
            if !line.is_empty() {
                self.submitted = Some(line);
            }
            self.close();
            return true;
        }
        if edge(Key::Backspace) {
            self.line.pop();
        }
        for k in Key::ALL.iter().copied() {
            if edge(k) {
                if let Some(c) = typed_char(k) {
                    if self.line.chars().count() < LINE_MAX {
                        self.line.push(c);
                    }
                }
            }
        }
        true
    }

    fn close(&mut self) {
        self.open = false;
        self.line.clear();
        self.swallow = true;
    }

    /// One frame of the command menu (`menu_key` toggles it; `free` as for [`Session::keys`], and
    /// never while the typed line is up). Returns whether the menu holds the wheel and the mouse
    /// buttons this frame (the frame it closes included).
    pub fn menu_input(&mut self, now: &KeySet, wheel: f32, free: bool, menu_key: Key) -> bool {
        let prev = std::mem::replace(&mut self.menu_prev, *now);
        let edge = |k: Key| now.is_down(k) && !prev.is_down(k);
        if !free || self.open {
            self.menu = None;
            return false;
        }
        let Some(menu) = &mut self.menu else {
            if edge(menu_key) {
                self.menu = Some(Menu { dirty: true, ..Menu::default() });
                return true;
            }
            return false;
        };
        if edge(menu_key) || edge(Key::Escape) {
            self.menu = None;
            self.swallow = true;
            return true;
        }
        let n = menu.options.len();
        if n > 0 {
            // Wheel up (positive notches) walks up the list, as BO2's lists scroll.
            let mut step = -(wheel.trunc() as i32);
            step += edge(Key::Down) as i32 - edge(Key::Up) as i32;
            menu.highlight = (menu.highlight as i32 + step).rem_euclid(n as i32) as usize;
        }
        if edge(Key::MouseRight) || edge(Key::Backspace) {
            if menu.picks.pop().is_some() {
                menu.dirty = true;
                menu.highlight = 0;
            }
        } else if edge(Key::MouseLeft) || edge(Key::Enter) {
            if let Some(text) = menu.options.get(menu.highlight).map(|o| o.text.clone()) {
                self.pick(&text);
            }
        }
        true
    }

    fn pick(&mut self, text: &str) {
        let Some(menu) = &mut self.menu else { return };
        if text == DONE {
            self.submitted = Some(menu.text());
            self.menu = None;
            self.swallow = true;
        } else {
            menu.picks.push(text.to_string());
            menu.dirty = true;
            menu.highlight = 0;
        }
    }

    /// Pick the option `text` if the menu offers it now (the autopilot's menu run). Returns whether
    /// it was offered.
    pub fn pick_text(&mut self, text: &str) -> bool {
        let offered = self.menu.as_ref().is_some_and(|m| !m.dirty && m.options.iter().any(|o| o.text == text));
        if offered {
            self.pick(text);
        }
        offered
    }

    /// Open the menu (the autopilot's menu run).
    pub fn open_menu(&mut self) {
        if self.menu.is_none() && !self.open {
            self.menu = Some(Menu { dirty: true, ..Menu::default() });
        }
    }

    /// One frame of the push-to-talk key (`free` as for [`Session::keys`]; never while the line or
    /// the menu is up, where the key types or the wheel picks). Leaving first person lets it go.
    pub fn voice_input(&mut self, now: &KeySet, free: bool, key: Key) {
        let down = free && !self.open && self.menu.is_none() && now.is_down(key);
        if down != self.voice_held {
            self.voice_held = down;
            self.voice_edge = Some(down);
        }
    }

    /// The push-to-talk edge this frame, if any.
    pub fn take_voice_edge(&mut self) -> Option<bool> {
        self.voice_edge.take()
    }

    /// Open the line with `line` in it, for the player to run (Enter) or drop (Escape): a voice
    /// command the confidence check would not run unasked.
    pub fn prefill(&mut self, line: &str) {
        self.menu = None;
        self.open = true;
        self.line = line.chars().take(LINE_MAX).collect();
    }

    /// Submit a line directly (the autopilot's command run).
    pub fn submit(&mut self, line: &str) {
        self.submitted = Some(line.to_string());
    }

    /// The line submitted this frame, if any.
    pub fn take_submitted(&mut self) -> Option<String> {
        self.submitted.take()
    }

    pub fn tick(&mut self, dt: f32) {
        if let Some(e) = &mut self.echo {
            e.age += dt;
            if e.age >= ECHO_TIME {
                self.echo = None;
            }
        }
    }
}

/// The engine services a command needs (the client host's `command_objects`, `command_buttons`,
/// `template_name` and `issue_order`, and the terrain).
pub trait Host {
    fn objects(&self) -> Vec<CmdObject>;
    fn buttons(&self) -> Vec<CmdButton>;
    fn template_name(&self, tmpl: u32) -> Option<String>;
    /// Send an order; the answer is an [`issued`] code.
    fn issue(&self, order: &CmdOrder, ids: &[u32]) -> i32;
    fn ground_height(&self, x: f32, y: f32) -> f32;
    /// The map's named areas (raw names) and their centres.
    fn places(&self) -> Vec<(String, Vec3)> {
        Vec::new()
    }
    /// The map's extent (low and high corners).
    fn extent(&self) -> (Vec3, Vec3) {
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(2000.0, 2000.0, 0.0))
    }
}

/// The world as the engine reports it now, for a command run or a menu refresh.
fn snapshot(host: &dyn Host) -> World {
    let mut world = World::new(host.objects(), host.buttons(), &|t| host.template_name(t));
    world.set_places(host.places());
    let (lo, hi) = host.extent();
    world.map_centre = Vec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, 0.0);
    world.map_radius = ((hi.x - lo.x).min(hi.y - lo.y) * 0.5).max(1.0);
    world
}

/// Where the General is and where he looks, for the resolver.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewer {
    pub general: u32,
    pub feet: Vec3,
    pub yaw: f32,
    pub eye: Vec3,
    pub dir: Vec3,
}

/// Run a submitted line: parse it, resolve it against the world now, send its orders, remember
/// its selection and team names, and set the echo. Returns the log line.
pub fn run(session: &mut Session, line: &str, host: &dyn Host, viewer: Viewer, tuning: &crate::tuning::Tuning) -> String {
    let mut world = snapshot(host);
    let places = if session.places_logged {
        String::new()
    } else {
        session.places_logged = true;
        let names: Vec<&str> = world.places.iter().map(|(n, _)| n.as_str()).collect();
        format!("commands: the map's places: {}\n", if names.is_empty() { "none".to_string() } else { names.join(", ") })
    };
    world.general = viewer.general;
    world.general_pos = viewer.feet;
    world.general_yaw = viewer.yaw;
    world.eye = viewer.eye;
    world.dir = viewer.dir;
    let vocab = world.vocabulary(&session.memory.team_names());
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    let settings = Settings {
        near_radius: tuning.command_near_radius,
        offset_distance: tuning.command_offset_distance,
        cluster_link: tuning.command_cluster_link,
        central_share: tuning.command_central_share,
        ground: &ground,
    };
    // The line as typed, then the normaliser's readings (synonyms, aliases): the first that parses
    // and resolves is used. Two readings that resolve to different commands are put to the player;
    // none: the error of the first reading that differed from the line (the more telling one).
    let tables = super::normal::get();
    let candidates = tables.candidates(line, &vocab);
    let mut outcome: Result<super::resolve::Plan, String> = Err(String::new());
    let mut first_error: Option<String> = None;
    let mut readings: Vec<String> = Vec::new();
    for (i, text) in candidates.iter().enumerate() {
        match super::parse(text, &vocab).map_err(|e| e.to_string()).and_then(|cmd| resolve(&cmd, &world, &session.memory, &vocab, &settings)) {
            Ok(plan) => {
                let canonical = plan.summary.split_once(": ").map_or(plan.summary.as_str(), |(_, t)| t).to_string();
                if outcome.is_err() {
                    outcome = Ok(plan);
                    readings.push(canonical);
                } else if !readings.contains(&canonical) {
                    readings.push(canonical);
                }
                // The line as typed is canonical: no reading can mean anything else.
                if i == 0 {
                    break;
                }
            }
            Err(e) => {
                if first_error.is_none() || (i == 1 && candidates[0] != *text) {
                    first_error = Some(e);
                }
            }
        }
    }
    if readings.len() > 1 {
        outcome = Err(format!("that could mean '{}': say which", readings.join("' or '")));
    } else if outcome.is_err() {
        outcome = Err(first_error.unwrap_or_else(|| "say a command".into()));
    }
    let refused = if world.refused_names.is_empty() { String::new() } else { format!(" (names left out: {})", world.refused_names.join(", ")) };
    match outcome {
        Ok(plan) => {
            // Each order's answer: what the engine refused is said, and a command none of whose
            // orders went out is refused as a whole (and changes nothing).
            let codes: Vec<i32> = plan.orders.iter().map(|(order, ids)| host.issue(order, ids)).collect();
            let sent = codes.iter().filter(|&&c| c == issued::SENT).count();
            let failed = codes.iter().copied().find(|&c| c != issued::SENT);
            if !plan.orders.is_empty() && sent == 0 {
                let why = issued::reason(failed.unwrap_or(issued::NOBODY));
                session.echo = Some(Echo { text: format!("{line}: {why}"), ok: false, age: 0.0 });
                return format!("{places}command: '{line}' refused: {why}{refused}");
            }
            session.memory.last = plan.selection.clone();
            for (name, ids) in plan.teams {
                session.memory.teams.retain(|(n, _)| n != &name);
                session.memory.teams.push((name, ids));
            }
            let partly = match failed {
                Some(code) => format!(" ({sent} of {} went out: {})", codes.len(), issued::reason(code)),
                None => String::new(),
            };
            session.echo = Some(Echo { text: format!("{}{partly}", plan.summary), ok: true, age: 0.0 });
            format!("{places}command: '{line}' -> {}, {sent} order(s){partly}{refused}", plan.summary)
        }
        Err(why) => {
            session.echo = Some(Echo { text: format!("{line}: {why}"), ok: false, age: 0.0 });
            format!("{places}command: '{line}' refused: {why}{refused}")
        }
    }
}

/// The match's vocabulary now, as a command would be read against it (the voice decoder's).
pub fn vocabulary(session: &Session, host: &dyn Host) -> super::Vocabulary {
    snapshot(host).vocabulary(&session.memory.team_names())
}

/// Recompute the menu's options after its picks changed: the completions the grammar allows next
/// over this world's vocabulary, `(done)` first when the picks are a whole command, each name with
/// how many of the player's objects it names.
pub fn refresh_menu(session: &mut Session, host: &dyn Host) {
    let teams = session.memory.team_names();
    let alive_in_team = |name: &str| session.memory.teams.iter().find(|(n, _)| n == name).map(|(_, ids)| ids.clone());
    let Some(menu) = session.menu.as_mut().filter(|m| m.dirty) else { return };
    let world = snapshot(host);
    let vocab = world.vocabulary(&teams);
    let c = super::parse::completions(&menu.text(), &vocab);
    let own: Vec<&CmdObject> = world.objects.iter().filter(|o| o.relation == super::world::relation::OWN && o.flags & super::world::cmdflags::SELECTABLE != 0).collect();
    let count = |text: &str| -> Option<usize> {
        if text == "units" {
            return Some(own.iter().filter(|o| o.flags & super::world::cmdflags::SELECT_ALL != 0).count());
        }
        if let Some((noun, tmpls)) = world.types.iter().find(|(n, _)| n.singular == text || n.plural == text) {
            let _ = noun;
            return Some(own.iter().filter(|o| tmpls.contains(&o.tmpl)).count());
        }
        if let Some(&(_, _, flag)) = super::world::CLASSES.iter().find(|(s, p, _)| *s == text || *p == text) {
            return Some(own.iter().filter(|o| o.flags & flag != 0).count());
        }
        alive_in_team(text).map(|ids| own.iter().filter(|o| ids.contains(&o.id)).count())
    };
    let mut options: Vec<MenuOption> = Vec::new();
    if c.complete {
        options.push(MenuOption { text: DONE.to_string(), count: None });
    }
    options.extend(c.next.iter().map(|t| MenuOption { text: t.clone(), count: count(t) }));
    menu.options = options;
    menu.highlight = menu.highlight.min(menu.options.len().saturating_sub(1));
    menu.dirty = false;
}
