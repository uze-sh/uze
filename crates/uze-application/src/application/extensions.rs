//! Which built-in extensions this machine switched off. Only the choice:
//! what switching one off takes away is the client's, which draws them.

use std::collections::BTreeSet;

use uze_core::{Result, extensions};

use super::services::Extensions;

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
