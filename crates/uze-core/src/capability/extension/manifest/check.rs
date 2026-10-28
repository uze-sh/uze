//! What one declaration of a manifest says about another, checked once
//! every one is read: the sources and surfaces they name, the grants what
//! they do needs, and the limits on what would run.

use std::time::Duration;

use uze_contract::{Does, EVERY_FLOOR, Grant, Manifest, Program, SourceKind};

use super::Decoded;
use crate::capability::extension::document::Report;

pub fn references(decoded: &Decoded, report: &mut Report<'_>) {
    surfaces_read_declared_sources(decoded, report);
    bar_items_open_a_surface(decoded, report);
    sources_run_what_this_build_runs(decoded, report);
    actions_name_what_is_declared(decoded, report);
    programs_are_granted(decoded, report);
    secrets_are_sent_somewhere(decoded, report);
}

/// A secret is only ever placed in a request `fetch` makes, so one kept by
/// an extension that may reach no host would be asked of the operator for
/// nothing.
fn secrets_are_sent_somewhere(decoded: &Decoded, report: &mut Report<'_>) {
    let manifest = &decoded.manifest;
    if manifest.hosts().next().is_some() {
        return;
    }
    for secret in manifest.preferences.iter().filter(|decl| decl.is_secret()) {
        report.whole(
            format!(
                "the secret `{}` is never used: only `fetch` sends one, and the manifest wants no `net:<host>`",
                secret.name
            ),
            Some("add the host the secret is sent to, such as \"net:api.github.com\", to `wants`".to_owned()),
        );
    }
}

/// The shortest interval a source answered by a program is asked at: each
/// answer is a process, where a script's is a call.
pub const PROGRAM_EVERY_FLOOR: Duration = Duration::from_secs(30);

/// The programs no extension may be granted, whatever it says: UZE itself,
/// whose every verb would run as the operator.
const NEVER_GRANTED: &[&str] = &["uze"];

fn programs_are_granted(decoded: &Decoded, report: &mut Report<'_>) {
    let manifest = &decoded.manifest;
    for grant in &manifest.wants {
        if let Some(program) = grant.program()
            && NEVER_GRANTED.contains(&program)
        {
            report.whole(
                format!("`{grant}` is never granted: it would run UZE itself as you"),
                Some(format!("remove \"{grant}\" from `wants`")),
            );
        }
    }
    let sources = manifest
        .sources
        .iter()
        .zip(&decoded.source_places)
        .filter_map(|(source, places)| match &source.kind {
            SourceKind::Run(program) => {
                Some((format!("the source `{}`", source.id), program, places.kind))
            }
            _ => None,
        });
    let actions = manifest
        .actions
        .iter()
        .zip(&decoded.action_places)
        .filter_map(|(action, places)| match &action.does {
            Does::Run(program) => {
                Some((format!("the action `{}`", action.id), program, places.does))
            }
            _ => None,
        });
    for (who, program, at) in sources.chain(actions) {
        program_is_granted(manifest, &who, program, at, report);
        substituted_values_follow_a_double_dash(&who, program, at, report);
    }
    for (decl, at) in manifest.mcp.iter().zip(&decoded.mcp_places) {
        let called = manifest.wants.iter().any(|grant| {
            grant
                .mcp_tool()
                .is_some_and(|(server, _)| server == decl.server)
        });
        if !called {
            report.at(
                *at,
                format!(
                    "`mcp \"{}\"` names a server no tool of which the manifest wants",
                    decl.server
                ),
                Some(format!("add \"mcp:{}/<tool>\" to `wants`", decl.server)),
            );
        }
    }
    for (decl, at) in manifest.exec.iter().zip(&decoded.exec_places) {
        if !manifest.wants(&Grant::Exec(decl.name.clone())) {
            report.at(
                *at,
                format!(
                    "`exec \"{}\"` names a program the manifest does not want",
                    decl.name
                ),
                Some(format!("add \"exec:{}\" to `wants`", decl.name)),
            );
        }
    }
}

