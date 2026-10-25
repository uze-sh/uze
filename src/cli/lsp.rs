//! `uze agent plugin lsp`: a language server for an extension's manifest,
//! views and tests, over stdio.
//!
//! Its diagnostics are the check's own: on every change it checks the
//! edited file's extension with the editor's unsaved text in place of
//! disk, binds its fixtures and runs its tests, exactly as
//! `uze agent plugin check` does. Completion and hover come from the
//! catalog the decoder checks against. It runs nothing a plugin ships and
//! writes no file.

mod complete;
mod position;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use serde_json::{Value, json};
use uze_application::{CheckedExtension, ExtensionOverlay, UzeApplication};

use complete::FileKind;

/// How long the server waits after a change for the next one before it
/// checks: typing produces a change per key.
const SETTLE: Duration = Duration::from_millis(150);

/// Runs the server on stdio until the editor shuts it down.
pub(crate) fn serve(app: &UzeApplication) -> uze_application::Result<()> {
    let (connection, threads) = Connection::stdio();
    run(app, &connection)?;
    // The writer thread ends once every sender of its channel is gone;
    // joining while the connection holds one waits forever.
    drop(connection);
    threads.join().map_err(|error| {
        uze_application::UzeError::LifecycleBlocked(format!("the language server's stdio: {error}"))
    })
}

/// The server over `connection`, from `initialize` to `exit`.
pub(crate) fn run(app: &UzeApplication, connection: &Connection) -> uze_application::Result<()> {
    let blocked = |error: lsp_server::ProtocolError| {
        uze_application::UzeError::LifecycleBlocked(format!("language server: {error}"))
    };
    connection.initialize(capabilities()).map_err(blocked)?;
    let mut server = Server::new(app);
    loop {
        let message = match server.pending.is_empty() {
            true => match connection.receiver.recv() {
                Ok(message) => message,
                Err(_) => return Ok(()),
            },
            false => match connection.receiver.recv_timeout(SETTLE) {
                Ok(message) => message,
                Err(error) if error.is_timeout() => {
                    server.publish(connection);
                    continue;
                }
                Err(_) => return Ok(()),
            },
        };
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request).map_err(blocked)? {
                    return Ok(());
                }
                let response = server.answer(request);
                let _ = connection.sender.send(Message::Response(response));
            }
            Message::Notification(notification) => {
                if notification.method == "exit" {
                    return Ok(());
                }
                server.note(notification);
            }
            Message::Response(_) => {}
        }
    }
}

fn capabilities() -> Value {
    json!({
        "textDocumentSync": 1,
        "completionProvider": { "triggerCharacters": ["\"", "{", ".", ":", " "] },
        "hoverProvider": true,
    })
}

/// An open file of an extension.
struct Document {
    text: String,
    /// The extension's directory, and the file's path inside it.
    extension: PathBuf,
    relative: String,
}

struct Server<'a> {
    app: &'a UzeApplication,
    documents: BTreeMap<String, Document>,
    /// Extensions changed since their diagnostics were last published.
    pending: BTreeSet<PathBuf>,
    /// The files each extension last had diagnostics published for, so a
    /// fixed file is cleared.
    published: BTreeMap<PathBuf, BTreeSet<String>>,
    /// The data each extension's views are bound to, for completing a
    /// reference, from its last check.
    fields: BTreeMap<PathBuf, BTreeMap<String, Value>>,
}

impl<'a> Server<'a> {
    fn new(app: &'a UzeApplication) -> Self {
        Self {
            app,
            documents: BTreeMap::new(),
            pending: BTreeSet::new(),
            published: BTreeMap::new(),
            fields: BTreeMap::new(),
        }
    }

