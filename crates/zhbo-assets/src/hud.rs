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
pub const VERSION: u32 = 15;
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
    // The slanted HUD numerals (`hud_mp_num_big_*`).
    hash("ZHBO_BO2_Digit0", 0x0ad6_b469, 64),
    hash("ZHBO_BO2_Digit1", 0x1023_a304, 64),
    hash("ZHBO_BO2_Digit2", 0x0341_43b7, 64),
    hash("ZHBO_BO2_Digit3", 0x147c_846c, 64),
    hash("ZHBO_BO2_Digit4", 0x056b_fdc0, 64),
    hash("ZHBO_BO2_Digit5", 0x0566_ac71, 64),
    hash("ZHBO_BO2_Digit6", 0x0979_d350, 64),
    hash("ZHBO_BO2_Digit7", 0x0b81_1161, 64),
    hash("ZHBO_BO2_Digit8", 0x08c6_95e9, 64),
    hash("ZHBO_BO2_Digit9", 0x1aa6_7f7b, 64),
    hash("ZHBO_BO2_Slash", 0x010f_77fb, 64),
    // Blood splatter, used for the damage overlay.
    hash("ZHBO_BO2_Blood", 0x0594_8908, 256),
    // Red-dot sight glow.
    hash("ZHBO_BO2_RedDot", 0x023f_fb05, 128),
    // Hit marker.
    mat("ZHBO_BO2_HitMarker", "damage_feedback", 128),
    // Minimap: your arrow, the dot for everyone else (tinted with their team colour), the compass.
    mat("ZHBO_BO2_PlayerArrow", "compassping_player", 32),
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
    pub found: Vec<&'static str>,
    pub missing: Vec<(&'static str, String)>,
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
    let mut found: Vec<(&Sprite, Rgba)> = Vec::new();
    for s in SPRITES {
        let hash = match s.source {
            Source::Hash(h) => Some(h),
            Source::Material(zone, material) => {
                if !zones.contains_key(zone) {
                    zones.insert(zone, crate::generals::load_zone(bo2_dir, zone)?);
                }
                zhbo_formats::xmodel::find_material(&zones[zone], material)
                    .and_then(|m| m.textures.iter().find_map(|t| t.image.as_ref().map(|i| i.stream_hash)))
            }
        };
        match hash.and_then(|h| paks.image(h)) {
            Some(img) => {
                let mut img = crate::generals::shrink(img, s.max);
                additive_to_alpha(&mut img);
                found.push((s, img));
                report.found.push(s.name);
            }
            None => report.missing.push((s.name, "not in this install".into())),
        }
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
            s.name,
            at.0,
            at.1,
            at.0 + img.width - 1, // Zero Hour's Coords are inclusive
            at.1 + img.height - 1
        ));
    }
    let mut files = vec![
        (format!("Art\\Textures\\{TEXTURE}"), images::tga_bytes(&atlas)),
        ("Data\\INI\\MappedImages\\HandCreated\\ZHBO_BO2.ini".to_string(), ini.into_bytes()),
        (VERSION_FILE.to_string(), VERSION.to_string().into_bytes()),
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

fn blit(dst: &mut Rgba, src: &Rgba, at: (u32, u32)) {
    for y in 0..src.height.min(dst.height.saturating_sub(at.1)) {
        let s = (y * src.width * 4) as usize;
        let w = src.width.min(dst.width.saturating_sub(at.0));
        let d = (((at.1 + y) * dst.width + at.0) * 4) as usize;
        dst.pixels[d..d + (w * 4) as usize].copy_from_slice(&src.pixels[s..s + (w * 4) as usize]);
    }
}

/// Version stored in an existing archive, if it is one of ours.
pub fn archive_version(path: &Path) -> Option<u32> {
    let big = zhbo_formats::big::BigArchive::open(path).ok()?;
    let e = big.find(VERSION_FILE)?;
    String::from_utf8(big.read(e).ok()?).ok()?.trim().parse().ok()
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
        assert!(report.missing.is_empty());
        let dir = std::env::temp_dir().join("zhbo-hud-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(ARCHIVE_NAME);
        std::fs::write(&p, bytes).unwrap();
        assert_eq!(archive_version(&p), Some(VERSION));
    }
}
