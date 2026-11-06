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
    /// focus (`ZHBO_AUTOPILOT=1`, `equipment`, `build`, `classpick`, `showcase`, `scope`, `smoke`, `splash`, `drops`, `flicker`).
    Scripted { equipment: bool, build: bool, classpick: bool, scope: bool, smoke: bool, splash: bool, drops: bool, flicker: bool },
    /// The two-instance multiplayer test (`ZHBO_AUTOPILOT=mp`, `scripts/mptest-mac.sh`): no scene
    /// is spawned (every machine would have to spawn it identically); each machine builds its own
    /// General from its Command Center, walks him to a meeting point, takes control and fights the
    /// other player's General through the replicated messages only. [`Config::mp_role`] picks
    /// which of the two fires the long burst.
    Multiplayer,
}

/// The multiplayer autopilot's part (`ZHBO_AUTOPILOT_ROLE`): presentation of the script only, never
/// a sim input (each machine scripts only its own player's input, which travels in the messages).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MpRole {
    /// Fires the long aimed burst that should kill the other General (the default).
    #[default]
    Shooter,
    /// Fires a short burst back, and is the one meant to die and come back (`target`).
    Target,
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
    /// 0 off (W3D), 1 the weapon in his hand while the W3D body draws, 2 the body too (**the
    /// default**: BO2's body with its upper-body actions, the combination's attachments and the
    /// class camo). Presentation only.
    pub native_body: u8,
    /// `ZHBO_AUTOPILOT_ROLE` for the multiplayer autopilot (`target`, or the shooter otherwise).
    pub mp_role: MpRole,
    /// `ZHBO_AUTOPILOT_ROUNDS`: how many first-person lives the multiplayer autopilot plays (1, the
    /// default: the short run; more: the long run, `client::Autopilot::mp_round`). Script only.
    pub mp_rounds: u32,
    /// `ZHBO_STATE_HASH_LOG`: log [`crate::sim::Sim::state_hash`] after every logic step
    /// (`state hash: frame N <hash> world <hash> seats <seat>:<hash> …` in `ConquerOps.log`: `Sim::fingerprint` and its parts), so two machines' logs of one network game
    /// can be compared frame by frame. A log only; never a sim input.
    pub state_hash_log: bool,
}

impl Config {
    /// Parse the two variables (the C++ side parses them once into `ZhboConfig` the same way).
    pub fn parse(loadout: Option<&str>, autopilot: Option<&str>) -> Config {
        Config {
            loadout: loadout.filter(|s| !s.is_empty()).map(str::to_string),
            autopilot: autopilot_mode(autopilot),
            native_viewmodel: false,
            native_body: 0,
            mp_role: MpRole::Shooter,
            mp_rounds: 1,
            state_hash_log: false,
        }
    }

