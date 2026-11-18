//! A laid-out, routed graph, turned into cells.
//!
//! Kept apart from the layout because it is the cheap half: choosing
//! another glyph set or selecting a node repaints, and neither has any
//! reason to move a box or re-run a search.

use crate::view::Role;

use crate::shared::canvas::{
    self, Canvas, Corners, EAST, Frame, Glyphs, NORTH, SOUTH, WEST, arrow_glyph, text_width,
};

use super::{
    layout::{self, Placement, TextKind},
    model::{Graph, Node, Shape, Stroke},
    route::{self, Route, Routes},
};

pub struct Scene {
    pub graph: Graph,
    pub placement: Placement,
    pub routes: Routes,
}

impl Scene {
    pub fn of(graph: Graph) -> Self {
        let placement = layout::place(&graph);
        let routes = route::route(&graph, &placement);
        Self {
            graph,
            placement,
            routes,
        }
    }

    fn touches(&self, route: &Route, selected: Option<usize>) -> bool {
        let edge = &self.graph.edges[route.edge];
        selected.is_some_and(|node| edge.from == node || edge.to == node)
    }
}

/// Which sides of each cell a line leaves by, and how that line is drawn.
struct Lines {
    width: i32,
    sides: Vec<u8>,
    stroke: Vec<Stroke>,
    lit: Vec<bool>,
}

impl Lines {
    fn index(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }
}

/// Where a box leads, if anywhere.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Leads {
    #[default]
    Nowhere,
    /// To the level below: another diagram draws its inside.
    Inside,
    /// To the code it stands for.
    ToCode,
    /// Both: inside when followed, to the code by its own mark.
    Both,
}

/// The cells of a box's border that carry its mark, which are also what
/// a click means "follow this" on.
pub fn mark_cells(frame: Frame) -> Frame {
    Frame {
        x: frame.x + frame.w - 5,
        y: frame.y + frame.h - 1,
        w: 4,
        h: 1,
    }
}

/// The cells of the second mark a box that leads both ways carries, left
/// of the first, where the box is wide enough for two.
pub fn code_mark_cells(frame: Frame, leads: Leads) -> Option<Frame> {
    (leads == Leads::Both && frame.w >= 11).then(|| Frame {
        x: frame.x + frame.w - 9,
        y: frame.y + frame.h - 1,
        w: 4,
        h: 1,
    })
}

pub fn paint(scene: &Scene, glyphs: Glyphs, selected: Option<usize>, leads: &[Leads]) -> Canvas {
    let Placement { width, height, .. } = scene.placement;
    let mut canvas = Canvas::new(width, height, glyphs);
    let size = (width * height) as usize;
    let mut lines = Lines {
        width,
        sides: vec![0; size],
        stroke: vec![Stroke::Solid; size],
        lit: vec![false; size],
    };
    let mut fences = vec![0u8; size];

    // The quietest thing drawn, under the routes rather than beside them:
    // a wall at the routes' own weight is another line among them, which
    // is the whole of what made a region hard to find. What delimits it
    // is the ground it stands on — see `architect::grounded` — and the
    // wall only has to be the frontier of that, not a rule competing for
    // the eye. The title above it is what carries the region's weight.
    for (frame, cluster) in scene.placement.clusters.iter().zip(&scene.graph.clusters) {
        canvas.frame(*frame, Corners::Square, Role::Faint);
        canvas.text(
            frame.x + 2,
            frame.y,
            &format!(" {} ", cluster.title),
            Role::Secondary,
            true,
        );
        for x in frame.x + 1..frame.x + frame.w - 1 {
            fences[lines.index(x, frame.y)] = EAST | WEST;
            fences[lines.index(x, frame.y + frame.h - 1)] = EAST | WEST;
        }
        for y in frame.y + 1..frame.y + frame.h - 1 {
            fences[lines.index(frame.x, y)] = NORTH | SOUTH;
            fences[lines.index(frame.x + frame.w - 1, y)] = NORTH | SOUTH;
        }
    }

    let mut arrows = Vec::new();
    for route in &scene.routes.routes {
        let edge = &scene.graph.edges[route.edge];
        let lit = scene.touches(route, selected);
        for (position, &(x, y, heading)) in route.cells.iter().enumerate() {
            let index = lines.index(x, y);
            let back = 1 << ((heading + 2) % 4);
            if position == 0 && edge.arrow_back {
                arrows.push((x, y, back, lit));
            }
            let onward = match route.cells.get(position + 1) {
                Some(&(_, _, next)) => 1 << next,
                None if edge.arrow => {
                    arrows.push((x, y, 1u8 << heading, lit));
                    0
                }
                None => 1 << heading,
            };
            // On a shared trunk the plainest stroke wins: a dotted edge
            // riding a solid one must not make the solid one look optional.
            if lines.sides[index] == 0 || edge.stroke != Stroke::Dotted {
                lines.stroke[index] = edge.stroke;
            }
            lines.sides[index] |= back | onward;
            lines.lit[index] |= lit;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let index = lines.index(x, y);
            if lines.sides[index] != 0 {
                let role = if lines.lit[index] {
                    Role::Accent
                } else {
                    Role::Dim
                };
                canvas.line(
                    x,
                    y,
                    lines.sides[index] | fences[index],
                    lines.stroke[index],
                    role,
                );
            }
        }
    }
    for (x, y, heading, lit) in arrows {
        let role = if lit { Role::Accent } else { Role::Muted };
        canvas.put(x, y, arrow_glyph(heading, glyphs), role, false);
    }

    for (index, node) in scene.graph.nodes.iter().enumerate() {
        paint_node(
            &mut canvas,
            node,
            scene.placement.nodes[index],
            selected == Some(index),
        );
        let leads = leads.get(index).copied().unwrap_or_default();
        let frame = scene.placement.nodes[index];
        if leads != Leads::Nowhere {
            let cells = mark_cells(frame);
            let glyph = canvas::leads_glyph(leads == Leads::ToCode, glyphs);
            canvas.text(cells.x, cells.y, &format!(" {glyph} "), Role::Accent, true);
        }
        if let Some(cells) = code_mark_cells(frame, leads) {
            let glyph = canvas::leads_glyph(true, glyphs);
            canvas.text(cells.x, cells.y, &format!(" {glyph} "), Role::Accent, true);
        }
    }
    let mut markers = Vec::new();
    for route in &scene.routes.routes {
        let edge = &scene.graph.edges[route.edge];
        if let Some(label) = edge.label.as_deref() {
            let role = if scene.touches(route, selected) {
                Role::Accent
            } else {
                Role::Secondary
            };
            // A label with nowhere to go is not written over a line: a
            // marker stands for it, and the key says what it was. C4 asks
            // for every line to be labelled, and a missing word is a line
            // that says nothing.
            if write_inline(&mut canvas, &mut lines, &scene.routes, route, label, role)
                || write_beside(&mut canvas, &lines, &scene.routes, route, label, role)
            {
                continue;
            }
            let placed_so_far = markers
                .iter()
                .filter(|m: &&Marker| m.marker.is_some())
                .count();
            let marker = marker_for(label, placed_so_far + 1);
            let placed = write_inline(&mut canvas, &mut lines, &scene.routes, route, &marker, role)
                || write_beside(&mut canvas, &lines, &scene.routes, route, &marker, role);
            markers.push(Marker {
                marker: placed.then_some(marker),
                from: scene.graph.nodes[edge.from].title.clone(),
                to: scene.graph.nodes[edge.to].title.clone(),
                label: label.to_owned(),
            });
        }
    }
    let entries = key(scene, leads, &markers, glyphs);
    with_key(canvas, &entries)
}

