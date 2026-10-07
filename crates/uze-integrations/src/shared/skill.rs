//! Shared Skill-frontmatter reading and generation for integrations that
//! cannot deliver the canonical `skills/<name>/SKILL.md` bytes as-is and
//! must translate them into a vendor representation (Claude's
//! `disable-model-invocation`/`user-invocable` fields, Codex's
//! `agents/openai.yaml` policy sidecar, OpenCode's
//! `metadata.opencode/*` controls).
//!
//! The canonical Skill model (ADR-030) is deliberately minimal: an optional
//! YAML-style frontmatter block whose only UZE-consumed fields are
//! `description` and the `invoke:` invocation policy, followed by the body.
//! Parsing here is best-effort and never fatal — a body that does not parse
//! as expected frontmatter is treated as body-only, which is always
//! deliverable. The canonical bytes themselves remain the Store's payload;
//! this module only reads them, and generated wrappers are always Derived
//! Artifacts under `$UZE_HOME`, never Store writes.
//!
//! The vendor fields below are deliberately NOT canonical UZE metadata:
//! they exist only to translate the canonical `invoke:` policy into each
//! harness's own surface (section 5 — no vendor field names in the
//! canonical model).
//!
//! A loose Skill reaches its harness as a directory of its own
//! ([`attach_skill_tree`]): the rendered `SKILL.md` and whatever else the
//! harness needs beside it, written, and the canonical supporting files,
//! copied. Nothing in it links into the Store.

use std::{
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, ManagedArtifact},
    home::UzeHome,
    integration::{IntegrationPort, active_plugin_name, qualified_exposure_name_candidates},
    router::CompatibilityRoute,
};

/// A Skill that nobody may invoke is never projected (ADR-030 §1).
pub(crate) fn invalid_policy_plan() -> ExposurePlan {
    ExposurePlan {
        route: CompatibilityRoute::Unsupported,
        mechanism: ExposureMechanism::Unsupported {
            rationale: "This Skill declares invoke.model: false and invoke.user: false — nobody can invoke it, so UZE never projects it. Fix the `invoke:` block in SKILL.md.".to_owned(),
        },
        evidence: "Invalid canonical invocation policy: a Skill that nobody may invoke is not a projectable capability (ADR-030 §1).".to_owned(),
    }
}

/// A Skill reaches `harness` through a managed user-scope attachment, which
/// exists only once `uze setup` has completed.
pub(crate) fn setup_pending_plan(harness: &str) -> ExposurePlan {
    ExposurePlan {
        route: CompatibilityRoute::Unsupported,
        mechanism: ExposureMechanism::Unsupported {
            rationale: format!(
                "{harness} has not completed `uze setup`; run `uze setup` so UZE can attach this Skill."
            ),
        },
        evidence: format!(
            "Skills reach {harness} through a managed user-scope attachment, which exists only once `uze setup` has completed."
        ),
    }
}

/// The physical entry name for `resource`: the one shared-root resolution
/// settled on, else the integration's own first candidate.
pub(crate) fn entry_name(integration: &dyn IntegrationPort, resource: &Resource) -> Option<String> {
    resource
        .resolved_exposure_name
        .clone()
        .or_else(|| {
            integration
                .exposure_name_candidates(resource)
                .into_iter()
                .next()
        })
        .map(|label| uze_core::path::file_name_for(&label))
}

/// The stable namespaced invocation label (`flow:review`, ADR-026) a
/// generated wrapper declares as its `name`.
pub(crate) fn skill_label(uze_home: &UzeHome, resource: &Resource) -> Option<String> {
    qualified_exposure_name_candidates(resource, &active_plugin_name(uze_home, resource))
        .into_iter()
        .next()
}

/// The root an earlier build generated `vendor`'s Skill wrappers under,
/// which an entry detached from that build may still point into.
pub(crate) fn skill_wrapper_root(uze_home: &UzeHome, vendor: &str) -> PathBuf {
    crate::shared::path::attachment_root(uze_home, vendor).join("skills")
}

