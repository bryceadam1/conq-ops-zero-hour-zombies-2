/*
** Conquer Ops - engine-side bridge: the shell create-a-class screen (the lobby's class menu).
** A full-screen modal window built in code (no .wnd is shipped--the mod rule), on the engine's
** own modal-dialog pattern (ExtendedMessageBox): the window swallows every input so the lobby
** beneath is inert, and its draw callback runs the Rust core's menu frame, which draws through
** the stored ZhboClientApi callbacks (valid during a draw, as with the stock menus) and plays its
** sounds through the stored sound callback (TheAudio works in the shell). The profile one-shot
** writes the file through the same zhbo_profile_write path the in-game screen uses. The lobbies'
** Create a Class button (Skirmish, LAN and online game setup: host patch 0018) is built here too.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
#include "GameClient/Shell.h"

namespace
{

// The one screen window (created by Zhbo_OpenClassMenu, destroyed by Zhbo_CloseClassMenu or the
// window manager; GWM_DESTROY keeps the pointer honest).
GameWindow *s_menuWindow = nullptr;

// Wheel notches since the last menu frame. The shell has no raw-wheel latch (the in-game
// s_wheelAccum feeds only a client frame), so the window's GWM_WHEEL messages accumulate here;
// the sign matches the in-game convention (up is positive).
float s_menuWheel = 0.0f;

// The screen swallows everything while it is up: the core reads the pointer and keys from the
// platform polls in the draw frame, so the messages themselves only need eating (and the wheel
// accumulated).
WindowMsgHandledType zhboMenuInput(GameWindow *, UnsignedInt msg, WindowMsgData, WindowMsgData)
{
	if (msg == GWM_WHEEL_UP)
		s_menuWheel += 1.0f;
	else if (msg == GWM_WHEEL_DOWN)
		s_menuWheel -= 1.0f;
	return MSG_HANDLED;
}

WindowMsgHandledType zhboMenuSystem(GameWindow *, UnsignedInt msg, WindowMsgData mData1, WindowMsgData mData2)
{
	switch (msg)
	{
	case GWM_INPUT_FOCUS:
		// Take the keyboard focus (as the stock dialogs do): the lobby's own Escape shortcut must
		// not fire while the screen is up. The core polls the keys itself.
		if (mData1 == TRUE)
			*(Bool *)mData2 = TRUE;
		break;
	case GWM_DESTROY:
		s_menuWindow = nullptr;
		break;
	default:
		return MSG_IGNORED;
	}
	return MSG_HANDLED;
}

void zhboMenuDraw(GameWindow *, WinInstanceData *)
{
	Zhbo_MenuFrame();
}

} // namespace

void Zhbo_OpenClassMenu()
{
	if (s_menuWindow != nullptr || !TheWindowManager || !TheDisplay)
		return;
	WinInstanceData instData;
	instData.init();
	// A root window (winSetModal requires one), the full screen, drawn last (it opens on top).
	GameWindow *win = TheWindowManager->winCreate(nullptr, WIN_STATUS_ENABLED, 0, 0,
		TheDisplay->getWidth(), TheDisplay->getHeight(), zhboMenuSystem, &instData);
	if (win == nullptr)
		return;
	win->winSetDrawFunc(zhboMenuDraw);
	win->winSetInputFunc(zhboMenuInput);
	TheWindowManager->winSetModal(win);
	TheWindowManager->winSetFocus(nullptr);
	TheWindowManager->winSetFocus(win);
	win->winHide(FALSE);
	win->winBringToTop();
	s_menuWindow = win;
	s_menuWheel = 0.0f;
	zhbo_menu_open();
	logLine("create-a-class: shell screen opened");
}

void Zhbo_CloseClassMenu()
{
	GameWindow *win = s_menuWindow;
	if (win == nullptr)
		return;
	s_menuWindow = nullptr;
	zhbo_menu_close();
	// Deferred destruction (processDestroyList), safe from inside the draw: the window is marked
	// and unlinked now, freed on the window manager's next update.
	TheWindowManager->winDestroy(win);
	// Hand the keyboard focus back to the shell screen beneath (the lobby), as after a stock
	// dialog closes (its Init gave it the focus; nobody re-runs that on our close). Only while the
	// shell is active: closing during a game transition (Zhbo_Reset) must not focus a hidden
	// shell window mid-match.
	if (TheShell && TheShell->isShellActive() && TheShell->top())
		TheWindowManager->winSetFocus(TheShell->top()->getFirstWindow());
	logLine("create-a-class: shell screen closed");
}

Bool Zhbo_ClassMenuOpen()
{
	return s_menuWindow != nullptr ? TRUE : FALSE;
}

void Zhbo_MenuFrame()
{
	if (s_menuWindow == nullptr)
		return;
	advanceDrawFrame();
	ZhboMenuIn in;
	memset(&in, 0, sizeof(in));
	if (zhbo_platform::windowFocused())
		zhbo_platform::readKeys(in.keys.bits);
	// The pointer and its held buttons from the engine's cursor (the shell is not mouse-captured;
	// TheMouse keeps it in step with the system cursor). Bit 0 left, 1 right, 2 middle (matches
	// zhbo_core::input::mouse_buttons).
	if (TheMouse)
	{
		const MouseIO *m = TheMouse->getMouseStatus();
		in.mouse_x = (float)m->pos.x;
		in.mouse_y = (float)m->pos.y;
		if (m->leftState == MBS_Down || m->leftState == MBS_DoubleClick)
			in.mouse_buttons |= 1u;
		if (m->rightState == MBS_Down || m->rightState == MBS_DoubleClick)
			in.mouse_buttons |= 2u;
		if (m->middleState == MBS_Down || m->middleState == MBS_DoubleClick)
			in.mouse_buttons |= 4u;
	}
	in.wheel = s_menuWheel;
	s_menuWheel = 0.0f;
	in.screen_w = (float)TheDisplay->getWidth();
	in.screen_h = (float)TheDisplay->getHeight();

	ZhboMenuOut out;
	memset(&out, 0, sizeof(out));
	zhbo_menu_frame(&in, &out);

	// The profile one-shot (the editor's Save, or a delete/copy): the core already applied it;
	// write the whole file, exactly as the in-game path does. A zero-word payload is a real write
	// too (deleting the last class leaves an empty profile).
	if (out.save_profile != 0)
	{
		if (const char *profile = zhbo_profile_write(out.save_profile_payload, out.save_profile_payload_len))
		{
			writeProfileFile(profile);
			logf("create-a-class: profile written from the shell menu (%d words)", (int)out.save_profile_payload_len);
		}
	}
	evictTexts();
	if (out.closed)
		Zhbo_CloseClassMenu();
}

// ------------------------------------------------------------------------------------------------
// The lobbies' Create a Class button
// ------------------------------------------------------------------------------------------------

NameKeyType Zhbo_CreateClassButtonID(const char *wnd)
{
	AsciiString name;
	name.format("%s:ButtonCreateClass", wnd);
	return TheNameKeyGenerator->nameToKey(name);
}

GameWindow *Zhbo_EnsureCreateClassButton(GameWindow *menuParent, const char *wnd, const char *panelName, const char *titleLineName, GameWindow *startButton)
{
	if (menuParent == nullptr || TheWindowManager == nullptr)
		return nullptr;
	// The layout's windows are destroyed when the shell pops the menu, so a cached pointer is stale
	// on a later visit: look the button up on this layout (a shipped .wnd element with the id would
	// win too) and build it only when it is missing. Idempotent within one layout.
	const NameKeyType id = Zhbo_CreateClassButtonID(wnd);
	if (GameWindow *have = TheWindowManager->winGetWindowFromId(menuParent, id))
		return have;
	const auto find = [&](const char *child) -> GameWindow *
	{
		if (child == nullptr)
			return nullptr;
		AsciiString name;
		name.format("%s:%s", wnd, child);
		return TheWindowManager->winGetWindowFromId(menuParent, TheNameKeyGenerator->nameToKey(name));
	};

	// Same size and font as the menu's Start button, so it reads as one of the shell's own.
	Int w = 174, h = 36;
	GameFont *font = nullptr;
	if (startButton)
	{
		startButton->winGetSize(&w, &h);
		font = startButton->winGetFont();
	}
	if (font == nullptr)
		font = TheWindowManager->winFindFont("Times New Roman", 14, FALSE);

	// The empty top-right corner of the menu's panel, in its title row: every game setup menu has
	// the same panel-and-title-line layout (SubParent / GadgetParent / FadeParent over the line
	// under the title). The button fits between the panel's top and that line, so a menu whose
	// title row is shorter than the Start button (online: 38 px) gets a shorter button. Without the
	// panel, the menu's lower right. gogoGadgetPushButton takes parent-relative coordinates.
	Int px = 0, py = 0, pw = 0, ph = 0;
	menuParent->winGetScreenPosition(&px, &py);
	menuParent->winGetSize(&pw, &ph);
	Int x = pw - w - 20, y = ph - h - 20;
	if (GameWindow *panel = find(panelName))
	{
		Int rx = 0, ry = 0, rw = 0, rh = 0;
		panel->winGetScreenPosition(&rx, &ry);
		panel->winGetSize(&rw, &rh);
		x = rx - px + rw - w - 4;
		y = ry - py + 4;
		if (GameWindow *line = find(titleLineName))
		{
			Int lx = 0, ly = 0;
			line->winGetScreenPosition(&lx, &ly);
			const Int room = ly - ry - 4 - 2;
			if (room > 12 && room < h)
				h = room;
		}
	}

	WinInstanceData instData;
	instData.init();
	BitSet(instData.m_style, GWS_PUSH_BUTTON | GWS_MOUSE_TRACK);
	// The plain draw path, not WIN_STATUS_IMAGE: the image path paints only the gadget's images,
	// and assignDefaultGadgetLook assigns none here (the retail shell uses its own Buttons-* images).
	GameWindow *button = TheWindowManager->gogoGadgetPushButton(menuParent,
		WIN_STATUS_ENABLED | WIN_STATUS_NO_FOCUS, x, y, w, h, &instData, font, FALSE);
	if (button == nullptr)
		return nullptr;
	// Set the text and its colours directly: assignDefaultGadgetLook skips them when defaultVisual
	// is FALSE, and an undefined colour has zero alpha (an invisible button). The text colours are
	// the Start button's; the fill is the menu panel's dark style.
	GadgetButtonSetText(button, UnicodeString(L"Create a Class"));
	button->winSetEnabledTextColors(TheWindowManager->winMakeColor(255, 255, 255, 255),
		TheWindowManager->winMakeColor(0, 0, 0, 255));
	button->winSetDisabledTextColors(TheWindowManager->winMakeColor(62, 64, 92, 255),
		TheWindowManager->winMakeColor(31, 32, 47, 255));
	button->winSetHiliteTextColors(TheWindowManager->winMakeColor(186, 255, 12, 255),
		TheWindowManager->winMakeColor(0, 2, 0, 255));
	GadgetButtonSetEnabledColor(button, TheWindowManager->winMakeColor(2, 2, 2, 220));
	GadgetButtonSetEnabledBorderColor(button, TheWindowManager->winMakeColor(47, 55, 168, 255));
	GadgetButtonSetHiliteColor(button, TheWindowManager->winMakeColor(24, 32, 60, 235));
	GadgetButtonSetHiliteBorderColor(button, TheWindowManager->winMakeColor(186, 255, 12, 255));
	GadgetButtonSetDisabledColor(button, TheWindowManager->winMakeColor(16, 16, 20, 220));
	GadgetButtonSetDisabledBorderColor(button, TheWindowManager->winMakeColor(64, 64, 80, 255));
	button->winSetTooltip(UnicodeString(L"Browse, edit and manage your custom classes"));
	// The id a .wnd element would have, so the menu's system handler routes its clicks through
	// the same comparison.
	button->winSetWindowId(id);
	logf("create-a-class: lobby button added to %s", wnd);
	return button;
}
