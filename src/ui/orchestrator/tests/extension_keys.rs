//! A declared extension's actions on the keyboard: its table registered
//! while it is enabled, its keys in the footer and the index, and a key
//! reaching the action through the dispatch every key takes.

use super::support::*;
use super::{ExtensionAnswer, Hosted, HostedExtensions, LoadedExtension, WorkspaceHit};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension, UzeHome};
use uze_extensions::contract::{Extension, Outcome, SurfaceInfo};
use uze_extensions::view::{
    Command, Content, Layout, Node, NodeKind, Role, ScrollDirection, ScrollTarget, Size, Span,
    View, ViewHit,
};
use uze_keys::{Action, Chord};

/// An engine that draws one line and answers a declared action by saying
/// which one reached it.
struct Keyed;

impl Extension for Keyed {
    fn identity(&self) -> String {
        "keyed".to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        Vec::new()
    }

    fn view(&self, _surface: &str, _space: Size) -> Option<View> {
        Some(View {
            tabs: Vec::new(),
            title: vec![Span::new("Keyed", Role::Muted)],
            caption: Vec::new(),
            navigator: None,
            content: Content::Tree {
                heading: "Keyed".to_owned(),
                scroll: 0,
                root: Node::new(
                    0,
                    NodeKind::Text {
                        spans: vec![Span::new("a board", Role::Default)],
                        truncate: true,
                        still: false,
                    },
                ),
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

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<uze_extensions::view::Section> {
        None
    }

    fn command(&mut self, _surface: &str, command: Command, _space: Size) -> Outcome {
        match command {
            Command::Close => Outcome::Close,
            _ => Outcome::Stay,
        }
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
}

impl Hosted for Keyed {
    fn press(&mut self, surface: &str, action: &str) -> Outcome {
        Outcome::Notice(Span::new(format!("{action} on {surface}"), Role::Muted))
    }
}

fn keyed(_compiled: CompiledExtension, _locale: &str) -> Box<dyn Hosted> {
    Box::new(Keyed)
}

/// Each test hosts the extension under a plugin of its own: the keyboard
/// is process-wide, and one test switching its extension off must not
/// take another's keys.
fn identity(plugin: &str) -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "market".to_owned(),
        plugin: plugin.to_owned(),
        id: "keyed-band".to_owned(),
    }
}

fn owner(plugin: &str) -> String {
    format!("{plugin}@market/keyed-band")
}

fn action(id: &str, key: Option<&str>) -> uze_contract::ActionDecl {
    uze_contract::ActionDecl {
        id: id.to_owned(),
        label: id[..1].to_uppercase() + &id[1..],
        on: "board".to_owned(),
        key: key.map(|key| uze_contract::KeySuggestion::parse(key).expect(key)),
        confirm: None,
        does: uze_contract::Does::Refresh(vec!["board".to_owned()]),
        with: Vec::new(),
        then: None,
    }
}

fn compiled(plugin: &str) -> CompiledExtension {
    CompiledExtension {
        identity: identity(plugin),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "keyed-band".to_owned(),
            name: "Keyed".to_owned(),
            description: String::new(),
            surfaces: vec![uze_contract::SurfaceDecl {
                band: None,
                bar: None,
                drawer: None,
                id: "board".to_owned(),
                slot: uze_contract::Slot::View,
                view: "board.uze".to_owned(),
                source: "board".to_owned(),
                title: Some("Keyed".to_owned()),
                applies_when: None,
            }],
            wants: Vec::new(),
            sources: vec![uze_contract::SourceDecl::new(
                "board",
                uze_contract::SourceKind::Fixture("fixtures/board.json".to_owned()),
            )],
            actions: vec![
                action("reload", Some("ctrl-r")),
                // The host's: offered without a key.
                action("quit", Some("ctrl-q")),
                action("archive", None),
            ],
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
            icons: Vec::new(),
            mcp: Vec::new(),
        },
        views: BTreeMap::new(),
    }
}

fn installed(plugin: &str) -> InstalledExtension {
    InstalledExtension {
        identity: identity(plugin),
        dir: PathBuf::from("/market/plugins/keys/extensions/keyed-band"),
        linked: false,
    }
}

/// The keyed extension hosted, and its view open in front of an agent.
fn keyed_view_open<'a>(home: &'a UzeHome, plugin: &str) -> Driven<'a> {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared = HostedExtensions {
        engine: keyed,
        discovered_at: Some(std::time::Instant::now()),
        ..HostedExtensions::default()
    };
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed(plugin)]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![LoadedExtension {
            installed: installed(plugin),
            stamp: 1,
            compiled: Ok(compiled(plugin)),
            data: BTreeMap::new(),
        }],
    }));
    let slot = model
        .declared_views()
        .next()
        .expect("the view is offered")
        .id;
    super::open_surface(&mut model, slot);
    let mut driven = driven(model, home).on_a_roomy_terminal();
    driven.frame();
    driven
}

