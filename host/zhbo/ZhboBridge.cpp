/*
** Conquer Ops - engine-side bridge. Fills the Rust core's host API with Zero Hour services and
** forwards engine events to it. See docs/CONTRACT.md for ownership and timing rules.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridge.h"
#include "zhbo_core.h"

#include "Common/GameUtility.h"
#include "Common/GlobalData.h"
#include "Common/MessageStream.h"
#include "Common/Player.h"
#include "Common/PlayerList.h"
#include "Common/Team.h"
#include "Common/ThingFactory.h"
#include "Common/ThingTemplate.h"
#include "GameClient/ControlBar.h"
#include "GameClient/Display.h"
#include "GameClient/DisplayString.h"
#include "GameClient/DisplayStringManager.h"
#include "GameClient/Drawable.h"
#include "GameClient/GameFont.h"
#include "GameClient/GUICallbacks.h"
#include "GameClient/InGameUI.h"
#include "GameClient/KeyDefs.h"
#include "GameClient/Mouse.h"
#include "GameClient/View.h"
#include "GameLogic/AI.h"
#include "GameLogic/Damage.h"
#include "GameLogic/GameLogic.h"
#include "GameLogic/Module/AIUpdate.h"
#include "GameLogic/Module/BodyModule.h"
#include "GameLogic/Object.h"
#include "GameLogic/PartitionManager.h"
#include "GameLogic/TerrainLogic.h"
#include "GameLogic/Weapon.h"

#include <map>
#include <string>
#include <vector>

extern HWND ApplicationHWnd;

namespace
{

Bool s_ready = FALSE;
FILE *s_log = nullptr;

// Client state carried between the client frame, the view update and the HUD draw.
ZhboClientOut s_out = {};
Bool s_mouseCaptured = FALSE;
Bool s_rtsUiHidden = FALSE;
ObjectID s_hiddenObject = INVALID_ID;
Real s_wheelAccum = 0.0f;
LARGE_INTEGER s_lastTick = {};

// One engine Weapon per FX template: fireWeaponTemplate needs a firing Weapon instance.
std::map<std::string, Weapon *> s_fxWeapons;

void logLine(const char *msg)
{
	DEBUG_LOG(("[zhbo] %s", msg));
	if (s_log)
	{
		fprintf(s_log, "[%u] %s\n", TheGameLogic ? TheGameLogic->getFrame() : 0u, msg);
		fflush(s_log);
	}
}

void logf(const char *fmt, ...)
{
	char buf[512];
	va_list args;
	va_start(args, fmt);
	vsnprintf(buf, sizeof(buf), fmt, args);
	va_end(args);
	logLine(buf);
}

// --------------------------------------------------------------------------------------------
// Logic API
// --------------------------------------------------------------------------------------------

void cbLog(const char *msg) { logLine(msg); }

float cbGroundHeight(float x, float y)
{
	return TheTerrainLogic ? TheTerrainLogic->getGroundHeight(x, y) : 0.0f;
}

int32_t cbIsWalkable(float x, float y)
{
	if (!TheTerrainLogic)
		return 1;
	if (TheTerrainLogic->isCliffCell(x, y))
		return 0;
	Real waterZ = 0.0f, terrainZ = 0.0f;
	// Infantry wade through shallow water but cannot swim.
	if (TheTerrainLogic->isUnderwater(x, y, &waterZ, &terrainZ) && waterZ - terrainZ > 6.0f)
		return 0;
	return 1;
}

void cbExtent(ZhboVec3 *lo, ZhboVec3 *hi)
{
	Region3D r;
	TheTerrainLogic->getExtent(&r);
	lo->x = r.lo.x; lo->y = r.lo.y; lo->z = r.lo.z;
	hi->x = r.hi.x; hi->y = r.hi.y; hi->z = r.hi.z;
}

void fillInfo(const Object *obj, const Object *viewer, ZhboObjInfo *out)
{
	memset(out, 0, sizeof(*out));
	out->id = (uint32_t)obj->getID();
	const Player *owner = obj->getControllingPlayer();
	out->owner_player = owner ? owner->getPlayerIndex() : -1;

	uint32_t flags = 0;
	if (obj->isKindOf(KINDOF_STRUCTURE)) flags |= ZHBO_OBJ_STRUCTURE;
	if (obj->isKindOf(KINDOF_INFANTRY)) flags |= ZHBO_OBJ_INFANTRY;
	if (obj->isKindOf(KINDOF_VEHICLE)) flags |= ZHBO_OBJ_VEHICLE;
	if (obj->isKindOf(KINDOF_AIRCRAFT)) flags |= ZHBO_OBJ_AIRCRAFT;
	if (obj->isEffectivelyDead()) flags |= ZHBO_OBJ_DEAD;
	if (obj->isKindOf(KINDOF_NO_COLLIDE) || obj->isKindOf(KINDOF_SHRUBBERY) || obj->isKindOf(KINDOF_PROJECTILE)
		|| obj->isKindOf(KINDOF_MINE) || obj->isKindOf(KINDOF_INERT) || obj->isKindOf(KINDOF_BRIDGE))
		flags |= ZHBO_OBJ_NO_COLLIDE;
	if (obj->isAirborneTarget() || obj->isSignificantlyAboveTerrain()) flags |= ZHBO_OBJ_AIRBORNE;
	if (viewer && viewer != obj)
	{
		const Relationship rel = viewer->getRelationship(obj);
		if (rel == ENEMIES) flags |= ZHBO_OBJ_ENEMY;
		else if (rel == ALLIES) flags |= ZHBO_OBJ_ALLY;
	}
	out->flags = flags;

	const Coord3D *pos = obj->getPosition();
	out->pos.x = pos->x; out->pos.y = pos->y; out->pos.z = pos->z;
	out->yaw = obj->getOrientation();

	const GeometryInfo &geom = obj->getGeometryInfo();
	switch (geom.getGeomType())
	{
		case GEOMETRY_SPHERE: out->geom_type = ZHBO_GEOM_SPHERE; break;
		case GEOMETRY_BOX: out->geom_type = ZHBO_GEOM_BOX; break;
		default: out->geom_type = ZHBO_GEOM_CYLINDER; break;
	}
	out->major_radius = geom.getMajorRadius();
	out->minor_radius = geom.getMinorRadius();
	out->height = geom.getMaxHeightAbovePosition();

	if (const BodyModuleInterface *body = obj->getBodyModule())
	{
		out->health = body->getHealth();
		out->max_health = body->getMaxHealth();
	}
}

int32_t cbObject(uint32_t id, uint32_t viewerId, ZhboObjInfo *out)
{
	const Object *obj = TheGameLogic->findObjectByID((ObjectID)id);
	if (!obj)
		return 0;
	const Object *viewer = TheGameLogic->findObjectByID((ObjectID)viewerId);
	fillInfo(obj, viewer, out);
	return 1;
}

int32_t cbQuery(const ZhboVec3 *center, float radius, uint32_t viewerId, ZhboObjInfo *out, int32_t cap)
{
	const Object *viewer = TheGameLogic->findObjectByID((ObjectID)viewerId);
	Coord3D c;
	c.set(center->x, center->y, center->z);
	SimpleObjectIterator *iter = ThePartitionManager->iterateObjectsInRange(&c, radius, FROM_CENTER_3D);
	MemoryPoolObjectHolder hold(iter);
	int32_t n = 0;
	for (Object *obj = iter->first(); obj && n < cap; obj = iter->next())
	{
		fillInfo(obj, viewer, &out[n++]);
	}
	return n;
}

void cbSetPose(uint32_t id, const ZhboVec3 *pos, float yaw, int32_t moving)
{
	Object *obj = TheGameLogic->findObjectByID((ObjectID)id);
	if (!obj)
		return;
	Coord3D p;
	p.set(pos->x, pos->y, pos->z);
	obj->setPosition(&p);
	obj->setOrientation(yaw);
	if (moving)
		obj->setModelConditionState(MODELCONDITION_MOVING);
	else
		obj->clearModelConditionState(MODELCONDITION_MOVING);
}

void cbSetControlled(uint32_t id, int32_t controlled)
{
	Object *obj = TheGameLogic->findObjectByID((ObjectID)id);
	if (!obj)
		return;
	if (AIUpdateInterface *ai = obj->getAIUpdateInterface())
	{
		// Drop whatever the unit was doing; on release it starts idle and auto-acquires as usual.
		ai->aiIdle(CMD_FROM_AI);
	}
	obj->clearModelConditionState(MODELCONDITION_MOVING);
	logf("General %u %s first-person control", id, controlled ? "entered" : "left");
}

Weapon *fxWeapon(const char *name)
{
	std::map<std::string, Weapon *>::iterator it = s_fxWeapons.find(name);
	if (it != s_fxWeapons.end())
		return it->second;
	Weapon *weapon = nullptr;
	if (const WeaponTemplate *tmpl = TheWeaponStore->findWeaponTemplate(name))
		weapon = TheWeaponStore->allocateNewWeapon(tmpl, PRIMARY_WEAPON);
	else
		logf("missing FX weapon template '%s'", name);
	s_fxWeapons[name] = weapon;
	return weapon;
}

void cbFireFx(uint32_t shooter, const char *fxWeapon_, uint32_t target, const ZhboVec3 *pos)
{
	Object *src = TheGameLogic->findObjectByID((ObjectID)shooter);
	Weapon *weapon = fxWeapon(fxWeapon_);
	if (!src || !weapon || !src->getDrawable())
		return;
	Coord3D p;
	p.set(pos->x, pos->y, pos->z);
	WeaponBonus bonus;
	weapon->getTemplate()->fireWeaponTemplate(src, PRIMARY_WEAPON, 0, nullptr, &p, bonus,
		FALSE /*isProjectileDetonation*/, TRUE /*ignoreRanges*/, weapon, nullptr, FALSE /*inflictDamage*/);
	(void)target;
}

