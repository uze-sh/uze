//! What an author reads beside the `uze:author` Skill's extension reference, generated from
//! this crate so it cannot say something the contract does not
//! (`extension-platform` D21, `extension-platform-closure` 2.1–2.2).
//!
//! - [`schema`]: the JSON Schema of a manifest as decoded and of a composed
//!   answer, with a worked example per slot and per source kind.
//! - [`reference`]: the exhaustive list, in Markdown: every slot, source
//!   kind, grant, event, action, envelope member and catalog node.
//!
//! Both are written to `docs/extensions/` by [`REGENERATE`], and the root
//! suite fails when the committed files differ from what this returns.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Duration;

use schemars::generate::SchemaSettings;
use serde_json::{Map, Value, json};

use crate::catalog::{CATALOG, COMMON_PROPS, COMPOSED_PROPS, NodeSpec, PropKind, RoleName};
use crate::{
    ActionDecl, AgentEvent, AgentTarget, BandDecl, BandPlace, Deliver, Does, Envelope, EventName,
    ExecDecl, Grant, KeySuggestion, Manifest, Output, PROTOCOL, PreferenceDecl, PreferenceKind,
    Program, PromptText, RESERVED_IDS, Slot, SourceDecl, SourceKind, SpriteDecl, SpriteState,
    SurfaceDecl,
};

/// Where the schema is committed, from the repository root.
pub const SCHEMA_PATH: &str = "docs/extensions/schema.json";
/// Where the reference is committed, from the repository root.
pub const REFERENCE_PATH: &str = "docs/extensions/reference.md";
/// The command that writes both.
pub const REGENERATE: &str =
    "cargo run --quiet -p uze-contract --features schema --example extension-docs";

/// The schema, as committed: pretty JSON ending in a newline.
pub fn schema() -> String {
    let mut text = serde_json::to_string_pretty(&schema_value()).expect("a schema is JSON");
    text.push('\n');
    text
}

fn schema_value() -> Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    generator.subschema_for::<Manifest>();
    generator.subschema_for::<Envelope>();
    let mut defs = generator.take_definitions(true);
    if let Some(Value::Object(manifest)) = defs.get_mut("Manifest") {
        let examples = examples()
            .into_iter()
            .map(|manifest| serde_json::to_value(manifest).expect("a manifest is JSON"))
            .collect();
        manifest.insert("examples".into(), Value::Array(examples));
    }
    for spec in CATALOG {
        defs.insert(node_def(spec), node_schema(spec));
    }
    defs.insert(
        "CatalogNode".into(),
        json!({
            "description": "A node of the catalog, as a composed answer writes it: `type`, \
                            then its `args`, `props` and `children`. Every value is a \
                            literal; `{x}` in a composed tree is text.",
            "oneOf": CATALOG.iter().map(node_ref).collect::<Vec<_>>(),
        }),
    );
    defs.insert(
        "ComposedAnswer".into(),
        json!({
            "description": "What a source with `output \"view\"` (or a resident script's \
                            `render`) answers: a tree of catalog nodes, alone or as the \
                            `data` of an envelope.",
            "anyOf": [
                { "$ref": "#/$defs/CatalogNode" },
                {
                    "allOf": [
                        { "$ref": "#/$defs/Envelope" },
                        { "properties": { "data": { "$ref": "#/$defs/CatalogNode" } } }
                    ]
                }
            ],
            "examples": composed_examples(),
        }),
    );
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "UZE extension contract",
        "description": format!(
            "A declared extension's manifest as UZE decodes it from `extension.kdl`, and the \
             answer a composed source gives. Protocol {PROTOCOL}. Generated from the \
             uze-contract crate by `{REGENERATE}`; do not edit."
        ),
        "anyOf": [
            { "$ref": "#/$defs/Manifest" },
            { "$ref": "#/$defs/ComposedAnswer" }
        ],
        "$defs": defs,
    })
}

