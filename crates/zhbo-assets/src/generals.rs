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
    /// Letter in the weapon viewmodels' W3D names ([`vm_model`]).
    pub letter: char,
}

pub const FACTIONS: &[Faction] = &[
    Faction {
        object: "ZHBO_General_USA",
        zone: "faction_seals_mp",
        body: "c_usa_mp_seal6_assault_fb",
        model: "ZHBO_USA_SKN",
        hands: "c_usa_mp_seal6_longsleeve_viewhands",
        viewmodel: "ZHBO_VMUSA_SKN",
        letter: 'U',
    },
    Faction {
        object: "ZHBO_General_China",
        zone: "faction_pla_mp",
        body: "c_chn_mp_pla_assault_fb",
        model: "ZHBO_CHN_SKN",
        hands: "c_chn_mp_pla_longsleeve_viewhands",
        viewmodel: "ZHBO_VMCHN_SKN",
        letter: 'C',
    },
    Faction {
        object: "ZHBO_General_GLA",
        zone: "faction_pmc_mp",
        body: "c_mul_mp_pmc_assault_fb",
        model: "ZHBO_GLA_SKN",
        hands: "c_mul_mp_pmc_longsleeve_viewhands",
        viewmodel: "ZHBO_VMGLA_SKN",
        letter: 'G',
    },
];

/// W3D names of weapon viewmodel `vm` (shared with `host/zhbo/ZhboViewmodel.cpp`): the hierarchy,
/// each faction's arms holding the weapon, and the clip for each BO2 animation slot.
pub fn vm_hierarchy(vm: i32) -> String {
    format!("ZV{vm:02}_SKL")
}
pub fn vm_model(vm: i32, letter: char) -> String {
    format!("ZV{vm:02}{letter}_SKN")
}
pub fn vm_anim(vm: i32, slot: usize) -> String {
    format!("ZV{vm:02}_A{slot:02X}")
}
/// Projectile models (thrown grenades, rockets) and the Zero Hour object that shows one.
pub fn projectile_model(vm: i32) -> String {
    format!("ZPR{vm:02}_SKN")
}
pub fn projectile_object(id: &str) -> String {
    format!("ZHBO_Proj_{id}")
}

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

/// (W3D animation name, BO2 animation in common_mp).
pub const ANIMS: &[(&str, &str)] = &[
    ("ZHBO_PLR_IDL", "pb_stand_alert"),
    ("ZHBO_PLR_RUN", "pb_combatrun_forward_loop"),
    ("ZHBO_PLR_ADS", "pb_stand_ads"),
    ("ZHBO_PLR_DTA", "pb_death_faceplant"),
    ("ZHBO_PLR_DTB", "pb_death_flyback"),
    // The scorestreak tablet coming up and going down (the field computer).
    ("ZHBO_PLR_TBU", "pt_tablet_stand_raise"),
    ("ZHBO_PLR_TBD", "pt_tablet_stand_putaway"),
];

/// Third-person sub-objects on the soldier model: weapon `vm`'s meshes, and the tablet's (shared
/// with `host/zhbo/ZhboBridge.cpp`).
pub fn world_mesh(vm: i32, k: usize) -> String {
    format!("W{vm:02}{k:02}")
}
pub fn tablet_mesh(k: usize) -> String {
    format!("TB{k:02}")
}
const TABLET_WORLD: &str = "t6_wpn_tablet_world";
/// The bone BO2's third-person tablet animations carry the tablet on.
const TABLET_TAG: &str = "tag_inhand";

pub(crate) fn load_zone(bo2_dir: &Path, name: &str) -> Result<Zone, String> {
    let path = bo2_dir.join("zone").join("all").join(format!("{name}.ff"));
    let file = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let zone = fastfile::read(&file).map_err(|e| format!("{name}: {e}"))?;
    t6zone::load(&zone).map_err(|e| format!("{name}: {e}"))
}

/// Image packs, opened on demand, with their entries grouped by name hash.
pub(crate) struct Paks {
    paks: Vec<(Ipak, BTreeMap<u32, Vec<IpakEntry>>)>,
}

