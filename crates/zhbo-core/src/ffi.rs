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

pub const ABI_VERSION: u32 = 12;

// Layouts shared with host/zhbo/zhbo_core.h (which asserts the same sizes on the C++ side).
const _: () = {
    use std::mem::size_of;
    assert!(size_of::<crate::host::ObjInfo>() == 56);
    assert!(size_of::<FpsInput>() == 20);
    assert!(size_of::<ClientIn>() == 320);
    assert!(size_of::<ClientOut>() == 132);
    assert!(size_of::<crate::sim::RtsWeapons>() == 28);
    assert!(size_of::<LogicApi>() == 17 * size_of::<usize>());
    assert!(size_of::<ClientApi>() == 10 * size_of::<usize>());
};

/// Returns the ABI version; the glue refuses to run if it does not match its header.
#[no_mangle]
pub extern "C" fn zhbo_abi_version() -> u32 {
    ABI_VERSION
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
        sim: Sim::new(),
        client: Client::default(),
    };
    use crate::host::LogicHost;
    state.logic.log(&format!("zhbo-core {} initialised (ABI {ABI_VERSION})", env!("CARGO_PKG_VERSION")));
    // ZHBO_AUTOPILOT=1 scripts the first-person input; =manual only sets up the test scene (the
    // bridge spawns the General and takes control) and leaves input to the player or a test driver.
    match std::env::var("ZHBO_AUTOPILOT").ok().as_deref() {
        None => {}
        Some("manual") => state.logic.log("autopilot scene enabled, manual input (ZHBO_AUTOPILOT=manual)"),
        Some(mode) => {
            state.client.autopilot = Some(crate::client::Autopilot { equipment: mode == "equipment", ..Default::default() });
            state.logic.log("autopilot enabled (ZHBO_AUTOPILOT)");
        }
    }
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
}

/// New game, loaded game or return to the shell.
#[no_mangle]
pub extern "C" fn zhbo_reset() {
    with_state(|s| {
        s.sim.reset();
        s.client.reset();
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
    use crate::sim::EventKind;
    for e in s.sim.events.iter().skip(first_new_event) {
        let line = match e.kind {
            EventKind::EnteredFps { general } => format!("player {} took control of General {general}", e.player),
            EventKind::FieldComputerOpened => format!("player {} opened the field computer", e.player),
            EventKind::ReturnedToRts { general } => format!("player {} returned General {general} to RTS control", e.player),
            EventKind::GeneralDied { general } => format!("player {}'s General {general} died", e.player),
            EventKind::HitMarker { headshot, kill: true } => format!("player {} killed a target (headshot: {headshot})", e.player),
            EventKind::Damaged { amount } => format!("player {}'s General took {amount:.0} damage", e.player),
            EventKind::Swapped => format!("player {} swapped weapons", e.player),
            EventKind::Melee => format!("player {} meleed", e.player),
            EventKind::Thrown { lethal } => format!("player {} threw a {}", e.player, if lethal { "lethal" } else { "tactical" }),
            EventKind::Exploded { at } => format!("player {}'s grenade went off at ({:.0}, {:.0})", e.player, at.x, at.y),
            EventKind::Planted { kind } => format!("player {} put down equipment ({kind:?})", e.player),
            EventKind::Detonated => format!("player {} set off their C4", e.player),
            EventKind::Intercepted { .. } => format!("player {}'s Trophy System shot down a projectile", e.player),
            EventKind::DestroyedEquipment => format!("player {} destroyed enemy equipment", e.player),
            EventKind::Hacked => format!("player {} hacked with the Black Hat", e.player),
            EventKind::AltToggled => format!("player {} toggled the weapon's alt mode", e.player),
            _ => continue,
        };
        s.logic.log(&line);
    }
    if frame % 30 == 0 {
        for (i, p) in s.sim.players.iter().enumerate() {
            if let Some(p) = p.as_ref().filter(|p| p.is_first_person()) {
                let w = p.arsenal.current();
                s.logic.log(&format!(
                    "player {i} pos ({:.1}, {:.1}, {:.1}) yaw {:.2} pitch {:.2} stance {:?} hp {:.0}/{:.0} {} {}|{}",
                    p.body.pos.x, p.body.pos.y, p.body.pos.z, p.yaw, p.pitch, p.body.stance,
                    p.health, p.max_health, w.def.display_name, w.ammo, w.reserve
                ));
            }
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
pub unsafe extern "C" fn zhbo_logic_rts_update(player: i32, general: u32, current_slot: i32, shots: *const u32, out: *mut crate::sim::RtsWeapons) {
    if shots.is_null() || out.is_null() {
        return;
    }
    let shots = [*shots, *shots.add(1), *shots.add(2)];
    let r = with_state(|s| s.sim.rts_update(&s.logic, player, general, current_slot, shots)).unwrap_or_default();
    *out = r;
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

/// How a controlled General looks in third person (see `Sim::world_poses`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldPose {
    pub general: u32,
    /// Viewmodel index of the weapon in hand: its `W<vm><k>` meshes are shown.
    pub weapon: i32,
    /// 0 no tablet, 1 tablet coming up (`USER_1`), 2 going down (`USER_2`).
    pub tablet: i32,
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
    for (i, (general, weapon, tablet)) in poses.into_iter().take(n).enumerate() {
        *out.add(i) = WorldPose { general, weapon, tablet };
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
        crate::hud::draw(&s.client_host, &s.client, &s.sim, local_player);
    });
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
    crate::strings::lookup(label).map_or(std::ptr::null(), |s| s.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_before_init_are_harmless() {
        zhbo_reset();
        zhbo_logic_step(1);
        assert_eq!(zhbo_logic_is_controlled(5), 0);
        let mut out = ClientOut::default();
        unsafe { zhbo_client_frame(&ClientIn::default(), &mut out) };
        assert_eq!(out.camera_override, 0);
        let s = unsafe { zhbo_string(c"OBJECT:ZHBO_General".as_ptr()) };
        assert!(!s.is_null());
    }
}
