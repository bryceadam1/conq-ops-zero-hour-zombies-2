//! ConquerOps launcher (`ConquerOps.exe` on Windows, `conquerops` on Linux/macOS).
//!
//! 1. Refuses to start unless both Command & Conquer Generals: Zero Hour and Call of Duty: Black Ops II
//!    are installed (the in-game core checks again, so the engine cannot be started around this).
//! 2. Builds `ConquerOps_BO2.big` (BO2 HUD art) from the player's own Black Ops II install if needed.
//! 3. Starts the mod's game executable (`generalszh.exe` on Windows, `generalszh` on macOS/Linux,
//!    next to this launcher) with the Zero Hour install as working directory and this folder as the
//!    `-mod` directory. The game is a native build everywhere, so no Wine is involved; it reads the
//!    Windows release's data files (on macOS/Linux, the Windows depots fetched through SteamCMD).
//!    Nothing is written into either game's folder.
//!
//! `--check` prints what was found and how the game would be started, without starting it.
//! Other arguments are passed to the game, e.g. `-win -xres 1280 -yres 720`.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The game executable staged next to the launcher by `cargo xtask build`.
const GAME_EXE: &str = if cfg!(windows) { "generalszh.exe" } else { "generalszh" };

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check_only = args.iter().any(|a| a == "--check");
    let game_args: Vec<String> = args.into_iter().filter(|a| a != "--check").collect();
    match launch(&game_args, check_only) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            // --check is run from a terminal; a modal dialog would only block it.
            if check_only {
                eprintln!("Conquer Ops: {msg}");
            } else {
                show_error(&msg);
            }
            ExitCode::FAILURE
        }
    }
}

fn launch(game_args: &[String], check_only: bool) -> Result<(), String> {
    let env = zhbo_requirements::environment();
    let req = zhbo_requirements::check_in(&env);
    if check_only {
        println!("Steam (native): {:?}", env.native_steam);
        println!("Wine prefixes:  {:?}", env.prefixes.iter().map(|p| (p.kind, &p.root)).collect::<Vec<_>>());
        println!("Zero Hour:      {:?}", req.zero_hour);
        println!("Black Ops II:   {:?}", req.black_ops2);
    }
    if !req.ok() {
        return Err(format!(
            "Conquer Ops requires both Command & Conquer Generals: Zero Hour and Call of Duty: Black Ops II.\n\n{}",
            req.problems()
        ));
    }
    let zh = req.zero_hour.clone().unwrap();
    let bo2 = req.black_ops2.clone().unwrap();

    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .ok_or("cannot locate the Conquer Ops folder")?;
    let game = here.join(GAME_EXE);
    if !game.is_file() {
        return Err(format!("{} is missing. Rebuild with `cargo xtask build`.", game.display()));
    }
    if !here.join("ConquerOps.big").is_file() {
        return Err(format!("ConquerOps.big is missing from {}", here.display()));
    }
    prepare_bo2_art(&here, &bo2.path);

    let mut mod_dir = here.to_string_lossy().into_owned();
    if !mod_dir.ends_with(std::path::MAIN_SEPARATOR) {
        mod_dir.push(std::path::MAIN_SEPARATOR);
    }
    let mut cmd = Command::new(&game);
    cmd.args(["-useCwd", "-mod", &mod_dir]).args(game_args);
    // The in-game requirements check reuses what was found here.
    cmd.current_dir(&zh.path).env("ZHBO_ZH_DIR", &zh.path).env("ZHBO_BO2_DIR", &bo2.path);
    // Linux builds ship their non-system shared libraries in libs/ (the engine's deploy script).
    let libs = here.join("libs");
    if cfg!(target_os = "linux") && libs.is_dir() {
        let mut path = libs.into_os_string();
        if let Some(old) = std::env::var_os("LD_LIBRARY_PATH") {
            path.push(":");
            path.push(old);
        }
        cmd.env("LD_LIBRARY_PATH", path);
    }
    if check_only {
        println!("Command:        {cmd:?}");
        return Ok(());
    }
    cmd.spawn().map_err(|e| format!("failed to start {}: {e}", game.display()))?;
    Ok(())
}

/// Build `ConquerOps_BO2.big` (BO2 HUD art from the player's own install) next to the launcher if
/// it is missing or from an older converter. Failure is not fatal: the HUD has drawn fallbacks.
fn prepare_bo2_art(here: &Path, bo2: &Path) {
    use zhbo_assets::hud;
    let out = here.join(hud::ARCHIVE_NAME);
    if hud::archive_version(&out) == Some(hud::VERSION) {
        return;
    }
    let line = match hud::build_archive(bo2) {
        Ok((bytes, report)) => match std::fs::write(&out, bytes) {
            Ok(()) => format!(
                "built {} ({} images, missing: {:?})\nGenerals: {}",
                out.display(),
                report.found.len(),
                report.missing,
                match &report.generals {
                    Some(Ok(summary)) => summary.clone(),
                    Some(Err(e)) => format!("kept Zero Hour art ({e})"),
                    None => "not converted".into(),
                }
            ),
            Err(e) => format!("could not write {}: {e}", out.display()),
        },
        Err(e) => format!("could not read Black Ops II art: {e}"),
    };
    let _ = std::fs::write(here.join("ConquerOps-launcher.log"), line + "\n");
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
    eprintln!("Conquer Ops: {msg}");
    // Also show a dialog when started from a desktop shortcut rather than a terminal.
    if cfg!(target_os = "macos") {
        let script = format!("display alert \"Conquer Ops\" message {msg:?} as critical");
        let _ = std::process::Command::new("osascript").args(["-e", &script]).status();
    } else {
        let _ = std::process::Command::new("zenity").args(["--error", "--title=Conquer Ops", "--text", msg]).status();
    }
}
