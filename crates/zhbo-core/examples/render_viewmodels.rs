//! Headless viewmodel sweep: every player weapon, and every attachment it offers, drawn through the
//! game's own native first-person path into PNG pairs — no game launch.
//!
//! ```text
//! cargo run -p zhbo-core --example render_viewmodels --features renderer -- <outdir>
//! ```
//!
//! Optional flags: `--archive <ConquerOps_BO2.big>`, `--weapon <id>` (repeatable: only these
//! weapons), `--no-attachments` (base weapons only). `--release` converts the combinations about ten
//! times faster; a full sweep is a few minutes.
//!
//! **What it draws.** The pipeline the game draws with, not a lookalike: the core's `nativevm` path
//! loads a viewmodel exactly as `host/zhbo/ZhboViewmodel.cpp` drives it (`zhbo_native_viewmodel`
//! plus the `zhbo_native_wanted`/`zhbo_native_provide` file protocol), composes the parts (the
//! faction's arms, the gun, its magazine and the attachment on the attachment's own view-model tag,
//! the manifest's `hide` tags, the combination's clip set), poses the clip and places the gun with
//! `nativevm::place` (hip: `ads = 0`; aimed: `ads = 1`, which slides the aim marker onto the view
//! axis). `nativevm::gpu` uploads the mesh and textures and submits the draw to a headless
//! `zhbo-gfx` renderer, which runs the real native BO2 pass. The pixels are therefore whatever the
//! game's shading, culling and blending make of the art — a see-through patch, a floating optic or
//! a gun off the aim axis shows up here as it would in a match.
//!
//! **Poses.** One image per (weapon, attachment): the hip pose on the left and the aimed pose on the
//! right, 640x360 each (1280x360). The camera is the game's eye frame — the eye at the origin, +X
//! forward, Z up — at the game's own field of view (`client::horizontal_fov`, which turns BO2's
//! 4:3 `cg_fov` into the horizontal FOV of a 16:9 screen, as the bridge hands it to the engine) and
//! a fixed forward view direction, identical for every render, so a gun that sits off the others is
//! obvious across the set. The gun's offset and angles come from the weapon definition the way
//! `viewmodel::Motion` builds them (`weapons::gunpos::GunPose::step` at stand, still: no bob, no
//! sway), so BO2's own pose numbers decide, never a hand-tuned constant here. No effects and no
//! muzzle flash.
//!
//! **Content.** Base weapons come from the player's converted archive (`ConquerOps_BO2.big`, in the
//! launcher's dist folder; `--archive` or `ZHBO_BO2_ARCHIVE` override): exactly the blobs the game
//! draws today. The archive only holds the base weapons and the loadout's own combination — the
//! converter converts what a class can carry — so a weapon with an attachment mounted solo (any
//! combination create-a-class can make) is converted on demand by
//! `zhbo_assets::viewmodel_sweep::Source`, the same `viewmodels::weapon_viewmodels` code
//! `cargo xtask build` runs, straight out of the player's Black Ops II install. It lives in memory
//! for that render only. Every render uses the USA viewhands (`U`), so the arms are in every image.
//! Nothing is read but the player's own installs; Black Ops II is never launched.
//!
//! **Output.** `<outdir>/<weapon>/base.png` and `<outdir>/<weapon>/att_<attachment>.png`, plus
//! `<outdir>/manifest.tsv` (`weapon`, `attachment`, `image`, `notes`: where the art came from, the
//! viewmodel's part count and log lines, and whether the renderer refused a draw). The background is
//! flat light grey (0.72) so dark gunmetal silhouettes and see-through holes read at a glance.
//!
//! Development tool only, on no shipped path; the rendered images are output, not source.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zhbo_core::nativevm::ffi::{self, ZhboNativeIn, ZhboNativeLight, DRAWN, LOADING};
use zhbo_core::tuning::Tuning;
use zhbo_core::weapons::gunpos::{GunInput, GunPos};
use zhbo_core::weapons::{self, WeaponDef};
use zhbo_core::zhbo_gfx;
use zhbo_formats::big::BigArchive;

