//! What one harness accepts under its `harness.<id>` block, and what it does
//! with a field it does not know.
//!
//! Each integration declares its agent dialect as data: the fields it knows,
//! each with the shape it must have, and whether the harness tolerates a
//! field it does not know (it is carried, unverified) or refuses one (it is
//! left out). The same declaration renders the delivered file, answers
//! `plugin check` and states the loss at install, so the three never
//! disagree. A value of the wrong shape for a field the harness is known to
//! read is an error: the author wrote it for that harness, and the harness
//! would drop the agent over it.
//!
//! An agent is authored by a plugin or a project, never by the operator, so
//! nothing it declares may widen what the harness lets it do beyond what
//! the operator granted: a field that would is withheld or lowered, and the
//! delivery says so ([`withheld`]). Claude Code holds a plugin's own agents
//! to the same rule.

use std::borrow::Cow;
use std::path::Path;

use uze_core::capability::{
    agent::AgentDocument,
    harness::{self, Findings, yaml as serde_yaml},
};

use crate::shared::package_root::resolve_bytes;

/// The shape a known field's value must have.
#[derive(Clone, Copy)]
pub(crate) enum Shape {
    Text,
    /// Text or a list of text.
    TextOrList,
    OneOf(&'static [&'static str]),
    /// `provider/model`.
    Qualified,
    /// `true` or `false`.
    Flag,
    /// A positive whole number.
    Count,
    /// `#rrggbb`.
    HexColor,
    /// An ordered list of `{action, resource, effect}` rules, `effect` one of
    /// allow, deny, ask (OpenCode V2's `permissions`). Only the rules that
    /// narrow are carried: an `allow` would grant the agent what the
    /// operator's own configuration asks about or denies.
    Rules,
    /// One of `levels`, ordered from the narrowest; a value above `ceiling`
    /// is lowered to it.
    Ceiling {
        levels: &'static [&'static str],
        ceiling: &'static str,
    },
    /// Widens what the harness lets the agent do; an authored agent never
    /// grants it to itself, so it is left out whatever the value.
    Withheld(&'static str),
    /// Read by the harness in a way that loses the agent: whatever the value,
    /// the harness drops an agent carrying the field. It is left out, and
    /// the reason is the warning.
    Refused(&'static str),
}

pub(crate) struct AgentDialect {
    pub(crate) known: &'static [(&'static str, Shape)],
    /// Whether the harness loads an agent carrying a field it does not know.
    pub(crate) carries_unknown: bool,
}

/// The fields `keys` are given for `document`, as this harness will write
/// them, and what `plugin check` says about the rest.
pub(crate) fn agent_block(
    dialect: &AgentDialect,
    keys: &[&str],
    document: &AgentDocument,
) -> (serde_yaml::Mapping, Findings) {
    let block = read_block(dialect, keys, document);
    (block.carried, block.findings)
}

/// What the harness block would have granted the agent beyond what the
/// operator did, each named as `harness.<key>.<field>` with what was taken
/// from it: the loss a delivery reports as Degraded.
pub(crate) fn withheld(
    dialect: &AgentDialect,
    keys: &[&str],
    document: &AgentDocument,
) -> Vec<String> {
    read_block(dialect, keys, document).withheld
}

struct Block {
    carried: serde_yaml::Mapping,
    findings: Findings,
    withheld: Vec<String>,
}

