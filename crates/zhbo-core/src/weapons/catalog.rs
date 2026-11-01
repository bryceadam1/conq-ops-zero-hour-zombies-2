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
    camo_order: Vec<&'static CamoOption>,
    reticles: HashMap<String, &'static CamoOption>,
    reticle_order: Vec<&'static CamoOption>,
    reticle_colors: HashMap<String, &'static CamoOption>,
    reticle_color_order: Vec<&'static CamoOption>,
    bonus_cards: Vec<&'static BonusCard>,
    perks: HashMap<String, &'static PerkRow>,
    ui_text: HashMap<String, &'static str>,
    /// The same strings upper-cased, for the screens BO2 draws through `UIExpression.ToUpper`.
    ui_upper: HashMap<String, &'static str>,
    weapon_groups: Vec<String>,
    class_caps: HashMap<String, i32>,
    built: Mutex<HashMap<String, &'static WeaponDef>>,
    pub tablet_meshes: u8,
    /// The soldier's hit boxes per body state (`HitBox` lines), `None` in a table without them.
    hit_body: Option<&'static crate::hitloc::Poses>,
    /// BO2's shellshocks by name (`Shock` blocks).
    shocks: Vec<&'static crate::shellshock::ShockDef>,
}

impl Catalog {
    pub fn new(defs: Vec<WeaponDef>) -> Catalog {
        Catalog::from_table(Table { weapons: defs, ..Default::default() })
    }

    pub fn from_table(t: Table) -> Catalog {
        let order: Vec<&'static WeaponDef> = t.weapons.into_iter().map(|d| &*Box::leak(Box::new(d))).collect();
        let by_id = order.iter().map(|d| (d.id.clone(), *d)).collect();
        let attachments = t.attachments.into_iter().map(|a| (a.id.clone(), &*Box::leak(Box::new(a)))).collect();
        let camo_order: Vec<&'static CamoOption> = t.camo.into_iter().map(|c| &*Box::leak(Box::new(c))).collect();
        let camo = camo_order.iter().map(|c| (c.id.clone(), *c)).collect();
        let reticle_order: Vec<&'static CamoOption> = t.reticles.into_iter().map(|r| &*Box::leak(Box::new(r))).collect();
        let reticles = reticle_order.iter().map(|r| (r.id.clone(), *r)).collect();
        let reticle_color_order: Vec<&'static CamoOption> = t.reticle_colors.into_iter().map(|c| &*Box::leak(Box::new(c))).collect();
        let reticle_colors = reticle_color_order.iter().map(|c| (c.id.clone(), *c)).collect();
        // The wildcard grid's order: `CoD.GetUnlockablesByGroupName("bonuscard")` sorted by
        // `UnlockablesComparisonFunction` — the sort key, then the item index (codbase.lua; G17).
        let mut bonus_cards: Vec<&'static BonusCard> = t.bonus_cards.into_iter().map(|b| &*Box::leak(Box::new(b))).collect();
        bonus_cards.sort_by_key(|b| (b.sort, b.index));
        let perks = t.perks.into_iter().map(|p| (p.stats.to_ascii_uppercase(), &*Box::leak(Box::new(p)))).collect();
        let ui_upper = t.ui_text.iter().map(|u| (u.key.clone(), &*Box::leak(u.text.to_uppercase().into_boxed_str()))).collect();
        let ui_text = t.ui_text.into_iter().map(|u| (u.key, &*Box::leak(u.text.into_boxed_str()))).collect();
        Catalog {
            by_id,
            order,
            attachments,
            camo,
            camo_order,
            reticles,
            reticle_order,
            reticle_colors,
            reticle_color_order,
            bonus_cards,
            perks,
            ui_text,
            ui_upper,
            weapon_groups: t.weapon_groups,
            class_caps: t.class_caps.into_iter().collect(),
            built: Mutex::new(HashMap::new()),
            tablet_meshes: t.tablet_meshes,
            hit_body: (!t.hit_boxes.is_empty()).then(|| &*Box::leak(Box::new(crate::hitloc::Poses::new(&t.hit_boxes)))),
            shocks: t.shocks.into_iter().map(|k| &*Box::leak(Box::new(k))).collect(),
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

    /// The create-a-class icon material of an attachment short name (`cac_mods_red_dot` for
    /// `reflex`): the image of its rows (BO2's variants of one short share it).
    pub fn attachment_image(&self, short: &str) -> Option<&'static str> {
        self.attachments.values().find(|a| a.short == short && !a.image.is_empty()).map(|a| a.image.as_str())
    }

    /// A camo option by id (`camo_multicam`), with its name and thumbnail image.
    pub fn camo(&self, id: &str) -> Option<&'static CamoOption> {
        self.camo.get(id).copied()
    }

