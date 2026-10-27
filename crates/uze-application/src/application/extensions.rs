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
mod program;
mod reach;
mod report;
mod runner;
mod runs;
mod summary;
mod watch;

pub use checked::{CheckedExtension, Overlay};
pub use checkout::{ExtensionCheckout, NotACheckout};
pub use compiled::{ExtensionLoad, FIXTURE_LIMIT, SourceData, extension_stamp};
pub use consent::{
    Asker, Consented, ExtensionConsent, ExtensionStanding, ExtensionState, ProgramPin, Switched,
    confinement, consent_key, enabled_per_project,
};
pub use costs::{MeasuredCost, MeasuredCosts, bytes_text};
pub use installed::InstalledExtension;
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
