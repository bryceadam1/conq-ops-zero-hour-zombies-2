/*
** Conquer Ops - engine-side bridge: the overhead General's weapons: shot tracking, ammo overlay and world poses.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
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
	const std::string text = readTextFile(zhbo_names::RTS_FX_PATH);
	if (text.empty())
		return;
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
// What each controlled General was last shown holding (applyWorldPoses, below).
struct WorldLook
{
	int32_t weapon;
	int32_t tablet;
	int32_t sidearm;
};
std::map<uint32_t, WorldLook> s_worldLooks;
} // namespace

// The General under RTS control: Zero Hour's AI fights with the loadout's weapons (PRIMARY rifle,
// SECONDARY pistol, TERTIARY lethal), but the ammo is the General's own in zhbo-core. Each logic
// frame every shot is counted against it, the slot the player picked is reported, an empty weapon
// can't fire, and the General is moved off it to one with ammo.
Bool isGeneral(const Object *obj)
{
	return obj && zhbo_names::isGeneral(obj->getTemplate()->getName().str());
}
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

void ResetRtsWeapons()
{
	memset(s_rtsHitFx, 0, sizeof(s_rtsHitFx));
	s_rtsHitFxLoaded = FALSE;
	s_rtsShots.clear();
	s_worldLooks.clear();
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
