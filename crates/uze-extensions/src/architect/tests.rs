use std::collections::BTreeSet;

use super::*;

const SPACE: Size = Size {
    width: 100,
    height: 30,
};

/// uze's own architecture, as the repository keeps it. Living fixtures:
/// what is checked here is also that the project's real artifacts draw.
// The C4 views, by file: two of them share the title `uze`, since a
// title names a scope and both are views of the system.
const CONTEXT: &str = "system-context.mmd";
const CONTAINERS: &str = "containers.mmd";
const CORE: &str = "core-components.mmd";
const WORKSPACE: &str = "workspace-components.mmd";

fn opened() -> ArchitectView {
    filled(ArchitectView::opening("~/project".to_owned()))
}

/// The same artifacts, read into a surface that may already have been
/// told where the viewer left off.
fn filled(mut state: ArchitectView) -> ArchitectView {
    let artifacts = vec![
        Artifact::read(
            "containers.mmd",
            include_str!("../../../../docs/architecture/containers.mmd"),
        ),
        Artifact::read(
            "system-context.mmd",
            include_str!("../../../../docs/architecture/system-context.mmd"),
        ),
        Artifact::read(
            "install-sequence.mmd",
            include_str!("../../../../docs/architecture/install-sequence.mmd"),
        ),
        Artifact::read(
            "crate-layering.mmd",
            include_str!("../../../../docs/architecture/crate-layering.mmd"),
        ),
        Artifact::read(
            "install-pipeline.mmd",
            include_str!("../../../../docs/architecture/install-pipeline.mmd"),
        ),
        Artifact::read(
            "core-components.mmd",
            include_str!("../../../../docs/architecture/core-components.mmd"),
        ),
        Artifact::read(
            "workspace-components.mmd",
            include_str!("../../../../docs/architecture/workspace-components.mmd"),
        ),
        Artifact::read(
            "agent-lifecycle.mmd",
            include_str!("../../../../docs/architecture/agent-lifecycle.mmd"),
        ),
        Artifact::read(
            "attachment-lifecycle.mmd",
            include_str!("../../../../docs/architecture/attachment-lifecycle.mmd"),
        ),
    ];
    state.absorb(ArtifactsAnswer {
        branch: "main".to_owned(),
        artifacts: Artifacts::Found {
            artifacts,
            project: PathBuf::from("/project"),
        },
    });
    state
}

/// The box `id` names on the diagram on show.
fn node_named_in(state: &ArchitectView, id: &str) -> Option<usize> {
    let Drawing::Graph(scene) = &state.drawing else {
        return None;
    };
    scene.graph.nodes.iter().position(|node| node.id == id)
}

fn showing(name: &str) -> ArchitectView {
    let mut state = opened();
    let index = state
        .catalog
        .artifacts()
        .iter()
        .position(|artifact| artifact.name == name || artifact.origin == name)
        .expect("the artifact exists");
    state.open(index);
    state
}

fn drawn(state: &ArchitectView) -> String {
    state.canvas.as_ref().expect("it draws").to_text()
}

/// Run with `--nocapture` to read the diagrams: this is the quickest way
/// to look at what the layout produced without opening the TUI.
#[test]
fn every_artifact_is_drawn_with_every_edge_routed() {
    let mut state = opened();
    for index in 0..state.catalog.artifacts().len() {
        state.open(index);
        let name = state.catalog.get(index).unwrap().name.clone();
        if let Drawing::Graph(scene) = &state.drawing {
            assert_eq!(scene.routes.unrouted, 0, "{name} left an edge out");
        }
        let text = drawn(&state);
        println!("\n=== {name} ===\n{text}");
        assert!(text.lines().count() > 5);
    }
}

#[test]
fn ascii_draws_the_same_diagram_in_seven_bit_characters() {
    let mut state = opened();
    state.show(Showing::Ascii);
    for index in 0..state.catalog.artifacts().len() {
        state.open(index);
        let foreign: Vec<char> = drawn(&state)
            .chars()
            .filter(|c| !c.is_ascii() && !c.is_alphanumeric() && *c != '·' && *c != '~')
            .collect();
        assert!(foreign.is_empty(), "{foreign:?}");
    }
}

#[test]
fn the_screen_is_exactly_the_space_it_was_given() {
    let state = opened();
    let Content::Lines { lines, scroll, .. } = view(&state, SPACE).content else {
        panic!("a diagram is lines");
    };
    assert_eq!((lines.len(), scroll), (30, 0));
    for line in &lines {
        let width: i32 = line
            .spans
            .iter()
            .map(|s| crate::shared::canvas::text_width(&s.text))
            .sum();
        assert!(width <= 100, "{width} columns would be cut");
    }
}

#[test]
fn a_drawing_smaller_than_the_screen_opens_in_the_middle_of_it() {
    let state = showing(CONTEXT);
    let wide = Size {
        width: 200,
        height: 30,
    };
    assert_eq!(state.corner(wide).0, (state.board_size().0 - 200) / 2);
}

#[test]
fn every_edge_of_the_board_can_be_brought_to_the_middle_of_the_screen() {
    let mut state = opened();
    let board = state.board_size();
    drag_by(&mut state, 5000, 5000, SPACE);
    assert_eq!(state.corner(SPACE), (-50, -15));
    drag_by(&mut state, -50_000, -50_000, SPACE);
    assert_eq!(state.corner(SPACE), (board.0 - 50, board.1 - 15));
}

#[test]
fn a_board_that_fits_the_screen_still_moves() {
    let mut state = showing(CONTEXT);
    let wide = Size {
        width: 200,
        height: 30,
    };
    let home = state.corner(wide);
    drag_by(&mut state, 20, 0, wide);
    assert_eq!(state.corner(wide), (home.0 - 20, home.1));
}

#[test]
fn a_click_on_the_minimap_brings_that_part_of_the_board_to_the_screen() {
    let mut state = opened();
    let home = state.corner(SPACE);
    let map = state.minimap(SPACE).expect("the screen has room for a map");
    let hit = ViewHit::PlaceCaret {
        line: (map.frame.y + map.frame.h - 2) as usize,
        cell: (map.frame.x + map.frame.w / 2) as usize,
    };
    handle_mouse(&mut state, Some(hit), SPACE);
    assert!(
        state.corner(SPACE).1 > home.1 + 10,
        "{:?}",
        state.corner(SPACE)
    );
    assert_eq!(state.picked, None);
}

#[test]
fn the_artifacts_go_round_and_an_area_opens_on_its_first() {
    let mut state = opened();
    let last = state.catalog.artifacts().len() - 1;
    handle_command(&mut state, Command::PreviousView, SPACE);
    assert_eq!(state.selected, last);
    handle_command(&mut state, Command::NextView, SPACE);
    assert_eq!(state.selected, 0);
    let View { navigator, .. } = view(&state, SPACE);
    let areas: Vec<usize> = navigator
        .unwrap()
        .rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Group { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(areas.len(), 3);
    handle_mouse(&mut state, Some(ViewHit::ToggleGroup(areas[2])), SPACE);
    assert_eq!(state.selected, areas[2]);
}

#[test]
fn clicking_a_box_selects_it_and_clicking_it_again_lets_go() {
    let mut state = showing("Crate layering");
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("the layering is a graph");
    };
    let frame = scene.placement.nodes[0];
    let corner = state.corner(SPACE);
    let hit = ViewHit::PlaceCaret {
        line: (frame.y + 1 - corner.1) as usize,
        cell: (frame.x + 1 - corner.0) as usize,
    };
    handle_mouse(&mut state, Some(hit), SPACE);
    assert_eq!(state.picked, Some(0));
    handle_mouse(&mut state, Some(hit), SPACE);
    assert_eq!(state.picked, None);
}

#[test]
fn a_surface_with_nothing_to_draw_says_why_and_what_to_do() {
    let mut state = ArchitectView::opening("~/project".to_owned());
    let Content::Message { hint, .. } = view(&state, SPACE).content else {
        panic!("a read in flight is a message");
    };
    assert_eq!(hint, None);

    struct Bare;
    impl Host for Bare {
        fn git(&self, _: &std::path::Path, _: &[&str], _: &[i32]) -> Result<String, String> {
            Err("no git here".to_owned())
        }
        fn repository_root(&self, _: &std::path::Path) -> Result<PathBuf, String> {
            Err("no git here".to_owned())
        }
        fn read_file(&self, _: &std::path::Path) -> Result<String, crate::Unreadable> {
            Err(crate::Unreadable::Failed("no such file".to_owned()))
        }
        fn list_dir(&self, _: &std::path::Path) -> Result<Vec<crate::DirEntry>, String> {
            Ok(Vec::new())
        }
        fn write_file(
            &self,
            _root: &std::path::Path,
            _: &std::path::Path,
            _: &str,
        ) -> Result<(), String> {
            Ok(())
        }
        fn delete_file(&self, _root: &std::path::Path, _: &std::path::Path) -> Result<(), String> {
            Ok(())
        }
        fn restore_to_head(
            &self,
            _: &std::path::Path,
            _: &[std::path::PathBuf],
        ) -> Result<(), String> {
            Ok(())
        }
        fn syntax_theme(&self) -> String {
            String::new()
        }
    }
    state.absorb(read_artifacts(
        &Bare,
        Path::new("/project"),
        ArtifactSource::Undeclared,
    ));
    let Content::Message { text, hint, .. } = view(&state, SPACE).content else {
        panic!("an undeclared project is a message");
    };
    assert!(text.contains("No artifacts declared"));
    assert!(hint.unwrap().contains("uze:architect"));

    let empty = ArtifactSource::Directories {
        roots: vec![ArtifactRoot {
            path: PathBuf::from("/project/docs/diagrams"),
            declared: "docs/diagrams".to_owned(),
        }],
        project: PathBuf::from("/project"),
    };
    state.absorb(read_artifacts(&Bare, Path::new("/project"), empty));
    let Content::Message { text, .. } = view(&state, SPACE).content else {
        panic!("an empty directory is a message");
    };
    assert!(text.contains("docs/diagrams"), "{text}");
    assert!(
        view(&state, SPACE).footer.is_empty(),
        "no keys stand under a message with nothing to act on"
    );

    // A manifest error is a paragraph: it goes under a short headline,
    // never in place of one.
    let reason = "malformed agents.yaml at /project/agents.yaml: unknown field `worktrees`";
    state.absorb(read_artifacts(
        &Bare,
        Path::new("/project"),
        ArtifactSource::Refused(reason.to_owned()),
    ));
    let Content::Message { text, hint, .. } = view(&state, SPACE).content else {
        panic!("a refused declaration is a message");
    };
    assert_eq!(text, "agents.yaml needs fixing");
    assert!(hint.unwrap().contains(reason));
}

