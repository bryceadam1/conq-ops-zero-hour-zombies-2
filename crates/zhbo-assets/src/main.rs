//! zhbo-assets - inspect and export Black Ops II content from the player's own install.
//!
//! ```text
//! zhbo-assets banks                                   list sound banks
//! zhbo-assets list <bank>                             sound entries: id, duration, channels, rate
//! zhbo-assets export <bank> <out-dir> [--min S] [--max S]
//!                                                     decode sound entries to WAV
//! zhbo-assets paks                                    list image packs
//! zhbo-assets images <pak> [--formats 1,13] [--max-size N]
//!                                                     image index: hash, format, size
//! zhbo-assets sheets <pak> <out-dir> [--formats ..] [--max-size N] [--hashes a,b] [--per-sheet 64] [--cell 96]
//!                                                     numbered contact sheets + index, to identify art by eye
//! zhbo-assets image <pak> <hash> <out.png>            export one image
//! zhbo-assets zones                                   list fastfiles
//! zhbo-assets zone <ff>                               decrypt a fastfile; asset counts by type
//! zhbo-assets find <text> [<ff> ...]                  names containing <text>, in every (or the given) zone
//! zhbo-assets model <ff> <name> [--png out] [--bones] read a model; stats, preview image, skeleton
//! zhbo-assets dump <ff> <text> [before] [after]        hex dump around a string in a zone
//! zhbo-assets pose <out.png> <frame> <ff>:<anim> <ff>:<model>[@<tag>] ...
//!                                                     render models posed by an animation frame
//! zhbo-assets hammer <out.png> <clip> <frame>         render the converted hammer viewmodel
//!                                                     (clip: ZHBO_VMH_PUL/IDL/SWG/PUT)
//! ```
//! `<bank>`/`<pak>`/`<ff>` are file names inside the BO2 install (`sound\`, `zone\all\`) or full paths.
//! Everything exported is the player's own game data: keep it local, never redistribute it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use zhbo_assets::{images, sounds};

mod preview;
use zhbo_formats::fastfile;
use zhbo_formats::ipak::Ipak;
use zhbo_formats::iwi::Format;
use zhbo_formats::sab::{SoundBank, FORMAT_FLAC, FORMAT_PCM16};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("zhbo-assets: {e}");
            ExitCode::FAILURE
        }
    }
}

fn bo2_dir() -> Result<PathBuf, String> {
    Ok(zhbo_requirements::check_requirements().black_ops2?.path)
}

fn resolve(name: &str, sub: &[&str]) -> Result<PathBuf, String> {
    if Path::new(name).is_file() {
        return Ok(PathBuf::from(name));
    }
    let mut p = bo2_dir()?;
    for s in sub {
        p = p.join(s);
    }
    Ok(p.join(name))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn list_dir(dir: PathBuf, ext: &[&str]) -> Result<(), String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| ext.iter().any(|x| n.ends_with(x)))
        .collect();
    names.sort();
    names.iter().for_each(|n| println!("{n}"));
    Ok(())
}

/// Image filter from `--formats` (IWi codes) and `--max-size` (largest side).
struct Filter {
    formats: Option<Vec<u8>>,
    max_size: u16,
    hashes: Option<Vec<u32>>,
}

