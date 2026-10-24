// Conquer Ops: the first-person viewmodel (BO2 arms holding the weapon), drawn in the 3D scene.
//
// The model and its clips are converted from the player's BO2 install into ConquerOps_BO2.big
// (zhbo_assets::generals). zhbo-core picks the clip and the time into it each frame; this places the
// model at the eye, facing the view, and poses it. Aiming down the sights slides the model sideways
// and down until the weapon's tag_sights bone sits on the line of sight. Built into
// z_gameenginedevice; W3DView::update calls ZhboViewmodel_Update (host/patches/0001).
#include "ZhboBridge.h"
#include "ZhboBridgeInternal.h"
#include "ZhboNames.h"
#include "zhbo_core.h"
#include "ZhboPlatform.h"

#include "Common/AudioEventRTS.h"
#include "Common/File.h"
#include "Common/FileSystem.h"
#include "Common/GameAudio.h"
#include "Common/GlobalData.h"
#include "GameClient/FXList.h"
#include "W3DDevice/GameClient/W3DDisplay.h"
#include "W3DDevice/GameClient/W3DScene.h"
#include "WW3D2/assetmgr.h"
#include "WW3D2/hanim.h"
#include "WW3D2/rendobj.h"

#include <algorithm>
#include <cmath>
#include <map>
#include <sstream>
#include <string>
#include <vector>

namespace
{
	// The tablet's clips (zhbo_assets::generals::TABLET_ANIMS).
	const char *const k_tablet_clips[3] = {
		"ZHBO_VMT_SKL.ZHBO_VMT_PUL", // ZHBO_VM_TABLET_PULLOUT
		"ZHBO_VMT_SKL.ZHBO_VMT_IDL", // ZHBO_VM_TABLET_IDLE
		"ZHBO_VMT_SKL.ZHBO_VMT_PUT", // ZHBO_VM_TABLET_PUTAWAY
	};
	// The hammer's (zhbo_assets::generals::HAMMER_ANIMS).
	const char *const k_hammer_clips[4] = {
		"ZHBO_VMH_SKL.ZHBO_VMH_PUL", // ZHBO_VM_HAMMER_PULLOUT
		"ZHBO_VMH_SKL.ZHBO_VMH_IDL", // ZHBO_VM_HAMMER_IDLE
		"ZHBO_VMH_SKL.ZHBO_VMH_SWG", // ZHBO_VM_HAMMER_SWING
		"ZHBO_VMH_SKL.ZHBO_VMH_PUT", // ZHBO_VM_HAMMER_PUTAWAY
	};

	// Clips that loop (zhbo_core::viewmodel::slot::loops), an alt mode's too.
	Bool loops(int clip)
	{
		if (clip == ZHBO_VM_TABLET_IDLE || clip == ZHBO_VM_HAMMER_IDLE)
			return TRUE;
		const int slot = clip >= ZHBO_VM_ALT_CLIPS ? clip - ZHBO_VM_ALT_CLIPS : clip;
		return slot == ZHBO_VM_SLOT_IDLE || slot == ZHBO_VM_SLOT_EMPTY_IDLE || slot == ZHBO_VM_SLOT_SPRINT_LOOP
			|| slot == ZHBO_VM_SLOT_SPRINT_EMPTY_LOOP;
	}

	RenderObjClass *s_model = nullptr;
	AsciiString s_modelName;
	// The loaded model's clips by clip id (BO2 slot or tablet clip).
	std::map<int, HAnimClass *> s_anims;
	AsciiString s_missing; // a model that failed to load; not retried

	// Sound notetracks of the clips (zhbo_assets::sfx): played as the clip passes their time.
	struct Note
	{
		int weapon;
		int clip;
		float time;
		AsciiString event;
	};
	std::vector<Note> s_notes;
	Bool s_notesLoaded = FALSE;
	int s_lastClip = -1;
	float s_lastTime = 0.0f;