fn node_def(spec: &NodeSpec) -> String {
    format!("{}Node", spec.name)
}

fn node_ref(spec: &NodeSpec) -> Value {
    json!({ "$ref": format!("#/$defs/{}", node_def(spec)) })
}

fn node_schema(spec: &NodeSpec) -> Value {
    let mut properties = Map::new();
    properties.insert("type".into(), json!({ "const": spec.name.as_str() }));
    let mut required = vec![json!("type")];
    if !spec.args.is_empty() {
        properties.insert(
            "args".into(),
            json!({
                "type": "array",
                "prefixItems": spec.args.iter().map(|kind| literal(*kind)).collect::<Vec<_>>(),
                "items": false,
                "minItems": spec.required_args,
            }),
        );
        if spec.required_args > 0 {
            required.push(json!("args"));
        }
    }
    let props: Map<String, Value> = spec
        .props
        .iter()
        .chain(
            COMMON_PROPS
                .iter()
                .filter(|(name, _)| COMPOSED_PROPS.contains(name)),
        )
        .map(|(name, kind)| {
            let mut schema = literal(*kind);
            if let (Value::Object(schema), Some(doc)) =
                (&mut schema, crate::catalog::prop_doc(name))
            {
                schema.insert("description".into(), json!(doc));
            }
            ((*name).to_owned(), schema)
        })
        .collect();
    properties.insert(
        "props".into(),
        json!({ "type": "object", "properties": props, "additionalProperties": false }),
    );
    let mut node = Map::new();
    node.insert("description".into(), json!(spec.name.doc()));
    node.insert("type".into(), json!("object"));
    if spec.props.iter().any(|(name, _)| *name == "action") {
        properties.insert(
            "item".into(),
            json!({ "description": "What the named action receives when it runs." }),
        );
        node.insert(
            "dependentSchemas".into(),
            json!({ "item": { "required": ["props"], "properties": { "props": { "required": ["action"] } } } }),
        );
    }
    if let Some(children) = spec.children {
        properties.insert(
            "children".into(),
            json!({
                "type": "array",
                "items": { "anyOf": children.iter().map(|child| node_ref(child.spec())).collect::<Vec<_>>() },
            }),
        );
    }
    node.insert("properties".into(), Value::Object(properties));
    node.insert("required".into(), Value::Array(required));
    node.insert("additionalProperties".into(), json!(false));
    Value::Object(node)
}

/// What a composed answer may write for a value of `kind`.
fn literal(kind: PropKind) -> Value {
    match kind {
        PropKind::Text => json!({ "type": ["string", "number", "boolean"] }),
        PropKind::Count => json!({ "type": "integer", "minimum": 0 }),
        PropKind::Number => json!({ "type": "number" }),
        PropKind::Flag => json!({ "type": "boolean" }),
        PropKind::Role => {
            json!({ "enum": RoleName::ALL.iter().map(|role| role.as_str()).collect::<Vec<_>>() })
        }
        PropKind::Identity => json!({ "type": "string" }),
        PropKind::Word(words) => json!({ "enum": words }),
        PropKind::Value => json!({}),
    }
}

/// How the reference names a value of `kind`.
fn kind_name(kind: PropKind) -> String {
    match kind {
        PropKind::Text => "text".into(),
        PropKind::Count => "count".into(),
        PropKind::Number => "number".into(),
        PropKind::Flag => "flag".into(),
        PropKind::Role => "role".into(),
        PropKind::Identity => "name".into(),
        PropKind::Word(words) => words
            .iter()
            .map(|word| format!("`{word}`"))
            .collect::<Vec<_>>()
            .join(" · "),
        PropKind::Value => "any value".into(),
    }
}

