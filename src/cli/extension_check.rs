//! The extensions half of `uze agent plugin check`: each extension's
//! fixtures bound to its views by the engine the workspace draws with, and
//! every field a view names that its fixture lacks said as a warning.

use std::collections::BTreeSet;

use uze_application::CheckedExtension;
use uze_contract::Span;
use uze_extensions::declared::{Absence, DeclaredExtension, TraceNode};

use crate::*;

/// The locale a check binds lookups in: the one the workspace draws in
/// until it knows the operator's.
const LOCALE: &str = "en";

/// Binds each surface of `checked` to its source's fixture and adds a
/// warning, placed in the view, for each field the view reaches for that
/// the fixture does not hold. A surface whose source has no fixture, or
/// one that did not read, has nothing to bind to and is left alone.
pub(crate) fn bind_fixtures(checked: &mut CheckedExtension) {
    let Some(compiled) = checked.compiled.clone() else {
        return;
    };
    let mut engine = DeclaredExtension::new(compiled.clone(), LOCALE);
    for (source, data) in &checked.fixtures {
        engine.set_data(source.clone(), Ok(data.clone()));
    }
    let mut said = BTreeSet::new();
    for surface in &compiled.manifest.surfaces {
        let Some(fixture) = compiled
            .manifest
            .sources
            .iter()
            .find(|source| source.id == surface.source)
            .and_then(|source| source.fixture.clone())
            .filter(|_| checked.fixtures.contains_key(&surface.source))
        else {
            continue;
        };
        let Some(trace) = engine.trace(&surface.id) else {
            continue;
        };
        let mut missing = Vec::new();
        missing_fields(&trace.items, &mut missing);
        for body in trace.templates.values() {
            missing_fields(body, &mut missing);
        }
        for (span, field) in missing {
            // Two surfaces drawing one view over one source find the same
            // field missing at the same place: said once.
            if !said.insert((surface.view.clone(), span.offset, field.clone())) {
                continue;
            }
            let place = checked
                .place(&surface.view, span.offset)
                .map_or_else(String::new, |(line, column)| format!(":{line}:{column}"));
            checked.warnings.push(format!(
                "{}{place}: `{field}` is not in the fixture {fixture}",
                surface.view
            ));
        }
    }
}

/// Every field a node was not drawn for because the data lacks it, in the
/// order the view writes them. A `when` that some item answered is the
/// presence test doing its job on the others, so it is said only when no
/// item ever held the field: then it is likely misspelled.
fn missing_fields(nodes: &[TraceNode], into: &mut Vec<(Span, String)>) {
    for node in nodes {
        for absence in &node.absent {
            let field = match absence {
                Absence::Missing { field } | Absence::EachMissing { field } => field,
                Absence::When {
                    field,
                    missing: true,
                } if node.drawn == 0 => field,
                _ => continue,
            };
            into.push((node.span, field.clone()));
        }
        missing_fields(&node.children, into);
    }
}

/// Each extension that is not clean, or warns, as a block under its id;
/// a clean one is already a row of what the plugin delivers.
pub(crate) fn render_extensions(extensions: &[CheckedExtension]) -> String {
    let mut text = String::new();
    for extension in extensions
        .iter()
        .filter(|extension| !extension.errors.is_empty() || !extension.warnings.is_empty())
    {
        text.push('\n');
        text.push_str(&progress::report_section(&format!(
            "extension {}",
            extension.id
        )));
        for error in &extension.errors {
            text.push_str(&format!("{} {error}\n", progress::error_icon()));
        }
        for warning in &extension.warnings {
            text.push_str(&format!("{} {warning}\n", progress::warning_icon()));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use uze_contract::NodeName;
    use uze_extensions::declared::TraceEntry;

    use super::*;

    fn guarded(drawn: u32) -> TraceNode {
        TraceNode {
            entry: TraceEntry::Node(NodeName::Text),
            span: Span { offset: 7, len: 4 },
            reached: 3,
            drawn,
            absent: vec![Absence::When {
                field: "tools".to_owned(),
                missing: true,
            }],
            children: Vec::new(),
        }
    }

    #[test]
    fn a_when_some_item_answered_is_not_a_missing_field() {
        let mut said = Vec::new();
        missing_fields(&[guarded(2)], &mut said);
        assert!(said.is_empty(), "{said:?}");
    }

    #[test]
    fn a_when_no_item_ever_answered_is_said_as_likely_misspelled() {
        let mut said = Vec::new();
        missing_fields(&[guarded(0)], &mut said);
        assert_eq!(said, [(Span { offset: 7, len: 4 }, "tools".to_owned())]);
    }
}
