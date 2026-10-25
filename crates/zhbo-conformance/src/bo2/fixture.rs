//! Synthetic BO2 weapon structs for the oracle's unit tests: a zone image built field by field
//! through the same layouts the oracle reads, so each G14 rule is pinned without an install.

use super::raw::{Variant, ANIMS, KINDS, UNIQUES};
use zhbo_formats::t6zone::layouts::{WeaponAttachment as WA, WeaponAttachmentUnique as WU, WeaponDef as WD, WeaponVariantDef as WV};
use zhbo_formats::t6zone::{self, Zone};

pub struct Mem {
    bytes: Vec<u8>,
    next: u32,
    strings: Vec<String>,
}

impl Mem {
    pub fn new() -> Mem {
        Mem { bytes: vec![0; 1 << 20], next: 0x100, strings: vec![String::new()] }
    }
    pub fn alloc(&mut self, size: u32) -> u32 {
        let at = self.next;
        self.next = (self.next + size + 15) & !15;
        at
    }
    pub fn put32(&mut self, at: u32, v: u32) {
        self.bytes[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn putf(&mut self, at: u32, v: f32) {
        self.put32(at, v.to_bits());
    }
    pub fn put8(&mut self, at: u32, v: u8) {
        self.bytes[at as usize] = v;
    }
    pub fn str(&mut self, s: &str) -> u32 {
        let at = self.alloc(s.len() as u32 + 1);
        self.bytes[at as usize..at as usize + s.len()].copy_from_slice(s.as_bytes());
        at
    }
    /// A `char*` field pointing at `s`.
    pub fn put_str(&mut self, field: u32, s: &str) {
        let p = self.str(s);
        self.put32(field, p);
    }
    /// An `XModel*`/`Material*` field: a struct whose first field names it.
    pub fn put_named(&mut self, field: u32, name: &str) {
        let obj = self.alloc(16);
        self.put_str(obj, name);
        self.put32(field, obj);
    }
    /// A `hideTags` list (script strings) at `field`.
    pub fn put_tags(&mut self, field: u32, tags: &[&str]) {
        let list = self.alloc(2 * (tags.len() as u32 + 1));
        for (i, t) in tags.iter().enumerate() {
            self.strings.push(t.to_string());
            let idx = (self.strings.len() - 1) as u16;
            self.bytes[(list + 2 * i as u32) as usize..(list + 2 * i as u32) as usize + 2].copy_from_slice(&idx.to_le_bytes());
        }
        self.put32(field, list);
    }
    /// A `char*[NUM_WEAP_ANIMS]` table at `field` with `slots` filled.
    pub fn put_anims(&mut self, field: u32, slots: &[(usize, &str)]) {
        let table = self.alloc(4 * ANIMS as u32);
        for (s, name) in slots {
            self.put_str(table + 4 * *s as u32, name);
        }
        self.put32(field, table);
    }
    pub fn zone(self) -> Zone {
        t6zone::synthetic("fixture", self.bytes, self.strings, Vec::new())
    }
}

/// A variant with its def, an `attachments[30]` and an `attachmentUniques[95]` array.
pub struct Gun {
    pub v: u32,
    pub d: u32,
    atts: u32,
    uniques: u32,
}

impl Gun {
    pub fn new(m: &mut Mem, name: &str) -> Gun {
        let (v, d) = (m.alloc(WV::SIZE), m.alloc(WD::SIZE));
        let (atts, uniques) = (m.alloc(4 * KINDS as u32), m.alloc(4 * UNIQUES));
        m.put_str(v + WV::szInternalName, name);
        m.put32(v + WV::weapDef, d);
        m.put32(v + WV::attachments, atts);
        m.put32(v + WV::attachmentUniques, uniques);
        Gun { v, d, atts, uniques }
    }
    /// `attachments[k]`, a `WeaponAttachment` of kind `k` on `point`.
    pub fn attachment(&self, m: &mut Mem, k: usize, point: u32) -> u32 {
        let a = m.alloc(WA::SIZE);
        m.put_str(a + WA::szInternalName, &format!("att{k}"));
        m.put32(a + WA::attachmentType, k as u32);
        m.put32(a + WA::attachmentPoint, point);
        for o in [WA::fFireTimeScale, WA::fReloadTimeScale, WA::clipSizeScale, WA::fDamageRangeScale] {
            m.putf(a + o, 1.0);
        }
        m.put32(self.atts + 4 * k as u32, a);
        a
    }
    /// `attachmentUniques[i]` of `kind`, combining `mask`, linked to `child`/`sibling`.
    pub fn unique(&self, m: &mut Mem, i: u32, kind: usize, mask: u32, child: i32, sibling: i32) -> u32 {
        let u = m.alloc(WU::SIZE);
        m.put_str(u + WU::szInternalName, &format!("au_{i}"));
        m.put32(u + WU::attachmentType, kind as u32);
        m.put32(u + WU::combinedAttachmentTypeMask, mask);
        m.put32(u + WU::childLink, child as u32);
        m.put32(u + WU::siblingLink, sibling as u32);
        m.put32(self.uniques + 4 * i, u);
        u
    }
    pub fn variant<'z>(&self, z: &'z Zone) -> Variant<'z> {
        Variant { z, at: self.v }
    }
}

/// Set unique `u`'s override bit for anim slot `s` and its anim name.
pub fn override_anim(m: &mut Mem, u: u32, s: usize, name: &str) {
    let word = u + WU::animationOverrides + 4 * (s as u32 >> 5);
    let cur = u32::from_le_bytes(m.bytes[word as usize..word as usize + 4].try_into().unwrap());
    m.put32(word, cur | 1 << (s & 31));
    let table = u32::from_le_bytes(m.bytes[(u + WU::szXAnims) as usize..(u + WU::szXAnims) as usize + 4].try_into().unwrap());
    let table = if table == 0 {
        m.put_anims(u + WU::szXAnims, &[]);
        u32::from_le_bytes(m.bytes[(u + WU::szXAnims) as usize..(u + WU::szXAnims) as usize + 4].try_into().unwrap())
    } else {
        table
    };
    m.put_str(table + 4 * s as u32, name);
}
