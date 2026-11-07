//! The optics' reticle options (`.zbr`): BO2's `mp/weaponoptions.csv` `reticle` rows as the lens
//! materials the mod draws.
//!
//! BO2 builds one table per optic column from those rows (`t6mp` `FUN_00529360`): row `n` of an
//! optic names the lens material the class's reticle option `n` swaps in (column 3; the RMR's own
//! in column 5). Each [`Reticles::sets`] entry is one such column in row order, so `set[0]` is the
//! stock material the optic's model carries (`mc/mtl_t6_attach_optic_reflex_reticle`) and
//! `set[n]` the option's (`..._reticle_0n`). The core matches a lens surface to its set by that
//! stock name and swaps in the chosen row's material.

use crate::io::{FormatError, Reader, Writer};
use crate::model::Material;

pub const MAGIC: [u8; 4] = *b"ZBR\0";
pub const VERSION: u32 = 3;

/// The blob's archive path (one per archive).
pub fn reticles_path() -> String {
    format!("{}reticles.zbr", crate::DIR)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reticles {
    /// One optic column each, by row: `[0]` the stock material, `[n]` reticle option `n`'s. A row
    /// the converter could not resolve keeps the stock material.
    pub sets: Vec<Vec<Material>>,
}

impl Reticles {
    /// The material reticle option `index` swaps in for the stock lens material `stock`, or `None`
    /// when no set starts with it (not an optic lens) or the set has no such row.
    pub fn option(&self, stock: &str, index: usize) -> Option<&Material> {
        self.sets.iter().find(|s| s.first().is_some_and(|m| m.name == stock)).and_then(|s| s.get(index))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(MAGIC, VERSION);
        w.count(self.sets.len());
        for s in &self.sets {
            w.count(s.len());
            for m in s {
                m.write(&mut w);
            }
        }
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Reticles, FormatError> {
        let mut r = Reader::open(data, MAGIC, "reticles", VERSION)?;
        let n = r.count(4)?;
        let mut sets = Vec::with_capacity(n);
        for _ in 0..n {
            let k = r.count(8)?;
            let mut set = Vec::with_capacity(k);
            for _ in 0..k {
                set.push(Material::read(&mut r)?);
            }
            sets.push(set);
        }
        r.finish()?;
        Ok(Reticles { sets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Reticle;

    fn lens(name: &str, scale: f32) -> Material {
        Material { name: name.into(), color: Some(format!("Art\\Textures\\{name}.tga")), reticle: Some(Reticle { scale, ..Default::default() }), ..Default::default() }
    }

    #[test]
    fn round_trips_and_finds_an_option_by_its_stock_material() {
        let r = Reticles {
            sets: vec![
                vec![lens("mc/mtl_t6_attach_optic_reflex_reticle", 14.6), lens("mc/mtl_t6_attach_optic_reflex_reticle_01", 14.6), lens("mc/mtl_t6_attach_optic_reflex_reticle_02", 41.0)],
                vec![lens("mc/mtl_t6_attach_optic_holo_reticle", 12.8)],
            ],
        };
        let bytes = r.to_bytes();
        assert_eq!(Reticles::from_bytes(&bytes).unwrap(), r);
        for n in 0..bytes.len() {
            assert!(Reticles::from_bytes(&bytes[..n]).is_err(), "prefix {n}");
        }
        assert_eq!(r.option("mc/mtl_t6_attach_optic_reflex_reticle", 2).map(|m| m.name.as_str()), Some("mc/mtl_t6_attach_optic_reflex_reticle_02"));
        assert!(r.option("mc/mtl_t6_attach_optic_holo_reticle", 1).is_none(), "no such row");
        assert!(r.option("mc/mtl_t6_wpn_ar_an94", 0).is_none(), "not a lens");
    }
}
