//! The places a declared extension stands beyond a view, a section and a
//! drawer: a status row in the sidebar's header, a band over the agent's
//! pane, and an overlay over the frame.

use super::support::*;
use super::{
    ExtensionAnswer, Hosted, HostedExtensions, LoadedExtension, WorkspaceHit, WorkspaceModel,
};
use crossterm::event::{MouseButton, MouseEventKind};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension};
use uze_extensions::ExtensionHit;
use uze_extensions::contract::{Extension, Outcome, SurfaceInfo};
use uze_extensions::view::{
    Command, Content, Layout, Node, NodeAct, NodeKind, Role, ScrollDirection, ScrollTarget,
    Section, Size, Span, View, ViewHit,
};

/// An engine that draws, on every surface, the text its source last
/// answered, and on its overlay a button that copies a note.
struct Probe {
    text: String,
}

const COPY: usize = 2;

fn text(id: usize, said: &str) -> Node {
    Node::new(
        id,
        NodeKind::Text {
            spans: vec![Span::new(said.to_owned(), Role::Default)],
            truncate: false,
            still: false,
        },
    )
}

impl Extension for Probe {
    fn identity(&self) -> String {
        "probe".to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        Vec::new()
    }

    fn view(&self, surface: &str, _space: Size) -> Option<View> {
        let mut root = Node::new(0, NodeKind::Stack { gap: 0 });
        if !self.text.is_empty() {
            root.children.push(text(1, &self.text));
        }
        if surface == "picker" {
            root.children.push(Node::new(
                COPY,
                NodeKind::Button {
                    label: "Copy the note".to_owned(),
                    primary: false,
                },
            ));
        }
        Some(View {
            tabs: Vec::new(),
            title: vec![Span::new("Notes", Role::Muted)],
            caption: Vec::new(),
            navigator: None,
            content: Content::Tree {
                heading: String::new(),
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
            label: String::new(),
        })
    }

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<Section> {
        None
    }

    fn command(&mut self, _surface: &str, command: Command, _space: Size) -> Outcome {
        match command {
            Command::Close => Outcome::Close,
            _ => Outcome::Stay,
        }
    }

    fn hit(&mut self, _surface: &str, hit: ViewHit, _space: Size) -> Outcome {
        match hit {
            ViewHit::Node {
                node: COPY,
                act: NodeAct::Press,
            } => Outcome::Copy("a note".to_owned()),
            _ => Outcome::Stay,
        }
    }

    fn scroll(
        &mut self,
        _surface: &str,
        _target: ScrollTarget,
        _direction: ScrollDirection,
        _space: Size,
    ) {
    }
}

impl Hosted for Probe {
    fn set_data(&mut self, _source: &str, data: uze_application::SourceData) {
        if let Ok(data) = data {
            self.text = data["text"].as_str().unwrap_or_default().to_owned();
        }
    }

    fn press(&mut self, _surface: &str, action: &str) -> Outcome {
        match action {
            "open-picker" => Outcome::Show("picker".to_owned()),
            _ => Outcome::Stay,
        }
    }
}

fn probe(_compiled: CompiledExtension, _locale: &str) -> Box<dyn Hosted> {
    Box::new(Probe {
        text: String::new(),
    })
}

fn identity(id: &str) -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "proving".to_owned(),
        plugin: "proof".to_owned(),
        id: id.to_owned(),
    }
}

fn installed(id: &str) -> InstalledExtension {
    InstalledExtension {
        identity: identity(id),
        dir: PathBuf::from(format!("/proving/plugins/proof/extensions/{id}")),
        linked: false,
    }
}

fn surface(id: &str, slot: uze_contract::Slot) -> uze_contract::SurfaceDecl {
    uze_contract::SurfaceDecl {
        band: (slot == uze_contract::Slot::Band).then_some(uze_contract::BandDecl {
            rows: 2,
            place: uze_contract::BandPlace::Top,
        }),
        bar: (slot == uze_contract::Slot::Bar).then_some(uze_contract::BarDecl::default()),
        drawer: (slot == uze_contract::Slot::Drawer).then_some(uze_contract::DrawerDecl::default()),
        id: id.to_owned(),
        slot,
        view: format!("{id}.kdl"),
        source: "notes".to_owned(),
        title: Some(match slot {
            uze_contract::Slot::Overlay => "Notes".to_owned(),
            _ => id.to_owned(),
        }),
        applies_when: None,
    }
}

