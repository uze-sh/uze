//! What the inspector beside a declared view says of its extension beyond
//! how the view was drawn (`extension-platform` D25, D28): its runs,
//! the events it saw, its grants and what it costs, for every extension;
//! and, for the author of a linked one, the tools that provoke what has
//! not happened yet: a source asked again with edited input or fed by its
//! fixture, a synthetic event, another size or theme, fake agents, and a
//! session's events recorded into a fixture.
//!
//! A rerun is asked through the engine's own job, so the runner answers it
//! under the consent any question is; reading a fixture and writing the
//! recording run off the drawing thread (`reads.rs`).

use super::*;
use uze_contract::{EventName, SourceKind};

/// The sizes an author tries a surface at, after the room it has.
const SIZES: [Option<(u16, u16)>; 4] = [None, Some((80, 24)), Some((60, 16)), Some((40, 12))];

/// The themes an author tries a surface in, after the workspace's own.
const THEMES: [Option<&str>; 3] = [None, Some("dark"), Some("light")];

/// Where the inspector records a session's events, inside the extension.
pub(in crate::ui::orchestrator) const RECORDED_FIXTURE: &str = "fixtures/session.json";

/// How often what the extensions cost is written for `uze doctor`.
pub(in crate::ui::orchestrator) const COSTS_EVERY: Duration = Duration::from_secs(30);

/// The most runs and events the inspector lists.
const LISTED: usize = 20;

/// Where the author is in the inspector, kept while it stands open.
#[derive(Default)]
pub(in crate::ui::orchestrator) struct InspectorState {
    /// Whether it holds the moves the view would otherwise take: opened by
    /// its key or clicked into, until a click lands on the view.
    pub(in crate::ui::orchestrator) keys: bool,
    /// The tool the keys are on.
    pub(in crate::ui::orchestrator) selected: usize,
    /// A source's input being edited, before it is asked again.
    pub(in crate::ui::orchestrator) edit: Option<(String, RenameBuffer)>,
    /// Into [`SIZES`].
    pub(in crate::ui::orchestrator) size: usize,
    /// Into [`THEMES`].
    pub(in crate::ui::orchestrator) theme: usize,
}

/// One thing the author of a linked extension can do from its inspector.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui::orchestrator) enum InspectorTool {
    /// Asks the source again with an input the author edits first.
    Rerun(String),
    /// Puts the source back to the input it declares.
    Restore(String),
    /// Feeds the source from `fixtures/<source>.json`, or live again.
    Fixture(String),
    Emit(EventName),
    Size,
    Theme,
    FakeAgents,
    Record,
}

/// The fixture a source is switched to: the one named after it.
pub(in crate::ui::orchestrator) fn fixture_of(source: &str) -> String {
    format!("fixtures/{source}.json")
}

impl WorkspaceModel {
    /// The open view's declared extension, its engine, and whether it was
    /// installed from a linked marketplace.
    pub(in crate::ui::orchestrator) fn inspected(
        &self,
    ) -> Option<(&ExtensionIdentity, &DeclaredExtension, bool)> {
        let slot = self.slot_in_pane()?;
        let identity = slot.owner.declared()?;
        let declared = &self.remembered.declared;
        let engine = declared.engine(&slot.owner)?.declared()?;
        let linked = declared
            .extensions
            .get(identity)
            .is_some_and(|extension| extension.installed.linked);
        Some((identity, engine, linked))
    }

    pub(in crate::ui::orchestrator) fn inspected_mut(&mut self) -> Option<&mut DeclaredExtension> {
        let owner = self.slot_in_pane()?.owner.clone();
        self.remembered
            .declared
            .engines
            .get_mut(&owner)?
            .declared_mut()
    }

