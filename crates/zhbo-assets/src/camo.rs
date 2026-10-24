//! BO2's weapon camo art: the solid/pattern layer images and the style materials, converted into
//! the archive beside the models. Pixels come from `common_mp` (the patch zone's copies are
//! name-only stubs); the style materials come from the patch's registered assets or are inline in
//! the camo asset itself. The recipe phase (C3) writes the blobs; this phase only writes textures.

use crate::character;
use crate::generals::MAX_TEXTURE;
use crate::images::tga_bytes;
use crate::paks::{shrink, Paks};
use crate::textures::{maps_file, put_occlusion, surface_maps};
use std::collections::{BTreeMap, BTreeSet};
use zhbo_formats::iwi::Rgba;
use zhbo_formats::t6zone::Zone;
use zhbo_formats::{weapon, xmodel};
use zhbo_native::model as nm;

pub(crate) struct Art {
    /// Archive path by BO2 image name.
    pub images: BTreeMap<String, String>,
    /// Converted style materials by BO2 material name.
    pub materials: BTreeMap<String, nm::Material>,
}

/// Convert every image and style material the camo assets name, once each. Missing art is skipped
/// and reported by the recipe phase; it is never fatal.
pub(crate) fn art(
    common: &Zone,
    patch: &Zone,
    inline: &Zone,
    paks: &Paks,
    sets: &[weapon::WeaponCamo],
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Art {
    let pixel = |img: &xmodel::Image| -> Option<Rgba> {
        paks.image(img.stream_hash).or_else(|| {
            xmodel::find_image(common, &img.name).and_then(|i| paks.image(i.stream_hash))
        })
    };
    let mut names: BTreeSet<String> = BTreeSet::new();
    for c in sets {
        for s in &c.sets {
            for i in [&s.solid_image, &s.pattern_image].into_iter().flatten() {
                names.insert(i.name.clone());
            }
        }
    }
    let mut images = BTreeMap::new();
    for name in names {
        let Some(img) = xmodel::find_image(common, &name) else { continue };
        let Some(rgba) = paks.image(img.stream_hash) else { continue };
        let rgba = shrink(rgba, MAX_TEXTURE);
        let path = format!("Art\\Textures\\{}", character::texture_file(&name));
        out.entry(path.clone()).or_insert_with(|| tga_bytes(&rgba));
        images.insert(name, path);
    }
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for c in sets {
        for set in &c.materials {
            for e in &set.entries {
                wanted.extend(e.camo.iter().cloned());
            }
        }
    }
    let mut materials = BTreeMap::new();
    for name in wanted {
        let bare = name.trim_start_matches("mc/");
        let found = xmodel::find_asset_material(patch, &name)
            .or_else(|| xmodel::find_asset_material(patch, bare))
            .or_else(|| xmodel::find_inline_material(inline, &name))
            .or_else(|| xmodel::find_inline_material(inline, bare))
            .or_else(|| xmodel::find_inline_material(common, &name));
        let Some(mat) = found else { continue };
        let Some(color) = mat.color_map() else { continue };
        let Some(rgba) = pixel(color) else { continue };
        let file = character::texture_file(&color.name);
        let path = format!("Art\\Textures\\{file}");
        if !out.contains_key(&path) {
            let mut rgba = shrink(rgba, MAX_TEXTURE);
            put_occlusion(&mut rgba, mat.image(xmodel::TS_OCCLUSION_MAP).and_then(pixel).as_ref());
            let maps = surface_maps(
                rgba.width,
                rgba.height,
                mat.image(xmodel::TS_NORMAL_MAP).and_then(pixel).as_ref(),
                mat.image(xmodel::TS_SPECULAR_MAP).and_then(pixel).as_ref(),
            );
            out.insert(format!("Art\\Textures\\{}", maps_file(&file)), tga_bytes(&maps));
            out.insert(path.clone(), tga_bytes(&rgba));
        }
        let blend = crate::native::blend(mat.blend());
        let opaque = mat.blend() == zhbo_formats::w3d::Blend::Opaque;
        materials.insert(
            name,
            nm::Material {
                name: mat.name.clone(),
                blend,
                emissive: matches!(blend, nm::Blend::Additive { .. }),
                color: Some(path),
                maps: opaque.then(|| format!("Art\\Textures\\{}", maps_file(&file))),
                sort_key: mat.sort_key,
                cull: crate::native::cull(mat.cull()),
                camo: nm::CamoLayer::None,
            },
        );
    }
    Art { images, materials }
}

/// Camo assets that belong to attachments (the optic's own camo), not to a weapon's recipe: the
/// class screen's camo applies to the weapon, and optic-camo handling is a follow-up (PLAN D5).
const ATTACHMENT_CAMO: &[&str] = &[
    "camo_reflex", "camo_holo", "camo_ir", "camo_acog", "camo_hybrid", "camo_mms", "camo_targetfinder",
];

/// Normalize a name for the view-model join: lowercase, split on non-alphanumerics.
fn tokens(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// The weapon whose view model carries `stem`: BO2 names camo assets after the view model
/// (`camo_xpr50` belongs to `as50_mp`, whose view model is `t6_wpn_sniper_xpr50_view`). A token
/// subset (or the joined form) so `camo_ballistic_knife` matches `..._knife_ballistic_view`.
fn weapon_by_view_model<'a>(bo2w: &'a [weapon::Weapon], stem: &str) -> Option<&'a str> {
    let want = tokens(stem);
    let joined: String = want.concat();
    let mut best: Option<(usize, &str)> = None;
    for w in bo2w {
        let Some(vm) = w.view_model.as_deref() else { continue };
        let have = tokens(vm);
        let mut score = want.iter().filter(|t| have.iter().any(|h| h == *t)).count();
        if !want.is_empty() && have.concat().contains(&joined) {
            score += want.len();
        }
        if score > 0 && best.is_none_or(|(s, _)| score > s) {
            best = Some((score, w.name.as_str()));
        }
    }
    best.map(|(_, id)| id)
}

