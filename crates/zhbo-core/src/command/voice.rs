//! Voice commands in the game (docs/COMMANDS.md section 11): the link to `conquerops-voice`, the
//! process beside the game that records the microphone, hears it and translates it into the
//! command language. Presentation only: what comes back is a command line, run like a typed one,
//! and only the orders it resolves to reach the sim.
//!
//! The link never blocks a frame: the process starts in the background, its replies are read on
//! their own thread, and [`Voice::poll`] takes whatever has arrived.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

use super::session::{Echo, Session};

/// The process's executable, beside the game's.
pub const EXE: &str = if cfg!(windows) { "conquerops-voice.exe" } else { "conquerops-voice" };

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// The models are loading.
    Starting,
    Ready,
    /// No voice this game, and why (no models yet, no executable, no microphone).
    Unavailable(String),
    Listening,
    /// The key came up; the answer is on its way.
    Thinking,
}

/// One utterance as the process heard it.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub transcript: String,
    pub command: String,
    /// The most probability the grammar's mask took away at one step.
    pub masked: f32,
    pub stuck: bool,
    pub no_speech: f32,
}

/// A reply line as an event.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    Ready,
    Unavailable(String),
    Listening,
    Heard(Heard),
    Error(String),
}

/// A line from the process (see `conquerops-voice`'s protocol).
pub fn parse_reply(line: &str) -> Option<Reply> {
    let f: Vec<&str> = line.split('\t').collect();
    Some(match f[0] {
        "ready" => Reply::Ready,
        "unavailable" => Reply::Unavailable(f.get(1).unwrap_or(&"").to_string()),
        "listening" => Reply::Listening,
        "error" => Reply::Error(f.get(1).unwrap_or(&"").to_string()),
        "heard" if f.len() >= 6 => Reply::Heard(Heard {
            masked: f[1].parse().ok()?,
            stuck: f[2] == "1",
            no_speech: f[3].parse().ok()?,
            transcript: f[4].to_string(),
            command: f[5].to_string(),
        }),
        _ => return None,
    })
}

pub struct Voice {
    pub state: State,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    replies: Option<Receiver<String>>,
    /// The vocabulary line last sent: sent again only when it changed.
    sent: String,
}

impl Voice {
    /// Start the process at `exe` (it loads its models in the background).
    pub fn start(exe: &Path) -> Voice {
        let off = |why: String| Voice { state: State::Unavailable(why), child: None, stdin: None, replies: None, sent: String::new() };
        if !exe.is_file() {
            return off(format!("{} is missing", exe.display()));
        }
        let mut cmd = Command::new(exe);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return off(format!("could not start {}: {e}", exe.display())),
        };
        let (tx, rx) = channel();
        if let Some(out) = child.stdout.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        Voice { state: State::Starting, stdin: child.stdin.take(), child: Some(child), replies: Some(rx), sent: String::new() }
    }

    /// The process beside the running game's executable.
    pub fn beside_game() -> Voice {
        let exe: Option<PathBuf> = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(EXE)));
        match exe {
            Some(exe) => Voice::start(&exe),
            None => Voice { state: State::Unavailable("cannot find the game's folder".into()), child: None, stdin: None, replies: None, sent: String::new() },
        }
    }

    fn send(&mut self, line: &str) {
        let ok = self.stdin.as_mut().is_some_and(|s| writeln!(s, "{line}").and_then(|_| s.flush()).is_ok());
        if !ok && !matches!(self.state, State::Unavailable(_)) {
            self.state = State::Unavailable("the voice process stopped".into());
        }
    }

    fn vocab(&mut self, line: &str) {
        if line != self.sent {
            self.send(&format!("vocab\t{line}"));
            self.sent = line.to_string();
        }
    }

    /// The push-to-talk key went down: record, for the match's names in `vocab_line`.
    pub fn listen(&mut self, vocab_line: &str) {
        if self.state == State::Ready {
            self.vocab(vocab_line);
            self.send("listen");
            self.state = State::Listening;
        }
    }

    /// It came up: hear what was recorded.
    pub fn stop(&mut self) {
        if self.state == State::Listening {
            self.send("stop");
            self.state = State::Thinking;
        }
    }

    /// Hear a WAV file instead of the microphone (the autotest).
    pub fn hear_file(&mut self, vocab_line: &str, path: &Path) {
        if self.state == State::Ready {
            self.vocab(vocab_line);
            self.send(&format!("hear-file\t{}", path.display()));
            self.state = State::Thinking;
        }
    }

    /// The replies that arrived since the last poll; the state follows them.
    pub fn poll(&mut self) -> Vec<Reply> {
        let Some(rx) = &self.replies else { return Vec::new() };
        let lines: Vec<String> = rx.try_iter().collect();
        let mut out = Vec::new();
        for line in lines {
            let Some(r) = parse_reply(&line) else { continue };
            match &r {
                Reply::Ready => self.state = State::Ready,
                Reply::Unavailable(why) => self.state = State::Unavailable(why.clone()),
                Reply::Listening => {}
                Reply::Heard(_) | Reply::Error(_) => {
                    if !matches!(self.state, State::Unavailable(_) | State::Starting) {
                        self.state = State::Ready;
                    }
                }
            }
            out.push(r);
        }
        out
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.send("quit");
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// What to do with an utterance (docs/COMMANDS.md section 9, the confidence check).
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// Run it as if typed.
    Run(String),
    /// Put it in the command line for the player to confirm (Enter) or drop (Escape): the model
    /// wanted something the grammar does not allow, so it may not be what was meant.
    Confirm(String),
    /// Nothing to run: no speech, or no command in it.
    Nothing,
}

