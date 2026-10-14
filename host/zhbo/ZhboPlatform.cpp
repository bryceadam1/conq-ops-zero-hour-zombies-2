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

	void readKeys(uint8_t keys[256])
	{
		std::memset(keys, 0, 256);
		const bool *state = SDL_GetKeyboardState(nullptr);
		for (int i = 0; i < 26; ++i)
		{
			keys['A' + i] = state[SDL_SCANCODE_A + i] ? 1 : 0;
		}
		// SDL orders the digit row 1..9, 0.
		for (int i = 1; i <= 9; ++i)
		{
			keys['0' + i] = state[SDL_SCANCODE_1 + (i - 1)] ? 1 : 0;
		}
		keys['0'] = state[SDL_SCANCODE_0] ? 1 : 0;
		keys[0x10] = (state[SDL_SCANCODE_LSHIFT] || state[SDL_SCANCODE_RSHIFT]) ? 1 : 0;
		keys[0x11] = (state[SDL_SCANCODE_LCTRL] || state[SDL_SCANCODE_RCTRL]) ? 1 : 0;
		keys[0x20] = state[SDL_SCANCODE_SPACE] ? 1 : 0;
		keys[0x09] = state[SDL_SCANCODE_TAB] ? 1 : 0;
		const SDL_MouseButtonFlags buttons = SDL_GetMouseState(nullptr, nullptr);
		keys[0x01] = (buttons & SDL_BUTTON_LMASK) ? 1 : 0;
		keys[0x02] = (buttons & SDL_BUTTON_RMASK) ? 1 : 0;
	}

	static bool s_captured = false;

	void setMouseCapture(bool capture)
	{
		s_captured = capture;
		if (TheSDL3Window != nullptr)
		{
			// Relative mode hides and confines the cursor and reports unaccelerated deltas.
			SDL_SetWindowRelativeMouseMode(TheSDL3Window, capture);
		}
		float dx, dy;
		SDL_GetRelativeMouseState(&dx, &dy); // drop motion from before the capture
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

	void readKeys(uint8_t keys[256])
	{
		for (int vk = 0; vk < 256; ++vk)
		{
			keys[vk] = (GetAsyncKeyState(vk) & 0x8000) ? 1 : 0;
		}
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
