//! A plugin's `Cells` through the real declared engine, drawn: a click in
//! one of its regions runs the action the region names, with its item.

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

/// A view with a grid whose one region eats the food at `x` 3, asking
/// first, so the question shows what the action received.
fn compiled() -> CompiledExtension {
    let view = uze_contract::CompiledView {
        file: "board.uze".to_owned(),
        items: vec![node(
            NodeName::Cells,
            &[],
            &[
                ("rows", binding_of(parse_text("{grid}").unwrap())),
                ("regions", binding_of(parse_text("{food}").unwrap())),
            ],
            Vec::new(),
        )],
        templates: BTreeMap::new(),
        lookups: Vec::new(),
    };
    let surface = |id: &str, slot| uze_contract::SurfaceDecl {
        band: None,
        bar: None,
        id: id.to_owned(),
        slot,
        view: "board.uze".to_owned(),
        source: "board".to_owned(),
        title: Some("Code".to_owned()),
        applies_when: None,
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
                id: "eat".to_owned(),
                label: "Eat".to_owned(),
                on: "board".to_owned(),
                key: None,
                confirm: Some("Eat the food at {item.x}?".to_owned()),
                does: uze_contract::Does::Script("archive.uze".to_owned()),
                with: Vec::new(),
                then: None,
            }],
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
            icons: Vec::new(),
            mcp: Vec::new(),
        },
        views: BTreeMap::from([("board.uze".to_owned(), view)]),
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
                Ok(serde_json::json!({
                    "grid": ["#..o", "...."],
                    "food": [{ "x": 3, "y": 0, "w": 1, "h": 1, "action": "eat", "item": { "x": 3 } }],
                })),
            )]),
        }],
    }));
    model
}

fn shown(rows: &[String], wanted: &str) -> bool {
    rows.iter().any(|row| row.contains(wanted))
}

#[test]
fn a_click_in_a_region_of_cells_runs_its_action_with_its_item() {
    let mut model = hosting();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let slot = model.declared_views().next().expect("the view").id;
    super::open_surface(&mut model, slot);
    full_frame_at(&mut model, area);
    let rows = frame_rows_at(&mut model, area);
    assert!(shown(&rows, "#..o"), "{rows:#?}");

    let region = model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit {
                slot: on,
                hit:
                    hit @ ViewHit::Node {
                        act: NodeAct::Region(0),
                        ..
                    },
            }) if *on == slot => Some((*rect, *hit)),
            _ => None,
        })
        .expect("the region is a target");
    let (rect, hit) = region;
    assert_eq!((rect.width, rect.height), (1, 1), "the food's one cell");
    let food = rows[usize::from(rect.y)].chars().nth(usize::from(rect.x));
    assert_eq!(food, Some('o'), "over the cell it names");
    let space = model.surface_frame.content_space;
    model.hit_surface(slot, hit, space);
    full_frame_at(&mut model, area);
    let rows = frame_rows_at(&mut model, area);
    assert!(shown(&rows, "Eat the food at 3?"), "{rows:#?}");
}
