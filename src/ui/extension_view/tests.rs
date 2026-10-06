use super::*;
use ratatui::{Terminal, backend::TestBackend};
use uze_extensions::view::{ContentLine, LineTone, Rgb, RowMenu};

/// Rendered Markdown as the rows `prose_rows` folds it to, in plain
/// text.
fn prose_at(markdown: &str, width: usize) -> Vec<String> {
    uze_extensions::code::markdown(markdown, "base16-ocean.dark", width)
        .iter()
        .flat_map(|line| prose_rows(line, width))
        .map(|row| row.spans.iter().map(|span| span.content.as_ref()).collect())
        .collect()
}

/// Prose breaks between words, never inside one, and a list item's
/// continuation sits under the item's text rather than under its
/// bullet.
#[test]
fn prose_breaks_between_words_and_hangs_under_the_item() {
    let rows = prose_at(
        "A test skill for checking how the drawer renders.\n\n\
             - You are validating the preview with a bullet that wraps.\n",
        24,
    );
    assert_eq!(
        rows,
        vec![
            "A test skill for",
            "checking how the drawer",
            "renders.",
            "",
            "• You are validating the",
            "  preview with a bullet",
            "  that wraps.",
            "",
        ]
    );
}

/// A span boundary is a change of style, not a place to break: the full
/// stop after a bold word stays on the word's row.
#[test]
fn prose_never_breaks_where_markup_meets_punctuation() {
    let rows = prose_at("Every file is one **artifact**. The rest wraps.\n", 25);
    assert!(
        rows.iter().any(|row| row.contains("artifact.")),
        "{rows:#?}"
    );
    assert!(!rows.iter().any(|row| row.starts_with('.')), "{rows:#?}");
}

/// A quote stays quoted on every row it takes, and a code line carries
/// on clear of its own indentation.
#[test]
fn a_quote_keeps_its_bar_and_code_hangs_past_its_indent() {
    let quote = prose_at("> Never commit without reading the diff.\n", 20);
    assert!(
        quote
            .iter()
            .filter(|row| !row.is_empty())
            .all(|row| row.starts_with("│ ")),
        "{quote:#?}"
    );

    let code = prose_at("```bash\ngit commit -m feat-preview-resources\n```\n", 20);
    assert_eq!(code[0], "  git commit -m");
    assert_eq!(code[1], "    feat-preview-res");
}

/// A word longer than a whole row is broken where the row ends, since
/// no row would ever hold it.
#[test]
fn a_word_longer_than_a_row_is_broken_at_the_cell() {
    let rows = prose_at("See https://example.com/a/very/long/path now.\n", 16);
    assert!(
        rows.iter().all(|row| row.chars().count() <= 16),
        "{rows:#?}"
    );
    assert_eq!(rows[0], "See");
    assert!(rows[1].starts_with("https://"));
}

#[test]
fn a_choice_list_on_a_board_too_narrow_for_it_draws_without_panicking() {
    let mut terminal = Terminal::new(TestBackend::new(8, 8)).unwrap();
    let rows = [ChoiceRow {
        hit: ViewHit::GrabNavigatorEdge,
        name: "overview",
        trailing: String::new(),
        highlighted: true,
    }];
    for width in [0, 1, 2] {
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render_choice_list(
                    frame,
                    Rect::new(0, 0, width, 1),
                    Rect::new(0, 0, width, 6),
                    &rows,
                    ViewHit::GrabNavigatorEdge,
                    &mut hits,
                );
            })
            .unwrap();
    }
}

/// A caption that fits is drawn where it always was; one that does
/// not passes through the room it has, a column at a time, and comes
/// round again — and says so, which is what keeps the clock turning
/// for it.
///
/// A branch name is the caption with no natural length, and it is
/// read from both ends: the end is exactly the half an "…" eats.
#[test]
fn a_caption_too_long_for_its_room_passes_through_it() {
    let header = |caption: &str, tick: Option<usize>| {
        let section = Section {
            title: "timeline".to_owned(),
            caption: Span::new(caption, Role::Muted),
            collapsed: true,
            resizable: false,
            scroll: 0,
            rows: Vec::new(),
        };
        let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
        let mut hits = Vec::new();
        let mut slid = false;
        terminal
            .draw(|frame| {
                let mut rows = crate::ui::Rows::over(frame.area());
                slid = render_section_with(frame, &section, &mut rows, false, tick, &[], &mut hits);
            })
            .unwrap();
        let row: String = (0..40)
            .map(|column| terminal.backend().buffer()[(column, 0)].symbol())
            .collect();
        (slid, row)
    };

    let short = "main";
    let (slid, row) = header(short, Some(0));
    assert!(!slid, "a caption that fits does not move: {row:?}");
    assert!(row.contains(short), "{row:?}");

    // Longer than the room the title leaves it.
    let long = "refactor/a-branch-nobody-shortened";
    let (slid, at_rest) = header(long, None);
    assert!(!slid, "and nothing moves unasked: {at_rest:?}");

    let (slid, first) = header(long, Some(0));
    assert!(slid, "asked, it slides: {first:?}");
    // It holds its start for a rest of ten beats, so a pointer crossing
    // the header on its way elsewhere sees nothing move.
    let (_, resting) = header(long, Some(9));
    assert_eq!(first, resting, "still resting: {resting:?}");
    // Then two beats a column, so the clock the spinners turn on does not
    // read as a flicker here.
    let (_, same) = header(long, Some(11));
    assert_eq!(first, same, "a column every other beat: {same:?}");
    let (_, moved) = header(long, Some(12));
    assert_ne!(first, moved, "and then it has moved: {moved:?}");

    // It comes round rather than jumping back: one full cycle of the
    // run, its rest included, lands on what it started from.
    let cycle = long.chars().count() + 3;
    let (_, round) = header(long, Some(10 + cycle * 2));
    assert_eq!(first, round, "one cycle returns it: {round:?}");
}

/// The row under the pointer has its name lifted to the bright text,
/// on the ground it already had, and no other row does.
#[test]
fn the_row_under_the_pointer_lifts_its_name() {
    let commit = |name: &str| uze_extensions::view::SectionRow {
        mark: uze_extensions::view::RowMark::Commit,
        mark_role: Role::Muted,
        name: Span::new(name, Role::Dim),
        trailing: Span::new("8m", Role::Muted),
    };
    let section = Section {
        title: "timeline".to_owned(),
        caption: Span::new("main", Role::Muted),
        collapsed: false,
        resizable: false,
        scroll: 0,
        rows: vec![commit("fix(ui): one"), commit("feat(cli): two")],
    };
    let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
    terminal
        .draw(|frame| {
            let mut rows = crate::ui::Rows::over(frame.area());
            render_section_with(
                frame,
                &section,
                &mut rows,
                false,
                None,
                &[1],
                &mut Vec::new(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let bright = theme::color(Token::TextBright);
    // The header, then one row per commit; the name starts past the
    // lead, the mark and its gap.
    let name = TRAILING_PAD + 2;
    assert_ne!(buffer[(name, 1)].fg, bright, "the resting row's name");
    assert_eq!(buffer[(name, 2)].fg, bright, "the hovered row's name");
    assert_eq!(
        buffer[(name, 2)].bg,
        buffer[(name, 1)].bg,
        "on the same ground as its neighbour"
    );
}

/// Which drawn row a board's menu lands on: the first, with no frame
/// above it.
const MENU_ROW: usize = 0;

fn sample() -> View {
    View {
        title: vec![Span::new("demo", Role::Bright)],
        caption: Vec::new(),
        navigator: Some(Navigator {
            heading: "CHANGES".to_owned(),
            badge: "2".to_owned(),
            focused: true,
            anchor: Some(1),
            choosing: None,
            menu: None,
            rows: vec![
                NavigatorRow::Group {
                    id: 0,
                    name: "src/".to_owned(),
                    depth: 0,
                    collapsed: false,
                    icon: RowIcon::Directory,
                },
                NavigatorRow::Item {
                    id: 7,
                    name: "ui.rs".to_owned(),
                    depth: 1,
                    marker: Span::new("M", Role::Warning),
                    marker_side: MarkerSide::Leading,
                    detail: String::new(),
                    selected: true,
                    icon: RowIcon::Code,
                },
            ],
        }),
        content: Content::Lines {
            first: 0,
            caret: None,
            medium: Medium::Text,
            total: 1,
            heading: "DIFF · src/ui.rs".to_owned(),
            scroll: 0,
            lines: vec![ContentLine {
                gutter: "+".to_owned(),
                number: "12".to_owned(),
                tone: LineTone::Added,
                spans: vec![Span {
                    text: "let x = 1;".to_owned(),
                    role: Role::Default,
                    color: Some(Rgb(1, 2, 3)),
                    ground: None,
                    bold: false,
                    italic: false,
                }],
            }],
        },
        footer: vec![Command::Close],
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: ViewLayout::Sidebar,
        trail: Vec::new(),
    }
}

/// A band is told apart from the tree by form — its name in capitals
/// with its count beside it, in glyphs every font has — and the air
/// before the next one is drawn blank and answers nothing.
#[test]
fn a_band_heading_is_set_apart_and_the_gap_before_it_is_air() {
    let mut view = sample();
    let navigator = view.navigator.as_mut().expect("the sample has a navigator");
    navigator.anchor = None;
    navigator.rows = vec![
        NavigatorRow::Band {
            id: 0,
            name: "in progress".to_owned(),
            count: 2,
            collapsed: false,
        },
        NavigatorRow::Item {
            id: 1,
            name: "a-change".to_owned(),
            depth: 1,
            marker: Span::new("1/2", Role::Muted),
            marker_side: MarkerSide::Trailing,
            detail: String::new(),
            selected: false,
            icon: RowIcon::None,
        },
        NavigatorRow::Gap,
        NavigatorRow::Band {
            id: 3,
            name: "ready to archive".to_owned(),
            count: 1,
            collapsed: true,
        },
    ];
    let (rows, hits) = draw(&view);

    let band = rows
        .iter()
        .position(|row| row.contains("IN PROGRESS 2"))
        .expect("the band's name is drawn in capitals, with its count");
    let gap = band + 2;
    assert!(
        rows[gap].chars().take(20).all(|glyph| glyph == ' '),
        "the gap is blank: {:?}",
        rows[gap]
    );
    assert!(rows[gap + 1].contains("READY TO ARCHIVE 1"));

    let answering: Vec<ViewHit> = hits
        .iter()
        .filter(|(rect, _)| usize::from(rect.y) == gap && rect.x < 20)
        .map(|(_, hit)| *hit)
        .collect();
    assert!(
        answering.is_empty(),
        "the gap answers nothing: {answering:?}"
    );
    assert!(
        hits.iter()
            .any(|(rect, hit)| usize::from(rect.y) == band && *hit == ViewHit::ToggleGroup(0)),
        "a band folds from its heading"
    );
}

fn draw(view: &View) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
    framed(view, 90, 14, uze_keys::Scope::Code)
}

/// The surface the way the workspace draws it: the bar's leading slot on
/// the first row, holding the surface's navigation, and the surface under
/// it — drawn first, so a selector's list opens over it.
fn framed(
    view: &View,
    width: u16,
    height: u16,
    scope: uze_keys::Scope,
) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            let area = frame.area();
            let slot = Rect::new(area.x, area.y, area.width, 1);
            let surface = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
            render(
                frame,
                view,
                surface,
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                scope,
                None,
                &mut hits,
            );
            let mut navigation = Vec::new();
            render_navigation(frame, view, slot, surface, &mut navigation);
            hits.splice(0..0, navigation);
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    (rows, hits)
}

/// With no list there is no list's column: the message is centred on
/// the whole width, the keys start at its left edge, and there is no
/// edge to drag.
#[test]
fn a_view_with_no_navigator_takes_the_whole_width() {
    let view = View {
        navigator: None,
        content: Content::Message {
            text: "Nothing here".to_owned(),
            hint: None,
            role: Role::Muted,
        },
        ..sample()
    };
    let (rows, hits) = draw(&view);

    let message = rows
        .iter()
        .find(|row: &&String| row.contains("Nothing here"))
        .expect("the message is drawn");
    let left = message.find("Nothing here").unwrap();
    let right = message.len() - left - "Nothing here".len();
    assert!(
        left.abs_diff(right) <= 1,
        "centred on 90 columns: {message:?}"
    );
    assert!(
        rows.last().unwrap().starts_with("esc"),
        "the keys start at the frame's edge: {:?}",
        rows.last()
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, ViewHit::GrabNavigatorEdge)),
        "no column, no edge"
    );
}

