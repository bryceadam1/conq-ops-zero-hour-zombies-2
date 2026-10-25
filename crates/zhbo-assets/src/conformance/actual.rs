//! `actual.csv`'s weapon rows: what the mod ships, read back from the launcher's own conversion.
//!
//! The rows are the mod's own create-a-class space — [`zhbo_core::classmenu::offer`]'s weapons with
//! every attachment set the editor accepts (any offered shorts up to the slot's cap, Gunfighter
//! included), resolved by the runtime catalog exactly as a class is (`Catalog::get`: one
//! attachment per point), each one's alt mode, and the loadout's knife. Every fact comes from what
//! ships: the converted weapon table (`weapons.ini`), the native viewmodel and world-weapon
//! manifests the def's `viewmodel` names (a combination the archive has no art for draws its
//! base's, as the game does), the model blobs, the converter's sound events and FXLists.
//!
//! Each column carries the mod's value of the same fact in BO2's vocabulary and unit; a fact the
//! mod does not carry is left empty (it fails, owner decision 1) — never synthesized.

use std::collections::{BTreeMap, HashMap};
use zhbo_conformance::schema::{f32_value, Row, RowBuilder, Schema};
use zhbo_conformance::weapons::{self as cols, enum_name, ATTACHMENTS};
use super::cac::group_of;
use zhbo_core::weapons::{self, FireMode, WeaponDef};
use zhbo_native::viewmodel::{Viewmodel, INSTANCE_LEFT};

/// The archive's MappedImage INI (the HUD's sprite collection).
const MAPPED_IMAGES: &str = "Data\\INI\\MappedImages\\HandCreated\\ZHBO_BO2.ini";
/// The faction whose viewmodel manifest is read: the arms are Zero Hour's and excluded, so any
/// faction's manifest lists the same weapon parts.
const FACTION: char = 'U';

/// What the conversion shipped, opened once.
pub struct Shipped {
    files: HashMap<String, Vec<u8>>,
    /// Sound event → the BO2 aliases mixed into it (the converter's `DynSfx`).
    events: HashMap<String, Vec<String>>,
    /// FXList name → the BO2 effects the converter could have made it from.
    fx: HashMap<String, Vec<String>>,
    /// The BO2 weapons as the converter parsed them (for the icon sprites' sources).
    bo2: Vec<zhbo_formats::weapon::Weapon>,
    sprites: String,
}

impl Shipped {
    /// Run the launcher's conversion in memory and install its weapon table as the game does.
    pub fn convert(bo2_dir: &std::path::Path) -> Result<Shipped, String> {
        // The archive the launcher writes (`prepare_bo2_art`): the HUD and CaC art, the fonts and
        // `generals::files`, packed and read back.
        let (archive, _report) = crate::hud::build_archive(bo2_dir)?;
        let files: HashMap<String, Vec<u8>> = zhbo_formats::big::members(&archive).map_err(|e| e.to_string())?.into_iter().collect();
        let table = files.get(crate::weapons::TABLE_PATH).ok_or("the conversion shipped no weapon table")?;
        let text = String::from_utf8_lossy(table).to_string();
        match weapons::install_catalog(&text) {
            Ok(_) => {}
            // A second run in one process (the CLI after a test): the table is the same conversion.
            Err(e) if e.contains("already installed") => {}
            Err(e) => return Err(e),
        }
        // The converter's sound events for the shipped defs (`weapons::sounds_and_fx`, the step
        // `generals::files` ran; deterministic over the same inputs).
        let common = crate::generals::load_zone(bo2_dir, "common_mp")?;
        let bo2 = zhbo_formats::weapon::all(&common).map_err(|e| e.to_string())?;
        let atts = zhbo_formats::weapon::attachments(&common).map_err(|e| e.to_string())?;
        let mut defs = zhbo_core::weapons::parse_full_table(&text)?.weapons;
        let (events, _) = crate::weapons::sounds_and_fx(&bo2, &atts, &mut defs, &crate::weapons::converted(&bo2));
        let mut ev = HashMap::new();
        for e in events {
            ev.insert(e.event, e.layers);
        }
        let mut fx: HashMap<String, Vec<String>> = HashMap::new();
        for w in &bo2 {
            for e in [&w.view_flash_effect, &w.world_flash_effect, &w.view_shell_eject_effect, &w.view_last_shot_eject_effect, &w.explosion_effect].into_iter().flatten() {
                let names = fx.entry(crate::fx::fxlist_name(&e.name)).or_default();
                if !names.contains(&e.name) {
                    names.push(e.name.clone());
                }
            }
        }
        let sprites = files.get(MAPPED_IMAGES).map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
        Ok(Shipped { files, events: ev, fx, bo2, sprites })
    }

