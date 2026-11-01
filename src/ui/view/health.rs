//! Actionable machine-health signals, for the management footer's
//! status and the dialog it opens.

use uze_application::AttachmentState;
use uze_application::application::{DeliveryFindingKind, DoctorReport, Severity as HealthSeverity};

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
    // What only an explicit repair, or the person, puts back: each names
    // what to run, `uze doctor --repair` for the repair lane.
    for finding in doctor
        .health
        .iter()
        .filter(|finding| !finding.has_own_row())
    {
        alerts.push(Alert {
            severity: match finding.severity {
                HealthSeverity::Problem => Severity::High,
                HealthSeverity::Warning => Severity::Medium,
            },
            label: finding.headline(),
            detail: match finding.action() {
                Some(action) => format!("{}. Run {action}", finding.detail),
                None => finding.detail.clone(),
            },
        });
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
    // A hook a harness holds back until its own review does not run at
    // all, a deny guard included: nothing is broken, but nothing protects
    // either until the person reviews it there. It stands until the
    // harness reads it as trusted.
    for package in &doctor.deliveries {
        for harness in &package.harnesses {
            let held: Vec<_> = harness
                .findings
                .iter()
                .filter(|finding| finding.kind == DeliveryFindingKind::HeldBack)
                .collect();
            for group in held.chunk_by(|left, right| left.detail == right.detail) {
                let names: Vec<&str> = group
                    .iter()
                    .map(|finding| finding.capability.as_str())
                    .collect();
                alerts.push(Alert {
                    severity: Severity::High,
                    label: format!(
                        "{} holds back {} of {}",
                        harness.display_name,
                        names.join(", "),
                        package.plugin
                    ),
                    detail: group[0].detail.clone(),
                });
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

    fn doctor_with(
        deliveries: Vec<uze_application::application::PackageDeliveryHealth>,
    ) -> DoctorReport {
        DoctorReport {
            program_confinement: Default::default(),
            extension_costs: None,
            uze_home: PathBuf::from("/home"),
            store: uze_application::application::StoreHealth::Ready,
            plugins: Vec::new(),
            harnesses: Vec::new(),
            attachments: Vec::new(),
            deliveries,
            ledger_error: None,
            provisioning_state_error: None,
            leftovers: Default::default(),
            maintenance: MaintenanceReport::default(),
            git_found: true,
            shell_refusal: None,
            ssh_missing: false,
            machine_concerns: Vec::new(),
            health: Vec::new(),
        }
    }

    /// A package whose bytes are gone waits on an explicit repair, and the
    /// alert says which command that is.
    #[test]
    fn missing_bytes_name_the_repair() {
        use uze_application::application::{HealthFinding, Lane, Subject};
        let mut doctor = doctor_with(Vec::new());
        doctor.health.push(HealthFinding {
            subject: Subject::PackageBytes,
            lane: Lane::Repair,
            severity: HealthSeverity::Warning,
            about: "flow@ai".to_owned(),
            detail: "its bytes are missing".to_owned(),
            path: Some("plugin_dir"),
            command: None,
        });
        let alerts = actionable_alerts(Some(&doctor));
        assert_eq!(alerts.len(), 1, "{alerts:?}");
        assert_eq!(alerts[0].label, "flow@ai is missing its bytes");
        assert!(
            alerts[0].detail.ends_with("Run uze doctor --repair"),
            "{}",
            alerts[0].detail
        );
    }

    #[test]
    fn a_hook_a_harness_holds_back_is_named_with_its_review_until_trusted() {
        use uze_application::application::{
            DeliveryFinding, HarnessDeliveryHealth, PackageDeliveryHealth,
        };
        let review = "Codex runs it only once you trust it: open Codex and choose \
                      \"Trust all and continue\" on its hook review";
        let held = |findings: Vec<DeliveryFinding>| {
            doctor_with(vec![PackageDeliveryHealth {
                plugin: "guard@acme".to_owned(),
                harnesses: vec![HarnessDeliveryHealth {
                    integration: "codex".to_owned(),
                    display_name: "Codex".to_owned(),
                    expected: 2,
                    present: 2,
                    findings,
                }],
            }])
        };
        let doctor = held(
            ["protect-env", "format"]
                .map(|hook| DeliveryFinding {
                    capability: hook.to_owned(),
                    kind: DeliveryFindingKind::HeldBack,
                    detail: review.to_owned(),
                })
                .to_vec(),
        );

        let alerts = actionable_alerts(Some(&doctor));

        assert_eq!(alerts.len(), 1, "{alerts:?}");
        assert_eq!(alerts[0].severity, Severity::High);
        assert_eq!(
            alerts[0].label,
            "Codex holds back protect-env, format of guard@acme"
        );
        assert_eq!(alerts[0].detail, review);
        assert!(
            actionable_alerts(Some(&held(Vec::new()))).is_empty(),
            "once trusted, the notice is gone"
        );
    }

    #[test]
    fn overview_alerts_classify_conflicts_as_high_and_missing_as_low() {
        use uze_application::application::{ManagedStateSummary, PackageManagedState};
        let doctor = DoctorReport {
            program_confinement: Default::default(),
            extension_costs: None,
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
            health: Vec::new(),
        };
        let alerts = actionable_alerts(Some(&doctor));
        assert_eq!(alerts[0].severity, Severity::High);
        assert!(alerts.iter().any(|alert| alert.severity == Severity::Low));
    }
}
