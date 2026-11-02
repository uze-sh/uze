//! What a harness was told about UZE itself rather than about one package,
//! taken back by a purge: the marketplaces UZE registered with it.

use std::path::Path;

use uze_core::{Result, home::UzeHome};

use super::marketplace::{MARKETPLACE, MarketplaceDialect, RETIRED_MARKETPLACE};

/// Unregisters each marketplace of UZE's that this harness still lists at a
/// root inside the UZE home — its vendor store, or the root an earlier
/// release registered — and answers the names it unregistered. One
/// registered from a root UZE does not own is somebody else's and stays.
pub(crate) fn marketplaces<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
) -> Result<Vec<String>> {
    let mut withdrawn = Vec::new();
    for name in [MARKETPLACE, RETIRED_MARKETPLACE] {
        let owned = D::registered_root(executable, command_home, name)
            .is_some_and(|root| root.starts_with(uze_home.root()));
        if owned {
            D::remove_marketplace(executable, command_home, name)?;
            withdrawn.push(name.to_owned());
        }
    }
    Ok(withdrawn)
}
