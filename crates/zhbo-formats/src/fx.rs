//! Black Ops II (T6) effects (`FxEffectDef`): an effect is a list of elements, each spawning
//! particles (sprites, tails, clouds), models, lights, sounds, decals or other effects, with its
//! spawn count and delay, lifetime, starting volume, velocity, gravity, and colour, alpha and size
//! over its life. Read from a loaded zone (`t6zone::Zone`) by following an `FxEffectDef` pointer
//! (a weapon's explosion effect, for instance). Times are milliseconds, distances inches.

use crate::t6zone::layouts::{FxEffectDef as FD, FxElemDef as FE};
use crate::t6zone::Zone;
use crate::xmodel::{read_material, Material};

/// `FxElemType`.
pub mod kind {
    pub const SPRITE_BILLBOARD: u8 = 0;
    pub const SPRITE_ORIENTED: u8 = 1;
    pub const SPRITE_ROTATED: u8 = 2;
    pub const TAIL: u8 = 3;
    pub const LINE: u8 = 4;
    pub const TRAIL: u8 = 5;
    pub const CLOUD: u8 = 6;
    pub const MODEL: u8 = 7;
    pub const OMNI_LIGHT: u8 = 8;
    pub const SPOT_LIGHT: u8 = 9;
    pub const SOUND: u8 = 10;
    pub const DECAL: u8 = 11;
    pub const RUNNER: u8 = 12;

    /// Elements drawn with a material (camera-facing or oriented quads).
    pub fn is_sprite(k: u8) -> bool {
        matches!(k, SPRITE_BILLBOARD | SPRITE_ORIENTED | SPRITE_ROTATED | TAIL | LINE | TRAIL | CLOUD)
    }
}

/// A base value and a random amount added to it (`FxFloatRange`, `FxIntRange`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    pub base: f32,
    pub amplitude: f32,
}

impl Range {
    pub fn min(self) -> f32 {
        self.base
    }
    pub fn max(self) -> f32 {
        self.base + self.amplitude
    }
}

/// The look of an element at one point in its life (`FxElemVisStateSample`, base plus random
/// amplitude).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VisSample {
    /// RGBA, 0..255.
    pub color: [u8; 4],
    pub color_amplitude: [u8; 4],
    /// Width and height (inches; sprites are this far from the centre to the edge).
    pub size: [f32; 2],
    pub size_amplitude: [f32; 2],
    pub scale: f32,
    pub rotation_total: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Elem {
    pub kind: u8,
    pub flags: u32,
    /// One-shot elements: how many spawn; looping ones: every `interval` msec, `count` at a time.
    pub count: Range,
    pub looping: bool,
    pub interval_msec: i32,
    pub delay_msec: Range,
    pub life_msec: Range,
    pub origin: [Range; 3],
    pub offset_radius: Range,
    pub offset_height: Range,
    pub gravity: Range,
    pub angular_velocity: [Range; 3],
    /// Velocity at spawn in the effect's frame and in the world (inches per second): base and
    /// random amplitude per axis.
    pub velocity: ([f32; 3], [f32; 3]),
    pub world_velocity: ([f32; 3], [f32; 3]),
    /// Colour, alpha and size over life, evenly spaced.
    pub vis: Vec<VisSample>,
    pub materials: Vec<Material>,
    pub models: Vec<String>,
    /// Effects a runner element starts.
    pub children: Vec<Effect>,
    /// The names of the effects a runner starts, whether or not this zone holds them.
    pub runs: Vec<String>,
    /// Flipbook atlas: columns and rows of the image (1 x 1 if none).
    pub atlas: (u32, u32),
    /// How the flipbook plays (`FxElemAtlas`).
    pub flipbook: Flipbook,
    /// The velocity graph: `velIntervalCount + 1` evenly spaced samples over the element's life.
    pub vel: Vec<VelSample>,
    /// Starting angles (radians) and the sprite's starting roll.
    pub spawn_angles: [Range; 3],
    pub initial_rotation: Range,
    /// Effects started where the element dies, where it hits something, and along its path
    /// (`effectOnDeath`, `effectOnImpact`, `effectEmitted`; the names, and the parsed effects when
    /// this zone holds them).
    pub on_death: Option<Box<Effect>>,
    pub on_impact: Option<Box<Effect>>,
    pub emitted: Option<Box<Effect>>,
    /// Distance between two emitted effects (inches).
    pub emit_dist: Range,
    /// Draw order among the effect's elements.
    pub sort_order: u8,
    /// Distance culling: the spawn range and the fade-in/out ranges (inches from the camera;
    /// 0 = none).
    pub spawn_range: Range,
    pub fade_in_range: Range,
    pub fade_out_range: Range,
    /// The effect class the element belongs to: emission elements are spawned by their effect's
    /// movement, never on its start.
    pub emission: bool,
}