fn read_block(dialect: &AgentDialect, keys: &[&str], document: &AgentDocument) -> Block {
    let mut carried = serde_yaml::Mapping::new();
    let mut findings = Findings::default();
    let mut withheld = Vec::new();
    for (key, value) in harness::fields_for(&document.frontmatter, keys) {
        let field = key.as_str();
        if harness::IDENTITY_FIELDS.contains(&field) {
            continue;
        }
        let path = format!("harness.{}.{field}", keys[0]);
        match dialect.known.iter().find(|(known, _)| *known == field) {
            Some((_, Shape::Refused(reason))) => findings
                .warnings
                .push(format!("`{path}` is left out: {reason}")),
            Some((_, Shape::Withheld(reason))) => {
                findings
                    .warnings
                    .push(format!("`{path}` is left out: {reason}"));
                withheld.push(path);
            }
            Some((_, shape)) => match conforms(*shape, &value) {
                Ok(()) => match narrowed(*shape, &value) {
                    Some((narrowed, taken)) => {
                        findings.warnings.push(format!(
                            "`{path}` is narrowed: {taken}, which an agent cannot grant itself"
                        ));
                        withheld.push(format!("{path} ({taken})"));
                        if let Some(narrowed) = narrowed {
                            carried.insert(key, narrowed);
                        }
                    }
                    None => {
                        carried.insert(key, value);
                    }
                },
                Err(expected) => findings.errors.push(format!(
                    "`{path}` must be {expected}; the harness would not load the agent with it"
                )),
            },
            None if dialect.carries_unknown => {
                findings.warnings.push(format!(
                    "`{path}` is not a field UZE has verified on this harness; it is delivered \
                     as written"
                ));
                carried.insert(key, value);
            }
            None => findings.warnings.push(format!(
                "`{path}` is not a field this harness reads, and it refuses an agent carrying \
                 one; it is left out"
            )),
        }
    }
    Block {
        carried,
        findings,
        withheld,
    }
}

/// A conforming value lowered to what an authored agent may hold, and what
/// was taken from it; `None` when it already may. The lowered value is
/// itself `None` when nothing of it is left to carry.
fn narrowed(
    shape: Shape,
    value: &serde_yaml::Value,
) -> Option<(Option<serde_yaml::Value>, String)> {
    match shape {
        Shape::Ceiling { levels, ceiling } => {
            let asked = value.as_str()?;
            let rank = |level: &str| levels.iter().position(|known| *known == level);
            (rank(asked)? > rank(ceiling)?).then(|| {
                (
                    Some(serde_yaml::Value::String(ceiling.to_owned())),
                    format!("`{asked}` is lowered to `{ceiling}`"),
                )
            })
        }
        Shape::Rules => {
            let rules = value.as_sequence()?;
            let narrowing: Vec<serde_yaml::Value> = rules
                .iter()
                .filter(|rule| {
                    rule.get("effect").and_then(serde_yaml::Value::as_str) != Some("allow")
                })
                .cloned()
                .collect();
            let dropped = rules.len() - narrowing.len();
            (dropped > 0).then(|| {
                (
                    (!narrowing.is_empty()).then(|| serde_yaml::Value::Sequence(narrowing)),
                    format!(
                        "{dropped} `allow` rule{} left out",
                        if dropped == 1 { " is" } else { "s are" }
                    ),
                )
            })
        }
        _ => None,
    }
}

fn conforms(shape: Shape, value: &serde_yaml::Value) -> Result<(), String> {
    let text_list = |value: &serde_yaml::Value| {
        value
            .as_sequence()
            .is_some_and(|items| items.iter().all(|item| item.as_str().is_some()))
    };
    let ok = match shape {
        Shape::Text => value.as_str().is_some_and(|text| !text.trim().is_empty()),
        Shape::TextOrList => value.as_str().is_some() || text_list(value),
        Shape::OneOf(choices)
        | Shape::Ceiling {
            levels: choices, ..
        } => value.as_str().is_some_and(|text| choices.contains(&text)),
        Shape::Qualified => value
            .as_str()
            .and_then(|text| text.split_once('/'))
            .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty()),
        Shape::Flag => value.as_bool().is_some(),
        Shape::Count => value.as_u64().is_some_and(|count| count > 0),
        Shape::HexColor => value.as_str().is_some_and(|text| {
            text.len() == 7
                && text.starts_with('#')
                && text[1..].chars().all(|digit| digit.is_ascii_hexdigit())
        }),
        Shape::Rules => value.as_sequence().is_some_and(|rules| {
            rules.iter().all(|rule| {
                let field = |name: &str| rule.get(name).and_then(serde_yaml::Value::as_str);
                field("action").is_some()
                    && field("resource").is_some()
                    && field("effect")
                        .is_some_and(|effect| ["allow", "deny", "ask"].contains(&effect))
            })
        }),
        Shape::Refused(_) | Shape::Withheld(_) => false,
    };
    if ok {
        return Ok(());
    }
    Err(match shape {
        Shape::Text => "text".to_owned(),
        Shape::TextOrList => "text or a list of text".to_owned(),
        Shape::OneOf(choices)
        | Shape::Ceiling {
            levels: choices, ..
        } => {
            format!("one of {}", choices.join(", "))
        }
        Shape::Qualified => "`provider/model`".to_owned(),
        Shape::Flag => "true or false".to_owned(),
        Shape::Count => "a positive whole number".to_owned(),
        Shape::HexColor => "a `#rrggbb` color".to_owned(),
        Shape::Rules => "a list of `{action, resource, effect}` rules".to_owned(),
        Shape::Refused(reason) | Shape::Withheld(reason) => reason.to_owned(),
    })
}

