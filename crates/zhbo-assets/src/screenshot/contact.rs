//! A labelled contact sheet of EVERY MappedImage in a built `ConquerOps_BO2.big`, plus the raw
//! atlas pages side by side.
//!
//! `zhbo-assets hud-sheet [out-dir] [--archive <big>] [--cols N]` is the art-QA view: each tile is
//! drawn through the same atlas lookup the game's `Prim::Image` uses (name → Texture + Coords →
//! pixels), so a rect that points at the wrong place on a page, names the wrong page, or has been
//! left behind by a stale archive is visible as split/mismatched art instead of hiding inside the
//! HUD. It reads only the locally converted archive; the output is build-directory data.

use super::atlas::Atlas;
use super::{raster, resolve_archive};
use crate::images;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use zhbo_core::hud::Prim;
use zhbo_formats::iwi::Rgba;

/// One tile: the art box above two lines of label.
const CELL_W: u32 = 260;
const CELL_H: u32 = 150;
const PAD: u32 = 6;
const ART_H: u32 = 112;

/// Render the archive's atlas pages and every sprite's view of them into `out`.
pub fn run(out: &Path, archive: Option<&Path>, cols: u32) -> Result<(), String> {
    let archive = resolve_archive(archive)?;
    let atlas = Atlas::open(&archive)?;
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("hud-sheet: {} ({} sprites on {} atlas pages)", archive.display(), atlas.len(), atlas.page_count());
    for name in atlas.invalid() {
        println!("  BAD RECT (outside its page, skipped): {name}");
    }

    write_pages(&atlas, out)?;
    write_sheet(&atlas, out, cols.max(1))?;
    Ok(())
}

/// Write each decoded page and a side-by-side composite of all of them.
fn write_pages(atlas: &Atlas, out: &Path) -> Result<(), String> {
    let pages = atlas.pages();
    for (i, (name, page)) in pages.iter().enumerate() {
        let file = out.join(format!("atlas-page-{i}.png"));
        images::write_png(&file, page)?;
        println!("wrote {} ({name}, {}x{})", file.display(), page.width, page.height);
    }
    // The pages next to each other, in the INI's page order: a sprite's neighbours on the shelf
    // become obvious against the tiles that draw from them.
    if pages.len() > 1 {
        let height = pages.iter().map(|(_, p)| p.height).max().unwrap_or(0);
        let width: u32 = pages.iter().map(|(_, p)| p.width).sum();
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let mut at_x = 0;
        for (_, page) in &pages {
            for y in 0..page.height {
                let s = (y * page.width * 4) as usize;
                let d = ((y * width + at_x) * 4) as usize;
                pixels[d..d + (page.width * 4) as usize].copy_from_slice(&page.pixels[s..s + (page.width * 4) as usize]);
            }
            at_x += page.width;
        }
        let file = out.join("atlas-pages.png");
        images::write_png(&file, &Rgba { width, height, pixels })?;
        println!("wrote {} ({width}x{height})", file.display());
    }
    Ok(())
}

