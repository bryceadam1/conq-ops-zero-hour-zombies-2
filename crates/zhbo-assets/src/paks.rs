/*
** Conquer Ops - converter: BO2 pack-file access and the shared image shrinker.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::images;
use zhbo_formats::fastfile;
use zhbo_formats::ipak::{ImageKey, Ipak};
use zhbo_formats::t6zone::{self, Zone};
use zhbo_formats::iwi::Rgba;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn load_zone(bo2_dir: &Path, name: &str) -> Result<Zone, String> {
    t6zone::load_file(&t6zone::install_path(bo2_dir, name))
}

/// BO2's multiplayer weapon zones, loaded by `zhbo_formats::weapon::MP_POOL` (`patch_mp` over
/// `common_patch_mp` over `common_mp`) through [`load_zone`]: the one place the converter opens the
/// weapon pool and picks its weapon definitions.
pub(crate) struct MpPool {
    pub(crate) patch: Zone,
    pub(crate) common_patch: Zone,
    pub(crate) common: Zone,
}

impl MpPool {
    pub(crate) fn load(bo2_dir: &Path) -> Result<MpPool, String> {
        let [patch, common_patch, common] = zhbo_formats::weapon::MP_POOL.map(|name| load_zone(bo2_dir, name));
        Ok(MpPool { patch: patch?, common_patch: common_patch?, common: common? })
    }

    /// The zones, highest priority first (`MP_POOL` order).
    pub(crate) fn zones(&self) -> [&Zone; 3] {
        [&self.patch, &self.common_patch, &self.common]
    }

    /// BO2's multiplayer weapons and attachments as its asset pool resolves them over the weapon
    /// zones: the balance patches in `common_patch_mp` replace the launch definitions, as
    /// `DB_LinkXAssetEntry` (`FUN_00597000`) does.
    pub(crate) fn weapons(
        &self,
    ) -> Result<(Vec<zhbo_formats::weapon::Weapon>, Vec<zhbo_formats::weapon::Attachment>), String> {
        let pool = self.zones();
        let weapons = zhbo_formats::weapon::pooled(&pool).map_err(|e| e.to_string())?;
        let attachments = zhbo_formats::weapon::pooled_attachments(&pool).map_err(|e| e.to_string())?;
        Ok((weapons, attachments))
    }
}

/// The image packs, in file-name order.
pub(crate) struct Paks {
    paks: Vec<Ipak>,
    /// Streamed images by name, from the zones [`Paks::learn`] saw: a material that only
    /// references an image (`common_mp`'s optic reticles point at a 0 x 0 `..._reticle_noise`
    /// stub) finds the zone that loads it.
    names: BTreeMap<String, ImageKey>,
}

impl Paks {
    pub(crate) fn open(bo2_dir: &Path) -> Result<Paks, String> {
        let dir = bo2_dir.join("zone").join("all");
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "ipak"))
            .collect();
        names.sort();
        let mut paks = Vec::new();
        for p in names {
            paks.push(Ipak::open(&p).map_err(|e| format!("{}: {e}", p.display()))?);
        }
        Ok(Paks { paks, names: BTreeMap::new() })
    }

    /// The pixels of the image with this key (`xmodel::Image::key`).
    pub(crate) fn image(&self, key: ImageKey) -> Option<Rgba> {
        self.paks.iter().find_map(|pak| pak.entry(key).and_then(|e| images::load(pak, e).ok()))
    }

    /// Remember the streamed images `zone` loads (top-level and nested), by name; the first zone
    /// to name one wins.
    pub(crate) fn learn(&mut self, zone: &Zone) {
        let Some(kind) = fastfile::asset_type_id("image") else { return };
        for a in zone.assets.iter().chain(&zone.linked).filter(|a| a.kind == kind) {
            let img = zhbo_formats::xmodel::read_image(zone, a.addr);
            if img.stream_hash != 0 {
                let key = img.key();
                self.names.entry(img.name).or_insert(key);
            }
        }
    }

    /// `img`'s pixels: its own streamed data, or for a reference stub (no stream hash) the data of
    /// the image of that name a learned zone loads (by its {data hash, name hash} key).
    pub(crate) fn resolve(&self, img: &zhbo_formats::xmodel::Image) -> Option<Rgba> {
        let key = if img.stream_hash != 0 { img.key() } else { *self.names.get(&img.name)? };
        self.image(key)
    }
}

/// Halve an image until it fits `max` (box filter).
pub(crate) fn shrink(mut img: Rgba, max: u32) -> Rgba {
    while img.width > max || img.height > max {
        let (w, h) = ((img.width / 2).max(1), (img.height / 2).max(1));
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (2 * x + dx).min(img.width - 1);
                        let sy = (2 * y + dy).min(img.height - 1);
                        sum += img.pixels[((sy * img.width + sx) * 4 + c) as usize] as u32;
                    }
                    out[((y * w + x) * 4 + c) as usize] = (sum / 4) as u8;
                }
            }
        }
        img = Rgba { width: w, height: h, pixels: out };
    }
    img
}
#[cfg(test)]
mod tests {
    use super::*;
    use zhbo_formats::ipak::r_hash_string;

    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn an_images_pack_entry_is_its_stream_hash_and_name_hash() {
        let bo2 = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        let pak = |name: &str| Ipak::open(bo2.join("zone").join("all").join(name)).unwrap();
        // The XPR-50 scope overlay: the GfxImage's stream hash (+40) is the entry's first field,
        // R_HashString of its name (= GfxImage::hash, +76) the second.
        let zone = load_zone(&bo2, "patch_mp").unwrap();
        let image = zhbo_formats::xmodel::find_material(&zone, "scope_overlay_xpr50")
            .and_then(|m| m.textures.into_iter().find_map(|t| t.image))
            .expect("the XPR-50 overlay material in patch_mp");
        assert_eq!(image.name, "scope_overlay_xpr50_1024");
        assert_eq!(image.hash, r_hash_string(&image.name));
        assert_eq!(image.key(), ImageKey { data_hash: 0x1c99_c1b0, name_hash: 0x9c47_55f7 });
        assert!(pak("patch_mp.ipak").entry(image.key()).is_some());
        // base.ipak holds an older revision under the same name hash and another data hash; only
        // the pair names the patched one.
        let base = pak("base.ipak");
        assert!(base.entry(image.key()).is_none());
        assert!(base.entries().iter().any(|e| e.name_hash == image.hash && e.data_hash == 0x03e7_5800));
        let paks = Paks::open(&bo2).unwrap();
        let rgba = paks.image(image.key()).expect("the overlay decodes");
        assert_eq!((rgba.width, rgba.height), (1024, 1024));
    }
}
