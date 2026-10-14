//! Steam libraries on Windows, Linux and macOS, including Windows Steam clients inside prefixes.

use crate::paths::{home, join_ci_all, same_path};
use crate::prefixes::Prefix;
use crate::vdf;
use std::fs;
use std::path::{Path, PathBuf};

/// A Steam library folder (contains `steamapps`), and the prefix whose Windows Steam owns it, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub root: PathBuf,
    pub prefix: Option<Prefix>,
}

/// Native Steam client installs on this OS (not inside a prefix).
pub fn native_roots() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        if let Some(p) = crate::reg::read_string(crate::reg::HKCU, r"Software\Valve\Steam", "SteamPath") {
            dirs.push(PathBuf::from(p));
        }
        if let Some(p) = crate::reg::read_string(crate::reg::HKLM, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath") {
            dirs.push(PathBuf::from(p));
        }
        dirs.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    }
    if let Some(h) = home() {
        if cfg!(target_os = "macos") {
            dirs.push(h.join("Library/Application Support/Steam"));
        } else if cfg!(unix) {
            for rel in [
                ".steam/steam",
                ".steam/root",
                ".local/share/Steam",
                ".var/app/com.valvesoftware.Steam/.local/share/Steam",
                "snap/steam/common/.local/share/Steam",
            ] {
                dirs.push(h.join(rel));
            }
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for d in dirs {
        // Collapse ~/.steam/steam style symlinks. Not on Windows, where it yields \\?\ paths.
        let d = if cfg!(unix) { fs::canonicalize(&d).unwrap_or(d) } else { d };
        if d.join("steamapps").is_dir() && !out.iter().any(|o| same_path(o, &d)) {
            out.push(d);
        }
    }
    out
}

/// Every library folder: native Steam clients and their `libraryfolders.vdf`, plus Windows Steam
/// clients inside the given prefixes (their Windows library paths are mapped back to host paths).
pub fn libraries(native: &[PathBuf], prefixes: &[Prefix]) -> Vec<Library> {
    let mut out: Vec<Library> = Vec::new();
    let mut push = |root: PathBuf, prefix: Option<Prefix>| {
        if root.join("steamapps").is_dir() && !out.iter().any(|l| same_path(&l.root, &root)) {
            out.push(Library { root, prefix });
        }
    };
    for steam in native {
        push(steam.clone(), None);
        for p in library_paths(steam) {
            push(PathBuf::from(p), None);
        }
    }
    for prefix in prefixes {
        for steam in [
            join_ci_all(&prefix.drive_c(), &["Program Files (x86)", "Steam"]),
            join_ci_all(&prefix.drive_c(), &["Program Files", "Steam"]),
        ] {
            if !steam.join("steamapps").is_dir() {
                continue;
            }
            push(steam.clone(), Some(prefix.clone()));
            for p in library_paths(&steam) {
                if let Some(host) = prefix.host_path(&p) {
                    push(host, Some(prefix.clone()));
                }
            }
        }
    }
    out
}

fn library_paths(steam: &Path) -> Vec<String> {
    fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf"))
        .map(|t| vdf::values(&t, "path"))
        .unwrap_or_default()
}

/// Install folder of `app` in `lib`, if its manifest is present.
pub fn app_dir(lib: &Library, app: u32) -> Option<PathBuf> {
    let steamapps = lib.root.join("steamapps");
    let manifest = fs::read_to_string(steamapps.join(format!("appmanifest_{app}.acf"))).ok()?;
    let installdir = vdf::value(&manifest, "installdir")?;
    Some(join_ci_all(&steamapps, &["common", &installdir]))
}
