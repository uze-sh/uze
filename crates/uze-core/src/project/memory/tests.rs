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
        cites: Vec::new(),
        missing: Vec::new(),
    }
}

#[test]
fn every_fact_is_given_a_line_when_it_fits_guidance_first() {
    let entries = vec![
        entry("dashboards", FactKind::Reference, "2026-10-03"),
        entry("secret-phrase", FactKind::User, "2026-10-18"),
        entry("reply-in-ptbr", FactKind::Feedback, "2026-10-01"),
        entry("lab-image", FactKind::Project, "2026-10-02"),
    ];
    let index = render_index(&entries, 2_000);
    let lines: Vec<&str> = index.lines().collect();
    assert_eq!(
        lines,
        [
            "- reply-in-ptbr (feedback): about reply-in-ptbr",
            "- secret-phrase (user): about secret-phrase",
            "- lab-image (project): about lab-image",
            "- dashboards (reference): about dashboards",
        ],
        "{index}"
    );
}

#[test]
fn past_the_room_lines_follow_the_ranking_and_the_rest_are_named() {
    let entries = vec![
        entry("reply-in-ptbr", FactKind::Feedback, "2026-10-01"),
        entry("secret-phrase", FactKind::User, "2026-10-18"),
        entry("lab-image", FactKind::Project, "2026-10-02"),
        entry("dashboards", FactKind::Reference, "2026-10-03"),
    ];
    let index = render_index(&entries, 180);
    assert!(index.len() <= 180, "{} bytes", index.len());
    assert!(index.starts_with("- reply-in-ptbr (feedback): "), "{index}");
    assert!(index.contains("- reference: dashboards"), "{index}");
    for name in ["secret-phrase", "lab-image", "dashboards"] {
        assert!(index.contains(name), "{name} is at least named: {index}");
    }
}

#[test]
fn one_long_description_never_ends_the_lines_after_it() {
    let mut long = entry("long", FactKind::Feedback, "2026-10-02");
    long.description = "word ".repeat(200);
    let short = entry("short", FactKind::Feedback, "2026-10-01");
    let index = render_index(&[long, short], 600);
    let first = index.lines().next().unwrap();
    assert!(first.ends_with('…') && first.len() < 220, "{first}");
    assert!(index.contains("- short (feedback): about short"), "{index}");
}

/// The case that left an operator's fact out of every agent but Claude:
/// enough guidance to fill the room pushed every other kind past it.
#[test]
fn guidance_past_the_room_never_pushes_another_fact_out_of_the_names() {
    let mut entries: Vec<MemoryEntry> = (0..30)
        .map(|n| {
            let mut guidance = entry(
                &format!("guidance-{n:02}"),
                FactKind::Feedback,
                "2026-10-01",
            );
            guidance.description = "a long standing guidance line ".repeat(5);
            guidance
        })
        .collect();
    entries.push(entry("secret-phrase", FactKind::User, "2026-10-18"));
    let index = render_index(&entries, 2_500);
    assert!(index.len() <= 2_500, "{} bytes", index.len());
    assert!(index.starts_with("- guidance-"), "{index}");
    assert!(index.contains("secret-phrase"), "{index}");
    assert!(
        index.contains("- feedback: guidance-"),
        "guidance with no room for a line is still named: {index}"
    );
    assert!(!index.contains("more kept"), "{index}");
}

#[test]
fn names_past_the_room_are_counted() {
    let entries: Vec<MemoryEntry> = (0..200)
        .map(|n| {
            entry(
                &format!("reference-number-{n:03}"),
                FactKind::Reference,
                "2026-10-01",
            )
        })
        .collect();
    let index = render_index(&entries, 600);
    assert!(index.len() <= 600, "{} bytes", index.len());
    assert!(
        index.contains("more kept; `uze agent memory list` lists them all."),
        "{index}"
    );
}

