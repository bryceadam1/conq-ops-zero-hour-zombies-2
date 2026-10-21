//! The mod's configuration: every `ZHBO_*` variable the Rust core reads, parsed once at init into
//! [`Config`]. Deterministic sim code reads only this struct — no wall-clock, no environment reads
//! during a game. See `docs/CONFIG.md` for the full variable inventory.

use std::sync::OnceLock;

/// ZHBO_AUTOPILOT's modes. Parsed the same way on both sides of the bridge (the Rust core here, the
/// C++ glue's `ZhboConfig`), so the modes mean the same thing everywhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Autopilot {
    /// No test scene (default).
    #[default]
    Off,
    /// The scene spawns (the bridge spawns the General and takes control) but input stays with the
    /// player or a test driver (`ZHBO_AUTOPILOT=manual`).
    Manual,
    /// Scripted first-person input drives the General (any other value); the flags pick the test's
    /// focus (`ZHBO_AUTOPILOT=1`, `equipment`, `build`, `classpick`, `showcase`).
    Scripted { equipment: bool, build: bool, classpick: bool },
}

/// The core's configuration, read once at init.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// `ZHBO_LOADOUT`: the General's kit spec ([`crate::weapons::Loadout::with_spec`]), or `None`
    /// for the built-in kit. Every player in a game must set it identically (it is part of the
    /// deterministic sim inputs).
    pub loadout: Option<String>,
    pub autopilot: Autopilot,
    /// `ZHBO_NATIVE_VIEWMODEL=1`: draw the first-person weapon through the renderer's native BO2
    /// pass (`nativevm`) instead of the W3D conversion. Presentation only, never a sim input.
    pub native_viewmodel: bool,
}

impl Config {
    /// Parse the two variables (the C++ side parses them once into `ZhboConfig` the same way).
    pub fn parse(loadout: Option<&str>, autopilot: Option<&str>) -> Config {
        Config { loadout: loadout.filter(|s| !s.is_empty()).map(str::to_string), autopilot: autopilot_mode(autopilot), native_viewmodel: false }
    }

    /// Read the environment once at init.
    pub fn from_env() -> Config {
        Config { native_viewmodel: flag(env("ZHBO_NATIVE_VIEWMODEL")), ..Config::parse(env("ZHBO_LOADOUT"), env("ZHBO_AUTOPILOT")) }
    }
}

/// An on/off variable: on for `1`, `true`, `yes` or `on` (any case), off otherwise or unset.
pub fn flag(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

/// ZHBO_AUTOPILOT's mode: `manual` for the scene-only setup, off/unset (or empty), or a scripted
/// mode (`equipment`/`build`/`classpick` pick the test's focus; anything else scripts the standard
/// input).
pub fn autopilot_mode(value: Option<&str>) -> Autopilot {
    match value {
        None | Some("") => Autopilot::Off,
        Some("manual") => Autopilot::Manual,
        Some(m) => Autopilot::Scripted {
            equipment: m == "equipment",
            build: m == "build",
            classpick: m == "classpick",
        },
    }
}

fn env(name: &str) -> Option<&'static str> {
    // Leak the value: the config is read once and lives for the process.
    std::env::var(name).ok().filter(|s| !s.is_empty()).map(|s| Box::leak(s.into_boxed_str()) as &str)
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Set once at init (`zhbo_init`). Tests and shell use run before that read the defaults.
pub fn init(config: Config) {
    let _ = CONFIG.set(config);
}

/// The configuration, falling back to the defaults before init (tests, converters).
pub fn get() -> &'static Config {
    CONFIG.get_or_init(Config::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_autopilot_modes() {
        assert_eq!(autopilot_mode(None), Autopilot::Off);
        assert_eq!(autopilot_mode(Some("")), Autopilot::Off);
        assert_eq!(autopilot_mode(Some("manual")), Autopilot::Manual);
        assert_eq!(
            autopilot_mode(Some("1")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false }
        );
        assert_eq!(
            autopilot_mode(Some("equipment")),
            Autopilot::Scripted { equipment: true, build: false, classpick: false }
        );
        assert_eq!(
            autopilot_mode(Some("build")),
            Autopilot::Scripted { equipment: false, build: true, classpick: false }
        );
        assert_eq!(
            autopilot_mode(Some("classpick")),
            Autopilot::Scripted { equipment: false, build: false, classpick: true }
        );
        // Any other value scripts the standard input, like before (the bridge matched modes the
        // same way; unknown values fall through to scripted input).
        assert_eq!(
            autopilot_mode(Some("showcase")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false }
        );
    }

    #[test]
    fn flags_read_the_usual_spellings() {
        assert!(flag(Some("1")) && flag(Some("TRUE")) && flag(Some("on")));
        assert!(!flag(None) && !flag(Some("0")) && !flag(Some("no")) && !flag(Some("")));
        assert!(!Config::parse(None, None).native_viewmodel, "off unless asked for");
    }

    #[test]
    fn loadout_spec_is_optional() {
        let c = Config::parse(None, None);
        assert_eq!(c.loadout, None);
        assert_eq!(c.autopilot, Autopilot::Off);
        assert_eq!(Config::parse(Some("primary=dsr50_mp+acog"), Some("manual")).loadout.as_deref(), Some("primary=dsr50_mp+acog"));
        // An empty loadout is the same as none (the var is unset or empty in both cases).
        assert_eq!(Config::parse(Some(""), None).loadout, None);
    }
}