//! The management surface — Overview, Plugins, Extensions, Harnesses,
//! Profiles, Keys and Settings — drawn as a modal over the workspace
//! client (`super::orchestrator`) rather than as a mode beside it. The
//! workspace owns the frame, the event loop and the terminal session; this
//! module owns what the modal keeps between openings
//! ([`ManagementMemory`]), what one opening is ([`TuiModel`]), and how it
//! is drawn into the rectangle the workspace hands it ([`render_modal`]).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use uze_application::{FirstStepsLayout, ManagementLayout, UzeHome};

use super::hit::Hit;
use super::keys::KeyboardSupport;
use super::model::{self, Focus, Overlay, Remembered, Route, Status, TuiModel};
use super::worker::{
    Intent, WorkerResult, dispatch, drain_worker_results, spawn_refresh, spawn_startup,
};
use super::{overlay, view};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{
    self, Edge, Rule, Surface,
    footer::Leading,
    hint::Hints,
    modal,
    tabs::{self, Tab},
};

/// How long a resolution of the machine stands for before opening the
/// modal re-resolves it. The window exists for one case: the session's
/// own warm-up has just answered and the operator opens the modal right
/// after it, which should show that answer rather than immediately ask
/// the same question again. Past it, opening the modal is a claim about
/// the machine *now* — a `uze add` run in one of the workspace's own panes
/// happened outside anything this client would hear about.
pub(crate) const RESOLUTION_STANDS_FOR: Duration = Duration::from_secs(30);

/// What the modal keeps between openings, owned by `super::run` for the
/// whole session the way the workspace's own
/// [`super::orchestrator::WorkspaceMemory`] is. The modal opens and closes
/// constantly; without this, each opening started from nothing. The
/// screen and the drawers are not here: they outlive the process, in the
/// `ClientLayout` `super::run` owns.
pub(crate) struct ManagementMemory {
    /// The channel every management worker answers on. Session-lived
    /// rather than per-opening, which is what lets the resolution start
    /// before the modal exists and lets an answer outlive the opening
    /// that asked for it, instead of dying with a dropped receiver.
    sender: Sender<WorkerResult>,
    receiver: Receiver<WorkerResult>,
    /// The last opening's resolved machine state and place in it, or
    /// `None` before the first.
    remembered: Option<Remembered>,
    /// Whether a worker still owes this session an answer. Carried across
    /// openings because the channel is: a refresh the operator closed the
    /// modal on still lands, and reopening must not ask a second time.
    in_flight: bool,
    /// The project the last resolution was about. A remembered answer is
    /// only an answer about the project it was asked for, so opening the
    /// modal over a different one asks again however recent it was.
    resolved_for: Option<PathBuf>,
}

impl ManagementMemory {
    /// A session's management memory, already resolving the machine on a
    /// thread of its own.
    ///
    /// Seeding the default plugins and applying the official snapshot's
    /// pending updates is what *opening uze* does — once, here, rather
    /// than on the first opening of the modal. Started before the
    /// workspace client even attaches, so the answer is normally waiting
    /// by the time anyone asks for it, and the work never sits in front
    /// of the operator as an empty list under a "refreshing" line.
    pub(crate) fn warming(home: &UzeHome) -> Self {
        let memory = Self::unresolved();
        // The launch directory, because the workspace has not attached
        // yet and there is no space to be standing in. What the modal is
        // actually about is decided at its first opening, which asks
        // again when the two differ (see [`Self::open`]).
        let root = context_root();
        spawn_startup(home.clone(), memory.sender.clone(), root.clone());
        Self {
            in_flight: true,
            resolved_for: Some(root),
            ..memory
        }
    }

