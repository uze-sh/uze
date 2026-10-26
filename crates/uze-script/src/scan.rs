//! Which host functions a script names, read from its text.
//!
//! Walking Rhai's syntax tree needs its `internals` feature, which D19 never
//! enables, so the check reads the tokens itself: every identifier outside a
//! comment, and every string whose whole text is a host function's name
//! (what `Fn("read_checkout")` would call). It errs towards naming too much;
//! the engine, which registers only what was granted, is the real bound.

use std::collections::BTreeSet;

use crate::reach::{Grant, Grants, HOST_FUNCTIONS};

/// The host functions `source` names.
pub fn host_functions_of(source: &str) -> BTreeSet<String> {
    let mut words = BTreeSet::new();
    let mut scanner = Scanner {
        text: source.as_bytes(),
        at: 0,
        words: &mut words,
    };
    scanner.code(false);
    HOST_FUNCTIONS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| words.contains(*name))
        .map(str::to_owned)
        .collect()
}

/// The host functions `source` names that `grants` do not register, each
/// with the grant it needs: what the check refuses.
pub fn ungranted_calls(source: &str, grants: &Grants) -> Vec<(String, Grant)> {
    let named = host_functions_of(source);
    HOST_FUNCTIONS
        .iter()
        .filter_map(|(name, grant)| Some((*name, (*grant)?)))
        .filter(|(name, grant)| named.contains(*name) && !grants.contains(grant))
        .map(|(name, grant)| (name.to_owned(), grant))
        .collect()
}

struct Scanner<'a> {
    text: &'a [u8],
    at: usize,
    words: &'a mut BTreeSet<String>,
}

impl Scanner<'_> {
    fn peek(&self, ahead: usize) -> Option<u8> {
        self.text.get(self.at + ahead).copied()
    }

    /// Code until the end, or until the `}` closing an interpolation.
    fn code(&mut self, interpolated: bool) {
        let mut braces = 0usize;
        while let Some(byte) = self.peek(0) {
            match byte {
                b'/' if self.peek(1) == Some(b'/') => self.line_comment(),
                b'/' if self.peek(1) == Some(b'*') => self.block_comment(),
                b'"' => self.quoted(b'"'),
                b'\'' => self.quoted(b'\''),
                b'`' => self.template(),
                b'#' if matches!(self.peek(1), Some(b'#' | b'"')) => self.raw(),
                b'{' => {
                    braces += 1;
                    self.at += 1;
                }
                b'}' if interpolated && braces == 0 => {
                    self.at += 1;
                    return;
                }
                b'}' => {
                    braces = braces.saturating_sub(1);
                    self.at += 1;
                }
                byte if byte == b'_' || byte.is_ascii_alphabetic() => self.word(),
                byte if byte.is_ascii_digit() => self.number(),
                _ => self.at += 1,
            }
        }
    }

    fn word(&mut self) {
        let start = self.at;
        while self
            .peek(0)
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
        {
            self.at += 1;
        }
        self.remember(start, self.at);
    }

    fn number(&mut self) {
        while self
            .peek(0)
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
        {
            self.at += 1;
        }
    }

    fn remember(&mut self, start: usize, end: usize) {
        if let Ok(word) = std::str::from_utf8(&self.text[start..end]) {
            self.words.insert(word.to_owned());
        }
    }

    fn line_comment(&mut self) {
        while self.peek(0).is_some_and(|byte| byte != b'\n') {
            self.at += 1;
        }
    }

    /// Rhai's block comments nest.
    fn block_comment(&mut self) {
        let mut depth = 0usize;
        while let Some(byte) = self.peek(0) {
            if byte == b'/' && self.peek(1) == Some(b'*') {
                depth += 1;
                self.at += 2;
            } else if byte == b'*' && self.peek(1) == Some(b'/') {
                depth -= 1;
                self.at += 2;
                if depth == 0 {
                    return;
                }
            } else {
                self.at += 1;
            }
        }
    }

    fn quoted(&mut self, quote: u8) {
        self.at += 1;
        let start = self.at;
        while let Some(byte) = self.peek(0) {
            if byte == b'\\' {
                self.at += 2;
            } else if byte == quote {
                self.remember(start, self.at);
                self.at += 1;
                return;
            } else {
                self.at += 1;
            }
        }
    }

    /// A back-quoted string, whose `${…}` is code.
    fn template(&mut self) {
        self.at += 1;
        while let Some(byte) = self.peek(0) {
            match byte {
                b'\\' => self.at += 2,
                b'`' => {
                    self.at += 1;
                    return;
                }
                b'$' if self.peek(1) == Some(b'{') => {
                    self.at += 2;
                    self.code(true);
                }
                _ => self.at += 1,
            }
        }
    }

    /// `#"…"#`, closed by a quote and as many hashes as opened it.
    fn raw(&mut self) {
        let mut hashes = 0;
        while self.peek(0) == Some(b'#') {
            hashes += 1;
            self.at += 1;
        }
        if self.peek(0) != Some(b'"') {
            return;
        }
        self.at += 1;
        let start = self.at;
        while let Some(byte) = self.peek(0) {
            if byte == b'"' && (1..=hashes).all(|offset| self.peek(offset) == Some(b'#')) {
                self.remember(start, self.at);
                self.at += 1 + hashes;
                return;
            }
            self.at += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(source: &str) -> Vec<String> {
        host_functions_of(source).into_iter().collect()
    }

    #[test]
    fn a_call_is_named_and_a_comment_is_not() {
        let source = "// read_checkout\n/* list_checkout /* nested */ clock */\nfn answer(i, c) { read_checkout(\"a\") }";
        assert_eq!(named(source), ["read_checkout"]);
    }

    #[test]
    fn a_string_naming_a_function_counts_and_prose_does_not() {
        assert_eq!(named(r#"let f = Fn("list_checkout");"#), ["list_checkout"]);
        assert!(named(r#"let s = "call read_checkout later";"#).is_empty());
    }

    #[test]
    fn code_inside_an_interpolation_is_read() {
        assert_eq!(named("let s = `x ${ clock() } y`;"), ["clock"]);
    }

    #[test]
    fn a_method_call_is_a_call() {
        assert_eq!(named(r#""notes".read_checkout()"#), ["read_checkout"]);
    }

    #[test]
    fn an_ungranted_call_names_the_grant_it_needs() {
        let source = "fn answer(i, c) { #{ data: list_checkout(\"\"), at: clock() } }";
        assert_eq!(
            ungranted_calls(source, &Grants::new()),
            [("list_checkout".to_owned(), Grant::ReadCheckout)]
        );
        assert!(ungranted_calls(source, &Grants::from([Grant::ReadCheckout])).is_empty());
    }
}
