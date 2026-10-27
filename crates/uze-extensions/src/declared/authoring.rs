//! What the inspector asks of an engine for the author of a linked
//! extension, and what it reads of any (`extension-platform` D25,
//! D28): a source asked again with edited input, a source fed by its
//! fixture instead of what answers it, a synthetic event, fake agents,
//! the events it saw, and what it holds.
//!
//! Nothing here runs anything: an edited input is asked through the same
//! job as any question, so the host's runner answers it under the same
//! consent, and a fixture's data is what the host read and hands over.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use uze_contract::EventName;

use super::{DeclaredExtension, SourceData, digest};
use crate::contract::{AgentHappened, CheckoutEvent};

/// What a node of a built view is reckoned to hold, its text included:
/// the tree is the host's type and says nothing of its own size.
const NODE_ESTIMATE: usize = std::mem::size_of::<crate::view::Node>() + 64;

/// The author's hand on an engine, kept apart from what it would do alone.
#[derive(Debug, Default)]
pub(super) struct Authoring {
    /// Sources asked with an input the author edited, until put back.
    pub(super) edited: BTreeMap<String, serde_json::Value>,
    /// Sources fed by their fixture instead of what answers them.
    pub(super) fixtured: BTreeSet<String>,
    /// Whether the agents handed to `agents` sources are the author's.
    pub(super) faking_agents: bool,
    /// The workspace's agents as last handed over while they were faked.
    pub(super) real_agents: serde_json::Value,
    /// What the compile weighs as JSON, for the generation it was taken at.
    compiled_bytes: RefCell<Option<(u64, usize)>>,
    /// What each source's data weighs, for the digest it was taken at.
    data_bytes: RefCell<BTreeMap<String, (u64, usize)>>,
}

/// The agents an author injects to see an `agents` source drawn without
/// running any.
pub fn fake_agents() -> serde_json::Value {
    serde_json::json!([
        { "agent": "fake-1", "harness": "fake", "label": "Fake agent", "status": "working", "checkout": "", "synthetic": true },
        { "agent": "fake-2", "harness": "fake", "label": "Waiting agent", "status": "waiting", "checkout": "", "synthetic": true },
        { "agent": "fake-3", "harness": "fake", "label": "Idle agent", "status": "idle", "checkout": "", "synthetic": true },
    ])
}

impl DeclaredExtension {
    /// Asks `source` again with `input` in place of what it declares, past
    /// any answer the host remembers, and keeps asking it so until
    /// [`Self::restore_input`].
    pub fn rerun(&mut self, source: &str, input: serde_json::Value) {
        if self.compiled.manifest.source(source).is_none() {
            return;
        }
        self.authoring.edited.insert(source.to_owned(), input);
        self.refresh(&[source.to_owned()], None);
    }

    /// Puts `source` back to the input it declares, asked again.
    pub fn restore_input(&mut self, source: &str) {
        if self.authoring.edited.remove(source).is_some() {
            self.refresh(&[source.to_owned()], None);
        }
    }

    /// The input the author edited `source`'s to, while it stands.
    pub fn edited_input(&self, source: &str) -> Option<&serde_json::Value> {
        self.authoring.edited.get(source)
    }

    /// The input `source` was last asked with.
    pub fn asked_input(&self, source: &str) -> Option<&serde_json::Value> {
        self.asked.get(source).map(|asked| &asked.input)
    }

    /// The input `source` is asked with now: the author's, or what it
    /// declares, resolved for `surface`.
    pub(super) fn input_now(&self, source: &str, surface: &str) -> serde_json::Value {
        match self.authoring.edited.get(source) {
            Some(input) => input.clone(),
            None => self.input_of(source, surface),
        }
    }

    /// Feeds `source` from its fixture, `data`, instead of asking what
    /// answers it, until [`Self::feed_live`].
    pub fn feed_fixture(&mut self, source: &str, data: SourceData) {
        if self.compiled.manifest.source(source).is_none() {
            return;
        }
        self.authoring.fixtured.insert(source.to_owned());
        self.asked.remove(source);
        self.stale.remove(source);
        self.set_data(source.to_owned(), data);
    }

    /// Asks `source` of what answers it again, after its fixture fed it.
    pub fn feed_live(&mut self, source: &str) {
        if self.authoring.fixtured.remove(source) {
            self.refresh(&[source.to_owned()], None);
        }
    }

    /// Whether `source` is fed by its fixture now.
    pub fn fed_by_fixture(&self, source: &str) -> bool {
        self.authoring.fixtured.contains(source)
    }

