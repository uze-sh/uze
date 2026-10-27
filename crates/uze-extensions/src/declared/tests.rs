//! The binder against compiled views built by hand, one rule at a time.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use uze_contract::binding::{binding_of, parse_text};
use uze_contract::compiled::{Lookup, Node, Span as SourceSpan};
use uze_contract::{
    Binding, CompiledExtension, CompiledView, Condition, Identity, Item, Manifest, NodeName, Path,
    Slot, SourceDecl, SourceKind, SurfaceDecl,
};

use super::*;
use crate::view::{self, NodeKind};

pub(super) const VIEW: &str = "board";
pub(super) const SECTION: &str = "band";
pub(super) const SOURCE: &str = "board";

/// A compiled node, built the way the decoder would hand it over.
pub(super) struct N(Node);

pub(super) fn n(name: NodeName) -> N {
    N(Node {
        name,
        args: Vec::new(),
        props: BTreeMap::new(),
        each: None,
        condition: None,
        children: Vec::new(),
        span: SourceSpan::default(),
    })
}

fn bound(source: &str) -> Binding {
    binding_of(parse_text(source).expect("a view's text parses"))
}

fn path(source: &str) -> Path {
    Path::parse(source).expect("a reference parses")
}

impl N {
    pub fn arg(mut self, source: &str) -> Self {
        self.0.args.push(bound(source));
        self
    }

    pub fn prop(mut self, name: &str, source: &str) -> Self {
        self.0.props.insert(name.to_owned(), bound(source));
        self
    }

    pub fn literal(mut self, name: &str, value: Value) -> Self {
        self.0
            .props
            .insert(name.to_owned(), Binding::Literal(value));
        self
    }

    pub fn each(mut self, list: &str) -> Self {
        self.0.each = Some(path(list));
        self
    }

    pub fn when(mut self, field: &str) -> Self {
        self.0.condition = Some(Condition::When(path(field)));
        self
    }

    pub fn unless(mut self, field: &str) -> Self {
        self.0.condition = Some(Condition::Unless(path(field)));
        self
    }

    pub fn show(mut self, id: &str, value: &str) -> Self {
        self.0.condition = Some(Condition::Show {
            id: id.to_owned(),
            value: value.to_owned(),
        });
        self
    }

    pub fn show_any(mut self, pairs: &[(&str, &str)]) -> Self {
        let pairs = pairs
            .iter()
            .map(|(id, value)| ((*id).to_owned(), (*value).to_owned()))
            .collect();
        self.0.condition = Some(Condition::ShowAny(pairs));
        self
    }

    pub fn child(mut self, child: N) -> Self {
        self.0.children.push(child.item());
        self
    }

    pub fn uses(mut self, template: &str) -> Self {
        self.0.children.push(Item::Use {
            template: template.to_owned(),
            span: SourceSpan::default(),
        });
        self
    }

    pub fn item(self) -> Item {
        Item::Node(self.0)
    }
}

pub(super) fn lookup(
    name: &str,
    locale: Option<&str>,
    default: Option<&str>,
    entries: &[(&str, &str)],
) -> Lookup {
    Lookup {
        name: name.to_owned(),
        locale: locale.map(str::to_owned),
        default: default.map(str::to_owned),
        entries: entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

pub(super) fn compiled_view(
    items: Vec<Item>,
    templates: Vec<(&str, Vec<Item>)>,
    lookups: Vec<Lookup>,
) -> CompiledView {
    CompiledView {
        file: "board.kdl".to_owned(),
        items,
        templates: templates
            .into_iter()
            .map(|(name, items)| (name.to_owned(), items))
            .collect(),
        lookups,
    }
}

/// One extension with the view on a `view` surface and a `section`.
pub(super) fn extension(view: CompiledView) -> CompiledExtension {
    let surface = |id: &str, slot| SurfaceDecl {
        band: None,
        id: id.to_owned(),
        slot,
        view: "board.kdl".to_owned(),
        source: SOURCE.to_owned(),
        title: Some("GSD".to_owned()),
    };
    CompiledExtension {
        identity: Identity {
            marketplace: "luizfbalves".to_owned(),
            plugin: "gsd".to_owned(),
            id: "gsd-band".to_owned(),
        },
        manifest: Manifest {
            protocol: uze_contract::PROTOCOL,
            id: "gsd-band".to_owned(),
            name: "GSD band".to_owned(),
            description: String::new(),
            surfaces: vec![surface(VIEW, Slot::View), surface(SECTION, Slot::Section)],
            wants: Vec::new(),
            sources: vec![SourceDecl::new(
                SOURCE,
                SourceKind::Fixture("fixtures/board.json".to_owned()),
            )],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
        },
        views: BTreeMap::from([("board.kdl".to_owned(), view)]),
    }
}

pub(super) fn hosted(items: Vec<Item>, data: Value) -> DeclaredExtension {
    hosted_view(compiled_view(items, Vec::new(), Vec::new()), data, "pt")
}

pub(super) fn hosted_view(view: CompiledView, data: Value, locale: &str) -> DeclaredExtension {
    let mut extension = DeclaredExtension::new(extension(view), locale);
    extension.set_data(SOURCE, Ok(data));
    extension
}

pub(super) const SPACE: Size = Size {
    width: 100,
    height: 30,
};

pub(super) fn tree(extension: &DeclaredExtension) -> view::Node {
    match extension.view(VIEW, SPACE).expect("a view surface").content {
        Content::Tree { root, .. } => root,
        other => panic!("not a tree: {other:?}"),
    }
}

/// Every node, depth first.
pub(super) fn nodes(root: &view::Node) -> Vec<&view::Node> {
    let mut out = vec![root];
    for child in &root.children {
        out.extend(nodes(child));
    }
    out
}

/// The text of every `Text`, in the order drawn.
pub(super) fn texts(root: &view::Node) -> Vec<String> {
    nodes(root)
        .into_iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Text { spans, .. } => {
                Some(spans.iter().map(|span| span.text.as_str()).collect())
            }
            _ => None,
        })
        .collect()
}

