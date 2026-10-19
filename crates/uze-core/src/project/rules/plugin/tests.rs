use super::*;
use std::fs;

const UI: &str = "---\ntrigger: glob\nglobs: **/ui/**\n---\nUI components live in ui/.\n";
const ALWAYS: &str = "---\ntrigger: always_on\n---\nThis organization writes tests first.\n";

fn rule(name: &str, text: &str) -> ShippedRule {
    ShippedRule {
        name: name.to_owned(),
        bytes: text.as_bytes().to_vec(),
    }
}

fn checkout(label: &str) -> PathBuf {
    let root = uze_testkit::temp::scratch(label);
    fs::create_dir_all(root.join(".git")).unwrap();
    root
}

fn acme() -> Namespace {
    Namespace::new("acme-conventions", "acme")
}

#[test]
fn the_copy_location_is_a_flat_file_carrying_the_plugin_prefix() {
    let root = Path::new("/project");
    assert_eq!(
        copy_location(root, &acme(), "ui.md"),
        Path::new("/project/.agents/rules/acme-conventions@acme--ui.md")
    );
    assert_eq!(
        acme().copy_identity("ui.md"),
        ".agents/rules/acme-conventions@acme--ui.md"
    );
}

#[test]
fn the_owner_of_a_copy_is_derived_from_its_name_alone() {
    let (owner, rule) = Namespace::owner_of(".agents/rules/acme-conventions@acme--ui.md").unwrap();
    assert_eq!(owner, acme());
    assert_eq!(rule, "ui.md");
    // A rule's own name may hold `--`: the first one after the marketplace
    // ends it, and names hold none.
    let (_, rule) = Namespace::owner_of("acme@corp--a--b.md").unwrap();
    assert_eq!(rule, "a--b.md");
    for own in [
        "ui.md",
        "ui@x.md",
        "Acme@corp--ui.md",
        "acme@--ui.md",
        "acme@corp--",
    ] {
        assert!(
            Namespace::owner_of(own).is_none(),
            "{own} is the project's own"
        );
    }
}

#[test]
fn a_package_ships_the_markdown_files_directly_under_its_rules() {
    let package = uze_testkit::temp::scratch("plugin-rules-shipped");
    fs::create_dir_all(package.join("rules/nested")).unwrap();
    fs::write(package.join("rules/ui.md"), UI).unwrap();
    fs::write(package.join("rules/always.md"), ALWAYS).unwrap();
    fs::write(package.join("rules/notes.txt"), "x").unwrap();
    fs::write(package.join("rules/nested/deep.md"), UI).unwrap();

    let shipped = shipped_rules(&package).unwrap();
    let names: Vec<&str> = shipped.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["always.md", "ui.md"]);
    assert_eq!(shipped[1].bytes, UI.as_bytes());
    assert_eq!(
        unread_entries(&package),
        ["rules/nested", "rules/notes.txt"]
    );
    assert!(shipped_rules(&package.join("absent")).unwrap().is_empty());
}

#[test]
fn copies_are_discovered_as_the_projects_own_rules_are() {
    let root = checkout("plugin-rules-discovered");
    let next = [rule("ui.md", UI)];
    converge_copies(&root, &acme(), &next, &next).unwrap();
    let found = crate::rules::discover(&root).unwrap();
    assert_eq!(
        found
            .rules
            .iter()
            .map(|rule| rule.path.as_str())
            .collect::<Vec<_>>(),
        [".agents/rules/acme-conventions@acme--ui.md"]
    );
}

#[test]
fn install_writes_byte_for_byte_and_never_touches_the_projects_own_rule() {
    let root = checkout("plugin-rules-install");
    fs::create_dir_all(root.join(RULES_DIR)).unwrap();
    fs::write(root.join(RULES_DIR).join("ui.md"), "mine").unwrap();
    fs::write(
        root.join(RULES_DIR).join("other@corp--ui.md"),
        "another plugin's",
    )
    .unwrap();
    let next = [rule("ui.md", UI), rule("always.md", ALWAYS)];

    let done = converge_copies(&root, &acme(), &next, &next).unwrap();

    assert_eq!(done.written.len(), 2);
    assert_eq!(
        fs::read(copy_location(&root, &acme(), "ui.md")).unwrap(),
        UI.as_bytes()
    );
    assert_eq!(
        fs::read_to_string(root.join(RULES_DIR).join("ui.md")).unwrap(),
        "mine"
    );
    assert_eq!(
        fs::read_to_string(root.join(RULES_DIR).join("other@corp--ui.md")).unwrap(),
        "another plugin's"
    );
    let again = converge_copies(&root, &acme(), &next, &next).unwrap();
    assert!(again.is_empty(), "{again:?}");
}

