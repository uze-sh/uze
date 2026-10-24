//! Between the protocol's places (a zero-based line and a column counted in
//! UTF-16 units) and the byte offsets and one-based character columns the
//! extension decoder speaks.

/// The byte offset of a protocol position in `text`, clamped to the line.
pub(super) fn offset_of(text: &str, line: u32, utf16_column: u32) -> usize {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(newline) => start += newline + 1,
            None => return text.len(),
        }
    }
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |newline| start + newline);
    let mut units = 0;
    for (at, ch) in text[start..end].char_indices() {
        if units >= utf16_column as usize {
            return start + at;
        }
        units += ch.len_utf16();
    }
    end
}

/// The protocol position of a one-based line and character column, as a
/// diagnostic places it. Line `0` is the whole file: its first character.
pub(super) fn position_of(text: &str, line: usize, column: usize) -> (u32, u32) {
    if line == 0 {
        return (0, 0);
    }
    let Some(row) = text.split('\n').nth(line - 1) else {
        return (u32::try_from(line - 1).unwrap_or(u32::MAX), 0);
    };
    let units: usize = row
        .chars()
        .take(column.saturating_sub(1))
        .map(char::len_utf16)
        .sum();
    (
        u32::try_from(line - 1).unwrap_or(u32::MAX),
        u32::try_from(units).unwrap_or(u32::MAX),
    )
}

/// The identifier around byte `offset`: letters, digits, `-`, `_` and `.`.
pub(super) fn word_at(text: &str, offset: usize) -> &str {
    let offset = offset.min(text.len());
    let is_word = |ch: char| ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.');
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_word(*ch))
        .last()
        .map_or(offset, |(at, _)| at);
    let end = text[offset..]
        .char_indices()
        .find(|(_, ch)| !is_word(*ch))
        .map_or(text.len(), |(at, _)| offset + at);
    &text[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_protocol_column_counts_utf16_units() {
        let text = "Text \"é😀x\"\nRow";
        assert_eq!(offset_of(text, 0, 7), 8, "after `é`, two bytes");
        assert_eq!(offset_of(text, 0, 9), 12, "after the emoji, two units");
        assert_eq!(offset_of(text, 1, 1), 16);
        assert_eq!(offset_of(text, 9, 0), text.len());
        assert_eq!(position_of(text, 1, 9), (0, 9));
        assert_eq!(position_of(text, 2, 2), (1, 1));
        assert_eq!(position_of(text, 0, 0), (0, 0));
    }

    #[test]
    fn the_word_under_the_cursor_is_an_identifier() {
        let text = "    Progress ratio=0.5";
        assert_eq!(word_at(text, 7), "Progress");
        assert_eq!(word_at(text, 15), "ratio");
        assert_eq!(word_at(text, 0), "");
    }
}
