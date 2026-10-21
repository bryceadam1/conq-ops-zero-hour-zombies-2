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

// The General under RTS control: Zero Hour's AI fights with his class's weapons: PRIMARY and
// SECONDARY are his two guns, TERTIARY carries out the command bar's Lethal, Tactical and Melee
// orders. zhbo-core names the Zero Hour weapon each slot must hold (ZhboRtsWeapons::templates,
// converted per BO2 weapon) and they are swapped in when it changes. The ammo is the General's own
// in zhbo-core: each logic frame every shot is counted against it, the slot the player picked is
// reported, an empty weapon can't fire, and the General is moved off it to one with ammo.
Bool isGeneral(const Object *obj)
{
	return obj && zhbo_names::isGeneral(obj->getTemplate()->getName().str());
}

Int rtsOrderAction(const CommandButton *button)
{
	if (!button)
		return -1;
	const AsciiString &name = button->getName();
	if (name == "Command_ZHBO_Lethal")
		return ZHBO_ACTION_LETHAL;
	if (name == "Command_ZHBO_Tactical")
		return ZHBO_ACTION_TACTICAL;
	if (name == "Command_ZHBO_Melee")
		return ZHBO_ACTION_MELEE;
	if (name == "Command_ZHBO_Reload")
		return ZHBO_ACTION_RELOAD;
	return -1;
}

void sendRtsOrder(Int action)
{
	if (!TheInGameUI)
		return;
	const DrawableList *selected = TheInGameUI->getAllSelectedDrawables();
	for (DrawableListCIt it = selected->begin(); it != selected->end(); ++it)
	{
		const Object *obj = *it ? (*it)->getObject() : nullptr;
		if (!isGeneral(obj) || !obj->isLocallyControlled())
			continue;
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_TAKE_CONTROL);
		msg->appendObjectIDArgument(obj->getID());
		msg->appendIntegerArgument(action);
	}
}

namespace
{
// Put the Zero Hour weapon zhbo-core names into each of the General's slots (unchanged slots and
// names the archive lacks are left alone).
void armRtsWeapons(Object *obj, const ZhboRtsWeapons &w)
{
	for (Int s = 0; s < 3; ++s)
	{
		char name[ZHBO_RTS_WEAPON_NAME_CAP + 1] = {};
		memcpy(name, w.templates[s], ZHBO_RTS_WEAPON_NAME_CAP);
		if (!name[0])
			continue;
		const Weapon *have = obj->getWeaponInWeaponSlot((WeaponSlotType)s);
		if (have && have->getTemplate()->getName() == name)
			continue;
		const WeaponTemplate *tmpl = TheWeaponStore ? TheWeaponStore->findWeaponTemplate(name) : nullptr;
		if (!tmpl)
		{
			static std::set<std::string> s_missing;
			if (s_missing.insert(name).second)
				logf("General %u: no Zero Hour weapon %s (reconvert the BO2 archive)", (unsigned)obj->getID(), name);
			continue;
		}
		obj->zhboSetSlotWeapon((WeaponSlotType)s, tmpl);
		logf("General %u armed slot %d with %s", (unsigned)obj->getID(), s, name);
	}
}
} // namespace

