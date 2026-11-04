//! Headless third-person sweep: every converted weapon's world model (`ZW<vm>.zbw`, the weapon the
//! overhead General carries) drawn through the game's own native third-person path into PNGs — no
//! game launch.
//!
//! ```text
//! cargo run --release -p zhbo-core --example render_world_weapons --features renderer -- <outdir> --archive <ConquerOps_BO2.big>
//! ```
//!
//! Optional flags: `--weapon <id>` (repeatable: only these weapons; default every base weapon of
//! the archive's table, `+`-free ids), `--all` (every converted weapon, combinations included).
//! `--torso <id>`: instead, the whole General in body mode (`ZHBO_NATIVE_BODY=2`) holding weapon
//! `<id>`, once standing and once mid-way through each torso action the archive's table gives it
//! (`zhbo_core::torso`: reload, knife, shot, raise, and the frag's throw), into `<outdir>/torso_<id>.png`.
//!
//! **What it draws.** The pipeline the game draws the overhead weapon with: `zhbo_native_body` in
//! weapon mode (`ZHBO_NATIVE_BODY=1`) with the engine's `tag_weapon_right` transform
//! at the origin, fed over the same file protocol the bridge serves (`zhbo_native_body_wanted` /
//! `_provide`). The core composes the manifest's parts (the world model, its base parts and the
//! attachments on their tags) and `nativebody::gpu` uploads and submits it to a headless `zhbo-gfx`
//! renderer running the real native BO2 pass, lit by one key light and an ambient term. The camera
//! looks down on the weapon from above and behind, as the RTS camera does, framed to the weapon's
//! bounds. A surface whose colour map is missing draws the renderer's white stand-in, so an
//! untextured weapon is obvious here as it is in a match. No class camo (that is the sim's).
//!
//! **Output.** `<outdir>/<weapon>.png` (480x320), `<outdir>/sheet.png` (all of them, six across)
//! and `<outdir>/manifest.tsv` (`weapon`, `vm`, `image`, `notes`: the core's log lines, the files
//! the archive lacked, and whether the renderer refused a draw).
//!
//! Development tool only, on no shipped path; the rendered images are output, not source.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zhbo_core::nativebody::ffi::{self, ZhboNativeBodyIn, DRAWN, LOADING};
use zhbo_core::nativevm::ffi::ZhboNativeLight;
use zhbo_core::zhbo_gfx;
use zhbo_formats::big::BigArchive;

const W: u32 = 480;
const H: u32 = 320;
const VIEW: u16 = 1;
const BEFORE_VIEW: u16 = 2;
const BACKGROUND: u32 = 0x5a6650ff;
const FACTION: u8 = b'U';
const ROUNDS: usize = 64;
/// The vertical field of view of the review camera (radians).
const FOV: f32 = 0.6;

type Shot = Arc<Mutex<Option<Vec<u8>>>>;

struct Archive {
    big: BigArchive,
    index: HashMap<String, usize>,
}

