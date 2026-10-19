/*
** Conquer Ops - engine-side bridge: the Rust core's logic callbacks, filled with Zero Hour services.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
// One engine Weapon per FX template: fireWeaponTemplate needs a firing Weapon instance.
std::map<std::string, Weapon *> s_fxWeapons;

// Smoke log throttle (cbSmoke): at most one line a second.
UnsignedInt s_smokeLastLog = 0;


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
} // namespace

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
		// Taking control is the player's order: as for any other, a dozer drops the task in hand
		// (DozerAIUpdate::aiDoCommand). On release, one still pending (a site laid out in first
		// person and left unfinished) is resumed.
		ai->aiIdle(controlled ? CMD_FROM_PLAYER : CMD_FROM_AI);
	}
	obj->clearModelConditionState(MODELCONDITION_MOVING);
	logf("General %u %s first-person control", id, controlled ? "entered" : "left");
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
// The engine's logic callback for the pick menu's choice; forwards to the Rust handler in ffi.rs.
void cbChooseClass(int32_t player, uint32_t generalId, int32_t index)
{
	zhbo_logic_choose_class(player, generalId, index, nullptr, 0);
}
void ResetFxWeapons()
{
	for (std::map<std::string, Weapon *>::iterator it = s_fxWeapons.begin(); it != s_fxWeapons.end(); ++it)
		deleteInstance(it->second);
	s_fxWeapons.clear();
	s_smokeLastLog = 0;
}
