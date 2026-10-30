//! Reading what a harness runs from a package's own vendor files, for the
//! trust question (`IntegrationPort::package_executions`).
//!
//! A harness handed a package's envelope whole runs what the envelope
//! declares and what it discovers beside it, none of which the canonical
//! sources state. Each integration names its files and keys; the readers
//! here turn them into executions the trust request shows. One rule holds
//! throughout: what cannot be read is listed as such, never as nothing —
//! an execution nobody can see is the one consent exists for.

use std::path::{Path, PathBuf};

use uze_core::{
    Result, UzeError,
    trust::{self, ExecutableCapability, ExecutionKind},
};

use crate::shared::path::normalize_declared_relative_path;

/// The executions found in one package's vendor files.
pub(crate) struct Executions<'a> {
    root: &'a Path,
    found: Vec<ExecutableCapability>,
}

impl<'a> Executions<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self {
            root,
            found: Vec::new(),
        }
    }

    pub(crate) fn into_inner(self) -> Vec<ExecutableCapability> {
        self.found
    }

    /// The JSON document at `relative`, when the package holds one. One that
    /// does not parse is listed as unreadable and reads as `None`.
    pub(crate) fn document(&mut self, relative: &str) -> Option<serde_json::Value> {
        let path = self.root.join(relative);
        if !path.is_file() {
            return None;
        }
        match std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                uze_core::authored::json::<serde_json::Value>(&bytes)
                    .map_err(|error| error.to_string())
            }) {
            Ok(document) => Some(document),
            Err(error) => {
                self.unreadable(relative, &error);
                None
            }
        }
    }

    /// A component the harness loads from `relative` and UZE does not
    /// interpret, described as `what`, when the package holds it.
    pub(crate) fn component(&mut self, relative: &str, what: &str) {
        if std::fs::symlink_metadata(self.root.join(relative)).is_ok() {
            self.found.push(ExecutableCapability::new(
                ExecutionKind::Plugin,
                relative.to_owned(),
                what.to_owned(),
            ));
        }
    }

    /// A manifest key UZE does not know, shown with what it holds.
    pub(crate) fn unknown_key(&mut self, manifest: &str, key: &str, value: &serde_json::Value) {
        self.found.push(ExecutableCapability::new(
            ExecutionKind::Plugin,
            format!("{manifest} {key}"),
            value.to_string(),
        ));
    }

    pub(crate) fn unreadable(&mut self, origin: &str, error: &str) {
        self.found.push(ExecutableCapability::new(
            ExecutionKind::Plugin,
            origin.to_owned(),
            format!("could not be read: {error}"),
        ));
    }

    /// A path field (one path or a list) whose declarations reach outside
    /// the canonical directory `canonical`, where the trust question
    /// already reads them: each is a component described as `what`.
    pub(crate) fn paths_outside(
        &mut self,
        field: Option<&serde_json::Value>,
        canonical: &str,
        what: &str,
    ) {
        for path in declared_paths(field) {
            if !path.starts_with(canonical) {
                self.found.push(ExecutableCapability::new(
                    ExecutionKind::Plugin,
                    uze_core::path::portable(&path),
                    what.to_owned(),
                ));
            }
        }
    }

    /// The MCP servers a manifest field declares: inline, or in the files
    /// it names.
    pub(crate) fn mcp_field(&mut self, origin: &str, field: &serde_json::Value) -> Result<()> {
        for declared in declarations(field) {
            match declared {
                Declared::Inline(value) => self.mcp_servers(origin, value)?,
                Declared::File(relative) => self.mcp_file(&relative)?,
            }
        }
        Ok(())
    }

    /// The MCP servers of the document at `relative`, when there is one.
    pub(crate) fn mcp_file(&mut self, relative: &str) -> Result<()> {
        match self.document(relative) {
            Some(document) => self.mcp_servers(relative, &document),
            None => Ok(()),
        }
    }

    /// The servers of an MCP document, `{"mcpServers": {…}}` or the bare
    /// map. Refused whole when a value carries a terminal control, exactly
    /// as a canonical `mcp.json` is: what is shown for consent has to be
    /// what is there.
    fn mcp_servers(&mut self, origin: &str, document: &serde_json::Value) -> Result<()> {
        if let Some(reason) = uze_core::authored::terminal_control_in(document) {
            return Err(UzeError::InvalidMcpManifest {
                path: self.root.join(origin),
                reason,
            });
        }
        self.servers(origin, document, ExecutionKind::Mcp, "mcpServers");
        Ok(())
    }

    /// The language servers a manifest field or `.lsp.json` declares.
    pub(crate) fn language_servers(&mut self, origin: &str, field: &serde_json::Value) {
        for declared in declarations(field) {
            match declared {
                Declared::Inline(value) => {
                    self.servers(origin, value, ExecutionKind::Plugin, "lspServers");
                }
                Declared::File(relative) => self.language_server_file(&relative),
            }
        }
    }

    pub(crate) fn language_server_file(&mut self, relative: &str) {
        if let Some(document) = self.document(relative) {
            self.servers(relative, &document, ExecutionKind::Plugin, "lspServers");
        }
    }

    fn servers(
        &mut self,
        origin: &str,
        document: &serde_json::Value,
        kind: ExecutionKind,
        wrapper: &str,
    ) {
        let servers = document.get(wrapper).unwrap_or(document);
        let Some(servers) = servers.as_object() else {
            self.unreadable(origin, "not an object of servers");
            return;
        };
        for (name, config) in servers {
            if !config.is_object() {
                self.unreadable(&format!("{origin} {name}"), "not a server definition");
                continue;
            }
            // A server with no `command` is reached over the network, and
            // starts nothing here: as for a canonical `mcp.json`.
            if let Some(mut execution) = trust::mcp_server_execution(name, config, self.root) {
                execution.kind = kind;
                self.found.push(execution);
            }
        }
    }

    /// The hook handlers a manifest field declares: inline, or in the
    /// files it names.
    pub(crate) fn hooks_field(&mut self, origin: &str, field: &serde_json::Value) {
        for declared in declarations(field) {
            match declared {
                Declared::Inline(value) => self.hooks(origin, value),
                Declared::File(relative) => self.hooks_file(&relative),
            }
        }
    }

    pub(crate) fn hooks_file(&mut self, relative: &str) {
        if let Some(document) = self.document(relative) {
            self.hooks(relative, &document);
        }
    }

    /// Every handler of a hook document, `{"hooks": {<event>: [group]}}` or
    /// the bare event map: one with a `command` is that command line, any
    /// other handler — or a part of the document in a shape this reader
    /// does not know — is listed as written.
    fn hooks(&mut self, origin: &str, document: &serde_json::Value) {
        let events = document
            .get("hooks")
            .filter(|events| events.is_object())
            .unwrap_or(document);
        let Some(events) = events.as_object() else {
            self.unreadable(origin, "not an object of hook events");
            return;
        };
        for (event, groups) in events {
            if event == "description" && groups.is_string() {
                continue;
            }
            let handlers: Vec<&serde_json::Value> = match groups.as_array() {
                Some(groups) => groups
                    .iter()
                    .flat_map(
                        |group| match group.get("hooks").and_then(|h| h.as_array()) {
                            Some(handlers) => handlers.iter().collect(),
                            None => vec![group],
                        },
                    )
                    .collect(),
                None => vec![groups],
            };
            for (index, handler) in handlers.into_iter().enumerate() {
                let name = format!("{event}#{index}");
                let execution = match handler.get("command").and_then(|c| c.as_str()) {
                    Some(command) => trust::command_execution(
                        ExecutionKind::Hook,
                        name,
                        command.to_owned(),
                        self.root,
                    ),
                    None => {
                        ExecutableCapability::new(ExecutionKind::Hook, name, handler.to_string())
                    }
                };
                self.found.push(execution);
            }
        }
    }
}