/// A generated SKILL.md: `name`, the canonical description re-quoted (never
/// raw-interpolated), the vendor's own `markers` as frontmatter lines, the
/// fields `keys` are given under the canonical `harness:` block, then the
/// canonical body verbatim.
pub fn render_skill_wrapper(
    name: &str,
    canonical_bytes: &[u8],
    markers: &[&str],
    keys: &[&str],
) -> String {
    let (description, body) = parse_skill_body(canonical_bytes);
    let mut document = format!("---\nname: {name}\n");
    if let Some(description) = description {
        let escaped = escape_yaml_double_quoted(&description);
        document.push_str(&format!("description: \"{escaped}\"\n"));
    }
    for marker in markers {
        document.push_str(marker);
        document.push('\n');
    }
    for line in crate::shared::dialect::skill_block_lines(canonical_bytes, keys) {
        document.push_str(&line);
        document.push('\n');
    }
    document.push_str("---\n");
    document.push_str(&body);
    document
}

/// Splits a canonical SKILL.md into `(description, body)`:
///
/// - A leading `---` line, then key-value lines, then a closing `---` line
///   is frontmatter; only the top-level `description` key is consumed,
///   everything else in the block is deliberately ignored (never
///   reinterpreted, never dropped from the canonical bytes — this function
///   only *reads*). Frontmatter that is malformed falls through to
///   "body-only". A quoted description is read as the text inside the
///   quotes, since the wrapper quotes it again; a block scalar (`>`, `|`)
///   is not followed onto its continuation lines.
/// - Everything after the closing marker is the body, with surrounding
///   whitespace preserved exactly as shipped (no trim, no rewrite). Bytes
///   that are not UTF-8 are carried as replacement characters rather than
///   dropping the body.
pub fn parse_skill_body(bytes: &[u8]) -> (Option<String>, String) {
    let text = String::from_utf8_lossy(bytes);
    let Some((head, body)) = split_frontmatter(text.strip_prefix('\u{feff}').unwrap_or(&text))
    else {
        return (None, text.into_owned());
    };
    let description = head
        .lines()
        .rev()
        .find_map(|line| key_value(line, "description"));
    (description.map(unquoted), body.to_owned())
}

pub use uze_core::skill::split_frontmatter;

/// The first value a top-level `key` holds in a frontmatter block, trimmed.
pub fn head_value<'a>(head: &'a str, key: &str) -> Option<&'a str> {
    head.lines().find_map(|line| key_value(line, key))
}

/// An indented line belongs to the mapping above it, so only an unindented
/// key is the top-level one — `metadata:` nesting a `description:` must not
/// stand in for the Skill's own.
fn key_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (candidate, value) = line.split_once(':')?;
    (candidate.trim_end() == key).then(|| value.trim())
}

/// The text a YAML flow scalar holds: the inside of a single- or
/// double-quoted value with its quote escapes undone, anything else as is.
fn unquoted(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return inner.replace("''", "'");
    }
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return value.to_owned();
    };
    let mut text = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => text.push('\r'),
            Some(escaped) => text.push(escaped),
            None => text.push('\\'),
        }
    }
    text
}

