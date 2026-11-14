/*
** Conquer Ops - engine-side bridge: the overhead General's weapons: shot tracking, ammo overlay and world poses.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
// ObjectDrawInterface: the selection-safe body hide goes through the draw modules
// (Drawable::getDrawModules()) instead of Drawable::setDrawableHidden.
#include "Common/DrawModule.h"
// The overhead General's speed (PhysicsBehavior::getVelocity) for his native body's gait.
#include "GameLogic/Module/PhysicsUpdate.h"
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
	int32_t animSet;
	// The native third person drew the weapon last frame: when it stops succeeding, the W3D
	// sub-objects must come back, so this is part of the change test.
	int32_t native;
};
std::map<uint32_t, WorldLook> s_worldLooks;
// The Generals whose W3D body is hidden for body mode (ZHBO_NATIVE_BODY=2). Hidden through
// the draw modules, not Drawable::m_hidden: that flag also deselects the General
// (Drawable::updateHiddenStatus) and makes CanSelectDrawable refuse him.
std::set<uint32_t> s_nativeBodyHidden;
// The dropped weapons whose W3D prop the native pass hides (drawDroppedWeapons).
std::set<uint32_t> s_nativeDropHidden;
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

Bool isSurvivor(const Object *obj)
{
	return obj && zhbo_names::isSurvivor(obj->getTemplate()->getName().str());
}

Bool isZombie(const Object *obj)
{
	return obj && zhbo_names::isZombie(obj->getTemplate()->getName().str());
}

Bool isZombieSpawner(const Object *obj)
{
	return obj && zhbo_names::isZombieSpawner(obj->getTemplate()->getName().str());
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
		if (!zhboCommandWindowShown(win))
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

namespace
{
// Hide or restore a drawable's model through its draw modules: the same path
// Drawable::updateHiddenStatus uses for m_hidden/m_hiddenByStealth, but without touching either.
// W3DModelDraw::setHidden hides only the render object and its effects (shadow, terrain decal,
// particles); selection state lives on the Drawable, so this leaves the General selected and
// selectable.
void setDrawModuleHidden(Drawable *draw, Bool hidden)
{
	if (!draw)
		return;
	for (DrawModule **dm = draw->getDrawModules(); *dm; ++dm)
	{
		ObjectDrawInterface *di = (*dm)->getObjectDrawInterface();
		if (di)
			di->setHidden(hidden != 0);
	}
}
} // namespace

void ResetRtsWeapons()
{
	// A reset destroys the drawables; restore any body still tracked so a surviving drawable
	// (or a load that reuses one) is not left invisible.
	for (std::set<uint32_t>::iterator it = s_nativeBodyHidden.begin(); it != s_nativeBodyHidden.end(); ++it)
	{
		Object *obj = TheGameLogic ? TheGameLogic->findObjectByID((ObjectID)*it) : nullptr;
		Drawable *draw = obj ? obj->getDrawable() : nullptr;
		if (draw)
			setDrawModuleHidden(draw, draw->isDrawableEffectivelyHidden());
	}
	s_nativeBodyHidden.clear();
	s_nativeDropHidden.clear();
	memset(s_rtsHitFx, 0, sizeof(s_rtsHitFx));
	s_rtsHitFxLoaded = FALSE;
	s_rtsShots.clear();
	s_worldLooks.clear();
}


// Third person: show the weapon each General has in hand (its W<vm><k> sub-objects on the soldier
// model) or the scorestreak tablet (TB<k>), play the tablet's raise or put-away (USER_1, USER_2),
// and raise the held weapon's animation-set condition (zhbo_names::ANIM_SET_CONDITIONS: BO2's
// pistol, launcher, knife, shield, ... stance from mp/playeranim.script). Only changes are applied.
//
// M2 (Track R7): with ZHBO_NATIVE_BODY=1 the weapon itself comes from the native BO2 path instead:
// zhbo_client_native_poses says which General holds which converted weapon, the drawable's
// tag_weapon_right transform places it in the world (zhbo_native_body, mode ZHBO_BODY_WEAPON), and
// the W3D W<vm><k> sub-objects are hidden while the native draw succeeds. Any refusal (no blob,
// no GPU pass, no tag) leaves the W3D weapon showing.

namespace
{
// A native blob's bytes from the archive, for the third person's request/answer loop.
Bool readNativeBodyFile(const char *path, std::string &out)
{
	File *f = TheFileSystem ? TheFileSystem->openFile(path) : nullptr;
	if (!f)
		return FALSE;
	const Int size = f->size();
	char *buf = f->readEntireAndClose();
	if (!buf)
		return FALSE;
	out.assign(buf, size);
	delete[] buf;
	return TRUE;
}

// Answer the core's third-person file requests and print its log lines.
void serveNativeBody()
{
	std::vector<std::string> wanted;
	char path[512];
	for (int i = 0; zhbo_native_body_wanted(i, path, sizeof(path)) >= 0; ++i)
		wanted.push_back(path);
	for (const std::string &p : wanted)
	{
		std::string bytes;
		if (readNativeBodyFile(p.c_str(), bytes))
			zhbo_native_body_provide(p.c_str(), reinterpret_cast<const uint8_t *>(bytes.data()), (int32_t)bytes.size());
		else
			zhbo_native_body_provide(p.c_str(), nullptr, -1);
	}
	char line[512];
	while (zhbo_native_body_take_log(line, sizeof(line)) >= 0)
		Zhbo_Log("%s", line);
}
} // namespace

// The scene's lights, the camera and the views as the native pass takes them (the viewmodel's
// sceneLights, same source): Zero Hour's global lights for the time of day (lightPos is the
// direction the light travels, so the light is the other way), the RTS camera's position for the
// shader's specular term, and the scene view drawn into before the composite.
//
// A General's body (`draw`) is lit as Zero Hour lights the W3D infantry it stands in for
// (RTS3DScene::renderOneObject, updateFixedLightEnvironments): every light's diffuse and ambient
// scaled by the map's InfantryLightScale for the time of day (or a script's override), each
// capped at 1, and the drawable's tint (a damage or EMP envelope, the selection flash) added to
// every light and to the ambient.
static void fillNativeScene(ZhboNativeBodyIn &in, const Drawable *draw = nullptr)
{
	if (TheGlobalData)
	{
		Real scale = 1.0f;
		if (draw && draw->isKindOf(KINDOF_INFANTRY))
			scale = TheGlobalData->m_scriptOverrideInfantryLightScale != -1.0f ? TheGlobalData->m_scriptOverrideInfantryLightScale
				: TheGlobalData->m_infantryLightScale[TheGlobalData->m_timeOfDay];
		Real tint[3] = { 0.0f, 0.0f, 0.0f };
		if (draw)
		{
			const Vector3 *colors[2] = { draw->getTintColor(), draw->getSelectionColor() };
			for (const Vector3 *c : colors)
			{
				if (!c)
					continue;
				tint[0] += c->X;
				tint[1] += c->Y;
				tint[2] += c->Z;
			}
		}
		const auto lit = [scale](Real v) { return std::min(v * scale, 1.0f); };
		const int lightCount = std::min(std::max(TheGlobalData->m_numGlobalLights, 0), 3);
		for (int l = 0; l < lightCount; ++l)
		{
			const GlobalData::TerrainLighting &tl = TheGlobalData->m_terrainObjectsLighting[TheGlobalData->m_timeOfDay][l];
			in.lights[l].dir[0] = -tl.lightPos.x;
			in.lights[l].dir[1] = -tl.lightPos.y;
			in.lights[l].dir[2] = -tl.lightPos.z;
			in.lights[l].color[0] = lit(tl.diffuse.red) + tint[0];
			in.lights[l].color[1] = lit(tl.diffuse.green) + tint[1];
			in.lights[l].color[2] = lit(tl.diffuse.blue) + tint[2];
			in.ambient[0] += lit(tl.ambient.red);
			in.ambient[1] += lit(tl.ambient.green);
			in.ambient[2] += lit(tl.ambient.blue);
		}
		for (int c = 0; c < 3; ++c)
			in.ambient[c] += tint[c];
		in.light_count = lightCount;
	}
	if (TheTacticalView)
	{
		const Coord3D camPos = TheTacticalView->get3DCameraPosition();
		in.eye[0] = camPos.x;
		in.eye[1] = camPos.y;
		in.eye[2] = camPos.z;
	}
	in.target_view = 1; // k_scene_view (the scene the drawable renders into)
	in.before_view = 9; // k_composite_view (the pass this runs before)
	if (const char *dbg = getenv("ZHBO_BO2_DEBUG"))
		in.debug = static_cast<float>(atoi(dbg));
}

namespace
{
// Whether Zero Hour would draw this object's W3D model this frame (a General's body, a dropped weapon's prop) (RTS3DScene::Visibility_Check and
// renderOneObject): not hidden (first person's own body, a transport or building holding him,
// stealth), not under the shroud, and not fogged for the local player, unless he was in the clear
// less than two seconds ago (five once dead), as renderOneObject keeps showing units leaving the
// player's sight. The native pass draws only those: the shroud must hide an enemy General the
// native pass would otherwise draw through it.
Bool nativeBodyVisible(Object *obj, Drawable *draw)
{
	if (draw->isDrawableEffectivelyHidden() || draw->getFullyObscuredByShroud())
		return FALSE;
	const Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
	if (!local)
		return TRUE;
	ObjectShroudStatus ss = obj->getShroudedStatus(local->getPlayerIndex());
	const UnsignedInt now = TheGameLogic->getFrame();
	if (ss == OBJECTSHROUD_CLEAR)
		draw->setShroudClearFrame(now);
	else if (ss >= OBJECTSHROUD_FOGGED && draw->getShroudClearFrame() != InvalidShroudClearFrame)
	{
		UnsignedInt limit = 2 * LOGICFRAMES_PER_SECOND;
		if (obj->isEffectivelyDead())
			limit += 3 * LOGICFRAMES_PER_SECOND;
		if (now < limit + draw->getShroudClearFrame())
			ss = OBJECTSHROUD_PARTIAL_CLEAR;
	}
	return ss < OBJECTSHROUD_FOGGED;
}

// How Zero Hour moves a General overhead (zhbo_client_native_poses_moving): the MOVING condition
// his locomotor raises and his horizontal speed, the physics velocity per logic frame at the
// logic rate.
ZhboGeneralMotion engineMotion(const Object *obj, const Drawable *draw)
{
	ZhboGeneralMotion m = {};
	m.general = (uint32_t)obj->getID();
	m.moving = draw->getModelConditionFlags().test(MODELCONDITION_MOVING) ? 1 : 0;
	if (const PhysicsBehavior *physics = obj->getPhysics())
	{
		const Coord3D *v = physics->getVelocity();
		m.speed = sqrtf(v->x * v->x + v->y * v->y) * LOGICFRAMES_PER_SECOND;
	}
	return m;
}
} // namespace

// The dropped weapons lying on the ground (sim/drops.rs): each through the native pass in
// ZHBO_BODY_LYING mode, keyed by its prop object, as BO2 draws the item (the combination, its camo).
// While a drop draws natively its W3D prop (the base weapon's world model) is hidden; a refusal or a
// rig still loading shows the prop again within the frame, as the body mode's hide does.
static void drawDroppedWeapons()
{
	std::set<uint32_t> drawn;
	ZhboDroppedWeapon drops[32];
	const int32_t count = zhbo_client_dropped_weapons(drops, 32);
	static Bool s_logged = FALSE;
	for (int32_t i = 0; i < count; ++i)
	{
		const ZhboDroppedWeapon &d = drops[i];
		Object *obj = TheGameLogic->findObjectByID((ObjectID)d.object);
		Drawable *draw = obj ? obj->getDrawable() : nullptr;
		if (!draw || !nativeBodyVisible(obj, draw))
			continue;
		ZhboNativeBodyIn in = {};
		in.general = d.object;
		in.mode = ZHBO_BODY_LYING;
		in.vm = d.vm;
		in.faction = 'U';
		in.rate = 1.0f;
		in.pos[0] = d.pos[0];
		in.pos[1] = d.pos[1];
		in.pos[2] = d.pos[2];
		in.yaw = d.yaw;
		// The prop is posed where the sim lays the drop (cbSetPose), so its drawable's render
		// transform is the same point between logic frames (host patch 0026): a toss in flight
		// moves at the render rate, as the Generals and the prop itself do.
		if (const Matrix3D *world = draw->getRenderTransformMatrix())
		{
			const Vector3 at = world->Get_Translation();
			in.pos[0] = at.X;
			in.pos[1] = at.Y;
			in.pos[2] = at.Z;
		}
		fillNativeScene(in);
		if (zhbo_native_body(&in) != ZHBO_NATIVE_DRAWN)
			continue;
		drawn.insert(d.object);
		if (s_nativeDropHidden.insert(d.object).second)
			setDrawModuleHidden(draw, TRUE);
		if (!s_logged)
		{
			s_logged = TRUE;
			logf("native body: dropped weapon %u (weapon %d) drawn lying, its W3D prop hidden", (unsigned)d.object, (int)d.vm);
		}
	}
	for (std::set<uint32_t>::iterator it = s_nativeDropHidden.begin(); it != s_nativeDropHidden.end();)
	{
		if (drawn.count(*it) == 0)
		{
			Object *obj = TheGameLogic->findObjectByID((ObjectID)*it);
			Drawable *draw = obj ? obj->getDrawable() : nullptr;
			if (draw)
				setDrawModuleHidden(draw, draw->isDrawableEffectivelyHidden());
			it = s_nativeDropHidden.erase(it);
		}
		else
			++it;
	}
}


void applyWorldPoses()
{
	// Every third-person index the soldier model carries meshes for: the converted viewmodels' and
	// an alt mode's own past them (zhbo_world_mesh_count(-3) is one past the highest; a core that
	// predates it answers 0, so the old 160 stands as the floor).
	enum { MIN_WORLD_INDICES = 160 };
	static std::vector<int32_t> s_meshes;
	static int32_t s_tabletMeshes = -1;
	if (s_tabletMeshes < 0)
	{
		s_tabletMeshes = zhbo_world_mesh_count(-2);
		const int32_t end = std::max<int32_t>(MIN_WORLD_INDICES, zhbo_world_mesh_count(-3));
		s_meshes.assign((size_t)end, 0);
		for (int32_t vm = 0; vm < end; ++vm)
			s_meshes[vm] = zhbo_world_mesh_count(vm);
	}
	const int32_t worldIndices = (int32_t)s_meshes.size();
	// Which Generals the native pass drew this frame: their W3D weapon stays hidden.
	std::set<uint32_t> nativeDrawn;
	// Which of them the full-body pass drew (ZHBO_NATIVE_BODY=2): their W3D body is hidden.
	std::set<uint32_t> nativeBodyDrawn;
	const int32_t nativeBodyMode = zhbo_native_body_enabled();
	if (nativeBodyMode != 0)
	{
		// The Generals to pose, then each one's engine motion (an overhead General walks or runs as
		// Zero Hour moves him), then the poses that take it.
		ZhboNativePose nativePoses[16];
		int32_t nativeCount = zhbo_client_native_poses_moving(nativePoses, 16, nullptr, 0);
		ZhboGeneralMotion motion[16];
		int32_t motionCount = 0;
		for (int32_t i = 0; i < nativeCount; ++i)
		{
			const Object *obj = TheGameLogic->findObjectByID((ObjectID)nativePoses[i].general);
			const Drawable *draw = obj ? obj->getDrawable() : nullptr;
			if (draw)
				motion[motionCount++] = engineMotion(obj, draw);
		}
		if (motionCount > 0)
			nativeCount = zhbo_client_native_poses_moving(nativePoses, 16, motion, motionCount);
		for (int32_t i = 0; i < nativeCount; ++i)
		{
			const ZhboNativePose &np = nativePoses[i];
			Object *obj = TheGameLogic->findObjectByID((ObjectID)np.general);
			Drawable *draw = obj ? obj->getDrawable() : nullptr;
			if (!draw)
				continue;
			// Whether the local player sees him, logged as it changes (the multiplayer test tells a
			// General out of sight from one the native pass failed to draw).
			const Bool visible = nativeBodyVisible(obj, draw);
			static std::map<uint32_t, Bool> s_inSight;
			std::map<uint32_t, Bool>::iterator sight = s_inSight.find(np.general);
			if (sight == s_inSight.end() || sight->second != visible)
			{
				s_inSight[np.general] = visible;
				logf("native body: General %u %s the local player's sight", (unsigned)np.general, visible ? "in" : "out of");
			}
			if (!visible)
				continue;
			// The General's world frame: feet and facing come from the drawable's transform.
			ZhboNativeBodyIn in = {};
			in.general = np.general;
			in.mode = (nativeBodyMode == ZHBO_BODY_FULL) ? ZHBO_BODY_FULL : ZHBO_BODY_WEAPON;
			in.vm = np.weapon;
			const ZhboFaction *faction = zhboFactionOfGeneralName(obj->getTemplate()->getName());
			in.faction = faction ? faction->letter : 'U';
			in.state = np.state;
			in.time = np.time;
			in.rate = np.rate;
			fillNativeScene(in, draw);
			// Where the drawable is drawn this frame (between logic frames, host patch 0026).
			if (const Matrix3D *world = draw->getRenderTransformMatrix())
			{
				const Vector3 pos = world->Get_Translation();
				in.pos[0] = pos.X;
				in.pos[1] = pos.Y;
				in.pos[2] = pos.Z;
				in.yaw = world->Get_Z_Rotation();
			}
			// The engine's own tag_weapon_right transform places the weapon: the same matrix the
			// W3D weapon hung from, so the native model sits exactly where it did. The W3D name
			// is 15 chars (W3D_NAME_LEN 16 with the NUL): the converter truncates the tag to
			// "tag_weapon_righ", so the lookup must ask for the truncated name.
			Matrix3D tagMatrix(true);
			if (draw->getCurrentWorldspaceClientBonePositions("tag_weapon_righ", tagMatrix))
			{
				in.tag_valid = 1;
				for (int r = 0; r < 3; ++r)
					for (int c = 0; c < 4; ++c)
						in.tag[r * 4 + c] = tagMatrix[r][c];
			}
			if (zhbo_native_body(&in) == ZHBO_NATIVE_DRAWN)
			{
				nativeDrawn.insert(np.general);
				if (nativeBodyMode == ZHBO_BODY_FULL)
					nativeBodyDrawn.insert(np.general);
			}
		}
		drawDroppedWeapons();
		// Answer this frame's requests after the draw calls, as the viewmodel does, so the next
		// frame's load can use them.
		serveNativeBody();
	}

	ZhboWorldPose poses[16];
	const int32_t n = zhbo_client_world_poses(poses, 16);
	for (int32_t i = 0; i < n; ++i)
	{
		const ZhboWorldPose &p = poses[i];
		Object *obj = TheGameLogic->findObjectByID((ObjectID)p.general);
		Drawable *draw = obj ? obj->getDrawable() : nullptr;
		if (!draw)
			continue;
		const Bool nativeWeapon = nativeDrawn.count(p.general) != 0 && p.tablet == 0;
		std::map<uint32_t, WorldLook>::iterator it = s_worldLooks.find(p.general);
		// A native-state change must refresh too: when the native pass stops succeeding the W3D
		// weapon has to come back.
		if (it != s_worldLooks.end() && it->second.weapon == p.weapon && it->second.tablet == p.tablet && it->second.animSet == p.anim_set
			&& it->second.native == (nativeWeapon ? 1 : 0))
			continue;
		char name[16];
		for (int32_t vm = 0; vm < worldIndices; ++vm)
		{
			for (int32_t k = 0; k < s_meshes[vm]; ++k)
			{
				zhbo_names::worldMesh(name, sizeof(name), vm, k);
				// The native pass draws the held weapon; the W3D sub-objects stay hidden for that
				// General (another weapon's stale sub-objects too).
				const Bool show = !nativeWeapon && vm == p.weapon && p.tablet == 0;
				draw->showSubObject(AsciiString(name), show);
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
		// The held weapon's animation set: its condition up (none for the rifleman's set 0, nor while
		// the tablet is up or he is dead), every other set's down. Zero Hour sets the RIDER
		// conditions only on vehicles with riders, never on infantry.
		{
			const Int setCount = (Int)(sizeof(zhbo_names::ANIM_SET_CONDITIONS) / sizeof(zhbo_names::ANIM_SET_CONDITIONS[0]));
			const Int want = (p.tablet == 0 && !obj->isEffectivelyDead()) ? p.anim_set : 0;
			ModelConditionFlags clr, set;
			for (Int k = 1; k <= setCount; ++k)
			{
				const Int bit = ModelConditionFlags::getSingleBitFromName(zhbo_names::ANIM_SET_CONDITIONS[k - 1]);
				if (bit < 0)
					continue;
				if (k == want)
					set.set(bit);
				else
					clr.set(bit);
			}
			draw->clearAndSetModelConditionFlags(clr, set);
			// The autotest's --rts check reads this: the secondary's set raised under its condition.
			if (want > 0 && want <= setCount && (it == s_worldLooks.end() || it->second.animSet != p.anim_set))
				logf("General %u third-person animation set %d (%s) for weapon %d", (unsigned)obj->getID(), (int)want,
					ModelConditionFlags::getSingleBitFromName(zhbo_names::ANIM_SET_CONDITIONS[want - 1]) >= 0 ? zhbo_names::ANIM_SET_CONDITIONS[want - 1] : "no such condition",
					(int)p.weapon);
		}
		WorldLook look = { p.weapon, p.tablet, p.anim_set, nativeWeapon ? 1 : 0 };
		s_worldLooks[p.general] = look;
	}

	// Body mode: the native pass owns the whole figure, so hide the W3D body for the Generals
	// it drew this frame and restore any body it drew last frame but not this one (a refusal,
	// a model still loading, or a switch back to weapon mode). Restoring within the same frame
	// is what keeps a failed native draw from leaving an invisible, uncommandable General.
	for (std::set<uint32_t>::iterator it = s_nativeBodyHidden.begin(); it != s_nativeBodyHidden.end();)
	{
		if (nativeBodyDrawn.count(*it) == 0)
		{
			Object *obj = TheGameLogic->findObjectByID((ObjectID)*it);
			Drawable *draw = obj ? obj->getDrawable() : nullptr;
			if (draw)
				setDrawModuleHidden(draw, draw->isDrawableEffectivelyHidden());
			it = s_nativeBodyHidden.erase(it);
		}
		else
			++it;
	}
	for (std::set<uint32_t>::iterator it = nativeBodyDrawn.begin(); it != nativeBodyDrawn.end(); ++it)
	{
		Object *obj = TheGameLogic->findObjectByID((ObjectID)*it);
		if (Drawable *draw = obj ? obj->getDrawable() : nullptr)
		{
			setDrawModuleHidden(draw, TRUE);
			// W3DModelDraw::setHidden takes the decal shadow with the model; the native body
			// stands where the model did, so his shadow stays as the drawable has it.
			for (DrawModule **dm = draw->getDrawModules(); *dm; ++dm)
				(*dm)->setShadowsEnabled(draw->getShadowsEnabled());
		}
	}
	s_nativeBodyHidden.swap(nativeBodyDrawn);
}

// Host patch 0029: a General the native pass draws in place of his hidden W3D body.
Bool Zhbo_NativeFireFx(Drawable *draw, const FXList *fxl, Real weaponSpeed, const Coord3D *victimPos, Real damageRadius)
{
	const Object *obj = draw ? draw->getObject() : nullptr;
	if (!obj || !fxl || s_nativeBodyHidden.count((uint32_t)obj->getID()) == 0)
		return FALSE;
	float m[12];
	if (!zhbo_native_body_flash((uint32_t)obj->getID(), m))
		return FALSE;
	Matrix3D mtx(true);
	mtx[0].Set(m[0], m[1], m[2], m[3]);
	mtx[1].Set(m[4], m[5], m[6], m[7]);
	mtx[2].Set(m[8], m[9], m[10], m[11]);
	Coord3D at;
	at.set(m[3], m[7], m[11]);
	FXList::doFXPos(fxl, &at, &mtx, weaponSpeed, victimPos, damageRadius);
	// The --rts check reads this: the shot leaves his drawn gun, well above his feet.
	static std::set<uint32_t> s_logged;
	if (s_logged.insert((uint32_t)obj->getID()).second)
		logf("General %u fires from his drawn muzzle (tag_flash %.1f units above his feet)", (unsigned)obj->getID(), at.z - obj->getPosition()->z);
	return TRUE;
}

Drawable *Zhbo_PickNativeDrawable(const Coord3D *start, const Coord3D *end, Int pickType, Real *nearest)
{
	if (!start || !end || !nearest || !(pickType & PICK_TYPE_SELECTABLE))
		return nullptr;
	Drawable *best = nullptr;
	for (std::set<uint32_t>::iterator it = s_nativeBodyHidden.begin(); it != s_nativeBodyHidden.end(); ++it)
	{
		Object *obj = TheGameLogic->findObjectByID((ObjectID)*it);
		Drawable *draw = obj ? obj->getDrawable() : nullptr;
		// The W3D model's collision type is what Zero Hour's own pick filters by: selectable
		// while the object is (W3DModelDraw::setSelectable), none once dead.
		if (!draw || obj->isEffectivelyDead() || !obj->isSelectable())
			continue;
		const Coord3D *feet = obj->getPosition();
		const ZhboVec3 f = { feet->x, feet->y, feet->z };
		const ZhboVec3 o = { start->x, start->y, start->z };
		const ZhboVec3 e = { end->x, end->y, end->z };
		const float hit = zhbo_client_pick_general((uint32_t)obj->getID(), &f, obj->getOrientation(), obj->getGeometryInfo().getMaxHeightAbovePosition(), &o, &e);
		if (hit >= 0.0f && hit < *nearest)
		{
			*nearest = hit;
			best = draw;
		}
	}
	return best;
}
