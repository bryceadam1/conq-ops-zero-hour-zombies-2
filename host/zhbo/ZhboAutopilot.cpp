/*
** Conquer Ops - engine-side bridge: the in-game test harness (ZHBO_AUTOPILOT): scene, scripted input, builds.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
#include "GameLogic/Module/ProductionUpdate.h"
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
		// Stop him first. Take Control lands as an order on a later logic frame, and first person
		// starts from his orientation then; an AI still turning him (his locomotor's 500 degrees a
		// second is a 16.7-degree step a frame) would hand the hammer a heading this search never
		// checked, and the structure lands in the command center's exit instead of open ground.
		if (AIUpdateInterface *ai = general->getAIUpdateInterface())
			ai->aiIdle(CMD_FROM_AI);
		const ThingTemplate *what = obj->getTemplate();
		const Real ahead = zhbo_hammer_place_near(what->getTemplateGeometryInfo().getBoundingCircleRadius());
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

// ZHBO_AUTOPILOT=mp: the two-instance multiplayer test (scripts/mptest-mac.sh). Nothing is spawned
// into the logic — every machine would have to spawn it identically — so each machine plays its
// own player the way a person would, through the replicated messages only: it orders a General
// from its Command Center, walks him to a meeting point between the two bases, and takes control;
// the core's script (zhbo-core client/autopilot.rs `mp_input`) fights. A General who dies is
// ordered again from the Command Center and taken once more (his class comes back with him).
enum MpPhase
{
	MP_ORDER,		// order a General at the Command Center
	MP_BUILDING,	// wait for him to come out
	MP_WALK,		// walk him to the meeting point
	MP_FIGHT,		// first person (the core's script)
	MP_RESPAWN,		// he died: order the next one, and take him straight into first person
	MP_DONE
};

// The meeting point is this far from the midpoint between the two Command Centers, on our side:
// the two Generals end up about twice this apart, inside rifle range.
const Real MP_STANDOFF = 60.0f;
// A walk ends when he is this close to the point, or stops for this long, or takes this long.
const Real MP_ARRIVED = 30.0f;
const UnsignedInt MP_WALK_STALL = 2 * LOGICFRAMES_PER_SECOND;
const UnsignedInt MP_WALK_LIMIT = 90 * LOGICFRAMES_PER_SECOND;

void findGeneralOf(Object *obj, void *userData)
{
	Object **found = (Object **)userData;
	if (!*found && isGeneral(obj) && !obj->isEffectivelyDead())
		*found = obj;
}

Object *mpCommandCenter(Player *p)
{
	Object *cc = nullptr;
	if (p)
		p->iterateObjects(findCommandCenter, &cc);
	return cc;
}

// The other side's Command Center: the first enemy player who has one.
Object *mpEnemyCommandCenter(Player *local)
{
	for (Int i = 0; i < ThePlayerList->getPlayerCount(); ++i)
	{
		Player *p = ThePlayerList->getNthPlayer(i);
		if (p == local || !p->getPlayerTemplate() || local->getRelationship(p->getDefaultTeam()) != ENEMIES)
			continue;
		if (Object *cc = mpCommandCenter(p))
			return cc;
	}
	return nullptr;
}

void mpSelect(Object *obj)
{
	TheInGameUI->deselectAllDrawables();
	if (obj->getDrawable())
		TheInGameUI->selectDrawable(obj->getDrawable());
	GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
	group->appendBooleanArgument(TRUE);
	group->appendObjectIDArgument(obj->getID());
}

// Order a General from the Command Center as its button would: the General construct button in
// its command set (injected at startup, the side's own), a production id, MSG_QUEUE_UNIT_CREATE.
Bool mpOrderGeneral(Object *cc)
{
	const CommandSet *set = TheControlBar->findCommandSet(cc->getCommandSetString());
	ProductionUpdateInterface *pu = cc->getProductionUpdateInterface();
	for (Int i = 0; set && pu && i < MAX_COMMANDS_PER_SET; ++i)
	{
		const CommandButton *b = set->getCommandButton(i);
		const ThingTemplate *what = b && b->getCommandType() == GUI_COMMAND_UNIT_BUILD ? b->getThingTemplate() : nullptr;
		if (!what || !what->getName().startsWith("ZHBO_General_"))
			continue;
		const CanMakeType can = TheBuildAssistant->canMakeUnit(cc, what);
		if (can != CANMAKE_OK)
		{
			logf("autopilot mp: the Command Center cannot make %s yet (%d)", what->getName().str(), (int)can);
			return FALSE;
		}
		mpSelect(cc);
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_QUEUE_UNIT_CREATE);
		msg->appendIntegerArgument(what->getTemplateID());
		msg->appendIntegerArgument(pu->requestUniqueUnitID());
		logf("autopilot mp: ordered %s at the Command Center", what->getName().str());
		return TRUE;
	}
	AsciiString names;
	for (Int i = 0; set && i < MAX_COMMANDS_PER_SET; ++i)
		if (const CommandButton *b = set->getCommandButton(i))
		{
			names.concat(b->getName());
			names.concat(b->getThingTemplate() ? "(" : "");
			names.concat(b->getThingTemplate() ? b->getThingTemplate()->getName().str() : "");
			names.concat(b->getThingTemplate() ? ") " : " ");
		}
	logf("autopilot mp: the Command Center %s (set %s, production %s) has no General button: %s", cc->getTemplate()->getName().str(),
		cc->getCommandSetString().str(), pu ? "yes" : "no", names.str());
	return FALSE;
}

void mpSetPhase(Int phase, UnsignedInt frame)
{
	s_autopilot.mpPhase = phase;
	s_autopilot.phaseFrame = frame;
}

void mpAutopilotFrame(UnsignedInt frame)
{
	Player *local = ThePlayerList->getLocalPlayer();
	if (!local || s_autopilot.mpPhase == MP_DONE)
		return;
	Object *cc = mpCommandCenter(local);
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	if (general && general->isEffectivelyDead())
		general = nullptr;
	switch (s_autopilot.mpPhase)
	{
		case MP_ORDER:
		case MP_RESPAWN:
			// At most once a second until the order goes through (this runs every rendered frame;
			// the Command Center may still be busy).
			if (cc && frame >= s_autopilot.mpNextTry)
			{
				s_autopilot.mpNextTry = frame + LOGICFRAMES_PER_SECOND;
				if (mpOrderGeneral(cc))
				{
					s_autopilot.mpRespawning = s_autopilot.mpPhase == MP_RESPAWN;
					s_autopilot.general = INVALID_ID;
					mpSetPhase(MP_BUILDING, frame);
				}
			}
			break;
		case MP_BUILDING:
		{
			Object *fresh = general;
			if (!fresh)
			{
				local->iterateObjects(findGeneralOf, &fresh);
				if (!fresh)
					break;
				s_autopilot.general = fresh->getID();
				s_autopilot.mpStillSince = frame;
				logf("autopilot mp: my General is %u", (unsigned)fresh->getID());
			}
			// Give him a second to step out of the Command Center before ordering him about.
			if (frame - s_autopilot.mpStillSince < LOGICFRAMES_PER_SECOND)
				break;
			Object *enemy = mpEnemyCommandCenter(local);
			if (s_autopilot.mpRespawning || !cc || !enemy)
			{
				// The second life is taken where he stands: the fight is over.
				logLine(s_autopilot.mpRespawning ? "autopilot mp: taking control of the next General" : "autopilot mp: no enemy Command Center; taking control here");
				Zhbo_RequestTakeControl(fresh);
				mpSetPhase(MP_FIGHT, frame);
				break;
			}
			const Coord3D *mine = cc->getPosition();
			const Coord3D *theirs = enemy->getPosition();
			Real dx = mine->x - theirs->x, dy = mine->y - theirs->y;
			const Real len = sqrtf(dx * dx + dy * dy);
			dx = len > 1.0f ? dx / len : 1.0f;
			dy = len > 1.0f ? dy / len : 0.0f;
			Coord3D meet;
			meet.set((mine->x + theirs->x) * 0.5f + dx * MP_STANDOFF, (mine->y + theirs->y) * 0.5f + dy * MP_STANDOFF, 0.0f);
			meet.z = TheTerrainLogic->getGroundHeight(meet.x, meet.y);
			s_autopilot.mpMeet = meet;
			mpSelect(fresh);
			GameMessage *move = TheMessageStream->appendMessage(GameMessage::MSG_DO_MOVETO);
			move->appendLocationArgument(meet);
			logf("autopilot mp: walking to the meeting point (%.0f, %.0f), %.0f from the enemy Command Center", meet.x, meet.y, len * 0.5f - MP_STANDOFF);
			s_autopilot.mpLastPos = *fresh->getPosition();
			s_autopilot.mpStillSince = frame;
			mpSetPhase(MP_WALK, frame);
			break;
		}
		case MP_WALK:
		{
			if (!general)
			{
				mpSetPhase(MP_RESPAWN, frame);
				break;
			}
			const Coord3D *at = general->getPosition();
			const Real mx = at->x - s_autopilot.mpLastPos.x, my = at->y - s_autopilot.mpLastPos.y;
			if (mx * mx + my * my > 1.0f)
			{
				s_autopilot.mpLastPos = *at;
				s_autopilot.mpStillSince = frame;
			}
			const Real ex = at->x - s_autopilot.mpMeet.x, ey = at->y - s_autopilot.mpMeet.y;
			const Bool arrived = ex * ex + ey * ey < MP_ARRIVED * MP_ARRIVED;
			const Bool stalled = frame - s_autopilot.mpStillSince > MP_WALK_STALL && frame - s_autopilot.phaseFrame > MP_WALK_STALL;
			if (arrived || stalled || frame - s_autopilot.phaseFrame > MP_WALK_LIMIT)
			{
				logf("autopilot mp: %s at (%.0f, %.0f) after %.0f s; taking control", arrived ? "arrived" : "stopped", at->x, at->y,
					(frame - s_autopilot.phaseFrame) / (Real)LOGICFRAMES_PER_SECOND);
				Zhbo_RequestTakeControl(general);
				mpSetPhase(MP_FIGHT, frame);
			}
			break;
		}
		case MP_FIGHT:
		{
			const Bool controlled = general && zhbo_logic_is_controlled((uint32_t)general->getID());
			if (controlled)
			{
				s_autopilot.controlledFrame = frame;
				break;
			}
			// Not (yet, or any longer) in first person: give the take-control a few seconds to land.
			if (s_autopilot.controlledFrame == 0 && general && frame - s_autopilot.phaseFrame < 10 * LOGICFRAMES_PER_SECOND)
				break;
			if (!general && !s_autopilot.mpRespawning)
			{
				logLine("autopilot mp: my General died; ordering the next one");
				s_autopilot.controlledFrame = 0;
				mpSetPhase(MP_RESPAWN, frame);
				break;
			}
			logf("autopilot mp: done (%s)", general ? "back overhead" : "died again");
			mpSetPhase(MP_DONE, frame);
			break;
		}
		default:
			break;
	}
}

// Structures Generals are building: when each started, to log how long it took.
std::map<ObjectID, UnsignedInt> s_generalBuilds;
} // namespace

Autopilot s_autopilot = {};

// The player the test scene is built for. In the recorded game that is the local player; a
// replay's playback runs the same logic step with the ReplayObserver as the local player, so the
// scene goes to the first human player instead, the one the recording's local player was in a
// skirmish (the only seat a scripted run plays). Every machine and every playback then spawns the
// same objects, and the recorded take-control finds its General.
static Player *autopilotPlayer()
{
	Player *local = ThePlayerList->getLocalPlayer();
	if (local && local->isPlayerActive() && !(TheRecorder && TheRecorder->isPlaybackMode()))
		return local;
	for (Int i = 0; i < ThePlayerList->getPlayerCount(); ++i)
	{
		Player *p = ThePlayerList->getNthPlayer(i);
		if (p && p->getPlayerType() == PLAYER_HUMAN && !p->isPlayerObserver() && p->getPlayerTemplate() && p->isPlayableSide())
			return p;
	}
	return local;
}

// A replay's playback: the recording carries every message the scripted run sent.
static Bool autopilotPlayback()
{
	return TheRecorder && TheRecorder->isPlaybackMode();
}

void autopilotSpawn()
{
	Player *local = autopilotPlayer();
	// The multiplayer run spawns nothing (see mpAutopilotFrame): it starts at the Command Center.
	if (s_config.autopilot_mp)
	{
		logLine("autopilot mp: ordering a General at the Command Center");
		s_autopilot.spawnFrame = TheGameLogic->getFrame();
		return;
	}
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

	// Three enemy riflemen ahead of the General (inside rifle range, outside the base). The smoke
	// scene places its own units around the screen once it stands.
	if (enemy && !s_autopilot.build && !s_config.autopilot_smoke)
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
	if (!s_autopilot.rts && !manual && !s_autopilot.build && !autopilotPlayback())
		Zhbo_RequestTakeControl(general);
}

void autopilotSmokeSeen(const Coord3D &at, Real radius)
{
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (s_autopilot.smokeFirst == 0)
	{
		s_autopilot.smokeFirst = frame;
		s_autopilot.smokeAt = at;
		logf("autopilot: smoke went off at (%.0f, %.0f)", at.x, at.y);
	}
	s_autopilot.smokeLast = frame;
	s_autopilot.smokeRadius = radius;
}

// What `obj` is attacking among the scene's units and the General, if any.
static Object *smokeSceneVictim(Object *obj)
{
	AIUpdateInterface *ai = obj ? obj->getAI() : nullptr;
	Object *victim = ai ? ai->getCurrentVictim() : nullptr;
	if (!victim)
		return nullptr;
	const ObjectID id = victim->getID();
	return id == s_autopilot.smokeRanger || id == s_autopilot.smokeRebel || id == s_autopilot.general ? victim : nullptr;
}

// ZHBO_AUTOPILOT=smoke: once the General's screen has grown (3 s after it went off, BO2's blocker at
// its 150 inches), a Ranger and a rebel stand on either side of it inside each other's rifle range,
// the line between them through its centre: neither may attack the other (or the General) while it
// stands, and one must once it clears. A Ranger ordered across it, its straight line through the
// centre, must walk round it (a neutral one, whom nobody shoots on the way).
void smokeAutopilotFrame(UnsignedInt frame)
{
	Autopilot &a = s_autopilot;
	if (a.smokeFirst == 0)
		return;
	Object *general = TheGameLogic->findObjectByID(a.general);
	const Coord3D c = a.smokeAt;
	if (!a.smokeScene && frame - a.smokeFirst >= 3 * LOGICFRAMES_PER_SECOND && general)
	{
		a.smokeScene = TRUE;
		Player *local = autopilotPlayer();
		Player *enemy = nullptr;
		for (Int i = 0; i < ThePlayerList->getPlayerCount() && !enemy; ++i)
		{
			Player *p = ThePlayerList->getNthPlayer(i);
			if (p != local && p->getPlayerTemplate() && local->getRelationship(p->getDefaultTeam()) == ENEMIES)
				enemy = p;
		}
		// Along the General's line to the screen (u), and across it (v).
		Coord3D u;
		u.set(c.x - general->getPosition()->x, c.y - general->getPosition()->y, 0.0f);
		const Real len = sqrtf(u.x * u.x + u.y * u.y);
		if (len > 1.0f)
			u.set(u.x / len, u.y / len, 0.0f);
		else
			u.set(Cos(general->getOrientation()), Sin(general->getOrientation()), 0.0f);
		const Real yaw = atan2f(u.y, u.x);
		Coord3D p;
		p.set(c.x - u.x * 40.0f, c.y - u.y * 40.0f, 0.0f);
		Object *ranger = spawnAt("AmericaInfantryRanger", local->getDefaultTeam(), p, yaw);
		p.set(c.x + u.x * 40.0f, c.y + u.y * 40.0f, 0.0f);
		Object *rebel = enemy ? spawnAt("GLAInfantryRebel", enemy->getDefaultTeam(), p, yaw + PI) : nullptr;
		p.set(c.x + u.y * 60.0f, c.y - u.x * 60.0f, 0.0f);
		// The neutral side's: no one shoots him on his way, and he stops for no one.
		Player *neutral = ThePlayerList->getNeutralPlayer();
		Object *mover = spawnAt("AmericaInfantryRanger", neutral ? neutral->getDefaultTeam() : local->getDefaultTeam(), p, yaw);
		a.smokeRanger = ranger ? ranger->getID() : INVALID_ID;
		a.smokeRebel = rebel ? rebel->getID() : INVALID_ID;
		a.smokeMover = mover ? mover->getID() : INVALID_ID;
		a.smokeMoverClosest = 1.0e9f;
		a.smokeMoverGoal.set(c.x - u.y * 60.0f, c.y + u.x * 60.0f, 0.0f);
		a.smokeMoverGoal.z = TheTerrainLogic->getGroundHeight(a.smokeMoverGoal.x, a.smokeMoverGoal.y);
		if (mover && mover->getAI())
			mover->getAI()->aiMoveToPosition(&a.smokeMoverGoal, CMD_FROM_PLAYER);
		logf("autopilot: smoke scene: Ranger %u and rebel %u %.0f apart across the screen (radius %.1f), Ranger %u ordered across it",
			(unsigned)a.smokeRanger, (unsigned)a.smokeRebel, 80.0f, a.smokeRadius, (unsigned)a.smokeMover);
	}
	if (!a.smokeScene)
		return;
	const Bool standing = frame <= a.smokeLast + 1;
	Object *ranger = TheGameLogic->findObjectByID(a.smokeRanger);
	Object *rebel = TheGameLogic->findObjectByID(a.smokeRebel);
	Object *fighters[2] = { ranger, rebel };
	for (Object *f : fighters)
	{
		Object *victim = smokeSceneVictim(f);
		if (!victim)
			continue;
		if (standing && !a.smokeThrough)
		{
			a.smokeThrough = TRUE;
			logf("autopilot: smoke: %s %u attacks %s %u through the screen", f->getTemplate()->getName().str(), (unsigned)f->getID(),
				victim->getTemplate()->getName().str(), (unsigned)victim->getID());
		}
		if (!standing && !a.smokeCleared)
		{
			a.smokeCleared = TRUE;
			logf("autopilot: smoke cleared: %s %u attacks %s %u (%.1f s after it went off)", f->getTemplate()->getName().str(), (unsigned)f->getID(),
				victim->getTemplate()->getName().str(), (unsigned)victim->getID(), (frame - a.smokeFirst) / (Real)LOGICFRAMES_PER_SECOND);
		}
	}
	if (standing && (frame - a.smokeFirst) % (2 * LOGICFRAMES_PER_SECOND) == 0)
		logf("autopilot: smoke: standing %.0f s, Ranger %s, rebel %s", (frame - a.smokeFirst) / (Real)LOGICFRAMES_PER_SECOND,
			smokeSceneVictim(ranger) ? "attacking" : "idle", smokeSceneVictim(rebel) ? "attacking" : "idle");
	Object *mover = TheGameLogic->findObjectByID(a.smokeMover);
	if (mover && !a.smokeMoverDone)
	{
		const Coord3D *m = mover->getPosition();
		const Real dx = m->x - c.x, dy = m->y - c.y;
		a.smokeMoverClosest = std::min(a.smokeMoverClosest, sqrtf(dx * dx + dy * dy));
		const Real gx = m->x - a.smokeMoverGoal.x, gy = m->y - a.smokeMoverGoal.y;
		if (gx * gx + gy * gy < 15.0f * 15.0f || !standing)
		{
			a.smokeMoverDone = TRUE;
			logf("autopilot: smoke: the Ranger ordered across %s; he passed %.1f from its centre (radius %.1f)",
				gx * gx + gy * gy < 15.0f * 15.0f ? "arrived" : "had not arrived before it cleared", a.smokeMoverClosest, a.smokeRadius);
		}
	}
}

void rtsAutopilotWave(UnsignedInt frame)
{
	const UnsignedInt t = frame - s_autopilot.spawnFrame;
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	// Two waves for his guns and the thrown lethal; one close rebel for the knife order.
	if (!general || (t != 230 && t != 410 && t != 500))
		return;
	const Bool close = t == 500;
	Player *local = autopilotPlayer();
	for (Int p = 0; p < ThePlayerList->getPlayerCount(); ++p)
	{
		Player *enemy = ThePlayerList->getNthPlayer(p);
		if (enemy == local || !enemy->getPlayerTemplate() || local->getRelationship(enemy->getDefaultTeam()) != ENEMIES)
			continue;
		const Real yaw = general->getOrientation();
		const Coord3D *g = general->getPosition();
		const Real ahead = close ? 40.0f : 90.0f;
		for (Int k = close ? 0 : -1; k <= (close ? 0 : 1); ++k)
		{
			Coord3D e;
			e.set(g->x + Cos(yaw) * ahead - Sin(yaw) * 10.0f * k, g->y + Sin(yaw) * ahead + Cos(yaw) * 10.0f * k, 0.0f);
			spawnAt("GLAInfantryRebel", enemy->getDefaultTeam(), e, yaw + PI);
		}
		logLine(close ? "autopilot: a rebel up close" : "autopilot: three more rebels");
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
	// The command bar's orders as its buttons give them: the order's action first (what
	// Zhbo_OnFireWeaponCommand sends), then Zero Hour's own weapon message for the third slot.
	const Object *rebel = TheAI ? TheAI->findClosestEnemy(general, 250.0f, AI::CAN_SEE) : nullptr;
	if (t >= 240 && s_done < 2)
	{
		s_done = 2;
		if (rebel)
		{
			sendRtsOrder(ZHBO_ACTION_LETHAL);
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_WEAPON_AT_LOCATION);
			msg->appendIntegerArgument(TERTIARY_WEAPON);
			msg->appendLocationArgument(*rebel->getPosition());
			msg->appendIntegerArgument(1);
			msg->appendObjectIDArgument(INVALID_ID);
			logLine("autopilot: ordered the lethal at a rebel");
		}
	}
	if (t >= 330 && s_done < 3)
	{
		s_done = 3;
		sendRtsOrder(ZHBO_ACTION_RELOAD);
		logLine("autopilot: ordered a reload");
	}
	if (t >= 420 && s_done < 4)
	{
		s_done = 4;
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_SWITCH_WEAPONS);
		msg->appendIntegerArgument(SECONDARY_WEAPON);
		logLine("autopilot: picked the pistol from the command bar");
	}
	if (t >= 505 && s_done < 5)
	{
		s_done = 5;
		if (rebel)
		{
			sendRtsOrder(ZHBO_ACTION_MELEE);
			GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_DO_WEAPON_AT_OBJECT);
			msg->appendIntegerArgument(TERTIARY_WEAPON);
			msg->appendObjectIDArgument(rebel->getID());
			msg->appendIntegerArgument(1);
			logf("autopilot: ordered a knife at rebel %u", (unsigned)rebel->getID());
		}
	}
	if (t >= 690 && s_done < 6)
	{
		s_done = 6;
		logLine("autopilot: taking control");
		Zhbo_RequestTakeControl(general);
	}
}

// ZHBO_AUTOTEST_SKIRMISH: from the main menu, open Skirmish and press its Start, as a player's
// clicks would (the replay test needs a recorded game, and a loaded save records none). Once per run.
void autopilotShellFrame()
{
	static Int s_step = 0;
	static double s_at = 0.0;
	if (!s_config.autotest_skirmish || s_step >= 2 || !TheShell || !TheWindowManager)
		return;
	const double now = zhbo_platform::now();
	if (s_at == 0.0)
		s_at = now;
	if (s_step == 0 && now - s_at > 3.0 && TheShell->getScreenCount() == 1)
	{
		s_step = 1;
		s_at = now;
		TheShell->push("Menus/SkirmishGameOptionsMenu.wnd");
		logLine("autotest: opened Skirmish");
		return;
	}
	if (s_step == 1 && now - s_at > 3.0)
	{
		const NameKeyType startId = TheNameKeyGenerator->nameToKey("SkirmishGameOptionsMenu.wnd:ButtonStart");
		GameWindow *start = TheWindowManager->winGetWindowFromId(nullptr, startId);
		GameWindow *parent = start ? start->winGetParent() : nullptr;
		while (parent && parent->winGetParent())
			parent = parent->winGetParent();
		if (!start || !parent)
			return;
		s_step = 2;
		TheWindowManager->winSendSystemMsg(parent, GBM_SELECTED, (WindowMsgData)start, startId);
		logLine("autotest: pressed Start");
	}
}

void autopilotFrame()
{
	// A replay plays back what the scripted run sent; nothing is sent again.
	if (!s_autopilot.enabled || autopilotPlayback())
		return;
	const UnsignedInt frame = TheGameLogic->getFrame();
	if (!s_autopilot.spawned)
		return; // the spawn itself happens in Zhbo_LogicStep
	if (s_autopilot.showcase)
	{
		showcaseFrame(frame);
		return;
	}
	if (s_config.autopilot_mp)
	{
		mpAutopilotFrame(frame);
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
	{
		// The manual scene starts overhead with the General selected, so his command bar (Take
		// Control, hotkey T) is up: the real-input test takes control by key, as a player would.
		// (Coming back overhead alive selects him again: the core's `select_object`.)
		Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
		if (general && !s_autopilot.manualSelected && frame - s_autopilot.spawnFrame >= 15)
		{
			s_autopilot.manualSelected = TRUE;
			TheInGameUI->deselectAllDrawables();
			TheInGameUI->selectDrawable(general->getDrawable());
			GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
			group->appendBooleanArgument(TRUE);
			group->appendObjectIDArgument(general->getID());
			logLine("autopilot: selected the General");
		}
		return;
	}
	// After the General is handed back overhead, take control again (tests re-entry): once, or
	// three times for the classpick run. Its first session declines the spawn (Escape on the class
	// screen sends the unclassed General back overhead), the second picks, the third forces the
	// screen open and Escapes it; then the General is killed and a new one stands in his place,
	// and the fourth session takes him: he must come in with the chosen class and no screen
	// (BO2's `pers["class"]` survives the death).
	Object *general = TheGameLogic->findObjectByID(s_autopilot.general);
	const Int retakes = s_config.autopilot_classpick ? 3 : 1;
	if (!general || s_autopilot.retakes >= retakes)
		return;
	if (zhbo_logic_is_controlled((uint32_t)s_autopilot.general))
	{
		s_autopilot.releasedFrame = 0;
		if (s_config.autopilot_classpick && s_autopilot.retakes == 2 && !s_autopilot.respawned)
		{
			if (s_autopilot.controlledFrame == 0)
				s_autopilot.controlledFrame = frame;
			else if (frame - s_autopilot.controlledFrame > 180 && !general->isEffectivelyDead())
			{
				const Coord3D pos = *general->getPosition();
				const Real yaw = general->getOrientation();
				Team *team = general->getTeam();
				logf("autopilot: killing General %u", (unsigned)general->getID());
				general->kill();
				Object *next = spawnAt("ZHBO_General_USA", team, pos, yaw);
				if (next)
				{
					s_autopilot.general = next->getID();
					logf("autopilot: respawned General %u", (unsigned)next->getID());
				}
				s_autopilot.respawned = TRUE;
			}
		}
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