fn composed_examples() -> Value {
    json!([
        {
            "type": "Stack",
            "props": { "gap": 1 },
            "children": [
                { "type": "Text", "args": ["Phase 12 · Query"], "props": { "role": "bright", "strong": true } },
                { "type": "Progress", "props": { "ratio": 0.4, "role": "warning" } },
                {
                    "type": "Row",
                    "props": { "gap": 2 },
                    "children": [
                        { "type": "Badge", "args": ["running"], "props": { "role": "info" } },
                        { "type": "Button", "args": ["Next phase"], "props": { "action": "next" }, "item": { "number": 13 } }
                    ]
                }
            ]
        },
        {
            "data": {
                "type": "Row",
                "children": [
                    { "type": "Gauge", "args": ["GPU"], "props": { "ratio": 0.82, "warning": 0.7, "danger": 0.9 } },
                    { "type": "Chart", "props": { "series": [3, 5, 8, 13], "kind": "sparkline" } }
                ]
            },
            "notice": "sampled 2s ago",
            "refresh": []
        }
    ])
}

/// One worked manifest per slot, each fed by a different kind of source,
/// so every slot and every source kind has one.
fn examples() -> Vec<Manifest> {
    vec![
        Manifest {
            wants: vec![Grant::ReadCheckout, Grant::Clipboard],
            surfaces: vec![surface("board", Slot::View, "items")],
            sources: vec![SourceDecl {
                input: BTreeMap::from([("file".into(), "NOTES.md".into())]),
                every: Some(Duration::from_secs(30)),
                refresh_on: vec![EventName::PathsChanged {
                    paths: vec!["NOTES.md".into()],
                }],
                ..SourceDecl::new("items", SourceKind::Script("items.rhai".into()))
            }],
            actions: vec![ActionDecl {
                key: KeySuggestion::parse("ctrl-y"),
                confirm: Some("Copy {text}?".into()),
                ..action("copy", "Copy", "board", Does::Clipboard("{text}".into()))
            }],
            ..manifest("notes", "Notes", "A `view` slot fed by a `script` source")
        },
        Manifest {
            surfaces: vec![surface("count", Slot::Section, "items")],
            sources: vec![SourceDecl::new(
                "items",
                SourceKind::Fixture("fixtures/items.json".into()),
            )],
            ..manifest(
                "notes-count",
                "Notes",
                "A `section` slot fed by a `fixture` source",
            )
        },
        Manifest {
            wants: vec![Grant::AgentObserve],
            surfaces: vec![surface("crew", Slot::Drawer, "agents")],
            sources: vec![SourceDecl {
                refresh_on: vec![EventName::Agent(AgentEvent::TurnFinished)],
                ..SourceDecl::new("agents", SourceKind::Agents)
            }],
            actions: vec![ActionDecl {
                key: KeySuggestion::parse("alt-g"),
                ..action(
                    "go",
                    "Go to agent",
                    "crew",
                    Does::FocusAgent {
                        target: AgentTarget::Named("{id}".into()),
                    },
                )
            }],
            ..manifest("crew", "Crew", "A `drawer` slot fed by the `agents` source")
        },
        Manifest {
            wants: vec![Grant::Exec("git".into())],
            surfaces: vec![surface("last", Slot::Status, "log")],
            sources: vec![SourceDecl {
                output: Output::Lines {
                    fields: vec!["hash".into(), "subject".into()],
                    separator: Output::TAB,
                },
                refresh_on: vec![EventName::CheckoutChanged],
                ..SourceDecl::new(
                    "log",
                    SourceKind::Run(Program {
                        argv: ["git", "log", "--format=%h%x09%s", "-n", "1"]
                            .map(String::from)
                            .to_vec(),
                        timeout: Some(Duration::from_secs(5)),
                    }),
                )
            }],
            exec: vec![ExecDecl {
                name: "git".into(),
                reads: Vec::new(),
            }],
            ..manifest(
                "last-commit",
                "Last commit",
                "A `status` slot fed by a `run` source read as lines",
            )
        },
        Manifest {
            wants: vec![Grant::AgentPrompt],
            surfaces: vec![SurfaceDecl {
                band: Some(BandDecl {
                    rows: 2,
                    place: BandPlace::Bottom,
                }),
                ..surface("trail", Slot::Band, "seen")
            }],
            sources: vec![SourceDecl {
                deliver: Deliver::Each,
                ..SourceDecl::new("seen", SourceKind::Events(20))
            }],
            actions: vec![ActionDecl {
                key: KeySuggestion::parse("ctrl-n"),
                ..action(
                    "next",
                    "Run phase {number}",
                    "trail",
                    Does::AgentPrompt {
                        prompt: PromptText::Skill {
                            skill: "gsd:execute-phase".into(),
                            args: vec!["{number}".into()],
                        },
                        target: AgentTarget::Own,
                        auto: None,
                    },
                )
            }],
            ..manifest("trail", "Trail", "A `band` slot fed by an `events` source")
        },
        Manifest {
            wants: vec![Grant::Resident, Grant::Store],
            surfaces: vec![
                surface("game", Slot::Overlay, "board"),
                surface("score", Slot::Section, "board"),
            ],
            sources: vec![SourceDecl {
                with_store: true,
                ..SourceDecl::new("board", SourceKind::Resident("counter.rhai".into()))
            }],
            actions: vec![ActionDecl {
                key: KeySuggestion::parse("f6"),
                ..action("play", "Play", "score", Does::Open("game".into()))
            }],
            preferences: vec![PreferenceDecl {
                name: "speed".into(),
                label: "Speed".into(),
                kind: PreferenceKind::Number {
                    min: Some(1),
                    max: Some(10),
                },
                default: json!(5),
            }],
            sprites: vec![SpriteDecl {
                id: "pet".into(),
                sheet: "pet.png".into(),
                frame: (16, 16),
                columns: 4,
                states: vec![SpriteState {
                    name: "ready".into(),
                    frames: vec![0, 1],
                    fps: 2,
                    looped: true,
                }],
                fallback: "ready".into(),
            }],
            ..manifest(
                "counter",
                "Counter",
                "An `overlay` slot fed by a `resident` source",
            )
        },
    ]
}

