//! Builds `ConquerOps_BO2.big`: Black Ops II HUD art from the player's own install, laid out in
//! 1024² atlas pages with Zero Hour `MappedImage` definitions so the HUD can draw it by name. The
//! create-a-class icons and menu chrome live in [`crate::cac`] and join the same pages.
//!
//! Most sprites are found by their BO2 material name in the decrypted zones (`common_mp`,
//! `code_post_gfx_mp`), whose image points into the `.ipak` packs; a few older ones by the image's
//! name hash, picked by eye from contact sheets (`zhbo-assets sheets`). Art that BO2 draws additively
//! (bright lines on black) gets its brightness as alpha, since Zero Hour's UI blends by alpha. The
//! archive is generated on the player's machine and must never be redistributed.

use crate::images;
use std::collections::HashMap;
use std::path::Path;
use zhbo_formats::big::write_archive;
use zhbo_formats::ipak::ImageKey;
use zhbo_formats::iwi::Rgba;
use zhbo_formats::t6zone::Zone;

/// Bump when the spec or layout changes so launchers regenerate the archive. The weapon table is
/// archive content: the attachment stacking fix changes its synthesized variant lines.
pub const VERSION: u32 = 92;
pub use zhbo_core::names::BO2_ARCHIVE_NAME as ARCHIVE_NAME;
pub use zhbo_core::names::BO2_ARCHIVE_VERSION_PATH as VERSION_FILE;
/// Stem of each page's TGA; every MappedImage names the page it was packed into.
const TEXTURE: &str = "zhbo_bo2_hud";
const ATLAS: u32 = 1024;

/// Where a sprite's image comes from.
pub enum Source {
    /// The pack image with this key (picked by eye from a contact sheet).
    Hash(ImageKey),
    /// The first image of a BO2 material in a zone (the material named in code, or read from BO2's
    /// tables at conversion time, as the wildcards' card images are).
    Material(&'static str, std::borrow::Cow<'static, str>),
    /// A material named at conversion time (weapon HUD icons, the LUI ports' materials), searched
    /// in the MP zones in order.
    Dynamic(String),
}

pub struct Sprite {
    /// MappedImage name used by the HUD (see `zhbo_core::hud`).
    pub name: &'static str,
    pub source: Source,
    /// Largest side in the atlas (bigger images are halved until they fit).
    pub max: u32,
}

const fn mat(name: &'static str, material: &'static str, max: u32) -> Sprite {
    Sprite { name, source: Source::Material("common_mp", std::borrow::Cow::Borrowed(material)), max }
}

const fn hash(name: &'static str, data_hash: u32, name_hash: u32, max: u32) -> Sprite {
    Sprite { name, source: Source::Hash(ImageKey { data_hash, name_hash }), max }
}

/// The BO2 HUD art Conquer Ops uses.
pub const SPRITES: &[Sprite] = &[
    // The slanted HUD numerals.
    mat("ZHBO_BO2_Digit0", "hud_mp_num_big_0", 64),
    mat("ZHBO_BO2_Digit1", "hud_mp_num_big_1", 64),
    mat("ZHBO_BO2_Digit2", "hud_mp_num_big_2", 64),
    mat("ZHBO_BO2_Digit3", "hud_mp_num_big_3", 64),
    mat("ZHBO_BO2_Digit4", "hud_mp_num_big_4", 64),
    mat("ZHBO_BO2_Digit5", "hud_mp_num_big_5", 64),
    mat("ZHBO_BO2_Digit6", "hud_mp_num_big_6", 64),
    mat("ZHBO_BO2_Digit7", "hud_mp_num_big_7", 64),
    mat("ZHBO_BO2_Digit8", "hud_mp_num_big_8", 64),
    mat("ZHBO_BO2_Digit9", "hud_mp_num_big_9", 64),
    mat("ZHBO_BO2_Slash", "hud_mp_num_big_slash", 64),
    // Blood splatter, used for the damage overlay.
    hash("ZHBO_BO2_Blood", 0x0594_8908, 0xdf3a_5741, 256),
    // Red-dot sight glow.
    hash("ZHBO_BO2_RedDot", 0x023f_fb05, 0xfeba_df16, 128),
    // Hit marker.
    mat("ZHBO_BO2_HitMarker", "damage_feedback", 128),
    // Minimap: your arrow, the dot for everyone else (tinted with their team colour), the compass.
    mat("ZHBO_BO2_PlayerArrow", "compassping_player", 32),
    mat("ZHBO_BO2_PlayerBracket", "compassping_player_bracket", 128),
    // The Scavenger pickup icon (`scavenger_hud_create`, `_weapons.gsc:1892-1912`).
    mat("ZHBO_BO2_ScavengerPickup", "hud_scavenger_pickup", 64),
    mat("ZHBO_BO2_Dot", "compassping_enemy", 16),
    mat("ZHBO_BO2_Compass", "hud_horizontal_compass_minimap", 512),
    // Bottom-left team panel: frame, its fill, and the emblem plate with its light.
    mat("ZHBO_BO2_LeftLower", "hud_mp_vis_left_lower", 256),
    mat("ZHBO_BO2_LeftLowerBack", "hud_mp_vis_left_lower_back", 256),
    mat("ZHBO_BO2_FactionBacking", "hud_faction_backing", 128),
    mat("ZHBO_BO2_FactionLight", "hud_faction_back_light", 128),
    // Bottom-right weapon panel: frame, and the weapon name's box (ends and stretched middle).
    mat("ZHBO_BO2_RightLower", "hud_mp_vis_right_lower", 256),
    mat("ZHBO_BO2_GunTitleLeft", "hud_mp_vis_gun_title_left", 64),
    mat("ZHBO_BO2_GunTitleMid", "hud_mp_vis_gun_title_mid", 64),
    mat("ZHBO_BO2_GunTitleRight", "hud_mp_vis_gun_title_right", 64),
    // Scorestreak column: the bar, a streak's box (outline, lit fill, glow) and the field
    // computer's icon (BO2's orbital VSAT).
    mat("ZHBO_BO2_StreakBar", "hud_mp_vis_ks_bar", 128),
    mat("ZHBO_BO2_StreakBarBack", "hud_mp_vis_ks_bar_backing", 128),
    mat("ZHBO_BO2_StreakBox", "hud_mp_vis_ks_dpad_box", 64),
    mat("ZHBO_BO2_StreakBoxLit", "hud_mp_vis_ks_dpad_box_back_lit", 64),
    mat("ZHBO_BO2_StreakGlow", "hud_ks_lit_glow", 128),
    Sprite { name: "ZHBO_BO2_StreakVsat", source: Source::Material("code_post_gfx_mp", std::borrow::Cow::Borrowed("hud_ks_spy_sat_big")), max: 128 },
];