/// The act for `heard`: run when the mask took at most `guard` of the model's probability at any
/// step, confirm above it, nothing when Whisper thought there was no speech (`no_speech` above
/// `silence`) or the mask left nothing to say.
pub fn act(heard: &Heard, guard: f32, silence: f32) -> Act {
    if heard.stuck || heard.command.is_empty() || heard.transcript.is_empty() || heard.no_speech > silence {
        Act::Nothing
    } else if heard.masked > guard {
        Act::Confirm(heard.command.clone())
    } else {
        Act::Run(heard.command.clone())
    }
}

/// Apply `act` to the command line, with the echo saying what was heard. Returns the log line.
pub fn apply(session: &mut Session, heard: &Heard, act: &Act) -> String {
    let said = &heard.transcript;
    match act {
        Act::Run(command) => {
            session.submit(command);
            format!("voice: heard '{said}', running '{command}' (mask {:.2})", heard.masked)
        }
        Act::Confirm(command) => {
            session.prefill(command);
            session.echo = Some(Echo { text: format!("Heard \"{said}\": Enter runs it"), ok: true, age: 0.0 });
            format!("voice: heard '{said}', asking about '{command}' (mask {:.2})", heard.masked)
        }
        Act::Nothing => {
            session.echo = Some(Echo { text: if said.is_empty() { "Didn't catch that".to_string() } else { format!("Didn't catch \"{said}\"") }, ok: false, age: 0.0 });
            format!("voice: heard '{said}', nothing to run (no speech {:.2}, stuck {})", heard.no_speech, heard.stuck)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(masked: f32) -> Heard {
        Heard { transcript: "rangers go to me".into(), command: "rangers move to me".into(), masked, stuck: false, no_speech: 0.01 }
    }

    #[test]
    fn replies_parse() {
        assert_eq!(parse_reply("ready"), Some(Reply::Ready));
        assert_eq!(parse_reply("unavailable\tno models"), Some(Reply::Unavailable("no models".into())));
        assert_eq!(parse_reply("heard\t0.250\t0\t0.010\trangers go to me\trangers move to me"), Some(Reply::Heard(heard(0.25))));
        assert_eq!(parse_reply("heard\tnope"), None);
    }

    #[test]
    fn a_sure_command_runs_and_an_unsure_one_asks() {
        assert_eq!(act(&heard(0.05), 0.1, 0.6), Act::Run("rangers move to me".into()));
        assert_eq!(act(&heard(0.4), 0.1, 0.6), Act::Confirm("rangers move to me".into()));
        assert_eq!(act(&Heard { no_speech: 0.9, ..heard(0.0) }, 0.1, 0.6), Act::Nothing);
        assert_eq!(act(&Heard { stuck: true, ..heard(0.0) }, 0.1, 0.6), Act::Nothing);

        let mut s = Session::default();
        apply(&mut s, &heard(0.05), &Act::Run("rangers move to me".into()));
        assert_eq!(s.take_submitted().as_deref(), Some("rangers move to me"));
        apply(&mut s, &heard(0.4), &Act::Confirm("rangers move to me".into()));
        assert!(s.open && s.line == "rangers move to me" && s.take_submitted().is_none());
    }

    #[test]
    fn without_its_executable_voice_is_off() {
        let v = Voice::start(Path::new("/nonexistent/conquerops-voice"));
        assert!(matches!(v.state, State::Unavailable(_)));
    }
}
