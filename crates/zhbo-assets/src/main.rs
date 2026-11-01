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
//! zhbo-assets screenshot [out-dir] [--archive <big>]  render the create-a-class screens to PNG
//!                                                     (default output: target/cac-shots/)
//! zhbo-assets hud-sheet [out-dir] [--archive <big>] [--cols N]
//!                                                     every MappedImage of a built archive as a
//!                                                     labelled contact sheet + the raw atlas pages
//!                                                     (default output: target/hud-sheet/)
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
        self.hashes.as_ref().is_none_or(|h| h.contains(&hash))
    }

    fn accepts(&self, h: &zhbo_formats::iwi::Header) -> bool {
        let code = match h.format {
            Format::Argb32 => 1,
            Format::Rgb24 => 2,
            Format::A8 => 5,
            Format::Dxt1 => 11,
            Format::Dxt3 => 12,
            Format::Dxt5 => 13,
            Format::Bc5 => 14,
            Format::Other(c) => c,
        };
        self.formats.as_ref().is_none_or(|f| f.contains(&code)) && h.width.max(h.height) <= self.max_size
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str).unwrap_or("") {
        "banks" => list_dir(bo2_dir()?.join("sound"), &[".sabs", ".sabl"]),
        "paks" => list_dir(bo2_dir()?.join("zone").join("all"), &[".ipak"]),
        "zones" => list_dir(bo2_dir()?.join("zone").join("all"), &[".ff"]),
        "zone" => cmd_zone(args),
        "model" => cmd_model(args),
        "pose" => cmd_pose(args),
        "walk" => cmd_walk(args),
        "assets" => cmd_assets(args),
        "material" => cmd_material(args),
        "weapons" => cmd_weapons(args),
        "attachments" => cmd_attachments(args),
        "fx" => cmd_fx(args),
        "strings" => cmd_strings(args),
        "probes" => cmd_probes(args),
        "aliases" => cmd_aliases(args),
        "mix" => cmd_mix(args),
        "anim" => cmd_anim(args),
        "conformance" => cmd_conformance(args),
        "screenshot" => cmd_screenshot(args),
        "audit-native" => cmd_audit_native(args),
        "build-archive" => cmd_build_archive(args),
        "hud-sheet" => cmd_hud_sheet(args),
        "hammer" => cmd_hammer(args),
        "posew3d" => cmd_posew3d(args),
        "dump" => cmd_dump(args),
        "find" => cmd_find(args),
        "list" => cmd_list(args),
        "fonts" => cmd_fonts(args),
        "export" => cmd_export(args),
        "images" => cmd_images(args),
        "sheets" => cmd_sheets(args),
        "image" => cmd_image(args),
        _ => Err("usage: zhbo-assets <banks|list|export|paks|images|sheets|image|conformance> ...".into()),
    }
}

