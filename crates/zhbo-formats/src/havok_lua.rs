//! Black Ops II (T6) compiled UI Lua: Havok Script bytecode, as `RawFile` assets in the player's own
//! `patch_mp.ff`/`patch_ui_mp.ff` (`ui_mp/t6/menus/cacutility.lua`, ...).
//!
//! A small reader, not a decompiler: it parses the chunk (the header, the type table, the nested
//! function records with their instructions and constants) and reports the **constant stores** —
//! every `table.field = <constant>` in instruction order, with the table's path when it is a global
//! path (`CoD.CACUtility.maxAttachments = 2`). That is the data a UI script declares; its logic is
//! read from the decompiled reference and transcribed where a rule is needed.
//!
//! The format follows the T6 dialect documented by CoDLuaDecompiler (MIT, JariKCoding): header
//! `\x1bLua` + version 0x51 + compiler 0x0D + endian + four sizes + flags, a `u32` type count at 14
//! and that many `(u32 type, u32 len, bytes)` names; a function is `u32 upvalues, u32 params,
//! u8 vararg, u32 max stack, u32 instruction count`, padding to 4, the instructions (4 bytes: A, C,
//! B low bits with the C-constant flag, B high bit with the opcode), `u32` constant count, the typed
//! constants, a footer `(u32, f32, u32 children)` and the children.

use crate::t6zone::layouts::RawFile as RF;
use crate::t6zone::Zone;
use crate::FormatError;

/// A constant of a function's pool.
#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    Nil,
    Bool(bool),
    Number(f32),
    Str(String),
    /// A hashed string or `ui64` (types 13 and 11).
    Hash(u64),
}

impl Const {
    /// The value as text (`2`, `0.035`, `weapon_smg`, `true`, `nil`, `#<hash>`).
    pub fn text(&self) -> String {
        match self {
            Const::Nil => "nil".into(),
            Const::Bool(b) => b.to_string(),
            Const::Number(n) if n.fract() == 0.0 && n.abs() < 1e9 => format!("{}", *n as i64),
            Const::Number(n) => format!("{n}"),
            Const::Str(s) => s.clone(),
            Const::Hash(h) => format!("#{h:016x}"),
        }
    }
}

/// One instruction, split into its fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instr {
    pub op: u8,
    pub a: u8,
    pub b: u16,
    pub c: u16,
    /// The C operand names a constant (`RK(C)` is `K(C)`).
    pub c_is_const: bool,
}

impl Instr {
    /// `Bx`: the 17-bit operand of `LOADK`/`GETGLOBAL`/`SETGLOBAL`/`CLOSURE`.
    pub fn bx(&self) -> u32 {
        self.b as u32 * 512 + self.c as u32 + if self.c_is_const { 256 } else { 0 }
    }
}

/// The opcodes the store scan follows (`HavokOpCode`).
pub mod op {
    pub const GETFIELD: u8 = 0;
    pub const GETGLOBAL: u8 = 6;
    pub const MOVE: u8 = 7;
    pub const SETFIELD: u8 = 15;
    pub const SETTABLE_S: u8 = 16;
    pub const SETTABLE_S_BK: u8 = 17;
    pub const LOADK: u8 = 25;
    pub const SETGLOBAL: u8 = 27;
    pub const NEWTABLE: u8 = 52;
    /// A pseudo-instruction carrying operands for the one before it (`GETFIELD_R1`'s cache slots,
    /// a closure's upvalues); it writes no register.
    pub const DATA: u8 = 76;
    pub const GETFIELD_R1: u8 = 73;
    pub const SETFIELD_R1: u8 = 74;
    pub const GETGLOBAL_MEM: u8 = 91;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Function {
    pub params: u32,
    pub instructions: Vec<Instr>,
    pub constants: Vec<Const>,
    pub children: Vec<Function>,
}

/// One constant store: `path.field = value` (`path` empty when the table is not a known global
/// path, e.g. a call's result), in the order the function executes them.
#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    pub path: String,
    pub field: String,
    pub value: Const,
}

