/*
** Conquer Ops - Zero Hour gameplay core: the installed weapon catalog (the converted BO2 table, or the built-in stand-ins).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::def::{BonusCard, CamoOption, PerkRow, WeaponDef};
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
    bonus_cards: Vec<&'static BonusCard>,
    perks: HashMap<String, &'static PerkRow>,
    ui_text: HashMap<String, &'static str>,
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
        // The wildcard grid's order: `CoD.GetUnlockablesByGroupName("bonuscard")` sorted by
        // `UnlockablesComparisonFunction` — the sort key, then the item index (codbase.lua; G17).
        let mut bonus_cards: Vec<&'static BonusCard> = t.bonus_cards.into_iter().map(|b| &*Box::leak(Box::new(b))).collect();
        bonus_cards.sort_by_key(|b| (b.sort, b.index));
        let perks = t.perks.into_iter().map(|p| (p.stats.to_ascii_uppercase(), &*Box::leak(Box::new(p)))).collect();
        let ui_text = t.ui_text.into_iter().map(|u| (u.key, &*Box::leak(u.text.into_boxed_str()))).collect();
        Catalog {
            by_id,
            order,
            attachments,
            camo,
            reticles,
            reticle_order,
            reticle_colors,
            reticle_color_order,
            bonus_cards,
            perks,
            ui_text,
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

    /// A localized UI string by its key (`MPUI_SECOND_SPECIAL_GRENADE_CAPS`); `None` when the table
    /// lacks it (a table from before the strings, or an install without the key).
    pub fn ui_text(&self, key: &str) -> Option<&'static str> {
        self.ui_text.get(key).copied()
    }

    /// BO2's bonus cards (the wildcards), in the grid's order.
    pub fn bonus_cards(&self) -> &[&'static BonusCard] {
        &self.bonus_cards
    }

    /// A bonus card by its statstable reference (`bonuscard_overkill`).
    pub fn bonus_card(&self, reference: &str) -> Option<&'static BonusCard> {
        self.bonus_cards.iter().copied().find(|b| b.reference == reference)
    }

    /// A perk's statstable row by its reference without `PERKS_` (`LIGHTWEIGHT`, `NINJA`).
    pub fn perk(&self, stats: &str) -> Option<&'static PerkRow> {
        self.perks.get(stats).copied()
    }

    pub fn all(&self) -> &[&'static WeaponDef] {
        &self.order
    }
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Install the converted BO2 table (once, before play). Returns the number of weapons, or why not.
pub fn install_catalog(text: &str) -> Result<usize, String> {
    let mut table = parse_full_table(text)?;
    let n = table.weapons.len();
    let built_in = parse_full_table(BUILTIN)?;
    // The UI strings a table converted before them lacks: the stand-ins (BO2's own).
    for u in built_in.ui_text {
        if !table.ui_text.iter().any(|t| t.key == u.key) {
            table.ui_text.push(u);
        }
    }
    if table.bonus_cards.is_empty() {
        // A table converted before the bonus cards were: the stand-in rows (BO2's own).
        table.bonus_cards = built_in.bonus_cards;
    }
    // The perk rows: a table converted before they carried their name and description (or before
    // it carried perks at all) keeps its specialty sets and takes the stand-ins' text (BO2's own).
    for b in built_in.perks {
        match table.perks.iter_mut().find(|p| p.stats.eq_ignore_ascii_case(&b.stats)) {
            Some(p) => {
                if p.name.is_empty() {
                    p.name = b.name;
                }
                if p.description.is_empty() {
                    p.description = b.description;
                }
            }
            None => table.perks.push(b),
        }
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
/// multiplayer values (rounded), in the same format as the converted table, BO2's sixteen perk rows
/// (`mp/statstable.csv:151-166`: name, description, specialties) so the perk grid and every perk's
/// effect have their rows, BO2's eight bonus cards (`mp/statstable.csv:178-185`) so the
/// wildcard grid has its rows, and the english UI strings of [`super::UI_TEXT_KEYS`]
/// (`en_code_post_gfx_mp.ff`). An ignored install test checks the rows and strings against the
/// player's install.
pub const BUILTIN: &str = "
UiText MPUI_PRIMARY_GRENADE_CAPS
  Text = LETHAL
End

UiText MPUI_SPECIAL_GRENADE_CAPS
  Text = TACTICAL
End

UiText MPUI_SECOND_SPECIAL_GRENADE_CAPS
  Text = SECOND TACTICAL
End

Perk LIGHTWEIGHT
  Name = Lightweight
  Description = Move faster.  Take no damage when falling.
  Specialties = fallheight movefaster
End

Perk HARDLINE
  Name = Hardline
  Description = Earn Scorestreaks faster.
  Specialties = earnmoremomentum
End

Perk BLIND_EYE
  Name = Blind Eye
  Description = Undetectable by AI controlled air support.
  Specialties = nottargetedbyairsupport
End

Perk FLAK_JACKET
  Name = Flak Jacket
  Description = Take less explosive damage.
  Specialties = flakjacket
End

Perk GHOST
  Name = Ghost
  Description = Cannot be detected by enemy UAVs while moving, planting or defusing bombs, or while controlling Scorestreaks.
  Specialties = gpsjammer
End

Perk TOUGHNESS
  Name = Toughness
  Description = Flinch less when shot.
  Specialties = bulletflinch
End

Perk COLD_BLOODED
  Name = Cold Blooded
  Description = Resistant to targeting systems including: Dual Band, Target Finder, Sensor Grenade, MMS, and player controlled aircraft.  No name or red crosshair when targeted.
  Specialties = immunemms immunenvthermal immunerangefinder nokillstreakreticle nomotionsensor noname
End

Perk FAST_HANDS
  Name = Fast Hands
  Description = Swap weapons faster and use equipment faster.  Reset the fuse when throwing back frag grenades.
  Specialties = fastequipmentuse fasttoss fastweaponswitch pin_back
End

Perk HARD_WIRED
  Name = Hard Wired
  Description = Immune to Counter-UAV and EMP.
  Specialties = immunecounteruav immuneemp
End

Perk SCAVENGER
  Name = Scavenger
  Description = Replenish ammo and equipment from enemies killed by non-explosive weapons.
  Specialties = scavenger
End

Perk DEXTERITY
  Name = Dexterity
  Description = \"Aim faster after sprinting.  Mantle and climb faster. \"
  Specialties = fastladderclimb fastmantle fastmeleerecovery sprintrecovery
End

Perk EXTREME_CONDITIONING
  Name = Extreme Conditioning
  Description = Sprint for a longer duration.
  Specialties = longersprint
End

Perk HACKER
  Name = Engineer
  Description = Show enemy equipment in the world.  Delay triggered explosives. Reroll and booby trap Care Packages.
  Specialties = delayexplosive showenemyequipment
End

Perk TACTICAL_MASK
  Name = Tactical Mask
  Description = Reduce the effects of Flashbangs, Concussion Grenades, and Shock Charges.
  Specialties = flashprotection proximityprotection stunprotection
End

Perk NINJA
  Name = Dead Silence
  Description = Move silently.
  Specialties = quieter
End

Perk AWARENESS
  Name = Awareness
  Description = Enemy movements are easier to hear.
  Specialties = loudenemies
End

BonusCard bonuscard_primary_gunfighter
  Name = Primary Gunfighter
  Description = Take a 3rd attachment for your primary weapon.
  Image = menu_mp_bonuscard_primary_gunfighter
  Slot = bonuscard1
  Cost = 1
  Sort = 6
  Index = 178
  UnlockLevel = 21
End

BonusCard bonuscard_secondary_gunfighter
  Name = Secondary Gunfighter
  Description = Take a 2nd attachment for your second weapon.
  Image = menu_mp_bonuscard_secondary_gunfighter
  Slot = bonuscard2
  Cost = 1
  Sort = 5
  Index = 179
  UnlockLevel = 18
End

BonusCard bonuscard_overkill
  Name = Overkill
  Description = Take a primary weapon as your second weapon.
  Image = menu_mp_bonuscard_overkill
  Slot = bonuscard3
  Cost = 1
  Sort = 4
  Index = 180
  UnlockLevel = 15
End

BonusCard bonuscard_perk_1_greed
  Name = Perk 1 Greed
  Description = Take a second Perk 1.
  Image = menu_mp_bonuscard_perk_1_greed
  Slot = bonuscard5
  Cost = 1
  Sort = 1
  Index = 181
  UnlockLevel = 3
End

BonusCard bonuscard_perk_2_greed
  Name = Perk 2 Greed
  Description = Take a second Perk 2.
  Image = menu_mp_bonuscard_perk_2_greed
  Slot = bonuscard6
  Cost = 1
  Sort = 2
  Index = 182
  UnlockLevel = 9
End

BonusCard bonuscard_perk_3_greed
  Name = Perk 3 Greed
  Description = Take a second Perk 3.
  Image = menu_mp_bonuscard_perk_3_greed
  Slot = bonuscard7
  Cost = 1
  Sort = 3
  Index = 183
  UnlockLevel = 12
End

BonusCard bonuscard_danger_close
  Name = Danger Close
  Description = Take a second Lethal.
  Image = menu_mp_bonuscard_danger_close
  Slot = bonuscard4
  Cost = 1
  Sort = 8
  Index = 184
  UnlockLevel = 27
End

BonusCard bonuscard_two_tacticals
  Name = Tactician
  Description = Take a tactical grenade in place of your lethal grenade.
  Image = menu_mp_bonuscard_two_tacticals
  Slot = bonuscard1
  Cost = 1
  Sort = 7
  Index = 185
  UnlockLevel = 24
End

Attachment reflex
  Short = reflex
  DisplayName = Reflex
  FullName = Reflex Sight
  Description = Precision red dot sight.
  AttributeDeltas = 0 5 15 0 0
  Point = 1
  Ads = 50 1 1 1
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
  CacSlot = primary
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
  CacSlot = secondary
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
  CacSlot = primarygrenade
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
  Projectile = 0 0 0 1 0
  Viewmodel = 2
  Place = plant
  Equipment = claymore
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
  Projectile = 0 0 0 1 0
  Viewmodel = 3
  Place = plant
  Equipment = trophy
End

Weapon knife_held_mp
  DisplayName = Knife
  Class = melee
  Melee = 135 0.6
  Viewmodel = 4
  Anims = 60200e6003f007f3804256
End

; The class melee weapon every class gets (BO2's `knife_mp`): a gun's melee strikes with it.
; Generic values, slower than the held knife's so the tests tell the two apart.
Weapon knife_mp
  DisplayName = Knife
  Class = melee
  Melee = 135 0.75
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
  Description = Fully automatic personal defense weapon.  Versatile and strong overall.
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