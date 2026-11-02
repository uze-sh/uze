//! `uze doctor --repair`: puts back what uze can of its own state, then
//! reports what came of each finding.
//!
//! Runs no bootstrap, like `upgrade` and `uninstall`: what it repairs may
//! be exactly what a bootstrap would trip over. The terminal runtime's
//! endpoint is asked here, since the runtime is the binary's to reach.

use crate::*;
use uze_application::application::{
    HealthFinding, RepairOutcome, RepairReport, SchemaEntry, SchemaFinding, Subject,
    TerminalEndpoint,
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

pub(crate) fn run_repair(
    home: UzeHome,
    format: OutputFormat,
    discard_leftovers: bool,
) -> Result<()> {
    let app = UzeApplication::from_env(home.clone())?;
    let report = with_spinner("Repairing", || {
        let plan = app.repair_plan(&WorkspaceEndpoint);
        Ok(app.repair(plan, &WorkspaceEndpoint, discard_leftovers))
    })?;
    emit(format, &report, |report| render_repair(home.root(), report));
    EXIT_CODE.store(report.exit_status(), std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

pub(crate) fn render_repair(home: &std::path::Path, report: &RepairReport) -> String {
    use progress::Change;
    let mut lines = format!("{}\n", progress::key_value("home", progress::path(home)));
    if let Some(measured) = &report.measured {
        lines.push_str(&format!(
            "{}\n",
            progress::key_value("size", uze::size::bytes_to_say(measured.total()))
        ));
    }
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
    lines.push_str(&render_home(home, &report.home));
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

/// What the home holds that its schema does not account for: what was
/// removed, and what is kept until the person says.
fn render_home(home: &std::path::Path, entries: &[SchemaEntry]) -> String {
    use progress::Change;
    if entries.is_empty() {
        return String::new();
    }
    let mut lines = format!("\n{}\n", progress::title("Home"));
    for entry in entries {
        let path = entry
            .path
            .strip_prefix(home)
            .unwrap_or(&entry.path)
            .display()
            .to_string();
        let size = entry.bytes.map(uze::size::bytes_to_say).unwrap_or_default();
        let (kind, detail) = match (entry.finding, entry.removed) {
            (SchemaFinding::Retired, true) => {
                (Change::Removed, "left by an earlier version".to_owned())
            }
            (SchemaFinding::EmptyDirectory, true) => (Change::Removed, "empty".to_owned()),
            (SchemaFinding::SetAside | SchemaFinding::Superseded, true) => {
                (Change::Removed, format!("{size} discarded"))
            }
            (SchemaFinding::SetAside, false) => (
                Change::Attention,
                format!("{size}, a record set aside that nothing reads"),
            ),
            (SchemaFinding::Superseded, false) => (
                Change::Attention,
                format!("{size}, the only copy of a version a failed restore kept"),
            ),
            (SchemaFinding::Unknown, _) => (
                Change::Attention,
                "unknown, maybe from a newer uze: left as it is".to_owned(),
            ),
            (SchemaFinding::Retired | SchemaFinding::EmptyDirectory, false) => {
                (Change::Failed, "could not be removed".to_owned())
            }
        };
        lines.push_str(&progress::change(kind, &path, Some(&detail)));
    }
    if entries
        .iter()
        .any(|entry| entry.is_leftover() && !entry.removed)
    {
        lines.push_str(&format!(
            "  {}\n",
            progress::next_step("uze doctor --repair --discard-leftovers")
        ));
    }
    lines
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
            ..RepairReport::default()
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

    #[test]
    fn the_home_says_what_it_kept_and_how_to_discard_it() {
        let home = std::path::Path::new("/home/me/.uze");
        let entry = |path: &str, finding, bytes, removed| SchemaEntry {
            path: home.join(path),
            tier: "record",
            finding,
            bytes,
            removed,
        };
        let report = RepairReport {
            home: vec![
                entry("state/profiles.json", SchemaFinding::Retired, None, true),
                entry(
                    "state/hosts.json.unreadable-1",
                    SchemaFinding::SetAside,
                    Some(2048),
                    false,
                ),
                entry("state/newer.json", SchemaFinding::Unknown, None, false),
            ],
            ..RepairReport::default()
        };
        let text = plain(&render_repair(home, &report));
        assert!(
            text.contains("state/profiles.json   left by an earlier version"),
            "{text}"
        );
        assert!(
            text.contains("state/hosts.json.unreadable-1   2.0 KB, a record set aside"),
            "{text}"
        );
        assert!(
            text.contains("state/newer.json   unknown, maybe from a newer uze"),
            "{text}"
        );
        assert!(
            text.contains("uze doctor --repair --discard-leftovers"),
            "{text}"
        );
        assert!(text.contains("nothing to repair"), "{text}");
    }
}
