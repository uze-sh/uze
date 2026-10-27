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
