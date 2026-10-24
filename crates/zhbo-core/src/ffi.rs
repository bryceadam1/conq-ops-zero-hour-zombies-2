//! C ABI called by the engine glue (`host/zhbo/ZhboBridge.cpp`). Declarations: `host/zhbo/zhbo_core.h`.
//!
//! All entry points run on the game's main thread; the mutex only guards against misuse. Callbacks
//! into the engine happen while the lock is held, so the engine must not call back into a
//! `zhbo_logic_*`/`zhbo_client_*` function from inside one; `zhbo_logic_is_controlled` and
//! `zhbo_string` are lock-free for exactly that reason.

use crate::client::{Client, ClientIn, ClientOut};
use crate::host::{CClientHost, CLogicHost, ClientApi, ClientHost, LogicApi};
use crate::input::FpsInput;
use crate::sim::{Sim, MAX_PLAYERS};
use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicU32, Ordering};
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

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_mut().map(f)
}

fn publish_controlled(sim: &Sim) {
    let mut ids = sim.controlled_ids();
    for slot in CONTROLLED.iter() {
        slot.store(ids.next().unwrap_or(0), Ordering::Relaxed);
    }
}

pub const ABI_VERSION: u32 = 25;

/// Field counts of the two API tables (`host.rs`'s `LogicApi`/`ClientApi`). `host/zhbo/zhbo_core.h`
/// defines the same numbers; both sides' layout asserts and `zhbo_abi_fields` derive from them.
pub const LOGIC_API_FIELDS: u64 = 23;
pub const CLIENT_API_FIELDS: u64 = 13;

// Layouts shared with host/zhbo/zhbo_core.h (which asserts the same sizes on the C++ side).
const _: () = {
    use std::mem::size_of;
    assert!(size_of::<crate::host::ObjInfo>() == 56);
    assert!(size_of::<FpsInput>() == 20);
    assert!(size_of::<ClientIn>() == 136);
    assert!(size_of::<ClientOut>() == 4508);
    assert!(size_of::<crate::sim::RtsWeapons>() == 192);
    assert!(size_of::<crate::sim::RtsIcons>() == 240);
    assert!(size_of::<crate::building::BuildOption>() == 64);
    assert!(size_of::<crate::building::Site>() == 64);
    assert!(size_of::<crate::menu::MenuIn>() == 56);
    assert!(size_of::<crate::menu::MenuOut>() == 4108);
    assert!(size_of::<ClassButton>() == 756);
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
#[no_mangle]
pub unsafe extern "C" fn zhbo_load_fonts(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy() };
    crate::lui::font::load(&text);
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
/// write, or null when the words don't decode.
///
/// # Safety
/// `words` must point to `len` `u32`s (or be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn zhbo_profile_write(words: *const u32, len: i32) -> *const c_char {
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
        crate::config::Autopilot::Scripted { equipment, build, classpick, scope } => {
            #[cfg(feature = "autopilot")]
            {
                state.client.autopilot = Some(crate::client::Autopilot { equipment, build, classpick, scope, ..Default::default() });
            }
            let _ = (equipment, build, classpick, scope);
            state.logic.log("autopilot enabled (ZHBO_AUTOPILOT)");
        }
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
    });
}

