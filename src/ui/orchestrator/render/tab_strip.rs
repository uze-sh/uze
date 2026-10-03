//! The tab strip: the selected agent, its shells, the surface buttons and the notice chip.

use super::*;

pub(in crate::ui::orchestrator) fn agent_activity_frame(tick: usize) -> String {
    theme::frame(Symbol::StatusWorking, tick)
}

/// The horizontal tab strip above the pane: the *selected space's* shell
/// tabs only — agent tabs live exclusively in the sidebar now (see
/// [`render_sidebar`]), so a tab [`agent_identity_for_tab`] recognizes
/// never appears here, the same way a shell tab never appears in the
/// sidebar; other spaces' shell tabs don't appear here either, only the
/// currently selected space's. An active-tab marker in `theme::color(Token::Accent)`/bold-bright
/// text, wrapped in the same neutral [`theme::color(Token::SurfaceRaised)`] chip the
/// sidebar already uses for "this is where you are" (its active space's
/// envelope, its agent tab rows) — this strip used to skip that fill and
/// lean on text weight alone, which read as a lighter kind of "selected"
/// than everywhere else in the TUI. A dim close mark per tab once
/// more than one exists in the selected space, and a trailing "+" opening
/// another shell in it.
pub(in crate::ui::orchestrator) fn render_tab_strip(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    // No left padding: the pane below sits flush against the divider (see
    // `compute_layout`'s own `content_rows[1].x`, with no left inset
    // either), so the first tab's marker has to start at that same column
    // or it reads as offset from whatever the pane shows directly under it
    // — a shell prompt in particular, which starts flush at column 0 too.
    let inner = Rule::new(Edge::Bottom)
        .padding(Padding::new(0, 1, 0, 0))
        .render(frame, area);

    let Some(session) = &model.session else {
        frame.render_widget(
            Paragraph::new(Span::styled("connecting…", theme::fg(Token::TextMuted))),
            inner,
        );
        return;
    };

    // Scoped to the selected space — switching spaces (sidebar) switches
    // which shells this strip shows, the actual "don't mix projects"
    // payoff of spaces existing at all.
    let space = session.selected_space();
    // …and, within it, to one context: the agent in front of the person
    // followed by the shells opened alongside it, never another agent's.
    // A `None` context is the space's own — its bootstrap shell and
    // anything opened with no agent selected.
    let context = context_agent(model, identities);
    let strip = strip_tabs(space, context, identities);
    // Closability is a per-space rule (the server refuses a removal that
    // would empty a space — see `Session::remove_tab`), so it's judged
    // against every tab in the selected space, not just the ones this
    // strip goes on to show. A close that would take the last of them
    // still goes through: `close_tab_keeping_a_shell` opens the space's
    // replacement first.
    let can_close = space.tabs.len() > 1;

    // The header's right end goes down first — before a tab is measured,
    // let alone drawn — and that order is the whole arrangement. The
    // controls take the columns they need from the right edge inward, and
    // what they leave is the room the tab side then has: neither what the
    // workspace *says* nor how many shells are open can move something
    // the operator is about to *press*, and no tab can be laid over one.
    // A message takes whatever is left after both, ending in a divider
    // that keeps it apart from the controls.
    //
    // Three zones, in one order, reading left to right: what the work is
    // *doing* (the delivery notification), what it has *changed*, and
    // what can be *done* to it. Muted hairlines between them, because
    // they answer three different questions and a reader looking for one
    // of them should not have to sort the row out first. Each hairline
    // belongs to the zone on its left and goes when that zone does, so
    // the strip never draws a divider with nothing on one side of it.
    let mut trailing_right = inner.right();
    // Outside every zone, on the strip's own edge: it is not about this
    // checkout the way the other three are, and the one thing at the end
    // of a row is the one thing nothing else can push around. No fill —
    // it wears the plain backdrop, white at rest and the accent under the
    // pointer, so a single glyph out here never reads as a fourth zone of
    // one button.
    //
    // Its rect is what the dropdown hangs off, so it is measured before
    // it is drawn and the hit carries the same rectangle the glyph is
    // centred in — pad included, since the padding is as much of the
    // target as the glyph is.
    if selected_agent_context(model, identities).is_some() {
        // Air on the leading side only. The strip already insets its own
        // right edge by a column (see `inner`), and a trailing pad on top
        // of that left the one glyph at the end of the row floating two
        // columns off the margin every other row is measured against.
        let sparkle = theme::glyph(Symbol::MarkSparkle);
        let width = Span::raw(&sparkle).width() as u16 + chip::PAD;
        let rect = Rect::new(trailing_right.saturating_sub(width), inner.y, width, 1);
        let hit = WorkspaceHit::OpenAgentSupport(rect);
        let hue = match chip_state(model, Some(hit)) {
            ChipState::Resting => theme::color(Token::TextBright),
            _ => theme::color(Token::Accent),
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(sparkle, Style::default().fg(hue)),
            ])),
            rect,
        );
        hits.push((rect, hit));
        trailing_right = ZoneEdge::Bare.left_of(rect.x);
    }

    // ── actions ────────────────────────────────────────────────────────
    // The extensions are one group of buttons, not chips with air
    // between them: each puts its surface where the pane is — the
    // same kind of errand — and one continuous ground says that, where
    // detached chips read as unrelated controls.
    //
    // All are always there — a checkout always has a shape, files and
    // an answer to what it intends, even when that answer is "no layout"
    // — which is what lets them be the fixed set the eye learns, with
    // every zone that comes and goes sitting to the left of them. The one
    // exception is the operator's own: an extension switched off takes
    // its button with it, and all three off take the group.
    //
    // No bold: words side by side at the same weight read as one
    // strip of controls, and bold made each of them claim the row on its
    // own. The pair of glyphs at the tab side's end is the other way
    // round, and says why in its own place.
    {
        // The one standing in the pane wears the strip's one highlight —
        // the pane shows one thing, so the strip lights one thing, and
        // while a surface is up that is its button, not the tab it
        // covers. Said by the ground, not by the word's hue or weight.
        // In the order a change is read: what it intends, what it was
        // described as, and what it is.
        let buttons: Vec<GroupButton> = [
            (
                spec::CATALOG.id,
                WorkspaceHit::OpenSpec,
                Symbol::Spec,
                "spec",
                model.spec.is_some(),
            ),
            (
                architect::CATALOG.id,
                WorkspaceHit::OpenArchitect,
                Symbol::Architect,
                // Short, as the other two are: three errands in one group,
                // and the strip's room is the tabs' before it is the
                // buttons'.
                "arch",
                model.architect.is_some(),
            ),
            (
                code::CATALOG.id,
                WorkspaceHit::OpenFiles,
                Symbol::Code,
                "code",
                model.code.is_some(),
            ),
        ]
        .into_iter()
        .filter(|(extension, ..)| model.offers_extension(extension))
        .map(|(_, hit, symbol, name, open)| GroupButton {
            hit,
            label: surface_label(symbol, name),
            hue: theme::color(Token::TextSecondary),
            strong: false,
            lit: open,
            switch: true,
        })
        .collect();
        if !buttons.is_empty() {
            let width = group_width(&buttons);
            let rect = Rect::new(trailing_right.saturating_sub(width), inner.y, width, 1);
            let (spans, group_hits) =
                button_group(model, &buttons, Token::SurfaceRaised, (rect.x, rect.y));
            hits.extend(group_hits);
            frame.render_widget(Paragraph::new(Line::from(spans)), rect);
            trailing_right = ZoneEdge::Filled.left_of(rect.x);
        }
    }

    // ── git changes ────────────────────────────────────────────────────
    // What changed, in the two numbers and nothing else — its own zone
    // and never a ground of its own. There are two extensions, so there
    // are two dedicated buttons; a third filled thing beside them read as
    // a third surface to open rather than as a count of the work sitting
    // next to the ways in, and a plate sliding in under the pointer put
    // the button back the moment anyone went near it.
    //
    // It is still a door, and the hue is what says so: the counts sit at
    // their muted strength and come up to full under the pointer. The
    // colour is the badge's whole message — green is additions, red is
    // deletions — so it cannot be given up at rest the way a grey label
    // could; holding it back and letting the pointer restore it says
    // "this answers you" without anything being drawn.
    //
    // Absent for a clean checkout, hairline and all. It is a badge as
    // much as a door, and a badge with nothing to say says nothing rather
    // than zero — which costs no reachability, because the diff is one
    // mode switch away inside the surface `code` opens.
    //
    // What the checkout owes its upstream shares the zone, left of the
    // counts: it is the same checkout's git state, and only a report —
    // the pull and the push are the operator's to run.
    let summary = model
        .remembered
        .git_badge
        .as_ref()
        .and_then(|badge| badge.summary);
    let sync = model
        .focused_cwd()
        .map(|cwd| sync_counts(model, &cwd))
        .unwrap_or_default();
    if summary.is_some() || !sync.is_empty() {
        trailing_right = render_zone_hairline(frame, inner, trailing_right, ZoneEdge::Bare);
    }
    if let Some(summary) = summary {
        let hit = WorkspaceHit::OpenChanges;
        let (additions, deletions) = match chip_state(model, Some(hit)) {
            ChipState::Resting | ChipState::Static => (
                theme::color(Token::StateSuccessMuted),
                theme::color(Token::StateDangerMuted),
            ),
            ChipState::Hovered | ChipState::Pressed => (
                theme::color(Token::StateSuccess),
                theme::color(Token::StateDanger),
            ),
        };
        let text = format!("+{} -{}", summary.additions, summary.deletions);
        let width = Span::raw(&text).width() as u16 + 2 * chip::PAD;
        let rect = Rect::new(trailing_right.saturating_sub(width), inner.y, width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("+{}", summary.additions),
                    Style::default().fg(additions),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", summary.deletions),
                    Style::default().fg(deletions),
                ),
                Span::raw(" "),
            ])),
            rect,
        );
        hits.push((rect, hit));
        trailing_right = if sync.is_empty() {
            ZoneEdge::Bare.left_of(rect.x)
        } else {
            rect.x
        };
    }
    if !sync.is_empty() {
        // The changes' own inset already stands between the two readings.
        let trailing = if summary.is_some() { "" } else { " " };
        let line = Line::from([vec![Span::raw(" ")], sync, vec![Span::raw(trailing)]].concat());
        let width = line.width() as u16;
        let rect = Rect::new(trailing_right.saturating_sub(width), inner.y, width, 1);
        frame.render_widget(Paragraph::new(line), rect);
        trailing_right = ZoneEdge::Bare.left_of(rect.x);
    }

    // ── small notifications ────────────────────────────────────────────
    // Where the delivery stands, as a chip: this one keeps a control's
    // filled shape because in most of its states it *is* one, and a
    // report wearing the same shape sits on the recessed ground that says
    // it is not (see [`ChipState::Static`]).
    //
    // Only what UZE cut. An agent in the project's own root is on the
    // operator's branch, and rebasing it onto the target, running the
    // gate over it and pushing it is theirs to ask for. Its state is
    // still drawn in the sidebar — where the work stands is a fact either
    // way — but this zone is about a delivery UZE performs.
    if let Some(tab) = model.selected_tab()
        && let Some(task) = model.tab_task(tab)
        && task.isolated
        && let Some((text, hue, pressable)) =
            delivery_notification(task, &model.drawn_state(task), model.tick)
    {
        trailing_right = render_zone_hairline(frame, inner, trailing_right, ZoneEdge::Filled);
        let hit = pressable.then_some(WorkspaceHit::Deliver(tab));
        let chip = Chip::new(&text, hue, chip_state(model, hit));
        let rect = chip.rect_ending_at(trailing_right, inner.y);
        chip.render(frame, rect);
        if let Some(hit) = hit {
            hits.push((rect, hit));
        }
        trailing_right = ZoneEdge::Filled.left_of(rect.x);
    }

    // Where the tab side must stop. The controls at the right end are
    // laid out before a single tab is drawn, so the strip's own buttons
    // can never end up under one of them: a strip too narrow for both
    // loses a tab's tail, which the strip can scroll back to, rather than
    // the button that makes the next tab, which nothing else offers.
    let limit = trailing_right;
    let extension_in_front =
        model.code.is_some() || model.architect.is_some() || model.spec.is_some();
    let mut spans = Vec::new();
    let mut x = inner.x;
    let strip_len = strip.len();
    // Where to draw the drag's insertion indicator, if anywhere — captured
    // during the loop below but drawn only after `spans`' one accumulated
    // `Line` covering the whole strip is painted, since that single later
    // render would otherwise cover over a bar drawn mid-loop (unlike the
    // sidebar's per-row renders, every chip here shares that one `Line`).
    let mut drop_indicator: Option<Rect> = None;
    for (strip_index, tab) in strip.into_iter().enumerate() {
        if x >= limit {
            break;
        }
        let is_last = strip_index + 1 == strip_len;
        let is_agent = Some(tab.id) == context;
        // One highlight on the strip: a surface standing in the pane
        // takes it from the tab it covers.
        let selected = tab.id == space.selected_tab && !extension_in_front;
        let marker_fg = if selected {
            theme::color(Token::Accent)
        } else {
            theme::color(Token::TextFaint)
        };
        let label_style = if selected {
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD)
        } else {
            theme::fg(Token::TextInactive)
        };
        let marker = Span::styled(
            // The agent leading the strip wears the same mark the button
            // that creates one does, so the first chip reads as the agent
            // this context is about rather than another shell.
            format!(
                "{} ",
                theme::glyph(match (is_agent, selected) {
                    (true, _) => Symbol::MarkSparkle,
                    (false, true) => Symbol::StatusSelected,
                    (false, false) => Symbol::StatusIdle,
                })
            ),
            Style::default().fg(if is_agent && !selected {
                theme::color(Token::TextInactive)
            } else {
                marker_fg
            }),
        );
        let renaming_this = model
            .renaming
            .as_ref()
            .filter(|(target, _)| *target == RenameTarget::Tab(tab.id))
            .map(|(_, buffer)| buffer);
        let tab_label = match renaming_this {
            Some(buffer) => rename_spans(buffer),
            // One name per agent across the whole frame: the tab's own
            // label, which is what the sidebar draws and what renaming
            // edits. A working agent's task carries a label of its own
            // (the prompt's slug, or the bare task identifier when it has
            // no prompt) — showing that here left the same agent reading
            // as "engineer" in the sidebar and "gic3jz" up top.
            None => vec![Span::styled(tab.label.clone(), label_style)],
        };
        // An agent is never closed by a stray click — that stays a
        // right-click and a confirmation in the sidebar (see `ContextMenu`),
        // the same rule that keeps the sidebar's own agent rows unclosable.
        let show_close = renaming_this.is_none() && can_close && !is_agent;
        let close_width = theme::width(Symbol::MarkClose);
        let content_width = marker.width() as u16
            + tab_label.iter().map(Span::width).sum::<usize>() as u16
            + if show_close { 1 + close_width } else { 0 };
        // 1 column of padding on each side, reserved whether or not this
        // tab is selected — only the theme::color(Token::SurfaceRaised) fill toggles with
        // `selected`, never the width. Sizing the chip itself to
        // `selected` used to mean every tab shifted horizontally the
        // moment selection moved past it, reading as the whole strip
        // "resizing" on every tab switch instead of just recoloring.
        let chip_start = x;
        let chip_width = content_width + 2 * chip::PAD;

        let mut chip = vec![Span::raw(" ")];
        chip.push(marker);
        chip.extend(tab_label);
        if show_close {
            chip.push(Span::raw(" "));
            chip.push(Span::styled(
                theme::glyph(Symbol::MarkClose),
                theme::fg(Token::TextDim),
            ));
            hits.push((
                Rect::new(
                    chip_start + chip::PAD + content_width - close_width,
                    inner.y,
                    close_width,
                    1,
                ),
                WorkspaceHit::CloseTab(tab.id),
            ));
        }
        chip.push(Span::raw(" "));
        // A tab is a control too, and the pointer says so — one shade
        // under the selected chip's own fill, so hovering an unselected
        // tab never reads as having already switched to it.
        if selected {
            row::pad_to(&mut chip, chip_width, theme::color(Token::SurfaceRaised));
        } else if model.hovered == Some(WorkspaceHit::SelectTab(tab.id)) {
            row::pad_to(
                &mut chip,
                chip_width,
                theme::color(Token::SurfaceRaisedSubtle),
            );
        }
        hits.push((
            Rect::new(chip_start, inner.y, chip_width, 1),
            WorkspaceHit::SelectTab(tab.id),
        ));
        // Same convention as the sidebar's own indicator two functions
        // away: an accent bar on the target chip's own leading column —
        // dropping at the end of the strip lands the bar on the last
        // chip too, not on a slot past it.
        if model.dragging_tab.is_some_and(|dragging| {
            dragging.is_pending_drop_row(TabDragGroup::Strip(space.id, context), tab.id, is_last)
        }) {
            drop_indicator = Some(Rect::new(chip_start, inner.y, 1, 1));
        }
        spans.extend(chip);
        // Just 1 column between chips, not 3 — each chip already reserves
        // its own 1-column pad on both sides (see `PAD` above), so a full
        // 3-column gap on top of that read as too much air once every tab
        // carried that padding, not just the selected one.
        spans.push(Span::raw(" "));
        x += chip_width + 1;
        // A "/" closes the agent leading the strip off from the shells
        // opened beside it, and from the "+" that opens another: two kinds
        // of tab, and without it the gap after the agent read as just
        // another gap between shells. No leading space — the chip's own
        // trailing gap is one — so it sits one column off either side.
        // `TextFaint`, the hue of the zone hairlines on this same backdrop:
        // a separator is not text, and the brighter `TextMuted` made it
        // read as one more word in the strip.
        if is_agent && x < limit {
            spans.push(Span::styled("/", theme::fg(Token::TextFaint)));
            spans.push(Span::raw(" "));
            x += 2;
        }
    }
    // A bold "+" creates a new shell tab directly. It stays neutral, just
    // bolder, being the plain action; a new agent is asked for on its
    // space's header in the sidebar (see `push_trailing_controls`), which
    // is where the space it lands in is named.
    //
    // `SurfaceRaisedBright` backs it where the actions take the plain
    // `SurfaceRaised`: a bare glyph has no word's weight carrying it, and
    // at the plain strength it reads as barely there.
    let buttons = [GroupButton {
        hit: WorkspaceHit::NewTab,
        label: "+".to_owned(),
        hue: theme::color(Token::TextInactive),
        strong: true,
        lit: false,
        switch: false,
    }];
    if x + group_width(&buttons) <= limit {
        let (actions, group_hits) =
            button_group(model, &buttons, Token::SurfaceRaisedBright, (x, inner.y));
        hits.extend(group_hits);
        spans.extend(actions);
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(
            inner.x,
            inner.y,
            limit.saturating_sub(inner.x),
            inner.height,
        ),
    );
    if let Some(rect) = drop_indicator {
        frame.render_widget(
            Paragraph::new(theme::glyph(Symbol::BarThick)).style(theme::fg(Token::Accent)),
            rect,
        );
    }

    render_notice_chip(frame, model, inner, trailing_right);
}

