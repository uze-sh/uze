use std::{fs, path::Path};

use crate::{
    delivery::{exposure::ManagedArtifact, integration::AttachmentReceipt},
    machine::inventory::{
        Claim, Definition, FoundResource, HarnessInventory, Origin, ResourceKind, Runs,
        SessionCounts, Status, assemble,
    },
};

/// A skill named by `relative`'s last part, kept in a directory every
/// platform can name: a plugin's skill is `git:commit`, and Windows refuses
/// a `:` in a file name.
fn skill(root: &Path, relative: &str, description: &str) -> FoundResource {
    let path = skill_dir(root, relative).join("SKILL.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let name = relative.rsplit('/').next().unwrap();
    fs::write(
        &path,
        format!("---\nname: {name}\ndescription: {description}\n---\n\nThe body a model reads when it uses the skill, at some length.\n"),
    )
    .unwrap();
    FoundResource::new(ResourceKind::Skill, name, Definition::Markdown(path))
}

fn skill_dir(root: &Path, relative: &str) -> std::path::PathBuf {
    root.join(relative.replace(':', "-"))
}

fn harness(id: &str, found: Vec<FoundResource>) -> HarnessInventory {
    HarnessInventory {
        id: id.to_owned(),
        name: id.to_owned(),
        found: Some(found),
        sessions: Some(SessionCounts::default()),
    }
}

fn tree_receipt(integration: &str, package: &str, path: &Path) -> AttachmentReceipt {
    AttachmentReceipt {
        package_id: package.to_owned(),
        resource_identity: None,
        integration: integration.to_owned(),
        artifact: ManagedArtifact::GeneratedTree {
            path: path.to_path_buf(),
            digest: String::new(),
        },
    }
}

#[test]
fn a_skill_a_receipt_put_there_is_uzes() {
    let root = uze_testkit::temp::TempDir::new("inventory-owned");
    let found = skill(root.path(), "a/skills/git:commit", "Write a commit.");
    let receipt = tree_receipt(
        "a",
        "git@uze",
        &skill_dir(root.path(), "a/skills/git:commit"),
    );
    let overview = assemble(vec![harness("a", vec![found])], &[receipt]);
    assert_eq!(
        overview.resources[0].origin,
        Origin::Uze {
            package: "git@uze".into()
        }
    );
}

#[test]
fn a_path_that_only_looks_like_uzes_is_not() {
    let root = uze_testkit::temp::TempDir::new("inventory-lookalike");
    let found = skill(root.path(), "a/skills/uze:init", "Looks like uze's.");
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    assert_eq!(overview.resources[0].origin, Origin::You);
}

#[test]
fn a_receipt_of_another_harness_owns_nothing_here() {
    let root = uze_testkit::temp::TempDir::new("inventory-other-harness");
    let found = skill(root.path(), "a/skills/git:commit", "Write a commit.");
    let receipt = tree_receipt(
        "b",
        "git@uze",
        &skill_dir(root.path(), "a/skills/git:commit"),
    );
    let overview = assemble(vec![harness("a", vec![found])], &[receipt]);
    assert_eq!(overview.resources[0].origin, Origin::You);
}

#[test]
fn a_plugin_uze_published_is_uzes_by_its_owned_receipt() {
    let root = uze_testkit::temp::TempDir::new("inventory-plugin");
    let found = skill(root.path(), "a/plugins/code/polish", "Polish code.")
        .from_plugin("code")
        .claimed(Claim::Owned {
            kind: "plugin-generated".into(),
            selector: "code@uze-store".into(),
        });
    let receipt = AttachmentReceipt {
        package_id: "code@uze".into(),
        resource_identity: None,
        integration: "a".into(),
        artifact: ManagedArtifact::IntegrationOwned {
            kind: "plugin-generated".into(),
            selector: "code@uze-store".into(),
            detail: Default::default(),
        },
    };
    let overview = assemble(vec![harness("a", vec![found])], &[receipt]);
    assert_eq!(
        overview.resources[0].origin,
        Origin::Uze {
            package: "code@uze".into()
        }
    );
}

#[test]
fn a_plugin_the_harness_installed_names_that_plugin() {
    let root = uze_testkit::temp::TempDir::new("inventory-harness-plugin");
    let found = skill(root.path(), "a/plugins/vercel/nextjs", "Next.js.").from_plugin("vercel");
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    assert_eq!(
        overview.resources[0].origin,
        Origin::Plugin {
            name: "vercel".into()
        }
    );
}

#[test]
fn a_skill_the_harness_ships_is_built_in() {
    let root = uze_testkit::temp::TempDir::new("inventory-built-in");
    let found = skill(root.path(), "a/builtin/review", "Review.").built_in();
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    assert_eq!(overview.resources[0].origin, Origin::BuiltIn);
}

#[test]
fn one_uze_skill_on_three_harnesses_is_one_row() {
    let root = uze_testkit::temp::TempDir::new("inventory-merge");
    let mut harnesses = Vec::new();
    let mut receipts = Vec::new();
    for (id, description) in [
        ("a", "Commit."),
        ("b", "Commit, for b."),
        ("c", "Commit, for c."),
    ] {
        let relative = format!("{id}/skills/git:commit");
        harnesses.push(harness(
            id,
            vec![skill(root.path(), &relative, description)],
        ));
        receipts.push(tree_receipt(
            id,
            "git@uze",
            &skill_dir(root.path(), &relative),
        ));
    }
    let overview = assemble(harnesses, &receipts);
    assert_eq!(overview.resources.len(), 1);
    let row = &overview.resources[0];
    assert_eq!(row.status, Status::Ok);
    let reached: Vec<_> = row.locations.iter().map(|l| l.harness.as_str()).collect();
    assert_eq!(reached, ["a", "b", "c"]);
}

#[test]
fn identical_copies_by_hand_merge_and_different_ones_are_duplicates() {
    let root = uze_testkit::temp::TempDir::new("inventory-duplicates");
    let same_a = skill(root.path(), "a/skills/review", "Review code.");
    let same_b = skill(root.path(), "b/skills/review", "Review code.");
    let merged = assemble(
        vec![harness("a", vec![same_a]), harness("b", vec![same_b])],
        &[],
    );
    assert_eq!(merged.resources.len(), 1);
    assert_eq!(merged.resources[0].status, Status::Ok);

    let other_root = uze_testkit::temp::TempDir::new("inventory-duplicates-differ");
    let a = skill(other_root.path(), "a/skills/review", "Review code.");
    let b = skill(
        other_root.path(),
        "b/skills/review",
        "Review code, another way.",
    );
    let differ = assemble(vec![harness("a", vec![a]), harness("b", vec![b])], &[]);
    assert_eq!(differ.resources.len(), 2);
    for row in &differ.resources {
        let Status::Duplicate { others } = &row.status else {
            panic!("{row:?} is not a duplicate");
        };
        assert_eq!(others.len(), 1);
        assert_ne!(others[0].harness, row.locations[0].harness);
    }
    assert_ne!(differ.resources[0].id, differ.resources[1].id);
}

#[test]
fn a_frontmatter_that_does_not_parse_is_broken() {
    let root = uze_testkit::temp::TempDir::new("inventory-frontmatter");
    let path = root.join("a/skills/bad/SKILL.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "---\nname: [unclosed\n---\nbody\n").unwrap();
    let found = FoundResource::new(ResourceKind::Skill, "bad", Definition::Markdown(path));
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    assert_eq!(
        overview.resources[0].status,
        Status::Broken {
            reason: "invalid frontmatter".into()
        }
    );
}

#[test]
fn a_hook_whose_script_is_gone_is_broken_and_first() {
    let root = uze_testkit::temp::TempDir::new("inventory-hook");
    let settings = root.join("a/settings.json");
    let fine = FoundResource::new(
        ResourceKind::Hook,
        "format",
        Definition::Entry(settings.clone()),
    )
    .running(Runs::Shell(format!(
        "{} --version",
        std::env::current_exe().unwrap().display()
    )));
    let gone = FoundResource::new(ResourceKind::Hook, "guard", Definition::Entry(settings))
        .running(Runs::Shell("bash /nowhere/guard.sh".into()));
    let overview = assemble(vec![harness("a", vec![fine, gone])], &[]);
    assert_eq!(overview.resources[0].name, "guard");
    assert!(matches!(
        overview.resources[0].status,
        Status::Broken { .. }
    ));
    assert_eq!(overview.resources[1].status, Status::Ok);
}

#[test]
fn a_skills_description_is_listed_and_its_body_loads_when_used() {
    let root = uze_testkit::temp::TempDir::new("inventory-tokens");
    let found = skill(
        root.path(),
        "a/skills/review",
        "Review code before it is merged.",
    );
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    let row = &overview.resources[0];
    assert!(row.per_session.unwrap() > 0);
    assert!(row.when_used.unwrap() > 0);
    assert_eq!(
        row.description.as_deref(),
        Some("Review code before it is merged.")
    );
}

#[test]
fn an_entry_in_a_config_file_has_no_figure() {
    let root = uze_testkit::temp::TempDir::new("inventory-entry");
    let found = FoundResource::new(
        ResourceKind::Mcp,
        "github",
        Definition::Entry(root.join("config.toml")),
    )
    .running(Runs::Url("https://example.invalid/mcp".into()));
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    let row = &overview.resources[0];
    assert_eq!((row.per_session, row.when_used), (None, None));
    assert_eq!(row.status, Status::Ok);
    assert!(!row.checked, "a server reached by URL is never judged");
}

#[test]
fn a_rule_is_listed_whole() {
    let root = uze_testkit::temp::TempDir::new("inventory-rule");
    let path = root.join("CLAUDE.md");
    fs::write(&path, "Answer in Portuguese.\n").unwrap();
    let found = FoundResource::new(ResourceKind::Rule, "CLAUDE.md", Definition::Whole(path));
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    assert_eq!(overview.resources[0].when_used, None);
    assert!(overview.resources[0].per_session.unwrap() > 0);
}

#[test]
fn within_a_kind_the_heaviest_comes_first() {
    let root = uze_testkit::temp::TempDir::new("inventory-order");
    let light = skill(root.path(), "a/skills/light", "Short.");
    let heavy = skill(
        root.path(),
        "a/skills/heavy",
        "A much longer description that costs many more tokens in every single session.",
    );
    let overview = assemble(vec![harness("a", vec![light, heavy])], &[]);
    let names: Vec<_> = overview.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["heavy", "light"]);
}

#[test]
fn a_harness_that_cannot_list_is_reported_as_such() {
    let overview = assemble(
        vec![HarnessInventory {
            id: "a".into(),
            name: "A".into(),
            found: None,
            sessions: None,
        }],
        &[],
    );
    assert!(!overview.harnesses[0].inventory_known);
    assert!(overview.harnesses[0].sessions.is_none());
    assert!(overview.resources.is_empty());
}

#[test]
fn every_session_counts_and_the_oldest_says_how_far_back_they_reach() {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000 * 86_400);
    let day = std::time::Duration::from_secs(86_400);
    let counts = SessionCounts::from_activity([now, now - day * 10, now - day * 120]);
    assert_eq!(counts.total, 3);
    assert_eq!(counts.oldest, Some(880 * 86_400));
}

#[test]
fn what_the_harness_proves_wrong_is_broken() {
    let root = uze_testkit::temp::TempDir::new("inventory-problem");
    let found = skill(root.path(), "a/plugins/gone/x", "X.")
        .from_plugin("gone")
        .with_problem("plugin directory missing");
    let overview = assemble(vec![harness("a", vec![found])], &[]);
    assert_eq!(
        overview.resources[0].status,
        Status::Broken {
            reason: "plugin directory missing".into()
        }
    );
}

#[test]
fn broken_then_duplicates_then_the_rest() {
    let root = uze_testkit::temp::TempDir::new("inventory-status-order");
    let heavy_ok = skill(
        root.path(),
        "a/skills/heavy",
        "A long description that would come first on weight alone, every session.",
    );
    let dup_a = skill(root.path(), "a/skills/twin", "One.");
    let dup_b = skill(root.path(), "b/skills/twin", "Two.");
    let broken = skill(root.path(), "a/skills/broken", "B.").with_problem("cannot be read");
    let overview = assemble(
        vec![
            harness("a", vec![heavy_ok, dup_a, broken]),
            harness("b", vec![dup_b]),
        ],
        &[],
    );
    let names: Vec<_> = overview.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["broken", "twin", "twin", "heavy"]);
}