/// Each pose is BO2's 16:9 slot; the pair is stitched side by side.
const W: u32 = 640;
const H: u32 = 360;
/// The arms every render uses (the USA viewhands), so the set is comparable.
const FACTION: u8 = b'U';
/// The scene view the native pass draws into, and the view it runs before (the engine's composite,
/// which this harness never creates: the pass flushes at the end of the frame, over the clear).
const VIEW: u16 = 1;
const BEFORE_VIEW: u16 = 2;
/// The time every render holds the idle clip at: its first frame, the authored neutral pose, so two
/// renders of the same weapon differ only in the pose asked for.
const IDLE_TIME: f32 = 0.0;
/// A flat light grey background (`bgfx::setViewClear` takes RRGGBBAA): ~0.72 in 8 bits.
const BACKGROUND: u32 = 0xb8b8b8ff;
/// The near and far planes, in world units (1 unit = 6 inches). The arms start at the eye, so the
/// near plane must be close enough that they do not clip into a hole.
const NEAR: f32 = 0.1;
const FAR: f32 = 100.0;
/// How many load rounds a viewmodel may take before the harness gives up (each round answers every
/// outstanding file request; the real path takes three or four).
const LOAD_ROUNDS: usize = 64;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("render_viewmodels: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1))?;
    let archive_path = resolve_archive(args.archive.as_deref())?;
    let archive = BigArchive::open(&archive_path).map_err(|e| format!("{}: {e}", archive_path.display()))?;
    install_catalog(&archive)?;
    let jobs = plan(&args);
    if jobs.is_empty() {
        return Err("no weapons matched".into());
    }
    println!("render_viewmodels: {} jobs from {}", jobs.len(), archive_path.display());
    std::fs::create_dir_all(&args.out).map_err(|e| format!("{}: {e}", args.out.display()))?;
    let manifest_path = args.out.join("manifest.tsv");
    let mut manifest = std::fs::File::create(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    writeln!(manifest, "weapon\tattachment\timage\tnotes").map_err(|e| e.to_string())?;
    zhbo_core::config::init(zhbo_core::config::Config { native_viewmodel: true, ..Default::default() });
    open_renderer()?;
    let mut source: Option<zhbo_assets::viewmodel_sweep::Source> = None;
    let mut counts = Counts::default();
    for (i, job) in jobs.iter().enumerate() {
        let stamp = format!("[{}/{}]", i + 1, jobs.len());
        match render_job(job, &archive, &mut source, &args.out) {
            Ok(note) => {
                counts.rendered += 1;
                println!("{stamp} {:<14} {:<12} {note}", job.weapon.id, job.attachment.as_deref().unwrap_or("-"));
                writeln!(manifest, "{}\t{}\t{}\t{}", job.weapon.id, job.attachment.as_deref().unwrap_or("-"), job.relative(), one_line(&note)).map_err(|e| e.to_string())?;
            }
            Err(e) => {
                counts.failed += 1;
                println!("{stamp} {:<14} {:<12} FAILED {e}", job.weapon.id, job.attachment.as_deref().unwrap_or("-"));
                writeln!(manifest, "{}\t{}\t-\tfailed: {}", job.weapon.id, job.attachment.as_deref().unwrap_or("-"), one_line(&e)).map_err(|e| e.to_string())?;
            }
        }
        manifest.flush().map_err(|e| e.to_string())?;
    }
    close_renderer();
    println!("render_viewmodels: {} images, {} failed, into {}", counts.rendered, counts.failed, args.out.display());
    if counts.failed > 0 {
        return Err(format!("{} of {} jobs failed (see {})", counts.failed, jobs.len(), manifest_path.display()));
    }
    Ok(())
}

#[derive(Default)]
struct Counts {
    rendered: usize,
    failed: usize,
}

// ---------------------------------------------------------------------------------------------
// Arguments and the job list

struct Args {
    out: PathBuf,
    archive: Option<PathBuf>,
    /// Only these weapon ids (`an94_mp`).
    weapons: Vec<String>,
    attachments: bool,
}

impl Args {
    fn parse(mut argv: impl Iterator<Item = String>) -> Result<Args, String> {
        const USAGE: &str = "usage: render_viewmodels <outdir> [--archive <big>] [--weapon <id>]... [--no-attachments]";
        let mut out = None;
        let (mut archive, mut weapons, mut attachments) = (None, Vec::new(), true);
        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "--archive" => archive = Some(PathBuf::from(argv.next().ok_or("--archive needs a path")?)),
                "--weapon" => weapons.push(argv.next().ok_or("--weapon needs a weapon id")?),
                "--no-attachments" => attachments = false,
                "-h" | "--help" => return Err(USAGE.into()),
                other if other.starts_with('-') => return Err(format!("unknown flag {other}\n{USAGE}")),
                other => {
                    if out.is_some() {
                        return Err(format!("unexpected argument {other:?}\n{USAGE}"));
                    }
                    out = Some(PathBuf::from(other));
                }
            }
        }
        Ok(Args { out: out.ok_or(USAGE)?, archive, weapons, attachments })
    }
}