struct Cursor<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], FormatError> {
        let s = self.d.get(self.at..self.at + n).ok_or(FormatError::Truncated(what))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self, what: &'static str) -> Result<u8, FormatError> {
        Ok(self.take(1, what)?[0])
    }
    fn u32(&mut self, what: &'static str) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.take(4, what)?.try_into().unwrap()))
    }
    /// A file-controlled count, refused when it cannot fit in the bytes left at `min` each.
    fn count(&mut self, min: usize, what: &'static str) -> Result<usize, FormatError> {
        let n = self.u32(what)? as usize;
        if n.saturating_mul(min) > self.d.len().saturating_sub(self.at) {
            return Err(FormatError::Corrupt(format!("{what}: count {n} exceeds the chunk")));
        }
        Ok(n)
    }
}

fn function(c: &mut Cursor, depth: usize) -> Result<Function, FormatError> {
    if depth > 64 {
        return Err(FormatError::Corrupt("functions nested past 64".into()));
    }
    let _upvalues = c.u32("upvalue count")?;
    let params = c.u32("parameter count")?;
    let _vararg = c.u8("vararg flag")?;
    let _stack = c.u32("max stack")?;
    let n = c.count(4, "instruction count")?;
    c.at += (4 - c.at % 4) % 4;
    let mut instructions = Vec::with_capacity(n);
    for _ in 0..n {
        let b = c.take(4, "instruction")?;
        instructions.push(Instr { op: b[3] >> 1, a: b[0], b: ((b[2] >> 1) as u16) | (((b[3] & 1) as u16) << 7), c: b[1] as u16, c_is_const: b[2] & 1 != 0 });
    }
    let k = c.count(1, "constant count")?;
    let mut constants = Vec::with_capacity(k);
    for _ in 0..k {
        constants.push(match c.u8("constant type")? {
            0 => Const::Nil,
            1 => Const::Bool(c.u8("boolean")? == 1),
            3 => Const::Number(f32::from_le_bytes(c.take(4, "number")?.try_into().unwrap())),
            4 => {
                let len = c.u32("string length")? as usize;
                let s = c.take(len, "string")?;
                Const::Str(String::from_utf8_lossy(&s[..len.saturating_sub(1)]).into_owned())
            }
            11 | 13 => Const::Hash(u64::from_le_bytes(c.take(8, "hash")?.try_into().unwrap())),
            t => return Err(FormatError::Corrupt(format!("unsupported constant type {t}"))),
        });
    }
    let _footer = c.take(8, "function footer")?;
    let kids = c.count(16, "child count")?;
    let mut children = Vec::with_capacity(kids);
    for _ in 0..kids {
        children.push(function(c, depth + 1)?);
    }
    Ok(Function { params, instructions, constants, children })
}

/// Parse a compiled T6 Lua chunk; returns its main function.
pub fn parse(data: &[u8]) -> Result<Function, FormatError> {
    if data.get(..4) != Some(b"\x1bLua".as_slice()) {
        let found = data.get(..4).map_or([0; 4], |m| m.try_into().unwrap());
        return Err(FormatError::BadMagic { expected: "\\x1bLua", found });
    }
    if data.get(4) != Some(&0x51) {
        return Err(FormatError::Corrupt("not Lua 5.1 Havok Script".into()));
    }
    let mut c = Cursor { d: data, at: 14 };
    let types = c.count(8, "type count")?;
    for _ in 0..types {
        let _ty = c.u32("type id")?;
        let len = c.u32("type name length")? as usize;
        c.take(len, "type name")?;
    }
    function(&mut c, 0)
}

impl Function {
    fn k(&self, i: u32) -> Option<&Const> {
        self.constants.get(i as usize)
    }