    /// What the author of the open linked extension can do; nothing for an
    /// extension from any other marketplace.
    pub(in crate::ui::orchestrator) fn inspector_tools(&self) -> Vec<InspectorTool> {
        let Some((_, engine, true)) = self.inspected() else {
            return Vec::new();
        };
        let manifest = &engine.compiled().manifest;
        let mut tools = Vec::new();
        for source in &manifest.sources {
            if !matches!(source.kind, SourceKind::Script(_) | SourceKind::Run(_)) {
                continue;
            }
            tools.push(InspectorTool::Rerun(source.id.clone()));
            if engine.edited_input(&source.id).is_some() {
                tools.push(InspectorTool::Restore(source.id.clone()));
            }
            tools.push(InspectorTool::Fixture(source.id.clone()));
        }
        let mut events: Vec<&EventName> = Vec::new();
        for event in manifest
            .sources
            .iter()
            .flat_map(|source| &source.refresh_on)
        {
            if !events.contains(&event) {
                events.push(event);
            }
        }
        tools.extend(events.into_iter().cloned().map(InspectorTool::Emit));
        tools.push(InspectorTool::Size);
        tools.push(InspectorTool::Theme);
        if manifest
            .sources
            .iter()
            .any(|source| matches!(source.kind, SourceKind::Agents))
        {
            tools.push(InspectorTool::FakeAgents);
        }
        tools.push(InspectorTool::Record);
        tools
    }

    /// What a tool's row says, as it stands now.
    pub(in crate::ui::orchestrator) fn tool_label(&self, tool: &InspectorTool) -> String {
        let engine = self.inspected().map(|(_, engine, _)| engine);
        let label = match tool {
            InspectorTool::Rerun(source) => format!("rerun {source} with input…"),
            InspectorTool::Restore(source) => format!("put {source}'s input back"),
            InspectorTool::Fixture(source)
                if engine.is_some_and(|engine| engine.fed_by_fixture(source)) =>
            {
                format!("feed {source} live")
            }
            InspectorTool::Fixture(source) => {
                format!("feed {source} from {}", fixture_of(source))
            }
            InspectorTool::Emit(event) => match event {
                EventName::PathsChanged { paths } => {
                    format!("emit {} {}", event.name(), paths.join(" "))
                }
                _ => format!("emit {}", event.name()),
            },
            InspectorTool::Size => match SIZES[self.inspector.size] {
                None => "size · the room it has".to_owned(),
                Some((width, height)) => format!("size · {width} by {height}"),
            },
            InspectorTool::Theme => match THEMES[self.inspector.theme] {
                None => "theme · the workspace's".to_owned(),
                Some(name) => format!("theme · {name}"),
            },
            InspectorTool::FakeAgents if engine.is_some_and(DeclaredExtension::faking_agents) => {
                "fake agents · on".to_owned()
            }
            InspectorTool::FakeAgents => "fake agents · off".to_owned(),
            InspectorTool::Record => "record session events".to_owned(),
        };
        Clean::label(&label).into_string()
    }

    /// The size the open view is drawn at, when the author chose one.
    pub(in crate::ui::orchestrator) fn inspected_size(&self) -> Option<(u16, u16)> {
        if !self.inspecting || !self.inspected().is_some_and(|(_, _, linked)| linked) {
            return None;
        }
        SIZES[self.inspector.size]
    }

