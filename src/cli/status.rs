//! Machine and project reports: doctor, harnesses, status, context.

use crate::*;

pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 { one } else { many }.to_owned()
}

/// What `uze doctor` found wrong, in the order it prints them: failures
/// (`×`) first, then what only needs attention (`!`).
pub(crate) struct DoctorFindings {
    pub(crate) problems: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn doctor_findings(report: &DoctorReport) -> DoctorFindings {
    use uze_application::application::StoreHealth;
    let mut problems = Vec::new();
    let mut warnings = Vec::new();
    match &report.store {
        StoreHealth::Ready => {}
        StoreHealth::Quarantined(entries) => {
            for entry in entries {
                warnings.push(format!("store  {entry}"));
            }
        }
        StoreHealth::Blocked(reason) => problems.push(format!("store  {reason}")),
    }
    for package in &report.deliveries {
        for harness in &package.harnesses {
            // A package entry that fails fails every capability it carries
            // for one reason: said once, naming them all.
            for group in harness
                .findings
                .chunk_by(|left, right| left.kind == right.kind && left.detail == right.detail)
            {
                let names: Vec<&str> = group
                    .iter()
                    .map(|finding| finding.capability.as_str())
                    .collect();
                // Held back is in place and waiting on the person, not
                // broken: a warning, worded as the wait it is.
                if group[0].kind == uze_application::application::DeliveryFindingKind::HeldBack {
                    warnings.push(format!(
                        "{}  {} of {} held back\n  {}",
                        harness.display_name,
                        names.join(", "),
                        package.plugin,
                        progress::label(&group[0].detail)
                    ));
                    continue;
                }
                let kind = format!("{:?}", group[0].kind).to_lowercase();
                problems.push(format!(
                    "{}  {} of {} {kind}\n  {}",
                    harness.display_name,
                    names.join(", "),
                    package.plugin,
                    progress::label(&group[0].detail)
                ));
            }
        }
    }
    for plugin in &report.plugins {
        for harness in &plugin.undelivered {
            warnings.push(format!(
                "{}  {} not delivered: {}",
                harness.display_name, plugin.id, harness.error
            ));
        }
    }
    for attachment in &report.attachments {
        let state = &attachment.state;
        let wrong = [
            (state.missing, "missing"),
            (state.drifted, "drifted"),
            (state.conflicts, "in conflict"),
            (state.blocked, "blocked"),
        ];
        for (n, what) in wrong {
            if n > 0 {
                problems.push(format!(
                    "{}  {} {what}",
                    attachment.plugin,
                    plural(n, "1 file", &format!("{n} files"))
                ));
            }
        }
        for hook in &attachment.hooks {
            if let Some(loss) = &hook.weakened {
                warnings.push(format!(
                    "{}  hook {} weakened on {}: {loss}",
                    attachment.plugin,
                    hook.hook,
                    harness_label_of(report, &hook.harness)
                ));
            }
        }
    }
    for harness in &report.harnesses {
        if let uze_application::PublicationStatus::Unpublished(reason) = &harness.publication {
            warnings.push(format!(
                "{}  package view not published: {reason}",
                harness.display_name
            ));
        }
    }
    if !report.git_found {
        problems.push(format!(
            "git  not found; marketplaces and agent checkouts need it ({})",
            uze_platform::git::INSTALL_HINT
        ));
    }
    if report.ssh_missing {
        problems.push(
            "ssh  not found; a marketplace is reached over SSH and cannot be refreshed".to_owned(),
        );
    }
    for concern in &report.machine_concerns {
        warnings.push(format!("{}  {}", concern.subject, concern.detail));
    }
    if let Some(refusal) = &report.shell_refusal {
        problems.push(format!(
            "shell  {refusal}; setup steps, gates and hooks need it"
        ));
    }
    if let Some(error) = &report.ledger_error {
        problems.push(format!("ledger  {error}"));
    }
    if let Some(error) = &report.provisioning_state_error {
        problems.push(format!("setup records  {error}"));
    }
    DoctorFindings { problems, warnings }
}

pub(crate) fn render_doctor(report: &DoctorReport) -> String {
    let findings = doctor_findings(report);
    let store = match &report.store {
        uze_application::application::StoreHealth::Ready => {
            format!("{} {}", progress::success_icon(), progress::label("ready"))
        }
        uze_application::application::StoreHealth::Quarantined(_) => format!(
            "{} {}",
            progress::warning_icon(),
            progress::label("ready, some records unreadable")
        ),
        uze_application::application::StoreHealth::Blocked(_) => {
            format!("{} {}", progress::error_icon(), progress::label("blocked"))
        }
    };
    let mut text = progress::aligned_rows(vec![
        vec![progress::label("home"), progress::path(&report.uze_home)],
        vec![progress::label("store"), store],
        vec![progress::label("plugins"), report.plugins.len().to_string()],
    ]);
    text.push_str("\n\n");
    // One row per agent: what it runs, whether it is set up, and how much
    // of every installed plugin reached it — summed, since a column per
    // plugin stops fitting at the third.
    let mut rows = vec![vec![
        String::new(),
        progress::label("version"),
        progress::label("setup"),
        progress::label("delivered"),
    ]];
    for (harness, mut row) in report.harnesses.iter().zip(harness_rows(&report.harnesses)) {
        let (present, expected, healthy) = report
            .deliveries
            .iter()
            .flat_map(|package| &package.harnesses)
            .filter(|delivery| delivery.integration == harness.integration)
            .fold((0, 0, true), |(present, expected, healthy), delivery| {
                (
                    present + delivery.present,
                    expected + delivery.expected,
                    healthy && delivery.healthy(),
                )
            });
        row.push(if !harness.detection.present || expected == 0 {
            String::new()
        } else if healthy {
            format!("{} {present}/{expected}", progress::success_icon())
        } else {
            format!("{} {present}/{expected}", progress::error_icon())
        });
        rows.push(row);
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    if !findings.problems.is_empty() || !findings.warnings.is_empty() {
        text.push('\n');
    }
    for problem in &findings.problems {
        text.push_str(&format!("{} {problem}\n", progress::error_icon()));
    }
    for warning in &findings.warnings {
        text.push_str(&format!("{} {warning}\n", progress::warning_icon()));
    }
    // One line counting them, so an operator finds this right after an
    // update without reading the whole report, and the newest few beneath
    // it — a report that names forty is one nobody reads.
    if report.leftovers.total > 0 {
        text.push_str(&format!(
            "\n{}\n",
            progress::title(format!(
                "{} left by a previous version",
                plural(
                    report.leftovers.total,
                    "1 record",
                    &format!("{} records", report.leftovers.total)
                )
            ))
        ));
        for record in &report.leftovers.set_aside {
            text.push_str(&format!(
                "  {}\n  {}\n",
                progress::path(&record.path),
                progress::label(record.remedy)
            ));
        }
        let listed = report.leftovers.set_aside.len();
        if report.leftovers.total > listed {
            text.push_str(&format!(
                "  {}\n",
                progress::label(format!("and {} older", report.leftovers.total - listed))
            ));
        }
    }
    if !report.leftovers.unregistered_packages.is_empty() {
        text.push_str(&format!(
            "\n{}\n",
            progress::title("Package bytes no install records")
        ));
        for path in &report.leftovers.unregistered_packages {
            text.push_str(&format!("  {}\n", progress::path(path)));
        }
        text.push_str(&format!(
            "  {}\n",
            progress::label(
                "nothing installs, updates or removes them; delete one once you no longer want it"
            )
        ));
    }
    if !report.maintenance.outcomes.is_empty() {
        text.push_str(&format!("\n{}\n", progress::title("Maintenance")));
        for outcome in &report.maintenance.outcomes {
            text.push_str(&format!("  {outcome}\n"));
        }
    }
    let outcome = match (findings.problems.len(), findings.warnings.len()) {
        (0, 0) => format!("{} no problems found", progress::success_icon()),
        (0, warnings) => count(warnings, "warning"),
        (problems, 0) => count(problems, "problem"),
        (problems, warnings) => {
            format!(
                "{}, {}",
                count(problems, "problem"),
                count(warnings, "warning")
            )
        }
    };
    // The docs close the report, as they close `uze --help`: a finding is
    // where a person goes looking for what it means.
    format!(
        "{}\n{}\n",
        progress::change_report("doctor", &text, &outcome),
        progress::aligned_rows(vec![vec![
            progress::label("docs"),
            DOCUMENTATION_URL.to_owned()
        ]])
    )
}

/// The label `DoctorReport.harnesses` carries for a hook row's stable
/// integration id — the id falls back to itself for any future id the
/// report doesn't describe.
pub(crate) fn harness_label_of(report: &DoctorReport, id: &str) -> String {
    report
        .harnesses
        .iter()
        .find(|harness| harness.integration == id)
        .map(|harness| harness.display_name.clone())
        .unwrap_or_else(|| id.to_owned())
}

pub(crate) fn render_market_list(marketplaces: &[MarketplaceSummary]) -> String {
    if marketplaces.is_empty() {
        return format!(
            "no marketplaces registered\n{}\n",
            progress::next_step("uze market add <owner/repo>")
        );
    }
    let rows = marketplaces
        .iter()
        .map(|market| {
            // Where it comes from goes last: it is the one column that can
            // be long, and last it pushes nothing else out of line.
            let source = match &market.linked_to {
                Some(checkout) => format!(
                    "{} {}",
                    progress::accent("linked"),
                    progress::label(progress::path(checkout))
                ),
                None if market.source.starts_with("embedded:") => progress::label("built in"),
                None => progress::label(&market.source),
            };
            vec![
                progress::title(&market.name),
                count(market.plugin_count, "plugin"),
                source,
            ]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

/// The marketplace teardown's answer, in product terms: each package it
/// took off the machine, each block, and where the registry entry ended.
pub(crate) fn render_market_removal(report: &MarketplaceRemovalReport) -> String {
    use progress::Change;
    let mut lines = progress::change(Change::Removed, &report.marketplace, None);
    for package in &report.removed {
        lines.push_str(&progress::change(Change::Removed, package, None));
    }
    for block in &report.blocked {
        lines.push_str(&progress::change(
            Change::Failed,
            &block.package,
            Some(&block.reason),
        ));
    }
    let outcome = if report.record_removed && report.removed.is_empty() {
        "1 marketplace removed, no plugins affected".to_owned()
    } else if report.record_removed {
        format!(
            "1 marketplace removed, {} taken off the machine",
            count(report.removed.len(), "plugin")
        )
    } else {
        format!(
            "{} stays registered until these come off",
            report.marketplace
        )
    };
    progress::change_report("market remove", &lines, &outcome)
}

pub(crate) fn render_market_hosts(hosts: &[HostEntry]) -> String {
    let rows = hosts
        .iter()
        .map(|host| {
            let mark = if host.default {
                progress::accent(progress::glyph(uze_theme::Symbol::StatusSelected))
            } else {
                " ".repeat(progress::glyph_width(uze_theme::Symbol::StatusSelected))
            };
            vec![
                format!("{mark} {}", progress::title(&host.alias)),
                progress::label(&host.base),
            ]
        })
        .collect();
    format!(
        "{}\n\n{}\n",
        progress::aligned_rows(rows),
        progress::label(format!(
            "{} resolves owner/repo; the others answer alias:owner/repo",
            progress::glyph(uze_theme::Symbol::StatusSelected)
        ))
    )
}

pub(crate) fn render_market_detail(detail: &MarketplaceSummary) -> String {
    let source = match &detail.linked_to {
        Some(checkout) => format!("linked {}", progress::path(checkout)),
        None if detail.source.starts_with("embedded:") => "built in".to_owned(),
        None => detail.source.clone(),
    };
    let mut text = progress::report_title(&detail.name, Some(&source));
    text.push_str(&format!("  {}\n", count(detail.plugin_count, "plugin")));
    text
}

pub(crate) fn render_harness_list(harnesses: &[HarnessHealth]) -> String {
    if harnesses.is_empty() {
        return "no agents registered\n".to_owned();
    }
    let unverified: Vec<&str> = harnesses
        .iter()
        .filter(|harness| harness.detection.present && harness.setup != "installed / verified")
        .map(|harness| harness.integration.as_str())
        .collect();
    let mut text = format!("{}\n", progress::aligned_rows(harness_rows(harnesses)));
    if !unverified.is_empty() {
        text.push('\n');
        text.push_str(&progress::next_step(&format!(
            "uze setup {}",
            unverified.join(" ")
        )));
        text.push('\n');
    }
    text
}

/// One row per agent — name, version, setup — shared by `setup list` and
/// `doctor`, which answer the same question for it.
pub(crate) fn harness_rows(harnesses: &[HarnessHealth]) -> Vec<Vec<String>> {
    harnesses
        .iter()
        .map(|harness| {
            if !harness.detection.present {
                return vec![
                    progress::label(&harness.display_name),
                    progress::label(progress::glyph(uze_theme::Symbol::MarkUnsupported)),
                    progress::label("not found"),
                ];
            }
            vec![
                progress::title(&harness.display_name),
                harness.detection.version.clone().unwrap_or_default(),
                setup_state(&harness.setup),
            ]
        })
        .collect()
}

/// A harness's setup as the mark and word a person reads.
pub(crate) fn setup_state(setup: &str) -> String {
    match setup {
        "installed / verified" => {
            format!("{} {}", progress::success_icon(), progress::label("set up"))
        }
        "installed / unverified" => {
            format!(
                "{} {}",
                progress::warning_icon(),
                progress::label("not verified")
            )
        }
        _ => progress::label("not set up"),
    }
}

pub(crate) fn render_harness_detail(harness: &HarnessHealth) -> String {
    let mut text =
        progress::report_title(&harness.display_name, harness.detection.version.as_deref());
    text.push('\n');
    let mut rows = Vec::new();
    if !harness.detection.present {
        rows.push(vec![
            progress::label("binary"),
            progress::label("not found"),
        ]);
    }
    rows.push(vec![progress::label("setup"), setup_state(&harness.setup)]);
    if let Some(provisioning) = &harness.provisioning {
        rows.push(vec![
            progress::label("installed by"),
            progress::label(provisioning.method.replace('-', " ")),
        ]);
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    text
}

/// Why `uze status` answered with the machine: `-m` asked for it, or there
/// was no project to answer about.
#[derive(Clone, Copy)]
pub(crate) enum MachineAsked {
    Explicitly,
    NoProjectHere,
}

/// `uze status -m`, or `uze status` outside a project: the machine read
/// model. The absence of a project is stated as the fact it is — and only
/// when it is one — and the packages answer follows.
pub(crate) fn render_machine_status(report: &MachineStatusReport, asked: MachineAsked) -> String {
    let mut text = String::new();
    if let MachineAsked::NoProjectHere = asked {
        text.push_str(&progress::label(
            "no project here, so this is the machine\n\n",
        ));
    }
    text.push_str(&render_plugin_list(&report.packages));
    text.push_str(&render_held_back(&report.held_back));
    text
}

/// What a harness holds back until the person acts in it: one line per
/// plugin on a harness, naming how many of its capabilities wait, and the
/// action under it. Empty when nothing waits.
pub(crate) fn render_held_back(notes: &[uze_application::application::HeldBackNote]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let mut text = format!("\n{}", progress::report_section("Waiting on you"));
    for group in notes.chunk_by(|left, right| {
        left.harness == right.harness && left.plugin == right.plugin && left.action == right.action
    }) {
        let one = &group[0];
        let count = match group.len() {
            1 => "1 capability".to_owned(),
            n => format!("{n} capabilities"),
        };
        text.push_str(&format!(
            "{} {}  {count} of {} held back\n  {}\n",
            progress::warning_icon(),
            one.harness,
            one.plugin,
            progress::label(&one.action)
        ));
    }
    text
}

/// What `uze status` answers inside a project: the package manager's
/// report, and what the workspace adds about the commands its policy runs.
#[derive(serde::Serialize)]
pub(crate) struct ProjectStatus {
    #[serde(flatten)]
    pub(crate) report: StatusReport,
    pub(crate) steps_not_spelled_here:
        Vec<uze_application::application::services::StepNotSpelledHere>,
}

pub(crate) fn render_status(status: &ProjectStatus) -> String {
    let report = &status.report;
    let name = report.root.file_name().map_or_else(
        || "project".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut text = progress::report_title(&name, Some(&progress::path(&report.root)));
    text.push_str(&format!("{}\n\n", status_headline(status)));

    text.push_str(&progress::report_section("Context"));
    let mut instructions = render_status_instructions(&report.instructions);
    if report.drift.stale_projection {
        instructions[1] = format!(
            "{} {}",
            progress::warning_icon(),
            progress::label("behind what uze keeps there")
        );
    }
    let mut rows = vec![instructions];
    rows.extend(report.harnesses.iter().map(render_status_harness));
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    text.push_str(&render_portability_gaps(&report.portability));

    let plugins = render_project_lock_status(&report.project_lock);
    let drift = render_drift(&report.drift);
    if !plugins.is_empty() || !drift.is_empty() {
        text.push('\n');
        text.push_str(&progress::report_section("Plugins"));
        text.push_str(&plugins);
        text.push_str(&drift);
    }
    if !report.drift.unreproducible_marketplaces.is_empty() {
        text.push('\n');
        text.push_str(&progress::report_section("Sources"));
        text.push_str(&progress::aligned_rows(
            report
                .drift
                .unreproducible_marketplaces
                .iter()
                .map(|market| {
                    vec![
                        market.clone(),
                        progress::label(
                            "a path only this machine has; will not reproduce elsewhere",
                        ),
                    ]
                })
                .collect(),
        ));
        text.push('\n');
    }
    text.push_str(&render_held_back(&report.held_back));
    for issue in &report.issues {
        text.push_str(&format!("{} {issue}\n", progress::warning_icon()));
    }
    for unspelled in &status.steps_not_spelled_here {
        let (step, consequence) = match unspelled.step {
            uze_application::PolicyStep::Setup => {
                ("setup", "skipped when a checkout is placed here")
            }
            uze_application::PolicyStep::Gate => ("gate", "every delivery from here is refused"),
        };
        text.push_str(&format!(
            "{} {step} `{}` has no `{}` spelling in agents.yaml: {consequence}\n",
            progress::warning_icon(),
            unspelled.command,
            unspelled.platform,
        ));
    }
    if let Some(step) = status_next_step(status) {
        text.push('\n');
        if step.starts_with("uze ") {
            text.push_str(&progress::next_step(step));
        } else {
            text.push_str(&progress::label(step));
        }
        text.push('\n');
    }
    text
}

pub(crate) fn status_headline(status: &ProjectStatus) -> String {
    let report = &status.report;
    let missing = locked_plugin_count(&report.project_lock);
    let attention = |what: String| {
        format!(
            "{} {}",
            progress::warning_icon(),
            progress::warning_text(what)
        )
    };
    if !report.issues.is_empty() {
        return attention("context needs reconciling".to_owned());
    }
    if missing > 0 {
        return attention(format!("{} not installed", count(missing, "plugin")));
    }
    let drift = &report.drift;
    if !drift.unresolved.is_empty() || !drift.surplus.is_empty() || !drift.missing.is_empty() {
        return attention("declared, not yet applied".to_owned());
    }
    if !status.steps_not_spelled_here.is_empty() {
        return attention(format!(
            "{} not spelled for this machine",
            count(status.steps_not_spelled_here.len(), "command")
        ));
    }
    match &report.portability {
        Portability::Portable => format!(
            "{} {}",
            progress::success_icon(),
            progress::success_text("ready")
        ),
        Portability::NoContext => attention("no AGENTS.md yet".to_owned()),
        _ => attention("context does not reach every agent".to_owned()),
    }
}

/// What `agents.yaml` asks for that the rest of the chain has not caught
/// up to. Silent when there is nothing owed — a clean project says nothing
/// rather than saying "no drift", which is a sentence nobody needs.
pub(crate) fn render_drift(drift: &uze_application::application::EnvironmentDrift) -> String {
    let mut rows = Vec::new();
    let mut attention = |plugins: &[String], what: &str| {
        for plugin in plugins {
            rows.push(vec![
                plugin.clone(),
                format!("{} {what}", progress::warning_icon()),
            ]);
        }
    };
    attention(&drift.unresolved, "declared, not installed");
    attention(&drift.surplus, "in agents.lock, no longer declared");
    attention(&drift.missing, "locked, absent from this machine");
    if rows.is_empty() {
        String::new()
    } else {
        format!("{}\n", progress::aligned_rows(rows))
    }
}

/// The file the rows beneath it are about. A coverage list that never
/// names the document it covers reads as an answer to a question nobody
/// asked — and when the file is absent, its absence *is* the finding.
pub(crate) fn render_status_instructions(
    instructions: &uze_application::application::InstructionsFile,
) -> Vec<String> {
    let name = instructions.path.file_name().map_or_else(
        || instructions.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let state = if !instructions.exists {
        format!("{} {}", progress::warning_icon(), progress::label("absent"))
    } else if instructions.managed_regions == 0 {
        format!(
            "{} {}",
            progress::success_icon(),
            progress::label("present")
        )
    } else {
        format!(
            "{} {}",
            progress::success_icon(),
            progress::label(format!(
                "{} {} managed by UZE",
                instructions.managed_regions,
                plural(instructions.managed_regions, "region", "regions")
            ))
        )
    };
    vec![name, state]
}

/// Why this project is not portable, in the words the inspection already
/// found. `status` says "needs attention" on its own line; without this
/// it never says what about.
pub(crate) fn render_portability_gaps(portability: &Portability) -> String {
    let gaps: Vec<String> = match portability {
        Portability::Portable | Portability::NoContext => return String::new(),
        Portability::PartiallyPortable { gaps } => gaps.clone(),
        Portability::VendorLocked { files } => files
            .iter()
            .map(|path| {
                format!(
                    "{}: carries instructions no other harness reads",
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned()
                    )
                )
            })
            .collect(),
    };
    gaps.iter()
        .map(|gap| format!("{} {gap}\n", progress::warning_icon()))
        .collect()
}

pub(crate) fn render_status_harness(
    harness: &uze_application::application::HarnessContextStatus,
) -> Vec<String> {
    let ok = |word: &str| format!("{} {}", progress::success_icon(), progress::label(word));
    let state = match &harness.delivery {
        HarnessContextDelivery::Native => ok("native"),
        HarnessContextDelivery::Projected => ok("inside the workspace"),
        HarnessContextDelivery::NotDetected => progress::label("not installed"),
        HarnessContextDelivery::Bridge {
            state: uze_application::AttachmentState::Matched,
            ..
        } => ok("bridged"),
        HarnessContextDelivery::Bridge { needed: false, .. } => ok("reads AGENTS.md natively"),
        HarnessContextDelivery::Bridge { .. } => format!(
            "{} {}",
            progress::warning_icon(),
            progress::label("needs reconciling")
        ),
    };
    vec![harness.display_name.clone(), state]
}

pub(crate) fn locked_plugin_count(
    status: &uze_application::application::ProjectLockStatus,
) -> usize {
    match status {
        uze_application::application::ProjectLockStatus::Present { plugins } => {
            plugins.iter().filter(|plugin| !plugin.installed).count()
        }
        _ => 0,
    }
}

/// The one command this project is owed, in the words of somebody who
/// runs `uze status` and nothing else. `install` converges the manifest
/// and leaves the context reconciled, so it is the answer to everything a
/// command *can* answer — but a project with no `AGENTS.md` is owed a
/// decision about what goes in one, and naming a command there would
/// promise a repair that running it does not perform.
pub(crate) fn status_next_step(status: &ProjectStatus) -> Option<&'static str> {
    let report = &status.report;
    if locked_plugin_count(&report.project_lock) > 0 || !report.drift.is_clear() {
        return Some("uze install");
    }
    if !status.steps_not_spelled_here.is_empty() {
        return Some("spell those commands for this machine's shell in agents.yaml");
    }
    match &report.portability {
        Portability::NoContext | Portability::VendorLocked { .. } => {
            Some("write an AGENTS.md; the uze:init skill drafts one with you")
        }
        Portability::Portable if report.issues.is_empty() => None,
        _ => Some("uze install"),
    }
}

pub(crate) fn render_project_lock_status(
    status: &uze_application::application::ProjectLockStatus,
) -> String {
    use uze_application::application::ProjectLockStatus;
    match status {
        ProjectLockStatus::Absent => String::new(),
        // The reason already names the file and what is wrong with it.
        ProjectLockStatus::Malformed { reason } => {
            format!("  {} {reason}\n", progress::warning_icon())
        }
        ProjectLockStatus::Present { plugins } => {
            if plugins.is_empty() {
                return String::new();
            }
            let rows = plugins
                .iter()
                .map(|plugin| {
                    // Quiet on purpose: past the lock is where the machine
                    // stands after every update, and `uze update` is how a
                    // project records it.
                    let state = if plugin.installed && plugin.differs_from_lock {
                        format!(
                            "{} {}",
                            progress::success_icon(),
                            progress::label("installed · not the lock's revision")
                        )
                    } else if plugin.installed {
                        format!(
                            "{} {}",
                            progress::success_icon(),
                            progress::label("installed")
                        )
                    } else {
                        format!(
                            "{} {}",
                            progress::warning_icon(),
                            progress::label("not installed")
                        )
                    };
                    vec![plugin.plugin.clone(), state]
                })
                .collect();
            format!("{}\n", progress::aligned_rows(rows))
        }
    }
}

pub(crate) fn render_context_status(status: &ProjectContextStatus) -> String {
    let portable = match &status.portability {
        Portability::Portable => format!(
            "{} {}",
            progress::success_icon(),
            progress::label("portable")
        ),
        other => format!(
            "{} {}",
            progress::warning_icon(),
            progress::label(render_portability(other))
        ),
    };
    let mut text = format!(
        "{}  {}  {portable}\n\n",
        progress::title("context"),
        progress::label(progress::path(&status.canonical))
    );
    let mut rows: Vec<Vec<String>> = status
        .sources
        .iter()
        .map(|source| {
            if !source.exists {
                return vec![source.file_name.clone(), progress::label("absent")];
            }
            let regions = source.managed_region_identities.len();
            vec![
                source.file_name.clone(),
                format!(
                    "{} {}, {}",
                    regions,
                    plural(regions, "managed region", "managed regions"),
                    if source.has_user_content {
                        "with user text"
                    } else {
                        "no user text"
                    }
                ),
            ]
        })
        .collect();
    text.push_str(&progress::aligned_rows(std::mem::take(&mut rows)));
    text.push_str("\n\n");
    for harness in &status.harnesses {
        let delivery = match &harness.delivery {
            HarnessContextDelivery::Native => progress::success_text("native"),
            HarnessContextDelivery::Projected => progress::success_text("inside the workspace"),
            HarnessContextDelivery::NotDetected => progress::label("not installed"),
            HarnessContextDelivery::Bridge { needed: false, .. } => format!(
                "{}  {}",
                progress::success_text("native"),
                progress::label("no bridge needed")
            ),
            HarnessContextDelivery::Bridge { state, .. } => {
                let word = attachment_word(*state);
                if matches!(state, uze_application::AttachmentState::Matched) {
                    progress::success_text(format!("bridge {word}"))
                } else {
                    progress::warning_text(format!("bridge {word}"))
                }
            }
        };
        rows.push(vec![harness.display_name.clone(), delivery]);
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    let mut notes = String::new();
    for contribution in &status.contributions {
        notes.push_str(&format!(
            "  {}   {}\n",
            contribution.package_id,
            progress::label(format!("{:?}", contribution.state).to_lowercase())
        ));
    }
    for orphan in &status.orphaned_regions {
        notes.push_str(&format!(
            "{} {orphan}   {}\n",
            progress::warning_icon(),
            progress::label("orphaned: no installed package claims it")
        ));
    }
    for malformed in &status.malformed_regions {
        notes.push_str(&format!(
            "{} {malformed}   {}\n",
            progress::error_icon(),
            progress::label("malformed: its markers cannot be trusted")
        ));
    }
    for warning in &status.warnings {
        notes.push_str(&format!("{} {warning}\n", progress::warning_icon()));
    }
    if !notes.is_empty() {
        text.push('\n');
        text.push_str(&notes);
    }
    text
}

/// A receipt's state as the word a person reads.
pub(crate) fn attachment_word(state: uze_application::AttachmentState) -> &'static str {
    use uze_application::AttachmentState;
    match state {
        AttachmentState::Matched => "in place",
        AttachmentState::Missing => "missing",
        AttachmentState::Drifted => "drifted",
        AttachmentState::Conflict => "in conflict",
        AttachmentState::Blocked => "blocked",
    }
}

pub(crate) fn render_portability(portability: &Portability) -> String {
    match portability {
        Portability::NoContext => "no instructions file yet".to_owned(),
        Portability::Portable => "portable".to_owned(),
        Portability::PartiallyPortable { gaps } => {
            format!("partially portable: {}", gaps.join("; "))
        }
        Portability::VendorLocked { files } => format!(
            "locked to one vendor: {}",
            files
                .iter()
                .map(|path| progress::path(path))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub(crate) fn render_action(action: &PlannedAction) -> String {
    match action {
        PlannedAction::Attach => progress::accent("write"),
        PlannedAction::NoChange => progress::label("unchanged"),
        PlannedAction::Remove => progress::warning_text("remove"),
        PlannedAction::Blocked(reason) => progress::error_text(format!("blocked: {reason}")),
    }
}

/// Bridge rows show the human label (`app.integration_label`); the plan's
/// own keys stay the stable ids — which is what `--format json` emits.
pub(crate) fn render_context_plan(plan: &ContextPlan, app: &UzeApplication) -> String {
    let mut rows = Vec::new();
    for contribution in &plan.agents_md_plan.contributions {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            contribution.package_id.as_str().to_owned(),
            render_action(&contribution.action),
        ]);
    }
    for orphan in &plan.agents_md_plan.orphans {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            orphan.region_identity.clone(),
            render_action(&orphan.action),
        ]);
    }
    for bridge in &plan.bridges {
        rows.push(vec![
            bridge.file.file_name().map_or_else(
                || progress::path(&bridge.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            app.health().integration_label(&bridge.integration),
            render_action(&bridge.action),
        ]);
    }
    if let Some(region) = &plan.authoring_region {
        rows.push(vec![
            region.file.file_name().map_or_else(
                || progress::path(&region.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            "plugin authoring".to_owned(),
            render_action(&region.action),
        ]);
        for identity in &region.superseded {
            rows.push(vec![
                String::new(),
                identity.clone(),
                progress::warning_text("remove, superseded"),
            ]);
        }
    }
    let checked = rows.len();
    if !plan.has_changes() {
        return format!(
            "{} nothing to reconcile {}\n",
            progress::success_icon(),
            progress::label(format!(
                "· {} checked",
                plural(checked, "1 entry", &format!("{checked} entries"))
            ))
        );
    }
    format!(
        "{}\n\n{}\n",
        progress::aligned_rows(rows),
        progress::next_step("uze agent context reconcile")
    )
}

pub(crate) fn render_context_reconciliation(
    report: &ContextReconciliationReport,
    app: &UzeApplication,
) -> String {
    let mut rows = Vec::new();
    for package in &report.packages {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            package.package_id.to_string(),
            progress::label(format!("{:?}", package.state).to_lowercase()),
        ]);
    }
    for orphan in &report.removed_orphans {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            orphan.clone(),
            progress::label("removed, orphaned"),
        ]);
    }
    for (orphan, reason) in &report.blocked_orphans {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            orphan.clone(),
            progress::error_text(format!("blocked: {reason}")),
        ]);
    }
    for (package, reason) in &report.failed {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            package.to_string(),
            progress::error_text(format!("failed: {reason}")),
        ]);
    }
    if let Some(region) = &report.authoring_region {
        rows.push(vec![
            region.file.file_name().map_or_else(
                || progress::path(&region.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            "plugin authoring".to_owned(),
            progress::label(format!("{:?}", region.state).to_lowercase()),
        ]);
        for identity in &region.removed_superseded {
            rows.push(vec![
                String::new(),
                identity.clone(),
                progress::label("removed, superseded"),
            ]);
        }
        for (identity, reason) in &region.blocked_superseded {
            rows.push(vec![
                String::new(),
                identity.clone(),
                progress::error_text(format!("blocked: {reason}")),
            ]);
        }
    }
    for bridge in &report.bridges {
        rows.push(vec![
            bridge.file.file_name().map_or_else(
                || progress::path(&bridge.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            app.health().integration_label(&bridge.integration),
            progress::label(format!("{:?}", bridge.state).to_lowercase()),
        ]);
    }
    let lines = if rows.is_empty() {
        String::new()
    } else {
        format!("{}\n", progress::aligned_rows(rows))
    };
    progress::change_report(
        "agent context reconcile",
        &lines,
        &format!("reconciled {}", progress::path(&report.agents_md)),
    )
}

pub(crate) fn render_managed_state(states: &[uze_application::AttachmentState]) -> String {
    let mut matched = 0;
    let mut missing = 0;
    let mut drifted = 0;
    let mut conflict = 0;
    let mut blocked = 0;
    for state in states {
        match state {
            uze_application::AttachmentState::Matched => matched += 1,
            uze_application::AttachmentState::Missing => missing += 1,
            uze_application::AttachmentState::Drifted => drifted += 1,
            uze_application::AttachmentState::Conflict => conflict += 1,
            uze_application::AttachmentState::Blocked => blocked += 1,
        }
    }
    format!(
        "{matched} matched, {missing} missing, {drifted} drifted, {conflict} conflicts, {blocked} blocked"
    )
}
