use super::*;
use uze_core::{
    Resource, exposure::ExposurePlan, integration::HarnessDetection, router::HarnessCapabilities,
};

/// A harness whose memory the test decides.
struct Harness {
    id: &'static str,
    writes_own: bool,
    keeps: Vec<MemoryEntry>,
}

impl IntegrationPort for Harness {
    fn id(&self) -> &'static str {
        self.id
    }
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }
    fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
        panic!("not used")
    }
    fn detect(&self) -> HarnessDetection {
        HarnessDetection::default()
    }
    fn memory_source(&self, _project_root: &Path) -> Vec<MemoryEntry> {
        self.keeps.clone()
    }
    fn writes_own_memory(&self) -> bool {
        self.writes_own
    }
}

fn kept(name: &str, origin: &str) -> MemoryEntry {
    MemoryEntry {
        name: name.into(),
        description: format!("kept by {origin}"),
        kind: FactKind::Feedback,
        scope: Scope::Project,
        origin: origin.into(),
        source: None,
        path: PathBuf::from(format!("/elsewhere/{name}.md")),
        updated: Some("2026-10-01".into()),
        writable: false,
        proposed: false,
    }
}

fn world(label: &str) -> (UzeHome, PathBuf) {
    let root = uze_testkit::temp::scratch(label);
    let project = root.join("repo");
    std::fs::create_dir_all(&project).unwrap();
    (UzeHome::at(root.join("uze")), project)
}

fn remember(home: &UzeHome, project: &Path, scope: Scope, name: &str, propose: bool) {
    MemoryStore::new(home, Some(project))
        .remember(
            scope,
            &Remember {
                name,
                kind: FactKind::Project,
                description: "a uze fact",
                body: "body",
                source: Source::Operator,
            },
            propose,
        )
        .unwrap();
}

fn harnesses() -> (Harness, Harness) {
    (
        Harness {
            id: "claude-code",
            writes_own: true,
            keeps: vec![
                kept("from-claude", "claude-code"),
                kept("shared-name", "claude-code"),
            ],
        },
        Harness {
            id: "codex",
            writes_own: false,
            keeps: vec![kept("codex-memory-summary", "codex")],
        },
    )
}

#[test]
fn an_agent_writing_through_uze_is_told_how_and_sees_every_other_source() {
    let (home, project) = world("app-memory-codex-agent");
    remember(&home, &project, Scope::Project, "shared-name", false);
    let (claude, codex) = harnesses();
    let all: Vec<&dyn IntegrationPort> = vec![&claude, &codex];

    let sections = context_sections(&home, &project, &codex, &all);
    let text = &sections[0].text;
    assert!(text.contains("uze agent memory remember"), "{text}");
    assert!(
        text.contains("- from-claude (feedback, kept by claude-code)"),
        "{text}"
    );
    assert!(
        !text.contains("codex-memory-summary"),
        "its own summary is not repeated"
    );
    assert_eq!(
        text.matches("shared-name").count(),
        1,
        "UZE's fact wins a name a harness also keeps"
    );
    assert!(text.len() <= MEMORY_SHARE);
}

#[test]
fn an_agent_writing_its_own_memory_is_told_how_to_read_and_not_how_to_write() {
    let (home, project) = world("app-memory-claude-agent");
    remember(&home, &project, Scope::Project, "a-uze-fact", false);
    let (claude, codex) = harnesses();
    let all: Vec<&dyn IntegrationPort> = vec![&claude, &codex];

    let sections = context_sections(&home, &project, &claude, &all);
    let text = &sections[0].text;
    assert!(
        !text.contains("uze agent memory remember"),
        "one way to remember: its own"
    );
    assert!(text.contains("uze agent memory list <words>"), "{text}");
    assert!(text.contains("- project: a-uze-fact"), "{text}");
    assert!(text.contains("codex-memory-summary"), "{text}");
    assert!(
        !text.contains("from-claude"),
        "Claude already loads its own"
    );
}

#[test]
fn nothing_kept_for_a_harness_that_writes_its_own_gives_no_section() {
    let (home, project) = world("app-memory-nothing");
    let claude = Harness {
        id: "claude-code",
        writes_own: true,
        keeps: Vec::new(),
    };
    assert!(context_sections(&home, &project, &claude, &[&claude]).is_empty());
}

#[test]
fn a_proposed_machine_fact_reaches_no_agent_until_accepted() {
    let (home, project) = world("app-memory-proposal");
    remember(&home, &project, Scope::Machine, "prefers-ptbr", true);
    let (claude, codex) = harnesses();
    let all: Vec<&dyn IntegrationPort> = vec![&claude, &codex];
    let before = context_sections(&home, &project, &codex, &all);
    assert!(!before[0].text.contains("prefers-ptbr"));

    MemoryStore::new(&home, None)
        .accept("prefers-ptbr")
        .unwrap();
    let after = context_sections(&home, &project, &codex, &all);
    assert!(
        after[0].text.contains("- project: prefers-ptbr"),
        "{}",
        after[0].text
    );
}

