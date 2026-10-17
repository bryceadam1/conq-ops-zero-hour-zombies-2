/*
** Conquer Ops - converter: the viewmodels: BO2 weapon models rebuilt as Zero Hour W3D hierarchies, plus the tablets and the hammer.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::character::{self, Rig};
use crate::generals::*;
use crate::images;
use std::path::Path;
use crate::paks::{load_zone, Paks};
use crate::textures::{finish, textures, Looks};
use zhbo_formats::iwi::Rgba;
use zhbo_formats::{t6zone::Zone, w3d, xanim, xmodel};
use std::collections::BTreeMap;


/// The magazine model of a gun's viewmodel: BO2 names them `t6_attach_mag_<weapon>_view` after a
/// part of the gun's own view model name (the zones don't link them for base weapons).

pub(crate) fn magazine(common: &Zone, view_model: &str) -> Option<xmodel::Model> {
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
pub(crate) fn find_model(patch: &Zone, common: &Zone, name: &str) -> Result<xmodel::Model, String> {
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
pub(crate) fn weapon_viewmodels(
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
    let mut looks = Looks::default();
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
        let gun_tex = textures(&gun, paks, out, &mut looks);
        let mag_tex = mag.as_ref().map(|m| textures(m, paks, out, &mut looks));
        let extra_tex: Vec<Vec<Option<String>>> = extra.iter().map(|(m, ..)| textures(m, paks, out, &mut looks)).collect();
        let mut rig: Option<Rig> = None;
        for (f, zone) in FACTIONS.iter().zip(zones) {
            let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
            let rig = rig.get_or_insert_with(|| {
                let mut rig = Rig::new(&hands, &parts);
                let root = gun.bones.first().and_then(|b| rig.pivot(&b.name)).unwrap_or(0);
                // Aiming lines `tag_sights` up with the eye; many BO2 viewmodels (every pistol) have
                // none, so give them one where their sights are.
                if rig.pivot("tag_sights").is_none() {
                    let at = rig.pivot("tag_sights_on").map(|p| rig.bind[p].1).or_else(|| {
                        let weapon = rig.pivot("tag_weapon").map_or(crate::xf::IDENTITY, |p| rig.bind[p]);
                        sight_point(&gun).map(|p| crate::xf::apply(weapon, character::scaled(p)))
                    });
                    if let Some(at) = at {
                        let local = crate::xf::compose(crate::xf::inverse(rig.bind[root]), (crate::xf::IDENTITY.0, at));
                        rig.add_marker("tag_sights", root, local);
                    }
                }
                for (model, tag, offset, pt) in &extra {
                    let at = tag.as_deref().and_then(|t| rig.pivot(t)).unwrap_or(root);
                    rig.attach(model, at, *offset);
                    // Aim through the optic: the line of sight goes up to the middle of its glass
                    // (the window sits in the upper part of the sight, above its mount).
                    if *pt == point::OPTIC as u32 {
                        if let Some(optic) = model.bones.first().and_then(|b| rig.pivot(&b.name)) {
                            let mid = (model.mins[2] + (model.maxs[2] - model.mins[2]) * OPTIC_WINDOW) * character::SCALE;
                            let pos = crate::xf::apply(rig.bind[optic], [0.0, 0.0, mid]);
                            rig.move_pivot("tag_sights", pos);
                        }
                    }
                }
                rig
            });
            let hands_tex = textures(&hands, paks, out, &mut looks);
            let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
            meshes.extend(character::skin_hiding(rig, &gun, "G", &gun_tex, &c.hide));
            if let (Some(m), Some(t)) = (&mag, &mag_tex) {
                meshes.extend(character::skin(rig, m, "M", t));
                // BO2's reloads bring the new magazine in on `tag_clip1` while the old one
                // (`tag_clip`) drops away: a second copy rides there (off screen until then).
                if let Some(new_mag) = rig.pivot("tag_clip1") {
                    meshes.extend(character::skin_rigid(m, new_mag, crate::xf::IDENTITY, "N", 0, t, &[]));
                }
            }
            for (k, ((model, ..), tex)) in extra.iter().zip(&extra_tex).enumerate() {
                meshes.extend(character::skin(rig, model, &format!("A{k}"), tex));
            }
            finish(&mut meshes, &looks);
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
    Ok((format!("viewmodels {} (surfaces left out: {})", made.join(", "), looks.dropped.into_iter().collect::<Vec<_>>().join(" ")), clips))
}

/// The third-person weapons on the soldier model: each converted weapon's BO2 world model and its
/// attachments' (at BO2's world offsets), rigid on the right hand as `W<vm><k>` meshes, so the bridge
/// shows the one in hand. Sets each definition's `world_meshes` (its alt mode shares them).
#[allow(clippy::too_many_arguments)]
pub(crate) fn world_weapons(
    patch: &Zone,
    common: &Zone,
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    rig: &Rig,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    looks: &mut Looks,
) -> (Vec<w3d::SkinMesh>, Vec<(usize, crate::xf::Xf)>) {
    let hand = rig.pivot(character::WEAPON_TAG).unwrap_or(0);
    let mut meshes = Vec::new();
    let mut flashes = Vec::new();
    for (vm, id) in converted.iter().enumerate() {
        let (base, shorts) = zhbo_core::attachments::split_id(id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let Some(world) = w.world_model.as_deref().and_then(|m| find_model(patch, common, m).ok()) else { continue };
        let c = combo(w, atts, &shorts);
        let prefix = format!("W{vm:02}");
        if let Some(b) = world.bones.iter().find(|b| b.name == "tag_flash") {
            flashes.push((vm, character::scaled_xf((b.rotation, b.position))));
        }
        let tex = textures(&world, paks, out, looks);
        let mut mine = character::skin_rigid(&world, hand, crate::xf::IDENTITY, &prefix, 0, &tex, &c.hide);
        for (u, _) in &c.parts {
            let Some(model) = u.world_model.as_deref().and_then(|m| find_model(patch, common, m).ok()) else { continue };
            // From the world model's own tag, or its origin.
            let tag = u.world_model_tag.as_deref().and_then(|t| world.bones.iter().find(|b| b.name == t));
            let base_xf = tag.map_or(crate::xf::IDENTITY, |b| character::scaled_xf((b.rotation, b.position)));
            let offset = crate::xf::compose(base_xf, character::scaled_xf((crate::xf::from_angles(u.world_rotation), u.world_offset)));
            let t = textures(&model, paks, out, looks);
            let first = mine.len();
            mine.extend(character::skin_rigid(&model, hand, offset, &prefix, first, &t, &[]));
        }
        finish(&mut mine, looks);
        let count = mine.len().min(99) as u8;
        for d in defs.iter_mut().filter(|d| &d.id == id || d.alt == *id && d.id.contains('~')) {
            d.world_meshes = count;
        }
        meshes.extend(mine);
    }
    (meshes, flashes)
}

/// Thrown and launched things for `converted` weapons: their BO2 projectile (or world) model as a
/// W3D model, and a Zero Hour object that shows it (`ZHBO_Proj_<id>`). Sets `projectile_object`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn projectiles(
    patch: &Zone,
    common: &Zone,
    paks: &Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    bo2: &[zhbo_formats::weapon::Weapon],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    models: &mut BTreeMap<String, String>,
) -> String {
    use zhbo_core::weapons::Class;
    let mut ini = String::from("; Generated by zhbo-assets: thrown, launched and placed BO2 weapons, moved by zhbo-core.\n\n");
    let mut looks = Looks::default();
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
        let tex = textures(&model, paks, out, &mut looks);
        let mut meshes = character::skin(&rig, &model, "P", &tex);
        finish(&mut meshes, &looks);
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
        models.insert(id.clone(), name.clone());
        made.push(id.as_str());
    }
    out.insert("Data\\INI\\Object\\ZHBO_BO2_Projectiles.ini".into(), ini.into_bytes());
    format!("projectiles {}", made.join(", "))
}

/// Each faction's arms holding the scorestreak tablet, with its lit screen.
pub(crate) fn tablets(common: &Zone, zones: &[Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Result<(), String> {
    let tablet = xmodel::find(common, TABLET).map_err(|e| e.to_string())?;
    let mut tablet_tex = textures(&tablet, paks, out, looks);
    // The screen's material layers several images in a shader; draw one composite, self-lit.
    let screen_surface = tablet.materials.iter().position(|m| m.as_ref().is_some_and(|m| m.name.contains("screen")));
    if let Some(i) = screen_surface {
        if let Some(img) = tablet.materials[i].as_ref().and_then(|m| tablet_screen(m, paks)) {
            out.insert(format!("Art\\Textures\\{TABLET_SCREEN}"), images::tga_bytes(&img));
            tablet_tex[i] = Some(TABLET_SCREEN.to_string());
        }
    }
    let mut rig: Option<Rig> = None;
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&tablet, "tag_weapon")]));
        let hands_tex = textures(&hands, paks, out, looks);
        let mut meshes = character::skin(rig, &hands, "H", &hands_tex);
        let mut screen = character::skin(rig, &tablet, "T", &tablet_tex);
        for m in &mut screen {
            m.emissive = m.texture.as_deref() == Some(TABLET_SCREEN);
        }
        meshes.extend(screen);
        finish(&mut meshes, looks);
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

/// One faction's hammer viewmodel: its arms (`hands`, the rig's base, with the knife) and the
/// hammer in the knife's grip.
pub(crate) fn hammer_meshes(rig: &Rig, hands: &xmodel::Model, hammer: &xmodel::Model, hands_tex: &[Option<String>], hammer_tex: &[Option<String>]) -> Result<Vec<w3d::SkinMesh>, String> {
    let grip = rig.pivot("j_gun").ok_or("no j_gun on the knife")?;
    let mut meshes = character::skin(rig, hands, "H", hands_tex);
    meshes.extend(character::skin_rigid(hammer, grip, character::scaled_xf(HAMMER_GRIP), "M", 0, hammer_tex, &[]));
    Ok(meshes)
}

/// The first faction's hammer viewmodel as converted, untextured, with the clip `clip` (a W3D name
/// from [`HAMMER_ANIMS`]): to check it by eye (`zhbo-assets hammer`).
pub fn hammer_preview(bo2_dir: &Path, clip: &str) -> Result<(Rig, Vec<w3d::SkinMesh>, Vec<w3d::Channel>), String> {
    let common = load_zone(bo2_dir, "common_mp")?;
    let faction = load_zone(bo2_dir, FACTIONS[0].zone)?;
    let prop_zone = load_zone(bo2_dir, HAMMER_ZONE)?;
    let hammer = xmodel::find(&prop_zone, HAMMER).map_err(|e| format!("{HAMMER}: {e}"))?;
    let knife = xmodel::find(&common, KNIFE_VIEW).map_err(|e| format!("{KNIFE_VIEW}: {e}"))?;
    let hands = xmodel::find(&faction, FACTIONS[0].hands).map_err(|e| e.to_string())?;
    let rig = Rig::new(&hands, &[(&knife, "tag_weapon")]);
    let meshes = hammer_meshes(&rig, &hands, &hammer, &[], &[])?;
    let bo2_name = HAMMER_ANIMS.iter().find(|(w, _)| *w == clip).map(|(_, b)| *b).ok_or_else(|| format!("no hammer clip {clip}"))?;
    let anim = xanim::find(&common, bo2_name).map_err(|e| e.to_string())?;
    let (_, channels) = character::animate(&rig, &anim);
    Ok((rig, meshes, channels))
}

pub(crate) fn hammers(bo2_dir: &Path, common: &Zone, zones: &[Zone], paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, looks: &mut Looks) -> Result<(), String> {
    let prop_zone = load_zone(bo2_dir, HAMMER_ZONE)?;
    let hammer = xmodel::find(&prop_zone, HAMMER).map_err(|e| format!("{HAMMER}: {e}"))?;
    let knife = xmodel::find(common, KNIFE_VIEW).map_err(|e| format!("{KNIFE_VIEW}: {e}"))?;
    let hammer_tex = textures(&hammer, paks, out, looks);
    let mut rig: Option<Rig> = None;
    for (f, zone) in FACTIONS.iter().zip(zones) {
        let hands = xmodel::find(zone, f.hands).map_err(|e| e.to_string())?;
        let rig = rig.get_or_insert_with(|| Rig::new(&hands, &[(&knife, "tag_weapon")]));
        let hands_tex = textures(&hands, paks, out, looks);
        let mut meshes = hammer_meshes(rig, &hands, &hammer, &hands_tex, &hammer_tex)?;
        finish(&mut meshes, looks);
        let name = hammer_model(f.viewmodel);
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, HAMMER_HIERARCHY, &meshes));
    }
    let rig = rig.ok_or("no hammer arms")?;
    out.insert(format!("Art\\W3D\\{HAMMER_HIERARCHY}.w3d"), rig.hierarchy_file(HAMMER_HIERARCHY));
    for (w3d_name, bo2_name) in HAMMER_ANIMS {
        let anim = xanim::find(common, bo2_name).map_err(|e| e.to_string())?;
        let (frames, channels) = character::animate(&rig, &anim);
        let fps = anim.frame_rate.round().max(1.0) as u32;
        out.insert(format!("Art\\W3D\\{w3d_name}.w3d"), w3d::animation(w3d_name, HAMMER_HIERARCHY, frames, fps, &channels));
    }
    Ok(())
}

/// The scorestreak screen as BO2 shows it: the background UI, the satellite panels over it, and the
/// targeting reticle over the panels.
pub(crate) fn tablet_screen(m: &xmodel::Material, paks: &Paks) -> Option<Rgba> {
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
pub(crate) fn over(dst: &mut Rgba, src: &Rgba, at: (u32, u32), size: (u32, u32)) {
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
pub(crate) fn tablet_world(patch: &Zone, common: &Zone, paks: &Paks, out: &mut BTreeMap<String, Vec<u8>>, rig: &Rig, looks: &mut Looks) -> Vec<w3d::SkinMesh> {
    let Ok(model) = find_model(patch, common, TABLET_WORLD) else { return Vec::new() };
    let Some(tag) = rig.pivot(TABLET_TAG).or_else(|| rig.pivot("tag_weapon_left")) else { return Vec::new() };
    let tex = textures(&model, paks, out, looks);
    let mut meshes = character::skin_rigid(&model, tag, crate::xf::IDENTITY, "TB", 0, &tex, &[]);
    finish(&mut meshes, looks);
    meshes
}