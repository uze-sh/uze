//! The forge a repository is published to.

use super::*;

/// Which family of forge `origin` points at, and therefore which word the
/// thing UZE publishes goes by there.
///
/// One thing has two names — a pull request and a merge request — and a
/// surface that picks one blind takes a side its reader may not be on.
/// Asking the remote removes the guess: the word is the one the reader's
/// own forge writes, and where the remote does not say, nothing is
/// claimed and `#` stands alone as it always did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Forge {
    /// The remote says nothing this recognizes — a bare repository, a
    /// self-hosted host named after the company rather than the product.
    #[default]
    Unknown,
    GitHub,
    GitLab,
}

impl Forge {
    /// The forge's own word for a request, for prose that has room for a
    /// word. `None` where the remote did not say, and then the sentence
    /// must name both or neither.
    pub const fn request_term(self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some("pull request"),
            Self::GitLab => Some("merge request"),
            Self::Unknown => None,
        }
    }

    /// The two-letter form, for a zone with room for two letters and
    /// nothing to put in them — a request that has no number yet. Once
    /// there is a number the number says everything, and the word in
    /// front of it is only length.
    pub const fn request_abbreviation(self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some("PR"),
            Self::GitLab => Some("MR"),
            Self::Unknown => None,
        }
    }

    /// Reads the family off a remote URL.
    ///
    /// The host is the whole of the evidence, in both spellings Git
    /// accepts — `https://host/owner/repo` and `git@host:owner/repo` —
    /// and a label match rather than an exact one, so an enterprise
    /// `github.acme.com` and a self-hosted `gitlab.acme.com` answer like
    /// the hosted product they are. A host naming neither is `Unknown`:
    /// guessing from a path or a protocol would be inventing an answer
    /// the reader has to check.
    ///
    /// The path is cut off before the user is, not after: an `@` is legal
    /// in a path, and taking the last one in the whole URL reads a tag
    /// as a host.
    pub fn from_remote_url(url: &str) -> Self {
        let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
        let authority = after_scheme.split('/').next().unwrap_or_default();
        let authority = authority
            .rsplit_once('@')
            .map_or(authority, |(_, rest)| rest);
        let host = authority
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match host
            .split('.')
            .find(|label| matches!(*label, "github" | "gitlab"))
        {
            Some("github") => Self::GitHub,
            Some("gitlab") => Self::GitLab,
            _ => Self::Unknown,
        }
    }
}

/// The forge this repository's `origin` points at.
///
/// A property of the project rather than of a task, so a surface drawing
/// many rows asks once. Cheap — one `git remote get-url` — but still a
/// process, so it belongs where the rest of a view's Git reads are and
/// never on a render path.
pub fn forge(primary: &Path) -> Forge {
    crate::git::read(primary, &["remote", "get-url", REMOTE])
        .ok()
        .and_then(|output| output.successful().ok())
        .map(|url| Forge::from_remote_url(url.trim()))
        .unwrap_or_default()
}
