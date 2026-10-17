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
#include "ZhboNames.h"
#include "zhbo_core.h"
#include "ZhboPlatform.h"
#if defined(GGC_BGFX_RENDERER_WGPU)
#include "zhbo_gfx.h"
#endif

#include "Common/AudioEventRTS.h"
#include "Common/ActionManager.h"
#include "Common/BuildAssistant.h"
#include "Common/File.h"
#include "Common/FileSystem.h"
#include "Common/AudioHandleSpecialValues.h"
#include "Common/GameAudio.h"
#include "Common/GameUtility.h"
#include "Common/GlobalData.h"
#include "Common/MessageStream.h"
#include "Common/Player.h"
#include "Common/PlayerList.h"
#include "Common/PlayerTemplate.h"
#include "Common/Radar.h"
#include "Common/Team.h"
#include "Common/ThingFactory.h"
#include "Common/ThingTemplate.h"
#include "GameClient/ControlBar.h"
#include "GameClient/Display.h"
#include "GameClient/DisplayString.h"
#include "GameClient/DisplayStringManager.h"
#include "GameClient/Drawable.h"
#include "GameClient/TerrainVisual.h"
#include "GameClient/FXList.h"
#include "GameClient/GadgetPushButton.h"
#include "GameClient/GameWindow.h"
#include "GameClient/GameWindowManager.h"
#include "Common/NameKeyGenerator.h"
#include "GameClient/GameClient.h"
#include "GameClient/GameFont.h"
#include "GameClient/GUICallbacks.h"
#include "GameClient/Image.h"
#include "GameClient/InGameUI.h"
#include "GameClient/KeyDefs.h"
#include "GameClient/Mouse.h"
#include "GameClient/View.h"
#include "GameLogic/AI.h"
#include "GameLogic/Damage.h"
#include "GameLogic/GameLogic.h"
#include "GameLogic/Module/AIUpdate.h"
#include "GameLogic/Module/BodyModule.h"
#include "GameLogic/Module/DozerAIUpdate.h"
#include "GameLogic/Object.h"
#include "GameLogic/PartitionManager.h"
#include "GameLogic/TerrainLogic.h"
#include "GameLogic/Weapon.h"

#include <algorithm>
#include <map>
#include <set>
#include <string>
#include <vector>

extern "C" void GGC_RequestBgfxFramebufferRebuild(); // BgfxBackend.cpp

// MSG_ZHBO_TAKE_CONTROL's optional second argument.
enum
{
	ZHBO_ACTION_TAKE_CONTROL = 0,
	ZHBO_ACTION_LETHAL = 1,
	ZHBO_ACTION_TACTICAL = 2,
};

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
double s_lastTick = 0.0;
// Smoke log throttle (cbSmoke): at most one line a second.
UnsignedInt s_smokeLastLog = 0;

// The glue's configuration (docs/CONFIG.md), read exactly once in Zhbo_Init: ZHBO_AUTOPILOT is
// parsed here once instead of three times with subtly different modes, and the per-frame getenv
// calls are gone. Mode meanings agree with the Rust core's zhbo-core/src/config.rs.
struct ZhboConfig
{
	Bool autopilot_enabled = FALSE;
	Bool autopilot_manual = FALSE;	  // ZHBO_AUTOPILOT=manual: scene only, input stays with the player
	Bool autopilot_equipment = FALSE; // =equipment: the Black Hat / Trophy System scene
	Bool autopilot_showcase = FALSE;
	Bool autopilot_rts = FALSE;
	Bool autopilot_build = FALSE;
	Bool force_radar = FALSE; // ZHBO_FORCE_RADAR: give the test player radar, to film the Tab map
	const char *showcase_object = nullptr; // ZHBO_SHOWCASE_OBJECT: what to stand in front of the camera
	Real showcase_zoom = 0.12f;			   // ZHBO_SHOWCASE_ZOOM, plus:
	Bool has_showcase_pitch = FALSE;	   // ZHBO_SHOWCASE_PITCH (radians)
	Real showcase_pitch = 0.0f;
	Bool has_showcase_angle = FALSE; // ZHBO_SHOWCASE_ANGLE (radians, around the General)
	Real showcase_angle = 0.0f;
	Bool closeup = FALSE;		 // ZHBO_CLOSEUP: follow the General closely in captures
	Real closeup_zoom = 0.0f;
} s_config;

void readConfig()
{
	s_config = {};
	const char *autopilot = getenv("ZHBO_AUTOPILOT");
	s_config.autopilot_enabled = autopilot != nullptr && autopilot[0] != 0;
	s_config.autopilot_manual = s_config.autopilot_enabled && strcmp(autopilot, "manual") == 0;
	s_config.autopilot_equipment = s_config.autopilot_enabled && strcmp(autopilot, "equipment") == 0;
	s_config.autopilot_showcase = s_config.autopilot_enabled && strcmp(autopilot, "showcase") == 0;
	s_config.autopilot_rts = s_config.autopilot_enabled && strcmp(autopilot, "rts") == 0;
	s_config.autopilot_build = s_config.autopilot_enabled && strcmp(autopilot, "build") == 0;
	s_config.force_radar = getenv("ZHBO_FORCE_RADAR") != nullptr;
	s_config.showcase_object = getenv("ZHBO_SHOWCASE_OBJECT");
	if (const char *v = getenv("ZHBO_SHOWCASE_ZOOM"))
		s_config.showcase_zoom = (Real)atof(v);
	if (const char *v = getenv("ZHBO_SHOWCASE_PITCH"))
	{
		s_config.has_showcase_pitch = TRUE;
		s_config.showcase_pitch = (Real)atof(v);
	}
	if (const char *v = getenv("ZHBO_SHOWCASE_ANGLE"))
	{
		s_config.has_showcase_angle = TRUE;
		s_config.showcase_angle = (Real)atof(v);
	}
	if (const char *v = getenv("ZHBO_CLOSEUP"))
	{
		s_config.closeup = TRUE;
		s_config.closeup_zoom = (Real)atof(v);
	}
}

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
	// The map-less fallback: the caller's generous default bounds (zhbo-core pre-fills them), so a
	// null terrain (shell, tests) clamps nothing instead of crashing.
	Region3D r;
	r.lo.set(-1e6f, -1e6f, -1e6f);
	r.hi.set(1e6f, 1e6f, 1e6f);
	if (TheTerrainLogic)
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
	out->color = owner ? (uint32_t)owner->getPlayerColor() : 0xFFC8C8C8u;

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
	if (obj->isKindOf(KINDOF_DRONE)) flags |= ZHBO_OBJ_DRONE;
	if (obj->isKindOf(KINDOF_PROJECTILE)) flags |= ZHBO_OBJ_PROJECTILE;
	if (obj->testStatus(OBJECT_STATUS_IS_FIRING_WEAPON)) flags |= ZHBO_OBJ_FIRING;
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

