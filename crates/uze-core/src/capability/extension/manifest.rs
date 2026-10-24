//! Decoding `extension.kdl` into [`Manifest`].
//!
//! The manifest's shape is fixed and small, so each node is matched by
//! name and every other spelling is refused, including the nodes later
//! protocols add, which are named as such rather than as typos.

use std::collections::BTreeSet;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use uze_contract::{
    Clean, Manifest, PROTOCOL, RESERVED_IDS, Slot, SourceDecl, SurfaceDecl, is_valid_id,
};

use super::document::{Place, Report, accepts, entries, refuse_extra, spelled};

const NODES: &[&str] = &["protocol", "id", "name", "description", "surface", "source"];
/// What later changes of `add-extension-platform` add to a manifest.
const LATER: &[&str] = &[
    "script",
    "program",
    "action",
    "wants",
    "refresh-on",
    "grant",
];
const SURFACE_PROPS: &[&str] = &["slot", "view", "source", "title"];

/// A manifest, and where in its file each surface and source was written,
/// for what the rest of the check says about them.
pub struct Decoded {
    pub manifest: Manifest,
    /// Parallel to `manifest.surfaces`: the offsets of `view=` and
    /// `source=`.
    pub surface_places: Vec<(usize, usize)>,
    /// Parallel to `manifest.sources`: the offset of its `fixture`.
    pub fixture_places: Vec<usize>,
}

