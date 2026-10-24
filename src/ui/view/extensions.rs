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
use crate::ui::widget::RowState;
use crate::ui::{content_area, render_screen_header};

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

fn render_extension_card(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    extension: ExtensionEntry<'_>,
    enabled: bool,
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
    let offers = uze_application::application::offers::extension_offers(enabled);
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
        lines.push(Line::from(Span::styled(
            summary.identity.to_string(),
            theme::fg(Token::TextPrimary),
        )));
        if summary.linked {
            lines.push(Line::from(Span::styled(
                "linked · redrawn on every save",
                theme::fg(Token::TextMuted),
            )));
        }
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
    if let ExtensionEntry::Declared(summary) = extension
        && !summary.errors.is_empty()
    {
        lines.push(Line::from(""));
        lines.push(super::section_label("last compile"));
        lines.extend(
            summary.errors.iter().map(|error| {
                Line::from(Span::styled(error.clone(), theme::fg(Token::StateDanger)))
            }),
        );
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), body);

    let from_a_plugin = matches!(extension, ExtensionEntry::Declared(_));
    render_drawer_footer(
        frame,
        status,
        if enabled {
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
        .collect();
    let usage = match (summary.views.is_empty(), summary.sections.is_empty()) {
        (false, false) => "Its button in the workspace's tab strip, and its section in the sidebar",
        (false, true) => "Its button in the workspace's tab strip",
        (true, false) => "Its section in the workspace's sidebar",
        (true, true) => "Nothing to open until it compiles",
    };
    if surfaces.is_empty() {
        (vec!["none yet".to_owned()], usage.to_owned())
    } else {
        (surfaces, usage.to_owned())
    }
}