pub(super) fn first(root: &view::Node, kind: fn(&NodeKind) -> bool) -> view::Node {
    nodes(root)
        .into_iter()
        .find(|node| kind(&node.kind))
        .cloned()
        .expect("the tree holds a node of that kind")
}

pub(super) fn act(node: usize, act: NodeAct) -> ViewHit {
    ViewHit::Node { node, act }
}

#[test]
fn a_lone_reference_keeps_its_type_and_mixed_text_is_a_string() {
    let extension = hosted(
        vec![
            n(NodeName::Progress)
                .child(
                    n(NodeName::Segment)
                        .prop("weight", "{plans}")
                        .prop("ratio", "{ratio}"),
                )
                .item(),
            n(NodeName::Text).arg("{plans} planos").item(),
            n(NodeName::Pips)
                .prop("done", "{done}")
                .prop("total", "{plans}")
                .item(),
        ],
        json!({ "plans": 19, "ratio": 0.89, "done": "17" }),
    );
    let root = tree(&extension);
    let progress = first(&root, |kind| matches!(kind, NodeKind::Progress { .. }));
    assert_eq!(
        progress.kind,
        NodeKind::Progress {
            segments: vec![view::Segment {
                weight: 19,
                filled: 890,
                role: Role::Default
            }]
        }
    );
    assert_eq!(texts(&root), ["19 planos"]);
    let pips = first(&root, |kind| matches!(kind, NodeKind::Pips { .. }));
    assert_eq!(
        pips.kind,
        NodeKind::Pips {
            done: 17,
            total: 19
        }
    );
}

#[test]
fn mixed_text_interpolates_scalars_only() {
    let extension = hosted(
        vec![
            n(NodeName::Text)
                .arg("[{list}|{object}|{flag}|{none}|{gone}]")
                .item(),
        ],
        json!({ "list": [1, 2], "object": { "a": 1 }, "flag": true, "none": null }),
    );
    assert_eq!(texts(&tree(&extension)), ["[||true||]"]);
}

#[test]
fn a_missing_field_bound_whole_hides_its_node_and_says_so() {
    let extension = hosted(
        vec![
            n(NodeName::Text).arg("{gone}").item(),
            n(NodeName::Text).arg("kept {gone}").item(),
        ],
        json!({}),
    );
    assert_eq!(texts(&tree(&extension)), ["kept "]);
    let trace = extension.trace(VIEW).expect("a trace");
    assert_eq!(
        trace.items[0].absent,
        [Absence::Missing {
            field: "gone".into()
        }]
    );
    assert_eq!(trace.items[1].drawn, 1);
}

#[test]
fn when_and_unless_test_presence_only() {
    let items = ["absent", "null", "no", "empty", "zero", "none", "yes"]
        .iter()
        .flat_map(|field| {
            [
                n(NodeName::Text)
                    .arg(&format!("when {field}"))
                    .when(field)
                    .item(),
                n(NodeName::Text)
                    .arg(&format!("unless {field}"))
                    .unless(field)
                    .item(),
            ]
        })
        .collect();
    let extension = hosted(
        items,
        json!({ "null": null, "no": false, "empty": "", "zero": 0, "none": [], "yes": [0] }),
    );
    assert_eq!(
        texts(&tree(&extension)),
        [
            "unless absent",
            "unless null",
            "unless no",
            "unless empty",
            "unless zero",
            "unless none",
            "when yes"
        ]
    );
}

#[test]
fn show_compares_a_ui_value_and_the_first_tab_is_the_default() {
    let items = vec![
        n(NodeName::Tabs)
            .prop("id", "tab")
            .child(n(NodeName::Tab).arg("Fases").prop("value", "phases"))
            .child(n(NodeName::Tab).arg("Log").prop("value", "log"))
            .item(),
        n(NodeName::Text).arg("phases").show("tab", "phases").item(),
        n(NodeName::Text).arg("log").show("tab", "log").item(),
        n(NodeName::Text)
            .arg("either")
            .show_any(&[("tab", "log"), ("tab", "phases")])
            .item(),
        n(NodeName::Text)
            .arg("never")
            .show_any(&[("tab", "other")])
            .item(),
        n(NodeName::Text).arg("{ui.tab}").item(),
    ];
    let mut extension = hosted(items, json!({}));
    let root = tree(&extension);
    assert_eq!(texts(&root), ["phases", "either", "phases"]);
    let tabs = first(&root, |kind| matches!(kind, NodeKind::Tabs { .. }));
    let tab = |label: &str, active| view::Tab {
        label: label.into(),
        active,
    };
    assert_eq!(
        tabs.kind,
        NodeKind::Tabs {
            tabs: vec![tab("Fases", true), tab("Log", false)]
        }
    );

    assert_eq!(
        extension.hit(VIEW, act(tabs.id, NodeAct::Tab(1)), SPACE),
        Outcome::Stay
    );
    assert_eq!(texts(&tree(&extension)), ["log", "either", "log"]);
    let trace = extension.trace(VIEW).expect("a trace");
    assert_eq!(
        trace.items[1].absent,
        [Absence::Show {
            id: "tab".into(),
            value: "phases".into(),
            actual: Some("log".into())
        }]
    );
    assert_eq!(trace.items[4].absent, [Absence::ShowAny]);
}

