//! The rules engine machine setup delivers (project-rules, design D4): one
//! per harness whose rules route is adapted, recorded under
//! `state::SETUP_OWNER`, removed only after inspection, and checked by
//! `doctor`. Which harness gets what is the integration's answer; nothing
//! here names one.

use std::time::{Duration, SystemTime};

use super::*;
use uze_core::exposure::ManagedArtifact;
use uze_core::integration::{AttachmentInspection, AttachmentReceipt, RulesRoute};

/// What one `setup` did about a harness's rules.
#[derive(Clone, Debug, Serialize)]
pub struct RulesSetup {
    pub route: RulesRoute,
    /// Whether the engine is now delivered to the harness.
    pub delivered: bool,
    /// Why delivering it failed, when it did. The harness's setup still
    /// completes: rules are one more thing it reaches, not a condition of it.
    pub error: Option<String>,
}

/// How long the record of what a conversation was handed is kept. A
/// session idle that long that resumes is handed its rules again, which is
/// all deleting the record ever costs.
pub(crate) const SESSION_RECORD_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

impl UzeApplication {
    /// Delivers the rules engine to `integration` when its route needs one,
    /// and records a receipt per artifact. A route that needs none (native,
    /// unsupported) is reported as it is, with nothing delivered.
    pub(crate) fn deliver_rules_engine(&self, integration: &dyn IntegrationPort) -> RulesSetup {
        let route = integration.rules_route();
        if !route.needs_engine() {
            return RulesSetup {
                route,
                delivered: false,
                error: None,
            };
        }
        let recorded = integration.attach_rules_engine().and_then(|artifacts| {
            for artifact in artifacts {
                state::record_receipt(&self.home, rules_receipt(integration.id(), artifact))?;
            }
            Ok(())
        });
        RulesSetup {
            route,
            delivered: recorded.is_ok(),
            error: recorded.err().map(|error| error.to_string()),
        }
    }

    /// Takes the rules engine back off the harness `integration_id` names:
    /// each of its receipts is inspected and detached, and forgotten only
    /// once what it describes is gone. A drifted or blocked one is left in
    /// place, in the harness and in the ledger, and returned as it was
    /// found.
    pub fn remove_rules_engine(
        &self,
        integration_id: &str,
    ) -> Result<Vec<(String, AttachmentInspection)>> {
        let _mutation = uze_core::persistence::MutationLock::acquire(&self.home)?;
        let integration_id = self.resolve_integration_id(integration_id)?;
        let integration = self
            .integrations
            .iter()
            .find(|integration| integration.id() == integration_id)
            .expect("a resolved integration is registered");
        let mut outcomes = Vec::new();
        for receipt in self.rules_receipts()? {
            if receipt.integration != integration_id {
                continue;
            }
            let inspection = integration.detach_receipt(&receipt)?;
            if inspection.state == AttachmentState::Missing {
                state::forget_receipt(&self.home, &receipt)?;
            }
            outcomes.push((
                receipt.resource_identity.clone().unwrap_or_default(),
                inspection,
            ));
        }
        Ok(outcomes)
    }

    /// Every receipt machine setup holds for the rules engine.
    fn rules_receipts(&self) -> Result<Vec<AttachmentReceipt>> {
        state::receipts(&self.home, Some(state::SETUP_OWNER))
    }

    /// Whether setup delivered the rules engine to `integration`: a receipt
    /// of its own stands for it.
    pub(crate) fn rules_engine_delivered(&self, integration: &dyn IntegrationPort) -> bool {
        self.rules_receipts().is_ok_and(|receipts| {
            receipts
                .iter()
                .any(|receipt| receipt.integration == integration.id())
        })
    }

    /// What `doctor` says about the delivered engine: its dependency missing
    /// and every entry or file of it that is no longer what UZE wrote, each
    /// naming the harnesses affected.
    pub(crate) fn rules_engine_concerns(&self) -> Vec<MachineConcern> {
        let Ok(receipts) = self.rules_receipts() else {
            return Vec::new();
        };
        let mut concerns = Vec::new();
        let mut delivered_to: Vec<&str> = Vec::new();
        for integration in &self.integrations {
            let theirs: Vec<&AttachmentReceipt> = receipts
                .iter()
                .filter(|receipt| receipt.integration == integration.id())
                .collect();
            if theirs.is_empty() {
                continue;
            }
            delivered_to.push(integration.display_name());
            for receipt in theirs {
                let inspection = integration.inspect_receipt(receipt);
                if inspection.state != AttachmentState::Matched {
                    concerns.push(MachineConcern {
                        subject: "rules".to_owned(),
                        detail: format!(
                            "{}: {}; `uze setup` delivers it again, unless it was edited",
                            integration.display_name(),
                            inspection.reason
                        ),
                    });
                }
            }
        }
        let dependency = uze_core::hook::WRAPPER_DEPENDENCY;
        if let Some(concern) = missing_dependency(
            dependency,
            &delivered_to,
            uze_core::subprocess::program_on_path(dependency),
        ) {
            concerns.insert(0, concern);
        }
        concerns
    }
}

/// The concern a delivered engine's missing `dependency` is, naming every
/// harness it was delivered to: the engine fails open, so without this a
/// person sees rules silently stop arriving.
fn missing_dependency(
    dependency: &str,
    delivered_to: &[&str],
    installed: bool,
) -> Option<MachineConcern> {
    (!delivered_to.is_empty() && !installed).then(|| MachineConcern {
        subject: "rules".to_owned(),
        detail: format!(
            "{dependency} is not installed, so the rules engine hands no rules to {}",
            delivered_to.join(", ")
        ),
    })
}

fn rules_receipt(integration: &str, artifact: ManagedArtifact) -> AttachmentReceipt {
    let selector = match &artifact {
        ManagedArtifact::IntegrationOwned { selector, .. } => selector.clone(),
        _ => String::new(),
    };
    AttachmentReceipt {
        package_id: state::SETUP_OWNER.to_owned(),
        resource_identity: Some(format!("rules:{selector}")),
        integration: integration.to_owned(),
        artifact,
    }
}

/// Deletes every per-session record under `dir` last written more than
/// `age` before `now`. Generated tier: a record that goes costs at most a
/// rule handed twice, so anything that cannot be read is simply left.
pub(crate) fn sweep_rule_sessions(dir: &std::path::Path, now: SystemTime, age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|elapsed| elapsed > age)
            });
        if stale && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_dependency_names_every_harness_the_engine_was_delivered_to() {
        let concern = missing_dependency("jq", &["Claude Code", "Codex"], false)
            .expect("a delivered engine without its dependency is a concern");
        assert_eq!(concern.subject, "rules");
        assert!(
            concern.detail.contains("jq is not installed"),
            "{}",
            concern.detail
        );
        assert!(
            concern.detail.contains("Claude Code, Codex"),
            "{}",
            concern.detail
        );
        assert!(missing_dependency("jq", &["Claude Code"], true).is_none());
        assert!(
            missing_dependency("jq", &[], false).is_none(),
            "no engine delivered, nothing to warn about"
        );
    }
}
