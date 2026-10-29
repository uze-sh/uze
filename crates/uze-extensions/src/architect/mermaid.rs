//! Mermaid source, read into a [`Diagram`].
//!
//! The part of the language an architecture document actually uses:
//! flowcharts with subgraphs, the C4 views, and sequence diagrams. Styling
//! statements are skipped rather than refused — what a node looks like is
//! the host's palette to decide, and a diagram that sets colours should
//! still be read for what it says.
//!
//! Written here rather than depended on: Mermaid's own parser is
//! JavaScript bound to a browser DOM, and nothing published for Rust
//! clears this workspace's bar for a new dependency.

use super::model::{
    Abstraction, Boundary, Cluster, Diagram, Edge, Flow, Graph, Participant, Sequence,
    SequenceStep, Shape, Stroke, View,
};

pub fn parse(source: &str) -> Result<Diagram, String> {
    let mut lines = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("%%"));
    let header = lines.next().ok_or("the diagram is empty")?;
    let keyword = header.split_whitespace().next().unwrap_or_default();
    match keyword {
        "flowchart" | "graph" => Ok(Diagram::Graph(flowchart(header, lines)?)),
        "sequenceDiagram" => Ok(Diagram::Sequence(sequence(lines))),
        _ if keyword.starts_with("C4") => Ok(Diagram::Graph(c4(View::of(keyword), lines)?)),
        other => Err(format!("`{other}` diagrams are not drawn yet")),
    }
}

fn flowchart<'a>(header: &str, lines: impl Iterator<Item = &'a str>) -> Result<Graph, String> {
    let direction = header
        .split_whitespace()
        .nth(1)
        .map(|direction| direction.trim_end_matches(';'));
    let flow = match direction {
        Some("LR" | "RL") => Flow::LeftRight,
        _ => Flow::TopDown,
    };
    let mut graph = Graph::new(flow);
    let mut open: Vec<usize> = Vec::new();
    for line in lines {
        let keyword = line.split_whitespace().next().unwrap_or_default();
        match keyword {
            "subgraph" => {
                let declared = line["subgraph".len()..].trim();
                graph.clusters.push(Cluster {
                    id: declared
                        .split(|c: char| c == '[' || c.is_whitespace())
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    title: subgraph_title(declared),
                    parent: open.last().copied(),
                    boundary: None,
                });
                open.push(graph.clusters.len() - 1);
            }
            "end" => {
                open.pop();
            }
            "click" => link_clicked(&mut graph, line),
            "classDef" | "class" | "style" | "linkStyle" | "direction" => {}
            _ => Statement::new(line).read_into(&mut graph, open.last().copied())?,
        }
    }
    Ok(graph)
}

/// `click id href "path"`, or the older `click id "path"`: the one
/// flowchart statement that says where a node leads.
fn link_clicked(graph: &mut Graph, line: &str) {
    let mut words = line.split_whitespace().skip(1);
    let Some(id) = words.next() else {
        return;
    };
    let target = line
        .split_once('"')
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(target, _)| target.trim());
    if let (Some(target), Some(node)) = (
        target.filter(|target| !target.is_empty()),
        graph.nodes.iter_mut().find(|node| node.id == id),
    ) {
        node.link = Some(target.to_owned());
    }
}

fn subgraph_title(rest: &str) -> String {
    let title = match rest.split_once('[') {
        Some((_, bracketed)) => bracketed.trim_end_matches(']'),
        None => rest,
    };
    unquoted(title)
}

fn unquoted(text: &str) -> String {
    text.trim().trim_matches('"').trim().to_owned()
}

fn with_breaks(text: &str) -> String {
    text.replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
}

/// One flowchart statement: node groups joined by links, `A & B --> C`.
struct Statement {
    chars: Vec<char>,
    at: usize,
}

struct Link {
    stroke: Stroke,
    arrow: bool,
    label: Option<String>,
}

