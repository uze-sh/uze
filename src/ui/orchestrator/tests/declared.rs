//! The surfaces plugins declare, hosted: opening and closing a view, a
//! gesture that runs nothing, answers that arrive too late, a reload that
//! keeps the viewer's place, and an error that keeps the last draw.

use super::support::*;
use super::{
    ExtensionAnswer, Hosted, HostedExtensions, Inspection, LoadedExtension, WorkspaceHit,
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
            Command::Activate => Outcome::Copy(format!("tab {}", self.tab)),
            Command::Back => Outcome::OpenPath {
                project: PathBuf::from("/project"),
                target: PathBuf::from("src/lib.rs"),
            },
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

    fn scroll(
        &mut self,
        _surface: &str,
        _target: ScrollTarget,
        _direction: ScrollDirection,
        _space: Size,
    ) {
    }

    fn place(&self, _surface: &str) -> Option<uze_extensions::contract::Opaque> {
        Some(uze_extensions::contract::Opaque::new(self.tab))
    }

    fn resume(&mut self, _surface: &str, place: uze_extensions::contract::Opaque) {
        if let Some(tab) = place.downcast::<usize>() {
            self.tab = tab;
        }
    }
}

impl Hosted for Board {
    fn replace(&mut self, _compiled: CompiledExtension) {
        self.compiles += 1;
    }

    fn set_data(&mut self, _source: &str, _data: uze_application::SourceData) {}

    fn forget(&mut self, _surface: &str) {
        self.tab = 0;
    }

    /// Taller than any drawer: two hundred nodes.
    fn inspect(&self, _surface: &str) -> Inspection {
        Inspection {
            nodes: (0..200).map(|node| format!("node {node}")).collect(),
            traced: true,
            ..Inspection::default()
        }
    }
}

fn board(_compiled: CompiledExtension, _locale: &str) -> Box<dyn Hosted> {
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
        band: None,
        bar: None,
        id: id.to_owned(),
        slot,
        view: format!("{id}.kdl"),
        source: "board".to_owned(),
        title: Some("Board".to_owned()),
        applies_when: None,
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
            wants: Vec::new(),
            sources: vec![uze_contract::SourceDecl::new(
                "board",
                uze_contract::SourceKind::Fixture("fixtures/board.json".to_owned()),
            )],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
            icons: Vec::new(),
            mcp: Vec::new(),
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
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![loaded(Ok(compiled()))],
    }
}

fn recompiled(
    compiled: std::result::Result<CompiledExtension, Vec<ExtensionDiagnostic>>,
) -> ExtensionAnswer {
    ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: false,
        discovered: None,
        recompiled: vec![identity()],
        records: Default::default(),
        loaded: vec![loaded(compiled)],
    }
}

/// An agent in front and the board extension hosted, as it is once its
/// discovery has landed.
fn hosting_the_board() -> WorkspaceModel {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = HostedExtensions {
        engine: board,
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    assert!(model.absorb_extension_answer(discovered()));
    enabled_where_its_agent_stands(&mut model);
    model
}

/// Every hosted extension running in the project in front: a door in the
/// bar is only offered for one that does.
fn enabled_where_its_agent_stands(model: &mut WorkspaceModel) {
    for hosted in model.remembered.declared.extensions.values_mut() {
        hosted.per_project = false;
    }
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
        .view_in_pane(Size {
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
    assert_eq!(driven.attach.model.open_slot, Some(slot), "opened");

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
        driven.attach.model.open_slot, None,
        "closed by the same button"
    );
}

/// A declared view is reached from the index as from its button: an
/// entry named after its title, with whose it is beside it.
#[test]
fn a_declared_view_opens_from_the_index() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-index"));
    let model = hosting_the_board();
    let slot = view_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let doors = driven.attach.model.declared_view_doors();
    let [door] = doors.as_slice() else {
        panic!("one view, one entry: {doors:?}");
    };
    assert!(door.label().starts_with("Open "), "{}", door.label());
    assert_eq!(door.description(), "gsd-band · gsd@market");

    driven.press_key(key_event(uze_keys::Chord::parse("f1").unwrap()));
    let index = driven.attach.model.action_index.as_ref().expect("open");
    assert!(index.offered.contains(door), "{:?}", index.offered);
    let rows = super::action_index_rows(
        &index.scopes,
        &index.offered,
        "",
        &driven.attach.model.disabled_extensions,
    );
    let at = rows
        .iter()
        .position(|(action, _)| action == door)
        .expect("listed");
    driven.attach.model.action_index.as_mut().unwrap().selected = at;
    driven.press_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(driven.attach.model.open_slot, Some(slot), "opened");
}

/// What a third party's surface raises outlives it, so its toast says
/// whose surface it was; a built-in's says nothing more.
#[test]
fn a_declared_extensions_toasts_are_attributed() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    model.follow_surface(Outcome::Failed {
        title: "archive failed".to_owned(),
        detail: "phase 12 is in use".to_owned(),
    });
    model.follow_surface(Outcome::Copy("phase 12".to_owned()));
    let said = format!("{:?}", model.toast_stack());
    assert!(
        said.contains("gsd-band · gsd@market: phase 12 is in use"),
        "{said}"
    );
    assert!(said.contains("gsd-band · gsd@market: phase 12"), "{said}");
    assert_eq!(
        model.clipboard.as_deref(),
        Some("phase 12"),
        "the text alone"
    );
}

#[test]
fn switching_a_tab_runs_nothing() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-tab"));
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let tab = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit {
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
fn a_copy_of_nothing_never_reaches_the_clipboard() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    model.copy_answered_by(" \n".to_owned(), Some(slot));
    assert_eq!(model.clipboard, None);
    assert!(model.toast_stack().is_empty(), "nothing was copied to say");
}

#[test]
fn a_view_that_asks_for_a_copy_or_a_path_gets_the_clipboard_or_the_code_surface() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    let space = Size {
        width: 80,
        height: 20,
    };
    model.tell_surface(Command::Activate, space);
    assert_eq!(model.clipboard.as_deref(), Some("tab 0"));

    model.tell_surface(Command::Back, space);
    assert_ne!(model.open_slot, Some(slot), "the view gave way");
    let code = code_view(&model).expect("the code surface opened");
    assert_eq!(code.root(), std::path::Path::new("/project"));
}

#[test]
fn an_answer_about_an_extension_no_longer_installed_is_dropped() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);

    // The plugin was removed: its view goes, whatever stood in the pane.
    model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(Vec::new()),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: Vec::new(),
    });
    assert_eq!(model.open_slot, None, "put away");
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
    super::open_surface(&mut model, slot);
    model.tell_surface(
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
    assert_eq!(model.open_slot, Some(slot));
    assert_eq!(said(&model), "tab 1 compile 2");
}

