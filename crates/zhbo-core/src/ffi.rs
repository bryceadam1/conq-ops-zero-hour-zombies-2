//! C ABI called by the engine glue (`host/zhbo/ZhboBridge.cpp`). Declarations: `host/zhbo/zhbo_core.h`.
//!
//! All entry points run on the game's main thread; the mutex only guards against misuse. Callbacks
//! into the engine happen while the lock is held, so the engine must not call back into a
//! `zhbo_logic_*`/`zhbo_client_*` function from inside one; `zhbo_logic_is_controlled` and
//! `zhbo_string` are lock-free for exactly that reason.

use crate::client::{Client, ClientHit, ClientIn, ClientOut};
use crate::host::{CClientHost, CLogicHost, ClientApi, ClientHost, LogicApi};
use crate::input::FpsInput;
use crate::sim::{Sim, MAX_PLAYERS};
use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

struct State {
    logic: CLogicHost,
    client_host: CClientHost,
    sim: Sim,
    client: Client,
    /// The shell create-a-class screen (`zhbo_menu_*`): `Some` while its window is up. Local UI
    /// only — it never touches the sim (docs/CONTRACT.md's menu ABI).
    menu: Option<crate::menu::MenuState>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Lock-free mirror of `Sim::controlled_ids` for the AI/physics suppression hooks.
static CONTROLLED: [AtomicU32; MAX_PLAYERS] = [const { AtomicU32::new(0) }; MAX_PLAYERS];
/// Per player, the last shellshock the log reported (`log_step`): his General and its serial.
static SHOCK_LOGGED: [AtomicU64; MAX_PLAYERS] = [const { AtomicU64::new(0) }; MAX_PLAYERS];
/// Per-player impact counts by surface (12 slots), accumulated every logic frame and printed
/// + reset on the 30s — the client's per-client-frame `sim.events.clear()` can't eat a kill.
static IMPACT_ACCUM: [[AtomicU32; 12]; MAX_PLAYERS] = [const { [const { AtomicU32::new(0) }; 12] }; MAX_PLAYERS];

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_mut().map(f)
}

/// Lock-free mirror of the frame's shield bearers ([`Sim::shields`], `crate::shield`) for the
/// engine's damage hook (`zhbo_logic_shield_damage`), which runs inside Zero Hour's own object
/// updates — possibly inside a `zhbo_logic_step` callback, so it must never take the state lock.
/// Written after each logic step from sim state only (deterministic on every peer).
const SHIELD_SLOTS: usize = 2 * MAX_PLAYERS;
static SHIELDS: [[AtomicU32; crate::shield::MIRROR_WORDS]; SHIELD_SLOTS] = [const { [const { AtomicU32::new(0) }; crate::shield::MIRROR_WORDS] }; SHIELD_SLOTS];
static SHIELD_COUNT: AtomicU32 = AtomicU32::new(0);
/// The tuning pack's four blast protections, mirrored with the bearers (no lock in the hook).
static SHIELD_PROTECTION: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];

/// Zero Hour damage to a deployed shield's object, queued by the hook (lock-free: a fixed ring of
/// object, source, kind, amount) and handed to the sim at the next logic step, in engine order.
const SHIELD_HITS: usize = 64;
static SHIELD_HIT_QUEUE: [[AtomicU32; 4]; SHIELD_HITS] = [const { [const { AtomicU32::new(0) }; 4] }; SHIELD_HITS];
static SHIELD_HIT_COUNT: AtomicU32 = AtomicU32::new(0);

fn publish_shields(sim: &Sim) {
    let n = sim.shields.len().min(SHIELD_SLOTS);
    for (slot, b) in SHIELDS.iter().zip(&sim.shields) {
        for (w, v) in slot.iter().zip(b.to_words()) {
            w.store(v, Ordering::Relaxed);
        }
    }
    SHIELD_COUNT.store(n as u32, Ordering::Relaxed);
    let t = crate::tuning::get();
    for (a, v) in SHIELD_PROTECTION.iter().zip([t.shield_blast_protection_30, t.shield_blast_protection_60, t.shield_blast_protection_120, t.shield_blast_protection_180]) {
        a.store(v.to_bits(), Ordering::Relaxed);
    }
}

fn mirrored_shields() -> Vec<crate::shield::Bearer> {
    let n = (SHIELD_COUNT.load(Ordering::Relaxed) as usize).min(SHIELD_SLOTS);
    SHIELDS[..n]
        .iter()
        .map(|slot| {
            let mut w = [0u32; crate::shield::MIRROR_WORDS];
            for (d, s) in w.iter_mut().zip(slot) {
                *d = s.load(Ordering::Relaxed);
            }
            crate::shield::Bearer::from_words(&w)
        })
        .collect()
}

/// Hand the queued engine damage to the deployed shields to the sim (before its step).
fn drain_shield_hits(s: &mut State) {
    let n = (SHIELD_HIT_COUNT.swap(0, Ordering::Relaxed) as usize).min(SHIELD_HITS);
    for e in &SHIELD_HIT_QUEUE[..n] {
        let w: [u32; 4] = std::array::from_fn(|k| e[k].load(Ordering::Relaxed));
        s.sim.engine_damage_to_deployed(&s.logic, w[0], w[1], w[2] as i32, f32::from_bits(w[3]));
    }
}

/// The Assault Shield in Zero Hour's damage path (`Object::attemptDamage`, host patch 0017): the
/// share of `amount` the engine applies to `victim`. A deployed shield's object takes none — the
/// damage is queued for the sim's wear-down; a first-person General's held/stowed shield stops a
/// round or blow from its side and cuts a blast ([`crate::shield::engine_scale`]); damage the sim
/// itself dealt (a controlled General's) passes, the sim already tested it. `kind` is the core's
/// [`crate::host::damage_kind`] (the bridge maps Zero Hour's `DamageType`; -1 for the rest);
/// `from` is the source's feet and height when `source_known`. Lock-free.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn zhbo_logic_shield_damage(victim: u32, source: u32, from_x: f32, from_y: f32, from_z: f32, from_height: f32, source_known: i32, kind: i32, amount: f32) -> f32 {
    if source != 0 && zhbo_logic_is_controlled(source) != 0 {
        return 1.0;
    }
    let bearers = mirrored_shields();
    if victim != 0 && bearers.iter().any(|b| b.object == victim) {
        let i = SHIELD_HIT_COUNT.load(Ordering::Relaxed) as usize;
        if i < SHIELD_HITS && amount > 0.0 {
            for (slot, v) in SHIELD_HIT_QUEUE[i].iter().zip([victim, source, kind as u32, amount.to_bits()]) {
                slot.store(v, Ordering::Relaxed);
            }
            SHIELD_HIT_COUNT.store(i as u32 + 1, Ordering::Relaxed);
        }
        return 0.0;
    }
    let from = (source_known != 0).then(|| (crate::math::Vec3::new(from_x, from_y, from_z), from_height));
    let p = |i: usize| f32::from_bits(SHIELD_PROTECTION[i].load(Ordering::Relaxed));
    let t = crate::tuning::Tuning { shield_blast_protection_30: p(0), shield_blast_protection_60: p(1), shield_blast_protection_120: p(2), shield_blast_protection_180: p(3), ..Default::default() };
    crate::shield::engine_scale(&bearers, victim, from, kind, &t)
}

/// Lock-free mirror of the standing smoke screens ([`Sim::smoke_blockers`], [`crate::smoke`]) for
/// the engine's sight and path hooks (host patch 0024), which run inside Zero Hour's object and
/// pathfinder updates (and its UI's attack checks), never under the state lock. Written after each
/// logic step from sim state only, so every peer's engine gets the same answers: centre, radius
/// (bits), the frame it went off on.
const SMOKE_SLOTS: usize = 32;
static SMOKES: [[AtomicU32; 5]; SMOKE_SLOTS] = [const { [const { AtomicU32::new(0) }; 5] }; SMOKE_SLOTS];
static SMOKE_COUNT: AtomicU32 = AtomicU32::new(0);
/// The tuning pack's always-seen distance (Zero Hour units, bits).
static SMOKE_MIN_DISTANCE: AtomicU32 = AtomicU32::new(0);

fn publish_smokes(sim: &Sim) {
    let blockers = sim.smoke_blockers();
    for (slot, b) in SMOKES.iter().zip(&blockers) {
        for (w, v) in slot.iter().zip([b.at.x.to_bits(), b.at.y.to_bits(), b.at.z.to_bits(), b.radius.to_bits(), b.born]) {
            w.store(v, Ordering::Relaxed);
        }
    }
    SMOKE_MIN_DISTANCE.store(sim.smoke.min_distance.to_bits(), Ordering::Relaxed);
    SMOKE_COUNT.store(blockers.len().min(SMOKE_SLOTS) as u32, Ordering::Relaxed);
}

/// Whether any mirrored screen passes `test` (no allocation: the pathfinder asks per cell).
fn any_smoke(test: impl Fn(&crate::smoke::Blocker) -> bool) -> bool {
    let n = (SMOKE_COUNT.load(Ordering::Relaxed) as usize).min(SMOKE_SLOTS);
    SMOKES[..n].iter().any(|slot| {
        let w = |k: usize| slot[k].load(Ordering::Relaxed);
        let f = |k: usize| f32::from_bits(w(k));
        test(&crate::smoke::Blocker { at: crate::math::Vec3::new(f(0), f(1), f(2)), radius: f(3), born: w(4) })
    })
}

/// Whether a standing smoke screen blocks the line of sight from `a` to `b` (world points, Zero
/// Hour units): BO2's sight test ([`crate::smoke::blocks_sight`]). The engine's units neither
/// acquire nor keep a target it hides (`WeaponSet::getAbleToAttackSpecificObject`). Lock-free.
#[no_mangle]
pub extern "C" fn zhbo_logic_smoke_blocks(ax: f32, ay: f32, az: f32, bx: f32, by: f32, bz: f32) -> i32 {
    if SMOKE_COUNT.load(Ordering::Relaxed) == 0 {
        return 0;
    }
    let (a, b) = (crate::math::Vec3::new(ax, ay, az), crate::math::Vec3::new(bx, by, bz));
    let min = f32::from_bits(SMOKE_MIN_DISTANCE.load(Ordering::Relaxed));
    if (b - a).dot(b - a) < min * min {
        return 0;
    }
    any_smoke(|s| crate::smoke::segment_distance_sq(a, b, s.at) < s.radius * s.radius) as i32
}

