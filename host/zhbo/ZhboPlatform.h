// Conquer Ops: the few OS services the bridge needs (window focus, raw keys, mouse capture and
// deltas, a timer, an error box, the HUD font). SDL3 builds (64-bit Windows, macOS, Linux) use
// SDL; the 32-bit DX8 Windows build uses Win32.
#pragma once

#include <stdint.h>

namespace zhbo_platform
{
	// Whether the game window has keyboard focus.
	bool windowFocused();

	// Fill keys[256], indexed by Windows virtual-key code (the Rust core's key vocabulary,
	// `zhbo_core::client::vk`): 0x01/0x02 mouse buttons, 0x10 Shift, 0x11 Ctrl, 0x20 Space,
	// '0'-'9', 'A'-'Z'. Non-zero means held.
	void readKeys(uint8_t keys[256]);

	// First-person mouse look: capture hides and confines the cursor; deltas are in pixels since
	// the previous call (zero when not captured).
	void setMouseCapture(bool capture);
	void mouseDelta(float &dx, float &dy);

	// Whether the system cursor is over the game window's client area.
	bool cursorOverWindow();

	// Seconds, monotonic.
	double now();

	void errorBox(const char *title, const char *text);

	// End the process at once, without running the engine's shutdown.
	[[noreturn]] void exitProcess(int code);

	// A narrow DIN-like face for the BO2 HUD that exists on this system.
	const char *hudFontName();
}
