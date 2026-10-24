//! The workspace's extensions as this machine has them: which ones the
//! operator switched off, and the declared ones installed plugins carry —
//! where their files are and what they compile to. What an extension draws
//! is the client's.

use std::collections::BTreeSet;

use uze_core::Result;

use uze_workspace::extensions;

use super::services::Extensions;

mod checked;
mod compiled;
mod installed;

pub use checked::CheckedExtension;
pub use compiled::{FIXTURE_LIMIT, SourceData, extension_stamp};
pub use installed::InstalledExtension;

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
