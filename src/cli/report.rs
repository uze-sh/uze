//! Package reports: updates, listings, inspections, deliveries, installs.

use crate::*;

/// The one word each freshness state reads as, everywhere.
///
/// "not checked" is spelled out rather than left blank: a listing that says
/// nothing about a plugin reads as a plugin with nothing to say, which is
/// exactly the collapse this replaces — `update_available: None` drew the
/// same as "current".
pub(crate) fn freshness_label(freshness: &uze_application::application::Freshness) -> String {
    use uze_application::application::FreshnessState;
    match &freshness.state {
        FreshnessState::UpToDate => "up to date".to_owned(),
        FreshnessState::Behind { commits: None } => "behind".to_owned(),
        FreshnessState::Behind {
            commits: Some(commits),
        } => format!("{commits} behind"),
        FreshnessState::Linked { checkout } => format!("linked to {}", checkout.display()),
        FreshnessState::Unpinned => "unpinned".to_owned(),
        FreshnessState::NotChecked => "not checked".to_owned(),
    }
}

/// What `uze update` moved, and what it deliberately did not.
///
/// A plugin already at its ref's head is said out loud rather than left
/// out: "nothing moved" and "this plugin was not considered" are different
/// answers, and a report that shows only what changed cannot tell them
/// apart.
/// An update as a line per plugin that moved or could not, with the ones
/// already current counted rather than listed.
pub(crate) fn render_update_summary(report: &uze_application::application::UpdateReport) -> String {
    use progress::Change;
    use uze_application::application::{UpdateOutcome, UpdateScope};
    let mut lines = String::new();
    let mut moved = 0;
    for outcome in &report.outcomes {
        match outcome {
            UpdateOutcome::Moved {
                plugin,
                revision,
                deliveries,
            } => {
                moved += 1;
                lines.push_str(&progress::change(
                    Change::Updated,
                    plugin,
                    Some(short_commit(revision)),
                ));
                lines.push_str(&delivery_issues(deliveries).0);
            }
            UpdateOutcome::FollowedLink {
                plugin,
                linked_source: checkout,
                deliveries,
            } => {
                moved += 1;
                let detail = match report.scope {
                    UpdateScope::Machine => {
                        format!(
                            "updated from the linked working tree at {}",
                            checkout.display()
                        )
                    }
                    UpdateScope::Project => format!(
                        "updated from the linked working tree at {}, which pins nothing",
                        checkout.display()
                    ),
                };
                lines.push_str(&progress::change(Change::Updated, plugin, Some(&detail)));
                lines.push_str(&delivery_issues(deliveries).0);
            }
            // What a current plugin lacks on a harness is not news: its
            // install said so, and `uze status -m` still does.
            UpdateOutcome::AlreadyCurrent { .. } => {}
            UpdateOutcome::Held { plugin, reason } => {
                lines.push_str(&progress::change(Change::Attention, plugin, Some(reason)));
            }
            UpdateOutcome::Blocked { plugin, reason } => {
                lines.push_str(&progress::change(
                    Change::Failed,
                    plugin,
                    Some(&format!("blocked: {reason}")),
                ));
            }
        }
    }
    if report.reconciled {
        lines.push_str(&progress::change(
            Change::Updated,
            "AGENTS.md",
            Some("reconciled"),
        ));
    }
    let outcome = if report.outcomes.is_empty() {
        match report.scope {
            UpdateScope::Project => "this project declares no plugins".to_owned(),
            UpdateScope::Machine => "no plugins installed on this machine".to_owned(),
        }
    } else if moved == 0 {
        format!(
            "{} up to date {}",
            progress::success_icon(),
            progress::label(format!("· {}", count(report.outcomes.len(), "plugin")))
        )
    } else {
        format!("{} updated", count(moved, "plugin"))
    };
    progress::change_report("update", &lines, &outcome)
}

