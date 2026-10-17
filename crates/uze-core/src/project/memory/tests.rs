use super::*;

fn home(label: &str) -> (UzeHome, PathBuf) {
    let root = uze_testkit::temp::scratch(label);
    let project = root.join("repo");
    fs::create_dir_all(&project).unwrap();
    (UzeHome::at(root.join("uze")), project)
}

fn fact<'a>(name: &'a str, description: &'a str, body: &'a str) -> Remember<'a> {
    Remember {
        name,
        kind: FactKind::Project,
        description,
        body,
        source: Source::Agent {
            harness: "codex".into(),
            task: "t-1".into(),
        },
    }
}

#[test]
fn a_fact_round_trips_through_its_document() {
    let (home, project) = home("memory-round-trip");
    let store = MemoryStore::new(&home, Some(&project));
    let written = store
        .remember(
            Scope::Project,
            &fact("wsl-kill", "kill by pid: use libc", "the body\n"),
            false,
        )
        .unwrap();
    assert!(!written.updated && !written.proposed);
    assert!(
        !written.path.starts_with(&project),
        "memory never enters a checkout"
    );

    let read = store.show(Scope::Project, "wsl-kill").unwrap();
    assert_eq!(read.description, "kill by pid: use libc");
    assert_eq!(read.body, "the body");
    assert_eq!(read.kind, FactKind::Project);
    assert_eq!(
        read.source,
        Source::Agent {
            harness: "codex".into(),
            task: "t-1".into()
        }
    );
}

#[test]
fn remembering_twice_keeps_the_creation_date_and_one_fact() {
    let (home, project) = home("memory-update");
    let store = MemoryStore::new(&home, Some(&project));
    store
        .remember(Scope::Project, &fact("a", "first", "one"), false)
        .unwrap();
    let path = store.dir(Scope::Project).unwrap().join("a.md");
    let original = fs::read_to_string(&path).unwrap().replace(
        &format!("created: \"{}\"", today()),
        "created: \"2026-01-02\"",
    );
    fs::write(&path, original).unwrap();

    let again = store
        .remember(Scope::Project, &fact("a", "second", "two"), false)
        .unwrap();
    assert!(again.updated);
    let read = store.show(Scope::Project, "a").unwrap();
    assert_eq!(read.created, "2026-01-02");
    assert_eq!(read.description, "second");
    assert_eq!(store.list(Scope::Project).unwrap().entries.len(), 1);
}

#[test]
fn an_unreadable_fact_is_reported_and_never_written_over() {
    let (home, project) = home("memory-unreadable");
    let store = MemoryStore::new(&home, Some(&project));
    let dir = store.dir(Scope::Project).unwrap();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("broken.md"), "no front matter here").unwrap();
    fs::write(dir.join("newer.md"), "---\nshape: 9\nname: newer\n---\nx").unwrap();

    let listing = store.list(Scope::Project).unwrap();
    assert!(listing.entries.is_empty());
    assert_eq!(listing.unreadable.len(), 2);
    assert!(
        store
            .remember(Scope::Project, &fact("broken", "d", "b"), false)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(dir.join("broken.md")).unwrap(),
        "no front matter here"
    );
}

#[test]
fn a_hand_edit_is_read_as_edited() {
    let (home, project) = home("memory-hand-edit");
    let store = MemoryStore::new(&home, Some(&project));
    let written = store
        .remember(Scope::Project, &fact("a", "before", "b"), false)
        .unwrap();
    let text = fs::read_to_string(&written.path)
        .unwrap()
        .replace("\"before\"", "\"after, by hand\"");
    fs::write(&written.path, text).unwrap();
    assert_eq!(
        store.list(Scope::Project).unwrap().entries[0].description,
        "after, by hand"
    );
}

#[test]
fn two_writers_of_different_facts_both_land() {
    let (home, project) = home("memory-concurrent");
    let names: Vec<String> = (0..8).map(|n| format!("fact-{n}")).collect();
    std::thread::scope(|scope| {
        for name in &names {
            let (home, project) = (&home, &project);
            scope.spawn(move || {
                MemoryStore::new(home, Some(project))
                    .remember(Scope::Project, &fact(name, "d", "b"), false)
                    .unwrap();
            });
        }
    });
    let listing = MemoryStore::new(&home, Some(&project))
        .list(Scope::Project)
        .unwrap();
    assert_eq!(listing.entries.len(), names.len());
    assert!(listing.unreadable.is_empty(), "every document whole");
}

