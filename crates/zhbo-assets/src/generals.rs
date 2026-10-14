//! The General as a Black Ops II soldier: W3D models, textures, animations and the object INI that
//! uses them, all converted from the player's own BO2 install into `ConquerOps_BO2.big`.
//!
//! The generated `Data\INI\Object\ZZZ_ZHBO_General.ini` replaces the one in `ConquerOps.big` (mod
//! archives load in name order, later ones overriding), swapping only each General's Draw module.
//! If conversion fails nothing is added and the Generals keep their Zero Hour art.

use crate::character::{self, Rig, HIERARCHY};
use crate::images;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use zhbo_formats::ipak::{Ipak, IpakEntry};
use zhbo_formats::iwi::Rgba;
use zhbo_formats::t6zone::Zone;
use zhbo_formats::{fastfile, t6zone, w3d, xanim, xmodel};

const BASE_INI: &str = include_str!("../../../data/Data/INI/Object/ZZZ_ZHBO_General.ini");
const INI_PATH: &str = "Data\\INI\\Object\\ZZZ_ZHBO_General.ini";
const MAX_TEXTURE: u32 = 512;
const WEAPON: &str = "t6_wpn_ar_an94_world";

pub struct Faction {
    /// Zero Hour object to restyle.
    pub object: &'static str,
    pub zone: &'static str,
    pub body: &'static str,
    /// W3D model name (at most 15 characters).
    pub model: &'static str,
    /// First-person arms, and the viewmodel W3D name (`ZhboViewmodel.cpp` picks it by object).
    pub hands: &'static str,
    pub viewmodel: &'static str,
}

pub const FACTIONS: &[Faction] = &[
    Faction {
        object: "ZHBO_General_USA",
        zone: "faction_seals_mp",
        body: "c_usa_mp_seal6_assault_fb",
        model: "ZHBO_USA_SKN",
        hands: "c_usa_mp_seal6_longsleeve_viewhands",
        viewmodel: "ZHBO_VMUSA_SKN",
    },
    Faction {
        object: "ZHBO_General_China",
        zone: "faction_pla_mp",
        body: "c_chn_mp_pla_assault_fb",
        model: "ZHBO_CHN_SKN",
        hands: "c_chn_mp_pla_longsleeve_viewhands",
        viewmodel: "ZHBO_VMCHN_SKN",
    },
    Faction {
        object: "ZHBO_General_GLA",
        zone: "faction_pmc_mp",
        body: "c_mul_mp_pmc_assault_fb",
        model: "ZHBO_GLA_SKN",
        hands: "c_mul_mp_pmc_longsleeve_viewhands",
        viewmodel: "ZHBO_VMGLA_SKN",
    },
];

pub const VM_HIERARCHY: &str = "ZHBO_VM_SKL";
/// Viewmodel clips, in `zhbo_core::viewmodel` order (IDLE, FIRE, RELOAD, RELOAD_EMPTY, SPRINT, RAISE).
pub const VM_ANIMS: &[(&str, &str)] = &[
    ("ZHBO_VM_IDL", "viewmodel_an94_idle"),
    ("ZHBO_VM_FIR", "viewmodel_an94_fire"),
    ("ZHBO_VM_RLD", "viewmodel_an94_reload"),
    ("ZHBO_VM_RLE", "viewmodel_an94_reload_empty"),
    ("ZHBO_VM_SPR", "viewmodel_an94_sprint_loop"),
    ("ZHBO_VM_RAI", "viewmodel_an94_pullout"),
];
/// The scorestreak tablet: each faction's arms holding it, on its own hierarchy, with clips in
/// `zhbo_core::viewmodel` order after the weapon's (TABLET_PULLOUT, TABLET_IDLE, TABLET_PUTAWAY).
/// Its W3D models are the weapon viewmodel's names with `_TAB` for `_SKN`.
pub const TABLET_HIERARCHY: &str = "ZHBO_VMT_SKL";
pub const TABLET_ANIMS: &[(&str, &str)] = &[
    ("ZHBO_VMT_PUL", "viewmodel_tablet_pullout"),
    ("ZHBO_VMT_IDL", "viewmodel_tablet_idle"),
    ("ZHBO_VMT_PUT", "viewmodel_tablet_putaway"),
];
const TABLET: &str = "t6_wpn_tablet_view";
const TABLET_SCREEN: &str = "zhbo_bo2_tablet_screen.tga";