/// `id`, declaring `surfaces`, its one source a fixture: it runs nothing,
/// so it is on wherever its switch is.
fn compiled(id: &str, surfaces: Vec<uze_contract::SurfaceDecl>) -> CompiledExtension {
    CompiledExtension {
        identity: identity(id),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            surfaces,
            wants: Vec::new(),
            sources: vec![uze_contract::SourceDecl::new(
                "notes",
                uze_contract::SourceKind::Fixture("fixtures/notes.json".to_owned()),
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

fn loaded(compiled: CompiledExtension) -> LoadedExtension {
    LoadedExtension {
        installed: installed(&compiled.identity.id),
        stamp: 1,
        compiled: Ok(compiled),
        data: BTreeMap::new(),
    }
}

/// The agent in front, with `hosted` found and compiled.
fn hosting(hosted: Vec<CompiledExtension>) -> WorkspaceModel {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = HostedExtensions {
        engine: probe,
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    let discovered = hosted
        .iter()
        .map(|compiled| installed(&compiled.identity.id))
        .collect();
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(discovered),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: hosted.into_iter().map(loaded).collect(),
    }));
    model
}

/// `notes`, with a view, a status row, a two-row band and an overlay.
fn hosting_notes() -> WorkspaceModel {
    hosting(vec![compiled(
        "notes",
        vec![
            surface("board", uze_contract::Slot::View),
            surface("meter", uze_contract::Slot::Status),
            surface("strip", uze_contract::Slot::Band),
            surface("picker", uze_contract::Slot::Overlay),
        ],
    )])
}

/// What `id`'s source answers now: `said`, or nothing when it is empty.
fn answers(model: &mut WorkspaceModel, id: &str, said: &str) {
    model
        .remembered
        .declared
        .engines
        .get_mut(&super::Owner::Declared(identity(id)))
        .expect("hosted")
        .set_data("notes", Ok(serde_json::json!({ "text": said })));
    model.dirty = true;
}

fn slot_of(model: &WorkspaceModel, surface: &str) -> u32 {
    model
        .remembered
        .declared
        .slots
        .iter()
        .find(|slot| slot.owner.declared().is_some() && slot.surface.id == surface)
        .expect("hosted")
        .id
}

const ROOMY: ratatui::layout::Rect = ratatui::layout::Rect::new(0, 0, 120, 40);

fn layout(model: &WorkspaceModel) -> super::WorkspaceLayout {
    super::compute_layout(ROOMY, model.sidebar_width, model.reserved())
}

/// The sidebar's rows as drawn, each cut at the column's edge.
fn sidebar_rows(model: &mut WorkspaceModel) -> Vec<String> {
    let width = usize::from(layout(model).sidebar.width);
    frame_rows_at(model, ROOMY)
        .into_iter()
        .map(|row| row.chars().take(width).collect::<String>())
        .collect()
}

#[test]
fn a_status_row_stands_under_the_header_folded_and_an_empty_answer_frees_it() {
    let mut model = hosting_notes();
    answers(
        &mut model,
        "notes",
        "GPU 9% · a tail far too long for any sidebar to hold whole",
    );
    let rows = sidebar_rows(&mut model);
    let row = rows
        .iter()
        .position(|row| row.contains("GPU 9%"))
        .unwrap_or_else(|| panic!("the status row is drawn: {rows:#?}"));
    assert_eq!(row, 2, "beneath the header and its rule: {rows:#?}");
    assert!(rows[row].contains('…'), "folded, never wrapped: {rows:#?}");
    assert!(
        !rows[row + 1].contains("sidebar"),
        "nothing of it on the next row: {rows:#?}"
    );

    let before = rows[row + 1].clone();
    answers(&mut model, "notes", "");
    let rows = sidebar_rows(&mut model);
    assert!(!rows.iter().any(|row| row.contains("GPU")), "{rows:#?}");
    assert_eq!(rows[row], before, "the rows under it move up into its row");
}

#[test]
fn the_status_rows_follow_the_operators_order_and_discovery_without_one() {
    let mut model = hosting(vec![
        compiled("clock", vec![surface("time", uze_contract::Slot::Status)]),
        compiled("notes", vec![surface("meter", uze_contract::Slot::Status)]),
    ]);
    answers(&mut model, "clock", "12:00");
    answers(&mut model, "notes", "GPU 9%");
    let at = |rows: &[String], said: &str| rows.iter().position(|row| row.contains(said));
    let rows = sidebar_rows(&mut model);
    assert!(
        at(&rows, "12:00") < at(&rows, "GPU 9%"),
        "in the order they were found: {rows:#?}"
    );

    model.management_layout.status_order =
        vec![crate::ui::extension_switch::declared_id(&identity("notes"))];
    let rows = sidebar_rows(&mut model);
    assert!(
        at(&rows, "GPU 9%") < at(&rows, "12:00"),
        "the one the operator moved up first: {rows:#?}"
    );
}

/// The pane of the agent in front, with no band over it: under the same
/// bar, which stands for what is installed, never for what answers.
fn unbanded(model: &WorkspaceModel) -> ratatui::layout::Rect {
    let reserved = super::render::Reserved {
        bar: model.has_bar(),
        ..Default::default()
    };
    super::compute_layout(ROOMY, model.sidebar_width, reserved).pane
}

#[test]
fn a_band_takes_its_rows_before_the_pane_is_sized_and_no_answer_resizes_it() {
    let mut model = hosting_notes();
    let whole = unbanded(&model);
    let waiting = layout(&model);
    assert_eq!(waiting.pane.y, whole.y + 2, "two rows off the top");
    assert_eq!(waiting.pane.height, whole.height - 2);
    assert_eq!(waiting.pane.width, whole.width);
    let band = waiting.bands[0].1;
    assert_eq!(
        band,
        ratatui::layout::Rect::new(whole.x, whole.y, whole.width, 2)
    );

    answers(&mut model, "notes", "phase 3 of 5");
    assert_eq!(layout(&model).pane, waiting.pane, "an answer sizes nothing");
    let rows = frame_rows_at(&mut model, ROOMY);
    let first: String = rows[usize::from(band.y)]
        .chars()
        .skip(usize::from(band.x))
        .collect();
    assert!(first.contains("phase 3 of 5"), "{rows:#?}");
    assert!(
        !first.trim_end().ends_with("notes"),
        "no id at the end of its first row, which read as a button: {first:?}"
    );

    answers(&mut model, "notes", "");
    assert_eq!(
        layout(&model).pane,
        waiting.pane,
        "nor does answering nothing"
    );
    let rows = frame_rows_at(&mut model, ROOMY);
    assert!(!rows.iter().any(|row| row.contains("phase 3")), "{rows:#?}");
}

#[test]
fn a_band_stands_over_an_agents_own_pane_in_a_project_it_is_enabled_in() {
    let mut model = hosting_notes();
    assert_eq!(model.reserved().bands.len(), 1);

    // One that runs code is enabled project by project.
    let notes = identity("notes");
    model
        .remembered
        .declared
        .extensions
        .get_mut(&notes)
        .expect("hosted")
        .per_project = true;
    assert!(model.reserved().bands.is_empty(), "not enabled anywhere");
    let project = model
        .projects
        .get(model.agent_directory_in_front().expect("an agent in front"))
        .cloned()
        .expect("its project");
    model
        .remembered
        .declared
        .extensions
        .get_mut(&notes)
        .expect("hosted")
        .record = Some(Some(uze_application::ExtensionConsent {
        enabled_in: [project].into(),
        ..Default::default()
    }));
    assert_eq!(model.reserved().bands.len(), 1, "enabled in its project");

    // The agent's shell is no agent's pane.
    let shell = model
        .session
        .as_ref()
        .unwrap()
        .selected_space()
        .tabs
        .iter()
        .find(|tab| tab.label == "Agent one shell")
        .map(|tab| tab.id)
        .expect("the agent's shell");
    select_tab(&mut model, shell);
    assert!(model.reserved().bands.is_empty(), "{:?}", model.reserved());

    // Switched off, it takes its rows back.
    let first = model.agent_tab_in_front.expect("the agent");
    select_tab(&mut model, first);
    model.follow_extension_switch(std::collections::BTreeSet::from([
        crate::ui::extension_switch::declared_id(&notes),
    ]));
    assert!(model.reserved().bands.is_empty());
}

#[test]
fn a_click_on_the_panes_first_row_under_a_top_band_reaches_it_as_its_first_row() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("band-click"));
    let mut model = hosting_notes();
    answers(&mut model, "notes", "phase 3 of 5");
    let pane = model.focused_pane();
    let mut snapshot = super::render::blank_pane(pane, 80, 24);
    snapshot.mouse = uze_terminal::MouseMode {
        reports_clicks: true,
        reports_drag: false,
        sgr: true,
    };
    model.panes.insert(pane, snapshot);
    let mut driven = driven(model, &home);
    driven.area = ROOMY;
    driven.frame();
    let drawn = layout(&driven.attach.model);
    let _ = driven.sent();

    let click = |driven: &mut Driven<'_>, column, row| {
        driven.press(column, row);
        driven.mouse(column, row, MouseEventKind::Up(MouseButton::Left));
    };
    click(&mut driven, drawn.pane.x, drawn.pane.y);
    let sent = driven.sent();
    assert!(
        sent.iter().any(|request| matches!(
            request,
            crate::ui::orchestrator::ClientRequest::Input { bytes, .. }
                if bytes.as_slice() == b"\x1b[<0;1;1M"
        )),
        "its own first row and column: {sent:?}"
    );

    // A press on the band itself is never the agent's.
    let band = drawn.bands[0].1;
    click(&mut driven, band.x, band.y);
    assert!(
        !driven.sent().iter().any(|request| matches!(
            request,
            crate::ui::orchestrator::ClientRequest::Input { .. }
        )),
        "the band's rows reach no agent"
    );
}

