//! `source "id" { … }`: who answers a source, what it is asked with, and
//! what asks it again.
//!
//! ```kdl
//! source "board" {
//!     script "board.rhai"
//!     input phase="{phase}"
//!     every "10s"
//!     refresh-on "paths.changed" ".planning"
//! }
//! ```

use std::collections::BTreeMap;
use std::time::Duration;

use kdl::{KdlDocument, KdlNode, KdlValue};
use uze_contract::{Clean, EventName, SourceDecl, SourceKind};

use super::{declared_id, is_relative_inside, relative_file, string_args, template, text};
use crate::capability::extension::document::{
    Place, Report, entries, nearest, refuse_extra, spelled,
};

const CHILDREN: &[&str] = &["script", "run", "fixture", "input", "every", "refresh-on"];
/// What later changes add inside a source, with the change that brings
/// each.
const LATER: &[(&str, u32)] = &[("deliver", 6), ("output", 5)];
/// The events a later change brings, by prefix.
const LATER_EVENTS: &[(&str, u32)] = &[("agent.", 6)];
const ANSWER_IT: &str = "answer it with `script \"….rhai\"` or `fixture \"….json\"`";

/// Where a source's parts were written.
pub struct Places {
    /// Its `script`, `run` or `fixture`.
    pub kind: usize,
    /// Its `every`, when it declares one.
    pub every: Option<usize>,
}

pub fn decode(node: &KdlNode, report: &mut Report<'_>) -> Option<(SourceDecl, Places)> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &[], true, report);
    let id = declared_id(node, parts.args.first().copied(), report);
    let mut kind: Option<(SourceKind, usize)> = None;
    let mut kinds_written = 0;
    let mut input = None;
    let mut every = None;
    let mut refresh_on: Vec<EventName> = Vec::new();
    let children = node.children().map(KdlDocument::nodes).unwrap_or_default();
    for child in children {
        let name = child.name().value();
        match name {
            "script" | "run" | "fixture" => {
                kinds_written += 1;
                if kinds_written > 1 {
                    report.on(
                        child,
                        "a source is answered by one of `script`, `run` or `fixture`",
                        Some("keep one".to_owned()),
                    );
                    continue;
                }
                kind = answer(child, report).map(|answer| (answer, child.offset()));
            }
            "input" if input.is_some() => twice(child, report),
            "input" => input = Some(self::input(child, report)),
            "every" if every.is_some() => twice(child, report),
            "every" => {
                every = Some(self::every(child, report).map(|every| (every, child.offset())))
            }
            "refresh-on" => {
                if let Some(event) = event(child, report) {
                    if refresh_on.iter().any(|seen| seen.name() == event.name()) {
                        report.on(
                            child,
                            format!("the source is refreshed on `{}` twice", event.name()),
                            Some("list every path in one `refresh-on`".to_owned()),
                        );
                    } else {
                        refresh_on.push(event);
                    }
                }
            }
            other => {
                if let Some((_, change)) = LATER.iter().find(|(later, _)| *later == other) {
                    report.on(
                        child,
                        format!("`{other}` arrives in a later change (change {change})"),
                        Some("remove it for now".to_owned()),
                    );
                } else {
                    super::unknown_child(child, "a source", CHILDREN, report);
                }
            }
        }
    }
    if kinds_written == 0 {
        report.on(
            node,
            "this source declares nothing to answer it",
            Some(ANSWER_IT.to_owned()),
        );
    }
    let (kind, kind_at) = kind?;
    let every = every.flatten();
    let mut source = SourceDecl::new(id?, kind);
    source.input = input.unwrap_or_default();
    source.every = every.map(|(every, _)| every);
    source.refresh_on = refresh_on;
    Some((
        source,
        Places {
            kind: kind_at,
            every: every.map(|(_, at)| at),
        },
    ))
}

fn twice(child: &KdlNode, report: &mut Report<'_>) {
    report.on(
        child,
        format!("a source takes one `{}`", child.name().value()),
        Some("keep one".to_owned()),
    );
}

fn answer(child: &KdlNode, report: &mut Report<'_>) -> Option<SourceKind> {
    let at = child.offset();
    match child.name().value() {
        "script" => text(child, report)
            .filter(|file| relative_file(file, "rhai", at, report))
            .map(SourceKind::Script),
        "fixture" => text(child, report)
            .filter(|file| relative_file(file, "json", at, report))
            .map(SourceKind::Fixture),
        _ => {
            let argv: Vec<String> = string_args(child, report)
                .into_iter()
                .map(|(arg, _)| arg)
                .collect();
            (!argv.is_empty()).then_some(SourceKind::Run(argv))
        }
    }
}

