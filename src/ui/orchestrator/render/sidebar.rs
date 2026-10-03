//! The sidebar: spaces, their agents, the spec summary, first steps and the timeline.

use super::*;

/// Whether `cwd` is outside any slot: no repository, no commit to branch
/// from — or an agent that simply has not been isolated, standing in
/// the operator's own directory. An agent there owes its
/// upstream a pull or a push that an agent in a slot never does, which is
/// the one thing its caption says beyond the harness.
pub(super) fn is_unisolated(cwd: &Path) -> bool {
    !uze_application::is_isolated_checkout(cwd)
}

/// The hue an agent's caption line is drawn in: dim, like every other
/// detail, except under the agent actually receiving keystrokes — whose
/// whole item, both rows of it, is what every command in the footer would
/// act on. Saying so on the caption too spares the operator tracing the
/// bold label back down a row.
pub(super) fn caption_color(is_current: bool) -> Color {
    if is_current {
        theme::color(Token::StateWarning)
    } else {
        theme::color(Token::TextDim)
    }
}

/// Pins a task's mark to an agent row's right edge — off the divider by
/// the same pad every row's trailing column keeps, so the
/// sidebar's right-hand column is one column and a mark keeps its place
/// however long the label is — and makes that cell a click target opening
/// the status catalog: a glyph nobody can look up is a glyph that reads as
/// decoration.
pub(super) fn push_trailing_mark(
    spans: &mut Vec<Span<'_>>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    label_rect: Rect,
    mark: &str,
    hue: Color,
) {
    let mark = Span::styled(mark.to_owned(), Style::default().fg(hue));
    let mark_width = mark.width() as u16;
    let used =
        spans.iter().map(|span| span.width() as u16).sum::<u16>() + mark_width + TRAILING_PAD;
    spans.push(Span::raw(
        " ".repeat(label_rect.width.saturating_sub(used).max(1) as usize),
    ));
    spans.push(mark);
    spans.push(Span::raw(" ".repeat(TRAILING_PAD as usize)));
    if let Some(mark_x) = label_rect.right().checked_sub(TRAILING_PAD + mark_width) {
        let cell = Rect::new(mark_x, label_rect.y, mark_width, 1);
        hits.push((cell, WorkspaceHit::OpenStatusCatalog(cell)));
    }
}

