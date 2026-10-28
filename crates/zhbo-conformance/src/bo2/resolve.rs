//! BO2's weapon-with-attachments resolution, transcribed from the engine (Ghidra G14,
//! `~/Documents/BO2-reference/ghidra/answers/G14.md`; the raw decompiles are `raw/_g14_*.c`).
//! Each function names the engine function it mirrors. Nothing here may call the converter.

use super::raw::{Attachment, Def, Unique, Variant, ANIMS};

/// `WEAPINVENTORY_ALTMODE`.
pub const ALTMODE: u32 = 3;
/// `ATTACHMENT_POINT_TOP` (the optic point).
pub const POINT_TOP: u32 = 1;
/// `eAttachment` of the silencer, whose unique wins a sound override (`FUN_008ebe80`).
pub const SILENCER: usize = 23;
/// `eAttachment` of stack fire, which replaces the fire type and clip (`FUN_008ecd90`, `FUN_008e5e50`).
pub const STACKFIRE: usize = 24;
/// The default attachment mount when a tag is empty (G10).
pub const DEFAULT_TAG: &str = "j_gun";
/// The default mount of a third-person part: `FUN_008ace30`'s `param_4`, which both of its callers
/// pass as `DAT_0240749c` — the script string `FUN_00678440` registers for **`tag_weapon`**, the
/// world models' root bone (not the viewmodel's `j_gun`).
pub const WORLD_DEFAULT_TAG: &str = "tag_weapon";

/// A weapon index: the variant and up to three attachment kinds, ascending with zeros last
/// (`FUN_008e7fd0` + `FUN_008e7f10`).
#[derive(Clone, Copy)]
pub struct Weapon<'z> {
    pub v: Variant<'z>,
    pub d: Def<'z>,
    pub kinds: [usize; 3],
}

impl<'z> Weapon<'z> {
    pub fn new(v: Variant<'z>, kinds: &[usize]) -> Option<Weapon<'z>> {
        let mut k = [0usize; 3];
        let mut sorted: Vec<usize> = kinds.iter().copied().filter(|&x| x != 0).collect();
        sorted.sort_unstable();
        for (i, x) in sorted.into_iter().take(3).enumerate() {
            k[i] = x;
        }
        Some(Weapon { v, d: v.def()?, kinds: k })
    }

    pub fn has_attachments(&self) -> bool {
        self.kinds[0] != 0
    }

    pub fn base(&self) -> Option<Unique<'z>> {
        self.v.unique(0)
    }

    /// `FUN_008eb360`: each field's single unique, the base unique standing in for a missing one
    /// (except in the third field), NULL for an empty field.
    pub fn singles(&self) -> [Option<Unique<'z>>; 3] {
        let [k1, k2, k3] = self.kinds;
        let u0 = self.base();
        let mut out = [None, None, None];
        if k3 != 0 {
            out[2] = self.v.unique(k3 as u32);
            out[0] = u0;
            out[1] = u0;
        }
        if k2 != 0 {
            out[1] = self.v.unique(k2 as u32).or(u0);
            out[0] = u0;
        }
        if k1 != 0 {
            out[0] = self.v.unique(k1 as u32).or(u0);
        }
        out
    }

