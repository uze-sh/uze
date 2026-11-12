//! The workspace's extensions as this machine has them: which ones the
//! operator switched off, the declared ones installed plugins carry —
//! where their files are and what they compile to — what the operator
//! accepted for each and where it is enabled, the checkout one may
//! read, and the runner that answers their sources. What an extension draws is the client's.

use std::collections::BTreeSet;

use uze_core::Result;

use uze_workspace::extensions;

use super::services::Extensions;

mod checked;
mod checkout;
mod compiled;
mod consent;
mod costs;
mod fetch;
mod installed;
mod mcp;
mod preferences;
mod program;
mod reach;
mod report;
mod runner;
mod runs;
mod summary;
mod watch;

pub use checked::{CheckedExtension, Overlay};
pub use checkout::{ExtensionCheckout, NotACheckout};
pub(crate) use compiled::CompiledViews;
pub use compiled::{ExtensionLoad, FIXTURE_LIMIT, SourceData, extension_stamp};
pub use consent::{
    Asker, Consented, ExtensionConsent, ExtensionStanding, ExtensionState, ProgramPin, Switched,
    confinement, consent_key, enabled_per_project,
};
pub use costs::{MeasuredCost, MeasuredCosts, bytes_text};
pub use installed::InstalledExtension;
pub(crate) use installed::declared_in;
pub(crate) use mcp::DeliveredPackages;
pub(crate) use program::{ExtensionPictures, ExtensionScratch};
use reach::CheckoutReach;
pub use report::{
    ExtensionCondition, ExtensionReport, PreferenceRow, grant_meaning, secrets_kept_in,
};
pub use runner::ExtensionRunner;
pub use runs::{ExtensionCost, ExtensionRun, MEMORY_TARGET, P95_TARGET, RunOutcome};
pub use summary::DeclaredExtensionSummary;
pub use watch::{WATCH_DEPTH, WATCH_ENTRIES, paths_digest};

impl Extensions<'_> {
    /// The ids switched off; empty until the operator switches one off.
    #[tracing::instrument(name = "extensions.disabled", skip_all, err)]
    pub fn disabled(&self) -> Result<BTreeSet<String>> {
        extensions::disabled(&self.0.home)
    }

    #[tracing::instrument(name = "extensions.set_enabled", skip_all, fields(id, enabled), err)]
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        extensions::set_enabled(&self.0.home, id, enabled)
    }
}

#[cfg(test)]
mod tests {
    use crate::UzeApplication;

    /// A built-in extension switched off is among those disabled until it
    /// is switched on again.
    #[test]
    fn an_extension_switched_off_is_disabled_until_switched_on() {
        let root = uze_testkit::temp::scratch("extensions-switch");
        let app = UzeApplication::new(uze_core::UzeHome::at(root.join("uze")), Vec::new());
        let extensions = app.extensions();
        assert!(extensions.disabled().unwrap().is_empty());

        extensions.set_enabled("focus", false).unwrap();
        assert_eq!(
            extensions
                .disabled()
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            ["focus"]
        );
        extensions.set_enabled("focus", true).unwrap();
        assert!(extensions.disabled().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