// The HUD's query: as the logic's, plus whether the local player's shroud or fog hides each object.
int32_t cbClientQuery(const ZhboVec3 *center, float radius, uint32_t viewerId, ZhboObjInfo *out, int32_t cap)
{
	const Int n = cbQuery(center, radius, viewerId, out, cap);
	const Int local = rts::getObservedOrLocalPlayer()->getPlayerIndex();
	for (Int i = 0; i < n; ++i)
	{
		const Object *obj = TheGameLogic->findObjectByID((ObjectID)out[i].id);
		const ObjectShroudStatus status = obj ? obj->getShroudedStatus(local) : OBJECTSHROUD_CLEAR;
		if (status == OBJECTSHROUD_FOGGED || status == OBJECTSHROUD_SHROUDED)
			out[i].flags |= ZHBO_OBJ_SHROUDED;
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
		// Taking control is the player's order: as for any other, a dozer drops the task in hand
		// (DozerAIUpdate::aiDoCommand). On release, one still pending (a site laid out in first
		// person and left unfinished) is resumed.
		ai->aiIdle(controlled ? CMD_FROM_PLAYER : CMD_FROM_AI);
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
	// The local player's own first-person General: its hidden body sits at the camera, so its
	// tracer and flare would fill the screen; the viewmodel plays its own BO2 muzzle flash instead.
	// Skipping is lockstep-safe: the *_FX weapons deal no damage and have no scatter or fire OCL, so
	// fireWeaponTemplate only plays client-side FX and sound for them.
	if (s_out.camera_override && s_out.hide_object == shooter)
		return;
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

int32_t cbDamage(uint32_t shooter, uint32_t victimId, float amount, int32_t kind)
{
	Object *victim = TheGameLogic->findObjectByID((ObjectID)victimId);
	if (!victim || victim->isEffectivelyDead())
		return 0;
	DamageInfo info;
	// Zero Hour's armour decides what each kind can hurt (explosions hurt vehicles, bullets barely).
	info.in.m_damageType = kind == ZHBO_DAMAGE_MELEE ? DAMAGE_MELEE : kind == ZHBO_DAMAGE_EXPLOSION ? DAMAGE_EXPLOSION : DAMAGE_SMALL_ARMS;
	info.in.m_deathType = DEATH_NORMAL;
	info.in.m_sourceID = (ObjectID)shooter;
	if (const Object *src = TheGameLogic->findObjectByID((ObjectID)shooter))
		if (const Player *p = src->getControllingPlayer())
			info.in.m_sourcePlayerMask = p->getPlayerMask();
	info.in.m_amount = amount;
	victim->attemptDamage(&info);
	return victim->isEffectivelyDead() ? 1 : 0;
}

// A thrown or launched BO2 weapon's object (zhbo_assets: ZHBO_Proj_<weapon>), on the owner's team.
uint32_t cbSpawn(const char *thing, uint32_t owner, const ZhboVec3 *pos, float yaw)
{
	const ThingTemplate *tmpl = TheThingFactory ? TheThingFactory->findTemplate(thing) : nullptr;
	Object *src = TheGameLogic->findObjectByID((ObjectID)owner);
	Team *team = src ? src->getTeam() : nullptr;
	if (!tmpl || !team)
		return 0;
	Object *obj = TheThingFactory->newObject(tmpl, team);
	if (!obj)
		return 0;
	Coord3D p;
	p.set(pos->x, pos->y, pos->z);
	obj->setPosition(&p);
	obj->setOrientation(yaw);
	return (uint32_t)obj->getID();
}

void cbDestroy(uint32_t id)
{
	if (Object *obj = TheGameLogic->findObjectByID((ObjectID)id))
		TheGameLogic->destroyObject(obj);
}

// Concussion, flash and shock charges paralyse; the EMP grenade shuts vehicles and buildings down.
void cbDisable(uint32_t id, int32_t kind, float seconds)
{
	Object *obj = TheGameLogic->findObjectByID((ObjectID)id);
	if (!obj || obj->isEffectivelyDead() || seconds <= 0.0f)
		return;
	// Clamp before the float->int cast: an absurd `seconds` (a data error) would overflow it (UB).
	seconds = std::min(seconds, 3600.0f);
	const UnsignedInt until = TheGameLogic->getFrame() + (UnsignedInt)(seconds * LOGICFRAMES_PER_SECOND);
	obj->setDisabledUntil(kind == ZHBO_DISABLE_EMP ? DISABLED_EMP : DISABLED_PARALYZED, until);
}

void cbFx(const char *name, const ZhboVec3 *pos)
{
	const FXList *fx = TheFXListStore ? TheFXListStore->findFXList(name) : nullptr;
	if (!fx)
		return;
	Coord3D p;
	p.set(pos->x, pos->y, pos->z);
	FXList::doFXPos(fx, &p);
}

// The General under RTS control: Zero Hour's AI fights with the loadout's weapons (PRIMARY rifle,
// SECONDARY pistol, TERTIARY lethal), but the ammo is the General's own in zhbo-core. Each logic
// frame every shot is counted against it, the slot the player picked is reported, an empty weapon
// can't fire, and the General is moved off it to one with ammo.
Bool isGeneral(const Object *obj)
{
	return obj && zhbo_names::isGeneral(obj->getTemplate()->getName().str());
}

// The overhead General's BO2 bullet impacts, by gun slot: on a soldier, a vehicle, anything else
// (zhbo_assets::generals::RTS_FX_PATH).
struct RtsHitFx
{
	AsciiString flesh, metal, hard;
};
RtsHitFx s_rtsHitFx[2];
Bool s_rtsHitFxLoaded = FALSE;

void loadRtsHitFx()
{
	s_rtsHitFxLoaded = TRUE;
	File *f = TheFileSystem ? TheFileSystem->openFile(zhbo_names::RTS_FX_PATH, File::READ | File::BINARY) : nullptr;
	if (!f)
		return;
	const Int size = f->size();
	char *buf = f->readEntireAndClose();
	if (!buf)
		return;
	std::string text(buf, size);
	delete[] buf;
	size_t at = 0;
	while (at < text.size())
	{
		size_t end = text.find('\n', at);
		if (end == std::string::npos)
			end = text.size();
		const std::string line = text.substr(at, end - at);
		at = end + 1;
		int slot;
		char flesh[128], metal[128], hard[128];
		if (line.empty() || line[0] == ';' || sscanf(line.c_str(), "%d %127s %127s %127s", &slot, flesh, metal, hard) != 4 || slot < 0 || slot > 1)
			continue;
		s_rtsHitFx[slot].flesh = flesh;
		s_rtsHitFx[slot].metal = metal;
		s_rtsHitFx[slot].hard = hard;
	}
}

// BO2's impact for a shot from gun slot `slot` landing on `victim`.
void playRtsHit(Int slot, const Object *victim)
{
	if (!s_rtsHitFxLoaded)
		loadRtsHitFx();
	if (slot < 0 || slot > 1 || !victim || !TheFXListStore)
		return;
	const RtsHitFx &h = s_rtsHitFx[slot];
	const AsciiString &name = victim->isKindOf(KINDOF_INFANTRY) ? h.flesh : victim->isKindOf(KINDOF_VEHICLE) ? h.metal : h.hard;
	const FXList *fx = name.isEmpty() || name == "NONE" ? nullptr : TheFXListStore->findFXList(name.str());
	if (!fx)
		return;
	Coord3D at;
	victim->getGeometryInfo().getCenterPosition(*victim->getPosition(), at);
	FXList::doFXPos(fx, &at);
}

struct RtsShots
{
	Bool seen;
	UnsignedInt last[3];
	Int slot;
	// Whom the General was last attacking, and when (to keep attacking after a forced switch).
	ObjectID victim;
	UnsignedInt victimFrame;
};
std::map<ObjectID, RtsShots> s_rtsShots;

void rtsWeapons(UnsignedInt frame)
{
	for (Object *obj = TheGameLogic->getFirstObject(); obj; obj = obj->getNextObject())
	{
		if (!isGeneral(obj) || obj->isEffectivelyDead() || !obj->getControllingPlayer())
			continue;
		if (s_rtsShots.find(obj->getID()) == s_rtsShots.end())
		{
			s_rtsShots[obj->getID()] = RtsShots();
			s_rtsShots[obj->getID()].slot = -1;
		}
		RtsShots &seen = s_rtsShots[obj->getID()];
		AIUpdateInterface *ai = obj->getAI();
		if (const Object *victim = ai ? ai->getCurrentVictim() : nullptr)
		{
			seen.victim = victim->getID();
			seen.victimFrame = frame;
		}
		uint32_t shots[3] = { 0, 0, 0 };
		for (Int s = 0; s < 3; ++s)
		{
			const Weapon *w = obj->getWeaponInWeaponSlot((WeaponSlotType)s);
			const UnsignedInt last = w ? w->getLastShotFrame() : 0;
			if (seen.seen && last != seen.last[s] && last != 0)
				shots[s] = 1;
			seen.last[s] = last;
		}
		seen.seen = TRUE;
		WeaponSlotType current = PRIMARY_WEAPON;
		obj->getCurrentWeapon(&current);
		ZhboRtsWeapons ammo = {}; // zeroed: the Rust call may early-return without filling it
		// Where a grenade from the third slot goes: what the AI is attacking (a unit or a spot).
		ZhboVec3 target;
		const ZhboVec3 *aim = nullptr;
		if (const Object *victim = ai ? ai->getCurrentVictim() : nullptr)
		{
			target.x = victim->getPosition()->x; target.y = victim->getPosition()->y; target.z = victim->getPosition()->z;
			aim = &target;
		}
		else if (const Coord3D *spot = ai ? ai->getCurrentVictimPos() : nullptr)
		{
			target.x = spot->x; target.y = spot->y; target.z = spot->z;
			aim = &target;
		}
		zhbo_logic_rts_update(obj->getControllingPlayer()->getPlayerIndex(), (uint32_t)obj->getID(), (int32_t)current, shots, aim, &ammo);
		if (const Object *victim = ai ? ai->getCurrentVictim() : nullptr)
		{
			for (Int s = 0; s < 2; ++s)
				if (shots[s])
					playRtsHit(s, victim);
		}
		for (Int s = 0; s < 3; ++s)
		{
			// The log is the test's oracle: every lethal and pistol shot, every tenth rifle round.
			if (shots[s] && (s > 0 || ammo.ammo[s] % 10 == 0))
				logf("General %u fired slot %d (%d/%d left)", (unsigned)obj->getID(), s, ammo.ammo[s], ammo.reserve[s]);
		}
		if (seen.slot != (Int)current)
		{
			logf("General %u uses weapon slot %d", (unsigned)obj->getID(), (Int)current);
			seen.slot = (Int)current;
		}
		if (ammo.lock >= 0 && ammo.lock < 3)
		{
			obj->setWeaponLock((WeaponSlotType)ammo.lock, LOCKED_PERMANENTLY);
			logf("General %u locked to weapon slot %d", (unsigned)obj->getID(), ammo.lock);
			// A weapon that ran dry hands over mid-fight: Zero Hour drops an attack whose target is
			// out of the new weapon's range, so order it again (he closes in with the pistol).
			Object *victim = TheGameLogic->findObjectByID(seen.victim);
			if (victim && (victim->isEffectivelyDead() || frame - seen.victimFrame >= 90))
				victim = nullptr;
			// Between targets (idle), the new gun's shorter reach would leave enemies the empty one
			// could hit unanswered: take on the closest one it could have.
			if (!victim && ai && ai->isIdle() && ammo.lock != (Int)current)
			{
				const Weapon *was = obj->getWeaponInWeaponSlot(current);
				victim = TheAI->findClosestEnemy(obj, was ? was->getAttackRange(obj) : 175.0f, AI::CAN_SEE | AI::CAN_ATTACK);
			}
			if (ai && victim && ammo.lock != (Int)current)
			{
				ai->aiAttackObject(victim, NO_MAX_SHOTS_LIMIT, ai->getLastCommandSource());
				logf("General %u keeps attacking %u", (unsigned)obj->getID(), (unsigned)victim->getID());
			}
		}
		for (Int s = 0; s < 3; ++s)
		{
			Weapon *w = obj->getWeaponInWeaponSlot((WeaponSlotType)s);
			if (w && ammo.ammo[s] + ammo.reserve[s] <= 0)
				w->setPossibleNextShotFrame(frame + 2);
		}
	}
}

void cbFillRect(float x, float y, float w, float h, uint32_t argb);
void cbText(float x, float y, const char *utf8, uint32_t argb, int32_t size, int32_t align);

// The ammo left on each of the selected General's weapon buttons (drawn over the command bar).
// The General's tactical grenade button (it shares the third slot with the lethal's).
Bool isTacticalButton(const CommandButton *button)
{
	return button && button->getName() == "Command_ZHBO_SwitchToTactical";
}

void drawRtsAmmo()
{
	if (!TheInGameUI || TheInGameUI->getSelectCount() != 1 || !TheWindowManager)
		return;
	const Drawable *draw = TheInGameUI->getFirstSelectedDrawable();
	const Object *obj = draw ? draw->getObject() : nullptr;
	if (!isGeneral(obj))
		return;
	ZhboRtsWeapons ammo = {};
	zhbo_client_rts_ammo((uint32_t)obj->getID(), &ammo);
	for (Int i = 1; i <= 14; ++i)
	{
		char name[64];
		snprintf(name, sizeof(name), "ControlBar.wnd:ButtonCommand%02d", i);
		GameWindow *win = TheWindowManager->winGetWindowFromId(nullptr, TheNameKeyGenerator->nameToKey(name));
		if (!win || win->winIsHidden())
			continue;
		const CommandButton *button = (const CommandButton *)GadgetButtonGetData(win);
		if (!button || button->getCommandType() != GUI_COMMAND_SWITCH_WEAPON)
			continue;
		const Int slot = (Int)button->getWeaponSlot();
		if (slot < 0 || slot > 2)
			continue;
		Int x, y, w, h;
		win->winGetScreenPosition(&x, &y);
		win->winGetSize(&w, &h);
		char text[32];
		Bool empty;
		if (slot == 2)
		{
			const Int count = isTacticalButton(button) ? ammo.tactical : ammo.ammo[2];
			snprintf(text, sizeof(text), "%d", count);
			empty = count <= 0;
		}
		else
		{
			snprintf(text, sizeof(text), "%d/%d", ammo.ammo[slot], ammo.reserve[slot]);
			empty = ammo.ammo[slot] + ammo.reserve[slot] <= 0;
		}
		const Real band = (Real)h * 0.3f;
		cbFillRect((float)x + 2.0f, (float)(y + h) - band - 1.0f, (float)w - 4.0f, band, 0xB0000000u);
		cbText((float)(x + w) - 4.0f, (float)(y + h) - band - 1.0f, text, empty ? 0xFFE83A2Eu : 0xFFFFFFFFu, (int32_t)(band * 0.85f), 2);
	}
}

// What each controlled General was last shown holding (applyWorldPoses).
struct WorldLook
{
	int32_t weapon;
	int32_t tablet;
	int32_t sidearm;
};
std::map<uint32_t, WorldLook> s_worldLooks;

// A smoke screen (BO2's smoke grenade) this frame: no one shoots into or out of it. Anything
// attacking with its attacker or its victim in the cloud goes back to idle (and picks another
// target it can see).
void cbSmoke(const ZhboVec3 *at, float radius)
{
	Coord3D center;
	center.set(at->x, at->y, at->z);
	SimpleObjectIterator *iter = ThePartitionManager->iterateObjectsInRange(&center, radius + 400.0f, FROM_CENTER_2D);
	MemoryPoolObjectHolder hold(iter);
	const Real r2 = radius * radius;
	Int blinded = 0;
	auto inside = [&](const Object *o)
	{
		const Coord3D *p = o->getPosition();
		const Real dx = p->x - center.x, dy = p->y - center.y;
		return dx * dx + dy * dy <= r2;
	};
	for (Object *obj = iter->first(); obj; obj = iter->next())
	{
		AIUpdateInterface *ai = obj->getAI();
		if (!ai || obj->isEffectivelyDead() || zhbo_logic_is_controlled((uint32_t)obj->getID()))
			continue;
		const Object *victim = ai->getCurrentVictim();
		if (victim && (inside(obj) || inside(victim)))
		{
			ai->aiIdle(CMD_FROM_AI);
			++blinded;
		}
	}
	// The log is the test's oracle (at most once a second).
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (blinded > 0 && frame >= s_smokeLastLog + 30)
	{
		s_smokeLastLog = frame;
		logf("smoke: %d attackers can't see through it", blinded);
	}
}

// The Black Hat: equipment changes team; a unit (a drone) defects.
void cbCapture(uint32_t id, uint32_t newOwner)
{
	Object *obj = TheGameLogic->findObjectByID((ObjectID)id);
	Object *owner = TheGameLogic->findObjectByID((ObjectID)newOwner);
	if (!obj || !owner || !owner->getTeam() || obj->isEffectivelyDead())
		return;
	if (obj->isKindOf(KINDOF_INERT))
		obj->setTeam(owner->getTeam());
	else
		obj->defect(owner->getTeam(), 0);
	logf("object %u captured by General %u", id, newOwner);
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

// Image-name lookups for the HUD (cbImage), including misses; TheMappedImageCollection resets per
// game, so the pointers go stale and the cache is cleared in Zhbo_Reset.
std::map<std::string, const Image *> s_images;

// Sound events already reported once (cbSound); cleared in Zhbo_Reset.
std::set<std::string> s_soundReported;

// A BO2 sound (zhbo_core::sounds): positional when `at` is given, else the player's own 2D sound.
void cbSound(const char *event, const ZhboVec3 *at)
{
	if (!TheAudio || !event)
		return;
	AudioEventRTS sound((AsciiString(event)));
	if (at)
	{
		Coord3D pos;
		pos.set(at->x, at->y, at->z);
		sound.setPosition(&pos);
	}
	const AudioHandle handle = TheAudio->addAudioEvent(&sound);
	// Report each event once: a handle below AHSV_FirstHandle is an error (unknown event, no sound).
	if (s_soundReported.insert(event).second)
		logf("sound %s: %s", event, handle >= AHSV_FirstHandle ? "playing" : handle == AHSV_Muted ? "muted" : handle == AHSV_NoSound ? "no sound" : "error");
}

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

const char *hudFontName()
{
	static const char *name = nullptr;
	if (!name)
	{
		name = zhbo_platform::hudFontName();
		logf("HUD font: %s", name);
	}
	return name;
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
	ds->setFont(TheFontLibrary->getFont(hudFontName(), size, TRUE));
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
	// Half the alpha as the shadow's alpha, computed on the byte so an odd alpha's low bit cannot
	// bleed into the red channel.
	const UnsignedInt shadow = (((argb >> 24) >> 1) & 0xFFu) << 24;
	ds->draw(px, (Int)y, argb, shadow);
}

int32_t cbImage(const char *name_, float x, float y, float w, float h, uint32_t argb)
{
	// "ZHBO_TeamLogo" is the local player's faction emblem from Zero Hour's own art (USA_Logo,
	// China_Logo, GLA_Logo), by the base side of their general.
	std::string name = name_;
	if (name == "ZHBO_TeamLogo")
	{
		const Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
		const PlayerTemplate *pt = local ? local->getPlayerTemplate() : nullptr;
		if (!pt || pt->getBaseSide().isEmpty())
			return 0;
		name = std::string(pt->getBaseSide().str()) + "_Logo";
	}
	// Mapped images are reset per game (TheMappedImageCollection), so the cache must not outlive
	// one: it is cleared in Zhbo_Reset (s_images), including misses.
	std::map<std::string, const Image *>::iterator it = s_images.find(name);
	const Image *image;
	if (it == s_images.end())
	{
		image = TheMappedImageCollection ? TheMappedImageCollection->findImageByName(name.c_str()) : nullptr;
		s_images[name] = image;
	}
	else
	{
		image = it->second;
	}
	if (!image)
		return 0;
	TheDisplay->drawImage(image, (Int)x, (Int)y, (Int)(x + w + 0.5f), (Int)(y + h + 0.5f), argb);
	return 1;
}

void cbClip(float x, float y, float w, float h)
{
	if (!TheDisplay)
		return;
	if (w <= 0.0f || h <= 0.0f)
	{
		TheDisplay->enableClipping(FALSE);
		return;
	}
	IRegion2D region;
	region.lo.x = (Int)x;
	region.lo.y = (Int)y;
	region.hi.x = (Int)(x + w + 0.5f);
	region.hi.y = (Int)(y + h + 0.5f);
	TheDisplay->setClipRegion(&region);
	TheDisplay->enableClipping(TRUE);
}

int32_t cbRadar(float x, float y, float w, float h, const ZhboVec3 *you, ZhboVec3 *youPx)
{
	// The same rule as the control bar's radar window (W3DLeftHUDDraw).
	if (!TheRadar || !rts::localPlayerHasRadar())
		return 0;
	// No tactical-view box: in first person it collapses to a stray line (patch 0005).
	TheRadar->setDrawViewBox(FALSE);
	TheRadar->draw((Int)x, (Int)y, (Int)w, (Int)h);
	TheRadar->setDrawViewBox(TRUE);
	// Where `you` lands: radar cells inside the aspect-preserving square the radar drew in.
	ICoord2D ul, lr, cell;
	TheRadar->findDrawPositions((Int)x, (Int)y, (Int)w, (Int)h, &ul, &lr);
	Coord3D world;
	world.set(you->x, you->y, you->z);
	if (youPx && TheRadar->worldToRadar(&world, &cell))
	{
		youPx->x = ul.x + (cell.x + 0.5f) * (lr.x - ul.x) / (Real)RADAR_CELL_WIDTH;
		youPx->y = lr.y - (cell.y + 0.5f) * (lr.y - ul.y) / (Real)RADAR_CELL_HEIGHT;
	}
	return 1;
}

int32_t cbCash(int32_t player)
{
	Player *p = ThePlayerList ? ThePlayerList->getNthPlayer(player) : nullptr;
	return p ? (int32_t)p->getMoney()->countMoney() : 0;
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

// Seconds left to keep Zero Hour's mouse in step with the system cursor after first person.
double s_reclaimMouseUntil = 0.0;

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

// --------------------------------------------------------------------------------------------
// Autopilot (ZHBO_AUTOPILOT=1): an in-game test scene. Single-player only; it spawns objects from
// the client, which is fine for a local test but would desync a network game.
// --------------------------------------------------------------------------------------------

struct Autopilot
{
	Bool enabled;
	// ZHBO_AUTOPILOT=showcase: the General alone, filmed close up in RTS (idle, a walk, death).
	Bool showcase;
	// ZHBO_AUTOPILOT=rts: the General stays under RTS control and is selected; the script picks
	// the lethal, then the pistol, from the command bar, then takes control.
	Bool rts;
	// ZHBO_AUTOPILOT=build: the General alone; the script opens his Build menu, orders a structure
	// and goes back to his commands (his building is logged as it completes). Then in first person
	// he builds another, and repairs it once the script damages it; back overhead, he is ordered
	// to repair it again.
	Bool build;
	Int buildPhase; // BuildPhase
	ObjectID built; // the structure built in first person
	UnsignedInt phaseFrame;
	Real damagedHealth;
	UnsignedInt spawnFrame;
	Bool spawned;
	ObjectID general;
	UnsignedInt releasedFrame;
	Int retakes;
	// Step machines of the scripted tests, so Zhbo_Reset (memset) can restart them in the same
	// process (function-local statics could not).
	UnsignedInt buildStep; // buildAutopilotFrame
	UnsignedInt rtsDone;	 // rtsAutopilotFrame
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
	if (s_config.force_radar && TheRadar)
		TheRadar->forceOn(local->getPlayerIndex(), TRUE);
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
	Object *general = spawnAt(s_autopilot.showcase && s_config.showcase_object ? s_config.showcase_object : "ZHBO_General_USA", local->getDefaultTeam(), pos, yaw);
	if (!general)
		return;
	s_autopilot.general = general->getID();
	// Test only: enough health that the scripted run (weapons, grenades, field computer) finishes
	// before the rebels win.
	if (!s_autopilot.showcase)
		if (BodyModuleInterface *body = general->getBodyModule())
			body->setMaxHealth(body->getMaxHealth() * 3.0f, FULLY_HEAL);
	if (s_autopilot.showcase)
	{
		logf("autopilot: showcase General %u at (%.0f, %.0f)", (unsigned)s_autopilot.general, pos.x, pos.y);
		return;
	}

	// Three enemy riflemen ahead of the General (inside rifle range, outside the base).
	if (enemy && !s_autopilot.build)
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
	// ZHBO_AUTOPILOT=equipment: an enemy Sentry Drone in front for the Black Hat, and two rocket
	// troopers firing at the General for the Trophy System.
	if (enemy && s_config.autopilot_equipment)
	{
		Coord3D d;
		d.set(pos.x + c * 45.0f, pos.y + s * 45.0f, 0.0f);
		Object *drone = spawnAt("AmericaVehicleSentryDrone", enemy->getDefaultTeam(), d, yaw + PI);
		for (Int i = -1; i <= 1; i += 2)
		{
			Coord3D e;
			e.set(pos.x + c * 110.0f - s * 30.0f * i, pos.y + s * 110.0f + c * 30.0f * i, 0.0f);
			Object *rpg = spawnAt("GLAInfantryTunnelDefender", enemy->getDefaultTeam(), e, yaw + PI);
			if (rpg && rpg->getAIUpdateInterface())
				rpg->getAIUpdateInterface()->aiAttackObject(general, NO_MAX_SHOTS_LIMIT, CMD_FROM_AI);
		}
		logf("autopilot: equipment scene: Sentry Drone %u and two rocket troopers", drone ? (unsigned)drone->getID() : 0u);
	}
	logf("autopilot: spawned General %u at (%.0f, %.0f), enemies: %s", (unsigned)s_autopilot.general, pos.x, pos.y,
		enemy ? "3 GLA rebels" : "none (no enemy player)");
	s_autopilot.spawnFrame = TheGameLogic->getFrame();
	// ZHBO_AUTOPILOT=manual: the scene only; the player starts overhead and does the rest.
	const Bool manual = mode && strcmp(mode, "manual") == 0;
	if (!s_autopilot.rts && !manual && !s_autopilot.build)
		Zhbo_RequestTakeControl(general);
}

// Showcase: keep the camera close on the General; walk out and back, then die.
void showcaseFrame(UnsignedInt frame)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general)
		return;
	const Coord3D *pos = general->getPosition();
	TheTacticalView->setZoomLimited(FALSE);
	TheTacticalView->lookAt(pos);
	TheTacticalView->setZoom(s_config.showcase_zoom);
	if (s_config.has_showcase_pitch)
		TheTacticalView->setPitch(s_config.showcase_pitch);
	if (s_config.has_showcase_angle)
		TheTacticalView->setAngle(general->getOrientation() + s_config.showcase_angle);
	AIUpdateInterface *ai = general->getAIUpdateInterface();
	const Real yaw = general->getOrientation();
	if (ai && (frame == 180 || frame == 300))
	{
		const Real d = frame == 180 ? 60.0f : -60.0f;
		Coord3D dest;
		dest.set(pos->x + Cos(yaw) * d, pos->y + Sin(yaw) * d, pos->z);
		ai->aiMoveToPosition(&dest, CMD_FROM_AI);
		logf("showcase: walking %s", d > 0 ? "out" : "back");
	}
	if (frame == 480 && !general->isEffectivelyDead())
	{
		logLine("showcase: killing the General");
		general->kill();
	}
}

// The RTS script: what a player does with the General's command bar.
// Fresh rebels in front of the General for the RTS script's lethal and pistol (the first ones are
// dead by then). Called from the logic step: objects made outside it crash when they fire.
void rtsAutopilotWave(UnsignedInt frame)
{
	const UnsignedInt t = frame - s_autopilot.spawnFrame;
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general || (t != 230 && t != 410))
		return;
	Player *local = ThePlayerList->getLocalPlayer();
	for (Int p = 0; p < ThePlayerList->getPlayerCount(); ++p)
	{
		Player *enemy = ThePlayerList->getNthPlayer(p);
		if (enemy == local || !enemy->getPlayerTemplate() || local->getRelationship(enemy->getDefaultTeam()) != ENEMIES)
			continue;
		const Real yaw = general->getOrientation();
		const Coord3D *g = general->getPosition();
		for (Int k = -1; k <= 1; ++k)
		{
			Coord3D e;
			e.set(g->x + Cos(yaw) * 90.0f - Sin(yaw) * 10.0f * k, g->y + Sin(yaw) * 90.0f + Cos(yaw) * 10.0f * k, 0.0f);
			spawnAt("GLAInfantryRebel", enemy->getDefaultTeam(), e, yaw + PI);
		}
		logLine("autopilot: three more rebels");
		break;
	}
}

// ZHBO_AUTOPILOT=build: select the General, open his Build menu, order the first structure in it
// he can build (somewhere legal near him) as the command bar's placement would, then press Back.
void buildAutopilotFrame(UnsignedInt frame)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general || !general->getDrawable())
		return;
	const UnsignedInt t = frame - s_autopilot.spawnFrame;
	UnsignedInt &s_step = s_autopilot.buildStep;
	if (t >= 15 && s_step < 1)
	{
		s_step = 1;
		TheInGameUI->deselectAllDrawables();
		TheInGameUI->selectDrawable(general->getDrawable());
		GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
		group->appendBooleanArgument(TRUE);
		group->appendObjectIDArgument(general->getID());
	}
	if (t >= 30 && s_step < 2)
	{
		s_step = 2;
		Zhbo_OnZhboCommand(general, TheControlBar->findCommandButton("Command_ZHBO_BuildMenu"));
		const CommandSet *set = TheControlBar->findCommandSet(general->getCommandSetString());
		AsciiString names;
		for (Int i = 0; set && i < MAX_COMMANDS_PER_SET; ++i)
			if (const CommandButton *b = set->getCommandButton(i))
			{
				names.concat(b->getName());
				names.concat(" ");
			}
		logf("autopilot: build menu %s: %s", general->getCommandSetString().str(), names.str());
	}
	if (t >= 45 && s_step < 3)
	{
		s_step = 3;
		const CommandSet *set = TheControlBar->findCommandSet(general->getCommandSetString());
		Player *player = general->getControllingPlayer();
		for (Int i = 0; set && i < MAX_COMMANDS_PER_SET; ++i)
		{
			const CommandButton *b = set->getCommandButton(i);
			const ThingTemplate *what = b && b->getCommandType() == GUI_COMMAND_DOZER_CONSTRUCT ? b->getThingTemplate() : nullptr;
			if (!what || TheBuildAssistant->canMakeUnit(general, what) != CANMAKE_OK)
				continue;
			const Coord3D *at = general->getPosition();
			for (Int ring = 0; ring < 24; ++ring)
			{
				const Real a = ring * (PI / 6.0f), d = 90.0f + 30.0f * (ring / 12);
				Coord3D pos;
				pos.set(at->x + Cos(a) * d, at->y + Sin(a) * d, at->z);
				const UnsignedInt checks = BuildAssistant::TERRAIN_RESTRICTIONS | BuildAssistant::CLEAR_PATH | BuildAssistant::NO_OBJECT_OVERLAP;
				if (TheBuildAssistant->isLocationLegalToBuild(&pos, what, 0.0f, checks, general, player) != LBC_OK)
					continue;
				GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DOZER_CONSTRUCT);
				msg->appendIntegerArgument(what->getTemplateID());
				msg->appendLocationArgument(pos);
				msg->appendRealArgument(0.0f);
				logf("autopilot: ordered %s at (%.0f, %.0f)", what->getName().str(), pos.x, pos.y);
				// The legality checks mark what was in the way; the placement UI would clear them.
				TheTerrainVisual->removeAllBibs();
				i = MAX_COMMANDS_PER_SET;
				break;
			}
		}
	}
	if (t >= 60 && s_step < 4)
	{
		s_step = 4;
		Zhbo_OnZhboCommand(general, TheControlBar->findCommandButton("Command_ZHBO_BuildBack"));
	}
}

