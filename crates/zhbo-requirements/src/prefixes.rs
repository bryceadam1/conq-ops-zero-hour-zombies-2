//! Wine-style prefixes (plain Wine, Proton `pfx`, CrossOver bottles, Whisky bottles) and the
//! mapping between their Windows paths and host paths.
//!
//! On Linux and macOS the games are Windows programs, installed either by a native Steam client
//! (Linux, run through Proton) or by a Windows Steam client inside a prefix (Wine, CrossOver,
//! Whisky). Conquer Ops runs the same way, so it needs to find both kinds.

use crate::paths::home;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefixKind {
    Wine,
    Proton,
    CrossOver,
    Whisky,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefix {
    pub kind: PrefixKind,
    /// The directory containing `drive_c` (for Proton: the `pfx` directory).
    pub root: PathBuf,
}

impl Prefix {
    pub fn drive_c(&self) -> PathBuf {
        self.root.join("drive_c")
    }

    /// Host path for a Windows path inside this prefix (`C:\Games` -> `<root>/drive_c/Games`,
    /// other letters through `dosdevices`).
    pub fn host_path(&self, windows: &str) -> Option<PathBuf> {
        let w = windows.replace('/', "\\");
        let (drive, rest) = w.split_once(":\\").or_else(|| w.split_once(':'))?;
        let drive = drive.to_ascii_lowercase();
        if drive.len() != 1 {
            return None;
        }
        let base = if drive == "c" {
            self.drive_c()
        } else {
            let link = self.root.join("dosdevices").join(format!("{drive}:"));
            fs::canonicalize(&link).unwrap_or(link)
        };
        Some(rest.split('\\').filter(|s| !s.is_empty()).fold(base, |p, s| p.join(s)))
    }

    /// The Windows path a program inside this prefix sees for a host path.
    pub fn windows_path(&self, host: &Path) -> String {
        if let Ok(rel) = host.strip_prefix(self.drive_c()) {
            return format!("C:\\{}", rel.to_string_lossy().replace('/', "\\"));
        }
        unix_to_wine_z(host)
    }
}

/// Wine maps the host root to `Z:`.
pub fn unix_to_wine_z(host: &Path) -> String {
    format!("Z:{}", host.to_string_lossy().replace('/', "\\"))
}

fn is_prefix(dir: &Path) -> bool {
    dir.join("drive_c").is_dir()
}

fn children(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

/// Every prefix we know how to look for on this machine. `steam_roots` are native Steam installs,
/// whose `steamapps/compatdata/<appid>/pfx` folders are Proton prefixes.
pub fn discover(steam_roots: &[PathBuf]) -> Vec<Prefix> {
    let mut out: Vec<Prefix> = Vec::new();
    let mut push = |kind: PrefixKind, root: PathBuf| {
        if is_prefix(&root) && !out.iter().any(|p| p.root == root) {
            out.push(Prefix { kind, root });
        }
    };
    if let Some(p) = std::env::var_os("WINEPREFIX") {
        push(PrefixKind::Wine, PathBuf::from(p));
    }
    if let Some(h) = home() {
        push(PrefixKind::Wine, h.join(".wine"));
        for d in children(&h.join(".local/share/wineprefixes")) {
            push(PrefixKind::Wine, d);
        }
        for d in children(&h.join("Games")) {
            push(PrefixKind::Wine, d); // Lutris default layout
        }
        for d in children(&h.join("Library/Application Support/CrossOver/Bottles")) {
            push(PrefixKind::CrossOver, d);
        }
        for d in children(&h.join("Library/Containers/com.isaacmarovitz.Whisky/Bottles")) {
            push(PrefixKind::Whisky, d);
        }
    }
    for s in steam_roots {
        for d in children(&s.join("steamapps").join("compatdata")) {
            push(PrefixKind::Proton, d.join("pfx"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paths_both_ways() {
        let p = Prefix { kind: PrefixKind::Wine, root: PathBuf::from("/home/me/.wine") };
        assert_eq!(
            p.host_path(r"C:\Program Files (x86)\Steam").unwrap(),
            PathBuf::from("/home/me/.wine/drive_c/Program Files (x86)/Steam")
        );
        assert_eq!(
            p.windows_path(Path::new("/home/me/.wine/drive_c/Games/ZH")),
            r"C:\Games\ZH"
        );
        assert_eq!(p.windows_path(Path::new("/opt/conquerops")), r"Z:\opt\conquerops");
        assert!(p.host_path("not a path").is_none());
    }
}