#[test]
fn the_list_of_artifacts_opens_on_the_one_on_show_and_steps_over_to_the_areas() {
    let mut state = showing(CORE);
    let areas = state.areas();
    handle_command(&mut state, Command::ChooseItem, SPACE);
    assert_eq!(state.choosing, Some(Choosing::Item(state.selected)));
    handle_command(&mut state, Command::Pan(PanDirection::Down), SPACE);
    handle_command(&mut state, Command::Pan(PanDirection::Down), SPACE);
    assert_eq!(
        state.choosing,
        Some(Choosing::Item(state.selected)),
        "two views share the CLI, so two steps down is where it started"
    );

    handle_command(&mut state, Command::Pan(PanDirection::Left), SPACE);
    assert_eq!(
        state.choosing,
        Some(Choosing::Group(areas[0])),
        "left is the areas"
    );
    handle_command(&mut state, Command::Pan(PanDirection::Down), SPACE);
    handle_command(&mut state, Command::Activate, SPACE);
    assert_eq!((state.selected, state.choosing), (areas[1], None));
}

#[test]
fn a_list_offers_only_the_area_on_show() {
    let state = showing("Crate layering");
    let names: Vec<&str> = state
        .choices()
        .into_iter()
        .map(|artifact| state.catalog.get(artifact).unwrap().name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Agent lifecycle",
            "Attachment lifecycle",
            "Crate layering",
            "Install pipeline"
        ]
    );
}

#[test]
fn leaving_a_list_leaves_the_surface_open() {
    let mut state = showing(CORE);
    handle_command(&mut state, Command::ChooseItem, SPACE);
    let outcome = handle_command(&mut state, Command::Close, SPACE);
    assert_eq!((outcome, state.choosing), (ArchitectOutcome::Stay, None));

    handle_mouse(&mut state, Some(ViewHit::ChooseGroup), SPACE);
    assert!(matches!(state.choosing, Some(Choosing::Group(_))));
    handle_mouse(&mut state, Some(ViewHit::ChooseItem), SPACE);
    assert!(
        matches!(state.choosing, Some(Choosing::Item(_))),
        "the other selector takes over rather than only shutting this one"
    );

    let before = state.selected;
    let board = ViewHit::PlaceCaret { line: 1, cell: 1 };
    handle_mouse(&mut state, Some(board), SPACE);
    assert_eq!(
        (state.selected, state.choosing, state.picked),
        (before, None, None),
        "a click off the list shuts it and does nothing else"
    );

    handle_mouse(&mut state, Some(ViewHit::ChooseItem), SPACE);
    handle_mouse(&mut state, Some(ViewHit::SelectItem(before + 1)), SPACE);
    assert_eq!((state.selected, state.choosing), (before + 1, None));
}

fn pick(state: &mut ArchitectView, alias: &str) {
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    state.picked = scene.graph.nodes.iter().position(|node| node.id == alias);
    assert!(state.picked.is_some(), "`{alias}` is drawn here");
}

/// The way down the menu shows, as names.
fn trail(state: &ArchitectView) -> Vec<String> {
    view(state, SPACE)
        .trail
        .into_iter()
        .map(|step| step.name)
        .collect()
}

/// The file of the view on show.
fn here(state: &ArchitectView) -> String {
    state
        .catalog
        .get(state.selected)
        .map(|artifact| artifact.origin.clone())
        .unwrap_or_default()
}

#[test]
fn the_views_are_joined_by_zoom_from_the_context_down_to_the_code() {
    let mut state = showing(CONTEXT);
    assert_eq!(trail(&state), ["Context"], "the root has no way down to it");

    pick(&mut state, "uze");
    assert_eq!(
        handle_command(&mut state, Command::Activate, SPACE),
        ArchitectOutcome::Stay
    );
    assert_eq!(trail(&state), ["Context", "Containers"]);

    pick(&mut state, "cli");
    handle_command(&mut state, Command::Activate, SPACE);
    assert_eq!(trail(&state), ["Context", "Containers", "Components"]);
    assert_eq!(
        here(&state),
        CORE,
        "the first of the views that zoom into the CLI"
    );

    pick(&mut state, "package");
    assert_eq!(
        handle_command(&mut state, Command::Activate, SPACE),
        ArchitectOutcome::OpenPath {
            project: PathBuf::from("/project"),
            target: PathBuf::from("/project/crates/uze-core/src/package.rs"),
        },
        "below a component is the code itself"
    );
}

/// C4 splits a large container's components over several views; they
/// share the container's place in the tree, and its step offers them.
#[test]
fn views_that_zoom_into_one_container_are_offered_on_its_step() {
    let mut state = showing(CORE);
    let step = view(&state, SPACE).trail.pop().expect("a way down");
    let offered: Vec<&str> = step.choices.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(offered, ["uze CLI: uze-core", "uze CLI: uze-workspace"]);

    handle_mouse(&mut state, Some(ViewHit::ChooseItem), SPACE);
    assert_eq!(state.choosing, Some(Choosing::Item(state.selected)));
    handle_mouse(
        &mut state,
        Some(ViewHit::SelectItem(step.choices[1].id)),
        SPACE,
    );
    assert_eq!(trail(&state), ["Context", "Containers", "Components"]);
    assert_eq!(here(&state), WORKSPACE);
}

#[test]
fn coming_back_puts_the_viewer_on_the_box_that_led_down() {
    let mut state = showing(CORE);
    handle_command(&mut state, Command::Back, SPACE);
    assert_eq!(here(&state), CONTAINERS);
    assert_eq!(
        state.picked,
        node_named_in(&state, "cli"),
        "the box the view below zooms into is picked"
    );

    handle_command(&mut state, Command::NextView, SPACE);
    handle_mouse(&mut state, Some(ViewHit::SelectTrail(0)), SPACE);
    assert_eq!(here(&state), CONTEXT);
    assert_eq!(state.picked, node_named_in(&state, "uze"));
}

/// The way down is the view's own ancestors, however it was reached.
#[test]
fn a_view_opened_from_anywhere_shows_its_own_way_down() {
    let mut state = showing(CONTEXT);
    handle_command(&mut state, Command::NextView, SPACE);
    handle_command(&mut state, Command::NextView, SPACE);
    assert_eq!(trail(&state), ["Context", "Containers", "Components"]);
    assert_eq!(here(&state), CORE);
}

/// The way down names levels, which are short; the view's own title,
/// which names its type and scope, is the view's label.
#[test]
fn a_c4_view_is_labelled_by_its_title_and_stepped_to_by_its_level() {
    let state = showing(CORE);
    let mut drawn = view(&state, SPACE);
    assert_eq!(drawn.label, "uze CLI: uze-core");
    assert_eq!(drawn.trail.pop().unwrap().name, "Components");
    assert_eq!(
        view(&showing("Crate layering"), SPACE).label,
        "",
        "off C4 the selector names it"
    );
}

#[test]
fn the_keys_walk_from_box_to_box() {
    let mut state = showing(CONTEXT);
    handle_command(&mut state, Command::SelectToward(PanDirection::Down), SPACE);
    let first = state
        .picked
        .expect("with nothing picked, the nearest box is");
    handle_command(&mut state, Command::SelectToward(PanDirection::Down), SPACE);
    let second = state.picked.unwrap();
    assert_ne!(first, second);
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("the context is a graph");
    };
    let frames = &scene.placement.nodes;
    assert!(
        frames[second].center().1 > frames[first].center().1,
        "down is down"
    );
}

/// The key that closes peels one level at a time. Anything else makes a
/// surface that can be entered but not looked around in: one press and
/// the viewer is back where they started with three levels of work gone.
#[test]
fn the_key_that_closes_goes_up_a_level_until_there_is_none_left() {
    let mut state = showing(CONTEXT);
    pick(&mut state, "uze");
    handle_command(&mut state, Command::Activate, SPACE);
    pick(&mut state, "cli");
    handle_command(&mut state, Command::Activate, SPACE);
    assert_eq!(here(&state), CORE);

    for left in [CONTAINERS, CONTEXT] {
        assert_eq!(
            handle_command(&mut state, Command::Close, SPACE),
            ArchitectOutcome::Stay
        );
        // Coming back selects the box that was entered, which is a level
        // of its own — the next press lets go of it, the one after goes up.
        assert_eq!(
            handle_command(&mut state, Command::Close, SPACE),
            ArchitectOutcome::Stay
        );
        assert_eq!(here(&state), left);
    }
    assert_eq!(
        handle_command(&mut state, Command::Close, SPACE),
        ArchitectOutcome::Close,
        "and only there does it close"
    );
}

