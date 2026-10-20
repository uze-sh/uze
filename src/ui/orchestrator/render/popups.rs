//! What floats over the workspace: the agent picker, the context menu, a commit's account, the status catalog, the new agent's popup, the delivery notification and the action index.

use super::*;

/// The directories the new agent's popup lists at once: as many as match,
/// between [`NEW_AGENT_POPUP_MIN_ROWS`] and [`NEW_AGENT_POPUP_MAX_ROWS`].
pub(in crate::ui::orchestrator) fn new_agent_popup_rows(directories: usize) -> u16 {
    let rows = u16::try_from(directories).unwrap_or(u16::MAX);
    rows.clamp(NEW_AGENT_POPUP_MIN_ROWS, NEW_AGENT_POPUP_MAX_ROWS)
}

const NEW_AGENT_POPUP_MIN_ROWS: u16 = 4;
const NEW_AGENT_POPUP_MAX_ROWS: u16 = 9;
/// Wide enough for a directory's name beside what is known about it.
const NEW_AGENT_POPUP_WIDTH: u16 = 54;
/// The rows around the listing: the harness, the line typed into and the
/// foot, each closed off by a rule.
const NEW_AGENT_POPUP_CHROME: u16 = 6;

/// Where a popup of `width` by `height` floats beside `anchor`: under it,
/// pushed back inside `area` at its edges.
fn popup_beside(area: Rect, anchor: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        anchor.x.min((area.x + area.width).saturating_sub(width)),
        (anchor.y + anchor.height).min((area.y + area.height).saturating_sub(height)),
        width,
        height,
    )
}

/// The card a new agent is chosen in, cleared over whatever is beneath. A
/// card, not a floating surface: the popup is anchored to the control that
/// opened it and measures itself — its height budgets its two border rows
/// and nothing else, and each row leads with its own inset.
///
/// The whole card answers a click, ahead of its rows: one on the line
/// being typed into, the title or the border is inside the popup, and must
/// not read as one outside it.
fn new_agent_card(
    frame: &mut ratatui::Frame<'_>,
    popup: Rect,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> Rect {
    frame.render_widget(Clear, popup);
    hits.push((popup, WorkspaceHit::NewAgentBody));
    Surface::card().title(" new agent ").render(frame, popup)
}

/// A line with `left` at its start and `right` against its end, `right`
/// kept whole and `left` cut to what is left over.
fn spread(rect: Rect, left: Vec<Span<'static>>, right: Vec<Span<'static>>) -> Line<'static> {
    let right_width: usize = right.iter().map(Span::width).sum();
    let room = usize::from(rect.width).saturating_sub(right_width + usize::from(TRAILING_PAD));
    let mut spans = Vec::new();
    let mut used = 0;
    for span in left {
        let width = span.width();
        if used + width > room {
            let cut = text::elide(&span.content, room.saturating_sub(used));
            used += text::columns(&cut);
            spans.push(Span::styled(cut, span.style));
            break;
        }
        used += width;
        spans.push(span);
    }
    spans.push(Span::raw(" ".repeat(room.saturating_sub(used))));
    spans.extend(right);
    Line::from(spans)
}

/// A hairline across the popup, between two of its parts.
fn popup_rule(frame: &mut ratatui::Frame<'_>, rows: &mut Rows) {
    if let Some(rect) = rows.next(1) {
        let _ = Rule::new(Edge::Top).render(frame, rect);
    }
}

/// The new agent's popup, floating beside the `new` it was asked from,
/// over the pane, so nothing in the sidebar moves while it is open: the
/// harness it runs, the directory it runs in, and what Enter does.
pub(in crate::ui::orchestrator) fn render_directory_step(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let height = step.rows + NEW_AGENT_POPUP_CHROME + 2;
    let popup = popup_beside(area, step.anchor, NEW_AGENT_POPUP_WIDTH, height);
    let inner = new_agent_card(frame, popup, hits);
    let foot = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
    let mut rows = Rows::over(Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    });
    if let Some(rect) = rows.next(1) {
        render_harness_field(frame, rect, model, step, hits);
    }
    popup_rule(frame, &mut rows);
    render_root_picker(frame, model, step, &mut rows, hits);
    let mut above_foot = Rows::over(Rect::new(foot.x, foot.y.saturating_sub(1), foot.width, 1));
    popup_rule(frame, &mut above_foot);
    render_new_agent_foot(frame, foot, model, step, hits);
}

/// The scopes the popup's keys are read from.
const NEW_AGENT_SCOPES: [uze_keys::Scope; 2] =
    [uze_keys::Scope::Global, uze_keys::Scope::RootPicker];

