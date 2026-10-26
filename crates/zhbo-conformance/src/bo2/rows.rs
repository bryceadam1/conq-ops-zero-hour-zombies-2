//! `expected.csv`'s weapon rows: every CaC weapon item with every combination a class can carry
//! (G15), each combination's alt mode, and the knives every class gets — each fact computed by
//! BO2's own rule over the player's install (G14).
//!
//! The row set: [`super::combos::items`] × [`super::combos::combos`]; for each, the alt weapon the
//! combination toggles to (`FUN_008ec9e0`) keyed by the same combination; and `knife_mp` (every
//! class, `_class.gsc:586`) and `knife_held_mp` (a class without a primary, `_class.gsc:682`).

use super::combos;
use super::install::Install;
use super::raw::{Attachment, Variant, ANIMS};
use super::resolve::{self, Part, Weapon, ALTMODE, STACKFIRE};
use crate::schema::{f32_value, Row, RowBuilder, Schema};
use crate::weapons::{self as cols, enum_name};
use std::collections::{BTreeMap, HashMap};
use zhbo_formats::t6zone::layouts::WeaponDef as WD;

/// `ATTACHMENT_DW`.
const DW: usize = 4;

/// The always-given melee weapons (`_class.gsc:586`, `:682`).
const KNIVES: [&str; 2] = ["knife_mp", "knife_held_mp"];

/// The def's sound fields behind [`cols::OVERRIDE_SOUNDS`] (same order as the unique's override
/// bits, G14 §4).
const OVERRIDE_SOUND_FIELDS: [u32; 12] = [
    WD::fireSound, WD::fireSoundPlayer, WD::fireLoopSound, WD::fireLoopSoundPlayer, WD::fireLoopEndSound,
    WD::fireLoopEndSoundPlayer, WD::fireStartSound, WD::fireStopSound, WD::fireStartSoundPlayer, WD::fireStopSoundPlayer,
    WD::fireLastSound, WD::fireLastSoundPlayer,
];

/// The def's sound fields behind [`cols::DEF_SOUNDS`].
const DEF_SOUND_FIELDS: [u32; 12] = [
    WD::emptyFireSoundPlayer, WD::pullbackSoundPlayer, WD::meleeSwipeSoundPlayer, WD::meleeHitSound, WD::rechamberSoundPlayer,
    WD::reloadSoundPlayer, WD::reloadEmptySoundPlayer, WD::raiseSoundPlayer, WD::firstRaiseSoundPlayer,
    WD::putawaySoundPlayer, WD::adsZoomSound, WD::projExplosionSound,
];

/// The def's effect fields behind [`cols::EFFECTS`] after the two flashes.
const DEF_EFFECT_FIELDS: [u32; 5] = [WD::viewShellEjectEffect, WD::worldShellEjectEffect, WD::viewLastShotEjectEffect, WD::worldLastShotEjectEffect, WD::projExplosionEffect];

/// One row to build: the variant, its combination, the parent (an alt mode's), the CaC context.
struct Job<'z> {
    w: Weapon<'z>,
    parent: Option<Weapon<'z>>,
    key: String,
    group: String,
    slot: String,
    requires: Vec<&'static str>,
}

pub struct Expected<'a> {
    install: &'a Install,
    variants: BTreeMap<String, Variant<'a>>,
    default_att: Option<Attachment<'a>>,
    materials: std::cell::RefCell<HashMap<String, String>>,
}

