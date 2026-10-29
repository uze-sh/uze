//! Cells, images and sprites drawn: roles in the grid, a region as a
//! target, and one picture placed for each slot drawn in a frame.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::{Terminal, backend::TestBackend};
use uze_extensions::view::CellRegion;

use super::*;
use crate::ui::graphics::{self, Painter};

fn at(now_ms: u64) -> TreeFrame {
    TreeFrame {
        clock: Clock {
            now_ms,
            instant: Instant::now(),
        },
        hovered: None,
        rested: 0,
        fold: false,
    }
}

fn glyph(ch: char, role: Role) -> CellGlyph {
    CellGlyph { ch, role }
}

fn picture(width: usize, height: usize, pixel: [u8; 4]) -> Picture {
    Picture {
        width,
        height,
        pixels: vec![pixel; width * height].into(),
    }
}

#[test]
fn cells_are_drawn_in_their_roles_and_each_region_is_a_target() {
    let root = Node::new(
        4,
        NodeKind::Cells {
            rows: vec![
                vec![glyph('#', Role::Muted), glyph('o', Role::Danger)],
                vec![glyph('.', Role::Default), glyph('.', Role::Default)],
            ],
            regions: vec![CellRegion {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            }],
        },
    );
    let mut terminal = Terminal::new(TestBackend::new(10, 3)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render_tree(frame, frame.area(), 0, &root, at(0), &mut hits);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "#");
    assert_eq!(buffer[(0, 0)].fg, color(Role::Muted));
    assert_eq!(buffer[(1, 0)].symbol(), "o");
    assert_eq!(buffer[(1, 0)].fg, color(Role::Danger));
    let region = ViewHit::Node {
        node: 4,
        act: NodeAct::Region(0),
    };
    assert_eq!(hits, vec![(Rect::new(1, 0, 1, 2), region)]);

    terminal
        .draw(|frame| {
            let hovered = TreeFrame {
                hovered: Some(region),
                ..at(0)
            };
            render_tree(frame, frame.area(), 0, &root, hovered, &mut Vec::new());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(1, 0)].fg,
        theme::lifted_ink(color(Role::Danger)),
        "under the pointer, a region lifts in its own hue"
    );
    assert_eq!(buffer[(0, 0)].fg, color(Role::Muted), "and only it");
}

#[test]
fn an_image_not_yet_read_says_its_alt_and_one_refused_its_bound() {
    let image = |refused: Option<&str>| {
        Node::new(
            0,
            NodeKind::Image {
                picture: None,
                alt: "a cat".to_owned(),
                refused: refused.map(str::to_owned),
                height: 4,
            },
        )
    };
    for (node, said) in [
        (image(None), "a cat"),
        (
            image(Some("the picture is over 8 MiB")),
            "the picture is over 8 MiB",
        ),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(30, 4)).unwrap();
        terminal
            .draw(|frame| {
                render_tree(frame, frame.area(), 0, &node, at(0), &mut Vec::new());
            })
            .unwrap();
        let first: String = (0..30)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
            .collect();
        assert_eq!(first.trim_end(), said);
    }
}

/// A band's sprite and a section's picture drawn in one frame are each
/// placed for their own slot, each inside the cells it was drawn in, and
/// each sent.
#[test]
fn an_image_and_a_sprite_in_two_slots_are_both_placed_in_one_frame() {
    let image = Node::new(
        0,
        NodeKind::Image {
            picture: Some(picture(40, 20, [200, 0, 0, 255])),
            alt: String::new(),
            refused: None,
            height: 4,
        },
    );
    let sprite = Node::new(
        0,
        NodeKind::Sprite {
            frames: vec![
                picture(2, 2, [0, 200, 0, 255]),
                picture(2, 2, [0, 0, 200, 255]),
            ],
            fps: 4,
            looped: true,
            since: Some(Instant::now()),
            height: 2,
            refused: None,
        },
    );
    let (band, section) = (Rect::new(0, 0, 40, 2), Rect::new(0, 3, 40, 4));
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    graphics::take_placed();
    terminal
        .draw(|frame| {
            graphics::drawn_at((10, 20), || {
                graphics::in_slot(1, || {
                    render_tree(frame, band, 0, &sprite, at(0), &mut Vec::new())
                });
                graphics::in_slot(2, || {
                    render_tree(frame, section, 0, &image, at(0), &mut Vec::new())
                });
            });
        })
        .unwrap();
    let placed = graphics::take_placed();
    assert_eq!(
        placed.iter().map(|(slot, _)| *slot).collect::<Vec<_>>(),
        [1, 2],
        "one placement for each slot"
    );
    let buffer = terminal.backend().buffer();
    for (slot, placement) in &placed {
        let room = if *slot == 1 { band } else { section };
        assert!(
            room.intersection(placement.cells) == placement.cells && placement.uncovered_in(buffer),
            "slot {slot}: {:?} inside {room:?}",
            placement.cells
        );
    }
    let mut painter = Painter::default();
    assert!(painter.after_frame(&placed).is_empty(), "both encoding");
    let mut written: Vec<Arc<Vec<u8>>> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while written.len() < 2 && Instant::now() < deadline {
        written.extend(painter.take_ready());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(written.len(), 2, "each slot's picture is sent");
}

#[test]
fn a_sprite_shows_the_frame_of_its_state_on_the_clock() {
    let red = [200, 0, 0, 255];
    let blue = [0, 0, 200, 255];
    let began = Instant::now();
    let sprite = |since| {
        Node::new(
            0,
            NodeKind::Sprite {
                frames: vec![picture(1, 2, red), picture(1, 2, blue)],
                fps: 2,
                looped: false,
                since: Some(since),
                height: 1,
                refused: None,
            },
        )
    };
    let drawn_in = |node: &Node| {
        let mut terminal = Terminal::new(TestBackend::new(1, 1)).unwrap();
        let held = TreeFrame {
            clock: Clock {
                now_ms: 0,
                instant: began + Duration::from_millis(750),
            },
            hovered: None,
            rested: 0,
            fold: false,
        };
        terminal
            .draw(|frame| {
                render_tree(frame, frame.area(), 0, node, held, &mut Vec::new());
            })
            .unwrap();
        terminal.backend().buffer()[(0, 0)].fg
    };
    assert_eq!(
        drawn_in(&sprite(began)),
        theme::pixel(blue),
        "1.5 frames in"
    );
    assert_eq!(
        drawn_in(&sprite(began + Duration::from_millis(500))),
        theme::pixel(red),
        "a quarter second into a state begun later"
    );
}

#[test]
fn the_heat_roles_draw_the_themes_ramp_five_steps_apart() {
    let ramp = [
        Role::Heat0,
        Role::Heat1,
        Role::Heat2,
        Role::Heat3,
        Role::Heat4,
    ];
    let root = Node::new(
        1,
        NodeKind::Cells {
            rows: vec![ramp.iter().map(|role| glyph('■', *role)).collect()],
            regions: Vec::new(),
        },
    );
    let mut terminal = Terminal::new(TestBackend::new(5, 1)).unwrap();
    terminal
        .draw(|frame| {
            render_tree(frame, frame.area(), 0, &root, at(0), &mut Vec::new());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let drawn: Vec<_> = (0..5).map(|x| buffer[(x, 0)].fg).collect();
    let expected: Vec<_> = Token::HEAT
        .iter()
        .map(|token| theme::color(*token))
        .collect();
    assert_eq!(drawn, expected);
    for (index, step) in drawn.iter().enumerate() {
        assert!(
            !drawn[index + 1..].contains(step),
            "two steps share a colour"
        );
    }
}
