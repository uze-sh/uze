//! What a declared action does once a gesture reaches it: resolved against
//! the item it was pressed on, asked first when it says so (or when what it
//! would copy is long), then run.
//!
//! What runs is the manifest's, never the answer's: an envelope can ask
//! sources again and say something, and only a button or a key reaches
//! `clipboard`, `open-path` and `open`.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;
use uze_contract::{ActionDecl, Answer, Asking, Clean, Does, Envelope, Question, Then};

use super::{DeclaredExtension, RecordedAction};
use crate::contract::{Job, Opaque, Outcome};
use crate::view::{Command, Confirm, Role, Span, ViewHit};

/// The longest text copied on one gesture without asking first, in bytes.
pub const COPY_UNASKED_MAX: usize = 256;

/// A gesture's action, resolved against its item and ready to run.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Run {
    /// Ask the host to run the action's script with `input`.
    Script {
        action: String,
        label: String,
        input: Value,
        checkout: Option<PathBuf>,
        /// Its second step, and the scope its templates read beside the
        /// answer.
        then: Option<(Then, Value)>,
    },
    Refresh(Vec<String>),
    Copy(String),
    OpenPath {
        project: PathBuf,
        target: PathBuf,
    },
    Open(String),
    /// Hand the host something to do to an agent, which it shows first.
    Agent(crate::contract::AgentRequest),
}

/// An action waiting on the operator's answer, drawn as its view's
/// question.
#[derive(Clone, Debug)]
pub(super) struct Pending {
    pub surface: String,
    pub run: Run,
    pub confirm: Confirm,
}

/// An action's script, waiting to be handed to the host.
#[derive(Clone, Debug)]
pub(super) struct Queued {
    pub surface: String,
    pub action: String,
    pub label: String,
    pub input: Value,
    pub checkout: PathBuf,
    pub then: Option<(Then, Value)>,
}

/// The host's answer to an action's script.
pub(super) struct ActionAnswer {
    surface: String,
    label: String,
    answer: Answer,
    then: Option<(Then, Value)>,
}

impl DeclaredExtension {
    /// The declared `action` reached by a gesture on `surface`, with the
    /// `item` it was pressed on: recorded for the inspector, and run once
    /// resolved, or asked about first. One the manifest does not declare
    /// is recorded and runs nothing.
    pub(super) fn act_on(&mut self, surface: &str, action: &str, item: Value) -> Outcome {
        self.act_with(surface, action, item, Value::Null)
    }

    /// [`DeclaredExtension::act_on`], with the values a form sent, by
    /// field id (`null` from anything but a form).
    pub(super) fn act_with(
        &mut self,
        surface: &str,
        action: &str,
        item: Value,
        form: Value,
    ) -> Outcome {
        let declared = self.compiled.manifest.action(action).cloned();
        self.recorded.push(RecordedAction {
            surface: surface.to_owned(),
            action: action.to_owned(),
            item: item.clone(),
            declared: declared.is_some(),
        });
        let Some(declared) = declared else {
            let said = Clean::label(&format!("{action} recorded, not run"));
            return Outcome::Notice(Span::clean(said, Role::Muted));
        };
        let scope = self.reading_scope(surface, &declared, &item, &form);
        let run = match self.resolve(surface, &declared, &item, &scope) {
            Ok(run) => run,
            Err(outcome) => return outcome,
        };
        match self.question_for(surface, &declared, &scope, &run) {
            Some(confirm) => self.ask_first(surface, &declared, run, confirm),
            None => self.execute(surface, run),
        }
    }