#[test]
fn the_read_instruction_is_given_with_any_index() {
    let (home, project) = world("app-memory-read-instruction");
    remember(&home, &project, Scope::Project, "a-uze-fact", false);
    let (claude, codex) = harnesses();
    let all: Vec<&dyn IntegrationPort> = vec![&claude, &codex];
    for launching in [&claude as &dyn IntegrationPort, &codex] {
        let text = &context_sections(&home, &project, launching, &all)[0].text;
        assert!(text.starts_with(READ_INSTRUCTION), "{text}");
        assert!(text.len() <= MEMORY_SHARE);
    }
}

fn caller(project: &Path) -> MemoryCaller<'_> {
    MemoryCaller::Operator { cwd: project }
}

fn app(home: &UzeHome, harnesses: Vec<Box<dyn IntegrationPort>>) -> UzeApplication {
    UzeApplication::new(home.clone(), harnesses)
}

fn remember_body(
    home: &UzeHome,
    project: &Path,
    scope: Scope,
    name: &str,
    kind: FactKind,
    body: &str,
) {
    MemoryStore::new(home, Some(project))
        .remember(
            scope,
            &Remember {
                name,
                kind,
                description: "about something else",
                body,
                source: Source::Operator,
            },
            false,
        )
        .unwrap();
}

#[test]
fn find_quotes_the_body_line_a_word_is_on_and_says_how_to_read_the_fact() {
    let (home, project) = world("app-memory-find-body");
    remember_body(
        &home,
        &project,
        Scope::Project,
        "phrase",
        FactKind::User,
        "first line\n\nThe phrase to answer with (\"frase secreta\") is kept here.",
    );
    remember_body(
        &home,
        &project,
        Scope::Machine,
        "unrelated",
        FactKind::User,
        "nothing",
    );
    let found = find::find(
        &gather(&home, Some(&project), &[], None),
        &["Secreta".to_owned()],
        None,
    );
    assert_eq!(found.total, 1);
    let matched = &found.matches[0];
    assert_eq!(matched.entry.name, "phrase");
    assert_eq!(matched.show, "uze agent memory show phrase");
    let body = matched.hits.iter().find(|hit| hit.field == "body").unwrap();
    assert!(body.text.contains("frase secreta"), "{body:?}");
    let file = std::fs::read_to_string(&matched.entry.path).unwrap();
    assert_eq!(
        file.lines().nth(body.line.unwrap() - 1).map(str::trim),
        Some(body.text.as_str()),
        "the line number is the file's"
    );
}

#[test]
fn find_ranks_by_how_many_words_a_fact_holds_and_filters_by_kind() {
    let (home, project) = world("app-memory-find-rank");
    remember_body(
        &home,
        &project,
        Scope::Project,
        "one-word",
        FactKind::Project,
        "alpha",
    );
    remember_body(
        &home,
        &project,
        Scope::Project,
        "both-words",
        FactKind::Reference,
        "alpha beta",
    );
    let report = gather(&home, Some(&project), &[], None);
    let words = ["alpha".to_owned(), "beta".to_owned()];
    let found = find::find(&report, &words, None);
    let names: Vec<&str> = found
        .matches
        .iter()
        .map(|m| m.entry.name.as_str())
        .collect();
    assert_eq!(names, ["both-words", "one-word"]);
    let projects = find::find(&report, &words, Some(FactKind::Project));
    assert_eq!(projects.total, 1);
}

#[test]
fn find_never_reaches_a_proposal() {
    let (home, project) = world("app-memory-find-proposal");
    remember(&home, &project, Scope::Machine, "proposed-fact", true);
    let found = find::find(
        &gather(&home, Some(&project), &[], None),
        &["proposed".to_owned()],
        None,
    );
    assert_eq!(found.total, 0);
}

#[test]
fn show_without_a_scope_finds_a_machine_fact() {
    let (home, project) = world("app-memory-show-machine");
    remember(&home, &project, Scope::Machine, "machine-only", false);
    let app = app(&home, Vec::new());
    let shown = app
        .memory()
        .show(caller(&project), Scope::Project, "machine-only")
        .unwrap();
    assert_eq!(shown.entry.scope, Scope::Machine);
}

#[test]
fn a_project_fact_wins_a_machine_fact_of_the_same_name() {
    let (home, project) = world("app-memory-collision");
    remember(&home, &project, Scope::Machine, "same", false);
    remember(&home, &project, Scope::Project, "same", false);
    let report = gather(&home, Some(&project), &[], None);
    let same: Vec<&MemoryEntry> = report.entries.iter().filter(|e| e.name == "same").collect();
    assert_eq!(same.len(), 1);
    assert_eq!(same[0].scope, Scope::Project);
}

#[test]
fn the_write_instruction_fits_its_share() {
    assert!(
        MEMORY_INSTRUCTION.len() <= INSTRUCTION_SHARE,
        "{} bytes",
        MEMORY_INSTRUCTION.len()
    );
    assert!(MEMORY_INSTRUCTION.contains("Never record a secret"));
}
