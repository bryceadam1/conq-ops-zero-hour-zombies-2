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
#include "Common/crc.h"
#include "Common/CRCDebug.h"
#include "Common/File.h"
#include "Common/FileSystem.h"
#include "Common/GameEngine.h"
#include "Common/AudioHandleSpecialValues.h"
#include "Common/GameAudio.h"
#include "Common/GameUtility.h"
#include "Common/GlobalData.h"
#include "Common/MessageStream.h"
#include "Common/Player.h"
#include "Common/PlayerList.h"
#include "Common/PlayerTemplate.h"
#include "Common/Recorder.h"
#include "Common/Radar.h"
#include "Common/Team.h"
#include "Common/ThingFactory.h"
#include "Common/ThingTemplate.h"
#include "Common/Xfer.h"
#include "Common/LocalFileSystem.h"
#include "Common/MapObject.h"
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
double s_classScreenSeenAt = -1.0;
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

static void installZombieMaps();
static Bool isZombiesMap();
// The loaded map is a Zombies map (set at the loading screen, Zhbo_Preload).
static Bool s_zombiesMap = FALSE;

// A file of the mod's in the game's user-data directory. Several game instances on one machine
// (the engine's multi-instance mode: the two-instance multiplayer test) share that directory; like
// the engine's own Options_Instance02.ini, every instance after the first gets its own copy
// (ConquerOps_Instance02.log, ...), so two running games never write each other's log or profile.
static std::string userDataFile(const char *stem, const char *ext)
{
	std::string path = TheGlobalData ? TheGlobalData->getPath_UserData().str() : "";
	path += stem;
	if (rts::ClientInstance::getInstanceId() > 1u)
	{
		char suffix[32];
		snprintf(suffix, sizeof(suffix), "_Instance%02u", rts::ClientInstance::getInstanceId());
		path += suffix;
	}
	path += ext;
	return path;
}

// The player's create-a-class profile: a text file in the game's user-data directory, next to the
// mod's log. Not in a BIG file (it is the player's own, written at runtime).
static std::string profilePath()
{
	return userDataFile("ConquerOps", ".profile");
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
	s_config.autopilot_mp = s_config.autopilot_enabled && strcmp(autopilot, "mp") == 0;
	s_config.autopilot_smoke = s_config.autopilot_enabled && strcmp(autopilot, "smoke") == 0;
	s_config.autopilot_splash = s_config.autopilot_enabled && strcmp(autopilot, "splash") == 0;
	s_config.autopilot_flicker = s_config.autopilot_enabled && strcmp(autopilot, "flicker") == 0;
	s_config.autopilot_drops = s_config.autopilot_enabled && strcmp(autopilot, "drops") == 0;
	s_config.autopilot_hipfire = s_config.autopilot_enabled && strcmp(autopilot, "hipfire") == 0;
	s_config.autopilot_command = s_config.autopilot_enabled && strcmp(autopilot, "command") == 0;
	s_config.autopilot_defeat = s_config.autopilot_enabled && strcmp(autopilot, "defeat") == 0;
	s_config.force_radar = getenv("ZHBO_FORCE_RADAR") != nullptr;
	s_config.autotest_skirmish = getenv("ZHBO_AUTOTEST_SKIRMISH") != nullptr;
	s_config.showcase_object = getenv("ZHBO_SHOWCASE_OBJECT");
	s_config.autopilot_general = getenv("ZHBO_AUTOPILOT_GENERAL");
	if (const char *v = getenv("ZHBO_AUTOPILOT_ZOMBIES"))
		s_config.autopilot_zombies = atoi(v);
	if (const char *v = getenv("ZHBO_AUTOPILOT_SPAWNERS"))
		s_config.autopilot_spawners = atoi(v);
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
	// TranZit's survivors (zhbo_core::names::SURVIVORS): Generals of no player side, on a Zombies
	// map only; their arms and body follow the General object, never the player's side.
	{ "", 'M', "MIS", "ZHBO_General_Survivor_Misty" },
	{ "", 'E', "MAR", "ZHBO_General_Survivor_Marlton" },
	{ "", 'R', "RUS", "ZHBO_General_Survivor_Russman" },
	{ "", 'S', "STU", "ZHBO_General_Survivor_Stuhlinger" },
};

const ZhboFaction *zhboFactionOfSide(const AsciiString &side)
{
	for (const ZhboFaction &f : FACTIONS)
		if (f.side[0] && side == f.side) // a survivor has no player side
			return &f;
	return nullptr;
}