int32_t cbDamage(uint32_t shooter, uint32_t victimId, float amount, int32_t melee)
{
	Object *victim = TheGameLogic->findObjectByID((ObjectID)victimId);
	if (!victim || victim->isEffectivelyDead())
		return 0;
	DamageInfo info;
	info.in.m_damageType = melee ? DAMAGE_MELEE : DAMAGE_SMALL_ARMS;
	info.in.m_deathType = DEATH_NORMAL;
	info.in.m_sourceID = (ObjectID)shooter;
	if (const Object *src = TheGameLogic->findObjectByID((ObjectID)shooter))
		if (const Player *p = src->getControllingPlayer())
			info.in.m_sourcePlayerMask = p->getPlayerMask();
	info.in.m_amount = amount;
	victim->attemptDamage(&info);
	return victim->isEffectivelyDead() ? 1 : 0;
}

void cbHeal(uint32_t id, float amount)
{
	Object *obj = TheGameLogic->findObjectByID((ObjectID)id);
	if (obj)
		obj->attemptHealing(amount, obj);
}

// --------------------------------------------------------------------------------------------
// Client API (HUD)
// --------------------------------------------------------------------------------------------

struct CachedText
{
	std::string text;
	Int size;
	DisplayString *ds;
	UnsignedInt lastUsed;
};
std::vector<CachedText> s_texts;
UnsignedInt s_drawFrame = 0;