#[test]
fn two_entries_of_one_file_are_not_duplicates() {
    let root = uze_testkit::temp::TempDir::new("inventory-neighbours");
    let settings = root.join("settings.json");
    let on_start = FoundResource::new(
        ResourceKind::Hook,
        "engine",
        Definition::Entry(settings.clone()),
    )
    .running(Runs::Url("https://example.invalid/a".into()));
    let on_tool = FoundResource::new(ResourceKind::Hook, "engine", Definition::Entry(settings))
        .running(Runs::Url("https://example.invalid/b".into()));
    let overview = assemble(vec![harness("a", vec![on_start, on_tool])], &[]);
    assert!(
        overview
            .resources
            .iter()
            .all(|row| row.status == Status::Ok)
    );
}

/// The budget a collection keeps on a machine carrying 500 resources:
/// reading every definition, measuring it and judging it. Generous in a
/// debug build, which is what `cargo test` runs; the release figure is the
/// one the design states.
#[test]
fn five_hundred_resources_are_assembled_within_budget() {
    let root = uze_testkit::temp::TempDir::new("inventory-budget");
    let found: Vec<FoundResource> = (0..500)
        .map(|n| {
            skill(
                root.path(),
                &format!("a/skills/skill-{n}"),
                "Review code before it is merged, in the project's own convention, and say why.",
            )
        })
        .collect();
    let budget = if cfg!(debug_assertions) {
        std::time::Duration::from_millis(3_000)
    } else {
        std::time::Duration::from_millis(300)
    };
    let started = std::time::Instant::now();
    let overview = assemble(vec![harness("a", found)], &[]);
    let took = started.elapsed();
    assert_eq!(overview.resources.len(), 500);
    assert!(took < budget, "took {took:?}, budget {budget:?}");
}