impl Archive {
    fn open(path: &Path) -> Result<Archive, String> {
        let big = BigArchive::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let index = big.entries().iter().enumerate().map(|(i, e)| (zhbo_formats::big::normalize(&e.path), i)).collect();
        Ok(Archive { big, index })
    }
    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        let i = *self.index.get(&zhbo_formats::big::normalize(path))?;
        self.big.read(&self.big.entries()[i]).ok()
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("render_world_weapons: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(args.first().ok_or("usage: render_world_weapons <outdir> --archive <big> [--weapon id]... [--all]")?);
    let mut archive_path = None;
    let mut only = Vec::new();
    let mut all = false;
    let mut torso: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--archive" => {
                archive_path = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--weapon" => {
                only.extend(args.get(i + 1).cloned());
                i += 1;
            }
            "--all" => all = true,
            "--torso" => {
                torso = args.get(i + 1).cloned();
                i += 1;
            }
            a => return Err(format!("unknown argument {a}")),
        }
        i += 1;
    }
    let archive = Archive::open(&archive_path.ok_or("--archive <ConquerOps_BO2.big> is required")?)?;
    let table_text = String::from_utf8(archive.bytes(zhbo_core::names::WEAPON_TABLE_PATH).ok_or("the archive has no weapon table")?).map_err(|e| e.to_string())?;
    let table = zhbo_core::weapons::parse_full_table(&table_text)?;
    let mut jobs: Vec<(String, i32)> = Vec::new();
    for d in table.weapons.iter().filter(|d| d.viewmodel >= 0) {
        let wanted = if only.is_empty() { all || !d.id.contains(['+', '~']) } else { only.contains(&d.id) };
        if wanted && !jobs.iter().any(|(_, vm)| *vm == d.viewmodel) {
            jobs.push((d.id.clone(), d.viewmodel));
        }
    }
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    if let Some(id) = torso {
        return render_torso(&archive, &table, &id, &out);
    }
    // The game's default third person: the native weapon in the W3D body's hand.
    zhbo_core::config::init(zhbo_core::config::Config { native_body: 1, ..Default::default() });
    let shot = open_renderer()?;
    let mut manifest = String::from("weapon\tvm\timage\tnotes\n");
    let mut tiles = Vec::new();
    for (n, (id, vm)) in jobs.iter().enumerate() {
        let (px, notes) = render(&archive, &shot, *vm, n as u32 + 1)?;
        let image = format!("{}.png", id.replace(['+', '~', '/', '\\'], "_"));
        let rgb = to_rgb(&px);
        write_png(&out.join(&image), W, H, &rgb)?;
        manifest.push_str(&format!("{id}\t{vm}\t{image}\t{}\n", notes.join(" | ").replace(['\t', '\n'], " ")));
        tiles.push(rgb);
        println!("{id}: {image}{}", if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) });
    }
    write_sheet(&out.join("sheet.png"), &tiles)?;
    std::fs::File::create(out.join("manifest.tsv")).and_then(|mut f| f.write_all(manifest.as_bytes())).map_err(|e| e.to_string())?;
    ffi::zhbo_native_body_reset();
    zhbo_gfx::ffi::zhgfx_shutdown();
    Ok(())
}

fn open_renderer() -> Result<Shot, String> {
    use zhbo_gfx::ffi::{with_renderer, zhgfx_init, ZhgfxInit};
    let init = ZhgfxInit { kind: 0, window: std::ptr::null_mut(), display: std::ptr::null_mut(), width: W, height: H, reset: 0, transient_vb_size: 0, transient_ib_size: 0 };
    if !unsafe { zhgfx_init(&init) } {
        return Err("no usable wgpu adapter (see zhbo-gfx.log); ZHBO_GFX_FALLBACK=1 tries the software one".into());
    }
    let sink: Shot = Arc::default();
    let target = sink.clone();
    with_renderer((), |r| {
        r.set_screenshot_fn(Box::new(move |_, _, _, _, data| *target.lock().unwrap() = Some(data.to_vec())));
        r.set_view_rect(VIEW, 0, 0, W as u16, H as u16);
        r.set_view_clear(VIEW, zhbo_gfx::state::CLEAR_COLOR, BACKGROUND, 1.0, 0);
    });
    Ok(sink)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-6);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// A look-at camera at `eye` on `target`, Z up (bx row-vector order: view x right, y up, z ahead).
fn look_at(eye: [f32; 3], target: [f32; 3]) -> [f32; 16] {
    let f = norm(sub(target, eye));
    let r = norm(cross(f, [0.0, 0.0, 1.0]));
    let u = cross(r, f);
    [r[0], u[0], f[0], 0.0, r[1], u[1], f[1], 0.0, r[2], u[2], f[2], 0.0, -dot(eye, r), -dot(eye, u), -dot(eye, f), 1.0]
}