#[test]
fn a_proposal_reaches_no_index_until_it_is_accepted() {
    let (home, project) = home("memory-proposal");
    let store = MemoryStore::new(&home, Some(&project));
    let proposed = store
        .remember(
            Scope::Machine,
            &fact("prefers-ptbr", "reply in pt-BR", "b"),
            true,
        )
        .unwrap();
    assert!(proposed.proposed);
    assert!(store.list(Scope::Machine).unwrap().entries.is_empty());
    assert_eq!(store.proposals().entries.len(), 1);

    store.accept("prefers-ptbr").unwrap();
    assert_eq!(store.list(Scope::Machine).unwrap().entries.len(), 1);
    assert!(store.proposals().entries.is_empty());

    store
        .remember(Scope::Machine, &fact("other", "d", "b"), true)
        .unwrap();
    assert!(store.discard("other").unwrap());
    assert!(store.proposals().entries.is_empty());
}

#[test]
fn outside_a_project_the_project_scope_refuses() {
    let (home, _project) = home("memory-no-project");
    let store = MemoryStore::new(&home, None);
    assert!(
        store
            .remember(Scope::Project, &fact("a", "d", "b"), false)
            .is_err()
    );
    assert!(
        store
            .remember(Scope::Machine, &fact("a", "d", "b"), false)
            .is_ok()
    );
}

#[test]
fn a_name_that_could_steer_a_path_is_refused() {
    let (home, project) = home("memory-names");
    let store = MemoryStore::new(&home, Some(&project));
    for name in ["../x", "A", "", "a/b", "-a"] {
        assert!(
            store
                .remember(Scope::Project, &fact(name, "d", "b"), false)
                .is_err(),
            "{name}"
        );
    }
}

#[test]
fn forgetting_removes_the_fact() {
    let (home, project) = home("memory-forget");
    let store = MemoryStore::new(&home, Some(&project));
    store
        .remember(Scope::Project, &fact("a", "d", "b"), false)
        .unwrap();
    assert!(store.forget(Scope::Project, "a").unwrap());
    assert!(!store.forget(Scope::Project, "a").unwrap());
    assert!(store.list(Scope::Project).unwrap().entries.is_empty());
}

fn entry(name: &str, kind: FactKind, updated: &str) -> MemoryEntry {
    MemoryEntry {
        name: name.into(),
        description: format!("about {name}"),
        kind,
        scope: Scope::Project,
        origin: UZE_ORIGIN.into(),
        source: None,
        path: PathBuf::from(format!("/m/{name}.md")),
        updated: Some(updated.into()),
        writable: true,
        proposed: false,
    }
}

#[test]
fn the_index_keeps_standing_guidance_first_and_counts_what_it_leaves_out() {
    let mut entries: Vec<MemoryEntry> = (0..40)
        .map(|n| {
            entry(
                &format!("ref-{n:02}"),
                FactKind::Reference,
                &format!("2026-10-{:02}", n % 28 + 1),
            )
        })
        .collect();
    entries.push(entry("old-feedback", FactKind::Feedback, "2020-01-01"));
    entries.push(entry("who", FactKind::User, "2020-01-01"));
    let index = render_index(&entries, 600);
    assert!(index.len() <= 600, "{} bytes", index.len());
    let first: Vec<&str> = index.lines().take(2).collect();
    assert!(first[0].starts_with("- old-feedback (feedback)"), "{index}");
    assert!(first[1].starts_with("- who (user)"), "{index}");
    assert!(index.contains("more kept; `uze agent memory list` lists them all."));
}

#[test]
fn a_harness_entry_names_where_to_read_it() {
    let mut claude = entry("from-claude", FactKind::Feedback, "2026-10-01");
    claude.origin = "claude-code".into();
    claude.path = PathBuf::from("/home/u/.claude/projects/x/memory/from-claude.md");
    let index = render_index(&[claude], 1000);
    assert!(index.contains("kept by claude-code"));
    assert!(index.contains("Read /home/u/.claude/projects/x/memory/from-claude.md"));
}

#[test]
fn a_foreign_front_matter_shape_reads_its_kind_and_date() {
    let (head, body) = read_document(
        "---\nname: reply-in-ptbr\ndescription: \"Reply in pt-BR\"\nmetadata:\n  type: feedback\n  \
         modified: 2026-09-26T00:56:09.401Z\n---\n\nThe body.\n",
    )
    .unwrap();
    assert_eq!(head.kind(), Some(FactKind::Feedback));
    assert_eq!(head.updated().as_deref(), Some("2026-09-26"));
    assert_eq!(body.trim(), "The body.");
}

#[test]
fn a_date_is_civil() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_743), (2026, 10, 17));
}
