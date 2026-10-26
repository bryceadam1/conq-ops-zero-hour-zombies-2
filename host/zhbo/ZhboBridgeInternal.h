/*
** Conquer Ops - engine-side bridge: shared declarations of the bridge's sections. The section
** files (ZhboLogicApi, ZhboRtsWeapons, ZhboHud, ZhboInput, ZhboAutopilot, ZhboScreenLook,
** ZhboControlBar, ZhboBuilding) are compiled together with ZhboBridge.cpp, which owns the public
** entry points and the API tables; this header is the only thing they share.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
#pragma once

#include "ZhboBridge.h"
#include "ZhboNames.h"
#include "zhbo_core.h"
#include "ZhboPlatform.h"

// The engine headers every section needs (the original god-file's include list, shared now).
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
#include "Common/TerrainTypes.h"
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

// The glue's configuration (docs/CONFIG.md), read exactly once in Zhbo_Init: ZHBO_AUTOPILOT is
// parsed here once instead of three times with subtly different modes, and the per-frame getenv
// calls are gone. Mode meanings agree with the Rust core's zhbo-core/src/config.rs.
struct ZhboConfig
{
	Bool autopilot_enabled = FALSE;
	Bool autopilot_manual = FALSE;	  // ZHBO_AUTOPILOT=manual: scene only, input stays with the player
	Bool autopilot_equipment = FALSE; // =equipment: the Black Hat / Trophy System scene
	Bool autopilot_showcase = FALSE;
	Bool autopilot_rts = FALSE;
	Bool autopilot_build = FALSE;
	Bool autopilot_classpick = FALSE; // =classpick: the create-a-class editor run (scene like the default scripted run)
	Bool force_radar = FALSE; // ZHBO_FORCE_RADAR: give the test player radar, to film the Tab map
	const char *showcase_object = nullptr; // ZHBO_SHOWCASE_OBJECT: what to stand in front of the camera
	Real showcase_zoom = 0.12f;			   // ZHBO_SHOWCASE_ZOOM, plus:
	Bool has_showcase_pitch = FALSE;	   // ZHBO_SHOWCASE_PITCH (radians)
	Real showcase_pitch = 0.0f;
	Bool has_showcase_angle = FALSE; // ZHBO_SHOWCASE_ANGLE (radians, around the General)
	Real showcase_angle = 0.0f;
	Bool closeup = FALSE;		 // ZHBO_CLOSEUP: follow the General closely in captures
	Real closeup_zoom = 0.0f;
};
extern ZhboConfig s_config;
void readConfig();

// Client state carried between the client frame, the view update and the HUD draw; the tick the
// hitch log reads. Owned by ZhboBridge.cpp.
extern ZhboClientOut s_out;
extern Real s_wheelAccum;
extern double s_lastTick;
// The command bar's pressed class button, waiting for the next client frame: 0 none, else 1-based
// in the class-button rows (ZhboControlBar's `chooseClass` fills it). Same one-shot discipline as
// the client's own class_pick_force latch.
extern Int s_classButtonPick;

// Logging: the debug output and the per-game log file (the tests' oracle).
void logLine(const char *msg);
void logf(const char *fmt, ...);
/// A whole file from the game's file system as text (empty when missing).
std::string readTextFile(const char *path);
/// Write the create-a-class profile file's text (ZhboBridge.cpp); works outside a match.
void writeProfileFile(const char *text);

// One faction row: what each mapping (side name, viewmodel letter, VM code, General object) shares.
struct ZhboFaction
{
	const char *side;  // PlayerTemplate's base side
	char letter;	   // the viewmodel arms variant (ZV%02d%c_SKN)
	const char *code;  // the tablet/hammer model code (ZHBO_VM%s_TAB)
	const char *general; // the General object template
};
const ZhboFaction *zhboFactionOfSide(const AsciiString &side);
const ZhboFaction *zhboFactionOfGeneralName(const AsciiString &name);

// --- Logic API callbacks (ZhboLogicApi.cpp) ---------------------------------------------------
void cbLog(const char *msg);
float cbGroundHeight(float x, float y);
/* The ground's surface at a point (a ZHBO_SURF_*): Zero Hour's terrain class there (R8d). */
int32_t cbSurfaceAt(float x, float y);
/* A point-FX frame: columns X along `normal`, Y to the left, Z up, origin at `at`. One builder for
   the viewmodel's eye frame and R8d's impact orientation (defined in ZhboViewmodel.cpp). */
