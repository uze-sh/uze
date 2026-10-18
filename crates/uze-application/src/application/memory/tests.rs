use super::*;
use uze_core::{
    Resource, exposure::ExposurePlan, integration::HarnessDetection, router::HarnessCapabilities,
};
use uze_workspace::agent_context::MEMORY_SHARE;

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
        cites: Vec::new(),
        missing: Vec::new(),
    }
}

fn world(label: &str) -> (UzeHome, PathBuf) {
    let root = uze_testkit::temp::scratch(label);
    let project = root.join("repo");
    std::fs::create_dir_all(&project).unwrap();
    (UzeHome::at(root.join("uze")), project)
}

/// Asked with the share memory is given beside a full briefing.
fn reserved(project: &Path) -> SectionAsk<'_> {
    SectionAsk {
        project_root: project,
        checkout: project,
        room: MEMORY_SHARE,
    }
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

    let sections = context_sections(&home, &reserved(&project), &codex, &all);
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

    let sections = context_sections(&home, &reserved(&project), &claude, &all);
    let text = &sections[0].text;
    assert!(
        !text.contains("uze agent memory remember"),
        "one way to remember: its own"
    );
    assert!(text.contains("uze agent memory list <words>"), "{text}");
    assert!(
        text.contains("- a-uze-fact (project): a uze fact"),
        "{text}"
    );
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
    assert!(context_sections(&home, &reserved(&project), &claude, &[&claude]).is_empty());
}

