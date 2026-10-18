//! A project's own declarations, written the way a person would write them.

use std::{collections::BTreeMap, fs, path::Path};

/// Writes `project/agents.yaml` declaring `packages`, each a
/// `plugin@marketplace` id: what makes a package's instructions this
/// project's to carry, since installing it only puts it in the machine's
/// Store. Each marketplace is declared by a path, never read here.
pub fn declare(project: &Path, packages: &[&str]) {
    let mut marketplaces: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for package in packages {
        let (plugin, marketplace) = package
            .rsplit_once('@')
            .unwrap_or_else(|| panic!("`{package}` is not a `plugin@marketplace` id"));
        marketplaces.entry(marketplace).or_default().push(plugin);
    }
    let mut manifest = String::from("marketplaces:\n");
    for (marketplace, plugins) in marketplaces {
        manifest.push_str(&format!(
            "  {marketplace}:\n    path: ../{marketplace}\n    plugins:\n"
        ));
        for plugin in plugins {
            manifest.push_str(&format!("      - {plugin}\n"));
        }
    }
    fs::create_dir_all(project).expect("the project directory");
    fs::write(project.join("agents.yaml"), manifest).expect("agents.yaml is written");
}
