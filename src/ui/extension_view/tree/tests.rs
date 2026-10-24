use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use uze_extensions::view::{Section, Span};

use super::*;
use crate::ui::extension_view::{SectionState, render_section};

/// Runs `check` once in the default dark theme and once in the light one,
/// each on this thread alone, so no neighbour drawing at the same moment
/// sees either.
fn in_both(check: impl Fn(&str)) {
    for (name, palette) in [
        ("uze", uze_theme::default_theme().clone()),
        (
            "light",
            uze_theme::builtin("light").expect("bundled").clone(),
        ),
    ] {
        uze_theme::with_active(palette, || check(name));
    }
}

fn at(now_ms: u64) -> TreeFrame {
    TreeFrame {
        clock: Clock { now_ms },
        hovered: None,
        fold: false,
    }
}

struct Drawn {
    buffer: Buffer,
    hits: Vec<(Rect, ViewHit)>,
    tree: TreeDrawn,
}

impl Drawn {
    fn rows(&self) -> Vec<String> {
        let area = self.buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| self.buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    fn fg(&self, x: u16, y: u16) -> Color {
        self.buffer[(x, y)].fg
    }

    fn bg(&self, x: u16, y: u16) -> Color {
        self.buffer[(x, y)].bg
    }

    fn hit_at(&self, x: u16, y: u16) -> Option<ViewHit> {
        self.hits
            .iter()
            .find(|(rect, _)| rect.contains(ratatui::layout::Position { x, y }))
            .map(|(_, hit)| *hit)
    }

    /// Where `needle` starts on row `y`, in cells.
    fn column_of(&self, y: u16, needle: &str) -> Option<u16> {
        let row = &self.rows()[usize::from(y)];
        let at = row.find(needle)?;
        Some(text::columns(&row[..at]) as u16)
    }
}

fn draw(root: &Node, width: u16, height: u16, scroll: usize, held: TreeFrame) -> Drawn {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = Vec::new();
    let mut tree = TreeDrawn::default();
    terminal
        .draw(|frame| {
            tree = render_tree(frame, frame.area(), scroll, root, held, &mut hits);
        })
        .unwrap();
    Drawn {
        buffer: terminal.backend().buffer().clone(),
        hits,
        tree,
    }
}

fn node(id: usize, kind: NodeKind) -> Node {
    Node::new(id, kind)
}

fn holding(mut parent: Node, children: Vec<Node>) -> Node {
    parent.children = children;
    parent
}

fn growing(mut child: Node) -> Node {
    child.grow = true;
    child
}

fn words(id: usize, said: &str, role: Role) -> Node {
    node(
        id,
        NodeKind::Text {
            spans: vec![Span::new(said, role)],
            truncate: true,
        },
    )
}

fn stack(id: usize, gap: u16, children: Vec<Node>) -> Node {
    holding(node(id, NodeKind::Stack { gap }), children)
}

fn row(id: usize, gap: u16, align: Align, children: Vec<Node>) -> Node {
    holding(node(id, NodeKind::Row { gap, align }), children)
}

fn segment(weight: u32, filled: u16, role: Role) -> Segment {
    Segment {
        weight,
        filled,
        role,
    }
}

fn tab(label: &str, active: bool) -> Tab {
    Tab {
        label: label.to_owned(),
        active,
    }
}

#[test]
fn a_stack_puts_its_children_a_gap_apart() {
    in_both(|theme| {
        let root = stack(
            0,
            1,
            vec![
                words(1, "first", Role::Default),
                words(2, "second", Role::Muted),
            ],
        );
        let drawn = draw(&root, 20, 4, 0, at(0));
        assert_eq!(drawn.rows()[..3], ["first", "", "second"], "{theme}");
        assert_eq!(drawn.fg(0, 0), color(Role::Default), "{theme}");
        assert_eq!(drawn.fg(0, 2), color(Role::Muted), "{theme}");
    });
}

#[test]
fn a_row_shares_what_is_left_with_its_growing_children_and_aligns_the_rest() {
    in_both(|theme| {
        let grown = row(
            0,
            1,
            Align::Start,
            vec![
                growing(words(1, "name", Role::Default)),
                words(2, "end", Role::Muted),
            ],
        );
        let drawn = draw(&grown, 20, 1, 0, at(0));
        assert_eq!(
            drawn.rows()[0],
            format!("name{}end", " ".repeat(13)),
            "{theme}"
        );

        let ended = row(
            0,
            1,
            Align::End,
            vec![words(1, "a", Role::Default), words(2, "b", Role::Default)],
        );
        assert_eq!(
            draw(&ended, 10, 1, 0, at(0)).rows()[0],
            "       a b",
            "{theme}"
        );
        let centred = row(0, 0, Align::Center, vec![words(1, "ab", Role::Default)]);
        assert_eq!(
            draw(&centred, 10, 1, 0, at(0)).rows()[0],
            "    ab",
            "{theme}"
        );
    });
}

#[test]
fn a_min_width_holds_a_column_open() {
    let mut name = words(1, "a", Role::Default);
    name.min_width = 6;
    let root = row(0, 1, Align::Start, vec![name, words(2, "b", Role::Default)]);
    assert_eq!(draw(&root, 20, 1, 0, at(0)).rows()[0], "a      b");
}

#[test]
fn text_that_does_not_fit_elides_with_the_house_ellipsis_or_wraps() {
    in_both(|theme| {
        let cut = words(0, "a name far too long", Role::Accent);
        let drawn = draw(&cut, 8, 1, 0, at(0));
        let ellipsis = theme::glyph(Symbol::Ellipsis);
        assert_eq!(drawn.rows()[0], format!("a name {ellipsis}"), "{theme}");
        assert_eq!(drawn.fg(0, 0), color(Role::Accent), "{theme}");

        let folded = node(
            0,
            NodeKind::Text {
                spans: vec![Span::new("one two three", Role::Default)],
                truncate: false,
            },
        );
        assert_eq!(
            draw(&folded, 8, 3, 0, at(0)).rows(),
            ["one two", "three", ""],
            "{theme}"
        );
    });
}

#[test]
fn a_badge_stands_on_a_ground_of_its_own_in_its_role() {
    in_both(|theme| {
        let badge = node(
            0,
            NodeKind::Badge {
                text: "new".into(),
                role: Role::Info,
            },
        );
        let drawn = draw(&badge, 10, 1, 0, at(0));
        assert_eq!(drawn.rows()[0], " new", "{theme}");
        assert_eq!(drawn.fg(1, 0), color(Role::Info), "{theme}");
        assert_eq!(
            drawn.bg(0, 0),
            theme::color(Token::SurfaceRaisedSubtle),
            "{theme}"
        );
        assert_eq!(
            drawn.bg(4, 0),
            theme::color(Token::SurfaceRaisedSubtle),
            "{theme}"
        );
    });
}

#[test]
fn marks_in_three_roles_are_one_strip_of_three_hues() {
    in_both(|theme| {
        let marks = node(
            0,
            NodeKind::Marks(vec![Role::Success, Role::Warning, Role::Danger]),
        );
        let drawn = draw(&marks, 10, 1, 0, at(0));
        let mark = theme::glyph(Symbol::MarkFacet);
        assert_eq!(drawn.rows()[0], mark.repeat(3), "{theme}");
        for (x, role) in [Role::Success, Role::Warning, Role::Danger]
            .into_iter()
            .enumerate()
        {
            assert_eq!(drawn.fg(x as u16, 0), color(role), "{theme}: mark {x}");
        }
        assert_ne!(drawn.fg(0, 0), drawn.fg(2, 0), "{theme}");
    });
}

#[test]
fn pips_fill_as_many_as_are_done() {
    in_both(|theme| {
        let pips = node(0, NodeKind::Pips { done: 3, total: 5 });
        let drawn = draw(&pips, 10, 1, 0, at(0));
        let (filled, empty) = (
            theme::glyph(Symbol::PipFilled),
            theme::glyph(Symbol::PipEmpty),
        );
        assert_eq!(
            drawn.rows()[0],
            format!("{}{}", filled.repeat(3), empty.repeat(2)),
            "{theme}"
        );
        assert_ne!(
            drawn.fg(0, 0),
            drawn.fg(4, 0),
            "{theme}: done and to do read apart"
        );
        // More done than there are is all of them, not a panic.
        let over = node(0, NodeKind::Pips { done: 9, total: 2 });
        assert_eq!(
            draw(&over, 10, 1, 0, at(0)).rows()[0],
            filled.repeat(2),
            "{theme}"
        );
    });
}

#[test]
fn a_plain_progress_bar_fills_to_its_share() {
    in_both(|theme| {
        let bar = node(
            0,
            NodeKind::Progress {
                segments: vec![segment(1, 500, Role::Success)],
            },
        );
        let drawn = draw(&bar, 10, 1, 0, at(0));
        let full = theme::frames(Symbol::ProgressFill).last().cloned().unwrap();
        let track = theme::glyph(Symbol::ProgressTrack);
        assert_eq!(
            drawn.rows()[0],
            format!("{}{}", full.repeat(5), track.repeat(5)),
            "{theme}"
        );
        assert_eq!(drawn.fg(0, 0), color(Role::Success), "{theme}");
        assert_eq!(drawn.fg(9, 0), theme::color(Token::TextFaint), "{theme}");
    });
}

/// Six segments, each as wide as its weight and filled to its own share
/// in its own role, to the eighth of a cell.
#[test]
fn a_segmented_progress_bar_has_one_segment_per_entry() {
    in_both(|theme| {
        let segments = vec![
            segment(1, 1000, Role::Success),
            segment(1, 500, Role::Warning),
            segment(2, 0, Role::Danger),
            segment(2, 250, Role::Info),
            segment(3, 1000, Role::Accent),
            segment(3, 125, Role::Muted),
        ];
        // 36 cells for the segments, five between them: 3, 3, 6, 6, 9, 9.
        let bar = node(
            0,
            NodeKind::Progress {
                segments: segments.clone(),
            },
        );
        let drawn = draw(&bar, 41, 1, 0, at(0));
        let steps = theme::frames(Symbol::ProgressFill);
        let full = steps.last().cloned().unwrap();
        let half = steps[3].clone();
        let eighth = steps[0].clone();
        let t = theme::glyph(Symbol::ProgressTrack);
        let expected = [
            full.repeat(3),
            format!("{full}{half}{t}"),
            t.repeat(6),
            format!("{full}{half}{}", t.repeat(4)),
            full.repeat(9),
            format!("{full}{eighth}{}", t.repeat(7)),
        ]
        .join(" ");
        assert_eq!(drawn.rows()[0], expected, "{theme}");
        let starts = [0u16, 4, 8, 15, 22, 32];
        for (start, segment) in starts.into_iter().zip(&segments) {
            if segment.filled > 0 {
                assert_eq!(
                    drawn.fg(start, 0),
                    color(segment.role),
                    "{theme}: at {start}"
                );
            }
        }
        for gap in [3u16, 7, 14, 21, 31] {
            assert_eq!(
                drawn.buffer[(gap, 0)].symbol(),
                " ",
                "{theme}: gap at {gap}"
            );
        }
    });
}

#[test]
fn a_bar_too_narrow_for_gaps_still_draws_every_weighted_segment() {
    let bar = node(
        0,
        NodeKind::Progress {
            segments: vec![
                segment(100, 1000, Role::Success),
                segment(1, 1000, Role::Danger),
            ],
        },
    );
    let drawn = draw(&bar, 4, 1, 0, at(0));
    assert_eq!(drawn.fg(3, 0), color(Role::Danger));
    assert_eq!(drawn.fg(0, 0), color(Role::Success));
}

#[test]
fn a_list_marks_its_selected_row_and_answers_a_click_on_each() {
    in_both(|theme| {
        let list = holding(
            node(
                7,
                NodeKind::List {
                    selected: Some(1),
                    empty: None,
                },
            ),
            vec![
                words(1, "alpha", Role::Default),
                words(2, "beta", Role::Default),
                words(3, "gamma", Role::Default),
            ],
        );
        let drawn = draw(&list, 12, 3, 0, at(0));
        assert_eq!(drawn.rows(), [" alpha", " beta", " gamma"], "{theme}");
        assert_eq!(
            drawn.bg(0, 1),
            theme::color(Token::SurfaceSelected),
            "{theme}"
        );
        assert_eq!(
            drawn.bg(11, 1),
            theme::color(Token::SurfaceSelected),
            "{theme}"
        );
        assert_ne!(
            drawn.bg(0, 0),
            theme::color(Token::SurfaceSelected),
            "{theme}"
        );
        for index in 0..3 {
            assert_eq!(
                drawn.hit_at(5, index as u16),
                Some(ViewHit::Node {
                    node: 7,
                    act: NodeAct::Row(index)
                }),
                "{theme}"
            );
        }
    });
}

#[test]
fn a_row_under_the_pointer_takes_the_hover_ground() {
    let list = holding(
        node(
            7,
            NodeKind::List {
                selected: None,
                empty: None,
            },
        ),
        vec![
            words(1, "alpha", Role::Default),
            words(2, "beta", Role::Default),
        ],
    );
    let held = TreeFrame {
        hovered: Some(ViewHit::Node {
            node: 7,
            act: NodeAct::Row(1),
        }),
        ..at(0)
    };
    let drawn = draw(&list, 12, 2, 0, held);
    assert_eq!(drawn.bg(3, 1), theme::color(Token::SurfaceHover));
    assert_ne!(drawn.bg(3, 0), theme::color(Token::SurfaceHover));
}

#[test]
fn an_empty_list_or_table_says_so_quietly() {
    in_both(|theme| {
        let list = node(
            0,
            NodeKind::List {
                selected: None,
                empty: Some("no phases yet".into()),
            },
        );
        let drawn = draw(&list, 20, 2, 0, at(0));
        assert_eq!(drawn.rows()[0], "no phases yet", "{theme}");
        assert_eq!(drawn.fg(0, 0), theme::color(Token::TextMuted), "{theme}");
        let table = node(
            0,
            NodeKind::Table {
                columns: vec!["phase".into()],
                empty: Some("nothing planned".into()),
            },
        );
        assert_eq!(
            draw(&table, 20, 2, 0, at(0)).rows()[0],
            "nothing planned",
            "{theme}"
        );
    });
}

#[test]
fn a_table_lines_its_cells_up_under_their_headings() {
    in_both(|theme| {
        let line = |id: usize, phase: &str, plans: &str, state: &str| {
            row(
                id,
                0,
                Align::Start,
                vec![
                    words(id + 1, phase, Role::Default),
                    words(id + 2, plans, Role::Muted),
                    words(id + 3, state, Role::Success),
                ],
            )
        };
        let table = holding(
            node(
                0,
                NodeKind::Table {
                    columns: vec!["phase".into(), "plans".into(), "state".into()],
                    empty: None,
                },
            ),
            vec![
                line(10, "1 setup the long phase", "3", "done"),
                line(20, "2 build", "12", "running"),
            ],
        );
        let drawn = draw(&table, 50, 4, 0, at(0));
        let rows = drawn.rows();
        assert_eq!(rows[1], "", "{theme}: a blank row under the headings");
        let plans = drawn.column_of(0, "plans").unwrap();
        let state = drawn.column_of(0, "state").unwrap();
        assert_eq!(plans, 24, "{theme}: the widest phase and the gap");
        assert_eq!(drawn.column_of(2, "3"), Some(plans), "{theme}");
        assert_eq!(drawn.column_of(3, "12"), Some(plans), "{theme}");
        assert_eq!(drawn.column_of(2, "done"), Some(state), "{theme}");
        assert_eq!(drawn.column_of(3, "running"), Some(state), "{theme}");
        assert_eq!(drawn.fg(0, 0), theme::color(Token::TextMuted), "{theme}");
        assert_eq!(drawn.fg(state, 3), color(Role::Success), "{theme}");
    });
}

#[test]
fn a_table_too_wide_gives_up_cells_from_its_widest_column() {
    let table = holding(
        node(
            0,
            NodeKind::Table {
                columns: vec!["name".into(), "n".into()],
                empty: None,
            },
        ),
        vec![row(
            1,
            0,
            Align::Start,
            vec![
                words(2, "a very long name indeed", Role::Default),
                words(3, "42", Role::Default),
            ],
        )],
    );
    let drawn = draw(&table, 16, 3, 0, at(0));
    assert!(drawn.rows()[2].ends_with("  42"), "{:?}", drawn.rows());
}

#[test]
fn tabs_are_the_house_chips_and_each_answers_with_its_index() {
    in_both(|theme| {
        let tabs = node(
            4,
            NodeKind::Tabs {
                tabs: vec![tab("phases", true), tab("log", false), tab("plans", false)],
            },
        );
        let drawn = draw(&tabs, 40, 1, 0, at(0));
        assert_eq!(drawn.rows()[0], " phases  log  plans", "{theme}");
        assert_eq!(
            drawn.bg(1, 0),
            theme::color(Token::SurfaceRaised),
            "{theme}"
        );
        assert_eq!(drawn.fg(1, 0), theme::color(Token::TextBright), "{theme}");
        assert_eq!(drawn.fg(9, 0), theme::color(Token::TextMuted), "{theme}");
        for (x, index) in [
            (0u16, 0usize),
            (3, 0),
            (7, 0),
            (8, 1),
            (12, 1),
            (13, 2),
            (19, 2),
        ] {
            assert_eq!(
                drawn.hit_at(x, 0),
                Some(ViewHit::Node {
                    node: 4,
                    act: NodeAct::Tab(index)
                }),
                "{theme}: at {x}"
            );
        }
        assert_eq!(drawn.hit_at(20, 0), None, "{theme}: past the last tab");
    });
}

#[test]
fn a_tab_at_rest_brightens_under_the_pointer_and_keeps_its_weight() {
    let tabs = node(
        4,
        NodeKind::Tabs {
            tabs: vec![tab("phases", true), tab("log", false)],
        },
    );
    let held = TreeFrame {
        hovered: Some(ViewHit::Node {
            node: 4,
            act: NodeAct::Tab(1),
        }),
        ..at(0)
    };
    let drawn = draw(&tabs, 40, 1, 0, held);
    assert_eq!(drawn.fg(9, 0), theme::color(Token::TextBright));
    assert!(!drawn.buffer[(9, 0)].modifier.contains(Modifier::BOLD));
}

#[test]
fn tabs_wider_than_the_room_draw_only_the_ones_that_fit_whole() {
    let tabs = node(
        4,
        NodeKind::Tabs {
            tabs: vec![tab("phases", true), tab("log", false), tab("plans", false)],
        },
    );
    let drawn = draw(&tabs, 14, 1, 0, at(0));
    assert_eq!(drawn.rows()[0], " phases  log");
    assert_eq!(drawn.hits.len(), 2);
}

#[test]
fn a_toggle_says_its_state_and_its_whole_width_flips_it() {
    in_both(|theme| {
        let on = node(
            5,
            NodeKind::Toggle {
                label: "auto".into(),
                on: true,
            },
        );
        let drawn = draw(&on, 20, 1, 0, at(0));
        let mark = theme::glyph(Symbol::MarkToggleOn);
        assert_eq!(drawn.rows()[0], format!("{mark} auto"), "{theme}");
        assert_eq!(drawn.fg(0, 0), theme::color(Token::Accent), "{theme}");
        let flip = Some(ViewHit::Node {
            node: 5,
            act: NodeAct::Toggle,
        });
        assert_eq!(drawn.hit_at(0, 0), flip, "{theme}");
        assert_eq!(drawn.hit_at(5, 0), flip, "{theme}");
        assert_eq!(drawn.hit_at(6, 0), None, "{theme}");

        let off = node(
            5,
            NodeKind::Toggle {
                label: "auto".into(),
                on: false,
            },
        );
        let mark = theme::glyph(Symbol::MarkToggleOff);
        assert_eq!(
            draw(&off, 20, 1, 0, at(0)).rows()[0],
            format!("{mark} auto"),
            "{theme}"
        );
    });
}

#[test]
fn a_button_is_the_house_button_and_a_click_anywhere_on_it_presses_it() {
    in_both(|theme| {
        let button = node(
            6,
            NodeKind::Button {
                label: "next".into(),
            },
        );
        let drawn = draw(&button, 20, 1, 0, at(0));
        assert_eq!(drawn.rows()[0], "  next", "{theme}");
        let press = Some(ViewHit::Node {
            node: 6,
            act: NodeAct::Press,
        });
        for x in 0..8 {
            assert_eq!(drawn.hit_at(x, 0), press, "{theme}: at {x}");
        }
        assert_eq!(drawn.hit_at(8, 0), None, "{theme}");
        assert_eq!(
            drawn.fg(2, 0),
            theme::color(Token::TextSecondary),
            "{theme}"
        );
    });
}

#[test]
fn a_button_under_the_pointer_fills_the_way_a_hovered_offer_does() {
    let button = node(
        6,
        NodeKind::Button {
            label: "next".into(),
        },
    );
    let held = TreeFrame {
        hovered: Some(ViewHit::Node {
            node: 6,
            act: NodeAct::Press,
        }),
        ..at(0)
    };
    let drawn = draw(&button, 20, 1, 0, held);
    assert_eq!(drawn.bg(2, 0), theme::color(Token::TextSecondary));
}

#[test]
fn a_busy_activity_moves_with_the_clock_and_an_idle_one_does_not() {
    in_both(|theme| {
        let busy = node(0, NodeKind::Activity { busy: true });
        let first = draw(&busy, 4, 1, 0, at(0));
        let next = draw(&busy, 4, 1, 0, at(ACTIVITY_FRAME_MS));
        assert_eq!(
            first.rows()[0],
            theme::frame(Symbol::ActivityBusy, 0),
            "{theme}"
        );
        assert_ne!(first.rows()[0], next.rows()[0], "{theme}: the next frame");
        assert!(first.tree.animates, "{theme}");
        assert_eq!(
            first.fg(0, 0),
            theme::color(Token::StateInFlight),
            "{theme}"
        );

        let idle = node(0, NodeKind::Activity { busy: false });
        let still = draw(&idle, 4, 1, 0, at(0));
        assert_eq!(
            still.rows()[0],
            theme::glyph(Symbol::ActivityIdle),
            "{theme}"
        );
        assert_eq!(
            still.rows(),
            draw(&idle, 4, 1, 0, at(ACTIVITY_FRAME_MS)).rows(),
            "{theme}"
        );
        assert!(!still.tree.animates, "{theme}");
    });
}

#[test]
fn elapsed_is_said_in_the_fewest_characters_and_ticks() {
    assert_eq!(elapsed(0, 12_000), "12s");
    assert_eq!(elapsed(0, 252_000), "4m12s");
    assert_eq!(elapsed(0, 3_840_000), "1h04m");
    assert_eq!(elapsed(0, 2 * 86_400_000 + 3 * 3_600_000), "2d03h");
    assert_eq!(elapsed(10_000, 0), "0s", "a start in the future is now");

    in_both(|theme| {
        let since = node(
            0,
            NodeKind::Elapsed {
                since_ms: Some(1_000_000),
                role: Role::Muted,
            },
        );
        let drawn = draw(&since, 10, 1, 0, at(1_252_000));
        assert_eq!(drawn.rows()[0], "4m12s", "{theme}");
        assert_eq!(drawn.fg(0, 0), color(Role::Muted), "{theme}");
        assert_eq!(
            draw(&since, 10, 1, 0, at(1_253_000)).rows()[0],
            "4m13s",
            "{theme}"
        );
        assert!(drawn.tree.animates, "{theme}");

        let never = node(
            0,
            NodeKind::Elapsed {
                since_ms: None,
                role: Role::Muted,
            },
        );
        assert_eq!(draw(&never, 10, 1, 0, at(0)).rows()[0], "", "{theme}");
    });
}

#[test]
fn a_log_draws_a_line_a_row_and_a_notice_in_its_role() {
    in_both(|theme| {
        let log = node(
            0,
            NodeKind::Log {
                lines: vec![
                    Span::new("plan 1 written", Role::Muted),
                    Span::new("plan 2 failed", Role::Danger),
                ],
            },
        );
        let drawn = draw(&log, 20, 2, 0, at(0));
        assert_eq!(drawn.rows(), ["plan 1 written", "plan 2 failed"], "{theme}");
        assert_eq!(drawn.fg(0, 1), color(Role::Danger), "{theme}");

        let notice = node(
            0,
            NodeKind::Notice {
                text: "folded at 500 nodes".into(),
                role: Role::Warning,
            },
        );
        let drawn = draw(&notice, 30, 1, 0, at(0));
        assert_eq!(drawn.rows()[0], "folded at 500 nodes", "{theme}");
        assert_eq!(drawn.fg(0, 0), color(Role::Warning), "{theme}");
    });
}

#[test]
fn scrolling_skips_rows_from_the_top_and_hits_follow_what_is_drawn() {
    let list = holding(
        node(
            9,
            NodeKind::List {
                selected: None,
                empty: None,
            },
        ),
        (0..10)
            .map(|index| words(index + 1, &format!("row {index}"), Role::Default))
            .collect(),
    );
    let drawn = draw(&list, 12, 3, 4, at(0));
    assert_eq!(drawn.rows(), [" row 4", " row 5", " row 6"]);
    assert_eq!(drawn.hits.len(), 3, "no hit for a row out of sight");
    assert_eq!(
        drawn.hit_at(1, 0),
        Some(ViewHit::Node {
            node: 9,
            act: NodeAct::Row(4)
        })
    );
    assert!(!drawn.tree.at_end);

    // Scrolled past the end, the window settles on the last page.
    let past = draw(&list, 12, 3, 40, at(0));
    assert_eq!(past.rows(), [" row 7", " row 8", " row 9"]);
    assert!(past.tree.at_end);
}

#[test]
fn nothing_is_drawn_outside_the_area_it_was_given() {
    let root = stack(
        0,
        0,
        vec![
            words(1, "a line much wider than its area", Role::Default),
            node(
                2,
                NodeKind::Button {
                    label: "a wide button".into(),
                },
            ),
            words(3, "below", Role::Default),
        ],
    );
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render_tree(frame, Rect::new(2, 1, 10, 2), 0, &root, at(0), &mut hits);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for y in 0..6u16 {
        for x in 0..20u16 {
            let inside = (2..12).contains(&x) && (1..3).contains(&y);
            if !inside {
                assert_eq!(buffer[(x, y)].symbol(), " ", "drawn at ({x}, {y})");
            }
        }
    }
    assert!(
        hits.iter()
            .all(|(rect, _)| rect.right() <= 12 && rect.bottom() <= 3)
    );
}

#[test]
fn a_folded_tree_draws_whole_nodes_and_says_how_many_more() {
    in_both(|theme| {
        let root = stack(
            0,
            0,
            (0..6)
                .map(|index| words(index + 1, &format!("item {index}"), Role::Default))
                .collect(),
        );
        let held = TreeFrame {
            fold: true,
            ..at(0)
        };
        let drawn = draw(&root, 20, 4, 0, held);
        let more = theme::glyph(Symbol::MarkMore);
        assert_eq!(
            drawn.rows(),
            ["item 0", "item 1", "item 2", &format!("{more} 3 more")],
            "{theme}"
        );
        assert_eq!(drawn.fg(0, 3), theme::color(Token::TextMuted), "{theme}");

        // A two-row child that does not fit whole is left out, not cut.
        let tall = stack(
            0,
            0,
            vec![
                words(1, "one", Role::Default),
                stack(
                    2,
                    0,
                    vec![
                        words(3, "two a", Role::Default),
                        words(4, "two b", Role::Default),
                    ],
                ),
                words(5, "three", Role::Default),
            ],
        );
        let drawn = draw(&tall, 20, 3, 0, held);
        assert_eq!(
            drawn.rows(),
            ["one", &format!("{more} 2 more"), ""],
            "{theme}"
        );
    });
}

#[test]
fn a_tree_section_folds_inside_its_rows_with_the_column_inset() {
    let section = Section {
        title: "gsd".into(),
        tree: Some(stack(
            0,
            0,
            (0..8)
                .map(|index| words(index + 1, &format!("phase {index}"), Role::Default))
                .collect(),
        )),
        ..Section::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(24, 5)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            let mut rows = crate::ui::Rows::over(frame.area());
            render_section(
                frame,
                &section,
                &mut rows,
                SectionState::default(),
                &mut hits,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..5u16)
        .map(|y| {
            (0..24u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    let more = theme::glyph(Symbol::MarkMore);
    assert!(rows[0].contains("gsd"), "{rows:?}");
    assert_eq!(
        rows[1..],
        [
            " phase 0",
            " phase 1",
            " phase 2",
            &format!(" {more} 5 more")
        ]
    );
    assert_eq!(
        crate::ui::extension_view::tree_section_height(section.tree.as_ref().unwrap(), 24),
        9
    );
}

#[test]
fn the_attribution_strip_names_the_extension_and_keeps_its_row() {
    in_both(|theme| {
        let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
        let mut left = Rect::default();
        terminal
            .draw(|frame| {
                let said = attribution("gsd-band", "gsd", "community");
                left = render_attribution(frame, frame.area(), &said);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let strip: String = (0..40u16).map(|x| buffer[(x, 0)].symbol()).collect();
        let separator = theme::glyph(Symbol::HintSeparator);
        assert_eq!(
            strip.trim_end(),
            format!(" gsd-band{separator}gsd@community"),
            "{theme}"
        );
        assert_eq!(
            buffer[(39, 0)].bg,
            theme::color(Token::SurfaceRaisedSubtle),
            "{theme}"
        );
        assert_eq!(buffer[(1, 0)].fg, theme::color(Token::TextMuted), "{theme}");
        assert_eq!(left, Rect::new(0, 1, 40, 2), "{theme}");
    });
}

/// The board the `gsd-band` plugin draws, built by hand: a header with
/// what is under way, a six-part bar, tabs, a table of phases with their
/// marks, pips and state, and the actions under it.
#[test]
fn a_gsd_band_board_draws_every_part_and_answers_every_control() {
    let phase = |id: usize, name: &str, marks: Vec<Role>, done: u16, state: (&str, Role)| {
        row(
            id,
            0,
            Align::Start,
            vec![
                words(id + 1, name, Role::Default),
                node(id + 2, NodeKind::Marks(marks)),
                node(id + 3, NodeKind::Pips { done, total: 4 }),
                node(
                    id + 4,
                    NodeKind::Badge {
                        text: state.0.into(),
                        role: state.1,
                    },
                ),
            ],
        )
    };
    let board = stack(
        0,
        1,
        vec![
            row(
                1,
                1,
                Align::Start,
                vec![
                    growing(words(2, "gsd-band · milestone 3", Role::Bright)),
                    node(3, NodeKind::Activity { busy: true }),
                    node(
                        4,
                        NodeKind::Elapsed {
                            since_ms: Some(0),
                            role: Role::Muted,
                        },
                    ),
                ],
            ),
            growing(node(
                5,
                NodeKind::Progress {
                    segments: vec![
                        segment(19, 1000, Role::Success),
                        segment(12, 800, Role::Success),
                        segment(8, 400, Role::Warning),
                        segment(6, 0, Role::Muted),
                        segment(4, 0, Role::Muted),
                        segment(2, 0, Role::Muted),
                    ],
                },
            )),
            node(
                6,
                NodeKind::Tabs {
                    tabs: vec![tab("phases", true), tab("log", false)],
                },
            ),
            holding(
                node(
                    7,
                    NodeKind::Table {
                        columns: vec!["phase".into(), "dpe".into(), "plans".into(), "state".into()],
                        empty: None,
                    },
                ),
                vec![
                    phase(
                        10,
                        "1 discovery",
                        vec![Role::Success; 3],
                        4,
                        ("done", Role::Success),
                    ),
                    phase(
                        20,
                        "2 engine",
                        vec![Role::Success, Role::Warning, Role::Muted],
                        2,
                        ("partial", Role::Warning),
                    ),
                ],
            ),
            row(
                30,
                2,
                Align::Start,
                vec![
                    node(
                        31,
                        NodeKind::Button {
                            label: "next".into(),
                        },
                    ),
                    node(
                        32,
                        NodeKind::Toggle {
                            label: "auto".into(),
                            on: false,
                        },
                    ),
                ],
            ),
        ],
    );
    in_both(|theme| {
        let drawn = draw(&board, 60, 14, 0, at(252_000));
        let rows = drawn.rows();
        assert!(
            rows[0].starts_with("gsd-band · milestone 3"),
            "{theme}: {rows:#?}"
        );
        assert!(rows[0].ends_with("4m12s"), "{theme}: {rows:#?}");
        let full = theme::frames(Symbol::ProgressFill).last().cloned().unwrap();
        assert!(rows[2].starts_with(&full), "{theme}: {rows:#?}");
        assert_eq!(drawn.fg(0, 2), color(Role::Success), "{theme}");
        assert_eq!(rows[4], " phases  log", "{theme}");
        assert!(rows[6].starts_with("phase"), "{theme}: {rows:#?}");
        assert_eq!(
            drawn.column_of(8, "done"),
            drawn.column_of(9, "partial"),
            "{theme}: {rows:#?}"
        );
        assert_eq!(
            drawn.column_of(6, "state"),
            drawn.column_of(8, "done").map(|x| x - 1)
        );
        assert!(
            rows[11].contains("next") && rows[11].contains("auto"),
            "{theme}: {rows:#?}"
        );
        let controls: Vec<ViewHit> = drawn.hits.iter().map(|(_, hit)| *hit).collect();
        for act in [
            ViewHit::Node {
                node: 6,
                act: NodeAct::Tab(0),
            },
            ViewHit::Node {
                node: 6,
                act: NodeAct::Tab(1),
            },
            ViewHit::Node {
                node: 31,
                act: NodeAct::Press,
            },
            ViewHit::Node {
                node: 32,
                act: NodeAct::Toggle,
            },
        ] {
            assert!(controls.contains(&act), "{theme}: {act:?} in {controls:?}");
        }
        assert!(drawn.tree.animates, "{theme}");
    });
}

#[test]
fn a_tree_view_wears_the_standard_scrollbar_when_it_overflows() {
    let root = stack(
        0,
        0,
        (0..30)
            .map(|index| words(index + 1, &format!("line {index}"), Role::Default))
            .collect(),
    );
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    let mut hits = Vec::new();
    let mut drawn = TreeDrawn::default();
    terminal
        .draw(|frame| {
            drawn = render_tree_content(
                frame,
                Rect::new(0, 0, 29, 10),
                ("board", 0, &root),
                &[],
                at(0),
                &mut hits,
            );
        })
        .unwrap();
    assert!(drawn.bar.is_some());
    assert!(
        hits.iter()
            .any(|(_, hit)| *hit == ViewHit::DragContentScrollbar)
    );
    let buffer = terminal.backend().buffer();
    let heading: String = (0..5u16).map(|x| buffer[(x, 0)].symbol()).collect();
    assert_eq!(heading, "board");
}

#[test]
fn a_text_a_row_squeezes_to_nothing_takes_no_rows_and_folds_nothing_away() {
    // The gsd band's header in a sidebar's width: its growing title is
    // left no cells by the controls beside it.
    let title = growing(node(
        1,
        NodeKind::Text {
            spans: vec![Span::new("GSD · v1.23 Assistente do lojista", Role::Muted)],
            truncate: false,
        },
    ));
    let header = row(
        0,
        0,
        Align::Center,
        vec![
            node(2, NodeKind::Activity { busy: false }),
            title,
            node(
                3,
                NodeKind::Tabs {
                    tabs: vec![tab("Phases", true), tab("Log", false)],
                },
            ),
            node(
                4,
                NodeKind::Button {
                    label: "clear".into(),
                },
            ),
            node(
                5,
                NodeKind::Button {
                    label: "▶ run phase 12".into(),
                },
            ),
        ],
    );
    assert_eq!(row_widths(&header.children, 0, Align::Center, 25).1[1], 0);
    assert_eq!(height(&header, 25), 1);

    let band = stack(
        6,
        0,
        vec![header, words(7, "Catálogo de ajuda", Role::Default)],
    );
    let folding = TreeFrame {
        fold: true,
        ..at(0)
    };
    let rows = draw(&band, 25, 4, 0, folding).rows();
    assert!(rows[1].contains("Catálogo de ajuda"), "{rows:#?}");
}