#[test]
fn a_proposed_machine_fact_reaches_no_agent_until_accepted() {
    let (home, project) = world("app-memory-proposal");
    remember(&home, &project, Scope::Machine, "prefers-ptbr", true);
    let (claude, codex) = harnesses();
    let all: Vec<&dyn IntegrationPort> = vec![&claude, &codex];
    let before = context_sections(&home, &reserved(&project), &codex, &all);
    assert!(!before[0].text.contains("prefers-ptbr"));

    MemoryStore::new(&home, None)
        .accept("prefers-ptbr")
        .unwrap();
    let after = context_sections(&home, &reserved(&project), &codex, &all);
    assert!(
        after[0]
            .text
            .contains("- prefers-ptbr (project, machine): a uze fact"),
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
        let text = &context_sections(&home, &reserved(&project), launching, &all)[0].text;
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

#[test]
fn memory_fills_the_room_it_is_asked_with() {
    let (home, project) = world("app-memory-room");
    for index in 0..80 {
        MemoryStore::new(&home, Some(&project))
            .remember(
                Scope::Project,
                &Remember {
                    name: &format!("fact-{index:02}"),
                    kind: FactKind::Feedback,
                    description: "a standing piece of guidance long enough to need its own line",
                    body: "body",
                    source: Source::Operator,
                },
                false,
            )
            .unwrap();
    }
    let codex = Harness {
        id: "codex",
        writes_own: false,
        keeps: Vec::new(),
    };
    let narrow = &context_sections(&home, &reserved(&project), &codex, &[])[0];
    let roomy = &context_sections(
        &home,
        &SectionAsk {
            room: 8_000,
            ..reserved(&project)
        },
        &codex,
        &[],
    )[0];
    assert_eq!(roomy.share, 8_000);
    assert!(
        roomy.text.len() > MEMORY_SHARE,
        "{} bytes",
        roomy.text.len()
    );
    assert!(roomy.text.len() <= 8_000);
    assert!(roomy.text.matches("\n- fact-").count() > narrow.text.matches("\n- fact-").count());
}

#[test]
fn find_folds_accents_and_ranks_a_name_over_a_body() {
    let (home, project) = world("app-memory-find-fold");
    remember_body(
        &home,
        &project,
        Scope::Project,
        "in-the-body",
        FactKind::Feedback,
        "A decisão foi registrada.",
    );
    remember_body(
        &home,
        &project,
        Scope::Project,
        "decisao-registrada",
        FactKind::Reference,
        "nothing",
    );
    let report = gather(&home, Some(&project), &[], None);
    let found = find::find(&report, &["decisao".to_owned()], None);
    let names: Vec<&str> = found
        .matches
        .iter()
        .map(|m| m.entry.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["decisao-registrada", "in-the-body"],
        "the name outranks a body, whatever the kind"
    );
    let body = found.matches[1]
        .hits
        .iter()
        .find(|hit| hit.field == "body")
        .unwrap();
    assert_eq!(body.text, "A decisão foi registrada.", "quoted as written");
}

#[test]
fn show_follows_links_both_ways_however_they_are_spelled() {
    let (home, project) = world("app-memory-links");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    let claude_dir = project.parent().unwrap().join("claude-memory");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let stemmed = claude_dir.join("feedback_reply_in_ptbr.md");
    std::fs::write(
        &stemmed,
        "---\nname: reply-in-ptbr\n---\n\nReply in pt-BR.\n",
    )
    .unwrap();
    let mut reply = kept("reply-in-ptbr", "claude-code");
    reply.path = stemmed;
    let prefixed = claude_dir.join("finish.md");
    std::fs::write(
        &prefixed,
        "---\nname: feedback-finish-tasks\n---\n\nFinish.\n",
    )
    .unwrap();
    let mut finish = kept("feedback-finish-tasks", "claude-code");
    finish.path = prefixed;
    let claude: Box<dyn IntegrationPort> = Box::new(Harness {
        id: "claude-code",
        writes_own: true,
        keeps: vec![reply, finish],
    });
    remember_body(
        &home,
        &project,
        Scope::Project,
        "a",
        FactKind::Project,
        "See [[b]], [[feedback_reply_in_ptbr]], [[finish_tasks]] and [[nowhere]].",
    );
    remember_body(
        &home,
        &project,
        Scope::Project,
        "b",
        FactKind::Project,
        "Plain.",
    );
    let app = app(&home, vec![claude]);

    let a = app
        .memory()
        .show(caller(&project), Scope::Project, "a")
        .unwrap();
    let resolved: Vec<(&str, Option<&str>)> = a
        .links
        .iter()
        .map(|link| {
            (
                link.written.as_str(),
                link.fact.as_ref().map(|f| f.name.as_str()),
            )
        })
        .collect();
    assert_eq!(
        resolved,
        [
            ("b", Some("b")),
            ("feedback_reply_in_ptbr", Some("reply-in-ptbr")),
            ("finish_tasks", Some("feedback-finish-tasks")),
            ("nowhere", None),
        ]
    );
    let b = app
        .memory()
        .show(caller(&project), Scope::Project, "b")
        .unwrap();
    let from: Vec<&str> = b.linked_from.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(from, ["a"]);
    let reply = app
        .memory()
        .show(caller(&project), Scope::Project, "reply-in-ptbr")
        .unwrap();
    assert_eq!(
        reply.linked_from.len(),
        1,
        "a source's fact is linked to as well"
    );
}

#[test]
fn a_link_two_facts_answer_loosely_is_left_unresolved() {
    let (home, project) = world("app-memory-links-ambiguous");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    remember_body(
        &home,
        &project,
        Scope::Project,
        "a",
        FactKind::Project,
        "[[ptbr_reply]]",
    );
    remember_body(
        &home,
        &project,
        Scope::Project,
        "ptbr-reply",
        FactKind::User,
        "x",
    );
    remember_body(
        &home,
        &project,
        Scope::Project,
        "user-ptbr-reply",
        FactKind::User,
        "y",
    );
    let shown = app(&home, Vec::new())
        .memory()
        .show(caller(&project), Scope::Project, "a")
        .unwrap();
    assert!(shown.links[0].fact.is_none(), "{:?}", shown.links);
}

/// A project at `repo/` with `crates/a/src/` and a `.gitignore` naming
/// `target/`.
fn citing_world(label: &str) -> (UzeHome, PathBuf) {
    let (home, project) = world(label);
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::create_dir_all(project.join("crates/a/src")).unwrap();
    std::fs::create_dir_all(project.join("target/debug")).unwrap();
    std::fs::write(project.join("crates/a/src/kept.rs"), "").unwrap();
    std::fs::write(project.join(".gitignore"), "/target/\n*.log\n").unwrap();
    (home, project)
}

fn missing_of<'a>(report: &'a MemoryReport, name: &str) -> &'a [String] {
    &report
        .entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap()
        .missing
}

