/*
** Conquer Ops - engine-side bridge: the in-game test harness (ZHBO_AUTOPILOT): scene, scripted input, builds.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
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
} // namespace

Autopilot s_autopilot = {};
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
	const Bool manual = s_config.autopilot_manual;
	if (!s_autopilot.rts && !manual && !s_autopilot.build)
		Zhbo_RequestTakeControl(general);
}

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

void ResetAutopilotTrackers()
{
	s_generalBuilds.clear();
}
