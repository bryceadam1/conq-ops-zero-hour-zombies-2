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

#define ZHBO_ABI_VERSION 26u

/* Field counts of the API tables below. The Rust side derives its layout asserts and
   zhbo_abi_fields() from the same numbers; Zhbo_Init refuses a mismatch. When adding a service:
   append the field here, bump both counts, bump ZHBO_ABI_VERSION, and see CONTRACT.md. */
#define ZHBO_LOGIC_API_FIELDS 24u
#define ZHBO_CLIENT_API_FIELDS 13u

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

/* What a shot hit (zhbo_core::impacts::Surface). The sim classifies an object hit from its kind
   flags; the bridge classifies the ground from Zero Hour's terrain class (cbSurfaceAt). The values
   are the ABI's: they go into ZhboClientHit.surface and back into zhbo_impact_fx. */
enum ZhboSurfaces
{
	ZHBO_SURF_DEFAULT = 0,
	ZHBO_SURF_FLESH,
	ZHBO_SURF_METAL,
	ZHBO_SURF_CONCRETE,
	ZHBO_SURF_DIRT,
	ZHBO_SURF_GRASS,
	ZHBO_SURF_ASPHALT,
	ZHBO_SURF_SAND,
	ZHBO_SURF_SNOW,
	ZHBO_SURF_ROCK,
	ZHBO_SURF_WOOD,
	ZHBO_SURF_WATER,
	ZHBO_SURFS
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
#define ZHBO_BUILD_BUSY 4

/* zhbo_core::building::Site: a structure the General can work on. kind: ZHBO_WORK_*. */
typedef struct ZhboWorkSite
{
	uint32_t id;
	uint32_t tmpl;
	int32_t kind;
	float percent; /* built, or (repairing) its health, 0..100 */
	char name[48]; /* display name, UTF-8 */
} ZhboWorkSite;
#define ZHBO_WORK_BUILD 0
#define ZHBO_WORK_REPAIR 1

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
	/* The closest structure in reach he can build or repair (as a dozer could); its id (0 none). */
	uint32_t (*work_site)(uint32_t general, const ZhboVec3 *pos, float reach, ZhboWorkSite *out);
	/* One logic frame of working on `site` at `rate` of a dozer's speed; how far along after
	   (100 done), or negative if another builder has it. */
	float (*work)(uint32_t general, uint32_t site, float rate);
	/* The pick menu's choice: the General assumes the class at the shared list's index
	   (crates/zhbo-core/src/classes.rs; the list ships in the mod's BIG file). */
	void (*choose_class)(int32_t player, uint32_t general, int32_t index);
	/* ABI 26: the ground's surface at a world point (a ZHBO_SURF_*), from Zero Hour's terrain
	   class (crates/zhbo-core/src/impacts.rs; CONTRACT.md "Shot impacts (R8d)"). Presentation
	   only: it feeds the impact FX, never damage or sim state. */
	int32_t (*surface_at)(float x, float y);
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

/* The 256-bit platform-neutral key-down set (see input.rs's Key enum: low bits first, one bit
   per named physical key, mouse button or wheel). */
typedef struct ZhboKeySet
{
	uint64_t bits[4];
} ZhboKeySet;

typedef struct ZhboClientIn
{
	float dt;
	int32_t local_player;
	uint32_t logic_frame;
	ZhboKeySet keys;
	float mouse_dx;
	float mouse_dy;
	float mouse_x; /* absolute cursor position (screen pixels), for menus (the class pick) */
	float mouse_y;
	uint32_t mouse_buttons; /* the mouse buttons' held state */
	float wheel;
	int32_t input_allowed;
	float screen_w;
	float screen_h;
	ZhboVec3 rts_cam_pos;
	ZhboVec3 rts_cam_dir;
	float rts_hfov; /* the RTS camera's horizontal field of view, radians */
	ZhboVec3 rts_pivot; /* the point the RTS camera pivots on, on the ground */
	/* One-shot: force the in-game class pick screen open (the command bar's Create-a-Class entry).
	   The lobby's button does NOT use this: it opens the shell screen (zhbo_menu_open) instead. */
	int32_t class_pick_force;
	/* One-shot: a command-bar class button was pressed: 1-based in the class-button rows
	   (zhbo_class_buttons' order: the presets, then the profile's customs), 0 = none. The core
	   emits the same choose_class pick the pick screen sends, naming the General it has under
	   command, so the bar and the screen never diverge. The bridge cannot queue that pick itself:
	   zhbo_client_frame writes a fresh ZhboClientOut over its copy every frame. */
	int32_t class_button;
} ZhboClientIn;

/* ABI 26 (R8d): one shot impact of the local player's this frame (zhbo_core::client::ClientHit).
   The bridge plays BO2's impact-table cell at `point`, oriented by `dir`; `surface` picks the cell,
   `head`/`fatal` the blood variant, `impact_type` the bucket. */
enum { ZHBO_CLIENT_HITS = 8 };
typedef struct ZhboClientHit
{
	ZhboVec3 point;       /* world, Zero Hour units */
	ZhboVec3 dir;         /* the pellet's direction; the effect faces back along it */
	int32_t  surface;     /* ZHBO_SURF_* */
	int32_t  head;        /* the hit is in the head band */
	int32_t  fatal;       /* the hit killed */
	int32_t  impact_type; /* BO2's WeaponDef.impactType: the impact-table bucket */
} ZhboClientHit;

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
	int32_t choose_class; /* one-shot: 1-based class index (0 = none, i32::MIN = custom payload) */
	uint32_t choose_class_general;
	/* A custom class's wire words (classes.rs' CustomClass::to_wire_words), valid when
	   choose_class_payload_len > 0: the bridge appends them to the pick message. */
	uint32_t choose_class_payload[64];
	int32_t choose_class_payload_len;
	/* One-shot: write the profile file now. 1 = the editor saved a class, 2 = the pick screen
	   deleted/copied a slot. A pick alone never sets it (re-picking must not rewrite the file).
	   The payload is the profile file's full text, UTF-8 packed four bytes per word
	   (zhbo_core::profile::pack_text); see zhbo_profile_write. */
	int32_t save_profile;
	uint32_t save_profile_payload[1024];
	int32_t save_profile_payload_len;
	/* The class screen (pick or editor) is up: the bridge leaves the cursor free and swallows the
	   game-bound mouse/key messages behind it. */
	int32_t menu_open;
	/* The local player's shot impacts this frame, in shot order (at most ZHBO_CLIENT_HITS). */
	ZhboClientHit hits[ZHBO_CLIENT_HITS];
	int32_t hits_len;
} ZhboClientOut;