#[test]
fn a_fact_citing_what_the_checkout_lacks_is_marked_and_left_as_written() {
    let (home, project) = citing_world("app-memory-citations");
    let cite = |scope, name: &str, body: &str| {
        remember_body(&home, &project, scope, name, FactKind::Project, body);
    };
    cite(Scope::Project, "moved", "Fixed in `crates/a/src/gone.rs`.");
    cite(
        Scope::Project,
        "current",
        "Lives in `crates/a/src/kept.rs`.",
    );
    cite(Scope::Project, "fragment", "See `platform/unix.rs`.");
    cite(Scope::Project, "ignored", "Built to `target/debug/out.rs`.");
    cite(
        Scope::Machine,
        "elsewhere",
        "Fixed in `crates/a/src/gone.rs`.",
    );
    let before = std::fs::read_to_string(
        MemoryStore::new(&home, Some(&project))
            .dir(Scope::Project)
            .unwrap()
            .join("moved.md"),
    )
    .unwrap();

    let report = app(&home, Vec::new())
        .memory()
        .list(caller(&project))
        .unwrap();
    assert_eq!(missing_of(&report, "moved"), ["crates/a/src/gone.rs"]);
    for name in ["current", "fragment", "ignored", "elsewhere"] {
        assert!(missing_of(&report, name).is_empty(), "{name}");
    }
    let shown = app(&home, Vec::new())
        .memory()
        .show(caller(&project), Scope::Project, "moved")
        .unwrap();
    assert_eq!(shown.entry.missing, ["crates/a/src/gone.rs"]);
    let after = std::fs::read_to_string(&shown.entry.path).unwrap();
    assert_eq!(before, after, "a mark is never written into the fact");
}

#[test]
fn a_file_only_the_agents_branch_has_is_not_missing_for_that_agent() {
    let (home, project) = citing_world("app-memory-citations-branch");
    remember_body(
        &home,
        &project,
        Scope::Project,
        "new-module",
        FactKind::Project,
        "Added `crates/a/src/added.rs`.",
    );
    let slot = project.join(".worktrees/slot");
    std::fs::create_dir_all(slot.join("crates/a/src")).unwrap();
    std::fs::write(slot.join(".git"), "gitdir: elsewhere\n").unwrap();
    std::fs::write(slot.join("crates/a/src/added.rs"), "").unwrap();
    let codex = Harness {
        id: "codex",
        writes_own: false,
        keeps: Vec::new(),
    };
    let in_slot = &context_sections(
        &home,
        &SectionAsk {
            checkout: &slot,
            ..reserved(&project)
        },
        &codex,
        &[],
    )[0]
    .text;
    assert!(!in_slot.contains("does not have"), "{in_slot}");
    let in_primary = &context_sections(&home, &reserved(&project), &codex, &[])[0].text;
    assert!(
        in_primary.contains("- new-module (project, cites a path this checkout does not have): "),
        "{in_primary}"
    );
}

#[test]
fn citations_are_judged_in_rank_order_up_to_the_cap() {
    let (home, project) = citing_world("app-memory-citations-cap");
    for index in 0..CITATION_CHECKS {
        remember_body(
            &home,
            &project,
            Scope::Project,
            &format!("reference-{index:02}"),
            FactKind::Reference,
            &format!("`crates/a/src/gone-{index}.rs`"),
        );
    }
    remember_body(
        &home,
        &project,
        Scope::Project,
        "guidance",
        FactKind::Feedback,
        "`crates/a/src/gone.rs`",
    );
    let report = app(&home, Vec::new())
        .memory()
        .list(caller(&project))
        .unwrap();
    let marked = report
        .entries
        .iter()
        .filter(|e| !e.missing.is_empty())
        .count();
    assert_eq!(marked, CITATION_CHECKS);
    assert!(
        !missing_of(&report, "guidance").is_empty(),
        "the fact ranked first is judged, whatever order the files sort in"
    );
    assert!(
        missing_of(&report, "reference-63").is_empty(),
        "past the cap"
    );
    let shown = app(&home, Vec::new())
        .memory()
        .show(caller(&project), Scope::Project, "reference-63")
        .unwrap();
    assert_eq!(
        shown.entry.missing,
        ["crates/a/src/gone-63.rs"],
        "one fact asked for is judged whole, whatever the cap left of the rest"
    );
}

