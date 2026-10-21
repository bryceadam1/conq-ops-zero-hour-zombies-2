//! The converted BO2 art pages, read back out of a built `ConquerOps_BO2.big`.
//!
//! The archive the launcher generates carries one MappedImage INI and its TGA atlas pages; this
//! module parses the INI into sprite-name → (page, pixel rect) and decodes the pages it names. It
//! never reads the BO2 install: the archive is the player's own already-converted copy, so
//! screenshots work offline and without the game.

use std::collections::HashMap;
use std::path::Path;
use zhbo_formats::big::BigArchive;
use zhbo_formats::iwi::Rgba;

/// The MappedImage definitions `zhbo_assets::hud` writes into the archive.
pub(crate) const INI_PATH: &str = "Data\\INI\\MappedImages\\HandCreated\\ZHBO_BO2.ini";

/// One sprite: which texture page it lives on and its inclusive pixel rectangle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MappedImage {
    pub texture: String,
    pub rect: [u32; 4],
}

/// The archive's sprites: decoded pages plus each name's place on one.
pub(crate) struct Atlas {
    pages: HashMap<String, Rgba>,
    images: HashMap<String, (String, [u32; 4])>,
    /// Names the INI defined but whose rect did not fit the page it names. They are skipped (the
    /// game would sample garbage), so the contact sheet reports them.
    invalid: Vec<String>,
}

impl Atlas {
    /// An atlas with no art: every `Prim::Image` takes its fallback (the no-BO2-art paths).
    pub(crate) fn empty() -> Atlas {
        Atlas { pages: HashMap::new(), images: HashMap::new(), invalid: Vec::new() }
    }

    /// Read `ConquerOps_BO2.big` and decode every page its INI names.
    pub(crate) fn open(path: &Path) -> Result<Atlas, String> {
        let big = BigArchive::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let entry = big
            .find(INI_PATH)
            .ok_or_else(|| format!("{}: no {INI_PATH} (is this a ConquerOps_BO2.big from the launcher?)", path.display()))?;
        let text = String::from_utf8(big.read(entry).map_err(|e| e.to_string())?).map_err(|e| format!("{INI_PATH}: {e}"))?;
        let mut atlas = Atlas::empty();
        for (name, image) in parse_mapped_images(&text) {
            if !atlas.pages.contains_key(&image.texture) {
                let texture = format!("Art\\Textures\\{}", image.texture);
                let entry = big
                    .find(&texture)
                    .ok_or_else(|| format!("{}: {texture} is missing from the archive", path.display()))?;
                let bytes = big.read(entry).map_err(|e| format!("{texture}: {e}"))?;
                let page = decode_tga(&bytes).map_err(|e| format!("{texture}: {e}"))?;
                atlas.pages.insert(image.texture.clone(), page);
            }
            let page = &atlas.pages[&image.texture];
            // A rect outside its page would read garbage: skip it and let the fallback draw.
            if image.rect[2] >= page.width || image.rect[3] >= page.height || image.rect[0] > image.rect[2] || image.rect[1] > image.rect[3] {
                atlas.invalid.push(name);
                continue;
            }
            atlas.images.insert(name, (image.texture, image.rect));
        }
        Ok(atlas)
    }

    /// How many names and texture pages the atlas carries (the harness reports both).
    pub(crate) fn len(&self) -> usize {
        self.images.len()
    }

    pub(crate) fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The page and inclusive pixel rect for a sprite name, or `None` when the archive lacks it.
    pub(crate) fn find(&self, name: &str) -> Option<(&Rgba, [u32; 4])> {
        let (page, rect) = self.images.get(name)?;
        Some((self.pages.get(page)?, *rect))
    }

    /// Every sprite name with the page it names and its inclusive rect, sorted by page and then by
    /// position on that page (the order the packer laid them out; the contact sheet uses it).
    pub(crate) fn layout(&self) -> Vec<(&str, &str, [u32; 4])> {
        let mut all: Vec<(&str, &str, [u32; 4])> = self.images.iter().map(|(name, (page, rect))| (name.as_str(), page.as_str(), *rect)).collect();
        all.sort_by(|a, b| a.1.cmp(b.1).then(a.2[1].cmp(&b.2[1])).then(a.2[0].cmp(&b.2[0])).then(a.0.cmp(b.0)));
        all
    }

    /// The decoded pages, by texture file name (sorted, so page 10 follows page 9).
    pub(crate) fn pages(&self) -> Vec<(&str, &Rgba)> {
        let mut pages: Vec<(&str, &Rgba)> = self.pages.iter().map(|(name, page)| (name.as_str(), page)).collect();
        pages.sort_by(|a, b| a.0.cmp(b.0));
        pages
    }

