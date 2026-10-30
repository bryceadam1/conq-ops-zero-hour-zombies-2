//! A weapon's camo recipes (`.zcm`): BO2's `WeaponCamo` asset as the mod applies it.
//!
//! The converter resolves the option ids to slots once (name matching + an explicit art table);
//! the core only looks up the chosen id. `sets` is the asset's global pattern list, `styles` the
//! per-style material recipes, and `materials` the converted style materials those pairs name.

use crate::io::{FormatError, Reader, Writer};
use crate::model::Material;

pub const MAGIC: [u8; 4] = *b"ZCM\0";
pub const VERSION: u32 = 1;

/// Archive path of `asset`'s camo blob (`camo_<weapon>`).
pub fn camo_path(asset: &str) -> String {
    format!("{}{}.zcm", crate::DIR, crate::file_stem(asset))
}

/// One pattern slot: the solid swatch, the tiling pattern, and this weapon's UV transform.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Set {
    pub solid: Option<String>,
    pub pattern: Option<String>,
    pub offset: [f32; 2],
    pub scale: f32,
    /// How strongly the layer tints the base colour (reference-tuned; C0).
    pub gain: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pair {
    pub base: String,
    pub camo: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub flags: u16,
    pub pairs: Vec<Pair>,
    pub consts: [f32; 8],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub entries: Vec<Entry>,
}

/// One camo option id resolved to its slots: `None` = the 48 assets carry no art for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CamoOption {
    pub id: String,
    pub pattern: Option<u32>,
    pub style: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Camo {
    pub name: String,
    pub solid_base: Option<String>,
    pub pattern_base: Option<String>,
    pub sets: Vec<Set>,
    pub styles: Vec<Style>,
    pub materials: Vec<Material>,
    pub options: Vec<CamoOption>,
}

fn write_opt_u32(w: &mut Writer, v: Option<u32>) {
    match v {
        Some(n) => {
            w.u8(1);
            w.u32(n);
        }
        None => w.u8(0),
    }
}

fn read_opt_u32(r: &mut Reader) -> Result<Option<u32>, FormatError> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(r.u32()?),
        c => return Err(FormatError::Corrupt(format!("option flag {c}"))),
    })
}

impl Camo {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(MAGIC, VERSION);
        w.str(&self.name);
        w.opt_str(self.solid_base.as_deref());
        w.opt_str(self.pattern_base.as_deref());
        w.count(self.sets.len());
        for s in &self.sets {
            w.opt_str(s.solid.as_deref());
            w.opt_str(s.pattern.as_deref());
            w.f32s(&s.offset);
            w.f32(s.scale);
            w.f32(s.gain);
        }
        w.count(self.styles.len());
        for st in &self.styles {
            w.count(st.entries.len());
            for e in &st.entries {
                w.u16(e.flags);
                w.count(e.pairs.len());
                for p in &e.pairs {
                    w.str(&p.base);
                    w.str(&p.camo);
                }
                w.f32s(&e.consts);
            }
        }
        w.count(self.materials.len());
        for m in &self.materials {
            m.write(&mut w);
        }
        w.count(self.options.len());
        for o in &self.options {
            w.str(&o.id);
            write_opt_u32(&mut w, o.pattern);
            write_opt_u32(&mut w, o.style);
        }
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Camo, FormatError> {
        let mut r = Reader::open(data, MAGIC, "camo", VERSION)?;
        let name = r.str()?;
        let solid_base = r.opt_str()?;
        let pattern_base = r.opt_str()?;
        let ns = r.count(2)?;
        let mut sets = Vec::with_capacity(ns);
        for _ in 0..ns {
            sets.push(Set { solid: r.opt_str()?, pattern: r.opt_str()?, offset: r.f32n()?, scale: r.f32()?, gain: r.f32()? });
        }
        let nst = r.count(4)?;
        let mut styles = Vec::with_capacity(nst);
        for _ in 0..nst {
            let ne = r.count(4)?;
            let mut entries = Vec::with_capacity(ne);
            for _ in 0..ne {
                let flags = r.u16()?;
                let np = r.count(8)?;
                let mut pairs = Vec::with_capacity(np);
                for _ in 0..np {
                    pairs.push(Pair { base: r.str()?, camo: r.str()? });
                }
                entries.push(Entry { flags, pairs, consts: r.f32n()? });
            }
            styles.push(Style { entries });
        }
        let nm = r.count(10)?;
        let mut materials = Vec::with_capacity(nm);
        for _ in 0..nm {
            materials.push(Material::read(&mut r)?);
        }
        let no = r.count(8)?;
        let mut options = Vec::with_capacity(no);
        for _ in 0..no {
            options.push(CamoOption { id: r.str()?, pattern: read_opt_u32(&mut r)?, style: read_opt_u32(&mut r)? });
        }
        r.finish()?;
        Ok(Camo { name, solid_base, pattern_base, sets, styles, materials, options })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Blend, CamoLayer, Cull};

    fn sample() -> Camo {
        Camo {
            name: "camo_an94".into(),
            solid_base: Some("Art\\Textures\\camo_off_solid.tga".into()),
            pattern_base: Some("Art\\Textures\\camo_off_pattern.tga".into()),
            sets: vec![
                Set { solid: Some("Art\\Textures\\t6_camo_atacs_solid.tga".into()), pattern: Some("Art\\Textures\\t6_camo_atacs_pattern.tga".into()), offset: [0.5, 0.31], scale: 4.0, gain: 2.0 },
            ],
            styles: vec![Style { entries: vec![
                Entry { flags: 6, pairs: vec![Pair { base: "mc/mtl_t6_wpn_ar_an94_camo1".into(), camo: "mc/mtl_weapon_camo_gold".into() }], consts: [10.0, 10.0, 0.46, 0.53, 0.0, 0.0, 0.0, 0.0] },
                Entry { flags: 0, pairs: Vec::new(), consts: [0.0; 8] },
            ] }],
            materials: vec![Material { name: "mc/mtl_weapon_camo_gold".into(), blend: Blend::Opaque, color: Some("Art\\Textures\\zhbo_camo_gold_col.tga".into()), maps: Some("Art\\Textures\\zhbo_camo_gold_col_nsg.tga".into()), sort_key: 0, cull: Cull::None, camo: CamoLayer::None, emissive: false, specular: Some("Art\\Textures\\zhbo_camo_gold_col_spec.tga".into()) }],
            options: vec![
                CamoOption { id: "camo_none".into(), pattern: None, style: None },
                CamoOption { id: "camo_atacs".into(), pattern: Some(0), style: None },
                CamoOption { id: "camo_gold".into(), pattern: None, style: Some(0) },
            ],
        }
    }

    #[test]
    fn round_trips_and_refuses_prefixes() {
        let c = sample();
        let bytes = c.to_bytes();
        assert_eq!(Camo::from_bytes(&bytes).unwrap(), c);
        for n in 0..bytes.len() {
            assert!(Camo::from_bytes(&bytes[..n]).is_err(), "prefix {n}");
        }
        let mut longer = bytes;
        longer.push(0);
        assert!(Camo::from_bytes(&longer).is_err(), "trailing bytes");
    }
}
