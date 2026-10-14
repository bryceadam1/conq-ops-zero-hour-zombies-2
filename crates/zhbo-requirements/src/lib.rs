//! Find the player's own installs of Command & Conquer Generals: Zero Hour and Call of Duty: Black
//! Ops II. Conquer Ops needs both: Zero Hour is the game you play, and Black Ops II supplies the
//! first-person content. The mod refuses to start unless both are installed.
//!
//! Works on Windows, Linux and macOS. On Linux/macOS the games are Windows programs installed by a
//! native Steam client (run through Proton) or by a Windows Steam client inside a Wine prefix,
//! CrossOver bottle or Whisky bottle; [`GameInstall::prefix`] records which.
//!
//! Only presence of real game files is checked (plus the Steam app manifest when installed through
//! Steam). Nothing here touches DRM, and the games' files are never modified.

pub mod paths;
pub mod prefixes;
pub mod steam;
pub mod vdf;

use paths::{is_file_ci, join_ci_all};
use prefixes::Prefix;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameInstall {
    pub path: PathBuf,
    /// How it was found, e.g. "Steam app 2732960" or "ZHBO_BO2_DIR".
    pub source: String,
    /// The Wine-style prefix whose Windows Steam installed it (Linux/macOS), if any.
    pub prefix: Option<Prefix>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirements {
    pub zero_hour: Result<GameInstall, String>,
    pub black_ops2: Result<GameInstall, String>,
}

impl Requirements {
    pub fn ok(&self) -> bool {
        self.zero_hour.is_ok() && self.black_ops2.is_ok()
    }

    /// Human-readable explanation of what is missing (empty when everything is present).
    pub fn problems(&self) -> String {
        let mut out = String::new();
        for r in [&self.zero_hour, &self.black_ops2] {
            if let Err(e) = r {
                out.push_str(e);
                out.push('\n');
            }
        }
        out
    }
}

/// Steam app ids. Zero Hour: the 2025 Steam release. Black Ops II ships as three apps sharing one folder.
pub const ZH_STEAM_APPS: &[u32] = &[2732960];
pub const BO2_STEAM_APPS: &[u32] = &[202970, 202990, 212910];

/// Everything the search found, for the launcher's diagnostics.
pub struct Environment {
    pub native_steam: Vec<PathBuf>,
    pub prefixes: Vec<Prefix>,
    pub libraries: Vec<steam::Library>,
}

pub fn environment() -> Environment {
    let native_steam = steam::native_roots();
    let prefixes = prefixes::discover(&native_steam);
    let libraries = steam::libraries(&native_steam, &prefixes);
    Environment { native_steam, prefixes, libraries }
}

pub fn check_requirements() -> Requirements {
    check_in(&environment())
}

pub fn check_in(env: &Environment) -> Requirements {
    Requirements {
        zero_hour: find(env, "ZHBO_ZH_DIR", ZH_STEAM_APPS, validate_zero_hour, &zh_registry_paths(), ZH_MISSING),
        black_ops2: find(env, "ZHBO_BO2_DIR", BO2_STEAM_APPS, validate_black_ops2, &[], BO2_MISSING),
    }
}

const ZH_MISSING: &str =
    "Command & Conquer Generals: Zero Hour is not installed. Install it (e.g. from Steam) to play Conquer Ops.";
const BO2_MISSING: &str = "Call of Duty: Black Ops II is not installed. Install it from Steam to play Conquer Ops.";

/// Files that prove a folder is a Zero Hour install (not just a stray folder name).
pub fn validate_zero_hour(dir: &Path) -> Result<(), String> {
    for f in ["INIZH.big", "W3DZH.big", "TexturesZH.big"] {
        if !is_file_ci(dir, &[f]) {
            return Err(format!("{} is not a Zero Hour install (missing {f})", dir.display()));
        }
    }
    // The original Generals data must be reachable too: the Steam release keeps it in ZH_Generals.
    let base = is_file_ci(dir, &["ZH_Generals", "INI.big"])
        || generals_registry_path().is_some_and(|p| is_file_ci(&p, &["INI.big"]));
    if !base {
        return Err(format!("{}: the original Generals data (ZH_Generals\\INI.big) was not found", dir.display()));
    }
    Ok(())
}

/// Files that prove a folder is a Black Ops II install.
pub fn validate_black_ops2(dir: &Path) -> Result<(), String> {
    if !["t6sp.exe", "t6mp.exe", "t6zm.exe"].iter().any(|e| is_file_ci(dir, &[e])) {
        return Err(format!("{} is not a Black Ops II install (no t6*.exe)", dir.display()));
    }
    if !is_file_ci(dir, &["zone", "all", "common_mp.ff"]) && !is_file_ci(dir, &["zone", "all", "common.ff"]) {
        return Err(format!("{}: Black Ops II fastfiles (zone\\all\\common*.ff) are missing", dir.display()));
    }
    let has_sounds = fs::read_dir(join_ci_all(dir, &["sound"]))
        .map(|rd| {
            rd.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                n.ends_with(".sabs") || n.ends_with(".sabl")
            })
        })
        .unwrap_or(false);
    if !has_sounds {
        return Err(format!("{}: Black Ops II sound banks (sound\\*.sabs) are missing", dir.display()));
    }
    Ok(())
}