pub(crate) fn render_update_report(report: &uze_application::application::UpdateReport) -> String {
    use uze_application::application::{UpdateOutcome, UpdateScope};
    let (scope, nothing_considered) = match report.scope {
        UpdateScope::Project => ("This project's pins", "This project declares no plugins"),
        UpdateScope::Machine => (
            "Packages on this machine",
            "No packages installed on this machine",
        ),
    };
    let title = progress::report_title("Update", Some(scope));
    if report.outcomes.is_empty() {
        return format!("{title}\n  {nothing_considered}\n");
    }
    let rows = report
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            UpdateOutcome::Moved {
                plugin, revision, ..
            } => vec![
                progress::title(plugin),
                progress::label(format!("moved to {}", &revision[..revision.len().min(12)])),
            ],
            UpdateOutcome::FollowedLink {
                plugin,
                linked_source: checkout,
                ..
            } => vec![
                progress::title(plugin),
                progress::label(match report.scope {
                    UpdateScope::Machine => format!(
                        "updated from the linked working tree at {}",
                        checkout.display()
                    ),
                    UpdateScope::Project => format!(
                        "updated from the linked working tree at {}, which pins nothing",
                        checkout.display()
                    ),
                }),
            ],
            UpdateOutcome::AlreadyCurrent { plugin, .. } => {
                vec![progress::title(plugin), progress::label("already current")]
            }
            UpdateOutcome::Held { plugin, reason } => {
                vec![progress::title(plugin), progress::label(reason)]
            }
            UpdateOutcome::Blocked { plugin, reason } => vec![
                progress::title(plugin),
                progress::warning_text(format!("blocked — {reason}")),
            ],
        })
        .collect();
    let mut text = format!("{title}\n{}\n", progress::aligned_rows(rows));
    let delivered: Vec<_> = report
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            UpdateOutcome::Moved {
                plugin, deliveries, ..
            }
            | UpdateOutcome::FollowedLink {
                plugin, deliveries, ..
            }
            | UpdateOutcome::AlreadyCurrent { plugin, deliveries }
                if !deliveries.is_empty() =>
            {
                Some((plugin, deliveries))
            }
            _ => None,
        })
        .collect();
    if !delivered.is_empty() {
        text.push_str(&format!("\n{}", progress::report_section("Delivery")));
        for (plugin, deliveries) in delivered {
            text.push_str(&format!("  {}\n", progress::title(plugin)));
            text.push_str(&render_deliveries(deliveries, false, "    "));
        }
    }
    if report.reconciled {
        text.push_str("  Project context reconciled\n");
    }
    text
}

pub(crate) fn render_plugin_list(
    plugins: &[uze_application::application::PluginSummary],
) -> String {
    if plugins.is_empty() {
        return "no plugins installed on this machine\n".to_owned();
    }
    let rows = plugins
        .iter()
        .map(|plugin| {
            let state = match &plugin.freshness.state {
                uze_application::application::FreshnessState::UpToDate => format!(
                    "{} {}",
                    progress::success_icon(),
                    progress::label("up to date")
                ),
                uze_application::application::FreshnessState::Linked { checkout } => format!(
                    "{} {}",
                    progress::accent("linked"),
                    progress::label(progress::path(checkout))
                ),
                uze_application::application::FreshnessState::Behind { .. } => format!(
                    "{} {}",
                    progress::warning_icon(),
                    progress::label(freshness_label(&plugin.freshness))
                ),
                _ => progress::label(freshness_label(&plugin.freshness)),
            };
            let delivery = if plugin.undelivered.is_empty() {
                String::new()
            } else {
                progress::warning_text("partially delivered")
            };
            vec![
                progress::title(&plugin.id),
                progress::label(format!(
                    "{} {}",
                    plugin.capability_count,
                    plural(plugin.capability_count, "capability", "capabilities")
                )),
                state,
                delivery,
            ]
        })
        .collect();
    let mut text = format!("{}\n", progress::aligned_rows(rows));
    let undelivered = render_undelivered(plugins);
    if !undelivered.is_empty() {
        text.push('\n');
        text.push_str(&undelivered);
    }
    text
}

