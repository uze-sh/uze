//! TUI view — Extensions route.
//!
//! The tool side of the product: extensions of the workspace itself (as
//! opposed to plugins, which are agentic packages delivered *to* harnesses
//! — see `view::plugins`). The official ones come straight from
//! `uze_extensions::registry::ExtensionRegistry::builtin`, the one
//! composition root that knows the bundled set; after them stand the ones
//! installed plugins declare, as the last refresh compiled them. The one
//! thing an operator decides about either is whether the workspace offers
//! it; a responsive catalog of compact cards says which, and the detail
//! drawer describes the selection the same way Plugins/Harnesses do — from
//! what the list already holds, so there is nothing to fetch.

use std::path::Path;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::catalog::{Badge, Card, render_card};
use super::{DrawerStatus, render_drawer_footer};
use crate::ui::hit::Hit;
use crate::ui::model::{ExtensionEntry, PreferencesEditor, ResizablePanel, Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{RowState, definitions, form, text};
use crate::ui::{content_area, render_screen_header};
use uze_application::{ExtensionCondition as Condition, ExtensionReport};

pub(crate) fn render_extensions(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let outer = content_area(area);
    // Shown whenever there is an extension to describe: the drawer is the
    // screen's detail column, not something opened and closed.
    let drawer_shown = model.selected_extension().is_some();
    let drawer_width =
        drawer_shown.then(|| super::drawer_width(ResizablePanel::Drawer, model, outer));
    let header_width = outer
        .width
        .saturating_sub(drawer_width.unwrap_or(0))
        .saturating_sub(if drawer_shown { 1 } else { 0 });
    let header_area = Rect::new(outer.x, outer.y, header_width, outer.height);
    let content = render_screen_header(
        frame,
        header_area,
        Route::Extensions,
        Some(Span::styled(
            bundled_caption(model),
            theme::fg(Token::TextMuted),
        )),
    );
    let mut y = content.y;
    let bottom = content.y + content.height;
    if y + 2 <= bottom {
        let filter_area = Rect::new(content.x, y, content.width, 1);
        hits.push((filter_area, Hit::FocusFilter));
        super::filter_box(
            frame,
            filter_area,
            &model.remembered.extension_screen.filter,
            "filter extensions…",
            model.filtering,
        );
        y += super::FILTER_ROWS;
    }
    let catalog_area = Rect::new(content.x, y, content.width, bottom.saturating_sub(y));

    let visible = model.extension_visible_indices();
    let entries = model.extension_entries();
    if entries.is_empty() || visible.is_empty() {
        let message = if entries.is_empty() {
            "No extensions available.".to_owned()
        } else {
            format!(
                "No extensions match \"{}\".",
                model.remembered.extension_screen.filter.trim()
            )
        };
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme::fg(Token::TextMuted))),
            catalog_area,
        );
    }
    let screen = &model.remembered.extension_screen;
    let grid = super::catalog::cards(catalog_area, visible.len(), screen.selected, &screen.scroll);
    for &(position, rect) in &grid.cards {
        let extension = entries[visible[position]];
        let state = RowState::of(
            position == screen.selected,
            model.hovered == Some(Hit::ExtensionRow(position)),
        );
        render_extension_card(
            frame,
            rect,
            extension,
            model.extension_enabled(&extension.id()),
            declared_standing(model, extension),
            state,
        );
        hits.push((rect, Hit::ExtensionRow(position)));
    }

    if let Some(extension) = model.selected_extension() {
        render_extension_drawer(frame, outer, model, extension, hits);
        super::scrollbar_on_drawer_rule(
            frame,
            outer,
            model,
            catalog_area,
            (grid.visible, grid.rows, grid.first),
            hits,
            None,
        );
    }
}

fn bundled_caption(model: &TuiModel) -> String {
    let bundled = model.extensions.len();
    let declared = model.declared_extensions.len();
    let off = model
        .extension_entries()
        .iter()
        .filter(|extension| !model.extension_enabled(&extension.id()))
        .count();
    let mut caption = format!("{bundled} bundled");
    if declared > 0 {
        caption.push_str(&format!(" · {declared} from plugins"));
    }
    if off > 0 {
        caption.push_str(&format!(" · {off} disabled"));
    }
    caption
}

