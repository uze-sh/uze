//! What a project declares about the workspace, read from the `workspace:`
//! section of `agents.yaml`.
//!
//! The file belongs to `uze-core`'s manifest module, which carries this
//! section unread. Reading it here is what keeps a mistake in it the
//! workspace's to report: a package command never parses it, so it never
//! fails over it.
//!
//! One flat section holds three things the workspace reads apart: the
//! policy for the agents it launches, the places the project keeps the
//! artifacts that describe it, and what an agent is asked at each state its
//! work reaches. A section that declares only places or actions declares no
//! policy, so the policy is still attributed to the default; an empty one
//! written on purpose (`workspace: {}`) declares the defaults, as choosing
//! them is.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    Result, UzeError,
    manifest::{self, Section},
    worktree::{CompletionBehavior, WorktreePolicy},
};

/// The key under `workspace:` that names places rather than policy.
const ARTIFACTS: &str = "artifacts";

/// The key under `workspace:` that names what an agent is asked, rather
/// than policy.
const ACTIONS: &str = "actions";

/// When an instruction is sent: the moment work reaches a state the
/// workspace shows. Named for the moment, never for a deed — what is asked
/// then is the instruction's own text — and its own vocabulary rather than
/// the view's, so a state added to what a surface draws is never accepted
/// here silently.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ActionState {
    OnChange,
    OnPublish,
    OnConflict,
    OnFail,
}

impl ActionState {
    pub const ALL: [Self; 4] = [
        Self::OnChange,
        Self::OnPublish,
        Self::OnConflict,
        Self::OnFail,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::OnChange => "on_change",
            Self::OnPublish => "on_publish",
            Self::OnConflict => "on_conflict",
            Self::OnFail => "on_fail",
        }
    }

    /// What is asked when the project declares nothing for this moment.
    /// Only the two moments where UZE itself left the agent's checkout in
    /// a state nobody but the agent can settle — a paused rebase, a branch
    /// the gate refused — have one: unsaid, the agent would sit in front
    /// of work it does not know is waiting.
    pub const fn fallback(self) -> Option<&'static str> {
        match self {
            Self::OnConflict => Some(
                "Your branch no longer rebases onto its target, and the rebase is paused in \
                 this checkout. Resolve the conflicts preserving the intent of your change, \
                 run git rebase --continue, run the project's checks, and end your turn.",
            ),
            Self::OnFail => Some(
                "The project's checks failed on your branch after it was rebased onto its \
                 target. Fix them on this branch, commit, and end your turn.",
            ),
            Self::OnChange | Self::OnPublish => None,
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.key() == key)
    }
}

/// What the project asks an agent at each state, and what it wrote under
/// `actions` that asks nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeclaredActions {
    pub instructions: BTreeMap<ActionState, String>,
    /// Each entry left out, said as the reason: an unknown key, or a value
    /// that is not a non-empty line of text.
    pub ignored: Vec<String>,
}

impl DeclaredActions {
    /// What is asked at each moment: the project's instruction where it
    /// declares one, the [`ActionState::fallback`] where it does not.
    pub fn with_fallbacks(&self) -> BTreeMap<ActionState, String> {
        let mut instructions: BTreeMap<ActionState, String> = ActionState::ALL
            .into_iter()
            .filter_map(|state| Some((state, state.fallback()?.to_owned())))
            .collect();
        instructions.extend(self.instructions.clone());
        instructions
    }
}