fn manifest(id: &str, name: &str, description: &str) -> Manifest {
    Manifest {
        protocol: PROTOCOL,
        id: id.into(),
        name: name.into(),
        description: description.into(),
        wants: Vec::new(),
        surfaces: Vec::new(),
        sources: Vec::new(),
        actions: Vec::new(),
        exec: Vec::new(),
        preferences: Vec::new(),
        sprites: Vec::new(),
    }
}

fn surface(id: &str, slot: Slot, source: &str) -> SurfaceDecl {
    SurfaceDecl {
        id: id.into(),
        slot,
        view: format!("{id}.kdl"),
        band: None,
        source: source.into(),
        title: None,
        applies_when: None,
    }
}

fn action(id: &str, label: &str, on: &str, does: Does) -> ActionDecl {
    ActionDecl {
        id: id.into(),
        label: label.into(),
        on: on.into(),
        key: None,
        confirm: None,
        does,
        with: Vec::new(),
        then: None,
    }
}

fn slot_doc(slot: Slot) -> &'static str {
    match slot {
        Slot::View => "The full frame, opened from the tab strip like the built-in surfaces.",
        Slot::Section => "A collapsible block of the workspace sidebar.",
        Slot::Drawer => "A panel docked beside the agent's pane, opened by a gesture.",
        Slot::Status => "One row of the sidebar's header.",
        Slot::Band => {
            "`rows=1`…`6` along the `place=\"top\"` (or `\"bottom\"`) of each agent's pane, \
             taken before the pane is sized."
        }
        Slot::Overlay => "A card over the frame, opened by an `open` action and closed by Esc.",
    }
}

const SLOTS: &[Slot] = &[
    Slot::View,
    Slot::Section,
    Slot::Drawer,
    Slot::Status,
    Slot::Band,
    Slot::Overlay,
];

