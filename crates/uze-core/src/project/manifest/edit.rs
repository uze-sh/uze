//! The one place the YAML document library is named.
//!
//! `agents.yaml` is the user's file, so a write must change the bytes it
//! means to change and leave every other byte — comments included —
//! exactly where they were. A serde round-trip cannot do that: it emits
//! from the struct, and a comment has no field to live in. So reading goes
//! through serde (typed, validated) and writing goes through this module,
//! which edits a concrete syntax tree in place.
//!
//! It exists as its own module for the same reason `uze-git` exists: the
//! library is named here and nowhere else, so replacing it later is one
//! file rather than every call site. That isolation is also what makes
//! depending on a `0.0.x` crate defensible — see `AGENTS.md`'s dependency
//! rule.

use std::path::{Path, PathBuf};

use noyalib::{
    compat::serde_yaml,
    cst::{Document, parse_document},
};

use serde::Serialize;

use super::Section;
use crate::{Result, UzeError, persistence::write_atomic_within};

/// A manifest open for editing: the authored bytes plus the tree that
/// knows where each declaration sits in them.
pub struct ManifestDocument {
    path: PathBuf,
    document: Document,
}

impl ManifestDocument {
    /// Opens `path` for editing. A file that is not YAML is refused here
    /// rather than at the first write, so a malformed manifest never gets
    /// half-edited.
    pub fn open(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(UzeError::read(path))?;
        Self::from_source(path, &text)
    }

    /// The in-memory constructor, so the guards can be tested without a
    /// filesystem.
    pub fn from_source(path: &Path, text: &str) -> Result<Self> {
        reject_shapes_we_will_not_edit(path, text)?;
        let document = parse_document(text).map_err(|error| malformed(path, error.to_string()))?;
        Ok(Self {
            path: path.to_path_buf(),
            document,
        })
    }

    /// A new, empty document — what a manifest UZE creates starts from.
    pub fn empty(path: &Path) -> Result<Self> {
        Self::from_source(path, "")
    }

    pub fn source(&self) -> &str {
        self.document.source()
    }

    /// Adds `key` to the mapping at `mapping_path`, or replaces its value
    /// when it is already there. The mapping itself is created when
    /// absent, since a manifest that has never declared a marketplace has
    /// no `marketplaces:` key to insert into.
    ///
    /// The value is a *typed* value, never a fragment of YAML text. The
    /// library emits it in the file's own style and verifies the spliced
    /// document loads back as exactly this value, which is what a caller
    /// spelling `{ a: b }` by hand could never guarantee: a path holding a
    /// comma, a ref holding a brace, a version that looks like a float —
    /// each one is the emitter's problem now, not a quoting rule this
    /// module has to remember.
    pub fn upsert(
        &mut self,
        mapping_path: &str,
        key: &str,
        value: &serde_yaml::Value,
    ) -> Result<()> {
        if self.document.get(mapping_path).is_none() {
            if self.holds_empty_root_key(mapping_path) {
                return self.fill_empty_root_key(mapping_path, key, value);
            }
            return self.append_mapping(mapping_path, key, value);
        }
        let full = format!("{mapping_path}.{key}");
        if self.document.get(&full).is_some() {
            // A value replacing one of its own kind is an in-place edit: the
            // library matches the value's existing style, and the comment
            // its author wrote beside it stays where they put it.
            if is_block(value) == self.holds_block(&full) {
                return self
                    .document
                    .set_value(&full, value)
                    .map_err(|error| self.refusal(&full, error.to_string()));
            }
            // Growing a scalar into a block, or folding a block into a
            // scalar, is not one: the library refuses the first and spells
            // the second as a multi-line plain scalar. When it was the
            // mapping's only entry the parent goes with it, and the
            // re-insert takes the create path.
            self.remove(mapping_path, key)?;
            return self.upsert(mapping_path, key, value);
        }
        self.document
            .insert_entry_value(mapping_path, key, value)
            .map_err(|error| self.refusal(mapping_path, error.to_string()))
    }

    /// Declares `key` in one of the sections the manifest carries for its
    /// owner, whatever shape its value takes. The owner decides what the
    /// value means and reads it back; this only places it.
    pub fn set_key(&mut self, section: Section, key: &str, value: &impl Serialize) -> Result<()> {
        self.set_in(section.key(), key, value)
    }