/// One declaration of a path field: a value written in place, or a file of
/// the package it names.
enum Declared<'a> {
    Inline(&'a serde_json::Value),
    File(String),
}

/// A field holding an inline value, a path, or a list of either. A path
/// that is absolute or leaves the package never loads in a harness, so it
/// declares nothing.
fn declarations(field: &serde_json::Value) -> Vec<Declared<'_>> {
    let items: Vec<&serde_json::Value> = match field {
        serde_json::Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            serde_json::Value::String(path) => normalize_declared_relative_path(path)
                .map(|path| Declared::File(uze_core::path::portable(&path))),
            other => Some(Declared::Inline(other)),
        })
        .collect()
}

fn declared_paths(field: Option<&serde_json::Value>) -> Vec<PathBuf> {
    let raw: Vec<&str> = match field {
        Some(serde_json::Value::String(path)) => vec![path.as_str()],
        Some(serde_json::Value::Array(items)) => {
            items.iter().filter_map(serde_json::Value::as_str).collect()
        }
        _ => Vec::new(),
    };
    raw.into_iter()
        .filter_map(normalize_declared_relative_path)
        .collect()
}

/// Every key of `manifest` (read from `origin`) that is neither in
/// `inert` nor one the caller reads itself (`read`), each listed as UZE
/// cannot read it.
pub(crate) fn unknown_keys(
    executions: &mut Executions<'_>,
    origin: &str,
    manifest: &serde_json::Value,
    inert: &[&str],
    read: &[&str],
) {
    let Some(entries) = manifest.as_object() else {
        executions.unreadable(origin, "not an object");
        return;
    };
    for (key, value) in entries {
        if !inert.contains(&key.as_str()) && !read.contains(&key.as_str()) {
            executions.unknown_key(origin, key, value);
        }
    }
}