/// What a hint quotes is drawn apart, without its backticks — and stays
/// apart on the next row when a fold split the quote.
#[test]
fn a_hint_draws_what_it_quotes_apart() {
    let mut quoted = false;
    let first = hint_line("Run `uze agent", &mut quoted);
    let second = hint_line("artifacts check` now", &mut quoted);
    let text = |line: &Line<'_>| {
        line.spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>()
    };
    assert_eq!(text(&first), "Run uze agent");
    assert_eq!(text(&second), "artifacts check now");
    assert_eq!(first.spans[1].style, theme::fg(Token::TextPrimary));
    assert_eq!(second.spans[0].style, theme::fg(Token::TextPrimary));
    assert_eq!(second.spans[1].style, theme::fg(Token::TextMuted));
    assert!(!quoted, "a closed quote leaves nothing open");
}

/// A hint's own lines are kept, and kept as written where they fit:
/// a list lined up in columns stays lined up.
#[test]
fn a_hint_keeps_its_lines_and_their_spacing() {
    let rows = hint_rows(
        "Pick one:\n\nOpenSpec   openspec/\nSpec Kit   .specify/",
        48,
    );
    assert_eq!(
        rows,
        [
            "Pick one:",
            "",
            "OpenSpec   openspec/",
            "Spec Kit   .specify/"
        ]
    );
    assert_eq!(
        hint_rows("one two three", 7),
        ["one two", "three"],
        "a line wider than the measure still folds"
    );
}

