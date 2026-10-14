//! `cargo xtask <command>` - builds Conquer Ops.
//!
//! ```text
//! cargo xtask build [--log] [--wgpu] [--dist DIR]
//!                                          Rust core + patched Zero Hour + ConquerOps.big + launcher;
//!                                          --wgpu: 64-bit SDL3 build on the Rust wgpu renderer
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

/// bobtista's fork of TheSuperHackers/GeneralsGameCode: adds the pluggable render backend
/// (IRenderBackend), SDL3, OpenAL and 64-bit builds that the wgpu renderer plugs into.
const ENGINE_URL: &str = "https://github.com/bobtista/GeneralsGameCode.git";

struct Opts {
    log_build: bool,
    /// Build the 64-bit SDL3 engine with the Rust wgpu renderer (GGC_BGFX_RENDERER=wgpu).
    wgpu: bool,
    dist: Option<PathBuf>,
    game_args: Vec<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        eprintln!("usage: cargo xtask <build|pack|run|patch|doctor> [--log] [--wgpu] [--dist DIR] [-- GAME_ARGS]");
        return ExitCode::FAILURE;
    };
    let mut opts = Opts { log_build: false, wgpu: false, dist: None, game_args: Vec::new() };
    let mut it = args.into_iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--log" => opts.log_build = true,
            "--wgpu" => opts.wgpu = true,
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
        "doctor" => doctor(&opts),
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
    // The submodule's current checkout wins, so a bumped-but-uncommitted pin builds as expected.
    let sub = repo_root().join("host/GeneralsGameCode");
    if sub.join(".git").exists() {
        if let Ok(head) = capture(git(&sub).args(["rev-parse", "HEAD"])) {
            if head.len() == 40 {
                return Ok(head);
            }
        }
    }
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

/// The directory holding Git for Windows' `sh.exe` (`<git>\usr\bin`), if it can be found.
fn git_sh_dir() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from(r"C:\Program Files\Git\usr\bin")];
    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            if dir.join("git.exe").is_file() {
                // <git>\cmd\git.exe or <git>\bin\git.exe
                if let Some(root) = dir.parent() {
                    candidates.push(root.join("usr").join("bin"));
                }
            }
        }
    }
    candidates.into_iter().find(|d| d.join("sh.exe").is_file())
}

