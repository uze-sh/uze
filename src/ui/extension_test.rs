//! An extension's tests, run by `uze agent plugin check`: each surface
//! drawn by the engine and the renderer the workspace uses, on a test
//! backend, driven by the gestures an operator could make.
//!
//! A gesture is found in what was drawn and routed as the workspace routes
//! it: a click goes to the hit rectangle the renderer registered under the
//! text, a key through the keymap in the declared scope. So a passing test
//! means the workspace draws and answers the same way.

mod snapshot;

use std::collections::BTreeMap;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use uze_application::{CheckedExtension, ExtensionDiagnostic, UzeApplication};
use uze_contract::{GivenData, Slot, StepKind, TestDecl, ThemeName};
use uze_extensions::contract::Extension as _;
use uze_extensions::declared::{Absence, DeclaredExtension, TraceEntry, TraceNode};
use uze_extensions::view::{Size, ViewHit};
use uze_keys::{Action, Front, Resolution, Scope};

use crate::ui::extension_view::{
    FooterKeys, NavigatorFrame, SectionState, command_for, render, render_docked, render_section,
    tree::{attribution, render_attribution},
};
use uze_theme::Token;

/// The size a test draws a surface at when it says none.
fn default_size(slot: Slot) -> (u16, u16) {
    match slot {
        Slot::View | Slot::Overlay => (100, 30),
        Slot::Section => (30, 16),
        Slot::Drawer => (46, 30),
        Slot::Status => (30, 1),
        Slot::Bar => (40, 1),
        Slot::Band => (100, 2),
    }
}

const DEFAULT_LOCALE: &str = "en";

/// Unicode's box-drawing block: the frame a drawer is drawn in, which a
/// test reading wrapped text skips. Read, never drawn, so no `Symbol`.
const BOX_DRAWING: std::ops::RangeInclusive<u32> = 0x2500..=0x257f;

/// What running one extension's tests found.
#[derive(Debug, Default)]
pub struct TestRun {
    pub passed: usize,
    pub failures: Vec<ExtensionDiagnostic>,
    /// Snapshots written by `--update-snapshots`, by path in the extension.
    pub written: Vec<String>,
}

/// Runs every test `checked` decoded. With `update`, a snapshot that is
/// missing or differs is written instead of failing.
pub fn run_tests(app: &UzeApplication, checked: &CheckedExtension, update: bool) -> TestRun {
    let mut run = TestRun::default();
    let Some(compiled) = &checked.compiled else {
        return run;
    };
    for (file, tests) in &checked.tests {
        for test in &tests.tests {
            let now_ms = test.world.now_ms.unwrap_or(0);
            let ran = crate::ui::extension_view::tree::stopped_at(now_ms, || {
                run_one(app, checked, compiled, file, test, update, &mut run.written)
            });
            match ran {
                Ok(()) => run.passed += 1,
                Err(failure) => run.failures.push(failure),
            }
        }
    }
    run
}