#[no_mangle]
pub extern "C" fn zhbo_logic_take_control(player: i32, general: u32) {
    with_state(|s| {
        s.sim.take_control(&s.logic, player, general);
        publish_controlled(&s.sim);
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
        let before = s.sim.events.len();
        s.sim.choose_class(player, general, index, payload);
        for e in s.sim.events.iter().skip(before) {
            if let Some(line) = crate::sim::describe_event(e) {
                s.logic.log(&line);
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn zhbo_logic_step(frame: u32) {
    with_state(|s| {
        let before = s.sim.events.len();
        s.sim.step(&s.logic, frame);
        publish_controlled(&s.sim);
        log_step(s, before, frame);
    });
}

/// `ConquerOps.log` is the in-game oracle: lifecycle events, kills, and a once-per-second pose.
fn log_step(s: &State, first_new_event: usize, frame: u32) {
    use crate::host::LogicHost;
    log_events(s, first_new_event);
    if frame.is_multiple_of(30) {
        use crate::impacts::Surface;
        use crate::sim::EventKind;
        for (i, p) in s.sim.players.iter().enumerate() {
            if let Some(p) = p.as_ref().filter(|p| p.is_first_person()) {
                let w = p.arsenal.current();
                s.logic.log(&format!(
                    "player {i} pos ({:.1}, {:.1}, {:.1}) yaw {:.2} pitch {:.2} stance {:?} hp {:.0}/{:.0} {} {}|{}",
                    p.body.pos.x, p.body.pos.y, p.body.pos.z, p.yaw, p.pitch, p.body.stance,
                    p.health, p.max_health, w.def.display_name, w.ammo, w.reserve
                ));
                // R8d: the shots that landed this second, by surface — the in-game oracle for the
                // autotest (`player 0 impacts: 3 (flesh 1, concrete 2)`) and for eyeballing which
                // surface a map actually reports. Quiet while not shooting.
                let mut counts = [0u32; Surface::ALL.len()];
                let mut landed = 0u32;
                for e in s.sim.events.iter().skip(first_new_event).filter(|e| e.player as usize == i) {
                    if let EventKind::Impact { surface, .. } = e.kind {
                        counts[surface as usize] += 1;
                        landed += 1;
                    }
                }
                if landed > 0 {
                    let by: Vec<String> =
                        Surface::ALL.iter().zip(counts).filter(|(_, n)| *n > 0).map(|(s, n)| format!("{} {n}", s.name())).collect();
                    s.logic.log(&format!("player {i} impacts: {landed} ({})", by.join(", ")));
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
        let before = s.sim.events.len();
        let r = s.sim.rts_update(&s.logic, player, general, current_slot, shots, target);
        log_events(s, before);
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
    pub payload: [u32; 64],
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
            payload: [0; 64],
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
    let presets = icons.iter().take(crate::classicons::PRESETS).filter(|c| c.is_some()).count();
    let customs = icons.iter().skip(crate::classicons::PRESETS).filter(|c| c.is_some()).count();
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
            if i >= crate::classicons::PRESETS {
                let words = crate::classicons::custom_payload(i);
                let len = words.len().min(64);
                b.payload[..len].copy_from_slice(&words[..len]);
                b.payload_len = len as i32;
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
    /// The sidearm (second gun) is in hand: the soldier holds it with BO2's pistol stance.
    pub sidearm: i32,
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
    for (i, (general, weapon, tablet, sidearm)) in poses.into_iter().take(n).enumerate() {
        *out.add(i) = WorldPose { general, weapon, tablet, sidearm: sidearm as i32 };
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
    let poses = with_state(|s| s.sim.native_poses((local_general != 0).then_some((local_general, local_moving != 0)))).unwrap_or_default();
    let n = poses.len().min(cap as usize);
    for (i, p) in poses.into_iter().take(n).enumerate() {
        *out.add(i) = crate::nativebody::ffi::ZhboNativePose { general: p.general, weapon: p.weapon, stance: p.stance, state: p.state as i32, time: p.time, rate: p.rate, sidearm: p.sidearm as i32 };
    }
    n as i32
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
        let out = s.client.frame(&mut s.sim, &inp);
        for r in std::mem::take(&mut s.client.cues.queue) {
            s.client_host.sound(r.event, r.at);
        }
        out
    })
    .unwrap_or_default();
    *out = result;
}

#[no_mangle]
pub extern "C" fn zhbo_client_draw_hud(local_player: i32) {
    with_state(|s| {
        let mut prims = Vec::new();
        crate::hud::draw(&s.client_host, &mut prims, &s.client, &s.sim, local_player);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The global State is shared: the ffi tests that init it run one at a time.
    fn state_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
    }

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
        assert_eq!(out[crate::classicons::PRESETS].present, 1);
        assert_eq!((out[crate::classicons::PRESETS].page, out[crate::classicons::PRESETS].slot), (1, 6));
        assert_eq!(ctext(&out[crate::classicons::PRESETS].name), "My Sniper");
        assert_eq!(ctext(&out[crate::classicons::PRESETS].primary), "ZHBO_BO2_Icon_dsr50_mp");
        let len = out[crate::classicons::PRESETS].payload_len as usize;
        assert_eq!(crate::classes::CustomClass::from_wire_words(&out[crate::classicons::PRESETS].payload[..len]), Some(c));
        // The empty custom slots carry no class.
        assert!(out[crate::classicons::PRESETS + 1..].iter().all(|b| b.present == 0));
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
}