const ZhboFaction *zhboFactionOfLetter(char letter)
{
	for (const ZhboFaction &f : FACTIONS)
		if (letter == f.letter)
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

// Multiplayer: every peer's sim must run on the same data, but each player converted the weapon
// table, and keeps the tuning pack and the classes, on his own machine; the engine's INI CRC
// (GameEngine::init, computed before Zhbo_Init) covers only the INI files the engine itself loads
// (the converted archive's Object/Weapon/FXList INIs among them), not these. The core's content
// fingerprint (content.rs) hashes what reaches the sim, part by part over the engine's CRC: the
// weapon table's sim fields (not its localized names, DLC locks or art), the class-pick
// vocabulary, the tuning pack, the class list, ZHBO_LOADOUT/ZHBO_AUTOPILOT, the mod build. Its
// total becomes m_iniCRC: the lobbies compare it (online staging rooms refuse a mismatch; LAN does
// since host patch 0018, and says which part differs since 0021), and replays record and check it.
static ZhboContentCrc s_content;
static Bool s_contentReady = FALSE;
// The other machine's parts from the last refused LAN join (host patch 0021).
static UnsignedInt s_refusedContent[ZHBO_CONTENT_PARTS];

static void foldModContentIntoIniCrc()
{
	const UnsignedInt engine = TheGlobalData->m_iniCRC;
	const UnsignedInt mod = zhbo_content_crc(engine, &s_content);
	s_contentReady = TRUE;
	TheWritableGlobalData->m_iniCRC = mod;
	const uint32_t *p = s_content.parts;
	logf("multiplayer: content CRC %08X (engine INI %08X, weapon table %08X, class vocabulary %08X, tuning %08X, classes %08X, settings %08X, build %08X)",
		(unsigned)mod, (unsigned)p[0], (unsigned)p[1], (unsigned)p[2], (unsigned)p[3], (unsigned)p[4], (unsigned)p[5], (unsigned)p[6]);
}

static UnicodeString unicodeOf(const char *utf8)
{
	// The content messages are the mod's ASCII strings (strings_en.ini).
	UnicodeString u;
	u.translate(AsciiString(utf8));
	return u;
}

// ------------------------------------------------------------------------------------------------
// Public entry points
// ------------------------------------------------------------------------------------------------
void Zhbo_ContentParts(UnsignedInt *out, Int n)
{
	for (Int i = 0; i < n; ++i)
		out[i] = s_contentReady && i < ZHBO_CONTENT_PARTS ? s_content.parts[i] : 0u;
}

UnicodeString Zhbo_ContentLabel()
{
	char buf[256];
	if (!s_contentReady || zhbo_content_label(buf, sizeof(buf)) < 0)
		return UnicodeString::TheEmptyString;
	return unicodeOf(buf);
}

UnicodeString Zhbo_ContentMismatch(const UnsignedInt *theirs, Int n)
{
	UnsignedInt parts[ZHBO_CONTENT_PARTS] = {};
	for (Int i = 0; theirs && i < n && i < ZHBO_CONTENT_PARTS; ++i)
		parts[i] = theirs[i];
	char buf[512];
	if (!s_contentReady || zhbo_content_mismatch(parts, buf, sizeof(buf)) < 0)
		return UnicodeString::TheEmptyString;
	logf("multiplayer: content mismatch: %s (theirs %08X %08X %08X %08X %08X %08X %08X)", buf,
		(unsigned)parts[0], (unsigned)parts[1], (unsigned)parts[2], (unsigned)parts[3], (unsigned)parts[4], (unsigned)parts[5], (unsigned)parts[6]);
	return unicodeOf(buf);
}

void Zhbo_NoteRefusedContent(const UnsignedInt *theirs, Int n)
{
	for (Int i = 0; i < ZHBO_CONTENT_PARTS; ++i)
		s_refusedContent[i] = theirs && i < n ? theirs[i] : 0u;
}

UnicodeString Zhbo_RefusedContentText()
{
	return Zhbo_ContentMismatch(s_refusedContent, ZHBO_CONTENT_PARTS);
}

UnicodeString Zhbo_ContentVersus(UnsignedInt theirTotal)
{
	char buf[256];
	if (!s_contentReady || zhbo_content_versus(theirTotal, buf, sizeof(buf)) < 0)
		return UnicodeString::TheEmptyString;
	logf("multiplayer: %s", buf);
	return unicodeOf(buf);
}

void Zhbo_Init()
{
	if (s_ready)
		return;

	s_log = fopen(userDataFile("ConquerOps", ".log").c_str(), "w");

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
		.surface_at = cbSurfaceAt,
		.order_attack = cbOrderAttack,
		.set_max_health = cbSetMaxHealth,
		.add_cash = cbAddCash,
		.spawn_survivor = cbSpawnSurvivor,
		.money = cbMoney,
		.kill = cbKill,
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
		.sound_gain = cbSoundGain,
		.sound_loop = cbSoundLoop,
		.player_name = cbPlayerName,
		.image_quad = cbImageQuad,
		.command_objects = cbCommandObjects,
		.template_name = cbTemplateName,
		.issue_order = cbIssueOrder,
		.command_buttons = cbCommandButtons,
		.map_places = cbMapPlaces,
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
	{
		// The first-person commands' synonyms: the mod's, then the player's own beside his profile
		// (read only; every instance shares it).
		std::string player;
		const std::string path = (TheGlobalData ? std::string(TheGlobalData->getPath_UserData().str()) : std::string()) + zhbo_names::PLAYER_COMMANDS_FILE;
		if (FILE *f = fopen(path.c_str(), "rb"))
		{
			char buf[4096];
			size_t n;
			while ((n = fread(buf, 1, sizeof(buf), f)) > 0)
				player.append(buf, n);
			fclose(f);
			logf("commands: the player's own synonyms from %s", path.c_str());
		}
		zhbo_load_command_words(readTextFile(zhbo_names::COMMANDS_PATH).c_str(), player.c_str());
	}
	zhbo_load_classes(readTextFile(zhbo_names::CLASSES_PATH).c_str());
	foldModContentIntoIniCrc();
	// BO2's impact table (R8d): the FXLists the converted archive carries for each bucket and
	// surface. Without it (or without the archive) no impact FX plays; the sound still does.
	const std::string impacts = readTextFile(zhbo_names::IMPACT_FX_PATH);
	if (!impacts.empty())
		logf("impacts: %d BO2 impact cells installed", (int)zhbo_load_impacts(impacts.c_str()));
	else
		logLine("impacts: no converted BO2 impact table");
	// BO2's third-person shots layer by layer (soundlayers.rs); without them others' shots play the
	// converted close mix with Zero Hour's falloff.
	const std::string layers = readTextFile(zhbo_names::SOUND_LAYERS_PATH);
	if (!layers.empty())
		logf("sound layers: %d BO2 third-person layers installed", (int)zhbo_load_sound_layers(layers.c_str()));
	zhbo_load_fonts(readTextFile(zhbo_names::FONTS_PATH).c_str());
	installZombieMaps();
	// Zombies' rule table (the install's mp/zombiemode.csv): the round engine's numbers.
	const std::string zombieRules = readTextFile(zhbo_names::ZOMBIE_RULES_PATH);
	if (!zombieRules.empty())
		logf("zombies: %d rule table rows installed", (int)zhbo_load_zombie_rules(zombieRules.c_str()));
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

	zhboLogGraphicsDetail();

	s_lastTick = zhbo_platform::now();
	s_ready = TRUE;
}

// A resolution change (the options menu, a live window resize, a second instance applying its own
// Options_InstanceNN.ini) rebuilds the whole ControlBar from its INI (InGameUI::recreateControlBar,
// host patch 0023): every injected button and made set is gone, so the Command Center has no Deploy
// General and the General no build or class menu. Put them back.
void Zhbo_ControlBarRebuilt()
{
	if (!s_ready)
		return;
	logLine("the control bar was rebuilt (a resolution change); adding the Conquer Ops buttons again");
	zhboInjectConstructButtons();
}

void Zhbo_Reset()
{
	// Every game start passes here (GameLogic::startNewGame), a loaded save too, which sends no
	// MSG_NEW_GAME: pace it at the speed its message noted, else Zero Hour's 30.
	zhboPaceStartingGame();
	zhboTeamRingsReset();
	s_zombiesMap = FALSE;
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
	ResetSurfaceGrid();
	ResetRtsWeapons();
	ResetHud();
	ResetBuilding();
	ResetAutopilotTrackers();
	ResetControlBarTrackers();
	s_wheelAccum = 0.0f;
	s_classScreenSeenAt = -1.0;
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
	// The map's ground surfaces, before the first logic step can ask for one.
	EnsureSurfaceGrid();
	// A Zombies map runs the core's round engine (and only one does: no map-wide scan elsewhere).
	s_zombiesMap = isZombiesMap();
	zhbo_zombies_map_loaded(s_zombiesMap ? 1 : 0);
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
	// The seventh argument: the frame the sender's machine showed (lag-compensated shots, its
	// prediction's acknowledgement). A message without it (an older replay) is resolved as it is.
	if (msg->getArgumentCount() >= 7)
		zhbo_logic_fps_input(playerIndex, (uint32_t)msg->getArgument(0)->objectID, &input, (uint32_t)msg->getArgument(6)->integer);
	else
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
	// Zero Hour's own desync check (MSG_LOGIC_CRC every CRC interval) shows its mismatch only on
	// screen in a release build: put it in the log too, once, for the multiplayer test.
	static Bool s_crcMismatchLogged = FALSE;
	if (frame <= 1)
		s_crcMismatchLogged = FALSE;
	// Where a game's frames begin in the log: the shell map runs the logic too (its frames, and the
	// state hash lines, count from 0 as well), so a comparison must take the match's alone.
	if (frame == 1)
		logf("game start: %s", TheGameLogic->isInShellGame() ? "the shell map" : (TheNetwork ? "a network game" : "a local game"));
	// Where this machine leaves a network game, by any path: the quit menu's Exit, the window's
	// close or a SIGTERM (both MSG_META_DEMO_INSTANT_QUIT -> GameLogic::quit), or the harness's
	// wall-clock exit (GGC_AUTO_EXIT_SECONDS: GameEngine::setQuitting). GameLogic::quit sends the
	// local player's MSG_SELF_DESTRUCT, which every machine runs in lockstep (his General dies on
	// all of them with no blow), and then this machine steps its last frames without the other
	// players' commands: the multiplayer test compares the state hash only before this line.
	static Bool s_leaveLogged = FALSE;
	if (frame <= 1)
		s_leaveLogged = FALSE;
	if (!s_leaveLogged && TheNetwork && !TheGameLogic->isInShellGame() &&
		(TheGameLogic->isQuitToDesktopRequested() || (TheGameEngine && TheGameEngine->getQuitting())))
	{
		s_leaveLogged = TRUE;
		logf("network game: this machine is leaving the game (frame %u)", (unsigned)frame);
	}
	if (!s_crcMismatchLogged && TheNetwork && TheNetwork->sawCRCMismatch())
	{
		s_crcMismatchLogged = TRUE;
		logf("multiplayer: Zero Hour reported a CRC mismatch (a desync) by frame %u", (unsigned)frame);
	}
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
	if (s_config.autopilot_smoke && s_autopilot.spawned)
		smokeAutopilotFrame(frame);
	if (s_config.autopilot_command && s_autopilot.spawned)
		commandAutopilotFrame(frame);
	if (s_config.autopilot_defeat && s_autopilot.spawned)
		defeatAutopilotFrame(frame);
	rtsWeapons(frame);
	trackGeneralBuilds(frame);
	trackIdleGenerals();
	zhbo_logic_step(frame);
}

Bool Zhbo_IsControlled(ObjectID id)
{
	return s_ready && zhbo_logic_is_controlled((uint32_t)id) != 0;
}

void Zhbo_XferLogicCRC(Xfer *xfer, Bool logicFrame)
{
	if (!s_ready || xfer == nullptr)
		return;
	ZhboLogicCrc crc;
	// An idle sim (no General deployed, no class picked) adds nothing: the CRC stays stock
	// Zero Hour's, so a game without the mod's play checks like one without the mod.
	if (zhbo_logic_crc(&crc, logicFrame ? 1 : 0) == 0)
		return;
	AsciiString marker = "MARKER:ConquerOps";
	xfer->xferAsciiString(&marker);
	UnsignedInt lo = (UnsignedInt)crc.hash;
	UnsignedInt hi = (UnsignedInt)(crc.hash >> 32);
	xfer->xferUnsignedInt(&lo);
	xfer->xferUnsignedInt(&hi);
	if (logicFrame)
	{
		// The CRC debug log (DEBUG_CRC builds; dumped on a mismatch): the parts name the seat, or
		// the shared world, whose state went apart. ConquerOps.log has the same line in every build.
		CRCGEN_LOG(("ConquerOps sim frame %u: %8.8X%8.8X world %8.8X%8.8X", crc.frame, hi, lo,
			(UnsignedInt)(crc.world >> 32), (UnsignedInt)crc.world));
		for (Int i = 0; i < (Int)(sizeof(crc.seats) / sizeof(crc.seats[0])); ++i)
		{
			if (crc.seats[i] != 0)
				CRCGEN_LOG(("ConquerOps seat %d: %8.8X%8.8X", i, (UnsignedInt)(crc.seats[i] >> 32), (UnsignedInt)crc.seats[i]));
		}
	}
}

// Zero Hour's damage types in the core's vocabulary: rounds and blows a shield stops, blasts it
// cuts; -1 for everything else (fire, poison, radiation, ...), which a shield lets through.
static int32_t shieldDamageKind(DamageType type)
{
	switch (type)
	{
		case DAMAGE_SMALL_ARMS:
		case DAMAGE_GATTLING:
		case DAMAGE_SNIPER:
		case DAMAGE_COMANCHE_VULCAN:
			return ZHBO_DAMAGE_SMALL_ARMS;
		case DAMAGE_MELEE:
			return ZHBO_DAMAGE_MELEE;
		case DAMAGE_EXPLOSION:
		case DAMAGE_ARMOR_PIERCING:
		case DAMAGE_INFANTRY_MISSILE:
		case DAMAGE_JET_MISSILES:
		case DAMAGE_STEALTHJET_MISSILES:
		case DAMAGE_LAND_MINE:
		case DAMAGE_AURORA_BOMB:
			return ZHBO_DAMAGE_EXPLOSION;
		default:
			return -1;
	}
}

void Zhbo_ShieldDamage(Object *obj, DamageInfo *info)
{
	if (!s_ready || obj == nullptr || info == nullptr || info->in.m_kill || info->in.m_amount <= 0.0f)
		return;
	const Object *src = TheGameLogic->findObjectByID(info->in.m_sourceID);
	// A zombie's swipe deals BO2's own damage to a General: its meleedamage as his share of BO2's
	// player health (zhbo_zombie_hit_damage), not the zombie weapon's Zero Hour amount.
	if (src && isZombie(src) && isGeneral(obj) && obj->getBodyModule())
	{
		info->in.m_amount = zhbo_zombie_hit_damage(obj->getBodyModule()->getMaxHealth());
		logf("zombie %u swiped General %u for %.1f of %.1f", (unsigned)src->getID(), (unsigned)obj->getID(), info->in.m_amount, obj->getBodyModule()->getMaxHealth());
	}
	const Coord3D *from = src ? src->getPosition() : nullptr;
	const float height = src ? src->getGeometryInfo().getMaxHeightAbovePosition() : 0.0f;
	const float scale = zhbo_logic_shield_damage((uint32_t)obj->getID(), (uint32_t)info->in.m_sourceID,
		from ? from->x : 0.0f, from ? from->y : 0.0f, from ? from->z : 0.0f, height, from ? 1 : 0,
		shieldDamageKind(info->in.m_damageType), info->in.m_amount);
	if (scale < 1.0f)
		info->in.m_amount *= scale < 0.0f ? 0.0f : scale;
	// A survivor on a Zombies map goes down instead of dying while another is up, and a downed one
	// takes no more harm (BO2's last stand): the core decides on the damage his body would take
	// after its armour (estimateDamage), and the blow is scaled to what it allows.
	if (s_zombiesMap && isSurvivor(obj) && info->in.m_amount > 0.0f && obj->getBodyModule())
	{
		BodyModuleInterface *body = obj->getBodyModule();
		const Real actual = body->estimateDamage(info->in);
		if (actual > 0.0f)
		{
			const float allowed = zhbo_logic_survivor_damage((uint32_t)obj->getID(), actual, body->getHealth());
			if (allowed < actual)
				info->in.m_amount *= allowed <= 0.0f ? 0.0f : allowed / actual;
		}
	}
	// What reached him, for his hurt and death (BO2's vDir): the blast's own direction when it
	// carries a shock wave (Weapon.cpp: victim less the exploding object), else from the source.
	if (info->in.m_amount > 0.0f)
	{
		Coord3D dir = info->in.m_shockWaveVector;
		if (dir.x == 0.0f && dir.y == 0.0f && dir.z == 0.0f && from)
		{
			dir = *obj->getPosition();
			dir.sub(*from);
		}
		zhbo_logic_engine_damage((uint32_t)obj->getID(), zhboDamageKind(info->in.m_damageType), dir.x, dir.y, dir.z);
	}
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
	// Only watching: an observer seat, a defeated player who stayed, or a replay's playback (whose
	// local player is the ReplayObserver). The core then never sends input for the seat it views.
	const Player *self = ThePlayerList->getLocalPlayer();
	in.spectating = (!self || !self->isPlayerActive() || (TheRecorder && TheRecorder->isPlaybackMode())) ? 1 : 0;
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
			"BACKSPACE", "7", "8", "9", "0", "-",
		};
		static_assert(sizeof(keyNames) / sizeof(keyNames[0]) == (int)zhbo_platform::keys::Key::COUNT, "key vocabulary must match");
		char held[96];
		Int n = 0;
		uint32_t hash = 0;
		for (Int k = 0; k < (Int)zhbo_platform::keys::Key::COUNT; ++k)
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

// R8d: the shot impacts played this second (the counter line is the autotest's proof that the
// names resolved and the FXLists ran).
static UnsignedInt s_impactFrames = 0;
static UnsignedInt s_impactPlayed = 0;
static UnsignedInt s_impactMissing = 0;
static Bool s_impactNameLogged = FALSE;

// Apply what the core asked for this frame: the input message, mouse capture, UI and sky state,
// the hidden body, the RTS camera's look-at, and the local player's shot impacts.
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
		msg->appendIntegerArgument((Int)s_out.input_seen_frame);
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

	// The General back overhead alive (the field computer, or Escape declining a spawn without a
	// class): select him again, as a click would, so his command bar is up (taking control
	// deselected him). The core sets it once, on the frame the camera lands.
	if (s_out.select_object)
	{
		Object *obj = TheGameLogic->findObjectByID((ObjectID)s_out.select_object);
		if (obj && !obj->isEffectivelyDead() && obj->getDrawable())
		{
			TheInGameUI->deselectAllDrawables();
			TheInGameUI->selectDrawable(obj->getDrawable());
			GameMessage *group = TheMessageStream->appendMessage(GameMessage::MSG_CREATE_SELECTED_GROUP);
			group->appendBooleanArgument(TRUE);
			group->appendObjectIDArgument(obj->getID());
			logf("selected General %u back overhead", (unsigned)obj->getID());
		}
		s_out.select_object = 0;
	}

	// The frame's shot impacts, the local player's and then other players' (R8d): BO2's impact-table cell at the hit point, oriented by
	// the surface frame. Each pellet has its own hit, so a shotgun raises up to eight.
	for (int32_t i = 0; i < s_out.hits_len && i < ZHBO_CLIENT_HITS; ++i)
	{
		const ZhboClientHit &hit = s_out.hits[i];
		char name[128] = "";
		const int32_t found = hit.exit ? zhbo_impact_exit_fx(hit.impact_type, hit.surface, hit.head, hit.fatal, name, (int32_t)sizeof(name))
		                               : zhbo_impact_fx(hit.impact_type, hit.surface, hit.head, hit.fatal, name, (int32_t)sizeof(name));
		if (!found || !name[0])
		{
			++s_impactMissing;
			continue;
		}
		const FXList *fx = TheFXListStore ? TheFXListStore->findFXList(name) : nullptr;
		if (!fx)
		{
			++s_impactMissing;
			if (!s_impactNameLogged)
			{
				s_impactNameLogged = TRUE;
				logf("impact fx: '%s' is not a converted FXList (stale archive?)", name);
			}
			continue;
		}
		Coord3D at;
		at.set(hit.point.x, hit.point.y, hit.point.z);
		// The surface's outward normal is the shot's reverse: the puff leaves the wall toward the
		// shooter, as BO2's impact frame does (the particle velocity axis 0 is the normal).
		Vector3 normal(-hit.dir.x, -hit.dir.y, -hit.dir.z);
		Matrix3D m = zhboFrameAt(at, normal);
		FXList::doFXPos(fx, &at, &m);
		++s_impactPlayed;
	}
	if (s_out.hits_len)
		s_out.hits_len = 0;

	// What they did, once a second, only while something happened.
	if (++s_impactFrames >= 60)
	{
		if (s_impactPlayed || s_impactMissing)
			logf("impact fx: %u played, %u missing", (unsigned)s_impactPlayed, (unsigned)s_impactMissing);
		s_impactFrames = 0;
		s_impactPlayed = 0;
		s_impactMissing = 0;
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
		autopilotShellFrame();
		return;
	}

	autopilotFrame();
	zhboDetailFrame();
	// A General with no class shows the class set instead of his own commands; once he has one (or
	// Create a Class opened it for a re-pick) the bar tracks that.
	zhboUpdateClassBar();

	const Bool focused = zhbo_platform::windowFocused();
	const Bool menuUp = TheInGameUI->isQuitMenuVisible() || TheGameLogic->isGamePaused();

	ZhboClientIn in;
	gatherClientInput(in, focused, menuUp);

	const Bool classScreenWasUp = s_out.menu_open != 0;
	zhbo_client_frame(&in, &s_out);
	if (classScreenWasUp || s_out.menu_open)
		s_classScreenSeenAt = zhbo_platform::now();
	applyWorldPoses();
	zhboTeamRingsFrame();
	applyClientOutput(focused, menuUp);
}


