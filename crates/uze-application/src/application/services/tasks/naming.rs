//! Naming an agent's work, judged against the project's branch vocabulary.

use super::*;

impl Workspace<'_> {
    /// Names the work of the agent a claim identifies.
    ///
    /// The claim is verified the way every reader verifies one: the store
    /// names its identifier, the key is the one that agent's launch was
    /// issued, and the directory is the record's own. The identifier and
    /// the directory are the caller's to choose — any process can stamp
    /// another agent's identity and stand in its checkout — so it is the key,
    /// held only by the launch it was issued to, that keeps one agent from
    /// renaming another's branch.
    /// An agent that is not isolated takes the name as its label alone:
    /// it works on the operator's branch, which already has the name it
    /// will keep, so there is nothing in Git to rename — but the operator
    /// still reads a tab for it, and "agent 3" says nothing about the work.
    ///
    /// Asking renames, however often it is asked. A branch is an address,
    /// and the work it holds turns out to be something else often enough
    /// that naming it once was never the realistic case — what refusing a
    /// second name actually stranded was a branch UZE had derived from a
    /// commit, because nothing downstream could tell that name from one an
    /// agent had chosen.
    ///
    /// Renaming to the name it already carries is not an error — the
    /// record is confirmed and Git is left alone — so an agent may state
    /// its branch's name without first asking what it is.
    #[tracing::instrument(name = "workspace.name_task", skip_all, fields(agent = %claim.id, cwd = %claim.cwd.display(), proposed = %proposed), err)]
    pub fn name_task(&self, claim: Claim<'_>, proposed: &str) -> Result<NamedTask> {
        let not_an_agent =
            || UzeError::TaskNaming("this process is not an agent UZE launched".to_owned());
        let owner = conversation::owner_of(&self.0.home, claim).ok_or_else(not_an_agent)?;
        if !owner.isolated {
            return self.label_in_place(&owner, proposed);
        }
        let (primary, policy) = self
            .repository_context(claim.cwd)
            .ok_or_else(|| UzeError::TaskNaming("not inside a Git working tree".to_owned()))?;
        let vocabulary = policy.branch.clone();
        let branch = vocabulary
            .accept(proposed)
            .map_err(|refusal| UzeError::TaskNaming(refusal_words(&refusal, &vocabulary)))?;
        let target = target_of(&primary, &policy);
        task::locked(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let agent = store.get_mut(&owner.agent).ok_or_else(not_an_agent)?;
            let task = agent.isolation().ok_or_else(not_an_agent)?;
            if checkout::current_branch(claim.cwd).as_deref() != Some(task.branch.as_str()) {
                return Err(UzeError::TaskNaming(
                    "this checkout is not on the task's branch — finish the rebase first"
                        .to_owned(),
                ));
            }
            if task.branch != branch {
                if checkout::name_is_taken(&primary, &branch) {
                    return Err(UzeError::TaskNaming(format!(
                        "`{branch}` already exists in this repository or on its remote"
                    )));
                }
                checkout::rename_branch(&primary, &task.branch.clone(), &branch)?;
            }
            agent.take_name(branch.clone());
            let agent_id = agent.id.clone();
            for child in store.agents.iter_mut() {
                if child.parent.as_ref() == Some(&agent_id)
                    && let Some(isolation) = child.isolation_mut()
                {
                    isolation.target = branch.clone();
                }
            }
            let agent = store.get(&agent_id).ok_or_else(not_an_agent)?;
            Ok(NamedTask {
                task: agent.id.as_str().to_owned(),
                branch: Some(branch),
                label: agent.label.clone(),
            })
        })
    }

    /// Names an agent in the operator's checkout: judged by the same
    /// vocabulary as a branch, so the label reads the same wherever the
    /// agent was placed, and recorded without touching Git — the branch
    /// is the operator's. The policy is read from the root the record is
    /// keyed on, which needs no repository.
    pub(super) fn label_in_place(
        &self,
        owner: &conversation::Owner,
        proposed: &str,
    ) -> Result<NamedTask> {
        let vocabulary = self.policy(&owner.project_root)?.branch;
        let name = vocabulary
            .accept(proposed)
            .map_err(|refusal| UzeError::TaskNaming(refusal_words(&refusal, &vocabulary)))?;
        task::locked(&self.0.home, &owner.project_root, |store| {
            let agent = store.get_mut(&owner.agent).ok_or_else(|| {
                UzeError::TaskNaming("this process is not an agent UZE launched".to_owned())
            })?;
            agent.take_label(&name);
            Ok(NamedTask {
                task: agent.id.as_str().to_owned(),
                branch: None,
                label: agent.label.clone(),
            })
        })
    }
}

/// What an isolation does with the changes the operator's tree holds.
///
/// Never "move" and never "discard": UZE cannot say whose uncommitted
/// change is whose, so the root's working tree is left exactly as it was
/// either way.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Carry {
    /// The checkout starts from the branch alone.
    #[default]
    Nothing,
    /// The checkout starts with a copy of what the root's tree holds over
    /// its `HEAD`, for the files the repository tracks.
    CopyOfChanges,
}

/// What naming a task produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedTask {
    pub task: String,
    /// The branch the work now lives on; `None` for an agent in the
    /// operator's checkout, whose naming renames nothing.
    pub branch: Option<String>,
    pub label: String,
}

/// A refusal, in words an agent can act on: which half was wrong, and what
/// this project would have accepted.
pub(super) fn refusal_words(refusal: &NameRefusal, vocabulary: &BranchVocabulary) -> String {
    match refusal {
        NameRefusal::NotDeclared => {
            "this project explicitly disables agent work names with `workspace.branch: agent`"
                .to_owned()
        }
        NameRefusal::UnknownType { found, .. } => format!(
            "`{found}` is not a type this project accepts; use one of `{}`",
            vocabulary.spelled()
        ),
        NameRefusal::UnexpectedType { found } => format!(
            "this project takes {}, so drop the `{found}/`",
            vocabulary.spelled()
        ),
        NameRefusal::MissingType { .. } => format!(
            "a name is `<type>/<subject>`; the types this project accepts are `{}`",
            vocabulary.spelled()
        ),
        NameRefusal::MalformedSubject { reason } => format!(
            "the subject is one or two words naming the intention, and {reason} — \
             try something like `fix/branch-naming`"
        ),
    }
}