const SHAPES: [(&str, &str, Shape); 8] = [
    ("([", "])", Shape::Round),
    ("[(", ")]", Shape::Database),
    ("((", "))", Shape::Round),
    ("{{", "}}", Shape::Decision),
    ("[", "]", Shape::Box),
    ("(", ")", Shape::Round),
    ("{", "}", Shape::Decision),
    (">", "]", Shape::Box),
];

impl Statement {
    fn new(line: &str) -> Self {
        Self {
            chars: line.trim_end_matches(';').chars().collect(),
            at: 0,
        }
    }

    fn read_into(mut self, graph: &mut Graph, cluster: Option<usize>) -> Result<(), String> {
        let mut sources = self.group(graph, cluster)?;
        loop {
            self.skip_spaces();
            if self.at >= self.chars.len() {
                return Ok(());
            }
            let link = self.link()?;
            let targets = self.group(graph, cluster)?;
            for &from in &sources {
                for &to in &targets {
                    graph.edges.push(Edge {
                        from,
                        to,
                        label: link.label.clone(),
                        description: None,
                        stroke: link.stroke,
                        arrow: link.arrow,
                        arrow_back: false,
                    });
                }
            }
            sources = targets;
        }
    }

    fn group(&mut self, graph: &mut Graph, cluster: Option<usize>) -> Result<Vec<usize>, String> {
        let mut nodes = vec![self.node(graph, cluster)?];
        loop {
            self.skip_spaces();
            if self.peek() != Some('&') {
                return Ok(nodes);
            }
            self.at += 1;
            nodes.push(self.node(graph, cluster)?);
        }
    }

    fn node(&mut self, graph: &mut Graph, cluster: Option<usize>) -> Result<usize, String> {
        self.skip_spaces();
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            self.at += 1;
        }
        if start == self.at {
            return Err(format!("expected a node at `{}`", self.rest()));
        }
        let id: String = self.chars[start..self.at].iter().collect();
        let index = graph.node_named(&id, cluster);
        for (opening, closing, shape) in SHAPES {
            if !self.rest().starts_with(opening) {
                continue;
            }
            self.at += opening.chars().count();
            let text = self.until(closing)?;
            let text = with_breaks(&unquoted(&text));
            let (title, description) = match text.split_once('\n') {
                Some((title, rest)) => (title.to_owned(), Some(rest.replace('\n', " "))),
                None => (text, None),
            };
            let node = &mut graph.nodes[index];
            node.title = title;
            node.description = description;
            node.shape = shape;
            // Declaring a node inside a subgraph is what places it there,
            // wherever an earlier edge first mentioned it.
            node.cluster = cluster.or(node.cluster);
            break;
        }
        Ok(index)
    }

    fn link(&mut self) -> Result<Link, String> {
        let opening = self.run_of("-=.<>");
        if opening.is_empty() {
            return Err(format!("expected a link at `{}`", self.rest()));
        }
        let mut run = opening.clone();
        let mut label = None;
        if opening.chars().count() <= 2 {
            let start = self.at;
            while self.at < self.chars.len() && !self.closes_inline_label() {
                self.at += 1;
            }
            label = Some(unquoted(
                &self.chars[start..self.at].iter().collect::<String>(),
            ));
            run.push_str(&self.run_of("-=.<>"));
        }
        self.skip_spaces();
        if self.peek() == Some('|') {
            self.at += 1;
            label = Some(unquoted(&self.until("|")?));
        }
        Ok(Link {
            stroke: if run.contains('=') {
                Stroke::Thick
            } else if run.contains('.') {
                Stroke::Dotted
            } else {
                Stroke::Solid
            },
            arrow: run.ends_with('>'),
            label: label.filter(|text| !text.is_empty()),
        })
    }

    fn closes_inline_label(&self) -> bool {
        let here = self.chars[self.at];
        let next = self.chars.get(self.at + 1).copied().unwrap_or(' ');
        "-=.".contains(here) && "-=.>".contains(next)
    }

    fn run_of(&mut self, allowed: &str) -> String {
        let start = self.at;
        while self.peek().is_some_and(|c| allowed.contains(c)) {
            self.at += 1;
        }
        self.chars[start..self.at].iter().collect()
    }

    fn until(&mut self, closing: &str) -> Result<String, String> {
        let rest = self.rest();
        let end = rest
            .find(closing)
            .ok_or_else(|| format!("`{closing}` never closes in `{rest}`"))?;
        let text = rest[..end].to_owned();
        self.at += rest[..end].chars().count() + closing.chars().count();
        Ok(text)
    }

    fn rest(&self) -> String {
        self.chars[self.at.min(self.chars.len())..].iter().collect()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }
}