    fn viewmodel(&self, vm: i32) -> Option<Viewmodel> {
        let bytes = self.files.get(&zhbo_native::viewmodel_path(vm, FACTION))?;
        Viewmodel::from_bytes(bytes).ok()
    }

    fn world_weapon(&self, vm: i32) -> Option<zhbo_native::thirdperson::Weapon> {
        let bytes = self.files.get(&zhbo_native::world_weapon_path(vm))?;
        zhbo_native::thirdperson::Weapon::from_bytes(bytes).ok()
    }

    fn model(&self, path: &str) -> Option<zhbo_native::model::Model> {
        zhbo_native::model::Model::from_bytes(self.files.get(path)?).ok()
    }

    /// The BO2 effect an FXList stands for, or the FXList itself when it is not a converted one
    /// (a Zero Hour stock effect). Two effects with one FXList are both named: the shipped name
    /// cannot tell them apart.
    fn effect(&self, fxlist: &str) -> Option<String> {
        if fxlist.is_empty() {
            return None;
        }
        Some(self.fx.get(fxlist).map_or_else(|| fxlist.to_string(), |n| n.join("|")))
    }

    fn sound(&self, event: &str) -> Option<String> {
        self.events.get(event).and_then(|l| l.first().cloned())
    }

    pub(crate) fn has_sprite(&self, name: &str) -> bool {
        self.sprites.lines().any(|l| l.trim_start().strip_prefix("MappedImage ").is_some_and(|n| n.trim() == name))
    }
}

/// A mod row before it is built.
struct Job {
    weapon: String,
    combo: Vec<String>,
    def: &'static WeaponDef,
    /// The def whose viewmodel draws this row (an alt mode's parent when the alt has none).
    vm_def: &'static WeaponDef,
    group: String,
    slot: String,
    requires: Vec<&'static str>,
}

/// The attachment shorts in `eAttachment` order (an unknown short last, by name).
fn bo2_order(shorts: &[String]) -> Vec<String> {
    let mut v = shorts.to_vec();
    v.sort_by_key(|s| (ATTACHMENTS.iter().position(|a| a == s).unwrap_or(usize::MAX), s.clone()));
    v
}

fn subsets<'a>(items: &[&'a str], max: usize) -> Vec<Vec<&'a str>> {
    let mut out = vec![Vec::new()];
    for size in 1..=max.min(items.len()) {
        let mut idx: Vec<usize> = (0..size).collect();
        loop {
            out.push(idx.iter().map(|&i| items[i]).collect());
            let mut i = size;
            while i > 0 && idx[i - 1] == items.len() - size + i - 1 {
                i -= 1;
            }
            if i == 0 {
                break;
            }
            idx[i - 1] += 1;
            for j in i..size {
                idx[j] = idx[j - 1] + 1;
            }
        }
    }
    out
}