/* The shell create-a-class screen (the lobby's class menu; docs/CONTRACT.md's menu ABI): the
   bridge's modal shell window fills ZhboMenuIn from its draw callback (keys from the platform
   poll, the pointer/buttons from the engine's cursor, the wheel from the window's GWM_WHEEL
   messages, the screen from the display) and calls zhbo_menu_frame; the core draws through the
   stored ZhboClientApi callbacks (valid during the window's draw) and plays its UI sounds through
   the stored sound callback. Pure local UI: no determinism/lockstep requirements; the profile file
   is its only output. */
typedef struct ZhboMenuIn
{
	ZhboKeySet keys;
	float mouse_x; /* absolute cursor position (screen pixels) */
	float mouse_y;
	uint32_t mouse_buttons; /* the mouse buttons' held state (bit 0 left, 1 right, 2 middle) */
	float wheel; /* wheel notches since last frame */
	float screen_w;
	float screen_h;
} ZhboMenuIn;

typedef struct ZhboMenuOut
{
	int32_t closed; /* one-shot: the screen asked to close (Escape/Back on the pick list) */
	/* One-shot: write the profile file now (1 = the editor saved a class, 2 = the list
	   deleted/copied a slot), same shape as ZhboClientOut's save_profile: the file's full UTF-8
	   text packed four bytes per word (zhbo_core::profile::pack_text); see zhbo_profile_write. */
	int32_t save_profile;
	uint32_t save_profile_payload[1024];
	int32_t save_profile_payload_len;
} ZhboMenuOut;