/// A label left off its line, and what stands for it there, if anything
/// found room.
struct Marker {
    marker: Option<String>,
    from: String,
    to: String,
    label: String,
}

/// What stands on a line for its label: a dynamic step's own number, or a
/// count of the labels moved to the key so far.
fn marker_for(label: &str, count: usize) -> String {
    let step = label
        .split_once(". ")
        .map(|(step, _)| step)
        .filter(|step| !step.is_empty() && step.chars().all(|c| c.is_ascii_digit()));
    match step {
        Some(step) => format!("{step}."),
        None => format!("[{count}]"),
    }
}

/// The key C4 asks every diagram to carry: what the drawing's notation
/// means, a line each, and only for what this drawing uses. Mermaid has no
/// way to write one, so it is read off the drawing itself.
fn key(
    scene: &Scene,
    leads: &[Leads],
    markers: &[Marker],
    glyphs: Glyphs,
) -> Vec<(String, String)> {
    let graph = &scene.graph;
    let c4 = graph.view.is_some();
    let uses = |shape: Shape| graph.nodes.iter().any(|node| node.shape == shape);
    let leading = |wanted: &[Leads]| leads.iter().any(|lead| wanted.contains(lead));
    let mut entries: Vec<(String, String)> = Vec::new();
    if c4 && !graph.edges.is_empty() {
        let arrow = arrow_glyph(EAST, glyphs);
        entries.push((
            format!("{arrow}"),
            "a relationship, pointing the way it runs: what it does [and how]".to_owned(),
        ));
    }
    if c4 && graph.edges.iter().any(|edge| edge.arrow_back) {
        entries.push((
            "two heads".to_owned(),
            "a relationship both ways".to_owned(),
        ));
    }
    if c4 && uses(Shape::Person) {
        entries.push(("rounded".to_owned(), "a person".to_owned()));
    }
    if uses(Shape::Database) {
        entries.push(("lid".to_owned(), "a store of data".to_owned()));
    }
    if uses(Shape::Queue) {
        entries.push(("second wall".to_owned(), "a queue".to_owned()));
    }
    if c4 && graph.nodes.iter().any(|node| node.external) {
        entries.push((
            "faint border".to_owned(),
            "outside the system being described".to_owned(),
        ));
    }
    if c4 && !graph.clusters.is_empty() {
        entries.push((
            "frame".to_owned(),
            "a boundary, its kind in brackets".to_owned(),
        ));
    }
    if leading(&[Leads::Inside, Leads::Both]) {
        entries.push((
            canvas::leads_glyph(false, glyphs).to_owned(),
            "another view zooms into it".to_owned(),
        ));
    }
    if leading(&[Leads::ToCode, Leads::Both]) {
        entries.push((
            canvas::leads_glyph(true, glyphs).to_owned(),
            "opens its code".to_owned(),
        ));
    }
    for marker in markers {
        let arrow = arrow_glyph(EAST, glyphs);
        entries.push((
            marker.marker.clone().unwrap_or_else(|| {
                match glyphs {
                    Glyphs::Unicode => "—",
                    Glyphs::Ascii => "-",
                }
                .to_owned()
            }),
            format!("{} {arrow} {}: {}", marker.from, marker.to, marker.label),
        ));
    }
    entries
}

/// The drawing with its key under it, a row apart.
fn with_key(drawing: Canvas, entries: &[(String, String)]) -> Canvas {
    if entries.is_empty() {
        return drawing;
    }
    let sample = entries
        .iter()
        .map(|(sample, _)| text_width(sample))
        .max()
        .unwrap_or(0);
    let widest = entries
        .iter()
        .map(|(_, meaning)| sample + 2 + text_width(meaning))
        .max()
        .unwrap_or(0);
    let top = drawing.height + 1;
    let mut canvas = Canvas::new(
        drawing.width.max(widest + 2),
        top + 1 + entries.len() as i32,
        drawing.glyphs,
    );
    for y in 0..drawing.height {
        for x in 0..drawing.width {
            if let Some(cell) = drawing.cell(x, y) {
                canvas.set(x, y, cell);
            }
        }
    }
    canvas.text(1, top, "Key", Role::Secondary, true);
    for (row, (glyph, meaning)) in entries.iter().enumerate() {
        let y = top + 1 + row as i32;
        canvas.text(1, y, glyph, Role::Accent, false);
        canvas.text(1 + sample + 2, y, meaning, Role::Muted, false);
    }
    canvas
}

