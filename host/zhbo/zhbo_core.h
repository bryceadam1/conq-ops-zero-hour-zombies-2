/*
** Conquer Ops - C declarations of the Rust core (crates/zhbo-core/src/ffi.rs, host.rs, client.rs).
** Keep in sync with the Rust side; ZHBO_ABI_VERSION must match zhbo_abi_version().
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
#pragma once

#include <stdint.h>

#define ZHBO_ABI_VERSION 14u

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ZhboVec3 { float x, y, z; } ZhboVec3;

enum ZhboObjFlags
{
	ZHBO_OBJ_STRUCTURE  = 1u << 0,
	ZHBO_OBJ_INFANTRY   = 1u << 1,
	ZHBO_OBJ_VEHICLE    = 1u << 2,
	ZHBO_OBJ_AIRCRAFT   = 1u << 3,
	ZHBO_OBJ_DEAD       = 1u << 4,
	ZHBO_OBJ_ENEMY      = 1u << 5,
	ZHBO_OBJ_ALLY       = 1u << 6,
	ZHBO_OBJ_NO_COLLIDE = 1u << 7,
	ZHBO_OBJ_AIRBORNE   = 1u << 8,
	ZHBO_OBJ_DRONE      = 1u << 9,  /* the Black Hat can hack it */
	ZHBO_OBJ_PROJECTILE = 1u << 10, /* a missile or shell in flight: the Trophy System shoots it down */
	ZHBO_OBJ_SHROUDED   = 1u << 11, /* client queries: hidden from the local player by shroud or fog */
	ZHBO_OBJ_FIRING     = 1u << 12, /* firing a weapon right now */
};

enum ZhboGeom { ZHBO_GEOM_SPHERE = 0, ZHBO_GEOM_CYLINDER = 1, ZHBO_GEOM_BOX = 2 };

typedef struct ZhboObjInfo
{
	uint32_t id;
	int32_t owner_player;
	uint32_t flags;
	ZhboVec3 pos;
	float yaw;
	int32_t geom_type;
	float major_radius;
	float minor_radius;
	float height;
	float health;
	float max_health;
	uint32_t color; /* owner's team colour, ARGB */
} ZhboObjInfo;

typedef struct ZhboFpsInput
{
	uint32_t held;
	uint32_t pressed;
	int32_t move_packed;
	float yaw;
	float pitch;
} ZhboFpsInput;

/* zhbo_core::building::BuildOption. status: ZHBO_BUILD_*. */
typedef struct ZhboBuildOption
{
	uint32_t tmpl;
	int32_t cost;
	int32_t status;
	float radius; /* its footprint's bounding radius */
	char name[48];
} ZhboBuildOption;
#define ZHBO_BUILD_OK 0
#define ZHBO_BUILD_NO_MONEY 1
#define ZHBO_BUILD_LOCKED 2
#define ZHBO_BUILD_BLOCKED 3

typedef struct ZhboLogicApi
{
	void (*log)(const char *msg);
	float (*ground_height)(float x, float y);
	int32_t (*is_walkable)(float x, float y);
	void (*extent)(ZhboVec3 *lo, ZhboVec3 *hi);
	int32_t (*object)(uint32_t id, uint32_t viewer, ZhboObjInfo *out);
	int32_t (*query)(const ZhboVec3 *center, float radius, uint32_t viewer, ZhboObjInfo *out, int32_t cap);
	void (*set_pose)(uint32_t id, const ZhboVec3 *pos, float yaw, int32_t moving);
	void (*set_controlled)(uint32_t id, int32_t controlled);
	void (*fire_fx)(uint32_t shooter, const char *fx_weapon, uint32_t target, const ZhboVec3 *pos);
	int32_t (*damage)(uint32_t shooter, uint32_t victim, float amount, int32_t kind); /* ZHBO_DAMAGE_* */
	void (*heal)(uint32_t id, float amount);
	/* A projectile's Zero Hour object, owned by `owner`'s player; 0 if it can't be made. */
	uint32_t (*spawn)(const char *thing, uint32_t owner, const ZhboVec3 *pos, float yaw);
	void (*destroy)(uint32_t id);
	void (*disable)(uint32_t id, int32_t kind, float seconds); /* ZHBO_DISABLE_* */
	void (*fx)(const char *fxlist, const ZhboVec3 *pos);
	void (*capture)(uint32_t id, uint32_t new_owner); /* to new_owner's side (the Black Hat) */
	void (*smoke)(const ZhboVec3 *center, float radius); /* a smoke screen this frame */
	/* Building (zhbo_core::building). The General's build menu: up to cap options; the count. */
	int32_t (*build_options)(uint32_t general, ZhboBuildOption *out, int32_t cap);
	int32_t (*can_place)(uint32_t general, uint32_t tmpl, const ZhboVec3 *pos, float angle);
	/* Pay for it and lay out its construction site; the site's id, or 0. */
	uint32_t (*place)(uint32_t general, uint32_t tmpl, const ZhboVec3 *pos, float angle);
	/* The closest of his side's structures under construction in reach; its id (0 none). */
	uint32_t (*build_site)(uint32_t general, const ZhboVec3 *pos, float reach, uint32_t *tmpl, float *percent);
	/* One logic frame of building `site` at `rate` of a dozer's speed; the percent after. */
	float (*construct)(uint32_t general, uint32_t site, float rate);
} ZhboLogicApi;

