//! What one declaration of a manifest says about another, checked once
//! every one is read: the sources and surfaces they name, the grants what
//! they do needs, and the limits on what would run.

use uze_contract::{Does, EVERY_FLOOR};

use super::Decoded;
use crate::capability::extension::document::Report;

pub fn references(decoded: &Decoded, report: &mut Report<'_>) {
    surfaces_read_declared_sources(decoded, report);
    sources_run_what_this_build_runs(decoded, report);
    actions_name_what_is_declared(decoded, report);
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

fn sources_run_what_this_build_runs(decoded: &Decoded, report: &mut Report<'_>) {
    for (source, places) in decoded.manifest.sources.iter().zip(&decoded.source_places) {
        if matches!(source.kind, uze_contract::SourceKind::Run(_)) {
            report.at(
                places.kind,
                "programs arrive in a later change (change 5)",
                Some("answer the source with `script \"….rhai\"`".to_owned()),
            );
        }
        if let (Some(every), Some(every_at)) = (source.every, places.every)
            && every < EVERY_FLOOR
        {
            report.at(
                every_at,
                format!(
                    "`every` is at least {}s, so a source is not asked more often than that",
                    EVERY_FLOOR.as_secs()
                ),
                Some(format!("write `every \"{}s\"`", EVERY_FLOOR.as_secs())),
            );
        }
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
        if action.confirm.is_some()
            && let (Some(key), Some(key_at)) = (&action.key, places.key)
            && key.is_bare()
        {
            report.at(
                key_at,
                format!(
                    "the action `{}` asks before it runs, so its key needs a modifier, not a bare `{key}`",
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
        if let Some(grant) = action.does.needs()
            && !manifest.wants(grant)
        {
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