/// One render: a weapon, or that weapon with one attachment mounted solo.
struct Job {
    weapon: &'static WeaponDef,
    /// The attachment's create-a-class short name (`reflex`), `None` for the bare weapon.
    attachment: Option<String>,
    /// The converted viewmodel id: `an94_mp`, or `an94_mp+reflex`.
    id: String,
}

impl Job {
    /// The image's path inside the output folder (`an94/base.png`).
    fn relative(&self) -> String {
        let file = match &self.attachment {
            None => "base.png".to_string(),
            Some(short) => format!("att_{short}.png"),
        };
        format!("{}/{}", weapon_dir(&self.weapon.id), file)
    }
}

/// The folder a weapon's images go in: its table id without the `_mp` suffix (`an94_mp` -> `an94`).
fn weapon_dir(id: &str) -> String {
    id.strip_suffix("_mp").unwrap_or(id).to_string()
}

/// The player's weapons from the installed table, each followed by its solo attachments.
///
/// Player weapons are the table's base entries: combinations (`an94_mp+gl+reflex`) are built on
/// demand by the catalog, and alt modes (`gl_an94_mp`) are reached through the weapon they toggle
/// from, so a base entry's id carries no `+`/`~` and its `alt` names nothing.
fn plan(args: &Args) -> Vec<Job> {
    let mut jobs = Vec::new();
    for def in weapons::catalog().all() {
        if def.id.contains('+') || def.id.contains('~') || !def.alt.is_empty() {
            continue;
        }
        if !args.weapons.is_empty() && !args.weapons.iter().any(|w| w == &def.id) {
            continue;
        }
        jobs.push(Job { weapon: def, attachment: None, id: def.id.clone() });
        if args.attachments {
            for (short, _asset) in &def.allowed {
                jobs.push(Job { weapon: def, attachment: Some(short.clone()), id: format!("{}+{short}", def.id) });
            }
        }
    }
    jobs
}

// ---------------------------------------------------------------------------------------------
// Content: the converted archive, and the converter for combinations it does not hold

