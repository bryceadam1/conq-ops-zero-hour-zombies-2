//! What others see of a shot besides the muzzle flash: BO2's third-person shell eject and its
//! bullet tracer, as Zero Hour FX weapons the sim fires (`zhbo_core::sim::fire`).
//!
//! * **Brass** (`FUN_00531630`): everyone but the shooter's own first-person view sees the def's
//!   `worldShellEjectEffect` (the round that empties the clip: `worldLastShotEjectEffect`) at the
//!   world model's `tag_brass`. Zero Hour plays an FX weapon's `FireFX` at the soldier rig's flash
//!   bone (`generals::flash_bone`, the world model's `tag_flash`), so the shell FXList is placed
//!   from there: offset and turned by `tag_brass` relative to `tag_flash` on the world model.
//! * **Tracer** (`FUN_008eb420`, `FUN_007c65c0`, `Tracer_Spawn` `FUN_007c6150`): the first single
//!   unique's `tracerType`, else the def's, drawn every `drawInterval`-th bullet by the sim's
//!   countdown, as a Zero Hour `Tracer` nugget flying from the flash bone to the bullet's end.

use std::collections::BTreeMap;

use zhbo_core::weapons::WeaponDef;
use zhbo_formats::t6zone::Zone;
use zhbo_formats::weapon::{Tracer, Weapon};

use crate::fx::Converted;
use crate::xf::{self, Xf};

/// The FX weapon that plays a base weapon's third-person brass.
pub fn brass_fx_weapon(base: &str) -> String {
    format!("ZHBO_FX_Brass_{base}")
}

/// The FX weapon that plays the last round's brass when it differs from every shot's.
pub fn last_brass_fx_weapon(base: &str) -> String {
    format!("ZHBO_FX_BrassLast_{base}")
}

/// The FX weapon that draws a BO2 tracer.
pub fn tracer_fx_weapon(tracer: &str) -> String {
    format!("ZHBO_FX_Tracer_{tracer}")
}

/// The placed brass FXList of a base weapon (`last`: the last-round one).
pub fn brass_fxlist(base: &str, last: bool) -> String {
    if last { format!("ZHBO_BO2FX_BrassLast_{base}") } else { format!("ZHBO_BO2FX_Brass_{base}") }
}

/// A client-only FX weapon block: no damage, its `FireFX` at the shooter's flash bone, silent.
fn fx_weapon_block(name: &str, fire_fx: &str) -> String {
    format!(
        "Weapon {name}\n  PrimaryDamage           = 0.0\n  PrimaryDamageRadius     = 0.0\n  AttackRange             = 1000.0\n  \
         DamageType              = SMALL_ARMS\n  DeathType               = NORMAL\n  WeaponSpeed             = 999999.0\n  \
         ProjectileObject        = NONE\n  FireFX                  = {fire_fx}\n  FireSound               = NONE\n  \
         RadiusDamageAffects     = ENEMIES\n  DelayBetweenShots       = 0\n  ClipSize                = 0\nEnd\n\n"
    )
}

/// The tracer a weapon `w` with the attachments `shorts` draws for a friendly shooter
/// (`FUN_008eb420`): the first single unique (in the combination's order) with one, the base
/// unique standing in for an attachment without its own single, else the def's.
pub fn resolved_tracer<'a>(w: &'a Weapon, shorts: &[&str]) -> Option<&'a Tracer> {
    let base = w.attachment_uniques.iter().find(|u| u.kind == 0 && u.combined == 0);
    for short in shorts {
        let Some((kind, _)) = w.attachment_variants.iter().find(|(_, id)| crate::weapons::short_name(id) == *short) else { continue };
        let single = w.attachment_uniques.iter().find(|u| u.kind == *kind && u.combined == 0).or(base);
        if let Some(t) = single.and_then(|u| u.tracer.as_ref()) {
            return Some(t);
        }
    }
    w.tracer.as_ref()
}

/// `tag_brass` relative to `tag_flash` on a world model, in Zero Hour units (the soldier rig's
/// scale), or `None` when the model lacks either tag (BO2 plays nothing at a missing tag).
pub fn brass_from_flash(model: &zhbo_formats::xmodel::Model) -> Option<Xf> {
    let tag = |n: &str| model.bones.iter().find(|b| b.name == n).map(|b| crate::character::scaled_xf((b.rotation, b.position)));
    Some(xf::compose(xf::inverse(tag("tag_flash")?), tag("tag_brass")?))
}

