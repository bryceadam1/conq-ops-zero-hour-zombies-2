//! `cargo xtask <command>` - builds Conquer Ops.
//!
//! ```text
//! cargo xtask build [--log] [--dist DIR]   Rust core + patched Zero Hour + ConquerOps.big + launcher
//! cargo xtask pack  [--dist DIR]           only repack data/ into ConquerOps.big
//! cargo xtask run   [--log] [-- ARGS]      build, then start the game through the launcher
//! cargo xtask patch                        prepare the patched engine checkout only
//! cargo xtask doctor                       list missing build prerequisites for this OS
//! ```
//!
//! The engine is built out of the repository (the repo may live on a slow network share) in a
//! per-user work folder (`%LOCALAPPDATA%\ConquerOps\work`, `~/.local/share/ConquerOps/work`,
//! `~/Library/Application Support/ConquerOps/work`), or `ZHBO_WORK_DIR`. The engine source is the
//! commit pinned by the `host/GeneralsGameCode` submodule, with `host/patches/*.patch` applied on top.
//!
//! Windows builds with Visual Studio 2022. Linux and macOS cross-compile the same Windows game with
//! MinGW-w64 (the upstream `mingw-w64-i686` preset, Rust target `i686-pc-windows-gnu`) and build a
//! native launcher that runs it through Proton, Wine, CrossOver or Whisky.

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
        eprintln!("usage: cargo xtask <build|pack|run|patch|doctor> [--log] [--dist DIR] [-- GAME_ARGS]");
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
        "doctor" => doctor(),
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
    let home = || env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".local/share")))
    };
    base.unwrap_or_else(env::temp_dir).join("ConquerOps").join("work")
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

fn on_path(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|p| {
        env::split_paths(&p).any(|d| d.join(program).is_file() || d.join(format!("{program}.exe")).is_file())
    })
}

/// How the 32-bit Windows game is compiled on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Toolchain {
    /// Visual Studio 2022 on Windows.
    Msvc,
    /// MinGW-w64 cross compilers on Linux/macOS (upstream preset `mingw-w64-i686`).
    MinGw,
}

impl Toolchain {
    fn host() -> Self {
        if cfg!(windows) {
            Toolchain::Msvc
        } else {
            Toolchain::MinGw
        }
    }

    /// Rust target for the core that links into generalszh.exe.
    fn core_target(self) -> &'static str {
        match self {
            Toolchain::Msvc => "i686-pc-windows-msvc",
            Toolchain::MinGw => "i686-pc-windows-gnu",
        }
    }

    fn core_lib_name(self) -> &'static str {
        match self {
            Toolchain::Msvc => "zhbo_core.lib",
            Toolchain::MinGw => "libzhbo_core.a",
        }
    }

    /// Launcher: 32-bit Windows next to the game on Windows, native elsewhere.
    fn launcher_target(self) -> Option<&'static str> {
        match self {
            Toolchain::Msvc => Some("i686-pc-windows-msvc"),
            Toolchain::MinGw => None,
        }
    }

    fn launcher_file(self) -> (&'static str, &'static str) {
        match self {
            Toolchain::Msvc => ("zhbo-launcher.exe", "ConquerOps.exe"),
            Toolchain::MinGw => ("zhbo-launcher", "conquerops"),
        }
    }

    /// Missing prerequisites, as human-readable fixes.
    fn problems(self) -> Vec<String> {
        let mut out = Vec::new();
        if !on_path("git") {
            out.push("git is not on PATH".into());
        }
        let installed = capture(Command::new("rustup").args(["target", "list", "--installed"])).unwrap_or_default();
        if !installed.lines().any(|l| l.trim() == self.core_target()) {
            out.push(format!("Rust target missing: rustup target add {}", self.core_target()));
        }
        match self {
            Toolchain::Msvc => {
                if let Err(e) = vcvars() {
                    out.push(format!("{e} (needs the C++ x86/x64 tools, CMake, ATL and MFC components)"));
                }
            }
            Toolchain::MinGw => {
                if !on_path("cmake") {
                    out.push("cmake (3.25 or newer) is not on PATH".into());
                }
                if !on_path("i686-w64-mingw32-g++") {
                    out.push(
                        "MinGW-w64 i686 compilers are missing (Debian/Ubuntu: apt install g++-mingw-w64-i686 \
                         mingw-w64-tools; Fedora: dnf install mingw32-gcc-c++; macOS: brew install mingw-w64)"
                            .into(),
                    );
                }
                if !on_path("make") {
                    out.push("make is not on PATH".into());
                }
            }
        }
        out
    }
}