    /// A memory nothing has asked the machine on behalf of — what a test
    /// starts from, so opening the modal never spawns a real resolution.
    pub(crate) fn unresolved() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            remembered: None,
            in_flight: false,
            resolved_for: None,
        }
    }

    pub(crate) fn sender(&self) -> &Sender<WorkerResult> {
        &self.sender
    }

    /// One opening of the modal, picking up where the last one left off.
    ///
    /// `first_steps` is the workspace's, handed in rather than read from
    /// the layout file: the two surfaces share the one list, and the
    /// workspace may have ticked a step off since the file was written.
    pub(crate) fn open(
        &mut self,
        home: &UzeHome,
        root: &Path,
        layout: &ManagementLayout,
        first_steps: &FirstStepsLayout,
        keyboard: KeyboardSupport,
    ) -> TuiModel {
        let mut model = TuiModel {
            context_root: root.to_path_buf(),
            first_steps_collapsed: first_steps.collapsed,
            first_steps_closed: first_steps.closed,
            steps_taken: first_steps.taken.clone(),
            // Asked of the terminal once, at startup: whether a chord can
            // reach uze at all is a property of the host, and the Keys
            // screen says so rather than letting a binding look alive and
            // do nothing.
            keyboard,
            ..TuiModel::recall(self.remembered.take(), layout)
        };
        // A resolution is an answer about one project, so a different one
        // asks again however recently the last answered — and asks even
        // with one in flight, because that one is about the project this
        // is not.
        let elsewhere = self.resolved_for.as_deref() != Some(root);
        if (opening_re_resolves(model.remembered.resolved_at) || elsewhere)
            && (!self.in_flight || elsewhere)
        {
            // Behind the frame: every list is already on screen, so
            // nothing about this reads as the plugins having gone away.
            spawn_refresh(
                home.clone(),
                self.sender.clone(),
                model.context_root.clone(),
            );
            self.resolved_for = Some(model.context_root.clone());
            self.in_flight = true;
        }
        model.maintenance_in_flight = self.in_flight;
        // Two cases, one answer. Nothing resolved yet is the operator
        // arriving within the first moments of the session; a different
        // project is the operator arriving somewhere the remembered
        // answer is not about. Either way the wait is named rather than
        // drawn as an empty environment, and the queued answer replaces
        // this on the first tick.
        if model.remembered.resolved_at.is_none() || elsewhere {
            model.status = Status::Working("Refreshing environment…".to_owned());
        }
        model
    }

    /// Keeps what an opening resolved and arranged for the next one, and
    /// hands back what the workspace owns of it: the shape the layout
    /// file keeps, and the first-steps list the two surfaces share.
    pub(crate) fn close(&mut self, model: TuiModel) -> (ManagementLayout, FirstStepsLayout) {
        self.in_flight = model.maintenance_in_flight;
        let layout = model.management_layout();
        let first_steps = FirstStepsLayout {
            collapsed: model.first_steps_collapsed,
            closed: model.first_steps_closed,
            taken: model.steps_taken.clone(),
        };
        self.remembered = Some(model.remember());
        (layout, first_steps)
    }

    /// One turn of the modal's own clock, taken by the workspace loop
    /// before each frame it draws the modal in: the spinner advances,
    /// transient status expires, every answer that arrived is absorbed,
    /// and whatever the screen now shows that has not been fetched yet is
    /// asked for.
    ///
    /// Answers are drained *before* the frame on purpose: one that
    /// arrived while the modal was closed is already in the channel when
    /// it opens, and draining first is what makes the very first frame
    /// show it.
    ///
    /// Answers whether the modal now looks different: the loop turns this
    /// every few milliseconds, and redrawing a modal nothing changed in is
    /// a whole frame for nothing. The spinner counts as a change for as
    /// long as it is on screen.
    pub(crate) fn tick(&mut self, model: &mut TuiModel, home: &UzeHome) -> bool {
        model.tick = model.tick.wrapping_add(1);
        let mut changed = matches!(model.status, model::Status::Working(_));
        changed |= model.expire_status();
        changed |= model.expire_update_badges();
        if let Some((revision, notice)) = crate::self_update::since(model.release_revision) {
            model.release = notice;
            model.release_revision = revision;
            changed = true;
        }
        changed |= drain_worker_results(model, &self.receiver);
        let drawer = if model.selection_settling(Instant::now()) {
            Intent::None
        } else {
            model.drawer_inspect_intent()
        };
        for missing in [
            drawer,
            model.profile_preview_intent(),
            model.settings_intent(),
        ] {
            if missing != Intent::None {
                dispatch(missing, home, &self.sender, model);
                changed = true;
            }
        }
        changed
    }
}