/// Where a declared extension stands in the project the screen was
/// opened in; nothing for a built-in.
fn declared_standing<'a>(
    model: &'a TuiModel,
    extension: ExtensionEntry<'_>,
) -> Option<&'a ExtensionReport> {
    match extension {
        ExtensionEntry::Builtin(_) => None,
        ExtensionEntry::Declared(summary) => model.extension_standing(&summary.identity),
    }
}

/// The word a card ends with for an extension whose switch is on: where it
/// stands, coloured by whether the operator has something to answer.
fn standing_word(standing: &ExtensionReport) -> Option<Span<'static>> {
    let (word, token) = match standing.condition {
        _ if standing.checkout_refused.is_some() => ("Runs nothing here", Token::StateWarning),
        Condition::NeedsConsent => ("Needs consent", Token::StateWarning),
        Condition::Held => ("Held", Token::StateWarning),
        Condition::NeedsEnabling => ("Not enabled here", Token::TextDim),
        Condition::Enabled | Condition::Consented | Condition::FixturesOnly | Condition::Broken => {
            return None;
        }
    };
    Some(Span::styled(word, theme::fg(token)))
}

fn render_extension_card(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    extension: ExtensionEntry<'_>,
    enabled: bool,
    standing: Option<&ExtensionReport>,
    state: RowState,
) {
    let enabled_state = || {
        if enabled {
            Span::styled("Enabled", theme::fg(Token::StateSuccess))
        } else {
            Span::styled("Disabled", theme::fg(Token::TextDim))
        }
    };
    let card = match extension {
        ExtensionEntry::Builtin(extension) => Card {
            name: extension.name,
            badge: Some(seal("Bundled")),
            description: extension.description,
            caption: Span::styled(extension.surface, theme::fg(Token::TextMuted)),
            state: Some(enabled_state()),
        },
        // What a disabled one got wrong does not reach the workspace, so
        // it is said only while the extension is on.
        ExtensionEntry::Declared(summary) => Card {
            name: &summary.name,
            badge: summary.official.then(|| seal("Official")),
            description: &summary.description,
            caption: Span::styled(identity_caption(summary), theme::fg(Token::TextMuted)),
            state: Some(if enabled && !summary.errors.is_empty() {
                Span::styled(
                    errors_said(summary.errors.len()),
                    theme::fg(Token::StateDanger),
                )
            } else if enabled && let Some(word) = standing.and_then(standing_word) {
                word
            } else {
                enabled_state()
            }),
        },
    };
    render_card(frame, rect, card, state);
}

/// uze's seal on a card, as a marketplace's verified publisher wears one:
/// `Bundled` for what ships inside uze and is never installed or removed,
/// `Official` for what uze publishes in its marketplace.
fn seal(label: &'static str) -> Badge {
    Badge {
        mark: theme::glyph(Symbol::MarkOfficial),
        label,
        color: theme::color(Token::StateInfo),
    }
}

/// `gsd@market/gsd-band`, and `· linked` when its author works on it
/// here.
fn identity_caption(summary: &uze_application::DeclaredExtensionSummary) -> String {
    if summary.linked {
        format!("{} · linked", summary.identity)
    } else {
        summary.identity.to_string()
    }
}

fn errors_said(count: usize) -> String {
    match count {
        1 => "1 error".to_owned(),
        _ => format!("{count} errors"),
    }
}