#[test]
fn an_error_keeps_the_last_draw_and_says_itself_over_it() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    model.absorb_extension_answer(recompiled(Err(vec![ExtensionDiagnostic {
        file: "board.uze".to_owned(),
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
            .any(|row| row.contains("board.uze:3:5: unknown node `Stak`")),
        "with the error over it: {rows:#?}"
    );
}

#[test]
fn an_extension_that_never_compiled_says_its_error_alone() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = HostedExtensions {
        engine: board,
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![loaded(Err(vec![ExtensionDiagnostic {
            file: "extension.kdl".to_owned(),
            line: 1,
            column: 1,
            message: "protocol 2 is newer than 1".to_owned(),
            suggestion: None,
        }]))],
    });
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
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
    assert_eq!(model.open_slot, None, "the stand-in went");
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
        .find(|slot| slot.surface.slot == SurfaceSlot::Section && slot.owner.declared().is_some())
        .expect("the band is hosted")
        .id;
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let header = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit {
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
        WorkspaceHit::Extension(ExtensionHit {
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
    super::open_surface(&mut model, slot);
    model.follow_extension_switch(std::collections::BTreeSet::from([super::switch_id(
        &identity(),
    )]));
    assert_eq!(model.open_slot, None);
    assert!(model.declared_views().next().is_none());
}

#[test]
fn the_inspector_stands_beside_the_view_and_esc_puts_it_away_first() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-inspector"));
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
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
    assert_eq!(driven.attach.model.open_slot, Some(slot));
    driven.press_key(key("esc"));
    assert_eq!(driven.attach.model.open_slot, None, "then the view");
}

/// The `gsd` fixture plugin installed from a marketplace linked to its
/// working tree under `root`, answering with the view file an author edits.
fn the_gsd_band_installed(root: &std::path::Path, home: &uze_application::UzeHome) -> PathBuf {
    let market = root.join("market");
    let manifest = serde_json::json!({
        "name": "studio",
        "plugins": [{ "name": "gsd", "source": "./plugins/gsd", "description": "GSD band." }],
    });
    uze_testkit::marketplace::stage(
        &market,
        &manifest.to_string(),
        &[("gsd".to_owned(), uze_testkit::fixtures::canonical("gsd"))],
    );
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    app.marketplace()
        .add(&market.display().to_string())
        .unwrap();
    app.marketplace().link("studio", &market).unwrap();
    app.marketplace()
        .install_plugin("gsd@studio", &uze_application::AlwaysTrust)
        .unwrap();
    market.join("plugins/gsd/extensions/gsd-band/board.uze")
}

/// What the real job answers, waited for: the read runs on its own
/// thread, as the attach loop runs it, and is absorbed once it answers.
fn answered(model: &mut WorkspaceModel, home: &uze_application::UzeHome, job: super::ExtensionJob) {
    let (sender, receiver) = std::sync::mpsc::channel();
    super::spawn_extension_job(home, job, sender);
    let answer = receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the extension job answers");
    assert!(model.absorb_extension_answer(answer));
}

fn shown(rows: &[String], wanted: &str) -> bool {
    rows.iter().any(|row| row.contains(wanted))
}

#[test]
fn the_real_gsd_band_opens_switches_its_tab_and_redraws_a_save() {
    let root = uze_testkit::temp::scratch("declared-gsd-band");
    let home = uze_application::UzeHome::at(root.join("uze"));
    let view_file = the_gsd_band_installed(&root, &home);

    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Discover(Default::default()),
    );
    enabled_where_its_agent_stands(&mut model);
    let slot = view_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::OpenDeclared(slot));
    driven.press(button.x, button.y);
    assert_eq!(driven.attach.model.open_slot, Some(slot));

    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    for wanted in [
        "gsd-band · gsd@studio",
        "v1.23  Assistente do lojista",
        "Consulta de dados e diagnóstico",
        "Relatórios do lojista",
        "17/19",
        "65%  3 of 6 done",
        "run phase 12",
        "12-18  Diagnóstico de estoque baixo por loja",
    ] {
        assert!(shown(&rows, wanted), "{wanted}: {rows:#?}");
    }
    // The band's section in the sidebar, drawn from its own compact view:
    // the milestone, its bar and the phase in progress fit the column
    // without folding the band away.
    let sidebar: Vec<String> = rows
        .iter()
        .map(|row| row.chars().take_while(|cell| *cell != '│').collect())
        .collect();
    assert!(shown(&sidebar, "▾ GSD"), "{sidebar:#?}");
    assert!(shown(&sidebar, "v1.23 65%"), "{sidebar:#?}");
    assert!(shown(&sidebar, "17/19"), "{sidebar:#?}");

    let log_tab = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit {
                slot: on,
                hit:
                    ViewHit::Node {
                        act: NodeAct::Tab(1),
                        ..
                    },
            }) if *on == slot => Some(*rect),
            _ => None,
        })
        .expect("the Activity tab is drawn");
    driven.press(log_tab.x, log_tab.y);
    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(shown(&rows, "Turn finished in 4m12s"), "{rows:#?}");
    assert!(!shown(&rows, "Relatórios do lojista"), "{rows:#?}");

    // The author saves the view in the working tree; the recompile the
    // poll would ask for redraws it, on the tab the viewer chose.
    let source = std::fs::read_to_string(&view_file).unwrap();
    std::fs::write(
        &view_file,
        source.replace(r#"Text "{percent}%""#, r#"Text "{percent}% done""#),
    )
    .unwrap();
    let installed = driven
        .attach
        .model
        .remembered
        .declared
        .extensions
        .values()
        .map(|hosted| hosted.installed.clone())
        .collect();
    answered(
        &mut driven.attach.model,
        &home,
        super::ExtensionJob::Recompile(installed),
    );
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(shown(&rows, "65% done"), "{rows:#?}");
    assert!(shown(&rows, "Turn finished in 4m12s"), "{rows:#?}");
    assert_eq!(driven.attach.model.open_slot, Some(slot), "kept open");
    std::fs::remove_dir_all(root).unwrap();
}

/// A second plugin's extension, installed while the workspace is open.
fn notes() -> InstalledExtension {
    InstalledExtension {
        identity: ExtensionIdentity {
            marketplace: "market".to_owned(),
            plugin: "notes".to_owned(),
            id: "notes".to_owned(),
        },
        dir: PathBuf::from("/market/plugins/notes/extensions/notes"),
        linked: false,
    }
}

fn notes_compiled() -> CompiledExtension {
    let mut compiled = compiled();
    compiled.identity = notes().identity;
    compiled.manifest.id = "notes".to_owned();
    compiled.manifest.surfaces.truncate(1);
    compiled
}

#[test]
fn a_discovery_is_due_when_the_plugins_move_or_the_clock_says_so() {
    let now = std::time::Instant::now();
    let mut declared = HostedExtensions::default();
    assert!(declared.discovery_due(0, now), "never asked this run");

    declared.discovered_at = Some(now);
    assert!(!declared.discovery_due(0, now), "just asked");
    assert!(
        declared.discovery_due(1, now),
        "an install landed in this process"
    );
    assert!(
        declared.discovery_due(0, now + super::DECLARED_REDISCOVER),
        "one made elsewhere is seen within the interval"
    );

    declared.discovery_pending = true;
    assert!(!declared.discovery_due(1, now), "one at a time");
}

#[test]
fn a_rediscovery_brings_in_a_new_extension_and_takes_a_removed_one_away() {
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    model.tell_surface(
        Command::NextView,
        Size {
            width: 80,
            height: 20,
        },
    );
    model.close_surface();
    assert_eq!(model.remembered.declared.places.len(), 1, "a place kept");

    // Installed: only the newcomer is loaded, the board is as it was.
    let changed = model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed(), notes()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![LoadedExtension {
            installed: notes(),
            stamp: 1,
            compiled: Ok(notes_compiled()),
            data: BTreeMap::new(),
        }],
    });
    assert!(changed);
    let views: Vec<String> = model
        .declared_views()
        .filter_map(|slot| slot.owner.declared().map(|identity| identity.id.clone()))
        .collect();
    assert_eq!(views, ["gsd-band", "notes"]);
    assert_eq!(model.declared_views().next().unwrap().id, slot, "kept");

    // Removed: its slots close and its place goes with them.
    super::open_surface(&mut model, slot);
    let changed = model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![notes()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: Vec::new(),
    });
    assert!(changed);
    assert_eq!(model.open_slot, None, "its open view closed");
    let declared = &model.remembered.declared;
    assert!(
        declared
            .slots
            .iter()
            .filter_map(|slot| slot.owner.declared())
            .all(|identity| *identity == notes().identity)
    );
    assert!(declared.places.is_empty(), "its place dropped");

    // Nothing moved: nothing to redraw.
    assert!(!model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![notes()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: Vec::new(),
    }));
}

