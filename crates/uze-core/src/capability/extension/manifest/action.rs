//! `action "id" label="…" on="surface" { … }`: what an extension offers the
//! operator to do, and the one thing it does.
//!
//! ```kdl
//! action "refresh" label="Refresh" on="board" key="ctrl-r" { refresh "board"; }
//! action "archive" label="Archive" on="board" key="ctrl-d" confirm="Archive {name}?" {
//!     script "archive.rhai"
//! }
//! ```

use kdl::{KdlDocument, KdlNode};
use uze_contract::{ActionDecl, Clean, Does, KeySuggestion};

use super::{
    declared_id, prop, referred_id, relative_file, required_prop, string_args, string_prop,
    template, text,
};
use crate::capability::extension::document::{Place, Report, entries, refuse_extra};

const PROPS: &[&str] = &["label", "on", "key", "confirm"];

/// Where an action's parts were written.
pub struct Places {
    /// Its `on=`.
    pub on: usize,
    /// Its `key=`, when it suggests one.
    pub key: Option<usize>,
    /// The node that says what it does.
    pub does: usize,
    /// Parallel to the ids `does` names: each `refresh` source, or the
    /// surface `open` names.
    pub targets: Vec<usize>,
}

pub fn decode(node: &KdlNode, report: &mut Report<'_>) -> Option<(ActionDecl, Places)> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, PROPS, true, report);
    let id = declared_id(node, parts.args.first().copied(), report);
    let label = required_prop(node, &parts, "label", report);
    let on = required_prop(node, &parts, "on", report)
        .filter(|(on, at)| referred_id(on, "surface", *at, report));
    let key = prop(&parts, "key").and_then(|entry| {
        let written = string_prop("key", entry, report)?;
        let parsed = KeySuggestion::parse(&written);
        if parsed.is_none() {
            report.on(
                entry,
                format!("`{written}` is not a key"),
                Some(
                    "write modifiers then the key, joined by `-`: `ctrl-r`, `alt-shift-x`, `f5`"
                        .to_owned(),
                ),
            );
        }
        parsed.map(|key| (key, entry.offset()))
    });
    let confirm = prop(&parts, "confirm").and_then(|entry| {
        string_prop("confirm", entry, report)
            .filter(|question| template(question, entry.offset(), report))
    });
    let mut does: Option<(Does, usize, Vec<usize>)> = None;
    let mut written = 0;
    for child in node.children().map(KdlDocument::nodes).unwrap_or_default() {
        if !Does::NAMES.contains(&child.name().value()) {
            super::unknown_child(child, "an action", Does::NAMES, report);
            continue;
        }
        written += 1;
        if written > 1 {
            report.on(
                child,
                "an action does one thing",
                Some("keep one, or declare another action".to_owned()),
            );
            continue;
        }
        does = what(child, report).map(|(what, targets)| (what, child.offset(), targets));
    }
    if written == 0 {
        report.on(
            node,
            "this action declares nothing to do",
            Some(format!(
                "add one of {} in a `{{ … }}`",
                Does::NAMES
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        );
    }
    let (does, does_at, targets) = does?;
    let (label, _) = label?;
    let (on, on_at) = on?;
    Some((
        ActionDecl {
            id: id?,
            label: Clean::label(&label).into_string(),
            on,
            key: key.as_ref().map(|(key, _)| key.clone()),
            confirm: confirm.map(|question| Clean::label(&question).into_string()),
            does,
        },
        Places {
            on: on_at,
            key: key.map(|(_, at)| at),
            does: does_at,
            targets,
        },
    ))
}

fn what(child: &KdlNode, report: &mut Report<'_>) -> Option<(Does, Vec<usize>)> {
    let at = child.offset();
    match child.name().value() {
        "script" => text(child, report)
            .filter(|file| relative_file(file, "rhai", at, report))
            .map(|file| (Does::Script(file), Vec::new())),
        "refresh" => {
            let ids: Vec<(String, usize)> = string_args(child, report)
                .into_iter()
                .filter(|(id, id_at)| referred_id(id, "source", *id_at, report))
                .collect();
            if ids.is_empty() {
                return None;
            }
            let (ids, places) = ids.into_iter().unzip();
            Some((Does::Refresh(ids), places))
        }
        "clipboard" => text(child, report)
            .filter(|copied| template(copied, at, report))
            .map(|copied| {
                (
                    Does::Clipboard(Clean::text(&copied).into_string()),
                    Vec::new(),
                )
            }),
        "open-path" => text(child, report)
            .filter(|path| template(path, at, report))
            .map(|path| {
                (
                    Does::OpenPath(Clean::label(&path).into_string()),
                    Vec::new(),
                )
            }),
        _ => {
            let parts = entries(child, report);
            refuse_extra(child, &parts, 1, &[], false, report);
            let arg = parts.args.first().copied();
            let surface = super::string_arg(child, arg, report)?;
            let surface_at = arg.map_or(at, Place::offset);
            referred_id(&surface, "surface", surface_at, report)
                .then(|| (Does::Open(surface), vec![surface_at]))
        }
    }
}