fn program_is_granted(
    manifest: &Manifest,
    who: &str,
    program: &Program,
    at: usize,
    report: &mut Report<'_>,
) {
    if program.is_packaged() {
        return;
    }
    let grant = Grant::Exec(program.program().to_owned());
    if !manifest.wants(&grant) {
        report.at(
            at,
            format!("{who} runs `{}`, which needs `{grant}`", program.program()),
            Some(format!("add \"{grant}\" to `wants`")),
        );
    }
}

/// A value substituted into an argv is one whole argument, and one that
/// begins with `-` would be read as a flag: an argument that begins with a
/// `{field}` is written after `--`.
fn substituted_values_follow_a_double_dash(
    who: &str,
    program: &Program,
    at: usize,
    report: &mut Report<'_>,
) {
    let mut after_dashes = false;
    for arg in program.argv.iter().skip(1) {
        if arg == "--" {
            after_dashes = true;
        } else if arg.starts_with('{') && !after_dashes {
            report.at(
                at,
                format!("{who} passes `{arg}` as an argument before `--`, where a value beginning with `-` would be a flag"),
                Some(format!("write \"--\" before \"{arg}\"")),
            );
        }
    }
}

fn surfaces_read_declared_sources(decoded: &Decoded, report: &mut Report<'_>) {
    let declared = ids(decoded.manifest.sources.iter().map(|source| &source.id));
    for (surface, (_, source_at)) in decoded
        .manifest
        .surfaces
        .iter()
        .zip(&decoded.surface_places)
    {
        if decoded.manifest.source(&surface.source).is_none() {
            report.at(
                *source_at,
                format!(
                    "the surface `{}` reads the source `{}`, which is not declared",
                    surface.id, surface.source
                ),
                Some(if declared.is_empty() {
                    format!(
                        "declare `source \"{}\" {{ script \"….rhai\"; }}`",
                        surface.source
                    )
                } else {
                    format!("declared sources: {declared}")
                }),
            );
        }
    }
}

/// A bar item's `opens` names a view or a drawer of the same extension:
/// the surfaces a click on it brings up.
fn bar_items_open_a_surface(decoded: &Decoded, report: &mut Report<'_>) {
    use uze_contract::Slot;
    let manifest = &decoded.manifest;
    let openable = ids(manifest
        .surfaces
        .iter()
        .filter(|surface| matches!(surface.slot, Slot::View | Slot::Drawer))
        .map(|surface| &surface.id));
    for (surface, (view_at, _)) in manifest.surfaces.iter().zip(&decoded.surface_places) {
        let Some(opens) = surface.bar.as_ref().and_then(|bar| bar.opens.as_ref()) else {
            continue;
        };
        let opened = manifest.surface(opens);
        if !opened.is_some_and(|opened| matches!(opened.slot, Slot::View | Slot::Drawer)) {
            report.at(
                *view_at,
                format!(
                    "the bar item `{}` opens `{opens}`, which is no view or drawer of this extension",
                    surface.id
                ),
                Some(if openable.is_empty() {
                    "declare a `view` or `drawer` surface for it to open".to_owned()
                } else {
                    format!("surfaces it can open: {openable}")
                }),
            );
        }
    }
}

