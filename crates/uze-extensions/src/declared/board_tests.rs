//! The gallery's `gsd-band` board, compiled by hand from the blueprint's
//! `board.kdl` and bound to its `fixtures/board.json`.

use serde_json::{Value, json};
use uze_contract::{CompiledView, Item, NodeName};

use super::tests::{
    SECTION, SPACE, VIEW, act, compiled_view, first, hosted_view, lookup, n, nodes, texts, tree,
};
use super::*;
use crate::view::{self, NodeKind};

fn phase_row() -> Vec<Item> {
    vec![
        n(NodeName::Row)
            .prop("gap", "1")
            .child(
                n(NodeName::Text)
                    .arg("{number}")
                    .literal("min-width", json!(3)),
            )
            .child(
                n(NodeName::Text)
                    .arg("{name}")
                    .literal("grow", json!(1))
                    .literal("truncate", json!(true)),
            )
            .child(
                n(NodeName::Marks)
                    .child(n(NodeName::Mark).prop("role", "{tone:d}"))
                    .child(n(NodeName::Mark).prop("role", "{tone:p}"))
                    .child(n(NodeName::Mark).prop("role", "{tone:e}")),
            )
            .child(
                n(NodeName::Pips)
                    .prop("done", "{summaries}")
                    .prop("total", "{plans}")
                    .when("active"),
            )
            .child(
                n(NodeName::Text)
                    .arg("{summaries}/{plans}")
                    .prop("role", "muted")
                    .when("finished"),
            )
            .child(n(NodeName::Text).arg("{status}").prop("role", "{tone:e}"))
            .item(),
    ]
}

fn board() -> CompiledView {
    let header = n(NodeName::Row)
        .prop("align", "center")
        .child(n(NodeName::Activity).prop("agent", "self"))
        .child(
            n(NodeName::Text)
                .arg("GSD · {version} {name}")
                .prop("role", "muted")
                .literal("grow", json!(1)),
        )
        .child(
            n(NodeName::Tabs)
                .prop("id", "tab")
                .child(n(NodeName::Tab).arg("{t::phases}").prop("value", "phases"))
                .child(n(NodeName::Tab).arg("{t::log}").prop("value", "log")),
        )
        .child(n(NodeName::Button).arg("{t::clear}").prop("action", "clear"))
        .child(
            n(NodeName::Button)
                .arg("{t::next} {next.phase}")
                .prop("action", "next")
                .prop("item", "{next}")
                .when("next"),
        );
    let measure = n(NodeName::Row)
        .prop("gap", "1")
        .child(
            n(NodeName::Progress).literal("grow", json!(1)).child(
                n(NodeName::Segment)
                    .each("phases")
                    .prop("weight", "{weight}")
                    .prop("ratio", "{ratio}")
                    .prop("role", "{tone:e}"),
            ),
        )
        .child(n(NodeName::Text).arg("{percent}%"))
        .child(
            n(NodeName::Text)
                .arg("· {done} de {total} fases")
                .prop("role", "muted"),
        );
    let items = vec![
        n(NodeName::Stack)
            .child(header)
            .child(measure)
            .child(
                n(NodeName::List)
                    .each("phases")
                    .show("tab", "phases")
                    .prop("empty", "{t::empty}")
                    .uses("phase-row"),
            )
            .child(n(NodeName::Log).prop("lines", "{log}").show("tab", "log"))
            .item(),
    ];
    let lookups = vec![
        lookup(
            "tone",
            None,
            Some("muted"),
            &[
                ("done", "success"),
                ("partial", "warning"),
                ("ready", "info"),
                ("blocked", "muted"),
            ],
        ),
        lookup(
            "t",
            Some("pt"),
            None,
            &[
                ("phases", "Fases"),
                ("log", "Log"),
                ("next", "▶ executar fase"),
                ("clear", "limpar"),
                ("empty", "Nenhuma fase em andamento"),
            ],
        ),
    ];
    compiled_view(items, vec![("phase-row", phase_row())], lookups)
}

