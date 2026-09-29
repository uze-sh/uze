//! The package root every harness is handed.
//!
//! A harness that installs a plugin reads files beside its skills —
//! `phases/`, a UI, a script a hook runs — through the root `${PLUGIN_ROOT}`
//! names, and a hook may write there (a build, a dependency install). That
//! root is therefore a copy in the generated tier rather than the Store: a
//! write into it changes nothing the lock pins, and the next delivery puts
//! the package back as it is.

use std::path::PathBuf;

use crate::{Result, UzeError, delivery::persistence, home::UzeHome, store::StoredPackage};

/// Held while a delivered root is compared and replaced: an install hands a
/// package to every harness at once, and each delivery materializes the
/// same root first. Across processes the mutation lock already serializes.
static MATERIALIZING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Makes [`UzeHome::delivered_package_dir`] hold exactly `package`, copying
/// it again only when the two differ, and returns it.
pub fn materialize(home: &UzeHome, package: &StoredPackage) -> Result<PathBuf> {
    let _materializing = MATERIALIZING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let destination = home.delivered_package_dir(&package.id);
    let wanted = crate::digest::tree_sha256(&package.root).map_err(|source| UzeError::Read {
        path: package.root.clone(),
        source,
    })?;
    if destination.is_dir()
        && crate::digest::tree_sha256(&destination).is_ok_and(|present| present == wanted)
    {
        return Ok(destination);
    }
    persistence::replace_dir(&destination, |stage| {
        crate::store::copy_tree(&package.root, stage)
    })?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        acquisition::{PackageSource, Provenance, ResolvedSource},
        store::PackageId,
    };

    fn installed(name: &str) -> (UzeHome, StoredPackage) {
        let base =
            std::env::temp_dir().join(format!("uze-delivered-root-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = UzeHome::at(base.join("home"));
        let manifest = base.join("store/plugin.json");
        let id = PackageId::from_marketplace_plugin("market", name, &manifest).unwrap();
        let root = home.plugin_dir(&id);
        std::fs::create_dir_all(root.join("phases")).unwrap();
        std::fs::write(root.join("plugin.json"), format!(r#"{{"name":"{name}"}}"#)).unwrap();
        std::fs::write(root.join("phases/plan.md"), "plan").unwrap();
        let package = StoredPackage {
            id,
            manifest: root.join("plugin.json"),
            root,
            provenance: Provenance {
                requested: PackageSource::Local {
                    path: base.join("source"),
                },
                resolved: ResolvedSource::Local {
                    path: base.join("source"),
                },
            },
            active_name: name.to_owned(),
        };
        (home, package)
    }

    #[test]
    fn the_delivered_root_is_a_copy_a_write_cannot_carry_into_the_store() {
        let (home, package) = installed("rooted");
        let root = materialize(&home, &package).unwrap();
        assert!(root.starts_with(home.runtime_dir()));
        assert_eq!(
            std::fs::read_to_string(root.join("phases/plan.md")).unwrap(),
            "plan"
        );

        std::fs::write(root.join("phases/plan.md"), "built over").unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        assert_eq!(
            std::fs::read_to_string(package.root.join("phases/plan.md")).unwrap(),
            "plan",
            "a write into the delivered root never reaches the Store"
        );

        materialize(&home, &package).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("phases/plan.md")).unwrap(),
            "plan"
        );
        assert!(
            !root.join("node_modules").exists(),
            "the next delivery restores it"
        );
    }
}