/// One weapon: load it over the file protocol, frame it from above and behind, draw, read back.
fn render(archive: &Archive, shot: &Shot, vm: i32, general: u32) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut notes = Vec::new();
    // The weapon's centre and size from its world model's bounds: the first part of the manifest.
    let bounds = archive
        .bytes(&zhbo_native::world_weapon_path(vm))
        .and_then(|b| zhbo_native::thirdperson::Weapon::from_bytes(&b).ok())
        .and_then(|w| w.parts.first().and_then(|p| archive.bytes(&p.model)))
        .and_then(|b| zhbo_native::model::Model::from_bytes(&b).ok())
        .map(|m| (m.mins, m.maxs));
    let k = zhbo_core::nativevm::ART_SCALE;
    let (centre, radius) = bounds.map_or(([0.0; 3], 30.0 * k), |(lo, hi)| {
        let c = [(lo[0] + hi[0]) * 0.5 * k, (lo[1] + hi[1]) * 0.5 * k, (lo[2] + hi[2]) * 0.5 * k];
        (c, (0..3).map(|a| (hi[a] - lo[a]).abs()).fold(0.0f32, f32::max) * 0.5 * k)
    });
    let dist = radius.max(0.5) / (FOV / 2.0).tan() * 1.4;
    // Above and behind the muzzle (+X), as the RTS camera looks down on a General.
    let eye = [centre[0] - dist * 0.55, centre[1] - dist * 0.45, centre[2] + dist * 0.70];
    let view = look_at(eye, centre);
    let aspect = W as f32 / H as f32;
    let t = 1.0 / (FOV / 2.0).tan();
    let (near, far) = (0.05f32, dist * 4.0);
    let proj = [t / aspect, 0.0, 0.0, 0.0, 0.0, t, 0.0, 0.0, 0.0, 0.0, far / (far - near), 1.0, 0.0, 0.0, -near * far / (far - near), 0.0];
    zhbo_gfx::ffi::with_renderer((), |r| r.set_view_transform(VIEW, Some(&view), Some(&proj)));
    let light = ZhboNativeLight { dir: norm([0.4, 0.3, -1.0]), color: [1.0, 0.97, 0.9] };
    let inp = ZhboNativeBodyIn {
        general,
        mode: 1,
        vm,
        faction: FACTION as i32,
        state: 0,
        time: 0.0,
        rate: 1.0,
        pos: [0.0; 3],
        yaw: 0.0,
        eye,
        tag: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        tag_valid: 1,
        ambient: [0.55, 0.55, 0.58],
        lights: [light, ZhboNativeLight::default(), ZhboNativeLight::default()],
        light_count: 1,
        target_view: VIEW,
        before_view: BEFORE_VIEW,
        debug: 0.0,
    };
    let mut answer = LOADING;
    for _ in 0..ROUNDS {
        answer = unsafe { ffi::zhbo_native_body(&inp) };
        serve(archive, &mut notes);
        if answer != LOADING {
            break;
        }
    }
    take_logs(&mut notes);
    if answer != DRAWN {
        notes.push(format!("not drawn (answer {answer})"));
    }
    shot.lock().unwrap().take();
    let refused = zhbo_gfx::ffi::with_renderer(false, |r| {
        r.touch(VIEW);
        r.request_screenshot(zhbo_gfx::runtime::INVALID, "");
        r.frame();
        let s = r.stats();
        s.native_skipped != 0 || s.errors != 0
    });
    if refused {
        notes.push("the renderer refused a draw".into());
    }
    let px = shot.lock().unwrap().take().ok_or("the renderer produced no screenshot")?;
    // A General per weapon keeps his own rig; drop it before the next one.
    ffi::zhbo_native_body_reset();
    Ok((px, notes))
}

