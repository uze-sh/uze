//! The portable package-root placeholder, `${PLUGIN_ROOT}`, and the one
//! place it is resolved.
//!
//! An author names a file of their own package through it in every file UZE
//! reads — `mcp.json`, `hooks.json`, a `SKILL.md`, an agent definition —
//! and it resolves to the package's build for the harness being delivered
//! to (`shared::build`), on every route of that harness: the whole package,
//! inside that harness's vendor store. No harness expands the portable
//! token, so this is the one path that holds in every harness and on every
//! route. It is never the sources': a hook that builds into its root would
//! write into the bytes the lock pins.
//!
//! A build keeps what its harness would misread at the plugin root (Claude's
//! `bin/`, `commands/`, ...) under [`CARRIED_DIR`], so a reference into one
//! of those paths resolves there: `${PLUGIN_ROOT}/bin/server` names
//! `<build>/.uze-carried/bin/server` on Claude Code and `<build>/bin/server`
//! where nothing is carried. [`PluginRoot`] is that path-aware root.
//!
//! Every function here takes the root already resolved
//! ([`crate::shared::build::VendorStore::root_of`]): which harness a text
//! is delivered to is the caller's to say.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

pub(crate) use uze_core::store::CARRIED_DIR;

/// How a package names its own root.
pub(crate) const TOKEN: &str = "${PLUGIN_ROOT}";

/// The root `${PLUGIN_ROOT}` resolves to on one harness: a directory, and
/// the package paths that sit under its [`CARRIED_DIR`] instead of where
/// the package has them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PluginRoot {
    root: PathBuf,
    carried: &'static [&'static str],
}

impl PluginRoot {
    /// A root holding every path where the package has it.
    pub(crate) fn whole(root: impl Into<PathBuf>) -> Self {
        Self::carrying(root, &[])
    }

    /// A build that keeps `carried` under its [`CARRIED_DIR`].
    pub(crate) fn carrying(root: impl Into<PathBuf>, carried: &'static [&'static str]) -> Self {
        Self {
            root: root.into(),
            carried,
        }
    }

    /// The directory itself: what a bare `${PLUGIN_ROOT}` names.
    pub(crate) fn path(&self) -> &Path {
        &self.root
    }

    /// Where the package's `relative` path is found under this root.
    pub(crate) fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        let relative = relative.as_ref();
        let relative = relative.strip_prefix(".").unwrap_or(relative);
        if self.is_carried(relative) {
            self.root.join(CARRIED_DIR).join(relative)
        } else {
            self.root.join(relative)
        }
    }

    /// The directory a path relative to the package is joined to, for a
    /// caller that joins it itself (a hook's exec-form script).
    pub(crate) fn base_for(&self, relative: impl AsRef<Path>) -> PathBuf {
        if self.is_carried(relative.as_ref()) {
            self.root.join(CARRIED_DIR)
        } else {
            self.root.clone()
        }
    }

    fn is_carried(&self, relative: &Path) -> bool {
        let relative = relative.strip_prefix(".").unwrap_or(relative);
        self.carried
            .iter()
            .any(|carried| relative.starts_with(Path::new(carried)))
    }

    /// `text` with every `${PLUGIN_ROOT}/<p>` whose `<p>` is carried
    /// rewritten to `${PLUGIN_ROOT}/.uze-carried/<p>`, the token kept: what
    /// a caller that substitutes the token itself (a shell line, quoted for
    /// its shell) is handed.
    pub(crate) fn carry_references<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.carried.is_empty() || !text.contains(TOKEN) {
            return Cow::Borrowed(text);
        }
        let mut rewritten = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find(TOKEN) {
            let after = &rest[at + TOKEN.len()..];
            rewritten.push_str(&rest[..at + TOKEN.len()]);
            if let Some(path) = after.strip_prefix(['/', '\\'])
                && self.carried.iter().any(|carried| names(path, carried))
            {
                rewritten.push('/');
                rewritten.push_str(CARRIED_DIR);
            }
            rest = after;
        }
        rewritten.push_str(rest);
        Cow::Owned(rewritten)
    }
}