fn c4<'a>(view: Option<View>, lines: impl Iterator<Item = &'a str>) -> Result<Graph, String> {
    let mut graph = Graph::new(Flow::TopDown);
    graph.view = view;
    let mut open: Vec<usize> = Vec::new();
    // Mermaid numbers a dynamic view's relations in the order they are
    // written, whatever index a `RelIndex` gives: the number drawn is the
    // one the reader can count to.
    let mut steps = 0;
    for line in lines {
        if line.starts_with('}') {
            open.pop();
            continue;
        }
        let Some((name, rest)) = line.split_once('(') else {
            continue;
        };
        let name = name.trim();
        let inside = rest.rsplit_once(')').map_or(rest, |(inside, _)| inside);
        let Some(signature) = Signature::of(name) else {
            continue;
        };
        let arguments = Arguments::read(inside, signature.parameters);
        let is_node = name == "Node" || name.starts_with("Node_") || name == "Deployment_Node";
        match signature.form {
            Form::Boundary => {
                let boundary = match name {
                    "System_Boundary" => Boundary::System,
                    "Container_Boundary" => Boundary::Container,
                    "Enterprise_Boundary" => Boundary::Enterprise,
                    _ if is_node => Boundary::Deployment,
                    _ => Boundary::Typed(arguments.get("type")),
                };
                let kind = match &boundary {
                    Boundary::System => Some("Software System".to_owned()),
                    Boundary::Container => Some("Container".to_owned()),
                    Boundary::Enterprise => Some("Enterprise".to_owned()),
                    Boundary::Typed(kind) => kind.clone(),
                    Boundary::Deployment => arguments.get("type"),
                };
                let alias = arguments.get("alias").unwrap_or_default();
                let label = arguments.get("label").unwrap_or_else(|| alias.clone());
                graph.clusters.push(Cluster {
                    id: alias,
                    title: match kind {
                        Some(kind) => format!("{label} [{kind}]"),
                        None => label,
                    },
                    parent: open.last().copied(),
                    boundary: Some(boundary),
                });
                open.push(graph.clusters.len() - 1);
            }
            Form::Relation => {
                let (Some(first), Some(second)) = (arguments.get("from"), arguments.get("to"))
                else {
                    return Err(format!("`{line}` names no two ends"));
                };
                // `Rel_Back(a, b)` is C4-PlantUML's `a <- b`: the relation
                // runs from its second end to its first.
                let (from, to) = match name.starts_with("Rel_Back") {
                    true => (second, first),
                    false => (first, second),
                };
                let from = graph.node_named(&from, None);
                let to = graph.node_named(&to, None);
                let said = match (arguments.get("label"), arguments.get("techn")) {
                    (Some(label), Some(technology)) => Some(format!("{label} [{technology}]")),
                    (label, _) => label,
                };
                let label = match view == Some(View::Dynamic) {
                    true => {
                        steps += 1;
                        Some(match said {
                            Some(said) => format!("{steps}. {said}"),
                            None => format!("{steps}."),
                        })
                    }
                    false => said,
                };
                graph.edges.push(Edge {
                    from,
                    to,
                    label,
                    description: arguments.get("descr"),
                    stroke: Stroke::Solid,
                    arrow: true,
                    arrow_back: name.starts_with("BiRel"),
                });
            }
            Form::Element => {
                let Some(element) = C4Element::named(name) else {
                    continue;
                };
                let Some(alias) = arguments.get("alias") else {
                    return Err(format!("`{line}` has no alias"));
                };
                let index = graph.node_named(&alias, open.last().copied());
                let external = name.ends_with("_Ext");
                let kind = match external {
                    true => format!("External {}", element.kind),
                    false => element.kind.to_owned(),
                };
                let node = &mut graph.nodes[index];
                node.title = arguments.get("label").unwrap_or(alias);
                node.kind = Some(match arguments.get("techn") {
                    Some(technology) => format!("{kind}: {technology}"),
                    None => kind,
                });
                node.description = arguments.get("descr");
                node.shape = element.shape;
                node.external = external;
                node.cluster = open.last().copied();
                node.link = arguments.get("link");
                node.abstraction = Some(element.abstraction);
            }
        }
    }
    Ok(graph)
}