    fn note(&mut self, notification: Notification) {
        let params = &notification.params;
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let text = params["textDocument"]["text"].as_str().unwrap_or_default();
                self.hold(&uri, text.to_owned());
            }
            "textDocument/didChange" => {
                let text = params["contentChanges"]
                    .as_array()
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str())
                    .unwrap_or_default();
                self.hold(&uri, text.to_owned());
            }
            "textDocument/didClose" => {
                if let Some(document) = self.documents.remove(&uri) {
                    self.pending.insert(document.extension);
                }
            }
            _ => {}
        }
    }

    fn hold(&mut self, uri: &str, text: String) {
        let Some(path) = path_of(uri) else {
            return;
        };
        let Some((extension, relative)) = extension_of(&path) else {
            return;
        };
        self.pending.insert(extension.clone());
        self.documents.insert(
            uri.to_owned(),
            Document {
                text,
                extension,
                relative,
            },
        );
    }

    /// Checks each extension changed since the last publish, and sends the
    /// diagnostics of every file of it, empty for a file now clean.
    fn publish(&mut self, connection: &Connection) {
        for extension in std::mem::take(&mut self.pending) {
            let checked = self.check(&extension);
            self.fields
                .insert(extension.clone(), checked.fixtures.clone());
            let mut by_file: BTreeMap<String, Vec<Value>> = BTreeMap::new();
            for (diagnostic, severity) in checked
                .errors
                .iter()
                .map(|diagnostic| (diagnostic, 1))
                .chain(checked.warnings.iter().map(|diagnostic| (diagnostic, 2)))
            {
                let text = self.text_of(&extension, &diagnostic.file);
                let (line, character) =
                    position::position_of(&text, diagnostic.line, diagnostic.column);
                let message = match &diagnostic.suggestion {
                    Some(suggestion) => format!("{} ({suggestion})", diagnostic.message),
                    None => diagnostic.message.clone(),
                };
                by_file
                    .entry(diagnostic.file.clone())
                    .or_default()
                    .push(json!({
                        "range": {
                            "start": { "line": line, "character": character },
                            "end": { "line": line, "character": character + 1 },
                        },
                        "severity": severity,
                        "source": "uze",
                        "message": message,
                    }));
            }
            let mut files: BTreeSet<String> = by_file.keys().cloned().collect();
            files.extend(
                self.documents
                    .values()
                    .filter(|document| document.extension == extension)
                    .map(|document| document.relative.clone()),
            );
            files.extend(self.published.remove(&extension).unwrap_or_default());
            for file in &files {
                let diagnostics = by_file.remove(file).unwrap_or_default();
                let uri = uri_of(&extension.join(file));
                let _ = connection
                    .sender
                    .send(Message::Notification(Notification::new(
                        "textDocument/publishDiagnostics".to_owned(),
                        json!({ "uri": uri, "diagnostics": diagnostics }),
                    )));
            }
            self.published.insert(extension, files);
        }
    }

    /// The extension checked as `uze agent plugin check` checks it, with
    /// every open file's text in place of disk.
    fn check(&self, extension: &Path) -> CheckedExtension {
        let overlay: ExtensionOverlay = self
            .documents
            .values()
            .filter(|document| document.extension == extension)
            .map(|document| (document.relative.clone(), document.text.clone()))
            .collect();
        let mut checked = self.app.extensions().check_extension(extension, &overlay);
        super::extension_check::bind_fixtures(&mut checked);
        let run = uze::ui::extension_test::run_tests(self.app, &checked, false);
        checked.errors.extend(run.failures);
        checked
    }

    fn text_of(&self, extension: &Path, relative: &str) -> String {
        self.documents
            .values()
            .find(|document| document.extension == extension && document.relative == relative)
            .map(|document| document.text.clone())
            .or_else(|| std::fs::read_to_string(extension.join(relative)).ok())
            .unwrap_or_default()
    }

    fn answer(&mut self, request: Request) -> Response {
        let params = &request.params;
        let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
        let line = params["position"]["line"].as_u64().unwrap_or(0) as u32;
        let character = params["position"]["character"].as_u64().unwrap_or(0) as u32;
        let document = self.documents.get(uri);
        let kind = document.and_then(|document| FileKind::of(&document.relative));
        match (request.method.as_str(), document, kind) {
            ("textDocument/completion", Some(document), Some(kind)) => {
                let offset = position::offset_of(&document.text, line, character);
                let fields = self.view_fields(document);
                Response::new_ok(
                    request.id,
                    complete::completions(kind, &document.text, offset, &fields),
                )
            }
            ("textDocument/hover", Some(document), Some(kind)) => {
                let offset = position::offset_of(&document.text, line, character);
                let word = position::word_at(&document.text, offset);
                let answer = complete::hover(kind, word)
                    .map(|said| json!({ "contents": { "kind": "markdown", "value": said } }))
                    .unwrap_or(Value::Null);
                Response::new_ok(request.id, answer)
            }
            ("textDocument/completion" | "textDocument/hover", _, _) => {
                Response::new_ok(request.id, Value::Null)
            }
            (method, _, _) => Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("{method} is not answered by uze's extension server"),
            ),
        }
    }

    /// The data the view being edited is bound to: the fixture of the
    /// first source a surface drawing it reads, from the last check.
    fn view_fields(&self, document: &Document) -> Value {
        let Some(fixtures) = self.fields.get(&document.extension) else {
            return Value::Null;
        };
        let manifest = self.text_of(&document.extension, "extension.kdl");
        let source = manifest.lines().find_map(|line| {
            let line = line.trim_start();
            if !line.starts_with("surface ")
                || !line.contains(&format!("view=\"{}\"", document.relative))
            {
                return None;
            }
            let rest = line.split("source=\"").nth(1)?;
            rest.split('"').next().map(str::to_owned)
        });
        source
            .and_then(|source| fixtures.get(&source).cloned())
            .or_else(|| fixtures.values().next().cloned())
            .unwrap_or(Value::Null)
    }
}

/// The extension a file belongs to: the nearest directory above it that
/// holds an `extension.kdl`, and the file's path inside it.
fn extension_of(path: &Path) -> Option<(PathBuf, String)> {
    let mut dir = path.parent()?;
    loop {
        if dir.join("extension.kdl").is_file() {
            let relative = path.strip_prefix(dir).ok()?;
            let relative = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            return Some((dir.to_path_buf(), relative));
        }
        dir = dir.parent()?;
    }
}

/// A `file:` URI's path, percent-decoded.
fn path_of(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(byte) = encoded
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    let text = String::from_utf8(decoded).ok()?;
    // `file:///C:/…` on Windows: the drive follows the third slash.
    let text = match text.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => text[1..].to_owned(),
        _ => text,
    };
    Some(PathBuf::from(text))
}

/// The `file:` URI of a path, with what a URI may not hold escaped.
fn uri_of(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::from("file://");
    if !text.starts_with('/') {
        encoded.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                encoded.push(char::from(byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests;