Matrix3D zhboFrameAt(const Coord3D &at, const Vector3 &normal);
int32_t cbIsWalkable(float x, float y);
void cbExtent(ZhboVec3 *lo, ZhboVec3 *hi);
int32_t cbObject(uint32_t id, uint32_t viewerId, ZhboObjInfo *out);
int32_t cbQuery(const ZhboVec3 *center, float radius, uint32_t viewerId, ZhboObjInfo *out, int32_t cap);
void cbSetPose(uint32_t id, const ZhboVec3 *pos, float yaw, int32_t moving);
void cbSetControlled(uint32_t id, int32_t controlled);
void cbFireFx(uint32_t shooter, const char *fxWeapon_, uint32_t target, const ZhboVec3 *pos);
int32_t cbDamage(uint32_t shooter, uint32_t victimId, float amount, int32_t kind);
uint32_t cbSpawn(const char *thing, uint32_t owner, const ZhboVec3 *pos, float yaw);
void cbDestroy(uint32_t id);
void cbDisable(uint32_t id, int32_t kind, float seconds);
void cbFx(const char *name, const ZhboVec3 *pos);
void cbCapture(uint32_t id, uint32_t newOwner);
void cbSmoke(const ZhboVec3 *at, float radius);
void cbHeal(uint32_t id, float amount);
int32_t cbBuildOptions(uint32_t general, ZhboBuildOption *out, int32_t cap);
int32_t cbCanPlace(uint32_t generalId, uint32_t tmpl, const ZhboVec3 *pos, float angle);
uint32_t cbPlace(uint32_t generalId, uint32_t tmpl, const ZhboVec3 *pos, float angle);
uint32_t cbWorkSite(uint32_t generalId, const ZhboVec3 *pos, float reach, ZhboWorkSite *out);
float cbWork(uint32_t generalId, uint32_t siteId, float rate);
void cbChooseClass(int32_t player, uint32_t generalId, int32_t index);

// --- Client API callbacks (ZhboHud.cpp, ZhboBuilding.cpp) -------------------------------------
void cbScreenSize(float *w, float *h);
void cbFillRect(float x, float y, float w, float h, uint32_t argb);
void cbLine(float x0, float y0, float x1, float y1, float width, uint32_t argb);
void cbText(float x, float y, const char *utf8, uint32_t argb, int32_t size, int32_t align);
int32_t cbImage(const char *name_, float x, float y, float w, float h, uint32_t argb);
int32_t cbClientQuery(const ZhboVec3 *center, float radius, uint32_t viewerId, ZhboObjInfo *out, int32_t cap);
int32_t cbCash(int32_t player);
void cbSound(const char *event, const ZhboVec3 *at);
void cbClip(float x, float y, float w, float h);
int32_t cbRadar(float x, float y, float w, float h, const ZhboVec3 *you, ZhboVec3 *youPx);
int32_t cbClientCanPlace(uint32_t generalId, uint32_t tmpl, const ZhboVec3 *pos, float angle);
void cbGhost(uint32_t tmpl, const ZhboVec3 *pos, float angle, int32_t legal);