/// The isolation policy the project declares, or `None` when it declares
/// none. Validated as the workspace uses it: a linked file must be one the
/// repository ignores, and a cap of zero is refused.
pub fn declared(root: &Path) -> Result<Option<WorktreePolicy>> {
    let path = manifest::manifest_path_for(root);
    let Some(policy) = read(root)?.policy(&path)? else {
        return Ok(None);
    };
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

/// Declares the delivery behavior, or takes the declaration back with
/// `None`, creating the manifest when the project has none, and reports
/// whether it had to — a caller showing a person the consequence of their
/// click says "this creates a tracked file" before it happens rather than
/// after.
pub fn set_completion(root: &Path, behavior: Option<CompletionBehavior>) -> Result<bool> {
    let created = match behavior {
        Some(behavior) => {
            manifest::set_scalar(root, Section::Workspace, "delivery", behavior.abi_name())?
        }
        None => {
            manifest::unset_scalar(root, Section::Workspace, "delivery")?;
            false
        }
    };
    // Re-read through the typed path: a write the schema would reject is a
    // bug here, not on a later command.
    declared(root)?;
    Ok(created)
}

/// Where the project keeps what describes it. Places and nothing else:
/// what each file in them *is* is read off the file, so a kind the
/// workspace learns later needs no key of its own.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DeclaredArtifacts {
    /// Each relative to the project root, and inside it.
    pub paths: Vec<PathBuf>,
}

impl<'de> Deserialize<'de> for DeclaredArtifacts {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Places {
            One(PathBuf),
            Many(Vec<PathBuf>),
        }
        match Places::deserialize(deserializer) {
            Ok(Places::One(path)) => Ok(Self { paths: vec![path] }),
            Ok(Places::Many(paths)) => Ok(Self { paths }),
            Err(_) => Err(serde::de::Error::custom(
                "`workspace.artifacts` takes a directory, or a list of them",
            )),
        }
    }
}

/// What the project asks an agent at each state.
///
/// Read leniently, unlike the policy beside it: an unknown policy key read
/// as a default is a policy silently not honoured, and refusing it is
/// right; an unknown action key makes nothing happen, so it is left out and
/// named, and the rest of the project still reads. Failing on it would
/// empty every agent's row over a typo in what a click sends.
pub fn actions(root: &Path) -> Result<DeclaredActions> {
    Ok(read(root)?.actions())
}

/// What is asked at each moment in `root`: see
/// [`DeclaredActions::with_fallbacks`]. A manifest that cannot be read
/// leaves the fallbacks, since a paused rebase is no less paused for it.
pub fn instructions(root: &Path) -> BTreeMap<ActionState, String> {
    actions(root).unwrap_or_default().with_fallbacks()
}

/// The artifact places the project declares, if any.
pub fn artifacts(root: &Path) -> Result<Option<DeclaredArtifacts>> {
    read(root)?.artifacts(&manifest::manifest_path_for(root))
}

/// The `workspace:` section as written, before either half is read; `None`
/// where the project wrote none, or wrote the key with nothing under it.
#[derive(Default)]
struct WorkspaceSection(Option<serde_json::Map<String, serde_json::Value>>);

fn read(root: &Path) -> Result<WorkspaceSection> {
    let path = manifest::manifest_path_for(root);
    let Some(manifest) = manifest::load(root)? else {
        return Ok(WorkspaceSection::default());
    };
    WorkspaceSection::of(manifest.section(Section::Workspace, &path)?, &path)
}