/// A refused gesture says so on the footer's row, in the ink its role
/// names, beside the keys rather than over them: otherwise a key that
/// was refused is a key that seemed to do nothing.
#[test]
fn a_notice_is_said_at_the_end_of_the_footer() {
    let view = View {
        notice: Some(Span::new("a.txt has unsaved changes", Role::Warning)),
        ..sample()
    };
    let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
    terminal
        .draw(|frame| {
            render(
                frame,
                &view,
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                None,
                &mut Vec::new(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let footer = buffer.area.height - 1;
    let row: String = (0..buffer.area.width)
        .map(|column| buffer[(column, footer)].symbol())
        .collect();
    assert!(
        row.trim_end().ends_with("a.txt has unsaved changes"),
        "{row:?}"
    );
    let at = row.find("a.txt").expect("the notice is on the row") as u16;
    assert_eq!(buffer[(at, footer)].fg, theme::color(Token::StateWarning));
    let (without, _) = draw(&sample());
    let hints = without[usize::from(footer)].trim_end();
    assert!(
        row.starts_with(hints),
        "the keys stay where they were: {row:?} against {hints:?}"
    );
}

/// The control that says where you are wears the same neutral lift
/// the board's own selector does — never the accent tint, which is
/// what a *list* marks its selection with. One extension, two
/// surfaces, one vocabulary.
#[test]
fn the_control_wears_the_lift_and_the_list_wears_the_tint() {
    let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
    terminal
        .draw(|frame| {
            render_chips(
                frame,
                0,
                0,
                &[
                    Mode {
                        label: "Files".to_owned(),
                        active: true,
                        icon: RowIcon::None,
                    },
                    Mode {
                        label: "Map".to_owned(),
                        active: false,
                        icon: RowIcon::None,
                    },
                ],
                &mut Vec::new(),
                ViewHit::SelectSubject,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    assert_eq!(
        buffer[(1, 0)].bg,
        theme::color(Token::SurfaceRaised),
        "the member you are on is lifted, not tinted"
    );
    assert_ne!(
        theme::color(Token::SurfaceRaised),
        theme::color(Token::SurfaceSelected),
        "and the two are genuinely different grounds"
    );
    assert_eq!(
        buffer[(9, 0)].bg,
        theme::color(Token::SurfaceBackground),
        "a member you are not on has no ground at all"
    );
}

/// A flat row reads name, then where it sits, quieter, then its status
/// at the right edge — and in a narrow column the place gives way
/// before the name does.
#[test]
fn a_flat_row_pins_its_marker_right_and_gives_up_the_detail_first() {
    let marker = Span::new("M", Role::Warning);
    let drawn = |width: u16| {
        let mut spans = vec![TextSpan::raw(" ")];
        push_flat_label(
            &mut spans,
            width,
            "ui.rs",
            "src/components/network",
            Style::default(),
            &marker,
        );
        spans
    };

    let wide = drawn(40);
    let text: String = wide.iter().map(|span| span.content.as_ref()).collect();
    assert!(
        text.starts_with(" ui.rs src/components/network"),
        "{text:?}"
    );
    assert!(text.ends_with(&format!("M{}", " ".repeat(TRAILING_PAD.into()))));
    assert_eq!(spans_width(&wide), 40, "the marker is pinned to the edge");
    let detail = wide
        .iter()
        .find(|span| span.content.contains("src/"))
        .expect("the place is drawn");
    assert_eq!(detail.style.fg, Some(theme::color(Token::TextMuted)));

    let narrow = drawn(12);
    let text: String = narrow.iter().map(|span| span.content.as_ref()).collect();
    assert!(text.contains("ui.rs") && !text.contains("src"), "{text:?}");
    assert_eq!(spans_width(&narrow), 12);
}

/// The header is one row with a question at each end: which half you
/// are in, and how the half you are in is drawn.
///
/// The row is the surface's own — it starts at the pane's edge and
/// spans both columns — so switching halves, which
/// switches the layout under it, leaves every control on it exactly
/// where it was.
#[test]
fn the_navigation_is_the_bars_and_does_not_move_with_the_layout() {
    let sidebar = View {
        title: vec![Span::new("code", Role::Muted)],
        caption: Vec::new(),
        navigator: Some(Navigator {
            heading: "FILES".to_owned(),
            badge: "7".to_owned(),
            focused: true,
            rows: vec![NavigatorRow::Item {
                id: 0,
                name: "main.rs".to_owned(),
                depth: 0,
                marker: Span::new("", Role::Muted),
                marker_side: MarkerSide::Leading,
                detail: String::new(),
                selected: true,
                icon: RowIcon::None,
            }],
            anchor: None,
            choosing: None,
            menu: None,
        }),
        content: Content::Lines {
            heading: "main.rs".to_owned(),
            scroll: 0,
            first: 0,
            total: 1,
            lines: vec![ContentLine {
                gutter: " ".to_owned(),
                number: "1".to_owned(),
                tone: LineTone::Neutral,
                spans: vec![Span::new("fn main() {}", Role::Default)],
            }],
            caret: None,
            medium: Medium::Text,
        },
        footer: Vec::new(),
        notice: None,
        confirm: None,
        modes: vec![
            Mode {
                label: "Preview".to_owned(),
                active: true,
                icon: RowIcon::None,
            },
            Mode {
                label: "Source".to_owned(),
                active: false,
                icon: RowIcon::None,
            },
        ],
        subjects: vec![
            Mode {
                label: "Files".to_owned(),
                active: true,
                icon: RowIcon::None,
            },
            Mode {
                label: "Map".to_owned(),
                active: false,
                icon: RowIcon::None,
            },
            Mode {
                label: "Changes".to_owned(),
                active: false,
                icon: RowIcon::None,
            },
        ],
        layout: ViewLayout::Sidebar,
        trail: Vec::new(),
    };
    let (rows, hits) = draw_sized(&sidebar, 80, 12);
    assert!(
        rows[0].contains("files") && rows[0].contains("changes"),
        "the halves are the bar's, in its lower case: {:?}",
        rows[0]
    );
    assert!(
        !rows[0].contains("Preview"),
        "and only the halves: how to draw one stays in the surface: {:?}",
        rows[0]
    );
    assert!(
        rows[1].contains("main.rs") && rows[1].contains("Preview"),
        "the modes ride the content's own heading row: {:?}",
        rows[1]
    );
    let nav_at = |hits: &[(Rect, ViewHit)]| {
        hits.iter()
            .find(|(_, hit)| matches!(hit, ViewHit::SelectSubject(0)))
            .map(|(rect, _)| (rect.x, rect.y))
            .expect("the halves can be pointed at")
    };
    let sidebar_at = nav_at(&hits);
    assert_eq!(sidebar_at, (0, 0), "flush with the slot's corner");

    // The map takes the frame, which is the switch that used to move
    // the control a column sideways.
    let board = View {
        navigator: None,
        layout: ViewLayout::Board,
        modes: vec![Mode {
            label: "Map".to_owned(),
            active: true,
            icon: RowIcon::None,
        }],
        ..sidebar
    };
    let (rows, hits) = draw_sized(&board, 80, 12);
    assert!(rows[0].contains("files"), "the same row: {:?}", rows[0]);
    assert_eq!(nav_at(&hits), sidebar_at, "at the same cell");
}

/// A row's menu is drawn under that row, over whatever it covers, and
/// its entries are the first thing a click there lands on.
#[test]
fn a_row_menu_lies_under_its_row_and_takes_the_click() {
    let row = |id: usize, name: &str| NavigatorRow::Item {
        id,
        name: name.to_owned(),
        depth: 0,
        marker: Span::new("M", Role::Warning),
        marker_side: MarkerSide::Trailing,
        detail: String::new(),
        selected: id == 1,
        icon: RowIcon::None,
    };
    let view = View {
        title: vec![Span::new("code", Role::Muted)],
        caption: Vec::new(),
        navigator: Some(Navigator {
            heading: "CHANGES".to_owned(),
            badge: "3".to_owned(),
            focused: true,
            rows: vec![row(0, "a.rs"), row(1, "b.rs"), row(2, "c.rs")],
            anchor: None,
            choosing: None,
            menu: Some(RowMenu {
                row: 1,
                entries: vec!["Open file".to_owned(), "Copy path".to_owned()],
                highlighted: 0,
            }),
        }),
        content: Content::Message {
            text: String::new(),
            hint: None,
            role: Role::Muted,
        },
        footer: Vec::new(),
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: ViewLayout::Sidebar,
        trail: Vec::new(),
    };
    let (rows, hits) = draw_with_menu(&view, None);

    let at = |wanted: ViewHit| {
        hits.iter()
            .find(|(_, hit)| *hit == wanted)
            .map(|(rect, _)| *rect)
            .expect("drawn")
    };
    let (opened_on, first) = (at(ViewHit::SelectItem(1)), at(ViewHit::MenuEntry(0)));
    assert!(first.y > opened_on.y, "under the row it was opened on");
    assert!(rows[first.y as usize].contains("Open file"), "{rows:?}");
    let over = hits
        .iter()
        .find(|(rect, _)| rect.contains(ratatui::layout::Position::new(first.x, first.y)))
        .map(|(_, hit)| *hit);
    assert_eq!(
        over,
        Some(ViewHit::MenuEntry(0)),
        "the entry, not the row under it"
    );

    // Asked for by the pointer, it opens where the pointer is.
    let pointer = Rect::new(30, 4, 1, 1);
    let (_, hits) = draw_with_menu(&view, Some(pointer));
    let first = hits
        .iter()
        .find(|(_, hit)| *hit == ViewHit::MenuEntry(0))
        .map(|(rect, _)| *rect)
        .expect("drawn");
    assert!(
        first.y > pointer.y && first.x.abs_diff(pointer.x) <= 2,
        "at the pointer: {first:?}"
    );
}

/// A surface's question is the product's dialog: its title, subject,
/// what agreeing does, and the two answers as buttons.
#[test]
fn a_surfaces_question_is_drawn_as_the_products_dialog() {
    let view = View {
        title: Vec::new(),
        caption: Vec::new(),
        navigator: None,
        content: Content::Message {
            text: String::new(),
            hint: None,
            role: Role::Muted,
        },
        footer: Vec::new(),
        notice: None,
        confirm: Some(uze_extensions::view::Confirm {
            title: "Discard changes".to_owned(),
            subject: "src/ui.rs".to_owned(),
            body: "Puts the file back.".to_owned(),
            confirm: "Discard".to_owned(),
            on_confirm: false,
            field: None,
        }),
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: ViewLayout::Sidebar,
        trail: Vec::new(),
    };
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    let mut answers = Vec::new();
    terminal
        .draw(|frame| {
            render_confirm(
                frame,
                view.confirm.as_ref().expect("asked"),
                frame.area(),
                uze_keys::Scope::Code,
                &mut answers,
            )
        })
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for words in [
        "Discard changes",
        "src/ui.rs",
        "Puts the file back.",
        "Cancel",
        "Discard",
    ] {
        assert!(screen.contains(words), "{words:?} is on the dialog");
    }
    assert_eq!(
        answers.iter().map(|(_, hit)| *hit).collect::<Vec<_>>(),
        [ViewHit::Answer(false), ViewHit::Answer(true)],
        "the way out before the affirmative"
    );
}

fn draw_with_menu(view: &View, at: Option<Rect>) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
    let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(
                frame,
                view,
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                None,
                &mut hits,
            );
            render_row_menu(frame, view, frame.area(), at, &mut hits);
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    (rows, hits)
}

/// A board's footer carries two things — the keys on its left and
/// what is drawn on its right — and they are written into one row.
/// Narrow enough and they used to meet in the middle, which reads as
/// neither: `g ren3 boxes · 2 edges`.
#[test]
fn a_boards_hints_stop_before_its_caption() {
    let view = View {
        title: vec![Span::new("architect", Role::Muted)],
        caption: Vec::new(),
        navigator: None,
        content: Content::Lines {
            first: 0,
            caret: None,
            medium: Medium::Text,
            total: 1,
            heading: "3 boxes · 2 edges · containers.mmd".to_owned(),
            scroll: 0,
            lines: Vec::new(),
        },
        footer: vec![
            Command::Close,
            Command::ChooseItem,
            Command::NextView,
            Command::NextMode,
        ],
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: ViewLayout::Board,
        trail: Vec::new(),
    };

    for width in [70, 90, 118] {
        let (rows, _) = draw_sized(&view, width, 12);
        let footer = rows
            .iter()
            .find(|row| row.contains("3 boxes"))
            .cloned()
            .unwrap_or_else(|| panic!("the caption is drawn at {width} columns: {rows:?}"));
        // Whatever room is left, the two never touch: the hints end,
        // then blank cells, then the caption.
        let caption = footer
            .find("3 boxes")
            .expect("the caption starts where it starts");
        assert!(
            footer[..caption].ends_with("  "),
            "hints ran into the caption at {width} columns: {footer:?}"
        );
        // And what is kept is whole: a list cut at the edge ends in
        // half a word, which reads as a key nobody can press.
        let hints = footer[..caption].trim_end();
        assert!(
            hints.is_empty() || KEPT_WHOLE.iter().any(|label| hints.ends_with(label)),
            "a hint was cut mid-word at {width} columns: {footer:?}"
        );
    }
}

/// Every label this view's own footer can end on. Ending on anything
/// else is a word the edge took half of.
const KEPT_WHOLE: [&str; 4] = ["close", "artifacts", "artifact", "rendering"];

fn draw_sized(view: &View, width: u16, height: u16) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
    framed(view, width, height, uze_keys::Scope::Architect)
}

/// A click anywhere on a board's drawing resolves to the cell that was
/// drawn there, in the space the frame reports having drawn in.
///
/// Both halves matter, and the second is the one that was wrong. A
/// surface that places things in the room it is given — the map's
/// tiles, the architect's diagrams — answers a click by laying itself
/// out again, so it has to be told the same room twice. The click path
/// used to recompute that from the pane's size instead of the frame's,
/// which is smaller by the sidebar and the tab strip: the right and
/// bottom bands of the drawing belonged to no tile at all, and
/// everything else belonged to the wrong one.
#[test]
fn a_board_click_resolves_in_the_space_the_frame_drew_in() {
    for (width, height) in [(90u16, 24u16), (120, 36), (70, 20)] {
        let area = Rect::new(0, 0, width, height);
        let (_, board_rect, _) = board_rows(area);
        let space = board_space(area);
        let lines: Vec<ContentLine> = (0..space.height)
            .map(|_| ContentLine {
                gutter: String::new(),
                number: String::new(),
                tone: LineTone::Neutral,
                spans: vec![Span::new("x".repeat(space.width as usize), Role::Default)],
            })
            .collect();
        let view = View {
            title: vec![Span::new("Map", Role::Bright)],
            caption: Vec::new(),
            navigator: None,
            content: Content::Lines {
                first: 0,
                heading: String::new(),
                scroll: 0,
                total: lines.len(),
                lines,
                caret: None,
                medium: Medium::Text,
            },
            footer: vec![Command::Close],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Board,
            trail: Vec::new(),
        };

        let mut hits = Vec::new();
        let mut reported = uze_extensions::view::Size::default();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                reported = render(
                    frame,
                    &view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                        sliding: None,
                    },
                    uze_keys::Scope::Architect,
                    None,
                    &mut hits,
                )
                .content_space;
            })
            .unwrap();

        assert_eq!(
            reported, space,
            "{width}x{height}: the frame must report the board it drew in"
        );

        for row in board_rect.y..board_rect.bottom() {
            for column in board_rect.x..board_rect.right() {
                let (rect, hit) = hits
                    .iter()
                    .find(|(rect, hit)| {
                        matches!(hit, ViewHit::PlaceCaret { .. })
                            && rect.x <= column
                            && column < rect.right()
                            && rect.y <= row
                            && row < rect.bottom()
                    })
                    .expect("every drawn cell of a board is clickable");
                let ViewHit::PlaceCaret { line, cell } = hit else {
                    unreachable!("filtered to PlaceCaret");
                };
                let resolved = caret_cell_at(*rect, *cell, column, 0);
                assert_eq!(
                    (*line, resolved),
                    (
                        usize::from(row - board_rect.y),
                        usize::from(column - board_rect.x)
                    ),
                    "{width}x{height}: cell ({column}, {row})"
                );
                assert!(
                    *line < usize::from(reported.height) && resolved < usize::from(reported.width),
                    "{width}x{height}: ({column}, {row}) resolved outside the \
                         space the surface was laid out in"
                );
            }
        }
    }
}

/// A board menu of `groups` areas, each holding `items` artifacts,
/// offered as `trail` — empty for a list to pick from, or a step per
/// level with the one being looked at marked.
fn board(groups: usize, items: usize, trail: &[(&str, bool)]) -> View {
    let mut rows = Vec::new();
    for group in 0..groups {
        rows.push(NavigatorRow::Group {
            id: group * items,
            name: format!("Area {group}"),
            depth: 0,
            collapsed: false,
            icon: RowIcon::None,
        });
        rows.extend((0..items).map(|item| NavigatorRow::Item {
            id: group * items + item,
            name: format!("Artifact {group}{item}"),
            depth: 1,
            marker: Span::default(),
            marker_side: MarkerSide::Leading,
            detail: String::new(),
            selected: group == 0 && item == 0,
            icon: RowIcon::None,
        }));
    }
    View {
        title: vec![Span::new("Board", Role::Bright)],
        caption: Vec::new(),
        navigator: Some(Navigator {
            heading: String::new(),
            badge: String::new(),
            focused: false,
            anchor: None,
            choosing: None,
            menu: None,
            rows,
        }),
        content: Content::Lines {
            first: 0,
            heading: String::new(),
            scroll: 0,
            lines: Vec::new(),
            total: 0,
            caret: None,
            medium: Medium::Text,
        },
        footer: vec![Command::Close],
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: ViewLayout::Board,
        trail: trail
            .iter()
            .map(|&(name, current)| TrailStep::new(name, current))
            .collect(),
    }
}

/// A selector that opens onto nothing but what is already on show is
/// not a control: no mark that says it opens, and no press that does.
#[test]
fn a_selector_with_one_choice_neither_opens_nor_says_it_does() {
    let chevron = theme::glyph(Symbol::ChevronExpanded);
    let (drawn, hits) = draw_sized(&board(1, 1, &[]), 80, 20);
    let menu = drawn[MENU_ROW].clone();
    assert!(!menu.contains(&chevron), "no mark on either: {menu}");
    assert!(!menu.contains(" 1 "), "nor a count of one: {menu}");
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, ViewHit::ChooseGroup | ViewHit::ChooseItem)),
        "and neither can be pressed"
    );

    let (drawn, hits) = draw_sized(&board(2, 3, &[]), 80, 20);
    assert_eq!(
        drawn[MENU_ROW].matches(chevron.as_str()).count(),
        2,
        "both open where there is a choice: {}",
        drawn[MENU_ROW]
    );
    assert!(
        hits.iter().any(|(_, h)| *h == ViewHit::ChooseGroup)
            && hits.iter().any(|(_, h)| *h == ViewHit::ChooseItem)
    );
}

