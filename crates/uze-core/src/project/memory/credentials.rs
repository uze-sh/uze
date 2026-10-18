//! What a credential pasted into a fact looks like.
//!
//! Best effort, and said to be: these are the shapes an agent pastes by
//! accident — a provider's token with its fixed prefix and length, a
//! private key block — matched with their whole token body and at a word
//! boundary, so prose that merely contains `sk-` or `AKIA` is left alone.
//! A secret with no recognisable shape is not caught.

/// A credential shape found, and the line it is on, counted from 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Found {
    pub shape: &'static str,
    pub line: usize,
}

/// The first provider credential in `text`.
pub fn credential(text: &str) -> Option<Found> {
    text.lines().enumerate().find_map(|(index, line)| {
        shape_in(line).map(|shape| Found {
            shape,
            line: index + 1,
        })
    })
}

/// The first assignment in `text` whose name says it holds a secret and
/// whose value looks like one rather than a placeholder: written, with a
/// caution, because documentation names such variables legitimately.
pub fn secret_assignment(text: &str) -> Option<usize> {
    text.lines()
        .position(|line| line.split_whitespace().any(looks_assigned))
        .map(|index| index + 1)
}

fn shape_in(line: &str) -> Option<&'static str> {
    if line.contains("-----BEGIN") && line.contains("PRIVATE KEY-----") {
        return Some("a private key");
    }
    let starts = line.char_indices().filter(|&(index, _)| {
        line[..index]
            .chars()
            .next_back()
            .is_none_or(|before| !is_token(before))
    });
    for (start, _) in starts {
        let rest = &line[start..];
        for (prefix, body, exact, shape) in PROVIDERS {
            if let Some(token) = rest.strip_prefix(prefix) {
                let length = token.chars().take_while(|c| body(*c)).count();
                let fits = match exact {
                    Some(exact) => {
                        length == *exact
                            && token[length..].chars().next().is_none_or(|c| !is_token(c))
                    }
                    None => length >= MINIMUM_BODY,
                };
                if fits {
                    return Some(shape);
                }
            }
        }
    }
    None
}

/// The fewest characters a token with no fixed length carries past its
/// prefix: enough that a word like `sk-learn` is never one.
const MINIMUM_BODY: usize = 20;

type Provider = (&'static str, fn(char) -> bool, Option<usize>, &'static str);

/// Prefix, the characters its body is made of, its exact length where the
/// provider fixes one, and what it is called. Longer prefixes first, so
/// `sk-ant-` is read as itself before `sk-`.
const PROVIDERS: &[Provider] = &[
    ("github_pat_", is_word, None, "a GitHub token"),
    ("ghp_", is_alphanumeric, Some(36), "a GitHub token"),
    ("gho_", is_alphanumeric, Some(36), "a GitHub token"),
    ("ghu_", is_alphanumeric, Some(36), "a GitHub token"),
    ("ghs_", is_alphanumeric, Some(36), "a GitHub token"),
    ("ghr_", is_alphanumeric, Some(36), "a GitHub token"),
    ("glpat-", is_token, None, "a GitLab token"),
    ("xoxa-", is_token, None, "a Slack token"),
    ("xoxb-", is_token, None, "a Slack token"),
    ("xoxp-", is_token, None, "a Slack token"),
    ("xoxr-", is_token, None, "a Slack token"),
    ("xoxs-", is_token, None, "a Slack token"),
    ("AKIA", is_upper_or_digit, Some(16), "an AWS access key"),
    ("sk-ant-", is_token, None, "an API key"),
    ("sk-proj-", is_token, None, "an API key"),
    ("sk-", is_token, None, "an API key"),
];

fn is_alphanumeric(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_upper_or_digit(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit()
}

/// The fewest characters a value must have to look like a secret.
const SECRET_VALUE: usize = 16;

fn looks_assigned(word: &str) -> bool {
    let Some((name, value)) = word.split_once('=').or_else(|| word.split_once(':')) else {
        return false;
    };
    let name = name
        .trim_matches(|c: char| !is_word(c))
        .to_ascii_uppercase();
    let value = value.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ',' | ';'));
    let named = ["KEY", "TOKEN", "SECRET", "PASSWORD"]
        .iter()
        .any(|end| name.ends_with(end));
    let placeholder = value.starts_with(['<', '$', '*'])
        || value
            .chars()
            .all(|c| c == 'x' || c == 'X' || c == '*' || c == '.');
    named
        && !placeholder
        && value.len() >= SECRET_VALUE
        && value.chars().any(|c| c.is_ascii_alphabetic())
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().all(is_token_value)
}

/// What a value pasted as a secret is made of; a call or an expression
/// (`compute_key()`) is not one.
fn is_token_value(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assembled at runtime: the repository's secret scanning reads the
    /// source, and a literal token here would be one more for it to find.
    fn token(prefix: &str, body: &str, length: usize) -> String {
        format!(
            "{prefix}{}",
            body.repeat(length).chars().take(length).collect::<String>()
        )
    }

    #[test]
    fn each_provider_shape_is_found_with_its_line() {
        let cases = [
            (
                token("gh", "p_", 2) + &token("", "a1B2", 36),
                "a GitHub token",
            ),
            (
                token("github_", "pat_", 4) + &token("", "a1_", 40),
                "a GitHub token",
            ),
            (
                token("glpat", "-", 1) + &token("", "x9", 20),
                "a GitLab token",
            ),
            (
                token("xox", "b-", 2) + &token("", "12-ab", 24),
                "a Slack token",
            ),
            (
                token("AK", "IA", 2) + &token("", "Z9", 16),
                "an AWS access key",
            ),
            (
                token("sk", "-ant-", 5) + &token("", "api03_", 40),
                "an API key",
            ),
            (token("sk", "-", 1) + &token("", "Ab3", 32), "an API key"),
            (
                format!("-----BEGIN {}PRIVATE KEY-----", "OPENSSH "),
                "a private key",
            ),
        ];
        for (secret, shape) in cases {
            let text = format!("first line\nthe value is {secret} here\n");
            assert_eq!(
                credential(&text),
                Some(Found { shape, line: 2 }),
                "{secret}"
            );
        }
    }

    #[test]
    fn prose_that_only_contains_a_prefix_is_left_alone() {
        for text in [
            "never-mask-exit-codes and ask-for-approval, task-id, disk-only",
            "the sk-learn library and AKIA as a word",
            "a short ghp_abc that is not a token",
            "risk-assessment-of-the-whole-thing-is-long",
        ] {
            assert_eq!(credential(text), None, "{text}");
        }
    }

    #[test]
    fn documentation_of_a_variable_is_not_a_secret() {
        for text in [
            "OPENAI_API_KEY=<your-api-key>",
            "UZE_AGENT_KEY=$UZE_AGENT_KEY",
            "TOKEN=${GITHUB_TOKEN}",
            "let primary_key = compute_key();",
            "PASSWORD=xxxxxxxxxxxxxxxxxxxx",
            "API_KEY=short1",
        ] {
            assert_eq!(secret_assignment(text), None, "{text}");
        }
        let assigned = format!("export\nDEPLOY_TOKEN={}", "f00ba4".repeat(4));
        assert_eq!(secret_assignment(&assigned), Some(2));
    }
}