#[test]
fn a_discovery_reloads_only_what_differs_from_what_was_compiled() {
    let model = hosting_the_board();
    let known = model.remembered.declared.known();
    assert_eq!(known.get(&identity()), Some(&(installed().dir, 1)));
}

#[test]
fn an_install_in_this_process_rediscovers_on_the_next_schedule() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-rediscover"));
    let mut model = hosting_the_board();
    let (jobs, answers) = std::sync::mpsc::channel();
    let (stamps, _polled) = std::sync::mpsc::channel();
    model.remembered.declared.discovery_pending = false;

    model.schedule_extension_jobs(&home, 0, &jobs, &stamps);
    assert!(
        !model.remembered.declared.discovery_pending,
        "nothing moved, nothing asked"
    );

    model.schedule_extension_jobs(&home, 1, &jobs, &stamps);
    assert!(model.remembered.declared.discovery_pending, "asked again");
    let answer = answers
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the discovery answered");
    // This home has no plugin installed: the board was removed.
    assert!(model.absorb_extension_answer(answer));
    assert!(
        model
            .remembered
            .declared
            .slots
            .iter()
            .all(|slot| slot.owner.declared().is_none()),
        "only the built-ins stay"
    );
    assert!(!model.remembered.declared.discovery_pending);
}

#[test]
fn every_engine_is_built_for_the_operators_language() {
    fn portuguese_only(compiled: CompiledExtension, locale: &str) -> Box<dyn Hosted> {
        assert_eq!(locale, "pt", "the engine was handed another language");
        board(compiled, locale)
    }
    assert_eq!(
        HostedExtensions::default().locale,
        uze_platform::locale::language(),
        "the platform's answer, read once"
    );
    let mut declared = HostedExtensions {
        engine: portuguese_only,
        locale: "pt".to_owned(),
        ..HostedExtensions::default()
    };
    declared.absorb_loaded(loaded(Ok(compiled())));
    let declared_slots = declared
        .slots
        .iter()
        .filter(|slot| slot.owner.declared().is_some());
    assert_eq!(declared_slots.count(), 2);
}

fn inspector_open() -> Driven<'static> {
    let home: &'static uze_application::UzeHome = Box::leak(Box::new(
        uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-inspector-drawer")),
    ));
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    let mut driven = driven(model, home).on_a_roomy_terminal();
    driven.frame();
    let hint = drawn(
        &driven.attach.model,
        WorkspaceHit::HintAction(uze_keys::Action::ToggleInspector),
    );
    driven.press(hint.x, hint.y);
    driven.frame();
    driven
}

/// The inspector's key is named in the view's footer, and clicking it is
/// pressing it: open, and closed again.
#[test]
fn clicking_the_inspector_hint_is_pressing_its_key() {
    let mut driven = inspector_open();
    assert!(driven.attach.model.inspecting, "opened by the pointer");
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(rows.iter().any(|row| row.contains("node 0")), "{rows:#?}");

    driven.attach.model.last_click = None;
    let hint = drawn(
        &driven.attach.model,
        WorkspaceHit::HintAction(uze_keys::Action::ToggleInspector),
    );
    driven.press(hint.x, hint.y);
    assert!(!driven.attach.model.inspecting, "closed by the same hint");
    assert!(driven.attach.model.open_slot.is_some(), "the view stays");
}

#[test]
fn the_inspector_scrolls_by_wheel_and_by_page_and_stops_at_its_end() {
    let mut driven = inspector_open();
    let drawer = drawn(&driven.attach.model, WorkspaceHit::Inspector);
    let model = &driven.attach.model;
    assert_eq!(model.inspector_scroll.get(), 0);

    driven.mouse(
        drawer.x + 2,
        drawer.y + 2,
        crossterm::event::MouseEventKind::ScrollDown,
    );
    driven.frame();
    let wheeled = driven.attach.model.inspector_scroll.get();
    assert!(wheeled > 0, "the wheel scrolled it");

    let key = |name: &str| key_event(uze_keys::Chord::parse(name).unwrap());
    driven.press_key(key("pagedown"));
    driven.frame();
    let paged = driven.attach.model.inspector_scroll.get();
    assert_eq!(
        paged,
        wheeled + drawer.height - 1,
        "a page, keeping a line of the last"
    );
    assert_eq!(
        said(&driven.attach.model),
        "tab 0 compile 1",
        "the view stayed"
    );

    for _ in 0..20 {
        driven.press_key(key("pagedown"));
    }
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(
        rows.iter().any(|row| row.contains("nothing pressed")),
        "its last group is on screen: {rows:#?}"
    );
    let settled = driven.attach.model.inspector_scroll.get();
    driven.press_key(key("pagedown"));
    driven.frame();
    assert_eq!(
        driven.attach.model.inspector_scroll.get(),
        settled,
        "stopped"
    );

    driven.press_key(key("pageup"));
    driven.frame();
    assert!(driven.attach.model.inspector_scroll.get() < settled);
}

#[test]
fn the_inspector_is_resized_by_its_rule_and_fits_the_frame() {
    let mut driven = inspector_open();
    let rule = drawn(&driven.attach.model, WorkspaceHit::ResizeOverviewDrawer);
    let drawer = drawn(&driven.attach.model, WorkspaceHit::Inspector);
    driven.press(rule.x, rule.y + 2);
    driven.mouse(
        rule.x + 10,
        rule.y + 2,
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
    );
    driven.mouse(
        rule.x + 10,
        rule.y + 2,
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
    );
    driven.frame();
    let moved = drawn(&driven.attach.model, WorkspaceHit::ResizeOverviewDrawer);
    assert_eq!(moved.x, rule.x + 10, "dragged narrower");
    let narrower = drawn(&driven.attach.model, WorkspaceHit::Inspector);
    assert_eq!(narrower.width + 10, drawer.width);
    assert!(driven.attach.model.inspecting, "a drag is not a dismissal");

    // On a narrow frame it never takes more than half the pane.
    driven.area = ratatui::layout::Rect::new(0, 0, 80, 30);
    driven.frame();
    let rule = drawn(&driven.attach.model, WorkspaceHit::ResizeOverviewDrawer);
    let layout = super::compute_layout(
        driven.area,
        driven.attach.model.sidebar_width,
        driven.attach.model.reserved(),
    );
    assert!(layout.pane.right() - rule.x <= layout.pane.width / 2 + 1);
}

/// The board with a drawer besides its view and section, hosted with an
/// agent in front.
fn hosting_the_drawer() -> WorkspaceModel {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = HostedExtensions {
        engine: board,
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    let mut with_drawer = compiled();
    with_drawer
        .manifest
        .surfaces
        .push(uze_contract::SurfaceDecl {
            band: None,
            bar: None,
            id: "side".to_owned(),
            slot: uze_contract::Slot::Drawer,
            view: "side.uze".to_owned(),
            source: "board".to_owned(),
            title: Some("Side".to_owned()),
            applies_when: None,
        });
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![loaded(Ok(with_drawer))],
    }));
    model.follow_drawer_space();
    model
}