fn render_extension_drawer(
    frame: &mut ratatui::Frame<'_>,
    content: Rect,
    model: &TuiModel,
    extension: ExtensionEntry<'_>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let inner = super::drawer(frame, content, ResizablePanel::Drawer, model, hits);
    let enabled = model.extension_enabled(&extension.id());
    let offers = model.extension_offers(extension);
    let standing = declared_standing(model, extension);
    // Preferences are their own controls in the body, a click away: a
    // button for them would only repeat what the rows already offer, so it
    // stays in the action index alone.
    let offers: Vec<_> = offers
        .into_iter()
        .filter(|offer| offer.action != uze_keys::Action::EditPreferences)
        .collect();
    let (body, status) = super::drawer_body_and_footer(inner, &offers);
    let mut grid = Grid::new(usize::from(body.width));
    let mut plugin_line = None;

    grid.lines.push(super::section_label("extension"));
    grid.lines.push(Line::from(Span::styled(
        extension.name().to_owned(),
        Style::default()
            .fg(theme::color(Token::TextBright))
            .add_modifier(Modifier::BOLD),
    )));
    if !extension.description().is_empty() {
        grid.gap();
        grid.prose(extension.description(), Token::TextSecondary);
    }
    grid.gap();
    match extension {
        ExtensionEntry::Builtin(extension) => {
            grid.field("opens", extension.usage, Token::TextSecondary);
        }
        ExtensionEntry::Declared(summary) => {
            // The plugin it comes with, a click away on the Marketplace:
            // what the package brings is said there, what runs is said
            // here.
            let link = if model.hovered_offer == Some(uze_keys::Action::ShowPlugin) {
                Token::TextBright
            } else {
                Token::TextMuted
            };
            plugin_line = Some(grid.lines.len());
            let plugin = format!(
                "{}@{}",
                summary.identity.plugin, summary.identity.marketplace
            );
            grid.field("from", &plugin, link);
            if summary.linked {
                grid.field("", "linked · redrawn on every save", Token::TextMuted);
            }
            for (index, surface) in declared_surfaces(summary).iter().enumerate() {
                grid.field(
                    if index == 0 { "opens" } else { "" },
                    surface,
                    Token::TextSecondary,
                );
            }
        }
    }
    let editing = model
        .preferences
        .as_ref()
        .filter(|editor| matches!(extension, ExtensionEntry::Declared(summary) if summary.identity == editor.identity));
    let preference_rows = standing.map_or_else(Vec::new, |standing| {
        standing_lines(
            &mut grid,
            standing,
            model.extension_project.as_deref(),
            editing,
            model.hovered.as_ref(),
        )
    });
    let lines = grid.lines;
    // What the last compile refused stands at the foot of the body, as it
    // does over a plugin's own surface while its author saves.
    let mut text_area = body;
    if let ExtensionEntry::Declared(summary) = extension
        && !summary.errors.is_empty()
    {
        let title = match summary.errors.len() {
            1 => "the last compile has an error".to_owned(),
            count => format!("the last compile has {count} errors"),
        };
        // The room under what the drawer says, a blank row apart.
        let taken = u16::try_from(lines.len() + 1)
            .unwrap_or(u16::MAX)
            .min(body.height);
        let room = Rect {
            y: body.y + taken,
            height: body.height - taken,
            ..body
        };
        let banner = crate::ui::widget::banner::render_folded(frame, room, &title, &summary.errors);
        text_area.height = body.height.saturating_sub(banner.height);
    }
    // While the keys are on the preferences, what the row they are on
    // holds stands at the body's foot, out of the scroll, as a dialog's
    // detail does: in sight whichever row it is.
    let first = match editing {
        Some(editor) => {
            let detail = Rect {
                y: text_area.bottom().saturating_sub(PREFERENCE_DETAIL_ROWS),
                height: PREFERENCE_DETAIL_ROWS.min(text_area.height),
                ..text_area
            };
            text_area.height = text_area.height.saturating_sub(detail.height);
            render_preference_detail(frame, detail, editor);
            following(&preference_rows, editor, usize::from(text_area.height))
        }
        None => 0,
    };
    // Where line `line` of the body stands on screen, once the body has
    // scrolled to `first`; nothing for a line out of sight.
    let on_screen = |line: usize| {
        line.checked_sub(first)
            .and_then(|row| u16::try_from(row).ok())
            .filter(|row| *row < text_area.height)
            .map(|row| body.y + row)
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(first).unwrap_or(u16::MAX), 0)),
        text_area,
    );
    if let Some(y) = plugin_line.and_then(on_screen) {
        hits.push((
            Rect::new(body.x, y, body.width, 1),
            Hit::OfferedAction(uze_keys::Action::ShowPlugin),
        ));
    }
    // Each part of a control before its row, so a click on one is not
    // taken for choosing the row.
    for placed in preference_rows {
        let Some(y) = on_screen(placed.line) else {
            continue;
        };
        for (x, width, hit) in placed.parts {
            if x < body.width {
                let width = width.min(body.width - x);
                hits.push((Rect::new(body.x + x, y, width, 1), hit));
            }
        }
        hits.push((
            Rect::new(body.x, y, body.width, 1),
            Hit::ExtensionPreferenceRow(placed.index),
        ));
    }

    render_drawer_footer(
        frame,
        status,
        if let Some(status) = enabled
            .then(|| {
                standing.and_then(|standing| {
                    standing_status(standing, model.extension_project.is_some())
                })
            })
            .flatten()
        {
            status
        } else if enabled {
            DrawerStatus {
                color: theme::color(Token::StateSuccess),
                headline: "Enabled",
                subtitle: "",
            }
        } else {
            DrawerStatus {
                color: theme::color(Token::TextDim),
                headline: "Disabled",
                subtitle: "Withdrawn from the workspace",
            }
        },
        &offers,
        model.hovered_offer,
        None,
        hits,
    );
}

