//! The custom-class pick payload: a [`CustomClass`] packed as catalog tokens, so every class the
//! editor can build fits the ABI's fixed choose-class payload ([`PAYLOAD_WORDS`] words).
//!
//! The profile file keeps the readable `name\nspec` text ([`CustomClass::to_wire`]); the pick does
//! not carry it. That text grows with every key a class sets — each attachment, the three camos,
//! the reticle and its colour, the offhand counts — and the name, and nothing bounds it under the
//! payload: a personalised class with an attachment on each gun (`tar21_mp+rangefinder`,
//! `crossbow_mp+ir`, gold camos) is 258 bytes, two over the 256 the payload holds, and the pick was
//! refused. BO2 itself stores a class as item numbers, not names (its `cacloadouts` stats: the
//! statstable item of each weapon slot, the weapon's attachment numbers, the camo, reticle and
//! perk indices); this codec does the same over the installed catalog, which every client holds
//! identically (the sim already resolves every weapon id against it):
//!
//! * **weapons** (primary, secondary, lethal, tactical, melee): the item's place in the catalog
//!   ([`crate::weapons::Catalog::all`]), then its attachments as the item's attachment numbers
//!   (its `allowed` order, BO2's statstable column 8). A dual-wield pistol (`fnp45_dw_mp`) is its
//!   item with `dw` ([`crate::classmenu::items::given_parts`]), rebuilt by `given_weapon`.
//! * **perks**: indices into [`crate::weapons::perks::PERKS`].
//! * **camos** (primary, knife, secondary), **reticles** (primary, secondary), **reticle colour**:
//!   indices into the catalog's camo, reticle and reticle-colour tables, BO2's own option order.
//! * the **name** as its UTF-8 bytes, the counts and reserves as numbers.
//!
//! Anything a vocabulary does not hold (a hand-edited profile, a value the catalog lacks) is
//! carried as a literal string, so every class round-trips exactly; only such a class can grow
//! past the payload, and the client then refuses the pick rather than truncate it. For a class
//! built from the catalog's own items, the size is bounded by construction ([`max_legal_bytes`]).
//!
//! The decoder also reads the old text form (a payload not starting with [`TAG`]), which the
//! profile-save entry point (`zhbo_profile_save`) still receives.

use crate::classes::CustomClass;
use crate::classmenu::items::{given_parts, given_weapon};
use crate::weapons::{self, perks::PERKS};

/// The choose-class payload's capacity in words (`ClientOut::choose_class_payload`,
/// `ClassButton::payload` and the bridge's receive buffer).
pub const PAYLOAD_WORDS: usize = 64;

/// The first byte of a token payload. The text form starts with the class name (typed letters,
/// digits and spaces) or the newline of an unnamed class, never a control byte.
pub const TAG: u8 = 0x01;

/// A field's code: no value (the slot inherits the base kit).
const EMPTY: u16 = 0;
/// A field's code: a literal string follows (`u8` length, then the bytes).
const LITERAL: u16 = u16::MAX;

/// The most attachments a weapon of a legal class carries: the editor's caps with the Gunfighter
/// cards ([`crate::classmenu::offer`]; BO2: three on a primary, two on a secondary).
pub fn max_legal_attachments() -> usize {
    let o = crate::classmenu::offer();
    o.primary_caps.1.max(o.secondary_caps.1)
}

/// The most perks a legal class carries (`maxSpecialtySlots`: three tiers, two each with Greed).
pub fn max_legal_perks() -> usize {
    crate::classes::max_specialty_slots()
}

/// The largest token payload a class built from the catalog's own items can have, in bytes: the
/// header, the name at [`crate::classmenu::NAME_MAX`] characters of up to four UTF-8 bytes, five
/// weapon slots with the most attachments, the counts, the most perks, the six option fields and
/// the two reserves. Every field is fixed-size except the name and the attachment and perk counts,
/// so this is an upper bound for every legal kit.
pub fn max_legal_bytes() -> usize {
    let header = 1 + 2;
    let name = 1 + crate::classmenu::NAME_MAX * 4;
    let gun = 2 + 1 + max_legal_attachments();
    let weapons = 5 * gun;
    let counts = 2;
    let perks = 1 + 2 * max_legal_perks();
    let options = 6 * 2;
    let reserves = 2 * 2;
    header + name + weapons + counts + perks + options + reserves
}