/// The General in body mode with weapon `id`: standing, then mid-way through each torso action.
fn render_torso(archive: &Archive, table: &zhbo_core::weapons::Table, id: &str, out: &Path) -> Result<(), String> {
    use zhbo_core::torso;
    let layers = String::from_utf8(archive.bytes(zhbo_core::names::SOUND_LAYERS_PATH).ok_or("no sound_layers.txt")?).map_err(|e| e.to_string())?;
    torso::install(&layers);
    let def = table.weapons.iter().find(|d| d.id == id).ok_or_else(|| format!("no weapon {id}"))?;
    let frag = table.weapons.iter().find(|d| d.id == "frag_grenade_mp");
    zhbo_core::config::init(zhbo_core::config::Config { native_body: 2, ..Default::default() });
    let shot = open_renderer()?;
    let stand = torso::context(0, 0, 0, false);
    let mut poses: Vec<(String, Option<(u8, u16)>)> = vec![("idle".into(), None)];
    for (name, d, action) in [("reload", Some(def), torso::RELOAD), ("knife", Some(def), torso::MELEE), ("fire", Some(def), torso::FIRE), ("raise", Some(def), torso::RAISE), ("throw", frag, torso::THROW), ("flinch_front", Some(def), torso::FLINCH), ("flinch_back", Some(def), torso::FLINCH_BACK), ("flinch_left", Some(def), torso::FLINCH_LEFT), ("flinch_right", Some(def), torso::FLINCH_RIGHT)] {
        if let Some(c) = d.and_then(|d| torso::pick_for(d, action, stand)) {
            poses.push((name.into(), Some((action, c))));
        }
    }
    // And dead: the corpse BO2's `DEATH` gives each kind of death (`zhbo_core::death`), held.
    use zhbo_core::death::DmgType as D;
    for (name, stance, ms, dir, dmg, from) in [
        ("death", 0, 0, 0, D::Normal, 2),
        ("death_headshot_front", 0, 0, 0, D::HeadShot, 1),
        ("death_shotgun_front", 0, 0, 0, D::NormalShotgun, 1),
        ("death_shotgun_back", 0, 0, 0, D::NormalShotgun, 2),
        ("death_shieldbash_front", 0, 0, 0, D::MeleeBash, 1),
        ("death_knife_back", 0, 0, 0, D::Melee, 2),
        ("death_explosive_front", 0, 0, 0, D::Explosive, 1),
        ("death_crouch_shotgun_left", 1, 0, 0, D::NormalShotgun, 3),
        ("death_run_backward", 0, 2, 2, D::Normal, 2),
        ("death_prone", 2, 0, 0, D::HeadShot, 1),
    ] {
        if let Some(&c) = torso::get().death(torso::death_context(stance, ms, dir, dmg as u8, from)).first() {
            poses.push((name.into(), Some((torso::DEATH, c))));
        }
    }
    let mut tiles = Vec::new();
    for (k, (name, clip)) in poses.iter().enumerate() {
        let general = 900 + k as u32;
        let t = clip.and_then(|(a, c)| torso::get().clip(c).map(|x| if a == torso::DEATH { x.length + 1.0 } else { x.length * 0.45 })).unwrap_or(0.0);
        // His class's frag is the lethal the pin and the throw hold (`tag_inhand`).
        ffi::publish_offhands(&[(general, [frag.map_or(-1, |f| f.viewmodel), -1])]);
        ffi::publish_torso(clip.map(|(action, clip)| ffi::TorsoAction { general, action, clip, t, lethal: true }).into_iter());
        let px = render_body(archive, &shot, def.viewmodel, def.body_anims, general)?;
        let clip_name = clip.and_then(|(_, c)| torso::get().clip(c)).map_or(String::new(), |c| c.name.clone());
        println!("{id} {name}: {clip_name} at {t:.2} s");
        tiles.push(to_rgb(&px));
    }
    write_sheet_cols(&out.join(format!("torso_{}.png", id.replace(['+', '~'], "_"))), &tiles, tiles.len().min(8) as u32)?;
    // The cost of several Generals at once: `n` bodies (each his own rig, half of them reloading),
    // drawn and presented `FRAMES` times; the core's pose and submit per General, and the frame.
    for n in [1u32, 4, 8, 16] {
        const FRAMES: u32 = 60;
        let generals: Vec<u32> = (0..n).map(|k| 2000 + k).collect();
        let reload = poses.iter().find(|p| p.0 == "reload").and_then(|p| p.1);
        let input = |g: u32, k: u32| ZhboNativeBodyIn {
            general: g,
            mode: 2,
            vm: def.viewmodel,
            faction: FACTION as i32,
            state: zhbo_native::thirdperson::state::id(def.body_anims, zhbo_native::thirdperson::state::STAND_RUN) as i32,
            time: k as f32 / 30.0,
            rate: 1.0,
            pos: [(g % 4) as f32 * 3.0, (g / 4 % 4) as f32 * 3.0, 0.0],
            target_view: VIEW,
            before_view: BEFORE_VIEW,
            light_count: 1,
            lights: [ZhboNativeLight { dir: norm([-0.4, 0.3, -1.0]), color: [1.0; 3] }, ZhboNativeLight::default(), ZhboNativeLight::default()],
            ..Default::default()
        };
        // Load every rig first.
        for _ in 0..ROUNDS {
            for &g in &generals {
                unsafe { ffi::zhbo_native_body(&input(g, 0)) };
            }
            let mut sink = Vec::new();
            serve(archive, &mut sink);
            zhbo_gfx::ffi::with_renderer((), |r| {
                r.touch(VIEW);
                r.frame();
            });
        }
        let (mut core, start, mut drawn) = (std::time::Duration::ZERO, std::time::Instant::now(), 0);
        for k in 0..FRAMES {
            ffi::publish_torso(generals.iter().filter(|g| *g % 2 == 0).filter_map(|&g| reload.map(|(action, clip)| ffi::TorsoAction { general: g, action, clip, t: k as f32 / 30.0, lethal: true })));
            let c = std::time::Instant::now();
            for &g in &generals {
                drawn += (unsafe { ffi::zhbo_native_body(&input(g, k)) } == DRAWN) as u32;
            }
            core += c.elapsed();
            zhbo_gfx::ffi::with_renderer((), |r| {
                r.touch(VIEW);
                r.frame();
            });
        }
        let total = start.elapsed();
        println!("bench {n} Generals ({drawn} of {} drawn): core {:.3} ms/frame, frame with GPU {:.3} ms/frame", n * FRAMES, core.as_secs_f64() * 1000.0 / FRAMES as f64, total.as_secs_f64() * 1000.0 / FRAMES as f64);
    }
    ffi::zhbo_native_body_reset();
    zhbo_gfx::ffi::zhgfx_shutdown();
    Ok(())
}

