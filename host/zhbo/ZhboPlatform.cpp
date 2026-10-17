// Conquer Ops: OS services for the bridge. See ZhboPlatform.h.
#include "ZhboPlatform.h"

#include <cstdlib>
#include <cstring>

#if defined(SAGE_USE_SDL3)

#include <SDL3/SDL.h>

extern SDL_Window *TheSDL3Window; // GeneralsMD/Code/GameEngineDevice/Source/SDL3GameEngine.cpp

#if defined(_WIN32)
#include <windows.h>
#endif

namespace zhbo_platform
{
	bool windowFocused()
	{
		return TheSDL3Window != nullptr && (SDL_GetWindowFlags(TheSDL3Window) & SDL_WINDOW_INPUT_FOCUS) != 0;
	}

	void readKeys(uint64_t bits[4])
	{
		bits[0] = 0;
		bits[1] = 0;
		bits[2] = 0;
		bits[3] = 0;
		using namespace keys;
		const bool *state = SDL_GetKeyboardState(nullptr);
		const SDL_Scancode letters[26] = { SDL_SCANCODE_A, SDL_SCANCODE_B, SDL_SCANCODE_C, SDL_SCANCODE_D,
			SDL_SCANCODE_E, SDL_SCANCODE_F, SDL_SCANCODE_G, SDL_SCANCODE_H, SDL_SCANCODE_I, SDL_SCANCODE_J,
			SDL_SCANCODE_K, SDL_SCANCODE_L, SDL_SCANCODE_M, SDL_SCANCODE_N, SDL_SCANCODE_O, SDL_SCANCODE_P,
			SDL_SCANCODE_Q, SDL_SCANCODE_R, SDL_SCANCODE_S, SDL_SCANCODE_T, SDL_SCANCODE_U, SDL_SCANCODE_V,
			SDL_SCANCODE_W, SDL_SCANCODE_X, SDL_SCANCODE_Y, SDL_SCANCODE_Z };
		for (int i = 0; i < 26; ++i)
		{
			setDown(bits, (Key)(Key::A + i), state[letters[i]]);
		}
		// SDL orders the digit row 1..9, 0.
		const SDL_Scancode digits[5] = { SDL_SCANCODE_1, SDL_SCANCODE_2, SDL_SCANCODE_3, SDL_SCANCODE_4, SDL_SCANCODE_5 };
		for (int i = 0; i < 5; ++i)
		{
			setDown(bits, (Key)(Key::Key1 + i), state[digits[i]]);
		}
		setDown(bits, Key::Space, state[SDL_SCANCODE_SPACE]);
		setDown(bits, Key::Tab, state[SDL_SCANCODE_TAB]);
		setDown(bits, Key::Shift, state[SDL_SCANCODE_LSHIFT] || state[SDL_SCANCODE_RSHIFT]);
		setDown(bits, Key::Control, state[SDL_SCANCODE_LCTRL] || state[SDL_SCANCODE_RCTRL]);
		const SDL_MouseButtonFlags buttons = SDL_GetMouseState(nullptr, nullptr);
		setDown(bits, Key::MouseLeft, (buttons & SDL_BUTTON_LMASK) != 0);
		setDown(bits, Key::MouseRight, (buttons & SDL_BUTTON_RMASK) != 0);
	}

	static bool s_captured = false;

	// The system cursor in the window's coordinates; false when it is outside the window.
	static bool cursorInWindow(float &x, float &y)
	{
		if (TheSDL3Window == nullptr)
			return false;
		float gx, gy;
		SDL_GetGlobalMouseState(&gx, &gy);
		int wx, wy, ww, wh;
		SDL_GetWindowPosition(TheSDL3Window, &wx, &wy);
		SDL_GetWindowSize(TheSDL3Window, &ww, &wh);
		x = gx - (float)wx;
		y = gy - (float)wy;
		return x >= 0.0f && y >= 0.0f && x < (float)ww && y < (float)wh;
	}

	void setMouseCapture(bool capture)
	{
		s_captured = capture;
		if (TheSDL3Window != nullptr)
		{
			// Relative mode hides and confines the cursor and reports unaccelerated deltas.
			SDL_SetWindowRelativeMouseMode(TheSDL3Window, capture);
			// Leaving relative mode on macOS leaves SDL thinking the cursor is over no window
			// until a click; warping it to where it already is puts it back over this one.
			float x, y;
			if (!capture && cursorInWindow(x, y))
				SDL_WarpMouseInWindow(TheSDL3Window, x, y);
		}
		float dx, dy;
		SDL_GetRelativeMouseState(&dx, &dy); // drop motion from before the capture
	}

	bool cursorOverWindow()
	{
		float x, y;
		return cursorInWindow(x, y);
	}

	void mouseDelta(float &dx, float &dy)
	{
		dx = dy = 0.0f;
		if (s_captured)
		{
			SDL_GetRelativeMouseState(&dx, &dy);
		}
	}

	double now()
	{
		return (double)SDL_GetPerformanceCounter() / (double)SDL_GetPerformanceFrequency();
	}

	void errorBox(const char *title, const char *text)
	{
		SDL_ShowSimpleMessageBox(SDL_MESSAGEBOX_ERROR, title, text, TheSDL3Window);
	}
}

