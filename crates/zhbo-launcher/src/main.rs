//! ConquerOps.exe - the mod's launcher.
//!
//! 1. Refuses to start unless both Command & Conquer Generals: Zero Hour and Call of Duty: Black Ops II
//!    are installed (the in-game core checks again, so the engine cannot be started around this).
//! 2. Starts the mod's `generalszh.exe` (next to this launcher) with the Zero Hour install as working
//!    directory and this folder as the `-mod` directory, so `ConquerOps.big` is loaded on top of the
//!    game's own archives. Nothing is written into either game's folder.
//!
//! Extra command-line arguments are passed to the game, e.g. `ConquerOps.exe -win -xres 1280 -yres 720`.

#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            show_error(&msg);
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), String> {
    let req = zhbo_requirements::check_requirements();
    if !req.ok() {
        return Err(format!(
            "Conquer Ops requires both Command & Conquer Generals: Zero Hour and Call of Duty: Black Ops II.\n\n{}",
            req.problems()
        ));
    }
    let zh = req.zero_hour.unwrap();

    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .ok_or("cannot locate the Conquer Ops folder")?;
    let game = here.join("generalszh.exe");
    if !game.is_file() {
        return Err(format!("{} is missing. Rebuild with `cargo xtask build`.", game.display()));
    }
    if !here.join("ConquerOps.big").is_file() {
        return Err(format!("ConquerOps.big is missing from {}", here.display()));
    }

    let mut mod_dir = here.to_string_lossy().into_owned();
    if !mod_dir.ends_with('\\') {
        mod_dir.push('\\');
    }
    Command::new(&game)
        .current_dir(&zh.path)
        .arg("-useCwd")
        .arg("-mod")
        .arg(&mod_dir)
        .args(std::env::args().skip(1))
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", game.display()))?;
    Ok(())
}

#[cfg(windows)]
fn show_error(msg: &str) {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    const MB_OK: u32 = 0x0;
    const MB_ICONERROR: u32 = 0x10;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(msg), wide("Conquer Ops"));
    // SAFETY: both strings are NUL-terminated and live for the call.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

#[cfg(not(windows))]
fn show_error(msg: &str) {
    eprintln!("{msg}");
}
