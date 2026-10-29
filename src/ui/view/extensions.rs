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
    widgets::{Paragraph, Wrap},
};

use super::catalog::{Badge, Card, render_card};
use super::{DrawerStatus, render_drawer_footer};
use crate::ui::hit::Hit;
use crate::ui::model::{ExtensionEntry, ResizablePanel, Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{RowState, text};
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
            badge: Some(Badge {
                mark: theme::glyph(Symbol::MarkOfficial),
                label: "Official",
                color: theme::color(Token::StateInfo),
            }),
            description: extension.description,
            caption: Span::styled(extension.surface, theme::fg(Token::TextMuted)),
            state: Some(enabled_state()),
        },
        // What a disabled one got wrong does not reach the workspace, so
        // it is said only while the extension is on.
        ExtensionEntry::Declared(summary) => Card {
            name: &summary.name,
            badge: None,
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

/// `gsd-band · gsd@market`, and `· linked` when its author works on it
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
    let (body, status) = super::drawer_body_and_footer(inner, &offers);

    let mut lines = vec![
        super::section_label("extension"),
        Line::from(Span::styled(
            extension.name().to_owned(),
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        )),
    ];
    if !extension.description().is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            extension.description().to_owned(),
            theme::fg(Token::TextSecondary),
        )));
    }
    let (surface, usage) = match extension {
        ExtensionEntry::Builtin(extension) => (
            vec![extension.surface.to_owned()],
            extension.usage.to_owned(),
        ),
        ExtensionEntry::Declared(summary) => declared_surfaces(summary),
    };
    if let ExtensionEntry::Declared(summary) = extension {
        lines.push(Line::from(""));
        lines.push(super::section_label("from"));
        // The plugin it comes with, a click away on the Marketplace: what
        // the package brings is said there, what runs is said here.
        let above: usize = lines
            .iter()
            .map(|line| text::fold(&line.to_string(), usize::from(body.width)).len())
            .sum();
        if let Ok(above) = u16::try_from(above)
            && above < body.height
        {
            hits.push((
                Rect::new(body.x, body.y + above, body.width, 1),
                Hit::OfferedAction(uze_keys::Action::ShowPlugin),
            ));
        }
        let link = if model.hovered_offer == Some(uze_keys::Action::ShowPlugin) {
            Token::TextBright
        } else {
            Token::TextMuted
        };
        lines.push(Line::from(Span::styled(
            summary.identity.to_string(),
            theme::fg(link),
        )));
        if summary.linked {
            lines.push(Line::from(Span::styled(
                "linked · redrawn on every save",
                theme::fg(Token::TextMuted),
            )));
        }
    }
    if let Some(standing) = standing {
        lines.extend(standing_lines(standing, model.extension_project.as_deref()));
    }
    lines.push(Line::from(""));
    lines.push(super::section_label("surface"));
    lines.extend(
        surface
            .into_iter()
            .map(|surface| Line::from(Span::styled(surface, theme::fg(Token::TextPrimary)))),
    );
    lines.push(Line::from(""));
    lines.push(super::section_label("how to open"));
    lines.push(Line::from(Span::styled(
        usage,
        theme::fg(Token::TextSecondary),
    )));
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
        let said: usize = lines
            .iter()
            .map(|line| text::fold(&line.to_string(), usize::from(body.width)).len())
            .sum();
        let taken = u16::try_from(said + 1).unwrap_or(u16::MAX).min(body.height);
        let room = Rect {
            y: body.y + taken,
            height: body.height - taken,
            ..body
        };
        let banner = crate::ui::widget::banner::render_folded(frame, room, &title, &summary.errors);
        text_area.height = body.height.saturating_sub(banner.height);
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text_area);

    let from_a_plugin = matches!(extension, ExtensionEntry::Declared(_));
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
                subtitle: if from_a_plugin {
                    "Comes with an installed plugin and is offered in the workspace"
                } else {
                    "Ships with uze and is offered in the workspace"
                },
            }
        } else {
            DrawerStatus {
                color: theme::color(Token::TextDim),
                headline: "Disabled",
                subtitle: "Its keys, buttons and sidebar sections are withdrawn",
            }
        },
        &offers,
        model.hovered_offer,
        None,
        hits,
    );
}

