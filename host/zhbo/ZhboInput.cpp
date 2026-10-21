/*
** Conquer Ops - engine-side bridge: input translation, mouse capture and the hidden first-person body.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
class ZhboTranslator : public GameMessageTranslator
{
public:
	virtual GameMessageDisposition translateGameMessage(const GameMessage *msg) override
	{
		// First person owns the mouse/keyboard; a class screen owns them too, from either view:
		// clicks and keys must not reach the RTS UI or the game behind it (Escape stays).
		if (!s_out.camera_override && !s_out.menu_open)
			return KEEP_MESSAGE;
		const GameMessage::Type t = msg->getType();
		if (t == GameMessage::MSG_RAW_MOUSE_WHEEL)
		{
			if (msg->getArgumentCount() > 1)
				s_wheelAccum += msg->getArgument(1)->real;
			return DESTROY_MESSAGE;
		}
		if (t > GameMessage::MSG_RAW_MOUSE_BEGIN && t < GameMessage::MSG_RAW_MOUSE_END)
			return DESTROY_MESSAGE;
		if (t == GameMessage::MSG_RAW_KEY_DOWN || t == GameMessage::MSG_RAW_KEY_UP)
		{
			// Escape still opens the game menu; everything else belongs to first person.
			if (msg->getArgumentCount() > 0 && msg->getArgument(0)->integer == KEY_ESC)
				return KEEP_MESSAGE;
			return DESTROY_MESSAGE;
		}
		return KEEP_MESSAGE;
	}
};

// Seconds left to keep Zero Hour's mouse in step with the system cursor after first person.
Bool s_mouseCaptured = FALSE;
Bool s_rtsUiHidden = FALSE;
ObjectID s_hiddenObject = INVALID_ID;
double s_reclaimMouseUntil = 0.0;
} // namespace


// Relative mouse mode reports the cursor as having left the window, and ending it sends no
// "entered" event until the next click (SDL's own "left" may also still be queued), so Zero Hour
// would keep the cursor uncaptured (no edge scrolling) and outside (no selection) until then. While
// the cursor really is over the window, tell the mouse so and capture it again, as entering would.
void reclaimMouse()
{
	if (!TheMouse || s_mouseCaptured || zhbo_platform::now() > s_reclaimMouseUntil)
		return;
	if (!zhbo_platform::windowFocused() || !zhbo_platform::cursorOverWindow())
		return;
	if (!TheMouse->isCursorInside())
	{
		TheMouse->onCursorMovedInside();
		TheMouse->syncPositionToSystemCursor();
	}
	if (!TheMouse->isCursorCaptured())
		TheMouse->refreshCursorCapture();
}

void setMouseCapture(Bool capture)
{
	if (capture == s_mouseCaptured)
		return;
	s_mouseCaptured = capture;
	zhbo_platform::setMouseCapture(capture != FALSE);
	// The Take Control button grabs the mouse on the press that starts first person, and first
	// person swallows the release: the window manager kept routing the next click overhead to that
	// button (nothing selected, no edge scrolling until a click). Nothing holds the mouse across
	// first person.
	if (TheWindowManager && TheWindowManager->winGetGrabWindow())
		TheWindowManager->winSetGrabWindow(nullptr);
	if (TheMouse) TheMouse->setVisibility(capture ? FALSE : TRUE);
	if (!capture && TheMouse)
	{
		TheMouse->syncPositionToSystemCursor();
		s_reclaimMouseUntil = zhbo_platform::now() + 3.0;
		reclaimMouse();
	}
}

void setHiddenObject(ObjectID id)
{
	if (id == s_hiddenObject)
	{
		// Re-assert every frame; other systems may toggle visibility.
		if (Object *obj = TheGameLogic->findObjectByID(id))
			if (Drawable *d = obj->getDrawable())
				d->setDrawableHidden(TRUE);
		return;
	}
	if (Object *prev = TheGameLogic->findObjectByID(s_hiddenObject))
		if (Drawable *d = prev->getDrawable())
			d->setDrawableHidden(FALSE);
	s_hiddenObject = id;
	if (Object *obj = TheGameLogic->findObjectByID(id))
		if (Drawable *d = obj->getDrawable())
			d->setDrawableHidden(TRUE);
}

void setRtsUiHidden(Bool hidden)
{
	if (hidden == s_rtsUiHidden)
		return;
	s_rtsUiHidden = hidden;
	if (hidden)
	{
		TheInGameUI->deselectAllDrawables();
		HideControlBar(TRUE);
	}
	else
	{
		ShowControlBar(TRUE);
	}
}
Bool zhboMouseCaptured()
{
	return s_mouseCaptured;
}

Bool zhboRtsUiHidden()
{
	return s_rtsUiHidden;
}

GameMessageTranslator *zhboCreateTranslator()
{
	return MSGNEW("GameClientSubsystem") ZhboTranslator;
}
