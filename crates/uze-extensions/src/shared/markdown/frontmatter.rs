//! A document's frontmatter, read as rows of a key/value table.
//!
//! Read by its shape — indentation, `key:`, `- item`, `[section]` — and
//! never parsed: a parser answers "this is not valid YAML", and a preview
//! has nothing to do with that answer but show less. What it cannot make
//! sense of becomes part of the value above it, so no text is lost.

use crate::view::Role;

/// One row: a key, how deep it sits under the keys above it, and its
/// value flattened to one line. A key that only opens a nested mapping
/// has an empty value.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub depth: usize,
    pub key: String,
    pub value: String,
}

/// A `---` block. Nested mappings become indented rows, a sequence is
/// joined into its key's value, and a block scalar (`|`, `>`) is folded
/// onto one line.
pub(super) fn yaml(source: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    // The indentation of each key the current line could be nested under.
    let mut parents: Vec<usize> = Vec::new();
    // A block scalar being read: the indentation of the key that opened it.
    let mut block: Option<usize> = None;

    for line in source.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if let Some(opener) = block {
            if trimmed.is_empty() || indent > opener {
                if let Some(entry) = entries.last_mut() {
                    append(&mut entry.value, trimmed, " ");
                }
                continue;
            }
            block = None;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or((trimmed == "-").then_some(""))
        {
            if let Some(entry) = entries.last_mut() {
                append(&mut entry.value, &unquote(item), ", ");
            }
            continue;
        }
        let Some((key, value)) = key_value(trimmed) else {
            if let Some(entry) = entries.last_mut() {
                append(&mut entry.value, trimmed, " ");
            }
            continue;
        };
        while parents.last().is_some_and(|&parent| parent >= indent) {
            parents.pop();
        }
        let depth = parents.len();
        parents.push(indent);
        let value = if is_block_scalar(value) {
            block = Some(indent);
            String::new()
        } else {
            flow(value)
        };
        entries.push(Entry {
            depth,
            key: unquote(key),
            value,
        });
    }
    entries
}

/// A `+++` block: `key = value`, with each `[section]` a row of its own
/// and its keys nested under it.
pub(super) fn toml(source: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut in_section = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(section) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_section = true;
            entries.push(Entry {
                depth: 0,
                key: section.trim_matches(['[', ']']).trim().to_owned(),
                value: String::new(),
            });
            continue;
        }
        match trimmed.split_once('=') {
            Some((key, value)) => entries.push(Entry {
                depth: usize::from(in_section),
                key: unquote(key.trim()),
                value: flow(value.trim()),
            }),
            None => {
                if let Some(entry) = entries.last_mut() {
                    let piece = trimmed.trim_matches([',', ']']).trim();
                    append(&mut entry.value, &unquote(piece), ", ");
                }
            }
        }
    }
    entries
}

/// How a value reads: a literal (`true`, `42`) apart from words.
pub(super) fn value_role(value: &str) -> Role {
    let literal = matches!(value, "true" | "false" | "null" | "~")
        || (!value.is_empty() && value.parse::<f64>().is_ok());
    if literal { Role::Accent } else { Role::Default }
}

/// `key: value`, where the colon is followed by a space or ends the line —
/// so a URL or a time inside a plain value does not split it.
fn key_value(line: &str) -> Option<(&str, &str)> {
    if let Some(key) = line.strip_suffix(':') {
        return Some((key.trim(), ""));
    }
    let (key, value) = line.split_once(": ")?;
    ((!key.is_empty() && !key.contains(' ')) || key.starts_with(['"', '\'']))
        .then(|| (key.trim(), value.trim()))
}

fn is_block_scalar(value: &str) -> bool {
    value.starts_with(['|', '>'])
        && value[1..]
            .chars()
            .all(|modifier| matches!(modifier, '+' | '-' | '0'..='9'))
}

/// A flow sequence (`[a, b]`) as the list it is, anything else unquoted.
fn flow(value: &str) -> String {
    match value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        Some(items) => items
            .split(',')
            .map(|item| unquote(item.trim()))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        None => unquote(value),
    }
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_owned();
        }
    }
    text.to_owned()
}

fn append(value: &mut String, piece: &str, separator: &str) {
    if piece.is_empty() {
        return;
    }
    if !value.is_empty() {
        value.push_str(separator);
    }
    value.push_str(piece);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(entries: &[Entry]) -> Vec<(usize, &str, &str)> {
        entries
            .iter()
            .map(|entry| (entry.depth, entry.key.as_str(), entry.value.as_str()))
            .collect()
    }

    #[test]
    fn keys_keep_the_order_the_author_wrote_them_in() {
        let entries = yaml("name: greet\ndescription: \"Says hi\"\nversion: 2\n");
        assert_eq!(
            rows(&entries),
            [
                (0, "name", "greet"),
                (0, "description", "Says hi"),
                (0, "version", "2")
            ]
        );
    }

    #[test]
    fn a_nested_mapping_is_rows_indented_under_its_key() {
        let entries = yaml("invoke:\n  model: true\n  user: false\nname: x\n");
        assert_eq!(
            rows(&entries),
            [
                (0, "invoke", ""),
                (1, "model", "true"),
                (1, "user", "false"),
                (0, "name", "x"),
            ]
        );
    }

    #[test]
    fn a_sequence_joins_into_its_key_in_either_spelling() {
        let block = yaml("tags:\n  - a\n  - \"b\"\n");
        let flowed = yaml("tags: [a, 'b']\n");
        assert_eq!(rows(&block), [(0, "tags", "a, b")]);
        assert_eq!(rows(&flowed), [(0, "tags", "a, b")]);
    }

    #[test]
    fn a_block_scalar_is_folded_onto_its_key() {
        let entries = yaml("description: >-\n  Long text\n  over lines.\nname: x\n");
        assert_eq!(
            rows(&entries),
            [
                (0, "description", "Long text over lines."),
                (0, "name", "x")
            ]
        );
    }

    /// A colon is only a key's when a space or the line's end follows it.
    #[test]
    fn a_url_in_a_value_does_not_split_it() {
        let entries = yaml("homepage: https://uze.sh/docs\n");
        assert_eq!(rows(&entries), [(0, "homepage", "https://uze.sh/docs")]);
    }

    #[test]
    fn toml_sections_nest_their_keys() {
        let entries = toml("title = \"Hi\"\n[extra]\nweight = 3\n");
        assert_eq!(
            rows(&entries),
            [(0, "title", "Hi"), (0, "extra", ""), (1, "weight", "3")]
        );
    }

    #[test]
    fn literals_read_apart_from_words() {
        assert_eq!(value_role("true"), Role::Accent);
        assert_eq!(value_role("3.5"), Role::Accent);
        assert_eq!(value_role("greet"), Role::Default);
    }
}