#[test]
fn an_overlay_opens_from_its_action_attributed_and_esc_gives_the_view_back() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("overlay-action"));
    let mut model = hosting_notes();
    answers(&mut model, "notes", "three notes");
    let board = slot_of(&model, "board");
    let picker = slot_of(&model, "picker");
    super::open_surface(&mut model, board);
    let mut driven = driven(model, &home);
    driven.area = ROOMY;
    let owner = crate::keymap::extension_owner(&identity("notes"));
    let action =
        uze_keys::extension::intern(&uze_keys::extension::action_name(&owner, "open-picker"));
    driven.attach.model.press_declared_action(action);
    assert_eq!(driven.attach.model.open_overlay, Some(picker), "opened");

    driven.frame();
    let rows = frame_rows_at(&mut driven.attach.model, ROOMY);
    let card = super::render::overlay_card(ROOMY);
    assert_eq!(
        (card.width, card.height),
        (84, 28),
        "seventy percent each way"
    );
    let strip: String = rows[usize::from(card.y + 1)]
        .chars()
        .skip(usize::from(card.x))
        .take(usize::from(card.width))
        .collect();
    assert!(
        strip.contains("Notes") && !strip.contains("proof@proving"),
        "headed by its title, not whose it is: {rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("Copy the note")),
        "{rows:#?}"
    );

    driven.press_key(key_event(uze_keys::Chord::parse("esc").unwrap()));
    assert_eq!(driven.attach.model.open_overlay, None, "esc closes it");
    assert_eq!(
        driven.attach.model.open_slot,
        Some(board),
        "and the view it opened over is in front again"
    );
}

