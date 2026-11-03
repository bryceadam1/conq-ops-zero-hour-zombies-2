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
    // A Sensor Grenade sees for its owner (and finds stealthed units).
    let sight = if base_id(&d.id) == "sensor_grenade_mp" {
        let r = zhbo_core::equipment::SENSOR_RANGE;
        format!("  VisionRange = {r}\n  ShroudClearingRange = {r}\n  Behavior = StealthDetectorUpdate ModuleTag_03\n    DetectionRate = 500\n    DetectionRange = {r}\n  End\n")
    } else {
        String::new()
    };
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
            "  KindOf = IMMOBILE\n  Body = ActiveBody ModuleTag_02\n    MaxHealth = {health:.1}\n    InitialHealth = {health:.1}\n  End\n{sight}  \
             Geometry = Box\n  GeometryMajorRadius = {:.2}\n  GeometryMinorRadius = {:.2}\n  GeometryHeight = {:.2}\n  GeometryIsSmall = No\n",
            half(0),
            half(1),
            (hi[2] - lo[2]) * k
        )
    } else {
        format!("  KindOf = NO_COLLIDE INERT IMMOBILE UNATTACKABLE CLICK_THROUGH\n  Body = InactiveBody ModuleTag_02\n  End\n{sight}  Geometry = Sphere\n  GeometryMajorRadius = 1.0\n  GeometryIsSmall = Yes\n")
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
        models.insert(id.clone(), name);
        made.push(id.as_str());
    }
    out.insert("Data\\INI\\Object\\ZHBO_BO2_Projectiles.ini".into(), ini.into_bytes());
    format!("projectiles {}", made.join(", "))
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
    let objects: BTreeMap<String, String> =
        defs.iter().filter(|d| !d.projectile_object.is_empty()).map(|d| (d.id.clone(), d.projectile_object.clone())).collect();
    for d in defs.iter_mut().filter(|d| d.projectile_object.is_empty()) {
        if let Some(o) = objects.get(base_id(&d.id)) {
            d.projectile_object = o.clone();
        }
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
        assert_eq!(scales.len(), 3, "three Generals");
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
}