fn find(
    env: &Environment,
    var: &str,
    apps: &[u32],
    validate: fn(&Path) -> Result<(), String>,
    extra: &[PathBuf],
    missing: &str,
) -> Result<GameInstall, String> {
    if let Some(dir) = std::env::var_os(var).map(PathBuf::from) {
        validate(&dir)?;
        let prefix = env.prefixes.iter().find(|p| dir.starts_with(p.drive_c())).cloned();
        return Ok(GameInstall { path: dir, source: var.into(), prefix });
    }
    for lib in &env.libraries {
        for &app in apps {
            if let Some(dir) = steam::app_dir(lib, app) {
                if validate(&dir).is_ok() {
                    return Ok(GameInstall { path: dir, source: format!("Steam app {app}"), prefix: lib.prefix.clone() });
                }
            }
        }
    }
    for dir in extra {
        if validate(dir).is_ok() {
            return Ok(GameInstall { path: dir.clone(), source: "EA registry".into(), prefix: None });
        }
    }
    Err(missing.into())
}

fn zh_registry_paths() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        return reg::read_string(
            reg::HKLM,
            r"SOFTWARE\WOW6432Node\Electronic Arts\EA Games\Command and Conquer Generals Zero Hour",
            "InstallPath",
        )
        .map(PathBuf::from)
        .into_iter()
        .collect();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

fn generals_registry_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return reg::read_string(reg::HKLM, r"SOFTWARE\WOW6432Node\Electronic Arts\EA Games\Generals", "InstallPath")
            .map(PathBuf::from);
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(windows)]
mod reg {
    //! Minimal registry access; declared directly so the static library only needs advapi32.lib.
    use std::ffi::c_void;

    pub type Hkey = isize;
    pub const HKCU: Hkey = 0x8000_0001u32 as i32 as isize;
    pub const HKLM: Hkey = 0x8000_0002u32 as i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: Hkey,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            ty: *mut u32,
            data: *mut c_void,
            size: *mut u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn read_string(root: Hkey, subkey: &str, value: &str) -> Option<String> {
        let (k, v) = (wide(subkey), wide(value));
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: buffers are valid for the sizes passed; RegGetValueW NUL-terminates REG_SZ.
        let rc = unsafe {
            RegGetValueW(root, k.as_ptr(), v.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
        };
        if rc != 0 {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        let s = String::from_utf16_lossy(&buf[..len]);
        (!s.is_empty()).then_some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prefixes::PrefixKind;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zhbo-req-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_bo2(d: &Path) {
        fs::write(d.join("t6mp.exe"), b"").unwrap();
        fs::create_dir_all(d.join("zone/all")).unwrap();
        fs::write(d.join("zone/all/common_mp.ff"), b"").unwrap();
        fs::create_dir_all(d.join("sound")).unwrap();
        fs::write(d.join("sound/mpl_common.all.sabs"), b"").unwrap();
    }

    fn fake_zh(d: &Path) {
        for f in ["INIZH.big", "W3DZH.big", "TexturesZH.big"] {
            fs::write(d.join(f), b"").unwrap();
        }
        fs::create_dir_all(d.join("ZH_Generals")).unwrap();
        fs::write(d.join("ZH_Generals/INI.big"), b"").unwrap();
    }

    #[test]
    fn validates_fake_installs() {
        let d = scratch("bo2");
        assert!(validate_black_ops2(&d).is_err());
        fake_bo2(&d);
        assert!(validate_black_ops2(&d).is_ok());
        let z = scratch("zh");
        fake_zh(&z);
        assert!(validate_zero_hour(&z).is_ok());
        fs::remove_file(z.join("W3DZH.big")).unwrap();
        assert!(validate_zero_hour(&z).is_err());
        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&z).ok();
    }

    /// A Windows Steam inside a Wine prefix with both games in a second library folder.
    #[test]
    fn finds_games_inside_a_prefix_library() {
        let root = scratch("prefix");
        let prefix = Prefix { kind: PrefixKind::CrossOver, root: root.join("bottle") };
        let steam = prefix.drive_c().join("Program Files (x86)/Steam");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let d_lib = prefix.drive_c().join("Games/SteamLibrary");
        fs::create_dir_all(d_lib.join("steamapps/common/Call of Duty Black Ops II")).unwrap();
        fs::create_dir_all(d_lib.join("steamapps/common/Command & Conquer Generals - Zero Hour")).unwrap();
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            "\"libraryfolders\"\n{\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Games\\\\SteamLibrary\"\n\t}\n}\n",
        )
        .unwrap();
        fs::write(d_lib.join("steamapps/appmanifest_202990.acf"), "\t\"installdir\"\t\t\"Call of Duty Black Ops II\"\n").unwrap();
        fs::write(
            d_lib.join("steamapps/appmanifest_2732960.acf"),
            "\t\"installdir\"\t\t\"Command & Conquer Generals - Zero Hour\"\n",
        )
        .unwrap();
        fake_bo2(&d_lib.join("steamapps/common/Call of Duty Black Ops II"));
        fake_zh(&d_lib.join("steamapps/common/Command & Conquer Generals - Zero Hour"));

        let libraries = steam::libraries(&[], std::slice::from_ref(&prefix));
        let env = Environment { native_steam: vec![], prefixes: vec![prefix.clone()], libraries };
        let req = check_in(&env);
        let bo2 = req.black_ops2.clone().expect("found BO2");
        assert_eq!(bo2.source, "Steam app 202990");
        assert_eq!(bo2.prefix.as_ref().map(|p| p.kind), Some(PrefixKind::CrossOver));
        assert!(req.zero_hour.is_ok());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    #[ignore = "inspects this machine's real installs"]
    fn real_machine() {
        let env = environment();
        println!("steam: {:?}\nprefixes: {:?}", env.native_steam, env.prefixes);
        println!("{:#?}", check_in(&env));
    }
}