fn rule(root: &Path, relative: &str, content: &[u8]) -> FoundResource {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
    FoundResource::new(ResourceKind::Rule, "AGENTS.md", Definition::Whole(path))
}

fn listed(root: &Path, per_session: &str, when_used: Option<&str>) -> FoundResource {
    FoundResource::new(
        ResourceKind::Agent,
        "reviewer",
        Definition::Listed {
            file: root.join("agents.toml"),
            per_session: per_session.to_owned(),
            when_used: when_used.map(str::to_owned),
        },
    )
}

/// How many rows two harnesses' copies of one resource make: one when they
/// say the same thing, two when they differ.
fn rows_of(a: FoundResource, b: FoundResource) -> usize {
    assemble(vec![harness("a", vec![a]), harness("b", vec![b])], &[])
        .resources
        .len()
}

#[test]
fn a_listed_definition_is_measured_by_the_text_its_integration_hands_over() {
    let root = uze_testkit::temp::TempDir::new("inventory-listed");
    let overview = assemble(
        vec![harness(
            "a",
            vec![listed(
                root.path(),
                "Reviews code.",
                Some("Read the diff, then say what is wrong with it."),
            )],
        )],
        &[],
    );
    let row = &overview.resources[0];
    assert!(row.per_session.unwrap() > 0);
    assert!(row.when_used.unwrap() > 0);
    assert!(row.checked, "a definition read is a fact checked");
}

