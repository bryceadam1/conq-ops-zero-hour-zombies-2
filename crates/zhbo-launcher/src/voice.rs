//! The voice models (docs/COMMANDS.md section 11): downloaded at install, never shipped.
//!
//! `data/ConquerOps/voice.ini` lists each model's folder and files (published path, size, SHA-256).
//! At launch, if any file is missing, the launcher starts itself again with `--fetch-voice` in the
//! background and goes on starting the game: voice turns on once the files are in. The fetch
//! downloads with the system's `curl` (shipped with Windows 10+, macOS and Linux distributions)
//! into `<name>.part`, checks the size and hash, and renames it into place, so a file present
//! under its own name is whole. Nothing here is fatal: without the models the game plays as before.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The manifest, one source for the launcher and the docs.
pub const MANIFEST: &str = include_str!("../../../data/ConquerOps/voice.ini");
/// Where the models go, beside the launcher (and the converted archive).
pub const DIR: &str = "voice";
/// A fetch running, or one that died: the next launch ignores a lock older than this.
const STALE_LOCK_SECS: u64 = 3 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// The model's folder under [`DIR`].
    pub dir: String,
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// The manifest's files: `[Model]` sections, each with `Dir = <folder>` and lines
/// `File = <name> | <url> | <size> | <sha256>`.
pub fn files(manifest: &str) -> Result<Vec<File>, String> {
    let mut out = Vec::new();
    let mut dir: Option<String> = None;
    for (n, line) in manifest.lines().enumerate() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            dir = None;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { return Err(format!("voice.ini line {}: no `=`", n + 1)) };
        match key.trim() {
            "Dir" => dir = Some(value.trim().to_string()),
            "File" => {
                let f: Vec<&str> = value.split('|').map(str::trim).collect();
                let (Some(dir), [name, url, size, sha256]) = (&dir, f.as_slice()) else {
                    return Err(format!("voice.ini line {}: a file needs its section's Dir and name | url | size | sha256", n + 1));
                };
                let size = size.parse().map_err(|_| format!("voice.ini line {}: size {size:?}", n + 1))?;
                out.push(File { dir: dir.clone(), name: name.to_string(), url: url.to_string(), size, sha256: sha256.to_lowercase() });
            }
            other => return Err(format!("voice.ini line {}: unknown key {other:?}", n + 1)),
        }
    }
    Ok(out)
}

fn path(here: &Path, f: &File) -> PathBuf {
    here.join(DIR).join(&f.dir).join(&f.name)
}

/// The files not yet in place (absent, or of the wrong size).
pub fn missing(here: &Path, files: &[File]) -> Vec<File> {
    files.iter().filter(|f| std::fs::metadata(path(here, f)).map_or(true, |m| m.len() != f.size)).cloned().collect()
}

/// Start the background fetch if anything is missing and no fetch is running.
pub fn ensure(here: &Path) {
    let Ok(files) = files(MANIFEST) else { return };
    if missing(here, &files).is_empty() || lock_held(here) {
        return;
    }
    let Ok(me) = std::env::current_exe() else { return };
    let mut cmd = Command::new(me);
    cmd.arg("--fetch-voice").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

fn lock(here: &Path) -> PathBuf {
    here.join(DIR).join("fetching.lock")
}

fn lock_held(here: &Path) -> bool {
    std::fs::metadata(lock(here)).ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() < STALE_LOCK_SECS)
}

/// `--fetch-voice`: download every missing file, logging to `voice/fetch.log`.
pub fn fetch(here: &Path) -> Result<(), String> {
    let files = files(MANIFEST)?;
    std::fs::create_dir_all(here.join(DIR)).map_err(|e| e.to_string())?;
    if lock_held(here) {
        return Err("another fetch is running".into());
    }
    std::fs::write(lock(here), std::process::id().to_string()).map_err(|e| e.to_string())?;
    let mut log = Vec::new();
    let mut result = Ok(());
    for f in missing(here, &files) {
        match fetch_one(here, &f) {
            Ok(()) => log.push(format!("fetched {}/{} ({} bytes)", f.dir, f.name, f.size)),
            Err(e) => {
                log.push(format!("could not fetch {}/{}: {e}", f.dir, f.name));
                result = Err(e);
            }
        }
    }
    let _ = std::fs::remove_file(lock(here));
    let _ = std::fs::write(here.join(DIR).join("fetch.log"), log.join("\n") + "\n");
    result
}

fn fetch_one(here: &Path, f: &File) -> Result<(), String> {
    let dest = path(here, f);
    let dir = dest.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let part = dir.join(format!("{}.part", f.name));
    let ok = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "--retry-delay", "5", "-o"])
        .arg(&part)
        .arg(&f.url)
        .status()
        .map_err(|e| format!("curl: {e}"))?
        .success();
    if !ok {
        let _ = std::fs::remove_file(&part);
        return Err(format!("curl failed for {}", f.url));
    }
    let size = std::fs::metadata(&part).map_err(|e| e.to_string())?.len();
    let hash = sha256(&part)?;
    if size != f.size || hash != f.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} came back as {size} bytes, sha256 {hash}; the manifest says {} bytes, {}", f.url, f.size, f.sha256));
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())
}

/// A file's SHA-256, in lowercase hex.
pub fn sha256(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_reads() {
        let listed = files(MANIFEST).unwrap();
        assert!(!listed.is_empty());
        for f in &listed {
            assert!(f.url.starts_with("https://"), "{f:?}");
            assert_eq!(f.sha256.len(), 64, "{f:?}");
            assert!(f.size > 0 && !f.dir.is_empty() && !f.name.contains('/'), "{f:?}");
        }
        assert!(files("File = a | b | 1 | c").is_err(), "a file outside a model's section");
    }

    #[test]
    fn a_file_is_missing_until_it_is_whole() {
        let here = std::env::temp_dir().join(format!("zhbo-voice-test-{}", std::process::id()));
        let f = File { dir: "m".into(), name: "w.bin".into(), url: "https://example.invalid/w.bin".into(), size: 3, sha256: "0".repeat(64) };
        std::fs::create_dir_all(here.join(DIR).join("m")).unwrap();
        assert_eq!(missing(&here, std::slice::from_ref(&f)).len(), 1);
        std::fs::write(path(&here, &f), b"ab").unwrap();
        assert_eq!(missing(&here, std::slice::from_ref(&f)).len(), 1, "a short file is not whole");
        std::fs::write(path(&here, &f), b"abc").unwrap();
        assert!(missing(&here, std::slice::from_ref(&f)).is_empty());
        assert_eq!(sha256(&path(&here, &f)).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_dir_all(&here);
    }
}