/// Each package the listing calls partially delivered, with the harness it
/// did not reach and why — a word in a table row names the state, and
/// this is what a person acts on.
pub(crate) fn render_undelivered(
    plugins: &[uze_application::application::PluginSummary],
) -> String {
    let mut text = String::new();
    for plugin in plugins {
        for harness in &plugin.undelivered {
            text.push_str(&format!(
                "  {} {} not delivered to {}: {}\n",
                progress::warning_icon(),
                progress::title(&plugin.active_name),
                harness.display_name,
                harness.error
            ));
        }
    }
    text
}

pub(crate) fn render_inspection(report: &PluginInspection, verbose: bool) -> String {
    use uze_application::CompatibilityRoute;
    let source = if report.plugin.source.starts_with('/') {
        progress::path(Path::new(&report.plugin.source))
    } else {
        report.plugin.source.clone()
    };
    let mut text = progress::report_title(&report.plugin.id, Some(&source));
    text.push('\n');
    // Agents across, capabilities down: there are at most a handful of
    // agents, and a plugin may carry any number of capabilities.
    let mut header = vec![String::new()];
    header.extend(
        report
            .deliveries
            .iter()
            .map(|delivery| progress::label(&delivery.display_name)),
    );
    let mut rows = vec![header];
    let mut notes = String::new();
    for capability in &report.capabilities {
        let mut row = vec![format!(
            "{} {}",
            progress::label(capability_word(capability.kind)),
            capability.name
        )];
        for delivery in &report.deliveries {
            let delivered = delivery
                .capabilities
                .iter()
                .find(|delivered| delivered.identity == capability.identity);
            // Narrowed to one agent, each cell also says the name a session
            // there sees the capability under.
            let seen_as = delivered
                .filter(|_| report.deliveries.len() == 1)
                .map(|one| {
                    format!(
                        " {}",
                        one.exposed_name.as_deref().unwrap_or(one.identity.as_str())
                    )
                })
                .unwrap_or_default();
            row.push(
                match delivered {
                    None => String::new(),
                    Some(one) if one.blocked.is_some() => progress::error_icon(),
                    Some(one) if one.kind == uze_application::CapabilityKind::Instruction => {
                        progress::success_icon()
                    }
                    Some(one) => match one.route {
                        CompatibilityRoute::Native => progress::success_icon(),
                        CompatibilityRoute::Adaptable | CompatibilityRoute::Degraded => {
                            progress::warning_text(progress::glyph(uze_theme::Symbol::MarkAdapted))
                        }
                        CompatibilityRoute::Unsupported => {
                            progress::label(progress::glyph(uze_theme::Symbol::MarkUnsupported))
                        }
                    },
                } + &seen_as,
            );
            if let Some(one) = delivered {
                let lost = match (&one.blocked, one.route) {
                    (Some(reason), _) => Some(format!("blocked: {reason}")),
                    (None, CompatibilityRoute::Native) => None,
                    (None, route) if one.kind != uze_application::CapabilityKind::Instruction => {
                        Some(format!(
                            "{}: {}",
                            route_word(route),
                            leading_sentence(&one.evidence)
                        ))
                    }
                    _ => None,
                };
                if let Some(lost) = lost {
                    notes.push_str(&progress::change(
                        progress::Change::Attention,
                        &delivery.display_name,
                        Some(&format!(
                            "{} {} {lost}",
                            capability_word(capability.kind),
                            capability.name
                        )),
                    ));
                }
            }
        }
        rows.push(row);
    }
    let mut envelope = vec![progress::label("delivery")];
    envelope.extend(report.deliveries.iter().map(|delivery| {
        progress::label(match &delivery.route {
            uze_application::application::DeliveryRoute::Package { .. } => "package",
            uze_application::application::DeliveryRoute::CapabilityByCapability { .. } => {
                "per item"
            }
        })
    }));
    rows.push(envelope);
    text.push_str(&progress::aligned_rows(rows));
    text.push_str("\n\n");
    text.push_str(&notes);
    if !report.plugin.undelivered.is_empty() {
        text.push_str(&render_undelivered(std::slice::from_ref(&report.plugin)));
    }
    let state = &report.managed_state;
    let wrong = state.missing + state.drifted + state.conflicts + state.blocked;
    if wrong == 0 {
        text.push_str(&format!(
            "{} {} in place, none drifted\n",
            progress::success_icon(),
            plural(state.matched, "1 file", &format!("{} files", state.matched))
        ));
    } else {
        text.push_str(&format!(
            "{} {} missing, {} drifted, {} in conflict, {} blocked\n",
            progress::error_icon(),
            state.missing,
            state.drifted,
            state.conflicts,
            state.blocked
        ));
    }
    if let Some(error) = &state.ledger_error {
        text.push_str(&format!(
            "{} ledger blocked: {error}\n",
            progress::error_icon()
        ));
    }
    if verbose {
        for delivery in &report.deliveries {
            text.push_str(&render_effective_delivery(delivery, verbose));
        }
    }
    text
}

