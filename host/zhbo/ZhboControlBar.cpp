/*
** Conquer Ops - engine-side bridge: the Command Center button injection and idle-worker tracking.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
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
	if (TheControlBar)
		ZhboControlBarAccess::injectConstructButtons();
}

void ResetControlBarTrackers()
{
	s_idleGenerals.clear();
}

