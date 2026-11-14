/*
** Conquer Ops - engine-side bridge: the house-colour ring under every General.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Deliberate divergence from both games (user's ruling, team colours): a General is hard to pick
// out and nothing tells his side, so every General stands on a ring in his owner's house colour,
// as Zero Hour colours a radius decal whose template leaves the colour to the owner
// (RadiusDecalTemplate::createRadiusDecal). The art is Zero Hour's own plain ring (DM_Ring, the
// demoralize decal's). Presentation only: a projected decal the client moves under him each frame,
// never read by the logic. It hides as he does (shroud and fog for another player's General,
// stealth, a hidden drawable) and under the local General while he is in first person.

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
#include "GameClient/Shadow.h"

#include <map>

namespace
{
// The ring's texture, its diameter as a multiple of the General's footprint (his geometry
// radius), and its opacity (0-255).
constexpr const char *RING_TEXTURE = "DM_Ring";
constexpr Real RING_FOOTPRINTS = 3.0f;
constexpr Int RING_OPACITY = 220;

std::map<ObjectID, Shadow *> s_rings;

Shadow *addRing(const Object *obj)
{
	if (!TheProjectedShadowManager)
		return nullptr;
	Shadow::ShadowTypeInfo info;
	info.allowUpdates = FALSE;
	info.allowWorldAlign = TRUE; // wraps over the ground under him
	info.m_type = SHADOW_ALPHA_DECAL;
	strlcpy(info.m_ShadowName, RING_TEXTURE, ARRAY_SIZE(info.m_ShadowName));
	const Real size = obj->getGeometryInfo().getMajorRadius() * RING_FOOTPRINTS;
	info.m_sizeX = size;
	info.m_sizeY = size;
	Shadow *ring = TheProjectedShadowManager->addDecal(&info);
	if (!ring)
	{
		logf("team ring: Zero Hour refused the %s decal", RING_TEXTURE);
		return nullptr;
	}
	ring->setAngle(0.0f);
	ring->setOpacity(RING_OPACITY);
	return ring;
}
} // namespace

void zhboTeamRingsFrame()
{
	const Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
	if (!local)
		return;
	std::map<ObjectID, Shadow *> kept;
	for (Object *obj = TheGameLogic->getFirstObject(); obj; obj = obj->getNextObject())
	{
		if (!isGeneral(obj) || obj->isEffectivelyDead())
			continue;
		const Player *owner = obj->getControllingPlayer();
		if (!owner)
			continue;
		Shadow *ring = nullptr;
		std::map<ObjectID, Shadow *>::iterator it = s_rings.find(obj->getID());
		if (it != s_rings.end())
		{
			ring = it->second;
			s_rings.erase(it);
		}
		else if (!(ring = addRing(obj)))
			continue;
		kept[obj->getID()] = ring;
		const Drawable *draw = obj->getDrawable();
		const Bool fogged = owner != local && obj->getShroudedStatus(local->getPlayerIndex()) >= OBJECTSHROUD_FOGGED;
		const Bool hidden = !draw || draw->isDrawableEffectivelyHidden();
		const Bool ownFirstPerson = owner == local && Zhbo_IsControlled(obj->getID());
		ring->enableShadowInvisible(fogged || hidden || ownFirstPerson);
		ring->enableShadowRender(TRUE);
		ring->setColor(owner->getPlayerColor());
		const Coord3D *pos = obj->getPosition();
		ring->setPosition(pos->x, pos->y, pos->z);
	}
	// The dead and the gone lose theirs.
	for (std::map<ObjectID, Shadow *>::iterator it = s_rings.begin(); it != s_rings.end(); ++it)
		it->second->release();
	s_rings.swap(kept);
}

void zhboTeamRingsReset()
{
	// Before Zero Hour's per-map shadow reset, which expects every decal returned by its owner.
	for (std::map<ObjectID, Shadow *>::iterator it = s_rings.begin(); it != s_rings.end(); ++it)
		it->second->release();
	s_rings.clear();
}
