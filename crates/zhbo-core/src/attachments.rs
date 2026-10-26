//! Black Ops II attachments: how each one changes the weapon it goes on, and building a weapon with
//! attachments (`an94_mp+grip+reflex`) from its base definition.
//!
//! The numbers are BO2's own `WeaponAttachment` scales, converted from the player's install into the
//! weapon table (`Attachment <id>` blocks). Each weapon lists the attachments create-a-class offers
//! on it and which variant it uses (an SMG's suppressor is `silencer_smg`), and the alt modes some of
//! them add (select fire, the under-barrel grenade launcher, the second optic), which are weapons of
//! their own in BO2.
//!
//! What BO2 does in script or the renderer rather than in these numbers is done by name: FMJ's
//! penetration and extra damage to vehicles, the grip's recoil reduction (its ADS recoil reduction
//! fields), the tactical knife's quicker melee, and the target finder marking enemies while aiming.

use crate::weapons::{FireMode, Penetrate, Specialties, WeaponDef};

/// BO2's attachment points (`eAttachmentPoint`): one attachment per point.
pub mod point {
    pub const OPTIC: u8 = 1;
}

/// One BO2 attachment. A scale of 1 leaves the value alone.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentDef {
    /// BO2's asset name, e.g. `silencer_smg`.
    pub id: String,
    /// The name create-a-class and weapon ids use, e.g. `silencer`.
    pub short: String,
    /// As BO2's menus show it, e.g. `Suppressor`.
    pub display_name: String,
    /// The full name BO2's attachment menu's lower title shows, e.g. `Reflex Sight`.
    pub full_name: String,
    /// The attachment's description line, from its `MPUI_*_DESC` key.
    pub description: String,
    /// BO2's attribute-table deltas for the stat bars (percent 0..100), in the table's order:
    /// damage, range, accuracy, mobility, fire rate (`mp/attributestable.csv`).
    pub attribute_deltas: [f32; 5],
    pub point: u8,
    pub silenced: bool,
    pub laser: bool,
    /// Select fire, stacked fire: the weapon fires this way instead.
    pub fire: Option<FireMode>,
    /// A fixed magazine (the crossbow's tri-bolt), 0 for none.
    pub clip: u16,
    pub damage_range_scale: f32,
    /// Zoomed field of view of an optic (degrees), 0 for none.
    pub ads_fov: f32,
    pub ads_in_scale: f32,
    pub ads_out_scale: f32,
    pub ads_move_scale: f32,
    pub hip_spread_min_scale: f32,
    pub hip_spread_max_scale: f32,
    pub fire_time_scale: f32,
    pub reload_scale: f32,
    pub reload_empty_scale: f32,
    pub reload_add_scale: f32,
    pub clip_scale: f32,
    /// ADS recoil reduction per shot and its limit (the grip).
    pub recoil_reduction: (f32, f32),
    /// The specialties the attachment grants (`WeaponAttachment.perks`: fast mag's `fastreload`,
    /// quickdraw's `fastads`, the laser's `bulletaccuracy`, rapid fire's `rof`).
    pub specialties: Specialties,
    /// `eAttachment`: BO2 walks a combination's attachments in this order (G14 §1).
    pub kind: u8,
    /// `fReloadQuick{,Empty,Add}TimeScale`: scale the quick reload times (products).
    pub reload_quick_scale: (f32, f32, f32),
    /// `fAdsIdleAmountScale`: scales the aimed idle sway (a product).
    pub ads_idle_scale: f32,
    /// The fields BO2 takes from the first (or, for the zoom levels, last) attachment whose value
    /// differs from `defaultattachment`'s; `None` where this attachment's equals it (no change).
    /// `fHipIdleAmount`.
    pub hip_idle: Option<f32>,
    /// `fAdsZoomFov2`, `fAdsZoomFov3` (the last differing wins).
    pub fov2: Option<f32>,
    pub fov3: Option<f32>,
    /// `fAdsZoomInFrac`, `fAdsZoomOutFrac`.
    pub zoom_in_frac: Option<f32>,
    pub zoom_out_frac: Option<f32>,
    /// `altScopeADSTransIn/OutTime` (seconds).
    pub alt_scope_in_time: Option<f32>,
    pub alt_scope_out_time: Option<f32>,
    /// `stackFireSpread`, `stackFireAccuracyDecay` (the first positive wins).
    pub stack_fire: (f32, f32),
    /// `penetrateType` (the first non-`None` wins).
    pub penetrate: Penetrate,
    /// OR-ed into the weapon's: `bDualMag`, `bInfraRed`, `mmsWeapon`, `bUseAsMelee`,
    /// `bAltWeaponAdsOnly`, `bAltWeaponDisableSwitching`.
    pub dual_mag: bool,
    pub infrared: bool,
    pub mms: bool,
    pub use_as_melee: bool,
    pub alt_ads_only: bool,
    pub alt_disable_switching: bool,
}

