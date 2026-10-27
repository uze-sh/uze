//! Decoding `extension.kdl` into [`Manifest`], and checking what it says.
//!
//! The manifest's shape is fixed and small, so each node is matched by
//! name and every other spelling is refused, including the nodes later
//! changes add, which are named as such rather than as typos.
//!
//! - [`source`]: a `source` and what answers and refreshes it.
//! - [`action`]: an `action` and what it does.
//! - [`grants`]: `wants`, and the grants this build does not offer yet.
//! - [`check`]: what one declaration says about another, once all are read.

mod action;
mod check;
mod grants;
mod source;
pub mod sprite;

use std::collections::BTreeSet;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use uze_contract::binding::parse_text;
use uze_contract::{
    Clean, Does, ExecDecl, Manifest, PROTOCOL, Program, RESERVED_IDS, Slot, SourceKind,
    SurfaceDecl, is_program_name, is_valid_id,
};

use super::document::{Place, Report, accepts, entries, nearest, refuse_extra, spelled};

const NODES: &[&str] = &[
    "protocol",
    "id",
    "name",
    "description",
    "wants",
    "surface",
    "source",
    "action",
    "exec",
    "preferences",
    "sprite",
];
/// What later changes of `add-extension-platform` add to a manifest, with
/// the change that brings each.
const LATER: &[(&str, u32)] = &[];
const SURFACE_PROPS: &[&str] = &[
    "slot",
    "view",
    "source",
    "title",
    "rows",
    "place",
    "applies-when",
];

/// A manifest, and where in its file each declaration was written, for
/// what the rest of the check says about them.
pub struct Decoded {
    pub manifest: Manifest,
    /// Parallel to `manifest.surfaces`: the offsets of `view=` and
    /// `source=`.
    pub surface_places: Vec<(usize, usize)>,
    /// Parallel to `manifest.sources`.
    pub source_places: Vec<source::Places>,
    /// Parallel to `manifest.actions`.
    pub action_places: Vec<action::Places>,
    /// Parallel to `manifest.exec`: where each `exec` was written.
    pub exec_places: Vec<usize>,
    /// Parallel to `manifest.sprites`: where each `sprite` was written.
    pub sprite_places: Vec<usize>,
}