    /// Whether the document holds anything at the dotted `path`.
    pub fn holds(&self, path: &str) -> bool {
        self.document.get(path).is_some()
    }

    /// Sets `key` in the mapping at the dotted `mapping_path`, which must
    /// exist: the entries beside it, and their comments, stay as written.
    pub fn set_in(&mut self, mapping_path: &str, key: &str, value: &impl Serialize) -> Result<()> {
        let value = serde_yaml::to_value(value)
            .map_err(|error| self.refusal(&format!("{mapping_path}.{key}"), error.to_string()))?;
        self.upsert(mapping_path, key, &value)
    }

    /// Takes `key` out of the mapping at the dotted `mapping_path`; the
    /// mapping goes with its last key.
    pub fn unset_in(&mut self, mapping_path: &str, key: &str) -> Result<()> {
        self.remove(mapping_path, key)
    }

    /// Takes `key` out of `section`; the section goes with its last key.
    pub fn unset_key(&mut self, section: Section, key: &str) -> Result<()> {
        self.unset_in(section.key(), key)
    }

    /// Appends `item` to the sequence at `sequence_path`, creating the
    /// sequence when the entry has none, and doing nothing when the item is
    /// already there. Declaring a plugin twice is a no-op, not a second
    /// line.
    pub fn push_unique(&mut self, sequence_path: &str, item: &str) -> Result<()> {
        let wanted = serde_yaml::Value::String(item.to_owned());
        match self.document.get(sequence_path) {
            Some(_) => {
                if self.items(sequence_path).iter().any(|held| held == item) {
                    return Ok(());
                }
                self.document
                    .push_back_value(sequence_path, &wanted)
                    .map_err(|error| self.refusal(sequence_path, error.to_string()))
            }
            None => {
                let (parent, key) = split_path(sequence_path);
                self.document
                    .insert_entry_value(parent, key, &serde_yaml::Value::Sequence(vec![wanted]))
                    .map_err(|error| self.refusal(sequence_path, error.to_string()))
            }
        }
    }

    /// Removes `item` from the sequence at `sequence_path`. When it was the
    /// last one the sequence's own key goes with it: a `plugins: []` left
    /// behind reads as a list that was made and emptied, which is not what
    /// happened — the entry above it simply has nothing taken from it now.
    pub fn remove_from(&mut self, sequence_path: &str, item: &str) -> Result<()> {
        let held = self.items(sequence_path);
        let Some(index) = held.iter().position(|candidate| candidate == item) else {
            return Ok(());
        };
        if held.len() == 1 {
            return self
                .document
                .remove(sequence_path)
                .map_err(|error| self.refusal(sequence_path, error.to_string()));
        }
        self.document
            .remove(&format!("{sequence_path}[{index}]"))
            .map_err(|error| self.refusal(sequence_path, error.to_string()))
    }

    fn holds_block(&self, path: &str) -> bool {
        self.held(path).as_ref().is_some_and(is_block)
    }

    /// The sequence's items as strings. A non-sequence, or an item that is
    /// not a scalar, reads as empty: the schema refuses both, and this is
    /// only ever asked to locate an entry to edit.
    fn items(&self, sequence_path: &str) -> Vec<String> {
        self.held(sequence_path)
            .as_ref()
            .and_then(|value| value.as_sequence())
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The value at a dotted path, walked segment by segment: `Value::get`
    /// reads one key, so a dotted path handed to it whole would look for a
    /// key literally named `marketplaces.acme.plugins` and quietly find
    /// nothing.
    fn held(&self, path: &str) -> Option<serde_yaml::Value> {
        let root = self.document.as_value();
        let mut value = Some(&*root);
        for segment in path.split('.') {
            value = value.and_then(|held| held.get(segment));
        }
        value.cloned()
    }

    /// Whether `key` is written at the root with no value, as the scaffold
    /// once wrote `worktrees:`: a heading over commented choices, which declares
    /// nothing and which the library therefore does not report as present.
    fn holds_empty_root_key(&self, key: &str) -> bool {
        self.document
            .source()
            .lines()
            .any(|line| is_empty_root_key(line, key))
    }

    /// Writes the first entry under an empty root key, directly beneath it,
    /// so the choice lands among the comments that explain it rather than in
    /// a second block of the same name at the end of the file.
    fn fill_empty_root_key(
        &mut self,
        mapping_path: &str,
        key: &str,
        value: &serde_yaml::Value,
    ) -> Result<()> {
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(key, value.clone());
        let emitted = serde_yaml::to_string(&serde_yaml::Value::Mapping(entry))
            .map_err(|error| self.refusal(mapping_path, error.to_string()))?;
        let indented: String = emitted.lines().map(|line| format!("  {line}\n")).collect();
        let mut text = String::new();
        let mut filled = false;
        for line in self.document.source().split_inclusive('\n') {
            text.push_str(line);
            if !filled && is_empty_root_key(line.trim_end_matches('\n'), mapping_path) {
                if !line.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&indented);
                filled = true;
            }
        }
        let reopened = Self::from_source(&self.path, &text)?;
        if reopened
            .document
            .get(&format!("{mapping_path}.{key}"))
            .is_none()
        {
            return Err(self.refusal(
                mapping_path,
                "the entry written under the empty key did not read back".to_owned(),
            ));
        }
        *self = reopened;
        Ok(())
    }

