//! What an installed package needs from the machine, and whether the
//! machine has it: the report `plugin install`, `plugin list` and `doctor`
//! print, and that the TUI shows on a package.
//!
//! Read-only end to end. A gap carries the command that would close it, for
//! the person to run; nothing here starts an installer.

use std::cell::OnceCell;

use serde::Serialize;
use uze_core::{
    Result,
    integration::AttachmentReceipt,
    requirement::{EffectiveRequirement, EffectiveRequirements, RequirementSource},
    requirement_check::{PackageManager, RequirementChecker},
    store::StoredPackage,
};

pub use uze_core::requirement_check::RequirementStatus;

use super::UzeApplication;

/// Every requirement of one installed package and where the machine stands
/// on each.
#[derive(Clone, Debug, Serialize)]
pub struct PackageRequirements {
    pub package: String,
    pub requirements: Vec<RequirementLine>,
}

impl PackageRequirements {
    /// The requirements the machine does not meet.
    pub fn gaps(&self) -> impl Iterator<Item = &RequirementLine> {
        self.requirements
            .iter()
            .filter(|requirement| !requirement.status.is_met())
    }
}

/// One executable a package needs.
#[derive(Clone, Debug, Serialize)]
pub struct RequirementLine {
    pub executable: String,
    /// The minimum version, as declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// Who needs it: `the plugin`, `hook wrapper`, `hook \`guard\``.
    pub needed_by: Vec<String>,
    /// The hook groups that deny every call they match while this is
    /// unmet; empty when nothing is denied for want of it.
    pub denies_while_unmet: Vec<String>,
    pub status: RequirementStatus,
    /// The command that installs it with a package manager this machine
    /// has, when one is known; `None` means it is installed by hand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_command: Option<String>,
}

/// One pass over the machine for any number of packages: each executable is
/// asked once, the ledger is read once, and package managers are looked
/// for only once something is missing.
pub(crate) struct RequirementCheck<'a> {
    application: &'a UzeApplication,
    checker: RequirementChecker,
    receipts: Vec<AttachmentReceipt>,
    managers: OnceCell<Vec<PackageManager>>,
}

impl UzeApplication {
    pub(crate) fn requirement_check(&self) -> RequirementCheck<'_> {
        RequirementCheck {
            application: self,
            checker: RequirementChecker::new(&self.home),
            receipts: uze_core::state::receipts(&self.home, None).unwrap_or_default(),
            managers: OnceCell::new(),
        }
    }
}

impl RequirementCheck<'_> {
    pub(crate) fn of(&self, package: &StoredPackage) -> Result<PackageRequirements> {
        let effective = self.effective(package)?;
        Ok(PackageRequirements {
            package: package.id.as_str().to_owned(),
            requirements: effective.iter().map(|entry| self.line(entry)).collect(),
        })
    }

    /// What the package declares, plus what every harness it was delivered
    /// to generated for it.
    fn effective(&self, package: &StoredPackage) -> Result<EffectiveRequirements> {
        let declared = uze_core::store::read_plugin_manifest(&package.root)
            .map(|manifest| manifest.requirements)
            .unwrap_or_default();
        let resources = uze_core::engine::package_resources(package)?;
        let resources: Vec<_> = resources.iter().collect();
        let contributed = self
            .application
            .integrations
            .iter()
            .filter(|integration| {
                self.receipts.iter().any(|receipt| {
                    receipt.package_id == package.id.as_str()
                        && receipt.integration == integration.id()
                })
            })
            .flat_map(|integration| integration.generated_requirements(package, &resources))
            .collect::<Vec<_>>();
        Ok(EffectiveRequirements::of(&declared, contributed))
    }

    fn line(&self, entry: &EffectiveRequirement) -> RequirementLine {
        let requirement = &entry.requirement;
        let status = self.checker.status(requirement);
        let install_command = (!status.is_met())
            .then(|| {
                uze_core::requirement_check::install_command(
                    &requirement.executable,
                    self.managers
                        .get_or_init(uze_core::requirement_check::available_managers),
                )
            })
            .flatten();
        RequirementLine {
            executable: requirement.executable.clone(),
            minimum: requirement.version.clone(),
            purpose: requirement.purpose.clone(),
            needed_by: entry
                .sources
                .iter()
                .map(RequirementSource::to_string)
                .collect(),
            denies_while_unmet: entry
                .denying_hooks()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            status,
            install_command,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_core::requirement::Requirement;

    #[test]
    fn a_line_names_every_source_and_the_hooks_that_deny_without_it() {
        let entry = EffectiveRequirement {
            requirement: Requirement::named("python3").with_purpose("the guard"),
            sources: vec![
                RequirementSource::Author,
                RequirementSource::Hook {
                    id: "guard".to_owned(),
                    fails_closed: true,
                },
            ],
        };
        assert_eq!(
            entry
                .sources
                .iter()
                .map(RequirementSource::to_string)
                .collect::<Vec<_>>(),
            vec!["the plugin".to_owned(), "hook `guard`".to_owned()]
        );
        assert_eq!(entry.denying_hooks(), vec!["guard"]);
    }
}
