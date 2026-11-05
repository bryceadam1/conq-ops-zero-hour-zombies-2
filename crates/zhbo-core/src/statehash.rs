//! The lockstep fingerprint's streaming hash: [`StateHash`] feeds a value's deterministic state
//! into a [`Hasher`], field by field in declaration order, so two machines holding the same sim
//! state get the same 64 bits (`Sim::state_hash`, `sim/lockstep.rs`, and through it Zero Hour's
//! own logic CRC, `zhbo_logic_crc`).
//!
//! Rules the impls follow:
//! * **Every field, by name.** A struct's impl destructures it without `..` ([`state_hash!`]), so a
//!   field added later does not compile until it is listed: hashed, or skipped with the reason
//!   (presentation, per-step scratch, or data the content CRC already covers).
//! * **Values, never memory.** Floats go in as their bits (two states agree only when every float
//!   is bit-identical: `-0.0` and `0.0` differ), integers widened to 64 bits, strings as length and
//!   bytes, an `Option` as a tag then the value, a `Vec` as its length then its items. Nothing reads
//!   an address, a `HashMap` order or the platform's layout, so the hash is the same on every
//!   machine and every build of the same source.
//! * **Shared definitions by identity.** A `&'static WeaponDef` is its id (the installed table is
//!   the same on every machine, and the content CRC proves it); the definition's numbers are not
//!   streamed.
//!
//! The mixing step is a bijection of the running state for a fixed input word (xor, an odd
//! multiply, a rotate), and so is the finish, so changing any single word of the stream always
//! changes the hash. It costs a few nanoseconds a word: a busy 16-seat sim is a few microseconds,
//! cheap enough for every CRC frame (Zero Hour's network CRC interval can be 1).
//!
//! Test builds also get [`StateHash::perturb`], which changes one leaf of the value (the `n`-th
//! in hash order), so a test can prove every leaf reaches the hash (`sim/lockstep_tests.rs`).

/// The running 64-bit state.
#[derive(Clone, Copy, Debug)]
pub struct Hasher(u64);

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub const fn new() -> Self {
        Hasher(0xcbf2_9ce4_8422_2325)
    }

    /// One 64-bit word into the state.
    #[inline]
    pub fn word(&mut self, w: u64) {
        self.0 = (self.0 ^ w).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(27);
    }

    /// A byte string: its length, then its bytes eight at a time (little-endian, zero-padded).
    pub fn bytes(&mut self, b: &[u8]) {
        self.word(b.len() as u64);
        let (words, rest) = b.as_chunks::<8>();
        for w in words {
            self.word(u64::from_le_bytes(*w));
        }
        if !rest.is_empty() {
            let mut w = [0u8; 8];
            w[..rest.len()].copy_from_slice(rest);
            self.word(u64::from_le_bytes(w));
        }
    }

    /// The hash: the state through MurmurHash3's 64-bit finaliser (a bijection that spreads every
    /// bit over the whole word, so the two 32-bit halves Zero Hour's CRC takes are both well mixed).
    pub fn finish(&self) -> u64 {
        let mut h = self.0;
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        h ^ (h >> 33)
    }
}

/// A variant's tag: its name's FNV-1a, so a reordered or inserted variant keeps every other one's.
pub const fn tag(name: &str) -> u64 {
    let b = name.as_bytes();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < b.len() {
        h ^= b[i] as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
        i += 1;
    }
    h
}

/// Deterministic state that goes into the lockstep fingerprint (see the module doc).
pub trait StateHash {
    fn hash(&self, h: &mut Hasher);

    /// Change the `n`-th leaf of this value (hash order), or count `n` down by this value's leaves
    /// and return `false` when it has fewer. A leaf is one hashed scalar, string or definition.
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool;
}

/// The hash of one value on its own.
pub fn hash_of<T: StateHash + ?Sized>(v: &T) -> u64 {
    let mut h = Hasher::new();
    v.hash(&mut h);
    h.finish()
}

/// Counts a leaf: `true` when it is the one to change.
#[cfg(test)]
pub(crate) fn take(n: &mut usize) -> bool {
    if *n == 0 {
        return true;
    }
    *n -= 1;
    false
}