    /// The single uniques up to the first NULL, as the accessors walk them.
    pub fn singles_walk(&self) -> Vec<Unique<'z>> {
        self.singles().into_iter().map_while(|u| u).collect()
    }

    /// `FUN_008eb300`: each field's `WeaponAttachment` (NULL for an empty field).
    pub fn atts(&self) -> [Option<Attachment<'z>>; 3] {
        self.kinds.map(|k| if k == 0 { None } else { self.v.attachment(k) })
    }

    /// The attachments up to the first NULL, as the accessors walk them.
    pub fn atts_walk(&self) -> Vec<Attachment<'z>> {
        self.atts().into_iter().map_while(|a| a).collect()
    }

    /// `FUN_008eb690`: the index of the unique kind `k` uses in this combination — the first child
    /// (along `childLink`, then `siblingLink`) whose combined mask shares another of the combo's
    /// kinds, else `k` itself.
    pub fn resolved_index(&self, k: usize) -> u32 {
        if k == 0 {
            return 0;
        }
        let others = self.kinds.iter().fold(0u32, |m, &x| m | (1u32 << (x & 31))) & !(1u32 << (k & 31)) & !1;
        let Some(u) = self.v.unique(k as u32) else { return k as u32 };
        let mut j = u.child();
        if j == 0 {
            return k as u32;
        }
        loop {
            let Some(c) = self.v.unique(j as u32) else { return k as u32 };
            if c.combined_mask() & others != 0 {
                return j as u32;
            }
            j = c.sibling();
            if j == 0 {
                return k as u32;
            }
        }
    }

    pub fn resolved(&self, k: usize) -> Option<Unique<'z>> {
        self.v.unique(self.resolved_index(k))
    }

    /// `FUN_008eb600`: the first field kind whose attachment sits on the top (optic) point.
    pub fn optic(&self) -> usize {
        self.kinds.iter().copied().find(|&k| k != 0 && self.v.attachment(k).is_some_and(|a| a.point() == POINT_TOP)).unwrap_or(0)
    }

    pub fn has_kind(&self, k: usize) -> bool {
        self.kinds.contains(&k) && k != 0
    }

    /// `FUN_008ed820`: any single unique disables the base attachment art.
    pub fn disables_base_attachment(&self) -> bool {
        self.singles_walk().iter().any(|u| u.disable_base_attachment())
    }

    /// `FUN_008ed860`: any single unique disables the base magazine.
    pub fn disables_base_clip(&self) -> bool {
        self.singles_walk().iter().any(|u| u.disable_base_clip())
    }

    /// `FUN_008ed8f0`: the first single unique that overrides the base attachment's offsets.
    pub fn base_attachment_override(&self) -> Option<Unique<'z>> {
        self.singles_walk().into_iter().find(|u| u.override_base_attachment_offsets())
    }

    /// The first single unique whose field (read by `get`) differs from the base unique's, else
    /// `None` — the pattern of `FUN_008ed100`, `FUN_008ed1e0`, `FUN_008ed250`, `FUN_008ed2c0`,
    /// `FUN_008ed570/5f0/670`.
    pub fn first_differing<T: PartialEq>(&self, get: impl Fn(&Unique<'z>) -> T) -> Option<T> {
        let u0 = self.base()?;
        let base = get(&u0);
        self.singles_walk().iter().map(&get).find(|x| *x != base)
    }

    /// The product of an attachment scale over the combination (`FUN_008ec740` and its siblings).
    pub fn scale(&self, get: impl Fn(&Attachment<'z>) -> f32) -> f32 {
        self.atts_walk().iter().fold(1.0f32, |p, a| p * get(a))
    }
}

/// `(int)(ms × scale)`: the accessors truncate their float product to an int.
pub fn scaled_ms(ms: i32, scale: f32) -> i32 {
    (ms as f32 * scale) as i32
}

/// `FUN_008e5e50`: the clip size — stackfire's own, else `round(clip × player_clipSizeMultiplier ×
/// Π clipSizeScale)` (min 1) for a clip of more than one round.
pub fn clip_size(w: &Weapon) -> i32 {
    // `player_clipSizeMultiplier` defaults to 1.0 (registered in the same function as
    // `player_sustainAmmo`, `raw/_g14_f.c`).
    let mut scale = 1.0f32;
    for a in w.atts_walk() {
        if a.kind() as usize == STACKFIRE {
            return a.clip_size();
        }
        scale *= a.clip_size_scale();
    }
    let clip = w.v.clip_size();
    if clip > 1 {
        // x87 `ROUND` in the default round-to-nearest-even mode, on `clip × scale + 9.3e-10`.
        let x = clip as f64 * scale as f64 + 9.313225746154785e-10;
        (x.round_ties_even() as i32).max(1)
    } else {
        clip
    }
}

/// `FUN_008eb720`: the first-raise animation — among the fields whose resolved unique overrides
/// slot 0x18, the one whose attachment has the highest `firstRaisePriority` (the first field always
/// takes it; a later one needs a strictly higher priority); none → empty.
fn first_raise(w: &Weapon) -> Option<String> {
    const S: usize = 0x18;
    let mut best: Option<String> = None;
    let mut prio = 0;
    for (i, &k) in w.kinds.iter().enumerate() {
        let idx = w.resolved_index(k);
        if idx == 0 {
            continue;
        }
        let Some(r) = w.v.unique(idx) else { continue };
        let p = w.v.attachment(k).map_or(0, |a| a.first_raise_priority());
        if r.overrides_anim(S) && (i == 0 || p > prio) {
            best = r.anims()[S].clone();
            prio = p;
        }
    }
    best
}

/// `FUN_008eddb0`: the animation slot `s` plays in a combination.
fn slot_anim(w: &Weapon, s: usize, optic: usize, alt_ads_only: bool) -> Option<String> {
    if s == 0x18 {
        return first_raise(w);
    }
    if w.d.inventory() == ALTMODE && alt_ads_only && (s == 0x1a || s == 0x1b || (0x55..=0x57).contains(&s)) {
        return w.v.anims()[s].clone();
    }
    if ((0x3d..=0x40).contains(&s) || (0x55..=0x57).contains(&s)) && optic != 0 {
        if let Some(r) = w.resolved(optic) {
            if r.overrides_anim(s) {
                return r.anims()[s].clone();
            }
        }
    }
    for &k in &w.kinds {
        let idx = w.resolved_index(k);
        if idx == 0 {
            break;
        }
        if let Some(r) = w.v.unique(idx) {
            if r.overrides_anim(s) {
                return r.anims()[s].clone();
            }
        }
    }
    w.base().and_then(|u| u.anims()[s].clone())
}

/// `FUN_0052d470`: an alt mode drops the attachment whose unique made it (its alt weapon is this
/// variant) before its animations are merged.
fn without_producer<'z>(w: &Weapon<'z>) -> Weapon<'z> {
    let me = w.v.name();
    let mut out = *w;
    if let Some(i) = w.kinds.iter().position(|&k| k != 0 && w.v.unique(k as u32).and_then(|u| u.alt_weapon_name()).is_some_and(|n| n == me)) {
        let mut rest: Vec<usize> = w.kinds.iter().copied().filter(|&k| k != 0).collect();
        rest.remove(i);
        out.kinds = [0; 3];
        for (j, k) in rest.into_iter().enumerate() {
            out.kinds[j] = k;
        }
    }
    out
}

