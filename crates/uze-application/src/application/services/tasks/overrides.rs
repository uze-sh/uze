//! What a placed agent's launch carries of the overrides the operator gave
//! it.

use uze_core::integration::IntegrationPort;
use uze_workspace::overrides;

use super::*;

/// What the `Restart in` panel opens on for one agent and harness: the
/// overrides the agent carries there, and the entries used lately, its
/// project's first.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StandingOverrides {
    pub recorded: Vec<String>,
    pub recent: Vec<overrides::record::RecentEntry>,
}

impl Workspace<'_> {
    /// [`StandingOverrides`] for `agent_id`, standing in `cwd` (its
    /// checkout or its project), on `harness`. A read of a record: off the
    /// thread that draws.
    #[tracing::instrument(name = "workspace.standing_overrides", skip_all, fields(agent = %agent_id, harness), err)]
    pub fn standing_overrides(
        &self,
        cwd: &Path,
        agent_id: &str,
        harness: &str,
    ) -> Result<StandingOverrides> {
        let root = conversation::project_of(&self.0.home, cwd)
            .map(|root| canonical(&root))
            .ok_or_else(|| {
                UzeError::AgentPlacement("this agent is not one UZE launched here".to_owned())
            })?;
        Ok(StandingOverrides {
            recorded: overrides::record::recorded(
                &self.0.home,
                &root,
                &AgentId::adopted(agent_id),
                harness,
            ),
            recent: overrides::record::recent_entries(&self.0.home, harness, &root),
        })
    }

    /// `placed`, with `overrides` recorded as its agent's on the harness its
    /// record now names, when the launch was given any, and with what the
    /// client adds itself if its tab runs that harness without UZE's
    /// launcher, which only the client knows.
    /// Recorded before the answer, so before the tab that launches it: the
    /// launcher reads them back at that launch. Best-effort like the rest of
    /// a placement's preparation: an agent whose overrides could not be
    /// written still starts, and says it starts without them.
    pub(super) fn launching_with(
        &self,
        mut placed: AgentPlacement,
        overrides: Option<&[String]>,
    ) -> AgentPlacement {
        let agent = placed.placement.agent().clone();
        let Some(harness) = task::load(&self.0.home, &placed.project)
            .ok()
            .and_then(|store| {
                store
                    .agent(agent.as_str())
                    .map(|found| found.harness.clone())
            })
        else {
            return placed;
        };
        if let Some(words) = overrides
            && let Err(error) = overrides::record::record_launch(
                &self.0.home,
                &placed.project,
                &agent,
                &harness,
                self.integration(&harness)
                    .map(IntegrationPort::flag_catalog)
                    .unwrap_or_default(),
                words,
            )
        {
            placed.warnings.push(format!(
                "its overrides could not be recorded, so it starts without them: {error}"
            ));
            return placed;
        }
        if let Some(integration) = self.integration(&harness) {
            placed.bare_launch = self.bare_launch(&placed.project, &agent, integration);
        }
        placed
    }

    /// The agent's recorded overrides on `integration`, checked as the
    /// launcher would check them on a launch it composed nothing for, with
    /// what is left out and what a flag passed gives up.
    fn bare_launch(
        &self,
        project: &Path,
        agent: &AgentId,
        integration: &dyn IntegrationPort,
    ) -> BareLaunch {
        let words = overrides::record::recorded(&self.0.home, project, agent, integration.id());
        if words.is_empty() {
            return BareLaunch::default();
        }
        let checked = overrides::compose(
            &words,
            integration.flag_catalog(),
            integration.managed_environment(),
            &[],
        );
        let said: Vec<String> = checked
            .left_out
            .iter()
            .chain(&checked.weakens)
            .cloned()
            .collect();
        let text = |word: std::ffi::OsString| word.to_string_lossy().into_owned();
        BareLaunch {
            args: checked.args.into_iter().map(text).collect(),
            env: checked
                .env
                .into_iter()
                .map(|(name, value)| (text(name), text(value)))
                .collect(),
            said: (!said.is_empty()).then(|| said.join("; ")),
        }
    }

    /// What a switch away from `outgoing` says when the agent had overrides
    /// there: a flag belongs to one harness, so none crosses, and the ones
    /// it had wait for it to come back.
    pub(super) fn overrides_left_behind(
        &self,
        project: &Path,
        agent: &AgentId,
        outgoing: &dyn IntegrationPort,
    ) -> Option<String> {
        let words = overrides::record::recorded(&self.0.home, project, agent, outgoing.id());
        (!words.is_empty()).then(|| {
            format!(
                "Its overrides on {} were left behind, and come back with it.",
                outgoing.display_name()
            )
        })
    }
}