    fn k_str(&self, i: u32) -> Option<String> {
        match self.k(i) {
            Some(Const::Str(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// The constant stores of this function's own instructions (not its children's), in order.
    /// Registers are followed only through `GETGLOBAL`/`GETFIELD`/`MOVE`/`NEWTABLE`, so a path is
    /// known exactly when the store's table is a global path.
    pub fn stores(&self) -> Vec<Store> {
        let mut regs: Vec<Option<String>> = Vec::new();
        let set = |regs: &mut Vec<Option<String>>, r: u8, v: Option<String>| {
            if regs.len() <= r as usize {
                regs.resize(r as usize + 1, None);
            }
            regs[r as usize] = v;
        };
        let get = |regs: &Vec<Option<String>>, r: u16| regs.get(r as usize).cloned().flatten();
        let mut out = Vec::new();
        for i in &self.instructions {
            match i.op {
                op::GETGLOBAL | op::GETGLOBAL_MEM => {
                    let v = self.k_str(i.bx());
                    set(&mut regs, i.a, v);
                }
                op::GETFIELD | op::GETFIELD_R1 => {
                    let v = get(&regs, i.b).zip(self.k_str(i.c as u32)).map(|(p, f)| format!("{p}.{f}"));
                    set(&mut regs, i.a, v);
                }
                op::MOVE => {
                    let v = get(&regs, i.b);
                    set(&mut regs, i.a, v);
                }
                op::SETFIELD | op::SETFIELD_R1 | op::SETTABLE_S_BK => {
                    if let (Some(field), true) = (self.k_str(i.b as u32), i.c_is_const) {
                        if let Some(value) = self.k(i.c as u32).cloned() {
                            out.push(Store { path: get(&regs, i.a as u16).unwrap_or_default(), field, value });
                        }
                    }
                }
                op::SETGLOBAL => {
                    // `name = R(A)`: the register now aliases the global.
                    let v = self.k_str(i.bx());
                    set(&mut regs, i.a, v);
                }
                op::DATA => {}
                op::NEWTABLE | op::LOADK => set(&mut regs, i.a, None),
                // Any other instruction writing R(A) clears what it held.
                _ => set(&mut regs, i.a, None),
            }
        }
        out
    }

    /// This function and every function nested in it, depth first.
    pub fn all(&self) -> Vec<&Function> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.all());
        }
        out
    }
}

/// A `RawFile` asset's bytes by name (`ui_mp/t6/menus/cacutility.lua`), or `None`.
pub fn rawfile(z: &Zone, name: &str) -> Option<Vec<u8>> {
    let kind = crate::fastfile::asset_type_id("rawfile")?;
    z.assets.iter().filter(|a| a.kind == kind).find(|a| z.str_ptr(a.addr + RF::name) == name).and_then(|a| {
        let len = z.u32(a.addr + RF::len) as usize;
        let buf = z.u32(a.addr + RF::buffer);
        z.try_bytes(buf, len).ok().map(<[u8]>::to_vec)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk with one function: `CoD.CACUtility.maxAttachments = 2`, then two
    /// `<call>.weaponGroup = "<group>"` stores, and one child function.
    fn chunk() -> Vec<u8> {
        let mut d = b"\x1bLua\x51\x0d\x01\x04\x04\x04\x04\x00\x00\x00".to_vec();
        d.extend(1u32.to_le_bytes()); // one type name
        d.extend(4u32.to_le_bytes());
        d.extend(5u32.to_le_bytes());
        d.extend(b"TNIL\0");
        let ins = |op: u8, a: u8, b: u16, c: u16, kc: bool| -> [u8; 4] { [a, c as u8, ((b as u8 & 0x7f) << 1) | kc as u8, (op << 1) | ((b >> 7) as u8 & 1)] };
        let mut f = Vec::new();
        f.extend(0u32.to_le_bytes());
        f.extend(0u32.to_le_bytes());
        f.push(1);
        f.extend(4u32.to_le_bytes());
        let code = [
            ins(op::GETGLOBAL, 0, 0, 0, false),     // r0 = CoD (K0)
            ins(op::GETFIELD, 0, 0, 1, false),      // r0 = r0.CACUtility (K1)
            ins(op::SETFIELD, 0, 2, 3, true),       // r0.maxAttachments (K2) = 2 (K3)
            ins(30, 1, 0, 0, false),                // r1 = <call>
            ins(op::SETFIELD, 1, 4, 5, true),       // r1.weaponGroup (K4) = "weapon_smg" (K5)
            ins(op::SETFIELD, 1, 4, 6, true),       // r1.weaponGroup = "weapon_assault" (K6)
        ];
        f.extend((code.len() as u32).to_le_bytes());
        let mut out = d;
        let pad = (4 - (out.len() + f.len()) % 4) % 4;
        f.extend(std::iter::repeat_n(0, pad));
        for c in code {
            f.extend(c);
        }
        let strs = ["CoD", "CACUtility", "maxAttachments"];
        f.extend(7u32.to_le_bytes());
        for s in strs {
            f.push(4);
            f.extend((s.len() as u32 + 1).to_le_bytes());
            f.extend(s.as_bytes());
            f.push(0);
        }
        f.push(3);
        f.extend(2.0f32.to_le_bytes());
        for s in ["weaponGroup", "weapon_smg", "weapon_assault"] {
            f.push(4);
            f.extend((s.len() as u32 + 1).to_le_bytes());
            f.extend(s.as_bytes());
            f.push(0);
        }
        f.extend(0u32.to_le_bytes());
        f.extend(0f32.to_le_bytes());
        f.extend(1u32.to_le_bytes()); // one child
        // The child: no instructions, one boolean constant, no children.
        f.extend(0u32.to_le_bytes());
        f.extend(1u32.to_le_bytes());
        f.push(0);
        f.extend(2u32.to_le_bytes());
        f.extend(0u32.to_le_bytes());
        let pad = (4 - (out.len() + f.len()) % 4) % 4;
        f.extend(std::iter::repeat_n(0, pad));
        f.extend(1u32.to_le_bytes());
        f.push(1);
        f.push(1);
        f.extend(0u32.to_le_bytes());
        f.extend(0f32.to_le_bytes());
        f.extend(0u32.to_le_bytes());
        out.extend(f);
        out
    }

    #[test]
    fn the_stores_of_a_chunk_come_back_in_order_with_their_paths() {
        let main = parse(&chunk()).unwrap();
        assert_eq!(main.children.len(), 1);
        assert_eq!(main.children[0].constants, vec![Const::Bool(true)]);
        assert_eq!(main.children[0].params, 1);
        let s = main.stores();
        assert_eq!(s[0], Store { path: "CoD.CACUtility".into(), field: "maxAttachments".into(), value: Const::Number(2.0) });
        let groups: Vec<String> = s.iter().filter(|x| x.field == "weaponGroup").map(|x| x.value.text()).collect();
        assert_eq!(groups, ["weapon_smg", "weapon_assault"]);
        assert_eq!(s[1].path, "", "a call's result has no global path");
        assert_eq!(main.all().len(), 2);
    }

    #[test]
    fn damaged_chunks_are_refused() {
        let good = chunk();
        assert!(matches!(parse(b"\x1bLub"), Err(FormatError::BadMagic { .. })));
        for cut in [10, 20, good.len() / 2, good.len() - 1] {
            assert!(parse(&good[..cut]).is_err(), "cut at {cut}");
        }
        let mut huge = good.clone();
        huge[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&huge).is_err());
    }

    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR)"]
    fn reads_the_cac_caps_from_the_install() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("ZHBO_BO2_DIR"));
        let file = std::fs::read(dir.join("zone/all/patch_mp.ff")).unwrap();
        let z = crate::t6zone::load(&crate::fastfile::read(&file).unwrap()).unwrap();
        let bytes = rawfile(&z, "ui_mp/t6/menus/cacutility.lua").expect("cacutility.lua is in patch_mp");
        let main = parse(&bytes).unwrap();
        let caps: Vec<(String, String)> = main.stores().into_iter().filter(|s| s.path == "CoD.CACUtility").map(|s| (s.field, s.value.text())).collect();
        assert!(caps.contains(&("maxAttachments".into(), "2".into())), "{caps:?}");
        assert!(caps.contains(&("maxBonusCards".into(), "3".into())), "{caps:?}");
    }
}
