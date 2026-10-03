//! What a spec-driven-development tool's layout means, as data.
//!
//! The tools differ in where their files go and what they are called, and
//! agree on what a reader does with them: a unit of intent holding a *why*,
//! a *how*, a list of *steps* and, in some, a *contract* that outlives the
//! unit. So the surface speaks in [`Role`]s and [`Subject`]s, and a
//! [`Dialect`] is a table that maps one tool's layout onto them.
//!
//! A table rather than a trait on purpose: a trait would let a dialect run
//! code, and then adding the next tool is a review of that code. A table
//! is reviewed by reading it.

/// What a file is *for*, in the order a unit's files are listed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Role {
    /// The motivation: a proposal, a requirements document.
    Why,
    /// The design.
    How,
    /// The work, as checkboxes.
    Steps,
    /// A requirement that outlives the unit that wrote it.
    Contract,
    /// A file the dialect names no role for. Listed, never hidden: a file
    /// that silently fails to appear looks like a lost file.
    Other,
}

/// What the surface can be about, in the order it offers them.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Subject {
    /// Units still in flight.
    Changes,
    /// What outlives them.
    Specs,
    /// Units that were finished and put away.
    Archive,
}

impl Subject {
    pub const ALL: [Subject; 3] = [Subject::Changes, Subject::Specs, Subject::Archive];

    pub fn label(self) -> &'static str {
        match self {
            Subject::Changes => "Changes",
            Subject::Specs => "Specs",
            Subject::Archive => "Archive",
        }
    }
}

/// How a collection's units are ordered by name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Order {
    Ascending,
    /// For a collection whose names begin with a date, so the most recent
    /// comes first.
    Descending,
}

/// What one unit of a collection is. Hidden entries and the collection's
/// `skip` are never units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    /// Every directory directly inside the path.
    Directories,
    /// Every directory beneath the path that holds this file, named by its
    /// path from the collection: a capability can sit under an area
    /// (`identity/user-auth`), and naming it by the area alone would list
    /// one unit where the tool keeps several.
    Holding(&'static str),
    /// The path is a file, and that file is the whole unit: a document a
    /// tool keeps one of, such as a project's constitution.
    File,
    /// Every document directly inside the path is a unit of its own, named
    /// by its stem: a tool that writes one file per plan rather than a
    /// directory per change.
    Files,
}

/// How far a change's steps got, by the tool's own account of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tally {
    /// The checkboxes of its first [`Role::Steps`] file.
    Checkboxes,
    /// Every file named `<prefix><step>` is a step, done once a file named
    /// `<prefix><receipt>` sits beside it: a tool that records finishing a
    /// plan by writing what it did, rather than by ticking the plan.
    Receipts {
        step: &'static str,
        receipt: &'static str,
    },
}

/// Where a dialect keeps one subject's units, and how they are found.
#[derive(Clone, Copy, Debug)]
pub struct Collection {
    pub subject: Subject,
    /// Relative to the checkout root.
    pub path: &'static str,
    pub shape: Shape,
    pub skip: &'static [&'static str],
    pub order: Order,
}

/// One tool's layout.
#[derive(Clone, Copy, Debug)]
pub struct Dialect {
    pub name: &'static str,
    /// A directory, relative to the checkout root, that only this tool
    /// makes. Its presence is the whole of detection: the tool decides
    /// where its files live, so nothing about it is declared a second time.
    pub marker: &'static str,
    pub collections: &'static [Collection],
    pub tally: Tally,
    /// Paths relative to a unit, first match wins; a file matching none is
    /// [`Role::Other`]. `*` stands for one path segment and `**` for one or
    /// more, and what they stood for names the file — `specs/**/spec.md`
    /// names `specs/agent/isolation/spec.md` as `agent/isolation`. A `*`
    /// inside a segment stands for any run of characters in it, and only
    /// matches: a file whose own name is a wildcard is named by its path,
    /// because what the wildcard stood for is the part that tells files
    /// apart (`*PLAN.md` names `01-02-PLAN.md` as `01-02-PLAN`). Files
    /// sharing a role list in the order their patterns are written, so the
    /// file a reader opens first for a role is written first.
    pub roles: &'static [(&'static str, Role)],
}

