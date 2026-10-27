//! A composed answer: a source that answers the shape of its surface as a
//! tree of catalog nodes (`output "view"`, `extension-compose`).
//!
//! The tree is validated against the same catalog a view file is compiled
//! against (each node's arguments, properties and children, and the node
//! cap), every string is cleaned, and it becomes a view whose every value
//! is a literal: nothing in a composed answer is a reference, so data
//! cannot turn into a binding. A control names an action the manifest
//! declares and carries its own `item`. A tree that does not validate is
//! refused whole, naming the first wrong node by its path.

use std::collections::BTreeMap;

use serde_json::Value;
use uze_contract::binding::Binding;
use uze_contract::catalog::{COMMON_PROPS, NodeName, PropKind, RoleName};
use uze_contract::{Clean, CompiledView, Item, Manifest, Node, Span};

use super::bind::NODE_CAP;

/// The common properties a composed node may carry: how it shares a row.
/// What decides whether a bound node is drawn (`each`, `when`, …) has no
/// meaning where every value is already there.
const COMPOSED_COMMON: &[&str] = &["grow", "min-width"];

/// The view `tree` composes, for the manifest `manifest`, or why not.
pub(crate) fn compose(
    tree: &Value,
    manifest: &Manifest,
    file: &str,
) -> Result<CompiledView, String> {
    let mut count = 0;
    let root = node(tree, manifest, "tree", &mut count)?;
    Ok(CompiledView {
        file: file.to_owned(),
        items: vec![Item::Node(root)],
        templates: BTreeMap::new(),
        lookups: Vec::new(),
    })
}