/// The second half of the menu is one control or the other, never
/// both: an area that descends is walked, and one that does not is
/// picked from. Which it is, is the view's answer, not the host's.
#[test]
fn an_area_that_descends_is_walked_and_one_that_does_not_is_picked_from() {
    let chevron = theme::glyph(Symbol::ChevronExpanded);
    let (drawn, hits) = draw_sized(&board(2, 3, &[]), 100, 20);
    assert_eq!(
        drawn[MENU_ROW].matches(chevron.as_str()).count(),
        2,
        "a set is two lists: {}",
        drawn[MENU_ROW]
    );
    assert!(hits.iter().any(|(_, h)| *h == ViewHit::ChooseItem));

    // Standing on the middle level of three: the one behind is the
    // way back, the one ahead is a level this descent reaches.
    let ladder = board(
        2,
        3,
        &[
            ("Context", false),
            ("Containers", true),
            ("Components", false),
        ],
    );
    let (drawn, hits) = draw_sized(&ladder, 100, 20);
    let menu = drawn[MENU_ROW].clone();
    assert!(
        menu.contains("Context") && menu.contains("Containers") && menu.contains("Components"),
        "every level is on show at once: {menu}"
    );
    assert_eq!(
        menu.matches(chevron.as_str()).count(),
        1,
        "and only the area is still a list: {menu}"
    );
    assert!(
        !hits.iter().any(|(_, h)| *h == ViewHit::ChooseItem),
        "a descent is walked, not opened"
    );
    assert!(
        hits.iter().any(|(_, h)| *h == ViewHit::SelectTrail(0))
            && hits.iter().any(|(_, h)| *h == ViewHit::SelectTrail(2)),
        "both directions are a place to go: {hits:?}"
    );
    assert!(
        !hits.iter().any(|(_, h)| *h == ViewHit::SelectTrail(1)),
        "except where the viewer already is"
    );
}

/// A descent too long for the row is cut around the step the viewer
/// is on — the one step that must never be the one cut off.
#[test]
fn a_descent_wider_than_the_row_keeps_the_step_it_is_standing_on() {
    let steps: Vec<(String, bool)> = (0..12)
        .map(|level| (format!("Level number {level}"), level == 7))
        .collect();
    let borrowed: Vec<(&str, bool)> = steps
        .iter()
        .map(|(name, current)| (name.as_str(), *current))
        .collect();
    let (drawn, _) = draw_sized(&board(2, 3, &borrowed), 70, 20);
    let menu = drawn[MENU_ROW].clone();
    assert!(menu.contains("Level number 7"), "{menu}");
    assert!(
        menu.contains(&theme::glyph(Symbol::Ellipsis)),
        "cut: {menu}"
    );
}

/// A group with more items than the board has rows: the list shows the
/// rows around the highlighted one and says where in the list that is,
/// so its last row is not mistaken for the last there is.
#[test]
fn a_list_longer_than_the_board_keeps_the_highlight_in_view_and_says_where_it_is() {
    let mut rows = vec![NavigatorRow::Group {
        id: 0,
        name: "Flowchart".to_owned(),
        depth: 0,
        collapsed: false,
        icon: RowIcon::None,
    }];
    rows.extend((0..30).map(|id| NavigatorRow::Item {
        id,
        name: format!("Flow {id:02}"),
        depth: 1,
        marker: Span::default(),
        marker_side: MarkerSide::Leading,
        detail: String::new(),
        selected: id == 0,
        icon: RowIcon::None,
    }));
    let view = View {
        title: vec![Span::new("Board", Role::Bright)],
        caption: Vec::new(),
        navigator: Some(Navigator {
            heading: String::new(),
            badge: String::new(),
            focused: false,
            anchor: None,
            choosing: Some(Choosing::Item(20)),
            menu: None,
            rows,
        }),
        content: Content::Lines {
            first: 0,
            heading: String::new(),
            scroll: 0,
            lines: Vec::new(),
            total: 0,
            caret: None,
            medium: Medium::Text,
        },
        footer: vec![Command::Close],
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: ViewLayout::Board,
        trail: Vec::new(),
    };
    let (drawn, hits) = draw_sized(&view, 80, 20);
    let text = drawn.join("\n");
    assert!(text.contains("Flowchart 30"), "{text}");
    assert!(
        text.contains("Flow 20") && !text.contains("Flow 02"),
        "{text}"
    );
    assert!(text.contains("21/30"), "{text}");
    let first_hit = hits
        .iter()
        .find(|(_, hit)| matches!(hit, ViewHit::SelectItem(_)))
        .map(|(_, hit)| *hit);
    assert_ne!(
        first_hit,
        Some(ViewHit::SelectItem(0)),
        "the rows cut off are not clickable"
    );
}