/// A surface chip's label: the word, and the glyph standing for it in a
/// glyph set that actually carries one.
///
/// Two of the three shipped sets deliberately carry none. A surface is an
/// idea — the code of a checkout, the shape of a project, what changed —
/// and plain Unicode has no sign for any of them, so what a set without
/// icons can offer is a box or an arrow that means nothing here. The word
/// already says it; the glyph joins it only where there is a real one.
pub(super) fn surface_label(symbol: Symbol, name: &str) -> String {
    match theme::glyph(symbol) {
        glyph if glyph.is_empty() => name.to_string(),
        glyph => format!("{glyph} {name}"),
    }
}

/// One member of a button group: what a click on it means, the label it
/// wears, and the hue that label takes.
pub(super) struct GroupButton {
    pub(super) hit: WorkspaceHit,
    pub(super) label: String,
    pub(super) hue: Color,
    /// Bold, for a member whose label is a bare glyph. A word carries
    /// itself at any weight; a lone "+" on a lifted fill reads as barely
    /// there beside one.
    pub(super) strong: bool,
    /// Standing in the pane: the strip's one highlight, filled.
    pub(super) lit: bool,
    /// A switch rather than a push: pressing it lights it or puts it out,
    /// and that change is its answer. The press flash on top of it drew a
    /// third look between the two — lit, then the flash, then unlit.
    pub(super) switch: bool,
}