impl Paks {
    pub(crate) fn open(bo2_dir: &Path) -> Result<Paks, String> {
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

    pub(crate) fn image(&self, hash: u32) -> Option<Rgba> {
        self.paks.iter().find_map(|(pak, groups)| groups.get(&hash).and_then(|parts| images::load(pak, parts).ok()))
    }
}

/// Halve an image until it fits `max` (box filter).
pub(crate) fn shrink(mut img: Rgba, max: u32) -> Rgba {
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
                if is_decal(&name, &rgba) {
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

/// A BO2 decal (clan tag, emblem, visor and watch glass, frost swatch): mostly transparent, and
/// named as one. Weapon colour maps can be mostly clear too (BO2 keeps other data in that alpha).
fn is_decal(name: &str, img: &Rgba) -> bool {
    const DECALS: &[&str] = &["clan", "emblem", "visor", "glass", "swatch", "decal"];
    let clear = img.pixels.chunks_exact(4).filter(|p| p[3] < 128).count();
    DECALS.iter().any(|d| name.contains(d)) && clear * 2 >= img.pixels.len() / 4
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

/// The Draw module for a General with a BO2 model. `hidden` are the sub-objects hidden until the
/// bridge shows them (the weapons not in hand, the tablet); every condition state hides them, and
/// the bridge's own show/hide wins over that (`ZhboBridge.cpp`). USER_1 raises the scorestreak
/// tablet (the field computer), USER_2 puts it away.
fn draw_module(model: &str, hidden: &[String]) -> String {
    let a = |n: &str| format!("{HIERARCHY}.{n}");
    // Zero Hour reads INI lines of up to about a kilobyte; several HideSubObject lines add up.
    let hide: String = hidden.chunks(80).map(|c| format!("\x20     HideSubObject       = {}\n", c.join(" "))).collect();
    format!(
        "  Draw = W3DModelDraw ModuleTag_01\n\
         \x20   OkToChangeModelColor = Yes\n\
         \x20   DefaultConditionState\n\
         \x20     Model               = {model}\n\
         \x20     Animation           = {idle}\n\
         \x20     AnimationMode       = LOOP\n\
         \x20     Flags               = RANDOMSTART\n\
         \x20     WeaponFireFXBone    = PRIMARY tag_flash\n\
         {hide}\
         \x20   End\n\
         \x20   ConditionState        = MOVING\n\
         \x20     Animation           = {run}\n\
         \x20     AnimationMode       = LOOP\n\
         \x20     Flags               = RANDOMSTART\n\
         \x20     ParticleSysBone     = None InfantryDustTrails\n\
         {hide}\
         \x20   End\n\
         \x20   AliasConditionState   = MOVING FIRING_A\n\
         \x20   AliasConditionState   = MOVING BETWEEN_FIRING_SHOTS_A\n\
         \x20   AliasConditionState   = MOVING RELOADING_A\n\
         \x20   ConditionState        = FIRING_A\n\
         \x20     Animation           = {ads}\n\
         \x20     AnimationMode       = LOOP\n\
         {hide}\
         \x20   End\n\
         \x20   AliasConditionState   = BETWEEN_FIRING_SHOTS_A\n\
         \x20   AliasConditionState   = RELOADING_A\n\
         \x20   ConditionState        = USER_1\n\
         \x20     Animation           = {tablet_up}\n\
         \x20     AnimationMode       = ONCE\n\
         {hide}\
         \x20   End\n\
         \x20   ConditionState        = USER_2\n\
         \x20     Animation           = {tablet_down}\n\
         \x20     AnimationMode       = ONCE\n\
         {hide}\
         \x20   End\n\
         \x20   ConditionState        = DYING\n\
         \x20     Animation           = {dta}\n\
         \x20     Animation           = {dtb}\n\
         \x20     AnimationMode       = ONCE\n\
         {hide}\
         \x20   End\n\
         \x20 End\n",
        idle = a("ZHBO_PLR_IDL"),
        run = a("ZHBO_PLR_RUN"),
        ads = a("ZHBO_PLR_ADS"),
        dta = a("ZHBO_PLR_DTA"),
        dtb = a("ZHBO_PLR_DTB"),
        tablet_up = a("ZHBO_PLR_TBU"),
        tablet_down = a("ZHBO_PLR_TBD"),
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

/// The magazine model of a gun's viewmodel: BO2 names them `t6_attach_mag_<weapon>_view` after a
/// part of the gun's own view model name (the zones don't link them for base weapons).
fn magazine(common: &Zone, view_model: &str) -> Option<xmodel::Model> {
    const SKIP: &[&str] = &["t6", "wpn", "ar", "smg", "lmg", "mg", "pistol", "sniper", "shotty", "launch", "view", "base"];
    view_model
        .trim_end_matches("_view")
        .split('_')
        .rev()
        .filter(|t| t.len() >= 3 && !SKIP.contains(t))
        .find_map(|t| xmodel::find(common, &format!("t6_attach_mag_{t}_view")).ok())
}

/// A model as BO2 loads it: `patch_mp` over `common_mp` (some `common_mp` models are empty stubs
/// that the patch replaces, such as the frag grenade's view model).
fn find_model(patch: &Zone, common: &Zone, name: &str) -> Result<xmodel::Model, String> {
    let patched = xmodel::find(patch, name).ok().filter(|m| !m.lods.is_empty());
    patched.map_or_else(|| xmodel::find(common, name).map_err(|e| format!("{name}: {e}")), Ok)
}

/// A notetrack source: viewmodel, clip (BO2 slot or tablet clip) and the BO2 animation.
pub type Clip = (i32, i32, xanim::Anim);

/// What a weapon's attachments change about its art: BO2's `WeaponAttachmentUnique` for each, the
/// animation set to use, the weapon's bones to hide, and the gunshot.
pub struct Combo<'a> {
    /// One per attachment (the single-attachment entry), with the attachment's point.
    pub parts: Vec<(&'a zhbo_formats::weapon::AttachmentUnique, u32)>,
    pub anims: Vec<Option<String>>,
    pub hide: Vec<String>,
    /// An attachment replaces the weapon's own magazine.
    pub no_clip: bool,
    pub fire_sound_player: Option<String>,
    pub fire_sound: Option<String>,
}

/// The art of `w` with the attachments `shorts` (BO2 create-a-class names).
pub fn combo<'a>(w: &'a zhbo_formats::weapon::Weapon, atts: &[zhbo_formats::weapon::Attachment], shorts: &[&str]) -> Combo<'a> {
    use crate::weapons::short_name;
    let mut parts = Vec::new();
    let mut mask = 0u32;
    for short in shorts {
        let Some((kind, id)) = w.attachment_variants.iter().find(|(_, id)| short_name(id) == *short) else { continue };
        mask |= 1 << kind;
        let point = atts.iter().find(|a| &a.name == id).map_or(0, |a| a.point);
        if let Some(u) = w.attachment_uniques.iter().find(|u| u.kind == *kind && u.combined == 0) {
            parts.push((u, point));
        }
    }
    let count = |a: &[Option<String>]| a.iter().filter(|x| x.is_some()).count();
    // The animations: the entry for exactly this combination, else the attachment with the most
    // of its own (a grip or launcher changes the hands), over the weapon's.
    let exact = (shorts.len() > 1).then(|| w.attachment_uniques.iter().find(|u| u.combined == mask)).flatten();
    let source = exact.or_else(|| parts.iter().map(|(u, _)| *u).max_by_key(|u| count(&u.anims)));
    let mut anims = w.anims.clone();
    if let Some(u) = source {
        for (i, a) in u.anims.iter().enumerate() {
            if a.is_some() && i < anims.len() {
                anims[i] = a.clone();
            }
        }
    }
    let mut hide = w.hide_tags.clone();
    for (u, _) in &parts {
        hide.extend(u.hide_tags.iter().cloned());
    }
    if let Some(u) = exact {
        hide.extend(u.hide_tags.iter().cloned());
    }
    hide.sort();
    hide.dedup();
    let sounds = parts.iter().map(|(u, _)| *u).find(|u| u.fire_sound_player.is_some() && u.fire_sound_player != w.fire_sound_player);
    Combo {
        no_clip: parts.iter().any(|(u, _)| u.disable_base_clip),
        fire_sound_player: sounds.and_then(|u| u.fire_sound_player.clone()).or_else(|| w.fire_sound_player.clone()),
        fire_sound: sounds.and_then(|u| u.fire_sound.clone()).or_else(|| w.fire_sound.clone()),
        parts,
        anims,
        hide,
    }
}

/// The BO2 weapon a converted id stands for: `an94_mp+gl+reflex` and `gl_an94_mp~...` are
/// `an94_mp` and `gl_an94_mp`.
pub fn base_id(id: &str) -> &str {
    id.split('~').next().unwrap_or(id).split('+').next().unwrap_or(id)
}

/// The first-person viewmodels of `converted` weapons: each faction's arms holding the weapon, its
/// magazine and its attachments (placed where BO2 puts them, iron sights hidden under an optic), on
/// one hierarchy per weapon, with a W3D clip for every BO2 animation slot it has, and its alt mode's
/// clips after them. Sets each definition's `viewmodel` (and its alt mode's). Returns a summary and
/// the clips (for sounds).
#[allow(clippy::too_many_arguments)]
fn weapon_viewmodels(
    patch: &Zone,
    common: &Zone,
    zones: &[Zone],
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
) -> Result<(String, Vec<Clip>), String> {
    use zhbo_core::attachments::point;
    let mut decals = BTreeSet::new();
    let mut made = Vec::new();
    let mut clips = Vec::new();
    let mut anim_cache: BTreeMap<String, Option<xanim::Anim>> = BTreeMap::new();
    for (vm, id) in converted.iter().enumerate() {
        let vm = vm as i32;
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(view) = w.view_model.as_deref() else { continue };
        let c = combo(w, atts, &shorts);
        let gun = find_model(patch, common, view).map_err(|e| format!("{id}: {e}"))?;
        let mag = if w.is_gun() && !c.no_clip { magazine(common, view) } else { None };
        let mut parts: Vec<(&xmodel::Model, &str)> = vec![(&gun, "tag_weapon")];
        if let Some(m) = &mag {
            parts.push((m, "j_gun"));
        }
        // The attachments' models, each with where it goes.
        let mut extra: Vec<(xmodel::Model, Option<String>, crate::xf::Xf, u32)> = Vec::new();
        for (u, pt) in &c.parts {
            let Some(name) = &u.view_model else { continue };
            let Ok(model) = find_model(patch, common, name) else { continue };
            let offset = character::scaled_xf((crate::xf::from_angles(u.view_rotation), u.view_offset));
            extra.push((model, u.view_model_tag.clone(), offset, *pt));
        }
        let gun_tex = textures(&gun, paks, out, &mut decals);
        let mag_tex = mag.as_ref().map(|m| textures(m, paks, out, &mut decals));
        let extra_tex: Vec<Vec<Option<String>>> = extra.iter().map(|(m, ..)| textures(m, paks, out, &mut decals)).collect();
        let mut rig: Option<Rig> = None;
        for (f, zone) in FACTIONS.iter().zip(zones) {
            let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
            let rig = rig.get_or_insert_with(|| {
                let mut rig = Rig::new(&hands, &parts);
                let root = gun.bones.first().and_then(|b| rig.pivot(&b.name)).unwrap_or(0);
                for (model, tag, offset, pt) in &extra {
                    let at = tag.as_deref().and_then(|t| rig.pivot(t)).unwrap_or(root);
                    rig.attach(model, at, *offset);
                    // Aim through the optic: the line of sight goes up to the middle of its glass.
                    if *pt == point::OPTIC as u32 {
                        if let Some(optic) = model.bones.first().and_then(|b| rig.pivot(&b.name)) {
                            let mid = (model.mins[2] + model.maxs[2]) * 0.5 * character::SCALE;
                            let pos = crate::xf::apply(rig.bind[optic], [0.0, 0.0, mid]);
                            rig.move_pivot("tag_sights", pos);
                        }
                    }
                }
                rig
            });
            let hands_tex = textures(&hands, paks, out, &mut decals);
            let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
            meshes.extend(character::skin_hiding(rig, &gun, "G", &gun_tex, &c.hide));
            if let (Some(m), Some(t)) = (&mag, &mag_tex) {
                meshes.extend(character::skin(rig, m, "M", t));
            }
            for (k, ((model, ..), tex)) in extra.iter().zip(&extra_tex).enumerate() {
                meshes.extend(character::skin(rig, model, &format!("A{k}"), tex));
            }
            finish(&mut meshes, &decals);
            let name = vm_model(vm, f.letter);
            out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &vm_hierarchy(vm), &meshes));
        }
        let rig = rig.ok_or("no viewmodel arms")?;
        let hierarchy = vm_hierarchy(vm);
        out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), rig.hierarchy_file(&hierarchy));
        // The weapon's clips, then its alt mode's after them on the same viewmodel.
        let alt_id = defs.iter().find(|d| &d.id == id).map(|d| d.alt.clone()).unwrap_or_default();
        let alt_anims = (!alt_id.is_empty()).then(|| bo2.iter().find(|a| a.name == base_id(&alt_id))).flatten().map(|a| a.anims.clone());
        let mut sets = vec![(0usize, c.anims.clone())];
        if let Some(a) = alt_anims.clone() {
            sets.push((zhbo_core::weapons::ALT_CLIPS as usize, a));
        }
        let mut n = 0;
        for (base_slot, anims) in &sets {
            for (slot, anim_name) in anims.iter().enumerate() {
                let Some(anim_name) = anim_name else { continue };
                let anim = anim_cache.entry(anim_name.clone()).or_insert_with(|| xanim::find(common, anim_name).ok());
                let Some(anim) = anim else { continue };
                let (frames, channels) = character::animate(&rig, anim);
                let fps = anim.frame_rate.round().max(1.0) as u32;
                let clip = vm_anim(vm, base_slot + slot);
                out.insert(format!("Art\\W3D\\{clip}.w3d"), w3d::animation(&clip, &hierarchy, frames, fps, &channels));
                clips.push((vm, (base_slot + slot) as i32, anim.clone()));
                n += 1;
            }
        }
        let mask = |a: &[Option<String>]| a.iter().enumerate().take(128).filter(|(_, x)| x.is_some()).fold(0u128, |m, (i, _)| m | (1u128 << i));
        if let Some(d) = defs.iter_mut().find(|d| &d.id == id) {
            d.viewmodel = vm;
            d.anims = mask(&c.anims);
        }
        if let (Some(a), Some(d)) = (alt_anims, defs.iter_mut().find(|d| d.id == alt_id)) {
            d.viewmodel = vm;
            d.clip_base = zhbo_core::weapons::ALT_CLIPS;
            d.anims = mask(&a);
        }
        made.push(format!(
            "{id} ({n} clips{}{})",
            if mag.is_some() { ", magazine" } else { "" },
            if extra.is_empty() { String::new() } else { format!(", {} attachment models", extra.len()) }
        ));
    }
    Ok((format!("viewmodels {} (decals dropped: {})", made.join(", "), decals.into_iter().collect::<Vec<_>>().join(" ")), clips))
}