#[test]
fn copies_by_hand_are_told_apart_by_what_they_say_however_they_are_defined() {
    let root = uze_testkit::temp::TempDir::new("inventory-compared");
    let a = root.join("a");
    let b = root.join("b");
    assert_eq!(
        rows_of(
            listed(&a, "Reviews code.", None),
            listed(&b, "Reviews code.", None)
        ),
        1
    );
    assert_eq!(
        rows_of(
            listed(&a, "Reviews code.", None),
            listed(&b, "Reviews prose.", None)
        ),
        2
    );
    assert_eq!(
        rows_of(
            rule(root.path(), "a/AGENTS.md", b"Answer in English.\n"),
            rule(root.path(), "b/AGENTS.md", b"Answer in Portuguese.\n")
        ),
        2
    );
    let bad = |relative: &str, text: &str| {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        FoundResource::new(ResourceKind::Skill, "bad", Definition::Markdown(path))
    };
    assert_eq!(
        rows_of(
            bad("a/skills/bad/SKILL.md", "---\nname: [one\n---\n"),
            bad("b/skills/bad/SKILL.md", "---\nname: [two\n---\n")
        ),
        2
    );
}

#[test]
fn a_definition_of_some_kilobytes_is_read_whole() {
    let root = uze_testkit::temp::TempDir::new("inventory-kilobytes");
    let a = vec![b'a'; 10_000];
    let b = vec![b'b'; 10_000];
    assert_eq!(
        rows_of(
            rule(root.path(), "a/AGENTS.md", &a),
            rule(root.path(), "b/AGENTS.md", &b)
        ),
        2,
        "two files of one size that say different things are not one"
    );
}

