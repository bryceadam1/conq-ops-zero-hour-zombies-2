//! Find the player's own installs of Command & Conquer Generals: Zero Hour and Call of Duty: Black
//! Ops II. Conquer Ops needs both: Zero Hour is the game you play, and Black Ops II supplies the
//! first-person content. The mod refuses to start unless both are installed.
//!
//! Only presence of real game files is checked (plus the Steam app manifest when installed through
//! Steam). Nothing here touches DRM, and the games' files are never modified.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameInstall {
    pub path: PathBuf,
    /// How it was found, e.g. "Steam app 2732960" or "ZHBO_BO2_DIR".
    pub source: String,
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
        if let Err(e) = &self.zero_hour {
            out.push_str(e);
            out.push('\n');
        }
        if let Err(e) = &self.black_ops2 {
            out.push_str(e);
            out.push('\n');
        }
        out
    }
}

/// Steam app ids. Zero Hour: the 2025 Steam release. Black Ops II ships as three apps sharing one folder.
pub const ZH_STEAM_APPS: &[u32] = &[2732960];
pub const BO2_STEAM_APPS: &[u32] = &[202970, 202990, 212910];

pub fn check_requirements() -> Requirements {
    let libraries = steam_libraries();
    Requirements { zero_hour: find_zero_hour(&libraries), black_ops2: find_black_ops2(&libraries) }
}

/// Files that prove a folder is a Zero Hour install (not just a stray folder name).
pub fn validate_zero_hour(dir: &Path) -> Result<(), String> {
    for f in ["INIZH.big", "W3DZH.big", "TexturesZH.big"] {
        if !dir.join(f).is_file() {
            return Err(format!("{} is not a Zero Hour install (missing {f})", dir.display()));
        }
    }
    // The original Generals data must be reachable too: the Steam release keeps it in ZH_Generals.
    if !dir.join("ZH_Generals").join("INI.big").is_file() && !registry_generals_path().is_some_and(|p| p.join("INI.big").is_file()) {
        return Err(format!("{}: the original Generals data (ZH_Generals\\INI.big) was not found", dir.display()));
    }
    Ok(())
}

/// Files that prove a folder is a Black Ops II install.
pub fn validate_black_ops2(dir: &Path) -> Result<(), String> {
    let exe = ["t6sp.exe", "t6mp.exe", "t6zm.exe"].iter().any(|e| dir.join(e).is_file());
    if !exe {
        return Err(format!("{} is not a Black Ops II install (no t6*.exe)", dir.display()));
    }
    let zone = dir.join("zone").join("all");
    if !zone.join("common_mp.ff").is_file() && !zone.join("common.ff").is_file() {
        return Err(format!("{}: Black Ops II fastfiles (zone\\all\\common*.ff) are missing", dir.display()));
    }
    let has_sounds = fs::read_dir(dir.join("sound"))
        .map(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "sabs" || x == "sabl")))
        .unwrap_or(false);
    if !has_sounds {
        return Err(format!("{}: Black Ops II sound banks (sound\\*.sabs) are missing", dir.display()));
    }
    Ok(())
}

fn find_zero_hour(libraries: &[PathBuf]) -> Result<GameInstall, String> {
    if let Some(dir) = std::env::var_os("ZHBO_ZH_DIR").map(PathBuf::from) {
        validate_zero_hour(&dir)?;
        return Ok(GameInstall { path: dir, source: "ZHBO_ZH_DIR".into() });
    }
    if let Some(found) = find_in_steam(libraries, ZH_STEAM_APPS, validate_zero_hour) {
        return Ok(found);
    }
    if let Some(dir) = registry_zero_hour_path() {
        if validate_zero_hour(&dir).is_ok() {
            return Ok(GameInstall { path: dir, source: "EA registry".into() });
        }
    }
    Err("Command & Conquer Generals: Zero Hour is not installed. Install it (e.g. from Steam) to play Conquer Ops.".into())
}

fn find_black_ops2(libraries: &[PathBuf]) -> Result<GameInstall, String> {
    if let Some(dir) = std::env::var_os("ZHBO_BO2_DIR").map(PathBuf::from) {
        validate_black_ops2(&dir)?;
        return Ok(GameInstall { path: dir, source: "ZHBO_BO2_DIR".into() });
    }
    if let Some(found) = find_in_steam(libraries, BO2_STEAM_APPS, validate_black_ops2) {
        return Ok(found);
    }
    Err("Call of Duty: Black Ops II is not installed. Install it from Steam to play Conquer Ops.".into())
}