/// Whether the ground point `(x, y)` is under a standing smoke screen (the pathfinder's extra
/// cost there). Lock-free.
#[no_mangle]
pub extern "C" fn zhbo_logic_smoke_covers(x: f32, y: f32) -> i32 {
    any_smoke(|s| crate::smoke::covers(std::slice::from_ref(s), x, y)) as i32
}

/// Whether the ground segment `(ax, ay)`–`(bx, by)` passes under a smoke screen that went off after
/// logic frame `since` (a path computed before it stood; the move repaths once). Lock-free.
#[no_mangle]
pub extern "C" fn zhbo_logic_smoke_crosses(ax: f32, ay: f32, bx: f32, by: f32, since: u32) -> i32 {
    any_smoke(|s| crate::smoke::crosses_since(std::slice::from_ref(s), ax, ay, bx, by, since)) as i32
}

fn publish_controlled(sim: &Sim) {
    let mut ids = sim.controlled_ids();
    for slot in CONTROLLED.iter() {
        slot.store(ids.next().unwrap_or(0), Ordering::Relaxed);
    }
}

pub const ABI_VERSION: u32 = 31;

/// Field counts of the two API tables (`host.rs`'s `LogicApi`/`ClientApi`). `host/zhbo/zhbo_core.h`
/// defines the same numbers; both sides' layout asserts and `zhbo_abi_fields` derive from them.
pub const LOGIC_API_FIELDS: u64 = 24;
pub const CLIENT_API_FIELDS: u64 = 16;

// Layouts shared with host/zhbo/zhbo_core.h (which asserts the same sizes on the C++ side).
const _: () = {
    use std::mem::size_of;
    assert!(size_of::<crate::host::ObjInfo>() == 56);
    assert!(size_of::<FpsInput>() == 20);
    assert!(size_of::<LogicCrc>() == 152);
    assert!(size_of::<ClientIn>() == 136);
    // `spectating` took the struct's tail padding: the size is unchanged, the offset is asserted.
    assert!(std::mem::offset_of!(ClientIn, spectating) == 132);
    assert!(size_of::<ClientOut>() == 4880);
    assert!(size_of::<ClientHit>() == 44);
    assert!(size_of::<crate::sim::RtsWeapons>() == 192);
    assert!(size_of::<crate::sim::RtsIcons>() == 240);
    assert!(size_of::<crate::building::BuildOption>() == 64);
    assert!(size_of::<crate::building::Site>() == 64);
    assert!(size_of::<crate::menu::MenuIn>() == 56);
    assert!(size_of::<crate::menu::MenuOut>() == 4108);
    assert!(size_of::<ClassButton>() == 756);
    assert!(size_of::<WorldPose>() == 16);
    // ZHBO_CONTENT_PARTS and ZhboContentCrc (appended after ABI 29).
    assert!(crate::content::PART_COUNT == 7);
    assert!(size_of::<crate::content::ContentCrc>() == 32);
    // host/zhbo/zhbo_core.h's ZHBO_CLASS_BUTTONS (the bridge's fixed array).
    assert!(crate::classicons::BUTTONS == 15);
    assert!(size_of::<LogicApi>() == (LOGIC_API_FIELDS as usize) * size_of::<usize>());
    assert!(size_of::<ClientApi>() == (CLIENT_API_FIELDS as usize) * size_of::<usize>());
};

/// Load the gameplay tuning pack (`data/ConquerOps/tuning.ini`); the sim's default values hold
/// until this runs.
///
/// # Safety
/// `text` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_tuning(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::tuning::load(&text);
}

/// Load the shipped strings file (`Data/ConquerOps/strings_en.ini`, read by the bridge's
/// `readTextFile`); `lookup` consults it before the built-in English fallback.
///
/// # Safety
/// `text` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_strings(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::strings::load(&text);
}

/// Load the BO2 font metrics (`names::FONTS_PATH` in the converted archive, read by the bridge's
/// `readTextFile`; empty without the archive, and text then uses the engine font).
///
/// # Safety
/// `text` is null or a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_fonts(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::lui::font::load(&text);
}

/// Load the converted BO2 impact table (`Data\ConquerOps\impacts.txt`, read by the bridge's
/// `readTextFile`); until it runs no impact FX plays (the impact sound still does). See
/// `crate::impacts`. Returns the number of rows installed (-1 for a null text).
///
/// # Safety
/// `text` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_impacts(text: *const c_char) -> i32 {
    if text.is_null() {
        return -1;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::impacts::install(&text) as i32
}

/// Load a map's ground surfaces (`crate::terrain`): `bytes` is the map file as the engine reads it
/// (decompressed, `CachedFileInputStream`), and `names`/`surfaces` are Zero Hour's terrain types in
/// collection order, each name with its `ZHBO_SURF_*` (the bridge classifies `Terrain.ini`'s
/// classes). Replaces the previous map's grid. Returns the grid's cell count, or -1 when the file
/// does not parse (the grid is then cleared: every ground reads the default surface, the same on
/// every machine). ABI 30.
///
/// # Safety
/// `bytes` must point at `len` readable bytes (or be null with `len` 0); `names` and `surfaces`
/// at `count` entries each, every name null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_terrain_load(bytes: *const u8, len: i32, names: *const *const c_char, surfaces: *const i32, count: i32) -> i32 {
    let bytes: &[u8] = match usize::try_from(len) {
        Ok(n) if n > 0 && !bytes.is_null() => unsafe { std::slice::from_raw_parts(bytes, n) },
        _ => &[],
    };
    let mut terrain = Vec::new();
    if let Ok(n) = usize::try_from(count) {
        if !names.is_null() && !surfaces.is_null() {
            for i in 0..n {
                let (name, surface) = unsafe { (*names.add(i), *surfaces.add(i)) };
                if !name.is_null() {
                    terrain.push((unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned(), crate::impacts::Surface::from_i32(surface)));
                }
            }
        }
    }
    match crate::terrain::parse_map(bytes, &terrain) {
        Ok(grid) => {
            let cells = grid.cells.len() as i32;
            crate::terrain::install(Some(grid));
            cells
        }
        Err(_) => {
            crate::terrain::install(None);
            -1
        }
    }
}

/// The loaded map grid's surface at a world point (a `ZHBO_SURF_*`; the default surface with no
/// map loaded): what the sim's ground query reads under the water table. The bridge's load-time
/// cross-check against the client's terrain visual uses it. ABI 30.
#[no_mangle]
pub extern "C" fn zhbo_terrain_surface_at(x: f32, y: f32) -> i32 {
    crate::terrain::surface_at(x, y) as i32
}

/// Load the third-person shots' layer rows (`Data\ConquerOps\sound_layers.txt`, read by the
/// bridge's `readTextFile`); until it runs another player's shot plays the converted close mix.
/// See `crate::soundlayers`. Returns the number of layers installed (-1 for a null text).
///
/// # Safety
/// `text` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_sound_layers(text: *const c_char) -> i32 {
    if text.is_null() {
        return -1;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    // The same file carries the third-person torso table (`torso`).
    crate::torso::install(&text);
    crate::soundlayers::install(&text) as i32
}

/// Load the class preset list (`Data/ConquerOps/classes.ini`, read by the bridge's
/// `readTextFile`); the five basic BO2 classes hold until this runs (see classes.rs).
///
/// # Safety
/// `text` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_classes(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::classes::load(&text);
}

/// Load the player's create-a-class profile (`ConquerOps.profile`, read by the bridge from the
/// user-data directory at init); the empty profile holds until this runs (see profile.rs).
///
/// # Safety
/// `text` must be a NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_profile(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::profile::load(&text);
}

/// The profile text buffer for [`zhbo_profile_save`]'s return: a stable UTF-8 NUL-terminated copy
/// the bridge reads and writes to the profile file. Replaced on every save (the bridge writes it
/// before the next call).
static PROFILE_TEXT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Keep `text` in the stable buffer `zhbo_profile_save`/`zhbo_profile_write` return.
fn store_profile_text(text: &str) -> *const c_char {
    let mut buf = PROFILE_TEXT.lock().unwrap_or_else(|e| e.into_inner());
    buf.clear();
    buf.extend_from_slice(text.as_bytes());
    buf.push(0);
    buf.as_ptr().cast::<c_char>()
}

/// Save a custom class into the profile from the choose-class payload's wire words
/// ([`crate::classes::CustomClass::from_wire_words`]). Returns a pointer to the profile's full text
/// (NUL-terminated) for the bridge to write, or null when the words don't decode (nothing saved).
///
/// Kept for callers that still have a class's wire words; the class screen now writes the whole
/// profile through [`zhbo_profile_write`] (delete/copy have no class to encode).
///
/// # Safety
/// `words` must point to `len` `u32`s (or be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn zhbo_profile_save(words: *const u32, len: i32) -> *const c_char {
    if words.is_null() || len <= 0 {
        return std::ptr::null();
    }
    let words = unsafe { std::slice::from_raw_parts(words, len as usize) };
    let Some(class) = crate::classes::CustomClass::from_wire_words(words) else {
        return std::ptr::null();
    };
    store_profile_text(&crate::profile::save(class))
}