/// `FUN_008ecb00`: the alt mode is ADS-only (the def's flag or any attachment's).
pub fn alt_ads_only(w: &Weapon) -> bool {
    w.d.alt_weapon_ads_only() || w.atts().into_iter().flatten().any(|a| a.alt_weapon_ads_only())
}

/// `FUN_008ecb60`: switching to the alt mode is disabled.
pub fn alt_disable_switching(w: &Weapon) -> bool {
    w.d.alt_weapon_disable_switching() || w.atts().into_iter().flatten().any(|a| a.alt_weapon_disable_switching())
}

/// `FUN_008eb9c0`: the weapon is dual wield (the def's flag, or an attachment's up to the first
/// empty field).
pub fn dual_wield(w: &Weapon) -> bool {
    w.d.dual_wield() || w.atts_walk().iter().any(|a| a.dual_wield())
}

/// `FUN_0052d540` + `FUN_0052d3e0`: the animation each slot 1..0x57 plays. `left` is the dual-wield
/// left gun's variant (for slots 0x4E..0x54). An empty slot plays its table's IDLE (slot 1).
pub fn anims(w: &Weapon, left: Option<Variant>) -> Vec<Option<String>> {
    let dual_wield = dual_wield(w);
    let own = w.v.anims();
    let pick = |first: &[Option<String>], second: &[Option<String>]| -> Vec<Option<String>> {
        (0..ANIMS)
            .map(|i| {
                if i == 0 {
                    return None;
                }
                let t = if (0x4e..=0x54).contains(&i) { second } else { first };
                t[i].clone().or_else(|| t[1].clone())
            })
            .collect()
    };
    let left_anims = left.map(|l| l.anims());
    if !dual_wield && own[1].is_none() {
        // No idle: the engine sets no table (`FUN_0052d540` returns early).
        return vec![None; ANIMS];
    }
    let src = if w.d.inventory() == ALTMODE && !alt_ads_only(w) { without_producer(w) } else { *w };
    let src_ads_only = alt_ads_only(&src);
    match w.base() {
        Some(u0) if !w.v.ignore_attachments() => {
            if !w.has_attachments() {
                let second = match (&left_anims, dual_wield) {
                    (Some(l), true) => l.clone(),
                    _ => u0.anims(),
                };
                return pick(&own, &second);
            }
            let mut table = own.clone();
            let optic = src.optic();
            for (s, slot) in table.iter_mut().enumerate() {
                // `FUN_008eb540`: the union of the fields' own single uniques' override bits.
                let candidate = src.kinds.iter().any(|&k| k != 0 && src.v.unique(k as u32).is_some_and(|u| u.overrides_anim(s)));
                if candidate {
                    *slot = slot_anim(&src, s, optic, src_ads_only);
                }
            }
            if w.d.inventory() == ALTMODE {
                for s in [0x1a, 0x1b, 0x56] {
                    table[s] = own[s].clone();
                }
            }
            let second = match (&left_anims, dual_wield) {
                (Some(l), true) => l.clone(),
                _ => table.clone(),
            };
            pick(&table, &second)
        }
        _ => {
            let second = match (&left_anims, dual_wield) {
                (Some(l), true) => l.clone(),
                _ => own.clone(),
            };
            pick(&own, &second)
        }
    }
}

