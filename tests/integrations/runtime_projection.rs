//! The runtime projection tree's lifecycle, end to end: a real integration
//! writes into it, and the sweep decides what still has a reason to exist.
//!
//! The unit tests in `uze-core` cover the sweep's rules against directories
//! a test made by hand. What only this level can show is that the rules and
//! the writer agree — that what `ClaudeIntegration` actually produces is
//! what `prune_projections` recognizes, and that the case the tree exists
//! for holds: an agent's checkout is destroyed, its projection goes, and the
//! repository it was cut from keeps its own.

use std::fs;
use std::path::{Path, PathBuf};

use uze_core::UzeHome;
use uze_core::harness_runtime::{self, RuntimeContext};
use uze_core::integration::{IntegrationPort, ProjectResourceRoute};
use uze_core::project_context::AgentsDirectoryResource;
use uze_integrations::claude::ClaudeIntegration;
use uze_integrations::registry::IntegrationRegistry;
use uze_testkit::temp::TestEnvironment;

/// A project carrying portable context, which is the only condition that
/// makes an integration project at all.
fn project_at(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join(".agents/skills/demo")).unwrap();
    fs::write(root.join(".agents/skills/demo/SKILL.md"), "canary\n").unwrap();
    fs::write(root.join("AGENTS.md"), "project instructions\n").unwrap();
    uze_platform::path::canonical(root).unwrap()
}

/// Drives the real vendor integration through the shim's own entry point,
/// and answers with the directory it told the harness to add.
fn project_into(home: &UzeHome, claude_home: &Path, cwd: &Path) -> PathBuf {
    let contribution = ClaudeIntegration::new(claude_home.to_path_buf(), home.clone())
        .runtime_contribution(&RuntimeContext { cwd, home });
    assert!(
        !contribution.is_passthrough(),
        "a project carrying context must be projected: {contribution:?}"
    );
    PathBuf::from(&contribution.extra_args[1])
}

#[test]
fn a_destroyed_checkout_loses_its_projection_and_its_repository_keeps_one() {
    let env = TestEnvironment::isolated();
    let home = UzeHome::at(&env.uze_home);
    let claude_home = env.root().join("claude-home");

    let primary = project_at(&env.root().join("repository"));
    let checkout = project_at(&env.root().join("repository/.worktrees/aueicn"));

    let primary_projection = project_into(&home, &claude_home, &primary);
    let checkout_projection = project_into(&home, &claude_home, &checkout);
    assert_ne!(
        primary_projection, checkout_projection,
        "a checkout is a project root of its own — sharing would hand one \
         branch's instructions to another"
    );
    for projection in [&primary_projection, &checkout_projection] {
        assert!(projection.join("CLAUDE.md").is_file());
        assert!(projection.join(".claude/skills").is_symlink());
    }

    // Delivery is done and UZE removes the agent's checkout.
    fs::remove_dir_all(&checkout).unwrap();

    let pruned = harness_runtime::prune_projections(&home);
    assert_eq!(
        pruned,
        vec![harness_runtime::project_id_for(&checkout)],
        "exactly the dead checkout's projection is swept"
    );
    assert!(!checkout_projection.exists());
    assert!(
        primary_projection.join("CLAUDE.md").is_file(),
        "the repository the checkout was cut from is untouched"
    );

    // And the sweep says nothing the second time: it is a statement about
    // the tree, not a queue of work.
    assert!(harness_runtime::prune_projections(&home).is_empty());
}

#[test]
fn a_swept_projection_is_rebuilt_by_the_next_launch() {
    let env = TestEnvironment::isolated();
    let home = UzeHome::at(&env.uze_home);
    let claude_home = env.root().join("claude-home");
    let project = project_at(&env.root().join("project"));

    let projection = project_into(&home, &claude_home, &project);
    fs::remove_dir_all(home.runtime_project_dir(&harness_runtime::project_id_for(&project)))
        .unwrap();

    // Nothing was lost that mattered: a projection is derived, so the
    // sweep can afford to be wrong about one.
    assert_eq!(project_into(&home, &claude_home, &project), projection);
    assert_eq!(
        fs::read_to_string(projection.join("CLAUDE.md")).unwrap(),
        format!("@{}\n", project.join("AGENTS.md").display())
    );
    assert!(projection.join(".claude/skills").is_symlink());
}

/// The projection tree is UZE's own, but the project it points at is not:
/// a sweep must never reach through the symlink it planted.
#[test]
fn sweeping_a_dead_projection_never_touches_the_project_it_pointed_at() {
    let env = TestEnvironment::isolated();
    let home = UzeHome::at(&env.uze_home);
    let claude_home = env.root().join("claude-home");
    let project = project_at(&env.root().join("project"));

    project_into(&home, &claude_home, &project);
    // The marker is what identifies a project directory; without one the
    // sweep collects it, symlinks into a live project and all.
    let project_dir = home.runtime_project_dir(&harness_runtime::project_id_for(&project));
    fs::remove_file(project_dir.join(harness_runtime::PROJECTION_MARKER)).unwrap();

    assert_eq!(harness_runtime::prune_projections(&home).len(), 1);
    assert!(!project_dir.exists());
    assert!(
        project.join(".agents/skills/demo/SKILL.md").is_file(),
        "the project's own Skills must survive their projection being swept"
    );
}

