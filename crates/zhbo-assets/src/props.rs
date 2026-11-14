//! Thrown, launched and placed BO2 weapons as Zero Hour objects (`ZHBO_Proj_<id>`), moved by
//! zhbo-core: a grenade or a launcher's round in flight, a stuck bolt, the placed equipment
//! (claymore, Bouncing Betty, C4, trophy system, tactical insertion, sensor grenade, shock charge,
//! Black Hat) and the planted Assault Shield.
//!
//! BO2 sizes every one of them against the same 72-inch player as the body, so they are drawn at
//! the General's size: the W3D skin at the art scale ([`character::SCALE`]) and the object at
//! [`OBJECT_SCALE`], the General's own `Scale` ([`zhbo_core::units::GENERAL_SCALE`]). A planted
//! shield then stands as tall beside its owner as the one on his arm, and a thrown grenade is the
//! size it was in his hand.

use crate::character::{self, Rig};
use crate::generals::{projectile_model, projectile_object};
use crate::textures::{finish, textures, Looks};
use crate::viewmodels::{base_id, find_model};
use std::collections::BTreeMap;
use zhbo_formats::{t6zone::Zone, w3d, xmodel::Model};

/// The `Scale` of every prop object: the General's ([`zhbo_core::units::GENERAL_SCALE`]).
pub const OBJECT_SCALE: f32 = zhbo_core::units::GENERAL_SCALE;

/// One converted prop: the BO2 model it shows (the planted shield already posed), its W3D rig and
/// skin, and its object's INI block.
pub(crate) struct Prop {
    pub shown: Model,
    pub rig: Rig,
    pub meshes: Vec<w3d::SkinMesh>,
    pub object: String,
    pub ini: String,
}

/// The BO2 model `d` shows in the world, if it is a prop: a thrown, launched or placed weapon's
/// `projectileModel` (else its world model); the Assault Shield's planted model
/// ([`crate::shield::deployed_model`]: `level.deployedshieldmodel` in its deploy animation's last
/// frame, not its pickup world model). `None` for a weapon that leaves nothing in the world.
pub(crate) fn shown_model(pool: &[&Zone], w: &zhbo_formats::weapon::Weapon, d: &zhbo_core::weapons::WeaponDef) -> Option<Result<Model, String>> {
    use zhbo_core::weapons::Class;
    let throws = d.class.is_offhand() || d.class == Class::Launcher || d.projectile_speed > 0.0;
    let shield = d.class == Class::Shield;
    if !(throws || shield) || d.class == Class::Melee {
        return None;
    }
    if shield {
        return Some(crate::shield::deployed_model(pool));
    }
    let name = w.projectile_model.clone().or_else(|| w.world_model.clone())?;
    Some(find_model(pool, &name))
}

/// Convert `d`'s prop (see [`shown_model`]); `None` when it has none or its model is missing.
pub(crate) fn prop(
    pool: &[&Zone],
    paks: &crate::paks::Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    w: &zhbo_formats::weapon::Weapon,
    d: &zhbo_core::weapons::WeaponDef,
    name: &str,
    looks: &mut Looks,
) -> Option<Prop> {
    use zhbo_core::weapons::Class;
    let shown = shown_model(pool, w, d)?.ok()?;
    // The planted shield is drawn from its deployed box's footing (the bottom middle of its
    // collision surfaces): Zero Hour's `Box` geometry has no offset, so the object stands there
    // (`zhbo_core::sim::deploy`) and the box rises from the ground. Everything else from its origin.
    let rig = match d.shield {
        Some(g) if d.class == Class::Shield => Rig::placed(&shown, character::scaled(g.deployed.footing())),
        _ => Rig::new(&shown, &[]),
    };
    let tex = textures(&shown, paks, out, looks);
    let mut meshes = character::skin(&rig, &shown, "P", &tex);
    finish(&mut meshes, looks);
    let object = projectile_object(&d.id.replace(['+', '~'], "_"));
    // No prop gives sight: a Sensor Grenade reveals by its scan's and its detonation's compass
    // pings ([`zhbo_core::sensor`]), not by clearing the fog.
    // The deployed Assault Shield is cover Zero Hour's units can shoot: solid, attackable, a box of
    // its collision surfaces (x is its facing) at its drawn size. The engine never applies the
    // damage — the bridge's hook hands it to the core's wear-down (`zhbo_logic_shield_damage`) —
    // so its health only tells the AI what it is up against: the script's deployed health.
    let body = if d.class == Class::Shield {
        let k = character::SCALE * OBJECT_SCALE;
        let (lo, hi) = crate::shield::collision_bounds(&shown);
        let half = |a: usize| (hi[a] - lo[a]) * 0.5 * k;
        let health = crate::shield::deployed_health_zh();
        format!(
            "  KindOf = IMMOBILE\n  Body = ActiveBody ModuleTag_02\n    MaxHealth = {health:.1}\n    InitialHealth = {health:.1}\n  End\n  \
             Geometry = Box\n  GeometryMajorRadius = {:.2}\n  GeometryMinorRadius = {:.2}\n  GeometryHeight = {:.2}\n  GeometryIsSmall = No\n",
            half(0),
            half(1),
            (hi[2] - lo[2]) * k
        )
    } else {
        "  KindOf = NO_COLLIDE INERT IMMOBILE UNATTACKABLE CLICK_THROUGH\n  Body = InactiveBody ModuleTag_02\n  End\n  Geometry = Sphere\n  GeometryMajorRadius = 1.0\n  GeometryIsSmall = Yes\n".to_string()
    };
    let ini = format!(
        "Object {object}\n  Draw = W3DModelDraw ModuleTag_01\n    OkToChangeModelColor = Yes\n    DefaultConditionState\n      Model = {name}\n    End\n  End\n  \
         ; Drawn at the General's size (zhbo_core::units::GENERAL_SCALE): BO2 sizes it against the same player.\n  Scale = {OBJECT_SCALE}\n{body}End\n\n"
    );
    Some(Prop { shown, rig, meshes, object, ini })
}

