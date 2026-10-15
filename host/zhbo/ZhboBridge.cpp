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
#include "ZhboPlatform.h"
#if defined(GGC_BGFX_RENDERER_WGPU)
#include "zhbo_gfx.h"
#endif

#include "Common/AudioEventRTS.h"
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
#include "GameClient/FXList.h"
#include "GameClient/GadgetPushButton.h"
#include "GameClient/GameWindow.h"
#include "GameClient/GameWindowManager.h"
#include "Common/NameKeyGenerator.h"
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
#include "GameLogic/Object.h"
#include "GameLogic/PartitionManager.h"
#include "GameLogic/TerrainLogic.h"
#include "GameLogic/Weapon.h"

#include <map>
#include <set>
#include <string>
#include <vector>

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
	return obj && strncmp(obj->getTemplate()->getName().str(), "ZHBO_General_", 13) == 0;
}

struct RtsShots
{
	Bool seen;
	UnsignedInt last[3];
	Int slot;
};
std::map<ObjectID, RtsShots> s_rtsShots;

void rtsWeapons(UnsignedInt frame)
{
	for (Object *obj = TheGameLogic->getFirstObject(); obj; obj = obj->getNextObject())
	{
		if (!isGeneral(obj) || obj->isEffectivelyDead() || !obj->getControllingPlayer())
			continue;
		if (s_rtsShots.find(obj->getID()) == s_rtsShots.end())
			s_rtsShots[obj->getID()].slot = -1;
		RtsShots &seen = s_rtsShots[obj->getID()];
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
		ZhboRtsWeapons ammo;
		zhbo_logic_rts_update(obj->getControllingPlayer()->getPlayerIndex(), (uint32_t)obj->getID(), (int32_t)current, shots, &ammo);
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
void drawRtsAmmo()
{
	if (!TheInGameUI || TheInGameUI->getSelectCount() != 1 || !TheWindowManager)
		return;
	const Drawable *draw = TheInGameUI->getFirstSelectedDrawable();
	const Object *obj = draw ? draw->getObject() : nullptr;
	if (!isGeneral(obj))
		return;
	ZhboRtsWeapons ammo;
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
		if (slot == 2)
			snprintf(text, sizeof(text), "%d", ammo.ammo[slot]);
		else
			snprintf(text, sizeof(text), "%d/%d", ammo.ammo[slot], ammo.reserve[slot]);
		const Bool empty = ammo.ammo[slot] + ammo.reserve[slot] <= 0;
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
	static UnsignedInt s_lastLog = 0;
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (blinded > 0 && frame >= s_lastLog + 30)
	{
		s_lastLog = frame;
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
	static std::set<std::string> s_reported;
	if (s_reported.insert(event).second)
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
	const UnsignedInt shadow = (argb & 0xFF000000u) >> 1 & 0xFF000000u;
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
	// Mapped images live for the whole session; cache lookups (including misses).
	static std::map<std::string, const Image *> s_images;
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

void setMouseCapture(Bool capture)
{
	if (capture == s_mouseCaptured)
		return;
	s_mouseCaptured = capture;
	zhbo_platform::setMouseCapture(capture != FALSE);
	if (TheMouse) TheMouse->setVisibility(capture ? FALSE : TRUE);
	// Relative mouse mode reports the cursor as having left the window, and ending it sends no
	// "entered" event until the next click, so selection and edge scrolling would stay dead until
	// then: tell the mouse it is back, where the system cursor really is.
	if (!capture && TheMouse)
	{
		if (!TheMouse->isCursorInside())
			TheMouse->onCursorMovedInside();
		TheMouse->syncPositionToSystemCursor();
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
	UnsignedInt spawnFrame;
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
	// ZHBO_FORCE_RADAR: give the test player radar, to film the Tab map.
	if (getenv("ZHBO_FORCE_RADAR") && TheRadar)
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
	const char *showcaseObject = getenv("ZHBO_SHOWCASE_OBJECT");
	Object *general = spawnAt(s_autopilot.showcase && showcaseObject ? showcaseObject : "ZHBO_General_USA", local->getDefaultTeam(), pos, yaw);
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
	// ZHBO_AUTOPILOT=equipment: an enemy Sentry Drone in front for the Black Hat, and two rocket
	// troopers firing at the General for the Trophy System.
	const char *mode = getenv("ZHBO_AUTOPILOT");
	if (enemy && mode && strcmp(mode, "equipment") == 0)
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
	if (!s_autopilot.rts)
		Zhbo_RequestTakeControl(general);
}

// Showcase: keep the camera close on the General; walk out and back, then die.
void showcaseFrame(UnsignedInt frame)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general)
		return;
	const Coord3D *pos = general->getPosition();
	// ZHBO_SHOWCASE_ZOOM / ZHBO_SHOWCASE_PITCH (radians) / ZHBO_SHOWCASE_ANGLE (radians, around the General).
	const char *zoomEnv = getenv("ZHBO_SHOWCASE_ZOOM");
	const char *pitchEnv = getenv("ZHBO_SHOWCASE_PITCH");
	const char *angleEnv = getenv("ZHBO_SHOWCASE_ANGLE");
	TheTacticalView->setZoomLimited(FALSE);
	TheTacticalView->lookAt(pos);
	TheTacticalView->setZoom(zoomEnv ? (Real)atof(zoomEnv) : 0.12f);
	if (pitchEnv)
		TheTacticalView->setPitch((Real)atof(pitchEnv));
	if (angleEnv)
		TheTacticalView->setAngle(general->getOrientation() + (Real)atof(angleEnv));
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

void rtsAutopilotFrame(UnsignedInt frame)
{
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (!general || !general->getDrawable())
		return;
	const UnsignedInt t = frame - s_autopilot.spawnFrame;
	static UnsignedInt s_done = 0;
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
		zhbo_platform::errorBox("Conquer Ops", text);
		zhbo_platform::exitProcess(1);
	}

	static const ZhboLogicApi logicApi = {
		cbLog, cbGroundHeight, cbIsWalkable, cbExtent, cbObject, cbQuery,
		cbSetPose, cbSetControlled, cbFireFx, cbDamage, cbHeal,
		cbSpawn, cbDestroy, cbDisable, cbFx, cbCapture, cbSmoke,
	};
	static const ZhboClientApi clientApi = { cbScreenSize, cbFillRect, cbLine, cbText, cbImage, cbClientQuery, cbCash, cbSound, cbClip, cbRadar };
	zhbo_init(&logicApi, &clientApi);

	// The BO2 weapon table, converted from the player's install into ConquerOps_BO2.big; without it
	// the core keeps its built-in stand-ins.
	if (File *f = TheFileSystem ? TheFileSystem->openFile("Data\\ConquerOps\\weapons.ini", File::READ | File::BINARY) : nullptr)
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

	s_autopilot.enabled = getenv("ZHBO_AUTOPILOT") != nullptr;
	s_autopilot.showcase = s_autopilot.enabled && strcmp(getenv("ZHBO_AUTOPILOT"), "showcase") == 0;
	s_autopilot.rts = s_autopilot.enabled && strcmp(getenv("ZHBO_AUTOPILOT"), "rts") == 0;

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
	memset(&s_autopilot, 0, sizeof(s_autopilot));
	s_autopilot.enabled = autopilot;
	s_autopilot.showcase = showcase;
	s_autopilot.rts = rts;
	for (std::map<std::string, Weapon *>::iterator it = s_fxWeapons.begin(); it != s_fxWeapons.end(); ++it)
		deleteInstance(it->second);
	s_fxWeapons.clear();
	s_worldLooks.clear();
	s_rtsShots.clear();
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
	if (s_autopilot.rts && s_autopilot.spawned)
		rtsAutopilotWave(frame);
	rtsWeapons(frame);
	zhbo_logic_step(frame);
}

Bool Zhbo_IsControlled(ObjectID id)
{
	return s_ready && zhbo_logic_is_controlled((uint32_t)id) != 0;
}

// Third person: show the weapon each controlled General has in hand (its W<vm><k> sub-objects on
// the soldier model) or the scorestreak tablet (TB<k>), and play the tablet's raise or put-away
// (USER_1, USER_2). Only changes are applied.

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
		if (it != s_worldLooks.end() && it->second.weapon == p.weapon && it->second.tablet == p.tablet)
			continue;
		char name[16];
		for (int32_t vm = 0; vm < MAX_VIEWMODELS; ++vm)
		{
			for (int32_t k = 0; k < s_meshes[vm]; ++k)
			{
				snprintf(name, sizeof(name), "W%02d%02d", vm, k);
				draw->showSubObject(AsciiString(name), vm == p.weapon && p.tablet == 0);
			}
		}
		for (int32_t k = 0; k < s_tabletMeshes; ++k)
		{
			snprintf(name, sizeof(name), "TB%02d", k);
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
		WorldLook look = { p.weapon, p.tablet };
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
	const Bool tablet = s_out.viewmodel >= ZHBO_VM_TABLET_PULLOUT && s_out.viewmodel <= ZHBO_VM_TABLET_PUTAWAY;
	AsciiString vm;
	if (tablet)
		vm.format("ZHBO_VM%s_TAB", china ? "CHN" : gla ? "GLA" : "USA");
	else if (s_out.viewmodel_weapon >= 0)
		vm.format("ZV%02d%c_SKN", s_out.viewmodel_weapon, china ? 'C' : gla ? 'G' : 'U');
	else
		return FALSE;
	*clip = s_out.viewmodel;
	*weapon = tablet ? ZHBO_VM_NONE : s_out.viewmodel_weapon;
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

void Zhbo_RequestTakeControl(Object *obj)
{
	if (!obj)
		return;
	GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_TAKE_CONTROL);
	msg->appendObjectIDArgument(obj->getID());
}

Bool Zhbo_IsWeaponEmpty(const Object *obj, Int slot)
{
	if (!s_ready || !isGeneral(obj) || slot < 0 || slot > 2)
		return FALSE;
	ZhboRtsWeapons ammo;
	zhbo_client_rts_ammo((uint32_t)obj->getID(), &ammo);
	return ammo.ammo[slot] + ammo.reserve[slot] <= 0;
}

const WideChar *Zhbo_FetchString(const char *label)
{
	// Usable before Zhbo_Init: the string table is static data in the Rust core.
	return (const WideChar *)zhbo_string(label);
}
