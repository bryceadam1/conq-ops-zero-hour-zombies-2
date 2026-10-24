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
    /// focus (`ZHBO_AUTOPILOT=1`, `equipment`, `build`, `classpick`, `showcase`, `scope`).
    Scripted { equipment: bool, build: bool, classpick: bool, scope: bool },
}

/// The core's configuration, read once at init.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// `ZHBO_LOADOUT`: the General's kit spec ([`crate::weapons::Loadout::with_spec`]), or `None`
    /// for the built-in kit. Every player in a game must set it identically (it is part of the
    /// deterministic sim inputs).
    pub loadout: Option<String>,
    pub autopilot: Autopilot,
    /// `ZHBO_NATIVE_VIEWMODEL`: draw the first-person weapon through the renderer's native BO2
    /// pass (`nativevm`) instead of the W3D conversion. **On by default** (the W3D conversion is
    /// the fallback: `0`/`false`/`no`/`off`); presentation only, never a sim input.
    pub native_viewmodel: bool,
    /// `ZHBO_NATIVE_BODY`: the third-person General through the same native pass (`nativebody`).
    /// 0 off (W3D), 1 the weapon in his hand while the W3D body draws, 2 the body too (Track R7's
    /// staging: verify the weapon before the body). Presentation only.
    pub native_body: u8,
}

impl Config {
    /// Parse the two variables (the C++ side parses them once into `ZhboConfig` the same way).
    pub fn parse(loadout: Option<&str>, autopilot: Option<&str>) -> Config {
        Config { loadout: loadout.filter(|s| !s.is_empty()).map(str::to_string), autopilot: autopilot_mode(autopilot), native_viewmodel: false, native_body: 0 }
    }

    /// Read the environment once at init.
    pub fn from_env() -> Config {
        Config { native_viewmodel: on_by_default(env("ZHBO_NATIVE_VIEWMODEL")), native_body: body_mode(env("ZHBO_NATIVE_BODY")), ..Config::parse(env("ZHBO_LOADOUT"), env("ZHBO_AUTOPILOT")) }
    }
}

/// `ZHBO_NATIVE_VIEWMODEL`: since the native pass out-verified the W3D conversion (the 2026-10-21
/// fixes: display-space shading, culling, skin weights, the near plane, hide tags), the native
/// BO2 viewmodel is the DEFAULT and the W3D conversion is the fallback: off only for `0`,
/// `false`, `no` or `off` (any case), on otherwise or unset.
fn on_by_default(value: Option<&str>) -> bool {
    !matches!(value.map(|v| v.trim().to_ascii_lowercase()), Some(v) if matches!(v.as_str(), "0" | "false" | "no" | "off"))
}

/// `ZHBO_NATIVE_BODY`: 0 off, 1 the weapon only (the W3D body draws), 2 the body and the weapon.
/// `full` and `weapon` are the same spellings the plan uses in prose.
pub fn body_mode(value: Option<&str>) -> u8 {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "2" || v == "full" => 2,
        Some(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on" | "weapon") => 1,
        _ => 0,
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
            scope: m == "scope",
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

/// The configuration, falling back to the defaults before init (tests, converters). The fallback
/// must not fill the cell: the bridge's loaders run before `zhbo_init`, and a `get_or_init` here
/// once locked the defaults in, so `init` silently dropped every variable it parsed.
pub fn get() -> &'static Config {
    static DEFAULT: Config = Config { loadout: None, autopilot: Autopilot::Off, native_viewmodel: false, native_body: 0 };
    CONFIG.get().unwrap_or(&DEFAULT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_viewmodel_is_on_by_default() {
        assert!(on_by_default(None));
        assert!(on_by_default(Some("1")) && on_by_default(Some("weird")));
        assert!(!on_by_default(Some("0")) && !on_by_default(Some("OFF")) && !on_by_default(Some("no")) && !on_by_default(Some(" false ")));
    }

    #[test]
    fn a_read_before_init_does_not_lock_in_the_defaults() {
        // The cell is process-wide; this test is its only writer.
        assert!(!get().native_viewmodel || CONFIG.get().is_some());
        let _ = get();
        init(Config { native_viewmodel: true, ..Config::default() });
        assert!(get().native_viewmodel, "init after an early read still takes effect");
    }

    #[test]
    fn parses_the_autopilot_modes() {
        assert_eq!(autopilot_mode(None), Autopilot::Off);
        assert_eq!(autopilot_mode(Some("")), Autopilot::Off);
        assert_eq!(autopilot_mode(Some("manual")), Autopilot::Manual);
        assert_eq!(
            autopilot_mode(Some("1")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false }
        );
        assert_eq!(
            autopilot_mode(Some("equipment")),
            Autopilot::Scripted { equipment: true, build: false, classpick: false, scope: false }
        );
        assert_eq!(
            autopilot_mode(Some("build")),
            Autopilot::Scripted { equipment: false, build: true, classpick: false, scope: false }
        );
        assert_eq!(
            autopilot_mode(Some("classpick")),
            Autopilot::Scripted { equipment: false, build: false, classpick: true, scope: false }
        );
        assert_eq!(
            autopilot_mode(Some("scope")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: true }
        );
        // Any other value scripts the standard input, like before (the bridge matched modes the
        // same way; unknown values fall through to scripted input).
        assert_eq!(
            autopilot_mode(Some("showcase")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false }
        );
    }

    #[test]
    fn flags_read_the_usual_spellings() {
        assert!(flag(Some("1")) && flag(Some("TRUE")) && flag(Some("on")));
        assert!(!flag(None) && !flag(Some("0")) && !flag(Some("no")) && !flag(Some("")));
        assert!(!Config::parse(None, None).native_viewmodel, "off unless asked for");
    }

    #[test]
    fn the_native_body_stages_the_weapon_before_the_body() {
        assert_eq!(body_mode(None), 0);
        assert_eq!(body_mode(Some("0")), 0);
        assert_eq!(body_mode(Some("weapon")), 1);
        assert_eq!(body_mode(Some("TRUE")), 1);
        assert_eq!(body_mode(Some("2")), 2);
        assert_eq!(body_mode(Some("full")), 2);
        assert_eq!(body_mode(Some("garbage")), 0);
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