    /// The theme the open view is drawn in, when the author chose one.
    pub(in crate::ui::orchestrator) fn inspected_theme(&self) -> Option<&'static str> {
        if !self.inspecting || !self.inspected().is_some_and(|(_, _, linked)| linked) {
            return None;
        }
        THEMES[self.inspector.theme]
    }

    /// Whether the inspector holds the moves the view would take.
    pub(in crate::ui::orchestrator) fn inspector_holds_keys(&self) -> bool {
        self.inspecting && self.inspector.keys
    }

    /// Moves the inspector's keys to the next or the previous tool.
    pub(in crate::ui::orchestrator) fn move_inspector_tool(&mut self, forward: bool) {
        let count = self.inspector_tools().len();
        if count == 0 {
            return;
        }
        let selected = self.inspector.selected.min(count - 1);
        self.inspector.selected = if forward {
            (selected + 1).min(count - 1)
        } else {
            selected.saturating_sub(1)
        };
        self.dirty = true;
    }

    /// Lets go of what the author chose, as the inspector closes.
    pub(in crate::ui::orchestrator) fn reset_inspector(&mut self) {
        self.inspector = InspectorState::default();
    }

    /// The runs, events, grants and cost the inspector lists for the open
    /// extension (D25, D28).
    pub(in crate::ui::orchestrator) fn inspect_extension(&self, inspection: &mut Inspection) {
        let Some((identity, engine, _)) = self.inspected() else {
            return;
        };
        let runner = &self.remembered.declared.runner;
        inspection.runs = runner
            .runs_of(identity)
            .iter()
            .take(LISTED)
            .flat_map(run_lines)
            .collect();
        inspection.events = engine.events_seen().take(LISTED).map(event_line).collect();
        let manifest = &engine.compiled().manifest;
        let mut grants: Vec<String> = manifest.wants.iter().map(ToString::to_string).collect();
        if manifest.programs().next().is_some() {
            grants.push(
                if runner.confines() {
                    "its programs run confined"
                } else {
                    "its programs run as you: this machine does not confine them"
                }
                .to_owned(),
            );
        }
        for source in &manifest.sources {
            if let Some(reason) = engine.withheld(&source.id) {
                grants.push(format!("{} not asked: {reason}", source.id));
            }
        }
        inspection.grants = grants
            .iter()
            .map(|line| Clean::label(line).into_string())
            .collect();
        let cost = runner.cost_of(identity, Instant::now());
        let memory = engine.approximate_bytes() + cost.remembered_bytes;
        let p95 = cost.p95.map_or_else(|| "–".to_owned(), duration_text);
        inspection.cost = vec![format!(
            "{:.1} runs/min · p95 {p95} · memory {}",
            cost.runs_per_minute,
            uze_application::bytes_text(memory)
        )];
        if cost.slow() {
            inspection.cost.push(format!(
                "over {} at p95",
                duration_text(uze_application::P95_TARGET)
            ));
        }
        if memory > uze_application::MEMORY_TARGET {
            inspection.cost.push(format!(
                "over {} held",
                uze_application::bytes_text(uze_application::MEMORY_TARGET)
            ));
        }
    }

    /// What every hosted declared extension costs now, as `uze doctor`
    /// reads it.
    pub(in crate::ui::orchestrator) fn measured_costs(&self) -> uze_application::MeasuredCosts {
        let declared = &self.remembered.declared;
        let now = Instant::now();
        let extensions = declared
            .engines
            .iter()
            .filter_map(|(owner, engine)| {
                let identity = owner.declared()?;
                let engine = engine.declared()?;
                let cost = declared.runner.cost_of(identity, now);
                Some(uze_application::MeasuredCost {
                    identity: identity.clone(),
                    runs_per_minute: cost.runs_per_minute,
                    p95_ms: cost.p95.map(|p95| p95.as_secs_f64() * 1000.0),
                    memory_bytes: engine.approximate_bytes() + cost.remembered_bytes,
                })
            })
            .collect();
        uze_application::MeasuredCosts::now(extensions)
    }

    /// Takes what the inspector asked off the frame.
    pub(in crate::ui::orchestrator) fn absorb_inspector(&mut self, answer: InspectorAnswer) {
        match answer {
            InspectorAnswer::Fixture {
                extension,
                source,
                data,
            } => match data {
                Ok(data) => {
                    if let Some(engine) = self
                        .remembered
                        .declared
                        .engines
                        .get_mut(&Owner::Declared(extension))
                        .and_then(|engine| engine.declared_mut())
                    {
                        engine.feed_fixture(&source, Ok(data));
                    }
                }
                Err(why) => self.say_failed(
                    &format!("{source} not fed from its fixture"),
                    Clean::label(&why).into_string(),
                ),
            },
            InspectorAnswer::Recorded { written } => match written {
                Ok(path) => self.raise_toast(
                    ToastKind::Done,
                    "events recorded",
                    crate::ui::display_project_path(&path),
                    None,
                ),
                Err(why) => {
                    self.say_failed("Events not recorded", Clean::label(&why).into_string());
                }
            },
        }
        self.dirty = true;
    }
}