/// Thrown and launched things for `converted` weapons: their BO2 projectile (or world) model as a
/// W3D model, and a Zero Hour object that shows it (`ZHBO_Proj_<id>`). Sets `projectile_object`.
pub(crate) fn projectiles(
    pool: &[&Zone],
    paks: &crate::paks::Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    bo2: &[zhbo_formats::weapon::Weapon],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
    models: &mut BTreeMap<String, String>,
) -> String {
    let mut ini = String::from("; Generated by zhbo-assets: thrown, launched and placed BO2 weapons, moved by zhbo-core.\n\n");
    let mut looks = Looks::default();
    let mut made = Vec::new();
    let ids = prop_ids(defs, converted);
    for (n, id) in ids.iter().enumerate() {
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(d) = defs.iter_mut().find(|d| &d.id == id) else { continue };
        let name = projectile_model(n as i32);
        let Some(p) = prop(pool, paks, out, w, d, &name, &mut looks) else { continue };
        let hierarchy = name.replace("_SKN", "_SKL");
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &hierarchy, &p.meshes));
        out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), p.rig.hierarchy_file(&hierarchy));
        ini.push_str(&p.ini);
        d.projectile_object = p.object;
        // The missile's box: its `projectileModel`'s bounds (t6mp `FUN_007825b0`).
        d.projectile_bounds = projectile_bounds(pool, w);
        models.insert(id.clone(), name);
        made.push(id.as_str());
    }
    out.insert(PROJECTILES_INI_PATH.into(), ini.into_bytes());
    format!("projectiles {}", made.join(", "))
}

/// The weapons that lie on the ground when dropped: the converted base weapons BO2 drops
/// (`zhbo_core::weapons::droppable`: the `primary` inventory, never a carried killstreak gun) that
/// hold rounds (the held knife never drops: `dropweaponfordeath` needs ammo). Each is its BO2
/// `worldModel` as a Zero Hour object `ZHBO_Drop_<base>` (`names::drop_object`), lying on its side
/// as a dropped gun settles (BO2's item physics; the mod rests it), drawn at the General's scale. Every
/// def of that base (its combinations, its alt modes) names the object. Returns the summary line.
pub(crate) fn drops(
    pool: &[&Zone],
    paks: &crate::paks::Paks,
    out: &mut BTreeMap<String, Vec<u8>>,
    bo2: &[zhbo_formats::weapon::Weapon],
    defs: &mut [zhbo_core::weapons::WeaponDef],
    converted: &[String],
) -> String {
    let mut ini = String::from("; Generated by zhbo-assets: dropped BO2 weapons lying on the ground, placed by zhbo-core.\n\n");
    let mut looks = Looks::default();
    let mut made: Vec<String> = Vec::new();
    for id in converted {
        let Some(d) = defs.iter().find(|d| &d.id == id) else { continue };
        if !lies_on_ground(d) {
            continue;
        }
        let Some(w) = bo2.iter().find(|w| w.name == base_id(id)) else { continue };
        let Some(Ok(model)) = w.world_model.as_deref().map(|m| find_model(pool, m)) else { continue };
        let name = zhbo_core::names::drop_model(made.len() as u16);
        let p = drop_prop(paks, out, &lying(&model), &name, base_id(id), &mut looks);
        let hierarchy = name.replace("_SKN", "_SKL");
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &hierarchy, &p.meshes));
        out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), p.rig.hierarchy_file(&hierarchy));
        ini.push_str(&p.ini);
        made.push(base_id(id).to_string());
    }
    for d in defs.iter_mut() {
        let base = base_id(&d.id);
        if made.iter().any(|m| m == base) && lies_on_ground(d) {
            d.drop_object = zhbo_core::names::drop_object(base);
        }
    }
    // The Scavenger pack (`zhbo_core::sim::scavenger`): `scavenger_item_mp`'s world model as it
    // lies (an item at rest, upright), the object `names::SCAVENGER_PACK_OBJECT`.
    let pack = bo2.iter().find(|w| w.name == zhbo_core::names::SCAVENGER_ITEM).and_then(|w| w.world_model.as_deref()).map(|m| find_model(pool, m));
    let pack_note = match pack {
        Some(Ok(model)) => {
            let name = zhbo_core::names::drop_model(made.len() as u16);
            let mut p = drop_prop(paks, out, &model, &name, zhbo_core::names::SCAVENGER_ITEM, &mut looks);
            p.ini = p.ini.replacen(&format!("Object {}\n", zhbo_core::names::drop_object(zhbo_core::names::SCAVENGER_ITEM)), &format!("Object {}\n", zhbo_core::names::SCAVENGER_PACK_OBJECT), 1);
            let hierarchy = name.replace("_SKN", "_SKL");
            out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &hierarchy, &p.meshes));
            out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), p.rig.hierarchy_file(&hierarchy));
            ini.push_str(&p.ini);
            format!(", the Scavenger pack ({})", model.name)
        }
        Some(Err(e)) => format!(", no Scavenger pack ({e})"),
        None => format!(", no Scavenger pack ({} not in the pool)", zhbo_core::names::SCAVENGER_ITEM),
    };
    out.insert("Data\\INI\\Object\\ZHBO_BO2_Drops.ini".into(), ini.into_bytes());
    format!("dropped weapons {}{pack_note}", made.join(", "))
}