fn paint_node(canvas: &mut Canvas, node: &Node, frame: Frame, selected: bool) {
    let corners = match node.shape {
        Shape::Box => Corners::Square,
        Shape::Decision => Corners::Marked,
        Shape::Round | Shape::Database | Shape::Person | Shape::Queue => Corners::Rounded,
    };
    let border = match (selected, node.external) {
        (true, _) => Role::Accent,
        (_, true) => Role::Faint,
        _ => Role::Muted,
    };
    for y in frame.y..frame.y + frame.h {
        for x in frame.x..frame.x + frame.w {
            canvas.put(x, y, ' ', Role::Default, false);
        }
    }
    canvas.frame(frame, corners, border);
    let mut row = frame.y + 1;
    if node.shape == Shape::Database {
        let right = frame.x + frame.w - 1;
        canvas.line(frame.x, row, NORTH | EAST | SOUTH, Stroke::Solid, border);
        canvas.line(right, row, NORTH | SOUTH | WEST, Stroke::Solid, border);
        for x in frame.x + 1..right {
            canvas.line(x, row, EAST | WEST, Stroke::Solid, border);
        }
        row += 1;
    }
    let wall = layout::queue_wall(node);
    if wall > 0 {
        let x = frame.x + frame.w - 2;
        for y in frame.y + 1..frame.y + frame.h - 1 {
            canvas.line(x, y, NORTH | SOUTH, Stroke::Solid, border);
        }
    }
    for (kind, text) in layout::node_text(node) {
        let (role, bold) = match (kind, selected, node.external) {
            (TextKind::Title, true, _) => (Role::Accent, true),
            (TextKind::Title, _, true) => (Role::Muted, false),
            (TextKind::Title, ..) => (Role::Bright, true),
            (TextKind::Kind, ..) => (Role::Dim, false),
            _ => (Role::Muted, false),
        };
        let x = frame.x + 1 + (frame.w - 2 - wall - text_width(&text)) / 2;
        canvas.text(x, row, &text, role, bold);
        row += 1;
    }
}

/// On the line itself, where a horizontal run is long enough to be
/// interrupted by it — and preferably a run no other edge shares.
fn write_inline(
    canvas: &mut Canvas,
    lines: &mut Lines,
    routes: &Routes,
    route: &Route,
    label: &str,
    role: Role,
) -> bool {
    let needed = text_width(label) + 2;
    for allow_shared in [false, true] {
        let mut run: Vec<(i32, i32)> = Vec::new();
        let body = &route.cells[..route.cells.len().saturating_sub(1)];
        for &(x, y, _) in body.iter().chain(std::iter::once(&(i32::MIN, 0, 0))) {
            let plain = x != i32::MIN
                && lines.sides[lines.index(x, y)] == EAST | WEST
                && (allow_shared || !routes.is_shared(x, y));
            let continues = plain
                && run
                    .last()
                    .is_none_or(|&(px, py)| py == y && (px - x).abs() == 1);
            if continues {
                run.push((x, y));
                continue;
            }
            if run.len() as i32 >= needed + 2 {
                let left = run.iter().map(|&(x, _)| x).min().unwrap_or(0);
                let start = left + (run.len() as i32 - needed) / 2;
                canvas.text(start, run[0].1, &format!(" {label} "), role, false);
                // No longer a line, so the next label cannot land on this one.
                for x in start..start + needed {
                    let index = lines.index(x, run[0].1);
                    lines.sides[index] = 0;
                }
                return true;
            }
            run.clear();
            if plain {
                run.push((x, y));
            }
        }
    }
    false
}