/// The tablet viewmodel's W3D name for a weapon viewmodel's.
pub fn tablet_model(viewmodel: &str) -> String {
    viewmodel.replace("_SKN", "_TAB")
}
const VM_WEAPON: &str = "t6_wpn_ar_an94_view";
const VM_MAGAZINE: &str = "t6_attach_mag_an94_view";

/// (W3D animation name, BO2 animation in common_mp).
pub const ANIMS: &[(&str, &str)] = &[
    ("ZHBO_PLR_IDL", "pb_stand_alert"),
    ("ZHBO_PLR_RUN", "pb_combatrun_forward_loop"),
    ("ZHBO_PLR_ADS", "pb_stand_ads"),
    ("ZHBO_PLR_DTA", "pb_death_faceplant"),
    ("ZHBO_PLR_DTB", "pb_death_flyback"),
];

fn load_zone(bo2_dir: &Path, name: &str) -> Result<Zone, String> {
    let path = bo2_dir.join("zone").join("all").join(format!("{name}.ff"));
    let file = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let zone = fastfile::read(&file).map_err(|e| format!("{name}: {e}"))?;
    t6zone::load(&zone).map_err(|e| format!("{name}: {e}"))
}

/// Image packs, opened on demand, with their entries grouped by name hash.
struct Paks {
    paks: Vec<(Ipak, BTreeMap<u32, Vec<IpakEntry>>)>,
}

impl Paks {
    fn open(bo2_dir: &Path) -> Result<Paks, String> {
        let dir = bo2_dir.join("zone").join("all");
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "ipak"))
            .collect();
        names.sort();
        let mut paks = Vec::new();
        for p in names {
            let pak = Ipak::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let groups = images::group_by_name(&pak);
            paks.push((pak, groups));
        }
        Ok(Paks { paks })
    }

    fn image(&self, hash: u32) -> Option<Rgba> {
        self.paks.iter().find_map(|(pak, groups)| groups.get(&hash).and_then(|parts| images::load(pak, parts).ok()))
    }
}

/// Halve an image until it fits `max` (box filter).
fn shrink(mut img: Rgba, max: u32) -> Rgba {
    while img.width > max || img.height > max {
        let (w, h) = ((img.width / 2).max(1), (img.height / 2).max(1));
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (2 * x + dx).min(img.width - 1);
                        let sy = (2 * y + dy).min(img.height - 1);
                        sum += img.pixels[((sy * img.width + sx) * 4 + c) as usize] as u32;
                    }
                    out[((y * w + x) * 4 + c) as usize] = (sum / 4) as u8;
                }
            }
        }
        img = Rgba { width: w, height: h, pixels: out };
    }
    img
}

/// Textures for a model's surfaces: the colour map of each, written once per name into `files`, with
/// the surface's shine in its alpha (see [`bake_shine`]). Decals (clan tag, emblem: colour maps that are
/// mostly clear, which BO2 blends over the weapon) are listed in `decals` instead.
fn textures(
    model: &xmodel::Model,
    paks: &Paks,
    files: &mut BTreeMap<String, Vec<u8>>,
    decals: &mut BTreeSet<String>,
) -> Vec<Option<String>> {
    model
        .materials
        .iter()
        .map(|m| {
            let m = m.as_ref()?;
            let img = m.color_map()?;
            let name = character::texture_file(&img.name);
            let path = format!("Art\\Textures\\{name}");
            if !files.contains_key(&path) && !decals.contains(&name) {
                let mut rgba = shrink(paks.image(img.stream_hash)?, MAX_TEXTURE);
                if is_decal(&rgba) {
                    decals.insert(name.clone());
                    return Some(name);
                }
                let spec = m.image(xmodel::TS_SPECULAR_MAP).and_then(|s| paks.image(s.stream_hash));
                bake_shine(&mut rgba, spec.as_ref());
                files.insert(path, images::tga_bytes(&rgba));
            }
            Some(name)
        })
        .collect()
}

/// Mostly transparent: at least half the texels below half alpha.
fn is_decal(img: &Rgba) -> bool {
    let clear = img.pixels.chunks_exact(4).filter(|p| p[3] < 128).count();
    clear * 2 >= img.pixels.len() / 4
}

/// How strongly the brightest, glossiest specular texel reflects the environment.
const SHINE: f32 = 0.85;