/// TranZit's wall buys (`zhbo_core::zombies::WallBuys`): each one's wall model (its map entity's
/// target, `zombies.ini` `[WallBuys]`) as the Zero Hour object `names::wall_buy_object`, laid on
/// the ground where the round engine stands it (the Zombies map has no wall to hang it on), as a
/// dropped weapon lies. Returns the summary line.
pub(crate) fn wall_buys(pool: &[&Zone], paks: &crate::paks::Paks, out: &mut BTreeMap<String, Vec<u8>>) -> String {
    let mut ini = String::from("; Generated by zhbo-assets: Zombies wall buys, stood by zhbo-core's round engine.\n\n");
    let mut looks = Looks::default();
    let mut made = Vec::new();
    let mut missing = Vec::new();
    for (n, b) in zhbo_core::zombies::WallBuys::get().buys.iter().enumerate() {
        let model = match find_model(pool, &b.model) {
            Ok(m) => m,
            Err(e) => {
                missing.push(format!("{} ({e})", b.weapon));
                continue;
            }
        };
        let name = zhbo_core::names::wall_buy_model(n as u16);
        let mut p = drop_prop(paks, out, &lying(&model), &name, &b.weapon, &mut looks);
        let object = zhbo_core::names::wall_buy_object(&b.weapon);
        p.ini = p.ini.replacen(&format!("Object {}\n", zhbo_core::names::drop_object(&b.weapon)), &format!("Object {object}\n"), 1);
        let hierarchy = name.replace("_SKN", "_SKL");
        out.insert(format!("Art\\W3D\\{name}.w3d"), w3d::skin_model(&name, &hierarchy, &p.meshes));
        out.insert(format!("Art\\W3D\\{hierarchy}.w3d"), p.rig.hierarchy_file(&hierarchy));
        ini.push_str(&p.ini);
        made.push(b.weapon.clone());
    }
    out.insert("Data\\INI\\Object\\ZHBO_ZM_WallBuys.ini".into(), ini.into_bytes());
    let missing = if missing.is_empty() { String::new() } else { format!(", no model: {}", missing.join(", ")) };
    format!("wall buys {}{missing}", made.join(", "))
}

/// Whether `d` can lie on the ground: BO2 drops it and it can hold rounds.
pub(crate) fn lies_on_ground(d: &zhbo_core::weapons::WeaponDef) -> bool {
    zhbo_core::weapons::droppable(d) && d.clip > 0
}

/// A dropped weapon's prop: the lying model's skin and its object, which nothing collides with or
/// targets.
fn drop_prop(paks: &crate::paks::Paks, out: &mut BTreeMap<String, Vec<u8>>, shown: &Model, name: &str, base: &str, looks: &mut Looks) -> Prop {
    let rig = Rig::new(shown, &[]);
    let tex = textures(shown, paks, out, looks);
    let mut meshes = character::skin(&rig, shown, "P", &tex);
    finish(&mut meshes, looks);
    let object = zhbo_core::names::drop_object(base);
    let ini = format!(
        "Object {object}\n  Draw = W3DModelDraw ModuleTag_01\n    OkToChangeModelColor = Yes\n    DefaultConditionState\n      Model = {name}\n    End\n  End\n  \
         ; Drawn at the General's size (zhbo_core::units::GENERAL_SCALE): BO2 sizes it against the same player.\n  Scale = {OBJECT_SCALE}\n  \
         KindOf = NO_COLLIDE INERT IMMOBILE UNATTACKABLE CLICK_THROUGH\n  Body = InactiveBody ModuleTag_02\n  End\n  Geometry = Sphere\n  GeometryMajorRadius = 1.0\n  GeometryIsSmall = Yes\nEnd\n\n"
    );
    Prop { shown: shown.clone(), rig, meshes, object, ini }
}

