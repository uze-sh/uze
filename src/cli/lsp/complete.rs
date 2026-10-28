//! Completion and hover for extension files, answered from the catalog the
//! decoder checks against, so an editor offers exactly what the check
//! accepts.

use serde_json::{Value, json};
use uze_application::extension_locate::{At, Located, locate};
use uze_contract::catalog::{CATALOG, COMMON_PROPS, PropKind, prop_doc};
use uze_contract::testing::{STEPS, step_doc};
use uze_contract::{NodeName, RoleName, Slot};

/// Which file of an extension is being edited.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileKind {
    Manifest,
    View,
    Test,
}

impl FileKind {
    /// By the file's path inside its extension's directory.
    pub(crate) fn of(relative: &str) -> Option<Self> {
        if relative == "extension.kdl" {
            Some(Self::Manifest)
        } else if relative.starts_with("tests/") && relative.ends_with(".test.kdl") {
            Some(Self::Test)
        } else if relative.ends_with(".kdl") {
            Some(Self::View)
        } else {
            None
        }
    }
}

const MANIFEST_NODES: &[(&str, &str)] = &[
    (
        "protocol",
        "The extension protocol this manifest speaks: `1`.",
    ),
    (
        "id",
        "The extension's id: the name of its directory under `extensions/`.",
    ),
    ("name", "What the extension is called where it is listed."),
    ("description", "One line on what it shows."),
    (
        "surface",
        "A place it draws: `surface \"<id>\" slot=… view=… source=…`.",
    ),
    (
        "source",
        "Where a surface's data comes from: `source \"<id>\" { script \"….rhai\"; }`.",
    ),
    (
        "action",
        "Something the operator can do on a surface: `action \"<id>\" label=… on=… { refresh \"<source>\"; }`.",
    ),
    (
        "wants",
        "What the extension reaches beyond drawing: `read:checkout`, `clipboard`, `open-path`, `exec:<program>`, `store`, `store:global`, `notify`.",
    ),
    (
        "exec",
        "What a granted program reads beyond the host's paths: `exec \"nvidia-smi\" { reads \"/dev/dxg\"; }`.",
    ),
    (
        "preferences",
        "Settings the operator edits: `preferences { number \"warn-at\" default=70 min=1 max=100; }`.",
    ),
];
const SOURCE_NODES: &[(&str, &str)] = &[
    ("script", "A Rhai file whose `answer` returns the data."),
    (
        "run",
        "A program it runs, as an argv: `run \"./bin/meter\" \"--\" \"{field}\"`.",
    ),
    ("fixture", "A JSON file the source is fed by."),
    ("events", "The last events the extension saw: `events 50`."),
    (
        "output",
        "How a program's output is read: `\"json\"`, `\"lines\"` or `\"view\"`.",
    ),
    ("fields", "The fields of a line, for `output \"lines\"`."),
    ("store", "Hand the extension's store to it on its input."),
    (
        "preferences",
        "Hand the extension's preferences to it on its input.",
    ),
    (
        "deliver",
        "How the events that ask it again reach it: `\"latest\"` or `\"each\"`.",
    ),
    ("input", "What it is asked with: `input name=\"{field}\"`."),
    (
        "every",
        "How often it is asked again while shown: `every \"10s\"`, at least 5s.",
    ),
    (
        "refresh-on",
        "What asks it again: `\"paths.changed\" \"<path>\"` or `\"checkout.changed\"`.",
    ),
    (
        "applies-when",
        "A path the checkout must hold for it to be asked: `applies-when \".planning\"`.",
    ),
];
const ACTION_PROPS: &[(&str, &str)] = &[
    ("label", "What the footer and menus call it."),
    ("on", "The surface it is offered on."),
    (
        "key",
        "The key it suggests, such as `ctrl-r`; bound only where it is free.",
    ),
    ("confirm", "The question asked before it runs."),
];
const ACTION_NODES: &[(&str, &str)] = &[
    ("script", "A Rhai file whose `answer` runs the action."),
    ("refresh", "The sources it asks again."),
    (
        "clipboard",
        "The text it copies, with `{field}` references.",
    ),
    ("open-path", "The file inside the checkout it opens."),
    ("open", "The surface of this extension it opens."),
    (
        "run",
        "A program it runs, as an argv, confined where the machine can.",
    ),
    (
        "with",
        "The UI and form values it reads: `with \"ui.sort\" \"form.*\"`.",
    ),
    (
        "then",
        "What its run's answer is for: `then { agent-prompt \"Review {answer.file}\" target=\"self\"; }`.",
    ),
];
const SURFACE_PROPS: &[(&str, &str)] = &[
    (
        "slot",
        "Where it is drawn: `view`, `section`, `drawer`, `status`, `band` or `overlay`.",
    ),
    (
        "rows",
        "How many rows a band takes from each agent's pane, 1 to 6.",
    ),
    (
        "place",
        "Where a band stands on the pane: `top` or `bottom`.",
    ),
    ("view", "The view file it draws, relative to the extension."),
    ("source", "The source its data comes from."),
    ("title", "What the host titles it with."),
    (
        "applies-when",
        "A path the checkout must hold for it to be offered: `applies-when=\".planning\"`.",
    ),
];
const DIRECTIVES: &[(&str, &str)] = &[
    (
        "lookup",
        "A table from a value to a role or a label: `lookup \"tone\" { done \"success\"; }`.",
    ),
    (
        "template",
        "Nodes named for reuse with `use`: `template \"row\" { … }`.",
    ),
    ("use", "Draw a template here, bound to the current item."),
];