/* One command-bar class button (zhbo_core::classicons): the one-page class command set the bridge
   builds while a General has no class: Take Control plus the ACTIVE set's ten classes (the five
   premade, then up to five customs); rows past the tenth come back present=0 (ZHBO_CLASS_BUTTONS
   is only the array capacity). The bridge caches these rows at game start and draws each class's
   kit over its button. The labels are also served to the engine through
   strings.rs (CONTROLBAR:ZHBO_ClassButton01..15), so the command button's TextLabel follows the
   live class list. */
#define ZHBO_CLASS_BUTTONS 15
typedef struct ZhboClassButton
{
	int32_t present;   /* non-zero when a class occupies this button */
	int32_t page;      /* 1 or 2 */
	int32_t slot;      /* the command set's index: page one starts at 1 (0 is Take Control) */
	char name[48];     /* the button's label, UTF-8 */
	char primary[48];  /* converted MappedImage names (ZHBO_BO2_Icon_<id>) */
	char secondary[48];
	char lethal[48];
	char tactical[48];
	char perks[6][40]; /* the perk icons, in pick order */
	int32_t perk_count;
	int32_t payload_len; /* a custom class's wire words (0 for a shipped preset) */
	uint32_t payload[64];
} ZhboClassButton;

/* `choose_class`'s marker for a class built in the editor (client/mod.rs' CHOOSE_CLASS_CUSTOM): the
   payload carries its wire words instead of a registry index. */
#define ZHBO_CHOOSE_CLASS_CUSTOM (-2147483647 - 1)

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
/* Per-table field counts (low word: logic, high word: client); Zhbo_Init refuses a mismatch. */
uint64_t zhbo_abi_fields(void);
/* Load the mod's shipped strings file (see strings.rs); lookup consults it first. */
void zhbo_load_strings(const char *text);
/* Load the gameplay tuning pack (see tuning.rs); the sim's defaults hold until this runs. */
void zhbo_load_tuning(const char *text);
/* Load the class presets (see classes.rs); the five basic BO2 classes hold until this runs. */
void zhbo_load_classes(const char *text);
/* Load the converted BO2 impact table (Data\ConquerOps\impacts.txt; see impacts.rs); returns the
   number of cells, -1 for null. */
int32_t zhbo_load_impacts(const char *text);
/* Load the BO2 font metrics (see lui/font.rs); empty text leaves the engine font in use. */
void zhbo_load_fonts(const char *text);
/* Load the player's create-a-class profile (see profile.rs); the empty profile holds until this runs. */
void zhbo_load_profile(const char *text);
/* Save a custom class from the choose-class wire words into the profile; returns the profile's full
   text (NUL-terminated) for the caller to write to the profile file, or null when the words don't decode. */
const char *zhbo_profile_save(const uint32_t *words, int32_t len);
/* Write the profile from a save_profile one-shot: words are the file's full UTF-8 text packed
   four bytes per word; returns the NUL-terminated text to write, or null when it doesn't decode. */
const char *zhbo_profile_write(const uint32_t *words, int32_t len);
void zhbo_init(const ZhboLogicApi *logic, const ZhboClientApi *client);
int32_t zhbo_install_weapons(const char *table); /* Data\\ConquerOps\\weapons.ini; count or -1 */
void zhbo_reset(void);
void zhbo_logic_take_control(int32_t player, uint32_t general);
void zhbo_logic_input(int32_t player, uint32_t general, const ZhboFpsInput *input);
/* The pick menu's choice (see classes.rs): the General assumes the class at the shared list's index.
   A custom class appends its wire words after the index as [len, word...]; payload/len are null/0 for a
   plain list pick. */
void zhbo_logic_choose_class(int32_t player, uint32_t general, int32_t index, const uint32_t *payload, int32_t len);
void zhbo_logic_step(uint32_t frame);
int32_t zhbo_logic_is_controlled(uint32_t object_id);
void zhbo_client_frame(const ZhboClientIn *in, ZhboClientOut *out);
void zhbo_client_draw_hud(int32_t local_player);
/* The shell create-a-class screen (see ZhboMenuIn above): open it (a fresh screen state), run one
   frame (input in, the close/profile one-shots out; the draw happens inside through the stored
   ZhboClientApi), close it (drop the state). */