/// One General in body mode, framed whole from the front-right, as a player beside him sees him.
fn render_body(archive: &Archive, shot: &Shot, vm: i32, body_anims: u8, general: u32) -> Result<Vec<u8>, String> {
    // The body is drawn at the General's size (`units::drawn_inches`).
    let k = zhbo_core::units::DRAWN_ZH_PER_INCH;
    let centre = [0.0, 0.0, 40.0 * k];
    let dist = 30.0 * k / (FOV / 2.0).tan();
    let eye = [centre[0] + dist * 0.75, centre[1] - dist * 0.6, centre[2] + dist * 0.15];
    let view = look_at(eye, centre);
    let aspect = W as f32 / H as f32;
    let t = 1.0 / (FOV / 2.0).tan();
    let (near, far) = (0.05f32, dist * 4.0);
    let proj = [t / aspect, 0.0, 0.0, 0.0, 0.0, t, 0.0, 0.0, 0.0, 0.0, far / (far - near), 1.0, 0.0, 0.0, -near * far / (far - near), 0.0];
    zhbo_gfx::ffi::with_renderer((), |r| r.set_view_transform(VIEW, Some(&view), Some(&proj)));
    let light = ZhboNativeLight { dir: norm([-0.4, 0.3, -1.0]), color: [1.0, 0.97, 0.9] };
    let inp = ZhboNativeBodyIn {
        general,
        mode: 2,
        vm,
        faction: FACTION as i32,
        state: zhbo_native::thirdperson::state::id(body_anims, zhbo_native::thirdperson::state::STAND_IDLE) as i32,
        time: 0.0,
        rate: 1.0,
        pos: [0.0; 3],
        yaw: 0.0,
        eye,
        ambient: [0.55, 0.55, 0.58],
        lights: [light, ZhboNativeLight::default(), ZhboNativeLight::default()],
        light_count: 1,
        target_view: VIEW,
        before_view: BEFORE_VIEW,
        ..Default::default()
    };
    let mut notes = Vec::new();
    // Twice past loading: the torso clip loads on the first drawn frame.
    let mut drawn = 0;
    for _ in 0..ROUNDS {
        let answer = unsafe { ffi::zhbo_native_body(&inp) };
        serve(archive, &mut notes);
        if answer == DRAWN {
            drawn += 1;
            if drawn >= 3 {
                break;
            }
        }
        // Each round submits a draw: present it so the next starts clean.
        zhbo_gfx::ffi::with_renderer((), |r| {
            r.touch(VIEW);
            r.frame();
        });
    }
    take_logs(&mut notes);
    if !notes.is_empty() {
        eprintln!("{notes:?}");
    }
    unsafe { ffi::zhbo_native_body(&inp) };
    shot.lock().unwrap().take();
    zhbo_gfx::ffi::with_renderer((), |r| {
        r.touch(VIEW);
        r.request_screenshot(zhbo_gfx::runtime::INVALID, "");
        r.frame();
    });
    shot.lock().unwrap().take().ok_or_else(|| "the renderer produced no screenshot".into())
}

