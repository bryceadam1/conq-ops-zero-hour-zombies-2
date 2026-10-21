/*
** Conquer Ops - engine-side bridge. Owns the shared bridge state, the faction table and the public
** entry points: Zhbo_Init fills the Rust core's host API with Zero Hour services, Zhbo_ClientFrame
** gathers each frame's input and applies its output, and the reset path clears every section's
** per-game state. The sections live in ZhboLogicApi/ZhboRtsWeapons/ZhboHud/ZhboInput/ZhboAutopilot/
** ZhboScreenLook/ZhboControlBar/ZhboBuilding (shared declarations in ZhboBridgeInternal.h).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridge.h"
#include "ZhboBridgeInternal.h"
#include "ZhboNames.h"
#include "zhbo_core.h"
#include "ZhboPlatform.h"
#if defined(GGC_BGFX_RENDERER_WGPU)
#include "zhbo_gfx.h"
#endif

#include "Common/AudioEventRTS.h"
#include "Common/ActionManager.h"
#include "Common/BuildAssistant.h"
#include "Common/File.h"
#include "Common/FileSystem.h"
#include "Common/AudioHandleSpecialValues.h"
#include "Common/GameAudio.h"
#include "Common/GameUtility.h"
#include "Common/GlobalData.h"
#include "Common/MessageStream.h"
#include "Common/Player.h"
#include "Common/PlayerList.h"
#include "Common/PlayerTemplate.h"
#include "Common/Radar.h"
#include "Common/Team.h"
#include "Common/ThingFactory.h"
#include "Common/ThingTemplate.h"
#include "GameClient/ControlBar.h"
#include "GameClient/Display.h"
#include "GameClient/DisplayString.h"
#include "GameClient/DisplayStringManager.h"
#include "GameClient/Drawable.h"
#include "GameClient/TerrainVisual.h"
#include "GameClient/FXList.h"
#include "GameClient/GadgetPushButton.h"
#include "GameClient/GameWindow.h"
#include "GameClient/GameWindowManager.h"
#include "Common/NameKeyGenerator.h"
#include "GameClient/GameClient.h"
#include "GameClient/GameFont.h"
#include "GameClient/GUICallbacks.h"
#include "GameClient/Image.h"
#include "GameClient/InGameUI.h"
#include "GameClient/KeyDefs.h"
#include "GameClient/Mouse.h"
#include "GameClient/View.h"
#include "GameLogic/AI.h"
#include "GameLogic/Damage.h"
#include "GameLogic/GameLogic.h"
#include "GameLogic/Module/AIUpdate.h"
#include "GameLogic/Module/BodyModule.h"
#include "GameLogic/Module/DozerAIUpdate.h"
#include "GameLogic/Object.h"
#include "GameLogic/PartitionManager.h"
#include "GameLogic/TerrainLogic.h"
#include "GameLogic/Weapon.h"

#include <algorithm>
#include <map>
#include <set>
#include <string>
#include <vector>

extern "C" void GGC_RequestBgfxFramebufferRebuild(); // BgfxBackend.cpp

namespace
{

Bool s_ready = FALSE;
FILE *s_log = nullptr;

} // namespace

// Client state carried between the client frame, the view update and the HUD draw; the tick the
// hitch log reads.
ZhboClientOut s_out = {};
Real s_wheelAccum = 0.0f;
static Int s_classPickLatch = 0;
// A command-bar class button pressed since the last client frame (see `chooseClass`).
Int s_classButtonPick = 0;

// The in-game Create-a-Class entry (patch 0001 routes GUI_COMMAND_ZHBO_CHOOSE_CLASS here). The
// command bar's class buttons and its Create a Class button share that command and are told apart
// by which command window the engine is dispatching: a class button hands its index to the next
// client frame, which sends that class's pick, and the Create a Class button swaps the General into
// the class set. (The class screen's own spawn-select path lives in the client core and
// never comes through here.) The lobby's button is a different path: it opens the shell screen
// (Zhbo_OpenClassMenu).
void Zhbo_ShowClassPick()
{
	Object *general = zhboSelectedGeneral();
	const Int button = zhboPressedClassButton();
	if (general && button >= 0)
	{
		zhboChooseClassButton(general, button);
		return;
	}
	if (general)
	{
		zhboOpenClassSet(general, TRUE);
		return;
	}
	// No General in hand (a scripted or legacy caller): keep the old overlay latch.
	s_classPickLatch = 1;
	logf("create-a-class: pick screen forced open");
}
double s_lastTick = 0.0;

void logLine(const char *msg)
{
	DEBUG_LOG(("[zhbo] %s", msg));
	if (s_log)
	{
		fprintf(s_log, "[%u] %s\n", TheGameLogic ? TheGameLogic->getFrame() : 0u, msg);
		fflush(s_log);
	}
}

void logf(const char *fmt, ...)
{
	char buf[512];
	va_list args;
	va_start(args, fmt);
	vsnprintf(buf, sizeof(buf), fmt, args);
	va_end(args);
	logLine(buf);
}

std::string readTextFile(const char *path)
{
	File *f = TheFileSystem ? TheFileSystem->openFile(path, File::READ | File::BINARY) : nullptr;
	if (!f)
		return {};
	const Int size = f->size();
	char *buf = f->readEntireAndClose();
	if (!buf)
		return {};
	std::string text(buf, size);
	delete[] buf;
	return text;
}

// The player's create-a-class profile: a text file in the game's user-data directory, next to the
// mod's log. Not in a BIG file (it is the player's own, written at runtime). The path is built the
// same way the log's is, so both land in the same place.
static std::string profilePath()
{
	std::string path = TheGlobalData ? TheGlobalData->getPath_UserData().str() : "";
	path += "ConquerOps.profile";
	return path;
}

// The profile file's text (empty when there is none yet).
static std::string readProfileFile()
{
	FILE *f = fopen(profilePath().c_str(), "rb");
	if (!f)
		return {};
	std::string text;
	char buf[512];
	size_t n;
	while ((n = fread(buf, 1, sizeof(buf), f)) > 0)
		text.append(buf, n);
	fclose(f);
	return text;
}

// Write the profile file's text, replacing whatever was there (the core returns the whole file on a
// save, so a single write is enough and a torn append cannot corrupt the list). Shared with the
// shell menu (ZhboMenu.cpp): plain file I/O, it works outside a match.
void writeProfileFile(const char *text)
{
	FILE *f = fopen(profilePath().c_str(), "wb");
	if (!f)
		return;
	fwrite(text, 1, strlen(text), f);
	fclose(f);
}

ZhboConfig s_config = {};

void readConfig()
{
	s_config = {};
	const char *autopilot = getenv("ZHBO_AUTOPILOT");
	s_config.autopilot_enabled = autopilot != nullptr && autopilot[0] != 0;
	s_config.autopilot_manual = s_config.autopilot_enabled && strcmp(autopilot, "manual") == 0;
	s_config.autopilot_equipment = s_config.autopilot_enabled && strcmp(autopilot, "equipment") == 0;
	s_config.autopilot_showcase = s_config.autopilot_enabled && strcmp(autopilot, "showcase") == 0;
	s_config.autopilot_rts = s_config.autopilot_enabled && strcmp(autopilot, "rts") == 0;
	s_config.autopilot_build = s_config.autopilot_enabled && strcmp(autopilot, "build") == 0;
	s_config.autopilot_classpick = s_config.autopilot_enabled && strcmp(autopilot, "classpick") == 0;
	s_config.force_radar = getenv("ZHBO_FORCE_RADAR") != nullptr;
	s_config.showcase_object = getenv("ZHBO_SHOWCASE_OBJECT");
	if (const char *v = getenv("ZHBO_SHOWCASE_ZOOM"))
		s_config.showcase_zoom = (Real)atof(v);
	if (const char *v = getenv("ZHBO_SHOWCASE_PITCH"))
	{
		s_config.has_showcase_pitch = TRUE;
		s_config.showcase_pitch = (Real)atof(v);
	}
	if (const char *v = getenv("ZHBO_SHOWCASE_ANGLE"))
	{
		s_config.has_showcase_angle = TRUE;
		s_config.showcase_angle = (Real)atof(v);
	}
	if (const char *v = getenv("ZHBO_CLOSEUP"))
	{
		s_config.closeup = TRUE;
		s_config.closeup_zoom = (Real)atof(v);
	}
}

// The three faction mappings (the player template's base side, the viewmodel arms letter, the
// tablet/hammer model code and the General object) were three scattered conditionals; now one
// table (the names come from the naming registry).
static const ZhboFaction FACTIONS[] = {
	{ "America", 'U', "USA", "ZHBO_General_USA" },
	{ "China", 'C', "CHN", "ZHBO_General_China" },
	{ "GLA", 'G', "GLA", "ZHBO_General_GLA" },
};

const ZhboFaction *zhboFactionOfSide(const AsciiString &side)
{
	for (const ZhboFaction &f : FACTIONS)
		if (side == f.side)
			return &f;
	return nullptr;
}

const ZhboFaction *zhboFactionOfGeneralName(const AsciiString &name)
{
	for (const ZhboFaction &f : FACTIONS)
		if (name == f.general)
			return &f;
	return nullptr;
}

void releaseClientState()
{
	setSkyForced(FALSE);
	setBo2Look(FALSE);
	setMouseCapture(FALSE);
	setHiddenObject(INVALID_ID);
	setRtsUiHidden(FALSE);
	memset(&s_out, 0, sizeof(s_out));
}

// ------------------------------------------------------------------------------------------------
// Public entry points
// ------------------------------------------------------------------------------------------------
void Zhbo_Init()
{
	if (s_ready)
		return;

	AsciiString logPath = TheGlobalData->getPath_UserData();
	logPath.concat("ConquerOps.log");
	s_log = fopen(logPath.str(), "w");

	if (zhbo_abi_version() != ZHBO_ABI_VERSION)
	{
		logf("zhbo-core ABI %u does not match glue ABI %u; mod disabled", zhbo_abi_version(), ZHBO_ABI_VERSION);
		return;
	}
	// A same-size but shifted API table would pass the version check: the counts catch it.
	if (const uint64_t fields = zhbo_abi_fields();
		(fields & 0xFFFFFFFFu) != ZHBO_LOGIC_API_FIELDS || (fields >> 32) != ZHBO_CLIENT_API_FIELDS)
	{
		logf("zhbo-core API tables have %u logic / %u client fields, glue expects %u / %u; mod disabled",
			(unsigned)(fields & 0xFFFFFFFFu), (unsigned)(fields >> 32), ZHBO_LOGIC_API_FIELDS, ZHBO_CLIENT_API_FIELDS);
		return;
	}

	// Conquer Ops needs both games installed; refuse to run otherwise.
	char problems[1024];
	if (!zhbo_check_requirements(problems, sizeof(problems)))
	{
		logf("requirements not met: %s", problems);
		char text[1200];
		snprintf(text, sizeof(text), "Conquer Ops requires both Command & Conquer Generals: Zero Hour and "
			"Call of Duty: Black Ops II to be installed.\n\n%s", problems);
		zhbo_platform::errorBox("Conquer Ops", text);
		zhbo_platform::exitProcess(1);
	}

	// Designated initializers: a field inserted mid-table cannot silently shift the rest, and the
	// compiler names any field that is missing or renamed.
	static const ZhboLogicApi logicApi = {
		.log = cbLog,
		.ground_height = cbGroundHeight,
		.is_walkable = cbIsWalkable,
		.extent = cbExtent,
		.object = cbObject,
		.query = cbQuery,
		.set_pose = cbSetPose,
		.set_controlled = cbSetControlled,
		.fire_fx = cbFireFx,
		.damage = cbDamage,
		.heal = cbHeal,
		.spawn = cbSpawn,
		.destroy = cbDestroy,
		.disable = cbDisable,
		.fx = cbFx,
		.capture = cbCapture,
		.smoke = cbSmoke,
		.build_options = cbBuildOptions,
		.can_place = cbCanPlace,
		.place = cbPlace,
		.work_site = cbWorkSite,
		.work = cbWork,
		.choose_class = cbChooseClass,
	};
	static const ZhboClientApi clientApi = {
		.screen_size = cbScreenSize,
		.fill_rect = cbFillRect,
		.line = cbLine,
		.text = cbText,
		.image = cbImage,
		.query = cbClientQuery,
		.cash = cbCash,
		.sound = cbSound,
		.clip = cbClip,
		.radar = cbRadar,
		.build_options = cbBuildOptions,
		.can_place = cbClientCanPlace,
		.ghost = cbGhost,
	};
	zhbo_init(&logicApi, &clientApi);

	// The BO2 weapon table, converted from the player's install into ConquerOps_BO2.big; without it
	// the core keeps its built-in stand-ins.
	const std::string table = readTextFile(zhbo_names::WEAPON_TABLE_PATH);
	if (!table.empty())
	{
		logf("weapons: %d BO2 weapons installed", zhbo_install_weapons(table.c_str()));
	}
	else
	{
		logLine("weapons: no converted BO2 weapon table; using the built-in one");
	}
	zhbo_load_strings(readTextFile(zhbo_names::STRINGS_PATH).c_str());
	zhbo_load_tuning(readTextFile(zhbo_names::TUNING_PATH).c_str());
	zhbo_load_classes(readTextFile(zhbo_names::CLASSES_PATH).c_str());
	// The player's create-a-class profile: a small text file next to the mod's log, in the game's
	// user-data directory (see zhbo_core::profile). Its custom classes join the pick screen's list.
	zhbo_load_profile(readProfileFile().c_str());

	zhboInjectConstructButtons();

	readConfig();
	s_autopilot.enabled = s_config.autopilot_enabled;
	s_autopilot.showcase = s_config.autopilot_showcase;
	s_autopilot.rts = s_config.autopilot_rts;
	s_autopilot.build = s_config.autopilot_build;

	// Render the scene in HDR (see setBo2Look); the format change needs a framebuffer rebuild.
	if (TheWritableGlobalData && !TheWritableGlobalData->m_bgfxHdr)
	{
		TheWritableGlobalData->m_bgfxHdr = TRUE;
		GGC_RequestBgfxFramebufferRebuild();
	}

	s_lastTick = zhbo_platform::now();
	s_ready = TRUE;
}

void Zhbo_Reset()
{
	if (!s_ready)
		return;
	// The shell create-a-class screen never survives a game transition (a new game, a loaded game
	// or a return to the shell): its window goes away with the shell.
	Zhbo_CloseClassMenu();
	releaseClientState();
	const Bool autopilot = s_autopilot.enabled;
	const Bool showcase = s_autopilot.showcase;
	const Bool rts = s_autopilot.rts;
	const Bool build = s_autopilot.build;
	memset(&s_autopilot, 0, sizeof(s_autopilot));
	s_autopilot.build = build;
	s_autopilot.enabled = autopilot;
	s_autopilot.showcase = showcase;
	s_autopilot.rts = rts;
	// Every per-game static: engine state that a new game rebuilds (zhbo-core keeps its own
	// surviving state behind zhbo_reset, called below). Each section owns its resets.
	ResetFxWeapons();
	ResetRtsWeapons();
	ResetHud();
	ResetBuilding();
	ResetAutopilotTrackers();
	ResetControlBarTrackers();
	s_wheelAccum = 0.0f;
	// A bar click from the leaving game must not arm a one-shot in the new one.
	s_classPickLatch = 0;
	s_classButtonPick = 0;
	s_lastTick = zhbo_platform::now(); // else the first frame after the reset logs a bogus hitch
	ZhboViewmodel_Reset();
	zhbo_reset();
	logLine("reset");
}

void Zhbo_Preload()
{
	if (!s_ready)
		return;
	const double started = zhbo_platform::now();
	// The mod's objects (the Generals, their grenades, C4 and other equipment): what the engine's
	// own preload does for KINDOF_PRELOAD things, which retail only runs with -preload.
	int objects = 0;
	for (const ThingTemplate *t = TheThingFactory->firstTemplate(); t; t = t->friend_getNextTemplate())
	{
		if (strncmp(t->getName().str(), "ZHBO_", 5) != 0)
			continue;
		if (Drawable *draw = TheThingFactory->newDrawable(t))
		{
			draw->preloadAssets(TheGlobalData->m_timeOfDay);
			TheGameClient->destroyDrawable(draw);
			++objects;
		}
	}
	logf("preloaded %d objects in %.0f ms", objects, (zhbo_platform::now() - started) * 1000.0);

	// The local General's first-person models: its side's tablet and the kit's weapons.
	const Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
	const PlayerTemplate *pt = local ? local->getPlayerTemplate() : nullptr;
	if (!pt)
		return;
	const ZhboFaction *faction = zhboFactionOfSide(pt->getBaseSide());
	if (!faction)
		return;
	const char factionLetter = faction->letter;
	int weapons[32];
	const int count = std::min<int>(zhbo_loadout_viewmodels(weapons, 32), 32);
	ZhboViewmodel_Preload(factionLetter, weapons, count);
}

void Zhbo_OnTakeControlMessage(Int playerIndex, const GameMessage *msg)
{
	if (!s_ready || msg->getArgumentCount() < 1)
		return;
	const ObjectID id = msg->getArgument(0)->objectID;
	Object *obj = TheGameLogic->findObjectByID(id);
	// The same prefix test as everywhere else (isGeneral): "ZHBO_General_", not a shorter one.
	if (!obj || !isGeneral(obj))
		return;
	// The same message, with an action, is an order for him under RTS control (ZhboRtsWeapons.cpp).
	const Int action = msg->getArgumentCount() > 1 ? msg->getArgument(1)->integer : ZHBO_ACTION_TAKE_CONTROL;
	if (action != ZHBO_ACTION_TAKE_CONTROL)
	{
		applyRtsOrder(playerIndex, obj, action);
		return;
	}
	zhbo_logic_take_control(playerIndex, (uint32_t)id);
}

void Zhbo_OnInputMessage(Int playerIndex, const GameMessage *msg)
{
	if (!s_ready || msg->getArgumentCount() < 6)
		return;
	ZhboFpsInput input;
	input.held = (uint32_t)msg->getArgument(1)->integer;
	input.pressed = (uint32_t)msg->getArgument(2)->integer;
	input.move_packed = msg->getArgument(3)->integer;
	input.yaw = msg->getArgument(4)->real;
	input.pitch = msg->getArgument(5)->real;
	zhbo_logic_input(playerIndex, (uint32_t)msg->getArgument(0)->objectID, &input);
}

void Zhbo_OnChooseClassMessage(Int playerIndex, const GameMessage *msg)
{
	if (!s_ready || msg->getArgumentCount() < 2)
		return;
	const Int index = msg->getArgument(1)->integer;
	// A custom class appends its wire words after the index (zhbo_core::classes'
	// CustomClass::to_wire_words): [len, word...]. Anything shorter is a plain list pick.
	if (msg->getArgumentCount() < 3)
	{
		zhbo_logic_choose_class(playerIndex, (uint32_t)msg->getArgument(0)->objectID, index, nullptr, 0);
		return;
	}
	const Int len = msg->getArgument(2)->integer;
	if (len < 0 || len > 64 || msg->getArgumentCount() < 3 + len)
		return;
	uint32_t payload[64];
	for (Int i = 0; i < len; ++i)
		payload[i] = (uint32_t)msg->getArgument(3 + i)->integer;
	zhbo_logic_choose_class(playerIndex, (uint32_t)msg->getArgument(0)->objectID, index, payload, len);
}

void Zhbo_LogicStep(UnsignedInt frame)
{
	if (!s_ready)
		return;
	// Objects must be created inside GameLogic::update: W3DModelDraw only resolves weapon barrel
	// bones while the logic is updating, and a drawable created outside it sizes its recoil state
	// to zero and later crashes in handleWeaponFireFX when it fires.
	if (s_autopilot.enabled && !s_autopilot.spawned && frame >= 60)
	{
		s_autopilot.spawned = TRUE;
		autopilotSpawn();
	}
	if (s_autopilot.rts && s_autopilot.spawned)
		rtsAutopilotWave(frame);
	rtsWeapons(frame);
	trackGeneralBuilds(frame);
	trackIdleGenerals();
	zhbo_logic_step(frame);
}

Bool Zhbo_IsControlled(ObjectID id)
{
	return s_ready && zhbo_logic_is_controlled((uint32_t)id) != 0;
}

// Gather this frame's input for zhbo_client_frame: timing, keys, mouse, camera.
static void gatherClientInput(ZhboClientIn &in, Bool focused, Bool menuUp)
{
	memset(&in, 0, sizeof(in));
	const double now = zhbo_platform::now();
	const float dt = (float)(now - s_lastTick);
	s_lastTick = now;
	if (dt > 0.06f && dt < 5.0f)
		logf("hitch: %.0f ms frame", dt * 1000.0f);

	in.dt = dt;
	in.local_player = rts::getObservedOrLocalPlayer()->getPlayerIndex();
	in.logic_frame = TheGameLogic->getFrame();
	if (focused && !menuUp)
	{
		zhbo_platform::readKeys(in.keys.bits);
	}

	// Test runs: log focus changes and the held keys, so an input test can tell lost focus from
	// lost keys.
	if (s_autopilot.enabled)
	{
		static Bool s_wasFocused = FALSE;
		static uint32_t s_lastKeyHash = 0;
		if (focused != s_wasFocused)
		{
			logf("window focus %s", focused ? "gained" : "lost");
			s_wasFocused = focused;
		}
		static const char *const keyNames[] = {
			"A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
			"R", "S", "T", "U", "V", "W", "X", "Y", "Z", "1", "2", "3", "4", "5", "6", "ESC",
			"SPACE", "TAB", "SHIFT", "CTRL", "LMB", "RMB", "UP", "DOWN", "LEFT", "RIGHT", "ENTER",
			"BACKSPACE", "7", "8", "9", "0",
		};
		static_assert(sizeof(keyNames) / sizeof(keyNames[0]) == (int)zhbo_platform::keys::Key::COUNT, "key vocabulary must match");
		char held[96];
		Int n = 0;
		uint32_t hash = 0;
		for (Int k = 0; k < (Int)sizeof(zhbo_platform::keys::Key); ++k)
		{
			const uint64_t bit = 1ull << (k % 64);
			if (in.keys.bits[k / 64] & bit)
			{
				for (const char *c = keyNames[k]; *c && n < (Int)sizeof(held) - 1; ++c)
					held[n++] = *c;
				if (n < (Int)sizeof(held) - 1)
					held[n++] = ' ';
				hash = hash * 31 + (uint32_t)k + 1;
			}
		}
		held[n] = 0;
		if (hash != s_lastKeyHash)
		{
			logf("keys held: [%s]", held);
			s_lastKeyHash = hash;
		}
	}
	if (zhboMouseCaptured() && focused)
	{
		zhbo_platform::mouseDelta(in.mouse_dx, in.mouse_dy);
	}
	in.wheel = s_wheelAccum;
	s_wheelAccum = 0.0f;
	// The class screens' pointer: the engine's own cursor (TheMouse keeps it in step with the
	// system cursor, and it is free while a screen is up) and its held buttons. Bit 0 left, 1 right,
	// 2 middle (matches zhbo_core::input::mouse_buttons).
	if (TheMouse)
	{
		const MouseIO *m = TheMouse->getMouseStatus();
		in.mouse_x = (float)m->pos.x;
		in.mouse_y = (float)m->pos.y;
		if (m->leftState == MBS_Down || m->leftState == MBS_DoubleClick)
			in.mouse_buttons |= 1u;
		if (m->rightState == MBS_Down || m->rightState == MBS_DoubleClick)
			in.mouse_buttons |= 2u;
		if (m->middleState == MBS_Down || m->middleState == MBS_DoubleClick)
			in.mouse_buttons |= 4u;
	}
	in.class_pick_force = s_classPickLatch;
	s_classPickLatch = 0;
	// The class button the bar dispatched since the last frame (one-shot: the core turns it into
	// the same choose_class pick the class screen sends).
	in.class_button = s_classButtonPick;
	s_classButtonPick = 0;
	in.input_allowed = (focused && !menuUp) ? 1 : 0;
	in.screen_w = (float)TheDisplay->getWidth();
	in.screen_h = (float)TheDisplay->getHeight();
	const Coord3D camPos = TheTacticalView->get3DCameraPosition();
	const Coord3D camDir = TheTacticalView->get3DCameraDirection();
	in.rts_cam_pos.x = camPos.x; in.rts_cam_pos.y = camPos.y; in.rts_cam_pos.z = camPos.z;
	in.rts_cam_dir.x = camDir.x; in.rts_cam_dir.y = camDir.y; in.rts_cam_dir.z = camDir.z;
	in.rts_hfov = TheTacticalView->getFieldOfView();
	const Coord3D &pivot = TheTacticalView->getPosition();
	in.rts_pivot.x = pivot.x; in.rts_pivot.y = pivot.y; in.rts_pivot.z = pivot.z;
}

// Apply what the core asked for this frame: the input message, mouse capture, UI and sky state,
// the hidden body, and the RTS camera's look-at.
static void applyClientOutput(Bool focused, Bool menuUp)
{
	if (s_out.send_input)
	{
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_FPS_INPUT);
		msg->appendObjectIDArgument((ObjectID)s_out.input_general);
		msg->appendIntegerArgument((Int)s_out.input.held);
		msg->appendIntegerArgument((Int)s_out.input.pressed);
		msg->appendIntegerArgument(s_out.input.move_packed);
		msg->appendRealArgument(s_out.input.yaw);
		msg->appendRealArgument(s_out.input.pitch);
	}

	// One-shot: the pick menu's class choice goes to the General (and to every client, since the
	// message is part of the lockstep stream). A custom class rides along as its wire words
	// (zhbo_core::classes' CustomClass::to_wire_words), appended after the index. The core's index
	// is 1-based so 0 can mean "no pick"; the message carries the list's own 0-based index.
	if (s_out.choose_class != 0)
	{
		GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_CHOOSE_CLASS);
		msg->appendObjectIDArgument((ObjectID)s_out.choose_class_general);
		const Int index = s_out.choose_class > 0 ? s_out.choose_class - 1 : s_out.choose_class;
		msg->appendIntegerArgument(index);
		if (s_out.choose_class_payload_len > 0)
		{
			msg->appendIntegerArgument((Int)s_out.choose_class_payload_len);
			for (Int i = 0; i < s_out.choose_class_payload_len; ++i)
				msg->appendIntegerArgument((Int)s_out.choose_class_payload[i]);
		}
		// The other half of the manual check: the bar's dispatch log names the button, this one
		// proves the pick really went out (the sim's "assumed the ... class" line proves it landed).
		if (s_out.choose_class == ZHBO_CHOOSE_CLASS_CUSTOM)
			logf("choose class: General %u gets a custom class (%d words)", (unsigned)s_out.choose_class_general, (int)s_out.choose_class_payload_len);
		else
			logf("choose class: General %u gets preset %d", (unsigned)s_out.choose_class_general, index);
		s_out.choose_class = 0;
		s_out.choose_class_payload_len = 0;
	}

	// One-shot: the profile file changed (the editor's Save, or a class-screen delete/copy). The
	// core already applied it; write the whole file, the same way the old save did. Picking a class
	// alone never sets this, so re-picking a saved class no longer rewrites the profile.
	if (s_out.save_profile != 0)
	{
		// A zero-word payload is a real write too: deleting the last class leaves an empty profile.
		if (const char *profile = zhbo_profile_write(s_out.save_profile_payload, s_out.save_profile_payload_len))
		{
			writeProfileFile(profile);
			if (s_out.save_profile == 1)
				logf("create-a-class: saved a custom class to the profile (%d words)", (int)s_out.save_profile_payload_len);
			else
				logf("create-a-class: profile updated (%d words)", (int)s_out.save_profile_payload_len);
		}
		s_out.save_profile = 0;
		s_out.save_profile_payload_len = 0;
	}

	// Never trap the cursor when the game is not the foreground window (Alt-Tab, background tests).
	setMouseCapture(s_out.capture_mouse != 0 && focused && !menuUp);
	reclaimMouse();
	setRtsUiHidden(s_out.hide_rts_ui != 0);
	setSkyForced(s_out.camera_override != 0);
	setBo2Look(s_out.camera_override != 0);
	setHiddenObject(s_out.hide_object ? (ObjectID)s_out.hide_object : INVALID_ID);

	if (s_out.rts_look_at_valid)
	{
		Coord3D target;
		target.set(s_out.rts_look_at.x, s_out.rts_look_at.y, s_out.rts_look_at.z);
		TheTacticalView->lookAt(&target);
		s_out.rts_look_at_valid = 0;
	}
}

void Zhbo_ClientFrame()
{
	if (!s_ready)
		return;
	const Bool inGame = TheGameLogic->isInGame() && !TheGameLogic->isInShellGame();
	if (!inGame)
	{
		if (s_out.camera_override || zhboMouseCaptured() || zhboRtsUiHidden())
			releaseClientState();
		return;
	}

	autopilotFrame();
	// A General with no class shows the class set instead of his own commands; once he has one (or
	// Create a Class opened it for a re-pick) the bar tracks that.
	zhboUpdateClassBar();

	const Bool focused = zhbo_platform::windowFocused();
	const Bool menuUp = TheInGameUI->isQuitMenuVisible() || TheGameLogic->isGamePaused();

	ZhboClientIn in;
	gatherClientInput(in, focused, menuUp);

	zhbo_client_frame(&in, &s_out);
	applyWorldPoses();
	applyClientOutput(focused, menuUp);
}


Bool Zhbo_GetCameraOverride(Coord3D *eye, Coord3D *dir, Real *horizontalFov)
{
	if (!s_ready || !s_out.camera_override)
		return FALSE;
	eye->set(s_out.eye.x, s_out.eye.y, s_out.eye.z);
	dir->set(s_out.dir.x, s_out.dir.y, s_out.dir.z);
	*horizontalFov = s_out.hfov;
	return TRUE;
}

Bool Zhbo_GetViewmodel(Int *clip, Int *weapon, Bool *fired, Real *time, Real *ads, AsciiString *model, Coord3D *offset, Coord3D *angles)
{
	if (!s_ready || !s_out.camera_override || s_out.viewmodel < 0 || !s_out.hide_object)
		return FALSE;
	const Object *general = TheGameLogic->findObjectByID((ObjectID)s_out.hide_object);
	if (!general)
		return FALSE;
	// One set of viewmodels per faction (zhbo_assets::generals::FACTIONS).
	const AsciiString &name = general->getTemplate()->getName();
	const ZhboFaction *faction = zhboFactionOfGeneralName(name);
	// The tablet clips play on the arms-and-tablet model (generals::tablet_model); everything else on
	// the converted weapon's viewmodel (generals::vm_model).
	// (An alt mode's clips come after the tablet's numbers, on the weapon's own viewmodel.)
	// The hammer's likewise on the arms-and-hammer model (generals::hammer_model).
	const Bool tablet = s_out.viewmodel >= ZHBO_VM_TABLET_PULLOUT && s_out.viewmodel <= ZHBO_VM_TABLET_PUTAWAY;
	const Bool hammer = s_out.viewmodel >= ZHBO_VM_HAMMER_PULLOUT && s_out.viewmodel <= ZHBO_VM_HAMMER_PUTAWAY;
	AsciiString vm;
	if (!faction)
		return FALSE;
	if (tablet || hammer)
		vm.format("ZHBO_VM%s_%s", faction->code, tablet ? "TAB" : "HAM");
	else if (s_out.viewmodel_weapon >= 0)
		vm.format("ZV%02d%c_SKN", s_out.viewmodel_weapon, faction->letter);
	else
		return FALSE;
	*clip = s_out.viewmodel;
	*weapon = tablet || hammer ? ZHBO_VM_NONE : s_out.viewmodel_weapon;
	*fired = s_out.viewmodel_fired != 0;
	*time = s_out.viewmodel_time;
	*ads = s_out.viewmodel_ads;
	offset->set(s_out.viewmodel_offset.x, s_out.viewmodel_offset.y, s_out.viewmodel_offset.z);
	angles->set(s_out.viewmodel_angles.x, s_out.viewmodel_angles.y, s_out.viewmodel_angles.z);
	model->set(vm);
	return TRUE;
}

void Zhbo_Log(const char *fmt, ...)
{
	char buf[512];
	va_list args;
	va_start(args, fmt);
	vsnprintf(buf, sizeof(buf), fmt, args);
	va_end(args);
	logLine(buf);
}

void Zhbo_DrawHud()
{
	if (!s_ready || !TheGameLogic->isInGame() || TheGameLogic->isInShellGame())
		return;
	advanceDrawFrame();
	zhbo_client_draw_hud(rts::getObservedOrLocalPlayer()->getPlayerIndex());
	if (!s_out.camera_override)
	{
		drawRtsAmmo();
		drawClassButtons();
	}
	evictTexts();
}

void Zhbo_DrawSky()
{
#if defined(GGC_BGFX_RENDERER_WGPU)
	// The bgfx backend sends 2D draws to its UI view, which runs after the 3D scene, so a 2D
	// gradient would cover the world. Paint the sky as the scene view's clear colour instead
	// (view 1 is BgfxBackend's kBgfxEngineView; alpha stays 1 for its water blending), and put
	// the backend's black back for the RTS camera.
	static bool s_skyClear = false;
	const bool wantSky = s_ready && s_out.camera_override;
	if (wantSky != s_skyClear)
	{
		zhgfx_set_view_clear(1, 0x0001 | 0x0002 | 0x0004, wantSky ? 0x7896B4FFu : 0x000000FFu, 1.0f, 0);
		s_skyClear = wantSky;
	}
	return;
#endif
	if (!s_ready || !s_out.camera_override)
		return;
	const Real w = (Real)TheDisplay->getWidth();
	const Real h = (Real)TheDisplay->getHeight();
	// Where the horizon lands on screen for the current pitch.
	const Real dz = s_out.dir.z < -0.99f ? -0.99f : (s_out.dir.z > 0.99f ? 0.99f : s_out.dir.z);
	const Real tanPitch = dz / sqrtf(1.0f - dz * dz);
	const Real tanHalfV = tanf(s_out.hfov * 0.5f) * h / w;
	Real horizon = h * 0.5f + tanPitch / tanHalfV * h * 0.5f;
	if (horizon < 0.0f) horizon = 0.0f;
	if (horizon > h) horizon = h;

	// Zenith blue to a pale haze at the horizon, then haze below it (terrain covers that part).
	const Int top[3] = { 46, 86, 140 }, low[3] = { 186, 204, 218 };
	const Int bands = 32;
	for (Int i = 0; i < bands; ++i)
	{
		const Real t = (Real)i / (Real)(bands - 1);
		const Int r = top[0] + (Int)((low[0] - top[0]) * t);
		const Int g = top[1] + (Int)((low[1] - top[1]) * t);
		const Int b = top[2] + (Int)((low[2] - top[2]) * t);
		const Real y0 = horizon * i / bands, y1 = horizon * (i + 1) / bands;
		TheDisplay->drawFillRect(0, (Int)y0, (Int)w, (Int)(y1 - y0) + 1, GameMakeColor(r, g, b, 255));
	}
	if (horizon < h)
		TheDisplay->drawFillRect(0, (Int)horizon, (Int)w, (Int)(h - horizon) + 1, GameMakeColor(low[0], low[1], low[2], 255));
}
GameMessageTranslator *Zhbo_CreateTranslator()
{
	return zhboCreateTranslator();
}

void Zhbo_OnZhboCommand(Object *obj, const CommandButton *button)
{
	if (!obj || !button)
		return;
	const AsciiString &name = button->getName();
	if (name == "Command_ZHBO_BuildMenu" || name == "Command_ZHBO_BuildBack")
	{
		// The command bar shows the selected object's command set: switch it (UI only).
		const AsciiString set = name == "Command_ZHBO_BuildMenu" ? zhboBuildSetFor(obj) : AsciiString::TheEmptyString;
		if (name == "Command_ZHBO_BuildMenu" && set.isEmpty())
		{
			logLine("Build menu: no dozer command set for this General's side");
			return;
		}
		obj->setCommandSetStringOverride(set);
		if (TheControlBar)
			TheControlBar->markUIDirty();
		logf("General %u %s", (unsigned)obj->getID(), set.isEmpty() ? "closed the build menu" : "opened the build menu");
		return;
	}
	if (name == "Command_ZHBO_ClassNext")
	{
		// The class set is one page (at most ten classes in page one's twelve class slots), so the
		// bridge never places this button. Its INI entry stays (no INI churn for a button the bar
		// no longer shows); if a stale set still showed it, never fall through to Take Control.
		logLine("class menu: the class set is one page; ClassNext does nothing");
		return;
	}
	if (name == "Command_ZHBO_ClassBack")
	{
		zhboCloseClassSet(obj);
		return;
	}
	if (rtsOrderAction(button) == ZHBO_ACTION_RELOAD)
	{
		sendRtsOrder(ZHBO_ACTION_RELOAD);
		return;
	}
	// Take Control: the class set (if it was up) hands the bar back to the General's own commands.
	zhboCloseClassSet(obj);
	Zhbo_RequestTakeControl(obj);
}

Real Zhbo_WorkRate(const Object *builder)
{
	return isGeneral(builder) ? zhbo_general_build_rate() : 1.0f;
}

void Zhbo_RequestTakeControl(Object *obj)
{
	if (!obj)
		return;
	GameMessage *msg = TheMessageStream->appendMessage(GameMessage::MSG_ZHBO_TAKE_CONTROL);
	msg->appendObjectIDArgument(obj->getID());
}

Int Zhbo_WeaponButtonState(const Object *obj, const CommandButton *command)
{
	if (!s_ready || !command || !isGeneral(obj))
		return -1;
	ZhboRtsWeapons ammo = {};
	zhbo_client_rts_ammo((uint32_t)obj->getID(), &ammo);
	// His Lethal, Tactical and Melee orders share the third slot: each is as good as what it has
	// left (the core's `orders`), never the slot's weapon state.
	const Int order = rtsOrderAction(command);
	if (order == ZHBO_ACTION_LETHAL || order == ZHBO_ACTION_TACTICAL || order == ZHBO_ACTION_MELEE)
		return (ammo.orders >> (order - ZHBO_ACTION_LETHAL)) & 1 ? 1 : 0;
	const Int slot = command->getCommandType() == GUI_COMMAND_SWITCH_WEAPON ? (Int)command->getWeaponSlot() : -1;
	if (slot < 0 || slot > 1)
		return -1;
	return ammo.ammo[slot] + ammo.reserve[slot] <= 0 ? 0 : -1;
}

void Zhbo_OnSwitchWeaponButton(const CommandButton *command)
{
	// His switch buttons are his two guns, which Zero Hour's own switch message handles.
	(void)command;
}

void Zhbo_OnFireWeaponCommand(const CommandButton *command)
{
	if (!s_ready)
		return;
	const Int action = rtsOrderAction(command);
	if (action == ZHBO_ACTION_LETHAL || action == ZHBO_ACTION_TACTICAL || action == ZHBO_ACTION_MELEE)
		sendRtsOrder(action);
}

const WideChar *Zhbo_FetchString(const char *label)
{
	// Usable before Zhbo_Init: the string table is static data in the Rust core.
	const uint16_t *text = zhbo_string(label);
	if (!text)
		return nullptr;
	// The ABI is UTF-16 (zhbo_core.h). WideChar is wchar_t: 16-bit on Windows, 32-bit on
	// macOS/Linux. Reinterpreting the buffer on the 32-bit platforms reads two UTF-16 units as
	// one WCHAR, runs past the 16-bit NUL terminator, and the font draws the garbage as "???".
	if (sizeof(WideChar) == sizeof(uint16_t))
		return reinterpret_cast<const WideChar *>(text);

	// GameTextManager::fetch copies the text into a UnicodeString before returning, so one
	// process-stable buffer is enough (and keeps the ABI unchanged).
	static std::vector<WideChar> wide;
	wide.clear();
	for (const uint16_t *c = text; *c; ++c)
	{
		unsigned cp = *c;
		if (cp >= 0xD800 && cp <= 0xDBFF && c[1] >= 0xDC00 && c[1] <= 0xDFFF)
		{
			cp = 0x10000u + ((cp - 0xD800u) << 10) + (c[1] - 0xDC00u);
			++c;
		}
		wide.push_back(static_cast<WideChar>(cp));
	}
	wide.push_back(0);
	return wide.data();
}