/// The parameters of each C4 macro, in Mermaid's order. Named here once so
/// a `$techn=` lands where the fourth positional argument would.
const ELEMENT: &[&str] = &["alias", "label", "descr", "sprite", "tags", "link"];
const TECHNICAL_ELEMENT: &[&str] = &["alias", "label", "techn", "descr", "sprite", "tags", "link"];
const BOUNDARY: &[&str] = &["alias", "label", "tags", "link"];
const TYPED_BOUNDARY: &[&str] = &["alias", "label", "type", "tags", "link"];
const DEPLOYMENT_NODE: &[&str] = &["alias", "label", "type", "descr", "sprite", "tags", "link"];
const RELATION: &[&str] = &[
    "from", "to", "label", "techn", "descr", "sprite", "tags", "link",
];
const INDEXED_RELATION: &[&str] = &[
    "index", "from", "to", "label", "techn", "descr", "sprite", "tags", "link",
];

/// What a macro declares, which decides what is read from it.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Form {
    Element,
    Boundary,
    Relation,
}

struct Signature {
    form: Form,
    parameters: &'static [&'static str],
}

impl Signature {
    /// The signature of a macro this surface reads; `None` for one it
    /// does not, which is skipped — styling among them.
    fn of(name: &str) -> Option<Self> {
        let (form, parameters) = match name {
            "Boundary" => (Form::Boundary, TYPED_BOUNDARY),
            _ if name.ends_with("_Boundary") => (Form::Boundary, BOUNDARY),
            "Node" | "Deployment_Node" => (Form::Boundary, DEPLOYMENT_NODE),
            _ if name.starts_with("Node_") => (Form::Boundary, DEPLOYMENT_NODE),
            _ if name.starts_with("RelIndex") => (Form::Relation, INDEXED_RELATION),
            _ if name.starts_with("Rel") || name.starts_with("BiRel") => (Form::Relation, RELATION),
            _ => match C4Element::named(name)?.abstraction {
                Abstraction::Person | Abstraction::System => (Form::Element, ELEMENT),
                Abstraction::Container | Abstraction::Component => {
                    (Form::Element, TECHNICAL_ELEMENT)
                }
            },
        };
        Some(Self { form, parameters })
    }
}

/// A macro's arguments, matched to its parameters: positionally, then by
/// `$name=`, which Mermaid accepts for any of them.
struct Arguments {
    values: Vec<(&'static str, String)>,
}

impl Arguments {
    fn read(inside: &str, parameters: &'static [&'static str]) -> Self {
        let mut values = Vec::new();
        let mut position = 0;
        for argument in split_arguments(inside) {
            match argument.strip_prefix('$') {
                Some(named) => {
                    let (key, value) = named.split_once('=').unwrap_or((named, ""));
                    if let Some(&key) = parameters.iter().find(|&&known| known == key.trim()) {
                        values.push((key, value.trim().to_owned()));
                    }
                }
                None => {
                    if let Some(&key) = parameters.get(position) {
                        values.push((key, argument));
                    }
                    position += 1;
                }
            }
        }
        Self { values }
    }

    /// The value given for `key`; an empty one is no value.
    fn get(&self, key: &str) -> Option<String> {
        self.values
            .iter()
            .rev()
            .find(|(known, value)| *known == key && !value.is_empty())
            .map(|(_, value)| value.clone())
    }
}

struct C4Element {
    kind: &'static str,
    abstraction: Abstraction,
    shape: Shape,
}

