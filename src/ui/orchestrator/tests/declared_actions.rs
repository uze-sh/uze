//! A plugin's action through the real declared engine, drawn: the strip
//! naming whose a section is, and the host's question before an action
//! that asks first.

use super::support::*;
use super::{ExtensionAnswer, HostedExtensions, LoadedExtension, WorkspaceHit, WorkspaceModel};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension};
use uze_contract::binding::{binding_of, parse_text};
use uze_contract::compiled::{Node, Span as SourceSpan};
use uze_contract::{Binding, Item, NodeName};
use uze_extensions::ExtensionHit;
use uze_extensions::view::{NodeAct, ViewHit};

fn identity() -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "market".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    }
}

fn node(name: NodeName, args: &[&str], props: &[(&str, Binding)], children: Vec<Item>) -> Item {
    Item::Node(Node {
        name,
        args: args
            .iter()
            .map(|text| binding_of(parse_text(text).expect("a view's text parses")))
            .collect(),
        props: props
            .iter()
            .map(|(name, binding)| ((*name).to_owned(), binding.clone()))
            .collect(),
        each: None,
        condition: None,
        children,
        span: SourceSpan::default(),
    })
}

/// A view with one button archiving the phase in the data, asking first,
/// and a section showing the same source. The extension calls itself
/// "Code", as a look-alike would.
fn compiled() -> CompiledExtension {
    let view = uze_contract::CompiledView {
        file: "board.kdl".to_owned(),
        items: vec![node(
            NodeName::Stack,
            &[],
            &[],
            vec![
                node(NodeName::Text, &["Phase {phase.name}"], &[], Vec::new()),
                node(
                    NodeName::Button,
                    &["Archive"],
                    &[
                        ("action", Binding::Literal(serde_json::json!("archive"))),
                        ("item", binding_of(parse_text("{phase}").unwrap())),
                    ],
                    Vec::new(),
                ),
            ],
        )],
        templates: BTreeMap::new(),
        lookups: Vec::new(),
    };
    let surface = |id: &str, slot| uze_contract::SurfaceDecl {
        band: None,
        id: id.to_owned(),
        slot,
        view: "board.kdl".to_owned(),
        source: "board".to_owned(),
        title: Some("Code".to_owned()),
    };
    CompiledExtension {
        identity: identity(),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "gsd-band".to_owned(),
            name: "Code".to_owned(),
            description: String::new(),
            surfaces: vec![
                surface("board", uze_contract::Slot::View),
                surface("band", uze_contract::Slot::Section),
            ],
            wants: Vec::new(),
            sources: vec![uze_contract::SourceDecl::new(
                "board",
                uze_contract::SourceKind::Fixture("fixtures/board.json".to_owned()),
            )],
            actions: vec![uze_contract::ActionDecl {
                id: "archive".to_owned(),
                label: "Archive".to_owned(),
                on: "board".to_owned(),
                key: None,
                confirm: Some("Archive {item.name}?".to_owned()),
                does: uze_contract::Does::Script("archive.rhai".to_owned()),
            }],
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
        },
        views: BTreeMap::from([("board.kdl".to_owned(), view)]),
    }
}

fn hosting() -> WorkspaceModel {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = HostedExtensions {
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    let installed = InstalledExtension {
        identity: identity(),
        dir: PathBuf::from("/market/plugins/gsd/extensions/gsd-band"),
        linked: false,
    };
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed.clone()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![LoadedExtension {
            installed,
            stamp: 1,
            compiled: Ok(compiled()),
            data: BTreeMap::from([(
                "board".to_owned(),
                Ok(serde_json::json!({ "phase": { "name": "auth" } })),
            )]),
        }],
    }));
    model
}

fn shown(rows: &[String], wanted: &str) -> bool {
    rows.iter().any(|row| row.contains(wanted))
}

#[test]
fn a_plugins_section_is_named_by_its_identity_over_its_header() {
    let mut model = hosting();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let rows = frame_rows_at(&mut model, area);
    let sidebar: Vec<String> = rows
        .iter()
        .map(|row| row.chars().take_while(|cell| *cell != '│').collect())
        .collect();
    let strip = sidebar
        .iter()
        .position(|row| row.contains("gsd-band · gsd@market"))
        .unwrap_or_else(|| panic!("the strip: {sidebar:#?}"));
    assert!(
        sidebar[strip + 1].contains("Code"),
        "the extension's own name only under it: {sidebar:#?}"
    );
}

#[test]
fn an_action_that_asks_first_is_a_host_dialog_attributed_and_esc_runs_nothing() {
    let mut model = hosting();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let slot = model.declared_views().next().expect("the view").id;
    super::open_surface(&mut model, slot);
    full_frame_at(&mut model, area);
    let rows = frame_rows_at(&mut model, area);
    assert!(shown(&rows, "gsd-band · gsd@market"), "{rows:#?}");
    assert!(shown(&rows, "Phase auth"), "{rows:#?}");

    let button = model
        .hits
        .iter()
        .find_map(|(_, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit {
                slot: on,
                hit:
                    hit @ ViewHit::Node {
                        act: NodeAct::Press,
                        ..
                    },
            }) if *on == slot => Some(*hit),
            _ => None,
        })
        .expect("the button is drawn");
    let space = model.surface_frame.content_space;
    model.hit_surface(slot, button, space);
    full_frame_at(&mut model, area);
    let rows = frame_rows_at(&mut model, area);
    assert!(shown(&rows, "Archive auth?"), "{rows:#?}");
    assert!(
        rows.iter()
            .filter(|row| row.contains("gsd-band · gsd@market"))
            .count()
            >= 2,
        "the question names whose it is, beside the strip: {rows:#?}"
    );
    let answers: Vec<ViewHit> = model
        .hits
        .iter()
        .filter_map(|(_, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit {
                hit: hit @ ViewHit::Answer(_),
                ..
            }) => Some(*hit),
            _ => None,
        })
        .collect();
    assert!(answers.contains(&ViewHit::Answer(true)), "{answers:?}");
    assert!(answers.contains(&ViewHit::Answer(false)), "{answers:?}");

    model.tell_surface(uze_extensions::view::Command::Close, space);
    let rows = frame_rows_at(&mut model, area);
    assert!(!shown(&rows, "Archive auth?"), "{rows:#?}");
    assert_eq!(model.open_slot, Some(slot), "the view stays");
}