/// Whether opening the screen asks the machine again, given when it last
/// answered. Nothing resolved yet is not an answer to stand on, so it
/// asks; a resolution inside [`RESOLUTION_STANDS_FOR`] is.
pub(crate) fn opening_re_resolves(resolved_at: Option<Instant>) -> bool {
    resolved_at.is_none_or(|at| at.elapsed() >= RESOLUTION_STANDS_FOR)
}

/// The directory this session speaks about, resolved the same way
/// [`TuiModel`]'s own `context_root` is — the workers started before a
/// model exists must ask the same question it would.
fn context_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

// --- Geometry -----------------------------------------------------------

/// The modal, over a frame the scrim has already pushed back: its border,
/// and the management surface inside it.
pub(crate) fn render_modal(
    frame: &mut ratatui::Frame<'_>,
    frame_area: Rect,
    model: &TuiModel,
    close_hovered: bool,
    hits: &mut Vec<(Rect, Hit)>,
) -> modal::Chrome {
    let area = modal::area(frame_area);
    frame.render_widget(Clear, area);
    Surface::card().render(frame, area);
    // A dialog open inside recedes the modal's own border too, which sits
    // outside the surface `render` dims. `render` paints its whole area
    // afresh, so what it draws is dimmed once, by itself.
    if !matches!(model.overlay, Overlay::None) {
        widget::scrim::render(frame, area);
    }
    let close = render(frame, inside_border(area), model, close_hovered, hits);
    modal::Chrome { area, close }
}

fn inside_border(area: Rect) -> Rect {
    Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

/// How far the header's and the footer's text sit inside the border: the
/// hairlines under and over them reach one column further out, so the
/// words read as hung inside the rules rather than ending with them.
const CHROME_INSET: u16 = 3;
const RULE_INSET: u16 = 1;

/// The surface's rows, top to bottom: the header with the screen tabs,
/// its hairline, a row of air, the screen, a row of air, the footer's
/// hairline and the footer.
struct Geometry {
    header: Rect,
    header_rule: Rect,
    content: Rect,
    footer: Rect,
}

fn compute_layout(area: Rect) -> Geometry {
    let row = |y: u16| Rect::new(area.x, y, area.width, 1.min(area.height));
    let inset = |rect: Rect, by: u16| Rect {
        x: rect.x + by.min(rect.width),
        width: rect.width.saturating_sub(2 * by),
        ..rect
    };
    let footer_y = area.bottom().saturating_sub(2).max(area.y);
    Geometry {
        header: inset(row(area.y), CHROME_INSET),
        header_rule: inset(row(area.y + 1), RULE_INSET),
        content: Rect::new(
            area.x,
            area.y + 3,
            area.width,
            area.height.saturating_sub(6),
        ),
        footer: Rect {
            height: 2.min(area.height),
            ..inset(row(footer_y), RULE_INSET)
        },
    }
}

// --- Rendering ----------------------------------------------------------

/// The management surface, filling `area` — the inside of the modal's
/// border, or a whole test frame — and the rect of the mark that closes
/// it.
///
/// One flat backdrop for the entire area — no panel ever paints its own
/// background; every division is a hairline or padding, never a filled
/// slab.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    close_hovered: bool,
    hits: &mut Vec<(Rect, Hit)>,
) -> Rect {
    widget::fill(frame, area, Token::SurfaceBackground);
    let layout = compute_layout(area);
    let close = render_header(frame, layout.header, model, close_hovered, hits);
    Rule::new(Edge::Top).render(frame, layout.header_rule);

    match model.route {
        Route::Overview => view::overview::render_overview(frame, layout.content, model, hits),
        Route::Plugins => view::plugins::render_plugins(frame, layout.content, model, hits),
        Route::Extensions => {
            view::extensions::render_extensions(frame, layout.content, model, hits)
        }
        Route::Harnesses => view::harnesses::render_harnesses(frame, layout.content, model, hits),
        Route::Profiles => view::profiles::render_profiles(frame, layout.content, model, hits),
        Route::Keys => view::keys::render_keys(frame, layout.content, model, hits),
        Route::Settings => view::settings::render_settings(frame, layout.content, model, hits),
    }

    render_footer(frame, layout.footer, model, hits);

    // Every arm below is a dialog: drawn in the middle of the surface, and
    // the only thing in it that answers until it is dealt with. The scrim
    // is what says so — it goes here rather than inside each arm because
    // what recedes is the screen underneath, which no dialog knows
    // anything about. Only this surface recedes: the workspace behind the
    // modal already has.
    if !matches!(model.overlay, Overlay::None) {
        widget::scrim::render(frame, area);
    }

    match &model.overlay {
        Overlay::None => {}
        Overlay::ActionIndex {
            scopes,
            filter,
            selected,
        } => overlay::render_action_index(frame, area, model, scopes, filter, *selected, hits),
        Overlay::HarnessHelp => overlay::render_harness_help(frame, area),
        Overlay::Health => overlay::render_health(frame, area, &model.alerts()),
        Overlay::ReleaseNotes(modal) => {
            let targets = super::release_notes::render(frame, area, modal);
            hits.insert(0, (targets.popup, Hit::OverlayBody));
        }
        Overlay::Confirm { kind, focus } => {
            overlay::render_confirmation(frame, area, kind, *focus, hits)
        }
        Overlay::AddMarketplace(input) => overlay::render_text_prompt(
            frame,
            area,
            &overlay::TextPrompt {
                title: "Add marketplace",
                body: "Registers it on this machine. Its plugins are listed under it, ready to \
                       install.",
                placeholder: "Local path or https://... source",
                confirm: "Add",
            },
            input,
            hits,
        ),
        Overlay::NewProfile(input) => overlay::render_text_prompt(
            frame,
            area,
            &overlay::TextPrompt {
                title: "New profile",
                body: "A named set of preferences, applied to the harnesses you choose.",
                placeholder: "Profile name",
                confirm: "Create",
            },
            input,
            hits,
        ),
    }
    close
}

