//! Each harness's hook vocabulary, held against what the harness declared.
//!
//! The binding tables are claims about four products that rename their
//! tools between releases. The other tests here check a table against
//! itself, which a table written from memory always passes. These check it
//! against the Lab's measurement instead: the name and fields each call
//! reached a real hook with (`hook_tools` in
//! `conformance/evidence/tools/<harness>.json`), and
//! the Lab's own expectation of the vocabulary
//! (`conformance/harnesses/<harness>/vocabulary.json`), written separately
//! so that a shared mistake surfaces as a disagreement.
//!
//! Each test reports every disagreement it finds, not the first: the list
//! is the work a vendor release left behind.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::tests::TARGETS;
use super::*;

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn read_json(path: PathBuf) -> Value {
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The name and input fields each scripted call reached a hook with, on the
/// version the snapshot names (the hooks contract's census): the hook side
/// of the vocabulary, which is what a matcher has to name.
fn declared(target: HookTarget) -> BTreeMap<String, Vec<String>> {
    let snapshot = read_json(conformance().join(format!("evidence/tools/{}.json", target.key())));
    serde_json::from_value(snapshot["hook_tools"].clone()).expect("a hook_tools map")
}

fn expectation(target: HookTarget) -> Value {
    read_json(conformance().join(format!("harnesses/{}/vocabulary.json", target.key())))
}

/// Tools the row was measured to have on a platform the Lab does not run
/// (`elsewhere.<platform>.tools`), each with the measurement it came from.
fn elsewhere(row: &Value) -> Vec<String> {
    row["elsewhere"]
        .as_object()
        .into_iter()
        .flat_map(|platforms| platforms.values())
        .flat_map(|platform| {
            assert!(
                platform["source"]
                    .as_str()
                    .is_some_and(|source| !source.is_empty()),
                "a tool measured elsewhere names its measurement"
            );
            platform["tools"].as_array().cloned().unwrap_or_default()
        })
        .map(|tool| tool.as_str().expect("a tool name").to_owned())
        .collect()
}

fn names(table: &Value, key: &str) -> Vec<String> {
    let mut names: Vec<String> = table[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` must be a list"))
        .iter()
        .map(|name| name.as_str().expect("a name").to_owned())
        .collect();
    names.sort();
    names
}

fn assert_none(problems: Vec<String>) {
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_bound_tool_and_field_is_one_the_harness_declared() {
    let mut problems = Vec::new();
    for target in TARGETS {
        let declared = declared(target);
        let expected = expectation(target);
        for binding in vocabulary(target).bindings {
            let row = &expected["aliases"][binding.alias];
            let measured_elsewhere = elsewhere(row);
            // A tool the harness offers only in some sessions, for a reason
            // the Lab measured (`optional`): its absence from this census is
            // the declared case, and the Lab's row still has to agree.
            let optional = row["optional"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty());
            for tool in binding.native_tool.iter().chain(binding.also_matches) {
                if measured_elsewhere.iter().any(|other| other == tool) {
                    continue;
                }
                if optional && !declared.contains_key(*tool) {
                    continue;
                }
                let Some(fields) = declared.get(*tool) else {
                    problems.push(format!(
                        "{target}/{}: no hook saw a call as `{tool}`",
                        binding.alias
                    ));
                    continue;
                };
                if Some(*tool) != binding.native_tool {
                    continue;
                }
                for (portable, native) in binding.fields {
                    if !fields.iter().any(|field| field == native) {
                        problems.push(format!(
                            "{target}/{}: `{tool}` declares no field `{native}` for `{portable}` (it has {fields:?})",
                            binding.alias
                        ));
                    }
                }
            }
        }
    }
    assert_none(problems);
}

#[test]
fn the_lab_and_the_integration_agree_on_every_alias() {
    let mut problems = Vec::new();
    for target in TARGETS {
        let expected = expectation(target);
        for binding in vocabulary(target).bindings {
            let row = &expected["aliases"][binding.alias];
            let bound: Vec<String> = binding
                .native_tool
                .iter()
                .chain(binding.also_matches)
                .map(|tool| (*tool).to_owned())
                .collect();
            // `unsupported`: the harness has a tool of this kind, measured
            // unable to carry the alias's portable fields, so the alias must
            // stay unbound there and be reported as such.
            let unbindable = row["unsupported"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty());
            // An optional tool no census has seen reach a hook yet: the
            // integration may leave the alias unbound until one does.
            let hooked = declared(target);
            let unmeasured = row["optional"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
                && row["tools"].as_array().is_some_and(|tools| {
                    tools
                        .iter()
                        .all(|tool| !hooked.contains_key(tool.as_str().unwrap_or_default()))
                });
            if unmeasured && bound.is_empty() {
                continue;
            }
            if row.is_null() || unbindable {
                if !bound.is_empty() {
                    problems.push(format!(
                        "{target}/{}: the Lab measured no tool that carries this alias, the integration binds {bound:?}",
                        binding.alias
                    ));
                }
                continue;
            }
            let mut tools: Vec<String> = row["tools"]
                .as_array()
                .unwrap_or_else(|| {
                    panic!("{target}/{}: no `tools` in the Lab's row", binding.alias)
                })
                .iter()
                .map(|tool| tool.as_str().expect("a tool name").to_owned())
                .collect();
            tools.extend(elsewhere(row));
            if bound != tools {
                problems.push(format!(
                    "{target}/{}: the integration binds {bound:?}, the Lab measured {tools:?}",
                    binding.alias
                ));
            }
            let fields: BTreeMap<&str, &str> = row["fields"]
                .as_object()
                .map(|fields| {
                    fields
                        .iter()
                        .map(|(portable, native)| {
                            (portable.as_str(), native.as_str().expect("a field"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let bound_fields: BTreeMap<&str, &str> = binding.fields.iter().copied().collect();
            if bound_fields != fields {
                problems.push(format!(
                    "{target}/{}: the integration reads {bound_fields:?}, the Lab measured {fields:?}",
                    binding.alias
                ));
            }
        }
    }
    assert_none(problems);
}

#[test]
fn every_event_and_effect_the_integration_claims_has_a_lab_row() {
    let mut problems = Vec::new();
    for target in TARGETS {
        let expected = expectation(target);
        let mut events: Vec<String> = target
            .events
            .iter()
            .map(|event| event.abi_name().to_owned())
            .collect();
        events.sort();
        let mut effects: Vec<String> = target
            .effects
            .iter()
            .map(|effect| effect.abi_name().to_owned())
            .collect();
        effects.sort();
        if events != names(&expected, "events") {
            problems.push(format!(
                "{target}: claims events {events:?}, the Lab exercises {:?}",
                names(&expected, "events")
            ));
        }
        if effects != names(&expected, "effects") {
            problems.push(format!(
                "{target}: claims effects {effects:?}, the Lab exercises {:?}",
                names(&expected, "effects")
            ));
        }
    }
    assert_none(problems);
}
