//! Builds `ConquerOps_BO2.big`: Black Ops II HUD art from the player's own install, laid out in one
//! atlas texture with Zero Hour `MappedImage` definitions so the HUD can draw it by name.
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
use zhbo_formats::iwi::Rgba;
use zhbo_formats::t6zone::Zone;

/// Bump when the spec or layout changes so launchers regenerate the archive.
pub const VERSION: u32 = 38;
pub const ARCHIVE_NAME: &str = "ConquerOps_BO2.big";
pub const VERSION_FILE: &str = "Data\\ConquerOps\\bo2_hud_version.txt";
const TEXTURE: &str = "zhbo_bo2_hud.tga";
const ATLAS: u32 = 1024;

/// Where a sprite's image comes from.
pub enum Source {
    /// The image with this name hash in the packs.
    Hash(u32),
    /// The first image of a BO2 material in a zone.
    Material(&'static str, &'static str),
    /// A material in `common_mp` named at conversion time (weapon HUD icons).
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
    Sprite { name, source: Source::Material("common_mp", material), max }
}

const fn hash(name: &'static str, hash: u32, max: u32) -> Sprite {
    Sprite { name, source: Source::Hash(hash), max }
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
    hash("ZHBO_BO2_Blood", 0x0594_8908, 256),
    // Red-dot sight glow.
    hash("ZHBO_BO2_RedDot", 0x023f_fb05, 128),
    // Hit marker.
    mat("ZHBO_BO2_HitMarker", "damage_feedback", 128),
    // Minimap: your arrow, the dot for everyone else (tinted with their team colour), the compass.
    mat("ZHBO_BO2_PlayerArrow", "compassping_player", 32),
    mat("ZHBO_BO2_PlayerBracket", "compassping_player_bracket", 128),
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
    Sprite { name: "ZHBO_BO2_StreakVsat", source: Source::Material("code_post_gfx_mp", "hud_ks_spy_sat_big"), max: 128 },
];

#[derive(Debug, Default)]
pub struct Report {
    pub found: Vec<String>,
    pub missing: Vec<(String, String)>,
    /// The BO2 Generals: what was converted, or why not.
    pub generals: Option<Result<String, String>>,
}

/// BO2 draws some HUD art additively: bright on opaque black. Give such an image its brightness as
/// alpha (and full-strength colour) so alpha blending looks the same.
fn additive_to_alpha(img: &mut Rgba) {
    if img.pixels.chunks_exact(4).any(|p| p[3] < 250) {
        return; // has real alpha
    }
    for p in img.pixels.chunks_exact_mut(4) {
        let a = p[0].max(p[1]).max(p[2]);
        if a > 0 {
            for c in &mut p[..3] {
                *c = (*c as u32 * 255 / a as u32) as u8;
            }
        }
        p[3] = a;
    }
}

/// Shelf-pack sizes into the atlas; returns each top-left corner, or `None` if they don't fit.
fn pack(sizes: &[(u32, u32)]) -> Option<Vec<(u32, u32)>> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i].1));
    let mut at = vec![(0, 0); sizes.len()];
    let (mut x, mut y, mut shelf) = (0, 0, 0);
    for i in order {
        let (w, h) = sizes[i];
        if x + w > ATLAS {
            x = 0;
            y += shelf;
            shelf = 0;
        }
        if w > ATLAS || y + h > ATLAS {
            return None;
        }
        at[i] = (x, y);
        x += w;
        shelf = shelf.max(h);
    }
    Some(at)
}

