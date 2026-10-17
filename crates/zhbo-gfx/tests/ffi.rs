//! The renderer's C ABI, called in-process (4.1): init, layout create, transient alloc
//! (including the overflow cases from 0.1b) and shutdown. One test drives the whole path with
//! valid pointers, like the C++ glue would; the log replaces the C trace callback.

use zhbo_gfx::state;
use zhbo_gfx::ffi::{zhgfx_init, zhgfx_register_layout, zhgfx_reset, zhgfx_set_callbacks, zhgfx_shutdown, zhgfx_transient_alloc, ZhgfxInit, ZhgfxLayout};

static LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

extern "C" fn ffi_trace(msg: *const std::os::raw::c_char) {
    let c = unsafe { std::ffi::CStr::from_ptr(msg) };
    LOG.lock().unwrap().push(c.to_string_lossy().into_owned());
}

#[test]
fn ffi_init_layout_transient_shutdown() {
    LOG.lock().unwrap().clear();
    unsafe {
        zhgfx_set_callbacks(Some(ffi_trace), None);
    }

    let init = ZhgfxInit {
        kind: 0, // headless
        window: std::ptr::null_mut(),
        display: std::ptr::null_mut(),
        width: 64,
        height: 64,
        reset: 0,
        transient_vb_size: 1 << 12,
        transient_ib_size: 1 << 12,
    };
    let up = unsafe { zhgfx_init(&init) };
    if !up {
        eprintln!("no usable adapter, skipping");
        return;
    }

    // A layout like the C++ glue builds: stride, per-attribute byte offsets and encodings
    // (bgfx's attribute encoding: (normalized << 7) | (type << 3) | (num - 1)).
    let enc = |num: u16, ty: u16, norm: u16| (norm << 7) | (ty << 3) | (num - 1);
    let mut offsets = [0u16; state::ATTRIB_COUNT];
    let mut attrs = [u16::MAX; state::ATTRIB_COUNT];
    offsets[0] = 0; attrs[0] = enc(3, 6, 0); // POSITION float3
    offsets[4] = 12; attrs[4] = enc(4, 1, 1); // COLOR0 uint8x4 normalized
    offsets[10] = 20; attrs[10] = enc(2, 6, 0); // TEXCOORD0 float2
    let layout = ZhgfxLayout { stride: 24, offset: offsets, attributes: attrs };
    let layout_id = unsafe { zhgfx_register_layout(&layout) };
    assert_ne!(layout_id, u16::MAX, "the layout registers");

    // Transient allocations work, and the hostiles from 0.1b are rejected cleanly. The vertex
    // and index arenas are separate, and each starts empty at offset 0.
    let mut vstart = u32::MAX;
    let vptr = unsafe { zhgfx_transient_alloc(false, 4, 24, std::ptr::addr_of_mut!(vstart)) };
    assert!(!vptr.is_null(), "vertex transient alloc");
    assert_eq!(vstart, 0, "a fresh vertex arena starts at byte 0");
    let mut istart = u32::MAX;
    let iptr = unsafe { zhgfx_transient_alloc(true, 6, 2, std::ptr::addr_of_mut!(istart)) };
    assert!(!iptr.is_null(), "index transient alloc");
    assert_eq!(istart, 0, "a fresh index arena starts at index 0");
    let mut istart2 = u32::MAX;
    let iptr2 = unsafe { zhgfx_transient_alloc(true, 6, 2, std::ptr::addr_of_mut!(istart2)) };
    assert!(!iptr2.is_null(), "a second index transient alloc");
    assert_ne!(istart2, istart, "the second index alloc's start advances in units of indices");
    let bad = unsafe { zhgfx_transient_alloc(false, u32::MAX, u32::MAX, std::ptr::addr_of_mut!(vstart)) };
    assert!(bad.is_null(), "the 0.1b overflow alloc is null");

    unsafe { zhgfx_reset(128, 128, 0) };
    unsafe { zhgfx_shutdown() };

    // The trace callback saw the renderer's startup line (the log path works).
    let lines = LOG.lock().unwrap();
    assert!(
        lines.iter().any(|l| l.contains("zhbo-gfx: renderer up on")),
        "the trace callback saw the startup log: {lines:?}"
    );
}