void applyRtsOrder(Int playerIndex, Object *general, Int action)
{
	const uint32_t id = (uint32_t)general->getID();
	if (action == ZHBO_ACTION_RELOAD)
	{
		WeaponSlotType current = PRIMARY_WEAPON;
		general->getCurrentWeapon(&current);
		const Int slot = zhbo_logic_rts_reload(playerIndex, id, (int32_t)current);
		Weapon *w = slot >= 0 && slot < 2 ? general->getWeaponInWeaponSlot((WeaponSlotType)slot) : nullptr;
		if (w)
		{
			w->reloadAmmo(general);
			logf("General %u reloads slot %d", (unsigned)id, slot);
		}
		return;
	}
	if (action < ZHBO_ACTION_LETHAL || action > ZHBO_ACTION_MELEE)
		return;
	zhbo_logic_rts_offhand(playerIndex, id, action - ZHBO_ACTION_LETHAL);
	// The order's weapon goes into the third slot now, before Zero Hour's own weapon message (next
	// in the stream) aims it: its reach is the order's. Zero Hour's temporary lock for the order
	// can't override a permanent one (the gun he was told to hold, or the one he went back to), so
	// that lock is released; zhbo-core locks him back to the gun once the order is carried out.
	ZhboRtsWeapons w = {};
	zhbo_client_rts_ammo(id, &w);
	armRtsWeapons(general, w);
	general->releaseWeaponLock(LOCKED_PERMANENTLY);
	static const char *const what[] = { "lethal", "tactical", "melee" };
	logf("General %u ordered to use the %s", (unsigned)id, what[action - ZHBO_ACTION_LETHAL]);
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
		// A General spawns unarmed now, and the scripted RTS autotest (ZHBO_AUTOPILOT=rts) drives
		// the command bar directly with no class-screen UI of its own: give its General the shipped
		// Assault preset on the first frame it is seen, so the run still exercises the RTS weapon
		// path (ZHBO_LOADOUT still contributes the kit's spare rounds, which the dry runs need).
		// Test-only: the autopilot is single-player-only (see ZhboBridgeInternal.h).
		if (s_autopilot.rts && !seen.seen)
			zhbo_logic_choose_class(obj->getControllingPlayer()->getPlayerIndex(), (uint32_t)obj->getID(), 0, nullptr, 0);
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
		armRtsWeapons(obj, ammo);
		if (const Object *victim = ai ? ai->getCurrentVictim() : nullptr)
		{
			for (Int s = 0; s < 2; ++s)
				if (shots[s])
					playRtsHit(s, victim);
		}
		for (Int s = 0; s < 3; ++s)
		{
			// The log is the test's oracle: every lethal and secondary shot, every tenth primary round.
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
			// out of the new weapon's range, so order it again (he closes in with the secondary).
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
// The ammo left on each of the selected General's gun buttons and offhand orders (drawn over the
// command bar).
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
	ZhboRtsIcons icons = {};
	zhbo_client_rts_icons((uint32_t)obj->getID(), &icons);
	for (Int i = 1; i <= 14; ++i)
	{
		char name[64];
		snprintf(name, sizeof(name), "ControlBar.wnd:ButtonCommand%02d", i);
		GameWindow *win = TheWindowManager->winGetWindowFromId(nullptr, TheNameKeyGenerator->nameToKey(name));
		if (!win || win->winIsHidden())
			continue;
		const CommandButton *button = (const CommandButton *)GadgetButtonGetData(win);
		if (!button)
			continue;
		const Int order = rtsOrderAction(button);
		const Int slot = button->getCommandType() == GUI_COMMAND_SWITCH_WEAPON ? (Int)button->getWeaponSlot() : -1;
		Int icon = -1;
		if (slot == 0 || slot == 1)
			icon = slot == 0 ? ZHBO_RTS_ICON_PRIMARY : ZHBO_RTS_ICON_SECONDARY;
		else if (order == ZHBO_ACTION_LETHAL)
			icon = ZHBO_RTS_ICON_LETHAL;
		else if (order == ZHBO_ACTION_TACTICAL)
			icon = ZHBO_RTS_ICON_TACTICAL;
		else if (order == ZHBO_ACTION_MELEE)
			icon = ZHBO_RTS_ICON_KNIFE;
		if (icon < 0)
			continue;
		Int x, y, w, h;
		win->winGetScreenPosition(&x, &y);
		win->winGetSize(&w, &h);
		// His kit's BO2 icon over the plain tile (his class is the player's; no button art can show it).
		char iconName[ZHBO_RTS_WEAPON_NAME_CAP + 1] = {};
		memcpy(iconName, icons.icons[icon], ZHBO_RTS_WEAPON_NAME_CAP);
		if (iconName[0])
		{
			const float pad = (float)h * 0.08f;
			cbImage(iconName, (float)x + pad, (float)y + pad, (float)w - 2.0f * pad, (float)h * 0.62f - pad, 0xFFF2FFFFu);
		}
		if (order == ZHBO_ACTION_MELEE)
			continue; // the knife never runs dry

		char text[32];
		Bool empty;
		if (order == ZHBO_ACTION_LETHAL || order == ZHBO_ACTION_TACTICAL)
		{
			const Int count = order == ZHBO_ACTION_TACTICAL ? ammo.tactical : ammo.lethal;
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
// and hold the secondary in BO2's sidearm stance (JAMMED). Only changes are applied.

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
		// BO2's sidearm stance while the secondary is in hand (JAMMED: only missiles use it otherwise).
		if (p.sidearm && p.tablet == 0 && !obj->isEffectivelyDead())
			draw->setModelConditionState(MODELCONDITION_JAMMED);
		else
			draw->clearModelConditionState(MODELCONDITION_JAMMED);
		WorldLook look = { p.weapon, p.tablet, p.sidearm };
		s_worldLooks[p.general] = look;
	}
}