impl Attach<'_> {
    /// Does what the inspector's tool at `index` does.
    pub(in crate::ui::orchestrator) fn run_inspector_tool(&mut self, index: usize) {
        let tools = self.model.inspector_tools();
        let Some(tool) = tools.get(index).cloned() else {
            return;
        };
        self.model.inspector.selected = index;
        self.model.inspector.keys = true;
        self.model.dirty = true;
        let Some((identity, _, _)) = self.model.inspected() else {
            return;
        };
        let identity = identity.clone();
        match tool {
            InspectorTool::Rerun(source) => {
                let Some((_, engine, _)) = self.model.inspected() else {
                    return;
                };
                let input = engine
                    .edited_input(&source)
                    .or_else(|| engine.asked_input(&source))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let text = serde_json::to_string(&input).unwrap_or_default();
                self.model.inspector.edit = Some((source, RenameBuffer::new(text)));
            }
            InspectorTool::Restore(source) => {
                if let Some(engine) = self.model.inspected_mut() {
                    engine.restore_input(&source);
                }
            }
            InspectorTool::Fixture(source) => {
                let fed = self
                    .model
                    .inspected()
                    .is_some_and(|(_, engine, _)| engine.fed_by_fixture(&source));
                if fed {
                    if let Some(engine) = self.model.inspected_mut() {
                        engine.feed_live(&source);
                    }
                    return;
                }
                let Some(dir) = self
                    .model
                    .remembered
                    .declared
                    .extensions
                    .get(&identity)
                    .map(|extension| extension.installed.dir.clone())
                else {
                    return;
                };
                spawn_inspector_fixture(
                    self.home,
                    identity,
                    dir,
                    source,
                    self.channels.inspector.sender.clone(),
                );
            }
            InspectorTool::Emit(event) => {
                if let Some(engine) = self.model.inspected_mut() {
                    engine.emit(&event, Instant::now());
                }
            }
            InspectorTool::Size => {
                self.model.inspector.size = (self.model.inspector.size + 1) % SIZES.len();
            }
            InspectorTool::Theme => {
                self.model.inspector.theme = (self.model.inspector.theme + 1) % THEMES.len();
            }
            InspectorTool::FakeAgents => {
                if let Some(engine) = self.model.inspected_mut() {
                    let fake = !engine.faking_agents();
                    engine.fake_agents(fake);
                }
            }
            InspectorTool::Record => {
                let Some((_, engine, _)) = self.model.inspected() else {
                    return;
                };
                let mut events: Vec<serde_json::Value> = engine.events_seen().cloned().collect();
                if events.is_empty() {
                    self.model
                        .say_failed("Events not recorded", "nothing happened yet".to_owned());
                    return;
                }
                // Oldest first, as they happened.
                events.reverse();
                spawn_inspector_record(
                    self.home,
                    identity,
                    serde_json::Value::Array(events),
                    self.channels.inspector.sender.clone(),
                );
            }
        }
    }

    /// A key while a source's input is edited: Enter asks the source again
    /// with it, Esc puts it away, and the rest edits it.
    pub(in crate::ui::orchestrator) fn inspector_edit_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                let Some((source, buffer)) = self.model.inspector.edit.as_ref() else {
                    return;
                };
                let source = source.clone();
                match serde_json::from_str::<serde_json::Value>(buffer.text()) {
                    Ok(input @ serde_json::Value::Object(_)) => {
                        self.model.inspector.edit = None;
                        if let Some(engine) = self.model.inspected_mut() {
                            engine.rerun(&source, input);
                        }
                    }
                    Ok(_) => self.model.say_failed(
                        &format!("{source} not asked again"),
                        "its input is a JSON object, `{…}`".to_owned(),
                    ),
                    Err(error) => self.model.say_failed(
                        &format!("{source} not asked again"),
                        format!("the input is not JSON: {error}"),
                    ),
                }
            }
            Action::Dismiss => self.model.inspector.edit = None,
            _ => {
                if let Some((_, buffer)) = self.model.inspector.edit.as_mut() {
                    match action {
                        Action::EraseBack => buffer.erase_back(),
                        Action::EraseForward => buffer.erase_forward(),
                        Action::CaretLeft => buffer.left(),
                        Action::CaretRight => buffer.right(),
                        Action::CaretLineStart => buffer.home(),
                        Action::CaretLineEnd => buffer.end(),
                        _ => {}
                    }
                }
            }
        }
        self.model.dirty = true;
    }

    /// Writes what the extensions cost for `uze doctor` on
    /// [`COSTS_EVERY`], while any declared extension is hosted.
    pub(in crate::ui::orchestrator) fn schedule_extension_costs(&mut self) {
        let now = Instant::now();
        let declared = &self.model.remembered.declared;
        let due = declared
            .costs_written_at
            .is_none_or(|at| now.duration_since(at) >= COSTS_EVERY);
        let hosting = declared
            .engines
            .keys()
            .any(|owner| owner.declared().is_some());
        let writing = declared
            .costs_writing
            .as_ref()
            .is_some_and(|writer| !writer.is_finished());
        if !due || !hosting || writing {
            return;
        }
        self.model.remembered.declared.costs_written_at = Some(now);
        self.model.remembered.declared.costs_writing = Some(spawn_extension_costs(
            self.home,
            self.model.measured_costs(),
        ));
    }
}

