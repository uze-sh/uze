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

/// One directory of units: every directory directly inside `path` that is
/// neither hidden nor in `skip` is one unit.
#[derive(Clone, Copy, Debug)]
pub struct Collection {
    pub subject: Subject,
    /// Relative to the checkout root.
    pub path: &'static str,
    pub skip: &'static [&'static str],
    pub order: Order,
}

/// One tool's layout.
#[derive(Clone, Copy, Debug)]
pub struct Dialect {
    pub name: &'static str,
    /// A directory at the checkout root that only this tool makes. Its
    /// presence is the whole of detection: the tool decides where its
    /// files live, so nothing about it is declared a second time.
    pub marker: &'static str,
    pub collections: &'static [Collection],
    /// Paths relative to a unit, first match wins; a file matching none is
    /// [`Role::Other`]. `*` stands for one path segment and `**` for one or
    /// more, and what they stood for names the file — `specs/**/spec.md`
    /// names `specs/agent/isolation/spec.md` as `agent/isolation`.
    pub roles: &'static [(&'static str, Role)],
}

pub const OPENSPEC: Dialect = Dialect {
    name: "OpenSpec",
    marker: "openspec",
    collections: &[
        Collection {
            subject: Subject::Changes,
            path: "openspec/changes",
            skip: &["archive"],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Specs,
            path: "openspec/specs",
            skip: &[],
            order: Order::Ascending,
        },
        Collection {
            subject: Subject::Archive,
            path: "openspec/changes/archive",
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
};

/// Every dialect this build reads, in the order a checkout holding more
/// than one lists them.
pub const SHIPPED: &[Dialect] = &[OPENSPEC];

/// What a file is for and what to call it, by its path inside its unit.
pub fn classify(dialect: &Dialect, relative: &str) -> (Role, String) {
    let segments: Vec<&str> = relative.split('/').collect();
    for (pattern, role) in dialect.roles {
        let pattern: Vec<&str> = pattern.split('/').collect();
        let mut captured = Vec::new();
        if matches(&pattern, &segments, &mut captured) {
            let name = if captured.is_empty() {
                stem(relative)
            } else {
                captured.join("/")
            };
            return (*role, name);
        }
    }
    (
        Role::Other,
        relative.strip_suffix(".md").unwrap_or(relative).to_owned(),
    )
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
            literal == segment && matches(rest, path_rest, captured)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_pattern_names_the_file_by_its_stem() {
        assert_eq!(
            classify(&OPENSPEC, "proposal.md"),
            (Role::Why, "proposal".to_owned())
        );
        assert_eq!(
            classify(&OPENSPEC, "tasks.md"),
            (Role::Steps, "tasks".to_owned())
        );
    }

    #[test]
    fn a_double_wildcard_names_the_file_by_what_it_stood_for() {
        assert_eq!(
            classify(&OPENSPEC, "specs/spec-surface/spec.md"),
            (Role::Contract, "spec-surface".to_owned())
        );
        assert_eq!(
            classify(&OPENSPEC, "specs/identity/user-auth/spec.md"),
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
    fn the_first_matching_pattern_wins() {
        // `spec.md` would match `**/spec.md` too; the literal comes first
        // and names it by its stem.
        assert_eq!(
            classify(&OPENSPEC, "spec.md"),
            (Role::Contract, "spec".to_owned())
        );
    }

    #[test]
    fn a_file_no_pattern_names_is_other_and_keeps_its_path() {
        assert_eq!(
            classify(&OPENSPEC, "adr/001-why.md"),
            (Role::Other, "adr/001-why".to_owned())
        );
        assert_eq!(
            classify(&OPENSPEC, "notes.md"),
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