/// The columns one member of a group claims: its label, and the air each
/// side of it that is as much of the control as the label is.
pub(super) fn group_member_width(label: &str) -> u16 {
    Span::raw(label).width() as u16 + 2 * chip::PAD
}

pub(super) fn group_width(buttons: &[GroupButton]) -> u16 {
    buttons
        .iter()
        .map(|button| group_member_width(&button.label))
        .sum()
}

/// A group of buttons: one continuous ground, the members meeting on
/// their own padding, each answering the pointer alone — so what lifts
/// under it is exactly what a click lands on.
///
/// Nothing is drawn between them. A divider glyph inside a group is a
/// column belonging to no member, and a group like this is only ever one
/// ground or another: the moment one member is hovered, that orphan
/// column keeps the resting fill and the seam reads as a sliver of a
/// third thing wedged between two buttons. With nothing there, the
/// members meet on their padding and the boundary *is* where the fill
/// changes — visible exactly when there is something to see, and nowhere
/// at rest. A drawn divider belongs on the flat backdrop (the zone
/// hairlines, the "/" after the agent tab), where there is no fill for
/// it to disagree with.
///
/// `resting` is the group's own fill, which is the one thing that differs
/// between the two groups on this strip: a pair of bare glyphs needs a
/// brighter one than a pair of words, having no weight or hue of its own
/// otherwise carrying it. Hover and press are the skins every other
/// control in this row wears.
///
/// It answers with spans and rects rather than drawing, because the two
/// callers place a group differently: one lays it at the strip's right
/// end in a rect of its own, the other appends it to the single line the
/// tab side is already building.
///
/// Local to this screen, not in `src/ui/widget/`, by that module's own
/// threshold: the second *file* is what moves a helper into the
/// vocabulary, and both groups are drawn by this function's own caller.
pub(super) fn button_group(
    model: &WorkspaceModel,
    buttons: &[GroupButton],
    resting: Token,
    origin: (u16, u16),
) -> (Vec<Span<'static>>, Vec<(Rect, WorkspaceHit)>) {
    let (mut x, y) = origin;
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    for button in buttons {
        let (hue, ground) = match chip_state(model, Some(button.hit)) {
            // Filled with the brightest ink and written in the backdrop,
            // the way a strong button is: a lit member is the one thing
            // on the strip that must be found at a glance, and a shade
            // over its neighbour's plate was not enough to find it. It
            // stays lit under the pointer — it is already the answer.
            _ if button.lit => (
                theme::color(Token::SurfaceBackground),
                theme::color(Token::TextBright),
            ),
            ChipState::Resting => (button.hue, theme::color(resting)),
            ChipState::Pressed if button.switch => ChipState::Hovered.skin(button.hue),
            other => other.skin(button.hue),
        };
        let mut label = Style::default().fg(hue).bg(ground);
        if button.strong {
            label = label.add_modifier(Modifier::BOLD);
        }
        let taken = group_member_width(&button.label);
        spans.push(Span::styled(" ", Style::default().bg(ground)));
        spans.push(Span::styled(button.label.clone(), label));
        spans.push(Span::styled(" ", Style::default().bg(ground)));
        hits.push((Rect::new(x, y, taken, 1), button.hit));
        x += taken;
    }
    (spans, hits)
}