    /// Names whose INI rect did not fit the page they name (skipped, so they draw fallbacks).
    pub(crate) fn invalid(&self) -> &[String] {
        &self.invalid
    }
}

/// Parse the generated MappedImage INI. It is flat and regular (`MappedImage <name>` … `End`), so a
/// line reader is enough; malformed blocks are skipped rather than aborting the whole set.
pub(crate) fn parse_mapped_images(text: &str) -> Vec<(String, MappedImage)> {
    let mut out = Vec::new();
    let mut current: Option<(String, MappedImage)> = None;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("MappedImage ") {
            current = Some((rest.trim().to_string(), MappedImage { texture: String::new(), rect: [0; 4] }));
        } else if line.eq_ignore_ascii_case("End") {
            if let Some((name, image)) = current.take() {
                if !name.is_empty() && !image.texture.is_empty() {
                    out.push((name, image));
                }
            }
        } else if let Some((key, value)) = line.split_once('=') {
            let Some((_, image)) = current.as_mut() else { continue };
            match key.trim() {
                "Texture" => image.texture = value.trim().to_string(),
                "Coords" => {
                    for part in value.split_whitespace() {
                        let Some((side, number)) = part.split_once(':') else { continue };
                        let Ok(n) = number.trim().parse::<u32>() else { continue };
                        match side.trim() {
                            "Left" => image.rect[0] = n,
                            "Top" => image.rect[1] = n,
                            "Right" => image.rect[2] = n,
                            "Bottom" => image.rect[3] = n,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Decode an uncompressed true-colour TGA (what `images::tga_bytes` writes: 32-bit BGRA, rows
/// bottom-up unless the top-origin bit says otherwise).
fn decode_tga(bytes: &[u8]) -> Result<Rgba, String> {
    if bytes.len() < 18 {
        return Err("truncated TGA header".into());
    }
    let id_len = bytes[0] as usize;
    let kind = bytes[2];
    let width = u16::from_le_bytes([bytes[12], bytes[13]]) as u32;
    let height = u16::from_le_bytes([bytes[14], bytes[15]]) as u32;
    let bpp = bytes[16];
    let descriptor = bytes[17];
    if kind != 2 {
        return Err(format!("unsupported TGA image type {kind} (expected uncompressed true colour)"));
    }
    let channels = match bpp {
        32 => 4,
        24 => 3,
        other => return Err(format!("unsupported TGA depth {other}")),
    };
    let start = 18 + id_len;
    let needed = width as usize * height as usize * channels;
    if width == 0 || height == 0 || bytes.len() < start + needed {
        return Err(format!("truncated TGA pixel data ({} < {})", bytes.len().saturating_sub(start), needed));
    }
    let top_down = descriptor & 0x20 != 0;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for row in 0..height {
        let src_row = if top_down { row } else { height - 1 - row };
        for col in 0..width {
            let s = start + (src_row * width + col) as usize * channels;
            let d = ((row * width + col) * 4) as usize;
            pixels[d] = bytes[s + 2];
            pixels[d + 1] = bytes[s + 1];
            pixels[d + 2] = bytes[s];
            pixels[d + 3] = if channels == 4 { bytes[s + 3] } else { 255 };
        }
    }
    Ok(Rgba { width, height, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; generated
MappedImage ZHBO_BO2_Digit0
  Texture = zhbo_bo2_hud_0.tga
  TextureWidth = 1024
  TextureHeight = 1024
  Coords = Left:497 Top:905 Right:528 Bottom:968
  Status = NONE
End

MappedImage ZHBO_BO2_RowFill
  Texture = zhbo_bo2_hud_1.tga
  TextureWidth = 1024
  TextureHeight = 1024
  Coords = Left:0 Top:0 Right:10 Bottom:9
  Status = NONE
End
";

    #[test]
    fn mapped_images_parse_name_texture_and_inclusive_coords() {
        let images = parse_mapped_images(SAMPLE);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].0, "ZHBO_BO2_Digit0");
        assert_eq!(images[0].1.texture, "zhbo_bo2_hud_0.tga");
        assert_eq!(images[0].1.rect, [497, 905, 528, 968]);
        assert_eq!(images[1].1.rect, [0, 0, 10, 9]);
    }

    #[test]
    fn tga_pages_round_trip_through_the_converter_writer() {
        // A 2x2 page with a distinct colour per row, through the same writer the archive uses.
        let img = Rgba { width: 2, height: 2, pixels: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0] };
        let decoded = decode_tga(&crate::images::tga_bytes(&img)).unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.pixels, img.pixels, "rows flip back to top-down, colours to RGBA");
    }

    #[test]
    fn an_empty_atlas_falls_through_for_every_name() {
        let atlas = Atlas::empty();
        assert_eq!(atlas.len(), 0);
        assert!(atlas.find("ZHBO_BO2_Digit0").is_none());
    }
}
