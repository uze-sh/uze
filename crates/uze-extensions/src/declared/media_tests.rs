//! Cells, images and sprites bound by the engine: a grid in roles whose
//! regions run actions, a picture read through the host one at a time,
//! and a sprite playing the state its field names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use uze_contract::{NodeName, SpriteDecl, SpriteState};

use super::tests::{SPACE, VIEW, act, compiled_view, extension, first, hosted, n, tree};
use super::*;
use crate::view::{CellGlyph, NodeKind};

/// A host holding pictures by the name a view gives them.
struct Showing {
    files: BTreeMap<String, Vec<u8>>,
}

impl crate::Host for Showing {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, _: &Path) -> Result<PathBuf, String> {
        Err("no git here".to_owned())
    }
    fn read_file(&self, _: &Path) -> Result<String, crate::Unreadable> {
        Err(crate::Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<crate::DirEntry>, String> {
        Ok(Vec::new())
    }
    fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn syntax_theme(&self) -> String {
        String::new()
    }
    fn media(&self, _: &uze_contract::Identity, name: &str) -> Result<Vec<u8>, String> {
        self.files
            .get(name)
            .cloned()
            .ok_or_else(|| format!("`{name}` is not there"))
    }
}

fn png(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let samples: Vec<u8> = (0..width * height).flat_map(|_| pixel).collect();
    writer.write_image_data(&samples).unwrap();
    writer.finish().unwrap();
    bytes
}

/// Runs every job the engine has now against `host`, as the pool would,
/// and answers how many ran.
fn read_all(engine: &mut DeclaredExtension, host: &Showing) -> usize {
    let jobs = engine.jobs();
    let count = jobs.len();
    for job in jobs {
        let answer = (job.run)(host);
        engine.absorb(answer);
    }
    count
}

#[test]
fn cells_draw_in_roles_and_a_colour_is_never_one() {
    let engine = hosted(
        vec![n(NodeName::Cells).prop("rows", "{grid}").item()],
        json!({ "grid": [
            "ab",
            [{ "ch": "x", "role": "danger" }, { "ch": "y", "role": "#ff0000" }, "\u{1b}"],
        ] }),
    );
    let cells = first(&tree(&engine), |kind| {
        matches!(kind, NodeKind::Cells { .. })
    });
    let NodeKind::Cells { rows, .. } = cells.kind else {
        unreachable!()
    };
    let glyph = |ch, role| CellGlyph { ch, role };
    assert_eq!(
        rows,
        vec![
            vec![glyph('a', Role::Default), glyph('b', Role::Default)],
            vec![
                glyph('x', Role::Danger),
                glyph('y', Role::Default),
                glyph(' ', Role::Default),
            ],
        ],
        "a colour value is no role, and a control character no glyph"
    );
}

#[test]
fn a_click_in_a_region_runs_its_action_with_its_item() {
    let mut engine = hosted(
        vec![
            n(NodeName::Cells)
                .prop("rows", "{grid}")
                .prop("regions", "{food}")
                .item(),
        ],
        json!({
            "grid": ["....", "...."],
            "food": [
                { "x": 0, "y": 0, "w": 1, "h": 1, "action": "skip", "item": null },
                { "x": 3, "y": 1, "w": 1, "h": 1, "action": "eat", "item": { "x": 3 } },
            ],
        }),
    );
    let cells = first(&tree(&engine), |kind| {
        matches!(kind, NodeKind::Cells { .. })
    });
    let NodeKind::Cells { regions, .. } = &cells.kind else {
        unreachable!()
    };
    assert_eq!(regions.len(), 2);
    engine.hit(VIEW, act(cells.id, NodeAct::Region(1)), SPACE);
    let pressed = engine
        .recorded()
        .last()
        .expect("the click reached an action");
    assert_eq!(
        (pressed.action.as_str(), &pressed.item),
        ("eat", &json!({ "x": 3 }))
    );
}

fn image(engine: &DeclaredExtension) -> NodeKind {
    first(&tree(engine), |kind| matches!(kind, NodeKind::Image { .. })).kind
}

#[test]
fn an_image_is_read_through_the_host_after_the_build_that_names_it() {
    let mut engine = hosted(
        vec![
            n(NodeName::Image)
                .arg("art/cat.png")
                .prop("alt", "a cat")
                .item(),
        ],
        json!({}),
    );
    let NodeKind::Image {
        picture,
        alt,
        height,
        ..
    } = image(&engine)
    else {
        unreachable!()
    };
    assert_eq!((picture, alt.as_str(), height), (None, "a cat", 8));
    let host = Showing {
        files: BTreeMap::from([("art/cat.png".to_owned(), png(3, 2, [1, 2, 3, 255]))]),
    };
    assert_eq!(read_all(&mut engine, &host), 1);
    let NodeKind::Image { picture, .. } = image(&engine) else {
        unreachable!()
    };
    let picture = picture.expect("drawn once read");
    assert_eq!((picture.width, picture.height), (3, 2));
    assert_eq!(read_all(&mut engine, &host), 0, "read once");
}

#[test]
fn an_oversized_image_says_its_bound_and_nothing_is_decoded() {
    let mut engine = hosted(vec![n(NodeName::Image).arg("huge.png").item()], json!({}));
    tree(&engine);
    let host = Showing {
        files: BTreeMap::from([("huge.png".to_owned(), vec![0; 20 * 1024 * 1024])]),
    };
    read_all(&mut engine, &host);
    let NodeKind::Image {
        picture, refused, ..
    } = image(&engine)
    else {
        unreachable!()
    };
    assert_eq!(picture, None);
    assert_eq!(refused.as_deref(), Some("the picture is over 8 MiB"));
}

#[test]
fn images_are_read_one_at_a_time() {
    let mut engine = hosted(
        vec![
            n(NodeName::Image).arg("a.png").item(),
            n(NodeName::Image).arg("b.png").item(),
        ],
        json!({}),
    );
    tree(&engine);
    let jobs = engine.jobs();
    assert_eq!(jobs.len(), 1, "one read out");
    assert!(engine.jobs().is_empty(), "the second waits for the first");
}

fn pet() -> SpriteDecl {
    let state = |name: &str, frames: Vec<u32>, looped| SpriteState {
        name: name.to_owned(),
        frames,
        fps: 4,
        looped,
    };
    SpriteDecl {
        id: "pet".to_owned(),
        sheet: "pet.png".to_owned(),
        frame: (2, 2),
        columns: 3,
        states: vec![
            state("idle", vec![0], true),
            state("working", vec![1, 2], true),
        ],
        fallback: "idle".to_owned(),
    }
}

fn sprite_engine(state: &str) -> DeclaredExtension {
    let view = compiled_view(
        vec![
            n(NodeName::Sprite)
                .arg("pet")
                .prop("state", "{state}")
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.sprites = vec![pet()];
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.set_data(super::tests::SOURCE, Ok(json!({ "state": state })));
    engine
}

fn sprite(engine: &DeclaredExtension) -> NodeKind {
    first(&tree(engine), |kind| {
        matches!(kind, NodeKind::Sprite { .. })
    })
    .kind
}

#[test]
fn a_sprite_plays_the_state_its_field_names_and_the_fallback_otherwise() {
    let host = Showing {
        files: BTreeMap::from([("pet.png".to_owned(), png(6, 2, [9, 9, 9, 255]))]),
    };
    let mut working = sprite_engine("working");
    tree(&working);
    read_all(&mut working, &host);
    let NodeKind::Sprite {
        frames,
        fps,
        looped,
        height,
        ..
    } = sprite(&working)
    else {
        unreachable!()
    };
    assert_eq!((frames.len(), fps, looped, height), (2, 4, true, 1));

    let mut lost = sprite_engine("sleeping");
    tree(&lost);
    read_all(&mut lost, &host);
    let NodeKind::Sprite { frames, .. } = sprite(&lost) else {
        unreachable!()
    };
    assert_eq!(frames.len(), 1, "idle, the fallback");
}

#[test]
fn a_state_keeps_playing_from_where_it_began_across_builds() {
    let mut engine = sprite_engine("working");
    let began = Instant::now();
    let pass = |engine: &mut DeclaredExtension, now| {
        engine.pass(&crate::contract::Pass {
            now,
            visible: &[],
            happened: &[],
        });
    };
    pass(&mut engine, began);
    let since_of = |engine: &DeclaredExtension| match sprite(engine) {
        NodeKind::Sprite { since, .. } => since,
        _ => unreachable!(),
    };
    assert_eq!(since_of(&engine), Some(began));
    pass(&mut engine, began + Duration::from_secs(1));
    engine.set_data(
        super::tests::SOURCE,
        Ok(json!({ "state": "working", "tick": 1 })),
    );
    assert_eq!(since_of(&engine), Some(began), "the same state goes on");
    engine.set_data(super::tests::SOURCE, Ok(json!({ "state": "idle" })));
    assert_eq!(
        since_of(&engine),
        Some(began + Duration::from_secs(1)),
        "a new state starts when it is seen"
    );
}
