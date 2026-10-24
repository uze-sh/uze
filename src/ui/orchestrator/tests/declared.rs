//! The surfaces plugins declare, hosted: opening and closing a view, a
//! gesture that runs nothing, answers that arrive too late, a reload that
//! keeps the viewer's place, and an error that keeps the last draw.

use super::support::*;
use super::{
    DeclaredExtensions, ExtensionAnswer, Hosted, Inspection, LoadedExtension, WorkspaceHit,
    WorkspaceModel,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uze_application::{
    CompiledExtension, ExtensionDiagnostic, ExtensionIdentity, InstalledExtension,
};
use uze_extensions::ExtensionHit;
use uze_extensions::contract::{Extension, Outcome, SurfaceInfo, SurfaceSlot};
use uze_extensions::view::{
    Command, Content, Layout, Node, NodeAct, NodeKind, Role, ScrollDirection, ScrollTarget,
    Section, Size, Span, Tab, View, ViewHit,
};

/// An engine that draws a row of two tabs and a line saying which one is
/// chosen and how many compiles it was handed: what the host keeps across
/// a reload is then on screen.
struct Board {
    tab: usize,
    compiles: usize,
}

impl Extension for Board {
    fn identity(&self) -> String {
        "board".to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        Vec::new()
    }

    fn view(&self, _surface: &str, _space: Size) -> Option<View> {
        let mut root = Node::new(0, NodeKind::Stack { gap: 0 });
        root.children = vec![
            Node::new(
                1,
                NodeKind::Tabs {
                    tabs: ["phases", "log"]
                        .iter()
                        .enumerate()
                        .map(|(index, label)| Tab {
                            label: (*label).to_owned(),
                            active: index == self.tab,
                        })
                        .collect(),
                },
            ),
            Node::new(
                2,
                NodeKind::Text {
                    spans: vec![Span::new(
                        format!("tab {} compile {}", self.tab, self.compiles),
                        Role::Default,
                    )],
                    truncate: true,
                },
            ),
        ];
        Some(View {
            title: vec![Span::new("Board", Role::Muted)],
            caption: Vec::new(),
            navigator: None,
            content: Content::Tree {
                heading: "Board".to_owned(),
                scroll: 0,
                root,
            },
            footer: vec![Command::Close],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: Layout::Sidebar,
            trail: Vec::new(),
        })
    }

    fn section(&self, _surface: &str, collapsed: bool) -> Option<Section> {
        Some(Section {
            title: "Band".to_owned(),
            collapsed,
            ..Section::default()
        })
    }

    fn command(&mut self, _surface: &str, command: Command, _space: Size) -> Outcome {
        match command {
            Command::Close => Outcome::Close,
            Command::NextView => {
                self.tab = (self.tab + 1) % 2;
                Outcome::Stay
            }
            _ => Outcome::Stay,
        }
    }

    fn hit(&mut self, _surface: &str, hit: ViewHit, _space: Size) -> Outcome {
        if let ViewHit::Node {
            node: 1,
            act: NodeAct::Tab(index),
        } = hit
        {
            self.tab = index;
        }
        Outcome::Stay
    }

    fn scroll(&mut self, _surface: &str, _target: ScrollTarget, _direction: ScrollDirection) {}
}

impl Hosted for Board {
    fn replace(&mut self, _compiled: CompiledExtension) {
        self.compiles += 1;
    }

    fn set_data(&mut self, _source: &str, _data: uze_application::SourceData) {}

    fn inspect(&self, _surface: &str) -> Inspection {
        Inspection::default()
    }
}

fn board(_compiled: CompiledExtension) -> Box<dyn Hosted> {
    Box::new(Board {
        tab: 0,
        compiles: 1,
    })
}

fn identity() -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "market".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    }
}

fn installed() -> InstalledExtension {
    InstalledExtension {
        identity: identity(),
        dir: PathBuf::from("/market/plugins/gsd/extensions/gsd-band"),
        linked: true,
    }
}

fn compiled() -> CompiledExtension {
    let surface = |id: &str, slot| uze_contract::SurfaceDecl {
        id: id.to_owned(),
        slot,
        view: format!("{id}.kdl"),
        source: "board".to_owned(),
        title: Some("Board".to_owned()),
    };
    CompiledExtension {
        identity: identity(),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "gsd-band".to_owned(),
            name: "GSD".to_owned(),
            description: String::new(),
            surfaces: vec![
                surface("board", uze_contract::Slot::View),
                surface("band", uze_contract::Slot::Section),
            ],
            sources: vec![uze_contract::SourceDecl {
                id: "board".to_owned(),
                fixture: Some("fixtures/board.json".to_owned()),
            }],
        },
        views: BTreeMap::new(),
    }
}

