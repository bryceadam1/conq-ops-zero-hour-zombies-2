/*
** Conquer Ops - engine-side bridge: the first-person command services (docs/COMMANDS.md): what the
** player could click overhead, the templates' display names, and orders sent as the overhead view
** sends them.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
#include "GameLogic/Module/ContainModule.h"
#include "GameLogic/Squad.h"

namespace
{
	// Zero Hour's select-all (CommandXlat's MSG_META_SELECT_ALL through InGameUI's
	// kindOfUnitSelection): own, none of DOZER/HARVESTER/IGNORES_SELECT_ALL, not contained, alive,
	// mass-selectable, on the map.
	Bool selectAllTakes(const Object *obj)
	{
		KindOfMaskType none, disqualifying;
		disqualifying.set(KINDOF_DOZER);
		disqualifying.set(KINDOF_HARVESTER);
		disqualifying.set(KINDOF_IGNORES_SELECT_ALL);
		return obj->isKindOfMulti(none, disqualifying) && !obj->isContained() && !obj->isEffectivelyDead() &&
			obj->isMassSelectable() && !obj->isOffMap();
	}

	int32_t relationOf(const Player *asker, const Object *obj)
	{
		const Player *owner = obj->getControllingPlayer();
		if (owner == asker)
			return ZHBO_REL_OWN;
		if (owner && owner->getSide() == "Civilian")
			return ZHBO_REL_CIVILIAN;
		switch (asker->getRelationship(obj->getTeam()))
		{
		case ALLIES: return ZHBO_REL_ALLY;
		case ENEMIES: return ZHBO_REL_ENEMY;
		default: return ZHBO_REL_NEUTRAL;
		}
	}

	const char *orderName(int32_t kind)
	{
		switch (kind)
		{
		case ZHBO_ORDER_MOVE: return "move";
		case ZHBO_ORDER_FORCE_MOVE: return "force move";
		case ZHBO_ORDER_ATTACK_MOVE: return "attack-move";
		case ZHBO_ORDER_STOP: return "stop";
		case ZHBO_ORDER_ATTACK: return "attack";
		case ZHBO_ORDER_FORCE_ATTACK: return "force attack";
		case ZHBO_ORDER_FORCE_ATTACK_GROUND: return "force attack ground";
		case ZHBO_ORDER_GUARD_POSITION: return "guard position";
		case ZHBO_ORDER_GUARD_OBJECT: return "guard object";
		case ZHBO_ORDER_MAKE_GROUP: return "make group";
		default: return nullptr;
		}
	}
} // namespace

int32_t cbCommandObjects(int32_t playerIndex, ZhboCmdObject *out, int32_t cap)
{
	Player *asker = ThePlayerList ? ThePlayerList->getNthPlayer(playerIndex) : nullptr;
	if (!asker || !out || cap <= 0 || !TheGameLogic)
		return 0;
	int32_t n = 0;
	for (Object *obj = TheGameLogic->getFirstObject(); obj && n < cap; obj = obj->getNextObject())
	{
		// What a click can take: selectable things (units and buildings, anyone's), not scenery,
		// not a passenger.
		if (obj->isEffectivelyDead() || !obj->isSelectable() || obj->isContained())
			continue;
		const int32_t relation = relationOf(asker, obj);
		const Bool structure = obj->isKindOf(KINDOF_STRUCTURE);
		Bool ghost = FALSE;
		if (relation != ZHBO_REL_OWN)
		{
			// Others' objects as the overhead view shows them: in sight, or a building seen before
			// and now in fog (its ghost object is what is drawn). Never a unit in fog or shroud.
			const ObjectShroudStatus status = obj->getShroudedStatus(playerIndex);
			if (status == OBJECTSHROUD_FOGGED && structure)
			{
				const PartitionData *pd = obj->friend_getPartitionData();
				ghost = pd && pd->getGhostObject();
				if (!ghost)
					continue;
			}
			else if (status != OBJECTSHROUD_CLEAR && status != OBJECTSHROUD_PARTIAL_CLEAR)
				continue;
		}
		ZhboCmdObject &o = out[n++];
		memset(&o, 0, sizeof(o));
		o.id = (uint32_t)obj->getID();
		o.tmpl = (uint32_t)obj->getTemplate()->getTemplateID();
		o.relation = relation;
		const Coord3D *p = obj->getPosition();
		o.pos.x = p->x;
		o.pos.y = p->y;
		o.pos.z = p->z;
		const BodyModuleInterface *body = obj->getBodyModule();
		o.health = body && body->getMaxHealth() > 0.0f ? body->getHealth() / body->getMaxHealth() : 1.0f;
		o.radius = obj->getGeometryInfo().getBoundingSphereRadius();
		o.veterancy = (int32_t)obj->getVeterancyLevel();
		uint32_t f = 0;
		if (structure) f |= ZHBO_CMD_STRUCTURE;
		if (obj->isKindOf(KINDOF_INFANTRY)) f |= ZHBO_CMD_INFANTRY;
		if (obj->isKindOf(KINDOF_VEHICLE)) f |= ZHBO_CMD_VEHICLE;
		if (obj->isKindOf(KINDOF_AIRCRAFT)) f |= ZHBO_CMD_AIRCRAFT;
		if (obj->isKindOf(KINDOF_DOZER)) f |= ZHBO_CMD_DOZER;
		if (obj->isKindOf(KINDOF_HARVESTER)) f |= ZHBO_CMD_HARVESTER;
		if (ghost) f |= ZHBO_CMD_GHOST;
		const AIUpdateInterface *ai = obj->getAI();
		if (ai && ai->isIdle()) f |= ZHBO_CMD_IDLE;
		const ContainModuleInterface *contain = obj->getContain();
		if (contain && contain->getContainCount() > 0) f |= ZHBO_CMD_LOADED;
		// The General is the player's own first person: never part of a selection.
		if (isGeneral(obj))
			f |= ZHBO_CMD_GENERAL;
		else if (relation == ZHBO_REL_OWN)
		{
			f |= ZHBO_CMD_SELECTABLE;
			if (selectAllTakes(obj))
				f |= ZHBO_CMD_SELECT_ALL;
		}
		o.flags = f;
		if (relation == ZHBO_REL_OWN)
			for (Int g = 0; g < NUM_HOTKEY_SQUADS; ++g)
			{
				Squad *squad = asker->getHotkeySquad(g);
				if (squad && squad->isOnSquad(obj))
					o.groups |= 1u << g;
			}
	}
	return n;
}

int32_t cbTemplateName(uint32_t tmpl, char *out, int32_t cap)
{
	if (!out || cap <= 0)
		return 0;
	out[0] = 0;
	const ThingTemplate *t = TheThingFactory ? TheThingFactory->findByTemplateID((UnsignedShort)tmpl) : nullptr;
	if (!t)
		return 0;
	AsciiString name;
	name.translate(t->getDisplayName());
	const Int n = std::min<Int>(name.getLength(), cap - 1);
	memcpy(out, name.str(), (size_t)n);
	out[n] = 0;
	return n;
}

void cbIssueOrder(const ZhboCmdOrder *order, const uint32_t *ids, int32_t count)
{
	const char *name = order ? orderName(order->kind) : nullptr;
	Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
	if (!name || !ids || count <= 0 || !local || !TheMessageStream)
		return;
	// The selection as the overhead view makes it (the autopilot's mpSelect): the drawables picked
	// locally, then MSG_CREATE_SELECTED_GROUP with the ids, which every machine replays.
	if (TheInGameUI)
		TheInGameUI->deselectAllDrawables();
	std::vector<ObjectID> mine;
	for (int32_t i = 0; i < count; ++i)
	{
		Object *obj = TheGameLogic->findObjectByID((ObjectID)ids[i]);
		if (!obj || obj->getControllingPlayer() != local || isGeneral(obj))
			continue;
		mine.push_back(obj->getID());
		if (TheInGameUI && obj->getDrawable())
			TheInGameUI->selectDrawable(obj->getDrawable());
	}
	if (mine.empty())
	{
		logf("command order: %s to nobody (none of the %d ids is the player's)", name, (int)count);
		return;
	}
	if (order->kind == ZHBO_ORDER_MAKE_GROUP)
	{
		if (order->arg < 0 || order->arg >= NUM_HOTKEY_SQUADS)
			return;
		GameMessage *team = TheMessageStream->appendMessage((GameMessage::Type)(GameMessage::MSG_CREATE_TEAM0 + order->arg));
		for (ObjectID id : mine)
			team->appendObjectIDArgument(id);
		logf("command order: make group %d of %d object(s)", (int)order->arg, (int)mine.size());
		return;
	}
	GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
	group->appendBooleanArgument(TRUE);
	for (ObjectID id : mine)
		group->appendObjectIDArgument(id);

	Coord3D pos;
	pos.set(order->pos.x, order->pos.y, order->pos.z);
	switch (order->kind)
	{
	case ZHBO_ORDER_MOVE:
		TheMessageStream->appendMessage(order->queued ? GameMessage::MSG_ADD_WAYPOINT : GameMessage::MSG_DO_MOVETO)->appendLocationArgument(pos);
		break;
	case ZHBO_ORDER_FORCE_MOVE:
		TheMessageStream->appendMessage(GameMessage::MSG_DO_FORCEMOVETO)->appendLocationArgument(pos);
		break;
	case ZHBO_ORDER_ATTACK_MOVE:
		TheMessageStream->appendMessage(GameMessage::MSG_DO_ATTACKMOVETO)->appendLocationArgument(pos);
		break;
	case ZHBO_ORDER_STOP:
		TheMessageStream->appendMessage(GameMessage::MSG_DO_STOP);
		break;
	case ZHBO_ORDER_ATTACK:
		TheMessageStream->appendMessage(GameMessage::MSG_DO_ATTACK_OBJECT)->appendObjectIDArgument((ObjectID)order->target);
		if (s_config.autopilot_command)
			autopilotCommandAttackOrdered();
		break;
	case ZHBO_ORDER_FORCE_ATTACK:
		TheMessageStream->appendMessage(GameMessage::MSG_DO_FORCE_ATTACK_OBJECT)->appendObjectIDArgument((ObjectID)order->target);
		break;
	case ZHBO_ORDER_FORCE_ATTACK_GROUND:
		TheMessageStream->appendMessage(GameMessage::MSG_DO_FORCE_ATTACK_GROUND)->appendLocationArgument(pos);
		break;
	case ZHBO_ORDER_GUARD_POSITION:
	{
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_GUARD_POSITION);
		msg->appendLocationArgument(pos);
		msg->appendIntegerArgument(GUARDMODE_NORMAL);
		break;
	}
	case ZHBO_ORDER_GUARD_OBJECT:
	{
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_GUARD_OBJECT);
		msg->appendObjectIDArgument((ObjectID)order->target);
		msg->appendIntegerArgument(GUARDMODE_NORMAL);
		break;
	}
	default:
		break;
	}
	logf("command order: %s%s to %d object(s), target %u at (%.0f, %.0f)", name, order->queued ? " (queued)" : "", (int)mine.size(),
		(unsigned)order->target, pos.x, pos.y);
}