/// `model` on its side (a quarter turn about its forward axis) with its lowest point (LOD 0) on its
/// origin's height: a gun as it lies on the ground.
pub(crate) fn lying(model: &Model) -> Model {
    let turn = |v: [f32; 3]| [v[0], -v[2], v[1]];
    let mut out = model.clone();
    for s in &mut out.surfaces {
        for v in &mut s.vertices {
            v.position = turn(v.position);
            v.normal = turn(v.normal);
        }
    }
    let lod = out.lods.first().map_or(0..0, |l| l.first_surface..l.first_surface + l.surface_count);
    let low = out.surfaces[lod].iter().flat_map(|s| s.vertices.iter().map(|v| v.position[2])).fold(f32::MAX, f32::min);
    let low = if low.is_finite() { low } else { 0.0 };
    for s in &mut out.surfaces {
        for v in &mut s.vertices {
            v.position[2] -= low;
        }
    }
    out
}

/// Where [`projectiles`] writes its objects.
pub(crate) const PROJECTILES_INI_PATH: &str = "Data\\INI\\Object\\ZHBO_BO2_Projectiles.ini";

/// The projectile objects of `text` with their trails: each `(object, systems)` pair's particle
/// systems attached to the object's model (`ParticleSysBone`, at its origin: no bone of that name),
/// so they follow it in flight and leave their particles behind.
pub(crate) fn with_trails(text: &str, trails: &[(String, Vec<String>)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut current: Option<&[String]> = None;
    for line in text.split_inclusive('\n') {
        if let Some(name) = line.strip_prefix("Object ") {
            let name = name.trim();
            current = trails.iter().find(|(o, _)| o == name).map(|(_, s)| s.as_slice());
        }
        out.push_str(line);
        if line.trim_start().starts_with("Model = ") {
            for sys in current.take().unwrap_or_default() {
                out.push_str(&format!("      ParticleSysBone = None {sys}\n"));
            }
        }
    }
    out
}

/// The ids [`projectiles`] converts, in order: the converted weapons, their alt modes (an
/// under-barrel launcher's grenade) and the grenade a projectile becomes (the crossbow's
/// `explosive_bolt_mp`).
pub(crate) fn prop_ids(defs: &[zhbo_core::weapons::WeaponDef], converted: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = converted.to_vec();
    for id in converted {
        if let Some(d) = defs.iter().find(|d| &d.id == id) {
            if !d.alt.is_empty() {
                ids.push(d.alt.clone());
            }
            // A base weapon's alt modes (the under-barrel launcher `gl_an94_mp`) launch their own
            // projectile; every combo carrying one shares it (`share_projectiles`).
            ids.extend(d.alt_modes.iter().map(|(_, alt)| alt.clone()).filter(|a| !ids_contains(converted, a)));
            // The grenade its projectile becomes (the crossbow's `explosive_bolt_mp`) shows its own.
            if !d.spawns.is_empty() && !ids_contains(&ids, &d.spawns) {
                ids.push(d.spawns.clone());
            }
        }
    }
    ids.dedup();
    ids
}

fn ids_contains(ids: &[String], id: &str) -> bool {
    ids.iter().any(|x| x == id)
}

/// A combo or alt-mode def fires its base weapon's BO2 `projectileModel`: give every def without a
/// projectile object the one its base (`base_id`) was converted with — the crossbow with an optic,
/// the under-barrel launcher of any combo (`gl_an94_mp~an94_mp+gl`).
pub(crate) fn share_projectiles(defs: &mut [zhbo_core::weapons::WeaponDef]) {
    let objects: BTreeMap<String, (String, [f32; 6])> =
        defs.iter().filter(|d| !d.projectile_object.is_empty()).map(|d| (d.id.clone(), (d.projectile_object.clone(), d.projectile_bounds))).collect();
    for d in defs.iter_mut().filter(|d| d.projectile_object.is_empty()) {
        if let Some((o, b)) = objects.get(base_id(&d.id)) {
            d.projectile_object = o.clone();
            d.projectile_bounds = *b;
        }
    }
}

/// The bounds of `w`'s `projectileModel` (mins, maxs, inches; all 0 without one): what t6mp gives
/// the missile entity it fires (`FUN_00607cb0` -> `FUN_007825b0`, the model's `mins`/`maxs`).
pub(crate) fn projectile_bounds(pool: &[&Zone], w: &zhbo_formats::weapon::Weapon) -> [f32; 6] {
    match w.projectile_model.as_deref().map(|m| find_model(pool, m)) {
        Some(Ok(m)) => [m.mins[0], m.mins[1], m.mins[2], m.maxs[0], m.maxs[1], m.maxs[2]],
        _ => [0.0; 6],
    }
}

#[cfg(test)]
/// The world-space bounds Zero Hour draws `meshes` at: each vertex on its pivot's bind transform
/// (the W3D skin is bone-relative), times the object's `scale`.
pub(crate) fn drawn_bounds(rig: &Rig, meshes: &[w3d::SkinMesh], scale: f32) -> ([f32; 3], [f32; 3]) {
    let points = meshes.iter().flat_map(|m| m.positions.iter().zip(&m.bones).map(|(p, &b)| crate::xf::apply(rig.bind[b as usize], *p)));
    points.fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| ([0, 1, 2].map(|k| lo[k].min(p[k] * scale)), [0, 1, 2].map(|k| hi[k].max(p[k] * scale))))
}