void zhbo_menu_open(void);
void zhbo_menu_frame(const ZhboMenuIn *in, ZhboMenuOut *out);
void zhbo_menu_close(void);
const uint16_t *zhbo_string(const char *label);
int32_t zhbo_client_world_poses(ZhboWorldPose *out, int32_t cap);
/* The General's Zero Hour weapon slots (primary, secondary, the third slot's order) under RTS
   control (zhbo_core::sim::RtsWeapons). */
#define ZHBO_RTS_WEAPON_NAME_CAP 48
typedef struct ZhboRtsWeapons
{
	int32_t ammo[3];    /* rounds in the magazine; for the third slot, what its order has left */
	int32_t reserve[3]; /* spare rounds */
	int32_t lock;       /* slot to lock to, -1 none */
	int32_t tactical;   /* tactical grenades left */
	int32_t tactical_selected; /* the offhand orders use the tactical (else the lethal) */
	/* ABI 22: */
	int32_t third;      /* the third slot's order: 0 lethal, 1 tactical, 2 one knife strike */
	int32_t lethal;     /* lethals left */
	int32_t orders;     /* orders that can be given, bit per `third` value */
	char templates[3][ZHBO_RTS_WEAPON_NAME_CAP]; /* the Zero Hour weapon each slot must hold, or "" to keep it */
} ZhboRtsWeapons;
/* target: where the third slot's throw lands (the AI's victim or attack point), or null. */
void zhbo_logic_rts_update(int32_t player, uint32_t general, int32_t current_slot, const uint32_t *shots, const ZhboVec3 *target, ZhboRtsWeapons *out);
/* The third slot's next order (`third` above). Lockstep: from MSG_ZHBO_TAKE_CONTROL. */
void zhbo_logic_rts_offhand(int32_t player, uint32_t general, int32_t choice);
void zhbo_client_rts_ammo(uint32_t general, ZhboRtsWeapons *out);
/* ABI 22: the reload order for the gun in `current_slot` (or the one he goes back to); returns the
   slot topped up, whose Zero Hour weapon must start reloading, or -1. Lockstep: from
   MSG_ZHBO_TAKE_CONTROL. */
int32_t zhbo_logic_rts_reload(int32_t player, uint32_t general, int32_t current_slot);
/* ABI 22: a General's kit icons for his command bar (zhbo_core::sim::RtsIcons), drawn over the
   plain tiles of his gun and order buttons; "" while he is classless. */
enum { ZHBO_RTS_ICON_PRIMARY, ZHBO_RTS_ICON_SECONDARY, ZHBO_RTS_ICON_LETHAL, ZHBO_RTS_ICON_TACTICAL, ZHBO_RTS_ICON_KNIFE, ZHBO_RTS_ICONS };
typedef struct ZhboRtsIcons
{
	char icons[ZHBO_RTS_ICONS][ZHBO_RTS_WEAPON_NAME_CAP];
} ZhboRtsIcons;
void zhbo_client_rts_icons(uint32_t general, ZhboRtsIcons *out);
/* The command bar's class buttons, in button order (the five presets then the profile's ten slots);
   returns how many rows were written. */
int32_t zhbo_class_buttons(ZhboClassButton *out, int32_t cap);
/* Non-zero once the General has assumed a class (the bar shows the class set until then). */
int32_t zhbo_general_has_class(uint32_t general);
int32_t zhbo_world_mesh_count(int32_t vm); /* -2: the tablet */
int32_t zhbo_loadout_viewmodels(int32_t *out, int32_t cap); /* the General's kit's viewmodels */
/* The first-person FXList names for viewmodel `vm`'s weapon (Track R8): its muzzle flash and shell
   eject, converted from BO2's `viewFlashEffect`/`viewShellEjectEffect`. Non-zero when the weapon is
   in the table; an empty string means the def has none (BO2 ejects no brass for it — only the
   muzzle flash has a fallback). */