enum BuildPhase
{
	BUILD_OVERHEAD,		// ordered from his Build menu
	BUILD_FPS,				// laid out with the hammer, built by holding the use key
	REPAIR_FPS,				// damaged by the script; repaired by holding the use key
	REPAIR_OVERHEAD,	// damaged again and ordered repaired, as a right-click would
	BUILD_TEST_DONE
};

// The script knocks this share of its health off the structure the General built.
void damageForTest(Object *obj, Real share)
{
	BodyModuleInterface *body = obj->getBodyModule();
	body->internalChangeHealth(-body->getMaxHealth() * share);
	s_autopilot.damagedHealth = body->getHealth();
	logf("autopilot: damaged %s to %.0f%%", obj->getTemplate()->getName().str(), body->getHealth() / body->getMaxHealth() * 100.0f);
}

// ZHBO_AUTOPILOT=build: a structure the General built is done; on to the next step.
void buildTestStructureDone(Object *obj)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general)
		return;
	if (s_autopilot.buildPhase == BUILD_OVERHEAD)
	{
		// Built from overhead; now the same in first person, facing open ground where the same
		// structure could go (as near as the hammer puts it). His own workers wander about: if
		// there is none where he stands, he is moved to where there is.
		s_autopilot.buildPhase = BUILD_FPS;
		const ThingTemplate *what = obj->getTemplate();
		const Real ahead = std::max(50.0f, what->getTemplateGeometryInfo().getBoundingCircleRadius() + 15.0f);
		const UnsignedInt checks = BuildAssistant::TERRAIN_RESTRICTIONS | BuildAssistant::CLEAR_PATH
			| BuildAssistant::NO_OBJECT_OVERLAP | BuildAssistant::SHROUD_REVEALED;
		const Coord3D start = *general->getPosition();
		Bool found = FALSE;
		for (Int ring = 0; ring < 4 && !found; ++ring)
		{
			for (Int spot = 0; spot < (ring ? 8 : 1) && !found; ++spot)
			{
				Coord3D g = start;
				g.x += Cos(spot * (PI / 4.0f)) * 100.0f * ring;
				g.y += Sin(spot * (PI / 4.0f)) * 100.0f * ring;
				g.z = TheTerrainLogic->getGroundHeight(g.x, g.y);
				for (Int k = 0; k < 24 && !found; ++k)
				{
					const Real a = k * (PI / 12.0f);
					Coord3D at;
					at.set(g.x + Cos(a) * ahead, g.y + Sin(a) * ahead, g.z);
					// The hammer turns the front towards him, and the script turns it a notch or two.
					found = TRUE;
					for (Int turn = 1; turn <= 2 && found; ++turn)
						found = TheBuildAssistant->isLocationLegalToBuild(&at, what, a + PI + turn * (PI / 12.0f), checks, general, nullptr) == LBC_OK;
					if (found)
					{
						general->setPosition(&g);
						general->setOrientation(a);
						logf("autopilot: facing open ground at %d degrees, %d from where he built", k * 15, ring * 100);
					}
				}
			}
		}
		TheTerrainVisual->removeAllBibs();
		if (!found)
			logLine("autopilot: no open ground around the General");
		logLine("autopilot: taking control to build in first person");
		Zhbo_RequestTakeControl(general);
	}
	else if (s_autopilot.buildPhase == BUILD_FPS)
	{
		s_autopilot.buildPhase = REPAIR_FPS;
		s_autopilot.built = obj->getID();
		damageForTest(obj, 0.3f);
	}
}

