//! What `docs/extensions/` commits is what the contract generates: the
//! schema and the reference cannot drift from the types and the catalog the
//! workspace checks extensions against (`extension-platform-closure` 2.1).

use uze_contract::docs;

use crate::layering::repository_root;

fn assert_current(path: &str, generated: &str) {
    let committed = std::fs::read_to_string(repository_root().join(path)).unwrap_or_default();
    assert!(
        committed.replace("\r\n", "\n") == generated,
        "{path} is not what the extension contract generates; run `{}` and commit it",
        docs::REGENERATE
    );
}

#[test]
fn the_extension_schema_is_generated_from_the_contract() {
    assert_current(docs::SCHEMA_PATH, &docs::schema());
}

#[test]
fn the_extension_reference_is_generated_from_the_catalog() {
    assert_current(docs::REFERENCE_PATH, &docs::reference());
}
