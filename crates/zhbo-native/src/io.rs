//! The blobs' byte layer: a little-endian writer, and a bounds-checked reader that turns every
//! short or corrupt read into an error (the core reads these at run time; a bad file must never
//! panic the game).

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum FormatError {
    /// Not this kind of blob (wrong magic).
    Magic { want: [u8; 4], got: [u8; 4] },
    /// A version this build does not read: reconvert.
    Version { kind: &'static str, want: u32, got: u32 },
    /// The data ended early or holds an impossible value.
    Corrupt(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Magic { want, got } => write!(f, "not a {} blob (starts {:?})", String::from_utf8_lossy(want), got),
            FormatError::Version { kind, want, got } => write!(f, "{kind} blob version {got}, this build reads {want} (reconvert the BO2 art)"),
            FormatError::Corrupt(m) => write!(f, "corrupt blob: {m}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// The most elements any one list may claim: far above any BO2 model, far below what a corrupt
/// count could make us allocate.
pub const MAX_COUNT: u32 = 1 << 22;

#[derive(Default)]
pub struct Writer {
    pub bytes: Vec<u8>,
}

impl Writer {
    pub fn header(magic: [u8; 4], version: u32) -> Writer {
        let mut w = Writer::default();
        w.bytes.extend_from_slice(&magic);
        w.u32(version);
        w
    }
    pub fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32s(&mut self, v: &[f32]) {
        for x in v {
            self.f32(*x);
        }
    }
    pub fn count(&mut self, n: usize) {
        self.u32(n as u32);
    }
    pub fn str(&mut self, s: &str) {
        self.count(s.len());
        self.bytes.extend_from_slice(s.as_bytes());
    }
    pub fn opt_str(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Check the magic and version, and position after them.
    pub fn open(data: &'a [u8], magic: [u8; 4], kind: &'static str, version: u32) -> Result<Reader<'a>, FormatError> {
        let mut r = Reader { data, at: 0 };
        let got: [u8; 4] = r.take(4)?.try_into().unwrap_or_default();
        if got != magic {
            return Err(FormatError::Magic { want: magic, got });
        }
        let v = r.u32()?;
        if v != version {
            return Err(FormatError::Version { kind, want: version, got: v });
        }
        Ok(r)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(|| FormatError::Corrupt(format!("{n} bytes wanted at {} of {}", self.at, self.data.len())))?;
        let s = &self.data[self.at..end];
        self.at = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, FormatError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<u32, FormatError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn f32(&mut self) -> Result<f32, FormatError> {
        let v = f32::from_bits(self.u32()?);
        if !v.is_finite() {
            return Err(FormatError::Corrupt(format!("non-finite float at {}", self.at - 4)));
        }
        Ok(v)
    }
    pub fn f32n<const N: usize>(&mut self) -> Result<[f32; N], FormatError> {
        let mut out = [0.0; N];
        for o in &mut out {
            *o = self.f32()?;
        }
        Ok(out)
    }
    /// A list length, refused past [`MAX_COUNT`] or past what the rest of the data could hold at
    /// `min_elem` bytes an element.
    pub fn count(&mut self, min_elem: usize) -> Result<usize, FormatError> {
        let n = self.u32()?;
        if n > MAX_COUNT || (n as usize).saturating_mul(min_elem) > self.data.len() - self.at {
            return Err(FormatError::Corrupt(format!("count {n} at {} exceeds the data", self.at - 4)));
        }
        Ok(n as usize)
    }
    pub fn str(&mut self) -> Result<String, FormatError> {
        let n = self.count(1)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| FormatError::Corrupt("a name is not UTF-8".into()))
    }
    pub fn opt_str(&mut self) -> Result<Option<String>, FormatError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.str()?)),
            v => Err(FormatError::Corrupt(format!("option tag {v}"))),
        }
    }
    /// The blob must end exactly here (trailing bytes mean a layout mismatch).
    pub fn finish(&self) -> Result<(), FormatError> {
        if self.at != self.data.len() {
            return Err(FormatError::Corrupt(format!("{} bytes left over", self.data.len() - self.at)));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_refuses_short_and_absurd_data() {
        let mut w = Writer::header(*b"TEST", 1);
        w.str("hi");
        w.u32(u32::MAX);
        let bytes = w.bytes;
        let mut r = Reader::open(&bytes, *b"TEST", "test", 1).unwrap();
        assert_eq!(r.str().unwrap(), "hi");
        assert!(r.count(1).is_err(), "a count larger than the data is corrupt");
        assert!(matches!(Reader::open(&bytes, *b"NOPE", "x", 1), Err(FormatError::Magic { .. })));
        assert!(matches!(Reader::open(&bytes, *b"TEST", "x", 2), Err(FormatError::Version { .. })));
        assert!(Reader::open(&bytes[..3], *b"TEST", "x", 1).is_err());
        let mut nan = Writer::default();
        nan.f32(f32::NAN);
        assert!(Reader { data: &nan.bytes, at: 0 }.f32().is_err());
    }
}