    /// Delivers `event` as though it happened, marked synthetic wherever
    /// it is shown: a checkout's event on the checkout its sources are
    /// shown on, an agent's from no agent. What an agent's event would
    /// send an agent unattended is not sent: there is no agent to send it.
    pub fn emit(&mut self, event: &EventName, now: Instant) {
        let checkout = self
            .shown_sources()
            .into_iter()
            .map(|(_, checkout, _)| checkout)
            .next()
            .or_else(|| self.in_front.clone())
            .unwrap_or_default();
        match event {
            EventName::Agent(agent) => {
                let unattended = self.unattended.len();
                self.take_agent_events(
                    &[AgentHappened {
                        event: *agent,
                        agent: "synthetic".to_owned(),
                        harness: "synthetic".to_owned(),
                        checkout,
                        detail: serde_json::json!({ "synthetic": true }),
                    }],
                    now,
                );
                self.unattended.truncate(unattended);
            }
            EventName::PathsChanged { .. } | EventName::CheckoutChanged => {
                let happened = CheckoutEvent {
                    checkout,
                    event: event.clone(),
                };
                let mut value = super::events::event_value(&happened);
                if let serde_json::Value::Object(fields) = &mut value {
                    fields.insert("synthetic".to_owned(), serde_json::Value::Bool(true));
                }
                self.gather_value(&happened, value, now);
            }
        }
    }

    /// Hands every `agents` source the author's fake agents, or the
    /// workspace's again.
    pub fn fake_agents(&mut self, fake: bool) {
        if fake == self.authoring.faking_agents {
            return;
        }
        if fake {
            self.authoring.real_agents = self.agents.clone();
            self.give_agents(&fake_agents());
            self.authoring.faking_agents = true;
        } else {
            self.authoring.faking_agents = false;
            let real = std::mem::take(&mut self.authoring.real_agents);
            self.give_agents(&real);
        }
    }

    /// Hands every `agents` source `agents`, when they differ from the
    /// last handed over.
    pub(super) fn give_agents(&mut self, agents: &serde_json::Value) {
        if *agents == self.agents {
            return;
        }
        self.agents = agents.clone();
        let sources: Vec<String> = self
            .compiled
            .manifest
            .sources
            .iter()
            .filter(|source| matches!(source.kind, uze_contract::SourceKind::Agents))
            .map(|source| source.id.clone())
            .collect();
        for source in sources {
            self.set_data(source, Ok(self.agents.clone()));
        }
    }

    /// Whether `agents` sources are handed fake agents now.
    pub fn faking_agents(&self) -> bool {
        self.authoring.faking_agents
    }

    /// The events it saw this session, newest first, synthetic ones
    /// marked `"synthetic": true`.
    pub fn events_seen(&self) -> impl Iterator<Item = &serde_json::Value> {
        self.log.iter()
    }

    /// What it holds, in bytes, reckoned: its compile and its sources'
    /// data as JSON, the events it kept and the views it built. A measure
    /// for D28's budget, not an allocator's count.
    pub fn approximate_bytes(&self) -> usize {
        let compiled = {
            let mut cached = self.authoring.compiled_bytes.borrow_mut();
            match *cached {
                Some((generation, bytes)) if generation == self.generation => bytes,
                _ => {
                    let bytes = serde_json::to_vec(&self.compiled).map_or(0, |bytes| bytes.len());
                    *cached = Some((self.generation, bytes));
                    bytes
                }
            }
        };
        let data: usize = {
            let mut weighed = self.authoring.data_bytes.borrow_mut();
            weighed.retain(|source, _| self.data.contains_key(source));
            self.data
                .iter()
                .map(|(source, data)| {
                    let now = self
                        .digests
                        .get(source)
                        .copied()
                        .unwrap_or_else(|| digest(data));
                    match weighed.get(source) {
                        Some((at, bytes)) if *at == now => *bytes,
                        _ => {
                            let bytes = match data {
                                Ok(value) => json_bytes(value),
                                Err(reason) => reason.len(),
                            };
                            weighed.insert(source.clone(), (now, bytes));
                            bytes
                        }
                    }
                })
                .sum()
        };
        let events: usize = self.log.iter().map(json_bytes).sum();
        let built: usize = self
            .memo
            .borrow()
            .values()
            .map(|memo| memo.built.count * NODE_ESTIMATE)
            .sum();
        compiled + data + events + built
    }
}

fn json_bytes(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}
