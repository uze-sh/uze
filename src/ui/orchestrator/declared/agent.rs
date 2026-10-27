//! What an extension asks to do to an agent (`extension-agent-bridge`, B5): taken by
//! the host, which shows a prompt before typing it, writes only through
//! `submit_notice` into a ready agent whose harness holds its pane, and
//! limits how often. Nothing is queued: a write that may not happen now is
//! refused with the reason.
//!
//! An unattended prompt (sent by an agent's event) is asked about once
//! per extension and agent; once allowed, it is typed for 24 hours without
//! asking, under the same guards and tighter rates. The pause key holds
//! every write and resets every count.

use std::collections::VecDeque;

use uze_extensions::contract::{AgentAsk, AgentRequest};

use crate::ui::orchestrator::*;
use crate::ui::widget::ToastKind;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};

/// How long allowing an extension's unattended prompts to an agent lasts.
const UNATTENDED_FOR: Duration = Duration::from_secs(24 * 60 * 60);
/// One write per extension and agent this often.
const PAIR_EVERY: Duration = Duration::from_secs(60);
/// Writes to one agent: three a minute, twenty an hour.
const AGENT_PER_MINUTE: usize = 3;
const AGENT_PER_HOUR: usize = 20;
/// Between two unattended writes to one agent.
const UNATTENDED_EVERY: Duration = Duration::from_secs(10);
/// The characters a prompt may not lead with: each is a harness's own
/// command syntax, which only a skill's spelling may use.
const COMMAND_LEADS: &[char] = &['!', '/', '#', '@', '$'];
/// The longest prompt sent with nobody pressing anything (*Product
/// decisions owed*, the unattended prompt rate).
const UNATTENDED_MAX: usize = 512;

/// A write the operator saw and allowed, or an unattended one allowed
/// before, waiting for the session to type it.
#[derive(Clone, Debug)]
pub(in crate::ui::orchestrator) struct Write {
    pub(in crate::ui::orchestrator) owner: Owner,
    pub(in crate::ui::orchestrator) agent: String,
    pub(in crate::ui::orchestrator) text: String,
    pub(in crate::ui::orchestrator) label: String,
    pub(in crate::ui::orchestrator) unattended: bool,
}

/// What extensions may still write.
#[derive(Default)]
pub(in crate::ui::orchestrator) struct Writes {
    pub(in crate::ui::orchestrator) paused: bool,
    /// Allowed, waiting for the session.
    pub(in crate::ui::orchestrator) allowed: Vec<Write>,
    /// Until when each extension may prompt each agent unattended.
    unattended_until: BTreeMap<(Owner, String), Instant>,
    rates: Rates,
}

/// The counts the rates are kept by.
#[derive(Default)]
struct Rates {
    to_agent: BTreeMap<String, VecDeque<Instant>>,
    last_unattended: BTreeMap<String, Instant>,
    /// Per extension and agent: when it last wrote, and in which turn.
    by_pair: BTreeMap<(Owner, String), (Instant, u64)>,
}

impl Rates {
    /// Why a write of `owner` to `agent` in turn `turn` may not happen
    /// now, or `None` when it may.
    fn refusal(
        &self,
        owner: &Owner,
        agent: &str,
        turn: u64,
        unattended: bool,
        now: Instant,
    ) -> Option<&'static str> {
        if let Some((at, written_turn)) = self.by_pair.get(&(owner.clone(), agent.to_owned())) {
            if *written_turn == turn {
                return Some("one prompt a turn");
            }
            if now.saturating_duration_since(*at) < PAIR_EVERY {
                return Some("one prompt a minute");
            }
        }
        let written = self.to_agent.get(agent);
        let within = |span: Duration| {
            written.map_or(0, |written| {
                written
                    .iter()
                    .filter(|at| now.saturating_duration_since(**at) < span)
                    .count()
            })
        };
        if within(Duration::from_secs(60)) >= AGENT_PER_MINUTE {
            return Some("three prompts a minute to one agent");
        }
        if within(Duration::from_secs(3600)) >= AGENT_PER_HOUR {
            return Some("twenty prompts an hour to one agent");
        }
        if unattended
            && self
                .last_unattended
                .get(agent)
                .is_some_and(|at| now.saturating_duration_since(*at) < UNATTENDED_EVERY)
        {
            return Some("ten seconds between prompts sent on their own");
        }
        None
    }

    fn note(&mut self, owner: &Owner, agent: &str, turn: u64, unattended: bool, now: Instant) {
        let written = self.to_agent.entry(agent.to_owned()).or_default();
        written.retain(|at| now.saturating_duration_since(*at) < Duration::from_secs(3600));
        written.push_back(now);
        if unattended {
            self.last_unattended.insert(agent.to_owned(), now);
        }
        self.by_pair
            .insert((owner.clone(), agent.to_owned()), (now, turn));
    }
}

