//! A component file, `<name>.uze`: a `<view>` block of KDL and a `<script>`
//! block of Rhai, each at most once (`extension-components` D1, D2).
//!
//! The one reader of the block rules, for every crate that meets a
//! component: the compile, the check, the highlighter and the language
//! server. A tag is alone on its line and unindented; trailing whitespace,
//! a carriage return and a byte-order mark are read past. Outside the
//! blocks only blank lines and `//` comments stand.
//!
//! A block is handed to its compile as the whole file with every byte
//! outside it turned into a space and every newline kept, so each offset,
//! span and position a compile reports is the file's own.

/// A component's suffix.
pub const SUFFIX: &str = "uze";

/// The largest component, in bytes: a script's bound, since a script is
/// what a component may hold most of. Its `<view>` is bounded as any view.
pub const COMPONENT_MAX: usize = 256 * 1024;

/// Whether `file` names a component.
pub fn is_component(file: &str) -> bool {
    file.strip_suffix(SUFFIX)
        .is_some_and(|stem| stem.ends_with('.') && stem.len() > 1)
}

/// What a block holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockKind {
    /// KDL: a view.
    View,
    /// Rhai: a script.
    Script,
}

impl BlockKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Script => "script",
        }
    }
}

/// A tag line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tag {
    Open(BlockKind),
    Close(BlockKind),
}

/// The tag `line` is, if it is one: the line without its newline, its
/// trailing whitespace and carriage return, and, on the first line, a
/// byte-order mark.
pub fn tag_of(line: &str) -> Option<Tag> {
    match line.trim_start_matches('\u{feff}').trim_end() {
        "<view>" => Some(Tag::Open(BlockKind::View)),
        "</view>" => Some(Tag::Close(BlockKind::View)),
        "<script>" => Some(Tag::Open(BlockKind::Script)),
        "</script>" => Some(Tag::Close(BlockKind::Script)),
        _ => None,
    }
}

/// Where a block's text lies in its file: from the line after its opening
/// tag to the start of its closing tag's line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    /// The offset of its opening tag.
    pub tag: usize,
    pub start: usize,
    pub end: usize,
}

impl Block {
    /// Its own bytes, without its tags.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A component's blocks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Component {
    pub view: Option<Block>,
    pub script: Option<Block>,
}

/// Why a file is not a component, and where.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentError {
    pub offset: usize,
    pub message: String,
    pub suggestion: Option<String>,
}

impl Component {
    /// The blocks of `text`, or everything wrong with it.
    pub fn read(text: &str) -> Result<Self, Vec<ComponentError>> {
        let mut component = Self::default();
        let mut errors = Vec::new();
        let mut open: Option<(BlockKind, usize, usize)> = None;
        let mut at = 0;
        for line in text.split_inclusive('\n') {
            let next = at + line.len();
            match (open, tag_of(line)) {
                (Some((kind, tag, start)), Some(Tag::Close(closing))) if closing == kind => {
                    let block = Block {
                        kind,
                        tag,
                        start,
                        end: at,
                    };
                    match kind {
                        BlockKind::View => component.view = Some(block),
                        BlockKind::Script => component.script = Some(block),
                    }
                    open = None;
                }
                (Some((kind, _, _)), Some(other)) => {
                    let (Tag::Open(named) | Tag::Close(named)) = other;
                    errors.push(ComponentError {
                        offset: at,
                        message: format!("`{}` inside `<{}>`", line.trim(), kind.name()),
                        suggestion: Some(if named == kind {
                            format!("a `<{}>` block holds no other", kind.name())
                        } else {
                            format!("close `<{}>` with `</{}>` first", kind.name(), kind.name())
                        }),
                    });
                }
                (Some(_), None) => {}
                (None, Some(Tag::Open(kind))) => {
                    let seen = match kind {
                        BlockKind::View => component.view.is_some(),
                        BlockKind::Script => component.script.is_some(),
                    };
                    if seen {
                        errors.push(ComponentError {
                            offset: at,
                            message: format!("a second `<{}>` block", kind.name()),
                            suggestion: Some(format!(
                                "a component holds one `<{}>`; join the two",
                                kind.name()
                            )),
                        });
                    }
                    open = Some((kind, at, next));
                }
                (None, Some(Tag::Close(kind))) => errors.push(ComponentError {
                    offset: at,
                    message: format!("`</{}>` closes no block", kind.name()),
                    suggestion: None,
                }),
                (None, None) => {
                    if let Some(error) = outside(line, at) {
                        errors.push(error);
                    }
                }
            }
            at = next;
        }
        if let Some((kind, tag, _)) = open {
            errors.push(ComponentError {
                offset: tag,
                message: format!("`<{}>` is never closed", kind.name()),
                suggestion: Some(format!(
                    "end it with `</{}>` alone on its line",
                    kind.name()
                )),
            });
        }
        if errors.is_empty() && component.view.is_none() && component.script.is_none() {
            errors.push(ComponentError {
                offset: 0,
                message: "holds no `<view>` and no `<script>`".to_owned(),
                suggestion: Some(
                    "write the view between `<view>` and `</view>`, the script between \
                     `<script>` and `</script>`, each tag alone on its line"
                        .to_owned(),
                ),
            });
        }
        if errors.is_empty() {
            Ok(component)
        } else {
            Err(errors)
        }
    }

