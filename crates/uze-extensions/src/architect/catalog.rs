//! What there is to look at: areas, and the artifacts in each.
//!
//! An artifact is a Mermaid file in the directory the project declares.
//! Everything the surface needs to know about it is read off the file
//! itself — its area from the diagram's first word, its name from a
//! `title:` — so there is no index beside the files to keep in step with
//! them. A list of what a directory holds is the one document that is
//! wrong the moment somebody adds a file.

use std::path::{Path, PathBuf};

use crate::Host;

use super::mermaid;
use super::model::{Diagram, Graph};
use super::zoom::{Place, Viewed, Zoom};

/// How deep the declared directory is read. Deep enough for a project
/// that sorts its diagrams into folders; bounded, because the path is
/// the project's to declare and a walk with no floor is how a surface
/// ends up reading a `node_modules`.
const DEPTH: usize = 4;
const EXTENSIONS: [&str; 2] = ["mmd", "mermaid"];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Kind {
    C4,
    Sequence,
    Flowchart,
    /// Mermaid this surface does not draw yet. Listed rather than hidden:
    /// a file that silently fails to appear looks like a lost file.
    Other,
}

impl Kind {
    pub fn of(diagram: &str) -> Self {
        let keyword = first_word(diagram);
        match keyword {
            "flowchart" | "graph" => Self::Flowchart,
            "sequenceDiagram" => Self::Sequence,
            _ if keyword.starts_with("C4") => Self::C4,
            _ => Self::Other,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::C4 => "C4",
            Self::Sequence => "Sequence",
            Self::Flowchart => "Flowchart",
            Self::Other => "Other",
        }
    }
}

/// The word a Mermaid diagram opens with, which says what it is.
fn first_word(diagram: &str) -> &str {
    diagram
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub name: String,
    pub kind: Kind,
    /// Where it lives, as the project would say it.
    pub origin: String,
    /// Where it lives, as a link in another diagram would name it:
    /// relative to the project, or absolute once it has been read from
    /// a directory.
    pub path: PathBuf,
    /// The file as written, front matter and all — what `Source` shows.
    pub source: String,
}

impl Artifact {
    pub fn read(origin: impl Into<String>, source: impl Into<String>) -> Self {
        let (origin, source) = (origin.into(), source.into());
        let (front_matter, diagram) = crate::shared::front_matter::split(&source);
        let name = titled(front_matter)
            .or_else(|| titled(diagram))
            .unwrap_or_else(|| name_from_path(&origin));
        let kind = Kind::of(diagram);
        Self {
            name,
            kind,
            path: PathBuf::from(&origin),
            origin,
            source,
        }
    }

    /// The diagram without its front matter, which Mermaid reads as
    /// settings and this surface reads only for the name.
    pub fn diagram(&self) -> &str {
        crate::shared::front_matter::split(&self.source).1
    }
}

/// A `title:` in front matter, or the `title` statement a diagram may
/// carry in its own body.
fn titled(text: &str) -> Option<String> {
    text.lines().map(str::trim).find_map(|line| {
        let rest = line.strip_prefix("title")?;
        let rest = rest.strip_prefix(':').unwrap_or(rest);
        let starts_a_value = rest.starts_with(char::is_whitespace);
        let title = rest.trim().trim_matches('"').trim();
        (starts_a_value && !title.is_empty()).then(|| title.to_owned())
    })
}

fn name_from_path(origin: &str) -> String {
    let stem = Path::new(origin)
        .file_stem()
        .map(|stem| stem.to_string_lossy().replace(['-', '_'], " "))
        .unwrap_or_default();
    let mut letters = stem.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => origin.to_owned(),
    }
}

/// Every artifact, ordered by area and within one by name — except C4,
/// whose views are in the order their zoom tree is walked, each before
/// what zooms into it. That is also the order the menu is in, so an index
/// into this is an artifact's identity for as long as the catalog is.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Catalog {
    artifacts: Vec<Artifact>,
    zoom: Zoom,
}

