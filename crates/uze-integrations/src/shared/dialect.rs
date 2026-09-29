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
    Number,
    Map,
    /// Text or a list of text.
    TextOrList,
    OneOf(&'static [&'static str]),
    /// `provider/model`.
    Qualified,
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
    let mut carried = serde_yaml::Mapping::new();
    let mut findings = Findings::default();
    for (key, value) in harness::fields_for(&document.frontmatter, keys) {
        let field = key.as_str();
        if harness::IDENTITY_FIELDS.contains(&field) {
            continue;
        }
        match dialect.known.iter().find(|(known, _)| *known == field) {
            Some((_, shape)) => match conforms(*shape, &value) {
                Ok(()) => {
                    carried.insert(key, value);
                }
                Err(expected) => findings.errors.push(format!(
                    "`harness.{}.{field}` must be {expected}; the harness would not load the \
                     agent with it",
                    keys[0]
                )),
            },
            None if dialect.carries_unknown => {
                findings.warnings.push(format!(
                    "`harness.{}.{field}` is not a field UZE has verified on this harness; it \
                     is delivered as written",
                    keys[0]
                ));
                carried.insert(key, value);
            }
            None => findings.warnings.push(format!(
                "`harness.{}.{field}` is not a field this harness reads, and it refuses an \
                 agent carrying one; it is left out",
                keys[0]
            )),
        }
    }
    (carried, findings)
}

fn conforms(shape: Shape, value: &serde_yaml::Value) -> Result<(), String> {
    let text_list = |value: &serde_yaml::Value| {
        value
            .as_sequence()
            .is_some_and(|items| items.iter().all(|item| item.as_str().is_some()))
    };
    let ok = match shape {
        Shape::Text => value.as_str().is_some_and(|text| !text.trim().is_empty()),
        Shape::Number => value.as_f64().is_some() || value.as_i64().is_some(),
        Shape::Map => value.as_mapping().is_some(),
        Shape::TextOrList => value.as_str().is_some() || text_list(value),
        Shape::OneOf(choices) => value.as_str().is_some_and(|text| choices.contains(&text)),
        Shape::Qualified => value
            .as_str()
            .and_then(|text| text.split_once('/'))
            .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty()),
    };
    if ok {
        return Ok(());
    }
    Err(match shape {
        Shape::Text => "text".to_owned(),
        Shape::Number => "a number".to_owned(),
        Shape::Map => "a mapping".to_owned(),
        Shape::TextOrList => "text or a list of text".to_owned(),
        Shape::OneOf(choices) => format!("one of {}", choices.join(", ")),
        Shape::Qualified => "`provider/model`".to_owned(),
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
            ("tools", Shape::Map),
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
    fn a_wrong_shape_is_an_error_and_is_not_carried() {
        let (carried, findings) = agent_block(
            &DIALECT,
            &["opencode"],
            &document(
                "name: a\ndescription: d\nharness:\n  opencode: { model: haiku, tools: Read }",
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