// --- ZhboRtsWeapons.cpp -----------------------------------------------------------------------
// MSG_ZHBO_TAKE_CONTROL's optional second argument: what the General is told to do.
enum
{
	ZHBO_ACTION_TAKE_CONTROL = 0,
	// The third slot's orders (zhbo_logic_rts_offhand's choice is the action minus one).
	ZHBO_ACTION_LETHAL = 1,
	ZHBO_ACTION_TACTICAL = 2,
	ZHBO_ACTION_MELEE = 3,
	ZHBO_ACTION_RELOAD = 4,
};
void rtsWeapons(UnsignedInt frame);
Bool isGeneral(const Object *obj);
/// The order a General's command button gives (ZHBO_ACTION_LETHAL/_TACTICAL/_MELEE/_RELOAD), or -1.
Int rtsOrderAction(const CommandButton *button);
/// Tell every client what each selected General is ordered to do (MSG_ZHBO_TAKE_CONTROL).
void sendRtsOrder(Int action);
/// Lockstep: carry out a General's order message (the logic step's MSG_ZHBO_TAKE_CONTROL).
void applyRtsOrder(Int playerIndex, Object *general, Int action);
void drawRtsAmmo();
void applyWorldPoses();

// --- ZhboHud.cpp ------------------------------------------------------------------------------
void evictTexts();
void advanceDrawFrame();

// --- ZhboInput.cpp ----------------------------------------------------------------------------
void setMouseCapture(Bool capture);
void reclaimMouse();
void setHiddenObject(ObjectID id);
void setRtsUiHidden(Bool hidden);
Bool zhboMouseCaptured();
Bool zhboRtsUiHidden();
GameMessageTranslator *zhboCreateTranslator();

// --- ZhboScreenLook.cpp -----------------------------------------------------------------------
void setSkyForced(Bool forced);
void setBo2Look(Bool on);

// --- ZhboControlBar.cpp -----------------------------------------------------------------------
AsciiString zhboBuildSetFor(const Object *general);
void zhboInjectConstructButtons();
// Whether a command-bar button window is on screen: it and every ancestor unhidden (the overlays'
// test; the window's own hidden bit misses the bar's context parent being hidden).
Bool zhboCommandWindowShown(GameWindow *win);
// The command bar's class set (zhbo_core::classicons): one page of presets and the active set's
// customs.
Object *zhboSelectedGeneral();
Int zhboPressedClassButton();
void zhboOpenClassSet(Object *general, Bool forced);
void zhboCloseClassSet(Object *general);
void zhboChooseClassButton(Object *general, Int button);
void zhboUpdateClassBar();
void drawClassButtons();


// --- ZhboAutopilot.cpp ------------------------------------------------------------------------
// The in-game test scene. Single-player only; it spawns objects from the client, which is fine for
// a local test but would desync a network game.
struct Autopilot
{
	Bool enabled;
	// ZHBO_AUTOPILOT=showcase: the General alone, filmed close up in RTS (idle, a walk, death).
	Bool showcase;
	// ZHBO_AUTOPILOT=rts: the General stays under RTS control and is selected; the script picks
	// the lethal, then the secondary (the Assault preset's SMAW), from the command bar, then takes
	// control.
	Bool rts;
	// ZHBO_AUTOPILOT=build: the General alone; the script opens his Build menu, orders a structure
	// and goes back to his commands (his building is logged as it completes). Then in first person
	// he builds another, and repairs it once the script damages it; back overhead, he is ordered
	// to repair it again.
	Bool build;
	Int buildPhase; // BuildPhase
	ObjectID built; // the structure built in first person
	UnsignedInt phaseFrame;
	Real damagedHealth;
	UnsignedInt spawnFrame;
	Bool spawned;
	ObjectID general;
	UnsignedInt releasedFrame;
	Int retakes;
	// Step machines of the scripted tests, so Zhbo_Reset (memset) can restart them in the same
	// process (function-local statics could not).
	UnsignedInt buildStep; // buildAutopilotFrame
	UnsignedInt rtsDone;	 // rtsAutopilotFrame
};
extern Autopilot s_autopilot;
void autopilotFrame();
void autopilotSpawn();
void rtsAutopilotWave(UnsignedInt frame);
void trackGeneralBuilds(UnsignedInt frame);
void trackIdleGenerals();

// Every per-game static belongs to one of these (Zhbo_Reset calls them all).
void ResetFxWeapons();
void ResetRtsWeapons();
void ResetHud();
void ResetBuilding();
void ResetAutopilotTrackers();
void ResetControlBarTrackers();