/// What is worth trying once on this side of the product, noted as taken
/// in the first-steps list the two surfaces share — the workspace's own
/// list names some of the same gestures, and one taken here is taken.
pub(crate) const FIRST_STEPS: [uze_keys::Action; 2] =
    [uze_keys::Action::Refresh, uze_keys::Action::OpenActionIndex];

/// The columns the modal's name takes at the head of the tab strip, so
/// the first tab starts at the same column whatever the name.
const NAME_WIDTH: usize = 12;

/// The header: the modal's name, a tab per screen, and the key that
/// closes it at the other end — which is also the close target, so it
/// answers the pointer the way the close mark it replaced did. Answers
/// with that target's rect.
fn render_header(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    close_hovered: bool,
    hits: &mut Vec<(Rect, Hit)>,
) -> Rect {
    let dismiss = uze_keys::Action::Dismiss;
    let scopes = model.scopes();
    let close_hint = Hints::new(&scopes)
        .key(dismiss)
        .hovered(close_hovered.then_some(dismiss));
    let close_width = close_hint.width().min(area.width);
    let close = Rect::new(
        area.right().saturating_sub(close_width),
        area.y,
        close_width,
        area.height,
    );
    // The modal's own chrome answers this one, wherever the client hosts
    // it, so the target the hint hands back is the rect it was given.
    let _ = close_hint.render(frame, close);

    let name = format!("{:<NAME_WIDTH$}", "manage");
    frame.render_widget(
        Paragraph::new(Span::styled(name, theme::fg_bold(Token::TextBright))),
        Rect {
            width: (NAME_WIDTH as u16).min(area.width),
            ..area
        },
    );
    let x = area.x + NAME_WIDTH as u16;
    let room_end = close.x.saturating_sub(tabs::GAP);
    let routes = model::routes();
    let strip: Vec<(Tab, Route)> = routes
        .iter()
        .map(|&route| {
            (
                Tab::new(route.label()).badge(route.badge(), Token::StateWarning),
                route,
            )
        })
        .collect();
    let open = routes.iter().position(|&route| route == model.route);
    let row = Rect::new(x, area.y, room_end.saturating_sub(x), area.height);
    for (rect, route) in tabs::tab_bar(frame, row, &strip, open, model.focus == Focus::Sidebar) {
        hits.push((rect, Hit::Route(route)));
    }
    close
}