/// The harness the agent will run, between the arrows that change it. A
/// click on either arrow is its key.
fn render_harness_field(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let arrow_hue = |action: Action| {
        if model.hovered == Some(WorkspaceHit::HintAction(action)) {
            Token::Accent
        } else {
            Token::TextFaint
        }
    };
    let left = vec![
        picker_lead(),
        Span::styled("harness", theme::fg(Token::TextDim)),
    ];
    let right = match step.chosen_harness() {
        Some(option) => {
            let mut right = vec![Span::styled(
                format!("{} ", theme::glyph(Symbol::StepPrevious)),
                theme::fg(arrow_hue(Action::FocusPrevious)),
            )];
            right.push(Span::styled(
                option.display_name.clone(),
                theme::fg_bold(Token::Accent),
            ));
            right.push(Span::styled(
                format!(" {}", theme::glyph(Symbol::StepNext)),
                theme::fg(arrow_hue(Action::FocusNext)),
            ));
            right
        }
        None => vec![Span::styled(NO_AGENT_SET_UP, theme::fg(Token::TextMuted))],
    };
    let right_width: u16 = right.iter().map(|span| span.width() as u16).sum();
    let end = rect.right().saturating_sub(TRAILING_PAD);
    if step.harnesses.len() > 1 {
        hits.push((
            Rect::new(end.saturating_sub(right_width), rect.y, 2, 1),
            WorkspaceHit::HintAction(Action::FocusPrevious),
        ));
        hits.push((
            Rect::new(end.saturating_sub(2), rect.y, 2, 1),
            WorkspaceHit::HintAction(Action::FocusNext),
        ));
    }
    frame.render_widget(Paragraph::new(spread(rect, left, right)), rect);
}

/// The popup's keys, Enter's first and the way out against the end: what
/// Enter does is the field and the row marked above, so the foot only
/// names the keys.
fn render_new_agent_foot(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let hovered = match model.hovered {
        Some(WorkspaceHit::HintAction(action)) => Some(action),
        _ => None,
    };
    let enter = if step.chosen_harness().is_some() {
        "new"
    } else {
        SET_ONE_UP
    };
    let keys = crate::ui::widget::hint::Hints::new(&NEW_AGENT_SCOPES)
        .named(Action::Activate, enter)
        .run(Action::FocusPrevious, Action::FocusNext, "harness")
        .hovered(hovered);
    let close = crate::ui::widget::hint::Hints::new(&NEW_AGENT_SCOPES)
        .named(Action::Dismiss, "close")
        .hovered(hovered);
    let close_width = close.width();
    let end = rect.right().saturating_sub(TRAILING_PAD);
    let start = rect.x + PICKER_LEAD as u16;
    let at = Rect {
        x: start,
        width: end.saturating_sub(start + close_width + 1),
        ..rect
    };
    let away = Rect {
        x: end.saturating_sub(close_width),
        width: close_width,
        ..rect
    };
    for (rect, action) in keys
        .render(frame, at)
        .into_iter()
        .chain(close.render(frame, away))
    {
        hits.push((rect, WorkspaceHit::HintAction(action)));
    }
}

/// The harness a resumed agent runs, listing `agent_options`, when the one
/// it ran with is gone. Not built on the management modal's dialog
/// helpers (those are shaped for static text, not a selectable,
/// hit-testable list) — this is self-contained, styled by hand to match
/// the same palette.
pub(in crate::ui::orchestrator) fn render_agent_picker(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    anchor: Rect,
    picker: &AgentPicker,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let content_width = picker
        .options
        .iter()
        .map(|option| option.display_name.chars().count() as u16)
        .max()
        .unwrap_or(16)
        .max(NO_AGENT_SET_UP.len() as u16);
    // Empty, it says so and offers the way out on a row of its own.
    let height = picker.options.len().max(2) as u16 + 2;
    let popup = popup_beside(area, anchor, content_width + 6, height);
    let inner = new_agent_card(frame, popup, hits);

    if picker.options.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {NO_AGENT_SET_UP}"),
                theme::fg(Token::TextMuted),
            )),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        if inner.height > 1 {
            let row = Rect::new(inner.x, inner.y + 1, inner.width, 1);
            render_agent_picker_row(frame, row, SET_ONE_UP, true);
            hits.push((row, WorkspaceHit::SetUpAgent));
        }
        return;
    }
    for (index, option) in picker.options.iter().enumerate() {
        if index as u16 >= inner.height {
            break;
        }
        let row = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
        render_agent_picker_row(frame, row, &option.display_name, index == picker.selected);
        hits.push((row, WorkspaceHit::PickAgent(index)));
    }
}

pub(super) const NO_AGENT_SET_UP: &str = "no agent set up yet";
pub(super) const SET_ONE_UP: &str = "set one up";

/// A filled bar for the selected row, not just bold text — a
/// narrowly-scoped exception to this design's usual no-filled-surfaces
/// rule, for one reason: a keyboard-navigable menu needs the affordance.
pub(super) fn render_agent_picker_row(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    label: &str,
    selected: bool,
) {
    let (style, text) = if selected {
        let style = Style::default()
            .bg(theme::color(Token::Accent))
            .fg(theme::color(Token::SurfaceBackground))
            .add_modifier(Modifier::BOLD);
        let text = format!(
            " {:<width$}",
            label,
            width = row.width.saturating_sub(1) as usize
        );
        (style, text)
    } else {
        (theme::fg(Token::TextInactive), format!(" {label}"))
    };
    frame.render_widget(Paragraph::new(Span::styled(text, style)), row);
}