impl Default for AttachmentDef {
    fn default() -> Self {
        AttachmentDef {
            id: String::new(),
            short: String::new(),
            display_name: String::new(),
            full_name: String::new(),
            description: String::new(),
            attribute_deltas: [0.0; 5],
            point: 0,
            silenced: false,
            laser: false,
            fire: None,
            clip: 0,
            damage_range_scale: 1.0,
            ads_fov: 0.0,
            ads_in_scale: 1.0,
            ads_out_scale: 1.0,
            ads_move_scale: 1.0,
            hip_spread_min_scale: 1.0,
            hip_spread_max_scale: 1.0,
            fire_time_scale: 1.0,
            reload_scale: 1.0,
            reload_empty_scale: 1.0,
            reload_add_scale: 1.0,
            clip_scale: 1.0,
            recoil_reduction: (0.0, 0.0),
            specialties: Specialties::NONE,
            kind: 0,
            reload_quick_scale: (1.0, 1.0, 1.0),
            ads_idle_scale: 1.0,
            hip_idle: None,
            fov2: None,
            fov3: None,
            zoom_in_frac: None,
            zoom_out_frac: None,
            alt_scope_in_time: None,
            alt_scope_out_time: None,
            stack_fire: (0.0, 0.0),
            penetrate: Penetrate::None,
            dual_mag: false,
            infrared: false,
            mms: false,
            use_as_melee: false,
            alt_ads_only: false,
            alt_disable_switching: false,
        }
    }
}

/// FMJ: bullets go through the first infantry target, and hit vehicles and structures harder (BO2's
/// FMJ does extra damage to scorestreaks and equipment).
pub const FMJ_VEHICLE_DAMAGE: f32 = 1.35;
/// Tactical knife: melee this much quicker.
pub const TACTICAL_KNIFE_MELEE: f32 = 0.6;

/// A scale of 1 leaves the value alone; a 0 multiplies by 0 (G14 §4). BO2 authors an unused scale
/// as 1.0, not 0.0: the install's real attachment rows all carry 1.0 in the columns they do not
/// change (only `,defaultattachment`, filtered from the table, carries 0.0).
fn scale(s: f32) -> f32 {
    s
}

/// Truncate a seconds value to whole milliseconds, as BO2's time products do (G14 §4). The
/// definition stores seconds as `f32`, so snap a value within `MS_EPS` (1e-3 ms) of a whole
/// millisecond first: the seconds round trip must not turn 700 ms into 699 ms. The authored scales
/// have three decimals and the base times are whole ms, so a real product cannot land that close to
/// an integer on the wrong side.
fn trunc_ms(s: f32) -> f32 {
    let ms = s * 1000.0;
    let ms = if (ms - ms.round()).abs() < 1e-3 { ms.round() } else { ms.trunc() };
    ms / 1000.0
}