/// A run as the inspector lists it: what was asked, how long it took and
/// how it ended, then the input it was asked with.
fn run_lines(run: &uze_application::ExtensionRun) -> [String; 2] {
    use uze_application::RunOutcome;
    let asked = match run.asking {
        uze_contract::Asking::Source => "source",
        uze_contract::Asking::Refresh => "refresh",
        uze_contract::Asking::Action => "action",
        uze_contract::Asking::Resident(_) => "start",
    };
    let ended = match &run.outcome {
        RunOutcome::Answered => "answered".to_owned(),
        RunOutcome::Remembered => "remembered".to_owned(),
        RunOutcome::Fixture => "fixture".to_owned(),
        RunOutcome::Failed(reason) => format!("failed: {reason}"),
        RunOutcome::Withheld(reason) => format!("not run: {reason}"),
    };
    [
        Clean::label(&format!(
            "{asked} {} · {} · {ended}",
            run.declaration,
            duration_text(run.took)
        ))
        .into_string(),
        Clean::label(&format!("  input {} · {}", run.input, run.digest)).into_string(),
    ]
}

/// An event as the inspector lists it: its name, what it names, and that
/// it was synthetic when it was.
fn event_line(event: &serde_json::Value) -> String {
    let name = event["name"].as_str().unwrap_or("event");
    let mut line = name.to_owned();
    if let Some(paths) = event["paths"].as_array() {
        let paths: Vec<&str> = paths.iter().filter_map(serde_json::Value::as_str).collect();
        line.push(' ');
        line.push_str(&paths.join(" "));
    }
    if let Some(agent) = event["agent"].as_str() {
        line.push_str(&format!(" · {agent}"));
    }
    if event["synthetic"] == serde_json::Value::Bool(true) {
        line.push_str(" · synthetic");
    }
    Clean::label(&line).into_string()
}

/// A duration as the inspector says it: tenths of a millisecond below a
/// second.
fn duration_text(took: Duration) -> String {
    let millis = took.as_secs_f64() * 1000.0;
    if millis < 1000.0 {
        format!("{millis:.1} ms")
    } else {
        format!("{:.1} s", millis / 1000.0)
    }
}