/// The right-click action menu — one row per [`MenuAction`] in
/// `menu.items`, keyboard-navigable (Up/Down + Enter) and mouse-clickable,
/// same mechanics and neutral styling as [`render_agent_picker`] (anchored
/// just under the right-clicked row, selected row filled instead of just
/// bold — no action gets a special color of its own, `close` included, so
/// the menu reads as one consistent list rather than singling a row out).
/// See [`ContextMenu`]'s own doc comment for why closing specifically still
/// requires this menu instead of a direct click.
pub(in crate::ui::orchestrator) fn render_context_menu(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    menu: &ContextMenu,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let labels: Vec<&str> = menu.labels.iter().map(String::as_str).collect();
    let rows = widget::menu::render(frame, area, menu.anchor, &labels, menu.selected);
    // The open group, beside the row that opened it: anchored one row up
    // so its first entry sits level with that row.
    let opened = rows.get(menu.selected).copied();
    for (index, row) in rows.into_iter().enumerate() {
        hits.push((row, WorkspaceHit::ContextMenuAction(index)));
    }
    if let (Some(row), Some(labels), Some(highlighted)) =
        (opened, menu.submenu_labels(), menu.submenu)
    {
        let anchor = Rect::new(row.right() + 1, row.y.saturating_sub(2), 1, 1);
        let rows = widget::menu::render(frame, area, anchor, &labels, highlighted);
        for (index, row) in rows.into_iter().enumerate() {
            hits.push((row, WorkspaceHit::ContextSubmenuAction(index)));
        }
    }
}

/// Where the commit popup goes and what it says, resolved once for both
/// drawing it and bounding its scroll.
pub(in crate::ui::orchestrator) struct CommitDetailLayout {
    pub(in crate::ui::orchestrator) rect: Rect,
    pub(in crate::ui::orchestrator) inner: Rect,
    pub(super) lines: Vec<Line<'static>>,
    /// The rows the text takes once wrapped to `inner`.
    pub(in crate::ui::orchestrator) content_rows: u16,
}

impl CommitDetailLayout {
    /// The furthest the text can be scrolled and still fill the popup —
    /// what the wheel is held to, so it never scrolls into blank rows.
    pub(in crate::ui::orchestrator) fn scroll_limit(&self) -> u16 {
        self.content_rows.saturating_sub(self.inner.height)
    }
}

/// One commit's account, beside the timeline row it was opened from and in
/// the pane's own columns — who, when, what it said, how much it touched,
/// and the branches and tags standing at it — the shape the support
/// dropdown already gives a fact sheet. A frame too narrow to fit it
/// beside the sidebar gets it over the pane instead, inset. Never wider
/// or taller than a hover card ought to be: a long message scrolls
/// inside it rather than growing it over the pane.
pub(in crate::ui::orchestrator) fn commit_detail_layout(
    area: Rect,
    popup: &CommitDetailPopup,
) -> CommitDetailLayout {
    const MAX_WIDTH: u16 = 72;
    const MAX_HEIGHT: u16 = 20;
    const MIN_BESIDE_WIDTH: u16 = 40;
    let detail = &popup.detail;

    let beside = popup.anchor.right() + 1;
    let (x, width) = if area.right().saturating_sub(beside + 1) >= MIN_BESIDE_WIDTH {
        (beside, (area.right() - beside - 1).min(MAX_WIDTH))
    } else {
        let width = area.width.saturating_sub(4).clamp(1, MAX_WIDTH);
        (area.x + area.width.saturating_sub(width) / 2, width)
    };
    let inner_width = usize::from(width.saturating_sub(2 + 2 * POPUP_H_PAD).max(1));

    let mut lines = vec![
        row::title_row("commit", "esc", inner_width),
        Line::default(),
        Line::from(vec![
            Span::styled(
                format!("{} ", theme::glyph(Symbol::MarkToggleOn)),
                theme::fg(Token::StateInfo),
            ),
            Span::styled(detail.author.clone(), theme::fg(Token::TextPrimary)),
            Span::styled(
                {
                    let separator = theme::glyph(Symbol::HintSeparator);
                    format!("{separator}{}{separator}{}", detail.age, detail.date)
                },
                theme::fg(Token::TextSecondary),
            ),
        ]),
        Line::from(Span::styled(
            detail.subject.clone(),
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        )),
    ];
    if !detail.body.is_empty() {
        lines.push(Line::default());
        lines.extend(rendered_commit_body(&detail.body, inner_width));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "{} file{} changed",
                detail.files_changed,
                if detail.files_changed == 1 { "" } else { "s" }
            ),
            theme::fg(Token::TextSecondary),
        ),
        Span::styled(
            format!("  +{}", detail.insertions),
            theme::fg(Token::StateSuccess),
        ),
        Span::styled(
            format!("  −{}", detail.deletions),
            theme::fg(Token::StateDanger),
        ),
    ]));
    // The target's label wears the target's gold — the hue the timeline
    // gives what has landed in it — and so does its remote-tracking twin;
    // every other ref at the commit is blue, like a commit still ahead.
    let is_target = |reference: &str| {
        popup.target.as_deref().is_some_and(|target| {
            reference == target
                || reference
                    .strip_suffix(target)
                    .is_some_and(|remote| remote.ends_with('/'))
        })
    };
    let mut footer: Vec<Span<'static>> = Vec::new();
    for reference in &detail.refs {
        if !footer.is_empty() {
            footer.push(Span::raw(" "));
        }
        let hue = if is_target(reference) {
            theme::color(Token::StateWarning)
        } else {
            theme::color(Token::StateInfo)
        };
        footer.push(Span::styled(
            format!(" {reference} "),
            theme::fg(Token::SurfaceBackground).bg(hue),
        ));
    }
    let used: usize = footer.iter().map(Span::width).sum();
    let gap = inner_width.saturating_sub(used + detail.short_hash.chars().count());
    footer.push(Span::raw(" ".repeat(gap.max(1))));
    footer.push(Span::styled(
        detail.short_hash.clone(),
        theme::fg(Token::TextMuted),
    ));
    lines.push(Line::from(footer));

    let content_rows: u16 = lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(inner_width) as u16)
        .sum();
    let height = (content_rows + 2 + 2 * POPUP_V_PAD)
        .min(area.height)
        .clamp(1, MAX_HEIGHT);
    let rect = Rect::new(
        x,
        popup.anchor.y.min(area.bottom().saturating_sub(height)),
        width,
        height,
    );
    let inner = commit_detail_block()
        .padding(Padding::new(
            POPUP_H_PAD,
            POPUP_H_PAD,
            POPUP_V_PAD,
            POPUP_V_PAD,
        ))
        .inner(rect);
    CommitDetailLayout {
        rect,
        inner,
        lines,
        content_rows,
    }
}

