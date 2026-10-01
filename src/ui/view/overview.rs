//! TUI view — Overview route.
//!
//! The machine dashboard: harnesses detected and plugins installed. Health
//! is the modal's footer's, on every screen; project-specific context
//! belongs in the dedicated project and harness views.

use ratatui::layout::Rect;

use crate::ui::hit::Hit;
use crate::ui::model::{Route, TuiModel};
use crate::ui::theme::Token;
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
    // Stat grid, each cell divided from its neighbor by a left
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
    ];
    if content.height > 1 {
        stat::cards(
            frame,
            Rect::new(content.x, content.y, content.width, 2),
            &stats,
        );
    }
}