/// `FxElemAtlas`: the flipbook's start frame, play mode, rate and frame count.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flipbook {
    /// `behavior`: bits 0-1 the start frame (0 fixed `index`, 1 random, 2 indexed by the spawn
    /// order), bit 2 play over the element's life, bit 3 loop only `loop_count` times.
    pub behavior: u8,
    pub index: u8,
    /// Frames per second when not played over life.
    pub fps: u8,
    pub loop_count: u8,
    /// Frames in the atlas (the low byte of `entryCountAndIndexRange`).
    pub frames: u32,
}

impl Flipbook {
    pub const START_MASK: u8 = 3;
    pub const START_RANDOM: u8 = 1;
    pub const START_INDEXED: u8 = 2;
    pub const PLAY_OVER_LIFE: u8 = 4;
    pub const LOOP_N_TIMES: u8 = 8;

    /// Whether the element changes frame while it lives.
    pub fn animates(&self) -> bool {
        self.frames > 1 && (self.behavior & Self::PLAY_OVER_LIFE != 0 || self.fps > 0)
    }
}

/// One sample of the velocity graph (`FxElemVelStateSample`): velocity (inches per msec) and the
/// displacement integrated to this point of the life (inches per msec of life: multiply by the
/// element's life), in the effect's frame and in the world, each a base and a random amplitude.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelSample {
    pub local_velocity: ([f32; 3], [f32; 3]),
    pub local_delta: ([f32; 3], [f32; 3]),
    pub world_velocity: ([f32; 3], [f32; 3]),
    pub world_delta: ([f32; 3], [f32; 3]),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub name: String,
    /// `efPriority`: how BO2 ranks the effect when its effect budget is full.
    pub priority: u8,
    pub looping_life_msec: i32,
    pub life_msec: i32,
    pub elems: Vec<Elem>,
}

fn range(z: &Zone, at: u32) -> Range {
    Range { base: z.f32(at), amplitude: z.f32(at + 4) }
}

fn int_range(z: &Zone, at: u32) -> Range {
    Range { base: z.u32(at) as i32 as f32, amplitude: z.u32(at + 4) as i32 as f32 }
}

fn vec3(z: &Zone, at: u32) -> [f32; 3] {
    [z.f32(at), z.f32(at + 4), z.f32(at + 8)]
}

fn vis_state(z: &Zone, at: u32) -> ([u8; 4], [f32; 2], f32, f32) {
    let c = [z.u8(at), z.u8(at + 1), z.u8(at + 2), z.u8(at + 3)];
    (c, [z.f32(at + 12), z.f32(at + 16)], z.f32(at + 20), z.f32(at + 8))
}

/// Read the `FxEffectDef` at `addr` (child effects too, `depth` levels deep).
pub fn read(z: &Zone, addr: u32) -> Effect {
    read_depth(z, addr, 2)
}