fn drawer_slot(model: &WorkspaceModel) -> u32 {
    model
        .declared_drawers()
        .next()
        .expect("the board's drawer is offered")
        .id
}

/// What the drawer's line says: which tab its engine holds.
fn drawer_said(model: &WorkspaceModel) -> String {
    let slot = model
        .remembered
        .declared
        .slot(drawer_slot(model))
        .expect("the drawer's slot");
    match model.surface_view(slot, Size::default()).content {
        Content::Tree { root, .. } => match &root.children[1].kind {
            NodeKind::Text { spans, .. } => spans[0].text.clone(),
            other => panic!("not the line: {other:?}"),
        },
        other => panic!("not the board: {other:?}"),
    }
}

fn layout_of(driven: &Driven<'_>) -> super::WorkspaceLayout {
    let model = &driven.attach.model;
    super::compute_layout(driven.area, model.sidebar_width, model.reserved())
}

#[test]
fn a_drawer_opens_from_its_button_beside_the_pane_and_closes_from_it() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("drawer-button"));
    let model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    let alone = layout_of(&driven);
    assert!(alone.drawer.is_none());
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::ToggleDrawer(slot));
    driven.press(button.x, button.y);
    assert_eq!(driven.attach.model.open_drawer(), Some(slot), "opened");
    assert_eq!(driven.attach.model.open_slot, None, "not a view");

    let docked = layout_of(&driven);
    let drawer = docked.drawer.expect("docked beside the pane");
    assert_eq!(docked.pane.x, alone.pane.x);
    assert_eq!(
        docked.pane.width + drawer.width,
        alone.pane.width,
        "the pane gives up exactly the drawer's column"
    );
    assert_eq!(docked.pane.right(), drawer.x, "beside it, never over it");
    assert_eq!(drawer.right(), alone.pane.right());

    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    let drawer_text: Vec<String> = rows
        .iter()
        .map(|row| row.chars().skip(usize::from(drawer.x)).collect())
        .collect();
    assert!(
        drawer_text
            .iter()
            .any(|row| row.contains("gsd-band · gsd@market")),
        "attributed: {rows:#?}"
    );
    assert!(
        drawer_text
            .iter()
            .any(|row| row.contains("tab 0 compile 1")),
        "its view under the strip: {rows:#?}"
    );

    driven.attach.model.last_click = None;
    let button = drawn(&driven.attach.model, WorkspaceHit::ToggleDrawer(slot));
    driven.press(button.x, button.y);
    assert_eq!(driven.attach.model.open_drawer(), None, "closed by it");
    let back = layout_of(&driven);
    assert!(back.drawer.is_none());
    assert_eq!(back.pane, alone.pane, "the pane has its width back");
}

#[test]
fn the_drawer_key_toggles_it_while_typing_still_reaches_the_pane() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("drawer-key"));
    let model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    let chord = uze_keys::active()
        .chord_for(uze_keys::Action::ToggleDrawer, &[uze_keys::Scope::Surfaces])
        .expect("the drawer's door has a key");
    driven.press_key(key_event(chord));
    assert_eq!(driven.attach.model.open_drawer(), Some(slot));
    let _ = driven.sent();

    driven.press_key(key_event(uze_keys::Chord::parse("x").unwrap()));
    assert!(
        driven.sent().iter().any(|request| matches!(
            request,
            crate::ui::orchestrator::ClientRequest::Input { .. }
        )),
        "the letter reached the agent"
    );
    driven.press_key(key_event(uze_keys::Chord::parse("tab").unwrap()));
    assert_eq!(
        drawer_said(&driven.attach.model),
        "tab 0 compile 1",
        "the drawer took no key it was not lent"
    );
    assert_eq!(driven.attach.model.open_drawer(), Some(slot), "still open");

    driven.press_key(key_event(chord));
    assert_eq!(driven.attach.model.open_drawer(), None, "closed by its key");
}

#[test]
fn a_click_in_the_drawer_reaches_its_engine_and_esc_gives_the_keys_back_then_closes() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("drawer-click"));
    let mut model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    model.toggle_drawer(slot, 200);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let tab = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit {
            slot,
            hit: ViewHit::Node {
                node: 1,
                act: NodeAct::Tab(1),
            },
        }),
    );
    driven.press(tab.x, tab.y);
    assert_eq!(drawer_said(&driven.attach.model), "tab 1 compile 1");
    assert!(driven.attach.model.drawer_focused(), "lent the keys");

    let key = |name: &str| key_event(uze_keys::Chord::parse(name).unwrap());
    driven.press_key(key("tab"));
    assert_eq!(
        drawer_said(&driven.attach.model),
        "tab 0 compile 1",
        "its key, as its tab"
    );

    driven.press_key(key("esc"));
    assert!(!driven.attach.model.drawer_focused(), "esc gives them back");
    assert_eq!(driven.attach.model.open_drawer(), Some(slot), "and stays");
    driven.press_key(key("esc"));
    assert_eq!(
        driven.attach.model.open_drawer(),
        None,
        "a second esc closes it"
    );

    // Given back by a click elsewhere, the next esc is the pane's.
    driven.attach.model.toggle_drawer(slot, 200);
    driven.frame();
    let body = drawn(&driven.attach.model, WorkspaceHit::DockBody);
    driven.press(body.x + 3, body.bottom() - 2);
    assert!(driven.attach.model.drawer_focused());
    let pane = layout_of(&driven).pane;
    driven.press(pane.x + 2, pane.y + 2);
    assert!(!driven.attach.model.drawer_focused(), "a click in the pane");
    driven.press_key(key("esc"));
    assert_eq!(
        driven.attach.model.open_drawer(),
        Some(slot),
        "esc was the pane's"
    );
}

#[test]
fn the_wheel_over_the_drawer_scrolls_it() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("drawer-wheel"));
    let mut model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    model.toggle_drawer(slot, 200);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let body = drawn(&driven.attach.model, WorkspaceHit::DockBody);
    driven.attach.model.dirty = false;
    driven.mouse(
        body.x + 4,
        body.y + 4,
        crossterm::event::MouseEventKind::ScrollDown,
    );
    assert!(driven.attach.model.dirty, "the drawer was scrolled");
    assert_eq!(driven.attach.model.open_drawer(), Some(slot));
}

#[test]
fn the_drawer_refuses_to_open_when_the_pane_would_be_too_narrow() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("drawer-narrow"));
    let model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    let mut driven = driven(model, &home);
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::ToggleDrawer(slot));
    driven.press(button.x, button.y);
    assert_eq!(driven.attach.model.open_drawer(), None, "not opened");
    let toasts = driven.attach.model.toast_stack();
    assert_eq!(toasts.len(), 1, "and said why");
    assert!(layout_of(&driven).drawer.is_none());
}

#[test]
fn the_drawer_is_resized_by_its_edge_and_remembers_it() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("drawer-resize"));
    let mut model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    model.toggle_drawer(slot, 200);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    // Wide enough that six columns either way stay within the drawer's
    // bounds and the pane's floor.
    driven.area = ratatui::layout::Rect::new(0, 0, 200, 40);
    driven.frame();
    let before = layout_of(&driven);
    let edge = drawn(&driven.attach.model, WorkspaceHit::ResizeDock);
    driven.press(edge.x, edge.y + 3);
    assert!(driven.attach.model.dragging_dock, "held");
    driven.mouse(
        edge.x + 6,
        edge.y + 3,
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
    );
    driven.mouse(
        edge.x + 6,
        edge.y + 3,
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
    );
    assert!(!driven.attach.model.dragging_dock, "let go");
    assert_eq!(
        driven.attach.model.open_drawer(),
        Some(slot),
        "a drag closes nothing"
    );
    let after = layout_of(&driven);
    assert_eq!(
        after.pane.width,
        before.pane.width + 6,
        "the pane took six back"
    );
    assert_eq!(after.drawer.unwrap().x, before.drawer.unwrap().x + 6);

    driven.attach.model.close_drawer();
    driven.attach.model.toggle_drawer(slot, 200);
    assert_eq!(layout_of(&driven).pane, after.pane, "the width is kept");
}