    /// The block of `kind`, if the component holds one.
    pub fn block(&self, kind: BlockKind) -> Option<Block> {
        match kind {
            BlockKind::View => self.view,
            BlockKind::Script => self.script,
        }
    }
}

/// What a line outside every block may be: blank, or a `//` comment.
fn outside(line: &str, at: usize) -> Option<ComponentError> {
    let text = line.trim_start_matches('\u{feff}').trim();
    if text.is_empty() || text.starts_with("//") {
        return None;
    }
    let indented = line.trim_start_matches('\u{feff}').starts_with([' ', '\t']);
    let (message, suggestion) = if text.starts_with("/*") {
        (
            "a `/* */` comment outside a block".to_owned(),
            "outside the blocks, comment with `//`, whose end is the line's".to_owned(),
        )
    } else if indented && tag_of(text).is_some() {
        (
            format!("`{text}` is indented"),
            "a tag stands at the start of its line".to_owned(),
        )
    } else {
        (
            "text outside a block".to_owned(),
            "move it into `<view>` or `<script>`; outside them only blank lines and `//` \
             comments stand"
                .to_owned(),
        )
    };
    Some(ComponentError {
        offset: at,
        message,
        suggestion: Some(suggestion),
    })
}

/// `text` with every byte outside `block` turned into a space, newlines
/// kept: a block read where it stands, every offset the file's. Each byte
/// becomes one ASCII byte, so a character of many bytes outside the block
/// leaves as many spaces and the offsets still hold.
pub fn blanked(text: &str, block: Block) -> String {
    let blank = |part: &str| -> String {
        part.bytes()
            .map(|byte| if byte == b'\n' { '\n' } else { ' ' })
            .collect()
    };
    let mut out = blank(&text[..block.start]);
    out.push_str(&text[block.start..block.end]);
    out.push_str(&blank(&text[block.end..]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL: &str = "// The drawer.\n<view>\nStack {\n    Text \"{month}\"\n}\n</view>\n\n<script>\nfn prev(input, context) { #{ data: () } }\n</script>\n";

    fn messages(text: &str) -> Vec<String> {
        Component::read(text)
            .unwrap_err()
            .into_iter()
            .map(|error| error.message)
            .collect()
    }

    #[test]
    fn a_component_holds_a_view_and_a_script_where_they_stand() {
        let component = Component::read(PANEL).unwrap();
        let view = component.view.unwrap();
        assert_eq!(
            &PANEL[view.start..view.end],
            "Stack {\n    Text \"{month}\"\n}\n"
        );
        let script = component.script.unwrap();
        assert!(PANEL[script.start..script.end].starts_with("fn prev"));
    }

    #[test]
    fn a_blanked_block_keeps_every_offset_and_line_of_the_file() {
        let component = Component::read(PANEL).unwrap();
        let view = component.view.unwrap();
        let blanked = blanked(PANEL, view);
        assert_eq!(blanked.len(), PANEL.len());
        assert_eq!(blanked.lines().count(), PANEL.lines().count());
        assert_eq!(&blanked[view.start..view.end], &PANEL[view.start..view.end]);
        assert!(!blanked.contains("<view>") && !blanked.contains("fn prev"));
        let at = PANEL.find("Text").unwrap();
        assert_eq!(&blanked[at..at + 4], "Text");
    }

    #[test]
    fn a_character_of_many_bytes_inside_or_outside_survives_the_blanking() {
        let text = "// Lançamentos\n<script>\nlet s = \"horas lançadas ⏱\";\n</script>\n";
        let script = Component::read(text).unwrap().script.unwrap();
        let blanked = blanked(text, script);
        assert_eq!(blanked.len(), text.len());
        assert!(blanked.contains("horas lançadas ⏱"));
        assert!(!blanked.contains("Lançamentos"));
    }

    #[test]
    fn windows_line_endings_trailing_space_and_a_byte_order_mark_are_read_past() {
        let text = "\u{feff}<view>\r\nText \"a\"\r\n</view>  \r\n";
        let view = Component::read(text).unwrap().view.unwrap();
        assert_eq!(&text[view.start..view.end], "Text \"a\"\r\n");
    }

    #[test]
    fn what_breaks_the_rules_is_said_at_its_line() {
        assert_eq!(messages("Text \"a\"\n"), ["text outside a block"]);
        assert_eq!(
            messages("/* x */\n<view>\n</view>\n"),
            ["a `/* */` comment outside a block"]
        );
        assert_eq!(messages("  <view>\n"), ["`<view>` is indented"]);
        assert_eq!(
            messages("<view>\nText \"a\"\n"),
            ["`<view>` is never closed"]
        );
        assert_eq!(
            messages("<view>\n<script>\n</script>\n</view>\n"),
            ["`<script>` inside `<view>`", "`</script>` inside `<view>`"]
        );
        assert_eq!(
            messages("<view>\n</view>\n<view>\n</view>\n"),
            ["a second `<view>` block"]
        );
        assert_eq!(messages("</script>\n"), ["`</script>` closes no block"]);
        assert_eq!(
            messages("// only a comment\n"),
            ["holds no `<view>` and no `<script>`"]
        );
    }

    #[test]
    fn the_offset_of_an_error_is_the_start_of_its_line() {
        let errors = Component::read("<view>\n</view>\nstray\n").unwrap_err();
        assert_eq!(errors[0].offset, "<view>\n</view>\n".len());
    }
}