/// Put a surface's shine in its colour map's alpha: BO2's specular map gives the specular colour in
/// RGB (bare metal bright, cloth and polymer dark) and gloss in alpha. Surfaces without one stay matte.
/// The colour maps' own alpha is unused: these meshes are drawn opaque, without alpha test.
fn bake_shine(color: &mut Rgba, spec: Option<&Rgba>) {
    let (w, h) = (color.width, color.height);
    for y in 0..h {
        for x in 0..w {
            let shine = spec.map_or(0.0, |s| {
                let (sx, sy) = (x * s.width / w, y * s.height / h);
                let p = &s.pixels[((sy * s.width + sx) * 4) as usize..][..4];
                let level = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
                let gloss = p[3] as f32 / 255.0;
                level * (0.3 + 0.7 * gloss) * SHINE
            });
            color.pixels[((y * w + x) * 4 + 3) as usize] = (shine.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// The environment the shiny surfaces reflect.
const ENV_MAP: &str = "zhbo_bo2_env.tga";

/// Drop decal surfaces and reflect the environment on every textured mesh (weighted by the shine
/// baked into its texture).
fn finish(meshes: &mut Vec<w3d::SkinMesh>, decals: &BTreeSet<String>) {
    meshes.retain(|m| m.texture.as_ref().is_none_or(|t| !decals.contains(t)));
    for m in meshes.iter_mut().filter(|m| m.texture.is_some()) {
        m.env_map = Some(ENV_MAP.to_string());
    }
}

/// An outdoor environment in BO2's muted grade, for W3D's world-space environment mapper (`u, v` =
/// `0.5 + 0.5 * reflection.xy`, so the centre is straight up or down and the rim is the horizon): a
/// pale overcast sky with a brighter sun side, a bright horizon band, and dark ground and skyline.
/// BO2's own reflection probes are cube maps stored in the map zone's temp block, which the zone
/// loader does not keep.
fn env_map() -> Rgba {
    const N: u32 = 128;
    let mut pixels = Vec::with_capacity((N * N * 4) as usize);
    let sun = (0.6f32, 0.45f32);
    for y in 0..N {
        for x in 0..N {
            let (rx, ry) = ((x as f32 + 0.5) / N as f32 * 2.0 - 1.0, (y as f32 + 0.5) / N as f32 * 2.0 - 1.0);
            let rho = (rx * rx + ry * ry).sqrt().min(1.0);
            let az = ry.atan2(rx);
            // Up and down share a texel: average the sky above and the ground below.
            let sky = [0.42, 0.46, 0.52];
            let ground = [0.10, 0.09, 0.08];
            let mut c: [f32; 3] = std::array::from_fn(|i| 0.5 * (sky[i] + ground[i]));
            // Horizon band, broken by a skyline of buildings and trees.
            let horizon = ((rho - 0.72) / 0.28).clamp(0.0, 1.0).powf(1.5);
            let skyline = 0.5 + 0.5 * ((az * 7.0).sin() * (az * 13.0 + 1.3).sin()).max(-0.2);
            let band = [0.62, 0.62, 0.60];
            for i in 0..3 {
                c[i] += (band[i] * skyline - c[i]) * horizon;
            }
            // The sun side.
            let d = ((rx - sun.0).powi(2) + (ry - sun.1).powi(2)).sqrt();
            let glow = (-d * d * 9.0).exp() * 0.45;
            let warm = [1.0, 0.92, 0.78];
            for i in 0..3 {
                c[i] += glow * warm[i];
            }
            pixels.extend(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
            pixels.push(255);
        }
    }
    Rgba { width: N, height: N, pixels }
}

/// The Draw module for a General with a BO2 model.
fn draw_module(model: &str) -> String {
    let a = |n: &str| format!("{HIERARCHY}.{n}");
    format!(
        "  Draw = W3DModelDraw ModuleTag_01\n\
         \x20   OkToChangeModelColor = Yes\n\
         \x20   DefaultConditionState\n\
         \x20     Model               = {model}\n\
         \x20     Animation           = {idle}\n\
         \x20     AnimationMode       = LOOP\n\
         \x20     Flags               = RANDOMSTART\n\
         \x20     WeaponFireFXBone    = PRIMARY tag_flash\n\
         \x20   End\n\
         \x20   ConditionState        = MOVING\n\
         \x20     Animation           = {run}\n\
         \x20     AnimationMode       = LOOP\n\
         \x20     Flags               = RANDOMSTART\n\
         \x20     ParticleSysBone     = None InfantryDustTrails\n\
         \x20   End\n\
         \x20   AliasConditionState   = MOVING FIRING_A\n\
         \x20   AliasConditionState   = MOVING BETWEEN_FIRING_SHOTS_A\n\
         \x20   AliasConditionState   = MOVING RELOADING_A\n\
         \x20   ConditionState        = FIRING_A\n\
         \x20     Animation           = {ads}\n\
         \x20     AnimationMode       = LOOP\n\
         \x20   End\n\
         \x20   AliasConditionState   = BETWEEN_FIRING_SHOTS_A\n\
         \x20   AliasConditionState   = RELOADING_A\n\
         \x20   ConditionState        = DYING\n\
         \x20     Animation           = {dta}\n\
         \x20     Animation           = {dtb}\n\
         \x20     AnimationMode       = ONCE\n\
         \x20   End\n\
         \x20 End\n",
        idle = a("ZHBO_PLR_IDL"),
        run = a("ZHBO_PLR_RUN"),
        ads = a("ZHBO_PLR_ADS"),
        dta = a("ZHBO_PLR_DTA"),
        dtb = a("ZHBO_PLR_DTB"),
    )
}

/// Replace `object`'s Draw module in `ini` (the base General INI) with `draw`.
pub fn replace_draw(ini: &str, object: &str, draw: &str) -> Result<String, String> {
    let header = format!("Object {object}\n");
    let start = ini.find(&header).ok_or_else(|| format!("{object} not in the General INI"))? + header.len();
    let draw_at = start + ini[start..].find("  Draw = W3DModelDraw").ok_or("no Draw module")?;
    let end_rel = ini[draw_at..].find("\n  End\n").ok_or("unterminated Draw module")?;
    let end = draw_at + end_rel + "\n  End\n".len();
    Ok(format!("{}{}{}", &ini[..draw_at], draw, &ini[end..]))
}

/// The first-person viewmodels: each faction's arms holding the AN-94 and its magazine, on one
/// shared hierarchy, with the clips the core's viewmodel state machine plays.
fn viewmodels(common: &Zone, zones: &[Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>) -> Result<String, String> {
    let gun = xmodel::find(common, VM_WEAPON).map_err(|e| e.to_string())?;
    let mag = xmodel::find(common, VM_MAGAZINE).map_err(|e| e.to_string())?;
    let mut rig: Option<Rig> = None;
    let mut made = Vec::new();
    let mut decals = BTreeSet::new();
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&gun, "tag_weapon"), (&mag, "j_gun")]));
        let hands_tex = textures(&hands, paks, out, &mut decals);
        let gun_tex = textures(&gun, paks, out, &mut decals);
        let mag_tex = textures(&mag, paks, out, &mut decals);
        let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
        meshes.extend(character::skin(rig, &gun, "G", &gun_tex));
        meshes.extend(character::skin(rig, &mag, "M", &mag_tex));
        finish(&mut meshes, &decals);
        out.insert(format!("Art\\W3D\\{}.w3d", f.viewmodel), w3d::skin_model(f.viewmodel, VM_HIERARCHY, &meshes));
        made.push(f.viewmodel);
    }
    let rig = rig.ok_or("no viewmodel arms")?;
    out.insert(format!("Art\\W3D\\{VM_HIERARCHY}.w3d"), rig.hierarchy_file(VM_HIERARCHY));
    for (w3d_name, bo2_name) in VM_ANIMS {
        let anim = xanim::find(common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, VM_HIERARCHY, frames, fps, &channels));
    }
    tablets(common, zones, paks, out, &mut decals)?;
    Ok(format!("viewmodels {} with tablets (decals dropped: {})", made.join(", "), decals.into_iter().collect::<Vec<_>>().join(" ")))
}

