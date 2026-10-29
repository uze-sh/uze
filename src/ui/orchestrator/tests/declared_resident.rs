//! A resident view focused in the pane: it reads keys raw, through the
//! dispatch every key takes, until Esc lets go of it; F1 stays the host's.

use super::support::*;
use super::{
    EngineAnswer, ExtensionAnswer, HostedExtensions, LoadedExtension, Owner, WorkspaceHit,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension, UzeHome};
use uze_contract::ResidentCall;
use uze_extensions::{Answer, Asking, DirEntry, Question, Unreadable};
use uze_keys::Chord;

const ROOT: &str = "/repo";

fn identity() -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "arcade".to_owned(),
        plugin: "games".to_owned(),
        id: "snake".to_owned(),
    }
}

/// A view showing a resident source.
fn compiled() -> CompiledExtension {
    compiled_as(uze_contract::Slot::View, true)
}

/// The game in `slot`, its source resident or a plain fixture.
fn compiled_as(slot: uze_contract::Slot, resident: bool) -> CompiledExtension {
    let view = uze_contract::CompiledView {
        file: "game.uze".to_owned(),
        items: Vec::new(),
        templates: BTreeMap::new(),
        lookups: Vec::new(),
    };
    CompiledExtension {
        identity: identity(),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "snake".to_owned(),
            name: "Snake".to_owned(),
            description: String::new(),
            surfaces: vec![uze_contract::SurfaceDecl {
                band: None,
                bar: None,
                drawer: None,
                id: "game".to_owned(),
                slot,
                view: "game.uze".to_owned(),
                source: "board".to_owned(),
                title: None,
                applies_when: None,
            }],
            wants: vec![uze_contract::Grant::Resident],
            sources: vec![uze_contract::SourceDecl::new(
                "board",
                if resident {
                    uze_contract::SourceKind::Resident("game.uze".to_owned())
                } else {
                    uze_contract::SourceKind::Fixture("board.json".to_owned())
                },
            )],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
            icons: Vec::new(),
            mcp: Vec::new(),
        },
        views: BTreeMap::from([("game.uze".to_owned(), view)]),
    }
}

/// A host whose resident session takes every call and asks no redraw,
/// keeping what it was called with.
#[derive(Default)]
struct Session(RefCell<Vec<(ResidentCall, serde_json::Value)>>);

impl uze_extensions::Host for Session {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, _: &Path) -> Result<PathBuf, String> {
        Err("no git here".to_owned())
    }
    fn read_file(&self, _: &Path) -> Result<String, Unreadable> {
        Err(Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<DirEntry>, String> {
        Ok(Vec::new())
    }
    fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn syntax_theme(&self) -> String {
        String::new()
    }
    fn answer(&self, question: Question) -> Answer {
        if let Asking::Resident(call) = question.asking {
            self.0.borrow_mut().push((call, question.input));
        }
        Answer::Answered {
            envelope: uze_contract::Envelope::data(serde_json::Value::Bool(false)),
            trace: Vec::new(),
        }
    }
}

/// The resident view open in the pane, drawn.
fn playing(home: &UzeHome) -> Driven<'_> {
    hosting(home, compiled())
}

/// The game as an overlay over the pane, beside the agent `a1`, drawn.
fn overlaid(home: &UzeHome, resident: bool) -> Driven<'_> {
    let mut driven = hosting(home, compiled_as(uze_contract::Slot::Overlay, resident));
    let model = &mut driven.attach.model;
    launch_agent_a1(model);
    let slot = model
        .remembered
        .declared
        .slots
        .iter()
        .find(|slot| slot.surface.id == "game")
        .expect("hosted")
        .id;
    model.open_overlay(slot);
    assert!(model.overlay_slot().is_some(), "the overlay stands");
    driven.frame();
    driven
}

/// `compiled` hosted, its view open in the pane when it is one.
fn hosting(home: &UzeHome, compiled: CompiledExtension) -> Driven<'_> {
    let mut model = agent_session_in(ROOT);
    model.remembered.declared = HostedExtensions {
        discovered_at: Some(Instant::now()),
        ..HostedExtensions::default()
    };
    let installed = InstalledExtension {
        identity: identity(),
        dir: PathBuf::from("/arcade/plugins/games/extensions/snake"),
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
            data: BTreeMap::new(),
        }],
    }));
    let view = model.declared_views().next().map(|slot| slot.id);
    if let Some(slot) = view {
        super::open_surface(&mut model, slot);
    }
    let mut driven = driven(model, home).on_a_roomy_terminal();
    driven.frame();
    driven
}