#[test]
fn a_drifted_copy_is_never_overwritten_and_a_missing_one_is_restored() {
    let root = checkout("plugin-rules-drift");
    let next = [rule("ui.md", UI), rule("always.md", ALWAYS)];
    converge_copies(&root, &acme(), &next, &next).unwrap();
    let edited = copy_location(&root, &acme(), "ui.md");
    fs::write(&edited, "edited by hand").unwrap();
    fs::remove_file(copy_location(&root, &acme(), "always.md")).unwrap();

    let findings = inspect_copies(&root, &acme(), &next).unwrap();
    let states: Vec<(&str, CopyState)> = findings
        .iter()
        .map(|finding| (finding.rule.as_str(), finding.state))
        .collect();
    assert_eq!(
        states,
        [
            ("always.md", CopyState::Missing),
            ("ui.md", CopyState::Drifted)
        ]
    );

    let done = converge_copies(&root, &acme(), &next, &next).unwrap();
    assert_eq!(done.drifted, [acme().copy_identity("ui.md")]);
    assert_eq!(done.written, [acme().copy_identity("always.md")]);
    assert_eq!(fs::read_to_string(&edited).unwrap(), "edited by hand");
}

#[test]
fn an_update_replaces_current_copies_and_removes_dropped_rules() {
    let root = checkout("plugin-rules-update");
    let before = [rule("ui.md", UI), rule("legacy.md", ALWAYS)];
    converge_copies(&root, &acme(), &before, &before).unwrap();
    let changed = "---\ntrigger: glob\nglobs: **/ui/**\n---\nChanged.\n";
    let after = [rule("ui.md", changed), rule("new.md", ALWAYS)];

    let done = converge_copies(&root, &acme(), &before, &after).unwrap();

    assert_eq!(
        done.written,
        [
            acme().copy_identity("ui.md"),
            acme().copy_identity("new.md")
        ]
    );
    assert_eq!(done.removed, [acme().copy_identity("legacy.md")]);
    assert!(!copy_location(&root, &acme(), "legacy.md").exists());
    assert_eq!(
        fs::read_to_string(copy_location(&root, &acme(), "ui.md")).unwrap(),
        changed
    );
}

#[test]
fn an_update_or_a_removal_refuses_a_drifted_copy_and_leaves_an_extra_file() {
    let root = checkout("plugin-rules-update-drift");
    let before = [rule("ui.md", UI), rule("legacy.md", ALWAYS)];
    converge_copies(&root, &acme(), &before, &before).unwrap();
    fs::write(copy_location(&root, &acme(), "legacy.md"), "kept").unwrap();
    fs::write(copy_location(&root, &acme(), "mine.md"), "extra").unwrap();

    let done = converge_copies(&root, &acme(), &before, &[]).unwrap();

    assert_eq!(done.removed, [acme().copy_identity("ui.md")]);
    assert_eq!(done.drifted, [acme().copy_identity("legacy.md")]);
    assert_eq!(done.extra, [acme().copy_identity("mine.md")]);
    assert_eq!(
        fs::read_to_string(copy_location(&root, &acme(), "legacy.md")).unwrap(),
        "kept"
    );
    assert_eq!(namespaces_present(&root), [acme()]);
}

// A symbolic link, which Windows lets an ordinary account make only in developer mode.
#[cfg(unix)]
#[test]
fn a_copy_that_is_a_link_is_never_written_through() {
    let root = checkout("plugin-rules-link");
    fs::create_dir_all(root.join(RULES_DIR)).unwrap();
    let elsewhere = root.join("elsewhere.md");
    fs::write(&elsewhere, UI).unwrap();
    std::os::unix::fs::symlink(&elsewhere, copy_location(&root, &acme(), "ui.md")).unwrap();
    let changed = [rule("ui.md", ALWAYS)];

    let done = converge_copies(&root, &acme(), &[rule("ui.md", UI)], &changed).unwrap();

    assert_eq!(done.drifted, [acme().copy_identity("ui.md")]);
    assert_eq!(fs::read_to_string(&elsewhere).unwrap(), UI);
}

/// A clone on Windows with `core.autocrlf` holds every committed copy in
/// CRLF: current, not edited, and still replaced by an update.
#[test]
fn a_copy_git_checked_out_with_crlf_is_current_and_still_updated() {
    let root = checkout("plugin-rules-crlf");
    let installed = vec![rule("ui.md", UI)];
    converge_copies(&root, &acme(), &installed, &installed).unwrap();
    let copy = copy_location(&root, &acme(), "ui.md");
    fs::write(&copy, UI.replace('\n', "\r\n")).unwrap();

    let findings = inspect_copies(&root, &acme(), &installed).unwrap();
    assert_eq!(findings[0].state, CopyState::Current);

    let updated = vec![rule("ui.md", &UI.replace("live in", "belong in"))];
    let done = converge_copies(&root, &acme(), &installed, &updated).unwrap();
    assert!(done.drifted.is_empty(), "{done:?}");
    assert!(fs::read_to_string(&copy).unwrap().contains("belong in"));

    // A real edit is still somebody's, whatever its line endings.
    fs::write(&copy, "edited by hand\r\n").unwrap();
    let findings = inspect_copies(&root, &acme(), &updated).unwrap();
    assert_eq!(findings[0].state, CopyState::Drifted);
    let _ = fs::remove_dir_all(root);
}