// ZHBO_AUTOPILOT=build, after the first-person repair: back overhead, damage it again and order
// the General to repair it; log how long that took against a dozer's time.
void repairTestFrame(UnsignedInt frame)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	Object *built = TheGameLogic->findObjectByID(s_autopilot.built);
	if (!general || !built || !general->getDrawable())
		return;
	BodyModuleInterface *body = built->getBodyModule();
	if (s_autopilot.buildPhase == REPAIR_FPS)
	{
		if (zhbo_logic_is_controlled((uint32_t)general->getID()))
		{
			s_autopilot.phaseFrame = 0;
			return;
		}
		if (s_autopilot.phaseFrame == 0)
			s_autopilot.phaseFrame = frame;
		if (frame - s_autopilot.phaseFrame < 3 * LOGICFRAMES_PER_SECOND)
			return;
		s_autopilot.buildPhase = REPAIR_OVERHEAD;
		s_autopilot.phaseFrame = 0;
		damageForTest(built, 0.15f);
		TheInGameUI->deselectAllDrawables();
		TheInGameUI->selectDrawable(general->getDrawable());
		GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
		group->appendBooleanArgument(TRUE);
		group->appendObjectIDArgument(general->getID());
		GameMessage *repair = TheMessageStream->appendMessage(GameMessage::MSG_DO_REPAIR);
		repair->appendObjectIDArgument(built->getID());
		logf("autopilot: ordered the General to repair %s", built->getTemplate()->getName().str());
		return;
	}
	if (s_autopilot.buildPhase != REPAIR_OVERHEAD)
		return;
	// Timed from the first health he gives it (after the walk).
	if (s_autopilot.phaseFrame == 0)
	{
		if (body->getHealth() > s_autopilot.damagedHealth)
			s_autopilot.phaseFrame = frame;
		return;
	}
	if (body->getHealth() < body->getMaxHealth())
		return;
	s_autopilot.buildPhase = BUILD_TEST_DONE;
	const DozerAIInterface *dozer = general->getAI() ? general->getAI()->getDozerAIInterface() : nullptr;
	const Real secs = (frame - s_autopilot.phaseFrame) / (Real)LOGICFRAMES_PER_SECOND;
	const Real dozerSecs = dozer ? 0.15f / dozer->getRepairHealthPerSecond() : 0.0f;
	logf("General repaired %s overhead in %.1f s (a dozer: %.1f s)", built->getTemplate()->getName().str(), secs, dozerSecs);
}

