//! The expected side: BO2's rules over the player's install.

pub mod combos;
pub mod install;
pub mod raw;
pub mod resolve;
pub mod rows;

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod probe;

/// Every expected weapon row for the install at `dir`.
pub fn weapon_rows(dir: &std::path::Path) -> Result<Vec<crate::schema::Row>, String> {
    let install = install::Install::open(dir)?;
    rows::Expected::new(&install).rows(&crate::weapons::schema())
}