    /// What `declared` would do on `item`, every template resolved. An
    /// `open-path` leaving the checkout is refused here, before anything is
    /// asked.
    fn resolve(
        &self,
        surface: &str,
        declared: &ActionDecl,
        item: &Value,
        scope: &Value,
    ) -> Result<Run, Outcome> {
        let label = Clean::label(&declared.label).into_string();
        if item.is_null() && reads_the_item(declared) {
            return Err(refused(&label, "select a row first"));
        }
        Ok(match &declared.does {
            Does::Script(_) | Does::Run(_) => Run::Script {
                action: declared.id.clone(),
                label,
                input: action_input(item, scope),
                checkout: self.checkout_of(surface),
                then: declared.then.clone().map(|then| (then, scope.clone())),
            },
            Does::Refresh(sources) => Run::Refresh(sources.clone()),
            Does::Clipboard(template) => {
                let text =
                    Clean::text(&self.resolve_scoped(surface, template, scope)).into_string();
                if text.trim().is_empty() {
                    return Err(refused(&label, "there is nothing to copy"));
                }
                Run::Copy(text)
            }
            Does::OpenPath(template) => {
                let written = self.resolve_scoped(surface, template, scope);
                let Some(project) = self.checkout_of(surface) else {
                    return Err(refused(&label, "no checkout is in front to open it in"));
                };
                let Some(relative) = inside_checkout(&written) else {
                    return Err(refused(
                        &label,
                        &format!("`{written}` is not a path inside the checkout"),
                    ));
                };
                Run::OpenPath {
                    target: project.join(relative),
                    project,
                }
            }
            Does::Open(target) => Run::Open(target.clone()),
            Does::AgentPrompt { prompt, target, .. } => {
                let what = match prompt {
                    uze_contract::PromptText::Text(text) => {
                        let text =
                            Clean::text(&self.resolve_scoped(surface, text, scope)).into_string();
                        if text.trim().is_empty() {
                            return Err(refused(&label, "there is nothing to say"));
                        }
                        crate::contract::AgentAsk::Prompt { text }
                    }
                    uze_contract::PromptText::Skill { skill, args } => {
                        crate::contract::AgentAsk::Skill {
                            skill: skill.clone(),
                            args: args
                                .iter()
                                .map(|arg| {
                                    Clean::label(&self.resolve_scoped(surface, arg, scope))
                                        .into_string()
                                })
                                .collect(),
                        }
                    }
                };
                Run::Agent(self.agent_request(surface, what, target, scope, label)?)
            }
            Does::AgentRestart { target } => Run::Agent(self.agent_request(
                surface,
                crate::contract::AgentAsk::Restart,
                target,
                scope,
                label,
            )?),
            Does::FocusAgent { target } => Run::Agent(self.agent_request(
                surface,
                crate::contract::AgentAsk::Focus,
                target,
                scope,
                label,
            )?),
        })
    }

    /// `what`, aimed at the agent `target` names: the surface's own, or the
    /// one whose id the template resolves to against `scope`.
    pub(super) fn agent_request(
        &self,
        surface: &str,
        what: crate::contract::AgentAsk,
        target: &uze_contract::AgentTarget,
        scope: &Value,
        label: String,
    ) -> Result<crate::contract::AgentRequest, Outcome> {
        let agent = match target {
            uze_contract::AgentTarget::Own => None,
            uze_contract::AgentTarget::Named(template) => {
                let id = self.resolve_scoped(surface, template, scope);
                if id.trim().is_empty() {
                    return Err(refused(&label, "it names no agent"));
                }
                Some(id)
            }
        };
        Ok(crate::contract::AgentRequest {
            what,
            agent,
            label,
            unattended: false,
        })
    }

    /// The question asked before `run`, when `declared` asks one or what
    /// it copies is over one line or [`COPY_UNASKED_MAX`] bytes.
    fn question_for(
        &self,
        surface: &str,
        declared: &ActionDecl,
        scope: &Value,
        run: &Run,
    ) -> Option<Confirm> {
        // What is typed into an agent is shown by the host, which also
        // confirms a restart: asking here too would ask twice.
        if matches!(run, Run::Agent(_)) {
            return None;
        }
        let title = match (&declared.confirm, run) {
            (Some(question), _) => self.resolve_scoped(surface, question, scope),
            (None, Run::Copy(text)) if text.lines().count() > 1 => {
                format!("Copy {} lines?", text.lines().count())
            }
            (None, Run::Copy(text)) if text.len() > COPY_UNASKED_MAX => {
                format!("Copy {} bytes?", text.len())
            }
            (None, _) => return None,
        };
        Some(Confirm {
            title: Clean::label(&title).into_string(),
            subject: self.compiled.identity.to_string(),
            body: self.arguments(declared, run),
            confirm: Clean::label(&declared.label).into_string(),
            on_confirm: false,
            field: None,
        })
    }