fn run_one(
    app: &UzeApplication,
    checked: &CheckedExtension,
    compiled: &uze_contract::CompiledExtension,
    file: &str,
    test: &TestDecl,
    update: bool,
    written: &mut Vec<String>,
) -> Result<(), ExtensionDiagnostic> {
    let fail = |offset: usize, why: String| {
        let (line, column) = checked.place(file, offset).unwrap_or((0, 0));
        ExtensionDiagnostic {
            file: file.to_owned(),
            line,
            column,
            message: format!("test \"{}\": {why}", test.name),
            suggestion: None,
        }
    };
    let world = &test.world;
    let Some(surface) = compiled.manifest.surface(&world.surface) else {
        let declared: Vec<String> = compiled
            .manifest
            .surfaces
            .iter()
            .map(|surface| format!("`{}`", surface.id))
            .collect();
        return Err(fail(
            test.span.offset,
            format!(
                "there is no surface `{}`; the manifest declares {}",
                world.surface,
                declared.join(", ")
            ),
        ));
    };
    let mut data = checked.fixtures.clone();
    for given in &world.given {
        let value = match &given.data {
            GivenData::Json(value) => value.clone(),
            GivenData::Fixture(fixture) => app
                .extensions()
                .fixture(&checked.dir, fixture)
                .map_err(|why| fail(given.span.offset, why.to_string()))?,
        };
        data.insert(given.source.clone(), value);
    }
    let theme = world.theme.unwrap_or(ThemeName::Dark);
    let (width, height) = world.size.unwrap_or_else(|| default_size(surface.slot));
    // The extension's keys are the workspace's: its table registered over
    // the operator's keymap, as an enabled extension's is there.
    let table = crate::keymap::extension_table(compiled);
    let owner = table.owner.clone();
    uze_keys::register_extension(table);
    let footer = uze_keys::active()
        .extension_actions(Front {
            owner: &owner,
            surface: &surface.id,
        })
        .into_iter()
        .filter(|(_, chord)| chord.is_some())
        .map(|(action, _)| action)
        .chain([Action::ToggleInspector])
        .collect();
    let mut stage = Stage {
        engine: DeclaredExtension::new(
            compiled.clone(),
            world.locale.as_deref().unwrap_or(DEFAULT_LOCALE),
        ),
        surface: surface.id.clone(),
        slot: surface.slot,
        identity: compiled.identity.clone(),
        owner,
        footer,
        theme,
        width,
        height,
        drawn: None,
    };
    for (source, value) in data {
        stage.engine.set_data(source, Ok(value));
    }
    for step in &test.steps {
        stage.draw();
        let at = step.span.offset;
        match &step.kind {
            StepKind::ExpectText(text) => {
                if !stage.shows(text) {
                    return Err(fail(
                        at,
                        format!("`{text}` is not drawn{}", stage.drawn_rows()),
                    ));
                }
            }
            StepKind::ExpectNoText(text) => {
                if stage.shows(text) {
                    return Err(fail(at, format!("`{text}` is drawn{}", stage.drawn_rows())));
                }
            }
            StepKind::Click(text) | StepKind::PressButton(text) => {
                let hit = stage.hit_on(text).ok_or_else(|| {
                    fail(
                        at,
                        format!(
                            "nothing drawn as `{text}` answers a click{}",
                            stage.drawn_rows()
                        ),
                    )
                })?;
                stage.engine.hit(&stage.surface, hit, stage.size());
            }
            StepKind::Press(chord) => {
                let parsed = uze_keys::Chord::parse(chord)
                    .map_err(|_| fail(at, format!("`{chord}` is not a key")))?;
                let front = Front {
                    owner: &stage.owner,
                    surface: &stage.surface,
                };
                let meaning =
                    uze_keys::active().resolve_for(parsed, &[Scope::Declared], Some(front));
                if let Resolution::Act(Action::Extension(id)) = meaning {
                    let action = uze_keys::extension::local(id);
                    stage.engine.press(&stage.surface, &action);
                    continue;
                }
                let command = match meaning {
                    Resolution::Act(action) => command_for(action),
                    _ => None,
                }
                .ok_or_else(|| {
                    fail(
                        at,
                        format!("`{chord}` does nothing on an extension's surface"),
                    )
                })?;
                stage.engine.command(&stage.surface, command, stage.size());
            }
            StepKind::ExpectAbsent { node, because } => {
                let Some(trace) = stage.engine.trace(&stage.surface) else {
                    return Err(fail(at, "the surface draws nothing yet".to_owned()));
                };
                let mut reasons = Vec::new();
                let found = absent_because(&trace.items, *node, because, &mut reasons)
                    || trace
                        .templates
                        .values()
                        .any(|body| absent_because(body, *node, because, &mut reasons));
                if !found {
                    let said = match reasons.is_empty() {
                        true => format!("no `{node}` is absent"),
                        false => format!(
                            "the absent `{node}`s are so because of {}",
                            reasons.join(", ")
                        ),
                    };
                    return Err(fail(
                        at,
                        format!("no `{node}` is absent because of `{because}`: {said}"),
                    ));
                }
            }
            StepKind::ExpectAction { action, args } => {
                let Some(trace) = stage.engine.trace(&stage.surface) else {
                    return Err(fail(at, "the surface draws nothing yet".to_owned()));
                };
                let matches = |recorded: &uze_extensions::declared::RecordedAction| {
                    recorded.action == *action
                        && args
                            .iter()
                            .all(|(key, value)| recorded.item.get(key) == Some(value))
                };
                if !trace.recorded.iter().any(matches) {
                    let seen: Vec<String> = trace
                        .recorded
                        .iter()
                        .map(|recorded| format!("`{}` with {}", recorded.action, recorded.item))
                        .collect();
                    let said = match seen.is_empty() {
                        true => "nothing was recorded".to_owned(),
                        false => format!("recorded: {}", seen.join("; ")),
                    };
                    return Err(fail(
                        at,
                        format!("the action `{action}` was not recorded as expected; {said}"),
                    ));
                }
            }
            StepKind::Snapshot(name) => {
                let path = format!(
                    "tests/snapshots/{name}.{}.{}x{}.{}.snap",
                    stage.slot.name(),
                    stage.width,
                    stage.height,
                    theme.as_str()
                );
                let taken = stage.snapshot();
                let target = unlinked(&checked.dir, &path).map_err(|why| fail(at, why))?;
                let held = std::fs::read_to_string(&target).ok();
                if held.as_deref() == Some(taken.as_str()) {
                    continue;
                }
                if update {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|error| fail(at, format!("cannot write {path}: {error}")))?;
                    }
                    std::fs::write(&target, &taken)
                        .map_err(|error| fail(at, format!("cannot write {path}: {error}")))?;
                    written.push(path);
                    continue;
                }
                return Err(fail(
                    at,
                    match held {
                        None => format!(
                            "there is no snapshot {path} yet; run the check with --update-snapshots to write it"
                        ),
                        Some(held) => format!(
                            "the cells differ from {path}{}",
                            snapshot::difference(&held, &taken)
                        ),
                    },
                ));
            }
        }
    }
    Ok(())
}