    /// The create path: `mapping_path` has never been declared, so there is
    /// no mapping to insert into and nothing above to preserve. Emitting
    /// the whole `key: value` block at once keeps the emitter's hands on
    /// the quoting here too.
    fn append_mapping(
        &mut self,
        mapping_path: &str,
        key: &str,
        value: &serde_yaml::Value,
    ) -> Result<()> {
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(key, value.clone());
        let mut block = serde_yaml::Mapping::new();
        block.insert(mapping_path, serde_yaml::Value::Mapping(entry));
        let mut text = serde_yaml::to_string(&serde_yaml::Value::Mapping(block))
            .map_err(|error| self.refusal(mapping_path, error.to_string()))?;
        // The emitter does not terminate its last line; a file UZE writes
        // always ends with one.
        if !text.ends_with('\n') {
            text.push('\n');
        }
        self.append_block(&text)
    }

    /// Removes `key` from the mapping at `mapping_path`. When it was the
    /// last entry, the mapping's own key goes with it: `marketplaces:` left
    /// holding `{}` reads as a declaration that was made and emptied,
    /// which is not what happened.
    pub(super) fn remove(&mut self, mapping_path: &str, key: &str) -> Result<()> {
        let full = format!("{mapping_path}.{key}");
        if self.document.get(&full).is_none() {
            return Ok(());
        }
        // Walked a segment at a time: the value's own lookup reads a dotted
        // path as one key, and a nested mapping's last entry would leave it
        // behind as `{}`.
        let last_one = {
            let root = self.document.as_value();
            mapping_path
                .split('.')
                .try_fold(&*root, |value, segment| value.get(segment))
                .and_then(|mapping| mapping.as_mapping().map(|entries| entries.len()))
                == Some(1)
        };
        if last_one {
            if self.is_the_only_root_key(mapping_path) {
                return self.drop_root_block(mapping_path);
            }
            self.document
                .remove(mapping_path)
                .map_err(|error| self.refusal(mapping_path, error.to_string()))?;
            return Ok(());
        }
        self.document
            .remove(&full)
            .map_err(|error| self.refusal(&full, error.to_string()))
    }

    fn is_the_only_root_key(&self, key: &str) -> bool {
        !key.contains('.')
            && self
                .document
                .as_value()
                .as_mapping()
                .is_some_and(|root| root.len() == 1)
    }

    /// Removes a root key's whole block by its lines. The library, asked to
    /// remove the last key of a document, leaves `{}` in place of the file
    /// and drops the comments around it; the comments are the author's.
    fn drop_root_block(&mut self, key: &str) -> Result<()> {
        let mut text = String::new();
        let mut inside = false;
        for line in self.document.source().split_inclusive('\n') {
            let bare = line.trim_end_matches(['\n', '\r']);
            let at_margin = !bare.is_empty() && !bare.starts_with([' ', '#']);
            if at_margin {
                inside = opens_root_key(bare, key);
            } else if bare.starts_with('#') {
                inside = false;
            }
            if !inside {
                text.push_str(line);
            }
        }
        *self = Self::from_source(&self.path, &text)?;
        Ok(())
    }