/// The way up is named in the footer only where there is one.
#[test]
fn the_footer_offers_the_way_up_only_once_something_has_been_entered() {
    let mut state = showing(CONTEXT);
    assert!(!view(&state, SPACE).footer.contains(&Command::Back));

    pick(&mut state, "uze");
    handle_command(&mut state, Command::Activate, SPACE);
    assert!(view(&state, SPACE).footer.contains(&Command::Back));
}

#[test]
fn a_selector_with_nothing_to_choose_stays_shut() {
    let lone = Artifact::read(
        "install-sequence.mmd",
        include_str!("../../../../docs/architecture/install-sequence.mmd"),
    );
    let mut state = ArchitectView::opening("~/project".to_owned());
    state.absorb(ArtifactsAnswer {
        branch: "main".to_owned(),
        artifacts: Artifacts::Found {
            artifacts: vec![lone],
            project: PathBuf::from("/project"),
        },
    });
    handle_command(&mut state, Command::ChooseGroup, SPACE);
    assert_eq!(state.choosing, None, "one area");
    handle_command(&mut state, Command::ChooseItem, SPACE);
    assert_eq!(state.choosing, None, "one artifact in it");

    let mut state = opened();
    handle_command(&mut state, Command::ChooseGroup, SPACE);
    assert!(state.choosing.is_some(), "four areas is a choice");
}

/// While a list is open the highlight follows the pointer: hovering a
/// row is highlighting it, which is what makes it read as a menu.
#[test]
fn hovering_a_row_of_an_open_list_highlights_it() {
    let mut state = showing("Crate layering");
    handle_command(&mut state, Command::ChooseItem, SPACE);
    let Some(Choosing::Item(first)) = state.choosing else {
        panic!("the artifacts of the area are offered");
    };
    let other = *state
        .choices()
        .iter()
        .find(|&&artifact| artifact != first)
        .expect("the area holds two");

    assert!(handle_hover(&mut state, Some(ViewHit::SelectItem(other))));
    assert_eq!(state.choosing, Some(Choosing::Item(other)));
    assert!(
        !handle_hover(&mut state, Some(ViewHit::SelectItem(other))),
        "standing still costs no frame"
    );
    assert_eq!(state.selected, first, "hovering chooses nothing");

    state.choosing = None;
    assert!(
        !handle_hover(&mut state, Some(ViewHit::SelectItem(other))),
        "and with no list open it is the board's pointer, not a menu's"
    );
}

/// A boundary can only be read as one if a line through it means
/// something. Crossing it is what an edge with an end inside does; an
/// edge with neither end inside goes around.
#[test]
fn an_edge_with_no_business_in_a_region_stays_out_of_it() {
    let Diagram::Graph(graph) = mermaid::parse(
        "flowchart TD\n a --> m --> z\n a --> z\n subgraph region [Region]\n m\n end",
    )
    .expect("it parses") else {
        panic!("a flowchart is a graph");
    };
    let scene = Scene::of(graph);
    let region = scene.placement.clusters[0];
    let past = scene
        .routes
        .routes
        .iter()
        .find(|route| {
            let edge = &scene.graph.edges[route.edge];
            scene.graph.nodes[edge.from].cluster.is_none()
                && scene.graph.nodes[edge.to].cluster.is_none()
        })
        .expect("the edge that skips the region is routed");
    for &(x, y, _) in &past.cells {
        assert!(
            !region.contains(x, y),
            "({x},{y}) is inside a region the edge has no end in"
        );
    }
}

/// The board's grid is the texture of having nothing on it, so a region
/// stands on a ground of its own — and a region inside a region takes
/// its parent's ground back, which is what tells two nested walls apart.
#[test]
fn a_region_stands_on_its_own_ground() {
    let outer = Frame {
        x: 0,
        y: 0,
        w: 20,
        h: 20,
    };
    let inner = Frame {
        x: 5,
        y: 5,
        w: 5,
        h: 5,
    };
    assert!(grounded(&[outer, inner], (30, 30)));
    assert!(!grounded(&[outer, inner], (2, 2)));
    assert!(grounded(&[outer, inner], (6, 6)));
}

/// Leaving the surface and coming back is coming back: the diagram, the
/// levels entered to reach it, the box selected and where the board was
/// moved to are all where they were left.
#[test]
fn coming_back_stands_where_the_viewer_stood() {
    let mut left = showing(CONTAINERS);
    left.picked = node_named_in(&left, "cli");
    let outcome = left.enter();
    assert_eq!(outcome, ArchitectOutcome::Stay, "the CLI goes inside");
    left.picked = node_named_in(&left, "delivery");
    left.corner = Some((12, 34));

    let back = filled(ArchitectView::opening("~/project".to_owned()).resuming(left.place()));
    assert_eq!(back.selected, left.selected);
    assert_eq!(back.picked, left.picked);
    assert_eq!(back.corner, Some((12, 34)));
}

/// A diagram that was deleted while the surface was shut is not an error
/// and not an empty board: the place is simply not restored.
#[test]
fn a_place_naming_a_diagram_that_is_gone_opens_at_the_top() {
    let place = ArchitectPlace {
        artifact: "vanished.mmd".to_owned(),
        ..ArchitectPlace::default()
    };
    let back = filled(ArchitectView::opening("~/project".to_owned()).resuming(place));
    assert_eq!(back.selected, 0);
}

/// The frame's two edges: what the surface is at the top, where it is
/// open at the foot — the same two the code surface draws, because a
/// reader switching between them is asking one question.
#[test]
fn the_frame_says_what_this_is_on_top_and_where_it_is_at_the_foot() {
    let mut state = opened();
    state.display_root = "~/uze/.worktrees/joipv0".to_owned();
    state.branch = "feat/thing".to_owned();
    let drawn = view(&state, SPACE);

    let said = |spans: &[Span]| {
        spans
            .iter()
            .map(|span| span.text.clone())
            .collect::<String>()
    };
    assert_eq!(
        said(&drawn.title),
        CATALOG.name,
        "the name it is registered under"
    );
    assert_eq!(said(&drawn.caption), "~/uze/.worktrees/joipv0 · feat/thing");

    let weight = |spans: &[Span], text: &str| {
        spans
            .iter()
            .find(|span| span.text == text)
            .map(|span| (span.role, span.bold))
    };
    assert_eq!(
        weight(&drawn.title, CATALOG.name),
        Some((Role::Muted, false)),
        "the surface's name is a label, said once and quietly"
    );
    assert_eq!(
        weight(&drawn.caption, "~/uze/.worktrees/"),
        Some((Role::Dim, false))
    );
    assert_eq!(weight(&drawn.caption, "joipv0"), Some((Role::Bright, true)));
    assert_eq!(
        weight(&drawn.caption, "feat/thing"),
        Some((Role::Accent, true))
    );
}

/// A directory of diagrams, served from memory. The check reads the same
/// two things the surface does — a listing and a file — so a host that
/// answers both is the whole world it needs.
struct Written(Vec<(&'static str, &'static str)>);

impl Host for Written {
    fn git(&self, _: &std::path::Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("not asked".to_owned())
    }
    fn repository_root(&self, _: &std::path::Path) -> Result<PathBuf, String> {
        Err("not asked".to_owned())
    }
    fn read_file(&self, path: &std::path::Path) -> Result<String, crate::Unreadable> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.0
            .iter()
            .find(|(written, _)| *written == name)
            .map(|(_, source)| (*source).to_owned())
            .ok_or_else(|| crate::Unreadable::Failed("no such file".to_owned()))
    }
    fn list_dir(&self, _: &std::path::Path) -> Result<Vec<crate::DirEntry>, String> {
        Ok(self
            .0
            .iter()
            .map(|(name, _)| crate::DirEntry {
                directory: false,
                name: (*name).to_owned(),
            })
            .collect())
    }
    fn write_file(
        &self,
        _root: &std::path::Path,
        _: &std::path::Path,
        _: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    fn delete_file(&self, _root: &std::path::Path, _: &std::path::Path) -> Result<(), String> {
        Ok(())
    }
    fn restore_to_head(&self, _: &std::path::Path, _: &[std::path::PathBuf]) -> Result<(), String> {
        Ok(())
    }
    fn syntax_theme(&self) -> String {
        String::new()
    }
    fn artifact_source(&self, _project: &std::path::Path) -> ArtifactSource {
        declared()
    }
}

fn declared() -> ArtifactSource {
    ArtifactSource::Directories {
        roots: vec![ArtifactRoot {
            path: PathBuf::from("/project/docs/architecture"),
            declared: "docs/architecture".to_owned(),
        }],
        project: PathBuf::from("/project"),
    }
}