thread_local! {
    static RENDERED_COMMIT_BODY: std::cell::RefCell<Option<RenderedCommitBody>> =
        const { std::cell::RefCell::new(None) };
}

struct RenderedCommitBody {
    body: String,
    theme: String,
    width: usize,
    rows: Vec<Line<'static>>,
}

/// The message body read as the Markdown it is written in, and folded at
/// word boundaries to the popup's width. Git hard-wraps a body near 72
/// columns; drawn line by line in a narrower card, every one of those
/// lines wrapped again and left a word stranded on a row of its own.
/// Rendered once per body, theme and width: the layout is asked on every
/// draw and every scroll.
fn rendered_commit_body(body: &str, width: usize) -> Vec<Line<'static>> {
    let theme = uze_theme::active().syntax_theme().to_owned();
    RENDERED_COMMIT_BODY.with_borrow_mut(|cached| {
        let current = cached.as_ref().is_some_and(|cached| {
            cached.body == body && cached.theme == theme && cached.width == width
        });
        if !current {
            let rows = uze_extensions::code::markdown(body, &theme, width)
                .iter()
                .flat_map(|line| crate::ui::extension_view::prose_rows(line, width))
                .collect();
            *cached = Some(RenderedCommitBody {
                body: body.to_owned(),
                theme,
                width,
                rows,
            });
        }
        cached
            .as_ref()
            .map(|cached| cached.rows.clone())
            .unwrap_or_default()
    })
}

pub(super) fn commit_detail_block() -> Block<'static> {
    Surface::card().into_block()
}