    /// Appends a block verbatim — used only when writing a manifest UZE
    /// itself authored, where there is no user formatting to preserve.
    pub fn append_block(&mut self, block: &str) -> Result<()> {
        let mut source = self.document.source().to_owned();
        if !source.is_empty() && !source.ends_with('\n') {
            source.push('\n');
        }
        source.push_str(block);
        self.document =
            parse_document(&source).map_err(|error| malformed(&self.path, error.to_string()))?;
        Ok(())
    }

    /// Writes the document back. `agents.yaml` sits at the project's root,
    /// so the directory holding it is what it may not lead out of.
    pub fn save(&self) -> Result<()> {
        let root = match self.path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        write_atomic_within(root, &self.path, self.document.source().as_bytes())
    }

    fn refusal(&self, path: &str, reason: String) -> UzeError {
        UzeError::MalformedManifest {
            path: self.path.clone(),
            reason: format!(
                "`{path}` could not be written ({reason}); edit `{}` by hand and run the command \
                 again",
                self.path.display()
            ),
        }
    }
}

/// A dotted path split into the mapping that holds the last key, and that
/// key. `marketplaces.acme.plugins` is `plugins` inside `marketplaces.acme`.
fn split_path(path: &str) -> (&str, &str) {
    match path.rsplit_once('.') {
        Some((parent, key)) => (parent, key),
        None => ("", path),
    }
}

fn malformed(path: &Path, reason: String) -> UzeError {
    UzeError::MalformedManifest {
        path: path.to_path_buf(),
        reason,
    }
}

/// YAML a targeted edit cannot reason about locally. Each of these makes a
/// byte range mean something elsewhere in the file, so splicing one place
/// can change another — refused before an edit is attempted rather than
/// after the file is mangled.
fn reject_shapes_we_will_not_edit(path: &Path, text: &str) -> Result<()> {
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let refuse = |what: &str| {
            Err(malformed(
                path,
                format!(
                    "line {} uses {what}, which UZE will not edit around; declare the manifest \
                     without it, or edit the file by hand",
                    number + 1
                ),
            ))
        };
        if line.starts_with("---") || line.starts_with("...") {
            return refuse("a document separator");
        }
        if trimmed.starts_with("<<:") {
            return refuse("a merge key");
        }
        if line.contains('\t') {
            return refuse("a tab");
        }
        let before_comment = trimmed.split(" #").next().unwrap_or(trimmed);
        if let Some((_, after_colon)) = before_comment.split_once(": ") {
            let value = after_colon.trim_start();
            if value.starts_with('&') || value.starts_with('*') {
                return refuse("a YAML anchor or alias");
            }
        }
    }
    Ok(())
}

fn is_block(value: &serde_yaml::Value) -> bool {
    matches!(
        value,
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_)
    )
}