void cbScreenSize(float *w, float *h)
{
	*w = (float)TheDisplay->getWidth();
	*h = (float)TheDisplay->getHeight();
}

void cbFillRect(float x, float y, float w, float h, uint32_t argb)
{
	TheDisplay->drawFillRect((Int)x, (Int)y, (Int)(w + 0.5f), (Int)(h + 0.5f), argb);
}

void cbLine(float x0, float y0, float x1, float y1, float width, uint32_t argb)
{
	TheDisplay->drawLine((Int)x0, (Int)y0, (Int)x1, (Int)y1, width, argb);
}

DisplayString *cachedText(const char *utf8, Int size)
{
	for (size_t i = 0; i < s_texts.size(); ++i)
	{
		if (s_texts[i].size == size && s_texts[i].text == utf8)
		{
			s_texts[i].lastUsed = s_drawFrame;
			return s_texts[i].ds;
		}
	}
	DisplayString *ds = TheDisplayStringManager->newDisplayString();
	if (!ds)
		return nullptr;
	ds->setFont(TheFontLibrary->getFont("Arial", size, TRUE));
	UnicodeString text;
	text.translate(AsciiString(utf8));
	ds->setText(text);
	CachedText entry = { utf8, size, ds, s_drawFrame };
	s_texts.push_back(entry);
	return ds;
}

void evictTexts()
{
	for (size_t i = 0; i < s_texts.size();)
	{
		if (s_drawFrame - s_texts[i].lastUsed > 120)
		{
			TheDisplayStringManager->freeDisplayString(s_texts[i].ds);
			s_texts[i] = s_texts.back();
			s_texts.pop_back();
		}
		else
		{
			++i;
		}
	}
}

