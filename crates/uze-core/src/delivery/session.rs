//! The identifier a harness gives a conversation.
//!
//! Shared rather than the workspace's: [`IntegrationPort`] speaks it, because
//! resuming a conversation is something a harness does, and a vertical that
//! only delivers plugins still implements the contract that names it. What
//! is remembered about a conversation, and for which agent, belongs to the
//! workspace; the name itself does not.
//!
//! [`IntegrationPort`]: crate::integration::IntegrationPort

use std::{
    fmt, fs,
    io::Read,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::digest;

/// A conversation as its harness names it. Opaque here on purpose: the
/// only code entitled to read it is the integration that resumes with it.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A fresh RFC 4122 version 4 identifier, for a harness that lets UZE
    /// name the conversation it is about to start. The default shape, not a
    /// universal one: a harness that refuses a UUID mints its own name
    /// (`IntegrationPort::new_session_id`).
    pub fn generate() -> Self {
        Self(format_uuid_v4(random_bytes()))
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What a harness's own records say a conversation was about, in terms no
/// vendor owns: read by the integration that wrote the transcript, written
/// into a briefing by the workspace. Every field is optional, and so is the
/// whole: a transcript UZE cannot read is a briefing made from Git alone.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConversationDigest {
    /// The operator's first turn, which is what the work is for.
    pub goal: Option<String>,
    /// The harness's own latest compaction summary, when it wrote one.
    /// Turns it already covers are not repeated in `turns`.
    pub summary: Option<String>,
    /// Operator turns and the agent's own text, oldest first, without tool
    /// output, reasoning or subagent turns.
    pub turns: Vec<Turn>,
    /// The last command the agent ran.
    pub last_command: Option<LastCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Turn {
    pub speaker: Speaker,
    pub text: String,
    /// When it was said, where the transcript records it.
    pub at_unix: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Speaker {
    Operator,
    Agent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LastCommand {
    pub line: String,
    /// `None` where the transcript does not say how it ended.
    pub failed: Option<bool>,
}

fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if let Ok(mut source) = fs::File::open("/dev/urandom")
        && source.read_exact(&mut bytes).is_ok()
    {
        return bytes;
    }
    // Only without `/dev/urandom`: unique within this process by the counter
    // and across processes by the pid and the clock, which is all a name a
    // harness keys one conversation by has to be.
    static MINTED: AtomicU64 = AtomicU64::new(0);
    let seed = digest::fnv1a64(
        format!(
            "{}:{}",
            std::process::id(),
            MINTED.fetch_add(1, Ordering::Relaxed)
        )
        .as_bytes(),
    );
    let nanos = uze_platform::clock::since_epoch().as_nanos() as u64;
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..].copy_from_slice(&nanos.to_le_bytes());
    bytes
}

fn format_uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_identifier_is_a_version_4_uuid() {
        let id = SessionId::generate();
        let text = id.as_str();
        assert_eq!(text.len(), 36, "{text}");
        assert_eq!(
            text.chars().filter(|c| *c == '-').count(),
            4,
            "grouping: {text}"
        );
        assert_eq!(&text[14..15], "4", "version nibble: {text}");
        assert!(
            matches!(&text[19..20], "8" | "9" | "a" | "b"),
            "variant nibble: {text}"
        );
        assert_ne!(SessionId::generate(), SessionId::generate());
    }
}
