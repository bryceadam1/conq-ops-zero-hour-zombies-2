//! The target finder's lens display (`.zrd`): what BO2's code draws into the ui3d texture for the
//! `rangefinder` attachment (`t6mp` `FUN_00526780`), converted from the player's install.
//!
//! BO2 draws, into ui3d window 5 (512 × 512 units, the lens's UVs 0..1) only above 0.99 ADS, six
//! sprites (idle) or ten (an enemy spotted) from the `mtl_t6_attach_optic_rangefinder_ui3d_background`
//! atlas, from a constant table its code builds on the stack, then a diamond from the same atlas on
//! every enemy player. The converter bakes the two sprite sets into window-sized frames ([`idle`],
//! [`spotted`]) and keeps the atlas and the diamond's atlas rectangle and size for the renderer,
//! which draws the diamonds where the marks cross the lens. The ui3d reticle samples the result
//! (its red/blue the two shapes, green/alpha the two glows).
//!
//! [`idle`]: RangeFinderDisplay::idle
//! [`spotted`]: RangeFinderDisplay::spotted

use crate::io::{FormatError, Reader, Writer};

pub const MAGIC: [u8; 4] = *b"ZRD\0";
pub const VERSION: u32 = 1;

/// The blob's archive path (one per archive).
pub fn display_path() -> String {
    format!("{}rangefinder.zrd", crate::DIR)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeFinderDisplay {
    /// The window with the six idle sprites, and with the ten spotted ones (`Art\Textures\...`, raw).
    pub idle: String,
    pub spotted: String,
    /// The sprite atlas (raw).
    pub atlas: String,
    /// The diamond's atlas rectangle: s0, t0, s1, t1.
    pub diamond_uv: [f32; 4],
    /// The diamond's size as a fraction of the window (`cg_rangeFinderDiamondSize` / 512).
    pub diamond_size: f32,
}

impl RangeFinderDisplay {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::header(MAGIC, VERSION);
        w.str(&self.idle);
        w.str(&self.spotted);
        w.str(&self.atlas);
        w.f32s(&self.diamond_uv);
        w.f32s(&[self.diamond_size]);
        w.bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<RangeFinderDisplay, FormatError> {
        let mut r = Reader::open(data, MAGIC, "rangefinder display", VERSION)?;
        let d = RangeFinderDisplay { idle: r.str()?, spotted: r.str()?, atlas: r.str()?, diamond_uv: r.f32n()?, diamond_size: r.f32()? };
        r.finish()?;
        if !d.diamond_uv.iter().all(|v| v.is_finite()) || !d.diamond_size.is_finite() {
            return Err(FormatError::Corrupt("non-finite diamond".into()));
        }
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_refuses_a_prefix() {
        let d = RangeFinderDisplay { idle: "a".into(), spotted: "b".into(), atlas: "c".into(), diamond_uv: [0.41, 0.27, 0.68, 0.8], diamond_size: 50.0 / 512.0 };
        let bytes = d.to_bytes();
        assert_eq!(RangeFinderDisplay::from_bytes(&bytes).unwrap(), d);
        for n in 0..bytes.len() {
            assert!(RangeFinderDisplay::from_bytes(&bytes[..n]).is_err(), "prefix {n}");
        }
    }
}