fn resolve_archive(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if path.is_file() { Ok(path.to_path_buf()) } else { Err(format!("{}: no such archive", path.display())) };
    }
    if let Some(path) = std::env::var_os("ZHBO_BO2_ARCHIVE").map(PathBuf::from) {
        return if path.is_file() { Ok(path) } else { Err(format!("ZHBO_BO2_ARCHIVE={}: no such archive", path.display())) };
    }
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join("Library/Application Support/ConquerOps/dist").join(zhbo_core::names::BO2_ARCHIVE_NAME));
        candidates.push(home.join(".local/share/ConquerOps/dist").join(zhbo_core::names::BO2_ARCHIVE_NAME));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(PathBuf::from(appdata).join("ConquerOps").join("dist").join(zhbo_core::names::BO2_ARCHIVE_NAME));
    }
    candidates.push(PathBuf::from("build/dist").join(zhbo_core::names::BO2_ARCHIVE_NAME));
    candidates.push(PathBuf::from("dist").join(zhbo_core::names::BO2_ARCHIVE_NAME));
    for path in &candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    Err(format!(
        "{} not found. Run the mod once through its launcher to convert it, or pass --archive <path>.\nLooked at:\n{}",
        zhbo_core::names::BO2_ARCHIVE_NAME,
        candidates.iter().map(|p| format!("  {}", p.display())).collect::<Vec<_>>().join("\n")
    ))
}