#[test]
fn each_space_keeps_its_own_drawer_and_place() {
    let mut model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    let first = model.space_in_front().unwrap();
    model.toggle_drawer(slot, 200);
    model.tell_drawer(Command::NextView);
    assert_eq!(drawer_said(&model), "tab 1 compile 1");

    let session = model.session.as_mut().unwrap();
    session.create_space(Some("two".into()), PathBuf::from("/two"), 80, 24);
    model.follow_drawer_space();
    assert_eq!(model.open_drawer(), None, "the other space has none open");
    model.toggle_drawer(slot, 200);
    assert_eq!(drawer_said(&model), "tab 0 compile 1", "and starts over");
    let second = model.space_in_front().unwrap();

    model.session.as_mut().unwrap().workspace.selected_space = first;
    model.follow_drawer_space();
    assert_eq!(model.open_drawer(), Some(slot));
    assert_eq!(drawer_said(&model), "tab 1 compile 1", "where it was left");
    model.close_drawer();

    model.session.as_mut().unwrap().workspace.selected_space = second;
    model.follow_drawer_space();
    assert_eq!(
        model.open_drawer(),
        Some(slot),
        "closing one closed only it"
    );
}

#[test]
fn a_switched_off_extension_takes_its_drawer_away() {
    let mut model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    model.toggle_drawer(slot, 200);
    model.follow_extension_switch(std::collections::BTreeSet::from([super::switch_id(
        &identity(),
    )]));
    assert_eq!(model.open_drawer(), None);
    assert!(model.declared_drawers().next().is_none());
    model.follow_extension_switch(std::collections::BTreeSet::new());
    assert_eq!(
        model.open_drawer(),
        None,
        "switched back on, it stays closed"
    );
}

#[test]
fn the_drawer_is_drawn_raised_beside_a_live_pane() {
    let mut model = hosting_the_drawer();
    let slot = drawer_slot(&model);
    model.toggle_drawer(slot, 200);
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let layout = super::compute_layout(area, model.sidebar_width, model.reserved());
    let drawer = layout.drawer.unwrap();
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(area.width, area.height))
            .unwrap();
    terminal
        .draw(|frame| {
            super::render::render(
                frame,
                &model,
                &identities_fixture(),
                &mut Vec::new(),
                &mut super::render::FrameMetrics::default(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let card = drawer.x + crate::ui::widget::drawer::DOCKED_GAP;
    assert_eq!(buffer[(card, drawer.y)].symbol(), "┌", "a card's edge");
    assert_eq!(buffer[(drawer.right() - 1, drawer.y)].symbol(), "┐");
    assert_eq!(
        buffer[(drawer.x, drawer.y + 3)].symbol(),
        " ",
        "air before it"
    );
    let raised = crate::ui::theme::color(crate::ui::theme::Token::SurfaceRaised);
    assert_eq!(
        buffer[(card + 3, drawer.bottom() - 3)].bg,
        raised,
        "on a raised ground"
    );
    assert_ne!(
        buffer[(layout.pane.x + 2, layout.pane.y + 3)].bg,
        raised,
        "the pane keeps its own"
    );
}

/// A test's click and the workspace's land on the same node: the runner
/// finds the text in what it drew and asks the renderer's hit map, as the
/// workspace's pointer does.
#[test]
fn a_tests_click_and_the_workspaces_reach_the_same_tab() {
    let root = uze_testkit::temp::scratch("declared-test-click");
    let home = uze_application::UzeHome::at(root.join("uze"));
    the_gsd_band_installed(&root, &home);
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let band = app
        .extensions()
        .installed()
        .into_iter()
        .find(|extension| extension.identity.id == "gsd-band")
        .expect("the band is installed");
    let load = app.extensions().load(&band).expect("it compiles");
    let compiled = load.compiled;
    let data: std::collections::BTreeMap<String, serde_json::Value> = load
        .fixtures
        .into_iter()
        .filter_map(|(source, data)| Some((source, data.ok()?)))
        .collect();
    let from_a_test =
        crate::ui::extension_test::hit_for_text(&compiled, data, "board", (112, 30), "Activity")
            .expect("a test finds the tab");

    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Discover(Default::default()),
    );
    enabled_where_its_agent_stands(&mut model);
    let slot = view_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::OpenDeclared(slot));
    driven.press(button.x, button.y);
    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("activity") && row.contains("phases"))
        .expect("the tabs are drawn, lowercase");
    let x = row[..row.find("activity").unwrap()].chars().count() as u16;
    let from_the_workspace = driven
        .attach
        .model
        .hits
        .iter()
        .rev()
        .find(|(rect, _)| rect.contains(ratatui::layout::Position { x, y: y as u16 }))
        .map(|(_, hit)| *hit);
    assert_eq!(
        from_the_workspace,
        Some(WorkspaceHit::Extension(ExtensionHit {
            slot,
            hit: from_a_test
        }))
    );
    let _ = std::fs::remove_dir_all(root);
}

/// The drawer's scrollbar answers the pointer as every scrollbar does: a
/// press on its track jumps there, and the thumb follows a drag.
#[test]
fn the_drawers_scrollbar_follows_the_pointer() {
    let root = uze_testkit::temp::scratch("drawer-scrollbar");
    let home = uze_application::UzeHome::at(root.join("uze"));
    the_gsd_band_installed(&root, &home);
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Discover(Default::default()),
    );
    let slot = drawer_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.area = ratatui::layout::Rect::new(0, 0, 120, 24);
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::ToggleDrawer(slot));
    driven.press(button.x, button.y);
    driven.frame();
    let bar = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit {
            slot,
            hit: ViewHit::DragContentScrollbar,
        }),
    );
    assert_eq!(
        driven.attach.model.dock_bar.get().map(|(_, first)| first),
        Some(0)
    );
    driven.press(bar.x, bar.y + bar.height - 1);
    driven.frame();
    let after_press = driven
        .attach
        .model
        .dock_bar
        .get()
        .map(|(_, first)| first)
        .unwrap();
    assert!(after_press > 0, "a press low on the track scrolls down");
    driven.mouse(
        bar.x,
        bar.y,
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
    );
    driven.frame();
    assert_eq!(
        driven.attach.model.dock_bar.get().map(|(_, first)| first),
        Some(0),
        "dragged back to the top"
    );
    driven.mouse(
        bar.x,
        bar.y,
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
    );
    assert!(!driven.attach.model.dragging_dock_content);
    let _ = std::fs::remove_dir_all(root);
}

/// An engine that asks the host for two reads once, one of which panics,
/// and says what came back.
struct Fetcher {
    asked: bool,
    said: Vec<String>,
}

impl Extension for Fetcher {
    fn identity(&self) -> String {
        "fetcher".to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        Vec::new()
    }

    fn view(&self, _surface: &str, _space: Size) -> Option<View> {
        None
    }

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<Section> {
        None
    }