#[test]
fn a_definition_at_the_read_limit_is_read_whole_and_one_past_it_is_measured() {
    const MEBIBYTE: usize = 1024 * 1024;
    let root = uze_testkit::temp::TempDir::new("inventory-read-limit");
    assert_eq!(
        rows_of(
            rule(root.path(), "a/AGENTS.md", &vec![b'a'; MEBIBYTE]),
            rule(root.path(), "b/AGENTS.md", &vec![b'b'; MEBIBYTE])
        ),
        2
    );
    let past = |relative: &str, size: usize| rule(root.path(), relative, &vec![b'x'; size]);
    assert_eq!(
        rows_of(
            past("c/AGENTS.md", MEBIBYTE + 1),
            past("d/AGENTS.md", MEBIBYTE + 2)
        ),
        2,
        "too large to read, they are still told apart by their size"
    );
    let overview = assemble(
        vec![harness("a", vec![past("e/AGENTS.md", 2 * MEBIBYTE)])],
        &[],
    );
    assert!(overview.resources[0].per_session.unwrap() > 0);

    let skill_path = root.join("f/skills/huge/SKILL.md");
    fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
    fs::write(&skill_path, vec![b'x'; 2 * MEBIBYTE]).unwrap();
    let huge = |path: &Path| {
        FoundResource::new(
            ResourceKind::Skill,
            "huge",
            Definition::Markdown(path.to_path_buf()),
        )
    };
    let other_path = root.join("g/skills/huge/SKILL.md");
    fs::create_dir_all(other_path.parent().unwrap()).unwrap();
    fs::write(&other_path, vec![b'x'; 2 * MEBIBYTE + 1]).unwrap();
    let overview = assemble(
        vec![
            harness("a", vec![huge(&skill_path)]),
            harness("b", vec![huge(&other_path)]),
        ],
        &[],
    );
    assert_eq!(overview.resources.len(), 2);
    assert!(overview.resources[0].when_used.unwrap() > 0);
}

#[test]
fn a_definition_that_cannot_be_read_is_broken() {
    let root = uze_testkit::temp::TempDir::new("inventory-unreadable");
    let whole = FoundResource::new(
        ResourceKind::Rule,
        "AGENTS.md",
        Definition::Whole(root.join("missing/AGENTS.md")),
    );
    let not_text = root.join("a/skills/binary/SKILL.md");
    fs::create_dir_all(not_text.parent().unwrap()).unwrap();
    fs::write(&not_text, [0xff, 0xfe, 0x00, 0x80]).unwrap();
    let markdown = FoundResource::new(
        ResourceKind::Skill,
        "binary",
        Definition::Markdown(not_text),
    );
    let overview = assemble(vec![harness("a", vec![whole, markdown])], &[]);
    for row in &overview.resources {
        assert_eq!(
            row.status,
            Status::Broken {
                reason: "cannot be read".into()
            },
            "{}",
            row.name
        );
    }
}

#[test]
fn a_receipt_owns_only_what_lies_under_what_it_placed() {
    let root = uze_testkit::temp::TempDir::new("inventory-receipt-elsewhere");
    let found = skill(root.path(), "a/skills/review", "Review.");
    let receipt = tree_receipt(
        "a",
        "git@uze",
        &skill_dir(root.path(), "a/skills/git:commit"),
    );
    let overview = assemble(vec![harness("a", vec![found])], &[receipt]);
    assert_eq!(overview.resources[0].origin, Origin::You);
}