/// Whether a zone's edge is a filled surface or bare text on the strip's
/// own backdrop.
///
/// It decides one column, and that column is the whole of the alignment.
/// A filled chip's padding belongs to the control — it is lit, hovered
/// and clicked with the glyphs — so the air beside a hairline has to be a
/// column of its own. Bare text's padding *is* that air already, and
/// adding a second column put the hairline one off centre between two
/// zones: two blank columns against the unfilled side, one against the
/// filled one.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ZoneEdge {
    Filled,
    Bare,
}

impl ZoneEdge {
    /// Where the next thing to the left may end, given this zone starts at
    /// `x`.
    pub(super) fn left_of(self, x: u16) -> u16 {
        match self {
            Self::Filled => x.saturating_sub(1),
            Self::Bare => x,
        }
    }
}

/// The hairline between two zones of the strip's right end, drawn ending
/// at `right` and answering with the column the zone to its left may end
/// at — which `next` decides, since that zone's own edge is what the air
/// beside the mark is measured against.
///
/// On the plain backdrop, where a drawn divider has no fill to disagree
/// with, and in the faintest text hue there is: it separates two things
/// that are already far apart in meaning, so it has only to be found when
/// looked for, never read. It sits outside every rect a pointer can land
/// on, and a zone draws its own before itself, so a zone with nothing to
/// say takes its divider with it.
pub(super) fn render_zone_hairline(
    frame: &mut ratatui::Frame<'_>,
    inner: Rect,
    right: u16,
    next: ZoneEdge,
) -> u16 {
    let divider = theme::glyph(Symbol::TreeColumnDivider);
    let width = Span::raw(&divider).width() as u16;
    let rect = Rect::new(right.saturating_sub(width), inner.y, width, 1);
    frame.render_widget(
        Paragraph::new(Span::styled(divider, theme::fg(Token::TextFaint))),
        rect,
    );
    next.left_of(rect.x)
}