#[derive(Debug, Default)]
pub struct Report {
    pub found: Vec<String>,
    pub missing: Vec<(String, String)>,
    /// The BO2 Generals: what was converted, or why not.
    pub generals: Option<Result<String, String>>,
    /// BO2's UI fonts (`fonts.rs`): what was converted, or why the engine font stays.
    pub fonts: Option<Result<String, String>>,
}

/// Mirror an image top to bottom (rows reversed).
fn flip_vertical(img: &mut Rgba) {
    let row = img.width as usize * 4;
    let rows: Vec<Vec<u8>> = img.pixels.chunks(row).rev().map(<[u8]>::to_vec).collect();
    img.pixels = rows.concat();
}

/// BO2 draws some HUD art additively: bright on opaque black. Give such an image its brightness as
/// alpha (and full-strength colour) so alpha blending looks the same.
fn additive_to_alpha(img: &mut Rgba) {
    if img.pixels.as_chunks::<4>().0.iter().any(|p| p[3] < 250) {
        return; // has real alpha
    }
    for p in img.pixels.as_chunks_mut::<4>().0 {
        let a = p[0].max(p[1]).max(p[2]);
        if a > 0 {
            for c in &mut p[..3] {
                *c = (*c as u32 * 255 / a as u32) as u8;
            }
        }
        p[3] = a;
    }
}

/// Shelf-pack sizes into as many `ATLAS`-sized pages as they need; returns each sprite's page and
/// top-left corner. Pages stay at 1024², well below the engine's texture ceiling, and the pack can
/// grow with the catalog without shrinking every image.
fn pack(sizes: &[(u32, u32)]) -> Option<Vec<(usize, (u32, u32))>> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i].1));
    let mut at = vec![(0, (0, 0)); sizes.len()];
    let (mut page, mut x, mut y, mut shelf) = (0, 0, 0, 0);
    for i in order {
        let (w, h) = sizes[i];
        if w > ATLAS || h > ATLAS {
            return None;
        }
        if x + w > ATLAS {
            x = 0;
            y += shelf;
            shelf = 0;
        }
        if y + h > ATLAS {
            page += 1;
            (x, y, shelf) = (0, 0, 0);
        }
        at[i] = (page, (x, y));
        x += w;
        shelf = shelf.max(h);
    }
    Some(at)
}

fn texture_name(page: usize) -> String {
    format!("{TEXTURE}_{page}.tga")
}

/// Every distinct `dpadIcon` of the player weapons and their alt modes (inventory `ALTMODE`), in
/// table order, with its streamed image when the variant's inline material has one: the
/// materials the HUD's alt-fire slot can draw.
fn dpad_icons(weapons: &[zhbo_formats::weapon::Weapon], roster: &crate::weapons::Roster) -> Vec<(String, Option<zhbo_formats::ipak::ImageKey>)> {
    let mut icons: Vec<(String, Option<zhbo_formats::ipak::ImageKey>)> = Vec::new();
    for w in weapons.iter().filter(|w| roster.is_player_weapon(w) || w.inventory == zhbo_formats::weapon::inventory::ALTMODE) {
        let Some(icon) = w.dpad_icon.as_ref().filter(|i| !i.is_empty()) else { continue };
        match icons.iter_mut().find(|(i, _)| i == icon) {
            Some((_, key)) => *key = key.or(w.dpad_icon_key),
            None => icons.push((icon.clone(), w.dpad_icon_key)),
        }
    }
    icons
}

/// The wanted sprites with each name once: the first source, the largest size.
fn merge_wanted(wanted: Vec<(String, Source, u32)>) -> Vec<(String, Source, u32)> {
    let mut out: Vec<(String, Source, u32)> = Vec::with_capacity(wanted.len());
    for (name, source, max) in wanted {
        match out.iter_mut().find(|(n, ..)| *n == name) {
            Some(w) => w.2 = w.2.max(max),
            None => out.push((name, source, max)),
        }
    }
    out
}

/// A standalone overlay page's texture: the file the archive stores (`Art\Textures\<this>`) and
/// the name its MappedImage gives, one spelling for both.
fn standalone_texture(name: &str) -> String {
    format!("{name}.tga")
}

