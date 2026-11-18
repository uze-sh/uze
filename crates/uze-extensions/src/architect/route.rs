//! How an edge gets from one box to another.
//!
//! A shortest-path search over the cell grid itself, rather than channels
//! reserved by the layout: the grid is small, the search is exact about
//! what is already drawn, and it needs no special case for an edge that
//! climbs back up, leaves a boundary or has to go around the outside.
//!
//! What makes the result read as a diagram is in the costs. A bend is
//! dear, so lines run straight; crossing another line is dearer, so they
//! go around when around is near; and running *along* a foreign line is
//! refused outright, because two edges sharing cells are one edge to
//! whoever reads it. The exception is deliberate — edges with the same
//! source, or the same target, may share a trunk and fork, which is how
//! a hand-drawn diagram does it too.
//!
//! A region costs the same way. An edge with an end inside one may cross
//! its wall and stand in it as freely as its own members do; one with
//! neither end inside pays for every cell it spends there, and pays
//! again to run alongside the wall. That is the whole of what makes a
//! boundary legible: a line through it says the two ends have something
//! to do with what it encloses, and until a line has to earn that, most
//! of them say it by accident.

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::shared::canvas::Frame;

use super::{
    layout::Placement,
    model::{Flow, Graph},
};

const STEP: i32 = 2;
const TURN: i32 = 6;
const CROSSING: i32 = 10;
const BOUNDARY: i32 = 3;
/// Hugging a box it has nothing to do with.
const HALO: i32 = 3;
/// Standing inside a region neither end of the edge belongs to. Charged
/// per cell rather than at the wall, so going around a region is cheaper
/// than crossing it however shallowly — which is the whole of what makes
/// a boundary read as one: a line through it says the two ends are
/// related to what it encloses, and most lines are not.
const TRESPASS: i32 = 5;
/// Leaving or arriving by a side that does not face the other end.
const SIDE: i32 = 12;
/// Riding a trunk a mate already drew. Small, so edges still bundle where
/// there is one way through, and enough that they part early — a label
/// can only say whose it is on a stretch that belongs to one edge.
const SHARED: i32 = 1;

/// North, east, south, west — the index is the bit in a cell's sides.
const HEADINGS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

fn opposite(heading: usize) -> usize {
    (heading + 2) % 4
}

fn is_horizontal(heading: usize) -> bool {
    heading % 2 == 1
}

pub struct Route {
    pub edge: usize,
    /// Every cell the line occupies, from beside the source to beside
    /// the target, with the heading it was entered by.
    pub cells: Vec<(i32, i32, usize)>,
}

pub struct Routes {
    pub routes: Vec<Route>,
    pub unrouted: usize,
    width: i32,
    usage: Vec<u8>,
}

