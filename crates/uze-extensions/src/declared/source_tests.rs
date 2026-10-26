//! Sources asked through the host: one question for each source a surface
//! shows, asked again only when its checkout or input changed, and an
//! answer taken only while its question is the latest.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use serde_json::json;
use uze_contract::NodeName;

use super::tests::{SECTION, SOURCE, VIEW, compiled_view, extension, n, texts, tree};
use super::*;
use crate::view::NodeKind;

/// A host that answers every question with what it was given, and keeps
/// the questions.
struct Answering {
    answer: Answer,
    asked: RefCell<Vec<Question>>,
}

impl Answering {
    fn with(answer: Answer) -> Self {
        Self {
            answer,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl crate::Host for Answering {
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
        self.asked.borrow_mut().push(question);
        self.answer.clone()
    }
}

fn answered(data: serde_json::Value) -> Answer {
    Answer::Answered {
        envelope: uze_contract::Envelope::data(data),
        trace: Vec::new(),
    }
}

fn board() -> DeclaredExtension {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("{count}").item()],
        Vec::new(),
        Vec::new(),
    );
    DeclaredExtension::new(extension(view), "pt")
}

fn checkout(root: &str) -> Checkout {
    Checkout {
        root: PathBuf::from(root),
        display: root.to_owned(),
        target: None,
    }
}

/// Asks what the engine has to ask now, of `host`.
fn ask_all(engine: &mut DeclaredExtension, host: &Answering) {
    let jobs = engine.jobs();
    run(engine, jobs, host);
}

/// Runs `jobs` on `host` and hands each answer back, in order.
fn run(engine: &mut DeclaredExtension, jobs: Vec<Job>, host: &Answering) {
    for job in jobs {
        let answer = (job.run)(host);
        assert_eq!(engine.absorb(answer), Outcome::Stay);
    }
}

#[test]
fn nothing_is_asked_until_a_surface_shows_its_source() {
    let mut engine = board();
    assert!(engine.jobs().is_empty(), "no checkout in front, none open");
    engine.focus(Some(&checkout("/work/a")));
    let jobs = engine.jobs();
    assert_eq!(jobs.len(), 1, "the section shows the source");
    let host = Answering::with(answered(json!({ "count": 3 })));
    run(&mut engine, jobs, &host);
    assert_eq!(texts(&tree(&engine)), ["3"]);
    let asked = host.asked.borrow();
    assert_eq!(asked[0].identity, engine.compiled().identity);
    assert_eq!(asked[0].declaration, SOURCE);
    assert_eq!(asked[0].checkout, Some(PathBuf::from("/work/a")));
    assert!(engine.jobs().is_empty(), "asked once");
}

#[test]
fn a_view_and_a_section_showing_one_source_ask_it_once() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    engine.open(VIEW, &checkout("/work/a"));
    assert_eq!(engine.jobs().len(), 1);

    engine.open(VIEW, &checkout("/work/b"));
    let jobs = engine.jobs();
    assert_eq!(jobs.len(), 1, "the open view's checkout is asked");
    let host = Answering::with(answered(json!({ "count": 1 })));
    run(&mut engine, jobs, &host);
    assert_eq!(
        host.asked.borrow()[0].checkout,
        Some(PathBuf::from("/work/b"))
    );
    assert!(
        engine.jobs().is_empty(),
        "and the section does not take turns"
    );

    engine.close(VIEW);
    assert_eq!(engine.jobs().len(), 1, "the section's checkout once more");
}

#[test]
fn an_answer_to_a_question_no_longer_the_latest_is_dropped() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    let first = engine.jobs();
    engine.focus(Some(&checkout("/work/b")));
    let second = engine.jobs();
    let late = Answering::with(answered(json!({ "count": "a" })));
    let latest = Answering::with(answered(json!({ "count": "b" })));
    run(&mut engine, second, &latest);
    run(&mut engine, first, &late);
    assert_eq!(texts(&tree(&engine)), ["b"]);
}

#[test]
fn a_failed_or_stopped_script_is_said_on_the_surface() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    let host = Answering::with(Answer::Failed {
        reason: "board.rhai: the script crossed its deadline bound".to_owned(),
        trace: vec!["looking".to_owned()],
    });
    ask_all(&mut engine, &host);
    assert_eq!(
        tree(&engine).kind,
        NodeKind::Notice {
            text: "board.rhai: the script crossed its deadline bound".to_owned(),
            role: Role::Danger,
        }
    );

    engine.focus(Some(&checkout("/work/b")));
    let jobs = engine.jobs();
    let fallback = jobs.into_iter().map(|job| (job.fallback)()).next().unwrap();
    engine.absorb(fallback);
    assert!(matches!(
        tree(&engine).kind,
        NodeKind::Notice { text, role: Role::Danger } if text == STOPPED
    ));
}

#[test]
fn a_source_the_host_would_not_ask_says_why_until_it_answers() {
    let mut engine = board();
    engine.set_data(SOURCE, Ok(json!({ "count": 9 })));
    engine.focus(Some(&checkout("/work/a")));
    let host = Answering::with(Answer::withheld("needs enabling in this project"));
    ask_all(&mut engine, &host);
    assert_eq!(
        tree(&engine).kind,
        NodeKind::Notice {
            text: "needs enabling in this project".to_owned(),
            role: Role::Warning,
        }
    );
    let section = engine.section(SECTION, false).expect("a section");
    assert!(matches!(
        section.tree.expect("a tree").kind,
        NodeKind::Notice {
            role: Role::Warning,
            ..
        }
    ));
    assert_eq!(
        engine.trace(VIEW).expect("a trace").data,
        DataState::Withheld("needs enabling in this project".to_owned())
    );

    // Opening the view asks again: the operator may have enabled it since.
    engine.open(VIEW, &checkout("/work/a"));
    let host = Answering::with(answered(json!({ "count": 4 })));
    ask_all(&mut engine, &host);
    assert_eq!(texts(&tree(&engine)), ["4"]);
    assert_eq!(engine.withheld(SOURCE), None);
}

#[test]
fn a_newer_compile_asks_every_shown_source_again() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    let host = Answering::with(answered(json!({ "count": 1 })));
    ask_all(&mut engine, &host);
    let compiled = engine.compiled().clone();
    engine.replace(compiled);
    assert_eq!(engine.jobs().len(), 1);
}
