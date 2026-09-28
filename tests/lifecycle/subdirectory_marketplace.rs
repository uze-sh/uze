//! A marketplace whose catalogue sits below its repository's root — a team
//! keeping its plugins in one directory of a larger repository. Read from
//! its mirror, from a linked checkout and from a lock, it resolves under
//! that directory and never at the root, where there is nothing.

use std::{fs, path::Path};

use uze_application::UzeApplication;
use uze_core::{UzeHome, project_lock, trust::AlwaysTrust};

const SUBPATH: &str = "aikit";

fn project(label: &str) -> (UzeApplication, uze_testkit::git::Repository) {
    let repository = uze_testkit::git::Repository::new(label);
    repository.commit_file("README.md", "# p\n");
    let home = uze_testkit::temp::scratch(&format!("{label}-home"));
    (
        UzeApplication::new(UzeHome::at(home), Vec::new()),
        repository,
    )
}

/// A repository whose root is somebody else's project and whose `aikit/`
/// is the marketplace, with its plugin at `./flow` relative to it.
fn monorepo_beside(under: &uze_testkit::git::Repository, at: &Path, body: &str) {
    let marketplace = at.join(SUBPATH);
    fs::create_dir_all(marketplace.join("flow/skills/one")).unwrap();
    fs::write(at.join("README.md"), "# the rest of the repository\n").unwrap();
    fs::write(
        marketplace.join("flow/plugin.json"),
        r#"{"name":"flow","description":"d"}"#,
    )
    .unwrap();
    write_skill(at, body);
    fs::write(
        marketplace.join("marketplace.json"),
        r#"{"name":"mkt","plugins":[{"name":"flow","source":"./flow"}]}"#,
    )
    .unwrap();
    under.git_in(at, &["init", "--quiet", "-b", "main", "."]);
    under.git_in(at, &["config", "user.name", "Test"]);
    under.git_in(at, &["config", "user.email", "t@example.invalid"]);
    under.git_in(at, &["add", "-A"]);
    under.git_in(at, &["commit", "-m", "first"]);
}

fn write_skill(at: &Path, body: &str) {
    fs::write(
        at.join(SUBPATH).join("flow/skills/one/SKILL.md"),
        format!("---\nname: one\ndescription: d\n---\n\n{body}\n"),
    )
    .unwrap();
}

fn installed_skill(application: &UzeApplication) -> String {
    let stored = application
        .plugins()
        .list()
        .unwrap()
        .into_iter()
        .find(|plugin| plugin.id == "flow@mkt")
        .expect("the plugin is installed")
        .store_path
        .join("skills/one/SKILL.md");
    fs::read_to_string(stored).unwrap()
}

#[test]
fn a_marketplace_in_a_local_subdirectory_is_installed_from_its_commits_and_locked_there() {
    let (application, repository) = project("subdir-mirrored");
    let root = repository.root().to_path_buf();
    let market = root.parent().unwrap().join("monorepo");
    monorepo_beside(&repository, &market, "committed body");

    let registration = application
        .marketplace()
        .register(&market.join(SUBPATH).display().to_string())
        .unwrap();
    assert_eq!(
        registration.subpath.as_deref(),
        Some(Path::new(SUBPATH)),
        "{registration:?}"
    );
    assert_eq!(
        registration.checkout.as_deref(),
        Some(market.canonicalize().unwrap().as_path()),
        "what is read is named, not only the identity: {registration:?}"
    );
    assert!(!registration.linked);

    application
        .project()
        .add(
            "flow",
            "mkt",
            &root,
            &AlwaysTrust,
            &uze_application::NoNameCollisionAuthority,
        )
        .unwrap();

    assert!(installed_skill(&application).contains("committed body"));
    let lock = project_lock::load_lock(&root).unwrap().unwrap();
    assert_eq!(
        lock.marketplaces["mkt"].subdirectory.as_deref(),
        Some(Path::new(SUBPATH)),
        "the lock says where the catalogue is: {lock:?}"
    );

    // A second machine has only the lock: it reads the same directory.
    let elsewhere = UzeApplication::new(
        UzeHome::at(uze_testkit::temp::scratch("subdir-mirrored-elsewhere")),
        Vec::new(),
    );
    elsewhere.project().install(&root, &AlwaysTrust).unwrap();
    assert!(installed_skill(&elsewhere).contains("committed body"));
}

#[test]
fn a_remote_locator_names_its_subdirectory_after_a_hash() {
    let (application, repository) = project("subdir-remote");
    let market = repository.root().parent().unwrap().join("monorepo");
    monorepo_beside(&repository, &market, "remote body");

    application
        .marketplace()
        .add(&format!("file://{}#{SUBPATH}", market.display()))
        .unwrap();
    application
        .marketplace()
        .install_plugin("flow@mkt", &AlwaysTrust)
        .unwrap();

    assert!(installed_skill(&application).contains("remote body"));
}

#[test]
fn a_linked_subdirectory_marketplace_follows_the_working_tree_under_its_subpath() {
    let (application, repository) = project("subdir-linked");
    let market = repository.root().parent().unwrap().join("monorepo");
    monorepo_beside(&repository, &market, "committed body");
    application
        .marketplace()
        .add(&market.join(SUBPATH).display().to_string())
        .unwrap();

    // The checkout or its marketplace directory: one link either way.
    application.marketplace().link("mkt", &market).unwrap();
    application
        .marketplace()
        .link("mkt", &market.join(SUBPATH))
        .unwrap();
    application
        .marketplace()
        .install_plugin("flow@mkt", &AlwaysTrust)
        .unwrap();
    write_skill(&market, "edited, never committed");
    application
        .project()
        .update(repository.root(), Some("flow@mkt"), true, &AlwaysTrust)
        .unwrap();

    assert!(
        installed_skill(&application).contains("edited, never committed"),
        "a linked marketplace is read at its subpath of the working tree"
    );
}

#[test]
fn linking_another_directory_of_the_same_repository_is_refused() {
    let (application, repository) = project("subdir-link-other");
    let market = repository.root().parent().unwrap().join("monorepo");
    monorepo_beside(&repository, &market, "a");
    fs::create_dir_all(market.join("other")).unwrap();
    fs::write(market.join("other/notes.md"), "not a marketplace\n").unwrap();
    application
        .marketplace()
        .add(&market.join(SUBPATH).display().to_string())
        .unwrap();

    let refused = application.marketplace().link("mkt", &market.join("other"));

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_subpath_escaping_the_repository_is_refused_and_nothing_is_recorded() {
    let (application, repository) = project("subdir-escape");
    let market = repository.root().parent().unwrap().join("monorepo");
    monorepo_beside(&repository, &market, "a");

    for escaping in ["../elsewhere", "aikit/../../elsewhere"] {
        let refused = application
            .marketplace()
            .add(&format!("file://{}#{escaping}", market.display()));
        assert!(
            matches!(
                refused,
                Err(uze_core::UzeError::MarketplaceSubpathEscapes { .. })
            ),
            "{escaping}: {refused:?}"
        );
    }
    assert!(
        application
            .marketplace()
            .list()
            .unwrap()
            .iter()
            .all(|marketplace| marketplace.name != "mkt"),
        "a refused registration records nothing"
    );
}