    /// What `run` will do, as the question shows it.
    fn arguments(&self, declared: &ActionDecl, run: &Run) -> String {
        let said = match (run, &declared.does) {
            (Run::Script { input, .. }, Does::Script(file)) => match &input["item"] {
                Value::Null => format!("Runs {file}"),
                item => format!("Runs {file} on {item}"),
            },
            (Run::Refresh(sources), _) => format!("Asks {} again", sources.join(", ")),
            (Run::Copy(text), _) => return Clean::text(text).into_string(),
            (Run::OpenPath { project, target }, _) => format!(
                "Opens {}",
                target.strip_prefix(project).unwrap_or(target).display()
            ),
            (Run::Open(surface), _) => format!("Opens {}", self.surface_title(surface)),
            (Run::Agent(request), _) => request.label.clone(),
            (Run::Script { .. }, _) => String::new(),
        };
        Clean::label(&said).into_string()
    }

    /// Holds `run` until it is answered, in the view that asks: the
    /// surface itself, or, for a section or a drawer, which have no room
    /// for a question, the view the action is offered on or the
    /// extension's first.
    fn ask_first(
        &mut self,
        surface: &str,
        declared: &ActionDecl,
        run: Run,
        confirm: Confirm,
    ) -> Outcome {
        let is_view = |id: &str| {
            self.compiled
                .manifest
                .surface(id)
                .is_some_and(|decl| decl.slot == uze_contract::Slot::View)
        };
        let asking = if is_view(surface) {
            surface.to_owned()
        } else if is_view(&declared.on) {
            declared.on.clone()
        } else {
            match self
                .compiled
                .manifest
                .surfaces
                .iter()
                .find(|decl| decl.slot == uze_contract::Slot::View)
            {
                Some(view) => view.id.clone(),
                None => {
                    return refused(
                        &Clean::label(&declared.label).into_string(),
                        "it asks first, and the extension has no view to ask in",
                    );
                }
            }
        };
        let shown = asking != surface;
        self.pending = Some(Pending {
            surface: asking.clone(),
            run,
            confirm,
        });
        if shown {
            Outcome::Show(asking)
        } else {
            Outcome::Stay
        }
    }

    /// Runs what was resolved.
    pub(super) fn execute(&mut self, surface: &str, run: Run) -> Outcome {
        match run {
            Run::Script {
                action,
                label,
                input,
                checkout,
                then,
            } => {
                let Some(checkout) = checkout else {
                    return refused(&label, "no checkout is in front to run it in");
                };
                let said = format!("{label}…");
                self.queued.push(Queued {
                    surface: surface.to_owned(),
                    action,
                    label,
                    input,
                    checkout,
                    then,
                });
                Outcome::Notice(Span::clean(Clean::label(&said), Role::Muted))
            }
            Run::Refresh(sources) => {
                self.refresh(&sources, None);
                Outcome::Stay
            }
            Run::Copy(text) => Outcome::Copy(text),
            Run::OpenPath { project, target } => Outcome::OpenPath { project, target },
            Run::Open(target) => Outcome::Show(target),
            Run::Agent(request) => Outcome::Agent(request),
        }
    }

    /// Answers the question standing on `surface`, when one does: an
    /// accepted one runs.
    pub(super) fn answer(&mut self, surface: &str, accepted: bool) -> Outcome {
        let Some(pending) = self.pending.take_if(|pending| pending.surface == surface) else {
            return Outcome::Stay;
        };
        if accepted {
            self.execute(surface, pending.run)
        } else {
            Outcome::Stay
        }
    }

