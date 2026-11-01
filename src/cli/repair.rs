//! `uze doctor --repair`: puts back what uze can of its own state, then
//! reports what came of each finding.
//!
//! Runs no bootstrap, like `upgrade` and `uninstall`: what it repairs may
//! be exactly what a bootstrap would trip over. The terminal runtime's
//! endpoint is asked here, since the runtime is the binary's to reach.

use crate::*;
use uze_application::application::{
    HealthFinding, RepairOutcome, RepairReport, Subject, TerminalEndpoint,
};

/// The terminal runtime's own word on its endpoint.
pub(crate) struct WorkspaceEndpoint;

impl TerminalEndpoint for WorkspaceEndpoint {
    fn stale(&self) -> Option<PathBuf> {
        uze_terminal::stale_endpoint().ok().flatten()
    }

    fn clear(&self) -> std::result::Result<bool, String> {
        uze_terminal::clear_stale_endpoint().map_err(|error| error.to_string())
    }
}

pub(crate) fn run_repair(home: UzeHome, format: OutputFormat) -> Result<()> {
    let app = UzeApplication::from_env(home.clone())?;
    let report = with_spinner("Repairing", || {
        let plan = app.repair_plan(&WorkspaceEndpoint);
        Ok(app.repair(plan, &WorkspaceEndpoint))
    })?;
    emit(format, &report, |report| render_repair(home.root(), report));
    EXIT_CODE.store(report.exit_status(), std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

pub(crate) fn render_repair(home: &std::path::Path, report: &RepairReport) -> String {
    use progress::Change;
    let mut lines = format!("{}\n", progress::key_value("home", progress::path(home)));
    if !report.entries.is_empty() {
        lines.push('\n');
    }
    let mut waiting = 0;
    let mut offline = 0;
    let mut failed = 0;
    for entry in &report.entries {
        let finding = &entry.finding;
        let named = named(finding);
        match &entry.outcome {
            RepairOutcome::Repaired => lines.push_str(&progress::change(
                Change::Added,
                &named,
                Some(put_back(finding)),
            )),
            RepairOutcome::NeedsNetwork { reason } => {
                offline += 1;
                lines.push_str(&progress::change(
                    Change::Attention,
                    &named,
                    Some(&format!("needs the network: {reason}")),
                ));
            }
            RepairOutcome::NeedsHuman { reason, command } => {
                waiting += 1;
                lines.push_str(&progress::change(Change::Failed, &named, Some(reason)));
                if let Some(command) = command {
                    lines.push_str(&format!("  {}\n", progress::next_step(command)));
                }
            }
            RepairOutcome::Unrepairable { reason } => {
                failed += 1;
                lines.push_str(&progress::change(
                    Change::Failed,
                    &named,
                    Some(&format!("could not be repaired: {reason}")),
                ));
            }
        }
    }
    if !report.maintenance.outcomes.is_empty() {
        lines.push_str(&format!("\n{}\n", progress::title("Maintenance")));
        for outcome in &report.maintenance.outcomes {
            lines.push_str(&format!("  {outcome}\n"));
        }
    }
    let repaired = report.repaired();
    let outcome = if report.entries.is_empty() {
        format!("{} nothing to repair", progress::success_icon())
    } else {
        let mut said = vec![format!("{repaired} repaired")];
        if waiting > 0 {
            said.push(format!(
                "{waiting} {}",
                plural(waiting, "needs you", "need you")
            ));
        }
        if offline > 0 {
            said.push(format!(
                "{offline} {}",
                plural(offline, "needs the network", "need the network")
            ));
        }
        if failed > 0 {
            said.push(format!(
                "{failed} {}",
                plural(failed, "could not be repaired", "could not be repaired")
            ));
        }
        said.join(", ")
    };
    progress::change_report("doctor --repair", &lines, &outcome)
}

/// What a finding is about, with the harness it is on when that is not
/// what it is about.
fn named(finding: &HealthFinding) -> String {
    match &finding.harness {
        Some(harness) if *harness != finding.about => format!("{} on {harness}", finding.about),
        _ => finding.about.clone(),
    }
}

/// What putting a finding back did, in a few words.
fn put_back(finding: &HealthFinding) -> &'static str {
    match finding.subject {
        Subject::PackageBytes => "bytes acquired again",
        Subject::Builds => "build rendered again",
        Subject::Registrations => "marketplace registered at its store again",
        Subject::TerminalEndpoint => "stale endpoint cleared",
        Subject::Shims => "launcher placed again",
        _ => "put back",
    }
}

#[cfg(test)]
mod tests {
    use uze_application::application::{Lane, RepairEntry, Severity};

    use super::*;

    fn finding(subject: Subject, about: &str, lane: Lane) -> HealthFinding {
        HealthFinding {
            subject,
            lane,
            severity: Severity::Warning,
            about: about.to_owned(),
            detail: "detail".to_owned(),
            harness: None,
            location: None,
            path: None,
            command: None,
        }
    }

    fn plain(text: &str) -> String {
        progress::unpainted(text)
    }

    #[test]
    fn a_healthy_machine_is_one_line_under_its_home() {
        let text = plain(&render_repair(
            std::path::Path::new("/home/me/.uze"),
            &RepairReport::default(),
        ));
        assert!(text.contains("nothing to repair"), "{text}");
        assert!(!text.contains("\n\n\n"), "{text:?}");
    }

    #[test]
    fn each_outcome_says_what_came_of_it_and_what_to_run() {
        let report = RepairReport {
            entries: vec![
                RepairEntry {
                    finding: finding(Subject::PackageBytes, "flow@ai", Lane::Repair),
                    outcome: RepairOutcome::Repaired,
                },
                RepairEntry {
                    finding: finding(Subject::PackageBytes, "lint@ai", Lane::Repair),
                    outcome: RepairOutcome::NeedsHuman {
                        reason: "where it came from now holds other bytes".to_owned(),
                        command: Some("uze update lint@ai".to_owned()),
                    },
                },
                RepairEntry {
                    finding: finding(Subject::PackageBytes, "docs@ai", Lane::Repair),
                    outcome: RepairOutcome::NeedsNetwork {
                        reason: "offline".to_owned(),
                    },
                },
            ],
            maintenance: Default::default(),
        };
        let text = plain(&render_repair(
            std::path::Path::new("/home/me/.uze"),
            &report,
        ));
        assert!(text.contains("flow@ai   bytes acquired again"), "{text}");
        assert!(text.contains("uze update lint@ai"), "{text}");
        assert!(
            text.contains("docs@ai   needs the network: offline"),
            "{text}"
        );
        assert!(
            text.contains("1 repaired, 1 needs you, 1 needs the network"),
            "{text}"
        );
        assert_eq!(report.exit_status(), 1);
    }
}
