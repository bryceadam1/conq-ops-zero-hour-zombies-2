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
    /// Flipbook atlas: columns and rows of the image (1 x 1 if none).
    pub atlas: (u32, u32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub name: String,
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
            elems.push(read_elem(z, e, (i as i32) < looping, depth));
        }
    }
    Effect {
        name: z.str_ptr(addr + FD::name),
        looping_life_msec: z.u32(addr + FD::msecLoopingLife) as i32,
        life_msec: z.u32(addr + FD::msecNonLoopingLife) as i32,
        elems,
    }
}

fn read_elem(z: &Zone, e: u32, looping: bool, depth: u32) -> Elem {
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
        // A runner names the effect it starts (`FxEffectDefRef`); look it up in this zone.
        kind::RUNNER if depth > 0 => {
            elem.children = visuals
                .iter()
                .filter_map(|&f| {
                    let name = z.str_ptr(f);
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

/// BO2's surface types (`surfaceType`): which explosion an impact plays depends on the ground.
pub mod surface {
    pub const DEFAULT: usize = 0;
    pub const DIRT: usize = 6;
    pub const GRASS: usize = 10;
}

/// The impact table's entry for an explosive weapon's `impactType` (the table orders its entries
/// differently from the enum): grenades and rifle grenades, rockets, and shells.
pub fn explosion_entry(impact_type: u32) -> Option<u32> {
    match impact_type {
        7 | 8 => Some(12),
        9 | 10 => Some(13),
        12 | 13 => Some(17),
        _ => None,
    }
}

/// The effect BO2's impact table (`FxImpactTable`) plays for table entry `entry` on `surface`,
/// falling back to the default surface.
pub fn impact(z: &Zone, entry: u32, surface: usize) -> Option<Effect> {
    let impact_type = entry;
    use crate::t6zone::layouts::{FxImpactEntry as IE, FxImpactTable as IT};
    let kind = crate::fastfile::asset_type_id("impactfx")?;
    let table = z.assets.iter().find(|a| a.kind == kind)?;
    let entries = z.u32(table.addr + IT::table);
    if entries == 0 || impact_type == 0 || impact_type > 32 {
        return None;
    }
    let entry = entries + impact_type * IE::SIZE;
    [surface, surface::DEFAULT]
        .into_iter()
        .map(|s| z.u32(entry + IE::nonflesh + 4 * s as u32))
        .find(|p| *p != 0)
        .map(|p| read(z, p))
        .filter(|e| !e.elems.is_empty())
}