fn read_depth(z: &Zone, addr: u32, depth: u32) -> Effect {
    let looping = z.u16(addr + FD::elemDefCountLooping) as i16 as i32;
    let one_shot = z.u16(addr + FD::elemDefCountOneShot) as i16 as i32;
    let emission = z.u16(addr + FD::elemDefCountEmission) as i16 as i32;
    let total = (looping + one_shot + emission).clamp(0, 64) as u32;
    let elems_at = z.u32(addr + FD::elemDefs);
    let mut elems = Vec::new();
    if elems_at != 0 {
        for i in 0..total {
            let e = elems_at + i * FE::SIZE;
            elems.push(read_elem(z, e, (i as i32) < looping, (i as i32) >= looping + one_shot, depth));
        }
    }
    Effect {
        name: z.str_ptr(addr + FD::name),
        priority: z.u8(addr + FD::efPriority),
        looping_life_msec: z.u32(addr + FD::msecLoopingLife) as i32,
        life_msec: z.u32(addr + FD::msecNonLoopingLife) as i32,
        elems,
    }
}

fn read_elem(z: &Zone, e: u32, looping: bool, emission: bool, depth: u32) -> Elem {
    let kind = z.u8(e + FE::elemType);
    let visual_count = z.u8(e + FE::visualCount) as u32;
    // One visual is stored in place; several are an array of pointers.
    let visuals: Vec<u32> = if visual_count == 1 {
        vec![z.u32(e + FE::visuals)]
    } else {
        let arr = z.u32(e + FE::visuals);
        if arr == 0 {
            Vec::new()
        } else {
            (0..visual_count.min(16)).map(|i| z.u32(arr + 4 * i)).collect()
        }
    };
    let visuals: Vec<u32> = visuals.into_iter().filter(|p| *p != 0).collect();
    let mut elem = Elem {
        kind,
        flags: z.u32(e + FE::flags),
        looping,
        delay_msec: int_range(z, e + FE::spawnDelayMsec),
        life_msec: int_range(z, e + FE::lifeSpanMsec),
        origin: [range(z, e + FE::spawnOrigin), range(z, e + FE::spawnOrigin + 8), range(z, e + FE::spawnOrigin + 16)],
        offset_radius: range(z, e + FE::spawnOffsetRadius),
        offset_height: range(z, e + FE::spawnOffsetHeight),
        gravity: range(z, e + FE::gravity),
        angular_velocity: [range(z, e + FE::angularVelocity), range(z, e + FE::angularVelocity + 8), range(z, e + FE::angularVelocity + 16)],
        spawn_angles: [range(z, e + FE::spawnAngles), range(z, e + FE::spawnAngles + 8), range(z, e + FE::spawnAngles + 16)],
        initial_rotation: range(z, e + FE::initialRotation),
        emit_dist: range(z, e + FE::emitDist),
        sort_order: z.u8(e + FE::sortOrder),
        spawn_range: range(z, e + FE::spawnRange),
        fade_in_range: range(z, e + FE::fadeInRange),
        fade_out_range: range(z, e + FE::fadeOutRange),
        emission,
        flipbook: Flipbook {
            behavior: z.u8(e + FE::atlas),
            index: z.u8(e + FE::atlas + 1),
            fps: z.u8(e + FE::atlas + 2),
            loop_count: z.u8(e + FE::atlas + 3),
            frames: (z.u16(e + FE::atlas + 6) as u32 & 0xff).max(1),
        },
        ..Default::default()
    };
    if looping {
        elem.interval_msec = z.u32(e + FE::spawn) as i32;
        elem.count = Range { base: z.u32(e + FE::spawn + 4) as i32 as f32, amplitude: 0.0 };
    } else {
        elem.count = int_range(z, e + FE::spawn);
    }
    let vel = z.u32(e + FE::velSamples);
    if vel != 0 {
        elem.velocity = (vec3(z, vel), vec3(z, vel + 12));
        elem.world_velocity = (vec3(z, vel + 48), vec3(z, vel + 60));
        let n = z.u8(e + FE::velIntervalCount) as u32 + 1;
        let pair = |at: u32| (vec3(z, at), vec3(z, at + 12));
        elem.vel = (0..n.min(64))
            .map(|i| {
                let s = vel + i * 96;
                VelSample { local_velocity: pair(s), local_delta: pair(s + 24), world_velocity: pair(s + 48), world_delta: pair(s + 72) }
            })
            .collect();
    }
    // The effects an element starts when it dies, hits, or moves.
    if depth > 0 {
        let child = |at: u32| -> Option<Box<Effect>> {
            let p = z.u32(e + at);
            if p == 0 {
                return None;
            }
            let name = z.cstr(p);
            let name = name.trim_start_matches(',');
            name.contains('/').then(|| find_depth(z, name, depth - 1)).flatten().map(Box::new)
        };
        elem.on_death = child(FE::effectOnDeath);
        elem.on_impact = child(FE::effectOnImpact);
        elem.emitted = child(FE::effectEmitted);
    }
    let vis = z.u32(e + FE::visSamples);
    if vis != 0 {
        let n = z.u8(e + FE::visStateIntervalCount) as u32 + 1;
        for i in 0..n.min(32) {
            let s = vis + i * 48;
            let (color, size, scale, rotation_total) = vis_state(z, s);
            let (color_amplitude, size_amplitude, _, _) = vis_state(z, s + 24);
            elem.vis.push(VisSample { color, color_amplitude, size, size_amplitude, scale, rotation_total });
        }
    }
    let bits = z.u8(e + FE::atlas + 4) as u32;
    let row_bits = z.u8(e + FE::atlas + 5) as u32;
    elem.atlas = (1 << bits.min(5), 1 << row_bits.min(5));
    match kind {
        k if kind::is_sprite(k) => elem.materials = visuals.iter().map(|&m| read_material(z, m)).collect(),
        kind::MODEL => elem.models = visuals.iter().map(|&m| z.str_ptr(m).trim_start_matches(',').to_string()).collect(),
        // A runner names the effect it starts. An `FxEffectDefRef` is loaded as its name (an
        // xstring: `Load_FxEffectDefRef`), so the visual is the `char*` itself, not a pointer to
        // one. Look the effect up in this zone.
        kind::RUNNER => {
            elem.runs = visuals.iter().map(|&f| z.cstr(f).trim_start_matches(',').to_string()).collect();
            if depth == 0 {
                return elem;
            }
            elem.children = visuals
                .iter()
                .filter_map(|&f| {
                    let name = z.cstr(f);
                    let name = name.trim_start_matches(',');
                    if name.contains('/') {
                        find_depth(z, name, depth - 1)
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => {}
    }
    elem
}

/// The `FxEffectDef` named `name` in a zone, if it is there.
/// The effect a def's pointer names: the pointed-at asset, or — when it is a reference stub
/// (`,weapon/shellejects/fx_rifle`: a name with no elements, the full asset living in a later zone
/// such as `patch_mp`) — the stub itself, name only, for [`bind`] to resolve against the pool.
pub fn read_or_stub(z: &Zone, addr: u32) -> Option<Effect> {
    (addr != 0).then(|| read(z, addr))
}

/// A parsed effect as BO2's asset pool binds it: the full asset of that name from the first zone in
/// `pool` (highest priority first) that holds one — every reference to a name resolves to the
/// pool's live asset (`DB_LinkXAssetEntry`, `FUN_00597000`: `patch_mp`'s `fx_pistol` over
/// `common_mp`'s own copy) — else the parse itself when it has elements; `None` for a stub no
/// zone defines.
pub fn bind(e: Option<Effect>, pool: &[&Zone]) -> Option<Effect> {
    let e = e?;
    let name = e.name.trim_start_matches(',');
    pool.iter().find_map(|z| find(z, name)).filter(|f| !f.elems.is_empty()).or_else(|| (!e.elems.is_empty()).then_some(e))
}

/// Bind every element's reference-stub material (`,gfx_fxt_smk_puff` in `common_mp`: a name with
/// no textures, the full material living in another zone of the pool) to the full material of that
/// name in the first zone of `pool` that has one, as BO2's asset pool does; runners' effects too.
/// A stub nothing resolves stays (its element draws nothing).
pub fn bind_materials(e: &mut Effect, pool: &[&Zone]) {
    for el in &mut e.elems {
        for m in &mut el.materials {
            if m.color_map().is_none() {
                if let Some(full) = pool.iter().find_map(|z| crate::xmodel::find_material(z, &m.name).filter(|f| f.color_map().is_some())) {
                    *m = full;
                }
            }
        }
        for c in el.children.iter_mut().chain(el.on_death.as_deref_mut()).chain(el.on_impact.as_deref_mut()).chain(el.emitted.as_deref_mut()) {
            bind_materials(c, pool);
        }
    }
}

pub fn find(z: &Zone, name: &str) -> Option<Effect> {
    find_depth(z, name, 2)
}

fn find_depth(z: &Zone, name: &str, depth: u32) -> Option<Effect> {
    let kind = crate::fastfile::asset_type_id("fx")?;
    z.assets
        .iter()
        .find(|a| a.kind == kind && z.str_ptr(a.addr + FD::name).trim_start_matches(',') == name)
        .map(|a| read_depth(z, a.addr, depth))
        .filter(|e| !e.elems.is_empty())
}

/// BO2's surface types (`materialSurfType_t`, `T6_Assets.h:5064-5099`): which explosion an impact
/// plays depends on the ground. `FLESH` is the bit index BO2 keeps *out* of `nonflesh[]` — a
/// soldier's hit uses the four `flesh[]` cells instead (`impact_flesh_variant`).
pub mod surface {
    pub const DEFAULT: usize = 0;
    pub const BRICK: usize = 2;
    pub const CONCRETE: usize = 5;
    pub const DIRT: usize = 6;
    pub const FLESH: usize = 7;
    pub const GRASS: usize = 10;
    pub const METAL: usize = 13;
    pub const ROCK: usize = 17;
    pub const SAND: usize = 18;
    pub const SNOW: usize = 19;
    pub const WATER: usize = 20;
    pub const WOOD: usize = 21;
    pub const ASPHALT: usize = 22;
}

/// The impact table's row for a bullet's `impactType` (an entry hit): small (pistols, SMGs) 0,
/// large (rifles) 3, armour-piercing 7, extreme 9, shotgun 5 — `IMPACT_KINDS`, from BO2's
/// `FUN_00533210`. (Rows 1/4/6/8/10 are the exit hits; before this was read from the switch the
/// small bucket used row 1, its exit row, which ships only fatal-exit blood.)
pub fn bullet_entry(impact_type: u32) -> Option<u32> {
    impact_kind(impact_type).filter(|_| is_bullet(impact_type)).map(|k| k.row)
}

/// The blood a bullet of table entry `entry` raises on a soldier (the first of its flesh effects).
pub fn impact_flesh(z: &Zone, entry: u32) -> Option<Effect> {
    use crate::t6zone::layouts::{FxImpactEntry as IE, FxImpactTable as IT};
    let kind = crate::fastfile::asset_type_id("impactfx")?;
    let table = z.assets.iter().find(|a| a.kind == kind)?;
    let entries = z.u32(table.addr + IT::table);
    if entries == 0 || entry > 32 {
        return None;
    }
    (0..4).map(|i| z.u32(entries + entry * IE::SIZE + IE::flesh + 4 * i)).find(|p| *p != 0).map(|p| read(z, p)).filter(|e| !e.elems.is_empty())
}

/// One of the four blood cells of table entry `entry`, by BO2's `FxFleshType` (0 body nonfatal,
/// 1 body fatal, 2 head nonfatal, 3 head fatal). A null cell falls back to the body-nonfatal one,
/// the way a null non-flesh cell falls back to `SURF_TYPE_DEFAULT`.
pub fn impact_flesh_variant(z: &Zone, entry: u32, variant: usize) -> Option<Effect> {
    use crate::t6zone::layouts::{FxImpactEntry as IE, FxImpactTable as IT};
    let kind = crate::fastfile::asset_type_id("impactfx")?;
    let table = z.assets.iter().find(|a| a.kind == kind)?;
    let entries = z.u32(table.addr + IT::table);
    if entries == 0 || entry > 32 || variant >= 4 {
        return None;
    }
    [variant, 0]
        .into_iter()
        .map(|v| z.u32(entries + entry * IE::SIZE + IE::flesh + 4 * v as u32))
        .find(|p| *p != 0)
        .map(|p| read(z, p))
        .filter(|e| !e.elems.is_empty())
}

/// What BO2's impact routine plays for an entry hit (no exit/vehicle flags) of one `impactType`:
/// the `FxImpactTable` row the FX comes from and the sound family, whose alias for a surface is
/// `<family>_<surface>` (`FUN_00533210`, t6mp; families registered by `FUN_004d4440` through
/// `FUN_004d42d0`, which hashes `"%s_%s"` with the 32 surface names). The projectile-explode event
/// (`FUN_004c0ac0`) and the grenade/equipment explode event (`FUN_004c1590`) both call it with the
/// surface the projectile hit, play the row's cell and the alias, and then the def's own
/// `projExplosionEffect`/`projExplosionSound` on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpactKind {
    /// BO2's `impactType` (the enum: 0 none, 1 bullet_small … 15 blade).
    pub impact_type: u32,
    /// The `FxImpactTable` row (`FUN_00533210`'s `uVar3` for an entry hit).
    pub row: u32,
    /// The sound family (`None`: the def's own field — `bounceSound`, `projDudSound` — which the
    /// mod does not carry).
    pub sound_family: Option<&'static str>,
    /// The row and family of an exit hit (impact flag 4, set where a penetrating round leaves a
    /// solid: `FUN_005d0350`/`FUN_00538e40`): `None` for the types that never penetrate. The
    /// families are `FUN_004d4440`'s strings (`0xb1a530` … `0xb1a498`); the shotgun's
    /// `prj_bulletspray_impact_small_exit` names no alias the install ships (its bank has
    /// `prj_bulletspray_small_exit_*`), so BO2's hash misses and a pellet's exit is silent.
    pub exit: Option<(u32, &'static str)>,
}

/// BO2's `impactType` -> impact row and sound family, transcribed from the switch in
/// `FUN_00533210` and the family-table addresses `FUN_004d4440` loads before each
/// `FUN_004d42d0` call (`ESI = 0x1061b88` grenade_explode, `0x1061c08` rifle_grenade, `0x1061c88`
/// rocket_explode, `0x1061d08` rocket_explode_xtreme, `0x1061d88` mortar_shell, `0x1061e08`
/// tank_shell, `0x1061f08` bullet_impact_small … `0x1062208` blade). The one source of this mapping:
/// the impact FX, the explosion FX and the explosion sounds all read it. Notes from the switch:
/// `rocket_explode_xtreme` (10) reads the plain `wpn_rocket_explode` table (its own family is
/// registered but unread), `mortar_shell`/`tank_shell` (12/13) prefer the def's
/// `mortarShellSound`/`tankShellSound` (+0x724/+0x728) when set (no MP def sets them), and the
/// rows skip 1/2/4/6/8/10 (exit hits) and 18.
pub const IMPACT_KINDS: [ImpactKind; 15] = [
    ImpactKind { impact_type: 1, row: 0, sound_family: Some("prj_bullet_impact_small"), exit: Some((1, "prj_bullet_impact_small_exit")) },
    ImpactKind { impact_type: 2, row: 3, sound_family: Some("prj_bullet_impact_large"), exit: Some((4, "prj_bullet_impact_large_exit")) },
    ImpactKind { impact_type: 3, row: 7, sound_family: Some("prj_bullet_impact_ap"), exit: Some((8, "prj_bullet_impact_ap_exit")) },
    ImpactKind { impact_type: 4, row: 9, sound_family: Some("prj_bullet_impact_xtreme"), exit: Some((10, "prj_bullet_impact_xtreme_exit")) },
    ImpactKind { impact_type: 5, row: 5, sound_family: Some("prj_bulletspray_small"), exit: Some((6, "prj_bulletspray_impact_small_exit")) },
    ImpactKind { impact_type: 6, row: 11, sound_family: None, exit: None },
    ImpactKind { impact_type: 7, row: 12, sound_family: Some("wpn_grenade_explode"), exit: None },
    ImpactKind { impact_type: 8, row: 12, sound_family: Some("wpn_rifle_grenade"), exit: None },
    ImpactKind { impact_type: 9, row: 13, sound_family: Some("wpn_rocket_explode"), exit: None },
    ImpactKind { impact_type: 10, row: 14, sound_family: Some("wpn_rocket_explode"), exit: None },
    ImpactKind { impact_type: 11, row: 15, sound_family: None, exit: None },
    ImpactKind { impact_type: 12, row: 16, sound_family: Some("wpn_mortar_shell"), exit: None },
    ImpactKind { impact_type: 13, row: 17, sound_family: Some("wpn_tank_shell"), exit: None },
    ImpactKind { impact_type: 14, row: 19, sound_family: Some("prj_bolt_impact"), exit: Some((19, "prj_bolt_impact_exit")) },
    ImpactKind { impact_type: 15, row: 20, sound_family: Some("prj_blade_impact"), exit: None },
];

/// The impact kind of `impact_type` (`None` for 0, "none", and out-of-range values).
pub fn impact_kind(impact_type: u32) -> Option<ImpactKind> {
    IMPACT_KINDS.iter().copied().find(|k| k.impact_type == impact_type)
}

/// The five bullet impact types (`bullet_small` … `shotgun`); the rest belong to projectiles.
pub fn is_bullet(impact_type: u32) -> bool {
    (1..=5).contains(&impact_type)
}

/// The effect BO2's impact table (`FxImpactTable`) plays for table entry `entry` on `surface`,
/// falling back to the default surface.
pub fn impact(z: &Zone, entry: u32, surface: usize) -> Option<Effect> {
    impact_cell(z, entry, surface).or_else(|| impact_cell(z, entry, surface::DEFAULT))
}

/// The table's own cell for `entry` on `surface`, without the default-surface fallback: what the
/// grenade-bounce and dud events read (t6mp `FUN_004c1590` events 0x47 and 0x51 index
/// `table[row].nonflesh[surface]` and play it only when it is set).
pub fn impact_cell(z: &Zone, entry: u32, surface: usize) -> Option<Effect> {
    use crate::t6zone::layouts::{FxImpactEntry as IE, FxImpactTable as IT};
    let kind = crate::fastfile::asset_type_id("impactfx")?;
    let table = z.assets.iter().find(|a| a.kind == kind)?;
    let entries = z.u32(table.addr + IT::table);
    if entries == 0 || entry > 32 || surface >= 32 {
        return None;
    }
    let p = z.u32(entries + entry * IE::SIZE + IE::nonflesh + 4 * surface as u32);
    (p != 0).then(|| read(z, p)).filter(|e| !e.elems.is_empty())
}

#[cfg(test)]
mod flesh_tests {
    use crate::t6zone::layouts::{FxImpactEntry as IE, FxImpactTable as IT};

    /// `FxImpactEntry` is 32 non-flesh pointers (128 bytes) followed by the four flesh pointers,
    /// and `impact_flesh_variant` reads `IE::flesh + 4 * variant`.
    #[test]
    fn the_four_flesh_cells_sit_after_the_nonflesh_ones() {
        assert_eq!(IE::nonflesh, 0);
        assert_eq!(IE::flesh, 32 * 4);
        assert_eq!(IE::flesh + 4 * 4, IE::SIZE);
        assert_eq!(IE::SIZE, 144);
        assert_eq!(IT::table, 4);
    }
}
