//! What a harness was told about UZE itself rather than about one package,
//! taken back by a purge: the marketplaces UZE registered with it.

use std::path::Path;

use uze_core::{Result, home::UzeHome};

use super::marketplace::{MarketplaceDialect, Origin, marketplace_root};

/// Unregisters each marketplace of UZE's that this harness still lists at
/// the root UZE gave it, and answers the names it unregistered. One
/// registered from a root UZE does not own is somebody else's and stays.
pub(crate) fn marketplaces<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
) -> Result<Vec<String>> {
    let mut withdrawn = Vec::new();
    for origin in [Origin::Explicit, Origin::Generated] {
        let root = marketplace_root::<D>(uze_home, origin);
        if D::marketplace_exists(executable, command_home, &root) {
            D::remove_marketplace(executable, command_home, origin.marketplace_name())?;
            withdrawn.push(origin.marketplace_name().to_owned());
        }
    }
    Ok(withdrawn)
}