/// `FUN_008ec1a0`: the hidden tags. With attachments (and both `attachments[0]` and the base
/// unique present), a single unique's tag is always hidden and a variant tag stays hidden only
/// when every single unique lists it too.
pub fn hide_tags(w: &Weapon) -> Vec<String> {
    let base = w.v.hide_tags();
    if w.v.attachment(0).is_none() || w.base().is_none() {
        return base;
    }
    let singles = w.singles_walk();
    let lists: Vec<Vec<String>> = singles.iter().map(|u| u.hide_tags()).collect();
    let mut out: Vec<String> = base.iter().filter(|t| lists.iter().all(|l| l.contains(t))).cloned().collect();
    for l in &lists {
        for t in l {
            if !base.contains(t) && !out.contains(t) {
                out.push(t.clone());
            }
        }
    }
    out.truncate(32);
    out
}

/// One drawn part: model, tag, offset (inches), rotation (degrees).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub model: String,
    pub tag: String,
    pub offset: [f32; 3],
    pub rotation: [f32; 3],
    /// The `WeaponCamo` the class camo applies to this piece (`FUN_00529600`): the def's for the
    /// base parts, the attachment's single unique's for its own.
    pub camo: Option<String>,
}

fn tag_or_default(t: Option<String>) -> String {
    t.filter(|t| !t.is_empty()).unwrap_or_else(|| DEFAULT_TAG.to_string())
}

fn world_tag_or_default(t: Option<String>) -> String {
    t.filter(|t| !t.is_empty()).unwrap_or_else(|| WORLD_DEFAULT_TAG.to_string())
}

/// `FUN_00535170`: the first-person parts after the gun itself, hip (`ads == false`) or aimed.
/// An alt mode draws its parent's parts (`parent`).
pub fn view_parts(w: &Weapon, ads: bool) -> Vec<Part> {
    let mut out = Vec::new();
    let no_base_attachment = w.disables_base_attachment();
    let no_base_clip = w.disables_base_clip();
    let slot7 = w.v.attach_view_model(7).0.is_some();
    for i in 0..8u32 {
        let (Some(model), tag) = w.v.attach_view_model(i) else { continue };
        if (i == 0 || i == 7) && no_base_attachment {
            continue;
        }
        if i == 6 && no_base_clip {
            continue;
        }
        if !ads && i == 7 {
            continue;
        }
        if ads && i == 0 && slot7 {
            continue;
        }
        let offset = match (i, w.base_attachment_override()) {
            (0, Some(u)) => u.view_offset_base_attachment(),
            _ => w.v.attach_view_offset(i),
        };
        out.push(Part { model, tag: tag_or_default(tag), offset, rotation: w.v.attach_view_rotation(i), camo: w.d.weapon_camo() });
    }
    let u0 = w.base();
    for s in w.singles().into_iter().flatten() {
        if Some(s) == u0 {
            continue;
        }
        let Some(r) = w.resolved(s.kind() as usize) else { continue };
        let model = if ads { r.view_model_ads().or_else(|| r.view_model()) } else { r.view_model() };
        let Some(model) = model else { continue };
        let tag = tag_or_default(r.view_model_tag());
        out.push(Part { model, tag: tag.clone(), offset: r.view_offset(), rotation: r.view_rotation(), camo: s.weapon_camo() });
        if let Some(add) = r.view_model_additional() {
            out.push(Part { model: add, tag, offset: r.view_add_offset(), rotation: r.view_add_rotation(), camo: s.weapon_camo() });
        }
    }
    out
}

/// `FUN_008ace30`: the third-person parts after the world model.
pub fn world_parts(w: &Weapon) -> Vec<Part> {
    let mut out = Vec::new();
    let no_base_attachment = w.disables_base_attachment();
    let no_base_clip = w.disables_base_clip();
    for i in 0..8u32 {
        let (Some(model), tag) = w.v.attach_world_model(i) else { continue };
        if i == 0 && no_base_attachment {
            continue;
        }
        if i == 6 && no_base_clip {
            continue;
        }
        let offset = match (i, w.base_attachment_override()) {
            (0, Some(u)) => u.world_offset_base_attachment(),
            _ => w.v.attach_world_offset(i),
        };
        out.push(Part { model, tag: world_tag_or_default(tag), offset, rotation: w.v.attach_world_rotation(i), camo: w.d.weapon_camo() });
    }
    for s in w.singles().into_iter().flatten() {
        if s.world_model().is_none() {
            continue;
        }
        let Some(r) = w.resolved(s.kind() as usize) else { continue };
        let Some(model) = r.world_model() else { continue };
        let tag = world_tag_or_default(r.world_model_tag());
        out.push(Part { model, tag: tag.clone(), offset: r.world_offset(), rotation: r.world_rotation(), camo: s.weapon_camo() });
        if let Some(add) = r.world_model_additional() {
            out.push(Part { model: add, tag, offset: r.world_add_offset(), rotation: r.world_add_rotation(), camo: s.weapon_camo() });
        }
    }
    out
}

