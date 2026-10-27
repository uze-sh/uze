//! What the workspace's agents did, told to the extensions that hold
//! `agent:observe` (`agent-bridge`).
//!
//! Each agent's journal is read from where the last read stopped, on the
//! checkout watch's one-second clock and only while an enabled extension
//! observes; a first read only sets the offset. A report becomes an
//! `agent.*` event with the tool's name scrubbed and its path kept only
//! inside the agent's checkout, relative to it. An agent whose harness has
//! reported nothing this session has its turns read from the pane's paint
//! instead: a turn starts when it starts working and finishes when it
//! stops.

use super::*;
use uze_contract::AgentEvent;
use uze_extensions::contract::AgentHappened;

/// One agent of the workspace, as the bridge tells it.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui::orchestrator) struct BridgedAgent {
    pub(in crate::ui::orchestrator) id: String,
    pub(in crate::ui::orchestrator) harness: String,
    pub(in crate::ui::orchestrator) label: String,
    pub(in crate::ui::orchestrator) space: String,
    pub(in crate::ui::orchestrator) checkout: PathBuf,
    pub(in crate::ui::orchestrator) pane: PaneId,
    /// How its harness's prompt invokes a skill, when it can.
    pub(in crate::ui::orchestrator) skill_prefix: Option<&'static str>,
}

/// What the bridge keeps between reads.
#[derive(Default)]
pub(in crate::ui::orchestrator) struct Bridge {
    /// Where each agent's journal is read from next.
    offsets: BTreeMap<String, u64>,
    /// The agents a report came from this session: their turns are no
    /// longer read from paint.
    reporting: BTreeSet<String>,
    /// Whether each agent was working at the last look.
    working: BTreeMap<String, bool>,
    /// When each working agent's turn began, in milliseconds since the
    /// epoch: the `since` an `Elapsed` counts from.
    turn_began: BTreeMap<String, u64>,
    /// The agents as last seen.
    agents: Vec<BridgedAgent>,
    /// What happened since the engines were last told.
    happened: Vec<AgentHappened>,
    /// Each agent's turns seen this session, for one write per turn.
    pub(in crate::ui::orchestrator) turns: BTreeMap<String, u64>,
    /// The agents that asked for an approval not yet answered.
    pub(in crate::ui::orchestrator) awaiting_approval: BTreeSet<String>,
    /// What extensions may still write, and whether the operator paused
    /// them.
    pub(in crate::ui::orchestrator) writes: super::agent::Writes,
}

/// The most of a tool's name an event carries.
const TOOL_MAX: usize = 64;

impl Bridge {
    /// The agents as last seen.
    pub(in crate::ui::orchestrator) fn agents_seen(&self) -> &[BridgedAgent] {
        &self.agents
    }
}

impl WorkspaceModel {
    /// Whether any enabled extension observes the agents.
    pub(in crate::ui::orchestrator) fn agents_observed(&self) -> bool {
        self.remembered
            .declared
            .extensions
            .iter()
            .any(|(identity, extension)| {
                extension.observes_agents
                    && self.offers_declared(&Owner::Declared(identity.clone()))
            })
    }

    /// The journals the next watch reads: each agent's, from its offset,
    /// or `None` for one never read, whose first read only sets it.
    pub(in crate::ui::orchestrator) fn journals_to_read(&self) -> Vec<(String, Option<u64>)> {
        if !self.agents_observed() {
            return Vec::new();
        }
        let bridge = &self.remembered.declared.bridge;
        bridge
            .agents
            .iter()
            .map(|agent| (agent.id.clone(), bridge.offsets.get(&agent.id).copied()))
            .collect()
    }

    /// Takes the agents as they stand: the paint's turns for those that
    /// report nothing.
    pub(in crate::ui::orchestrator) fn see_agents(&mut self, agents: Vec<BridgedAgent>) {
        let working: Vec<(String, bool)> = agents
            .iter()
            .map(|agent| (agent.id.clone(), self.agent_is_working(agent.pane)))
            .collect();
        let bridge = &mut self.remembered.declared.bridge;
        let mut painted = Vec::new();
        for ((id, now), agent) in working.iter().zip(&agents) {
            let before = bridge.working.insert(id.clone(), *now);
            if bridge.reporting.contains(id) || before.is_none_or(|before| before == *now) {
                continue;
            }
            painted.push(AgentHappened {
                event: if *now {
                    AgentEvent::TurnStarted
                } else {
                    AgentEvent::TurnFinished
                },
                agent: id.clone(),
                harness: agent.harness.clone(),
                checkout: agent.checkout.clone(),
                detail: serde_json::json!({ "source": "paint" }),
            });
        }
        bridge
            .working
            .retain(|id, _| agents.iter().any(|agent| &agent.id == id));
        bridge
            .offsets
            .retain(|id, _| agents.iter().any(|agent| &agent.id == id));
        bridge
            .turn_began
            .retain(|id, _| agents.iter().any(|agent| &agent.id == id));
        bridge.agents = agents;
        for event in &painted {
            note_turn(bridge, event);
        }
        bridge.happened.extend(painted);
    }