/// A capability kind as one short word: the matrix row names it beside
/// the capability's own name.
pub(crate) fn capability_word(kind: uze_application::CapabilityKind) -> &'static str {
    use uze_application::CapabilityKind;
    match kind {
        CapabilityKind::Instruction => "instruction",
        CapabilityKind::AgentSkill => "skill",
        CapabilityKind::Mcp => "mcp",
        CapabilityKind::Agent => "agent",
        CapabilityKind::Hook => "hook",
    }
}

/// One harness's effective view: the route the install report would name,
/// then every capability with the name a session sees it under, its route
/// and where it lands, and what a capability short of native loses.
pub(crate) fn render_effective_delivery(
    delivery: &uze_application::application::HarnessDelivery,
    verbose: bool,
) -> String {
    use uze_application::CompatibilityRoute;
    let mut out = String::new();
    let (headline, why) = describe_route(&delivery.route);
    let detected = if delivery.detected {
        String::new()
    } else {
        format!("  {}", progress::label("(not detected)"))
    };
    out.push_str(&format!(
        "\n{}  {headline}{detected}\n",
        progress::title(&delivery.display_name)
    ));
    out.push_str(&format!("  {}\n", progress::label(format!("why: {why}"))));
    for capability in &delivery.capabilities {
        let kind = capability_word(capability.kind);
        let name = capability
            .exposed_name
            .as_deref()
            .unwrap_or(capability.identity.as_str());
        if capability.kind == uze_application::CapabilityKind::Instruction {
            out.push_str(&format!(
                "  {kind:<10}  {name}  {}\n",
                progress::label("through the project context")
            ));
            continue;
        }
        if let Some(reason) = &capability.blocked {
            out.push_str(&format!(
                "  {kind:<10}  {name}  {}\n",
                progress::warning_text(format!("blocked — {reason}"))
            ));
            continue;
        }
        let route = route_word(capability.route);
        let route = if capability.route == CompatibilityRoute::Native {
            progress::success_text(route)
        } else {
            progress::warning_text(route)
        };
        let carrier = if capability.provided_by_package {
            " in the package"
        } else {
            ""
        };
        let location = capability
            .location
            .as_ref()
            .map(|location| format!("  {}", progress::label(location.display().to_string())))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {kind:<10}  {name}  {route}{carrier}{location}\n"
        ));
        if capability.route != CompatibilityRoute::Native || verbose {
            let evidence = if verbose {
                capability.evidence.as_str()
            } else {
                leading_sentence(&capability.evidence)
            };
            out.push_str(&format!("              {}\n", progress::label(evidence)));
        }
    }
    out
}