fn jobs() -> Vec<Job> {
    use zhbo_core::classmenu::{draft_weapon_id, offer, offered_attachments};
    let o = offer();
    let cat = weapons::catalog();
    let mut out: Vec<Job> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // (slot, its weapons, its caps without/with the card, the card).
    type Slot<'a> = (&'a str, &'a Vec<&'static str>, (usize, usize), &'static str);
    let slots: [Slot; 4] = [
        ("primary", &o.primary, o.primary_caps, "primarygunfighter"),
        ("secondary", &o.secondary, o.secondary_caps, "secondarygunfighter"),
        ("primarygrenade", &o.lethal, (0, 0), ""),
        ("specialgrenade", &o.tactical, (0, 0), ""),
    ];
    for (slot, ids, (base_cap, cap), card) in slots {
        for &base in ids {
            let shorts = offered_attachments(base);
            for set in subsets(&shorts, cap) {
                let Some(def) = cat.get(&draft_weapon_id(base, &set)) else { continue };
                let combo = bo2_order(&def.attachments);
                if !seen.insert((base.to_string(), combo.clone())) {
                    continue;
                }
                let requires = if set.len() > base_cap { vec![card] } else { Vec::new() };
                out.push(Job { weapon: base.to_string(), combo: combo.clone(), def, vm_def: def, group: group_of(def.class).to_string(), slot: slot.to_string(), requires: requires.clone() });
                if def.alt.is_empty() {
                    continue;
                }
                if let Some(alt) = cat.get(&def.alt) {
                    let alt_base = crate::generals::base_id(&def.alt).to_string();
                    if seen.insert((alt_base.clone(), combo.clone())) {
                        let vm_def = if alt.viewmodel >= 0 { alt } else { def };
                        out.push(Job { weapon: alt_base, combo, def: alt, vm_def, group: String::new(), slot: String::new(), requires });
                    }
                }
            }
        }
    }
    let melee = weapons::Loadout::default().melee;
    if let Some(def) = cat.get(&melee) {
        if seen.insert((melee.clone(), Vec::new())) {
            out.push(Job { weapon: melee, combo: Vec::new(), def, vm_def: def, group: String::new(), slot: String::new(), requires: Vec::new() });
        }
    }
    out
}

/// Every actual row.
pub fn rows(shipped: &Shipped, schema: &Schema) -> Result<Vec<Row>, String> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for j in jobs() {
        match row(shipped, schema, &j) {
            Ok(r) => out.push(r),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors.join("\n"))
    }
}

/// Seconds to BO2's milliseconds.
fn ms(s: f32) -> f64 {
    f32_value(s) * 1000.0
}

/// Zero Hour units to inches.
fn inch(u: f32) -> f64 {
    f32_value(u / zhbo_core::units::ZH_PER_INCH)
}

fn quat(q: [f32; 4]) -> [f64; 4] {
    zhbo_conformance::bo2::rows::canonical_quat(q.map(|c| c as f64))
}

/// A model blob path back to the BO2 model it was made from (the blob is named after it).
fn stem(path: &str) -> String {
    path.rsplit('\\').next().unwrap_or(path).trim_end_matches(".zbm").trim_end_matches(".zba").to_string()
}

/// The BO2 `weapAnimFiles_t` slot `s` as a mod clip id: the left gun's `_LEFT` slots are the
/// right gun's slot on the left instance; an alt mode's clips follow its weapon's.
fn clip_id(s: usize, base: i32) -> i32 {
    let left = |right: i32| right | INSTANCE_LEFT as i32;
    let id = match s {
        0x4e => left(0x04),
        0x4f => left(0x06),
        0x50 => left(0x07),
        0x51 => left(0x01),
        0x52 => left(0x02),
        0x53 => left(0x12),
        0x54 => left(0x10),
        _ => s as i32,
    };
    // The alt offset applies to the slot, the instance bit stays in the high byte.
    ((id & !(INSTANCE_LEFT as i32)) + base) | (id & INSTANCE_LEFT as i32)
}