int32_t zhbo_viewmodel_fx(int32_t vm, char *flash, int32_t flash_cap, char *shell, int32_t shell_cap);
/* ABI 26: the FXList BO2 plays for `impact_type` on `surface` (a ZHBO_SURF_*); `head`/`fatal` pick
   the flesh variant. Non-zero when the table has the cell (an empty name means nothing converted);
   the core owns the table, so the bridge names no FXList. */
int32_t zhbo_impact_fx(int32_t impact_type, int32_t surface, int32_t head, int32_t fatal, char *out, int32_t cap);
float zhbo_general_build_rate(void); /* a General's building speed, share of a dozer's */
int32_t zhbo_check_requirements(char *msg, uint32_t cap);

/* The native first-person viewmodel (Track R; nativevm/ffi.rs, CONTRACT.md "Native viewmodel").
   On by default; ZHBO_NATIVE_VIEWMODEL=0/false/no/off falls back to the W3D conversion. Each
   frame: zhbo_native_viewmodel, then answer every zhbo_native_wanted path with
   zhbo_native_provide (len < 0: not in the archive), and log each zhbo_native_take_log line. */
typedef struct ZhboNativeLight
{
	float dir[3];   /* towards the light, world space */
	float color[3]; /* display-encoded */
} ZhboNativeLight;
typedef struct ZhboNativeIn
{
	float eye[3];
	float dir[3];
	float offset[3]; /* the gun's view-space offset and yaw/pitch/roll, as the W3D path takes them */
	float angles[3];
	int32_t clip;
	int32_t vm;      /* converted weapon index (ZV<vm><faction>) */
	int32_t faction; /* the arms' letter: 'U', 'C', 'G' */
	float time;
	float ads;
	float rate;      /* clip playback rate, 1 = BO2's own */
	float ambient[3];
	ZhboNativeLight lights[3];
	int32_t light_count;
	uint16_t target_view; /* the engine's scene view (drawn into) */
	uint16_t before_view; /* the view the pass runs before (the scene composite) */
	float debug;          /* ZHBO_BO2_DEBUG's term view, 0 off */
} ZhboNativeIn;
#define ZHBO_NATIVE_NOT_NATIVE 0 /* draw the W3D model */
#define ZHBO_NATIVE_DRAWN 1      /* drawn natively: hide the W3D model */
#define ZHBO_NATIVE_LOADING 2    /* files still coming: the W3D model this frame */
int32_t zhbo_native_viewmodel(const ZhboNativeIn *in);
int32_t zhbo_native_wanted(int32_t i, char *buf, int32_t cap);
void zhbo_native_provide(const char *path, const uint8_t *data, int32_t len);
int32_t zhbo_native_take_log(char *buf, int32_t cap);
int32_t zhbo_native_bone(const char *name, float *out12); /* world 3x4 rows of the last pose */
void zhbo_native_reset(void);

/* The native third person (Track R7; nativebody/ffi.rs, CONTRACT.md "The native third person").
   ZHBO_NATIVE_BODY=1 draws the weapon in the General's hand while the W3D body stays; 2 draws the
   body too. Each frame: zhbo_client_native_poses, then zhbo_native_body for each General, then
   answer every zhbo_native_body_wanted path with zhbo_native_body_provide (len < 0: not in the
   archive) and log each zhbo_native_body_take_log line. */
