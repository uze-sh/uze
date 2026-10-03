//! The navigator: the menu, the selector, the trail, the choice list and the tree.

use super::*;

/// A board's menu, on one row: two selectors, the group on show and the
/// item on show in it, each opening a list over the board.
///
/// One level is visible at a time and one word of each, which is what
/// keeps the row readable however many items a group holds — a row of
/// every item is a row that has to be cut, and the cut lands on whatever
/// the viewer was looking for. Once something has been entered the way
/// in takes the place of the second selector, whose job its last step
/// then does.
pub(super) fn render_menu(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    board: Rect,
    navigator: &Navigator,
    trail: &[TrailStep],
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let groups = groups_of(navigator);
    let Some(active) = active_group(navigator).and_then(|id| groups.iter().find(|g| g.id == id))
    else {
        return;
    };
    let items = items_of(navigator, active.id);
    let on_show = items
        .iter()
        .find(|item| item.selected)
        .or(items.first())
        .map_or("", |item| item.name);

    // A selector opens only where there is something to choose. With one
    // group, or one item in it, the list would offer what is already on
    // show — so the mark that says it opens is left off and the press
    // that would open it is never offered.
    let opens_groups = groups.len() > 1;
    let group_selector = render_selector(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        active.name,
        // How many the area holds, unless the trail beside it is already
        // showing them one by one.
        Some(active.items).filter(|&items| items > 1 && trail.is_empty()),
        opens_groups,
    );
    if opens_groups {
        hits.push((group_selector, ViewHit::ChooseGroup));
    }

    let divider = "  /  ";
    let mut x = group_selector.right();
    if x.saturating_add(divider.len() as u16) < area.right() {
        frame.render_widget(
            Paragraph::new(TextSpan::styled(divider, theme::fg(Token::TextDim))),
            Rect::new(x, area.y, divider.len() as u16, 1),
        );
        x += divider.len() as u16;
    }
    let rest = Rect::new(x, area.y, area.right().saturating_sub(x), 1);
    // Which of the two the second half is, is the view's answer: a
    // descent is walked, and where there is none there is a list to pick
    // from. One control, never both — an area is one thing or the other.
    let opens_items = trail.is_empty() && items.len() > 1;
    let item_selector = match trail.is_empty() {
        true => render_selector(frame, rest, on_show, None, opens_items),
        false => render_trail(frame, rest, trail, hits),
    };
    if opens_items {
        hits.push((item_selector, ViewHit::ChooseItem));
    }

    match navigator.choosing {
        Some(Choosing::Group(highlighted)) => {
            let rows: Vec<ChoiceRow<'_>> = groups
                .iter()
                .map(|group| ChoiceRow {
                    hit: ViewHit::ToggleGroup(group.id),
                    name: group.name,
                    trailing: group.items.to_string(),
                    highlighted: group.id == highlighted,
                })
                .collect();
            render_choice_list(
                frame,
                group_selector,
                board,
                &rows,
                ViewHit::ChooseGroup,
                hits,
            );
        }
        Some(Choosing::Item(highlighted)) => {
            let rows: Vec<ChoiceRow<'_>> = items
                .iter()
                .map(|item| ChoiceRow {
                    hit: ViewHit::SelectItem(item.id),
                    name: item.name,
                    trailing: String::new(),
                    highlighted: item.id == highlighted,
                })
                .collect();
            render_choice_list(
                frame,
                item_selector,
                board,
                &rows,
                ViewHit::ChooseItem,
                hits,
            );
        }
        None => {}
    }
}