/// The tiles at full size, `cols` across.
fn write_sheet_cols(path: &Path, tiles: &[Vec<u8>], cols: u32) -> Result<(), String> {
    let rows = (tiles.len() as u32).div_ceil(cols).max(1);
    let (sw, sh) = (W * cols, H * rows);
    let mut sheet = vec![0u8; (sw * sh * 3) as usize];
    for (k, t) in tiles.iter().enumerate() {
        let (ox, oy) = ((k as u32 % cols) * W, (k as u32 / cols) * H);
        for y in 0..H {
            let src = ((y * W) * 3) as usize;
            let dst = (((oy + y) * sw + ox) * 3) as usize;
            sheet[dst..dst + (W * 3) as usize].copy_from_slice(&t[src..src + (W * 3) as usize]);
        }
    }
    write_png(path, sw, sh, &sheet)
}

fn serve(archive: &Archive, notes: &mut Vec<String>) {
    let mut buf = vec![0u8; 1024];
    for _ in 0..4096 {
        let n = unsafe { ffi::zhbo_native_body_wanted(0, buf.as_mut_ptr().cast(), buf.len() as i32) };
        if n < 0 {
            return;
        }
        let path = String::from_utf8_lossy(&buf[..n as usize]).into_owned();
        let mut c = path.clone().into_bytes();
        c.push(0);
        match archive.bytes(&path) {
            Some(bytes) => unsafe { ffi::zhbo_native_body_provide(c.as_ptr().cast(), bytes.as_ptr(), bytes.len() as i32) },
            None => {
                notes.push(format!("{path} is not in the archive"));
                unsafe { ffi::zhbo_native_body_provide(c.as_ptr().cast(), std::ptr::null(), -1) };
            }
        }
    }
}

fn take_logs(into: &mut Vec<String>) {
    let mut buf = vec![0u8; 512];
    for _ in 0..256 {
        let n = unsafe { ffi::zhbo_native_body_take_log(buf.as_mut_ptr().cast(), buf.len() as i32) };
        if n < 0 {
            return;
        }
        let line = String::from_utf8_lossy(&buf[..n as usize]).into_owned();
        // The composed line is every load's; only the problems are notes.
        if !line.contains("composed") {
            into.push(line);
        }
    }
}

fn to_rgb(bgra: &[u8]) -> Vec<u8> {
    bgra.as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0]]).collect()
}

fn write_png(path: &Path, w: u32, h: u32, rgb: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| format!("{}: {e}", path.display()))?;
    writer.write_image_data(rgb).map_err(|e| format!("{}: {e}", path.display()))
}

/// The tiles at half size, six across.
fn write_sheet(path: &Path, tiles: &[Vec<u8>]) -> Result<(), String> {
    const COLS: u32 = 6;
    let (tw, th) = (W / 2, H / 2);
    let rows = (tiles.len() as u32).div_ceil(COLS).max(1);
    let (sw, sh) = (tw * COLS, th * rows);
    let mut sheet = vec![0u8; (sw * sh * 3) as usize];
    for (k, t) in tiles.iter().enumerate() {
        let (ox, oy) = ((k as u32 % COLS) * tw, (k as u32 / COLS) * th);
        for y in 0..th {
            for x in 0..tw {
                let src = (((y * 2) * W + x * 2) * 3) as usize;
                let dst = (((oy + y) * sw + ox + x) * 3) as usize;
                sheet[dst..dst + 3].copy_from_slice(&t[src..src + 3]);
            }
        }
    }
    write_png(path, sw, sh, &sheet)
}
