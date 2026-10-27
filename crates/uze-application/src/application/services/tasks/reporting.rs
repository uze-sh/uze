//! An agent reporting about itself, for the workspace's agent bridge.

use super::*;

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
    /// The reports agent `agent` appended since `offset`, and where to read
    /// from next: what the client turns into the extensions' `agent.*`
    /// events. At `trace`: it is asked every second for every agent while
    /// an extension observes them.
    #[tracing::instrument(name = "workspace.reports_since", level = "trace", skip_all, fields(agent = %agent))]
    pub fn reports_since(
        &self,
        agent: &str,
        offset: u64,
    ) -> (Vec<uze_workspace::reports::Report>, u64) {
        uze_workspace::reports::read_from(&self.0.home, agent, offset)
    }
}