/// One pass of the engines, its jobs answered by `host` the way the frame
/// loop takes an answer.
fn run(driven: &mut Driven<'_>, host: &Session) {
    let model = &mut driven.attach.model;
    model.pass_engines(Instant::now());
    let owner = Owner::Declared(identity());
    let jobs = model
        .remembered
        .declared
        .engines
        .get_mut(&owner)
        .expect("hosted")
        .jobs();
    for job in jobs {
        let answer = (job.run)(host);
        model.absorb_engine_answer(EngineAnswer {
            extension: owner.clone(),
            answer,
        });
    }
}

fn inputs(host: &Session) -> Vec<serde_json::Value> {
    host.0
        .borrow()
        .iter()
        .filter(|(call, _)| *call == ResidentCall::Input)
        .map(|(_, input)| input.clone())
        .collect()
}

fn key(text: &str) -> crossterm::event::KeyEvent {
    key_event(Chord::parse(text).unwrap())
}

#[test]
fn a_focused_resident_view_reads_up_and_esc_lets_go_of_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("resident-keys"));
    let mut driven = playing(&home);
    let host = Session::default();
    run(&mut driven, &host);
    assert_eq!(
        host.0.borrow().first().map(|(call, _)| *call),
        Some(ResidentCall::Start),
        "in view, its session starts"
    );

    driven.press_key(key("up"));
    driven.press_key(key("ctrl+a"));
    run(&mut driven, &host);
    run(&mut driven, &host);
    assert_eq!(
        inputs(&host),
        [
            serde_json::json!({ "key": "up", "mods": [] }),
            serde_json::json!({ "key": "a", "mods": ["ctrl"] }),
        ]
    );

    driven.press_key(key("f1"));
    assert!(
        driven.attach.model.action_index.is_some(),
        "F1 stays the host's"
    );
    driven.press_key(key("esc"));
    assert!(driven.attach.model.action_index.is_none());
    driven.press_key(key("f8"));
    assert!(
        driven.attach.model.remembered.declared.bridge.writes.paused,
        "F8 pauses the extensions"
    );
    driven.press_key(key("f8"));
    assert!(
        !driven.attach.model.remembered.declared.bridge.writes.paused,
        "and F8 again lets them go"
    );
    run(&mut driven, &host);
    assert_eq!(inputs(&host).len(), 2, "F8 stays the host's too");

    driven.press_key(key("esc"));
    assert!(
        driven.attach.model.raw_input_slot().is_none(),
        "Esc lets go of it"
    );
    assert!(
        driven.attach.model.open_slot.is_some(),
        "and leaves it open"
    );
    driven.press_key(key("up"));
    run(&mut driven, &host);
    assert_eq!(inputs(&host).len(), 2, "its keys are the host's again");

    driven.press_key(key("esc"));
    assert!(
        driven.attach.model.open_slot.is_none(),
        "the next Esc puts it away"
    );
}

#[test]
fn a_press_on_a_resident_view_is_its_relative_to_it_and_takes_it_back() {
    let home = UzeHome::at(uze_testkit::temp::scratch("resident-pointer"));
    let mut driven = playing(&home);
    let host = Session::default();
    run(&mut driven, &host);
    driven.press_key(key("esc"));
    assert!(driven.attach.model.raw_input_slot().is_none());

    let area = driven.attach.model.surface_frame.content_area;
    assert!(area.width > 4 && area.height > 2, "{area:?}");
    driven.press(area.x + 3, area.y + 2);
    run(&mut driven, &host);
    assert_eq!(
        inputs(&host),
        [serde_json::json!({ "x": 3, "y": 2, "button": "left" })]
    );
    assert!(
        driven.attach.model.raw_input_slot().is_some(),
        "a press takes it back"
    );
}

/// The view open, its agent `a1` launched in the pane behind it.
fn playing_beside_an_agent(home: &UzeHome) -> Driven<'_> {
    let mut driven = playing(home);
    launch_agent_a1(&mut driven.attach.model);
    driven.frame();
    driven
}