/// One selector: what is chosen, how many there are to choose from when
/// that is worth a number, and — when it opens onto anything other than
/// itself — the mark that says so. Without that mark it is not a control
/// but a label: where the viewer is.
pub(super) fn render_selector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    name: &str,
    count: Option<usize>,
    opens: bool,
) -> Rect {
    let raised = theme::color(Token::SurfaceRaised);
    let mut spans = vec![TextSpan::styled(
        format!(" {name} "),
        theme::fg_bold(Token::TextBright).bg(raised),
    )];
    if let Some(count) = count {
        spans.push(TextSpan::styled(
            format!("{count} "),
            theme::fg(Token::TextDim).bg(raised),
        ));
    }
    if opens {
        spans.push(TextSpan::styled(
            format!("{} ", theme::glyph(Symbol::ChevronExpanded)),
            theme::fg(Token::TextMuted).bg(raised),
        ));
    }
    let rect = Rect::new(area.x, area.y, spans_width(&spans).min(area.width), 1);
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    rect
}

pub(super) struct MenuGroup<'a> {
    pub(super) id: usize,
    pub(super) name: &'a str,
    pub(super) items: usize,
}

pub(super) struct MenuItem<'a> {
    pub(super) id: usize,
    pub(super) name: &'a str,
    pub(super) selected: bool,
}

pub(super) fn groups_of(navigator: &Navigator) -> Vec<MenuGroup<'_>> {
    let mut groups: Vec<MenuGroup<'_>> = Vec::new();
    for row in &navigator.rows {
        match row {
            // A board's menu has one level of heading, so a band is one.
            NavigatorRow::Group { id, name, .. } | NavigatorRow::Band { id, name, .. } => {
                groups.push(MenuGroup {
                    id: *id,
                    name,
                    items: 0,
                });
            }
            NavigatorRow::Item { .. } => {
                if let Some(group) = groups.last_mut() {
                    group.items += 1;
                }
            }
            NavigatorRow::Gap => {}
        }
    }
    groups
}

pub(super) fn items_of(navigator: &Navigator, group: usize) -> Vec<MenuItem<'_>> {
    let mut items = Vec::new();
    let mut current = None;
    for row in &navigator.rows {
        match row {
            NavigatorRow::Group { id, .. } | NavigatorRow::Band { id, .. } => current = Some(*id),
            NavigatorRow::Item {
                id, name, selected, ..
            } if current == Some(group) => items.push(MenuItem {
                id: *id,
                name,
                selected: *selected,
            }),
            NavigatorRow::Item { .. } | NavigatorRow::Gap => {}
        }
    }
    items
}