/// BO2's camo option id → (pattern image stem, style family). `None` = the 48 assets carry no art
/// for it: the option stays selectable and draws the stock finish (research §5.1/§5.4).
const OPTION_ART: &[(&str, Option<&str>, Option<&str>)] = &[
    ("camo_none", None, None),
    ("camo_multicam", None, None),
    ("camo_matuka", None, None),
    ("camo_mtp", None, None),
    ("camo_snake", None, None),
    ("camo_atacs", Some("t6_camo_atacs"), None),
    ("camo_erdl", Some("t6_camo_erdl"), None),
    ("camo_siberia", Some("t6_camo_siberia"), None),
    ("camo_choco", Some("t6_camo_choco"), None),
    ("camo_tiger_blue", Some("t6_camo_tiger_blue"), None),
    ("camo_ghostex", Some("t6_camo_ghostex_delta6"), None),
    ("camo_kryptek", Some("t6_camo_kryptek_typhon"), None),
    ("camo_blossom", Some("t6_camo_blossom"), None),
    ("camo_artofwar", Some("t6_camo_artofwar"), None),
    ("camo_ronin", Some("t6_camo_ronin"), None),
    ("camo_skulls", Some("t6_camo_skulls"), None),
    ("camo_collectors", Some("t6_camo_bo2collectors"), None),
    ("camo_elite1", Some("t6_camo_elite"), None),
    ("camo_carbon", None, Some("carbon_fiber")),
    ("camo_gold", None, Some("gold")),
    ("camo_diamond", None, Some("diamond")),
    // DLC/MTX rows the class screen does not offer yet; mapped now so the blob is future-proof.
    ("camo_ukpunk", Some("t6_camo_ukpunk"), None),
    ("camo_benjamins", Some("t6_camo_benjamins"), None),
    ("camo_dayofdead", Some("t6_camo_dayofdead"), None),
    ("camo_graffiti", Some("t6_camo_graffiti"), None),
    ("camo_kawaii", Some("t6_camo_kawaii"), None),
    ("camo_partyrock", Some("t6_camo_partyrock"), None),
    ("camo_zombies", Some("t6_camo_zombies"), Some("zombies")),
    ("camo_bacon", None, Some("bacon")),
    ("camo_ghost", None, Some("ghost")),
    ("camo_paladin", None, Some("armor")),
    ("camo_cybertron", None, Some("cybertron")),
    ("camo_dragon", None, Some("dragon")),
    ("camo_toxic", None, Some("toxic")),
    ("camo_mystic", None, Some("mystic")),
    ("camo_comics", Some("t6_camo_comic"), None),
    ("camo_aqua", Some("camo_mtx3_aqua"), None),
    ("camo_breach", Some("camo_mtx3_breach"), None),
    ("camo_dcoyote", Some("camo_mtx3_coyote"), None),
    ("camo_zulu", Some("camo_mtx3_zulu"), None),
    ("camo_massacre", Some("camo_mtx3_massacre"), None),
    ("camo_octane", Some("camo_mtx4_octane"), None),
    ("camo_jungletiger", Some("camo_mtx4_tiger"), None),
    ("camo_wanted", Some("camo_mtx4_wanted"), None),
    ("camo_roxann", Some("t6_camo_roxann"), None),
];

/// The 15 style families in the assets' fixed order (research §1.3).
const STYLE_FAMILIES: [&str; 15] = [
    "gold", "carbon_fiber", "diamond", "zombies", "ghost", "bacon", "armor", "muertos", "zmb_dlc2",
    "cybertron", "etching", "dragon", "3layer", "toxic", "mystic",
];