const SOURCE_KINDS: &[(&str, &str, &str)] = &[
    (
        "script",
        "`script \"x.rhai\"`",
        "A Rhai file whose `answer(input, context)` returns the envelope.",
    ),
    (
        "run",
        "`run \"<program>\" \"<arg>\"… timeout=\"5s\"`",
        "A program the workspace runs, pinned and confined where the machine can; its \
         standard output is read as `output` says.",
    ),
    (
        "fixture",
        "`fixture \"fixtures/x.json\"`",
        "A JSON file whose contents are the source's data.",
    ),
    (
        "events",
        "`events <n>`",
        "The workspace's own: the last `n` events the extension saw, newest first.",
    ),
    (
        "agents",
        "`agents`",
        "The workspace's own: every agent of the open spaces, under `agent:observe`.",
    ),
    (
        "resident",
        "`resident \"x.rhai\"`",
        "A Rhai file kept running while a surface showing it is open, under `resident`: \
         `start`, `event`, `input`, `render`, `stop`.",
    ),
];

const OUTPUTS: &[(&str, &str)] = &[
    (
        "json",
        "One JSON value, or an envelope around one (the default).",
    ),
    (
        "lines",
        "One item per line, its fields named in order and split at a tab or the declared \
         separator, each trimmed.",
    ),
    (
        "view",
        "A composed answer: a tree of catalog nodes the workspace validates and draws.",
    ),
];

const DELIVERIES: &[(&str, &str)] = &[
    (
        "latest",
        "Events arriving together are asked about once, with the last (the default).",
    ),
    (
        "each",
        "Asked once per event, in order, at most 16 a burst; the next says how many were dropped.",
    ),
];

fn grant_doc(grant: &Grant) -> &'static str {
    match grant {
        Grant::ReadCheckout => {
            "`read_checkout(path)` and `list_checkout(path)`, beneath the checkout root."
        }
        Grant::Clipboard => "A `clipboard` action.",
        Grant::OpenPath => "An `open-path` action.",
        Grant::Store => "The extension's store, one per project.",
        Grant::StoreGlobal => {
            "The extension's store, one for the machine, consented to on its own."
        }
        Grant::Notify => "A notification and a sound an answer asks for, rate-limited.",
        Grant::AgentObserve => "The `agents` source, `agent.*` events and `focus-agent`.",
        Grant::AgentPrompt => "`agent-prompt` on the operator's gesture, shown first.",
        Grant::AgentPromptAuto => {
            "`agent-prompt … when=`: fixed text sent on an agent's event, allowed per agent for a day."
        }
        Grant::AgentRestart => "`agent-restart`, always asked.",
        Grant::Resident => {
            "A `resident` source: its script kept running while shown, `timer`, and raw keys \
             and pointer while focused."
        }
        Grant::Exec(_) => {
            "Running that program, resolved once on the search path, pinned and confined."
        }
        Grant::Net(_) => {
            "`fetch` to that one host, over HTTPS, and `secret` preferences sent to it."
        }
    }
}

const EVENTS: &[(&str, &str)] = &[
    (
        EventName::PATHS_CHANGED,
        "A file beneath one of the listed paths, relative to the checkout, changed.",
    ),
    (
        EventName::CHECKOUT_CHANGED,
        "The checkout's branch, head or working tree changed.",
    ),
];

fn agent_event_doc(event: AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionStarted => "An agent's session started.",
        AgentEvent::TurnStarted => "An agent began a turn.",
        AgentEvent::TurnFinished => "An agent finished a turn.",
        AgentEvent::ToolStarted => "An agent started a tool.",
        AgentEvent::ToolFinished => "An agent's tool finished.",
        AgentEvent::ApprovalAsked => "An agent asked the operator to approve something.",
        AgentEvent::ApprovalResolved => "The operator answered an agent's approval.",
    }
}