	void loadNotes()
	{
		s_notesLoaded = TRUE;
		const std::string text = readTextFile(zhbo_names::VIEWMODEL_SOUNDS_PATH);
		std::istringstream in(text);
		std::string line;
		while (std::getline(in, line))
		{
			if (line.empty() || line[0] == ';')
				continue;
			std::istringstream fields(line);
			Note n;
			std::string event;
			if (fields >> n.weapon >> n.clip >> n.time >> event)
			{
				n.event = event.c_str();
				s_notes.push_back(n);
			}
		}
		Zhbo_Log("viewmodel: %d sound notetracks", (int)s_notes.size());
	}

	int s_lastWeapon = -2;

	// Play the notes of the weapon's `clip` in (from, to]; a clip that (re)started plays from its
	// beginning.
	void playNotes(int weapon, int clip, float time)
	{
		const Bool restarted = clip != s_lastClip || weapon != s_lastWeapon || time < s_lastTime;
		const float from = restarted ? -1.0f : s_lastTime;
		for (const Note &n : s_notes)
		{
			if (n.weapon == weapon && n.clip == clip && n.time > from && n.time <= time && TheAudio)
			{
				AudioEventRTS sound(n.event);
				TheAudio->addAudioEvent(&sound);
			}
		}
		s_lastClip = clip;
		s_lastWeapon = weapon;
		s_lastTime = time;
	}

	// An FXList at a viewmodel bone, oriented with it.
	void boneFx(const char *fxName, const char *bone)
	{
		const int index = s_model->Get_Bone_Index(bone);
		const FXList *fx = TheFXListStore ? TheFXListStore->findFXList(fxName) : nullptr;
		if (index <= 0 || !fx)
			return;
		const Matrix3D &m = s_model->Get_Bone_Transform(index);
		const Vector3 t = m.Get_Translation();
		Coord3D pos;
		pos.set(t.X, t.Y, t.Z);
		FXList::doFXPos(fx, &pos, &m);
	}

	// The clips of viewmodel `name`: the tablet's, or weapon `weapon`'s. Each weapon and the tablet
	// have their own hierarchy, so only this model's.
	void loadClips(WW3DAssetManager *assets, const AsciiString &name, int weapon, std::map<int, HAnimClass *> &out)
	{
		if (name.endsWith("_TAB"))
		{
			for (int i = 0; i < 3; ++i)
				if (HAnimClass *anim = assets->Get_HAnim(k_tablet_clips[i]))
					out[ZHBO_VM_TABLET_PULLOUT + i] = anim;
		}
		else if (name.endsWith("_HAM"))
		{
			for (int i = 0; i < 4; ++i)
				if (HAnimClass *anim = assets->Get_HAnim(k_hammer_clips[i]))
					out[ZHBO_VM_HAMMER_PULLOUT + i] = anim;
		}
		else if (weapon >= 0)
		{
			// zhbo_assets::generals::vm_anim: one clip per BO2 animation slot the weapon has, then its
			// alt mode's (select fire, grenade launcher) after ZHBO_VM_ALT_CLIPS.
			for (int base = 0; base <= ZHBO_VM_ALT_CLIPS; base += ZHBO_VM_ALT_CLIPS)
			{
				for (int slot = 0; slot < ZHBO_VM_SLOT_COUNT; ++slot)
				{
					char clip[64];
					snprintf(clip, sizeof(clip), "ZV%02d_SKL.ZV%02d_A%02X", weapon, weapon, base + slot);
					if (HAnimClass *anim = assets->Get_HAnim(clip))
						out[base + slot] = anim;
				}
			}
		}
	}

	void release()
	{
		for (auto &a : s_anims)
		{
			REF_PTR_RELEASE(a.second);
		}
		s_anims.clear();
		if (s_model)
		{
			s_model->Remove();
			REF_PTR_RELEASE(s_model);
		}
		s_modelName.clear();
	}

