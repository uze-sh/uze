//! What floats over the workspace: the agent picker, the context menu, a commit's account, the status catalog, the root picker, the delivery notification and the action index.

use super::*;

/// A small popup listing `agent_options`, opened by the selected space
/// header's "✦ new" — a dropdown anchored just below it, creating the
/// picked agent as a new tab in that space. Not built on
/// the management modal's dialog helpers (those are shaped for static
/// text, not a selectable, hit-testable list) — this is self-contained,
/// styled by hand to match the same palette.
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
    let rows = picker.options.len().max(2) as u16;
    let width = (content_width + 6).min(area.width);
    let height = (rows + 2).min(area.height);
    let popup = Rect::new(
        anchor.x.min((area.x + area.width).saturating_sub(width)),
        (anchor.y + anchor.height).min((area.y + area.height).saturating_sub(height)),
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    // A card, not a floating surface: this menu is anchored to the control
    // that opened it and measures itself — `height` budgets its two border
    // rows and nothing else, and each row leads with its own inset (below).
    // Given the floating inset on top of that, the last option fell
    // outside the box, and with one harness installed there was nothing
    // left to draw at all.
    let inner = Surface::card().title(" new agent ").render(frame, popup);

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
    let labels: Vec<String> = menu.items.iter().map(|action| action.label()).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let rows = widget::menu::render(frame, area, menu.anchor, &labels, menu.selected);
    for (index, row) in rows.into_iter().enumerate() {
        hits.push((row, WorkspaceHit::ContextMenuAction(index)));
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
        (area.x + (area.width - width) / 2, width)
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
        lines.extend(detail.body.lines().map(|line| {
            Line::from(Span::styled(
                line.to_owned(),
                theme::fg(Token::TextSecondary),
            ))
        }));
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
            "on the remote, level with it — with its reviewer",
        ),
        (
            WorkStateView::Integrating,
            "delivering",
            "the rebase, the gate and the push, in flight",
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
            WorkStateView::Parked,
            "parked",
            "no agent left; the work is still there",
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

/// The "+ space" prompt and the directories it currently matches, drawn as
/// rows of the sidebar itself rather than a floating popup: the prompt is
/// choosing where the next space in this very list goes. It stands where
/// the first space's header stands, with the listing directly under it the
/// way a space's tabs sit under theirs.
///
/// The kinds and the directories they would be created in share the darker
/// of the column's two surfaces — they are the panel — and what is being
/// typed stands on the lighter one, as does whichever directory it has
/// landed on: the two rows that answer to the keyboard are the two that are
/// lifted.
pub(super) fn render_root_picker(
    frame: &mut ratatui::Frame<'_>,
    picker: &RootPicker,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    render_query_row(frame, picker, rows);
    if picker.match_count() == 0 {
        if let Some(rect) = rows.next(1) {
            let mut spans = vec![
                picker_lead(),
                Span::styled("no directory matches", theme::fg(Token::TextFaint)),
            ];
            row::pad_to(
                &mut spans,
                rect.width,
                theme::color(Token::SurfaceRaisedSubtle),
            );
            frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        }
        return;
    }
    // The picker has the column to itself, so it offers as many
    // directories as the column has rows — one held back for the tail that
    // says how many more there are.
    let visible = usize::from(rows.remaining()).saturating_sub(1).max(1);
    let start = picker.window_start(visible);
    let needle = picker.input().to_lowercase();
    for (index, candidate) in picker.matches().enumerate().skip(start).take(visible) {
        let Some(rect) = rows.next(1) else { return };
        let selected = picker.selection() == Some(index);
        // A name the query is the head of says so, in the hue of the query
        // itself; a match found further in says nothing, rather than
        // colouring letters that had nothing to do with it.
        let (matched, rest) = split_at_head(&candidate.name, &needle);
        let rest_hue = if selected {
            Token::TextBright
        } else {
            Token::TextInactive
        };
        let mut spans = vec![
            picker_lead(),
            Span::styled(matched, theme::fg(Token::Accent)),
            Span::styled(rest, theme::fg(rest_hue)),
        ];
        row::pad_to(
            &mut spans,
            rect.width,
            theme::color(if selected {
                Token::SurfaceRaised
            } else {
                Token::SurfaceRaisedSubtle
            }),
        );
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        hits.push((rect, WorkspaceHit::PickSpaceRoot(index)));
    }
    let hidden = picker.match_count().saturating_sub(start + visible);
    if hidden > 0
        && let Some(rect) = rows.next(1)
    {
        let mut spans = vec![
            picker_lead(),
            Span::styled(format!("+{hidden} more"), theme::fg(Token::TextFaint)),
        ];
        row::pad_to(
            &mut spans,
            rect.width,
            theme::color(Token::SurfaceRaisedSubtle),
        );
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
}

/// `text` split after the head `needle` names, when it names one at all.
pub(super) fn split_at_head(text: &str, needle: &str) -> (String, String) {
    if needle.is_empty() || !text.to_lowercase().starts_with(needle) {
        return (String::new(), text.to_owned());
    }
    let cut = text
        .char_indices()
        .nth(needle.chars().count())
        .map_or(text.len(), |(index, _)| index);
    (text[..cut].to_owned(), text[cut..].to_owned())
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
pub(super) fn render_query_row(
    frame: &mut ratatui::Frame<'_>,
    picker: &RootPicker,
    rows: &mut Rows,
) {
    let Some(rect) = rows.next(1) else { return };
    let needle = picker.input();
    let mut spans = vec![
        picker_lead(),
        Span::styled(
            format!("{needle}{}", theme::glyph(Symbol::CursorText)),
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        ),
    ];
    // Where the typing starts from, and only until there is typing: the
    // line says everything else itself (see `RootPicker`). A path pinned
    // to the right of the line all the way through said where the prompt
    // was in a second place, which is the half that went stale the
    // moment the two could disagree.
    if needle.is_empty() {
        let used: u16 = spans.iter().map(|span| span.width() as u16).sum();
        let room = rect.width.saturating_sub(used + TRAILING_PAD + 1);
        spans.push(Span::styled(
            text::elide_head(
                &crate::ui::display_project_path(picker.base()),
                room as usize,
            ),
            theme::fg(Token::TextDim),
        ));
    }
    row::pad_to(&mut spans, rect.width, theme::color(Token::SurfaceRaised));
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    // The rail the selected space wears, so the row being typed into
    // reads as the space it is about to become.
    frame.render_widget(
        Paragraph::new(theme::glyph(Symbol::BarThin)).style(
            Style::default()
                .fg(theme::color(Token::Accent))
                .bg(theme::color(Token::SurfaceRaised)),
        ),
        Rect::new(rect.x, rect.y, 1, 1),
    );
}

pub(super) fn chip_state(model: &WorkspaceModel, hit: Option<WorkspaceHit>) -> ChipState {
    let Some(hit) = hit else {
        return ChipState::Static;
    };
    if model.pressed_hit() == Some(hit) {
        ChipState::Pressed
    } else if model.hovered == Some(hit) {
        ChipState::Hovered
    } else {
        ChipState::Resting
    }
}

/// The small notification for a task: what the delivery is about, and
/// where it stands.
///
/// Two parts and no forge's word for either. The subject is the request's
/// own number, or the ending for a completion that is not a request at
/// all (see [`delivery_subject`]); the standing is [`task_mark`]'s own
/// mark, so the strip, the sidebar row and the status legend cannot come
/// to say different things about one state.
///
/// One verb over three completions read the same whether it was about to
/// fast-forward the target under you, open a request against it, or touch
/// nothing outside the branch — which is why the subject is the half that
/// is always there, and the count rides the mark rather than standing
/// alone in front of it.
pub(super) fn delivery_notification(
    task: &AgentView,
    state: &WorkStateView,
    tick: usize,
) -> Option<(String, Color, bool)> {
    // Which states put a notification in this zone: the ones handing work
    // over passes through. `Uncommitted` and `Parked` carry marks of
    // their own in the sidebar, but they are facts about a checkout
    // rather than steps of a delivery, and this zone is the second.
    // Conditioned, not disabled: a state that cannot be delivered is
    // still reported, and the report simply is not a button.
    let pressable = match state {
        WorkStateView::Ready | WorkStateView::Published | WorkStateView::GateFailed => true,
        WorkStateView::Conflicted { .. }
        | WorkStateView::Integrating
        | WorkStateView::Integrated => false,
        _ => return None,
    };
    let (mark, hue) = task_mark(state)?;
    let standing = match state {
        // The one state `task_mark` cannot lend its glyph to. Its mark is
        // the vocabulary's "there is work to hand over", and the patched
        // set draws that as a create-a-request icon — true for the
        // completion that opens one, a lie in front of a button about to
        // fast-forward the target or to touch nothing outside the branch.
        // What a press sends is commits, whatever the ending, so the
        // count wears the arrow that means exactly that and nothing about
        // a forge.
        //
        // The count is what a press would send, which is not how far the
        // branch is from the target: that distance is the merge's
        // question and stays open until the request lands.
        WorkStateView::Ready => format!(
            "{}{}",
            theme::glyph(Symbol::SyncAhead),
            task.unsynced.unwrap_or(task.ahead)
        ),
        // The one report in this row that is also work in progress, so it
        // is the one that moves: a rebase, a gate and a push take as long
        // as the project's checks do, and a still mark for that many
        // seconds reads as a screen that has stopped. The sidebar's mark
        // keeps `Symbol::Ellipsis` — a single cell has no room to turn.
        WorkStateView::Integrating => agent_activity_frame(tick),
        _ => mark,
    };
    Some((
        format!("{} {standing}", delivery_subject(task)),
        hue,
        pressable,
    ))
}

/// What a delivery is *about*, in the fewest characters that name it: the
/// request's own number once the forge has one, the forge's word for a
/// request before that, and — for the two completions that are not a
/// request at all — where the work is going instead.
///
/// A number needs no word in front of it: `#41` is already unambiguous,
/// and "PR" there would be length spent on nothing. The word earns its
/// place only in the one case that used to leave a bare `#` standing
/// alone — a request that does not exist yet — and only where `origin`
/// said which forge this is. Picking one of the two names blind would
/// take a side the reader may not be on, so an unrecognized remote keeps
/// the `#` both forges write. The branch's published name is not here
/// either: it is the one part of this that has no bound on its length,
/// and a zone that changes width with a branch name moves every button
/// left of it.
pub(super) fn delivery_subject(task: &AgentView) -> String {
    match task.completion {
        CompletionBehavior::Pr => match task.published_request {
            Some(request) => format!("#{request}"),
            None => task.forge.request_abbreviation().unwrap_or("#").to_owned(),
        },
        CompletionBehavior::Merge => {
            format!("{} {}", theme::glyph(Symbol::ArrowTo), task.target)
        }
        CompletionBehavior::Handoff => "hand off".to_owned(),
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
    let rows = action_index_rows(&index.scopes, &index.filter, disabled);
    let reachable = action_index_rows(&index.scopes, "", disabled).len();
    let entries = action_index::render(
        frame,
        area,
        &rows,
        reachable,
        &index.filter,
        index.selected,
        WorkspaceHit::ActionIndexEntry,
    );
    // Prepended: what is underneath must not answer a click meant here.
    hits.splice(0..0, entries);
}
