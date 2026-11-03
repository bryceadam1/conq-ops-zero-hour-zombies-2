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

/// The hand-written General objects (`ZZZ_ZHBO_General.ini`) the converter fills in.
pub(crate) fn base_ini() -> &'static str {
    BASE_INI
}
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
    /// The Zero Hour side (`PlayerTemplate` base side) of the General: its `<side>_Logo` is the
    /// faction's weapon emblem (`emblems`).
    pub side: &'static str,
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
        side: "USA",
    },
    Faction {
        object: "ZHBO_General_China",
        zone: "faction_pla_mp",
        body: "c_chn_mp_pla_assault_fb",
        model: "ZHBO_CHN_SKN",
        hands: "c_chn_mp_pla_longsleeve_viewhands",
        viewmodel: zhbo_core::names::FACTION_VIEWMODEL_CHN,
        letter: 'C',
        side: "China",
    },
    Faction {
        object: "ZHBO_General_GLA",
        zone: "faction_pmc_mp",
        body: "c_mul_mp_pmc_assault_fb",
        model: "ZHBO_GLA_SKN",
        hands: "c_mul_mp_pmc_longsleeve_viewhands",
        viewmodel: zhbo_core::names::FACTION_VIEWMODEL_GLA,
        letter: 'G',
        side: "GLA",
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyClip<'a> {
    /// One body animation as it is.
    Body(&'a str),
    /// A torso animation over a stance (its first frame) for the legs.
    Torso(&'a str, &'a str),
}

/// (W3D animation name, how it is made): the clips every animation set shares. The Draw module
/// (`draw_module`) plays them by state; each set's own are [`set_anims`].
pub const ANIMS: &[(&str, BodyClip<'static>)] = &[
    // A grenade thrown (the lethal or the tactical).
    ("ZHBO_PLR_THR", BodyClip::Torso("pt_grenade_stand_throw", "pb_stand_alert")),
    ("ZHBO_PLR_DTA", BodyClip::Body("pb_death_faceplant")),
    ("ZHBO_PLR_DTB", BodyClip::Body("pb_death_flyback")),
    // The scorestreak tablet coming up and going down (the field computer).
    ("ZHBO_PLR_TBU", BodyClip::Torso("pt_tablet_stand_raise", "pb_stand_alert")),
    ("ZHBO_PLR_TBD", BodyClip::Torso("pt_tablet_stand_putaway", "pb_stand_alert")),
];

/// The W3D body's clips of one animation set, in the Draw module's order: idle, run, aim, fire,
/// reload.
pub const SET_CLIPS: [&str; 5] = ["IDL", "RUN", "ADS", "FIR", "RLD"];

/// The W3D name of animation set `set`'s clip `what` ([`SET_CLIPS`]): the rifleman's
/// `ZHBO_PLR_IDL`, set 3's `ZHBO_PLR_IDL3`.
pub fn set_anim(what: &str, set: usize) -> String {
    if set == 0 {
        format!("ZHBO_PLR_{what}")
    } else {
        format!("ZHBO_PLR_{what}{set}")
    }
}

/// [`set_anims`]: per set the W3D clip names ([`SET_CLIPS`] order), and the clips to convert.
pub(crate) type SetAnims<'a> = (Vec<[String; 5]>, Vec<(String, BodyClip<'a>)>);

/// How the W3D body makes animation set `s`'s clips ([`SET_CLIPS`] order): BO2's stand idle, run
/// and ADS of the set (`thirdperson::SetNames`), and its aimed fire and reload torso over the aim
/// and the idle. A clip the set lacks is the idle's (the W3D body has no fallback chain).
pub(crate) fn set_clips<'a>(s: &'a crate::thirdperson::SetNames) -> Option<[BodyClip<'a>; 5]> {
    use zhbo_native::thirdperson::state;
    let idle = s.state(state::STAND_IDLE)?;
    let run = s.state(state::STAND_RUN).unwrap_or(idle);
    let aim = s.state(state::STAND_ADS).unwrap_or(idle);
    let over = |torso: Option<&'a str>, stance: &'a str| torso.map_or(BodyClip::Body(stance), |t| BodyClip::Torso(t, stance));
    Some([BodyClip::Body(idle), BodyClip::Body(run), BodyClip::Body(aim), over(s.fire.as_deref(), aim), over(s.reload.as_deref(), idle)])
}

/// Every animation set's W3D clips: the names the Draw module plays per set (a set's clip made the
/// same way as the rifleman's is the rifleman's), and the clips to convert (name, how).
pub(crate) fn set_anims(sets: &[crate::thirdperson::SetNames]) -> Result<SetAnims<'_>, String> {
    let mut names = Vec::new();
    let mut convert: Vec<(String, BodyClip)> = Vec::new();
    let rifle = sets.first().and_then(set_clips).ok_or("the rifleman's animation set has no stand idle")?;
    for (k, s) in sets.iter().enumerate() {
        let clips = set_clips(s).unwrap_or(rifle);
        let mut row: [String; 5] = Default::default();
        for (i, how) in clips.into_iter().enumerate() {
            let own = k == 0 || how != rifle[i];
            row[i] = set_anim(SET_CLIPS[i], if own { k } else { 0 });
            if own {
                convert.push((row[i].clone(), how));
            }
        }
        names.push(row);
    }
    Ok((names, convert))
}