impl<'a> Expected<'a> {
    pub fn new(install: &'a Install) -> Expected<'a> {
        let atts = install.attachments();
        Expected { install, variants: install.variants(), default_att: atts.get("defaultattachment").copied(), materials: Default::default() }
    }

    /// A model's LOD0 surface materials, `;`-joined (cached by model).
    fn model_materials(&self, model: &str) -> String {
        if let Some(m) = self.materials.borrow().get(model) {
            return m.clone();
        }
        let text = self
            .install
            .model(model)
            .map(|m| {
                let lod = m.lods.first().map_or(0..0, |l| l.first_surface..l.first_surface + l.surface_count);
                lod.filter_map(|i| m.materials.get(i).cloned().flatten().map(|mat| mat.name)).collect::<Vec<_>>().join(";")
            })
            .unwrap_or_default();
        self.materials.borrow_mut().insert(model.to_string(), text.clone());
        text
    }

    fn jobs(&self) -> Result<Vec<Job<'a>>, String> {
        let stats = self.install.table("mp/statstable.csv")?;
        let att_table = combos::table(&self.install.table("mp/attachmenttable.csv")?);
        let atts = self.install.attachments();
        let mut jobs: Vec<Job> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for item in combos::items(&stats) {
            let Some(&v) = self.variants.get(&item.variant) else { continue };
            let kinds = combos::offered(&item, &v, &atts);
            for c in combos::combos(&item, &kinds, &att_table) {
                // The weapon `GetLoadoutWeapon` (`FUN_00631780`) gives: `<ref>_mp+a+b` for a weapon
                // whose variant has `attachments[0]` (`FUN_008eb260`), but the dual-wield
                // attachment switches to the underscore form, `<ref>_dw_mp`, with no `+` part.
                let (v, kinds, key) = if c.kinds.contains(&DW) {
                    let others: Vec<&str> = c.kinds.iter().filter(|&&k| k != DW).map(|&k| cols::ATTACHMENTS[k]).collect();
                    let name = std::iter::once(item.reference.as_str()).chain(std::iter::once("dw")).chain(others).collect::<Vec<_>>().join("_") + "_mp";
                    let Some(&dv) = self.variants.get(&name) else { continue };
                    (dv, Vec::new(), String::new())
                } else {
                    (v, c.kinds.clone(), c.key())
                };
                let Some(w) = Weapon::new(v, &kinds) else { continue };
                if !seen.insert((v.name(), key.clone())) {
                    continue;
                }
                let alt = alt_weapon(&w);
                jobs.push(Job { w, parent: None, key: key.clone(), group: item.group.clone(), slot: item.slot.clone(), requires: c.requires.clone() });
                // The alt mode keeps the combination's attachment fields (only the variant byte
                // changes, `FUN_008ec9e0`).
                if let Some(alt) = alt.and_then(|a| self.variants.get(&a).copied()) {
                    if let Some(aw) = Weapon::new(alt, &kinds) {
                        if seen.insert((alt.name(), key.clone())) {
                            jobs.push(Job { w: aw, parent: Some(w), key, group: String::new(), slot: String::new(), requires: c.requires.clone() });
                        }
                    }
                }
            }
        }
        for k in KNIVES {
            if let Some(w) = self.variants.get(k).and_then(|v| Weapon::new(*v, &[])) {
                if seen.insert((k.to_string(), String::new())) {
                    jobs.push(Job { w, parent: None, key: String::new(), group: String::new(), slot: String::new(), requires: Vec::new() });
                }
            }
        }
        Ok(jobs)
    }