#[test]
fn a_check_reports_every_artifact_by_what_drawing_it_found() {
    let checkup = check(
        &Written(vec![
            ("fine.mmd", "flowchart LR\n  a[A] --> b[B]\n"),
            ("roadmap.mmd", "gantt\n  title Roadmap\n"),
            ("half.mmd", "C4Context\n  Person(a, \"A\")\n  Rel(a)\n"),
        ]),
        declared(),
    );
    let Checkup::Checked {
        declared,
        artifacts,
    } = checkup
    else {
        panic!("a directory of diagrams is checked");
    };
    assert_eq!(declared, "docs/architecture");
    let verdict = |origin: &str| {
        artifacts
            .iter()
            .find(|artifact| artifact.origin == origin)
            .map(|artifact| artifact.verdict.clone())
            .expect("every file is reported")
    };
    assert_eq!(verdict("fine.mmd"), Verdict::Drawn);
    // Quoted from the parser, never restated: a check that described the
    // grammar in its own words would be a second answer to drift from.
    assert_eq!(
        verdict("roadmap.mmd"),
        Verdict::Undrawable("`gantt` diagrams are not drawn yet".to_owned())
    );
    assert!(
        matches!(verdict("half.mmd"), Verdict::Undrawable(reason) if reason.contains("Rel(a)"))
    );
}

/// The failure an author cannot see by looking: the diagram opens, the
/// boxes are all there, and a relation is simply absent from the board.
#[test]
fn a_diagram_that_draws_with_relations_missing_is_not_drawn() {
    let mut dense = String::from("flowchart TD\n");
    for from in 0..7 {
        for to in 0..7 {
            if from != to {
                dense.push_str(&format!("  n{from}[Node {from}] --> n{to}[Node {to}]\n"));
            }
        }
    }
    let dense: &'static str = Box::leak(dense.into_boxed_str());
    let Checkup::Checked { artifacts, .. } =
        check(&Written(vec![("dense.mmd", dense)]), declared())
    else {
        panic!("a directory of diagrams is checked");
    };
    let Verdict::Unrouted { edges } = artifacts[0].verdict else {
        panic!("every node joined to every other leaves edges with nowhere to go");
    };
    assert!(edges > 0);
    assert!(
        !matches!(artifacts[0].verdict, Verdict::Drawn),
        "which is what fails a check"
    );
}

#[test]
fn what_a_project_declares_decides_whether_having_no_diagrams_is_a_fault() {
    let nothing = check(&Written(Vec::new()), ArtifactSource::Undeclared);
    assert!(
        matches!(nothing, Checkup::Nothing { text, .. } if text.contains("No artifacts declared")),
        "declaring none is an answer, not a mistake"
    );

    let refused = check(
        &Written(Vec::new()),
        ArtifactSource::Refused(
            "`workspace.artifacts` names `../x`, which leaves the project".to_owned(),
        ),
    );
    assert!(
        matches!(refused, Checkup::Unusable { text, .. } if text.contains("leaves the project")),
        "a declaration the host will not follow is somebody's mistake"
    );

    let empty = check(&Written(Vec::new()), declared());
    assert!(
        matches!(empty, Checkup::Checked { artifacts, .. } if artifacts.is_empty()),
        "a project may declare where its diagrams will go before drawing one"
    );
}

/// A link is repository content, and what it opens can be edited and
/// deleted: one that climbs out of the project, or names a path from the
/// root, is not followed and is not drawn as leading anywhere.
#[test]
fn a_link_that_leaves_the_project_leads_nowhere() {
    for (link, followed) in [
        ("crates/core", true),
        ("./docs", true),
        ("../elsewhere", false),
        ("docs/../../elsewhere", false),
        ("/etc/passwd", false),
    ] {
        let mut state = ArchitectView::opening("~/project".to_owned());
        state.absorb(ArtifactsAnswer {
            branch: "main".to_owned(),
            artifacts: Artifacts::Found {
                artifacts: vec![Artifact::read(
                    "linked.mmd",
                    format!("flowchart TD\n a --> b\n click a \"{link}\"\n"),
                )],
                project: PathBuf::from("/project"),
            },
        });
        pick(&mut state, "a");
        let Drawing::Graph(scene) = &state.drawing else {
            panic!("a flowchart is a graph");
        };
        let a = state.picked.expect("picked");
        assert_eq!(
            state.leads(scene, a) == Leads::ToCode,
            followed,
            "{link} is marked as leading to code"
        );
        let outcome = handle_command(&mut state, Command::Activate, SPACE);
        assert_eq!(
            matches!(outcome, ArchitectOutcome::OpenPath { .. }),
            followed,
            "{link} is followed"
        );
    }
}

/// Laying a board out and routing it is the most expensive thing this
/// surface does, so a diagram already drawn is not drawn again for going
/// back to it — until the artifacts are read again.
#[test]
fn a_diagram_already_laid_out_is_not_laid_out_again() {
    let laid_out = |state: &ArchitectView| match &state.drawing {
        Drawing::Graph(scene) => &**scene as *const Scene,
        _ => panic!("a graph is on show"),
    };
    let mut state = showing(CONTEXT);
    let first = laid_out(&state);
    pick(&mut state, "uze");
    handle_command(&mut state, Command::Activate, SPACE);
    assert_eq!(here(&state), CONTAINERS);
    handle_command(&mut state, Command::Back, SPACE);
    assert_eq!(here(&state), CONTEXT);
    assert_eq!(laid_out(&state), first, "the same layout, not a new one");

    assert!(!state.laid_out.is_empty(), "the level left is kept");
    let state = filled(state);
    assert!(state.laid_out.is_empty(), "a fresh read lays out afresh");
}

/// A box's link is followed as a file, so a check says which ones open
/// nothing: a module's directory or a file that moved looks exactly like a
/// working link on the board.
#[test]
fn a_check_names_every_link_that_opens_no_file() {
    let diagram = "flowchart TD\n  a[Kept] --> b[Moved]\n  b --> c[Outside]\n  click a href \"src/lib.rs\"\n  click b href \"src/gone.rs\"\n  click c href \"../outside.rs\"\n";
    let Checkup::Checked { artifacts, .. } = check(
        &Written(vec![("links.mmd", diagram), ("lib.rs", "pub fn f() {}")]),
        declared(),
    ) else {
        panic!("a directory of diagrams is checked");
    };
    let links = &artifacts
        .iter()
        .find(|artifact| artifact.origin.ends_with("links.mmd"))
        .expect("the diagram is checked")
        .broken_links;
    assert_eq!(links.len(), 2, "{links:?}");
    assert!(links[0].contains("`src/gone.rs` is not a file in the project"));
    assert!(links[1].contains("`../outside.rs` leaves the project"));
}

/// The same file name under two declared roots is two artifacts, each
/// named by the root it came from.
#[test]
fn two_roots_keep_two_files_of_one_name_apart() {
    let root = |declared: &str| ArtifactRoot {
        path: PathBuf::from("/project").join(declared),
        declared: declared.to_owned(),
    };
    let checkup = check(
        &Written(vec![("overview.mmd", "flowchart LR\n  a[A] --> b[B]\n")]),
        ArtifactSource::Directories {
            roots: vec![root("docs"), root("design")],
            project: PathBuf::from("/project"),
        },
    );
    let Checkup::Checked {
        declared,
        artifacts,
    } = checkup
    else {
        panic!("two directories of diagrams are checked");
    };
    assert_eq!(declared, "docs, design");
    let mut origins: Vec<_> = artifacts
        .iter()
        .map(|artifact| artifact.origin.clone())
        .collect();
    origins.sort();
    assert_eq!(origins, vec!["design/overview.mmd", "docs/overview.mmd"]);
}

mod hosted {
    use super::*;
    use crate::architect::{ArchitectExtension, SURFACE};
    use crate::contract::{Checkout, Extension, Opaque};
    use crate::view::Content;

    const SPACE: Size = Size {
        width: 80,
        height: 24,
    };

    fn checkout(root: &str) -> Checkout {
        Checkout {
            root: PathBuf::from(root),
            display: root.to_owned(),
            target: None,
            agent: None,
        }
    }

    fn overview() -> Written {
        Written(vec![("overview.mmd", "flowchart LR\n  a[A] --> b[B]\n")])
    }

    /// The answer each of the extension's jobs gives, run against `host`.
    fn run(extension: &mut ArchitectExtension, host: &dyn Host) -> Vec<Opaque> {
        extension
            .jobs()
            .into_iter()
            .map(|job| (job.run)(host))
            .collect()
    }

    fn reading(extension: &ArchitectExtension) -> bool {
        matches!(
            extension.view(SURFACE, SPACE).map(|view| view.content),
            Some(Content::Message { text, .. }) if text.starts_with("Reading")
        )
    }

    #[test]
    fn opening_asks_one_read_and_its_answer_draws_the_artifacts() {
        let mut extension = ArchitectExtension::default();
        assert!(extension.view(SURFACE, SPACE).is_none(), "closed, nothing");
        extension.open(SURFACE, &checkout("/project"));
        assert!(reading(&extension));
        let answers = run(&mut extension, &overview());
        assert_eq!(answers.len(), 1);
        assert!(extension.jobs().is_empty(), "asked once per opening");
        for answer in answers {
            extension.absorb(answer);
        }
        assert!(!reading(&extension), "the artifacts are drawn");
    }

    #[test]
    fn an_answer_about_a_checkout_left_behind_is_dropped() {
        let mut extension = ArchitectExtension::default();
        extension.open(SURFACE, &checkout("/first"));
        let late = run(&mut extension, &overview());
        extension.open(SURFACE, &checkout("/second"));
        for answer in late {
            extension.absorb(answer);
        }
        assert!(reading(&extension), "still reading the second");
    }

    #[test]
    fn a_place_survives_closing_and_a_closed_surface_draws_nothing() {
        let mut extension = ArchitectExtension::default();
        extension.open(SURFACE, &checkout("/project"));
        for answer in run(&mut extension, &overview()) {
            extension.absorb(answer);
        }
        let place = extension.place(SURFACE).expect("a place");
        extension.close(SURFACE);
        assert!(extension.view(SURFACE, SPACE).is_none());
        assert!(extension.place(SURFACE).is_none());

        extension.open(SURFACE, &checkout("/project"));
        extension.resume(SURFACE, place);
        for answer in run(&mut extension, &overview()) {
            extension.absorb(answer);
        }
        assert!(!reading(&extension));
    }