/// The third-person weapons on the soldier model: each converted weapon's BO2 world model and its
/// attachments' (at BO2's world offsets), rigid on the right hand as `W<vm><k>` meshes, so the bridge
/// shows the one in hand. Sets each definition's `world_meshes` (its alt mode shares them).
#[allow(clippy::too_many_arguments)]
fn world_weapons(
    patch: &Zone,
    common: &Zone,
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    rig: &Rig,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    decals: &mut BTreeSet<String>,
) -> Vec<w3d::SkinMesh> {
    let hand = rig.pivot(character::WEAPON_TAG).unwrap_or(0);
    let mut meshes = Vec::new();
    for (vm, id) in converted.iter().enumerate() {
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(world) = w.world_model.as_deref().and_then(|m| find_model(patch, common, m).ok()) else { continue };
        let c = combo(w, atts, &shorts);
        let prefix = format!("W{vm:02}");
        let tex = textures(&world, paks, out, decals);
        let mut mine = character::skin_rigid(&world, hand, crate::xf::IDENTITY, &prefix, 0, &tex, &c.hide);
        for (u, _) in &c.parts {
            let Some(model) = u.world_model.as_deref().and_then(|m| find_model(patch, common, m).ok()) else { continue };
            // From the world model's own tag, or its origin.
            let tag = u.world_model_tag.as_deref().and_then(|t| world.bones.iter().find(|b| b.name == t));
            let base_xf = tag.map_or(crate::xf::IDENTITY, |b| character::scaled_xf((b.rotation, b.position)));
            let offset = crate::xf::compose(base_xf, character::scaled_xf((crate::xf::from_angles(u.world_rotation), u.world_offset)));
            let t = textures(&model, paks, out, decals);
            let first = mine.len();
            mine.extend(character::skin_rigid(&model, hand, offset, &prefix, first, &t, &[]));
        }
        finish(&mut mine, decals);
        let count = mine.len().min(99) as u8;
        for d in defs.iter_mut().filter(|d| &d.id == id || d.alt == *id && d.id.contains('~')) {
            d.world_meshes = count;
        }
        meshes.extend(mine);
    }
    meshes
}