    /// Every expected row under `schema` (the weapons schema).
    pub fn rows(&self, schema: &Schema) -> Result<Vec<Row>, String> {
        let jobs = self.jobs()?;
        let mut out = Vec::with_capacity(jobs.len());
        let mut errors = Vec::new();
        for j in &jobs {
            match self.row(schema, j) {
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

    fn row(&self, schema: &Schema, j: &Job) -> Result<Row, String> {
        let w = &j.w;
        let (v, d) = (w.v, w.d);
        let da = self.default_att.as_ref();
        let mut b = RowBuilder::new(schema, v.name(), j.key.clone());
        let fl = |x: f32| f32_value(x);
        let fls = |xs: &[f32]| xs.iter().map(|x| f32_value(*x)).collect::<Vec<f64>>();

        // Names and identity.
        b.text("display_name", self.install.localize(&v.display_name().unwrap_or_default()));
        // The HUD lists each field's attachment name, skipping a field the variant has no
        // attachment for (`FUN_00950700`).
        let names: Vec<String> = w.atts().into_iter().flatten().filter_map(|a| a.display_name()).map(|n| self.install.localize(&n)).collect();
        b.list("attachment_names", &names);
        b.text("weap_type", enum_name(cols::WEAP_TYPE, d.weap_type()))
            .text("weap_class", enum_name(cols::WEAP_CLASS, d.weap_class()))
            .text("inventory", enum_name(cols::INVENTORY, d.inventory()))
            .text("offhand_class", enum_name(cols::OFFHAND_CLASS, d.offhand_class()))
            .text("offhand_slot", enum_name(cols::OFFHAND_SLOT, d.offhand_slot()));
        let fire = w.atts_walk().into_iter().find(|a| a.kind() as usize == STACKFIRE).map_or(d.fire_type(), |a| a.fire_type());
        b.text("fire_type", enum_name(cols::FIRE_TYPE, fire))
            .int("shot_count", d.shot_count() as i64)
            .int("shots_before_rechamber", d.shots_before_rechamber() as i64)
            .bool("bolt_action", d.bolt_action());

        // MP melee is the class's melee weapon whatever is held: every class gets `knife_mp`
        // (`_class.gsc:586`) and every `MOD_MELEE` kill but the riot shield's is a stab
        // (`_scoreevents.gsc:375`). A melee or riot-shield weapon melees with itself.
        let md = if d.weap_type() == 7 || d.weap_type() == 8 { d } else { self.variants.get("knife_mp").and_then(|k| k.def()).unwrap_or(d) };

        // Timing.
        let ms = |b: &mut RowBuilder, c: &str, x: i32| {
            b.float(c, x as f64);
        };
        ms(&mut b, "fire_time", resolve::scaled_ms(d.fire_time(), w.scale(|a| a.fire_time_scale())));
        ms(&mut b, "last_fire_time", d.last_fire_time());
        ms(&mut b, "fire_delay", d.fire_delay());
        ms(&mut b, "burst_delay", d.burst_delay());
        ms(&mut b, "rechamber_time", d.rechamber_time());
        ms(&mut b, "rechamber_bolt_time", d.rechamber_bolt_time());
        ms(&mut b, "hold_fire_time", d.hold_fire_time());
        ms(&mut b, "reload_time", resolve::scaled_ms(v.reload_time(), w.scale(|a| a.reload_time_scale())));
        ms(&mut b, "reload_empty_time", resolve::scaled_ms(v.reload_empty_time(), w.scale(|a| a.reload_empty_time_scale())));
        ms(&mut b, "reload_add_time", resolve::scaled_ms(d.reload_add_time(), w.scale(|a| a.reload_add_time_scale())));
        ms(&mut b, "reload_empty_add_time", d.reload_empty_add_time());
        ms(&mut b, "reload_quick_time", resolve::scaled_ms(v.reload_quick_time(), w.scale(|a| a.reload_quick_time_scale())));
        ms(&mut b, "reload_quick_empty_time", resolve::scaled_ms(v.reload_quick_empty_time(), w.scale(|a| a.reload_quick_empty_time_scale())));
        ms(&mut b, "reload_quick_add_time", resolve::scaled_ms(d.reload_quick_add_time(), w.scale(|a| a.reload_quick_add_time_scale())));
        ms(&mut b, "reload_start_time", d.reload_start_time());
        ms(&mut b, "reload_end_time", d.reload_end_time());
        ms(&mut b, "raise_time", d.raise_time());
        ms(&mut b, "first_raise_time", w.first_differing(|u| u.first_raise_time()).unwrap_or(d.first_raise_time()));
        ms(&mut b, "drop_time", d.drop_time());
        ms(&mut b, "alt_raise_time", v.alt_raise_time());
        let alt_drop = if d.inventory() != ALTMODE { w.first_differing(|u| u.alt_drop_time()) } else { None };
        ms(&mut b, "alt_drop_time", alt_drop.unwrap_or(d.alt_drop_time()));
        ms(&mut b, "quick_raise_time", d.quick_raise_time());
        ms(&mut b, "quick_drop_time", d.quick_drop_time());
        ms(&mut b, "empty_raise_time", d.empty_raise_time());
        ms(&mut b, "empty_drop_time", d.empty_drop_time());
        ms(&mut b, "sprint_in_time", d.sprint_in_time());
        ms(&mut b, "sprint_out_time", d.sprint_out_time());
        ms(&mut b, "ads_in_time", resolve::scaled_ms(v.ads_in_time(), w.scale(|a| a.ads_in_scale())));
        ms(&mut b, "ads_out_time", resolve::scaled_ms(v.ads_out_time(), w.scale(|a| a.ads_out_scale())));
        ms(&mut b, "melee_time", md.melee_time());
        ms(&mut b, "melee_delay", md.melee_delay());
        ms(&mut b, "spin_up_time", d.spin_up_time());
        ms(&mut b, "spin_down_time", d.spin_down_time());
        ms(&mut b, "fuse_time", d.fuse_time());
        ms(&mut b, "detonate_time", d.detonate_time());
        b.int("reload_ammo_add", w.first_differing(|u| u.reload_ammo_add()).unwrap_or(d.reload_ammo_add()) as i64)
            .int("reload_start_add", w.first_differing(|u| u.reload_start_add()).unwrap_or(d.reload_start_add()) as i64)
            .bool("segmented_reload", d.segmented_reload() || (!v.ignore_attachments() && w.singles_walk().iter().any(|u| u.segmented_reload())));

        // Ammo.
        let clip = resolve::clip_size(w);
        let rounds = |n: i32| if d.ammo_clip_relative() { clip * n } else { n };
        b.int("clip_size", clip as i64)
            .int("start_ammo_rounds", rounds(d.start_ammo()) as i64)
            .int("max_ammo_rounds", rounds(d.max_ammo()) as i64)
            .bool("clip_only", d.clip_only())
            .bool("dual_mag", v.dual_mag() || (!v.ignore_attachments() && w.atts_walk().iter().any(|a| a.dual_mag())))
            .bool("shared_ammo", d.shared_ammo() || (!v.ignore_attachments() && w.atts_walk().iter().any(|a| a.shared_ammo())));

        // Damage.
        let range_scale = w.scale(|a| a.damage_range_scale());
        b.list("damage", &d.damage().map(|x| x.to_string()))
            .floats("damage_range", &fls(&d.damage_range().map(|r| r * range_scale)))
            .int("min_player_damage", d.min_player_damage() as i64)
            .int("melee_damage", md.melee_damage() as i64);
        let def_mults = d.location_multipliers();
        let mults: Vec<f32> = match w.base() {
            None => def_mults.clone(),
            Some(u0) => {
                let base = u0.location_multipliers();
                (0..def_mults.len())
                    .map(|p| {
                        w.singles_walk().iter().map(|u| u.location_multipliers()).find_map(|m| m.get(p).copied().filter(|x| Some(x) != base.get(p))).unwrap_or(def_mults[p])
                    })
                    .collect()
            }
        };
        b.floats("location_multipliers", &fls(&mults))
            .float("max_range", fl(d.max_range()))
            .text("penetrate_type", enum_name(cols::PENETRATE_TYPE, w.atts_walk().iter().map(|a| a.penetrate_type()).find(|&p| p != 0).unwrap_or(d.penetrate_type())))
            .float("explosion_radius", d.explosion_radius() as f64)
            .float("explosion_radius_min", d.explosion_radius_min() as f64)
            .int("explosion_inner_damage", d.explosion_inner_damage() as i64)
            .int("explosion_outer_damage", d.explosion_outer_damage() as i64)
            .float("projectile_speed", d.projectile_speed() as f64)
            .float("projectile_speed_up", d.projectile_speed_up() as f64)
            .bool("projectile_impact_explode", d.projectile_impact_explode())
            .opt_text("projectile_model", d.projectile_model())
            .text("impact_type", enum_name(cols::IMPACT_TYPE, d.impact_type()));

        // Spread.
        let (smin, smax) = (w.scale(|a| a.hip_spread_min_scale()), w.scale(|a| a.hip_spread_max_scale()));
        b.floats(
            "hip_spread",
            &fls(&[
                d.hip_spread_stand_min() * smin,
                d.hip_spread_stand_max() * smax,
                d.hip_spread_ducked_min() * smin,
                d.hip_spread_ducked_max() * smax,
                d.hip_spread_prone_min() * smin,
                d.hip_spread_prone_max() * smax,
            ]),
        )
        .float("hip_spread_decay", fl(d.hip_spread_decay()))
        .float("hip_spread_fire_add", fl(d.hip_spread_fire_add()))
        .float("hip_spread_move_add", fl(d.hip_spread_move_add()))
        .float("ads_spread", fl(d.ads_spread()))
        .float("stack_fire_spread", fl(w.atts_walk().iter().map(|a| a.stack_fire_spread()).find(|x| *x > 0.0).unwrap_or(d.stack_fire_spread())))
        .float("stack_fire_accuracy_decay", fl(w.atts_walk().iter().map(|a| a.stack_fire_accuracy_decay()).find(|x| *x > 0.0).unwrap_or(d.stack_fire_accuracy_decay())));

        // ADS.
        let fov = [
            resolve::last_differing(w, da, |a| a.ads_zoom_fov1(), v.ads_zoom_fov1()),
            resolve::last_differing(w, da, |a| a.ads_zoom_fov2(), v.ads_zoom_fov2()),
            resolve::last_differing(w, da, |a| a.ads_zoom_fov3(), v.ads_zoom_fov3()),
        ];
        let move_base = if d.ads_move_speed_scale() > 0.0 { d.ads_move_speed_scale() } else { 1.0 };
        b.float("ads_zoom_fov1", fl(fov[0]))
            .float("ads_zoom_fov2", fl(fov[1]))
            .float("ads_zoom_fov3", fl(fov[2]))
            .float("ads_zoom_in_frac", fl(resolve::first_att_differing(w, da, |a| a.ads_zoom_in_frac(), v.ads_zoom_in_frac())))
            .float("ads_zoom_out_frac", fl(resolve::first_att_differing(w, da, |a| a.ads_zoom_out_frac(), v.ads_zoom_out_frac())))
            .float("ads_move_speed_scale", fl(resolve::last_differing(w, da, |a| a.ads_move_speed_scale(), move_base)))
            .float("move_speed_scale", fl(d.move_speed_scale()))
            .float("sprint_duration_scale", fl(d.sprint_duration_scale()))
            .float("alt_scope_ads_in_time", fl(resolve::first_att_differing(w, da, |a| a.alt_scope_ads_in_time(), d.alt_scope_ads_in_time())))
            .float("alt_scope_ads_out_time", fl(resolve::first_att_differing(w, da, |a| a.alt_scope_ads_out_time(), d.alt_scope_ads_out_time())));
        // `FUN_008ebdb0`: each of start/end from the first single (nulls skipped) differing from the
        // base unique, else the def's.
        let mut dof = [d.ads_dof_start(), d.ads_dof_end()];
        if v.attachment(0).is_some() {
            if let Some(u0) = w.base() {
                let singles: Vec<_> = w.singles().into_iter().flatten().collect();
                if let Some(u) = singles.iter().find(|u| u.ads_dof_start() != u0.ads_dof_start()) {
                    dof[0] = u.ads_dof_start();
                }
                if let Some(u) = singles.iter().find(|u| u.ads_dof_end() != u0.ads_dof_end()) {
                    dof[1] = u.ads_dof_end();
                }
            }
        }
        b.floats("ads_dof", &fls(&dof)).float("overlay_alpha_scale", fl(v.overlay_alpha_scale()));

        // Kick and recoil.
        b.floats("ads_view_kick", &fls(&[d.ads_view_kick_pitch_min(), d.ads_view_kick_pitch_max(), d.ads_view_kick_yaw_min(), d.ads_view_kick_yaw_max()]))
            .floats("hip_view_kick", &fls(&[d.hip_view_kick_pitch_min(), d.hip_view_kick_pitch_max(), d.hip_view_kick_yaw_min(), d.hip_view_kick_yaw_max()]))
            .floats("view_kick_min", &fls(&[d.ads_view_kick_min(), d.hip_view_kick_min()]))
            .floats("view_kick_center", &fls(&[v.ads_view_kick_center_speed() * w.scale(|a| a.ads_view_kick_center_speed_scale()), v.hip_view_kick_center_speed()]))
            .floats(
                "ads_gun_kick",
                &fls(&[
                    d.ads_gun_kick_pitch_min(),
                    d.ads_gun_kick_pitch_max(),
                    d.ads_gun_kick_yaw_min(),
                    d.ads_gun_kick_yaw_max(),
                    d.ads_gun_kick_accel(),
                    d.ads_gun_kick_speed_max(),
                    d.ads_gun_kick_speed_decay(),
                    d.ads_gun_kick_static_decay(),
                    d.ads_gun_kick_reduced_percent(),
                ]),
            )
            .int("ads_gun_kick_reduced_bullets", d.ads_gun_kick_reduced_bullets() as i64)
            .floats(
                "hip_gun_kick",
                &fls(&[
                    d.hip_gun_kick_pitch_min(),
                    d.hip_gun_kick_pitch_max(),
                    d.hip_gun_kick_yaw_min(),
                    d.hip_gun_kick_yaw_max(),
                    d.hip_gun_kick_accel(),
                    d.hip_gun_kick_speed_max(),
                    d.hip_gun_kick_speed_decay(),
                    d.hip_gun_kick_static_decay(),
                    d.hip_gun_kick_reduced_percent(),
                ]),
            )
            .int("hip_gun_kick_reduced_bullets", d.hip_gun_kick_reduced_bullets() as i64)
            .floats("gun_max", &fls(&[d.gun_max_pitch(), d.gun_max_yaw()]))
            .floats(
                "ads_recoil_reduction",
                &fls(&[
                    resolve::last_differing(w, da, |a| a.ads_recoil_reduction_rate(), d.ads_recoil_reduction_rate()),
                    resolve::last_differing(w, da, |a| a.ads_recoil_reduction_limit(), d.ads_recoil_reduction_limit()),
                ]),
            )
            .float("ads_recoil_return_rate", fl(d.ads_recoil_return_rate()));

        // Sway and the gun's position.
        let hip_sway = match w.atts_walk().into_iter().find(|a| a.sway_override()) {
            Some(a) => [0x68, 0x6c, 0x70, 0x74, 0x78, 0x7c].map(|o| a.f32_at(o)),
            None => [d.sway_max_angle(), d.sway_lerp_speed(), d.sway_pitch_scale(), d.sway_yaw_scale(), d.sway_horiz_scale(), d.sway_vert_scale()],
        };
        let ads_sway = match w.atts_walk().into_iter().find(|a| a.ads_sway_override()) {
            Some(a) => [0x84, 0x88, 0x8c, 0x90, 0x94, 0x98].map(|o| a.f32_at(o)),
            None => [d.ads_sway_max_angle(), d.ads_sway_lerp_speed(), d.ads_sway_pitch_scale(), d.ads_sway_yaw_scale(), v.ads_sway_horiz_scale(), v.ads_sway_vert_scale()],
        };
        let mut stand_move = d.stand_move();
        let mut stand_rot = d.stand_rot();
        let mut strafe_rot = d.strafe_rot();
        // `FUN_008ed410` / `FUN_008ed4f0`: the view-position overrides, the last attachment
        // differing from `defaultattachment` winning (+0xac stand move forward, +0xb0/+0xb8 stand
        // rotation pitch/roll, +0xa8 strafe rotation roll).
        let over = |o: u32, def: f32| resolve::last_differing(w, da, |a| a.f32_at(o), def);
        stand_move[0] = over(0xac, stand_move[0]);
        stand_rot[0] = over(0xb0, stand_rot[0]);
        stand_rot[2] = over(0xb8, stand_rot[2]);
        strafe_rot[2] = over(0xa8, strafe_rot[2]);
        b.floats("hip_sway", &fls(&hip_sway))
            .floats("ads_sway", &fls(&ads_sway))
            .floats("idle_amount", &fls(&[d.ads_idle_amount() * w.scale(|a| a.ads_idle_amount_scale()), resolve::first_att_differing(w, da, |a| a.hip_idle_amount(), d.hip_idle_amount())]))
            .floats("stand_move", &fls(&stand_move))
            .floats("stand_rot", &fls(&stand_rot))
            .floats("strafe_move", &fls(&d.strafe_move()))
            .floats("strafe_rot", &fls(&strafe_rot))
            .floats("ducked_ofs", &fls(&d.ducked_ofs()))
            .floats("ducked_move", &fls(&d.ducked_move()))
            .floats("ducked_rot", &fls(&d.ducked_rot()))
            .floats("prone_ofs", &fls(&d.prone_ofs()))
            .floats("prone_move", &fls(&d.prone_move()))
            .floats("prone_rot", &fls(&d.prone_rot()))
            .floats("sprint_ofs", &fls(&d.sprint_ofs()))
            .floats("sprint_rot", &fls(&d.sprint_rot()))
            .floats("pos_rates", &fls(&[d.pos_move_rate(), d.pos_prone_move_rate(), d.pos_rot_rate(), d.pos_prone_rot_rate()]))
            .float("ads_aim_pitch", fl(d.ads_aim_pitch()))
            .float("ads_bob_factor", fl(d.ads_bob_factor()));

        // Crosshair and the overlay.
        let overlay = match w.base() {
            None => (v.overlay_material(), v.overlay_material_low_res(), d.overlay_reticle()),
            Some(_) => (
                w.first_differing(|u| u.overlay_material()).unwrap_or_else(|| v.overlay_material()),
                w.first_differing(|u| u.overlay_material_low_res()).unwrap_or_else(|| v.overlay_material_low_res()),
                w.first_differing(|u| u.overlay_reticle()).unwrap_or(d.overlay_reticle()),
            ),
        };
        b.opt_text("reticle_center", d.reticle_center())
            .opt_text("reticle_side", d.reticle_side())
            .list("reticle_sizes", &[d.reticle_center_size(), d.reticle_side_size(), d.reticle_min_ofs()].map(|x| x.to_string()))
            .floats("reticle_fracs", &fls(&[d.hip_reticle_side_pos(), d.ads_crosshair_in_frac(), d.ads_crosshair_out_frac()]))
            .bool("keep_crosshair_when_ads", d.keep_crosshair_when_ads())
            .opt_text("overlay_material", overlay.0)
            .opt_text("overlay_material_low_res", overlay.1)
            .text("overlay_reticle", enum_name(cols::OVERLAY_RETICLE, overlay.2))
            .opt_text("hud_icon", d.hud_icon())
            .opt_text("dpad_icon", v.dpad_icon());

        // Alt mode, dual wield, flags, specialties.
        let dw = resolve::dual_wield(w);
        let left_name = dw.then(|| dual_wield_weapon(w)).flatten();
        let left = left_name.as_ref().and_then(|n| self.variants.get(n).copied());
        b.opt_text("alt_weapon", alt_weapon(w))
            .bool("alt_weapon_ads_only", resolve::alt_ads_only(w))
            .bool("alt_weapon_disable_switching", resolve::alt_disable_switching(w))
            .bool("dual_wield", dw)
            .opt_text("dual_wield_weapon", left_name.clone())
            .opt_text("dual_wield_view_model", left.and_then(|l| l.def()).and_then(|d| d.view_model()))
            .bool("silenced", v.silenced() || (!v.ignore_attachments() && w.atts_walk().iter().any(|a| a.silenced())))
            .bool("laser_sight", d.laser_sight() || (!v.ignore_attachments() && w.atts_walk().iter().any(|a| a.laser_sight())))
            .bool("infrared", v.infrared() || w.atts_walk().iter().any(|a| a.infrared()))
            .bool("mms_weapon", v.mms_weapon() || w.atts().into_iter().flatten().any(|a| a.mms_weapon()))
            .bool("use_as_melee", d.has_bayonet() || d.use_as_melee() || w.atts_walk().iter().any(|a| a.use_as_melee()))
            .bool("cook_off_hold", d.cook_off_hold())
            .bool("rifle_bullet", d.rifle_bullet());
        let (mut p0, mut p1) = v.perks();
        for a in w.atts_walk() {
            let (a0, a1) = a.perks();
            p0 |= a0;
            p1 |= a1;
        }
        let bits: Vec<String> = (0..64).filter(|i| if *i < 32 { p0 & (1 << i) != 0 } else { p1 & (1 << (i - 32)) != 0 }).map(|i: u32| i.to_string()).collect();
        b.set("specialties", &bits);

        // Parts: an alt mode draws its parent's (`FUN_00535170`).
        let pw = j.parent.as_ref().unwrap_or(w);
        let gun = d.view_model();
        let hip = resolve::view_parts(pw, false);
        let ads = resolve::view_parts(pw, true);
        part_cols(&mut b, "view_part", &hip);
        let mut ads_models: Vec<String> = ads.iter().map(|p| p.model.clone()).collect();
        ads_models.sort();
        b.opt_text("view_model", gun.clone()).list("view_ads_part_models", &ads_models);
        let mut hip = hip;
        hip.sort_by_key(|p| cols::part_order(&p.model, &p.tag));
        // An alt mode's hide bits are its parent's too (`FUN_00535540` swaps an ALTMODE weapon for
        // `ps+0x1c0` before `CG_SetWeaponHidePartBits`).
        b.set("view_hide_tags", &resolve::hide_tags(pw));
        let mats: Vec<String> = gun.iter().cloned().chain(hip.iter().map(|p| p.model.clone())).map(|m| self.model_materials(&m)).collect();
        b.list("view_part_materials", &mats);
        b.opt_text("world_model", d.world_model());
        part_cols(&mut b, "world_part", &resolve::world_parts(pw));

        // Animations.
        let table = resolve::anims(w, left);
        for (s, name) in cols::ANIM_SLOTS.iter().enumerate().skip(1) {
            let clip = if cols::is_left_slot(s) && !dw { None } else { table.get(s).cloned().flatten() };
            b.opt_text(&format!("anim_{name}"), clip);
        }
        debug_assert_eq!(table.len(), ANIMS);

        // Sounds and effects.
        for (i, (c, f)) in cols::OVERRIDE_SOUNDS.iter().zip(OVERRIDE_SOUND_FIELDS).enumerate() {
            b.opt_text(c, resolve::sound(w, i as u32, d.sound(f)));
        }
        for (c, f) in cols::DEF_SOUNDS.iter().zip(DEF_SOUND_FIELDS) {
            // A melee's swing and hit are the melee weapon's (see `md`).
            let from = if f == WD::meleeSwipeSoundPlayer || f == WD::meleeHitSound { md } else { d };
            b.opt_text(c, from.sound(f));
        }
        b.opt_text("view_flash_effect", resolve::effect(w, 0, d.effect(WD::viewFlashEffect)))
            .opt_text("world_flash_effect", resolve::effect(w, 1, d.effect(WD::worldFlashEffect)));
        for (c, f) in cols::EFFECTS[2..].iter().zip(DEF_EFFECT_FIELDS) {
            b.opt_text(c, d.effect(f));
        }
        // `FUN_008eb420` in the shooter's own view: the first single with a tracer, else the def's.
        b.opt_text("tracer", w.singles_walk().iter().find_map(|u| u.tracer()).or_else(|| d.tracer()));

        let camos: Vec<String> = std::iter::once(d.weapon_camo()).chain(hip.iter().map(|p| p.camo.clone())).map(Option::unwrap_or_default).collect();
        b.list("view_part_camos", &camos).text("cac_group", &j.group).text("cac_slot", &j.slot).set("cac_requires", &j.requires);
        b.finish()
    }
}

fn part_cols(b: &mut RowBuilder, prefix: &str, parts: &[Part]) {
    let mut parts = parts.to_vec();
    parts.sort_by_key(|p| cols::part_order(&p.model, &p.tag));
    b.list(&format!("{prefix}_models"), &parts.iter().map(|p| p.model.clone()).collect::<Vec<_>>())
        .list(&format!("{prefix}_tags"), &parts.iter().map(|p| p.tag.clone()).collect::<Vec<_>>())
        .floats(&format!("{prefix}_offsets"), &parts.iter().flat_map(|p| p.offset).map(f32_value).collect::<Vec<_>>())
        .floats(&format!("{prefix}_rotations"), &parts.iter().flat_map(|p| angles_to_quat(p.rotation)).collect::<Vec<_>>());
}

/// BO2's pitch/yaw/roll (degrees) as a unit quaternion `x, y, z, w` with `w >= 0`: the engine's
/// `AnglesToAxis` (forward `(cp·cy, cp·sy, −sp)`, left `(sr·sp·cy − cr·sy, sr·sp·sy + cr·cy, sr·cp)`,
/// up `(cr·sp·cy + sr·sy, cr·sp·sy − sr·cy, cr·cp)`) and the standard matrix-to-quaternion step.
pub fn angles_to_quat(a: [f32; 3]) -> [f64; 4] {
    let r = |d: f32| (d as f64).to_radians();
    let (sp, cp) = r(a[0]).sin_cos();
    let (sy, cy) = r(a[1]).sin_cos();
    let (sr, cr) = r(a[2]).sin_cos();
    let f = [cp * cy, cp * sy, -sp];
    let l = [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp];
    let u = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    // Columns of the rotation matrix are the axes: m[row][col].
    let m = [[f[0], l[0], u[0]], [f[1], l[1], u[1]], [f[2], l[2], u[2]]];
    let tr = m[0][0] + m[1][1] + m[2][2];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [(m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s]
    };
    canonical_quat(q)
}

/// A unit quaternion with `w >= 0` (q and −q are the same rotation), components rounded to 1e-6
/// so `-0` and noise do not print. The sign is chosen on the rounded components: a half-turn's
/// `w` of −1e-9 rounds to 0 and must not flip the axis (`1|0|0|0` vs `-1|0|0|0`).
pub fn canonical_quat(q: [f64; 4]) -> [f64; 4] {
    let n = q.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-12);
    let r = q.map(|c| (c / n * 1e6).round() / 1e6);
    let sign = if r[3] < 0.0 || (r[3] == 0.0 && r.iter().find(|c| **c != 0.0).is_some_and(|c| *c < 0.0)) { -1.0 } else { 1.0 };
    r.map(|c| c * sign + 0.0)
}

/// `FUN_008ec9e0`: the weapon the combination toggles to. The `*Index` fields are resolved from
/// the names when the zone loads, so the names stand for them here.
pub fn alt_weapon(w: &Weapon) -> Option<String> {
    if w.d.inventory() == ALTMODE {
        return w.v.alt_weapon_name();
    }
    if w.base().is_some() {
        if let Some(n) = w.singles_walk().iter().find_map(|u| u.alt_weapon_name()) {
            return Some(n);
        }
    }
    w.v.alt_weapon_name()
}

/// `FUN_008eca70`: the dual-wield left gun.
pub fn dual_wield_weapon(w: &Weapon) -> Option<String> {
    if w.d.inventory() == ALTMODE {
        return w.d.dual_wield_weapon_name();
    }
    if w.base().is_some() {
        if let Some(n) = w.singles_walk().iter().find_map(|u| u.dual_wield_weapon_name()) {
            return Some(n);
        }
    }
    w.d.dual_wield_weapon_name()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_become_the_id_tech_rotation() {
        assert_eq!(angles_to_quat([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0, 1.0]);
        // Yaw 90: +X turns to +Y — a rotation of 90 about +Z.
        let q = angles_to_quat([0.0, 90.0, 0.0]);
        assert!((q[2] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6 && (q[3] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6, "{q:?}");
        // Pitch 90 looks down: forward becomes −Z — +90 about +Y.
        let q = angles_to_quat([90.0, 0.0, 0.0]);
        assert!((q[1] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6, "{q:?}");
        // Roll 90 about +X; and 360° is the identity.
        let q = angles_to_quat([0.0, 0.0, 90.0]);
        assert!((q[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6, "{q:?}");
        assert_eq!(angles_to_quat([0.0, 360.0, 0.0]), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_half_turn_keeps_one_sign_whatever_the_noise_in_w() {
        // q and −q are the same half-turn; a `w` that rounds to zero must not pick the sign.
        assert_eq!(canonical_quat([1.0, 0.0, 0.0, -1e-9]), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(canonical_quat([-1.0, 0.0, 0.0, 1e-9]), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(canonical_quat([-1.0, 0.0, 0.0, -1e-9]), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(canonical_quat([0.0, 0.0, 0.0, -1.0]), [0.0, 0.0, 0.0, 1.0]);
    }
}