/// Read the sprites from the BO2 install at `bo2_dir` and return the archive bytes.
pub fn build_archive(bo2_dir: &Path) -> Result<(Vec<u8>, Report), String> {
    let mut report = Report::default();
    let paks = crate::generals::Paks::open(bo2_dir)?;
    let mut zones: HashMap<&str, Zone> = HashMap::new();
    let mut found: Vec<(String, Rgba)> = Vec::new();
    // Full-screen art that cannot share a 1024-px atlas page (the snipers' scope overlays).
    let mut standalone: Vec<(String, Rgba)> = Vec::new();
    // The fixed HUD and create-a-class sprites, then each converted weapon's own HUD icon
    // (its `hudIcon` material, `WeaponDef::hud_icon_image`: the class buttons, the editor's rows and
    // the lethal and tactical counters draw them).
    let mut wanted: Vec<(String, Source, u32)> = SPRITES
        .iter()
        .chain(crate::cac::SPRITES.iter())
        .map(|s| (s.name.to_string(), match &s.source {
            Source::Hash(h) => Source::Hash(*h),
            Source::Material(z, m) => Source::Material(z, m.clone()),
            Source::Dynamic(m) => Source::Dynamic(m.clone()),
        }, s.max))
        .collect();
    // The materials the LUI ports draw, under their BO2 names (`zhbo_core::lui::materials`).
    // A name ending in `_flipv` is its base material mirrored vertically (LUI draws an image whose
    // top is below its bottom upside down; `addLargePopupBackground`'s bottom rule is one).
    for (material, max) in zhbo_core::lui::materials::MATERIALS {
        let base = material.strip_suffix(zhbo_core::lui::materials::FLIP_V).unwrap_or(material);
        wanted.push((zhbo_core::lui::materials::image(material), Source::Dynamic(base.to_string()), *max));
    }
    // The snipers' full-screen ADS overlays: standalone pages, not the HUD atlas.
    for (material, max) in zhbo_core::hud::OVERLAY_MATERIALS {
        let base = material.strip_suffix(zhbo_core::lui::materials::FLIP_V).unwrap_or(material);
        wanted.push((zhbo_core::lui::materials::image(material), Source::Dynamic(base.to_string()), *max));
    }
    let mp = crate::paks::MpPool::load(bo2_dir)?;
    let (weapons, attachments) = mp.weapons()?;
    let crate::paks::MpPool { patch, common, .. } = mp;
    let roster = crate::weapons::Roster::read(&patch);
    zones.insert("common_mp", common);
    // The create-a-class weapon pictures: each item's image in BO2's `mp/statstable.csv` plus
    // `_big` (`classpreview.lua`'s `GetItemImage(...) .. "_big"`). The knife's personalize page
    // shows `knife_held`'s column 6 as it is (`caccamomenu.lua`'s `knifePersonalization` branch:
    // `combat_tab_knife`, no `_big`).
    let item_images = crate::cac::item_rows(bo2_dir);
    for id in crate::weapons::converted_base(&weapons, &roster) {
        let Some(w) = weapons.iter().find(|w| w.name == crate::generals::base_id(&id)) else { continue };
        let base = crate::generals::base_id(&id);
        if let Some((image, slot)) = item_images.get(base.trim_end_matches("_mp")) {
            let offhand = [zhbo_core::classmenu::items::PRIMARY_GRENADE, zhbo_core::classmenu::items::SPECIAL_GRENADE].contains(&slot.as_str());
            if base == zhbo_core::weapons::KNIFE_HELD {
                wanted.push((zhbo_core::names::cac_weapon_image(base), Source::Dynamic(image.clone()), 512));
            } else if image.starts_with("menu_mp_weapons_") {
                wanted.push((zhbo_core::names::cac_weapon_image(base), Source::Dynamic(format!("{image}_big")), 512));
            } else if offhand {
                // The offhand items' pictures (their images are HUD icons, `hud_grenadeicon`) are
                // the image plus `_256` (`cacgrenadesbutton.lua:199-205`: the grenade button's
                // `GetItemImage(...) .. "_256"`).
                wanted.push((zhbo_core::names::cac_weapon_image(base), Source::Dynamic(format!("{image}_256")), 256));
            }
        }
        // The weapon's HUD icon under its material's name (`WeaponDef::hud_icon_image`): one
        // sprite per material, shared by every def that names it (an alt mode names its gun's).
        let Some(icon) = &w.hud_icon else { continue };
        let name = zhbo_core::lui::materials::image(icon);
        match w.hud_icon_key {
            Some(hash) => wanted.push((name, Source::Hash(hash), 64)),
            // Equipment icons are stubs in common_mp; the menus' zones hold the real material.
            None => wanted.push((name, Source::Dynamic(icon.clone()), 64)),
        }
    }
    // Each converted weapon's crosshair art (`reticleSide`/`reticleCenter`, `hud::center`), its
    // obituary icon (`killIcon`, `hud::obituary`; a 2:1 or 4:1 icon kept twice as wide) and its
    // grenade indicator icon (`indicatorIcon`, `hud::grenade`), under their material names, packed
    // once each: the player weapons' and their alt modes' (a grenade launcher's own kill icon), as
    // the alt-fire slot's art below.
    for w in weapons.iter().filter(|w| roster.is_player_weapon(w) || w.inventory == zhbo_formats::weapon::inventory::ALTMODE) {
        for m in [&w.reticle.side, &w.reticle.center].into_iter().flatten() {
            wanted.push((zhbo_core::lui::materials::image(m), Source::Dynamic(m.clone()), 64));
        }
        if let Some(icon) = &w.kill_icon {
            let max = if w.kill_icon_ratio == 0 { 64 } else { 128 };
            wanted.push((zhbo_core::lui::materials::image(icon), w.kill_icon_key.map_or_else(|| Source::Dynamic(icon.clone()), Source::Hash), max));
        }
        if let Some(icon) = &w.indicator_icon {
            wanted.push((zhbo_core::lui::materials::image(icon), w.indicator_icon_key.map_or_else(|| Source::Dynamic(icon.clone()), Source::Hash), 64));
        }
    }
    // The alt-fire slot's art (`zhbo_core::hud::ammo::alt_slot_icon`): each variant's `dpadIcon`
    // (`hud_gl_select`, `hud_select_fire_bullet_stack`), the player weapons' and their alt modes'.
    for (icon, hash) in dpad_icons(&weapons, &roster) {
        let source = hash.map_or_else(|| Source::Dynamic(icon.clone()), Source::Hash);
        wanted.push((zhbo_core::lui::materials::image(&icon), source, 128));
    }
    // The personalize camo grid's thumbnails (`menu_camo_*_pattern`, `menu_theater_nodata` for
    // `None`): the same `mp/attachmenttable.csv` rows the weapon table carries.
    let names = crate::weapons::names(bo2_dir);
    for c in crate::weapons::camo_options(&patch, &names) {
        let image = c.image.clone();
        if !image.is_empty() {
            wanted.push((image.clone(), Source::Dynamic(image.clone()), 128));
            // A store camo's class-preview swatch is its own `<image>_32` (`SetupMTXCamoSwatch`).
            if c.mtx {
                let swatch = format!("{image}_32");
                wanted.push((swatch.clone(), Source::Dynamic(swatch), 32));
            }
        }
    }
    // The wildcard grid's art: each bonus card's own image (`mp/statstable.csv` column 6), the
    // same rows the weapon table's `BonusCard` blocks carry.
    wanted.extend(crate::cac::wildcard_sprites(&crate::weapons::bonus_card_rows(&patch, &names)));
    // The reticle grid's art: each reticle row's `<optic>_<N>` material per optic, the Hybrid
    // Optic's pair (`zhbo_core::weapons::reticle_art`, `reticlesgridbutton.lua` `SetupButtonImages`;
    // `patch_ui_mp`). The Hybrid's `reflex_N`/`acog_N` are the other optics' own art, packed once.
    let mut reticle_art: Vec<String> = Vec::new();
    for r in crate::weapons::reticle_options(&patch, &names) {
        for optic in zhbo_core::weapons::RETICLE_OPTICS {
            for image in zhbo_core::weapons::reticle_art(optic, &r.id) {
                if !reticle_art.contains(&image) {
                    reticle_art.push(image.clone());
                    wanted.push((image.clone(), Source::Dynamic(image), 128));
                }
            }
        }
    }
    // One icon per attachment in the converted table: its `mp/attachmenttable.csv` image (column
    // 6), the material the core names through `hud::sprites::attachment_icon`. BO2's variants share
    // an art set (`silencer_smg`/`silencer` are both the suppressor); an attachment with no image
    // row is reported, never fatal.
    let att_meta = crate::weapons::attachment_metadata(&patch, &std::collections::HashMap::new());
    let mut seen_atts: Vec<String> = Vec::new();
    for att in attachments {
        if att.kind == 0 || att.name.starts_with(',') {
            continue;
        }
        let short = crate::weapons::short_name(&att.name);
        if seen_atts.contains(&short) {
            continue;
        }
        seen_atts.push(short.clone());
        match att_meta.get(&short).map(|m| m.image.as_str()).filter(|i| !i.is_empty()) {
            Some(image) => wanted.push((zhbo_core::lui::materials::image(image), Source::Dynamic(image.to_string()), 64)),
            None => report.missing.push((format!("attachment {short}"), "no icon in mp/attachmenttable.csv".into())),
        }
    }
    // One sprite per name: a material several surfaces draw (the FHJ's `menu_mp_weapons_fhj` is
    // its HUD icon and its action-slot icon) is packed once, at the largest size any asks for.
    let wanted = merge_wanted(wanted);
    let overlay_names: std::collections::HashSet<String> = zhbo_core::hud::OVERLAY_MATERIALS.iter().map(|(m, _)| zhbo_core::lui::materials::image(m)).collect();
    for (name, source, max) in &wanted {
        let hash = match source {
            Source::Hash(h) => Some(*h),
            Source::Material(zone, material) => {
                // A missing menu zone leaves its sprites on the report's `missing` list instead of
                // failing the whole conversion (the HUD has drawn fallbacks for every one).
                if !zones.contains_key(zone) {
                    if let Ok(z) = crate::generals::load_zone(bo2_dir, zone) {
                        zones.insert(zone, z);
                    }
                }
                zones.get(zone)
                    .and_then(|z| zhbo_formats::xmodel::find_material(z, material.as_ref()))
                    .and_then(|m| m.textures.iter().find_map(|t| t.image.as_ref().map(|i| i.key())))
            }
            Source::Dynamic(material) => {
                // Weapon menu art lives in the MP patches; a few DLC guns only in `frontend`, the
                // smoke grenade's icon only in `code_post_gfx`.
                for zone in ["code_post_gfx_mp", "ui_mp", "patch_mp", "patch_ui_mp", "frontend", "code_post_gfx"] {
                    if !zones.contains_key(zone) {
                        if let Ok(z) = crate::generals::load_zone(bo2_dir, zone) {
                            zones.insert(zone, z);
                        }
                    }
                }
                // Each zone's own assets first, then the materials nested inside other assets (also
                // pool members); only then the slow scan for a material loaded inline inside another
                // asset (`hud_mp_firerate_*` lives inside `common_mp`'s weapons). `code_post_gfx`
                // (the shared post-gfx zone) last: the only zone that loads the smoke grenade's
                // `hud_willy_pete` in full.
                let order = ["common_mp", "code_post_gfx_mp", "ui_mp", "patch_mp", "patch_ui_mp", "frontend", "code_post_gfx"];
                // The material's first image; a `,name` reference stub (no stream hash) is resolved
                // by name in the image pool, as BO2 does: `hud_willy_pete` in `common_mp` points at
                // `,hud_us_smokegrenade`, whose full image is `patch_mp`'s.
                let image = |m: zhbo_formats::xmodel::Material| {
                    let i = m.textures.iter().find_map(|t| t.image.clone())?;
                    if i.stream_hash != 0 {
                        return Some(i.key());
                    }
                    order.iter().filter_map(|z| zones.get(z)).find_map(|z| zhbo_formats::xmodel::find_image(z, &i.name)).map(|i| i.key())
                };
                let mc = format!("mc/{material}");
                order
                    .iter()
                    .filter_map(|z| zones.get(z))
                    .find_map(|z| {
                        zhbo_formats::xmodel::find_asset_material(z, material)
                            .or_else(|| zhbo_formats::xmodel::find_asset_material(z, &mc))
                            .and_then(image)
                    })
                    .or_else(|| {
                        // A material nested inside another asset is in BO2's pool too
                        // (`Zone::linked`): the map-only ADS overlays (`envg_ir`, `specter`,
                        // `vzoom`) are nested in `common_mp`'s attachment uniques.
                        order.iter().filter_map(|z| zones.get(z)).find_map(|z| {
                            zhbo_formats::xmodel::find_linked_material(z, material)
                                .or_else(|| zhbo_formats::xmodel::find_linked_material(z, &mc))
                                .and_then(image)
                        })
                    })
                    .or_else(|| {
                        order
                            .iter()
                            .filter_map(|z| zones.get(z))
                            .find_map(|z| {
                                zhbo_formats::xmodel::find_inline_material(z, material)
                                    .or_else(|| zhbo_formats::xmodel::find_inline_material(z, &mc))
                                    .and_then(image)
                            })
                    })
            }
        };
        match hash.and_then(|h| paks.image(h)) {
            Some(img) => {
                let mut img = crate::generals::shrink(img, *max);
                additive_to_alpha(&mut img);
                if name.ends_with(zhbo_core::lui::materials::FLIP_V) {
                    flip_vertical(&mut img);
                }
                if overlay_names.contains(name) {
                    standalone.push((name.clone(), img));
                } else {
                    found.push((name.clone(), img));
                }
                report.found.push(name.clone());
            }
            None => report.missing.push((name.clone(), "not in this install".into())),
        }
    }
    // The General's command bar buttons: the loadout's BO2 icons on Zero Hour-sized tiles, a BO2
    // reticle for Take Control, and the class set's chrome (`data/.../ZHBO_A_Buttons.ini` keeps
    // Zero Hour stand-ins for the no-archive path).
    for (name, img) in button_tiles(&found) {
        report.found.push(name.clone());
        found.push((name, img));
    }
    // A pixel of padding around each sprite keeps filtering from bleeding between them; the
    // padding repeats the sprite's edge texels, so a linear sample at the sprite's edge sees its
    // own texture as BO2's standalone image would (no fade into the neighbour or the empty page —
    // a tiled strip like `menu_mp_cac_backing` would otherwise seam at every repeat).
    let sizes: Vec<(u32, u32)> = found.iter().map(|(_, i)| (i.width + 2, i.height + 2)).collect();
    let places = pack(&sizes).ok_or("a HUD sprite is larger than the 1024px atlas page")?;
    let pages = places.iter().map(|(page, _)| *page).max().unwrap_or(0) + 1;
    let mut atlases: Vec<Rgba> =
        (0..pages).map(|_| Rgba { width: ATLAS, height: ATLAS, pixels: vec![0; (ATLAS * ATLAS * 4) as usize] }).collect();
    let mut ini = String::from(
        "; Generated by zhbo-assets from the player's own Call of Duty: Black Ops II install.\n\
         ; Private data: do not redistribute.\n\n",
    );
    for ((s, img), (page, (x, y))) in found.iter().zip(places) {
        let at = (x + 1, y + 1);
        blit(&mut atlases[page], img, at);
        ini.push_str(&format!(
            "MappedImage {}\n  Texture = {}\n  TextureWidth = {ATLAS}\n  TextureHeight = {ATLAS}\n  \
             Coords = Left:{} Top:{} Right:{} Bottom:{}\n  Status = NONE\nEnd\n\n",
            s,
            texture_name(page),
            at.0,
            at.1,
            at.0 + img.width - 1, // Zero Hour's Coords are inclusive
            at.1 + img.height - 1
        ));
    }
    // The standalone overlay pages: one texture each, named after the material. The MappedImage
    // names its page as the archive stores it (`<name>.tga`), like the atlas pages.
    let mut standalone_files: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, img) in standalone {
        let texture = standalone_texture(&name);
        standalone_files.push((format!("Art\\Textures\\{texture}"), images::tga_bytes(&img)));
        ini.push_str(&format!(
            "MappedImage {}\n  Texture = {}\n  TextureWidth = {}\n  TextureHeight = {}\n  \
             Coords = Left:0 Top:0 Right:{} Bottom:{}\n  Status = NONE\nEnd\n\n",
            name,
            texture,
            img.width,
            img.height,
            img.width - 1,
            img.height - 1
        ));
    }
    let mut files: Vec<(String, Vec<u8>)> = atlases
        .iter()
        .enumerate()
        .map(|(page, atlas)| (format!("Art\\Textures\\{}", texture_name(page)), images::tga_bytes(atlas)))
        .collect();
    files.extend(standalone_files);
    files.push(("Data\\INI\\MappedImages\\HandCreated\\ZHBO_BO2.ini".to_string(), ini.into_bytes()));
    files.push((VERSION_FILE.to_string(), version_stamp().into_bytes()));
    // BO2's UI fonts. Without them the screens keep the engine font.
    report.fonts = Some(crate::fonts::files(bo2_dir).map(|(font_files, summary)| {
        files.extend(font_files);
        summary
    }));
    // The Generals as BO2 soldiers. On failure they keep their Zero Hour art.
    report.generals = Some(match crate::generals::files(bo2_dir) {
        Ok((general_files, summary)) => {
            files.extend(general_files);
            Ok(summary)
        }
        Err(e) => Err(e),
    });
    Ok((write_archive(&files), report))
}