impl WorkspaceSection {
    fn of(value: Option<serde_json::Value>, path: &Path) -> Result<Self> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(Self::default()),
            Some(serde_json::Value::Object(map)) => Ok(Self(Some(map))),
            Some(_) => Err(malformed(
                path,
                "`workspace:` holds keys, not a value".to_owned(),
            )),
        }
    }

    fn policy(&self, path: &Path) -> Result<Option<WorktreePolicy>> {
        let Some(section) = &self.0 else {
            return Ok(None);
        };
        let mut policy = section.clone();
        let places = policy.remove(ARTIFACTS).is_some();
        let actions = policy.remove(ACTIONS).is_some();
        if (places || actions) && policy.is_empty() {
            return Ok(None);
        }
        serde_json::from_value(serde_json::Value::Object(policy))
            .map(Some)
            .map_err(|error| malformed(path, format!("workspace: {error}")))
    }

    fn actions(&self) -> DeclaredActions {
        let Some(value) = self.0.as_ref().and_then(|section| section.get(ACTIONS)) else {
            return DeclaredActions::default();
        };
        let mut declared = DeclaredActions::default();
        let Some(entries) = value.as_object() else {
            declared.ignored.push(format!("`{ACTIONS}` must hold keys"));
            return declared;
        };
        for (key, value) in entries {
            let Some(state) = ActionState::from_key(key) else {
                declared.ignored.push(format!("unknown action `{key}`"));
                continue;
            };
            match value.as_str().map(str::trim) {
                Some(text) if !text.is_empty() => {
                    declared.instructions.insert(state, text.to_owned());
                }
                _ => declared
                    .ignored
                    .push(format!("`{key}` is not one line of text")),
            }
        }
        declared
    }

    fn artifacts(&self, path: &Path) -> Result<Option<DeclaredArtifacts>> {
        self.0
            .as_ref()
            .and_then(|section| section.get(ARTIFACTS))
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|error| malformed(path, error.to_string()))
            })
            .transpose()
    }
}

fn malformed(path: &Path, reason: String) -> UzeError {
    UzeError::MalformedManifest {
        path: path.to_path_buf(),
        reason,
    }
}