/// Pack a class into the pick payload's words; `None` if a literal is longer than a field holds
/// (255 bytes) — no class the editor builds has one.
pub fn encode(c: &CustomClass) -> Option<Vec<u32>> {
    let mut body = Vec::with_capacity(96);
    literal(&mut body, &c.name)?;
    for id in [&c.primary, &c.secondary, &c.lethal, &c.tactical, &c.melee] {
        weapon(&mut body, id)?;
    }
    body.push(c.lethal_count);
    body.push(c.tactical_count);
    body.push(u8::try_from(c.perks.len()).ok()?);
    for p in &c.perks {
        coded(&mut body, PERKS.iter().position(|d| d.id == p), p)?;
    }
    let cat = weapons::catalog();
    coded(&mut body, cat.camo_index(&c.camo), &c.camo)?;
    coded(&mut body, cat.camo_index(&c.knife), &c.knife)?;
    coded(&mut body, cat.camo_index(&c.secondary_camo), &c.secondary_camo)?;
    coded(&mut body, cat.reticles().iter().position(|r| r.id == c.reticle), &c.reticle)?;
    coded(&mut body, cat.reticle_colors().iter().position(|r| r.id == c.reticle_color), &c.reticle_color)?;
    coded(&mut body, cat.reticles().iter().position(|r| r.id == c.secondary_reticle), &c.secondary_reticle)?;
    body.extend_from_slice(&c.reserve[0].to_le_bytes());
    body.extend_from_slice(&c.reserve[1].to_le_bytes());

    let mut bytes = Vec::with_capacity(body.len() + 3);
    bytes.push(TAG);
    bytes.extend_from_slice(&u16::try_from(body.len()).ok()?.to_le_bytes());
    bytes.extend_from_slice(&body);
    Some(pack(&bytes))
}

/// Undo [`encode`]; `None` for a malformed or truncated payload. A payload without [`TAG`] is the
/// older text form ([`CustomClass::to_wire`] packed four bytes a word).
pub fn decode(words: &[u32]) -> Option<CustomClass> {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    if bytes.first() != Some(&TAG) {
        return decode_text(&bytes);
    }
    let len = u16::from_le_bytes([*bytes.get(1)?, *bytes.get(2)?]) as usize;
    let mut r = Reader { bytes: bytes.get(3..3 + len)?, at: 0 };
    let mut c = CustomClass { name: r.literal()?, ..CustomClass::default() };
    c.primary = r.weapon()?;
    c.secondary = r.weapon()?;
    c.lethal = r.weapon()?;
    c.tactical = r.weapon()?;
    c.melee = r.weapon()?;
    c.lethal_count = r.u8()?;
    c.tactical_count = r.u8()?;
    let perks = r.u8()?;
    for _ in 0..perks {
        c.perks.push(r.coded(|i| PERKS.get(i).map(|d| d.id.to_string()))?);
    }
    let cat = weapons::catalog();
    let camo = |i: usize| cat.camos().get(i).map(|o| o.id.clone());
    c.camo = r.coded(camo)?;
    c.knife = r.coded(camo)?;
    c.secondary_camo = r.coded(camo)?;
    c.reticle = r.coded(|i| cat.reticles().get(i).map(|o| o.id.clone()))?;
    c.reticle_color = r.coded(|i| cat.reticle_colors().get(i).map(|o| o.id.clone()))?;
    c.secondary_reticle = r.coded(|i| cat.reticles().get(i).map(|o| o.id.clone()))?;
    c.reserve = [r.u16()?, r.u16()?];
    // Every byte the header counted must be a field: a longer body is not this format.
    (r.at == r.bytes.len()).then_some(c)
}

/// The text form, for payloads written before the tokens: UTF-8 up to the first NUL (the words
/// carry no length, so the last word's padding is NUL).
fn decode_text(bytes: &[u8]) -> Option<CustomClass> {
    let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    CustomClass::from_wire(std::str::from_utf8(&bytes[..len]).ok()?)
}

/// Bytes packed little-endian four per word, the last one NUL-padded.
pub(crate) fn pack(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|chunk| chunk.iter().enumerate().fold(0u32, |w, (i, b)| w | (*b as u32) << (8 * i)))
        .collect()
}