/// Zero Hour's `RotateX`, `RotateY`, `RotateZ` (degrees, post-multiplied in that order onto the
/// spawner's matrix: `M * Rx * Ry * Rz`) for a rotation quaternion `(x, y, z, w)`.
pub fn rotate_xyz(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = xf::qnorm(q);
    // The rotation matrix (column vectors) of q.
    let r02 = 2.0 * (x * z + w * y);
    let r12 = 2.0 * (y * z - w * x);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    let r01 = 2.0 * (x * y - w * z);
    let r00 = 1.0 - 2.0 * (y * y + z * z);
    // R = Rx(a) Ry(b) Rz(c): r02 = sin b, r12 = -sin a cos b, r22 = cos a cos b,
    // r01 = -cos b sin c, r00 = cos b cos c.
    let b = r02.clamp(-1.0, 1.0).asin();
    let (a, c) = if r02.abs() < 0.9999 {
        ((-r12).atan2(r22), (-r01).atan2(r00))
    } else {
        // Gimbal lock: fold the X turn into Z.
        let r10 = 2.0 * (x * y + w * z);
        let r11 = 1.0 - 2.0 * (x * x + z * z);
        (0.0, r10.atan2(r11))
    };
    [a.to_degrees(), b.to_degrees(), c.to_degrees()]
}

/// A tracer material's hue: its colour map's alpha-weighted mean, scaled so the brightest channel
/// is 1 (BO2's tracer colours carry the brightness). White when the material has no colour map.
pub fn material_tint(zones: &[&Zone], material: Option<&str>, paks: &dyn Fn(u32) -> Option<zhbo_formats::iwi::Rgba>) -> [f32; 3] {
    let white = [1.0; 3];
    let Some(name) = material else { return white };
    let Some(img) = zones.iter().find_map(|z| zhbo_formats::xmodel::find_material(z, name).and_then(|m| m.color_map().cloned())) else { return white };
    let Some(rgba) = paks(img.stream_hash) else { return white };
    let mut sum = [0.0f64; 3];
    for p in rgba.pixels.as_chunks::<4>().0 {
        let a = p[3] as f64 / 255.0;
        for (s, c) in sum.iter_mut().zip(p) {
            *s += *c as f64 * a;
        }
    }
    let max = sum.iter().cloned().fold(0.0, f64::max);
    if max <= 0.0 {
        return white;
    }
    [(sum[0] / max) as f32, (sum[1] / max) as f32, (sum[2] / max) as f32]
}

/// Sets every def's `world_shell_fx_weapon`, `world_last_shot_shell_fx_weapon`,
/// `tracer_fx_weapon` and `tracer_interval` (the combinations and alt modes too: brass is the
/// def's, the tracer is resolved per combination), converting the effects into `effects`.
/// Returns the FX weapons' INI and, per base weapon id, its placed brass FXList (for the overhead
/// General's combined `FireFX`, [`rts_fire_fx`]).
pub fn convert(
    defs: &mut [WeaponDef],
    bo2: &[Weapon],
    zones: &[&Zone],
    effects: &mut Converted,
    paks: &dyn Fn(u32) -> Option<zhbo_formats::iwi::Rgba>,
) -> (String, BTreeMap<String, String>) {
    let mut ini = String::from("; Generated by zhbo-assets: what others see of a shot (shotfx.rs): BO2's world brass and tracers.\n\n");
    // Per BO2 weapon: (every shot's FX weapon, the last round's).
    let mut brass: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut placed: BTreeMap<String, String> = BTreeMap::new();
    let mut tracers: BTreeMap<String, String> = BTreeMap::new();
    let (patch, common) = (zones[0], zones.get(1).copied().unwrap_or(zones[0]));
    for d in defs.iter_mut() {
        let base = crate::viewmodels::base_id(&d.id);
        let Some(w) = bo2.iter().find(|w| w.name == base) else { continue };
        let fx = brass.entry(base.to_string()).or_insert_with(|| {
            let Some(rel) = w.world_model.as_deref().and_then(|m| crate::viewmodels::find_model(patch, common, m).ok()).and_then(|m| brass_from_flash(&m)) else {
                return Default::default();
            };
            let mut one = |e: &zhbo_formats::fx::Effect, last: bool| -> Option<String> {
                let shell = effects.add_shell(e, zones, paks);
                let list = effects.placed(&shell, &brass_fxlist(base, last), rel.1, rotate_xyz(rel.0))?;
                let name = if last { last_brass_fx_weapon(base) } else { brass_fx_weapon(base) };
                ini.push_str(&fx_weapon_block(&name, &list));
                if !last {
                    placed.insert(base.to_string(), list);
                }
                Some(name)
            };
            let every = w.world_shell_eject_effect.as_ref().and_then(|e| one(e, false)).unwrap_or_default();
            // BO2 takes the last-round effect only when both last fields are set (`FUN_00531630`);
            // the same effect plays from the same FX weapon.
            let last = match (&w.world_last_shot_eject_effect, &w.world_shell_eject_effect) {
                (Some(l), Some(e)) if l.name == e.name => every.clone(),
                (Some(l), _) => one(l, true).unwrap_or_else(|| every.clone()),
                _ => every.clone(),
            };
            (every, last)
        });
        (d.world_shell_fx_weapon, d.world_last_shot_shell_fx_weapon) = fx.clone();
        let (_, shorts) = zhbo_core::attachments::split_id(d.id.split('~').next().unwrap_or(&d.id));
        let Some(t) = resolved_tracer(w, &shorts).filter(|t| t.draw_interval > 0) else { continue };
        let name = tracers.entry(t.name.clone()).or_insert_with(|| {
            let tint = material_tint(zones, t.material.as_deref(), paks);
            let list = effects.tracer(t, tint, 1.0);
            let name = tracer_fx_weapon(&t.name);
            ini.push_str(&fx_weapon_block(&name, &list));
            name
        });
        d.tracer_fx_weapon = name.clone();
        d.tracer_interval = t.draw_interval;
    }
    (ini, placed)
}