fn prompt(text: &str) -> uze_extensions::contract::AgentRequest {
    uze_extensions::contract::AgentRequest {
        what: uze_extensions::contract::AgentAsk::Prompt {
            text: text.to_owned(),
        },
        agent: None,
        action: "review".to_owned(),
        label: "Review".to_owned(),
        unattended: false,
    }
}

fn typed(sent: &[uze_terminal::ClientRequest]) -> Vec<String> {
    sent.iter()
        .filter_map(|request| match request {
            uze_terminal::ClientRequest::Input { bytes, .. } => {
                Some(String::from_utf8_lossy(bytes).into_owned())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_question_is_answered_by_keyboard_over_a_declared_view() {
    let home = UzeHome::at(uze_testkit::temp::scratch("resident-agent-question"));
    let mut driven = playing_beside_an_agent(&home);
    let host = Session::default();
    run(&mut driven, &host);
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review PLAN.md"));
    driven.frame();
    driven.press_key(key("right"));
    driven.press_key(key("enter"));
    assert!(
        driven.attach.model.agent_write.is_none(),
        "Enter answers it"
    );
    driven.pump();
    let sent = typed(&driven.sent());
    assert_eq!(sent.len(), 1, "{sent:?}");
    run(&mut driven, &host);
    assert!(inputs(&host).is_empty(), "the view behind heard nothing");
}

#[test]
fn a_question_over_a_resident_overlay_takes_its_keys_and_clicks() {
    let home = UzeHome::at(uze_testkit::temp::scratch("resident-overlay-question"));
    let mut driven = overlaid(&home, true);
    let host = Session::default();
    run(&mut driven, &host);
    assert!(
        driven.attach.model.raw_input_slot().is_some(),
        "the overlay reads raw keys"
    );

    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review PLAN.md"));
    driven.frame();
    driven.press_key(key("right"));
    driven.press_key(key("enter"));
    assert!(
        driven.attach.model.agent_write.is_none(),
        "Enter answers it"
    );
    driven.pump();
    assert_eq!(typed(&driven.sent()).len(), 1);

    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("And then"));
    driven.frame();
    let cancel = driven.hit(|hit| *hit == WorkspaceHit::AgentWriteAnswer(false));
    let overlay = driven.attach.model.overlay_area.get();
    assert!(
        overlay.contains(ratatui::layout::Position::new(cancel.x, cancel.y)),
        "the answer stands over the overlay: {cancel:?} in {overlay:?}"
    );
    driven.press(cancel.x, cancel.y);
    assert!(
        driven.attach.model.agent_write.is_none(),
        "the click answers it"
    );
    run(&mut driven, &host);
    run(&mut driven, &host);
    assert!(
        inputs(&host).is_empty(),
        "nothing meant for the question reached the overlay: {:?}",
        inputs(&host)
    );
}

/// F8 over the overlay in front pauses every write: a prompt asked then
/// is refused before it is shown, and nothing reaches the agent.
fn f8_pauses_over(driven: &mut Driven<'_>, host: &Session) {
    driven.press_key(key("f8"));
    assert!(
        driven.attach.model.remembered.declared.bridge.writes.paused,
        "F8 pauses over the overlay"
    );
    assert!(
        driven.attach.model.overlay_slot().is_some(),
        "and leaves it standing"
    );
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review PLAN.md"));
    assert!(driven.attach.model.agent_write.is_none(), "not even asked");
    driven.pump();
    assert!(typed(&driven.sent()).is_empty());
    run(driven, host);
    assert!(inputs(host).is_empty(), "F8 never reaches the overlay");
    driven.press_key(key("f8"));
    assert!(!driven.attach.model.remembered.declared.bridge.writes.paused);
}

#[test]
fn f8_pauses_over_a_plain_overlay() {
    let home = UzeHome::at(uze_testkit::temp::scratch("overlay-pause"));
    let mut driven = overlaid(&home, false);
    let host = Session::default();
    f8_pauses_over(&mut driven, &host);
}

#[test]
fn f8_pauses_over_a_focused_resident_overlay() {
    let home = UzeHome::at(uze_testkit::temp::scratch("resident-overlay-pause"));
    let mut driven = overlaid(&home, true);
    let host = Session::default();
    run(&mut driven, &host);
    assert!(driven.attach.model.raw_input_slot().is_some());
    f8_pauses_over(&mut driven, &host);
}