/// Write the profile from the client's `save_profile` one-shot: the words are the profile file's
/// full text (`profile::pack_text`), which the core already applied (the editor's Save, or a
/// delete/copy). Loads it back (idempotent) and returns a NUL-terminated copy for the bridge to
/// write, or null when the words don't decode. `len` [`crate::profile::PROFILE_LIVE`] means the
/// text did not fit the payload: the core's own (already applied) profile is the text.
///
/// # Safety
/// `words` must point to `len` `u32`s (or be null when `len` is 0 or `PROFILE_LIVE`).
#[no_mangle]
pub unsafe extern "C" fn zhbo_profile_write(words: *const u32, len: i32) -> *const c_char {
    if len == crate::profile::PROFILE_LIVE {
        return store_profile_text(&crate::profile::get().to_text());
    }
    // An empty profile (the last class deleted) is a legitimate write of zero words.
    let words: &[u32] = unsafe {
        if len > 0 && !words.is_null() {
            std::slice::from_raw_parts(words, len as usize)
        } else if len == 0 {
            &[]
        } else {
            return std::ptr::null();
        }
    };
    let Some(text) = crate::profile::unpack_text(words) else {
        return std::ptr::null();
    };
    crate::profile::load(&text);
    store_profile_text(&text)
}

/// Returns the ABI version; the glue refuses to run if it does not match its header.
#[no_mangle]
pub extern "C" fn zhbo_abi_version() -> u32 {
    ABI_VERSION
}

/// The API tables' field counts, packed (low word: logic, high word: client). `Zhbo_Init` refuses
/// a mismatch with a distinct message, so a silently shifted table is caught even when the sizes
/// happen to agree.
#[no_mangle]
pub extern "C" fn zhbo_abi_fields() -> u64 {
    LOGIC_API_FIELDS | (CLIENT_API_FIELDS << 32)
}

/// Install the weapon table (`Data\ConquerOps\weapons.ini`, converted from the player's BO2 install)
/// before play. Returns the number of weapons, or -1 if it was rejected (the built-in table stays).
///
/// # Safety
/// `text` must be a NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_install_weapons(text: *const std::ffi::c_char) -> i32 {
    if text.is_null() {
        return -1;
    }
    let text = std::ffi::CStr::from_ptr(text).to_string_lossy();
    match crate::weapons::install_catalog(&text) {
        Ok(n) => n as i32,
        Err(_) => -1,
    }
}

/// # Safety
/// `logic` and `client` must point to fully initialised tables that outlive the process.
#[no_mangle]
pub unsafe extern "C" fn zhbo_init(logic: *const LogicApi, client: *const ClientApi) {
    if logic.is_null() || client.is_null() {
        return;
    }
    let mut state = State {
        logic: CLogicHost(*logic),
        client_host: CClientHost(*client),
        sim: Sim::with_tuning(&crate::tuning::get()),
        client: Client::default(),
        menu: None,
    };
    use crate::host::LogicHost;
    state.logic.log(&format!("zhbo-core {} initialised (ABI {ABI_VERSION})", env!("CARGO_PKG_VERSION")));
    // Every ZHBO_* variable the core reads, parsed once (see config.rs); the sim reads the struct.
    let config = crate::config::Config::from_env();
    match config.autopilot {
        crate::config::Autopilot::Off => {}
        crate::config::Autopilot::Manual => {
            state.logic.log("autopilot scene enabled, manual input (ZHBO_AUTOPILOT=manual)")
        }
        crate::config::Autopilot::Scripted { equipment, build, classpick, scope, smoke, splash, drops } => {
            #[cfg(feature = "autopilot")]
            {
                state.client.autopilot = Some(crate::client::Autopilot { equipment, build, classpick, scope, smoke, splash, drops, ..Default::default() });
            }
            let _ = (equipment, build, classpick, scope, smoke, splash, drops);
            state.logic.log("autopilot enabled (ZHBO_AUTOPILOT)");
        }
        crate::config::Autopilot::Multiplayer => {
            let shooter = config.mp_role == crate::config::MpRole::Shooter;
            #[cfg(feature = "autopilot")]
            {
                state.client.autopilot = Some(crate::client::Autopilot { mp: true, mp_shooter: shooter, mp_rounds: config.mp_rounds, ..Default::default() });
            }
            state.logic.log(&format!("autopilot enabled: the multiplayer run, as the {}, {} round(s) (ZHBO_AUTOPILOT=mp)", if shooter { "shooter" } else { "target" }, config.mp_rounds));
        }
    }
    if config.state_hash_log {
        state.logic.log("logging the sim's state hash every logic frame (ZHBO_STATE_HASH_LOG)");
    }
    crate::config::init(config);
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
}

/// New game, loaded game or return to the shell.
#[no_mangle]
pub extern "C" fn zhbo_reset() {
    with_state(|s| {
        s.sim.reset();
        s.client.reset();
        // The shell create-a-class screen never survives a game transition (its window is the
        // bridge's; it goes away with the shell).
        s.menu = None;
        publish_controlled(&s.sim);
        publish_shields(&s.sim);
        publish_smokes(&s.sim);
        crate::nativebody::ffi::publish_stowed(&s.sim.stowed_weapons());
    });
}

#[no_mangle]
pub extern "C" fn zhbo_logic_take_control(player: i32, general: u32) {
    with_state(|s| {
        s.sim.take_control(&s.logic, player, general);
        publish_controlled(&s.sim);
        publish_shields(&s.sim);
        publish_smokes(&s.sim);
        crate::nativebody::ffi::publish_stowed(&s.sim.stowed_weapons());
    });
}

/// # Safety
/// `input` must point to a valid [`FpsInput`].
#[no_mangle]
pub unsafe extern "C" fn zhbo_logic_input(player: i32, general: u32, input: *const FpsInput) {
    if input.is_null() {
        return;
    }
    let input = *input;
    with_state(|s| s.sim.input(player, general, input));
}

/// `MSG_ZHBO_FPS_INPUT` with its seventh argument: the logic frame the sender's machine showed when
/// it made the input (`ZhboClientOut.input_seen_frame`). Its shots are resolved against that frame
/// (`sim::antilag`) and the sender's prediction is acknowledged by it (`client::predict`).
/// Appended to the ABI after `zhbo_logic_input`, which stays for a message without the argument.
///
/// # Safety
/// `input` must point to a valid [`FpsInput`].
#[no_mangle]
pub unsafe extern "C" fn zhbo_logic_fps_input(player: i32, general: u32, input: *const FpsInput, seen_frame: u32) {
    if input.is_null() {
        return;
    }
    let input = *input;
    with_state(|s| s.sim.input_seen(player, general, input, seen_frame));
}