#[test]
fn a_lookup_follows_the_locale_and_falls_back_to_its_default() {
    let view = |locale: &str| {
        let items = vec![
            n(NodeName::Text)
                .arg("{t::next} {phase}")
                .prop("role", "{tone:e}")
                .item(),
            n(NodeName::Badge)
                .arg("{t::clear}")
                .prop("role", "{tone:other}")
                .item(),
        ];
        let lookups = vec![
            lookup("tone", None, Some("muted"), &[("partial", "warning")]),
            lookup(
                "t",
                Some("pt"),
                None,
                &[("next", "executar fase"), ("clear", "limpar")],
            ),
            lookup(
                "t",
                Some("en"),
                None,
                &[("next", "run phase"), ("clear", "clear")],
            ),
        ];
        let data = json!({ "e": "partial", "other": "unheard-of", "phase": "12" });
        hosted_view(compiled_view(items, Vec::new(), lookups), data, locale)
    };
    let root = tree(&view("pt"));
    let text = first(&root, |kind| matches!(kind, NodeKind::Text { .. }));
    let NodeKind::Text { spans, .. } = &text.kind else {
        unreachable!()
    };
    assert_eq!(spans[0].text, "executar fase 12");
    assert_eq!(spans[0].role, Role::Warning);
    let badge = first(&root, |kind| matches!(kind, NodeKind::Badge { .. }));
    assert_eq!(
        badge.kind,
        NodeKind::Badge {
            text: "limpar".into(),
            role: Role::Muted
        }
    );
    assert_eq!(texts(&tree(&view("en"))), ["run phase 12"]);
    assert_eq!(texts(&tree(&view("fr"))), ["executar fase 12"]);
}

