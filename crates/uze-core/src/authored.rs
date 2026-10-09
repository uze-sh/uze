//! What a person wrote, read as they wrote it.

/// `bytes` without the UTF-8 byte-order mark an editor may have put first.
/// Windows PowerShell 5.1 writes one with `-Encoding UTF8`, as do older
/// editors, and it is not part of the document: JSON's grammar leaves it
/// out, and a parser handed it reports the file as unreadable at line 1.
pub fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// Parses a JSON document a person wrote: a manifest, a hook or server
/// declaration, a marketplace.
pub fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    serde_json::from_slice(without_bom(bytes))
}

/// Whether `character` would act on a terminal rather than be drawn on it:
/// the C0 controls (save the line feed and tab text is laid out with), DEL,
/// the C1 controls, and the bidirectional overrides and isolates that
/// reorder what follows them.
pub fn is_terminal_control(character: char) -> bool {
    matches!(character,
        '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'
        | '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}')
}

/// The first character in `text` that [`is_terminal_control`], or a line
/// break or tab where `single_line` says there can be none: what a value
/// read from a file another person wrote is refused for.
pub fn control_character(text: &str, single_line: bool) -> Option<char> {
    text.chars().find(|character| {
        is_terminal_control(*character) || (single_line && matches!(character, '\n' | '\t'))
    })
}

/// `text` with every terminal control spelled out visibly (`\u{1b}`)
/// instead of acting: the one form text somebody else wrote reaches a
/// terminal in. Line feeds and tabs are kept; [`inert_line`] escapes them
/// too, for a value that must stay on the line it is printed on.
pub fn inert(text: &str) -> std::borrow::Cow<'_, str> {
    escaped(text, is_terminal_control)
}

/// [`inert`] for a value printed on one line: a line break or tab inside
/// it is spelled out as well, so it cannot start a line that reads as
/// UZE's own.
pub fn inert_line(text: &str) -> std::borrow::Cow<'_, str> {
    escaped(text, |character| {
        is_terminal_control(character) || matches!(character, '\n' | '\t')
    })
}

fn escaped(text: &str, acts: impl Fn(char) -> bool) -> std::borrow::Cow<'_, str> {
    if !text.chars().any(&acts) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut visible = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if acts(character) {
            visible.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        } else {
            visible.push(character);
        }
    }
    std::borrow::Cow::Owned(visible)
}

/// Where in `value` a key or string carries a terminal control, said as
/// the reason a file declaring it is refused. A command, an argument or a
/// name is shown to the person asked to trust it, and one that can rewrite
/// the line it is shown on cannot be judged.
pub fn terminal_control_in(value: &serde_json::Value) -> Option<String> {
    fn walk(value: &serde_json::Value, at: &str) -> Option<String> {
        let refused = |text: &str, at: &str| {
            control_character(text, false).map(|character| {
                format!(
                    "`{}` holds the control character U+{:04X}, which no value may carry",
                    inert_line(at),
                    u32::from(character)
                )
            })
        };
        match value {
            serde_json::Value::String(text) => refused(text, at),
            serde_json::Value::Array(items) => items
                .iter()
                .enumerate()
                .find_map(|(index, item)| walk(item, &format!("{at}[{index}]"))),
            serde_json::Value::Object(entries) => entries.iter().find_map(|(key, item)| {
                let here = if at.is_empty() {
                    key.clone()
                } else {
                    format!("{at}.{key}")
                };
                refused(key, &here).or_else(|| walk(item, &here))
            }),
            _ => None,
        }
    }
    walk(value, "")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_control_anywhere_in_a_document_is_named_by_where_it_is() {
        let clean =
            serde_json::json!({ "mcpServers": { "a": { "command": "x", "args": ["--y"] } } });
        assert_eq!(super::terminal_control_in(&clean), None);
        let tainted =
            serde_json::json!({ "mcpServers": { "a": { "args": ["ok", "\u{1b}]0;x\u{7}"] } } });
        let reason = super::terminal_control_in(&tainted).unwrap();
        assert!(reason.contains("mcpServers.a.args[1]"), "{reason}");
        assert!(reason.contains("U+001B"), "{reason}");
        let in_key = serde_json::json!({ "mcpServers": { "a\u{202e}b": {} } });
        assert!(
            super::terminal_control_in(&in_key)
                .unwrap()
                .contains("U+202E")
        );
    }

    /// An escape sequence, a C1 introducer and a right-to-left override are
    /// shown as what they are, never performed.
    #[test]
    fn terminal_controls_are_spelled_out_never_performed() {
        assert_eq!(
            super::inert("ok\u{1b}[2K\rfake\u{9b}x\u{202e}y\nnext"),
            "ok\\u{1b}[2K\\u{d}fake\\u{9b}x\\u{202e}y\nnext"
        );
        assert_eq!(super::inert_line("a\nb\tc"), "a\\u{a}b\\u{9}c");
        assert!(matches!(
            super::inert("plain café"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(super::control_character("x\u{7}", false), Some('\u{7}'));
        assert_eq!(super::control_character("a\nb", false), None);
        assert_eq!(super::control_character("a\nb", true), Some('\n'));
    }

    #[test]
    fn a_document_reads_the_same_with_or_without_a_byte_order_mark() {
        let plain: serde_json::Value = super::json(br#"{"name":"guard"}"#).unwrap();
        let marked: serde_json::Value = super::json(b"\xEF\xBB\xBF{\"name\":\"guard\"}").unwrap();
        assert_eq!(plain, marked);
        assert!(super::json::<serde_json::Value>(b"\xEF\xBB\xBF").is_err());
    }
}
