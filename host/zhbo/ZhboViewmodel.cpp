// Conquer Ops: the first-person viewmodel (BO2 arms holding the weapon), drawn in the 3D scene.
//
// The model and its clips are converted from the player's BO2 install into ConquerOps_BO2.big
// (zhbo_assets::generals). zhbo-core picks the clip and the time into it each frame; this places the
// model at the eye, facing the view, and poses it. Aiming down the sights slides the model sideways
// and down until the weapon's tag_sights bone sits on the line of sight. Built into
// z_gameenginedevice; W3DView::update calls ZhboViewmodel_Update (host/patches/0001).
#include "ZhboBridge.h"
#include "zhbo_core.h"

#include "W3DDevice/GameClient/W3DDisplay.h"
#include "W3DDevice/GameClient/W3DScene.h"
#include "WW3D2/assetmgr.h"
#include "WW3D2/hanim.h"
#include "WW3D2/rendobj.h"

#include <cmath>

namespace
{
	const char *const k_clips[ZHBO_VM_COUNT] = {
		"ZHBO_VM_SKL.ZHBO_VM_IDL", // ZHBO_VM_IDLE
		"ZHBO_VM_SKL.ZHBO_VM_FIR", // ZHBO_VM_FIRE
		"ZHBO_VM_SKL.ZHBO_VM_RLD", // ZHBO_VM_RELOAD
		"ZHBO_VM_SKL.ZHBO_VM_RLE", // ZHBO_VM_RELOAD_EMPTY
		"ZHBO_VM_SKL.ZHBO_VM_SPR", // ZHBO_VM_SPRINT
		"ZHBO_VM_SKL.ZHBO_VM_RAI", // ZHBO_VM_RAISE
	};

	RenderObjClass *s_model = nullptr;
	AsciiString s_modelName;
	HAnimClass *s_anims[ZHBO_VM_COUNT] = {};
	AsciiString s_missing; // a model that failed to load; not retried

	void release()
	{
		for (HAnimClass *&a : s_anims)
		{
			REF_PTR_RELEASE(a);
		}
		if (s_model)
		{
			s_model->Remove();
			REF_PTR_RELEASE(s_model);
		}
		s_modelName.clear();
	}

	Bool load(const AsciiString &name)
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
		for (int i = 0; i < ZHBO_VM_COUNT; ++i)
		{
			s_anims[i] = assets->Get_HAnim(k_clips[i]);
			if (!s_anims[i])
				Zhbo_Log("viewmodel: missing clip %s", k_clips[i]);
		}
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
	Int clip = ZHBO_VM_NONE;
	Real time = 0.0f, ads = 0.0f;
	AsciiString name;
	if (!active || !Zhbo_GetViewmodel(&clip, &time, &ads, &name) || clip < 0 || clip >= ZHBO_VM_COUNT)
	{
		if (s_model)
			s_model->Set_Hidden(true);
		return;
	}
	if (name != s_modelName && name != s_missing && !load(name))
		return;
	if (!s_model)
		return;
	s_model->Set_Hidden(false);

	Vector3 x, y, z;
	Matrix3D m = viewTransform(eye, dir, x, y, z);
	s_model->Set_Transform(m);

	if (HAnimClass *anim = s_anims[clip])
	{
		const Real frames = (Real)anim->Get_Num_Frames();
		Real frame = time * anim->Get_Frame_Rate();
		if (clip == ZHBO_VM_IDLE || clip == ZHBO_VM_SPRINT)
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
}
