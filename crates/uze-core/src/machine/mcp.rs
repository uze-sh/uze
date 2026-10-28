//! A stdio MCP client, enough for an extension to call a tool of a server
//! a plugin delivers (`extension-mcp` D4, D6).
//!
//! JSON-RPC 2.0, one message per line: `initialize`, the `initialized`
//! notification, then `tools/call` as asked. What the server sends that is
//! not the answer awaited is read past: a notification is ignored and a
//! request is answered "method not found", since this client offers the
//! server nothing. Every line is bounded, and so is what is kept of its
//! standard error, which is quoted when it ends.
//!
//! The process runs in a group of its own and the whole group is ended
//! when the session is dropped, as a program's is.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uze_platform::process::{Seat, Tree, spawn_tree};

/// The protocol version this client asks for. A server answering another
/// is spoken to all the same: `tools/call` has not changed shape.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The longest line a server may write: one message.
pub const LINE_MAX: usize = 4 << 20;

/// The most of a result a call hands back, as JSON.
pub const RESULT_MAX: usize = 1 << 20;

/// What is kept of the server's standard error, its last bytes.
pub const STDERR_KEPT: usize = 64 << 10;

/// How much of standard error an error quotes.
const STDERR_QUOTED: usize = 512;

/// How a server is started: what a harness would be handed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpLaunch {
    pub command: PathBuf,
    pub args: Vec<String>,
    /// Everything it sees in its environment.
    pub environment: Vec<(String, OsString)>,
    pub cwd: PathBuf,
}

/// Why a call answered nothing.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum McpError {
    #[error("the server could not be started: {0}")]
    NotStarted(String),
    #[error("the server ended{}", quoted(.0))]
    Closed(String),
    #[error("the server answered nothing in {} s", .0.as_secs())]
    TimedOut(Duration),
    #[error("the server spoke out of protocol: {0}")]
    Protocol(String),
    #[error("{0}")]
    Server(String),
    /// The tool ran and said it failed.
    #[error("{0}")]
    Tool(String),
    #[error("the result is over {} KiB", RESULT_MAX >> 10)]
    TooLarge,
}

impl McpError {
    /// Whether the session can be asked again after this: a failure of
    /// the tool or a refusal by the server leaves it as it was.
    pub fn leaves_session(&self) -> bool {
        matches!(self, Self::Server(_) | Self::Tool(_) | Self::TooLarge)
    }
}

fn quoted(stderr: &str) -> String {
    let tail = stderr.trim();
    if tail.is_empty() {
        return String::new();
    }
    let start = tail
        .char_indices()
        .map(|(at, _)| at)
        .find(|at| tail.len() - at <= STDERR_QUOTED)
        .unwrap_or(0);
    format!(": {}", &tail[start..])
}

/// What arrives from the server, one line at a time.
enum Line {
    Message(String),
    TooLong,
    Closed,
}

/// The protocol over a writer and the lines read back, apart from any
/// process, so it is tested on its own.
struct Wire {
    writer: Box<dyn Write + Send>,
    lines: Receiver<Line>,
    next_id: u64,
}

impl Wire {
    fn send(&mut self, message: &Value) -> Result<(), McpError> {
        let mut bytes = serde_json::to_vec(message).expect("a JSON value serializes");
        bytes.push(b'\n');
        self.writer
            .write_all(&bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|_| McpError::Closed(String::new()))
    }

    fn notify(&mut self, method: &str) -> Result<(), McpError> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }))
    }

    /// Sends `method` and waits until `deadline` for its answer.
    fn request(
        &mut self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, McpError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(Line::Message(line)) => line,
                Ok(Line::TooLong) => {
                    return Err(McpError::Protocol(format!(
                        "a message over {} MiB",
                        LINE_MAX >> 20
                    )));
                }
                Ok(Line::Closed) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(McpError::Closed(String::new()));
                }
                Err(RecvTimeoutError::Timeout) => return Err(McpError::TimedOut(Duration::ZERO)),
            };
            if line.trim().is_empty() {
                continue;
            }
            let message: Value = serde_json::from_str(&line).map_err(|error| {
                McpError::Protocol(format!("a line that is not JSON ({error})"))
            })?;
            if let Some(asked) = message.get("method") {
                // A request of the server's own, which this client serves
                // none of; a notification needs no answer.
                if let Some(their_id) = message.get("id") {
                    self.send(&json!({
                        "jsonrpc": "2.0",
                        "id": their_id,
                        "error": { "code": -32601, "message": format!("{asked} is not served here") },
                    }))?;
                }
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let said = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("the server refused the request");
                return Err(McpError::Server(said.to_owned()));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| McpError::Protocol("an answer with no result".to_owned()));
        }
    }

    fn initialize(&mut self, deadline: Instant) -> Result<(), McpError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "uze", "version": env!("CARGO_PKG_VERSION") },
            }),
            deadline,
        )?;
        self.notify("notifications/initialized")
    }

    fn call(&mut self, tool: &str, arguments: Value, deadline: Instant) -> Result<Value, McpError> {
        let result = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
            deadline,
        )?;
        result_value(&result)
    }
}