/// Write the labelled grid: one tile per MappedImage, in pack order.
fn write_sheet(atlas: &Atlas, out: &Path, cols: u32) -> Result<(), String> {
    let entries = atlas.layout();
    let rows = (entries.len() as u32).div_ceil(cols).max(1);
    let (width, height) = (CELL_W * cols, CELL_H * rows);
    let mut prims = vec![Prim::Rect { x: 0.0, y: 0.0, w: width as f32, h: height as f32, argb: 0xFF10_1010 }];
    // A text index next to the PNG: page, rect, size and how much of the rect is opaque. A rect
    // that is (nearly) empty is a pointing error the sheet also shows as a blank tile.
    let mut index = String::new();
    for (i, (name, page, rect)) in entries.iter().enumerate() {
        let (cx, cy) = ((i as u32 % cols) * CELL_W, (i as u32 / cols) * CELL_H);
        let (rw, rh) = (rect[2] - rect[0] + 1, rect[3] - rect[1] + 1);
        prims.push(Prim::Rect { x: (cx + 2) as f32, y: (cy + 2) as f32, w: (CELL_W - 4) as f32, h: (CELL_H - 4) as f32, argb: 0xFF20_2020 });
        // Fit the sprite in the art box, aspect kept; nearest-neighbour, so pixel boundaries stay
        // readable and a rect that straddles two sprites cannot be hidden by smoothing.
        let art_w = CELL_W - 2 * PAD;
        let scale = (art_w as f32 / rw as f32).min(ART_H as f32 / rh as f32);
        let (dw, dh) = ((rw as f32 * scale).max(1.0), (rh as f32 * scale).max(1.0));
        let (dx, dy) = (cx as f32 + PAD as f32 + (art_w as f32 - dw) / 2.0, cy as f32 + PAD as f32 + (ART_H as f32 - dh) / 2.0);
        prims.push(Prim::Image { name: (*name).to_string(), x: dx, y: dy, w: dw, h: dh, argb: 0xFFFF_FFFF, fallback: Vec::new() });
        // Labels are clipped to the tile, so a long weapon name cannot spill into its neighbour.
        prims.push(Prim::ClipBegin { x: (cx + 2) as f32, y: (cy + CELL_H - 28) as f32, w: (CELL_W - 4) as f32, h: 26.0 });
        prims.push(Prim::Text { x: (cx + PAD) as f32, y: (cy + CELL_H - 28) as f32, text: (*name).to_string(), argb: 0xFFFF_FFFF, size: 7, align: 0 });
        prims.push(Prim::Text {
            x: (cx + PAD) as f32,
            y: (cy + CELL_H - 17) as f32,
            text: format!("{page} {rw}x{rh} @{},{}", rect[0], rect[1]),
            argb: 0xFFB0_B0B0,
            size: 7,
            align: 0,
        });
        prims.push(Prim::ClipEnd);

        let opaque = atlas.find(name).map_or(0.0, |(pg, r)| opaque_fraction(pg, r));
        index.push_str(&format!("{}  {page}  {}x{} @{},{}  opaque {:.0}%\n", name, rw, rh, rect[0], rect[1], opaque * 100.0));
        if opaque == 0.0 {
            println!("  EMPTY RECT (every pixel transparent): {name} {page} @{},{} {}x{}", rect[0], rect[1], rw, rh);
        }
    }
    if !atlas.invalid().is_empty() {
        index.push_str(&format!("\n{} rect(s) outside their page (skipped): {}\n", atlas.invalid().len(), atlas.invalid().join(", ")));
    }
    let mut missing = BTreeSet::new();
    let canvas = raster::render(&prims, atlas, width, height, &mut missing);
    if !missing.is_empty() {
        println!("  missing from the atlas (fallback drawn): {missing:?}");
    }
    let file = out.join("hud-sheet.png");
    images::write_png(&file, &Rgba { width, height, pixels: canvas.pixels().to_vec() })?;
    println!("wrote {} ({width}x{height}, {} tiles)", file.display(), entries.len());
    let txt = out.join("hud-sheet.txt");
    fs::write(&txt, index).map_err(|e| format!("{}: {e}", txt.display()))?;
    println!("wrote {}", txt.display());
    Ok(())
}

/// The share of a rect's pixels that carry any alpha (0 means the name points at empty atlas).
fn opaque_fraction(page: &Rgba, rect: [u32; 4]) -> f32 {
    let mut opaque = 0u32;
    let mut total = 0u32;
    for y in rect[1]..=rect[3].min(page.height.saturating_sub(1)) {
        for x in rect[0]..=rect[2].min(page.width.saturating_sub(1)) {
            let i = ((y * page.width + x) * 4) as usize;
            opaque += u32::from(page.pixels[i + 3] > 0);
            total += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        opaque as f32 / total as f32
    }
}