/// A literal string: its length (at most 255 bytes) and its bytes.
fn literal(out: &mut Vec<u8>, s: &str) -> Option<()> {
    out.push(u8::try_from(s.len()).ok()?);
    out.extend_from_slice(s.as_bytes());
    Some(())
}

/// A vocabulary field: [`EMPTY`], the index plus one, or [`LITERAL`] and the string.
fn coded(out: &mut Vec<u8>, index: Option<usize>, s: &str) -> Option<()> {
    if s.is_empty() {
        out.extend_from_slice(&EMPTY.to_le_bytes());
        return Some(());
    }
    match index.and_then(|i| u16::try_from(i + 1).ok()).filter(|c| *c != LITERAL) {
        Some(code) => out.extend_from_slice(&code.to_le_bytes()),
        None => {
            out.extend_from_slice(&LITERAL.to_le_bytes());
            literal(out, s)?;
        }
    }
    Some(())
}

/// A weapon slot: the item's catalog code and its attachment numbers, or the whole id as a literal
/// when the item is not in the catalog, an attachment is not one it offers, or the id is not the
/// spelling the item and attachments rebuild to (so the round trip is always exact).
fn weapon(out: &mut Vec<u8>, id: &str) -> Option<()> {
    let tokens = (!id.is_empty()).then(|| weapon_tokens(id)).flatten();
    match tokens {
        Some((code, atts)) => {
            out.extend_from_slice(&code.to_le_bytes());
            out.push(u8::try_from(atts.len()).ok()?);
            out.extend_from_slice(&atts);
        }
        None => coded(out, None, id)?,
    }
    Some(())
}

/// `id` as its item's code and attachment numbers, if it rebuilds to exactly `id`.
fn weapon_tokens(id: &str) -> Option<(u16, Vec<u8>)> {
    let cat = weapons::catalog();
    let (item, shorts) = given_parts(id);
    let def = cat.get(&item)?;
    let index = cat.all().iter().position(|d| std::ptr::eq(*d, def))?;
    let code = u16::try_from(index + 1).ok().filter(|c| *c != LITERAL)?;
    let atts: Vec<u8> = shorts
        .iter()
        .map(|s| def.allowed.iter().position(|(a, _)| a == s).and_then(|i| u8::try_from(i).ok()))
        .collect::<Option<_>>()?;
    let refs: Vec<&str> = shorts.iter().map(String::as_str).collect();
    (given_weapon(&def.id, &refs) == id).then_some((code, atts))
}

