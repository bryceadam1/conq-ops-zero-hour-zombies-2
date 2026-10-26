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

use crate::weapons::{FireMode, Specialties, WeaponDef};

/// BO2's attachment points (`eAttachmentPoint`). The point says where the art mounts; it does not
/// decide which attachments go together — the compatibility lists do ([`compatible`]).
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
    /// The attachment shorts this one may be combined with: BO2's `compatibleAttachments`
    /// (`mp/attachmenttable.csv` column 11, `FUN_008e70f0`). Pairing needs both lists to name the
    /// other ([`compatible`]).
    pub compatible: Vec<String>,
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
            compatible: Vec::new(),
        }
    }
}

/// The most attachments one weapon carries: the weapon index has three attachment fields
/// (`FUN_008e7fd0`, "More than %d attachments on weapon named %s").
pub const MAX_PER_WEAPON: usize = 3;

/// `FUN_008e6b20` (`AreAttachmentsCompatible`): two attachments go together only when each one's
/// compatibility list names the other. The same attachment is never compatible with itself.
pub fn compatible(a: &AttachmentDef, b: &AttachmentDef) -> bool {
    a.short != b.short && a.compatible.contains(&b.short) && b.compatible.contains(&a.short)
}

/// BO2's conflicting-attachment check on a weapon (`FUN_008eb870`, `tu6_conflictingAttachmentCheck`,
/// on by default) over `atts` in canonical order (`eAttachment` order, the shorts' sorted order;
/// `FUN_008e7f10`), repeats dropped, at most [`MAX_PER_WEAPON`]: the third goes if it conflicts
/// with the first or the second, then the second goes (the third moving up) if it conflicts with
/// the first. A legal set comes back unchanged.
pub fn sanitize<'a>(atts: &[&'a AttachmentDef]) -> Vec<&'a AttachmentDef> {
    let mut v: Vec<&AttachmentDef> = atts.to_vec();
    v.sort_by(|a, b| a.short.cmp(&b.short));
    v.dedup_by(|a, b| a.short == b.short);
    v.truncate(MAX_PER_WEAPON);
    if v.len() == 3 && (!compatible(v[1], v[2]) || !compatible(v[0], v[2])) {
        v.pop();
    }
    if v.len() >= 2 && !compatible(v[0], v[1]) {
        v.remove(1);
    }
    v
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

/// `base` with `atts` on it, as BO2 plays it. A set BO2 forbids is cut down by its own check
/// ([`sanitize`]); ones the weapon doesn't offer must be filtered out by the caller (see
/// [`resolve`]). Art fields
/// (viewmodel, sounds) are the base weapon's; the converter overrides them for the combinations it
/// converts.
pub fn apply(base: &WeaponDef, atts: &[&AttachmentDef]) -> WeaponDef {
    let atts = sanitize(atts);

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
    // BO2 counts spare ammo in magazines, so a bigger magazine carries more spare rounds.
    if d.clip != old_clip && !atts.iter().any(|a| a.fire.is_some() && a.clip > 1) {
        d.start_reserve = (d.start_reserve as u32 * d.clip as u32 / old_clip as u32) as u16;
        d.max_reserve = (d.max_reserve as u32 * d.clip as u32 / old_clip as u32) as u16;
    }
    d
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
        let ext = AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.345, point: 3, compatible: vec!["reflex".into()], ..Default::default() };
        let reflex = AttachmentDef { id: "reflex".into(), short: "reflex".into(), ads_fov: 50.0, point: point::OPTIC, compatible: vec!["extclip".into()], ..Default::default() };
        let d = apply(&base(), &[&reflex, &ext]);
        assert_eq!(d.id, "an94_mp+extclip+reflex");
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (40, 120, 280));
        assert_eq!(d.ads_fov, 50.0);
        assert_eq!(d.attachments, vec!["extclip", "reflex"]);
    }

    /// A test attachment with BO2's compatibility list.
    fn att(short: &str, point: u8, with: &[&str]) -> AttachmentDef {
        AttachmentDef { id: short.into(), short: short.into(), point, compatible: with.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn compatibility_is_mutual_and_not_the_attach_point() {
        // BO2's grip and select fire share the bottom point yet list each other; the grenade
        // launcher lists the ACOG, which does not list it back.
        let grip = att("grip", 2, &["sf", "acog"]);
        let sf = att("sf", 2, &["grip"]);
        let acog = att("acog", 1, &["grip"]);
        let gl = att("gl", 2, &["acog"]);
        assert!(compatible(&grip, &sf) && compatible(&sf, &grip));
        assert!(!compatible(&gl, &acog), "gl lists acog, but acog does not list gl");
        assert!(!compatible(&grip, &grip));
        let d = apply(&base(), &[&grip, &sf]);
        assert_eq!(d.attachments, vec!["grip", "sf"], "a same-point pair BO2 allows stays");
    }

    #[test]
    fn a_forbidden_pair_keeps_the_first_in_canonical_order() {
        let reflex = AttachmentDef { ads_fov: 50.0, ..att("reflex", point::OPTIC, &[]) };
        let acog = AttachmentDef { ads_fov: 35.0, ..att("acog", point::OPTIC, &[]) };
        let d = apply(&base(), &[&reflex, &acog]);
        assert_eq!(d.attachments, vec!["acog"], "acog sorts first; reflex conflicts with it");
        assert_eq!(d.ads_fov, 35.0);
    }

    /// `FUN_008eb870`: the third is checked first, against both; then the second against the first.
    #[test]
    fn the_sanitizer_follows_bo2s_order() {
        // a–b conflict, a–c fine, b–c conflict: the third goes (it conflicts with b), then b goes.
        let a = att("a", 0, &["c"]);
        let b = att("b", 0, &[]);
        let c = att("c", 0, &["a"]);
        let got: Vec<&str> = sanitize(&[&c, &b, &a]).iter().map(|x| x.short.as_str()).collect();
        assert_eq!(got, vec!["a"]);
        // a–b conflict only: the third survives and moves up.
        let a = att("a", 0, &["c"]);
        let b = att("b", 0, &["c"]);
        let c = att("c", 0, &["a", "b"]);
        let got: Vec<&str> = sanitize(&[&a, &b, &c]).iter().map(|x| x.short.as_str()).collect();
        assert_eq!(got, vec!["a", "c"]);
        // A legal set is untouched; a fourth is past the weapon index's three fields.
        let all = ["a", "b", "c", "d"];
        let defs: Vec<AttachmentDef> = all.iter().map(|s| att(s, 0, &all)).collect();
        let refs: Vec<&AttachmentDef> = defs.iter().collect();
        assert_eq!(sanitize(&refs).len(), MAX_PER_WEAPON);
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
}