pub(in crate::ui::orchestrator) fn render_commit_detail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    popup: &CommitDetailPopup,
) {
    let layout = commit_detail_layout(area, popup);
    let scroll = popup.scroll.min(layout.scroll_limit());
    frame.render_widget(Clear, layout.rect);
    frame.render_widget(commit_detail_block(), layout.rect);
    frame.render_widget(
        Paragraph::new(layout.lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        layout.inner,
    );
}

/// The legend for the two status columns an agent row carries, opened by
/// clicking the task mark (see [`WorkspaceHit::OpenStatusCatalog`]).
///
/// Every row is generated from the same tables the sidebar draws with —
/// [`task_mark`] and [`AgentTabStatus::glyph`]/`color` — so a glyph or a
/// hue can never say one thing in the row and another in its own legend.
/// Adding a state to either enum shows up here by itself; only the
/// sentence explaining it is written by hand.
pub(in crate::ui::orchestrator) fn render_status_catalog(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    anchor: Rect,
    tick: usize,
) {
    // The agent column answers "what is the process doing", the work
    // column "where does the work in its checkout stand" — two questions
    // about the same row, which is exactly why they are two columns and
    // why one legend has to carry both.
    //
    // Both answer for every agent. The work column used to be readable
    // only for an isolated one, so an operator who launched an agent in
    // the project's own root met eight marks that never appeared and
    // reasonably concluded it was broken.
    let agent_rows: Vec<(String, Color, &str, &str)> = [
        (
            AgentTabStatus::Working,
            "working",
            "producing output right now",
        ),
        (
            AgentTabStatus::Completed,
            "completed",
            "finished while you were elsewhere",
        ),
        (
            AgentTabStatus::Selected,
            "here",
            "the tab you are typing into",
        ),
        (AgentTabStatus::Idle, "idle", "quiet, and not where you are"),
    ]
    .into_iter()
    .map(|(status, name, meaning)| {
        (
            status.glyph(tick).trim_end().to_owned(),
            status.color(),
            name,
            meaning,
        )
    })
    .collect();

    // `Running` is absent on purpose: it is the state that draws no mark,
    // because a task with a clean tree and nothing ahead has nothing to
    // report yet — and "the agent is alive" is the other column's answer,
    // which it gives with a spinner. A legend of marks that names a state
    // with no mark leaves a blank glyph and two rows meaning the same
    // thing. The `filter_map` below keeps that true for whatever is added
    // here next.
    let task_rows: Vec<(String, Color, &str, &str)> = [
        (
            WorkStateView::Uncommitted,
            "uncommitted",
            "changes in the checkout, not committed",
        ),
        (
            WorkStateView::Ready,
            "ready",
            "commits ahead on a clean tree — deliverable",
        ),
        (
            WorkStateView::Published,
            "published",
            "on the remote, level with it, no request yet",
        ),
        (
            WorkStateView::InReview,
            "in review",
            "a request is open for it — with its reviewer",
        ),
        (
            WorkStateView::Integrating,
            "delivering",
            "the rebase, the gate, and the merge or push, in flight",
        ),
        (
            WorkStateView::Conflicted { files: Vec::new() },
            "conflict",
            "the rebase stopped; resolve it in the checkout",
        ),
        (
            WorkStateView::GateFailed,
            "checks failed",
            "the gate failed on the rebased commits",
        ),
        (
            WorkStateView::Integrated,
            "delivered",
            "the work is in the target",
        ),
        (
            WorkStateView::Shelved,
            "unfinished",
            "no agent left; its work is kept for it",
        ),
    ]
    .into_iter()
    .filter_map(|(state, name, meaning)| {
        let (mark, hue) = task_mark(&state)?;
        Some((mark.to_owned(), hue, name, meaning))
    })
    .collect();

    /// Tighter than a popup's own inset: the catalog is a table, and its
    /// columns carry the separation an inset would otherwise provide.
    const CATALOG_H_PAD: u16 = 1;
    const GLYPH_COLUMN: usize = 3;
    let name_column = agent_rows
        .iter()
        .chain(&task_rows)
        .map(|(_, _, name, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    let content_width = agent_rows
        .iter()
        .chain(&task_rows)
        .map(|(_, _, _, meaning)| GLYPH_COLUMN + name_column + 2 + meaning.chars().count())
        .max()
        .unwrap_or(0) as u16;

    let mut lines: Vec<Line<'static>> = Vec::new();
    let section =
        |title: &str, rows: &[(String, Color, &str, &str)], lines: &mut Vec<Line<'static>>| {
            lines.push(Line::from(Span::styled(
                title.to_owned(),
                theme::fg(Token::TextMuted),
            )));
            for (glyph, hue, name, meaning) in rows {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{glyph:<GLYPH_COLUMN$}"),
                        Style::default().fg(*hue).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{name:<name_column$}  "),
                        theme::fg(Token::TextPrimary),
                    ),
                    Span::styled((*meaning).to_owned(), theme::fg(Token::TextSecondary)),
                ]));
            }
        };
    section("AGENT", &agent_rows, &mut lines);
    lines.push(Line::from(""));
    section("WORK", &task_rows, &mut lines);

    let width = (content_width + 2 * CATALOG_H_PAD + 2).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    // Anchored to the glyph that was clicked, like every other dropdown
    // here — and pulled back inside the frame when that glyph sits too
    // close to an edge for the popup to fit beside it.
    let popup = Rect::new(
        anchor.x.min((area.x + area.width).saturating_sub(width)),
        (anchor.y + anchor.height).min((area.y + area.height).saturating_sub(height)),
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    // The catalog's own horizontal inset, and no row above: its first
    // line is a heading the title already names.
    let inner = Surface::floating()
        .title(" status ")
        .padding(Padding::new(CATALOG_H_PAD, CATALOG_H_PAD, 0, 0))
        .render(frame, popup);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The directory step's prompt and the directories it currently matches,
/// one row each in `rows`, on the popup's own ground: no row is lifted, and
/// the one Enter takes wears the bar the harness step's row does.
fn render_root_picker(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let picker = &step.picker;
    render_query_row(frame, picker, rows);
    popup_rule(frame, rows);
    if picker.match_count() == 0 {
        if let Some(rect) = rows.next(1) {
            let spans = vec![
                picker_lead(),
                Span::styled("no directory matches", theme::fg(Token::TextFaint)),
            ];
            frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        }
        return;
    }
    let visible = usize::from(step.rows);
    let start = picker.window_start(visible);
    let needle = picker.input().to_lowercase();
    for (index, candidate) in picker.matches().enumerate().skip(start).take(visible) {
        let Some(rect) = rows.next(1) else { return };
        hits.push((rect, WorkspaceHit::PickDirectory(index)));
        let mark = directory_mark(model, step.space, candidate);
        // The row the keyboard is on wears the bar a menu's row does: the
        // one Enter takes.
        if picker.selection() == Some(index) {
            // Cut at its end like every other row, leaving the mark its
            // room and a column between them.
            let width = text::columns(&mark.text) as u16;
            let gap = if width > 0 { width + 1 } else { 0 };
            let room = usize::from(
                rect.width
                    .saturating_sub(PICKER_LEAD as u16 + gap + TRAILING_PAD),
            );
            render_agent_picker_row(frame, rect, &text::elide(&candidate.name, room), true);
            let at = Rect {
                x: rect.right().saturating_sub(width + TRAILING_PAD),
                width,
                ..rect
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    mark.text,
                    Style::default()
                        .bg(theme::color(Token::Accent))
                        .fg(theme::color(Token::SurfaceBackground)),
                )),
                at,
            );
            continue;
        }
        // A name the query is the head of says so, in the hue of the query
        // itself; a match found further in says nothing, rather than
        // colouring letters that had nothing to do with it.
        let (matched, rest) = split_at_head(&candidate.name, &needle);
        let left = vec![
            picker_lead(),
            Span::styled(matched, theme::fg(Token::Accent)),
            Span::styled(rest, theme::fg(Token::TextInactive)),
        ];
        let right = vec![Span::styled(mark.text, theme::fg(mark.hue))];
        frame.render_widget(Paragraph::new(spread(rect, left, right)), rect);
    }
}