pub(super) fn spans_width(spans: &[TextSpan<'_>]) -> u16 {
    spans.iter().map(TextSpan::width).sum::<usize>() as u16
}

/// The group the selection is in — the one whose items are on show.
pub(super) fn active_group(navigator: &Navigator) -> Option<usize> {
    let mut group = None;
    for row in &navigator.rows {
        match row {
            NavigatorRow::Group { id, .. } | NavigatorRow::Band { id, .. } => group = Some(*id),
            NavigatorRow::Item { selected: true, .. } => return group,
            NavigatorRow::Item { .. } | NavigatorRow::Gap => {}
        }
    }
    // Nothing selected is still somewhere: the first group.
    navigator.rows.iter().find_map(|row| match row {
        NavigatorRow::Group { id, .. } | NavigatorRow::Band { id, .. } => Some(*id),
        NavigatorRow::Item { .. } | NavigatorRow::Gap => None,
    })
}

/// A descent, drawn as the row it is walked on: a step per level, the
/// one being looked at drawn as the chip the item selector would be, and
/// every other step a place to go — back, for the ones already walked,
/// and on, for the ones this descent reaches but nobody has entered.
///
/// Cut around the current step rather than from one end, because that is
/// the one step that must never be the one cut off, and it need not be
/// the last: a model's levels are all there from the start, and the
/// viewer may be standing in the middle of them. Answers where the
/// current step was drawn.
pub(super) fn render_trail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    trail: &[TrailStep],
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rect {
    let chevron = format!(" {} ", theme::glyph(Symbol::ChevronRight));
    let chevron_width = TextSpan::raw(chevron.as_str()).width() as u16;
    let more = theme::glyph(Symbol::Ellipsis);
    let more_width = TextSpan::raw(more.as_str()).width() as u16 + chevron_width;
    let width_of = |step: &TrailStep| TextSpan::raw(step.name.as_str()).width() as u16 + 2;
    let Some(current) = trail.iter().position(|step| step.current) else {
        return Rect::new(area.x, area.y, 0, 1);
    };

    // Outward from the current step, taking what fits — the way back
    // first, because a step already walked is the one more likely wanted.
    let (mut first, mut last) = (current, current);
    let mut used = width_of(&trail[current]) + 2;
    loop {
        let room = |used: u16, edge: bool| {
            area.width
                .saturating_sub(used + if edge { more_width } else { 0 })
        };
        let back = first
            .checked_sub(1)
            .filter(|&step| width_of(&trail[step]) + chevron_width <= room(used, step > 0));
        if let Some(step) = back {
            used += width_of(&trail[step]) + chevron_width;
            first = step;
        }
        let on = Some(last + 1)
            .filter(|&step| step < trail.len())
            .filter(|&step| {
                width_of(&trail[step]) + chevron_width <= room(used, step + 1 < trail.len())
            });
        if let Some(step) = on {
            used += width_of(&trail[step]) + chevron_width;
            last = step;
        }
        if back.is_none() && on.is_none() {
            break;
        }
    }

    let quiet = theme::fg(Token::TextDim);
    let mut x = area.x;
    let draw = |frame: &mut ratatui::Frame<'_>, text: String, style: Style, x: &mut u16| {
        let width =
            (TextSpan::raw(text.as_str()).width() as u16).min(area.right().saturating_sub(*x));
        let rect = Rect::new(*x, area.y, width, 1);
        frame.render_widget(Paragraph::new(TextSpan::styled(text, style)), rect);
        *x += width;
        rect
    };
    if first > 0 {
        draw(frame, more.clone(), quiet, &mut x);
        draw(frame, chevron.clone(), quiet, &mut x);
    }
    let mut here = Rect::new(x, area.y, 0, 1);
    for (step, name) in trail.iter().enumerate().take(last + 1).skip(first) {
        if step == current {
            here = render_selector(
                frame,
                Rect::new(x, area.y, area.right().saturating_sub(x), 1),
                &name.name,
                None,
                false,
            );
            x = here.right();
        } else {
            // A step behind is the way back and a step ahead is a level
            // not yet reached: told apart by weight, since both are
            // pressed the same way.
            let ink = match step < current {
                true => theme::fg(Token::TextSecondary),
                false => quiet,
            };
            let rect = draw(frame, format!(" {} ", name.name), ink, &mut x);
            hits.push((rect, ViewHit::SelectTrail(step)));
        }
        if step < last {
            draw(frame, chevron.clone(), quiet, &mut x);
        }
    }
    if last + 1 < trail.len() {
        draw(frame, chevron, quiet, &mut x);
        draw(frame, more, quiet, &mut x);
    }
    here
}

pub(super) struct ChoiceRow<'a> {
    pub(super) hit: ViewHit,
    pub(super) name: &'a str,
    pub(super) trailing: String,
    pub(super) highlighted: bool,
}