/// `FUN_008ebe80`: sound `i` of the overridable set (bit `i` ↔ unique field `0x100 + 4i`).
pub fn sound(w: &Weapon, i: u32, def: Option<String>) -> Option<String> {
    if w.v.attachment(0).is_none() || w.v.ignore_attachments() {
        return def;
    }
    let Some(u0) = w.base() else { return def };
    let bit = 1u32 << i;
    if u0.sound_overrides() & bit == 0 {
        return def;
    }
    if w.has_kind(SILENCER) {
        if let Some(s) = w.v.unique(SILENCER as u32).filter(|s| *s != u0 && s.sound_overrides() & bit != 0) {
            return s.override_sound(i);
        }
    }
    w.singles().into_iter().flatten().find(|s| *s != u0 && s.sound_overrides() & bit != 0).map_or(def, |s| s.override_sound(i))
}

/// `FUN_008ec0c0`: effect `i` (0 view flash, 1 world flash).
pub fn effect(w: &Weapon, i: u32, def: Option<String>) -> Option<String> {
    if w.v.attachment(0).is_none() {
        return def;
    }
    let Some(u0) = w.base() else { return def };
    let bit = 1u32 << i;
    if u0.effect_overrides() & bit == 0 {
        return def;
    }
    w.singles_walk().into_iter().find(|s| *s != u0 && s.effect_overrides() & bit != 0).map_or(def, |s| s.override_effect(i))
}

/// The value of the last attachment whose field differs from `defaultattachment`'s, else `def`
/// (`FUN_008ec3f0`, `FUN_008ecd00`, `FUN_008ed410`, `FUN_008ed080`).
pub fn last_differing(w: &Weapon, default: Option<&Attachment>, get: impl Fn(&Attachment) -> f32, def: f32) -> f32 {
    let base = default.map(&get);
    w.atts_walk().iter().map(&get).filter(|x| Some(*x) != base).last().unwrap_or(def)
}