/// Command bar button size (Zero Hour's are 61 x 49; twice that keeps the art sharp).
const TILE: (u32, u32) = (122, 98);

/// A command bar button: a dark tile with `art` fitted in the middle (aspect kept). A reticle tile
/// (Take Control) draws BO2-style crosshair lines around a small `art` (the red dot).
/// A white arrow pointing left (the Build menu's Back), on clear.
fn back_arrow() -> Rgba {
    let (w, h) = (48u32, 32u32);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let dy = (y as f32 + 0.5 - h as f32 / 2.0).abs();
            // The head: a triangle from the tip at the left; the shaft: a bar to the right.
            let head = (x as f32) < 18.0 && dy <= x as f32 * 0.9;
            let shaft = (16..44).contains(&x) && dy <= 4.0;
            if head || shaft {
                let p = ((y * w + x) * 4) as usize;
                pixels[p..p + 4].copy_from_slice(&[235, 235, 235, 255]);
            }
        }
    }
    Rgba { width: w, height: h, pixels }
}

fn button_tile(art: Option<&Rgba>, reticle: bool) -> Rgba {
    let (w, h) = TILE;
    let mut t = Rgba { width: w, height: h, pixels: vec![0; (w * h * 4) as usize] };
    for y in 0..h {
        let shade = 46 - (y * 24 / h) as u8;
        for x in 0..w {
            let p = ((y * w + x) * 4) as usize;
            t.pixels[p..p + 4].copy_from_slice(&[shade, shade + 3, shade + 6, 255]);
        }
    }
    let mut over = |x: u32, y: u32, c: [u8; 4]| {
        if x < w && y < h {
            let p = ((y * w + x) * 4) as usize;
            let a = c[3] as u32;
            for (k, &ck) in c.iter().enumerate().take(3) {
                t.pixels[p + k] = ((ck as u32 * a + t.pixels[p + k] as u32 * (255 - a)) / 255) as u8;
            }
        }
    };
    if reticle {
        let (cx, cy) = (w / 2, h / 2);
        for d in 10..30 {
            for k in 0..3u32 {
                over(cx + d, cy - 1 + k, [235, 235, 235, 255]);
                over(cx - d, cy - 1 + k, [235, 235, 235, 255]);
                over(cx - 1 + k, cy + d, [235, 235, 235, 255]);
                over(cx - 1 + k, cy - d, [235, 235, 235, 255]);
            }
        }
    }
    if let Some(src) = art {
        let (bw, bh) = if reticle { (28.0, 28.0) } else { (w as f32 * 0.9, h as f32 * 0.82) };
        let scale = (bw / src.width as f32).min(bh / src.height as f32);
        let (dw, dh) = ((src.width as f32 * scale) as u32, (src.height as f32 * scale) as u32);
        let (ox, oy) = ((w - dw) / 2, (h - dh) / 2);
        for y in 0..dh {
            for x in 0..dw {
                let (sx, sy) = ((x as f32 / scale) as u32, (y as f32 / scale) as u32);
                let p = ((sy.min(src.height - 1) * src.width + sx.min(src.width - 1)) * 4) as usize;
                let c = &src.pixels[p..p + 4];
                over(ox + x, oy + y, [c[0], c[1], c[2], c[3]]);
            }
        }
    }
    t
}