macro_rules! ints {
    ($($t:ty),*) => {$(
        impl StateHash for $t {
            #[inline]
            fn hash(&self, h: &mut Hasher) {
                h.word(*self as u64);
            }
            #[cfg(test)]
            fn perturb(&mut self, n: &mut usize) -> bool {
                take(n) && { *self = self.wrapping_add(1); true }
            }
        }
    )*};
}
ints!(u8, u16, u32, u64, usize, i8, i16, i32, i64);

impl StateHash for bool {
    #[inline]
    fn hash(&self, h: &mut Hasher) {
        h.word(*self as u64);
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        take(n) && {
            *self = !*self;
            true
        }
    }
}

impl StateHash for f32 {
    #[inline]
    fn hash(&self, h: &mut Hasher) {
        h.word(self.to_bits() as u64);
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        take(n) && {
            *self = f32::from_bits(self.to_bits() ^ 1);
            true
        }
    }
}

impl StateHash for String {
    fn hash(&self, h: &mut Hasher) {
        h.bytes(self.as_bytes());
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        take(n) && {
            self.push('~');
            true
        }
    }
}

/// A static name (a sound alias, a shock's file): its text.
impl StateHash for &'static str {
    fn hash(&self, h: &mut Hasher) {
        h.bytes(self.as_bytes());
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        take(n) && {
            *self = if *self == "~" { "~~" } else { "~" };
            true
        }
    }
}

impl<T: StateHash> StateHash for Option<T> {
    #[inline]
    fn hash(&self, h: &mut Hasher) {
        match self {
            None => h.word(0),
            Some(v) => {
                h.word(1);
                v.hash(h);
            }
        }
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        self.as_mut().is_some_and(|v| v.perturb(n))
    }
}

impl<T: StateHash> StateHash for [T] {
    fn hash(&self, h: &mut Hasher) {
        h.word(self.len() as u64);
        for v in self {
            v.hash(h);
        }
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        self.iter_mut().any(|v| v.perturb(n))
    }
}

impl<T: StateHash> StateHash for Vec<T> {
    fn hash(&self, h: &mut Hasher) {
        self.as_slice().hash(h);
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        self.as_mut_slice().perturb(n)
    }
}

impl<T: StateHash> StateHash for std::collections::VecDeque<T> {
    fn hash(&self, h: &mut Hasher) {
        h.word(self.len() as u64);
        for v in self {
            v.hash(h);
        }
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        self.iter_mut().any(|v| v.perturb(n))
    }
}

impl<T: StateHash, const N: usize> StateHash for [T; N] {
    fn hash(&self, h: &mut Hasher) {
        for v in self {
            v.hash(h);
        }
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        self.iter_mut().any(|v| v.perturb(n))
    }
}

macro_rules! tuples {
    ($(($($v:ident),+)),*) => {$(
        #[allow(non_snake_case)]
        impl<$($v: StateHash),+> StateHash for ($($v,)+) {
            fn hash(&self, h: &mut Hasher) {
                let ($($v,)+) = self;
                $($v.hash(h);)+
            }
            #[cfg(test)]
            fn perturb(&mut self, n: &mut usize) -> bool {
                let ($($v,)+) = self;
                $(if $v.perturb(n) { return true; })+
                false
            }
        }
    )*};
}
tuples!((A, B), (A, B, C), (A, B, C, D));

impl<T: StateHash + ?Sized> StateHash for std::cell::RefCell<T> {
    fn hash(&self, h: &mut Hasher) {
        self.borrow().hash(h);
    }
    #[cfg(test)]
    fn perturb(&mut self, n: &mut usize) -> bool {
        self.get_mut().perturb(n)
    }
}

