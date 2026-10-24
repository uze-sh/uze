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
        discovered_at: Some(std::time::Instant::now()),
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
        discovered_at: Some(std::time::Instant::now()),
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
    market.join("plugins/gsd/extensions/gsd-band/board.kdl")
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
    let slot = view_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let button = drawn(&driven.attach.model, WorkspaceHit::OpenDeclared(slot));
    driven.press(button.x, button.y);
    assert_eq!(driven.attach.model.declared_open, Some(slot));

    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    for wanted in [
        "gsd-band · gsd@studio",
        "GSD · v1.23 Assistente do lojista",
        "Consulta de dados e diagn",
        "Relatórios do lojista",
        "17/19",
        "65% · 3 of 6 phases",
        "▶ run phase 12",
    ] {
        assert!(shown(&rows, wanted), "{wanted}: {rows:#?}");
    }
    // The band's section in the sidebar, drawn from the same view: its
    // header row fits the column by leaving the title out, not by folding
    // the whole band away.
    let sidebar: Vec<String> = rows
        .iter()
        .map(|row| row.chars().take_while(|cell| *cell != '│').collect())
        .collect();
    assert!(shown(&sidebar, "▾ GSD"), "{sidebar:#?}");
    assert!(shown(&sidebar, "3 of 6 phases"), "{sidebar:#?}");

    let log_tab = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit::Declared {
                slot: on,
                hit:
                    ViewHit::Node {
                        act: NodeAct::Tab(1),
                        ..
                    },
            }) if *on == slot => Some(*rect),
            _ => None,
        })
        .expect("the Log tab is drawn");
    driven.press(log_tab.x, log_tab.y);
    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(shown(&rows, "turno concluído em 4m12s"), "{rows:#?}");
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
    assert!(shown(&rows, "turno concluído em 4m12s"), "{rows:#?}");
    assert_eq!(driven.attach.model.declared_open, Some(slot), "kept open");
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
    let mut declared = DeclaredExtensions::default();
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
    super::open_declared(&mut model, slot);
    model.tell_declared(
        Command::NextView,
        Size {
            width: 80,
            height: 20,
        },
    );
    model.close_declared();
    assert_eq!(model.remembered.declared.places.len(), 1, "a place kept");

    // Installed: only the newcomer is loaded, the board is as it was.
    let changed = model.absorb_extension_answer(ExtensionAnswer {
        discovered: Some(vec![installed(), notes()]),
        recompiled: Vec::new(),
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
        .map(|slot| slot.identity.id.clone())
        .collect();
    assert_eq!(views, ["gsd-band", "notes"]);
    assert_eq!(model.declared_views().next().unwrap().id, slot, "kept");

    // Removed: its slots close and its place goes with them.
    super::open_declared(&mut model, slot);
    let changed = model.absorb_extension_answer(ExtensionAnswer {
        discovered: Some(vec![notes()]),
        recompiled: Vec::new(),
        loaded: Vec::new(),
    });
    assert!(changed);
    assert_eq!(model.declared_open, None, "its open view closed");
    let declared = &model.remembered.declared;
    assert!(
        declared
            .slots
            .iter()
            .all(|slot| slot.identity == notes().identity)
    );
    assert!(declared.places.is_empty(), "its place dropped");

    // Nothing moved: nothing to redraw.
    assert!(!model.absorb_extension_answer(ExtensionAnswer {
        discovered: Some(vec![notes()]),
        recompiled: Vec::new(),
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
    assert!(model.remembered.declared.slots.is_empty());
    assert!(!model.remembered.declared.discovery_pending);
}

#[test]
fn every_engine_is_built_for_the_operators_language() {
    fn portuguese_only(compiled: CompiledExtension, locale: &str) -> Box<dyn Hosted> {
        assert_eq!(locale, "pt", "the engine was handed another language");
        board(compiled, locale)
    }
    assert_eq!(
        DeclaredExtensions::default().locale,
        uze_platform::locale::language(),
        "the platform's answer, read once"
    );
    let mut declared = DeclaredExtensions {
        engine: portuguese_only,
        locale: "pt".to_owned(),
        ..DeclaredExtensions::default()
    };
    declared.absorb_loaded(loaded(Ok(compiled())));
    assert_eq!(declared.slots.len(), 2);
}

fn inspector_open() -> Driven<'static> {
    let home: &'static uze_application::UzeHome = Box::leak(Box::new(
        uze_application::UzeHome::at(uze_testkit::temp::scratch("declared-inspector-drawer")),
    ));
    let mut model = hosting_the_board();
    let slot = view_slot(&model);
    super::open_declared(&mut model, slot);
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
    assert!(
        driven.attach.model.declared_open.is_some(),
        "the view stays"
    );
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
    let layout = super::compute_layout(driven.area, driven.attach.model.sidebar_width);
    assert!(layout.pane.right() - rule.x <= layout.pane.width / 2 + 1);
}