/// `line`, at column zero, opens `key`'s block: the key spelled plain or
/// quoted either way, then its colon.
fn opens_root_key(line: &str, key: &str) -> bool {
    ["", "\"", "'"].into_iter().any(|quote| {
        line.strip_prefix(quote)
            .and_then(|rest| rest.strip_prefix(key))
            .and_then(|rest| rest.strip_prefix(quote))
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

/// `line` is `key:` at column zero with nothing after it but a comment.
fn is_empty_root_key(line: &str, key: &str) -> bool {
    line.strip_prefix(key)
        .and_then(|rest| rest.strip_prefix(':'))
        .is_some_and(|rest| {
            let rest = rest.trim();
            rest.is_empty() || rest.starts_with('#')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORED: &str = "\
# O ambiente de agentes deste projeto.
workspace:
  delivery: pr        # entrega via pull request
  link: [.env.local]

marketplaces:
  # pinado ate o fix do #431 subir
  ai: { path: ../ai, ref: v0.3.1, plugins: [flow] }
";

    fn open(text: &str) -> ManifestDocument {
        ManifestDocument::from_source(Path::new("/p/agents.yaml"), text).unwrap()
    }

    fn value(yaml: &str) -> serde_yaml::Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    fn reparsed(document: &ManifestDocument) -> serde_yaml::Value {
        serde_yaml::from_str(document.source()).unwrap()
    }

    #[test]
    fn reading_and_writing_back_untouched_is_byte_identical() {
        assert_eq!(open(AUTHORED).source(), AUTHORED);
    }

    #[test]
    fn setting_a_scalar_changes_only_that_value() {
        let mut document = open(AUTHORED);
        document
            .upsert("workspace", "delivery", &value("merge"))
            .unwrap();
        assert_eq!(
            document.source().replace("delivery: merge", "delivery: pr"),
            AUTHORED
        );
    }

    #[test]
    fn adding_a_marketplace_keeps_the_comment_above_its_neighbour() {
        let mut document = open(AUTHORED);
        document
            .upsert(
                "marketplaces",
                "tools",
                &value("path: ../tools\nplugins: [git]\n"),
            )
            .unwrap();
        let after = document.source();
        assert!(after.contains("# pinado ate o fix do #431 subir"));
        assert!(after.contains("ai: { path: ../ai, ref: v0.3.1, plugins: [flow] }"));
        assert_eq!(
            reparsed(&document)["marketplaces"]["tools"]["plugins"][0].as_str(),
            Some("git"),
            "{after}"
        );
    }

    #[test]
    fn adding_the_first_marketplace_creates_the_mapping() {
        let mut document = open("workspace:\n  delivery: pr\n");
        document
            .upsert(
                "marketplaces",
                "ai",
                &value("path: ../ai\nplugins: [flow]\n"),
            )
            .unwrap();
        assert!(
            document
                .source()
                .starts_with("workspace:\n  delivery: pr\n")
        );
        assert_eq!(
            reparsed(&document)["marketplaces"]["ai"]["plugins"][0].as_str(),
            Some("flow")
        );
    }

    #[test]
    fn pushing_a_plugin_is_idempotent_and_keeps_the_entry() {
        let mut document = open(AUTHORED);
        document
            .push_unique("marketplaces.ai.plugins", "review")
            .unwrap();
        document
            .push_unique("marketplaces.ai.plugins", "review")
            .unwrap();
        let plugins = reparsed(&document)["marketplaces"]["ai"]["plugins"].clone();
        assert_eq!(plugins.as_sequence().map(Vec::len), Some(2), "{plugins:?}");
        assert!(
            document
                .source()
                .contains("# pinado ate o fix do #431 subir")
        );
    }

    #[test]
    fn a_scalar_entry_can_grow_into_a_block_without_losing_its_neighbours() {
        let mut document = open("marketplaces:\n  ai: ../ai\n  tools: ../tools\n");
        document
            .upsert("marketplaces", "ai", &value("path: ../ai\nref: v0.4.0\n"))
            .unwrap();
        let after = document.source();
        assert!(after.contains("ref: v0.4.0"), "{after}");
        assert!(after.contains("tools: ../tools"), "{after}");
    }

    #[test]
    fn declaring_a_marketplace_that_is_already_there_replaces_its_value() {
        let mut document = open(AUTHORED);
        document
            .upsert(
                "marketplaces",
                "ai",
                &value("path: ../ai\nref: v0.4.0\nplugins: [flow]\n"),
            )
            .unwrap();
        assert!(document.source().contains("v0.4.0"));
        assert!(!document.source().contains("v0.3.1"));
    }

    /// The emitter, not this module, decides how a value is spelled — so a
    /// path holding a comma stays one entry instead of becoming two.
    #[test]
    fn a_value_that_would_break_the_syntax_is_quoted_by_the_emitter() {
        let mut document = open("workspace:\n  delivery: pr\n");
        document
            .upsert(
                "marketplaces",
                "odd",
                &value("path: \"../my, dir\"\nref: \"release/{next}\"\n"),
            )
            .unwrap();
        let reparsed = reparsed(&document);
        assert_eq!(
            reparsed["marketplaces"]["odd"]["path"].as_str(),
            Some("../my, dir")
        );
        assert_eq!(
            reparsed["marketplaces"]["odd"]["ref"].as_str(),
            Some("release/{next}")
        );
    }

    #[test]
    fn removing_the_last_marketplace_removes_the_mapping_rather_than_leaving_it_empty() {
        let mut document = open(AUTHORED);
        document.remove("marketplaces", "ai").unwrap();
        let after = document.source();
        assert!(!after.contains("{}"), "left an empty mapping: {after}");
        assert!(!after.contains("marketplaces:"));
        assert!(after.contains("delivery: pr"));
    }

    #[test]
    fn removing_something_absent_is_not_an_error() {
        let mut document = open(AUTHORED);
        document.remove("marketplaces", "never-declared").unwrap();
        assert_eq!(document.source(), AUTHORED);
    }

    #[test]
    fn any_workspace_key_can_be_set_and_unset_keeping_every_comment() {
        let mut document = open(AUTHORED);
        document
            .set_key(
                Section::Workspace,
                "gate",
                &vec!["cargo test", "cargo clippy"],
            )
            .unwrap();
        document.set_key(Section::Workspace, "slots", &4).unwrap();
        document
            .set_key(Section::Workspace, "link", &vec![".env", ".env.local"])
            .unwrap();
        document.unset_key(Section::Workspace, "delivery").unwrap();
        let after = reparsed(&document);
        assert_eq!(after["workspace"]["gate"][1].as_str(), Some("cargo clippy"));
        assert_eq!(after["workspace"]["slots"].as_u64(), Some(4));
        assert_eq!(after["workspace"]["link"][1].as_str(), Some(".env.local"));
        assert!(after["workspace"].get("delivery").is_none());
        let source = document.source();
        assert!(
            source.contains("# O ambiente de agentes deste projeto."),
            "{source}"
        );
        assert!(
            source.contains("# pinado ate o fix do #431 subir"),
            "{source}"
        );
    }

    #[test]
    fn a_list_can_become_a_scalar_and_back() {
        let mut document = open("workspace:\n  gate:\n    - a\n    - b\n  slots: 2\n");
        document
            .set_key(Section::Workspace, "gate", &"make check")
            .unwrap();
        assert_eq!(
            reparsed(&document)["workspace"]["gate"].as_str(),
            Some("make check"),
            "{}",
            document.source()
        );
        document
            .set_key(Section::Workspace, "gate", &vec!["x", "y", "z"])
            .unwrap();
        let after = reparsed(&document);
        assert_eq!(after["workspace"]["gate"][2].as_str(), Some("z"));
        assert_eq!(after["workspace"]["slots"].as_u64(), Some(2));
    }

    #[test]
    fn unsetting_the_last_workspace_key_takes_the_section_with_it() {
        let mut document = open("# kept\nworkspace:\n  slots: 2\n");
        document.unset_key(Section::Workspace, "slots").unwrap();
        assert_eq!(document.source().trim_end(), "# kept");
    }

    #[test]
    fn a_quoted_section_goes_with_its_last_key_like_a_plain_one() {
        for spelled in ["\"workspace\"", "'workspace'"] {
            let mut document = open(&format!("# kept\n{spelled}:\n  slots: 2\n"));
            document.unset_key(Section::Workspace, "slots").unwrap();
            assert_eq!(document.source().trim_end(), "# kept", "{spelled}");
        }
    }

    #[test]
    fn unsetting_the_last_key_of_a_nested_mapping_takes_the_mapping_with_it() {
        let mut document = ManifestDocument::from_source(
            Path::new("/project/agents.yaml"),
            "workspace:\n  delivery: push\n  actions:\n    on_change: Commit.\n",
        )
        .unwrap();
        document.unset_in("workspace.actions", "on_change").unwrap();
        assert_eq!(document.source(), "workspace:\n  delivery: push\n");
    }

    #[test]
    fn anchors_aliases_merge_keys_tabs_and_streams_are_refused_up_front() {
        for (what, text) in [
            ("anchor", "a: &base 1\nb: 2\n"),
            ("alias", "a: 1\nb: *base\n"),
            ("merge key", "a:\n  <<: *base\n"),
            ("tab", "a:\n\tb: 1\n"),
            ("stream", "---\na: 1\n"),
        ] {
            let outcome = ManifestDocument::from_source(Path::new("/p/agents.yaml"), text);
            assert!(outcome.is_err(), "{what} should have been refused");
        }
    }

    #[test]
    fn a_hash_inside_a_comment_is_not_mistaken_for_a_shape_we_refuse() {
        let text = "# uses <<: and &anchors in prose\nworkspace:\n  delivery: pr\n";
        assert_eq!(open(text).source(), text);
    }
}
