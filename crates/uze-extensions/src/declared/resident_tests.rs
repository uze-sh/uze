//! A resident source: its session opened and closed with the surfaces in
//! view, its renders asked only when owed and within the frame and the
//! rate, its raw input queued, its failure said on its surface.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uze_contract::{NodeName, ResidentCall, SourceDecl, SourceKind};

use super::source_tests::checkout;
use super::tests::{SOURCE, SPACE, VIEW, compiled_view, extension, n, texts, tree};
use super::*;
use crate::contract::RawInput;
use crate::view::NodeKind;

/// A host whose resident session counts the renders it was asked for,
/// and answers every other call with `redraw`, a tick asked with `timer`.
struct Session {
    calls: RefCell<Vec<(ResidentCall, Value)>>,
    redraw: bool,
    timer: Option<u64>,
    render: RefCell<Answer>,
}

impl Session {
    fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            redraw: true,
            timer: None,
            render: RefCell::new(rendered("frame")),
        }
    }

    fn calls(&self) -> Vec<ResidentCall> {
        self.calls.borrow().iter().map(|(call, _)| *call).collect()
    }
}

fn rendered(text: &str) -> Answer {
    Answer::Answered {
        envelope: uze_contract::Envelope::data(json!({"type": "Text", "args": [text]})),
        trace: Vec::new(),
    }
}

impl crate::Host for Session {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, _: &Path) -> Result<PathBuf, String> {
        Err("no git here".to_owned())
    }
    fn read_file(&self, _: &Path) -> Result<String, crate::Unreadable> {
        Err(crate::Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<crate::DirEntry>, String> {
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
        let Asking::Resident(call) = question.asking else {
            panic!("a resident source is asked only through its session");
        };
        assert_eq!(question.declaration, SOURCE);
        self.calls.borrow_mut().push((call, question.input));
        match call {
            ResidentCall::Render => self.render.borrow().clone(),
            _ => Answer::Answered {
                envelope: uze_contract::Envelope {
                    timer: self.timer,
                    ..uze_contract::Envelope::data(Value::Bool(self.redraw))
                },
                trace: Vec::new(),
            },
        }
    }
}

/// A game: a view and a section showing one resident source.
fn game() -> DeclaredExtension {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("waiting").item()],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.wants = vec![uze_contract::Grant::Resident];
    compiled.manifest.sources = vec![SourceDecl::new(
        SOURCE,
        SourceKind::Resident("game.rhai".to_owned()),
    )];
    DeclaredExtension::new(compiled, "en")
}

fn pass(engine: &mut DeclaredExtension, now: Instant, visible: &[&str]) {
    let visible: Vec<String> = visible.iter().map(|id| (*id).to_owned()).collect();
    engine.pass(&Pass {
        now,
        visible: &visible,
        happened: &[],
    });
}

/// Runs what the engine asks now, answering each from `host`.
fn run(engine: &mut DeclaredExtension, host: &Session) -> usize {
    let jobs = engine.jobs();
    let count = jobs.len();
    for job in jobs {
        let answer = (job.run)(host);
        engine.absorb(answer);
    }
    count
}

/// A frame drawn.
fn draw(engine: &DeclaredExtension) {
    engine.view(VIEW, SPACE);
}

#[test]
fn a_session_opens_with_its_surface_in_view_and_closes_with_it() {
    let mut engine = game();
    let host = Session::new();
    let start = Instant::now();
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[]);
    assert_eq!(run(&mut engine, &host), 0, "nothing in view, nothing runs");

    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    assert_eq!(host.calls(), [ResidentCall::Start]);
    assert_eq!(engine.resident_sessions(), [SOURCE]);
    assert_eq!(run(&mut engine, &host), 0, "no frame drawn yet");
    draw(&engine);
    run(&mut engine, &host);
    assert_eq!(
        host.calls(),
        [ResidentCall::Start, ResidentCall::Render],
        "a start answering a redraw is rendered"
    );
    assert_eq!(texts(&tree(&engine)), ["frame"]);
    let size = host.calls.borrow()[1].1.clone();
    assert_eq!(size, json!({"width": SPACE.width, "height": SPACE.height}));

    engine.close(VIEW);
    pass(&mut engine, start, &[]);
    run(&mut engine, &host);
    assert_eq!(host.calls().last(), Some(&ResidentCall::Stop));
    assert!(engine.resident_sessions().is_empty());

    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    assert_eq!(
        host.calls().last(),
        Some(&ResidentCall::Start),
        "in view again, started again"
    );
}

#[test]
fn renders_wait_for_a_drawn_frame_and_the_rate() {
    let mut engine = game();
    let host = Session::new();
    let start = Instant::now();
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    draw(&engine);
    run(&mut engine, &host);
    let renders = |host: &Session| {
        host.calls()
            .iter()
            .filter(|call| **call == ResidentCall::Render)
            .count()
    };
    assert_eq!(renders(&host), 1);

    // A key answers a redraw; no frame was drawn since the last render.
    engine.raw_input(
        VIEW,
        RawInput::Key {
            key: "up".to_owned(),
            mods: Vec::new(),
        },
    );
    let later = start + Duration::from_millis(100);
    pass(&mut engine, later, &[VIEW]);
    run(&mut engine, &host);
    run(&mut engine, &host);
    assert_eq!(renders(&host), 1, "not before a frame is drawn");

    draw(&engine);
    run(&mut engine, &host);
    assert_eq!(renders(&host), 2, "once the frame is drawn");

    // Owed again at once: the rate holds it back.
    engine.raw_input(
        VIEW,
        RawInput::Key {
            key: "up".to_owned(),
            mods: Vec::new(),
        },
    );
    run(&mut engine, &host);
    draw(&engine);
    pass(&mut engine, later + Duration::from_millis(10), &[VIEW]);
    run(&mut engine, &host);
    assert_eq!(renders(&host), 2, "not sooner than a thirtieth of a second");
    pass(&mut engine, later + Duration::from_millis(40), &[VIEW]);
    run(&mut engine, &host);
    assert_eq!(renders(&host), 3);

    // Nothing owed, nothing asked, however many frames.
    draw(&engine);
    pass(&mut engine, later + Duration::from_secs(1), &[VIEW]);
    assert_eq!(run(&mut engine, &host), 0);
}