fn loaded(
    compiled: std::result::Result<CompiledExtension, Vec<ExtensionDiagnostic>>,
) -> LoadedExtension {
    LoadedExtension {
        installed: installed(),
        stamp: 1,
        compiled,
        data: BTreeMap::new(),
    }
}

fn discovered() -> ExtensionAnswer {
    ExtensionAnswer {
        discovered: Some(vec![installed()]),
        recompiled: Vec::new(),
        loaded: vec![loaded(Ok(compiled()))],
    }
}

fn recompiled(
    compiled: std::result::Result<CompiledExtension, Vec<ExtensionDiagnostic>>,
) -> ExtensionAnswer {
    ExtensionAnswer {
        discovered: None,
        recompiled: vec![identity()],
        loaded: vec![loaded(compiled)],
    }
}

/// An agent in front and the board extension hosted, as it is once its
/// discovery has landed.
fn hosting_the_board() -> WorkspaceModel {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = DeclaredExtensions {
        engine: board,
        discovered: true,
        ..DeclaredExtensions::default()
    };
    assert!(model.absorb_extension_answer(discovered()));
    model
}

/// Where the last frame drew `wanted`, anywhere on screen.
fn drawn(model: &WorkspaceModel, wanted: WorkspaceHit) -> ratatui::layout::Rect {
    model
        .hits
        .iter()
        .find(|(_, hit)| *hit == wanted)
        .map(|(rect, _)| *rect)
        .unwrap_or_else(|| panic!("{wanted:?} was not drawn"))
}

fn view_slot(model: &WorkspaceModel) -> u32 {
    model
        .declared_views()
        .next()
        .expect("the board's view is offered")
        .id
}

fn said(model: &WorkspaceModel) -> String {
    match model
        .declared_view(Size {
            width: 80,
            height: 20,
        })
        .map(|view| view.content)
    {
        Some(Content::Tree { root, .. }) => match &root.children[1].kind {
            NodeKind::Text { spans, .. } => spans[0].text.clone(),
            other => panic!("not the line: {other:?}"),
        },
        other => panic!("not the board: {other:?}"),
    }
}

#[test]
fn a_declared_view_opens_from_its_button_and_closes_from_it() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-open"));
    let model = hosting_the_board();
    let slot = view_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::OpenDeclared(slot));
    driven.press(button.x, button.y);
    assert_eq!(driven.attach.model.declared_open, Some(slot), "opened");

    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(
        rows.iter().any(|row| row.contains("gsd-band")),
        "attributed: {rows:#?}"
    );
    driven.attach.model.last_click = None;
    let button = drawn(&driven.attach.model, WorkspaceHit::OpenDeclared(slot));
    driven.press(button.x, button.y);
    assert_eq!(
        driven.attach.model.declared_open, None,
        "closed by the same button"
    );
}

#[test]
fn switching_a_tab_runs_nothing() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-tab"));
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let tab = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit::Declared {
            slot,
            hit: ViewHit::Node {
                node: 1,
                act: NodeAct::Tab(1),
            },
        }),
    );
    driven.press(tab.x, tab.y);
    assert_eq!(said(&driven.attach.model), "tab 1 compile 1");
    let declared = &driven.attach.model.remembered.declared;
    assert!(
        !declared.discovery_pending && declared.recompiling.is_empty() && !declared.stamps_pending,
        "no read was asked for"
    );
    assert!(
        driven
            .attach
            .channels
            .extension_jobs
            .receiver
            .try_recv()
            .is_err(),
        "and none answered"
    );
}

#[test]
fn an_answer_about_an_extension_no_longer_installed_is_dropped() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);

    // The plugin was removed: its view goes, whatever stood in the pane.
    model.absorb_extension_answer(ExtensionAnswer {
        discovered: Some(Vec::new()),
        recompiled: Vec::new(),
        loaded: Vec::new(),
    });
    assert_eq!(model.declared_open, None, "put away");
    assert!(model.declared_views().next().is_none());

    // A recompile that was out meanwhile does not bring it back.
    model.absorb_extension_answer(recompiled(Ok(compiled())));
    assert!(model.declared_views().next().is_none(), "stays away");
    assert!(model.remembered.declared.recompiling.is_empty(), "released");
}

