//! The declared extensions as a list of them says what each is: whose it
//! is, whether its author is working on it here, what it offers and what
//! was wrong with it when it last compiled.

use uze_contract::{Clean, CompiledExtension, Identity, Slot};
use uze_core::capability::extension::Diagnostic;

use super::InstalledExtension;
use crate::application::services::Extensions;

/// One declared extension, as the Extensions screen lists it. Every text
/// it holds is the plugin's, cleaned to be drawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredExtensionSummary {
    pub identity: Identity,
    /// Read from a linked marketplace's working tree.
    pub linked: bool,
    /// What its manifest calls it, or its id when it did not compile.
    pub name: String,
    pub description: String,
    /// The titles of the views it opens from the tab strip.
    pub views: Vec<String>,
    /// The titles of the sections it draws in the sidebar.
    pub sections: Vec<String>,
    /// The titles of the drawers it docks beside the agent's pane.
    pub drawers: Vec<String>,
    /// What its last compile said was wrong, one problem a line. Empty
    /// when it compiled.
    pub errors: Vec<String>,
}

impl Extensions<'_> {
    /// Every declared extension the installed plugins carry, compiled
    /// (from the cache when nothing changed) to say what it offers or why
    /// it cannot.
    #[tracing::instrument(name = "extensions.summaries", skip_all)]
    pub fn summaries(&self) -> Vec<DeclaredExtensionSummary> {
        self.installed()
            .into_iter()
            .map(|installed| {
                let compiled = self.compile(&installed);
                summarize(installed, compiled.as_ref())
            })
            .collect()
    }
}

fn summarize(
    installed: InstalledExtension,
    compiled: Result<&CompiledExtension, &Vec<Diagnostic>>,
) -> DeclaredExtensionSummary {
    let clean = |text: &str| Clean::label(text).into_string();
    let mut summary = DeclaredExtensionSummary {
        name: clean(&installed.identity.id),
        identity: installed.identity,
        linked: installed.linked,
        description: String::new(),
        views: Vec::new(),
        sections: Vec::new(),
        drawers: Vec::new(),
        errors: Vec::new(),
    };
    match compiled {
        Ok(compiled) => {
            let manifest = &compiled.manifest;
            if !manifest.name.trim().is_empty() {
                summary.name = clean(&manifest.name);
            }
            summary.description = clean(&manifest.description);
            for surface in &manifest.surfaces {
                let title = clean(surface.title.as_deref().unwrap_or(&manifest.name));
                match surface.slot {
                    Slot::View => summary.views.push(title),
                    Slot::Section => summary.sections.push(title),
                    Slot::Drawer => summary.drawers.push(title),
                }
            }
        }
        Err(diagnostics) => {
            summary.errors = diagnostics
                .iter()
                .map(|diagnostic| clean(&diagnostic.to_string()))
                .collect();
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;

    fn installed() -> InstalledExtension {
        InstalledExtension {
            identity: Identity {
                marketplace: "market".to_owned(),
                plugin: "gsd".to_owned(),
                id: "gsd-band".to_owned(),
            },
            dir: PathBuf::from("/market/plugins/gsd/extensions/gsd-band"),
            linked: true,
        }
    }

    #[test]
    fn a_compiled_extension_says_what_it_offers() {
        let surface = |id: &str, slot, title: Option<&str>| uze_contract::SurfaceDecl {
            id: id.to_owned(),
            slot,
            view: format!("{id}.kdl"),
            source: "board".to_owned(),
            title: title.map(str::to_owned),
        };
        let compiled = CompiledExtension {
            identity: installed().identity,
            manifest: uze_contract::Manifest {
                protocol: 1,
                id: "gsd-band".to_owned(),
                name: "GSD\u{1b}[31m".to_owned(),
                description: "Phases and plans".to_owned(),
                surfaces: vec![
                    surface("board", Slot::View, Some("Board")),
                    surface("band", Slot::Section, None),
                ],
                sources: Vec::new(),
            },
            views: BTreeMap::new(),
        };

        let summary = summarize(installed(), Ok(&compiled));
        assert!(summary.linked);
        assert!(!summary.name.contains('\u{1b}'), "{:?}", summary.name);
        assert!(summary.name.starts_with("GSD"));
        assert_eq!(summary.description, "Phases and plans");
        assert_eq!(summary.views, ["Board"]);
        assert_eq!(summary.sections.len(), 1, "titled by the manifest's name");
        assert!(summary.errors.is_empty());
    }

    #[test]
    fn one_that_did_not_compile_is_named_by_its_id_and_says_why() {
        let diagnostics = vec![Diagnostic {
            file: "extension.kdl".to_owned(),
            line: 1,
            column: 1,
            message: "protocol 2 is newer than 1".to_owned(),
            suggestion: None,
        }];
        let summary = summarize(installed(), Err(&diagnostics));
        assert_eq!(summary.name, "gsd-band");
        assert_eq!(
            summary.errors,
            ["extension.kdl:1:1: protocol 2 is newer than 1"]
        );
        assert!(summary.views.is_empty() && summary.sections.is_empty());
    }
}