/// One block per space the user has created (blank-line separated — see
/// the loop below), each expanded (no collapse/accordion) into the agent
/// tabs [`agent_identity_for_tab`] recognizes as running inside it, laid
/// out alike for either kind: a tree whose items carry their status, their
/// task's mark and a caption naming the branch or the directory. A
/// space with no agent tabs shows its current `cwd` alone in place of the
/// tree, so an empty space still reads as "somewhere", not blank. Plain shell
/// tabs (and anything else not recognized as an agent) never appear here;
/// they still exist in the tab strip above the pane (see
/// [`render_tab_strip`]), scoped to whichever space is selected. The
/// underlying workspace/directory this client is attached to (see
/// `Workspace` in `uze-terminal`) is deliberately never shown — it's
/// infrastructure the user never organizes by; spaces are the only unit
/// that matters here.
pub(in crate::ui::orchestrator) fn render_sidebar(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    metrics: &mut FrameMetrics,
) {
    // No padding at all. Top: the header must land on the exact row the tab
    // strip's own content does (that block has none either), or the two
    // panes' dividers drift out of alignment by one row. Right: the rows
    // keep their own pad off the divider. Left: every row of the column
    // already leads with a column of its own — a space's gutter, a listing's
    // lead — and an inset under those read as a margin the column could not
    // afford.
    let inner = Rule::draggable(Edge::Right, model.dragging_sidebar).render(frame, area);

    let mut rows = Rows::over(inner);

    // The header names the surface and ends in the control that opens the
    // other one — the management modal. No top padding: it lands on the
    // exact row the tab strip's own content does.
    if let Some(rect) = rows.next(1) {
        // One column in, the pad its controls keep at the other end: the
        // header is the column's own chrome, and a name flush against the
        // edge under a right-hand pad reads as a row that slipped.
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{}work", " ".repeat(TRAILING_PAD as usize)),
                theme::fg_bold(Token::TextMuted),
            )),
            rect,
        );
        let more = theme::glyph(Symbol::Manage);
        let more_width = theme::width(Symbol::Manage);
        // One column off the divider, the same pad every row of this column
        // keeps at its right edge.
        let more_rect = Rect::new(
            rect.right().saturating_sub(more_width + TRAILING_PAD),
            rect.y,
            more_width,
            1,
        );
        // The way to grow the column sits beside the way out of it, with a
        // rule between them: one names an action this surface takes, the
        // other opens another surface, and side by side without it they read
        // as one pair of controls.
        // Muted while the prompt it opens is open: the word is where that
        // prompt came from, and in the accent beside it, it reads as a
        // second way in rather than as the one already taken. Otherwise the
        // accent is held back until the pointer asks for it, so the header's
        // one coloured word does not outshout the column under it.
        let new = "+ space";
        let new_hue = if model.root_picker.is_some() {
            Token::TextMuted
        } else {
            match chip_state(model, Some(WorkspaceHit::NewSpace)) {
                ChipState::Hovered | ChipState::Pressed => Token::Accent,
                ChipState::Resting | ChipState::Static => Token::AccentMuted,
            }
        };
        let new_width = Span::raw(new).width() as u16;
        let divider = theme::glyph(Symbol::TreeColumnDivider);
        let divider_width = theme::width(Symbol::TreeColumnDivider);
        let new_rect = Rect::new(
            more_rect
                .x
                .saturating_sub(new_width + divider_width + 2 * HEADER_GAP),
            rect.y,
            new_width,
            1,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(new, theme::fg_bold(new_hue))),
            new_rect,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(divider, theme::fg(Token::SurfaceHover))),
            Rect::new(new_rect.right() + HEADER_GAP, rect.y, divider_width, 1),
        );
        hits.push((new_rect, WorkspaceHit::NewSpace));
        // Never a button: only the mark's own colour answers the pointer
        // — muted at rest, beside a label of the same weight, brighter
        // under the hover and brightest while pressed.
        let more_hue = match chip_state(model, Some(WorkspaceHit::OpenManage)) {
            ChipState::Pressed => Token::TextBright,
            ChipState::Hovered => Token::TextPrimary,
            ChipState::Resting | ChipState::Static => Token::TextMuted,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(more, theme::fg_bold(more_hue))),
            more_rect,
        );
        hits.push((more_rect, WorkspaceHit::OpenManage));
    }
    if let Some(error) = &model.error
        && let Some(rect) = rows.next(1)
    {
        frame.render_widget(
            Paragraph::new(Span::styled(
                error.clone(),
                Style::default()
                    .fg(theme::color(Token::StateDanger))
                    .add_modifier(Modifier::BOLD),
            )),
            rect,
        );
    }

    // The hairline under the header lands on the row the tab strip's own
    // bottom border does, so the two columns share one divider line.
    if let Some(rect) = rows.next(1) {
        frame.render_widget(
            Paragraph::new(Span::styled(
                // The full width, up to the divider between the two
                // columns: it is a rule closing the header, not a row of
                // content keeping the column's trailing pad.
                theme::glyph(Symbol::TreeDivider).repeat(rect.width as usize),
                theme::fg(Token::BorderFaint),
            )),
            rect,
        );
    }

    let Some(session) = &model.session else {
        return;
    };

    // A blank row above each space, the first one included: each card
    // gets an edge on both sides, and it is also where a space being
    // dragged is shown landing (see `draw_space_drop`).
    //
    // The fill alone was tried and is not enough. Every card but the one
    // in front is faded, so where two faded cards meet there is no edge
    // to find — the column reads as one surface with headers in it, and
    // the card in front reads as the top or the bottom of whichever
    // neighbour it touches rather than as its own thing. The row over the
    // first card is the same argument against the rule above it.
    //
    // Except between two minimized spaces, the one in front included: a
    // row of nothing between two headers is half the column spent on
    // spaces nobody is looking into, and the fill of the one in front is
    // edge enough on its own. They pack, and the rows open again around
    // whichever one is expanded.
    let rearranging = model.dragging_space.is_some_and(|dragging| dragging.armed);
    let mut gap_above = rows.slot(1).visible();
    // While the root picker is open it owns the column: the listing it
    // draws is a tree of directories, and side by side with the tree of
    // spaces neither would read as the one being chosen from. It stands
    // where the spaces it would join stand. Closing it brings them back.
    if let Some(picker) = &model.root_picker {
        render_root_picker(frame, picker, &mut rows, hits);
        return;
    }

    // The timeline keeps the foot of the column whatever the spaces above
    // come to: a section trailing the last space would sink out of sight
    // under a long tree, and a history that is only there while the tree
    // is short is no place to go looking for one. Its rows are reserved
    // before the spaces are laid out, and handed back to them below.
    let timeline = model
        .remembered
        .git_badge
        .as_ref()
        .and_then(|badge| badge.timeline.as_ref());
    // Two sections stacked at the foot: the steps above the history, each
    // taking its rows before the tree is laid out, so neither is ever
    // drawn over the other. Any of them may be open at once; each is
    // budgeted against what the ones below it left, so together they
    // cannot eat the column the spaces are for.
    let column_bottom = rows.bottom;
    let steps = model.first_steps();
    let steps_height = steps.height();
    let reserved = timeline.map_or(0, |timeline| {
        timeline_height(
            timeline,
            model.timeline_collapsed,
            model.timeline_rows,
            rows.remaining().saturating_sub(steps_height),
        )
    });
    // The spec section sits on the timeline: what the work intends over
    // what it did. Reserved the same way, before the tree.
    let spec_summary = model.spec_summary();
    let spec_reserved = spec_summary.map_or(0, |summary| {
        spec_summary_height(
            summary,
            model.spec_summary_open,
            rows.remaining()
                .saturating_sub(steps_height)
                .saturating_sub(reserved),
        )
    });
    let foot = reserved + spec_reserved;
    let strip = steps.rect(Rect::new(
        inner.x,
        inner.y,
        inner.width,
        column_bottom.saturating_sub(inner.y).saturating_sub(foot),
    ));

    // One row of air above the foot, so a tree that grows to meet it still
    // reads as a tree over two sections rather than as one list.
    rows.bottom = strip.map_or(column_bottom - foot, |rect| rect.y.saturating_sub(1));
    // The release notice sits on whatever holds the foot — the steps, or
    // the history once the steps are put away — rather than under them: it
    // is news, and news below two sections reads as the column's floor.
    if let Some(notice) = model.release.as_ref().map(crate::ui::ReleaseNotice)
        && let Some(rect) = notice.rect(Rect::new(
            inner.x,
            inner.y,
            inner.width,
            rows.bottom.saturating_sub(inner.y),
        ))
    {
        let targets = notice.render(frame, rect);
        hits.push((targets.dismiss, WorkspaceHit::DismissRelease));
        hits.extend(
            targets
                .notes
                .into_iter()
                .map(|rect| (rect, WorkspaceHit::OpenReleaseNotes)),
        );
        rows.bottom = rect.y;
    }

    // What the column cannot show is scrolled to, not lost: the tree grows
    // with the work, and a space that fell off the foot of it — under a
    // long tree above, or under the timeline holding the foot — used to be
    // unreachable rather than merely out of view. The bound is measured
    // here, where the tree's own window is known, and handed back for the
    // wheel to stay inside (see `scroll_tree`).
    let overflow =
        tree_rows(model, session, identities, rearranging).saturating_sub(rows.remaining());
    metrics.tree_overflow = overflow;
    rows.scroll_past(model.remembered.tree_scroll.min(overflow));

    let dropping = model
        .dragging_space
        .filter(|dragging| dragging.armed)
        .and_then(|dragging| dragging.pending);
    for (index, space) in session.workspace.spaces.iter().enumerate() {
        let is_active_space = space.id == session.workspace.selected_space;
        // The row under this space is also the row over the next one. It
        // is wanted beside a space that is open — and always while a
        // space is being carried, because it is the one place a drop can
        // be drawn.
        let margin = rearranging
            || !model.space_folded(space)
            || session.workspace.spaces[index + 1..]
                .first()
                .is_none_or(|next| !model.space_folded(next));
        if dropping == Some(PendingDrop::Before(space.id)) {
            draw_space_drop(frame, gap_above);
        }
        let header = rows.slot(1);
        if header.is_full() {
            break;
        }
        if let Some(header_rect) = header.visible() {
            render_space_header(frame, header_rect, session, space, model, identities, hits);
        }
        // Where a space's work is, under its name. Minimized, only while
        // it is the one in front: the name is what a person reads down a
        // column of spaces, and a path under every one of them is a
        // second column of text to read past to reach the first. Asked
        // for by selecting it, which is when the answer is worth a row.
        //
        // Open with nothing in it, always — there it is not detail under
        // a name, it is the whole of what the block has to show, and
        // without it the space is a header over nothing.
        let folded = model.space_folded(space);
        let empty = agent_tabs_of(space, identities).is_empty();
        if (folded && is_active_space) || (!folded && empty) {
            render_space_caption(frame, &mut rows, hits, model, session, space, identities);
        }
        if folded {
            gap_above = margin.then(|| rows.slot(1).visible()).flatten();
            continue;
        }

        // Every fact a row draws, resolved once per agent, so drawing only
        // decides where each goes.
        let agent_tabs = agents_in_drawing_order(model, space, identities);
        let agents: Vec<SidebarAgent<'_>> = agent_tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                SidebarAgent::resolve(
                    model,
                    identities,
                    space,
                    tab,
                    index + 1 == agent_tabs.len(),
                    is_active_space,
                )
            })
            .collect();

        if !agents.is_empty() {
            let captions: Vec<TreeCaption> = agents
                .iter()
                .map(|agent| TreeCaption::resolve(model, agent))
                .collect();
            draw_tree(frame, &mut rows, hits, is_active_space, &agents, &captions);
        }
        gap_above = margin.then(|| rows.slot(1).visible()).flatten();
    }
    if dropping == Some(PendingDrop::End) {
        draw_space_drop(frame, gap_above);
    }

    if let Some(rect) = strip {
        let mut section_hits = Vec::new();
        crate::ui::extension_view::render_section(
            frame,
            &steps.section(),
            &mut Rows::over(rect),
            false,
            &mut section_hits,
        );
        // The closing mark rides on the header, and an ordinary click
        // resolves against the *first* rect that contains it
        // (`WorkspaceModel::hit_rect_at`) — so the mark goes in ahead of
        // the header it sits on, or the header swallows it and the section
        // folds instead of leaving.
        if let Some(rect) = section_hits.iter().find_map(|(rect, hit)| {
            matches!(hit, ViewHit::ToggleSection)
                .then(|| steps.close_rect(*rect))
                .flatten()
        }) {
            hits.push((rect, WorkspaceHit::CloseFirstSteps));
        }
        for (rect, hit) in section_hits {
            match hit {
                ViewHit::ToggleSection => hits.push((rect, WorkspaceHit::ToggleFirstSteps)),
                ViewHit::SelectItem(index) => {
                    if let Some(action) = steps.steps.get(index) {
                        hits.push((rect, WorkspaceHit::QuickAction(*action)));
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(summary) = spec_summary
        && spec_reserved > 0
    {
        rows.scroll_past(0);
        let air = if model.spec_summary_open && spec_reserved > 1 {
            SPEC_SUMMARY_AIR
        } else {
            0
        };
        rows.bottom = column_bottom - reserved - air;
        rows.y = column_bottom - foot;
        render_spec_summary(frame, summary, model, &mut rows, hits);
    }
    if let Some(timeline) = timeline
        && reserved > 0
    {
        rows.scroll_past(0);
        rows.bottom = column_bottom;
        rows.y = column_bottom - reserved;
        metrics.marquee |= render_timeline(frame, timeline, model, &mut rows, hits);
    }
}

/// The rows the spec section takes: its header, and while it is open a row
/// per change and a row of air under the last, so the next section's
/// header starts a block of its own rather than reading as one more
/// change — all within half of what the column has left, since the spaces
/// are what the sidebar is for. Nothing when even the header would not
/// fit.
pub(in crate::ui::orchestrator) fn spec_summary_height(
    summary: &uze_extensions::spec::Summary,
    open: bool,
    remaining: u16,
) -> u16 {
    let budget = remaining / 2;
    if budget < 1 {
        return 0;
    }
    let changes = u16::try_from(summary.changes.len()).unwrap_or(u16::MAX);
    if open {
        1 + changes + SPEC_SUMMARY_AIR
    } else {
        1
    }
    .min(budget)
}

/// The blank row an open spec section keeps under its last change.
pub(super) const SPEC_SUMMARY_AIR: u16 = 1;

/// The sidebar's spec section. The extension says what it holds; this
/// supplies the fold, which is host state, and tags the hits it gets back.
pub(super) fn render_spec_summary(
    frame: &mut ratatui::Frame<'_>,
    summary: &uze_extensions::spec::Summary,
    model: &WorkspaceModel,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let section = uze_extensions::spec::summary_section(summary, !model.spec_summary_open, 0);
    let mut section_hits = Vec::new();
    let mut column = Rows::over(Rect::new(rows.x, rows.y, rows.width, rows.remaining()));
    let hovered_row = match model.hovered {
        Some(WorkspaceHit::Extension(ExtensionHit::SpecSummary(ViewHit::SelectItem(index)))) => {
            Some(index)
        }
        _ => None,
    };
    let on_show = model
        .spec
        .as_ref()
        .and_then(uze_extensions::spec::SpecView::change_on_show)
        .and_then(|name| {
            summary
                .changes
                .iter()
                .position(|change| change.name == name)
        });
    let lit_rows: Vec<usize> = hovered_row.into_iter().chain(on_show).collect();
    crate::ui::extension_view::render_section_with(
        frame,
        &section,
        &mut column,
        false,
        None,
        &lit_rows,
        &mut section_hits,
    );
    hits.extend(section_hits.into_iter().map(|(rect, hit)| {
        (
            rect,
            WorkspaceHit::Extension(ExtensionHit::SpecSummary(hit)),
        )
    }));
}

/// The rows the space tree comes to, whether or not the column can show
/// them all: a header per space, the cwd caption a space with no agent
/// shows in place of its tree — and a minimized one shows only while it
/// is the one in front — two rows per agent with the separator row
/// between the two groups, the blank row over each space except between
/// two that come to a single line, and the column's own top margin. A minimized space,
/// or an open one with no agent, has its header and the caption under it. Measured up front rather
/// than counted while drawing, because how far the tree may be scrolled
/// has to be known before its first row is laid out.
pub(super) fn tree_rows(
    model: &WorkspaceModel,
    session: &Session,
    identities: &[AgentIdentity],
    rearranging: bool,
) -> u16 {
    let spaces = &session.workspace.spaces;
    spaces
        .iter()
        .enumerate()
        .map(|(index, space)| {
            let agents = agent_tabs_of(space, identities).len() as u16;
            // The same conditions `render_sidebar` draws by: a minimized
            // space nobody is in is its header and nothing else, and two
            // minimized spaces in a row have no blank row between them.
            let body = if model.space_folded(space) {
                u16::from(space.id == session.workspace.selected_space)
            } else if agents == 0 {
                1
            } else {
                agent_rows(agents)
            };
            let margin = rearranging
                || !model.space_folded(space)
                || spaces[index + 1..]
                    .first()
                    .is_none_or(|next| !model.space_folded(next));
            1 + body + u16::from(margin)
        })
        .sum::<u16>()
        // The column's own top margin, which is a row above the first
        // space rather than one of the rows between a pair of them.
        + 1
}

/// Where a dragged space would land: an accent hairline across the blank
/// row between two spaces — a line where it goes, rather than a mark on a
/// header, whose leading column already says which space is selected.
pub(super) fn draw_space_drop(frame: &mut ratatui::Frame<'_>, gap: Option<Rect>) {
    let Some(gap) = gap else {
        return;
    };
    let width = gap.width.saturating_sub(1 + TRAILING_PAD);
    frame.render_widget(
        Paragraph::new(theme::glyph(Symbol::TreeDivider).repeat(width as usize))
            .style(theme::fg(Token::Accent)),
        Rect::new(gap.x + 1, gap.y, width, 1),
    );
}

/// The agent tabs of a space, in the order the sidebar draws them — the
/// order `step_agent` walks too.
pub(in crate::ui::orchestrator) fn agent_tabs_of<'a>(
    space: &'a Space,
    identities: &[AgentIdentity],
) -> Vec<&'a Tab> {
    space
        .tabs
        .iter()
        .filter(|tab| agent_identity_for_tab(identities, tab).is_some())
        .collect()
}

/// A space's agents in the order the column draws them: the ones working
/// in its own root, then the isolated ones, each group keeping the order
/// the operator dragged it into.
///
/// The keyboard walks this order rather than the tab order, because an
/// agent that isolates moves between the groups and a walk that skipped
/// past where the row *is* would be reading a column nobody sees.
pub(in crate::ui::orchestrator) fn agents_in_drawing_order<'a>(
    model: &WorkspaceModel,
    space: &'a Space,
    identities: &[AgentIdentity],
) -> Vec<&'a Tab> {
    let mut tabs = agent_tabs_of(space, identities);
    tabs.sort_by_key(|tab| agent_group(model, tab.id));
    tabs
}