/// A board is drawn as the extension cut it: the screen it was told
/// about is the screen it gets, so no row is folded and the last
/// column is still the drawing's. Then the click: the row and column
/// the host resolves must land in the box drawn there.
#[test]
fn a_board_is_drawn_as_it_was_cut_and_a_click_lands_in_the_box_under_it() {
    use uze_extensions::architect;
    let (width, height) = (150, 45);
    let space = board_space(Rect::new(0, 0, width, height));
    let mut state = architect::ArchitectView::opening("~/project".to_owned());
    let artifacts: Vec<architect::Artifact> = [
        (
            "containers.mmd",
            include_str!("../../../docs/architecture/containers.mmd"),
        ),
        (
            "system-context.mmd",
            include_str!("../../../docs/architecture/system-context.mmd"),
        ),
        (
            "install-sequence.mmd",
            include_str!("../../../docs/architecture/install-sequence.mmd"),
        ),
        (
            "crate-layering.mmd",
            include_str!("../../../docs/architecture/crate-layering.mmd"),
        ),
        (
            "install-pipeline.mmd",
            include_str!("../../../docs/architecture/install-pipeline.mmd"),
        ),
    ]
    .map(|(origin, source)| architect::Artifact::read(origin, source))
    .into();
    state.absorb(architect::ArtifactsAnswer {
        branch: "main".to_owned(),
        artifacts: architect::Artifacts::Found {
            artifacts,
            project: std::path::PathBuf::from("/project"),
        },
    });
    // The catalog opens on the outermost view; the box this clicks is a
    // container, one level in.
    architect::handle_mouse(&mut state, Some(ViewHit::SelectItem(1)), space);
    let (rows, hits) = draw_sized(&architect::view(&state, space), width, height);
    if std::env::var_os("UZE_SHOW_BOARD").is_some() {
        println!("{}", rows.join("\n"));
    }
    assert!(
        rows[MENU_ROW].contains("C4")
            && !rows[MENU_ROW].contains("C4 2")
            && rows[MENU_ROW].contains("System context")
            && rows[MENU_ROW].contains("Containers"),
        "one row: the area on show, then its levels, the one on show among them: {}",
        rows[MENU_ROW]
    );
    assert!(
        !rows[MENU_ROW].contains("Sequence") && !rows[MENU_ROW].contains("Install"),
        "and nothing of any other area: {}",
        rows[MENU_ROW]
    );
    let footer = rows[usize::from(height) - 1].as_str();
    assert!(
        footer.contains("o artifacts")
            && footer.contains("tab next artifact")
            && footer.contains("containers.mmd"),
        "the footer names the board's own keys: {footer}"
    );

    let title_row = rows
        .iter()
        .position(|row| row.contains("Workspace TUI"))
        .expect("the box is drawn") as u16;
    let drawn = &rows[usize::from(title_row)];
    let title_column = drawn[..drawn.find("Workspace TUI").unwrap()]
        .chars()
        .count() as u16;
    let (rect, hit) = hits
        .iter()
        .find(|(rect, hit)| {
            matches!(hit, ViewHit::PlaceCaret { .. })
                && rect.y == title_row
                && rect.x <= title_column
                && title_column < rect.x + rect.width
        })
        .expect("the row is a click target");
    let ViewHit::PlaceCaret { line, cell } = *hit else {
        unreachable!()
    };
    architect::handle_mouse(
        &mut state,
        Some(ViewHit::PlaceCaret {
            line,
            cell: cell + usize::from(title_column - rect.x),
        }),
        space,
    );
    let Content::Lines { heading, .. } = architect::view(&state, space).content else {
        panic!("a diagram is lines");
    };
    assert!(heading.starts_with("Workspace TUI"), "{heading}");
}

/// The whole point of the contract: an extension names a row, the host
/// decides where it went, so the host is the only side that can answer
/// a click.
#[test]
fn a_click_target_comes_from_what_the_host_drew() {
    let (rows, hits) = draw(&sample());
    // Not just any row mentioning the file: the content heading names
    // it too.
    let item_row = rows
        .iter()
        .position(|row: &String| row.contains("ui.rs") && !row.contains("DIFF"))
        .expect("the item is drawn") as u16;
    let hit = hits
        .iter()
        .find(|(_, hit)| matches!(hit, ViewHit::SelectItem(7)))
        .expect("the item is clickable by the id the extension gave it");
    assert_eq!(
        hit.0.y, item_row,
        "the hit must sit on the row the host actually drew"
    );
    assert!(
        hits.iter()
            .any(|(_, hit)| *hit == ViewHit::GrabNavigatorEdge),
        "the edge is one target for both of its jobs"
    );
}

/// An extension says what a row *is*; the vocabulary says what that
/// looks like. So the icon appears only where the active set draws
/// one, and takes no column where it does not — which is every
/// built-in set but `nerd`, because plain Unicode has no folder mark
/// that is not an emoji.
///
/// Asserted at the seam rather than on a frame drawn under a swapped
/// theme: the active theme is process-wide, and a test that put `nerd`
/// in force to photograph a row would change the glyphs — and with the
/// declared widths, the column positions — under every other test
/// drawing at that moment. Which set declares which glyph is
/// `uze-theme`'s own question, and it answers it in
/// `every_nerd_glyph_is_an_icon_a_patched_font_supplies`.
#[test]
fn a_rows_icon_comes_from_the_set_and_takes_no_column_when_there_is_none() {
    // Every kind names a symbol, so a kind added later cannot quietly
    // draw nothing by falling through.
    for icon in [
        RowIcon::Directory,
        RowIcon::DirectoryOpen,
        RowIcon::File,
        RowIcon::Code,
        RowIcon::Markup,
        RowIcon::Config,
        RowIcon::Lock,
        RowIcon::Data,
        RowIcon::Image,
        RowIcon::Archive,
        RowIcon::Git,
        RowIcon::Legal,
    ] {
        let symbol = icon_symbol(icon).unwrap_or_else(|| panic!("{icon:?} names no symbol"));
        // And the set that has icons draws every one of them, in one
        // cell — resolved here rather than put in force.
        let mut layers = vec![uze_theme::default_file()];
        layers.extend(uze_theme::glyph_set_file("nerd"));
        let nerd = uze_theme::resolve_stack(
            &uze_theme::Identity::from_file("nerd", uze_theme::default_file()),
            &layers,
        )
        .expect("the bundled nerd set resolves")
        .theme;
        let drawn = nerd.symbol(symbol);
        assert!(
            !drawn.glyph().trim().is_empty(),
            "the nerd set draws nothing for {icon:?}"
        );
        assert_eq!(drawn.width(), 1, "{icon:?} is not one cell wide");

        // The default draws none of them, and the row holds no column
        // open for what is not there.
        assert!(
            uze_theme::default_theme().glyph(symbol).trim().is_empty(),
            "the default set grew a {icon:?} glyph — it has no column for one"
        );
    }
    assert!(row_icon(RowIcon::None).is_none());
    assert!(
        row_icon(RowIcon::Code).is_none(),
        "a blank glyph must take no column at all"
    );
}

