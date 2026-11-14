//! A map's entities (`MapEnts`, asset type 16): the entity string Radiant compiled into the map's
//! zone (`zm_transit.ff`), `{ "key" "value" ... }` blocks. `MapEnts` begins `name`, `entityString`,
//! `numEntityChars` (T6's layout, the generated loader's).

use crate::t6zone::Zone;
use crate::FormatError;

/// One entity: its key/value pairs in the file's order.
pub type Entity = Vec<(String, String)>;

/// The value of `key` in `e`.
pub fn get<'a>(e: &'a Entity, key: &str) -> Option<&'a str> {
    e.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Parse an entity string.
pub fn parse(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut cur: Option<Entity> = None;
    let mut quoted: Vec<String> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => cur = Some(Vec::new()),
            '}' => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
                quoted.clear();
            }
            '"' => {
                let mut s = String::new();
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                    s.push(d);
                }
                quoted.push(s);
                if quoted.len() == 2 {
                    let v = quoted.pop().unwrap_or_default();
                    let k = quoted.pop().unwrap_or_default();
                    if let Some(e) = cur.as_mut() {
                        e.push((k, v));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The entities of the zone's map: its `MapEnts` asset, or (a map zone, `zm_transit.ff`) the one
/// its collision map points at (`clipMap_t::mapEnts`; the map's `clipmap_pvs` asset).
pub fn entities(z: &Zone) -> Result<Option<Vec<Entity>>, FormatError> {
    use crate::t6zone::layouts as L;
    let kind = |name: &str| crate::fastfile::asset_type_id(name);
    let ents = if let Some(a) = z.assets.iter().find(|a| Some(a.kind) == kind("mapents")) {
        a.addr
    } else if let Some(a) = z.assets.iter().find(|a| Some(a.kind) == kind("clipmap_pvs") || Some(a.kind) == kind("clipmap")) {
        z.try_u32(a.addr + L::clipMap_t::mapEnts)?
    } else {
        return Ok(None);
    };
    if ents == 0 {
        return Ok(None);
    }
    let text = z.try_str_ptr(ents + L::MapEnts::entityString)?;
    Ok(Some(parse(&text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entity_string_parses_into_its_blocks() {
        let e = parse("{\n\"classname\" \"worldspawn\"\n}\n{\n\"targetname\" \"weapon_upgrade\"\n\"zombie_weapon_upgrade\" \"m14_zm\"\n}\n");
        assert_eq!(e.len(), 2);
        assert_eq!(get(&e[1], "zombie_weapon_upgrade"), Some("m14_zm"));
    }

    /// Over the install: TranZit's map zone carries its entities, its wall buys among them.
    #[test]
    #[ignore]
    fn reads_tranzits_entities() {
        let dir = std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR");
        let file = std::fs::read(std::path::Path::new(&dir).join("zone/all/zm_transit.ff")).unwrap();
        let zone = crate::fastfile::read(&file).unwrap();
        let loaded = crate::t6zone::load(&zone).unwrap();
        let ents = entities(&loaded).unwrap().expect("MapEnts");
        let walls = ents.iter().filter(|e| get(e, "targetname") == Some("weapon_upgrade")).count();
        eprintln!("{} entities, {walls} weapon_upgrade", ents.len());
        assert!(walls >= 10);
    }
}
