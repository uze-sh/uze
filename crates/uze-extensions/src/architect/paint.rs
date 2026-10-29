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