#[test]
fn an_overlay_is_offered_in_the_index_and_opens_from_it() {
    let model = hosting_notes();
    let picker = slot_of(&model, "picker");
    let doors = model.declared_view_doors();
    let door = doors
        .iter()
        .find(|door| door.label() == "Open Notes")
        .unwrap_or_else(|| panic!("offered: {doors:?}"));
    assert_eq!(door.description(), "proof@proving/notes");
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("overlay-index"));
    let mut driven = driven(model, &home);
    let uze_keys::Action::Extension(id) = *door else {
        panic!("an extension's door");
    };
    assert!(driven.attach.model.open_declared_door(id));
    assert_eq!(driven.attach.model.open_overlay, Some(picker));
    assert_eq!(driven.attach.model.open_slot, None, "over the pane");
}

#[test]
fn an_overlay_is_modal_a_click_in_it_is_its_own_and_one_outside_closes_it() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("overlay-click"));
    let mut model = hosting_notes();
    let picker = slot_of(&model, "picker");
    model.open_overlay(picker);
    let mut driven = driven(model, &home);
    driven.area = ROOMY;
    driven.frame();
    let copy = driven.hit(|hit| {
        *hit == WorkspaceHit::Extension(ExtensionHit {
            slot: picker,
            hit: ViewHit::Node {
                node: COPY,
                act: NodeAct::Press,
            },
        })
    });
    driven.press(copy.x, copy.y);
    assert_eq!(
        driven.attach.model.clipboard.as_deref(),
        Some("a note"),
        "its button is its engine's"
    );
    assert_eq!(driven.attach.model.open_overlay, Some(picker), "still open");

    // A letter is the overlay's, never the agent's behind it.
    let _ = driven.sent();
    driven.press_key(key_event(uze_keys::Chord::parse("x").unwrap()));
    assert!(
        !driven.sent().iter().any(|request| matches!(
            request,
            crate::ui::orchestrator::ClientRequest::Input { .. }
        )),
        "sealed"
    );

    driven.frame();
    let close = driven.hit(|hit| *hit == WorkspaceHit::CloseOverlay);
    driven.press(close.x, close.y);
    assert_eq!(driven.attach.model.open_overlay, None, "its mark closes it");

    driven.attach.model.open_overlay(picker);
    driven.frame();
    driven.press(0, ROOMY.bottom() - 1);
    assert_eq!(driven.attach.model.open_overlay, None, "a click outside");
    assert!(
        !driven.attach.model.recent.open,
        "and reaches nothing under it"
    );
}

