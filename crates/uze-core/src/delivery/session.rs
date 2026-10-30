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
///
/// It arrives from outside UZE — a harness's own store, which the agent
/// can write — and leaves as a word of the harness's command line and a
/// file name, so only [`SessionId::parse`] makes one, and stored records
/// are read through it too: a value shaped like an option or a path is
/// never a conversation.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId(String);

/// Longer than any harness's identifier, short enough to bound a file name.
const LONGEST: usize = 128;

/// A value that is not a conversation identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotASessionId(String);

impl fmt::Display for NotASessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} is not a conversation identifier",
            self.0.chars().take(LONGEST).collect::<String>()
        )
    }
}

impl std::error::Error for NotASessionId {}

impl SessionId {
    /// An identifier of 1 to 128 ASCII letters, digits, `-` and `_` that
    /// begins with a letter or a digit: every shape a harness uses, and
    /// none that a command line reads as an option or a file system as a
    /// path.
    pub fn parse(value: impl Into<String>) -> Result<Self, NotASessionId> {
        let value = value.into();
        let well_formed = value.len() <= LONGEST
            && value
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphanumeric())
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            });
        if well_formed {
            Ok(Self(value))
        } else {
            Err(NotASessionId(value))
        }
    }

    /// Whether this is an RFC 4122 UUID in its hyphenated text form, the
    /// shape most harnesses name a conversation in.
    pub fn is_uuid(&self) -> bool {
        let bytes = self.0.as_bytes();
        bytes.len() == 36
            && bytes.iter().enumerate().all(|(at, byte)| match at {
                8 | 13 | 18 | 23 => *byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            })
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

impl TryFrom<String> for SessionId {
    type Error = NotASessionId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SessionId> for String {
    fn from(session: SessionId) -> Self {
        session.0
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
        assert!(id.is_uuid());
        assert_eq!(SessionId::parse(text).as_ref(), Ok(&id));
    }

    /// An identifier is read back from a store an agent can write and
    /// placed in a harness's command line and in a file name: nothing
    /// shaped like an option, a path or more than one word is one.
    #[test]
    fn a_value_shaped_like_an_option_or_a_path_is_not_an_identifier() {
        for refused in [
            "",
            "--dangerously-bypass-approvals-and-sandbox",
            "-r",
            "_leading",
            "../escape",
            "a/b",
            "a\\b",
            "two words",
            "tab\tbed",
            "line\nbreak",
            "C:drive",
            "key=value",
            "ünïcode",
            &"x".repeat(129),
        ] {
            assert!(
                SessionId::parse(refused).is_err(),
                "{refused:?} was accepted"
            );
        }
        for accepted in [
            "019a5c2e-81c0-7800-8aa1-bbab2a035a53",
            "ses_3b8fZq0",
            "c-1",
            &"x".repeat(128),
        ] {
            assert!(
                SessionId::parse(accepted).is_ok(),
                "{accepted:?} was refused"
            );
        }
    }

    #[test]
    fn a_stored_identifier_is_read_through_the_same_check() {
        let refused: Result<SessionId, _> = serde_json::from_str("\"--resume\"");
        assert!(refused.is_err());
        let read: SessionId = serde_json::from_str("\"ses_1\"").unwrap();
        assert_eq!(serde_json::to_string(&read).unwrap(), "\"ses_1\"");
    }

    #[test]
    fn only_the_hyphenated_form_is_a_uuid() {
        let uuid = |text: &str| SessionId::parse(text).unwrap().is_uuid();
        assert!(uuid("01a18a77-749a-7a01-a296-197f1ad4016c"));
        assert!(!uuid("01a18a77749a7a01a296197f1ad4016c"));
        assert!(!uuid("01a18a77-749a-7a01-a296-197f1ad4016"));
        assert!(!uuid("ses_1"));
    }
}