// ------------------------------------------------------------------------------------------------
// Commands
// ------------------------------------------------------------------------------------------------

fn doctor() -> Result<()> {
    let tc = Toolchain::host();
    eprintln!("toolchain: {tc:?} (core target {})", tc.core_target());
    eprintln!("work dir:  {}", work_dir().display());
    let problems = tc.problems();
    if problems.is_empty() {
        eprintln!("all build prerequisites found");
        Ok(())
    } else {
        Err(format!("missing build prerequisites:\n  {}", problems.join("\n  ")))
    }
}

fn build(opts: &Opts) -> Result<PathBuf> {
    let root = repo_root();
    let tc = Toolchain::host();
    let problems = tc.problems();
    if !problems.is_empty() {
        return Err(format!("missing build prerequisites:\n  {}", problems.join("\n  ")));
    }
    run_cmd(cargo().current_dir(&root).args(["build", "--release", "-p", "zhbo-core", "--target", tc.core_target()]))?;
    let mut launcher = cargo();
    launcher.current_dir(&root).args(["build", "--release", "-p", "zhbo-launcher"]);
    if let Some(t) = tc.launcher_target() {
        launcher.args(["--target", t]);
    }
    run_cmd(&mut launcher)?;
    let core_lib = rust_target_dir().join(tc.core_target()).join("release").join(tc.core_lib_name());
    let launcher_dir = match tc.launcher_target() {
        Some(t) => rust_target_dir().join(t).join("release"),
        None => rust_target_dir().join("release"),
    };

    let engine = prepare_engine()?;
    let glue = work_dir().join("zhbo-glue");
    copy_dir_if_changed(&root.join("host/zhbo"), &glue)?;
    let fwd = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut defs = vec![format!("-DZHBO_GLUE_DIR={}", fwd(&glue)), format!("-DZHBO_CORE_LIB={}", fwd(&core_lib))];
    if opts.log_build {
        defs.push("-DRTS_DEBUG_LOGGING=ON".into());
        defs.push("-DRTS_DEBUG_CRASHING=ON".into());
    }

    let exe_dir = match tc {
        Toolchain::Msvc => {
            let build_dir = if opts.log_build { "win32-zhbo-log" } else { "win32-zhbo" };
            let script = work_dir().join("build-engine.cmd");
            fs::write(
                &script,
                format!(
                    "@echo off\r\ncall \"{vcvars}\" >nul\r\ncd /d \"{engine}\"\r\n\
                     cmake --preset win32 -B build/{build_dir} {defs}\r\n\
                     if errorlevel 1 exit /b 1\r\n\
                     cmake --build build/{build_dir} --config Release --target z_generals\r\n",
                    vcvars = vcvars()?.display(),
                    engine = engine.display(),
                    defs = defs.join(" "),
                ),
            )
            .map_err(|e| e.to_string())?;
            run_cmd(Command::new("cmd").arg("/c").arg(&script).current_dir(work_dir()))?;
            engine.join("build").join(build_dir).join("GeneralsMD").join("Release")
        }
        Toolchain::MinGw => {
            let build_dir = if opts.log_build { "mingw-zhbo-log" } else { "mingw-zhbo" };
            let jobs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).to_string();
            run_cmd(
                Command::new("cmake")
                    .current_dir(&engine)
                    .args(["--preset", "mingw-w64-i686", "-B", &format!("build/{build_dir}")])
                    .args(&defs),
            )?;
            run_cmd(
                Command::new("cmake")
                    .current_dir(&engine)
                    .args(["--build", &format!("build/{build_dir}"), "--target", "z_generals", "-j", &jobs]),
            )?;
            engine.join("build").join(build_dir).join("GeneralsMD")
        }
    };

    let dist = dist_dir(opts);
    fs::create_dir_all(&dist).map_err(|e| e.to_string())?;
    fs::copy(exe_dir.join("generalszh.exe"), dist.join("generalszh.exe")).map_err(|e| format!("copy generalszh.exe: {e}"))?;
    if exe_dir.join("generalszh.pdb").is_file() {
        fs::copy(exe_dir.join("generalszh.pdb"), dist.join("generalszh.pdb")).map_err(|e| e.to_string())?;
    }
    let (from, to) = tc.launcher_file();
    fs::copy(launcher_dir.join(from), dist.join(to)).map_err(|e| format!("copy launcher: {e}"))?;
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
    let (_, launcher) = Toolchain::host().launcher_file();
    run_cmd(Command::new(dist.join(launcher)).args(game_args))
}
