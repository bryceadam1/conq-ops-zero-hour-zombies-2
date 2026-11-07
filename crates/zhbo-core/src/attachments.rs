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
//! extra damage to vehicles (its penetration is the `bulletpenetration` specialty it grants,
//! `crate::penetration`), the grip's recoil reduction (its ADS recoil reduction
//! fields), the tactical knife's quicker melee, and the target finder marking enemies while aiming.

use crate::weapons::{FireMode, Penetrate, Specialties, Sway, WeaponDef};

/// BO2's attachment points (`eAttachmentPoint`). The point says where the art mounts; it does not
/// decide which attachments go together — the compatibility lists do ([`compatible`]).
pub mod point {
    pub const OPTIC: u8 = 1;
}

/// The `eAttachment` BO2's code tests for the target finder (`t6mp` `FUN_008eb270(weapon, 0x13)`
/// in `FUN_00528ec0` and `FUN_00526780`).
pub const TARGET_FINDER_KIND: u8 = 0x13;

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
    /// Its create-a-class icon material (`mp/attachmenttable.csv` column 6, `cac_mods_red_dot`);
    /// the art is converted as `lui::materials::image(image)`. Empty for none.
    pub image: String,
    pub point: u8,
    pub silenced: bool,
    pub laser: bool,
    /// BO2's `mmsMaxDist` (inches): the millimetre scanner's reach (its `mmsWeapon` flag is
    /// [`Self::mms`] below, OR-ed into the weapon's traits).
    pub mms_max_dist: f32,
    /// Select fire, stacked fire: the weapon fires this way instead.
    pub fire: Option<FireMode>,
    /// The fire-type attachment's own `iClipSize` (the tri-bolt's stack), 0 for none.
    pub clip: u16,
    pub damage_range_scale: f32,
    /// Zoomed field of view of an optic (degrees), 0 for none.
    pub ads_fov: f32,
    pub ads_in_scale: f32,
    pub ads_out_scale: f32,
    /// BO2's `adsMoveSpeedScale` when it differs from `defaultattachment`'s: it replaces the
    /// weapon's own ADS walk scale (`FUN_008ed080`). 0 when the attachment leaves the weapon's.
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
    /// `eAttachment` (`WeaponAttachment::attachmentType`): BO2 walks a combination's attachments
    /// in this order (G14 §1), and code tests the weapon for a kind (the target finder, 0x13).
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
    /// `fAdsViewKickCenterSpeedScale`: scales the aimed view-kick recentre speed (a product,
    /// `FUN_008ecf70`).
    pub ads_kick_center_scale: f32,
    /// The hip and aimed sway override blocks (`swayOverride`, `adsSwayOverride`): the first
    /// attachment carrying one supplies all six values (`FUN_008ec590` / `FUN_008ec660`).
    pub sway: Option<Sway>,
    pub ads_sway: Option<Sway>,
    /// The view-position overrides, `None` where equal to `defaultattachment`'s (the last
    /// differing wins, `FUN_008ed410` / `FUN_008ed4f0`): stand move forward, stand rotation
    /// pitch and roll, strafe rotation roll.
    pub stand_move_f: Option<f32>,
    pub stand_rot_pitch: Option<f32>,
    pub stand_rot_roll: Option<f32>,
    pub strafe_rot_roll: Option<f32>,
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
            image: String::new(),
            point: 0,
            silenced: false,
            laser: false,
            mms_max_dist: 0.0,
            fire: None,
            clip: 0,
            damage_range_scale: 1.0,
            ads_fov: 0.0,
            ads_in_scale: 1.0,
            ads_out_scale: 1.0,
            ads_move_scale: 0.0,
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
            ads_kick_center_scale: 1.0,
            sway: None,
            ads_sway: None,
            stand_move_f: None,
            stand_rot_pitch: None,
            stand_rot_roll: None,
            strafe_rot_roll: None,
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
    d.attachment_fire_sounds.clear();
    d.attachment_fire_start_sounds.clear();
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
        // BO2's ADS walk is not a product: the last attachment (canonical order) whose
        // `adsMoveSpeedScale` differs from `defaultattachment`'s replaces the weapon's, and
        // nothing clamps it (`FUN_008ed080`; G14 §4). The converter keeps only such values.
        if a.ads_move_scale != 0.0 {
            d.ads_move_scale = a.ads_move_scale;
        }
        for s in d.hip_spread.iter_mut() {
            s.0 *= scale(a.hip_spread_min_scale);
            s.1 *= scale(a.hip_spread_max_scale);
        }
        d.reload_time *= scale(a.reload_scale);
        d.reload_empty_time *= scale(a.reload_empty_scale);
        // `iReloadEmptyAddTime` has no accessor: BO2 reads the def's raw value (G14 §4).
        d.reload_add_time *= scale(a.reload_add_scale);
        // The clip scales are multiplied as one product and rounded once after the loop (G14 §4).
        if let Some(f) = a.fire {
            d.fire = f;
            // The fire-type attachment (stackfire) also replaces the clip with its own
            // `iClipSize` (`FUN_008e5e50`, G14 §4): the tri-bolt's stack of three.
            if a.clip > 0 {
                clip_fixed = true;
                d.clip = a.clip;
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
            if !d.silenced_fire_start_sound.is_empty() {
                d.fire_start_sound = d.silenced_fire_start_sound.clone();
            }
        }
        d.laser |= a.laser;
        // BO2's target finder is the weapon testing for attachment kind 0x13
        // (`FUN_008eb270(weapon, 0x13)` in `FUN_00528ec0`/`FUN_00526780`: the combo's packed kinds).
        d.target_finder |= a.kind == TARGET_FINDER_KIND;
        // BO2's weapon is a scanner when its variant or any attachment is (`FUN_008ebce0`).
        if a.mms {
            d.mms_max_dist = a.mms_max_dist;
        }
        d.specialties = d.specialties.union(a.specialties);
        if a.recoil_reduction.1 > 0.0 {
            d.recoil_reduction = a.recoil_reduction;
        }
    }
    // A sound another attachment's unique overrides (`FUN_008ebe80`): the silencer's, swapped in
    // above, wins; else the first mounted attachment in `eAttachment` order (the base lists them in
    // that order) that overrides it.
    let silencer = |swapped: &str| atts.iter().any(|a| a.silenced) && !swapped.is_empty();
    let first = |overrides: &[(String, String)]| overrides.iter().find(|(short, _)| shorts.contains(&short.as_str())).map(|(_, e)| e.clone());
    if !silencer(&base.silenced_fire_sound) {
        if let Some(e) = first(&base.attachment_fire_sounds) {
            d.fire_sound = e;
        }
    }
    if !silencer(&base.silenced_fire_start_sound) {
        if let Some(e) = first(&base.attachment_fire_start_sounds) {
            d.fire_start_sound = e;
        }
    }
    // Each hit location's multiplier: the first mounted attachment (in `eAttachment` order) whose
    // unique sets it differently from the base unique, else the def's (`FUN_008ed780`).
    for (l, m) in d.location_multipliers.iter_mut().enumerate() {
        if let Some(x) = base
            .attachment_location_multipliers
            .iter()
            .filter(|(short, _)| shorts.contains(&short.as_str()))
            .find_map(|(_, list)| list.iter().find(|(loc, _)| *loc as usize == l).map(|(_, x)| *x))
        {
            *m = x;
        }
    }
    d.attachment_location_multipliers.clear();
    // BO2 truncates every time product to whole milliseconds after all scales are applied
    // (`FUN_008ec740`, `FUN_008ec7a0`, `FUN_008ec860`, `FUN_008ecca0`; G14 §4).
    d.fire_time = trunc_ms(d.fire_time);
    d.reload_time = trunc_ms(d.reload_time);
    d.reload_empty_time = trunc_ms(d.reload_empty_time);
    d.reload_add_time = trunc_ms(d.reload_add_time);
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
    if d.clip != old_clip && !clip_fixed {
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
/// and 3, the view-position overrides), the override blocks (sway), first positive (stack-fire
/// spread), first non-`None` (penetration) and OR-ed flags.
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
    // The aimed view-kick recentre speed: the variant's × Π scale (`FUN_008ecf70`), kept apart
    // because the hip recentre reads the unscaled speed (`view_kick::centre_speed`).
    d.ads_kick_center_scale *= product(|a| a.ads_kick_center_scale);
    // The sway override blocks: the first attachment with one supplies all six values.
    d.hip_sway = walk.iter().find_map(|a| a.sway).unwrap_or(d.hip_sway);
    d.ads_sway = walk.iter().find_map(|a| a.ads_sway).unwrap_or(d.ads_sway);
    // The gun's movement pose overrides (Steady Aim's level hold): the last differing wins.
    let p = &mut d.view_pos;
    p.stand_move[0] = last(|a| a.stand_move_f).unwrap_or(p.stand_move[0]);
    p.stand_rot[0] = last(|a| a.stand_rot_pitch).unwrap_or(p.stand_rot[0]);
    p.stand_rot[2] = last(|a| a.stand_rot_roll).unwrap_or(p.stand_rot[2]);
    p.strafe_rot[2] = last(|a| a.strafe_rot_roll).unwrap_or(p.strafe_rot[2]);
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

/// The alt mode `alt_base` (select fire, the second optic) of a weapon that also carries `others`
/// (the combination's attachments except the one that made the alt). BO2 keeps the combination's
/// attachment fields on the alt's weapon index and swaps only the variant (`FUN_008ec9e0`), so every
/// accessor resolves them against the **alt variant's own** attachment table (`FUN_008eb300`,
/// `FUN_008eb360`, e.g. the clip size `FUN_008e5e50`; G14 §2/§4) — here `alt_base.allowed`. An
/// attachment the alt does not offer changes none of its numbers; BO2's alt variants offer none, so
/// the alt plays by its own definition. The names stay in `attachments`: the alt draws its parent's
/// parts (`FUN_00535170`), so the optic and its reticle are still the parent's.
pub fn alt_mode<'a>(alt_base: &WeaponDef, others: &[&str], lookup: impl Fn(&str) -> Option<&'a AttachmentDef>) -> WeaponDef {
    let own = resolve(alt_base, others, lookup);
    let mut d = apply(alt_base, &own);
    let mut names: Vec<String> = others.iter().filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
    names.sort_unstable();
    names.dedup();
    d.attachments = names;
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

    /// [`apply`] over attachments that all pair with each other: the stacking tests are about the
    /// stacking rules, not combo-legality's compatibility lists.
    fn apply_paired(base: &WeaponDef, atts: &[&AttachmentDef]) -> WeaponDef {
        let shorts: Vec<String> = atts.iter().map(|a| a.short.clone()).collect();
        let owned: Vec<AttachmentDef> =
            atts.iter().map(|a| AttachmentDef { compatible: shorts.iter().filter(|s| **s != a.short).cloned().collect(), ..(*a).clone() }).collect();
        let refs: Vec<&AttachmentDef> = owned.iter().collect();
        apply(base, &refs)
    }

    fn base() -> WeaponDef {
        WeaponDef {
            id: "an94_mp".into(),
            clip: 30,
            start_reserve: 90,
            max_reserve: 240,
            fire_time: 0.1,
            ads_fov: 55.0,
            ads_in_time: 0.3,
            allowed: vec![("extclip".into(), "extclip".into()), ("reflex".into(), "reflex".into()), ("acog".into(), "acog".into())],
            ..Default::default()
        }
    }

    /// `FUN_008ed780`: per location, the first mounted attachment whose unique sets it differently
    /// from the base unique, else the def's (a sniper's silencer drops its arm multipliers).
    #[test]
    fn an_attachments_unique_replaces_the_location_multipliers_it_changes() {
        let mut b = base();
        b.allowed.push(("silencer".into(), "silencer".into()));
        b.location_multipliers[2] = 2.0;
        b.location_multipliers[9] = 1.5;
        b.attachment_location_multipliers = vec![("silencer".into(), vec![(9, 1.0)])];
        let sil = AttachmentDef { id: "silencer".into(), short: "silencer".into(), point: 4, ..Default::default() };
        let ext = AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.345, point: 3, ..Default::default() };
        let d = apply_paired(&b, &[&sil, &ext]);
        assert_eq!((d.location_multipliers[2], d.location_multipliers[9]), (2.0, 1.0));
        assert!(d.attachment_location_multipliers.is_empty(), "a combination is not a base");
        assert_eq!(apply(&b, &[&ext]).location_multipliers[9], 1.5);
        // The table carries both.
        let text = crate::weapons::format_table(&[b.clone()]);
        assert_eq!(crate::weapons::parse_table(&text).unwrap()[0].attachment_location_multipliers, b.attachment_location_multipliers);
    }

    #[test]
    fn extended_mags_carry_more_spare_rounds_and_optics_zoom() {
        let ext = AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.345, point: 3, compatible: vec!["reflex".into()], ..Default::default() };
        let reflex = AttachmentDef { id: "reflex".into(), short: "reflex".into(), ads_fov: 50.0, point: point::OPTIC, compatible: vec!["extclip".into()], ..Default::default() };
        let d = apply(&base(), &[&reflex, &ext]);
        assert_eq!(d.id, "an94_mp+extclip+reflex");
        assert_eq!((d.clip, d.start_reserve, d.max_reserve), (40, 120, 320));
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

    /// Another attachment's sound override (`FUN_008ebe80`): the first one mounted in the base's
    /// (`eAttachment`) order; a silencer that overrides the same sound wins, one that does not
    /// leaves it to the others.
    #[test]
    fn an_attachment_overrides_the_shot_and_opening_after_the_silencer() {
        let base = WeaponDef {
            id: "sig556_mp".into(),
            fire_sound: "ZHBO_BO2_Fire_sig556_mp".into(),
            fire_start_sound: "ZHBO_BO2_Snd_wpn_sig556_fire_plr_burst".into(),
            silenced_fire_sound: "ZHBO_BO2_Fire_sig556_mp_silenced".into(),
            attachment_fire_sounds: vec![("gl".into(), "gl_shot".into()), ("sf".into(), "sf_shot".into())],
            attachment_fire_start_sounds: vec![("sf".into(), "NONE".into())],
            ..Default::default()
        };
        let att = |short: &str, silenced| AttachmentDef { id: short.into(), short: short.into(), silenced, ..Default::default() };
        let (sf, gl, sil, reflex) = (att("sf", false), att("gl", false), att("silencer", true), att("reflex", false));
        let d = apply_paired(&base, &[&reflex, &sf]);
        assert_eq!((d.fire_sound.as_str(), d.fire_start_sound.as_str()), ("sf_shot", "NONE"));
        assert!(d.attachment_fire_sounds.is_empty() && d.attachment_fire_start_sounds.is_empty());
        assert_eq!(apply_paired(&base, &[&sf, &gl]).fire_sound, "gl_shot", "the base's order, not the mount order");
        let d = apply_paired(&base, &[&sf, &sil]);
        assert_eq!(d.fire_sound, "ZHBO_BO2_Fire_sig556_mp_silenced", "the silencer's override wins");
        assert_eq!(d.fire_start_sound, "NONE", "it does not override the opening: the select fire's stands");
        assert_eq!(apply_paired(&base, &[&reflex]).fire_start_sound, base.fire_start_sound);
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

    /// The ADS walk is replaced, not multiplied (`FUN_008ed080`): an SMG (BO2 2.0) with
    /// `stalker_smg` (2.55) walks at the base × 2.55, not × 2 × 2.55 clamped to a full walk; an
    /// attachment that keeps `defaultattachment`'s value (stored as 0) leaves the weapon's own.
    #[test]
    fn ads_move_scale_is_replaced_by_the_attachment_that_sets_one() {
        let smg = WeaponDef { ads_move_scale: 2.0, ..Default::default() };
        let stalker = AttachmentDef { id: "stalker_smg".into(), short: "stalker".into(), ads_move_scale: 2.55, point: 5, ..Default::default() };
        let dualclip = AttachmentDef { id: "dualclip".into(), short: "dualclip".into(), point: 3, ..Default::default() };
        let d = apply(&smg, &[&stalker]);
        assert!((d.ads_move_scale - 2.55).abs() < 1e-6, "{}", d.ads_move_scale);
        // Order does not matter for one setter: dualclip sorts before stalker and keeps nothing.
        let d = apply_paired(&smg, &[&stalker, &dualclip]);
        assert!((d.ads_move_scale - 2.55).abs() < 1e-6, "{}", d.ads_move_scale);
        // No setter: the weapon's own value, untouched.
        let d = apply(&smg, &[&dualclip]);
        assert!((d.ads_move_scale - 2.0).abs() < 1e-6, "{}", d.ads_move_scale);
    }

    /// With two setters the last in BO2's canonical order (ascending `eAttachment`, which is the
    /// short names' alphabetical order) wins, and nothing clamps a value past a full walk.
    #[test]
    fn ads_move_scale_last_setter_wins_without_a_clamp() {
        let base = WeaponDef { ads_move_scale: 1.0, ..Default::default() };
        let a = AttachmentDef { id: "acog".into(), short: "acog".into(), ads_move_scale: 1.5, point: point::OPTIC, ..Default::default() };
        let z = AttachmentDef { id: "vzoom".into(), short: "vzoom".into(), ads_move_scale: 3.0, point: 2, ..Default::default() };
        let d = apply_paired(&base, &[&z, &a]);
        assert!((d.ads_move_scale - 3.0).abs() < 1e-6, "{}", d.ads_move_scale);
        assert!(d.ads_move_scale > 1.0, "no clamp: {}", d.ads_move_scale);
    }

    /// G14 `FUN_008ec9e0`/`FUN_008eb300`: the alt mode resolves the parent's attachments against
    /// its own table. One that offers nothing (every BO2 alt) keeps all its numbers; one that
    /// offers an attachment takes that one only. The names carry over either way.
    #[test]
    fn an_alt_mode_takes_only_the_attachments_it_offers_itself() {
        let alt = WeaponDef { id: "sf_an94_mp".into(), clip: 30, fire_time: 0.1, reload_time: 2.4, ads_fov: 55.0, ..Default::default() };
        let ext = AttachmentDef { id: "extclip".into(), short: "extclip".into(), clip_scale: 1.345, point: 3, ..Default::default() };
        let rf = AttachmentDef { id: "rf".into(), short: "rf".into(), fire_time_scale: 0.65, ..Default::default() };
        let acog = AttachmentDef { id: "acog".into(), short: "acog".into(), ads_fov: 35.0, point: point::OPTIC, laser: true, ..Default::default() };
        let all = [&ext, &rf, &acog];
        let lookup = |id: &str| all.iter().copied().find(|a| a.id == id);
        let d = alt_mode(&alt, &["rf", "extclip", "acog"], lookup);
        assert_eq!((d.clip, d.fire_time, d.reload_time, d.ads_fov, d.laser), (30, 0.1, 2.4, 55.0, false));
        assert_eq!(d.attachments, vec!["acog", "extclip", "rf"]);

        let offers = WeaponDef { allowed: vec![("acog".into(), "acog".into())], ..alt.clone() };
        let d = alt_mode(&offers, &["rf", "extclip", "acog"], lookup);
        assert_eq!((d.clip, d.fire_time, d.ads_fov), (30, 0.1, 35.0));
        assert_eq!(d.attachments, vec!["acog", "extclip", "rf"]);
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
        // At most three attachments go on a weapon (`sanitize`, combo-legality): the dual magazine's
        // fields are checked on their own combination.
        let d = apply_paired(&base, &[&vzoom, &steady, &acog]);
        let mag = apply_paired(&base, &[&dualclip, &acog]);
        assert_eq!(mag.timing.reload_quick_time, 1.012, "1500 × 0.675 = 1012.5 truncates to 1012 ms");
        assert_eq!(mag.timing.reload_quick_empty_time, 1.35);
        assert_eq!((d.zoom.fov2, d.zoom.fov3), (12.5, 30.0), "the last differing (kind 29) wins");
        assert_eq!((d.zoom.in_frac, d.zoom.out_frac), (0.5, 0.4), "the first differing (kind 1) wins");
        assert!((d.zoom.idle_amount.0 - 6.0).abs() < 1e-5);
        assert_eq!(d.zoom.idle_amount.1, 10.0);
        assert_eq!(d.traits.stack_fire, (1.5, 1.0));
        assert_eq!(d.traits.penetrate, Penetrate::Medium);
        assert!(mag.traits.dual_mag && mag.traits.infrared && !mag.traits.mms);

        // No attachment changes a field: the weapon's own stands.
        let plain = apply(&base, &[]);
        assert_eq!((plain.zoom.fov2, plain.zoom.in_frac, plain.zoom.idle_amount), (15.0, 0.0, (60.0, 20.0)));

        // A variant that ignores its attachments keeps its own dual-mag flag.
        let mut ignoring = base.clone();
        ignoring.traits.ignore_attachments = true;
        assert!(!apply(&ignoring, &[&dualclip]).traits.dual_mag);
    }

    #[test]
    fn gun_feel_fields_stack_bo2s_way() {
        // an94-like base: its own sway and movement pose, a 1500 aimed recentre speed.
        let mut base = base();
        base.hip_sway = Sway { max_angle: 5.0, lerp_speed: 6.0, pitch_scale: -2.0, yaw_scale: -1.0, horiz_scale: 0.3, vert_scale: 0.3 };
        base.ads_sway = Sway { max_angle: 0.75, lerp_speed: 5.5, pitch_scale: 0.15, yaw_scale: 0.15, horiz_scale: 0.15, vert_scale: 0.15 };
        base.view_kick_center = (1500.0, 1500.0);
        base.view_pos.stand_move = [-0.4, 0.0, 0.0];
        base.view_pos.stand_rot = [1.4, 0.0, -1.4];
        base.view_pos.strafe_rot = [0.0, 0.0, 5.0];
        let steady_sway = Sway { max_angle: 1.5, lerp_speed: 5.0, pitch_scale: 0.5, yaw_scale: -0.5, horiz_scale: 0.075, vert_scale: 0.075 };
        let other_sway = Sway { max_angle: 9.0, ..steady_sway };
        let steady = AttachmentDef {
            id: "steadyaim".into(),
            short: "steadyaim".into(),
            kind: 26,
            sway: Some(steady_sway),
            stand_rot_pitch: Some(-0.1),
            stand_rot_roll: Some(0.0),
            strafe_rot_roll: Some(1.0),
            ..Default::default()
        };
        let silencer = AttachmentDef { id: "silencer".into(), short: "silencer".into(), kind: 23, ads_kick_center_scale: 0.975, sway: Some(other_sway), ..Default::default() };
        let acog = AttachmentDef { id: "acog".into(), short: "acog".into(), kind: 1, ads_kick_center_scale: 1.01, strafe_rot_roll: Some(3.0), ..Default::default() };
        let d = apply_paired(&base, &[&steady, &silencer, &acog]);
        // The aimed recentre speed is a product; the hip one has no scale.
        assert_eq!((d.view_kick_center, d.ads_kick_center_scale), ((1500.0, 1500.0), 1.01f32 * 0.975));
        // The override block: the first in eAttachment order (silencer 23 before steadyaim 26)
        // supplies all six; the aimed set is untouched without an aimed override.
        assert_eq!(d.hip_sway, other_sway);
        assert_eq!(d.ads_sway, base.ads_sway);
        assert_eq!(apply(&base, &[&steady]).hip_sway, steady_sway);
        // The movement pose: the last differing wins per component; yaw and stand move stay.
        assert_eq!(d.view_pos.stand_rot, [-0.1, 0.0, 0.0]);
        assert_eq!(d.view_pos.strafe_rot, [0.0, 0.0, 1.0], "steadyaim (26) walks after acog (1)");
        assert_eq!(d.view_pos.stand_move, [-0.4, 0.0, 0.0]);
        assert_eq!(apply(&base, &[&acog]).view_pos.strafe_rot, [0.0, 0.0, 3.0]);
    }

    /// The target finder is BO2's attachment kind 0x13, whatever the attachment is called.
    #[test]
    fn the_target_finder_is_attachment_kind_0x13() {
        let finder = AttachmentDef { short: "anything".into(), kind: TARGET_FINDER_KIND, ..Default::default() };
        assert!(apply(&base(), &[&finder]).target_finder);
        let named = AttachmentDef { short: "rangefinder".into(), kind: 0x12, ..Default::default() };
        assert!(!apply(&base(), &[&named]).target_finder, "a name is not the kind");
    }
}
