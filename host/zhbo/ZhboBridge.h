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
class CommandButton;
class GameMessage;
class GameMessageTranslator;
class Object;

/// A General's weapon switch button: 0 restricted (no ammo left), 1 available but not lit (the
/// grenade not chosen), -1 Zero Hour decides.
Int Zhbo_WeaponButtonState(const Object *obj, const CommandButton *command);
/// A weapon switch button pressed (before Zero Hour's own switch message): a General's lethal and
/// tactical buttons tell every client which grenade his third slot throws.
void Zhbo_OnSwitchWeaponButton(const CommandButton *command);

/// Startup, after every subsystem and INI file is loaded (end of GameEngine::init).
void Zhbo_Init();
/// New game, loaded game, or back to the shell.
void Zhbo_Reset();
/// Loading screen, after the map's objects exist: load the mod's models now rather than the first
/// time each is shown (GameLogic::startNewGame).
void Zhbo_Preload();

// --- Logic (deterministic lockstep) ------------------------------------------------------------
void Zhbo_OnTakeControlMessage(Int playerIndex, const GameMessage *msg);
void Zhbo_OnInputMessage(Int playerIndex, const GameMessage *msg);
/// Once per logic frame, right after the frame's command list was processed.
void Zhbo_LogicStep(UnsignedInt frame);
/// True while the object is driven by first-person input (its AI and physics must not run).
Bool Zhbo_IsControlled(ObjectID id);

// --- Client (per rendered frame) ---------------------------------------------------------------
/// Before the display/views update (GameClient::update).
void Zhbo_ClientFrame();
/// W3DView::update: returns true and fills the camera when first person owns the camera.
Bool Zhbo_GetCameraOverride(Coord3D *eye, Coord3D *dir, Real *horizontalFov);
/// The first-person viewmodel this frame: clip (ZHBO_VM_*), seconds into it, aim-down-sights 0..1,
/// and the W3D model for the local General's faction. False when none should show.
Bool Zhbo_GetViewmodel(Int *clip, Int *weapon, Bool *fired, Real *time, Real *ads, AsciiString *model, Coord3D *offset, Coord3D *angles);
/// Device side (ZhboViewmodel.cpp): place, pose or hide the viewmodel. W3DView::update calls it
/// every frame with the first-person camera, or with `active` false.
void ZhboViewmodel_Update(Bool active, const Coord3D &eye, const Coord3D &dir);
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

/// A printf-style line in ConquerOps.log.
void Zhbo_Log(const char *fmt, ...);

// --- Data --------------------------------------------------------------------------------------
/// Text for the mod's string labels; null if the label is not one of ours.
const WideChar *Zhbo_FetchString(const char *label);
