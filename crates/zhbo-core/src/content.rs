/*
** Conquer Ops - Zero Hour gameplay core: the multiplayer content fingerprint.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The multiplayer content fingerprint: what every peer of a lockstep match must hold
//! identically, split into named parts so a refused join can say which part differs.
//!
//! Each player converts the weapon table from his own Black Ops II install. Installs differ in
//! ways that must not keep players apart: the language (every localized name and description),
//! the DLC packs (the Peacekeeper's lock, the DLC camos a weapon offers, the art that converts),
//! the order a file system lists files in. None of that reaches the sim: only the weapon table's
//! [`Role::Sim`] fields do, and those come from the multiplayer weapon pool every final-patch
//! install has (`patch_mp`, `common_patch_mp`, `common_mp`; CONTRACT.md "Weapons are BO2's
//! patched definitions"). So the fingerprint hashes the parsed data, field by field, and leaves
//! the presentation fields out; a localized or DLC-richer install fingerprints the same.
//!
//! BO2 gates DLC items by their owner (`Engine.HasDLCForItem(controller, item)` in
//! `changeclass.lua:214` and `cacweaponbutton.lua:540` asks the local controller) and only the
//! map packs by the whole party (`partylobby.lua:870-934`, `MPUI_DLC_WARNING_OTHER_PLAYERS`); a
//! DLC weapon's definition ships in the patch every player has. The mod does the same: the
//! Create-a-Class lock is the picker's own install (`CacLocked`, presentation), and a peer
//! without the pack still runs the identical sim for it.
//!
//! The parts ([`Part`]): the engine's own INI CRC (the INIs Zero Hour loads, the converted
//! archive's Object/Weapon/FXList files among them; the bridge supplies it), the weapon table's
//! sim fields, the class-pick vocabulary ([`crate::classwire`]'s indices), the tuning pack, the
//! class list, the sim settings the environment sets, and the mod build. [`fingerprint`] folds
//! them into the one number the lobbies compare (`m_iniCRC`).

use crate::weapons::table::{Field, Role, Table};

/// The parts of the fingerprint, in [`ContentCrc::parts`] order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Part {
    /// Zero Hour's INI CRC before the mod's part: the engine's INIs and the converted archive's.
    Engine,
    /// The weapon table's sim fields ([`Role::Sim`]), every block in table order.
    Weapons,
    /// The ids the class pick is encoded against ([`crate::classwire`]): weapons in catalog
    /// order, the camo, reticle and reticle-colour rows, the perks.
    Vocabulary,
    /// The tuning pack's values ([`crate::tuning`]).
    Tuning,
    /// The shipped class list ([`crate::classes`]): a preset pick travels as its index.
    Classes,
    /// The sim inputs the environment sets (`ZHBO_LOADOUT`, `ZHBO_AUTOPILOT`).
    Settings,
    /// The core's build: the ABI version and field counts, the crate version, the table schema.
    Build,
}

/// How many parts a [`ContentCrc`] carries.
pub const PART_COUNT: usize = 7;

impl Part {
    pub const ALL: [Part; PART_COUNT] = [Part::Engine, Part::Weapons, Part::Vocabulary, Part::Tuning, Part::Classes, Part::Settings, Part::Build];

    /// The strings label naming the part in a mismatch message (`strings_en.ini`).
    pub fn label(self) -> &'static str {
        match self {
            Part::Engine => "LAN:ZHBO_ContentEngine",
            Part::Weapons => "LAN:ZHBO_ContentWeapons",
            Part::Vocabulary => "LAN:ZHBO_ContentVocabulary",
            Part::Tuning => "LAN:ZHBO_ContentTuning",
            Part::Classes => "LAN:ZHBO_ContentClasses",
            Part::Settings => "LAN:ZHBO_ContentSettings",
            Part::Build => "LAN:ZHBO_ContentBuild",
        }
    }
}

/// The fingerprint: each part's hash and the total the lobbies compare.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentCrc {
    pub parts: [u32; PART_COUNT],
    pub total: u32,
}

/// FNV-1a, 32 bits: stable across platforms and builds (no `std` hasher, whose seed and
/// algorithm are unspecified). Strings are length-prefixed, so bytes cannot move between fields.
#[derive(Clone, Copy)]
pub struct Hash(u32);

impl Default for Hash {
    fn default() -> Self {
        Hash(0x811C_9DC5)
    }
}

impl Hash {
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        for &x in b {
            self.0 = (self.0 ^ x as u32).wrapping_mul(0x0100_0193);
        }
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32).bytes(s.as_bytes())
    }
    pub fn get(&self) -> u32 {
        self.0
    }
}

/// Hash one block's sim fields: its header and id, then `key value` for each [`Role::Sim`] field
/// (the formatted value, the table's own text for it).
fn block<T>(h: &mut Hash, header: &str, id: &str, fields: &[Field<T>], t: &T) {
    h.str(header).str(id);
    for f in fields.iter().filter(|f| f.role == Role::Sim) {
        h.str(f.key).str(&(f.get)(t));
    }
}

/// The weapon table's [`Part::Weapons`] hash: the top-level sim lines (hit boxes, weapon groups,
/// class caps) and every block's sim fields, in the table's order. The camo, reticle and UI text
/// blocks carry only presentation (their ids and order are [`vocabulary`]'s); `TabletMeshes` is
/// the third-person art.
pub fn weapons(t: &Table) -> u32 {
    use crate::weapons::table::{ATTACHMENT_FIELDS, BONUS_CARD_FIELDS, PERK_FIELDS, WEAPON_FIELDS};
    let mut h = Hash::default();
    for b in &t.hit_boxes {
        h.str("HitBox").str(&b.format());
    }
    h.str("WeaponGroups");
    for g in &t.weapon_groups {
        h.str(g);
    }
    for (name, value) in &t.class_caps {
        h.str("ClassCap").str(name).u32(*value as u32);
    }
    for p in &t.perks {
        block(&mut h, "Perk", &p.stats, PERK_FIELDS, p);
    }
    for b in &t.bonus_cards {
        block(&mut h, "BonusCard", &b.reference, BONUS_CARD_FIELDS, b);
    }
    for k in &t.shocks {
        block(&mut h, "Shock", &k.name, crate::shellshock::SHOCK_FIELDS, k);
    }
    for a in &t.attachments {
        block(&mut h, "Attachment", &a.id, ATTACHMENT_FIELDS, a);
    }
    for d in &t.weapons {
        block(&mut h, "Weapon", &d.id, WEAPON_FIELDS, d);
    }
    h.get()
}

/// The [`Part::Vocabulary`] hash: what a class pick's indices resolve against
/// ([`crate::classwire`]): the weapons in catalog order, the camo, reticle and reticle-colour
/// option ids in BO2's row order, the perk ids. Every final-patch install lists the same rows
/// whatever DLC it has (the table keeps every camo row; a DLC weapon is listed, locked).
pub fn vocabulary(t: &Table) -> u32 {
    let mut h = Hash::default();
    h.str("Weapons");
    for d in &t.weapons {
        h.str(&d.id);
    }
    for (name, rows) in [("Camo", &t.camo), ("Reticle", &t.reticles), ("ReticleColor", &t.reticle_colors)] {
        h.str(name);
        for r in rows {
            h.str(&r.id);
        }
    }
    h.str("Perks");
    for p in crate::weapons::perks::PERKS {
        h.str(p.id);
    }
    h.get()
}

/// The [`Part::Tuning`] hash: every key and the bits of its value, then the `[MeansOfDeath]` and
/// `[ZeroHourDamage]` tables' rows (the parsed pack, so a comment or the spacing of the file changes
/// nothing).
pub fn tuning(t: &crate::tuning::Tuning) -> u32 {
    let mut h = Hash::default();
    for (key, v) in crate::tuning::Tuning::KEYS.iter().zip(t.values()) {
        h.str(key).u32(v.to_bits());
    }
    for (m, zh) in t.means_of_death.entries() {
        h.str(m.name()).str(zh);
    }
    for (zh, m) in t.zh_damage.entries() {
        h.str(zh).str(m.name());
    }
    h.get()
}

/// The [`Part::Classes`] hash: the class list in force, in order (id, name, spec).
pub fn classes(list: &[crate::classes::ClassDef]) -> u32 {
    let mut h = Hash::default();
    for c in list {
        h.str(&c.id).str(&c.name).str(&c.spec);
    }
    h.get()
}

/// The [`Part::Settings`] hash: `ZHBO_LOADOUT` (the base kit) and `ZHBO_AUTOPILOT` (spawns and
/// picks inside the logic step), as set (`None` and empty alike).
pub fn settings(loadout: Option<&str>, autopilot: Option<&str>) -> u32 {
    let mut h = Hash::default();
    h.str(loadout.unwrap_or_default()).str(autopilot.unwrap_or_default());
    h.get()
}

/// The [`Part::Build`] hash: the ABI version and table field counts, the core's version, and the
/// weapon table's schema (every block's keys and roles), so two builds that would read the same
/// table differently never meet.
pub fn build() -> u32 {
    use crate::weapons::table::{ATTACHMENT_FIELDS, BONUS_CARD_FIELDS, CAMO_FIELDS, PERK_FIELDS, UI_TEXT_FIELDS, WEAPON_FIELDS};
    fn schema<T>(h: &mut Hash, fields: &[Field<T>]) {
        h.u32(fields.len() as u32);
        for f in fields {
            h.str(f.key).u32(f.role as u32);
        }
    }
    let mut h = Hash::default();
    h.u32(crate::ffi::ABI_VERSION).u32(crate::ffi::LOGIC_API_FIELDS as u32).u32(crate::ffi::CLIENT_API_FIELDS as u32);
    h.str(env!("CARGO_PKG_VERSION"));
    schema(&mut h, ATTACHMENT_FIELDS);
    schema(&mut h, WEAPON_FIELDS);
    schema(&mut h, CAMO_FIELDS);
    schema(&mut h, PERK_FIELDS);
    schema(&mut h, BONUS_CARD_FIELDS);
    schema(&mut h, UI_TEXT_FIELDS);
    schema(&mut h, crate::shellshock::SHOCK_FIELDS);
    h.get()
}

/// Fold the parts into the total the lobbies compare (it replaces `m_iniCRC`).
pub fn total(parts: &[u32; PART_COUNT]) -> u32 {
    let mut h = Hash::default();
    for p in parts {
        h.u32(*p);
    }
    h.get()
}

/// The fingerprint of the content in force: the installed catalog's table, the loaded tuning pack
/// and class list, the environment's sim settings, this build, over the engine's INI CRC.
pub fn fingerprint(engine_ini: u32) -> ContentCrc {
    let (weapons, vocabulary) = crate::weapons::catalog_fingerprint();
    let env = |name: &str| std::env::var(name).ok();
    let mut parts = [0u32; PART_COUNT];
    parts[Part::Engine as usize] = engine_ini;
    parts[Part::Weapons as usize] = weapons;
    parts[Part::Vocabulary as usize] = vocabulary;
    parts[Part::Tuning as usize] = tuning(&crate::tuning::get());
    parts[Part::Classes as usize] = classes(&crate::classes::get());
    parts[Part::Settings as usize] = settings(env("ZHBO_LOADOUT").as_deref(), env("ZHBO_AUTOPILOT").as_deref());
    parts[Part::Build as usize] = build();
    ContentCrc { parts, total: total(&parts) }
}

/// The parts that differ between two fingerprints.
pub fn differing(ours: &[u32; PART_COUNT], theirs: &[u32; PART_COUNT]) -> Vec<Part> {
    Part::ALL.into_iter().filter(|p| ours[*p as usize] != theirs[*p as usize]).collect()
}

/// The short form shown in the lobby and the log: the total as eight hex digits.
pub fn short(total: u32) -> String {
    format!("{total:08X}")
}

/// The one-line lobby message for a refused join: which parts differ, by name
/// (`LAN:ZHBO_ContentMismatch` with the list in place of `%s`). A remote without the parts (an
/// older build: all zero) is a build difference.
pub fn describe_mismatch(ours: &[u32; PART_COUNT], theirs: &[u32; PART_COUNT]) -> String {
    let parts = if theirs.iter().all(|p| *p == 0) { vec![Part::Build] } else { differing(ours, theirs) };
    let names: Vec<String> = parts.iter().map(|p| crate::strings::text(p.label())).collect();
    let list = if names.is_empty() { crate::strings::text(Part::Build.label()) } else { names.join(", ") };
    crate::strings::text("LAN:ZHBO_ContentMismatch").replace("%s", &list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::table::{format_full_table, parse_full_table, ATTACHMENT_FIELDS, BONUS_CARD_FIELDS, CAMO_FIELDS, PERK_FIELDS, UI_TEXT_FIELDS, WEAPON_FIELDS};

    fn builtin() -> Table {
        parse_full_table(crate::weapons::BUILTIN).unwrap()
    }

    /// Set every presentation field of every block to another value: what a localized install,
    /// a DLC pack or a different art conversion changes.
    fn scramble_presentation(t: &mut Table, salt: &str) {
        fn each<T>(fields: &[Field<T>], t: &mut T, salt: &str) {
            for f in fields.iter().filter(|f| f.role == Role::Presentation) {
                // A value of the field's own shape: every number bumped, every word salted.
                let cur = (f.get)(t);
                if f.kind == crate::weapons::table::FieldKind::Enum {
                    continue;
                }
                let new = if f.kind == crate::weapons::table::FieldKind::Hex {
                    "ff".to_string()
                } else if cur.trim().is_empty() {
                    if matches!(f.kind, crate::weapons::table::FieldKind::Pairs) { format!("{salt}=x") } else { salt.to_string() }
                } else {
                    cur.split_whitespace()
                        .map(|w| match (w.parse::<f32>(), w.split_once('=')) {
                            (Ok(v), _) => format!("{}", v + 1.0),
                            _ if w == "-" => "1".to_string(),
                            (_, Some((k, _))) => format!("{k}={salt}"),
                            _ => format!("{salt}_{w}"),
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                // An optional group written `-` takes its full count of numbers.
                let ones = ["1"; 9].join(" ");
                let ok = [new.as_str(), ones.as_str()].into_iter().any(|text| {
                    let nums: Vec<f32> = text.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    (f.set)(t, crate::weapons::table::Values { text, nums: &nums }).is_ok()
                });
                assert!(ok, "{}: no value of its shape parses", f.key);
            }
        }
        for a in &mut t.attachments {
            each(ATTACHMENT_FIELDS, a, salt);
        }
        for d in &mut t.weapons {
            each(WEAPON_FIELDS, d, salt);
        }
        for c in t.camo.iter_mut().chain(&mut t.reticles).chain(&mut t.reticle_colors) {
            each(CAMO_FIELDS, c, salt);
        }
        for p in &mut t.perks {
            each(PERK_FIELDS, p, salt);
        }
        for b in &mut t.bonus_cards {
            each(BONUS_CARD_FIELDS, b, salt);
        }
        for u in &mut t.ui_text {
            each(UI_TEXT_FIELDS, u, salt);
        }
        for k in &mut t.shocks {
            each(crate::shellshock::SHOCK_FIELDS, k, salt);
        }
        t.tablet_meshes = t.tablet_meshes.wrapping_add(3);
    }

    #[test]
    fn presentation_fields_leave_the_fingerprint_alone() {
        let t = builtin();
        let mut other = t.clone();
        scramble_presentation(&mut other, "fr");
        assert_ne!(format_full_table(&t), format_full_table(&other), "the scramble changed the text");
        // Through the text, as the bridge hands it over.
        let other = parse_full_table(&format_full_table(&other)).unwrap();
        assert_eq!(weapons(&other), weapons(&t));
        assert_eq!(vocabulary(&other), vocabulary(&t));
    }

    #[test]
    fn every_sim_field_moves_the_fingerprint() {
        // One sim field of one weapon changed (a patch-level balance number): the weapon part moves
        // and the vocabulary stays.
        let t = builtin();
        let mut other = t.clone();
        other.weapons[0].clip += 1;
        assert_ne!(weapons(&other), weapons(&t));
        assert_eq!(vocabulary(&other), vocabulary(&t));
        // A row more (a weapon an older patch lacks): both move.
        let mut other = t.clone();
        let mut extra = other.weapons[0].clone();
        extra.id = "peacekeeper_mp".into();
        other.weapons.push(extra);
        assert_ne!(weapons(&other), weapons(&t));
        assert_ne!(vocabulary(&other), vocabulary(&t));
        // The camo rows' order is the class pick's camo index.
        let mut other = t.clone();
        for id in ["camo_a", "camo_b"] {
            other.camo.push(crate::weapons::CamoOption { id: id.into(), ..Default::default() });
        }
        let base = vocabulary(&other);
        let n = other.camo.len();
        other.camo.swap(n - 2, n - 1);
        assert_ne!(vocabulary(&other), base);
    }

    #[test]
    fn mismatch_names_the_parts() {
        let _g = crate::strings::lock();
        let ours = [1, 2, 3, 4, 5, 6, 7];
        let mut theirs = ours;
        theirs[Part::Weapons as usize] = 9;
        theirs[Part::Tuning as usize] = 9;
        assert_eq!(differing(&ours, &theirs), [Part::Weapons, Part::Tuning]);
        let text = describe_mismatch(&ours, &theirs);
        assert!(text.contains(&crate::strings::text(Part::Weapons.label())) && text.contains(&crate::strings::text(Part::Tuning.label())), "{text}");
        assert!(!text.contains(&crate::strings::text(Part::Classes.label())), "{text}");
        // An older build sends no parts.
        assert!(describe_mismatch(&ours, &[0; PART_COUNT]).contains(&crate::strings::text(Part::Build.label())));
        assert_eq!(total(&ours), total(&ours));
        assert_ne!(total(&ours), total(&theirs));
        assert_eq!(short(0x1A2B3C), "001A2B3C");
    }

    #[test]
    fn tuning_and_classes_hash_the_parsed_data() {
        let a = crate::tuning::parse("g_speed = 190\n");
        let b = crate::tuning::parse("; a comment\n  g_speed   =   190.0\n");
        assert_eq!(tuning(&a), tuning(&b));
        assert_ne!(tuning(&a), tuning(&crate::tuning::parse("g_speed = 191\n")));
        // The means-of-death table is sim data too: a peer mapping an impact to another armour
        // row would deal other damage.
        assert_ne!(tuning(&a), tuning(&crate::tuning::parse("g_speed = 190\nMOD_IMPACT = SMALL_ARMS\n")));
        let basic = crate::classes::basic();
        let mut other = basic.clone();
        other.swap(0, 1);
        assert_ne!(classes(&basic), classes(&other), "a preset travels as its index");
        assert_ne!(settings(Some("primary=an94_mp"), None), settings(None, None));
        assert_eq!(settings(Some(""), None), settings(None, None));
    }

    /// The presentation side of the core: per machine by design (the local client and its gun
    /// position, the HUD, the class screens and the profile they edit, the renderer passes, the
    /// sounds, the conformance oracle, the C ABI's client calls), plus the schema and the catalog
    /// that only define and look up the fields, and the attachment combiner that copies each field
    /// into the combination's same field. Everything else is reachable from a logic step — among it
    /// the received class's repair (`classrepair::repair_received`, which `Sim::choose_class` runs on
    /// every machine) and the class-editor rules it applies (`classmenu/items.rs`, `legal.rs`,
    /// `draft.rs`, `wildcard.rs`: slot fit, compatibility, the Pick 10 costs and the cards).
    const PRESENTATION: &[&str] = &[
        "client/", "hud/", "classui/", "lui/", "nativevm/", "nativebody/", "conformance/",
        "classmenu/apply.rs", "classmenu/editor.rs", "classmenu/grid.rs", "classmenu/keys.rs", "classmenu/nav.rs",
        "classmenu/offer.rs", "classmenu/options.rs", "classmenu/personalize.rs", "classmenu/slot.rs", "classmenu/step.rs",
        "menu.rs", "ffi.rs", "sounds.rs", "soundlayers.rs", "torso.rs", "viewmodel.rs", "weapons/gunpos.rs",
        "profile.rs", "classicons.rs", "weapons/def.rs", "weapons/table.rs", "weapons/catalog.rs", "attachments.rs",
        "content.rs",
    ];

    /// The struct members each presentation key sets (the schema's `set` closures), by block.
    const MEMBERS: &[(&str, &[&str])] = &[
        ("DisplayName", &["display_name"]),
        ("FullName", &["full_name"]),
        ("Description", &["description"]),
        ("Image", &["image"]),
        ("SwayOverride", &["sway"]),
        ("AdsSwayOverride", &["ads_sway"]),
        ("ViewPosOverride", &["stand_move_f", "stand_rot_pitch", "stand_rot_roll", "strafe_rot_roll"]),
        ("ViewFlashFx", &["view_flash_fx"]),
        ("ViewShellFx", &["view_shell_fx"]),
        ("ViewLastShotShellFx", &["view_last_shot_shell_fx"]),
        ("FireSound", &["fire_sound"]),
        ("EmptyFireSound", &["empty_fire_sound"]),
        ("SilencedFireSound", &["silenced_fire_sound"]),
        ("SilencedFireSoundNpc", &["silenced_fire_sound_npc"]),
        ("SilencedViewFlashFx", &["silenced_view_flash_fx"]),
        ("RaiseSound", &["raise_sound"]),
        ("FirstRaiseSound", &["first_raise_sound"]),
        ("AltSwitchSound", &["alt_switch_sound"]),
        ("HudIconRatio", &["hud_icon_ratio"]),
        ("PickupSound", &["pickup_sound"]),
        ("AmmoPickupSound", &["ammo_pickup_sound"]),
        ("PickupSoundNpc", &["npc_pickup_sound"]),
        ("AmmoPickupSoundNpc", &["npc_ammo_pickup_sound"]),
        ("PutawaySound", &["putaway_sound"]),
        ("AdsZoomSound", &["ads_zoom_sound"]),
        ("MeleeSwipeSound", &["melee_swipe_sound"]),
        ("MeleeHitSound", &["melee_hit_sound"]),
        ("FireStartSound", &["fire_start_sound"]),
        ("FireLastSound", &["fire_last_sound"]),
        ("PullbackSound", &["pullback_sound"]),
        ("NpcSounds", &["npc_sounds"]),
        ("TorsoAnims", &["torso_anims"]),
        ("SilencedFireStartSound", &["silenced_fire_start_sound"]),
        ("AttachmentFireSounds", &["attachment_fire_sounds"]),
        ("AttachmentFireStartSounds", &["attachment_fire_start_sounds"]),
        ("BounceSounds", &["bounce_sounds"]),
        ("Viewmodel", &["viewmodel"]),
        ("Anims", &["anims"]),
        ("ClipBase", &["clip_base"]),
        ("WorldMeshes", &["world_meshes"]),
        ("Attributes", &["attributes"]),
        ("AttachmentAttributes", &["attachment_attributes"]),
        ("CacGroup", &["cac_group"]),
        ("CacName", &["cac_name"]),
        ("CacLocked", &["cac_locked"]),
        ("CacLockHint", &["cac_lock_hint"]),
        ("CacOrder", &["cac_sort", "cac_index"]),
        ("DualWieldWeapon", &["dual_wield_weapon"]),
        ("LeftFireSound", &["left_fire_sound"]),
        ("HipSway", &["hip_sway"]),
        ("AdsSway", &["ads_sway"]),
        ("ViewStand", &["view_pos"]),
        ("ViewDucked", &["view_pos"]),
        ("ViewProne", &["view_pos"]),
        ("ViewStrafe", &["view_pos"]),
        ("ViewSprint", &["view_pos"]),
        ("ViewRates", &["view_pos"]),
        ("ViewAds", &["view_pos"]),
        ("SprintCycleScale", &["view_pos"]),
        ("CamoList", &["camo"]),
        ("CamoMaterial", &["camo_material"]),
        ("OverlayMaterialLowRes", &["overlay_material_low_res"]),
        ("DpadIcon", &["dpad_icon"]),
        ("LowAmmoWarning", &["low_ammo_threshold", "no_partial_reload", "ammo_counter"]),
        ("HudIcon", &["hud_icon"]),
        ("KillIcon", &["kill_icon"]),
        ("KillIconShape", &["kill_icon"]),
        ("Indicator", &["indicator"]),
        ("IndicatorIcon", &["indicator"]),
        ("WorldWeapon", &["world_weapon"]),
        ("EnemyTracerFxWeapon", &["enemy_tracer_fx_weapon"]),
        ("FireSoundNpc", &["npc_fire_sound"]),
        ("FireStartSoundNpc", &["npc_fire_start_sound"]),
        ("FireLastSoundNpc", &["npc_fire_last_sound"]),
        ("Name", &["name"]),
        ("Sort", &["sort"]),
        ("Tab", &["tab"]),
        ("Mtx", &["mtx"]),
        ("Text", &["text"]),
        ("SoundLoop", &["sound_loop"]),
    ];

    fn presentation_keys() -> Vec<&'static str> {
        fn of<T>(fields: &[Field<T>]) -> Vec<&'static str> {
            fields.iter().filter(|f| f.role == Role::Presentation).map(|f| f.key).collect()
        }
        let mut keys = [of(ATTACHMENT_FIELDS), of(WEAPON_FIELDS), of(CAMO_FIELDS), of(PERK_FIELDS), of(BONUS_CARD_FIELDS), of(UI_TEXT_FIELDS), of(crate::shellshock::SHOCK_FIELDS)].concat();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    #[test]
    fn every_presentation_key_names_its_members() {
        for k in presentation_keys() {
            assert!(MEMBERS.iter().any(|(m, _)| *m == k), "presentation key {k} has no MEMBERS row");
        }
    }

    /// The guard on the classification: no sim-reachable source reads a member a presentation key
    /// sets on a weapon-table type (the members' accesses `def.fire_sound`, `.view_pos`). A camo,
    /// reticle or class *choice* the sim carries is a [`crate::sim::PlayerSim`] or class string of
    /// the same name, not the table's row; those uses are listed with their reason.
    #[test]
    fn the_sim_reads_no_presentation_field() {
        // Same-named members of other types the sim carries (not the weapon table's).
        const OTHER_TYPES: &[(&str, &str)] = &[
            // Checked by hand (2026-11-01, `grep -nE '\.(camo|name|image|sort|text)\b'` over the
            // sim-reachable files): every hit is another type's member.
            ("camo", "the player's class camo choice (PlayerSim::camo, CustomClass::camo), never WeaponDef::camo"),
            ("name", "class, shock and build names (ClassDef::name, ShockDef::name), never a localized row name"),
            ("text", "the field parser's Values::text"),
            ("image", "no sim-reachable read"),
            ("sort", "no sim-reachable read"),
        ];
        // The received class's repair runs in the logic step (`Sim::choose_class`), so it and the
        // editor rules it applies are scanned, never listed as presentation.
        for reached in ["classrepair.rs", "classmenu/items.rs", "classmenu/legal.rs", "classmenu/draft.rs", "classmenu/wildcard.rs", "classmenu.rs"] {
            assert!(!PRESENTATION.iter().any(|p| reached.starts_with(p)), "{reached} is sim-reachable (repair_received)");
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        let mut dirs = vec![root.clone()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let path = e.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|x| x == "rs") {
                    files.push(path);
                }
            }
        }
        let members: Vec<&str> = presentation_keys()
            .into_iter()
            .flat_map(|k| MEMBERS.iter().find(|(m, _)| *m == k).map(|(_, ms)| *ms).unwrap_or_default().iter().copied())
            .filter(|m| !OTHER_TYPES.iter().any(|(o, _)| o == m))
            .collect();
        let mut found = Vec::new();
        for path in &files {
            let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            let name = rel.rsplit('/').next().unwrap_or_default();
            if PRESENTATION.iter().any(|p| rel.starts_with(p)) || name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "#[cfg(test)]" && lines.get(i + 1).is_some_and(|n| n.starts_with("mod ") && n.trim_end().ends_with('{')) {
                    break;
                }
                // A read the reviewer placed on the presentation side, with the reason, in a file
                // the sim also uses (`// content: presentation (<why>)`).
                if line.contains("// content: presentation (") {
                    continue;
                }
                let code = line.split("//").next().unwrap_or_default();
                for m in &members {
                    let pat = format!(".{m}");
                    let hit = code.match_indices(&pat).any(|(at, _)| {
                        let after = code[at + pat.len()..].chars().next();
                        !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    });
                    if hit {
                        found.push(format!("{rel}:{}: `.{m}` in `{}`", i + 1, line.trim()));
                    }
                }
            }
        }
        assert!(found.is_empty(), "sim-reachable code reads a presentation field (make it Role::Sim, or move the read to the client):\n{}", found.join("\n"));
    }
}