/// LSP completion item kinds, by what a name is.
const KIND_FUNCTION: u8 = 3;
const KIND_VARIABLE: u8 = 6;
const KIND_PROPERTY: u8 = 10;
const KIND_VALUE: u8 = 12;
const KIND_KEYWORD: u8 = 14;

fn item(label: &str, kind: u8, doc: &str) -> Value {
    json!({ "label": label, "kind": kind, "documentation": doc })
}

/// What to offer at byte `offset` of `text`. `fields` are the keys of the
/// data the view is bound to, at the top level.
pub(crate) fn completions(kind: FileKind, text: &str, offset: usize, fields: &Value) -> Vec<Value> {
    let located = locate(text, offset);
    match (&located.at, kind) {
        (At::NodeName { .. }, FileKind::Manifest) => match located.path.as_slice() {
            [] => MANIFEST_NODES
                .iter()
                .map(|(name, doc)| item(name, KIND_KEYWORD, doc))
                .collect(),
            [.., parent] if parent == "source" => SOURCE_NODES
                .iter()
                .map(|(name, doc)| item(name, KIND_KEYWORD, doc))
                .collect(),
            [.., parent] if parent == "action" => ACTION_NODES
                .iter()
                .map(|(name, doc)| item(name, KIND_KEYWORD, doc))
                .collect(),
            _ => Vec::new(),
        },
        (At::NodeName { .. }, FileKind::Test) => match located.path.as_slice() {
            [] => vec![item(
                "test",
                KIND_KEYWORD,
                "A test: `test \"what it proves\" { … }`.",
            )],
            _ => STEPS
                .iter()
                .map(|(name, doc)| item(name, KIND_KEYWORD, doc))
                .collect(),
        },
        (At::NodeName { .. }, FileKind::View) => view_nodes(&located),
        (At::PropName { .. }, _) => properties(kind, &located),
        (At::Value { prop, .. }, _) => values(kind, &located, prop.as_deref(), text),
        (At::Reference { prefix, .. }, _) => references(prefix, fields, text),
        (At::Nothing, _) => Vec::new(),
    }
}

/// The nodes valid where a view's cursor is: what the enclosing node may
/// hold, every node at the top, and the directives at the top.
fn view_nodes(located: &Located) -> Vec<Value> {
    let parent = located.path.last().and_then(|name| NodeName::parse(name));
    let allowed: Vec<NodeName> = match parent {
        Some(parent) => parent
            .spec()
            .children
            .map(<[NodeName]>::to_vec)
            .unwrap_or_default(),
        None => CATALOG.iter().map(|spec| spec.name).collect(),
    };
    let mut items: Vec<Value> = allowed
        .iter()
        .map(|name| item(name.as_str(), KIND_FUNCTION, name.doc()))
        .collect();
    if located.path.is_empty() {
        items.extend(
            DIRECTIVES
                .iter()
                .map(|(name, doc)| item(name, KIND_KEYWORD, doc)),
        );
    } else if parent.is_some() {
        items.push(item("use", KIND_KEYWORD, DIRECTIVES[2].1));
    }
    items
}

fn properties(kind: FileKind, located: &Located) -> Vec<Value> {
    let Some(node) = located.node.as_deref() else {
        return Vec::new();
    };
    match kind {
        FileKind::Manifest if node == "surface" => SURFACE_PROPS
            .iter()
            .map(|(name, doc)| item(name, KIND_PROPERTY, doc))
            .collect(),
        FileKind::Manifest if node == "action" => ACTION_PROPS
            .iter()
            .map(|(name, doc)| item(name, KIND_PROPERTY, doc))
            .collect(),
        FileKind::Test => match node {
            "given" => vec![
                item(
                    "fixture",
                    KIND_PROPERTY,
                    "A JSON file, relative to the extension.",
                ),
                item(
                    "json",
                    KIND_PROPERTY,
                    "JSON written here: `json=#\"{ … }\"#`.",
                ),
            ],
            "expect-absent" => vec![item(
                "because",
                KIND_PROPERTY,
                "The field or UI value it is absent for.",
            )],
            _ => Vec::new(),
        },
        FileKind::View => match NodeName::parse(node) {
            Some(name) => name
                .spec()
                .props
                .iter()
                .chain(COMMON_PROPS)
                .map(|(prop, _)| item(prop, KIND_PROPERTY, prop_doc(prop).unwrap_or_default()))
                .collect(),
            None => Vec::new(),
        },
        FileKind::Manifest => Vec::new(),
    }
}