/// An open list, hung from the selector it belongs to and drawn over the
/// board: a row for each choice, the highlighted one filled. A list
/// longer than the board is tall shows the rows around the highlighted
/// one, which is the one row that must never be the one cut off.
///
/// Dressed as the workspace's other dropdowns are — the surface it sits
/// on rather than one raised above it, a fill on the highlighted row and
/// nothing on the rest. The highlight follows the pointer, so hovering a
/// row *is* highlighting it and there is no second, quieter state to
/// draw.
pub(super) fn render_choice_list(
    frame: &mut ratatui::Frame<'_>,
    selector: Rect,
    board: Rect,
    rows: &[ChoiceRow<'_>],
    own_selector: ViewHit,
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let widest = rows
        .iter()
        .map(|row| TextSpan::raw(row.name).width() + row.trailing.len())
        .max()
        .unwrap_or(0) as u16;
    let width = widest
        .saturating_add(6)
        .max(selector.width)
        .min(board.width);
    let visible = (rows.len() as u16)
        .min(board.height.saturating_sub(2))
        .max(1);
    let area = Rect::new(
        selector.x.min(board.right().saturating_sub(width)),
        selector.bottom(),
        width,
        visible + 2,
    );
    let highlighted = rows.iter().position(|row| row.highlighted).unwrap_or(0);
    let first = highlighted
        .saturating_sub(usize::from(visible) / 2)
        .min(rows.len().saturating_sub(usize::from(visible)));

    frame.render_widget(Clear, area);
    let mut frame_surface = Surface::card();
    // A list that was cut says so, and where in it the highlight is —
    // otherwise its last row reads as the last there is.
    if rows.len() > usize::from(visible) {
        frame_surface = frame_surface.hint(Line::from(TextSpan::styled(
            format!(" {}/{} ", highlighted + 1, rows.len()),
            theme::fg(Token::TextDim),
        )));
    }
    frame_surface.render(frame, area);
    for (line, row) in rows.iter().skip(first).take(visible.into()).enumerate() {
        let rect = Rect::new(
            area.x + 1,
            area.y + 1 + line as u16,
            area.width.saturating_sub(2),
            1,
        );
        // A filled bar for the highlighted row, not just bold text: the
        // same narrowly-scoped exception the agent picker makes, for the
        // same reason — a menu the pointer and the arrows share needs the
        // affordance.
        let fill = match row.highlighted {
            true => theme::color(Token::Accent),
            false => theme::color(Token::SurfaceBackground),
        };
        let ink = match row.highlighted {
            true => theme::fg_bold(Token::SurfaceBackground),
            false => theme::fg(Token::TextInactive),
        };
        let gap = usize::from(rect.width)
            .saturating_sub(TextSpan::raw(row.name).width() + row.trailing.len() + 2);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                TextSpan::styled(format!(" {}", row.name), ink.bg(fill)),
                TextSpan::styled(" ".repeat(gap), Style::default().bg(fill)),
                TextSpan::styled(
                    format!("{} ", row.trailing),
                    theme::fg(Token::TextDim).bg(fill),
                ),
            ])),
            rect,
        );
        hits.push((rect, row.hit));
    }
    // The list's own frame is still the list: a click on it is not a
    // click on the board beneath.
    hits.push((area, own_selector));
}