/// Install the player's converted weapon table, so coverage is the table the game plays with.
fn install_catalog(archive: &BigArchive) -> Result<(), String> {
    let path = zhbo_core::names::WEAPON_TABLE_PATH;
    let entry = archive.find(path).ok_or_else(|| format!("the archive has no {path}"))?;
    let bytes = archive.read(entry).map_err(|e| format!("{path}: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("{path}: {e}"))?;
    let n = weapons::install_catalog(&text)?;
    println!("render_viewmodels: {n} weapon table entries");
    Ok(())
}

/// The bytes behind one archive path, for one render.
enum Art<'a> {
    /// The converted blobs in the player's archive (base weapons: what the game draws today).
    Archive(&'a BigArchive),
    /// A combination the converter built in memory (`zhbo_assets::viewmodel_sweep`).
    Converted(&'a BTreeMap<String, Vec<u8>>),
}

impl Art<'_> {
    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Art::Archive(big) => big.find(path).and_then(|e| big.read(e).ok()),
            Art::Converted(map) => map.get(path).cloned(),
        }
    }

    fn has(&self, path: &str) -> bool {
        match self {
            Art::Archive(big) => big.find(path).is_some(),
            Art::Converted(map) => map.contains_key(path),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Rendering

/// The headless renderer's screenshot callback target: the last frame's BGRA pixels.
type Shot = Arc<Mutex<Option<Vec<u8>>>>;

/// The screenshot sink `open_renderer` installed (the renderer's callback runs on this thread).
static SHOT: Mutex<Option<Shot>> = Mutex::new(None);

fn shot() -> Option<Shot> {
    SHOT.lock().unwrap().clone()
}

fn open_renderer() -> Result<(), String> {
    use zhbo_gfx::ffi::{with_renderer, zhgfx_init, ZhgfxInit};
    let init = ZhgfxInit { kind: 0, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: W, height: H, reset: 0, transient_vb_size: 0, transient_ib_size: 0 };
    if !unsafe { zhgfx_init(&init) } {
        return Err("no usable wgpu adapter (see zhbo-gfx.log); ZHBO_GFX_FALLBACK=1 tries the software one".into());
    }
    let sink: Shot = Arc::default();
    let target = sink.clone();
    with_renderer((), |r| {
        r.set_screenshot_fn(Box::new(move |_, _, _, _, data| *target.lock().unwrap() = Some(data.to_vec())));
    });
    *SHOT.lock().unwrap() = Some(sink);
    // The camera: the game's eye frame at the origin looking down +X with Z up — the frame
    // `nativevm::place::root` puts the model in — at the game's own field of view. `horizontal_fov`
    // takes BO2's 4:3 `cg_fov` in degrees and returns the horizontal FOV in radians (Hor+), the
    // value the bridge hands the engine. The near plane is close enough that the arms start at the
    // eye without clipping into a hole.
    let aspect = W as f32 / H as f32;
    let t = 1.0 / (zhbo_core::client::horizontal_fov(Tuning::default().cg_fov, aspect) / 2.0).tan();
    let mut view = [0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    if let Ok(v) = std::env::var("ZHBO_SWEEP_BACK") {
        let back: f32 = v.parse::<f32>().unwrap_or(0.0) / 6.0;
        view[14] += back;
    }
    let proj = [t, 0.0, 0.0, 0.0, 0.0, t * aspect, 0.0, 0.0, 0.0, 0.0, FAR / (FAR - NEAR), 1.0, 0.0, 0.0, -NEAR * FAR / (FAR - NEAR), 0.0];
    let adapter = with_renderer(String::new(), |r| {
        r.set_view_rect(VIEW, 0, 0, W as u16, H as u16);
        r.set_view_clear(VIEW, zhbo_gfx::state::CLEAR_COLOR, BACKGROUND, 1.0, 0);
        r.set_view_transform(VIEW, Some(&view), Some(&proj));
        r.adapter_name()
    });
    println!("render_viewmodels: {adapter}");
    Ok(())
}

fn close_renderer() {
    // The core's native state holds GPU handles; drop them before the device goes away.
    ffi::zhbo_native_reset();
    *SHOT.lock().unwrap() = None;
    zhbo_gfx::ffi::zhgfx_shutdown();
}

/// One pose, ready to draw.
#[derive(Clone, Copy)]
struct Pose {
    ads: f32,
    offset: [f32; 3],
    angles: [f32; 3],
}

/// The gun's transform for `ads`, as `viewmodel::Motion` builds it from the weapon definition: the
/// offset in art units (forward, left, up) and the angles in radians (yaw, pitch, roll). Stand,
/// still: no bob, no sway, no landing dip, so the pose is the definition's own numbers.
fn pose(def: &WeaponDef, ads: f32) -> Pose {
    let tuning = Tuning::default();
    let input = GunInput { stance: 0, ads, fov: tuning.cg_fov, since_land: f32::MAX, ..Default::default() };
    let mut state = GunPos::default();
    let mut step = zhbo_core::weapons::gunpos::GunPose::default();
    for _ in 0..8 {
        step = state.step(def, &input, &tuning, 1.0 / 60.0);
    }
    let inch = zhbo_native::ART_SCALE;
    Pose {
        ads,
        offset: [step.offset[0] * inch, -step.offset[1] * inch, step.offset[2] * inch],
        angles: [step.angles[1].to_radians(), step.angles[0].to_radians(), step.angles[2].to_radians()],
    }
}

/// Draw one pose and return its RGB pixels plus the viewmodel's log lines.
///
/// The native state is reset first, so each render composes its own viewmodel and the previous
/// one's GPU resources go: the harness walks the whole catalog in one process.
fn render_pose(art: &Art, vm: i32, p: Pose) -> Result<(Rgb, Vec<String>), String> {
    ffi::zhbo_native_reset();
    let inp = ZhboNativeIn {
        eye: [0.0; 3],
        dir: [1.0, 0.0, 0.0],
        offset: p.offset,
        angles: p.angles,
        clip: zhbo_core::viewmodel::slot::IDLE,
        vm,
        faction: FACTION as i32,
        time: IDLE_TIME,
        ads: p.ads,
        rate: 1.0,
        ambient: [0.22, 0.2, 0.17],
        lights: [
            ZhboNativeLight { dir: [0.81, -0.38, 0.45], color: [1.0, 1.0, 1.0] },
            ZhboNativeLight { dir: [-0.79, -0.62, 0.0], color: [0.24, 0.24, 0.31] },
            ZhboNativeLight::default(),
        ],
        light_count: 2,
        target_view: VIEW,
        before_view: BEFORE_VIEW,
        debug: 0.0,
    };
    let mut logs = Vec::new();
    // Load: each round answers every outstanding file request; the first frame that draws the
    // viewmodel also queues its draw. The clip arrives a round after the models, so the first drawn
    // frame can still hold the bind pose — draw that one and discard it, then draw the picture.
    for round in 0..2 {
        let mut answer = LOADING;
        for _ in 0..LOAD_ROUNDS {
            answer = unsafe { ffi::zhbo_native_viewmodel(&inp) };
            if answer != LOADING {
                break;
            }
            serve(art, &mut logs)?;
        }
        take_logs(&mut logs);
        if answer != DRAWN {
            let why = logs.join("; ");
            return Err(if why.is_empty() { "the native viewmodel path refused the viewmodel".into() } else { why });
        }
        let (px, refused) = frame()?;
        if refused {
            return Err("the renderer refused a native draw (see zhbo-gfx.log)".into());
        }
        if round == 1 {
            logs.dedup();
            return Ok((Rgb::from_bgra(&px, W, H), logs));
        }
    }
    unreachable!("the second round returns")
}

/// Draw the queued native pass and read the backbuffer back. Returns the pixels and whether the
/// renderer refused a draw.
fn frame() -> Result<(Vec<u8>, bool), String> {
    let sink = shot().ok_or("the renderer is not open")?;
    sink.lock().unwrap().take();
    let stats = zhbo_gfx::ffi::with_renderer(None, |r| {
        r.touch(VIEW);
        r.request_screenshot(zhbo_gfx::runtime::INVALID, "");
        r.frame();
        let s = r.stats();
        Some((s.native_skipped, s.errors))
    })
    .ok_or("the renderer is not open")?;
    let px = sink.lock().unwrap().take().ok_or("the renderer produced no screenshot")?;
    Ok((px, stats != (0, 0)))
}

/// Answer every outstanding file request from `art` (`None` when it does not hold the file).
fn serve(art: &Art, logs: &mut Vec<String>) -> Result<(), String> {
    let mut buf = vec![0u8; 1024];
    for _ in 0..4096 {
        let n = unsafe { ffi::zhbo_native_wanted(0, buf.as_mut_ptr().cast(), buf.len() as i32) };
        if n < 0 {
            return Ok(());
        }
        let path = String::from_utf8_lossy(&buf[..n as usize]).into_owned();
        match art.bytes(&path) {
            Some(bytes) => unsafe { ffi::zhbo_native_provide(buf.as_ptr().cast(), bytes.as_ptr(), bytes.len() as i32) },
            None => {
                logs.push(format!("{path} is not in the archive"));
                unsafe { ffi::zhbo_native_provide(buf.as_ptr().cast(), std::ptr::null(), -1) };
            }
        }
    }
    Err("the viewmodel asked for more files than the bridge answers in one round".into())
}

fn take_logs(into: &mut Vec<String>) {
    let mut buf = vec![0u8; 512];
    for _ in 0..256 {
        let n = unsafe { ffi::zhbo_native_take_log(buf.as_mut_ptr().cast(), buf.len() as i32) };
        if n < 0 {
            return;
        }
        into.push(String::from_utf8_lossy(&buf[..n as usize]).into_owned());
    }
}

/// An RGB image (top row first).
struct Rgb {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Rgb {
    fn from_bgra(px: &[u8], width: u32, height: u32) -> Rgb {
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        for p in px.as_chunks::<4>().0 {
            data.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        Rgb { width, height, data }
    }

    /// This image with `right` beside it.
    fn beside(&self, right: &Rgb) -> Rgb {
        let width = self.width + right.width;
        let height = self.height.max(right.height);
        let mut data = vec![0u8; (width * height * 3) as usize];
        for (x, img) in [(0u32, self), (self.width, right)] {
            for y in 0..img.height {
                let src = (y * img.width * 3) as usize;
                let dst = ((y * width + x) * 3) as usize;
                data[dst..dst + (img.width * 3) as usize].copy_from_slice(&img.data[src..src + (img.width * 3) as usize]);
            }
        }
        Rgb { width, height, data }
    }

    fn write_png(&self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| format!("{}: {e}", path.display()))?;
        writer.write_image_data(&self.data).map_err(|e| format!("{}: {e}", path.display()))
    }
}

// ---------------------------------------------------------------------------------------------
// One job

/// Render `job`: hip and ADS, stitched, plus the note for the manifest.
fn render_job(job: &Job, archive: &BigArchive, source: &mut Option<zhbo_assets::viewmodel_sweep::Source>, out: &Path) -> Result<String, String> {
    // Base weapons draw the archive's own blobs. A combination does too only when the table holds
    // that exact combination (the loadout's own kit, `weapons::converted`): everything else is
    // converted here, into memory for this render only.
    let held = archive_vm(job).filter(|&vm| Art::Archive(archive).has(&zhbo_native::viewmodel_path(vm, FACTION as char)));
    let converted = match held {
        Some(_) => None,
        None => {
            let dir = bo2_dir()?;
            if source.is_none() {
                println!("render_viewmodels: opening the install at {}", dir.display());
                *source = Some(zhbo_assets::viewmodel_sweep::Source::open(&dir)?);
            }
            Some(source.as_ref().unwrap().composition(&job.id)?)
        }
    };
    let (art, vm, from) = match &converted {
        Some(map) => (Art::Converted(map), 0, "converted"),
        None => (Art::Archive(archive), held.unwrap_or(job.weapon.viewmodel), "archive"),
    };
    let count = part_count(&art, vm);
    let base = window_base_parts(archive, job);
    let (hip, mut logs) = render_pose(&art, vm, pose(job.weapon, 0.0))?;
    let (ads, more) = render_pose(&art, vm, pose(job.weapon, 1.0))?;
    logs.extend(more);
    let path = out.join(job.relative());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    hip.beside(&ads).write_png(&path)?;
    let mut note = format!("{from}, {count} parts");
    if let Some(short) = &job.attachment {
        note.push_str(&format!(", attachment `{short}`"));
        if Some(count) == base {
            note.push_str(" (adds no view-model part: no attachment art)");
        }
    }
    logs.retain(|l| !l.is_empty());
    if !logs.is_empty() {
        note.push_str(&format!("; {}", logs.join("; ")));
    }
    Ok(note)
}

/// The converted viewmodel index the archive holds for `job`, when the table itself names it: the
/// base weapon always, a combination only if it is one the converter built (`an94_mp+gl+reflex`).
fn archive_vm(job: &Job) -> Option<i32> {
    if job.attachment.is_none() {
        return Some(job.weapon.viewmodel);
    }
    weapons::catalog().all().iter().find(|d| d.id == job.id).map(|d| d.viewmodel)
}

/// The base weapon's manifest part count, from the archive (the combination's is compared to it).
fn window_base_parts(archive: &BigArchive, job: &Job) -> Option<usize> {
    job.attachment.as_ref()?;
    Some(part_count(&Art::Archive(archive), job.weapon.viewmodel))
}

fn part_count(art: &Art, vm: i32) -> usize {
    art.bytes(&zhbo_native::viewmodel_path(vm, FACTION as char))
        .and_then(|b| zhbo_native::viewmodel::Viewmodel::from_bytes(&b).ok())
        .map_or(0, |v| v.parts.len())
}

/// The Black Ops II install the converter reads: `ZHBO_BO2_DIR`, else the requirements' detection.
fn bo2_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("ZHBO_BO2_DIR").map(PathBuf::from).filter(|p| p.is_dir()) {
        return Ok(dir);
    }
    let req = zhbo_requirements::check_in(&zhbo_requirements::environment());
    req.black_ops2.map(|b| b.path).ok().filter(|p| p.is_dir()).ok_or_else(|| {
        "the attachment's art is not in the archive and the Black Ops II install was not found: set ZHBO_BO2_DIR, or run the mod's launcher once to convert the combinations you want".to_string()
    })
}

fn one_line(s: &str) -> String {
    s.replace(['\t', '\n'], " ")
}