impl Filter {
    fn from_args(args: &[String]) -> Self {
        Self {
            formats: flag(args, "--formats").map(|f| f.split(',').filter_map(|c| c.trim().parse().ok()).collect()),
            max_size: flag(args, "--max-size").and_then(|v| v.parse().ok()).unwrap_or(u16::MAX),
            hashes: flag(args, "--hashes")
                .map(|h| h.split(',').filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok()).collect()),
        }
    }

    fn wants_hash(&self, hash: u32) -> bool {
        self.hashes.as_ref().map_or(true, |h| h.contains(&hash))
    }

    fn accepts(&self, h: &zhbo_formats::iwi::Header) -> bool {
        let code = match h.format {
            Format::Argb32 => 1,
            Format::Rgb24 => 2,
            Format::Dxt1 => 11,
            Format::Dxt3 => 12,
            Format::Dxt5 => 13,
            Format::Bc5 => 14,
            Format::Other(c) => c,
        };
        self.formats.as_ref().map_or(true, |f| f.contains(&code)) && h.width.max(h.height) <= self.max_size
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str).unwrap_or("") {
        "banks" => list_dir(bo2_dir()?.join("sound"), &[".sabs", ".sabl"]),
        "paks" => list_dir(bo2_dir()?.join("zone").join("all"), &[".ipak"]),
        "zones" => list_dir(bo2_dir()?.join("zone").join("all"), &[".ff"]),
        "zone" => {
            let zone = open_zone(args.get(1).ok_or("zone <ff>")?)?;
            let index = zone.index().map_err(|e| e.to_string())?;
            println!("zone {}: {} bytes, {} script strings, {} assets", zone.name, zone.data.len(), index.script_strings.len(), index.assets.len());
            let mut counts = std::collections::BTreeMap::new();
            for &(t, _) in &index.assets {
                *counts.entry(t).or_insert(0usize) += 1;
            }
            for (t, n) in counts {
                println!("{:>6}  {}", n, fastfile::asset_type_name(t));
            }
            if args.iter().any(|a| a == "--order") {
                // Runs of asset types in stream order: what a parser must walk to reach an asset.
                println!("order:");
                let mut runs: Vec<(u32, usize, usize)> = Vec::new(); // (type, first index, count)
                for (i, &(t, _)) in index.assets.iter().enumerate() {
                    match runs.last_mut() {
                        Some(r) if r.0 == t => r.2 += 1,
                        _ => runs.push((t, i, 1)),
                    }
                }
                for (t, first, n) in runs {
                    println!("{:>6} {:>6}  {}", first, n, fastfile::asset_type_name(t));
                }
            }
            Ok(())
        }
        "model" => {
            let usage = "model <ff> <name> [--png out.png]";
            let zone = load_full_zone(args.get(1).ok_or(usage)?)?;
            let name = args.get(2).ok_or(usage)?;
            let model = zhbo_formats::xmodel::find(&zone, name).map_err(|e| e.to_string())?;
            println!("{}: {} bones, {} surfaces, {} lods, bounds {:?}..{:?}", model.name, model.bones.len(), model.surfaces.len(), model.lods.len(), model.mins, model.maxs);
            for (i, l) in model.lods.iter().enumerate() {
                let surfs = &model.surfaces[l.first_surface..l.first_surface + l.surface_count];
                let tris: usize = surfs.iter().map(|s| s.triangles.len()).sum();
                let verts: usize = surfs.iter().map(|s| s.vertices.len()).sum();
                println!("  lod{i}: dist {:.0}, {} surfaces, {verts} vertices, {tris} triangles", l.distance, l.surface_count);
            }
            let names: Vec<&str> = model.bones.iter().take(12).map(|b| b.name.as_str()).collect();
            println!("  bones: {} ...", names.join(" "));
            let paks = image_paks()?;
            for (i, m) in model.materials.iter().enumerate().take(model.lods[0].surface_count) {
                match m {
                    None => println!("  surface {i}: material shared or unreadable"),
                    Some(m) => {
                        println!("  surface {i}: {}", m.name);
                        for t in &m.textures {
                            match &t.image {
                                Some(img) => {
                                    let find = |h: u32| paks.iter().find(|(_, _, g)| g.contains_key(&h)).map(|(n, _, _)| n.as_str()).unwrap_or("-");
                                    println!(
                                        "      semantic {:>2}: {} {}x{} hash {:#010x} in {}, stream hash {:#010x} ({} parts) in {}",
                                        t.semantic, img.name, img.width, img.height, img.hash, find(img.hash), img.stream_hash, img.streamed_parts, find(img.stream_hash)
                                    );
                                }
                                None => println!("      semantic {:>2}: (shared image)", t.semantic),
                            }
                        }
                    }
                }
            }
            if args.iter().any(|a| a == "--bones") {
                for (i, b) in model.bones.iter().enumerate() {
                    println!("  bone {i:>3} {:<24} parent {:>4} pos {:>7.2?} rot {:>6.3?} local {:>7.2?}", b.name, b.parent.map_or(-1, |p| p as i64), b.position, b.rotation, b.local_position);
                }
                for (i, s) in model.surfaces.iter().enumerate().take(model.lods[0].surface_count) {
                    let mut bones: Vec<u16> = s.vertices.iter().flat_map(|v| v.weights.iter().map(|w| w.0)).collect();
                    bones.sort_unstable();
                    bones.dedup();
                    let c = s.vertices.iter().fold([0.0f32; 3], |a, v| [a[0] + v.position[0], a[1] + v.position[1], a[2] + v.position[2]]);
                    let n = s.vertices.len().max(1) as f32;
                    println!("  surface {i}: {} verts, bones {bones:?}, centroid {:>7.2?}", s.vertices.len(), [c[0] / n, c[1] / n, c[2] / n]);
                }
            }
            if let Some(out) = flag(args, "--png") {
                let textures = color_maps(&model, &paks);
                preview::render(&model, &textures, Path::new(out)).map_err(|e| e.to_string())?;
                println!("wrote {out}");
            }
            Ok(())
        }
        "pose" => {
            // pose <out.png> <frame|fraction> <anim-ff>:<anim> <ff>:<model>[@<parent-bone>] ...
            let usage = "pose <out.png> <frame> <ff>:<anim> <ff>:<model>[@<tag>] ...";
            let out = args.get(1).ok_or(usage)?;
            let frame: f32 = args.get(2).and_then(|v| v.parse().ok()).ok_or(usage)?;
            let split = |s: &str| -> Result<(String, String), String> {
                s.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())).ok_or_else(|| usage.to_string())
            };
            let mut zones: ZoneCache = Default::default();
            let (anim_ff, anim_name) = split(args.get(3).ok_or(usage)?)?;
            load_zone(&mut zones, &anim_ff)?;
            let anim = zhbo_formats::xanim::find(&zones[&anim_ff], &anim_name).map_err(|e| e.to_string())?;
            println!("{}: {} frames at {} fps, {} bones", anim.name, anim.num_frames, anim.frame_rate, anim.bones.len());
            let paks = image_paks()?;
            let mut models = Vec::new();
            for spec in &args[4..] {
                let (ff, rest) = split(spec)?;
                let (name, tag) = match rest.split_once('@') {
                    Some((n, t)) => (n.to_string(), Some(t.to_string())),
                    None => (rest, None),
                };
                load_zone(&mut zones, &ff)?;
                let model = zhbo_formats::xmodel::find(&zones[&ff], &name).map_err(|e| e.to_string())?;
                let textures = color_maps(&model, &paks);
                models.push((model, textures, tag));
            }
            let refs: Vec<(&zhbo_formats::xmodel::Model, &preview::Textures, Option<&str>)> =
                models.iter().map(|(m, x, t)| (m, x, t.as_deref())).collect();
            let meshes = preview::pose(&refs, &anim, frame);
            preview::render_meshes(&meshes, Path::new(out)).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        "walk" => {
            // Fully load zones (every asset, every pointer) and report.
            let names: Vec<String> = if args.len() > 1 {
                args[1..].to_vec()
            } else {
                let mut v: Vec<String> = fs::read_dir(bo2_dir()?.join("zone").join("all"))
                    .map_err(|e| e.to_string())?
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(".ff"))
                    .collect();
                v.sort();
                v
            };
            let mut failed = 0;
            for name in names {
                let zone = open_zone(&name)?;
                match zhbo_formats::t6zone::load(&zone) {
                    Ok(z) => println!("{name:<36} ok   {:>6} assets, {} bytes left over", z.assets.len(), z.trailing_bytes),
                    Err(e) => {
                        failed += 1;
                        println!("{name:<36} FAIL {e}");
                    }
                }
            }
            if failed > 0 {
                return Err(format!("{failed} zones failed"));
            }
            Ok(())
        }
        "assets" => {
            // assets <ff> [type]: names of every asset (of a type, e.g. xanim) in a zone.
            let zone = load_full_zone(args.get(1).ok_or("assets <ff> [type]")?)?;
            let want = args.get(2).map(String::as_str);
            for a in &zone.assets {
                let kind = fastfile::asset_type_name(a.kind);
                if want.is_none_or(|w| w == kind) {
                    println!("{kind:<14} {}", zone.asset_name(a).unwrap_or_default());
                }
            }
            Ok(())
        }
        "material" => {
            // material <ff> <out dir> <name>...: a material's images as PNGs (HUD art is materials).
            let usage = "material <ff> <out dir> <name>...";
            let zone = load_full_zone(args.get(1).ok_or(usage)?)?;
            let out = PathBuf::from(args.get(2).ok_or(usage)?);
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let paks = image_paks()?;
            for name in &args[3..] {
                let Some(m) = zhbo_formats::xmodel::find_material(&zone, name) else {
                    println!("{name}: not in this zone");
                    continue;
                };
                for (i, t) in m.textures.iter().enumerate() {
                    let Some(img) = &t.image else { continue };
                    let found = paks.iter().find_map(|(n, pak, g)| g.get(&img.stream_hash).map(|parts| (n, images::load(pak, parts))));
                    match found {
                        Some((pak, Ok(rgba))) => {
                            let file = out.join(format!("{name}_{i}.png"));
                            images::write_png(&file, &rgba)?;
                            // Also over dark grey, since most HUD art is white on clear.
                            let mut dark = rgba.clone();
                            for px in dark.pixels.chunks_exact_mut(4) {
                                let a = px[3] as u32;
                                for c in &mut px[..3] {
                                    *c = ((*c as u32 * a + 40 * (255 - a)) / 255) as u8;
                                }
                                px[3] = 255;
                            }
                            images::write_png(&out.join(format!("{name}_{i}_dark.png")), &dark)?;
                            println!("{name} [{i}] semantic {} {} {}x{} {pak} {:#010x}", t.semantic, img.name, rgba.width, rgba.height, img.stream_hash);
                        }
                        Some((_, Err(e))) => println!("{name} [{i}] {}: {e}", img.name),
                        None => println!("{name} [{i}] {} {}x{}: not in the packs (hash {:#010x})", img.name, img.width, img.height, img.stream_hash),
                    }
                }
            }
            Ok(())
        }
        "weapons" => {
            // weapons [name filter]: BO2 multiplayer weapon definitions (common_mp).
            use zhbo_formats::weapon;
            let zone = load_full_zone("common_mp.ff")?;
            let filter = args.get(1).map(String::as_str);
            for w in weapon::all(&zone).map_err(|e| e.to_string())? {
                if filter.is_some_and(|f| !w.name.contains(f)) {
                    continue;
                }
                println!(
                    "{:<24} {:<18} type {} class {:>2} inv {} fire {} clip {:>3} start {:<3} max {:<3} rpm {:>5.0} dmg {:?}@{:?} reload {:.2}/{:.2} add {:.2} ads {:.2} raise {:.2} drop {:.2} spread hip {:?} ads {:.2} move {:.2} pellets {} fuse {:.2} expl {}/{}-{} vm {:?} anims {}",
                    w.name,
                    w.display_name,
                    w.weap_type,
                    w.class,
                    w.inventory,
                    w.fire_type,
                    w.clip_size,
                    w.start_ammo,
                    w.max_ammo,
                    if w.fire_time > 0.0 { 60.0 / w.fire_time } else { 0.0 },
                    &w.damage[..3],
                    &w.damage_range[..3],
                    w.reload_time,
                    w.reload_empty_time,
                    w.reload_add_time,
                    w.ads_in_time,
                    w.raise_time,
                    w.drop_time,
                    w.hip_spread_stand,
                    w.ads_spread,
                    w.move_speed_scale,
                    w.shot_count,
                    w.fuse_time,
                    w.explosion_radius,
                    w.explosion_inner_damage,
                    w.explosion_outer_damage,
                    w.view_model,
                    w.anims.iter().filter(|a| a.is_some()).count(),
                );
                if args.iter().any(|a| a == "--more") {
                    println!("    attach {:?} base parts {:?} uniques {}", w.attach_view_models, w.base_view_parts(), w.attachment_uniques.len());
                    for u in w.attachment_uniques.iter().take(6) {
                        println!("      unique {} kind {} vm {:?} tag {:?}", u.name, u.kind, u.view_model, u.view_model_tag);
                    }
                    println!("    sounds fire {:?} plr {:?} icon {:?} dpad {:?} world {:?} projectile {:?} impact {} icon hash {:?}", w.fire_sound, w.fire_sound_player, w.hud_icon, w.dpad_icon, w.world_model, w.projectile_model, w.projectile_impact_explode, w.hud_icon_hash);
                    println!("    kick ads {:?} hip {:?} mult {:?}", w.ads_view_kick, w.hip_view_kick, w.location_multipliers.iter().take(7).collect::<Vec<_>>());
                    for (i, a) in w.anims.iter().enumerate() {
                        if let Some(a) = a {
                            print!(" {i:#x}={a}");
                        }
                    }
                    println!();
                }
            }
            Ok(())
        }
        "attachments" => {
            // attachments [weapon filter]: BO2's attachments, and which ones each weapon allows.
            use zhbo_formats::weapon;
            let zone = load_full_zone("common_mp.ff")?;
            let all = weapon::attachments(&zone).map_err(|e| e.to_string())?;
            if args.iter().any(|a| a == "--raw") {
                use zhbo_formats::t6zone::layouts::WeaponAttachment as WA;
                let kind = zhbo_formats::fastfile::asset_type_id("attachment").unwrap();
                for a in zone.assets.iter().filter(|a| a.kind == kind) {
                    let at = a.addr;
                    println!(
                        "{:<20} perks {:#x} {:#x} recoil {} {} kickc {} custom {} {} {} {} {} {} pen {} hipidle {}",
                        zone.str_ptr(at),
                        zone.u32(at + WA::perks),
                        zone.u32(at + WA::perks + 4),
                        zone.f32(at + WA::fAdsRecoilReductionRate),
                        zone.f32(at + WA::fAdsRecoilReductionLimit),
                        zone.f32(at + WA::fAdsViewKickCenterSpeedScale),
                        zone.f32(at + WA::customFloat0),
                        zone.f32(at + WA::customFloat1),
                        zone.f32(at + WA::customFloat2),
                        zone.u32(at + WA::customBool0),
                        zone.u32(at + WA::customBool1),
                        zone.u32(at + WA::customBool2),
                        zone.u32(at + WA::penetrateType),
                        zone.f32(at + WA::fHipIdleAmount),
                    );
                }
                return Ok(());
            }
            for a in &all {
                println!("{a:?}");
            }
            if let Some(f) = args.get(1) {
                for w in weapon::all(&zone).map_err(|e| e.to_string())?.into_iter().filter(|w| w.name.contains(f.as_str())) {
                    let names: Vec<&str> = all.iter().filter(|a| w.allowed_attachments & (1 << a.kind) != 0).map(|a| a.name.as_str()).collect();
                    println!("{} allows {:#x}: {names:?}\n  variants {:?} hide {:?} alt {:?}", w.name, w.allowed_attachments, w.attachment_variants, w.hide_tags, w.alt_weapon);
                    for u in &w.attachment_uniques {
                        println!("  unique {} kind {} combined {:#x} vm {:?} world {:?} tag {:?}/{:?} noclip {} anims {} off {:?} rot {:?} woff {:?} hide {:?} snd {:?} alt {:?} dw {:?}", u.name, u.kind, u.combined, u.view_model, u.world_model, u.view_model_tag, u.world_model_tag, u.disable_base_clip, u.anims.iter().filter(|a| a.is_some()).count(), u.view_offset, u.view_rotation, u.world_offset, u.hide_tags, u.fire_sound_player, u.alt_weapon, u.dual_wield_weapon);
                    }
                }
            }
            Ok(())
        }
        "fx" => {
            // fx <weapon>: the BO2 effect a weapon's projectile makes when it goes off.
            use zhbo_formats::{fx, weapon};
            let zone = load_full_zone("common_mp.ff")?;
            let name = args.get(1).ok_or("fx <weapon>")?;
            if name == "--list" {
                let kind = zhbo_formats::fastfile::asset_type_id("fx").unwrap();
                for a in zone.assets.iter().filter(|a| a.kind == kind) {
                    let e = fx::read(&zone, a.addr);
                    println!("{} ({} elems, life {})", e.name, e.elems.len(), e.life_msec);
                }
                return Ok(());
            }
            if name == "--impacts" {
                for t in 0..20u32 {
                    let n = fx::impact(&zone, t, fx::surface::DIRT).map(|e| e.name).unwrap_or_default();
                    println!("impact type {t}: {n}");
                }
                for w in weapon::all(&zone).map_err(|e| e.to_string())?.iter().filter(|w| ["frag_grenade_mp", "claymore_mp", "gl_an94_mp", "m32_mp", "satchel_charge_mp", "usrpg_mp", "an94_mp", "fnp45_mp", "bouncingbetty_mp", "sticky_grenade_mp"].contains(&w.name.as_str())) {
                    println!("{} impact type {}", w.name, w.impact_type);
                }
                return Ok(());
            }
            if let Some(e) = name.strip_prefix("effect:").and_then(|n| fx::find(&zone, n)) {
                println!("{e:#?}");
                return Ok(());
            }
            let w = weapon::all(&zone).map_err(|e| e.to_string())?.into_iter().find(|w| &w.name == name).ok_or("no such weapon")?;
            fn dump(e: &fx::Effect, indent: &str) {
                println!("{indent}effect {} life {} looping {} elems {}", e.name, e.life_msec, e.looping_life_msec, e.elems.len());
                for el in &e.elems {
                    let r = |r: fx::Range| format!("{}+{}", r.base, r.amplitude);
                    println!(
                        "{indent}  kind {:>2} loop {} count {} delay {} life {} radius {} grav {} vel {:?}/{:?} wvel {:?}/{:?} atlas {:?} mats {:?} models {:?}",
                        el.kind, el.looping, r(el.count), r(el.delay_msec), r(el.life_msec), r(el.offset_radius), r(el.gravity), el.velocity.0, el.velocity.1, el.world_velocity.0, el.world_velocity.1, el.atlas,
                        el.materials.iter().map(|m| m.color_map().map(|i| i.name.clone()).unwrap_or_else(|| m.name.clone())).collect::<Vec<_>>(), el.models
                    );
                    if let (Some(a), Some(b)) = (el.vis.first(), el.vis.last()) {
                        println!("{indent}      vis {} samples: {:?} size {:?} -> {:?} size {:?}", el.vis.len(), a.color, a.size, b.color, b.size);
                    }
                    for c in &el.children {
                        dump(c, &format!("{indent}    "));
                    }
                }
            }
            match &w.explosion_effect {
                Some(e) => dump(e, ""),
                None => println!("{name} (impact type {}) has no explosion effect", w.impact_type),
            }
            Ok(())
        }
        "strings" => {
            // strings <language zone, e.g. en_common_mp.ff> [filter]: localized strings.
            let path = resolve(args.get(1).ok_or("strings <zone> [filter]")?, &["zone", "english"])?;
            let file = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let zone = zhbo_formats::t6zone::load(&fastfile::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let mut all: Vec<_> = zhbo_formats::localize::entries(&zone).map_err(|e| e.to_string())?.into_iter().collect();
            all.sort();
            for (k, v) in all.iter().filter(|(k, _)| args.get(2).is_none_or(|f| k.contains(f.as_str()))) {
                println!("{k} = {v}");
            }
            Ok(())
        }
        "probes" => {
            // probes <ff>: a map's reflection probes (GfxWorld::draw, at +396) and their images.
            let zone = load_full_zone(args.get(1).ok_or("probes <ff>")?)?;
            let world = zone.assets.iter().find(|a| fastfile::asset_type_name(a.kind) == "gfxworld").ok_or("no gfxworld")?;
            let draw = world.addr + 396;
            let (count, probes) = (zone.u32(draw), zone.u32(draw + 4));
            let paks = image_paks()?;
            println!("{count} probes");
            for i in 0..count.min(64) {
                let p = probes + 76 * i;
                let origin = [zone.f32(p), zone.f32(p + 4), zone.f32(p + 8)];
                let img = zone.u32(p + 60);
                if img == 0 {
                    continue;
                }
                let im = zhbo_formats::xmodel::read_image(&zone, img);
                let head = paks.iter().find_map(|(n, pak, g)| g.get(&im.stream_hash).and_then(|parts| images::header(pak, parts)).map(|h| (n.clone(), h)));
                println!("{i:3} {origin:?} {} {}x{} stream {:#x} parts {} -> {head:?}", im.name, im.width, im.height, im.stream_hash, im.streamed_parts);
                let def = zone.u32(img);
                if def != 0 {
                    println!("    loadDef levels {} flags {:#x} format {} size {} mapType {} depth {}", zone.u8(def), zone.u8(def + 1), zone.u32(def + 4), zone.u32(def + 8), zone.u8(img + 16), zone.u16(img + 24));
                }
            }
            Ok(())
        }
        "aliases" => {
            // aliases <ff> [text]: BO2 sound aliases in a zone's sound banks.
            let zone = load_full_zone(args.get(1).ok_or("aliases <ff> [text]")?)?;
            let filter = args.get(2).map(|f| f.to_ascii_lowercase());
            for bank in zhbo_formats::sndalias::banks(&zone).map_err(|e| e.to_string())? {
                println!("bank {}: {} aliases, {} loaded sounds, {} bytes of loaded data", bank.name, bank.aliases.len(), bank.loaded.len(), bank.loaded_data.len());
                if args.iter().any(|a| a == "--loaded") {
                    for e in bank.loaded.iter().take(5) {
                        println!("  loaded {:#010x} size {} offset {} frames {} rate {} ch {} fmt {}", e.id, e.size, e.offset, e.frame_count, e.rate_index, e.channels, e.format);
                    }
                }
                for a in &bank.aliases {
                    if filter.as_ref().is_some_and(|f| !a.name.to_ascii_lowercase().contains(f.as_str())) {
                        continue;
                    }
                    let v = &a.variants;
                    let first = v.first();
                    println!(
                        "  {:<48} {} variant(s), asset {:#010x} vol {:.2}..{:.2} {}",
                        a.name,
                        v.len(),
                        first.map_or(0, |v| v.asset_id),
                        first.map_or(0.0, |v| v.volume.0),
                        first.map_or(0.0, |v| v.volume.1),
                        first.map_or("", |v| v.file.as_str())
                    );
                }
            }
            Ok(())
        }
        "anim" => {
            // anim <ff> <name>: tracks per bone.
            let zone = load_full_zone(args.get(1).ok_or("anim <ff> <name>")?)?;
            let anim = zhbo_formats::xanim::find(&zone, args.get(2).ok_or("anim <ff> <name>")?).map_err(|e| e.to_string())?;
            println!("{}: {} frames at {} fps, loop {}, {} bones", anim.name, anim.num_frames, anim.frame_rate, anim.looping, anim.bones.len());
            for b in &anim.bones {
                let r = b.rotation.as_ref().map_or("-".to_string(), |t| format!("{} keys, first {:.3?}", t.len(), t[0].1));
                let t = b.translation.as_ref().map_or("-".to_string(), |t| {
                    let lo = (0..3).map(|k| t.iter().map(|x| x.1[k]).fold(f32::MAX, f32::min)).collect::<Vec<_>>();
                    let hi = (0..3).map(|k| t.iter().map(|x| x.1[k]).fold(f32::MIN, f32::max)).collect::<Vec<_>>();
                    format!("{} keys, first {:.2?}, range {:.2?}..{:.2?}", t.len(), t[0].1, lo, hi)
                });
                println!("  {:<24} rot {r:<44} pos {t}", b.name);
            }
            for n in &anim.notifies {
                println!("  notify {:.3} {}", n.time, n.name);
            }
            Ok(())
        }
        "hammer" => {
            let usage = "hammer <out.png> <clip> <frame>";
            let out = args.get(1).ok_or(usage)?;
            let clip = args.get(2).ok_or(usage)?;
            let frame: usize = args.get(3).and_then(|v| v.parse().ok()).ok_or(usage)?;
            let (rig, meshes, channels) = zhbo_assets::generals::hammer_preview(&bo2_dir()?, clip)?;
            let posed = preview::pose_w3d(&rig.pivots, &meshes, &channels, frame);
            preview::render_meshes(&posed, Path::new(out)).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        "posew3d" => {
            // posew3d <out.png> <frame> <ff>:<anim> <ff>:<base>[ <ff>:<model>@<tag> ...]: render through
            // the W3D conversion (rig, skin, animation) as the engine would draw it.
            let usage = "posew3d <out.png> <frame> <ff>:<anim> <ff>:<base> [<ff>:<model>@<tag> ...]";
            let out = args.get(1).ok_or(usage)?;
            let frame: usize = args.get(2).and_then(|v| v.parse().ok()).ok_or(usage)?;
            let split = |s: &str| -> Result<(String, String), String> {
                s.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())).ok_or_else(|| usage.to_string())
            };
            let mut zones: ZoneCache = Default::default();
            let (anim_ff, anim_name) = split(args.get(3).ok_or(usage)?)?;
            load_zone(&mut zones, &anim_ff)?;
            let anim = zhbo_formats::xanim::find(&zones[&anim_ff], &anim_name).map_err(|e| e.to_string())?;
            let mut models = Vec::new();
            for spec in &args[4..] {
                let (ff, rest) = split(spec)?;
                let (name, tag) = match rest.split_once('@') {
                    Some((n, t)) => (n.to_string(), Some(t.to_string())),
                    None => (rest, None),
                };
                load_zone(&mut zones, &ff)?;
                models.push((zhbo_formats::xmodel::find(&zones[&ff], &name).map_err(|e| e.to_string())?, tag));
            }
            let attachments: Vec<(&zhbo_formats::xmodel::Model, &str)> =
                models[1..].iter().map(|(m, t)| (m, t.as_deref().unwrap_or(""))).collect();
            let rig = zhbo_assets::character::Rig::new(&models[0].0, &attachments);
            let mut meshes = Vec::new();
            for (m, _) in &models {
                meshes.extend(zhbo_assets::character::skin(&rig, m, "X", &[]));
            }
            let (_, channels) = zhbo_assets::character::animate(&rig, &anim);
            let posed = preview::pose_w3d(&rig.pivots, &meshes, &channels, frame);
            preview::render_meshes(&posed, Path::new(out)).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        "dump" => {
            // Hex dump around each occurrence of a string in a zone, for format work.
            let zone = open_zone(args.get(1).ok_or("dump <ff> <text> [before] [after]")?)?;
            let needle = args.get(2).ok_or("dump <ff> <text> [before] [after]")?.as_bytes();
            let before: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(256);
            let after: usize = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(64);
            let d = &zone.data;
            let mut start = 0;
            while let Some(rel) = d[start..].windows(needle.len()).position(|w| w == needle) {
                let at = start + rel;
                start = at + 1;
                println!("== at {at:#x}");
                let from = at.saturating_sub(before) & !15;
                for row in (from..(at + after).min(d.len())).step_by(16) {
                    let bytes = &d[row..(row + 16).min(d.len())];
                    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    let text: String = bytes.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
                    println!("{row:08x}  {:<47}  {text}", hex.join(" "));
                }
            }
            Ok(())
        }
        "find" => {
            let needle = args.get(1).ok_or("find <text> [zone.ff ...]")?.to_ascii_lowercase();
            let zones: Vec<PathBuf> = if args.len() > 2 {
                args[2..].iter().map(|z| resolve(z, &["zone", "all"])).collect::<Result<_, _>>()?
            } else {
                let mut v: Vec<PathBuf> = fs::read_dir(bo2_dir()?.join("zone").join("all"))
                    .map_err(|e| e.to_string())?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "ff"))
                    .collect();
                v.sort();
                v
            };
            for path in zones {
                let zone = match fs::read(&path).map_err(|e| e.to_string()).and_then(|f| fastfile::read(&f).map_err(|e| e.to_string())) {
                    Ok(z) => z,
                    Err(e) => {
                        eprintln!("{}: {e}", path.display());
                        continue;
                    }
                };
                let mut seen = std::collections::BTreeSet::new();
                for s in zone.data.split(|&b| b == 0) {
                    if s.len() >= 4 && s.iter().all(|&b| (0x20..0x7f).contains(&b)) {
                        let text = String::from_utf8_lossy(s);
                        if text.to_ascii_lowercase().contains(&needle) && seen.insert(text.to_string()) {
                            println!("{}: {text}", zone.name);
                        }
                    }
                }
            }
            Ok(())
        }
        "list" => {
            let bank = open_bank(args.get(1).ok_or("list <bank>")?)?;
            println!("{:>10}  {:>7}  {:>2}  {:>6}  format", "id", "seconds", "ch", "rate");
            for e in bank.entries() {
                let fmt = match e.format {
                    FORMAT_FLAC => "flac",
                    FORMAT_PCM16 => "pcm16",
                    _ => "other",
                };
                println!("{:#010x}  {:>7.2}  {:>2}  {:>6}  {fmt}", e.id, e.duration_secs().unwrap_or(0.0), e.channels, e.sample_rate().unwrap_or(0));
            }
            Ok(())
        }
        "export" => {
            let bank = open_bank(args.get(1).ok_or("export <bank> <out-dir>")?)?;
            let out = PathBuf::from(args.get(2).ok_or("export <bank> <out-dir>")?);
            let min: f32 = flag(args, "--min").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let max: f32 = flag(args, "--max").and_then(|v| v.parse().ok()).unwrap_or(f32::INFINITY);
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let mut n = 0;
            for e in bank.entries() {
                let d = e.duration_secs().unwrap_or(0.0);
                if d < min || d > max {
                    continue;
                }
                if let Some(wav) = sounds::entry_to_wav(&bank, e)? {
                    fs::write(out.join(format!("{:08x}.wav", e.id)), wav).map_err(|err| err.to_string())?;
                    n += 1;
                }
            }
            println!("exported {n} entries to {}", out.display());
            Ok(())
        }
        "images" => {
            let pak = open_pak(args.get(1).ok_or("images <pak>")?)?;
            let filter = Filter::from_args(args);
            for (hash, parts) in images::group_by_name(&pak) {
                if let Some(h) = images::header(&pak, &parts).filter(|h| filter.accepts(h)) {
                    println!("{hash:08x}  {:?}  {}x{}  parts {}", h.format, h.width, h.height, parts.len());
                }
            }
            Ok(())
        }
        "sheets" => {
            let pak = open_pak(args.get(1).ok_or("sheets <pak> <out-dir>")?)?;
            let out = PathBuf::from(args.get(2).ok_or("sheets <pak> <out-dir>")?);
            let filter = Filter::from_args(args);
            let per: usize = flag(args, "--per-sheet").and_then(|v| v.parse().ok()).unwrap_or(64);
            let cell: u32 = flag(args, "--cell").and_then(|v| v.parse().ok()).unwrap_or(96);
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let mut batch = Vec::new();
            let mut index = String::new();
            let mut sheet_no = 0;
            let groups = images::group_by_name(&pak);
            let flush = |batch: &mut Vec<_>, index: &mut String, sheet_no: &mut usize| -> Result<(), String> {
                if batch.is_empty() {
                    return Ok(());
                }
                let s = images::sheet(batch, cell, 8);
                images::write_png(&out.join(format!("sheet_{sheet_no:03}.png")), &s)?;
                fs::write(out.join(format!("sheet_{sheet_no:03}.txt")), &*index).map_err(|e| e.to_string())?;
                batch.clear();
                index.clear();
                *sheet_no += 1;
                Ok(())
            };
            for (hash, parts) in &groups {
                if !filter.wants_hash(*hash) {
                    continue;
                }
                let Some(h) = images::header(&pak, parts).filter(|h| filter.accepts(h)) else { continue };
                let Ok(img) = images::load(&pak, parts) else { continue };
                index.push_str(&format!("{:3} {hash:08x} {:?} {}x{}\n", batch.len(), h.format, h.width, h.height));
                batch.push(img);
                if batch.len() == per {
                    flush(&mut batch, &mut index, &mut sheet_no)?;
                }
            }
            flush(&mut batch, &mut index, &mut sheet_no)?;
            println!("wrote {sheet_no} sheets to {} (cells are numbered left-to-right, 8 per row)", out.display());
            Ok(())
        }
        "image" => {
            let pak = open_pak(args.get(1).ok_or("image <pak> <hash> <out.png>")?)?;
            let hash = u32::from_str_radix(args.get(2).ok_or("image <pak> <hash> <out.png>")?.trim_start_matches("0x"), 16)
                .map_err(|e| e.to_string())?;
            let out = PathBuf::from(args.get(3).ok_or("image <pak> <hash> <out.png>")?);
            let groups = images::group_by_name(&pak);
            let parts = groups.get(&hash).ok_or("no image with that hash")?;
            images::write_png(&out, &images::load(&pak, parts)?)
        }
        _ => Err("usage: zhbo-assets <banks|list|export|paks|images|sheets|image> ...".into()),
    }
}

