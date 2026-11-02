//! `uze uninstall`: taking uze off this machine, after showing the plan.
//!
//! Runs no bootstrap, like `upgrade`: nothing is seeded, prepared or
//! published on the way out. What is running is checked here rather than
//! in the application, because the terminal runtime is the binary's to ask.

use crate::*;
use uze::size::bytes_to_say;
use uze_application::application::{
    BinaryOutcome, BinaryPlan, ReceiptFate, UninstallMode, UninstallPlan, UninstallReport,
};

pub(crate) fn run_uninstall(home: UzeHome, yes: bool, purge: bool) -> Result<()> {
    refuse_while_running(&home)?;
    let app = UzeApplication::from_env(home.clone())?;
    let interactive = prompt::interactive();
    let mode = if purge {
        UninstallMode::Purge
    } else if yes || !interactive {
        UninstallMode::Program
    } else {
        match ask_mode() {
            Some(mode) => mode,
            None => {
                progress::success("nothing changed");
                return Ok(());
            }
        }
    };
    let plan = with_spinner("Reading what uze holds on this machine", || {
        Ok(app.uninstall_plan(mode))
    })?;
    if plan.is_empty() {
        progress::success("nothing of uze's is on this machine");
        return Ok(());
    }
    print!("{}", progress::for_terminal(&render_plan(&plan)));
    if !yes {
        let again = match mode {
            UninstallMode::Program => "uze uninstall --yes",
            UninstallMode::Purge => "uze uninstall --yes --purge",
        };
        if !interactive {
            return Err(uze_application::UzeError::UninstallRefused {
                reason: "nothing was removed: uninstall asks before it acts, and there is no \
                         one here to answer"
                    .to_owned(),
                next: again.to_owned(),
            });
        }
        let question = match mode {
            UninstallMode::Program => "Remove uze, keeping your plugins working?",
            UninstallMode::Purge => "Remove uze and every plugin it delivered?",
        };
        if progress::uninterrupted(|| prompt::confirm(question, false)) != Some(true) {
            progress::success("nothing changed");
            return Ok(());
        }
    }
    // Asked again under the lock: an agent or an update may have started
    // while the plan was read or the question was open.
    let still_clear = || refuse_while_running(&home);
    let report = with_spinner("Uninstalling", || app.uninstall(&plan, &still_clear))?;
    print!("{}", progress::for_terminal(&render_report(&report)));
    if report.needs_the_person() {
        EXIT_CODE.store(1, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

/// The two ways off the machine, asked when neither was named.
fn ask_mode() -> Option<UninstallMode> {
    let choices = [
        prompt::Choice {
            label: "keep my plugins".to_owned(),
            hint: "remove the program; what it delivered keeps working".to_owned(),
            selected: true,
        },
        prompt::Choice {
            label: "remove everything".to_owned(),
            hint: "uninstall every plugin it delivered, then ~/.uze".to_owned(),
            selected: false,
        },
    ];
    let picked =
        progress::uninterrupted(|| prompt::select("How should uze leave this machine?", &choices))?;
    Some(if picked == 0 {
        UninstallMode::Program
    } else {
        UninstallMode::Purge
    })
}

/// Refuses, naming it, while anything of uze's runs: a shell inside one of
/// the workspace's panes, its agents, clients or panes, or an update being
/// put in place. A workspace server holding nothing is stopped instead.
fn refuse_while_running(home: &UzeHome) -> Result<()> {
    let refused = |reason: String, next: &str| uze_application::UzeError::UninstallRefused {
        reason,
        next: next.to_owned(),
    };
    if std::env::var_os(uze_terminal::launch::PANE_VARIABLE).is_some() {
        return Err(refused(
            "this shell is a pane of the workspace, and uninstall would end it".to_owned(),
            "run uze uninstall from a terminal outside the workspace",
        ));
    }
    if uze::self_update::underway(home) {
        return Err(refused(
            "an update is being installed right now".to_owned(),
            "uze uninstall, once it is done",
        ));
    }
    match uze_terminal::usage() {
        Ok(uze_terminal::Usage::NotRunning) => Ok(()),
        Ok(uze_terminal::Usage::OtherBuild) => Err(refused(
            "a workspace started by another uze build is running".to_owned(),
            "uze workspace stop",
        )),
        Ok(uze_terminal::Usage::Running(report)) => {
            let mut running = Vec::new();
            if report.agents.count > 0 {
                running.push(count(report.agents.count as usize, "agent"));
            }
            if report.clients.count > 0 {
                running.push(count(report.clients.count as usize, "open workspace"));
            }
            if report.shells.count > 0 {
                running.push(count(report.shells.count as usize, "shell"));
            }
            if running.is_empty() {
                uze_terminal::stop().map_err(terminal_error)?;
                return Ok(());
            }
            let next = if report.agents.count > 0 {
                "end the agents, then uze workspace stop"
            } else {
                "uze workspace stop"
            };
            Err(refused(
                format!("still running in the workspace: {}", running.join(", ")),
                next,
            ))
        }
        Err(error) => Err(refused(
            format!("cannot tell whether the workspace is running: {error}"),
            "uze workspace stop",
        )),
    }
}

fn mode_to_say(mode: UninstallMode) -> &'static str {
    match mode {
        UninstallMode::Program => "remove the program, keep your plugins",
        UninstallMode::Purge => "remove every trace",
    }
}

/// The plan, before anything changes: what goes, what stays, what is left
/// for a reason, and the work uninstall never touches.
pub(crate) fn render_plan(plan: &UninstallPlan) -> String {
    let mut text = progress::report_title("uze uninstall", Some(mode_to_say(plan.mode)));
    let mut remove: Vec<Vec<String>> = plan
        .entries
        .iter()
        .filter(|entry| entry.remove)
        .map(|entry| {
            vec![
                progress::path(&entry.path),
                progress::label(entry.holds),
                bytes_to_say(entry.bytes),
            ]
        })
        .collect();
    if let BinaryPlan::Placed { path, path_entry } = &plan.binary {
        remove.push(vec![
            progress::path(path),
            progress::label("the program"),
            String::new(),
        ]);
        if let Some(entry) = path_entry {
            remove.push(vec![
                progress::path(entry),
                progress::label("on your Path"),
                String::new(),
            ]);
        }
    }
    for project in &plan.projects {
        for link in &project.links {
            remove.push(vec![
                progress::path(link),
                progress::label("workspace link"),
                String::new(),
            ]);
        }
    }
    section(&mut text, "Remove", remove);
    let keep = plan
        .entries
        .iter()
        .filter(|entry| !entry.remove)
        .map(|entry| {
            vec![
                progress::path(&entry.path),
                progress::label(entry.holds),
                bytes_to_say(entry.bytes),
            ]
        })
        .collect();
    let mut keep: Vec<Vec<String>> = keep;
    for harness in &plan.harnesses {
        for store in &harness.keeps {
            keep.push(vec![
                progress::path(store),
                progress::label(format!("what is left on {} still reads it", harness.name)),
                String::new(),
            ]);
        }
    }
    section(&mut text, "Keep", keep);
    if plan.mode == UninstallMode::Purge {
        let agents = plan
            .harnesses
            .iter()
            .map(|harness| {
                let uninstalled = harness
                    .receipts
                    .iter()
                    .filter(|receipt| receipt.fate == ReceiptFate::Detach)
                    .count();
                let left = harness
                    .receipts
                    .iter()
                    .filter(|receipt| receipt.fate == ReceiptFate::Leave)
                    .count();
                let mut said = format!("{uninstalled} to uninstall");
                if left > 0 {
                    said.push_str(&format!(" · {left} left"));
                }
                vec![progress::accent(&harness.name), said]
            })
            .collect();
        section(&mut text, "Agents", agents);
        let left = plan
            .left()
            .map(|(harness, receipt)| {
                vec![
                    format!("{} {}", progress::warning_icon(), receipt.subject),
                    progress::label(format!("{} · {}", harness.name, receipt.reason)),
                ]
            })
            .collect();
        section(&mut text, "Left as it is", left);
    }
    section(&mut text, "Not touched", untouched(plan));
    text.push('\n');
    text
}

/// What uninstall leaves on purpose: agents uze installed for you, a
/// binary something else put there, and work not landed yet.
fn untouched(plan: &UninstallPlan) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = plan
        .provisioned
        .iter()
        .map(|name| {
            vec![
                progress::accent(name),
                progress::label("installed by uze setup, and yours to keep"),
            ]
        })
        .collect();
    if let BinaryPlan::Elsewhere { path, remove_with } = &plan.binary {
        rows.push(vec![
            progress::path(path),
            progress::label(binary_left_to_say(remove_with.as_deref())),
        ]);
    }
    if let Some(running) = &plan.running {
        rows.push(vec![
            progress::path(&running.path),
            progress::label(binary_left_to_say(running.remove_with.as_deref())),
        ]);
    }
    rows.extend(work_rows(&plan.projects));
    rows
}

fn binary_left_to_say(remove_with: Option<&str>) -> String {
    match remove_with {
        Some(command) => format!("not placed by the installer: {command}"),
        None => "not placed by the installer: remove it the way it was put there".to_owned(),
    }
}

fn work_rows(projects: &[uze_application::application::ProjectWork]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for project in projects {
        for checkout in &project.unlanded {
            rows.push(vec![
                progress::path(&checkout.path),
                progress::label(format!("{} not landed", count(checkout.commits, "commit"))),
            ]);
        }
        if !project.shelves.is_empty() {
            rows.push(vec![
                progress::path(&project.root),
                progress::label(format!(
                    "{} on a shelf",
                    count(project.shelves.len(), "agent")
                )),
            ]);
        }
    }
    rows
}

fn section(text: &mut String, name: &str, rows: Vec<Vec<String>>) {
    if rows.is_empty() {
        return;
    }
    text.push('\n');
    text.push_str(&progress::report_section(name));
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
}

/// What uninstall did, in the change report every command closes with, and
/// under it what a person may still want to do.
pub(crate) fn render_report(report: &UninstallReport) -> String {
    let mut lines = String::new();
    for (harness, subject) in &report.detached {
        lines.push_str(&progress::change(
            progress::Change::Removed,
            subject,
            Some(harness),
        ));
    }
    for (harness, name) in &report.withdrawn {
        lines.push_str(&progress::change(
            progress::Change::Removed,
            &format!("marketplace {name}"),
            Some(harness),
        ));
    }
    for link in &report.links_removed {
        lines.push_str(&progress::change(
            progress::Change::Removed,
            &progress::path(link),
            Some("workspace link"),
        ));
    }
    if let BinaryOutcome::Removed {
        path,
        path_entry_removed,
    } = &report.binary
    {
        lines.push_str(&progress::change(
            progress::Change::Removed,
            &progress::path(path),
            Some("the program"),
        ));
        if let Some(entry) = path_entry_removed {
            lines.push_str(&progress::change(
                progress::Change::Removed,
                &progress::path(entry),
                Some("from your Path"),
            ));
        }
    }
    for path in &report.removed {
        lines.push_str(&progress::change(
            progress::Change::Removed,
            &progress::path(path),
            None,
        ));
    }
    for (harness, subject, reason) in &report.left {
        lines.push_str(&progress::change(
            progress::Change::Attention,
            subject,
            Some(&format!("left on {harness}: {reason}")),
        ));
    }
    if report.mode == UninstallMode::Purge {
        for path in report
            .kept
            .iter()
            .filter(|path| path.starts_with(&report.home))
        {
            lines.push_str(&progress::change(
                progress::Change::Attention,
                &progress::path(path),
                Some("kept: what is left still reads it"),
            ));
        }
    }
    for step in &report.manual {
        lines.push_str(&progress::change(
            progress::Change::Attention,
            step,
            Some("left for you"),
        ));
    }
    let outcome = match report.mode {
        _ if report.needs_the_person() => "Removed uze; finish what is listed above by hand",
        UninstallMode::Program => "Removed uze from this machine; your plugins keep working",
        UninstallMode::Purge if report.left.is_empty() => "Removed uze and everything it delivered",
        UninstallMode::Purge => "Removed uze; what it left is listed above",
    };
    let mut text = progress::change_report("uninstall", &lines, outcome);
    let mut notes: Vec<Vec<String>> = report
        .provisioned
        .iter()
        .map(|name| {
            vec![
                progress::accent(name),
                progress::label("installed by uze setup, and yours to keep"),
            ]
        })
        .collect();
    if let BinaryOutcome::Left { path, remove_with } = &report.binary {
        notes.push(vec![
            progress::path(path),
            progress::label(binary_left_to_say(remove_with.as_deref())),
        ]);
    }
    if let Some(running) = &report.running {
        notes.push(vec![
            progress::path(&running.path),
            progress::label(binary_left_to_say(running.remove_with.as_deref())),
        ]);
    }
    notes.extend(work_rows(&report.projects));
    section(&mut text, "Not touched", notes);
    if report.mode == UninstallMode::Program {
        text.push('\n');
        text.push_str(&progress::label(
            if matches!(report.binary, BinaryOutcome::Removed { .. }) {
                "To remove every trace later, install uze again, then run"
            } else {
                "To remove every trace later, run"
            },
        ));
        text.push('\n');
        text.push_str(&progress::next_step("uze uninstall --purge"));
        text.push('\n');
    }
    text
}
