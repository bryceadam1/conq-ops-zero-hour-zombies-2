// Conquer Ops: the first-person viewmodel (BO2 arms holding the weapon), drawn in the 3D scene.
//
// The model and its clips are converted from the player's BO2 install into ConquerOps_BO2.big
// (zhbo_assets::generals). zhbo-core picks the clip and the time into it each frame; this places the
// model at the eye, facing the view, and poses it. Aiming down the sights slides the model sideways
// and down until the weapon's tag_sights bone sits on the line of sight. Built into
// z_gameenginedevice; W3DView::update calls ZhboViewmodel_Update (host/patches/0001).
#include "ZhboBridge.h"
#include "zhbo_core.h"

#include "Common/AudioEventRTS.h"
#include "Common/File.h"
#include "Common/FileSystem.h"
#include "Common/GameAudio.h"
#include "GameClient/FXList.h"
#include "W3DDevice/GameClient/W3DDisplay.h"
#include "W3DDevice/GameClient/W3DScene.h"
#include "WW3D2/assetmgr.h"
#include "WW3D2/hanim.h"
#include "WW3D2/rendobj.h"

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

	// Clips that loop (zhbo_core::viewmodel::slot::loops), an alt mode's too.
	Bool loops(int clip)
	{
		if (clip == ZHBO_VM_TABLET_IDLE)
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
		File *f = TheFileSystem ? TheFileSystem->openFile("Data\\ConquerOps\\viewmodel_sounds.txt", File::READ | File::BINARY) : nullptr;
		if (!f)
			return;
		const Int size = f->size();
		char *buf = f->readEntireAndClose();
		if (!buf)
			return;
		std::istringstream in(std::string(buf, size));
		delete[] buf;
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
		// Only this model's clips: each weapon and the tablet have their own hierarchy.
		if (name.endsWith("_TAB"))
		{
			for (int i = 0; i < 3; ++i)
				if (HAnimClass *anim = assets->Get_HAnim(k_tablet_clips[i]))
					s_anims[ZHBO_VM_TABLET_PULLOUT + i] = anim;
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
						s_anims[base + slot] = anim;
				}
			}
		}
		Zhbo_Log("viewmodel: %d clips for %s", (int)s_anims.size(), name.str());
		s_modelName = name;
		Zhbo_Log("viewmodel: loaded %s", name.str());
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
		Matrix3D m(true);
		m[0].Set(x.X, y.X, z.X, eye.x);
		m[1].Set(x.Y, y.Y, z.Y, eye.y);
		m[2].Set(x.Z, y.Z, z.Z, eye.z);
		return m;
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
	if (name != s_modelName && name != s_missing && !load(name, weapon))
		return;
	if (!s_model)
		return;
	s_model->Set_Hidden(false);

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

	// Aim down the sights: move the model so the sight bone is on the view axis.
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

	if (fired)
	{
		boneFx("ZHBO_ViewmodelMuzzle", "tag_flash");
		boneFx("ZHBO_ViewmodelBrass", "tag_brass");
	}
}
