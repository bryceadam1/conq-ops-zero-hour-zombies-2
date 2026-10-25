//! The create-a-class table's columns: one row per thing the class editor offers or decides — a
//! weapon, an attachment on a weapon, a perk, a wildcard, a camo/reticle/reticle-colour option, a
//! class cap, a weapon category's place in the menu, and the weapon a pistol with the dual-wield
//! attachment becomes.
//!
//! Key: `kind` (`weapon`, `attachment`, `perk`, `wildcard`, `camo`, `reticle`, `reticle_color`,
//! `constant`, `category`, `dualwield`) and `item` (the BO2 reference: `an94`, `an94+reflex`,
//! `PERKS_LIGHTWEIGHT`'s `specialty_movefaster|...` row is `lightweight`, a cap's Lua field name, a
//! category's position). A column a kind has no use for is empty on both sides.

use crate::schema::{Kind, Schema};

pub fn schema() -> Schema {
    let mut s = Schema::new("cac", ["kind", "item"]);
    s.col("name", "names", Kind::Text, "the localized name the menu shows (statstable/attachmenttable column 3)")
        .col("description", "names", Kind::Text, "the localized description (column 7)")
        .col("image", "art", Kind::Text, "the material the menu draws (column 6)")
        .col("group", "menu", Kind::Text, "the item's group: a weapon's CaC group, an attachment's attach point, a perk's tier slot")
        .col("slot", "menu", Kind::Text, "the loadout slot (statstable column 13)")
        .col("cost", "allocation", Kind::Int, "the allocation points the item costs (statstable column 12)")
        .col("order", "menu", Kind::Int, "the item's place in its list (G17: column 9 ascending, then the item index; an attachment by the weapon's column-8 order)")
        .col("members", "rules", Kind::Set, "a perk's specialties, an attachment's compatible attachments (G15)")
        .col("value", "rules", Kind::Text, "a cap's value, a category's group, the weapon a dual-wield class gets (G16)");
    s
}