/// Read the sprites from the BO2 install at `bo2_dir` and return the archive bytes.
pub fn build_archive(bo2_dir: &Path) -> Result<(Vec<u8>, Report), String> {
    let mut report = Report::default();
    let paks = crate::generals::Paks::open(bo2_dir)?;
    let mut zones: HashMap<&str, Zone> = HashMap::new();
    let mut found: Vec<(String, Rgba)> = Vec::new();
    // The fixed sprites, then each converted weapon's own HUD icon (`ZHBO_BO2_Icon_<weapon>`, the
    // lethal and tactical counters use them).
    let mut wanted: Vec<(String, Source, u32)> = SPRITES
        .iter()
        .map(|s| (s.name.to_string(), match &s.source {
            Source::Hash(h) => Source::Hash(*h),
            Source::Material(z, m) => Source::Material(z, m),
            Source::Dynamic(m) => Source::Dynamic(m.clone()),
        }, s.max))
        .collect();
    zones.insert("common_mp", crate::generals::load_zone(bo2_dir, "common_mp")?);
    let weapons = zhbo_formats::weapon::all(&zones["common_mp"]);
    for id in crate::weapons::converted(&weapons) {
        let Some(w) = weapons.iter().find(|w| w.name == crate::generals::base_id(&id)) else { continue };
        let name = format!("ZHBO_BO2_Icon_{id}");
        match (w.hud_icon_hash, &w.hud_icon) {
            (Some(hash), _) => wanted.push((name, Source::Hash(hash), 64)),
            // Equipment icons are stubs in common_mp; the menus' zones hold the real material.
            (None, Some(icon)) => wanted.push((name, Source::Dynamic(icon.clone()), 64)),
            _ => {}
        }
    }
    for (name, source, max) in &wanted {
        let hash = match source {
            Source::Hash(h) => Some(*h),
            Source::Material(zone, material) => {
                if !zones.contains_key(zone) {
                    zones.insert(zone, crate::generals::load_zone(bo2_dir, zone)?);
                }
                zhbo_formats::xmodel::find_material(&zones[zone], material)
                    .and_then(|m| m.textures.iter().find_map(|t| t.image.as_ref().map(|i| i.stream_hash)))
            }
            Source::Dynamic(material) => {
                for zone in ["code_post_gfx_mp", "patch_mp"] {
                    if !zones.contains_key(zone) {
                        if let Ok(z) = crate::generals::load_zone(bo2_dir, zone) {
                            zones.insert(zone, z);
                        }
                    }
                }
                ["common_mp", "code_post_gfx_mp", "patch_mp"]
                    .iter()
                    .filter_map(|z| zones.get(z))
                    .filter_map(|z| zhbo_formats::xmodel::find_material(z, material))
                    .find_map(|m| m.textures.iter().find_map(|t| t.image.as_ref().map(|i| i.stream_hash)).filter(|h| *h != 0))
            }
        };
        match hash.and_then(|h| paks.image(h)) {
            Some(img) => {
                let mut img = crate::generals::shrink(img, *max);
                additive_to_alpha(&mut img);
                found.push((name.clone(), img));
                report.found.push(name.clone());
            }
            None => report.missing.push((name.clone(), "not in this install".into())),
        }
    }
    // The General's command bar buttons: the loadout's BO2 icons on Zero Hour-sized tiles, and a
    // BO2 reticle for Take Control (`data/.../ZHBO_A_Buttons.ini` has Zero Hour stand-ins).
    let l = zhbo_core::weapons::Loadout::default();
    let icon = |id: &str| found.iter().find(|(n, _)| *n == format!("ZHBO_BO2_Icon_{id}")).map(|(_, i)| i.clone());
    let mut tiles = Vec::new();
    for (name, id) in [("ZHBO_Button_Primary", &l.primary), ("ZHBO_Button_Secondary", &l.secondary), ("ZHBO_Button_Lethal", &l.lethal), ("ZHBO_Button_Tactical", &l.tactical)] {
        if let Some(img) = icon(id) {
            tiles.push((name.to_string(), button_tile(Some(&img), false)));
        }
    }
    let dot = found.iter().find(|(n, _)| n == "ZHBO_BO2_RedDot").map(|(_, i)| i.clone());
    tiles.push(("ZHBO_Button_TakeControl".to_string(), button_tile(dot.as_ref(), true)));
    tiles.push(("ZHBO_Button_Back".to_string(), button_tile(Some(&back_arrow()), false)));
    for (name, img) in tiles {
        report.found.push(name.clone());
        found.push((name, img));
    }
    // A pixel of padding around each sprite keeps filtering from bleeding between them.
    let sizes: Vec<(u32, u32)> = found.iter().map(|(_, i)| (i.width + 2, i.height + 2)).collect();
    let places = pack(&sizes).ok_or("HUD sprites do not fit the atlas")?;
    let mut atlas = Rgba { width: ATLAS, height: ATLAS, pixels: vec![0; (ATLAS * ATLAS * 4) as usize] };
    let mut ini = String::from(
        "; Generated by zhbo-assets from the player's own Call of Duty: Black Ops II install.\n\
         ; Private data: do not redistribute.\n\n",
    );
    for ((s, img), (x, y)) in found.iter().zip(places) {
        let at = (x + 1, y + 1);
        blit(&mut atlas, img, at);
        ini.push_str(&format!(
            "MappedImage {}\n  Texture = {TEXTURE}\n  TextureWidth = {ATLAS}\n  TextureHeight = {ATLAS}\n  \
             Coords = Left:{} Top:{} Right:{} Bottom:{}\n  Status = NONE\nEnd\n\n",
            s,
            at.0,
            at.1,
            at.0 + img.width - 1, // Zero Hour's Coords are inclusive
            at.1 + img.height - 1
        ));
    }
    let mut files = vec![
        (format!("Art\\Textures\\{TEXTURE}"), images::tga_bytes(&atlas)),
        ("Data\\INI\\MappedImages\\HandCreated\\ZHBO_BO2.ini".to_string(), ini.into_bytes()),
        (VERSION_FILE.to_string(), version_stamp().into_bytes()),
    ];
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
            for k in 0..3 {
                t.pixels[p + k] = ((c[k] as u32 * a + t.pixels[p + k] as u32 * (255 - a)) / 255) as u8;
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

fn blit(dst: &mut Rgba, src: &Rgba, at: (u32, u32)) {
    for y in 0..src.height.min(dst.height.saturating_sub(at.1)) {
        let s = (y * src.width * 4) as usize;
        let w = src.width.min(dst.width.saturating_sub(at.0));
        let d = (((at.1 + y) * dst.width + at.0) * 4) as usize;
        dst.pixels[d..d + (w * 4) as usize].copy_from_slice(&src.pixels[s..s + (w * 4) as usize]);
    }
}

/// What the archive was built from: the converter version, and the loadout overrides that pick which
/// weapons' art is converted (`ZHBO_LOADOUT`, `ZHBO_CONVERT_ALL_WEAPONS`).
fn version_stamp() -> String {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    // Only the parts that pick art: spare rounds (`primary_reserve`, ...) don't change it.
    let loadout: Vec<String> = var("ZHBO_LOADOUT").split(';').filter(|p| !p.trim_start().contains("_reserve")).map(str::to_string).collect();
    format!("{VERSION}\nloadout={}\nall={}\n", loadout.join(";"), var("ZHBO_CONVERT_ALL_WEAPONS"))
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
    fn packing_fits_without_overlap() {
        let sizes: Vec<(u32, u32)> = vec![(514, 66), (258, 130), (258, 130), (258, 130), (130, 258), (66, 66), (34, 66), (18, 18)];
        let at = pack(&sizes).unwrap();
        for (i, (a, sa)) in at.iter().zip(&sizes).enumerate() {
            assert!(a.0 + sa.0 <= ATLAS && a.1 + sa.1 <= ATLAS);
            for (b, sb) in at[i + 1..].iter().zip(&sizes[i + 1..]) {
                let overlap = a.0 < b.0 + sb.0 && b.0 < a.0 + sa.0 && a.1 < b.1 + sb.1 && b.1 < a.1 + sa.1;
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
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
    fn additive_art_gets_alpha() {
        let mut img = Rgba { width: 2, height: 1, pixels: vec![0, 0, 0, 255, 100, 50, 0, 255] };
        additive_to_alpha(&mut img);
        assert_eq!(img.pixels, vec![0, 0, 0, 0, 255, 127, 0, 100]);
    }

    #[test]
    fn spec_matches_the_names_the_hud_draws() {
        let mut spec: Vec<&str> = SPRITES.iter().map(|s| s.name).collect();
        let mut hud: Vec<&str> = zhbo_core::hud::sprites::ALL.to_vec();
        spec.sort();
        hud.sort();
        assert_eq!(spec, hud);
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn builds_from_real_install() {
        let bo2 = zhbo_requirements::check_requirements().black_ops2.unwrap();
        let (bytes, report) = build_archive(&bo2.path).unwrap();
        println!("{report:?}");
        // A few weapons have no HUD icon image in BO2 (the HUD then shows their short name).
        assert!(report.missing.iter().all(|(name, _)| name.starts_with("ZHBO_BO2_Icon_")), "{:?}", report.missing);
        let dir = std::env::temp_dir().join("zhbo-hud-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(ARCHIVE_NAME);
        std::fs::write(&p, bytes).unwrap();
        assert_eq!(archive_version(&p), Some(VERSION));
    }
}
