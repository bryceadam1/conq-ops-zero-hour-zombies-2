//! Path helpers that behave the same on Windows, Linux and macOS.

use std::fs;
use std::path::{Path, PathBuf};

/// `dir/name`, matching `name` case-insensitively when the exact spelling is absent. Windows game
/// files are installed with whatever case the publisher used, and Linux filesystems are case-sensitive.
pub fn join_ci(dir: &Path, name: &str) -> PathBuf {
    let exact = dir.join(name);
    if exact.exists() {
        return exact;
    }
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                return e.path();
            }
        }
    }
    exact
}

/// Join several components case-insensitively (`join_ci_all(root, &["zone", "all", "mp.ipak"])`).
pub fn join_ci_all(root: &Path, parts: &[&str]) -> PathBuf {
    parts.iter().fold(root.to_path_buf(), |p, part| join_ci(&p, part))
}

pub fn is_file_ci(root: &Path, parts: &[&str]) -> bool {
    join_ci_all(root, parts).is_file()
}

/// Compare paths ignoring separators, trailing slashes and (Windows-style) case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_ascii_lowercase();
    norm(a) == norm(b)
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_files_regardless_of_case() {
        let d = std::env::temp_dir().join(format!("zhbo-ci-{}", std::process::id()));
        fs::create_dir_all(d.join("Zone").join("All")).unwrap();
        fs::write(d.join("Zone").join("All").join("MP.ipak"), b"").unwrap();
        assert!(is_file_ci(&d, &["zone", "all", "mp.ipak"]));
        assert!(!is_file_ci(&d, &["zone", "all", "base.ipak"]));
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn same_path_ignores_style() {
        assert!(same_path(Path::new("C:\\Steam\\"), Path::new("c:/steam")));
    }
}
