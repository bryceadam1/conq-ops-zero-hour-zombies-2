/*
** Conquer Ops - converter: the Zero Hour side of the conversion: names, factions and the orchestration (files).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::character::{self, HIERARCHY, Rig};
use crate::{ini, paks, textures, viewmodels};
use zhbo_formats::w3d;
use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::{t6zone::Zone, xanim, xmodel};

const BASE_INI: &str = include_str!("../../../data/Data/INI/Object/ZZZ_ZHBO_General.ini");
const INI_PATH: &str = "Data\\INI\\Object\\ZZZ_ZHBO_General.ini";
pub(crate) const MAX_TEXTURE: u32 = 512;
pub(crate) const TABLET_TAG: &str = "tag_inhand";
const WEAPON: &str = "t6_wpn_ar_an94_world";

pub(crate) use ini::{draw_module, ini_set, replace_draw};
pub(crate) use paks::{load_zone, shrink, Paks};
pub use viewmodels::{base_id, combo, hammer_preview, Combo};

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
        viewmodel: zhbo_core::names::FACTION_VIEWMODEL_USA,
        letter: 'U',
    },
    Faction {
        object: "ZHBO_General_China",
        zone: "faction_pla_mp",
        body: "c_chn_mp_pla_assault_fb",
        model: "ZHBO_CHN_SKN",
        hands: "c_chn_mp_pla_longsleeve_viewhands",
        viewmodel: zhbo_core::names::FACTION_VIEWMODEL_CHN,
        letter: 'C',
    },
    Faction {
        object: "ZHBO_General_GLA",
        zone: "faction_pmc_mp",
        body: "c_mul_mp_pmc_assault_fb",
        model: "ZHBO_GLA_SKN",
        hands: "c_mul_mp_pmc_longsleeve_viewhands",
        viewmodel: zhbo_core::names::FACTION_VIEWMODEL_GLA,
        letter: 'G',
    },
];

/// W3D names of weapon viewmodel `vm` (shared with `host/zhbo/ZhboViewmodel.cpp`): the hierarchy,
/// each faction's arms holding the weapon, and the clip for each BO2 animation slot. The names
/// themselves live in the naming registry (`zhbo_core::names`).
pub fn vm_hierarchy(vm: i32) -> String {
    zhbo_core::names::viewmodel_skl(vm as u8)
}
pub fn vm_model(vm: i32, letter: char) -> String {
    zhbo_core::names::viewmodel_skn(vm as u8, letter)
}
pub fn vm_anim(vm: i32, slot: usize) -> String {
    zhbo_core::names::viewmodel_anim(vm as u8, slot as u8)
}
/// Projectile models (thrown grenades, rockets) and the Zero Hour object that shows one.
pub fn projectile_model(vm: i32) -> String {
    zhbo_core::names::projectile_model(vm as u8)
}
pub fn projectile_object(id: &str) -> String {
    zhbo_core::names::projectile_object(id)
}

/// The scorestreak tablet: each faction's arms holding it, on its own hierarchy, with clips in
/// `zhbo_core::viewmodel` order after the weapon's (TABLET_PULLOUT, TABLET_IDLE, TABLET_PUTAWAY).
/// Its W3D models are the weapon viewmodel's names with `_TAB` for `_SKN`.
pub use zhbo_core::names::{TABLET_HIERARCHY, TABLET_IDLE, TABLET_PULLOUT, TABLET_PUTAWAY};
pub const TABLET_ANIMS: &[(&str, &str)] = &[
    (TABLET_PULLOUT, "viewmodel_tablet_pullout"),
    (TABLET_IDLE, "viewmodel_tablet_idle"),
    (TABLET_PUTAWAY, "viewmodel_tablet_putaway"),
];
pub(crate) const TABLET: &str = "t6_wpn_tablet_view";
pub(crate) const TABLET_SCREEN: &str = "zhbo_bo2_tablet_screen.tga";

/// The tablet viewmodel's W3D name for a weapon viewmodel's.
pub fn tablet_model(viewmodel: &str) -> String {
    viewmodel.replace("_SKN", "_TAB")
}

/// The builder's hammer (first person): each faction's arms holding BO2's hammer prop in the knife's
/// grip, with the knife's clips, in `zhbo_core::viewmodel` order (HAMMER_PULLOUT, HAMMER_IDLE,
/// HAMMER_SWING, HAMMER_PUTAWAY). Its W3D models are the weapon viewmodel's names with `_HAM` for
/// `_SKN`.
pub use zhbo_core::names::{HAMMER_HIERARCHY, HAMMER_IDLE, HAMMER_PULLOUT, HAMMER_PUTAWAY, HAMMER_SWING};
pub const HAMMER_ANIMS: &[(&str, &str)] = &[
    (HAMMER_PULLOUT, "viewmodel_strider_pullout"),
    (HAMMER_IDLE, "viewmodel_strider_idle"),
    (HAMMER_SWING, "viewmodel_strider_melee_swipe"),
    (HAMMER_PUTAWAY, "viewmodel_strider_putaway"),
];
pub(crate) const KNIFE_VIEW: &str = "t6_wpn_knife_base_view";
/// BO2's hammer, a campaign prop, and a small zone that has it.
pub(crate) const HAMMER: &str = "p_glo_tools_hammer";
pub(crate) const HAMMER_ZONE: &str = "haiti_gump_front_door";
/// The hammer in the knife's grip, on `j_gun` (which points along the blade): the prop lies along
/// its Y with the head at +Y, so its Y goes forward (X), gripped 2 inches from the end. Its face is
/// at -X and its claw at +X; turned about the handle so the face, not the claw, leads the swing.
pub(crate) const HAMMER_GRIP: crate::xf::Xf = ([0.5, 0.5, -0.5, 0.5], [5.0, 0.5, 0.0]);

/// The hammer viewmodel's W3D name for a weapon viewmodel's.
pub fn hammer_model(viewmodel: &str) -> String {
    viewmodel.replace("_SKN", "_HAM")
}

/// How a third-person clip is made from BO2's (all in common_mp). BO2 layers a player's torso
/// animations (`pt_*`) over body animations (`pb_*`); Zero Hour plays one at a time.
pub enum BodyClip {
    /// One body animation as it is.
    Body(&'static str),
    /// A torso animation over a stance (its first frame) for the legs.
    Torso(&'static str, &'static str),
    /// A body animation's legs under a stance's upper body (its first frame).
    Legs(&'static str, &'static str),
}

/// (W3D animation name, how it is made). The Draw module (`draw_module`) plays them by state.
pub const ANIMS: &[(&str, BodyClip)] = &[
    ("ZHBO_PLR_IDL", BodyClip::Body("pb_stand_alert")),
    ("ZHBO_PLR_RUN", BodyClip::Body("pb_combatrun_forward_loop")),
    ("ZHBO_PLR_ADS", BodyClip::Body("pb_stand_ads")),
    ("ZHBO_PLR_FIR", BodyClip::Torso("pt_rifle_fire_ads", "pb_stand_ads")),
    ("ZHBO_PLR_RLD", BodyClip::Torso("pt_reload_stand_rifle", "pb_stand_alert")),
    // The sidearm: BO2's pistol stance, aim and reload, and the run's legs under the pistol stance.
    ("ZHBO_PLR_PID", BodyClip::Body("pb_stand_alert_pistol")),
    ("ZHBO_PLR_PRN", BodyClip::Legs("pb_combatrun_forward_loop", "pb_stand_alert_pistol")),
    ("ZHBO_PLR_PAD", BodyClip::Body("pb_stand_ads_pistol")),
    ("ZHBO_PLR_PRL", BodyClip::Torso("pt_reload_stand_pistol", "pb_stand_alert_pistol")),
    // A grenade thrown (the lethal or the tactical).
    ("ZHBO_PLR_THR", BodyClip::Torso("pt_grenade_stand_throw", "pb_stand_alert")),
    ("ZHBO_PLR_DTA", BodyClip::Body("pb_death_faceplant")),
    ("ZHBO_PLR_DTB", BodyClip::Body("pb_death_flyback")),
    // The scorestreak tablet coming up and going down (the field computer).
    ("ZHBO_PLR_TBU", BodyClip::Torso("pt_tablet_stand_raise", "pb_stand_alert")),
    ("ZHBO_PLR_TBD", BodyClip::Torso("pt_tablet_stand_putaway", "pb_stand_alert")),
];

/// BO2's leg and hip bones (`BodyClip::Legs` takes these from the moving animation).
fn is_leg_bone(name: &str) -> bool {
    ["mainroot", "hip", "knee", "ankle", "ball", "toe", "spinelower"].iter().any(|k| name.contains(k))
}

/// Build a clip from BO2's animations.
fn body_clip(common: &Zone, c: &BodyClip) -> Result<xanim::Anim, String> {
    let find = |n: &str| xanim::find(common, n).map_err(|e| e.to_string());
    Ok(match c {
        BodyClip::Body(n) => find(n)?,
        BodyClip::Torso(torso, stance) => find(torso)?.layered_over(&find(stance)?),
        BodyClip::Legs(legs, stance) => find(legs)?.legs_under(&find(stance)?, is_leg_bone),
    })
}

/// Where a viewmodel gun's sights are, from its shape (model space, inches): the top of the front
/// sight, the highest point of the front third of the gun, which sits on the line of sight. For
/// guns without BO2's own `tag_sights`.
pub(crate) fn sight_point(gun: &xmodel::Model) -> Option<[f32; 3]> {
    let lod = gun.lods.first()?;
    let verts: Vec<[f32; 3]> = gun.surfaces[lod.first_surface..lod.first_surface + lod.surface_count]
        .iter()
        .flat_map(|s| s.vertices.iter().map(|v| v.position))
        .collect();
    let (lo, hi) = verts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
    if verts.is_empty() || hi - lo < 1.0 {
        return None;
    }
    let front = hi - (hi - lo) / 3.0;
    verts.iter().filter(|p| p[0] >= front).copied().fold(None, |best: Option<[f32; 3]>, p| match best {
        Some(b) if b[2] >= p[2] => Some(b),
        _ => Some(p),
    })
}

/// The marker bone at converted weapon `vm`'s muzzle (`tag_flash`) on the soldier rig: where its
/// shots and BO2 muzzle flash come from.
pub fn flash_bone(vm: usize) -> String {
    format!("ZF{vm:02}")
}

/// Third-person sub-objects on the soldier model: weapon `vm`'s meshes, and the tablet's (shared
/// with `host/zhbo/ZhboBridge.cpp`).
pub fn world_mesh(vm: i32, k: usize) -> String {
    format!("W{vm:02}{k:02}")
}
pub fn tablet_mesh(k: usize) -> String {
    format!("TB{k:02}")
}
pub(crate) const TABLET_WORLD: &str = "t6_wpn_tablet_world";
/// How high up an optic its window's centre is (share of the optic's height).
pub(crate) const OPTIC_WINDOW: f32 = 0.72;
/// BO2's smoke grenade cloud (its script plays it; the weapon names no effect).
const SMOKE_EFFECT: &str = "weapon/grenade/fx_smoke_willy_pete";
/// The overhead General's bullet impacts (see `ZhboBridge.cpp`, `loadRtsFx`).
pub use zhbo_core::names::RTS_FX_PATH;
/// The box a BO2 soldier is clicked by (inches: a man standing, with some room).
const SOLDIER_BOX_CENTRE: [f32; 3] = [0.0, 0.0, 36.0];
const SOLDIER_BOX_EXTENT: [f32; 3] = [18.0, 18.0, 38.0];


/// Convert everything; returns the archive files and a one-line summary.

pub fn files(bo2_dir: &Path) -> Result<(Vec<(String, Vec<u8>)>, String), String> {
    let common = load_zone(bo2_dir, "common_mp")?;
    let weapon = xmodel::find(&common, WEAPON).map_err(|e| e.to_string())?;
    let paks = Paks::open(bo2_dir)?;
    let patch = load_zone(bo2_dir, crate::fx::ZONE)?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut summary = Vec::new();

    // Every BO2 multiplayer weapon a player can carry, with its attachments and alt modes, as the
    // core's weapon table; art for the converted ones (the loadout first, then the rest).
    let bo2 = zhbo_formats::weapon::all(&common).map_err(|e| e.to_string())?;
    let atts = zhbo_formats::weapon::attachments(&common).map_err(|e| e.to_string())?;
    let names = crate::weapons::names(bo2_dir);
    let mut table = crate::weapons::table(&bo2, &atts, &names);
    let converted = crate::weapons::converted(&bo2);
    crate::weapons::add_variants(&mut table, &converted);
    #[cfg(debug_assertions)]
    {
        // The schema-driven table (refactor plan 1.4) must survive format+parse unchanged: a
        // schema field the converter forgot to map shows up here, at the machine that has BO2.
        let text = zhbo_core::weapons::format_full_table(&table);
        let back = zhbo_core::weapons::parse_full_table(&text).expect("the converter's table re-parses");
        assert_eq!(zhbo_core::weapons::format_full_table(&back), text, "the converter's table is a fixed point");
    }

    let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(bo2_dir, f.zone)).collect::<Result<_, _>>()?;
    let (vm_summary, mut clips) = viewmodels::weapon_viewmodels(&patch, &common, &zones, &paks, &mut out, &bo2, &atts, &mut table.weapons, &converted)?;
    summary.push(vm_summary);

    // The soldiers: body, every converted weapon in the right hand (one shown at a time), and the
    // tablet.
    let mut rig = Rig::new(&xmodel::find(&zones[0], FACTIONS[0].body).map_err(|e| e.to_string())?, &[(&weapon, character::WEAPON_TAG)]);
    let mut looks = textures::Looks::default();
    let (held, flashes) = viewmodels::world_weapons(&patch, &common, &paks, &mut out, &rig, &bo2, &atts, &mut table.weapons, &converted, &mut looks);
    let tablet = viewmodels::tablet_world(&patch, &common, &paks, &mut out, &rig, &mut looks);
    // Each weapon's muzzle on the hand, so shots and flashes leave the gun in hand.
    let hand = rig.pivot(character::WEAPON_TAG).unwrap_or(0);
    for (vm, at) in &flashes {
        rig.add_marker(&flash_bone(*vm), hand, *at);
    }
    let loadout_vm = |id: &str| converted.iter().position(|c| c == id).unwrap_or(0);
    let guns = (loadout_vm(&zhbo_core::weapons::Loadout::default().primary), loadout_vm(&zhbo_core::weapons::Loadout::default().secondary));
    table.tablet_meshes = tablet.len() as u8;
    // Shown at first: the primary (viewmodel 0).
    let mut hidden: Vec<String> = held.iter().map(|m| m.name.clone()).filter(|n| !n.starts_with("W00")).collect();
    hidden.extend(tablet.iter().map(|m| m.name.clone()));
    let mut ini = BASE_INI.to_string();
    for (f, zone) in FACTIONS.iter().zip(&zones) {
        let body = xmodel::find(zone, f.body).map_err(|e| e.to_string())?;
        let body_tex = textures::textures(&body, &paks, &mut out, &mut looks);
        let mut meshes = character::skin(&rig, &body, "B", &body_tex);
        textures::finish(&mut meshes, &looks);
        meshes.extend(held.iter().cloned());
        meshes.extend(tablet.iter().cloned());
        // A box around the standing soldier to click him by (his skin is unposed by the feet).
        let pick = (character::scaled(SOLDIER_BOX_CENTRE), character::scaled(SOLDIER_BOX_EXTENT));
        out.insert(format!("Art\\W3D\\{}.w3d", f.model), w3d::skin_model_with_box(f.model, HIERARCHY, &meshes, Some(pick)));
        ini = replace_draw(&ini, f.object, &draw_module(f.model, &hidden, guns))?;
        summary.push(format!("{} = {} ({} meshes)", f.object, f.body, meshes.len()));
    }
    summary.push(format!("third person: {} weapon meshes, {} tablet meshes", held.len(), tablet.len()));
    if !looks.dropped.is_empty() {
        summary.push(format!("surfaces left out: {}", looks.dropped.iter().cloned().collect::<Vec<_>>().join(" ")));
    }
    out.insert(format!("Art\\W3D\\{HIERARCHY}.w3d"), rig.hierarchy_file(HIERARCHY));
    for (w3d_name, how) in ANIMS {
        let anim = body_clip(&common, how)?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, HIERARCHY, frames, fps, &channels));
    }

    let mut tablet_looks = textures::Looks::default();
    viewmodels::tablets(&common, &zones, &paks, &mut out, &mut tablet_looks)?;
    for (i, (_, bo2_name)) in TABLET_ANIMS.iter().enumerate() {
        let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
        clips.push((zhbo_core::viewmodel::NONE, zhbo_core::viewmodel::TABLET_PULLOUT + i as i32, anim));
    }
    // The hammer is optional: without it the builder's hands show nothing.
    match viewmodels::hammers(bo2_dir, &common, &zones, &paks, &mut out, &mut tablet_looks) {
        Ok(()) => {
            for (i, (_, bo2_name)) in HAMMER_ANIMS.iter().enumerate() {
                let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
                clips.push((zhbo_core::viewmodel::NONE, zhbo_core::viewmodel::HAMMER_PULLOUT + i as i32, anim));
            }
        }
        Err(e) => summary.push(format!("no hammer viewmodel: {e}")),
    }
    let mut projectile_models = BTreeMap::new();
    summary.push(viewmodels::projectiles(&patch, &common, &paks, &mut out, &bo2, &mut table.weapons, &converted, &mut projectile_models));

    // BO2 sound effects: the general ones, each converted weapon's own shots, and the viewmodel
    // clips' sound notetracks.
    let world = load_zone(bo2_dir, crate::sfx::WORLD_ZONE)?;
    let (extra, mut fx_ini) = crate::weapons::sounds_and_fx(&bo2, &atts, &mut table.weapons, &converted);
    let (sound_files, sound_summary) = crate::sfx::files(&common, &world, bo2_dir, &clips, &extra)?;
    out.extend(sound_files);
    summary.push(sound_summary);

    // BO2's own explosions for the converted weapons (and equipment breaking), as Zero Hour
    // particle systems.
    let mut effects = crate::fx::Converted::default();
    let mut ids: Vec<String> = converted.clone();
    ids.extend(table.weapons.iter().filter(|d| d.id.contains('~')).map(|d| d.id.clone()));
    // Each gun's muzzle flash as others see it (BO2's `worldFlashEffect`), by weapon id.
    let mut flashes: BTreeMap<String, String> = BTreeMap::new();
    for id in &ids {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(d) = table.weapons.iter_mut().find(|d| &d.id == id) else { continue };
        if let Some(f) = &w.world_flash_effect {
            flashes.insert(id.clone(), effects.add(f, &|h| paks.image(h)));
        }
        // BO2's smoke grenade plays its cloud from script: use it by name, for the screen's time.
        if d.effect == zhbo_core::weapons::Effect::Smoke {
            if let Some(e) = w.explosion_effect.clone().or_else(|| zhbo_formats::fx::find(&common, SMOKE_EFFECT)) {
                d.explosion_fx = effects.add_timed(&e, zhbo_core::projectiles::SMOKE_TIME, &|h| paks.image(h));
            }
            continue;
        }
        let Some(e) = &w.explosion_effect else { continue };
        if matches!(d.effect, zhbo_core::weapons::Effect::Explode | zhbo_core::weapons::Effect::Stun | zhbo_core::weapons::Effect::Emp) {
            d.explosion_fx = effects.add(e, &|h| paks.image(h));
        }
    }
    for name in zhbo_core::equipment::BO2_EFFECTS {
        if let Some(e) = zhbo_formats::fx::find(&common, name) {
            effects.add(&e, &|h| paks.image(h));
        }
    }
    // The shots others see in first person flash as BO2's do (not Zero Hour's tracer).
    for (id, flash) in &flashes {
        if let Some(d) = table.weapons.iter().find(|d| &d.id == id && !d.fx_weapon.is_empty()) {
            fx_ini = ini_set(&fx_ini, "Weapon", &d.fx_weapon, "FireFX", flash)?;
        }
    }
    out.insert("Data\\INI\\Weapon\\ZHBO_BO2_Weapons.ini".into(), fx_ini.into_bytes());
    // Under RTS control: the rifle's and pistol's BO2 hits on what they shoot, by what it is
    // (ZhboBridge plays them as the shots land).
    let loadout = zhbo_core::weapons::Loadout::default();
    let mut rts_fx = String::from("; slot, then BO2's bullet impact on a soldier, a vehicle and anything else (FXLists)\n");
    for (slot, id) in [(0, &loadout.primary), (1, &loadout.secondary)] {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(entry) = zhbo_formats::fx::bullet_entry(w.impact_type) else { continue };
        let mut add = |e: Option<zhbo_formats::fx::Effect>| e.map_or("NONE".to_string(), |e| effects.add(&e, &|h| paks.image(h)));
        let flesh = add(zhbo_formats::fx::impact_flesh(&common, entry));
        let metal = add(zhbo_formats::fx::impact(&common, entry, zhbo_formats::fx::surface::METAL));
        let hard = add(zhbo_formats::fx::impact(&common, entry, zhbo_formats::fx::surface::CONCRETE));
        rts_fx.push_str(&format!("{slot} {flesh} {metal} {hard}\n"));
    }
    out.insert(RTS_FX_PATH.into(), rts_fx.into_bytes());
    summary.push(format!("{} BO2 effects ({} textures)", effects.fxlists.matches("FXList ").count(), effects.textures.len()));
    out.insert("Data\\INI\\ParticleSystem\\ZHBO_BO2_Effects.ini".into(), effects.particles.clone().into_bytes());
    out.insert("Data\\INI\\FXList\\ZHBO_BO2_Effects.ini".into(), effects.fxlists.clone().into_bytes());
    out.extend(std::mem::take(&mut effects.textures));

    // The General under RTS control fights with the loadout's rifle, pistol and lethal: their BO2
    // numbers on the Zero Hour weapons, and BO2's grenade model on the thrown one.
    if let Some(text) = out.get(crate::sfx::WEAPON_INI_PATH).map(|b| String::from_utf8_lossy(b).into_owned()) {
        let mut rts = crate::weapons::rts_weapon_edits(&table.weapons);
        // BO2's muzzle flash, from the gun in hand (the Draw module's fire bones).
        for (weapon, id) in [("ZHBO_GeneralRifle", &loadout.primary), ("ZHBO_GeneralPistol", &loadout.secondary)] {
            if let Some(flash) = flashes.get(id) {
                rts.push((weapon.to_string(), "FireFX".to_string(), flash.clone()));
            }
        }
        let text = rts.iter().try_fold(text, |t, (weapon, key, value)| ini_set(&t, "Weapon", weapon, key, value))?;
        out.insert(crate::sfx::WEAPON_INI_PATH.into(), text.into_bytes());
    }

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
    use crate::viewmodels::weapon_viewmodels;
    use crate::textures::{put_occlusion, surface_maps};
    use zhbo_formats::iwi::Rgba;

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
        let bo2 = zhbo_formats::weapon::all(&common).unwrap();
        let atts = zhbo_formats::weapon::attachments(&common).unwrap();
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
        let bo2 = zhbo_formats::weapon::all(&common).unwrap();
        let atts = zhbo_formats::weapon::attachments(&common).unwrap();
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
    fn surface_maps_pack_normal_gloss_and_specular() {
        let normal = solid(2, 2, [100, 150, 255, 255]);
        let spec = solid(8, 8, [200, 200, 200, 40]);
        let maps = surface_maps(4, 4, Some(&normal), Some(&spec));
        assert!(maps.pixels.chunks_exact(4).all(|p| p == [100, 150, 40, 200]));
        let flat = surface_maps(2, 2, None, None);
        assert!(flat.pixels.chunks_exact(4).all(|p| p == [128, 128, 0, 0]), "no maps: flat and matte");
        let mut color = solid(4, 4, [90, 90, 90, 7]);
        put_occlusion(&mut color, Some(&solid(2, 2, [60, 0, 0, 255])));
        assert!(color.pixels.chunks_exact(4).all(|p| p == [90, 90, 90, 60]));
    }

    #[test]
    fn surfaces_blend_as_their_materials_do() {
        let mut looks = textures::Looks::default();
        looks.blend.insert("glass_a.tga".into(), w3d::Blend::Alpha);
        looks.blend.insert("glow_a.tga".into(), w3d::Blend::Additive { alpha: false });
        looks.dropped.insert("reticle.tga".into());
        looks.maps.insert("gun.tga".into(), "gun_nsg.tga".into());
        let mesh = |t: &str| w3d::SkinMesh { texture: Some(t.into()), ..Default::default() };
        let mut meshes = vec![mesh("gun.tga"), mesh("glass_a.tga"), mesh("glow_a.tga"), mesh("reticle.tga")];
        textures::finish(&mut meshes, &looks);
        assert_eq!(meshes.len(), 3, "the reticle is left out");
        assert_eq!((meshes[0].blend, meshes[0].bo2_maps.as_deref()), (w3d::Blend::Opaque, Some("gun_nsg.tga")));
        assert_eq!((meshes[1].blend, meshes[1].bo2_maps.as_deref()), (w3d::Blend::Alpha, None), "glass keeps its alpha");
        assert!(meshes[2].emissive, "glows are self-lit");
    }

    #[test]
    fn ini_set_replaces_or_adds_a_key_in_one_block() {
        let ini = "Weapon A\n  ClipSize = 1\nEnd\nWeapon B\n  ClipSize = 2\nEnd\nObject C\n  Draw = W3DModelDraw ModuleTag_01\n    ConditionState = NONE\n      Model = X\n    End\n  End\nEnd\n";
        let a = ini_set(ini, "Weapon", "B", "ClipSize", "9").unwrap();
        assert!(a.contains("ClipSize = 1") && a.contains("ClipSize = 9") && !a.contains("ClipSize = 2"));
        let b = ini_set(&a, "Weapon", "A", "FireSound", "S").unwrap();
        assert!(b.contains("  ClipSize = 1\n  FireSound = S\nEnd\nWeapon B"), "{b}");
        let c = ini_set(&b, "Object", "C", "Model", "ZPR00_SKN").unwrap();
        assert!(c.contains("      Model = ZPR00_SKN"), "{c}");
        assert!(ini_set(ini, "Weapon", "Z", "ClipSize", "1").is_err());
    }

    #[test]
    fn replaces_only_the_named_draw_module() {
        for f in FACTIONS {
            let out = replace_draw(BASE_INI, f.object, &draw_module(f.model, &["W0100".to_string()], (0, 1))).unwrap();
            assert!(out.contains("WeaponFireFXBone    = PRIMARY ZF00") && out.contains("WeaponFireFXBone    = SECONDARY ZF01"));
            assert!(out.contains(&format!("Model               = {}", f.model)));
            // The rest of the object (weapons, body, locomotor) is untouched.
            assert_eq!(out.matches("Object ZHBO_General_").count(), 3);
            assert_eq!(out.matches("WeaponSet").count(), BASE_INI.matches("WeaponSet").count());
            // Every Draw module (three Generals and their grenade) still closes before the
            // object's other modules.
            assert_eq!(out.matches("Draw = W3DModelDraw").count(), BASE_INI.matches("Draw = W3DModelDraw").count());
        }
        assert!(FACTIONS.iter().all(|f| f.model.len() <= 15));
        assert!(ANIMS.iter().all(|a| a.0.len() <= 15) && HIERARCHY.len() <= 15);
    }

    /// The converter's whole table (weapons, attachments, alt-mode variants) formatted and parsed
    /// back through the schema must come out identical (refactor plan 1.4's fixed-point check; the
    /// debug assertion in `files` guards it on every conversion too).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn weapon_table_is_a_fixed_point() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let common = load_zone(&bo2_dir, "common_mp").unwrap();
        let names = crate::weapons::names(&bo2_dir);
        let bo2 = zhbo_formats::weapon::all(&common).unwrap();
        let atts = zhbo_formats::weapon::attachments(&common).unwrap();
        let mut table = crate::weapons::table(&bo2, &atts, &names);
        let converted = crate::weapons::converted(&bo2);
        crate::weapons::add_variants(&mut table, &converted);
        let text = zhbo_core::weapons::format_full_table(&table);
        let back = zhbo_core::weapons::parse_full_table(&text).expect("the table re-parses");
        assert_eq!(zhbo_core::weapons::format_full_table(&back), text, "format ∘ parse is the identity on the converter's table");
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn converts_from_real_install() {
        let bo2 = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let (files, summary) = files(Path::new(&bo2)).unwrap();
        println!("{summary}");
        assert!(files.iter().any(|(p, _)| p == INI_PATH));
        assert!(files.iter().filter(|(p, _)| p.ends_with(".w3d")).count() >= 1 + FACTIONS.len() + ANIMS.len());
        let text = |path: &str| files.iter().find(|(p, _)| p == path).map(|(_, b)| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
        // The overhead General's BO2 hits (a soldier, a vehicle, anything else) for both guns.
        let hits = text(RTS_FX_PATH);
        for slot in ["0 ", "1 "] {
            let line = hits.lines().find(|l| l.starts_with(slot)).unwrap_or_else(|| panic!("no impacts for slot {slot}: {hits}"));
            assert!(line.split(' ').skip(1).all(|fx| fx.starts_with("ZHBO_BO2FX_")), "{line}");
        }
        // Its guns flash as BO2's do, from their own muzzles.
        let weapons = text(crate::sfx::WEAPON_INI_PATH);
        let rifle = &weapons[weapons.find("Weapon ZHBO_GeneralRifle").unwrap()..];
        assert!(rifle[..rifle.find("\nEnd").unwrap()].contains("FireFX                  = ZHBO_BO2FX_") || rifle[..rifle.find("\nEnd").unwrap()].contains("FireFX = ZHBO_BO2FX_"), "{rifle}");
        let draw = text(INI_PATH);
        assert!(draw.contains("WeaponFireFXBone    = SECONDARY ZF01") && draw.contains("ZHBO_PLR_PRL"));
    }
}
