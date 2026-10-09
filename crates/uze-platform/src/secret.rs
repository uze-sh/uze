//! Secrets: bytes nobody can predict, from the operating system's own
//! generator, and the comparison that does not say how close a guess came.
//!
//! The identifiers elsewhere in UZE are digests of time and pid: unique on
//! one machine and readable in a path, and exactly as predictable as the
//! clock. A value that decides who may do something is made here instead.

use std::io;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

/// How many bytes a [`token`] carries: enough that guessing one is not a
/// strategy.
pub const TOKEN_BYTES: usize = 32;

/// Fills `buffer` from the operating system's generator.
pub fn fill(buffer: &mut [u8]) -> io::Result<()> {
    imp::fill(buffer)
}

/// A fresh secret of [`TOKEN_BYTES`], as lowercase hex: text that survives
/// a file, an environment variable and a wire unchanged.
pub fn token() -> io::Result<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Whether `offered` is `expected`, taking as long for a guess that shares
/// a prefix as for one that shares nothing.
pub fn matches(offered: &[u8], expected: &[u8]) -> bool {
    if offered.len() != expected.len() {
        return false;
    }
    offered
        .iter()
        .zip(expected)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn two_tokens_are_never_the_same() {
        let first = super::token().unwrap();
        let second = super::token().unwrap();
        assert_eq!(first.len(), 2 * super::TOKEN_BYTES);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn only_the_same_bytes_match() {
        assert!(super::matches(b"abc", b"abc"));
        assert!(!super::matches(b"abd", b"abc"));
        assert!(!super::matches(b"ab", b"abc"));
        assert!(!super::matches(b"", b"abc"));
    }
}
