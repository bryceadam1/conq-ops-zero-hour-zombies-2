//! BO2's footstep tables (`FootstepTableDef`, asset type 57): per surface type and movement, the
//! sound alias a footfall plays, stored as the alias name's hash.
//!
//! The MP map zones carry `default_1st_person` (the player's own steps), `default_3rd_person`
//! (everyone else's), their `_quiet` variants (Dead Silence, `specialty_quieter`) and
//! `default_3rd_person_loud` (what Awareness, `specialty_loudenemies`, makes of an enemy's steps).
//! The engine's footstep event (`EV_FOOTSTEP_*`, t6mp `FUN_008c16c0`) plays the cell of the
//! surface under the feet; `clientscripts/mp/_footsteps.csc` adds the jump (`fly_step_run_*`) and
//! the landing (`fly_land_*`).

use crate::t6zone::Zone;

/// `FootstepTableDef` (`layouts::FootstepTableDef`): the name, then `sndAliasTable[32][7]`.
pub const ASSET_FOOTSTEP_TABLE: u32 = 57;
/// Surface types per table (`SURF_TYPE_COUNT`).
pub const SURFACES: usize = 32;
/// Movements per surface.
pub const MOVES: usize = 7;

/// BO2's surface types in the tables' row order (`SURF_TYPE_*`, as every MP footstep table's
/// cells name them: row 5's are `fly_step_*_concrete`). The last three rows name no surface the
/// tables' aliases spell out.
pub const SURFACE_NAMES: [&str; SURFACES] = [
    "default", "bark", "brick", "carpet", "cloth", "concrete", "dirt", "flesh", "foliage", "glass", "grass", "gravel", "ice", "metal", "mud", "paper", "plaster",
    "rock", "sand", "snow", "water", "wood", "asphalt", "ceramic", "plastic", "rubber", "cushion", "fruit", "paintedmetal", "", "", "",
];

/// One table: its name and each surface type's alias hashes (0: none).
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub name: String,
    pub aliases: [[u32; MOVES]; SURFACES],
}

/// BO2's sound alias hash (t6mp `FUN_0083c1d0`): seed 0x1505, each lower-cased byte added after
/// multiplying by 0x1003f. The tables and the alias records both carry it.
pub fn alias_hash(name: &str) -> u32 {
    name.bytes().fold(0x1505u32, |h, c| h.wrapping_mul(0x1003f).wrapping_add(c.to_ascii_lowercase() as u32))
}

/// Every footstep table in a loaded zone.
pub fn tables(zone: &Zone) -> Vec<Table> {
    zone.assets
        .iter()
        .filter(|a| a.kind == ASSET_FOOTSTEP_TABLE)
        .map(|a| {
            let mut aliases = [[0u32; MOVES]; SURFACES];
            for (s, row) in aliases.iter_mut().enumerate() {
                for (m, cell) in row.iter_mut().enumerate() {
                    *cell = zone.u32(a.addr + 4 + ((s * MOVES + m) * 4) as u32);
                }
            }
            Table { name: zone.asset_name(a).unwrap_or_default(), aliases }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash is the one `sndalias::Bank::alias` documents (case-blind).
    #[test]
    fn the_hash_ignores_case() {
        assert_eq!(alias_hash("fly_step_run_plr_grass"), alias_hash("FLY_Step_Run_Plr_Grass"));
        assert_ne!(alias_hash("fly_step_run_plr_grass"), alias_hash("fly_step_run_plr_metal"));
        assert_eq!(alias_hash(""), 0x1505);
    }
}
