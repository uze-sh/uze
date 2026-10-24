//! The display nodes of `extension-catalog-growth`: what each asks for and
//! the rows it draws. They draw data alone, so each is a function from its
//! node and a width to lines; laying them out stays with the tree.

use std::cell::RefCell;
use std::collections::HashMap;

use uze_extensions::view::{ChartKind, ContentLine};

use super::*;
use crate::ui::extension_view::prose::prose_rows;

/// The most rows a `Markdown` or `Code` block takes, so a fixture cannot
/// grow a view without limit.
pub(super) const BLOCK_ROWS_MAX: usize = 200;
/// The columns a gauge's bar takes at least.
const GAUGE_BAR_MIN: u16 = 10;
/// The columns of a gauge's percentage: `100%`.
const PERCENT_WIDTH: u16 = 4;
/// The widest a key column of `Details` takes, in a share of the width.
const KEY_SHARE: u16 = 2;

pub(super) fn rule(label: Option<&str>, role: Role, width: u16) -> Line<'static> {
    let line = theme::glyph(Symbol::TreeDivider);
    let ink = Style::default().fg(color(Role::Faint));
    match label {
        None => Line::from(TextSpan::styled(line.repeat(usize::from(width)), ink)),
        Some(label) => {
            let said = text::elide(label, usize::from(width.saturating_sub(4)));
            let used = text::columns(&said) + 3;
            let rest = usize::from(width).saturating_sub(used);
            Line::from(vec![
                TextSpan::styled(line.clone(), ink),
                TextSpan::raw(" "),
                TextSpan::styled(said, Style::default().fg(color(role))),
                TextSpan::raw(" "),
                TextSpan::styled(line.repeat(rest), ink),
            ])
        }
    }
}

pub(super) fn rule_width(label: Option<&str>) -> u16 {
    label.map_or(1, |label| cells_of(text::columns(label)).saturating_add(4))
}

/// The key column of a `Details`: as wide as its widest key, up to half the
/// width.
pub(super) fn key_column(keys: &[&str], width: u16) -> u16 {
    let widest = keys
        .iter()
        .map(|key| cells_of(text::columns(key)))
        .max()
        .unwrap_or(0);
    widest.min(width / KEY_SHARE)
}

pub(super) fn entry(key: &str, value: &str, role: Role, keys: u16, width: u16) -> Line<'static> {
    let key = text::elide(key, usize::from(keys));
    let pad = usize::from(keys).saturating_sub(text::columns(&key)) + 2;
    let room = width.saturating_sub(keys + 2);
    let value = elided(&[Span::new(value, role)], room);
    let mut spans = vec![TextSpan::styled(
        format!("{key}{}", " ".repeat(pad)),
        theme::fg(Token::TextMuted),
    )];
    spans.extend(value.spans);
    Line::from(spans)
}

pub(super) fn gauge_width(label: &str) -> u16 {
    let label = cells_of(text::columns(label));
    let gap = u16::from(label > 0);
    label + gap + GAUGE_BAR_MIN + 1 + PERCENT_WIDTH
}

/// A gauge drawn with its label in a column `labels` wide, so the bars of
/// gauges stacked together start at the same place.
pub(super) fn gauge(
    label: &str,
    labels: u16,
    filled: u16,
    role: Role,
    width: u16,
) -> Line<'static> {
    let label_cells = labels.min(width / 2);
    let gap = u16::from(label_cells > 0);
    let bar = width.saturating_sub(label_cells + gap + 1 + PERCENT_WIDTH);
    let mut spans = Vec::new();
    if label_cells > 0 {
        let said = text::elide(label, usize::from(label_cells));
        let pad = usize::from(label_cells).saturating_sub(text::columns(&said)) + 1;
        spans.push(TextSpan::styled(
            format!("{said}{}", " ".repeat(pad)),
            theme::fg(Token::TextSecondary),
        ));
    }
    let segment = Segment {
        weight: 1,
        filled,
        role,
    };
    spans.extend(progress_line(&[segment], bar).spans);
    spans.push(TextSpan::styled(
        format!(" {:>3}%", (u32::from(filled) + 5) / 10),
        Style::default().fg(color(role)),
    ));
    Line::from(spans)
}

/// A level from thousandths: the frame of [`Symbol::ChartLevel`] it reads
/// as, `None` for nothing to draw.
fn level(of_thousand: u32, frames: usize) -> Option<usize> {
    if of_thousand == 0 || frames == 0 {
        return None;
    }
    let at = (of_thousand as usize * frames).div_ceil(1000);
    Some(at.clamp(1, frames) - 1)
}