impl Routes {
    /// Whether more than one edge runs through this cell — a trunk, where
    /// a label would not say whose it is.
    pub fn is_shared(&self, x: i32, y: i32) -> bool {
        self.usage
            .get((y * self.width + x) as usize)
            .is_some_and(|&count| count > 1)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Boundary {
    None,
    Horizontal,
    Vertical,
}

/// The two ends an edge joins. Two edges are mates when they share one.
type Ends = (usize, usize);

fn mates(a: Ends, b: Ends) -> bool {
    a.0 == b.0 || a.1 == b.1
}

/// What one edge is allowed to do, carried through the search.
#[derive(Clone, Copy)]
struct Passage {
    ends: Ends,
    /// The regions either end stands in, as bits — the only ones this
    /// edge has business inside. Every other one it pays to be in.
    inside: u64,
}

/// The regions `node` stands in, outermost included, as bits.
fn regions_of(graph: &Graph, node: usize) -> u64 {
    let mut mask = 0;
    let mut cluster = graph.nodes[node].cluster;
    while let Some(index) = cluster {
        mask |= region_bit(index);
        cluster = graph.clusters[index].parent;
    }
    mask
}

/// A region's bit, or none once there are more regions than bits: a
/// diagram with sixty-five boundaries is one the extra ones are simply
/// free to cross, which is what the layout did for all of them before.
fn region_bit(cluster: usize) -> u64 {
    match cluster < u64::BITS as usize {
        true => 1 << cluster,
        false => 0,
    }
}

/// `frame` with `by` cells taken off every side.
fn deflated(frame: Frame, by: i32) -> Frame {
    Frame {
        x: frame.x + by,
        y: frame.y + by,
        w: frame.w - by * 2,
        h: frame.h - by * 2,
    }
}

struct Grid {
    width: i32,
    height: i32,
    blocked: Vec<bool>,
    halo: Vec<bool>,
    boundary: Vec<Boundary>,
    /// The regions each cell stands in, as bits.
    inside: Vec<u64>,
    horizontal: Vec<Option<Ends>>,
    vertical: Vec<Option<Ends>>,
}

impl Grid {
    fn new(graph: &Graph, placement: &Placement) -> Self {
        let size = (placement.width * placement.height).max(0) as usize;
        let mut grid = Self {
            width: placement.width,
            height: placement.height,
            blocked: vec![false; size],
            halo: vec![false; size],
            boundary: vec![Boundary::None; size],
            inside: vec![0; size],
            horizontal: vec![None; size],
            vertical: vec![None; size],
        };
        for (index, (frame, cluster)) in placement.clusters.iter().zip(&graph.clusters).enumerate()
        {
            grid.fence(*frame, crate::shared::canvas::text_width(&cluster.title));
            grid.enclose(*frame, region_bit(index));
        }
        for frame in &placement.nodes {
            for y in frame.y - 1..=frame.y + frame.h {
                for x in frame.x - 1..=frame.x + frame.w {
                    if let Some(index) = grid.index(x, y) {
                        grid.halo[index] = true;
                        grid.blocked[index] |= frame.contains(x, y);
                    }
                }
            }
        }
        grid
    }

    /// A boundary can be crossed, square on and away from its corners
    /// and its title — never walked along.
    fn fence(&mut self, frame: Frame, title_width: i32) {
        let (right, bottom) = (frame.x + frame.w - 1, frame.y + frame.h - 1);
        for x in frame.x..=right {
            for y in [frame.y, bottom] {
                if let Some(index) = self.index(x, y) {
                    let corner = x == frame.x || x == right;
                    let titled = y == frame.y && x <= frame.x + title_width + 3;
                    self.boundary[index] = Boundary::Horizontal;
                    self.blocked[index] |= corner || titled;
                }
            }
        }
        for y in frame.y + 1..bottom {
            for x in [frame.x, right] {
                if let Some(index) = self.index(x, y) {
                    self.boundary[index] = Boundary::Vertical;
                }
            }
        }
    }

    /// Marks what a region covers, and puts a halo on both sides of its
    /// wall so a line does not run along it: two rules a cell apart read
    /// as one doubled wall rather than as a boundary and an edge.
    fn enclose(&mut self, frame: Frame, bit: u64) {
        let wall = deflated(frame, 1);
        let clear = deflated(frame, 2);
        for y in frame.y - 1..=frame.y + frame.h {
            for x in frame.x - 1..=frame.x + frame.w {
                let Some(index) = self.index(x, y) else {
                    continue;
                };
                let on_the_wall = frame.contains(x, y) && !wall.contains(x, y);
                if frame.contains(x, y) {
                    self.inside[index] |= bit;
                }
                if !on_the_wall && !clear.contains(x, y) {
                    self.halo[index] = true;
                }
            }
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height)
            .then(|| (y * self.width + x) as usize)
    }

    fn owners(&self, index: usize, heading: usize) -> (Option<Ends>, Option<Ends>) {
        if is_horizontal(heading) {
            (self.horizontal[index], self.vertical[index])
        } else {
            (self.vertical[index], self.horizontal[index])
        }
    }

    /// What entering this cell by `heading` costs, or `None` if it may
    /// not be entered that way at all.
    fn entering(&self, x: i32, y: i32, heading: usize, passage: Passage) -> Option<i32> {
        let index = self.index(x, y)?;
        if self.blocked[index] {
            return None;
        }
        let along = match self.boundary[index] {
            Boundary::Horizontal => is_horizontal(heading),
            Boundary::Vertical => !is_horizontal(heading),
            Boundary::None => false,
        };
        if along {
            return None;
        }
        let (same, across) = self.owners(index, heading);
        if same.is_some_and(|owner| !mates(owner, passage.ends)) {
            return None;
        }
        let mut cost = STEP;
        if same.is_some() {
            cost += SHARED;
        }
        if self.halo[index] {
            cost += HALO;
        }
        if self.boundary[index] != Boundary::None {
            cost += BOUNDARY;
        }
        if across.is_some_and(|owner| !mates(owner, passage.ends)) {
            cost += CROSSING;
        }
        let trespass = self.inside[index] & !passage.inside;
        cost += TRESPASS * trespass.count_ones() as i32;
        Some(cost)
    }

    /// A bend needs a cell of its own: not on a boundary, and not where
    /// a foreign line already runs, since a corner drawn on a crossing
    /// reads as the two lines joining.
    fn may_turn(&self, index: usize, passage: Passage) -> bool {
        let foreign = |owner: Option<Ends>| owner.is_some_and(|owner| !mates(owner, passage.ends));
        self.boundary[index] == Boundary::None
            && !foreign(self.horizontal[index])
            && !foreign(self.vertical[index])
    }

    fn claim(&mut self, index: usize, heading: usize, ends: Ends) {
        let lane = if is_horizontal(heading) {
            &mut self.horizontal[index]
        } else {
            &mut self.vertical[index]
        };
        lane.get_or_insert(ends);
    }
}

pub fn route(graph: &Graph, placement: &Placement) -> Routes {
    let mut grid = Grid::new(graph, placement);
    let mut order: Vec<usize> = (0..graph.edges.len())
        .filter(|&edge| graph.edges[edge].from != graph.edges[edge].to)
        .collect();
    // Short edges first: they have one good path and nothing to go
    // around yet, and a long edge can afford the detour they leave it.
    order.sort_by_key(|&edge| {
        let (ax, ay) = placement.nodes[graph.edges[edge].from].center();
        let (bx, by) = placement.nodes[graph.edges[edge].to].center();
        (ax - bx).abs() + (ay - by).abs()
    });
    let mut routes = Routes {
        routes: Vec::new(),
        unrouted: 0,
        width: placement.width,
        usage: vec![0; grid.blocked.len()],
    };
    let mut search = Search::new(grid.blocked.len() * 4);
    for edge in order {
        let ends = (graph.edges[edge].from, graph.edges[edge].to);
        let passage = Passage {
            ends,
            inside: regions_of(graph, ends.0) | regions_of(graph, ends.1),
        };
        let from = placement.nodes[ends.0];
        let to = placement.nodes[ends.1];
        match shortest(&grid, &mut search, graph.flow, from, to, passage) {
            Some(cells) => {
                for (position, &(x, y, heading)) in cells.iter().enumerate() {
                    let index = grid.index(x, y).expect("a routed cell is on the grid");
                    grid.claim(index, heading, ends);
                    if let Some(&(_, _, leaving)) = cells.get(position + 1) {
                        grid.claim(index, leaving, ends);
                    }
                    routes.usage[index] = routes.usage[index].saturating_add(1);
                }
                routes.routes.push(Route { edge, cells });
            }
            None => routes.unrouted += 1,
        }
    }
    routes.routes.sort_by_key(|route| route.edge);
    routes
}

/// The side of `from` that faces `to`, as a heading.
fn facing(flow: Flow, from: Frame, to: Frame) -> usize {
    let below = to.y >= from.y + from.h;
    let above = to.y + to.h <= from.y;
    let right = to.x >= from.x + from.w;
    let left = to.x + to.w <= from.x;
    let (fx, fy) = from.center();
    let (tx, ty) = to.center();
    match flow {
        Flow::TopDown if below => 2,
        Flow::TopDown if above => 0,
        Flow::TopDown => {
            if tx >= fx {
                1
            } else {
                3
            }
        }
        Flow::LeftRight if right => 1,
        Flow::LeftRight if left => 3,
        Flow::LeftRight => {
            if ty >= fy {
                2
            } else {
                0
            }
        }
    }
}

/// The cells just outside `frame`, one side at a time and never at a
/// corner: `(x, y, side, distance from that side's middle)`.
fn ports(frame: Frame) -> Vec<(i32, i32, usize, i32)> {
    let (cx, cy) = frame.center();
    let mut ports = Vec::new();
    for x in frame.x + 1..frame.x + frame.w - 1 {
        ports.push((x, frame.y - 1, 0, (x - cx).abs()));
        ports.push((x, frame.y + frame.h, 2, (x - cx).abs()));
    }
    for y in frame.y + 1..frame.y + frame.h - 1 {
        ports.push((frame.x + frame.w, y, 1, (y - cy).abs()));
        ports.push((frame.x - 1, y, 3, (y - cy).abs()));
    }
    ports
}

fn side_cost(side: usize, preferred: usize) -> i32 {
    if side == preferred {
        0
    } else if side == opposite(preferred) {
        SIDE * 2
    } else {
        SIDE
    }
}

/// A frontier entry: the estimate first, so the heap pops the most
/// promising state, then the cost so far and the state itself.
type Frontier = Reverse<(i32, i32, i32, i32, usize)>;

/// The search's working state, one slot per cell and heading, kept across
/// every edge of a diagram.
///
/// Four slots a cell, three arrays, one search per edge: allocating them
/// afresh was most of what routing a large board cost. Only the slots a
/// search wrote are put back, because an edge's search reaches a
/// neighbourhood of its two ends and not the whole board.
struct Search {
    best: Vec<i32>,
    came_from: Vec<usize>,
    arrival: Vec<i32>,
    touched: Vec<usize>,
    open: BinaryHeap<Frontier>,
}

impl Search {
    fn new(states: usize) -> Self {
        Self {
            best: vec![i32::MAX; states],
            came_from: vec![usize::MAX; states],
            arrival: vec![-1; states],
            touched: Vec::new(),
            open: BinaryHeap::new(),
        }
    }

    fn reset(&mut self) {
        for &state in &self.touched {
            self.best[state] = i32::MAX;
            self.came_from[state] = usize::MAX;
            self.arrival[state] = -1;
        }
        self.touched.clear();
        self.open.clear();
    }

    fn arrive(&mut self, state: usize, cost: i32) {
        self.arrival[state] = cost;
        self.touched.push(state);
    }

    fn reach(&mut self, state: usize, cost: i32, from: usize) {
        self.best[state] = cost;
        self.came_from[state] = from;
        self.touched.push(state);
    }
}

fn shortest(
    grid: &Grid,
    search: &mut Search,
    flow: Flow,
    from: Frame,
    to: Frame,
    passage: Passage,
) -> Option<Vec<(i32, i32, usize)>> {
    search.reset();
    let state = |x: i32, y: i32, heading: usize| (y * grid.width + x) as usize * 4 + heading;

    let exit = facing(flow, from, to);
    let entry = opposite(facing(flow, from, to));
    for (x, y, side, off_centre) in ports(to) {
        if grid.index(x, y).is_some() {
            // Arriving by a side means travelling *into* it.
            search.arrive(
                state(x, y, opposite(side)),
                side_cost(side, entry) + off_centre,
            );
        }
    }
    let remaining = |x: i32, y: i32| {
        let dx = (to.x - 1 - x).max(x - (to.x + to.w)).max(0);
        let dy = (to.y - 1 - y).max(y - (to.y + to.h)).max(0);
        (dx + dy) * STEP
    };
    for (x, y, side, off_centre) in ports(from) {
        let Some(cost) = grid.entering(x, y, side, passage) else {
            continue;
        };
        let cost = cost + side_cost(side, exit) + off_centre;
        let start = state(x, y, side);
        if cost < search.best[start] {
            search.reach(start, cost, usize::MAX);
            search
                .open
                .push(Reverse((cost + remaining(x, y), cost, x, y, side)));
        }
    }

    let mut finish: Option<(i32, usize)> = None;
    while let Some(Reverse((estimate, cost, x, y, heading))) = search.open.pop() {
        if finish.is_some_and(|(total, _)| estimate >= total) {
            break;
        }
        let here = state(x, y, heading);
        if cost > search.best[here] {
            continue;
        }
        if search.arrival[here] >= 0 {
            let total = cost + search.arrival[here];
            if finish.is_none_or(|(known, _)| total < known) {
                finish = Some((total, here));
            }
        }
        let index = here / 4;
        // The cell beside the source is a stub, never a corner: a line
        // that bends the moment it leaves a box reads as part of its border.
        let may_turn = search.came_from[here] != usize::MAX && grid.may_turn(index, passage);
        for (next, (dx, dy)) in HEADINGS.iter().enumerate() {
            if next == opposite(heading) || (next != heading && !may_turn) {
                continue;
            }
            let (nx, ny) = (x + dx, y + dy);
            let Some(step) = grid.entering(nx, ny, next, passage) else {
                continue;
            };
            let total = cost + step + if next == heading { 0 } else { TURN };
            let there = state(nx, ny, next);
            if total < search.best[there] {
                search.reach(there, total, here);
                search
                    .open
                    .push(Reverse((total + remaining(nx, ny), total, nx, ny, next)));
            }
        }
    }

    let (_, mut at) = finish?;
    let mut cells = Vec::new();
    while at != usize::MAX {
        let index = (at / 4) as i32;
        cells.push((index % grid.width, index / grid.width, at % 4));
        at = search.came_from[at];
    }
    cells.reverse();
    Some(cells)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architect::{layout, mermaid, model::Diagram};

    fn routed(source: &str) -> (Placement, Routes) {
        let Diagram::Graph(graph) = mermaid::parse(source).unwrap() else {
            panic!("expected a graph");
        };
        let placement = layout::place(&graph);
        let routes = route(&graph, &placement);
        (placement, routes)
    }

    #[test]
    fn every_edge_of_a_tangle_is_routed_and_none_enters_a_box() {
        let (placement, routes) = routed(
            "flowchart TD\n a --> b & c & d\n b --> e\n c --> e\n d --> a\n e --> a\n b --> d",
        );
        assert_eq!(routes.unrouted, 0);
        assert_eq!(routes.routes.len(), 8);
        for route in &routes.routes {
            for &(x, y, _) in &route.cells {
                assert!(
                    placement.node_at(x, y).is_none(),
                    "({x},{y}) is inside a box"
                );
            }
        }
    }

    #[test]
    fn a_route_is_one_unbroken_line_that_ends_facing_its_target() {
        let (placement, routes) = routed("flowchart LR\n a --> b\n b --> c\n a --> c");
        for route in &routes.routes {
            for pair in route.cells.windows(2) {
                let ((ax, ay, _), (bx, by, heading)) = (pair[0], pair[1]);
                assert_eq!((bx - ax, by - ay), HEADINGS[heading]);
            }
            let &(x, y, heading) = route.cells.last().unwrap();
            let (dx, dy) = HEADINGS[heading];
            assert!(placement.node_at(x + dx, y + dy).is_some());
        }
    }

    fn graph_of(source: &str) -> Graph {
        let Diagram::Graph(graph) = mermaid::parse(source).unwrap() else {
            panic!("expected a graph");
        };
        graph
    }

    const TANGLE: &str =
        "flowchart TD\n a --> b & c & d\n b --> e\n c --> e\n d --> a\n e --> a\n b --> d";
    const FAN: &str = "flowchart TD\n a --> b\n a --> c\n a --> d\n a --> e\n a --> f";

    #[test]
    fn opposite_headings_face_each_other() {
        assert_eq!(
            [0, 1, 2, 3].map(opposite),
            [2, 3, 0, 1],
            "north, east, south, west"
        );
    }

    #[test]
    fn two_edges_are_mates_when_they_share_a_source_or_a_target() {
        assert!(mates((0, 1), (0, 2)), "one source");
        assert!(mates((0, 1), (2, 1)), "one target");
        assert!(!mates((0, 1), (1, 0)), "one's source is the other's target");
        assert!(!mates((0, 1), (2, 3)));
    }

    #[test]
    fn a_region_has_its_own_bit_until_the_bits_run_out() {
        assert_eq!(region_bit(0), 1);
        assert_eq!(region_bit(3), 8);
        assert_eq!(region_bit(63), 1 << 63);
        assert_eq!(region_bit(64), 0);
        assert_eq!(region_bit(65), 0);
    }

    #[test]
    fn a_box_stands_in_its_region_and_every_region_around_it() {
        let graph = graph_of(
            "flowchart TD\n subgraph outer [Outer]\n subgraph inner [Inner]\n a\n end\n b\n end\n c",
        );
        let id = |name: &str| graph.nodes.iter().position(|node| node.id == name).unwrap();
        let cluster = |name: &str| graph.clusters.iter().position(|c| c.id == name).unwrap();
        let (outer, inner) = (region_bit(cluster("outer")), region_bit(cluster("inner")));
        assert_eq!(regions_of(&graph, id("a")), outer | inner);
        assert_eq!(regions_of(&graph, id("b")), outer);
        assert_eq!(regions_of(&graph, id("c")), 0);
    }

    #[test]
    fn a_deflated_frame_loses_as_much_on_every_side() {
        let frame = Frame {
            x: 10,
            y: 20,
            w: 8,
            h: 6,
        };
        assert_eq!(
            deflated(frame, 2),
            Frame {
                x: 12,
                y: 22,
                w: 4,
                h: 2
            }
        );
    }

    #[test]
    fn a_box_faces_another_by_the_side_nearest_it_in_the_drawings_flow() {
        let from = Frame {
            x: 10,
            y: 10,
            w: 6,
            h: 4,
        };
        let at = |x: i32, y: i32| Frame { x, y, w: 6, h: 4 };
        let (north, east, south, west) = (0, 1, 2, 3);
        // Top down: a box wholly below or above is faced vertically, even
        // when far to one side; one beside it, horizontally.
        assert_eq!(facing(Flow::TopDown, from, at(40, 14)), south, "just below");
        assert_eq!(facing(Flow::TopDown, from, at(-40, 6)), north, "just above");
        assert_eq!(facing(Flow::TopDown, from, at(30, 11)), east);
        assert_eq!(facing(Flow::TopDown, from, at(-10, 9)), west);
        assert_eq!(facing(Flow::TopDown, from, at(10, 12)), east, "level");
        assert_eq!(facing(Flow::TopDown, from, at(16, 13)), east, "overlapping");
        // Left to right: the same, turned.
        assert_eq!(
            facing(Flow::LeftRight, from, at(16, 40)),
            east,
            "just right"
        );
        assert_eq!(facing(Flow::LeftRight, from, at(4, -40)), west, "just left");
        assert_eq!(facing(Flow::LeftRight, from, at(11, 30)), south);
        assert_eq!(facing(Flow::LeftRight, from, at(9, -10)), north);
        assert_eq!(facing(Flow::LeftRight, from, at(12, 10)), south, "level");
        assert_eq!(
            facing(Flow::LeftRight, from, at(15, 10)),
            south,
            "overlapping"
        );
    }

    #[test]
    fn the_ports_of_a_box_are_the_cells_beside_its_sides_short_of_the_corners() {
        let frame = Frame {
            x: 10,
            y: 20,
            w: 5,
            h: 4,
        };
        let mut found = ports(frame);
        found.sort();
        let mut wanted = vec![
            (11, 19, 0, 1),
            (12, 19, 0, 0),
            (13, 19, 0, 1),
            (11, 24, 2, 1),
            (12, 24, 2, 0),
            (13, 24, 2, 1),
            (15, 21, 1, 1),
            (15, 22, 1, 0),
            (9, 21, 3, 1),
            (9, 22, 3, 0),
        ];
        wanted.sort();
        assert_eq!(found, wanted);
    }

    #[test]
    fn a_side_facing_the_other_end_is_cheapest_and_the_far_side_dearest() {
        for preferred in 0..4 {
            let beside = (preferred + 1) % 4;
            assert_eq!(side_cost(preferred, preferred), 0);
            assert!(side_cost(beside, preferred) > 0);
            assert!(side_cost(opposite(preferred), preferred) > side_cost(beside, preferred));
            assert_eq!(
                side_cost(opposite(beside), preferred),
                side_cost(beside, preferred)
            );
        }
    }

    #[test]
    fn a_cell_is_shared_exactly_where_more_than_one_line_runs() {
        let (placement, routes) = routed(FAN);
        let mut shared = 0;
        for y in 0..placement.height {
            for x in 0..placement.width {
                let running = routes
                    .routes
                    .iter()
                    .filter(|route| route.cells.iter().any(|&(cx, cy, _)| (cx, cy) == (x, y)))
                    .count();
                assert_eq!(routes.is_shared(x, y), running > 1, "({x},{y})");
                shared += usize::from(running > 1);
            }
        }
        assert!(shared > 0, "a fan out of one box shares a trunk");
        assert!(!routes.is_shared(0, placement.height + 5), "off the board");
    }

    /// Whether the line leaves and enters this cell along one axis.
    fn straight_through(route: &Route, position: usize) -> Option<bool> {
        let (_, _, entered) = route.cells[position];
        let left = route
            .cells
            .get(position + 1)
            .map_or(entered, |&(_, _, next)| next);
        (entered == left).then(|| is_horizontal(entered))
    }

    #[test]
    fn lines_of_unrelated_edges_only_ever_cross_square_on() {
        for source in [
            TANGLE,
            FAN,
            "flowchart LR\n a --> b & c\n d --> b\n c --> d\n e --> a",
        ] {
            let graph = graph_of(source);
            let (_, routes) = routed(source);
            for one in &routes.routes {
                for other in &routes.routes {
                    let ends = |route: &Route| {
                        let edge = &graph.edges[route.edge];
                        (edge.from, edge.to)
                    };
                    if mates(ends(one), ends(other)) {
                        continue;
                    }
                    for (a, &(x, y, _)) in one.cells.iter().enumerate() {
                        let Some(b) = other
                            .cells
                            .iter()
                            .position(|&(ox, oy, _)| (ox, oy) == (x, y))
                        else {
                            continue;
                        };
                        let (along_one, along_other) =
                            (straight_through(one, a), straight_through(other, b));
                        assert!(
                            along_one.is_some() && along_other.is_some(),
                            "{source}: no bend on a crossing at ({x},{y})"
                        );
                        assert_ne!(
                            along_one, along_other,
                            "{source}: ({x},{y}) is ridden along"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_line_leaves_its_box_straight_before_it_bends() {
        for source in [TANGLE, FAN, "flowchart LR\n a --> b & c & d\n d --> a"] {
            let graph = graph_of(source);
            let (placement, routes) = routed(source);
            for route in &routes.routes {
                let (x, y, heading) = route.cells[0];
                let (dx, dy) = HEADINGS[heading];
                let source_box = placement.nodes[graph.edges[route.edge].from];
                assert!(
                    source_box.contains(x - dx, y - dy),
                    "{source}: it starts beside its box"
                );
                if let Some(&(_, _, next)) = route.cells.get(1) {
                    assert_eq!(next, heading, "{source}: a stub first");
                }
            }
        }
    }

    #[test]
    fn a_line_with_a_clear_way_runs_straight_from_the_middle_of_the_facing_side() {
        for source in [
            "flowchart TD\n a[Alpha] --> b[Alpha]",
            "flowchart LR\n a[Alpha] --> b[Alpha]",
        ] {
            let (placement, routes) = routed(source);
            let route = &routes.routes[0];
            let (from, to) = (placement.nodes[0], placement.nodes[1]);
            let heading = route.cells[0].2;
            assert!(
                route.cells.iter().all(|&(_, _, h)| h == heading),
                "{source}: no bend"
            );
            let (x, y, _) = route.cells[0];
            if source.contains("TD") {
                assert_eq!(heading, 2, "{source}: down");
                assert_eq!(x, from.center().0, "{source}");
                assert_eq!(
                    route.cells.len() as i32,
                    to.y - (from.y + from.h),
                    "{source}"
                );
            } else {
                assert_eq!(heading, 1, "{source}: across");
                assert_eq!(y, from.center().1, "{source}");
                assert_eq!(
                    route.cells.len() as i32,
                    to.x - (from.x + from.w),
                    "{source}"
                );
            }
        }
    }

    /// The number of times a line changes heading.
    fn bends(route: &Route) -> usize {
        route
            .cells
            .windows(2)
            .filter(|pair| pair[0].2 != pair[1].2)
            .count()
    }

    #[test]
    fn a_line_to_a_box_off_to_one_side_turns_no_more_than_it_must() {
        for source in [
            "flowchart TD\n a[A rather wide box] --> b & c",
            "flowchart LR\n a --> b & c",
        ] {
            let (_, routes) = routed(source);
            for route in &routes.routes {
                assert!(bends(route) <= 2, "{source}: {:?}", route.cells);
                let (x, y, _) = route.cells[0];
                let &(lx, ly, _) = route.cells.last().unwrap();
                assert_eq!(
                    route.cells.len() as i32 - 1,
                    (lx - x).abs() + (ly - y).abs(),
                    "{source}: never doubles back"
                );
            }
        }
    }

    #[test]
    fn a_line_climbing_back_up_leaves_by_the_top_and_arrives_from_below() {
        let source = "flowchart TD\n a --> b\n b --> c\n c --> a";
        let graph = graph_of(source);
        let (placement, routes) = routed(source);
        let back = routes
            .routes
            .iter()
            .find(|route| graph.nodes[graph.edges[route.edge].from].id == "c")
            .unwrap();
        let (from, to) = (placement.nodes[2], placement.nodes[0]);
        assert!(to.y < from.y, "the target is above");
        let first = back.cells[0];
        assert!(
            first.1 < from.y || first.2 == 1 || first.2 == 3,
            "it leaves by the top or a side, never the bottom: {first:?}"
        );
        assert_eq!(routes.unrouted, 0);
    }

    fn region_grid(source: &str) -> (Graph, Placement, Grid, Frame) {
        let graph = graph_of(source);
        let placement = layout::place(&graph);
        let grid = Grid::new(&graph, &placement);
        let wall = placement.clusters[0];
        (graph, placement, grid, wall)
    }

    const OUTSIDE: Passage = Passage {
        ends: (100, 101),
        inside: 0,
    };

    #[test]
    fn a_regions_wall_is_crossed_square_on_away_from_its_corners_and_title() {
        let (graph, _, grid, wall) = region_grid(
            "flowchart TD\n subgraph r [Region]\n a[A box wide enough for room] --> b\n end",
        );
        let (north, east, south, west) = (0, 1, 2, 3);
        let (right, bottom) = (wall.x + wall.w - 1, wall.y + wall.h - 1);
        for (x, y) in [
            (wall.x, wall.y),
            (right, wall.y),
            (wall.x, bottom),
            (right, bottom),
        ] {
            for heading in 0..4 {
                assert_eq!(
                    grid.entering(x, y, heading, OUTSIDE),
                    None,
                    "corner ({x},{y})"
                );
            }
        }
        let title = crate::shared::canvas::text_width(&graph.clusters[0].title);
        let last_of_title = wall.x + title + 3;
        assert_eq!(
            grid.entering(last_of_title, wall.y, south, OUTSIDE),
            None,
            "on the title"
        );
        assert_eq!(
            grid.entering(wall.x + 1, wall.y, south, OUTSIDE),
            None,
            "before the title"
        );
        let past_title = last_of_title + 1;
        assert!(past_title < right);
        assert!(grid.entering(past_title, wall.y, south, OUTSIDE).is_some());
        assert!(grid.entering(past_title, wall.y, north, OUTSIDE).is_some());
        assert_eq!(
            grid.entering(past_title, wall.y, east, OUTSIDE),
            None,
            "along the top"
        );
        let middle = wall.x + wall.w / 2;
        assert!(grid.entering(middle, bottom, south, OUTSIDE).is_some());
        assert_eq!(
            grid.entering(middle, bottom, west, OUTSIDE),
            None,
            "along the bottom"
        );
        let level = wall.y + wall.h / 2;
        for x in [wall.x, right] {
            assert!(
                grid.entering(x, level, east, OUTSIDE).is_some(),
                "across a side"
            );
            assert!(
                grid.entering(x, level, west, OUTSIDE).is_some(),
                "across a side"
            );
            assert_eq!(
                grid.entering(x, level, south, OUTSIDE),
                None,
                "along a side"
            );
            assert_eq!(
                grid.entering(x, level, north, OUTSIDE),
                None,
                "along a side"
            );
        }
    }

    /// A cell clear of every box, region and wall.
    fn open_ground(grid: &Grid) -> (i32, i32) {
        (0..grid.width)
            .flat_map(|x| (0..grid.height).map(move |y| (x, y)))
            .find(|&(x, y)| {
                let index = grid.index(x, y).unwrap();
                !grid.halo[index]
                    && !grid.blocked[index]
                    && grid.inside[index] == 0
                    && grid.boundary[index] == Boundary::None
            })
            .expect("open ground")
    }

    #[test]
    fn crossing_a_wall_costs_more_than_open_ground() {
        let (_, _, grid, wall) = region_grid(
            "flowchart TD\n subgraph r [Region]\n a[A box wide enough for room] --> b\n end\n c",
        );
        let (x, y) = open_ground(&grid);
        let open = grid.entering(x, y, 1, OUTSIDE).unwrap();
        let across = grid
            .entering(wall.x, wall.y + wall.h / 2, 1, OUTSIDE)
            .unwrap();
        assert!(across > open, "{across} > {open}");
    }

    #[test]
    fn standing_in_a_region_costs_only_a_line_with_no_end_there() {
        let (graph, placement, grid, wall) = region_grid(
            "flowchart TD\n subgraph r [Region]\n a[A box wide enough for room] --> b\n end",
        );
        let a = graph.nodes.iter().position(|node| node.id == "a").unwrap();
        let member = Passage {
            ends: (100, 101),
            inside: regions_of(&graph, a),
        };
        let box_a = placement.nodes[a];
        // Clear of the boxes and of the wall's halo.
        let (x, y) = (wall.x + 3, box_a.y + box_a.h + 2);
        assert!(placement.node_at(x, y).is_none());
        let stranger = grid.entering(x, y, 2, OUTSIDE).unwrap();
        let belongs = grid.entering(x, y, 2, member).unwrap();
        assert!(stranger > belongs, "{stranger} > {belongs}");
        let outside = grid.entering(wall.x + wall.w + 4, wall.y + 3, 2, OUTSIDE);
        if let Some(outside) = outside {
            assert_eq!(
                outside, belongs,
                "a member stands in its region as on open ground"
            );
        }
    }

    #[test]
    fn hugging_a_wall_from_either_side_costs_more_than_standing_clear_of_it() {
        let (_, placement, grid, wall) = region_grid(
            "flowchart TD\n subgraph r [Region]\n a[A box wide enough for room] --> b\n end\n c[A box wide enough for room]",
        );
        let level = wall.y + wall.h / 2;
        let member = Passage {
            ends: (100, 101),
            inside: u64::MAX,
        };
        let just_inside = grid.entering(wall.x + 1, level, 2, member).unwrap();
        let clear_inside = grid.entering(wall.x + 2, level, 2, member).unwrap();
        assert!(just_inside > clear_inside, "{just_inside} > {clear_inside}");
        let (x, y) = (wall.x - 1, level);
        if x >= 0 && placement.node_at(x, y).is_none() {
            let just_outside = grid.entering(x, y, 2, OUTSIDE).unwrap();
            assert!(just_outside > clear_inside);
        }
    }

    #[test]
    fn a_lane_is_its_owners_and_their_mates_and_crossing_another_costs() {
        let (_, _, mut grid, _) = region_grid(
            "flowchart TD\n subgraph r [Region]\n x\n end\n a[A box wide enough for room] --> b",
        );
        let (x, y) = open_ground(&grid);
        let index = grid.index(x, y).unwrap();
        let owner = Passage {
            ends: (0, 1),
            inside: 0,
        };
        let fresh = grid.entering(x, y, 1, owner).unwrap();
        grid.claim(index, 1, (0, 1));
        let mate = Passage {
            ends: (0, 7),
            inside: 0,
        };
        let stranger = Passage {
            ends: (5, 6),
            inside: 0,
        };
        assert!(
            grid.entering(x, y, 1, mate).unwrap() > fresh,
            "riding a trunk"
        );
        assert!(
            grid.entering(x, y, 3, mate).is_some(),
            "either way along it"
        );
        assert_eq!(
            grid.entering(x, y, 1, stranger),
            None,
            "never along a stranger's"
        );
        let crossing = grid.entering(x, y, 0, stranger).unwrap();
        assert!(crossing > fresh, "crossing costs");
        assert_eq!(
            grid.entering(x, y, 0, mate),
            Some(fresh),
            "a mate crosses for free"
        );
        assert!(
            !grid.may_turn(index, stranger),
            "no bend on a stranger's line"
        );
        assert!(grid.may_turn(index, mate), "a mate may fork there");
    }

    #[test]
    fn no_bend_stands_on_a_wall() {
        let (_, _, grid, wall) = region_grid(
            "flowchart TD\n subgraph r [Region]\n a[A box wide enough for room] --> b\n end",
        );
        let level = wall.y + wall.h / 2;
        let on_the_wall = grid.index(wall.x, level).unwrap();
        let inside = grid.index(wall.x + 2, level).unwrap();
        assert!(!grid.may_turn(on_the_wall, OUTSIDE));
        assert!(grid.may_turn(inside, OUTSIDE));
    }
}
