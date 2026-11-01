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
        clock: Clock {
            now_ms,
            instant: std::time::Instant::now(),
        },
        hovered: None,
        rested: 0,
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
            still: false,
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

/// A row without room folds its wrapping text onto a second row, widest
/// first, rather than cutting the last word to a sliver or losing it.
#[test]
fn a_crowded_row_wraps_its_widest_text_and_keeps_the_last_whole() {
    let paragraph = |id, said: &str, role| {
        node(
            id,
            NodeKind::Text {
                spans: vec![Span::new(said, role)],
                truncate: false,
                still: false,
            },
        )
    };
    let crowded = row(
        0,
        2,
        Align::Start,
        vec![
            paragraph(
                1,
                "12 · A long phase name that takes the row",
                Role::Default,
            ),
            paragraph(2, "2 plans left", Role::Muted),
        ],
    );
    let rows = draw(&crowded, 40, 3, 0, at(0)).rows();
    assert!(rows[0].ends_with("2 plans left"), "{rows:#?}");
    assert!(rows[1].contains("takes the row"), "{rows:#?}");

    let tight = row(
        0,
        1,
        Align::Start,
        vec![
            paragraph(1, "412 passed", Role::Success),
            paragraph(2, "2 failed", Role::Danger),
            paragraph(3, "9 skipped", Role::Faint),
        ],
    );
    let rows = draw(&tight, 28, 2, 0, at(0)).rows();
    assert!(rows[0].ends_with("9 skipped"), "{rows:#?}");
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
                still: false,
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
                line: false,
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
                line: false,
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
            line: false,
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
                primary: false,
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
            primary: false,
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
                until_ms: None,
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
                until_ms: None,
                role: Role::Muted,
            },
        );
        assert_eq!(draw(&never, 10, 1, 0, at(0)).rows()[0], "", "{theme}");
    });
}

#[test]
fn a_countdown_is_said_as_a_clock_and_stops_at_nought() {
    assert_eq!(countdown(1_500_000, 2_000), "24:58");
    assert_eq!(
        countdown(1_500_000, 1_499_500),
        "0:01",
        "a second begun is left"
    );
    assert_eq!(countdown(3_725_000, 0), "1:02:05");
    assert_eq!(countdown(0, 10_000), "0:00", "an end gone by is over");

    in_both(|theme| {
        let until = node(
            0,
            NodeKind::Elapsed {
                since_ms: Some(0),
                until_ms: Some(1_500_000),
                role: Role::Warning,
            },
        );
        let drawn = draw(&until, 10, 1, 0, at(2_000));
        assert_eq!(
            drawn.rows()[0],
            "24:58",
            "{theme}: `until` wins over `since`"
        );
        assert_eq!(drawn.fg(0, 0), color(Role::Warning), "{theme}");
        assert!(drawn.tree.animates, "{theme}");
    });
}