#[test]
fn an_overlay_card_keeps_its_least_room_on_a_small_frame() {
    let small = ratatui::layout::Rect::new(0, 0, 50, 12);
    let card = super::render::overlay_card(small);
    assert_eq!((card.width, card.height), (40, 10));
    assert_eq!((card.x, card.y), (5, 1), "centred");
    let tiny = ratatui::layout::Rect::new(0, 0, 30, 8);
    assert_eq!(
        super::render::overlay_card(tiny),
        tiny,
        "never past the frame"
    );
}

/// A plugin's view and drawer as `notes` declares them, with an item of the
/// bar on `align`'s side that opens the drawer.
fn hosting_a_bar(align: uze_contract::BarAlign) -> WorkspaceModel {
    let mut item = surface("today", uze_contract::Slot::Bar);
    item.bar = Some(uze_contract::BarDecl {
        align,
        opens: Some("panel".to_owned()),
    });
    let mut panel = surface("panel", uze_contract::Slot::Drawer);
    panel.title = Some("Timesheet".to_owned());
    hosting(vec![compiled(
        "notes",
        vec![surface("board", uze_contract::Slot::View), panel, item],
    )])
}

fn bar_row(model: &mut WorkspaceModel) -> String {
    let at = layout(model).bar.expect("the bar stands");
    let rows = frame_rows_at(model, ROOMY);
    rows[usize::from(at.y + 1)].clone()
}

#[test]
fn a_plugins_doors_stand_in_the_bar_lowercase_and_never_in_the_tab_strip() {
    let mut panel = surface("panel", uze_contract::Slot::Drawer);
    panel.title = Some("Timesheet".to_owned());
    let mut model = hosting(vec![compiled(
        "notes",
        vec![surface("board", uze_contract::Slot::View), panel],
    )]);
    let strip = frame_rows_at(&mut model, ROOMY)[0].clone();
    assert!(
        !strip.contains("board") && !strip.contains("imesheet"),
        "{strip}"
    );
    assert!(strip.contains("spec"), "UZE's own stay: {strip}");
    let bar = bar_row(&mut model);
    assert!(bar.contains("board") && bar.contains("timesheet"), "{bar}");
    assert!(!bar.contains("Timesheet"), "drawn lowercase: {bar}");
}

