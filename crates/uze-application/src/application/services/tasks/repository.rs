//! What the workspace reads about a repository: its policy, its branches, its agents and their views.

use super::*;

impl Workspace<'_> {
    /// The project's declared policy, or the defaults when its manifest
    /// declares none. Read from the primary checkout on purpose: a worktree
    /// never declares a policy of its own, and nothing machine-scoped
    /// participates, so the same repository resolves identically everywhere.
    /// A malformed manifest is an error rather than a silent default.
    pub(in crate::application::services) fn policy(
        &self,
        primary: &Path,
    ) -> Result<WorktreePolicy> {
        uze_workspace::declaration::policy(primary)
    }

    /// The row a placement answers with: the agent just recorded, read
    /// back and derived exactly as an evaluation derives every other row.
    ///
    /// One derivation, so the row a launch draws and the row the first
    /// evaluation replaces it with can never disagree about which group
    /// the agent is in. Without it a client has only the directory to go
    /// on until that evaluation lands — a Git pass over the whole
    /// repository — and draws a freshly isolated agent among the ones
    /// sharing the operator's checkout for as long as it takes.
    pub(super) fn placed_view(
        &self,
        primary: &Path,
        id: &str,
        policy: &WorktreePolicy,
    ) -> Option<AgentView> {
        let target = target_of(primary, policy);
        let store = task::load(&self.0.home, primary).ok()?;
        AgentView::from_agent(
            primary,
            store.agent(id)?,
            policy.completion,
            &target,
            landing::forge(primary),
        )
    }

    /// The repository `cwd` belongs to, with its policy and recorded tasks.
    pub(super) fn repository(&self, cwd: &Path) -> Option<Repository> {
        self.open(cwd).ok().flatten()
    }

    /// The same without the tasks: what a *mutation* resolves before it
    /// takes the document, so the one read it acts on is the one
    /// [`task::locked`] takes under the lock rather than an earlier one
    /// somebody else has since replaced.
    pub(in crate::application::services) fn repository_context(
        &self,
        cwd: &Path,
    ) -> Option<(PathBuf, WorktreePolicy)> {
        let primary = worktree::primary_checkout(cwd)?;
        let policy = self.policy(&primary).ok()?;
        Some((primary, policy))
    }

    /// The same, telling "there is no repository here" apart from "there
    /// is one and its recorded tasks could not be read" — the second is
    /// the condition every caller used to render as the first.
    pub(super) fn open(&self, cwd: &Path) -> std::result::Result<Option<Repository>, String> {
        let Some(primary) = worktree::primary_checkout(cwd) else {
            return Ok(None);
        };
        let Ok(policy) = self.policy(&primary) else {
            return Ok(None);
        };
        let store = task::load(&self.0.home, &primary).map_err(|error| error.to_string())?;
        Ok(Some(Repository {
            primary,
            policy,
            store,
        }))
    }

    /// The primary checkout `cwd` belongs to — the key every task view
    /// hangs off — or `None` outside a Git working tree.
    #[tracing::instrument(name = "workspace.primary_of", skip_all, fields(cwd = %cwd.display()))]
    pub fn primary_of(&self, cwd: &Path) -> Option<PathBuf> {
        worktree::primary_checkout(cwd)
    }

    /// The branch checked out where `cwd` sits — what an agent working
    /// outside any slot is on — or `None` for a detached `HEAD` or no
    /// repository at all.
    #[tracing::instrument(name = "workspace.current_branch", skip_all, fields(cwd = %cwd.display()))]
    pub fn current_branch(&self, cwd: &Path) -> Option<String> {
        checkout::current_branch(cwd)
    }

    /// How the branch checked out at `cwd` stands against its upstream —
    /// what a pull would bring and a push would send — when that branch
    /// is the repository's delivery target. `None` on any other branch,
    /// and without an upstream to measure against: an agent on a branch
    /// of its own reaches the remote through the target, so the target
    /// is the one branch whose sync with it is worth a caption.
    #[tracing::instrument(name = "workspace.target_upstream_sync", skip_all, fields(cwd = %cwd.display()))]
    pub fn target_upstream_sync(&self, cwd: &Path) -> Option<UpstreamSync> {
        let repository = self.repository(cwd)?;
        if checkout::current_branch(cwd)? != repository.target() {
            return None;
        }
        let divergence = checkout::upstream_divergence(cwd)?;
        Some(UpstreamSync {
            pull: divergence.behind,
            push: divergence.ahead,
        })
    }

    /// Every piece of work on this machine that no live agent is in front
    /// of, whichever project it belongs to.
    ///
    /// Answers from UZE's own records of every project it has recorded an
    /// agent for — not from the projects this session happens to have
    /// opened — so a space closed by accident does not take its agents'
    /// work out of the one surface that exists to find it again.
    ///
    /// Liveness is not asked here. Which agents a client is in front of is
    /// the client's own question, and it answers it by launch stamp; this
    /// hands over everything still preserved and lets the caller subtract.
    ///
    /// A project whose records cannot be read is skipped rather than
    /// refused: withholding every other project's work because one
    /// document is unreadable is the failure this list exists to prevent.
    #[tracing::instrument(name = "workspace.preserved_work", skip_all)]
    pub fn preserved_work(&self) -> Vec<PreservedWork> {
        let mut preserved: Vec<PreservedWork> = uze_core::record::roots(&self.0.home)
            .into_iter()
            .flat_map(|project| {
                let agents = task::load(&self.0.home, &project)
                    .map(|store| store.agents)
                    .unwrap_or_default();
                agents.into_iter().filter_map({
                    let project = project.clone();
                    move |agent| PreservedWork::from_agent(&project, &agent)
                })
            })
            .collect();
        preserved.sort_by(|left, right| {
            left.created_at_unix
                .cmp(&right.created_at_unix)
                .then_with(|| left.id.cmp(&right.id))
        });
        preserved
    }

    /// Every task recorded for `cwd`'s repository, as last evaluated.
    #[tracing::instrument(name = "workspace.tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn tasks(&self, cwd: &Path) -> Vec<AgentView> {
        self.repository(cwd)
            .map(|repository| repository.views())
            .unwrap_or_default()
    }

    /// The project's say in delivery, for a header to name what `deliver`
    /// will do.
    /// What declaring a completion behavior would do, so a caller can say
    /// it before doing it rather than after. Writing the policy touches a
    /// *tracked* file — the one the whole team reads — and the projected
    /// `AGENTS.md` still needs `uze agent context reconcile` to follow it, so a
    /// click that silently did both would be a click nobody could predict.
    #[tracing::instrument(name = "workspace.completion_change_consequence", skip_all, fields(cwd = %cwd.display()))]
    pub fn completion_change_consequence(&self, cwd: &Path) -> Option<PolicyWriteConsequence> {
        let repository = self.repository(cwd)?;
        let manifest = manifest::manifest_path_for(&repository.primary);
        Some(PolicyWriteConsequence {
            creates_manifest: !manifest.exists(),
            manifest,
        })
    }

    /// Declares the completion behavior for the repository `cwd` belongs
    /// to, creating `agents.yaml` when the project has none. Reports
    /// whether it created it, so the caller can say which of the two
    /// things just happened.
    ///
    /// The policy is the primary checkout's, always: an isolated checkout
    /// declaring one of its own would be a per-worktree policy, which
    /// there is deliberately none of.
    #[tracing::instrument(name = "workspace.set_completion", skip_all, fields(cwd = %cwd.display()), err)]
    pub fn set_completion(&self, cwd: &Path, behavior: CompletionBehavior) -> Result<bool> {
        // `MissingPath` is what resolving a project root already answers
        // with when there is nothing to resolve; a policy is a repository's,
        // and outside one there is no primary checkout to declare it in.
        let primary = self
            .repository(cwd)
            .map(|repository| repository.primary)
            .ok_or_else(|| UzeError::MissingPath(cwd.to_path_buf()))?;
        uze_workspace::declaration::set_completion(&primary, behavior)
    }

    #[tracing::instrument(name = "workspace.delivery_policy", skip_all, fields(cwd = %cwd.display()))]
    pub fn delivery_policy(&self, cwd: &Path) -> Option<DeliveryPolicyView> {
        let repository = self.repository(cwd)?;
        Some(DeliveryPolicyView {
            completion: repository.policy.completion.abi_name(),
            target: repository
                .policy
                .target
                .clone()
                .or_else(|| checkout::current_branch(&repository.primary)),
            gate: repository
                .policy
                .gate
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }
}