/// What a declared extension offers, a surface a line, and how each is
/// reached.
fn declared_surfaces(summary: &uze_application::DeclaredExtensionSummary) -> (Vec<String>, String) {
    let surfaces: Vec<String> = summary
        .views
        .iter()
        .map(|title| format!("{title} · view"))
        .chain(
            summary
                .sections
                .iter()
                .map(|title| format!("{title} · section")),
        )
        .chain(
            summary
                .drawers
                .iter()
                .map(|title| format!("{title} · drawer")),
        )
        .chain(
            summary
                .statuses
                .iter()
                .map(|title| format!("{title} · status row")),
        )
        .chain(summary.bands.iter().map(|(title, rows)| match rows {
            1 => format!("{title} · band, 1 row"),
            rows => format!("{title} · band, {rows} rows"),
        }))
        .chain(
            summary
                .overlays
                .iter()
                .map(|title| format!("{title} · overlay")),
        )
        .collect();
    let no_button = summary.views.is_empty() && summary.drawers.is_empty();
    let usage = match (no_button, summary.sections.is_empty()) {
        (false, false) => "Its button in the workspace's tab strip, and its section in the sidebar",
        (false, true) => "Its button in the workspace's tab strip",
        (true, false) => "Its section in the workspace's sidebar",
        (true, true) => "",
    };
    let mut usage: Vec<String> = Some(usage.to_owned())
        .filter(|usage| !usage.is_empty())
        .into_iter()
        .collect();
    if !summary.statuses.is_empty() {
        usage.push("its row in the sidebar's header".to_owned());
    }
    if !summary.bands.is_empty() {
        usage.push("its band over each agent's pane".to_owned());
    }
    if !summary.overlays.is_empty() {
        usage.push("Open, in the action index".to_owned());
    }
    let usage = match usage.split_first() {
        None => "Nothing to open until it compiles".to_owned(),
        Some((first, rest)) => {
            let mut said = capitalised(first);
            for part in rest {
                said.push_str(" · ");
                said.push_str(part);
            }
            said
        }
    };
    if surfaces.is_empty() {
        (vec!["none yet".to_owned()], usage)
    } else {
        (surfaces, usage)
    }
}

fn capitalised(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// The drawer's account of where a declared extension stands: the grants
/// it wants with what each lets it do, and, when it runs nothing, why.
fn standing_lines(standing: &ExtensionReport, project: Option<&Path>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !standing.grants.is_empty() {
        lines.push(Line::from(""));
        lines.push(super::section_label("grants"));
        for grant in &standing.grants {
            lines.push(Line::from(vec![
                Span::styled(grant.clone(), theme::fg(Token::TextPrimary)),
                Span::styled(
                    format!(" · {}", uze_application::grant_meaning(grant)),
                    theme::fg(Token::TextMuted),
                ),
            ]));
        }
    }
    if !standing.programs.is_empty() {
        lines.push(Line::from(""));
        lines.push(super::section_label("programs"));
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
            lines.push(Line::from(vec![
                Span::styled(program.name.clone(), theme::fg(Token::TextPrimary)),
                Span::styled(format!(" · {found}"), theme::fg(token)),
            ]));
            if !program.reads.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("reads {}", program.reads.join(", ")),
                    theme::fg(Token::TextMuted),
                )));
            }
        }
        let (confinement, token) = if standing.confined == Some(true) {
            (
                "confined: reads the checkout, writes only a scratch folder, no network",
                Token::TextMuted,
            )
        } else {
            (
                "runs as you: this machine cannot confine programs",
                Token::StateWarning,
            )
        };
        lines.push(Line::from(Span::styled(confinement, theme::fg(token))));
    }
    if !standing.preferences.is_empty() {
        lines.push(Line::from(""));
        lines.push(super::section_label("preferences"));
        for preference in &standing.preferences {
            lines.push(Line::from(vec![
                Span::styled(preference.label.clone(), theme::fg(Token::TextPrimary)),
                Span::styled(
                    format!(" · {}", preference.said()),
                    theme::fg(Token::TextMuted),
                ),
            ]));
        }
    }
    let mut said = Vec::new();
    if standing.condition == Condition::Held {
        said.push((
            format!(
                "its update wants {}, which you have not allowed",
                standing.new_grants.join(", ")
            ),
            Token::StateWarning,
        ));
    }
    if let Some(reason) = &standing.checkout_refused {
        said.push((format!("runs nothing here: {reason}"), Token::StateWarning));
    }
    if let Some(day) = standing.updated_on() {
        said.push((format!("updated {day}, same grants"), Token::TextMuted));
    }
    let elsewhere = standing
        .enabled_in
        .iter()
        .filter(|enabled| project.is_none_or(|project| enabled.as_path() != project))
        .count();
    if standing.enabled_everywhere {
        said.push(("enabled in every project".to_owned(), Token::TextMuted));
    } else if elsewhere > 0 {
        said.push((
            match elsewhere {
                1 => "enabled in 1 other project".to_owned(),
                n => format!("enabled in {n} other projects"),
            },
            Token::TextMuted,
        ));
    }
    if !said.is_empty() {
        lines.push(Line::from(""));
        lines.push(super::section_label("this project"));
        lines.extend(
            said.into_iter()
                .map(|(text, token)| Line::from(Span::styled(text, theme::fg(token)))),
        );
    }
    lines
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
            subtitle: "Its scripts answer in every project",
        },
        Condition::Enabled => DrawerStatus {
            color: theme::color(Token::StateSuccess),
            headline: "Enabled here",
            subtitle: "Its scripts answer in this project",
        },
        Condition::Consented | Condition::FixturesOnly | Condition::Broken => return None,
    })
}