/// Which of a space's two groups the agent on `tab` belongs to — read
/// off the record UZE wrote rather than the shape of the directory the
/// pane happens to sit in.
pub(in crate::ui::orchestrator) fn agent_group(model: &WorkspaceModel, tab: TabId) -> AgentGroup {
    // The record's own answer, not the branch: every agent is on one now,
    // and an agent in the space's root would have grouped itself with the
    // isolated ones the moment its row learnt what branch that was.
    if let Some(task) = model.tab_task(tab) {
        return if task.isolated {
            AgentGroup::Isolated
        } else {
            AgentGroup::InTheRoot
        };
    }
    // Until there is one — the first frames of an attach, where the panes
    // arrive from the runtime and the records are still being read — the
    // directory the pane stands in is the only fact there is, and it is a
    // fact rather than a guess: `.worktrees/<id>` is a layout UZE owns,
    // and a slot is never a space of its own, so a pane in one was put
    // there by a placement. Drawing every agent in the root's group until
    // a Git pass answers says something untrue about where they are, and
    // says it to an operator who just opened the client.
    model
        .tab(tab)
        .filter(|tab| uze_application::is_isolated_checkout(&tab.pane.cwd))
        .map_or(AgentGroup::InTheRoot, |_| AgentGroup::Isolated)
}