/// A chart's rows, top first. A sparkline is one row of levels; a bar
/// chart stacks whole cells from the bottom and tops each with a level. As
/// many values as fit are drawn, the latest kept.
pub(super) fn chart(
    kind: ChartKind,
    levels: &[u16],
    height: u16,
    role: Role,
    width: u16,
) -> Vec<Line<'static>> {
    let frames = theme::frames(Symbol::ChartLevel);
    let shown = &levels[levels.len().saturating_sub(usize::from(width))..];
    let ink = Style::default().fg(color(role));
    let glyph =
        |index: Option<usize>| index.map_or_else(|| " ".to_owned(), |at| frames[at].clone());
    match kind {
        ChartKind::Sparkline => vec![Line::from(TextSpan::styled(
            shown
                .iter()
                .map(|value| glyph(level(u32::from(*value), frames.len())))
                .collect::<String>(),
            ink,
        ))],
        ChartKind::Bar => {
            let rows = u32::from(height.max(1));
            (0..rows)
                .map(|row| {
                    let from_bottom = rows - 1 - row;
                    let cells: String = shown
                        .iter()
                        .map(|value| {
                            // How much of this row the bar fills, in
                            // thousandths of a cell.
                            let scaled = u32::from(*value) * rows;
                            let below = from_bottom * 1000;
                            match scaled.checked_sub(below) {
                                Some(left) if left >= 1000 => {
                                    frames.last().cloned().unwrap_or_default()
                                }
                                Some(left) => glyph(level(left, frames.len())),
                                None => " ".to_owned(),
                            }
                        })
                        .collect();
                    Line::from(TextSpan::styled(cells, ink))
                })
                .collect()
        }
    }
}

pub(super) fn stat(
    value: &str,
    label: &str,
    delta: Option<&str>,
    role: Role,
    width: u16,
) -> Vec<Line<'static>> {
    let mut figure = vec![TextSpan::styled(
        text::elide(value, usize::from(width)),
        theme::fg_bold(Token::TextBright),
    )];
    if let Some(delta) = delta {
        let used = text::columns(value) + 2;
        if used + text::columns(delta) <= usize::from(width) {
            figure.push(TextSpan::raw("  "));
            figure.push(TextSpan::styled(
                delta.to_owned(),
                Style::default().fg(color(role)),
            ));
        }
    }
    vec![
        Line::from(figure),
        Line::from(TextSpan::styled(
            text::elide(label, usize::from(width)),
            theme::fg(Token::TextMuted),
        )),
    ]
}

pub(super) fn stat_width(value: &str, label: &str, delta: Option<&str>) -> u16 {
    let figure = text::columns(value) + delta.map_or(0, |delta| 2 + text::columns(delta));
    cells_of(figure.max(text::columns(label)))
}

pub(super) fn kbd(key: &str, width: u16) -> Line<'static> {
    Line::from(TextSpan::styled(
        text::elide(key, usize::from(width)),
        crate::ui::widget::hint::key_style(),
    ))
}

/// A folded block's text, syntax theme, width and whether it is cut.
type FoldedKey = (String, String, u16, bool);

thread_local! {
    /// Markdown and code folded per text, syntax theme and width: a
    /// Markdown parse with a highlighting pass is milliseconds a frame
    /// should not pay again. Small, and dropped whole when it fills.
    static FOLDED: RefCell<HashMap<FoldedKey, Vec<Line<'static>>>> =
        RefCell::new(HashMap::new());
}
const FOLDED_MAX: usize = 64;
/// The measure a `Code` block is laid out at before it is cut to its width.
const CODE_MEASURE: usize = 1024;

/// A `Markdown` node's rows at `width`: prose folded at word boundaries.
pub(super) fn markdown(source: &str, width: u16) -> Vec<Line<'static>> {
    folded(source, width, false)
}

/// A `Code` node's rows at `width`: the block highlighted for `language`,
/// each line cut at the width rather than wrapped.
pub(super) fn code(source: &str, language: &str, width: u16) -> Vec<Line<'static>> {
    let fence = "`".repeat(longest_backtick_run(source).max(2) + 1);
    let block = format!(
        "{fence}{language}\n{}\n{fence}\n",
        source.trim_end_matches('\n')
    );
    folded(&block, width, true)
}

fn longest_backtick_run(source: &str) -> usize {
    source
        .split(|ch| ch != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
}

fn folded(source: &str, width: u16, cut: bool) -> Vec<Line<'static>> {
    let theme_name = uze_theme::active().syntax_theme().to_owned();
    let key = (source.to_owned(), theme_name.clone(), width, cut);
    if let Some(lines) = FOLDED.with_borrow(|folded| folded.get(&key).cloned()) {
        return lines;
    }
    // Code is cut at the width, so it is laid out as if nothing bounded it.
    let measure = match cut {
        true => CODE_MEASURE,
        false => usize::from(width),
    };
    let content: Vec<ContentLine> = uze_extensions::code::markdown(source, &theme_name, measure);
    let mut lines: Vec<Line<'static>> = match cut {
        true => content
            .iter()
            .map(|line| elided(&line.spans, width))
            .collect(),
        false => content
            .iter()
            .flat_map(|line| prose_rows(line, usize::from(width)))
            .collect(),
    };
    // The blank row Markdown leaves after a block is the next block's air,
    // and a node has none after it.
    while lines
        .last()
        .is_some_and(|line| line.spans.iter().all(|span| span.content.trim().is_empty()))
    {
        lines.pop();
    }
    lines.truncate(BLOCK_ROWS_MAX);
    FOLDED.with_borrow_mut(|folded| {
        if folded.len() >= FOLDED_MAX {
            folded.clear();
        }
        folded.insert(key, lines.clone());
    });
    lines
}

/// The widest line of a block, for what it asks of a row.
pub(super) fn block_width(source: &str) -> u16 {
    cells_of(source.lines().map(text::columns).max().unwrap_or(0))
}