/// Everything the workspace has to say, in the one place it says it: the
/// header's own row, left of the actions and divided from them, where the
/// operator's eye already is. Nothing here is clickable and nothing here
/// moves a button — the actions were laid out before this was, and this
/// only takes the room they left.
pub(super) fn render_notice_chip(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    inner: Rect,
    actions_left: u16,
) {
    let Some(chip) = model.notice_chip() else {
        return;
    };
    let spans = vec![
        Span::raw(" "),
        // Work still running says so by moving, which is what buys the
        // words the right to be two: "delivering", not "delivering every
        // ready task…".
        Span::styled(
            match chip.busy {
                true => format!("{} ", agent_activity_frame(model.tick)),
                false => String::new(),
            },
            theme::fg(Token::Accent),
        ),
        Span::styled(
            chip.text,
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        // The zone divider, in the same hue and on the same plain backdrop
        // as every other hairline between the header's zones.
        // No filled chip behind any of this: a message is not a control,
        // and the raised surface is what made it read as one.
        Span::styled(
            theme::glyph(Symbol::TreeColumnDivider),
            theme::fg(Token::TextFaint),
        ),
    ];
    // Never past the strip's left edge: what does not fit is this
    // message's own tail, clipped by its rect, not the tabs beside it.
    let Some(room) = actions_left.checked_sub(inner.x).filter(|room| *room > 0) else {
        return;
    };
    let width = (spans.iter().map(Span::width).sum::<usize>() as u16).min(room);
    let rect = Rect::new(actions_left.saturating_sub(width), inner.y, width, 1);
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}