/// The drawer's body, folded to its width as it is built, so a row's
/// place on screen is its index: keys in the column the Marketplace's
/// panel uses, values hanging under their own first row, and tables whose
/// first column is as wide as their longest name.
struct Grid {
    width: usize,
    lines: Vec<Line<'static>>,
}

impl Grid {
    fn new(width: usize) -> Self {
        Self {
            width: width.max(1),
            lines: Vec::new(),
        }
    }

    fn gap(&mut self) {
        self.lines.push(Line::from(""));
    }

    fn air(&mut self, rows: usize) {
        for _ in 0..rows {
            self.gap();
        }
    }

    fn prose(&mut self, said: &str, token: Token) {
        self.lines.extend(
            text::fold(said, self.width)
                .into_iter()
                .map(|row| Line::from(Span::styled(row, theme::fg(token)))),
        );
    }

    /// `key` in the shared key column, `value` folded beside it.
    fn field(&mut self, key: &str, value: &str, token: Token) {
        self.define(
            &[(key.to_owned(), value.to_owned(), token)],
            Token::TextDim,
            definitions::Column::Fixed(super::FIELD_KEY_WIDTH),
        );
    }

    /// A section of rows, each a name and what it says, the names in a
    /// column as wide as the longest of them.
    fn table(&mut self, heading: &str, rows: &[(String, String, Token)]) {
        if rows.is_empty() {
            return;
        }
        self.gap();
        self.lines.push(super::section_label(heading));
        self.define(rows, Token::TextCode, definitions::Column::Fit);
    }

    fn define(
        &mut self,
        rows: &[(String, String, Token)],
        ink: Token,
        column: definitions::Column,
    ) {
        let entries: Vec<_> = rows
            .iter()
            .map(|(name, said, token)| definitions::Entry {
                name,
                said,
                name_style: theme::fg(ink),
                said_style: theme::fg(*token),
            })
            .collect();
        self.lines.extend(
            definitions::lines(&entries, column, self.width)
                .into_iter()
                .flatten(),
        );
    }
}

/// What a declared extension offers, a surface a line, each with where it
/// is reached.
fn declared_surfaces(summary: &uze_application::DeclaredExtensionSummary) -> Vec<String> {
    let surfaces: Vec<String> = summary
        .views
        .iter()
        .map(|title| format!("{title} · view, from the bar"))
        .chain(
            summary
                .drawers
                .iter()
                .map(|title| format!("{title} · drawer, from the bar")),
        )
        .chain(
            summary
                .sections
                .iter()
                .map(|title| format!("{title} · sidebar section")),
        )
        .chain(
            summary
                .statuses
                .iter()
                .map(|title| format!("{title} · sidebar header row")),
        )
        .chain(summary.bands.iter().map(|(title, rows)| match rows {
            1 => format!("{title} · band over each pane, 1 row"),
            rows => format!("{title} · band over each pane, {rows} rows"),
        }))
        .chain(
            summary
                .overlays
                .iter()
                .map(|title| format!("{title} · overlay, from the action index")),
        )
        .collect();
    if surfaces.is_empty() {
        vec!["nothing until it compiles".to_owned()]
    } else {
        surfaces
    }
}

