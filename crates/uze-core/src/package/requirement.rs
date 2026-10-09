//! The executables a package's scripts need on the machine.
//!
//! No harness gives a plugin a way to say "my hook needs `python3`", and a
//! command hook whose program is missing exits 127, which every
//! command-hook harness treats as a non-blocking error: the tool runs and
//! the guard silently never did. So a package declares what it needs under
//! `extensions["sh.uze"].requirements` in its `plugin.json` (the standard's
//! place for one client's data), and whatever UZE generates for it adds its
//! own needs to the same set. UZE checks the set and tells the person what
//! is missing; it never installs anything.

use std::{cmp::Ordering, fmt, path::Path};

use serde::{Deserialize, Serialize};

use crate::error::{Result, UzeError};

/// One executable a package needs on `PATH`, as `plugin.json` declares it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub executable: String,
    /// The minimum version, written `>=1.6` or `1.6`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Why the package needs it, in the author's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

impl Requirement {
    pub fn named(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            version: None,
            purpose: None,
        }
    }

    pub fn with_purpose(mut self, purpose: impl Into<String>) -> Self {
        self.purpose = Some(purpose.into());
        self
    }

    /// The version the executable must reach, when one is declared.
    pub fn minimum(&self) -> Option<Version> {
        self.version.as_deref().and_then(Version::constraint)
    }

    fn fault(&self) -> Option<String> {
        let executable = self.executable.trim();
        if executable.is_empty() {
            return Some("a requirement names no `executable`".to_owned());
        }
        if executable != self.executable
            || executable.chars().any(|character| {
                character.is_whitespace()
                    || matches!(character, '/' | '\\')
                    || crate::authored::is_terminal_control(character)
            })
        {
            return Some(format!(
                "requirement `{}` must be a bare program name, found on `PATH`",
                crate::authored::inert_line(&self.executable)
            ));
        }
        if let Some(version) = &self.version
            && Version::constraint(version).is_none()
        {
            return Some(format!(
                "requirement `{executable}` has version `{version}`; write a minimum such as \
                 `>=1.6`"
            ));
        }
        None
    }
}

/// Who needs a requirement: the author, or something UZE generated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequirementSource {
    /// Declared in the package's own `plugin.json`.
    Author,
    /// Needed by an artifact UZE generates for the package (the hook
    /// wrapper's `jq`), not by anything the author wrote.
    Artifact { what: String },
    /// Needed to run one hook group's handler. A group that fails closed
    /// denies every call it matches while the executable is missing.
    Hook { id: String, fails_closed: bool },
}

impl fmt::Display for RequirementSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Author => formatter.write_str("the plugin"),
            Self::Artifact { what } => formatter.write_str(what),
            Self::Hook { id, .. } => write!(formatter, "hook `{id}`"),
        }
    }
}

/// A requirement and every source that needs it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectiveRequirement {
    pub requirement: Requirement,
    pub sources: Vec<RequirementSource>,
}