/// A rendered document is the one content with no gutter, and the
/// gutter is where every other mode's left margin quietly came from.
/// Without a margin of its own, a wrapped paragraph runs into both
/// borders.
#[test]
fn unnumbered_content_is_inset_where_numbered_content_leans_on_its_gutter() {
    let prose = |text: &str| ContentLine {
        gutter: String::new(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: vec![Span {
            text: text.to_owned(),
            role: Role::Default,
            color: None,
            ground: None,
            bold: false,
            italic: false,
        }],
    };

    let mut view = sample();
    let Content::Lines { lines, heading, .. } = &mut view.content else {
        unreachable!("the sample is Lines")
    };
    *lines = vec![prose("PROSE")];
    *heading = "HEADING".to_owned();
    let (rows, _) = draw(&view);

    // Measured against the heading rather than the frame, because the
    // heading is drawn at the content area's own left edge — so the
    // difference is the margin and nothing else. In *columns*: the
    // frame's own rules are multi-byte, so a byte offset is not where
    // the terminal put anything.
    let column_of = |needle: &str| -> usize {
        rows.iter()
            .find_map(|row: &String| row.find(needle).map(|byte| row[..byte].chars().count()))
            .unwrap_or_else(|| panic!("`{needle}` is drawn"))
    };
    assert_eq!(
        column_of("PROSE") - column_of("HEADING"),
        usize::from(PROSE_INSET),
        "prose must be inset from the edge its own heading sits on"
    );

    // And the numbered case is untouched — its gutter is the margin.
    let (numbered, _) = draw(&sample());
    let row = numbered
        .iter()
        .find(|row: &&String| row.contains("let x = 1;"))
        .expect("the code line is drawn");
    assert!(
        row.contains("12"),
        "the gutter still carries the number: {row:?}"
    );
}

/// A scrollbar is drawn only when there is something to scroll, and
/// the column it takes comes out of the content rather than sitting
/// on top of it.
#[test]
fn a_scrollbar_appears_only_when_the_content_outgrows_the_frame() {
    let mut view = sample();
    let Content::Lines { lines, total, .. } = &mut view.content else {
        unreachable!("the sample shows lines");
    };
    *total = lines.len();

    let (_rows, hits) = draw(&view);
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| *hit == ViewHit::DragContentScrollbar),
        "one line in a tall frame has nowhere to scroll to"
    );

    let Content::Lines { total, .. } = &mut view.content else {
        unreachable!("the sample shows lines");
    };
    *total = 500;
    let (_rows, hits) = draw(&view);
    let track = hits
        .iter()
        .find(|(_, hit)| *hit == ViewHit::DragContentScrollbar)
        .expect("five hundred lines in a short frame is a scrollbar")
        .0;
    assert_eq!(track.width, crate::ui::widget::Scrollbar::width());
    assert!(
        track.height > 1,
        "the whole groove is the target, not just the handle"
    );
    // The complaint this answers: a groove a column short of the edge
    // and a divider beside it read as two controls arguing.
    let (_navigator, content, _footer) = content_columns(Rect::new(0, 0, 90, 14), Some(24));
    assert_eq!(
        track.x,
        content.right(),
        "the groove hugs the edge rather than leaving a gap beside it"
    );
}

/// A mode the keyboard can reach has to be one a pointer can reach,
/// drawn where the thing it changes is.
#[test]
fn the_modes_a_surface_offers_are_a_control_on_its_heading_row() {
    let mut view = sample();
    view.modes = vec![
        Mode {
            label: "Preview".to_owned(),
            active: false,
            icon: RowIcon::None,
        },
        Mode {
            label: "Source".to_owned(),
            active: true,
            icon: RowIcon::None,
        },
    ];
    let (rows, hits) = draw(&view);

    let segments: Vec<(Rect, usize)> = hits
        .iter()
        .filter_map(|(rect, hit)| match hit {
            ViewHit::SelectMode(index) => Some((*rect, *index)),
            _ => None,
        })
        .collect();
    assert_eq!(
        segments.len(),
        2,
        "both are offered, not just the other one"
    );
    assert_eq!(segments[0].1, 0);
    assert!(
        segments[0].0.x < segments[1].0.x,
        "in the order the extension gave them"
    );

    let heading_row = rows
        .iter()
        .position(|row: &String| row.contains("DIFF"))
        .expect("the heading is drawn") as u16;
    assert_eq!(
        segments[0].0.y, heading_row,
        "beside the heading of what they change, not in the footer"
    );
    assert!(
        rows[heading_row as usize].contains("Preview")
            && rows[heading_row as usize].contains("Source"),
        "and both labels are legible: {}",
        rows[heading_row as usize]
    );
}

/// A surface with one way of showing itself offers no choice, and the
/// heading gets the whole row back.
#[test]
fn a_surface_with_one_mode_draws_no_control() {
    let (_rows, hits) = draw(&sample());
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, ViewHit::SelectMode(_)))
    );
}

