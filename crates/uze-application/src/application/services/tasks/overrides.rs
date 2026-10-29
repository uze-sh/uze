//! What a placed agent's launch carries of the overrides the operator gave
//! it.

use uze_core::integration::IntegrationPort;
use uze_workspace::overrides;

use super::*;

impl Workspace<'_> {
    /// `placed`, with `overrides` recorded as its agent's on the harness its
    /// record now names, when the launch was given any, and with what the
    /// client adds itself when that harness starts without UZE's launcher.
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
                words,
            )
        {
            placed.warnings.push(format!(
                "its overrides could not be recorded, so it starts without them: {error}"
            ));
            return placed;
        }
        if let Some(integration) = self.integration(&harness)
            && !self.launches_through_launcher(integration)
        {
            placed.bare_launch =
                self.bare_launch(&placed.project, &agent, integration, &mut placed.warnings);
        }
        placed
    }

    /// Whether an agent of `integration` starts through UZE's launcher,
    /// which carries its overrides itself: the same rule that names the
    /// launch, read off the gap it reports, so the two never disagree.
    fn launches_through_launcher(&self, integration: &dyn IntegrationPort) -> bool {
        self.launcher(integration, integration.id()).1.is_none()
    }

    /// The agent's recorded overrides on `integration`, checked as the
    /// launcher would check them on a launch it composed nothing for. What
    /// is left out, and what a flag passed gives up, is said with the
    /// placement's warnings.
    fn bare_launch(
        &self,
        project: &Path,
        agent: &AgentId,
        integration: &dyn IntegrationPort,
        warnings: &mut Vec<String>,
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
        if !said.is_empty() {
            warnings.push(said.join("; "));
        }
        let text = |word: std::ffi::OsString| word.to_string_lossy().into_owned();
        BareLaunch {
            args: checked.args.into_iter().map(text).collect(),
            env: checked
                .env
                .into_iter()
                .map(|(name, value)| (text(name), text(value)))
                .collect(),
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
