//! The expected side: BO2's rules over the player's install.

pub mod cac;
pub mod combos;
pub mod install;
pub mod raw;
pub mod resolve;
pub mod rows;

#[cfg(test)]
mod fixture;

/// Every expected weapon row for the install at `dir`.
pub fn weapon_rows(dir: &std::path::Path) -> Result<Vec<crate::schema::Row>, String> {
    let install = install::Install::open(dir)?;
    rows::Expected::new(&install).rows(&crate::weapons::schema())
}

/// Both expected tables for the install at `dir`: the weapon rows and the create-a-class rows.
pub fn expected(dir: &std::path::Path) -> Result<(Vec<crate::schema::Row>, Vec<crate::schema::Row>), String> {
    let install = install::Install::open(dir)?;
    let weapons = rows::Expected::new(&install).rows(&crate::weapons::schema())?;
    let cac = cac::rows(&install, &crate::cac::schema())?;
    Ok((weapons, cac))
}
