//! What the application does only because an earlier release left it,
//! and the door to the vocabulary that declares it (the `earlier-releases`
//! rule).

pub use uze_core::document::earlier::{Earlier, FLOOR, Lasts, Release};

use super::{BUILT_IN_MARKETPLACE, UzeApplication, bootstrap};
use crate::Result;
use uze_core::trust;

/// The official marketplace's name before it was `uze`: what an earlier
/// release recorded its plugins under.
const RETIRED_OFFICIAL_NAME: &str = "uze-official";

/// This crate's entries.
pub fn earlier() -> Vec<Earlier> {
    vec![Earlier {
        name: "the official marketplace's earlier name",
        // Moved off on every bootstrap that finds it (#328, first released
        // in beta.21): bounded by the last release that installed under it.
        lasts: Lasts::Until(Release::beta(1, 0, 0, 20)),
        spelled: &[RETIRED_OFFICIAL_NAME],
        site: file!(),
    }]
}

/// Whether `id` is a plugin installed under the official marketplace's
/// earlier name.
pub(super) fn under_the_retired_official_name(id: &str) -> bool {
    id.ends_with(&format!("@{RETIRED_OFFICIAL_NAME}"))
}

impl UzeApplication {
    /// Every plugin an earlier release installed under the official
    /// marketplace's old name, taken off as a removal takes it (its receipts
    /// say what it delivered) and installed again under `uze`; the bundled
    /// ones are seeded again right after. Answers whether it installed any.
    pub(super) fn move_off_the_retired_official_name(&self) -> Result<bool> {
        let retired = format!("@{RETIRED_OFFICIAL_NAME}");
        let names: Vec<String> = self
            .store
            .package_ids()?
            .iter()
            .filter_map(|id| id.as_str().strip_suffix(&retired).map(str::to_owned))
            .collect();
        for name in &names {
            let id = format!("{name}{retired}");
            if let Err(error) = self.plugins().detach_and_remove_unpublished(&id, true) {
                tracing::warn!(plugin = %id, %error, "a plugin under the retired name stayed");
            }
        }
        let mut installed_any = false;
        for name in names
            .iter()
            .filter(|name| !bootstrap::DEFAULT_PLUGIN_IDS.contains(&name.as_str()))
        {
            let installed = bootstrap::materialize(name).and_then(|materialized| {
                self.plugins().install_materialized(
                    materialized,
                    BUILT_IN_MARKETPLACE,
                    None,
                    &trust::NoTrustAuthority,
                    &uze_core::naming::NoNameCollisionAuthority,
                    trust::SourceOrigin::Operator,
                )
            });
            match installed {
                Ok(_) => installed_any = true,
                Err(error) => tracing::warn!(
                    plugin = %name,
                    %error,
                    "a plugin under the retired name was not installed again"
                ),
            }
        }
        Ok(installed_any)
    }
}
