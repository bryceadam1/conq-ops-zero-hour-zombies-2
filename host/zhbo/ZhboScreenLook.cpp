/*
** Conquer Ops - engine-side bridge: the sky and BO2-grade screen look while first person owns the camera.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
// The RTS camera never sees the horizon, so maps normally leave the skybox off. Turn it on while
// first person owns the camera and restore the map's setting afterwards.
namespace
{
Bool s_skyForced = FALSE;
Real s_skyWas = 0.0f;
} // namespace

void setSkyForced(Bool forced)
{
	if (forced == s_skyForced || !TheWritableGlobalData)
		return;
	s_skyForced = forced;
	if (forced)
	{
		s_skyWas = TheWritableGlobalData->m_drawSkyBox;
		TheWritableGlobalData->m_drawSkyBox = TRUE;
	}
	else
	{
		TheWritableGlobalData->m_drawSkyBox = s_skyWas;
	}
}

namespace
{
struct ScreenLook
{
	Bool bloom, grade, vignette;
	Real bloomThreshold, bloomIntensity, gradeStrength, gradeTemperature, gradeTint, vignetteStrength;
};
Bool s_bo2Look = FALSE;
ScreenLook s_lookWas;

} // namespace

void setBo2Look(Bool on)
{
	if (on == s_bo2Look || !TheWritableGlobalData)
		return;
	s_bo2Look = on;
	GlobalData *gd = TheWritableGlobalData;
	if (on)
	{
		s_lookWas = { gd->m_bgfxBloom, gd->m_bgfxColorGrade, gd->m_bgfxVignette, gd->m_bgfxBloomThreshold, gd->m_bgfxBloomIntensity,
			gd->m_bgfxColorGradeStrength, gd->m_bgfxColorGradeTemperature, gd->m_bgfxColorGradeTint, gd->m_bgfxVignetteStrength };
		gd->m_bgfxBloom = TRUE;
		gd->m_bgfxBloomThreshold = 0.8f;
		gd->m_bgfxBloomIntensity = 0.35f;
		gd->m_bgfxColorGrade = TRUE;
		gd->m_bgfxColorGradeStrength = 0.45f;
		gd->m_bgfxColorGradeTemperature = -0.15f;
		gd->m_bgfxColorGradeTint = 0.0f;
		gd->m_bgfxVignette = TRUE;
		gd->m_bgfxVignetteStrength = 0.22f;
	}
	else
	{
		gd->m_bgfxBloom = s_lookWas.bloom;
		gd->m_bgfxColorGrade = s_lookWas.grade;
		gd->m_bgfxVignette = s_lookWas.vignette;
		gd->m_bgfxBloomThreshold = s_lookWas.bloomThreshold;
		gd->m_bgfxBloomIntensity = s_lookWas.bloomIntensity;
		gd->m_bgfxColorGradeStrength = s_lookWas.gradeStrength;
		gd->m_bgfxColorGradeTemperature = s_lookWas.gradeTemperature;
		gd->m_bgfxColorGradeTint = s_lookWas.gradeTint;
		gd->m_bgfxVignetteStrength = s_lookWas.vignetteStrength;
	}
}