    fn command(&mut self, _surface: &str, _command: Command, _space: Size) -> Outcome {
        Outcome::Stay
    }

    fn hit(&mut self, _surface: &str, _hit: ViewHit, _space: Size) -> Outcome {
        Outcome::Stay
    }

    fn scroll(
        &mut self,
        _surface: &str,
        _target: ScrollTarget,
        _direction: ScrollDirection,
        _space: Size,
    ) {
    }

    fn jobs(&mut self) -> Vec<uze_extensions::contract::Job> {
        if std::mem::replace(&mut self.asked, true) {
            return Vec::new();
        }
        let answer = |said: &'static str| -> uze_extensions::contract::JobFallback {
            Box::new(move || uze_extensions::contract::Opaque::new(said.to_owned()))
        };
        vec![
            uze_extensions::contract::Job {
                run: Box::new(|host| {
                    let read = host.read_file(std::path::Path::new(file!())).is_ok();
                    uze_extensions::contract::Opaque::new(format!("read through the host: {read}"))
                }),
                fallback: answer("never"),
            },
            uze_extensions::contract::Job {
                run: Box::new(|_| panic!("a read that fails")),
                fallback: answer("fell back"),
            },
        ]
    }

    fn absorb(&mut self, answer: uze_extensions::contract::Opaque) -> Outcome {
        if let Some(said) = answer.downcast::<String>() {
            self.said.push(said);
        }
        Outcome::Stay
    }
}

impl Hosted for Fetcher {
    fn replace(&mut self, _compiled: CompiledExtension) {}
    fn set_data(&mut self, _source: &str, _data: uze_application::SourceData) {}
    fn inspect(&self, _surface: &str) -> Inspection {
        Inspection {
            nodes: self.said.clone(),
            ..Inspection::default()
        }
    }
}

fn fetcher(_compiled: CompiledExtension, _locale: &str) -> Box<dyn Hosted> {
    Box::new(Fetcher {
        asked: false,
        said: Vec::new(),
    })
}

/// A hosted engine's jobs run on a thread with the workspace's host, once,
/// and each answer reaches the same engine; a job that panics answers its
/// fallback rather than leaving the engine waiting.
#[test]
fn a_hosted_engines_jobs_run_off_the_frame_and_answer_it() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-jobs"));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    // The spec counts the checkout in front on the same pass, and the
    // code reads its history; off, only this extension's jobs are handed
    // out.
    model.follow_extension_switch(
        [
            uze_extensions::spec::CATALOG.id.to_owned(),
            uze_extensions::code::CATALOG.id.to_owned(),
        ]
        .into(),
    );
    model.remembered.declared = HostedExtensions {
        engine: fetcher,
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![loaded(Ok(compiled()))],
    }));
    // One engine for the extension, however many surfaces it declares.
    let declared = &model.remembered.declared;
    let ours = |owner: &super::Owner| owner.declared() == Some(&identity());
    assert!(
        declared
            .slots
            .iter()
            .filter(|slot| ours(&slot.owner))
            .count()
            > 1
    );
    let engines = declared.engines.keys().filter(|owner| ours(owner)).count();
    assert_eq!(engines, 1);
    let (sender, receiver) = std::sync::mpsc::channel();
    assert_eq!(model.schedule_engine_jobs(&home, &sender), 2);
    let mut absorbed = 0;
    while absorbed < 2 * engines {
        let answer = receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("each job answers");
        assert!(model.absorb_engine_answer(answer));
        absorbed += 1;
    }
    assert_eq!(model.schedule_engine_jobs(&home, &sender), 0, "asked once");
    let engine = model
        .remembered
        .declared
        .engines
        .get(&super::Owner::Declared(identity()))
        .expect("an engine");
    let mut said = engine.inspect("board").nodes;
    said.sort();
    assert_eq!(said, ["fell back", "read through the host: true"]);
}

/// A discovery that failed (its thread panicked, or the application could
/// not be built) still releases the discovery it reserved, so the next one
/// is asked when it falls due rather than never.
#[test]
fn a_failed_discovery_lets_the_next_one_be_asked() {
    let (mut model, _first, _second) = two_agents_with_shells();
    let declared = &mut model.remembered.declared;
    declared.discovery_pending = true;
    declared.discovered_at = Some(std::time::Instant::now());
    model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: None,
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: Vec::new(),
    });
    let declared = &model.remembered.declared;
    assert!(!declared.discovery_pending);
    assert!(
        declared.discovery_due(declared.plugins_seen + 1, std::time::Instant::now()),
        "a plugin installed since is looked for"
    );
}

/// A full-frame declared view's scrollbar answers the pointer as the
/// drawer's does: a press low on its track scrolls the view there.
#[test]
fn a_declared_views_scrollbar_follows_the_pointer() {
    let root = uze_testkit::temp::scratch("declared-view-scrollbar");
    let home = uze_application::UzeHome::at(root.join("uze"));
    the_gsd_band_installed(&root, &home);
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Discover(Default::default()),
    );
    enabled_where_its_agent_stands(&mut model);
    let slot = view_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.area = ratatui::layout::Rect::new(0, 0, 120, 14);
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::OpenDeclared(slot));
    driven.press(button.x, button.y);
    driven.frame();
    let shown = |driven: &Driven<'_>| {
        let space = uze_extensions::view::Size {
            width: 100,
            height: 10,
        };
        match driven
            .attach
            .model
            .view_in_pane(space)
            .map(|view| view.content)
        {
            Some(Content::Tree { scroll, .. }) => scroll,
            _ => panic!("a tree"),
        }
    };
    assert_eq!(shown(&driven), 0);
    let bar = drawn(
        &driven.attach.model,
        WorkspaceHit::Extension(ExtensionHit {
            slot,
            hit: ViewHit::DragContentScrollbar,
        }),
    );
    driven.press(bar.x, bar.y + bar.height - 1);
    driven.frame();
    assert!(shown(&driven) > 0, "a press low on the track scrolls down");
    let _ = std::fs::remove_dir_all(root);
}

/// A Git checkout outside the system's temporary directory, which an
/// extension may never read: beside the build's own output.
fn checkout_outside_the_temporary_directory(label: &str) -> PathBuf {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/extension-checkouts")
        .join(format!("{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&checkout);
    std::fs::create_dir_all(checkout.join(".planning")).unwrap();
    uze_testkit::git::isolated_git_in(&checkout, &["init", "-q"]);
    uze_platform::path::canonical(&checkout).unwrap()
}

/// Every engine job asked now and the answers they give, absorbed until
/// `wanted` is drawn or the wait runs out; the rows drawn last.
fn drawn_once_answered(
    model: &mut WorkspaceModel,
    home: &uze_application::UzeHome,
    area: ratatui::layout::Rect,
    wanted: &str,
) -> Vec<String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        model.schedule_engine_jobs(home, &sender);
        while let Ok(answer) = receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            model.absorb_engine_answer(answer);
        }
        let rows = frame_rows_at(model, area);
        if shown(&rows, wanted) || std::time::Instant::now() > deadline {
            return rows;
        }
    }
}

