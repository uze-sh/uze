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
fn an_agent_writing_its_own_memory_is_given_only_the_index() {
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
    assert!(text.contains("- a-uze-fact"));
    assert!(text.contains("codex-memory-summary"));
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
    assert!(after[0].text.contains("- prefers-ptbr (project, machine)"));
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