fn validate(policy: &WorktreePolicy, path: &Path) -> Result<()> {
    if policy.slots == Some(0) {
        // Zero is honored, and honoring it refuses every checkout: the cap
        // is compared with `>=`, so the first task fails with "cap
        // reached" and nothing says the manifest is why.
        return Err(malformed(
            path,
            "`workspace.slots` is 0, which would refuse every checkout there is; leave it \
             undeclared for no cap at all, or give it at least 1"
                .to_owned(),
        ));
    }
    if let Some((link, why)) = policy.misplaced_links().into_iter().next() {
        return Err(malformed(
            path,
            format!(
                "`workspace.link` names `{}`, which is {why}; a link is a relative path inside \
                 the repository",
                link.display()
            ),
        ));
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
        let answer = crate::git::read(root, &["check-ignore", "--quiet", "--", &spelled]).map_err(
            |error| UzeError::MalformedManifest {
                path: path.to_path_buf(),
                reason: format!("`workspace.link` names `{spelled}`, but {error}"),
            },
        )?;
        match answer.code {
            Some(0) => {}
            Some(1) => {
                return Err(UzeError::MalformedManifest {
                    path: path.to_path_buf(),
                    reason: format!(
                        "`workspace.link` names `{spelled}`, which the repository does not \
                         ignore; a linked file must be ignored, or it would be committed as a \
                         symlink from an agent's checkout"
                    ),
                });
            }
            _ => {
                return Err(UzeError::MalformedManifest {
                    path: path.to_path_buf(),
                    reason: format!(
                        "`workspace.link` names `{spelled}`, but this directory is not a Git \
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
        manifest::{MANIFEST_FILE_NAME, ensure_exists, manifest_path_for},
        worktree::AgentPlacementDefault,
    };

    /// The sections a test reads back: the workspace's policy as the
    /// workspace parses it, and the package manager's marketplaces beside it.
    #[derive(Debug)]
    struct Parsed {
        policy: Option<WorktreePolicy>,
        marketplaces: BTreeMap<String, DeclaredMarketplace>,
    }

    fn parsed(text: &str) -> Result<Parsed> {
        let path = Path::new("/p/agents.yaml");
        let manifest = manifest::parse(text, path)?;
        let policy = WorkspaceSection::of(manifest.section(Section::Workspace, path)?, path)?
            .policy(path)?;
        if let Some(policy) = &policy {
            validate(policy, path)?;
        }
        Ok(Parsed {
            policy,
            marketplaces: manifest.marketplaces,
        })
    }

    fn load(root: &Path) -> Result<Option<Parsed>> {
        let policy = declared(root)?;
        Ok(manifest::load(root)?.map(|manifest| Parsed {
            policy,
            marketplaces: manifest.marketplaces,
        }))
    }

    #[test]
    fn a_policy_only_manifest_is_valid() {
        let manifest = parsed("workspace:\n  delivery: push\n").unwrap();
        assert_eq!(
            manifest.policy.unwrap().completion,
            Some(CompletionBehavior::Push)
        );
        assert!(manifest.marketplaces.is_empty());
    }

    #[test]
    fn a_misspelled_field_is_named_rather_than_ignored() {
        let error = parsed("workspace:\n  completon: pr\n").unwrap_err();
        assert!(error.to_string().contains("completon"), "{error}");
    }

    #[test]
    fn a_command_may_be_spelled_per_platform() {
        let one = parsed("workspace:\n  setup:\n    posix: pnpm i && cp a b\n    windows: pnpm i; Copy-Item a b\n")
            .unwrap()
            .policy
            .unwrap();
        assert_eq!(one.setup.len(), 1);
        assert_eq!(
            one.setup[0].spelling(uze_core::shell::Family::Posix),
            Some("pnpm i && cp a b")
        );
        assert_eq!(
            one.setup[0].spelling(uze_core::shell::Family::PowerShell),
            Some("pnpm i; Copy-Item a b")
        );

        let mixed = parsed("workspace:\n  gate:\n    - cargo test\n    - windows: cargo test --target x86_64-pc-windows-msvc\n")
            .unwrap()
            .policy
            .unwrap();
        assert_eq!(mixed.gate.len(), 2);
        assert_eq!(mixed.gate[1].spelling(uze_core::shell::Family::Posix), None);
        assert!(
            mixed.gate[1]
                .spelling(uze_core::shell::Family::PowerShell)
                .is_some()
        );
    }

    #[test]
    fn a_command_and_a_list_of_commands_are_both_accepted() {
        let one = parsed("workspace:\n  gate: cargo test\n").unwrap();
        assert_eq!(one.policy.unwrap().gate, vec!["cargo test".into()]);
        let many = parsed("workspace:\n  gate:\n    - cargo test\n    - cargo clippy\n").unwrap();
        assert_eq!(many.policy.unwrap().gate.len(), 2);
    }

    #[test]
    fn declaring_the_policy_creates_the_manifest_and_says_that_it_did() {
        let root = uze_testkit::temp::scratch("manifest-set-completion");
        assert!(
            set_completion(&root, Some(CompletionBehavior::Push)).unwrap(),
            "the first call creates the file, and the caller is told so"
        );
        assert_eq!(
            policy(&root).unwrap().completion,
            Some(CompletionBehavior::Push)
        );

        assert!(
            !set_completion(&root, Some(CompletionBehavior::Merge)).unwrap(),
            "the second call changes a file that already exists"
        );
        assert_eq!(
            policy(&root).unwrap().completion,
            Some(CompletionBehavior::Merge)
        );
    }

    #[test]
    fn declaring_the_policy_into_a_manifest_that_has_no_policy_block_adds_one() {
        let root = uze_testkit::temp::scratch("manifest-set-completion-marketless");
        let authored = "# ours\nmarketplaces:\n  ai:\n    path: ../ai\n    plugins: [flow]\n";
        fs::write(manifest_path_for(&root), authored).unwrap();

        assert!(
            !set_completion(&root, Some(CompletionBehavior::Merge)).unwrap(),
            "a manifest that exists is not created"
        );
        let manifest = load(&root).unwrap().unwrap();
        assert_eq!(
            manifest.policy.unwrap().completion,
            Some(CompletionBehavior::Merge)
        );
        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(written.contains("# ours"), "{written}");
        assert_eq!(manifest.marketplaces["ai"].plugins, vec!["flow".to_owned()]);
    }

    #[test]
    fn a_cap_of_zero_is_refused_rather_than_honored() {
        let error = parsed("workspace:\n  slots: 0\n").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("refuse every checkout"), "{message}");
        assert!(parsed("workspace:\n  slots: 1\n").is_ok());
        assert!(parsed("workspace:\n  delivery: push\n").is_ok());
    }

    #[test]
    fn a_link_escaping_the_repository_is_rejected() {
        let error = parsed("workspace:\n  link: [../outside]\n").unwrap_err();
        assert!(error.to_string().contains("workspace.link"), "{error}");
    }

    #[test]
    fn a_link_to_a_tracked_file_is_rejected_and_an_ignored_one_loads() {
        let repository = uze_testkit::git::Repository::new("manifest-links");
        repository.commit_file(".gitignore", ".env\n");
        let root = repository.root();

        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "workspace:\n  link: [.env]\n",
        )
        .unwrap();
        let manifest = load(root).unwrap().unwrap();
        assert_eq!(manifest.policy.unwrap().link, vec![PathBuf::from(".env")]);

        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "workspace:\n  link: [README.md]\n",
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
            parsed("workspace:\n  delivery: merge\n  directory: ./.worktrees\n").unwrap_err();
        assert!(error.to_string().contains("directory"), "{error}");
    }

    #[test]
    fn the_policy_round_trips_with_every_field() {
        let manifest = parsed(
            "workspace:\n  worktree: always\n  target: develop\n  delivery: push\n  \
             link: [.env, .env.local]\n  setup: pnpm install\n  gate:\n    - cargo test\n    \
             - cargo clippy\n  slots: 3\n",
        )
        .unwrap();
        let policy = manifest.policy.unwrap();
        assert_eq!(policy.default, AgentPlacementDefault::Isolated);
        assert_eq!(policy.target.as_deref(), Some("develop"));
        assert_eq!(policy.completion, Some(CompletionBehavior::Push));
        assert_eq!(policy.link.len(), 2);
        assert_eq!(policy.setup, vec!["pnpm install".into()]);
        assert_eq!(
            policy.gate,
            vec!["cargo test".into(), "cargo clippy".into()],
            "a list is what makes a failure say which step failed"
        );
        assert_eq!(policy.slots, Some(3));
    }

    /// When an agent gets a worktree is the project's answer, not a
    /// person's, and an undeclared one is the answer UZE has always given:
    /// in the project's own root, moved when somebody asks. A value nobody
    /// can read is refused by name with the rest of the policy rather
    /// than quietly falling back to that default — a project that meant
    /// `always` and typed it wrong would otherwise put every agent in the
    /// operator's own tree.
    #[test]
    fn where_an_agent_starts_is_declared_defaulted_or_refused_by_name() {
        assert_eq!(
            parsed("workspace:\n  delivery: merge\n")
                .unwrap()
                .policy
                .unwrap()
                .default,
            AgentPlacementDefault::InPlace,
            "undeclared is what was already in force"
        );
        assert_eq!(
            parsed("workspace:\n  worktree: manual\n")
                .unwrap()
                .policy
                .unwrap()
                .default,
            AgentPlacementDefault::InPlace
        );

        let reason = parsed("workspace:\n  worktree: isolated\n")
            .unwrap_err()
            .to_string();
        assert!(
            reason.contains("isolated"),
            "names the value it read: {reason}"
        );
        assert!(
            reason.contains("manual") && reason.contains("always"),
            "the refusal names both answers it would have taken: {reason}"
        );
    }

    /// `handoff` and `pr` are gone, not aliased: an old spelling is a
    /// mistake to name, and the refusal says what the section takes now.
    #[test]
    fn a_retired_delivery_is_refused_naming_what_is_accepted() {
        for retired in ["handoff", "pr"] {
            let reason = parsed(&format!("workspace:\n  delivery: {retired}\n"))
                .unwrap_err()
                .to_string();
            assert!(
                reason.contains("merge") && reason.contains("push"),
                "the refusal names both behaviors it would have taken: {reason}"
            );
        }
    }

    #[test]
    fn an_undeclared_delivery_is_no_delivery() {
        let policy = parsed("workspace:\n  worktree: always\n")
            .unwrap()
            .policy
            .unwrap();
        assert_eq!(policy.completion, None);
    }

    #[test]
    fn taking_a_delivery_back_removes_the_key() {
        let root = uze_testkit::temp::scratch("manifest-unset-completion");
        set_completion(&root, Some(CompletionBehavior::Push)).unwrap();
        assert!(!set_completion(&root, None).unwrap());
        let written = fs::read_to_string(root.join("agents.yaml")).unwrap();
        assert!(!written.contains("delivery"), "{written}");
        assert_eq!(policy(&root).unwrap().completion, None);
    }

    fn project_declaring(label: &str, workspace: &str) -> PathBuf {
        let root = uze_testkit::temp::scratch(label);
        fs::write(root.join("agents.yaml"), format!("workspace:\n{workspace}")).unwrap();
        root
    }

    #[test]
    fn declared_actions_read_by_the_moment_they_answer() {
        let root = project_declaring(
            "declared-actions",
            "  delivery: push\n  actions:\n    on_publish: Open a pull request.\n    \
             on_fail: \"  Fix the checks.  \"\n",
        );
        let declared = actions(&root).unwrap();
        assert_eq!(
            declared.instructions,
            BTreeMap::from([
                (ActionState::OnPublish, "Open a pull request.".to_owned()),
                (ActionState::OnFail, "Fix the checks.".to_owned()),
            ])
        );
        assert!(declared.ignored.is_empty(), "{:?}", declared.ignored);
        assert_eq!(
            policy(&root).unwrap().completion,
            Some(CompletionBehavior::Push),
            "the policy beside them still reads"
        );
    }

    #[test]
    fn an_unknown_state_or_an_empty_instruction_is_left_out_and_named() {
        let root = project_declaring(
            "declared-actions-ignored",
            "  delivery: merge\n  actions:\n    on_redy: typo\n    published: old key\n    \
             on_conflict: \"\"\n    on_change: Commit your work.\n",
        );
        let declared = actions(&root).unwrap();
        assert_eq!(
            declared.instructions,
            BTreeMap::from([(ActionState::OnChange, "Commit your work.".to_owned())])
        );
        assert_eq!(declared.ignored.len(), 3, "{:?}", declared.ignored);
        assert!(
            declared
                .ignored
                .iter()
                .any(|reason| reason.contains("on_redy")),
            "names the key: {:?}",
            declared.ignored
        );
        assert!(
            declared
                .ignored
                .iter()
                .any(|reason| reason.contains("on_conflict")),
            "{:?}",
            declared.ignored
        );
        assert_eq!(
            policy(&root).unwrap().completion,
            Some(CompletionBehavior::Merge),
            "a bad action key never stops the policy from reading"
        );
    }

    #[test]
    fn actions_alone_declare_no_policy_and_none_declared_is_empty() {
        let root = project_declaring(
            "declared-actions-alone",
            "  actions:\n    on_change: Commit your work.\n",
        );
        assert_eq!(declared(&root).unwrap(), None);
        assert_eq!(actions(&root).unwrap().instructions.len(), 1);

        let root = project_declaring("declared-actions-none", "  delivery: merge\n");
        assert_eq!(actions(&root).unwrap(), DeclaredActions::default());
    }

    #[test]
    fn a_conflict_and_a_failed_gate_are_asked_about_unless_the_project_says_otherwise() {
        let root = project_declaring(
            "declared-actions-fallback",
            "  delivery: push\n  actions:\n    on_fail: Fix the checks.\n",
        );
        let instructions = instructions(&root);
        assert_eq!(
            instructions.get(&ActionState::OnFail).map(String::as_str),
            Some("Fix the checks.")
        );
        assert_eq!(
            instructions
                .get(&ActionState::OnConflict)
                .map(String::as_str),
            ActionState::OnConflict.fallback()
        );
        assert_eq!(instructions.get(&ActionState::OnPublish), None);
        assert_eq!(instructions.get(&ActionState::OnChange), None);
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
            !written.contains("workspace"),
            "a created manifest says nothing about the workspace: {written}"
        );
    }

    #[test]
    fn choosing_a_delivery_adds_the_section_and_leaves_the_rest_alone() {
        let root = uze_testkit::temp::scratch("declaration-scaffold-then-choose");
        ensure_exists(&root).unwrap();
        let before = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(!set_completion(&root, Some(CompletionBehavior::Push)).unwrap());
        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(written.starts_with(before.trim_end()), "{written}");
        assert!(
            written.contains("workspace:\n  delivery: push"),
            "{written}"
        );
        assert_eq!(
            declared(&root)
                .unwrap()
                .and_then(|policy| policy.completion),
            Some(CompletionBehavior::Push)
        );
    }

    #[test]
    fn places_alone_declare_no_policy() {
        let manifest = parsed("workspace:\n  artifacts: docs\n").unwrap();
        assert_eq!(manifest.policy, None);
    }

    #[test]
    fn artifacts_are_one_place_or_several_and_never_a_mapping() {
        let read = |text: &str| {
            let path = Path::new("/p/agents.yaml");
            let manifest = manifest::parse(text, path)?;
            WorkspaceSection::of(manifest.section(Section::Workspace, path)?, path)?.artifacts(path)
        };
        assert_eq!(
            read("workspace:\n  artifacts: docs\n")
                .unwrap()
                .unwrap()
                .paths,
            vec![PathBuf::from("docs")]
        );
        assert_eq!(
            read("workspace:\n  artifacts: [docs, design]\n")
                .unwrap()
                .unwrap()
                .paths,
            vec![PathBuf::from("docs"), PathBuf::from("design")]
        );
        let reason = read("workspace:\n  artifacts:\n    path: docs\n")
            .unwrap_err()
            .to_string();
        assert!(reason.contains("a directory, or a list"), "{reason}");
    }

    /// The created manifest no longer teaches the workspace's keys, so the
    /// reference page does: a knob nobody can discover fails the build.
    #[test]
    fn the_reference_documents_every_key_the_workspace_reads() {
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
            default: AgentPlacementDefault::Isolated,
            branch: crate::worktree::BranchVocabulary::Unset,
            target: Some("main".to_owned()),
            completion: Some(CompletionBehavior::Merge),
            link: vec![PathBuf::from(".env")],
            setup: vec!["pnpm install".into()],
            gate: vec!["pnpm test".into()],
            slots: Some(3),
            idle_days: Some(3),
        };
        let reference = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/content/docs/reference/project-files.mdx"
        ))
        .unwrap();
        let keys = [
            declared_keys(&policy),
            vec![ARTIFACTS.to_owned(), ACTIONS.to_owned()],
            ActionState::ALL
                .iter()
                .map(|state| state.key().to_owned())
                .collect(),
        ]
        .concat();
        assert!(keys.len() > 5, "the shapes emitted nothing to check");
        for key in keys {
            assert!(
                reference.contains(&format!("{key}:")),
                "project-files.mdx never mentions `workspace.{key}`"
            );
        }
    }

    #[test]
    fn a_workspace_section_the_workspace_rejects_does_not_fail_the_manifest() {
        let repository = uze_testkit::git::Repository::new("declaration-unignored-link");
        repository.commit_file("README.md", "tracked\n");
        let root = repository.root();
        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "workspace:\n  link: [README.md]\n",
        )
        .unwrap();
        assert!(manifest::load(root).unwrap().is_some(), "the file reads");
        assert!(declared(root).is_err(), "the workspace reports the link");
    }
}