/// The id of `base` with `shorts` (sorted, without repeats): `an94_mp+grip+reflex`.
pub fn variant_id(base: &str, shorts: &[&str]) -> String {
    let mut v: Vec<&str> = shorts.iter().copied().filter(|s| !s.is_empty()).collect();
    v.sort_unstable();
    v.dedup();
    std::iter::once(base).chain(v).collect::<Vec<_>>().join("+")
}

/// Split a weapon id into its base and attachment short names.
pub fn split_id(id: &str) -> (&str, Vec<&str>) {
    let mut parts = id.split('+');
    let base = parts.next().unwrap_or("");
    (base, parts.collect())
}

/// `base` with `atts` on it, as BO2 plays it. Attachments sharing a point keep the last; ones the
/// weapon doesn't offer must be filtered out by the caller (see [`resolve`]). Art fields
/// (viewmodel, sounds) are the base weapon's; the converter overrides them for the combinations it
/// converts.
pub fn apply(base: &WeaponDef, atts: &[&AttachmentDef]) -> WeaponDef {
    let mut atts: Vec<&AttachmentDef> = atts.to_vec();
    // One per point, the last given wins.
    let mut seen = Vec::new();
    atts.reverse();
    atts.retain(|a| a.point == 0 || {
        let fresh = !seen.contains(&a.point);
        seen.push(a.point);
        fresh
    });
    atts.reverse();
    atts.sort_by(|a, b| a.short.cmp(&b.short));
    atts.dedup_by(|a, b| a.short == b.short);

    let mut d = base.clone();
    let shorts: Vec<&str> = atts.iter().map(|a| a.short.as_str()).collect();
    d.id = variant_id(&base.id, &shorts);
    d.attachments = shorts.iter().map(|s| s.to_string()).collect();
    d.allowed.clear();
    d.alt_modes.clear();
    d.alt.clear();
    let old_clip = d.clip.max(1);
    let mut clip_product = 1.0f64;
    let mut clip_fixed = false;
    for a in &atts {
        clip_product *= a.clip_scale as f64;
        d.fire_time *= scale(a.fire_time_scale);
        for r in d.damage_range.iter_mut() {
            *r *= scale(a.damage_range_scale);
        }
        if a.ads_fov > 1.0 {
            d.ads_fov = a.ads_fov;
        }
        d.ads_in_time *= scale(a.ads_in_scale);
        d.ads_out_time *= scale(a.ads_out_scale);
        d.ads_move_scale = (d.ads_move_scale * scale(a.ads_move_scale)).min(1.0);
        for s in d.hip_spread.iter_mut() {
            s.0 *= scale(a.hip_spread_min_scale);
            s.1 *= scale(a.hip_spread_max_scale);
        }
        d.reload_time *= scale(a.reload_scale);
        d.reload_empty_time *= scale(a.reload_empty_scale);
        d.reload_add_time *= scale(a.reload_add_scale);
        d.reload_empty_add_time *= scale(a.reload_add_scale);
        // The clip scales are multiplied as one product and rounded once after the loop (G14 §4).
        if let Some(f) = a.fire {
            d.fire = f;
            if a.clip > 1 {
                // Tri-bolt: one pull fires all three bolts of a three-bolt load, so the magazine
                // is one shot and spare bolts count in threes.
                clip_fixed = true;
                d.pellets = a.clip;
                let per = a.clip as u32;
                d.clip = ((d.clip as u32 / per).max(1)) as u16;
                d.start_reserve = (d.start_reserve as u32 / per) as u16;
                d.max_reserve = (d.max_reserve as u32 / per) as u16;
            }
        }
        d.silenced |= a.silenced;
        if a.silenced {
            if !d.silenced_fire_sound.is_empty() {
                d.fire_sound = d.silenced_fire_sound.clone();
            }
            if !d.silenced_fx_weapon.is_empty() {
                d.fx_weapon = d.silenced_fx_weapon.clone();
            }
            if !d.silenced_view_flash_fx.is_empty() {
                d.view_flash_fx = d.silenced_view_flash_fx.clone();
            }
        }
        d.laser |= a.laser;
        d.specialties = d.specialties.union(a.specialties);
        if a.recoil_reduction.1 > 0.0 {
            d.recoil_reduction = a.recoil_reduction;
        }
        match a.short.as_str() {
            "fmj" => d.fmj = true,
            "tacknife" => d.melee_time *= TACTICAL_KNIFE_MELEE,
            "rangefinder" => d.target_finder = true,
            _ => {}
        }
    }
    // BO2 truncates every time product to whole milliseconds after all scales are applied
    // (`FUN_008ec740`, `FUN_008ec7a0`, `FUN_008ec860`, `FUN_008ecca0`; G14 §4).
    d.fire_time = trunc_ms(d.fire_time);
    d.reload_time = trunc_ms(d.reload_time);
    d.reload_empty_time = trunc_ms(d.reload_empty_time);
    d.reload_add_time = trunc_ms(d.reload_add_time);
    d.reload_empty_add_time = trunc_ms(d.reload_empty_add_time);
    d.ads_in_time = trunc_ms(d.ads_in_time);
    d.ads_out_time = trunc_ms(d.ads_out_time);
    if !clip_fixed && old_clip > 1 {
        // BO2 multiplies every clip-size scale, then rounds to nearest-even (`FUN_008e5e50`;
        // G14 §4). The f64 product of the f32 fields is exact, so an exact tie is a true tie
        // (46.5 -> 46): no epsilon (the plan's +9.3e-10 pushed exact ties up; measured
        // contradiction). `round_ties_even` is nearest-even, not `f32::round`'s ties-away.
        d.clip = ((old_clip as f64 * clip_product).round_ties_even() as u16).max(1);
    }
    apply_handling(&mut d, &atts);
    // BO2 counts spare ammo in magazines, so a bigger magazine carries more spare rounds.
    if d.clip != old_clip && !atts.iter().any(|a| a.fire.is_some() && a.clip > 1) {
        d.start_reserve = (d.start_reserve as u32 * d.clip as u32 / old_clip as u32) as u16;
        d.max_reserve = (d.max_reserve as u32 * d.clip as u32 / old_clip as u32) as u16;
    }
    d
}