    use crate::contract::SurfaceSlot;
    use crate::view::{ScrollTarget, View};

    /// An extension open on `/project`, its artifacts read from `host`.
    fn opened_on(host: &dyn Host) -> ArchitectExtension {
        let mut extension = ArchitectExtension::default();
        extension.open(SURFACE, &checkout("/project"));
        for answer in run(&mut extension, host) {
            extension.absorb(answer);
        }
        extension
    }

    fn two() -> Written {
        Written(vec![
            ("first.mmd", "flowchart LR\n  a[A] --> b[B]\n"),
            ("second.mmd", "flowchart LR\n  c[C] --> d[D]\n"),
        ])
    }

    fn corner_of(extension: &ArchitectExtension) -> Option<(i32, i32)> {
        extension
            .place(SURFACE)
            .and_then(|place| place.downcast::<ArchitectPlace>())
            .expect("a place")
            .corner
    }

    fn selected_of(view: View) -> Vec<String> {
        view.navigator
            .unwrap()
            .rows
            .into_iter()
            .filter_map(|row| match row {
                crate::view::NavigatorRow::Item {
                    name,
                    selected: true,
                    ..
                } => Some(name),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_extension_names_its_one_board() {
        let extension = ArchitectExtension::default();
        assert_eq!(extension.identity(), CATALOG.id);
        let surfaces = extension.surfaces();
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].id, SURFACE);
        assert_eq!(surfaces[0].slot, SurfaceSlot::View);
        assert_eq!(surfaces[0].title, CATALOG.name);
        assert!(
            extension.section(SURFACE, false).is_none(),
            "a board, not a section"
        );
        assert_eq!(extension.layout(SURFACE), Layout::Board);
        assert!(extension.drags(SURFACE), "a board is moved by dragging");
    }

    #[test]
    fn a_command_reaches_the_board_it_names_and_no_other() {
        let mut extension = opened_on(&two());
        extension.command("elsewhere", Command::NextView, SPACE);
        assert_eq!(
            selected_of(extension.view(SURFACE, SPACE).unwrap()),
            ["First"]
        );
        extension.command(SURFACE, Command::NextView, SPACE);
        assert_eq!(
            selected_of(extension.view(SURFACE, SPACE).unwrap()),
            ["Second"]
        );
    }

    #[test]
    fn the_board_moves_by_wheel_drag_pan_and_scroll() {
        let mut extension = opened_on(&two());
        assert_eq!(corner_of(&extension), None, "home");
        extension.scroll(
            SURFACE,
            ScrollTarget::Navigator,
            ScrollDirection::Down,
            SPACE,
        );
        assert_eq!(
            corner_of(&extension),
            None,
            "the list scrolls, not the board"
        );
        extension.scroll(SURFACE, ScrollTarget::Content, ScrollDirection::Down, SPACE);
        let scrolled = corner_of(&extension).expect("moved");

        let mut extension = opened_on(&two());
        extension.drag(SURFACE, 3, 0, SPACE);
        assert!(corner_of(&extension).is_some(), "dragged");

        let mut extension = opened_on(&two());
        extension.pan(SURFACE, 3, SPACE);
        assert!(corner_of(&extension).is_some(), "panned");

        let mut extension = opened_on(&two());
        extension.scroll_to(SURFACE, scrolled.1 as usize, SPACE);
        assert_eq!(
            corner_of(&extension).map(|corner| corner.1),
            Some(scrolled.1)
        );
    }

    #[test]
    fn a_press_that_misses_shuts_an_open_list() {
        let mut extension = opened_on(&two());
        extension.command(SURFACE, Command::ChooseItem, SPACE);
        let choosing = |extension: &ArchitectExtension| {
            extension
                .view(SURFACE, SPACE)
                .unwrap()
                .navigator
                .unwrap()
                .choosing
        };
        assert!(choosing(&extension).is_some());
        assert!(extension.hover(SURFACE, Some(ViewHit::SelectItem(1))));
        extension.missed(SURFACE, SPACE);
        assert_eq!(choosing(&extension), None);
    }

    #[test]
    fn the_source_on_show_is_copied_as_text() {
        let mut extension = opened_on(&two());
        extension.hit(SURFACE, ViewHit::SelectMode(2), SPACE);
        assert_eq!(extension.text(SURFACE, 0..1), ["flowchart LR"]);
    }

    #[test]
    fn a_resumed_board_opens_where_the_viewer_left_it() {
        let mut extension = ArchitectExtension::default();
        extension.open(SURFACE, &checkout("/project"));
        extension.resume(
            SURFACE,
            Opaque::new(ArchitectPlace {
                artifact: "second.mmd".to_owned(),
                ..ArchitectPlace::default()
            }),
        );
        for answer in run(&mut extension, &two()) {
            extension.absorb(answer);
        }
        assert_eq!(
            selected_of(extension.view(SURFACE, SPACE).unwrap()),
            ["Second"]
        );
    }
}

fn drawing(artifacts: Vec<Artifact>) -> ArchitectView {
    let mut state = ArchitectView::opening("~/project".to_owned());
    state.absorb(ArtifactsAnswer {
        branch: "main".to_owned(),
        artifacts: Artifacts::Found {
            artifacts,
            project: PathBuf::from("/project"),
        },
    });
    state
}

/// A container with a view of its own components and a link to its code
/// leads both ways: inside when followed, to the code by its own mark.
#[test]
fn a_box_that_leads_both_ways_carries_both_marks() {
    let mut state = drawing(vec![
        Artifact::read(
            "a.mmd",
            "C4Container\nSystem_Boundary(s, \"S\") {\n\
             Container(api, \"The API server\", \"Rust\", \"Serves\", $link=\"src/api.rs\")\n}\n",
        ),
        Artifact::read(
            "b.mmd",
            "C4Component\nContainer_Boundary(api, \"API\") {\n}\n",
        ),
    ]);
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let api = node_named_in(&state, "api").unwrap();
    assert_eq!(state.leads(scene, api), Leads::Both);
    let frame = scene.placement.nodes[api];
    let code = paint::code_mark_cells(frame, Leads::Both).expect("room for two marks");
    let corner = state.corner(SPACE);
    let on_code = ViewHit::PlaceCaret {
        line: (code.y - corner.1) as usize,
        cell: (code.x + 1 - corner.0) as usize,
    };
    assert_eq!(
        handle_mouse(&mut state, Some(on_code), SPACE),
        ArchitectOutcome::OpenPath {
            project: PathBuf::from("/project"),
            target: PathBuf::from("/project/src/api.rs"),
        }
    );
    state.picked = Some(api);
    handle_command(&mut state, Command::Activate, SPACE);
    assert_eq!(state.selected, 1, "followed, it goes inside");
}

#[test]
fn a_link_that_is_a_url_leads_nowhere() {
    let state = drawing(vec![Artifact::read(
        "a.mmd",
        "C4Component\nComponent(x, \"X\", \"Rust\", \"x\", $link=\"https://example.com/x\")\n",
    )]);
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    assert_eq!(state.leads(scene, 0), Leads::Nowhere);
    assert_eq!(
        zoom::project_path("src/lib.rs#L42"),
        Some(PathBuf::from("src/lib.rs")),
        "a fragment is dropped, not followed"
    );
}

/// The text of the key under the drawing on show.
fn key_of(state: &ArchitectView) -> String {
    let text = state.canvas.as_ref().expect("a drawing").to_text();
    text.split_once("\n Key\n")
        .map(|(_, key)| key.to_owned())
        .unwrap_or_default()
}

#[test]
fn the_key_explains_only_what_the_drawing_uses() {
    let state = drawing(vec![Artifact::read(
        "a.mmd",
        "C4Container\nSystem_Boundary(s, \"S\") {\n\
         ContainerDb(db, \"Store\", \"Files\", \"Bytes\")\n}\n\
         System_Ext(git, \"Git\", \"Remote\")\nRel(db, git, \"Mirrors\", \"Git\")\n",
    )]);
    let key = key_of(&state);
    assert!(key.contains("a store of data"), "{key}");
    assert!(key.contains("outside the system being described"), "{key}");
    assert!(key.contains("a boundary"), "{key}");
    assert!(
        !key.contains("a queue") && !key.contains("a person"),
        "{key}"
    );
}

/// A label with no room on its line leaves a marker the key reads out.
#[test]
fn a_label_with_no_room_is_kept_in_the_key() {
    let long = "a relationship whose label is far too long to fit on any line of this board at all";
    let state = drawing(vec![Artifact::read(
        "a.mmd",
        format!("C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a, b, \"{long}\")\n"),
    )]);
    let key = key_of(&state);
    assert!(key.contains(&format!("A ► B: {long}")), "{key}");
}

fn notes_of(files: Vec<(&'static str, &'static str)>, origin: &str) -> Vec<String> {
    let Checkup::Checked { artifacts, .. } = check(&Written(files), declared()) else {
        panic!("a directory of diagrams is checked");
    };
    artifacts
        .into_iter()
        .find(|artifact| artifact.origin.ends_with(origin))
        .expect("the diagram is checked")
        .notes
}

#[test]
fn the_check_notes_where_a_view_departs_from_c4_notation() {
    let notes = notes_of(
        vec![
            (
                "context.mmd",
                "---\ntitle: Context for shop\n---\nC4Context\n\
                 System(shop, \"Shop\", \"Sells\")\nPerson(api, \"API user\", \"Calls\")\n",
            ),
            (
                "views.mmd",
                "---\ntitle: Shop parts\n---\nC4Container\n\
                 Container(api, \"API\")\nContainerDb(db, \"DB\", \"SQL\", \"Rows\")\n\
                 Rel(api, db)\nBiRel(db, api, \"Syncs\")\n",
            ),
        ],
        "views.mmd",
    );
    for wanted in [
        "the view is drawn inside no `System_Boundary`, so it zooms into nothing",
        "`api` has no description",
        "`api` names no technology",
        "`api` is a container here and a person in context.mmd",
        "the line from `api` to `db` has no label",
        "the line between `db` and `api` runs both ways; C4 lines run one way",
    ] {
        assert!(
            notes.iter().any(|note| note == wanted),
            "{wanted}: {notes:?}"
        );
    }
    assert_eq!(notes.len(), 6, "{notes:?}");
}

/// The repository's own C4 views are the model's example: they are held
/// to no notes at all.
#[test]
fn the_repositorys_own_c4_views_follow_the_notation() {
    let files = vec![
        (
            "system-context.mmd",
            include_str!("../../../../docs/architecture/system-context.mmd"),
        ),
        (
            "containers.mmd",
            include_str!("../../../../docs/architecture/containers.mmd"),
        ),
        (
            "core-components.mmd",
            include_str!("../../../../docs/architecture/core-components.mmd"),
        ),
        (
            "workspace-components.mmd",
            include_str!("../../../../docs/architecture/workspace-components.mmd"),
        ),
    ];
    for origin in [
        "system-context.mmd",
        "containers.mmd",
        "core-components.mmd",
        "workspace-components.mmd",
    ] {
        assert_eq!(
            notes_of(files.clone(), origin),
            Vec::<String>::new(),
            "{origin}"
        );
    }
}

/// A tree of directories and files, served from memory: the listing of
/// each directory it knows, and the files it holds by their full path.
/// A directory it does not know cannot be listed.
struct Tree {
    directories: Vec<(&'static str, Vec<(&'static str, bool)>)>,
    files: Vec<(&'static str, &'static str)>,
}