/// Decodes the manifest of the extension found under `extensions/<dir_id>/`.
/// `None` when it declares a protocol this build does not speak: nothing
/// else in it can be read with confidence then.
pub fn decode(document: &KdlDocument, dir_id: &str, report: &mut Report<'_>) -> Option<Decoded> {
    let protocol = protocol(document, report)?;
    let mut decoded = Decoded {
        manifest: Manifest {
            protocol,
            id: String::new(),
            name: String::new(),
            description: String::new(),
            surfaces: Vec::new(),
            sources: Vec::new(),
        },
        surface_places: Vec::new(),
        fixture_places: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for node in document.nodes() {
        let name = node.name().value();
        let scalar = matches!(name, "protocol" | "id" | "name" | "description");
        if scalar && !seen.insert(name) {
            report.on(
                node,
                format!("`{name}` is declared twice"),
                Some("keep one".to_owned()),
            );
            continue;
        }
        match name {
            "protocol" => {
                let parts = entries(node, report);
                refuse_extra(node, &parts, 1, &[], false, report);
            }
            "id" => decoded.manifest.id = id(node, dir_id, report).unwrap_or_default(),
            "name" => {
                decoded.manifest.name = text(node, report)
                    .map(|name| Clean::label(&name).into_string())
                    .unwrap_or_default();
            }
            "description" => {
                decoded.manifest.description = text(node, report)
                    .map(|text| Clean::text(&text).into_string())
                    .unwrap_or_default();
            }
            "surface" => {
                if let Some((surface, places)) = surface(node, report) {
                    decoded.manifest.surfaces.push(surface);
                    decoded.surface_places.push(places);
                }
            }
            "source" => {
                if let Some((source, place)) = source(node, report) {
                    decoded.manifest.sources.push(source);
                    decoded.fixture_places.push(place);
                }
            }
            other => unknown(node, other, report),
        }
    }
    for required in ["id", "name", "description"] {
        if !seen.contains(required) {
            report.whole(
                format!("declares no `{required}`"),
                Some(format!("add `{required} \"…\"`")),
            );
        }
    }
    refuse_repeated_ids(document, report);
    refuse_unknown_sources(&decoded, report);
    Some(decoded)
}

fn protocol(document: &KdlDocument, report: &mut Report<'_>) -> Option<u32> {
    let Some(node) = document
        .nodes()
        .iter()
        .find(|node| node.name().value() == "protocol")
    else {
        report.whole(
            "declares no `protocol`",
            Some(format!("add `protocol {PROTOCOL}` as its first line")),
        );
        return Some(PROTOCOL);
    };
    let declared = node
        .entries()
        .first()
        .filter(|entry| entry.name().is_none());
    match declared.map(KdlEntry::value) {
        Some(KdlValue::Integer(number)) if *number == i128::from(PROTOCOL) => Some(PROTOCOL),
        Some(KdlValue::Integer(number)) => {
            report.on(
                node,
                format!(
                    "declares protocol {number}, and this build of UZE speaks protocol {PROTOCOL}"
                ),
                Some(if *number > i128::from(PROTOCOL) {
                    "update UZE to install this extension".to_owned()
                } else {
                    format!("port the extension to protocol {PROTOCOL}")
                }),
            );
            None
        }
        other => {
            report.on(
                node,
                format!(
                    "`protocol` takes a whole number, not {}",
                    other.map_or_else(|| "nothing".to_owned(), spelled)
                ),
                Some(format!("write `protocol {PROTOCOL}`")),
            );
            Some(PROTOCOL)
        }
    }
}

/// The one string argument of a node that holds nothing else.
fn text(node: &KdlNode, report: &mut Report<'_>) -> Option<String> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &[], false, report);
    string_arg(node, parts.args.first().copied(), report)
}

fn string_arg(node: &KdlNode, arg: Option<&KdlEntry>, report: &mut Report<'_>) -> Option<String> {
    let name = node.name().value();
    match arg.map(KdlEntry::value) {
        Some(KdlValue::String(text)) => Some(text.clone()),
        Some(other) => {
            report.on(
                arg?,
                format!("`{name}` takes a string, not {}", spelled(other)),
                Some(format!("write `{name} \"{other}\"`")),
            );
            None
        }
        None => {
            report.on(
                node,
                format!("`{name}` takes a string"),
                Some(format!("write `{name} \"…\"`")),
            );
            None
        }
    }
}

fn id(node: &KdlNode, dir_id: &str, report: &mut Report<'_>) -> Option<String> {
    let id = text(node, report)?;
    if !is_valid_id(&id) {
        let lowered = id.to_ascii_lowercase();
        report.on(
            node,
            format!(
                "`{id}` is not an extension id: lowercase letters, digits and single hyphens, starting with a letter"
            ),
            is_valid_id(&lowered).then(|| format!("write `id \"{lowered}\"`")),
        );
        return None;
    }
    if RESERVED_IDS.contains(&id.as_str()) {
        report.on(
            node,
            format!("`{id}` is reserved by a built-in extension"),
            Some("choose another id".to_owned()),
        );
        return None;
    }
    if id != dir_id {
        report.on(
            node,
            format!("declares the id `{id}` from the directory `extensions/{dir_id}/`"),
            Some("name the directory after the id".to_owned()),
        );
    }
    Some(id)
}

fn surface(node: &KdlNode, report: &mut Report<'_>) -> Option<(SurfaceDecl, (usize, usize))> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, SURFACE_PROPS, false, report);
    let id = declared_id(node, parts.args.first().copied(), report);
    let prop = |name: &str| {
        parts
            .props
            .iter()
            .find(|(prop, _)| *prop == name)
            .map(|(_, entry)| *entry)
    };
    let required = |name: &str, report: &mut Report<'_>| {
        let entry = prop(name);
        if entry.is_none() {
            report.on(
                node,
                format!("`surface` declares no `{name}=`"),
                Some(format!("add `{name}=\"…\"`")),
            );
        }
        entry.and_then(|entry| string_prop(name, entry, report).map(|text| (text, entry.offset())))
    };
    let slot = required("slot", report);
    let view = required("view", report);
    let source = required("source", report);
    let title = prop("title").and_then(|entry| string_prop("title", entry, report));
    let slot = slot.and_then(|(slot, offset)| {
        let parsed = Slot::parse(&slot);
        if parsed.is_none() {
            report.at(
                offset,
                format!("this version has no slot `{slot}`"),
                Some(format!("a slot is one of {}", Slot::NAMES.join(", "))),
            );
        }
        parsed
    });
    let view = view.filter(|(view, offset)| relative_file(view, "kdl", *offset, report));
    let source = source.filter(|(source, offset)| {
        let valid = is_valid_id(source);
        if !valid {
            report.at(*offset, format!("`{source}` is not a source id"), None);
        }
        valid
    });
    let ((view, view_at), (source, source_at)) = (view?, source?);
    Some((
        SurfaceDecl {
            id: id?,
            slot: slot?,
            view,
            source,
            title: title.map(|title| Clean::label(&title).into_string()),
        },
        (view_at, source_at),
    ))
}