/// What a row of the listing says about its directory, against its end.
struct DirectoryMark {
    text: String,
    hue: Token,
}

/// A project the space's agents already work in says so, with the branch
/// its checkout is on when it was read — the directory a second agent in
/// the space most likely wants. Any other repository says it is one;
/// a plain directory says nothing.
fn directory_mark(
    model: &WorkspaceModel,
    space: SpaceId,
    candidate: &crate::ui::root_picker::Candidate,
) -> DirectoryMark {
    let in_space = model
        .projects_of(space)
        .iter()
        .any(|project| uze_platform::path::same_path(project, &candidate.path));
    if in_space {
        let text = match model.branch_of(&candidate.path) {
            Some(branch) => format!("space {} {branch}", theme::glyph(Symbol::HintSeparator)),
            None => "space".to_owned(),
        };
        return DirectoryMark {
            text,
            hue: Token::StateSuccess,
        };
    }
    DirectoryMark {
        text: if candidate.repository == Some(true) {
            "git".to_owned()
        } else {
            String::new()
        },
        hue: Token::TextFaint,
    }
}

/// `text` split after the head `needle` names, when it names one at all.
/// `needle` is lowercase, and each character of `text` is lowered on its
/// own as it is compared: lowering can change how many characters there
/// are (`İ` is two), so a count taken on the lowered text cuts the
/// original in the wrong place.
pub(super) fn split_at_head(text: &str, needle: &str) -> (String, String) {
    let unmatched = || (String::new(), text.to_owned());
    if needle.is_empty() {
        return unmatched();
    }
    let mut wanted = needle.chars().peekable();
    for (index, character) in text.char_indices() {
        for lowered in character.to_lowercase() {
            match wanted.next() {
                Some(expected) if expected == lowered => {}
                Some(_) => return unmatched(),
                None => break,
            }
        }
        if wanted.peek().is_none() {
            let cut = index + character.len_utf8();
            return (text[..cut].to_owned(), text[cut..].to_owned());
        }
    }
    unmatched()
}

/// The column every row of the picker starts in — the kinds, what is being
/// typed, and each directory offered — with the row's own leading cell left
/// to the mark that says where the keyboard is.
pub(super) fn picker_lead() -> Span<'static> {
    Span::raw(" ".repeat(PICKER_LEAD))
}

/// The width of that leading cell.
pub(super) const PICKER_LEAD: usize = 1;

/// The gap on either side of the rule between the header's two controls.
pub(super) const HEADER_GAP: u16 = 1;

/// The row being typed into: what is being looked for, with the directory
/// it is being looked for in at the row's other end — a prompt that opened
/// with that path already typed into it asked to be deleted before it could
/// be used. The accent down its leading column says this is the row the
/// keyboard is in.
/// What the line typed into says while nothing is.
const FILTER_PLACEHOLDER: &str = "filter directories";