/// The overhead General's `FireFX` for base weapon `id`: no sim drives his shots (Zero Hour's own
/// weapon does), so his flash, his placed brass and his tracer are one FXList, the tracer drawn
/// with probability `1 / drawInterval` (Zero Hour's client random; the cadence the sim keeps
/// exactly for first-person Generals). `None` when he has no flash to build on.
pub fn rts_fire_fx(
    id: &str,
    flash: &str,
    bo2: &[Weapon],
    placed: &BTreeMap<String, String>,
    zones: &[&Zone],
    effects: &mut Converted,
    paks: &dyn Fn(u32) -> Option<zhbo_formats::iwi::Rgba>,
) -> Option<String> {
    let w = bo2.iter().find(|w| w.name == id)?;
    let mut extra = String::new();
    if let Some(t) = w.tracer.as_ref().filter(|t| t.draw_interval > 0) {
        let tint = material_tint(zones, t.material.as_deref(), paks);
        let list = effects.tracer(t, tint, 1.0 / t.draw_interval as f32);
        extra = effects.body(&list).unwrap_or_default().to_string();
    }
    let brass = placed.get(id).map(String::as_str).unwrap_or("");
    if brass.is_empty() && extra.is_empty() {
        return None;
    }
    effects.compose(&format!("ZHBO_BO2FX_Rts_{id}"), &[flash, brass], &extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quat_of(deg: [f32; 3]) -> [f32; 4] {
        // M * Rx(a) * Ry(b) * Rz(c) as one quaternion.
        let h = |d: f32| (d.to_radians() * 0.5).sin_cos();
        let (sa, ca) = h(deg[0]);
        let (sb, cb) = h(deg[1]);
        let (sc, cc) = h(deg[2]);
        xf::qmul(xf::qmul([sa, 0.0, 0.0, ca], [0.0, sb, 0.0, cb]), [0.0, 0.0, sc, cc])
    }

    #[test]
    fn rotate_xyz_inverts_zero_hours_rotation_order() {
        for deg in [[0.0, 0.0, 0.0], [90.0, 0.0, 0.0], [10.0, 20.0, 30.0], [-45.0, 60.0, 170.0], [0.0, -30.0, 90.0]] {
            let back = rotate_xyz(quat_of(deg));
            let (a, b) = (quat_of(back), quat_of(deg));
            let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs();
            assert!(dot > 0.9999, "{deg:?} -> {back:?}");
        }
        assert!(rotate_xyz([0.0, 0.0, 0.0, 1.0]).iter().all(|a| a.abs() < 1e-4));
    }

    #[test]
    fn a_combination_draws_its_first_single_uniques_tracer() {
        use zhbo_formats::weapon::AttachmentUnique;
        let t = |n: &str| Tracer { name: n.into(), draw_interval: 1, ..Default::default() };
        let w = Weapon {
            name: "mk48_mp".into(),
            tracer: Some(t("lmg")),
            attachment_variants: vec![(5, "ir_lmg".into()), (9, "grip_lmg".into())],
            attachment_uniques: vec![
                AttachmentUnique { name: "au_mk48_ir".into(), kind: 5, tracer: Some(t("lmg_scope")), ..Default::default() },
                AttachmentUnique { name: "au_mk48_grip".into(), kind: 9, ..Default::default() },
            ],
            ..Default::default()
        };
        let short = |id: &str| crate::weapons::short_name(id);
        let (ir, grip) = (short("ir_lmg"), short("grip_lmg"));
        assert_eq!(resolved_tracer(&w, &[]).map(|t| t.name.as_str()), Some("lmg"));
        assert_eq!(resolved_tracer(&w, &[grip.as_str()]).map(|t| t.name.as_str()), Some("lmg"));
        assert_eq!(resolved_tracer(&w, &[grip.as_str(), ir.as_str()]).map(|t| t.name.as_str()), Some("lmg_scope"));
    }
}
