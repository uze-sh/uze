//! A pane's cells, drawn from its snapshot in the active palette.

use super::*;

pub(in crate::ui::orchestrator) fn render_pane(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
) {
    let Some(snapshot) = model.panes.get(&model.focused_pane()) else {
        frame.render_widget(
            Paragraph::new(model.error.as_deref().unwrap_or(" starting shell…"))
                .style(theme::fg(Token::TextMuted)),
            area,
        );
        return;
    };
    let width = area.width.min(snapshot.columns);
    let height = area.height.min(snapshot.rows);
    let palette = theme::Palette::active();
    let buffer = frame.buffer_mut();
    let mut encoded = [0u8; 4];
    for row in 0..height {
        for column in 0..width {
            let index = usize::from(row) * usize::from(snapshot.columns) + usize::from(column);
            if let Some(cell) = snapshot.cells.get(index) {
                let mut style = cell_style(cell, &palette);
                // Reversed against the cell's own colours rather than
                // tinted with one of ours: a pane's content can be any
                // colour at all, and inversion is the one mark that
                // stays legible over every one of them.
                if cell.attributes.selected {
                    style = if cell.attributes.inverse {
                        style.remove_modifier(Modifier::REVERSED)
                    } else {
                        style.add_modifier(Modifier::REVERSED)
                    };
                }
                buffer[(area.x + column, area.y + row)]
                    .set_symbol(cell.character.encode_utf8(&mut encoded))
                    .set_style(style);
            }
        }
    }
    if snapshot.cursor.row < height && snapshot.cursor.column < width {
        buffer[(
            area.x + snapshot.cursor.column,
            area.y + snapshot.cursor.row,
        )]
            .set_style(
                Style::default()
                    .bg(theme::color(Token::TextBright))
                    .fg(theme::color(Token::SurfaceBackground)),
            );
    }
}

pub(in crate::ui::orchestrator) fn cell_style(
    cell: &uze_terminal::RenderCell,
    palette: &theme::Palette,
) -> Style {
    let mut style = Style::default()
        .fg(color(cell.foreground, palette))
        .bg(color(cell.background, palette));
    if cell.attributes.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.attributes.dim {
        style = style.add_modifier(Modifier::DIM);
    }
    if cell.attributes.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.attributes.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.attributes.inverse {
        style = style.add_modifier(Modifier::REVERSED);
    }
    if cell.attributes.hidden {
        style = style.add_modifier(Modifier::HIDDEN);
    }
    if cell.attributes.strikeout {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}

pub(in crate::ui::orchestrator) fn color(color: TerminalColor, palette: &theme::Palette) -> Color {
    match color {
        TerminalColor::DefaultForeground => palette.color(Token::TextPrimary),
        TerminalColor::DefaultBackground => palette.color(Token::SurfaceBackground),
        TerminalColor::Rgb { red, green, blue } => theme::content(red, green, blue),
        // The 16 a program can name by index are the theme's, so a pane
        // cannot contradict the chrome drawn around it. Above 15 are the
        // 240 extended entries no theme defines — passed through as the
        // index they are.
        TerminalColor::Indexed(index) => palette.ansi(index).unwrap_or(Color::Indexed(index)),
    }
}