pub(super) fn render_query_row(
    frame: &mut ratatui::Frame<'_>,
    picker: &RootPicker,
    rows: &mut Rows,
) {
    let Some(rect) = rows.next(1) else { return };
    let needle = picker.input();
    // Where the typing starts from, ahead of it: a path typed out from `~`
    // or the root leaves it behind, and says the whole of itself.
    let absolute = needle.starts_with('~') || needle.starts_with(std::path::is_separator);
    let count = match picker.selection() {
        Some(index) => format!("{}/{}", index + 1, picker.match_count()),
        None => picker.match_count().to_string(),
    };
    let mut left = vec![picker_lead()];
    if !absolute {
        let origin = crate::ui::display_project_path(picker.origin());
        let separator = std::path::MAIN_SEPARATOR;
        let origin = if origin.ends_with(separator) {
            origin
        } else {
            format!("{origin}{separator}")
        };
        // What is typed is kept whole and the origin gives way, from its
        // head, down to a third of the line.
        let placeholder = if needle.is_empty() {
            text::columns(FILTER_PLACEHOLDER)
        } else {
            0
        };
        let kept = PICKER_LEAD + text::columns(needle) + 1 + placeholder + count.len() + 2;
        let room = usize::from(rect.width)
            .saturating_sub(kept)
            .max(usize::from(rect.width) / 3);
        left.push(Span::styled(
            text::elide_head(&origin, room),
            theme::fg(Token::TextDim),
        ));
    }
    // What is typed is a path too, and gives way the same way, from its
    // head: its end, where the cursor is, is what is being typed.
    let taken: usize = left.iter().map(Span::width).sum();
    let room = usize::from(rect.width).saturating_sub(taken + 1 + count.len() + 2);
    left.push(Span::styled(
        text::elide_head(needle, room),
        theme::fg_bold(Token::TextBright),
    ));
    left.push(Span::styled(
        theme::glyph(Symbol::CursorText),
        theme::fg(Token::Accent),
    ));
    if needle.is_empty() {
        left.push(Span::styled(
            FILTER_PLACEHOLDER,
            theme::fg(Token::TextFaint),
        ));
    }
    let right = vec![Span::styled(count, theme::fg(Token::TextFaint))];
    frame.render_widget(Paragraph::new(spread(rect, left, right)), rect);
}

pub(super) fn chip_state(model: &WorkspaceModel, hit: WorkspaceHit) -> ChipState {
    model.pointer_on(hit)
}

/// The work-state status for a task: what the delivery is about, where it
/// stands, and the hue that says so.
///
/// Two parts and no forge's word for either. The subject is the request's
/// own number, or where the work is going (see [`delivery_subject`]); the
/// standing is [`task_tone`]'s own mark, so the strip, the sidebar row and
/// the status legend cannot come to say different things about one state.
/// Whether a click answers it is not this function's question: that is
/// [`WorkspaceModel::status_answer`]'s.
///
/// `Uncommitted` is the exception that asks it. The diff count beside
/// this zone already reports uncommitted changes, and is the shortcut to
/// them, so a status saying so again is drawn only when the project asks
/// something there: then it is the control that sends it, not a report.
pub(super) fn status_notification(
    task: &AgentView,
    state: &WorkStateView,
    tick: usize,
) -> Option<(String, Token)> {
    if *state == WorkStateView::Uncommitted {
        task.action.as_ref()?;
        let (symbol, hue) = task_tone(state)?;
        return Some((format!("{} uncommitted", theme::glyph(symbol)), hue));
    }
    // Which states put a status in this zone: the ones handing work over
    // passes through. `Shelved` carries a mark of its own in the sidebar,
    // but it is a fact about a checkout rather than a step of a delivery,
    // and this zone is the second.
    if !matches!(
        state,
        WorkStateView::Ready
            | WorkStateView::Published
            | WorkStateView::InReview
            | WorkStateView::GateFailed
            | WorkStateView::Conflicted { .. }
            | WorkStateView::Integrating
            | WorkStateView::Integrated
    ) {
        return None;
    }
    let (symbol, hue) = task_tone(state)?;
    // In review, the request's own number is the whole of it: `#259` says
    // a request is open, and a mark beside it only repeats that.
    if *state == WorkStateView::InReview {
        return Some((delivery_subject(task), hue));
    }
    let standing = match state {
        // The one state `task_tone` cannot lend its glyph to. Its mark is
        // the vocabulary's "there is work to hand over", and the patched
        // set draws that as a create-a-request icon, which is a claim about
        // a forge this status does not make. What is waiting is commits,
        // so the count wears the arrow that means exactly that.
        //
        // The count is what is not on the remote yet once the branch is
        // there, and what the branch has beyond its base before that.
        WorkStateView::Ready => format!(
            "{}{}",
            theme::glyph(Symbol::SyncAhead),
            task.unsynced.unwrap_or(task.ahead)
        ),
        // The one status in this row that is also work in progress, so it
        // is the one that moves: a rebase, a gate and a push take as long
        // as the project's checks do, and a still mark for that many
        // seconds reads as a screen that has stopped. The sidebar's mark
        // keeps `Symbol::Ellipsis` — a single cell has no room to turn.
        WorkStateView::Integrating => agent_activity_frame(tick),
        _ => theme::glyph(symbol),
    };
    Some((format!("{} {standing}", delivery_subject(task)), hue))
}

