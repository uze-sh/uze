//! `action "id" label="…" on="surface" { … }`: what an extension offers the
//! operator to do, and the one thing it does.
//!
//! ```kdl
//! action "refresh" label="Refresh" on="board" key="ctrl-r" { refresh "board"; }
//! action "archive" label="Archive" on="board" key="ctrl-d" confirm="Archive {name}?" {
//!     script "archive.uze"
//! }
//! action "review" label="Review next" on="board" {
//!     run "./bin/next.sh"
//!     with "ui.sort" "form.*"
//!     then { agent-prompt "Review {answer.file}" target="self"; }
//! }
//! ```
//!
//! `with` names the host-held values the action reads beside its item
//! (D24); `then` is the second step of a `run` or a `script`: a prompt
//! whose `{answer.*}` reads what the first step answered, sent only on the
//! gesture that ran it and shown first (D16).

use kdl::{KdlDocument, KdlNode};
use uze_contract::{ActionDecl, Clean, Does, KeySuggestion, Then};

use super::{
    declared_id, prop, referred_id, relative_file, required_prop, string_args, string_prop,
    template, text,
};
use crate::capability::extension::document::{Place, Report, entries, refuse_extra};

const PROPS: &[&str] = &["label", "on", "key", "confirm"];

/// Where an action's parts were written.
pub struct Places {
    /// Its `on=`.
    pub on: usize,
    /// Its `key=`, when it suggests one.
    pub key: Option<usize>,
    /// The node that says what it does.
    pub does: usize,
    /// Parallel to the ids `does` names: each `refresh` source, or the
    /// surface `open` names.
    pub targets: Vec<usize>,
}