/// Each faction's arms holding the scorestreak tablet, with its lit screen.
fn tablets(common: &Zone, zones: &[Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, decals: &mut BTreeSet<String>) -> Result<(), String> {
    let tablet = xmodel::find(common, TABLET).map_err(|e| e.to_string())?;
    let mut tablet_tex = textures(&tablet, paks, out, decals);
    // The screen's material layers several images in a shader; draw one composite, self-lit.
    let screen_surface = tablet.materials.iter().position(|m| m.as_ref().is_some_and(|m| m.name.contains("screen")));
    if let Some(i) = screen_surface {
        if let Some(mut img) = tablet.materials[i].as_ref().and_then(|m| tablet_screen(m, paks)) {
            // No reflection over the lit screen (alpha is the shine).
            bake_shine(&mut img, None);
            out.insert(format!("Art\\Textures\\{TABLET_SCREEN}"), images::tga_bytes(&img));
            tablet_tex[i] = Some(TABLET_SCREEN.to_string());
        }
    }
    let mut rig: Option<Rig> = None;
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&tablet, "tag_weapon")]));
        let hands_tex = textures(&hands, paks, out, decals);
        let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
        let mut screen = character::skin(rig, &tablet, "T", &tablet_tex);
        for m in &mut screen {
            m.emissive = m.texture.as_deref() == Some(TABLET_SCREEN);
        }
        meshes.extend(screen);
        finish(&mut meshes, decals);
        let name = tablet_model(f.viewmodel);
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, TABLET_HIERARCHY, &meshes));
    }
    let rig = rig.ok_or("no tablet arms")?;
    out.insert(format!("Art\\W3D\\{TABLET_HIERARCHY}.w3d"), rig.hierarchy_file(TABLET_HIERARCHY));
    for (w3d_name, bo2_name) in TABLET_ANIMS {
        let anim = xanim::find(common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, TABLET_HIERARCHY, frames, fps, &channels));
    }
    Ok(())
}