impl Decoded {
    /// Each file the manifest names for something to read or run, what it
    /// is, and the offset it was written at: fixtures and scripts, which
    /// must exist inside the extension's directory.
    pub fn files(&self) -> Vec<(&'static str, &str, usize)> {
        let sources = self
            .manifest
            .sources
            .iter()
            .zip(&self.source_places)
            .filter_map(|(source, places)| match &source.kind {
                SourceKind::Fixture(file) => Some(("fixture", file.as_str(), places.kind)),
                SourceKind::Script(file) => Some(("script", file.as_str(), places.kind)),
                SourceKind::Run(program) => {
                    packaged(program).map(|file| ("program", file, places.kind))
                }
                SourceKind::Resident(file) => Some(("script", file.as_str(), places.kind)),
                SourceKind::Events(_) | SourceKind::Agents => None,
            });
        let actions = self
            .manifest
            .actions
            .iter()
            .zip(&self.action_places)
            .filter_map(|(action, places)| match &action.does {
                Does::Script(file) => Some(("script", file.as_str(), places.does)),
                Does::Run(program) => packaged(program).map(|file| ("program", file, places.does)),
                _ => None,
            });
        let sheets = self
            .manifest
            .sprites
            .iter()
            .zip(&self.sprite_places)
            .map(|(sprite, at)| ("sprite sheet", sprite.sheet.as_str(), *at));
        sources.chain(actions).chain(sheets).collect()
    }
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
            wants: Vec::new(),
            surfaces: Vec::new(),
            sources: Vec::new(),
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
        },
        surface_places: Vec::new(),
        source_places: Vec::new(),
        action_places: Vec::new(),
        exec_places: Vec::new(),
        sprite_places: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for node in document.nodes() {
        let name = node.name().value();
        let scalar = matches!(
            name,
            "protocol" | "id" | "name" | "description" | "wants" | "preferences"
        );
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
            "wants" => decoded.manifest.wants = grants::wants(node, report),
            "surface" => {
                if let Some((surface, places)) = surface(node, report) {
                    decoded.manifest.surfaces.push(surface);
                    decoded.surface_places.push(places);
                }
            }
            "source" => {
                if let Some((source, places)) = source::decode(node, report) {
                    decoded.manifest.sources.push(source);
                    decoded.source_places.push(places);
                }
            }
            "action" => {
                if let Some((action, places)) = action::decode(node, report) {
                    decoded.manifest.actions.push(action);
                    decoded.action_places.push(places);
                }
            }
            "preferences" => decoded.manifest.preferences = preferences(node, report),
            "sprite" => {
                if let Some(sprite) = sprite::decode(node, report) {
                    if decoded.manifest.sprite(&sprite.id).is_some() {
                        report.on(
                            node,
                            format!("the sprite `{}` is declared twice", sprite.id),
                            None,
                        );
                    } else {
                        decoded.sprite_places.push(node.offset());
                        decoded.manifest.sprites.push(sprite);
                    }
                }
            }
            "exec" => {
                if let Some(decl) = exec(node, report) {
                    if decoded
                        .manifest
                        .exec
                        .iter()
                        .any(|seen| seen.name == decl.name)
                    {
                        report.on(
                            node,
                            format!("`exec \"{}\"` is declared twice", decl.name),
                            Some("list every path it reads in one `exec`".to_owned()),
                        );
                    } else {
                        decoded.manifest.exec.push(decl);
                        decoded.exec_places.push(node.offset());
                    }
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
    check::references(&decoded, report);
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

/// Every argument of a node that takes one string or more, in order, and
/// nothing else.
fn string_args(node: &KdlNode, report: &mut Report<'_>) -> Vec<(String, usize)> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, usize::MAX, &[], false, report);
    if parts.args.is_empty() {
        let name = node.name().value();
        report.on(
            node,
            format!("`{name}` takes at least one string"),
            Some(format!("write `{name} \"…\"`")),
        );
    }
    parts
        .args
        .iter()
        .filter_map(|arg| string_arg(node, Some(arg), report).map(|text| (text, arg.offset())))
        .collect()
}

fn id(node: &KdlNode, dir_id: &str, report: &mut Report<'_>) -> Option<String> {
    let id = text(node, report)?;
    if !is_valid_id(&id) {
        let lowered = id.to_ascii_lowercase();
        report.on(
            node,
            format!(
                "`{id}` is not an extension id: lowercase letters, digits and single hyphens, starting with a letter, at most {} long",
                uze_contract::ID_MAX
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

/// The value of property `name` of an entry list, when written.
fn prop<'n>(parts: &super::document::Entries<'n>, name: &str) -> Option<&'n KdlEntry> {
    parts
        .props
        .iter()
        .find(|(prop, _)| *prop == name)
        .map(|(_, entry)| *entry)
}

/// A string property that must be written, with its offset.
fn required_prop(
    node: &KdlNode,
    parts: &super::document::Entries<'_>,
    name: &str,
    report: &mut Report<'_>,
) -> Option<(String, usize)> {
    let entry = prop(parts, name);
    if entry.is_none() {
        report.on(
            node,
            format!("`{}` declares no `{name}=`", node.name().value()),
            Some(format!("add `{name}=\"…\"`")),
        );
    }
    entry.and_then(|entry| string_prop(name, entry, report).map(|text| (text, entry.offset())))
}

fn surface(node: &KdlNode, report: &mut Report<'_>) -> Option<(SurfaceDecl, (usize, usize))> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, SURFACE_PROPS, false, report);
    let id = declared_id(node, parts.args.first().copied(), report);
    let slot = required_prop(node, &parts, "slot", report);
    let view = required_prop(node, &parts, "view", report);
    let source = required_prop(node, &parts, "source", report);
    let title = prop(&parts, "title").and_then(|entry| string_prop("title", entry, report));
    let applies_when = match prop(&parts, "applies-when") {
        None => Some(None),
        Some(entry) => string_prop("applies-when", entry, report)
            .filter(|marker| applies_when(marker, entry.offset(), report))
            .map(Some),
    };
    let slot = slot.and_then(|(slot, offset)| {
        let parsed = Slot::parse(&slot);
        if parsed.is_none() {
            report.at(
                offset,
                format!("this version has no slot `{slot}`"),
                Some(match nearest(&slot, Slot::NAMES) {
                    Some(near) => format!("did you mean `{near}`?"),
                    None => format!("a slot is one of {}", Slot::NAMES.join(", ")),
                }),
            );
        }
        parsed
    });
    let view = view.filter(|(view, offset)| relative_file(view, "kdl", *offset, report));
    let source = source.filter(|(source, offset)| referred_id(source, "source", *offset, report));
    let band = band(node, &parts, slot?, report);
    let ((view, view_at), (source, source_at)) = (view?, source?);
    Some((
        SurfaceDecl {
            id: id?,
            slot: slot?,
            view,
            source,
            title: title.map(|title| Clean::label(&title).into_string()),
            band: band?,
            applies_when: applies_when?,
        },
        (view_at, source_at),
    ))
}

/// A band's `rows=` (1 to 6) and `place=` (`top` or `bottom`); refused on
/// any other slot. `Some(None)` for a surface that is no band.
fn band(
    node: &KdlNode,
    parts: &super::document::Entries<'_>,
    slot: Slot,
    report: &mut Report<'_>,
) -> Option<Option<uze_contract::BandDecl>> {
    let rows = prop(parts, "rows");
    let place = prop(parts, "place");
    if slot != Slot::Band {
        for entry in [rows, place].into_iter().flatten() {
            report.on(
                entry,
                "only a band takes `rows` and `place`",
                Some("remove it, or write `slot=\"band\"`".to_owned()),
            );
        }
        return Some(None);
    }
    let Some(rows_entry) = rows else {
        report.on(
            node,
            "a band names how many rows it takes",
            Some(format!(
                "add `rows=2` (1 to {})",
                uze_contract::BAND_ROWS_MAX
            )),
        );
        return None;
    };
    let counted = match rows_entry.value() {
        KdlValue::Integer(rows) => u8::try_from(*rows).ok(),
        _ => None,
    }
    .filter(|rows| (1..=uze_contract::BAND_ROWS_MAX).contains(rows));
    let Some(rows) = counted else {
        report.on(
            rows_entry,
            format!("a band takes 1 to {} rows", uze_contract::BAND_ROWS_MAX),
            Some("write a whole number, such as `rows=2`".to_owned()),
        );
        return None;
    };
    let place = match place {
        None => uze_contract::BandPlace::Top,
        Some(entry) => {
            let written = string_prop("place", entry, report)?;
            let Some(place) = uze_contract::BandPlace::parse(&written) else {
                report.on(
                    entry,
                    format!("a band is placed `top` or `bottom`, not `{written}`"),
                    None,
                );
                return None;
            };
            place
        }
    };
    Some(Some(uze_contract::BandDecl { rows, place }))
}

/// `preferences { number "warn-at" label="…" default=70 min=1 max=100; … }`.
fn preferences(node: &KdlNode, report: &mut Report<'_>) -> Vec<uze_contract::PreferenceDecl> {
    use uze_contract::{PreferenceDecl, PreferenceKind};
    let parts = entries(node, report);
    refuse_extra(node, &parts, 0, &[], true, report);
    let mut declared: Vec<PreferenceDecl> = Vec::new();
    for child in node.children().map(KdlDocument::nodes).unwrap_or_default() {
        let kind_name = child.name().value();
        if !PreferenceKind::NAMES.contains(&kind_name) {
            unknown_child(child, "`preferences`", PreferenceKind::NAMES, report);
            continue;
        }
        let parts = entries(child, report);
        let allowed: &[&str] = match kind_name {
            "number" => &["label", "default", "min", "max"],
            "secret" => &["label"],
            _ => &["label", "default"],
        };
        let max_args = if kind_name == "choice" { usize::MAX } else { 1 };
        refuse_extra(child, &parts, max_args, allowed, false, report);
        let Some(name) = declared_id(child, parts.args.first().copied(), report) else {
            continue;
        };
        if declared.iter().any(|seen| seen.name == name) {
            report.on(
                child,
                format!("the preference `{name}` is declared twice"),
                None,
            );
            continue;
        }
        let label = prop(&parts, "label")
            .and_then(|entry| string_prop("label", entry, report))
            .unwrap_or_else(|| name.clone());
        let integer = |key: &str, report: &mut Report<'_>| {
            prop(&parts, key).and_then(|entry| match entry.value() {
                KdlValue::Integer(number) => i64::try_from(*number).ok(),
                other => {
                    report.on(
                        entry,
                        format!("`{key}` is a whole number, not {}", spelled(other)),
                        None,
                    );
                    None
                }
            })
        };
        if kind_name == "secret" {
            declared.push(PreferenceDecl {
                name,
                label: Clean::label(&label).into_string(),
                kind: PreferenceKind::Secret,
                default: serde_json::Value::Null,
            });
            continue;
        }
        let kind = match kind_name {
            "text" => PreferenceKind::Text,
            "toggle" => PreferenceKind::Toggle,
            "number" => PreferenceKind::Number {
                min: integer("min", report),
                max: integer("max", report),
            },
            _ => {
                let values: Vec<String> = parts
                    .args
                    .iter()
                    .skip(1)
                    .filter_map(|arg| string_arg(child, Some(arg), report))
                    .collect();
                if values.is_empty() {
                    report.on(
                        child,
                        format!("the choice `{name}` names the values it offers"),
                        Some(format!("write `choice \"{name}\" \"one\" \"other\"`")),
                    );
                    continue;
                }
                PreferenceKind::Choice { values }
            }
        };
        let Some(default_entry) = prop(&parts, "default") else {
            report.on(
                child,
                format!("the preference `{name}` says its `default`"),
                None,
            );
            continue;
        };
        let default = match (default_entry.value(), &kind) {
            (KdlValue::String(text), PreferenceKind::Text | PreferenceKind::Choice { .. }) => {
                serde_json::Value::String(text.clone())
            }
            (KdlValue::Bool(flag), PreferenceKind::Toggle) => serde_json::Value::Bool(*flag),
            (KdlValue::Integer(number), PreferenceKind::Number { .. }) => {
                serde_json::Value::from(i64::try_from(*number).unwrap_or_default())
            }
            (KdlValue::Float(number), PreferenceKind::Number { .. }) => {
                serde_json::Value::from(*number)
            }
            (other, _) => {
                report.on(
                    default_entry,
                    format!(
                        "the default of a `{kind_name}` cannot be {}",
                        spelled(other)
                    ),
                    None,
                );
                continue;
            }
        };
        let decl = PreferenceDecl {
            name,
            label: Clean::label(&label).into_string(),
            kind,
            default: default.clone(),
        };
        if !decl.holds(&default) {
            report.on(
                default_entry,
                format!("the default of `{}` is not a value it may hold", decl.name),
                None,
            );
            continue;
        }
        declared.push(decl);
    }
    declared
}

/// A surface's, a source's or an action's own id, its first argument.
fn declared_id(node: &KdlNode, arg: Option<&KdlEntry>, report: &mut Report<'_>) -> Option<String> {
    let id = string_arg(node, arg, report)?;
    if is_valid_id(&id) {
        return Some(id);
    }
    report.on(
        arg?,
        format!(
            "`{id}` is not an id: lowercase letters, digits and single hyphens, starting with a letter, at most {} long",
            uze_contract::ID_MAX
        ),
        None,
    );
    None
}

/// Whether `id`, naming a `kind` declared elsewhere, is spelled as an id.
fn referred_id(id: &str, kind: &str, offset: usize, report: &mut Report<'_>) -> bool {
    let valid = is_valid_id(id);
    if !valid {
        report.at(offset, format!("`{id}` is not a {kind} id"), None);
    }
    valid
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

/// Whether `text`, which the host fills in when it runs something, holds
/// only literal text and `{field}` references. Reports why not.
fn template(text: &str, offset: usize, report: &mut Report<'_>) -> bool {
    match parse_text(text) {
        Ok(_) => true,
        Err(why) => {
            report.at(offset, why, None);
            false
        }
    }
}

/// The file inside the extension a packaged program names, without its
/// leading `./`.
pub fn packaged(program: &Program) -> Option<&str> {
    program
        .is_packaged()
        .then(|| program.program().trim_start_matches("./"))
}

/// What in a program's first element says it was written as a shell
/// command rather than an argv.
const SHELL: &[char] = &[
    ' ', '\t', '|', '&', ';', '<', '>', '$', '`', '(', ')', '*', '?',
];

/// `run "<program>" "<arg>" … timeout="5s"`: an argv, its first element a
/// path inside the extension (`./bin/meter`) or a program's name an
/// `exec:` grant covers, each later one a whole argument with `{field}`
/// references.
fn program(node: &KdlNode, report: &mut Report<'_>) -> Option<Program> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, usize::MAX, &["timeout"], false, report);
    let mut argv = Vec::new();
    for arg in &parts.args {
        let text = string_arg(node, Some(arg), report)?;
        if !argv.is_empty() && !template(&text, arg.offset(), report) {
            return None;
        }
        argv.push(text);
    }
    let Some(first) = argv.first() else {
        report.on(
            node,
            "`run` names a program",
            Some(
                "write `run \"./bin/meter\"` or `run \"nvidia-smi\" \"--query-gpu=name\"`"
                    .to_owned(),
            ),
        );
        return None;
    };
    if first.contains(SHELL) {
        report.on(
            parts.args[0],
            format!("`{first}` is a shell command: a program is an argv array"),
            Some("write the program and each argument as strings of their own, with no pipe or redirect".to_owned()),
        );
        return None;
    }
    let mut program = Program::new(argv);
    if program.is_packaged() {
        let file = packaged(&program).unwrap_or_default();
        if !is_relative_inside(file) {
            report.on(
                parts.args[0],
                format!(
                    "`{}` is not a path inside the extension's directory",
                    program.program()
                ),
                Some("write `./` and a relative path with `/`, without `..`".to_owned()),
            );
            return None;
        }
    } else if !is_program_name(program.program()) {
        report.on(
            parts.args[0],
            format!("`{}` is not a program's name", program.program()),
            Some("name the program as `exec:` grants it, such as `nvidia-smi`".to_owned()),
        );
        return None;
    }
    if let Some(entry) = prop(&parts, "timeout") {
        let written = string_prop("timeout", entry, report)?;
        match source::duration(&written) {
            Some(limit) if limit.is_zero() || limit > uze_contract::PROGRAM_TIME_LIMIT => {
                report.on(
                    entry,
                    format!(
                        "a program runs for at most {}s",
                        uze_contract::PROGRAM_TIME_LIMIT.as_secs()
                    ),
                    Some("lower it, or leave it out for the host's limit".to_owned()),
                );
                return None;
            }
            Some(limit) => program.timeout = Some(limit),
            None => {
                report.on(
                    entry,
                    format!("`{written}` is not a duration"),
                    Some("write a whole number and a unit, such as `timeout=\"5s\"`".to_owned()),
                );
                return None;
            }
        }
    }
    Some(program)
}

/// `exec "nvidia-smi" { reads "/dev/nvidiactl" "/dev/dxg" }`: the absolute
/// paths a granted program reads beyond the host's own.
fn exec(node: &KdlNode, report: &mut Report<'_>) -> Option<ExecDecl> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &[], true, report);
    let name = string_arg(node, parts.args.first().copied(), report)?;
    if !is_program_name(&name) {
        report.on(node, format!("`{name}` is not a program's name"), None);
        return None;
    }
    let mut reads = Vec::new();
    for child in node.children().map(KdlDocument::nodes).unwrap_or_default() {
        if child.name().value() != "reads" {
            unknown_child(child, "an `exec`", &["reads"], report);
            continue;
        }
        for (path, at) in string_args(child, report) {
            let absolute = std::path::Path::new(&path).is_absolute()
                && !path.split('/').any(|part| part == "..");
            if absolute {
                reads.push(path);
            } else {
                report.at(
                    at,
                    format!("`{path}` is not an absolute path"),
                    Some(
                        "write the device or directory the program reads, such as `/dev/nvidiactl`"
                            .to_owned(),
                    ),
                );
            }
        }
    }
    Some(ExecDecl { name, reads })
}

/// Whether `path` is relative and stays beneath where it is resolved from:
/// no root, no drive, no `..`, `/` between its parts.
fn is_relative_inside(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && std::path::Path::new(path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// Whether `marker`, what `applies-when` names, is a path inside a
/// checkout: relative, with `/`, never climbing out. Reports why not.
pub fn applies_when(marker: &str, offset: usize, report: &mut Report<'_>) -> bool {
    let inside = is_relative_inside(marker);
    if !inside {
        report.at(
            offset,
            format!("`{marker}` is not a path inside a checkout"),
            Some("write the file or directory it needs, relative to the checkout: `applies-when=\".planning\"`".to_owned()),
        );
    }
    inside
}

/// Whether `file` names a file inside the extension's directory with the
/// `extension` it must have. Reports why not.
pub fn relative_file(file: &str, extension: &str, offset: usize, report: &mut Report<'_>) -> bool {
    if !is_relative_inside(file) {
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
    if let Some((_, change)) = LATER.iter().find(|(later, _)| *later == name) {
        report.on(
            node,
            format!("`{name}` arrives in a later change (change {change})"),
            Some("remove it for now".to_owned()),
        );
        return;
    }
    let case = NODES.iter().find(|known| known.eq_ignore_ascii_case(name));
    report.on(
        node,
        format!("a manifest has no `{name}`"),
        Some(match (case, nearest(name, NODES)) {
            (Some(known), _) => format!("manifest nodes are lowercase: write `{known}`"),
            (None, Some(near)) => format!("did you mean `{near}`?"),
            (None, None) => format!(
                "a manifest declares {}",
                accepts(NODES).trim_start_matches("it takes ")
            ),
        }),
    );
}

/// A node inside a block that is not one of `known`, said with the nearest
/// spelling or the list.
fn unknown_child(child: &KdlNode, holder: &str, known: &[&str], report: &mut Report<'_>) {
    let name = child.name().value();
    let case = known.iter().find(|known| known.eq_ignore_ascii_case(name));
    report.on(
        child,
        format!("{holder} holds no `{name}`"),
        Some(match (case, nearest(name, known)) {
            (Some(known), _) => format!("write `{known}`"),
            (None, Some(near)) => format!("did you mean `{near}`?"),
            (None, None) => format!(
                "{holder} holds {}",
                accepts(known).trim_start_matches("it takes ")
            ),
        }),
    );
}

fn refuse_repeated_ids(document: &KdlDocument, report: &mut Report<'_>) {
    for kind in ["surface", "source", "action"] {
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

#[cfg(test)]
mod tests;