impl C4Element {
    fn named(name: &str) -> Option<Self> {
        let base = name.strip_suffix("_Ext").unwrap_or(name);
        let (kind, abstraction, stem) = if base.starts_with("Person") {
            ("Person", Abstraction::Person, "Person")
        } else if base.starts_with("System") {
            ("Software System", Abstraction::System, "System")
        } else if base.starts_with("Container") {
            ("Container", Abstraction::Container, "Container")
        } else if base.starts_with("Component") {
            ("Component", Abstraction::Component, "Component")
        } else {
            return None;
        };
        let shape = match &base[stem.len()..] {
            "" if abstraction == Abstraction::Person => Shape::Person,
            "" => Shape::Box,
            "Db" => Shape::Database,
            "Queue" => Shape::Queue,
            _ => return None,
        };
        Some(Self {
            kind,
            abstraction,
            shape,
        })
    }
}

/// A macro's arguments as written: comma-separated, quotes optional, and
/// a comma inside quotes is text.
fn split_arguments(inside: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in inside.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => arguments.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    arguments.push(current);
    arguments
        .into_iter()
        .map(|argument| argument.trim().to_owned())
        .collect()
}

fn sequence<'a>(lines: impl Iterator<Item = &'a str>) -> Sequence {
    let mut sequence = Sequence::default();
    for line in lines {
        let (keyword, rest) = line.split_once(' ').unwrap_or((line, ""));
        match keyword {
            "participant" | "actor" => {
                let (id, title) = rest.split_once(" as ").unwrap_or((rest, rest));
                let index = participant(&mut sequence, id.trim());
                sequence.participants[index].title = title.trim().to_owned();
            }
            "Note" | "note" => {
                if let Some((place, text)) = rest.split_once(':') {
                    let first = place
                        .trim_start_matches("over")
                        .trim_start_matches("right of")
                        .trim_start_matches("left of")
                        .split(',')
                        .next()
                        .unwrap_or_default()
                        .trim();
                    let over = participant(&mut sequence, first);
                    sequence.steps.push(SequenceStep::Note {
                        over,
                        text: with_breaks(text.trim()).replace('\n', " "),
                    });
                }
            }
            "loop" | "alt" | "opt" | "par" | "else" | "and" | "critical" | "break" => {
                sequence
                    .steps
                    .push(SequenceStep::Divider(line.trim().to_owned()));
            }
            "end" => sequence.steps.push(SequenceStep::Divider("end".to_owned())),
            _ => message(&mut sequence, line),
        }
    }
    sequence
}

fn message(sequence: &mut Sequence, line: &str) {
    let Some((ends, text)) = line.split_once(':') else {
        return;
    };
    let Some(arrow_at) = ends.find('-') else {
        return;
    };
    let from = ends[..arrow_at].trim();
    let arrow_len = ends[arrow_at..]
        .find(|c: char| !"->x)".contains(c))
        .unwrap_or(ends.len() - arrow_at);
    let arrow = &ends[arrow_at..arrow_at + arrow_len];
    let to = ends[arrow_at + arrow_len..].trim_matches(|c: char| c == '+' || c == '-' || c == ' ');
    if from.is_empty() || to.is_empty() {
        return;
    }
    let from = participant(sequence, from);
    let to = participant(sequence, to);
    sequence.steps.push(SequenceStep::Message {
        from,
        to,
        text: text.trim().to_owned(),
        stroke: if arrow.starts_with("--") {
            Stroke::Dotted
        } else {
            Stroke::Solid
        },
    });
}