void cbText(float x, float y, const char *utf8, uint32_t argb, int32_t size, int32_t align)
{
	DisplayString *ds = cachedText(utf8, size < 6 ? 6 : size);
	if (!ds)
		return;
	Int w = ds->getWidth();
	Int px = (Int)x;
	if (align == 1) px -= w / 2;
	else if (align == 2) px -= w;
	const UnsignedInt shadow = (argb & 0xFF000000u) >> 1 & 0xFF000000u;
	ds->draw(px, (Int)y, argb, shadow);
}

// --------------------------------------------------------------------------------------------
// Input
// --------------------------------------------------------------------------------------------

class ZhboTranslator : public GameMessageTranslator
{
public:
	virtual GameMessageDisposition translateGameMessage(const GameMessage *msg) override
	{
		if (!s_out.camera_override)
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

POINT windowCenter()
{
	RECT rc;
	GetClientRect(ApplicationHWnd, &rc);
	POINT c = { (rc.right - rc.left) / 2, (rc.bottom - rc.top) / 2 };
	ClientToScreen(ApplicationHWnd, &c);
	return c;
}

void setMouseCapture(Bool capture)
{
	if (capture == s_mouseCaptured)
		return;
	s_mouseCaptured = capture;
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
		if (TheMouse) TheMouse->setVisibility(FALSE);
	}
	else
	{
		ClipCursor(nullptr);
		if (TheMouse) TheMouse->setVisibility(TRUE);
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

// --------------------------------------------------------------------------------------------
// Autopilot (ZHBO_AUTOPILOT=1): an in-game test scene. Single-player only; it spawns objects from
// the client, which is fine for a local test but would desync a network game.
// --------------------------------------------------------------------------------------------

struct Autopilot
{
	Bool enabled;
	Bool spawned;
	ObjectID general;
	UnsignedInt releasedFrame;
	Int retakes;
};
Autopilot s_autopilot = {};

void findCommandCenter(Object *obj, void *userData)
{
	Object **found = (Object **)userData;
	if (!*found && obj->isKindOf(KINDOF_COMMANDCENTER) && !obj->isEffectivelyDead())
		*found = obj;
}

Object *spawnAt(const char *templateName, Team *team, const Coord3D &pos, Real yaw)
{
	const ThingTemplate *tmpl = TheThingFactory->findTemplate(templateName);
	if (!tmpl || !team)
	{
		logf("autopilot: cannot spawn %s", templateName);
		return nullptr;
	}
	Object *obj = TheThingFactory->newObject(tmpl, team);
	Coord3D p = pos;
	p.z = TheTerrainLogic->getGroundHeight(p.x, p.y);
	obj->setPosition(&p);
	obj->setOrientation(yaw);
	return obj;
}

void autopilotSpawn()
{
	Player *local = ThePlayerList->getLocalPlayer();
	Object *cc = nullptr;
	local->iterateObjects(findCommandCenter, &cc);
	if (!cc)
		return;

	Player *enemy = nullptr;
	for (Int i = 0; i < ThePlayerList->getPlayerCount() && !enemy; ++i)
	{
		Player *p = ThePlayerList->getNthPlayer(i);
		if (p != local && p->getPlayerTemplate() && local->getRelationship(p->getDefaultTeam()) == ENEMIES)
			enemy = p;
	}

	// Stand the General in front of the Command Center, facing away from it.
	const Real yaw = cc->getOrientation();
	const Real c = Cos(yaw), s = Sin(yaw);
	const Coord3D *ccPos = cc->getPosition();
	Coord3D pos;
	pos.set(ccPos->x + c * 90.0f, ccPos->y + s * 90.0f, 0.0f);
	Object *general = spawnAt("ZHBO_General_USA", local->getDefaultTeam(), pos, yaw);
	if (!general)
		return;
	s_autopilot.general = general->getID();

	// Three enemy riflemen ahead of the General (inside rifle range, outside the base).
	if (enemy)
	{
		for (Int i = -1; i <= 1; ++i)
		{
			Coord3D e;
			e.set(pos.x + c * 70.0f - s * 12.0f * i, pos.y + s * 70.0f + c * 12.0f * i, 0.0f);
			Object *rebel = spawnAt("GLAInfantryRebel", enemy->getDefaultTeam(), e, yaw + PI);
			// One rebel is ordered onto the General so the test also covers incoming damage.
			if (rebel && i == 0 && rebel->getAIUpdateInterface())
				rebel->getAIUpdateInterface()->aiAttackObject(general, NO_MAX_SHOTS_LIMIT, CMD_FROM_AI);
		}
	}
	logf("autopilot: spawned General %u at (%.0f, %.0f), enemies: %s", (unsigned)s_autopilot.general, pos.x, pos.y,
		enemy ? "3 GLA rebels" : "none (no enemy player)");
	Zhbo_RequestTakeControl(general);
}

void autopilotFrame()
{
	if (!s_autopilot.enabled)
		return;
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (!s_autopilot.spawned)
		return; // the spawn itself happens in Zhbo_LogicStep
	// After the field computer hands the General back, take control again once (tests re-entry).
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general || s_autopilot.retakes >= 1)
		return;
	if (zhbo_logic_is_controlled((uint32_t)s_autopilot.general))
	{
		s_autopilot.releasedFrame = 0;
		return;
	}
	if (s_autopilot.releasedFrame == 0)
		s_autopilot.releasedFrame = frame;
	else if (frame - s_autopilot.releasedFrame > 90)
	{
		++s_autopilot.retakes;
		logLine("autopilot: taking control again");
		Zhbo_RequestTakeControl(general);
	}
}

// The RTS camera never sees the horizon, so maps normally leave the skybox off. Turn it on while
// first person owns the camera and restore the map's setting afterwards.
Bool s_skyForced = FALSE;
Real s_skyWas = 0.0f;

void setSkyForced(Bool forced)
{
	if (forced == s_skyForced || !TheWritableGlobalData)
		return;
	s_skyForced = forced;
	if (forced)
	{
		s_skyWas = TheWritableGlobalData->m_drawSkyBox;
		TheWritableGlobalData->m_drawSkyBox = TRUE;
	}
	else
	{
		TheWritableGlobalData->m_drawSkyBox = s_skyWas;
	}
}

void releaseClientState()
{
	setSkyForced(FALSE);
	setMouseCapture(FALSE);
	setHiddenObject(INVALID_ID);
	setRtsUiHidden(FALSE);
	memset(&s_out, 0, sizeof(s_out));
}

} // namespace

// ------------------------------------------------------------------------------------------------
// Command Center button injection. Friend of CommandSet/ControlBar (see host/patches).
// ------------------------------------------------------------------------------------------------
class ZhboControlBarAccess
{
public:
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
	}
};