fn find_in_steam(libraries: &[PathBuf], apps: &[u32], validate: fn(&Path) -> Result<(), String>) -> Option<GameInstall> {
    for lib in libraries {
        let steamapps = lib.join("steamapps");
        for &app in apps {
            let Ok(manifest) = fs::read_to_string(steamapps.join(format!("appmanifest_{app}.acf"))) else { continue };
            let Some(installdir) = vdf_value(&manifest, "installdir") else { continue };
            let dir = steamapps.join("common").join(installdir);
            if validate(&dir).is_ok() {
                return Some(GameInstall { path: dir, source: format!("Steam app {app}") });
            }
        }
    }
    None
}

/// Steam library roots: the Steam install plus every `path` in `libraryfolders.vdf`.
pub fn steam_libraries() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !roots.iter().any(|r| same_path(r, &p)) {
            roots.push(p);
        }
    };
    for steam in steam_install_dirs() {
        push(steam.clone());
        if let Ok(vdf) = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
            for path in vdf_values(&vdf, "path") {
                push(PathBuf::from(path));
            }
        }
    }
    roots
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase();
    norm(a) == norm(b)
}

fn steam_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(p) = reg::read_string(reg::HKCU, r"Software\Valve\Steam", "SteamPath") {
            dirs.push(PathBuf::from(p));
        }
        if let Some(p) = reg::read_string(reg::HKLM, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath") {
            dirs.push(PathBuf::from(p));
        }
    }
    dirs.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    dirs.retain(|d| d.join("steamapps").is_dir());
    dirs
}

fn registry_zero_hour_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return reg::read_string(
            reg::HKLM,
            r"SOFTWARE\WOW6432Node\Electronic Arts\EA Games\Command and Conquer Generals Zero Hour",
            "InstallPath",
        )
        .map(PathBuf::from);
    }
    #[allow(unreachable_code)]
    None
}

fn registry_generals_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return reg::read_string(reg::HKLM, r"SOFTWARE\WOW6432Node\Electronic Arts\EA Games\Generals", "InstallPath")
            .map(PathBuf::from);
    }
    #[allow(unreachable_code)]
    None
}

/// First `"key" "value"` pair in a Valve KeyValues text file.
pub fn vdf_value(text: &str, key: &str) -> Option<String> {
    vdf_values(text, key).into_iter().next()
}

/// Every `"key" "value"` pair for `key` (case-insensitive), with `\\` unescaped.
pub fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let quoted: Vec<&str> = line.split('"').collect();
        // `"key"\t\t"value"` splits into ["", key, "\t\t", value, ""].
        if quoted.len() >= 5 && quoted[1].eq_ignore_ascii_case(key) {
            out.push(quoted[3].replace("\\\\", "\\"));
        }
    }
    out
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

    #[test]
    fn parses_library_folders_and_manifests() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(vdf_values(vdf, "path"), vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"202970\"\n\t\"installdir\"\t\t\"Call of Duty Black Ops II\"\n}\n";
        assert_eq!(vdf_value(acf, "installdir").as_deref(), Some("Call of Duty Black Ops II"));
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zhbo-req-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn validates_a_fake_black_ops2_folder() {
        let d = scratch("bo2");
        assert!(validate_black_ops2(&d).is_err());
        fs::write(d.join("t6mp.exe"), b"").unwrap();
        fs::create_dir_all(d.join("zone/all")).unwrap();
        fs::write(d.join("zone/all/common_mp.ff"), b"").unwrap();
        assert!(validate_black_ops2(&d).unwrap_err().contains("sound"));
        fs::create_dir_all(d.join("sound")).unwrap();
        fs::write(d.join("sound/mpl_common.all.sabs"), b"").unwrap();
        assert!(validate_black_ops2(&d).is_ok());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn validates_a_fake_zero_hour_folder() {
        let d = scratch("zh");
        for f in ["INIZH.big", "W3DZH.big", "TexturesZH.big"] {
            fs::write(d.join(f), b"").unwrap();
        }
        fs::create_dir_all(d.join("ZH_Generals")).unwrap();
        fs::write(d.join("ZH_Generals/INI.big"), b"").unwrap();
        assert!(validate_zero_hour(&d).is_ok());
        fs::remove_file(d.join("W3DZH.big")).unwrap();
        assert!(validate_zero_hour(&d).is_err());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    #[ignore = "inspects this machine's real installs"]
    fn real_machine() {
        let r = check_requirements();
        println!("{r:#?}");
    }
}
