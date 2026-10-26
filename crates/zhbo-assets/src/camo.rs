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
    /// Names whose pixels resolved in none of the zones (reported, never fatal).
    pub unresolved: Vec<String>,
}

/// The variant suffixes a style material's base name may be stored under (the blob names the base,
/// the zones carry the `_1` variant; research §3(d) bug D2).
fn suffix_candidates(name: &str) -> Vec<String> {
    vec![format!("{name}_1")]
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
    let mut wanted_images: BTreeMap<String, &xmodel::Image> = BTreeMap::new();
    for c in sets {
        for s in &c.sets {
            for i in [&s.solid_image, &s.pattern_image].into_iter().flatten() {
                wanted_images.entry(i.name.clone()).or_insert(i);
            }
        }
    }
    let mut images = BTreeMap::new();
    let mut unresolved: Vec<String> = Vec::new();
    for (name, image) in wanted_images {
        let found = paks.image(image.stream_hash)
            .or_else(|| xmodel::find_image(common, &name).and_then(|i| paks.image(i.stream_hash)))
            .or_else(|| xmodel::find_image(patch, &name).and_then(|i| paks.image(i.stream_hash)));
        let Some(rgba) = found else {
            unresolved.push(name);
            continue;
        };
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
            .or_else(|| xmodel::find_asset_material(common, &name))
            .or_else(|| xmodel::find_asset_material(common, bare))
            .or_else(|| xmodel::find_inline_material(inline, &name))
            .or_else(|| xmodel::find_inline_material(inline, bare))
            .or_else(|| xmodel::find_inline_material(common, &name))
            .or_else(|| suffix_candidates(&name).into_iter().find_map(|n| {
                let bare = n.trim_start_matches("mc/");
                xmodel::find_asset_material(patch, &n)
                    .or_else(|| xmodel::find_asset_material(common, &n))
                    .or_else(|| xmodel::find_inline_material(inline, &n))
                    .or_else(|| xmodel::find_inline_material(common, &n))
                    .or_else(|| xmodel::find_asset_material(patch, bare))
                    .or_else(|| xmodel::find_asset_material(common, bare))
            }));
        let Some(mat) = found else {
            unresolved.push(name);
            continue;
        };
        let color = mat.color_map();
        let rgba = color.and_then(pixel);
        // The shared style map is a 32x32 black specular-named texture; using it as the base colour
        // turns the surface black. Keep the surface's stock map instead (research §3(d) bug D2).
        let black = color.is_some_and(|c| c.name.contains("_spc"));
        let mut texture = None;
        let mut maps_texture = None;
        if let (Some(color), Some(rgba), false) = (color, rgba, black) {
            let file = character::texture_file(&color.name);
            let path = format!("Art\\Textures\\{file}");
            let maps_path = format!("Art\\Textures\\{}", maps_file(&file));
            if !out.contains_key(&path) {
                let mut rgba = shrink(rgba, MAX_TEXTURE);
                put_occlusion(&mut rgba, mat.image(xmodel::TS_OCCLUSION_MAP).and_then(pixel).as_ref());
                let maps = surface_maps(
                    rgba.width,
                    rgba.height,
                    mat.image(xmodel::TS_NORMAL_MAP).and_then(pixel).as_ref(),
                    mat.image(xmodel::TS_SPECULAR_MAP).and_then(pixel).as_ref(),
                );
                out.insert(maps_path.clone(), tga_bytes(&maps));
                out.insert(path.clone(), tga_bytes(&rgba));
            }
            texture = Some(path);
            maps_texture = Some(maps_path);
        }
        if texture.is_none() {
            unresolved.push(name.clone());
        }
        let blend = crate::native::blend(mat.blend());
        let opaque = mat.blend() == zhbo_formats::w3d::Blend::Opaque;
        materials.insert(
            name,
            nm::Material {
                name: mat.name.clone(),
                blend,
                emissive: matches!(blend, nm::Blend::Additive { .. }),
                color: texture,
                maps: opaque.then_some(maps_texture).flatten(),
                sort_key: mat.sort_key,
                cull: crate::native::cull(mat.cull()),
                camo: nm::CamoLayer::None,
            },
        );
    }
    Art { images, materials, unresolved }
}

