//! The `harness:` block: what an author says to one harness only.
//!
//! A skill or agent's frontmatter root is portable: every harness reads
//! `name`, `description` and the invocation policy with one meaning. What
//! differs (a model, a tool list, a reasoning effort) is written under
//! `harness.<harness id>`, and the integration for that harness renders it
//! into its own format; the block itself is never delivered. The Core knows
//! the block's shape and the root's rules, and nothing about any harness:
//! which keys a harness honours is its integration's knowledge.
//!
//! ```yaml
//! name: security
//! description: Reviews the diff for security flaws
//! harness:
//!   claude-code: { model: haiku, tools: [Read, Grep] }
//!   opencode: { model: anthropic/claude-haiku-4-5 }
//! ```

use std::borrow::Cow;

use noyalib::{ParserConfig, compat::serde_yaml, from_str_with_config};

use crate::skill::split_frontmatter;

/// The YAML model frontmatter is read into, for an integration that
/// renders a block without taking a YAML dependency of its own.
pub use noyalib::compat::serde_yaml as yaml;

/// The frontmatter key the block lives under.
pub const BLOCK: &str = "harness";

/// Root fields every harness reads with one meaning. A block that sets one
/// would give the same capability two identities, so it may not.
pub const IDENTITY_FIELDS: &[&str] = &["name", "description", "invoke"];

/// Root fields each harness spells differently. At the root they reach
/// only the harness whose spelling they happen to be (a Claude `model:
/// haiku` makes OpenCode drop the agent), so they belong in the block.
pub const PER_HARNESS_FIELDS: &[&str] = &["model", "tools"];

/// The block, by harness key, as authored; empty when there is none.
pub fn blocks(frontmatter: &serde_yaml::Mapping) -> Result<serde_yaml::Mapping, String> {
    match frontmatter.get(BLOCK) {
        None | Some(serde_yaml::Value::Null) => Ok(serde_yaml::Mapping::new()),
        Some(serde_yaml::Value::Mapping(blocks)) => {
            for (key, value) in blocks {
                if !matches!(
                    value,
                    serde_yaml::Value::Mapping(_) | serde_yaml::Value::Null
                ) {
                    return Err(format!(
                        "`{BLOCK}.{key}` is not a mapping — each harness takes its own fields, \
                         e.g. `{BLOCK}: {{ {key}: {{ model: … }} }}`"
                    ));
                }
            }
            Ok(blocks.clone())
        }
        Some(_) => Err(format!(
            "`{BLOCK}` is not a mapping — it maps a harness id to that harness's fields"
        )),
    }
}