pub(super) fn render_navigator(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    navigator: &Navigator,
    subjects: &[Mode],
    scroll: NavigatorScroll,
    resizing: bool,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> (NavigatorScroll, Option<Scrollbar>) {
    // Padding on the divider's side only: a column indented from the
    // pane's own edge as well leaves its rows further in than the nav
    // above them, and there is nothing on that side for them to clear.
    let inner = Rule::draggable(Edge::Right, resizing)
        .ground(Token::SurfaceBackground)
        .padding(Padding::new(0, 1, 0, 0))
        .render(frame, area);

    // A heading only where nothing above it already names the list: the
    // nav row says which half you are in, and a word repeating it is a
    // word that has to be read to learn nothing.
    let mut heading = match subjects.is_empty() {
        true => vec![TextSpan::styled(
            navigator.heading.clone(),
            Style::default()
                .fg(theme::color(Token::TextSecondary))
                .add_modifier(Modifier::BOLD),
        )],
        false => Vec::new(),
    };
    // The panel's own right padding is the gap a trailing caption keeps
    // off the divider, so the row is measured as if it were the pad.
    row::push_trailing(
        &mut heading,
        inner.width + TRAILING_PAD,
        navigator.badge.clone(),
        theme::color(Token::TextMuted),
    );
    frame.render_widget(
        Paragraph::new(Line::from(heading)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let rows = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    );
    // The groove comes out of the list's own width, so a row is never
    // drawn under the handle that would sit on top of it.
    // On the divider itself, not beside it. Beside it was two lines, and
    // adjacent was still two lines; drawn *on* it, the divider is the
    // line and the handle is the stretch of it that says where you are —
    // which is the only thing a scrollbar was ever adding.
    //
    // Sharing the column is what makes the two gestures separable rather
    // than ambiguous: the handle is grabbed to scroll, and the rest of
    // the line is grabbed to move the divider. What it costs is clicking
    // the empty groove to jump, which is the lesser of the two.
    let bar = Scrollbar::measure(
        Rect::new(
            area.right().saturating_sub(Scrollbar::width()),
            rows.y,
            Scrollbar::width(),
            rows.height,
        ),
        rows.height as usize,
        navigator.rows.len(),
    );
    let list = rows;
    let visible = list.height as usize;
    let settled = scroll.settled(navigator.anchor, navigator.rows.len(), visible);
    for (offset, row) in navigator
        .rows
        .iter()
        .skip(settled.first)
        .take(visible)
        .enumerate()
    {
        let rect = Rect::new(list.x, list.y + offset as u16, list.width, 1);
        match row {
            NavigatorRow::Gap => {}
            // Where the eye lands to find its part of the list, so it is
            // told apart from the tree by form rather than by hue: capitals
            // in bold, with the count quieter beside them. Plain ASCII on
            // purpose — small capitals and subscript digits are glyphs a
            // terminal's font may not carry, and a heading drawn as boxes
            // divides nothing.
            NavigatorRow::Band {
                id,
                name,
                count,
                collapsed,
            } => {
                let fold = mark::disclosure(!*collapsed);
                let spans = vec![
                    TextSpan::raw(" "),
                    TextSpan::styled(format!("{fold} "), theme::fg(Token::TextMuted)),
                    TextSpan::styled(
                        name.to_uppercase(),
                        theme::fg(Token::TextSecondary).add_modifier(Modifier::BOLD),
                    ),
                    TextSpan::raw(" "),
                    TextSpan::styled(count.to_string(), theme::fg(Token::TextMuted)),
                ];
                frame.render_widget(Paragraph::new(Line::from(spans)), rect);
                hits.push((rect, ViewHit::ToggleGroup(*id)));
            }
            NavigatorRow::Group {
                id,
                name,
                depth,
                collapsed,
                icon,
            } => {
                let fold = mark::disclosure(!*collapsed);
                let mut spans = vec![
                    TextSpan::raw(" "),
                    TextSpan::raw(tree_indent(*depth, rect.width)),
                    TextSpan::styled(format!("{fold} "), theme::fg(Token::TextMuted)),
                ];
                spans.extend(row_icon(*icon));
                // A compact row's name is a path, whose end is where the
                // row leads; a plain directory's is a name, read from the
                // start.
                let room = label_room(&spans, rect.width);
                let name = match name.contains('/') {
                    true => text::elide_head(name, room),
                    false => text::elide(name, room),
                };
                spans.push(TextSpan::styled(name, theme::fg(Token::TextSecondary)));
                frame.render_widget(Paragraph::new(Line::from(spans)), rect);
                hits.push((rect, ViewHit::ToggleGroup(*id)));
            }
            NavigatorRow::Item {
                id,
                name,
                depth,
                marker,
                marker_side,
                detail,
                selected,
                icon,
            } => {
                let label_style = match (*selected, navigator.focused) {
                    (true, true) => Style::default()
                        .fg(theme::color(Token::TextBright))
                        .add_modifier(Modifier::BOLD),
                    (true, false) => theme::fg(Token::TextBright),
                    (false, _) => theme::fg(Token::TextInactive),
                };
                // The selected row is marked the way every other list in
                // the product marks its selection — the accent bar and the
                // selected surface the status listing uses — rather than a
                // neutral lift that the diff beside it easily outshone.
                let mut spans = vec![
                    TextSpan::styled(
                        if *selected {
                            theme::glyph(Symbol::TreeColumnDivider)
                        } else {
                            " ".to_owned()
                        },
                        theme::fg(Token::Accent),
                    ),
                    TextSpan::raw(tree_indent(*depth, rect.width)),
                ];
                match marker_side {
                    MarkerSide::Leading => {
                        spans.push(leading_marker(marker));
                        spans.extend(row_icon(*icon));
                        let room = label_room(&spans, rect.width);
                        spans.push(TextSpan::styled(
                            text::elide_file_name(name, room),
                            label_style,
                        ));
                    }
                    MarkerSide::Trailing => {
                        spans.extend(row_icon(*icon));
                        push_flat_label(&mut spans, rect.width, name, detail, label_style, marker);
                    }
                }
                if *selected {
                    row::pad_to(&mut spans, rect.width, theme::color(Token::SurfaceSelected));
                }
                frame.render_widget(Paragraph::new(Line::from(spans)), rect);
                hits.push((rect, ViewHit::SelectItem(*id)));
            }
        }
    }
    if let Some(bar) = bar {
        bar.render_on_draggable(frame, settled.first, resizing);
    }
    (settled, bar)
}

/// A tree row's marker: in the column a group's fold mark stands in, and
/// holding that column's width even when it has nothing to say — so a
/// file with no status still lines its icon and name up with the folders
/// beside it.
pub(super) fn leading_marker(marker: &Span) -> TextSpan<'static> {
    let fold_width = TextSpan::raw(theme::glyph(Symbol::ChevronCollapsed)).width();
    let marker_width = TextSpan::raw(marker.text.as_str()).width();
    styled(&Span {
        text: format!(
            "{}{} ",
            marker.text,
            " ".repeat(fold_width.saturating_sub(marker_width))
        ),
        ..marker.clone()
    })
}

/// A flat row's name, its `detail` quieter after it, and the marker pinned
/// to the right edge. The name gives way last: it is what the row is read
/// for, and the detail only tells apart two rows that share one.
/// Levels past which a tree indents by one column rather than two.
pub(super) const FULL_INDENT_LEVELS: usize = 4;

/// Columns a tree row keeps for its name however deep it sits.
pub(super) const LABEL_FLOOR: usize = 12;

/// The blank a row at `depth` starts with.
///
/// Two columns a level near the top, where the shape of the tree is read,
/// and one past [`FULL_INDENT_LEVELS`], where it is only counted; and never
/// so much that fewer than [`LABEL_FLOOR`] columns are left for the name.
/// Indentation that pushes the name out of the column shows where a file
/// is at the cost of which file it is.
pub(super) fn tree_indent(depth: usize, width: u16) -> String {
    let natural = 2 * depth.min(FULL_INDENT_LEVELS) + depth.saturating_sub(FULL_INDENT_LEVELS);
    let ceiling = usize::from(width).saturating_sub(LABEL_FLOOR + usize::from(TRAILING_PAD) + 4);
    " ".repeat(natural.min(ceiling))
}

/// The columns left for a row's name after what `spans` already holds,
/// keeping the trailing pad off the divider.
pub(super) fn label_room(spans: &[TextSpan<'_>], width: u16) -> usize {
    let used: usize = spans.iter().map(TextSpan::width).sum();
    usize::from(width)
        .saturating_sub(used + usize::from(TRAILING_PAD))
        .max(1)
}

pub(super) fn push_flat_label(
    spans: &mut Vec<TextSpan<'static>>,
    width: u16,
    name: &str,
    detail: &str,
    label_style: Style,
    marker: &Span,
) {
    let leading: usize = spans.iter().map(TextSpan::width).sum();
    let marker_width = TextSpan::raw(marker.text.as_str()).width();
    let room = usize::from(width)
        .saturating_sub(leading + marker_width + usize::from(TRAILING_PAD) + 1)
        .max(1);
    let name = text::elide(name, room);
    let left = room.saturating_sub(TextSpan::raw(name.as_str()).width());
    spans.push(TextSpan::styled(name, label_style));
    if !detail.is_empty() && left > 1 {
        spans.push(TextSpan::styled(
            format!(" {}", text::elide(detail, left - 1)),
            theme::fg(Token::TextMuted),
        ));
    }
    let hue = styled(marker)
        .style
        .fg
        .unwrap_or_else(|| color(marker.role));
    row::push_trailing(spans, width, marker.text.clone(), hue);
}
