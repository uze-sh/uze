//! How C4 views zoom into one another.
//!
//! C4 is a tree of zooms, not a ladder: a container view is the inside of
//! one software system, a component view the inside of one container, and
//! several views may be the inside of the same thing. The tree is read off
//! what the views already say — the boundary a view is drawn inside, and
//! the kind of element a view one level up draws under that alias — so a
//! project writes nothing beside its diagrams to have it. A `$link` naming
//! another view says it outright, and is believed first.

use std::path::{Component, Path, PathBuf};

use super::model::{Abstraction, Boundary, Graph, View};

/// Where a view stands in the model. The order is the order views that
/// share a parent are listed in: the levels from the outside in, then the
/// views that are not a level.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Place {
    Landscape,
    Context,
    Container,
    Component,
    Dynamic,
    Deployment,
}

impl Place {
    /// How the menu names a step of the way down.
    pub fn name(self) -> &'static str {
        match self {
            Self::Landscape => "Landscape",
            Self::Context => "Context",
            Self::Container => "Containers",
            Self::Component => "Components",
            Self::Dynamic => "Dynamic",
            Self::Deployment => "Deployment",
        }
    }
}

/// What a view zooms into: the alias, and the kind of element that alias
/// must name in the view above.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Scope {
    alias: String,
    kind: Abstraction,
}

/// One C4 view as the tree needs it.
pub struct Viewed<'a> {
    pub graph: &'a Graph,
    /// Where the file is, as a link would name it.
    pub path: PathBuf,
}

/// The tree, by index into the views it was read from. `None` for a view
/// that is not C4.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Zoom {
    places: Vec<Option<Place>>,
    parents: Vec<Option<usize>>,
    /// The alias of the box in the parent that leads here.
    through: Vec<Option<String>>,
    /// What each view zooms into, whether or not a view above draws it.
    scopes: Vec<Option<String>>,
}