/// Whether the text after `${PLUGIN_ROOT}/` names `carried` or a path under
/// it: the carried path, then the end of the path.
fn names(path: &str, carried: &str) -> bool {
    path.strip_prefix(carried).is_some_and(|after| {
        after
            .chars()
            .next()
            .is_none_or(|next| !(next.is_alphanumeric() || "._-+@".contains(next)))
    })
}

/// `text` with every placeholder resolved to `root`; borrowed when
/// there is nothing to resolve, so a caller can tell a text that needs a
/// rewritten copy from one that can be delivered as it is.
pub(crate) fn resolve_text<'a>(text: &'a str, root: &PluginRoot) -> Cow<'a, str> {
    if text.contains(TOKEN) {
        Cow::Owned(
            root.carry_references(text)
                .replace(TOKEN, &root.path().to_string_lossy()),
        )
    } else {
        Cow::Borrowed(text)
    }
}

/// `bytes` — a `SKILL.md` or an agent definition as written — with the
/// placeholder resolved; borrowed when there is nothing to resolve. Bytes
/// that are not UTF-8 carry no placeholder anyone could have written.
pub(crate) fn resolve_bytes<'a>(bytes: &'a [u8], root: &PluginRoot) -> Cow<'a, [u8]> {
    match std::str::from_utf8(bytes).map(|text| resolve_text(text, root)) {
        Ok(Cow::Owned(text)) => Cow::Owned(text.into_bytes()),
        _ => Cow::Borrowed(bytes),
    }
}

/// Resolves the placeholder in every string a JSON document carries.
pub(crate) fn resolve_json(value: &serde_json::Value, root: &PluginRoot) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(resolve_text(text, root).into_owned())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|item| resolve_json(item, root)).collect())
        }
        serde_json::Value::Object(entries) => serde_json::Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), resolve_json(value, root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole() -> PluginRoot {
        PluginRoot::whole("/store/p")
    }

    #[test]
    fn text_without_the_placeholder_is_borrowed() {
        assert!(matches!(
            resolve_text("plain body", &whole()),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn every_occurrence_resolves_and_vendor_tokens_are_left_alone() {
        let text = "a=${PLUGIN_ROOT}/x b=${PLUGIN_ROOT}/y c=${CLAUDE_PLUGIN_ROOT}/z";
        assert_eq!(
            resolve_text(text, &whole()),
            "a=/store/p/x b=/store/p/y c=${CLAUDE_PLUGIN_ROOT}/z"
        );
    }

    #[test]
    fn a_reference_into_a_carried_path_resolves_under_the_carried_directory() {
        let root = PluginRoot::carrying("/store/p", &["bin", "hooks/hooks.json"]);
        let text = "${PLUGIN_ROOT}/bin/server ${PLUGIN_ROOT}/bin \
                    ${PLUGIN_ROOT}/binary ${PLUGIN_ROOT}/hooks/hooks.json \
                    ${PLUGIN_ROOT}/hooks/check.sh ${PLUGIN_ROOT}";
        assert_eq!(
            resolve_text(text, &root),
            "/store/p/.uze-carried/bin/server /store/p/.uze-carried/bin \
                    /store/p/binary /store/p/.uze-carried/hooks/hooks.json \
                    /store/p/hooks/check.sh /store/p"
        );
        assert_eq!(
            root.join("bin/server"),
            Path::new("/store/p/.uze-carried/bin/server")
        );
        assert_eq!(
            root.join("./bin/server"),
            Path::new("/store/p/.uze-carried/bin/server")
        );
        assert_eq!(root.join("binary"), Path::new("/store/p/binary"));
        assert_eq!(root.base_for("bin/x"), Path::new("/store/p/.uze-carried"));
    }
}