pub fn decode(node: &KdlNode, report: &mut Report<'_>) -> Option<(ActionDecl, Places)> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, PROPS, true, report);
    let id = declared_id(node, parts.args.first().copied(), report);
    let label = required_prop(node, &parts, "label", report);
    let on = required_prop(node, &parts, "on", report)
        .filter(|(on, at)| referred_id(on, "surface", *at, report));
    let key = prop(&parts, "key").and_then(|entry| {
        let written = string_prop("key", entry, report)?;
        let parsed = KeySuggestion::parse(&written);
        if parsed.is_none() {
            report.on(
                entry,
                format!("`{written}` is not a key"),
                Some(
                    "write modifiers then the key, joined by `-`: `ctrl-r`, `alt-shift-x`, `f5`"
                        .to_owned(),
                ),
            );
        }
        parsed.map(|key| (key, entry.offset()))
    });
    let confirm = prop(&parts, "confirm").and_then(|entry| {
        string_prop("confirm", entry, report)
            .filter(|question| template(question, entry.offset(), report))
    });
    let mut does: Option<(Does, usize, Vec<usize>)> = None;
    let mut written = 0;
    let mut with: Option<Vec<String>> = None;
    let mut then: Option<(Option<Then>, usize)> = None;
    for child in node.children().map(KdlDocument::nodes).unwrap_or_default() {
        match child.name().value() {
            "with" if with.is_some() => {
                report.on(
                    child,
                    "an action takes one `with`",
                    Some("list every value in it".to_owned()),
                );
                continue;
            }
            "with" => {
                with = Some(self::with(child, report));
                continue;
            }
            "then" if then.is_some() => {
                report.on(child, "an action takes one `then`", None);
                continue;
            }
            "then" => {
                then = Some((self::then(child, report), child.offset()));
                continue;
            }
            _ => {}
        }
        // `script` is no action's: `what` refuses it, saying to `call`.
        if !Does::NAMES.contains(&child.name().value()) && child.name().value() != "script" {
            let known: Vec<&str> = Does::NAMES
                .iter()
                .copied()
                .chain(["with", "then"])
                .collect();
            super::unknown_child(child, "an action", &known, report);
            continue;
        }
        written += 1;
        if written > 1 {
            report.on(
                child,
                "an action does one thing",
                Some("keep one, or declare another action".to_owned()),
            );
            continue;
        }
        does = what(child, report).map(|(what, targets)| (what, child.offset(), targets));
    }
    if written == 0 {
        report.on(
            node,
            "this action declares nothing to do",
            Some(format!(
                "add one of {} in a `{{ … }}`",
                Does::NAMES
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        );
    }
    let (does, does_at, targets) = does?;
    let then = match then {
        None => None,
        Some((then, at)) => {
            if !matches!(does, Does::Run(_) | Does::Call { .. }) {
                report.at(
                    at,
                    "only an action that runs something has a `then`: it reads what the run answered",
                    Some("give it `run` or `call`, or send the prompt with `agent-prompt` alone".to_owned()),
                );
                return None;
            }
            Some(then?)
        }
    };
    let (label, _) = label?;
    let (on, on_at) = on?;
    Some((
        ActionDecl {
            id: id?,
            label: Clean::label(&label).into_string(),
            on,
            key: key.as_ref().map(|(key, _)| key.clone()),
            confirm: confirm.map(|question| Clean::label(&question).into_string()),
            does,
            with: with.unwrap_or_default(),
            then,
        },
        Places {
            on: on_at,
            key: key.map(|(_, at)| at),
            does: does_at,
            targets,
        },
    ))
}

/// `with "ui.<id>" "form.<id>" "form.*"`: the host-held values an action
/// reads beside its item, each named whole.
fn with(child: &KdlNode, report: &mut Report<'_>) -> Vec<String> {
    string_args(child, report)
        .into_iter()
        .filter(|(read, at)| {
            let named = read == "form.*"
                || read
                    .strip_prefix("ui.")
                    .or_else(|| read.strip_prefix("form."))
                    .is_some_and(uze_contract::binding::is_field_name);
            if !named {
                report.at(
                    *at,
                    format!("`{read}` is not a value an action reads"),
                    Some("write `ui.<id>`, `form.<id>` or `form.*`".to_owned()),
                );
            }
            named
        })
        .map(|(read, _)| read)
        .collect()
}

/// `then { agent-prompt "…{answer.field}…" target=… }`: one prompt, sent
/// on the gesture that ran the action, never on an event.
fn then(child: &KdlNode, report: &mut Report<'_>) -> Option<Then> {
    let parts = entries(child, report);
    refuse_extra(child, &parts, 0, &[], true, report);
    let steps = child.children().map(KdlDocument::nodes).unwrap_or_default();
    let [step] = steps else {
        report.on(
            child,
            "`then` holds one `agent-prompt`",
            Some(
                "write `then { agent-prompt \"Review {answer.file}\" target=\"self\"; }`"
                    .to_owned(),
            ),
        );
        return None;
    };
    if step.name().value() != "agent-prompt" {
        super::unknown_child(step, "`then`", &["agent-prompt"], report);
        return None;
    }
    if let Some(entry) = step
        .entries()
        .iter()
        .find(|entry| entry.name().map(kdl::KdlIdentifier::value) == Some("when"))
    {
        report.on(
            entry,
            "a prompt made from an answer is never sent unattended: its text is not fixed",
            Some("remove `when`; it is sent on the gesture that runs the action".to_owned()),
        );
        return None;
    }
    match agent_prompt(step, report)? {
        Does::AgentPrompt { prompt, target, .. } => Some(Then { prompt, target }),
        _ => None,
    }
}

/// `target="self"`, or a `{field}` naming an agent's id from the `agents`
/// source.
fn target(
    node: &KdlNode,
    parts: &crate::capability::extension::document::Entries<'_>,
    report: &mut Report<'_>,
) -> Option<uze_contract::AgentTarget> {
    let Some(entry) = prop(parts, "target") else {
        report.on(
            node,
            "an agent action names its `target`",
            Some(
                "write `target=\"self\"`, or `target=\"{id}\"` from the `agents` source".to_owned(),
            ),
        );
        return None;
    };
    let written = string_prop("target", entry, report)?;
    if written == "self" {
        return Some(uze_contract::AgentTarget::Own);
    }
    if !written.starts_with('{') || !template(&written, entry.offset(), report) {
        report.on(
            entry,
            format!(
                "`{written}` is not an agent: write `self`, or a `{{field}}` holding an agent's id"
            ),
            None,
        );
        return None;
    }
    Some(uze_contract::AgentTarget::Named(written))
}

/// `agent-prompt "text" target=…`, or `agent-prompt skill="…" "<arg>" …
/// target=…`, and `when="agent.turn.finished"` for one sent unattended,
/// whose text and arguments are then fixed: nothing an answer, an event or
/// the repository says becomes a prompt (D16).
fn agent_prompt(child: &KdlNode, report: &mut Report<'_>) -> Option<Does> {
    let parts = entries(child, report);
    refuse_extra(
        child,
        &parts,
        usize::MAX,
        &["target", "skill", "when"],
        false,
        report,
    );
    let target = target(child, &parts, report)?;
    let args: Vec<String> = parts
        .args
        .iter()
        .filter_map(|arg| super::string_arg(child, Some(arg), report))
        .collect();
    for (arg, entry) in args.iter().zip(&parts.args) {
        if !template(arg, entry.offset(), report) {
            return None;
        }
    }
    let skill = match prop(&parts, "skill") {
        None => None,
        Some(entry) => {
            let skill = string_prop("skill", entry, report)?;
            if !uze_contract::manifest::is_skill_name(&skill) {
                report.on(
                    entry,
                    format!("`{skill}` is not a skill: a skill is named `<plugin>:<skill>`"),
                    Some(
                        "a harness's own commands are not skills, and an extension cannot type one"
                            .to_owned(),
                    ),
                );
                return None;
            }
            Some(skill)
        }
    };
    let prompt = match skill {
        Some(skill) => uze_contract::PromptText::Skill { skill, args },
        None => {
            let [text] = args.as_slice() else {
                report.on(
                    child,
                    "`agent-prompt` types one text, or a `skill=` with its arguments",
                    Some("write `agent-prompt \"Review PLAN.md\" target=\"self\"`".to_owned()),
                );
                return None;
            };
            if text.trim().is_empty() {
                report.on(child, "an agent prompt says something", None);
                return None;
            }
            uze_contract::PromptText::Text(text.clone())
        }
    };
    let auto = match prop(&parts, "when") {
        None => None,
        Some(entry) => {
            let written = string_prop("when", entry, report)?;
            let Some(on) =
                uze_contract::AgentEvent::parse(&written).filter(|_| written.starts_with("agent."))
            else {
                report.on(entry, format!("`{written}` is not an agent event"), None);
                return None;
            };
            let fixed = match &prompt {
                uze_contract::PromptText::Text(text) => !text.contains('{'),
                uze_contract::PromptText::Skill { args, .. } => {
                    args.iter().all(|arg| !arg.contains('{'))
                }
            };
            if !fixed {
                report.on(
                    entry,
                    "a prompt sent unattended is fixed: it may not read a `{field}`",
                    Some("write its text out, or send it on a gesture instead".to_owned()),
                );
                return None;
            }
            Some(uze_contract::AutoPrompt {
                on,
                values: Vec::new(),
            })
        }
    };
    Some(Does::AgentPrompt {
        prompt,
        target,
        auto,
    })
}

/// The names a `call` may not take: a source's entry, and a resident's,
/// which run in a session a call never enters (`extension-components` D4).
const NOT_CALLED: &[&str] = &[
    uze_contract::SOURCE_ENTRY,
    "start",
    "event",
    "input",
    "render",
    "stop",
];

/// `call "prev"` or `call "prev" in="sheet.uze"`.
fn call(child: &KdlNode, report: &mut Report<'_>) -> Option<Does> {
    let parts = entries(child, report);
    refuse_extra(child, &parts, 1, &["in"], false, report);
    let function = super::string_arg(child, parts.args.first().copied(), report)?;
    let is_name = function
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && function
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !is_name {
        report.on(
            child,
            format!("`{function}` is not a function's name"),
            None,
        );
        return None;
    }
    if NOT_CALLED.contains(&function.as_str()) {
        report.on(
            child,
            format!("`{function}` is an entry the host calls, not one an action calls"),
            Some("give the function the action runs a name of its own".to_owned()),
        );
        return None;
    }
    let file = match super::prop(&parts, "in") {
        None => None,
        Some(entry) => {
            let file = super::string_prop("in", entry, report)?;
            if !relative_file(
                &file,
                uze_contract::component::SUFFIX,
                entry.offset(),
                report,
            ) {
                return None;
            }
            Some(file)
        }
    };
    Some(Does::Call { function, file })
}

fn what(child: &KdlNode, report: &mut Report<'_>) -> Option<(Does, Vec<usize>)> {
    let at = child.offset();
    match child.name().value() {
        "script" => {
            // One way to run an action's code: a named function
            // (`extension-components` D10). A source's `answer` is asked
            // again with `refresh`.
            let named = text(child, report).unwrap_or_default();
            report.at(
                at,
                "an action runs a function of a component with `call`, not `script`",
                Some(format!(
                    "write `call \"<fn>\" in=\"{named}\"` and name the function there; to ask a \
                     source again, `refresh \"<source>\"`"
                )),
            );
            None
        }
        "call" => call(child, report).map(|does| (does, Vec::new())),
        "refresh" => {
            let ids: Vec<(String, usize)> = string_args(child, report)
                .into_iter()
                .filter(|(id, id_at)| referred_id(id, "source", *id_at, report))
                .collect();
            if ids.is_empty() {
                return None;
            }
            let (ids, places) = ids.into_iter().unzip();
            Some((Does::Refresh(ids), places))
        }
        "clipboard" => text(child, report)
            .filter(|copied| template(copied, at, report))
            .map(|copied| {
                (
                    Does::Clipboard(Clean::text(&copied).into_string()),
                    Vec::new(),
                )
            }),
        "run" => super::program(child, report).map(|program| (Does::Run(program), Vec::new())),
        "agent-prompt" => agent_prompt(child, report).map(|does| (does, Vec::new())),
        "agent-restart" | "focus-agent" => {
            let parts = entries(child, report);
            refuse_extra(child, &parts, 0, &["target"], false, report);
            let target = target(child, &parts, report)?;
            Some((
                if child.name().value() == "agent-restart" {
                    Does::AgentRestart { target }
                } else {
                    Does::FocusAgent { target }
                },
                Vec::new(),
            ))
        }
        "open-path" => text(child, report)
            .filter(|path| template(path, at, report))
            .map(|path| {
                (
                    Does::OpenPath(Clean::label(&path).into_string()),
                    Vec::new(),
                )
            }),
        _ => {
            let parts = entries(child, report);
            refuse_extra(child, &parts, 1, &[], false, report);
            let arg = parts.args.first().copied();
            let surface = super::string_arg(child, arg, report)?;
            let surface_at = arg.map_or(at, Place::offset);
            referred_id(&surface, "surface", surface_at, report)
                .then(|| (Does::Open(surface), vec![surface_at]))
        }
    }
}
