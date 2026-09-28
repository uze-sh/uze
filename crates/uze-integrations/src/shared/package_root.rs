//! The portable package-root placeholder, `${PLUGIN_ROOT}`, and the one
//! place it is resolved.
//!
//! An author names a file of their own package through it in every file UZE
//! reads — `mcp.json`, `hooks.json`, a `SKILL.md`, an agent definition —
//! and it resolves to the Store package root in all of them. No harness
//! expands the portable token, and the ones that stage their own copy of a
//! plugin do not follow the symlinks some deliveries are made of, so the
//! one path that holds in every harness and on every route is the Store's.

use std::{borrow::Cow, path::Path};

/// How a package names its own root.
pub(crate) const TOKEN: &str = "${PLUGIN_ROOT}";

/// `text` with every placeholder resolved to `package_root`; borrowed when
/// there is nothing to resolve, so a caller can tell a text that needs a
/// rewritten copy from one that can be delivered as it is.
pub(crate) fn resolve_text<'a>(text: &'a str, package_root: &Path) -> Cow<'a, str> {
    if text.contains(TOKEN) {
        Cow::Owned(text.replace(TOKEN, &package_root.to_string_lossy()))
    } else {
        Cow::Borrowed(text)
    }
}

/// `bytes` — a `SKILL.md` or an agent definition as written — with the
/// placeholder resolved; borrowed when there is nothing to resolve. Bytes
/// that are not UTF-8 carry no placeholder anyone could have written.
pub(crate) fn resolve_bytes<'a>(bytes: &'a [u8], package_root: &Path) -> Cow<'a, [u8]> {
    match std::str::from_utf8(bytes).map(|text| resolve_text(text, package_root)) {
        Ok(Cow::Owned(text)) => Cow::Owned(text.into_bytes()),
        _ => Cow::Borrowed(bytes),
    }
}

/// Resolves the placeholder in every string a JSON document carries.
pub(crate) fn resolve_json(value: &serde_json::Value, package_root: &Path) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(resolve_text(text, package_root).into_owned())
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| resolve_json(item, package_root))
                .collect(),
        ),
        serde_json::Value::Object(entries) => serde_json::Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), resolve_json(value, package_root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_without_the_placeholder_is_borrowed() {
        assert!(matches!(
            resolve_text("plain body", Path::new("/store/p")),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn every_occurrence_resolves_and_vendor_tokens_are_left_alone() {
        let text = "a=${PLUGIN_ROOT}/x b=${PLUGIN_ROOT}/y c=${CLAUDE_PLUGIN_ROOT}/z";
        assert_eq!(
            resolve_text(text, Path::new("/store/p")),
            "a=/store/p/x b=/store/p/y c=${CLAUDE_PLUGIN_ROOT}/z"
        );
    }
}