fn nested() -> (Vec<Item>, Vec<(&'static str, Vec<Item>)>) {
    let items = vec![
        n(NodeName::Stack)
            .each("groups")
            .child(n(NodeName::Text).arg("{name}"))
            .child(n(NodeName::Row).each("members").uses("member"))
            .item(),
    ];
    let templates = vec![(
        "member",
        vec![n(NodeName::Text).arg("{^.name}/{name}").item()],
    )];
    (items, templates)
}

#[test]
fn nested_each_and_a_template_reach_the_enclosing_item_with_a_caret() {
    let (items, templates) = nested();
    let data = json!({ "groups": [
        { "name": "a", "members": [{ "name": "x" }, { "name": "y" }] },
        { "name": "b", "members": [{ "name": "z" }] },
    ] });
    let extension = hosted_view(compiled_view(items, templates, Vec::new()), data, "pt");
    assert_eq!(texts(&tree(&extension)), ["a", "a/x", "a/y", "b", "b/z"]);
    let trace = extension.trace(VIEW).expect("a trace");
    assert_eq!(trace.items[0].drawn, 2);
    assert_eq!(trace.items[0].children[1].drawn, 3);
    assert_eq!(
        trace.items[0].children[1].children[0].entry,
        TraceEntry::Use("member".into())
    );
    assert_eq!(trace.templates["member"][0].drawn, 3);
}

#[test]
fn an_expansion_past_the_cap_is_folded_with_a_notice() {
    let (items, templates) = nested();
    let members: Vec<Value> = (0..100).map(|at| json!({ "name": at })).collect();
    let groups: Vec<Value> = (0..100)
        .map(|at| json!({ "name": at, "members": members }))
        .collect();
    let view = compiled_view(items, templates, Vec::new());
    let extension = hosted_view(view, json!({ "groups": groups }), "pt");
    let root = tree(&extension);
    assert!(nodes(&root).len() <= NODE_CAP + 2, "{}", nodes(&root).len());
    let notice = root.children.last().expect("a notice last");
    assert!(
        matches!(&notice.kind, NodeKind::Notice { text, role: Role::Warning } if text.contains("2000")),
        "{notice:?}"
    );
    let trace = extension.trace(VIEW).expect("a trace");
    assert!(trace.folded);
    fn capped(nodes: &[TraceNode]) -> bool {
        nodes
            .iter()
            .any(|node| node.absent.contains(&Absence::CapReached) || capped(&node.children))
    }
    assert!(capped(&trace.items));
}

#[test]
fn the_trace_says_why_each_node_is_absent() {
    let items = vec![
        n(NodeName::Button)
            .arg("go")
            .prop("action", "next")
            .when("next")
            .item(),
        n(NodeName::Text).arg("idle").unless("busy").item(),
        n(NodeName::Text).arg("{x}").each("missing").item(),
        n(NodeName::Text).arg("{x}").each("empty").item(),
        n(NodeName::Text).arg("{x}").each("scalar").item(),
        n(NodeName::Row)
            .when("off")
            .child(n(NodeName::Text).arg("inside"))
            .item(),
        n(NodeName::Stack).uses("nowhere").item(),
        n(NodeName::Mark).item(),
    ];
    let extension = hosted(
        items,
        json!({ "busy": true, "empty": [], "scalar": 3, "off": false }),
    );
    let trace = extension.trace(VIEW).expect("a trace");
    let reasons: Vec<&[Absence]> = trace.items.iter().map(|node| &node.absent[..]).collect();
    let field = |name: &str| name.to_owned();
    assert_eq!(
        reasons[..6],
        [
            &[Absence::When {
                field: field("next"),
                missing: true
            }][..],
            &[Absence::Unless {
                field: field("busy")
            }],
            &[Absence::EachMissing {
                field: field("missing")
            }],
            &[Absence::EachEmpty {
                field: field("empty")
            }],
            &[Absence::EachNotList {
                field: field("scalar")
            }],
            &[Absence::When {
                field: field("off"),
                missing: false
            }],
        ]
    );
    assert_eq!(trace.items[5].children[0].reached, 0, "inside a hidden row");
    assert_eq!(
        trace.items[6].children[0].absent,
        [Absence::UnknownTemplate]
    );
    assert_eq!(trace.items[7].absent, [Absence::Misplaced]);
    assert_eq!(trace.items[0].entry, TraceEntry::Node(NodeName::Button));
}

#[test]
fn a_pressed_button_records_its_action_with_the_item_and_runs_nothing() {
    let items = vec![
        n(NodeName::Button)
            .arg("run {next.phase}")
            .prop("action", "next")
            .prop("item", "{next}")
            .item(),
    ];
    let data = json!({ "next": { "phase": "12", "skill": "x" } });
    let mut extension = hosted(items, data.clone());
    let button = first(&tree(&extension), |kind| {
        matches!(kind, NodeKind::Button { .. })
    });
    assert_eq!(
        button.kind,
        NodeKind::Button {
            label: "run 12".into()
        }
    );
    let outcome = extension.hit(VIEW, act(button.id, NodeAct::Press), SPACE);
    assert!(matches!(&outcome, Outcome::Notice(span) if span.text == "next recorded, not run"));
    let expected = RecordedAction {
        surface: VIEW.into(),
        action: "next".into(),
        item: json!({ "phase": "12", "skill": "x" }),
        declared: false,
    };
    assert_eq!(extension.recorded(), std::slice::from_ref(&expected));
    assert_eq!(extension.trace(VIEW).expect("a trace").recorded, [expected]);
    assert_eq!(extension.trace(SECTION).expect("a trace").recorded, []);
    assert_eq!(extension.data(SOURCE), Some(&Ok(data)));
}

#[test]
fn a_key_on_a_declared_action_runs_it_where_it_is_offered() {
    let mut compiled = extension(compiled_view(Vec::new(), Vec::new(), Vec::new()));
    compiled.manifest.actions = vec![uze_contract::ActionDecl {
        id: "refresh".to_owned(),
        label: "Refresh".to_owned(),
        on: VIEW.to_owned(),
        key: None,
        confirm: None,
        does: uze_contract::Does::Refresh(vec![SOURCE.to_owned()]),
    }];
    let mut extension = DeclaredExtension::new(compiled, "en");
    assert_eq!(
        extension.press(SECTION, "refresh"),
        Outcome::Stay,
        "not offered there"
    );
    assert_eq!(
        extension.press(VIEW, "missing"),
        Outcome::Stay,
        "not declared"
    );
    assert!(extension.recorded().is_empty());
    assert_eq!(extension.press(VIEW, "refresh"), Outcome::Stay);
    assert_eq!(
        extension.recorded(),
        [RecordedAction {
            surface: VIEW.into(),
            action: "refresh".into(),
            item: Value::Null,
            declared: true,
        }]
    );
}

#[test]
fn a_toggle_flips_its_ui_value_from_the_declared_default() {
    let items = vec![
        n(NodeName::Toggle)
            .arg("detail")
            .prop("id", "detail")
            .literal("on", json!(true))
            .item(),
        n(NodeName::Text)
            .arg("shown")
            .show("detail", "false")
            .item(),
    ];
    let mut extension = hosted(items, json!({}));
    let is_toggle = |kind: &NodeKind| matches!(kind, NodeKind::Toggle { .. });
    let toggle = first(&tree(&extension), is_toggle);
    assert_eq!(
        toggle.kind,
        NodeKind::Toggle {
            label: "detail".into(),
            on: true
        }
    );
    extension.hit(VIEW, act(toggle.id, NodeAct::Toggle), SPACE);
    assert_eq!(extension.ui(VIEW).expect("ui state")["detail"], "false");
    let root = tree(&extension);
    assert_eq!(texts(&root), ["shown"]);
    assert!(matches!(
        first(&root, is_toggle).kind,
        NodeKind::Toggle { on: false, .. }
    ));
}

#[test]
fn a_list_repeats_its_rows_and_holds_its_selection() {
    let items = vec![
        n(NodeName::List)
            .prop("id", "phase")
            .prop("empty", "nothing")
            .each("phases")
            .child(n(NodeName::Text).arg("{name}"))
            .item(),
    ];
    let is_list = |kind: &NodeKind| matches!(kind, NodeKind::List { .. });
    let data = json!({ "phases": [{ "name": "a" }, { "name": "b" }] });
    let mut extension = hosted(items.clone(), data);
    let list = first(&tree(&extension), is_list);
    assert_eq!(list.children.len(), 2);
    assert_eq!(
        list.kind,
        NodeKind::List {
            selected: None,
            empty: Some("nothing".into())
        }
    );
    extension.hit(VIEW, act(list.id, NodeAct::Row(1)), SPACE);
    assert!(matches!(
        first(&tree(&extension), is_list).kind,
        NodeKind::List {
            selected: Some(1),
            ..
        }
    ));
    extension.hit(VIEW, act(list.id, NodeAct::Row(7)), SPACE);
    assert_eq!(extension.ui(VIEW).expect("ui state")["phase"], "1");
    extension.command(VIEW, Command::SelectPrevious, SPACE);
    assert_eq!(extension.ui(VIEW).expect("ui state")["phase"], "0");

    let empty = hosted(items, json!({ "phases": [] }));
    assert!(first(&tree(&empty), is_list).children.is_empty());
    assert_eq!(
        empty.trace(VIEW).expect("a trace").items[0].absent,
        [Absence::EachEmpty {
            field: "phases".into()
        }]
    );
}

#[test]
fn every_node_kind_reads_its_properties() {
    let items = vec![
        n(NodeName::Row)
            .prop("gap", "2")
            .prop("align", "end")
            .child(
                n(NodeName::Text)
                    .arg("{name}")
                    .literal("strong", json!(true))
                    .literal("truncate", json!(true))
                    .literal("grow", json!(1))
                    .literal("min-width", json!(3))
                    .prop("role", "accent"),
            )
            .child(
                n(NodeName::Marks)
                    .child(n(NodeName::Mark).prop("role", "success"))
                    .child(n(NodeName::Mark).prop("role", "{state}")),
            )
            .child(
                n(NodeName::Progress)
                    .prop("ratio", "{over}")
                    .prop("role", "info"),
            )
            .child(n(NodeName::Activity))
            .child(
                n(NodeName::Elapsed)
                    .prop("since", "{started}")
                    .prop("role", "dim"),
            )
            .child(n(NodeName::Elapsed).prop("since", "soon"))
            .item(),
        n(NodeName::Table)
            .arg("Fase")
            .arg("{label}")
            .prop("empty", "none")
            .child(
                n(NodeName::Row)
                    .each("rows")
                    .child(n(NodeName::Text).arg("{n}")),
            )
            .item(),
        n(NodeName::Log).prop("lines", "{log}").item(),
    ];
    let data = json!({
        "name": "Fase 12", "state": "warning", "over": 1.7, "label": "Estado",
        "started": "2023-11-14T22:13:20Z", "rows": [{ "n": 1 }, { "n": 2 }],
        "log": ["one", { "text": "two\nthree", "role": "danger" }],
    });
    let root = tree(&hosted(items, data));
    let row = &root.children[0];
    assert_eq!(
        row.kind,
        NodeKind::Row {
            gap: 2,
            align: view::Align::End
        }
    );
    let text = &row.children[0];
    assert!(text.grow);
    assert_eq!(text.min_width, 3);
    let NodeKind::Text { spans, truncate } = &text.kind else {
        panic!("{text:?}")
    };
    assert!(*truncate && spans[0].bold && spans[0].role == Role::Accent);
    assert_eq!(
        row.children[1].kind,
        NodeKind::Marks(vec![Role::Success, Role::Warning])
    );
    assert_eq!(
        row.children[2].kind,
        NodeKind::Progress {
            segments: vec![view::Segment {
                weight: 1,
                filled: 1000,
                role: Role::Info
            }]
        }
    );
    assert_eq!(row.children[3].kind, NodeKind::Activity { busy: false });
    assert_eq!(
        row.children[4].kind,
        NodeKind::Elapsed {
            since_ms: Some(1_700_000_000_000),
            role: Role::Dim
        }
    );
    assert_eq!(
        row.children[5].kind,
        NodeKind::Elapsed {
            since_ms: None,
            role: Role::Default
        }
    );
    let table = &root.children[1];
    assert_eq!(
        table.kind,
        NodeKind::Table {
            columns: vec!["Fase".into(), "Estado".into()],
            empty: Some("none".into())
        }
    );
    assert_eq!(table.children.len(), 2);
    let NodeKind::Log { lines } = &root.children[2].kind else {
        panic!("{:?}", root.children[2])
    };
    let lines: Vec<(&str, Role)> = lines
        .iter()
        .map(|span| (span.text.as_str(), span.role))
        .collect();
    assert_eq!(
        lines,
        [
            ("one", Role::Default),
            ("two", Role::Danger),
            ("three", Role::Danger)
        ]
    );
}

#[test]
fn an_escape_sequence_in_data_is_drawn_inert() {
    let osc = "\u{1b}]52;c;ZXZpbA==\u{7}build";
    let items = vec![
        n(NodeName::Text).arg("{name}").item(),
        n(NodeName::Badge).arg("{name} \u{202e}x").item(),
        n(NodeName::Log).prop("lines", "{lines}").item(),
    ];
    let root = tree(&hosted(items, json!({ "name": osc, "lines": [osc] })));
    assert_eq!(texts(&root), ["build"]);
    assert_eq!(
        root.children[1].kind,
        NodeKind::Badge {
            text: "build x".into(),
            role: Role::Default
        }
    );
    assert_eq!(
        root.children[2].kind,
        NodeKind::Log {
            lines: vec![view::Span::new("build", Role::Default)]
        }
    );
}

#[test]
fn ids_are_unique_and_stable_for_the_same_view_and_data() {
    let (items, templates) = nested();
    let data = json!({ "groups": [{ "name": "a", "members": [{ "name": "x" }] }] });
    let ids = || -> Vec<usize> {
        let view = compiled_view(items.clone(), templates.clone(), Vec::new());
        let root = tree(&hosted_view(view, data.clone(), "pt"));
        nodes(&root).iter().map(|node| node.id).collect()
    };
    let mut unique = ids();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids().len());
    assert_eq!(ids(), ids());
}

