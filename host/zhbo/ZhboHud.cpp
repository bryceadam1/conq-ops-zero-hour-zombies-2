/*
** Conquer Ops - engine-side bridge: the Rust core's client (HUD) callbacks: text, fills, images, radar, sound.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

// Precompiled engine headers are force-included by the z_gameengine target.
#include "ZhboBridgeInternal.h"
namespace
{
struct CachedText
{
	std::string text;
	Int size;
	DisplayString *ds;
	UnsignedInt lastUsed;
};
std::vector<CachedText> s_texts;
UnsignedInt s_drawFrame = 0;

// Image-name lookups for the HUD (cbImage), including misses; TheMappedImageCollection resets per
// game, so the pointers go stale and the cache is cleared in Zhbo_Reset.
std::map<std::string, const Image *> s_images;

// Sound events already reported once (cbSound); cleared in Zhbo_Reset.
std::set<std::string> s_soundReported;

} // namespace
const char *hudFontName()
{
	static const char *name = nullptr;
	if (!name)
	{
		name = zhbo_platform::hudFontName();
		logf("HUD font: %s", name);
	}
	return name;
}

DisplayString *cachedText(const char *utf8, Int size)
{
	for (size_t i = 0; i < s_texts.size(); ++i)
	{
		if (s_texts[i].size == size && s_texts[i].text == utf8)
		{
			s_texts[i].lastUsed = s_drawFrame;
			return s_texts[i].ds;
		}
	}
	DisplayString *ds = TheDisplayStringManager->newDisplayString();
	if (!ds)
		return nullptr;
	ds->setFont(TheFontLibrary->getFont(hudFontName(), size, TRUE));
	UnicodeString text;
	text.translate(AsciiString(utf8));
	ds->setText(text);
	CachedText entry = { utf8, size, ds, s_drawFrame };
	s_texts.push_back(entry);
	return ds;
}

void evictTexts()
{
	for (size_t i = 0; i < s_texts.size();)
	{
		if (s_drawFrame - s_texts[i].lastUsed > 120)
		{
			TheDisplayStringManager->freeDisplayString(s_texts[i].ds);
			s_texts[i] = s_texts.back();
			s_texts.pop_back();
		}
		else
		{
			++i;
		}
	}
}

// The HUD's query: as the logic's, plus whether the local player's shroud or fog hides each object.
int32_t cbClientQuery(const ZhboVec3 *center, float radius, uint32_t viewerId, ZhboObjInfo *out, int32_t cap)
{
	const Int n = cbQuery(center, radius, viewerId, out, cap);
	const Int local = rts::getObservedOrLocalPlayer()->getPlayerIndex();
	for (Int i = 0; i < n; ++i)
	{
		const Object *obj = TheGameLogic->findObjectByID((ObjectID)out[i].id);
		const ObjectShroudStatus status = obj ? obj->getShroudedStatus(local) : OBJECTSHROUD_CLEAR;
		if (status == OBJECTSHROUD_FOGGED || status == OBJECTSHROUD_SHROUDED)
			out[i].flags |= ZHBO_OBJ_SHROUDED;
	}
	return n;
}
void cbSound(const char *event, const ZhboVec3 *at)
{
	if (!TheAudio || !event)
		return;
	AudioEventRTS sound((AsciiString(event)));
	if (at)
	{
		Coord3D pos;
		pos.set(at->x, at->y, at->z);
		sound.setPosition(&pos);
	}
	const AudioHandle handle = TheAudio->addAudioEvent(&sound);
	// Report each event once: a handle below AHSV_FirstHandle is an error (unknown event, no sound).
	if (s_soundReported.insert(event).second)
		logf("sound %s: %s", event, handle >= AHSV_FirstHandle ? "playing" : handle == AHSV_Muted ? "muted" : handle == AHSV_NoSound ? "no sound" : "error");
}

void cbScreenSize(float *w, float *h)
{
	*w = (float)TheDisplay->getWidth();
	*h = (float)TheDisplay->getHeight();
}

void cbFillRect(float x, float y, float w, float h, uint32_t argb)
{
	TheDisplay->drawFillRect((Int)x, (Int)y, (Int)(w + 0.5f), (Int)(h + 0.5f), argb);
}

void cbLine(float x0, float y0, float x1, float y1, float width, uint32_t argb)
{
	TheDisplay->drawLine((Int)x0, (Int)y0, (Int)x1, (Int)y1, width, argb);
}
void cbText(float x, float y, const char *utf8, uint32_t argb, int32_t size, int32_t align)
{
	DisplayString *ds = cachedText(utf8, size < 6 ? 6 : size);
	if (!ds)
		return;
	Int w = ds->getWidth();
	Int px = (Int)x;
	if (align == 1) px -= w / 2;
	else if (align == 2) px -= w;
	// Half the alpha as the shadow's alpha, computed on the byte so an odd alpha's low bit cannot
	// bleed into the red channel.
	const UnsignedInt shadow = (((argb >> 24) >> 1) & 0xFFu) << 24;
	ds->draw(px, (Int)y, argb, shadow);
}

int32_t cbImage(const char *name_, float x, float y, float w, float h, uint32_t argb)
{
	// "ZHBO_TeamLogo" is the local player's faction emblem from Zero Hour's own art (USA_Logo,
	// China_Logo, GLA_Logo), by the base side of their general.
	std::string name = name_;
	if (name == "ZHBO_TeamLogo")
	{
		const Player *local = ThePlayerList ? ThePlayerList->getLocalPlayer() : nullptr;
		const PlayerTemplate *pt = local ? local->getPlayerTemplate() : nullptr;
		if (!pt || pt->getBaseSide().isEmpty())
			return 0;
		name = std::string(pt->getBaseSide().str()) + "_Logo";
	}
	// Mapped images are reset per game (TheMappedImageCollection), so the cache must not outlive
	// one: it is cleared in Zhbo_Reset (s_images), including misses.
	std::map<std::string, const Image *>::iterator it = s_images.find(name);
	const Image *image;
	if (it == s_images.end())
	{
		image = TheMappedImageCollection ? TheMappedImageCollection->findImageByName(name.c_str()) : nullptr;
		s_images[name] = image;
	}
	else
	{
		image = it->second;
	}
	if (!image)
		return 0;
	TheDisplay->drawImage(image, (Int)x, (Int)y, (Int)(x + w + 0.5f), (Int)(y + h + 0.5f), argb);
	return 1;
}

void cbClip(float x, float y, float w, float h)
{
	if (!TheDisplay)
		return;
	if (w <= 0.0f || h <= 0.0f)
	{
		TheDisplay->enableClipping(FALSE);
		return;
	}
	IRegion2D region;
	region.lo.x = (Int)x;
	region.lo.y = (Int)y;
	region.hi.x = (Int)(x + w + 0.5f);
	region.hi.y = (Int)(y + h + 0.5f);
	TheDisplay->setClipRegion(&region);
	TheDisplay->enableClipping(TRUE);
}

int32_t cbRadar(float x, float y, float w, float h, const ZhboVec3 *you, ZhboVec3 *youPx)
{
	// The same rule as the control bar's radar window (W3DLeftHUDDraw).
	if (!TheRadar || !rts::localPlayerHasRadar())
		return 0;
	// No tactical-view box: in first person it collapses to a stray line (patch 0005).
	TheRadar->setDrawViewBox(FALSE);
	TheRadar->draw((Int)x, (Int)y, (Int)w, (Int)h);
	TheRadar->setDrawViewBox(TRUE);
	// Where `you` lands: radar cells inside the aspect-preserving square the radar drew in.
	ICoord2D ul, lr, cell;
	TheRadar->findDrawPositions((Int)x, (Int)y, (Int)w, (Int)h, &ul, &lr);
	Coord3D world;
	world.set(you->x, you->y, you->z);
	if (youPx && TheRadar->worldToRadar(&world, &cell))
	{
		youPx->x = ul.x + (cell.x + 0.5f) * (lr.x - ul.x) / (Real)RADAR_CELL_WIDTH;
		youPx->y = lr.y - (cell.y + 0.5f) * (lr.y - ul.y) / (Real)RADAR_CELL_HEIGHT;
	}
	return 1;
}

int32_t cbCash(int32_t player)
{
	Player *p = ThePlayerList ? ThePlayerList->getNthPlayer(player) : nullptr;
	return p ? (int32_t)p->getMoney()->countMoney() : 0;
}

void ResetHud()
{
	// Mapped images are reset per game (TheMappedImageCollection): the cached pointers go
	// stale, and the misses too. The text cache survives (DisplayStrings do not reset).
	s_images.clear();
	s_soundReported.clear();
}

void advanceDrawFrame()
{
	++s_drawFrame;
}
