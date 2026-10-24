//! Decoding a `tests/*.test.kdl` into [`TestFile`].
//!
//! A test file is third-party KDL like a view, so it is read through the
//! same bounded parse and gets the same refusals: unknown directives with
//! the nearest name, repeated properties, type annotations. What a step
//! means is the runner's; this only says whether it is well formed.

use std::collections::BTreeSet;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use uze_contract::{
    Given, GivenData, NodeName, Span, Step, StepKind, TestDecl, TestFile, ThemeName, World,
    testing::STEPS,
};

use super::document::{Place, Report, entries, nearest, refuse_extra, spelled};
use super::manifest::relative_file;

/// Where an extension keeps its tests, and how a test file is named.
pub const TESTS_DIR: &str = "tests";
pub const TEST_SUFFIX: &str = ".test.kdl";

/// The largest size a test may draw a surface at, either way.
const SIZE_MAX: u16 = 500;

pub fn decode(document: &KdlDocument, report: &mut Report<'_>) -> TestFile {
    let mut file = TestFile::default();
    let mut names = BTreeSet::new();
    for node in document.nodes() {
        let name = node.name().value();
        if name != "test" {
            report.on(
                node,
                format!("a test file holds `test` blocks, not `{name}`"),
                Some("write `test \"what it proves\" { … }`".to_owned()),
            );
            continue;
        }
        let Some(test) = test(node, report) else {
            continue;
        };
        if !names.insert(test.name.clone()) {
            report.on(
                node,
                format!("two tests are named \"{}\"", test.name),
                Some("give each test its own name".to_owned()),
            );
            continue;
        }
        file.tests.push(test);
    }
    file
}

fn span_of(place: &impl Place, len: usize) -> Span {
    Span {
        offset: place.offset(),
        len,
    }
}

fn test(node: &KdlNode, report: &mut Report<'_>) -> Option<TestDecl> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &[], true, report);
    let name = string(node, parts.args.first().copied(), report)?;
    let mut world = World::default();
    let mut surface = None;
    let mut steps = Vec::new();
    let mut said = BTreeSet::new();
    let Some(body) = node.children() else {
        report.on(
            node,
            format!("the test \"{name}\" has no steps"),
            Some("open a block: `{ surface \"…\"; expect-text \"…\" }`".to_owned()),
        );
        return None;
    };
    for step in body.nodes() {
        let directive = step.name().value();
        let once = matches!(directive, "surface" | "size" | "theme" | "locale");
        if once && !said.insert(directive) {
            report.on(
                step,
                format!("`{directive}` is said twice in one test"),
                Some("keep one".to_owned()),
            );
            continue;
        }
        let span = span_of(step, step.span().len());
        match directive {
            "surface" => surface = one_string(step, report),
            "given" => {
                if let Some(given) = given(step, span, report) {
                    world.given.push(given);
                }
            }
            "size" => world.size = size(step, report),
            "theme" => {
                world.theme = one_string(step, report).and_then(|theme| {
                    let parsed = ThemeName::parse(&theme);
                    if parsed.is_none() {
                        report.on(
                            step,
                            format!("there is no theme `{theme}`"),
                            Some("write `dark` or `light`".to_owned()),
                        );
                    }
                    parsed
                });
            }
            "locale" => world.locale = one_string(step, report),
            "expect-text" => push(
                &mut steps,
                span,
                one_string(step, report).map(StepKind::ExpectText),
            ),
            "expect-no-text" => push(
                &mut steps,
                span,
                one_string(step, report).map(StepKind::ExpectNoText),
            ),
            "expect-absent" => push(&mut steps, span, absent(step, report)),
            "expect-action" => push(&mut steps, span, action(step, report)),
            "click" => push(
                &mut steps,
                span,
                one_string(step, report).map(StepKind::Click),
            ),
            "press" => push(
                &mut steps,
                span,
                one_string(step, report).map(StepKind::Press),
            ),
            "press-button" => push(
                &mut steps,
                span,
                one_string(step, report).map(StepKind::PressButton),
            ),
            "snapshot" => push(&mut steps, span, snapshot(step, report)),
            other => unknown(step, other, report),
        }
    }
    let Some(surface) = surface else {
        report.on(
            node,
            format!("the test \"{name}\" opens no surface"),
            Some("say which: `surface \"<id>\"`".to_owned()),
        );
        return None;
    };
    world.surface = surface;
    Some(TestDecl {
        name,
        span: span_of(node, node.name().len()),
        world,
        steps,
    })
}

fn push(steps: &mut Vec<Step>, span: Span, kind: Option<StepKind>) {
    if let Some(kind) = kind {
        steps.push(Step { kind, span });
    }
}