/// The value of the first attachment whose field differs from `defaultattachment`'s, else `def`
/// (`FUN_008ec4b0`, `FUN_008ec520`, `FUN_008ecbc0`, `FUN_008ecc30`, `FUN_008ed3a0`).
pub fn first_att_differing(w: &Weapon, default: Option<&Attachment>, get: impl Fn(&Attachment) -> f32, def: f32) -> f32 {
    let base = default.map(&get);
    w.atts_walk().iter().map(&get).find(|x| Some(*x) != base).unwrap_or(def)
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{override_anim, Gun, Mem};
    use super::*;
    use zhbo_formats::t6zone::layouts::{WeaponAttachment as WA, WeaponAttachmentUnique as WU, WeaponDef as WD, WeaponVariantDef as WV};

    const ACOG: usize = 1;
    const GL: usize = 11;
    const GRIP: usize = 12;
    const REFLEX: usize = 20;

    /// an94-like: base unique 0, acog (1) with two combined children (acog+gl at 63, acog+grip
    /// at 64), gl (11), grip (12), reflex (20).
    fn gun() -> (Mem, Gun, [u32; 6]) {
        let mut m = Mem::new();
        let g = Gun::new(&mut m, "an94_mp");
        g.attachment(&mut m, 0, 0);
        g.attachment(&mut m, ACOG, POINT_TOP);
        g.attachment(&mut m, GL, 2);
        g.attachment(&mut m, GRIP, 2);
        g.attachment(&mut m, REFLEX, POINT_TOP);
        let u0 = g.unique(&mut m, 0, 0, 0, 0, 0);
        let acog = g.unique(&mut m, ACOG as u32, ACOG, 0, 63, 0);
        let acog_gl = g.unique(&mut m, 63, ACOG, 1 << ACOG | 1 << GL, 0, 64);
        let acog_grip = g.unique(&mut m, 64, GRIP, 1 << ACOG | 1 << GRIP, 0, 0);
        let gl = g.unique(&mut m, GL as u32, GL, 0, 0, 0);
        let grip = g.unique(&mut m, GRIP as u32, GRIP, 0, 0, 0);
        g.unique(&mut m, REFLEX as u32, REFLEX, 0, 0, 0);
        m.put_anims(g.v + WV::szXAnims, &[(1, "idle"), (0x10, "reload"), (0x55, "ads_up"), (0x18, "first_raise")]);
        m.put_anims(u0 + WU::szXAnims, &[(1, "u0_idle"), (0x10, "u0_reload"), (0x55, "u0_ads_up")]);
        (m, g, [u0, acog, acog_gl, acog_grip, gl, grip])
    }

    #[test]
    fn a_combination_resolves_to_the_child_sharing_another_kind() {
        let (m, g, _) = gun();
        let z = m.zone();
        let v = g.variant(&z);
        let w = |k: &[usize]| Weapon::new(v, k).unwrap();
        assert_eq!(w(&[ACOG]).resolved_index(ACOG), ACOG as u32, "alone: the single unique");
        assert_eq!(w(&[GL, ACOG]).resolved_index(ACOG), 63, "the first child sharing gl");
        assert_eq!(w(&[ACOG, GRIP]).resolved_index(ACOG), 64, "the sibling sharing grip");
        assert_eq!(w(&[ACOG, REFLEX]).resolved_index(ACOG), ACOG as u32, "no child shares reflex");
        assert_eq!(w(&[ACOG, GL]).resolved_index(GL), GL as u32, "gl has no children");
        assert_eq!(w(&[REFLEX, ACOG, GL]).kinds, [ACOG, GL, REFLEX], "the fields sort by eAttachment");
    }

    #[test]
    fn the_anim_table_follows_the_optic_then_the_lowest_kind_then_the_base_unique() {
        let (mut m, g, [_, acog, acog_gl, _, gl, grip]) = gun();
        override_anim(&mut m, acog, 0x55, "acog_ads_up");
        override_anim(&mut m, acog_gl, 0x55, "acog_gl_ads_up");
        override_anim(&mut m, gl, 0x10, "gl_reload");
        override_anim(&mut m, gl, 0x55, "gl_ads_up");
        override_anim(&mut m, grip, 0x10, "grip_reload");
        // A slot only grip's single unique overrides but whose resolved unique (acog+grip) does not.
        override_anim(&mut m, grip, 0x04, "grip_fire");
        let z = m.zone();
        let v = g.variant(&z);
        let t = anims(&Weapon::new(v, &[ACOG, GL]).unwrap(), None);
        assert_eq!(t[0x55].as_deref(), Some("acog_gl_ads_up"), "ADS slots: the optic's resolved unique first");
        assert_eq!(t[0x10].as_deref(), Some("gl_reload"), "otherwise the lowest kind overriding it");
        assert_eq!(t[0x02].as_deref(), Some("idle"), "an empty slot plays IDLE");
        let t = anims(&Weapon::new(v, &[GL, GRIP]).unwrap(), None);
        assert_eq!(t[0x10].as_deref(), Some("gl_reload"), "gl (11) before grip (12)");
        let t = anims(&Weapon::new(v, &[GRIP]).unwrap(), None);
        assert_eq!(t[0x10].as_deref(), Some("grip_reload"));
        assert_eq!(t[0x04].as_deref(), Some("grip_fire"));
        let t = anims(&Weapon::new(v, &[ACOG, GRIP]).unwrap(), None);
        assert_eq!(t[0x04].as_deref(), Some("grip_fire"), "grip's own resolved unique is itself");
        let t = anims(&Weapon::new(v, &[]).unwrap(), None);
        assert_eq!((t[0x10].as_deref(), t[0x55].as_deref()), (Some("reload"), Some("ads_up")), "the bare weapon plays the variant's table");
        // first_raise is overridden by nobody: the candidate set never includes it.
        assert_eq!(anims(&Weapon::new(v, &[ACOG, GL]).unwrap(), None)[0x18].as_deref(), Some("first_raise"));
    }

    #[test]
    fn the_base_unique_answers_a_slot_no_resolved_unique_overrides() {
        let (mut m, g, [_, acog, acog_gl, ..]) = gun();
        // acog's single unique flags 0x10, but its resolved child (acog+gl) does not: the base
        // unique's name plays, not the variant's (`FUN_008eddb0`'s last line).
        override_anim(&mut m, acog, 0x10, "acog_reload");
        let _ = acog_gl;
        let z = m.zone();
        let t = anims(&Weapon::new(g.variant(&z), &[ACOG, GL]).unwrap(), None);
        assert_eq!(t[0x10].as_deref(), Some("u0_reload"));
    }

    #[test]
    fn first_raise_goes_to_the_highest_priority() {
        let mut m = Mem::new();
        let g = Gun::new(&mut m, "x_mp");
        g.attachment(&mut m, 0, 0);
        let acog_att = g.attachment(&mut m, ACOG, POINT_TOP);
        let gl_att = g.attachment(&mut m, GL, 2);
        g.unique(&mut m, 0, 0, 0, 0, 0);
        let acog = g.unique(&mut m, ACOG as u32, ACOG, 0, 0, 0);
        let gl = g.unique(&mut m, GL as u32, GL, 0, 0, 0);
        m.put_anims(g.v + WV::szXAnims, &[(1, "idle"), (0x18, "first_raise")]);
        override_anim(&mut m, acog, 0x18, "acog_first");
        override_anim(&mut m, gl, 0x18, "gl_first");
        m.put32(acog_att + WA::firstRaisePriority, 2);
        m.put32(gl_att + WA::firstRaisePriority, 5);
        let z = m.zone();
        let t = anims(&Weapon::new(g.variant(&z), &[ACOG, GL]).unwrap(), None);
        assert_eq!(t[0x18].as_deref(), Some("gl_first"), "gl's 5 beats acog's 2");
    }

    #[test]
    fn the_first_field_takes_first_raise_whatever_its_priority() {
        let mut m = Mem::new();
        let g = Gun::new(&mut m, "x_mp");
        g.attachment(&mut m, 0, 0);
        g.attachment(&mut m, ACOG, POINT_TOP);
        let gl_att = g.attachment(&mut m, GL, 2);
        g.unique(&mut m, 0, 0, 0, 0, 0);
        let acog = g.unique(&mut m, ACOG as u32, ACOG, 0, 0, 0);
        g.unique(&mut m, GL as u32, GL, 0, 0, 0);
        m.put_anims(g.v + WV::szXAnims, &[(1, "idle")]);
        override_anim(&mut m, acog, 0x18, "acog_first");
        // gl does not override 0x18, so its priority never competes.
        m.put32(gl_att + WA::firstRaisePriority, 9);
        let z = m.zone();
        let t = anims(&Weapon::new(g.variant(&z), &[ACOG, GL]).unwrap(), None);
        assert_eq!(t[0x18].as_deref(), Some("acog_first"));
    }

    fn zone_bytes(z: &zhbo_formats::t6zone::Zone) -> Vec<u8> {
        z.bytes(0, 1 << 20).to_vec()
    }

    fn zone_strings(z: &zhbo_formats::t6zone::Zone) -> Vec<String> {
        (0..256u16).map(|i| z.script_string(i).to_string()).collect()
    }

    #[test]
    fn hide_tags_keep_a_base_tag_only_when_every_unique_lists_it() {
        let (mut m, g, [u0, acog, _, _, gl, _]) = gun();
        m.put_tags(g.v + WV::hideTags, &["tag_base_a", "tag_base_b"]);
        m.put_tags(acog + WU::hideTags, &["tag_base_a", "tag_sights"]);
        m.put_tags(gl + WU::hideTags, &["tag_base_a", "tag_gl_stub"]);
        let _ = u0;
        let z = m.zone();
        let v = g.variant(&z);
        assert_eq!(hide_tags(&Weapon::new(v, &[]).unwrap()), ["tag_base_a", "tag_base_b"]);
        assert_eq!(hide_tags(&Weapon::new(v, &[ACOG, GL]).unwrap()), ["tag_base_a", "tag_sights", "tag_gl_stub"]);
    }

    #[test]
    fn parts_follow_the_disable_flags_the_resolved_unique_and_the_ads_swap() {
        let (mut m, g, [_, acog, acog_gl, _, _, _]) = gun();
        // Base slots: 0 the scope, 6 the magazine, 7 the scope's ADS model.
        let models = m.alloc(32);
        let tags = m.alloc(32);
        m.put_named(models, "scope_view");
        m.put_named(models + 24, "mag_view");
        m.put_named(models + 28, "scope_ads");
        m.put32(g.v + WV::attachViewModel, models);
        m.put32(g.v + WV::attachViewModelTag, tags);
        m.put_str(tags + 24, "tag_clip");
        m.putf(g.v + WV::attachViewModelOffsets + 12 * 6, 2.5);
        m.put_named(acog + WU::viewModel, "acog_view");
        m.put_named(acog_gl + WU::viewModel, "acog_on_gl_view");
        m.put_str(acog_gl + WU::viewModelTag, "tag_acog");
        m.put8(acog_gl + WU::disableBaseWeaponAttachment, 1);
        let z = m.zone();
        let v = g.variant(&z);
        let names = |ps: Vec<Part>| ps.into_iter().map(|p| (p.model, p.tag)).collect::<Vec<_>>();
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(names(view_parts(&Weapon::new(v, &[]).unwrap(), false)), [s("scope_view", "j_gun"), s("mag_view", "tag_clip")]);
        assert_eq!(names(view_parts(&Weapon::new(v, &[]).unwrap(), true)), [s("mag_view", "tag_clip"), s("scope_ads", "j_gun")]);
        // acog alone: its single unique's model; acog+gl: the child's model and tag. The child's
        // disable flag is not read (the flags come from the single uniques, `FUN_008ed820`).
        assert_eq!(names(view_parts(&Weapon::new(v, &[ACOG]).unwrap(), false)), [s("scope_view", "j_gun"), s("mag_view", "tag_clip"), s("acog_view", "j_gun")]);
        assert_eq!(names(view_parts(&Weapon::new(v, &[ACOG, GL]).unwrap(), false))[2], s("acog_on_gl_view", "tag_acog"));
        assert_eq!(view_parts(&Weapon::new(v, &[]).unwrap(), false)[1].offset, [2.5, 0.0, 0.0]);
    }

    /// Third person mounts an untagged part on `tag_weapon` (`FUN_008ace30`'s default, the
    /// `DAT_0240749c` script string `FUN_00678440` registers), not the viewmodel's `j_gun`.
    #[test]
    fn world_parts_default_to_tag_weapon() {
        let (mut m, g, [_, acog, _, _, _, _]) = gun();
        let models = m.alloc(32);
        let tags = m.alloc(32);
        m.put_named(models + 24, "mag_world");
        m.put32(g.v + WV::attachWorldModel, models);
        m.put32(g.v + WV::attachWorldModelTag, tags);
        m.put_named(acog + WU::worldModel, "acog_world");
        let z = m.zone();
        let v = g.variant(&z);
        let names = |ps: Vec<Part>| ps.into_iter().map(|p| (p.model, p.tag)).collect::<Vec<_>>();
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(names(world_parts(&Weapon::new(v, &[ACOG]).unwrap())), [s("mag_world", "tag_weapon"), s("acog_world", "tag_weapon")]);
    }

    #[test]
    fn the_clip_rounds_and_stackfire_replaces_it() {
        let mut m = Mem::new();
        let g = Gun::new(&mut m, "x_mp");
        let ext = g.attachment(&mut m, 6, 3);
        m.putf(ext + WA::clipSizeScale, 1.345);
        let stack = g.attachment(&mut m, STACKFIRE, 3);
        m.put32(stack + WA::iClipSize, 3);
        m.put32(g.v + WV::iClipSize, 30);
        let z = m.zone();
        let v = g.variant(&z);
        assert_eq!(clip_size(&Weapon::new(v, &[]).unwrap()), 30);
        assert_eq!(clip_size(&Weapon::new(v, &[6]).unwrap()), 40, "30 × 1.345 = 40.35 rounds to 40");
        assert_eq!(clip_size(&Weapon::new(v, &[6, STACKFIRE]).unwrap()), 3);
        assert_eq!(scaled_ms(96, 0.75), 72);
        assert_eq!(scaled_ms(100, 0.333), 33, "the product truncates");
    }

    #[test]
    fn sounds_need_the_base_uniques_override_bit_and_prefer_the_silencer() {
        let mut m = Mem::new();
        let g = Gun::new(&mut m, "x_mp");
        g.attachment(&mut m, 0, 0);
        g.attachment(&mut m, GRIP, 2);
        g.attachment(&mut m, SILENCER, 4);
        let u0 = g.unique(&mut m, 0, 0, 0, 0, 0);
        let grip = g.unique(&mut m, GRIP as u32, GRIP, 0, 0, 0);
        let sil = g.unique(&mut m, SILENCER as u32, SILENCER, 0, 0, 0);
        m.put32(grip + WU::soundOverrides, 0b10);
        m.put_str(grip + WU::fireSoundPlayer, "grip_plr");
        m.put32(sil + WU::soundOverrides, 0b10);
        m.put_str(sil + WU::fireSoundPlayer, "silenced_plr");
        m.put_str(g.d + WD::fireSoundPlayer, "def_plr");
        let z0 = m;
        // Without the base unique's bit nobody overrides.
        let z = z0.zone();
        let v = g.variant(&z);
        let def = Some("def_plr".to_string());
        assert_eq!(sound(&Weapon::new(v, &[GRIP, SILENCER]).unwrap(), 1, def.clone()).as_deref(), Some("def_plr"));
        let mut bytes = zone_bytes(&z);
        bytes[(u0 + WU::soundOverrides) as usize] = 0b10;
        let z = zhbo_formats::t6zone::synthetic("fixture", bytes, zone_strings(&z), Vec::new());
        let v = g.variant(&z);
        assert_eq!(sound(&Weapon::new(v, &[GRIP, SILENCER]).unwrap(), 1, def.clone()).as_deref(), Some("silenced_plr"), "the silencer first");
        assert_eq!(sound(&Weapon::new(v, &[GRIP]).unwrap(), 1, def.clone()).as_deref(), Some("grip_plr"));
        assert_eq!(sound(&Weapon::new(v, &[]).unwrap(), 1, def).as_deref(), Some("def_plr"));
    }
}