#[test]
fn a_reload_keeps_the_place() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
    model.tell_declared(
        Command::NextView,
        Size {
            width: 80,
            height: 20,
        },
    );
    assert_eq!(said(&model), "tab 1 compile 1");

    // A save settles and compiles: the engine is handed the newer compile
    // in place, so the tab chosen and the slot open stay as they were.
    model
        .remembered
        .declared
        .absorb_stamps(vec![(identity(), 2)], std::time::Instant::now());
    let settled = model
        .remembered
        .declared
        .settled(std::time::Instant::now() + super::DECLARED_SETTLE);
    assert_eq!(settled.len(), 1, "the save settled");
    model.absorb_extension_answer(recompiled(Ok(compiled())));
    assert_eq!(model.declared_open, Some(slot));
    assert_eq!(said(&model), "tab 1 compile 2");
}

#[test]
fn an_error_keeps_the_last_draw_and_says_itself_over_it() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
    model.absorb_extension_answer(recompiled(Err(vec![ExtensionDiagnostic {
        file: "board.kdl".to_owned(),
        line: 3,
        column: 5,
        message: "unknown node `Stak`".to_owned(),
        suggestion: Some("Stack".to_owned()),
    }])));
    assert_eq!(said(&model), "tab 0 compile 1", "the last good draw stays");

    let rows = frame_rows_at(&mut model, ratatui::layout::Rect::new(0, 0, 140, 40));
    assert!(
        rows.iter().any(|row| row.contains("tab 0 compile 1")),
        "drawn: {rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("board.kdl:3:5: unknown node `Stak`")),
        "with the error over it: {rows:#?}"
    );
}

#[test]
fn an_extension_that_never_compiled_says_its_error_alone() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = DeclaredExtensions {
        engine: board,
        discovered: true,
        ..DeclaredExtensions::default()
    };
    model.absorb_extension_answer(ExtensionAnswer {
        discovered: Some(vec![installed()]),
        recompiled: Vec::new(),
        loaded: vec![loaded(Err(vec![ExtensionDiagnostic {
            file: "extension.kdl".to_owned(),
            line: 1,
            column: 1,
            message: "protocol 2 is newer than 1".to_owned(),
            suggestion: None,
        }]))],
    });
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
    let rows = frame_rows_at(&mut model, ratatui::layout::Rect::new(0, 0, 140, 40));
    assert!(
        rows.iter().any(|row| row.contains("can't be drawn")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("protocol 2 is newer than 1")),
        "{rows:#?}"
    );

    // Once it compiles, the stand-in gives way to its real surfaces.
    model.absorb_extension_answer(recompiled(Ok(compiled())));
    assert_eq!(model.declared_open, None, "the stand-in went");
    assert_eq!(model.declared_views().count(), 1);
}

#[test]
fn a_declared_section_folds_from_its_header() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-section"));
    let model = hosting_the_board();
    let section = model
        .remembered
        .declared
        .slots
        .iter()
        .find(|slot| slot.surface.slot == SurfaceSlot::Section)
        .expect("the band is hosted")
        .id;
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let header = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit::Declared {
            slot: section,
            hit: ViewHit::ToggleSection,
        }),
    );
    driven.press(header.x, header.y);
    let folded = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .remembered
            .declared
            .slot(section)
            .expect("still hosted")
            .collapsed
    };
    assert!(folded(&driven));
    driven.attach.model.last_click = None;
    driven.frame();
    let header = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit::Declared {
            slot: section,
            hit: ViewHit::ToggleSection,
        }),
    );
    driven.press(header.x, header.y);
    assert!(!folded(&driven));
}

#[test]
fn a_switched_off_extension_takes_its_button_and_its_view() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
    model.follow_extension_switch(std::collections::BTreeSet::from([super::switch_id(
        &identity(),
    )]));
    assert_eq!(model.declared_open, None);
    assert!(model.declared_views().next().is_none());
}

#[test]
fn the_inspector_stands_beside_the_view_and_esc_puts_it_away_first() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-inspector"));
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    let key = |name: &str| key_event(uze_keys::Chord::parse(name).unwrap());
    driven.press_key(key("i"));
    assert!(driven.attach.model.inspecting);
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(
        rows.iter().any(|row| row.contains("inspector")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("tab 0 compile 1")),
        "the view beside it: {rows:#?}"
    );

    driven.press_key(key("esc"));
    assert!(!driven.attach.model.inspecting, "the inspector goes first");
    assert_eq!(driven.attach.model.declared_open, Some(slot));
    driven.press_key(key("esc"));
    assert_eq!(driven.attach.model.declared_open, None, "then the view");
}