    /// A command while a question stands on `surface`: the keyboard moves
    /// between the two answers, and only an answer closes it. `None` when
    /// nothing is asked there.
    pub(super) fn command_while_asking(
        &mut self,
        surface: &str,
        command: &Command,
    ) -> Option<Outcome> {
        let pending = self
            .pending
            .as_mut()
            .filter(|pending| pending.surface == surface)?;
        Some(match command {
            Command::Activate => {
                let accepted = pending.confirm.on_confirm;
                self.answer(surface, accepted)
            }
            Command::Close => self.answer(surface, false),
            Command::FocusNext
            | Command::Collapse
            | Command::Expand
            | Command::NextView
            | Command::PreviousView
            | Command::SelectNext
            | Command::SelectPrevious => {
                pending.confirm.on_confirm = !pending.confirm.on_confirm;
                Outcome::Stay
            }
            _ => Outcome::Stay,
        })
    }

    /// A hit while a question stands on `surface`: only its answers reach
    /// anything.
    pub(super) fn hit_while_asking(&mut self, surface: &str, hit: &ViewHit) -> Option<Outcome> {
        self.pending
            .as_ref()
            .filter(|pending| pending.surface == surface)?;
        Some(match hit {
            ViewHit::Answer(accepted) => self.answer(surface, *accepted),
            _ => Outcome::Stay,
        })
    }

    /// The jobs running the actions' scripts queued since the last ask.
    pub(super) fn action_jobs(&mut self) -> Vec<Job> {
        let identity = self.compiled.identity.clone();
        std::mem::take(&mut self.queued)
            .into_iter()
            .map(|queued| {
                let question = Question {
                    identity: identity.clone(),
                    declaration: queued.action,
                    asking: Asking::Action,
                    input: queued.input,
                    checkout: Some(queued.checkout),
                };
                let (surface, label, then) = (queued.surface, queued.label, queued.then);
                let (stopped_surface, stopped_label) = (surface.clone(), label.clone());
                Job {
                    run: Box::new(move |host| {
                        Opaque::new(ActionAnswer {
                            surface,
                            label,
                            answer: host.answer(question),
                            then,
                        })
                    }),
                    fallback: Box::new(move || {
                        Opaque::new(ActionAnswer {
                            surface: stopped_surface,
                            label: stopped_label,
                            answer: Answer::Failed {
                                reason: super::STOPPED.to_owned(),
                                trace: Vec::new(),
                            },
                            then: None,
                        })
                    }),
                }
            })
            .collect()
    }

    /// What an action's script answered: its envelope applied to the
    /// surface it was pressed on, or why it did not run, as a toast.
    pub(super) fn absorb_action(&mut self, answered: ActionAnswer) -> Outcome {
        let ActionAnswer {
            surface,
            label,
            answer,
            then,
        } = answered;
        self.notices.remove(&surface);
        match answer {
            Answer::Answered { envelope, .. } => {
                let applied = self.apply(&envelope, std::slice::from_ref(&surface), None);
                match then {
                    Some((then, scope)) => {
                        self.second_step(&surface, &label, &then, scope, &envelope.data)
                    }
                    None => applied,
                }
            }
            Answer::Failed { reason, .. } => Outcome::Failed {
                title: Clean::label(&format!("{label} failed")).into_string(),
                detail: Clean::label(&reason).into_string(),
            },
            Answer::Withheld { reason } => Outcome::Failed {
                title: Clean::label(&format!("{label} did not run")).into_string(),
                detail: Clean::label(&reason).into_string(),
            },
        }
    }