/// A cursor over a token body.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(b)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }

    fn literal(&mut self) -> Option<String> {
        let len = self.u8()? as usize;
        let s = self.bytes.get(self.at..self.at + len)?;
        self.at += len;
        String::from_utf8(s.to_vec()).ok()
    }

    /// A vocabulary field; `None` (malformed) for an index past the vocabulary.
    fn coded(&mut self, lookup: impl Fn(usize) -> Option<String>) -> Option<String> {
        match self.u16()? {
            EMPTY => Some(String::new()),
            LITERAL => self.literal(),
            code => lookup(code as usize - 1),
        }
    }

    fn weapon(&mut self) -> Option<String> {
        match self.u16()? {
            EMPTY => Some(String::new()),
            LITERAL => self.literal(),
            code => {
                let def = *weapons::catalog().all().get(code as usize - 1)?;
                let n = self.u8()?;
                let mut shorts = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    shorts.push(def.allowed.get(self.u8()? as usize)?.0.as_str());
                }
                Some(given_weapon(&def.id, &shorts))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The playtest class that could not be picked (the player's own profile, round 2): an
    /// attachment on each gun and all three camos made its text 258 bytes, two over the payload.
    fn playtest_class() -> CustomClass {
        let mut c = CustomClass::from_spec(
            "primary=tar21_mp+rangefinder;secondary=crossbow_mp+ir;lethal=frag_grenade_mp;tactical=willy_pete_mp;\
             lethal_count=1;tactical_count=1;melee=knife_held_mp;perks=Lightweight,Toughness,Dexterity;camo=camo_gold;\
             knife=camo_gold;secondary_camo=camo_artofwar",
        );
        c.name = "Custom 2".into();
        c
    }

    #[test]
    fn the_largest_legal_class_fits_the_payload_by_construction() {
        assert!(max_legal_bytes() <= PAYLOAD_WORDS * 4, "{} bytes > the {}-word payload", max_legal_bytes(), PAYLOAD_WORDS);
    }

    #[test]
    fn the_playtest_class_fits_and_round_trips() {
        let c = playtest_class();
        assert!(c.to_wire().len() > PAYLOAD_WORDS * 4, "the seed is the one the text form could not send");
        let words = c.to_wire_words();
        assert!(words.len() <= PAYLOAD_WORDS, "{} words", words.len());
        assert_eq!(CustomClass::from_wire_words(&words), Some(c));
    }

    /// Every value the editor can give every field, with the largest of every other field
    /// (`classmenu::kit_coverage`): each fits the payload, comes back exactly, and its kit is the
    /// class's. The real-install run of the same sweep is zhbo-assets'
    /// `every_legal_class_fits_the_pick_payload`.
    #[test]
    fn every_legal_class_fits_the_payload_and_round_trips() {
        let mut n = 0;
        let mut largest = 0;
        crate::classmenu::kit_coverage(|c| {
            n += 1;
            let words = c.to_wire_words();
            largest = largest.max(words.len());
            assert!(words.len() <= PAYLOAD_WORDS, "{} words: {c:?}", words.len());
            let back = CustomClass::from_wire_words(&words).unwrap_or_else(|| panic!("{c:?} does not decode"));
            assert_eq!(&back, c);
            assert_eq!(back.loadout(), c.loadout());
        });
        println!("swept {n} classes, largest {largest} words, bound {} bytes", max_legal_bytes());
        assert!(n > 50, "the sweep visited {n} classes");
        assert!(largest <= PAYLOAD_WORDS);
    }

    #[test]
    fn catalog_items_are_tokens_and_unknown_values_are_literals() {
        let c = CustomClass { name: "A".into(), primary: "an94_mp+gl+reflex".into(), secondary: "fnp45_mp".into(), ..CustomClass::default() };
        let tokens = c.to_wire_words().len();
        // An item the catalog lacks, an attachment the item does not offer, an unsorted spelling
        // and a camo outside the table all survive verbatim.
        for odd in [
            CustomClass { primary: "nosuch_mp".into(), ..c.clone() },
            CustomClass { primary: "an94_mp+nosuch".into(), ..c.clone() },
            CustomClass { primary: "an94_mp+reflex+gl".into(), ..c.clone() },
            CustomClass { camo: "Gold".into(), perks: vec!["NoSuchPerk".into()], ..c.clone() },
        ] {
            let words = odd.to_wire_words();
            assert_eq!(CustomClass::from_wire_words(&words), Some(odd.clone()));
            assert!(words.len() > tokens, "{odd:?} rides as a literal");
        }
        assert_eq!(CustomClass::from_wire_words(&c.to_wire_words()), Some(c));
    }

    #[test]
    fn a_dual_wield_pistol_is_its_item_with_dw() {
        let ids: Vec<&str> = weapons::catalog().all().iter().map(|d| d.id.as_str()).filter(|id| id.contains("_dw")).collect();
        for id in ids {
            let (item, shorts) = given_parts(id);
            if shorts.is_empty() {
                continue;
            }
            let c = CustomClass { secondary: id.into(), ..CustomClass::default() };
            let words = encode(&c).unwrap();
            assert_eq!(decode(&words), Some(c), "{id} = {item} + {shorts:?}");
        }
    }

    #[test]
    fn a_malformed_payload_is_refused_and_the_text_form_still_reads() {
        let c = playtest_class();
        let words = c.to_wire_words();
        assert_eq!(decode(&words[..words.len() - 1]), None, "truncated");
        assert_eq!(decode(&[]), None);
        // An item index past the catalog.
        let mut bytes = vec![TAG, 0, 0, 0];
        bytes.extend_from_slice(&(weapons::catalog().all().len() as u16 + 1).to_le_bytes());
        bytes.push(0);
        bytes[1] = (bytes.len() - 3) as u8;
        assert_eq!(decode(&pack(&bytes)), None);
        // The text form (`to_wire` packed), which older payloads carry.
        let small = CustomClass { name: "Old".into(), primary: "an94_mp".into(), ..CustomClass::default() };
        assert_eq!(decode(&pack(small.to_wire().as_bytes())), Some(small));
    }
}