/// Compact per-harness report for an install/add: one line per harness with
/// its route and — when an attachment was recorded — where. Evidence
/// sentences and full attachment details are `--verbose`-only; `doctor`/
/// `uze inspect` states the same facts read-only. Harness rows carry the
/// human label (`app.integration_label`) — the report's own keys stay the
/// stable ids, which is what `--format json` emits.
/// A capability whose vendor-visible name is held by something UZE does not
/// own is warned about, never raised: the package is installed and its other
/// capabilities are delivered, which is the same shape a failed publication
/// already has (`PublicationOutcome::error`). Silence is what the naming
/// rule forbids — an explicit conflict the operator can act on is what it
/// asks for, and that is a sentence, not an exit code.
pub(crate) fn warn_blocked(report: &AddPluginReport, app: &UzeApplication) {
    for one in &report.blocked {
        progress::warn(&format!(
            "{}: {} was not delivered — {}",
            app.health().integration_label(&one.integration),
            one.capability,
            one.reason
        ));
    }
}

/// An install as a line per package and a closing count, the way a package
/// manager reports one: a harness is named only when it did not receive the
/// whole package. Everything else waits for `--verbose` and `uze inspect`.
pub(crate) fn render_add_summary(report: &AddPluginReport) -> String {
    use progress::Change;
    let commit = report.plugin.commit.as_deref().map(short_commit);
    let mut lines = progress::change(Change::Added, &report.plugin.id, commit);
    let (issues, _) = delivery_issues(&report.deliveries);
    if !issues.is_empty() {
        lines.push('\n');
        lines.push_str(&issues);
    }
    let scope = if report.declared {
        "1 plugin added to this project"
    } else {
        "1 plugin installed on this machine only"
    };
    progress::change_report("install", &lines, scope)
}