/// The scorestreak screen as BO2 shows it: the background UI, the satellite panels over it, and the
/// targeting reticle over the panels.
fn tablet_screen(m: &xmodel::Material, paks: &Paks) -> Option<Rgba> {
    let layer = |part: &str| {
        m.textures
            .iter()
            .filter_map(|t| t.image.as_ref())
            .find(|i| i.name.contains(part))
            .and_then(|i| paks.image(i.stream_hash))
    };
    let mut screen = layer("background")?;
    if let Some(top) = layer("killstreaks_top") {
        let size = (screen.width, screen.height);
        over(&mut screen, &top, (0, 0), size);
    }
    if let Some(reticle) = layer("working_top") {
        let size = screen.height / 2;
        let at = (screen.width * 37 / 100 - size / 2, screen.height * 45 / 100 - size / 2);
        over(&mut screen, &reticle, at, (size, size));
    }
    Some(screen)
}

/// Alpha-blend `src`, scaled to `size`, over `dst` at `at`.
fn over(dst: &mut Rgba, src: &Rgba, at: (u32, u32), size: (u32, u32)) {
    for y in 0..size.1.min(dst.height.saturating_sub(at.1)) {
        for x in 0..size.0.min(dst.width.saturating_sub(at.0)) {
            let (sx, sy) = (x * src.width / size.0, y * src.height / size.1);
            let s = &src.pixels[((sy * src.width + sx) * 4) as usize..][..4];
            let a = s[3] as u32;
            let d = ((at.1 + y) * dst.width + at.0 + x) as usize * 4;
            for c in 0..3 {
                let v = (s[c] as u32 * a + dst.pixels[d + c] as u32 * (255 - a)) / 255;
                dst.pixels[d + c] = v as u8;
            }
        }
    }
}