// Structures Generals are building: when each started, to log how long it took.
std::map<ObjectID, UnsignedInt> s_generalBuilds;

void trackGeneralBuilds(UnsignedInt frame)
{
	for (Object *obj = TheGameLogic->getFirstObject(); obj; obj = obj->getNextObject())
	{
		if (!obj->isKindOf(KINDOF_STRUCTURE))
			continue;
		const Object *builder = TheGameLogic->findObjectByID(obj->getBuilderID());
		std::map<ObjectID, UnsignedInt>::iterator it = s_generalBuilds.find(obj->getID());
		if (obj->testStatus(OBJECT_STATUS_UNDER_CONSTRUCTION))
		{
			if (it == s_generalBuilds.end() && isGeneral(builder))
			{
				s_generalBuilds[obj->getID()] = frame;
				logf("General %u started building %s", (unsigned)builder->getID(), obj->getTemplate()->getName().str());
			}
		}
		else if (it != s_generalBuilds.end())
		{
			const Real secs = (frame - it->second) / (Real)LOGICFRAMES_PER_SECOND;
			const Real dozer = obj->getTemplate()->calcTimeToBuild(obj->getControllingPlayer()) / (Real)LOGICFRAMES_PER_SECOND;
			logf("General finished building %s in %.1f s (a dozer: %.1f s)", obj->getTemplate()->getName().str(), secs, dozer);
			s_generalBuilds.erase(it);
			if (s_autopilot.build)
				buildTestStructureDone(obj);
		}
	}
}

