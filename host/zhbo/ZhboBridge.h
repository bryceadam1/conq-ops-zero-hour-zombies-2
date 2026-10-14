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

class GameMessage;
class GameMessageTranslator;
class Object;

/// Startup, after every subsystem and INI file is loaded (end of GameEngine::init).
void Zhbo_Init();
/// New game, loaded game, or back to the shell.
void Zhbo_Reset();

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
/// After the in-game UI is drawn.
void Zhbo_DrawHud();
/// Before the 3D views are drawn: paints a sky gradient while first person owns the camera.
void Zhbo_DrawSky();
/// Swallows mouse/keyboard input while first person owns it. Attach before every other translator.
GameMessageTranslator *Zhbo_CreateTranslator();
/// "Take Control" command button pressed with `obj` selected.
void Zhbo_RequestTakeControl(Object *obj);

// --- Data --------------------------------------------------------------------------------------
/// Text for the mod's string labels; null if the label is not one of ours.
const WideChar *Zhbo_FetchString(const char *label);