    /// Takes what the journals held since the last read.
    pub(in crate::ui::orchestrator) fn absorb_reports(
        &mut self,
        read: Vec<(String, Option<Vec<uze_application::AgentReport>>, u64)>,
    ) {
        let bridge = &mut self.remembered.declared.bridge;
        for (id, reports, offset) in read {
            bridge.offsets.insert(id.clone(), offset);
            let Some(reports) = reports else {
                continue;
            };
            let Some(agent) = bridge.agents.iter().find(|agent| agent.id == id) else {
                continue;
            };
            if !reports.is_empty() {
                bridge.reporting.insert(id.clone());
            }
            let happened: Vec<AgentHappened> = reports
                .into_iter()
                .filter_map(|report| happened_of(agent, report))
                .collect();
            for event in &happened {
                note_turn(bridge, event);
            }
            bridge.happened.extend(happened);
        }
    }

    /// The agents as an `agents` source answers them.
    fn agents_value(&self) -> serde_json::Value {
        let bridge = &self.remembered.declared.bridge;
        serde_json::Value::Array(
            bridge
                .agents
                .iter()
                .map(|agent| {
                    let state = match self.agent_tab_status(agent.pane, false) {
                        AgentTabStatus::Working => "working",
                        AgentTabStatus::Completed => "done",
                        AgentTabStatus::Selected | AgentTabStatus::Idle => "ready",
                    };
                    serde_json::json!({
                        "id": agent.id,
                        "harness": agent.harness,
                        "label": agent.label,
                        "space": agent.space,
                        "checkout": agent.checkout.display().to_string(),
                        "state": state,
                        "since": bridge.turn_began.get(&agent.id),
                        "reports": bridge.reporting.contains(&agent.id),
                    })
                })
                .collect(),
        )
    }

    /// Tells each observing engine what the agents did and how they stand,
    /// and follows the prompts its events sent.
    pub(in crate::ui::orchestrator) fn tell_agents(&mut self, now: Instant) {
        let happened = std::mem::take(&mut self.remembered.declared.bridge.happened);
        if !self.agents_observed() {
            return;
        }
        let value = self.agents_value();
        let observers: Vec<Owner> = self
            .remembered
            .declared
            .extensions
            .iter()
            .filter(|(_, extension)| extension.observes_agents)
            .map(|(identity, _)| Owner::Declared(identity.clone()))
            .filter(|owner| self.offers_declared(owner))
            .collect();
        let mut requests = Vec::new();
        for owner in observers {
            let asking = owner.clone();
            let Some(engine) = self.remembered.declared.engines.get_mut(&owner) else {
                continue;
            };
            if !happened.is_empty() {
                engine.agent_events(&happened, now);
            }
            engine.agents(&value);
            requests.extend(
                engine
                    .requests()
                    .into_iter()
                    .map(|request| (asking.clone(), request)),
            );
        }
        if !happened.is_empty() {
            self.dirty = true;
        }
        for (owner, request) in requests {
            if let Outcome::Agent(request) = request {
                self.ask_of_agent(owner, request);
            }
        }
    }
}

/// What `event` says of its agent's turn and of an approval it waits on.
fn note_turn(bridge: &mut Bridge, event: &AgentHappened) {
    match event.event {
        AgentEvent::TurnStarted => {
            *bridge.turns.entry(event.agent.clone()).or_default() += 1;
            bridge.awaiting_approval.remove(&event.agent);
            let at = event.detail["at"].as_u64().unwrap_or_else(epoch_ms);
            bridge.turn_began.insert(event.agent.clone(), at);
        }
        AgentEvent::TurnFinished => {
            bridge.turn_began.remove(&event.agent);
            bridge.awaiting_approval.remove(&event.agent);
        }
        AgentEvent::ApprovalAsked => {
            bridge.awaiting_approval.insert(event.agent.clone());
        }
        _ => {
            bridge.awaiting_approval.remove(&event.agent);
        }
    }
}

/// Now, in milliseconds since the epoch: when a turn read from the paint
/// began, which no report dated.
fn epoch_ms() -> u64 {
    u64::try_from(uze_platform::clock::since_epoch().as_millis()).unwrap_or(u64::MAX)
}