#[test]
fn a_tick_asked_for_arrives_as_an_event_when_due() {
    let mut engine = game();
    let host = Session {
        timer: Some(100),
        redraw: false,
        ..Session::new()
    };
    let start = Instant::now();
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    pass(&mut engine, start + Duration::from_millis(50), &[VIEW]);
    assert_eq!(run(&mut engine, &host), 0, "not yet due");
    pass(&mut engine, start + Duration::from_millis(100), &[VIEW]);
    run(&mut engine, &host);
    let calls = host.calls.borrow();
    assert_eq!(
        *calls.last().unwrap(),
        (ResidentCall::Event, json!({"name": "timer"}))
    );
}

#[test]
fn raw_input_reaches_the_session_as_the_keymap_spells_it() {
    let mut engine = game();
    let host = Session::new();
    assert!(engine.reads_raw_input(VIEW));
    assert!(
        !engine.reads_raw_input(super::tests::SECTION),
        "a section is never focused"
    );
    let start = Instant::now();
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    engine.raw_input(
        VIEW,
        RawInput::Key {
            key: "a".to_owned(),
            mods: vec!["ctrl".to_owned()],
        },
    );
    engine.raw_input(
        VIEW,
        RawInput::Pointer {
            x: 3,
            y: 1,
            button: "left".to_owned(),
        },
    );
    while run(&mut engine, &host) > 0 {
        draw(&engine);
    }
    let inputs: Vec<Value> = host
        .calls
        .borrow()
        .iter()
        .filter(|(call, _)| *call == ResidentCall::Input)
        .map(|(_, input)| input.clone())
        .collect();
    assert_eq!(
        inputs,
        [
            json!({"key": "a", "mods": ["ctrl"]}),
            json!({"x": 3, "y": 1, "button": "left"})
        ]
    );
}

#[test]
fn a_breach_stops_the_session_and_the_surface_names_the_bound() {
    let mut engine = game();
    let host = Session::new();
    let start = Instant::now();
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    draw(&engine);
    run(&mut engine, &host);
    assert_eq!(texts(&tree(&engine)), ["frame"]);

    *host.render.borrow_mut() = Answer::Failed {
        reason: "game.rhai: the script crossed its operations bound at 3:5".to_owned(),
        trace: Vec::new(),
    };
    engine.raw_input(
        VIEW,
        RawInput::Key {
            key: "up".to_owned(),
            mods: Vec::new(),
        },
    );
    draw(&engine);
    pass(&mut engine, start + Duration::from_secs(1), &[VIEW]);
    run(&mut engine, &host);
    draw(&engine);
    run(&mut engine, &host);
    assert!(matches!(
        tree(&engine).kind,
        NodeKind::Notice { text, .. } if text.contains("operations bound")
    ));
    assert!(engine.resident_sessions().is_empty(), "stopped");
    engine.raw_input(
        VIEW,
        RawInput::Key {
            key: "up".to_owned(),
            mods: Vec::new(),
        },
    );
    pass(&mut engine, start + Duration::from_secs(2), &[VIEW]);
    assert_eq!(
        run(&mut engine, &host),
        0,
        "a stopped session is asked nothing"
    );
}

#[test]
fn a_render_not_yet_answered_keeps_the_last_frame() {
    let mut engine = game();
    let host = Session::new();
    let start = Instant::now();
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, start, &[VIEW]);
    run(&mut engine, &host);
    draw(&engine);
    run(&mut engine, &host);
    engine.raw_input(
        VIEW,
        RawInput::Key {
            key: "up".to_owned(),
            mods: Vec::new(),
        },
    );
    run(&mut engine, &host);
    draw(&engine);
    pass(&mut engine, start + Duration::from_secs(1), &[VIEW]);
    let late = engine.jobs();
    assert_eq!(late.len(), 1, "the render is out");
    assert_eq!(
        texts(&tree(&engine)),
        ["frame"],
        "and the last frame stands"
    );
    *host.render.borrow_mut() = rendered("next");
    for job in late {
        let answer = (job.run)(&host);
        engine.absorb(answer);
    }
    assert_eq!(texts(&tree(&engine)), ["next"]);
}

#[test]
fn a_tree_the_catalog_refuses_is_said_and_not_drawn() {
    let mut engine = game();
    let host = Session::new();
    *host.render.borrow_mut() = Answer::Answered {
        envelope: uze_contract::Envelope::data(json!({"type": "iframe"})),
        trace: Vec::new(),
    };
    engine.open(VIEW, &checkout("/work/a"));
    pass(&mut engine, Instant::now(), &[VIEW]);
    run(&mut engine, &host);
    draw(&engine);
    run(&mut engine, &host);
    assert!(matches!(
        tree(&engine).kind,
        NodeKind::Notice { text, .. } if text.contains("iframe")
    ));
}