impl EffectiveRequirement {
    /// The hook groups that deny every call they match while this is
    /// missing.
    pub fn denying_hooks(&self) -> Vec<&str> {
        self.sources
            .iter()
            .filter_map(|source| match source {
                RequirementSource::Hook {
                    id,
                    fails_closed: true,
                } => Some(id.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// A package's whole set: what it declares and what was generated for it,
/// one entry per executable.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct EffectiveRequirements(Vec<EffectiveRequirement>);

impl EffectiveRequirements {
    /// Merges every `(requirement, source)` by executable: the strictest
    /// minimum wins, the first purpose stated is kept, and every source is
    /// named once.
    pub fn from_sources(
        entries: impl IntoIterator<Item = (Requirement, RequirementSource)>,
    ) -> Self {
        let mut merged: Vec<EffectiveRequirement> = Vec::new();
        for (requirement, source) in entries {
            match merged
                .iter_mut()
                .find(|entry| entry.requirement.executable == requirement.executable)
            {
                Some(entry) => {
                    if stricter(&requirement, &entry.requirement) {
                        entry.requirement.version = requirement.version.clone();
                    }
                    if entry.requirement.purpose.is_none() {
                        entry.requirement.purpose = requirement.purpose.clone();
                    }
                    if !entry.sources.contains(&source) {
                        entry.sources.push(source);
                    }
                }
                None => merged.push(EffectiveRequirement {
                    requirement,
                    sources: vec![source],
                }),
            }
        }
        merged.sort_by(|left, right| {
            left.requirement
                .executable
                .cmp(&right.requirement.executable)
        });
        Self(merged)
    }

    /// What `declared` states, attributed to the author, plus what
    /// generated artifacts add.
    pub fn of(
        declared: &[Requirement],
        contributed: impl IntoIterator<Item = (Requirement, RequirementSource)>,
    ) -> Self {
        Self::from_sources(
            declared
                .iter()
                .cloned()
                .map(|requirement| (requirement, RequirementSource::Author))
                .chain(contributed),
        )
    }

    pub fn iter(&self) -> impl Iterator<Item = &EffectiveRequirement> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn stricter(candidate: &Requirement, current: &Requirement) -> bool {
    match (candidate.minimum(), current.minimum()) {
        (Some(candidate), Some(current)) => candidate > current,
        (Some(_), None) => true,
        _ => false,
    }
}

/// A dotted version, compared component by component (`1.10` > `1.9`), a
/// missing component counting as zero (`1.6` == `1.6.0`).
#[derive(Clone, Debug)]
pub struct Version(Vec<u64>);

impl Version {
    /// The first dotted number in `text`: how a `--version` line is read
    /// (`jq-1.7.1`, `Python 3.12.3`, `v20.11.0`).
    pub fn find(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            if bytes[start].is_ascii_digit() {
                let mut end = start;
                while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                    end += 1;
                }
                let run = text[start..end].trim_end_matches('.');
                if let Some(version) = Self::parse(run) {
                    return Some(version);
                }
                start = end;
            } else {
                start += 1;
            }
        }
        None
    }

    /// A declared minimum: `>=1.6` or `1.6`.
    pub fn constraint(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::parse(text.strip_prefix(">=").unwrap_or(text).trim())
    }

    fn parse(text: &str) -> Option<Self> {
        let parts = text
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        (!parts.is_empty()).then_some(Self(parts))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let width = self.0.len().max(other.0.len());
        (0..width)
            .map(|index| {
                let left = self.0.get(index).copied().unwrap_or(0);
                let right = other.0.get(index).copied().unwrap_or(0);
                left.cmp(&right)
            })
            .find(|ordering| ordering.is_ne())
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(u64::to_string).collect();
        formatter.write_str(&parts.join("."))
    }
}

/// The requirements a parsed `plugin.json` declares, refusing a malformed
/// one: a requirement nobody can check would be reported met forever.
pub fn declared_in(manifest: &serde_json::Value, path: &Path) -> Result<Vec<Requirement>> {
    let Some(value) = manifest
        .get("extensions")
        .and_then(|extensions| extensions.get(crate::authoring::UZE_NAMESPACE))
        .and_then(|ours| ours.get(REQUIREMENTS_KEY))
    else {
        return Ok(Vec::new());
    };
    let invalid = |reason: String| UzeError::InvalidRequirement {
        path: path.to_path_buf(),
        reason,
    };
    let requirements: Vec<Requirement> =
        serde_json::from_value(value.clone()).map_err(|error| invalid(error.to_string()))?;
    if let Some(fault) = requirements.iter().find_map(Requirement::fault) {
        return Err(invalid(fault));
    }
    Ok(requirements)
}

/// The key under UZE's manifest namespace that holds the requirements.
pub const REQUIREMENTS_KEY: &str = "requirements";

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(executable: &str, version: Option<&str>) -> Requirement {
        Requirement {
            executable: executable.to_owned(),
            version: version.map(str::to_owned),
            purpose: None,
        }
    }

    fn manifest(requirements: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "name": "demo",
            "extensions": { "sh.uze": { "requirements": requirements } },
        })
    }

    #[test]
    fn a_declared_requirement_is_read_from_uzes_namespace() {
        let declared = declared_in(
            &manifest(serde_json::json!([
                {"executable": "jq", "version": ">=1.6", "purpose": "hooks parse JSON"}
            ])),
            Path::new("plugin.json"),
        )
        .unwrap();
        assert_eq!(
            declared,
            vec![Requirement {
                executable: "jq".to_owned(),
                version: Some(">=1.6".to_owned()),
                purpose: Some("hooks parse JSON".to_owned()),
            }]
        );
        assert!(
            declared_in(
                &serde_json::json!({"name": "demo"}),
                Path::new("plugin.json")
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn a_malformed_requirement_is_refused_by_name() {
        for (entry, named) in [
            (serde_json::json!([{"version": ">=1"}]), "executable"),
            (serde_json::json!([{"executable": ""}]), "executable"),
            (serde_json::json!([{"executable": "/usr/bin/jq"}]), "bare"),
            (
                serde_json::json!([{"executable": "jq", "version": "latest"}]),
                "latest",
            ),
            (serde_json::json!([{"executable": "jq", "url": "x"}]), "url"),
            (serde_json::json!({"executable": "jq"}), "sequence"),
        ] {
            let error = declared_in(&manifest(entry.clone()), Path::new("plugin.json"))
                .expect_err(&format!("{entry} must be refused"))
                .to_string();
            assert!(error.contains(named), "{entry}: {error}");
        }
    }

    #[test]
    fn the_effective_set_merges_by_executable_and_keeps_the_strictest_minimum() {
        let set = EffectiveRequirements::of(
            &[
                requirement("jq", Some(">=1.6")),
                requirement("python3", None).with_purpose("the guard"),
            ],
            [
                (
                    requirement("jq", Some("1.7")),
                    RequirementSource::Artifact {
                        what: "hook wrapper".to_owned(),
                    },
                ),
                (
                    requirement("jq", Some(">=1.5")),
                    RequirementSource::Artifact {
                        what: "hook wrapper".to_owned(),
                    },
                ),
            ],
        );
        let entries: Vec<_> = set.iter().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].requirement.executable, "jq");
        assert_eq!(entries[0].requirement.version.as_deref(), Some("1.7"));
        assert_eq!(
            entries[0].sources,
            vec![
                RequirementSource::Author,
                RequirementSource::Artifact {
                    what: "hook wrapper".to_owned()
                }
            ]
        );
        assert_eq!(entries[1].requirement.purpose.as_deref(), Some("the guard"));
    }

    #[test]
    fn a_fail_closed_hook_is_named_as_denying() {
        let set = EffectiveRequirements::from_sources([
            (
                requirement("python3", None),
                RequirementSource::Hook {
                    id: "guard".to_owned(),
                    fails_closed: true,
                },
            ),
            (
                requirement("python3", None),
                RequirementSource::Hook {
                    id: "log".to_owned(),
                    fails_closed: false,
                },
            ),
        ]);
        assert_eq!(set.iter().next().unwrap().denying_hooks(), vec!["guard"]);
    }

    #[test]
    fn versions_compare_by_component_and_are_found_in_version_lines() {
        assert!(Version::constraint("1.10").unwrap() > Version::constraint(">=1.9").unwrap());
        assert_eq!(
            Version::constraint("1.6").unwrap(),
            Version::constraint("1.6.0").unwrap()
        );
        for (line, version) in [
            ("jq-1.7.1", "1.7.1"),
            ("Python 3.12.3", "3.12.3"),
            ("v20.11.0", "20.11.0"),
            ("git version 2.43.0.windows.1", "2.43.0"),
        ] {
            assert_eq!(Version::find(line).unwrap().to_string(), version, "{line}");
        }
        assert_eq!(Version::find("no digits here"), None);
    }
}