impl Host for Tree {
    fn git(&self, _: &std::path::Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("not asked".to_owned())
    }
    fn repository_root(&self, _: &std::path::Path) -> Result<PathBuf, String> {
        Err("not asked".to_owned())
    }
    fn read_file(&self, path: &std::path::Path) -> Result<String, crate::Unreadable> {
        self.files
            .iter()
            .find(|(written, _)| Path::new(written) == path)
            .map(|(_, source)| (*source).to_owned())
            .ok_or_else(|| crate::Unreadable::Failed("no such file".to_owned()))
    }
    fn list_dir(&self, path: &std::path::Path) -> Result<Vec<crate::DirEntry>, String> {
        self.directories
            .iter()
            .find(|(listed, _)| Path::new(listed) == path)
            .map(|(_, entries)| {
                entries
                    .iter()
                    .map(|&(name, directory)| crate::DirEntry {
                        directory,
                        name: name.to_owned(),
                    })
                    .collect()
            })
            .ok_or_else(|| "permission denied".to_owned())
    }
    fn write_file(
        &self,
        _root: &std::path::Path,
        _: &std::path::Path,
        _: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    fn delete_file(&self, _root: &std::path::Path, _: &std::path::Path) -> Result<(), String> {
        Ok(())
    }
    fn restore_to_head(&self, _: &std::path::Path, _: &[std::path::PathBuf]) -> Result<(), String> {
        Ok(())
    }
    fn syntax_theme(&self) -> String {
        String::new()
    }
}

const FLOW: &str = "flowchart TD\n  a --> b\n";

#[test]
fn a_directory_that_cannot_be_listed_is_named_with_its_reason() {
    let locked = Tree {
        directories: Vec::new(),
        files: Vec::new(),
    };
    let answer = read_artifacts(&locked, Path::new("/project"), declared());
    let Artifacts::Nothing { text, hint } = answer.artifacts else {
        panic!("nothing could be read");
    };
    assert!(
        text.contains("`docs/architecture` could not be read"),
        "{text}"
    );
    assert!(hint.contains("permission denied"), "{hint}");
    assert!(hint.contains("workspace.artifacts"), "{hint}");

    let Checkup::Unusable { text, hint } = check(&locked, declared()) else {
        panic!("a directory that cannot be read is somebody's to fix");
    };
    assert!(text.contains("docs/architecture"), "{text}");
    assert!(hint.contains("permission denied"), "{hint}");
}

#[test]
fn a_directory_of_diagrams_is_read_into_artifacts() {
    let answer = read_artifacts(
        &Written(vec![("overview.mmd", FLOW)]),
        Path::new("/project"),
        declared(),
    );
    let Artifacts::Found { artifacts, project } = answer.artifacts else {
        panic!("a diagram was found");
    };
    assert_eq!(artifacts.len(), 1);
    assert_eq!(project, PathBuf::from("/project"));
}

/// The walk goes a few directories deep and no further, and only the
/// declared directory itself failing to list is a failure.
#[test]
fn diagrams_are_found_a_few_directories_down_and_no_deeper() {
    let tree = Tree {
        directories: vec![
            (
                "/d",
                vec![("top.mmd", false), ("1", true), ("locked", true)],
            ),
            ("/d/1", vec![("2", true)]),
            ("/d/1/2", vec![("3", true)]),
            ("/d/1/2/3", vec![("4", true)]),
            ("/d/1/2/3/4", vec![("deep.mmd", false), ("5", true)]),
            ("/d/1/2/3/4/5", vec![("deeper.mmd", false)]),
        ],
        files: vec![
            ("/d/top.mmd", FLOW),
            ("/d/1/2/3/4/deep.mmd", FLOW),
            ("/d/1/2/3/4/5/deeper.mmd", FLOW),
        ],
    };
    let found = catalog::read(&tree, Path::new("/d")).expect("the directory lists");
    let mut names: Vec<String> = found
        .iter()
        .map(|artifact| {
            Path::new(&artifact.origin)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["deep.mmd", "top.mmd"]);

    assert_eq!(
        catalog::read(&tree, Path::new("/elsewhere")),
        Err("permission denied".to_owned())
    );
}

/// A box may stand for a module's directory, which the code surface opens;
/// a path that names nothing is still broken, whatever sits beside it.
#[test]
fn a_link_to_a_directory_opens_something() {
    let diagram =
        "flowchart TD\n  a --> b\n  click a href \"src/module\"\n  click b href \"src/missing\"\n";
    let tree = Tree {
        directories: vec![
            ("/project/docs/architecture", vec![("links.mmd", false)]),
            ("/project/src", vec![("module", true), ("other", true)]),
        ],
        files: vec![("/project/docs/architecture/links.mmd", diagram)],
    };
    let Checkup::Checked { artifacts, .. } = check(&tree, declared()) else {
        panic!("a directory of diagrams is checked");
    };
    assert_eq!(
        artifacts[0].broken_links,
        ["`src/missing` is not a file in the project"]
    );
}

/// Only a C4 view is held to C4's notation: a flowchart's bare arrow is
/// not a line missing its label.
#[test]
fn a_flowchart_is_held_to_no_notation() {
    assert_eq!(
        notes_of(vec![("flow.mmd", FLOW)], "flow.mmd"),
        Vec::<String>::new()
    );
}

#[test]
fn the_list_of_areas_goes_round_both_ways_and_steps_over_to_the_artifacts() {
    let mut state = showing("Crate layering");
    let areas = state.areas();
    assert_eq!(areas.len(), 3);
    handle_command(&mut state, Command::ChooseGroup, SPACE);
    assert_eq!(state.choosing, Some(Choosing::Group(areas[2])));
    handle_command(&mut state, Command::Pan(PanDirection::Up), SPACE);
    assert_eq!(state.choosing, Some(Choosing::Group(areas[1])));
    handle_command(&mut state, Command::Pan(PanDirection::Down), SPACE);
    assert_eq!(state.choosing, Some(Choosing::Group(areas[2])));
    handle_command(&mut state, Command::Pan(PanDirection::Down), SPACE);
    assert_eq!(state.choosing, Some(Choosing::Group(areas[0])), "round");
    handle_command(&mut state, Command::Pan(PanDirection::Right), SPACE);
    assert_eq!(
        state.choosing,
        Some(Choosing::Item(state.selected)),
        "right is the artifacts"
    );
}

#[test]
fn the_areas_are_listed_by_name_with_the_artifact_on_show_selected() {
    let mut state = opened();
    handle_command(&mut state, Command::NextView, SPACE);
    let rows = view(&state, SPACE).navigator.unwrap().rows;
    let groups: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Group { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(groups, ["C4", "Sequence", "Flowchart"]);
    let selected: Vec<usize> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Item {
                id, selected: true, ..
            } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(selected, [1]);
}

/// A view with no other way down to its place is a label, not a selector.
#[test]
fn a_step_with_nothing_else_to_offer_is_not_a_selector() {
    let step = view(&showing(CONTAINERS), SPACE)
        .trail
        .pop()
        .expect("a way down");
    assert!(step.choices.is_empty(), "{:?}", step.choices.len());
}

#[test]
fn pressing_the_step_on_show_stays_there() {
    let mut state = showing(CORE);
    handle_mouse(&mut state, Some(ViewHit::SelectTrail(2)), SPACE);
    assert_eq!(here(&state), CORE);
    handle_mouse(&mut state, Some(ViewHit::SelectTrail(1)), SPACE);
    assert_eq!(here(&state), CONTAINERS);
}

#[test]
fn hovering_an_area_of_the_open_list_highlights_it() {
    let mut state = opened();
    let areas = state.areas();
    handle_command(&mut state, Command::ChooseGroup, SPACE);
    assert!(handle_hover(
        &mut state,
        Some(ViewHit::ToggleGroup(areas[1]))
    ));
    assert_eq!(state.choosing, Some(Choosing::Group(areas[1])));
    assert!(!handle_hover(
        &mut state,
        Some(ViewHit::ToggleGroup(areas[1]))
    ));
    assert_eq!(state.selected, 0, "hovering chooses nothing");
}

/// A source long enough to scroll: a comment a line, then the diagram.
fn long_source() -> String {
    let mut source = String::new();
    for line in 0..100 {
        source.push_str(&format!("%% line {line}\n"));
    }
    source.push_str("flowchart TD\n  a[Alpha] --> b[Beta]\n");
    source
}

#[test]
fn the_source_is_read_as_numbered_lines_and_copied_as_written() {
    let source = "flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n";
    let mut state = drawing(vec![Artifact::read("a.mmd", source)]);
    assert!(text(&state, 0..2).is_empty(), "a drawing has no text");
    handle_mouse(&mut state, Some(ViewHit::SelectMode(2)), SPACE);
    let Content::Lines {
        lines,
        total,
        medium,
        ..
    } = view(&state, SPACE).content
    else {
        panic!("the source is lines");
    };
    assert_eq!((total, medium), (3, Medium::Text));
    let numbered: Vec<(String, String)> = lines
        .iter()
        .map(|line| {
            let text: String = line.spans.iter().map(|span| span.text.clone()).collect();
            (line.number.clone(), text)
        })
        .collect();
    assert_eq!(
        numbered,
        [
            ("1".to_owned(), "flowchart TD".to_owned()),
            ("2".to_owned(), "  a[A] --> b[B]".to_owned()),
            ("3".to_owned(), "  b --> c[C]".to_owned()),
        ]
    );
    assert_eq!(text(&state, 1..3), ["  a[A] --> b[B]", "  b --> c[C]"]);
}

#[test]
fn the_ways_of_drawing_go_round_with_the_one_on_show_marked() {
    let mut state = opened();
    let active = |state: &ArchitectView| {
        view(state, SPACE)
            .modes
            .into_iter()
            .filter(|mode| mode.active)
            .map(|mode| mode.label)
            .collect::<Vec<_>>()
    };
    let labels: Vec<String> = view(&state, SPACE)
        .modes
        .into_iter()
        .map(|mode| mode.label)
        .collect();
    assert_eq!(labels, ["Unicode", "ASCII", "Source"]);
    for next in ["Unicode", "ASCII", "Source", "Unicode"] {
        assert_eq!(active(&state), [next]);
        handle_command(&mut state, Command::NextMode, SPACE);
    }
}

/// The source is a document: it scrolls down to its last line and no
/// further, and not sideways.
#[test]
fn the_source_scrolls_like_a_document() {
    let mut state = drawing(vec![Artifact::read("long.mmd", long_source())]);
    state.show(Showing::Source);
    let lines = 102;
    let first = |state: &ArchitectView| match view(state, SPACE).content {
        Content::Lines { first, .. } => first,
        _ => panic!("the source is lines"),
    };
    scroll_to(&mut state, 500, SPACE);
    assert_eq!(first(&state), lines - 30, "the last line at the foot");
    scroll_to(&mut state, 10, SPACE);
    assert_eq!(first(&state), 10);
    handle_scroll(&mut state, ScrollDirection::Up, SPACE);
    assert!(first(&state) < 10);
    handle_scroll(&mut state, ScrollDirection::Down, SPACE);
    assert_eq!(first(&state), 10);
    handle_command(&mut state, Command::Pan(PanDirection::Right), SPACE);
    assert_eq!(
        state.corner(SPACE).0,
        0,
        "a document does not move sideways"
    );
    scroll_to(&mut state, 0, SPACE);
    handle_command(&mut state, Command::Pan(PanDirection::Up), SPACE);
    assert_eq!(first(&state), 0, "nor above its first line");
}

#[test]
fn a_drawing_shorter_than_the_screen_opens_in_the_middle_of_it_down() {
    let state = showing(CONTEXT);
    let tall = Size {
        width: 300,
        height: 120,
    };
    let board = state.board_size();
    assert!(board.1 < 120);
    let above = -state.corner(tall).1;
    let below = 120 - board.1 - above;
    assert!((above - below).abs() <= 1, "{above} above, {below} below");
}

#[test]
fn a_click_on_the_minimap_centres_the_screen_on_the_part_clicked() {
    let mut state = opened();
    let map = state.minimap(SPACE).expect("the screen has room for a map");
    let (x, y) = (map.frame.x + map.frame.w / 2, map.frame.y + map.frame.h / 2);
    let target = map.board_cell_at(x, y).expect("inside the map");
    handle_mouse(
        &mut state,
        Some(ViewHit::PlaceCaret {
            line: y as usize,
            cell: x as usize,
        }),
        SPACE,
    );
    let corner = state.corner(SPACE);
    assert_eq!((corner.0 + 50, corner.1 + 15), target);
}

/// A cell of `node`'s box on the screen that is neither of its marks.
fn plain_cell_of(state: &ArchitectView, node: usize) -> ViewHit {
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let frame = scene.placement.nodes[node];
    let leads = state.leads(scene, node);
    let corner = state.corner(SPACE);
    let mark = paint::mark_cells(frame);
    let code = paint::code_mark_cells(frame, leads);
    for y in frame.y + 1..frame.y + frame.h - 1 {
        for x in frame.x + 1..frame.x + frame.w - 1 {
            let on_code = code.is_some_and(|cells| cells.contains(x, y));
            let on_screen = (0..100).contains(&(x - corner.0)) && (0..30).contains(&(y - corner.1));
            if !mark.contains(x, y) && !on_code && on_screen {
                return ViewHit::PlaceCaret {
                    line: (y - corner.1) as usize,
                    cell: (x - corner.0) as usize,
                };
            }
        }
    }
    panic!("the box has a plain cell on the screen");
}

/// A double click is a second click on a picked box: it goes where the
/// box leads, and the first only picks it.
#[test]
fn a_second_click_on_a_box_that_leads_inside_goes_inside() {
    let mut state = showing(CONTEXT);
    let uze = node_named_in(&state, "uze").unwrap();
    let hit = plain_cell_of(&state, uze);
    handle_mouse(&mut state, Some(hit), SPACE);
    assert_eq!(
        (here(&state), state.picked),
        (CONTEXT.to_owned(), Some(uze))
    );
    handle_mouse(&mut state, Some(hit), SPACE);
    assert_eq!(here(&state), CONTAINERS);
}

/// Going back up stands the viewer on the box that led down, in the middle
/// of the screen.
#[test]
fn coming_back_brings_the_box_that_led_down_to_the_middle() {
    let mut state = showing(CORE);
    handle_command(&mut state, Command::Back, SPACE);
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let frame = scene.placement.nodes[state.picked.expect("the box is picked")];
    let corner = state.corner(SPACE);
    assert!(
        frame.contains(corner.0 + 50, corner.1 + 15),
        "{frame:?} from {corner:?}"
    );
}

fn fully_visible(frame: Frame, corner: (i32, i32)) -> bool {
    frame.x >= corner.0
        && frame.y >= corner.1
        && frame.x + frame.w <= corner.0 + 100
        && frame.y + frame.h <= corner.1 + 30
}

/// Walking the boxes moves the board only when the box walked to is not
/// already wholly on the screen, and then it brings it wholly on.
#[test]
fn walking_to_a_box_out_of_view_brings_it_into_view_and_only_then() {
    let (mut stayed, mut moved) = (0, 0);
    for name in [CONTEXT, CONTAINERS, CORE, WORKSPACE, "Crate layering"] {
        for direction in [
            PanDirection::Down,
            PanDirection::Right,
            PanDirection::Up,
            PanDirection::Left,
        ] {
            let mut state = showing(name);
            for _ in 0..8 {
                let before = state.corner(SPACE);
                let was = state.picked;
                handle_command(&mut state, Command::SelectToward(direction), SPACE);
                let Some(picked) = state.picked.filter(|&picked| Some(picked) != was) else {
                    break;
                };
                let Drawing::Graph(scene) = &state.drawing else {
                    panic!("a graph is on show");
                };
                let frame = scene.placement.nodes[picked];
                let after = state.corner(SPACE);
                if fully_visible(frame, before) {
                    assert_eq!(after, before, "{name}: {frame:?} was in view");
                    stayed += 1;
                } else {
                    assert!(
                        fully_visible(frame, after),
                        "{name}: {frame:?} from {after:?}"
                    );
                    moved += 1;
                }
            }
        }
    }
    assert!(stayed > 0 && moved > 0, "{stayed} stayed, {moved} moved");
}

/// With nothing picked, the first step picks the box in the middle of the
/// screen.
#[test]
fn with_nothing_picked_the_box_in_the_middle_of_the_screen_is_picked() {
    let mut state = showing(CONTAINERS);
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let centres: Vec<(i32, i32)> = scene.placement.nodes.iter().map(|f| f.center()).collect();
    for (node, centre) in centres.into_iter().enumerate() {
        state.picked = None;
        state.corner = Some((centre.0 - 50, centre.1 - 15));
        handle_command(&mut state, Command::SelectToward(PanDirection::Down), SPACE);
        assert_eq!(state.picked, Some(node));
    }
}

fn heading_of(state: &ArchitectView) -> String {
    match view(state, SPACE).content {
        Content::Lines { heading, .. } => heading,
        _ => panic!("a drawing is lines"),
    }
}

/// The heading counts what is drawn, or says of the box picked what joins
/// it, and names the file it all came from.
#[test]
fn the_heading_counts_the_drawing_and_names_its_file() {
    let source = "flowchart TD\n  a[Alpha] --> b[Beta]\n  a --> c[Gamma]\n";
    let mut state = drawing(vec![Artifact::read("a.mmd", source)]);
    assert_eq!(heading_of(&state), "3 boxes · 2 edges · a.mmd");
    pick(&mut state, "a");
    assert_eq!(heading_of(&state), "Alpha · 0 in · 2 out · a.mmd");
    pick(&mut state, "b");
    assert_eq!(heading_of(&state), "Beta · 1 in · 0 out · a.mmd");
    state.show(Showing::Source);
    assert_eq!(heading_of(&state), "Beta · 1 in · 0 out · a.mmd");

    let mut dense = String::from("flowchart TD\n");
    for from in 0..7 {
        for to in 0..7 {
            if from != to {
                dense.push_str(&format!("  n{from} --> n{to}\n"));
            }
        }
    }
    let state = drawing(vec![Artifact::read("dense.mmd", dense)]);
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let unrouted = scene.routes.unrouted;
    assert!(unrouted > 0);
    assert_eq!(
        heading_of(&state),
        format!("7 boxes · 42 edges · {unrouted} not routed · dense.mmd")
    );
}

#[test]
fn the_heading_says_where_a_picked_box_leads() {
    let mut state = showing(CONTEXT);
    pick(&mut state, "uze");
    let heading = heading_of(&state);
    assert!(heading.ends_with(" · enter goes inside"), "{heading}");
}

/// The screen is the board under it: every cell the drawing wrote, the
/// grid in the empty space around it — regular, and moving with the
/// board — and no grid inside a region.
#[test]
fn the_screen_shows_the_drawing_on_its_grid() {
    let mut state = drawing(vec![Artifact::read(
        "a.mmd",
        "flowchart TD\n  a --> m --> z\n  subgraph region [Region]\n  m\n  end\n",
    )]);
    let wide = Size {
        width: 160,
        height: 60,
    };
    let dots = |state: &ArchitectView| {
        let screen = state.screen(wide).expect("a screen");
        let corner = state.corner(wide);
        let mut dots = Vec::new();
        for y in 0..60 {
            for x in 0..160 {
                let cell = screen.cell(x, y).unwrap();
                if cell.glyph == '·' && cell.role == Role::Faint {
                    dots.push((x + corner.0, y + corner.1));
                }
            }
        }
        dots
    };
    let screen = state.screen(wide).expect("a screen");
    let corner = state.corner(wide);
    let board = state.canvas.as_ref().unwrap();
    for y in 0..60 {
        for x in 0..160 {
            if let Some(cell) = board.cell(x + corner.0, y + corner.1)
                && cell.solid
            {
                assert_eq!(screen.cell(x, y), Some(cell), "({x},{y})");
            }
        }
    }

    let on_board = dots(&state);
    assert!(!on_board.is_empty(), "the grid shows");
    let columns: BTreeSet<i32> = on_board.iter().map(|&(x, _)| x).collect();
    let rows: BTreeSet<i32> = on_board.iter().map(|&(_, y)| y).collect();
    let gaps = |set: &BTreeSet<i32>| {
        set.iter()
            .zip(set.iter().skip(1))
            .map(|(a, b)| b - a)
            .collect::<BTreeSet<i32>>()
    };
    assert_eq!(gaps(&columns).len(), 1, "evenly spaced across");
    assert_eq!(gaps(&rows).len(), 1, "evenly spaced down");
    assert!(gaps(&columns).iter().all(|&gap| gap > 1));
    assert!(gaps(&rows).iter().all(|&gap| gap > 1));
    assert!(
        on_board
            .iter()
            .any(|&(x, y)| x >= 0 && y >= 0 && x < board.width && y < board.height),
        "the grid shows through the empty parts of the board itself"
    );

    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let region = scene.placement.clusters[0];
    assert!(
        !on_board.iter().any(|&(x, y)| region.contains(x, y)),
        "a region stands on its own ground"
    );

    let before = state.corner(wide);
    drag_by(&mut state, 1, 0, wide);
    let after = state.corner(wide);
    assert_ne!(before, after);
    let both = before.0.max(after.0)..before.0.min(after.0) + 160;
    let shared = |dots: Vec<(i32, i32)>| {
        dots.into_iter()
            .filter(|(x, _)| both.contains(x))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shared(dots(&state)),
        shared(on_board),
        "the grid is the board's"
    );
}

/// The map over the board shows the boxes as well as the screen's outline.
#[test]
fn the_minimap_shows_the_boxes() {
    let state = showing(CONTAINERS);
    let map = state.minimap(SPACE).expect("room for a map");
    let screen = state.screen(SPACE).expect("a screen");
    let boxes = (map.frame.y + 1..map.frame.y + map.frame.h - 1)
        .flat_map(|y| (map.frame.x + 1..map.frame.x + map.frame.w - 1).map(move |x| (x, y)))
        .filter(|&(x, y)| screen.cell(x, y).unwrap().role == Role::Secondary)
        .count();
    assert!(boxes > 0);
}

#[test]
fn the_arrows_move_the_board_the_way_they_point() {
    let mut state = opened();
    let start = state.corner(SPACE);
    let moved = |state: &mut ArchitectView, command: Command| {
        let before = state.corner(SPACE);
        handle_command(state, command, SPACE);
        let after = state.corner(SPACE);
        (after.0 - before.0, after.1 - before.1)
    };
    let right = moved(&mut state, Command::Pan(PanDirection::Right));
    assert!(right.0 > 0 && right.1 == 0, "{right:?}");
    let left = moved(&mut state, Command::Pan(PanDirection::Left));
    assert_eq!(left, (-right.0, 0));
    let down = moved(&mut state, Command::Pan(PanDirection::Down));
    assert!(down.0 == 0 && down.1 > 0, "{down:?}");
    let up = moved(&mut state, Command::Pan(PanDirection::Up));
    assert_eq!(up, (0, -down.1));
    let page = moved(&mut state, Command::ScrollPageDown);
    assert!(page.0 == 0 && page.1 > down.1, "{page:?}");
    let back = moved(&mut state, Command::ScrollPageUp);
    assert_eq!(back, (0, -page.1));
    assert_eq!(state.corner(SPACE), start);
    pan(&mut state, 4, SPACE);
    assert_eq!(state.corner(SPACE), (start.0 + 4, start.1));
}

#[test]
fn the_close_button_closes() {
    let mut state = opened();
    assert_eq!(
        handle_mouse(&mut state, Some(ViewHit::Close), SPACE),
        ArchitectOutcome::Close
    );
}

#[test]
fn a_selector_pressed_while_its_own_list_is_open_shuts_it() {
    let mut state = showing(CORE);
    handle_mouse(&mut state, Some(ViewHit::ChooseGroup), SPACE);
    assert!(state.choosing.is_some());
    handle_mouse(&mut state, Some(ViewHit::ChooseGroup), SPACE);
    assert_eq!(state.choosing, None);
    handle_mouse(&mut state, Some(ViewHit::ChooseItem), SPACE);
    assert!(state.choosing.is_some());
    handle_mouse(&mut state, Some(ViewHit::ChooseItem), SPACE);
    assert_eq!(state.choosing, None);
}

#[test]
fn a_way_of_drawing_pressed_while_a_list_is_open_only_shuts_the_list() {
    let mut state = showing(CORE);
    handle_mouse(&mut state, Some(ViewHit::ChooseItem), SPACE);
    handle_mouse(&mut state, Some(ViewHit::SelectMode(2)), SPACE);
    assert_eq!((state.choosing, state.showing), (None, Showing::Unicode));
    handle_mouse(&mut state, Some(ViewHit::SelectMode(2)), SPACE);
    assert_eq!(state.showing, Showing::Source);
}

/// The source is text: a click on it picks no box, wherever the boxes
/// would be on the board.
#[test]
fn a_click_on_the_source_picks_no_box() {
    let mut state = showing("Crate layering");
    let hit = plain_cell_of(&state, 0);
    let ViewHit::PlaceCaret { line, .. } = hit else {
        panic!("a cell");
    };
    let number = ViewHit::LineNumber {
        line,
        extend: false,
    };
    state.show(Showing::Source);
    handle_mouse(&mut state, Some(hit), SPACE);
    handle_mouse(&mut state, Some(number), SPACE);
    assert_eq!(state.picked, None);
}

/// A press at the board's left edge reaches the board like any other.
#[test]
fn a_press_at_the_left_edge_of_a_board_picks_the_box_there() {
    let mut state = showing("Crate layering");
    let Drawing::Graph(scene) = &state.drawing else {
        panic!("a graph is on show");
    };
    let frame = scene.placement.nodes[0];
    state.corner = Some((frame.x, frame.y - 5));
    let hit = ViewHit::LineNumber {
        line: 6,
        extend: false,
    };
    handle_mouse(&mut state, Some(hit), SPACE);
    assert_eq!(state.picked, Some(0));
}