#[test]
fn an_item_stands_on_its_side_and_a_click_opens_what_it_names() {
    let mut model = hosting_a_bar(uze_contract::BarAlign::Right);
    answers(&mut model, "notes", "6h12 today");
    let bar = bar_row(&mut model);
    assert!(
        !bar.contains("timesheet"),
        "the drawer the item opens has the item for its door: {bar}"
    );
    let item = bar
        .find("6h12 today")
        .unwrap_or_else(|| panic!("the item is drawn: {bar}"));
    assert!(
        item > bar.find("board").unwrap(),
        "right of the doors: {bar}"
    );
    assert!(
        bar[item..].trim_end().ends_with("today"),
        "against the bar's right end: {bar}"
    );
    let drawer = slot_of(&model, "panel");
    let column = u16::try_from(bar[..item].chars().count()).unwrap();
    let row = layout(&model).bar.unwrap().y + 1;
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("bar-item"));
    let mut driven = driven(model, &home);
    driven.area = ROOMY;
    driven.frame();
    assert_eq!(driven.attach.model.open_drawer(), None);
    driven.press(column + 1, row);
    assert_eq!(
        driven.attach.model.open_drawer(),
        Some(drawer),
        "the item opens the drawer it names"
    );
}

#[test]
fn an_item_that_opens_something_is_lifted_in_its_own_hue_under_the_pointer() {
    let mut model = hosting_a_bar(uze_contract::BarAlign::Left);
    answers(&mut model, "notes", "6h12 today");
    let row = layout(&model).bar.unwrap().y + 1;
    let column = u16::try_from(bar_row(&mut model).find("6h12").expect("drawn")).unwrap();
    let ink = |model: &mut WorkspaceModel| frame_buffer_at(model, ROOMY)[(column, row)].fg;
    let resting = ink(&mut model);
    assert_eq!(
        resting,
        crate::ui::theme::color(uze_theme::Token::TextSecondary),
        "held back at rest, like the bar's doors"
    );
    let item = model
        .remembered
        .declared
        .slots
        .iter()
        .find(|slot| slot.surface.id == "today")
        .expect("hosted");
    model.hovered = model.bar_target(item);
    assert!(model.hovered.is_some(), "the item opens the drawer");
    assert_eq!(
        ink(&mut model),
        crate::ui::theme::color(uze_theme::Token::TextBright),
        "the pointer shows it answers a click"
    );
}

#[test]
fn an_extension_not_enabled_here_has_no_door_in_the_bar() {
    let mut panel = surface("panel", uze_contract::Slot::Drawer);
    panel.title = Some("Timesheet".to_owned());
    let mut model = hosting(vec![compiled(
        "notes",
        vec![surface("board", uze_contract::Slot::View), panel],
    )]);
    for hosted in model.remembered.declared.extensions.values_mut() {
        hosted.per_project = true;
        hosted.record = Some(None);
    }
    assert!(model.bar_doors().is_empty(), "nothing it shows runs here");
    assert!(!model.has_bar(), "so the bar takes no row for it");
}

#[test]
fn without_a_plugin_in_it_the_bar_takes_no_row() {
    let mut model = hosting(vec![compiled(
        "notes",
        vec![surface("meter", uze_contract::Slot::Status)],
    )]);
    assert!(!model.has_bar());
    assert_eq!(layout(&model).bar, None);
    let with_one = hosting_a_bar(uze_contract::BarAlign::Left);
    assert_eq!(
        layout(&with_one).pane.height + 2,
        layout(&model).pane.height,
        "the bar's hairline and row"
    );
    let _ = frame_rows_at(&mut model, ROOMY);
}

/// A plugin that puts nothing in the bar.
fn no_bar() -> WorkspaceModel {
    hosting(vec![compiled(
        "notes",
        vec![surface("meter", uze_contract::Slot::Status)],
    )])
}

/// An installed release puts its restart at the bar's right end, ahead of
/// every plugin's item there, and stands the bar up for it when no plugin
/// does.
#[test]
fn the_restart_takes_the_bars_right_end_and_stands_the_bar_up() {
    let mut bare = no_bar();
    assert_eq!(layout(&bare).bar, None, "no release, no plugin, no bar");
    bare.release = Some(crate::self_update::Notice {
        waiting: Some("9.0.1".to_owned()),
        arrived: None,
    });
    let bar = bar_row(&mut bare);
    assert!(bar.trim_end().ends_with("restart"), "{bar}");

    let mut model = hosting_a_bar(uze_contract::BarAlign::Right);
    answers(&mut model, "notes", "6h12 today");
    model.release = Some(crate::self_update::Notice {
        waiting: Some("9.0.1".to_owned()),
        arrived: None,
    });
    let bar = bar_row(&mut model);
    let item = bar.find("6h12 today").expect("the item still stands");
    let restart = bar.find("restart").expect("the restart stands");
    assert!(restart > item, "the restart is first from the right: {bar}");
    assert!(bar.trim_end().ends_with("restart"), "{bar}");
}