#[test]
fn a_new_name_saying_what_another_fact_says_is_written_and_named() {
    let (home, project) = world("app-memory-similar");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    let app = app(&home, Vec::new());
    let write = |name: &str, description: &str| {
        app.memory()
            .remember(
                caller(&project),
                Scope::Project,
                name,
                FactKind::Feedback,
                description,
                "",
            )
            .unwrap()
    };
    let first = write(
        "reply-in-ptbr",
        "Answer the operator in Brazilian Portuguese; code and commits stay English",
    );
    assert!(first.similar.is_none());
    let paraphrase = write("reply-pt-br", "Answer the user in Brazilian Portuguese");
    assert_eq!(
        paraphrase.similar.map(|fact| fact.name).as_deref(),
        Some("reply-in-ptbr")
    );
    assert!(paraphrase.path.exists(), "written all the same");
    let again = write(
        "reply-pt-br",
        "Answer the user in Brazilian Portuguese, always",
    );
    assert!(
        again.updated && again.similar.is_none(),
        "an update is not a duplicate"
    );
    let unrelated = write("squash-at-merge", "Squash with the merge button");
    assert!(unrelated.similar.is_none());
}

#[test]
fn a_near_duplicate_is_only_a_fact_the_same_write_could_have_updated() {
    let (home, project) = world("app-memory-similar-scope");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    let mut claude_kept = kept("answer-in-portuguese", "claude-code");
    claude_kept.description = "Answer the operator in Brazilian Portuguese".into();
    let claude: Box<dyn IntegrationPort> = Box::new(Harness {
        id: "claude-code",
        writes_own: true,
        keeps: vec![claude_kept],
    });
    let app = app(&home, vec![claude]);
    let write = |scope, name: &str, description: &str| {
        app.memory()
            .remember(
                caller(&project),
                scope,
                name,
                FactKind::Feedback,
                description,
                "",
            )
            .unwrap()
    };
    write(
        Scope::Project,
        "reply-in-ptbr",
        "Reply to the operator in Brazilian Portuguese always",
    );
    let machine = write(
        Scope::Machine,
        "reply-in-ptbr",
        "Reply to the operator in Brazilian Portuguese always",
    );
    assert!(
        machine.similar.is_none(),
        "the project fact is not one a machine write updates: {:?}",
        machine.similar
    );
    let paraphrase = write(
        Scope::Project,
        "portuguese-replies",
        "Answer the operator in Brazilian Portuguese",
    );
    assert_eq!(
        paraphrase.similar.map(|fact| fact.name).as_deref(),
        Some("reply-in-ptbr"),
        "the project fact this write could have updated, never the one a harness keeps"
    );
}

#[test]
fn a_linked_fact_citing_what_the_checkout_lacks_carries_its_mark() {
    let (home, project) = citing_world("app-memory-citations-linked");
    remember_body(
        &home,
        &project,
        Scope::Project,
        "a",
        FactKind::Project,
        "See [[b]].",
    );
    remember_body(
        &home,
        &project,
        Scope::Project,
        "b",
        FactKind::Project,
        "Fixed in `crates/a/src/gone.rs`, see [[a]].",
    );
    let shown = app(&home, Vec::new())
        .memory()
        .show(caller(&project), Scope::Project, "a")
        .unwrap();
    let linked = shown.links[0].fact.as_ref().unwrap();
    assert_eq!(linked.missing, ["crates/a/src/gone.rs"], "links to");
    assert_eq!(
        shown.linked_from[0].missing,
        ["crates/a/src/gone.rs"],
        "linked from"
    );
}

#[test]
fn a_proposal_and_an_applied_fact_are_never_offered_for_each_other() {
    let (home, project) = world("app-memory-similar-proposal");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    let app = app(&home, Vec::new());
    let description = "Reply to the operator in Brazilian Portuguese always";
    let agent = MemoryCaller::Agent(uze_workspace::conversation::Claim {
        id: "not-recorded",
        key: "k",
        cwd: &project,
    });
    let proposal = app
        .memory()
        .remember(
            agent,
            Scope::Machine,
            "reply-in-ptbr",
            FactKind::Feedback,
            description,
            "",
        )
        .unwrap();
    assert!(proposal.proposed);
    let applied = app
        .memory()
        .remember(
            caller(&project),
            Scope::Machine,
            "portuguese-replies",
            FactKind::Feedback,
            description,
            "",
        )
        .unwrap();
    assert!(
        applied.similar.is_none(),
        "remembering under a proposal's name writes beside it, never over it: {:?}",
        applied.similar
    );
}