/// The `Scale` an object block of an INI sets (1 when it sets none, as the engine reads it).
pub(crate) fn ini_scale(block: &str) -> f32 {
    block.lines().map(str::trim).find_map(|l| l.strip_prefix("Scale").and_then(|r| r.trim_start().strip_prefix('=')).and_then(|v| v.trim().parse().ok())).unwrap_or(1.0)
}

/// A triangle soup in Zero Hour world units (a development preview's input).
pub struct Soup {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Development preview: each prop `ids` names (every prop when empty) standing beside a General's
/// body, both as Zero Hour draws them — the body in BO2's standing alert pose at the art scale
/// times his INI `Scale`, the prop's converted W3D skin times its object's `Scale` (or
/// `prop_scale`, to see another), a planted shield on its `riotshield_placement_zoffset` origin,
/// anything else on the ground. With `bo2` both are BO2's own inches instead, the reference.
/// Returns (id, body, prop) per prop.
pub fn line_up(bo2_dir: &std::path::Path, ids: &[String], prop_scale: Option<f32>, bo2: bool) -> Result<Vec<(String, Soup, Soup)>, String> {
    let mp = crate::paks::MpPool::load(bo2_dir)?;
    let (weapons, atts) = mp.weapons()?;
    let pool = [&mp.patch, &mp.common_patch, &mp.common];
    let paks = crate::paks::Paks::open_for(bo2_dir, &pool)?;
    let patch_ui = crate::paks::load_zone(bo2_dir, "patch_ui_mp")?;
    let (table, roster) = crate::generals::class_table(bo2_dir, &weapons, &atts, &mp.patch, &patch_ui, &mut Vec::new());
    let converted = crate::weapons::converted_base(&weapons, &roster);
    let general = crate::generals::base_ini();
    let general_scale = ini_scale(general.split("\nObject ").nth(1).unwrap_or_default());
    let body_k = if bo2 { 1.0 } else { character::SCALE * general_scale };
    let faction = &crate::generals::FACTIONS[0];
    let zone = crate::paks::load_zone(bo2_dir, faction.zone)?;
    let body = zhbo_formats::xmodel::find(&zone, faction.body).map_err(|e| e.to_string())?;
    let stand = crate::viewmodels::find_anim(&pool, crate::shield::STOWED_POSE).ok_or("no standing pose")?;
    let body = crate::shield::posed_model(&body, &stand);
    let body = soup(&body, |p| p.map(|c| c * body_k));
    let zoffset = zhbo_core::tuning::Tuning::default().riotshield_placement_zoffset;
    let (mut out, mut looks, mut made) = (BTreeMap::new(), Looks::default(), Vec::new());
    for id in prop_ids(&table.weapons, &converted).iter().filter(|id| ids.is_empty() || ids.contains(id)) {
        let (Some(w), Some(d)) = (weapons.iter().find(|w| w.name == base_id(id)), table.weapons.iter().find(|d| &d.id == id)) else { continue };
        let Some(p) = prop(&pool, &paks, &mut out, w, d, "PREVIEW", &mut looks) else { continue };
        // The reference is BO2's model itself, in inches; the mod's is the converted skin (art
        // units: inches times the art scale) as the object draws it, times `k`.
        let k = prop_scale.unwrap_or_else(|| ini_scale(&p.ini));
        let unit = if bo2 { 1.0 } else { character::SCALE * k };
        let (positions, triangles) = if bo2 {
            let s = soup(&p.shown, |v| v);
            (s.positions, s.triangles)
        } else {
            let (mut positions, mut triangles) = (Vec::new(), Vec::new());
            for m in &p.meshes {
                let first = positions.len() as u32;
                positions.extend(m.positions.iter().zip(&m.bones).map(|(v, &b)| crate::xf::apply(p.rig.bind[b as usize], *v).map(|c| c * k)));
                triangles.extend(m.triangles.iter().map(|t| t.map(|i| first + i)));
            }
            (positions, triangles)
        };
        // Stand it beside the body: a planted shield where its object stands (BO2's model on its
        // origin's height; the mod's skin, drawn from the deployed box's footing, on the footing's),
        // the rest on the ground.
        let lo = positions.iter().fold([f32::MAX; 3], |a, p| [0, 1, 2].map(|i| a[i].min(p[i])));
        let hi = positions.iter().fold([f32::MIN; 3], |a, p| [0, 1, 2].map(|i| a[i].max(p[i])));
        let footing = d.shield.filter(|_| !bo2).map_or(0.0, |g| g.deployed.footing()[2]);
        let lift = if d.class == zhbo_core::weapons::Class::Shield { (zoffset + footing) * unit } else { -lo[2] };
        let shift = [-(lo[0] + hi[0]) * 0.5, 20.0 * body_k - lo[1], lift];
        let positions = positions.into_iter().map(|p| [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]]).collect();
        made.push((id.clone(), Soup { positions: body.positions.clone(), triangles: body.triangles.clone() }, Soup { positions, triangles }));
    }
    Ok(made)
}

