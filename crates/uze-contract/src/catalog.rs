//! The nodes a view may name, and what each one accepts.
//!
//! One table, read by the decoder that checks a view, the engine that binds
//! it and, later, the language server that completes it, so the three can
//! never disagree about what `Progress` takes. A node joins the catalog when
//! a gallery entry needs it (D15, D20); this is the starting set
//! (`extension-view-engine`).

use serde::{Deserialize, Serialize};

/// A catalog node, as a view spells it: PascalCase, unlike a view's
/// directives (`template`, `use`, `lookup`), which are lowercase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum NodeName {
    Stack,
    Row,
    Text,
    Badge,
    Marks,
    Mark,
    Pips,
    Progress,
    Segment,
    List,
    Table,
    Tabs,
    Tab,
    Toggle,
    Button,
    Activity,
    Elapsed,
    Log,
    Rule,
    Details,
    Entry,
    Markdown,
    Code,
    Gauge,
    Chart,
    Stat,
    Kbd,
    Grid,
}

/// What a property's value must be once bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropKind {
    /// Display text: a literal, a reference or mixed text.
    Text,
    /// A whole number of cells or a count.
    Count,
    /// A number, such as a weight or a ratio.
    Number,
    /// `#true` or `#false`, or a reference to one.
    Flag,
    /// One of [`RoleName`], or a reference or lookup resolving to one.
    Role,
    /// A literal the view declares and the host keys state or actions by.
    /// Never interpolated (D23): identity cannot depend on data.
    Identity,
    /// A literal naming one of a closed set of words.
    Word(&'static [&'static str]),
    /// Any value, handed through to whatever receives it (an action's item).
    Value,
}

/// What a node accepts: its arguments, its properties and whether it holds
/// other nodes.
#[derive(Clone, Copy, Debug)]
pub struct NodeSpec {
    pub name: NodeName,
    /// The kind of each positional argument, in order; more than this many
    /// is refused.
    pub args: &'static [PropKind],
    /// How many of [`NodeSpec::args`] must be present.
    pub required_args: usize,
    pub props: &'static [(&'static str, PropKind)],
    /// The nodes it may hold, or `None` for a leaf.
    pub children: Option<&'static [NodeName]>,
}

/// Properties every node accepts, beside its own: whether it is drawn, and
/// how it shares a row.
pub const COMMON_PROPS: &[(&str, PropKind)] = &[
    ("each", PropKind::Identity),
    ("when", PropKind::Identity),
    ("unless", PropKind::Identity),
    ("show", PropKind::Identity),
    ("show-any", PropKind::Identity),
    ("grow", PropKind::Flag),
    ("min-width", PropKind::Count),
];

const ALIGN: PropKind = PropKind::Word(&["start", "center", "end"]);

/// Any node a container may hold.
const ANY: &[NodeName] = &[
    NodeName::Stack,
    NodeName::Row,
    NodeName::Text,
    NodeName::Badge,
    NodeName::Marks,
    NodeName::Pips,
    NodeName::Progress,
    NodeName::List,
    NodeName::Table,
    NodeName::Tabs,
    NodeName::Toggle,
    NodeName::Button,
    NodeName::Activity,
    NodeName::Elapsed,
    NodeName::Log,
    NodeName::Rule,
    NodeName::Details,
    NodeName::Markdown,
    NodeName::Code,
    NodeName::Gauge,
    NodeName::Chart,
    NodeName::Stat,
    NodeName::Kbd,
    NodeName::Grid,
];

const CHART_KIND: PropKind = PropKind::Word(&["sparkline", "bar"]);

