//! What a project declares about the workspace, read from its own sections
//! of `agents.yaml`.
//!
//! The file belongs to `uze-core`'s manifest module, which carries these
//! sections unread. Reading them here is what keeps a mistake in them the
//! workspace's to report: a package command never parses them, so it never
//! fails over them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    Result, UzeError,
    manifest::{self, Section},
    worktree::{CompletionBehavior, WorktreePolicy},
};

/// The isolation policy the project declares, or `None` when it declares
/// none. Validated as the workspace uses it: a linked file must be one the
/// repository ignores, and a cap of zero is refused.
pub fn declared(root: &Path) -> Result<Option<WorktreePolicy>> {
    let Some(policy) = manifest::load_section::<WorktreePolicy>(root, Section::Worktrees)? else {
        return Ok(None);
    };
    let path = manifest::manifest_path_for(root);
    validate(&policy, &path)?;
    reject_unignored_links(root, &path, &policy)?;
    Ok(Some(policy))
}

/// The policy in force for a project: what the manifest declares, or the
/// built-in default. No machine-scoped setting participates — an
/// undeclared policy resolves the same way on every machine, so the text
/// projected into `AGENTS.md` does not depend on who ran the command.
pub fn policy(root: &Path) -> Result<WorktreePolicy> {
    Ok(declared(root)?.unwrap_or_default())
}

/// Declares the completion behavior, creating the manifest when the project
/// has none, and reports whether it had to — a caller showing a person the
/// consequence of their click says "this creates a tracked file" before it
/// happens rather than after.
pub fn set_completion(root: &Path, behavior: CompletionBehavior) -> Result<bool> {
    let created =
        manifest::set_scalar(root, Section::Worktrees, "completion", behavior.abi_name())?;
    // Re-read through the typed path: a write the schema would reject is a
    // bug here, not on a later command.
    declared(root)?;
    Ok(created)
}

/// Where the project keeps what describes it — its architecture diagrams,
/// today. A directory and nothing else: what each file in it *is* is read
/// off the file, so there is no second place for that to go stale in.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredArtifacts {
    /// Relative to the project root, and inside it.
    pub path: PathBuf,
}

/// The artifacts directory the project declares, if any.
pub fn artifacts(root: &Path) -> Result<Option<DeclaredArtifacts>> {
    manifest::load_section(root, Section::Artifacts)
}

fn validate(policy: &WorktreePolicy, path: &Path) -> Result<()> {
    let malformed = |reason: String| UzeError::MalformedManifest {
        path: path.to_path_buf(),
        reason,
    };
    if policy.slots == Some(0) {
        // Zero is honored, and honoring it refuses every checkout: the cap
        // is compared with `>=`, so the first task fails with "cap
        // reached" and nothing says the manifest is why.
        return Err(malformed(
            "`worktrees.slots` is 0, which would refuse every checkout there is; leave it \
             undeclared for no cap at all, or give it at least 1"
                .to_owned(),
        ));
    }
    if let Some((link, why)) = policy.misplaced_links().into_iter().next() {
        return Err(malformed(format!(
            "`worktrees.link` names `{}`, which is {why}; a link is a relative path inside the \
             repository",
            link.display()
        )));
    }
    Ok(())
}