// ------------------------------------------------------------------------------------------------
// Public entry points
// ------------------------------------------------------------------------------------------------

void Zhbo_Init()
{
	if (s_ready)
		return;

	AsciiString logPath = TheGlobalData->getPath_UserData();
	logPath.concat("ConquerOps.log");
	s_log = fopen(logPath.str(), "w");

	if (zhbo_abi_version() != ZHBO_ABI_VERSION)
	{
		logf("zhbo-core ABI %u does not match glue ABI %u; mod disabled", zhbo_abi_version(), ZHBO_ABI_VERSION);
		return;
	}

	// Conquer Ops needs both games installed; refuse to run otherwise.
	char problems[1024];
	if (!zhbo_check_requirements(problems, sizeof(problems)))
	{
		logf("requirements not met: %s", problems);
		char text[1200];
		snprintf(text, sizeof(text), "Conquer Ops requires both Command & Conquer Generals: Zero Hour and "
			"Call of Duty: Black Ops II to be installed.\n\n%s", problems);
		MessageBoxA(nullptr, text, "Conquer Ops", MB_OK | MB_ICONERROR);
		ExitProcess(1);
	}

	static const ZhboLogicApi logicApi = {
		cbLog, cbGroundHeight, cbIsWalkable, cbExtent, cbObject, cbQuery,
		cbSetPose, cbSetControlled, cbFireFx, cbDamage, cbHeal,
	};
	static const ZhboClientApi clientApi = { cbScreenSize, cbFillRect, cbLine, cbText };
	zhbo_init(&logicApi, &clientApi);

	if (TheControlBar)
		ZhboControlBarAccess::injectConstructButtons();

	s_autopilot.enabled = getenv("ZHBO_AUTOPILOT") != nullptr;

	QueryPerformanceCounter(&s_lastTick);
	s_ready = TRUE;
}

