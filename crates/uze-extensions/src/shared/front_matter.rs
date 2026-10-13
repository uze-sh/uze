//! The YAML block a document may open with, between two `---` lines.

/// The front matter of `text` and what follows it, or no front matter and
/// all of `text` when it does not open with one.
///
/// Only a line that is exactly `---` opens or closes the block: `----` is
/// a Markdown rule and `---foo` is text. A line may end in `\r\n`, since a
/// record written on Windows is the same record.
pub fn split(text: &str) -> (&str, &str) {
    let opened = text.trim_start();
    let Some(rest) = opened
        .strip_prefix("---\n")
        .or_else(|| opened.strip_prefix("---\r\n"))
    else {
        return ("", text);
    };
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "---" {
            let front = rest[..at].strip_suffix('\n').unwrap_or(&rest[..at]);
            let front = front.strip_suffix('\r').unwrap_or(front);
            return (front, &rest[at + line.len()..]);
        }
        at += line.len();
    }
    ("", text)
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn a_block_between_two_rules_is_split_from_the_body() {
        assert_eq!(split("---\ntitle: A\n---\nbody\n"), ("title: A", "body\n"));
        assert_eq!(split("\n---\n---\nbody"), ("", "body"));
        assert_eq!(split("---\ntitle: A\n---"), ("title: A", ""));
    }

    #[test]
    fn crlf_line_endings_are_the_same_block() {
        assert_eq!(
            split("---\r\ntitle: A\r\nstatus: x\r\n---\r\nbody\r\n"),
            ("title: A\r\nstatus: x", "body\r\n")
        );
    }

    #[test]
    fn only_a_line_of_exactly_three_dashes_opens_or_closes() {
        for text in ["----\ntitle: A\n----\nbody", "---foo\ntitle: A\n---\nbody"] {
            assert_eq!(split(text), ("", text), "{text:?} opens nothing");
        }
        let text = "---\ntitle: A\n----\nbody\n";
        assert_eq!(split(text), ("", text), "a longer rule closes nothing");
        assert_eq!(
            split("---\ntitle: A\n---x\n---\nbody"),
            ("title: A\n---x", "body")
        );
    }

    #[test]
    fn a_block_never_closed_is_no_front_matter() {
        let text = "---\ntitle: A\nbody\n";
        assert_eq!(split(text), ("", text));
    }
}