/// What a tool's result hands a script: its structured content when it
/// has one; else its text, read as JSON when the whole of it is JSON. A
/// result the tool marks as an error is the error, in its own words.
pub fn result_value(result: &Value) -> Result<Value, McpError> {
    let text: String = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(McpError::Tool(if text.is_empty() {
            "the tool failed and said nothing".to_owned()
        } else {
            text
        }));
    }
    let value = match result.get("structuredContent") {
        Some(structured) if !structured.is_null() => structured.clone(),
        _ => serde_json::from_str(text.trim()).unwrap_or(Value::String(text)),
    };
    if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > RESULT_MAX) {
        return Err(McpError::TooLarge);
    }
    Ok(value)
}

/// One server, started and initialized, ended when dropped.
pub struct McpSession {
    wire: Wire,
    child: Child,
    tree: Tree,
    stderr: Arc<Mutex<VecDeque<u8>>>,
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl McpSession {
    /// Starts the server `launch` describes and initializes it within
    /// `within`.
    pub fn start(launch: &McpLaunch, within: Duration) -> Result<Self, McpError> {
        let mut command = Command::new(&launch.command);
        command
            .args(&launch.args)
            .current_dir(&launch.cwd)
            .env_clear()
            .envs(launch.environment.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (mut child, tree) = spawn_tree(&mut command, Seat::NoTerminal).map_err(|error| {
            McpError::NotStarted(format!("{}: {error}", launch.command.display()))
        })?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        keep_tail(child.stderr.take().expect("piped"), Arc::clone(&stderr));
        let mut session = Self {
            wire: Wire {
                writer: Box::new(stdin),
                lines: read_lines(stdout),
                next_id: 0,
            },
            child,
            tree,
            stderr,
        };
        session
            .wire
            .initialize(Instant::now() + within)
            .map_err(|error| session.explained(error, within))?;
        Ok(session)
    }

    /// Calls `tool` with `arguments`, waiting at most `within`.
    pub fn call(
        &mut self,
        tool: &str,
        arguments: Value,
        within: Duration,
    ) -> Result<Value, McpError> {
        let answered = self.wire.call(tool, arguments, Instant::now() + within);
        answered.map_err(|error| self.explained(error, within))
    }

    /// `error` with what only the session knows: the time it waited, and
    /// what the server last said on standard error when it ended.
    fn explained(&self, error: McpError, within: Duration) -> McpError {
        match error {
            McpError::TimedOut(_) => McpError::TimedOut(within),
            McpError::Closed(_) => {
                // Its last words may still be in the pipe as it dies.
                thread::sleep(Duration::from_millis(50));
                let kept = self
                    .stderr
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let (front, back) = kept.as_slices();
                let bytes = [front, back].concat();
                McpError::Closed(String::from_utf8_lossy(&bytes).into_owned())
            }
            other => other,
        }
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        // Ends the group twice, the second time while the child is dead
        // but unreaped, so no pid in it can have been reused (the
        // `ending-processes` rule).
        self.tree.end();
        uze_platform::process::wait_without_reaping(self.child.id());
        self.tree.end();
        let _ = self.child.wait();
    }
}

/// Reads `stdout` line by line on a thread of its own, each bounded.
fn read_lines(stdout: impl Read + Send + 'static) -> Receiver<Line> {
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            let read = (&mut reader)
                .take(LINE_MAX as u64 + 1)
                .read_until(b'\n', &mut line);
            let sent = match read {
                Ok(0) | Err(_) => {
                    let _ = sender.send(Line::Closed);
                    return;
                }
                Ok(_) if line.len() > LINE_MAX => sender.send(Line::TooLong),
                Ok(_) => sender.send(Line::Message(String::from_utf8_lossy(&line).into_owned())),
            };
            if sent.is_err() {
                return;
            }
        }
    });
    lines
}