    /// The prompt a two-step action's answer fills: its `{answer.*}` read
    /// from what the first step answered, cleaned, and handed to the host,
    /// which shows it before anything is typed. It stands on the gesture
    /// that ran the first step, so it is never unattended.
    fn second_step(
        &self,
        surface: &str,
        label: &str,
        then: &Then,
        mut scope: Value,
        answer: &Value,
    ) -> Outcome {
        if let Value::Object(fields) = &mut scope {
            fields.insert("answer".to_owned(), answer.clone());
        }
        let what = match &then.prompt {
            uze_contract::PromptText::Text(text) => {
                let text = Clean::text(&self.resolve_scoped(surface, text, &scope)).into_string();
                if text.trim().is_empty() {
                    return refused(label, "its answer left nothing to say");
                }
                crate::contract::AgentAsk::Prompt { text }
            }
            uze_contract::PromptText::Skill { skill, args } => crate::contract::AgentAsk::Skill {
                skill: skill.clone(),
                args: args
                    .iter()
                    .map(|arg| {
                        Clean::label(&self.resolve_scoped(surface, arg, &scope)).into_string()
                    })
                    .collect(),
            },
        };
        match self.agent_request(surface, what, &then.target, &scope, label.to_owned()) {
            Ok(request) => Outcome::Agent(request),
            Err(outcome) => outcome,
        }
    }

    /// What an action's templates read: its item's fields (and `item`),
    /// the form values it was sent, and the `ui.<id>` values its `with`
    /// names, as `form` and `ui`.
    fn reading_scope(
        &self,
        surface: &str,
        declared: &ActionDecl,
        item: &Value,
        form: &Value,
    ) -> Value {
        let mut scope = template_scope(item);
        let ui = self.ui_read(surface, declared);
        if let Value::Object(fields) = &mut scope {
            if !form.is_null() {
                fields.insert("form".to_owned(), form.clone());
            }
            if !ui.is_empty() {
                fields.insert("ui".to_owned(), Value::Object(ui));
            }
        }
        scope
    }

    /// The `ui.<id>` values `declared` reads, as the view holds them.
    fn ui_read(&self, surface: &str, declared: &ActionDecl) -> serde_json::Map<String, Value> {
        let Some((_, view)) = self
            .declaration(surface)
            .or_else(|| self.declaration(&declared.on))
        else {
            return serde_json::Map::new();
        };
        let held = self.ui.get(surface).or_else(|| self.ui.get(&declared.on));
        super::bind::effective_ui(view, held)
            .into_iter()
            .filter(|(id, _)| declared.reads_ui(id))
            .map(|(id, value)| (id, Value::String(Clean::text(&value).into_string())))
            .collect()
    }

    /// What an envelope says beside its data: the sources to ask again,
    /// the notice the `surfaces` showing it say, a notification or a cue,
    /// and the toast. A source
    /// that answered (`from`) is never asked again by its own answer.
    pub(super) fn apply(
        &mut self,
        envelope: &Envelope,
        surfaces: &[String],
        from: Option<&str>,
    ) -> Outcome {
        self.refresh(&envelope.refresh, from);
        if let Some(notice) = &envelope.notice {
            let said = Span::clean(Clean::label(notice), Role::Info);
            for surface in surfaces {
                self.notices.insert(surface.clone(), said.clone());
            }
        }
        self.ask_to_notify(envelope, surfaces);
        match &envelope.toast {
            Some(toast) => Outcome::Toast(Clean::label(toast).into_string()),
            None => Outcome::Stay,
        }
    }

    /// Asks `sources` again, past any answer the host remembers, once a
    /// surface shows each. `from` is the source whose answer asked.
    pub(super) fn refresh(&mut self, sources: &[String], from: Option<&str>) {
        for source in sources {
            if Some(source.as_str()) == from || self.compiled.manifest.source(source).is_none() {
                continue;
            }
            self.asked.remove(source);
            self.stale.insert(source.clone());
        }
    }

    /// The checkout `surface` is about: the one an open view was opened
    /// on, or the one in front.
    fn checkout_of(&self, surface: &str) -> Option<PathBuf> {
        self.opened
            .get(surface)
            .cloned()
            .or_else(|| self.in_front.clone())
    }
}

