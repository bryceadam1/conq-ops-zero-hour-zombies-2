/*
** Conquer Ops - engine-side bridge: what Zero Hour's graphics detail (GameLOD) is doing, in the log.
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
