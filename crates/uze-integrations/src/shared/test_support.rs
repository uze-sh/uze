//! Fixtures the integrations' own tests share.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use uze_core::capability::{Capability, CapabilityKind, Resource};
use uze_core::store::StoredPackage;

pub(crate) fn temp_root(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

/// The package's `commit` Skill.
pub(crate) fn skill_resource(pkg: &StoredPackage) -> Resource {
    let path = pkg.root.join("skills/commit/SKILL.md");
    Resource::from_package(
        pkg.id.clone(),
        pkg.root.clone(),
        Capability {
            kind: CapabilityKind::AgentSkill,
            path,
            payload: Vec::new(),
        },
    )
}

/// The package's MCP server `name`, declared in its `mcp.json`.
pub(crate) fn mcp_resource(pkg: &StoredPackage, name: &str) -> Resource {
    let path = pkg.root.join("mcp.json");
    Resource::from_package_named(
        pkg.id.clone(),
        pkg.root.clone(),
        Capability {
            kind: CapabilityKind::Mcp,
            path,
            payload: Vec::new(),
        },
        name.to_owned(),
    )
}

/// Every path under `root`, directories included.
pub(crate) fn walk(root: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            out.insert(path);
        }
    }
    out
}
