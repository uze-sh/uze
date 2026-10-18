//! Actionable machine-health signals, for the management footer's
//! status and the dialog it opens.

use uze_application::AttachmentState;
use uze_application::application::DoctorReport;

pub(crate) use crate::ui::widget::alerts::{Alert, Severity};

/// Projects the current health snapshot into only the problems an operator
/// can act on. Passing checks stay out of the Overview: their detailed state
/// already belongs to the Plugins and Integrations screens.
pub(crate) fn actionable_alerts(doctor: Option<&DoctorReport>) -> Vec<Alert> {
    let Some(doctor) = doctor else {
        return Vec::new();
    };
    let mut alerts = Vec::new();
    for (label, error) in [
        ("Attachment ledger unreadable", &doctor.ledger_error),
        (
            "Provisioning state unreadable",
            &doctor.provisioning_state_error,
        ),
    ] {
        if let Some(detail) = error {
            alerts.push(Alert {
                severity: Severity::High,
                label: label.to_owned(),
                detail: detail.clone(),
            });
        }
    }
    // Not `High`: nothing is broken and nothing is waiting. It is here so
    // that an upgrade which could not carry something across is met by a
    // sentence rather than by work the operator cannot find.
    if doctor.leftovers.total > 0 {
        alerts.push(Alert {
            severity: Severity::Low,
            label: format!(
                "{} record{} left by a previous version",
                doctor.leftovers.total,
                if doctor.leftovers.total == 1 { "" } else { "s" }
            ),
            detail: match doctor.leftovers.set_aside.first() {
                Some(newest) => format!("{} — {}", newest.path.display(), newest.remedy),
                None => "nothing reads them; remove them when you no longer want them".to_owned(),
            },
        });
    }
    let configured: Vec<_> = doctor
        .harnesses
        .iter()
        .filter(|harness| harness.configured())
        .collect();
    for package in &doctor.attachments {
        let state = &package.state;
        if state.conflicts > 0 || state.blocked > 0 {
            alerts.push(Alert {
                severity: Severity::High,
                label: format!("{} needs attention", package.plugin),
                detail: format!("{} conflict(s), {} blocked", state.conflicts, state.blocked),
            });
        } else if state.drifted > 0 {
            alerts.push(Alert {
                severity: Severity::Medium,
                label: format!("{} attachments drifted", package.plugin),
                detail: format!("{} attachment(s) differ from UZE's record", state.drifted),
            });
        } else if state.missing > 0 {
            alerts.push(Alert {
                severity: Severity::Low,
                label: format!("{} attachments missing", package.plugin),
                detail: format!("{} attachment(s) need reattaching", state.missing),
            });
        }
        // A hook row exists for every harness UZE knows, configured or not;
        // what one the operator never set up would lose is not their problem.
        for (hook, harness) in package.hooks.iter().filter_map(|hook| {
            configured
                .iter()
                .find(|harness| harness.integration == hook.harness)
                .map(|harness| (hook, harness))
        }) {
            if let Some(loss) = &hook.weakened {
                alerts.push(Alert {
                    severity: Severity::Medium,
                    label: format!(
                        "{} hook {} is approximated on {}",
                        package.plugin, hook.hook, harness.display_name
                    ),
                    detail: loss.clone(),
                });
            }
            match hook.state {
                Some(
                    AttachmentState::Drifted | AttachmentState::Conflict | AttachmentState::Blocked,
                ) => {
                    alerts.push(Alert {
                        severity: Severity::High,
                        label: format!(
                            "{} hook {} needs attention on {}",
                            package.plugin, hook.hook, harness.display_name
                        ),
                        detail: hook.event.clone(),
                    });
                }
                Some(AttachmentState::Missing) => alerts.push(Alert {
                    severity: Severity::Low,
                    label: format!(
                        "{} hook {} is missing on {}",
                        package.plugin, hook.hook, harness.display_name
                    ),
                    detail: hook.event.clone(),
                }),
                _ => {}
            }
        }
    }
    for plugin in &doctor.plugins {
        // Startup already applies every update it can settle on its own,
        // so one still standing here is one that needs a person: an
        // out-of-band source, new executable capability to confirm, or
        // managed state the update refused to disturb.
        if plugin.freshness.behind() {
            alerts.push(Alert {
                severity: Severity::Low,
                label: format!("{} update available", plugin.id),
                detail: "Open Plugins and press u to confirm it".to_owned(),
            });
        }
    }
    alerts.sort_by_key(|alert| alert.severity);
    alerts
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uze_application::application::MaintenanceReport;

    use super::*;

    #[test]
    fn overview_alerts_classify_conflicts_as_high_and_missing_as_low() {
        use uze_application::application::{ManagedStateSummary, PackageManagedState};
        let doctor = DoctorReport {
            uze_home: PathBuf::from("/home"),
            store: uze_application::application::StoreHealth::Ready,
            plugins: Vec::new(),
            harnesses: Vec::new(),
            attachments: vec![
                PackageManagedState {
                    hooks: Vec::new(),
                    plugin: "acme".to_owned(),
                    state: ManagedStateSummary {
                        matched: 0,
                        missing: 1,
                        drifted: 0,
                        conflicts: 1,
                        blocked: 0,
                        ledger_error: None,
                    },
                },
                PackageManagedState {
                    hooks: Vec::new(),
                    plugin: "example".to_owned(),
                    state: ManagedStateSummary {
                        matched: 0,
                        missing: 1,
                        drifted: 0,
                        conflicts: 0,
                        blocked: 0,
                        ledger_error: None,
                    },
                },
            ],
            deliveries: Vec::new(),
            ledger_error: None,
            provisioning_state_error: None,
            leftovers: Default::default(),
            maintenance: MaintenanceReport::default(),
            git_found: true,
            shell_refusal: None,
            ssh_missing: false,
            machine_concerns: Vec::new(),
        };
        let alerts = actionable_alerts(Some(&doctor));
        assert_eq!(alerts[0].severity, Severity::High);
        assert!(alerts.iter().any(|alert| alert.severity == Severity::Low));
    }
}