typedef struct ZhboNativePose
{
	uint32_t general;
	int32_t weapon;  /* converted weapon index (WeaponDef::viewmodel), -1 none */
	int32_t stance;  /* 0 stand, 1 crouch, 2 prone */
	int32_t state;   /* zhbo_native::thirdperson::state clip id */
	float time;      /* seconds into the clip */
	float rate;      /* clip playback rate, 1 = BO2's own */
	int32_t sidearm; /* the second gun is in hand */
} ZhboNativePose;
typedef struct ZhboNativeBodyIn
{
	uint32_t general;
	int32_t mode;    /* ZHBO_BODY_WEAPON or ZHBO_BODY_FULL */
	int32_t vm;      /* converted weapon index, -1 none */
	int32_t faction; /* the body's letter: 'U', 'C', 'G' */
	int32_t state;
	float time;
	float rate;
	float pos[3];    /* the General's feet, world units */
	float yaw;       /* radians, 0 = +X, counter-clockwise */
	float eye[3];    /* the camera's world position (the shader's specular term) */
	float tag[12];   /* the engine drawable's tag_weapon_right world transform, 3x4 rows */
	int32_t tag_valid;
	float ambient[3];
	ZhboNativeLight lights[3];
	int32_t light_count;
	uint16_t target_view;
	uint16_t before_view;
	float debug;
} ZhboNativeBodyIn;
#define ZHBO_BODY_WEAPON 1 /* the weapon only: the W3D body draws */
#define ZHBO_BODY_FULL 2   /* the body and the weapon: the bridge hides the W3D body */
int32_t zhbo_client_native_poses(ZhboNativePose *out, int32_t cap, uint32_t local_general, int32_t local_moving);
int32_t zhbo_native_body_enabled(void); /* ZHBO_NATIVE_BODY's mode; the bridge skips its work at 0 */
int32_t zhbo_native_body(const ZhboNativeBodyIn *in);
int32_t zhbo_native_body_ready(const ZhboNativeBodyIn *in); /* 1 when the frame can be drawn */
int32_t zhbo_native_body_wanted(int32_t i, char *buf, int32_t cap);
void zhbo_native_body_provide(const char *path, const uint8_t *data, int32_t len);
int32_t zhbo_native_body_take_log(char *buf, int32_t cap);
int32_t zhbo_native_body_bone(const char *name, float *out12); /* world 3x4 rows of the last pose */
void zhbo_native_body_reset(void);

#ifdef __cplusplus
}

static_assert(sizeof(ZhboObjInfo) == 56, "ZhboObjInfo layout must match host.rs");
static_assert(sizeof(ZhboFpsInput) == 20, "ZhboFpsInput layout must match input.rs");
static_assert(sizeof(ZhboClientIn) == 136, "ZhboClientIn layout must match client.rs");
static_assert(sizeof(ZhboClientOut) == 4832, "ZhboClientOut layout must match client.rs");
static_assert(sizeof(ZhboClientHit) == 40, "ZhboClientHit layout must match client.rs");
static_assert(sizeof(ZhboMenuIn) == 56, "ZhboMenuIn layout must match menu.rs");
static_assert(sizeof(ZhboMenuOut) == 4108, "ZhboMenuOut layout must match menu.rs");
static_assert(sizeof(ZhboLogicApi) == ZHBO_LOGIC_API_FIELDS * sizeof(void *), "ZhboLogicApi layout must match host.rs");
static_assert(sizeof(ZhboWorldPose) == 16, "ZhboWorldPose layout must match ffi.rs");
static_assert(sizeof(ZhboRtsWeapons) == 192, "ZhboRtsWeapons layout must match sim/rts.rs");
static_assert(sizeof(ZhboRtsIcons) == 240, "ZhboRtsIcons layout must match sim/rts.rs");
static_assert(sizeof(ZhboClassButton) == 756, "ZhboClassButton layout must match ffi.rs");
static_assert(sizeof(ZhboClientApi) == ZHBO_CLIENT_API_FIELDS * sizeof(void *), "ZhboClientApi layout must match host.rs");
static_assert(sizeof(ZhboBuildOption) == 64, "ZhboBuildOption layout must match building.rs");
static_assert(sizeof(ZhboWorkSite) == 64, "ZhboWorkSite layout must match building.rs");
static_assert(sizeof(ZhboNativeLight) == 24, "ZhboNativeLight layout must match nativevm/ffi.rs");
static_assert(sizeof(ZhboNativeIn) == 168, "ZhboNativeIn layout must match nativevm/ffi.rs");
static_assert(sizeof(ZhboNativePose) == 28, "ZhboNativePose layout must match nativebody/ffi.rs");
static_assert(sizeof(ZhboNativeBodyIn) == 204, "ZhboNativeBodyIn layout must match nativebody/ffi.rs");
#endif
