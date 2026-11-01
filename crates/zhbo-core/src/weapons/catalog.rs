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
    hud_strings: HashMap<String, &'static str>,
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
            hud_strings: t.hud_strings.into_iter().map(|(k, v)| (k, &*Box::leak(v.into_boxed_str()))).collect(),
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
    /// BO2's localized HUD string for a localize key ([`super::table::HUD_STRINGS`]), if the
    /// converted table carries it.
    pub fn hud_string(&self, key: &str) -> Option<&'static str> {
        self.hud_strings.get(key).copied()
    }

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

    /// The viewmodel whose third-person W3D meshes (`W<vm><k>`) the soldier shows for `def`: its
    /// own when the converter put its world model on the body (the base set), else the base
    /// weapon's — a combination (`mp7_mp+reflex`) or its alt mode (`gl_an94_mp~an94_mp+gl`) has a
    /// viewmodel of its own but no body meshes (`converted_base`), and the native third person
    /// draws it from its own `.zbw` instead.
    pub fn world_viewmodel(&self, def: &WeaponDef) -> i32 {
        if def.world_meshes > 0 {
            return def.viewmodel;
        }
        def.id
            .split('~')
            .filter_map(|part| self.by_id.get(attachments::split_id(part).0))
            .find(|b| b.world_meshes > 0)
            .map_or(def.viewmodel, |b| b.viewmodel)
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
  Point = 1
  Ads = 50 1 1 0
  Compatible = gl
End

Attachment gl
  Short = gl
  DisplayName = Grenade Launcher
  Point = 2
  Compatible = reflex
End

; Dual wield pairs with nothing (its `compatibleAttachments` is empty): a pistol with it is
; the separate `_dw_mp` weapon (G16), stood in below for the Five-seven.
Attachment dw
  Short = dw
  DisplayName = Dual Wield
  Point = 2
End

Weapon gl_an94_mp
  DisplayName = Grenade Launcher
  Class = launcher
  WeapClass = grenade
  Fire = semi
  FireTime = 0.22
  Clip = 1
  StartReserve = 1
  MaxReserve = 1
  Reload = 2.75 2.75 1.4 1.4
  Raise = 0.55 0.55 0.6 0.55 0.6
  EmptySwitch = 0.55 0.6
  AltSwitch = 0.7 0
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
  CacSlot = primary
  Fire = auto
  FireTime = 0.096
  Clip = 30
  StartReserve = 90
  MaxReserve = 240
  Reload = 2.47 3.11 1.3 1.9
  Raise = 0.6 1.1 0.4 0.35 0.3
  EmptySwitch = 0.6 0.4
  AltSwitch = 0.7 0
  SprintOutTime = 0.25
  Ads = 0.25 0.2 50
  Damage = 40 33 33 0 0 0
  DamageRange = 1300 1301 2250 0 0 0
  HeadMultiplier = 1.2
  HipSpread = 3 6.75 2.5 6 2 5
  Spread = 6 0.5 0.5 0
  AdsKick = 30 47.5 -50 50
  HipKick = 30 47.5 -50 50
  ViewKickCenter = 1500 1500
  Move = 0.95 0.5 1
  FxWeapon = ZHBO_MTAR_FX
  Viewmodel = 0
  Anims = 60000ee003f007ff85005e
  Allowed = reflex=reflex gl=gl
  Attributes = 60 60 55 50
  AttachmentAttributes = reflex=0,0,5,15
  AltModes = gl=gl_an94_mp
End

Weapon fnp45_mp
  DisplayName = Tac-45
  Class = pistol
  WeapClass = pistol
  Attributes = 30 75 30 65
  CacSlot = secondary
  Fire = semi
  FireTime = 0.096
  Clip = 10
  StartReserve = 30
  MaxReserve = 80
  Reload = 1.48 1.8 1.1 1.4
  Raise = 0.25 0.6 0.25 0.2 0.2
  EmptySwitch = 0.25 0.25
  SprintOutTime = 0.15
  Ads = 0.12 0.1 60
  Damage = 55 45 45 0 0 0
  DamageRange = 350 351 750 0 0 0
  HeadMultiplier = 1.1
  HipSpread = 3 5 2.5 4.5 2 4
  Spread = 7 0.6 0.5 0.3
  AdsKick = 60 80 -40 40
  HipKick = 60 80 -40 40
  ViewKickCenter = 1100 1100
  Move = 1 0.65 1
  FxWeapon = ZHBO_B23R_FX
  Viewmodel = 1
  Anims = 60007e60fff03fff85e256
End

Weapon frag_grenade_mp
  DisplayName = Grenade
  Class = lethal
  CacSlot = primarygrenade
  Fire = semi
  FireTime = 0.6
  Clip = 1
  StartReserve = 1
  MaxReserve = 2
  Effect = explode
  ExplosionFx = FX_BoobyTrapExplosion
  Fuse = 3.5 1
  ; Generic offhand states: pull-back, release-to-throw delay (no detonator).
  FireCycle = 0 0.1 0 0 0.5 0 0
  Offhand = 0 0 0
  Explosion = 42 0 150 50
  Projectile = 225 50 1 0 0
  Viewmodel = 2
  Anims = e800076
End

Weapon claymore_mp
  DisplayName = Claymore
  Class = equipment
  CacSlot = primarygrenade
  Fire = semi
  FireTime = 0.6
  Clip = 1
  StartReserve = 1
  MaxReserve = 2
  Effect = explode
  Fuse = 3.5 0
  Explosion = 42 0 150 50
  FireCycle = 0 0.15 0 0 0.2 0 0
  Offhand = 0 0 0
  Projectile = 0 0 0 1 0
  Viewmodel = 2
  Place = plant
  Equipment = claymore
End

; C4 (BO2's `satchel_charge_mp`): thrown and stuck; with none left in hand its button is the
; detonator (`hasDetonator`). Generic offhand times.
Weapon satchel_charge_mp
  DisplayName = C4
  Class = equipment
  CacSlot = primarygrenade
  Fire = semi
  FireTime = 0.6
  Clip = 2
  StartReserve = 2
  MaxReserve = 2
  Effect = explode
  Fuse = 3.5 0
  FireCycle = 0 0.15 0 0 0.1 0 0
  DetonateTime = 0.5
  Offhand = 0.2 1 0
  Explosion = 42 0 150 50
  Projectile = 100 0 1 0 1
  Viewmodel = 2
  Equipment = c4
End

Weapon concussion_grenade_mp
  DisplayName = Concussion
  Class = tactical
  CacSlot = specialgrenade
  Fire = semi
  FireTime = 0.6
  Clip = 2
  StartReserve = 2
  MaxReserve = 3
  Effect = stun
  ExplosionFx = WeaponFX_RangerFlashBangGrenadeDetonation
  Fuse = 1.5 0
  FireCycle = 0 0.1 0 0 0.5 0 0
  Offhand = 0 0 0
  Explosion = 67 0 4 2
  Projectile = 225 50 1 0 0
  Viewmodel = 3
  Anims = e800076
End

; Placed tactical equipment (BO2's `specialgrenade` slot lists it beside the grenades).
Weapon trophy_system_mp
  DisplayName = Trophy System
  Class = equipment
  CacSlot = specialgrenade
  Fire = semi
  FireTime = 0.6
  Clip = 1
  StartReserve = 1
  MaxReserve = 1
  FireCycle = 0 0.1 0 0 0.5 0 0
  Offhand = 0 0 0
  Projectile = 0 0 0 1 0
  Viewmodel = 3
  Place = plant
  Equipment = trophy
End

Weapon knife_held_mp
  DisplayName = Knife
  Class = melee
  Melee = 135 0.6
  MeleeDelay = 0.05
  Viewmodel = 4
  Anims = 60200e6003f007f3804256
End

; The Assault Shield stand-in (a primary of class `shield`): generic boxes in front of and behind a
; 72-inch body at generic values; the converted table poses BO2's own collmaps on the BO2 body.
Weapon riotshield_mp
  DisplayName = Assault Shield
  Class = shield
  CacSlot = primary
  CacGroup = weapon_special
  Fire = semi
  FireTime = 0.5
  Clip = 1
  StartReserve = 0
  MaxReserve = 0
  Move = 0.9 0.9 1
  Melee = 70 0.8
  MeleeDelay = 0.1
  Shield = 0 0 0 1 16 0 36 -4 -17 -26 4 17 26 0 0 1 0 -8 0 44 -4 -17 -14 4 17 14
End

; The class melee weapon every class gets (BO2's `knife_mp`): a gun's melee strikes with it.
; Generic values, slower than the held knife's so the tests tell the two apart.
Weapon knife_mp
  DisplayName = Knife
  Class = melee
  Melee = 135 0.75
  MeleeDelay = 0.1
  Viewmodel = 4
End

; `CacSlot` is the statstable slot each stand-in is a Create-a-Class item in (column 13).
; The five default classes (data/ConquerOps/classes.ini) name BO2's own guns, so those ids must
; resolve even with no converted table: they stand in at the schema's generic values (name and
; class only — no BO2 numbers are copied here). The player's converted table replaces every one
; of these entries with the real weapon, which the mod's requirements check guarantees exists.
Weapon tar21_mp
  DisplayName = MTAR
  Class = rifle
  CacSlot = primary
End

Weapon mp7_mp
  DisplayName = MP7
  Class = smg
  CacSlot = primary
  Allowed = reflex=reflex
End

Weapon mk48_mp
  DisplayName = MK48
  Class = lmg
  CacSlot = primary
End

Weapon 870mcs_mp
  DisplayName = 870 MCS
  Class = shotgun
  CacSlot = primary
End

Weapon svu_mp
  DisplayName = SVU
  Class = sniper
  CacSlot = primary
End

Weapon smaw_mp
  DisplayName = SMAW
  Class = launcher
  CacSlot = secondary
End

Weapon fiveseven_mp
  DisplayName = Five-Seven
  Class = pistol
  CacSlot = secondary
  Allowed = dw=dw
End

; A pistol by class that BO2 lists under SPECIALS (its statstable group, `CacGroup`).
Weapon knife_ballistic_mp
  DisplayName = Ballistic Knife
  Class = pistol
  CacSlot = secondary
  CacGroup = weapon_special
End

; What a class with the Five-seven and Dual Wield gets: no CaC item of its own.
Weapon fiveseven_dw_mp
  DisplayName = Five-Seven Dual Wield
  Class = pistol
  DualWieldWeapon = fiveseven_lh_mp
End
";
#[cfg(test)]
mod tests {
    use super::*;

    /// A converted combination has its own viewmodel but no body meshes: the W3D third person shows
    /// its base weapon's, and an alt mode's falls back through its parent combination.
    #[test]
    fn a_combination_shows_its_base_weapons_world_meshes() {
        let def = |id: &str, vm: i32, meshes: u8| WeaponDef { id: id.into(), viewmodel: vm, world_meshes: meshes, ..WeaponDef::default() };
        let c = Catalog::new(vec![def("mp7_mp", 5, 4), def("mp7_mp+reflex", 900, 0), def("gl_an94_mp~an94_mp+gl", 901, 0), def("an94_mp", 1, 5)]);
        let world = |id: &str| c.world_viewmodel(c.get(id).unwrap());
        assert_eq!(world("mp7_mp"), 5, "a base weapon shows its own");
        assert_eq!(world("mp7_mp+reflex"), 5, "a combination shows its base weapon's");
        assert_eq!(world("gl_an94_mp~an94_mp+gl"), 1, "an alt mode falls back through its parent");
    }
}