/// The two groups a space's column is drawn in, in the order it draws
/// them: the agents sharing the space's own root, then the ones working
/// in a checkout of their own.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ui::orchestrator) enum AgentGroup {
    InTheRoot,
    Isolated,
}

impl AgentGroup {
    pub(in crate::ui::orchestrator) fn is_isolated(self) -> bool {
        self == Self::Isolated
    }
}

/// The rows a space's agents take under its caption — the header, the
/// caption and the blank row after the space are the caller's. Every kind draws the same
/// two-row item, so one measure serves them all, and the scroll bound,
/// taken before the first row is drawn, can never disagree with the rows
/// that follow.
pub(super) fn agent_rows(agents: u16) -> u16 {
    (agents * 3).saturating_sub(1)
}

/// One agent of a space, resolved once: what its two rows say and
/// which of its states are on.
pub(super) struct SidebarAgent<'a> {
    pub(super) tab: &'a Tab,
    /// Whether this agent works in a checkout of its own. What decides
    /// the group its row sits in, the hue it wears, and whether the tree
    /// branches for it.
    pub(super) isolated: bool,
    /// Selected *and* in the active space: the one agent receiving
    /// keystrokes.
    pub(super) is_current: bool,
    pub(super) status: AgentTabStatus,
    pub(super) renaming: Option<&'a RenameBuffer>,
    pub(super) drop_target: bool,
    pub(super) harness: Option<&'a str>,
    pub(super) tick: usize,
}

/// What only the tree's two-row item says about an agent: its task's mark
/// and the caption row beneath the label.
pub(super) struct TreeCaption {
    pub(super) task_mark: Option<(String, Color)>,
    /// A checkout removed from under the agent whose task the preserved
    /// list still holds — the row offers to resume it.
    pub(super) resumable: bool,
    /// The harness running the agent, or the words for a checkout that
    /// is gone.
    pub(super) detail: String,
    pub(super) detail_color: Color,
}

impl TreeCaption {
    pub(super) fn resolve(model: &WorkspaceModel, agent: &SidebarAgent<'_>) -> Self {
        let tab = agent.tab;
        let task = model.tab_task(tab.id);
        let task_mark = task.and_then(|task| task_mark(&model.drawn_state(task)));
        // A checkout removed from under the agent is said in words, not as
        // the kernel's `(deleted)` path: the process cannot work there any
        // more, and the task it was running is what the preserved list now
        // holds.
        let lost = model.remembered.lost_checkouts.contains(&tab.pane.id);
        let resumable = lost && model.lost_task(tab.id).is_some();
        // What runs here, named by its id (`claude`, `codex`) — the same
        // word the picker launches and the process reports.
        //
        // Not the branch, which this row used to carry: a task's label is
        // *derived* from its branch (`worktree::label_of`), so the caption
        // repeated the name above it minus the type — and a task nobody
        // named reads `agent/<id>` under a label that is the identifier
        // itself. For an agent in the root it was worse: they all share
        // the operator's branch, so every caption said the same thing.
        // The branch belongs to the space, and the space's header and the
        // timeline are where it is said once.
        //
        // The harness is the one fact that tells two agents of a space
        // apart, and it cost a click to see. A row is only drawn for a
        // recognized harness (`agent_tabs_of`), so the pane's own process
        // is a fallback for a race, not a second answer.
        let detail = if lost {
            "checkout removed".to_owned()
        } else {
            agent
                .harness
                .unwrap_or(tab.pane.process.as_str())
                .to_owned()
        };
        let detail_color = if lost {
            theme::color(Token::StateWarning)
        } else {
            caption_color(agent.is_current)
        };
        Self {
            task_mark,
            resumable,
            detail,
            detail_color,
        }
    }
}

impl<'a> SidebarAgent<'a> {
    pub(super) fn resolve(
        model: &'a WorkspaceModel,
        identities: &'a [AgentIdentity],
        space: &Space,
        tab: &'a Tab,
        is_last: bool,
        is_active_space: bool,
    ) -> Self {
        // The agent the space is about, not its `selected_tab`: a shell
        // opened beside an agent is part of that agent's own context, and
        // switching into it must not unselect the agent in this tree (see
        // `space_context_agent`). Every space names a context agent,
        // including the ones the user is not in — so `selected` alone put
        // a `●` on one agent per open space, each claiming to be the one
        // receiving keystrokes. Only the active space's selection is that
        // agent.
        let selected = Some(tab.id) == space_context_agent(space, identities);
        let is_current = is_active_space && selected;
        let renaming = model
            .renaming
            .as_ref()
            .filter(|(target, _)| *target == RenameTarget::Tab(tab.id))
            .map(|(_, buffer)| buffer);
        // A tab-reorder drag in this exact space, resolved to drop right
        // before (or, on the last row, at the end after) this one.
        let drop_target = model.dragging_tab.is_some_and(|dragging| {
            dragging.is_pending_drop_row(
                TabDragGroup::Agents(space.id, agent_group(model, tab.id)),
                tab.id,
                is_last,
            )
        });
        Self {
            tab,
            isolated: agent_group(model, tab.id).is_isolated(),
            is_current,
            status: model.agent_tab_status(tab.pane.id, is_current),
            renaming,
            drop_target,
            harness: agent_identity_for_tab(identities, tab),
            tick: model.tick,
        }
    }

    /// The row's label: the rename buffer being typed, or the tab's label
    /// elided to `room`.
    pub(super) fn label(&self, room: u16) -> Vec<Span<'static>> {
        match self.renaming {
            Some(buffer) => rename_spans(buffer),
            None => {
                // One item in the whole column is bright and bold, and it
                // is whatever is receiving keystrokes — here, or the
                // header of the space in front (see
                // `render_space_header`, which wears the same style under
                // the same predicate).
                //
                // The hue used to follow the space's own context agent
                // while the weight followed `is_current`, and every space
                // names a context agent — the ones nobody is in
                // included. Walking from space A to space B left A's
                // agent in the bright hue and merely unbolded, so two
                // rows claimed to be where the keyboard was.
                //
                // What that hue was worth saying — which agent a space
                // would land on — is now said only once the space is
                // entered, by the same row going bright. A second, dimmer
                // register for it in the spaces nobody is in would be a
                // third level of emphasis in a column that is asking for
                // fewer.
                vec![Span::styled(
                    text::elide(&self.tab.label, room as usize),
                    label_style(self.is_current, false),
                )]
            }
        }
    }
}