/// `input name="text" …`: what the source is asked with, each value text
/// with `{field}` references.
fn input(child: &KdlNode, report: &mut Report<'_>) -> BTreeMap<String, String> {
    let parts = entries(child, report);
    for extra in &parts.args {
        report.on(
            *extra,
            "`input` takes properties, not arguments",
            Some(format!("write `input name={}`", spelled(extra.value()))),
        );
    }
    if let Some(children) = child.children() {
        report.on(
            children,
            "`input` holds no block",
            Some("remove the `{ … }`".to_owned()),
        );
    }
    let mut input = BTreeMap::new();
    for (name, entry) in &parts.props {
        if !uze_contract::binding::is_field_name(name) {
            report.on(*entry, format!("`{name}` is not an input name"), None);
            continue;
        }
        let KdlValue::String(value) = entry.value() else {
            report.on(
                *entry,
                format!("an input is text, not {}", spelled(entry.value())),
                Some(format!("write `{name}=\"{}\"`", entry.value())),
            );
            continue;
        };
        if template(value, entry.offset(), report) {
            input.insert((*name).to_owned(), Clean::text(value).into_string());
        }
    }
    input
}

/// `every "10s"`: a whole number of `ms`, `s`, `m` or `h`. How short is
/// the check's to say, so a too-short one is still read.
fn every(child: &KdlNode, report: &mut Report<'_>) -> Option<Duration> {
    let written = text(child, report)?;
    let parsed = duration(&written);
    if parsed.is_none() {
        report.on(
            child,
            format!("`{written}` is not a duration"),
            Some("write a whole number and a unit, such as `every \"10s\"` or `\"5m\"`".to_owned()),
        );
    }
    parsed
}

pub fn duration(written: &str) -> Option<Duration> {
    let digits = written.find(|ch: char| !ch.is_ascii_digit())?;
    let (number, unit) = written.split_at(digits);
    let number: u64 = number.parse().ok()?;
    let millis = match unit {
        "ms" => Some(number),
        "s" => number.checked_mul(1000),
        "m" => number.checked_mul(60_000),
        "h" => number.checked_mul(3_600_000),
        _ => None,
    }?;
    Some(Duration::from_millis(millis))
}

/// `refresh-on "paths.changed" "dir" …` or `refresh-on "checkout.changed"`.
fn event(child: &KdlNode, report: &mut Report<'_>) -> Option<EventName> {
    let mut args = string_args(child, report).into_iter();
    let (name, at) = args.next()?;
    let rest: Vec<(String, usize)> = args.collect();
    match name.as_str() {
        EventName::PATHS_CHANGED => {
            if rest.is_empty() {
                report.at(
                    at,
                    "`paths.changed` names the paths it watches",
                    Some("write `refresh-on \"paths.changed\" \".planning\"`".to_owned()),
                );
                return None;
            }
            let mut paths = Vec::new();
            for (path, path_at) in rest {
                if is_relative_inside(&path) {
                    paths.push(path);
                } else {
                    report.at(
                        path_at,
                        format!("`{path}` is not a path inside the checkout"),
                        Some("write a relative path with `/`, without `..`".to_owned()),
                    );
                }
            }
            (!paths.is_empty()).then_some(EventName::PathsChanged { paths })
        }
        EventName::CHECKOUT_CHANGED => {
            for (extra, extra_at) in rest {
                report.at(
                    extra_at,
                    "`checkout.changed` takes no paths",
                    Some(format!("remove \"{extra}\"")),
                );
            }
            Some(EventName::CheckoutChanged)
        }
        other => {
            if let Some((_, change)) = LATER_EVENTS
                .iter()
                .find(|(prefix, _)| other.starts_with(prefix))
            {
                report.at(
                    at,
                    format!("`{other}` arrives in a later change (change {change})"),
                    Some("remove it for now".to_owned()),
                );
            } else {
                report.at(
                    at,
                    format!("there is no event `{other}`"),
                    Some(match nearest(other, EventName::NAMES) {
                        Some(near) => format!("did you mean `{near}`?"),
                        None => format!("an event is one of {}", EventName::NAMES.join(", ")),
                    }),
                );
            }
            None
        }
    }
}