	Bool load(const AsciiString &name, int weapon)
	{
		release();
		const double started = zhbo_platform::now();
		WW3DAssetManager *assets = WW3DAssetManager::Get_Instance();
		s_model = assets ? assets->Create_Render_Obj(name.str()) : nullptr;
		if (!s_model || !W3DDisplay::m_3DScene)
		{
			REF_PTR_RELEASE(s_model);
			s_missing = name;
			Zhbo_Log("viewmodel: no model %s (BO2 art not converted?)", name.str());
			return FALSE;
		}
		W3DDisplay::m_3DScene->Add_Render_Object(s_model);
		loadClips(assets, name, weapon, s_anims);
		Zhbo_Log("viewmodel: %d clips for %s", (int)s_anims.size(), name.str());
		s_modelName = name;
		Zhbo_Log("viewmodel: loaded %s in %.0f ms", name.str(), (zhbo_platform::now() - started) * 1000.0);
		return TRUE;
	}

	// --- The native BO2 pass (Track R; zhbo-core nativevm, opt-in ZHBO_NATIVE_VIEWMODEL=1) -------

	// BgfxBackend's view ids: the scene the viewmodel draws into (kBgfxEngineView) and the composite
	// it draws before (kBgfxSceneCompositeView), so bloom and the tonemap see it.
	const uint16_t k_scene_view = 1;
	const uint16_t k_composite_view = 9;

	// An archive file's bytes; FALSE when there is no such file.
	Bool readArchiveFile(const char *path, std::string &out)
	{
		File *f = TheFileSystem ? TheFileSystem->openFile(path, File::READ | File::BINARY) : nullptr;
		if (!f)
			return FALSE;
		const Int size = f->size();
		char *buf = f->readEntireAndClose();
		if (!buf)
			return FALSE;
		out.assign(buf, size);
		delete[] buf;
		return TRUE;
	}

	// Answer the core's file requests and print its log lines.
	void serveNative()
	{
		std::vector<std::string> wanted;
		char path[512];
		for (int i = 0; zhbo_native_wanted(i, path, sizeof(path)) >= 0; ++i)
			wanted.push_back(path);
		for (const std::string &p : wanted)
		{
			std::string bytes;
			if (readArchiveFile(p.c_str(), bytes))
				zhbo_native_provide(p.c_str(), reinterpret_cast<const uint8_t *>(bytes.data()), (int32_t)bytes.size());
			else
				zhbo_native_provide(p.c_str(), nullptr, -1);
		}
		char line[512];
		while (zhbo_native_take_log(line, sizeof(line)) >= 0)
			Zhbo_Log("%s", line);
	}

	// An FXList at a bone of the native viewmodel's last pose.
	void nativeBoneFx(const char *fxName, const char *bone)
	{
		float m[12];
		const FXList *fx = TheFXListStore ? TheFXListStore->findFXList(fxName) : nullptr;
		if (!fx || !zhbo_native_bone(bone, m))
			return;
		Matrix3D mtx(true);
		mtx[0].Set(m[0], m[1], m[2], m[3]);
		mtx[1].Set(m[4], m[5], m[6], m[7]);
		mtx[2].Set(m[8], m[9], m[10], m[11]);
		Coord3D pos;
		pos.set(m[3], m[7], m[11]);
		FXList::doFXPos(fx, &pos, &mtx);
	}

	// The weapon's own BO2 first-person effects (Track R8) at its flash and brass tags. Brass is the
	// def's alone: BO2 ejects a shell only when `viewShellEjectEffect` is set (41 of 172 MP defs; the
	// AN-94 and every other AR and sniper define none), so only the muzzle flash has a fallback.
	// `tag_brass1` is the `_lh` model's left port, present only on a dual-wield pair, so a single
	// gun's second call is a no-op.
	void viewmodelFx(int weapon, Bool native)
	{
		char flash[128] = "";
		char shell[128] = "";
		zhbo_viewmodel_fx(weapon, flash, static_cast<int32_t>(sizeof(flash)), shell, static_cast<int32_t>(sizeof(shell)));
		const char *flashFx = flash[0] ? flash : "ZHBO_ViewmodelMuzzle";
		if (native)
		{
			nativeBoneFx(flashFx, "tag_flash");
			if (shell[0])
			{
				nativeBoneFx(shell, "tag_brass");
				nativeBoneFx(shell, "tag_brass1");
			}
		}
		else
		{
			boneFx(flashFx, "tag_flash");
			if (shell[0])
			{
				boneFx(shell, "tag_brass");
				boneFx(shell, "tag_brass1");
			}
		}
	}