/// Convert everything; returns the archive files and a one-line summary.
pub fn files(bo2_dir: &Path) -> Result<(Vec<(String, Vec<u8>)>, String), String> {
    let common = load_zone(bo2_dir, "common_mp")?;
    let weapon = xmodel::find(&common, WEAPON).map_err(|e| e.to_string())?;
    let paks = Paks::open(bo2_dir)?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut ini = BASE_INI.to_string();
    let mut rig: Option<Rig> = None;
    let mut summary = Vec::new();

    out.insert(format!("Art\\Textures\\{ENV_MAP}"), images::tga_bytes(&env_map()));
    let mut zones = Vec::new();
    let mut decals = BTreeSet::new();
    for f in FACTIONS {
        let zone = load_zone(bo2_dir, f.zone)?;
        let body = xmodel::find(&zone, f.body).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&body, &[(&weapon, character::WEAPON_TAG)]));
        let body_tex = textures(&body, &paks, &mut out, &mut decals);
        let weapon_tex = textures(&weapon, &paks, &mut out, &mut decals);
        let mut meshes = character::skin(rig, &body, "B", &body_tex);
        meshes.extend(character::skin(rig, &weapon, "W", &weapon_tex));
        finish(&mut meshes, &decals);
        out.insert(format!("Art\\W3D\\{}.w3d", f.model), w3d::skin_model(f.model, HIERARCHY, &meshes));
        ini = replace_draw(&ini, f.object, &draw_module(f.model))?;
        summary.push(format!("{} = {} ({} meshes)", f.object, f.body, meshes.len()));
        zones.push(zone);
    }
    if !decals.is_empty() {
        summary.push(format!("decals dropped: {}", decals.into_iter().collect::<Vec<_>>().join(" ")));
    }
    let rig = rig.ok_or("no bodies")?;
    out.insert(format!("Art\\W3D\\{HIERARCHY}.w3d"), rig.hierarchy_file(HIERARCHY));
    for (w3d_name, bo2_name) in ANIMS {
        let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, HIERARCHY, frames, fps, &channels));
    }
    summary.push(viewmodels(&common, &zones, &paks, &mut out)?);

    // BO2 sound effects, including the viewmodel clips' sound notetracks.
    let world = load_zone(bo2_dir, crate::sfx::WORLD_ZONE)?;
    let clips = VM_ANIMS
        .iter()
        .chain(TABLET_ANIMS)
        .map(|(_, bo2)| xanim::find(&common, bo2).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let (sound_files, sound_summary) = crate::sfx::files(&common, &world, bo2_dir, &clips)?;
    out.extend(sound_files);
    summary.push(sound_summary);

    // First-person muzzle flash with BO2's flash sprite.
    let fx_zone = load_zone(bo2_dir, crate::fx::ZONE)?;
    out.extend(crate::fx::files(&fx_zone, &|hash| paks.image(hash))?);
    out.insert(INI_PATH.to_string(), ini.into_bytes());
    let textures = out.keys().filter(|k| k.ends_with(".tga")).count();
    Ok((out.into_iter().collect(), format!("{}; {textures} textures, {} animations", summary.join(", "), ANIMS.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, p: [u8; 4]) -> Rgba {
        Rgba { width: w, height: h, pixels: p.repeat((w * h) as usize) }
    }

    #[test]
    fn shine_follows_the_specular_map() {
        let mut metal = solid(4, 4, [90, 90, 90, 255]);
        bake_shine(&mut metal, Some(&solid(2, 2, [255, 255, 255, 255])));
        assert!(metal.pixels.chunks_exact(4).all(|p| p[3] > 200 && p[..3] == [90, 90, 90]));
        let mut cloth = solid(4, 4, [90, 90, 90, 255]);
        bake_shine(&mut cloth, Some(&solid(8, 8, [30, 30, 30, 40])));
        assert!(cloth.pixels.chunks_exact(4).all(|p| p[3] < 20));
        let mut plain = solid(2, 2, [90, 90, 90, 255]);
        bake_shine(&mut plain, None);
        assert!(plain.pixels.chunks_exact(4).all(|p| p[3] == 0), "no specular map: matte");
    }

    #[test]
    fn decals_are_mostly_clear() {
        assert!(is_decal(&solid(4, 4, [255, 255, 255, 0])));
        assert!(!is_decal(&solid(4, 4, [255, 255, 255, 255])));
    }

    #[test]
    fn environment_is_brighter_at_the_horizon_and_sun() {
        let env = env_map();
        let at = |x: u32, y: u32| env.pixels[((y * env.width + x) * 4) as usize];
        let n = env.width;
        let centre = at(n / 2, n / 2);
        assert!(at(n / 2, n - 2) > centre || at(1, n / 2) > centre, "horizon brighter than straight up/down");
        assert!(at((n as f32 * 0.8) as u32, (n as f32 * 0.72) as u32) > centre, "sun side");
    }

    #[test]
    fn replaces_only_the_named_draw_module() {
        for f in FACTIONS {
            let out = replace_draw(BASE_INI, f.object, &draw_module(f.model)).unwrap();
            assert!(out.contains(&format!("Model               = {}", f.model)));
            // The rest of the object (weapons, body, locomotor) is untouched.
            assert_eq!(out.matches("Object ZHBO_General_").count(), 3);
            assert_eq!(out.matches("WeaponSet").count(), BASE_INI.matches("WeaponSet").count());
            // Every Draw module still closes before the object's other modules.
            assert_eq!(out.matches("Draw = W3DModelDraw").count(), 3);
        }
        assert!(FACTIONS.iter().all(|f| f.model.len() <= 15));
        assert!(ANIMS.iter().all(|a| a.0.len() <= 15) && HIERARCHY.len() <= 15);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn converts_from_real_install() {
        let bo2 = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let (files, summary) = files(Path::new(&bo2)).unwrap();
        println!("{summary}");
        assert!(files.iter().any(|(p, _)| p == INI_PATH));
        assert!(files.iter().filter(|(p, _)| p.ends_with(".w3d")).count() >= 1 + FACTIONS.len() + ANIMS.len());
    }
}