#else // Win32 (the 32-bit DX8 build)

#include <windows.h>

extern HWND ApplicationHWnd;

namespace zhbo_platform
{
	bool windowFocused()
	{
		return GetForegroundWindow() == ApplicationHWnd;
	}

	void readKeys(uint64_t bits[4])
	{
		using namespace keys;
		const Key letters[26] = { Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
			Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S,
			Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z };
		for (int i = 0; i < 26; ++i)
		{
			setDown(bits, letters[i], (GetAsyncKeyState('A' + i) & 0x8000) != 0);
		}
		const Key digits[5] = { Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5 };
		for (int i = 0; i < 5; ++i)
		{
			setDown(bits, digits[i], (GetAsyncKeyState('1' + i) & 0x8000) != 0);
		}
		setDown(bits, Key::Space, (GetAsyncKeyState(VK_SPACE) & 0x8000) != 0);
		setDown(bits, Key::Tab, (GetAsyncKeyState(VK_TAB) & 0x8000) != 0);
		setDown(bits, Key::Shift, (GetAsyncKeyState(VK_SHIFT) & 0x8000) != 0);
		setDown(bits, Key::Control, (GetAsyncKeyState(VK_CONTROL) & 0x8000) != 0);
		setDown(bits, Key::MouseLeft, (GetAsyncKeyState(VK_LBUTTON) & 0x8000) != 0);
		setDown(bits, Key::MouseRight, (GetAsyncKeyState(VK_RBUTTON) & 0x8000) != 0);
	}

	static bool s_captured = false;

	static POINT windowCenter()
	{
		RECT rc;
		GetClientRect(ApplicationHWnd, &rc);
		POINT c = { (rc.right - rc.left) / 2, (rc.bottom - rc.top) / 2 };
		ClientToScreen(ApplicationHWnd, &c);
		return c;
	}

	void setMouseCapture(bool capture)
	{
		s_captured = capture;
		if (capture)
		{
			RECT rc;
			GetClientRect(ApplicationHWnd, &rc);
			POINT tl = { rc.left, rc.top }, br = { rc.right, rc.bottom };
			ClientToScreen(ApplicationHWnd, &tl);
			ClientToScreen(ApplicationHWnd, &br);
			RECT clip = { tl.x, tl.y, br.x, br.y };
			ClipCursor(&clip);
			POINT c = windowCenter();
			SetCursorPos(c.x, c.y);
		}
		else
		{
			ClipCursor(nullptr);
		}
	}

	bool cursorOverWindow()
	{
		RECT rc;
		POINT p;
		GetClientRect(ApplicationHWnd, &rc);
		GetCursorPos(&p);
		ScreenToClient(ApplicationHWnd, &p);
		return p.x >= rc.left && p.y >= rc.top && p.x < rc.right && p.y < rc.bottom;
	}

	void mouseDelta(float &dx, float &dy)
	{
		dx = dy = 0.0f;
		if (!s_captured)
		{
			return;
		}
		POINT c = windowCenter();
		POINT p;
		GetCursorPos(&p);
		dx = (float)(p.x - c.x);
		dy = (float)(p.y - c.y);
		SetCursorPos(c.x, c.y);
	}

	double now()
	{
		LARGE_INTEGER t, f;
		QueryPerformanceCounter(&t);
		QueryPerformanceFrequency(&f);
		return (double)t.QuadPart / (double)f.QuadPart;
	}

	void errorBox(const char *title, const char *text)
	{
		MessageBoxA(nullptr, text, title, MB_OK | MB_ICONERROR);
	}
}

#endif

namespace zhbo_platform
{
	// BO2's HUD type is a narrow DIN-like face that only exists inside BO2's encrypted fastfiles.
	const char *hudFontName()
	{
#if defined(_WIN32)
		// Bahnschrift (DIN) ships with Windows 10 and later; Arial otherwise.
		static const char *name = nullptr;
		if (name == nullptr)
		{
			LOGFONTA lf;
			std::memset(&lf, 0, sizeof(lf));
			lf.lfCharSet = DEFAULT_CHARSET;
			std::strcpy(lf.lfFaceName, "Bahnschrift");
			struct Probe
			{
				static int CALLBACK found(const LOGFONTA *, const TEXTMETRICA *, DWORD, LPARAM param)
				{
					*(bool *)param = true;
					return 0;
				}
			};
			bool present = false;
			HDC dc = GetDC(nullptr);
			EnumFontFamiliesExA(dc, &lf, (FONTENUMPROCA)Probe::found, (LPARAM)&present, 0);
			ReleaseDC(nullptr, dc);
			name = present ? "Bahnschrift" : "Arial";
		}
		return name;
#elif defined(__APPLE__)
		// TODO(mac): verify the engine's macOS font path resolves this family; fall back to Arial.
		return "DIN Alternate";
#else
		// TODO(linux): pick a condensed face that the engine's Linux font path finds.
		return "DejaVu Sans Condensed";
#endif
	}

	void exitProcess(int code)
	{
#if defined(_WIN32)
		ExitProcess(code);
#else
		std::_Exit(code);
#endif
	}
}
