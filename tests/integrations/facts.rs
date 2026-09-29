//! Every harness says what it was measured to do, and each fact names the
//! Lab check that proves it.
//!
//! A fact a delivery depends on and nothing proves is a guess that ships:
//! the harness moves, the delivery keeps assuming, and a user finds out.
//! Naming the check turns a harness release that breaks the fact into a
//! failed nightly instead.

use std::path::Path;

use uze_core::home::UzeHome;
use uze_integrations::registry::IntegrationRegistry;

#[test]
fn every_harness_states_its_facts_and_each_names_a_lab_check_that_exists() {
    let root = uze_testkit::temp::scratch("harness-facts");
    let home = UzeHome::at(root.join("uze"));
    let (integrations, _) = IntegrationRegistry::isolated(&root, &home).into_parts();
    let conformance = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance");
    let mut unproven = Vec::new();
    for integration in &integrations {
        let facts = integration.facts();
        assert!(
            !facts.is_empty(),
            "{} states no measured facts",
            integration.id()
        );
        for fact in facts {
            let (file, function) = fact
                .proven_by
                .split_once("::")
                .expect("`proven_by` is `<path>::<function>`");
            let source = std::fs::read_to_string(conformance.join(file)).unwrap_or_default();
            if !source.contains(&format!("def {function}(")) {
                unproven.push(format!(
                    "{}: `{}` names `{}`, which the Lab does not define",
                    integration.id(),
                    fact.fact,
                    fact.proven_by
                ));
            }
            assert!(
                !fact.measured_on.is_empty(),
                "{}: {}",
                integration.id(),
                fact.fact
            );
        }
    }
    assert!(unproven.is_empty(), "{}", unproven.join("\n"));
    let _ = std::fs::remove_dir_all(root);
}