/// Every path under `root` with its bytes (a link as its target), so two
/// listings compare equal only when nothing was added, removed or changed.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_symlink() {
                let target = fs::read_link(&path).unwrap();
                entries.push((relative, target.into_os_string().into_encoded_bytes()));
            } else if path.is_dir() {
                pending.push(path);
            } else {
                entries.push((relative, fs::read(&path).unwrap()));
            }
        }
    }
    entries.sort();
    entries
}

/// A project's `.agents/` is the project's own. Each kind is answered on its
/// own: a harness that reads it natively is left alone for it, one that
/// does not is handed it through its runtime projection, a harness offering
/// neither is handed nothing, and in no case does the checkout gain a byte.
#[test]
fn a_project_agents_directory_reaches_every_harness_without_a_write_into_the_checkout() {
    let env = TestEnvironment::isolated();
    let home = UzeHome::at(&env.uze_home);
    let project = project_at(&env.root().join("project"));
    fs::create_dir_all(project.join(".agents/skills/demo/references")).unwrap();
    fs::write(
        project.join(".agents/skills/demo/references/notes.md"),
        "notes\n",
    )
    .unwrap();
    fs::create_dir_all(project.join(".agents/agents")).unwrap();
    let reviewer = "---\nname: house-reviewer\ndescription: Reviews the house\n---\nReview it.\n";
    fs::write(project.join(".agents/agents/reviewer.md"), reviewer).unwrap();
    let before = tree(&project);

    let registry = IntegrationRegistry::isolated(&env.root().join("harnesses"), &home);
    for integration in registry.iter() {
        let contribution = integration.runtime_contribution(&RuntimeContext {
            cwd: &project,
            home: &home,
        });
        let projected: Vec<_> = AgentsDirectoryResource::ALL
            .into_iter()
            .filter(|kind| {
                integration.project_resource_route(*kind) == ProjectResourceRoute::RuntimeProjection
            })
            .collect();
        if projected.is_empty() {
            assert!(
                contribution.is_passthrough(),
                "{} is handed no part of .agents/ and must be left alone: {contribution:?}",
                integration.id()
            );
            continue;
        }
        assert!(
            !contribution.is_passthrough(),
            "{} must be handed {projected:?}: {contribution:?}",
            integration.id()
        );
        for kind in projected {
            assert_projected(integration.id(), kind, &contribution, &home, reviewer);
        }
    }

    assert_eq!(tree(&project), before, "the checkout must not change");
}

/// What a projected kind looks like in the one shape each launcher uses:
/// an `--add-dir` target mirroring `.agents/`, or a `-c` configuration
/// layer declaring the project's agents as roles.
fn assert_projected(
    harness: &str,
    kind: AgentsDirectoryResource,
    contribution: &uze_core::harness_runtime::HarnessRuntimeContribution,
    home: &UzeHome,
    reviewer: &str,
) {
    let flag = contribution.extra_args[0].to_string_lossy();
    let value = contribution.extra_args[1].to_string_lossy();
    match (flag.as_ref(), kind) {
        ("--add-dir", AgentsDirectoryResource::Skills) => {
            let projection = PathBuf::from(value.as_ref());
            assert!(projection.starts_with(home.runtime_dir()));
            let skills = fs::read_dir(projection.join(".claude/skills"))
                .unwrap()
                .count();
            assert_eq!(skills, 1, "the project's one Skill is discoverable");
            assert_eq!(
                fs::read_to_string(projection.join(".claude/skills/demo/references/notes.md"))
                    .unwrap(),
                "notes\n",
                "a Skill keeps its supporting files"
            );
        }
        ("--add-dir", AgentsDirectoryResource::Agents) => {
            let projection = PathBuf::from(value.as_ref());
            assert_eq!(
                fs::read_to_string(projection.join(".claude/agents/reviewer.md")).unwrap(),
                reviewer,
                "linked as written, so the harness names it `house-reviewer` itself"
            );
        }
        ("-c", AgentsDirectoryResource::Agents) => {
            assert!(
                value.starts_with("agents={")
                    && value.contains("\"house-reviewer\" = { description = \"Reviews the house\""),
                "the agent is a role under its logical name: {value}"
            );
            let role = value
                .split_once("config_file = \"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(path, _)| PathBuf::from(path))
                .expect("the role names its configuration file");
            assert!(role.starts_with(home.runtime_dir()));
            assert_eq!(
                fs::read_to_string(role).unwrap(),
                "developer_instructions = \"Review it.\"\n"
            );
        }
        other => {
            panic!("{harness} projects {kind:?} in a shape this test does not know: {other:?}")
        }
    }
}
