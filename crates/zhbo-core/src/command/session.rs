//! The first-person command line (docs/COMMANDS.md section 8, the typed entry) and running a
//! submitted line against the engine.
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
    let mut world = World::new(host.objects(), host.buttons(), &|t| host.template_name(t));
    world.general = viewer.general;
    world.general_pos = viewer.feet;
    world.general_yaw = viewer.yaw;
    world.eye = viewer.eye;
    world.dir = viewer.dir;
    let vocab = world.vocabulary(&session.memory.team_names());
    let ground = |x: f32, y: f32| host.ground_height(x, y);
    let settings = Settings { near_radius: tuning.command_near_radius, offset_distance: tuning.command_offset_distance, ground: &ground };
    let outcome = super::parse(line, &vocab)
        .map_err(|e| e.to_string())
        .and_then(|cmd| resolve(&cmd, &world, &session.memory, &vocab, &settings));
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
                return format!("command: '{line}' refused: {why}{refused}");
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
            format!("command: '{line}' -> {}, {sent} order(s){partly}{refused}", plan.summary)
        }
        Err(why) => {
            session.echo = Some(Echo { text: format!("{line}: {why}"), ok: false, age: 0.0 });
            format!("command: '{line}' refused: {why}{refused}")
        }
    }
}
