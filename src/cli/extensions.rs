//! The declared extensions as the CLI reports them (`uze status`,
//! `uze inspect`) and the question `uze install` asks about them on a
//! terminal of the operator's own.
//!
//! Consent and enabling are answered only here and on the workspace's
//! Extensions screen. An agent's pane, a pipe or a script gets the report
//! and nothing else: what it needs is said, and nobody answers for the
//! operator (`extension-consent`).

use uze_application::{ExtensionCondition, ExtensionReport};

use crate::*;

/// Where a report's states are judged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StandingScope {
    /// In the project the command runs in.
    Here,
    /// On the machine: enabled in which projects, with no `here`.
    Machine,
}

/// The `Extensions` section of a report: one row per extension, its state
/// and what it wants, and a row below for each project it is enabled in
/// (machine scope) or for why this checkout runs nothing. Empty when no
/// installed plugin declares an extension.
pub(crate) fn render_extension_standings(
    reports: &[ExtensionReport],
    scope: StandingScope,
) -> String {
    if reports.is_empty() {
        return String::new();
    }
    let mut rows = Vec::new();
    for report in reports {
        rows.push(vec![
            report.attribution(),
            condition_said(report.condition),
            condition_detail(report),
        ]);
        let below = |detail: String| vec![String::new(), String::new(), detail];
        if scope == StandingScope::Machine {
            for project in &report.enabled_in {
                rows.push(below(format!(
                    "{} {}",
                    progress::label("enabled in"),
                    progress::path(project)
                )));
            }
        } else if report.condition == ExtensionCondition::Enabled && report.enabled_in.len() > 1 {
            rows.push(below(progress::label(format!(
                "also enabled in {}",
                count(report.enabled_in.len() - 1, "other project")
            ))));
        }
        if let Some(reason) = &report.checkout_refused {
            rows.push(below(format!(
                "{} {}",
                progress::warning_icon(),
                progress::label(format!("runs nothing here: {reason}"))
            )));
        }
        for program in &report.programs {
            rows.push(below(match &program.path {
                Some(path) => format!("{} {}", program.name, progress::path(path)),
                None => format!(
                    "{} {} {}",
                    progress::warning_icon(),
                    program.name,
                    progress::label(program.unresolved.as_deref().unwrap_or("not found"))
                ),
            }));
        }
        for host in report
            .grants
            .iter()
            .filter_map(|grant| grant.strip_prefix("net:"))
        {
            rows.push(below(format!("{} {host}", progress::label("reaches"))));
        }
        match report.confined {
            Some(true) => rows.push(below(progress::label("its programs run confined"))),
            Some(false) => rows.push(below(format!(
                "{} {}",
                progress::warning_icon(),
                progress::label("its programs run as you: this machine cannot confine them")
            ))),
            None => {}
        }
    }
    let mut text = progress::report_section("Extensions");
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    text
}

fn condition_said(condition: ExtensionCondition) -> String {
    let said = condition.said();
    match condition {
        ExtensionCondition::Enabled | ExtensionCondition::Consented => {
            format!("{} {said}", progress::success_icon())
        }
        ExtensionCondition::NeedsConsent
        | ExtensionCondition::NeedsEnabling
        | ExtensionCondition::Held => format!("{} {said}", progress::warning_icon()),
        ExtensionCondition::Broken => format!("{} {said}", progress::error_icon()),
        ExtensionCondition::FixturesOnly => progress::label(said),
    }
}

/// What a row adds to its state: the grants it wants, or wants anew, the
/// day a same-grant update was applied, or the first thing wrong with it.
fn condition_detail(report: &ExtensionReport) -> String {
    let grants = |grants: &[String]| grants.join(", ");
    match report.condition {
        ExtensionCondition::Held => {
            progress::label(format!("its update wants {}", grants(&report.new_grants)))
        }
        ExtensionCondition::Broken => {
            progress::label(report.errors.first().cloned().unwrap_or_default())
        }
        ExtensionCondition::FixturesOnly => String::new(),
        _ => {
            let mut detail = if report.grants.is_empty() {
                "runs a script".to_owned()
            } else {
                format!("wants {}", grants(&report.grants))
            };
            if let Some(day) = report.updated_on() {
                detail.push_str(&format!(" · updated {day}, same grants"));
            }
            progress::label(detail)
        }
    }
}

