//! How a compiled view refers to its data and to the UI state the host
//! holds.
//!
//! A grammar of references, never of expressions (D23): a view can say
//! *which* value, and *whether* one is there, and nothing about how to
//! compute one. Anything that needs computing is a field the data carries.

use serde::{Deserialize, Serialize};

/// Where a reference starts looking.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Scope {
    /// The current item of the innermost `each`, or the whole data outside
    /// one. `{field}`, `{a.b}`, `{.}`.
    Item,
    /// The item of the enclosing `each`. `{^.field}`.
    Parent,
    /// A value the host holds for a declared `id` (D24). `{ui.tab}`.
    Ui,
    /// What the host says about where the surface is drawn. `{context.size}`.
    Context,
}

/// A reference into data: a scope and the fields walked from it. No
/// segments means the scope's value itself (`{.}`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub scope: Scope,
    pub segments: Vec<String>,
}

impl Path {
    /// Reads the text between braces: `name`, `next.phase`, `.`, `^.agent`,
    /// `ui.tab`, `context.size`. `None` for anything else, which is how an
    /// expression is refused: there is no spelling of one here.
    pub fn parse(source: &str) -> Option<Self> {
        let source = source.trim();
        if source == "." {
            return Some(Self {
                scope: Scope::Item,
                segments: Vec::new(),
            });
        }
        let (scope, rest) = if let Some(rest) = source.strip_prefix("^.") {
            (Scope::Parent, rest)
        } else if let Some(rest) = source.strip_prefix("ui.") {
            (Scope::Ui, rest)
        } else if let Some(rest) = source.strip_prefix("context.") {
            (Scope::Context, rest)
        } else {
            (Scope::Item, source)
        };
        let segments: Vec<String> = rest.split('.').map(str::to_owned).collect();
        segments
            .iter()
            .all(|segment| is_field_name(segment))
            .then_some(Self { scope, segments })
    }
}

/// A field name a reference may walk: letters, digits, `_` and `-`,
/// starting with a letter or `_`. Narrow so that `{a + b}`, `{f(x)}` or
/// `{a ? b : c}` are simply not references.
pub fn is_field_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

/// One piece of a property or argument that mixes literal text with values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TextPart {
    Literal(String),
    Ref(Path),
    /// `{table:field}`: the value found under `field` looked up in the
    /// named lookup table.
    Lookup {
        table: String,
        path: Path,
    },
    /// `{table::key}`: the entry `key` itself, a label the view names and
    /// the data has no part in, such as `{t::next}` for a translated word.
    Label {
        table: String,
        key: String,
    },
}

/// What a property or an argument is bound to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Binding {
    /// A value written in the view itself.
    Literal(serde_json::Value),
    /// Exactly one reference and nothing around it: the value keeps the
    /// JSON type it has in the data, so `weight="{plans}"` is a number.
    Ref(Path),
    /// Exactly one lookup and nothing around it.
    Lookup { table: String, path: Path },
    /// Exactly one label and nothing around it.
    Label { table: String, key: String },
    /// Text with references in it, always a string, interpolating scalars
    /// only. Display text only: the compiler refuses it on a property that
    /// carries identity.
    Text(Vec<TextPart>),
}

/// When a node is drawn at all.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// The value is present: not absent, `null`, `false`, `""`, `0` or `[]`.
    When(Path),
    /// The value is not present, by the same test.
    Unless(Path),
    /// A UI value (`ui.<id>`) equals a literal the view itself declares.
    Show { id: String, value: String },
    /// Any of several such pairs.
    ShowAny(Vec<(String, String)>),
}

/// Whether `value` counts as present for [`Condition::When`] and
/// [`Condition::Unless`]. One definition, so the engine and the inspector's
/// "why absent" never disagree.
pub fn is_present(value: Option<&serde_json::Value>) -> bool {
    use serde_json::Value;
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(_)) => true,
    }
}

/// Splits a string written in a view into literal text and references.
/// `{{` and `}}` are literal braces. `Err` names the first brace pair that
/// holds something that is not a reference.
pub fn parse_text(source: &str) -> Result<Vec<TextPart>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = source;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("{{") {
            literal.push('{');
            rest = after;
        } else if let Some(after) = rest.strip_prefix("}}") {
            literal.push('}');
            rest = after;
        } else if let Some(after) = rest.strip_prefix('{') {
            let Some(end) = after.find('}') else {
                return Err(format!("`{{` with no closing `}}` in \"{source}\""));
            };
            let inner = &after[..end];
            if !literal.is_empty() {
                parts.push(TextPart::Literal(std::mem::take(&mut literal)));
            }
            parts.push(reference(inner)?);
            rest = &after[end + 1..];
        } else {
            let ch = rest.chars().next().unwrap_or_default();
            literal.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    if !literal.is_empty() {
        parts.push(TextPart::Literal(literal));
    }
    Ok(parts)
}