/// `zhbo-assets zone`.
fn cmd_zone(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets model`.
fn cmd_model(args: &[String]) -> Result<(), String> {
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
                            let pak = paks.iter().find(|(_, p)| p.entry(img.key()).is_some()).map_or("-", |(n, _)| n.as_str());
                            println!(
                                "      semantic {:>2}: {} {}x{} name hash {:#010x}, stream hash {:#010x} ({} parts) in {}",
                                t.semantic, img.name, img.width, img.height, img.hash, img.stream_hash, img.streamed_parts, pak
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
    if args.iter().any(|a| a == "--hitboxes") {
        for (i, h) in model.hit_boxes.iter().enumerate().filter(|(_, h)| h.radius_squared > 0.0 || h.location != 0) {
            println!("  hit {i:>3} {:<24} loc {:>2} r2 {:>8.2} mins {:>7.2?} maxs {:>7.2?} offset {:>7.2?}", model.bones[i].name, h.location, h.radius_squared, h.mins, h.maxs, h.offset);
        }
    }
    if let Some(out) = flag(args, "--png") {
        let textures = color_maps(&model, &paks);
        preview::render(&model, &textures, Path::new(out)).map_err(|e| e.to_string())?;
        println!("wrote {out}");
    }
    Ok(())
}

/// `zhbo-assets pose`.
fn cmd_pose(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets walk`.
fn cmd_walk(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets assets`.
fn cmd_assets(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets material`.
fn cmd_material(args: &[String]) -> Result<(), String> {
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
            let found = paks.iter().find_map(|(n, pak)| pak.entry(img.key()).map(|e| (n, images::load(pak, e))));
            match found {
                Some((pak, Ok(rgba))) => {
                    let file = out.join(format!("{name}_{i}.png"));
                    images::write_png(&file, &rgba)?;
                    // Also over dark grey, since most HUD art is white on clear.
                    let mut dark = rgba.clone();
                    for px in dark.pixels.as_chunks_mut::<4>().0 {
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

/// `zhbo-assets weapons`.
fn cmd_weapons(args: &[String]) -> Result<(), String> {
    // weapons [name filter]: BO2 multiplayer weapon definitions (common_mp).
    use zhbo_formats::weapon;
    let zone = load_full_zone("common_mp.ff")?;
    let filter = args.get(1).map(String::as_str);
    for w in weapon::all(&zone).map_err(|e| e.to_string())? {
        if filter.is_some_and(|f| !w.name.contains(f)) {
            continue;
        }
        println!(
            "{:<24} {:<18} type {} class {:>2} pat {:>2} inv {} fire {} clip {:>3} start {:<3} max {:<3} rpm {:>5.0} dmg {:?}@{:?} reload {:.2}/{:.2} add {:.2} ads {:.2} raise {:.2} drop {:.2} spread hip {:?} ads {:.2} move {:.2} pellets {} fuse {:.2} expl {}/{}-{} vm {:?} anims {}",
            w.name,
            w.display_name,
            w.weap_type,
            w.class,
            w.player_anim_type,
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
            println!("    sounds fire {:?} plr {:?} icon {:?} dpad {:?} world {:?} projectile {:?} impact {} icon key {:?} dpad key {:?}", w.fire_sound, w.fire_sound_player, w.hud_icon, w.dpad_icon, w.world_model, w.projectile_model, w.projectile_impact_explode, w.hud_icon_key, w.dpad_icon_key);
            println!("    raise {:?} first {:?} putaway {:?} alt switch {:?}", w.raise_sound_player, w.first_raise_sound_player, w.putaway_sound_player, w.alt_switch_sound_player);
            println!("    overlay {:?} low {:?} reticle {} size {:?}", w.overlay_material, w.overlay_material_low_res, w.overlay_reticle, w.overlay_size);
            println!("    kick ads {:?} hip {:?} mult {:?}", w.ads_view_kick, w.hip_view_kick, w.location_multipliers.iter().take(7).collect::<Vec<_>>());
            println!(
                "    offhand slot {} class {} hold {:.3} delay {:.3} fire {:.3} detonate {:.3}/{:.3} quick {:.3}/{:.3} detonator {} hold_to_throw {} cancelable {} hold_use {} plantable {} cook {}",
                w.offhand_slot, w.offhand_class, w.hold_fire_time, w.fire_delay, w.fire_time, w.detonate_time, w.detonate_delay, w.quick_raise_time, w.quick_drop_time,
                w.has_detonator, w.hold_button_to_throw, w.offhand_hold_is_cancelable, w.is_hold_use_grenade, w.plantable, w.cook_off_hold
            );
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

/// `zhbo-assets attachments`.
fn cmd_attachments(args: &[String]) -> Result<(), String> {
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
                println!("  unique {} [{}] kind {} child {} sibling {} combined {:#x} vmads {:?} vm {:?} vmad {:?} addoff {:?} world {:?} tag {:?}/{:?} noclip {} nobaseatt {} override {} baseoff {:?} anims {} off {:?} rot {:?} woff {:?} hide {:?} snd {:?} alt {:?} dw {:?}", u.name, u.index, u.kind, u.child, u.sibling, u.combined, u.view_model_ads, u.view_model, u.view_model_additional, u.view_add_offset, u.world_model, u.view_model_tag, u.world_model_tag, u.disable_base_clip, u.disable_base_attachment, u.override_base_attachment_offsets, u.base_attachment_offset, u.anims.iter().filter(|a| a.is_some()).count(), u.view_offset, u.view_rotation, u.world_offset, u.hide_tags, u.fire_sound_player, u.alt_weapon, u.dual_wield_weapon);
            }
        }
    }
    Ok(())
}

/// `zhbo-assets fx`.
fn cmd_fx(args: &[String]) -> Result<(), String> {
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
        // Every table row on dirt and concrete, its first blood cell, and the impact type
        // that reads it (`fx::IMPACT_KINDS`).
        for row in 0..24u32 {
            let n = |s| fx::impact(&zone, row, s).map(|e| e.name).unwrap_or_default();
            let blood = fx::impact_flesh(&zone, row).map(|e| e.name).unwrap_or_default();
            let types: Vec<u32> = fx::IMPACT_KINDS.iter().filter(|k| k.row == row).map(|k| k.impact_type).collect();
            println!("row {row} (impact types {types:?}): dirt {} concrete {} blood {blood}", n(fx::surface::DIRT), n(fx::surface::CONCRETE));
        }
        // The bounce and dud rows' own cells per surface (no default fallback).
        for row in [11u32, 15] {
            let cells: Vec<String> = (0..32).map(|s| format!("{s}={}", fx::impact_cell(&zone, row, s).map(|e| e.name).unwrap_or_else(|| "-".into()))).collect();
            println!("row {row} cells: {}", cells.join(" "));
        }
        for w in weapon::all(&zone).map_err(|e| e.to_string())?.iter().filter(|w| !fx::is_bullet(w.impact_type)) {
            let own = w.explosion_effect.as_ref().map(|e| e.name.as_str()).unwrap_or("-");
            println!("{} impact type {} own effect {own} own sound {}", w.name, w.impact_type, w.projectile_explosion_sound.as_deref().unwrap_or("-"));
            // The bounce and dud fields (`FUN_004c1590` events 0x47 and 0x51).
            let bounce: Vec<String> = w.bounce_sounds.iter().enumerate().filter_map(|(i, a)| a.as_ref().map(|a| format!("{i}={a}"))).collect();
            let dud = w.projectile_dud_effect.as_ref().map(|e| e.name.as_str()).unwrap_or("-");
            println!("  activate dist {} dud effect {dud} dud sound {} bounce [{}]", w.projectile_activate_dist, w.projectile_dud_sound.as_deref().unwrap_or("-"), bounce.join(" "));
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

/// `zhbo-assets strings`.
fn cmd_strings(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets probes`.
fn cmd_probes(args: &[String]) -> Result<(), String> {
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
        let head = paks.iter().find_map(|(n, pak)| pak.entry(im.key()).and_then(|e| images::header(pak, e)).map(|h| (n.clone(), h)));
        println!("{i:3} {origin:?} {} {}x{} stream {:#x} parts {} -> {head:?}", im.name, im.width, im.height, im.stream_hash, im.streamed_parts);
        let def = zone.u32(img);
        if def != 0 {
            println!("    loadDef levels {} flags {:#x} format {} size {} mapType {} depth {}", zone.u8(def), zone.u8(def + 1), zone.u32(def + 4), zone.u32(def + 8), zone.u8(img + 16), zone.u16(img + 24));
        }
    }
    Ok(())
}

/// `zhbo-assets aliases`.
fn cmd_aliases(args: &[String]) -> Result<(), String> {
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
                "  {:<48} {} variant(s), asset {:#010x} vol {:.2}..{:.2} dist {:?} {}{}",
                a.name,
                v.len(),
                first.map_or(0, |v| v.asset_id),
                first.map_or(0.0, |v| v.volume.0),
                first.map_or(0.0, |v| v.volume.1),
                first.map(|v| v.distance),
                first.map_or("", |v| v.file.as_str()),
                first.and_then(|v| v.secondary.as_deref()).map_or(String::new(), |s| format!(" then {s}"))
            );
        }
    }
    Ok(())
}

/// `zhbo-assets mix`.
fn cmd_mix(args: &[String]) -> Result<(), String> {
    // mix <ff> <alias> [inches...]: BO2's mixer on an alias and its secondary chain — each
    // layer's bus, group, pan, start delay, curves and gain at each distance (default 0).
    let zone = load_full_zone(args.get(1).ok_or("mix <ff> <alias> [inches...]")?)?;
    let name = args.get(2).ok_or("mix <ff> <alias> [inches...]")?;
    let gfx = load_full_zone(&format!("{}.ff", zhbo_assets::sfx::MIXER_ZONE))?;
    let globals = zhbo_formats::sndglobals::read(&gfx).map_err(|e| e.to_string())?.ok_or("no SndDriverGlobals")?;
    let mixer = zhbo_assets::mixer::Mixer::new(&globals).ok_or("no master snapshot")?;
    let banks = zhbo_formats::sndalias::banks(&zone).map_err(|e| e.to_string())?;
    let find = |n: &str| banks.iter().find_map(|b| b.alias(n));
    let mut at: Vec<f32> = args[3..].iter().filter_map(|a| a.parse().ok()).collect();
    if at.is_empty() {
        at.push(0.0);
    }
    println!("master {}: bus gains {:?}", mixer.master.name, mixer.master.bus_gain);
    let chain = zhbo_assets::sfx::secondary_chain(&[name.as_str()], find);
    for a in &chain {
        let Some(v) = a.variants.first() else { continue };
        let curve = |c: u8| globals.curve(c).map_or("?".to_string(), |c| c.name.clone());
        println!(
            "  {:<40} vol {:.3} {:?} {} pan {} {} delay {}ms dist {:?} curves {}/{} gains {}",
            a.name,
            v.volume.1,
            mixer.bus(v),
            globals.groups.get(v.mix.group as usize).map_or("?", |g| g.name.as_str()),
            globals.pan(v.mix.pan).map_or("?", |p| p.name.as_str()),
            if v.mix.positional { "3d" } else { "2d" },
            v.start_delay_ms,
            v.distance,
            curve(v.mix.min_falloff_curve),
            curve(v.mix.falloff_curve),
            at.iter().map(|d| format!("{d}:{:.4}", mixer.gain(v, *d))).collect::<Vec<_>>().join(" ")
        );
    }
    Ok(())
}

/// `zhbo-assets anim`.
fn cmd_anim(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets conformance`.
fn cmd_conformance(args: &[String]) -> Result<(), String> {
    // conformance <out-dir>: expected.csv (BO2's rules over the install) and actual.csv (what
    // the mod ships), diffed and clustered, for every table. The directory must be outside
    // the repository; the install is ZHBO_BO2_DIR or the detected one.
    let out = args.get(1).filter(|a| !a.starts_with("--")).map(PathBuf::from).ok_or("usage: zhbo-assets conformance <out-dir outside the repository>")?;
    let bo2 = match std::env::var("ZHBO_BO2_DIR") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => bo2_dir()?,
    };
    let tables = zhbo_assets::conformance::run(&bo2, &out)?;
    print!("{}", zhbo_assets::conformance::summary(&tables, &out));
    let gaps: usize = tables.iter().map(|t| t.report.gap_count()).sum();
    println!("{gaps} gap clusters (0 is parity)");
    Ok(())
}

/// `zhbo-assets screenshot`.
fn cmd_screenshot(args: &[String]) -> Result<(), String> {
    // screenshot [out-dir] [--archive path]: the class screens through the real drawing code.
    let out = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/cac-shots"));
    let archive = flag(args, "--archive").map(PathBuf::from);
    let size = flag(args, "--size").and_then(|v| v.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
    zhbo_assets::screenshot::run(&out, archive.as_deref(), size)
}

/// `zhbo-assets audit-native`.
fn cmd_audit_native(args: &[String]) -> Result<(), String> {
    // audit-native [archive]: load every converted weapon's first- and third-person art the
    // way the game does and list each one that would draw nothing.
    let path = args.get(1).map(PathBuf::from).ok_or("audit-native <ConquerOps_BO2.big>")?;
    let report = zhbo_assets::native_audit::audit_archive(&path)?;
    for p in &report.problems {
        println!("{}: {}", p.weapon, p.what);
    }
    println!("{} first-person loads, {} world manifests, {} problems", report.first_person, report.third_person, report.problems.len());
    if report.problems.is_empty() { Ok(()) } else { Err(format!("{} weapons do not draw", report.problems.len())) }
}

/// `zhbo-assets build-archive`.
fn cmd_build_archive(args: &[String]) -> Result<(), String> {
    // build-archive <out.big>: the launcher's BO2 conversion into a file of your choice
    // (development: refresh the archive the screenshot harness reads without launching).
    let out = PathBuf::from(args.get(1).ok_or("build-archive <out.big>")?);
    let (bytes, report) = zhbo_assets::hud::build_archive(&bo2_dir()?)?;
    fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("built {} ({} images, missing: {:?})", out.display(), report.found.len(), report.missing);
    if let Some(Err(e)) = &report.generals {
        println!("generals: {e}");
    }
    Ok(())
}

/// `zhbo-assets hud-sheet`.
fn cmd_hud_sheet(args: &[String]) -> Result<(), String> {
    // hud-sheet [out-dir] [--archive path] [--cols N]: every MappedImage, labelled.
    let out = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/hud-sheet"));
    let archive = flag(args, "--archive").map(PathBuf::from);
    let cols = flag(args, "--cols").and_then(|c| c.parse().ok()).unwrap_or(5);
    zhbo_assets::screenshot::run_contact(&out, archive.as_deref(), cols)
}

/// `zhbo-assets hammer`.
fn cmd_hammer(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets posew3d`.
fn cmd_posew3d(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets dump`.
fn cmd_dump(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets find`.
fn cmd_find(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets list`.
fn cmd_list(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets fonts`.
fn cmd_fonts(args: &[String]) -> Result<(), String> {
    // fonts <ff>: every font in a zone (the UI's are in zone/english/en_code_post_gfx_mp.ff).
    let zone = load_full_zone(args.get(1).ok_or("fonts <ff>")?)?;
    for f in zhbo_formats::font::all(&zone).map_err(|e| e.to_string())? {
        let image = f.material.as_ref().and_then(|m| m.textures.iter().find_map(|t| t.image.as_ref()));
        println!(
            "{:<28} height {:>3}, {:>4} glyphs, {:>4} kerning pairs, material {} ({})",
            f.name,
            f.pixel_height,
            f.glyphs.len(),
            f.kerning.len(),
            f.material.as_ref().map_or("-", |m| m.name.as_str()),
            image.map_or("no image".to_string(), |i| format!("{} {}x{} hash {:#010x}", i.name, i.width, i.height, i.stream_hash))
        );
        if let Some(g) = f.glyphs.iter().find(|g| g.letter == u16::from(b'A')) {
            println!("  'A': {g:?}");
        }
    }
    Ok(())
}

/// `zhbo-assets export`.
fn cmd_export(args: &[String]) -> Result<(), String> {
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

/// `zhbo-assets images`.
fn cmd_images(args: &[String]) -> Result<(), String> {
    let pak = open_pak(args.get(1).ok_or("images <pak>")?)?;
    let filter = Filter::from_args(args);
    for e in pak.entries() {
        if let Some(h) = images::header(&pak, e).filter(|h| filter.accepts(h)) {
            println!("{:08x} {:08x}  {:?}  {}x{}", e.data_hash, e.name_hash, h.format, h.width, h.height);
        }
    }
    Ok(())
}

/// `zhbo-assets sheets`.
fn cmd_sheets(args: &[String]) -> Result<(), String> {
    let pak = open_pak(args.get(1).ok_or("sheets <pak> <out-dir>")?)?;
    let out = PathBuf::from(args.get(2).ok_or("sheets <pak> <out-dir>")?);
    let filter = Filter::from_args(args);
    let per: usize = flag(args, "--per-sheet").and_then(|v| v.parse().ok()).unwrap_or(64);
    let cell: u32 = flag(args, "--cell").and_then(|v| v.parse().ok()).unwrap_or(96);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut batch = Vec::new();
    let mut index = String::new();
    let mut sheet_no = 0;
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
    for e in pak.entries() {
        if !filter.wants_hash(e.data_hash) {
            continue;
        }
        let Some(h) = images::header(&pak, e).filter(|h| filter.accepts(h)) else { continue };
        let Ok(img) = images::load(&pak, e) else { continue };
        index.push_str(&format!("{:3} {:08x} {:08x} {:?} {}x{}\n", batch.len(), e.data_hash, e.name_hash, h.format, h.width, h.height));
        batch.push(img);
        if batch.len() == per {
            flush(&mut batch, &mut index, &mut sheet_no)?;
        }
    }
    flush(&mut batch, &mut index, &mut sheet_no)?;
    println!("wrote {sheet_no} sheets to {} (cells are numbered left-to-right, 8 per row)", out.display());
    Ok(())
}

/// `zhbo-assets image`.
fn cmd_image(args: &[String]) -> Result<(), String> {
    let pak = open_pak(args.get(1).ok_or("image <pak> <hash> <out.png>")?)?;
    let hash = u32::from_str_radix(args.get(2).ok_or("image <pak> <hash> <out.png>")?.trim_start_matches("0x"), 16)
        .map_err(|e| e.to_string())?;
    let out = PathBuf::from(args.get(3).ok_or("image <pak> <hash> <out.png>")?);
    // The hash is either field (the material dump prints both); it must name one entry.
    let found: Vec<_> = pak.entries().iter().filter(|e| e.data_hash == hash || e.name_hash == hash).collect();
    match found[..] {
        [e] => images::write_png(&out, &images::load(&pak, e)?),
        [] => Err("no image with that hash".into()),
        _ => Err(format!("{} images match that hash (data, name): {:x?}", found.len(), found.iter().map(|e| (e.data_hash, e.name_hash)).collect::<Vec<_>>())),
    }
}

fn open_bank(name: &str) -> Result<SoundBank, String> {
    let path = resolve(name, &["sound"])?;
    SoundBank::open(&path).map_err(|e| format!("{}: {e}", path.display()))
}

type Paks = Vec<(String, Ipak)>;

/// The colour map of each surface's material, decoded from the image packs.
fn color_maps(model: &zhbo_formats::xmodel::Model, paks: &Paks) -> preview::Textures {
    model
        .materials
        .iter()
        .map(|m| {
            let img = m.as_ref()?.color_map()?;
            paks.iter().find_map(|(_, pak)| pak.entry(img.key()).and_then(|e| images::load(pak, e).ok())).map(std::rc::Rc::new)
        })
        .collect()
}

/// Every image pack, with its file name.
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
        out.push((n, pak));
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
    zhbo_formats::t6zone::load_file(&resolve(name, &["zone", "all"])?)
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