fn participant(sequence: &mut Sequence, id: &str) -> usize {
    if let Some(index) = sequence.participants.iter().position(|p| p.id == id) {
        return index;
    }
    sequence.participants.push(Participant {
        id: id.to_owned(),
        title: id.to_owned(),
    });
    sequence.participants.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(source: &str) -> Graph {
        match parse(source).expect("the diagram parses") {
            Diagram::Graph(graph) => graph,
            Diagram::Sequence(_) => panic!("expected a graph"),
        }
    }

    /// A statement may end in a semicolon, and the header is a statement.
    #[test]
    fn a_direction_ending_in_a_semicolon_is_still_the_direction() {
        assert_eq!(graph("graph LR;\n a --> b").flow, Flow::LeftRight);
        assert_eq!(graph("flowchart RL;\n a --> b").flow, Flow::LeftRight);
    }

    #[test]
    fn a_chain_with_a_fan_out_becomes_one_edge_per_pair() {
        let graph = graph("flowchart TD\n  a[Alpha] --> b(Beta) & c[(Gamma)] -.->|reads| d");
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 4);
        assert_eq!(graph.nodes[2].shape, Shape::Database);
        assert_eq!(graph.edges[3].label.as_deref(), Some("reads"));
        assert_eq!(graph.edges[3].stroke, Stroke::Dotted);
    }

    #[test]
    fn an_inline_label_is_the_same_label_as_a_piped_one() {
        let graph = graph("graph LR\n  a -- ships to --> b\n  a == must ==> c");
        assert_eq!(graph.flow, Flow::LeftRight);
        assert_eq!(graph.edges[0].label.as_deref(), Some("ships to"));
        assert_eq!(graph.edges[1].stroke, Stroke::Thick);
    }

    #[test]
    fn a_node_declared_in_a_subgraph_lives_there() {
        let graph =
            graph("flowchart TD\n  x --> a\n  subgraph core [The core]\n    a[Alpha]\n  end");
        assert_eq!(graph.clusters[0].title, "The core");
        assert_eq!(graph.nodes[1].cluster, Some(0));
        assert_eq!(graph.nodes[0].cluster, None);
    }

    #[test]
    fn c4_elements_carry_their_kind_and_their_boundary() {
        let graph = graph(
            "C4Container\n  Person(dev, \"Developer\", \"Writes, reviews\")\n  \
             System_Boundary(uze, \"uze\") {\n    \
             ContainerDb(store, \"Store\", \"Filesystem\", \"Package bytes\")\n  }\n  \
             System_Ext(git, \"Git\")\n  Rel(dev, store, \"Installs\", \"CLI\")",
        );
        assert_eq!(
            graph.nodes[0].description.as_deref(),
            Some("Writes, reviews")
        );
        assert_eq!(
            graph.nodes[1].kind.as_deref(),
            Some("Container: Filesystem")
        );
        assert_eq!(graph.nodes[1].cluster, Some(0));
        assert!(graph.nodes[2].external);
        assert_eq!(
            graph.nodes[2].kind.as_deref(),
            Some("External Software System")
        );
        assert_eq!(graph.clusters[0].title, "uze [Software System]");
        assert_eq!(graph.clusters[0].boundary, Some(Boundary::System));
        assert_eq!(graph.nodes[1].abstraction, Some(Abstraction::Container));
        assert_eq!(graph.clusters[0].id, "uze");
        assert_eq!(graph.edges[0].label.as_deref(), Some("Installs [CLI]"));
    }

    #[test]
    fn a_dynamic_view_numbers_its_relations_without_losing_their_ends() {
        let graph = graph(
            "C4Dynamic\n  Container(a, \"A\")\n  Container(b, \"B\")\n  RelIndex(1, a, b, \"Asks\")",
        );
        assert_eq!((graph.edges[0].from, graph.edges[0].to), (0, 1));
        assert_eq!(graph.edges[0].label.as_deref(), Some("1. Asks"));
        assert_eq!(graph.nodes.len(), 2, "the index did not become a box");
    }

    /// Mermaid draws a dynamic view's steps in the order they are written,
    /// whatever a `RelIndex` says, and numbers a plain `Rel` there too.
    #[test]
    fn a_dynamic_view_counts_its_steps_as_they_are_written() {
        let graph = graph(
            "C4Dynamic\n  Container(a, \"A\")\n  Container(b, \"B\")\n  \
             RelIndex(7, a, b, \"Asks\")\n  Rel(b, a, \"Answers\", \"JSON\")",
        );
        let labels: Vec<_> = graph.edges.iter().map(|e| e.label.as_deref()).collect();
        assert_eq!(labels, [Some("1. Asks"), Some("2. Answers [JSON]")]);
    }

    #[test]
    fn a_named_argument_lands_where_its_position_would() {
        let graph = graph(
            "C4Container\n  Container(api, \"API\", $descr=\"Serves\", $techn=\"Rust\")\n  \
             Container(db, \"DB\")\n  Rel(api, db, \"Reads\", $techn=\"SQL\", $descr=\"rows\")",
        );
        assert_eq!(graph.nodes[0].kind.as_deref(), Some("Container: Rust"));
        assert_eq!(graph.nodes[0].description.as_deref(), Some("Serves"));
        assert_eq!(graph.edges[0].label.as_deref(), Some("Reads [SQL]"));
        assert_eq!(graph.edges[0].description.as_deref(), Some("rows"));
    }

    /// A typed boundary's third argument is its tags, not a kind to print.
    #[test]
    fn a_boundary_reads_tags_where_a_plain_one_reads_a_type() {
        let graph = graph(
            "C4Component\n  Container_Boundary(cli, \"CLI\", \"v2\") {\n  }\n  \
             Boundary(core, \"uze-core\", \"Crate\") {\n  }",
        );
        assert_eq!(graph.clusters[0].title, "CLI [Container]");
        assert_eq!(graph.clusters[1].title, "uze-core [Crate]");
        assert_eq!(
            graph.clusters[1].boundary,
            Some(Boundary::Typed(Some("Crate".to_owned())))
        );
    }

    #[test]
    fn a_relation_back_runs_the_other_way_and_a_bidirectional_one_both() {
        let graph = graph(
            "C4Context\n  System(a, \"A\")\n  System(b, \"B\")\n  \
             Rel_Back(a, b, \"Feeds\")\n  BiRel(a, b, \"Syncs\")",
        );
        assert_eq!((graph.edges[0].from, graph.edges[0].to), (1, 0));
        assert!(!graph.edges[0].arrow_back);
        assert!(graph.edges[1].arrow && graph.edges[1].arrow_back);
    }

    #[test]
    fn a_queue_is_its_own_shape() {
        let graph = graph("C4Container\n  ContainerQueue(q, \"Jobs\", \"Redis\")");
        assert_eq!(graph.nodes[0].shape, Shape::Queue);
    }

    #[test]
    fn a_deployment_node_holds_what_is_declared_inside_it() {
        let graph = graph(
            "C4Deployment\n  Deployment_Node(host, \"Laptop\", \"Linux\") {\n    \
             Container(cli, \"CLI\", \"Rust\")\n  }",
        );
        assert_eq!(graph.clusters[0].title, "Laptop [Linux]");
        assert_eq!(graph.nodes[0].cluster, Some(0));
    }

    #[test]
    fn a_box_says_where_in_the_project_it_is() {
        let c4 = graph(
            "C4Component\n  Component(store, \"store\", \"Rust\", \"Owns bytes\", \
             $link=\"crates/uze-core/src/package/store.rs\")",
        );
        assert_eq!(
            c4.nodes[0].link.as_deref(),
            Some("crates/uze-core/src/package/store.rs")
        );
        assert_eq!(c4.nodes[0].description.as_deref(), Some("Owns bytes"));

        let flow = graph("flowchart TD\n  a[Alpha] --> b\n  click a href \"src/alpha.rs\"");
        assert_eq!(flow.nodes[0].link.as_deref(), Some("src/alpha.rs"));
    }

    #[test]
    fn a_sequence_keeps_its_participants_in_the_order_they_appear() {
        let Diagram::Sequence(sequence) = parse(
            "sequenceDiagram\n  actor U as User\n  U->>CLI: uze install\n  CLI-->>U: done\n  \
             Note over CLI: cached",
        )
        .unwrap() else {
            panic!("expected a sequence");
        };
        assert_eq!(sequence.participants[0].title, "User");
        assert_eq!(sequence.participants[1].id, "CLI");
        assert_eq!(sequence.steps.len(), 3);
    }
}
