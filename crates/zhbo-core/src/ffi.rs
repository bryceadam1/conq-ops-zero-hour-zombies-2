//! C ABI called by the engine glue (`host/zhbo/ZhboBridge.cpp`). Declarations: `host/zhbo/zhbo_core.h`.
//!
//! All entry points run on the game's main thread; the mutex only guards against misuse. Callbacks
//! into the engine happen while the lock is held, so the engine must not call back into a
//! `zhbo_logic_*`/`zhbo_client_*` function from inside one; `zhbo_logic_is_controlled` and
//! `zhbo_string` are lock-free for exactly that reason.

use crate::client::{Client, ClientIn, ClientOut};
use crate::host::{CClientHost, CLogicHost, ClientApi, LogicApi};
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

pub const ABI_VERSION: u32 = 1;

/// Returns the ABI version; the glue refuses to run if it does not match its header.
#[no_mangle]
pub extern "C" fn zhbo_abi_version() -> u32 {
    ABI_VERSION
}

/// # Safety
/// `logic` and `client` must point to fully initialised tables that outlive the process.
#[no_mangle]
pub unsafe extern "C" fn zhbo_init(logic: *const LogicApi, client: *const ClientApi) {
    if logic.is_null() || client.is_null() {
        return;
    }
    let state = State {
        logic: CLogicHost(*logic),
        client_host: CClientHost(*client),
        sim: Sim::new(),
        client: Client::default(),
    };
    use crate::host::LogicHost;
    state.logic.log(&format!("zhbo-core {} initialised (ABI {ABI_VERSION})", env!("CARGO_PKG_VERSION")));
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
        s.sim.step(&s.logic, frame);
        publish_controlled(&s.sim);
    });
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
    let result = with_state(|s| s.client.frame(&mut s.sim, &inp)).unwrap_or_default();
    *out = result;
}

#[no_mangle]
pub extern "C" fn zhbo_client_draw_hud(local_player: i32) {
    with_state(|s| {
        let player = s.sim.player(local_player);
        crate::hud::draw(&s.client_host, &s.client, player);
    });
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