#[test]
fn a_build_is_served_again_until_its_inputs_change() {
    let mut extension = hosted(vec![n(NodeName::Text).arg("{a}").item()], json!({ "a": 1 }));
    tree(&extension);
    tree(&extension);
    assert_eq!(extension.builds(), 1);
    extension.set_data(SOURCE, Ok(json!({ "a": 1 })));
    tree(&extension);
    assert_eq!(extension.builds(), 1, "the same data is the same digest");
    let narrow = Size {
        width: 40,
        height: 10,
    };
    extension.view(VIEW, narrow);
    assert_eq!(extension.builds(), 2, "another size is another build");
    extension.set_data(SOURCE, Ok(json!({ "a": 2 })));
    assert_eq!(texts(&tree(&extension)), ["2"]);
    assert_eq!(extension.builds(), 3);
}

#[test]
fn a_surface_waits_for_its_data_and_says_why_it_has_none() {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("x").item()],
        Vec::new(),
        Vec::new(),
    );
    let mut extension = DeclaredExtension::new(extension(view), "pt");
    assert!(matches!(
        tree(&extension).kind,
        NodeKind::Notice {
            role: Role::Muted,
            ..
        }
    ));
    let trace = extension.trace(VIEW).expect("a trace");
    assert_eq!(trace.data, DataState::Waiting);
    assert_eq!(trace.items[0].reached, 0);
    extension.set_data(SOURCE, Err("board.json: \u{1b}[2Jno such file".into()));
    assert_eq!(
        tree(&extension).kind,
        NodeKind::Notice {
            text: "board.json: no such file".into(),
            role: Role::Danger
        }
    );
}