/// The layer's tint strength (C0's probe; research §5.3).
const CAMO_GAIN: f32 = 2.0;

/// The set whose solid or pattern image starts with `stem` (`t6_camo_atacs`).
fn pattern_slot(c: &weapon::WeaponCamo, stem: &str) -> Option<u32> {
    fn image_name(i: &Option<xmodel::Image>) -> &str {
        i.as_ref().map_or("", |i| i.name.as_str())
    }
    c.sets
        .iter()
        .position(|s| image_name(&s.pattern_image).starts_with(stem) || image_name(&s.solid_image).starts_with(stem))
        .map(|i| i as u32)
}

/// The style slot whose entries name `camo_<family>`, and which sits at the fixed order's index.
fn style_slot(c: &weapon::WeaponCamo, family: &str) -> Option<u32> {
    let i = STYLE_FAMILIES.iter().position(|f| *f == family)? as u32;
    let needle = format!("camo_{family}");
    c.materials.get(i as usize).filter(|m| {
        m.entries.iter().any(|e| e.camo.iter().any(|n| n.contains(&needle)))
    })?;
    Some(i)
}

pub struct Summary {
    pub assets: usize,
    pub images: usize,
    pub styles: usize,
    pub unresolved: Vec<String>,
    /// Attachment camo assets skipped by design (they are not per-weapon recipes).
    pub excluded: Vec<String>,
}