impl Dialect {
    /// Whether a finished unit is put away, rather than staying where it
    /// was written.
    pub fn archives(&self) -> bool {
        self.collections
            .iter()
            .any(|collection| collection.subject == Subject::Archive)
    }
}

pub const OPENSPEC: Dialect = Dialect {
    name: "OpenSpec",
    marker: "openspec",
    collections: &[
        Collection {
            subject: Subject::Changes,
            path: "openspec/changes",
            shape: Shape::Directories,
            skip: &["archive"],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: "openspec/specs",
            shape: Shape::Holding("spec.md"),
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Archive,
            path: "openspec/changes/archive",
            shape: Shape::Directories,
            skip: &[],
            order: Order::Descending,
        },
    ],
    roles: &[
        ("proposal.md", Role::Why),
        ("design.md", Role::How),
        ("tasks.md", Role::Steps),
        ("specs/**/spec.md", Role::Contract),
        ("spec.md", Role::Contract),
        ("**/spec.md", Role::Contract),
    ],
    tally: Tally::Checkboxes,
};

/// GitHub's Spec Kit: one directory per feature under `specs/`, numbered
/// or timestamped, so ascending is the order they were started in. What
/// outlives a feature is the project's constitution, the principles every
/// plan is checked against; nothing is put away.
pub const SPEC_KIT: Dialect = Dialect {
    name: "Spec Kit",
    marker: ".specify",
    collections: &[
        Collection {
            subject: Subject::Changes,
            path: "specs",
            shape: Shape::Directories,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: ".specify/memory/constitution.md",
            shape: Shape::File,
            skip: &[],
            order: Order::Ascending,
        },
    ],
    roles: &[
        ("spec.md", Role::Why),
        ("plan.md", Role::How),
        // What `plan` writes beside the plan, read after it.
        ("research.md", Role::How),
        ("data-model.md", Role::How),
        ("quickstart.md", Role::How),
        ("tasks.md", Role::Steps),
        ("constitution.md", Role::Contract),
        // `contracts/` are the feature's interfaces, not a requirement that
        // outlives it, and `checklists/` are not its steps: both stay
        // `Other`, named by their path.
    ],
    tally: Tally::Checkboxes,
};

/// Superpowers: no directory per change, but a file per document, dated,
/// so descending is newest first. `brainstorming` writes the design and
/// `writing-plans` the plan as checkbox steps, under separate directories
/// and with no link between them a table could follow, so each is a unit:
/// the plans are what is in flight, the designs what they were built from.
pub const SUPERPOWERS: Dialect = Dialect {
    name: "Superpowers",
    marker: "docs/superpowers",
    collections: &[
        Collection {
            subject: Subject::Changes,
            path: "docs/superpowers/plans",
            shape: Shape::Files,
            skip: &[],
            order: Order::Descending,
        },
        Collection {
            subject: Subject::Specs,
            path: "docs/superpowers/specs",
            shape: Shape::Files,
            skip: &[],
            order: Order::Descending,
        },
    ],
    roles: &[("*-design.md", Role::How), ("*.md", Role::Steps)],
    tally: Tally::Checkboxes,
};