/// The fields `keys` (a harness's id and aliases) are given, in authored
/// order; the first key present wins.
pub fn fields_for(frontmatter: &serde_yaml::Mapping, keys: &[&str]) -> serde_yaml::Mapping {
    let Ok(blocks) = blocks(frontmatter) else {
        return serde_yaml::Mapping::new();
    };
    keys.iter()
        .find_map(|key| match blocks.get(key) {
            Some(serde_yaml::Value::Mapping(fields)) => Some(fields.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// What the root and the block say, before any harness is asked: a block
/// that is not shaped as one, or that redefines an identity field, is an
/// error; a per-harness field at the root is a warning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Findings {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn common_findings(frontmatter: &serde_yaml::Mapping) -> Findings {
    let mut findings = Findings::default();
    match blocks(frontmatter) {
        Err(error) => findings.errors.push(error),
        Ok(blocks) => {
            for (harness, fields) in &blocks {
                let serde_yaml::Value::Mapping(fields) = fields else {
                    continue;
                };
                for field in IDENTITY_FIELDS {
                    if fields.contains_key(field) {
                        findings.errors.push(format!(
                            "`{BLOCK}.{harness}.{field}` redefines a field every harness reads \
                             with one meaning; keep `{field}` at the root only"
                        ));
                    }
                }
            }
        }
    }
    for field in PER_HARNESS_FIELDS {
        if frontmatter.contains_key(field) {
            findings.warnings.push(format!(
                "`{field}` at the root is spelled differently by each harness and is left out \
                 wherever it does not fit; write it under `{BLOCK}.<harness>`"
            ));
        }
    }
    findings
}

/// The frontmatter of `text` as a mapping, when it parses as one.
pub fn frontmatter_of(text: &str) -> Option<(serde_yaml::Mapping, &str)> {
    let (head, body) = split_frontmatter(text)?;
    match from_str_with_config::<serde_yaml::Value>(head, &ParserConfig::serde_yaml_compat()) {
        Ok(serde_yaml::Value::Mapping(mapping)) => Some((mapping, body)),
        Ok(serde_yaml::Value::Null) => Some((serde_yaml::Mapping::new(), body)),
        _ => None,
    }
}

/// A `SKILL.md` as one harness receives it: the block taken out and the
/// fields `keys` are given merged into the root, identity fields left
/// alone. Bytes with no block are returned untouched, so a skill that says
/// nothing per harness is still delivered byte for byte.
pub fn skill_for<'a>(payload: &'a [u8], keys: &[&str]) -> Cow<'a, [u8]> {
    let Ok(text) = std::str::from_utf8(payload) else {
        return Cow::Borrowed(payload);
    };
    let Some((mut frontmatter, body)) = frontmatter_of(text) else {
        return Cow::Borrowed(payload);
    };
    if !frontmatter.contains_key(BLOCK) {
        return Cow::Borrowed(payload);
    }
    let fields = fields_for(&frontmatter, keys);
    frontmatter.remove(BLOCK);
    for (key, value) in fields {
        if !IDENTITY_FIELDS.contains(&key.as_str()) {
            frontmatter.insert(key, value);
        }
    }
    Cow::Owned(render(&frontmatter, body).into_bytes())
}

/// A frontmatter block and a body as Markdown.
pub fn render(frontmatter: &serde_yaml::Mapping, body: &str) -> String {
    let head = serde_yaml::to_string(&serde_yaml::Value::Mapping(frontmatter.clone()))
        .expect("a mapping of parsed YAML values serializes");
    format!("---\n{}\n---\n{body}", head.trim_end_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(yaml: &str) -> serde_yaml::Mapping {
        frontmatter_of(&format!("---\n{yaml}\n---\nbody\n"))
            .unwrap()
            .0
    }

    #[test]
    fn a_harness_gets_its_own_fields_by_id_or_alias() {
        let frontmatter = mapping(
            "name: security\nharness:\n  claude-code: { model: haiku }\n  opencode: { model: a/b }",
        );
        let claude = fields_for(&frontmatter, &["claude-code", "claude"]);
        assert_eq!(claude.get("model").and_then(|v| v.as_str()), Some("haiku"));
        let opencode = fields_for(&frontmatter, &["opencode"]);
        assert_eq!(opencode.get("model").and_then(|v| v.as_str()), Some("a/b"));
        assert!(fields_for(&frontmatter, &["codex"]).is_empty());
    }

    #[test]
    fn a_block_that_redefines_identity_is_an_error_and_a_root_model_a_warning() {
        let findings = common_findings(&mapping(
            "name: security\nmodel: haiku\nharness:\n  codex: { name: other }",
        ));
        assert_eq!(findings.errors.len(), 1, "{findings:?}");
        assert!(findings.errors[0].contains("harness.codex.name"));
        assert_eq!(findings.warnings.len(), 1);
        assert!(findings.warnings[0].contains("`model`"));
    }

    #[test]
    fn a_block_that_is_not_a_mapping_is_an_error() {
        let findings = common_findings(&mapping("name: x\nharness: [claude-code]"));
        assert_eq!(findings.errors.len(), 1);
        let findings = common_findings(&mapping("name: x\nharness:\n  codex: high"));
        assert_eq!(findings.errors.len(), 1);
    }

    #[test]
    fn a_skill_without_a_block_is_delivered_byte_for_byte() {
        let payload = b"---\nname: deploy\ndescription: Deploys\n---\nRun it.\n";
        assert!(matches!(skill_for(payload, &["codex"]), Cow::Borrowed(_)));
    }

    #[test]
    fn a_skill_loses_its_block_and_gains_its_harness_fields() {
        let payload = b"---\nname: deploy\ndescription: Deploys\nharness:\n  claude-code:\n    allowed-tools: [Bash]\n    name: nope\n  codex: { x: 1 }\n---\nRun it.\n";
        let delivered =
            String::from_utf8(skill_for(payload, &["claude-code"]).into_owned()).unwrap();
        assert!(!delivered.contains("harness"), "{delivered}");
        assert!(delivered.contains("allowed-tools"), "{delivered}");
        assert!(
            delivered.contains("name: deploy"),
            "identity stays: {delivered}"
        );
        assert!(
            !delivered.contains("x: 1"),
            "another harness's fields stay out"
        );
        assert!(delivered.ends_with("---\nRun it.\n"));
    }
}