/// The drawer's account of where a declared extension stands: the grants
/// it wants with what each lets it do, its programs, its preferences and,
/// when it runs nothing here or runs elsewhere, why and where.
fn standing_lines(
    grid: &mut Grid,
    standing: &ExtensionReport,
    project: Option<&Path>,
    editing: Option<&PreferencesEditor>,
    hovered: Option<&Hit>,
) -> Vec<PlacedPreference> {
    // Each preference as a settings form draws a key: its label in a
    // column, then the control that changes it. First, since it is the
    // one part of the drawer that is set rather than read.
    let mut preference_rows = Vec::new();
    let preferences = editing.map_or(standing.preferences.as_slice(), |editor| &editor.rows);
    if !preferences.is_empty() {
        // A group of the form, as the settings form spaces one.
        grid.air(form::GROUP_AIR);
        grid.lines.push(form::heading("preferences", grid.width));
        grid.air(form::KEY_AIR);
        let layout =
            form::Layout::fit(preferences.iter().map(|row| row.label.as_str()), grid.width);
        for (index, row) in preferences.iter().enumerate() {
            if index > 0 {
                grid.air(form::KEY_AIR);
            }
            let marked = editing.filter(|editor| editor.selected == index);
            let state = RowState::of(
                marked.is_some(),
                hovered == Some(&Hit::ExtensionPreferenceRow(index)),
            );
            let lit = match hovered {
                Some(Hit::ExtensionPreferenceStep { index: at, forward }) if *at == index => {
                    Some(if *forward {
                        form::Part::Increase
                    } else {
                        form::Part::Decrease
                    })
                }
                Some(Hit::ExtensionPreferenceChoice { index: at, option }) if *at == index => {
                    Some(form::Part::Option(*option))
                }
                Some(Hit::ExtensionPreferenceValue(at)) if *at == index => Some(form::Part::Value),
                _ => None,
            };
            let typing = marked.and_then(|editor| editor.typing.as_deref());
            let key = preference_key(row, state, typing, layout, lit);
            preference_rows.push(PlacedPreference {
                line: grid.lines.len(),
                index,
                parts: key
                    .parts
                    .into_iter()
                    .map(|(x, width, part)| (x, width, preference_hit(index, part)))
                    .collect(),
            });
            grid.lines.push(key.line);
        }
    }

    let grants: Vec<_> = standing
        .grants
        .iter()
        .map(|grant| {
            (
                grant.clone(),
                uze_application::grant_meaning(grant).to_owned(),
                Token::TextSecondary,
            )
        })
        .collect();
    grid.table("grants", &grants);

    let mut programs = Vec::new();
    for program in &standing.programs {
        let (found, token) = match &program.path {
            Some(path) => (path.display().to_string(), Token::TextMuted),
            None if program.packaged => ("packaged with it".to_owned(), Token::TextMuted),
            None => (
                program
                    .unresolved
                    .clone()
                    .unwrap_or_else(|| "not found".to_owned()),
                Token::StateWarning,
            ),
        };
        programs.push((program.name.clone(), found, token));
        if !program.reads.is_empty() {
            programs.push((
                String::new(),
                format!("reads {}", program.reads.join(", ")),
                Token::TextMuted,
            ));
        }
    }
    grid.table("programs", &programs);
    if !standing.programs.is_empty() {
        if standing.confined == Some(true) {
            grid.prose(
                "confined: reads the checkout, writes only scratch, no network",
                Token::TextMuted,
            );
        } else {
            grid.prose(
                "runs as you: this machine cannot confine programs",
                Token::StateWarning,
            );
        }
    }

    let mut notes = Vec::new();
    if standing.condition == Condition::Held {
        notes.push((
            "wants",
            format!("{}, not allowed yet", standing.new_grants.join(", ")),
            Token::StateWarning,
        ));
    }
    if let Some(reason) = &standing.checkout_refused {
        // The foot says it runs nothing here; this says why.
        notes.push(("why", reason.clone(), Token::StateWarning));
    }
    // Every project is the foot's to say; only other projects are news.
    let elsewhere = standing
        .enabled_in
        .iter()
        .filter(|enabled| project.is_none_or(|project| enabled.as_path() != project))
        .count();
    if !standing.enabled_everywhere && elsewhere > 0 {
        notes.push((
            "elsewhere",
            match elsewhere {
                1 => "enabled in 1 other project".to_owned(),
                n => format!("enabled in {n} other projects"),
            },
            Token::TextMuted,
        ));
    }
    if let Some(day) = standing.updated_on() {
        notes.push(("updated", format!("{day}, same grants"), Token::TextMuted));
    }
    if !notes.is_empty() {
        grid.gap();
        for (key, said, token) in notes {
            grid.field(key, &said, token);
        }
    }
    preference_rows
}