/// Every `WeaponCamo` asset a BO2 def or attachment unique points at (`weaponCamo`): the gun's
/// own (`camo_an94`, the INSAS's `camo_msmc`) and the attachments' (`camo_reflex`, `camo_ir`). BO2
/// paints each viewmodel part with its own entry's asset (`FUN_00529600`, G16), so every one of
/// them needs a recipe.
fn referenced(bo2w: &[weapon::Weapon]) -> BTreeSet<&str> {
    bo2w.iter()
        .flat_map(|w| w.weapon_camo.iter().chain(w.attachment_uniques.iter().flat_map(|u| u.weapon_camo.iter())))
        .map(String::as_str)
        .collect()
}

/// BO2's camo option id → (pattern image stem, style family). `None` = the 48 assets carry no art
/// for it: the option stays selectable and draws the stock finish (research §5.1/§5.4).
const OPTION_ART: &[(&str, Option<&str>, Option<&str>)] = &[
    ("camo_none", None, None),
    ("camo_multicam", Some("t6_camo_devgru"), None),
    ("camo_matuka", Some("t6_camo_bloodshot"), None),
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
    /// Camo assets no def or unique points at (no part wears them): no blob.
    pub unreferenced: Vec<String>,
}

/// Build every camo blob and write it beside the art: one per asset a def or unique points at.
/// `table` supplies the option ids (`WeaponDef.camo`, the base-tab list, from the weapon wearing the
/// asset; an attachment's asset — and a weapon the player table filters, the riot shield — takes
/// the base camo rows, since the class's one camo choice indexes every part's asset alike).
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
    let mut summary = Summary { assets: 0, images: art.images.len(), styles: art.materials.len(), unresolved: art.unresolved.clone(), unreferenced: Vec::new() };
    let wanted = referenced(bo2w);
    for c in sets {
        if !wanted.contains(c.name.as_str()) {
            summary.unreferenced.push(c.name.clone());
            continue;
        }
        let camo_ids: Vec<String> = match table.weapons.iter().find(|w| w.camo_material == c.name) {
            Some(w) => w.camo.clone(),
            None => table.camo.iter().take(crate::weapons::BASE_CAMO_OPTIONS).map(|c| c.id.clone()).collect(),
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
            materials: {
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                c.materials.iter().flat_map(|s| s.entries.iter().flat_map(|e| e.camo.iter()))
                    .filter(|n| seen.insert(n.as_str()))
                    .filter_map(|n| art.materials.get(n).cloned()).collect()
            },
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
        // The plain table's option list is empty; take the base tab from the patch's own table
        // (the production path does this in `table_with_metadata` with `camo_options`).
        let base: Vec<String> = crate::weapons::camo_options(&patch, &names)
            .iter()
            .take(crate::weapons::BASE_CAMO_OPTIONS)
            .map(|c| c.id.clone())
            .collect();
        for w in &mut table.weapons {
            w.camo = base.clone();
        }
        table.camo = crate::weapons::camo_options(&patch, &names);
        let mut out = std::collections::BTreeMap::new();
        let s = files(&common, &patch, &inline, &paks, &sets, &bo2w, &table, &mut out).unwrap();
        let resolved: Vec<&str> = sets.iter().map(|c| c.name.as_str()).filter(|n| out.contains_key(&zhbo_native::camo_path(n))).collect();
        println!("resolved camo assets ({}): {resolved:?}", resolved.len());
        println!("unreferenced camo assets ({}): {:?}", s.unreferenced.len(), s.unreferenced);
        println!("unresolved ({}): {:?}", s.unresolved.len(), s.unresolved);
        // The optic camo assets are the attachments' own recipes (G16): each converts, and its
        // pattern options resolve like a weapon's.
        for name in ["camo_reflex", "camo_holo", "camo_ir", "camo_acog", "camo_hybrid", "camo_mms", "camo_targetfinder"] {
            let blob = out.get(&zhbo_native::camo_path(name)).unwrap_or_else(|| panic!("{name} must convert"));
            let blob = zhbo_native::camo::Camo::from_bytes(blob).unwrap();
            let atacs = blob.options.iter().find(|o| o.id == "camo_atacs").unwrap_or_else(|| panic!("{name}: no atacs row"));
            assert!(atacs.pattern.is_some(), "{name}: atacs has no pattern set");
        }
        // Every weapon asset the defs point at, the AS50's `camo_xpr50`, x95l/chicom/msmc,
        // `camo_ballistic_knife` and the riot shield (which the player table filters), plus the
        // seven optic assets.
        assert!(s.assets >= 45, "{} blobs written", s.assets);
        assert!(resolved.contains(&"camo_xpr50"), "the AS50's camo must convert: {resolved:?}");
        assert!(resolved.contains(&"camo_riot_shield"), "the riot shield's camo must convert: {resolved:?}");
        assert_eq!(s.assets, resolved.len(), "assets count matches the written blobs");
        let no_art: Vec<&String> = base
            .iter()
            .filter(|id| id.as_str() != "camo_none" && s.unresolved.iter().any(|u| u.ends_with(&format!("{id} has no art"))))
            .collect();
        assert!(no_art.is_empty(), "base options without art: {no_art:?}");
        let an94 = zhbo_native::camo::Camo::from_bytes(&out[&zhbo_native::camo_path("camo_an94")]).unwrap();
        assert_eq!((an94.sets.len(), an94.styles.len()), (40, 15));
        let opt = |id: &str| an94.options.iter().find(|o| o.id == id).unwrap();
        assert!(opt("camo_atacs").pattern.is_some());
        assert_eq!(opt("camo_gold").style, Some(0));
        assert_eq!(opt("camo_multicam").style, None);
        assert!(opt("camo_multicam").pattern.is_some(), "multicam resolves to the devgru set");
        assert!(opt("camo_matuka").pattern.is_some(), "matuka resolves to the bloodshot set");
        let multicam = opt("camo_multicam").pattern.unwrap() as usize;
        let matuka = opt("camo_matuka").pattern.unwrap() as usize;
        assert!(an94.sets[multicam].pattern.as_deref().is_some_and(|p| p.contains("devgru")), "{:?}", an94.sets[multicam].pattern);
        assert!(an94.sets[matuka].pattern.as_deref().is_some_and(|p| p.contains("bloodshot")), "{:?}", an94.sets[matuka].pattern);
        assert!(an94.sets[1].pattern.as_deref().is_some_and(|p| p.contains("atacs")));
        assert!(out.keys().any(|k| k.contains("camo_atacs_pattern")), "the pattern art");
        // The gold style's real colour map is `~-gcamo_code_spc`; the converter now skips the
        // shared black `_spc` map (the surface keeps its stock map), so the archive must not
        // carry it as a colour map (Change 2c / plan 5c).
        assert!(!out.keys().any(|k| k.contains("camo_code_spc")), "the black _spc map must not be a colour map");
    }

    /// The base 19 camo options (`attachmenttable.csv:32-50`) all name art in `OPTION_ART`,
    /// except `camo_none` (stock). The two formerly art-less names resolve to the devgru and
    /// bloodshot sets the CSV's e4 indices select (multicam e4=1 -> set 0; matuka e4=7 -> set 6).
    #[test]
    fn the_base_camo_options_all_name_art() {
        const BASE: [&str; 19] = [
            "camo_none", "camo_multicam", "camo_atacs", "camo_erdl", "camo_siberia", "camo_choco",
            "camo_tiger_blue", "camo_matuka", "camo_ghostex", "camo_kryptek", "camo_carbon",
            "camo_blossom", "camo_artofwar", "camo_ronin", "camo_skulls", "camo_gold",
            "camo_diamond", "camo_elite1", "camo_collectors",
        ];
        let entry = |id: &str| OPTION_ART.iter().copied().find(|(o, _, _)| *o == id);
        for id in BASE {
            let (_, stem, family) = entry(id).unwrap_or_else(|| panic!("{id} missing from OPTION_ART"));
            if id == "camo_none" {
                assert!(stem.is_none() && family.is_none(), "camo_none must draw the stock finish");
            } else {
                assert!(stem.is_some() || family.is_some(), "{id} names no art");
            }
        }
        assert_eq!(entry("camo_multicam").and_then(|(_, s, _)| s), Some("t6_camo_devgru"));
        assert_eq!(entry("camo_matuka").and_then(|(_, s, _)| s), Some("t6_camo_bloodshot"));
    }

    use super::suffix_candidates;

    #[test]
    fn suffix_candidates_try_the_variant() {
        assert_eq!(suffix_candidates("mc/mtl_weapon_camo_gold"), vec!["mc/mtl_weapon_camo_gold_1".to_string()]);
        assert_eq!(suffix_candidates("mc/mtl_weapon_camo_gold_alt"), vec!["mc/mtl_weapon_camo_gold_alt_1".to_string()]);
    }
}