fn values(kind: FileKind, located: &Located, prop: Option<&str>, text: &str) -> Vec<Value> {
    let words = |names: &[&str]| -> Vec<Value> {
        names
            .iter()
            .map(|name| item(name, KIND_VALUE, ""))
            .collect()
    };
    match (kind, located.node.as_deref(), prop) {
        (FileKind::Manifest, Some("surface"), Some("slot")) => words(Slot::NAMES),
        (FileKind::Test, Some("theme"), None) => words(&["dark", "light"]),
        (FileKind::Test, Some("expect-absent"), None) => CATALOG
            .iter()
            .map(|spec| item(spec.name.as_str(), KIND_FUNCTION, spec.name.doc()))
            .collect(),
        (FileKind::View, Some(node), Some(prop)) => {
            match NodeName::parse(node).and_then(|name| name.prop(prop)) {
                Some(PropKind::Role) => {
                    let mut items: Vec<Value> = RoleName::ALL
                        .iter()
                        .map(|role| item(role.as_str(), KIND_VALUE, role.doc()))
                        .collect();
                    items.extend(lookup_tables(text).into_iter().map(|table| {
                        item(
                            &format!("{{{table}:}}"),
                            KIND_VARIABLE,
                            "A role looked up from a field.",
                        )
                    }));
                    items
                }
                Some(PropKind::Word(names)) => words(names),
                Some(PropKind::Flag) => words(&["#true", "#false"]),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Inside `{…}`: the data's fields, walked along a dotted prefix, the UI
/// scope, and the view's lookup tables.
fn references(prefix: &str, fields: &Value, text: &str) -> Vec<Value> {
    // `{table:field}` looks a field's value up: after the colon, a field.
    if prefix.contains(':') {
        return keys(fields)
            .into_iter()
            .map(|key| item(&key, KIND_VARIABLE, ""))
            .collect();
    }
    let mut at = fields;
    let mut parts: Vec<&str> = prefix.split('.').collect();
    parts.pop();
    for part in &parts {
        at = match at {
            Value::Object(map) => map.get(*part).unwrap_or(&Value::Null),
            Value::Array(list) => list
                .first()
                .and_then(|first| first.get(*part))
                .unwrap_or(&Value::Null),
            _ => &Value::Null,
        };
    }
    let mut items: Vec<Value> = keys(at)
        .into_iter()
        .map(|key| item(&key, KIND_VARIABLE, "A field of the data."))
        .collect();
    if parts.is_empty() {
        items.push(item(
            "ui.",
            KIND_VARIABLE,
            "A value the host holds for a `Tabs`, `Toggle` or `List` with an `id`.",
        ));
        items.extend(lookup_tables(text).into_iter().map(|table| {
            item(
                &format!("{table}:"),
                KIND_VARIABLE,
                "Look a field's value up in this table; `{table::key}` for a label.",
            )
        }));
    }
    items
}

/// The keys of an object, or of a list's first item.
fn keys(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => map.keys().cloned().collect(),
        Value::Array(list) => list.first().map(keys).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The lookup tables a view declares, read off its lines.
fn lookup_tables(text: &str) -> Vec<String> {
    let mut tables: Vec<String> = text
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("lookup \""))
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_owned)
        .collect();
    tables.sort();
    tables.dedup();
    tables
}

/// What hovering `word` in a file of `kind` explains.
pub(crate) fn hover(kind: FileKind, word: &str) -> Option<String> {
    match kind {
        FileKind::View => {
            if let Some(name) = NodeName::parse(word) {
                let spec = name.spec();
                let props: Vec<String> = spec
                    .props
                    .iter()
                    .map(|(prop, kind)| {
                        format!(
                            "- `{prop}` {}: {}",
                            kind_name(*kind),
                            prop_doc(prop).unwrap_or_default()
                        )
                    })
                    .collect();
                let mut said = format!("**{name}**\n\n{}", name.doc());
                if !props.is_empty() {
                    said.push_str(&format!("\n\n{}", props.join("\n")));
                }
                return Some(said);
            }
            if let Some(role) = RoleName::parse(word) {
                return Some(format!("**{}** (role)\n\n{}", role.as_str(), role.doc()));
            }
            DIRECTIVES
                .iter()
                .find(|(name, _)| *name == word)
                .map(|(name, doc)| format!("**{name}**\n\n{doc}"))
                .or_else(|| prop_doc(word).map(|doc| format!("**{word}**\n\n{doc}")))
        }
        FileKind::Test => step_doc(word).map(|doc| format!("**{word}**\n\n{doc}")),
        FileKind::Manifest => MANIFEST_NODES
            .iter()
            .chain(SURFACE_PROPS)
            .chain(SOURCE_NODES)
            .chain(ACTION_PROPS)
            .chain(ACTION_NODES)
            .find(|(name, _)| *name == word)
            .map(|(name, doc)| format!("**{name}**\n\n{doc}")),
    }
}

fn kind_name(kind: PropKind) -> &'static str {
    match kind {
        PropKind::Text => "text",
        PropKind::Count => "count",
        PropKind::Size => "size",
        PropKind::Number => "number",
        PropKind::Flag => "flag",
        PropKind::Role => "role",
        PropKind::Identity => "literal",
        PropKind::Word(_) => "word",
        PropKind::Value => "value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[Value]) -> Vec<String> {
        items
            .iter()
            .map(|item| item["label"].as_str().unwrap().to_owned())
            .collect()
    }

    fn at_cursor(kind: FileKind, marked: &str, fields: &Value) -> Vec<String> {
        let offset = marked.find('|').unwrap();
        labels(&completions(
            kind,
            &marked.replacen('|', "", 1),
            offset,
            fields,
        ))
    }

    #[test]
    fn a_node_offers_only_its_properties() {
        let said = at_cursor(FileKind::View, "Stack {\n    Progress |", &Value::Null);
        assert!(
            said.contains(&"ratio".to_owned()) && said.contains(&"when".to_owned()),
            "{said:?}"
        );
        assert!(!said.contains(&"truncate".to_owned()), "{said:?}");
    }

    #[test]
    fn inside_a_container_only_its_children_are_offered() {
        let said = at_cursor(FileKind::View, "Progress {\n    |", &Value::Null);
        assert_eq!(said, ["Segment", "use"]);
        let top = at_cursor(FileKind::View, "|", &Value::Null);
        assert!(top.contains(&"Stack".to_owned()) && top.contains(&"lookup".to_owned()));
    }

    #[test]
    fn a_role_is_completed_from_the_roles_and_the_views_lookups() {
        let said = at_cursor(
            FileKind::View,
            "lookup \"tone\" { a \"success\"; }\nText \"a\" role=\"|",
            &Value::Null,
        );
        assert!(said.contains(&"muted".to_owned()), "{said:?}");
        assert!(said.contains(&"{tone:}".to_owned()), "{said:?}");
    }

    #[test]
    fn a_reference_offers_the_fixtures_fields_along_its_path() {
        let fields = json!({"current": {"name": "x", "done": 1}, "phases": [{"number": "09"}]});
        let top = at_cursor(FileKind::View, "Text \"{|", &fields);
        assert!(
            top.contains(&"current".to_owned()) && top.contains(&"ui.".to_owned()),
            "{top:?}"
        );
        let deeper = at_cursor(FileKind::View, "Text \"{current.|", &fields);
        assert_eq!(deeper, ["name", "done"]);
        let listed = at_cursor(FileKind::View, "Text \"{phases.|", &fields);
        assert_eq!(listed, ["number"]);
    }

    #[test]
    fn tests_and_manifests_offer_their_own_vocabulary() {
        let steps = at_cursor(FileKind::Test, "test \"a\" {\n    expect-|", &Value::Null);
        assert!(steps.contains(&"expect-text".to_owned()), "{steps:?}");
        let slots = at_cursor(FileKind::Manifest, "surface \"b\" slot=\"|", &Value::Null);
        assert_eq!(
            slots,
            [
                "view", "section", "drawer", "status", "band", "overlay", "bar"
            ]
        );
        let answers = at_cursor(FileKind::Manifest, "source \"b\" {\n    s|", &Value::Null);
        assert!(answers.contains(&"script".to_owned()), "{answers:?}");
        let does = at_cursor(FileKind::Manifest, "action \"r\" {\n    r|", &Value::Null);
        assert!(does.contains(&"refresh".to_owned()), "{does:?}");
    }

    #[test]
    fn hover_explains_a_node_with_its_properties() {
        let said = hover(FileKind::View, "Segment").expect("a node");
        assert!(said.contains("weight") && said.contains("ratio"), "{said}");
        assert!(hover(FileKind::Test, "expect-text").is_some());
        assert!(hover(FileKind::View, "nothing-here").is_none());
    }
}
