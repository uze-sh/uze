//! A resident view focused in the pane: it reads keys raw, through the
//! dispatch every key takes, until Esc lets go of it; F1 stays the host's.

use super::support::*;
use super::{EngineAnswer, ExtensionAnswer, HostedExtensions, LoadedExtension, Owner};
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
    let view = uze_contract::CompiledView {
        file: "game.kdl".to_owned(),
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
                id: "game".to_owned(),
                slot: uze_contract::Slot::View,
                view: "game.kdl".to_owned(),
                source: "board".to_owned(),
                title: None,
            }],
            wants: vec![uze_contract::Grant::Resident],
            sources: vec![uze_contract::SourceDecl::new(
                "board",
                uze_contract::SourceKind::Resident("game.rhai".to_owned()),
            )],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
        },
        views: BTreeMap::from([("game.kdl".to_owned(), view)]),
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
            compiled: Ok(compiled()),
            data: BTreeMap::new(),
        }],
    }));
    let slot = model.declared_views().next().expect("the view").id;
    super::open_surface(&mut model, slot);
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
    driven.press_key(key("f8"));
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