impl Catalog {
    /// `project` is what a diagram's links are relative to.
    pub fn of(mut artifacts: Vec<Artifact>, project: &Path) -> Self {
        artifacts.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
        let graphs: Vec<Option<Graph>> = artifacts
            .iter()
            .map(
                |artifact| match (artifact.kind, mermaid::parse(artifact.diagram())) {
                    (Kind::C4, Ok(Diagram::Graph(graph))) => Some(graph),
                    _ => None,
                },
            )
            .collect();
        let views: Vec<Option<Viewed<'_>>> = graphs
            .iter()
            .zip(&artifacts)
            .map(|(graph, artifact)| {
                graph.as_ref().map(|graph| Viewed {
                    graph,
                    path: artifact.path.clone(),
                })
            })
            .collect();
        let zoom = Zoom::of(&views, project);
        let mut order = zoom.walk(|view| {
            (
                zoom.place(view).unwrap_or(Place::Deployment),
                artifacts[view].name.clone(),
            )
        });
        let rest: Vec<usize> = (0..artifacts.len())
            .filter(|index| !order.contains(index))
            .collect();
        order.extend(rest);
        let zoom = zoom.reordered(&order);
        let mut taken: Vec<Option<Artifact>> = artifacts.into_iter().map(Some).collect();
        let artifacts = order
            .iter()
            .filter_map(|&index| taken[index].take())
            .collect();
        Self { artifacts, zoom }
    }

    pub fn zoom(&self) -> &Zoom {
        &self.zoom
    }

    pub fn artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }

    pub fn get(&self, index: usize) -> Option<&Artifact> {
        self.artifacts.get(index)
    }
}