const DOES: &[(&str, &str, &str)] = &[
    (
        "script",
        "`script \"x.rhai\"`",
        "Runs the file's `answer` and applies its envelope.",
    ),
    (
        "refresh",
        "`refresh \"<source>\"…`",
        "Asks these sources again.",
    ),
    (
        "clipboard",
        "`clipboard \"<template>\"`",
        "Copies the text to the clipboard; under `clipboard`.",
    ),
    (
        "open-path",
        "`open-path \"<template>\"`",
        "Opens a file inside the checkout; under `open-path`.",
    ),
    (
        "open",
        "`open \"<surface>\"`",
        "Opens another surface of the extension; an overlay's `open` key must be a \
         function key or carry ctrl or alt.",
    ),
    (
        "run",
        "`run \"<program>\" \"<arg>\"…`",
        "Runs a program, confined where the machine can, and applies the envelope it answers.",
    ),
    (
        "agent-prompt",
        "`agent-prompt \"<text>\" target=\"self\"` or `skill=\"<plugin>:<skill>\"`",
        "Types a prompt into an agent, shown first and never queued; under `agent:prompt`, \
         or `agent:prompt:auto` with `when=`.",
    ),
    (
        "agent-restart",
        "`agent-restart target=\"self\"`",
        "Starts an agent's harness on a new conversation, always asked; under `agent:restart`.",
    ),
    (
        "focus-agent",
        "`focus-agent target=\"{id}\"`",
        "Puts an agent's pane in front; under `agent:observe`.",
    ),
];

const MEMBERS: &[(&str, &str)] = &[
    ("data", "What the source's views bind; required."),
    ("refresh", "Source ids to ask again."),
    ("toast", "A line the workspace shows briefly."),
    (
        "notice",
        "A line said on the surface until the next answer.",
    ),
    (
        "store",
        "Keys of the extension's store to replace, `null` removing one; under `store`.",
    ),
    (
        "notify",
        "`{title, body}`: a notification, attributed; under `notify`.",
    ),
    (
        "sound",
        "A cue: `done`, `attention` or `error`; under `notify`.",
    ),
];

const PREFERENCE_KINDS: &[(&str, &str)] = &[
    ("text", "Text."),
    ("number", "A number, between `min` and `max` when given."),
    ("toggle", "`#true` or `#false`."),
    ("choice", "One of the values listed after its name."),
    (
        "secret",
        "Kept in the system's keychain, read only by `fetch`'s `secret` option.",
    ),
];