	// The scene's lights as the native pass takes them: Zero Hour's global lights for the time of
	// day (`lightPos` is the direction the light travels, so the light is the other way).
	void sceneLights(ZhboNativeIn &in)
	{
		if (!TheGlobalData)
			return;
		const int n = std::min(std::max(TheGlobalData->m_numGlobalLights, 0), 3);
		for (int i = 0; i < n; ++i)
		{
			const GlobalData::TerrainLighting &l = TheGlobalData->m_terrainObjectsLighting[TheGlobalData->m_timeOfDay][i];
			in.lights[i].dir[0] = -l.lightPos.x;
			in.lights[i].dir[1] = -l.lightPos.y;
			in.lights[i].dir[2] = -l.lightPos.z;
			in.lights[i].color[0] = l.diffuse.red;
			in.lights[i].color[1] = l.diffuse.green;
			in.lights[i].color[2] = l.diffuse.blue;
			in.ambient[0] += l.ambient.red;
			in.ambient[1] += l.ambient.green;
			in.ambient[2] += l.ambient.blue;
		}
		in.light_count = n;
	}

	// Draw the weapon through the native pass; TRUE when it did (the W3D model stays hidden).
	Bool nativeViewmodel(const Coord3D &eye, const Coord3D &dir, Int clip, Int weapon, const AsciiString &name, Real time, Real ads,
		const Coord3D &offset, const Coord3D &angles, Bool fired)
	{
		if (weapon < 0 || name.getLength() < 5 || !name.startsWith("ZV"))
			return FALSE;
		static const float s_debug = getenv("ZHBO_BO2_DEBUG") ? static_cast<float>(atoi(getenv("ZHBO_BO2_DEBUG"))) : 0.0f;
		ZhboNativeIn in = {};
		in.eye[0] = eye.x; in.eye[1] = eye.y; in.eye[2] = eye.z;
		in.dir[0] = dir.x; in.dir[1] = dir.y; in.dir[2] = dir.z;
		in.offset[0] = offset.x; in.offset[1] = offset.y; in.offset[2] = offset.z;
		in.angles[0] = angles.x; in.angles[1] = angles.y; in.angles[2] = angles.z;
		in.clip = clip;
		in.vm = weapon;
		in.faction = name.getCharAt(4);
		in.time = time;
		in.ads = ads;
		in.rate = 1.0f;
		sceneLights(in);
		in.target_view = k_scene_view;
		in.before_view = k_composite_view;
		in.debug = s_debug;
		const int32_t result = zhbo_native_viewmodel(&in);
		serveNative();
		if (result != ZHBO_NATIVE_DRAWN)
			return FALSE;
		if (s_model)
			s_model->Set_Hidden(true);
		if (fired)
			viewmodelFx(weapon, TRUE);
		return TRUE;
	}

	// Columns: X forward along `dir`, Y to the left, Z up; origin at the eye.
	Matrix3D viewTransform(const Coord3D &eye, const Coord3D &dir, Vector3 &x, Vector3 &y, Vector3 &z)
	{
		x.Set(dir.x, dir.y, dir.z);
		x.Normalize();
		Vector3::Cross_Product(Vector3(0.0f, 0.0f, 1.0f), x, &y);
		if (y.Length2() < 1e-6f)
			y.Set(0.0f, 1.0f, 0.0f);
		y.Normalize();
		Vector3::Cross_Product(x, y, &z);
		return zhboFrameAt(eye, x);
	}
}