/// A magazine for the Reload button: a slanted box with rounds stacked at its lips.
fn magazine_glyph() -> Rgba {
    let (w, h) = (32u32, 44u32);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        // The box leans like a rifle magazine; its top third holds the rounds.
        let lean = (h - y) as f32 * 0.25;
        for x in 0..w {
            let fx = x as f32 - lean;
            let wall = (3.0..21.0).contains(&fx) && (fx < 6.0 || fx >= 18.0 || y >= h - 3);
            let round = (8.0..16.0).contains(&fx) && y < h / 3 && y % 5 < 3;
            if wall || round {
                let p = ((y * w + x) * 4) as usize;
                pixels[p..p + 4].copy_from_slice(&[235, 235, 235, 255]);
            }
        }
    }
    Rgba { width: w, height: h, pixels }
}

/// The command bar's generated tiles. `ZHBO_Button_ClassTile` is a plain face because the bridge
/// draws a kit over it every frame (each class button's, and the General's own on his gun and
/// order buttons: his class is the player's, so no conversion-time art can show it) — it must
/// never fall back to a base-game cameo (`ZHBO_A_Buttons.ini`'s hand-set cell), which read as
/// parts of unrelated unit art under the kit. `ZHBO_Button_ChooseClass` carries BO2's
/// create-a-class grid pattern; its base-game rect straddled two cells of
/// `SAUserInterface512_005.tga`, stitching two cameos.
fn button_tiles(found: &[(String, Rgba)]) -> Vec<(String, Rgba)> {
    let art = |name: &str| found.iter().find(|(n, _)| n == name).map(|(_, i)| i.clone());
    let mut tiles = Vec::new();
    let dot = art("ZHBO_BO2_RedDot");
    tiles.push(("ZHBO_Button_TakeControl".to_string(), button_tile(dot.as_ref(), true)));
    tiles.push(("ZHBO_Button_Back".to_string(), button_tile(Some(&back_arrow()), false)));
    tiles.push(("ZHBO_Button_Reload".to_string(), button_tile(Some(&magazine_glyph()), false)));
    tiles.push(("ZHBO_Button_ClassTile".to_string(), button_tile(None, false)));
    tiles.push(("ZHBO_Button_ChooseClass".to_string(), button_tile(art("ZHBO_BO2_MenuGrid").as_ref(), false)));
    tiles
}