fn body_clip(common: &Zone, c: &BodyClip) -> Result<xanim::Anim, String> {
    let find = |n: &str| xanim::find(common, n).map_err(|e| e.to_string());
    Ok(match c {
        BodyClip::Body(n) => find(n)?,
        BodyClip::Torso(torso, stance) => find(torso)?.layered_over(&find(stance)?),
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
/// BO2's smoke grenade raises its screen from script, as the timed effect of this weapon
/// (`level.fx_smokegrenade_single`, `_smokegrenade.gsc:16`; `spawntimedfx` plays the weapon's
/// `projExplosionEffect`, `weapon/grenade/fx_smoke_willy_pete`): the grenade names no effect.
const SMOKE_SCREEN_WEAPON: &str = "smoke_center_mp";
/// The overhead General's bullet impacts (see `ZhboBridge.cpp`, `loadRtsFx`).
pub use zhbo_core::names::RTS_FX_PATH;
/// The FX weapons INI: each converted gun's third-person flash (`FireFX`; its shot is the
/// client's, layer by layer — `FireSound = NONE`, `zhbo_core::soundlayers`) and the overhead
/// General's weapons.
pub const FX_WEAPONS_INI_PATH: &str = "Data\\INI\\Weapon\\ZHBO_BO2_Weapons.ini";
/// The box a BO2 soldier is clicked by (inches: a man standing, with some room).
const SOLDIER_BOX_CENTRE: [f32; 3] = [0.0, 0.0, 36.0];
const SOLDIER_BOX_EXTENT: [f32; 3] = [18.0, 18.0, 38.0];

/// The archive's files: `(path, bytes)`.
pub type Files = Vec<(String, Vec<u8>)>;

/// The weapon table's create-a-class side, before any art: every carryable BO2 weapon with its
/// attachments and alt modes, the camo/reticle/colour options, the attribute bars, the perks, the
/// wildcards, the class items' slots, the weapon groups and the class caps — everything the editor
/// offers and a class names. [`files`] then locks the items whose art does not convert and adds
/// the converted combinations; the real-install sweep of every legal class reads this directly.
/// Returns the table and the statstable roster it was built over.
pub fn class_table(
    bo2_dir: &Path,
    bo2: &[zhbo_formats::weapon::Weapon],
    atts: &[zhbo_formats::weapon::Attachment],
    patch: &Zone,
    patch_ui: &Zone,
    summary: &mut Vec<String>,
) -> (zhbo_core::weapons::Table, crate::weapons::Roster) {
    let names = crate::weapons::names(bo2_dir);
    let meta = crate::weapons::attachment_metadata(patch, &names);
    // BO2's camo choices (`mp/attachmenttable.csv`) and the 48 `camo_<weapon>` sets in
    // `common_patch_mp` (the per-weapon material swap, `WeaponCamo`).
    let camo = crate::weapons::camo_options(patch, &names);
    let reticles = crate::weapons::reticle_options(patch, &names);
    let reticle_colors = crate::weapons::reticle_color_options(patch, &names);
    let roster = crate::weapons::Roster::read(patch);
    let options = crate::weapons::OptionTables {
        roster: &roster,
        camo: &camo,
        reticles: &reticles,
        reticle_colors: &reticle_colors,
    };
    let mut table = crate::weapons::table_with_metadata(bo2, atts, crate::weapons::default_attachment(patch).as_ref(), &names, &meta, &options);
    // The create-a-class bars (`mp/attributestable.csv`), before the combinations copy their base.
    let (bars, att_bars) = crate::weapons::attributes(patch_ui);
    crate::weapons::apply_attributes(&mut table, &bars, &att_bars);
    // The perks: their name, description and what each does (its specialty set, Ghidra G1), from
    // the player's own statstable.
    table.perks = crate::weapons::perk_rows(patch, &names);
    // The wildcards: the same statstable's bonus-card rows (name, description, slot, cost, order).
    table.bonus_cards = crate::weapons::bonus_card_rows(patch, &names);
    // Which weapons Create-a-Class offers, in which slot: the same statstable's weapon items.
    crate::weapons::set_cac_slots(&mut table, patch, &names);
    // The weapon groups' order (the editor's category order), from the UI Lua the conformance's
    // expected side reads.
    match zhbo_conformance::bo2::cac::weapon_groups(patch, patch_ui) {
        Ok(groups) => table.weapon_groups = groups,
        Err(e) => summary.push(format!("weapon groups: {e} (the built-in order stands in)")),
    }
    // The class caps the core reads from data, from the same Lua.
    match zhbo_conformance::bo2::cac::class_caps(patch, patch_ui) {
        Ok(caps) => {
            table.class_caps = zhbo_core::classes::DATA_CAPS.iter().filter_map(|k| caps.get(*k).and_then(|v| v.parse().ok()).map(|v| (k.to_string(), v))).collect();
        }
        Err(e) => summary.push(format!("class caps: {e} (the built-in caps stand in)")),
    }
    (table, roster)
}

/// The DLC packs of the install: each `zone/all/<dlc>_load_mp.ff` (`dlc0`, Revolution's `dlc1`,
/// ...: the zone `Engine.HasDLCForItem`'s pack check stands for, [`crate::weapons::lock_items_without_art`]),
/// sorted.
pub fn installed_dlc(bo2_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(bo2_dir.join("zone").join("all"))
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix("_load_mp.ff").filter(|n| n.starts_with("dlc")).map(str::to_string)).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// Convert everything; returns the archive files and a one-line summary.
pub fn files(bo2_dir: &Path) -> Result<(Files, String), String> {
    // BO2's weapon asset pool, highest priority first (`zhbo_formats::weapon::MP_POOL`).
    let mp = crate::paks::MpPool::load(bo2_dir)?;
    let (bo2, atts) = mp.weapons()?;
    let crate::paks::MpPool { patch, common_patch, common } = mp;
    let weapon = xmodel::find(&common, WEAPON).map_err(|e| e.to_string())?;
    let paks = Paks::open_for(bo2_dir, &[&patch, &common_patch, &common])?;
    let reticle_zone = load_zone(bo2_dir, crate::textures::RETICLE_ZONE).ok();
    let patch_ui = load_zone(bo2_dir, "patch_ui_mp")?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut summary = Vec::new();

    // Every BO2 multiplayer weapon a player can carry, with its attachments and alt modes, as the
    // core's weapon table; art for the converted ones (the loadout first, then the rest).
    let pool = [&patch, &common_patch, &common];
    let camo_assets = zhbo_formats::weapon::camo_all(&common_patch).map_err(|e| e.to_string())?;
    let (mut table, roster) = class_table(bo2_dir, &bo2, &atts, &patch, &patch_ui, &mut summary);
    let statstable = zhbo_formats::stringtable::rows(&patch, "mp/statstable.csv").ok().flatten().unwrap_or_default();
    crate::weapons::lock_items_without_art(
        &mut table,
        &bo2,
        &statstable,
        |w| w.view_model.as_deref().and_then(|m| viewmodels::find_model(&pool, m).ok()).is_none_or(|m| crate::weapons::model_art_converts(&m, |i| paks.resolve(i).is_some())),
        |dlc| zhbo_formats::t6zone::install_path(bo2_dir, &format!("{dlc}_load_mp")).is_file(),
    );
    // Which DLC packs this install has: the lobby shows them, the multiplayer fingerprint leaves
    // them out (a DLC item is its owner's; zhbo_core::content).
    table.installed_dlc = installed_dlc(bo2_dir);
    // The third-person body's animation sets: what BO2's player animation script plays for each
    // weapon's weapClass and playerAnimType. The combinations built next copy their base's set.
    let body_sets = crate::thirdperson::assign_sets(&common, &bo2, &mut table.weapons);
    if !body_sets.dropped().is_empty() {
        let keys: Vec<String> = body_sets.dropped().iter().map(|k| format!("{}/{}", k.class, k.anim_type)).collect();
        summary.push(format!("third-person animation sets past the body's {}: {} keep the rifleman's", zhbo_native::thirdperson::state::MAX_SETS, keys.join(" ")));
    }
    let converted = crate::weapons::converted(&patch, &bo2);
    let converted_base = crate::weapons::converted_base(&bo2, &roster);
    crate::weapons::add_variants(&mut table, &converted, &bo2);
    let camo = crate::camo::files(&common, &patch, &common_patch, &paks, &camo_assets, &bo2, &table, &mut out)?;
    crate::camo::offer_what_converted(&mut table, &camo.with_art);
    summary.push(format!(
        "camo: {} assets, {} pattern images, {} style materials, no art: {}",
        camo.assets,
        camo.images,
        camo.styles,
        if camo.unresolved.is_empty() { "none".to_string() } else { camo.unresolved.join(" ") }
    ));
    #[cfg(debug_assertions)]
    {
        // The schema-driven table (refactor plan 1.4) must survive format+parse unchanged: a
        // schema field the converter forgot to map shows up here, at the machine that has BO2.
        let text = zhbo_core::weapons::format_full_table(&table);
        let back = zhbo_core::weapons::parse_full_table(&text).expect("the converter's table re-parses");
        assert_eq!(zhbo_core::weapons::format_full_table(&back), text, "the converter's table is a fixed point");
    }

    let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(bo2_dir, f.zone)).collect::<Result<_, _>>()?;
    // The Assault Shield's collision boxes, posed on the MP body (the factions share its skeleton).
    if table.weapons.iter().any(|d| crate::weapons::is_shield_id(&d.id, &bo2)) {
        let body = xmodel::find(&zones[0], FACTIONS[0].body).map_err(|e| e.to_string())?;
        let geometry = crate::shield::geometry(&common, &body)?;
        for d in table.weapons.iter_mut().filter(|d| crate::weapons::is_shield_id(&d.id, &bo2)) {
            d.shield = Some(geometry);
        }
    }
    let base_count = converted_base.len();
    let (vm_summary, mut clips) = viewmodels::weapon_viewmodels(&pool, &zones, &paks, &mut out, &bo2, &atts, &mut table.weapons, &converted, base_count)?;
    summary.push(vm_summary);
    // The weapon emblem: each faction's Zero Hour logo, from the player's own Zero Hour install.
    match zhbo_requirements::check_requirements().zero_hour {
        Ok(zh) => summary.push(crate::emblems::faction_emblems(&zh.path, &mut out)),
        Err(e) => summary.push(format!("faction emblems skipped: {e}")),
    }
    // The CaC reticle options as lens materials, one blob for every optic.
    // The store rows' materials (`reticle_11..37`) live only in `patch_mp`.
    let reticle_zones: Vec<&Zone> = reticle_zone.iter().chain([&patch, &common]).collect();
    let reticles = crate::reticles::reticles(&patch, &reticle_zones, &paks, &mut out);
    summary.push(format!("{} reticle option sets", reticles.sets.len()));
    out.insert(zhbo_native::reticles::reticles_path(), reticles.to_bytes());
    // The target finder's lens display: the atlas and the sprite table its code draws.
    match crate::rangefinder::display(bo2_dir, &reticle_zones, &paks, &mut out) {
        Ok(d) => {
            out.insert(zhbo_native::display::display_path(), d.to_bytes());
            summary.push("the target finder's display".into());
        }
        Err(e) => summary.push(format!("the target finder's display skipped: {e}")),
    }

    // Track R7: the third-person body's clips. BO2's MP bodies share the player skeleton, so one
    // set serves every faction; each faction's body manifest names them (`zhbo_native::body_path`).
    let (body_clips, body_unresolved) = crate::thirdperson::clips(&common, &body_sets);
    for c in &body_clips {
        crate::native::put_anim(&mut out, &c.anim);
    }
    if !body_unresolved.is_empty() {
        summary.push(format!("native body clips missing for states {body_unresolved:?}"));
    }
    summary.push(format!(
        "third-person animation sets: {}",
        (0..body_sets.sets.len()).map(|k| format!("{k}={}", body_sets.keys().filter(|(_, s)| *s as usize == k).map(|(key, _)| format!("{}/{}", key.class, key.anim_type)).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(" ")
    ));
    summary.push(format!("native body clips: {}", body_clips.iter().map(|c| format!("{}=`{}`", c.state, c.anim.name)).collect::<Vec<_>>().join(", ")));
    // The soldier's hit boxes (the MP bodies share them) in every body state, for the sim's trace;
    // standing is the one every soldier without a sim body state takes.
    if !body_clips.iter().any(|c| c.state == zhbo_native::thirdperson::state::STAND_IDLE) {
        return Err("no stand-idle body clip for the hit boxes".into());
    }
    table.hit_boxes = crate::thirdperson::hit_boxes(&xmodel::find(&zones[0], FACTIONS[0].body).map_err(|e| e.to_string())?, &body_clips);
    summary.push(format!("soldier hit boxes: {} over {} body states", table.hit_boxes.len(), body_clips.len()));
    // The shellshocks the GSC plays (`shock/<name>.shock` in the weapon pool).
    table.shocks = crate::weapons::shocks(&pool)?;
    summary.push(format!("{} shellshocks", table.shocks.len()));

    // The soldiers: body, every converted weapon in the right hand (one shown at a time), and the
    // tablet.
    let mut rig = Rig::new(&xmodel::find(&zones[0], FACTIONS[0].body).map_err(|e| e.to_string())?, &[(&weapon, character::WEAPON_TAG)]);
    let mut looks = textures::Looks::default();
    // Every legal combination gets its native world-weapon manifest (the shared models deduped by
    // name); only the base set's meshes ride the W3D body, and a combination shows its base weapon
    // there (`Catalog::world_viewmodel`).
    let (held, flashes) = viewmodels::world_weapons(&pool, &paks, &mut out, &rig, &bo2, &atts, &mut table.weapons, &converted, base_count, &mut looks);
    let tablet = viewmodels::tablet_world(&pool, &paks, &mut out, &rig, &mut looks);
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
    let (set_names, set_convert) = set_anims(&body_sets.sets)?;
    let mut ini = BASE_INI.to_string();
    for (f, zone) in FACTIONS.iter().zip(&zones) {
        let mut body = xmodel::find(zone, f.body).map_err(|e| e.to_string())?;
        // The faction zone loads after `common_mp` and references the shared eye, mouth and
        // datapad materials it defines.
        viewmodels::resolve_materials(&[zone, &common], &mut body);
        let body_tex = textures::textures(&body, &paks, &mut out, &mut looks);
        // Track R7: the same body as a native blob (skeleton, skin weights and surfaces), with the
        // clip of every movement state, for the renderer's native BO2 pass.
        let native_body = crate::native::model(&body, &body_tex, &looks);
        let native_body_path = crate::native::put_model(&mut out, &native_body);
        let body_manifest = zhbo_native::thirdperson::Body {
            faction: f.letter as u8,
            model: native_body_path,
            weapon_tag: character::WEAPON_TAG.to_string(),
            clips: body_clips.iter().map(|c| (c.state, zhbo_native::anim_path(&c.anim.name))).collect(),
        };
        out.insert(zhbo_native::body_path(f.letter), body_manifest.to_bytes());
        let mut meshes = character::skin(&rig, &body, "B", &body_tex);
        textures::finish(&mut meshes, &looks);
        meshes.extend(held.iter().cloned());
        meshes.extend(tablet.iter().cloned());
        // A box around the standing soldier to click him by (his skin is unposed by the feet).
        let pick = (character::scaled(SOLDIER_BOX_CENTRE), character::scaled(SOLDIER_BOX_EXTENT));
        out.insert(format!("Art\\W3D\\{}.w3d", f.model), w3d::skin_model_with_box(f.model, HIERARCHY, &meshes, Some(pick)));
        ini = replace_draw(&ini, f.object, &draw_module(f.model, &hidden, guns, &set_names))?;
        summary.push(format!("{} = {} ({} meshes)", f.object, f.body, meshes.len()));
    }
    summary.push(format!("third person: {} weapon meshes, {} tablet meshes", held.len(), tablet.len()));
    if !looks.dropped.is_empty() {
        summary.push(format!("surfaces left out: {}", looks.dropped.iter().cloned().collect::<Vec<_>>().join(" ")));
    }
    out.insert(format!("Art\\W3D\\{HIERARCHY}.w3d"), rig.hierarchy_file(HIERARCHY));
    for (w3d_name, how) in ANIMS.iter().map(|(n, h)| (n.to_string(), *h)).chain(set_convert) {
        let w3d_name = w3d_name.as_str();
        let anim = body_clip(&common, &how)?;
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
    summary.push(crate::props::projectiles(&pool, &paks, &mut out, &bo2, &mut table.weapons, &converted_base, &mut projectile_models));
    crate::props::share_projectiles(&mut table.weapons);
    summary.push(crate::props::drops(&pool, &paks, &mut out, &bo2, &mut table.weapons, &converted_base));

    // BO2 sound effects: the general ones, each converted weapon's own shots, and the viewmodel
    // clips' sound notetracks.
    let world = load_zone(bo2_dir, crate::sfx::WORLD_ZONE)?;
    // A hit's or blast's sound is its impact type's family alias for the surface
    // (`impacts::sound_rows`), from whichever bank has it (the bullet and bolt families live only in
    // the map's), like every converter event.
    let alias_names: std::collections::HashSet<String> = [&common, &world]
        .into_iter()
        .flat_map(|z| zhbo_formats::sndalias::banks(z).unwrap_or_default())
        .filter(|b| b.name.starts_with("mpl_"))
        // An alias with no sound in any variant (`prj_bullet_impact_small_snow`: asset 0, no
        // secondary) plays nothing in BO2 either: it is a `NONE` row, not a dangling event.
        .flat_map(|b| b.aliases.into_iter().filter(|a| a.variants.iter().any(|v| v.asset_id != 0 || v.secondary.is_some())).map(|a| a.name.to_ascii_lowercase()))
        .collect();
    // A bounce cell whose alias no bank holds is silent, as BO2's hash miss (`wpn_grenade_bounce_snow`).
    for d in &mut table.weapons {
        crate::impacts::silence_missing(&mut d.bounce_sounds, &|a| alias_names.contains(&a.to_ascii_lowercase()));
    }
    let (mut extra, mut fx_ini) = crate::weapons::sounds_and_fx(&bo2, &atts, &mut table.weapons, &converted_base);
    let blast_types = crate::impacts::blast_types(&table.weapons);
    // The bullet buckets the guns carry and every blast type: BO2's impact table and sounds for
    // each (`fx::ImpactKind`).
    let impact_types: Vec<u32> = {
        let mut t: Vec<u32> = table
            .weapons
            .iter()
            .filter(|d| d.class.is_gun() && zhbo_formats::fx::bullet_entry(d.impact_type as u32).is_some())
            .map(|d| d.impact_type as u32)
            .chain(blast_types.iter().copied())
            .chain(crate::impacts::bounce_dud_types(&table.weapons))
            .collect();
        t.sort_unstable();
        t.dedup();
        t
    };
    let (impact_sounds, blast_aliases) = crate::impacts::sound_rows(&impact_types, &|a| alias_names.contains(&a.to_ascii_lowercase()));
    extra.extend(blast_aliases.into_iter().map(|a| crate::sfx::DynSfx {
        event: zhbo_core::names::def_sound_event(&a),
        layers: vec![a],
        kind: crate::sfx::Kind::World,
        limit: 2,
    }));
    // The shellshocks' ringing loops (`Shock … SoundLoop`), each a looping 2D event the client
    // starts and stops (`shellshock::Presented`).
    let mut loops: Vec<&str> = table.shocks.iter().map(|s| s.sound_loop.as_str()).filter(|a| !a.is_empty()).collect();
    loops.sort_unstable();
    loops.dedup();
    extra.extend(loops.into_iter().map(|a| crate::sfx::DynSfx { event: zhbo_core::shellshock::loop_event(a).to_string(), layers: vec![a.to_string()], kind: crate::sfx::Kind::PlayerLoop, limit: 1 }));
    let (sound_files, sound_summary) = crate::sfx::files(&common, &world, bo2_dir, &clips, &extra)?;
    out.extend(sound_files);
    summary.push(sound_summary);

    // BO2's own explosions for the converted weapons (and equipment breaking), as Zero Hour
    // particle systems.
    let fx_pool = [&patch, &common_patch, &common];
    let mut effects = crate::fx::Converted::with_pool(&fx_pool);
    let mut ids: Vec<String> = crate::weapons::with_alt_modes(&table.weapons, &converted_base);
    ids.extend(table.weapons.iter().filter(|d| d.id.contains('~')).map(|d| d.id.clone()));
    // Each gun's muzzle flash as others see it (BO2's `worldFlashEffect`), by weapon id.
    let mut flashes: BTreeMap<String, String> = BTreeMap::new();
    for id in &ids {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(d) = table.weapons.iter_mut().find(|d| &d.id == id) else { continue };
        if let Some(f) = &w.world_flash_effect {
            // Played at the muzzle bone with its matrix (X down the barrel), like BO2's.
            flashes.insert(id.clone(), effects.add_framed(f, crate::fx::Role::Flash, &|h| paks.image(h)));
        }
        if let Some(f) = &w.view_flash_effect {
            // Only claim it when the converter can draw it (sprites or runner children). An effect
            // with only a light or a model would leave an empty FXList, and the bridge's generic
            // gas flash is better than nothing.
            let drawable = f.elems.iter().any(|e| {
                zhbo_formats::fx::kind::is_sprite(e.kind) || e.kind == zhbo_formats::fx::kind::RUNNER
            });
            if drawable {
                d.view_flash_fx = effects.add_framed(f, crate::fx::Role::Flash, &|h| paks.image(h));
            }
        }
        // The shared suppressor flash pair (BO2's `fx_muz_silencer_md` + `_3p`). BO2's
        // `AttachmentUnique` carries no flash fields (weapon.rs:127-175), so the shared pair is the
        // value when the weapon offers a silenced attachment.
        if w.attachment_variants.iter().any(|(kind, _)| atts.iter().any(|a| a.kind == *kind && a.silenced)) {
            if let Some(e) = zhbo_formats::fx::find(&common, crate::fx::SILENCER_VIEW_FLASH) {
                d.silenced_view_flash_fx = effects.add_framed(&e, crate::fx::Role::Flash, &|h| paks.image(h));
            }
            if !d.silenced_fx_weapon.is_empty() {
                if let Some(e) = zhbo_formats::fx::find(&common, crate::fx::SILENCER_WORLD_FLASH) {
                    let world_fx = effects.add_framed(&e, crate::fx::Role::Flash, &|h| paks.image(h));
                    fx_ini = ini_set(&fx_ini, "Weapon", &d.silenced_fx_weapon, "FireFX", &world_fx)?;
                }
            }
        }
        if let Some(f) = &w.view_shell_eject_effect {
            // The patch zone's copies of the shell models and materials are reference stubs for
            // some families (the pistol's live in `common_mp`); the shell resolver tries these in
            // order, per model and per material.
            d.view_shell_fx = effects.add_shell(f, &[&patch, &common], &|h| paks.image(h));
        }
        if let Some(f) = &w.view_last_shot_eject_effect {
            d.view_last_shot_shell_fx = effects.add_shell(f, &[&patch, &common], &|h| paks.image(h));
        }
        // A dud's own effect (`projDudEffect`, event 0x51), over the table's `projectile_dud` cell.
        if let Some(e) = &w.projectile_dud_effect {
            if d.activate_dist > 0.0 {
                d.dud_fx = effects.add_as(e, crate::fx::Role::Impact, &|h| paks.image(h));
            }
        }
        // BO2's smoke grenade plays its cloud from script (the timed effect of
        // `SMOKE_SCREEN_WEAPON`), standing the tuning pack's screen time, as an area effect.
        if d.effect == zhbo_core::weapons::Effect::Smoke {
            // The pooled weapon can come from a zone whose copy of the effect's materials are
            // reference stubs: the effect is read again by its name from `common_mp`, which holds
            // the real one.
            let screen = || {
                let e = bo2.iter().find(|s| s.name == SMOKE_SCREEN_WEAPON).and_then(|s| s.explosion_effect.clone())?;
                zhbo_formats::fx::find(&common, &e.name).or(Some(e))
            };
            if let Some(e) = w.explosion_effect.clone().or_else(screen) {
                d.explosion_fx = effects.add_area(&e, zhbo_core::tuning::get().smoke_duration, &|h| paks.image(h));
            }
            continue;
        }
        // The def's own `projExplosionEffect`, played on top of the impact table's cell (BO2's
        // `FUN_004c0ac0`); most explosives leave it empty and blow up with the table's alone.
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
    // What others see of a shot besides the flash: the world brass and the tracer, as FX weapons
    // the sim fires (every def, combinations included).
    let (shot_ini, placed) = crate::shotfx::convert(&mut table.weapons, &bo2, &pool, &mut effects, &|h| paks.image(h));
    fx_ini.push_str(&shot_ini);
    // The overhead General fires Zero Hour's own weapon: his flash, brass and tracer are one FXList.
    let mut rts_flashes = flashes.clone();
    for (id, flash) in &flashes {
        if let Some(fx) = crate::shotfx::rts_fire_fx(id, flash, &bo2, &placed, &pool, &mut effects, &|h| paks.image(h)) {
            rts_flashes.insert(id.clone(), fx);
        }
    }
    // The overhead General's weapon for every player weapon (swapped in by class and order).
    let has_event = |name: &str| extra.iter().any(|e| e.event == name);
    fx_ini.push_str(&crate::weapons::rts_weapons_ini(&table.weapons, &rts_flashes, &has_event));
    out.insert(FX_WEAPONS_INI_PATH.into(), fx_ini.into_bytes());
    // Under RTS control: the rifle's and pistol's BO2 hits on what they shoot, by what it is
    // (ZhboBridge plays them as the shots land).
    let loadout = zhbo_core::weapons::Loadout::default();
    let mut rts_fx = String::from("; slot, then BO2's bullet impact on a soldier, a vehicle and anything else (FXLists)\n");
    for (slot, id) in [(0, &loadout.primary), (1, &loadout.secondary)] {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(entry) = zhbo_formats::fx::bullet_entry(w.impact_type) else { continue };
        let mut add = |e: Option<zhbo_formats::fx::Effect>| e.map_or("NONE".to_string(), |e| effects.add_as(&e, crate::fx::Role::Impact, &|h| paks.image(h)));
        let flesh = add(zhbo_formats::fx::impact_flesh(&common, entry));
        let metal = add(zhbo_formats::fx::impact(&common, entry, zhbo_formats::fx::surface::METAL));
        let hard = add(zhbo_formats::fx::impact(&common, entry, zhbo_formats::fx::surface::CONCRETE));
        rts_fx.push_str(&format!("{slot} {flesh} {metal} {hard}\n"));
    }
    out.insert(RTS_FX_PATH.into(), rts_fx.into_bytes());
    // R8d: BO2's whole impact table for the bullet buckets the converted guns use and the impact
    // types of everything that goes off (`impact_types`, above), keyed by the mod's surface slots.
    // Must run before the effects INI is written.
    let (mut impact_text, unmapped) = crate::impacts::table_text(&common, &impact_types, &mut effects, &|h| paks.image(h));
    impact_text.push_str(&impact_sounds);
    if !unmapped.is_empty() {
        summary.push(format!("impact types with no BO2 row (no impact FX): {unmapped:?}"));
    }
    summary.push(format!("{} BO2 impact cells", impact_text.lines().filter(|l| !l.starts_with(';')).count()));
    out.insert(zhbo_core::names::IMPACT_FX_PATH.into(), impact_text.into_bytes());
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

    // The combinations were built before the passes above filled their bases' sounds and FX.
    crate::weapons::inherit_converted_assets(&mut table, &bo2);
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
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, common_patch, common } = mp;
        let pool = [&patch, &common_patch, &common];
        let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(&bo2_dir, f.zone).unwrap()).collect();
        let paks = Paks::open(&bo2_dir).unwrap();
        let names = crate::weapons::names(&bo2_dir);
        // The weapon's own first-person effects are parsed (R8) and bound as BO2's asset pool binds
        // them: `common_mp` holds `weapon/shellejects/fx_rifle` only as a reference stub, the full
        // asset is `patch_mp`'s — so the AN-94, the other ARs and the snipers DO eject brass.
        let an94 = bo2.iter().find(|w| w.name == "an94_mp").expect("an94_mp");
        assert!(an94.view_flash_effect.is_some(), "the AN-94 has a viewFlashEffect");
        assert_eq!(an94.view_shell_eject_effect.as_ref().map(|e| e.name.as_str()), Some("weapon/shellejects/fx_rifle"), "bound from patch_mp");
        let shell = |n: &str| bo2.iter().find(|w| w.name == n).unwrap_or_else(|| panic!("{n}")).view_shell_eject_effect.is_some();
        for n in ["m32_mp", "crossbow_mp"] {
            assert!(!shell(n), "{n} defines none and must eject nothing");
        }
        for n in ["mp7_mp", "mk48_mp", "870mcs_mp", "hk416_mp", "dsr50_mp"] {
            assert!(shell(n), "{n} defines its own shell effect");
        }
        // Without the later zone the stub stays unbound: the pool, not the parse, decides.
        let common_only = zhbo_formats::weapon::all(&common).unwrap();
        assert!(common_only.iter().find(|w| w.name == "an94_mp").unwrap().view_shell_eject_effect.is_none());
        for w in &bo2 {
            assert_eq!(w.view_shell_eject_effect.is_some(), w.view_last_shot_eject_effect.is_some(), "{}: the last-shot twin", w.name);
        }
        let mut defs: Vec<zhbo_core::weapons::WeaponDef> =
            { let roster = crate::weapons::Roster::read(&patch); bo2.iter().filter(|w| roster.is_player_weapon(w)).map(|w| crate::weapons::to_def(w, &names)).collect() };
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
            match weapon_viewmodels(&pool, &zones, &paks, &mut out, &bo2, &atts, &mut defs, std::slice::from_ref(id), 1) {
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

    /// BO2's balance patches (`common_patch_mp` over `common_mp`, `zhbo_formats::weapon::MP_POOL`):
    /// every patched weapon converts from the patch's definition, not the launch one, and plays it
    /// in the simulation — a weapon whose magazine the patch changed (the M8A1's 36 → 32) empties
    /// the patched magazine under held fire. Each patched attachment converts from the patch too,
    /// and the Peacekeeper (only in the patch zone) is a player weapon with a converted viewmodel.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_balance_patches_convert_and_play() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (pooled, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, common_patch, common } = mp;
        let names = crate::weapons::names(&bo2_dir);
        let launch = zhbo_formats::weapon::all(&common).unwrap();
        let patched = zhbo_formats::weapon::all_in(&common_patch, &[&patch, &common]).unwrap();
        assert!(patched.len() > 70, "{} patched weapons", patched.len());
        let mut clip_changes = 0;
        let roster = crate::weapons::Roster::read(&patch);
        for p in patched.iter().filter(|w| roster.is_player_weapon(w)) {
            let got = pooled.iter().find(|w| w.name == p.name).unwrap_or_else(|| panic!("{} pooled", p.name));
            let def = crate::weapons::to_def(got, &names);
            assert_eq!(def, crate::weapons::to_def(p, &names), "{}: the patch's definition", p.name);
            let Some(old) = launch.iter().find(|w| w.name == p.name) else { continue };
            if old.clip_size != p.clip_size {
                clip_changes += 1;
                assert_eq!(def.clip as i32, p.clip_size, "{}", p.name);
                let fails = zhbo_core::conformance::check(Box::leak(Box::new(def)));
                assert!(fails.is_empty(), "{}: {fails:?}", p.name);
            }
        }
        assert!(clip_changes > 0, "the patch changes a magazine");
        for a in zhbo_formats::weapon::attachments(&common_patch).unwrap().iter().filter(|a| !a.name.starts_with(',')) {
            assert_eq!(atts.iter().find(|x| x.name == a.name), Some(a), "{}: the patch's attachment", a.name);
        }
        // The Peacekeeper: only the patch zone defines it, its models and its clips.
        let pk = pooled.iter().find(|w| w.name == "peacekeeper_mp").expect("peacekeeper_mp in the pool");
        assert!(roster.is_player_weapon(pk), "the statstable lists the Peacekeeper");
        assert!(launch.iter().all(|w| w.name != pk.name));
        let pool = [&patch, &common_patch, &common];
        let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(&bo2_dir, f.zone).unwrap()).collect();
        let paks = Paks::open(&bo2_dir).unwrap();
        let mut defs = vec![crate::weapons::to_def(pk, &names)];
        let mut out = BTreeMap::new();
        let (_, clips) = weapon_viewmodels(&pool, &zones, &paks, &mut out, &pooled, &atts, &mut defs, std::slice::from_ref(&pk.name), 1).unwrap();
        assert!(!clips.is_empty(), "the Peacekeeper's clips convert from common_patch_mp");
    }

    /// Every projectile the install converts to — the launchers, the under-barrel grenades, the
    /// crossbow (every legal combination: stack fire's three bolts each), the ballistic knife and
    /// every thrown offhand — hurts a BO2 soldier by BO2's own data at 150, 600 and 1200 inches
    /// (`zhbo_core::conformance::projectile`): the strike's `iDamage`, the impact fuse and its
    /// arming distance, a bolt's spawned grenade going off on its fuse, the blast's falloff. And the
    /// defs carry what BO2's say: a `bolt` explosion's `szSpawnedGrenadeWeaponName` is the def's
    /// spawned grenade and is in the table; `stickiness` all/all_orient is sticky.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_projectile_hurts_by_bo2s_data() {
        use zhbo_formats::weapon::{proj_explosion, stickiness};
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, .. } = mp;
        let names = crate::weapons::names(&bo2_dir);
        let roster = crate::weapons::Roster::read(&patch);
        let mut table = crate::weapons::table(&bo2, &atts, crate::weapons::default_attachment(&patch).as_ref(), &names, &roster);
        crate::weapons::add_variants(&mut table, &crate::weapons::converted(&patch, &bo2), &bo2);
        let defs: &'static [zhbo_core::weapons::WeaponDef] = Box::leak(table.weapons.into_boxed_slice());
        let resolve = |id: &str| defs.iter().find(|d| d.id == id);
        let mut fails = Vec::new();
        for w in bo2.iter().filter(|w| roster.is_player_weapon(w)) {
            let Some(d) = resolve(&w.name) else { continue };
            let spawns = (w.proj_explosion == proj_explosion::BOLT).then(|| w.spawned_grenade_weapon.clone()).flatten().unwrap_or_default();
            if d.spawns != spawns {
                fails.push(format!("{}: spawns {:?}, BO2's bolt spawns {spawns:?}", w.name, d.spawns));
            }
            if !spawns.is_empty() && resolve(&spawns).is_none() {
                fails.push(format!("{}: its spawned grenade {spawns} is not in the table", w.name));
            }
            let sticks = matches!(w.stickiness, stickiness::ALL | stickiness::ALL_ORIENT) && d.effect != zhbo_core::weapons::Effect::Impact;
            if d.sticky != sticks {
                fails.push(format!("{}: sticky {}, BO2's stickiness {}", w.name, d.sticky, w.stickiness));
            }
        }
        let swept: Vec<&str> = defs.iter().filter(|d| zhbo_core::conformance::projectile::is_projectile(d)).map(|d| d.id.as_str()).collect();
        for d in defs.iter().filter(|d| zhbo_core::conformance::projectile::is_projectile(d)) {
            fails.extend(zhbo_core::conformance::projectile::check(d, &resolve));
        }
        println!("{} projectile weapons swept: {}", swept.len(), swept.join(" "));
        for id in ["crossbow_mp", "crossbow_mp+stackfire", "knife_ballistic_mp", "smaw_mp", "usrpg_mp", "fhj18_mp", "gl_an94_mp", "frag_grenade_mp", "sticky_grenade_mp", "hatchet_mp"] {
            assert!(swept.contains(&id), "{id} is swept");
        }
        assert!(fails.is_empty(), "{fails:#?}");
    }

    /// Every explosive the install converts to (launchers, launcher grenades, thrown grenades, the
    /// crossbow's bolt grenade, placed charges and mines, the stuns) against every class of Zero
    /// Hour object, at several distances from its solid: BO2's radius damage (falloff over the
    /// distance to the bounds) and, for an impact fuse, the struck one's strike by BO2's vehicle
    /// rule while its neighbour takes the blast (`zhbo_core::conformance::splash`).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_explosive_hurts_zero_hour_objects_by_bo2s_radius_damage() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, .. } = mp;
        let names = crate::weapons::names(&bo2_dir);
        let roster = crate::weapons::Roster::read(&patch);
        let mut table = crate::weapons::table(&bo2, &atts, crate::weapons::default_attachment(&patch).as_ref(), &names, &roster);
        crate::weapons::add_variants(&mut table, &crate::weapons::converted(&patch, &bo2), &bo2);
        let defs: &'static [zhbo_core::weapons::WeaponDef] = Box::leak(table.weapons.into_boxed_slice());
        let swept: Vec<&zhbo_core::weapons::WeaponDef> = defs.iter().filter(|d| zhbo_core::conformance::splash::is_explosive(d)).collect();
        let fails: Vec<String> = swept.iter().flat_map(|d| zhbo_core::conformance::splash::check(d)).collect();
        let ids: Vec<&str> = swept.iter().map(|d| d.id.as_str()).collect();
        println!("{} explosives swept: {}", ids.len(), ids.join(" "));
        for id in ["smaw_mp", "usrpg_mp", "fhj18_mp", "gl_an94_mp", "frag_grenade_mp", "sticky_grenade_mp", "satchel_charge_mp", "claymore_mp", "bouncingbetty_mp", "explosive_bolt_mp", "concussion_grenade_mp", "proximity_grenade_mp"] {
            assert!(ids.contains(&id), "{id} is swept");
        }
        assert!(fails.is_empty(), "{fails:#?}");
    }

    /// Every attachment BO2 offers on every weapon: the weapon with it (and its alt mode) plays by
    /// the attachment's numbers, and its viewmodel with the attachment's model converts.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_attachment_converts_and_plays() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, common_patch, common } = mp;
        let pool = [&patch, &common_patch, &common];
        let zones: Vec<Zone> = FACTIONS.iter().map(|f| load_zone(&bo2_dir, f.zone).unwrap()).collect();
        let paks = Paks::open(&bo2_dir).unwrap();
        let names = crate::weapons::names(&bo2_dir);
        let table = crate::weapons::table(&bo2, &atts, crate::weapons::default_attachment(&patch).as_ref(), &names, &crate::weapons::Roster::read(&patch));
        let mut fails = Vec::new();
        let mut combos = 0;
        for base in table.weapons.iter().filter(|d| !d.allowed.is_empty()) {
            for (short, _) in &base.allowed {
                let id = zhbo_core::attachments::variant_id(&base.id, &[short.as_str()]);
                let mut t = table.clone();
                crate::weapons::add_variants(&mut t, std::slice::from_ref(&id), &bo2);
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
                match weapon_viewmodels(&pool, &zones, &paks, &mut out, &bo2, &atts, &mut t.weapons, std::slice::from_ref(&id), 1) {
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
        assert!(maps.pixels.as_chunks::<4>().0.iter().all(|p| *p == [100, 150, 40, 200]));
        let flat = surface_maps(2, 2, None, None);
        assert!(flat.pixels.as_chunks::<4>().0.iter().all(|p| *p == [128, 128, 0, 0]), "no maps: flat and matte");
        let mut color = solid(4, 4, [90, 90, 90, 7]);
        put_occlusion(&mut color, Some(&solid(2, 2, [60, 0, 0, 255])));
        assert!(color.pixels.as_chunks::<4>().0.iter().all(|p| *p == [90, 90, 90, 60]));
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
        let sets: Vec<[String; 5]> = (0..2).map(|k| SET_CLIPS.map(|c| set_anim(c, k))).collect();
        for f in FACTIONS {
            let out = replace_draw(BASE_INI, f.object, &draw_module(f.model, &["W0100".to_string()], (0, 1), &sets)).unwrap();
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
        let most = zhbo_native::thirdperson::state::MAX_SETS as usize - 1;
        assert!(SET_CLIPS.iter().all(|c| set_anim(c, most).len() <= 15), "a set's W3D clip names fit W3D's 15 characters");
    }

    /// The Draw module plays each animation set's clips under its model condition, before the
    /// rifleman's: Zero Hour takes the state matching the most of the unit's conditions, so a set's
    /// state (its flag on top of the rifleman's conditions) wins while the flag is up, and a
    /// shared state (dying, throwing) gets the flag too so it still wins over the set's idle.
    #[test]
    fn each_animation_set_has_its_own_states_under_its_condition() {
        let sets: Vec<[String; 5]> = (0..3).map(|k| SET_CLIPS.map(|c| set_anim(c, k))).collect();
        let d = draw_module("M", &[], (0, 1), &sets);
        let anim_of = |cond: &str| {
            let at = d.find(&format!("ConditionState        = {cond}\n")).or_else(|| d.find(&format!("AliasConditionState   = {cond}\n")).map(|alias| d[..alias].rfind("ConditionState        = ").unwrap()))?;
            let block = &d[at..];
            let anim = block.lines().find_map(|l| l.trim().strip_prefix("Animation           = "))?;
            Some(anim.trim_start_matches(&format!("{HIERARCHY}.")).to_string())
        };
        let flag = |k: usize| zhbo_core::names::ANIM_SET_CONDITIONS[k - 1];
        assert_eq!(anim_of("MOVING").as_deref(), Some("ZHBO_PLR_RUN"));
        assert_eq!(anim_of(&format!("MOVING {}", flag(1))).as_deref(), Some("ZHBO_PLR_RUN1"), "set 1 runs its own run");
        assert_eq!(anim_of(&format!("MOVING FIRING_B {}", flag(2))).as_deref(), Some("ZHBO_PLR_RUN2"));
        assert_eq!(anim_of(flag(2)).as_deref(), Some("ZHBO_PLR_IDL2"));
        assert_eq!(anim_of(&format!("FIRING_B {}", flag(1))).as_deref(), Some("ZHBO_PLR_FIR1"), "either gun slot fires the weapon in hand");
        assert_eq!(anim_of("RELOADING_B").as_deref(), Some("ZHBO_PLR_RLD"), "a rifle in the second slot reloads as a rifle");
        assert_eq!(anim_of(&format!("DYING {}", flag(2))).as_deref(), Some("ZHBO_PLR_DTA"));
        assert_eq!(anim_of(&format!("PREATTACK_C {}", flag(1))).as_deref(), Some("ZHBO_PLR_THR"));
        // A set's states come before the rifleman's (ties go to the first).
        assert!(d.find(&format!("= MOVING {}\n", flag(1))).unwrap() < d.find("= MOVING\n").unwrap());
        assert!(!d.contains("JAMMED"), "the sidearm's old condition is gone");
    }

    /// The converter's whole table (weapons, attachments, alt-mode variants) formatted and parsed
    /// back through the schema must come out identical (refactor plan 1.4's fixed-point check; the
    /// debug assertion in `files` guards it on every conversion too).
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn weapon_table_is_a_fixed_point() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, .. } = mp;
        let names = crate::weapons::names(&bo2_dir);
        let mut table = crate::weapons::table(&bo2, &atts, crate::weapons::default_attachment(&patch).as_ref(), &names, &crate::weapons::Roster::read(&patch));
        let converted = crate::weapons::converted(&patch, &bo2);
        crate::weapons::add_variants(&mut table, &converted, &bo2);
        let text = zhbo_core::weapons::format_full_table(&table);
        let back = zhbo_core::weapons::parse_full_table(&text).expect("the table re-parses");
        assert_eq!(zhbo_core::weapons::format_full_table(&back), text, "format ∘ parse is the identity on the converter's table");
    }

    /// The synthesized per-combination defs carry the mounted optic's overlay, not the base's: the
    /// base AS50 keeps `scope_overlay_xpr50`, the ACOG's empty unique means no overlay, and FMJ
    /// leaves the base scope (so the base fields stand). The production `converted` list only
    /// carries the loadout's combo, so this pins the AS50 combos explicitly.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_synthesized_combos_overlay_follows_the_mounted_optic() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, .. } = mp;
        let names = crate::weapons::names(&bo2_dir);
        let mut table = crate::weapons::table(&bo2, &atts, crate::weapons::default_attachment(&patch).as_ref(), &names, &crate::weapons::Roster::read(&patch));
        crate::weapons::add_variants(&mut table, &["as50_mp+acog".to_string(), "as50_mp+fmj".to_string()], &bo2);
        let def = |id: &str| table.weapons.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id}"));
        assert_eq!(def("as50_mp").overlay_material, "scope_overlay_xpr50");
        assert!(def("as50_mp+acog").overlay_material.is_empty(), "the ACOG's unique has no overlay");
        assert_eq!(def("as50_mp+fmj").overlay_material, "scope_overlay_xpr50", "FMJ leaves the base scope");
    }

    /// The third-person tear, across every base weapon: each weapon's world meshes on the soldier
    /// are exactly `W<vm>00 .. W<vm><n-1>` with `n` its `WorldMeshes` (the bridge shows `k < n`, so a
    /// gap hides every surface after it — the MP7 drew two of four), every vertex rides the hand,
    /// and every triangle indexes a finite vertex.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_world_weapon_shows_all_its_meshes() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let mp = crate::paks::MpPool::load(&bo2_dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let crate::paks::MpPool { patch, common_patch, common } = mp;
        let pool = [&patch, &common_patch, &common];
        let paks = Paks::open(&bo2_dir).unwrap();
        let names = crate::weapons::names(&bo2_dir);
        // The attachments carry BO2's compatibility lists (the metadata), as `files` builds them:
        // without them the loadout's `an94_mp+gl+reflex` sanitizes to a different combination.
        let meta = crate::weapons::attachment_metadata(&patch, &names);
        let opts = crate::weapons::OptionTables { roster: &crate::weapons::Roster::read(&patch), camo: &[], reticles: &[], reticle_colors: &[] };
        let mut table = crate::weapons::table_with_metadata(&bo2, &atts, crate::weapons::default_attachment(&patch).as_ref(), &names, &meta, &opts);
        let ids = crate::weapons::converted_base(&bo2, opts.roster);
        crate::weapons::add_variants(&mut table, &ids, &bo2);
        let weapon = xmodel::find(&common, WEAPON).unwrap();
        let body = xmodel::find(&load_zone(&bo2_dir, FACTIONS[0].zone).unwrap(), FACTIONS[0].body).unwrap();
        let rig = Rig::new(&body, &[(&weapon, character::WEAPON_TAG)]);
        let hand = rig.pivot(character::WEAPON_TAG).unwrap() as u16;
        let mut out = BTreeMap::new();
        let mut looks = textures::Looks::default();
        let (held, _) = viewmodels::world_weapons(&pool, &paks, &mut out, &rig, &bo2, &atts, &mut table.weapons, &ids, ids.len(), &mut looks);
        let mut fails = Vec::new();
        let mut shown = 0;
        for (vm, id) in ids.iter().enumerate() {
            let Some(d) = table.weapons.iter().find(|d| &d.id == id) else { continue };
            let prefix = format!("W{vm:02}");
            let mut mine: Vec<&str> = held.iter().map(|m| m.name.as_str()).filter(|n| n.starts_with(&prefix) && n.len() == prefix.len() + 2).collect();
            mine.sort_unstable();
            let want: Vec<String> = (0..d.world_meshes as usize).map(|k| world_mesh(vm as i32, k)).collect();
            if mine != want {
                fails.push(format!("{id}: meshes {mine:?}, the bridge shows {want:?}"));
            }
            shown += mine.len();
        }
        for m in &held {
            if m.bones.iter().any(|&b| b != hand) {
                fails.push(format!("{}: a vertex off the hand", m.name));
            }
            if m.positions.iter().flatten().any(|c| !c.is_finite()) || m.triangles.iter().flatten().any(|&i| i as usize >= m.positions.len()) {
                fails.push(format!("{}: a bad vertex or index", m.name));
            }
        }
        println!("{} base weapons, {shown} world meshes", ids.len());
        assert_eq!(shown, held.len(), "every world mesh belongs to its weapon's shown range");
        assert!(fails.is_empty(), "{} problems:\n{}", fails.len(), fails.join("\n"));
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
        // The overhead General's BO2 hits (a soldier, a vehicle, anything else) for the base kit's
        // bullet guns. BO2's default class (the Assault premade) carries the SMAW as its second
        // weapon: a launcher's slot has no bullet impacts.
        let hits = text(RTS_FX_PATH);
        let base = zhbo_core::weapons::Loadout::default();
        let mp = crate::paks::MpPool::load(Path::new(&bo2)).unwrap();
        let (weapons, _) = mp.weapons().unwrap();
        let bullet = |id: &str| weapons.iter().find(|w| w.name == crate::generals::base_id(id)).is_some_and(|w| w.weap_type == zhbo_formats::weapon::kind::BULLET);
        for (slot, id) in [("0 ", &base.primary), ("1 ", &base.secondary)] {
            if !bullet(id) {
                continue;
            }
            let line = hits.lines().find(|l| l.starts_with(slot)).unwrap_or_else(|| panic!("no impacts for slot {slot}: {hits}"));
            assert!(line.split(' ').skip(1).all(|fx| fx.starts_with("ZHBO_BO2FX_")), "{line}");
        }
        // R8d: every converted gun's bullet bucket has cells, and every cell names a converted
        // FXList. A row that points at nothing is a silent "no impact" in game, so it fails here.
        let table = zhbo_core::weapons::parse_full_table(&text(crate::weapons::TABLE_PATH)).unwrap();
        // The Create-a-Class items whose art the install lacks are listed locked with their DLC's
        // hint: the Peacekeeper's pictures are Revolution's (`c-primary/007`); every other item's
        // art converts. Its combinations carry the base's lock, as they carry its other item fields.
        let locked: Vec<(&str, &str)> = table.weapons.iter().filter(|d| d.cac_locked && d.attachments.is_empty()).map(|d| (d.id.as_str(), d.cac_lock_hint.as_str())).collect();
        assert_eq!(locked, [("peacekeeper_mp", "Revolution DLC Pack required.")]);
        assert!(table.weapons.iter().filter(|d| d.cac_locked).all(|d| zhbo_core::attachments::split_id(&d.id).0 == "peacekeeper_mp"));
        let impacts = text(zhbo_core::names::IMPACT_FX_PATH);
        let effects_ini = text("Data\\INI\\FXList\\ZHBO_BO2_Effects.ini");
        // 5b: every def-level sound the table names (raise, putaway, zoom, melee, fire start/last,
        // pull-back, blast) ships as an AudioEvent, or is one BO2 itself leaves silent: its alias is
        // `null_sound` or in no multiplayer bank (then the converter reports it missing).
        let audio = text("Data\\INI\\SoundEffects\\ZHBO_BO2.ini");
        let mut dangling = std::collections::BTreeSet::new();
        for d in &table.weapons {
            for (event, _) in d.def_sound_events() {
                let silent = zhbo_core::names::def_sound_alias(event) == Some(zhbo_core::sounds::NULL_SOUND_ALIAS)
                    || summary.contains(&format!("weapon sound {event}\""));
                if zhbo_core::names::def_sound_alias(event).is_some() && !silent && !audio.contains(&format!("AudioEvent {event}\n")) {
                    dangling.insert(event.to_string());
                }
            }
        }
        assert!(dangling.is_empty(), "def sounds with no converted event: {dangling:?}");
        let mut buckets = std::collections::BTreeSet::new();
        for d in table.weapons.iter().filter(|d| d.class.is_gun()) {
            assert!(d.impact_type > 0, "{} has no impact type", d.id);
            assert!(d.rounds_per_shot() as usize <= zhbo_core::client::CLIENT_HITS, "{} fires {} rounds a shot", d.id, d.rounds_per_shot());
            // Launchers use the explosion table and the crossbow/knife's 14 is BO2's own "missing
            // impact" row; only the bullet buckets get cells here.
            if zhbo_formats::fx::bullet_entry(d.impact_type as u32).is_some() {
                buckets.insert(d.impact_type);
            }
        }
        // Everything that goes off brings its impact type's row (FX) and family (sound).
        let blasts: std::collections::BTreeSet<i32> = crate::impacts::blast_types(&table.weapons).into_iter().map(|t| t as i32).collect();
        assert!(blasts.contains(&7) && blasts.contains(&9), "the frag's and the rocket's types: {blasts:?}");
        for line in impacts.lines().filter(|l| !l.starts_with(';') && !l.trim().is_empty()) {
            // An `exit`-prefixed row is a bullet's exit row or family (penetration's flag 4).
            let exit = line.starts_with("exit ");
            let w: Vec<&str> = line.split_whitespace().skip(exit as usize).collect();
            assert!(w.len() == 4, "bad impact row: {line}");
            assert!(!exit || zhbo_formats::fx::is_bullet(w[1].parse().unwrap()), "an exit row for a non-bullet: {line}");
            let t: i32 = w[1].parse().unwrap();
            let bounce_dud: Vec<i32> = crate::impacts::bounce_dud_types(&table.weapons).into_iter().map(|t| t as i32).collect();
            assert!(buckets.contains(&t) || blasts.contains(&t) || bounce_dud.contains(&t), "row for a type nothing carries: {line}");
            if w[0] == "sound" {
                assert!(w[3] == zhbo_core::sounds::NO_SOUND || audio.contains(&format!("AudioEvent {}\n", w[3])), "{} ships no AudioEvent", w[3]);
            } else {
                assert!(w[3].starts_with("ZHBO_BO2FX_"), "{line}");
                assert!(effects_ini.contains(&format!("FXList {}\n", w[3])), "{} names a missing FXList", w[3]);
            }
        }
        for t in buckets.iter().chain(&blasts) {
            assert!(impacts.contains(&format!("impact {t} default ")), "type {t} has no default cell");
        }
        // The grenade's blast sounds differ by surface (BO2's `wpn_grenade_explode_<surface>`).
        for s in ["default", "dirt", "concrete", "metal", "water"] {
            assert!(impacts.contains(&format!("sound 7 {s} ZHBO_BO2_Snd_wpn_grenade_explode_{s}\n")), "no grenade blast sound on {s}");
            // And a rifle's hit (BO2's `prj_bullet_impact_large_<surface>`, the map bank's).
            assert!(impacts.contains(&format!("sound 2 {s} ZHBO_BO2_Snd_prj_bullet_impact_large_{s}\n")), "no rifle hit sound on {s}");
        }
        // A frag's bounce plays BO2's per-surface `wpn_grenade_bounce_<surface>` and the
        // `grenade_bounce` row's dust (none on a soldier: BO2's cell is empty); the launcher
        // grenade can be a dud, with the `projectile_dud` row and its own effect and sound.
        let frag = table.weapons.iter().find(|d| d.id == "frag_grenade_mp").expect("frag");
        assert_eq!(frag.bounce_sound(zhbo_core::impacts::Surface::Concrete), "ZHBO_BO2_Snd_wpn_grenade_bounce_concrete");
        for (_, e) in &frag.bounce_sounds {
            assert!(e == zhbo_core::sounds::NO_SOUND || audio.contains(&format!("AudioEvent {e}\n")), "{e} ships no AudioEvent");
        }
        assert!(impacts.lines().any(|l| l.starts_with("impact 6 dirt ")), "no bounce dust on dirt");
        // Cell by cell: BO2's own default-surface cell is a row, its empty flesh cell is none.
        assert!(!impacts.lines().any(|l| l.starts_with("impact 6 flesh ")), "the bounce row has no flesh cell");
        assert!(impacts.lines().any(|l| l.starts_with("impact 11 flesh ")), "the dud row's flesh cell (footstep dust)");
        let gl = table.weapons.iter().find(|d| d.id == "gl_an94_mp").expect("gl_an94_mp");
        assert!(gl.activate_dist > 0.0 && gl.impact_explode, "the launcher grenade arms after a distance");
        assert_eq!(gl.dud_sound, "ZHBO_BO2_Snd_wpn_grenade_bounce_flesh");
        assert!(effects_ini.contains(&format!("FXList {}\n", gl.dud_fx)), "dud effect {:?} not converted", gl.dud_fx);
        assert!(impacts.lines().any(|l| l.starts_with("impact 11 concrete ")), "no dud cell on concrete");
        for bucket in &buckets {
            // BO2 leaves some nonfatal blood cells null, so require at least one blood cell per
            // bucket, not the nonfatal one.
            assert!(
                impacts.lines().any(|l| l.starts_with(&format!("flesh {bucket} "))),
                "bucket {bucket} has no blood cell"
            );
        }
        // Its guns flash as BO2's do, from their own muzzles.
        let weapons = text(crate::sfx::WEAPON_INI_PATH);
        let rifle = &weapons[weapons.find("Weapon ZHBO_GeneralRifle").unwrap()..];
        assert!(rifle[..rifle.find("\nEnd").unwrap()].contains("FireFX                  = ZHBO_BO2FX_") || rifle[..rifle.find("\nEnd").unwrap()].contains("FireFX = ZHBO_BO2FX_"), "{rifle}");
        let draw = text(INI_PATH);
        assert!(draw.contains("WeaponFireFXBone    = SECONDARY ZF01"));
        // Every animation set past the rifleman's runs its own clip under its condition, and its
        // W3D clips are in the archive.
        let table = zhbo_core::weapons::parse_full_table(&text(crate::weapons::TABLE_PATH)).unwrap();
        let sets = table.weapons.iter().map(|d| d.body_anims).max().unwrap_or(0) as usize;
        assert!(sets >= 5, "{sets} animation sets past the rifleman's (pistol, launcher, knife, shield, dual wield, ...)");
        for k in 1..=sets {
            let cond = zhbo_core::names::ANIM_SET_CONDITIONS[k - 1];
            assert!(draw.contains(&format!("ConditionState        = MOVING {cond}\n")), "set {k}: no {cond} run state");
            // A set's own W3D clips are those made differently from the rifleman's (the LMG set
            // differs only in its sprint, which the W3D body lacks, and its fire and reload).
            assert!(SET_CLIPS.iter().any(|c| files.iter().any(|(p, _)| p == &format!("Art\\W3D\\{}.w3d", set_anim(c, k)))), "set {k}: no W3D clip of its own");
        }
        let pistol = table.weapons.iter().find(|d| d.id == "fnp45_mp").unwrap().body_anims;
        assert!(pistol > 0 && files.iter().any(|(p, _)| p == &format!("Art\\W3D\\{}.w3d", set_anim("RUN", pistol as usize))), "the pistol runs its own run");
        // Every player weapon has the overhead General's Zero Hour weapon, and its name fits the
        // ABI's slot (`ZhboRtsWeapons::templates`).
        let rts = text(FX_WEAPONS_INI_PATH);
        let table = zhbo_core::weapons::parse_full_table(&text(crate::weapons::TABLE_PATH)).unwrap();
        for d in table.weapons.iter().filter(|d| !d.id.contains(['+', '~'])) {
            let name = zhbo_core::names::rts_weapon(&d.id);
            assert!(name.len() < zhbo_core::names::RTS_WEAPON_NAME_CAP, "{name} is too long for the ABI");
            assert!(rts.contains(&format!("Weapon {name}\n")), "no {name}");
        }
        // Shell ejection (notetrack-brass): only a def that defines one converts a shell FXList, and
        // that FXList is spawned at the posed `tag_brass` — oriented to the bone, on BO2's axes.
        let by_id = |id: &str| table.weapons.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id}"));
        assert!(!by_id("an94_mp").view_shell_fx.is_empty(), "the AN-94's patch_mp fx_rifle must convert");
        assert_eq!(by_id("an94_mp+grip").view_shell_fx, by_id("an94_mp").view_shell_fx, "a combination ejects its base's brass");
        assert!(!by_id("mp7_mp").view_shell_fx.is_empty(), "the MP7's shell effect must convert");
        let fxs = text("Data\\INI\\FXList\\ZHBO_BO2_Effects.ini");
        let at = fxs.find("FXList ZHBO_BO2FX_weapon_shellejects_fx_pistol\n").expect("the pistol shell FXList");
        let rest = &fxs[at..];
        let pistol = &rest[..rest.find("\nEnd\n").unwrap()];
        // fx_pistol's model and material are split across the patch and base zones, so this is the
        // one shell FXList that proves the cross-zone resolution: a stub-only lookup leaves it empty.
        assert!(pistol.contains("  ParticleSystem"), "the pistol shell FXList must convert a system: {pistol}");
        assert!(pistol.contains("OrientToObject = Yes"), "{pistol}");
        // The axes are on the particle systems the FXList names (a nugget carries only the name and
        // orientation), so read their definitions from the same run's ParticleSystem file.
        let names: Vec<String> = pistol.lines().filter_map(|l| l.trim().strip_prefix("Name = ")).map(str::to_string).collect();
        assert!(!names.is_empty(), "the pistol shell FXList names no system: {pistol}");
        let particles = text("Data\\INI\\ParticleSystem\\ZHBO_BO2_Effects.ini");
        // Every blast, stun, EMP and smoke effect a weapon names draws something: an FXList left
        // empty (an effect read from a zone whose materials are reference stubs, as the smoke
        // screen's pooled `smoke_center_mp` copy is) is a silent nothing in game.
        let fx_body = |name: &str| fxs.find(&format!("FXList {name}\n")).map(|at| fxs[at..][..fxs[at..].find("\nEnd\n").unwrap_or(0)].to_string());
        let mut empty = std::collections::BTreeSet::new();
        for d in &table.weapons {
            if let Some(body) = d.explosion_fx.starts_with("ZHBO_BO2FX_").then(|| fx_body(&d.explosion_fx)).flatten() {
                if !body.contains("\n  ") {
                    empty.insert(format!("{} ({})", d.explosion_fx, d.id));
                }
            }
        }
        assert!(empty.is_empty(), "converted blast effects that draw nothing: {empty:?}");
        // The smoke grenade's screen: BO2's `smoke_center_mp` effect, as an area effect (drawn at
        // every dynamic LOD and never evicted by a fight's particles) whose looping parts stand the
        // tuning pack's screen time.
        let smoke = by_id("willy_pete_mp");
        let body = fx_body(&smoke.explosion_fx).unwrap_or_else(|| panic!("no FXList {}", smoke.explosion_fx));
        let systems: Vec<&str> = body.lines().filter_map(|l| l.trim().strip_prefix("Name = ")).collect();
        assert!(systems.len() >= 10, "the cloud's systems: {body}");
        let frames = (zhbo_core::tuning::get().smoke_duration * 30.0).round();
        let mut looping = 0;
        for name in &systems {
            let at = particles.find(&format!("ParticleSystem {name}\n")).unwrap_or_else(|| panic!("no ParticleSystem {name}"));
            let def = &particles[at..][..particles[at..].find("\nEnd\n").unwrap()];
            assert!(def.contains("Priority = AREA_EFFECT"), "{def}");
            if !def.contains("BurstDelay = 0 0") {
                looping += 1;
                assert!(def.contains(&format!("SystemLifetime = {frames:.0}\n")), "{def}");
            }
        }
        assert!(looping >= 8, "the cloud's looping parts: {looping}");
        // Every converted effect takes its role's priority (`fx::Role`), across the whole catalog:
        // what the table and the impact table name, by what plays it. Nothing a blast, a hit or a
        // flash draws is Zero Hour's lowest priority (culled below 25 fps, unable to make room at
        // the particle cap); casings are.
        let priorities = |list: &str| -> Vec<String> {
            let Some(body) = fx_body(list) else { return Vec::new() };
            body.lines()
                .filter_map(|l| l.trim().strip_prefix("Name = "))
                .map(|name| {
                    let at = particles.find(&format!("ParticleSystem {name}\n")).unwrap_or_else(|| panic!("no ParticleSystem {name}"));
                    let def = &particles[at..][..particles[at..].find("\nEnd\n").unwrap()];
                    def.lines().find_map(|l| l.trim().strip_prefix("Priority = ")).unwrap_or("").to_string()
                })
                .collect()
        };
        // A list played in two roles takes the higher priority (`fx_footstep_dust` is a bounce's
        // kick and a dud's flesh cell), so gather every use first.
        let rank = |p: &str| ["WEAPON_EXPLOSION", "AREA_EFFECT", "CRITICAL"].iter().position(|x| *x == p).unwrap_or(0);
        let mut wanted: BTreeMap<String, (&str, String)> = BTreeMap::new();
        let mut check = |list: &str, want: &'static str, what: &str| {
            if list.is_empty() || list == "NONE" {
                return;
            }
            let e = wanted.entry(list.to_string()).or_insert((want, what.to_string()));
            if rank(want) > rank(e.0) {
                *e = (want, what.to_string());
            }
        };
        for d in &table.weapons {
            if d.effect != zhbo_core::weapons::Effect::Smoke {
                check(&d.explosion_fx, "CRITICAL", "blast");
            }
            check(&d.dud_fx, "CRITICAL", "dud");
            check(&d.view_flash_fx, "CRITICAL", "flash");
            check(&d.silenced_view_flash_fx, "CRITICAL", "flash");
            check(&d.view_shell_fx, "WEAPON_EXPLOSION", "shell");
        }
        let impacts = text(zhbo_core::names::IMPACT_FX_PATH);
        for l in impacts.lines().filter(|l| !l.starts_with(';')) {
            let f: Vec<&str> = l.split_whitespace().collect();
            let (kind, t, list) = match f.as_slice() {
                ["exit", k, t, _, list] => (*k, *t, *list),
                [k, t, _, list] if *k != "sound" => (*k, *t, *list),
                _ => continue,
            };
            let t: i32 = t.parse().unwrap_or(-1);
            let want = if t == zhbo_core::impacts::GRENADE_BOUNCE { "WEAPON_EXPLOSION" } else { "CRITICAL" };
            check(list, want, kind);
        }
        let mut wrong = Vec::new();
        for (list, (want, what)) in &wanted {
            for p in priorities(list) {
                if p != *want {
                    wrong.push(format!("{what} {list}: {p}, not {want}"));
                }
            }
        }
        assert!(wanted.len() > 100, "{} converted lists checked", wanted.len());
        assert!(wrong.is_empty(), "{} systems off their role's priority: {:?}", wrong.len(), &wrong[..wrong.len().min(20)]);
        let defs: Vec<&str> = names
            .iter()
            .map(|name| {
                let at = particles.find(&format!("ParticleSystem {name}\n")).unwrap_or_else(|| panic!("no ParticleSystem {name}"));
                let rest = &particles[at..];
                &rest[..rest.find("\nEnd\n").unwrap()]
            })
            .collect();
        // The pistols' fx_pistol is the asset pool's: `patch_mp`'s, over `common_mp`'s own copy
        // (`zhbo_formats::fx::bind`). Its velocities are mostly +X in the tag's frame: identity axes
        // put BO2's Z in VelOrthoZ — only for elements whose velocity graph moves along Z at all
        // (its first sample, or the integrated delta a speeding-up graph converts to) — where the
        // old up-is-X swap put BO2's X.
        let live = zhbo_formats::fx::find(&load_zone(Path::new(&bo2), crate::fx::ZONE).unwrap(), "weapon/shellejects/fx_pistol").unwrap();
        let z_of = |v: &zhbo_formats::fx::VelSample| v.local_velocity.0[2] != 0.0 || v.local_velocity.1[2] != 0.0 || v.local_delta.0[2] != 0.0 || v.local_delta.1[2] != 0.0;
        let moving_z = live.elems.iter().filter(|e| e.kind == zhbo_formats::fx::kind::MODEL && e.vel.iter().any(z_of)).count();
        let vel = |a: &str| defs.iter().flat_map(|d| d.lines().filter(|l| l.trim_start().starts_with(a))).map(str::trim).collect::<Vec<_>>();
        let converted_z = vel("VelOrthoZ =").iter().filter(|l| !l.ends_with("0.000 0.000")).count();
        assert!(converted_z <= moving_z, "{converted_z} systems move along Z, BO2 moves {moving_z}: {defs:?}");
        // A shell is drawn the size BO2's model element gives no size for (never 0: invisible).
        assert!(defs.iter().all(|d| !d.contains("Size = 0.00 0.00")), "{defs:?}");
        assert!(vel("VelOrthoX =").iter().any(|l| !l.ends_with("0.000 0.000")), "{defs:?}");
        // 5b family C: what others see. The AN-94's world brass (`fx_rifle`, a stub bound from
        // patch_mp) plays from its own FX weapon, placed from the flash bone to `tag_brass`; its
        // combinations eject the same; its tracer is BO2's `assaultrifle`, every 3rd bullet. A
        // scoped LMG combination takes its optic unique's `lmg_scope` (FUN_008eb420).
        let block = |ini: &str, head: &str| {
            let at = ini.find(head).unwrap_or_else(|| panic!("no {head}"));
            let rest = &ini[at..];
            rest[..rest.find("\nEnd\n").unwrap()].to_string()
        };
        let weapons_ini = text("Data\\INI\\Weapon\\ZHBO_BO2_Weapons.ini");
        let an94 = by_id("an94_mp");
        assert_eq!(an94.world_shell_fx_weapon, crate::shotfx::brass_fx_weapon("an94_mp"));
        assert_eq!(an94.world_last_shot_shell_fx_weapon, an94.world_shell_fx_weapon, "BO2's last-round effect is the same fx_rifle");
        assert_eq!(by_id("an94_mp+grip").world_shell_fx_weapon, an94.world_shell_fx_weapon);
        assert_eq!(an94.tracer_fx_weapon, crate::shotfx::tracer_fx_weapon("assaultrifle"));
        assert_eq!(an94.tracer_interval, 3);
        let mk48_ir = table.weapons.iter().find(|d| d.id.starts_with("mk48_mp+") && d.id.contains("+ir")).expect("an mk48 ir combination");
        assert_eq!(mk48_ir.tracer_fx_weapon, crate::shotfx::tracer_fx_weapon("lmg_scope"), "{}", mk48_ir.id);
        // An enemy viewer's: `assaultrifle_enemy` (gfx_tracer_enemy's hue); the IR unique carries
        // no enemy tracer, so its own `lmg_scope` comes first in the enemy walk too.
        assert_eq!(an94.enemy_tracer_fx_weapon, crate::shotfx::tracer_fx_weapon("assaultrifle_enemy"));
        assert_eq!(mk48_ir.enemy_tracer_fx_weapon, mk48_ir.tracer_fx_weapon, "{}", mk48_ir.id);
        let colour = |fx: &str| {
            let list = block(&weapons_ini, &format!("Weapon {fx}\n"));
            let fire = list.lines().find_map(|l| l.trim().strip_prefix("FireFX")).map(|v| v.trim_start_matches([' ', '=']).to_string()).unwrap();
            block(&fxs, &format!("FXList {fire}\n")).lines().find(|l| l.trim().starts_with("Color")).map(str::to_string).unwrap()
        };
        assert_ne!(colour(&an94.tracer_fx_weapon), colour(&an94.enemy_tracer_fx_weapon), "the enemy material tints the beam");
        assert!(by_id("judge_mp").world_shell_fx_weapon.is_empty(), "the Executioner ejects nothing in BO2");
        let brass = block(&weapons_ini, &format!("Weapon {}\n", an94.world_shell_fx_weapon));
        assert!(brass.contains(&format!("FireFX                  = {}", crate::shotfx::brass_fxlist("an94_mp", false))), "{brass}");
        let placed = block(&fxs, &format!("FXList {}\n", crate::shotfx::brass_fxlist("an94_mp", false)));
        assert!(placed.contains("OrientToObject = Yes") && placed.contains("Offset = X:") && placed.contains("RotateZ = "), "{placed}");
        let tracer = block(&fxs, &format!("FXList {}\n", crate::fx::tracer_fxlist("assaultrifle", 1.0)));
        assert!(tracer.contains("  Tracer\n") && tracer.contains("Probability = 1.00"), "{tracer}");
        // The overhead General: flash, brass and a 1-in-3 tracer in one FireFX.
        let rts_fx = block(&fxs, "FXList ZHBO_BO2FX_Rts_an94_mp\n");
        assert!(rts_fx.contains("RotateZ = ") && rts_fx.contains("Probability = 0.33"), "{rts_fx}");
        native_blobs_are_whole(&files);
        wrist_datapads_are_off(&files);
    }

    /// Every native viewmodel (Track R) reads back, names models, clips and textures that are in
    /// the archive, composes without a missing tag, and carries its aim marker.
    fn native_blobs_are_whole(files: &[(String, Vec<u8>)]) {
        use zhbo_native::{anim::Anim, model::Model, pose::{PartRef, Skeleton}, viewmodel::Viewmodel};
        let file = |path: &str| files.iter().find(|(p, _)| p == path).map(|(_, b)| b.as_slice());
        let manifests: Vec<&(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.ends_with(".zbv")).collect();
        let native: Vec<&(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.starts_with(zhbo_native::DIR)).collect();
        // The converted table says which ids are offhands (their BO2 offhand slot).
        let table = file(crate::weapons::TABLE_PATH).map(|b| zhbo_core::weapons::parse_full_table(&String::from_utf8_lossy(b)).expect("the weapon table parses")).expect("the weapon table");
        println!("native blobs: {} files, {:.1} MB", native.len(), native.iter().map(|(_, b)| b.len()).sum::<usize>() as f64 / 1e6);
        assert!(manifests.len() >= FACTIONS.len(), "{} native viewmodels", manifests.len());
        for (path, bytes) in manifests {
            let vm = Viewmodel::from_bytes(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            // The gun part is the first part that hangs from a tag (the arms base is untagged) and
            // its model is the weapon's view model. Off-hand equipment rides the left palm's
            // `tag_knife_attach`; the gun control stays on the primary gun's `tag_weapon`.
            let gun_part = vm.parts.iter().find(|p| p.tag.is_some()).unwrap_or_else(|| panic!("{path}: no gun part"));
            if table.weapons.iter().find(|d| d.id == vm.id).is_some_and(|d| d.class.is_offhand()) {
                assert_eq!(gun_part.tag.as_deref(), Some("tag_knife_attach"), "{path} ({}): the equipment should hang from the left palm", vm.id);
            } else if vm.id == "an94_mp" {
                assert_eq!(gun_part.tag.as_deref(), Some("tag_weapon"), "{path}: the gun control stays on the gun anchor");
            }
            let models: Vec<Model> = vm.parts.iter().map(|p| Model::from_bytes(file(&p.model).unwrap_or_else(|| panic!("{path}: no {}", p.model))).unwrap_or_else(|e| panic!("{}: {e}", p.model))).collect();
            for m in &models {
                no_empty_surfaces(m);
                for mat in &m.materials {
                    for t in mat.color.iter().chain(&mat.maps) {
                        assert!(file(t).is_some(), "{}: texture {t} not written", m.name);
                    }
                }
            }
            let parts: Vec<PartRef> = vm.parts.iter().zip(&models).map(|(p, m)| PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: p.rigid }).collect();
            let (skel, problems) = Skeleton::compose(&parts, &vm.markers, &vm.hide);
            assert!(problems.is_empty(), "{path}: {problems:?}");
            assert!(skel.bone(crate::native::AIM_TAG).is_some(), "{path}: no aim marker");
            for (_, clip) in &vm.clips {
                Anim::from_bytes(file(clip).unwrap_or_else(|| panic!("{path}: no {clip}"))).unwrap_or_else(|e| panic!("{clip}: {e}"));
            }
        }
        native_bodies_are_whole(files);
    }

    /// The converter never writes a surface with no indices: BO2 has none in the converted zones
    /// (the catalogue scan over `common_mp`, `patch_mp`, the faction zones, `common_patch_mp`,
    /// `common` and `code_post_gfx`), the W3D path skips an empty source surface, and an empty blob
    /// surface can never draw. The composer skips a surface its hide tags empty; the blob is never
    /// allowed to hold one in the first place.
    fn no_empty_surfaces(m: &zhbo_native::model::Model) {
        for (i, s) in m.surfaces.iter().enumerate() {
            assert!(!s.indices.is_empty(), "{}: surface {i} has no indices (the converter must not write an empty surface)", m.name);
        }
    }

    /// The first-person hands' wrist datapad is BO2's campaign map/readout screen. Multiplayer's
    /// watch face is not that screen, and drawn statically its yellow readouts look like caution
    /// stripes and its radar like a magenta dot; the converted hands must use the neutral off
    /// screen, and the campaign image must never reach the archive.
    fn wrist_datapads_are_off(files: &[(String, Vec<u8>)]) {
        use zhbo_native::model::Model;
        let file = |path: &str| files.iter().find(|(p, _)| p == path).map(|(_, b)| b.as_slice());
        let off = format!("Art\\Textures\\{}", crate::textures::DATAPAD_OFF);
        let campaign = "Art\\Textures\\zhbo_viewarm_datapad_c.tga";
        assert!(!files.iter().any(|(p, _)| p == campaign), "the campaign datapad screen was written");
        assert!(files.iter().any(|(p, _)| p == &off), "the neutral datapad screen was not written");
        for f in FACTIONS {
            let path = zhbo_native::model_path(f.hands);
            let m = Model::from_bytes(file(&path).unwrap_or_else(|| panic!("no {path}"))).unwrap_or_else(|e| panic!("{path}: {e}"));
            let mut found = 0;
            for mat in &m.materials {
                if mat.name.contains("datapad") {
                    found += 1;
                    assert_eq!(mat.color.as_deref(), Some(off.as_str()), "{}: the campaign datapad screen is still assigned", f.hands);
                }
                assert!(!mat.color.as_deref().is_some_and(|t| t.contains("viewarm_datapad_c")), "{}: the campaign datapad screen leaked", f.hands);
            }
            assert_eq!(found, 1, "{}: expected its wrist datapad", f.hands);
        }
    }

    /// Track R7: every faction body reads back, is a 72-inch player with its weapon tag and every
    /// movement state's clip, and composes with a world weapon and its world attachments without a
    /// missing tag; its textures and the weapon's are in the archive.
    fn native_bodies_are_whole(files: &[(String, Vec<u8>)]) {
        use zhbo_native::thirdperson::{state, Body, Weapon};
        use zhbo_native::{anim::Anim, model::Model, pose::{PartRef, Skeleton}};
        let file = |path: &str| files.iter().find(|(p, _)| p == path).map(|(_, b)| b.as_slice());
        let model = |path: &str| {
            let m = Model::from_bytes(file(path).unwrap_or_else(|| panic!("no model {path}"))).unwrap_or_else(|e| panic!("{path}: {e}"));
            no_empty_surfaces(&m);
            m
        };
        let textures_are_in = |m: &Model, what: &str| {
            for mat in &m.materials {
                for t in mat.color.iter().chain(&mat.maps) {
                    assert!(file(t).is_some(), "{what}: texture {t} not written");
                }
            }
        };
        let bodies: Vec<&(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.ends_with(".zbc")).collect();
        assert_eq!(bodies.len(), FACTIONS.len(), "one body manifest per faction");
        let weapons: Vec<&(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.ends_with(".zbw")).collect();
        assert!(!weapons.is_empty(), "no world weapon manifests");
        println!("native third person: {} bodies, {} world weapons", bodies.len(), weapons.len());
        for (path, bytes) in &bodies {
            let body = Body::from_bytes(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            let bm = model(&body.model);
            textures_are_in(&bm, &body.model);
            // A BO2 MP player body is a 72-inch soldier in its own (inch) space.
            let height = bm.maxs[2] - bm.mins[2];
            assert!((66.0..=78.0).contains(&height), "{}: {height} inches tall", body.model);
            assert!(bm.bone(&body.weapon_tag).is_some(), "{path}: no {} bone", body.weapon_tag);
            // Every movement state resolves to a clip in the archive, and reads back.
            for st in 1..=state::COUNT {
                let clip = body.clip(st).unwrap_or_else(|| panic!("{path}: no clip for state {st}"));
                Anim::from_bytes(file(clip).unwrap_or_else(|| panic!("{path}: no {clip}"))).unwrap_or_else(|e| panic!("{clip}: {e}"));
            }
            // Composes with every converted world weapon on its weapon tag, no missing tags.
            for (wpath, wbytes) in &weapons {
                let w = Weapon::from_bytes(wbytes).unwrap_or_else(|e| panic!("{wpath}: {e}"));
                let models: Vec<Model> = w.parts.iter().map(|p| model(&p.model)).collect();
                for m in &models {
                    textures_are_in(m, wpath);
                }
                let mut parts: Vec<PartRef> = Vec::with_capacity(models.len() + 1);
                parts.push(PartRef { model: &bm, tag: None, offset: crate::xf::IDENTITY, rigid: false });
                // The world model hangs on its own body bone when it names one (the Assault Shield's
                // `tag_weapon_left`, a stowed manifest's `tag_stowed_back`), else on the body's
                // weapon tag; its attachments on its bones.
                let base_tag = w.parts[0].tag.as_deref().or(Some(body.weapon_tag.as_str()));
                parts.push(PartRef { model: &models[0], tag: base_tag, offset: w.parts[0].offset, rigid: true });
                for (p, m) in w.parts.iter().zip(&models).skip(1) {
                    parts.push(PartRef { model: m, tag: p.tag.as_deref(), offset: p.offset, rigid: true });
                }
                let (_, problems) = Skeleton::compose(&parts, &[], &w.hide);
                assert!(problems.is_empty(), "{path} + {wpath}: {problems:?}");
            }
        }
        // A world weapon composes on its own too (the weapon-only mode, while the W3D body draws).
        for (wpath, wbytes) in &weapons {
            let w = Weapon::from_bytes(wbytes).unwrap_or_else(|e| panic!("{wpath}: {e}"));
            let models: Vec<Model> = w.parts.iter().map(|p| model(&p.model)).collect();
            // Without a body the base part hangs from the model's origin (`nativebody::load`).
            let parts: Vec<PartRef> = w.parts.iter().zip(&models).enumerate().map(|(i, (p, m))| PartRef { model: m, tag: if i == 0 { None } else { p.tag.as_deref() }, offset: p.offset, rigid: true }).collect();
            let (_, problems) = Skeleton::compose(&parts, &[], &w.hide);
            assert!(problems.is_empty(), "{wpath}: {problems:?}");
        }
    }
}