/// Keeps the last [`STDERR_KEPT`] bytes of `stderr`, read on a thread.
fn keep_tail(mut stderr: impl Read + Send + 'static, kept: Arc<Mutex<VecDeque<u8>>>) {
    thread::spawn(move || {
        let mut chunk = [0_u8; 4096];
        while let Ok(read) = stderr.read(&mut chunk) {
            if read == 0 {
                return;
            }
            let mut tail = kept
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            tail.extend(&chunk[..read]);
            let over = tail.len().saturating_sub(STDERR_KEPT);
            tail.drain(..over);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wire whose server is `serve`, answering each request it reads
    /// with the lines it returns.
    fn wire(serve: impl Fn(&Value) -> Vec<String> + Send + 'static) -> Wire {
        let (from_client, to_server) = std::io::pipe().unwrap();
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(from_client).lines() {
                let Ok(line) = line else { return };
                let asked: Value = serde_json::from_str(&line).unwrap();
                for reply in serve(&asked) {
                    if sender.send(Line::Message(reply)).is_err() {
                        return;
                    }
                }
            }
            let _ = sender.send(Line::Closed);
        });
        Wire {
            writer: Box::new(to_server),
            lines,
            next_id: 0,
        }
    }

    fn answer(asked: &Value, result: Value) -> String {
        json!({ "jsonrpc": "2.0", "id": asked["id"], "result": result }).to_string()
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn a_tool_is_called_after_the_handshake_and_its_json_text_is_data() {
        let mut wire = wire(|asked| match asked["method"].as_str() {
            Some("initialize") => {
                assert_eq!(asked["params"]["protocolVersion"], PROTOCOL_VERSION);
                vec![answer(
                    asked,
                    json!({ "protocolVersion": "2025-03-26", "capabilities": {} }),
                )]
            }
            Some("tools/call") => {
                assert_eq!(asked["params"]["name"], "rtc_timesheet");
                vec![
                    json!({ "jsonrpc": "2.0", "method": "notifications/progress" }).to_string(),
                    answer(
                        asked,
                        json!({ "content": [{ "type": "text", "text": "{\"total\": 96}" }] }),
                    ),
                ]
            }
            _ => Vec::new(),
        });
        wire.initialize(soon()).unwrap();
        let value = wire
            .call("rtc_timesheet", json!({ "month": "2026-10" }), soon())
            .unwrap();
        assert_eq!(value, json!({ "total": 96 }));
    }

    #[test]
    fn a_request_of_the_servers_own_is_refused_and_the_wait_goes_on() {
        let mut wire = wire(|asked| match asked["method"].as_str() {
            Some("tools/call") => {
                vec![json!({ "jsonrpc": "2.0", "id": "s1", "method": "roots/list" }).to_string()]
            }
            None => {
                assert_eq!(asked["id"], "s1");
                assert_eq!(asked["error"]["code"], -32601);
                vec![answer(
                    &json!({ "id": 1 }),
                    json!({ "content": [{ "type": "text", "text": "plain words" }] }),
                )]
            }
            _ => Vec::new(),
        });
        assert_eq!(
            wire.call("t", json!({}), soon()).unwrap(),
            json!("plain words")
        );
    }

    #[test]
    fn a_tool_error_a_server_error_and_silence_each_say_so() {
        let mut wire = wire(|asked| {
            match asked["params"]["name"].as_str() {
            Some("fails") => vec![answer(
                asked,
                json!({ "isError": true, "content": [{ "type": "text", "text": "RTC said 401" }] }),
            )],
            Some("unknown") => vec![json!({
                "jsonrpc": "2.0", "id": asked["id"], "error": { "code": -32602, "message": "no tool unknown" },
            })
            .to_string()],
            _ => Vec::new(),
        }
        });
        assert_eq!(
            wire.call("fails", json!({}), soon()),
            Err(McpError::Tool("RTC said 401".to_owned()))
        );
        assert_eq!(
            wire.call("unknown", json!({}), soon()),
            Err(McpError::Server("no tool unknown".to_owned()))
        );
        let silent = wire.call(
            "silent",
            json!({}),
            Instant::now() + Duration::from_millis(50),
        );
        assert!(matches!(silent, Err(McpError::TimedOut(_))), "{silent:?}");
    }

    #[test]
    fn structured_content_wins_and_a_large_result_is_refused() {
        let both = json!({
            "structuredContent": { "days": [] },
            "content": [{ "type": "text", "text": "ignored" }],
        });
        assert_eq!(result_value(&both), Ok(json!({ "days": [] })));
        let huge = "x".repeat(RESULT_MAX + 1);
        let large = json!({ "content": [{ "type": "text", "text": huge }] });
        assert_eq!(result_value(&large), Err(McpError::TooLarge));
    }

    // The server stand-in is a POSIX `sh` one-liner.
    #[cfg(unix)]
    #[test]
    fn a_server_that_dies_is_reported_with_its_last_words() {
        let launch = McpLaunch {
            command: PathBuf::from("/bin/sh"),
            args: vec![
                "-c".into(),
                "read line; echo 'login failed: no NETWORK_USER' >&2; exit 1".into(),
            ],
            environment: Vec::new(),
            cwd: std::env::temp_dir(),
        };
        let error = McpSession::start(&launch, Duration::from_secs(5)).unwrap_err();
        assert_eq!(
            error,
            McpError::Closed("login failed: no NETWORK_USER\n".to_owned())
        );
        assert!(error.to_string().ends_with("no NETWORK_USER"), "{error}");
    }
}