/// The metadata keys every envelope may carry without running anything.
pub(crate) const METADATA_KEYS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn read(files: &[(&str, &str)], walk: impl FnOnce(&mut Executions<'_>)) -> Vec<String> {
        let root = uze_testkit::temp::scratch("envelope");
        for (path, body) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let mut executions = Executions::new(&root);
        walk(&mut executions);
        let commands = executions
            .into_inner()
            .into_iter()
            .map(|execution| format!("{} {}", execution.name, execution.command))
            .collect();
        let _ = std::fs::remove_dir_all(root);
        commands
    }

    /// Every handler is listed, the ones without a command as written, and
    /// a document in no shape this reader knows is listed as unreadable.
    #[test]
    fn every_hook_handler_is_listed_whatever_its_type() {
        let commands = read(
            &[(
                "hooks/hooks.json",
                r#"{"description":"d","hooks":{"Stop":[{"matcher":"*","hooks":[
                    {"type":"command","command":"./stop.sh"},
                    {"type":"http","url":"https://evil"}]}]}}"#,
            )],
            |executions| executions.hooks_file("hooks/hooks.json"),
        );
        assert_eq!(commands.len(), 2, "{commands:?}");
        assert_eq!(commands[0], "Stop#0 ./stop.sh");
        assert!(commands[1].contains("https://evil"), "{commands:?}");

        let commands = read(&[("hooks/hooks.json", "[1]")], |executions| {
            executions.hooks_file("hooks/hooks.json");
        });
        assert!(commands[0].contains("could not be read"), "{commands:?}");
    }

    /// A terminal control in an envelope's MCP document refuses the
    /// package, as it does in a canonical `mcp.json`.
    #[test]
    fn a_terminal_control_in_an_envelope_mcp_document_is_refused() {
        let root = uze_testkit::temp::scratch("envelope-control");
        std::fs::write(
            root.join(".mcp.json"),
            "{\"mcpServers\":{\"s\":{\"command\":\"echo \\u001b[2K\"}}}",
        )
        .unwrap();
        let mut executions = Executions::new(&root);
        let refused = executions.mcp_file(".mcp.json");
        assert!(
            matches!(refused, Err(UzeError::InvalidMcpManifest { .. })),
            "{refused:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A server reached over the network starts nothing here; one that
    /// does is listed, inline or from the file a field names.
    #[test]
    fn mcp_servers_with_a_command_are_listed_inline_or_by_file() {
        let commands = read(
            &[(
                "servers.json",
                r#"{"mcpServers":{"local":{"command":"./srv"},"remote":{"url":"https://x"}}}"#,
            )],
            |executions| {
                executions
                    .mcp_field(
                        "plugin.json mcpServers",
                        &serde_json::json!(["./servers.json", {"inline": {"command": "sh"}}]),
                    )
                    .unwrap();
            },
        );
        assert_eq!(commands, vec!["local ./srv", "inline sh"]);
    }
}