/// The reference, as committed.
pub fn reference() -> String {
    let mut out = String::new();
    let mut line = |text: &str| {
        out.push_str(text);
        out.push('\n');
    };
    line("# Extension protocol and catalog reference");
    line("");
    line(&format!(
        "<!-- Generated from the uze-contract crate by `{REGENERATE}`; do not edit. -->"
    ));
    line("");
    line(
        "The exhaustive list of what a declared extension may write and answer: every slot, \
         source kind, grant, event, action, envelope member and catalog node. It is generated \
         from the contract the workspace checks against, so it says what the build accepts. \
         To learn to write an extension, read the `uze:author` Skill's extension reference \
         (`plugins/uze/skills/author/references/extension.md`); the machine-readable form, \
         with a worked \
         manifest per slot and per source kind, is [`schema.json`](schema.json).",
    );
    line("");
    line(
        "Owner: `crates/uze-contract` (`src/docs.rs`). A change to the contract changes this \
         page in the same commit, or the root suite's drift test fails.",
    );
    line("");
    line("## Protocol");
    line("");
    line(&format!(
        "- `protocol {PROTOCOL}`: a manifest declaring another is refused with both numbers."
    ));
    line(&format!(
        "- An id is lowercase letters, digits and single hyphens, starting with a letter, at \
         most {} long. Reserved: {}.",
        crate::ID_MAX,
        code_list(RESERVED_IDS.iter().copied())
    ));
    line("");
    table(
        &mut line,
        "## Slots",
        &["slot", "where"],
        SLOTS
            .iter()
            .map(|slot| vec![code(slot.name()), slot_doc(*slot).to_owned()]),
    );
    expect_names(
        "source kind",
        SourceKind::NAMES,
        SOURCE_KINDS.iter().map(|row| row.0),
    );
    table(
        &mut line,
        "## Sources",
        &["kind", "written", "answered by"],
        SOURCE_KINDS
            .iter()
            .map(|(name, written, doc)| vec![code(name), (*written).to_owned(), (*doc).to_owned()]),
    );
    line(
        "A source also takes `input name=\"<template>\"…`, `every \"<n>s|m|h\"` (at least \
         5s), `refresh-on \"<event>\" <paths…>`, `output`, `deliver`, `store`, \
         `preferences` and `applies-when \"<path>\"`: a source whose path the checkout \
         lacks is never asked. A surface takes the same as a property, \
         `applies-when=\"<path>\"`, and is then offered only in a checkout that has it.",
    );
    line("");
    expect_names("output", Output::NAMES, OUTPUTS.iter().map(|row| row.0));
    table(
        &mut line,
        "### `output`",
        &["output", "how standard output is read"],
        OUTPUTS
            .iter()
            .map(|(name, doc)| vec![code(name), (*doc).to_owned()]),
    );
    expect_names(
        "deliver",
        Deliver::NAMES,
        DELIVERIES.iter().map(|row| row.0),
    );
    table(
        &mut line,
        "### `deliver`",
        &["deliver", "how events reach the source"],
        DELIVERIES
            .iter()
            .map(|(name, doc)| vec![code(name), (*doc).to_owned()]),
    );
    let grants = Grant::FIXED
        .iter()
        .cloned()
        .chain([Grant::Exec("<program>".into()), Grant::Net("<host>".into())]);
    table(
        &mut line,
        "## Grants",
        &["grant", "reaches"],
        grants.map(|grant| vec![code(&grant.name()), grant_doc(&grant).to_owned()]),
    );
    let events = EVENTS
        .iter()
        .map(|(name, doc)| vec![code(name), (*doc).to_owned()])
        .chain(AgentEvent::ALL.iter().map(|event| {
            vec![
                code(event.name()),
                format!("{} Needs `agent:observe`.", agent_event_doc(*event)),
            ]
        }));
    table(
        &mut line,
        "## Events (`refresh-on`)",
        &["event", "when"],
        events,
    );
    expect_names("action", Does::NAMES, DOES.iter().map(|row| row.0));
    table(
        &mut line,
        "## Actions",
        &["does", "written", "what"],
        DOES.iter()
            .map(|(name, written, doc)| vec![code(name), (*written).to_owned(), (*doc).to_owned()]),
    );
    line(&format!(
        "An action is `action \"<id>\" label=… on=\"<surface>\" key=… confirm=… {{ <does> }}`. \
         `key` is a suggestion, modifiers joined by `-` ({}) before a printable character or \
         one of {}. An action with `confirm`, and one that reaches an agent, needs a modifier. \
         Inside its block, `with \"ui.<id>\" \"form.<id>\" \"form.*\"` names the UI state and \
         form fields its input receives, and after a `run` or `script`, \
         `then {{ agent-prompt \"…{{answer.<field>}}…\" target=… }}` types a prompt filled from \
         the first step's answer, on a gesture only, under `agent:prompt`.",
        code_list(KeySuggestion::MODIFIERS.iter().copied()),
        code_list(KeySuggestion::NAMED.iter().copied())
    ));
    line("");
    expect_names(
        "envelope member",
        Envelope::MEMBERS,
        MEMBERS.iter().map(|row| row.0),
    );
    table(
        &mut line,
        "## The envelope",
        &["member", "what"],
        MEMBERS
            .iter()
            .map(|(name, doc)| vec![code(name), (*doc).to_owned()]),
    );
    line(
        "A script answers the envelope; a program answers it or a bare value, read as its \
         `data`. Any other member fails the answer.",
    );
    line("");
    expect_names(
        "preference kind",
        PreferenceKind::NAMES,
        PREFERENCE_KINDS.iter().map(|row| row.0),
    );
    table(
        &mut line,
        "## Preferences",
        &["kind", "holds"],
        PREFERENCE_KINDS
            .iter()
            .map(|(name, doc)| vec![code(name), (*doc).to_owned()]),
    );
    catalog(&mut line);
    out
}