void Zhbo_Reset()
{
	if (!s_ready)
		return;
	releaseClientState();
	const Bool autopilot = s_autopilot.enabled;
	memset(&s_autopilot, 0, sizeof(s_autopilot));
	s_autopilot.enabled = autopilot;
	for (std::map<std::string, Weapon *>::iterator it = s_fxWeapons.begin(); it != s_fxWeapons.end(); ++it)
		deleteInstance(it->second);
	s_fxWeapons.clear();
	zhbo_reset();
	logLine("reset");
}

void Zhbo_OnTakeControlMessage(Int playerIndex, const GameMessage *msg)
{
	if (!s_ready || msg->getArgumentCount() < 1)
		return;
	const ObjectID id = msg->getArgument(0)->objectID;
	Object *obj = TheGameLogic->findObjectByID(id);
	if (!obj || strncmp(obj->getTemplate()->getName().str(), "ZHBO_General", 12) != 0)
		return;
	zhbo_logic_take_control(playerIndex, (uint32_t)id);
}

void Zhbo_OnInputMessage(Int playerIndex, const GameMessage *msg)
{
	if (!s_ready || msg->getArgumentCount() < 6)
		return;
	ZhboFpsInput input;
	input.held = (uint32_t)msg->getArgument(1)->integer;
	input.pressed = (uint32_t)msg->getArgument(2)->integer;
	input.move_packed = msg->getArgument(3)->integer;
	input.yaw = msg->getArgument(4)->real;
	input.pitch = msg->getArgument(5)->real;
	zhbo_logic_input(playerIndex, (uint32_t)msg->getArgument(0)->objectID, &input);
}

void Zhbo_LogicStep(UnsignedInt frame)
{
	if (!s_ready)
		return;
	// Objects must be created inside GameLogic::update: W3DModelDraw only resolves weapon barrel
	// bones while the logic is updating, and a drawable created outside it sizes its recoil state
	// to zero and later crashes in handleWeaponFireFX when it fires.
	if (s_autopilot.enabled && !s_autopilot.spawned && frame >= 60)
	{
		s_autopilot.spawned = TRUE;
		autopilotSpawn();
	}
	zhbo_logic_step(frame);
}

Bool Zhbo_IsControlled(ObjectID id)
{
	return s_ready && zhbo_logic_is_controlled((uint32_t)id) != 0;
}

void Zhbo_ClientFrame()
{
	if (!s_ready)
		return;
	const Bool inGame = TheGameLogic->isInGame() && !TheGameLogic->isInShellGame();
	if (!inGame)
	{
		if (s_out.camera_override || s_mouseCaptured || s_rtsUiHidden)
			releaseClientState();
		return;
	}

	autopilotFrame();

	LARGE_INTEGER now, freq;
	QueryPerformanceCounter(&now);
	QueryPerformanceFrequency(&freq);
	const float dt = (float)((double)(now.QuadPart - s_lastTick.QuadPart) / (double)freq.QuadPart);
	s_lastTick = now;

	const Bool focused = GetForegroundWindow() == ApplicationHWnd;
	const Bool menuUp = TheInGameUI->isQuitMenuVisible() || TheGameLogic->isGamePaused();

	ZhboClientIn in;
	memset(&in, 0, sizeof(in));
	in.dt = dt;
	in.local_player = rts::getObservedOrLocalPlayer()->getPlayerIndex();
	in.logic_frame = TheGameLogic->getFrame();
	if (focused && !menuUp)
	{
		for (int vk = 0; vk < 256; ++vk)
			in.keys[vk] = (GetAsyncKeyState(vk) & 0x8000) ? 1 : 0;
	}
	if (s_mouseCaptured && focused)
	{
		POINT c = windowCenter();
		POINT p;
		GetCursorPos(&p);
		in.mouse_dx = (float)(p.x - c.x);
		in.mouse_dy = (float)(p.y - c.y);
		SetCursorPos(c.x, c.y);
	}
	in.wheel = s_wheelAccum;
	s_wheelAccum = 0.0f;
	in.input_allowed = (focused && !menuUp) ? 1 : 0;
	in.screen_w = (float)TheDisplay->getWidth();
	in.screen_h = (float)TheDisplay->getHeight();
	const Coord3D camPos = TheTacticalView->get3DCameraPosition();
	const Coord3D camDir = TheTacticalView->get3DCameraDirection();
	in.rts_cam_pos.x = camPos.x; in.rts_cam_pos.y = camPos.y; in.rts_cam_pos.z = camPos.z;
	in.rts_cam_dir.x = camDir.x; in.rts_cam_dir.y = camDir.y; in.rts_cam_dir.z = camDir.z;

	zhbo_client_frame(&in, &s_out);

	if (s_out.send_input)
	{
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_FPS_INPUT);
		msg->appendObjectIDArgument((ObjectID)s_out.input_general);
		msg->appendIntegerArgument((Int)s_out.input.held);
		msg->appendIntegerArgument((Int)s_out.input.pressed);
		msg->appendIntegerArgument(s_out.input.move_packed);
		msg->appendRealArgument(s_out.input.yaw);
		msg->appendRealArgument(s_out.input.pitch);
	}

	// Never trap the cursor when the game is not the foreground window (Alt-Tab, background tests).
	setMouseCapture(s_out.capture_mouse != 0 && focused && !menuUp);
	setRtsUiHidden(s_out.hide_rts_ui != 0);
	setSkyForced(s_out.camera_override != 0);
	setHiddenObject(s_out.hide_object ? (ObjectID)s_out.hide_object : INVALID_ID);

	if (s_out.rts_look_at_valid)
	{
		Coord3D target;
		target.set(s_out.rts_look_at.x, s_out.rts_look_at.y, s_out.rts_look_at.z);
		TheTacticalView->lookAt(&target);
		s_out.rts_look_at_valid = 0;
	}
}

