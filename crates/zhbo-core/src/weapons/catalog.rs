/*
** Conquer Ops - Zero Hour gameplay core: the installed weapon catalog (the converted BO2 table, or the built-in stand-ins).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::def::{CamoOption, WeaponDef};
use super::table::{parse_full_table, Table};
use crate::attachments::{self, AttachmentDef};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};


/// All weapons and attachments by id. Weapons with attachments the table doesn't hold are built
/// from their base on first use (identically on every client) and kept.
pub struct Catalog {
    by_id: HashMap<String, &'static WeaponDef>,
    order: Vec<&'static WeaponDef>,
    attachments: HashMap<String, &'static AttachmentDef>,
    camo: HashMap<String, &'static CamoOption>,
    reticles: HashMap<String, &'static CamoOption>,
    reticle_order: Vec<&'static CamoOption>,
    reticle_colors: HashMap<String, &'static CamoOption>,
    reticle_color_order: Vec<&'static CamoOption>,
    built: Mutex<HashMap<String, &'static WeaponDef>>,
    pub tablet_meshes: u8,
}

impl Catalog {
    pub fn new(defs: Vec<WeaponDef>) -> Catalog {
        Catalog::from_table(Table { weapons: defs, ..Default::default() })
    }

    pub fn from_table(t: Table) -> Catalog {
        let order: Vec<&'static WeaponDef> = t.weapons.into_iter().map(|d| &*Box::leak(Box::new(d))).collect();
        let by_id = order.iter().map(|d| (d.id.clone(), *d)).collect();
        let attachments = t.attachments.into_iter().map(|a| (a.id.clone(), &*Box::leak(Box::new(a)))).collect();
        let camo = t.camo.into_iter().map(|c| (c.id.clone(), &*Box::leak(Box::new(c)))).collect();
        let reticle_order: Vec<&'static CamoOption> = t.reticles.into_iter().map(|r| &*Box::leak(Box::new(r))).collect();
        let reticles = reticle_order.iter().map(|r| (r.id.clone(), *r)).collect();
        let reticle_color_order: Vec<&'static CamoOption> = t.reticle_colors.into_iter().map(|c| &*Box::leak(Box::new(c))).collect();
        let reticle_colors = reticle_color_order.iter().map(|c| (c.id.clone(), *c)).collect();
        Catalog {
            by_id,
            order,
            attachments,
            camo,
            reticles,
            reticle_order,
            reticle_colors,
            reticle_color_order,
            built: Mutex::new(HashMap::new()),
            tablet_meshes: t.tablet_meshes,
        }
    }

    /// A weapon by id; `base+att+att` builds the attachments onto the base if the table doesn't
    /// hold that combination. `None` if the base is unknown.
    pub fn get(&self, id: &str) -> Option<&'static WeaponDef> {
        if let Some(d) = self.by_id.get(id) {
            return Some(*d);
        }
        let (base_id, shorts) = attachments::split_id(id);
        if shorts.is_empty() {
            return None;
        }
        // The table may hold the same combination in its sorted spelling.
        let canonical = attachments::variant_id(base_id, &shorts);
        if let Some(d) = self.by_id.get(&canonical) {
            return Some(*d);
        }
        let mut built = self.built.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(d) = built.get(&canonical) {
            return Some(*d);
        }
        let base = *self.by_id.get(base_id)?;
        let atts = attachments::resolve(base, &shorts, |a| self.attachments.get(a).copied());
        let mut d = attachments::apply(base, &atts);
        if let Some((_, alt_id)) = base.alt_modes.iter().find(|(short, _)| d.attachments.contains(short)) {
            d.alt = alt_id.clone();
        }
        let d: &'static WeaponDef = Box::leak(Box::new(d));
        built.insert(canonical, d);
        if id != d.id {
            built.insert(id.to_string(), d);
        }
        Some(d)
    }

    pub fn attachment(&self, id: &str) -> Option<&'static AttachmentDef> {
        self.attachments.get(id).copied()
    }

    /// A camo option by id (`camo_multicam`), with its name and thumbnail image.
    pub fn camo(&self, id: &str) -> Option<&'static CamoOption> {
        self.camo.get(id).copied()
    }

    /// A reticle option by id (`reticle_3`), in BO2's base `reticle_0..10` order.
    pub fn reticle(&self, id: &str) -> Option<&'static CamoOption> {
        self.reticles.get(id).copied()
    }

    /// The base reticle options, in BO2's table order.
    pub fn reticles(&self) -> &[&'static CamoOption] {
        &self.reticle_order
    }

    /// A reticle colour by id (`reticle_color_1`), with its localized colour name.
    pub fn reticle_color(&self, id: &str) -> Option<&'static CamoOption> {
        self.reticle_colors.get(id).copied()
    }

    /// The base reticle colours, in BO2's table order.
    pub fn reticle_colors(&self) -> &[&'static CamoOption] {
        &self.reticle_color_order
    }

    pub fn all(&self) -> &[&'static WeaponDef] {
        &self.order
    }
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Install the converted BO2 table (once, before play). Returns the number of weapons, or why not.
pub fn install_catalog(text: &str) -> Result<usize, String> {
    let table = parse_full_table(text)?;
    let n = table.weapons.len();
    if !table.perks.is_empty() {
        super::perks::install_converted(&table.perks);
    }
    CATALOG.set(Catalog::from_table(table)).map_err(|_| "weapon catalog already installed".to_string())?;
    Ok(n)
}

/// The installed catalog, or the built-in one.
pub fn catalog() -> &'static Catalog {
    CATALOG.get().unwrap_or_else(|| {
        static BUILT_IN: OnceLock<Catalog> = OnceLock::new();
        BUILT_IN.get_or_init(|| Catalog::from_table(parse_full_table(BUILTIN).expect("built-in weapon table")))
    })
}

/// Unknown ids [`weapon`]/[`weapon_or_none`] were asked for, once each. A test asserts this is
/// empty, and [`note_unresolved`] prints each id once (the autotest captures the process's
/// stdout/stderr), so a kit that quietly became the catalog's first entry shows up.
static UNRESOLVED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn note_unresolved(id: &str) {
    let mut seen = UNRESOLVED.lock().unwrap_or_else(|e| e.into_inner());
    if seen.iter().any(|s| s == id) {
        return;
    }
    seen.push(id.to_string());
    // Loud on purpose: an unknown id used to be silently replaced by the AN-94, which was
    // indistinguishable from "the class pick did nothing" in the log.
    eprintln!("zhbo: unknown weapon id {id:?} (not in the weapon table)");
}

/// The weapon ids that did not resolve since the last [`clear_unresolved`]. The tests assert this
/// is empty after building a kit; the FFI layer logs it once per logic step when non-empty.
pub fn unresolved() -> Vec<String> {
    UNRESOLVED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Forget the unresolved ids (the tests, and a game reset).
pub fn clear_unresolved() {
    UNRESOLVED.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// The weapon with this id, or `None` when the table does not hold it — never a substitution.
/// Kit construction uses this so an unknown id leaves the slot out instead of handing the General
/// the catalog's first entry (the stock AN-94) with no trace.
pub fn weapon_or_none(id: &str) -> Option<&'static WeaponDef> {
    let def = catalog().get(id);
    if def.is_none() {
        note_unresolved(id);
    }
    def
}

/// The weapon with this id, falling back to the built-in table and then to its first entry.
/// Display-only callers use this; the substitution is recorded (and printed once per id) so it is
/// never silent. Kit construction uses [`weapon_or_none`].
pub fn weapon(id: &str) -> &'static WeaponDef {
    weapon_or_none(id).or_else(|| catalog().all().first().copied()).expect("weapon catalog is empty")
}

/// Stand-in table when BO2 is not converted: the default loadout's weapons with BO2's published
/// multiplayer values (rounded), in the same format as the converted table.
pub const BUILTIN: &str = "
Attachment reflex
  Short = reflex
  DisplayName = Reflex
  FullName = Reflex Sight
  Description = Precision red dot sight.
  AttributeDeltas = 0 5 15 0 0
  Point = 1
  Ads = 50 1 1 1
End

Attachment gl
  Short = gl
  DisplayName = Grenade Launcher
  Point = 2
End

Weapon gl_an94_mp
  DisplayName = Grenade Launcher
  Class = launcher
  Fire = semi
  FireTime = 0.22
  Clip = 1
  StartReserve = 1
  MaxReserve = 1
  Reload = 2.75 2.75 1.4 1.4
  Raise = 0.55 0.55 0.6 0.55 0.6
  Ads = 0.5 0.5 50
  Effect = explode
  ; explodes on impact; the fuse is the safety timeout (0 would go off in the barrel)
  Fuse = 2.5 0
  Explosion = 42.7 0 135 50
  Projectile = 300 0 1 1 0
  Alt = an94_mp
  Flags = 1 0 0 0 0
End

Weapon an94_mp
  DisplayName = AN-94
  Class = rifle
  Fire = auto
  FireTime = 0.096
  Clip = 30
  StartReserve = 90
  MaxReserve = 210
  Reload = 2.47 3.11 1.3 1.9
  Raise = 0.6 1.1 0.4 0.35 0.3
  SprintOutTime = 0.25
  Ads = 0.25 0.2 50
  Damage = 40 33 33 0 0 0
  DamageRange = 216.7 216.8 375 0 0 0
  HeadMultiplier = 1.2
  HipSpread = 3 6.75 2.5 6 2 5
  Spread = 6 0.5 0.5 0
  AdsKick = 30 47.5 -50 50
  HipKick = 30 47.5 -50 50
  Move = 0.95 0.5 1
  FxWeapon = ZHBO_MTAR_FX
  Viewmodel = 0
  Anims = 60000ee003f007ff85005e
  Allowed = reflex=reflex gl=gl
  AltModes = gl=gl_an94_mp
End

Weapon fnp45_mp
  DisplayName = Tac-45
  Class = pistol
  Fire = semi
  FireTime = 0.096
  Clip = 10
  StartReserve = 30
  MaxReserve = 70
  Reload = 1.48 1.8 1.1 1.4
  Raise = 0.25 0.6 0.25 0.2 0.2
  SprintOutTime = 0.15
  Ads = 0.12 0.1 60
  Damage = 55 45 45 0 0 0
  DamageRange = 58.3 58.4 125 0 0 0
  HeadMultiplier = 1.1
  HipSpread = 3 5 2.5 4.5 2 4
  Spread = 7 0.6 0.5 0.3
  AdsKick = 60 80 -40 40
  HipKick = 60 80 -40 40
  Move = 1 0.65 1
  FxWeapon = ZHBO_B23R_FX
  Viewmodel = 1
  Anims = 60007e60fff03fff85e256
End

Weapon frag_grenade_mp
  DisplayName = Grenade
  Class = lethal
  Fire = semi
  FireTime = 0.6
  Clip = 1
  StartReserve = 1
  MaxReserve = 2
  Effect = explode
  ExplosionFx = FX_BoobyTrapExplosion
  Fuse = 3.5 1
  Explosion = 42 0 150 50
  Projectile = 225 50 1 0 0
  Viewmodel = 2
  Anims = e800076
End

Weapon claymore_mp
  DisplayName = Claymore
  Class = lethal
  Fire = semi
  FireTime = 0.6
  Clip = 1
  StartReserve = 1
  MaxReserve = 2
  Effect = explode
  Fuse = 3.5 0
  Explosion = 42 0 150 50
  Projectile = 0 0 0 1 0
  Viewmodel = 2
  Place = plant
End

Weapon concussion_grenade_mp
  DisplayName = Concussion
  Class = tactical
  Fire = semi
  FireTime = 0.6
  Clip = 2
  StartReserve = 2
  MaxReserve = 3
  Effect = stun
  ExplosionFx = WeaponFX_RangerFlashBangGrenadeDetonation
  Fuse = 1.5 0
  Explosion = 67 0 4 2
  Projectile = 225 50 1 0 0
  Viewmodel = 3
  Anims = e800076
End

Weapon knife_held_mp
  DisplayName = Knife
  Class = melee
  Melee = 135 0.6
  Viewmodel = 4
  Anims = 60200e6003f007f3804256
End

; The five default classes (data/ConquerOps/classes.ini) name BO2's own guns, so those ids must
; resolve even with no converted table: they stand in at the schema's generic values (name and
; class only — no BO2 numbers are copied here). The player's converted table replaces every one
; of these entries with the real weapon, which the mod's requirements check guarantees exists.
Weapon tar21_mp
  DisplayName = MTAR
  Class = rifle
End

Weapon mp7_mp
  DisplayName = MP7
  Class = smg
  Allowed = reflex=reflex
End

Weapon mk48_mp
  DisplayName = MK48
  Class = lmg
End

Weapon 870mcs_mp
  DisplayName = 870 MCS
  Class = shotgun
End

Weapon svu_mp
  DisplayName = SVU
  Class = sniper
End

Weapon smaw_mp
  DisplayName = SMAW
  Class = launcher
End

Weapon fiveseven_mp
  DisplayName = Five-Seven
  Class = pistol
End
";