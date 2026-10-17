/*
** Conquer Ops - converter: BO2 pack-file access and the shared image shrinker.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::images;
use zhbo_formats::fastfile;
use zhbo_formats::ipak::{Ipak, IpakEntry};
use zhbo_formats::t6zone::{self, Zone};
use zhbo_formats::iwi::Rgba;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn load_zone(bo2_dir: &Path, name: &str) -> Result<Zone, String> {
    let path = bo2_dir.join("zone").join("all").join(format!("{name}.ff"));
    let file = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let zone = fastfile::read(&file).map_err(|e| format!("{name}: {e}"))?;
    t6zone::load(&zone).map_err(|e| format!("{name}: {e}"))
}

/// Image packs, opened on demand, with their entries grouped by name hash.
pub(crate) struct Paks {
    paks: Vec<(Ipak, BTreeMap<u32, Vec<IpakEntry>>)>,
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
            let pak = Ipak::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let groups = images::group_by_name(&pak);
            paks.push((pak, groups));
        }
        Ok(Paks { paks })
    }

    pub(crate) fn image(&self, hash: u32) -> Option<Rgba> {
        self.paks.iter().find_map(|(pak, groups)| groups.get(&hash).and_then(|parts| images::load(pak, parts).ok()))
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