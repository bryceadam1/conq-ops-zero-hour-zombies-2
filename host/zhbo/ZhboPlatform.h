// Conquer Ops: the few OS services the bridge needs (window focus, raw keys, mouse capture and
// deltas, a timer, an error box, the HUD font). SDL3 builds (64-bit Windows, macOS, Linux) use
// SDL; the 32-bit DX8 Windows build uses Win32.
#pragma once

#include <stdint.h>

namespace zhbo_platform
{
	// Whether the game window has keyboard focus.
	bool windowFocused();

	// Fill the 256-bit platform-neutral key-down set (bit i = key i of the Rust core's `Key`
	// enum, A..Z, Key1..Key6, Escape, Space, Tab, Shift, Control, MouseLeft, MouseRight, Up, Down,
	// Left, Right, Enter, Backspace). The bridge maps native scancodes or virtual keys straight to
	// these bits. The order must match `crates/zhbo-core/src/input.rs`'s `Key` enum exactly (the
	// bridge's `keyNames` table and its static_assert guard the count).
	void readKeys(uint64_t keys[4]);

	// The key enum, shared with the bridge (bit indices must match input.rs's `Key`).
	namespace keys
	{
		enum Key : uint8_t
		{
			A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
			Key1, Key2, Key3, Key4, Key5, Key6, Escape,
			Space, Tab, Shift, Control,
			MouseLeft, MouseRight,
			Up, Down, Left, Right, Enter, Backspace,
			COUNT = Backspace + 1,
		};
		inline void setDown(uint64_t bits[4], Key key, bool down)
		{
			const int k = (int)key;
			const uint64_t mask = 1ull << (k % 64);
			if (down)
				bits[k / 64] |= mask;
			else
				bits[k / 64] &= ~mask;
		}
	}

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
