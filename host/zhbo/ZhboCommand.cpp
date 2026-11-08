/*
** Conquer Ops - engine-side bridge: the first-person command services (docs/COMMANDS.md): what the
** player could click overhead, the buttons of his command sets, the templates' display names, and
** orders sent as the overhead view sends them.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
#include "Common/MultiplayerSettings.h"
#include "Common/Science.h"
#include "Common/SpecialPower.h"
#include "Common/Upgrade.h"
#include "GameClient/CommandXlat.h"
#include "GameClient/GameText.h"
#include "GameLogic/Module/ContainModule.h"
#include "GameLogic/Module/ProductionUpdate.h"
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

	// The bridge's stable code for each GUICommandType (ALLOW_SURRENDER shifts the engine's numbers).
	int32_t buttonCode(GUICommandType t)
	{
		switch (t)
		{
		case GUI_COMMAND_DOZER_CONSTRUCT: return ZHBO_BUTTON_DOZER_CONSTRUCT;
		case GUI_COMMAND_DOZER_CONSTRUCT_CANCEL: return ZHBO_BUTTON_DOZER_CONSTRUCT_CANCEL;
		case GUI_COMMAND_UNIT_BUILD: return ZHBO_BUTTON_UNIT_BUILD;
		case GUI_COMMAND_CANCEL_UNIT_BUILD: return ZHBO_BUTTON_CANCEL_UNIT_BUILD;
		case GUI_COMMAND_PLAYER_UPGRADE: return ZHBO_BUTTON_PLAYER_UPGRADE;
		case GUI_COMMAND_OBJECT_UPGRADE: return ZHBO_BUTTON_OBJECT_UPGRADE;
		case GUI_COMMAND_CANCEL_UPGRADE: return ZHBO_BUTTON_CANCEL_UPGRADE;
		case GUI_COMMAND_ATTACK_MOVE: return ZHBO_BUTTON_ATTACK_MOVE;
		case GUI_COMMAND_GUARD: return ZHBO_BUTTON_GUARD;
		case GUI_COMMAND_GUARD_WITHOUT_PURSUIT: return ZHBO_BUTTON_GUARD_WITHOUT_PURSUIT;
		case GUI_COMMAND_GUARD_FLYING_UNITS_ONLY: return ZHBO_BUTTON_GUARD_FLYING_UNITS_ONLY;
		case GUI_COMMAND_STOP: return ZHBO_BUTTON_STOP;
		case GUI_COMMAND_WAYPOINTS: return ZHBO_BUTTON_WAYPOINTS;
		case GUI_COMMAND_EXIT_CONTAINER: return ZHBO_BUTTON_EXIT_CONTAINER;
		case GUI_COMMAND_EVACUATE: return ZHBO_BUTTON_EVACUATE;
		case GUI_COMMAND_EXECUTE_RAILED_TRANSPORT: return ZHBO_BUTTON_EXECUTE_RAILED_TRANSPORT;
		case GUI_COMMAND_BEACON_DELETE: return ZHBO_BUTTON_BEACON_DELETE;
		case GUI_COMMAND_SET_RALLY_POINT: return ZHBO_BUTTON_SET_RALLY_POINT;
		case GUI_COMMAND_SELL: return ZHBO_BUTTON_SELL;
		case GUI_COMMAND_FIRE_WEAPON: return ZHBO_BUTTON_FIRE_WEAPON;
		case GUI_COMMAND_SPECIAL_POWER: return ZHBO_BUTTON_SPECIAL_POWER;
		case GUI_COMMAND_PURCHASE_SCIENCE: return ZHBO_BUTTON_PURCHASE_SCIENCE;
		case GUI_COMMAND_HACK_INTERNET: return ZHBO_BUTTON_HACK_INTERNET;
		case GUI_COMMAND_TOGGLE_OVERCHARGE: return ZHBO_BUTTON_TOGGLE_OVERCHARGE;
#ifdef ALLOW_SURRENDER
		case GUI_COMMAND_POW_RETURN_TO_PRISON: return ZHBO_BUTTON_POW_RETURN_TO_PRISON;
		case GUICOMMANDMODE_PICK_UP_PRISONER: return ZHBO_BUTTON_PICK_UP_PRISONER;
#endif
		case GUI_COMMAND_COMBATDROP: return ZHBO_BUTTON_COMBATDROP;
		case GUI_COMMAND_SWITCH_WEAPON: return ZHBO_BUTTON_SWITCH_WEAPON;
		case GUICOMMANDMODE_HIJACK_VEHICLE: return ZHBO_BUTTON_HIJACK_VEHICLE;
		case GUICOMMANDMODE_CONVERT_TO_CARBOMB: return ZHBO_BUTTON_CONVERT_TO_CARBOMB;
		case GUICOMMANDMODE_SABOTAGE_BUILDING: return ZHBO_BUTTON_SABOTAGE_BUILDING;
		case GUICOMMANDMODE_PLACE_BEACON: return ZHBO_BUTTON_PLACE_BEACON;
		case GUI_COMMAND_SPECIAL_POWER_FROM_SHORTCUT: return ZHBO_BUTTON_SPECIAL_POWER_FROM_SHORTCUT;
		case GUI_COMMAND_SPECIAL_POWER_CONSTRUCT: return ZHBO_BUTTON_SPECIAL_POWER_CONSTRUCT;
		case GUI_COMMAND_SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT: return ZHBO_BUTTON_SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT;
		case GUI_COMMAND_SELECT_ALL_UNITS_OF_TYPE: return ZHBO_BUTTON_SELECT_ALL_UNITS_OF_TYPE;
		default: return ZHBO_BUTTON_NONE;
		}
	}

	void copyText(char *dst, size_t cap, const UnicodeString &text)
	{
		AsciiString a;
		a.translate(text);
		const size_t n = std::min<size_t>((size_t)a.getLength(), cap - 1);
		memcpy(dst, a.str(), n);
		dst[n] = 0;
	}

	// The player's own command set `set` (ZHBO_PLAYERSET_*).
	const CommandSet *playerSet(const Player *player, int32_t set)
	{
		const PlayerTemplate *pt = player ? player->getPlayerTemplate() : nullptr;
		if (!pt || !TheControlBar)
			return nullptr;
		switch (set)
		{
		case ZHBO_PLAYERSET_SHORTCUTS: return TheControlBar->findCommandSet(pt->getSpecialPowerShortcutCommandSet());
		case ZHBO_PLAYERSET_RANK1: return TheControlBar->findCommandSet(pt->getPurchaseScienceCommandSetRank1());
		case ZHBO_PLAYERSET_RANK3: return TheControlBar->findCommandSet(pt->getPurchaseScienceCommandSetRank3());
		case ZHBO_PLAYERSET_RANK8: return TheControlBar->findCommandSet(pt->getPurchaseScienceCommandSetRank8());
		default: return nullptr;
		}
	}

	const CommandSet *ownerSet(const Object *obj)
	{
		return obj && TheControlBar ? TheControlBar->findCommandSet(obj->getCommandSetString()) : nullptr;
	}

	// The science a promotion button would buy now: the first of its ranks the player lacks, has
	// the prerequisites for and can pay for (ControlBar::processCommandUI's PURCHASE_SCIENCE).
	ScienceType purchasable(const CommandButton *b, Player *player)
	{
		for (size_t i = 0; i < b->getScienceVec().size(); ++i)
		{
			const ScienceType st = b->getScienceVec()[i];
			if (!player->hasScience(st) && TheScienceStore->playerHasPrereqsForScience(player, st) &&
				TheScienceStore->getSciencePurchaseCost(st) <= player->getSciencePurchasePoints())
				return st;
		}
		return SCIENCE_INVALID;
	}

	// Whether a button can be pressed now, as the control bar would show it. The player's own sets
	// ask the object that would carry it out: the most ready shortcut building, or (a promotion) the
	// player's points.
	int32_t availabilityOf(const CommandButton *b, Object *obj, Player *player)
	{
		if (b->getCommandType() == GUI_COMMAND_PURCHASE_SCIENCE)
		{
			for (size_t i = 0; i < b->getScienceVec().size(); ++i)
				if (player->hasScience(b->getScienceVec()[i]))
					return COMMAND_ACTIVE;
			return purchasable(b, player) != SCIENCE_INVALID ? COMMAND_AVAILABLE : COMMAND_RESTRICTED;
		}
		if (!obj && b->getSpecialPowerTemplate() &&
			(b->getCommandType() == GUI_COMMAND_SPECIAL_POWER_FROM_SHORTCUT || b->getCommandType() == GUI_COMMAND_SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT))
			obj = player->findMostReadyShortcutSpecialPowerOfType(b->getSpecialPowerTemplate()->getSpecialPowerType());
		if (!obj)
			return COMMAND_HIDDEN;
		return (int32_t)zhboCommandAvailability(b, obj);
	}

	// What a button is about, as display text: the upgrade's name, else its label (a power, a
	// promotion, a weapon mode); empty for a build (its template names it).
	void buttonName(const CommandButton *b, char *out, size_t cap)
	{
		out[0] = 0;
		if (!TheGameText)
			return;
		if (const UpgradeTemplate *u = b->getUpgradeTemplate())
		{
			if (!u->getDisplayNameLabel().isEmpty())
			{
				copyText(out, cap, TheGameText->fetch(u->getDisplayNameLabel()));
				return;
			}
		}
		if (b->getThingTemplate() && b->getCommandType() != GUI_COMMAND_SPECIAL_POWER && b->getCommandType() != GUI_COMMAND_SPECIAL_POWER_FROM_SHORTCUT)
			return;
		if (!b->getTextLabel().isEmpty())
			copyText(out, cap, TheGameText->fetch(b->getTextLabel()));
	}

	void addButtons(const CommandSet *set, uint32_t owner, int32_t setCode, Object *obj, Player *player, ZhboCmdButton *out, int32_t cap, int32_t &n)
	{
		for (Int i = 0; set && i < MAX_COMMANDS_PER_SET && n < cap; ++i)
		{
			const CommandButton *b = set->getCommandButton(i);
			const int32_t code = b ? buttonCode(b->getCommandType()) : ZHBO_BUTTON_NONE;
			if (code == ZHBO_BUTTON_NONE)
				continue;
			ZhboCmdButton &o = out[n++];
			memset(&o, 0, sizeof(o));
			o.owner = owner;
			o.set = setCode;
			o.slot = i;
			o.code = code;
			o.options = (uint32_t)b->getOptions();
			o.tmpl = b->getThingTemplate() ? (uint32_t)b->getThingTemplate()->getTemplateID() : 0;
			o.availability = availabilityOf(b, obj, player);
			buttonName(b, o.name, sizeof(o.name));
			if (TheGameText && !b->getTextLabel().isEmpty())
				copyText(o.label, sizeof(o.label), TheGameText->fetch(b->getTextLabel()));
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
		case ZHBO_ORDER_BUTTON: return "button";
		case ZHBO_ORDER_CONSTRUCT: return "construct";
		case ZHBO_ORDER_CANCEL: return "cancel";
		case ZHBO_ORDER_UNLOAD: return "unload";
		case ZHBO_ORDER_CONTEXT: return "right-click";
		case ZHBO_ORDER_SCATTER: return "scatter";
		case ZHBO_ORDER_FORM_UP: return "form up";
		case ZHBO_ORDER_CHEER: return "cheer";
		case ZHBO_ORDER_PLACE_BEACON: return "place beacon";
		case ZHBO_ORDER_REMOVE_BEACON: return "remove beacon";
		default: return nullptr;
		}
	}

	// What evaluateContextCommand's message does, as ZHBO_CTX_* (0: it does nothing).
	int32_t contextOutcome(GameMessage::Type t)
	{
		switch (t)
		{
		case GameMessage::MSG_DO_MOVETO:
		case GameMessage::MSG_DO_FORCEMOVETO:
		case GameMessage::MSG_DO_ATTACKMOVETO:
		case GameMessage::MSG_ADD_WAYPOINT: return ZHBO_CTX_MOVE;
		case GameMessage::MSG_DO_ATTACK_OBJECT:
		case GameMessage::MSG_DO_FORCE_ATTACK_OBJECT:
		case GameMessage::MSG_DO_FORCE_ATTACK_GROUND:
		case GameMessage::MSG_DO_WEAPON_AT_OBJECT:
		case GameMessage::MSG_DO_WEAPON_AT_LOCATION: return ZHBO_CTX_ATTACK;
		case GameMessage::MSG_ENTER: return ZHBO_CTX_ENTER;
		case GameMessage::MSG_CAPTUREBUILDING: return ZHBO_CTX_CAPTURE;
		case GameMessage::MSG_DO_REPAIR: return ZHBO_CTX_REPAIR;
		case GameMessage::MSG_RESUME_CONSTRUCTION: return ZHBO_CTX_RESUME_CONSTRUCTION;
		case GameMessage::MSG_GET_REPAIRED: return ZHBO_CTX_GET_REPAIRED;
		case GameMessage::MSG_GET_HEALED: return ZHBO_CTX_GET_HEALED;
		case GameMessage::MSG_DOCK: return ZHBO_CTX_DOCK;
		case GameMessage::MSG_CONVERT_TO_CARBOMB: return ZHBO_CTX_CARBOMB;
		case GameMessage::MSG_DISABLEVEHICLE_HACK:
		case GameMessage::MSG_DISABLEBUILDING_HACK:
		case GameMessage::MSG_STEALCASH_HACK: return ZHBO_CTX_HACK;
		case GameMessage::MSG_DO_SALVAGE: return ZHBO_CTX_SALVAGE;
		case GameMessage::MSG_DO_SPECIAL_POWER:
		case GameMessage::MSG_DO_SPECIAL_POWER_AT_LOCATION:
		case GameMessage::MSG_DO_SPECIAL_POWER_AT_OBJECT:
		case GameMessage::MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION: return ZHBO_CTX_SPECIAL_POWER;
		case GameMessage::MSG_SET_RALLY_POINT: return ZHBO_CTX_RALLY_POINT;
		case GameMessage::MSG_INVALID:
		case GameMessage::MSG_DO_INVALID_HINT:
		case GameMessage::MSG_INVALID_GUICOMMAND_HINT:
		case GameMessage::MSG_IMPOSSIBLE_ATTACK_HINT: return 0;
		default: return ZHBO_CTX_OTHER;
		}
	}

	// The keyboard's modifier modes are first person's keys (Ctrl is prone): a click from here is a
	// plain click, so they are off while the translator evaluates it, and restored after.
	struct PlainClick
	{
		Bool force, waypoint, prefer;
		PlainClick()
			: force(TheInGameUI->isInForceAttackMode()), waypoint(TheInGameUI->isInWaypointMode()), prefer(TheInGameUI->isInPreferSelectionMode())
		{
			TheInGameUI->setForceAttackMode(FALSE);
			TheInGameUI->setWaypointMode(FALSE);
			TheInGameUI->setPreferSelectionMode(FALSE);
		}
		~PlainClick()
		{
			TheInGameUI->setGUICommand(nullptr);
			TheInGameUI->setForceAttackMode(force);
			TheInGameUI->setWaypointMode(waypoint);
			TheInGameUI->setPreferSelectionMode(prefer);
		}
	};

	int32_t canMakeCode(CanMakeType cmt)
	{
		switch (cmt)
		{
		case CANMAKE_OK: return ZHBO_ISSUED_SENT;
		case CANMAKE_NO_MONEY: return ZHBO_ISSUED_CANT_AFFORD;
		case CANMAKE_QUEUE_FULL:
		case CANMAKE_PARKING_PLACES_FULL: return ZHBO_ISSUED_QUEUE_FULL;
		default: return ZHBO_ISSUED_UNAVAILABLE;
		}
	}

	int32_t availabilityCode(int32_t a)
	{
		switch (a)
		{
		case COMMAND_AVAILABLE:
		case COMMAND_ACTIVE: return ZHBO_ISSUED_SENT;
		case COMMAND_NOT_READY: return ZHBO_ISSUED_NOT_READY;
		case COMMAND_CANT_AFFORD: return ZHBO_ISSUED_CANT_AFFORD;
		default: return ZHBO_ISSUED_UNAVAILABLE;
		}
	}

	// The engine's own click handling, owned here: evaluateContextCommand reads only the UI's state
	// (the selected drawables, a pending GUI command), never the mouse a translator saw.
	CommandTranslator &translator()
	{
		static CommandTranslator s_translator;
		return s_translator;
	}

	// The selection as the overhead view makes it (the autopilot's mpSelect): the drawables picked
	// locally, then MSG_CREATE_SELECTED_GROUP with the ids, which every machine replays. The
	// bridge's translator sees the message too, as the stream's translator sees a selection: its
	// move and attack paths act only once it knows a group exists (CommandXlat's m_teamExists).
	void select(const std::vector<Object *> &objs)
	{
		if (TheInGameUI)
			TheInGameUI->deselectAllDrawables();
		GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
		group->appendBooleanArgument(TRUE);
		for (Object *obj : objs)
		{
			group->appendObjectIDArgument(obj->getID());
			if (TheInGameUI && obj->getDrawable())
				TheInGameUI->selectDrawable(obj->getDrawable());
		}
		// Through the base, as the message stream calls every translator.
		static_cast<GameMessageTranslator &>(translator()).translateGameMessage(group);
	}

	// The nearest place to `at` where `what` may go down for `builder`, as the overhead placement
	// tests it (PlaceEventTranslator's isLocationLegalToBuild flags), searching rings outward a
	// footprint radius apart.
	Bool legalPlace(const ThingTemplate *what, Object *builder, Real angle, Coord3D &at)
	{
		const UnsignedInt checks = BuildAssistant::USE_QUICK_PATHFIND | BuildAssistant::TERRAIN_RESTRICTIONS | BuildAssistant::CLEAR_PATH |
			BuildAssistant::NO_OBJECT_OVERLAP | BuildAssistant::SHROUD_REVEALED | BuildAssistant::IGNORE_STEALTHED |
			BuildAssistant::FAIL_STEALTHED_WITHOUT_FEEDBACK;
		const Real step = std::max(10.0f, what->getTemplateGeometryInfo().getBoundingCircleRadius());
		const Coord3D centre = at;
		for (Int ring = 0; ring <= 8; ++ring)
		{
			const Int around = ring == 0 ? 1 : 8 * ring;
			for (Int k = 0; k < around; ++k)
			{
				const Real a = (2.0f * PI * k) / around;
				Coord3D p;
				p.set(centre.x + Cos(a) * step * ring, centre.y + Sin(a) * step * ring, 0.0f);
				p.z = TheTerrainLogic->getGroundHeight(p.x, p.y);
				if (TheBuildAssistant->isLocationLegalToBuild(&p, what, angle, checks, builder, nullptr) == LBC_OK)
				{
					at = p;
					return TRUE;
				}
			}
		}
		return FALSE;
	}

	// ZHBO_ORDER_BUTTON: the button pressed as ControlBar::processCommandUI does; one that needs a
	// target finished as the world click would (CommandTranslator::evaluateContextCommand with it
	// pending); a special power's construction placed as PlaceEventTranslator places it.
	int32_t pressButton(const ZhboCmdOrder &o, Object *obj, Player *local)
	{
		const CommandSet *set = obj ? ownerSet(obj) : playerSet(local, o.set);
		const CommandButton *b = set && o.slot >= 0 && o.slot < MAX_COMMANDS_PER_SET ? set->getCommandButton(o.slot) : nullptr;
		if (!b)
			return ZHBO_ISSUED_UNAVAILABLE;
		const int32_t ready = availabilityCode(availabilityOf(b, obj, local));
		if (ready != ZHBO_ISSUED_SENT)
			return ready;
		const GUICommandType type = b->getCommandType();
		Coord3D pos;
		pos.set(o.pos.x, o.pos.y, o.pos.z);
		// The object a shortcut fires from (processCommandUI's findMostReadyShortcutSpecialPowerOfType).
		Object *source = obj;
		if (!source && b->getSpecialPowerTemplate())
			source = local->findMostReadyShortcutSpecialPowerOfType(b->getSpecialPowerTemplate()->getSpecialPowerType());
		if (obj)
			select({ obj });
		if (obj && BitIsSet(b->getOptions(), SINGLE_USE_COMMAND))
			obj->markSingleUseCommandUsed();

		if (type == GUI_COMMAND_SPECIAL_POWER_CONSTRUCT || type == GUI_COMMAND_SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT)
		{
			const ThingTemplate *what = b->getThingTemplate();
			if (!source || !what || !b->getSpecialPowerTemplate())
				return ZHBO_ISSUED_UNAVAILABLE;
			const int32_t can = canMakeCode(TheBuildAssistant->canMakeUnit(source, what));
			if (can != ZHBO_ISSUED_SENT)
				return can;
			const Real angle = what->getPlacementViewAngle();
			if (!legalPlace(what, source, angle, pos))
				return ZHBO_ISSUED_NO_PLACE;
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_SPECIAL_POWER_AT_LOCATION);
			msg->appendIntegerArgument(b->getSpecialPowerTemplate()->getID());
			msg->appendLocationArgument(pos);
			msg->appendRealArgument(angle);
			msg->appendObjectIDArgument(INVALID_ID);
			msg->appendIntegerArgument(b->getOptions());
			msg->appendObjectIDArgument(source->getID());
			return ZHBO_ISSUED_SENT;
		}

		if (BitIsSet(b->getOptions(), COMMAND_OPTION_NEED_TARGET))
		{
			if (b->getOptions() & USES_MINE_CLEARING_WEAPONSET)
				TheMessageStream->appendMessage(GameMessage::MSG_SET_MINE_CLEARING_DETAIL);
			PlainClick plain;
			TheInGameUI->setGUICommand(b);
			Object *target = o.target ? TheGameLogic->findObjectByID((ObjectID)o.target) : nullptr;
			Drawable *draw = target ? target->getDrawable() : nullptr;
			const GameMessage::Type t = translator().evaluateContextCommand(draw, &pos, CommandTranslator::DO_COMMAND);
			return contextOutcome(t) ? ZHBO_ISSUED_SENT : ZHBO_ISSUED_BAD_TARGET;
		}

		switch (type)
		{
		case GUI_COMMAND_DOZER_CONSTRUCT_CANCEL:
			TheMessageStream->appendMessage(GameMessage::MSG_DOZER_CANCEL_CONSTRUCT);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_UNIT_BUILD:
		{
			const ThingTemplate *what = b->getThingTemplate();
			ProductionUpdateInterface *pu = obj ? obj->getProductionUpdateInterface() : nullptr;
			if (!what || !pu)
				return ZHBO_ISSUED_UNAVAILABLE;
			const int32_t can = canMakeCode(TheBuildAssistant->canMakeUnit(obj, what));
			if (can != ZHBO_ISSUED_SENT)
				return can;
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_QUEUE_UNIT_CREATE);
			msg->appendIntegerArgument(what->getTemplateID());
			msg->appendIntegerArgument(pu->requestUniqueUnitID());
			return ZHBO_ISSUED_SENT;
		}
		case GUI_COMMAND_PLAYER_UPGRADE:
		case GUI_COMMAND_OBJECT_UPGRADE:
		{
			const UpgradeTemplate *u = b->getUpgradeTemplate();
			if (!u || !obj)
				return ZHBO_ISSUED_UNAVAILABLE;
			if (!TheUpgradeCenter->canAffordUpgrade(local, u, FALSE))
				return ZHBO_ISSUED_CANT_AFFORD;
			if (ProductionUpdateInterface *pu = obj->getProductionUpdateInterface())
				if (pu->canQueueUpgrade(u) == CANMAKE_QUEUE_FULL)
					return ZHBO_ISSUED_QUEUE_FULL;
			if (type == GUI_COMMAND_OBJECT_UPGRADE && (obj->hasUpgrade(u) || !obj->affectedByUpgrade(u)))
				return ZHBO_ISSUED_UNAVAILABLE;
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_QUEUE_UPGRADE);
			msg->appendObjectIDArgument(obj->getID());
			msg->appendIntegerArgument(u->getUpgradeNameKey());
			return ZHBO_ISSUED_SENT;
		}
		case GUI_COMMAND_STOP:
			TheMessageStream->appendMessage(GameMessage::MSG_DO_STOP);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_EVACUATE:
			TheMessageStream->appendMessage(GameMessage::MSG_EVACUATE);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_EXECUTE_RAILED_TRANSPORT:
			TheMessageStream->appendMessage(GameMessage::MSG_EXECUTE_RAILED_TRANSPORT);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_HACK_INTERNET:
			TheMessageStream->appendMessage(GameMessage::MSG_INTERNET_HACK);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_SELL:
			TheMessageStream->appendMessage(GameMessage::MSG_SELL);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_TOGGLE_OVERCHARGE:
			TheMessageStream->appendMessage(GameMessage::MSG_TOGGLE_OVERCHARGE);
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_SWITCH_WEAPON:
			TheMessageStream->appendMessage(GameMessage::MSG_SWITCH_WEAPONS)->appendIntegerArgument(b->getWeaponSlot());
			return ZHBO_ISSUED_SENT;
		case GUI_COMMAND_FIRE_WEAPON:
		{
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_WEAPON);
			msg->appendIntegerArgument(b->getWeaponSlot());
			msg->appendIntegerArgument(b->getMaxShotsToFire());
			return ZHBO_ISSUED_SENT;
		}
		case GUI_COMMAND_SPECIAL_POWER_FROM_SHORTCUT:
		case GUI_COMMAND_SPECIAL_POWER:
		{
			const SpecialPowerTemplate *sp = b->getSpecialPowerTemplate();
			if (!sp || !source)
				return ZHBO_ISSUED_UNAVAILABLE;
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_SPECIAL_POWER);
			msg->appendIntegerArgument(sp->getID());
			msg->appendIntegerArgument(b->getOptions());
			msg->appendObjectIDArgument(type == GUI_COMMAND_SPECIAL_POWER ? INVALID_ID : source->getID());
			return ZHBO_ISSUED_SENT;
		}
		case GUI_COMMAND_PURCHASE_SCIENCE:
		{
			const ScienceType st = purchasable(b, local);
			if (st == SCIENCE_INVALID)
				return ZHBO_ISSUED_UNAVAILABLE;
			TheMessageStream->appendMessage(GameMessage::MSG_PURCHASE_SCIENCE)->appendIntegerArgument(st);
			return ZHBO_ISSUED_SENT;
		}
		default:
			// The rest are no presses of their own: the language orders them directly (attack-move,
			// guard, stop, a build through ZHBO_ORDER_CONSTRUCT, `then` for waypoints, a type as the
			// selection).
			return ZHBO_ISSUED_UNAVAILABLE;
		}
	}

	// ZHBO_ORDER_CONSTRUCT: MSG_DOZER_CONSTRUCT at the nearest legal place (PlaceEventTranslator's
	// test), or MSG_DOZER_CONSTRUCT_LINE for a line structure (a wall), from pos to end.
	int32_t construct(const ZhboCmdOrder &o, Object *builder)
	{
		const ThingTemplate *what = TheThingFactory->findByTemplateID((UnsignedShort)o.tmpl);
		if (!what)
			return ZHBO_ISSUED_UNAVAILABLE;
		const int32_t can = canMakeCode(TheBuildAssistant->canMakeUnit(builder, what));
		if (can != ZHBO_ISSUED_SENT)
			return can;
		// The template's own angle unless `facing` named one (InGameUI::placeBuildAvailable).
		const Real angle = o.angle == o.angle ? o.angle : what->getPlacementViewAngle();
		Coord3D at;
		at.set(o.pos.x, o.pos.y, TheTerrainLogic->getGroundHeight(o.pos.x, o.pos.y));
		const Bool line = what->isKindOf(KINDOF_LINEBUILD);
		if (o.arg == 1 && !line)
			return ZHBO_ISSUED_BAD_TARGET;
		if (!line && !legalPlace(what, builder, angle, at))
			return ZHBO_ISSUED_NO_PLACE;
		select({ builder });
		GameMessage *msg = TheMessageStream->appendMessage(line ? GameMessage::MSG_DOZER_CONSTRUCT_LINE : GameMessage::MSG_DOZER_CONSTRUCT);
		msg->appendIntegerArgument(what->getTemplateID());
		msg->appendLocationArgument(at);
		msg->appendRealArgument(angle);
		if (line)
		{
			Coord3D end = at;
			if (o.arg == 1)
				end.set(o.end.x, o.end.y, TheTerrainLogic->getGroundHeight(o.end.x, o.end.y));
			msg->appendLocationArgument(end);
		}
		return ZHBO_ISSUED_SENT;
	}

	// ZHBO_ORDER_CANCEL: in the first producer that has one queued, the last `tmpl` (its production
	// id, MSG_CANCEL_UNIT_CREATE) or the upgrade of button `slot` (MSG_CANCEL_UPGRADE).
	int32_t cancel(const ZhboCmdOrder &o, const std::vector<Object *> &producers)
	{
		for (Object *p : producers)
		{
			ProductionUpdateInterface *pu = p->getProductionUpdateInterface();
			if (!pu)
				continue;
			if (o.tmpl)
			{
				ProductionID last = PRODUCTIONID_INVALID;
				for (const ProductionEntry *e = pu->firstProduction(); e; e = pu->nextProduction(e))
					if (e->getProductionObject() && e->getProductionObject()->getTemplateID() == o.tmpl)
						last = e->getProductionID();
				if (last == PRODUCTIONID_INVALID)
					continue;
				select({ p });
				TheMessageStream->appendMessage(GameMessage::MSG_CANCEL_UNIT_CREATE)->appendIntegerArgument(last);
				return ZHBO_ISSUED_SENT;
			}
			const CommandSet *set = ownerSet(p);
			const CommandButton *b = set && o.slot >= 0 && o.slot < MAX_COMMANDS_PER_SET ? set->getCommandButton(o.slot) : nullptr;
			const UpgradeTemplate *u = b ? b->getUpgradeTemplate() : nullptr;
			if (!u || !pu->isUpgradeInQueue(u))
				continue;
			select({ p });
			TheMessageStream->appendMessage(GameMessage::MSG_CANCEL_UPGRADE)->appendIntegerArgument(u->getUpgradeNameKey());
			return ZHBO_ISSUED_SENT;
		}
		return ZHBO_ISSUED_NOT_FOUND;
	}

	// ZHBO_ORDER_CONTEXT: the right-click, only if what it would do is allowed.
	int32_t rightClick(const ZhboCmdOrder &o, const std::vector<Object *> &objs)
	{
		select(objs);
		PlainClick plain;
		Object *target = o.target ? TheGameLogic->findObjectByID((ObjectID)o.target) : nullptr;
		Drawable *draw = target ? target->getDrawable() : nullptr;
		if (o.target && !draw)
			return ZHBO_ISSUED_BAD_TARGET;
		Coord3D pos;
		pos.set(o.pos.x, o.pos.y, o.pos.z);
		const int32_t would = contextOutcome(translator().evaluateContextCommand(draw, &pos, CommandTranslator::EVALUATE_ONLY));
		if (would == 0)
			return ZHBO_ISSUED_BAD_TARGET;
		if ((would & o.arg) == 0)
			return ZHBO_ISSUED_OTHER_ACTION;
		translator().evaluateContextCommand(draw, &pos, CommandTranslator::DO_COMMAND);
		return ZHBO_ISSUED_SENT;
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
		// What a click can take: selectable things (units and buildings, anyone's), not scenery. A
		// passenger is listed only when it is the player's own (`unload` names it).
		if (obj->isEffectivelyDead() || !obj->isSelectable())
			continue;
		const int32_t relation = relationOf(asker, obj);
		const Bool contained = obj->isContained();
		if (contained && relation != ZHBO_REL_OWN)
			continue;
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
		if (const ProductionUpdateInterface *pu = obj->getProductionUpdateInterface())
			o.queue = (int32_t)pu->getProductionCount();
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
		if (contained)
		{
			f |= ZHBO_CMD_CONTAINED;
			o.container = (uint32_t)obj->getContainedBy()->getID();
		}
		// The General is the player's own first person: never part of a selection.
		if (isGeneral(obj))
			f |= ZHBO_CMD_GENERAL;
		else if (relation == ZHBO_REL_OWN && !contained)
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

int32_t cbCommandButtons(int32_t playerIndex, ZhboCmdButton *out, int32_t cap)
{
	Player *player = ThePlayerList ? ThePlayerList->getNthPlayer(playerIndex) : nullptr;
	if (!player || !out || cap <= 0 || !TheGameLogic || !TheControlBar)
		return 0;
	int32_t n = 0;
	// Each of the player's objects' current command set, as the control bar shows it when the
	// object is selected; the General's are his own first person's (constraint 1's exception).
	for (Object *obj = TheGameLogic->getFirstObject(); obj && n < cap; obj = obj->getNextObject())
	{
		if (obj->getControllingPlayer() != player || obj->isEffectivelyDead() || obj->isContained() || !obj->isSelectable() || isGeneral(obj))
			continue;
		addButtons(ownerSet(obj), (uint32_t)obj->getID(), 0, obj, player, out, cap, n);
	}
	// The player's own sets: the special-power shortcuts and the Generals' promotions.
	for (int32_t set : { ZHBO_PLAYERSET_SHORTCUTS, ZHBO_PLAYERSET_RANK1, ZHBO_PLAYERSET_RANK3, ZHBO_PLAYERSET_RANK8 })
		addButtons(playerSet(player, set), 0, set, nullptr, player, out, cap, n);
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
	copyText(out, (size_t)cap, t->getDisplayName());
	return (int32_t)strlen(out);
}

int32_t cbIssueOrder(const ZhboCmdOrder *order, const uint32_t *ids, int32_t count)
{
	const char *name = order ? orderName(order->kind) : nullptr;
	Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
	if (!name || !local || !TheMessageStream || !TheInGameUI)
		return ZHBO_ISSUED_UNAVAILABLE;
	const ZhboCmdOrder &o = *order;
	std::vector<Object *> mine;
	for (int32_t i = 0; ids && i < count; ++i)
	{
		Object *obj = TheGameLogic->findObjectByID((ObjectID)ids[i]);
		if (obj && obj->getControllingPlayer() == local && !isGeneral(obj))
			mine.push_back(obj);
	}
	// Only a press of the player's own sets, and a beacon, name nobody.
	const Bool nobodyNeeded = (o.kind == ZHBO_ORDER_BUTTON && count == 0) || o.kind == ZHBO_ORDER_PLACE_BEACON;
	if (mine.empty() && !nobodyNeeded)
	{
		logf("command order: %s to nobody (none of the %d ids is the player's)", name, (int)count);
		return ZHBO_ISSUED_NOBODY;
	}
	Coord3D pos;
	pos.set(o.pos.x, o.pos.y, o.pos.z);
	int32_t result = ZHBO_ISSUED_SENT;
	switch (o.kind)
	{
	case ZHBO_ORDER_MAKE_GROUP:
	{
		if (o.arg < 0 || o.arg >= NUM_HOTKEY_SQUADS)
			return ZHBO_ISSUED_UNAVAILABLE;
		GameMessage *team = TheMessageStream->appendMessage((GameMessage::Type)(GameMessage::MSG_CREATE_TEAM0 + o.arg));
		for (Object *obj : mine)
			team->appendObjectIDArgument(obj->getID());
		break;
	}
	case ZHBO_ORDER_BUTTON:
		result = pressButton(o, mine.empty() ? nullptr : mine.front(), local);
		break;
	case ZHBO_ORDER_CONSTRUCT:
		result = construct(o, mine.front());
		break;
	case ZHBO_ORDER_CANCEL:
		result = cancel(o, mine);
		break;
	case ZHBO_ORDER_UNLOAD:
	{
		Object *rider = TheGameLogic->findObjectByID((ObjectID)o.target);
		if (!rider || rider->getContainedBy() != mine.front())
			return ZHBO_ISSUED_NOT_FOUND;
		select({ mine.front() });
		TheMessageStream->appendMessage(GameMessage::MSG_EXIT)->appendObjectIDArgument(rider->getID());
		break;
	}
	case ZHBO_ORDER_CONTEXT:
		result = rightClick(o, mine);
		break;
	case ZHBO_ORDER_PLACE_BEACON:
	{
		// As MSG_META_PLACE_BEACON allows one: under the multiplayer settings' cap.
		const ThingTemplate *beacon = TheThingFactory->findTemplate(local->getPlayerTemplate()->getBeaconTemplate());
		Int have = 0;
		local->countObjectsByThingTemplate(1, &beacon, false, &have);
		if (!TheMultiplayerSettings || have >= TheMultiplayerSettings->getMaxBeaconsPerPlayer())
			return ZHBO_ISSUED_UNAVAILABLE;
		TheMessageStream->appendMessage(GameMessage::MSG_PLACE_BEACON)->appendLocationArgument(pos);
		break;
	}
	default:
	{
		select(mine);
		switch (o.kind)
		{
		case ZHBO_ORDER_MOVE:
			TheMessageStream->appendMessage(o.queued ? GameMessage::MSG_ADD_WAYPOINT : GameMessage::MSG_DO_MOVETO)->appendLocationArgument(pos);
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
			TheMessageStream->appendMessage(GameMessage::MSG_DO_ATTACK_OBJECT)->appendObjectIDArgument((ObjectID)o.target);
			if (s_config.autopilot_command)
				autopilotCommandAttackOrdered();
			break;
		case ZHBO_ORDER_FORCE_ATTACK:
			TheMessageStream->appendMessage(GameMessage::MSG_DO_FORCE_ATTACK_OBJECT)->appendObjectIDArgument((ObjectID)o.target);
			break;
		case ZHBO_ORDER_FORCE_ATTACK_GROUND:
			TheMessageStream->appendMessage(GameMessage::MSG_DO_FORCE_ATTACK_GROUND)->appendLocationArgument(pos);
			break;
		case ZHBO_ORDER_GUARD_POSITION:
		{
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_GUARD_POSITION);
			msg->appendLocationArgument(pos);
			msg->appendIntegerArgument(o.arg);
			break;
		}
		case ZHBO_ORDER_GUARD_OBJECT:
		{
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_GUARD_OBJECT);
			msg->appendObjectIDArgument((ObjectID)o.target);
			msg->appendIntegerArgument(o.arg);
			break;
		}
		case ZHBO_ORDER_SCATTER:
			TheMessageStream->appendMessage(GameMessage::MSG_DO_SCATTER);
			break;
		case ZHBO_ORDER_FORM_UP:
			TheMessageStream->appendMessage(GameMessage::MSG_CREATE_FORMATION);
			break;
		case ZHBO_ORDER_CHEER:
			TheMessageStream->appendMessage(GameMessage::MSG_DO_CHEER);
			break;
		case ZHBO_ORDER_REMOVE_BEACON:
			TheMessageStream->appendMessage(GameMessage::MSG_REMOVE_BEACON);
			break;
		default:
			return ZHBO_ISSUED_UNAVAILABLE;
		}
		break;
	}
	}
	logf("command order: %s%s to %d object(s), target %u at (%.0f, %.0f), slot %d, tmpl %u: %d", name, o.queued ? " (queued)" : "",
		(int)mine.size(), (unsigned)o.target, pos.x, pos.y, (int)o.slot, (unsigned)o.tmpl, (int)result);
	return result;
}