/// `report` of `agent` as an event, or `None` for one no event names.
fn happened_of(
    agent: &BridgedAgent,
    report: uze_application::AgentReport,
) -> Option<AgentHappened> {
    let event = AgentEvent::parse(&report.event)?;
    let mut detail = serde_json::Map::new();
    detail.insert("at".to_owned(), report.at.into());
    if let Some(tool) = report
        .tool
        .as_deref()
        .map(scrubbed_tool)
        .filter(|tool| !tool.is_empty())
    {
        detail.insert("tool".to_owned(), tool.into());
    }
    if let Some(path) = report
        .path
        .as_deref()
        .and_then(|path| inside_checkout(&agent.checkout, path))
    {
        detail.insert("path".to_owned(), path.into());
    }
    if let Some(call) = report.call {
        detail.insert("call".to_owned(), call.into());
    }
    if let Some(outcome) = report.outcome {
        detail.insert("outcome".to_owned(), outcome.into());
    }
    Some(AgentHappened {
        event,
        agent: agent.id.clone(),
        harness: agent.harness.clone(),
        checkout: agent.checkout.clone(),
        detail: serde_json::Value::Object(detail),
    })
}

/// A tool's name as an event carries it: the characters a tool's name is
/// made of, bounded. What else a harness put there stays in its journal.
fn scrubbed_tool(tool: &str) -> String {
    tool.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.'))
        .take(TOOL_MAX)
        .collect()
}

/// `path` relative to `checkout` when it names something inside it, and
/// nothing otherwise: an event never carries a path outside the agent's
/// checkout.
fn inside_checkout(checkout: &Path, path: &str) -> Option<String> {
    let path = Path::new(path);
    let relative = if path.is_absolute() {
        path.strip_prefix(checkout).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    let inside = relative
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)));
    (inside && !relative.as_os_str().is_empty()).then(|| relative.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> BridgedAgent {
        BridgedAgent {
            id: "a1".into(),
            harness: "harness".into(),
            label: "fix".into(),
            space: "uze".into(),
            checkout: PathBuf::from("/work/uze"),
            pane: PaneId(1),
            skill_prefix: Some("/"),
        }
    }

    fn report(tool: &str, path: &str) -> uze_application::AgentReport {
        uze_application::AgentReport {
            event: "tool.started".into(),
            at: 7,
            tool: Some(tool.into()),
            path: Some(path.into()),
            call: Some("c1".into()),
            outcome: None,
        }
    }

    #[test]
    fn a_report_keeps_only_a_path_inside_the_checkout_and_a_scrubbed_tool() {
        let inside = happened_of(&agent(), report("Edit", "/work/uze/src/main.rs")).unwrap();
        assert_eq!(inside.event, AgentEvent::ToolStarted);
        assert_eq!(inside.detail["path"], "src/main.rs");
        assert_eq!(inside.detail["tool"], "Edit");
        assert_eq!(inside.detail["call"], "c1");
        let outside = happened_of(&agent(), report("Bash $(rm -rf ~)", "/etc/passwd")).unwrap();
        assert!(outside.detail.get("path").is_none());
        assert_eq!(outside.detail["tool"], "Bashrm-rf");
        let climbing = happened_of(&agent(), report("Read", "../other/secret")).unwrap();
        assert!(climbing.detail.get("path").is_none());
        let unknown = uze_application::AgentReport {
            event: "session.exploded".into(),
            ..report("x", "y")
        };
        assert!(happened_of(&agent(), unknown).is_none());
    }

    #[test]
    fn a_turn_is_dated_from_its_report_until_it_finishes() {
        let mut bridge = Bridge::default();
        let turn = |event, detail| AgentHappened {
            event,
            agent: "a1".into(),
            harness: "harness".into(),
            checkout: PathBuf::from("/work/uze"),
            detail,
        };
        note_turn(
            &mut bridge,
            &turn(AgentEvent::TurnStarted, serde_json::json!({ "at": 7 })),
        );
        assert_eq!(bridge.turn_began.get("a1"), Some(&7));
        note_turn(
            &mut bridge,
            &turn(AgentEvent::TurnFinished, serde_json::json!({ "at": 9 })),
        );
        assert!(bridge.turn_began.is_empty());
        note_turn(
            &mut bridge,
            &turn(
                AgentEvent::TurnStarted,
                serde_json::json!({ "source": "paint" }),
            ),
        );
        assert!(bridge.turn_began["a1"] > 7, "a painted turn is dated now");
    }
}