/// Build every camo blob and write it beside the art. `table` supplies each asset's option ids
/// (`WeaponDef.camo`, already the base-tab list).
#[allow(clippy::too_many_arguments)]
pub(crate) fn files(
    common: &Zone,
    patch: &Zone,
    inline: &Zone,
    paks: &Paks,
    sets: &[weapon::WeaponCamo],
    bo2w: &[weapon::Weapon],
    table: &zhbo_core::weapons::Table,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<Summary, String> {
    let art = art(common, patch, inline, paks, sets, out);
    let mut summary = Summary { assets: 0, images: art.images.len(), styles: art.materials.len(), unresolved: Vec::new(), excluded: Vec::new() };
    for c in sets {
        // Optic camo assets are attachment recipes, not per-weapon ones (PLAN D5's follow-up).
        if ATTACHMENT_CAMO.contains(&c.name.as_str()) {
            summary.excluded.push(c.name.clone());
            continue;
        }
        let stem = c.name.strip_prefix("camo_").unwrap_or(&c.name);
        let by_view = weapon_by_view_model(bo2w, stem);
        let matched = table.weapons.iter().find(|w| w.camo_material == c.name)
            .or_else(|| by_view.and_then(|id| table.weapons.iter().find(|w| w.id == id)));
        // A view-model-resolved weapon that the player table filters (the riot shield) still gets a
        // blob with the base camo rows, so every weapon camo asset has a recipe.
        let camo_ids: Vec<String> = match matched {
            Some(w) => w.camo.clone(),
            None if by_view.is_some() => table.camo.iter().take(crate::weapons::BASE_CAMO_OPTIONS).map(|c| c.id.clone()).collect(),
            None => continue,
        };
        let mut blob = zhbo_native::camo::Camo {
            name: c.name.clone(),
            solid_base: c.solid_base_image.as_ref().and_then(|i| art.images.get(&i.name).cloned()),
            pattern_base: c.pattern_base_image.as_ref().and_then(|i| art.images.get(&i.name).cloned()),
            sets: c.sets.iter().map(|s| zhbo_native::camo::Set {
                solid: s.solid_image.as_ref().and_then(|i| art.images.get(&i.name).cloned()),
                pattern: s.pattern_image.as_ref().and_then(|i| art.images.get(&i.name).cloned()),
                offset: s.offset,
                scale: s.scale,
                gain: CAMO_GAIN,
            }).collect(),
            styles: c.materials.iter().map(|set| zhbo_native::camo::Style {
                entries: set.entries.iter().map(|e| {
                    let n = e.base.len().min(e.camo.len());
                    if n != e.base.len() || n != e.camo.len() {
                        summary.unresolved.push(format!("{}: {} base vs {} camo", c.name, e.base.len(), e.camo.len()));
                    }
                    zhbo_native::camo::Entry {
                        flags: e.replace_flags,
                        pairs: (0..n).map(|k| zhbo_native::camo::Pair { base: e.base[k].clone(), camo: e.camo[k].clone() }).collect(),
                        consts: e.shader_consts,
                    }
                }).collect(),
            }).collect(),
            materials: c.materials.iter().flat_map(|s| s.entries.iter().flat_map(|e| e.camo.iter()))
                .filter_map(|n| art.materials.get(n).cloned()).collect(),
            options: Vec::new(),
        };
        for id in &camo_ids {
            let (stem, family) = OPTION_ART.iter().find(|(o, _, _)| o == id).map(|(_, s, f)| (*s, *f)).unwrap_or((None, None));
            let pattern = stem.and_then(|s| pattern_slot(c, s));
            let style = family.and_then(|f| style_slot(c, f));
            if id != "camo_none" && pattern.is_none() && style.is_none() {
                summary.unresolved.push(format!("{}: {id} has no art", c.name));
            }
            blob.options.push(zhbo_native::camo::CamoOption { id: id.clone(), pattern, style });
        }
        out.insert(zhbo_native::camo_path(&c.name), blob.to_bytes());
        summary.assets += 1;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_camo_assets_convert_with_their_art() {
        let bo2 = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let common = crate::paks::load_zone(&bo2, "common_mp").unwrap();
        let patch = crate::paks::load_zone(&bo2, crate::fx::ZONE).unwrap();
        let inline = crate::paks::load_zone(&bo2, "common_patch_mp").unwrap();
        let paks = Paks::open(&bo2).unwrap();
        let sets = zhbo_formats::weapon::camo_all(&inline).unwrap();
        let names = crate::weapons::names(&bo2);
        let bo2w = zhbo_formats::weapon::all(&common).unwrap();
        let atts = zhbo_formats::weapon::attachments(&common).unwrap();
        let mut table = crate::weapons::table(&bo2w, &atts, &names);
        // The plain table leaves the option lists empty; fill what this test needs from the
        // converted set names (the real path uses table_with_metadata).
        let set_names: std::collections::HashSet<String> = sets.iter().map(|c| c.name.clone()).collect();
        for w in &mut table.weapons {
            w.camo = vec![
                "camo_none".into(), "camo_multicam".into(), "camo_atacs".into(), "camo_gold".into(),
                "camo_diamond".into(), "camo_carbon".into(),
            ];
            w.camo_material = crate::weapons::camo_material_for(&w.id, &set_names).unwrap_or_default();
        }
        let mut out = std::collections::BTreeMap::new();
        let s = files(&common, &patch, &inline, &paks, &sets, &bo2w, &table, &mut out).unwrap();
        let resolved: Vec<&str> = sets.iter().map(|c| c.name.as_str()).filter(|n| out.contains_key(&zhbo_native::camo_path(n))).collect();
        println!("resolved camo assets ({}): {resolved:?}", resolved.len());
        println!("excluded attachment camo assets ({}): {:?}", s.excluded.len(), s.excluded);
        // The seven optic camo assets are attachment recipes: they must produce no blob.
        for name in ["camo_reflex", "camo_holo", "camo_ir", "camo_acog", "camo_hybrid", "camo_mms", "camo_targetfinder"] {
            assert!(!out.contains_key(&zhbo_native::camo_path(name)), "{name} must not convert");
            assert!(s.excluded.iter().any(|e| e == name), "{name} is not excluded");
        }
        // 32 id-matched weapons plus the view-model join: the AS50's `camo_xpr50`, x95l/chicom/msmc,
        // `camo_ballistic_knife` and the riot shield (which the player table filters).
        assert!(s.assets >= 38, "{} blobs written", s.assets);
        assert!(resolved.contains(&"camo_xpr50"), "the AS50's camo must convert: {resolved:?}");
        assert!(resolved.contains(&"camo_riot_shield"), "the riot shield's camo must convert: {resolved:?}");
        assert_eq!(s.assets, resolved.len(), "assets count matches the written blobs");
        assert!(s.unresolved.iter().any(|u| u.ends_with("camo_multicam has no art")), "{:?}", s.unresolved);
        let an94 = zhbo_native::camo::Camo::from_bytes(&out[&zhbo_native::camo_path("camo_an94")]).unwrap();
        assert_eq!((an94.sets.len(), an94.styles.len()), (40, 15));
        let opt = |id: &str| an94.options.iter().find(|o| o.id == id).unwrap();
        assert!(opt("camo_atacs").pattern.is_some());
        assert_eq!(opt("camo_gold").style, Some(0));
        assert_eq!((opt("camo_multicam").pattern, opt("camo_multicam").style), (None, None));
        assert!(an94.sets[1].pattern.as_deref().is_some_and(|p| p.contains("atacs")));
        assert!(out.keys().any(|k| k.contains("camo_atacs_pattern")), "the pattern art");
        // The gold style's real colour map is `~-gcamo_code_spc`; `character::texture_file` strips
        // the `~-g` prefix, so the archive path is `zhbo_camo_code_spc.tga` (the plan's assumed
        // `camo_gold_col` name is not what this install's material carries).
        assert!(out.keys().any(|k| k.contains("camo_code_spc")), "the style art");
    }
}