/// Get Shit Done: phases of the current milestone under `phases/`, ad-hoc
/// work under `quick/`, and the project-wide documents every phase is
/// planned against at the root of `.planning/`. Every file in a phase is
/// prefixed with its number, and a plan is finished when the executor
/// writes the summary beside it: GSD plans are task blocks, not
/// checkboxes. A completed milestone moves its phases to
/// `milestones/v<X.Y>-phases/` and keeps a copy of the roadmap and the
/// requirements beside them.
pub const GSD: Dialect = Dialect {
    name: "GSD",
    marker: ".planning",
    collections: &[
        Collection {
            subject: Subject::Changes,
            path: ".planning/phases",
            shape: Shape::Directories,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Changes,
            path: ".planning/quick",
            shape: Shape::Directories,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: ".planning/PROJECT.md",
            shape: Shape::File,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: ".planning/REQUIREMENTS.md",
            shape: Shape::File,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: ".planning/ROADMAP.md",
            shape: Shape::File,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: ".planning/STATE.md",
            shape: Shape::File,
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Archive,
            path: ".planning/milestones",
            shape: Shape::Directories,
            skip: &[],
            order: Order::Descending,
        },
        Collection {
            subject: Subject::Archive,
            path: ".planning/milestones",
            shape: Shape::Files,
            skip: &[],
            order: Order::Descending,
        },
    ],
    roles: &[
        // The UI and AI design contracts end in `SPEC.md` too, and are a
        // how rather than the phase's locked requirements.
        ("*UI-SPEC.md", Role::How),
        ("*AI-SPEC.md", Role::How),
        ("*SPEC.md", Role::Why),
        ("*CONTEXT.md", Role::Why),
        ("PROJECT.md", Role::Why),
        ("*RESEARCH.md", Role::How),
        ("DISCOVERY.md", Role::How),
        ("*PLAN.md", Role::Steps),
        ("*ROADMAP.md", Role::Steps),
        ("*REQUIREMENTS.md", Role::Contract),
        // An archived milestone's phases sit one directory deeper.
        ("*/*UI-SPEC.md", Role::How),
        ("*/*AI-SPEC.md", Role::How),
        ("*/*SPEC.md", Role::Why),
        ("*/*CONTEXT.md", Role::Why),
        ("*/*RESEARCH.md", Role::How),
        ("*/*PLAN.md", Role::Steps),
        // Summaries, verification, UAT, validation and the discussion log
        // are what happened, not what was intended: `Other`, by path.
    ],
    tally: Tally::Receipts {
        step: "PLAN.md",
        receipt: "SUMMARY.md",
    },
};

/// Every dialect this build reads, in the order a checkout holding more
/// than one lists them.
pub const SHIPPED: &[Dialect] = &[OPENSPEC, SPEC_KIT, SUPERPOWERS, GSD];

/// What a file is for, what to call it, and where it lists among the
/// files sharing its role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Classified {
    pub role: Role,
    pub name: String,
    /// The position of the pattern that matched; past every pattern for
    /// [`Role::Other`].
    pub rank: usize,
}

/// What a file is for and what to call it, by its path inside its unit.
pub fn classify(dialect: &Dialect, relative: &str) -> Classified {
    let segments: Vec<&str> = relative.split('/').collect();
    for (rank, (pattern, role)) in dialect.roles.iter().enumerate() {
        let pattern: Vec<&str> = pattern.split('/').collect();
        let mut captured = Vec::new();
        if matches(&pattern, &segments, &mut captured) {
            let named_by_wildcard = pattern
                .last()
                .is_some_and(|last| last.len() > 1 && last.contains('*'));
            let name = if named_by_wildcard {
                without_markdown(relative)
            } else if captured.is_empty() {
                stem(relative)
            } else {
                captured.join("/")
            };
            return Classified {
                role: *role,
                name,
                rank,
            };
        }
    }
    Classified {
        role: Role::Other,
        name: without_markdown(relative),
        rank: dialect.roles.len(),
    }
}

fn without_markdown(relative: &str) -> String {
    relative.strip_suffix(".md").unwrap_or(relative).to_owned()
}

fn stem(relative: &str) -> String {
    let file = relative.rsplit('/').next().unwrap_or(relative);
    file.strip_suffix(".md").unwrap_or(file).to_owned()
}

fn matches<'a>(pattern: &[&str], path: &[&'a str], captured: &mut Vec<&'a str>) -> bool {
    match (pattern.split_first(), path.split_first()) {
        (None, None) => true,
        (Some((&"**", rest)), Some(_)) => {
            // One or more segments: try the shortest run first, so the
            // literal after it anchors as early as it can.
            (1..=path.len()).any(|taken| {
                let mut attempt = captured.clone();
                attempt.extend_from_slice(&path[..taken]);
                if matches(rest, &path[taken..], &mut attempt) {
                    *captured = attempt;
                    true
                } else {
                    false
                }
            })
        }
        (Some((&"*", rest)), Some((segment, path_rest))) => {
            captured.push(segment);
            matches(rest, path_rest, captured) || {
                captured.pop();
                false
            }
        }
        (Some((literal, rest)), Some((segment, path_rest))) => {
            within_segment(literal, segment) && matches(rest, path_rest, captured)
        }
        _ => false,
    }
}