/// `(int)(ms × scale)`: BO2's time-product accessors multiply the whole-millisecond time by the
/// `f32` scale product and truncate (`FUN_008ec8c0/920/980`; G14 §4).
fn scaled_ms(s: f32, scale: f32) -> f32 {
    ((s * 1000.0).round() * scale) as i32 as f32 / 1000.0
}

/// The handling fields' combine rules (G14 §4), walking the attachments in `eAttachment` order as
/// BO2 does: products (quick reload truncated to whole ms, the aimed idle), first differing from
/// `defaultattachment` (zoom fractions, alt-scope times, hip idle), last differing (zoom levels 2
/// and 3), first positive (stack-fire spread), first non-`None` (penetration) and OR-ed flags.
fn apply_handling(d: &mut WeaponDef, atts: &[&AttachmentDef]) {
    let mut walk: Vec<&AttachmentDef> = atts.to_vec();
    walk.sort_by_key(|a| a.kind);
    let product = |get: fn(&AttachmentDef) -> f32| walk.iter().fold(1.0f32, |p, a| p * get(a));
    let first = |get: fn(&AttachmentDef) -> Option<f32>| walk.iter().find_map(|a| get(a));
    let last = |get: fn(&AttachmentDef) -> Option<f32>| walk.iter().rev().find_map(|a| get(a));
    let t = &mut d.timing;
    t.reload_quick_time = scaled_ms(t.reload_quick_time, product(|a| a.reload_quick_scale.0));
    t.reload_quick_empty_time = scaled_ms(t.reload_quick_empty_time, product(|a| a.reload_quick_scale.1));
    t.reload_quick_add_time = scaled_ms(t.reload_quick_add_time, product(|a| a.reload_quick_scale.2));
    let z = &mut d.zoom;
    z.idle_amount.0 *= product(|a| a.ads_idle_scale);
    z.idle_amount.1 = first(|a| a.hip_idle).unwrap_or(z.idle_amount.1);
    z.fov2 = last(|a| a.fov2).unwrap_or(z.fov2);
    z.fov3 = last(|a| a.fov3).unwrap_or(z.fov3);
    z.in_frac = first(|a| a.zoom_in_frac).unwrap_or(z.in_frac);
    z.out_frac = first(|a| a.zoom_out_frac).unwrap_or(z.out_frac);
    z.alt_scope_in_time = first(|a| a.alt_scope_in_time).unwrap_or(z.alt_scope_in_time);
    z.alt_scope_out_time = first(|a| a.alt_scope_out_time).unwrap_or(z.alt_scope_out_time);
    let tr = &mut d.traits;
    tr.stack_fire.0 = walk.iter().map(|a| a.stack_fire.0).find(|x| *x > 0.0).unwrap_or(tr.stack_fire.0);
    tr.stack_fire.1 = walk.iter().map(|a| a.stack_fire.1).find(|x| *x > 0.0).unwrap_or(tr.stack_fire.1);
    tr.penetrate = walk.iter().map(|a| a.penetrate).find(|p| *p != Penetrate::None).unwrap_or(tr.penetrate);
    // `bIgnoreAttachments` gates the dual magazine (`FUN_008eba90`); the other flags OR regardless.
    tr.dual_mag |= !tr.ignore_attachments && walk.iter().any(|a| a.dual_mag);
    tr.infrared |= walk.iter().any(|a| a.infrared);
    tr.mms |= walk.iter().any(|a| a.mms);
    tr.use_as_melee |= walk.iter().any(|a| a.use_as_melee);
    tr.alt_ads_only |= walk.iter().any(|a| a.alt_ads_only);
    tr.alt_disable_switching |= walk.iter().any(|a| a.alt_disable_switching);
}