fn catalog(line: &mut impl FnMut(&str)) {
    line("## Catalog");
    line("");
    line(
        "A view names these nodes, PascalCase. A property's value is a literal, a `{field}` \
         reference or text mixing both, except a `name`, which is a literal: identity never \
         depends on data. In a composed answer every value is a literal.",
    );
    line("");
    table(
        line,
        "### Properties every node takes",
        &["property", "value", "what", "composed"],
        COMMON_PROPS.iter().map(|(name, kind)| {
            vec![
                code(name),
                kind_name(*kind),
                crate::catalog::prop_doc(name)
                    .unwrap_or_default()
                    .to_owned(),
                if COMPOSED_PROPS.contains(name) {
                    "yes"
                } else {
                    "no"
                }
                .to_owned(),
            ]
        }),
    );
    for spec in CATALOG {
        line(&format!("### `{}`", spec.name));
        line("");
        line(spec.name.doc());
        line("");
        if !spec.args.is_empty() {
            let args = spec
                .args
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    let optional = if index < spec.required_args {
                        ""
                    } else {
                        ", optional"
                    };
                    format!("{}{optional}", kind_name(*kind))
                })
                .collect::<Vec<_>>()
                .join("; ");
            line(&format!("- Arguments: {args}."));
        }
        match spec.children {
            None => line("- Holds no other node."),
            Some(children) => line(&format!(
                "- Holds: {}.",
                code_list(children.iter().map(|child| child.as_str()))
            )),
        }
        line("");
        if !spec.props.is_empty() {
            let mut rows = Vec::new();
            for (name, kind) in spec.props {
                rows.push(vec![
                    code(name),
                    kind_name(*kind),
                    crate::catalog::prop_doc(name)
                        .unwrap_or_default()
                        .to_owned(),
                ]);
            }
            write_table(line, &["property", "value", "what"], rows.into_iter());
        }
    }
    table(
        line,
        "### Roles",
        &["role", "when"],
        RoleName::ALL
            .iter()
            .map(|role| vec![code(role.as_str()), role.doc().to_owned()]),
    );
}

fn table(
    line: &mut impl FnMut(&str),
    heading: &str,
    columns: &[&str],
    rows: impl Iterator<Item = Vec<String>>,
) {
    line(heading);
    line("");
    write_table(line, columns, rows);
}

fn write_table(
    line: &mut impl FnMut(&str),
    columns: &[&str],
    rows: impl Iterator<Item = Vec<String>>,
) {
    line(&format!("| {} |", columns.join(" | ")));
    line(&format!("|{}", "---|".repeat(columns.len())));
    for row in rows {
        let mut text = String::from("|");
        for cell in row {
            let _ = write!(text, " {} |", cell.replace('|', "\\|"));
        }
        line(&text);
    }
    line("");
}

fn code(text: &str) -> String {
    format!("`{text}`")
}

fn code_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    items.map(code).collect::<Vec<_>>().join(", ")
}

/// Every name the contract accepts has a line here, and no line names
/// one it does not: a word added to the contract without its line stops
/// the reference from being written.
fn expect_names<'a>(what: &str, names: &[&str], documented: impl Iterator<Item = &'a str>) {
    let documented: Vec<&str> = documented.collect();
    assert_eq!(
        names, documented,
        "every {what} the contract names needs its line in uze-contract's docs.rs, in order"
    );
}
