use std::{fs, path::Path};

use crate::{
    delivery::{exposure::ManagedArtifact, integration::AttachmentReceipt},
    machine::inventory::{
        Claim, Definition, FoundResource, HarnessInventory, Origin, ResourceKind, Runs,
        SessionCounts, Status, assemble,
    },
};

fn skill(root: &Path, relative: &str, description: &str) -> FoundResource {
    let path = root.join(relative).join("SKILL.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let name = relative.rsplit('/').next().unwrap();
    fs::write(
        &path,
        format!("---\nname: {name}\ndescription: {description}\n---\n\nThe body a model reads when it uses the skill, at some length.\n"),
    )
    .unwrap();
    FoundResource::new(ResourceKind::Skill, name, Definition::Markdown(path))
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
    let receipt = tree_receipt("a", "git@uze", &root.join("a/skills/git:commit"));
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
    let receipt = tree_receipt("b", "git@uze", &root.join("a/skills/git:commit"));
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
        receipts.push(tree_receipt(id, "git@uze", &root.join(&relative)));
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
fn sessions_are_counted_by_their_last_activity() {
    let now = std::time::SystemTime::now();
    let day = std::time::Duration::from_secs(86_400);
    let counts =
        SessionCounts::from_activity([now, now - day * 10, now - day * 60, now - day * 120], now);
    assert_eq!((counts.days_7, counts.days_30, counts.days_90), (1, 2, 3));
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
