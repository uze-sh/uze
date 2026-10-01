//! TUI view — Overview route.
//!
//! The machine dashboard: harnesses detected, plugins installed, marketplace
//! sources, and the global health line. Project-specific context belongs in
//! the dedicated project and harness views, not this machine-scoped overview.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::health::Severity;
use crate::ui::hit::Hit;
use crate::ui::model::{Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::stat::{self, Stat};
use crate::ui::{content_area, render_screen_header};

pub(crate) fn render_overview(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    _hits: &mut Vec<(Rect, Hit)>,
) {
    let area = content_area(area);
    let content = render_screen_header(frame, area, Route::Overview, None);

    let harness_total = model
        .remembered
        .doctor
        .as_ref()
        .map_or(0, |d| d.harnesses.len());
    let harness_detected = model.remembered.doctor.as_ref().map_or(0, |d| {
        d.harnesses.iter().filter(|h| h.detection.present).count()
    });
    let alerts = model.alerts();
    let mut y = content.y;

    // Status line: dot + headline + detail, matching the design's single
    // "All systems healthy — N harnesses detected, ..." summary row.
    let (color, headline) = if alerts.is_empty() {
        (theme::color(Token::StateSuccess), "All systems healthy")
    } else {
        (theme::color(Token::StateWarning), "Attention needed")
    };
    let detail = if alerts.is_empty() {
        format!(
            "— {harness_detected} harness{} detected",
            if harness_detected == 1 { "" } else { "es" }
        )
    } else {
        format!("— {} item(s) need attention", alerts.len())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", theme::glyph(Symbol::StatusSelected)),
                Style::default().fg(color),
            ),
            Span::styled(
                headline,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(detail, theme::fg(Token::TextMuted)),
        ])),
        Rect::new(content.x, y, content.width, 1),
    );
    y += 3;

    // 3-column stat grid, each cell divided from its neighbor by a left
    // hairline border — the design's `border-left:1px solid rgba(...)`.
    let stats = [
        Stat {
            label: "Harnesses detected".to_owned(),
            value: format!("{harness_detected}/{harness_total}"),
            hue: Token::TextBright,
            mark: None,
        },
        Stat {
            label: "Plugins installed".to_owned(),
            value: model.remembered.plugins.len().to_string(),
            hue: Token::TextBright,
            mark: None,
        },
        Stat {
            label: "Active profile".to_owned(),
            value: model
                .remembered
                .profiles
                .iter()
                .find(|profile| profile.active)
                .map_or_else(|| "none".to_owned(), |profile| profile.id.clone()),
            hue: Token::StateSuccess,
            mark: None,
        },
    ];
    if y + 1 < content.y + content.height {
        stat::cards(frame, Rect::new(content.x, y, content.width, 2), &stats);
    }
    y += 5;

    if alerts.is_empty() {
        return;
    }
    let bottom = content.y + content.height;
    if y < bottom {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Needs attention",
                theme::fg_bold(Token::TextMuted),
            )),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
    }
    let room = bottom.saturating_sub(y) as usize;
    // The last row says how many did not fit rather than cutting them off
    // in silence: a list of problems that ends at the frame's edge reads
    // as complete.
    let shown = if alerts.len() > room {
        room.saturating_sub(1)
    } else {
        alerts.len()
    };
    for alert in alerts.iter().take(shown) {
        let (symbol, color) = match alert.severity {
            Severity::High => (Symbol::MarkClose, theme::color(Token::StateDanger)),
            Severity::Medium => (Symbol::MarkAttention, theme::color(Token::StateWarning)),
            Severity::Low => (Symbol::MarkDot, theme::color(Token::Accent)),
        };
        let glyph = theme::glyph(symbol);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{glyph} "), Style::default().fg(color)),
                Span::styled(alert.label.clone(), theme::fg(Token::TextBright)),
                Span::styled(format!(" — {}", alert.detail), theme::fg(Token::TextMuted)),
            ])),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
    }
    if shown < alerts.len() && y < bottom {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("… {} more", alerts.len() - shown),
                theme::fg(Token::TextMuted),
            )),
            Rect::new(content.x, y, content.width, 1),
        );
    }
}
