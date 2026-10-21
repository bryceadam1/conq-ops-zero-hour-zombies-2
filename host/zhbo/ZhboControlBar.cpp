/*
** Conquer Ops - engine-side bridge: the Command Center button injection and idle-worker tracking.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
// The command bar's class set: the 15 class buttons' live data (zhbo_core::classicons through
// zhbo_class_buttons) and the command buttons they were registered as, plus whether the class set
// is up on which General. One page: a class set is at most ten classes (the five presets and the
// active set's five customs), all within page one's class slots. Rebuilt at game start so a
// profile (or an active set) edited in the shell between matches is picked up.
ZhboClassButton s_classButtonData[ZHBO_CLASS_BUTTONS];
const CommandButton *s_classButtons[ZHBO_CLASS_BUTTONS];
Int s_classButtonCount = 0;
Bool s_classSetOpen = FALSE;   // the class set is up in place of the General's own commands
Bool s_classSetForced = FALSE; // opened by Create a Class (stays until Back or a pick)
ObjectID s_classSetGeneral = INVALID_ID;
// A pick is in flight to the sim (a class-set button was pressed): the set stays up for the frame
// or two before the message lands, then the class landing closes it (even a forced re-pick's).
// Also keeps a pending pick from being re-opened behind if something else closed the set meanwhile.
ObjectID s_classPickPending = INVALID_ID;

} // namespace

class ZhboControlBarAccess
{
public:
	/// The General's Build menu for a dozer command set `dozerSet` (its name).
	static AsciiString buildSetName(const AsciiString &dozerSet)
	{
		AsciiString name("ZHBO_Build_");
		name.concat(dozerSet);
		return name;
	}

	/// For every dozer and worker command set, a copy with Back to the General's own commands.
	static void makeBuildSets()
	{
		const CommandButton *back = TheControlBar->findCommandButton("Command_ZHBO_BuildBack");
		if (!back)
			return;
		Int made = 0;
		for (const ThingTemplate *t = TheThingFactory->firstTemplate(); t; t = t->friend_getNextTemplate())
		{
			if (!isWorker(t))
				continue;
			const AsciiString &dozerSet = t->friend_getCommandSetString();
			const CommandSet *from = dozerSet.isEmpty() ? nullptr : TheControlBar->findCommandSet(dozerSet);
			if (!from || TheControlBar->findCommandSet(buildSetName(dozerSet)))
				continue;
			CommandSet *set = TheControlBar->newCommandSet(buildSetName(dozerSet));
			for (Int i = 0; i < MAX_COMMANDS_PER_SET; ++i)
				set->m_command[i] = from->getCommandButton(i);
			// Back in the first free bottom-row slot (12, 10), else in place of the last button.
			static const Int backSlots[] = { 11, 9, 13 };
			Int slot = backSlots[2];
			for (Int k = 0; k < 2; ++k)
				if (!set->m_command[backSlots[k]]) { slot = backSlots[k]; break; }
			set->m_command[slot] = back;
			++made;
		}
		logf("made %d General build menus", made);
		logBuildMenus();
	}

	/// The class set's command set. One page: a full class set is ten classes, inside page one's
	/// twelve class slots.
	static AsciiString classSetName()
	{
		return AsciiString("ZHBO_ClassPage1");
	}

	/// The INI command button that carries class button `button` (0-based).
	static AsciiString classButtonName(Int button)
	{
		AsciiString name;
		name.format("Command_ZHBO_Class%02d", button + 1);
		return name;
	}

	/// (Re)build the class set from the live class list: Take Control in slot 0, the five presets
	/// then the active set's custom slots at their own button indices, and Back in the bottom-right
	/// slot (a one-page set still needs its way out for a classed General who opened it to re-pick).
	/// Absent rows (the free custom slots and the ABI's padding rows) are left out, so their INI
	/// buttons are never registered nor placed. Called at startup and at every game start: the shell
	/// can change the profile and its active class set between matches, and the labels, icons and
	/// payloads are re-derived from that set here.
	static void makeClassSets()
	{
		if (!TheControlBar)
			return;
		const CommandButton *takeControl = TheControlBar->findCommandButton("Command_ZHBO_TakeControl");
		const CommandButton *back = TheControlBar->findCommandButton("Command_ZHBO_ClassBack");
		Int count = zhbo_class_buttons(s_classButtonData, ZHBO_CLASS_BUTTONS);
		if (count < 0)
			count = 0;
		if (count > ZHBO_CLASS_BUTTONS)
			count = ZHBO_CLASS_BUTTONS;
		s_classButtonCount = count;
		Int classes = 0;
		for (Int i = 0; i < ZHBO_CLASS_BUTTONS; ++i)
			s_classButtons[i] = nullptr;
		for (Int i = 0; i < s_classButtonCount; ++i)
		{
			if (!s_classButtonData[i].present)
				continue;
			s_classButtons[i] = TheControlBar->findCommandButton(classButtonName(i));
			++classes;
			if (!s_classButtons[i])
				logf("class menu: %s is missing (is ConquerOps.big loaded?)", classButtonName(i).str());
		}
		CommandSet *set = ensureSet(classSetName());
		clearSet(set);
		if (takeControl)
			set->m_command[0] = takeControl;
		for (Int i = 0; i < s_classButtonCount; ++i)
		{
			const ZhboClassButton &row = s_classButtonData[i];
			if (!row.present || !s_classButtons[i] || row.page != 1 || row.slot < 0 || row.slot >= MAX_COMMANDS_PER_SET)
				continue;
			set->m_command[row.slot] = s_classButtons[i];
		}
		if (back)
			set->m_command[13] = back;
		logf("made the class menu: %d classes, one page", classes);
	}

	/// The class set with this name, made on first use (the INI defines the buttons, not the set).
	static CommandSet *ensureSet(const AsciiString &name)
	{
		CommandSet *set = TheControlBar->findNonConstCommandSet(name);
		return set ? set : TheControlBar->newCommandSet(name);
	}

	/// Empty a set's 18 slots so a rebuild drops the classes the profile no longer has.
	static void clearSet(CommandSet *set)
	{
		for (Int i = 0; i < MAX_COMMANDS_PER_SET; ++i)
			set->m_command[i] = nullptr;
	}

	/// The single selected General, if the selection is exactly one General.
	static Object *selectedGeneral()
	{
		if (!TheInGameUI || TheInGameUI->getSelectCount() != 1)
			return nullptr;
		Drawable *draw = TheInGameUI->getFirstSelectedDrawable();
		Object *obj = draw ? draw->getObject() : nullptr;
		return isGeneral(obj) ? obj : nullptr;
	}

	/// The class button the engine is dispatching right now (it still carries WIN_STATE_SELECTED
	/// inside processCommandUI), or -1. Every class button shares one Command type, so the
	/// create-a-class handler learns which class was clicked from the selected command window.
	static Int pressedClassButton()
	{
		if (!TheControlBar)
			return -1;
		for (Int i = 0; i < MAX_COMMANDS_PER_SET; ++i)
		{
			GameWindow *win = TheControlBar->m_commandWindows[i];
			if (!win || win->winIsHidden())
				continue;
			WinInstanceData *inst = win->winGetInstanceData();
			if (!inst || !BitIsSet(inst->getState(), WIN_STATE_SELECTED))
				continue;
			const CommandButton *button = (const CommandButton *)GadgetButtonGetData(win);
			if (!button)
				continue;
			for (Int b = 0; b < s_classButtonCount; ++b)
				if (s_classButtons[b] && s_classButtons[b] == button)
					return b;
		}
		return -1;
	}

	/// Show the class set instead of the General's own command set (a UI-only override).
	static void openClassSet(Object *general, Bool forced)
	{
		if (!general || !TheControlBar)
			return;
		if (!TheControlBar->findCommandSet(classSetName()))
		{
			logLine("class menu: its command set is missing");
			return;
		}
		s_classSetGeneral = general->getID();
		s_classSetOpen = TRUE;
		s_classSetForced = forced;
		general->setCommandSetStringOverride(classSetName());
		TheControlBar->markUIDirty();
		logf("General %u opened the class menu", (unsigned)general->getID());
	}

	/// Back to the General's own command set.
	static void closeClassSet(Object *general)
	{
		if (!general)
			return;
		const Bool wasOpen = s_classSetOpen;
		s_classSetOpen = FALSE;
		s_classSetForced = FALSE;
		general->setCommandSetStringOverride(AsciiString::TheEmptyString);
		if (TheControlBar)
			TheControlBar->markUIDirty();
		if (wasOpen)
			logf("General %u closed the class menu", (unsigned)general->getID());
	}

	/// A class button was pressed: hand the button to the next client frame, which emits the same
	/// choose_class pick the class screen does (the core maps the button to its class, presets by
	/// index and customs by their wire words) and the bridge turns that into
	/// MSG_ZHBO_CHOOSE_CLASS. Queueing the pick into `s_out` here instead would be lost: every
	/// frame's `zhbo_client_frame` writes a fresh whole ZhboClientOut over `s_out` before
	/// `applyClientOutput` reads the pick, so the class never landed. The set stays up until the
	/// class does (see `updateClassSet`).
	static void chooseClass(Object *general, Int button)
	{
		if (!general || button < 0 || button >= s_classButtonCount)
			return;
		const ZhboClassButton &row = s_classButtonData[button];
		if (!row.present)
			return;
		// 1-based for the core: 0 is the one-shot's "no button".
		s_classButtonPick = button + 1;
		s_classPickPending = general->getID();
		logf("class menu: General %u picks %s (button %d)", (unsigned)general->getID(), row.name, button + 1);
	}

	/// Keep the class set and the selection in step: an unclassed General shows the class set; once
	/// he has a class his own commands return (unless Create a Class opened it for a re-pick).
	static void updateClassSet()
	{
		if (!TheControlBar || !TheInGameUI)
			return;
		Object *general = selectedGeneral();
		const ObjectID id = general ? general->getID() : INVALID_ID;
		if (id != s_classSetGeneral)
		{
			// Leaving a General with the class set up: hand him back his own commands, so selecting
			// him again does not keep the UI-only override forever.
			if (s_classSetGeneral != INVALID_ID && s_classSetOpen)
			{
				Object *prev = TheGameLogic->findObjectByID(s_classSetGeneral);
				const AsciiString &was = prev ? prev->getCommandSetString() : AsciiString::TheEmptyString;
				if (prev && was == classSetName())
					closeClassSet(prev);
			}
			s_classSetGeneral = id;
			s_classSetOpen = FALSE;
			s_classSetForced = FALSE;
			s_classPickPending = INVALID_ID;
		}
		if (!general)
			return;
		// Our class set is a UI-only override on the object; it may have been replaced since (the
		// Build menu, a script). Only close the set we actually put up, and never stomp another.
		const AsciiString &current = general->getCommandSetString();
		const Bool ours = current == classSetName();
		const Bool classed = zhbo_general_has_class((uint32_t)id) != 0;
		if (classed)
		{
			// The class button's pick landed (the client sends it, the sim applies it on its next
			// logic frame): hand the General his own commands. A forced set (a Create a Class
			// re-pick) closes too once its own pick lands; without one it stays up for the re-pick.
			const Bool picked = s_classPickPending == id;
			s_classPickPending = INVALID_ID;
			if (s_classSetOpen && ours && (picked || !s_classSetForced))
				closeClassSet(general);
			return;
		}
		if (id == s_classPickPending)
			return;
		if (!s_classSetOpen && !current.startsWith("ZHBO_Build_"))
			openClassSet(general, FALSE);
	}

	/// Draw each class's kit over its command-bar button (the ammo overlay's pattern): the primary
	/// weapon large, the secondary/lethal/tactical as minis, the perks as pips, plus Back's glyph on
	/// its plain tile. Only present classes whose buttons are visible are drawn.
	static void drawClassButtons()
	{
		if (!TheControlBar || !TheWindowManager || s_classButtonCount <= 0)
			return;
		for (Int i = 1; i <= 14; ++i)
		{
			char name[64];
			snprintf(name, sizeof(name), "ControlBar.wnd:ButtonCommand%02d", i);
			GameWindow *win = TheWindowManager->winGetWindowFromId(nullptr, TheNameKeyGenerator->nameToKey(name));
			if (!win || win->winIsHidden())
				continue;
			const CommandButton *button = (const CommandButton *)GadgetButtonGetData(win);
			if (!button)
				continue;
			Int x, y, w, h;
			win->winGetScreenPosition(&x, &y);
			win->winGetSize(&w, &h);
			Int found = -1;
			for (Int b = 0; b < s_classButtonCount; ++b)
			{
				if (s_classButtons[b] && s_classButtons[b] == button)
				{
					found = b;
					break;
				}
			}
			// The ABI's rows past the active set's classes are absent (rows 10..14 with a full set):
			// no kit to draw even if a stale command window still carries one of their buttons.
			if (found >= 0 && s_classButtonData[found].present)
			{
				drawClassKit(s_classButtonData[found], x, y, w, h);
				continue;
			}
			if (button->getName() == "Command_ZHBO_ClassBack")
				cbText((float)x + (float)w * 0.5f, (float)y + (float)h * 0.2f, "<", 0xFFF2C230u, (int)((float)h * 0.5f), 1);
		}
	}

	/// One button's kit: the primary icon large, the three offhand icons as minis along the bottom,
	/// the perk icons as a two-column strip of pips up the right edge.
	static void drawClassKit(const ZhboClassButton &row, Int x, Int y, Int w, Int h)
	{
		const Real side = (Real)(w < h ? w : h);
		const Real pad = 2.0f;
		const Real primary = side * 0.60f;
		cbImage(row.primary, (float)x + pad, (float)y + pad, primary, primary, 0xFFF2FFFFu);

		const Real mini = side * 0.24f;
		Real mx = (float)x + pad;
		const Real my = (float)(y + h) - mini - 1.0f;
		const char *offhands[3] = { row.secondary, row.lethal, row.tactical };
		for (Int i = 0; i < 3; ++i)
		{
			if (offhands[i][0])
				cbImage(offhands[i], mx, my, mini, mini, 0xE0FFFFFFu);
			mx += mini + 1.0f;
		}

		const Real pip = side * 0.16f;
		const Real right = (float)(x + w) - pad - pip;
		Real px = right;
		Real py = my - pip - 2.0f;
		const Int perks = row.perk_count < 0 ? 0 : (row.perk_count > 6 ? 6 : row.perk_count);
		for (Int i = 0; i < perks; ++i)
		{
			if (row.perks[i][0])
				cbImage(row.perks[i], px, py, pip, pip, 0xD0FFFFFFu);
			if ((i & 1) == 0)
				px -= pip + 1.0f;
			else
			{
				px = right;
				py -= pip + 1.0f;
			}
		}
	}

	/// A stock dozer or worker (the General is one too, but not anyone's worker).
	static Bool isWorker(const ThingTemplate *t)
	{
		return t && t->isKindOf(KINDOF_DOZER) && strncmp(t->getName().str(), "ZHBO_", 5) != 0;
	}

	/// The worker a faction trains: the one its Command Center builds (each General's own, as the
	/// Toxin General's Chem_GLAWorker), else one it starts with.
	static const ThingTemplate *workerOf(const PlayerTemplate *pt)
	{
		if (!pt)
			return nullptr;
		const ThingTemplate *cc = TheThingFactory->findTemplate(pt->getStartingBuilding());
		const CommandSet *set = cc ? TheControlBar->findCommandSet(cc->friend_getCommandSetString()) : nullptr;
		for (Int i = 0; set && i < MAX_COMMANDS_PER_SET; ++i)
		{
			const CommandButton *b = set->getCommandButton(i);
			if (b && b->getCommandType() == GUI_COMMAND_UNIT_BUILD && isWorker(b->getThingTemplate()))
				return b->getThingTemplate();
		}
		for (Int i = 0; i < MAX_MP_STARTING_UNITS; ++i)
		{
			const ThingTemplate *t = TheThingFactory->findTemplate(pt->getStartingUnit(i));
			if (isWorker(t))
				return t;
		}
		return nullptr;
	}

	/// The Build menu for `general`: his faction's worker's, with Back.
	static AsciiString buildSetFor(const Object *general)
	{
		const Player *player = general ? general->getControllingPlayer() : nullptr;
		const ThingTemplate *worker = player ? workerOf(player->getPlayerTemplate()) : nullptr;
		const AsciiString name = worker ? buildSetName(worker->friend_getCommandSetString()) : AsciiString::TheEmptyString;
		return !name.isEmpty() && TheControlBar->findCommandSet(name) ? name : AsciiString::TheEmptyString;
	}

	/// Which worker's menu each faction's General builds from (the build test reads it).
	static void logBuildMenus()
	{
		for (Int i = 0; ThePlayerTemplateStore && i < ThePlayerTemplateStore->getPlayerTemplateCount(); ++i)
		{
			const PlayerTemplate *pt = ThePlayerTemplateStore->getNthPlayerTemplate(i);
			if (!pt || pt->isObserver() || !pt->isPlayableSide())
				continue;
			const ThingTemplate *worker = workerOf(pt);
			logf("build menu: %s builds as %s", pt->getName().str(), worker ? worker->getName().str() : "(no worker)");
		}
	}

	/// Whether `obj` is on the Next Idle Worker button's list.
	static Bool isIdleWorker(Object *obj)
	{
		return TheInGameUI && TheInGameUI->findIdleWorker(obj) != nullptr;
	}

	static void injectConstructButtons()
	{
		const CommandButton *usa = TheControlBar->findCommandButton("Command_ZHBO_ConstructGeneral_USA");
		const CommandButton *china = TheControlBar->findCommandButton("Command_ZHBO_ConstructGeneral_China");
		const CommandButton *gla = TheControlBar->findCommandButton("Command_ZHBO_ConstructGeneral_GLA");
		if (!usa || !china || !gla)
		{
			logLine("Construct General buttons missing - is ConquerOps.big loaded?");
			return;
		}
		// 1-based UI slots 3, 2, 11, 12 are free in every stock Command Center set.
		static const Int preferredSlots[] = { 2, 1, 10, 11 };
		Int injected = 0;
		for (CommandSet *set = TheControlBar->m_commandSets; set; set = set->friend_getNext())
		{
			const AsciiString &name = set->getName();
			if (strstr(name.str(), "CommandCenterCommandSet") == nullptr || name.startsWith("Fake"))
				continue;
			const CommandButton *button = usa;
			if (strstr(name.str(), "China")) button = china;
			else if (strstr(name.str(), "GLA")) button = gla;

			Bool present = FALSE;
			for (Int i = 0; i < MAX_COMMANDS_PER_SET; ++i)
				present |= set->m_command[i] == button;
			if (present)
				continue;
			for (Int k = 0; k < (Int)ARRAY_SIZE(preferredSlots); ++k)
			{
				if (set->m_command[preferredSlots[k]] == nullptr)
				{
					set->m_command[preferredSlots[k]] = button;
					++injected;
					break;
				}
			}
		}
		logf("added Deploy General to %d Command Center command sets", injected);
		makeBuildSets();
	}
};

namespace
{
// Generals on the Next Idle Worker button's list (a dozer's idle state puts them there): logged as
// they join or leave it.
std::set<ObjectID> s_idleGenerals;
} // namespace

void trackIdleGenerals()
{
	if (!TheInGameUI)
		return;
	for (Object *obj = TheGameLogic->getFirstObject(); obj; obj = obj->getNextObject())
	{
		if (!isGeneral(obj))
			continue;
		const Bool idle = ZhboControlBarAccess::isIdleWorker(obj);
		if (idle == (s_idleGenerals.count(obj->getID()) > 0))
			continue;
		if (idle)
			s_idleGenerals.insert(obj->getID());
		else
			s_idleGenerals.erase(obj->getID());
		logf("General %u %s the idle workers", (unsigned)obj->getID(), idle ? "joined" : "left");
	}
}

AsciiString zhboBuildSetFor(const Object *general)
{
	return ZhboControlBarAccess::buildSetFor(general);
}

void zhboInjectConstructButtons()
{
	if (!TheControlBar)
		return;
	ZhboControlBarAccess::injectConstructButtons();
	// Independent of the Command Center injection (a missing Deploy button must not cost the class
	// menu): build the class set from the live class list.
	ZhboControlBarAccess::makeClassSets();
}

Object *zhboSelectedGeneral()
{
	return ZhboControlBarAccess::selectedGeneral();
}

Int zhboPressedClassButton()
{
	return ZhboControlBarAccess::pressedClassButton();
}

void zhboOpenClassSet(Object *general, Bool forced)
{
	ZhboControlBarAccess::openClassSet(general, forced);
}

void zhboCloseClassSet(Object *general)
{
	ZhboControlBarAccess::closeClassSet(general);
}

void zhboChooseClassButton(Object *general, Int button)
{
	ZhboControlBarAccess::chooseClass(general, button);
}

void zhboUpdateClassBar()
{
	ZhboControlBarAccess::updateClassSet();
}

void drawClassButtons()
{
	ZhboControlBarAccess::drawClassButtons();
}

void ResetControlBarTrackers()
{
	s_idleGenerals.clear();
	s_classSetOpen = FALSE;
	s_classSetForced = FALSE;
	s_classSetGeneral = INVALID_ID;
	s_classPickPending = INVALID_ID;
	s_classButtonCount = 0;
	// The shell may have edited the profile, or switched which class set is active, between
	// matches: rebuild the set from the live class list (labels and icons too). The set-building
	// call also refreshes every row's page/slot and the customs' pick payloads.
	if (TheControlBar)
		ZhboControlBarAccess::makeClassSets();
}