    /// Every camo option in BO2's table order (`mp/attachmenttable.csv`'s `camo` rows; the
    /// converter writes one `Camo` block per row, none skipped).
    pub fn camos(&self) -> &[&'static CamoOption] {
        &self.camo_order
    }

    /// A camo option's BO2 group index: its row's place among the `camo` rows, the index
    /// `AddCamos` walks (`GetWeaponOptionGroupIndex(i, WEAPONOPTION_GROUP_CAMO)` for `i` in
    /// `0..GetNumWeaponOptions`). A weapon's offered list is a subsequence of this order.
    pub fn camo_index(&self, id: &str) -> Option<usize> {
        self.camo_order.iter().position(|c| c.id == id)
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

    /// BO2's `Engine.Localize(key)`: the install's text, the built-in stand-in without one, the
    /// key itself (loudly visible) when neither table has it.
    pub fn label(&self, key: &'static str) -> &'static str {
        self.ui_text(key).unwrap_or(key)
    }

    /// `UIExpression.ToUpper(Engine.Localize(key))`, for the headings and dialog entries BO2's Lua
    /// upper-cases (`cacclassoptions.lua:113-118`, `:181-187`).
    pub fn label_upper(&self, key: &'static str) -> &'static str {
        self.ui_upper.get(key).copied().unwrap_or(key)
    }

    /// BO2's Create-a-Class weapon groups (`weapon_smg`, `weapon_assault`, …) in the order its
    /// weapon-type list adds them (`cacweaponslot.lua`).
    pub fn weapon_groups(&self) -> &[String] {
        &self.weapon_groups
    }

    /// A `CoD.CACUtility` class cap by name (`maxPerksInEachCategory`), when the table has it.
    pub fn class_cap(&self, name: &str) -> Option<i32> {
        self.class_caps.get(name).copied()
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

    /// A shellshock by its file name (`concussion_grenade_mp`, `flashbang`).
    pub fn shock(&self, name: &str) -> Option<&'static crate::shellshock::ShockDef> {
        self.shocks.iter().copied().find(|k| k.name == name)
    }

    /// The soldier's hit boxes per body state, when the table carries them.
    pub fn hit_body(&self) -> Option<&'static crate::hitloc::Poses> {
        self.hit_body
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
    let mut table = parse_full_table(text)?;
    let n = table.weapons.len();
    let built_in = parse_full_table(BUILTIN)?;
    // The weapon groups a table converted before them lacks: the stand-ins (BO2's own).
    if table.weapon_groups.is_empty() {
        table.weapon_groups = built_in.weapon_groups;
    }
    // The class caps it lacks, likewise.
    for (name, value) in built_in.class_caps {
        if !table.class_caps.iter().any(|(n, _)| *n == name) {
            table.class_caps.push((name, value));
        }
    }
    // The UI strings a table converted before them lacks: the stand-ins (BO2's own).
    for u in built_in.ui_text {
        if !table.ui_text.iter().any(|t| t.key == u.key) {
            table.ui_text.push(u);
        }
    }
    if table.shocks.is_empty() {
        // A table converted before the shellshocks were: the stand-in rows (BO2's own).
        table.shocks = built_in.shocks;
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
/// player's install. `WeaponGroups` is the order `cacweaponslot.lua` adds the weapon groups in;
/// the `ClassCap.*` lines are `cacutility.lua`'s ([`crate::classes::DATA_CAPS`]).
pub const BUILTIN: &str = "
Shock concussion_grenade_mp
  Flashed = 1
  Blur = 0.4 12
  Flash = 7.5 15
  Look = 1 22 22 0.1 2
  Movement = 0.32
  Kick = 0.75 0.1
  Sound = 1 2.5 0.1
  SoundLoop = 
End
Shock flashbang
  Flashed = 1
  Blur = 0.4 1
  Flash = 3.5 1
  Look = 0 90 90 0.5 2
  Movement = 0.8
  Kick = 0.75 0.05
  Sound = 0.25 1 0.1
  SoundLoop = chr_flashbang_tinnitus_loop
End
Shock frag_grenade_mp
  Flashed = 0
  Blur = 0.4 1
  Flash = 0 0
  Look = 0 90 90 0.5 2
  Movement = 0.4
  Kick = 0.75 0.05
  Sound = 0.1 2 0
  SoundLoop = 
End
Shock proximity_grenade
  Flashed = 0
  Blur = 2 2
  Flash = 0 0
  Look = 1 1 1 0.05 4
  Movement = 0.1
  Kick = 2 0.75
  Sound = 0.25 0 0
  SoundLoop = 
End
Shock proximity_grenade_exit
  Flashed = 0
  Blur = 0.1 2
  Flash = 0 0
  Look = 1 30 30 0.2 2
  Movement = 0.525
  Kick = 2 0.05
  Sound = 0.25 0 0
  SoundLoop = 
End
WeaponGroups = weapon_smg weapon_assault weapon_cqb weapon_lmg weapon_sniper weapon_pistol weapon_launcher weapon_special
ClassCap.maxCustomClass = 5
ClassCap.maxGrenades = 2
ClassCap.maxSpecialtySlots = 6
ClassCap.maxPerks = 12
ClassCap.maxAttachments = 2
ClassCap.maxPerksInEachCategory = 2
ClassCap.maxBonusCards = 3
ClassCap.maxDefaultClass = 5

UiText MPUI_PRIMARY_GRENADE_CAPS
  Text = LETHAL
End

UiText MPUI_SPECIAL_GRENADE_CAPS
  Text = TACTICAL
End

UiText MPUI_SECOND_SPECIAL_GRENADE_CAPS
  Text = SECOND TACTICAL
End

UiText WEAPON_CANT_PLANT_EQUIPMENT
  Text = Cannot Plant Here
End

UiText MPUI_PRIMARY_CAPS
  Text = PRIMARY
End

UiText MPUI_SECONDARY_CAPS
  Text = SECONDARY
End

UiText MPUI_ATTACHMENTS_CAPS
  Text = ATTACHMENTS
End

UiText MENU_WILDCARDS_CAPS
  Text = WILDCARDS
End

UiText MPUI_ATTACHMENTS_NOT_AVAILABLE
  Text = Attachments not available
End

UiText MPUI_CHOOSE_CLASS_CAPS
  Text = CHOOSE CLASS
End

UiText MPUI_FIRE_RATE_CAPS
  Text = FIRE RATE
End

UiText MPUI_DAMAGE_CAPS
  Text = DAMAGE
End

UiText MPUI_RANGE_CAPS
  Text = RANGE
End

UiText MPUI_ACCURACY_CAPS
  Text = ACCURACY
End

UiText MENU_CLASS_OPTIONS
  Text = Class Options
End

UiText MENU_CLASS_SET_OPTIONS
  Text = Class Set Options
End

UiText MENU_RENAME
  Text = Rename
End

UiText MENU_COPY_CAPS
  Text = COPY
End

UiText MENU_RESET_TO_DEFAULT
  Text = Reset to Default
End

UiText MENU_CLEAR_LOADOUT_CAPS
  Text = CLEAR LOADOUT
End

UiText MENU_BACK
  Text = Back
End

UiText MENU_SELECT
  Text = Select
End

UiText MPUI_PERSONALIZE_KNIFE
  Text = Personalize Knife
End

UiText MPUI_WEAPON_SMG_CAPS
  Text = SUBMACHINE GUNS
End

UiText MPUI_WEAPON_ASSAULT_CAPS
  Text = ASSAULT RIFLES
End

UiText MPUI_WEAPON_CQB_CAPS
  Text = SHOTGUNS
End

UiText MPUI_WEAPON_LMG_CAPS
  Text = LIGHT MACHINE GUNS
End

UiText MPUI_WEAPON_SNIPER_CAPS
  Text = SNIPER RIFLES
End

UiText MPUI_WEAPON_SPECIAL_CAPS
  Text = SPECIALS
End

UiText MPUI_WEAPON_LAUNCHER_CAPS
  Text = LAUNCHERS
End

UiText MPUI_WEAPON_PISTOL_CAPS
  Text = PISTOLS
End

UiText MPUI_CAMO_CAPS
  Text = CAMO
End

UiText MPUI_RETICLES_CAPS
  Text = RETICLES
End

UiText MPUI_RETICLE_COLOR_CAPS
  Text = RETICLE COLOR
End

UiText MPUI_MAP_CATEGORY_DLC_CAPS
  Text = DLC
End

UiText MPUI_DLC2_CAPS
  Text = DLC2
End

UiText MENU_CUSTOMIZED_CAPS
  Text = CUSTOMIZED
End

UiText MENU_SCOREBOARD_CAPS
  Text = SCOREBOARD
End

UiText MPUI_SCORE_CAPS
  Text = SCORE
End

UiText MPUI_KILLS_CAPS
  Text = KILLS
End

UiText MPUI_DEATHS_CAPS
  Text = DEATHS
End

UiText MPUI_ASSISTS_CAPS
  Text = ASSISTS
End

UiText WEAPON_NO_AMMO
  Text = No Ammo
End

UiText MENU_SELECT_RETICLE
  Text = Select Reticle
End

UiText MENU_X_OPTICS
  Text = &&1 Reticles
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
  Image = cac_mods_red_dot
  DisplayName = Reflex
  FullName = Reflex Sight
  Description = Precision red dot sight.
  Point = 1
  Ads = 50 1 1 0
  Compatible = gl
End

Attachment gl
  Short = gl
  Image = hud_grenade_launcher
  DisplayName = Grenade Launcher
  Point = 2
  Compatible = reflex
End

; Dual wield pairs with nothing (its `compatibleAttachments` is empty): a pistol with it is
; the separate `_dw_mp` weapon (G16), stood in below for the Five-seven.
Attachment dw
  Short = dw
  Image = cac_mods_dual_wield
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
  Flags = 1 0 0 0
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
  LocationMultipliers = 1 1.2 1.2 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1
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
  LocationMultipliers = 1 1.1 1.1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1
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
  ExplodeOnGround = 1
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

    #[test]
    fn a_camo_keeps_its_bo2_group_index_when_a_weapon_offers_a_subset() {
        let camo: Vec<CamoOption> = ["camo_none", "camo_atacs", "camo_mtp", "camo_ukpunk"].iter().map(|id| CamoOption { id: (*id).into(), ..Default::default() }).collect();
        // A weapon whose install converted no art for `camo_mtp`: the converter trims it.
        let gun = WeaponDef { id: "an94_mp".into(), camo: vec!["camo_none".into(), "camo_atacs".into(), "camo_ukpunk".into()], ..Default::default() };
        let cat = Catalog::from_table(Table { weapons: vec![gun], camo, ..Default::default() });
        let ids: Vec<&str> = cat.camos().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["camo_none", "camo_atacs", "camo_mtp", "camo_ukpunk"], "the table's order");
        let offered: Vec<Option<usize>> = cat.get("an94_mp").unwrap().camo.iter().map(|c| cat.camo_index(c)).collect();
        assert_eq!(offered, [Some(0), Some(1), Some(3)], "the trimmed list keeps BO2's indices, not its own positions");
        assert_eq!(cat.camo_index("camo_unknown"), None);
    }
}