fn node(value: &Value, manifest: &Manifest, at: &str, count: &mut usize) -> Result<Node, String> {
    *count += 1;
    if *count > NODE_CAP {
        return Err(format!("the answer holds more than {NODE_CAP} nodes"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| format!("{at} is not a node: an object with a `type`"))?;
    if let Some(key) = object
        .keys()
        .find(|key| !["type", "args", "props", "children", "item"].contains(&key.as_str()))
    {
        return Err(format!("{at} has `{key}`, which a node does not"));
    }
    let named = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{at} names no `type`"))?;
    let name = NodeName::parse(named)
        .ok_or_else(|| format!("{at} is a `{named}`, a node the catalog does not have"))?;
    let spec = name.spec();
    let args = object
        .get("args")
        .map(|args| {
            args.as_array()
                .ok_or_else(|| format!("{at}.args is not a list"))
        })
        .transpose()?
        .cloned()
        .unwrap_or_default();
    if args.len() > spec.args.len() || args.len() < spec.required_args {
        return Err(format!(
            "{at} is a `{named}` with {} argument(s); it takes {} to {}",
            args.len(),
            spec.required_args,
            spec.args.len()
        ));
    }
    let args = args
        .iter()
        .zip(spec.args)
        .enumerate()
        .map(|(index, (value, kind))| literal(value, *kind, &format!("{at}.args[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut props = BTreeMap::new();
    if let Some(given) = object.get("props") {
        let given = given
            .as_object()
            .ok_or_else(|| format!("{at}.props is not an object"))?;
        for (prop, value) in given {
            let kind = spec
                .props
                .iter()
                .find(|(name, _)| name == prop)
                .map(|(_, kind)| *kind)
                .or_else(|| {
                    COMMON_PROPS
                        .iter()
                        .find(|(name, _)| name == prop && COMPOSED_COMMON.contains(name))
                        .map(|(_, kind)| *kind)
                })
                .ok_or_else(|| {
                    format!("{at}.props has `{prop}`, which a `{named}` does not take")
                })?;
            props.insert(
                prop.clone(),
                literal(value, kind, &format!("{at}.props.{prop}"))?,
            );
        }
    }
    if let Some(action) = props.get("action") {
        let Binding::Literal(Value::String(action)) = action else {
            return Err(format!("{at}.props.action is not an action's id"));
        };
        if manifest.action(action).is_none() {
            return Err(format!(
                "{at}.props.action names `{action}`, an action the manifest does not declare"
            ));
        }
    }
    if let Some(item) = object.get("item") {
        if !props.contains_key("action") {
            return Err(format!(
                "{at} carries an `item` and names no `action` to hand it to"
            ));
        }
        props.insert("item".to_owned(), Binding::Literal(cleaned(item)));
    }
    let children = match object.get("children") {
        None => Vec::new(),
        Some(children) => {
            let children = children
                .as_array()
                .ok_or_else(|| format!("{at}.children is not a list"))?;
            let Some(allowed) = spec.children else {
                return Err(format!("{at} is a `{named}`, which holds no other node"));
            };
            children
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    let at = format!("{at}.children[{index}]");
                    let child = node(child, manifest, &at, count)?;
                    if allowed.contains(&child.name) {
                        Ok(Item::Node(child))
                    } else {
                        Err(format!(
                            "{at} is a `{}`, which a `{named}` does not hold",
                            child.name.as_str()
                        ))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    Ok(Node {
        name,
        args,
        props,
        each: None,
        condition: None,
        children,
        span: Span { offset: 0, len: 0 },
    })
}

/// `value` as a literal of `kind`, cleaned, or why it is not one.
fn literal(value: &Value, kind: PropKind, at: &str) -> Result<Binding, String> {
    let wrong = |what: &str| Err(format!("{at} is not {what}"));
    let value = match kind {
        PropKind::Text => match value {
            Value::String(text) => Value::String(Clean::text(text).into_string()),
            Value::Number(number) => Value::String(number.to_string()),
            Value::Bool(flag) => Value::String(flag.to_string()),
            _ => return wrong("text"),
        },
        PropKind::Count => match value.as_u64() {
            Some(count) => Value::from(count),
            None => return wrong("a whole number"),
        },
        PropKind::Number => match value.as_f64() {
            Some(number) if number.is_finite() => value.clone(),
            _ => return wrong("a number"),
        },
        PropKind::Flag => match value {
            Value::Bool(_) => value.clone(),
            _ => return wrong("true or false"),
        },
        PropKind::Role => match value.as_str().and_then(RoleName::parse) {
            Some(_) => value.clone(),
            None => return wrong("a role"),
        },
        PropKind::Identity => match value {
            Value::String(text) => Value::String(Clean::label(text).into_string()),
            _ => return wrong("a name"),
        },
        PropKind::Word(words) => match value.as_str() {
            Some(word) if words.contains(&word) => value.clone(),
            _ => return wrong(&format!("one of {}", words.join(", "))),
        },
        PropKind::Value => cleaned(value),
    };
    Ok(Binding::Literal(value))
}

/// `value` with every string in it cleaned.
fn cleaned(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(Clean::text(text).into_string()),
        Value::Array(items) => Value::Array(items.iter().map(cleaned).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (Clean::label(key).into_string(), cleaned(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uze_contract::{ActionDecl, Does};

    fn manifest() -> Manifest {
        Manifest {
            protocol: uze_contract::PROTOCOL,
            id: "board".into(),
            name: "Board".into(),
            description: String::new(),
            wants: Vec::new(),
            surfaces: Vec::new(),
            sources: Vec::new(),
            actions: vec![ActionDecl {
                id: "archive".into(),
                label: "Archive".into(),
                on: "board".into(),
                key: None,
                confirm: Some("Archive {name}?".into()),
                does: Does::Refresh(Vec::new()),
            }],
            exec: Vec::new(),
            preferences: Vec::new(),
        }
    }

    fn refused(tree: Value) -> String {
        compose(&tree, &manifest(), "board.kdl").expect_err("refused")
    }

    #[test]
    fn a_tree_of_catalog_nodes_becomes_a_view_of_literals() {
        let tree = json!({"type": "Stack", "props": {"gap": 1}, "children": [
            {"type": "Text", "args": ["3 open \u{1b}[31m{phase}"], "props": {"strong": true}},
            {"type": "Progress", "children": [
                {"type": "Segment", "props": {"weight": 2, "ratio": 0.5, "role": "success"}},
                {"type": "Segment", "props": {"weight": 1, "ratio": 0.0}}
            ]},
            {"type": "Button", "args": ["Archive"], "props": {"action": "archive"}, "item": {"name": "auth"}}
        ]});
        let view = compose(&tree, &manifest(), "board.kdl").unwrap();
        let Item::Node(root) = &view.items[0] else {
            panic!("a node")
        };
        assert_eq!(root.children.len(), 3);
        let Item::Node(text) = &root.children[0] else {
            panic!("a node")
        };
        assert_eq!(
            text.args[0],
            Binding::Literal(json!("3 open {phase}")),
            "cleaned, and a brace is text, never a reference"
        );
        let Item::Node(button) = &root.children[2] else {
            panic!("a node")
        };
        assert_eq!(
            button.props["item"],
            Binding::Literal(json!({"name": "auth"}))
        );
    }

    #[test]
    fn a_tree_that_does_not_validate_names_its_first_wrong_node() {
        assert_eq!(
            refused(json!({"type": "Stack", "children": [{"type": "iframe"}]})),
            "tree.children[0] is a `iframe`, a node the catalog does not have"
        );
        assert!(
            refused(json!({"type": "Text", "args": ["a"], "children": []}))
                .contains("holds no other node")
        );
        assert!(
            refused(json!({"type": "Button", "args": ["Go"], "props": {"action": "wipe"}}))
                .contains("an action the manifest does not declare")
        );
        assert!(
            refused(json!({"type": "Text", "args": ["a"], "item": {}}))
                .contains("names no `action`")
        );
        assert!(
            refused(json!({"type": "Text", "args": ["a"], "props": {"role": "red"}}))
                .contains("is not a role")
        );
        assert!(
            refused(json!({"type": "Text", "args": ["a"], "props": {"when": "x"}}))
                .contains("does not take")
        );
        let wide: Vec<Value> = (0..NODE_CAP).map(|_| json!({"type": "Rule"})).collect();
        assert!(
            refused(json!({"type": "Stack", "children": wide})).contains("more than 2000 nodes")
        );
    }
}