fn unknown(node: &KdlNode, name: &str, report: &mut Report<'_>) {
    let known: Vec<&str> = STEPS.iter().map(|(step, _)| *step).collect();
    let suggestion = match nearest(name, &known) {
        Some(near) => format!("did you mean `{near}`?"),
        None => format!(
            "a test may use {}",
            known
                .iter()
                .map(|step| format!("`{step}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    report.on(
        node,
        format!("a test has no step `{name}`"),
        Some(suggestion),
    );
}

fn string(node: &KdlNode, arg: Option<&KdlEntry>, report: &mut Report<'_>) -> Option<String> {
    let name = node.name().value();
    match arg.map(KdlEntry::value) {
        Some(KdlValue::String(text)) => Some(text.clone()),
        Some(other) => {
            report.on(
                arg?,
                format!("`{name}` takes a string, not {}", spelled(other)),
                Some(format!("write `{name} \"{other}\"`")),
            );
            None
        }
        None => {
            report.on(
                node,
                format!("`{name}` takes a string"),
                Some(format!("write `{name} \"…\"`")),
            );
            None
        }
    }
}

/// The one string argument of a step that holds nothing else.
fn one_string(node: &KdlNode, report: &mut Report<'_>) -> Option<String> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &[], false, report);
    string(node, parts.args.first().copied(), report)
}

fn given(node: &KdlNode, span: Span, report: &mut Report<'_>) -> Option<Given> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &["fixture", "json"], false, report);
    let source = string(node, parts.args.first().copied(), report)?;
    let data = match parts.props.as_slice() {
        [("fixture", entry)] => {
            let KdlValue::String(file) = entry.value() else {
                report.on(
                    *entry,
                    "`fixture` takes a path",
                    Some("write `fixture=\"fixtures/….json\"`".to_owned()),
                );
                return None;
            };
            if !relative_file(file, "json", entry.offset(), report) {
                return None;
            }
            GivenData::Fixture(file.clone())
        }
        [("json", entry)] => {
            let KdlValue::String(text) = entry.value() else {
                report.on(
                    *entry,
                    "`json` takes a string of JSON",
                    Some("write `json=#\"{ … }\"#`".to_owned()),
                );
                return None;
            };
            match serde_json::from_str(text) {
                Ok(value) => GivenData::Json(value),
                Err(error) => {
                    report.on(*entry, format!("the JSON does not read: {error}"), None);
                    return None;
                }
            }
        }
        _ => {
            report.on(
                node,
                format!("`given \"{source}\"` needs one of `fixture=` or `json=`"),
                Some("write `fixture=\"fixtures/….json\"` or `json=#\"{ … }\"#`".to_owned()),
            );
            return None;
        }
    };
    Some(Given { source, data, span })
}

fn size(node: &KdlNode, report: &mut Report<'_>) -> Option<(u16, u16)> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 2, &[], false, report);
    let cells = |entry: Option<&&KdlEntry>| match entry.map(|entry| entry.value()) {
        Some(KdlValue::Integer(n)) => u16::try_from(*n)
            .ok()
            .filter(|n| (1..=SIZE_MAX).contains(n)),
        _ => None,
    };
    match (cells(parts.args.first()), cells(parts.args.get(1))) {
        (Some(width), Some(height)) => Some((width, height)),
        _ => {
            report.on(
                node,
                format!("`size` takes the columns and the rows, each from 1 to {SIZE_MAX}"),
                Some("write `size 46 24`".to_owned()),
            );
            None
        }
    }
}

fn absent(node: &KdlNode, report: &mut Report<'_>) -> Option<StepKind> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &["because"], false, report);
    let kind = string(node, parts.args.first().copied(), report)?;
    let Some(name) = NodeName::parse(&kind) else {
        let names: Vec<&str> = uze_contract::catalog::CATALOG
            .iter()
            .map(|spec| spec.name.as_str())
            .collect();
        report.on(
            parts.args[0],
            format!("`{kind}` is not a catalog node"),
            nearest(&kind, &names).map(|near| format!("did you mean `{near}`?")),
        );
        return None;
    };
    let because = parts.props.iter().find(|(prop, _)| *prop == "because");
    let Some(KdlValue::String(because)) = because.map(|(_, entry)| entry.value()) else {
        report.on(
            node,
            "`expect-absent` says why the node is absent",
            Some("add `because=\"<field or ui value>\"`".to_owned()),
        );
        return None;
    };
    Some(StepKind::ExpectAbsent {
        node: name,
        because: because.clone(),
    })
}

fn action(node: &KdlNode, report: &mut Report<'_>) -> Option<StepKind> {
    let parts = entries(node, report);
    // Every property is a key of the item the action received, so any
    // name is accepted; only the argument count and a block are checked.
    let keys: Vec<&str> = parts.props.iter().map(|(key, _)| *key).collect();
    refuse_extra(node, &parts, 1, &keys, false, report);
    let action = string(node, parts.args.first().copied(), report)?;
    let args = parts
        .props
        .iter()
        .map(|(key, entry)| ((*key).to_owned(), json_of(entry.value())))
        .collect();
    Some(StepKind::ExpectAction { action, args })
}

fn json_of(value: &KdlValue) -> serde_json::Value {
    match value {
        KdlValue::String(text) => serde_json::Value::String(text.clone()),
        KdlValue::Integer(n) => serde_json::Value::from(i64::try_from(*n).unwrap_or(i64::MAX)),
        KdlValue::Float(n) => serde_json::Number::from_f64(*n)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        KdlValue::Bool(flag) => serde_json::Value::Bool(*flag),
        KdlValue::Null => serde_json::Value::Null,
    }
}

fn snapshot(node: &KdlNode, report: &mut Report<'_>) -> Option<StepKind> {
    let name = one_string(node, report)?;
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if !plain {
        report.on(
            node,
            format!("the snapshot name `{name}` becomes a file name"),
            Some("use letters, digits, `-` and `_`".to_owned()),
        );
        return None;
    }
    Some(StepKind::Snapshot(name))
}