// Columns: X along `normal`, Y to the left, Z up; origin at `at`. The one frame builder for point
// FX: the viewmodel's eye frame and R8d's impact orientation.
Matrix3D zhboFrameAt(const Coord3D &at, const Vector3 &normal)
{
	Vector3 x = normal;
	if (x.Length2() < 1e-6f)
		x.Set(1.0f, 0.0f, 0.0f);
	x.Normalize();
	Vector3 y, z;
	Vector3::Cross_Product(Vector3(0.0f, 0.0f, 1.0f), x, &y);
	if (y.Length2() < 1e-6f)
		y.Set(0.0f, 1.0f, 0.0f);
	y.Normalize();
	Vector3::Cross_Product(x, y, &z);
	Matrix3D m(true);
	m[0].Set(x.X, y.X, z.X, at.x);
	m[1].Set(x.Y, y.Y, z.Y, at.y);
	m[2].Set(x.Z, y.Z, z.Z, at.z);
	return m;
}

// A game reset tears down the 3D scene (W3DScene) and reloads assets, so the held model and clip
// references go stale: drop them and the per-game caches (without this, the model would keep
// rendering over the shell and the notes list would survive into the next game).
void ZhboViewmodel_Reset()
{
	release();
	s_missing.clear();
	s_notes.clear();
	s_notesLoaded = FALSE;
	s_lastClip = -1;
	s_lastWeapon = -2;
	s_lastTime = 0.0f;
	zhbo_native_reset();
}

void ZhboViewmodel_Preload(char faction, const int *weapons, int count)
{
	WW3DAssetManager *assets = WW3DAssetManager::Get_Instance();
	if (!assets)
		return;
	const double started = zhbo_platform::now();
	int loaded = 0;
	for (int i = -2; i < count; ++i)
	{
		AsciiString name;
		if (i < 0)
			name.format("ZHBO_VM%s_%s", faction == 'C' ? "CHN" : faction == 'G' ? "GLA" : "USA", i == -1 ? "TAB" : "HAM");
		else
			name.format("ZV%02d%c_SKN", weapons[i], faction);
		// The asset manager keeps the prototype and the clips once loaded; these copies can go.
		RenderObjClass *model = assets->Create_Render_Obj(name.str());
		if (!model)
			continue;
		REF_PTR_RELEASE(model);
		std::map<int, HAnimClass *> clips;
		loadClips(assets, name, i < 0 ? ZHBO_VM_NONE : weapons[i], clips);
		for (auto &c : clips)
			REF_PTR_RELEASE(c.second);
		++loaded;
	}
	Zhbo_Log("viewmodel: preloaded %d viewmodels in %.0f ms", loaded, (zhbo_platform::now() - started) * 1000.0);
}

namespace
{
	// ZHBO_NATIVE_COMPARE=1 (with ZHBO_NATIVE_VIEWMODEL=1): pose the W3D model too, hidden, and log
	// where both paths put the gun's tags once a second, to measure any placement difference in game.
	Bool compareMode()
	{
		static const Bool s_compare = getenv("ZHBO_NATIVE_COMPARE") && atoi(getenv("ZHBO_NATIVE_COMPARE")) != 0;
		return s_compare;
	}

	void logCompare(Int clip, Real time, Real ads)
	{
		static int s_calls = 0;
		if (s_calls++ % 30 != 0)
			return;
		for (const char *bone : { "tag_weapon", "tag_flash" })
		{
			float m[12];
			const int index = s_model->Get_Bone_Index(bone);
			if (index <= 0 || !zhbo_native_bone(bone, m))
				continue;
			const Vector3 w = s_model->Get_Bone_Transform(index).Get_Translation();
			const float dx = m[3] - w.X, dy = m[7] - w.Y, dz = m[11] - w.Z;
			Zhbo_Log("native compare: clip %d t %.2f ads %.2f %s w3d (%.3f %.3f %.3f) native (%.3f %.3f %.3f) off %.3f", clip, time, ads, bone,
				w.X, w.Y, w.Z, m[3], m[7], m[11], sqrtf(dx * dx + dy * dy + dz * dz));
		}
	}