/* zhbo_core::host::damage_kind and disable_kind. */
#define ZHBO_DAMAGE_SMALL_ARMS 0
#define ZHBO_DAMAGE_MELEE 1
#define ZHBO_DAMAGE_EXPLOSION 2
#define ZHBO_DISABLE_PARALYZED 0
#define ZHBO_DISABLE_EMP 1

typedef struct ZhboClientApi
{
	void (*screen_size)(float *w, float *h);
	void (*fill_rect)(float x, float y, float w, float h, uint32_t argb);
	void (*line)(float x0, float y0, float x1, float y1, float width, uint32_t argb);
	void (*text)(float x, float y, const char *utf8, uint32_t argb, int32_t size, int32_t align);
	int32_t (*image)(const char *name, float x, float y, float w, float h, uint32_t argb);
	int32_t (*query)(const ZhboVec3 *center, float radius, uint32_t viewer, ZhboObjInfo *out, int32_t cap);
	int32_t (*cash)(int32_t player);
	void (*sound)(const char *event, const ZhboVec3 *at); /* at: null for the player's own sounds */
	void (*clip)(float x, float y, float w, float h); /* clip later images; w <= 0 turns it off */
	/* Zero Hour's radar in a screen rectangle; 0 if the local player has no radar. you_px gets
	   where the world point `you` is on it (x, y). */
	int32_t (*radar)(float x, float y, float w, float h, const ZhboVec3 *you, ZhboVec3 *you_px);
	int32_t (*build_options)(uint32_t general, ZhboBuildOption *out, int32_t cap);
	int32_t (*can_place)(uint32_t general, uint32_t tmpl, const ZhboVec3 *pos, float angle);
	/* The hammer's placement ghost; tmpl 0 hides it. */
	void (*ghost)(uint32_t tmpl, const ZhboVec3 *pos, float angle, int32_t legal);
} ZhboClientApi;

typedef struct ZhboClientIn
{
	float dt;
	int32_t local_player;
	uint32_t logic_frame;
	uint8_t keys[256];
	float mouse_dx;
	float mouse_dy;
	float wheel;
	int32_t input_allowed;
	float screen_w;
	float screen_h;
	ZhboVec3 rts_cam_pos;
	ZhboVec3 rts_cam_dir;
	float rts_hfov; /* the RTS camera's horizontal field of view, radians */
	ZhboVec3 rts_pivot; /* the point the RTS camera pivots on, on the ground */
} ZhboClientIn;

typedef struct ZhboClientOut
{
	int32_t camera_override;
	ZhboVec3 eye;
	ZhboVec3 dir;
	float hfov;
	int32_t capture_mouse;
	int32_t hide_rts_ui;
	uint32_t hide_object;
	int32_t send_input;
	uint32_t input_general;
	ZhboFpsInput input;
	int32_t rts_look_at_valid;
	ZhboVec3 rts_look_at;
	int32_t viewmodel;      /* BO2 animation slot (weapAnimFiles_t), a ZHBO_VM_TABLET_* clip, or ZHBO_VM_NONE */
	int32_t viewmodel_weapon; /* converted weapon viewmodel (ZVnn models), ZHBO_VM_NONE for the tablet */
	int32_t viewmodel_fired;  /* the gun fired this frame: muzzle flash and brass */
	float viewmodel_time;   /* seconds into the clip */
	float viewmodel_ads;    /* 0..1 aim-down-sights */
	ZhboVec3 viewmodel_offset; /* sway/bob/recoil: forward, left, up (world units) */
	ZhboVec3 viewmodel_angles; /* yaw (left +), pitch (muzzle up +), roll; radians */
} ZhboClientOut;

/* Viewmodel clips (zhbo_core::viewmodel): BO2's animation slots, then the scorestreak tablet's (its
   own model: the faction's weapon viewmodel name with _TAB). */