/// A rendered document has no line numbers, so it gets its left
/// margin back rather than being indented by a column reserved for
/// nothing.
#[test]
fn unnumbered_lines_are_not_indented_by_an_empty_gutter() {
    let numbered = [ContentLine {
        gutter: "+".to_owned(),
        number: "12".to_owned(),
        tone: LineTone::Added,
        spans: vec![Span::new("code", Role::Default)],
    }];
    let prose = [ContentLine {
        gutter: " ".to_owned(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: vec![Span::new("a paragraph", Role::Default)],
    }];

    assert_eq!(gutter_width(&numbered), GUTTER_WIDTH);
    assert_eq!(gutter_width(&prose), 0);
}

/// The caret marks the character it is on; it never replaces it.
///
/// Drawing a mark into the cell is the obvious implementation and the
/// wrong one: the letter being edited becomes the one letter the
/// person cannot see. This is the test that says so.
/// Marked text is inverted where each of its characters was drawn,
/// and nothing either side of it is.
#[test]
fn marked_text_is_inverted_where_it_was_drawn() {
    let view = sample();
    let Content::Lines { heading, .. } = &view.content else {
        unreachable!("the sample shows lines");
    };
    let mut selection = TextSelection::pressed(Caret { line: 0, column: 3 }, heading.clone());
    selection.carry(Caret { line: 0, column: 6 });

    let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(
                frame,
                &view,
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                Some(&selection),
                &mut hits,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row = hits
        .iter()
        .find(|(_, hit)| matches!(hit, ViewHit::PlaceCaret { line: 0, .. }))
        .expect("the first line is drawn")
        .0;

    let inverted: Vec<u16> = (0..12)
        .filter(|cell| {
            buffer[(row.x + GUTTER_WIDTH + cell, row.y)]
                .modifier
                .contains(Modifier::REVERSED)
        })
        .collect();
    assert_eq!(inverted, [3, 4, 5, 6]);
}

/// Draws `view` and answers the screen and what the frame recorded.
fn drawn_rows(view: &View) -> (ratatui::buffer::Buffer, Rendered) {
    let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();
    let mut rendered = Rendered::default();
    terminal
        .draw(|frame| {
            rendered = render(
                frame,
                view,
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                None,
                &mut Vec::new(),
            );
        })
        .unwrap();
    (terminal.backend().buffer().clone(), rendered)
}

/// Prose breaks between its words, and every character recorded is
/// the one drawn in that cell — so a pointer over a letter names that
/// letter, on whichever row the fold put it.
#[test]
fn prose_breaks_at_words_and_each_character_is_recorded_where_it_was_drawn() {
    let text = "the quick brown fox jumps over the lazy dog again and again";
    let mut view = sample();
    let Content::Lines { lines, .. } = &mut view.content else {
        unreachable!("the sample shows lines");
    };
    *lines = vec![ContentLine {
        gutter: String::new(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: vec![Span::new(text, Role::Default)],
    }];

    let (buffer, rendered) = drawn_rows(&view);

    assert!(rendered.text_rows.len() > 1, "the line is folded");
    let characters: Vec<char> = text.chars().collect();
    for row in &rendered.text_rows {
        for glyph in &row.glyphs {
            assert_eq!(
                buffer[(glyph.x, row.area.y)].symbol(),
                characters[glyph.index].to_string()
            );
        }
        let first = row.glyphs.first().expect("no row is empty").index;
        assert!(
            first == 0 || characters[first - 1] == ' ',
            "a row starts at a word"
        );
    }
}

fn prose(text: &str) -> ContentLine {
    ContentLine {
        gutter: String::new(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: vec![Span::new(text, Role::Default)],
    }
}

/// Where [`fold`] puts each character, as `(row, cell, character)`.
fn folded(text: &str, width: usize, breaks: Breaks) -> Vec<(usize, usize, char)> {
    let mut placed = Vec::new();
    fold(&prose(text), width, breaks, |row, cell, _, character| {
        placed.push((row, cell, character));
    });
    placed
}

/// The row each character of `text` lands on, as a string per row.
fn rows_of(text: &str, width: usize, breaks: Breaks) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for (row, _, character) in folded(text, width, breaks) {
        if rows.len() <= row {
            rows.push(String::new());
        }
        rows[row].push(character);
    }
    rows
}

#[test]
fn prose_folds_before_a_word_and_a_space_hangs_past_the_edge() {
    assert_eq!(rows_of("ab cd ef", 4, Breaks::Words), ["ab ", "cd ", "ef"]);
    // The space after "abcde" does not open a row of its own.
    assert_eq!(rows_of("abcde fg", 5, Breaks::Words), ["abcde ", "fg"]);
}

/// A word wider than the whole row starts a row of its own, then
/// breaks at the cell, since there is no word boundary left to use.
#[test]
fn a_word_wider_than_the_row_breaks_at_the_cell() {
    assert_eq!(
        rows_of("ab cdefghij", 4, Breaks::Words),
        ["ab ", "cdef", "ghij"]
    );
}

#[test]
fn a_tab_and_a_wide_glyph_take_the_cells_they_are_drawn_in() {
    assert_eq!(
        folded("a\tb", 80, Breaks::Cells),
        [(0, 0, 'a'), (0, 1, '\t'), (0, 1 + TAB_WIDTH, 'b')]
    );
    // Two cells left on the row is room for a wide glyph; one is not.
    assert_eq!(rows_of("abc世", 4, Breaks::Cells), ["abc", "世"]);
    assert_eq!(rows_of("ab世", 4, Breaks::Cells), ["ab世"]);
}

/// Past the last glyph of a folded row is that glyph; past the last
/// glyph of the row the line ends on is the line's end.
#[test]
fn beyond_a_folded_row_is_its_last_character_and_beyond_the_line_its_end() {
    let rows = text_rows(
        &prose("ab cd"),
        7,
        Rect::new(0, 0, 3, 4),
        0,
        3,
        Breaks::Words,
    );
    let beyond: Vec<usize> = rows.iter().map(|row| row.beyond).collect();
    assert_eq!(beyond, [2, 5]);
    assert!(rows.iter().all(|row| row.line == 7));
}

/// A line cut at the edge is walked to its end, however far past the
/// row it runs, without a cell past the row being taken for one in it.
#[test]
fn a_line_far_wider_than_the_row_records_only_what_fits() {
    let long = "x".repeat(70_000);
    let rows = text_rows(
        &prose(&long),
        0,
        Rect::new(2, 0, 10, 1),
        0,
        usize::MAX,
        Breaks::Cells,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].glyphs.len(), 10);
    assert_eq!(rows[0].beyond, 70_000);
}

/// Under a question there is nothing to mark: the press belongs to the
/// question, which the caller draws over the content.
#[test]
fn a_question_over_the_content_leaves_no_text_to_mark() {
    let view = View {
        confirm: Some(uze_extensions::view::Confirm {
            title: "Discard changes?".to_owned(),
            subject: "a.txt".to_owned(),
            body: "The edits are lost.".to_owned(),
            confirm: "Discard".to_owned(),
            on_confirm: false,
            field: None,
        }),
        ..sample()
    };

    assert!(!drawn_rows(&sample()).1.text_rows.is_empty());
    assert!(drawn_rows(&view).1.text_rows.is_empty());
}

/// A drawing is pointed at, never marked: the frame records no text.
#[test]
fn a_drawing_records_no_text() {
    let mut view = sample();
    let Content::Lines { medium, .. } = &mut view.content else {
        unreachable!("the sample shows lines");
    };
    *medium = Medium::Drawing;

    let (_, rendered) = drawn_rows(&view);

    assert!(rendered.text_rows.is_empty());
}

#[test]
fn the_caret_marks_the_character_it_sits_on_without_hiding_it() {
    let mut view = sample();
    let Content::Lines { caret, lines, .. } = &mut view.content else {
        unreachable!("the sample shows lines");
    };
    *caret = Some(Caret { line: 0, column: 4 });
    let text = lines[0].spans[0].text.clone();
    let under_caret = text.chars().nth(4).expect("a character to sit on");

    let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(
                frame,
                &view,
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                None,
                &mut hits,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();

    let hit = hits
        .iter()
        .find(|(_, hit)| matches!(hit, ViewHit::PlaceCaret { line: 0, .. }))
        .expect("a drawn content row can be clicked to place the caret");
    let (x, y) = (hit.0.x + GUTTER_WIDTH + 4, hit.0.y);

    assert_eq!(
        buffer[(x, y)].symbol(),
        under_caret.to_string(),
        "the character under the caret is still on screen"
    );
    assert_eq!(
        buffer[(x, y)].bg,
        theme::color(Token::Accent),
        "and it is marked by inverting its cell"
    );
}

fn code_line(text: &str) -> ContentLine {
    ContentLine {
        gutter: " ".to_owned(),
        number: "1".to_owned(),
        tone: LineTone::Neutral,
        spans: vec![Span::new(text, Role::Default)],
    }
}

/// Draws `line` with the caret at `column` into a text column
/// `width` cells wide, and answers what landed where.
fn drawn_with_caret(line: &ContentLine, width: u16, column: usize) -> ratatui::buffer::Buffer {
    let area = Rect::new(0, 0, GUTTER_WIDTH + width, 4);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            render_line(frame, area, line, GUTTER_WIDTH, true);
            render_caret(frame, area, line, column, GUTTER_WIDTH);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

/// ratatui drops a tab as a control character, which drew every
/// tab-indented file flush left and put the caret a tab short.
#[test]
fn a_tab_is_drawn_as_the_indentation_it_is() {
    let buffer = drawn_with_caret(&code_line("\tx"), 10, 1);
    let x = GUTTER_WIDTH + TAB_WIDTH as u16;
    assert_eq!(buffer[(x, 0)].symbol(), "x");
    assert_eq!(buffer[(x, 0)].bg, theme::color(Token::Accent));
}

/// Folded code breaks at the cell, not the word, and the caret is
/// placed by the same walk: under word wrap the caret on a long
/// line drifted off the character it named.
#[test]
fn the_caret_on_a_folded_line_sits_on_its_own_character() {
    let line = code_line("abcd efghijkl");
    assert_eq!(line_height(&line, GUTTER_WIDTH + 6, GUTTER_WIDTH), 3);
    let buffer = drawn_with_caret(&line, 6, 9);
    let (x, y) = (GUTTER_WIDTH + 3, 1);
    assert_eq!(buffer[(x, y)].symbol(), "i");
    assert_eq!(buffer[(x, y)].bg, theme::color(Token::Accent));
}

/// A click resolves to a text position through two halves that each
/// know only their own side: the host counts cells from the row it
/// drew, and the extension turns cells into characters.
#[test]
fn a_click_resolves_to_the_cell_it_landed_on() {
    let view = sample();
    let (_rows, hits) = draw(&view);
    let (rect, hit) = hits
        .iter()
        .find(|(_, hit)| matches!(hit, ViewHit::PlaceCaret { line: 0, .. }))
        .expect("the first content line is clickable");
    let ViewHit::PlaceCaret { cell, .. } = hit else {
        unreachable!("matched above");
    };

    assert_eq!(
        caret_cell_at(*rect, *cell, rect.x + GUTTER_WIDTH + 6, GUTTER_WIDTH),
        6,
        "six cells past the start of the text is six cells into the line"
    );
    assert_eq!(
        caret_cell_at(*rect, *cell, rect.x, GUTTER_WIDTH),
        0,
        "a click on the gutter belongs to the start of the line, not past it"
    );
    // Content that is not numbered has no gutter, and a click on it
    // resolved as though it had one landed seven cells to the left of
    // the pointer — on the map, a tile or two over.
    assert_eq!(
        caret_cell_at(*rect, *cell, rect.x + 6, 0),
        6,
        "with no gutter, the text starts where the row does"
    );
}

/// Chrome resolves through the palette; content keeps the colour it
/// brought. An extension that could paint its own chrome is an
/// extension that drifts from the design system.
#[test]
fn chrome_uses_the_hosts_palette_and_content_keeps_its_own() {
    // Reads the active theme, so it takes its turn with the test that
    // swaps it.
    let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
    terminal
        .draw(|frame| {
            render(
                frame,
                &sample(),
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll: NavigatorScroll::default(),
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                None,
                &mut Vec::new(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    // By cell, never by byte offset: the border glyphs are multi-byte,
    // so a byte index into the joined row is not a column.
    let cell_at = |needle: &str| {
        (0..buffer.area.height).find_map(|row| {
            let cells: Vec<String> = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol().to_owned())
                .collect();
            let wanted: Vec<String> = needle
                .chars()
                .map(|character| character.to_string())
                .collect();
            cells
                .windows(wanted.len())
                .position(|window| window == wanted.as_slice())
                .map(|column| buffer[(column as u16, row)].clone())
        })
    };
    assert_eq!(
        cell_at("let x = 1;").unwrap().fg,
        Color::Rgb(1, 2, 3),
        "syntax colour is the extension's own data"
    );
    assert_eq!(
        cell_at("CHANGES").unwrap().fg,
        theme::color(Token::TextSecondary),
        "a heading is chrome, so it resolves through the palette"
    );
    assert_eq!(
        cell_at("M ui.rs").unwrap().bg,
        theme::color(Token::SurfaceSelected),
        "the selected row carries the surface every other list marks its selection with"
    );
    assert_eq!(
        cell_at("M ").unwrap().fg,
        theme::color(Token::StateWarning),
        "Role::Warning"
    );
}

/// Wrapping is the host's, so the row a long line occupies is too —
/// this used to be asserted inside the extension, which could only
/// guess at the column width.
#[test]
fn a_line_too_long_for_the_column_occupies_more_than_one_row() {
    let line = ContentLine {
        gutter: " ".to_owned(),
        number: "1".to_owned(),
        tone: LineTone::Neutral,
        spans: vec![Span::new("abcdefgh", Role::Default)],
    };
    assert_eq!(line_height(&line, GUTTER_WIDTH + 4, GUTTER_WIDTH), 2);
    assert_eq!(line_height(&line, GUTTER_WIDTH + 8, GUTTER_WIDTH), 1);
}

/// A group folds from its own row, and says so with the same mark the
/// sidebar's sections use.
#[test]
fn a_group_row_is_a_fold_target() {
    let mut view = sample();
    if let Some(navigator) = view.navigator.as_mut()
        && let NavigatorRow::Group { collapsed, .. } = &mut navigator.rows[0]
    {
        *collapsed = true;
    }
    let (rows, hits) = draw(&view);
    let group_row = rows
        .iter()
        .position(|row: &String| row.contains("src/") && !row.contains("DIFF"))
        .expect("the group is drawn") as u16;
    let hit = hits
        .iter()
        .find(|(_, hit)| matches!(hit, ViewHit::ToggleGroup(0)))
        .expect("the group folds by the id the extension gave it");
    assert_eq!(hit.0.y, group_row);
    assert!(
        rows[group_row as usize].contains(&theme::glyph(Symbol::ChevronCollapsed)),
        "{:?}",
        rows[group_row as usize]
    );
}

fn tall_navigator(anchor: Option<usize>) -> View {
    View {
        navigator: Some(Navigator {
            anchor,
            rows: (0..40)
                .map(|index| NavigatorRow::Item {
                    icon: RowIcon::None,
                    id: index,
                    name: format!("file-{index}.rs"),
                    depth: 0,
                    marker: Span::new("M", Role::Warning),
                    marker_side: MarkerSide::Leading,
                    detail: String::new(),
                    selected: Some(index) == anchor,
                })
                .collect(),
            ..sample().navigator.unwrap()
        }),
        ..sample()
    }
}

fn drawn_with(view: &View, scroll: NavigatorScroll) -> (Vec<String>, NavigatorScroll) {
    let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
    let mut settled = NavigatorScroll::default();
    terminal
        .draw(|frame| {
            settled = render(
                frame,
                view,
                frame.area(),
                NavigatorFrame {
                    width: Some(24),
                    scroll,
                    resizing: false,
                    sliding: None,
                },
                uze_keys::Scope::Code,
                None,
                &mut Vec::new(),
            )
            .navigator_scroll;
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    (rows, settled)
}

/// The list scrolls where the wheel put it, and comes back to the
/// selection only when the selection moves — a wheel looking at rows
/// far from it is not pulled back every frame.
#[test]
fn the_list_follows_the_anchor_only_when_it_changes() {
    let view = tall_navigator(Some(30));

    let (rows, settled) = drawn_with(&view, NavigatorScroll::default());
    assert!(
        rows.iter().any(|row| row.contains("file-30.rs")),
        "a new anchor is brought on screen: {rows:?}"
    );
    assert!(settled.first > 0);
    assert_eq!(settled.revealed, Some(30));

    let scrolled_away = NavigatorScroll {
        first: 0,
        ..settled
    };
    let (rows, settled) = drawn_with(&view, scrolled_away);
    assert!(
        rows.iter().any(|row| row.contains("file-0.rs")),
        "the same anchor does not pull the list back: {rows:?}"
    );
    assert_eq!(settled.first, 0);

    let (_, settled) = drawn_with(
        &view,
        NavigatorScroll {
            first: 500,
            ..settled
        },
    );
    assert!(
        settled.first < 40,
        "held to the rows that exist, so the wheel back is not a long way: {settled:?}"
    );
}

#[test]
fn a_view_without_a_navigator_leaves_the_column_empty() {
    let view = View {
        navigator: None,
        content: Content::Message {
            text: "not a git repository".to_owned(),
            hint: None,
            role: Role::Danger,
        },
        ..sample()
    };
    let (rows, _) = draw(&view);
    assert!(rows.iter().any(|row| row.contains("not a git repository")));
    assert!(
        !rows.iter().any(|row| row.contains("CHANGES")),
        "nothing to navigate means no list, not an empty one: {rows:?}"
    );
}

fn draw_message(text: &str, hint: Option<&str>, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            render_message(
                frame,
                frame.area(),
                text,
                hint,
                theme::color(Token::TextMuted),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol().to_owned())
                .collect::<String>()
        })
        .collect()
}

/// An empty state sits in the middle of its pane, however many lines
/// its hint wraps onto.
#[test]
fn a_message_is_centred_on_its_own_height() {
    let rows = draw_message(
        "No file selected",
        Some("Pick one on the left to read it, or press e to edit it in place."),
        100,
        21,
    );
    let drawn: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.trim().is_empty())
        .map(|(index, _)| index)
        .collect();
    let above = drawn[0];
    let below = rows.len() - 1 - drawn[drawn.len() - 1];
    assert!(
        above.abs_diff(below) <= 1,
        "{above} above, {below} below: {rows:#?}"
    );
}

#[test]
fn a_hint_wraps_to_a_reading_measure_under_a_capitalised_heading() {
    let rows = draw_message(
        "No file selected",
        Some("Pick one on the left to read it, or press e to edit it in place."),
        120,
        12,
    );
    assert!(
        rows.iter().any(|row| row.contains("NO FILE SELECTED")),
        "{rows:#?}"
    );
    let hint: Vec<&String> = rows
        .iter()
        .filter(|row| {
            let row = row.trim();
            !row.is_empty() && row != "NO FILE SELECTED"
        })
        .collect();
    assert!(hint.len() >= 2, "the hint breaks into lines: {rows:#?}");
    assert!(
        hint.iter()
            .all(|row| row.trim().chars().count() <= usize::from(MESSAGE_MEASURE)),
        "{hint:#?}"
    );
}

#[test]
fn a_message_without_a_hint_keeps_its_case() {
    let rows = draw_message("not a git repository", None, 60, 5);
    assert!(rows.iter().any(|row| row.contains("not a git repository")));
}

/// A file with nothing to report sits beside folders at its depth, and
/// its name starts where theirs do — its empty marker keeps the column
/// the folders' fold mark takes.
#[test]
fn an_item_without_a_marker_lines_up_with_the_groups_beside_it() {
    let view = View {
        navigator: Some(Navigator {
            heading: "FILES".to_owned(),
            badge: String::new(),
            focused: true,
            anchor: None,
            choosing: None,
            menu: None,
            rows: vec![
                NavigatorRow::Group {
                    id: 0,
                    name: "src".to_owned(),
                    depth: 0,
                    collapsed: true,
                    icon: RowIcon::Directory,
                },
                NavigatorRow::Item {
                    id: 1,
                    name: "Cargo.toml".to_owned(),
                    depth: 0,
                    marker: Span::new(String::new(), Role::Muted),
                    marker_side: MarkerSide::Leading,
                    detail: String::new(),
                    selected: false,
                    icon: RowIcon::Config,
                },
                NavigatorRow::Item {
                    id: 2,
                    name: "main.rs".to_owned(),
                    depth: 0,
                    marker: Span::new("M", Role::Warning),
                    marker_side: MarkerSide::Leading,
                    detail: String::new(),
                    selected: false,
                    icon: RowIcon::Code,
                },
            ],
        }),
        ..sample()
    };
    let (rows, _) = draw(&view);
    let column = |name: &str| {
        rows.iter()
            .find_map(|row| {
                // The navigator column only: the content beside it
                // names files too.
                let cells: Vec<char> = row.chars().take(24).collect();
                let first = name.chars().next().unwrap();
                (0..cells.len()).find(|&start| {
                    cells[start] == first
                        && cells[start..]
                            .iter()
                            .take(name.chars().count())
                            .copied()
                            .eq(name.chars())
                })
            })
            .unwrap_or_else(|| panic!("{name} is drawn: {rows:#?}"))
    };
    assert_eq!(column("Cargo.toml"), column("src"), "{rows:#?}");
    assert_eq!(column("main.rs"), column("src"), "{rows:#?}");
}

/// A name too long for the list's column slides while the pointer is on
/// its row, and stops at an "…" otherwise; only a frame that slid one asks
/// the host for the next.
#[test]
fn a_long_name_slides_under_the_pointer_and_is_cut_elsewhere() {
    let long = "0042-adopt-a-much-longer-name-than-the-column-holds.md";
    let view = View {
        navigator: Some(Navigator {
            heading: "DECISIONS".to_owned(),
            badge: "1".to_owned(),
            focused: true,
            rows: vec![NavigatorRow::Item {
                id: 0,
                name: long.to_owned(),
                depth: 0,
                marker: Span::new("", Role::Muted),
                marker_side: MarkerSide::Leading,
                detail: String::new(),
                selected: false,
                icon: RowIcon::None,
            }],
            anchor: None,
            choosing: None,
            menu: None,
        }),
        ..sample()
    };
    let draw = |sliding: Option<(ViewHit, usize)>| {
        let mut terminal = Terminal::new(TestBackend::new(90, 8)).unwrap();
        let mut rendered = None;
        terminal
            .draw(|frame| {
                rendered = Some(render(
                    frame,
                    &view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                        sliding,
                    },
                    uze_keys::Scope::Spec,
                    None,
                    &mut Vec::new(),
                ));
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row: String = (0..24).map(|column| buffer[(column, 1)].symbol()).collect();
        (row, rendered.unwrap().marquee)
    };

    let (resting, slid) = draw(None);
    assert!(
        resting.contains('…'),
        "cut where nothing points: {resting:?}"
    );
    assert!(!slid, "and nothing asks for another frame");

    let (first, slid) = draw(Some((ViewHit::SelectItem(0), 0)));
    let (held, _) = draw(Some((ViewHit::SelectItem(0), 9)));
    let (later, _) = draw(Some((ViewHit::SelectItem(0), 20)));
    assert_eq!(first, held, "holding its start before it moves");
    assert!(slid, "a sliding name keeps the clock turning");
    assert!(
        !first.contains('…'),
        "under the pointer it passes through: {first:?}"
    );
    assert_ne!(first, later, "and then moving with the clock");

    let (elsewhere, slid) = draw(Some((ViewHit::SelectItem(7), 5)));
    assert_eq!(
        elsewhere, resting,
        "another row under the pointer moves nothing here"
    );
    assert!(!slid);
}