#[test]
fn each_slot_answers_only_as_itself() {
    let mut extension = hosted(vec![n(NodeName::Text).arg("x").item()], json!({}));
    assert!(extension.section(VIEW, false).is_none());
    assert!(extension.view(SECTION, SPACE).is_none());
    let section = extension.section(SECTION, true).expect("a section");
    assert_eq!(section.title, "GSD");
    assert!(section.collapsed && section.rows.is_empty());
    assert_eq!(texts(&section.tree.expect("a tree")), ["x"]);
    let view = extension.view(VIEW, SPACE).expect("a view");
    assert_eq!(view.footer, [Command::Close]);
    assert_eq!(view.title[0].text, "GSD");
    assert!(view.navigator.is_none());
    assert_eq!(
        extension.command(VIEW, Command::Close, SPACE),
        Outcome::Close
    );
    assert_eq!(extension.hit(VIEW, ViewHit::Close, SPACE), Outcome::Close);
    assert!(extension.view("nowhere", SPACE).is_none());
    assert!(extension.trace("nowhere").is_none());
}

#[test]
fn a_drawer_answers_a_tree_view_as_a_view_does() {
    let mut compiled = extension(compiled_view(
        vec![n(NodeName::Text).arg("x").item()],
        Vec::new(),
        Vec::new(),
    ));
    compiled.manifest.surfaces[1].slot = Slot::Drawer;
    let mut extension = DeclaredExtension::new(compiled, "pt");
    extension.set_data(SOURCE, Ok(json!({})));
    let drawer = &extension.surfaces()[1];
    assert_eq!(drawer.slot, SurfaceSlot::Drawer);
    assert!(extension.section(SECTION, false).is_none());
    let view = extension.view(SECTION, SPACE).expect("a drawer is a view");
    assert_eq!(view.title[0].text, "GSD");
    let Content::Tree { root, .. } = view.content else {
        panic!("a drawer is a tree");
    };
    assert_eq!(texts(&root), ["x"]);
}

#[test]
fn a_forgotten_surface_starts_over() {
    let mut extension = hosted(tabbed(), json!({}));
    extension.command(VIEW, Command::NextView, SPACE);
    assert!(extension.ui(VIEW).is_some_and(|ui| !ui.is_empty()));
    extension.forget(VIEW);
    assert!(extension.ui(VIEW).is_none());
}

#[test]
fn a_scrollbar_held_at_a_row_scrolls_there_and_no_further_than_the_tree() {
    let lines = (0..6)
        .map(|line| n(NodeName::Text).arg(&format!("line {line}")).item())
        .collect();
    let mut extension = hosted(lines, json!({}));
    let scroll = |extension: &DeclaredExtension| match extension.view(VIEW, SPACE) {
        Some(View {
            content: Content::Tree { scroll, .. },
            ..
        }) => scroll,
        _ => panic!("a tree"),
    };
    assert_eq!(scroll(&extension), 0);
    extension.scroll_to(VIEW, 3, SPACE);
    assert_eq!(scroll(&extension), 3);
    extension.scroll_to(VIEW, 99, SPACE);
    assert_eq!(scroll(&extension), 6);
}

fn tabbed() -> Vec<Item> {
    vec![
        n(NodeName::Tabs)
            .prop("id", "tab")
            .child(n(NodeName::Tab).arg("A").prop("value", "a"))
            .child(n(NodeName::Tab).arg("B").prop("value", "b"))
            .child(n(NodeName::Tab).arg("C").prop("value", "c"))
            .item(),
        n(NodeName::Text).arg("{ui.tab}").item(),
    ]
}

#[test]
fn next_and_previous_view_cycle_the_first_tabs() {
    let mut extension = hosted(tabbed(), json!({}));
    extension.command(VIEW, Command::PreviousView, SPACE);
    assert_eq!(texts(&tree(&extension)), ["c"]);
    extension.command(VIEW, Command::NextView, SPACE);
    extension.command(VIEW, Command::NextView, SPACE);
    assert_eq!(texts(&tree(&extension)), ["b"]);
}

#[test]
fn a_place_carries_the_ui_state_and_a_newer_compile_keeps_the_ids_it_still_has() {
    let mut extension = hosted(tabbed(), json!({}));
    extension.command(VIEW, Command::NextView, SPACE);
    let place = extension.place(VIEW).expect("a place");
    let mut reopened = hosted(tabbed(), json!({}));
    reopened.resume(VIEW, place);
    assert_eq!(texts(&tree(&reopened)), ["b"]);

    reopened.replace(extension_of(tabbed()));
    assert_eq!(texts(&tree(&reopened)), ["b"]);
    reopened.replace(extension_of(vec![n(NodeName::Text).arg("{ui.tab}").item()]));
    assert!(reopened.ui(VIEW).expect("ui state").is_empty());
}

