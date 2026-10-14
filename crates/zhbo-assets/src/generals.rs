//! The General as a Black Ops II soldier: W3D models, textures, animations and the object INI that
//! uses them, all converted from the player's own BO2 install into `ConquerOps_BO2.big`.
//!
//! The generated `Data\INI\Object\ZZZ_ZHBO_General.ini` replaces the one in `ConquerOps.big` (mod
//! archives load in name order, later ones overriding), swapping only each General's Draw module.
//! If conversion fails nothing is added and the Generals keep their Zero Hour art.

use crate::character::{self, Rig, HIERARCHY};
use crate::images;
use std::collections::BTreeMap;
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

/// Textures for a model's surfaces: the colour map of each, written once per name into `files`.
fn textures(model: &xmodel::Model, paks: &Paks, files: &mut BTreeMap<String, Vec<u8>>) -> Vec<Option<String>> {
    model
        .materials
        .iter()
        .map(|m| {
            let img = m.as_ref()?.color_map()?;
            let name = character::texture_file(&img.name);
            let path = format!("Art\\Textures\\{name}");
            if !files.contains_key(&path) {
                let rgba = paks.image(img.stream_hash)?;
                files.insert(path, images::tga_bytes(&shrink(rgba, MAX_TEXTURE)));
            }
            Some(name)
        })
        .collect()
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
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&gun, "tag_weapon"), (&mag, "j_gun")]));
        let hands_tex = textures(&hands, paks, out);
        let gun_tex = textures(&gun, paks, out);
        let mag_tex = textures(&mag, paks, out);
        let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
        meshes.extend(character::skin(rig, &gun, "G", &gun_tex));
        meshes.extend(character::skin(rig, &mag, "M", &mag_tex));
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
    Ok(format!("viewmodels {}", made.join(", ")))
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

    let mut zones = Vec::new();
    for f in FACTIONS {
        let zone = load_zone(bo2_dir, f.zone)?;
        let body = xmodel::find(&zone, f.body).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&body, &[(&weapon, character::WEAPON_TAG)]));
        let body_tex = textures(&body, &paks, &mut out);
        let weapon_tex = textures(&weapon, &paks, &mut out);
        let mut meshes = character::skin(rig, &body, "B", &body_tex);
        meshes.extend(character::skin(rig, &weapon, "W", &weapon_tex));
        out.insert(format!("Art\\W3D\\{}.w3d", f.model), w3d::skin_model(f.model, HIERARCHY, &meshes));
        ini = replace_draw(&ini, f.object, &draw_module(f.model))?;
        summary.push(format!("{} = {} ({} meshes)", f.object, f.body, meshes.len()));
        zones.push(zone);
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
    out.insert(INI_PATH.to_string(), ini.into_bytes());
    let textures = out.keys().filter(|k| k.ends_with(".tga")).count();
    Ok((out.into_iter().collect(), format!("{}; {textures} textures, {} animations", summary.join(", "), ANIMS.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

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
