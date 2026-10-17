/*
** Conquer Ops - engine-side bridge: building in first person: the hammer's build menu, sites, work
** and ghost.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
	// What `general` can build: the construct buttons of his build menu.
	std::vector<const ThingTemplate *> buildables(const Object *general)
	{
		std::vector<const ThingTemplate *> out;
		const CommandSet *set = general ? TheControlBar->findCommandSet(zhboBuildSetFor(general)) : nullptr;
		for (Int i = 0; set && i < MAX_COMMANDS_PER_SET; ++i)
		{
			const CommandButton *b = set->getCommandButton(i);
			if (b && b->getCommandType() == GUI_COMMAND_DOZER_CONSTRUCT && b->getThingTemplate())
				out.push_back(b->getThingTemplate());
		}
		return out;
	}

	// Zero Hour lets a builder make only what its current command set offers
	// (BuildAssistant::isPossibleToMakeUnit): while the hammer asks, his build menu is that set.
	struct BuildMenuOn
	{
		Object *obj;
		AsciiString before;
		Bool overridden;
		explicit BuildMenuOn(Object *o) : obj(o), overridden(FALSE)
		{
			if (!obj)
				return;
			const AsciiString set = zhboBuildSetFor(obj);
			if (set.isEmpty() || obj->getCommandSetString() == set)
				return;
			overridden = obj->getCommandSetString() != obj->getTemplate()->friend_getCommandSetString();
			before = obj->getCommandSetString();
			obj->setCommandSetStringOverride(set);
		}
		~BuildMenuOn()
		{
			if (obj && !before.isEmpty())
				obj->setCommandSetStringOverride(overridden ? before : AsciiString::TheEmptyString);
		}
	};

	void utf8(const UnicodeString &text, char *out, size_t cap)
	{
		size_t n = 0;
		unsigned high = 0; // pending high surrogate (0xD800..0xDBFF), 0 when none
		for (const WideChar *c = text.str(); c && *c && n + 4 < cap; ++c)
		{
			const unsigned v = (unsigned)*c;
			if (v >= 0xD800 && v <= 0xDBFF)
			{
				// A pair follows: its low half combines into one code point. A lone high surrogate
				// (no low half before the next one or the end) becomes '?' below.
				if (high)
					out[n++] = '?';
				high = v;
				continue;
			}
			if (v >= 0xDC00 && v <= 0xDFFF)
			{
				if (high)
				{
					const unsigned cp = 0x10000u + ((high - 0xD800u) << 10) + (v - 0xDC00u);
					out[n++] = (char)(0xF0 | (cp >> 18));
					out[n++] = (char)(0x80 | ((cp >> 12) & 0x3F));
					out[n++] = (char)(0x80 | ((cp >> 6) & 0x3F));
					out[n++] = (char)(0x80 | (cp & 0x3F));
				}
				else
				{
					out[n++] = '?'; // lone low surrogate
				}
				high = 0;
				continue;
			}
			if (high)
			{
				out[n++] = '?'; // high surrogate with no low half after it
				high = 0;
			}
			if (v < 0x80)
				out[n++] = (char)v;
			else if (v < 0x800)
			{
				out[n++] = (char)(0xC0 | (v >> 6));
				out[n++] = (char)(0x80 | (v & 0x3F));
			}
			else
			{
				out[n++] = (char)(0xE0 | (v >> 12));
				out[n++] = (char)(0x80 | ((v >> 6) & 0x3F));
				out[n++] = (char)(0x80 | (v & 0x3F));
			}
		}
		if (high && n + 1 < cap)
			out[n++] = '?'; // trailing lone high surrogate
		out[n] = 0;
	}

	int32_t buildOptions(uint32_t generalId, ZhboBuildOption *out, int32_t cap)
	{
		Object *general = TheGameLogic->findObjectByID((ObjectID)generalId);
		Player *player = general ? general->getControllingPlayer() : nullptr;
		if (!player)
			return 0;
		BuildMenuOn menu(general);
		int32_t n = 0;
		for (const ThingTemplate *what : buildables(general))
		{
			if (n >= cap)
				break;
			ZhboBuildOption &o = out[n++];
			o.tmpl = what->getTemplateID();
			o.cost = what->calcCostToBuild(player);
			const CanMakeType can = TheBuildAssistant->canMakeUnit(general, what);
			o.status = can == CANMAKE_OK ? ZHBO_BUILD_OK : can == CANMAKE_NO_MONEY ? ZHBO_BUILD_NO_MONEY : ZHBO_BUILD_LOCKED;
			o.radius = what->getTemplateGeometryInfo().getBoundingCircleRadius();
			UnicodeString name = what->getDisplayName();
			if (name.isEmpty())
				name.translate(what->getName());
			utf8(name, o.name, sizeof(o.name));
		}
		return n;
	}

	const ThingTemplate *buildable(const Object *general, uint32_t tmpl)
	{
		for (const ThingTemplate *what : buildables(general))
			if (what->getTemplateID() == tmpl)
				return what;
		return nullptr;
	}

	// Zero Hour's own test for the dozer's placement (DozerAIUpdate::construct).
	LegalBuildCode legalAt(Object *general, const ThingTemplate *what, const ZhboVec3 *pos, float angle, UnsignedInt extra)
	{
		Coord3D at;
		at.set(pos->x, pos->y, pos->z);
		const UnsignedInt checks = BuildAssistant::TERRAIN_RESTRICTIONS | BuildAssistant::CLEAR_PATH
			| BuildAssistant::NO_OBJECT_OVERLAP | BuildAssistant::SHROUD_REVEALED | extra;
		return TheBuildAssistant->isLocationLegalToBuild(&at, what, angle, checks, general, nullptr);
	}
} // namespace


int32_t cbBuildOptions(uint32_t general, ZhboBuildOption *out, int32_t cap)
{
	return buildOptions(general, out, cap);
}

int32_t cbCanPlace(uint32_t generalId, uint32_t tmpl, const ZhboVec3 *pos, float angle)
{
	Object *general = TheGameLogic->findObjectByID((ObjectID)generalId);
	const ThingTemplate *what = buildable(general, tmpl);
	BuildMenuOn menu(general);
	const LegalBuildCode code = what ? legalAt(general, what, pos, angle, 0) : LBC_GENERIC_FAILURE;
	TheTerrainVisual->removeAllBibs(); // the check marks what is in the way
	if (code != LBC_OK)
		logf("General %u can't place %s at (%.0f, %.0f): legal-build code %d", generalId,
			what ? what->getName().str() : "?", pos->x, pos->y, (int)code);
	if (code == LBC_OBJECTS_IN_THE_WAY && what)
	{
		Coord3D at;
		at.set(pos->x, pos->y, pos->z);
		SimpleObjectIterator *iter = ThePartitionManager->iterateObjectsInRange(&at, what->getTemplateGeometryInfo().getBoundingCircleRadius() * 1.5f, FROM_CENTER_2D);
		MemoryPoolObjectHolder hold(iter);
		for (Object *o = iter->first(); o; o = iter->next())
			logf("  near: %s %u busy %d disabled %d", o->getTemplate()->getName().str(), (unsigned)o->getID(),
				o->getAI() ? (int)o->getAI()->isBusy() : -1, (int)o->isDisabled());
	}
	return code == LBC_OK;
}

// As a dozer: pay, and lay out the site (BuildAssistant::buildObjectNow -> DozerAIUpdate::construct).
// The General's AI is off in first person, so the site waits for him.
uint32_t cbPlace(uint32_t generalId, uint32_t tmpl, const ZhboVec3 *pos, float angle)
{
	Object *general = TheGameLogic->findObjectByID((ObjectID)generalId);
	Player *player = general ? general->getControllingPlayer() : nullptr;
	const ThingTemplate *what = buildable(general, tmpl);
	BuildMenuOn menu(general);
	if (!what || !player || TheBuildAssistant->canMakeUnit(general, what) != CANMAKE_OK)
		return 0;
	Coord3D at;
	at.set(pos->x, pos->y, pos->z);
	Object *site = TheBuildAssistant->buildObjectNow(general, what, &at, angle, player);
	TheTerrainVisual->removeAllBibs();
	if (!site)
		return 0;
	logf("General %u placed %s in first person", generalId, what->getName().str());
	return (uint32_t)site->getID();
}

// How far `p` is from the edge of `obj`'s footprint.


namespace
{
	Real footprintDistance(const Object *obj, const Coord3D *p)
	{
		const GeometryInfo &g = obj->getGeometryInfo();
		const Coord3D *c = obj->getPosition();
		const Real dx = p->x - c->x, dy = p->y - c->y;
		if (g.getGeomType() != GEOMETRY_BOX)
			return sqrtf(dx * dx + dy * dy) - g.getMajorRadius();
		const Real a = obj->getOrientation();
		const Real lx = fabsf(dx * Cos(a) + dy * Sin(a)) - g.getMajorRadius();
		const Real ly = fabsf(-dx * Sin(a) + dy * Cos(a)) - g.getMinorRadius();
		const Real ox = lx > 0.0f ? lx : 0.0f, oy = ly > 0.0f ? ly : 0.0f;
		return sqrtf(ox * ox + oy * oy);
	}

	DozerAIInterface *dozerOf(Object *obj)
	{
		AIUpdateInterface *ai = obj ? obj->getAI() : nullptr;
		return ai ? ai->getDozerAIInterface() : nullptr;
	}

	// What `general` can do at `obj` by Zero Hour's rules for a dozer (ActionManager): ZHBO_WORK_*,
	// or -1. A site he laid out himself is his to build even with his dozer task on it.
	Int workOn(const Object *general, const Object *obj)
	{
		if (!obj->isKindOf(KINDOF_STRUCTURE))
			return -1;
		if (obj->testStatus(OBJECT_STATUS_UNDER_CONSTRUCTION))
		{
			const Bool his = obj->getBuilderID() == general->getID() && obj->getControllingPlayer() == general->getControllingPlayer();
			return his || TheActionManager->canResumeConstructionOf(general, obj, CMD_FROM_PLAYER) ? ZHBO_WORK_BUILD : -1;
		}
		return TheActionManager->canRepairObject(general, obj, CMD_FROM_PLAYER) ? ZHBO_WORK_REPAIR : -1;
	}

	// How far along work on `obj` is: built, or its health.
	float workPercent(const Object *obj, Int kind)
	{
		if (kind == ZHBO_WORK_BUILD)
			return obj->getConstructionPercent();
		const BodyModuleInterface *body = obj->getBodyModule();
		return body && body->getMaxHealth() > 0.0f ? body->getHealth() / body->getMaxHealth() * 100.0f : 100.0f;
	}
} // namespace


uint32_t cbWorkSite(uint32_t generalId, const ZhboVec3 *pos, float reach, ZhboWorkSite *out)
{
	const Object *general = TheGameLogic->findObjectByID((ObjectID)generalId);
	if (!general || !general->getControllingPlayer())
		return 0;
	Coord3D at;
	at.set(pos->x, pos->y, pos->z);
	SimpleObjectIterator *iter = ThePartitionManager->iterateObjectsInRange(&at, reach + 300.0f, FROM_CENTER_2D);
	MemoryPoolObjectHolder hold(iter);
	const Object *best = nullptr;
	Int bestKind = -1;
	Real bestDistance = reach;
	for (Object *obj = iter->first(); obj; obj = iter->next())
	{
		const Int kind = workOn(general, obj);
		if (kind < 0)
			continue;
		const Real d = footprintDistance(obj, &at);
		if (d <= bestDistance && (!best || d < bestDistance || obj->getID() < best->getID()))
		{
			best = obj;
			bestKind = kind;
			bestDistance = d;
		}
	}
	if (!best)
		return 0;
	out->id = (uint32_t)best->getID();
	out->tmpl = best->getTemplate()->getTemplateID();
	out->kind = bestKind;
	out->percent = workPercent(best, bestKind);
	UnicodeString name = best->getTemplate()->getDisplayName();
	if (name.isEmpty())
		name.translate(best->getTemplate()->getName());
	utf8(name, out->name, sizeof(out->name));
	return out->id;
}

// One frame of his work on `siteId`, the dozer's own (DozerAIUpdate::buildStep and repairStep,
// host/patches/0012) at `rate`, the first-person speed.
float cbWork(uint32_t generalId, uint32_t siteId, float rate)
{
	Object *general = TheGameLogic->findObjectByID((ObjectID)generalId);
	Object *site = TheGameLogic->findObjectByID((ObjectID)siteId);
	const Int kind = general && site ? workOn(general, site) : -1;
	if (kind == ZHBO_WORK_BUILD)
	{
		// As a dozer arriving at the site (DozerActionMoveToActionPosState): it is being built now.
		site->clearAndSetModelConditionFlags(MAKE_MODELCONDITION_MASK(MODELCONDITION_AWAITING_CONSTRUCTION),
			MAKE_MODELCONDITION_MASK2(MODELCONDITION_PARTIALLY_CONSTRUCTED, MODELCONDITION_ACTIVELY_BEING_CONSTRUCTED));
		if (!DozerAIUpdate::buildStep(general, site, rate, FALSE))
			return site->getConstructionPercent();
		// Built by hand: his dozer task on it (from laying it out) is done too.
		DozerAIInterface *dozer = dozerOf(general);
		if (dozer && dozer->getTaskTarget(DOZER_TASK_BUILD) == site->getID())
			dozer->cancelTask(DOZER_TASK_BUILD);
		logf("General %u finished building %s in first person", generalId, site->getTemplate()->getName().str());
		return 100.0f;
	}
	if (kind == ZHBO_WORK_REPAIR)
	{
		if (!DozerAIUpdate::repairStep(general, site, rate))
			return -1.0f; // another builder has it
		const float percent = workPercent(site, kind);
		if (percent < 100.0f)
			return percent;
		if (general->isLocallyViewed())
			TheInGameUI->message("DOZER:RepairComplete");
		logf("General %u repaired %s in first person", generalId, site->getTemplate()->getName().str());
		return 100.0f;
	}
	return -1.0f;
}

// The hammer's placement ghost: Zero Hour's placement icon (see-through, red where it can't go).


namespace
{
	Drawable *s_ghost = nullptr;
	uint32_t s_ghostTmpl = 0;
} // namespace


void cbGhost(uint32_t tmpl, const ZhboVec3 *pos, float angle, int32_t legal)
{
	if (tmpl != s_ghostTmpl && s_ghost)
	{
		TheGameClient->destroyDrawable(s_ghost);
		s_ghost = nullptr;
	}
	s_ghostTmpl = tmpl;
	if (!tmpl)
		return;
	if (!s_ghost)
	{
		const ThingTemplate *what = TheThingFactory->findByTemplateID((UnsignedShort)tmpl);
		if (!what)
			return;
		UnsignedInt status = DRAWABLE_STATUS_NO_STATE_PARTICLES;
		status |= TheGlobalData->m_objectPlacementShadows ? DRAWABLE_STATUS_SHADOWS : 0;
		s_ghost = TheThingFactory->newDrawable(what, status);
		if (!s_ghost)
			return;
		s_ghost->setDrawableOpacity(TheGlobalData->m_objectPlacementOpacity);
		if (const Player *me = ThePlayerList->getLocalPlayer())
			s_ghost->setIndicatorColor(me->getPlayerColor());
	}
	Coord3D at;
	at.set(pos->x, pos->y, pos->z);
	s_ghost->setPosition(&at);
	s_ghost->setOrientation(angle);
	static const RGBColor illegal = { 1.0f, 0.0f, 0.0f };
	s_ghost->colorTint(legal ? nullptr : &illegal);
}


namespace
{
	// The last placement check (cbClientCanPlace): keyed on what it checked, so a changed input is
	// not answered with a stale result. Cleared in Zhbo_Reset (it reads engine state per game).
	struct CanPlaceCache
	{
		uint32_t general, tmpl;
		float x, y, z, angle;
		int32_t result;
	};
	CanPlaceCache s_canPlace = {};
} // namespace


int32_t cbClientCanPlace(uint32_t generalId, uint32_t tmpl, const ZhboVec3 *pos, float angle)
{
	// Every other frame, as Zero Hour's placement UI: what is in the way stays marked meanwhile.
	if (TheGameClient->getFrame() & 1)
		return s_canPlace.result;
	if (s_canPlace.general == generalId && s_canPlace.tmpl == tmpl && s_canPlace.x == pos->x && s_canPlace.y == pos->y
		&& s_canPlace.z == pos->z && s_canPlace.angle == angle)
		return s_canPlace.result;
	TheTerrainVisual->removeAllBibs();
	Object *general = TheGameLogic->findObjectByID((ObjectID)generalId);
	const ThingTemplate *what = buildable(general, tmpl);
	s_canPlace = { generalId, tmpl, pos->x, pos->y, pos->z, angle,
		what && legalAt(general, what, pos, angle, BuildAssistant::USE_QUICK_PATHFIND | BuildAssistant::IGNORE_STEALTHED) == LBC_OK ? 1 : 0 };
	return s_canPlace.result;
}


namespace
{
	// A reset destroys every drawable, the ghost too.
	void forgetGhost()
	{
		s_ghost = nullptr;
		s_ghostTmpl = 0;
	}
} // namespace

void ResetBuilding()
{
	forgetGhost();
	memset(&s_canPlace, 0, sizeof(s_canPlace));
}
