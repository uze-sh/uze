//! Which fields a view reaches for that its fixture does not hold, found by
//! binding it the way the workspace does: once for every choice of every
//! `Tabs` it declares, since a field under a second tab is never reached
//! while the first one shows.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use uze_contract::{Binding, CompiledExtension, CompiledView, Item, NodeName, Span};

use super::{Absence, DeclaredExtension, TraceNode};

/// Past this many combinations of tab choices, each tab is bound once with
/// the others at their first, rather than every combination.
pub const TAB_COMBINATIONS_MAX: usize = 64;

/// A field one of a surface's nodes reaches for and its fixture lacks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissingField {
    pub surface: String,
    /// The view file, as the manifest names it.
    pub view: String,
    /// The fixture the source is fed by.
    pub fixture: String,
    pub span: Span,
    pub field: String,
}

/// Every field the surfaces of `compiled` reach for that their fixtures
/// lack, once per view, place and field. `fixtures` holds each source's
/// fixture that read; a surface whose source has none is left alone.
pub fn missing_fields(
    compiled: &CompiledExtension,
    fixtures: &BTreeMap<String, Value>,
    locale: &str,
) -> Vec<MissingField> {
    let mut found = Vec::new();
    let mut said = BTreeSet::new();
    for surface in &compiled.manifest.surfaces {
        let Some(fixture) = compiled
            .manifest
            .source(&surface.source)
            .and_then(|source| source.fixture.clone())
            .filter(|_| fixtures.contains_key(&surface.source))
        else {
            continue;
        };
        let Some(view) = compiled.views.get(&surface.view) else {
            continue;
        };
        for choices in tab_choices(view) {
            let mut engine = DeclaredExtension::new(compiled.clone(), locale);
            for (source, data) in fixtures {
                engine.set_data(source.clone(), Ok(data.clone()));
            }
            for (id, value) in choices {
                engine.set_ui(&surface.id, id, value);
            }
            let Some(trace) = engine.trace(&surface.id) else {
                continue;
            };
            let mut missing = Vec::new();
            collect(&trace.items, &mut missing);
            for body in trace.templates.values() {
                collect(body, &mut missing);
            }
            for (span, field) in missing {
                // Two surfaces drawing one view over one source, or two
                // choices of a tab drawing one node, find the same field
                // missing at the same place: said once.
                if said.insert((surface.view.clone(), span.offset, field.clone())) {
                    found.push(MissingField {
                        surface: surface.id.clone(),
                        view: surface.view.clone(),
                        fixture: fixture.clone(),
                        span,
                        field,
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| (&a.view, a.span.offset).cmp(&(&b.view, b.span.offset)));
    found
}

/// Every field a node was not drawn for because the data lacks it. A `when`
/// that some item answered is the presence test doing its job on the
/// others, so it is said only when no item ever held the field: then it is
/// likely misspelled.
fn collect(nodes: &[TraceNode], into: &mut Vec<(Span, String)>) {
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
        collect(&node.children, into);
    }
}

/// The UI values to bind `view` under: every combination of its tabs'
/// choices, or, past [`TAB_COMBINATIONS_MAX`], each choice of each tab
/// alone. A view with no tabs is bound once, as it opens.
fn tab_choices(view: &CompiledView) -> Vec<Vec<(String, String)>> {
    let mut tabs = Vec::new();
    find_tabs(&view.items, &mut tabs);
    for body in view.templates.values() {
        find_tabs(body, &mut tabs);
    }
    tabs.dedup_by(|a, b| a.0 == b.0);
    let combinations = tabs
        .iter()
        .map(|(_, values)| values.len().max(1))
        .try_fold(1usize, |product, count| product.checked_mul(count))
        .unwrap_or(usize::MAX);
    if combinations > TAB_COMBINATIONS_MAX {
        let mut each = vec![Vec::new()];
        for (id, values) in &tabs {
            for value in values {
                each.push(vec![(id.clone(), value.clone())]);
            }
        }
        return each;
    }
    tabs.iter().fold(vec![Vec::new()], |sets, (id, values)| {
        sets.iter()
            .flat_map(|set| {
                values.iter().map(move |value| {
                    let mut with = set.clone();
                    with.push((id.clone(), value.clone()));
                    with
                })
            })
            .collect()
    })
}

/// Each `Tabs` with an `id`, and the values of its tabs, in view order.
fn find_tabs(items: &[Item], into: &mut Vec<(String, Vec<String>)>) {
    for item in items {
        let Item::Node(node) = item else {
            continue;
        };
        if node.name == NodeName::Tabs
            && let Some(Binding::Literal(Value::String(id))) = node.props.get("id")
        {
            let values = node
                .children
                .iter()
                .filter_map(|child| match child {
                    Item::Node(tab) => match tab.props.get("value") {
                        Some(Binding::Literal(Value::String(value))) => Some(value.clone()),
                        _ => None,
                    },
                    Item::Use { .. } => None,
                })
                .collect();
            into.push((id.clone(), values));
        }
        find_tabs(&node.children, into);
    }
}
