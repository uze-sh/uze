//! The one line an operator types overrides into, split into words the way
//! a POSIX shell would split it, and nothing more: no variable, glob, tilde
//! or command is expanded, because nothing typed here reaches a shell.

use std::fmt;

/// A line that cannot be split: a quote opened and never closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Unterminated {
    /// `'` or `"`.
    pub quote: char,
    /// The byte offset of the opening quote, so the line can show where.
    pub at: usize,
}

impl fmt::Display for Unterminated {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "the {} opened here is never closed", self.quote)
    }
}

impl std::error::Error for Unterminated {}

/// The words of `line`. Single quotes keep everything up to the next one;
/// double quotes keep everything but `\"`, `\\`, `` \` `` and `\$`, which
/// stand for the character escaped; outside quotes a backslash stands for
/// the character after it, and one ending the line for itself.
pub fn split(line: &str) -> Result<Vec<String>, Unterminated> {
    let mut words = Vec::new();
    let mut word = String::new();
    // A word can be empty (`''`), so whether one is open is its own fact.
    let mut open = false;
    let mut characters = line.char_indices().peekable();
    while let Some((at, character)) = characters.next() {
        match character {
            '\'' => {
                open = true;
                loop {
                    match characters.next() {
                        Some((_, '\'')) => break,
                        Some((_, quoted)) => word.push(quoted),
                        None => return Err(Unterminated { quote: '\'', at }),
                    }
                }
            }
            '"' => {
                open = true;
                loop {
                    match characters.next() {
                        Some((_, '"')) => break,
                        Some((_, '\\')) => match characters.peek() {
                            Some(&(_, escaped @ ('"' | '\\' | '`' | '$'))) => {
                                word.push(escaped);
                                characters.next();
                            }
                            _ => word.push('\\'),
                        },
                        Some((_, quoted)) => word.push(quoted),
                        None => return Err(Unterminated { quote: '"', at }),
                    }
                }
            }
            '\\' => {
                open = true;
                word.push(characters.next().map_or('\\', |(_, escaped)| escaped));
            }
            blank if blank.is_whitespace() => {
                if open {
                    words.push(std::mem::take(&mut word));
                    open = false;
                }
            }
            other => {
                open = true;
                word.push(other);
            }
        }
    }
    if open {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        split(line).unwrap()
    }

    #[test]
    fn words_are_separated_by_any_run_of_whitespace() {
        assert_eq!(
            words("  DEBUG=1 \t--model   opus "),
            ["DEBUG=1", "--model", "opus"]
        );
        assert!(words("   ").is_empty());
    }

    #[test]
    fn single_quotes_keep_everything_literally() {
        assert_eq!(words(r#"'a "b" \c $D'"#), [r#"a "b" \c $D"#]);
    }

    #[test]
    fn double_quotes_take_only_their_own_escapes() {
        assert_eq!(
            words(r#""say \"hi\" \\ \$HOME C:\dir""#),
            [r#"say "hi" \ $HOME C:\dir"#]
        );
    }

    #[test]
    fn a_backslash_outside_quotes_escapes_the_next_character() {
        assert_eq!(words(r"a\ b c\\d e\"), ["a b", r"c\d", r"e\"]);
    }

    #[test]
    fn quoted_parts_join_the_word_they_touch() {
        assert_eq!(
            words(r#"--append-system-prompt="be brief"x"#),
            ["--append-system-prompt=be briefx"]
        );
        assert_eq!(words("NAME='' next"), ["NAME=", "next"]);
        assert_eq!(words("'' \"\""), ["", ""]);
    }

    #[test]
    fn nothing_is_expanded() {
        assert_eq!(words("$HOME ~ * $(id)"), ["$HOME", "~", "*", "$(id)"]);
    }

    #[test]
    fn an_unterminated_quote_says_which_and_where() {
        assert_eq!(
            split("--model 'opus"),
            Err(Unterminated { quote: '\'', at: 8 })
        );
        let error = split(r#"A="b"#).unwrap_err();
        assert_eq!(error, Unterminated { quote: '"', at: 2 });
        assert_eq!(error.to_string(), "the \" opened here is never closed");
    }
}
