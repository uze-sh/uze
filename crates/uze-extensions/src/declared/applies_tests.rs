//! `applies-when`: a surface that is for some checkouts only is neither
//! offered nor drawn elsewhere, and a source that is not is never asked.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use uze_contract::NodeName;

use super::source_tests::{Answering, answered, checkout};
use super::tests::{SECTION, SOURCE, VIEW, compiled_view, extension, n};
use super::*;

/// A host whose checkouts hold exactly `paths`, each as `root/relative`,
/// answering every source with `{ "count": 1 }`.
struct Marked {
    answering: Answering,
    paths: RefCell<BTreeSet<PathBuf>>,
    listed: RefCell<Vec<PathBuf>>,
}

impl Marked {
    fn with(paths: &[&str]) -> Self {
        Self {
            answering: Answering::with(answered(json!({ "count": 1 }))),
            paths: RefCell::new(paths.iter().map(PathBuf::from).collect()),
            listed: RefCell::new(Vec::new()),
        }
    }
}

impl crate::Host for Marked {
    fn git(&self, root: &Path, args: &[&str], answers: &[i32]) -> Result<String, String> {
        self.answering.git(root, args, answers)
    }
    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        self.answering.repository_root(path)
    }
    fn read_file(&self, path: &Path) -> Result<String, crate::Unreadable> {
        self.answering.read_file(path)
    }
    fn list_dir(&self, path: &Path) -> Result<Vec<crate::DirEntry>, String> {
        self.listed.borrow_mut().push(path.to_path_buf());
        Ok(self
            .paths
            .borrow()
            .iter()
            .filter(|held| held.parent() == Some(path))
            .map(|held| crate::DirEntry {
                directory: false,
                name: held.file_name().unwrap().to_string_lossy().into_owned(),
            })
            .collect())
    }
    fn write_file(&self, root: &Path, path: &Path, contents: &str) -> Result<(), String> {
        self.answering.write_file(root, path, contents)
    }
    fn delete_file(&self, root: &Path, path: &Path) -> Result<(), String> {
        self.answering.delete_file(root, path)
    }
    fn restore_to_head(&self, root: &Path, paths: &[PathBuf]) -> Result<(), String> {
        self.answering.restore_to_head(root, paths)
    }
    fn syntax_theme(&self) -> String {
        String::new()
    }
    fn answer(&self, question: Question) -> Answer {
        self.answering.answer(question)
    }
}

/// The board's section for `.planning` checkouts only, its source for
/// those holding `.planning/STATE.md`.
fn marked(surface: Option<&str>, source: Option<&str>) -> DeclaredExtension {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("{count}").item()],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    for decl in &mut compiled.manifest.surfaces {
        decl.applies_when = surface.map(str::to_owned);
    }
    compiled.manifest.sources[0].applies_when = source.map(str::to_owned);
    DeclaredExtension::new(compiled, "en")
}

fn ask(engine: &mut DeclaredExtension, host: &Marked) -> usize {
    let jobs = engine.jobs();
    let count = jobs.len();
    for job in jobs {
        let answer = (job.run)(host);
        engine.absorb(answer);
    }
    count
}

fn questions(host: &Marked) -> Vec<String> {
    host.answering
        .asked
        .borrow()
        .iter()
        .map(|question| question.declaration.clone())
        .collect()
}

#[test]
fn a_surface_applies_only_once_its_checkout_is_known_to_hold_the_path() {
    let mut engine = marked(Some(".planning"), None);
    engine.focus(Some(&checkout("/work/a")));
    assert!(!engine.applies(SECTION), "unknown is not applying");
    assert!(engine.section(SECTION, false).is_none());

    let host = Marked::with(&["/work/a/.planning"]);
    ask(&mut engine, &host);
    assert!(engine.applies(SECTION));
    assert!(engine.section(SECTION, false).is_some());

    // Another checkout in front, without it: the section goes.
    engine.focus(Some(&checkout("/work/b")));
    ask(&mut engine, &host);
    assert!(!engine.applies(SECTION));
    assert!(engine.section(SECTION, false).is_none());
}

#[test]
fn a_surface_naming_no_path_always_applies() {
    let mut engine = marked(None, None);
    engine.focus(Some(&checkout("/work/a")));
    assert!(engine.applies(SECTION) && engine.applies(VIEW));
}

#[test]
fn a_source_that_does_not_apply_is_never_asked_and_says_why() {
    let mut engine = marked(None, Some(".planning/STATE.md"));
    engine.focus(Some(&checkout("/work/b")));
    let host = Marked::with(&["/work/a/.planning/STATE.md"]);
    ask(&mut engine, &host);
    ask(&mut engine, &host);
    assert!(questions(&host).is_empty(), "{:?}", questions(&host));
    assert_eq!(
        engine.withheld(SOURCE),
        Some("Not for this checkout: it has no .planning/STATE.md")
    );
    assert_eq!(
        host.listed.borrow().as_slice(),
        &[PathBuf::from("/work/b/.planning")],
        "the parent is listed once, nothing of the file is read"
    );

    engine.focus(Some(&checkout("/work/a")));
    ask(&mut engine, &host);
    ask(&mut engine, &host);
    assert_eq!(questions(&host), vec![SOURCE.to_owned()]);
    assert_eq!(engine.withheld(SOURCE), None);
}

#[test]
fn a_path_that_appears_later_is_seen_once_the_recheck_passes() {
    let mut engine = marked(Some(".planning"), None);
    engine.focus(Some(&checkout("/work/a")));
    let start = Instant::now();
    let pass = |engine: &mut DeclaredExtension, now| {
        engine.pass(&Pass {
            now,
            visible: &[],
            happened: &[],
        });
    };
    pass(&mut engine, start);
    let host = Marked::with(&[]);
    ask(&mut engine, &host);
    assert!(!engine.applies(SECTION));

    host.paths
        .borrow_mut()
        .insert(PathBuf::from("/work/a/.planning"));
    pass(&mut engine, start + Duration::from_secs(1));
    assert_eq!(
        ask(&mut engine, &host),
        0,
        "asked again only after the recheck"
    );
    pass(&mut engine, start + APPLIES_RECHECK);
    ask(&mut engine, &host);
    assert!(engine.applies(SECTION));
}

#[test]
fn the_host_answers_a_path_that_climbs_out_with_nothing() {
    let host = Marked::with(&["/work/secret"]);
    assert!(!crate::Host::checkout_has(
        &host,
        Path::new("/work/a"),
        Path::new("../secret")
    ));
    assert!(host.listed.borrow().is_empty());
}