/// A label being renamed: bright and bold, since it is where keys land,
/// with the caret where the next one will.
pub(super) fn rename_spans(buffer: &RenameBuffer) -> Vec<Span<'static>> {
    let style = Style::default()
        .fg(theme::color(Token::TextBright))
        .add_modifier(Modifier::BOLD);
    let (before, after) = buffer.split();
    vec![
        Span::styled(before.to_owned(), style),
        Span::styled(theme::glyph(Symbol::CursorText), style),
        Span::styled(after.to_owned(), style),
    ]
}

/// The row under a minimized or empty space's header: the directory the
/// space's work is in. Lit along with its header, and selecting the space
/// like any caption.
///
/// Minimized, it is drawn only for the space in front. A name is what a
/// person reads down a column of spaces, and the path under every one of
/// them was a second column of text to read past to reach the first —
/// tiring exactly when the column is long, which is when the names
/// matter most. Selecting a space is how it is asked for, and the row it
/// costs is a row the space in front can afford.
///
/// The directory rather than the branch its root is on. A space is a
/// place, and the fold is what asks about it: opened, its agents answer
/// for themselves and say their own branches; minimized, they are gone
/// from the column and the one thing left to say is *where* — which a
/// branch name does not say, and which was the question the header's own
/// `⇄` existed to answer, a control per space for a fact the row could
/// simply carry.
pub(super) fn render_space_caption(
    frame: &mut ratatui::Frame<'_>,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    model: &WorkspaceModel,
    session: &Session,
    space: &Space,
    identities: &[AgentIdentity],
) {
    let Some(rect) = rows.slot(1).visible() else {
        return;
    };
    let selected = space.id == session.workspace.selected_space;
    let caption = crate::ui::display_project_path(&space_cwd(space, identities));
    // Pinned to the right edge, the one trailing column every row in the
    // sidebar keeps: the column the header's own name leads stays the
    // space's, and what it is about reads as a caption to it rather than
    // as another row of the tree. Lit along with its header.
    let is_current = header_is_current(space, session, identities, model.space_folded(space));
    let mut spans = vec![space_gutter(selected, is_current)];
    let hue = theme::color(Token::TextDim);
    row::push_trailing(&mut spans, rect.width, caption, hue);
    // A minimized space is its header and this row, and the two are one
    // item: the trace the header wears when it is the row in front runs
    // through this row too, or the item would be lit down half its height.
    let ground = if is_current {
        theme::tinted(Token::Accent, Token::SurfaceRaisedSubtle)
    } else {
        block_ground(selected || !model.space_folded(space))
    };
    row::pad_to(&mut spans, rect.width, ground);
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    hits.push((rect, WorkspaceHit::SelectSpace(space.id)));
}

/// The ground under a space's rows: the panel itself where a space is
/// `lifted`, and the same panel faded where it is not. Every space is a
/// block either way — what the fill says is which blocks have something
/// standing on them.
///
/// A space is lifted when it is the one in front *or* when it is open,
/// which is not the same rule as "selected" and used not to be one at
/// all. The fade is a third of a surface that is itself seven percent
/// over the background, so what reaches the screen is about two — enough
/// to find the edge of a minimized card, which is two rows reading as one
/// item, and not enough to be a ground. An open space has agent rows
/// standing on it, and on that fill they stood on the panel instead: the
/// block had a header and then nothing under it, so its rows read as
/// loose in the column rather than as its contents. Which one is in front
/// is the gutter's answer now (see [`space_gutter`]), not the fill's.
pub(super) fn block_ground(lifted: bool) -> Color {
    if lifted {
        theme::color(Token::SurfaceRaisedSubtle)
    } else {
        theme::faded(Token::SurfaceRaisedSubtle)
    }
}