fn source(node: &KdlNode, report: &mut Report<'_>) -> Option<(SourceDecl, usize)> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &[], true, report);
    let id = declared_id(node, parts.args.first().copied(), report);
    let mut fixture = None;
    for child in node.children().map(KdlDocument::nodes).unwrap_or_default() {
        match child.name().value() {
            "fixture" if fixture.is_some() => {
                report.on(
                    child,
                    "a source takes one `fixture`",
                    Some("keep one".to_owned()),
                );
            }
            "fixture" => {
                fixture = text(child, report)
                    .filter(|file| relative_file(file, "json", child.offset(), report))
                    .map(|file| (file, child.offset()));
            }
            other if LATER.contains(&other) || other == "run" => report.on(
                child,
                format!("a source fed by `{other}` is not supported by protocol {PROTOCOL}"),
                Some("feed it a `fixture \"….json\"`".to_owned()),
            ),
            other => report.on(
                child,
                format!("a source holds no `{other}`"),
                Some("feed it a `fixture \"….json\"`".to_owned()),
            ),
        }
    }
    if fixture.is_none()
        && node
            .children()
            .is_none_or(|children| children.nodes().is_empty())
    {
        report.on(
            node,
            "this source declares nothing to feed it",
            Some("add `{ fixture \"fixtures/….json\"; }`".to_owned()),
        );
    }
    let (fixture, place) = fixture?;
    Some((
        SourceDecl {
            id: id?,
            fixture: Some(fixture),
        },
        place,
    ))
}

/// A surface's or a source's own id, its first argument.
fn declared_id(node: &KdlNode, arg: Option<&KdlEntry>, report: &mut Report<'_>) -> Option<String> {
    let id = string_arg(node, arg, report)?;
    if is_valid_id(&id) {
        return Some(id);
    }
    report.on(
        arg?,
        format!(
            "`{id}` is not an id: lowercase letters, digits and single hyphens, starting with a letter"
        ),
        None,
    );
    None
}

fn string_prop(name: &str, entry: &KdlEntry, report: &mut Report<'_>) -> Option<String> {
    match entry.value() {
        KdlValue::String(text) => Some(text.clone()),
        other => {
            report.on(
                entry,
                format!("`{name}` takes a string, not {}", spelled(other)),
                Some(format!("write `{name}=\"{other}\"`")),
            );
            None
        }
    }
}

/// Whether `file` names a file inside the extension's directory with the
/// `extension` it must have. Reports why not.
pub fn relative_file(file: &str, extension: &str, offset: usize, report: &mut Report<'_>) -> bool {
    let inside = !file.is_empty()
        && !file.contains('\\')
        && !file.contains(':')
        && std::path::Path::new(file)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    if !inside {
        report.at(
            offset,
            format!("`{file}` is not a path inside the extension's directory"),
            Some("write a relative path with `/`, without `..`".to_owned()),
        );
        return false;
    }
    if !file.ends_with(&format!(".{extension}")) {
        report.at(
            offset,
            format!("`{file}` is not a `.{extension}` file"),
            None,
        );
        return false;
    }
    true
}

fn unknown(node: &KdlNode, name: &str, report: &mut Report<'_>) {
    if LATER.contains(&name) {
        report.on(
            node,
            format!("`{name}` is not supported by protocol {PROTOCOL}"),
            Some("remove it: this version draws from fixtures and runs nothing".to_owned()),
        );
        return;
    }
    let case = NODES.iter().find(|known| known.eq_ignore_ascii_case(name));
    report.on(
        node,
        format!("a manifest has no `{name}`"),
        Some(match case {
            Some(known) => format!("manifest nodes are lowercase: write `{known}`"),
            None => format!(
                "a manifest declares {}",
                accepts(NODES).trim_start_matches("it takes ")
            ),
        }),
    );
}

fn refuse_repeated_ids(document: &KdlDocument, report: &mut Report<'_>) {
    for kind in ["surface", "source"] {
        let mut seen = BTreeSet::new();
        for node in document
            .nodes()
            .iter()
            .filter(|node| node.name().value() == kind)
        {
            let Some(KdlValue::String(id)) = node.entries().first().map(KdlEntry::value) else {
                continue;
            };
            if !seen.insert(id.as_str()) {
                report.on(node, format!("the {kind} `{id}` is declared twice"), None);
            }
        }
    }
}