/// `relative` under the extension's `dir`, refused when it, or a directory
/// on the way to it, is a link: a snapshot is read and written there, and
/// a plugin's link would aim that write at a file of the operator's.
fn unlinked(dir: &std::path::Path, relative: &str) -> Result<std::path::PathBuf, String> {
    let mut at = dir.to_path_buf();
    for part in relative.split('/') {
        at.push(part);
        if std::fs::symlink_metadata(&at).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(format!(
                "{relative} is reached through a link, and a snapshot is written only inside the extension"
            ));
        }
    }
    Ok(at)
}

/// Whether a node of `kind` was not drawn because of `because`: a field a
/// `when`, `unless`, `each` or reference missed, or the UI value a `show`
/// compared. Every reason found for a node of that kind lands in `reasons`.
fn absent_because(
    nodes: &[TraceNode],
    kind: uze_contract::NodeName,
    because: &str,
    reasons: &mut Vec<String>,
) -> bool {
    nodes.iter().any(|node| {
        let ours = node.entry == TraceEntry::Node(kind);
        let here = ours
            && node.absent.iter().any(|absence| {
                let reason = match absence {
                    Absence::When { field, .. }
                    | Absence::Unless { field }
                    | Absence::Missing { field }
                    | Absence::EachMissing { field }
                    | Absence::EachEmpty { field }
                    | Absence::EachNotList { field } => field.clone(),
                    Absence::Show { id, value, .. } => format!("ui.{id}={value}"),
                    _ => return false,
                };
                let matched = reason == because || reason.trim_start_matches("ui.") == because;
                reasons.push(format!("`{reason}`"));
                matched
            });
        here || absent_because(&node.children, kind, because, reasons)
    })
}