/// The pick menu's choice (see `classes.rs`): the General assumes the class at the shared list's
/// index, or a custom class when `payload` carries its wire words. A negative index with an empty
/// payload is ignored.
///
/// # Safety
/// `payload` must point to `len` u32 words when `len` > 0 (the bridge's stack array).
#[no_mangle]
pub unsafe extern "C" fn zhbo_logic_choose_class(player: i32, general: u32, index: i32, payload: *const u32, len: i32) {
    // # Safety: the bridge passes a valid pointer and length, per the contract above.
    let payload: &[u32] = unsafe {
        if len > 0 && !payload.is_null() {
            std::slice::from_raw_parts(payload, len as usize)
        } else {
            &[]
        }
    };
    // A custom class travels as a payload; the index is then unused (`CHOOSE_CLASS_CUSTOM` is
    // `i32::MIN`), so it must not be clamped to a real list index. A negative index with an empty
    // payload is the ignored case the contract above describes.
    let index = if payload.is_empty() { usize::try_from(index).unwrap_or(usize::MAX) } else { 0 };
    with_state(|s| {
        use crate::host::LogicHost;
        // The engine dispatches messages before `zhbo_logic_step`, so `log_step`'s snapshot
        // already contains the event; log the pick here (the user-facing "assumed the ... class").
        let before = s.sim.emitted;
        s.sim.choose_class(player, general, index, payload);
        for e in s.sim.events.iter().skip(s.sim.first_event_since(before)) {
            if let Some(line) = crate::sim::describe_event(e) {
                s.logic.log(&line);
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn zhbo_logic_step(frame: u32) {
    with_state(|s| {
        let before = s.sim.emitted;
        drain_shield_hits(s);
        s.sim.step(&s.logic, frame);
        // The step's tracer bullets, as this machine's viewer sees them (client-only FX).
        s.client.tracers.play(&s.logic, &mut s.sim.tracers);
        publish_controlled(&s.sim);
        publish_shields(&s.sim);
        publish_smokes(&s.sim);
        crate::nativebody::ffi::publish_stowed(&s.sim.stowed_weapons());
        log_step(s, s.sim.first_event_since(before), frame);
        for (shooter, target, seen) in std::mem::take(&mut s.sim.antilag_hits) {
            use crate::host::LogicHost;
            s.logic.log(&format!("antilag: General {shooter}'s round hit General {target} where he stood at frame {seen} ({} back)", frame.saturating_sub(seen)));
        }
        // Two machines' logs of one network game agree on this line for every frame both ran, or
        // the sims diverged there (scripts/mptest-mac.sh compares them): the fingerprint with its
        // parts, so a divergence names the seat (or the world) it is in.
        if crate::config::get().state_hash_log {
            use crate::host::LogicHost;
            s.logic.log(&format!("state hash: frame {frame} {}", fingerprint_parts(&s.sim.fingerprint())));
        }
    });
}

/// The sim's share of Zero Hour's logic CRC (`ZhboLogicCrc`): [`crate::sim::Fingerprint`] for the
/// C side. An empty seat's hash is 0.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogicCrc {
    pub hash: u64,
    pub world: u64,
    pub seats: [u64; MAX_PLAYERS],
    pub frame: u32,
    /// 1 when the sim holds state (the hash then joins the CRC), 0 when it is idle.
    pub active: u32,
}

/// `GameLogic::getCRC` (host patch 0020, after `MARKER:TheAI`): the sim's state hash
/// (`Sim::state_hash`, the multiplayer gate's) and its per-seat parts into `out`. Returns 1 when
/// the sim holds state and the bridge folds the hash into the CRC, 0 when it is idle (no General
/// deployed, no class picked: the CRC stays stock Zero Hour's) or not initialised. With `log`
/// (the logic frame's own CRC, not a client-side or deep-dump one) an active sim also writes the
/// parts to `ConquerOps.log`, so two machines' logs name the seat a desync started in.
///
/// # Safety
/// `out` must point to a writable [`LogicCrc`] (or be null: nothing is written).
#[no_mangle]
pub unsafe extern "C" fn zhbo_logic_crc(out: *mut LogicCrc, log: i32) -> i32 {
    let crc = with_state(|s| {
        let f = s.sim.fingerprint();
        if f.active && log != 0 {
            use crate::host::LogicHost;
            s.logic.log(&crc_line(&f));
        }
        f
    });
    let Some(f) = crc else { return 0 };
    if !out.is_null() {
        // # Safety: the caller passes a valid, writable LogicCrc (see above).
        unsafe { *out = LogicCrc { hash: f.hash, world: f.world, seats: f.seats.map(|s| s.unwrap_or(0)), frame: f.frame, active: f.active as u32 } };
    }
    f.active as i32
}

/// The CRC frame's `ConquerOps.log` line: the hash, the world's part and each seat in use.
fn crc_line(f: &crate::sim::Fingerprint) -> String {
    format!("crc frame {}: {}", f.frame, fingerprint_parts(f))
}

/// A fingerprint as the logs print it: `<hash> world <hash> seats <seat>:<hash> …` (the CRC
/// frame's line and `ZHBO_STATE_HASH_LOG`'s per-frame one), so two machines' lines diff to the
/// seat, or the world, that went apart.
fn fingerprint_parts(f: &crate::sim::Fingerprint) -> String {
    let mut line = format!("{:016x} world {:016x} seats", f.hash, f.world);
    for (i, s) in f.seats.iter().enumerate() {
        if let Some(s) = s {
            line.push_str(&format!(" {i}:{s:016x}"));
        }
    }
    line
}

/// `ConquerOps.log` is the in-game oracle: lifecycle events, kills, and a once-per-second pose.
fn log_step(s: &State, first_new_event: usize, frame: u32) {
    use crate::host::LogicHost;
    log_events(s, first_new_event);
    // R8d: count the Impact events into the per-player accumulator EVERY frame (the client clears
    // `sim.events` per client frame, so the events must be counted before they go); the print +
    // reset happens on the 30s, so a kill on a non-print frame still shows in the next line.
    {
        use crate::sim::EventKind;
        for e in s.sim.events.iter().skip(first_new_event) {
            if let EventKind::Impact { surface, .. } = e.kind {
                IMPACT_ACCUM[e.player as usize % IMPACT_ACCUM.len()][surface as usize].fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    // A shellshock taking hold (the sim's own, so every machine logs it for every player).
    for (i, p) in s.sim.players.iter().enumerate() {
        if let Some((general, a)) = p.as_ref().and_then(|p| Some((p.general, p.shock.active.as_ref()?))) {
            let key = (general as u64) << 32 | a.serial as u64;
            if SHOCK_LOGGED[i % MAX_PLAYERS].swap(key, Ordering::Relaxed) != key {
                s.logic.log(&format!("player {i}'s General is shellshocked ({}, {:.1} s)", a.def.name, a.duration));
            }
        }
    }
    if frame.is_multiple_of(30) {
        use crate::impacts::Surface;
        for (i, p) in s.sim.players.iter().enumerate() {
            if let Some(p) = p.as_ref().filter(|p| p.is_first_person()) {
                let w = p.arsenal.current();
                // Behind the spawn's class screen nothing is in hand (`PlayerSim::awaiting_class`).
                let held = if p.awaiting_class() { "awaiting a class".to_string() } else { format!("{} {}|{}", w.def.display_name, w.ammo, w.reserve) };
                s.logic.log(&format!(
                    "player {i} pos ({:.1}, {:.1}, {:.1}) yaw {:.2} pitch {:.2} stance {:?} hp {:.0}/{:.0} {held}",
                    p.body.pos.x, p.body.pos.y, p.body.pos.z, p.yaw, p.pitch, p.body.stance,
                    p.health, p.max_health
                ));
                let counts = &IMPACT_ACCUM[i % IMPACT_ACCUM.len()];
                let by: Vec<(&str, u32)> = Surface::ALL
                    .iter()
                    .enumerate()
                    .map(|(k, s)| (s.name(), counts[k].swap(0, Ordering::Relaxed)))
                    .filter(|&(_, n)| n > 0)
                    .collect();
                if !by.is_empty() {
                    let landed: u32 = by.iter().map(|&(_, n)| n).sum();
                    let list: Vec<String> = by.iter().map(|(name, n)| format!("{name} {n}")).collect();
                    s.logic.log(&format!("player {i} impacts: {landed} ({})", list.join(", ")));
                }
            }
        }
    }
}

/// The sim's events from `first_new_event` on, as `ConquerOps.log` lines.
fn log_events(s: &State, first_new_event: usize) {
    use crate::host::LogicHost;
    for e in s.sim.events.iter().skip(first_new_event) {
        if let Some(line) = crate::sim::describe_event(e) {
            s.logic.log(&line);
        }
    }
}

/// A General under RTS control, once per logic frame (see `Sim::rts_update`): the Zero Hour weapon
/// slot in use, the shots each slot fired since the last call, and back the ammo per slot and the
/// slot to lock to.
///
/// # Safety
/// `shots` must point to 3 values; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn zhbo_logic_rts_update(
    player: i32,
    general: u32,
    current_slot: i32,
    shots: *const u32,
    target: *const crate::math::Vec3,
    out: *mut crate::sim::RtsWeapons,
) {
    if shots.is_null() || out.is_null() {
        return;
    }
    let shots = [*shots, *shots.add(1), *shots.add(2)];
    let target = (!target.is_null()).then(|| *target);
    // His orders' throws, plants and knife strikes happen here, before the logic step: log them too.
    let r = with_state(|s| {
        let before = s.sim.emitted;
        let r = s.sim.rts_update(&s.logic, player, general, current_slot, shots, target);
        log_events(s, s.sim.first_event_since(before));
        r
    })
    .unwrap_or_default();
    *out = r;
}

/// Under RTS control: what the General's third weapon slot does when it next fires — `choice` 0
/// throws (or plants) the lethal, 1 the tactical, 2 strikes once with the knife (`sim::Third`).
/// Lockstep: from a game message.
#[no_mangle]
pub extern "C" fn zhbo_logic_rts_offhand(player: i32, general: u32, choice: i32) {
    with_state(|s| s.sim.rts_offhand(player, general, choice));
}

/// Under RTS control: the reload order for the gun in Zero Hour slot `current_slot`. Returns the
/// slot whose magazine was topped up (the bridge starts Zero Hour's reload on it), or -1 when
/// there was nothing to reload. Lockstep: from a game message.
#[no_mangle]
pub extern "C" fn zhbo_logic_rts_reload(player: i32, general: u32, current_slot: i32) -> i32 {
    with_state(|s| s.sim.rts_reload(player, general, current_slot)).unwrap_or(-1)
}

/// The ammo of a General's weapon slots, for the control bar.
///
/// # Safety
/// `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn zhbo_client_rts_ammo(general: u32, out: *mut crate::sim::RtsWeapons) {
    if out.is_null() {
        return;
    }
    *out = with_state(|s| s.sim.rts_ammo(general)).unwrap_or_else(|| crate::sim::Sim::new().rts_ammo(general));
}

/// The selected General's kit icons for his command bar (`sim::RtsIcons`).
///
/// # Safety
/// `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn zhbo_client_rts_icons(general: u32, out: *mut crate::sim::RtsIcons) {
    if out.is_null() {
        return;
    }
    *out = with_state(|s| s.sim.rts_icons(general)).unwrap_or_default();
}

/// One command-bar class button (zhbo_core::classicons): which class it carries, where it sits in
/// the one-page class command set, its label, its kit's converted icon names, and (for a custom
/// class) the pick message's wire words. The bridge caches these at game start and draws the icons
/// over the button's rect.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ClassButton {
    /// Non-zero when a class occupies this button; otherwise the button is unused.
    pub present: i32,
    /// The command set's page, always 1 (the active set fits one page; kept for layout stability).
    pub page: i32,
    /// The command set's index: 0 is Take Control, 1..=10 the active set's classes.
    pub slot: i32,
    /// The button's label, UTF-8 NUL-terminated (also served to the engine by `strings.rs`).
    pub name: [u8; 48],
    /// The primary weapon's converted HUD icon (`ZHBO_BO2_Icon_<id>`).
    pub primary: [u8; 48],
    /// The secondary weapon's icon.
    pub secondary: [u8; 48],
    /// The lethal equipment's icon.
    pub lethal: [u8; 48],
    /// The tactical equipment's icon.
    pub tactical: [u8; 48],
    /// The perk icons, in pick order (at most six: three tiers plus Greed picks).
    pub perks: [[u8; 40]; 6],
    pub perk_count: i32,
    /// A custom class's wire words (`classes.rs`); zero for a shipped preset (picked by index).
    pub payload_len: i32,
    pub payload: [u32; crate::classwire::PAYLOAD_WORDS],
}

impl Default for ClassButton {
    fn default() -> Self {
        ClassButton {
            present: 0,
            page: 0,
            slot: 0,
            name: [0; 48],
            primary: [0; 48],
            secondary: [0; 48],
            lethal: [0; 48],
            tactical: [0; 48],
            perks: [[0; 40]; 6],
            perk_count: 0,
            payload_len: 0,
            payload: [0; crate::classwire::PAYLOAD_WORDS],
        }
    }
}

/// Copy `text` into a fixed C buffer, NUL-terminated and truncated on a byte boundary.
fn put_text(dst: &mut [u8], text: &str) {
    let bytes = text.as_bytes();
    let n = bytes.len().min(dst.len().saturating_sub(1));
    dst[..n].copy_from_slice(&bytes[..n]);
    if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

/// Fill `out` with the command bar's class buttons (`zhbo_core::classicons::icons`/`labels`);
/// returns how many were written (the class list's 15 slots, present or not).
///
/// # Safety
/// `out` must be writable for `cap` `ClassButton`s.
#[no_mangle]
pub unsafe extern "C" fn zhbo_class_buttons(out: *mut ClassButton, cap: i32) -> i32 {
    if out.is_null() || cap <= 0 {
        return 0;
    }
    let icons = crate::classicons::icons();
    let labels = crate::classicons::labels();
    let presets = icons.iter().take(crate::classicons::preset_rows()).filter(|c| c.is_some()).count();
    let customs = icons.iter().skip(crate::classicons::preset_rows()).filter(|c| c.is_some()).count();
    let n = icons.len().min(cap as usize);
    for i in 0..n {
        let mut b = ClassButton::default();
        if let Some(c) = &icons[i] {
            b.present = 1;
            if let Some((page, slot)) = crate::classicons::button_page_for(i, presets, customs) {
                b.page = page as i32;
                b.slot = slot as i32;
            }
            put_text(&mut b.name, labels[i].as_deref().unwrap_or(c.name.as_str()));
            put_text(&mut b.primary, &c.primary);
            put_text(&mut b.secondary, &c.secondary);
            put_text(&mut b.lethal, &c.lethal);
            put_text(&mut b.tactical, &c.tactical);
            let perks = c.perks.len().min(6);
            for k in 0..perks {
                put_text(&mut b.perks[k], c.perks[k]);
            }
            b.perk_count = perks as i32;
            if i >= crate::classicons::preset_rows() {
                // Never a truncated payload: it would not decode. Every class built from the
                // catalog fits (`classwire`); one that does not carries none.
                let words = crate::classicons::custom_payload(i);
                if words.len() <= b.payload.len() {
                    b.payload[..words.len()].copy_from_slice(&words);
                    b.payload_len = words.len() as i32;
                }
            }
        }
        *out.add(i) = b;
    }
    n as i32
}

/// Whether a General has assumed a class yet (the command bar shows the class set until he has).
#[no_mangle]
pub extern "C" fn zhbo_general_has_class(general: u32) -> i32 {
    with_state(|s| s.sim.players.iter().flatten().any(|p| p.general == general && p.class.is_some()) as i32).unwrap_or(0)
}

/// How a controlled General looks in third person (see `Sim::world_poses`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldPose {
    pub general: u32,
    /// Viewmodel index of the weapon in hand: its `W<vm><k>` meshes are shown.
    pub weapon: i32,
    /// 0 no tablet, 1 tablet coming up (`USER_1`), 2 going down (`USER_2`).
    pub tablet: i32,
    /// The held weapon's body animation set (`WeaponDef::body_anims`): 0 the rifleman's, else the
    /// W3D body's model condition `names::ANIM_SET_CONDITIONS[anim_set - 1]` (BO2's pistol, launcher,
    /// knife, shield, ... stance from `mp/playeranim.script`).
    pub anim_set: i32,
}

/// Fill `out` with up to `cap` poses; returns how many.
///
/// # Safety
/// `out` must be writable for `cap` entries.
#[no_mangle]
pub unsafe extern "C" fn zhbo_client_world_poses(out: *mut WorldPose, cap: i32) -> i32 {
    if out.is_null() || cap <= 0 {
        return 0;
    }
    let poses = with_state(|s| s.sim.world_poses()).unwrap_or_default();
    let n = poses.len().min(cap as usize);
    for (i, (general, weapon, tablet, anim_set)) in poses.into_iter().take(n).enumerate() {
        *out.add(i) = WorldPose { general, weapon, tablet, anim_set: anim_set as i32 };
    }
    n as i32
}

/// Fill `out` with up to `cap` native third-person poses (Track R7): the weapon in each General's
/// hand and the body clip his movement state picks. `local_moving` is the engine's answer for
/// `local_general` while he is under RTS control. Returns how many.
///
/// # Safety
/// `out` must be writable for `cap` entries.
#[no_mangle]
pub unsafe extern "C" fn zhbo_client_native_poses(out: *mut crate::nativebody::ffi::ZhboNativePose, cap: i32, local_general: u32, local_moving: i32) -> i32 {
    if out.is_null() || cap <= 0 {
        return 0;
    }
    let poses = with_state(|s| s.sim.native_poses((local_general != 0).then_some((local_general, local_moving != 0)), s.client.first_person_general())).unwrap_or_default();
    let n = poses.len().min(cap as usize);
    for (i, p) in poses.into_iter().take(n).enumerate() {
        *out.add(i) = crate::nativebody::ffi::ZhboNativePose { general: p.general, weapon: p.weapon, stance: p.stance, state: p.state as i32, time: p.time, rate: p.rate, sidearm: p.sidearm as i32 };
    }
    n as i32
}

/// The class camo option General `general`'s weapon `vm` wears in third person (the sim's
/// [`crate::sim::Sim::third_person_camo`]); `None` before a game or for any other weapon.
pub(crate) fn third_person_camo(general: u32, vm: i32) -> Option<String> {
    with_state(|s| s.sim.third_person_camo(general, vm)).flatten()
}

/// Third-person meshes of viewmodel `vm`'s weapon on the soldier model, or of the tablet for -2.
#[no_mangle]
pub extern "C" fn zhbo_world_mesh_count(vm: i32) -> i32 {
    let c = crate::weapons::catalog();
    if vm == -2 {
        return c.tablet_meshes as i32;
    }
    c.all().iter().filter(|d| d.viewmodel == vm).map(|d| d.world_meshes as i32).max().unwrap_or(0)
}

/// The viewmodels of the General's kit, so the host can load them before they are first shown.
/// Writes up to `cap` indices to `out`; returns how many there are.
///
/// # Safety
/// `out` must point to `cap` writable `i32`s (or be null with `cap` 0).
#[no_mangle]
pub unsafe extern "C" fn zhbo_loadout_viewmodels(out: *mut i32, cap: i32) -> i32 {
    let vms = crate::weapons::Loadout::default().viewmodels();
    if !out.is_null() {
        for (i, vm) in vms.iter().take(cap.max(0) as usize).enumerate() {
            *out.add(i) = *vm;
        }
    }
    vms.len() as i32
}

/// Write `text` as a NUL-terminated string into `dst` (truncating to `cap - 1` bytes).
///
/// # Safety
/// `dst` must point to `cap` writable bytes, or be null.
unsafe fn write_c_str(dst: *mut c_char, cap: i32, text: &str) {
    if dst.is_null() || cap <= 0 {
        return;
    }
    let n = text.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(text.as_ptr(), dst.cast::<u8>(), n);
    *dst.add(n) = 0;
}

/// The fingerprint [`zhbo_content_crc`] last computed: what a remote's parts are compared with.
static CONTENT: std::sync::Mutex<Option<crate::content::ContentCrc>> = std::sync::Mutex::new(None);

/// The multiplayer content fingerprint ([`crate::content::fingerprint`]) over Zero Hour's own INI
/// CRC `engine_ini`, after the weapon table, the tuning pack and the classes are loaded. Fills
/// `out` (when not null) with each part and the total, and returns the total: the bridge makes it
/// `m_iniCRC`, the number the lobbies and replays compare.
///
/// # Safety
/// `out` must be null or point to a writable `ContentCrc`.
#[no_mangle]
pub unsafe extern "C" fn zhbo_content_crc(engine_ini: u32, out: *mut crate::content::ContentCrc) -> u32 {
    let c = crate::content::fingerprint(engine_ini);
    *CONTENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
    if !out.is_null() {
        *out = c;
    }
    c.total
}

/// The lobby line for this machine's fingerprint (`LAN:ZHBO_ContentCRC`: "Conquer Ops content
/// 1A2B3C4D") into `out` (UTF-8, NUL-terminated); returns its length, or -1 before
/// [`zhbo_content_crc`] ran.
///
/// # Safety
/// `out` must point to `cap` writable bytes, or be null.
#[no_mangle]
pub unsafe extern "C" fn zhbo_content_label(out: *mut c_char, cap: i32) -> i32 {
    let Some(c) = *CONTENT.lock().unwrap_or_else(|e| e.into_inner()) else { return -1 };
    let mut text = crate::strings::text("LAN:ZHBO_ContentCRC").replace("%s", &crate::content::short(c.total));
    let dlc = crate::weapons::catalog().installed_dlc();
    if !dlc.is_empty() {
        text.push(' ');
        text.push_str(&crate::strings::text("LAN:ZHBO_ContentDlc").replace("%s", &dlc.join(" ")));
    }
    write_c_str(out, cap, &text);
    text.len() as i32
}

/// The message for a join the content check refused (`LAN:ZHBO_ContentMismatch`, naming the parts
/// that differ) from the other machine's `PART_COUNT` parts (`theirs`; null or all zero for a
/// build that sends none). Writes UTF-8 into `out`; returns its length, or -1 before
/// [`zhbo_content_crc`] ran.
///
/// # Safety
/// `theirs` must be null or point to `PART_COUNT` readable words; `out` to `cap` writable bytes,
/// or be null.
#[no_mangle]
pub unsafe extern "C" fn zhbo_content_mismatch(theirs: *const u32, out: *mut c_char, cap: i32) -> i32 {
    let Some(c) = *CONTENT.lock().unwrap_or_else(|e| e.into_inner()) else { return -1 };
    let mut remote = [0u32; crate::content::PART_COUNT];
    if !theirs.is_null() {
        remote.copy_from_slice(std::slice::from_raw_parts(theirs, crate::content::PART_COUNT));
    }
    let text = crate::content::describe_mismatch(&c.parts, &remote);
    write_c_str(out, cap, &text);
    text.len() as i32
}

/// Online, where a staging room carries only the total: both totals in short form
/// (`LAN:ZHBO_ContentVersus`) into `out` (UTF-8); returns its length, or -1 before
/// [`zhbo_content_crc`] ran.
///
/// # Safety
/// `out` must point to `cap` writable bytes, or be null.
#[no_mangle]
pub unsafe extern "C" fn zhbo_content_versus(theirs: u32, out: *mut c_char, cap: i32) -> i32 {
    let Some(c) = *CONTENT.lock().unwrap_or_else(|e| e.into_inner()) else { return -1 };
    let text = crate::strings::text("LAN:ZHBO_ContentVersus").replacen("%s", &crate::content::short(c.total), 1).replacen("%s", &crate::content::short(theirs), 1);
    write_c_str(out, cap, &text);
    text.len() as i32
}

/// The nearest the first-person hammer places a structure whose footprint has bounding radius
/// `radius` (`building::place_near`): the in-game harness lays its `--build` test out with it
/// instead of copying the numbers. ABI 30.
#[no_mangle]
pub extern "C" fn zhbo_hammer_place_near(radius: f32) -> f32 {
    crate::building::place_near(radius)
}

/// The first-person FXList names for the weapon on viewmodel `vm`: its muzzle flash (`viewFlashEffect`)
/// and shell eject (`viewShellEjectEffect`). Returns 0 if no weapon uses that viewmodel.
///
/// # Safety
/// `flash`/`shell` must point to `flash_cap`/`shell_cap` writable bytes (or be null).
#[no_mangle]
pub unsafe extern "C" fn zhbo_viewmodel_fx(vm: i32, flash: *mut c_char, flash_cap: i32, shell: *mut c_char, shell_cap: i32) -> i32 {
    let Some(d) = crate::weapons::catalog().all().iter().find(|d| d.viewmodel == vm) else { return 0 };
    unsafe {
        write_c_str(flash, flash_cap, &d.view_flash_fx);
        write_c_str(shell, shell_cap, &d.view_shell_fx);
    }
    1
}

/// The `i`-th tracer of the local player's own gun this rendered frame (BO2 draws them on the same
/// countdown as everyone's, from the viewmodel's flash tag: `FUN_00535f80`): the FX weapon whose
/// `FireFX` draws it, the bullet's end, and whether a dual-wield pair's left gun fired it (`left`
/// 1: start it at the `_lh` model's `tag_flash1`, BO2's left `viewmodelFx` tag). Returns 0 past the
/// last one.
///
/// # Safety
/// `fx_weapon` must point to `cap` writable bytes (or be null); `end` and `left` must be writable or
/// null.
#[no_mangle]
pub unsafe extern "C" fn zhbo_viewmodel_tracer(i: i32, fx_weapon: *mut c_char, cap: i32, end: *mut crate::math::Vec3, left: *mut i32) -> i32 {
    let Some(t) = with_state(|s| usize::try_from(i).ok().and_then(|i| s.client.tracers.frame.get(i).copied())).flatten() else { return 0 };
    unsafe {
        write_c_str(fx_weapon, cap, t.fx_weapon);
        if !end.is_null() {
            *end = t.end;
        }
        if !left.is_null() {
            *left = t.left as i32;
        }
    }
    1
}

/// The FXList BO2 plays for a shot of `impact_type` on `surface` (a `ZHBO_SURF_*`), for a hit that
/// was `head`/`fatal` (the flesh variants). Returns 1 when the table has a cell (the name may be
/// empty when nothing was converted for it), 0 when it has no row at all. The bridge plays the
/// name; the core owns the table, so no FXList name is written in C++.
///
/// # Safety
/// `out` must point to `cap` writable bytes, or be null.
#[no_mangle]
pub unsafe extern "C" fn zhbo_impact_fx(impact_type: i32, surface: i32, head: i32, fatal: i32, out: *mut c_char, cap: i32) -> i32 {
    let surface = crate::impacts::Surface::from_i32(surface);
    let Some(name) = crate::impacts::fx_name(impact_type, surface, head != 0, fatal != 0) else { return 0 };
    unsafe { write_c_str(out, cap, &name) };
    1
}

/// [`zhbo_impact_fx`] for a penetrating round leaving a solid: the type's exit row (BO2's impact
/// flag 4, `FUN_00533210`), with the same fallbacks.
///
/// # Safety
/// `out` must point to `cap` writable bytes, or be null.
#[no_mangle]
pub unsafe extern "C" fn zhbo_impact_exit_fx(impact_type: i32, surface: i32, head: i32, fatal: i32, out: *mut c_char, cap: i32) -> i32 {
    let surface = crate::impacts::Surface::from_i32(surface);
    let Some(name) = crate::impacts::exit_fx_name(impact_type, surface, head != 0, fatal != 0) else { return 0 };
    unsafe { write_c_str(out, cap, &name) };
    1
}

/// How fast a General builds, as a share of a dozer's speed.
#[no_mangle]
pub extern "C" fn zhbo_general_build_rate() -> f32 {
    crate::building::GENERAL_BUILD_RATE
}

/// Non-zero if the object's own AI/physics must be suspended this frame.
#[no_mangle]
pub extern "C" fn zhbo_logic_is_controlled(object_id: u32) -> i32 {
    if object_id == 0 {
        return 0;
    }
    CONTROLLED.iter().any(|s| s.load(Ordering::Relaxed) == object_id) as i32
}

/// # Safety
/// `input` must be valid; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn zhbo_client_frame(input: *const ClientIn, out: *mut ClientOut) {
    if input.is_null() || out.is_null() {
        return;
    }
    let inp = *input;
    let result = with_state(|s| {
        // The engine's logic is at hand (read only): the local first person is predicted.
        let out = s.client.frame_with(&mut s.sim, Some(&s.logic), &inp);
        #[cfg(feature = "autopilot")]
        if let Some(ap) = s.client.autopilot.as_mut() {
            use crate::host::LogicHost;
            for line in ap.log.drain(..) {
                s.logic.log(&line);
            }
        }
        for r in std::mem::take(&mut s.client.cues.queue) {
            if r.gain == 1.0 {
                s.client_host.sound(r.event, r.at);
            } else {
                s.client_host.sound_gain(r.event, r.at, r.gain);
            }
        }
        for (event, gain) in s.client.cues.loops.drain(..) {
            s.client_host.sound_loop(event, gain);
        }
        // The seat this frame presented: the local player's, or the General a spectator follows.
        let viewed = s.client.viewed;
        if let Some(line) = s.client.spectator_log(&s.sim, inp.logic_frame) {
            crate::host::LogicHost::log(&s.logic, &line);
        }
        // The local player's class camos, for the native viewmodel's next draw: each held weapon
        // wears its own (`_class.gsc`: the primary `primarycamo`, the sidearm `secondarycamo`, the
        // melee weapon `knifecamo`).
        let choices = s.sim.player(viewed).map_or_else(Vec::new, |p| crate::nativevm::class_camos(&p.held_camos(), &p.arsenal.current().def.id));
        crate::nativevm::set_local_camos(choices);
        // The viewmodel BO2 does not draw this frame: its scope overlay's reticle zoom is up.
        let hidden = s.client.hud.lui.gun_under_overlay.then(|| s.sim.player(viewed).map(|p| p.arsenal.current_weapon().viewmodel)).flatten();
        crate::nativevm::set_gun_under_overlay(hidden);
        // And the class's reticles for the optic lenses: the primary's and the sidearm's
        // (`primaryreticle` / `secondaryreticle`).
        let reticles = s.sim.player(viewed).map_or_else(Vec::new, |p| crate::nativevm::class_reticles(&p.held_reticles()));
        crate::nativevm::set_local_reticle(reticles);
        // And what the optic displays show (the MMS's characters, the target finder's colour).
        let optics = s.sim.player(viewed).map(|p| (p.optics.alt, p.optics.blips.clone()));
        crate::nativevm::update_local_optics(inp.dt, optics);
        // And its shellshock's screen layers (blur, grab, white) for the scene.
        let shock = s.sim.player(viewed).filter(|p| p.is_first_person()).and_then(|p| {
            let a = p.shock.active.as_ref()?;
            let sc = p.shock.screen();
            Some(crate::nativevm::LocalShock { serial: a.serial, flashed: a.def.flashed, white: sc.white, shot: sc.shot, blur_alpha: crate::shellshock::blur_alpha(sc.blur, inp.dt) })
        });
        crate::nativevm::set_local_shock(shock);
        out
    })
    .unwrap_or_default();
    *out = result;
}

#[no_mangle]
pub extern "C" fn zhbo_client_draw_hud(local_player: i32) {
    with_state(|s| {
        let mut prims = Vec::new();
        // A spectator's HUD is the followed General's (BO2 shows the spectated player's own).
        let seat = if s.client.spectator.is_some() { s.client.viewed } else { local_player };
        crate::hud::draw(&s.client_host, &mut prims, &s.client, &s.sim, seat);
        crate::hud::render(&prims, &s.client_host);
    });
}

// ------------------------------------------------------------------------------------------------
// The shell create-a-class menu (the lobby's class screen; docs/CONTRACT.md's menu ABI). The
// bridge's shell window calls these from its open/draw/close points; the screen draws through the
// stored client (HUD) callbacks and its sounds through the stored sound callback, both valid
// during the window's draw.
// ------------------------------------------------------------------------------------------------

/// Open the shell class screen (the lobby's Create a Class button): a fresh `MenuState` (any
/// screen already up is replaced).
#[no_mangle]
pub extern "C" fn zhbo_menu_open() {
    with_state(|s| s.menu = Some(crate::menu::MenuState::new()));
}

/// Close the shell class screen: drop its state. The bridge calls this when its window goes away.
#[no_mangle]
pub extern "C" fn zhbo_menu_close() {
    with_state(|s| s.menu = None);
}

/// One frame of the shell class screen: fold the input, play the queued cues, draw the prims. When
/// no screen is open the frame reports `closed` so a stray window closes instead of spinning.
///
/// # Safety
/// `input` must be valid; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn zhbo_menu_frame(input: *const crate::menu::MenuIn, out: *mut crate::menu::MenuOut) {
    if input.is_null() || out.is_null() {
        return;
    }
    let inp = *input;
    // With no state (or no screen open) the frame reports closed: a stray window pops instead of
    // spinning on a black screen.
    let closed = crate::menu::MenuOut { closed: 1, ..Default::default() };
    let result = with_state(|s| {
        let Some(menu) = s.menu.as_mut() else {
            return closed;
        };
        let out = menu.frame(&inp);
        for cue in menu.cues.drain(..) {
            s.client_host.sound(cue, None);
        }
        let mut prims = Vec::new();
        crate::classui::draw_screen(&mut prims, inp.screen_w, inp.screen_h, &menu.view());
        // The shell's `S`/`O` dialogs draw over the screen (BO2's popup panels).
        if let Some(dialog) = menu.dialog() {
            crate::classui::draw_dialog(&mut prims, inp.screen_w, inp.screen_h, dialog, menu.dialog_open());
        }
        crate::hud::render(&prims, &s.client_host);
        out
    })
    .unwrap_or(closed);
    *out = result;
}

/// Both games must be installed. Returns 1 if so; otherwise 0 and writes a NUL-terminated
/// explanation (UTF-8) into `msg` (up to `cap` bytes).
///
/// # Safety
/// `msg` must be writable for `cap` bytes (or null with `cap` 0).
#[no_mangle]
pub unsafe extern "C" fn zhbo_check_requirements(msg: *mut c_char, cap: u32) -> i32 {
    let req = zhbo_requirements::check_requirements();
    if req.ok() {
        return 1;
    }
    if !msg.is_null() && cap > 0 {
        let text = req.problems();
        let n = text.len().min(cap as usize - 1);
        std::ptr::copy_nonoverlapping(text.as_ptr(), msg.cast::<u8>(), n);
        *msg.add(n) = 0;
    }
    0
}

/// UTF-16, NUL-terminated text for a mod string label, or null if the label is not ours.
///
/// # Safety
/// `label` must be a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn zhbo_string(label: *const c_char) -> *const u16 {
    if label.is_null() {
        return std::ptr::null();
    }
    let Ok(label) = CStr::from_ptr(label).to_str() else { return std::ptr::null() };
    let text = crate::strings::lookup(label);
    {
        // One line per mod label when the answer changes: whether the core served it. A label
        // logged `served` that still draws as "???" is not a lookup miss — the text left the core
        // as valid UTF-16, so the fault is on the engine side of the ABI (see MODLOG); a `MISSED`
        // line names the label to add to the table. Engine labels that are not ours stay quiet.
        use std::collections::HashMap;
        use std::sync::Mutex;
        static SEEN: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
        if label.to_ascii_lowercase().contains("zhbo") {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            let served = text.is_some();
            let table = seen.get_or_insert_with(HashMap::new);
            let changed = match table.get_mut(label) {
                Some(previous) => std::mem::replace(previous, served) != served,
                None => {
                    table.insert(label.to_string(), served);
                    true
                }
            };
            if changed {
                if served {
                    eprintln!("zhbo: string {label:?} served");
                } else {
                    eprintln!("zhbo: string {label:?} MISSED (no entry in the mod's string table: the engine draws ???)");
                }
            }
        }
    }
    text.map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The global State and the per-frame mirrors it publishes (the shield mirror, the native
/// viewmodel's local camo/reticle/overlay transport, the third person's stowed weapons) are process-global: tests that drive the State
/// through the ABI, or read those mirrors, run one at a time under this lock. The native viewmodel
/// and third-person tests take it through their `with_state` (lock order: this, then that module's State).
#[cfg(test)]
pub(crate) fn state_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_before_init_are_harmless() {
        let _g = state_lock();
        zhbo_reset();
        zhbo_logic_step(1);
        assert_eq!(zhbo_logic_is_controlled(5), 0);
        let mut out = ClientOut::default();
        unsafe { zhbo_client_frame(&ClientIn::default(), &mut out) };
        assert_eq!(out.camera_override, 0);
        let s = unsafe { zhbo_string(c"OBJECT:ZHBO_General".as_ptr()) };
        assert!(!s.is_null());
        // The shell menu's entry points, too.
        zhbo_menu_open();
        let mut mout = crate::menu::MenuOut::default();
        unsafe { zhbo_menu_frame(&crate::menu::MenuIn::default(), &mut mout) };
        zhbo_menu_close();
    }

    /// The shell menu over the ABI: open, a frame (draws through all-None callbacks, guarded), and
    /// `zhbo_reset` drops the screen so no menu state leaks into a match.
    #[test]
    fn the_menu_opens_frames_and_reset_drops_it() {
        let _g = state_lock();
        // All-None callback tables: every C*Host method guards on None.
        // # Safety: both tables are plain structs of Option<fn> fields; zeroed is all-None.
        let logic: LogicApi = unsafe { std::mem::zeroed() };
        let client: ClientApi = unsafe { std::mem::zeroed() };
        unsafe { zhbo_init(&logic, &client) };

        // A frame with no menu open reports closed (a stray window would pop).
        let mut out = crate::menu::MenuOut::default();
        unsafe { zhbo_menu_frame(&crate::menu::MenuIn::default(), &mut out) };
        assert_eq!(out.closed, 1);

        zhbo_menu_open();
        let mut out = crate::menu::MenuOut::default();
        unsafe { zhbo_menu_frame(&crate::menu::MenuIn::default(), &mut out) };
        assert_eq!(out.closed, 0, "the screen is up");
        assert_eq!(out.save_profile, 0);

        zhbo_reset();
        let mut out = crate::menu::MenuOut::default();
        unsafe { zhbo_menu_frame(&crate::menu::MenuIn::default(), &mut out) };
        assert_eq!(out.closed, 1, "reset dropped the screen");
        zhbo_menu_close();
    }

    #[test]
    fn abi_fields_report_both_tables() {
        // host/zhbo/zhbo_core.h asserts the same counts; Zhbo_Init unpacks both words.
        assert_eq!(zhbo_abi_fields() & 0xFFFF_FFFF, LOGIC_API_FIELDS);
        assert_eq!(zhbo_abi_fields() >> 32, CLIENT_API_FIELDS);
        assert_eq!(std::mem::size_of::<crate::host::LogicApi>(), (LOGIC_API_FIELDS as usize) * std::mem::size_of::<usize>());
        assert_eq!(std::mem::size_of::<crate::host::ClientApi>(), (CLIENT_API_FIELDS as usize) * std::mem::size_of::<usize>());
    }

    #[test]
    fn viewmodel_fx_leaves_the_buffers_alone_when_the_viewmodel_is_unknown() {
        let mut flash = [1u8; 8];
        let mut shell = [1u8; 8];
        let found = unsafe { zhbo_viewmodel_fx(9999, flash.as_mut_ptr().cast(), 8, shell.as_mut_ptr().cast(), 8) };
        assert_eq!(found, 0);
        assert_eq!(flash[0], 1, "no weapon writes nothing");
        assert_eq!(shell[0], 1);
    }

    #[test]
    fn impact_fx_resolves_through_the_converted_table() {
        // The impact table is process-global and the sim tests read it in parallel, so this test
        // installs the same text they do (`impacts::TEST_TABLE`), through the ABI.
        let c = std::ffi::CString::new(crate::impacts::TEST_TABLE).unwrap();
        unsafe { zhbo_load_impacts(c.as_ptr()) };

        let mut buf = [0u8; 64];
        let n = unsafe { zhbo_impact_fx(77, crate::impacts::Surface::Concrete as i32, 0, 0, buf.as_mut_ptr().cast(), 64) };
        assert_eq!(n, 1);
        assert_eq!(ctext(&buf), "FX_C");

        // No cell for the surface: the bucket's default cell (BO2's SURF_TYPE_DEFAULT).
        let mut buf = [0u8; 64];
        unsafe { zhbo_impact_fx(77, crate::impacts::Surface::Metal as i32, 0, 0, buf.as_mut_ptr().cast(), 64) };
        assert_eq!(ctext(&buf), "FX_D");

        // Blood where a round leaves a body: the exit table's flesh row.
        let mut buf = [0u8; 64];
        let n = unsafe { zhbo_impact_exit_fx(78, crate::impacts::Surface::Flesh as i32, 0, 0, buf.as_mut_ptr().cast(), 64) };
        assert_eq!((n, ctext(&buf)), (1, "FX_EXIT_BLOOD"));

        // A bucket with no rows: 0, and nothing written.
        let mut buf = [1u8; 8];
        let n = unsafe { zhbo_impact_fx(9, crate::impacts::Surface::Concrete as i32, 0, 0, buf.as_mut_ptr().cast(), 8) };
        assert_eq!(n, 0);
        assert_eq!(buf[0], 1);
    }

    fn ctext(buf: &[u8]) -> &str {
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        std::str::from_utf8(&buf[..end]).unwrap()
    }

    #[test]
    fn class_buttons_describe_the_pages_and_carry_the_pick_payload() {
        let _g = crate::profile::lock();
        crate::profile::reset();
        let c = crate::classes::CustomClass {
            name: "My Sniper".into(),
            primary: "dsr50_mp+acog".into(),
            ..crate::classes::CustomClass::default()
        };
        crate::profile::save(c.clone());
        let mut out = [ClassButton::default(); crate::classicons::BUTTONS];
        let n = unsafe { zhbo_class_buttons(out.as_mut_ptr(), out.len() as i32) };
        assert_eq!(n as usize, crate::classicons::BUTTONS);
        // The preset buttons: page one, right after Take Control.
        assert_eq!(out[0].present, 1);
        assert_eq!((out[0].page, out[0].slot), (1, 1));
        assert!(!ctext(&out[0].name).is_empty(), "a preset is labelled");
        assert!(ctext(&out[0].primary).starts_with("ZHBO_BO2_Icon_"));
        assert!(out[0].payload_len == 0, "a preset picks by index");
        // The one custom class sits on page one's first custom slot, with its wire words.
        assert_eq!(out[crate::classicons::preset_rows()].present, 1);
        assert_eq!((out[crate::classicons::preset_rows()].page, out[crate::classicons::preset_rows()].slot), (1, 6));
        assert_eq!(ctext(&out[crate::classicons::preset_rows()].name), "My Sniper");
        assert_eq!(ctext(&out[crate::classicons::preset_rows()].primary), "ZHBO_BO2_Icon_dsr50_mp");
        let len = out[crate::classicons::preset_rows()].payload_len as usize;
        assert_eq!(crate::classes::CustomClass::from_wire_words(&out[crate::classicons::preset_rows()].payload[..len]), Some(c));
        // The empty custom slots carry no class.
        assert!(out[crate::classicons::preset_rows() + 1..].iter().all(|b| b.present == 0));
        crate::profile::reset();
    }

    /// The command bar's class button crosses the ABI: `ZhboClientIn.class_button` (1-based) makes
    /// `zhbo_client_frame` emit the pick, and a frame without it emits nothing. This is the half
    /// the bug lost: the bridge queued `choose_class` into the shared out struct and the next
    /// `zhbo_client_frame` wrote a fresh struct over it before the bridge could read the pick.
    #[test]
    fn a_command_bar_class_button_reaches_the_client_out_through_the_abi() {
        let _g = state_lock();
        let logic: LogicApi = unsafe { std::mem::zeroed() };
        let client: ClientApi = unsafe { std::mem::zeroed() };
        unsafe { zhbo_init(&logic, &client) };
        let mut out = ClientOut::default();
        let inp = ClientIn { class_button: 1, ..Default::default() };
        unsafe { zhbo_client_frame(&inp, &mut out) };
        assert_eq!(out.choose_class, 1, "button 1 is the first preset, 1-based");
        assert_eq!(out.choose_class_payload_len, 0);
        unsafe { zhbo_client_frame(&ClientIn::default(), &mut out) };
        assert_eq!(out.choose_class, 0, "the one-shot is consumed, not repeated");
        zhbo_reset();
    }

    #[test]
    fn general_has_class_follows_the_sim() {
        let _g = state_lock();
        let logic: LogicApi = unsafe { std::mem::zeroed() };
        let client: ClientApi = unsafe { std::mem::zeroed() };
        unsafe { zhbo_init(&logic, &client) };
        assert_eq!(zhbo_general_has_class(1), 0);
        let words = crate::classes::CustomClass { primary: "an94_mp".into(), ..crate::classes::CustomClass::default() }.to_wire_words();
        unsafe { zhbo_logic_choose_class(0, 1, i32::MIN, words.as_ptr(), words.len() as i32) };
        assert_eq!(zhbo_general_has_class(1), 1);
        assert_eq!(zhbo_general_has_class(2), 0, "another General has no class");
        zhbo_reset();
    }

    #[test]
    fn the_shield_hook_queues_a_deployed_shields_damage_and_passes_the_sims_own() {
        // The shield mirror is process-global and `zhbo_init`/`zhbo_logic_step`/`zhbo_reset`
        // republish it from the global State: run with the other State tests, not beside them.
        let _g = state_lock();
        let mut sim = Sim::new();
        let def = crate::weapons::catalog().get("riotshield_mp").unwrap();
        let carried = crate::shield::Bearer::of(5, crate::math::Vec3::new(0.0, 0.0, 0.0), 0.0, def, true).unwrap();
        sim.shields = vec![carried, crate::shield::Bearer { general: 0, object: 77, ..carried }];
        publish_shields(&sim);
        SHIELD_HIT_COUNT.store(0, Ordering::Relaxed);
        assert_eq!(zhbo_logic_shield_damage(77, 3, 0.0, 0.0, 0.0, 12.0, 1, crate::host::damage_kind::SMALL_ARMS, 30.0), 0.0, "the engine applies none of it");
        assert_eq!(SHIELD_HIT_COUNT.load(Ordering::Relaxed), 1, "queued for the sim");
        // A round from in front of General 5's held shield is stopped; from behind it is not.
        assert_eq!(zhbo_logic_shield_damage(5, 3, 60.0, 0.0, 0.0, 12.0, 1, crate::host::damage_kind::SMALL_ARMS, 30.0), 0.0);
        assert_eq!(zhbo_logic_shield_damage(5, 3, -60.0, 0.0, 0.0, 12.0, 1, crate::host::damage_kind::SMALL_ARMS, 30.0), 1.0);
        SHIELD_HIT_COUNT.store(0, Ordering::Relaxed);
        SHIELD_COUNT.store(0, Ordering::Relaxed);
    }

    /// The smoke hooks over the ABI (host patch 0024): the mirror of the sim's screens answers the
    /// engine's sight, ground and path questions, and empties when the screens clear.
    #[test]
    fn the_smoke_hooks_answer_from_the_sims_screens() {
        let _g = state_lock();
        let mut sim = Sim::new();
        sim.frame = 100;
        sim.smokes.push(crate::smoke::Smoke { at: crate::math::Vec3::new(500.0, 500.0, 0.0), born: 30 });
        publish_smokes(&sim);
        let r = sim.smoke_blockers()[0].radius;
        assert!((r - crate::units::inches(150.0)).abs() < 1e-3, "grown to its cap: {r}");
        // Sight: through it, beside it, and two units close together inside it.
        assert_eq!(zhbo_logic_smoke_blocks(300.0, 500.0, 5.0, 700.0, 500.0, 5.0), 1);
        assert_eq!(zhbo_logic_smoke_blocks(300.0, 600.0, 5.0, 700.0, 600.0, 5.0), 0);
        assert_eq!(zhbo_logic_smoke_blocks(495.0, 500.0, 5.0, 505.0, 500.0, 5.0), 0, "closer than 80 inches");
        // The ground under it, and a path computed before or after it went off.
        assert_eq!((zhbo_logic_smoke_covers(510.0, 490.0), zhbo_logic_smoke_covers(560.0, 500.0)), (1, 0));
        assert_eq!(zhbo_logic_smoke_crosses(300.0, 500.0, 700.0, 500.0, 29), 1);
        assert_eq!(zhbo_logic_smoke_crosses(300.0, 500.0, 700.0, 500.0, 30), 0);
        // Cleared: no screen, no answers.
        sim.frame = 30 + sim.smoke.frames;
        publish_smokes(&sim);
        assert_eq!((zhbo_logic_smoke_blocks(300.0, 500.0, 5.0, 700.0, 500.0, 5.0), zhbo_logic_smoke_covers(500.0, 500.0)), (0, 0));
    }

    /// The CRC hook over the ABI: an idle sim adds nothing (the CRC stays stock Zero Hour's), a class
    /// pick makes it active, and the reported hash and seats are the sim's own fingerprint.
    #[test]
    fn the_logic_crc_reports_the_sims_fingerprint_once_it_holds_state() {
        let _g = state_lock();
        let logic: LogicApi = unsafe { std::mem::zeroed() };
        let client: ClientApi = unsafe { std::mem::zeroed() };
        unsafe { zhbo_init(&logic, &client) };
        let mut out = LogicCrc { active: 7, ..Default::default() };
        assert_eq!(unsafe { zhbo_logic_crc(&mut out, 1) }, 0, "idle");
        assert_eq!(out.active, 0);
        let words = crate::classes::CustomClass { primary: "an94_mp".into(), ..crate::classes::CustomClass::default() }.to_wire_words();
        unsafe { zhbo_logic_choose_class(2, 1, i32::MIN, words.as_ptr(), words.len() as i32) };
        zhbo_logic_step(5);
        assert_eq!(unsafe { zhbo_logic_crc(&mut out, 1) }, 1, "a seat holds a class");
        let f = with_state(|s| s.sim.fingerprint()).unwrap();
        assert_eq!((out.hash, out.world, out.frame, out.active), (f.hash, f.world, 5, 1));
        assert_eq!(out.hash, with_state(|s| s.sim.state_hash()).unwrap(), "the lockstep gate's hash");
        assert!(out.seats[2] != 0 && out.seats.iter().enumerate().all(|(i, &s)| (s != 0) == (i == 2)), "{:x?}", out.seats);
        assert!(crc_line(&f).ends_with(&format!("seats 2:{:016x}", out.seats[2])));
        assert_eq!(unsafe { zhbo_logic_crc(std::ptr::null_mut(), 0) }, 1, "null out is allowed");
        zhbo_reset();
        assert_eq!(unsafe { zhbo_logic_crc(&mut out, 0) }, 0, "a new game starts idle");
    }

    /// Every `zhbo_*` function the core exports is declared in `host/zhbo/zhbo_core.h`, and every
    /// one the header declares is exported: a function on one side only is a broken ABI the
    /// compiler cannot see (the bridge links against the header).
    #[test]
    fn the_c_header_declares_exactly_the_exported_functions() {
        fn exported(dir: &std::path::Path, out: &mut std::collections::BTreeSet<String>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    exported(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    for rest in text.split("extern \"C\" fn ").skip(1) {
                        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                        if name.starts_with("zhbo_") {
                            out.insert(name);
                        }
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut rust = std::collections::BTreeSet::new();
        exported(&root.join("src"), &mut rust);
        let header = std::fs::read_to_string(root.join("../../host/zhbo/zhbo_core.h")).expect("host/zhbo/zhbo_core.h is checked in");
        // A declaration is a top-level line naming `zhbo_<name>(` (not a comment, macro or typedef).
        let declared: std::collections::BTreeSet<String> = header
            .lines()
            .filter(|l| !l.starts_with([' ', '\t', '#', '/', '*']) && !l.starts_with("typedef"))
            .flat_map(|l| l.match_indices("zhbo_").map(move |(i, _)| &l[i..]))
            .filter_map(|r| {
                let name: String = r.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                r[name.len()..].starts_with('(').then_some(name)
            })
            .collect();
        let missing: Vec<_> = rust.difference(&declared).collect();
        let unexported: Vec<_> = declared.difference(&rust).collect();
        assert!(missing.is_empty() && unexported.is_empty(), "exported but not in zhbo_core.h: {missing:?}; declared but not exported: {unexported:?}");
        assert!(rust.len() > 50, "the scan found the exports ({})", rust.len());
    }
}