fn refuse_unknown_sources(decoded: &Decoded, report: &mut Report<'_>) {
    let declared: Vec<&str> = decoded
        .manifest
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .collect();
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
                        "declare `source \"{}\" {{ fixture \"….json\"; }}`",
                        surface.source
                    )
                } else {
                    format!("declared sources: {}", declared.join(", "))
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::extension::Diagnostic;

    pub(crate) const GSD: &str = r#"// extension.kdl
protocol 1
id "gsd-band"
name "GSD band"
description "O milestone GSD do checkout"
surface "board" slot="view" view="board.kdl" source="board"
surface "summary" slot="section" view="board.kdl" source="board" title="GSD"
source "board" { fixture "fixtures/board.json"; }
"#;

    fn decode_text(source: &str) -> (Option<Decoded>, Vec<Diagnostic>) {
        let document: KdlDocument = source.parse().expect("valid KDL");
        let mut report = Report::new("extension.kdl", source);
        let decoded = decode(&document, "gsd-band", &mut report);
        (decoded, report.into_diagnostics())
    }

    fn refusals(source: &str) -> Vec<Diagnostic> {
        let (_, diagnostics) = decode_text(source);
        assert!(!diagnostics.is_empty(), "{source} was accepted");
        diagnostics
    }

    #[test]
    fn the_gsd_band_manifest_decodes() {
        let (decoded, diagnostics) = decode_text(GSD);
        assert_eq!(diagnostics, vec![]);
        let manifest = decoded.unwrap().manifest;
        assert_eq!(manifest.protocol, 1);
        assert_eq!(manifest.id, "gsd-band");
        assert_eq!(manifest.name, "GSD band");
        assert_eq!(manifest.surfaces.len(), 2);
        let summary = manifest.surface("summary").unwrap();
        assert_eq!(summary.slot, Slot::Section);
        assert_eq!(summary.view, "board.kdl");
        assert_eq!(summary.title.as_deref(), Some("GSD"));
        assert_eq!(manifest.surface("board").unwrap().title, None);
        assert_eq!(
            manifest.source("board").unwrap().fixture.as_deref(),
            Some("fixtures/board.json")
        );
    }

    #[test]
    fn an_unknown_protocol_names_both_versions_and_stops() {
        let (decoded, diagnostics) = decode_text(&GSD.replace("protocol 1", "protocol 2"));
        assert!(decoded.is_none());
        assert_eq!(diagnostics.len(), 1);
        let message = &diagnostics[0].message;
        assert!(
            message.contains("protocol 2") && message.contains("protocol 1"),
            "{message}"
        );
        assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 1));
    }

    #[test]
    fn a_reserved_or_malformed_id_is_refused() {
        let reserved = refusals(&GSD.replace("\"gsd-band\"", "\"code\""));
        assert!(
            reserved[0]
                .message
                .contains("`code` is reserved by a built-in extension")
        );
        let malformed = refusals(&GSD.replace("\"gsd-band\"", "\"GSD-band\""));
        assert!(malformed[0].message.contains("is not an extension id"));
        assert_eq!(
            malformed[0].suggestion.as_deref(),
            Some("write `id \"gsd-band\"`")
        );
        let elsewhere = refusals(&GSD.replace("\"gsd-band\"", "\"other\""));
        assert!(elsewhere[0].message.contains("extensions/gsd-band/"));
    }

    #[test]
    fn a_surface_is_checked_property_by_property() {
        let diagnostics = refusals(&GSD.replace(
            r#"surface "board" slot="view" view="board.kdl" source="board""#,
            r#"surface "board" slot="band" view="../board.kdl" source="board" colour="red""#,
        ));
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "`surface` has no property `colour`",
                "this version has no slot `band`",
                "`../board.kdl` is not a path inside the extension's directory",
            ]
        );
        let unknown = refusals(&GSD.replace(r#"source="board" title"#, r#"source="feed" title"#));
        assert!(
            unknown[0].message.contains("`feed`, which is not declared"),
            "{}",
            unknown[0]
        );
        assert_eq!(
            unknown[0].suggestion.as_deref(),
            Some("declared sources: board")
        );
    }

    #[test]
    fn a_missing_part_and_a_repeated_one_are_named() {
        let diagnostics = refusals(&GSD.replace("name \"GSD band\"\n", "id \"gsd-band\"\n"));
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.contains(&"`id` is declared twice"), "{messages:?}");
        assert!(messages.contains(&"declares no `name`"), "{messages:?}");
        let twice = refusals(&format!(
            "{GSD}source \"board\" {{ fixture \"b.json\"; }}\n"
        ));
        assert!(
            twice[0]
                .message
                .contains("source `board` is declared twice")
        );
    }

    #[test]
    fn later_protocols_nodes_and_the_wrong_case_are_told_apart() {
        let later = refusals(&format!("{GSD}action \"next\"\n"));
        assert!(later[0].message.contains("not supported by protocol 1"));
        let case = refusals(&format!("{GSD}Surface \"x\"\n"));
        assert_eq!(
            case[0].suggestion.as_deref(),
            Some("manifest nodes are lowercase: write `surface`")
        );
        let script =
            refusals(&GSD.replace("fixture \"fixtures/board.json\"", "script \"board.rhai\""));
        assert!(
            script[0].message.contains("`script` is not supported"),
            "{}",
            script[0]
        );
    }
}