#define ZHBO_VM_NONE (-1)
#define ZHBO_VM_SLOT_IDLE 0x01
#define ZHBO_VM_SLOT_EMPTY_IDLE 0x02
#define ZHBO_VM_SLOT_FIRE 0x04
#define ZHBO_VM_SLOT_LASTSHOT 0x06
#define ZHBO_VM_SLOT_SPRINT_LOOP 0x21
#define ZHBO_VM_SLOT_SPRINT_EMPTY_LOOP 0x24
#define ZHBO_VM_SLOT_ADS_FIRE 0x3D
#define ZHBO_VM_SLOT_ADS_LASTSHOT 0x3E
#define ZHBO_VM_SLOT_COUNT 0x58
#define ZHBO_VM_TABLET_PULLOUT 0x60
#define ZHBO_VM_TABLET_IDLE 0x61
#define ZHBO_VM_TABLET_PUTAWAY 0x62
/* The builder's hammer (its own model: the faction's weapon viewmodel name with _HAM). */
#define ZHBO_VM_HAMMER_PULLOUT 0x64
#define ZHBO_VM_HAMMER_IDLE 0x65
#define ZHBO_VM_HAMMER_SWING 0x66
#define ZHBO_VM_HAMMER_PUTAWAY 0x67
/* An alt mode's clips are its BO2 slots plus this, on its weapon's viewmodel (zhbo_core::weapons::ALT_CLIPS). */
#define ZHBO_VM_ALT_CLIPS 0x80

/* How a controlled General looks in third person (zhbo_core::ffi::WorldPose). Its soldier model
   carries each converted weapon as sub-objects W<vm:02><k:02> and the tablet as TB<k:02>. */
typedef struct ZhboWorldPose
{
	uint32_t general;
	int32_t weapon; /* viewmodel index of the weapon in hand */
	int32_t tablet; /* 0 none, 1 coming up (MODELCONDITION_USER_1), 2 going down (USER_2) */
	int32_t sidearm; /* the sidearm is in hand: pistol stance (MODELCONDITION_JAMMED) */
} ZhboWorldPose;

uint32_t zhbo_abi_version(void);
void zhbo_init(const ZhboLogicApi *logic, const ZhboClientApi *client);
int32_t zhbo_install_weapons(const char *table); /* Data\\ConquerOps\\weapons.ini; count or -1 */
void zhbo_reset(void);
void zhbo_logic_take_control(int32_t player, uint32_t general);
void zhbo_logic_input(int32_t player, uint32_t general, const ZhboFpsInput *input);
void zhbo_logic_step(uint32_t frame);
int32_t zhbo_logic_is_controlled(uint32_t object_id);
void zhbo_client_frame(const ZhboClientIn *in, ZhboClientOut *out);
void zhbo_client_draw_hud(int32_t local_player);
const uint16_t *zhbo_string(const char *label);
int32_t zhbo_client_world_poses(ZhboWorldPose *out, int32_t cap);
/* The General's Zero Hour weapon slots (primary, secondary, lethal) under RTS control
   (zhbo_core::sim::RtsWeapons). */
typedef struct ZhboRtsWeapons
{
	int32_t ammo[3];    /* rounds in the magazine; grenades for the lethal */
	int32_t reserve[3]; /* spare rounds */
	int32_t lock;       /* slot to lock to, -1 none */
	int32_t tactical;   /* tactical grenades left */
	int32_t tactical_selected; /* the third slot throws the tactical (else the lethal) */
} ZhboRtsWeapons;
/* target: where the third slot's throw lands (the AI's victim or attack point), or null. */
void zhbo_logic_rts_update(int32_t player, uint32_t general, int32_t current_slot, const uint32_t *shots, const ZhboVec3 *target, ZhboRtsWeapons *out);
void zhbo_logic_rts_offhand(int32_t player, uint32_t general, int32_t tactical);
void zhbo_client_rts_ammo(uint32_t general, ZhboRtsWeapons *out);
int32_t zhbo_world_mesh_count(int32_t vm); /* -2: the tablet */
int32_t zhbo_loadout_viewmodels(int32_t *out, int32_t cap); /* the General's kit's viewmodels */
float zhbo_general_build_rate(void); /* a General's building speed, share of a dozer's */
int32_t zhbo_check_requirements(char *msg, uint32_t cap);

#ifdef __cplusplus
}

static_assert(sizeof(ZhboObjInfo) == 56, "ZhboObjInfo layout must match host.rs");
static_assert(sizeof(ZhboFpsInput) == 20, "ZhboFpsInput layout must match input.rs");
static_assert(sizeof(ZhboClientIn) == 332, "ZhboClientIn layout must match client.rs");
static_assert(sizeof(ZhboClientOut) == 132, "ZhboClientOut layout must match client.rs");
static_assert(sizeof(ZhboLogicApi) == 22 * sizeof(void *), "ZhboLogicApi layout must match host.rs");
static_assert(sizeof(ZhboWorldPose) == 16, "ZhboWorldPose layout must match ffi.rs");
static_assert(sizeof(ZhboRtsWeapons) == 36, "ZhboRtsWeapons layout must match sim.rs");
static_assert(sizeof(ZhboClientApi) == 13 * sizeof(void *), "ZhboClientApi layout must match host.rs");
static_assert(sizeof(ZhboBuildOption) == 64, "ZhboBuildOption layout must match building.rs");
#endif
