/*
** Conquer Ops - engine-side bridge between Zero Hour (TheSuperHackers GeneralsGameCode) and the
** Rust core. Every engine hook added by host/patches calls one of these functions.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
#pragma once

#include "Lib/BaseType.h"
#include "Common/GameType.h"

class AsciiString;
class UnicodeString;
class CommandButton;
class GameMessage;
class GameMessageTranslator;
class GameWindow;
enum NameKeyType CPP_11(: Int); // Common/STLTypedefs.h
class Object;
class DamageInfo;
class Path;
class Xfer;

#include <string>
/// A whole file from the game's file system as text (empty when missing). Shared by the glue's
/// section files (definition in ZhboBridge.cpp).
std::string readTextFile(const char *path);

/// A General's weapon switch or order button: 0 restricted (no ammo left, or an order he can't
/// carry out), 1 available, -1 Zero Hour decides.
Int Zhbo_WeaponButtonState(const Object *obj, const CommandButton *command);
/// A weapon switch button pressed (before Zero Hour's own switch message). The General's switch
/// buttons are his guns only (patch 0010's hook; his offhands are orders now), so nothing to add.
void Zhbo_OnSwitchWeaponButton(const CommandButton *command);
/// A FIRE_WEAPON order given (patch 0016, before Zero Hour's own weapon message): a General's
/// Lethal, Tactical and Melee orders tell every client what his third slot does first.
void Zhbo_OnFireWeaponCommand(const CommandButton *command);

/// Startup, after every subsystem and INI file is loaded (end of GameEngine::init).
void Zhbo_Init();
/// New game, loaded game, or back to the shell.
void Zhbo_Reset();

/// InGameUI::recreateControlBar built a new ControlBar from the INI (host patch 0023): re-add the
/// mod's buttons and command sets.
void Zhbo_ControlBarRebuilt();
/// Loading screen, after the map's objects exist: load the mod's models now rather than the first
/// time each is shown (GameLogic::startNewGame).
void Zhbo_Preload();

// --- Logic (deterministic lockstep) ------------------------------------------------------------
void Zhbo_OnTakeControlMessage(Int playerIndex, const GameMessage *msg);
void Zhbo_OnInputMessage(Int playerIndex, const GameMessage *msg);
/// The pick menu's choice: the General assumes the class at the shared list's index (classes.rs).
void Zhbo_OnChooseClassMessage(Int playerIndex, const GameMessage *msg);
/// Once per logic frame, right after the frame's command list was processed.
void Zhbo_LogicStep(UnsignedInt frame);
/// True while the object is driven by first-person input (its AI and physics must not run).
Bool Zhbo_IsControlled(ObjectID id);
/// The first-person sim's state in Zero Hour's logic CRC (patch 0020, GameLogic::getCRC after
/// MARKER:TheAI): `MARKER:ConquerOps` and the sim's 64-bit state hash, when the sim holds state.
/// `logicFrame` (the logic update's own CRC) also logs the per-seat parts.
void Zhbo_XferLogicCRC(Xfer *xfer, Bool logicFrame);
/// The Assault Shield in the damage path (patch 0017, top of Object::attemptDamage): scales the
/// damage a held/stowed shield stops or a deployed shield's object takes (the core wears that down).
void Zhbo_ShieldDamage(Object *obj, DamageInfo *info);
/// The smoke screen (patch 0024): whether one hides `victim` from `source` (the line between
/// their centres passes through it), so `source` can neither acquire nor keep attacking it
/// (WeaponSet::getAbleToAttackSpecificObject).
Bool Zhbo_SmokeHides(const Object *source, const Object *victim);
/// The pathfinder's extra cost for the cell at (cellX, cellY) while a screen stands over it
/// (Pathfinder::examineNeighboringCells and findGroundPath): high, not blocking, so a unit inside
/// can still leave and a goal inside can still be reached.
Int Zhbo_SmokePathCost(Int cellX, Int cellY);
/// Whether `path` (computed on frame `pathFrame`) passes under a screen that went off since: the
/// move recomputes it once (AIInternalMoveToState::update).
Bool Zhbo_SmokeCrossesPath(Path *path, UnsignedInt pathFrame);

/// Force the in-game Create-a-Class pick screen open (the command bar's "Create a Class" entry;
/// the at-spawn pick needs no forcing). One-shot latch consumed by the next client frame. The
/// lobby's button does NOT use this--it opens the shell screen (Zhbo_OpenClassMenu) instead.
void Zhbo_ShowClassPick();

// --- The shell create-a-class screen (the lobby's class menu) -----------------------------------
/// Open the shell class screen: a full-screen modal window whose draw callback runs the Rust
/// core's menu frame (ZhboMenu.cpp). Called by the skirmish lobby's Create a Class button
/// (patch 0015); a no-op while the screen is already up.
void Zhbo_OpenClassMenu();
/// Close the shell class screen (its window and the core's menu state); also called from
/// Zhbo_Reset so the screen never leaks into a match. A no-op while it is not up.
void Zhbo_CloseClassMenu();
/// Whether the shell class screen is up.
Bool Zhbo_ClassMenuOpen();
/// The game setup menus' Create a Class button (patch 0018: the Skirmish, LAN and online
/// lobbies; a click calls Zhbo_OpenClassMenu). Its window id in menu `wnd`
/// ("LanGameOptionsMenu.wnd"): `<wnd>:ButtonCreateClass`, the id a shipped element would carry.
NameKeyType Zhbo_CreateClassButtonID(const char *wnd);
/// Find or build the button on `menuParent`'s layout (the mod ships no .wnd): the Start button's
/// size and font, in the top-right corner of the panel `panelName`, fitted above the title line
/// `titleLineName` (both child names in `wnd`; null when the menu has none). Call it from the
/// menu's Init on every visit: the layout's windows die with the menu.
GameWindow *Zhbo_EnsureCreateClassButton(GameWindow *menuParent, const char *wnd, const char *panelName, const char *titleLineName, GameWindow *startButton);
/// One menu frame: fills ZhboMenuIn, calls zhbo_menu_frame, writes the profile one-shot, closes
/// on request. Called by the shell window's draw callback (ZhboMenu.cpp).
void Zhbo_MenuFrame();

// --- Client (per rendered frame) ---------------------------------------------------------------
/// Before the display/views update (GameClient::update).
void Zhbo_ClientFrame();
/// W3DView::update: returns true and fills the camera when first person owns the camera.
/// `nearPlane` is the viewmodel's near: the gun's rear sits at the eye, so the world's near
/// (NearZ = MAP_XY_FACTOR = 10 in W3DView.cpp) clips it into jagged holes. The first-person view
/// takes 0.1 units instead (the native pass's projection value; BO2's r_znear 4" is ~0.67 units),
/// which leaves the world's near for the world when the RTS view takes over.
Bool Zhbo_GetCameraOverride(Coord3D *eye, Coord3D *dir, Real *horizontalFov, Real *nearPlane, Real *roll);
/// The first-person viewmodel this frame: clip (ZHBO_VM_*), seconds into it, aim-down-sights 0..1,
/// the W3D model for the local General's faction, and that faction's letter (`U`, `C`, `G`: the
/// native manifest's suffix — never read it back out of `model`, whose weapon number has any
/// width). False when none should show.
Bool Zhbo_GetViewmodel(Int *clip, Int *weapon, Bool *fired, Bool *firedLeft, Real *time, Real *ads, AsciiString *model, char *faction, Coord3D *offset, Coord3D *angles);
/// Device side (ZhboViewmodel.cpp): place, pose or hide the viewmodel. W3DView::update calls it
/// every frame with the first-person camera, or with `active` false.
void ZhboViewmodel_Update(Bool active, const Coord3D &eye, const Coord3D &dir);
/// Drop the viewmodel's model, clips and per-game caches (Zhbo_Reset: the scene and assets are
/// torn down per game).
void ZhboViewmodel_Reset();
/// Load the tablet and these weapon viewmodels of a faction ('U', 'C', 'G'), and their clips.
void ZhboViewmodel_Preload(char faction, const int *weapons, int count);
/// After the in-game UI is drawn.
void Zhbo_DrawHud();
/// Before the 3D views are drawn: paints a sky gradient while first person owns the camera.
void Zhbo_DrawSky();
/// Swallows mouse/keyboard input while first person owns it. Attach before every other translator.
GameMessageTranslator *Zhbo_CreateTranslator();
/// "Take Control" command button pressed with `obj` selected.
void Zhbo_RequestTakeControl(Object *obj);
/// One of the General's own command buttons (Command = ZHBO_TAKE_CONTROL: Take Control, the Build
/// menu and Back from it) pressed with `obj` selected.
void Zhbo_OnZhboCommand(Object *obj, const CommandButton *button);
/// How fast `builder` builds and repairs, as a share of a dozer's speed (DozerAIUpdate, host patch 0012).
Real Zhbo_WorkRate(const Object *builder);
/// The command set a Conquer Ops General builds from (his side's worker menu), or empty for any
/// other builder. The logic's construction check reads it (BuildAssistant::isPossibleToMakeUnit,
/// host patch 0019) rather than the General's current command set: the Build menu is a UI-only
/// override on the machine whose player opened it, so reading that diverged between peers.
AsciiString Zhbo_BuildCommandSet(const Object *builder);

/// A printf-style line in ConquerOps.log.
void Zhbo_Log(const char *fmt, ...);

// --- Multiplayer content check (content.rs; host patches 0018, 0021) --------------------------
/// This machine's content fingerprint parts (ZHBO_CONTENT_PARTS of them; `n` words written, zero
/// past them or before Zhbo_Init): a LAN join request and a refusal carry them.
void Zhbo_ContentParts(UnsignedInt *out, Int n);
/// "Conquer Ops content 1A2B3C4D": the lobby line (empty before Zhbo_Init).
UnicodeString Zhbo_ContentLabel();
/// The refused-join message naming the parts that differ from `theirs` (`n` words; logged too).
UnicodeString Zhbo_ContentMismatch(const UnsignedInt *theirs, Int n);
/// Keep the host's parts from a LAN refusal for the join-failed box (Zhbo_RefusedContentText).
void Zhbo_NoteRefusedContent(const UnsignedInt *theirs, Int n);
UnicodeString Zhbo_RefusedContentText();
/// Online: the staging room carries only the total; both totals, short form (logged too).
UnicodeString Zhbo_ContentVersus(UnsignedInt theirTotal);

// --- Data --------------------------------------------------------------------------------------
/// Text for the mod's string labels; null if the label is not one of ours.
const WideChar *Zhbo_FetchString(const char *label);