impl Zoom {
    /// Reads the tree. `project` is what a link is relative to.
    pub fn of(views: &[Option<Viewed<'_>>], project: &Path) -> Self {
        let places: Vec<Option<Place>> = views
            .iter()
            .map(|view| view.as_ref().and_then(|view| place_of(view.graph)))
            .collect();
        let scopes: Vec<Option<Scope>> = views
            .iter()
            .zip(&places)
            .map(|(view, place)| scope_of(view.as_ref()?.graph, (*place)?))
            .collect();
        let mut parents = vec![None; views.len()];
        let mut through = vec![None; views.len()];
        for (from, view) in views.iter().enumerate() {
            let Some(view) = view else { continue };
            for node in &view.graph.nodes {
                let Some(target) = node.link.as_deref().and_then(|link| {
                    let path = project_path(link)?;
                    let wanted = normal(&project.join(path));
                    views.iter().position(|other| {
                        other
                            .as_ref()
                            .is_some_and(|other| normal(&project.join(&other.path)) == wanted)
                    })
                }) else {
                    continue;
                };
                if target != from && places[target].is_some() && parents[target].is_none() {
                    parents[target] = Some(from);
                    through[target] = Some(node.id.clone());
                }
            }
        }
        for (child, scope) in scopes.iter().enumerate() {
            let Some(scope) = scope.as_ref().filter(|_| parents[child].is_none()) else {
                continue;
            };
            let wanted = match scope.kind {
                Abstraction::System => [Place::Context, Place::Landscape],
                _ => [Place::Container, Place::Container],
            };
            let parent = wanted.iter().find_map(|wanted| {
                (0..views.len()).find(|&candidate| {
                    candidate != child
                        && places[candidate] == Some(*wanted)
                        && views[candidate].as_ref().is_some_and(|view| {
                            view.graph.nodes.iter().any(|node| {
                                node.id == scope.alias
                                    && !node.external
                                    && node.abstraction == Some(scope.kind)
                            })
                        })
                })
            });
            if let Some(parent) = parent {
                parents[child] = Some(parent);
                through[child] = Some(scope.alias.clone());
            }
        }
        // A link may close a loop; the tree keeps the first way in and
        // makes the view that would come back round a root.
        for start in 0..parents.len() {
            let mut seen = vec![start];
            let mut at = start;
            while let Some(parent) = parents[at] {
                if seen.contains(&parent) {
                    parents[at] = None;
                    through[at] = None;
                    break;
                }
                seen.push(parent);
                at = parent;
            }
        }
        Self {
            places,
            parents,
            through,
            scopes: scopes
                .into_iter()
                .map(|scope| scope.map(|scope| scope.alias))
                .collect(),
        }
    }

    pub fn place(&self, view: usize) -> Option<Place> {
        self.places.get(view).copied().flatten()
    }

    pub fn parent(&self, view: usize) -> Option<usize> {
        self.parents.get(view).copied().flatten()
    }

    /// The alias of the box in the parent that leads to `view`.
    pub fn through(&self, view: usize) -> Option<&str> {
        self.through.get(view).and_then(Option::as_deref)
    }

    /// What `view` zooms into, as an alias, when it zooms into anything.
    pub fn scope(&self, view: usize) -> Option<&str> {
        self.scopes.get(view).and_then(Option::as_deref)
    }

    /// The views a box `alias` on `view` leads into, in tree order when
    /// `order` is the tree's order.
    pub fn inside(&self, view: usize, alias: &str) -> Vec<usize> {
        (0..self.parents.len())
            .filter(|&child| {
                self.parents[child] == Some(view) && self.through[child].as_deref() == Some(alias)
            })
            .collect()
    }

    /// The views that share `view`'s parent, itself included — the roots,
    /// for a root.
    pub fn siblings(&self, view: usize) -> Vec<usize> {
        if self.place(view).is_none() {
            return Vec::new();
        }
        let parent = self.parent(view);
        (0..self.parents.len())
            .filter(|&other| self.places[other].is_some() && self.parents[other] == parent)
            .collect()
    }

    /// From the root down to `view`.
    pub fn path(&self, view: usize) -> Vec<usize> {
        let mut path = vec![view];
        while let Some(parent) = self.parent(path[0]) {
            path.insert(0, parent);
        }
        path
    }

    /// The C4 views walked depth first, a view before what zooms into it
    /// and, among one parent's, by `rank`.
    pub fn walk(&self, rank: impl Fn(usize) -> (Place, String)) -> Vec<usize> {
        let ordered = |mut views: Vec<usize>| {
            views.sort_by_key(|&view| rank(view));
            views
        };
        let mut walked = Vec::new();
        let mut pending: Vec<usize> = ordered(
            (0..self.parents.len())
                .filter(|&view| self.places[view].is_some() && self.parents[view].is_none())
                .collect(),
        );
        pending.reverse();
        while let Some(view) = pending.pop() {
            walked.push(view);
            let mut children = ordered(
                (0..self.parents.len())
                    .filter(|&child| self.parents[child] == Some(view))
                    .collect(),
            );
            children.reverse();
            pending.extend(children);
        }
        walked
    }

    /// The same tree with every index moved to where `order` put it:
    /// `order[new] == old`.
    pub fn reordered(&self, order: &[usize]) -> Self {
        let mut moved = vec![0; order.len()];
        for (new, &old) in order.iter().enumerate() {
            moved[old] = new;
        }
        let pick = |new: usize| order[new];
        Self {
            places: (0..order.len()).map(|new| self.places[pick(new)]).collect(),
            parents: (0..order.len())
                .map(|new| self.parents[pick(new)].map(|parent| moved[parent]))
                .collect(),
            through: (0..order.len())
                .map(|new| self.through[pick(new)].clone())
                .collect(),
            scopes: (0..order.len())
                .map(|new| self.scopes[pick(new)].clone())
                .collect(),
        }
    }
}

/// Which place a C4 view takes. A context view with one system in scope
/// is that system's context; with none, or several, or an enterprise
/// boundary, it is a landscape — C4's "really just a system context
/// diagram" without the focus on one system.
fn place_of(graph: &Graph) -> Option<Place> {
    Some(match graph.view? {
        View::Context => match in_scope(graph).len() == 1 && !has_enterprise(graph) {
            true => Place::Context,
            false => Place::Landscape,
        },
        View::Container => Place::Container,
        View::Component => Place::Component,
        View::Dynamic => Place::Dynamic,
        View::Deployment => Place::Deployment,
    })
}

fn in_scope(graph: &Graph) -> Vec<&str> {
    graph
        .nodes
        .iter()
        .filter(|node| node.abstraction == Some(Abstraction::System) && !node.external)
        .map(|node| node.id.as_str())
        .collect()
}

fn has_enterprise(graph: &Graph) -> bool {
    graph
        .clusters
        .iter()
        .any(|cluster| cluster.boundary == Some(Boundary::Enterprise))
}

fn scope_of(graph: &Graph, place: Place) -> Option<Scope> {
    let outermost = |kinds: &[Abstraction]| {
        graph.clusters.iter().find_map(|cluster| {
            let kind = bounds(cluster.boundary.as_ref()?)?;
            let nested = std::iter::successors(cluster.parent, |&up| graph.clusters[up].parent)
                .any(|up| {
                    graph.clusters[up]
                        .boundary
                        .as_ref()
                        .and_then(bounds)
                        .is_some_and(|up| kinds.contains(&up))
                });
            (kinds.contains(&kind) && !nested).then(|| Scope {
                alias: cluster.id.clone(),
                kind,
            })
        })
    };
    match place {
        Place::Context => in_scope(graph).first().map(|alias| Scope {
            alias: (*alias).to_owned(),
            kind: Abstraction::System,
        }),
        Place::Container => outermost(&[Abstraction::System]),
        Place::Component => outermost(&[Abstraction::Container]),
        Place::Dynamic => outermost(&[Abstraction::System, Abstraction::Container]),
        Place::Landscape | Place::Deployment => None,
    }
    .filter(|scope| !scope.alias.is_empty())
}

/// The kind of element a boundary is the inside of, when it is one.
fn bounds(boundary: &Boundary) -> Option<Abstraction> {
    match boundary {
        Boundary::System => Some(Abstraction::System),
        Boundary::Container => Some(Abstraction::Container),
        Boundary::Typed(Some(kind)) => match kind.to_lowercase().as_str() {
            "system" | "software system" => Some(Abstraction::System),
            "container" => Some(Abstraction::Container),
            _ => None,
        },
        _ => None,
    }
}

/// `link` as a path inside the project, or `None` for one that is not:
/// an absolute path, a `..`, or a URL. A `#` fragment is dropped.
///
/// The diagram is repository content, and the code surface a link opens
/// offers to edit and delete what it lands on — so nothing outside the
/// project is followed anywhere.
pub fn project_path(link: &str) -> Option<PathBuf> {
    let path = link.split('#').next().unwrap_or_default();
    let path = Path::new(path);
    let mut components = path.components().peekable();
    components.peek()?;
    let inside = path.components().all(|component| match component {
        Component::Normal(part) => !part.to_string_lossy().contains(':'),
        Component::CurDir => true,
        _ => false,
    });
    inside.then(|| path.to_path_buf())
}

/// A path with its `.` steps taken out, so two spellings of one file
/// compare equal.
fn normal(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| *component != Component::CurDir)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architect::{mermaid, model::Diagram};