/// What a delivery is *about*, in the fewest characters that name it: the
/// request's own number once the forge has one, and where the work is
/// going until then. The same whatever the project's delivery, since a
/// request is a fact of the remote, whoever opened it.
///
/// A number needs no word in front of it: `#41` is already unambiguous.
/// The branch's published name is not here either: it is the one part of
/// this that has no bound on its length, and a zone that changes width
/// with a branch name moves every button left of it.
pub(super) fn delivery_subject(task: &AgentView) -> String {
    match task.published_request {
        Some(request) => format!("#{request}"),
        None => format!("{} {}", theme::glyph(Symbol::ArrowTo), task.target),
    }
}

/// Everything that can be done here, each with the key that reaches it.
///
/// The workspace had no such surface at all: two of its most useful
/// gestures were reachable only by someone who had read the source. Every
/// word here comes from the action and every key from the keymap, so it is
/// right by construction and stays right after a rebind.
pub(in crate::ui::orchestrator) fn render_action_index(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    index: &ActionIndexOverlay,
    disabled: &std::collections::BTreeSet<String>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let rows = action_index_rows(&index.scopes, &index.offered, &index.filter, disabled);
    let reachable = action_index_rows(&index.scopes, &index.offered, "", disabled).len();
    let mut answering = index.scopes.clone();
    answering.push(uze_keys::Scope::ActionIndex);
    let hint = crate::ui::widget::dialog::border_hint(
        &answering,
        &[
            (uze_keys::Action::Activate, "run"),
            (uze_keys::Action::Dismiss, "close"),
        ],
    );
    let entries = action_index::render(
        frame,
        area,
        &rows,
        reachable,
        &index.filter,
        index.selected,
        hint,
        WorkspaceHit::ActionIndexEntry,
    );
    // Prepended: what is underneath must not answer a click meant here.
    hits.splice(0..0, entries);
}

#[cfg(test)]
mod tests {
    use super::split_at_head;
    use ratatui::layout::Rect;

    #[test]
    fn the_head_a_query_names_is_cut_where_it_ends_in_the_original() {
        assert_eq!(
            split_at_head("Projects", "pro"),
            ("Pro".into(), "jects".into())
        );
        assert_eq!(
            split_at_head("İstanbul", &"İs".to_lowercase()),
            ("İs".into(), "tanbul".into())
        );
        assert_eq!(split_at_head("uze", "uzer"), (String::new(), "uze".into()));
        assert_eq!(split_at_head("docs", "ox"), (String::new(), "docs".into()));
        assert_eq!(split_at_head("docs", ""), (String::new(), "docs".into()));
    }

    /// The picker measures itself: `height` budgets its two border rows and
    /// nothing else, because each row carries its own lead. Drawn on a surface
    /// that insets as well, the last option fell outside the box — and with
    /// one harness installed there was no option left to draw at all, which is
    /// what "never showed 'agent 1'" was.
    #[test]
    fn the_agent_picker_draws_every_option_it_sized_itself_for() {
        use ratatui::{Terminal, backend::TestBackend};

        use crate::ui::orchestrator::render;

        let option = |name: &str| crate::ui::orchestrator::AgentOption {
            display_name: name.to_owned(),
            integration: "claude-code".to_owned(),
            command: vec!["claude".to_owned()],
            continuity_gap: None,
        };
        let drawn = |names: &[&str]| {
            let picker = crate::ui::orchestrator::AgentPicker {
                options: names.iter().map(|name| option(name)).collect(),
                selected: 0,
                anchor: Rect::new(2, 1, 3, 1),
                launch: crate::ui::orchestrator::AgentLaunch::Resume(
                    crate::ui::orchestrator::ResumeTarget {
                        primary: "/repo".into(),
                        task: "t1".into(),
                        replacing: None,
                        harness: None,
                    },
                ),
            };
            let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
            let mut hits = Vec::new();
            terminal
                .draw(|frame| {
                    render::render_agent_picker(
                        frame,
                        Rect::new(0, 0, 60, 20),
                        picker.anchor,
                        &picker,
                        &mut hits,
                    );
                })
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            (0..20)
                .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        };

        let one = drawn(&["Claude Code"]);
        assert!(
            one.contains("Claude Code"),
            "the only harness installed is drawn:\n{one}"
        );

        let three = drawn(&["Claude Code", "Codex", "OpenCode"]);
        for name in ["Claude Code", "Codex", "OpenCode"] {
            assert!(three.contains(name), "{name} is drawn:\n{three}");
        }
    }
}