/// Whether what `declared` resolves (what it copies or opens, and the
/// question it asks first) reads the item it is pressed on. A script's
/// input is its own to read, so a script that asks nothing about the row
/// runs on none.
fn reads_the_item(declared: &ActionDecl) -> bool {
    let done: Vec<&String> = match &declared.does {
        Does::Clipboard(template) | Does::OpenPath(template) => vec![template],
        Does::Run(program) => program.argv.iter().skip(1).collect(),
        Does::AgentPrompt { prompt, target, .. } => {
            let mut read: Vec<&String> = match prompt {
                uze_contract::PromptText::Text(text) => vec![text],
                uze_contract::PromptText::Skill { args, .. } => args.iter().collect(),
            };
            if let uze_contract::AgentTarget::Named(template) = target {
                read.push(template);
            }
            read
        }
        Does::AgentRestart {
            target: uze_contract::AgentTarget::Named(template),
        }
        | Does::FocusAgent {
            target: uze_contract::AgentTarget::Named(template),
        } => vec![template],
        Does::AgentRestart { .. } | Does::FocusAgent { .. } => Vec::new(),
        Does::Script(_) | Does::Refresh(_) | Does::Open(_) => Vec::new(),
    };
    done.into_iter()
        .chain(&declared.confirm)
        .any(|template| template_reads_the_item(template))
}

fn template_reads_the_item(template: &str) -> bool {
    use uze_contract::binding::{Scope, TextPart};
    let Ok(parts) = uze_contract::binding::parse_text(template) else {
        return false;
    };
    // `form.*` and `answer.*` are read beside the item, never from it.
    let beside = |path: &uze_contract::Path| {
        matches!(
            path.segments.first().map(String::as_str),
            Some("form" | "answer")
        )
    };
    parts.iter().any(|part| match part {
        TextPart::Ref(path) | TextPart::Lookup { path, .. } => {
            matches!(path.scope, Scope::Item | Scope::Parent) && !beside(path)
        }
        TextPart::Literal(_) | TextPart::Label { .. } => false,
    })
}

/// A refusal said as a toast: the gesture did nothing, and why.
fn refused(label: &str, why: &str) -> Outcome {
    Outcome::Failed {
        title: Clean::label(&format!("{label} did not run")).into_string(),
        detail: Clean::label(why).into_string(),
    }
}

/// `written` as a path beneath a checkout's root, by its words alone: not
/// empty, not absolute, and never climbing out with `..`. A link inside
/// the checkout is the checkout's own, opened as the code surface opens
/// any of its files.
pub(super) fn inside_checkout(written: &str) -> Option<PathBuf> {
    let path = Path::new(written.trim());
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!relative.as_os_str().is_empty()).then_some(relative)
}

/// What a script or a program is asked with: its item, and the form and
/// UI values it reads, when there are any.
fn action_input(item: &Value, scope: &Value) -> Value {
    let mut input = serde_json::json!({ "item": item });
    for read in ["form", "ui"] {
        if let Some(value) = scope.get(read)
            && let Value::Object(fields) = &mut input
        {
            fields.insert(read.to_owned(), value.clone());
        }
    }
    input
}

/// What a template reads as its item: the item itself, with `item` naming
/// it too, so `{name}` and `{item.name}` read the same field.
pub(super) fn template_scope(item: &Value) -> Value {
    match item {
        Value::Object(fields) if !fields.contains_key("item") => {
            let mut fields = fields.clone();
            fields.insert("item".to_owned(), item.clone());
            Value::Object(fields)
        }
        _ => serde_json::json!({ "item": item }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_inside_the_checkout_is_its_words_beneath_the_root() {
        assert_eq!(
            inside_checkout("./.planning/STATE.md"),
            Some(PathBuf::from(".planning/STATE.md"))
        );
        for outside in ["../secrets", "/etc/passwd", "a/../../b", "", "."] {
            assert_eq!(inside_checkout(outside), None, "{outside}");
        }
    }

    #[test]
    fn a_template_reads_its_item_by_name_and_as_item() {
        let scope = template_scope(&serde_json::json!({ "hash": "abc" }));
        assert_eq!(scope["hash"], "abc");
        assert_eq!(scope["item"]["hash"], "abc");
        assert_eq!(
            template_scope(&Value::Null),
            serde_json::json!({ "item": null })
        );
    }
}