/// A `SKILL.md` as this harness receives it: the package root resolved,
/// the `harness:` block taken out and this harness's fields merged in.
pub(crate) fn delivered_skill<'a>(
    payload: &'a [u8],
    package_root: &Path,
    keys: &[&str],
) -> Cow<'a, [u8]> {
    let resolved = resolve_bytes(payload, package_root);
    match harness::skill_for(&resolved, keys) {
        Cow::Borrowed(_) => resolved,
        Cow::Owned(rendered) => Cow::Owned(rendered),
    }
}

/// This harness's `harness:` fields as frontmatter lines, for a delivery
/// that writes its own `SKILL.md` from the canonical name, description and
/// body.
pub(crate) fn skill_block_lines(payload: &[u8], keys: &[&str]) -> Vec<String> {
    let Some((frontmatter, _)) = std::str::from_utf8(payload)
        .ok()
        .and_then(harness::frontmatter_of)
    else {
        return Vec::new();
    };
    let mut fields = harness::fields_for(&frontmatter, keys);
    for identity in harness::IDENTITY_FIELDS {
        fields.remove(identity);
    }
    if fields.is_empty() {
        return Vec::new();
    }
    serde_yaml::to_string(&serde_yaml::Value::Mapping(fields))
        .expect("a mapping of parsed YAML values serializes")
        .lines()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALECT: AgentDialect = AgentDialect {
        known: &[
            ("model", Shape::Qualified),
            ("permissions", Shape::Rules),
            ("effort", Shape::OneOf(&["low", "high"])),
        ],
        carries_unknown: false,
    };

    fn document(yaml: &str) -> AgentDocument {
        AgentDocument::parse(format!("---\n{yaml}\n---\nbody\n").as_bytes()).unwrap()
    }

    #[test]
    fn known_fields_are_carried_and_checked() {
        let (carried, findings) = agent_block(
            &DIALECT,
            &["opencode"],
            &document(
                "name: a\ndescription: d\nharness:\n  opencode: { model: anthropic/haiku, effort: high }",
            ),
        );
        assert_eq!(carried.len(), 2);
        assert!(findings.errors.is_empty() && findings.warnings.is_empty());
    }

    #[test]
    fn the_v2_shapes_accept_what_opencode_reads_and_nothing_else() {
        let yaml = |value: &str| serde_yaml::from_str::<serde_yaml::Value>(value).unwrap();
        assert!(conforms(Shape::Flag, &yaml("true")).is_ok());
        assert!(conforms(Shape::Flag, &yaml("yes please")).is_err());
        assert!(conforms(Shape::Count, &yaml("3")).is_ok());
        assert!(conforms(Shape::Count, &yaml("0")).is_err());
        assert!(conforms(Shape::HexColor, &yaml("'#a1b2c3'")).is_ok());
        assert!(conforms(Shape::HexColor, &yaml("teal")).is_err());
        assert!(
            conforms(
                Shape::Rules,
                &yaml("[{action: shell, resource: '*', effect: deny}]")
            )
            .is_ok()
        );
        assert!(
            conforms(
                Shape::Rules,
                &yaml("[{action: shell, resource: '*', effect: maybe}]")
            )
            .is_err()
        );
    }

    #[test]
    fn a_wrong_shape_is_an_error_and_is_not_carried() {
        let (carried, findings) = agent_block(
            &DIALECT,
            &["opencode"],
            &document(
                "name: a\ndescription: d\nharness:\n  opencode: { model: haiku, permissions: Read }",
            ),
        );
        assert!(carried.is_empty());
        assert_eq!(findings.errors.len(), 2, "{findings:?}");
        assert!(findings.errors[0].contains("provider/model"));
    }

    #[test]
    fn an_unknown_field_is_left_out_by_a_strict_harness_and_carried_by_a_tolerant_one() {
        let doc = document("name: a\ndescription: d\nharness:\n  codex: { nickname: x }");
        let (carried, findings) = agent_block(&DIALECT, &["codex"], &doc);
        assert!(carried.is_empty());
        assert!(findings.warnings[0].contains("left out"));
        let tolerant = AgentDialect {
            carries_unknown: true,
            ..DIALECT
        };
        let (carried, findings) = agent_block(&tolerant, &["codex"], &doc);
        assert_eq!(carried.len(), 1);
        assert!(findings.warnings[0].contains("delivered as written"));
    }

    #[test]
    fn a_refused_field_is_left_out_with_its_reason() {
        let dialect = AgentDialect {
            known: &[("model", Shape::Refused("the harness drops the agent"))],
            carries_unknown: true,
        };
        let (carried, findings) = agent_block(
            &dialect,
            &["agy"],
            &document("name: a\ndescription: d\nharness:\n  agy: { model: gemini }"),
        );
        assert!(carried.is_empty());
        assert!(findings.errors.is_empty());
        assert!(findings.warnings[0].contains("the harness drops the agent"));
    }

    #[test]
    fn nothing_an_agent_declares_widens_what_the_harness_lets_it_do() {
        let dialect = AgentDialect {
            known: &[
                ("mode", Shape::Withheld("an agent cannot grant itself this")),
                (
                    "sandbox",
                    Shape::Ceiling {
                        levels: &["read-only", "workspace-write", "full"],
                        ceiling: "workspace-write",
                    },
                ),
                ("permissions", Shape::Rules),
            ],
            carries_unknown: false,
        };
        let doc = document(
            "name: a\ndescription: d\nharness:\n  x:\n    mode: bypass\n    sandbox: full\n    \
             permissions: [{action: shell, resource: '*', effect: allow}, {action: edit, \
             resource: '*', effect: deny}]",
        );
        let (carried, findings) = agent_block(&dialect, &["x"], &doc);
        assert!(findings.errors.is_empty(), "{findings:?}");
        assert!(!carried.contains_key("mode"));
        assert_eq!(carried["sandbox"].as_str(), Some("workspace-write"));
        let rules = carried["permissions"].as_sequence().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["effect"].as_str(), Some("deny"));
        assert_eq!(
            withheld(&dialect, &["x"], &doc),
            [
                "harness.x.mode",
                "harness.x.sandbox (`full` is lowered to `workspace-write`)",
                "harness.x.permissions (1 `allow` rule is left out)",
            ]
        );

        let within = document(
            "name: a\ndescription: d\nharness:\n  x: { sandbox: read-only, permissions: \
             [{action: shell, resource: '*', effect: ask}] }",
        );
        assert!(withheld(&dialect, &["x"], &within).is_empty());
        assert_eq!(agent_block(&dialect, &["x"], &within).0.len(), 2);
    }

    #[test]
    fn skill_block_lines_are_the_harness_fields_only() {
        let payload =
            b"---\nname: deploy\nharness:\n  opencode: { temperature: 0.1, name: x }\n---\nbody\n";
        assert_eq!(
            skill_block_lines(payload, &["opencode"]),
            vec!["temperature: 0.1".to_owned()]
        );
        assert!(skill_block_lines(payload, &["codex"]).is_empty());
    }
}
