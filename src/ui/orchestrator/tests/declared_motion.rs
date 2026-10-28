//! What moves on a declared surface with no run behind it (parent 13.5):
//! an `Activity` and an `Elapsed` move with the host's clock from one
//! frame to the next while nothing asks the extension anything, and a
//! segmented `Progress` stays as its answer drew it.

use super::render::{self, FrameMetrics};
use super::support::*;
use super::{ExtensionAnswer, HostedExtensions, LoadedExtension, Owner, WorkspaceModel};
use crate::ui::extension_view::tree::stopped_at;
use crate::ui::theme::{self, Symbol};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension};

/// An instant on the host's clock, in milliseconds of the epoch, on a
/// beat of the activity mark.
const NOW_MS: u64 = 1_800_000_000_000;
/// When the run the row counts began: `12.9s` before [`NOW_MS`].
const SINCE_MS: u64 = NOW_MS - 12_900;
/// One beat of the activity mark later, and past the next whole second.
const LATER_MS: u64 = NOW_MS + 120;

fn identity() -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "proving".to_owned(),
        plugin: "gsd".to_owned(),
        id: "phase".to_owned(),
    }
}

/// A status row whose source composes its tree and runs nothing.
fn compiled() -> CompiledExtension {
    let mut source = uze_contract::SourceDecl::new(
        "phase",
        uze_contract::SourceKind::Fixture("fixtures/phase.json".to_owned()),
    );
    source.output = uze_contract::Output::View;
    CompiledExtension {
        identity: identity(),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "phase".to_owned(),
            name: "Phase".to_owned(),
            description: String::new(),
            surfaces: vec![uze_contract::SurfaceDecl {
                band: None,
                bar: None,
                id: "phase".to_owned(),
                slot: uze_contract::Slot::Status,
                view: "phase.kdl".to_owned(),
                source: "phase".to_owned(),
                title: None,
                applies_when: None,
            }],
            wants: Vec::new(),
            sources: vec![source],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
        },
        // Drawn until the source composes its own: nothing, here.
        views: BTreeMap::from([(
            "phase.kdl".to_owned(),
            uze_contract::CompiledView {
                file: "phase.kdl".to_owned(),
                items: Vec::new(),
                templates: BTreeMap::new(),
                lookups: Vec::new(),
            },
        )]),
    }
}

/// An agent in front, and the row answered once: a busy mark, the time
/// since the run began, and a bar of three phases.
fn answered() -> WorkspaceModel {
    let mut model = agent_session_in("/repo");
    model.remembered.declared = HostedExtensions {
        discovered_at: Some(Instant::now()),
        ..HostedExtensions::default()
    };
    let installed = InstalledExtension {
        identity: identity(),
        dir: PathBuf::from("/proving/plugins/gsd/extensions/phase"),
        linked: false,
    };
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed.clone()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![LoadedExtension {
            installed,
            stamp: 1,
            compiled: Ok(compiled()),
            data: BTreeMap::new(),
        }],
    }));
    let tree = serde_json::json!({
        "type": "Row",
        "props": {"gap": 1},
        "children": [
            {"type": "Activity", "props": {"busy": true}},
            {"type": "Elapsed", "props": {"since": SINCE_MS}},
            {"type": "Progress", "props": {"grow": true}, "children": [
                {"type": "Segment", "props": {"weight": 1, "ratio": 1.0, "role": "success"}},
                {"type": "Segment", "props": {"weight": 1, "ratio": 0.5}},
                {"type": "Segment", "props": {"weight": 1, "ratio": 0.0}}
            ]}
        ]
    });
    model
        .remembered
        .declared
        .engines
        .get_mut(&Owner::Declared(identity()))
        .expect("hosted")
        .set_data("phase", Ok(tree));
    model
}

/// The status row as the whole client draws it at `now_ms`, and whether
/// the frame asked for the next one.
fn row_at(model: &WorkspaceModel, now_ms: u64) -> (String, bool) {
    stopped_at(now_ms, || {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let mut metrics = FrameMetrics::default();
        terminal
            .draw(|frame| {
                render::render(
                    frame,
                    model,
                    &identities_fixture(),
                    &mut Vec::new(),
                    &mut metrics,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect();
        // Beneath the header and its rule, as every status row stands.
        (rows[2].clone(), metrics.marquee)
    })
}

/// Everything after the elapsed time: the bar, and what stands beside it.
fn bar(row: &str, elapsed: &str) -> String {
    let (_, after) = row.split_once(elapsed).expect("the time is drawn");
    after.to_owned()
}

#[test]
fn the_activity_mark_and_the_elapsed_time_move_with_no_run_and_the_bar_holds() {
    let model = answered();
    let (first, turns) = row_at(&model, NOW_MS);
    let (later, _) = row_at(&model, LATER_MS);

    let beat = |now_ms: u64| usize::try_from(now_ms / 120).unwrap();
    assert!(
        first.contains(&theme::frame(Symbol::ActivityBusy, beat(NOW_MS))),
        "{first}"
    );
    assert!(
        later.contains(&theme::frame(Symbol::ActivityBusy, beat(LATER_MS))),
        "{later}"
    );
    if theme::frames(Symbol::ActivityBusy).len() > 1 {
        assert_ne!(
            theme::frame(Symbol::ActivityBusy, beat(NOW_MS)),
            theme::frame(Symbol::ActivityBusy, beat(LATER_MS)),
            "the mark is on its next frame"
        );
    }
    assert!(first.contains("12s"), "{first}");
    assert!(later.contains("13s"), "{later}");
    assert_ne!(first, later, "the row moved");
    assert!(turns, "a frame that moves asks the loop for the next one");

    assert_eq!(
        bar(&first, "12s"),
        bar(&later, "13s"),
        "the bar is what the answer said, at every instant"
    );
    assert!(
        bar(&first, "12s").contains(&theme::frames(Symbol::ProgressFill).last().cloned().unwrap()),
        "the first phase is drawn full: {first}"
    );
}