#[test]
fn a_section_is_drawn_from_a_script_that_reads_the_checkout_once_enabled() {
    let root = uze_testkit::temp::scratch("declared-script");
    let home = uze_application::UzeHome::at(root.join("uze"));
    let band = the_gsd_band_installed(&root, &home)
        .parent()
        .unwrap()
        .to_path_buf();
    let manifest = band.join("extension.kdl");
    let declared = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        declared.replace(
            "source \"milestone\" {",
            "surface \"lines\" slot=\"section\" view=\"lines.uze\" source=\"state\" title=\"State\"\n\n\
             source \"milestone\" {",
        ),
    )
    .unwrap();
    std::fs::write(
        band.join("lines.uze"),
        "<view>\nStack {\n    Text \"{lines} lines of state\"\n}\n</view>\n",
    )
    .unwrap();
    let checkout = checkout_outside_the_temporary_directory("declared-script");
    std::fs::write(checkout.join(".planning/STATE.md"), "a\nb\nc\nd").unwrap();

    let mut model = agent_session_in(&checkout.display().to_string());
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Discover(Default::default()),
    );
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let rows = drawn_once_answered(&mut model, &home, area, "needs consent");
    assert!(shown(&rows, "needs consent"), "{rows:#?}");
    assert!(
        shown(&rows, "v1.23 65%"),
        "the fixture draws as before: {rows:#?}"
    );

    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let identity = ExtensionIdentity {
        marketplace: "studio".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    };
    let extensions = app.extensions();
    extensions
        .consent(
            &identity,
            &extensions.digest(&identity).unwrap(),
            uze_application::Asker::Operator,
        )
        .unwrap();
    let installed: Vec<InstalledExtension> = model
        .remembered
        .declared
        .extensions
        .values()
        .map(|hosted| hosted.installed.clone())
        .collect();
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Recompile(installed.clone()),
    );
    // One row: the reason, cut to the column, and the way to answer it.
    let rows = drawn_once_answered(&mut model, &home, area, "needs enabling");
    assert!(shown(&rows, "needs enabling"), "{rows:#?}");
    assert!(shown(&rows, "review"), "{rows:#?}");

    extensions
        .enable(
            &identity,
            &uze_application::project_root_of(&checkout),
            uze_application::Asker::Operator,
        )
        .unwrap();
    answered(&mut model, &home, super::ExtensionJob::Recompile(installed));
    let rows = drawn_once_answered(&mut model, &home, area, "4 lines of state");
    assert!(shown(&rows, "4 lines of state"), "{rows:#?}");
    assert_eq!(model.remembered.declared.runner.runs(), 1);

    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(checkout).unwrap();
}

/// Consent and enabling are a record, not a compile: the discovery the
/// answer starts finds it moved, and the section shown asks again without
/// waiting for its extension to be saved or its view to be opened.
#[test]
fn a_section_asks_again_once_its_extension_is_enabled_or_disabled() {
    let root = uze_testkit::temp::scratch("declared-record");
    let home = uze_application::UzeHome::at(root.join("uze"));
    let band = the_gsd_band_installed(&root, &home)
        .parent()
        .unwrap()
        .to_path_buf();
    let manifest = band.join("extension.kdl");
    let declared = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        declared.replace(
            "source \"milestone\" {",
            "surface \"lines\" slot=\"section\" view=\"lines.uze\" source=\"state\" title=\"State\"\n\n\
             source \"milestone\" {",
        ),
    )
    .unwrap();
    std::fs::write(
        band.join("lines.uze"),
        "<view>\nStack {\n    Text \"{lines} lines of state\"\n}\n</view>\n",
    )
    .unwrap();
    let checkout = checkout_outside_the_temporary_directory("declared-record");
    std::fs::write(checkout.join(".planning/STATE.md"), "a\nb\nc\nd").unwrap();

    let mut model = agent_session_in(&checkout.display().to_string());
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        &home,
        super::ExtensionJob::Discover(Default::default()),
    );
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let rows = drawn_once_answered(&mut model, &home, area, "needs consent");
    assert!(shown(&rows, "needs consent"), "{rows:#?}");

    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let identity = ExtensionIdentity {
        marketplace: "studio".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    };
    let project = uze_application::project_root_of(&checkout);
    let extensions = app.extensions();
    extensions
        .consent(
            &identity,
            &extensions.digest(&identity).unwrap(),
            uze_application::Asker::Operator,
        )
        .unwrap();
    extensions
        .enable(&identity, &project, uze_application::Asker::Operator)
        .unwrap();
    let known = model.remembered.declared.known();
    answered(&mut model, &home, super::ExtensionJob::Discover(known));
    let rows = drawn_once_answered(&mut model, &home, area, "4 lines of state");
    assert!(shown(&rows, "4 lines of state"), "{rows:#?}");
    assert!(!shown(&rows, "needs consent"), "{rows:#?}");

    extensions.disable(&identity, &project).unwrap();
    let known = model.remembered.declared.known();
    answered(&mut model, &home, super::ExtensionJob::Discover(known));
    let rows = drawn_once_answered(&mut model, &home, area, "needs enabling");
    assert!(shown(&rows, "needs enabling"), "{rows:#?}");
    assert!(!shown(&rows, "4 lines of state"), "{rows:#?}");

    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(checkout).unwrap();
}

/// The real `gsd-band`, linked, consented to and enabled, with a section
/// drawn from its `state` script, and its board open with the inspector
/// opened by its key.
fn inspecting_the_linked_band(label: &str) -> (Driven<'static>, PathBuf, PathBuf, PathBuf) {
    let root = uze_testkit::temp::scratch(label);
    let home: &'static uze_application::UzeHome =
        Box::leak(Box::new(uze_application::UzeHome::at(root.join("uze"))));
    let band = the_gsd_band_installed(&root, home)
        .parent()
        .unwrap()
        .to_path_buf();
    let manifest = band.join("extension.kdl");
    let declared = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        declared.replace(
            "source \"milestone\" {",
            "surface \"lines\" slot=\"section\" view=\"lines.uze\" source=\"state\" title=\"State\"\n\n\
             source \"milestone\" {",
        ),
    )
    .unwrap();
    std::fs::write(
        band.join("lines.uze"),
        "<view>\nStack {\n    Text \"{lines} lines of state\"\n}\n</view>\n",
    )
    .unwrap();
    let checkout = checkout_outside_the_temporary_directory(label);
    std::fs::write(checkout.join(".planning/STATE.md"), "a\nb\nc\nd").unwrap();
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let identity = ExtensionIdentity {
        marketplace: "studio".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    };
    app.extensions()
        .consent(
            &identity,
            &app.extensions().digest(&identity).unwrap(),
            uze_application::Asker::Operator,
        )
        .unwrap();
    app.extensions()
        .enable(
            &identity,
            &uze_application::project_root_of(&checkout),
            uze_application::Asker::Operator,
        )
        .unwrap();

    let mut model = agent_session_in(&checkout.display().to_string());
    model.remembered.declared.discovered_at = Some(std::time::Instant::now());
    answered(
        &mut model,
        home,
        super::ExtensionJob::Discover(Default::default()),
    );
    let area = ratatui::layout::Rect::new(0, 0, 160, 50);
    let rows = drawn_once_answered(&mut model, home, area, "4 lines of state");
    assert!(shown(&rows, "4 lines of state"), "{rows:#?}");
    let slot = view_slot(&model);
    super::open_surface(&mut model, slot);
    let mut driven = driven(model, home);
    driven.area = area;
    driven.press_key(key_event(uze_keys::Chord::parse("i").unwrap()));
    assert!(driven.attach.model.inspecting);
    assert!(driven.attach.model.inspector_holds_keys(), "opened last");
    driven.frame();
    (driven, root, checkout, band)
}

/// Pumps the attach, as its loop does, until `wanted` is drawn.
fn pumped_until(driven: &mut Driven<'_>, wanted: &str) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let _ = driven.pump();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let rows = frame_rows_at(&mut driven.attach.model, driven.area);
        if shown(&rows, wanted) || std::time::Instant::now() > deadline {
            return rows;
        }
    }
}

