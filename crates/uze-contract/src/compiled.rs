//! A view after it was read and checked: what the engine binds to data.
//!
//! Produced only by the decoder in `uze-core`, which refuses anything the
//! catalog does not allow before one of these exists, so whoever holds a
//! [`CompiledView`] may trust its shape and never parses a document again.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Identity;
use crate::binding::{Binding, Condition, Path};
use crate::catalog::NodeName;
use crate::manifest::Manifest;

/// Where something was written, in the source file: a byte offset and a
/// length. Kept on every node so the inspector and a later diagnostic can
/// point back at the line that produced what is on screen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

/// An extension ready to be hosted: its manifest and every view its
/// surfaces name, compiled.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompiledExtension {
    pub identity: Identity,
    pub manifest: Manifest,
    /// By the view file's path as the manifest names it.
    pub views: BTreeMap<String, CompiledView>,
}

impl CompiledExtension {
    pub fn view_of(&self, surface: &str) -> Option<&CompiledView> {
        let surface = self.manifest.surface(surface)?;
        self.views.get(&surface.view)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompiledView {
    /// The view file's path, relative to the extension's directory.
    pub file: String,
    /// The top-level nodes, in order.
    pub items: Vec<Item>,
    pub templates: BTreeMap<String, Vec<Item>>,
    pub lookups: Vec<Lookup>,
}

/// One entry of a node list: a node, or a template used in its place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Item {
    Node(Node),
    /// `use "name"`: the template's items, bound against the current item.
    Use {
        template: String,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub name: NodeName,
    /// Positional arguments, already checked against the catalog's kinds.
    pub args: Vec<Binding>,
    /// Properties by name, without `each`, `when`, `unless`, `show` and
    /// `show-any`, which are lifted into [`Node::each`] and
    /// [`Node::condition`].
    pub props: BTreeMap<String, Binding>,
    /// Repeat this node once per element of the list found here.
    pub each: Option<Path>,
    pub condition: Option<Condition>,
    pub children: Vec<Item>,
    pub span: Span,
}

/// A table that maps a value to a role, a label or a symbol, so a program
/// says `partial` and the view decides that reads as `warning`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Lookup {
    pub name: String,
    /// The locale this table speaks, or `None` for a table every locale
    /// shares.
    pub locale: Option<String>,
    /// Used when a value has no entry.
    pub default: Option<String>,
    pub entries: BTreeMap<String, String>,
}

/// The entry `table` gives `value` in `locale`: that locale's table, else
/// the one with no locale, else the first table of that name; then the
/// entry, else the table's default.
pub fn look_up<'a>(
    lookups: &'a [Lookup],
    table: &str,
    value: &str,
    locale: &str,
) -> Option<&'a str> {
    let named = || lookups.iter().filter(|lookup| lookup.name == table);
    let chosen = named()
        .find(|lookup| lookup.locale.as_deref() == Some(locale))
        .or_else(|| named().find(|lookup| lookup.locale.is_none()))
        .or_else(|| named().next())?;
    chosen
        .entries
        .get(value)
        .or(chosen.default.as_ref())
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(locale: Option<&str>, entries: &[(&str, &str)], default: Option<&str>) -> Lookup {
        Lookup {
            name: "t".into(),
            locale: locale.map(str::to_owned),
            default: default.map(str::to_owned),
            entries: entries
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
        }
    }

    #[test]
    fn a_lookup_follows_the_locale_then_falls_back() {
        let lookups = vec![
            table(Some("pt"), &[("next", "▶ executar fase")], None),
            table(Some("en"), &[("next", "▶ run phase")], None),
        ];
        assert_eq!(
            look_up(&lookups, "t", "next", "pt"),
            Some("▶ executar fase")
        );
        assert_eq!(look_up(&lookups, "t", "next", "en"), Some("▶ run phase"));
        assert_eq!(
            look_up(&lookups, "t", "next", "fr"),
            Some("▶ executar fase")
        );
    }

    #[test]
    fn a_missing_value_draws_the_default() {
        let lookups = vec![table(None, &[("partial", "warning")], Some("muted"))];
        assert_eq!(look_up(&lookups, "t", "partial", "pt"), Some("warning"));
        assert_eq!(look_up(&lookups, "t", "other", "pt"), Some("muted"));
        assert_eq!(look_up(&lookups, "missing", "x", "pt"), None);
    }
}