void rtsAutopilotFrame(UnsignedInt frame)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general || !general->getDrawable())
		return;
	const UnsignedInt t = frame - s_autopilot.spawnFrame;
	// ZHBO_CLOSEUP: follow the General closely, to see his animations and effects in captures.
	if (s_config.closeup)
	{
		TheTacticalView->setZoomLimited(FALSE);
		TheTacticalView->lookAt(general->getPosition());
		TheTacticalView->setZoom(s_config.closeup_zoom);
	}
	UnsignedInt &s_done = s_autopilot.rtsDone;
	if (t >= 15 && s_done < 1)
	{
		s_done = 1;
		TheInGameUI->deselectAllDrawables();
		TheInGameUI->selectDrawable(general->getDrawable());
		GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
		group->appendBooleanArgument(TRUE);
		group->appendObjectIDArgument(general->getID());
		logLine("autopilot: selected the General");
	}
	const struct { UnsignedInt at; Int slot; const char *what; } picks[] = { { 240, TERTIARY_WEAPON, "lethal" }, { 420, SECONDARY_WEAPON, "pistol" } };
	for (Int i = 0; i < 2; ++i)
	{
		if (t >= picks[i].at && s_done < (UnsignedInt)(2 + i))
		{
			s_done = 2 + i;

			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_SWITCH_WEAPONS);
			msg->appendIntegerArgument(picks[i].slot);
			logf("autopilot: picked the %s from the command bar", picks[i].what);
		}
	}
	if (t >= 600 && s_done < 4)
	{
		s_done = 4;
		logLine("autopilot: taking control");
		Zhbo_RequestTakeControl(general);
	}
}