/// Thrown and launched things for `converted` weapons: their BO2 projectile (or world) model as a
/// W3D model, and a Zero Hour object that shows it (`ZHBO_Proj_<id>`). Sets `projectile_object`.
fn projectiles(patch: &Zone, common: &Zone, paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, bo2: &[zhbo_formats::weapon::Weapon], defs: &mut [zhbo_core::weapons::WeaponDef], converted: &[String]) -> String {
    use zhbo_core::weapons::Class;
    let mut ini = String::from("; Generated by zhbo-assets: thrown, launched and placed BO2 weapons, moved by zhbo-core.\n\n");
    let mut decals = BTreeSet::new();
    let mut made = Vec::new();
    // The converted weapons and their alt modes (an under-barrel launcher's grenade).
    let mut ids: Vec<String> = converted.to_vec();
    for id in converted {
        if let Some(alt) = defs.iter().find(|d| &d.id == id).map(|d| d.alt.clone()).filter(|a| !a.is_empty()) {
            ids.push(alt);
        }
    }
    for (n, id) in ids.iter().enumerate() {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(d) = defs.iter_mut().find(|d| &d.id == id) else { continue };
        let throws = d.class.is_offhand() || d.class == Class::Launcher || d.projectile_speed > 0.0;
        if !throws || d.class == Class::Melee {
            continue;
        }
        let Some(model_name) = w.projectile_model.clone().or_else(|| w.world_model.clone()) else { continue };
        let Ok(model) = find_model(patch, common, &model_name) else { continue };
        let rig = Rig::new(&model, &[]);
        let tex = textures(&model, paks, out, &mut decals);
        let mut meshes = character::skin(&rig, &model, "P", &tex);
        finish(&mut meshes, &decals);
        let name = projectile_model(n as i32);
        let hierarchy = name.replace("_SKN", "_SKL");
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &hierarchy, &meshes));
        out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), rig.hierarchy_file(&hierarchy));
        let object = projectile_object(&id.replace(['+', '~'], "_"));
        // A Sensor Grenade sees for its owner (and finds stealthed units).
        let sight = if base_id(id) == "sensor_grenade_mp" {
            let r = zhbo_core::equipment::SENSOR_RANGE;
            format!("  VisionRange = {r}\n  ShroudClearingRange = {r}\n  Behavior = StealthDetectorUpdate ModuleTag_03\n    DetectionRate = 500\n    DetectionRange = {r}\n  End\n")
        } else {
            String::new()
        };
        ini.push_str(&format!(
            "Object {object}\n  Draw = W3DModelDraw ModuleTag_01\n    OkToChangeModelColor = Yes\n    DefaultConditionState\n      Model = {name}\n    End\n  End\n  \
             KindOf = NO_COLLIDE INERT IMMOBILE UNATTACKABLE CLICK_THROUGH\n  Body = InactiveBody ModuleTag_02\n  End\n{sight}  \
             Geometry = Sphere\n  GeometryMajorRadius = 1.0\n  GeometryIsSmall = Yes\nEnd\n\n"
        ));
        d.projectile_object = object;
        made.push(id.as_str());
    }
    out.insert("Data\\INI\\Object\\ZHBO_BO2_Projectiles.ini".into(), ini.into_bytes());
    format!("projectiles {}", made.join(", "))
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