    /// Read the environment once at init.
    pub fn from_env() -> Config {
        Config {
            native_viewmodel: on_by_default(env("ZHBO_NATIVE_VIEWMODEL")),
            native_body: body_mode(env("ZHBO_NATIVE_BODY")),
            mp_role: mp_role(env("ZHBO_AUTOPILOT_ROLE")),
            mp_rounds: env("ZHBO_AUTOPILOT_ROUNDS").and_then(|v| v.trim().parse().ok()).filter(|&n| n >= 1).unwrap_or(1),
            state_hash_log: flag(env("ZHBO_STATE_HASH_LOG")),
            ..Config::parse(env("ZHBO_LOADOUT"), env("ZHBO_AUTOPILOT"))
        }
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
/// The full native BO2 body is the default (unset, empty or unrecognised): it is the only third
/// person that plays BO2's upper-body actions (`torso`) and stands at the drawn size the sim traces.
/// `1`/`weapon` opt out to the native weapon in the W3D body's hand, `0`/`false`/`no`/`off` to the
/// W3D weapon and body. `full` and `weapon` are the same spellings the plan uses in prose.
pub fn body_mode(value: Option<&str>) -> u8 {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if matches!(v.as_str(), "1" | "weapon" | "true" | "yes" | "on") => 1,
        Some(v) if matches!(v.as_str(), "0" | "false" | "no" | "off") => 0,
        _ => 2,
    }
}

/// An on/off variable: on for `1`, `true`, `yes` or `on` (any case), off otherwise or unset.
pub fn flag(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

/// `ZHBO_AUTOPILOT_ROLE`: `target` (any case) for the side meant to die; anything else shoots.
pub fn mp_role(value: Option<&str>) -> MpRole {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "target" => MpRole::Target,
        _ => MpRole::Shooter,
    }
}

/// ZHBO_AUTOPILOT's mode: `manual` for the scene-only setup, off/unset (or empty), `mp` for the
/// two-instance multiplayer test, or a scripted mode (`equipment`/`build`/`classpick` pick the
/// test's focus; anything else scripts the standard input).
pub fn autopilot_mode(value: Option<&str>) -> Autopilot {
    match value {
        None | Some("") => Autopilot::Off,
        Some("manual") => Autopilot::Manual,
        Some("mp") => Autopilot::Multiplayer,
        Some(m) => Autopilot::Scripted {
            equipment: m == "equipment",
            build: m == "build",
            classpick: m == "classpick",
            scope: m == "scope",
            smoke: m == "smoke",
            splash: m == "splash",
            drops: m == "drops",
            flicker: m == "flicker",
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
    static DEFAULT: Config =
        Config { loadout: None, autopilot: Autopilot::Off, native_viewmodel: false, native_body: 0, mp_role: MpRole::Shooter, mp_rounds: 1, state_hash_log: false };
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
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false, smoke: false, splash: false, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("equipment")),
            Autopilot::Scripted { equipment: true, build: false, classpick: false, scope: false, smoke: false, splash: false, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("build")),
            Autopilot::Scripted { equipment: false, build: true, classpick: false, scope: false, smoke: false, splash: false, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("classpick")),
            Autopilot::Scripted { equipment: false, build: false, classpick: true, scope: false, smoke: false, splash: false, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("scope")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: true, smoke: false, splash: false, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("smoke")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false, smoke: true, splash: false, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("splash")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false, smoke: false, splash: true, drops: false, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("drops")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false, smoke: false, splash: false, drops: true, flicker: false }
        );
        assert_eq!(
            autopilot_mode(Some("flicker")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false, smoke: false, splash: false, drops: false, flicker: true }
        );
        assert_eq!(autopilot_mode(Some("mp")), Autopilot::Multiplayer);
        // Any other value scripts the standard input, like before (the bridge matched modes the
        // same way; unknown values fall through to scripted input).
        assert_eq!(
            autopilot_mode(Some("showcase")),
            Autopilot::Scripted { equipment: false, build: false, classpick: false, scope: false, smoke: false, splash: false, drops: false, flicker: false }
        );
    }

    #[test]
    fn the_multiplayer_role_defaults_to_the_shooter() {
        assert_eq!(mp_role(None), MpRole::Shooter);
        assert_eq!(mp_role(Some("shooter")), MpRole::Shooter);
        assert_eq!(mp_role(Some(" Target ")), MpRole::Target);
        assert_eq!(mp_role(Some("anything")), MpRole::Shooter);
    }

    #[test]
    fn flags_read_the_usual_spellings() {
        assert!(flag(Some("1")) && flag(Some("TRUE")) && flag(Some("on")));
        assert!(!flag(None) && !flag(Some("0")) && !flag(Some("no")) && !flag(Some("")));
        assert!(!Config::parse(None, None).native_viewmodel, "off unless asked for");
    }

    #[test]
    fn the_native_body_stages_the_weapon_before_the_body() {
        assert_eq!(body_mode(None), 2, "the native body is the default");
        assert_eq!(body_mode(Some("")), 2);
        assert_eq!(body_mode(Some("1")), 1);
        assert_eq!(body_mode(Some("0")), 0);
        assert_eq!(body_mode(Some("Off")), 0);
        assert_eq!(body_mode(Some("false")), 0);
        assert_eq!(body_mode(Some("weapon")), 1);
        assert_eq!(body_mode(Some("TRUE")), 1);
        assert_eq!(body_mode(Some("2")), 2);
        assert_eq!(body_mode(Some("full")), 2);
        assert_eq!(body_mode(Some("garbage")), 2);
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