    fn graph(source: &str) -> Graph {
        match mermaid::parse(source).expect("the diagram parses") {
            Diagram::Graph(graph) => graph,
            Diagram::Sequence(_) => panic!("expected a graph"),
        }
    }

    /// The tree read off `files`, each a view at its path in the project.
    fn zoom_of(files: &[(&str, &str)]) -> Zoom {
        let graphs: Vec<Graph> = files.iter().map(|(_, source)| graph(source)).collect();
        let views: Vec<Option<Viewed<'_>>> = graphs
            .iter()
            .zip(files)
            .map(|(graph, (path, _))| {
                Some(Viewed {
                    graph,
                    path: PathBuf::from(path),
                })
            })
            .collect();
        Zoom::of(&views, Path::new("project"))
    }

    #[test]
    fn a_context_with_one_system_of_its_own_is_that_systems_context() {
        let zoom = zoom_of(&[(
            "context.mmd",
            "C4Context\nSystem(shop, \"Shop\")\nSystem_Ext(bank, \"Bank\")\nRel(shop, bank, \"Pays\")",
        )]);
        assert_eq!(zoom.place(0), Some(Place::Context));
        assert_eq!(zoom.scope(0), Some("shop"));
    }

    #[test]
    fn a_context_with_no_system_of_its_own_or_an_enterprise_is_a_landscape() {
        let zoom = zoom_of(&[
            ("bare.mmd", "C4Context\nSystem_Ext(bank, \"Bank\")"),
            (
                "enterprise.mmd",
                "C4Context\nEnterprise_Boundary(corp, \"Corp\") {\nSystem(shop, \"Shop\")\n}",
            ),
        ]);
        assert_eq!(zoom.place(0), Some(Place::Landscape));
        assert_eq!(zoom.place(1), Some(Place::Landscape));
    }