/// The question before a prompt is typed: the exact text, the agent, the
/// extension; or, for an unattended one, whether it may prompt this agent
/// on its own.
pub(in crate::ui::orchestrator) struct AgentWriteDialog {
    write: Write,
    agent_label: String,
    on_allow: bool,
}

impl WorkspaceModel {
    /// Takes an extension's request about an agent.
    pub(in crate::ui::orchestrator) fn ask_of_agent(
        &mut self,
        owner: Owner,
        request: AgentRequest,
    ) {
        let by = attribution(&owner);
        let refuse = |model: &mut Self, reason: &str| {
            model.raise_toast(
                ToastKind::Told,
                "not sent",
                format!("{by}: {}: {reason}", request.label),
                None,
            );
        };
        if self.remembered.declared.bridge.writes.paused && !matches!(request.what, AgentAsk::Focus)
        {
            return refuse(self, "extensions are paused (F8)");
        }
        let Some(agent) = request.agent.clone().or_else(|| self.agent_in_front()) else {
            return refuse(self, "no agent is in front");
        };
        let Some(seen) = self
            .remembered
            .declared
            .bridge
            .agents_seen()
            .iter()
            .find(|seen| seen.id == agent)
            .cloned()
        else {
            return refuse(self, "that agent is not open");
        };
        if !self.reaches_agent_in(&owner, &seen.checkout) {
            return refuse(self, "it is not enabled in that agent's project");
        }
        let text = match &request.what {
            AgentAsk::Focus => {
                let tab = self
                    .tabs()
                    .find(|tab| tab.pane.id == seen.pane)
                    .map(|tab| tab.id);
                self.agent_focus = tab;
                return;
            }
            AgentAsk::Restart => {
                return refuse(
                    self,
                    "restarting an agent from an extension is not available yet",
                );
            }
            AgentAsk::Prompt { text } => text.clone(),
            AgentAsk::Skill { skill, args } => match seen.skill_prefix {
                Some(prefix) => uze_application::skill_invocation(prefix, skill, args),
                None => return refuse(self, "its harness has no way to invoke a skill by name"),
            },
        };
        let skill = matches!(request.what, AgentAsk::Skill { .. });
        if let Err(reason) = strictly_text(&text, skill) {
            return refuse(self, reason);
        }
        if request.unattended && text.len() > UNATTENDED_MAX {
            return refuse(self, "a prompt sent on its own is 512 bytes at most");
        }
        let write = Write {
            owner: owner.clone(),
            agent: agent.clone(),
            text,
            label: request.label.clone(),
            unattended: request.unattended,
        };
        let writes = &mut self.remembered.declared.bridge.writes;
        if request.unattended
            && writes
                .unattended_until
                .get(&(owner, agent))
                .is_some_and(|until| Instant::now() < *until)
        {
            writes.allowed.push(write);
            return;
        }
        if self.agent_write.is_some() {
            return refuse(self, "another prompt is waiting for your answer");
        }
        self.agent_write = Some(AgentWriteDialog {
            write,
            agent_label: format!("{} · {}", seen.label, seen.space),
            on_allow: false,
        });
        self.dirty = true;
    }

    /// The agent of the tab in front, when it is one UZE launched.
    fn agent_in_front(&self) -> Option<String> {
        let tab = self.session.as_ref()?.selected_tab();
        launched_agent_id(tab).map(str::to_owned)
    }

    /// Takes the answer off the question: allowing hands the write to the
    /// session, and an unattended one is allowed for a day.
    pub(in crate::ui::orchestrator) fn answer_agent_write(&mut self, allow: bool) {
        self.dirty = true;
        let Some(dialog) = self.agent_write.take() else {
            return;
        };
        if !allow {
            return;
        }
        let writes = &mut self.remembered.declared.bridge.writes;
        if dialog.write.unattended {
            writes.unattended_until.insert(
                (dialog.write.owner.clone(), dialog.write.agent.clone()),
                Instant::now() + UNATTENDED_FOR,
            );
        }
        writes.allowed.push(dialog.write);
    }