fn row(sh: &Shipped, schema: &Schema, j: &Job) -> Result<Row, String> {
    let d = j.def;
    let cat = weapons::catalog();
    let mut b = RowBuilder::new(schema, j.weapon.clone(), j.combo.join("+"));
    let fl = |x: f32| f32_value(x);
    let fls = |xs: &[f32]| xs.iter().map(|x| f32_value(*x)).collect::<Vec<f64>>();
    let base_def = cat.get(crate::generals::base_id(&d.id)).unwrap_or(d);

    // Names and identity.
    b.text("display_name", &d.display_name);
    let names: Vec<String> = j
        .combo
        .iter()
        .filter_map(|s| base_def.allowed.iter().find(|(short, _)| short == s).and_then(|(_, asset)| cat.attachment(asset)).map(|a| a.display_name.clone()))
        .collect();
    b.list("attachment_names", &names);
    for c in ["weap_type", "weap_class", "inventory", "offhand_class"] {
        b.absent(c);
    }
    let fire = match d.fire {
        FireMode::Auto => "fullauto".to_string(),
        FireMode::Semi => "singleshot".to_string(),
        FireMode::Burst(n) => format!("burstfire{n}"),
        FireMode::Minigun => "minigun".to_string(),
    };
    b.text("fire_type", fire).int("shot_count", d.pellets as i64).absent("shots_before_rechamber").absent("bolt_action");

    // Timing.
    for (c, v) in [
        ("fire_time", Some(d.fire_time)),
        ("last_fire_time", None),
        ("fire_delay", None),
        ("burst_delay", Some(d.burst_delay)),
        ("rechamber_time", None),
        ("rechamber_bolt_time", None),
        ("hold_fire_time", None),
        ("reload_time", Some(d.reload_time)),
        ("reload_empty_time", Some(d.reload_empty_time)),
        ("reload_add_time", Some(d.reload_add_time)),
        ("reload_empty_add_time", Some(d.reload_empty_add_time)),
        ("reload_quick_time", None),
        ("reload_quick_empty_time", None),
        ("reload_quick_add_time", None),
        ("reload_start_time", Some(d.reload_start_time)),
        ("reload_end_time", Some(d.reload_end_time)),
        ("raise_time", Some(d.raise_time)),
        ("first_raise_time", Some(d.first_raise_time)),
        ("drop_time", Some(d.drop_time)),
        ("alt_raise_time", None),
        ("alt_drop_time", None),
        ("quick_raise_time", Some(d.quick_raise_time)),
        ("quick_drop_time", Some(d.quick_drop_time)),
        ("empty_raise_time", None),
        ("empty_drop_time", None),
        ("sprint_in_time", None),
        ("sprint_out_time", Some(d.sprint_out_time)),
        ("ads_in_time", Some(d.ads_in_time)),
        ("ads_out_time", Some(d.ads_out_time)),
        ("melee_time", Some(d.melee_time)),
        ("melee_delay", None),
        ("spin_up_time", Some(d.spin_up_time)),
        ("spin_down_time", None),
        ("fuse_time", Some(d.fuse_time)),
        ("detonate_time", None),
    ] {
        match v {
            Some(s) => b.float(c, ms(s)),
            None => b.absent(c),
        };
    }
    // The mod's shell-by-shell reload is BO2's segmented reload: rounds per cycle with a start
    // and an end (the converter's `shells`).
    b.int("reload_ammo_add", d.reload_ammo_add as i64).absent("reload_start_add").bool("segmented_reload", d.reload_ammo_add > 0 && d.reload_start_time > 0.0);

    // Ammo: a gun carries its spare rounds beside the magazine; an offhand's reserve is the count
    // in hand (the converter's `to_def`).
    let (start, max) = if d.class.is_offhand() { (d.start_reserve as i64, d.max_reserve as i64) } else { (d.clip as i64 + d.start_reserve as i64, d.clip as i64 + d.max_reserve as i64) };
    b.int("clip_size", d.clip as i64)
        .int("start_ammo_rounds", start)
        .int("max_ammo_rounds", max)
        .absent("clip_only")
        .absent("dual_mag")
        .absent("shared_ammo");

    // Damage.
    b.list("damage", &d.damage.map(|x| format!("{}", x as i64)))
        .floats("damage_range", &d.damage_range.map(inch))
        .absent("min_player_damage")
        .int("melee_damage", d.melee_damage as i64)
        .absent("location_multipliers")
        .float("max_range", inch(d.max_range))
        .absent("penetrate_type")
        .float("explosion_radius", inch(d.explosion_radius))
        .float("explosion_radius_min", inch(d.explosion_radius_min))
        .int("explosion_inner_damage", d.explosion_inner_damage as i64)
        .int("explosion_outer_damage", d.explosion_outer_damage as i64)
        .float("projectile_speed", inch(d.projectile_speed))
        .float("projectile_speed_up", inch(d.projectile_speed_up))
        .bool("projectile_impact_explode", d.impact_explode)
        .absent("projectile_model")
        .text("impact_type", enum_name(cols::IMPACT_TYPE, d.impact_type.max(0) as u32));

    // Spread.
    b.floats("hip_spread", &fls(&d.hip_spread.iter().flat_map(|(a, c)| [*a, *c]).collect::<Vec<_>>()))
        .float("hip_spread_decay", fl(d.spread_decay))
        .float("hip_spread_fire_add", fl(d.spread_fire_add))
        .float("hip_spread_move_add", fl(d.spread_move_add))
        .float("ads_spread", fl(d.ads_spread))
        .absent("stack_fire_spread")
        .absent("stack_fire_accuracy_decay");

    // ADS: one zoomed field of view; the ADS walk is the converter's base times BO2's scale.
    b.float("ads_zoom_fov1", fl(d.ads_fov))
        .absent("ads_zoom_fov2")
        .absent("ads_zoom_fov3")
        .absent("ads_zoom_in_frac")
        .absent("ads_zoom_out_frac")
        .float("ads_move_speed_scale", fl(d.ads_move_scale / crate::weapons::ADS_MOVE_BASE))
        .float("move_speed_scale", fl(d.move_scale))
        .float("sprint_duration_scale", fl(d.sprint_duration_scale))
        .absent("alt_scope_ads_in_time")
        .absent("alt_scope_ads_out_time")
        .absent("ads_dof")
        .absent("overlay_alpha_scale");

    // Kick and recoil.
    let gk = |k: &weapons::GunKick| fls(&[k.pitch.0, k.pitch.1, k.yaw.0, k.yaw.1, k.accel, k.speed_max, k.speed_decay, k.static_decay, k.reduced_percent]);
    b.floats("ads_view_kick", &fls(&[d.ads_kick.pitch.0, d.ads_kick.pitch.1, d.ads_kick.yaw.0, d.ads_kick.yaw.1]))
        .floats("hip_view_kick", &fls(&[d.hip_kick.pitch.0, d.hip_kick.pitch.1, d.hip_kick.yaw.0, d.hip_kick.yaw.1]))
        .floats("view_kick_min", &fls(&[d.view_kick_min.0, d.view_kick_min.1]))
        .floats("view_kick_center", &fls(&[d.view_kick_center.0, d.view_kick_center.1]))
        .floats("ads_gun_kick", &gk(&d.ads_gun_kick))
        .int("ads_gun_kick_reduced_bullets", d.ads_gun_kick.reduced_bullets as i64)
        .floats("hip_gun_kick", &gk(&d.hip_gun_kick))
        .int("hip_gun_kick_reduced_bullets", d.hip_gun_kick.reduced_bullets as i64)
        .floats("gun_max", &fls(&[d.gun_max.0, d.gun_max.1]))
        .floats("ads_recoil_reduction", &fls(&[d.recoil_reduction.0, d.recoil_reduction.1]))
        .float("ads_recoil_return_rate", fl(d.recoil_return_rate));

    // Sway and the gun's position.
    let sw = |s: &weapons::Sway| fls(&[s.max_angle, s.lerp_speed, s.pitch_scale, s.yaw_scale, s.horiz_scale, s.vert_scale]);
    let p = &d.view_pos;
    b.floats("hip_sway", &sw(&d.hip_sway))
        .floats("ads_sway", &sw(&d.ads_sway))
        .absent("idle_amount")
        .floats("stand_move", &fls(&p.stand_move))
        .floats("stand_rot", &fls(&p.stand_rot))
        .floats("strafe_move", &fls(&p.strafe_move))
        .floats("strafe_rot", &fls(&p.strafe_rot))
        .floats("ducked_ofs", &fls(&p.ducked_ofs))
        .floats("ducked_move", &fls(&p.ducked_move))
        .floats("ducked_rot", &fls(&p.ducked_rot))
        .floats("prone_ofs", &fls(&p.prone_ofs))
        .floats("prone_move", &fls(&p.prone_move))
        .floats("prone_rot", &fls(&p.prone_rot))
        .floats("sprint_ofs", &fls(&p.sprint_ofs))
        .floats("sprint_rot", &fls(&p.sprint_rot))
        .floats("pos_rates", &fls(&[p.move_rate, p.prone_move_rate, p.rot_rate, p.prone_rot_rate]))
        .float("ads_aim_pitch", fl(p.ads_aim_pitch))
        .float("ads_bob_factor", fl(p.ads_bob_factor));

    // Crosshair and the overlay.
    let r = &d.reticle;
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let icon_base = crate::generals::base_id(&d.id).to_string();
    let hud_icon = sh
        .has_sprite(&format!("ZHBO_BO2_Icon_{icon_base}"))
        .then(|| sh.bo2.iter().find(|w| w.name == icon_base).and_then(|w| w.hud_icon.clone()))
        .flatten();
    b.opt_text("reticle_center", opt(&r.center))
        .opt_text("reticle_side", opt(&r.side))
        .list("reticle_sizes", &[r.center_size, r.side_size, r.min_ofs].map(|x| format!("{}", x as i64)))
        .floats("reticle_fracs", &fls(&[r.side_pos, r.ads_in_frac, r.ads_out_frac]))
        .bool("keep_crosshair_when_ads", r.keep_when_ads)
        .opt_text("overlay_material", opt(&d.overlay_material))
        .opt_text("overlay_material_low_res", opt(&d.overlay_material_low_res))
        .text("overlay_reticle", enum_name(cols::OVERLAY_RETICLE, d.overlay_reticle))
        .opt_text("hud_icon", hud_icon)
        .absent("dpad_icon");

    // The viewmodel the game draws for this def.
    let vm = j.vm_def.viewmodel;
    let manifest = (vm >= 0).then(|| sh.viewmodel(vm)).flatten();
    // Parts: the arms (no tag) first, the gun on its mount next; the weapon's parts after.
    let (gun, parts): (Option<&zhbo_native::viewmodel::Part>, Vec<&zhbo_native::viewmodel::Part>) = match &manifest {
        Some(m) => {
            let mut rest: Vec<&zhbo_native::viewmodel::Part> = m.parts.iter().skip(2).collect();
            rest.sort_by_key(|p| cols::part_order(&stem(&p.model), p.tag.as_deref().unwrap_or("")));
            (m.parts.get(1), rest)
        }
        None => (None, Vec::new()),
    };
    let lh = parts.iter().find(|p| stem(&p.model).ends_with("_view_lh"));
    let dual = manifest.as_ref().is_some_and(|m| m.clips.iter().any(|(id, _)| id & INSTANCE_LEFT != 0)) || lh.is_some();

    // Alt mode, dual wield, flags, specialties.
    b.opt_text("alt_weapon", (!d.alt.is_empty()).then(|| crate::generals::base_id(&d.alt).to_string()))
        .absent("alt_weapon_ads_only")
        .absent("alt_weapon_disable_switching")
        .bool("dual_wield", dual)
        .absent("dual_wield_weapon")
        .opt_text("dual_wield_view_model", lh.map(|p| stem(&p.model)))
        .bool("silenced", d.silenced)
        .bool("laser_sight", d.laser)
        .absent("infrared")
        .absent("mms_weapon")
        .absent("use_as_melee")
        .bool("cook_off_hold", d.cook)
        .absent("rifle_bullet");
    let bits: Vec<String> = (0..64u32).filter(|i| d.specialties.0 & (1u64 << i) != 0).map(|i| i.to_string()).collect();
    b.set("specialties", &bits);

    // Parts and what they draw.
    let models: Vec<Option<zhbo_native::model::Model>> = manifest.as_ref().map(|m| m.parts.iter().map(|p| sh.model(&p.model)).collect()).unwrap_or_default();
    let mats = |i: usize| -> String {
        models
            .get(i)
            .cloned()
            .flatten()
            .map(|m| m.lod0().iter().filter_map(|s| m.materials.get(s.material as usize).map(|x| x.name.clone())).collect::<Vec<_>>().join(";"))
            .unwrap_or_default()
    };
    match &manifest {
        Some(m) => {
            let skip = 2.min(m.parts.len());
            b.opt_text("view_model", gun.map(|p| stem(&p.model)))
                .list("view_part_models", &parts.iter().filter(|p| !p.ads_only).map(|p| stem(&p.model)).collect::<Vec<_>>())
                .list("view_part_tags", &parts.iter().filter(|p| !p.ads_only).map(|p| p.tag.clone().unwrap_or_default()).collect::<Vec<_>>())
                .floats("view_part_offsets", &parts.iter().filter(|p| !p.ads_only).flat_map(|p| p.offset.1).map(f32_value).collect::<Vec<_>>())
                .floats("view_part_rotations", &parts.iter().filter(|p| !p.ads_only).flat_map(|p| quat(p.offset.0)).collect::<Vec<_>>());
            // The aimed half: the game's own part mask at full ADS.
            let all: Vec<zhbo_native::model::Model> = models.iter().map(|m| m.clone().unwrap_or_default()).collect();
            let overlay = !d.overlay_material.is_empty() && sh.has_sprite(&zhbo_core::lui::materials::image(&d.overlay_material));
            let aimed = zhbo_core::nativevm::visible_parts(&m.parts, &all, 1.0, overlay);
            let mut ads: Vec<String> = m.parts.iter().enumerate().skip(skip).filter(|(i, _)| aimed.get(*i).copied().unwrap_or(true)).map(|(_, p)| stem(&p.model)).collect();
            ads.sort();
            b.list("view_ads_part_models", &ads).set("view_hide_tags", &m.hide);
            let mut hip_idx: Vec<usize> = (2..m.parts.len()).filter(|&i| !m.parts[i].ads_only).collect();
            hip_idx.sort_by_key(|&i| cols::part_order(&stem(&m.parts[i].model), m.parts[i].tag.as_deref().unwrap_or("")));
            hip_idx.insert(0, 1);
            b.list("view_part_materials", &hip_idx.iter().map(|&i| mats(i)).collect::<Vec<_>>());
        }
        None => {
            for c in ["view_model", "view_part_models", "view_part_tags", "view_part_offsets", "view_part_rotations", "view_ads_part_models", "view_hide_tags", "view_part_materials"] {
                b.absent(c);
            }
        }
    }
    match (vm >= 0).then(|| sh.world_weapon(vm)).flatten() {
        Some(w) => {
            let mut rest: Vec<&zhbo_native::viewmodel::Part> = w.parts.iter().skip(1).collect();
            rest.sort_by_key(|p| cols::part_order(&stem(&p.model), p.tag.as_deref().unwrap_or("")));
            b.opt_text("world_model", w.parts.first().map(|p| stem(&p.model)))
                .list("world_part_models", &rest.iter().map(|p| stem(&p.model)).collect::<Vec<_>>())
                .list("world_part_tags", &rest.iter().map(|p| p.tag.clone().unwrap_or_default()).collect::<Vec<_>>())
                .floats("world_part_offsets", &rest.iter().flat_map(|p| p.offset.1).map(f32_value).collect::<Vec<_>>())
                .floats("world_part_rotations", &rest.iter().flat_map(|p| quat(p.offset.0)).collect::<Vec<_>>());
        }
        None => {
            for c in ["world_model", "world_part_models", "world_part_tags", "world_part_offsets", "world_part_rotations"] {
                b.absent(c);
            }
        }
    }

    // Animations: the clip the game shows for each slot (`nativevm::clip_for`), the ADS layer's
    // straight from the manifest (`Loaded::ads_clip` never falls back).
    for (s, name) in cols::ANIM_SLOTS.iter().enumerate().skip(1) {
        let col = format!("anim_{name}");
        // The `_LEFT` slots are the dual-wield left gun's; a single gun has none.
        if cols::is_left_slot(s) && !dual {
            b.absent(&col);
            continue;
        }
        let clip = manifest.as_ref().and_then(|m| {
            let id = clip_id(s, d.clip_base);
            if s == 0x55 || s == 0x56 {
                m.clip(id as u16)
            } else {
                zhbo_core::nativevm::clip_for(m, id)
            }
            .map(stem)
        });
        b.opt_text(&col, clip);
    }

    // Sounds: the player's gunshot event and the FX weapon's third-person event; the loadout's
    // offhand and knife share the global throw/pin/blast/swing events.
    let l = weapons::Loadout::default();
    let key = d.fx_weapon.strip_prefix("ZHBO_FX_").unwrap_or("");
    let npc = (!key.is_empty()).then(|| sh.sound(&format!("ZHBO_BO2_FireNpc_{key}"))).flatten();
    let plr = if d.id == l.lethal { sh.sound(zhbo_core::sounds::THROW) } else { (!d.fire_sound.is_empty()).then(|| sh.sound(&d.fire_sound)).flatten() };
    for c in cols::OVERRIDE_SOUNDS {
        match c {
            "fire_sound" => b.opt_text(c, npc.clone()),
            "fire_sound_player" => b.opt_text(c, plr.clone()),
            _ => b.absent(c),
        };
    }
    // The mod's dry fire is one event for every gun (`sfx::SFX`'s `ZHBO_BO2_DryFire`).
    let dry = crate::sfx::SFX.iter().find(|x| x.event == "ZHBO_BO2_DryFire").and_then(|x| x.layers.first()).map(|a| a.to_string());
    for c in cols::DEF_SOUNDS {
        let v = match c {
            "empty_fire_sound_player" if d.class.is_gun() => dry.clone(),
            "pullback_sound_player" if d.id == l.lethal => sh.sound(zhbo_core::sounds::PULL_PIN),
            "projectile_explosion_sound" if d.id == l.lethal => sh.sound(zhbo_core::sounds::EXPLOSION),
            "melee_swipe_sound_player" if d.id == l.melee => sh.sound(zhbo_core::sounds::MELEE_SWING),
            _ => None,
        };
        b.opt_text(c, v);
    }
    b.opt_text("view_flash_effect", sh.effect(&d.view_flash_fx))
        .absent("world_flash_effect")
        .opt_text("view_shell_eject_effect", sh.effect(&d.view_shell_fx))
        .absent("world_shell_eject_effect")
        .opt_text("view_last_shot_eject_effect", sh.effect(&d.view_last_shot_shell_fx))
        .absent("world_last_shot_eject_effect")
        .opt_text("projectile_explosion_effect", sh.effect(&d.explosion_fx))
        .absent("tracer");

    // Camo: the mod applies the def's one recipe over the whole composed mesh.
    let n = manifest.as_ref().map_or(0, |m| 1 + m.parts.iter().skip(2).filter(|p| !p.ads_only).count());
    b.list("view_part_camos", &vec![d.camo_material.clone(); n]).text("cac_group", &j.group).text("cac_slot", &j.slot).set("cac_requires", &j.requires);
    b.finish()
}