/// The drawer's foot for a declared extension that runs code and waits
/// for the operator, or runs nothing in this checkout; `None` when the
/// switch's own foot says it.
fn standing_status(
    standing: &ExtensionReport,
    in_a_project: bool,
) -> Option<DrawerStatus<'static>> {
    let warning = theme::color(Token::StateWarning);
    Some(match standing.condition {
        Condition::NeedsConsent | Condition::NeedsEnabling | Condition::Held if !in_a_project => {
            DrawerStatus {
                color: warning,
                headline: "Needs a project",
                subtitle: "Open an agent in a project, then enable it from there",
            }
        }
        _ if standing.checkout_refused.is_some() => DrawerStatus {
            color: warning,
            headline: "Runs nothing here",
            subtitle: "This checkout is not one an extension may read",
        },
        Condition::NeedsConsent => DrawerStatus {
            color: warning,
            headline: "Needs consent",
            subtitle: "It runs a script; nothing of it runs until you allow its grants",
        },
        Condition::Held => DrawerStatus {
            color: warning,
            headline: "Held",
            subtitle: "Its update wants more than you allowed; nothing of it runs",
        },
        Condition::NeedsEnabling => DrawerStatus {
            color: theme::color(Token::TextDim),
            headline: "Not enabled here",
            subtitle: "Allowed on this machine, and off in this project",
        },
        Condition::Enabled if standing.enabled_everywhere => DrawerStatus {
            color: theme::color(Token::StateSuccess),
            headline: "Enabled everywhere",
            subtitle: "",
        },
        Condition::Enabled => DrawerStatus {
            color: theme::color(Token::StateSuccess),
            headline: "Enabled here",
            subtitle: "",
        },
        Condition::Consented | Condition::FixturesOnly | Condition::Broken => return None,
    })
}

/// The rows a preference's detail keeps under the rows while the keys are
/// on them.
const PREFERENCE_DETAIL: usize = 2;
/// The rows the detail takes at the body's foot: its rule, then the
/// detail.
const PREFERENCE_DETAIL_ROWS: u16 = 1 + PREFERENCE_DETAIL as u16;

/// A preference's row in the drawer, and where on it each part of its
/// control answers a click.
struct PlacedPreference {
    /// The line of the drawer's body it stands on.
    line: usize,
    /// Its place among the extension's preferences.
    index: usize,
    /// Each part's column from the body's start, its width, and its hit.
    parts: Vec<(u16, u16, Hit)>,
}

/// `row` as a key of the form: a number between its steps, a toggle or a
/// short choice side by side, anything else in its box, typed into while
/// `typing` holds what is typed.
fn preference_key(
    row: &uze_application::PreferenceRow,
    state: RowState,
    typing: Option<&str>,
    layout: form::Layout,
    lit: Option<form::Part>,
) -> form::Key {
    let said = row.said();
    let control = |control| form::key(&row.label, state, control, layout, lit);
    if let Some(typed) = typing {
        // A secret is drawn as one dot per character typed.
        let shown = if row.is_secret() {
            theme::glyph(Symbol::CursorMasked).repeat(typed.chars().count())
        } else {
            typed.to_owned()
        };
        // Cut at the head, so the end being typed and the caret stay in
        // the box.
        let shown = text::elide_head(&shown, layout.field.saturating_sub(3));
        return control(form::Control::Typing {
            before: &shown,
            after: "",
        });
    }
    match row.kind {
        "number" => control(form::Control::Stepper {
            value: &said,
            tone: Token::TextBright,
            editable: true,
        }),
        "toggle" => control(form::Control::Choice {
            options: &["off", "on"],
            chosen: &said,
        }),
        "choice" => {
            let options: Vec<&str> = row.values.iter().map(String::as_str).collect();
            control(form::Control::Choice {
                options: &options,
                chosen: &said,
            })
        }
        _ => {
            let quiet = if row.is_secret() {
                row.value != serde_json::Value::Bool(true)
            } else {
                row.value.as_str().is_none_or(str::is_empty)
            };
            control(form::Control::Boxed {
                value: if quiet && !row.is_secret() {
                    "empty"
                } else {
                    &said
                },
                tone: if quiet {
                    Token::TextDim
                } else {
                    Token::TextPrimary
                },
            })
        }
    }
}