fn render_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    // Brighter than the hints beside it because it answers a click: it
    // opens this release's notes. Brighter still under the pointer.
    let tone = if model.version_hovered {
        Token::TextSecondary
    } else {
        Token::TextDim
    };
    let version = Line::from(Span::styled(
        format!("v{}", crate::self_update::running()),
        theme::fg(tone),
    ));
    let mut trailers = vec![(version, Hit::RunningReleaseNotes)];
    trailers.extend(health_status(model).map(|line| (line, Hit::HealthStatus)));
    trailers.extend(release_notice(model).map(|line| (line, Hit::OpenReleaseNotes)));
    let (lines, targets): (Vec<_>, Vec<_>) = trailers.into_iter().unzip();
    let scopes = model.scopes();
    let drawn = widget::footer::render(
        frame,
        area,
        CHROME_INSET - RULE_INSET,
        footer_leading(model, &scopes),
        lines,
    );
    hits.extend(drawn.trailers.into_iter().zip(targets));
    hits.extend(model.hint_targets(drawn.hints));
}

/// A newer release than this one, said where the footer says what just
/// happened: the news is the version, and a click reads its notes.
fn release_notice(model: &TuiModel) -> Option<Line<'static>> {
    let notice = model.release.as_ref()?;
    Some(Line::from(Span::styled(
        format!(
            "{} v{} available",
            theme::glyph(Symbol::ArrowUp),
            notice.version()
        ),
        theme::fg(Token::Accent),
    )))
}

/// The machine's health in a few words, beside the version: the one place
/// it is said, on every screen of the modal. `None` until the first health
/// read lands, rather than a "healthy" nobody checked.
fn health_status(model: &TuiModel) -> Option<Line<'static>> {
    model.remembered.doctor.as_ref()?;
    let alerts = model.alerts();
    let (symbol, hue, words) = match alerts.iter().map(|alert| alert.severity).min() {
        None => (
            Symbol::StatusSelected,
            Token::StateSuccess,
            "healthy".to_owned(),
        ),
        Some(severity) => (
            Symbol::MarkAttention,
            if severity == view::health::Severity::High {
                Token::StateDanger
            } else {
                Token::StateWarning
            },
            format!("{} need attention", alerts.len()),
        ),
    };
    let words_style = if model.health_hovered {
        theme::fg(hue)
    } else if alerts.is_empty() {
        theme::fg(Token::TextSecondary)
    } else {
        theme::fg(hue)
    };
    Some(Line::from(vec![
        Span::styled(format!("{} ", theme::glyph(symbol)), theme::fg(hue)),
        Span::styled(words, words_style),
    ]))
}

/// The hint line: what can be done here, with the keys that do it.
///
/// Every key in it comes from the keymap — `chord_for`, through the hint
/// widget — so a rebound key says its new chord here without anyone
/// remembering to.
fn hints<'a>(model: &TuiModel, scopes: &'a [uze_keys::Scope]) -> Hints<'a> {
    use uze_keys::Action;
    let mut hints = Hints::new(scopes)
        .run(Action::SelectPrevious, Action::SelectNext, "move")
        .hovered(model.hovered_offer);
    if model.route == Route::Plugins {
        hints = hints.run(Action::FocusContent, Action::FocusSidebar, "expand");
    }
    if let Some(label) = activate_label(model) {
        hints = hints.named(Action::Activate, label);
    }
    if model.has_filter() {
        hints = hints.named(Action::StartFilter, "filter");
    }
    if model.route == Route::Plugins {
        hints = hints.named(Action::AddMarketplace, "add");
    }
    let screens = model::routes().len().min(9) as u8;
    hints.run(Action::SelectTab(1), Action::SelectTab(screens), "section")
}