    #[test]
    fn a_deployment_view_takes_the_deployment_place() {
        let zoom = zoom_of(&[(
            "deployment.mmd",
            "C4Deployment\nDeployment_Node(host, \"Laptop\", \"Linux\") {\nContainer(api, \"API\")\n}",
        )]);
        assert_eq!(zoom.place(0), Some(Place::Deployment));
    }

    #[test]
    fn a_view_zooms_into_the_system_it_is_drawn_inside_even_with_no_view_above() {
        let zoom = zoom_of(&[(
            "containers.mmd",
            "C4Container\nContainer_Boundary(web, \"Web\") {\nContainer(page, \"Page\")\n}\n\
             System_Boundary(shop, \"Shop\") {\nContainer(api, \"API\")\n}",
        )]);
        assert_eq!(zoom.parent(0), None);
        assert_eq!(zoom.scope(0), Some("shop"), "the system, not the container");
    }

    #[test]
    fn a_plain_boundary_bounds_what_its_type_names() {
        let zoom = zoom_of(&[
            (
                "system.mmd",
                "C4Container\nBoundary(shop, \"Shop\", \"Software System\") {\nContainer(api, \"API\")\n}",
            ),
            (
                "container.mmd",
                "C4Component\nBoundary(api, \"API\", \"container\") {\nComponent(router, \"Router\")\n}",
            ),
            (
                "team.mmd",
                "C4Container\nBoundary(people, \"People\", \"team\") {\nContainer(api, \"API\")\n}",
            ),
        ]);
        assert_eq!(zoom.scope(0), Some("shop"));
        assert_eq!(zoom.scope(1), Some("api"));
        assert_eq!(zoom.scope(2), None);
    }

    #[test]
    fn a_box_leads_only_into_the_views_drawn_inside_it() {
        let zoom = zoom_of(&[
            (
                "landscape.mmd",
                "C4Context\nSystem(shop, \"Shop\")\nSystem(blog, \"Blog\")",
            ),
            (
                "shop.mmd",
                "C4Container\nSystem_Boundary(shop, \"Shop\") {\nContainer(api, \"API\")\n}",
            ),
            (
                "blog.mmd",
                "C4Container\nSystem_Boundary(blog, \"Blog\") {\nContainer(cms, \"CMS\")\n}",
            ),
        ]);
        assert_eq!(zoom.place(0), Some(Place::Landscape));
        assert_eq!(zoom.inside(0, "shop"), vec![1]);
        assert_eq!(zoom.inside(0, "blog"), vec![2]);
    }

    #[test]
    fn a_link_from_a_view_to_itself_leaves_the_way_in_to_the_view_that_links_to_it() {
        let zoom = zoom_of(&[
            (
                "containers.mmd",
                "C4Container\nContainer(api, \"API\", \"Rust\", \"Serves\", $link=\"containers.mmd\")",
            ),
            (
                "context.mmd",
                "C4Context\nSystem(shop, \"Shop\", \"Sells\", $link=\"containers.mmd\")",
            ),
        ]);
        assert_eq!(zoom.parent(0), Some(1));
        assert_eq!(zoom.through(0), Some("shop"));
    }

    #[test]
    fn a_view_two_links_lead_into_keeps_the_first_way_in() {
        let zoom = zoom_of(&[
            (
                "first.mmd",
                "C4Context\nSystem(shop, \"Shop\", \"Sells\", $link=\"containers.mmd\")",
            ),
            (
                "second.mmd",
                "C4Context\nSystem(store, \"Store\", \"Sells\", $link=\"containers.mmd\")",
            ),
            ("containers.mmd", "C4Container\nContainer(api, \"API\")"),
        ]);
        assert_eq!(zoom.parent(2), Some(0));
        assert_eq!(zoom.through(2), Some("shop"));
    }
}