	// Pose the W3D model for this frame (load, place, animate, slide onto the sights). FALSE when
	// there is no W3D model to pose.
	Bool poseW3d(const Coord3D &eye, const Coord3D &dir, Int clip, Int weapon, const AsciiString &name, Real time, Real ads,
		const Coord3D &offset, const Coord3D &angles)
	{
		if (name != s_modelName && name != s_missing && !load(name, weapon))
			return FALSE;
		if (!s_model)
			return FALSE;

		Vector3 x, y, z;
		Matrix3D m = viewTransform(eye, dir, x, y, z);
		// Sway, bob and recoil from the core: offset in view space, then yaw, pitch (muzzle up), roll.
		m.Translate(Vector3(offset.x, offset.y, offset.z));
		m.Rotate_Z(angles.x);
		m.Rotate_Y(-angles.y);
		m.Rotate_X(angles.z);
		s_model->Set_Transform(m);

		std::map<int, HAnimClass *>::iterator it = s_anims.find(clip);
		if (it == s_anims.end()) // a slot this weapon (or alt mode) doesn't have: hold its idle
			it = s_anims.find((clip >= ZHBO_VM_ALT_CLIPS ? ZHBO_VM_ALT_CLIPS : 0) + ZHBO_VM_SLOT_IDLE);
		if (it == s_anims.end())
			it = s_anims.find(ZHBO_VM_SLOT_IDLE);
		if (it != s_anims.end())
		{
			HAnimClass *anim = it->second;
			const Real frames = (Real)anim->Get_Num_Frames();
			Real frame = time * anim->Get_Frame_Rate();
			if (loops(it->first))
				frame = frames > 1.0f ? fmodf(frame, frames - 1.0f) : 0.0f;
			else
				frame = std::min(frame, std::max(frames - 1.0f, 0.0f));
			s_model->Set_Animation(anim, frame, RenderObjClass::ANIM_MODE_MANUAL);
		}

		// Aim down the sights: move the model so the sight bone is on the view axis (the converter
		// gives every gun one: BO2's, an optic's window, or its rear sight's top).
		if (ads > 0.0f)
		{
			const int sights = s_model->Get_Bone_Index("tag_sights");
			static Bool s_warned = FALSE;
			if (sights <= 0 && !s_warned)
			{
				s_warned = TRUE;
				Zhbo_Log("viewmodel: no tag_sights bone (index %d)", sights);
			}
			if (sights > 0)
			{
				Vector3 local;
				Matrix3D::Inverse_Transform_Vector(m, s_model->Get_Bone_Transform(sights).Get_Translation(), &local);
				m.Adjust_Translation((y * -local.Y + z * -local.Z) * ads);
				s_model->Set_Transform(m);
			}
		}
		return TRUE;
	}
}

void ZhboViewmodel_Update(Bool active, const Coord3D &eye, const Coord3D &dir)
{
	Int clip = ZHBO_VM_NONE, weapon = ZHBO_VM_NONE;
	Bool fired = FALSE;
	Real time = 0.0f, ads = 0.0f;
	AsciiString name;
	Coord3D offset, angles;
	if (!active || !Zhbo_GetViewmodel(&clip, &weapon, &fired, &time, &ads, &name, &offset, &angles) || clip < 0)
	{
		if (s_model)
			s_model->Set_Hidden(true);
		s_lastClip = -1;
		return;
	}
	if (!s_notesLoaded)
		loadNotes();
	playNotes(weapon, clip, time);
	const Bool native = nativeViewmodel(eye, dir, clip, weapon, name, time, ads, offset, angles, fired);
	if (native && !compareMode())
		return;
	if (!poseW3d(eye, dir, clip, weapon, name, time, ads, offset, angles))
		return;
	if (native)
	{
		s_model->Set_Hidden(true);
		logCompare(clip, time, ads);
		return;
	}
	s_model->Set_Hidden(false);
	if (fired)
		viewmodelFx(weapon, FALSE);
}