fn extension_of(items: Vec<Item>) -> CompiledExtension {
    extension(compiled_view(items, Vec::new(), Vec::new()))
}

#[test]
fn memory_carries_the_recorded_actions() {
    let items = vec![n(NodeName::Button).arg("go").prop("action", "go").item()];
    let mut extension = hosted(items.clone(), json!({}));
    let button = first(&tree(&extension), |kind| {
        matches!(kind, NodeKind::Button { .. })
    });
    extension.hit(VIEW, act(button.id, NodeAct::Press), SPACE);
    let memory = extension.memory().expect("memory");
    let mut again = hosted(items, json!({}));
    again.restore_memory(memory);
    assert_eq!(again.recorded().len(), 1);
}

#[test]
fn a_wrapping_text_is_a_paragraph_and_is_not_cut_at_a_labels_length() {
    let long = "word ".repeat(120).trim_end().to_owned();
    let extension = hosted(
        vec![
            n(NodeName::Text).arg("{note}").item(),
            n(NodeName::Text)
                .arg("{note}")
                .literal("truncate", json!(true))
                .item(),
        ],
        json!({ "note": long }),
    );
    let root = tree(&extension);
    let said: Vec<String> = root
        .children
        .iter()
        .map(|node| match &node.kind {
            NodeKind::Text { spans, .. } => spans.iter().map(|span| span.text.clone()).collect(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(said[0], long, "a paragraph keeps every word");
    assert_eq!(
        said[1].chars().count(),
        uze_contract::clean::LABEL_MAX,
        "a label is cut at a label's length"
    );
}

#[test]
fn a_field_under_a_second_tab_is_found_missing_and_an_answered_when_is_not() {
    let view = compiled_view(
        vec![
            n(NodeName::Tabs)
                .prop("id", "tab")
                .child(n(NodeName::Tab).arg("Phases").prop("value", "phases"))
                .child(n(NodeName::Tab).arg("Log").prop("value", "log"))
                .item(),
            n(NodeName::Text)
                .arg("{phases}")
                .show("tab", "phases")
                .item(),
            n(NodeName::Log)
                .prop("lines", "{log}")
                .show("tab", "log")
                .item(),
            n(NodeName::List)
                .child(
                    n(NodeName::Text)
                        .each("servers")
                        .arg("{tools}")
                        .when("tools"),
                )
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let fixtures = BTreeMap::from([(
        SOURCE.to_owned(),
        json!({"phases": 3, "servers": [{"tools": "6 tools"}, {"name": "sentry"}]}),
    )]);
    let missing = super::missing_fields(&extension(view), &fixtures, "en");
    let fields: Vec<&str> = missing.iter().map(|field| field.field.as_str()).collect();
    assert_eq!(fields, ["log"], "{missing:#?}");
    assert_eq!(missing[0].fixture, "fixtures/board.json");
}

#[test]
fn too_many_tab_combinations_bind_each_tab_alone() {
    let tabs = |id: &str| {
        let mut tabs = n(NodeName::Tabs).prop("id", id);
        for value in ["a", "b", "c", "d", "e"] {
            tabs = tabs.child(n(NodeName::Tab).arg(value).prop("value", value));
        }
        tabs.item()
    };
    let mut items: Vec<Item> = ["one", "two", "three"].iter().map(|id| tabs(id)).collect();
    items.push(n(NodeName::Text).arg("{deep}").show("three", "e").item());
    let view = compiled_view(items, Vec::new(), Vec::new());
    let fixtures = BTreeMap::from([(SOURCE.to_owned(), json!({}))]);
    let missing = super::missing_fields(&extension(view), &fixtures, "en");
    assert!(
        missing.iter().any(|field| field.field == "deep"),
        "{missing:#?}"
    );
}

#[test]
fn a_gauge_turns_at_its_thresholds_and_a_chart_scales_to_its_largest() {
    let gauge = |used: f64| {
        let extension = hosted(
            vec![
                n(NodeName::Gauge)
                    .arg("Context")
                    .prop("ratio", "{used}")
                    .prop("role", "info")
                    .literal("warning", json!(0.7))
                    .literal("danger", json!(0.9))
                    .item(),
            ],
            json!({ "used": used }),
        );
        tree(&extension).children[0].kind.clone()
    };
    assert!(matches!(
        gauge(0.5),
        NodeKind::Gauge {
            role: Role::Info,
            filled: 500,
            ..
        }
    ));
    assert!(matches!(
        gauge(0.75),
        NodeKind::Gauge {
            role: Role::Warning,
            ..
        }
    ));
    assert!(matches!(
        gauge(0.93),
        NodeKind::Gauge {
            role: Role::Danger,
            filled: 930,
            ..
        }
    ));

    let extension = hosted(
        vec![
            n(NodeName::Chart)
                .prop("series", "{history}")
                .prop("kind", "bar")
                .item(),
        ],
        json!({ "history": [1, 4, 8, -3, "x"] }),
    );
    let NodeKind::Chart {
        kind,
        levels,
        height,
        ..
    } = tree(&extension).children[0].kind.clone()
    else {
        panic!("a chart");
    };
    assert_eq!(kind, view::ChartKind::Bar);
    assert_eq!(levels, [125, 500, 1000, 0, 0]);
    assert_eq!(height, 4, "the default height of bars");
}

#[test]
fn details_stats_rules_blocks_and_keys_bind_their_arguments() {
    let extension = hosted(
        vec![
            n(NodeName::Details)
                .child(
                    n(NodeName::Entry)
                        .arg("Plan")
                        .arg("{plan}")
                        .prop("role", "info"),
                )
                .item(),
            n(NodeName::Stat)
                .arg("{cost}")
                .arg("Today")
                .prop("delta", "+12%")
                .prop("role", "success")
                .item(),
            n(NodeName::Rule).arg("Blockers").item(),
            n(NodeName::Rule).item(),
            n(NodeName::Markdown).arg("{plan_text}").item(),
            n(NodeName::Code)
                .arg("{snippet}")
                .prop("language", "rust")
                .item(),
            n(NodeName::Kbd).arg("alt+k").item(),
            n(NodeName::Grid)
                .prop("gap", "2")
                .child(n(NodeName::Text).arg("a"))
                .item(),
        ],
        json!({ "plan": "Max 5x", "cost": "$15.48", "plan_text": "## Plan\n- one", "snippet": "fn main() {}\n" }),
    );
    let root = tree(&extension);
    let kinds: Vec<NodeKind> = root.children.iter().map(|node| node.kind.clone()).collect();
    assert_eq!(kinds[0], NodeKind::Details);
    assert_eq!(
        root.children[0].children[0].kind,
        NodeKind::Entry {
            key: "Plan".into(),
            value: "Max 5x".into(),
            role: Role::Info
        }
    );
    assert_eq!(
        kinds[1],
        NodeKind::Stat {
            value: "$15.48".into(),
            label: "Today".into(),
            delta: Some("+12%".into()),
            role: Role::Success
        }
    );
    assert_eq!(
        kinds[2],
        NodeKind::Rule {
            label: Some("Blockers".into()),
            role: Role::Default
        }
    );
    assert_eq!(
        kinds[3],
        NodeKind::Rule {
            label: None,
            role: Role::Default
        }
    );
    assert_eq!(
        kinds[4],
        NodeKind::Markdown {
            text: "## Plan\n- one".into()
        },
        "lines kept"
    );
    assert_eq!(
        kinds[5],
        NodeKind::Code {
            text: "fn main() {}\n".into(),
            language: "rust".into()
        }
    );
    assert_eq!(
        kinds[6],
        NodeKind::Kbd {
            key: "alt+k".into()
        }
    );
    assert_eq!(kinds[7], NodeKind::Grid { gap: 2 });
}

#[test]
fn tabs_without_values_are_bound_by_their_labels_and_open_on_the_first() {
    let view = compiled_view(
        vec![
            n(NodeName::Tabs)
                .prop("id", "mode")
                .child(n(NodeName::Tab).arg("A"))
                .child(n(NodeName::Tab).arg("B").prop("value", "b"))
                .item(),
            n(NodeName::Text).arg("{titel}").show("mode", "A").item(),
            n(NodeName::Text).arg("{other}").show("mode", "b").item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let fixtures = BTreeMap::from([(SOURCE.to_owned(), json!({}))]);
    let missing = super::missing_fields(&extension(view.clone()), &fixtures, "en");
    let fields: Vec<&str> = missing.iter().map(|field| field.field.as_str()).collect();
    assert_eq!(fields, ["titel", "other"], "{missing:#?}");

    let extension = hosted_view(view, json!({"titel": "first", "other": "second"}), "en");
    assert_eq!(
        texts(&tree(&extension)),
        ["first"],
        "the first tab is the one drawn"
    );
}

#[test]
fn a_log_keeps_its_latest_lines_and_counts_them_against_the_cap() {
    let lines: Vec<String> = (0..10_000).map(|line| format!("line {line}")).collect();
    let extension = hosted(
        vec![n(NodeName::Log).prop("lines", "{log}").item()],
        json!({ "log": lines }),
    );
    let NodeKind::Log { lines } = tree(&extension).children[0].kind.clone() else {
        panic!("a log");
    };
    assert_eq!(lines.len(), 500);
    assert_eq!(
        lines.last().map(|span| span.text.as_str()),
        Some("line 9999")
    );

    let items: Vec<Value> = (0..100).map(|_| json!({})).collect();
    let log: Vec<String> = (0..500).map(|line| format!("{line}")).collect();
    let multiplied = hosted(
        vec![
            n(NodeName::Stack)
                .child(n(NodeName::Log).each("items").prop("lines", "{^.log}"))
                .item(),
        ],
        json!({ "items": items, "log": log }),
    );
    let drawn: usize = nodes(&tree(&multiplied))
        .iter()
        .map(|node| match &node.kind {
            NodeKind::Log { lines } => lines.len(),
            _ => 1,
        })
        .sum();
    assert!(drawn > 1_000, "the logs were drawn: {drawn}");
    assert!(
        drawn <= super::NODE_CAP + 500,
        "bounded by the cap: {drawn}"
    );
}

#[test]
fn a_template_brought_tab_leaves_the_others_checked() {
    let view = compiled_view(
        vec![
            n(NodeName::Tabs)
                .prop("id", "tab")
                .child(n(NodeName::Tab).arg("Tasks").prop("value", "tasks"))
                .child(n(NodeName::Tab).arg("Notes").prop("value", "notes"))
                .uses("more")
                .item(),
            n(NodeName::Text)
                .arg("{nosuchfield}")
                .show("tab", "notes")
                .item(),
        ],
        vec![(
            "more",
            vec![n(NodeName::Tab).arg("Extra").prop("value", "extra").item()],
        )],
        Vec::new(),
    );
    let fixtures = BTreeMap::from([(SOURCE.to_owned(), json!({}))]);
    let missing = super::missing_fields(&extension(view), &fixtures, "en");
    assert!(
        missing.iter().any(|field| field.field == "nosuchfield"),
        "{missing:#?}"
    );
}