/// What Enter does here, in the word the footer gives it, or `None` where
/// it does nothing — a hint for a key that answers nothing is a hint that
/// lies. Mirrors the `Activate` arm of `perform` screen by screen; a screen
/// that changes what Enter does changes this with it.
fn activate_label(model: &TuiModel) -> Option<String> {
    use model::{PluginPane, ProfilePanel, SettingsRow};
    if model.focus == Focus::Sidebar {
        return Some("open".to_owned());
    }
    match model.route {
        Route::Keys => Some("change key".to_owned()),
        Route::Settings => match model.selected_settings_row() {
            Some(
                SettingsRow::Theme { .. }
                | SettingsRow::GlyphSet { .. }
                | SettingsRow::Chime { .. },
            ) => Some("choose".to_owned()),
            _ => None,
        },
        Route::Plugins if model.plugin_pane == PluginPane::Markets => Some("open".to_owned()),
        Route::Plugins => model
            .selected_marketplace_plugin()
            .and_then(|plugin| view::plugins::primary_offer(&plugin.offers()))
            .map(|action| action.label()),
        Route::Profiles if model.profile_preview_open => Some("toggle".to_owned()),
        Route::Profiles => match model.profile_panel {
            ProfilePanel::List => Some("edit".to_owned()),
            ProfilePanel::Editor => Some("change".to_owned()),
            ProfilePanel::Harnesses => None,
        },
        Route::Extensions => model.selected_extension().map(|extension| {
            if model.extension_enabled(extension.id) {
                "disable"
            } else {
                "enable"
            }
            .to_owned()
        }),
        Route::Overview | Route::Harnesses => None,
    }
}

/// What the footer says: how the last thing went while there is anything
/// to say about it, and otherwise what can be done here.
fn footer_leading<'a>(model: &TuiModel, scopes: &'a [uze_keys::Scope]) -> Leading<'a> {
    let (hue, text) = match &model.status {
        model::Status::Idle => return Leading::Hints(hints(model, scopes)),
        model::Status::Working(value) => (
            Token::StateWarning,
            format!(
                "{} {value}",
                theme::frame(theme::Symbol::StatusWorking, model.tick)
            ),
        ),
        model::Status::Success(value) => (Token::StateSuccess, value.clone()),
        model::Status::Error(value) => (Token::StateDanger, value.clone()),
    };
    Leading::Status(Line::from(Span::styled(text, theme::fg_bold(hue))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The workspace loop turns the modal's clock every few milliseconds;
    /// a turn that moved nothing must not cost a frame, and a spinner on
    /// screen must keep turning.
    #[test]
    fn the_modal_redraws_only_when_its_clock_moved_something() {
        let home = UzeHome::at(uze_testkit::temp::scratch("management-tick"));
        let mut memory = ManagementMemory::unresolved();
        let mut model = TuiModel::default();
        memory.tick(&mut model, &home);

        assert!(
            !memory.tick(&mut model, &home),
            "an idle modal is left alone"
        );

        model.status = Status::Working("Refreshing environment…".to_owned());
        assert!(memory.tick(&mut model, &home), "a spinner advances");

        model.say("done");
        model.status_expires_at = Some(Instant::now());
        assert!(
            memory.tick(&mut model, &home),
            "a status going quiet redraws"
        );
        assert!(!memory.tick(&mut model, &home));

        memory
            .sender
            .send(WorkerResult::ReleaseNotesRead("1.0.0".to_owned(), None))
            .unwrap();
        assert!(memory.tick(&mut model, &home), "an answer arriving redraws");
    }

    /// The footer names what Enter does on the screen in front of the
    /// reader, and says nothing where it does nothing.
    #[test]
    fn the_footer_names_what_enter_does_on_each_screen() {
        let mut model = TuiModel {
            focus: Focus::Content,
            ..TuiModel::default()
        };
        model.route = Route::Harnesses;
        assert_eq!(activate_label(&model), None, "Enter does nothing here");
        model.route = Route::Keys;
        assert_eq!(activate_label(&model).as_deref(), Some("change key"));
        model.route = Route::Plugins;
        model.select_plugin_market(Some("uze-official".to_owned()));
        assert_eq!(
            activate_label(&model).as_deref(),
            Some("open"),
            "on a marketplace, Enter steps into it"
        );
        model.focus = Focus::Sidebar;
        assert_eq!(activate_label(&model).as_deref(), Some("open"));
    }
}