/// The attachments of `shorts` that `base` offers, as definitions from `lookup` (by BO2 asset id).
pub fn resolve<'a>(base: &WeaponDef, shorts: &[&str], lookup: impl Fn(&str) -> Option<&'a AttachmentDef>) -> Vec<&'a AttachmentDef> {
    shorts
        .iter()
        .filter_map(|s| base.allowed.iter().find(|(short, _)| short == s))
        .filter_map(|(_, id)| lookup(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> WeaponDef {
        WeaponDef {
            id: "an94_mp".into(),
            clip: 30,
            start_reserve: 90,
            max_reserve: 210,
            fire_time: 0.1,
            ads_fov: 55.0,
            ads_in_time: 0.3,
            allowed: vec![("extclip".into(), "extclip".into()), ("reflex".into(), "reflex".into()), ("acog".into(), "acog".into())],
            ..Default::default()
        }
    }

    #[test]
    fn extended_mags_carry_more_spare_rounds_and_optics_zoom() {
        let ext = AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.345, point: 3, ..Default::default() };
        let reflex = AttachmentDef { id: "reflex".into(), short: "reflex".into(), ads_fov: 50.0, point: point::OPTIC, ..Default::default() };
        let d = apply(&base(), &[&reflex, &ext]);
        assert_eq!(d.id, "an94_mp+extclip+reflex");
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (40, 120, 280));
        assert_eq!(d.ads_fov, 50.0);
        assert_eq!(d.attachments, vec!["extclip", "reflex"]);
    }

    #[test]
    fn one_optic_at_a_time() {
        let reflex = AttachmentDef { id: "reflex".into(), short: "reflex".into(), ads_fov: 50.0, point: point::OPTIC, ..Default::default() };
        let acog = AttachmentDef { id: "acog".into(), short: "acog".into(), ads_fov: 35.0, point: point::OPTIC, ..Default::default() };
        let d = apply(&base(), &[&reflex, &acog]);
        assert_eq!(d.attachments, vec!["acog"]);
        assert_eq!(d.ads_fov, 35.0);
    }

    #[test]
    fn ids_split_and_resolve_against_what_the_weapon_offers() {
        assert_eq!(split_id("an94_mp+reflex+grip"), ("an94_mp", vec!["reflex", "grip"]));
        assert_eq!(variant_id("an94_mp", &["reflex", "grip", "reflex"]), "an94_mp+grip+reflex");
        let reflex = AttachmentDef { id: "reflex".into(), short: "reflex".into(), ..Default::default() };
        let got = resolve(&base(), &["reflex", "silencer"], |id| (id == "reflex").then_some(&reflex));
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn a_silenced_attachment_swaps_the_shot_and_flash() {
        let base = WeaponDef {
            id: "an94_mp".into(),
            fire_sound: "ZHBO_BO2_Fire_an94_mp".into(),
            fx_weapon: "ZHBO_FX_an94_mp".into(),
            view_flash_fx: "ZHBO_BO2FX_fx_muz_ar_flash_1p".into(),
            silenced_fire_sound: "ZHBO_BO2_Fire_an94_mp_silenced".into(),
            silenced_fx_weapon: "ZHBO_FX_an94_mp_silenced".into(),
            silenced_view_flash_fx: "ZHBO_BO2FX_fx_muz_silencer_md".into(),
            ..Default::default()
        };
        let silencer = AttachmentDef { id: "silencer".into(), short: "silencer".into(), silenced: true, ..Default::default() };
        let d = apply(&base, &[&silencer]);
        assert!(d.silenced);
        assert_eq!(d.fire_sound, "ZHBO_BO2_Fire_an94_mp_silenced");
        assert_eq!(d.fx_weapon, "ZHBO_FX_an94_mp_silenced");
        assert_eq!(d.view_flash_fx, "ZHBO_BO2FX_fx_muz_silencer_md");
    }

    #[test]
    fn time_scales_truncate_to_whole_milliseconds() {
        let base = WeaponDef { fire_time: 0.1, reload_time: 2.4, ..Default::default() };
        let fast = AttachmentDef {
            id: "fast".into(),
            short: "fast".into(),
            fire_time_scale: 0.75,
            reload_scale: 0.675,
            ..Default::default()
        };
        let d = apply(&base, &[&fast]);
        assert!((d.fire_time - 0.075).abs() < 1e-6, "fire_time {}", d.fire_time);
        assert!((d.reload_time - 1.62).abs() < 1e-6, "reload_time {}", d.reload_time);
    }

    #[test]
    fn a_zero_scale_zeroes_the_product() {
        let base = WeaponDef { ads_in_time: 0.3, ..Default::default() };
        let none = AttachmentDef { id: "none".into(), short: "none".into(), ads_in_scale: 0.0, ..Default::default() };
        let d = apply(&base, &[&none]);
        assert_eq!(d.ads_in_time, 0.0);
    }

    /// The ADS walk keeps the multiply-and-clamp rule: the replacement walk the plan proposed was
    /// reverted (it fired on every attachment's generic 1.0 and erased the base's distinctive
    /// value — the conformance column quadrupled, 924 -> 3,903 rows). The corrected rule needs
    /// its own cycle; BO2's no-change sentinel here is 1.0, not 0.0.
    #[test]
    fn ads_move_scale_multiplies_and_clamps() {
        let base = WeaponDef { ads_move_scale: 0.7, ..Default::default() };
        let stalker = AttachmentDef {
            id: "stalker".into(),
            short: "stalker".into(),
            ads_move_scale: 1.12,
            point: 5,
            ..Default::default()
        };
        let d = apply(&base, &[&stalker]);
        assert!((d.ads_move_scale - 0.7 * 1.12).abs() < 1e-6, "{}", d.ads_move_scale);

        // The clamp holds at 1.0: a bigger scale on a high base cannot pass it.
        let strong = AttachmentDef { id: "s2".into(), short: "s2".into(), ads_move_scale: 2.55, ..Default::default() };
        let d = apply(&base, &[&strong]);
        assert!((d.ads_move_scale - 1.0).abs() < 1e-6, "{}", d.ads_move_scale);
    }

    #[test]
    fn clip_scales_multiply_then_round_to_nearest_even() {
        let base = WeaponDef { clip: 31, ..Default::default() };
        let ext = AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.5, point: 3, ..Default::default() };
        let d = apply(&base, &[&ext]);
        assert_eq!(d.clip, 46, "31 × 1.5 = 46.5 rounds to even 46");
    }

    /// G14 §4's handling rules, walked in `eAttachment` order: quick reloads multiply and truncate
    /// to whole ms, the aimed idle multiplies, the zoom fractions and hip idle take the first
    /// attachment that changes them, zoom levels 2/3 the last, stack fire the first positive,
    /// penetration the first non-`None`, and the flags OR (the dual mag gated by
    /// `ignore_attachments`).
    #[test]
    fn handling_fields_stack_bo2s_way() {
        let mut base = WeaponDef::default();
        base.timing.reload_quick_time = 1.5;
        base.timing.reload_quick_empty_time = 2.0;
        base.zoom = crate::weapons::Zoom { fov2: 15.0, fov3: 15.0, in_frac: 0.0, out_frac: 0.05, idle_amount: (60.0, 20.0), ..Default::default() };
        // Listed out of kind order on purpose: the walk sorts by kind.
        let vzoom = AttachmentDef {
            id: "vzoom".into(), short: "vzoom".into(), kind: 29, point: 6,
            fov2: Some(12.5), fov3: Some(30.0), zoom_in_frac: Some(0.016), zoom_out_frac: Some(0.05),
            ..Default::default()
        };
        let acog = AttachmentDef {
            id: "acog".into(), short: "acog".into(), kind: 1, point: point::OPTIC,
            fov2: Some(35.0), fov3: Some(35.0), zoom_in_frac: Some(0.5), zoom_out_frac: Some(0.4), ads_idle_scale: 0.1,
            ..Default::default()
        };
        let dualclip = AttachmentDef {
            id: "dualclip".into(), short: "dualclip".into(), kind: 6, point: 3,
            reload_quick_scale: (0.675, 0.675, 1.0), dual_mag: true, infrared: true,
            ..Default::default()
        };
        let steady = AttachmentDef { id: "steadyaim".into(), short: "steadyaim".into(), kind: 25, hip_idle: Some(10.0), stack_fire: (1.5, 1.0), penetrate: Penetrate::Medium, ..Default::default() };
        let d = apply(&base, &[&vzoom, &steady, &dualclip, &acog]);
        assert_eq!(d.timing.reload_quick_time, 1.012, "1500 × 0.675 = 1012.5 truncates to 1012 ms");
        assert_eq!(d.timing.reload_quick_empty_time, 1.35);
        assert_eq!((d.zoom.fov2, d.zoom.fov3), (12.5, 30.0), "the last differing (kind 29) wins");
        assert_eq!((d.zoom.in_frac, d.zoom.out_frac), (0.5, 0.4), "the first differing (kind 1) wins");
        assert!((d.zoom.idle_amount.0 - 6.0).abs() < 1e-5);
        assert_eq!(d.zoom.idle_amount.1, 10.0);
        assert_eq!(d.traits.stack_fire, (1.5, 1.0));
        assert_eq!(d.traits.penetrate, Penetrate::Medium);
        assert!(d.traits.dual_mag && d.traits.infrared && !d.traits.mms);

        // No attachment changes a field: the weapon's own stands.
        let plain = apply(&base, &[]);
        assert_eq!((plain.zoom.fov2, plain.zoom.in_frac, plain.zoom.idle_amount), (15.0, 0.0, (60.0, 20.0)));

        // A variant that ignores its attachments keeps its own dual-mag flag.
        let mut ignoring = base.clone();
        ignoring.traits.ignore_attachments = true;
        assert!(!apply(&ignoring, &[&dualclip]).traits.dual_mag);
    }
}