/// The actual rows as a sorted map, for tests.
pub fn by_key(rows: &[Row]) -> BTreeMap<(String, String), &Row> {
    rows.iter().map(|r| (r.key.clone(), r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsets_are_every_combination_up_to_the_cap_smallest_first() {
        let s = subsets(&["a", "b", "c"], 2);
        assert_eq!(s, vec![vec![], vec!["a"], vec!["b"], vec!["c"], vec!["a", "b"], vec!["a", "c"], vec!["b", "c"]]);
        assert_eq!(subsets(&["a"], 3).len(), 2);
        assert_eq!(subsets(&[], 2), vec![Vec::<&str>::new()]);
    }

    #[test]
    fn left_slots_are_the_right_slots_on_the_left_instance() {
        assert_eq!(clip_id(0x51, 0), 0x01 | INSTANCE_LEFT as i32);
        assert_eq!(clip_id(0x10, 0), 0x10);
        assert_eq!(clip_id(0x10, 0x80), 0x90);
        assert_eq!(clip_id(0x54, 0x80), 0x90 | INSTANCE_LEFT as i32);
    }

    #[test]
    fn shorts_sort_in_eattachment_order() {
        let v = bo2_order(&["reflex".into(), "acog".into(), "gl".into(), "zzz".into()]);
        assert_eq!(v, vec!["acog", "gl", "reflex", "zzz"]);
    }
}