/// `StateHash` for a struct or an enum, naming every field. (The macro is the crate's own, not
/// exported, so the example is text, not a doctest: `cargo test -- --ignored` would run an
/// `ignore` block and fail to find it.)
///
/// ```text
/// state_hash!(struct Body { pos, vel, on_ground } skip { scratch });
/// state_hash!(enum Mode { Fps, FieldComputer { t }, Rts });
/// ```
///
/// A struct is destructured without `..`: each field is either hashed (in the order listed) or in
/// `skip` (say why beside the call). An enum hashes its variant's [`tag`] and then the variant's
/// fields; its perturbation changes a field of the current variant, or moves a field-less variant
/// to another field-less one.
macro_rules! state_hash {
    (struct $t:ident { $($f:ident),* $(,)? } $(skip { $($s:ident),* $(,)? })?) => {
        impl $crate::statehash::StateHash for $t {
            fn hash(&self, h: &mut $crate::statehash::Hasher) {
                let $t { $($f,)* $($($s: _,)*)? } = self;
                $($crate::statehash::StateHash::hash($f, h);)*
            }
            #[cfg(test)]
            fn perturb(&mut self, n: &mut usize) -> bool {
                let $t { $($f,)* $($($s: _,)*)? } = self;
                $(if $crate::statehash::StateHash::perturb($f, n) { return true; })*
                false
            }
        }
    };
    (enum $t:ident { $($v:ident $({ $($f:ident),* $(,)? })?),* $(,)? }) => {
        impl $crate::statehash::StateHash for $t {
            fn hash(&self, h: &mut $crate::statehash::Hasher) {
                match self {
                    $($t::$v $({ $($f),* })? => {
                        h.word(const { $crate::statehash::tag(stringify!($v)) });
                        $($($crate::statehash::StateHash::hash($f, h);)*)?
                    })*
                }
            }
            #[cfg(test)]
            fn perturb(&mut self, n: &mut usize) -> bool {
                let tag_of = |v: &$t| match v {
                    $($t::$v { .. } => const { $crate::statehash::tag(stringify!($v)) },)*
                };
                let here = tag_of(self);
                match self {
                    $($t::$v $({ $($f),* })? => {
                        $($(if $crate::statehash::StateHash::perturb($f, n) { return true; })*)?
                    })*
                }
                // A field-less variant is a leaf of its own when there is another to move to.
                let mut units: Vec<$t> = [$($crate::statehash::state_hash!(@unit $t::$v $({ $($f),* })?)),*].into_iter().flatten().collect();
                if units.len() > 1 && units.iter().any(|u| tag_of(u) == here) && $crate::statehash::take(n) {
                    let i = units.iter().position(|u| tag_of(u) != here).unwrap_or(0);
                    *self = units.swap_remove(i);
                    return true;
                }
                false
            }
        }
    };
    (@unit $t:ident :: $v:ident) => { Some($t::$v) };
    (@unit $t:ident :: $v:ident { $($f:ident),* }) => { None };
}
pub(crate) use state_hash;

#[cfg(test)]
mod tests {
    use super::*;

    /// The stream's hash is pinned: the same values hash the same on every platform and build (no
    /// address, layout or endianness enters it). A change here changes every CRC.
    #[test]
    fn the_hash_of_a_fixed_stream_is_pinned() {
        let v = ((7u32, -2i32, 1.5f32, true), String::from("an94_mp+reflex"), Some(3u16), vec![0.25f32, -0.0]);
        assert_eq!(hash_of(&v), 0xed72_4bca_e3ca_7777, "{:#018x}", hash_of(&v));
    }

    /// Values that print or pack alike still hash apart.
    #[test]
    fn neighbouring_values_hash_apart() {
        assert_ne!(hash_of(&0.0f32), hash_of(&-0.0f32), "a float by its bits");
        assert_ne!(hash_of(&None::<u32>), hash_of(&Some(0u32)), "the option's tag");
        assert_ne!(hash_of(&Vec::<u32>::new()), hash_of(&vec![0u32]), "the length");
        assert_ne!(hash_of(&String::from("ab")), hash_of(&String::from("ab\0")), "a string's length");
        assert_ne!(hash_of(&(1u32, 2u32)), hash_of(&(2u32, 1u32)), "the order");
        assert_ne!(hash_of(&(vec![1u32], vec![2u32])), hash_of(&(vec![1u32, 2], Vec::<u32>::new())), "where one list ends");
        assert_ne!(tag("Fps"), tag("Rts"));
    }

    /// A long string streams in whole words; every byte of it, the last partial word's too, counts.
    #[test]
    fn every_byte_of_a_string_counts() {
        let s = "frag_grenade_mp+extclip";
        for i in 0..s.len() {
            let mut b = s.as_bytes().to_vec();
            b[i] ^= 1;
            let mut h = Hasher::new();
            h.bytes(&b);
            assert_ne!(h.finish(), hash_of(&s.to_string()), "byte {i}");
        }
    }
}