/// A commit as a person compares two of them.
pub(crate) fn short_commit(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

/// Each harness that did not receive a package whole, one line per thing
/// it lacks, and how many harnesses the package reached out of those it
/// was delivered to.
pub(crate) fn delivery_issues(
    deliveries: &[uze_application::application::HarnessDeliveryReport],
) -> (String, String) {
    use progress::Change;
    use uze_application::application::HarnessDeliveryOutcome;
    let width = deliveries
        .iter()
        .map(|delivery| delivery.display_name.chars().count())
        .max()
        .unwrap_or_default();
    let mut lines = String::new();
    let mut reached = 0;
    for delivery in deliveries {
        let harness = format!("{:<width$}", delivery.display_name);
        match &delivery.outcome {
            HarnessDeliveryOutcome::Delivered {
                blocked,
                shortfalls,
                ..
            } => {
                reached += 1;
                for one in blocked {
                    lines.push_str(&progress::change(
                        Change::Attention,
                        &harness,
                        Some(&format!("{} not delivered: {}", one.capability, one.reason)),
                    ));
                }
                for one in shortfalls {
                    lines.push_str(&progress::change(
                        Change::Attention,
                        &harness,
                        Some(&format!(
                            "{} {}: {}",
                            one.capability,
                            route_word(one.route),
                            leading_sentence(&one.evidence)
                        )),
                    ));
                }
            }
            HarnessDeliveryOutcome::Failed { .. } => {
                lines.push_str(&progress::change(
                    Change::Failed,
                    &harness,
                    Some("not delivered"),
                ));
            }
        }
    }
    let harnesses = match (reached, deliveries.len()) {
        (_, 0) => "no harness detected".to_owned(),
        (1, 1) => "1 harness".to_owned(),
        (reached, total) if reached == total => format!("{total} harnesses"),
        (reached, total) => format!("{reached} of {total} harnesses"),
    };
    (lines, harnesses)
}

pub(crate) fn render_add_report(report: &AddPluginReport, verbose: bool) -> String {
    let mut out = format!("\n{}", progress::report_section("Delivery"));
    if report.deliveries.is_empty() {
        out.push_str(&format!(
            "  {}\n",
            progress::label("no harness detected; nothing was delivered")
        ));
    }
    out.push_str(&render_deliveries(&report.deliveries, verbose, "  "));
    out
}

/// Every harness a delivery reached or failed on: one header line each —
/// its route — with why that route and every artifact recorded under it.
/// Nothing is collapsed per harness: a package entry and the capabilities
/// delivered beside it are all listed, since a duplicate delivery is
/// exactly what a collapsed report used to hide.
pub(crate) fn render_deliveries(
    deliveries: &[uze_application::application::HarnessDeliveryReport],
    verbose: bool,
    indent: &str,
) -> String {
    use uze_application::application::HarnessDeliveryOutcome;
    let mut out = String::new();
    for delivery in deliveries {
        let harness = progress::title(&delivery.display_name);
        match &delivery.outcome {
            HarnessDeliveryOutcome::Delivered {
                route,
                attachments,
                blocked,
                shortfalls,
            } => {
                let (headline, why) = describe_route(route);
                out.push_str(&format!("{indent}{harness}  {headline}\n"));
                out.push_str(&format!(
                    "{indent}  {}\n",
                    progress::label(format!("why: {why}"))
                ));
                if verbose
                    && let uze_application::application::DeliveryRoute::Package { evidence, .. } =
                        route
                {
                    out.push_str(&format!("{indent}  {}\n", progress::label(evidence)));
                }
                for location in attachments {
                    out.push_str(&format!("{indent}  {}\n", location.display()));
                }
                for one in blocked {
                    out.push_str(&format!(
                        "{indent}  {}\n",
                        progress::warning_text(format!(
                            "{} not delivered — {}",
                            one.capability, one.reason
                        ))
                    ));
                }
                for one in shortfalls {
                    let evidence = if verbose {
                        one.evidence.as_str()
                    } else {
                        leading_sentence(&one.evidence)
                    };
                    out.push_str(&format!(
                        "{indent}  {}\n",
                        progress::warning_text(format!(
                            "{} {} — {evidence}",
                            one.capability,
                            route_word(one.route),
                        ))
                    ));
                }
            }
            HarnessDeliveryOutcome::Failed { error } => {
                out.push_str(&format!(
                    "{indent}{harness}  {}\n",
                    progress::error_text("failed")
                ));
                out.push_str(&format!("{indent}  {error}\n"));
                out.push_str(&format!(
                    "{indent}  {}\n",
                    progress::label("nothing was left attached to this harness")
                ));
            }
        }
    }
    out
}

/// A shortfall's evidence leads with what the capability lost; the rest
/// explains the mechanism and waits for `--verbose`.
pub(crate) fn leading_sentence(evidence: &str) -> &str {
    evidence
        .find(". ")
        .map_or(evidence, |end| &evidence[..=end])
}

/// How a capability's route reads beside it when it is less than native.
pub(crate) fn route_word(route: uze_application::CompatibilityRoute) -> &'static str {
    use uze_application::CompatibilityRoute;
    match route {
        CompatibilityRoute::Native => "native",
        CompatibilityRoute::Adaptable => "adapted",
        CompatibilityRoute::Degraded => "degraded",
        CompatibilityRoute::Unsupported => "unsupported",
    }
}

/// A route as its header reads, and the reason it was taken.
pub(crate) fn describe_route(
    route: &uze_application::application::DeliveryRoute,
) -> (String, String) {
    use uze_application::{PackageEnvelope, application::DeliveryRoute};
    match route {
        DeliveryRoute::Package {
            envelope, route, ..
        } => {
            let route = format!("{route:?}").to_lowercase();
            match envelope {
                PackageEnvelope::Own => (
                    format!("{route} package, its own manifest"),
                    "the plugin ships a manifest this harness reads".to_owned(),
                ),
                PackageEnvelope::Generated => (
                    format!("{route} package, generated manifest"),
                    "the plugin ships no manifest this harness reads, so UZE wrote one from \
                     its capabilities"
                        .to_owned(),
                ),
            }
        }
        DeliveryRoute::CapabilityByCapability { reason } => {
            ("capability by capability".to_owned(), reason.clone())
        }
    }
}