void autopilotFrame()
{
	if (!s_autopilot.enabled)
		return;
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (!s_autopilot.spawned)
		return; // the spawn itself happens in Zhbo_LogicStep
	if (s_autopilot.showcase)
	{
		showcaseFrame(frame);
		return;
	}
	if (s_autopilot.rts)
	{
		rtsAutopilotFrame(frame);
		return;
	}
	if (s_autopilot.build)
	{
		buildAutopilotFrame(frame);
		repairTestFrame(frame);
		return;
	}
	if (s_config.autopilot_manual)
		return;
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

// BO2's look on screen while first person owns the camera (the surfaces themselves are lit as
// BO2 lights them by host patch 0009): bloom on bright highlights, BO2 multiplayer's slightly cool
// grade and a soft vignette. The player's own settings come back in the overhead view. The scene is
// rendered in HDR throughout (Zhbo_Init) so the BO2 highlights have headroom to roll off and bloom.
struct ScreenLook
{
	Bool bloom, grade, vignette;
	Real bloomThreshold, bloomIntensity, gradeStrength, gradeTemperature, gradeTint, vignetteStrength;
};
Bool s_bo2Look = FALSE;
ScreenLook s_lookWas;

void setBo2Look(Bool on)
{
	if (on == s_bo2Look || !TheWritableGlobalData)
		return;
	s_bo2Look = on;
	GlobalData *gd = TheWritableGlobalData;
	if (on)
	{
		s_lookWas = { gd->m_bgfxBloom, gd->m_bgfxColorGrade, gd->m_bgfxVignette, gd->m_bgfxBloomThreshold, gd->m_bgfxBloomIntensity,
			gd->m_bgfxColorGradeStrength, gd->m_bgfxColorGradeTemperature, gd->m_bgfxColorGradeTint, gd->m_bgfxVignetteStrength };
		gd->m_bgfxBloom = TRUE;
		gd->m_bgfxBloomThreshold = 0.8f;
		gd->m_bgfxBloomIntensity = 0.35f;
		gd->m_bgfxColorGrade = TRUE;
		gd->m_bgfxColorGradeStrength = 0.45f;
		gd->m_bgfxColorGradeTemperature = -0.15f;
		gd->m_bgfxColorGradeTint = 0.0f;
		gd->m_bgfxVignette = TRUE;
		gd->m_bgfxVignetteStrength = 0.22f;
	}
	else
	{
		gd->m_bgfxBloom = s_lookWas.bloom;
		gd->m_bgfxColorGrade = s_lookWas.grade;
		gd->m_bgfxVignette = s_lookWas.vignette;
		gd->m_bgfxBloomThreshold = s_lookWas.bloomThreshold;
		gd->m_bgfxBloomIntensity = s_lookWas.bloomIntensity;
		gd->m_bgfxColorGradeStrength = s_lookWas.gradeStrength;
		gd->m_bgfxColorGradeTemperature = s_lookWas.gradeTemperature;
		gd->m_bgfxColorGradeTint = s_lookWas.gradeTint;
		gd->m_bgfxVignetteStrength = s_lookWas.vignetteStrength;
	}
}

void releaseClientState()
{
	setSkyForced(FALSE);
	setBo2Look(FALSE);
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
}

// ------------------------------------------------------------------------------------------------
// Building in first person (zhbo_core::building): the hammer places from the General's build menu,
// and holding the use key at a site builds it.
// ------------------------------------------------------------------------------------------------
namespace
{
	// What `general` can build: the construct buttons of his build menu.
	std::vector<const ThingTemplate *> buildables(const Object *general)
	{
		std::vector<const ThingTemplate *> out;
		const CommandSet *set = general ? TheControlBar->findCommandSet(ZhboControlBarAccess::buildSetFor(general)) : nullptr;
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
			const AsciiString set = ZhboControlBarAccess::buildSetFor(obj);
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
	Drawable *s_ghost = nullptr;
	uint32_t s_ghostTmpl = 0;

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

	// The last placement check (cbClientCanPlace): keyed on what it checked, so a changed input is
	// not answered with a stale result. Cleared in Zhbo_Reset (it reads engine state per game).
	struct CanPlaceCache
	{
		uint32_t general, tmpl;
		float x, y, z, angle;
		int32_t result;
	};
	CanPlaceCache s_canPlace = {};

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

	// A reset destroys every drawable, the ghost too.
	void forgetGhost()
	{
		s_ghost = nullptr;
		s_ghostTmpl = 0;
	}
}

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
	// A same-size but shifted API table would pass the version check: the counts catch it.
	if (const uint64_t fields = zhbo_abi_fields();
		(fields & 0xFFFFFFFFu) != ZHBO_LOGIC_API_FIELDS || (fields >> 32) != ZHBO_CLIENT_API_FIELDS)
	{
		logf("zhbo-core API tables have %u logic / %u client fields, glue expects %u / %u; mod disabled",
			(unsigned)(fields & 0xFFFFFFFFu), (unsigned)(fields >> 32), ZHBO_LOGIC_API_FIELDS, ZHBO_CLIENT_API_FIELDS);
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
		zhbo_platform::errorBox("Conquer Ops", text);
		zhbo_platform::exitProcess(1);
	}

	// Designated initializers: a field inserted mid-table cannot silently shift the rest, and the
	// compiler names any field that is missing or renamed.
	static const ZhboLogicApi logicApi = {
		.log = cbLog,
		.ground_height = cbGroundHeight,
		.is_walkable = cbIsWalkable,
		.extent = cbExtent,
		.object = cbObject,
		.query = cbQuery,
		.set_pose = cbSetPose,
		.set_controlled = cbSetControlled,
		.fire_fx = cbFireFx,
		.damage = cbDamage,
		.heal = cbHeal,
		.spawn = cbSpawn,
		.destroy = cbDestroy,
		.disable = cbDisable,
		.fx = cbFx,
		.capture = cbCapture,
		.smoke = cbSmoke,
		.build_options = cbBuildOptions,
		.can_place = cbCanPlace,
		.place = cbPlace,
		.work_site = cbWorkSite,
		.work = cbWork,
	};
	static const ZhboClientApi clientApi = {
		.screen_size = cbScreenSize,
		.fill_rect = cbFillRect,
		.line = cbLine,
		.text = cbText,
		.image = cbImage,
		.query = cbClientQuery,
		.cash = cbCash,
		.sound = cbSound,
		.clip = cbClip,
		.radar = cbRadar,
		.build_options = cbBuildOptions,
		.can_place = cbClientCanPlace,
		.ghost = cbGhost,
	};
	zhbo_init(&logicApi, &clientApi);

	// The BO2 weapon table, converted from the player's install into ConquerOps_BO2.big; without it
	// the core keeps its built-in stand-ins.
	if (File *f = TheFileSystem ? TheFileSystem->openFile(zhbo_names::WEAPON_TABLE_PATH, File::READ | File::BINARY) : nullptr)
	{
		const Int size = f->size();
		if (char *buf = f->readEntireAndClose())
		{
			std::string text(buf, size);
			delete[] buf;
			logf("weapons: %d BO2 weapons installed", zhbo_install_weapons(text.c_str()));
		}
	}
	else
	{
		logLine("weapons: no converted BO2 weapon table; using the built-in one");
	}

	if (TheControlBar)
		ZhboControlBarAccess::injectConstructButtons();

	readConfig();
	s_autopilot.enabled = s_config.autopilot_enabled;
	s_autopilot.showcase = s_config.autopilot_showcase;
	s_autopilot.rts = s_config.autopilot_rts;
	s_autopilot.build = s_config.autopilot_build;

	// Render the scene in HDR (see setBo2Look); the format change needs a framebuffer rebuild.
	if (TheWritableGlobalData && !TheWritableGlobalData->m_bgfxHdr)
	{
		TheWritableGlobalData->m_bgfxHdr = TRUE;
		GGC_RequestBgfxFramebufferRebuild();
	}

	s_lastTick = zhbo_platform::now();
	s_ready = TRUE;
}

void Zhbo_Reset()
{
	if (!s_ready)
		return;
	releaseClientState();
	const Bool autopilot = s_autopilot.enabled;
	const Bool showcase = s_autopilot.showcase;
	const Bool rts = s_autopilot.rts;
	const Bool build = s_autopilot.build;
	memset(&s_autopilot, 0, sizeof(s_autopilot));
	s_autopilot.build = build;
	s_autopilot.enabled = autopilot;
	s_autopilot.showcase = showcase;
	s_autopilot.rts = rts;
	for (std::map<std::string, Weapon *>::iterator it = s_fxWeapons.begin(); it != s_fxWeapons.end(); ++it)
		deleteInstance(it->second);
	s_fxWeapons.clear();
	s_worldLooks.clear();
	s_rtsShots.clear();
	s_generalBuilds.clear();
	s_idleGenerals.clear();
	forgetGhost();
	// Every per-game static: engine state that a new game rebuilds (zhbo-core keeps its own
	// surviving state behind zhbo_reset, called below).
	memset(s_rtsHitFx, 0, sizeof(s_rtsHitFx));
	s_rtsHitFxLoaded = FALSE;
	s_images.clear();
	s_soundReported.clear();
	s_smokeLastLog = 0;
	memset(&s_canPlace, 0, sizeof(s_canPlace));
	s_wheelAccum = 0.0f;
	s_lastTick = zhbo_platform::now(); // else the first frame after the reset logs a bogus hitch
	ZhboViewmodel_Reset();
	zhbo_reset();
	logLine("reset");
}

void Zhbo_Preload()
{
	if (!s_ready)
		return;
	const double started = zhbo_platform::now();
	// The mod's objects (the Generals, their grenades, C4 and other equipment): what the engine's
	// own preload does for KINDOF_PRELOAD things, which retail only runs with -preload.
	int objects = 0;
	for (const ThingTemplate *t = TheThingFactory->firstTemplate(); t; t = t->friend_getNextTemplate())
	{
		if (strncmp(t->getName().str(), "ZHBO_", 5) != 0)
			continue;
		if (Drawable *draw = TheThingFactory->newDrawable(t))
		{
			draw->preloadAssets(TheGlobalData->m_timeOfDay);
			TheGameClient->destroyDrawable(draw);
			++objects;
		}
	}
	logf("preloaded %d objects in %.0f ms", objects, (zhbo_platform::now() - started) * 1000.0);

	// The local General's first-person models: its side's tablet and the kit's weapons.
	const Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
	const PlayerTemplate *pt = local ? local->getPlayerTemplate() : nullptr;
	if (!pt)
		return;
	const AsciiString side = pt->getBaseSide();
	const char faction = side == "China" ? 'C' : side == "GLA" ? 'G' : side == "America" ? 'U' : 0;
	if (!faction)
		return;
	int weapons[32];
	const int count = std::min<int>(zhbo_loadout_viewmodels(weapons, 32), 32);
	ZhboViewmodel_Preload(faction, weapons, count);
}

void Zhbo_OnTakeControlMessage(Int playerIndex, const GameMessage *msg)
{
	if (!s_ready || msg->getArgumentCount() < 1)
		return;
	const ObjectID id = msg->getArgument(0)->objectID;
	Object *obj = TheGameLogic->findObjectByID(id);
	// The same prefix test as everywhere else (isGeneral): "ZHBO_General_", not a shorter one.
	if (!obj || !isGeneral(obj))
		return;
	// The same message, with an action, chooses the grenade the General's third slot throws.
	const Int action = msg->getArgumentCount() > 1 ? msg->getArgument(1)->integer : ZHBO_ACTION_TAKE_CONTROL;
	if (action == ZHBO_ACTION_LETHAL || action == ZHBO_ACTION_TACTICAL)
	{
		zhbo_logic_rts_offhand(playerIndex, (uint32_t)id, action == ZHBO_ACTION_TACTICAL ? 1 : 0);
		return;
	}
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
	if (s_autopilot.rts && s_autopilot.spawned)
		rtsAutopilotWave(frame);
	rtsWeapons(frame);
	trackGeneralBuilds(frame);
	trackIdleGenerals();
	zhbo_logic_step(frame);
}

Bool Zhbo_IsControlled(ObjectID id)
{
	return s_ready && zhbo_logic_is_controlled((uint32_t)id) != 0;
}

// Third person: show the weapon each General has in hand (its W<vm><k> sub-objects on the soldier
// model) or the scorestreak tablet (TB<k>), play the tablet's raise or put-away (USER_1, USER_2),
// and hold the sidearm in BO2's pistol stance (JAMMED). Only changes are applied.

void applyWorldPoses()
{
	ZhboWorldPose poses[16];
	const int32_t n = zhbo_client_world_poses(poses, 16);
	enum { MAX_VIEWMODELS = 160 };
	static int32_t s_meshes[MAX_VIEWMODELS];
	static int32_t s_tabletMeshes = -1;
	if (s_tabletMeshes < 0)
	{
		s_tabletMeshes = zhbo_world_mesh_count(-2);
		for (int32_t vm = 0; vm < MAX_VIEWMODELS; ++vm)
			s_meshes[vm] = zhbo_world_mesh_count(vm);
	}
	for (int32_t i = 0; i < n; ++i)
	{
		const ZhboWorldPose &p = poses[i];
		Object *obj = TheGameLogic->findObjectByID((ObjectID)p.general);
		Drawable *draw = obj ? obj->getDrawable() : nullptr;
		if (!draw)
			continue;
		std::map<uint32_t, WorldLook>::iterator it = s_worldLooks.find(p.general);
		if (it != s_worldLooks.end() && it->second.weapon == p.weapon && it->second.tablet == p.tablet && it->second.sidearm == p.sidearm)
			continue;
		char name[16];
		for (int32_t vm = 0; vm < MAX_VIEWMODELS; ++vm)
		{
			for (int32_t k = 0; k < s_meshes[vm]; ++k)
			{
				zhbo_names::worldMesh(name, sizeof(name), vm, k);
				draw->showSubObject(AsciiString(name), vm == p.weapon && p.tablet == 0);
			}
		}
		for (int32_t k = 0; k < s_tabletMeshes; ++k)
		{
			zhbo_names::tabletMesh(name, sizeof(name), k);
			draw->showSubObject(AsciiString(name), p.tablet != 0);
		}
		draw->updateSubObjects();
		if (p.tablet == 1)
			draw->clearAndSetModelConditionState(MODELCONDITION_USER_2, MODELCONDITION_USER_1);
		else if (p.tablet == 2)
			draw->clearAndSetModelConditionState(MODELCONDITION_USER_1, MODELCONDITION_USER_2);
		else
		{
			draw->clearModelConditionState(MODELCONDITION_USER_1);
			draw->clearModelConditionState(MODELCONDITION_USER_2);
		}
		// BO2's pistol stance while the sidearm is in hand (JAMMED: only missiles use it otherwise).
		if (p.sidearm && p.tablet == 0 && !obj->isEffectivelyDead())
			draw->setModelConditionState(MODELCONDITION_JAMMED);
		else
			draw->clearModelConditionState(MODELCONDITION_JAMMED);
		WorldLook look = { p.weapon, p.tablet, p.sidearm };
		s_worldLooks[p.general] = look;
	}
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

	const double now = zhbo_platform::now();
	const float dt = (float)(now - s_lastTick);
	s_lastTick = now;
	if (dt > 0.06f && dt < 5.0f)
		logf("hitch: %.0f ms frame", dt * 1000.0f);

	const Bool focused = zhbo_platform::windowFocused();
	const Bool menuUp = TheInGameUI->isQuitMenuVisible() || TheGameLogic->isGamePaused();

	ZhboClientIn in;
	memset(&in, 0, sizeof(in));
	in.dt = dt;
	in.local_player = rts::getObservedOrLocalPlayer()->getPlayerIndex();
	in.logic_frame = TheGameLogic->getFrame();
	if (focused && !menuUp)
	{
		zhbo_platform::readKeys(in.keys);
	}

	// Test runs: log focus changes and the held keys, so an input test can tell lost focus from
	// lost keys.
	if (s_autopilot.enabled)
	{
		static Bool s_wasFocused = FALSE;
		static uint32_t s_lastKeyHash = 0;
		if (focused != s_wasFocused)
		{
			logf("window focus %s", focused ? "gained" : "lost");
			s_wasFocused = focused;
		}
		char held[64];
		Int n = 0;
		uint32_t hash = 0;
		for (Int k = 0; k < 256 && n < (Int)sizeof(held) - 1; ++k)
		{
			if (in.keys[k])
			{
				held[n++] = (k >= '0' && k <= 'Z') ? (char)k : '?';
				hash = hash * 31 + (uint32_t)k + 1;
			}
		}
		held[n] = 0;
		if (hash != s_lastKeyHash)
		{
			logf("keys held: [%s]", held);
			s_lastKeyHash = hash;
		}
	}
	if (s_mouseCaptured && focused)
	{
		zhbo_platform::mouseDelta(in.mouse_dx, in.mouse_dy);
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
	in.rts_hfov = TheTacticalView->getFieldOfView();
	const Coord3D &pivot = TheTacticalView->getPosition();
	in.rts_pivot.x = pivot.x; in.rts_pivot.y = pivot.y; in.rts_pivot.z = pivot.z;

	zhbo_client_frame(&in, &s_out);
	applyWorldPoses();

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
	reclaimMouse();
	setRtsUiHidden(s_out.hide_rts_ui != 0);
	setSkyForced(s_out.camera_override != 0);
	setBo2Look(s_out.camera_override != 0);
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

Bool Zhbo_GetViewmodel(Int *clip, Int *weapon, Bool *fired, Real *time, Real *ads, AsciiString *model, Coord3D *offset, Coord3D *angles)
{
	if (!s_ready || !s_out.camera_override || s_out.viewmodel < 0 || !s_out.hide_object)
		return FALSE;
	const Object *general = TheGameLogic->findObjectByID((ObjectID)s_out.hide_object);
	if (!general)
		return FALSE;
	// One set of viewmodels per faction (zhbo_assets::generals::FACTIONS).
	const AsciiString &name = general->getTemplate()->getName();
	const Bool china = name == "ZHBO_General_China", gla = name == "ZHBO_General_GLA";
	// The tablet clips play on the arms-and-tablet model (generals::tablet_model); everything else on
	// the converted weapon's viewmodel (generals::vm_model).
	// (An alt mode's clips come after the tablet's numbers, on the weapon's own viewmodel.)
	// The hammer's likewise on the arms-and-hammer model (generals::hammer_model).
	const Bool tablet = s_out.viewmodel >= ZHBO_VM_TABLET_PULLOUT && s_out.viewmodel <= ZHBO_VM_TABLET_PUTAWAY;
	const Bool hammer = s_out.viewmodel >= ZHBO_VM_HAMMER_PULLOUT && s_out.viewmodel <= ZHBO_VM_HAMMER_PUTAWAY;
	AsciiString vm;
	if (tablet || hammer)
		vm.format("ZHBO_VM%s_%s", china ? "CHN" : gla ? "GLA" : "USA", tablet ? "TAB" : "HAM");
	else if (s_out.viewmodel_weapon >= 0)
		vm.format("ZV%02d%c_SKN", s_out.viewmodel_weapon, china ? 'C' : gla ? 'G' : 'U');
	else
		return FALSE;
	*clip = s_out.viewmodel;
	*weapon = tablet || hammer ? ZHBO_VM_NONE : s_out.viewmodel_weapon;
	*fired = s_out.viewmodel_fired != 0;
	*time = s_out.viewmodel_time;
	*ads = s_out.viewmodel_ads;
	offset->set(s_out.viewmodel_offset.x, s_out.viewmodel_offset.y, s_out.viewmodel_offset.z);
	angles->set(s_out.viewmodel_angles.x, s_out.viewmodel_angles.y, s_out.viewmodel_angles.z);
	model->set(vm);
	return TRUE;
}

void Zhbo_Log(const char *fmt, ...)
{
	char buf[512];
	va_list args;
	va_start(args, fmt);
	vsnprintf(buf, sizeof(buf), fmt, args);
	va_end(args);
	logLine(buf);
}

void Zhbo_DrawHud()
{
	if (!s_ready || !TheGameLogic->isInGame() || TheGameLogic->isInShellGame())
		return;
	++s_drawFrame;
	zhbo_client_draw_hud(rts::getObservedOrLocalPlayer()->getPlayerIndex());
	if (!s_out.camera_override)
		drawRtsAmmo();
	evictTexts();
}

void Zhbo_DrawSky()
{
#if defined(GGC_BGFX_RENDERER_WGPU)
	// The bgfx backend sends 2D draws to its UI view, which runs after the 3D scene, so a 2D
	// gradient would cover the world. Paint the sky as the scene view's clear colour instead
	// (view 1 is BgfxBackend's kBgfxEngineView; alpha stays 1 for its water blending), and put
	// the backend's black back for the RTS camera.
	static bool s_skyClear = false;
	const bool wantSky = s_ready && s_out.camera_override;
	if (wantSky != s_skyClear)
	{
		zhgfx_set_view_clear(1, 0x0001 | 0x0002 | 0x0004, wantSky ? 0x7896B4FFu : 0x000000FFu, 1.0f, 0);
		s_skyClear = wantSky;
	}
	return;
#endif
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

void Zhbo_OnZhboCommand(Object *obj, const CommandButton *button)
{
	if (!obj || !button)
		return;
	const AsciiString &name = button->getName();
	if (name == "Command_ZHBO_BuildMenu" || name == "Command_ZHBO_BuildBack")
	{
		// The command bar shows the selected object's command set: switch it (UI only).
		const AsciiString set = name == "Command_ZHBO_BuildMenu" ? ZhboControlBarAccess::buildSetFor(obj) : AsciiString::TheEmptyString;
		if (name == "Command_ZHBO_BuildMenu" && set.isEmpty())
		{
			logLine("Build menu: no dozer command set for this General's side");
			return;
		}
		obj->setCommandSetStringOverride(set);
		if (TheControlBar)
			TheControlBar->markUIDirty();
		logf("General %u %s", (unsigned)obj->getID(), set.isEmpty() ? "closed the build menu" : "opened the build menu");
		return;
	}
	Zhbo_RequestTakeControl(obj);
}

Real Zhbo_WorkRate(const Object *builder)
{
	return isGeneral(builder) ? zhbo_general_build_rate() : 1.0f;
}

void Zhbo_RequestTakeControl(Object *obj)
{
	if (!obj)
		return;
	GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_TAKE_CONTROL);
	msg->appendObjectIDArgument(obj->getID());
}

Int Zhbo_WeaponButtonState(const Object *obj, const CommandButton *command)
{
	const Int slot = command ? (Int)command->getWeaponSlot() : -1;
	if (!s_ready || !isGeneral(obj) || slot < 0 || slot > 2)
		return -1;
	ZhboRtsWeapons ammo = {};
	zhbo_client_rts_ammo((uint32_t)obj->getID(), &ammo);
	if (slot < 2)
		return ammo.ammo[slot] + ammo.reserve[slot] <= 0 ? 0 : -1;
	const Bool tactical = isTacticalButton(command);
	if ((tactical ? ammo.tactical : ammo.ammo[2]) <= 0)
		return 0;
	// Both grenade buttons are the third slot: only the chosen one shows as in use.
	return (tactical ? 1 : 0) != ammo.tactical_selected ? 1 : -1;
}

void Zhbo_OnSwitchWeaponButton(const CommandButton *command)
{
	if (!s_ready || !command || command->getWeaponSlot() != TERTIARY_WEAPON || !TheInGameUI)
		return;
	const Int choice = isTacticalButton(command) ? ZHBO_ACTION_TACTICAL : ZHBO_ACTION_LETHAL;
	const DrawableList *selected = TheInGameUI->getAllSelectedDrawables();
	for (DrawableListCIt it = selected->begin(); it != selected->end(); ++it)
	{
		const Object *obj = *it ? (*it)->getObject() : nullptr;
		if (!isGeneral(obj) || !obj->isLocallyControlled())
			continue;
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_TAKE_CONTROL);
		msg->appendObjectIDArgument(obj->getID());
		msg->appendIntegerArgument(choice);
	}
}

const WideChar *Zhbo_FetchString(const char *label)
{
	// Usable before Zhbo_Init: the string table is static data in the Rust core.
	return (const WideChar *)zhbo_string(label);
}