/// Beside a vertical run, nearest its middle, wherever the cells are
/// free — on one row, or broken over two when one is too narrow a gap.
fn write_beside(
    canvas: &mut Canvas,
    lines: &Lines,
    routes: &Routes,
    route: &Route,
    label: &str,
    role: Role,
) -> bool {
    let broken = broken_in_two(label);
    let shapes: Vec<Vec<&str>> = std::iter::once(vec![label])
        .chain(
            broken
                .as_ref()
                .map(|(first, second)| vec![first.as_str(), second.as_str()]),
        )
        .collect();
    for rows in &shapes {
        let width = rows.iter().map(|row| text_width(row)).max().unwrap_or(0);
        for allow_shared in [false, true] {
            let upright: Vec<(i32, i32)> = route
                .cells
                .iter()
                .map(|&(x, y, _)| (x, y))
                .filter(|&(x, y)| lines.sides[lines.index(x, y)] == NORTH | SOUTH)
                .filter(|&(x, y)| allow_shared || !routes.is_shared(x, y))
                .collect();
            let middle = upright.len() / 2;
            let mut nearest: Vec<usize> = (0..upright.len()).collect();
            nearest.sort_by_key(|&index| index.abs_diff(middle));
            for index in nearest {
                let (x, y) = upright[index];
                for start in [x + 2, x - 1 - width] {
                    let free = (0..rows.len() as i32).all(|row| {
                        (start - 1..=start + width).all(|column| canvas.is_blank(column, y + row))
                    });
                    if free {
                        for (row, text) in rows.iter().enumerate() {
                            canvas.text(start, y + row as i32, text, role, false);
                        }
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// The label split at the space nearest its middle, if it has one.
fn broken_in_two(label: &str) -> Option<(String, String)> {
    let middle = label.len() / 2;
    let at = label
        .match_indices(' ')
        .map(|(index, _)| index)
        .min_by_key(|index| index.abs_diff(middle))?;
    Some((label[..at].to_owned(), label[at + 1..].to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architect::{mermaid, model::Diagram};

    fn scene(source: &str) -> Scene {
        match mermaid::parse(source).expect("the diagram parses") {
            Diagram::Graph(graph) => Scene::of(graph),
            Diagram::Sequence(_) => panic!("expected a graph"),
        }
    }

    fn glyph(canvas: &Canvas, x: i32, y: i32) -> char {
        canvas.cell(x, y).map_or(' ', |cell| cell.glyph)
    }

    fn row(canvas: &Canvas, y: i32) -> String {
        (0..canvas.width).map(|x| glyph(canvas, x, y)).collect()
    }

    /// Where `text` first appears, as (column, row).
    fn find(canvas: &Canvas, text: &str) -> Option<(i32, i32)> {
        (0..canvas.height).find_map(|y| {
            let line: Vec<char> = row(canvas, y).chars().collect();
            let wanted: Vec<char> = text.chars().collect();
            line.windows(wanted.len())
                .position(|w| w == wanted.as_slice())
                .map(|x| (x as i32, y))
        })
    }

    /// The sides a box-drawing glyph leaves its cell by.
    fn leaves_by(glyph: char) -> u8 {
        match glyph {
            '│' | '┆' | '┃' => NORTH | SOUTH,
            '─' | '┄' | '━' => EAST | WEST,
            '└' | '╰' => NORTH | EAST,
            '┌' | '╭' => EAST | SOUTH,
            '┐' | '╮' => SOUTH | WEST,
            '┘' | '╯' => NORTH | WEST,
            '├' => NORTH | EAST | SOUTH,
            '┤' => NORTH | SOUTH | WEST,
            '┬' => EAST | SOUTH | WEST,
            '┴' => NORTH | EAST | WEST,
            '┼' => NORTH | EAST | SOUTH | WEST,
            _ => 0,
        }
    }

    /// A heading of the router (north, east, south, west) as a side.
    fn side(heading: usize) -> u8 {
        1 << heading
    }

    fn toward(heading: usize) -> char {
        arrow_glyph(side(heading), Glyphs::Unicode)
    }

    const TANGLE: &str =
        "flowchart TD\n a --> b & c & d\n b --> e\n c --> e\n d --> a\n e --> a\n b --> d";

    /// Every line is drawn as one: each cell joins the one before and the
    /// one after it, the first touches its source and the last its target,
    /// with a head wherever the edge has one.
    #[test]
    fn every_line_is_drawn_unbroken_from_its_source_to_its_target() {
        for source in [
            TANGLE,
            "flowchart LR\n a --> b\n b --> c\n a --> c\n c --> a",
            "flowchart LR\n a --- b\n b --- c\n a --- c",
            "C4Context\nPerson(u, \"User\")\nSystem(s, \"Shop\")\nBiRel(u, s, \"Uses\")",
        ] {
            let scene = scene(source);
            let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
            assert_eq!(scene.routes.unrouted, 0);
            for route in &scene.routes.routes {
                let edge = &scene.graph.edges[route.edge];
                let last = route.cells.len() - 1;
                for (position, &(x, y, heading)) in route.cells.iter().enumerate() {
                    let drawn = glyph(&canvas, x, y);
                    let at = format!("{source}: ({x},{y}) {drawn:?}");
                    if position == 0 && edge.arrow_back {
                        assert_eq!(drawn, toward((heading + 2) % 4), "a head back: {at}");
                        continue;
                    }
                    if position == last && edge.arrow {
                        assert_eq!(drawn, toward(heading), "a head on: {at}");
                        continue;
                    }
                    let back = side((heading + 2) % 4);
                    let on = route
                        .cells
                        .get(position + 1)
                        .map_or(side(heading), |&(_, _, next)| side(next));
                    let sides = leaves_by(drawn);
                    assert_eq!(sides & back, back, "joins what is behind: {at}");
                    assert_eq!(sides & on, on, "joins what is ahead: {at}");
                }
            }
        }
    }

    #[test]
    fn the_lines_of_a_selected_box_are_lit_and_no_other() {
        let scene = scene(TANGLE);
        let selected = 1;
        let canvas = paint(&scene, Glyphs::Unicode, Some(selected), &[]);
        let quiet = paint(&scene, Glyphs::Unicode, None, &[]);
        for route in &scene.routes.routes {
            let edge = &scene.graph.edges[route.edge];
            let touches = edge.from == selected || edge.to == selected;
            let &(x, y, _) = route.cells.last().unwrap();
            let head = canvas.cell(x, y).unwrap().role;
            assert_eq!(head == Role::Accent, touches, "the head of {edge:?}");
            assert_eq!(quiet.cell(x, y).unwrap().role, Role::Muted);
            let (x, y, _) = route.cells[0];
            assert_eq!(quiet.cell(x, y).unwrap().role, Role::Dim);
            if touches {
                assert_eq!(canvas.cell(x, y).unwrap().role, Role::Accent);
            }
        }
    }

    #[test]
    fn a_dotted_edge_is_dotted_until_it_rides_a_solid_ones_trunk() {
        let scene = scene("flowchart TD\n a --> b\n a -.-> c\n a --> d\n a --> e\n a --> f");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let dotted = scene
            .routes
            .routes
            .iter()
            .find(|route| scene.graph.edges[route.edge].stroke == Stroke::Dotted)
            .unwrap();
        let mut alone = 0;
        for &(x, y, _) in &dotted.cells {
            let drawn = glyph(&canvas, x, y);
            if scene.routes.is_shared(x, y) {
                assert!(!matches!(drawn, '┆' | '┄'), "({x},{y}) on the trunk");
            } else if matches!(leaves_by(drawn), s if s == NORTH | SOUTH || s == EAST | WEST) {
                assert!(matches!(drawn, '┆' | '┄'), "({x},{y}) {drawn:?}");
                alone += 1;
            }
        }
        assert!(alone > 0, "some of it runs alone");
        assert!(
            dotted
                .cells
                .iter()
                .any(|&(x, y, _)| scene.routes.is_shared(x, y)),
            "some of it rides a trunk"
        );
    }

    #[test]
    fn a_line_crossing_a_regions_wall_joins_it() {
        for source in [
            "flowchart TD\n subgraph R [Region]\n b\n end\n a --> b\n b --> c",
            "flowchart LR\n subgraph R [Region]\n b\n end\n a --> b\n b --> c",
        ] {
            let scene = scene(source);
            let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
            let wall = scene.placement.clusters[0];
            let on_the_wall = |x: i32, y: i32| {
                wall.contains(x, y)
                    && (x == wall.x
                        || y == wall.y
                        || x == wall.x + wall.w - 1
                        || y == wall.y + wall.h - 1)
            };
            let crossings: Vec<(i32, i32)> = scene
                .routes
                .routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|&(x, y, _)| (x, y)))
                .filter(|&(x, y)| on_the_wall(x, y))
                .collect();
            assert_eq!(crossings.len(), 2, "{source}: in and out");
            for (x, y) in crossings {
                assert_eq!(glyph(&canvas, x, y), '┼', "{source}: ({x},{y})");
            }
        }
    }

    #[test]
    fn a_region_is_a_faint_frame_with_its_title_on_top() {
        let scene = scene("flowchart TD\n subgraph R [Region]\n a --> b\n end");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let Frame { x, y, w, h } = scene.placement.clusters[0];
        let (right, bottom) = (x + w - 1, y + h - 1);
        for (cx, cy, corner) in [
            (x, y, '┌'),
            (right, y, '┐'),
            (x, bottom, '└'),
            (right, bottom, '┘'),
        ] {
            assert_eq!(glyph(&canvas, cx, cy), corner);
            assert_eq!(canvas.cell(cx, cy).unwrap().role, Role::Faint);
        }
        assert_eq!(glyph(&canvas, x, y + 1), '│');
        assert_eq!(glyph(&canvas, right, bottom - 1), '│');
        assert_eq!(glyph(&canvas, right - 1, bottom), '─');
        assert_eq!(find(&canvas, " Region "), Some((x + 2, y)));
        let title = canvas.cell(x + 3, y).unwrap();
        assert_eq!((title.role, title.bold), (Role::Secondary, true));
    }

    /// The frame of node `index`, and the text on each row inside it.
    fn inside(scene: &Scene, canvas: &Canvas, index: usize) -> (Frame, Vec<String>) {
        let frame = scene.placement.nodes[index];
        let rows = (frame.y + 1..frame.y + frame.h - 1)
            .map(|y| {
                (frame.x + 1..frame.x + frame.w - 1)
                    .map(|x| glyph(canvas, x, y))
                    .collect()
            })
            .collect();
        (frame, rows)
    }

    /// Whether `text` sits on `row` with as much room on its left as on
    /// its right, give or take the odd column.
    fn centred(row: &str, text: &str) -> bool {
        let Some(at) = row.find(text) else {
            return false;
        };
        let left = row[..at].chars().count();
        let right = row[at + text.len()..].chars().count();
        left.abs_diff(right) <= 1
    }

    #[test]
    fn every_box_is_framed_in_its_shape_where_it_was_placed_with_its_title_centred() {
        let scene =
            scene("flowchart TD\n a[Alpha] --> b(Beta)\n a --> d{Choice}\n b --> e[Epsilon wide]");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        for (index, title, corners) in [
            (0, "Alpha", ['┌', '┐', '└', '┘']),
            (1, "Beta", ['╭', '╮', '╰', '╯']),
            (2, "Choice", ['◇'; 4]),
            (3, "Epsilon wide", ['┌', '┐', '└', '┘']),
        ] {
            let (frame, rows) = inside(&scene, &canvas, index);
            let Frame { x, y, w, h } = frame;
            let (right, bottom) = (x + w - 1, y + h - 1);
            assert_eq!(
                [
                    glyph(&canvas, x, y),
                    glyph(&canvas, right, y),
                    glyph(&canvas, x, bottom),
                    glyph(&canvas, right, bottom),
                ],
                corners,
                "{title}"
            );
            for column in x + 1..right {
                assert_eq!(glyph(&canvas, column, y), '─', "{title} top");
            }
            for line in y + 1..bottom {
                assert_eq!(glyph(&canvas, x, line), '│', "{title} left");
                assert_eq!(glyph(&canvas, right, line), '│', "{title} right");
            }
            assert!(centred(&rows[0], title), "{title}: {rows:?}");
            for line in y..=bottom {
                for column in x..=right {
                    assert!(!canvas.is_blank(column, line), "{title}: the box is solid");
                }
            }
        }
    }

    #[test]
    fn a_box_says_what_it_is_a_line_at_a_time_under_its_title() {
        let scene = scene(
            "C4Container\nContainer(api, \"API\", \"Rust\", \"Serves requests\")\n\
             System_Ext(bank, \"Bank\", \"Pays\")\nRel(api, bank, \"Charges\")",
        );
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let (frame, rows) = inside(&scene, &canvas, 0);
        assert!(centred(&rows[0], "API"), "{rows:?}");
        assert!(centred(&rows[1], "[Container: Rust]"), "{rows:?}");
        assert!(rows[2].trim().is_empty(), "{rows:?}");
        assert!(centred(&rows[3], "Serves requests"), "{rows:?}");
        let role_of = |text: &str, row: i32| {
            let line = row_of(&canvas, frame, row);
            let at = line.find(text).unwrap();
            let x = frame.x + 1 + line[..at].chars().count() as i32;
            let cell = canvas.cell(x, frame.y + 1 + row).unwrap();
            (cell.role, cell.bold)
        };
        assert_eq!(role_of("API", 0), (Role::Bright, true));
        assert_eq!(role_of("[Container", 1), (Role::Dim, false));
        assert_eq!(role_of("Serves", 3), (Role::Muted, false));
        assert_eq!(canvas.cell(frame.x, frame.y).unwrap().role, Role::Muted);

        let bank = scene.placement.nodes[1];
        let line = row_of(&canvas, bank, 0);
        let at = line.find("Bank").unwrap();
        let title = canvas
            .cell(bank.x + 1 + line[..at].chars().count() as i32, bank.y + 1)
            .unwrap();
        assert_eq!(
            (title.role, title.bold),
            (Role::Muted, false),
            "outside: quieter"
        );
        assert_eq!(canvas.cell(bank.x, bank.y).unwrap().role, Role::Faint);
    }

    fn row_of(canvas: &Canvas, frame: Frame, row: i32) -> String {
        (frame.x + 1..frame.x + frame.w - 1)
            .map(|x| glyph(canvas, x, frame.y + 1 + row))
            .collect()
    }

    #[test]
    fn a_selected_box_is_drawn_in_the_accent_whatever_else_it_is() {
        let scene = scene(
            "C4Container\nContainer(api, \"API\", \"Rust\", \"Serves\")\n\
             System_Ext(bank, \"Bank\", \"Pays\")\nRel(api, bank, \"Charges\")",
        );
        for selected in [0, 1] {
            let canvas = paint(&scene, Glyphs::Unicode, Some(selected), &[]);
            let frame = scene.placement.nodes[selected];
            assert_eq!(canvas.cell(frame.x, frame.y).unwrap().role, Role::Accent);
            let line = row_of(&canvas, frame, 0);
            let at = line.len() - line.trim_start().len();
            let title = canvas.cell(frame.x + 1 + at as i32, frame.y + 1).unwrap();
            assert_eq!((title.role, title.bold), (Role::Accent, true), "{line}");
        }
    }

    #[test]
    fn a_store_has_a_lid_with_its_title_under_it() {
        let scene = scene("flowchart TD\n s[(Store)]");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let (frame, rows) = inside(&scene, &canvas, 0);
        let lid = frame.y + 1;
        assert_eq!(glyph(&canvas, frame.x, lid), '├');
        assert_eq!(glyph(&canvas, frame.x + frame.w - 1, lid), '┤');
        assert!(rows[0].chars().all(|c| c == '─'), "{rows:?}");
        assert!(centred(&rows[1], "Store"), "{rows:?}");
    }

    #[test]
    fn a_queue_has_a_second_wall_with_its_words_centred_beside_it() {
        let scene = scene("C4Container\nContainerQueue(q, \"Jobs\", \"Redis\", \"Holds work\")");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let (frame, rows) = inside(&scene, &canvas, 0);
        let wall = frame.x + frame.w - 2;
        for y in frame.y + 1..frame.y + frame.h - 1 {
            assert_eq!(glyph(&canvas, wall, y), '│', "{rows:?}");
        }
        assert_eq!(
            glyph(&canvas, wall, frame.y),
            '─',
            "the wall stays inside the border"
        );
        assert_eq!(glyph(&canvas, wall, frame.y + frame.h - 1), '─');
        for (row, text) in [(0, "Jobs"), (1, "[Container: Redis]"), (3, "Holds work")] {
            let before_the_wall: String = rows[row]
                .chars()
                .take(rows[row].chars().count() - 1)
                .collect();
            assert!(centred(&before_the_wall, text), "{rows:?}");
        }
    }

    #[test]
    fn a_mark_sits_on_the_right_of_a_boxs_lower_border() {
        let frame = Frame {
            x: 10,
            y: 4,
            w: 12,
            h: 5,
        };
        let mark = mark_cells(frame);
        assert_eq!(mark.y, frame.y + frame.h - 1, "on the lower border");
        assert_eq!(mark.h, 1);
        assert!(mark.x > frame.x + frame.w / 2, "on its right");
        assert!(mark.x + mark.w < frame.x + frame.w, "short of the corner");
        let code = code_mark_cells(frame, Leads::Both).expect("room for two");
        assert_eq!(code.y, mark.y);
        assert!(code.x > frame.x, "short of the corner");
        assert!(
            code.x + code.w <= mark.x && code.x + code.w + 1 >= mark.x,
            "just left of the first"
        );
        assert_eq!(code_mark_cells(frame, Leads::Inside), None);
        assert_eq!(code_mark_cells(frame, Leads::ToCode), None);
        let narrow = Frame { w: 10, ..frame };
        assert_eq!(code_mark_cells(narrow, Leads::Both), None);
        assert!(code_mark_cells(Frame { w: 11, ..frame }, Leads::Both).is_some());
    }

    #[test]
    fn a_box_that_leads_somewhere_carries_its_mark_on_its_border() {
        let scene = scene(
            "flowchart LR\n a[A wide box] --> b[Another one] --> c[And a third] --> d[A fourth one]",
        );
        let leads = [Leads::Inside, Leads::ToCode, Leads::Both, Leads::Nowhere];
        let canvas = paint(&scene, Glyphs::Unicode, None, &leads);
        let mark_at = |cells: Frame| -> String {
            (cells.x..cells.x + cells.w)
                .map(|x| glyph(&canvas, x, cells.y))
                .collect()
        };
        let frames = &scene.placement.nodes;
        assert_eq!(mark_at(mark_cells(frames[0])), " » ─");
        assert_eq!(mark_at(mark_cells(frames[1])), " ↗ ─");
        assert_eq!(mark_at(mark_cells(frames[2])), " » ─");
        assert_eq!(
            mark_at(code_mark_cells(frames[2], Leads::Both).unwrap()),
            " ↗ ─"
        );
        assert_eq!(mark_at(mark_cells(frames[3])), "────");
        let mark = canvas
            .cell(mark_cells(frames[0]).x + 1, mark_cells(frames[0]).y)
            .unwrap();
        assert_eq!(mark.role, Role::Accent);
    }

    #[test]
    fn a_label_is_written_into_a_long_enough_level_stretch_of_its_line() {
        let scene = scene("flowchart LR\n a[Alpha] -->|writes| b[Beta]");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let route = &scene.routes.routes[0];
        let (x, y) = find(&canvas, " writes ").expect("written on the line");
        let end = x + " writes ".len() as i32;
        let run: Vec<i32> = route
            .cells
            .iter()
            .filter(|&&(_, cy, _)| cy == y)
            .map(|&(cx, _, _)| cx)
            .collect();
        let (first, last) = (*run.iter().min().unwrap(), *run.iter().max().unwrap());
        assert!(
            first < x && end <= last,
            "inside the line's run: {}",
            row(&canvas, y)
        );
        assert_eq!(glyph(&canvas, x - 1, y), '─', "{}", row(&canvas, y));
        assert_eq!(glyph(&canvas, end, y), '─', "{}", row(&canvas, y));
        assert!(
            (x - first).abs_diff(last - end) <= 1,
            "in the middle of the run: {}",
            row(&canvas, y)
        );
        let cell = canvas.cell(x + 1, y).unwrap();
        assert_eq!(cell.role, Role::Secondary);
        let lit = paint(&scene, Glyphs::Unicode, Some(0), &[]);
        assert_eq!(lit.cell(x + 1, y).unwrap().role, Role::Accent);
    }

    #[test]
    fn a_label_with_no_level_stretch_stands_beside_the_middle_of_its_line() {
        let scene =
            scene("flowchart TD\n a[A very wide box title] -->|writes| b[Another wide box]");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let route = &scene.routes.routes[0];
        let (x, y) = find(&canvas, "writes").expect("written beside the line");
        let &(line, _, _) = route
            .cells
            .iter()
            .find(|&&(_, cy, _)| cy == y)
            .expect("on a row of the line");
        let end = x + "writes".len() as i32;
        assert!(
            x == line + 2 || end == line - 1,
            "a column apart: {}",
            row(&canvas, y)
        );
        let rows: Vec<i32> = route.cells.iter().map(|&(_, cy, _)| cy).collect();
        let middle = (rows.iter().min().unwrap() + rows.iter().max().unwrap()) / 2;
        assert!((y - middle).abs() <= 1, "row {y}, middle {middle}");
    }

    #[test]
    fn a_label_is_broken_at_the_space_nearest_its_middle() {
        assert_eq!(
            broken_in_two("one two three"),
            Some(("one two".to_owned(), "three".to_owned()))
        );
        assert_eq!(
            broken_in_two("aa bbbbbbbb"),
            Some(("aa".to_owned(), "bbbbbbbb".to_owned()))
        );
        assert_eq!(
            broken_in_two("aaaa bb cc dd"),
            Some(("aaaa bb".to_owned(), "cc dd".to_owned()))
        );
        assert_eq!(broken_in_two("single"), None);
    }

    #[test]
    fn a_label_moved_to_the_key_is_stood_for_by_its_step_or_its_count() {
        assert_eq!(marker_for("3. Calls the API", 1), "3.");
        assert_eq!(marker_for("12. Calls", 1), "12.");
        assert_eq!(marker_for("Calls the API", 2), "[2]");
        assert_eq!(marker_for(". Calls", 1), "[1]");
        assert_eq!(marker_for("3a. Calls", 4), "[4]");
    }

    #[test]
    fn a_label_wider_than_the_drawing_is_moved_to_the_key() {
        let long = "x".repeat(200);
        let longer = "y".repeat(210);
        let scene = scene(&format!(
            "flowchart TD\n a[Alpha] -->|{long}| b[Beta]\n b -->|{longer}| c[Gamma]\n c -->|fits| d[Delta]"
        ));
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        assert!(
            find(&canvas, "fits").is_some(),
            "a label that fits stays on its line"
        );
        let (_, key) = find(&canvas, "Key").expect("a key");
        let (_, first) = find(&canvas, "[1]").expect("a marker");
        let (_, second) = find(&canvas, "[2]").expect("a second marker");
        assert!(
            first < key && second < key,
            "the markers stand on the drawing"
        );
        assert!(
            find(&canvas, &format!("Alpha ► Beta: {long}")).is_some_and(|(_, y)| y > key),
            "the key says what the first was"
        );
        let (x, y) = find(&canvas, &format!("Beta ► Gamma: {longer}")).unwrap();
        assert_eq!(glyph(&canvas, 1, y), '[');
        assert_eq!(&row(&canvas, y)[..4], " [2]");
        assert!(x > 4);
    }

    #[test]
    fn a_dynamic_steps_number_stands_on_its_line_for_a_label_moved_to_the_key() {
        let long = "z".repeat(200);
        let scene = scene(&format!(
            "C4Dynamic\nContainer(a, \"Alpha\")\nContainer(b, \"Beta\")\nRel(a, b, \"{long}\")"
        ));
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let (_, key) = find(&canvas, "Key").expect("a key");
        let (_, marker) = find(&canvas, "1.").expect("the step's number");
        assert!(marker < key);
        assert!(find(&canvas, &format!("Alpha ► Beta: 1. {long}")).is_some());
    }

    #[test]
    fn a_key_entry_with_no_room_for_a_marker_says_so_with_a_dash() {
        let scene =
            scene("C4Context\nSystem(a, \"Alpha\")\nSystem(b, \"Beta\")\nRel(a, b, \"Uses\")");
        let markers = [Marker {
            marker: None,
            from: "Alpha".to_owned(),
            to: "Beta".to_owned(),
            label: "Uses".to_owned(),
        }];
        let unicode = key(&scene, &[], &markers, Glyphs::Unicode);
        assert_eq!(
            unicode.last().unwrap(),
            &("—".to_owned(), "Alpha ► Beta: Uses".to_owned())
        );
        let ascii = key(&scene, &[], &markers, Glyphs::Ascii);
        assert_eq!(
            ascii.last().unwrap(),
            &("-".to_owned(), "Alpha > Beta: Uses".to_owned())
        );
    }

    fn samples(scene: &Scene, leads: &[Leads]) -> Vec<String> {
        key(scene, leads, &[], Glyphs::Unicode)
            .into_iter()
            .map(|(sample, _)| sample)
            .collect()
    }

    #[test]
    fn the_key_names_only_the_notation_the_drawing_uses() {
        assert!(samples(&scene("flowchart TD\n a --> b"), &[]).is_empty());
        assert_eq!(
            samples(
                &scene("flowchart TD\n subgraph R\n a[(Db)] --> b\n end"),
                &[]
            ),
            ["lid"],
            "a frame is a C4 boundary's notation"
        );
        assert!(samples(&scene("C4Context\nSystem(a, \"A\")"), &[]).is_empty());
        assert_eq!(
            samples(
                &scene("C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a, b, \"Uses\")"),
                &[Leads::Inside]
            ),
            ["►", "»"]
        );
        assert_eq!(
            samples(
                &scene(
                    "C4Container\nPerson(u, \"U\")\nSystem_Boundary(s, \"S\") {\n\
                     ContainerQueue(q, \"Q\")\n}\nSystem_Ext(x, \"X\")\nBiRel(u, q, \"Puts\")"
                ),
                &[Leads::ToCode]
            ),
            [
                "►",
                "two heads",
                "rounded",
                "second wall",
                "faint border",
                "frame",
                "↗"
            ]
        );
        assert_eq!(
            samples(&scene("flowchart TD\n a --> b"), &[Leads::Both]),
            ["»", "↗"]
        );
    }

    #[test]
    fn the_key_stands_a_row_under_the_drawing_in_two_columns() {
        let scene = scene("flowchart TD\n a[(Db)] --> b\n a --> c");
        let leads = [Leads::Both];
        let canvas = paint(&scene, Glyphs::Unicode, None, &leads);
        let below = scene.placement.height + 1;
        assert_eq!(find(&canvas, "Key"), Some((1, below)));
        let entries = key(&scene, &leads, &[], Glyphs::Unicode);
        let widest = entries
            .iter()
            .map(|(sample, _)| text_width(sample))
            .max()
            .unwrap();
        for (index, (sample, meaning)) in entries.iter().enumerate() {
            let y = below + 1 + index as i32;
            assert!(
                row(&canvas, y).starts_with(&format!(" {sample} ")),
                "{sample}"
            );
            assert_eq!(
                find(&canvas, meaning),
                Some((1 + widest + 2, y)),
                "{meaning}"
            );
        }
        assert_eq!(canvas.height, below + 1 + entries.len() as i32);
    }

    #[test]
    fn the_key_is_never_cut_by_a_narrow_drawing() {
        let scene = scene("C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nRel(a, b, \"Uses\")");
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let meaning = "a relationship, pointing the way it runs: what it does [and how]";
        let (x, _) = find(&canvas, meaning).expect("the whole of it");
        assert!(x + text_width(meaning) < canvas.width, "with a margin");
        assert!(scene.placement.width < canvas.width);
    }

    /// A scene of hand-placed boxes, routed as the surface would.
    fn placed(source: &str, nodes: Vec<Frame>, clusters: Vec<Frame>) -> Scene {
        let Diagram::Graph(graph) = mermaid::parse(source).unwrap() else {
            panic!("expected a graph");
        };
        let placement = Placement {
            nodes,
            clusters,
            width: 30,
            height: 30,
        };
        let routes = route::route(&graph, &placement);
        Scene {
            graph,
            placement,
            routes,
        }
    }

    fn small(x: i32, y: i32) -> Frame {
        Frame { x, y, w: 3, h: 3 }
    }

    /// A line that passes beside a region's corner, on the row or column
    /// of its wall, is not joined to the wall it does not cross.
    #[test]
    fn a_line_passing_beside_a_regions_corner_is_not_joined_to_its_wall() {
        let wall = Frame {
            x: 10,
            y: 10,
            w: 10,
            h: 4,
        };
        let (left, right) = (wall.x - 1, wall.x + wall.w);
        let down = placed(
            "flowchart TD\n subgraph r [R]\n m\n end\n a --> b\n c --> d",
            // m, then each column's two boxes centred on it.
            vec![
                small(0, 26),
                small(right - 1, 0),
                small(right - 1, 24),
                small(left - 1, 0),
                small(left - 1, 24),
            ],
            vec![wall],
        );
        let canvas = paint(&down, Glyphs::Unicode, None, &[]);
        for x in [left, right] {
            for y in [wall.y, wall.y + wall.h - 1] {
                assert_eq!(glyph(&canvas, x, y), '│', "({x},{y}): {}", row(&canvas, y));
            }
        }
        // Narrow, so going round costs more than passing it.
        let wall = Frame {
            x: 10,
            y: 10,
            w: 4,
            h: 10,
        };
        let (above, below) = (wall.y - 1, wall.y + wall.h);
        let across = placed(
            "flowchart TD\n subgraph r [R]\n m\n end\n a --> b\n c --> d",
            // m, then each row's two boxes centred on it.
            vec![
                small(26, 26),
                small(0, above - 1),
                small(26, above - 1),
                small(0, below - 1),
                small(26, below - 1),
            ],
            vec![wall],
        );
        let canvas = paint(&across, Glyphs::Unicode, None, &[]);
        for y in [above, below] {
            for x in [wall.x, wall.x + wall.w - 1] {
                assert_eq!(glyph(&canvas, x, y), '─', "({x},{y}): {}", row(&canvas, y));
            }
        }
    }

    /// A line with no head still reaches its box: where another line
    /// crosses the cell beside the box, the two meet as a crossing.
    #[test]
    fn a_line_without_a_head_reaches_into_its_box() {
        let scene = placed(
            "flowchart TD\n a --- b\n c --> d",
            vec![small(10, 0), small(10, 10), small(0, 8), small(20, 8)],
            vec![],
        );
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        assert_eq!(glyph(&canvas, 11, 9), '┼', "{}", row(&canvas, 9));
    }

    /// A level stretch one cell too short for a label and a line on both
    /// sides of it is left alone: the label goes beside the line instead,
    /// and never over a bend.
    #[test]
    fn a_label_is_never_squeezed_into_a_stretch_without_line_on_both_sides() {
        let scene = placed(
            "flowchart TD\n a -->|ab| b",
            vec![small(2, 0), small(8, 12)],
            vec![],
        );
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        let route = &scene.routes.routes[0];
        let level = route
            .cells
            .windows(2)
            .filter(|pair| pair[0].2 == 1 && pair[1].2 == 1)
            .count();
        assert!(level < "ab".len() + 4, "the stretch is too short");
        let (x, y) = find(&canvas, "ab").expect("the label is written");
        for column in x..x + 2 {
            assert!(
                !route
                    .cells
                    .iter()
                    .any(|&(cx, cy, _)| (cx, cy) == (column, y)),
                "({column},{y}) is on the line: {}",
                row(&canvas, y)
            );
        }
    }

    /// A straight line down column `x` from the top of the board to the
    /// bottom, labelled `label`, beside whatever else `others` places.
    fn upright(x: i32, label: &str, others: Vec<Frame>) -> (Scene, Canvas) {
        let mut source = format!("flowchart TD\n a -->|{label}| b");
        let mut nodes = vec![small(x - 1, 0), small(x - 1, 26)];
        for (index, frame) in others.into_iter().enumerate() {
            source.push_str(&format!("\n c{index}"));
            nodes.push(frame);
        }
        let scene = placed(&source, nodes, vec![]);
        let canvas = paint(&scene, Glyphs::Unicode, None, &[]);
        (scene, canvas)
    }

    #[test]
    fn a_label_beside_a_line_keeps_one_column_from_it_on_whichever_side_has_room() {
        let (_, canvas) = upright(4, "writes", vec![]);
        let (x, y) = find(&canvas, "writes").unwrap();
        assert_eq!(x, 6, "right of the line: {}", row(&canvas, y));
        let (_, canvas) = upright(27, "writes", vec![]);
        let (x, y) = find(&canvas, "writes").unwrap();
        assert_eq!(x + 6, 26, "left of the line: {}", row(&canvas, y));
    }

    #[test]
    fn a_label_beside_a_long_line_stands_by_its_middle() {
        let (scene, canvas) = upright(10, "writes", vec![]);
        let rows: Vec<i32> = scene.routes.routes[0]
            .cells
            .iter()
            .map(|&(_, y, _)| y)
            .collect();
        let middle = (rows.iter().min().unwrap() + rows.iter().max().unwrap()) / 2;
        let (_, y) = find(&canvas, "writes").unwrap();
        assert!((y - middle).abs() <= 1, "row {y}, middle {middle}");
    }

    #[test]
    fn a_label_too_wide_for_one_row_beside_its_line_is_broken_over_two() {
        let (first, second) = ("a".repeat(12), "b".repeat(12));
        let (_, canvas) = upright(14, &format!("{first} {second}"), vec![]);
        let (x, y) = find(&canvas, &first).expect("the first half");
        assert_eq!(x, 16);
        assert_eq!(
            find(&canvas, &second),
            Some((x, y + 1)),
            "the second under it"
        );
    }

    #[test]
    fn a_label_never_touches_a_box_beside_its_line() {
        let tall = |x: i32| Frame {
            x,
            y: 2,
            w: 5,
            h: 24,
        };
        for (line, beside) in [(4, tall(12)), (27, tall(15))] {
            let (scene, canvas) = upright(line, "writes", vec![beside]);
            let (_, y) = find(&canvas, "writes").expect("said somewhere");
            assert!(
                y > scene.placement.height,
                "no room beside the line, so it is in the key: {}",
                row(&canvas, y)
            );
        }
    }

    /// Both rows of a broken label must be free: one whose second row
    /// would land on a box moves up to where both are.
    #[test]
    fn a_label_broken_over_two_rows_covers_nothing_with_either() {
        let (first, second) = ("a".repeat(12), "b".repeat(12));
        let label = format!("{first} {second}");
        let (_, open) = upright(12, &label, vec![]);
        let (_, row_on_open) = find(&open, &first).unwrap();
        let below = Frame {
            x: 18,
            y: row_on_open + 1,
            w: 5,
            h: 5,
        };
        let (_, canvas) = upright(12, &label, vec![below]);
        assert_eq!(
            glyph(&canvas, below.x, below.y),
            '┌',
            "{}",
            row(&canvas, below.y)
        );
        let (x, y) = find(&canvas, &first).expect("the first half");
        assert_eq!(find(&canvas, &second), Some((x, y + 1)));
        assert!(y + 1 < below.y, "both rows clear of the box");
    }
}