pub const CATALOG: &[NodeSpec] = &[
    NodeSpec {
        name: NodeName::Stack,
        args: &[],
        required_args: 0,
        props: &[("gap", PropKind::Count)],
        children: Some(ANY),
    },
    NodeSpec {
        name: NodeName::Row,
        args: &[],
        required_args: 0,
        props: &[("gap", PropKind::Count), ("align", ALIGN)],
        children: Some(ANY),
    },
    NodeSpec {
        name: NodeName::Text,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[
            ("role", PropKind::Role),
            ("strong", PropKind::Flag),
            ("truncate", PropKind::Flag),
        ],
        children: None,
    },
    NodeSpec {
        name: NodeName::Badge,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[("role", PropKind::Role)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Marks,
        args: &[],
        required_args: 0,
        props: &[],
        children: Some(&[NodeName::Mark]),
    },
    NodeSpec {
        name: NodeName::Mark,
        args: &[],
        required_args: 0,
        props: &[("role", PropKind::Role)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Pips,
        args: &[],
        required_args: 0,
        props: &[("done", PropKind::Count), ("total", PropKind::Count)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Progress,
        args: &[],
        required_args: 0,
        props: &[("ratio", PropKind::Number), ("role", PropKind::Role)],
        children: Some(&[NodeName::Segment]),
    },
    NodeSpec {
        name: NodeName::Segment,
        args: &[],
        required_args: 0,
        props: &[
            ("weight", PropKind::Number),
            ("ratio", PropKind::Number),
            ("role", PropKind::Role),
        ],
        children: None,
    },
    NodeSpec {
        name: NodeName::List,
        args: &[],
        required_args: 0,
        props: &[("id", PropKind::Identity), ("empty", PropKind::Text)],
        children: Some(ANY),
    },
    NodeSpec {
        name: NodeName::Table,
        args: &[
            PropKind::Text,
            PropKind::Text,
            PropKind::Text,
            PropKind::Text,
            PropKind::Text,
            PropKind::Text,
            PropKind::Text,
            PropKind::Text,
        ],
        required_args: 0,
        props: &[("empty", PropKind::Text)],
        children: Some(&[NodeName::Row]),
    },
    NodeSpec {
        name: NodeName::Tabs,
        args: &[],
        required_args: 0,
        props: &[("id", PropKind::Identity)],
        children: Some(&[NodeName::Tab]),
    },
    NodeSpec {
        name: NodeName::Tab,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[("value", PropKind::Identity)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Toggle,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[("id", PropKind::Identity), ("on", PropKind::Flag)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Button,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[("action", PropKind::Identity), ("item", PropKind::Value)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Activity,
        args: &[],
        required_args: 0,
        props: &[("agent", PropKind::Identity), ("busy", PropKind::Flag)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Elapsed,
        args: &[],
        required_args: 0,
        props: &[("since", PropKind::Text), ("role", PropKind::Role)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Log,
        args: &[],
        required_args: 0,
        props: &[("lines", PropKind::Value)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Rule,
        args: &[PropKind::Text],
        required_args: 0,
        props: &[("role", PropKind::Role)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Details,
        args: &[],
        required_args: 0,
        props: &[],
        children: Some(&[NodeName::Entry]),
    },
    NodeSpec {
        name: NodeName::Entry,
        args: &[PropKind::Text, PropKind::Text],
        required_args: 2,
        props: &[("role", PropKind::Role)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Markdown,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[],
        children: None,
    },
    NodeSpec {
        name: NodeName::Code,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[("language", PropKind::Text)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Gauge,
        args: &[PropKind::Text],
        required_args: 0,
        props: &[
            ("ratio", PropKind::Number),
            ("role", PropKind::Role),
            ("warning", PropKind::Number),
            ("danger", PropKind::Number),
        ],
        children: None,
    },
    NodeSpec {
        name: NodeName::Chart,
        args: &[],
        required_args: 0,
        props: &[
            ("series", PropKind::Value),
            ("kind", CHART_KIND),
            ("height", PropKind::Count),
            ("role", PropKind::Role),
        ],
        children: None,
    },
    NodeSpec {
        name: NodeName::Stat,
        args: &[PropKind::Text, PropKind::Text],
        required_args: 2,
        props: &[("delta", PropKind::Text), ("role", PropKind::Role)],
        children: None,
    },
    NodeSpec {
        name: NodeName::Kbd,
        args: &[PropKind::Text],
        required_args: 1,
        props: &[],
        children: None,
    },
    NodeSpec {
        name: NodeName::Grid,
        args: &[],
        required_args: 0,
        props: &[("gap", PropKind::Count)],
        children: Some(ANY),
    },
];

impl NodeName {
    pub fn spec(self) -> &'static NodeSpec {
        CATALOG
            .iter()
            .find(|spec| spec.name == self)
            .expect("every node name has a catalog entry, proved by a test")
    }

    /// The name as a view writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stack => "Stack",
            Self::Row => "Row",
            Self::Text => "Text",
            Self::Badge => "Badge",
            Self::Marks => "Marks",
            Self::Mark => "Mark",
            Self::Pips => "Pips",
            Self::Progress => "Progress",
            Self::Segment => "Segment",
            Self::List => "List",
            Self::Table => "Table",
            Self::Tabs => "Tabs",
            Self::Tab => "Tab",
            Self::Toggle => "Toggle",
            Self::Button => "Button",
            Self::Activity => "Activity",
            Self::Elapsed => "Elapsed",
            Self::Log => "Log",
            Self::Rule => "Rule",
            Self::Details => "Details",
            Self::Entry => "Entry",
            Self::Markdown => "Markdown",
            Self::Code => "Code",
            Self::Gauge => "Gauge",
            Self::Chart => "Chart",
            Self::Stat => "Stat",
            Self::Kbd => "Kbd",
            Self::Grid => "Grid",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        CATALOG
            .iter()
            .map(|spec| spec.name)
            .find(|node| node.as_str() == name)
    }

    /// A catalog node written in the wrong case, for the check's
    /// suggestion: `stack` → `Stack`.
    pub fn parse_ignoring_case(name: &str) -> Option<Self> {
        CATALOG
            .iter()
            .map(|spec| spec.name)
            .find(|node| node.as_str().eq_ignore_ascii_case(name))
    }

    /// The kind of `prop` on this node, its own or one every node takes.
    pub fn prop(self, prop: &str) -> Option<PropKind> {
        self.spec()
            .props
            .iter()
            .chain(COMMON_PROPS)
            .find(|(name, _)| *name == prop)
            .map(|(_, kind)| *kind)
    }
}

impl NodeName {
    /// What the node draws, in one line: what hover shows and completion
    /// describes, so an editor and the reference say the same thing.
    pub fn doc(self) -> &'static str {
        match self {
            Self::Stack => "Its children one under another, `gap` rows apart.",
            Self::Row => {
                "Its children side by side, `gap` columns apart; `grow` children share what is left, the rest are placed by `align`."
            }
            Self::Text => {
                "Text in a role, `strong` for emphasis; with `truncate` one line cut with an ellipsis, otherwise a paragraph that wraps."
            }
            Self::Badge => "A short label on a ground of its role's colour.",
            Self::Marks => "Role-coloured marks in a row, one per `Mark`.",
            Self::Mark => "One mark of a `Marks`, in its role's colour.",
            Self::Pips => "`done` of `total` as a row of filled and empty pips.",
            Self::Progress => "A bar: one fill at `ratio`, or one segment per `Segment`.",
            Self::Segment => {
                "One segment of a `Progress`: as wide as its `weight`, filled to its `ratio`, in its role."
            }
            Self::List => {
                "Rows repeated over `each`; with an `id`, the host holds which one is selected."
            }
            Self::Table => {
                "Named columns, one argument per heading; each child `Row` is a line whose children are its cells."
            }
            Self::Tabs => {
                "A choice the host holds as `ui.<id>`; `show=\"ui.<id>=<value>\"` picks what is drawn under it."
            }
            Self::Tab => "One choice of a `Tabs`: its label, and the `value` `show` compares with.",
            Self::Toggle => "A switch the host holds as `ui.<id>`, `on` at first when given.",
            Self::Button => "A label that records its `action` with its `item` when pressed.",
            Self::Activity => "A mark the host animates while `busy`.",
            Self::Elapsed => "The time since `since`, ticked by the host.",
            Self::Log => "Lines, each a string or an object with `text` and a `role`.",
            Self::Rule => "A hairline across the width, with its label when given.",
            Self::Details => {
                "Key/value pairs, one `Entry` each, with the keys in one aligned column."
            }
            Self::Entry => "One pair of a `Details`: its key, then its value, in its role.",
            Self::Markdown => {
                "Text written in Markdown: headings, lists, quotes, emphasis and fenced code, wrapped."
            }
            Self::Code => {
                "A block of code highlighted for its `language`, one row per line, cut rather than wrapped."
            }
            Self::Gauge => {
                "A labelled meter at `ratio`, in `role` until it reaches `warning` or `danger`."
            }
            Self::Chart => {
                "A list of numbers (`series`) as one `sparkline` row or as `bar`s `height` rows tall."
            }
            Self::Stat => "A figure and its label, with a `delta` in its role when given.",
            Self::Kbd => "A key, drawn as the workspace draws its own keys.",
            Self::Grid => {
                "Its children as tiles, in as many columns as fit their `min-width`, row by row."
            }
        }
    }
}

impl std::fmt::Display for NodeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The semantic colours a view may name: the host's roles (`view::Role` in
/// `uze-extensions`), spelled in lowercase. No colour value is a role, which
/// is how `role="#ff0000"` is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RoleName {
    Default,
    Muted,
    Secondary,
    Bright,
    Inactive,
    Accent,
    Dim,
    Faint,
    Info,
    Success,
    Warning,
    Danger,
}

impl RoleName {
    pub const ALL: &'static [RoleName] = &[
        Self::Default,
        Self::Muted,
        Self::Secondary,
        Self::Bright,
        Self::Inactive,
        Self::Accent,
        Self::Dim,
        Self::Faint,
        Self::Info,
        Self::Success,
        Self::Warning,
        Self::Danger,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Muted => "muted",
            Self::Secondary => "secondary",
            Self::Bright => "bright",
            Self::Inactive => "inactive",
            Self::Accent => "accent",
            Self::Dim => "dim",
            Self::Faint => "faint",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|role| role.as_str() == name)
    }

    /// When to reach for the role, in one line.
    pub fn doc(self) -> &'static str {
        match self {
            Self::Default => "Body text.",
            Self::Muted => "Supporting text a reader skims past.",
            Self::Secondary => "Labels and headings of a group.",
            Self::Bright => "What the eye should land on first.",
            Self::Inactive => "Something switched off or not reachable.",
            Self::Accent => "The extension's one point of emphasis.",
            Self::Dim => "Quieter than muted, still readable.",
            Self::Faint => "Hints, placeholders and empty states.",
            Self::Info => "Neutral status: a version, a count, a note.",
            Self::Success => "Done, passing, healthy.",
            Self::Warning => "In progress, waiting, needs attention.",
            Self::Danger => "Failed, blocked, destructive.",
        }
    }
}

/// What a property does, in one line, for the properties every node takes
/// and the ones a node of the catalog names.
pub fn prop_doc(prop: &str) -> Option<&'static str> {
    Some(match prop {
        "each" => "Repeat this node once per item of a list field.",
        "when" => "Draw only while the field is present: not missing, null, false, empty or zero.",
        "unless" => "Draw only while the field is absent.",
        "show" => "Draw only while a UI value equals a declared literal: `ui.<id>=<value>`.",
        "show-any" => "Draw while any of several `ui.<id>=<value>` pairs, comma-separated, holds.",
        "grow" => "Take a share of what is left of the row.",
        "min-width" => "Never draw narrower than this many columns.",
        "gap" => "Rows (in a `Stack`) or columns (in a `Row`) between children.",
        "align" => "Where a row with nothing that grows sits: `start`, `center` or `end`.",
        "role" => "The semantic colour, or `{table:field}` to look one up.",
        "strong" => "Emphasis.",
        "truncate" => "One line, cut with an ellipsis, instead of a paragraph that wraps.",
        "done" => "How many pips are filled.",
        "total" => "How many pips there are.",
        "ratio" => "How full, from 0 to 1.",
        "weight" => "How wide, relative to the other segments.",
        "id" => "The name the host keeps this node's state under, as `ui.<id>`.",
        "empty" => "What to draw when there are no rows.",
        "value" => "What `show` compares the tab's choice with.",
        "on" => "Whether the switch starts on.",
        "action" => "The action a press records, by name.",
        "item" => "What the action receives: a field, or `{.}` for the whole item.",
        "agent" => "The agent whose activity this mark follows.",
        "busy" => "Whether the mark is animated.",
        "since" => "A timestamp: milliseconds, or an RFC 3339 date.",
        "lines" => "A list field of lines, or one string of several.",
        "language" => "The language the code is highlighted for, such as `rust` or `json`.",
        "warning" => "The ratio from which the gauge is drawn in the `warning` role.",
        "danger" => "The ratio from which the gauge is drawn in the `danger` role.",
        "series" => "A list field of numbers.",
        "kind" => "`sparkline` (one row) or `bar` (columns `height` rows tall).",
        "height" => "How many rows the bars take.",
        "delta" => "A change beside the figure, such as `+12%`, in `role`.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_has_one_catalog_entry_and_round_trips_its_name() {
        for spec in CATALOG {
            assert_eq!(NodeName::parse(spec.name.as_str()), Some(spec.name));
            assert_eq!(
                CATALOG
                    .iter()
                    .filter(|other| other.name == spec.name)
                    .count(),
                1
            );
            assert!(spec.required_args <= spec.args.len());
        }
    }

    #[test]
    fn every_node_role_and_property_is_documented() {
        for spec in CATALOG {
            assert!(!spec.name.doc().is_empty());
            for (prop, _) in spec.props.iter().chain(COMMON_PROPS) {
                assert!(prop_doc(prop).is_some(), "{} {prop}", spec.name);
            }
        }
        for role in RoleName::ALL {
            assert!(!role.doc().is_empty());
        }
    }

    #[test]
    fn the_wrong_case_is_found_for_a_suggestion_but_not_accepted() {
        assert_eq!(NodeName::parse("stack"), None);
        assert_eq!(
            NodeName::parse_ignoring_case("stack"),
            Some(NodeName::Stack)
        );
    }

    #[test]
    fn identity_properties_are_never_text() {
        assert_eq!(NodeName::Button.prop("action"), Some(PropKind::Identity));
        assert_eq!(NodeName::Tabs.prop("id"), Some(PropKind::Identity));
        assert_eq!(NodeName::Text.prop("each"), Some(PropKind::Identity));
        assert_eq!(NodeName::Text.prop("colour"), None);
    }

    #[test]
    fn a_colour_value_is_not_a_role() {
        assert_eq!(RoleName::parse("warning"), Some(RoleName::Warning));
        assert_eq!(RoleName::parse("#ff0000"), None);
        assert_eq!(RoleName::ALL.len(), 12);
    }
}