fn named(plugin: &str, action: &str) -> Action {
    Action::parse(&uze_keys::extension::action_name(&owner(plugin), action))
        .expect("an extension action")
}

fn noticed(driven: &Driven<'_>) -> Option<String> {
    driven
        .attach
        .model
        .open_notice
        .as_ref()
        .map(|span| span.text.clone())
}

#[test]
fn the_footer_names_a_declared_action_by_the_key_it_suggests_and_a_click_presses_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("extension-keys-footer"));
    let mut driven = keyed_view_open(&home, "keys-footer");
    let rows = frame_rows_at(&mut driven.attach.model, driven.area);
    assert!(
        rows.iter().any(|row| row.contains("ctrl+r reload")),
        "{rows:#?}"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("quit") || row.contains("archive")),
        "one with no key is not in the footer: {rows:#?}"
    );
    let hint = driven.hit(|hit| *hit == WorkspaceHit::HintAction(named("keys-footer", "reload")));
    driven.press(hint.x, hint.y);
    assert_eq!(noticed(&driven).as_deref(), Some("reload on board"));
}

#[test]
fn a_suggested_key_reaches_the_action_only_while_its_surface_is_in_front() {
    let home = UzeHome::at(uze_testkit::temp::scratch("extension-keys-press"));
    let mut driven = keyed_view_open(&home, "keys-press");
    let reload = Chord::parse("ctrl+r").unwrap();
    driven.press_key(key_event(reload));
    assert_eq!(noticed(&driven).as_deref(), Some("reload on board"));

    // The host keeps what it holds: ctrl+q is still quit's, and the
    // action that suggested it has no key.
    assert_eq!(
        uze_keys::active().chord_for(named("keys-press", "quit"), &[uze_keys::Scope::Declared]),
        None
    );

    driven.attach.model.close_surface();
    driven.attach.model.open_notice = None;
    driven.frame();
    driven.press_key(key_event(reload));
    assert_eq!(
        noticed(&driven),
        None,
        "with the view put away it is the pane's"
    );
}

#[test]
fn the_index_offers_every_declared_action_with_its_key_or_by_pointer() {
    let home = UzeHome::at(uze_testkit::temp::scratch("extension-keys-index"));
    let mut driven = keyed_view_open(&home, "keys-index");
    driven.press_key(key_event(Chord::parse("f1").unwrap()));
    let index = driven
        .attach
        .model
        .action_index
        .as_ref()
        .expect("the index is open");
    let rows = super::action_index_rows(
        &index.scopes,
        &index.offered,
        "",
        &driven.attach.model.disabled_extensions,
    );
    for (action, chord) in [
        ("reload", Chord::parse("ctrl+r").ok()),
        ("quit", None),
        ("archive", None),
    ] {
        assert!(
            rows.contains(&(named("keys-index", action), chord)),
            "{action} with {chord:?}: {rows:?}"
        );
    }
}

#[test]
fn switching_the_extension_off_takes_its_keys_away() {
    let home = UzeHome::at(uze_testkit::temp::scratch("extension-keys-off"));
    let mut driven = keyed_view_open(&home, "keys-off");
    let owner = owner("keys-off");
    let front = uze_keys::Front {
        owner: &owner,
        surface: "board",
    };
    assert_eq!(uze_keys::active().extension_actions(front).len(), 3);

    let switch = crate::ui::extension_switch::declared_id(&identity("keys-off"));
    driven
        .attach
        .model
        .follow_extension_switch(BTreeSet::from([switch]));
    assert!(uze_keys::active().extension_actions(front).is_empty());

    driven.attach.model.follow_extension_switch(BTreeSet::new());
    assert_eq!(
        uze_keys::active().extension_actions(front).len(),
        3,
        "and back when it is switched on"
    );
}
