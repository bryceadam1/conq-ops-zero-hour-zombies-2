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

#define ZHBO_ABI_VERSION 7u

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
	int32_t (*damage)(uint32_t shooter, uint32_t victim, float amount, int32_t melee);
	void (*heal)(uint32_t id, float amount);
} ZhboLogicApi;

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
	int32_t viewmodel;      /* ZHBO_VM_* clip, or ZHBO_VM_NONE */
	float viewmodel_time;   /* seconds into the clip */
	float viewmodel_ads;    /* 0..1 aim-down-sights */
	ZhboVec3 viewmodel_offset; /* sway/bob/recoil: forward, left, up (world units) */
	ZhboVec3 viewmodel_angles; /* yaw (left +), pitch (muzzle up +), roll; radians */
} ZhboClientOut;

/* Viewmodel clips (zhbo_core::viewmodel). */
#define ZHBO_VM_NONE (-1)
#define ZHBO_VM_IDLE 0
#define ZHBO_VM_FIRE 1
#define ZHBO_VM_RELOAD 2
#define ZHBO_VM_RELOAD_EMPTY 3
#define ZHBO_VM_SPRINT 4
#define ZHBO_VM_RAISE 5
/* The scorestreak tablet (its own model: the weapon viewmodel's name with _TAB for _SKN). */
#define ZHBO_VM_TABLET_PULLOUT 6
#define ZHBO_VM_TABLET_IDLE 7
#define ZHBO_VM_TABLET_PUTAWAY 8
#define ZHBO_VM_COUNT 9

uint32_t zhbo_abi_version(void);
void zhbo_init(const ZhboLogicApi *logic, const ZhboClientApi *client);
void zhbo_reset(void);
void zhbo_logic_take_control(int32_t player, uint32_t general);
void zhbo_logic_input(int32_t player, uint32_t general, const ZhboFpsInput *input);
void zhbo_logic_step(uint32_t frame);
int32_t zhbo_logic_is_controlled(uint32_t object_id);
void zhbo_client_frame(const ZhboClientIn *in, ZhboClientOut *out);
void zhbo_client_draw_hud(int32_t local_player);
const uint16_t *zhbo_string(const char *label);
int32_t zhbo_check_requirements(char *msg, uint32_t cap);

#ifdef __cplusplus
}

static_assert(sizeof(ZhboObjInfo) == 56, "ZhboObjInfo layout must match host.rs");
static_assert(sizeof(ZhboFpsInput) == 20, "ZhboFpsInput layout must match input.rs");
static_assert(sizeof(ZhboClientIn) == 316, "ZhboClientIn layout must match client.rs");
static_assert(sizeof(ZhboClientOut) == 124, "ZhboClientOut layout must match client.rs");
static_assert(sizeof(ZhboLogicApi) == 11 * sizeof(void *), "ZhboLogicApi layout must match host.rs");
static_assert(sizeof(ZhboClientApi) == 9 * sizeof(void *), "ZhboClientApi layout must match host.rs");
#endif
