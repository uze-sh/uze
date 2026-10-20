use super::*;
use std::fs;

const FOLDER: &str = ".agents/rules/acme@corp";

fn checkout(label: &str, manifest: Option<&str>) -> PathBuf {
    let root = uze_testkit::temp::scratch(label);
    uze_testkit::git::mark_repository(&root);
    if let Some(text) = manifest {
        fs::create_dir_all(root.join(".agents")).unwrap();
        fs::write(root.join(MANIFEST_FILE), text).unwrap();
    }
    root
}

fn manifest_text(root: &Path) -> String {
    fs::read_to_string(root.join(MANIFEST_FILE)).unwrap()
}

#[test]
fn a_missing_manifest_is_created_holding_uzes_entry_alone() {
    let root = checkout("rules-manifest-create", None);
    assert_eq!(register(&root, FOLDER).unwrap(), Registration::Added);
    assert_eq!(
        manifest_text(&root),
        "{\n  \"entries\": [\n    {\n      \"path\": \".agents/rules/acme@corp\"\n    }\n  ]\n}\n"
    );
    assert_eq!(register(&root, FOLDER).unwrap(), Registration::Present);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_operators_entries_and_keys_are_written_back_in_their_order() {
    let operator = r#"{"version": 2, "entries": [{"path": ".agents/rules/team", "exclude": ["old.md"], "note": "ours"}], "inherits": ["../shared"]}"#;
    let root = checkout("rules-manifest-keep", Some(operator));

    assert_eq!(register(&root, FOLDER).unwrap(), Registration::Added);
    let written: Value = serde_json::from_str(&manifest_text(&root)).unwrap();
    let keys: Vec<&str> = written
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["version", "entries", "inherits"]);
    let original: Value = serde_json::from_str(operator).unwrap();
    assert_eq!(written["entries"][0], original["entries"][0]);
    assert_eq!(written["entries"][1], serde_json::json!({"path": FOLDER}));
    assert_eq!(written["inherits"], original["inherits"]);

    assert_eq!(
        unregister(&root, FOLDER).unwrap(),
        Unregistration::Removed {
            manifest_removed: false
        }
    );
    let back: Value = serde_json::from_str(&manifest_text(&root)).unwrap();
    assert_eq!(back, original, "only UZE's entry went");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_manifest_that_does_not_parse_is_never_written() {
    let broken = "{ \"entries\": [ { \"path\": \".agents/rules/team\" } ";
    for text in [broken, "[]", "{\"entries\": {}}"] {
        let root = checkout("rules-manifest-broken", Some(text));
        assert!(matches!(
            register(&root, FOLDER).unwrap(),
            Registration::Unreadable(_)
        ));
        assert!(matches!(
            unregister(&root, FOLDER).unwrap(),
            Unregistration::Unreadable(_)
        ));
        assert_eq!(manifest_text(&root), text, "its bytes are kept");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn a_folder_the_operator_registered_is_left_as_they_wrote_it() {
    let operator =
        r#"{"entries": [{"path": "./.agents/rules/acme@corp/", "exclude": ["noisy.md"]}]}"#;
    let root = checkout("rules-manifest-operators-entry", Some(operator));
    assert_eq!(register(&root, FOLDER).unwrap(), Registration::Present);
    assert_eq!(unregister(&root, FOLDER).unwrap(), Unregistration::Absent);
    assert_eq!(manifest_text(&root), operator);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_manifest_goes_once_uzes_last_entry_does_and_nothing_else_is_left() {
    let root = checkout("rules-manifest-remove", None);
    register(&root, FOLDER).unwrap();
    register(&root, ".agents/rules/other@corp").unwrap();
    assert_eq!(
        unregister(&root, FOLDER).unwrap(),
        Unregistration::Removed {
            manifest_removed: false
        }
    );
    assert_eq!(
        unregister(&root, ".agents/rules/other@corp").unwrap(),
        Unregistration::Removed {
            manifest_removed: true
        }
    );
    assert!(!root.join(MANIFEST_FILE).exists());
    assert_eq!(unregister(&root, FOLDER).unwrap(), Unregistration::Absent);

    // A key of the operator's keeps the manifest, entries or none.
    fs::write(
        root.join(MANIFEST_FILE),
        r#"{"inherits": ["../shared"], "entries": [{"path": ".agents/rules/acme@corp"}]}"#,
    )
    .unwrap();
    assert_eq!(
        unregister(&root, FOLDER).unwrap(),
        Unregistration::Removed {
            manifest_removed: false
        }
    );
    assert!(root.join(MANIFEST_FILE).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_path_is_read_relative_to_the_checkout_and_never_outside_it() {
    let root = Path::new("/work/project");
    assert_eq!(
        within(root, ".agents/rules/team").unwrap(),
        ".agents/rules/team"
    );
    assert_eq!(
        within(root, "./.agents//rules/team/").unwrap(),
        ".agents/rules/team"
    );
    assert_eq!(
        within(root, ".agents/rules/../rules/x").unwrap(),
        ".agents/rules/x"
    );
    assert_eq!(within(root, ".").unwrap(), "");
    assert_eq!(within(root, "../elsewhere"), None);
    // A path opening with `/` is absolute on Unix only; Windows needs a
    // drive, and the case table covers it through `{root}`.
    #[cfg(unix)]
    {
        assert_eq!(
            within(root, "/work/project/.agents/rules/team").unwrap(),
            ".agents/rules/team"
        );
        assert_eq!(within(root, "/work/other/rules"), None);
    }
}