Bool Zhbo_GetCameraOverride(Coord3D *eye, Coord3D *dir, Real *horizontalFov)
{
	if (!s_ready || !s_out.camera_override)
		return FALSE;
	eye->set(s_out.eye.x, s_out.eye.y, s_out.eye.z);
	dir->set(s_out.dir.x, s_out.dir.y, s_out.dir.z);
	*horizontalFov = s_out.hfov;
	return TRUE;
}

void Zhbo_DrawHud()
{
	if (!s_ready || !TheGameLogic->isInGame() || TheGameLogic->isInShellGame())
		return;
	++s_drawFrame;
	zhbo_client_draw_hud(rts::getObservedOrLocalPlayer()->getPlayerIndex());
	evictTexts();
}

void Zhbo_DrawSky()
{
	if (!s_ready || !s_out.camera_override)
		return;
	const Real w = (Real)TheDisplay->getWidth();
	const Real h = (Real)TheDisplay->getHeight();
	// Where the horizon lands on screen for the current pitch.
	const Real dz = s_out.dir.z < -0.99f ? -0.99f : (s_out.dir.z > 0.99f ? 0.99f : s_out.dir.z);
	const Real tanPitch = dz / sqrtf(1.0f - dz * dz);
	const Real tanHalfV = tanf(s_out.hfov * 0.5f) * h / w;
	Real horizon = h * 0.5f + tanPitch / tanHalfV * h * 0.5f;
	if (horizon < 0.0f) horizon = 0.0f;
	if (horizon > h) horizon = h;

	// Zenith blue to a pale haze at the horizon, then haze below it (terrain covers that part).
	const Int top[3] = { 46, 86, 140 }, low[3] = { 186, 204, 218 };
	const Int bands = 32;
	for (Int i = 0; i < bands; ++i)
	{
		const Real t = (Real)i / (Real)(bands - 1);
		const Int r = top[0] + (Int)((low[0] - top[0]) * t);
		const Int g = top[1] + (Int)((low[1] - top[1]) * t);
		const Int b = top[2] + (Int)((low[2] - top[2]) * t);
		const Real y0 = horizon * i / bands, y1 = horizon * (i + 1) / bands;
		TheDisplay->drawFillRect(0, (Int)y0, (Int)w, (Int)(y1 - y0) + 1, GameMakeColor(r, g, b, 255));
	}
	if (horizon < h)
		TheDisplay->drawFillRect(0, (Int)horizon, (Int)w, (Int)(h - horizon) + 1, GameMakeColor(low[0], low[1], low[2], 255));
}

GameMessageTranslator *Zhbo_CreateTranslator()
{
	return MSGNEW("GameClientSubsystem") ZhboTranslator;
}

void Zhbo_RequestTakeControl(Object *obj)
{
	if (!obj)
		return;
	GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_TAKE_CONTROL);
	msg->appendObjectIDArgument(obj->getID());
}

const WideChar *Zhbo_FetchString(const char *label)
{
	// Usable before Zhbo_Init: the string table is static data in the Rust core.
	return (const WideChar *)zhbo_string(label);
}
