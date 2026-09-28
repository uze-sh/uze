//! The canonical Agent (ADR-031): one Markdown definition under a package's
//! `agents/`, its frontmatter carrying metadata and its body the prompt.
//!
//! A harness sees an agent under the label `<plugin>:<logical name>`. The
//! logical name is the definition's subdirectories below `agents/` joined
//! with `:`, then its frontmatter `name`, else its file stem —
//! `agents/review/security.md` is `review:security`. That is the rule a
//! harness that namespaces plugin agents already applies; adopting it as
//! the portable rule means one agent answers to one label on every harness,
//! and two files of the same name in different subdirectories never
//! collide.

use std::path::Path;

use noyalib::{ParserConfig, compat::serde_yaml, from_str_with_config};

use crate::skill::split_frontmatter;

/// An agent definition read from its canonical bytes. Every field the
/// author wrote is kept in `frontmatter`: which of them a harness receives
/// is that harness's integration's decision, never this module's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentDocument {
    pub name: Option<String>,
    pub description: Option<String>,
    pub frontmatter: serde_yaml::Mapping,
    pub body: String,
}

impl AgentDocument {
    /// `None` when the frontmatter does not parse as a YAML mapping: such a
    /// definition has no reliable name or description to project.
    pub fn parse(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let Some((head, body)) = split_frontmatter(text) else {
            return Some(Self {
                body: text.to_owned(),
                ..Self::default()
            });
        };
        let parsed: serde_yaml::Value =
            from_str_with_config(head, &ParserConfig::serde_yaml_compat()).ok()?;
        let frontmatter = match parsed {
            serde_yaml::Value::Mapping(mapping) => mapping,
            serde_yaml::Value::Null => serde_yaml::Mapping::new(),
            _ => return None,
        };
        let text_field = |key: &str| {
            frontmatter
                .get(key)
                .and_then(serde_yaml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        Some(Self {
            name: text_field("name"),
            description: text_field("description"),
            body: body.to_owned(),
            frontmatter,
        })
    }
}

impl AgentDocument {
    /// The definition as Markdown again: named `name` when the harness
    /// reads the name from the frontmatter, then the `set` fields the
    /// harness needs, then the authored fields `keep` accepts
    /// (`description` is always carried), with `body` as its prompt. What a
    /// projection into a harness's own Markdown agent directory writes;
    /// which fields it keeps is the harness's decision.
    pub fn render(
        &self,
        name: Option<&str>,
        set: &[(&str, &str)],
        keep: impl Fn(&str) -> bool,
        body: &str,
    ) -> String {
        let mut frontmatter = serde_yaml::Mapping::new();
        if let Some(name) = name {
            frontmatter.insert(
                "name".to_owned(),
                serde_yaml::Value::String(name.to_owned()),
            );
        }
        for (key, value) in set {
            frontmatter.insert(
                (*key).to_owned(),
                serde_yaml::Value::String((*value).to_owned()),
            );
        }
        if let Some(description) = &self.description {
            frontmatter.insert(
                "description".to_owned(),
                serde_yaml::Value::String(description.clone()),
            );
        }
        for (key, value) in &self.frontmatter {
            let key = key.as_str();
            if key != "name"
                && key != "description"
                && !set.iter().any(|(fixed, _)| *fixed == key)
                && keep(key)
            {
                frontmatter.insert(key.to_owned(), value.clone());
            }
        }
        let head = serde_yaml::to_string(&serde_yaml::Value::Mapping(frontmatter))
            .expect("a mapping of parsed YAML values serializes");
        let head = head.trim_end_matches('\n');
        format!("---\n{head}\n---\n{body}")
    }
}

/// The agent's logical name from its path below `agents/` and its bytes.
/// A definition whose frontmatter does not parse is named after its file,
/// which is what a harness that still loads it calls it.
pub fn logical_name(relative_to_agents: &Path, payload: &[u8]) -> Option<String> {
    let stem = relative_to_agents.file_stem()?.to_str()?;
    let name = AgentDocument::parse(payload)
        .and_then(|document| document.name)
        .unwrap_or_else(|| stem.to_owned());
    let mut segments = Vec::new();
    if let Some(parent) = relative_to_agents.parent() {
        for component in parent.components() {
            segments.push(component.as_os_str().to_str()?.to_owned());
        }
    }
    segments.push(name);
    Some(segments.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_agent_is_named_after_its_file() {
        assert_eq!(
            logical_name(
                Path::new("reviewer.md"),
                b"---\ndescription: d\n---\nbody\n"
            )
            .as_deref(),
            Some("reviewer")
        );
    }

    #[test]
    fn a_nested_agent_joins_its_subdirectories() {
        assert_eq!(
            logical_name(Path::new("review/security.md"), b"body").as_deref(),
            Some("review:security")
        );
    }

    #[test]
    fn a_frontmatter_name_replaces_only_the_file_name() {
        assert_eq!(
            logical_name(
                Path::new("review/security.md"),
                b"---\nname: audit\ndescription: d\n---\nbody\n"
            )
            .as_deref(),
            Some("review:audit")
        );
    }

    #[test]
    fn an_unparseable_frontmatter_falls_back_to_the_file() {
        assert_eq!(
            logical_name(Path::new("x.md"), b"---\nname: [unclosed\n---\nbody\n").as_deref(),
            Some("x")
        );
    }

    #[test]
    fn a_render_names_the_agent_and_keeps_only_accepted_fields() {
        let document = AgentDocument::parse(
            b"---\nname: reviewer\ndescription: \"Reviews: code\"\nmodel: haiku\ntools: Read, Grep\n---\n\nPrompt\n",
        )
        .unwrap();
        let rendered = document.render(
            Some("kit:reviewer"),
            &[],
            |key| key == "model",
            "\nPrompt\n",
        );
        let reparsed = AgentDocument::parse(rendered.as_bytes()).unwrap();
        assert_eq!(reparsed.name.as_deref(), Some("kit:reviewer"));
        assert_eq!(reparsed.description.as_deref(), Some("Reviews: code"));
        assert!(reparsed.frontmatter.get("model").is_some());
        assert!(reparsed.frontmatter.get("tools").is_none());
        assert_eq!(reparsed.body, "\nPrompt\n");
    }

    #[test]
    fn every_authored_field_is_kept() {
        let document = AgentDocument::parse(
            b"---\nname: reviewer\ndescription: Reviews\nmodel: haiku\ntools: Read, Grep\n---\n\nPrompt\n",
        )
        .unwrap();
        assert_eq!(document.name.as_deref(), Some("reviewer"));
        assert_eq!(document.description.as_deref(), Some("Reviews"));
        assert_eq!(document.frontmatter.len(), 4);
        assert_eq!(document.body.trim(), "Prompt");
    }
}