fn tool(driven: &Driven<'_>, wanted: &super::InspectorTool) -> usize {
    driven
        .attach
        .model
        .inspector_tools()
        .iter()
        .position(|tool| tool == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} is not offered"))
}

/// The inspector of a linked extension lists its runs, events, grants
/// and cost, and its author reruns a source with edited input: the rerun
/// goes through the runner like any question, and is listed with the
/// input it was asked with (D25, D28).
#[test]
fn a_linked_extensions_inspector_lists_its_runs_and_reruns_with_edited_input() {
    let (mut driven, root, checkout, _) = inspecting_the_linked_band("declared-inspector-rerun");
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    for wanted in [
        "author",
        "rerun state with input…",
        "feed state from fixtures/state.json",
        "emit paths.changed .planning",
        "size · the room it has",
        "record session events",
    ] {
        assert!(shown(&rows, wanted), "{wanted}: {rows:#?}");
    }
    let inspection = driven.attach.model.inspection().unwrap();
    let listed = |lines: &[String], wanted: &str| lines.iter().any(|line| line.contains(wanted));
    assert!(
        listed(&inspection.runs, "source state"),
        "{:?}",
        inspection.runs
    );
    assert!(
        listed(&inspection.runs, "answered"),
        "{:?}",
        inspection.runs
    );
    assert!(
        listed(&inspection.runs, "source milestone"),
        "{:?}",
        inspection.runs
    );
    assert_eq!(inspection.grants, ["read:checkout"]);
    assert!(
        listed(&inspection.cost, "runs/min · p95"),
        "{:?}",
        inspection.cost
    );

    let rerun = tool(&driven, &super::InspectorTool::Rerun("state".to_owned()));
    let row = drawn(&driven.attach.model, WorkspaceHit::InspectorTool(rerun));
    driven.press(row.x, row.y);
    let (source, buffer) = driven
        .attach
        .model
        .inspector
        .edit
        .as_ref()
        .expect("editing");
    assert_eq!(source, "state");
    assert!(buffer.text().starts_with('{'), "{}", buffer.text());
    driven.frame();
    let cancel = drawn(
        &driven.attach.model,
        WorkspaceHit::HintAction(uze_keys::Action::Dismiss),
    );
    assert!(cancel.y > row.y, "its keys stand under the field");

    driven.attach.model.inspector.edit = Some((
        "state".to_owned(),
        super::RenameBuffer::new(r#"{"phase":"7""#.to_owned()),
    ));
    driven.press_key(key_event(uze_keys::Chord::parse("}").unwrap()));
    assert_eq!(
        driven
            .attach
            .model
            .inspector
            .edit
            .as_ref()
            .unwrap()
            .1
            .text(),
        r#"{"phase":"7"}"#,
        "typing lands in the field"
    );
    driven.frame();
    let rerun_hint = drawn(
        &driven.attach.model,
        WorkspaceHit::HintAction(uze_keys::Action::Activate),
    );
    driven.press(rerun_hint.x, rerun_hint.y);
    assert!(driven.attach.model.inspector.edit.is_none(), "asked");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while driven.attach.model.remembered.declared.runner.runs() < 2
        && std::time::Instant::now() < deadline
    {
        let _ = driven.pump();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let runs = driven.attach.model.inspection().unwrap().runs;
    assert!(runs[0].starts_with("refresh state"), "{runs:?}");
    assert!(runs[1].contains(r#"input {"phase":"7"}"#), "{runs:?}");
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(shown(&rows, "put state's input back"), "{rows:#?}");
    assert_eq!(driven.attach.model.remembered.declared.runner.runs(), 2);

    driven.settle();
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(checkout).unwrap();
}

/// The author's other tools: a source fed by its fixture and back, a
/// synthetic event, another size and theme, and a session's events
/// recorded into a fixture; the keys walk them as the pointer does.
#[test]
fn a_linked_extensions_author_switches_fixtures_emits_resizes_and_records() {
    let (mut driven, root, checkout, band) = inspecting_the_linked_band("declared-inspector-tools");
    std::fs::write(band.join("fixtures/state.json"), r#"{ "lines": 42 }"#).unwrap();
    let feed = tool(&driven, &super::InspectorTool::Fixture("state".to_owned()));
    let row = drawn(&driven.attach.model, WorkspaceHit::InspectorTool(feed));
    driven.press(row.x, row.y);
    let rows = pumped_until(&mut driven, "42 lines of state");
    assert!(shown(&rows, "42 lines of state"), "{rows:#?}");
    assert!(shown(&rows, "feed state live"), "{rows:#?}");
    let row = drawn(&driven.attach.model, WorkspaceHit::InspectorTool(feed));
    driven.press(row.x, row.y);
    let rows = pumped_until(&mut driven, "4 lines of state");
    assert!(shown(&rows, "4 lines of state"), "{rows:#?}");

    let emit = tool(
        &driven,
        &super::InspectorTool::Emit(uze_contract::EventName::PathsChanged {
            paths: vec![".planning".to_owned()],
        }),
    );
    let row = drawn(&driven.attach.model, WorkspaceHit::InspectorTool(emit));
    driven.press(row.x, row.y);
    let events = driven.attach.model.inspection().unwrap().events;
    assert_eq!(events[0], "paths.changed .planning · synthetic");

    // The keys walk the tools from where the click left them.
    let size = tool(&driven, &super::InspectorTool::Size);
    for _ in emit..size {
        driven.press_key(key_event(uze_keys::Chord::parse("down").unwrap()));
    }
    driven.press_key(key_event(uze_keys::Chord::parse("enter").unwrap()));
    assert_eq!(driven.attach.model.inspected_size(), Some((80, 24)));
    driven.press_key(key_event(uze_keys::Chord::parse("down").unwrap()));
    driven.press_key(key_event(uze_keys::Chord::parse("enter").unwrap()));
    assert_eq!(driven.attach.model.inspected_theme(), Some("dark"));
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(shown(&rows, "size · 80 by 24"), "{rows:#?}");
    assert!(shown(&rows, "theme · dark"), "{rows:#?}");

    let record = tool(&driven, &super::InspectorTool::Record);
    let row = drawn(&driven.attach.model, WorkspaceHit::InspectorTool(record));
    driven.press(row.x, row.y);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let told = |driven: &Driven<'_>| {
        let toasts = &driven.attach.model.remembered.toasts;
        toasts.iter().any(|toast| toast.text == "events recorded")
    };
    while !told(&driven) && std::time::Instant::now() < deadline {
        let _ = driven.pump();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(told(&driven), "the recording was never told");
    let recorded = band.join("fixtures/session.json");
    let events: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&recorded).unwrap()).unwrap();
    assert_eq!(events[0]["synthetic"], serde_json::json!(true), "{events}");

    driven.press_key(key_event(uze_keys::Chord::parse("esc").unwrap()));
    assert!(!driven.attach.model.inspecting);
    assert_eq!(
        driven.attach.model.inspected_size(),
        None,
        "the author's size goes with the inspector"
    );
    driven.settle();
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(checkout).unwrap();
}

/// An extension from a marketplace that is not linked offers its author
/// nothing to provoke: the inspector only reads it.
#[test]
fn an_installed_extensions_inspector_offers_no_tools() {
    let mut driven = inspector_open();
    assert!(driven.attach.model.inspector_tools().is_empty());
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(!shown(&rows, "author"), "{rows:#?}");
}
