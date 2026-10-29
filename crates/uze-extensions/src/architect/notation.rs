//! Where a C4 view departs from the model's notation.
//!
//! C4 asks every element for a type, a description and — for containers
//! and components — a technology; every line for a label, running one
//! way; every title for the view's type and scope; and every view below
//! the context for the one element above it that it zooms into. The type
//! is the macro, so it cannot be missing; the rest can, and none of it
//! stops a diagram drawing, which is why nobody notices until a reader
//! has to guess.

use super::catalog::Catalog;
use super::model::{Abstraction, Graph};
use super::zoom::Place;

/// The notes for the artifact at `index`, given every C4 view's graph by
/// the same index (`None` for anything else).
pub fn notes(catalog: &Catalog, graphs: &[Option<Graph>], index: usize) -> Vec<String> {
    let (Some(graph), Some(artifact)) = (&graphs[index], catalog.get(index)) else {
        return Vec::new();
    };
    let zoom = catalog.zoom();
    let mut notes = Vec::new();
    if let Some(place) = zoom.place(index)
        && !artifact.name.to_lowercase().contains(place.title_word())
    {
        notes.push(format!(
            "the title does not say it is a {} view",
            place.title_word()
        ));
    }
    if let Some(place) = zoom.place(index)
        && matches!(place, Place::Container | Place::Component)
        && zoom.parent(index).is_none()
    {
        let above = match place {
            Place::Container => "a system in a context view",
            _ => "a container in a container view",
        };
        notes.push(match zoom.scope(index) {
            Some(scope) => format!("the view zooms into `{scope}`, which no view draws as {above}"),
            None => format!(
                "the view is drawn inside no `{}`, so it zooms into nothing",
                match place {
                    Place::Container => "System_Boundary",
                    _ => "Container_Boundary",
                }
            ),
        });
    }
    for node in graph.nodes.iter().filter(|node| node.abstraction.is_some()) {
        if node.description.is_none() {
            notes.push(format!("`{}` has no description", node.id));
        }
        let technical = matches!(
            node.abstraction,
            Some(Abstraction::Container | Abstraction::Component)
        );
        if technical && !node.kind.as_deref().is_some_and(|kind| kind.contains(':')) {
            notes.push(format!("`{}` names no technology", node.id));
        }
        let elsewhere = graphs.iter().enumerate().find_map(|(other, graph)| {
            let found = graph
                .as_ref()?
                .nodes
                .iter()
                .find(|other| other.id == node.id && other.abstraction.is_some())?;
            (other != index && found.abstraction != node.abstraction)
                .then(|| (found.abstraction, catalog.get(other)))
        });
        if let Some((Some(kind), Some(other))) = elsewhere {
            notes.push(format!(
                "`{}` is a {} here and a {} in {}",
                node.id,
                named(node.abstraction),
                named(Some(kind)),
                other.origin
            ));
        }
    }
    for edge in &graph.edges {
        let (from, to) = (&graph.nodes[edge.from].id, &graph.nodes[edge.to].id);
        // A dynamic step is numbered whatever it says, so a bare number
        // is a line with no label.
        let unlabelled = edge.label.as_deref().is_none_or(|label| {
            label
                .strip_suffix('.')
                .is_some_and(|step| step.chars().all(|c| c.is_ascii_digit()))
        });
        if unlabelled {
            notes.push(format!("the line from `{from}` to `{to}` has no label"));
        }
        if edge.arrow_back {
            notes.push(format!(
                "the line between `{from}` and `{to}` runs both ways; C4 lines run one way"
            ));
        }
    }
    notes
}

fn named(abstraction: Option<Abstraction>) -> &'static str {
    match abstraction {
        Some(Abstraction::Person) => "person",
        Some(Abstraction::System) => "software system",
        Some(Abstraction::Container) => "container",
        Some(Abstraction::Component) => "component",
        None => "box",
    }
}