Bool Zhbo_GetCameraOverride(Coord3D *eye, Coord3D *dir, Real *horizontalFov, Real *nearPlane, Real *roll)
{
	if (!s_ready || !s_out.camera_override)
		return FALSE;
	eye->set(s_out.eye.x, s_out.eye.y, s_out.eye.z);
	dir->set(s_out.dir.x, s_out.dir.y, s_out.dir.z);
	*horizontalFov = s_out.hfov;
	*roll = s_out.view_roll; // the view kick's roll, Look_At_Dir's argument
	// The near plane: `*nearPlane` comes in as the overhead view's own (NearZ = MAP_XY_FACTOR = 10,
	// W3DView.cpp) and leaves scaled by the camera's distance from the eyes as a share of the
	// overhead camera's (ZhboClientOut.near_scale), never below the core's first-person near (BO2's
	// r_znear, 4" ~ 0.67 units): the near keeps the overhead view's ratio to the ground the camera
	// looks at as it dives. A 0.1 near through the whole dive left the depth buffer a hundredth of
	// the overhead view's precision with the camera hundreds of units up: the roads and shadows on
	// the ground fought the terrain. The native viewmodel draws on a depth and near of its own
	// (zhbo-gfx native.rs); the W3D fallback's gun is solid at BO2's near (MODLOG's MP7 sweep found
	// 0.1 and 1.0 solid, the tear at 2.0).
	const Real worldNear = *nearPlane;
	const Real fpsNear = s_out.fps_near;
	const Real scale = s_out.near_scale < 0.0f ? 0.0f : (s_out.near_scale > 1.0f ? 1.0f : s_out.near_scale);
	if (fpsNear > 0.0f)
		*nearPlane = std::max(fpsNear, worldNear * scale);
	return TRUE;
}

