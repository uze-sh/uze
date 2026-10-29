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
        file: "board.uze".to_owned(),
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
        bar: None,
        drawer: None,
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
                id: "archive".to_owned(),
                label: "Archive".to_owned(),
                on: "board".to_owned(),
                key: None,
                confirm: Some("Archive {item.name}?".to_owned()),
                does: uze_contract::Does::Call {
                    function: "archive".to_owned(),
                    file: Some("archive.uze".to_owned()),
                },
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
    hosting_compiled(compiled())
}

fn hosting_compiled(compiled: CompiledExtension) -> WorkspaceModel {
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
            compiled: Ok(compiled),
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

/// A plugin's section is drawn flat in the sidebar, as the sidebar's own
/// are: its header is its title, with no strip naming whose it is above it
/// (`extension-bar` D8).
#[test]
fn a_plugins_section_is_drawn_flat_under_its_own_title() {
    let mut model = hosting();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let rows = frame_rows_at(&mut model, area);
    let sidebar: Vec<String> = rows
        .iter()
        .map(|row| row.chars().take_while(|cell| *cell != '│').collect())
        .collect();
    assert!(
        sidebar.iter().any(|row| row.contains("Code")),
        "the section's own title: {sidebar:#?}"
    );
    assert!(
        !sidebar
            .iter()
            .any(|row| row.contains("gsd-band · gsd@market")),
        "no strip in the sidebar: {sidebar:#?}"
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
    assert!(shown(&rows, "Code"), "headed by its title: {rows:#?}");
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
            == 1,
        "the question names whose it is, and only the question: {rows:#?}"
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

#[test]
fn an_answered_actions_script_is_handed_to_the_host() {
    let mut model = hosting();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let slot = model.declared_views().next().expect("the view").id;
    super::open_surface(&mut model, slot);
    full_frame_at(&mut model, area);
    let home = uze_core::UzeHome::at(uze_testkit::temp::scratch("orchestrator-action-handed"));
    let (sender, _answers) = std::sync::mpsc::channel();
    model.schedule_engine_jobs_at(&home, &sender, std::time::Instant::now());
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
    model.hit_surface(slot, ViewHit::Answer(true), space);
    assert_eq!(
        model.schedule_engine_jobs_at(&home, &sender, std::time::Instant::now()),
        1,
        "the action's script is asked of the host"
    );
}

/// A section's button that asks to type into an agent: its answer is the
/// section's own, so it reaches the host's question (or its refusal),
/// never the view that happens to be open in the pane.
#[test]
fn a_sections_button_asks_of_the_agent_rather_than_of_the_open_view() {
    let mut compiled = compiled();
    compiled.manifest.wants = vec![uze_contract::Grant::AgentPrompt];
    compiled.manifest.actions = vec![uze_contract::ActionDecl {
        id: "archive".to_owned(),
        label: "Ask for a review".to_owned(),
        on: "band".to_owned(),
        key: None,
        confirm: None,
        does: uze_contract::Does::AgentPrompt {
            prompt: uze_contract::PromptText::Text("Review PLAN.md".to_owned()),
            target: uze_contract::AgentTarget::Own,
            auto: None,
        },
        with: Vec::new(),
        then: None,
    }];
    let mut model = hosting_compiled(compiled);
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    full_frame_at(&mut model, area);
    let (slot, button) = model
        .hits
        .iter()
        .find_map(|(_, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit:
                    hit @ ViewHit::Node {
                        act: NodeAct::Press,
                        ..
                    },
            }) => Some((*slot, *hit)),
            _ => None,
        })
        .expect("the section's button is drawn");
    assert_ne!(model.open_slot, Some(slot), "a section, not the open view");
    let space = model.surface_frame.content_space;
    model.hit_surface(slot, button, space);
    let said = format!("{:?}", model.toast_stack());
    assert!(said.contains("not sent"), "{said}");
    assert!(said.contains("Ask for a review"), "{said}");
}
