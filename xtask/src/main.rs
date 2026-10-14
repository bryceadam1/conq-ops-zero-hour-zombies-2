//! `cargo xtask <command>` - builds Conquer Ops.
//!
//! ```text
//! cargo xtask build [--log] [--dist DIR]   Rust core + patched Zero Hour + ConquerOps.big + launcher
//! cargo xtask pack  [--dist DIR]           only repack data/ into ConquerOps.big
//! cargo xtask run   [--log] [-- ARGS]      build, then start the game through the launcher
//! cargo xtask patch                        prepare the patched engine checkout only
//! ```
//!
//! The engine is built out of the repository (the repo may live on a slow network share) in
//! `%LOCALAPPDATA%\ConquerOps\work`, or `ZHBO_WORK_DIR`. The engine source is the commit pinned by the
//! `host/GeneralsGameCode` submodule, with `host/patches/*.patch` applied on top.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, String>;

const ENGINE_URL: &str = "https://github.com/TheSuperHackers/GeneralsGameCode.git";

struct Opts {
    log_build: bool,
    dist: Option<PathBuf>,
    game_args: Vec<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        eprintln!("usage: cargo xtask <build|pack|run|patch> [--log] [--dist DIR] [-- GAME_ARGS]");
        return ExitCode::FAILURE;
    };
    let mut opts = Opts { log_build: false, dist: None, game_args: Vec::new() };
    let mut it = args.into_iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--log" => opts.log_build = true,
            "--dist" => opts.dist = it.next().map(PathBuf::from),
            "--" => opts.game_args = it.by_ref().collect(),
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let result = match cmd.as_str() {
        "build" => build(&opts).map(|_| ()),
        "pack" => pack(&dist_dir(&opts)).map(|_| ()),
        "run" => build(&opts).and_then(|dist| run(&dist, &opts.game_args)),
        "patch" => prepare_engine().map(|_| ()),
        other => Err(format!("unknown command {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn work_dir() -> PathBuf {
    if let Some(d) = env::var_os("ZHBO_WORK_DIR") {
        return PathBuf::from(d);
    }
    let base = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    base.join("ConquerOps").join("work")
}

fn dist_dir(opts: &Opts) -> PathBuf {
    opts.dist.clone().unwrap_or_else(|| work_dir().parent().unwrap().join("dist"))
}

fn run_cmd(cmd: &mut Command) -> Result<()> {
    eprintln!("> {cmd:?}");
    let status = cmd.status().map_err(|e| format!("failed to start {:?}: {e}", cmd.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{:?} failed with {status}", cmd.get_program()))
    }
}

fn capture(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().map_err(|e| format!("failed to start {:?}: {e}", cmd.get_program()))?;
    if !out.status.success() {
        return Err(format!("{cmd:?} failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    // The repository may sit on a network share that git considers "unsafe".
    c.arg("-c").arg("safe.directory=*").arg("-C").arg(dir);
    c
}

// ------------------------------------------------------------------------------------------------
// Engine checkout
// ------------------------------------------------------------------------------------------------

fn pinned_engine_commit() -> Result<String> {
    let out = capture(git(&repo_root()).args(["ls-tree", "HEAD", "host/GeneralsGameCode"]))?;
    // "160000 commit <sha>\thost/GeneralsGameCode"
    out.split_whitespace()
        .nth(2)
        .map(str::to_string)
        .ok_or_else(|| "host/GeneralsGameCode is not a submodule in HEAD".to_string())
}

fn patch_files() -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(repo_root().join("host/patches"))
        .map_err(|e| format!("host/patches: {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "patch"))
        .collect();
    v.sort();
    Ok(v)
}

fn fnv1a(data: &[u8], mut h: u64) -> u64 {
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Clone (once) and reset the engine to the pinned commit with our patches applied. Skips the
/// reset when nothing changed so incremental builds stay incremental.
fn prepare_engine() -> Result<PathBuf> {
    let work = work_dir();
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let engine = work.join("GeneralsGameCode");
    let commit = pinned_engine_commit()?;
    let patches = patch_files()?;

    let mut stamp = fnv1a(commit.as_bytes(), 0xcbf2_9ce4_8422_2325);
    for p in &patches {
        stamp = fnv1a(&fs::read(p).map_err(|e| e.to_string())?, stamp);
    }
    let stamp = format!("{commit} {stamp:016x}");
    let stamp_file = work.join("engine.stamp");
    if engine.join(".git").exists() && fs::read_to_string(&stamp_file).ok().as_deref() == Some(stamp.as_str()) {
        return Ok(engine);
    }

    if !engine.join(".git").exists() {
        let local = repo_root().join("host/GeneralsGameCode");
        let source = if local.join(".git").exists() { local.to_string_lossy().into_owned() } else { ENGINE_URL.to_string() };
        run_cmd(Command::new("git").args(["-c", "safe.directory=*", "clone", "--no-checkout", &source]).arg(&engine))?;
    }
    let have_commit = git(&engine)
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_commit {
        run_cmd(git(&engine).args(["fetch", ENGINE_URL, &commit]))?;
    }
    run_cmd(git(&engine).args(["reset", "--hard", "--quiet", &commit]))?;
    for p in &patches {
        run_cmd(git(&engine).arg("apply").arg("--whitespace=nowarn").arg(p))?;
    }
    fs::write(&stamp_file, stamp).map_err(|e| e.to_string())?;
    Ok(engine)
}

fn copy_dir_if_changed(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_if_changed(&from, &to)?;
        } else {
            let data = fs::read(&from).map_err(|e| e.to_string())?;
            if fs::read(&to).ok().as_deref() != Some(data.as_slice()) {
                fs::write(&to, data).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Toolchain
// ------------------------------------------------------------------------------------------------

fn vcvars() -> Result<PathBuf> {
    let vswhere = PathBuf::from(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    let install = capture(Command::new(&vswhere).args([
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
    ]))
    .map_err(|e| format!("Visual Studio 2022 C++ build tools not found ({e})"))?;
    let build = PathBuf::from(install.lines().next().unwrap_or_default()).join(r"VC\Auxiliary\Build");
    // Native ARM64 compilers targeting x86 are much faster than emulated x86 ones on ARM machines.
    let arch = env::var("PROCESSOR_ARCHITEW6432").or_else(|_| env::var("PROCESSOR_ARCHITECTURE")).unwrap_or_default();
    let candidates: &[&str] = if arch.eq_ignore_ascii_case("ARM64") {
        &["vcvarsarm64_x86.bat", "vcvars32.bat"]
    } else {
        &["vcvarsamd64_x86.bat", "vcvars32.bat"]
    };
    candidates
        .iter()
        .map(|c| build.join(c))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("no x86 vcvars script in {}", build.display()))
}

fn cargo() -> Command {
    Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn rust_target_dir() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| repo_root().join("target"))
}

// ------------------------------------------------------------------------------------------------
// Commands
// ------------------------------------------------------------------------------------------------

fn build(opts: &Opts) -> Result<PathBuf> {
    let root = repo_root();
    run_cmd(cargo().current_dir(&root).args(["build", "--release", "-p", "zhbo-core", "-p", "zhbo-launcher"]))?;
    let release = rust_target_dir().join("i686-pc-windows-msvc").join("release");
    let core_lib = release.join("zhbo_core.lib");

    let engine = prepare_engine()?;
    let glue = work_dir().join("zhbo-glue");
    copy_dir_if_changed(&root.join("host/zhbo"), &glue)?;

    let preset_dir = if opts.log_build { "win32-zhbo-log" } else { "win32-zhbo" };
    let fwd = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let extra = if opts.log_build { " -DRTS_DEBUG_LOGGING=ON -DRTS_DEBUG_CRASHING=ON" } else { "" };
    let script = work_dir().join("build-engine.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\ncall \"{vcvars}\" >nul\r\ncd /d \"{engine}\"\r\n\
             cmake --preset win32 -B build/{preset_dir} -DZHBO_GLUE_DIR={glue} -DZHBO_CORE_LIB={lib}{extra}\r\n\
             if errorlevel 1 exit /b 1\r\n\
             cmake --build build/{preset_dir} --config Release --target z_generals\r\n",
            vcvars = vcvars()?.display(),
            engine = engine.display(),
            glue = fwd(&glue),
            lib = fwd(&core_lib),
        ),
    )
    .map_err(|e| e.to_string())?;
    run_cmd(Command::new("cmd").arg("/c").arg(&script).current_dir(work_dir()))?;

    let dist = dist_dir(opts);
    fs::create_dir_all(&dist).map_err(|e| e.to_string())?;
    let exe_dir = engine.join("build").join(preset_dir).join("GeneralsMD").join("Release");
    for f in ["generalszh.exe", "generalszh.pdb"] {
        fs::copy(exe_dir.join(f), dist.join(f)).map_err(|e| format!("copy {f}: {e}"))?;
    }
    fs::copy(release.join("zhbo-launcher.exe"), dist.join("ConquerOps.exe")).map_err(|e| format!("copy launcher: {e}"))?;
    pack(&dist)?;
    eprintln!("Conquer Ops staged in {}", dist.display());
    Ok(dist)
}

/// Pack `data/` into `ConquerOps.big`. Every file keeps its path relative to `data/`.
fn pack(dist: &Path) -> Result<PathBuf> {
    let data = repo_root().join("data");
    let mut files = Vec::new();
    collect(&data, &data, &mut files)?;
    files.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    let bytes = zhbo_formats::big::write_archive(&files);
    fs::create_dir_all(dist).map_err(|e| e.to_string())?;
    let out = dist.join("ConquerOps.big");
    fs::write(&out, bytes).map_err(|e| e.to_string())?;
    eprintln!("packed {} files into {}", files.len(), out.display());
    Ok(out)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(root, &p, out)?;
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('/', "\\");
            out.push((rel, fs::read(&p).map_err(|e| e.to_string())?));
        }
    }
    Ok(())
}

fn run(dist: &Path, game_args: &[String]) -> Result<()> {
    run_cmd(Command::new(dist.join("ConquerOps.exe")).args(game_args))
}