Bool Zhbo_GetViewmodel(Int *clip, Int *weapon, Bool *fired, Bool *firedLeft, Real *time, Real *ads, AsciiString *model, char *faction, Coord3D *offset, Coord3D *angles)
{
	if (!s_ready || !s_out.camera_override || s_out.viewmodel < 0 || !s_out.hide_object)
		return FALSE;
	const Object *general = TheGameLogic->findObjectByID((ObjectID)s_out.hide_object);
	if (!general)
		return FALSE;
	// One set of viewmodels per faction (zhbo_assets::generals::FACTIONS).
	const AsciiString &name = general->getTemplate()->getName();
	const ZhboFaction *side = zhboFactionOfGeneralName(name);
	// The tablet clips play on the arms-and-tablet model (generals::tablet_model); everything else on
	// the converted weapon's viewmodel (generals::vm_model).
	// (An alt mode's clips come after the tablet's numbers, on the weapon's own viewmodel.)
	// The hammer's likewise on the arms-and-hammer model (generals::hammer_model).
	const Bool tablet = s_out.viewmodel >= ZHBO_VM_TABLET_PULLOUT && s_out.viewmodel <= ZHBO_VM_TABLET_PUTAWAY;
	const Bool hammer = s_out.viewmodel >= ZHBO_VM_HAMMER_PULLOUT && s_out.viewmodel <= ZHBO_VM_HAMMER_PUTAWAY;
	AsciiString vm;
	if (!side)
		return FALSE;
	if (tablet || hammer)
		vm.format("ZHBO_VM%s_%s", side->code, tablet ? "TAB" : "HAM");
	else if (s_out.viewmodel_weapon >= 0)
		vm.format("ZV%02d%c_SKN", s_out.viewmodel_weapon, side->letter);
	else
		return FALSE;
	*clip = s_out.viewmodel;
	*weapon = tablet || hammer ? ZHBO_VM_NONE : s_out.viewmodel_weapon;
	*fired = s_out.viewmodel_fired != 0;
	*firedLeft = s_out.viewmodel_fired_left != 0;
	*time = s_out.viewmodel_time;
	*ads = s_out.viewmodel_ads;
	offset->set(s_out.viewmodel_offset.x, s_out.viewmodel_offset.y, s_out.viewmodel_offset.z);
	angles->set(s_out.viewmodel_angles.x, s_out.viewmodel_angles.y, s_out.viewmodel_angles.z);
	model->set(vm);
	*faction = side->letter;
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

// Whether the loaded map is a Zombies map: one of its objects is a zombie spawn point.
static Bool isZombiesMap()
{
	for (MapObject *m = MapObject::getFirstMapObject(); m; m = m->getNext())
		if (zhbo_names::isZombieSpawner(m->getName().str()))
			return TRUE;
	return FALSE;
}

// A slot's survivor template (Misty, Marlton, Russman, Stuhlinger by slot).
static const char *survivorOfSlot(Int slot)
{
	const Int n = (Int)(sizeof(zhbo_names::SURVIVOR_OBJECTS) / sizeof(zhbo_names::SURVIVOR_OBJECTS[0]));
	return zhbo_names::SURVIVOR_OBJECTS[(slot % n + n) % n];
}

// The game slot a player plays from ("player<slot>", GameLogic::startNewGame), or -1.
static Int slotOfPlayer(const Player *player)
{
	for (Int i = 0; player && ThePlayerList && i < MAX_SLOTS; ++i)
	{
		AsciiString name;
		name.format("player%d", i);
		if (ThePlayerList->findPlayerWithNameKey(TheNameKeyGenerator->nameToKey(name)) == player)
			return i;
	}
	return -1;
}

uint32_t cbSpawnSurvivor(int32_t playerIndex, const ZhboVec3 *pos, float yaw)
{
	Player *player = ThePlayerList ? ThePlayerList->getNthPlayer(playerIndex) : nullptr;
	const Int slot = slotOfPlayer(player);
	const ThingTemplate *tmpl = slot >= 0 && TheThingFactory ? TheThingFactory->findTemplate(survivorOfSlot(slot)) : nullptr;
	if (!tmpl || !player->getDefaultTeam())
		return 0;
	Object *obj = TheThingFactory->newObject(tmpl, player->getDefaultTeam());
	if (!obj)
		return 0;
	Coord3D p;
	p.set(pos->x, pos->y, TheTerrainLogic ? TheTerrainLogic->getGroundHeight(pos->x, pos->y) : pos->z);
	obj->setPosition(&p);
	obj->setOrientation(yaw);
	player->onUnitCreated(nullptr, obj);
	return (uint32_t)obj->getID();
}

Bool zhboZombiesMap()
{
	return s_zombiesMap;
}

int32_t cbMoney(int32_t playerIndex)
{
	Player *player = ThePlayerList ? ThePlayerList->getNthPlayer(playerIndex) : nullptr;
	return player && player->getMoney() ? (int32_t)player->getMoney()->countMoney() : 0;
}

AsciiString Zhbo_ZombiesStartObject(Int slotNum, Player *player)
{
	if (!isZombiesMap() || !player || !ThePlayerList)
		return AsciiString::TheEmptyString;
	// BO2's starting points (level.player_starting_points) in the one wallet, not the skirmish's
	// starting cash.
	if (Money *money = player->getMoney())
	{
		money->withdraw(money->countMoney(), FALSE);
		money->deposit((UnsignedInt)std::max(0, (int)zhbo_zombie_start_points()), FALSE, FALSE);
	}
	// The zombies are the neutral side's (they join their spawn point's): it and the player are
	// enemies both ways, so the zombies attack him and his units fight them.
	Player *neutral = ThePlayerList->getNeutralPlayer();
	if (neutral && neutral != player)
	{
		neutral->setPlayerRelationship(player, ENEMIES);
		player->setPlayerRelationship(neutral, ENEMIES);
	}
	const char *survivor = survivorOfSlot(slotNum);
	logf("zombies map: player slot %d starts as %s, no buildings, %d points", (int)slotNum, survivor, (int)zhbo_zombie_start_points());
	return AsciiString(survivor);
}

// The Zombies maps the converter built from the player's own install ride the archive; the map
// list reads user maps from the user-data folder (MapCache::updateCache), so each is copied there
// when missing or out of date.
static void installZombieMaps()
{
	// ZHBO_NO_ZOMBIE_MAPS: leave the user's Maps folder alone (a test switch).
	if (!TheGlobalData || getenv("ZHBO_NO_ZOMBIE_MAPS"))
		return;
	const std::string userData = TheGlobalData->getPath_UserData().str();
	for (const zhbo_names::ZombieMapFile &m : zhbo_names::ZOMBIE_MAP_FILES)
	{
		const std::string bytes = readTextFile(m.archive);
		if (bytes.empty())
			continue;
		const std::string dir = userData + "Maps/" + m.folder;
		const std::string path = dir + "/" + m.file;
		std::string old;
		if (FILE *f = fopen(path.c_str(), "rb"))
		{
			char buf[65536];
			size_t n;
			while ((n = fread(buf, 1, sizeof(buf), f)) > 0)
				old.append(buf, n);
			fclose(f);
		}
		if (old == bytes)
			continue;
		if (TheLocalFileSystem)
		{
			TheLocalFileSystem->createDirectory((userData + "Maps").c_str());
			TheLocalFileSystem->createDirectory(dir.c_str());
		}
		if (FILE *f = fopen(path.c_str(), "wb"))
		{
			fwrite(bytes.data(), 1, bytes.size(), f);
			fclose(f);
			logf("zombies map: installed %s", path.c_str());
		}
	}
}

static void noteLivingGeneral(Object *obj, void *found)
{
	if (isGeneral(obj) && !obj->isEffectivelyDead())
		*static_cast<Bool *>(found) = TRUE;
}

Bool Zhbo_HasLivingGeneral(const Player *player)
{
	// Not gated on s_ready: the rule is the mod's whether or not the core came up, and it reads
	// only the engine's own objects.
	Bool found = FALSE;
	if (player)
		player->iterateObjects(noteLivingGeneral, &found);
	// On a Zombies map the survivors stand together, as BO2's players do: a player whose survivor
	// fell is back at the next round (the core's spectators_respawn) while any survivor lives, and
	// the game is over when none does.
	if (!found && player && s_zombiesMap && ThePlayerList)
		for (Int i = 0; i < ThePlayerList->getPlayerCount() && !found; ++i)
			if (const Player *other = ThePlayerList->getNthPlayer(i))
				if (other != player && other->isPlayerActive())
					other->iterateObjects(noteLivingGeneral, &found);
	return found;
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
