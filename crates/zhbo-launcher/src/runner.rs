//! How to start the (Windows) game executable on this OS.
//!
//! * Windows: directly.
//! * Linux: through Proton when Zero Hour came from a native Steam library (reusing Zero Hour's own
//!   Proton prefix), or through Wine when it lives inside a Wine prefix.
//! * macOS: through the same CrossOver or Whisky bottle the games are installed in, or plain Wine.
//!
//! Overrides: `ZHBO_PROTON=/path/to/proton`, `ZHBO_WINE=/path/to/wine`, `WINEPREFIX`.

use std::path::{Path, PathBuf};
use std::process::Command;
use zhbo_requirements::paths::home;
use zhbo_requirements::prefixes::{unix_to_wine_z, Prefix, PrefixKind};
use zhbo_requirements::{Environment, GameInstall};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runner {
    Native,
    Wine { wine: PathBuf, prefix: Option<Prefix>, args: Vec<String> },
    Proton { proton: PathBuf, compat_data: PathBuf, steam_root: PathBuf },
}

impl Runner {
    pub fn detect(zh: &GameInstall, env: &Environment) -> Result<Runner, String> {
        if cfg!(windows) {
            return Ok(Runner::Native);
        }
        if let Some(wine) = std::env::var_os("ZHBO_WINE").map(PathBuf::from) {
            return Ok(Runner::Wine { wine, prefix: zh.prefix.clone(), args: vec![] });
        }
        if let Some(proton) = std::env::var_os("ZHBO_PROTON").map(PathBuf::from) {
            return proton_runner(proton, zh, env);
        }
        match &zh.prefix {
            Some(p) if p.kind == PrefixKind::Proton => {
                let proton = newest_proton(env).ok_or("Zero Hour is installed in a Proton prefix but no Proton was found")?;
                Ok(Runner::Proton {
                    proton,
                    compat_data: p.root.parent().unwrap_or(&p.root).to_path_buf(),
                    steam_root: env.native_steam.first().cloned().unwrap_or_default(),
                })
            }
            Some(p) if p.kind == PrefixKind::CrossOver => {
                let wine = PathBuf::from("/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wine");
                let bottle = p.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                Ok(Runner::Wine { wine, prefix: Some(p.clone()), args: vec!["--bottle".into(), bottle] })
            }
            Some(p) if p.kind == PrefixKind::Whisky => {
                let wine = home()
                    .map(|h| h.join("Library/Application Support/com.isaacmarovitz.Whisky/Libraries/Wine/bin/wine64"))
                    .filter(|w| w.is_file())
                    .ok_or("Whisky's Wine was not found")?;
                Ok(Runner::Wine { wine, prefix: Some(p.clone()), args: vec![] })
            }
            Some(p) => Ok(Runner::Wine { wine: find_wine().ok_or("wine is not installed")?, prefix: Some(p.clone()), args: vec![] }),
            None => {
                // Native Steam library: prefer Proton (how Steam itself runs Zero Hour on Linux).
                if let Some(proton) = newest_proton(env) {
                    return proton_runner(proton, zh, env);
                }
                let wine = find_wine().ok_or("neither Proton nor Wine was found; install one to run Conquer Ops")?;
                Ok(Runner::Wine { wine, prefix: None, args: vec![] })
            }
        }
    }

    /// The path a program started by this runner sees for a host path.
    pub fn windows_path(&self, host: &Path) -> String {
        match self {
            Runner::Native => host.display().to_string(),
            Runner::Wine { prefix: Some(p), .. } => p.windows_path(host),
            Runner::Wine { prefix: None, .. } | Runner::Proton { .. } => unix_to_wine_z(host),
        }
    }

    pub fn command(&self, exe: &Path, args: &[String]) -> Command {
        match self {
            Runner::Native => {
                let mut c = Command::new(exe);
                c.args(args);
                c
            }
            Runner::Wine { wine, prefix, args: extra } => {
                let mut c = Command::new(wine);
                c.args(extra).arg(exe).args(args);
                if let Some(p) = prefix.as_ref().filter(|p| p.kind != PrefixKind::CrossOver) {
                    c.env("WINEPREFIX", &p.root);
                }
                c
            }
            Runner::Proton { proton, compat_data, steam_root } => {
                let mut c = Command::new(proton);
                c.arg("run").arg(exe).args(args);
                c.env("STEAM_COMPAT_DATA_PATH", compat_data).env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_root);
                c
            }
        }
    }
}

fn proton_runner(proton: PathBuf, zh: &GameInstall, env: &Environment) -> Result<Runner, String> {
    // Reuse Zero Hour's own Proton prefix: <library>/steamapps/compatdata/2732960.
    let library = zh.path.ancestors().find(|a| a.file_name().is_some_and(|n| n == "steamapps")).map(Path::to_path_buf);
    let compat_data = library
        .map(|l| l.join("compatdata").join(zhbo_requirements::ZH_STEAM_APPS[0].to_string()))
        .ok_or("cannot work out Zero Hour's Steam library for its Proton prefix")?;
    std::fs::create_dir_all(&compat_data).map_err(|e| format!("{}: {e}", compat_data.display()))?;
    Ok(Runner::Proton { proton, compat_data, steam_root: env.native_steam.first().cloned().unwrap_or_default() })
}

/// Newest Proton build in any Steam library or `compatibilitytools.d`.
fn newest_proton(env: &Environment) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut scan = |dir: PathBuf| {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                if name.contains("proton") && e.path().join("proton").is_file() {
                    candidates.push(e.path().join("proton"));
                }
            }
        }
    };
    for lib in env.libraries.iter().filter(|l| l.prefix.is_none()) {
        scan(lib.root.join("steamapps").join("common"));
    }
    for s in &env.native_steam {
        scan(s.join("compatibilitytools.d"));
    }
    candidates.sort_by_key(|p| proton_rank(p));
    candidates.pop()
}

/// Order Proton builds: "Experimental" newest, otherwise by version number in the folder name.
fn proton_rank(p: &Path) -> (u32, Vec<u32>) {
    let name = p.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let experimental = name.contains("experimental") as u32;
    let nums = name.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect();
    (experimental, nums)
}

fn find_wine() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in ["wine", "wine64"] {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    ["/opt/homebrew/bin/wine", "/usr/local/bin/wine", "/opt/wine-stable/bin/wine"].iter().map(PathBuf::from).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proton_ranking_prefers_experimental_then_newest() {
        let mut v = vec![
            PathBuf::from("/s/Proton 8.0/proton"),
            PathBuf::from("/s/Proton - Experimental/proton"),
            PathBuf::from("/s/Proton 9.0 (Beta)/proton"),
        ];
        v.sort_by_key(|p| proton_rank(p));
        assert!(v.last().unwrap().to_string_lossy().contains("Experimental"));
        assert!(v[1].to_string_lossy().contains("9.0"));
    }

    #[test]
    fn wine_paths_use_the_prefix_or_z_drive() {
        let p = Prefix { kind: PrefixKind::Wine, root: PathBuf::from("/home/me/.wine") };
        let r = Runner::Wine { wine: "wine".into(), prefix: Some(p), args: vec![] };
        assert_eq!(r.windows_path(Path::new("/home/me/.wine/drive_c/ZH")), r"C:\ZH");
        let proton = Runner::Proton { proton: "p".into(), compat_data: "c".into(), steam_root: "s".into() };
        assert_eq!(proton.windows_path(Path::new("/home/me/mods/ConquerOps")), r"Z:\home\me\mods\ConquerOps");
    }
}