fn blit(dst: &mut Rgba, src: &Rgba, at: (u32, u32)) {
    for y in 0..src.height.min(dst.height.saturating_sub(at.1)) {
        let s = (y * src.width * 4) as usize;
        let w = src.width.min(dst.width.saturating_sub(at.0));
        let d = (((at.1 + y) * dst.width + at.0) * 4) as usize;
        dst.pixels[d..d + (w * 4) as usize].copy_from_slice(&src.pixels[s..s + (w * 4) as usize]);
    }
    // The one-texel ring round the sprite repeats its nearest edge texel (corners included).
    if src.width == 0 || src.height == 0 {
        return;
    }
    let (x0, y0) = (at.0 as i64, at.1 as i64);
    let (sw, sh) = (src.width as i64, src.height as i64);
    for y in y0 - 1..=y0 + sh {
        for x in x0 - 1..=x0 + sw {
            let inside = (x0..x0 + sw).contains(&x) && (y0..y0 + sh).contains(&y);
            if inside || x < 0 || y < 0 || x >= dst.width as i64 || y >= dst.height as i64 {
                continue;
            }
            let (sx, sy) = ((x - x0).clamp(0, sw - 1), (y - y0).clamp(0, sh - 1));
            let s = ((sy * sw + sx) * 4) as usize;
            let d = ((y * dst.width as i64 + x) * 4) as usize;
            dst.pixels[d..d + 4].copy_from_slice(&src.pixels[s..s + 4]);
        }
    }
}

/// What the archive was built from: the converter version, and the loadout overrides that pick which
/// weapons' art is converted (`ZHBO_LOADOUT`, `ZHBO_CONVERT_ALL_WEAPONS`, `ZHBO_CONVERT_ALL_COMBOS`).
fn version_stamp() -> String {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    // Only the parts that pick art: spare rounds (`primary_reserve`, ...) don't change it.
    let loadout: Vec<String> = var("ZHBO_LOADOUT").split(';').filter(|p| !p.trim_start().contains("_reserve")).map(str::to_string).collect();
    format!("{VERSION}\nloadout={}\nall={}\ncombos={}\n", loadout.join(";"), var("ZHBO_CONVERT_ALL_WEAPONS"), var("ZHBO_CONVERT_ALL_COMBOS"))
}

/// The archive's version entry as [`build_archive`] writes it (its path and this build's stamp): what
/// [`archive_version`] reads back. For tests that stand an archive in for a real one.
pub fn version_entry() -> (String, Vec<u8>) {
    (VERSION_FILE.to_string(), version_stamp().into_bytes())
}

/// Whether the archive at `path` is one this converter would keep: ours, this [`VERSION`], built
/// for this loadout. The launcher rebuilds any other; `cargo xtask build` keeps a current one in
/// the dist (it is the player's own conversion, not part of the staged build).
pub fn archive_current(path: &Path) -> bool {
    archive_version(path) == Some(VERSION)
}