#[test]
fn a_harness_entry_says_who_keeps_it_and_no_path() {
    let mut claude = entry("from-claude", FactKind::Feedback, "2026-10-01");
    claude.origin = "claude-code".into();
    claude.path = PathBuf::from("/home/u/.claude/projects/x/memory/from-claude.md");
    let index = render_index(&[claude], 1000);
    assert!(index.contains("kept by claude-code"));
    assert!(!index.contains("/home/u/.claude"), "{index}");
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

#[test]
fn citations_are_paths_and_nothing_that_only_looks_like_one() {
    let body = "\
Fixed in `crates/uze-core/src/subprocess.rs:42`, see `docs/adr/` and
`.github/workflows/ci.yml#L10`; built into `target/debug/uze`.
Not paths: `openspec/changes/<name>/`, `src/ui/*.rs`, `docs/readme`,
`platform`, `/etc/hosts`, `~/.uze/state`, `$HOME/x.rs`, `./run.sh`,
`a/../b.rs`, `https://example.com/a.rs`, `KEY=a/b.rs`, `user@host/a.rs`.
```
`crates/in/a/fence.rs`
```
Twice: `docs/adr/`.";
    assert_eq!(
        cited_paths(body),
        [
            "crates/uze-core/src/subprocess.rs",
            "docs/adr/",
            ".github/workflows/ci.yml",
        ],
        "a file with no extension (`target/debug/uze`) could be a branch, and is left out"
    );
}

#[test]
fn a_fact_citing_what_is_gone_is_marked_where_it_is_named_and_named_once() {
    let mut described = entry("moved-file", FactKind::Feedback, "2026-10-03");
    described.missing = vec!["crates/a/b.rs".into()];
    let current = entry("still-there", FactKind::Project, "2026-10-02");
    let index = render_index(&[described, current], 1_000);
    assert!(
        index.contains("- moved-file (feedback, cites a path this checkout does not have): "),
        "{index}"
    );
    assert!(
        !index.contains("crates/a/b.rs"),
        "no index line carries a path"
    );

    let stale: Vec<MemoryEntry> = (0..12)
        .map(|n| {
            let mut fact = entry(&format!("moved-{n:02}"), FactKind::Project, "2026-10-01");
            fact.description = "a file that moved".into();
            fact.missing = vec!["crates/a/b.rs".into()];
            fact
        })
        .collect();
    let index = render_index(&stale, 400);
    assert!(index.len() <= 400, "{} bytes", index.len());
    for n in 0..12 {
        let name = format!("moved-{n:02}");
        assert!(
            index.matches(&name).count() <= 1,
            "{name} named twice: {index}"
        );
    }
    let group = index
        .lines()
        .find(|line| line.starts_with("- citing paths this checkout does not have"))
        .unwrap_or_default();
    for n in 0..12 {
        let name = format!("moved-{n:02}");
        let described = index.contains(&format!("- {name} (project, cites a path"));
        assert!(
            described || group.contains(&name) || !index.contains(&name),
            "{name} is named outside the stale group without its mark: {index}"
        );
    }
    assert!(
        !group.is_empty(),
        "facts named alone were not grouped: {index}"
    );
}

#[test]
fn a_pasted_credential_is_refused_without_repeating_it() {
    let (home, project) = home("memory-credential");
    let store = MemoryStore::new(&home, Some(&project));
    // Assembled here: the repository's secret scanning reads the source.
    let secret = format!("gh{}_{}", "p", "Z9y8".repeat(9));
    let body = format!("Deploys with\n{secret}\n");
    let refused = store
        .remember(
            Scope::Project,
            &fact("deploy", "how it deploys", &body),
            false,
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("line 2 of the body"), "{refused}");
    assert!(refused.contains("a GitHub token"), "{refused}");
    assert!(!refused.contains(&secret), "never repeated: {refused}");
    assert!(store.list(Scope::Project).unwrap().entries.is_empty());

    let documented = store
        .remember(
            Scope::Project,
            &fact(
                "deploy",
                "how it deploys",
                "Set `DEPLOY_TOKEN=<token>` first.",
            ),
            false,
        )
        .unwrap();
    assert_eq!(documented.secret_looking, None);
}

#[test]
fn a_secret_looking_assignment_is_written_with_where_it_is() {
    let (home, project) = home("memory-secret-looking");
    let store = MemoryStore::new(&home, Some(&project));
    // Assembled here: the repository's secret scanning reads the source.
    let assigned = format!("DEPLOY_TOKEN={}", "f00ba4".repeat(4));
    let in_body = store
        .remember(
            Scope::Project,
            &fact(
                "deploy",
                "how it deploys",
                &format!("Run with\n{assigned}\n"),
            ),
            false,
        )
        .unwrap();
    assert_eq!(
        in_body.secret_looking,
        Some(Spot {
            field: "body",
            line: 2
        })
    );
    let in_description = store
        .remember(
            Scope::Project,
            &fact("deploy", &format!("run with {assigned}"), ""),
            false,
        )
        .unwrap();
    assert_eq!(
        in_description.secret_looking,
        Some(Spot {
            field: "description",
            line: 1
        }),
        "the description reaches every agent's index"
    );
}

#[test]
fn a_credential_in_the_description_is_refused_too() {
    let (home, project) = home("memory-credential-description");
    let store = MemoryStore::new(&home, Some(&project));
    let secret = format!("gh{}_{}", "p", "Z9y8".repeat(9));
    let refused = store
        .remember(
            Scope::Project,
            &fact("deploy", &format!("uses {secret}"), ""),
            false,
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("line 1 of the description"), "{refused}");
    assert!(store.list(Scope::Project).unwrap().entries.is_empty());
}

#[test]
fn a_scope_is_labelled_as_its_report_names_it() {
    for scope in [Scope::Project, Scope::Machine] {
        assert_eq!(
            serde_json::to_value(scope).unwrap().as_str(),
            Some(scope.label())
        );
    }
}

#[test]
fn every_kind_reads_back_from_its_own_label() {
    for kind in FactKind::ALL {
        assert_eq!(FactKind::parse(kind.label()), Some(kind));
    }
    assert_eq!(FactKind::parse("opinion"), None);
}

#[test]
fn a_source_is_described_by_who_wrote_it() {
    assert_eq!(Source::Operator.describe(), "the operator");
    let named = Source::Agent {
        harness: "some-harness".into(),
        task: "t-7".into(),
    }
    .describe();
    assert!(
        named.contains("some-harness") && named.contains("t-7"),
        "{named}"
    );
    let unnamed = Source::Agent {
        harness: String::new(),
        task: "t-7".into(),
    }
    .describe();
    assert!(
        unnamed.starts_with("an agent") && unnamed.contains("t-7"),
        "{unnamed}"
    );
}

#[test]
fn a_store_knows_whether_it_has_a_project() {
    let (home, project) = home("memory-has-project");
    assert!(MemoryStore::new(&home, Some(&project)).has_project());
    assert!(!MemoryStore::new(&home, None).has_project());
}

#[test]
fn a_machine_fact_the_operator_writes_applies_at_once() {
    let (home, project) = home("memory-machine-direct");
    let store = MemoryStore::new(&home, Some(&project));
    let written = store
        .remember(
            Scope::Machine,
            &fact("prefers-ptbr", "reply in pt-BR", "b"),
            false,
        )
        .unwrap();
    assert!(!written.proposed);
    assert_eq!(store.list(Scope::Machine).unwrap().entries.len(), 1);
    assert!(store.proposals().entries.is_empty());

    let project_fact = store
        .remember(Scope::Project, &fact("lab-image", "d", "b"), true)
        .unwrap();
    assert!(
        !project_fact.proposed,
        "only a machine fact is ever proposed"
    );
}

#[test]
fn a_fact_from_a_newer_build_is_unreadable() {
    let (home, project) = home("memory-newer-shape");
    let store = MemoryStore::new(&home, Some(&project));
    let dir = store.dir(Scope::Project).unwrap();
    fs::create_dir_all(&dir).unwrap();
    let newer = "---\nshape: 2\nname: newer\ndescription: d\ntype: project\n---\n\nbody\n";
    fs::write(dir.join("newer.md"), newer).unwrap();

    let listing = store.list(Scope::Project).unwrap();
    assert!(listing.entries.is_empty(), "{listing:?}");
    assert_eq!(listing.unreadable.len(), 1);
}

#[test]
fn only_markdown_files_are_read_as_facts() {
    let (home, project) = home("memory-markdown-only");
    let store = MemoryStore::new(&home, Some(&project));
    store
        .remember(Scope::Project, &fact("kept", "d", "b"), false)
        .unwrap();
    let dir = store.dir(Scope::Project).unwrap();
    fs::write(dir.join("notes.txt"), "not a fact").unwrap();
    fs::create_dir_all(dir.join("folder.md")).unwrap();

    let listing = store.list(Scope::Project).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert!(listing.unreadable.is_empty(), "{listing:?}");
}

#[test]
fn forgetting_what_cannot_be_removed_is_an_error_not_an_absence() {
    let (home, project) = home("memory-forget-refused");
    let store = MemoryStore::new(&home, Some(&project));
    let dir = store.dir(Scope::Project).unwrap();
    fs::create_dir_all(dir.join("stuck.md")).unwrap();
    assert!(store.forget(Scope::Project, "stuck").is_err());
}

#[test]
fn discarding_answers_whether_there_was_a_proposal() {
    let (home, project) = home("memory-discard");
    let store = MemoryStore::new(&home, Some(&project));
    assert!(!store.discard("never-proposed").unwrap());

    fs::create_dir_all(home.proposed_memory_dir().join("stuck.md")).unwrap();
    assert!(store.discard("stuck").is_err());
}

#[test]
fn citations_with_a_space_or_an_odd_segment_are_not_paths() {
    let body = "Run `cargo fmt crates/a.rs`, see `src//main.rs` or `src/./main.rs`, \
                and `release/v2.` but `crates/kept.rs`.";
    assert_eq!(cited_paths(body), ["crates/kept.rs"]);
}

/// The index stays within its bound at every size, and an index that fits
/// its bound exactly is the one that bound gives.
#[test]
fn an_index_fits_its_bound_and_uses_it_to_the_byte() {
    let entries: Vec<MemoryEntry> = [
        ("reply-in-ptbr", FactKind::Feedback),
        ("secret-phrase", FactKind::User),
        ("lab-image", FactKind::Project),
        ("dashboards", FactKind::Reference),
        ("runbooks", FactKind::Reference),
    ]
    .into_iter()
    .map(|(name, kind)| entry(name, kind, "2026-10-01"))
    .collect();
    let full = render_index(&entries, usize::MAX);
    for bound in 150..=full.len() + 8 {
        let index = render_index(&entries, bound);
        assert!(
            index.len() <= bound,
            "{} bytes in {bound}: {index}",
            index.len()
        );
        if index.contains(NAMES_HEADING) {
            assert_eq!(render_index(&entries, index.len()), index, "at {bound}");
        }
    }
}

#[test]
fn a_description_cut_inside_a_character_is_cut_before_it() {
    let mut long = entry("long", FactKind::Feedback, "2026-10-01");
    long.description = format!("{}é and more", "a".repeat(LINE_DESCRIPTION - 1));
    let index = render_index(&[long], 1_000);
    let description = index.split_once("): ").unwrap().1;
    assert_eq!(
        description,
        format!("{}…", "a".repeat(LINE_DESCRIPTION - 1))
    );
}

#[test]
fn today_is_a_date_of_this_era() {
    let today = today();
    let parts: Vec<&str> = today.split('-').collect();
    assert_eq!(parts.len(), 3, "{today}");
    let year: i64 = parts[0].parse().unwrap();
    assert!((2026..2200).contains(&year), "{today}");
    assert_eq!((parts[1].len(), parts[2].len()), (2, 2), "{today}");
}

#[test]
fn dates_across_leap_days_centuries_and_year_ends_are_civil() {
    for (days, date) in [
        (-1, (1969, 12, 31)),
        (10_956, (1999, 12, 31)),
        (11_016, (2000, 2, 29)),
        (11_017, (2000, 3, 1)),
        (19_737, (2024, 1, 15)),
        (47_540, (2100, 2, 28)),
        (47_541, (2100, 3, 1)),
        (157_113, (2400, 2, 29)),
    ] {
        assert_eq!(civil_from_days(days), date, "{days}");
    }
}