/// Each agent a two-row item — status and name over the harness running
/// it — one item straight after the next, for either kind, all of it
/// beside the space's gutter (see [`space_gutter`]). Selection in the
/// tree is the item's own fill, a trace of the space's hue over the
/// panel the space sits on, plus the status glyph; the drop indicator
/// during a drag is an accent bar down the item's leading column, on
/// both of its rows so it reads as the whole item.
///
/// An agent in the space's own root stands where its space stands — it
/// works there, so there is no level to draw. The isolated ones hang off
/// a trunk of their own, one column in, because their work does: the
/// group that is somewhere else gets the one indent in the column, and
/// the line down it says those rows are one collection. A group of one
/// is no collection, so it stands where the root's agents stand.
///
/// That trunk is the group's, not the space's, which is why it reads
/// where the old `├─` did not: the space's gutter is a bar flush against
/// its cell and a box-drawing corner cannot meet one, so the connector
/// pointed at a line it could not touch. Given a column of its own there
/// is nothing to meet — it is its own line, and it starts and ends with
/// the group.
///
/// The agent receiving keystrokes is the one stretch of the *space's*
/// gutter in the accent; the trunk stays quiet either way, because it
/// answers which group, not which row.
///
/// Only an open space with agents in it reaches here, so every row this
/// draws stands on a lifted ground whether or not its space is the one
/// in front (see [`block_ground`]).
pub(super) fn draw_tree(
    frame: &mut ratatui::Frame<'_>,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    is_active_space: bool,
    agents: &[SidebarAgent<'_>],
    captions: &[TreeCaption],
) {
    // Two groups: the agents working in the space's own root, then the
    // isolated ones. A blank row between them, and only when both have
    // somebody in them — a separator between collections, never between
    // siblings. Isolating an agent moves its row from the first group to
    // the second, which is how the operator sees the action happened.
    // Which of the isolated ones opens the group's trunk and which
    // closes it — the two rows whose glyph is not the one in between.
    let first_isolated = agents.iter().position(|agent| agent.isolated);
    let last_isolated = agents.iter().rposition(|agent| agent.isolated);
    /// Every space `draw_tree` is called for is open.
    const OPEN: bool = true;
    let mut previous: Option<bool> = None;
    for (index, (agent, caption)) in agents.iter().zip(captions).enumerate() {
        let tab = agent.tab;
        let isolated = agent.isolated;
        let branch = Branch::of(
            isolated,
            Some(index) == first_isolated,
            Some(index) == last_isolated,
        );
        if previous.is_some_and(|was| was != isolated)
            && let Some(gap) = rows.slot(1).visible()
        {
            let mut spans = vec![space_gutter(is_active_space, false)];
            row::pad_to(&mut spans, gap.width, block_ground(OPEN));
            frame.render_widget(Paragraph::new(Line::from(spans)), gap);
        }
        previous = Some(isolated);
        // The status glyph and the name land one column inside the
        // header's fold and name, so the column reads space > agent at the
        // cost of one column. The agent receiving keystrokes lights its
        // stretch of the gutter, and so does the row a dragged agent would
        // land before: the same line, in the accent, never a heavier one.
        let lit = agent.is_current || agent.drop_target;
        // The row the keyboard is on carries a trace of its own group's
        // hue over the panel every other row sits on — the item is two
        // rows and the status glyph is one cell, so the block is what
        // reads as "here" at a glance. Nothing outside the active space
        // is tinted: `is_current` is selected *and* receiving keystrokes.
        let surface = if agent.is_current {
            Some(theme::tinted(Token::Accent, Token::SurfaceRaisedSubtle))
        } else {
            Some(block_ground(OPEN))
        };
        // The status glyph in the header's own fold column for an agent
        // in the root, and one further in for one working in a checkout
        // of its own — that column carrying the group's own trunk.
        let lead = || [space_gutter(is_active_space, lit), branch.corner()];
        let label_slot = rows.slot(1);
        if label_slot.is_full() {
            break;
        }
        if let Some(label_rect) = label_slot.visible() {
            let [gutter_span, indent_span] = lead();
            let indicator_span = Span::styled(
                agent.status.glyph(agent.tick),
                Style::default().fg(agent.status.color()),
            );
            // Elided rather than run under the mark pinned to the right
            // edge, the way the branch beneath it is.
            let taken = (gutter_span.width() + indent_span.width() + indicator_span.width()) as u16
                + caption
                    .task_mark
                    .as_ref()
                    .map_or(0, |(mark, _)| 1 + Span::raw(mark.as_str()).width() as u16)
                + TRAILING_PAD;
            let room = label_rect.width.saturating_sub(taken).max(1);
            let label = agent.label(room);
            // The task mark behind the label is the one click target that
            // opens the catalog (see `push_trailing_mark`): the status glyph
            // in front of the name is not, so the row's leading column stays
            // a plain part of selecting the tab. Pushed before the row's
            // own `SelectTab` hit below, since the click search takes the
            // first rect it lands in — a 1-column target inside a row-wide
            // one only ever wins by being found first.
            let mut spans = vec![gutter_span, indent_span, indicator_span];
            spans.extend(label);
            if let Some((mark, hue)) = &caption.task_mark {
                push_trailing_mark(&mut spans, hits, label_rect, mark, *hue);
            }
            if let Some(surface) = surface {
                row::pad_to(&mut spans, label_rect.width, surface);
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), label_rect);
            hits.push((label_rect, WorkspaceHit::SelectTab(tab.id)));
        }

        if let Some(detail_rect) = rows.slot(1).visible() {
            // Under the agent's name, past the indent and the status
            // column, in either kind.
            let mut spans = vec![
                space_gutter(is_active_space, lit),
                branch.stem(),
                Span::raw("  "),
            ];
            // Right-aligned under the task mark, with the same trailing
            // pad off the divider. The way back in, on the row itself:
            // "resume" puts the task this pane was running into a slot of
            // its own, via the same picker a new agent goes through.
            // Offered only while the task is waiting for one (see
            // `lost_task`).
            //
            // The only thing this edge carries. What a pull and a push
            // would move used to stand here too, and it is not the
            // agent's: it is read from the checkout, so every agent
            // sharing one — four in a space's own root is an ordinary
            // day — printed the same two numbers under its own name. It
            // is said once now, on the header of the space whose checkout
            // it is about (see `push_trailing_sync`), which is where the
            // branch went for the same reason.
            const RESUME: &str = "resume";
            let trailing: Vec<Span<'_>> = if caption.resumable {
                vec![Span::styled(RESUME, theme::fg(Token::Accent))]
            } else {
                Vec::new()
            };
            // The branch is elided, never cut: a name longer than the column
            // used to run under the caption and off the right edge, so
            // the one thing the row was pinning there — "resume" — was
            // what disappeared.
            {
                let taken: u16 = spans
                    .iter()
                    .chain(&trailing)
                    .map(|span| span.width() as u16)
                    .sum::<u16>()
                    + TRAILING_PAD;
                let room = detail_rect.width.saturating_sub(taken).max(1);
                spans.push(Span::styled(
                    text::elide(&caption.detail, room as usize),
                    Style::default().fg(caption.detail_color),
                ));
            }
            if !trailing.is_empty() {
                let x = detail_rect
                    .right()
                    .saturating_sub(TRAILING_PAD + RESUME.len() as u16);
                hits.push((
                    Rect::new(x, detail_rect.y, RESUME.len() as u16, 1),
                    WorkspaceHit::ResumeLostCheckout(tab.id),
                ));
                let used: u16 = spans
                    .iter()
                    .chain(&trailing)
                    .map(|span| span.width() as u16)
                    .sum::<u16>()
                    + TRAILING_PAD;
                let gap = detail_rect.width.saturating_sub(used).max(1);
                spans.push(Span::raw(" ".repeat(gap as usize)));
                spans.extend(trailing);
                spans.push(Span::raw(" ".repeat(TRAILING_PAD as usize)));
            }
            if let Some(surface) = surface {
                row::pad_to(&mut spans, detail_rect.width, surface);
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), detail_rect);
            // The label and its dim branch/cwd caption read as one tree
            // item — clicking the caption line must select the tab too, not
            // just the label text above it.
            hits.push((detail_rect, WorkspaceHit::SelectTab(tab.id)));
        }
    }
}

/// What is worth trying once on this side of the product: putting an agent
/// to work, moving between them, seeing what a change actually did, finding
/// the work no live tab is in front of, and the surface that lists the
/// rest. Each is a gesture nobody discovers by staring at a screen, and
/// none of them destroys anything, so a list that invites them costs the
/// reader nothing.
pub(in crate::ui::orchestrator) const FIRST_STEPS: [Action; 6] = [
    Action::NewAgent,
    Action::NextAgent,
    Action::ToggleChanges,
    Action::ToggleFiles,
    Action::ToggleWork,
    Action::OpenActionIndex,
];

/// Named from the mode rather than from what is open: the key beside a
/// step must not change because an overlay is up.
pub(in crate::ui::orchestrator) const FIRST_STEP_SCOPES: &[uze_keys::Scope] =
    &[uze_keys::Scope::Global, uze_keys::Scope::Workspace];

/// The rows the tree above the timeline keeps whatever the section is
/// dragged to — a space header, an agent and its caption, and the blank
/// row after them.
pub(super) const MIN_TREE_ROWS: u16 = 4;

/// The rows the timeline section takes at the foot of the column: its
/// header, and while it is open the divider under it and one row per
/// commit. Left alone, that is within half of what the column has left,
/// since the spaces are what the sidebar is for; dragged (`rows_wanted`),
/// it is what was asked for, within the history there is and what the
/// column can spare past the tree's minimum. Nothing when even the header
/// would not fit.
pub(in crate::ui::orchestrator) fn timeline_height(
    timeline: &uze_extensions::code::Timeline,
    collapsed: bool,
    rows_wanted: Option<u16>,
    remaining: u16,
) -> u16 {
    let commits = timeline.commits.len() as u16;
    let (chrome, rows, budget) = match rows_wanted {
        _ if collapsed => (1, 0, remaining / 2),
        Some(wanted) => (
            TIMELINE_CHROME,
            wanted.clamp(1, commits),
            remaining.saturating_sub(MIN_TREE_ROWS),
        ),
        None => (TIMELINE_CHROME, commits, remaining / 2),
    };
    if budget < chrome {
        return 0;
    }
    (chrome + rows).min(budget)
}

/// The rows of an open timeline section that are not commits: its header
/// and the divider under it. The drag handler subtracts the same two to
/// turn where the divider was dropped into a count of commit rows.
pub(in crate::ui::orchestrator) const TIMELINE_CHROME: u16 = 2;