/// One test's surface, as it stands after the steps so far.
struct Stage {
    engine: DeclaredExtension,
    surface: String,
    slot: Slot,
    identity: uze_contract::Identity,
    /// The extension's owner as the keymap names it.
    owner: String,
    /// What the host adds to its footer, as the workspace does.
    footer: Vec<Action>,
    theme: ThemeName,
    width: u16,
    height: u16,
    drawn: Option<Drawn>,
}

struct Drawn {
    buffer: Buffer,
    hits: Vec<(Rect, ViewHit)>,
}

impl Stage {
    fn size(&self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn palette(&self) -> uze_theme::Theme {
        match self.theme {
            ThemeName::Dark => uze_theme::default_theme().clone(),
            ThemeName::Light => uze_theme::builtin("light")
                .cloned()
                .unwrap_or_else(|| uze_theme::default_theme().clone()),
        }
    }

    /// Draws the surface in its slot's frame, as the workspace does.
    fn draw(&mut self) {
        let palette = self.palette();
        let drawn = uze_theme::with_active(palette, || {
            let mut terminal =
                Terminal::new(TestBackend::new(self.width, self.height)).expect("a test backend");
            let mut hits = Vec::new();
            let said = attribution(&self.identity);
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    crate::ui::widget::surface::fill(frame, area, Token::SurfaceBackground);
                    match self.slot {
                        Slot::Section => {
                            if let Some(section) = self.engine.section(&self.surface, false) {
                                let area = render_attribution(frame, area, &said);
                                let mut rows = crate::ui::Rows::over(area);
                                render_section(
                                    frame,
                                    &section,
                                    &mut rows,
                                    SectionState::default(),
                                    &mut hits,
                                );
                            }
                        }
                        Slot::Drawer => {
                            let (inner, _) =
                                crate::ui::widget::drawer::render_docked(frame, area, false, None);
                            let content = render_attribution(frame, inner, &said);
                            let space = Size {
                                width: content.width,
                                height: content.height,
                            };
                            if let Some(view) = self.engine.view(&self.surface, space) {
                                render_docked(frame, &view, content, None, &mut hits);
                            }
                        }
                        // A row of the header, an item of the bar or a band
                        // over a pane: drawn
                        // whole into its rows, attributed where the host
                        // draws it rather than in a strip of its own.
                        Slot::Status | Slot::Band | Slot::Bar => {
                            let space = Size {
                                width: area.width,
                                height: area.height,
                            };
                            if let Some(view) = self.engine.view(&self.surface, space) {
                                render_docked(frame, &view, area, None, &mut hits);
                            }
                        }
                        Slot::View | Slot::Overlay => {
                            let content = render_attribution(frame, area, &said);
                            let space = Size {
                                width: content.width,
                                height: content.height,
                            };
                            if let Some(view) = self.engine.view(&self.surface, space) {
                                render(
                                    frame,
                                    &view,
                                    content,
                                    NavigatorFrame::default(),
                                    FooterKeys {
                                        scope: Scope::Declared,
                                        hovered: None,
                                        host: &self.footer,
                                    },
                                    None,
                                    &mut hits,
                                );
                            }
                        }
                    }
                })
                .expect("a test backend draws");
            Drawn {
                buffer: terminal.backend().buffer().clone(),
                hits,
            }
        });
        self.drawn = Some(drawn);
    }

    /// Each row as text, with the column each character starts at.
    fn rows(&self) -> Vec<(String, Vec<u16>)> {
        let Some(drawn) = &self.drawn else {
            return Vec::new();
        };
        let area = drawn.buffer.area;
        (0..area.height)
            .map(|y| {
                let mut text = String::new();
                let mut columns = Vec::new();
                for x in 0..area.width {
                    let symbol = drawn.buffer[(x, y)].symbol();
                    for ch in symbol.chars() {
                        text.push(ch);
                        columns.push(x);
                    }
                }
                (text, columns)
            })
            .collect()
    }

    fn shows(&self, wanted: &str) -> bool {
        let rows = self.rows();
        if rows.iter().any(|(text, _)| text.contains(wanted)) {
            return true;
        }
        // A paragraph wraps: its words, rejoined across rows, without the
        // frame a drawer draws around them.
        let words =
            |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_owned).collect() };
        let joined: Vec<String> = rows
            .iter()
            .flat_map(|(text, _)| {
                let unframed: String = text
                    .chars()
                    .map(|ch| {
                        if BOX_DRAWING.contains(&u32::from(ch)) {
                            ' '
                        } else {
                            ch
                        }
                    })
                    .collect();
                words(&unframed)
            })
            .collect();
        joined.join(" ").contains(&words(wanted).join(" "))
    }

    /// The hit under the first place `text` is drawn, the last registered
    /// winning as the workspace's does.
    fn hit_on(&self, text: &str) -> Option<ViewHit> {
        let drawn = self.drawn.as_ref()?;
        for (y, (row, columns)) in self.rows().iter().enumerate() {
            // A control's label is drawn lowercase whatever the test
            // wrote, so a step names it in either case.
            let index = match row.find(text) {
                Some(byte) => row[..byte].chars().count(),
                None => {
                    let lower = row.to_lowercase();
                    let Some(byte) = lower.find(&text.to_lowercase()) else {
                        continue;
                    };
                    lower[..byte].chars().count()
                }
            };
            let x = *columns.get(index)?;
            let y = u16::try_from(y).ok()?;
            // The first hit drawn under the cell, as the workspace resolves
            // a click: a button inside a list's row is the button's.
            return drawn
                .hits
                .iter()
                .find(|(rect, _)| rect.contains(ratatui::layout::Position { x, y }))
                .map(|(_, hit)| *hit);
        }
        None
    }

    fn drawn_rows(&self) -> String {
        let rows: Vec<String> = self
            .rows()
            .into_iter()
            .map(|(text, _)| text.trim_end().to_owned())
            .collect();
        let last = rows
            .iter()
            .rposition(|row| !row.trim().is_empty())
            .map_or(0, |at| at + 1);
        format!("; drawn:\n{}", rows[..last].join("\n"))
    }

    fn snapshot(&self) -> String {
        let palette = self.palette();
        let drawn = self.drawn.as_ref().expect("drawn before a snapshot");
        uze_theme::with_active(palette, || {
            snapshot::take(
                &drawn.buffer,
                self.slot,
                self.width,
                self.height,
                self.theme,
            )
        })
    }
}

/// The extensions' tests a check runs, by extension id.
pub type Runs = BTreeMap<String, TestRun>;

/// What a test's click on `text` reaches on `surface`, drawn at `size` in
/// the dark theme: for proving a test and the workspace agree.
#[cfg(test)]
pub(crate) fn hit_for_text(
    compiled: &uze_contract::CompiledExtension,
    data: BTreeMap<String, serde_json::Value>,
    surface: &str,
    (width, height): (u16, u16),
    text: &str,
) -> Option<ViewHit> {
    let slot = compiled.manifest.surface(surface)?.slot;
    let mut stage = Stage {
        engine: DeclaredExtension::new(compiled.clone(), DEFAULT_LOCALE),
        surface: surface.to_owned(),
        slot,
        identity: compiled.identity.clone(),
        owner: crate::keymap::extension_owner(&compiled.identity),
        footer: vec![Action::ToggleInspector],
        theme: ThemeName::Dark,
        width,
        height,
        drawn: None,
    };
    for (source, value) in data {
        stage.engine.set_data(source, Ok(value));
    }
    stage.draw();
    stage.hit_on(text)
}