/// What a click on `part` of preference `index`'s control is.
fn preference_hit(index: usize, part: form::Part) -> Hit {
    match part {
        form::Part::Decrease => Hit::ExtensionPreferenceStep {
            index,
            forward: false,
        },
        form::Part::Increase => Hit::ExtensionPreferenceStep {
            index,
            forward: true,
        },
        form::Part::Value => Hit::ExtensionPreferenceValue(index),
        form::Part::Option(option) => Hit::ExtensionPreferenceChoice { index, option },
    }
}

/// The first line of the body to show while the keys are on `editor`'s
/// row, `height` lines being shown: where it was, moved only as far as
/// brings the row into sight.
fn following(rows: &[PlacedPreference], editor: &PreferencesEditor, height: usize) -> usize {
    let first = editor.scroll.get();
    let Some(on) = rows.iter().find(|row| row.index == editor.selected) else {
        return first;
    };
    let first = if on.line < first {
        on.line
    } else if on.line >= first + height {
        on.line + 1 - height
    } else {
        first
    };
    editor.scroll.set(first);
    first
}

/// What the row the keys are on holds, under a rule at the foot of the
/// body: its kind, range and default, in rows kept whatever it says.
fn render_preference_detail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    editor: &PreferencesEditor,
) {
    let Some(row) = editor.rows.get(editor.selected) else {
        return;
    };
    let content = crate::ui::widget::Rule::new(crate::ui::widget::Edge::Top).render(frame, area);
    let mut said = preference_meaning(row);
    if row.is_secret() {
        said.push_str(" · empty forgets it");
    }
    let lines: Vec<Line<'static>> = text::fold(&said, usize::from(content.width).max(1))
        .into_iter()
        .take(PREFERENCE_DETAIL)
        .map(|folded| Line::from(Span::styled(folded, theme::fg(Token::TextSecondary))))
        .collect();
    frame.render_widget(Paragraph::new(lines), content);
}

/// What the keys do on the preference they are on, in the footer: what
/// the arrows and Enter do to a row of its kind, and Esc back to the
/// cards.
pub(crate) fn preference_hints<'a>(
    hints: crate::ui::widget::hint::Hints<'a>,
    editor: &PreferencesEditor,
) -> crate::ui::widget::hint::Hints<'a> {
    use uze_keys::Action;
    let Some(selected) = editor.rows.get(editor.selected) else {
        return hints;
    };
    if editor.typing.is_some() {
        return hints
            .named(Action::Activate, "set")
            .named(Action::Dismiss, "cancel");
    }
    let mut hints = hints;
    if editor.rows.len() > 1 {
        hints = hints.run(Action::SelectPrevious, Action::SelectNext, "move");
    }
    let (step, enter) = match selected.kind {
        "number" => (Some("step"), "type"),
        "choice" => (Some("choose"), "next"),
        "toggle" => (None, "switch"),
        _ => (None, "type"),
    };
    if let Some(step) = step {
        hints = hints.run(Action::PreviousValue, Action::NextValue, step);
    }
    hints
        .named(Action::Activate, enter)
        .named(Action::Dismiss, "back")
}

/// What `row` holds, as the detail and the typing step say it: its kind
/// with its range, then its default (`A number from 1 to 90 · default
/// 25`), or for a secret where it is kept.
fn preference_meaning(row: &uze_application::PreferenceRow) -> String {
    let kind = match row.kind {
        "number" => match (row.min, row.max) {
            (Some(min), Some(max)) => format!("A number from {min} to {max}"),
            (Some(min), None) => format!("A number, {min} or more"),
            (None, Some(max)) => format!("A number up to {max}"),
            (None, None) => "A number".to_owned(),
        },
        "toggle" => "On or off".to_owned(),
        "choice" => format!("One of {}", row.values.join(", ")),
        "secret" => {
            return format!(
                "A secret, kept in {} and never shown again",
                uze_application::secrets_kept_in()
            );
        }
        _ => "Text".to_owned(),
    };
    let default = match &row.default {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(on) => Some(if *on { "on" } else { "off" }.to_owned()),
        serde_json::Value::String(text) if text.is_empty() => None,
        serde_json::Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    };
    match default {
        Some(default) => format!("{kind} · default {default}"),
        None => kind,
    }
}