/// Every Mermaid file under `directory`, or why it could not be listed.
pub fn read(host: &dyn Host, directory: &Path) -> Result<Vec<Artifact>, String> {
    let mut artifacts = Vec::new();
    let mut pending: Vec<(PathBuf, usize)> = vec![(directory.to_path_buf(), 0)];
    while let Some((folder, depth)) = pending.pop() {
        let entries = match host.list_dir(&folder) {
            Ok(entries) => entries,
            Err(reason) if depth == 0 => return Err(reason),
            Err(_) => continue,
        };
        for entry in entries {
            let path = folder.join(&entry.name);
            if entry.name.starts_with('.') {
                continue;
            }
            if entry.directory {
                if depth < DEPTH {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            let is_mermaid = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| EXTENSIONS.contains(&extension));
            if !is_mermaid {
                continue;
            }
            let origin = path
                .strip_prefix(directory)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            // An unreadable file still gets its tab: the reason is what
            // the board shows, which is more use than the file going missing.
            let source = host
                .read_file(&path)
                .unwrap_or_else(|reason| format!("%% {reason}\n"));
            artifacts.push(Artifact::read(origin, source));
        }
    }
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artifact_says_which_area_it_belongs_to() {
        assert_eq!(Kind::of("%% note\n\nC4Container\n title x"), Kind::C4);
        assert_eq!(Kind::of("sequenceDiagram\n a->>b: hi"), Kind::Sequence);
        assert_eq!(Kind::of("graph LR\n a --> b"), Kind::Flowchart);
        assert_eq!(Kind::of("gantt\n title x"), Kind::Other);
    }

    #[test]
    fn an_artifact_names_itself_and_falls_back_to_its_file() {
        let titled = Artifact::read("a/ctx.mmd", "---\ntitle: System context\n---\nC4Context\n");
        assert_eq!(
            (titled.name.as_str(), titled.kind),
            ("System context", Kind::C4)
        );
        assert_eq!(titled.diagram().trim(), "C4Context");

        let in_body = Artifact::read("x.mmd", "C4Container\n  title \"Containers of uze\"\n");
        assert_eq!(in_body.name, "Containers of uze");

        let unnamed = Artifact::read("flows/install-pipeline.mmd", "flowchart LR\n a --> b\n");
        assert_eq!(unnamed.name, "Install pipeline");
        assert_eq!(unnamed.diagram(), "flowchart LR\n a --> b\n");
    }

    #[test]
    fn a_node_called_title_is_not_a_title() {
        let artifact = Artifact::read("x.mmd", "flowchart TD\n  titles --> b\n  title[Heading]\n");
        assert_eq!(artifact.name, "X");
    }

    fn names(catalog: &Catalog) -> Vec<&str> {
        catalog
            .artifacts()
            .iter()
            .map(|a| a.name.as_str())
            .collect()
    }

    /// Each view before what zooms into it, whatever the names say, and a
    /// view that is not a level after the ones that are.
    #[test]
    fn c4_views_are_listed_as_their_zooms_are_walked() {
        let catalog = Catalog::of(
            vec![
                Artifact::read(
                    "a.mmd",
                    "---\ntitle: A components\n---\nC4Component\n\
                     Container_Boundary(cli, \"CLI\") {\n}\n",
                ),
                Artifact::read(
                    "b.mmd",
                    "---\ntitle: Containers\n---\nC4Container\n\
                     System_Boundary(uze, \"uze\") {\nContainer(cli, \"CLI\", \"Rust\")\n}\n",
                ),
                Artifact::read("c.mmd", "---\ntitle: Deployment\n---\nC4Deployment\n"),
                Artifact::read(
                    "d.mmd",
                    "---\ntitle: System context\n---\nC4Context\nSystem(uze, \"uze\")\n",
                ),
                Artifact::read(
                    "e.mmd",
                    "---\ntitle: Flow\n---\nC4Dynamic\n\
                     System_Boundary(uze, \"uze\") {\n}\n",
                ),
                Artifact::read(
                    "f.mmd",
                    "---\ntitle: B components\n---\nC4Component\n\
                     Container_Boundary(cli, \"CLI\") {\n}\n",
                ),
            ],
            Path::new("/project"),
        );
        assert_eq!(
            names(&catalog),
            [
                "System context",
                "Containers",
                "A components",
                "B components",
                "Flow",
                "Deployment"
            ]
        );
        let zoom = catalog.zoom();
        assert_eq!(zoom.parent(1), Some(0));
        assert_eq!((zoom.parent(2), zoom.parent(3)), (Some(1), Some(1)));
        assert_eq!(
            zoom.parent(4),
            Some(0),
            "a dynamic view sits by the containers"
        );
        assert_eq!(zoom.parent(5), None, "a deployment view is a root");
        assert_eq!(zoom.inside(1, "cli"), [2, 3]);
        assert_eq!(zoom.path(3), [0, 1, 3]);
        assert_eq!(zoom.siblings(2), [2, 3]);
    }

    /// The old rule joined any box to any boundary with its alias, so a
    /// system box skipped the containers and landed in a component view.
    #[test]
    fn a_view_zooms_only_into_an_element_of_its_kind_one_level_up() {
        let catalog = Catalog::of(
            vec![
                Artifact::read(
                    "a.mmd",
                    "C4Context\nSystem(uze, \"uze\")\nSystem_Ext(git, \"Git\")\n",
                ),
                Artifact::read(
                    "b.mmd",
                    "C4Component\nContainer_Boundary(uze, \"uze\") {\n}\n",
                ),
                Artifact::read("c.mmd", "C4Container\nSystem_Boundary(git, \"Git\") {\n}\n"),
            ],
            Path::new("/project"),
        );
        let zoom = catalog.zoom();
        assert_eq!(zoom.inside(0, "uze"), Vec::<usize>::new());
        assert!(
            (1..3).all(|view| zoom.parent(view).is_none()),
            "a component view needs a container, an external system no inside"
        );
    }

    #[test]
    fn a_link_to_another_view_is_its_inside() {
        let catalog = Catalog::of(
            vec![
                Artifact::read(
                    "docs/a.mmd",
                    "C4Context\nSystem(pay, \"Payments\", $link=\"docs/b.mmd\")\n",
                ),
                Artifact::read("docs/b.mmd", "C4Container\n"),
            ],
            Path::new("/project"),
        );
        assert_eq!(catalog.zoom().inside(0, "pay"), [1]);
    }

    #[test]
    fn a_context_with_several_systems_is_a_landscape() {
        let catalog = Catalog::of(
            vec![Artifact::read(
                "a.mmd",
                "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\n",
            )],
            Path::new("/project"),
        );
        assert_eq!(catalog.zoom().place(0), Some(Place::Landscape));
    }

    #[test]
    fn the_catalog_is_ordered_by_area_then_by_name() {
        let catalog = Catalog::of(
            vec![
                Artifact::read("b.mmd", "flowchart TD\n a --> b"),
                Artifact::read("z.mmd", "C4Context\n"),
                Artifact::read("a.mmd", "C4Context\n"),
            ],
            Path::new("/project"),
        );
        let names: Vec<&str> = catalog
            .artifacts()
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["A", "Z", "B"]);
    }
}