/// The tablet in the soldier's hand, as `TB<k>` meshes on BO2's `tag_inhand` (the bone its tablet
/// animations carry).
fn tablet_world(patch: &Zone, common: &Zone, paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, rig: &Rig, decals: &mut BTreeSet<String>) -> Vec<w3d::SkinMesh> {
    let Ok(model) = find_model(patch, common, TABLET_WORLD) else { return Vec::new() };
    let Some(tag) = rig.pivot(TABLET_TAG).or_else(|| rig.pivot("tag_weapon_left")) else { return Vec::new() };
    let tex = textures(&model, paks, out, decals);
    let mut meshes = character::skin_rigid(&model, tag, crate::xf::IDENTITY, "TB", 0, &tex, &[]);
    finish(&mut meshes, decals);
    meshes
}

/// Convert everything; returns the archive files and a one-line summary.
pub fn files(bo2_dir: &Path) -> Result<(Vec<(String, Vec<u8>)>, String), String> {
    let common = load_zone(bo2_dir, "common_mp")?;
    let weapon = xmodel::find(&common, WEAPON).map_err(|e| e.to_string())?;
    let paks = Paks::open(bo2_dir)?;
    let patch = load_zone(bo2_dir, crate::fx::ZONE)?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut summary = Vec::new();
    out.insert(format!("Art\\Textures\\{ENV_MAP}"), images::tga_bytes(&env_map()));

    // Every BO2 multiplayer weapon a player can carry, with its attachments and alt modes, as the
    // core's weapon table; art for the converted ones (the loadout first, then the rest).
    let bo2 = zhbo_formats::weapon::all(&common);
    let atts = zhbo_formats::weapon::attachments(&common);
    let names = crate::weapons::names(bo2_dir);
    let mut table = crate::weapons::table(&bo2, &atts, &names);
    let converted = crate::weapons::converted(&bo2);
    crate::weapons::add_variants(&mut table, &converted);

    let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(bo2_dir, f.zone)).collect::<Result<_, _>>()?;
    let (vm_summary, mut clips) = weapon_viewmodels(&patch, &common, &zones, &paks, &mut out, &bo2, &atts, &mut table.weapons, &converted)?;
    summary.push(vm_summary);

    // The soldiers: body, every converted weapon in the right hand (one shown at a time), and the
    // tablet.
    let rig = Rig::new(&xmodel::find(&zones[0], FACTIONS[0].body).map_err(|e| e.to_string())?, &[(&weapon, character::WEAPON_TAG)]);
    let mut decals = BTreeSet::new();
    let held = world_weapons(&patch, &common, &paks, &mut out, &rig, &bo2, &atts, &mut table.weapons, &converted, &mut decals);
    let tablet = tablet_world(&patch, &common, &paks, &mut out, &rig, &mut decals);
    table.tablet_meshes = tablet.len() as u8;
    // Shown at first: the primary (viewmodel 0).
    let mut hidden: Vec<String> = held.iter().map(|m| m.name.clone()).filter(|n| !n.starts_with("W00")).collect();
    hidden.extend(tablet.iter().map(|m| m.name.clone()));
    let mut ini = BASE_INI.to_string();
    for (f, zone) in FACTIONS.iter().zip(&zones) {
        let body = xmodel::find(zone, f.body).map_err(|e| e.to_string())?;
        let body_tex = textures(&body, &paks, &mut out, &mut decals);
        let mut meshes = character::skin(&rig, &body, "B", &body_tex);
        finish(&mut meshes, &decals);
        meshes.extend(held.iter().cloned());
        meshes.extend(tablet.iter().cloned());
        out.insert(format!("Art\\W3D\\{}.w3d", f.model), w3d::skin_model(f.model, HIERARCHY, &meshes));
        ini = replace_draw(&ini, f.object, &draw_module(f.model, &hidden))?;
        summary.push(format!("{} = {} ({} meshes)", f.object, f.body, meshes.len()));
    }
    summary.push(format!("third person: {} weapon meshes, {} tablet meshes", held.len(), tablet.len()));
    if !decals.is_empty() {
        summary.push(format!("decals dropped: {}", decals.iter().cloned().collect::<Vec<_>>().join(" ")));
    }
    out.insert(format!("Art\\W3D\\{HIERARCHY}.w3d"), rig.hierarchy_file(HIERARCHY));
    for (w3d_name, bo2_name) in ANIMS {
        let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, HIERARCHY, frames, fps, &channels));
    }

    let mut tablet_decals = BTreeSet::new();
    tablets(&common, &zones, &paks, &mut out, &mut tablet_decals)?;
    for (i, (_, bo2_name)) in TABLET_ANIMS.iter().enumerate() {
        let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
        clips.push((zhbo_core::viewmodel::NONE, zhbo_core::viewmodel::TABLET_PULLOUT + i as i32, anim));
    }
    summary.push(projectiles(&patch, &common, &paks, &mut out, &bo2, &mut table.weapons, &converted));

    // BO2 sound effects: the general ones, each converted weapon's own shots, and the viewmodel
    // clips' sound notetracks.
    let world = load_zone(bo2_dir, crate::sfx::WORLD_ZONE)?;
    let (extra, fx_ini) = crate::weapons::sounds_and_fx(&bo2, &atts, &mut table.weapons, &converted);
    out.insert("Data\\INI\\Weapon\\ZHBO_BO2_Weapons.ini".into(), fx_ini.into_bytes());
    let (sound_files, sound_summary) = crate::sfx::files(&common, &world, bo2_dir, &clips, &extra)?;
    out.extend(sound_files);
    summary.push(sound_summary);

    // BO2's own explosions for the converted weapons (and equipment breaking), as Zero Hour
    // particle systems.
    let mut effects = crate::fx::Converted::default();
    let mut ids: Vec<String> = converted.clone();
    ids.extend(table.weapons.iter().filter(|d| d.id.contains('~')).map(|d| d.id.clone()));
    for id in &ids {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(e) = &w.explosion_effect else { continue };
        let Some(d) = table.weapons.iter_mut().find(|d| &d.id == id) else { continue };
        if matches!(d.effect, zhbo_core::weapons::Effect::Explode | zhbo_core::weapons::Effect::Stun | zhbo_core::weapons::Effect::Emp | zhbo_core::weapons::Effect::Smoke) {
            d.explosion_fx = effects.add(e, &|h| paks.image(h));
        }
    }
    for name in zhbo_core::equipment::BO2_EFFECTS {
        if let Some(e) = zhbo_formats::fx::find(&common, name) {
            effects.add(&e, &|h| paks.image(h));
        }
    }
    summary.push(format!("{} BO2 effects ({} textures)", effects.fxlists.matches("FXList ").count(), effects.textures.len()));
    out.insert("Data\\INI\\ParticleSystem\\ZHBO_BO2_Effects.ini".into(), effects.particles.clone().into_bytes());
    out.insert("Data\\INI\\FXList\\ZHBO_BO2_Effects.ini".into(), effects.fxlists.clone().into_bytes());
    out.extend(std::mem::take(&mut effects.textures));

    out.insert(crate::weapons::TABLE_PATH.into(), zhbo_core::weapons::format_full_table(&table).into_bytes());
    summary.push(format!("{} weapons and {} attachments in the table", table.weapons.len(), table.attachments.len()));

    // First-person muzzle flash with BO2's flash sprite.
    out.extend(crate::fx::files(&patch, &|hash| paks.image(hash))?);
    out.insert(INI_PATH.to_string(), ini.into_bytes());
    let textures = out.keys().filter(|k| k.ends_with(".tga")).count();
    Ok((out.into_iter().collect(), format!("{}; {textures} textures, {} animations", summary.join(", "), ANIMS.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every BO2 multiplayer weapon a player can carry, from this machine's install: its definition
    /// converts and plays by BO2's rules in the simulation (`zhbo_core::conformance`), and its
    /// viewmodel (arms, weapon, magazine) and every BO2 animation convert.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_mp_weapon_converts_and_plays_like_bo2() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let common = load_zone(&bo2_dir, "common_mp").unwrap();
        let patch = load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(&bo2_dir, f.zone).unwrap()).collect();
        let paks = Paks::open(&bo2_dir).unwrap();
        let names = crate::weapons::names(&bo2_dir);
        let bo2 = zhbo_formats::weapon::all(&common);
        let atts = zhbo_formats::weapon::attachments(&common);
        let mut defs: Vec<zhbo_core::weapons::WeaponDef> =
            bo2.iter().filter(|w| crate::weapons::is_player_weapon(w)).map(|w| crate::weapons::to_def(w, &names)).collect();
        assert!(defs.len() > 50, "{} player weapons", defs.len());
        // The table carries every one of them unchanged.
        let again = zhbo_core::weapons::parse_table(&zhbo_core::weapons::format_table(&defs)).unwrap();
        for (a, b) in again.iter().zip(&defs) {
            assert_eq!(a, b, "{} round trip", b.id);
        }
        assert_eq!(again.len(), defs.len());

        let mut fails = Vec::new();
        let ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();
        for (i, id) in ids.iter().enumerate() {
            let mut out = BTreeMap::new();
            match weapon_viewmodels(&patch, &common, &zones, &paks, &mut out, &bo2, &atts, &mut defs, std::slice::from_ref(id)) {
                Ok((summary, clips)) => {
                    let d = defs.iter().find(|d| &d.id == id).unwrap();
                    let models = out.keys().filter(|k| k.contains("_SKN")).count();
                    if models != FACTIONS.len() {
                        fails.push(format!("{id}: {models} viewmodels"));
                    }
                    if clips.is_empty() && d.class != zhbo_core::weapons::Class::Equipment {
                        fails.push(format!("{id}: no animations ({summary})"));
                    }
                    let def: &'static zhbo_core::weapons::WeaponDef = Box::leak(Box::new(d.clone()));
                    fails.extend(zhbo_core::conformance::check(def));
                    println!(
                        "{i:2} {id:<24} {:<22} {:?} {:>9?} clip {:>3}/{:<3} rpm {:>5.0} dmg {:>3.0}-{:<3.0} {} clips",
                        d.display_name,
                        d.class,
                        d.fire,
                        d.clip,
                        d.start_reserve,
                        d.rounds_per_minute(),
                        d.damage[0],
                        d.damage[1],
                        clips.len()
                    );
                }
                Err(e) => fails.push(format!("{id}: {e}")),
            }
        }
        assert!(fails.is_empty(), "{} problems:\n{}", fails.len(), fails.join("\n"));
    }

    /// Every attachment BO2 offers on every weapon: the weapon with it (and its alt mode) plays by
    /// the attachment's numbers, and its viewmodel with the attachment's model converts.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_attachment_converts_and_plays() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let common = load_zone(&bo2_dir, "common_mp").unwrap();
        let patch = load_zone(&bo2_dir, crate::fx::ZONE).unwrap();
        let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(&bo2_dir, f.zone).unwrap()).collect();
        let paks = Paks::open(&bo2_dir).unwrap();
        let names = crate::weapons::names(&bo2_dir);
        let bo2 = zhbo_formats::weapon::all(&common);
        let atts = zhbo_formats::weapon::attachments(&common);
        let table = crate::weapons::table(&bo2, &atts, &names);
        let mut fails = Vec::new();
        let mut combos = 0;
        for base in table.weapons.iter().filter(|d| !d.allowed.is_empty()) {
            for (short, _) in &base.allowed {
                let id = zhbo_core::attachments::variant_id(&base.id, &[short.as_str()]);
                let mut t = table.clone();
                crate::weapons::add_variants(&mut t, std::slice::from_ref(&id));
                let Some(d) = t.weapons.iter().find(|d| d.id == id).cloned() else {
                    fails.push(format!("{id}: not built"));
                    continue;
                };
                if d.attachments != vec![short.clone()] {
                    fails.push(format!("{id}: attachments {:?}", d.attachments));
                }
                let mut checked = vec![d.clone()];
                if let Some(alt) = t.weapons.iter().find(|a| !d.alt.is_empty() && a.id == d.alt) {
                    checked.push(alt.clone());
                }
                for c in checked {
                    fails.extend(zhbo_core::conformance::check(Box::leak(Box::new(c))));
                }
                let mut out = BTreeMap::new();
                match weapon_viewmodels(&patch, &common, &zones, &paks, &mut out, &bo2, &atts, &mut t.weapons, std::slice::from_ref(&id)) {
                    Ok((summary, clips)) => {
                        let has_model = base_weapon(&bo2, &base.id).attachment_uniques.iter().any(|u| u.combined == 0 && u.view_model.is_some() && crate::weapons::short_name(&base.allowed.iter().find(|(s, _)| s == short).unwrap().1) == *short && w_kind(&bo2, &base.id, short) == Some(u.kind));
                        if has_model && !summary.contains("attachment model") {
                            fails.push(format!("{id}: attachment model missing ({summary})"));
                        }
                        if clips.is_empty() {
                            fails.push(format!("{id}: no clips"));
                        }
                    }
                    Err(e) => fails.push(format!("{id}: {e}")),
                }
                combos += 1;
            }
        }
        println!("{combos} weapon and attachment combinations");
        assert!(combos > 300, "{combos} combinations");
        assert!(fails.is_empty(), "{} problems:\n{}", fails.len(), fails.join("\n"));
    }

    fn base_weapon<'a>(bo2: &'a [zhbo_formats::weapon::Weapon], id: &str) -> &'a zhbo_formats::weapon::Weapon {
        bo2.iter().find(|w| w.name == id).unwrap()
    }

    fn w_kind(bo2: &[zhbo_formats::weapon::Weapon], id: &str, short: &str) -> Option<u32> {
        base_weapon(bo2, id).attachment_variants.iter().find(|(_, a)| crate::weapons::short_name(a) == short).map(|(k, _)| *k)
    }

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
        assert!(is_decal("zhbo_clan_font_c.tga", &solid(4, 4, [255, 255, 255, 0])));
        assert!(!is_decal("zhbo_clan_font_c.tga", &solid(4, 4, [255, 255, 255, 255])));
        assert!(!is_decal("zhbo_mtl_t6_wpn_pistol_fnp45_col.tga", &solid(4, 4, [255, 255, 255, 0])), "weapon colour maps stay");
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
            let out = replace_draw(BASE_INI, f.object, &draw_module(f.model, &["W0100".to_string()])).unwrap();
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