/// Version stored in an existing archive, if it is one of ours and was built for this loadout.
pub fn archive_version(path: &Path) -> Option<u32> {
    let big = zhbo_formats::big::BigArchive::open(path).ok()?;
    let e = big.find(VERSION_FILE)?;
    let text = String::from_utf8(big.read(e).ok()?).ok()?;
    if text.trim_end() != version_stamp().trim_end() {
        return Some(0);
    }
    text.lines().next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_atlas_padding_repeats_the_sprite_edge() {
        // Linear filtering at a sprite's edge samples its padding: an opaque strip tiled across
        // the screen must not fade into the empty page at every repeat.
        let mut page = Rgba { width: 6, height: 5, pixels: vec![0; 6 * 5 * 4] };
        let px = |v: u8| [v, v, v, 255];
        let src = Rgba { width: 2, height: 2, pixels: [px(10), px(20), px(30), px(40)].concat() };
        blit(&mut page, &src, (1, 1));
        let at = |x: u32, y: u32| page.pixels[((y * 6 + x) * 4) as usize];
        let rows: Vec<Vec<u8>> = (0..5).map(|y| (0..6).map(|x| at(x, y)).collect()).collect();
        assert_eq!(rows, vec![
            vec![10, 10, 20, 20, 0, 0],
            vec![10, 10, 20, 20, 0, 0],
            vec![30, 30, 40, 40, 0, 0],
            vec![30, 30, 40, 40, 0, 0],
            vec![0, 0, 0, 0, 0, 0],
        ]);
    }

    #[test]
    fn a_sprite_two_surfaces_want_is_packed_once_at_the_larger_size() {
        let w = vec![
            ("ZHBO_Mat_menu_mp_weapons_fhj".to_string(), Source::Dynamic("menu_mp_weapons_fhj".into()), 64),
            ("ZHBO_Mat_hud_gl_select".to_string(), Source::Dynamic("hud_gl_select".into()), 128),
            ("ZHBO_Mat_menu_mp_weapons_fhj".to_string(), Source::Dynamic("menu_mp_weapons_fhj".into()), 128),
        ];
        let m = merge_wanted(w);
        assert_eq!(m.iter().map(|(n, _, max)| (n.as_str(), *max)).collect::<Vec<_>>(), [("ZHBO_Mat_menu_mp_weapons_fhj", 128), ("ZHBO_Mat_hud_gl_select", 128)]);
    }

    #[test]
    fn dpad_icons_are_the_alt_modes_distinct_materials() {
        use zhbo_formats::weapon::{inventory, Weapon};
        let alt = |name: &str, icon: Option<&str>, hash: Option<u32>| Weapon { name: name.into(), inventory: inventory::ALTMODE, dpad_icon: icon.map(str::to_string), dpad_icon_key: hash.map(|h| zhbo_formats::ipak::ImageKey { data_hash: h, name_hash: h }), ..Default::default() };
        let weapons = [
            alt("gl_an94_mp", Some("hud_gl_select"), None),
            alt("gl_scar_mp", Some("hud_gl_select"), Some(7)),
            alt("sf_an94_mp", Some("hud_select_fire_bullet_stack"), Some(9)),
            alt("dualoptic_an94_mp", None, None),
            // Not a player weapon nor an alt mode: its icon is never drawn.
            Weapon { name: "killstreak_remote_turret_mp".into(), dpad_icon: Some("hud_ks_turret".into()), ..Default::default() },
        ];
        // One entry per material; any variant's streamed image serves.
        let key = |h: u32| zhbo_formats::ipak::ImageKey { data_hash: h, name_hash: h };
        assert_eq!(dpad_icons(&weapons, &crate::weapons::Roster::any()), vec![("hud_gl_select".to_string(), Some(key(7))), ("hud_select_fire_bullet_stack".to_string(), Some(key(9)))]);
    }

    fn assert_packed(sizes: &[(u32, u32)], at: &[(usize, (u32, u32))]) {
        for (i, ((page, a), sa)) in at.iter().zip(sizes).enumerate() {
            assert!(a.0 + sa.0 <= ATLAS && a.1 + sa.1 <= ATLAS, "{page}:{a:?} out of bounds");
            for ((other_page, b), sb) in at[i + 1..].iter().zip(&sizes[i + 1..]) {
                if page != other_page {
                    continue;
                }
                let overlap = a.0 < b.0 + sb.0 && b.0 < a.0 + sa.0 && a.1 < b.1 + sb.1 && b.1 < a.1 + sa.1;
                assert!(!overlap, "{page}:{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn packing_fits_without_overlap() {
        let sizes: Vec<(u32, u32)> = vec![(514, 66), (258, 130), (258, 130), (258, 130), (130, 258), (66, 66), (34, 66), (18, 18)];
        let at = pack(&sizes).unwrap();
        assert_packed(&sizes, &at);
    }

    #[test]
    fn packing_opens_a_new_page_when_the_atlas_is_full() {
        let sizes: Vec<(u32, u32)> = vec![(ATLAS, ATLAS), (ATLAS, ATLAS)];
        let at = pack(&sizes).unwrap();
        assert_eq!(at[0].0, 0);
        assert_eq!(at[1].0, 1);
        assert_eq!(texture_name(at[0].0), "zhbo_bo2_hud_0.tga");
        assert_eq!(texture_name(at[1].0), "zhbo_bo2_hud_1.tga");
        assert_eq!(standalone_texture("ZHBO_Mat_scope_overlay"), "ZHBO_Mat_scope_overlay.tga", "named as the archive stores it");
        assert_packed(&sizes, &at);
    }

    #[test]
    fn the_cac_pack_fits_pages() {
        // The fixed sprites at their maximum size, plus a full converted weapon list's icons and
        // the command bar tiles; the pack must land every one of them, on one or more pages.
        let mut sizes: Vec<(u32, u32)> = SPRITES
            .iter()
            .chain(crate::cac::SPRITES.iter())
            .map(|s| (s.max + 2, s.max + 2))
            .collect();
        for _ in 0..128 {
            sizes.push((66, 66));
        }
        let at = pack(&sizes).unwrap();
        assert_eq!(at.len(), sizes.len(), "every wanted sprite is placed");
        assert_packed(&sizes, &at);
        assert!(at.iter().any(|(page, _)| *page > 0), "the pack outgrew the old single atlas");
    }

    #[test]
    fn button_tiles_fit_the_art() {
        let wide = Rgba { width: 64, height: 16, pixels: [255u8, 255, 255, 255].repeat(64 * 16) };
        let t = button_tile(Some(&wide), false);
        assert_eq!((t.width, t.height), TILE);
        let px = |x: u32, y: u32| t.pixels[((y * t.width + x) * 4) as usize];
        assert_eq!(px(TILE.0 / 2, TILE.1 / 2), 255, "art in the middle");
        assert!(px(TILE.0 / 2, 5) < 60, "dark above the wide art");
        let r = button_tile(None, true);
        assert!(r.pixels[(((TILE.1 / 2) * TILE.0 + TILE.0 / 2 + 20) * 4) as usize] > 200, "crosshair line");
    }

    #[test]
    fn the_class_bar_buttons_are_generated_plain_tiles() {
        // The class buttons draw their kit over the tile every frame; the Create a Class button's
        // base-game rect straddled two cells of SAUserInterface512_005 (the tail of one cameo and
        // the head of the next). Both must come from the converter, not from that hand-set sheet.
        let tiles = button_tiles(&[]);
        let names: Vec<&str> = tiles.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"ZHBO_Button_ClassTile"));
        assert!(names.contains(&"ZHBO_Button_ChooseClass"));
        let class = &tiles.iter().find(|(n, _)| n == "ZHBO_Button_ClassTile").unwrap().1;
        assert_eq!((class.width, class.height), TILE);
        // A plain face: only the dark gradient, so the kit reads over it.
        assert!(class.pixels.chunks(4).all(|p| p[3] == 255 && p[0] < 60 && p[1] < 60 && p[2] < 60));
    }

    #[test]
    fn every_command_button_image_is_a_generated_tile() {
        // A command button whose ButtonImage only exists as a hand-set rect in ZHBO_A_Buttons.ini
        // samples a cameo cell, not a button face: that is how the class buttons painted base-game
        // soldier art under their kit and how Create a Class straddled two cells. Only the Build
        // menu's dozer cameo is deliberately Zero Hour art.
        const BUTTONS: &str = include_str!("../../../data/Data/INI/CommandButton/ZHBO_CommandButtons.ini");
        let generated: Vec<String> = button_tiles(&[]).into_iter().map(|(n, _)| n).collect();
        let mut checked = 0;
        for line in BUTTONS.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let name = value.trim();
            if key.trim() != "ButtonImage" || !name.starts_with("ZHBO_Button_") {
                continue;
            }
            checked += 1;
            assert!(
                generated.iter().any(|g| g == name) || name == "ZHBO_Button_Build",
                "{name} has no generated tile: its base-game rect is a cameo cell, not a button face"
            );
        }
        assert!(checked > 10, "the command button INI was found and parsed ({checked})");
    }

    #[test]
    fn additive_art_gets_alpha() {
        let mut img = Rgba { width: 2, height: 1, pixels: vec![0, 0, 0, 255, 100, 50, 0, 255] };
        additive_to_alpha(&mut img);
        assert_eq!(img.pixels, vec![0, 0, 0, 0, 255, 127, 0, 100]);
    }

    #[test]
    fn spec_matches_the_names_the_hud_draws() {
        let mut spec: Vec<&str> = SPRITES.iter().chain(crate::cac::SPRITES.iter()).map(|s| s.name).collect();
        // Attachment icons are made from the catalog at build time and the wildcard cards from the
        // statstable's bonus-card rows, so their names are not in either fixed sprite list (the
        // core lists them for the class screen).
        let mut hud: Vec<&str> =
            zhbo_core::hud::sprites::ALL.iter().copied().filter(|n| !n.contains("_Attach_") && !n.contains("_Wildcard_")).collect();
        spec.sort_unstable();
        hud.sort_unstable();
        assert_eq!(spec, hud);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn builds_from_real_install() {
        let bo2 = zhbo_requirements::check_requirements().black_ops2.unwrap();
        let (bytes, report) = build_archive(&bo2.path).unwrap();
        println!("{report:?}");
        // A few weapons have no HUD icon image in BO2 (the HUD then shows their short name).
        // Every ADS overlay converts: the four sniper ones are `patch_mp` assets, `envg_ir`,
        // `specter` and `vzoom` are nested in `common_mp` (`Zone::linked`), as is the Willy
        // Pete's HUD icon.
        for name in [
            "ZHBO_Mat_scope_overlay_ballista",
            "ZHBO_Mat_scope_overlay_dsr50",
            "ZHBO_Mat_scope_overlay_envg_ir",
            "ZHBO_Mat_scope_overlay_specter",
            "ZHBO_Mat_scope_overlay_svu",
            "ZHBO_Mat_scope_overlay_vzoom",
            "ZHBO_Mat_scope_overlay_xpr50",
            // The alt-fire slot's art (inline in common_mp's alt modes) and the smoke grenade's
            // counter icon (only `code_post_gfx` loads it).
            "ZHBO_Mat_hud_gl_select",
            "ZHBO_Mat_hud_select_fire_bullet_stack",
            "ZHBO_Mat_hud_willy_pete",
        ] {
            assert!(report.found.iter().any(|f| f == name), "{name} did not convert: {:?}", report.missing);
        }
        // Only a weapon's HUD icon may lack an image.
        let (pool, _) = crate::paks::MpPool::load(&bo2.path).unwrap().weapons().unwrap();
        let icons: std::collections::BTreeSet<String> = pool.iter().filter_map(|w| w.hud_icon.as_deref()).map(zhbo_core::lui::materials::image).collect();
        assert!(report.missing.iter().all(|(name, _)| icons.contains(name)), "{:?}", report.missing);
        // The reticle grid: every custom-reticle optic's art for every reticle row converts (the
        // Hybrid Optic's pair included), so no cell of any optic's page is blank.
        let patch = crate::generals::load_zone(&bo2.path, crate::fx::ZONE).unwrap();
        let rows = crate::weapons::reticle_options(&patch, &crate::weapons::names(&bo2.path));
        assert_eq!(rows.len(), 38, "reticle_0..37");
        for optic in zhbo_core::weapons::RETICLE_OPTICS {
            for r in &rows {
                let art = zhbo_core::weapons::reticle_art(optic, &r.id);
                assert!(!art.is_empty(), "{optic} {}", r.id);
                for image in art {
                    assert!(report.found.contains(&image), "{optic} {}: {image} did not convert", r.id);
                }
            }
        }
        // The HUD fills the screen round a scope overlay with black (`zhbo_core::hud`'s
        // `draw_scope_overlay`), standing in for BO2's stretch of the art's own edge texels
        // (`FUN_004b4840`): every overlay's border must be opaque black for that to be BO2's look.
        let files = zhbo_formats::big::members(&bytes).unwrap();
        for (material, _) in zhbo_core::hud::OVERLAY_MATERIALS {
            let path = format!("Art\\Textures\\{}", standalone_texture(&zhbo_core::lui::materials::image(material)));
            let tga = &files.iter().find(|(p, _)| *p == path).unwrap_or_else(|| panic!("{path} is in the archive")).1;
            let (w, h) = (u16::from_le_bytes([tga[12], tga[13]]) as usize, u16::from_le_bytes([tga[14], tga[15]]) as usize);
            let px = |x: usize, y: usize| &tga[18 + (y * w + x) * 4..][..4];
            let edge = (0..h).flat_map(|y| [(0, y), (w - 1, y)]).chain((0..w).flat_map(|x| [(x, 0), (x, h - 1)]));
            for (x, y) in edge {
                assert_eq!(px(x, y), [0, 0, 0, 255], "{material}: border texel ({x}, {y}) is not opaque black");
            }
        }
        let dir = std::env::temp_dir().join("zhbo-hud-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(ARCHIVE_NAME);
        std::fs::write(&p, bytes).unwrap();
        assert_eq!(archive_version(&p), Some(VERSION));
    }
}