/// The sidebar's commit-timeline section.
///
/// Nothing here knows what a commit is. The extension says what the
/// section holds ([`git::timeline_section`]) and
/// `extension_view::render_section` draws it; this only supplies the host
/// state the extension is not allowed to hold — whether the section is
/// folded, how far it is scrolled, whether its divider is being dragged —
/// and tags the hits that come back with the surface they came from.
pub(super) fn render_timeline(
    frame: &mut ratatui::Frame<'_>,
    timeline: &uze_extensions::code::Timeline,
    model: &WorkspaceModel,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let section = uze_extensions::code::timeline_section(
        timeline,
        model.timeline_collapsed,
        model.timeline_scroll,
    );
    let mut section_hits = Vec::new();
    let mut column = Rows::over(Rect::new(rows.x, rows.y, rows.width, rows.remaining()));
    // The branch name this header carries is the one caption in the
    // column with no natural length — it is whatever somebody called the
    // work — so it is the one that runs past its room. While the pointer
    // is on the header, it slides instead of stopping at an "…": a
    // branch is read from both ends, and the end is the half an "…" eats.
    let hovered = model.hovered
        == Some(WorkspaceHit::Extension(ExtensionHit::CodeTimeline(
            ViewHit::ToggleSection,
        )));
    let hovered_row = match model.hovered {
        Some(WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::SelectItem(index)))) => {
            Some(index)
        }
        _ => None,
    };
    let sliding = crate::ui::extension_view::render_section_with(
        frame,
        &section,
        &mut column,
        model.dragging_timeline,
        hovered.then_some(model.tick),
        hovered_row.as_slice(),
        &mut section_hits,
    );
    hits.extend(section_hits.into_iter().map(|(rect, hit)| {
        (
            rect,
            WorkspaceHit::Extension(ExtensionHit::CodeTimeline(hit)),
        )
    }));
    sliding
}

/// One space's header row in the sidebar tree — its label — dim for every
/// space,
/// active one included: the space is a container, not the thing the
/// operator is looking at, so bold is reserved for the agent tab actually
/// receiving keystrokes (see `render_sidebar`'s agent-row `label_style`).
/// The active space's whole envelope (this header plus every tab/detail/cwd
/// row nested under it — see the `is_active_space` fill in
/// [`render_sidebar`]) gets a neutral background instead of a left accent
/// bar, so the highlight reads as "this whole block is where you are"
/// rather than a thin per-row marker or an on-brand "selected" tint
/// This header row itself stays at the lighter
/// [`theme::color(Token::SurfaceRaised)`] while the rows it anchors go one
/// step darker, [`theme::color(Token::SurfaceRaisedSubtle)`] — the title
/// lifts slightly above the block it names instead of blending into it.
pub(in crate::ui::orchestrator) fn render_space_header(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    session: &Session,
    space: &Space,
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let selected = space.id == session.workspace.selected_space;
    let collapsed = model.space_folded(space);
    // The header is a target of its own: clicking it lands on the space's
    // own shells, a context no agent row below speaks for. While that is
    // where the operator is — or the space is minimized, so its header is
    // all there is of it — the header wears the bar a selected item does.
    let is_current = header_is_current(space, session, identities, collapsed);
    let renaming_this = model
        .renaming
        .as_ref()
        .filter(|(target, _)| *target == RenameTarget::Space(space.id))
        .map(|(_, buffer)| buffer);
    // Bold while this is the space in front, and bright with it only
    // while the header is also the row receiving keystrokes — which is
    // when the space is minimized or speaks for no agent of its own, and
    // exactly when no agent row of it can be current. So the column
    // holds one bright name and, above it, the weight saying which block
    // that name is in.
    let name_style = label_style(is_current, selected);
    let fold = mark::disclosure(!collapsed);
    let mut spans = vec![
        space_gutter(selected, is_current),
        Span::styled(format!("{fold} "), theme::fg(Token::TextSecondary)),
    ];
    // The fold and the space after it: a target two cells wide, pushed
    // ahead of the row's own `SelectSpace` so it wins the click.
    hits.push((
        Rect::new(rect.x + 1, rect.y, 2, 1),
        WorkspaceHit::ToggleSpaceCollapsed(space.id),
    ));
    match renaming_this {
        Some(buffer) => spans.extend(rename_spans(buffer)),
        None => {
            // The label is what the space is called, and the row says that
            // alone. Where its work lives is the caption under it (see
            // [`render_space_caption`]) once the fold asks — a row that
            // can hold a path of any length, which this one, one line wide
            // and already carrying a name, could not.
            spans.push(Span::styled(space.label.clone(), name_style));
            if collapsed && let Some(status) = folded_status(model, space, identities) {
                spans.push(Span::styled(
                    format!(" {}", status.glyph(model.tick)),
                    Style::default().fg(status.color()),
                ));
            }
            push_trailing_controls(&mut spans, hits, rect, model, selected);
        }
    }
    // The space's own row is a target like any other, so when it is the
    // one in front it wears what a selected agent row wears: the same
    // trace of the accent, over its own lighter surface. Everything else
    // about the header is unchanged — the trace says "this row", not
    // "this block", which is what the fill underneath already says.
    let ground = if is_current {
        // The one overlay: exactly what an agent row in front wears, and
        // what the caption under a minimized header wears with it. A
        // second tone for the same meaning would read as two states.
        theme::tinted(Token::Accent, Token::SurfaceRaisedSubtle)
    } else {
        theme::color(Token::SurfaceRaised)
    };
    row::pad_to(
        &mut spans,
        rect.width,
        if selected || !collapsed {
            ground
        } else {
            theme::fade(ground)
        },
    );
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    hits.push((rect, WorkspaceHit::SelectSpace(space.id)));
}

/// The line down a space's leading column, from its header to its last
/// row: the space as one block, and only the `selected` one — a rail
/// beside every space said once per space what the block's own fill
/// already says, so the column read as a set of cages the selection had
/// to be found inside. `lit`, in the theme's accent, along what is
/// selected within it.
///
/// A bar rather than a box-drawing vertical, and the faintest tone rather
/// than the muted one: this marks the *edge* of a block, which is what
/// the `bar.*` glyphs are for and why they sit flush left in their cell,
/// where `│` is centred and drawn at whatever weight the font gives a
/// rule. It is the quietest thing in the column on purpose — it says
/// which block, not what is in it.
///
/// One hue for every agent, whichever group it sits in: the groups are
/// already told apart by where they stand in the column, under the
/// separator the two collections are split by, and a second axis saying
/// the same thing costs a colour that then means nothing else.
/// The one column between the space's gutter and an agent's row: empty
/// for an agent in the space's own root, and the isolated group's trunk
/// for one working in a checkout of its own.
///
/// One column, not two. Every step here is paid twice — once by the
/// group and once by the level inside it — so a two-column step put an
/// isolated agent's name four columns past its space's, and the column
/// spent more of itself on saying where a name sits than on the name.
/// Hence the indent *is* the trunk rather than sitting beside one: a
/// single column carrying both the level and the line that says these
/// rows are one collection.
///
/// The trunk is drawn by its ends, not by one glyph repeated: a `├`
/// carries a stem *upward* as well, so at the top of a group it pointed
/// at the blank row above and read as a line broken off rather than a
/// line starting. Each end closes.
///
/// A group of one takes no step at all: there is nothing for a line to
/// join, and an arm to a single row read as a stray mark beside a name
/// standing where every other name stands. The blank row above it still
/// says it is the other collection.
#[derive(Clone, Copy)]
pub(super) enum Branch {
    /// In the space's own root, or the only isolated one: no level, no
    /// line.
    None,
    /// The first of the isolated ones, opening the trunk.
    Opens,
    /// Isolated, with siblings above and below it.
    Carries,
    /// The last of them: the trunk closes on its name and nothing runs
    /// under its caption.
    Closes,
}