/// After an install in a project: each extension the installed plugins
/// declare that waits for the operator here. On a terminal of the
/// operator's own it asks, consent first and then enabling, showing the
/// grants; anywhere else (a pipe, an agent's pane) it says what waits and
/// where to answer. No flag answers it.
pub(crate) fn ask_about_extensions(app: &UzeApplication, plugins: &[String]) {
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let Some(project) = app.extensions().project_here(&cwd) else {
        return;
    };
    let waiting: Vec<ExtensionReport> = app
        .extensions()
        .reports(Some(&project))
        .into_iter()
        .filter(|report| {
            // An install of what the project declares names each plugin
            // as its lock does, bare; one of a single plugin qualifies it.
            plugins
                .iter()
                .any(|plugin| *plugin == report.plugin_id() || *plugin == report.plugin)
        })
        .filter(|report| report.condition.awaits_the_operator())
        .collect();
    if waiting.is_empty() {
        return;
    }
    let may_ask = asker() == uze_application::Asker::Operator && prompt::interactive();
    for report in &waiting {
        if !may_ask || report.condition == ExtensionCondition::Held {
            say_what_waits(report);
            continue;
        }
        if let Err(error) = ask_about(app, report, &project) {
            progress::warn(&error.to_string());
        }
    }
}

/// The line a non-interactive install ends with for one extension.
fn say_what_waits(report: &ExtensionReport) {
    let what = match report.condition {
        ExtensionCondition::Held => format!(
            "{} is held: its update wants {}; accept it from the workspace",
            report.attribution(),
            report.new_grants.join(", ")
        ),
        _ => format!(
            "{} installed disabled; enable it from the workspace",
            report.attribution()
        ),
    };
    progress::warn(&what);
}

fn ask_about(
    app: &UzeApplication,
    report: &ExtensionReport,
    project: &Path,
) -> uze_application::Result<()> {
    let identity = report.identity();
    let extensions = app.extensions();
    if report.condition == ExtensionCondition::NeedsConsent {
        let accepted = progress::uninterrupted(|| {
            eprint!("{}", consent_evidence(report));
            prompt::confirm("Allow it to run?", false)
        });
        if accepted != Some(true) {
            say_what_waits(report);
            return Ok(());
        }
        // Its programs are a question of their own, asked after the rest.
        if !report.programs.is_empty() {
            let confined = report.confined.unwrap_or(false);
            let allowed = progress::uninterrupted(|| {
                eprint!("{}", programs_evidence(report, confined));
                prompt::confirm(
                    if confined {
                        "Allow these programs to run, confined?"
                    } else {
                        "Allow these programs to run as you?"
                    },
                    false,
                )
            });
            if allowed != Some(true) {
                say_what_waits(report);
                return Ok(());
            }
        }
        extensions.consent(&identity, uze_application::Asker::Operator)?;
    }
    let enable = progress::uninterrupted(|| {
        prompt::confirm(&format!("Enable it in {}?", progress::path(project)), true)
    });
    if enable != Some(true) {
        say_what_waits(report);
        return Ok(());
    }
    extensions.enable(&identity, project, uze_application::Asker::Operator)?;
    progress::success(&format!(
        "{} enabled in {}",
        report.attribution(),
        progress::path(project)
    ));
    Ok(())
}

/// What the programs question is about: each program, where it was found,
/// and what running it means on this machine.
fn programs_evidence(report: &ExtensionReport, confined: bool) -> String {
    let mut text = format!(
        "\n  {} {}\n\n",
        progress::warning_icon(),
        progress::label(if confined {
            "runs these programs confined: reading this checkout and its own files, writing only \
             a scratch folder, reaching no network"
        } else {
            "This machine cannot confine programs. Each runs as you, with everything you can \
             reach: your files, your keys, the network."
        })
    );
    let rows = report
        .programs
        .iter()
        .map(|program| {
            let found = match &program.path {
                Some(path) => progress::path(path),
                None => progress::label(program.unresolved.as_deref().unwrap_or("not found")),
            };
            let mut notes = Vec::new();
            if program.runs_anything {
                notes.push("can run anything".to_owned());
            }
            if !program.reads.is_empty() {
                notes.push(format!("reads {}", program.reads.join(", ")));
            }
            vec![
                program.name.clone(),
                found,
                progress::label(notes.join(" · ")),
            ]
        })
        .collect();
    text.push_str(&progress::aligned_rows(rows));
    text.push_str("\n\n");
    text
}

/// What the consent question is about, above it on the stream it is asked
/// on: who the extension is and every grant it wants.
fn consent_evidence(report: &ExtensionReport) -> String {
    let mut text = format!(
        "\n{}",
        progress::report_title(&report.name, Some(&report.attribution()))
    );
    text.push_str(&format!(
        "  {} {}\n\n",
        progress::warning_icon(),
        progress::label("runs a script inside the workspace, with these grants")
    ));
    let rows = if report.grants.is_empty() {
        vec![vec![progress::label(
            "none: it draws what its script answers",
        )]]
    } else {
        report
            .grants
            .iter()
            .filter(|grant| !grant.starts_with("exec:"))
            .map(|grant| {
                vec![
                    grant.clone(),
                    progress::label(uze_application::grant_meaning(grant)),
                ]
            })
            .collect()
    };
    text.push_str(&progress::aligned_rows(rows));
    text.push_str("\n\n");
    text
}
