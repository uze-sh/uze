//! An estimate of how many tokens a text costs a model, made offline.
//!
//! Not any vendor's tokenizer — each model has its own, and none ships with
//! uze. The Overview ranks and compares resources, which an estimate within
//! about 15% serves; the figure is always shown as one. A real BPE table
//! was weighed and refused: megabytes in the binary for a number that would
//! still not be the vendor's.
//!
//! The text is read as runs: a Latin word is about one token and a long one
//! a little more, digits go about three to a token, a run of punctuation
//! about two characters to a token, an ideograph or kana somewhat less than
//! one, and a word in another script about a token per three letters.

/// Estimated tokens in `text`.
pub fn estimate_tokens(text: &str) -> u32 {
    let mut total = 0.0_f64;
    let mut chars = text.chars().peekable();
    while let Some(first) = chars.next() {
        if first.is_whitespace() {
            continue;
        }
        let class = Class::of(first);
        let mut length = 1usize;
        if class.runs() {
            while chars
                .peek()
                .is_some_and(|next| !next.is_whitespace() && Class::of(*next) == class)
            {
                chars.next();
                length += 1;
            }
        }
        total += class.cost(length);
    }
    total.round() as u32
}

/// Estimated tokens of a file too large to read whole, from its size.
pub fn estimate_from_size(bytes: u64) -> u32 {
    (bytes / 4).min(u64::from(u32::MAX)) as u32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Class {
    Latin,
    Digit,
    Ideograph,
    OtherLetter,
    Punctuation,
}

impl Class {
    fn of(c: char) -> Self {
        if c.is_ascii_alphabetic() {
            Self::Latin
        } else if c.is_ascii_digit() {
            Self::Digit
        } else if matches!(c as u32, 0x3000..=0x9FFF | 0xAC00..=0xD7AF) {
            Self::Ideograph
        } else if c.is_alphabetic() {
            Self::OtherLetter
        } else {
            Self::Punctuation
        }
    }

    /// An ideograph is a token of its own; every other class runs on.
    fn runs(self) -> bool {
        self != Self::Ideograph
    }

    fn cost(self, length: usize) -> f64 {
        let length = length as f64;
        match self {
            Self::Latin => 1.0 + ((length - 8.0) / 6.0).max(0.0),
            Self::Digit => ((length + 2.0) / 3.0).floor(),
            Self::Ideograph => 0.6,
            Self::OtherLetter => 1.0 + length / 3.0,
            Self::Punctuation => (length / 2.0).max(1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference counts from `o200k_base`, the encoding of current OpenAI
    /// models, computed once outside the build: the estimate has to stay
    /// within 15% of each across the scripts the Overview meets.
    const CALIBRATION: [(&str, u32); 4] = [
        (
            "Make code ready for review - put a branch, a module or a whole codebase through six lenses (correctness, security, architecture, performance, quality, tests) with reviewers that did not write it, prove each finding, and fix the confirmed ones in small verified steps. Use before opening a merge or pull request, after finishing an implementation, or when asked to review an implementation and fix what turns up.",
            83,
        ),
        (
            "Everything under the home directory belongs to exactly one tier, decided by what deleting it costs, never by which module wrote it. A thing must not sit in a tier that claims a different cost than it has. Only records declare a shape. Generated and remembered things are produced or observed again when they cannot be read.",
            63,
        ),
        (
            "Revisa o código desta branch, encontra os problemas de correção e de segurança, e corrige os confirmados em passos pequenos e verificados antes de abrir o pull request.",
            36,
        ),
        (
            "日本語のテキストを数えます。これはテストです。コードを確認して、問題を修正してください。",
            27,
        ),
    ];

    #[test]
    fn the_estimate_stays_within_fifteen_percent_of_a_real_tokenizer() {
        for (text, reference) in CALIBRATION {
            let estimate = f64::from(estimate_tokens(text));
            let reference = f64::from(reference);
            let error = (estimate - reference).abs() / reference;
            assert!(
                error <= 0.15,
                "{estimate} against {reference} ({:.0}%) for {text:?}",
                error * 100.0
            );
        }
    }

    #[test]
    fn nothing_costs_nothing() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("  \n\t "), 0);
    }

    #[test]
    fn a_file_too_large_to_read_is_estimated_from_its_size() {
        assert_eq!(estimate_from_size(4_000), 1_000);
    }
}