/// The restart answers a click as its action answers the index: both
/// leave the loop to restart, and the runtime is only detached from.
#[test]
fn clicking_the_restart_is_choosing_it_in_the_index() {
    use crate::ui::orchestrator::{Flow, WorkspaceExit};

    let installed = || {
        let mut model = no_bar();
        model.release = Some(crate::self_update::Notice {
            waiting: Some("9.0.1".to_owned()),
            arrived: None,
        });
        model
    };
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("bar-restart"));

    let mut clicked = driven(installed(), &home);
    clicked.area = ROOMY;
    clicked.frame();
    let button = clicked
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::RestartIntoRelease).then_some(*rect))
        .expect("the bar offers the restart");
    assert!(matches!(
        clicked.press_answering(button.x, button.y),
        Flow::Exit(WorkspaceExit::Restart)
    ));

    let mut chosen = driven(installed(), &home);
    chosen.press_key(key_event(uze_keys::Chord::parse("f1").unwrap()));
    let index = chosen.attach.model.action_index.as_ref().expect("open");
    let rows = super::action_index_rows(
        &index.scopes,
        &index.offered,
        "",
        &chosen.attach.model.disabled_extensions,
    );
    let at = rows
        .iter()
        .position(|(action, _)| *action == uze_keys::Action::RestartUze)
        .unwrap_or_else(|| panic!("offered: {:?}", index.offered));
    chosen.attach.model.action_index.as_mut().unwrap().selected = at;
    assert!(matches!(
        chosen.press_key_answering(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
        Flow::Exit(WorkspaceExit::Restart)
    ));
}

/// With nothing installed to restart into, nothing offers it.
#[test]
fn without_an_installed_release_nothing_offers_a_restart() {
    let home = uze_application::UzeHome::at(uze_testkit::temp::scratch("bar-no-restart"));
    let mut client = driven(no_bar(), &home);
    client.press_key(key_event(uze_keys::Chord::parse("f1").unwrap()));
    let index = client.attach.model.action_index.as_ref().expect("open");
    assert!(!index.offered.contains(&uze_keys::Action::RestartUze));
}

/// The restart is text on the bar's own ground: green at rest, a stronger
/// green under the pointer, and never a fill of its own.
#[test]
fn the_restart_is_green_text_on_the_bars_ground() {
    let mut model = no_bar();
    model.release = Some(crate::self_update::Notice {
        waiting: Some("9.0.1".to_owned()),
        arrived: None,
    });
    let row = layout(&model).bar.unwrap().y + 1;
    let column = u16::try_from(
        bar_row(&mut model)
            .find("restart")
            .expect("the restart stands"),
    )
    .unwrap();
    let buffer = frame_buffer_at(&mut model, ROOMY);
    let cell = &buffer[(column, row)];
    assert_eq!(
        cell.fg,
        crate::ui::theme::color(uze_theme::Token::StateSuccess)
    );
    assert_eq!(
        cell.bg,
        buffer[(column - 4, row)].bg,
        "no fill of its own: the ground beside it"
    );

    model.hovered = Some(WorkspaceHit::RestartIntoRelease);
    assert_eq!(
        frame_buffer_at(&mut model, ROOMY)[(column, row)].fg,
        crate::ui::theme::lifted(uze_theme::Token::StateSuccess),
        "a stronger version of itself under the pointer"
    );
}

/// Two items on the bar stand two columns apart, counted from what each
/// draws: the room each asks for carries an inset that used to double the
/// air between them.
#[test]
fn items_on_the_bar_stand_two_columns_apart() {
    let mut model = hosting_a_bar(uze_contract::BarAlign::Right);
    answers(&mut model, "notes", "6h12 today");
    drafted(&mut model, "src/naming.rs");
    let bar = bar_row(&mut model);
    assert!(bar.contains("✎ 1  6h12 today"), "{bar:?}");
}
