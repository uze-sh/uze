//! What a trace may keep of a value somebody typed.
//!
//! `uze market add https://user:token@host/market` is a shape people use,
//! so a credential arriving in a span field, an error or Git's argument
//! line is an ordinary input, not a mistake — and a journal an operator
//! attaches to a report, or a collector somebody else runs, is exactly the
//! place it must not survive. One rule, here, because every layer that
//! records such a value (`uze-git`, `uze-core`, the application facade,
//! the binary's own telemetry) builds on this crate and on no other common
//! one; two copies of it had already drifted apart.
//!
//! Not an operating-system question: domain-free text, kept in the one
//! leaf they all share.

/// What stands where a secret was.
pub const REDACTED: &str = "<redacted>";

/// `text` with every credential it carries replaced by [`REDACTED`], word
/// by word, keeping everything else — whitespace included — as it was.
///
/// Two shapes are credentials:
/// - the userinfo of a `scheme://` authority. Only the authority holds it,
///   and the host follows its *last* `@`: a password may itself carry one,
///   and a path or reference after the host (`repo@v1`) is not userinfo.
/// - a word that is a token a provider mints, standing on its own. Only
///   shapes a provider actually mints: a length-and-alphabet guess would
///   redact the digests and package ids a trace exists to show.
pub fn redact(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for word in text.split_inclusive(char::is_whitespace) {
        let bare = word.trim_end();
        if looks_minted(bare) {
            result.push_str(REDACTED);
            result.push_str(&word[bare.len()..]);
        } else {
            redact_userinfo(word, &mut result);
        }
    }
    result
}

fn redact_userinfo(word: &str, into: &mut String) {
    let Some(scheme) = word.find("://") else {
        into.push_str(word);
        return;
    };
    let start = scheme + 3;
    let rest = &word[start..];
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    match authority.rfind('@') {
        Some(at) => {
            into.push_str(&word[..start]);
            into.push_str(REDACTED);
            into.push('@');
            into.push_str(&word[start + at + 1..]);
        }
        None => into.push_str(word),
    }
}

/// Whether `word` is a credential standing on its own.
fn looks_minted(word: &str) -> bool {
    const MINTED: [&str; 7] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "xox"];
    MINTED
        .iter()
        .any(|prefix| word.starts_with(prefix) && word.len() > prefix.len() + 8)
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn the_userinfo_of_a_url_is_redacted_and_the_rest_kept() {
        assert_eq!(
            redact("market add team https://romullo:ghp_0123456789abcdef@github.com/org/m.git"),
            "market add team https://<redacted>@github.com/org/m.git"
        );
        assert_eq!(
            redact("foo@https://token@host/market"),
            "foo@https://<redacted>@host/market"
        );
        assert_eq!(
            redact("failed to clone https://user:secret@example.com/repo.git now"),
            "failed to clone https://<redacted>@example.com/repo.git now"
        );
    }

    #[test]
    fn the_host_follows_the_last_at_of_the_authority_and_nothing_past_it() {
        let redacted = redact("cannot reach https://user:p@ss@example.com/repo.git");
        assert_eq!(
            redacted,
            "cannot reach https://<redacted>@example.com/repo.git"
        );
        assert_eq!(
            redact("https://github.com/org/a@b/market"),
            "https://github.com/org/a@b/market",
            "an `@` in the path is not userinfo"
        );
        assert_eq!(redact("plugin@market"), "plugin@market");
        assert_eq!(redact("https://h/x?u=a@b"), "https://h/x?u=a@b");
    }

    #[test]
    fn a_minted_token_standing_alone_is_redacted() {
        assert_eq!(
            redact("install -m ghp_0123456789abcdefghij\n"),
            "install -m <redacted>\n"
        );
        assert_eq!(
            redact("agent work name fix/ghp_short"),
            "agent work name fix/ghp_short",
            "a word that merely starts like a token is not one"
        );
        assert_eq!(redact("ghp_short"), "ghp_short");
    }

    #[test]
    fn text_with_nothing_to_hide_is_left_as_it_was() {
        let text = "  git -C /tmp/x fetch --quiet origin\tmain\n";
        assert_eq!(redact(text), text);
    }
}