fn sources_run_what_this_build_runs(decoded: &Decoded, report: &mut Report<'_>) {
    for (source, places) in decoded.manifest.sources.iter().zip(&decoded.source_places) {
        let observes = matches!(source.kind, SourceKind::Agents)
            || source
                .refresh_on
                .iter()
                .any(|event| matches!(event, uze_contract::EventName::Agent(_)));
        if observes && !decoded.manifest.wants(&Grant::AgentObserve) {
            report.at(
                places.kind,
                format!(
                    "the source `{}` reads the workspace's agents, which needs `agent:observe`",
                    source.id
                ),
                Some("add \"agent:observe\" to `wants`".to_owned()),
            );
        }
        if matches!(source.kind, SourceKind::Resident(_))
            && !decoded.manifest.wants(&Grant::Resident)
        {
            report.at(
                places.kind,
                format!(
                    "the source `{}` keeps its script alive, which needs `resident`",
                    source.id
                ),
                Some("add \"resident\" to `wants`".to_owned()),
            );
        }
        let runs = matches!(source.kind, SourceKind::Run(_));
        let floor = if runs {
            PROGRAM_EVERY_FLOOR
        } else {
            EVERY_FLOOR
        };
        if let (Some(every), Some(every_at)) = (source.every, places.every)
            && every < floor
        {
            report.at(
                every_at,
                format!(
                    "`every` is at least {}s{}, so a source is not asked more often than that",
                    floor.as_secs(),
                    if runs { " for a program" } else { "" },
                ),
                Some(format!("write `every \"{}s\"`", floor.as_secs())),
            );
        }
        if source.deliver == uze_contract::Deliver::Each && source.refresh_on.is_empty() {
            report.at(
                places.kind,
                format!(
                    "the source `{}` is delivered `each` event, and no event refreshes it",
                    source.id
                ),
                Some("add a `refresh-on`, or remove `deliver`".to_owned()),
            );
        }
        if !runs
            && source.output != uze_contract::Output::View
            && let Some(output_at) = places.output
        {
            report.at(
                output_at,
                "`output` says how a program's output is read, and this source runs no program",
                Some("remove `output`".to_owned()),
            );
        }
    }
}

/// Why an action's key may not be a bare letter, when it may not: one
/// stray keystroke must not be what asks before it runs or what reaches an
/// agent (A4, R-U7).
fn needs_a_modifier(action: &uze_contract::ActionDecl) -> Option<&'static str> {
    match &action.does {
        _ if action.confirm.is_some() => Some("asks before it runs"),
        Does::AgentPrompt { .. } | Does::AgentRestart { .. } | Does::FocusAgent { .. } => {
            Some("reaches an agent")
        }
        _ => None,
    }
}

fn actions_name_what_is_declared(decoded: &Decoded, report: &mut Report<'_>) {
    let manifest = &decoded.manifest;
    let surfaces = ids(manifest.surfaces.iter().map(|surface| &surface.id));
    let sources = ids(manifest.sources.iter().map(|source| &source.id));
    for (action, places) in manifest.actions.iter().zip(&decoded.action_places) {
        if manifest.surface(&action.on).is_none() {
            report.at(
                places.on,
                format!(
                    "the action `{}` is offered on the surface `{}`, which is not declared",
                    action.id, action.on
                ),
                declared(&surfaces, "surfaces"),
            );
        }
        if let Some(why) = needs_a_modifier(action)
            && let (Some(key), Some(key_at)) = (&action.key, places.key)
            && key.is_bare()
        {
            report.at(
                key_at,
                format!(
                    "the action `{}` {why}, so its key needs a modifier, not a bare `{key}`",
                    action.id
                ),
                Some(format!("write `key=\"ctrl-{key}\"`")),
            );
        }
        match &action.does {
            Does::Refresh(refreshed) => {
                for (source, at) in refreshed.iter().zip(&places.targets) {
                    if manifest.source(source).is_none() {
                        report.at(
                            *at,
                            format!(
                                "the action `{}` refreshes the source `{source}`, which is not declared",
                                action.id
                            ),
                            declared(&sources, "sources"),
                        );
                    }
                }
            }
            Does::Open(surface) if manifest.surface(surface).is_none() => {
                report.at(
                    places.targets.first().copied().unwrap_or(places.does),
                    format!(
                        "the action `{}` opens the surface `{surface}`, which is not declared",
                        action.id
                    ),
                    declared(&surfaces, "surfaces"),
                );
            }
            _ => {}
        }
        for grant in action.needs() {
            if manifest.wants(&grant) {
                continue;
            }
            report.at(
                places.does,
                format!(
                    "the action `{}` needs `{grant}`, which the manifest does not want",
                    action.id
                ),
                Some(format!("add \"{grant}\" to `wants`")),
            );
        }
    }
}

fn ids<'a>(ids: impl Iterator<Item = &'a String>) -> String {
    ids.map(String::as_str).collect::<Vec<_>>().join(", ")
}

fn declared(listed: &str, kind: &str) -> Option<String> {
    (!listed.is_empty()).then(|| format!("declared {kind}: {listed}"))
}
