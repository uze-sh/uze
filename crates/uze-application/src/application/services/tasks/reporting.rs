//! An agent reporting about itself, for the workspace's agent bridge.

use super::*;
use uze_workspace::review::ReviewRecord;

impl Workspace<'_> {
    /// Appends `report` to the journal of the agent a claim identifies,
    /// verified as every claim is: by the key its launch was issued. Refused
    /// for a process that is not an agent UZE launched.
    #[tracing::instrument(name = "workspace.report", skip_all, fields(agent = %claim.id, event = %report.event), err)]
    pub fn report(&self, claim: Claim<'_>, report: uze_workspace::reports::Report) -> Result<()> {
        let owner = conversation::owner_of(&self.0.home, claim).ok_or_else(|| {
            UzeError::AgentWork("this process is not an agent UZE launched".to_owned())
        })?;
        uze_workspace::reports::append(&self.0.home, owner.agent.as_str(), report)
    }
}

impl Workspace<'_> {
    /// The reports agent `agent` appended since `cursor`, and where to read
    /// from next: what the client turns into the extensions' `agent.*`
    /// events. At `trace`: it is asked every second for every agent while
    /// an extension observes them.
    #[tracing::instrument(name = "workspace.reports_since", level = "trace", skip_all, fields(agent = %agent))]
    pub fn reports_since(
        &self,
        agent: &str,
        cursor: &uze_workspace::reports::Cursor,
    ) -> (
        Vec<uze_workspace::reports::Report>,
        uze_workspace::reports::Cursor,
    ) {
        uze_workspace::reports::read_from(&self.0.home, agent, cursor)
    }
}

impl Workspace<'_> {
    /// The review the operator keeps for the agent working in `checkout`.
    pub fn review_of_checkout(&self, checkout: &std::path::Path) -> Result<ReviewRecord> {
        uze_workspace::review::of_checkout(&self.0.home, checkout)
    }

    /// Edits the review of the agent working in `checkout` under the
    /// review lock, answering what was written.
    pub fn update_review(
        &self,
        checkout: &std::path::Path,
        change: impl FnOnce(&mut ReviewRecord),
    ) -> Result<ReviewRecord> {
        uze_workspace::review::update_checkout(&self.0.home, checkout, change)
    }

    /// What `uze agent review show` prints for the agent a claim
    /// identifies: the comments sent to it, in full.
    #[tracing::instrument(name = "workspace.review_show", skip_all, fields(agent = %claim.id), err)]
    pub fn review_show(&self, claim: Claim<'_>) -> Result<String> {
        uze_workspace::review::show(&self.0.home, claim)
    }

    /// Attaches an answer to a comment sent to the agent a claim
    /// identifies.
    #[tracing::instrument(name = "workspace.review_reply", skip_all, fields(agent = %claim.id), err)]
    pub fn review_reply(&self, claim: Claim<'_>, reference: &str, answer: &str) -> Result<()> {
        uze_workspace::review::reply(&self.0.home, claim, reference, answer)
    }
}
