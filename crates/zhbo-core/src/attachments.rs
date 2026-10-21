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
}

impl Default for AttachmentDef {
    fn default() -> Self {
        AttachmentDef {
            id: String::new(),
            short: String::new(),
            display_name: String::new(),
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
        }
    }
}

/// FMJ: bullets go through the first infantry target, and hit vehicles and structures harder (BO2's
/// FMJ does extra damage to scorestreaks and equipment).
pub const FMJ_VEHICLE_DAMAGE: f32 = 1.35;
/// Tactical knife: melee this much quicker.
pub const TACTICAL_KNIFE_MELEE: f32 = 0.6;

/// A scale BO2 left unset (0) is no change.
fn scale(s: f32) -> f32 {
    if s > 0.0 {
        s
    } else {
        1.0
    }
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
    for a in &atts {
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
        if (scale(a.clip_scale) - 1.0).abs() > 1e-3 {
            d.clip = ((d.clip as f32 * a.clip_scale).round() as u16).max(1);
        }
        if let Some(f) = a.fire {
            d.fire = f;
            if a.clip > 1 {
                // Tri-bolt: one pull fires all three bolts of a three-bolt load, so the magazine
                // is one shot and spare bolts count in threes.
                d.pellets = a.clip;
                let per = a.clip as u32;
                d.clip = ((d.clip as u32 / per).max(1)) as u16;
                d.start_reserve = (d.start_reserve as u32 / per) as u16;
                d.max_reserve = (d.max_reserve as u32 / per) as u16;
            }
        }
        d.silenced |= a.silenced;
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
}