fn soup(model: &Model, f: impl Fn([f32; 3]) -> [f32; 3]) -> Soup {
    let mut s = Soup { positions: Vec::new(), triangles: Vec::new() };
    let Some(lod) = model.lods.first() else { return s };
    for surf in &model.surfaces[lod.first_surface..lod.first_surface + lod.surface_count] {
        let first = s.positions.len() as u32;
        s.positions.extend(surf.vertices.iter().map(|v| f(v.position)));
        s.triangles.extend(surf.triangles.iter().map(|t| [first + t[0] as u32, first + t[2] as u32, first + t[1] as u32]));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A combo or an alt mode of a combo fires its base weapon's converted projectile.
    #[test]
    fn combos_share_their_base_weapons_projectile() {
        use zhbo_core::weapons::WeaponDef;
        let mut defs = vec![
            WeaponDef { id: "crossbow_mp".into(), projectile_object: "ZHBO_Proj_crossbow_mp".into(), ..Default::default() },
            WeaponDef { id: "crossbow_mp+acog".into(), ..Default::default() },
            WeaponDef { id: "gl_an94_mp".into(), projectile_object: "ZHBO_Proj_gl_an94_mp".into(), ..Default::default() },
            WeaponDef { id: "gl_an94_mp~an94_mp+gl".into(), ..Default::default() },
            WeaponDef { id: "an94_mp+gl".into(), ..Default::default() },
        ];
        share_projectiles(&mut defs);
        assert_eq!(defs[1].projectile_object, "ZHBO_Proj_crossbow_mp");
        assert_eq!(defs[3].projectile_object, "ZHBO_Proj_gl_an94_mp");
        assert_eq!(defs[4].projectile_object, "", "the rifle itself fires bullets");
    }

    /// The General's `Scale` (the hand-written object INI the converter fills in) and the props'
    /// are one number: the core's `GENERAL_SCALE`. A prop drawn at another scale than the body
    /// reads too small or too big beside him.
    #[test]
    fn props_are_drawn_at_the_generals_scale() {
        let general = crate::generals::base_ini();
        let scales: Vec<f32> = general.split("\nObject ").skip(1).map(ini_scale).collect();
        assert_eq!(scales.len(), crate::generals::FACTIONS.len(), "every General, the survivors too");
        assert!(scales.iter().all(|&s| s == zhbo_core::units::GENERAL_SCALE), "{scales:?}");
        assert_eq!(OBJECT_SCALE, zhbo_core::units::GENERAL_SCALE);
        assert_eq!(character::SCALE * OBJECT_SCALE, zhbo_core::units::DRAWN_ZH_PER_INCH);
        assert_eq!(ini_scale("Object A\n  Scale                 = 1.15\nEnd\n"), 1.15);
        assert_eq!(ini_scale("Object A\n  ColorScale = 2\nEnd\n"), 1.0);
    }

    /// Real install: every prop of the shipped table — each thrown, launched and placed weapon and
    /// the planted shield — is drawn at its BO2 size times the factor the General's body is drawn
    /// at (the art scale times his INI `Scale`), measured on the converted W3D skin and the object's
    /// own INI block; and the planted shield's collision box (the sim's and the object's) is the
    /// drawn model's collision surfaces.
    #[test]
    #[ignore]
    fn every_prop_is_drawn_at_the_bodys_scale() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        crate::test_catalog::install_shipped(&dir);
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (bo2, _) = mp.weapons().unwrap();
        let pool = [&mp.patch, &mp.common_patch, &mp.common];
        let paks = crate::paks::Paks::open_for(&dir, &pool).unwrap();
        let general = crate::generals::base_ini();
        let body_factor = character::SCALE * ini_scale(general.split("\nObject ").nth(1).unwrap());
        let defs: Vec<zhbo_core::weapons::WeaponDef> = zhbo_core::weapons::catalog().all().iter().map(|d| (*d).clone()).collect();
        let (mut out, mut looks, mut checked) = (BTreeMap::new(), Looks::default(), Vec::new());
        for d in defs.iter().filter(|d| !d.projectile_object.is_empty() && !d.id.contains(['+', '~'])) {
            let w = bo2.iter().find(|w| w.name == base_id(&d.id)).unwrap_or_else(|| panic!("{}: no BO2 weapon", d.id));
            let p = prop(&pool, &paks, &mut out, w, d, "PROBE", &mut looks).unwrap_or_else(|| panic!("{}: no prop", d.id));
            assert_eq!(p.object, d.projectile_object, "{}", d.id);
            // BO2's size: the shown model's own vertices (LOD 0), measured here, not by the converter.
            let lod = &p.shown.lods[0];
            let verts = p.shown.surfaces[lod.first_surface..lod.first_surface + lod.surface_count].iter().flat_map(|s| s.vertices.iter().map(|v| v.position));
            let (lo, hi) = verts.fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| ([0, 1, 2].map(|k| lo[k].min(v[k])), [0, 1, 2].map(|k| hi[k].max(v[k]))));
            let (dlo, dhi) = drawn_bounds(&p.rig, &p.meshes, ini_scale(&p.ini));
            for k in 0..3 {
                // Surfaces `finish` drops (an emblem decal) may shrink the drawn box a little.
                let (want, got) = ((hi[k] - lo[k]) * body_factor, dhi[k] - dlo[k]);
                assert!((got - want).abs() <= 0.02 * want.max(0.5), "{} axis {k}: drawn {got:.3} units, BO2 {want:.3} at the body's scale", d.id);
            }
            if let Some(g) = d.shield {
                let (clo, chi) = crate::shield::collision_bounds(&p.shown);
                assert_eq!((g.deployed.mins, g.deployed.maxs), (clo, chi), "the table's deployed box is the planted model's collision surfaces");
                // Drawn from the box's footing (Zero Hour's `Box` has no offset: the object stands
                // there and the geometry rises from it, centred): the posed collision surfaces sit
                // centred on the object's origin and rise from it.
                let f = g.deployed.footing();
                let k = character::SCALE * ini_scale(&p.ini);
                let drawn = |v: [f32; 3]| [0, 1, 2].map(|a| (v[a] - f[a]) * k);
                let (blo, bhi) = (drawn(clo), drawn(chi));
                for a in 0..2 {
                    assert!((blo[a] + bhi[a]).abs() < 1e-3, "axis {a}: the box is centred on the object ({blo:?}..{bhi:?})");
                }
                assert!(blo[2].abs() < 1e-3, "the box rises from the object's feet ({blo:?})");
                // And the skin drawn from that point: its posed vertices moved by the footing.
                let (dlo, _) = drawn_bounds(&p.rig, &p.meshes, ini_scale(&p.ini));
                assert!((dlo[2] - (lo[2] - f[2]) * k).abs() < 0.05, "the skin's bottom {dlo:?} is the model's bottom over the footing");
                // The panel slid up over the visor in the deploy animation: taller than the folded
                // carry model's surfaces would say about it, the leg and spikes (no collision) out.
                assert!(chi[2] - clo[2] > 50.0 && chi[0] - clo[0] < 12.0, "deployed collision {clo:?}..{chi:?}");
            }
            checked.push(d.id.clone());
        }
        println!("{} props: {}", checked.len(), checked.join(" "));
        for must in ["riotshield_mp", "frag_grenade_mp", "claymore_mp", "bouncingbetty_mp", "satchel_charge_mp", "trophy_system_mp", "tactical_insertion_mp", "sensor_grenade_mp", "proximity_grenade_mp", "pda_hack_mp", "crossbow_mp"] {
            assert!(checked.iter().any(|c| c == must), "{must} is a prop: {checked:?}");
        }
    }

    /// A gun lies on its side with its lowest point on the ground.
    #[test]
    fn a_dropped_gun_lies_on_its_side_on_the_ground() {
        use zhbo_formats::xmodel::{Lod, Surface, Vertex};
        let v = |p: [f32; 3]| Vertex { position: p, normal: [0.0, 0.0, 1.0], ..Default::default() };
        let model = Model {
            name: "gun".into(),
            bones: Vec::new(),
            surfaces: vec![Surface { vertices: vec![v([0.0, 0.0, -2.0]), v([30.0, 1.0, 5.0]), v([10.0, -1.0, 0.0])], ..Default::default() }],
            lods: vec![Lod { distance: 0.0, first_surface: 0, surface_count: 1 }],
            radius: 0.0,
            mins: [0.0; 3],
            maxs: [0.0; 3],
            materials: vec![None],
            hit_boxes: Vec::new(),
            coll_surfs: Vec::new(),
        };
        let l = lying(&model);
        let p: Vec<[f32; 3]> = l.surfaces[0].vertices.iter().map(|v| v.position).collect();
        assert_eq!(p, vec![[0.0, 2.0, 1.0], [30.0, -5.0, 2.0], [10.0, 0.0, 0.0]], "a quarter turn about x, lifted onto the ground");
        assert_eq!(l.surfaces[0].vertices[0].normal, [0.0, -1.0, 0.0]);
    }

    /// Real install, every weapon of the shipped table: it lies on the ground when dropped exactly
    /// when BO2's own data says a death drops it (`maydropweapon`: the `primary` inventory; not a
    /// carried killstreak gun, `isheldkillstreakweapon`; rounds to drop), read here from BO2's raw
    /// weapon, not from the converted def; and then its def names its base's `ZHBO_Drop_` object,
    /// whose model is its BO2 world model lying on its side, at the General's scale.
    #[test]
    #[ignore]
    fn every_weapon_drops_by_bo2s_rule_and_lies_as_its_world_model() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        crate::test_catalog::install_shipped(&dir);
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (bo2, _) = mp.weapons().unwrap();
        let pool = [&mp.patch, &mp.common_patch, &mp.common];
        let paks = crate::paks::Paks::open_for(&dir, &pool).unwrap();
        let body_factor = character::SCALE * OBJECT_SCALE;
        let (mut out, mut looks) = (BTreeMap::new(), Looks::default());
        let mut by_class: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut measured: Vec<String> = Vec::new();
        for d in zhbo_core::weapons::catalog().all() {
            let w = bo2.iter().find(|w| w.name == base_id(&d.id)).unwrap_or_else(|| panic!("{}: no BO2 weapon", d.id));
            // BO2's rule from its own data: inventory 0 is `primary` (`weapInventoryType_t`).
            let mode = bo2.iter().find(|x| x.name == d.id.split('~').next().unwrap_or(&d.id)).unwrap_or(w);
            let expected = mode.inventory == 0 && !w.carried_killstreak && w.clip_size > 0;
            assert_eq!(lies_on_ground(d), expected, "{}: the drop rule", d.id);
            let e = by_class.entry(format!("{:?}", d.class)).or_default();
            e.0 += 1;
            if !expected {
                assert!(d.drop_object.is_empty(), "{}: never dropped, no object", d.id);
                continue;
            }
            e.1 += 1;
            assert_eq!(d.drop_object, zhbo_core::names::drop_object(base_id(&d.id)), "{}: its base's ground object", d.id);
            if d.id.contains(['+', '~']) || measured.contains(&d.id) {
                continue;
            }
            let model = find_model(&pool, w.world_model.as_deref().unwrap_or_else(|| panic!("{}: no world model", d.id))).unwrap();
            let lie = lying(&model);
            let p = drop_prop(&paks, &mut out, &lie, "PROBE", base_id(&d.id), &mut looks);
            assert_eq!(p.object, d.drop_object);
            assert_eq!(ini_scale(&p.ini), zhbo_core::units::GENERAL_SCALE, "{}: drawn at the General's scale", d.id);
            let lod = &model.lods[0];
            let verts = model.surfaces[lod.first_surface..lod.first_surface + lod.surface_count].iter().flat_map(|s| s.vertices.iter().map(|v| v.position));
            let (lo, hi) = verts.fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| ([0, 1, 2].map(|k| lo[k].min(v[k])), [0, 1, 2].map(|k| hi[k].max(v[k]))));
            let (dlo, dhi) = drawn_bounds(&p.rig, &p.meshes, ini_scale(&p.ini));
            // On its side: its length along x, its height (BO2's z) across y, its width (y) up.
            for (k, bo2_axis) in [(0, 0), (1, 2), (2, 1)] {
                let (want, got) = ((hi[bo2_axis] - lo[bo2_axis]) * body_factor, dhi[k] - dlo[k]);
                assert!((got - want).abs() <= 0.02 * want.max(0.5), "{} axis {k}: drawn {got:.3}, BO2 {want:.3}", d.id);
            }
            assert!(dlo[2].abs() < 0.01, "{}: lies on the ground ({})", d.id, dlo[2]);
            measured.push(d.id.clone());
        }
        println!("drops by class (weapons, dropped): {by_class:?}; {} ground models", measured.len());
        for must in ["an94_mp", "fnp45_mp", "riotshield_mp", "smaw_mp", "crossbow_mp", "dsr50_mp", "ksg_mp"] {
            assert!(measured.iter().any(|m| m == must), "{must} lies on the ground: {measured:?}");
        }
    }

}
