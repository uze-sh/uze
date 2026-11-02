//! What the application does only because an earlier release left it,
//! and the door to the vocabulary that declares it (the `earlier-releases`
//! rule): the regions earlier releases wrote into a project's files
//! ([`regions`]).

pub use uze_core::document::earlier::{Earlier, FLOOR, Lasts, Release};

pub(crate) mod regions;

/// This crate's entries. The AGENTS.md regions are declared where they are
/// named, in `uze-core`.
pub fn earlier() -> Vec<Earlier> {
    vec![
        Earlier {
            name: "compiled extension views earlier releases kept loose in runtime/extensions",
            // Removed when the compiled-views collector next runs: unbounded.
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "package copies earlier releases kept in runtime/packages for every harness",
            // Kept while a receipt names one: unbounded.
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "package copies in runtime/packages kept for beta.22's MCP servers",
            // beta.22 starts an installed package's server from its copy
            // there; once it is below the floor, `collect_package_copies`
            // keeps only what a receipt names.
            lasts: Lasts::Until(Release::beta(1, 0, 0, 22)),
            spelled: &[],
            site: file!(),
        },
    ]
}

/// The owned map's name for one of [`package_copies_dir`]'s copies, which
/// the delivered-packages collector claims.
pub(crate) const PACKAGE_COPIES: &str = "retired_package_dir";

/// Where an earlier release kept a copy of every installed package and
/// pointed every harness at it.
pub(crate) fn package_copies_dir(home: &uze_core::UzeHome) -> std::path::PathBuf {
    home.retired_packages_dir()
}

/// Whether an earlier release's package copies are still on this machine.
pub(crate) fn holds_package_copies(home: &uze_core::UzeHome) -> bool {
    package_copies_dir(home).is_dir()
}

/// Removes the package copies an earlier release made that no receipt
/// names any more (`named` is the ledger, serialized) and no installed
/// package serves MCP from (`serves`, by package id: the floor release,
/// running beside this one, starts its servers from here), then their
/// directory once it is empty. Called under the mutation lock.
pub(crate) fn collect_package_copies(
    home: &uze_core::UzeHome,
    named: &str,
    serves: &dyn Fn(&str) -> bool,
) -> Vec<std::path::PathBuf> {
    let root = package_copies_dir(home);
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let path = entry.path();
        if named.contains(path.to_string_lossy().as_ref())
            || serves(&entry.file_name().to_string_lossy())
        {
            continue;
        }
        let gone = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if gone.is_ok() {
            removed.push(path);
        }
    }
    let _ = std::fs::remove_dir(&root);
    removed
}

/// Removes the compiled views an earlier release kept loose in the
/// extensions' corner (`loose_compiled_extension_path`): sixteen hex digits
/// and `.json`, which nothing in that corner is named today.
pub(crate) fn remove_loose_compiled_views(home: &uze_core::UzeHome) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(home.runtime_extensions_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_suffix(".json"))
                    .is_some_and(|key| {
                        key.len() == 16 && key.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
        })
        .filter(|path| std::fs::remove_file(path).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_loose_compiled_view_leaves_the_extensions_corner() {
        let home = uze_core::UzeHome::at(uze_testkit::temp::scratch("loose-compiled-views"));
        let corner = home.runtime_extensions_dir();
        std::fs::create_dir_all(home.compiled_extensions_dir()).unwrap();
        let loose = home.loose_compiled_extension_path("0123456789abcdef");
        std::fs::write(&loose, "{}").unwrap();
        std::fs::write(home.extension_costs_path(), "{}").unwrap();
        std::fs::write(home.compiled_extension_path("fedcba9876543210"), "{}").unwrap();

        assert_eq!(remove_loose_compiled_views(&home), vec![loose.clone()]);
        assert!(!loose.exists());
        assert!(home.extension_costs_path().exists());
        assert!(home.compiled_extension_path("fedcba9876543210").exists());
        assert!(corner.join("compiled").is_dir());
    }

    #[test]
    fn an_earlier_package_copy_goes_once_no_receipt_names_it() {
        let home = uze_core::UzeHome::at(uze_testkit::temp::scratch("earlier-package-copies"));
        let named = package_copies_dir(&home).join("flow@ai");
        let unnamed = package_copies_dir(&home).join("gone@ai");
        std::fs::create_dir_all(&named).unwrap();
        std::fs::create_dir_all(&unnamed).unwrap();
        let ledger = format!("{{\"target\":\"{}/skills/x\"}}", named.display());

        assert_eq!(
            collect_package_copies(&home, &ledger, &|_| false),
            vec![unnamed.clone()]
        );
        assert!(named.is_dir() && !unnamed.exists());

        std::fs::remove_dir_all(&named).unwrap();
        assert!(collect_package_copies(&home, "", &|_| false).is_empty());
        assert!(
            !holds_package_copies(&home),
            "the emptied directory goes too"
        );
    }

    #[test]
    fn an_earlier_copy_an_installed_server_may_run_from_is_kept() {
        let home = uze_core::UzeHome::at(uze_testkit::temp::scratch("earlier-server-copies"));
        let serving = package_copies_dir(&home).join("tracker@ai");
        std::fs::create_dir_all(&serving).unwrap();

        let removed = collect_package_copies(&home, "", &|id| id == "tracker@ai");

        assert!(removed.is_empty(), "{removed:?}");
        assert!(
            serving.is_dir(),
            "the floor release, running beside this one, starts its servers from here"
        );
    }
}