/// A linked file must be ignored by the repository: a tracked file linked
/// into a checkout would land in the agent's commits as a symlink. Asked of
/// Git only when a manifest declares links, so one that declares none costs
/// no subprocess to read.
fn reject_unignored_links(root: &Path, path: &Path, policy: &WorktreePolicy) -> Result<()> {
    for link in &policy.link {
        let spelled = link.to_string_lossy();
        let answer =
            uze_git::read(root, &["check-ignore", "--quiet", "--", &spelled]).map_err(|error| {
                UzeError::MalformedManifest {
                    path: path.to_path_buf(),
                    reason: format!("`worktrees.link` names `{spelled}`, but {error}"),
                }
            })?;
        match answer.code {
            Some(0) => {}
            Some(1) => {
                return Err(UzeError::MalformedManifest {
                    path: path.to_path_buf(),
                    reason: format!(
                        "`worktrees.link` names `{spelled}`, which the repository does not \
                         ignore; a linked file must be ignored, or it would be committed as a \
                         symlink from an agent's checkout"
                    ),
                });
            }
            _ => {
                return Err(UzeError::MalformedManifest {
                    path: path.to_path_buf(),
                    reason: format!(
                        "`worktrees.link` names `{spelled}`, but this directory is not a Git \
                         repository that could ignore it"
                    ),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs};

    use serde::Serialize;
    use uze_core::manifest::DeclaredMarketplace;

    use super::*;
    use crate::{
        manifest::{MANIFEST_FILE_NAME, SCAFFOLD, ensure_exists, manifest_path_for},
        worktree::AgentPlacementDefault,
    };

    /// The sections a test reads back: the workspace's policy as the
    /// workspace parses it, and the package manager's marketplaces beside it.
    #[derive(Debug)]
    struct Parsed {
        worktrees: Option<WorktreePolicy>,
        marketplaces: BTreeMap<String, DeclaredMarketplace>,
    }

    fn parsed(text: &str) -> Result<Parsed> {
        let path = Path::new("/p/agents.yaml");
        let manifest = manifest::parse(text, path)?;
        let worktrees = manifest.section::<WorktreePolicy>(Section::Worktrees, path)?;
        if let Some(policy) = &worktrees {
            validate(policy, path)?;
        }
        Ok(Parsed {
            worktrees,
            marketplaces: manifest.marketplaces,
        })
    }

    fn load(root: &Path) -> Result<Option<Parsed>> {
        let worktrees = declared(root)?;
        Ok(manifest::load(root)?.map(|manifest| Parsed {
            worktrees,
            marketplaces: manifest.marketplaces,
        }))
    }

    #[test]
    fn a_policy_only_manifest_is_valid() {
        let manifest = parsed("worktrees:\n  completion: pr\n").unwrap();
        assert_eq!(
            manifest.worktrees.unwrap().completion,
            CompletionBehavior::Pr
        );
        assert!(manifest.marketplaces.is_empty());
    }

    #[test]
    fn a_misspelled_field_is_named_rather_than_ignored() {
        let error = parsed("worktrees:\n  completon: pr\n").unwrap_err();
        assert!(error.to_string().contains("completon"), "{error}");
    }

    #[test]
    fn a_command_and_a_list_of_commands_are_both_accepted() {
        let one = parsed("worktrees:\n  gate: cargo test\n").unwrap();
        assert_eq!(one.worktrees.unwrap().gate, vec!["cargo test".to_owned()]);
        let many = parsed("worktrees:\n  gate:\n    - cargo test\n    - cargo clippy\n").unwrap();
        assert_eq!(many.worktrees.unwrap().gate.len(), 2);
    }

    #[test]
    fn declaring_the_policy_creates_the_manifest_and_says_that_it_did() {
        let root = uze_testkit::temp::scratch("manifest-set-completion");
        assert!(
            set_completion(&root, CompletionBehavior::Pr).unwrap(),
            "the first call creates the file, and the caller is told so"
        );
        assert_eq!(policy(&root).unwrap().completion, CompletionBehavior::Pr);

        assert!(
            !set_completion(&root, CompletionBehavior::Merge).unwrap(),
            "the second call changes a file that already exists"
        );
        assert_eq!(policy(&root).unwrap().completion, CompletionBehavior::Merge);
    }

    /// The scaffold's own commentary is what teaches the choices, so a
    /// click that changes one must not take the explanation with it.
    #[test]
    fn declaring_the_policy_keeps_the_comment_that_explains_it() {
        let root = uze_testkit::temp::scratch("manifest-set-completion-comments");
        set_completion(&root, CompletionBehavior::Pr).unwrap();
        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(written.contains("completion: pr"), "{written}");
        assert!(written.contains("handoff | merge | pr"), "{written}");
        assert!(written.contains("# slots: 3"), "{written}");
    }

    #[test]
    fn declaring_the_policy_into_a_manifest_that_has_no_policy_block_adds_one() {
        let root = uze_testkit::temp::scratch("manifest-set-completion-marketless");
        let authored = "# ours\nmarketplaces:\n  ai:\n    path: ../ai\n    plugins: [flow]\n";
        fs::write(manifest_path_for(&root), authored).unwrap();

        assert!(
            !set_completion(&root, CompletionBehavior::Merge).unwrap(),
            "a manifest that exists is not created"
        );
        let manifest = load(&root).unwrap().unwrap();
        assert_eq!(
            manifest.worktrees.unwrap().completion,
            CompletionBehavior::Merge
        );
        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(written.contains("# ours"), "{written}");
        assert_eq!(manifest.marketplaces["ai"].plugins, vec!["flow".to_owned()]);
    }

    #[test]
    fn a_cap_of_zero_is_refused_rather_than_honored() {
        let error = parsed("worktrees:\n  slots: 0\n").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("refuse every checkout"), "{message}");
        assert!(parsed("worktrees:\n  slots: 1\n").is_ok());
        assert!(parsed("worktrees:\n  completion: pr\n").is_ok());
    }

    #[test]
    fn a_link_escaping_the_repository_is_rejected() {
        let error = parsed("worktrees:\n  link: [../outside]\n").unwrap_err();
        assert!(error.to_string().contains("worktrees.link"), "{error}");
    }

    #[test]
    fn a_link_to_a_tracked_file_is_rejected_and_an_ignored_one_loads() {
        let repository = uze_testkit::git::Repository::new("manifest-links");
        repository.commit_file(".gitignore", ".env\n");
        let root = repository.root();

        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "worktrees:\n  link: [.env]\n",
        )
        .unwrap();
        let manifest = load(root).unwrap().unwrap();
        assert_eq!(
            manifest.worktrees.unwrap().link,
            vec![PathBuf::from(".env")]
        );

        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "worktrees:\n  link: [README.md]\n",
        )
        .unwrap();
        let error = load(root).unwrap_err();
        let UzeError::MalformedManifest { reason, .. } = error else {
            panic!("a tracked link must be a malformed manifest");
        };
        assert!(
            reason.contains("README.md") && reason.contains("ignore"),
            "{reason}"
        );
    }

    #[test]
    fn an_unknown_key_inside_the_policy_block_is_refused_by_name() {
        let error =
            parsed("worktrees:\n  completion: merge\n  directory: ./.worktrees\n").unwrap_err();
        assert!(error.to_string().contains("directory"), "{error}");
    }

    #[test]
    fn the_policy_round_trips_with_every_field() {
        let manifest = parsed(
            "worktrees:\n  default: isolated\n  target: develop\n  completion: pr\n  \
             link: [.env, .env.local]\n  setup: pnpm install\n  gate:\n    - cargo test\n    \
             - cargo clippy\n  slots: 3\n",
        )
        .unwrap();
        let policy = manifest.worktrees.unwrap();
        assert_eq!(policy.default, AgentPlacementDefault::Isolated);
        assert_eq!(policy.target.as_deref(), Some("develop"));
        assert_eq!(policy.completion, CompletionBehavior::Pr);
        assert_eq!(policy.link.len(), 2);
        assert_eq!(policy.setup, vec!["pnpm install".to_owned()]);
        assert_eq!(
            policy.gate,
            vec!["cargo test".to_owned(), "cargo clippy".to_owned()],
            "a list is what makes a failure say which step failed"
        );
        assert_eq!(policy.slots, Some(3));
    }

    /// Where an agent starts is the project's answer, not a person's, and
    /// an undeclared one is the answer UZE has always given: in the
    /// project's own root, isolated when somebody asks. A value nobody
    /// can read is refused by name with the rest of the policy rather
    /// than quietly falling back to that default — a project that meant
    /// `isolated` and typed it wrong would otherwise put every agent in
    /// the operator's own tree.
    #[test]
    fn where_an_agent_starts_is_declared_defaulted_or_refused_by_name() {
        assert_eq!(
            parsed("worktrees:\n  completion: handoff\n")
                .unwrap()
                .worktrees
                .unwrap()
                .default,
            AgentPlacementDefault::InPlace,
            "undeclared is what was already in force"
        );
        assert_eq!(
            parsed("worktrees:\n  default: in-place\n")
                .unwrap()
                .worktrees
                .unwrap()
                .default,
            AgentPlacementDefault::InPlace
        );

        let reason = parsed("worktrees:\n  default: worktree\n")
            .unwrap_err()
            .to_string();
        assert!(
            reason.contains("worktree"),
            "names the value it read: {reason}"
        );
        assert!(
            reason.contains("in-place") && reason.contains("isolated"),
            "the refusal names both answers it would have taken: {reason}"
        );
    }

    #[test]
    fn ensure_exists_creates_a_commented_default_and_is_idempotent() {
        let root = uze_testkit::temp::scratch("manifest-ensure");
        assert!(ensure_exists(&root).unwrap(), "the first call creates it");
        let first = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(first.contains("completion: handoff"), "{first}");
        assert!(
            first.contains("handoff | merge | pr"),
            "the choices must be discoverable by opening the file: {first}"
        );
        assert_eq!(
            policy(&root).unwrap(),
            WorktreePolicy::default(),
            "everything but the live line is commented, so a created manifest declares exactly \
             what was already in force"
        );

        assert!(
            !ensure_exists(&root).unwrap(),
            "the second call changes nothing"
        );
        assert_eq!(fs::read_to_string(manifest_path_for(&root)).unwrap(), first);
    }

    /// The scaffold is documentation that must not drift from the schema.
    /// Each struct literal is exhaustive, so a field added to the
    /// workspace's sections has to be named here, and this then asks the
    /// created file to offer it — a new knob nobody can discover fails the
    /// build. `uze-core` asks the same of the package manager's section.
    #[test]
    fn a_created_manifest_offers_every_field_the_workspace_reads() {
        fn declared_keys(shape: &impl Serialize) -> Vec<String> {
            serde_json::to_value(shape)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect()
        }

        let policy = WorktreePolicy {
            default: Default::default(),
            branch: crate::worktree::BranchVocabulary::Unset,
            target: Some("main".to_owned()),
            completion: CompletionBehavior::Handoff,
            link: vec![PathBuf::from(".env")],
            setup: vec!["pnpm install".to_owned()],
            gate: vec!["pnpm test".to_owned()],
            slots: Some(3),
            spare: Some(2),
            idle_days: Some(3),
        };
        let artifacts = DeclaredArtifacts {
            path: PathBuf::from("docs/architecture"),
        };

        let keys = [declared_keys(&policy), declared_keys(&artifacts)].concat();
        assert!(keys.len() > 5, "the shapes emitted nothing to check");
        for key in keys {
            assert!(
                SCAFFOLD.contains(&format!("{key}:")),
                "the created manifest never mentions `{key}`, so nothing tells a reader it \
                 exists:\n{SCAFFOLD}"
            );
        }
    }

    #[test]
    fn an_undeclared_policy_is_the_built_in_default() {
        let directory = std::env::temp_dir().join(format!("uze-manifest-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        assert_eq!(policy(&directory).unwrap(), WorktreePolicy::default());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_scaffolded_manifest_declares_no_policy() {
        let root = uze_testkit::temp::scratch("declaration-scaffold-declares-nothing");
        assert!(ensure_exists(&root).unwrap());
        assert_eq!(declared(&root).unwrap(), None);
        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(
            written.contains("# completion: handoff"),
            "the choice is offered, commented: {written}"
        );
    }

    #[test]
    fn choosing_a_completion_writes_it_under_the_scaffolds_empty_key() {
        let root = uze_testkit::temp::scratch("declaration-scaffold-then-choose");
        ensure_exists(&root).unwrap();
        assert!(!set_completion(&root, CompletionBehavior::Pr).unwrap());
        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(
            written.contains("worktrees:\n  completion: pr\n"),
            "{written}"
        );
        assert_eq!(
            written
                .lines()
                .filter(|line| line.starts_with("worktrees:"))
                .count(),
            1,
            "one section, not a second block at the end: {written}"
        );
        assert_eq!(
            declared(&root).unwrap().map(|policy| policy.completion),
            Some(CompletionBehavior::Pr)
        );
    }

    #[test]
    fn a_workspace_section_the_workspace_rejects_does_not_fail_the_manifest() {
        let repository = uze_testkit::git::Repository::new("declaration-unignored-link");
        repository.commit_file("README.md", "tracked\n");
        let root = repository.root();
        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "worktrees:\n  link: [README.md]\n",
        )
        .unwrap();
        assert!(manifest::load(root).unwrap().is_some(), "the file reads");
        assert!(declared(root).is_err(), "the workspace reports the link");
    }
}