/// The Visual Studio environment script for 32-bit (`x64 == false`) or 64-bit x86 targets.
fn vcvars(x64: bool) -> Result<PathBuf> {
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
    let candidates: &[&str] = match (arch.eq_ignore_ascii_case("ARM64"), x64) {
        (true, false) => &["vcvarsarm64_x86.bat", "vcvars32.bat"],
        (false, false) => &["vcvarsamd64_x86.bat", "vcvars32.bat"],
        (true, true) => &["vcvarsarm64_amd64.bat", "vcvars64.bat"],
        (false, true) => &["vcvars64.bat"],
    };
    candidates
        .iter()
        .map(|c| build.join(c))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("no {} vcvars script in {}", if x64 { "x64" } else { "x86" }, build.display()))
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
    fn core_target(self, wgpu: bool) -> &'static str {
        match (self, wgpu) {
            (Toolchain::Msvc, true) => "x86_64-pc-windows-msvc",
            (Toolchain::Msvc, false) => "i686-pc-windows-msvc",
            (Toolchain::MinGw, _) => "i686-pc-windows-gnu",
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
    fn problems(self, wgpu: bool) -> Vec<String> {
        let mut out = Vec::new();
        if !on_path("git") {
            out.push("git is not on PATH".into());
        }
        let installed = capture(Command::new("rustup").args(["target", "list", "--installed"])).unwrap_or_default();
        if !installed.lines().any(|l| l.trim() == self.core_target(wgpu)) {
            out.push(format!("Rust target missing: rustup target add {}", self.core_target(wgpu)));
        }
        if wgpu && self != Toolchain::Msvc {
            out.push("--wgpu builds are Windows-only until the native macOS/Linux builds land".into());
        }
        match self {
            Toolchain::Msvc => {
                if let Err(e) = vcvars(wgpu) {
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
                if !on_path("i686-w64-mingw32-widl") && !on_path("widl") {
                    out.push(
                        "widl (IDL compiler, needed by the engine) is missing (Debian/Ubuntu: apt install \
                         mingw-w64-tools; Fedora: dnf install mingw32-widl; macOS: included in brew's mingw-w64)"
                            .into(),
                    );
                }
            }
        }
        out
    }
}

// ------------------------------------------------------------------------------------------------
// Commands
// ------------------------------------------------------------------------------------------------

fn doctor(opts: &Opts) -> Result<()> {
    let tc = Toolchain::host();
    eprintln!("toolchain: {tc:?} (core target {})", tc.core_target(opts.wgpu));
    eprintln!("work dir:  {}", work_dir().display());
    let problems = tc.problems(opts.wgpu);
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
    let problems = tc.problems(opts.wgpu);
    if !problems.is_empty() {
        return Err(format!("missing build prerequisites:\n  {}", problems.join("\n  ")));
    }
    let core_target = tc.core_target(opts.wgpu);
    let mut core = cargo();
    core.current_dir(&root).args(["build", "--release", "-p", "zhbo-core", "--target", core_target]);
    if opts.wgpu {
        core.args(["--features", "renderer"]);
    }
    run_cmd(&mut core)?;
    // zhbo-shaderc runs on the build machine, translating the engine's shaders to WGSL.
    let shaderc = rust_target_dir().join("release").join(if cfg!(windows) { "zhbo-shaderc.exe" } else { "zhbo-shaderc" });
    if opts.wgpu {
        run_cmd(cargo().current_dir(&root).args(["build", "--release", "-p", "zhbo-gfx", "--bin", "zhbo-shaderc"]))?;
    }
    let mut launcher = cargo();
    launcher.current_dir(&root).args(["build", "--release", "-p", "zhbo-launcher"]);
    if let Some(t) = tc.launcher_target() {
        launcher.args(["--target", t]);
    }
    run_cmd(&mut launcher)?;
    let core_lib = rust_target_dir().join(core_target).join("release").join(tc.core_lib_name());
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
    if opts.wgpu {
        defs.push("-DGGC_BGFX_RENDERER=wgpu".into());
        defs.push(format!("-DZHBO_SHADERC={}", fwd(&shaderc)));
    }

    let exe_dir = match tc {
        Toolchain::Msvc => {
            let (preset, build_dir) = match (opts.wgpu, opts.log_build) {
                (true, true) => ("win64", "win64-zhbo-log"),
                (true, false) => ("win64", "win64-zhbo"),
                (false, true) => ("win32", "win32-zhbo-log"),
                (false, false) => ("win32", "win32-zhbo"),
            };
            let script = work_dir().join("build-engine.cmd");
            // bgfx.cmake's FetchContent patch step runs `sh`; Git for Windows ships one.
            let sh_path = git_sh_dir()
                .map(|d| format!("set \"PATH=%PATH%;{}\"\r\n", d.display()))
                .unwrap_or_default();
            if opts.wgpu {
                // The SDL3/OpenAL audio device decodes with FFmpeg; the win64 preset points
                // FFMPEG_ROOT at <engine>/.deps-cache/ffmpeg-x64.
                ensure_ffmpeg_win64(&engine)?;
            }
            fs::write(
                &script,
                format!(
                    "@echo off\r\ncall \"{vcvars}\" >nul\r\n{sh_path}cd /d \"{engine}\"\r\n\
                     cmake --preset {preset} -B build/{build_dir} {defs}\r\n\
                     if errorlevel 1 exit /b 1\r\n\
                     cmake --build build/{build_dir} --config Release --target z_generals\r\n",
                    vcvars = vcvars(opts.wgpu)?.display(),
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
    if opts.wgpu {
        // Runtime DLLs: SDL3 and OpenAL from the engine's fetched dependencies (Release config),
        // FFmpeg from the prebuilt package.
        let deps = exe_dir.parent().and_then(Path::parent).map(|b| b.join("_deps")).unwrap_or_default();
        let mut dlls = Vec::new();
        find_files(&deps, &mut |p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
            let release = p.components().any(|c| c.as_os_str().eq_ignore_ascii_case("Release"));
            release && name.ends_with(".dll") && (name.starts_with("sdl3") || name.starts_with("openal32"))
        }, &mut dlls);
        if dlls.is_empty() {
            return Err(format!("no SDL3/OpenAL runtime DLLs under {}", deps.display()));
        }
        find_files(&ensure_ffmpeg_win64(&engine)?.join("bin"), &mut |p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")), &mut dlls);
        for dll in dlls {
            fs::copy(&dll, dist.join(dll.file_name().unwrap())).map_err(|e| format!("copy {}: {e}", dll.display()))?;
        }
    }
    let (from, to) = tc.launcher_file();
    fs::copy(launcher_dir.join(from), dist.join(to)).map_err(|e| format!("copy launcher: {e}"))?;
    pack(&dist)?;
    eprintln!("Conquer Ops staged in {}", dist.display());
    Ok(dist)
}

fn find_files(dir: &Path, keep: &mut dyn FnMut(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            find_files(&p, keep, out);
        } else if keep(&p) {
            out.push(p);
        }
    }
}

/// BtbN's FFmpeg 8.1 win64 shared build (headers, import libs, DLLs), as the engine fork's CI uses.
/// Downloaded once into the work dir. The asset name is a rolling alias for the 8.1 branch.
const FFMPEG_WIN64_URL: &str =
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-win64-gpl-shared-8.1.zip";

fn ensure_ffmpeg_win64(engine: &Path) -> Result<PathBuf> {
    // Where the fork's win64 preset points FFMPEG_ROOT (untracked, so the engine reset keeps it).
    let deps = engine.join(".deps-cache");
    let root = deps.join("ffmpeg-x64");
    if root.join("include").join("libavcodec").is_dir() {
        return Ok(root);
    }
    fs::create_dir_all(&deps).map_err(|e| e.to_string())?;
    let zip = deps.join("ffmpeg.zip");
    // BtbN re-uploads the alias assets on every autobuild, so a download can briefly 404.
    let mut ok = false;
    for attempt in 1..=5 {
        if run_cmd(Command::new("curl").args(["-fL", "--retry", "3", "-o"]).arg(&zip).arg(FFMPEG_WIN64_URL)).is_ok() {
            ok = true;
            break;
        }
        eprintln!("FFmpeg download attempt {attempt}/5 failed");
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
    if !ok {
        return Err(format!("could not download {FFMPEG_WIN64_URL}"));
    }
    let tmp = deps.join("ffmpeg-tmp");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    run_cmd(Command::new("tar").arg("-xf").arg(&zip).arg("-C").arg(&tmp))?;
    let inner = fs::read_dir(&tmp)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .ok_or("the FFmpeg archive has no top-level folder")?;
    fs::rename(&inner, &root).map_err(|e| format!("move FFmpeg into place: {e}"))?;
    let _ = fs::remove_dir_all(&tmp);
    let _ = fs::remove_file(&zip);
    Ok(root)
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