    /// Pauses every extension's writes, or lets them go again; either way
    /// every count starts over.
    pub(in crate::ui::orchestrator) fn toggle_agent_writes(&mut self) {
        let writes = &mut self.remembered.declared.bridge.writes;
        writes.paused = !writes.paused;
        writes.rates = Rates::default();
        writes.allowed.clear();
        let paused = writes.paused;
        self.raise_toast(
            ToastKind::Told,
            if paused {
                "extensions paused"
            } else {
                "extensions resumed"
            },
            if paused {
                "no extension types into an agent until F8 again"
            } else {
                "extensions may prompt agents again"
            },
            None,
        );
    }

    /// Why `write` may not be typed into `pane` now, or `None` when it
    /// may: the agent ready, its harness in front of the pane, no approval
    /// waiting, nothing typed, and within the rates.
    fn write_refusal(&self, write: &Write, pane: PaneId, harness_in_front: bool) -> Option<String> {
        let bridge = &self.remembered.declared.bridge;
        if bridge.writes.paused {
            return Some("extensions are paused (F8)".to_owned());
        }
        if !harness_in_front {
            return Some("its harness is not what runs in its pane".to_owned());
        }
        if bridge.awaiting_approval.contains(&write.agent) {
            return Some("it is waiting on an approval".to_owned());
        }
        if let Some((title, _)) = self.may_submit(pane).refusal(&write.label) {
            return Some(title.to_owned());
        }
        let turn = bridge.turns.get(&write.agent).copied().unwrap_or(0);
        bridge
            .writes
            .rates
            .refusal(
                &write.owner,
                &write.agent,
                turn,
                write.unattended,
                Instant::now(),
            )
            .map(str::to_owned)
    }
}

impl Attach<'_> {
    /// Types every allowed write that may be typed now, and says why of
    /// every other, which is dropped: a write is never held for later.
    pub(in crate::ui::orchestrator) fn type_agent_writes(&mut self) {
        let allowed = std::mem::take(&mut self.model.remembered.declared.bridge.writes.allowed);
        for write in allowed {
            let by = attribution(&write.owner);
            let tab = self
                .model
                .tabs()
                .find(|tab| launched_agent_id(tab) == Some(write.agent.as_str()));
            let Some((pane, harness_in_front)) =
                tab.map(|tab| (tab.pane.id, agent_for_tab(&self.identities, tab).is_some()))
            else {
                continue;
            };
            if let Some(reason) = self.model.write_refusal(&write, pane, harness_in_front) {
                self.model.raise_toast(
                    ToastKind::Told,
                    "not sent",
                    format!("{by}: {}: {reason}", write.label),
                    None,
                );
                continue;
            }
            self.submit_notice(pane, &write.text);
            self.model
                .note_agent_prompt_submission(pane, &self.identities, None);
            let bridge = &mut self.model.remembered.declared.bridge;
            let turn = bridge.turns.get(&write.agent).copied().unwrap_or(0);
            bridge.writes.rates.note(
                &write.owner,
                &write.agent,
                turn,
                write.unattended,
                Instant::now(),
            );
            self.model.raise_toast(
                ToastKind::Done,
                "sent to agent",
                format!("{by}: {}", crate::ui::widget::text::elide(&write.text, 32)),
                None,
            );
        }
    }

    /// A key while the question is open: it answers, moves between the
    /// answers, or does nothing.
    pub(in crate::ui::orchestrator) fn agent_write_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                let allow = self
                    .model
                    .agent_write
                    .as_ref()
                    .is_some_and(|open| open.on_allow);
                self.model.answer_agent_write(allow);
            }
            Action::Dismiss => self.model.answer_agent_write(false),
            Action::FocusNext | Action::FocusPrevious => {
                if let Some(open) = self.model.agent_write.as_mut() {
                    open.on_allow = !open.on_allow;
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on the question nothing, anywhere else it puts it away.
    pub(in crate::ui::orchestrator) fn agent_write_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::AgentWriteAnswer(allow)) => self.model.answer_agent_write(allow),
            Some(WorkspaceHit::AgentWriteBody) => {}
            _ => self.model.answer_agent_write(false),
        }
    }

    /// Puts the agent an extension asked for in front.
    pub(in crate::ui::orchestrator) fn follow_agent_focus(&mut self) {
        if let Some(tab) = self.model.agent_focus.take() {
            self.model.put_away_space_overview();
            self.asked_for_a_tab = true;
            self.model.acknowledge_completed_tab(tab);
            let _ = send_request(&mut self.stream, &ClientRequest::SelectTab { tab });
        }
    }
}