fn fixture() -> Value {
    json!({
      "version": "v1.23",
      "name": "Assistente do lojista",
      "percent": 65, "done": 3, "total": 6,
      "next": { "phase": "12", "skill": "gsd:execute-phase" },
      "phases": [
        { "number": "11", "name": "Catálogo de ajuda", "plans": 6, "summaries": 6,
          "weight": 6, "ratio": 1.0, "d": "done", "p": "done", "e": "done",
          "finished": true, "active": false, "status": "Concluída" },
        { "number": "12", "name": "Consulta de dados e diagnóstico", "plans": 19, "summaries": 17,
          "weight": 19, "ratio": 0.89, "d": "done", "p": "done", "e": "partial",
          "finished": false, "active": true, "status": "Execução parcial (17/19)" },
        { "number": "13", "name": "Ações com confirmação", "plans": 0, "summaries": 0,
          "weight": 4, "ratio": 0.0, "d": "ready", "p": "blocked", "e": "blocked",
          "finished": false, "active": false, "status": "Pronta para discussão" }
      ],
      "log": ["12-17-SUMMARY.md escrito", "turno concluído em 4m12s"]
    })
}

fn kinds(nodes: &[view::Node]) -> Vec<std::mem::Discriminant<NodeKind>> {
    nodes
        .iter()
        .map(|node| std::mem::discriminant(&node.kind))
        .collect()
}