impl Branch {
    pub(super) fn of(isolated: bool, first: bool, last: bool) -> Self {
        match (isolated, first, last) {
            (false, ..) | (true, true, true) => Self::None,
            (true, true, false) => Self::Opens,
            (true, false, false) => Self::Carries,
            (true, false, true) => Self::Closes,
        }
    }

    /// What stands beside the agent's own name.
    pub(super) fn corner(self) -> Span<'static> {
        match self {
            Self::None => Span::raw(""),
            Self::Opens => Self::drawn(Symbol::TreeFirst),
            Self::Carries => Self::drawn(Symbol::TreeBranch),
            Self::Closes => Self::drawn(Symbol::TreeLast),
        }
    }

    /// What stands beside the caption under it, which is the same item:
    /// the trunk runs through it, unless the group ended on the name
    /// above.
    pub(super) fn stem(self) -> Span<'static> {
        match self {
            Self::Opens | Self::Carries => Self::drawn(Symbol::TreeVertical),
            Self::None => Span::raw(""),
            Self::Closes => Span::raw(" "),
        }
    }

    /// One column of a tree glyph. `tree.branch` and `tree.last` are two
    /// cells — a corner and the `─` that reached across to the status
    /// column — and that arm is what made the old connector cost a second
    /// column it did not need.
    pub(super) fn drawn(symbol: Symbol) -> Span<'static> {
        let glyph = theme::glyph(symbol);
        let corner = glyph.chars().next().map(String::from).unwrap_or_default();
        Span::styled(corner, theme::fg(Token::TextFaint))
    }
}

pub(super) fn space_gutter(selected: bool, lit: bool) -> Span<'static> {
    if !selected {
        return Span::raw(" ".repeat(theme::width(Symbol::BarThin) as usize));
    }
    Span::styled(theme::glyph(Symbol::BarThin), gutter_style(lit))
}

/// What a name wears: bright *and* bold for the one row receiving
/// keystrokes, bold alone for a name that is in front without being that
/// row, and quiet otherwise.
///
/// Two levels because there are two questions and one of them is not the
/// other's. Exactly one row in the column is `current` — an agent, or
/// the header of a space that is minimized or speaks for no agent — so
/// exactly one name is ever bright, which is what the hue is for. Which
/// *space* is in front is a second fact, and its header carried no mark
/// of it at all: a space of nothing but agents can never be the current
/// row, because there is no tab of its own to land on, so its name had
/// no state to reach however it was clicked. The weight answers that one
/// on its own, and costs the hue nothing.
pub(super) fn label_style(current: bool, in_front: bool) -> Style {
    let style = Style::default().fg(if current {
        theme::color(Token::TextBright)
    } else {
        theme::color(Token::TextInactive)
    });
    if current || in_front {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

pub(super) fn gutter_style(lit: bool) -> Style {
    if lit {
        theme::fg(Token::Accent)
    } else {
        theme::fg(Token::TextFaint)
    }
}

/// What a minimized space's agents are doing that is worth seeing through
/// the fold: one of them working, or one finished while nobody looked.
/// `None` when every agent is quiet — the header then says nothing more
/// than its name.
pub(super) fn folded_status(
    model: &WorkspaceModel,
    space: &Space,
    identities: &[AgentIdentity],
) -> Option<AgentTabStatus> {
    let statuses: Vec<AgentTabStatus> = agent_tabs_of(space, identities)
        .iter()
        .map(|tab| model.agent_tab_status(tab.pane.id, false))
        .collect();
    [AgentTabStatus::Working, AgentTabStatus::Completed]
        .into_iter()
        .find(|wanted| statuses.contains(wanted))
}

/// Whether a space's header is the selected item: its space is selected and
/// either no agent of it is — its own shells are — or it is minimized, so
/// the header is all there is of it.
pub(super) fn header_is_current(
    space: &Space,
    session: &Session,
    identities: &[AgentIdentity],
    folded: bool,
) -> bool {
    space.id == session.workspace.selected_space
        && (folded || space_context_agent(space, identities).is_none())
}

/// The mark a task's state puts after its label, and its hue: agent state
/// (`AgentTabStatus`) owns the column in front of the name, so what the
/// *task* is doing follows it.
///
/// Symbols only, never emoji: an emoji-presentation codepoint (`⚠`, `⏸`,
/// and `✎` in most terminal fonts) is drawn from a different family than
/// everything around it, double-width in some terminals and not others,
/// and immune to the hue this returns — it would ignore the color that
/// carries the meaning. Each state also gets a hue of its own rather than
/// three sharing `theme::color(Token::TextDim)`: color is what tells these apart at a glance,
/// the glyph is what tells them apart once you look. `Ready` deliberately
/// does *not* reuse `✓` — that is `AgentTabStatus::Completed`'s glyph one
/// column to the left, and the same mark in the same accent meaning two
/// different things is what made the second column read as an echo of the
/// first. It wears a mark of its own, `task.ready`, instead.
/// [`render_status_catalog`] is this table's legend and must move with it.
pub(in crate::ui::orchestrator) fn task_mark(state: &WorkStateView) -> Option<(String, Color)> {
    let (symbol, hue) = match state {
        // Nothing to report, and for the same reason: a task that has not
        // committed yet and one whose agent left with nothing both hold
        // no work. `Closed` in particular must not wear `Integrated`'s
        // arrow — that arrow claims a delivery.
        WorkStateView::Running | WorkStateView::Closed => return None,
        WorkStateView::Uncommitted => (Symbol::PlusMinus, theme::color(Token::StateInfo)),
        WorkStateView::Ready => (Symbol::TaskReady, theme::color(Token::Accent)),
        // The one mark that points away from UZE, because the work does:
        // it is on the forge, and what happens to it next happens there.
        // Muted for the same reason the button is — nothing is being asked
        // of the operator — and deliberately not `Integrated`'s arrow,
        // which claims the work is in the target.
        WorkStateView::Published => (Symbol::ArrowExternal, theme::color(Token::StatePublished)),
        WorkStateView::Integrating => (Symbol::Ellipsis, theme::color(Token::StateInFlight)),
        // Split, where one warning mark used to cover both: a paused rebase
        // wants your hands in the slot, a failed gate wants the code fixed —
        // different work, and the sidebar was the one surface that never
        // said which (the strip's own button already did).
        WorkStateView::Conflicted { .. } => {
            (Symbol::MarkAttention, theme::color(Token::StateWarning))
        }
        WorkStateView::GateFailed => (Symbol::MarkCross, theme::color(Token::StateDanger)),
        WorkStateView::Integrated => (Symbol::ArrowUp, theme::color(Token::StateLanded)),
        WorkStateView::Parked => (Symbol::Menu, theme::color(Token::TextMuted)),
    };
    Some((theme::glyph(symbol), hue))
}