/// The ending an install that stayed installed on some harnesses and not
/// others deserves: the report is on screen, and the exit status says the
/// install did not do everything it was asked.
pub(crate) fn undelivered_failure<'a>(
    undelivered: impl Iterator<
        Item = (
            &'a str,
            &'a uze_application::application::HarnessDeliveryReport,
        ),
    >,
) -> Option<uze_application::UzeError> {
    let lines: Vec<String> = undelivered
        .map(|(package, delivery)| {
            format!(
                "  `{package}` to {}: {}",
                delivery.display_name,
                delivery.error().unwrap_or_default()
            )
        })
        .collect();
    (!lines.is_empty()).then(|| {
        uze_application::UzeError::DeliveryFailed(format!(
            "installed, but not delivered everywhere:\n{}\n`uze status -m` lists each as \
             partially delivered until a later install or update reaches every harness",
            lines.join("\n")
        ))
    })
}

pub(crate) fn render_remove(report: &RemovePluginReport) -> String {
    match report {
        RemovePluginReport::AlreadyAbsent { plugin } => progress::change_report(
            "remove",
            "",
            &format!("nothing to remove: no uze state remains for {plugin}"),
        ),
        RemovePluginReport::Removed { plugin, .. } => progress::change_report(
            "remove",
            &progress::change(progress::Change::Removed, plugin, None),
            "1 plugin removed from this machine",
        ),
        RemovePluginReport::Blocked { report, plan } => {
            let mut text = progress::report_title("Removal blocked", Some(&report.package_id));
            text.push_str(&format!(
                "{}\n\n",
                progress::warning_text(format!("Plan: {plan:?}"))
            ));
            text.push_str(&progress::report_section("Managed state"));
            text.push_str(&format!(
                "{}\n",
                render_managed_state(
                    &report
                        .receipts
                        .iter()
                        .map(|receipt| receipt.inspection.state)
                        .collect::<Vec<_>>(),
                )
            ));
            text
        }
    }
}

/// A project install as a line per plugin it moved, `+` placed, `-` taken
/// out, and one it could not reach named with why — never merely absent,
/// since an environment missing a plugin that says nothing looks complete.
pub(crate) fn render_install(report: &uze_application::application::InstallReport) -> String {
    use progress::Change;
    use uze_application::application::InstallReport;
    match report {
        InstallReport::NoChanges => progress::change_report(
            "install",
            "",
            &format!("{} up to date", progress::success_icon()),
        ),
        InstallReport::NoProject => progress::change_report(
            "install",
            "",
            "no project here, so nothing was declared to install",
        ),
        InstallReport::Installed {
            plugins,
            removed,
            skipped,
            reconciled,
            ..
        } => {
            let mut lines = String::new();
            for plugin in plugins {
                lines.push_str(&progress::change(Change::Added, plugin, None));
            }
            for plugin in removed {
                lines.push_str(&progress::change(Change::Removed, plugin, None));
            }
            for one in skipped {
                lines.push_str(&progress::change(
                    Change::Attention,
                    &one.plugin,
                    Some(&one.reason),
                ));
            }
            if *reconciled {
                lines.push_str(&progress::change(
                    Change::Updated,
                    "AGENTS.md",
                    Some("reconciled"),
                ));
            }
            let mut outcome = Vec::new();
            if !plugins.is_empty() {
                outcome.push(format!("{} installed", count(plugins.len(), "plugin")));
            }
            if !removed.is_empty() {
                outcome.push(format!("{} removed", count(removed.len(), "plugin")));
            }
            if outcome.is_empty() {
                outcome.push(format!("{} up to date", progress::success_icon()));
            }
            progress::change_report("install", &lines, &outcome.join(", "))
        }
    }
}