fn spans(node: &view::Node) -> (String, Role) {
    match &node.kind {
        NodeKind::Text { spans, .. } => (spans[0].text.clone(), spans[0].role),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn the_board_is_drawn_from_its_fixture() {
    let extension = hosted_view(board(), fixture(), "pt");
    let root = tree(&extension);
    assert!(matches!(root.kind, NodeKind::Stack { gap: 0 }));
    assert_eq!(root.children.len(), 3, "the log waits behind its tab");

    let header = &root.children[0];
    assert_eq!(
        kinds(&header.children),
        kinds(&[
            view::Node::new(0, NodeKind::Activity { busy: false }),
            view::Node::new(
                0,
                NodeKind::Text {
                    spans: Vec::new(),
                    truncate: false
                }
            ),
            view::Node::new(0, NodeKind::Tabs { tabs: Vec::new() }),
            view::Node::new(
                0,
                NodeKind::Button {
                    label: String::new()
                }
            ),
            view::Node::new(
                0,
                NodeKind::Button {
                    label: String::new()
                }
            ),
        ])
    );
    assert_eq!(
        spans(&header.children[1]),
        ("GSD · v1.23 Assistente do lojista".into(), Role::Muted)
    );
    assert!(header.children[1].grow);
    let tabs: Vec<(String, bool)> = match &header.children[2].kind {
        NodeKind::Tabs { tabs } => tabs
            .iter()
            .map(|tab| (tab.label.clone(), tab.active))
            .collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(tabs, [("Fases".into(), true), ("Log".into(), false)]);
    assert_eq!(
        header.children[4].kind,
        NodeKind::Button {
            label: "▶ executar fase 12".into()
        }
    );

    let measure = &root.children[1];
    let segment = |weight, filled, role| view::Segment {
        weight,
        filled,
        role,
    };
    assert_eq!(
        measure.children[0].kind,
        NodeKind::Progress {
            segments: vec![
                segment(6, 1000, Role::Success),
                segment(19, 890, Role::Warning),
                segment(4, 0, Role::Muted),
            ]
        }
    );
    assert_eq!(spans(&measure.children[1]).0, "65%");
    assert_eq!(spans(&measure.children[2]).0, "· 3 de 6 fases");

    let list = &root.children[2];
    assert_eq!(
        list.kind,
        NodeKind::List {
            selected: None,
            empty: Some("Nenhuma fase em andamento".into())
        }
    );
    assert_eq!(list.children.len(), 3);
    let finished = &list.children[0];
    assert_eq!(
        finished.children[2].kind,
        NodeKind::Marks(vec![Role::Success, Role::Success, Role::Success])
    );
    assert_eq!(spans(&finished.children[3]), ("6/6".into(), Role::Muted));
    let active = &list.children[1];
    assert_eq!(active.children[0].min_width, 3);
    assert_eq!(
        active.children[3].kind,
        NodeKind::Pips {
            done: 17,
            total: 19
        }
    );
    assert_eq!(
        spans(&active.children[4]),
        ("Execução parcial (17/19)".into(), Role::Warning)
    );
    let blocked = &list.children[2];
    assert_eq!(
        blocked.children[2].kind,
        NodeKind::Marks(vec![Role::Info, Role::Muted, Role::Muted])
    );
    assert_eq!(blocked.children.len(), 4, "neither pips nor a count");
}

#[test]
fn switching_to_the_log_changes_only_the_ui_state() {
    let mut extension = hosted_view(board(), fixture(), "pt");
    let tabs = first(&tree(&extension), |kind| {
        matches!(kind, NodeKind::Tabs { .. })
    });
    extension.hit(VIEW, act(tabs.id, NodeAct::Tab(1)), SPACE);
    assert_eq!(extension.ui(VIEW).expect("ui state")["tab"], "log");
    assert_eq!(extension.data(super::tests::SOURCE), Some(&Ok(fixture())));
    assert!(extension.recorded().is_empty());
    let root = tree(&extension);
    let log = root.children.last().expect("the log");
    assert_eq!(
        log.kind,
        NodeKind::Log {
            lines: vec![
                view::Span::new("12-17-SUMMARY.md escrito", Role::Default),
                view::Span::new("turno concluído em 4m12s", Role::Default),
            ]
        }
    );
    assert!(
        nodes(&root)
            .iter()
            .all(|node| !matches!(node.kind, NodeKind::List { .. }))
    );
    let trace = extension.trace(VIEW).expect("a trace");
    let list = &trace.items[0].children[2];
    assert_eq!((list.reached, list.drawn), (1, 0));
    assert_eq!(
        list.absent,
        [Absence::Show {
            id: "tab".into(),
            value: "phases".into(),
            actual: Some("log".into())
        }]
    );
    assert_eq!(trace.templates["phase-row"][0].reached, 0);
}

#[test]
fn the_next_step_is_recorded_with_its_phase() {
    let mut extension = hosted_view(board(), fixture(), "pt");
    let root = tree(&extension);
    let next = nodes(&root)
        .into_iter()
        .find(|node| matches!(&node.kind, NodeKind::Button { label } if label.contains("12")))
        .expect("the next button")
        .id;
    extension.hit(VIEW, act(next, NodeAct::Press), SPACE);
    assert_eq!(
        extension.recorded(),
        [RecordedAction {
            surface: VIEW.into(),
            action: "next".into(),
            item: json!({ "phase": "12", "skill": "gsd:execute-phase" }),
        }]
    );
}

#[test]
fn without_a_next_step_the_button_is_absent_and_the_trace_says_why() {
    let mut data = fixture();
    data.as_object_mut().expect("an object").remove("next");
    let extension = hosted_view(board(), data, "pt");
    let root = tree(&extension);
    assert_eq!(root.children[0].children.len(), 4);
    let trace = extension.trace(VIEW).expect("a trace");
    assert_eq!(
        trace.items[0].children[0].children[4].absent,
        [Absence::When {
            field: "next".into(),
            missing: true
        }]
    );
}

#[test]
fn the_band_draws_the_same_board_in_the_sidebar() {
    let extension = hosted_view(board(), fixture(), "pt");
    let section = extension.section(SECTION, false).expect("a section");
    let root = section.tree.expect("a tree");
    assert!(texts(&root).contains(&"Consulta de dados e diagnóstico".to_owned()));
}