fn reference(inner: &str) -> Result<TextPart, String> {
    if let Some((table, key)) = inner.split_once("::") {
        let (table, key) = (table.trim(), key.trim());
        return (is_field_name(table) && is_field_name(key))
            .then(|| TextPart::Label {
                table: table.to_owned(),
                key: key.to_owned(),
            })
            .ok_or_else(|| format!("`{{{inner}}}` is not a label: write `{{table::key}}`"));
    }
    if let Some((table, field)) = inner.split_once(':') {
        let path = Path::parse(field).filter(|_| is_field_name(table.trim()));
        return path
            .map(|path| TextPart::Lookup {
                table: table.trim().to_owned(),
                path,
            })
            .ok_or_else(|| format!("`{{{inner}}}` is not a lookup: write `{{table:field}}`"));
    }
    Path::parse(inner).map(TextPart::Ref).ok_or_else(|| {
        format!("`{{{inner}}}` is not a reference: a reference names a field, such as `{{name}}` or `{{next.phase}}`, and computes nothing")
    })
}

/// Turns parsed text into the binding it is: a lone reference or lookup
/// keeps its type, anything else is text.
pub fn binding_of(parts: Vec<TextPart>) -> Binding {
    match <[TextPart; 1]>::try_from(parts) {
        Ok([TextPart::Ref(path)]) => Binding::Ref(path),
        Ok([TextPart::Lookup { table, path }]) => Binding::Lookup { table, path },
        Ok([TextPart::Label { table, key }]) => Binding::Label { table, key },
        Ok([TextPart::Literal(text)]) => Binding::Literal(serde_json::Value::String(text)),
        Err(parts) if parts.is_empty() => {
            Binding::Literal(serde_json::Value::String(String::new()))
        }
        Err(parts) => Binding::Text(parts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn path(scope: Scope, segments: &[&str]) -> Path {
        Path {
            scope,
            segments: segments.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn references_name_fields_in_four_scopes() {
        assert_eq!(Path::parse("name"), Some(path(Scope::Item, &["name"])));
        assert_eq!(
            Path::parse("next.phase"),
            Some(path(Scope::Item, &["next", "phase"]))
        );
        assert_eq!(Path::parse("."), Some(path(Scope::Item, &[])));
        assert_eq!(
            Path::parse("^.agent"),
            Some(path(Scope::Parent, &["agent"]))
        );
        assert_eq!(Path::parse("ui.tab"), Some(path(Scope::Ui, &["tab"])));
        assert_eq!(
            Path::parse("context.size"),
            Some(path(Scope::Context, &["size"]))
        );
    }

    #[test]
    fn an_expression_is_not_a_reference() {
        for expression in ["done > 0 ? done : '-'", "a + b", "f(x)", "a..b", "", "1st"] {
            assert_eq!(Path::parse(expression), None, "{expression}");
        }
    }

    #[test]
    fn a_lone_reference_keeps_its_type_and_mixed_text_is_a_string() {
        let lone = binding_of(parse_text("{plans}").unwrap());
        assert_eq!(lone, Binding::Ref(path(Scope::Item, &["plans"])));
        let mixed = binding_of(parse_text("{plans} planos").unwrap());
        assert!(matches!(mixed, Binding::Text(parts) if parts.len() == 2));
        let plain = binding_of(parse_text("Fases").unwrap());
        assert_eq!(plain, Binding::Literal(json!("Fases")));
    }

    #[test]
    fn doubled_braces_are_literal_and_lookups_are_named() {
        assert_eq!(
            parse_text("{{x}}").unwrap(),
            vec![TextPart::Literal("{x}".into())]
        );
        assert_eq!(
            binding_of(parse_text("{tone:e}").unwrap()),
            Binding::Lookup {
                table: "tone".into(),
                path: path(Scope::Item, &["e"])
            }
        );
        assert_eq!(
            binding_of(parse_text("{t::next}").unwrap()),
            Binding::Label {
                table: "t".into(),
                key: "next".into()
            }
        );
        assert!(parse_text("{t::}").is_err());
        assert!(parse_text("{done > 0}").is_err());
        assert!(parse_text("{open").is_err());
    }

    #[test]
    fn presence_is_one_definition() {
        for absent in [
            None,
            Some(&json!(null)),
            Some(&json!(false)),
            Some(&json!("")),
            Some(&json!(0)),
            Some(&json!([])),
        ] {
            assert!(!is_present(absent), "{absent:?}");
        }
        for present in [json!(true), json!("x"), json!(1), json!([1]), json!({})] {
            assert!(is_present(Some(&present)), "{present}");
        }
    }
}
