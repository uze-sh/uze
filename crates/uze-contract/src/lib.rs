//! The contract between a declared extension and everything that handles it.
//!
//! A plugin declares an extension in KDL; `uze-core` reads and checks it,
//! `uze-extensions` binds it to data and describes what to draw, and the
//! workspace client draws that. Each of those needs the same definition of
//! what an extension *is*, and none of them may reach the others: the domain
//! never depends on presentation, and presentation never names the domain.
//! This crate is that definition, the way `uze-theme` is the design
//! vocabulary both the TUI and the CLI resolve against.
//!
//! Plain data and pure functions only. No I/O, no UZE crate, no rendering
//! library and no KDL parser: what arrives here has already been parsed, and
//! the client that links this crate must never be able to parse a document
//! a third party wrote (see `add-extension-platform`, D22).
//!
//! - [`manifest`]: what an `extension.kdl` declares.
//! - [`compiled`]: a view after it was checked, ready to bind.
//! - [`binding`]: how a compiled view refers to data and UI state.
//! - [`catalog`]: the nodes a view may name, and what each accepts.
//! - [`clean`]: the one way a string an extension supplied becomes drawable.

#![forbid(unsafe_code)]

pub mod binding;
pub mod catalog;
pub mod clean;
pub mod compiled;
pub mod manifest;

pub use binding::{Binding, Condition, Path, Scope, TextPart};
pub use catalog::{NodeName, PropKind, RoleName};
pub use clean::Clean;
pub use compiled::{CompiledExtension, CompiledView, Item, Lookup, Node, Span};
pub use manifest::{Manifest, Slot, SourceDecl, SurfaceDecl};

/// The extension protocol this build speaks. A manifest declaring another
/// is refused with both numbers, never guessed at.
pub const PROTOCOL: u32 = 1;

/// Ids a declared extension may not take, because a built-in extension or
/// UZE itself already answers to them.
pub const RESERVED_IDS: &[&str] = &["code", "architect", "spec", "uze"];

/// Whether `id` is spelled the way an extension id must be: lowercase ASCII
/// letters, digits and single hyphens, starting with a letter.
///
/// Narrow on purpose (S15 of the first review): an id ends up in paths,
/// keymap names and the attribution UZE draws, and a `../` or a homoglyph
/// in any of those is a way to pass for something else.
pub fn is_valid_id(id: &str) -> bool {
    let mut previous_hyphen = false;
    id.chars().enumerate().all(|(index, ch)| {
        let ok = match ch {
            'a'..='z' => true,
            '0'..='9' | '-' => index > 0,
            _ => false,
        };
        let doubled = ch == '-' && previous_hyphen;
        previous_hyphen = ch == '-';
        ok && !doubled
    }) && !id.is_empty()
        && !id.ends_with('-')
}

/// An extension's identity: which plugin, from which marketplace, and the
/// id it declares. What its state is keyed by, so two plugins declaring the
/// same id never share anything (S4).
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct Identity {
    pub marketplace: String,
    pub plugin: String,
    pub id: String,
}

impl std::fmt::Display for Identity {
    /// `id · plugin@marketplace`, the attribution the host draws.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} · {}@{}", self.id, self.plugin, self.marketplace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_lowercase_words_joined_by_single_hyphens() {
        for good in ["gsd-band", "a", "gpu2", "agents-hud"] {
            assert!(is_valid_id(good), "{good}");
        }
        for bad in ["", "-a", "a-", "a--b", "GSD", "2fa", "a_b", "../x", "а"] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn identity_reads_as_the_attribution() {
        let identity = Identity {
            marketplace: "luizfbalves".into(),
            plugin: "gsd".into(),
            id: "gsd-band".into(),
        };
        assert_eq!(identity.to_string(), "gsd-band · gsd@luizfbalves");
    }
}