/// The question, drawn over everything but the toasts: the exact text,
/// which agent it goes to, and which extension asks.
pub(in crate::ui::orchestrator) fn render_agent_write(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &AgentWriteDialog,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    crate::ui::widget::scrim::render(frame, area);
    let (title, body, confirm) = if open.write.unattended {
        (
            format!("Let {} prompt this agent on its own?", open.write.label),
            vec![
                format!("To {}, on its events, for a day, typing:", open.agent_label),
                open.write.text.clone(),
                "A few times an hour at most. F8 pauses every extension.".to_owned(),
            ],
            "Allow for a day",
        )
    } else {
        (
            format!("Type this into {}?", open.agent_label),
            vec![open.write.text.clone()],
            "Type it",
        )
    };
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Caution,
            title: &title,
            subject: Some(Line::from(attribution(&open.write.owner))),
            body,
            confirm: Some(confirm),
            focus: Some(if open.on_allow { 1 } else { CANCEL }),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::AgentWriteAnswer(false),
        WorkspaceHit::AgentWriteAnswer(true),
    );
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::AgentWriteBody)]),
    );
}

/// Who asks, as every surface of a third party is attributed.
fn attribution(owner: &Owner) -> String {
    match owner.declared() {
        Some(identity) => crate::ui::extension_view::tree::attribution(identity),
        None => owner.switch_id(),
    }
}

/// `text` as it may be typed into an agent, or why not (D16): one line of
/// text, leading with none of a harness's command characters unless it is
/// the harness's own spelling of a skill.
fn strictly_text(text: &str, skill: bool) -> std::result::Result<(), &'static str> {
    if text.trim().is_empty() {
        return Err("there is nothing to type");
    }
    if text.chars().any(char::is_control) {
        return Err("it holds a control character or a line break");
    }
    if !skill && text.trim_start().starts_with(COMMAND_LEADS) {
        return Err("it begins with a harness's command character");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> Owner {
        Owner::Builtin("probe")
    }

    #[test]
    fn a_prompt_is_one_line_of_text_unless_it_is_a_skill() {
        assert!(strictly_text("Review PLAN.md", false).is_ok());
        assert!(strictly_text("/clear", false).is_err());
        assert!(strictly_text("  !rm -rf ~", false).is_err());
        assert!(strictly_text("one\ntwo", false).is_err());
        assert!(strictly_text("\u{1b}[2J", false).is_err());
        assert!(strictly_text("/gsd:execute-phase 12", true).is_ok());
    }

    #[test]
    fn the_rates_hold_one_a_turn_one_a_minute_and_three_a_minute_per_agent() {
        let mut rates = Rates::default();
        let start = Instant::now();
        let other = Owner::Builtin("other");
        let third = Owner::Builtin("third");
        let fourth = Owner::Builtin("fourth");
        assert_eq!(rates.refusal(&owner(), "a1", 1, false, start), None);
        rates.note(&owner(), "a1", 1, false, start);
        let later = start + Duration::from_secs(61);
        assert_eq!(
            rates.refusal(&owner(), "a1", 1, false, later),
            Some("one prompt a turn")
        );
        let soon = start + Duration::from_secs(5);
        assert_eq!(
            rates.refusal(&owner(), "a1", 2, false, soon),
            Some("one prompt a minute")
        );
        assert_eq!(rates.refusal(&owner(), "a1", 2, false, later), None);
        rates.note(&other, "a1", 2, false, soon);
        rates.note(&third, "a1", 2, false, soon);
        assert_eq!(
            rates.refusal(&fourth, "a1", 2, false, soon),
            Some("three prompts a minute to one agent")
        );
        assert_eq!(
            rates.refusal(&fourth, "a2", 1, false, soon),
            None,
            "another agent counts apart"
        );
        rates.note(&fourth, "a2", 1, true, soon);
        assert_eq!(
            rates.refusal(&owner(), "a2", 2, true, soon + Duration::from_secs(3)),
            Some("ten seconds between prompts sent on their own")
        );
    }
}