fn open_bank(name: &str) -> Result<SoundBank, String> {
    let path = resolve(name, &["sound"])?;
    SoundBank::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}

type Paks = Vec<(String, Ipak, std::collections::BTreeMap<u32, Vec<zhbo_formats::ipak::IpakEntry>>)>;

/// The colour map of each surface's material, decoded from the image packs.
fn color_maps(model: &zhbo_formats::xmodel::Model, paks: &Paks) -> preview::Textures {
    model
        .materials
        .iter()
        .map(|m| {
            let img = m.as_ref()?.color_map()?;
            let (_, pak, groups) = paks.iter().find(|(_, _, g)| g.contains_key(&img.stream_hash))?;
            images::load(pak, &groups[&img.stream_hash]).ok().map(std::rc::Rc::new)
        })
        .collect()
}

/// Every image pack with its entries grouped by name hash.
fn image_paks() -> Result<Paks, String> {
    let dir = bo2_dir()?.join("zone").join("all");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ipak"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for n in names {
        let pak = open_pak(&n)?;
        let groups = images::group_by_name(&pak);
        out.push((n, pak, groups));
    }
    Ok(out)
}

/// Fully loaded zones, by file name.
type ZoneCache = std::collections::HashMap<String, zhbo_formats::t6zone::Zone>;

fn load_zone(zones: &mut ZoneCache, name: &str) -> Result<(), String> {
    if !zones.contains_key(name) {
        zones.insert(name.to_string(), load_full_zone(name)?);
    }
    Ok(())
}

/// Decrypt a fastfile and load every asset in it.
fn load_full_zone(name: &str) -> Result<zhbo_formats::t6zone::Zone, String> {
    let zone = open_zone(name)?;
    zhbo_formats::t6zone::load(&zone).map_err(|e| format!("{name}: {e}"))
}

fn open_zone(name: &str) -> Result<fastfile::Zone, String> {
    let path = resolve(name, &["zone", "all"])?;
    let file = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    fastfile::read(&file).map_err(|e| format!("{}: {e}", path.display()))
}

fn open_pak(name: &str) -> Result<Ipak, String> {
    let path = resolve(name, &["zone", "all"])?;
    Ipak::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}