#[test]
fn a_hook_running_the_wrapper_uze_generated_is_uzes() {
    let root = uze_testkit::temp::TempDir::new("inventory-hook-wrapper");
    let wrapper = root.join("wrappers").join("guard.sh");
    let found = FoundResource::new(
        ResourceKind::Hook,
        "guard",
        Definition::Entry(root.join("settings.json")),
    )
    .running(Runs::Shell(format!("sh {}", wrapper.display())));
    let receipt = AttachmentReceipt {
        package_id: "guard@uze".into(),
        resource_identity: None,
        integration: "a".into(),
        artifact: ManagedArtifact::HookConfigEntry {
            config_file: root.join("settings.json"),
            entry_name: "another-name".into(),
            event: crate::capability::hook::HookEvent::PreToolUse,
            expected: String::new(),
            wrapper,
        },
    };
    let overview = assemble(vec![harness("a", vec![found])], &[receipt]);
    assert_eq!(
        overview.resources[0].origin,
        Origin::Uze {
            package: "guard@uze".into()
        }
    );
}

#[test]
fn copies_of_one_name_in_one_harness_each_have_their_own_id() {
    let root = uze_testkit::temp::TempDir::new("inventory-same-name");
    let copies: Vec<_> = ["first", "second", "third"]
        .iter()
        .map(|place| skill(root.path(), &format!("a/{place}/review"), place))
        .collect();
    let alone = assemble(vec![harness("a", vec![copies[0].clone()])], &[]).resources[0]
        .id
        .clone();
    let overview = assemble(vec![harness("a", copies)], &[]);
    let ids: std::collections::BTreeSet<_> = overview.resources.iter().map(|r| &r.id).collect();
    assert_eq!(ids.len(), 3, "{ids:?}");
    assert!(ids.contains(&alone), "the first keeps the id it has alone");
    assert!(
        !alone.contains('#'),
        "only a later copy is told apart by its place: {alone}"
    );
}

#[test]
fn an_id_names_the_package_for_what_uze_delivered_and_the_harness_otherwise() {
    let root = uze_testkit::temp::TempDir::new("inventory-ids");
    let id_of = |harness_id: &str, relative: &str, receipts: bool| {
        let found = skill(root.path(), relative, "Commit.");
        let receipt = tree_receipt(harness_id, "git@uze", &skill_dir(root.path(), relative));
        let receipts = if receipts { vec![receipt] } else { vec![] };
        assemble(vec![harness(harness_id, vec![found])], &receipts).resources[0]
            .id
            .clone()
    };
    assert_eq!(
        id_of("a", "a/skills/commit", true),
        id_of("b", "b/elsewhere/commit", true),
        "a reinstall elsewhere, or on another harness, keeps the id"
    );
    assert_ne!(
        id_of("a", "a/skills/commit", false),
        id_of("b", "b/skills/commit", false)
    );
    assert_eq!(
        id_of("a", "a/skills/commit", false),
        id_of("a", "a/moved/commit", false)
    );
    let review = skill(root.path(), "a/skills/review", "Review.");
    assert_ne!(
        assemble(vec![harness("a", vec![review])], &[]).resources[0].id,
        id_of("a", "a/skills/commit", false)
    );
}

#[test]
fn what_is_checked_is_a_definition_read_or_a_command_looked_for() {
    let root = uze_testkit::temp::TempDir::new("inventory-checked");
    let settings = root.join("settings.json");
    let skill = skill(root.path(), "a/skills/review", "Review.");
    let command = FoundResource::new(
        ResourceKind::Hook,
        "format",
        Definition::Entry(settings.clone()),
    )
    .running(Runs::Program(
        std::env::current_exe().unwrap().display().to_string(),
    ));
    let proven_wrong = FoundResource::new(ResourceKind::Mcp, "gone", Definition::Entry(settings))
        .with_problem("its plugin is missing");
    let overview = assemble(vec![harness("a", vec![skill, command, proven_wrong])], &[]);
    for row in &overview.resources {
        assert!(row.checked, "{}", row.name);
    }
}

#[test]
fn a_row_reaches_the_harnesses_that_carry_it_and_no_other() {
    let root = uze_testkit::temp::TempDir::new("inventory-reaches");
    let a = skill(root.path(), "a/skills/review", "Review code.");
    let b = skill(root.path(), "b/skills/review", "Review code.");
    let overview = assemble(vec![harness("a", vec![a]), harness("b", vec![b])], &[]);
    let row = &overview.resources[0];
    assert!(row.reaches("a") && row.reaches("b"));
    assert!(!row.reaches("c"));
}