/// Whether `segment` matches `pattern`, where a `*` inside it stands for
/// any run of characters, the empty one included.
fn within_segment(pattern: &str, segment: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == segment,
        Some((head, tail)) => {
            let Some(rest) = segment.strip_prefix(head) else {
                return false;
            };
            (0..=rest.len())
                .filter(|start| rest.is_char_boundary(*start))
                .any(|start| within_segment(tail, &rest[start..]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(dialect: &Dialect, relative: &str) -> (Role, String) {
        let classified = classify(dialect, relative);
        (classified.role, classified.name)
    }

    #[test]
    fn a_literal_pattern_names_the_file_by_its_stem() {
        assert_eq!(
            named(&OPENSPEC, "proposal.md"),
            (Role::Why, "proposal".to_owned())
        );
        assert_eq!(
            named(&OPENSPEC, "tasks.md"),
            (Role::Steps, "tasks".to_owned())
        );
    }

    #[test]
    fn a_double_wildcard_names_the_file_by_what_it_stood_for() {
        assert_eq!(
            named(&OPENSPEC, "specs/spec-surface/spec.md"),
            (Role::Contract, "spec-surface".to_owned())
        );
        assert_eq!(
            named(&OPENSPEC, "specs/identity/user-auth/spec.md"),
            (Role::Contract, "identity/user-auth".to_owned())
        );
    }

    #[test]
    fn a_double_wildcard_needs_at_least_one_segment() {
        assert!(!matches(
            &["specs", "**", "spec.md"],
            &["specs", "spec.md"],
            &mut Vec::new()
        ));
    }

    #[test]
    fn a_single_wildcard_stands_for_exactly_one_segment() {
        let mut captured = Vec::new();
        assert!(matches(&["*", "spec.md"], &["a", "spec.md"], &mut captured));
        assert_eq!(captured, ["a"]);
        assert!(!matches(
            &["*", "spec.md"],
            &["a", "b", "spec.md"],
            &mut Vec::new()
        ));
    }

    #[test]
    fn a_wildcard_inside_a_segment_matches_any_run_and_names_the_file_by_its_path() {
        assert_eq!(
            named(&GSD, "03-01-PLAN.md"),
            (Role::Steps, "03-01-PLAN".to_owned())
        );
        assert_eq!(
            named(&GSD, "01-foundation/01-02-PLAN.md"),
            (Role::Steps, "01-foundation/01-02-PLAN".to_owned())
        );
        assert!(within_segment("*PLAN.md", "PLAN.md"), "the empty run too");
        assert!(!within_segment("*PLAN.md", "ROADMAP.md"));
        assert!(!within_segment("*-design.md", "2026-09-14-design-notes.md"));
    }

    #[test]
    fn the_first_matching_pattern_wins() {
        // `spec.md` would match `**/spec.md` too; the literal comes first
        // and names it by its stem.
        assert_eq!(
            named(&OPENSPEC, "spec.md"),
            (Role::Contract, "spec".to_owned())
        );
    }

    #[test]
    fn a_file_no_pattern_names_is_other_and_keeps_its_path() {
        assert_eq!(
            named(&OPENSPEC, "adr/001-why.md"),
            (Role::Other, "adr/001-why".to_owned())
        );
        assert_eq!(
            named(&OPENSPEC, "notes.md"),
            (Role::Other, "notes".to_owned())
        );
    }

    #[test]
    fn roles_list_in_reading_order() {
        let mut roles = vec![
            Role::Other,
            Role::Contract,
            Role::Steps,
            Role::Why,
            Role::How,
        ];
        roles.sort();
        assert_eq!(
            roles,
            [
                Role::Why,
                Role::How,
                Role::Steps,
                Role::Contract,
                Role::Other
            ]
        );
    }
}
