/*
** Conquer Ops - engine-side bridge: the Rust core's logic callbacks, filled with Zero Hour services.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
#include "Common/MapReaderWriterInfo.h"
#include "GameLogic/AIPathfind.h"
namespace
{
// One engine Weapon per FX template: fireWeaponTemplate needs a firing Weapon instance.
std::map<std::string, Weapon *> s_fxWeapons;

// Smoke log throttles (cbSmoke, Zhbo_SmokeHides, Zhbo_SmokeCrossesPath): at most one line a second each.
UnsignedInt s_smokeLastLog = 0;
UnsignedInt s_smokeHideLastLog = 0;
UnsignedInt s_smokeRepathLastLog = 0;

// The map whose ground surfaces the core holds (EnsureSurfaceGrid); empty after a reset.
AsciiString s_surfaceMap;

// Zero Hour's terrain class (Common/TerrainTypes.h, a Terrain.ini entry's Class) as the BO2 surface
// whose impact cells and bounce scales it takes.
int32_t surfaceOfClass(TerrainClass c)
{
	switch (c)
	{
		case TERRAIN_ASPHALT: return ZHBO_SURF_ASPHALT;
		case TERRAIN_CONCRETE:
		case TERRAIN_RESIDENTIAL:
		case TERRAIN_URBAN:
		case TERRAIN_CHINA:
		case TERRAIN_COBBLESTONE_GRASS: return ZHBO_SURF_CONCRETE;
		case TERRAIN_GRASS:
		case TERRAIN_ACCENT_GRASS:
		case TERRAIN_EASTERN_EUROPE_1:
		case TERRAIN_EASTERN_EUROPE_2:
		case TERRAIN_EASTERN_EUROPE_3:
		case TERRAIN_SWISS_1:
		case TERRAIN_SWISS_2:
		case TERRAIN_SWISS_3:
		case TERRAIN_FIELD:
		case TERRAIN_BEACH_PARK: return ZHBO_SURF_GRASS;
		case TERRAIN_SNOW_1:
		case TERRAIN_SNOW_2:
		case TERRAIN_SNOW_3:
		case TERRAIN_RUGGED_SNOW:
		case TERRAIN_FLAT_SNOW: return ZHBO_SURF_SNOW;
		case TERRAIN_ROCK:
		case TERRAIN_ACCENT_ROCK:
		case TERRAIN_CLIFF:
		case TERRAIN_RUGGED_MOUNTAIN:
		case TERRAIN_TRANSITION: return ZHBO_SURF_ROCK;
		case TERRAIN_WOOD: return ZHBO_SURF_WOOD;
		case TERRAIN_SAND:
		case TERRAIN_ACCENT_SAND:
		case TERRAIN_DESERT_1:
		case TERRAIN_DESERT_2:
		case TERRAIN_DESERT_3:
		case TERRAIN_LIVE_DESERT:
		case TERRAIN_DRY_DESERT:
		case TERRAIN_TROPICAL_BEACH: return ZHBO_SURF_SAND;
		case TERRAIN_DIRT:
		case TERRAIN_BLEND_EDGES:
		case TERRAIN_NONE:
		default: return ZHBO_SURF_DIRT;
	}
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

int32_t cbSurfaceAt(float x, float y)
{
	// The water table over the ground: BO2's water cell. TerrainLogic's water is logic state.
	// The ground under it is the core's map grid (zhbo_terrain_load, EnsureSurfaceGrid), read from
	// the map file itself, never from the client's TheTerrainVisual: the surface feeds the sim (a
	// grenade's bounce), so a peer with other graphics settings or no terrain visual at all must
	// get the same answer (docs/CONTRACT.md "Multiplayer").
	EnsureSurfaceGrid();
	if (TheTerrainLogic && TheTerrainLogic->isUnderwater(x, y))
		return ZHBO_SURF_WATER;
	return ZHBO_SURF_DEFAULT;
}

void EnsureSurfaceGrid()
{
	if (!TheTerrainLogic || !TheTerrainTypes)
		return;
	const AsciiString map = TheTerrainLogic->getSourceFilename();
	if (map.isEmpty() || map == s_surfaceMap)
		return;
	s_surfaceMap = map;
	// The map file as the logic reads it (W3DTerrainLogic::loadMap: CachedFileInputStream
	// decompresses a RefPack'd map).
	std::vector<char> bytes;
	CachedFileInputStream in;
	if (in.open(map))
	{
		char buf[65536];
		Int n;
		while ((n = in.read(buf, sizeof(buf))) > 0)
			bytes.insert(bytes.end(), buf, buf + n);
		in.close();
	}
	// Zero Hour's terrain types in collection order (findTerrain's search order), each classified.
	std::vector<AsciiString> names;
	std::vector<int32_t> surfaces;
	for (TerrainType *t = TheTerrainTypes->firstTerrain(); t; t = TheTerrainTypes->nextTerrain(t))
	{
		names.push_back(t->getName());
		surfaces.push_back(surfaceOfClass(t->getClass()));
	}
	std::vector<const char *> namePtrs;
	for (const AsciiString &n : names)
		namePtrs.push_back(n.str());
	const int32_t cells = zhbo_terrain_load(bytes.empty() ? nullptr : (const uint8_t *)bytes.data(), (int32_t)bytes.size(),
		namePtrs.data(), surfaces.data(), (int32_t)namePtrs.size());
	if (cells < 0)
		logf("terrain: %s (%d bytes) did not parse; every ground reads the default surface", map.str(), (int)bytes.size());
	else
		logf("terrain: %s: %d cells of ground surface, %d terrain types", map.str(), (int)cells, (int)names.size());

	// A cross-check where a terrain visual exists (it never decides anything): its own tile lookup
	// (TerrainVisual::getTerrainTile, what this query used to read) against the grid, on every
	// fourth cell of the map. A headless machine has nothing to compare and says so.
	if (!TheTerrainVisual)
	{
		logf("terrain: no terrain visual on this machine; the grid alone answers");
		return;
	}
	Region3D r;
	TheTerrainLogic->getExtent(&r);
	Int sampled = 0, agree = 0;
	for (Real y = r.lo.y + 5.0f; y < r.hi.y; y += 40.0f)
	{
		for (Real x = r.lo.x + 5.0f; x < r.hi.x; x += 40.0f)
		{
			TerrainType *tile = TheTerrainVisual->getTerrainTile(x, y);
			const int32_t visual = tile ? surfaceOfClass(tile->getClass()) : ZHBO_SURF_DEFAULT;
			const int32_t grid = zhbo_terrain_surface_at(x, y);
			++sampled;
			if (visual == grid)
				++agree;
			else if (sampled - agree == 1)
				logf("terrain: first disagreement at (%.0f, %.0f): visual %d, grid %d", x, y, (int)visual, (int)grid);
		}
	}
	logf("terrain: the grid agrees with the terrain visual on %d of %d sampled cells", (int)agree, (int)sampled);
}

void ResetSurfaceGrid()
{
	s_surfaceMap.clear();
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
	// Every object whose bounding sphere comes within `radius`: Zero Hour's own measure for radius
	// damage (Weapon.cpp DAMAGE_RANGE_CALC_TYPE). A centre-to-centre query missed a structure or an
	// Overlord whose hull was in reach but whose centre was not; the core measures each object's
	// solid itself (docs/CONTRACT.md, "The object query").
	SimpleObjectIterator *iter = ThePartitionManager->iterateObjectsInRange(&c, radius, FROM_BOUNDINGSPHERE_3D);
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
// A smoke screen (BO2's smoke grenade) this frame, at its sight blocker's radius now. What it does
// to Zero Hour's units is the engine's own (host patch 0024 asks the core's mirror: no unit
// acquires or keeps a target the screen hides, and paths go around it); this only logs it, the
// test's oracle (at most once a second).
void cbSmoke(const ZhboVec3 *at, float radius)
{
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (frame >= s_smokeLastLog + LOGICFRAMES_PER_SECOND)
	{
		s_smokeLastLog = frame;
		logf("smoke: screen at (%.0f, %.0f) blocks sight %.1f around it", at->x, at->y, radius);
	}
	if (s_config.autopilot_smoke)
	{
		Coord3D c;
		c.set(at->x, at->y, at->z);
		autopilotSmokeSeen(c, radius);
	}
}

// The point a unit sees from and is seen at: its centre (feet plus half its height).
static Coord3D sightPoint(const Object *obj)
{
	Coord3D p = *obj->getPosition();
	p.z += obj->getGeometryInfo().getMaxHeightAbovePosition() * 0.5f;
	return p;
}

Bool Zhbo_SmokeHides(const Object *source, const Object *victim)
{
	if (!source || !victim)
		return FALSE;
	const Coord3D a = sightPoint(source), b = sightPoint(victim);
	if (!zhbo_logic_smoke_blocks(a.x, a.y, a.z, b.x, b.y, b.z))
		return FALSE;
	const UnsignedInt frame = TheGameLogic ? TheGameLogic->getFrame() : 0;
	if (frame >= s_smokeHideLastLog + LOGICFRAMES_PER_SECOND)
	{
		s_smokeHideLastLog = frame;
		logf("smoke: %s %u can't see %s %u through it", source->getTemplate()->getName().str(), (unsigned)source->getID(),
			victim->getTemplate()->getName().str(), (unsigned)victim->getID());
	}
	return TRUE;
}

// Pathfinder::examineNeighboringCells' own penalty for a cell a path may still cross (tunneling
// out through an invalid cell is 10 x COST_ORTHOGONAL; COST_ORTHOGONAL is 10): enough that a path
// goes round a screen (a few cells further) rather than through it, never a wall.
static const Int SMOKE_CELL_COST = 10 * 10;

Int Zhbo_SmokePathCost(Int cellX, Int cellY)
{
	const Real x = (cellX + 0.5f) * PATHFIND_CELL_SIZE_F, y = (cellY + 0.5f) * PATHFIND_CELL_SIZE_F;
	return zhbo_logic_smoke_covers(x, y) ? SMOKE_CELL_COST : 0;
}

Bool Zhbo_SmokeCrossesPath(Path *path, UnsignedInt pathFrame)
{
	if (!path)
		return FALSE;
	for (const PathNode *n = path->getFirstNode(); n && n->getNext(); n = n->getNext())
	{
		const Coord3D *a = n->getPosition(), *b = n->getNext()->getPosition();
		if (zhbo_logic_smoke_crosses(a->x, a->y, b->x, b->y, pathFrame))
		{
			const UnsignedInt frame = TheGameLogic->getFrame();
			if (frame >= s_smokeRepathLastLog + LOGICFRAMES_PER_SECOND)
			{
				s_smokeRepathLastLog = frame;
				logf("smoke: a move's path crosses a new screen; repathing around it");
			}
			return TRUE;
		}
	}
	return FALSE;
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
	s_smokeHideLastLog = 0;
	s_smokeRepathLastLog = 0;
}
