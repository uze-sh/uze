//! What the application does only because an earlier release left it,
//! and the door to the vocabulary that declares it (the `earlier-releases`
//! rule): the regions earlier releases wrote into a project's files
//! ([`regions`]).

pub use uze_core::document::earlier::{Earlier, FLOOR, Lasts, Release};

pub(crate) mod regions;

/// This crate's entries. The AGENTS.md regions are declared where they are
/// named, in `uze-core`.
pub fn earlier() -> Vec<Earlier> {
    vec![Earlier {
        name: "compiled extension views earlier releases kept loose in runtime/extensions",
        // Removed when the compiled-views collector next runs: unbounded.
        lasts: Lasts::Unbounded,
        spelled: &[],
        site: file!(),
    }]
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
}