#[test]
fn digits_draw_their_figure_five_rows_tall_and_a_countdown_ticks() {
    in_both(|theme| {
        let figure = node(
            0,
            NodeKind::Digits {
                text: "25:00".into(),
                until_ms: None,
                role: Role::Info,
            },
        );
        assert_eq!((tree_width(&figure), tree_height(&figure, 40)), (17, 5));
        let drawn = draw(&figure, 20, 5, 0, at(0));
        assert_eq!(drawn.rows()[0].trim_end(), "███ ███   ███ ███", "{theme}");
        assert_eq!(drawn.rows()[4].trim_end(), "███ ███   ███ ███", "{theme}");
        assert_eq!(drawn.fg(0, 0), color(Role::Info), "{theme}");
        assert!(!drawn.tree.animates, "{theme}: a figure stands still");

        let left = node(
            0,
            NodeKind::Digits {
                text: String::new(),
                until_ms: Some(1_100_000),
                role: Role::Warning,
            },
        );
        assert_eq!(tree_height(&left, 40), 5);
        let drawn = draw(&left, 20, 5, 0, at(0));
        assert_eq!(
            drawn.rows()[0].trim_end(),
            " █  ███   ███ ███",
            "{theme}: 18:20"
        );
        assert_eq!(drawn.fg(1, 0), color(Role::Warning), "{theme}");
        assert!(drawn.tree.animates, "{theme}");
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
fn a_withheld_surface_says_why_inset_as_rows_are_and_answers_with_review() {
    let withheld = node(
        3,
        NodeKind::Withheld {
            text: "needs consent".into(),
        },
    );
    let drawn = draw(&withheld, 30, 1, 0, at(0));
    let row = &drawn.rows()[0];
    assert!(
        row.starts_with(" needs consent"),
        "inset as a list's rows: {row:?}"
    );
    assert!(row.trim_end().ends_with("review"), "{row:?}");
    assert_eq!(drawn.fg(1, 0), color(Role::Warning));
    assert_eq!(
        drawn.hit_at(5, 0),
        Some(ViewHit::Node {
            node: 3,
            act: NodeAct::Review,
        }),
        "the whole row answers"
    );
}

/// A section's list reads as the sidebar's own rows: no ground, its text
/// held back at rest, and the row under the pointer lit in its own hue.
#[test]
fn a_sections_list_rows_take_no_ground_and_light_under_the_pointer() {
    let list = holding(
        node(
            9,
            NodeKind::List {
                selected: None,
                empty: None,
            },
        ),
        vec![
            words(1, "extension view engine", Role::Default),
            words(2, "doctor delivery", Role::Default),
        ],
    );
    let second = ViewHit::Node {
        node: 9,
        act: NodeAct::Row(1),
    };
    let in_section = |hovered| TreeFrame {
        fold: true,
        ..TreeFrame::now(hovered)
    };
    let drawn = draw(&list, 30, 2, 0, in_section(Some(second)));
    assert_eq!(
        drawn.fg(2, 0),
        theme::color(Token::TextSecondary),
        "held back at rest"
    );
    assert_eq!(
        drawn.fg(2, 1),
        theme::color(Token::TextBright),
        "lit under the pointer"
    );
    for row in 0..2 {
        for x in 0..30 {
            assert_eq!(drawn.bg(x, row), Color::Reset, "no ground at ({x}, {row})");
        }
    }
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
                    primary: false,
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
fn a_surface_is_headed_by_its_own_title_and_nothing_else() {
    in_both(|theme| {
        let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
        let mut left = Rect::default();
        terminal
            .draw(|frame| {
                left = render_title(frame, frame.area(), "Contributions");
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row: String = (0..40u16).map(|x| buffer[(x, 0)].symbol()).collect();
        assert_eq!(
            row.trim_end(),
            " Contributions",
            "{theme}: no plugin, no marketplace"
        );
        assert_eq!(buffer[(1, 0)].fg, theme::color(Token::TextMuted), "{theme}");
        assert_ne!(
            buffer[(39, 0)].bg,
            theme::color(Token::SurfaceRaisedSubtle),
            "{theme}: no strip of its own"
        );
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
                            until_ms: None,
                            role: Role::Muted,
                        },
                    ),
                ],
            ),
            growing(node(
                5,
                NodeKind::Progress {
                    line: false,
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
                            primary: false,
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
            still: false,
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
                    primary: false,
                },
            ),
            node(
                5,
                NodeKind::Button {
                    label: "▶ run phase 12".into(),
                    primary: false,
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

fn gauge_node(id: usize, label: &str, filled: u16, role: Role) -> Node {
    node(
        id,
        NodeKind::Gauge {
            label: label.into(),
            filled,
            role,
        },
    )
}

#[test]
fn gauges_in_a_stack_share_their_label_column_and_say_their_percent() {
    in_both(|theme| {
        let root = stack(
            0,
            0,
            vec![
                gauge_node(1, "Opus", 620, Role::Info),
                gauge_node(2, "Sonnet", 930, Role::Danger),
            ],
        );
        let drawn = draw(&root, 40, 2, 0, at(0));
        let rows = drawn.rows();
        assert!(
            rows[0].ends_with(" 62%") && rows[1].ends_with(" 93%"),
            "{theme}: {rows:#?}"
        );
        let bar_start = |row: &str| {
            row.char_indices()
                .find(|(_, ch)| !ch.is_alphanumeric() && *ch != ' ')
                .map(|(at, _)| row[..at].chars().count())
        };
        assert_eq!(
            bar_start(&rows[0]),
            bar_start(&rows[1]),
            "{theme}: {rows:#?}"
        );
        assert_eq!(drawn.fg(39, 1), color(Role::Danger), "{theme}");
    });
}

#[test]
fn a_sparkline_scales_to_its_largest_and_bars_stack_from_the_bottom() {
    in_both(|theme| {
        let levels = theme::frames(Symbol::ChartLevel);
        let spark = node(
            0,
            NodeKind::Chart {
                kind: uze_extensions::view::ChartKind::Sparkline,
                levels: vec![125, 500, 1000, 0],
                height: 1,
                role: Role::Accent,
            },
        );
        let row = draw(&spark, 4, 1, 0, at(0)).rows()[0].clone();
        let cells: Vec<String> = row.chars().map(String::from).collect();
        assert_eq!(cells[0], levels[0], "{theme}: the lowest level above empty");
        assert_eq!(cells[2], *levels.last().unwrap(), "{theme}: the tallest");
        let bars = node(
            0,
            NodeKind::Chart {
                kind: uze_extensions::view::ChartKind::Bar,
                levels: vec![1000, 500, 0],
                height: 2,
                role: Role::Info,
            },
        );
        let rows = draw(&bars, 3, 2, 0, at(0)).rows();
        let full = levels.last().unwrap();
        assert!(rows[0].starts_with(full.as_str()), "{theme}: {rows:#?}");
        assert!(
            rows[1].starts_with(&format!("{full}{full}")),
            "{theme}: {rows:#?}"
        );
        let zeros = node(
            0,
            NodeKind::Chart {
                kind: uze_extensions::view::ChartKind::Sparkline,
                levels: vec![0, 0],
                height: 1,
                role: Role::Info,
            },
        );
        assert_eq!(
            draw(&zeros, 4, 1, 0, at(0)).rows()[0],
            "",
            "{theme}: nothing to draw"
        );
    });
}

#[test]
fn a_grid_reflows_its_tiles_with_the_width() {
    let tile = |id: usize| words(id, &format!("tile {id}"), Role::Default);
    let mut grid = node(0, NodeKind::Grid { gap: 2 });
    grid.min_width = 20;
    grid.children = (1..=4).map(tile).collect();
    let narrow = draw(&grid, 44, 3, 0, at(0)).rows();
    assert!(
        narrow[0].contains("tile 1") && narrow[0].contains("tile 2"),
        "{narrow:#?}"
    );
    assert!(
        narrow[2].contains("tile 3"),
        "two rows with a gap: {narrow:#?}"
    );
    let wide = draw(&grid, 90, 1, 0, at(0)).rows();
    assert!(wide[0].contains("tile 4"), "one row of four: {wide:#?}");
}

#[test]
fn details_align_their_keys_and_code_is_cut_not_wrapped() {
    let entry = |id, key: &str, value: &str| {
        node(
            id,
            NodeKind::Entry {
                key: key.into(),
                value: value.into(),
                role: Role::Default,
            },
        )
    };
    let mut details = node(0, NodeKind::Details);
    details.children = vec![entry(1, "Plan", "Max 5x"), entry(2, "Resets in", "1h54m")];
    let rows = draw(&details, 30, 2, 0, at(0)).rows();
    assert_eq!(rows[0].find("Max"), rows[1].find("1h54m"), "{rows:#?}");

    let code = node(
        0,
        NodeKind::Code {
            text: "let base = Duration::from_millis(100); // a long line of code\n".into(),
            language: "rust".into(),
        },
    );
    assert_eq!(height(&code, 24), 1, "one line of code is one row, cut");
}

#[test]
fn markdown_wraps_and_stops_at_its_cap() {
    let long: String = (0..400).map(|line| format!("- item {line}\n")).collect();
    let markdown = node(0, NodeKind::Markdown { text: long });
    assert_eq!(height(&markdown, 40), super::display::BLOCK_ROWS_MAX);
    let prose = node(
        1,
        NodeKind::Markdown {
            text: "Make payment retries back off and stop after five attempts.".into(),
        },
    );
    assert!(height(&prose, 20) > 1, "wrapped to its width");
}

fn select(id: usize, chosen: usize, focused: bool) -> Node {
    node(
        id,
        NodeKind::Select {
            label: "Sort".into(),
            options: vec!["Newest".into(), "Oldest".into()],
            chosen,
            focused,
        },
    )
}

fn field(id: usize, value: &str, focused: bool, editing: bool) -> Node {
    node(
        id,
        NodeKind::Field {
            label: "Filter".into(),
            value: value.into(),
            placeholder: "name".into(),
            focused,
            editing,
        },
    )
}

#[test]
fn a_select_is_a_forms_choice_whose_options_are_chosen_with_a_press() {
    in_both(|theme| {
        let drawn = draw(&select(3, 1, false), 40, 1, 0, at(0));
        let row = &drawn.rows()[0];
        assert!(row.starts_with("  Sort"), "{theme}: {row}");
        let choose = |index| {
            Some(ViewHit::Node {
                node: 3,
                act: NodeAct::Choose(index),
            })
        };
        let newest = drawn.column_of(0, "Newest").unwrap();
        let oldest = drawn.column_of(0, "Oldest").unwrap();
        assert_eq!(drawn.hit_at(newest, 0), choose(0), "{theme}");
        assert_eq!(drawn.hit_at(oldest, 0), choose(1), "{theme}");
        assert_eq!(drawn.hit_at(0, 0), None, "{theme}: the label is no target");
        assert_eq!(
            drawn.bg(oldest, 0),
            theme::color(Token::Accent),
            "{theme}: the chosen option is the accent"
        );
        let focused = draw(&select(3, 1, true), 40, 1, 0, at(0));
        assert!(
            focused.rows()[0].contains(&theme::glyph(Symbol::Prompt)),
            "{theme}: focused, it wears the keys' mark"
        );
    });
}

#[test]
fn a_select_too_wide_for_its_room_is_a_box_a_press_steps_on() {
    let drawn = draw(&select(3, 1, false), 18, 1, 0, at(0));
    let oldest = drawn.column_of(0, "Old").unwrap();
    assert_eq!(
        drawn.hit_at(oldest, 0),
        Some(ViewHit::Node {
            node: 3,
            act: NodeAct::Step { forward: true },
        })
    );
    assert_eq!(drawn.bg(oldest, 0), theme::color(Token::SurfaceRecessed));
}

#[test]
fn a_select_option_under_the_pointer_lights_without_changing_weight() {
    let at_rest = draw(&select(3, 1, false), 40, 1, 0, at(0));
    let newest = at_rest.column_of(0, "Newest").unwrap();
    let held = TreeFrame {
        hovered: Some(ViewHit::Node {
            node: 3,
            act: NodeAct::Choose(0),
        }),
        ..at(0)
    };
    let hovered = draw(&select(3, 1, false), 40, 1, 0, held);
    assert_ne!(hovered.bg(newest, 0), at_rest.bg(newest, 0));
    assert_eq!(
        hovered.buffer[(newest, 0)].modifier,
        at_rest.buffer[(newest, 0)].modifier
    );
}

#[test]
fn a_field_is_a_forms_box_with_its_caret_only_while_typed_into() {
    in_both(|theme| {
        let caret = theme::glyph(Symbol::CursorText);
        let resting = draw(&field(4, "", false, false), 30, 2, 0, at(0));
        assert_eq!(resting.rows()[0].trim_end(), "  Filter   name", "{theme}");
        let name = resting.column_of(0, "name").unwrap();
        assert_eq!(
            resting.bg(name, 0),
            theme::color(Token::SurfaceRecessed),
            "{theme}: the box says it is an input"
        );
        assert_eq!(
            resting.fg(name, 0),
            theme::color(Token::TextDim),
            "{theme}: the placeholder is quiet"
        );
        let edit = Some(ViewHit::Node {
            node: 4,
            act: NodeAct::Edit,
        });
        for (x, y) in [(0, 0), (12, 0), (29, 1)] {
            assert_eq!(resting.hit_at(x, y), edit, "{theme}: at {x},{y}");
        }

        let typing = draw(&field(4, "bil", true, true), 30, 2, 0, at(0));
        assert!(
            typing.rows()[0].contains(&format!("bil{caret}")),
            "{theme}: {:?}",
            typing.rows()
        );
        assert!(
            typing.rows()[0].contains(&theme::glyph(Symbol::Prompt)),
            "{theme}"
        );
    });
}

#[test]
fn a_form_draws_its_fields_then_a_button_that_sends_it() {
    in_both(|theme| {
        let form = holding(
            node(
                9,
                NodeKind::Form {
                    submit: "Comment".into(),
                    focused: false,
                },
            ),
            vec![field(10, "ok", false, false), select(11, 0, false)],
        );
        let drawn = draw(&form, 40, 5, 0, at(0));
        let rows = drawn.rows();
        assert!(rows[0].starts_with("  Filter"), "{theme}: {rows:?}");
        assert!(rows[2].starts_with("  Sort"), "{theme}: {rows:?}");
        // One label column for the form's keys: the box and the first
        // option start together.
        let column = |y: u16| {
            (0..40)
                .find(|x| {
                    let ground = drawn.bg(*x, y);
                    ground == theme::color(Token::SurfaceRecessed)
                        || ground == theme::color(Token::Accent)
                })
                .unwrap()
        };
        assert_eq!(column(0), column(2), "{theme}: {rows:?}");
        assert_eq!(rows[3], "  Comment", "{theme}");
        let send = Some(ViewHit::Node {
            node: 9,
            act: NodeAct::Press,
        });
        assert_eq!(drawn.hit_at(2, 3), send, "{theme}");
        assert_eq!(drawn.hit_at(0, 3), send, "{theme}");
        assert_eq!(
            drawn.fg(2, 3),
            theme::color(Token::TextSecondary),
            "{theme}"
        );
        let focused = holding(
            node(
                9,
                NodeKind::Form {
                    submit: "Comment".into(),
                    focused: true,
                },
            ),
            vec![field(10, "ok", false, false)],
        );
        assert_eq!(
            draw(&focused, 30, 4, 0, at(0)).fg(2, 2),
            theme::color(Token::Accent),
            "{theme}: focused, the button is the accent"
        );
    });
}

fn boxed(id: usize, layout: uze_extensions::view::BoxLayout) -> Node {
    node(id, NodeKind::Box(layout))
}

#[test]
fn a_row_box_of_percentages_wraps_five_to_a_line() {
    use uze_extensions::view::BoxLayout;
    let card = |id: usize| {
        holding(
            boxed(
                id,
                BoxLayout {
                    width: Some(uze_contract::catalog::Size::Percent(20)),
                    ..BoxLayout::default()
                },
            ),
            vec![words(id + 100, &format!("d{id}"), Role::Default)],
        )
    };
    let row = holding(
        boxed(
            0,
            BoxLayout {
                row: true,
                wrap: true,
                ..BoxLayout::default()
            },
        ),
        (1..=7).map(card).collect(),
    );
    let rows = draw(&row, 50, 3, 0, at(0)).rows();
    assert!(
        rows[0].contains("d1") && rows[0].contains("d5"),
        "{rows:#?}"
    );
    assert!(!rows[0].contains("d6"), "{rows:#?}");
    assert!(
        rows[1].contains("d6") && rows[1].contains("d7"),
        "{rows:#?}"
    );
    assert_eq!(
        rows[0].find("d2").unwrap() - rows[0].find("d1").unwrap(),
        10,
        "each a fifth of 50: {rows:#?}"
    );
}

#[test]
fn text_on_a_ground_takes_the_ink_that_reads_on_it() {
    use uze_extensions::view::{BoxLayout, Ground};
    let card = holding(
        boxed(
            0,
            BoxLayout {
                ground: Some(Ground {
                    role: Role::Info,
                    soft: false,
                }),
                pad_x: 1,
                ..BoxLayout::default()
            },
        ),
        vec![words(1, "13", Role::Faint)],
    );
    let drawn = draw(&card, 10, 1, 0, at(0));
    let cell = &drawn.buffer[(1, 0)];
    assert_eq!(cell.symbol(), "1");
    let ground = theme::color(Token::StateInfo);
    assert_eq!(cell.bg, ground, "the box's ground is the role's hue");
    assert_eq!(
        cell.fg,
        theme::ink_on(ground),
        "the faint role is not drawn on the ground"
    );
    assert_ne!(cell.fg, theme::color(Token::TextFaint));
    assert_eq!(drawn.buffer[(9, 0)].bg, ground, "the ground fills the box");
}

#[test]
fn a_full_line_progress_reads_as_a_line_not_a_slab() {
    let bar = node(
        0,
        NodeKind::Progress {
            line: true,
            segments: vec![segment(1, 1000, Role::Info)],
        },
    );
    let rows = draw(&bar, 8, 1, 0, at(0)).rows();
    let line = theme::glyph(Symbol::ProgressLine);
    assert_eq!(rows[0].trim_end(), line.repeat(8));
    let half = node(
        0,
        NodeKind::Progress {
            line: true,
            segments: vec![segment(1, 500, Role::Info)],
        },
    );
    let drawn = draw(&half, 8, 1, 0, at(0));
    assert_eq!(drawn.buffer[(0, 0)].fg, theme::color(Token::StateInfo));
    assert_eq!(
        drawn.buffer[(7, 0)].fg,
        theme::color(Token::TextFaint),
        "the track is quiet"
    );
}

/// The theme a glyph set makes of the default, the way `[appearance]
/// glyphs` puts it in force.
fn in_glyph_set(set: &str) -> uze_theme::Theme {
    let mut layers = vec![uze_theme::default_file()];
    layers.extend(uze_theme::glyph_set_file(set));
    uze_theme::resolve_stack(
        &uze_theme::Identity::from_file(set, uze_theme::default_file()),
        &layers,
    )
    .expect("the bundled set resolves")
    .theme
}

#[test]
fn an_icon_draws_in_the_glyph_set_the_operator_chose() {
    use uze_extensions::view::IconGlyph;
    let kennel = || IconGlyph::Declared {
        nerd: "\u{f0a43}".to_owned(),
        unicode: "⌂".to_owned(),
        ascii: "K".to_owned(),
    };
    let icons = || {
        row(
            0,
            1,
            Align::Start,
            vec![
                node(
                    1,
                    NodeKind::Icon {
                        glyph: IconGlyph::Host("clock".to_owned()),
                        role: Role::Info,
                    },
                ),
                node(
                    2,
                    NodeKind::Icon {
                        glyph: kennel(),
                        role: Role::Info,
                    },
                ),
                node(
                    3,
                    NodeKind::Icon {
                        glyph: IconGlyph::Unknown,
                        role: Role::Info,
                    },
                ),
                words(4, "end", Role::Muted),
            ],
        )
    };
    for (set, clock, declared) in [
        ("default", "◷", "⌂"),
        ("nerd", "\u{f017}", "\u{f0a43}"),
        ("ascii", "t", "K"),
    ] {
        uze_theme::with_active(in_glyph_set(set), || {
            let drawn = draw(&icons(), 20, 1, 0, at(0));
            // A name nothing knows draws no cell and takes no gap: the
            // label after it starts where it would without it.
            assert_eq!(
                drawn.rows()[0].trim_end(),
                format!("{clock} {declared} end"),
                "{set}"
            );
            assert_eq!(drawn.fg(0, 0), color(Role::Info), "{set}");
        });
    }
}

/// A cut line answers the pointer: resting on it, it holds still, then
/// slides sideways to show all of itself; off it, it is cut with an
/// ellipsis; `marquee=#false` keeps it cut and gives it no target.
#[test]
fn a_cut_line_slides_under_the_pointer_after_a_pause_unless_it_is_still() {
    let line = |still: bool| {
        node(
            7,
            NodeKind::Text {
                spans: vec![Span::new(
                    "Tokenize the card with the payment provider",
                    Role::Default,
                )],
                truncate: true,
                still,
            },
        )
    };
    let read = ViewHit::Node {
        node: 7,
        act: NodeAct::Read,
    };
    let at_rest = draw(&line(false), 20, 1, 0, at(0));
    assert_eq!(at_rest.rows()[0].trim_end(), "Tokenize the card w…");
    assert!(
        at_rest.hits.iter().any(|(_, hit)| *hit == read),
        "a target for the pointer"
    );
    assert!(!at_rest.tree.animates, "nothing moves at rest");

    let hovered = |rested| TreeFrame {
        hovered: Some(read),
        rested,
        ..at(0)
    };
    let arrived = draw(&line(false), 20, 1, 0, hovered(0));
    assert_eq!(
        arrived.rows()[0].trim_end(),
        "Tokenize the card wi",
        "the start, held still"
    );
    assert!(arrived.tree.animates, "the clock turns for it");
    let later = draw(&line(false), 20, 1, 0, hovered(40));
    assert!(
        !later.rows()[0].starts_with("Tokenize"),
        "it slid: {:?}",
        later.rows()[0]
    );

    let still = draw(&line(true), 20, 1, 0, hovered(40));
    assert_eq!(still.rows()[0].trim_end(), "Tokenize the card w…");
    assert!(still.hits.is_empty(), "no target for a still line");
}

/// A cut line in a list's row slides only where nothing else answers the
/// pointer: the row is what a click on its text chooses, and its target
/// comes first.
#[test]
fn a_cut_line_in_a_row_leaves_the_click_to_the_row() {
    let list = holding(
        node(
            1,
            NodeKind::List {
                selected: None,
                empty: None,
            },
        ),
        vec![node(
            2,
            NodeKind::Text {
                spans: vec![Span::new(
                    "Tokenize the card with the payment provider",
                    Role::Default,
                )],
                truncate: true,
                still: false,
            },
        )],
    );
    let drawn = draw(&list, 20, 1, 0, at(0));
    let first = drawn
        .hits
        .iter()
        .find(|(rect, _)| rect.contains(ratatui::layout::Position::new(4, 0)))
        .map(|(_, hit)| *hit);
    assert_eq!(
        first,
        Some(ViewHit::Node {
            node: 1,
            act: NodeAct::Row(0),
        }),
        "{:?}",
        drawn.hits
    );
}
