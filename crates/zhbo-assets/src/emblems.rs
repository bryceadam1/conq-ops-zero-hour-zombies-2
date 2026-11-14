//! The weapon emblem: each faction's Zero Hour logo, from the player's own Zero Hour install.
//!
//! BO2 paints the player's emblem on the gun's emblem surface (`mc/mtl_player_icon`, the
//! `sw4_3d_player_icon` technique: the colour map at the mesh UVs). The mod's deliberate
//! divergence: Zero Hour's faction replaces BO2's emblem, so the surface shows the General's side
//! logo — the same art the HUD's team panel draws (`ZHBO_TeamLogo`, `<Side>_Logo`). Zero Hour
//! defines those as `MappedImage <Side>_Logo` blocks (a texture page and a rect) in its
//! `Data\INI\MappedImages` INIs; the page lives in a (localized) `Art\Textures` folder of one of its
//! `.big` archives. The converter crops each faction's rect into
//! `zhbo_native::emblem_path(letter)`.

use crate::generals::FACTIONS;
use crate::images;
use crate::screenshot::atlas::parse_mapped_images;
use std::collections::BTreeMap;
use std::path::Path;
use zhbo_formats::big::BigArchive;
use zhbo_formats::iwi::Rgba;

/// The `.big` archives of a Zero Hour install, in name order (a fixed, deterministic search).
fn archives(zh_dir: &Path) -> Vec<BigArchive> {
    let mut paths: Vec<_> = std::fs::read_dir(zh_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("big")))
        .collect();
    paths.sort();
    paths.into_iter().filter_map(|p| BigArchive::open(&p).ok()).collect()
}

/// The `MappedImage` named `name` in the install's mapped-image INIs: its texture file and rect.
fn mapped_image(bigs: &[BigArchive], name: &str) -> Option<(String, [u32; 4])> {
    bigs.iter().find_map(|b| {
        b.entries()
            .iter()
            .filter(|e| e.path.to_ascii_lowercase().starts_with("data\\ini\\mappedimages\\") && e.path.to_ascii_lowercase().ends_with(".ini"))
            .find_map(|e| {
                let text = String::from_utf8_lossy(&b.read(e).ok()?).into_owned();
                parse_mapped_images(&text).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, m)| (m.texture, m.rect))
            })
    })
}

/// The texture page `file` from any `Art\Textures` folder of the install's archives.
fn page(bigs: &[BigArchive], file: &str) -> Option<Rgba> {
    let suffix = format!("art\\textures\\{}", file.to_ascii_lowercase());
    bigs.iter().find_map(|b| {
        let e = b.entries().iter().find(|e| e.path.to_ascii_lowercase().ends_with(&suffix))?;
        let img = zhbo_native::tga::decode(&b.read(e).ok()?).ok()?;
        Some(Rgba { width: img.width, height: img.height, pixels: img.rgba })
    })
}

/// `rect` (`Left`, `Top`, `Right`, `Bottom`; right and bottom exclusive, as Zero Hour sizes an
/// image by `Right - Left`) of `img`, or `None` if it does not fit.
pub(crate) fn crop(img: &Rgba, rect: [u32; 4]) -> Option<Rgba> {
    let [l, t, r, b] = rect;
    if r <= l || b <= t || r > img.width || b > img.height {
        return None;
    }
    let (w, h) = (r - l, b - t);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in t..b {
        let at = ((y * img.width + l) * 4) as usize;
        pixels.extend_from_slice(&img.pixels[at..at + (w * 4) as usize]);
    }
    Some(Rgba { width: w, height: h, pixels })
}

/// Write each faction's emblem (its side's `<Side>_Logo`) into `files`; the summary names the
/// ones the install lacks.
pub(crate) fn faction_emblems(zh_dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> String {
    let bigs = archives(zh_dir);
    let mut missing = Vec::new();
    // A survivor has no side, so no emblem (the Zombies weapons wear none).
    for f in FACTIONS.iter().filter(|f| !f.side.is_empty()) {
        let logo = format!("{}_Logo", f.side);
        match mapped_image(&bigs, &logo).and_then(|(texture, rect)| crop(&page(&bigs, &texture)?, rect)) {
            Some(img) => {
                files.insert(zhbo_native::emblem_path(f.letter), images::tga_bytes(&img));
            }
            None => missing.push(logo),
        }
    }
    if missing.is_empty() { format!("{} faction emblems", FACTIONS.iter().filter(|f| !f.side.is_empty()).count()) } else { format!("faction emblems missing: {}", missing.join(" ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_takes_the_rect_right_and_bottom_exclusive() {
        let img = Rgba { width: 4, height: 3, pixels: (0..48).collect() };
        let c = crop(&img, [1, 1, 3, 3]).unwrap();
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(c.pixels, [20, 21, 22, 23, 24, 25, 26, 27, 36, 37, 38, 39, 40, 41, 42, 43]);
        assert!(crop(&img, [0, 0, 5, 1]).is_none(), "past the page");
    }

    /// Needs the player's Zero Hour install: `ZHBO_ZH_DIR=... cargo test -p zhbo-assets -- --ignored`.
    #[test]
    #[ignore]
    fn the_installs_faction_logos_convert() {
        let zh = zhbo_requirements::check_requirements().zero_hour.expect("a Zero Hour install").path;
        let mut files = BTreeMap::new();
        let summary = faction_emblems(&zh, &mut files);
        assert_eq!(summary, "3 faction emblems");
        for f in FACTIONS.iter().filter(|f| !f.side.is_empty()) {
            let img = zhbo_native::tga::decode(&files[&zhbo_native::emblem_path(f.letter)]).unwrap();
            assert_eq!((img.width, img.height), (48, 48), "{}: the 48 x 48 logo", f.side);
            assert!(img.rgba.chunks(4).any(|p| p[3] > 128), "{}: not blank", f.side);
        }
    }
}
