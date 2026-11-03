/*
** Conquer Ops - engine-side bridge: the render rate (host patch 0026) and what Zero Hour's graphics
** detail (GameLOD) is doing, in the log.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#define DEFINE_PARTICLE_SYSTEM_NAMES // ParticlePriorityNames, as GameLOD.cpp has it
#include "GameClient/ParticleSys.h"
#include "ZhboBridgeInternal.h"
#include "ZhboPlatform.h"
#include "Common/FramePacer.h"
#include "Common/GameLOD.h"
#include "Common/GlobalData.h"
#include "Common/OptionPreferences.h"
#include "GameClient/Display.h"

// Presentation only: the static detail is per machine and the dynamic one follows the render frame
// rate, so nothing here (or in GameLOD) may reach the sim. GameLOD's two logic-side knobs, the
// debris skip mask and the slow-death scale, are pinned by the data (zhbo-assets' detail tests).
namespace
{
// How often the frame-rate line is logged while a game runs, in seconds.
constexpr double FRAME_RATE_LOG_SECONDS = 5.0;
double s_nextFrameRateLog = 0.0;
Int s_lastDynamicLevel = -2;

const char *staticName(StaticGameLODLevel level)
{
	return level == STATIC_GAME_LOD_UNKNOWN ? "unset" : TheGameLODManager->getStaticGameLODLevelName(level);
}
} // namespace

void zhboLogGraphicsDetail()
{
	if (!TheGameLODManager || !TheGlobalData)
		return;
	// Options.ini as GameLOD read it: the detector's pick and the level the file names.
	OptionPreferences prefs;
	const StaticGameLODLevel ideal = (StaticGameLODLevel)prefs.getIdealStaticGameDetail();
	const StaticGameLODLevel level = TheGameLODManager->getStaticLODLevel();
	const char *source = level == ideal ? "the recommended level" : "the player's choice";
	logf("graphics detail: %s (%s; recommended %s%s), particle cap %d, texture reduction %d, shadows %s, dynamic LOD %s",
		staticName(level), source, staticName(ideal),
		TheGameLODManager->isCpuUnclassified() ? " for a CPU the 2002 presets cannot classify" : "",
		TheGlobalData->m_maxParticleCount, TheGlobalData->m_textureReductionFactor,
		TheGlobalData->m_useShadowVolumes ? "volumes" : TheGlobalData->m_useShadowDecals ? "decals" : "off",
		TheGlobalData->m_enableDynamicLOD ? "on" : "off");
	s_lastDynamicLevel = -2;
	s_nextFrameRateLog = 0.0;
}

void zhboDetailFrame()
{
	if (!TheGameLODManager || !TheDisplay)
		return;
	const Real fps = TheDisplay->getAverageFPS();
	const Int dynamic = (Int)TheGameLODManager->getDynamicLODLevel();
	const UnsignedInt particles = TheParticleSystemManager ? TheParticleSystemManager->getParticleCount() : 0;
	if (dynamic != s_lastDynamicLevel && dynamic != (Int)DYNAMIC_GAME_LOD_UNKNOWN)
	{
		logf("dynamic LOD: %s at %.1f fps (draws particles from %s up)",
			TheGameLODManager->getDynamicGameLODLevelName((DynamicGameLODLevel)dynamic), fps,
			ParticlePriorityNames[TheGameLODManager->getMinDynamicParticlePriority()]);
		s_lastDynamicLevel = dynamic;
	}
	const double now = zhbo_platform::now();
	if (now < s_nextFrameRateLog)
		return;
	s_nextFrameRateLog = now + FRAME_RATE_LOG_SECONDS;
	logf("frame rate: %.1f fps average, dynamic LOD %s, %u of %d particles",
		fps, dynamic == (Int)DYNAMIC_GAME_LOD_UNKNOWN ? "unset" : TheGameLODManager->getDynamicGameLODLevelName((DynamicGameLODLevel)dynamic),
		particles, TheGlobalData->m_maxParticleCount);
}

// ------------------------------------------------------------------------------------------------
// The render rate. Zero Hour capped rendering at its 30 Hz logic rate and ran one logic frame per
// rendered frame (outside network games); a shooter wants the display's rate. The logic stays at the
// game's speed through SuperHackers' logic time scale (FramePacer), so the sim steps the same frames
// at any render rate, and what is drawn between them is interpolated (the first-person eye in the
// core, drawables and W3D animation in host patch 0026, particles by SuperHackers).
// ------------------------------------------------------------------------------------------------
namespace
{
// A display that reports no rate, or less than this, still renders this fast.
constexpr Int MIN_DEFAULT_RENDER_FPS = 60;
Int s_loggedCap = -1;
Int s_loggedLogic = -1;
} // namespace

void Zhbo_PaceNewGame(Int gameLogicFps)
{
	if (!TheFramePacer || !TheWritableGlobalData)
		return;
	const Int logicFps = gameLogicFps > 0 ? gameLogicFps : LOGICFRAMES_PER_SECOND;

	// Options.ini `MaxRenderFPS`: absent, 0 or "Display" follows the display; "Uncapped" removes the
	// cap; a number caps there (never below the logic rate, which would slow the game down).
	OptionPreferences prefs;
	OptionPreferences::const_iterator it = prefs.find("MaxRenderFPS");
	const AsciiString setting = it != prefs.end() ? it->second : AsciiString::TheEmptyString;
	const Int asked = setting.isEmpty() ? 0 : atoi(setting.str());
	const Int display = zhbo_platform::displayRefreshHz();
	Int cap;
	const char *why;
	if (setting.compareNoCase("Uncapped") == 0 || asked < 0)
	{
		cap = RenderFpsPreset::UncappedFpsValue;
		why = "uncapped (Options.ini MaxRenderFPS)";
	}
	else if (asked > 0)
	{
		cap = std::max(asked, logicFps);
		why = "Options.ini MaxRenderFPS";
	}
	else
	{
		cap = std::max(display, MIN_DEFAULT_RENDER_FPS);
		why = "the display's rate";
	}
	TheFramePacer->setFramesPerSecondLimit(cap);
	TheWritableGlobalData->m_useFpsLimit = TRUE;
	TheFramePacer->setLogicTimeScaleFps(logicFps);
	TheFramePacer->enableLogicTimeScale(TRUE);
	TheFramePacer->setGameLogicFps(logicFps);
	if (cap != s_loggedCap || logicFps != s_loggedLogic)
	{
		logf("render rate: up to %d fps (%s; display %d Hz), logic %d fps", cap == (Int)RenderFpsPreset::UncappedFpsValue ? 0 : cap,
			why, display, logicFps);
		s_loggedCap = cap;
		s_loggedLogic = logicFps;
	}
}