/// Escapes `value` as the contents of a YAML double-quoted scalar, so it can
/// be safely written as `"{escaped}"` in generated frontmatter regardless of
/// content — colons, embedded newlines, quotes, or text shaped like another
/// frontmatter key (e.g. `disable-model-invocation: false`) can never break
/// out of the quoted value or forge/duplicate a key. Only backslash, double
/// quote, and control characters need escaping inside a double-quoted YAML
/// scalar; everything else is passed through verbatim.
pub fn escape_yaml_double_quoted(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                escaped.push_str(&format!("\\x{:02x}", other as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

/// Whether the payload's own frontmatter carries `disable-model-invocation:
/// true` — the Claude marker that keeps a Skill explicit-only rather than
/// model-invocable. Recognized only when the trimmed value is exactly
/// `true`. Malformed or absent frontmatter, or a non-UTF8 payload, is
/// `false` — never a panic, never a guess.
pub fn has_disable_model_invocation(bytes: &[u8]) -> bool {
    frontmatter_is_true(bytes, "disable-model-invocation")
}

/// Whether the payload's own frontmatter carries `user-invocable: false` —
/// the Claude marker that hides a Skill from the `/` catalog while the
/// model may still load it automatically. Same strictness as
/// [`has_disable_model_invocation`].
pub fn has_user_invocable_false(bytes: &[u8]) -> bool {
    frontmatter_value(bytes, "user-invocable").is_some_and(|value| value == "false")
}

fn frontmatter_is_true(bytes: &[u8], key: &str) -> bool {
    frontmatter_value(bytes, key).is_some_and(|value| value == "true")
}

/// Reads one top-level frontmatter value (e.g. `name`) from canonical
/// bytes, so a generated wrapper can preserve the canonically-declared
/// identity instead of inventing one. `None` for absent/malformed
/// frontmatter or non-UTF8 payload — never a guess.
pub fn frontmatter_value(bytes: &[u8], key: &str) -> Option<String> {
    let text = is_utf8(bytes)?;
    let (head, _) = split_frontmatter(text.strip_prefix('\u{feff}').unwrap_or(text))?;
    head_value(head, key).map(str::to_owned)
}

/// Whether `skill_dir` carries Codex's explicit-only policy sidecar:
/// `agents/openai.yaml` declaring `allow_implicit_invocation: false`.
pub fn has_explicit_only_sidecar(skill_dir: &Path) -> bool {
    fs::read_to_string(skill_dir.join("agents/openai.yaml")).is_ok_and(|policy| {
        policy
            .lines()
            .any(|line| line.trim() == "allow_implicit_invocation: false")
    })
}

fn is_utf8(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

/// A loose Skill as planned: the directory it will be delivered as, whose
/// digest is recorded once the tree is attached.
pub(crate) fn skill_tree_plan(path: PathBuf) -> ManagedArtifact {
    ManagedArtifact::GeneratedTree {
        path,
        digest: String::new(),
    }
}

/// Delivers `resource` as the directory `path`: every canonical supporting
/// file copied (the names in `skip` left out), then `rendered` written over
/// them — the `SKILL.md` in the harness's own encoding and any file the
/// harness reads beside it. Replaced whole, so a session reading it sees
/// one build; and returned as the receipt's artifact, digest included.
pub(crate) fn attach_skill_tree(
    uze_home: &UzeHome,
    path: &Path,
    resource: &Resource,
    rendered: &[(&str, Vec<u8>)],
    skip: &[&str],
) -> Result<ManagedArtifact> {
    let owned = uze_core::state::owned_tree_digest(uze_home, path)?;
    let canonical_dir = resource
        .capability
        .path
        .parent()
        .expect("SKILL.md has a parent");
    let digest = uze_core::exposure::attach_generated_tree(path, owned.as_deref(), |staging| {
        copy_supporting_files(canonical_dir, &resource.package_root, staging, skip)?;
        for (relative, bytes) in rendered {
            let file = staging.join(relative);
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
            }
            write_file(&file, bytes)?;
        }
        Ok(())
    })?;
    Ok(ManagedArtifact::GeneratedTree {
        path: path.to_path_buf(),
        digest,
    })
}

/// Copies a canonical skill directory's files except `SKILL.md` and `skip`,
/// with the envelope mirror's containment rule: a link inside the package is
/// copied as the bytes it names, one that escapes it is refused by name. An
/// absent canonical directory (a Resource built without a real Store path,
/// in unit-test contexts) has nothing to copy.
fn copy_supporting_files(
    canonical_dir: &Path,
    package_root: &Path,
    destination: &Path,
    skip: &[&str],
) -> Result<()> {
    if !canonical_dir.is_dir() {
        return Ok(());
    }
    let package_root =
        uze_core::path::canonical(package_root).unwrap_or_else(|_| package_root.to_path_buf());
    let mut excluded = vec!["SKILL.md"];
    excluded.extend_from_slice(skip);
    crate::shared::tree::mirror_tree(canonical_dir, destination, &package_root, &excluded)
}

/// Codex's official invocation-policy metadata: `agents/openai.yaml` beside
/// `SKILL.md`, with `policy.allow_implicit_invocation: false`. Per Codex's
/// Build skills documentation this makes the skill *not* implicitly
/// invocable by the model while explicit `$skill` invocation still works
/// (verified against codex-cli 0.149.0 via `codex debug prompt-input`).
pub(crate) const EXPLICIT_ONLY_POLICY_YAML: &str = "policy:\n  allow_implicit_invocation: false\n";

/// Writes Codex's explicit-only policy sidecar into `skill_dir`.
pub(crate) fn write_explicit_only_sidecar(skill_dir: &Path) -> Result<()> {
    let agents = skill_dir.join("agents");
    fs::create_dir_all(&agents).map_err(UzeError::write(&agents))?;
    write_file(
        &agents.join("openai.yaml"),
        EXPLICIT_ONLY_POLICY_YAML.as_bytes(),
    )
}

pub(crate) fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    fs::write(path, content).map_err(UzeError::write(path))
}

/// The invocation controls a harness reads from a Skill's own frontmatter
/// that the Skill wrote by hand. UZE writes each of them from the canonical
/// `invoke:` block when it delivers, so one written by hand either repeats
/// that or overrides it on this harness alone, and means nothing to any
/// other. `controls` are paths into the frontmatter (`metadata` then a key
/// under it).
pub(crate) fn hand_written_controls(
    resource: &Resource,
    controls: &[&[&str]],
) -> uze_core::capability::harness::Findings {
    let mut findings = uze_core::capability::harness::Findings::default();
    if resource.capability.kind != uze_core::capability::CapabilityKind::AgentSkill {
        return findings;
    }
    let Some((frontmatter, _)) = std::str::from_utf8(&resource.capability.payload)
        .ok()
        .and_then(uze_core::capability::harness::frontmatter_of)
    else {
        return findings;
    };
    for path in controls {
        let Some((first, rest)) = path.split_first() else {
            continue;
        };
        let mut value = frontmatter.get(first);
        for key in rest {
            value = value
                .and_then(|value| value.as_mapping())
                .and_then(|mapping| mapping.get(key));
        }
        if value.is_some() {
            findings.warnings.push(format!(
                "`{}` is this harness's own invocation control, written by hand; say who may \
                 invoke the skill with `invoke:`, which UZE writes into every harness's own field",
                path.join(".")
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapper_carries_the_harness_block_fields_for_its_harness_only() {
        let bytes = b"---\nname: deploy\ndescription: Deploys\nharness:\n  opencode: { temperature: 0.2 }\n  codex: { y: 2 }\n---\nRun.\n";
        let rendered = render_skill_wrapper("flow:deploy", bytes, &["slash: false"], &["opencode"]);
        assert!(
            rendered.contains("slash: false\ntemperature: 0.2\n"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("harness") && !rendered.contains("y: 2"),
            "{rendered}"
        );
        assert!(rendered.ends_with("---\nRun.\n"));
    }

    #[test]
    fn reads_description_and_preserves_body_verbatim() {
        let bytes = b"---\ndescription: Review code\nothers: ignored\n---\n\nBody line one.\nBody line two.\n";
        let (description, body) = parse_skill_body(bytes);
        assert_eq!(description.as_deref(), Some("Review code"));
        assert_eq!(body, "\nBody line one.\nBody line two.\n");
    }

    #[test]
    fn body_only_file_has_no_description() {
        let bytes = b"Just a prompt body.\n";
        let (description, body) = parse_skill_body(bytes);
        assert_eq!(description, None);
        assert_eq!(body, "Just a prompt body.\n");
    }

    #[test]
    fn malformed_frontmatter_falls_through_to_body_only() {
        let bytes = b"---\ndescription: broken\nno closing marker\n";
        let (description, body) = parse_skill_body(bytes);
        assert_eq!(description, None);
        assert_eq!(body, std::str::from_utf8(bytes).unwrap());
    }

    #[test]
    fn a_body_that_is_not_utf8_is_carried_not_dropped() {
        let bytes = b"---\ndescription: d\n---\nBody \xff here.\n";
        let (description, body) = parse_skill_body(bytes);
        assert_eq!(description.as_deref(), Some("d"));
        assert_eq!(body, "Body \u{fffd} here.\n");
    }

    #[test]
    fn a_quoted_description_is_read_without_its_quotes() {
        for (declared, read) in [
            (r#""Review: code""#, "Review: code"),
            (r#""Say \"hi\"""#, r#"Say "hi""#),
            ("'It''s here'", "It's here"),
            ("plain text", "plain text"),
        ] {
            let bytes = format!("---\ndescription: {declared}\n---\nbody\n");
            assert_eq!(
                parse_skill_body(bytes.as_bytes()).0.as_deref(),
                Some(read),
                "{declared}"
            );
        }
        let rendered =
            render_skill_wrapper("n", b"---\ndescription: \"quoted\"\n---\nbody\n", &[], &[]);
        assert!(rendered.contains("description: \"quoted\"\n"), "{rendered}");
    }

    #[test]
    fn a_nested_key_never_stands_in_for_the_top_level_one() {
        let bytes =
            b"---\ndescription: top\nmetadata:\n  description: nested\n  name: inner\n---\nbody\n";
        assert_eq!(parse_skill_body(bytes).0.as_deref(), Some("top"));
        assert_eq!(frontmatter_value(bytes, "name"), None);
    }

    #[test]
    fn crlf_frontmatter_is_frontmatter() {
        let bytes = b"---\r\ndescription: Review code\r\nuser-invocable: false\r\n---\r\nBody.\r\n";
        let (description, body) = parse_skill_body(bytes);
        assert_eq!(description.as_deref(), Some("Review code"));
        assert_eq!(body, "Body.\r\n");
        assert!(has_user_invocable_false(bytes));
    }

    #[test]
    fn escapes_backslash_quote_and_control_characters() {
        let escaped = escape_yaml_double_quoted("a \\ b \" c\nd\te");
        assert_eq!(escaped, "a \\\\ b \\\" c\\nd\\te");
    }

    #[test]
    fn escaped_description_cannot_forge_or_duplicate_the_marker() {
        let tricky = "Has: a colon, \"quotes\", a\nnewline, and disable-model-invocation: false";
        let escaped = escape_yaml_double_quoted(tricky);
        assert!(!escaped.contains('\n'));
        assert!(
            escaped
                .match_indices('"')
                .all(|(index, _)| index > 0 && escaped.as_bytes()[index - 1] == b'\\')
        );
        let frontmatter =
            format!("---\ndescription: \"{escaped}\"\ndisable-model-invocation: true\n---\nbody\n");
        // The description text may mention the marker name; it must never
        // become a second key. Exactly one frontmatter LINE declares it,
        // it stays `true`, and the embedded `false` text is trapped inside
        // the quoted description scalar.
        let marker_lines = frontmatter
            .lines()
            .filter(|line| line.trim_start().starts_with("disable-model-invocation:"))
            .collect::<Vec<_>>();
        assert_eq!(marker_lines.len(), 1, "{frontmatter}");
        assert_eq!(marker_lines[0].trim(), "disable-model-invocation: true");
        assert!(has_disable_model_invocation(frontmatter.as_bytes()));
    }

    #[test]
    fn detects_disable_model_invocation_true() {
        let bytes = b"---\ndescription: d\ndisable-model-invocation: true\n---\nbody\n";
        assert!(has_disable_model_invocation(bytes));
    }

    #[test]
    fn absent_marker_is_false() {
        let bytes = b"---\ndescription: d\n---\nbody\n";
        assert!(!has_disable_model_invocation(bytes));
    }

    #[test]
    fn marker_set_to_false_is_false() {
        let bytes = b"---\ndisable-model-invocation: false\n---\nbody\n";
        assert!(!has_disable_model_invocation(bytes));
    }

    #[test]
    fn malformed_frontmatter_marker_is_false() {
        let bytes = b"---\ndisable-model-invocation: true\nno closing marker\n";
        assert!(!has_disable_model_invocation(bytes));
    }

    #[test]
    fn non_utf8_marker_is_false() {
        assert!(!has_disable_model_invocation(b"\xff\xfe\x00"));
    }

    #[test]
    fn detects_user_invocable_false_and_only_false() {
        let bytes = b"---\nuser-invocable: false\n---\nbody\n";
        assert!(has_user_invocable_false(bytes));
        let bytes = b"---\nuser-invocable: true\n---\nbody\n";
        assert!(!has_user_invocable_false(bytes));
        let bytes = b"---\ndescription: d\n---\nbody\n";
        assert!(!has_user_invocable_false(bytes));
    }
}
