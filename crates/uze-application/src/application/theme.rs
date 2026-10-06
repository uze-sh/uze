//! Selecting a theme.
//!
//! The facade's whole interest in appearance is an id and a list of files.
//! Resolving that id into colours and glyphs is the design system's job, and
//! deliberately not routed through here: a read model that carried resolved
//! colours would put the vocabulary in the domain's public surface, where
//! every token added later becomes a change to this crate.

use serde::Serialize;
use uze_core::{
    Result,
    appearance::{self, ADAPTIVE, Background},
};

use super::services::Themes;

/// A theme the operator can select.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThemeSummary {
    /// What `uze config theme set` takes. A file's own stem, or a built-in's name.
    pub id: String,
    pub active: bool,
    /// `None` for a theme UZE carries rather than one someone wrote.
    pub path: Option<std::path::PathBuf>,
}

/// A glyph set the operator can select.
///
/// Carries an id and nothing else, for the same reason [`ThemeSummary`]
/// carries no colours: what the set actually *draws* is the design system's
/// to answer, and a surface that wants to show it asks there.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GlyphSetSummary {
    pub id: String,
    pub active: bool,
}

impl Themes<'_> {
    /// Every theme this machine can load: the adaptive selection, the ones
    /// UZE carries, then the ones the operator wrote. A file that shadows a
    /// built-in's name wins, the way a local override should — and is listed
    /// once, as theirs. One named `adaptive` is left out: that name selects
    /// the adaptive behaviour, so the file could never be drawn in.
    #[tracing::instrument(name = "themes.list", skip_all, err)]
    pub fn list(&self, builtin: &[&str]) -> Result<Vec<ThemeSummary>> {
        let active = self.active()?;
        let mut written = appearance::available(&self.0.home)?;
        written.retain(|(id, _)| id != ADAPTIVE);
        let shadowed: Vec<&str> = written.iter().map(|(id, _)| id.as_str()).collect();
        let summaries = std::iter::once(&ADAPTIVE)
            .chain(builtin)
            .filter(|id| !shadowed.contains(*id))
            .map(|id| ThemeSummary {
                id: (*id).to_owned(),
                active: false,
                path: None,
            })
            .chain(written.iter().map(|(id, path)| ThemeSummary {
                id: id.clone(),
                active: false,
                path: Some(path.clone()),
            }))
            .map(|summary| ThemeSummary {
                active: active.as_deref() == Some(summary.id.as_str()),
                ..summary
            });
        Ok(summaries.collect())
    }

    /// The selected theme's id, or `None` while the operator has not chosen.
    // At debug: the workspace reads it every few seconds while it follows
    // the desktop, and an `info` span each time fills the journal. A failure
    // is still recorded, at error.
    #[tracing::instrument(name = "themes.active", level = "debug", skip_all, err)]
    pub fn active(&self) -> Result<Option<String>> {
        appearance::active(&self.0.home)
    }

    /// The file a written theme lives in, or `None` when the id names a
    /// built-in (or nothing at all).
    #[tracing::instrument(name = "themes.path_of", skip_all, fields(id = %id), err)]
    pub fn path_of(&self, id: &str) -> Result<Option<std::path::PathBuf>> {
        Ok(appearance::available(&self.0.home)?
            .into_iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, path)| path))
    }

    /// Records the selection. Does not validate that the id names anything:
    /// only the design system can say whether a theme resolves, and it says
    /// so by loading it.
    #[tracing::instrument(name = "themes.select", skip_all, fields(id = %id), err)]
    pub fn select(&self, id: &str) -> Result<()> {
        appearance::set_active(&self.0.home, id)
    }

    /// Every glyph set UZE carries, marking the selected one. Takes the
    /// available ids for the same reason [`Themes::list`] does: which sets
    /// exist is the design system's list, not the domain's.
    #[tracing::instrument(name = "themes.glyph_sets", skip_all)]
    pub fn glyph_sets(&self, available: &[&str]) -> Result<Vec<GlyphSetSummary>> {
        let selected = self.glyphs()?;
        Ok(available
            .iter()
            .map(|id| GlyphSetSummary {
                id: (*id).to_owned(),
                active: selected.as_deref() == Some(*id),
            })
            .collect())
    }

    /// The selected glyph set's id, or `None` while the operator has not
    /// chosen — which draws the default set, not nothing.
    #[tracing::instrument(name = "themes.glyphs", skip_all, err)]
    pub fn glyphs(&self) -> Result<Option<String>> {
        appearance::glyphs(&self.0.home)
    }

    /// The theme an adaptive selection draws in on this background, or
    /// `None` while the operator has not said.
    #[tracing::instrument(name = "themes.adaptive", skip_all, err)]
    pub fn adaptive(&self, background: Background) -> Result<Option<String>> {
        appearance::adaptive(&self.0.home, background)
    }

    /// Records the theme an adaptive selection draws in on this
    /// background. Does not select it.
    #[tracing::instrument(name = "themes.set_adaptive", skip_all, fields(id = %id), err)]
    pub fn set_adaptive(&self, background: Background, id: &str) -> Result<()> {
        appearance::set_adaptive(&self.0.home, background, id)
    }

    /// The background last observed, or dark while none has been: dark is
    /// what every theme UZE drew before it could ask was made for.
    #[tracing::instrument(name = "themes.background", level = "debug", skip_all)]
    pub fn background(&self) -> Background {
        appearance::observed_background(&self.0.home).unwrap_or(Background::Dark)
    }

    /// Remembers what was observed, and says whether it changed.
    #[tracing::instrument(name = "themes.observe_background", level = "debug", skip_all, err)]
    pub fn observe_background(&self, background: Background) -> Result<bool> {
        appearance::observe_background(&self.0.home, background)
    }

    /// Records the glyph set. Independent of [`Themes::select`] in both
    /// directions: neither call reads or writes the other's half.
    #[tracing::instrument(name = "themes.select_glyphs", skip_all, fields(id = %id), err)]
    pub fn select_glyphs(&self, id: &str) -> Result<()> {
        appearance::set_glyphs(&self.0.home, id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{UzeApplication, UzeHome};

    /// `adaptive` is a selection, not a file: a theme written under that
    /// name could never be drawn in, so it neither replaces the selection
    /// in the list nor appears as a second entry.
    #[test]
    fn a_theme_written_as_adaptive_does_not_take_the_selections_place() {
        let root = uze_testkit::temp::scratch("theme-adaptive-reserved");
        let home = UzeHome::at(&root);
        std::fs::create_dir_all(home.themes_dir()).expect("themes dir");
        std::fs::write(home.themes_dir().join("adaptive.json"), "{}").expect("theme file");
        let app = UzeApplication::new(home, Vec::new());

        let listed = app.themes().list(&["default"]).expect("listed");
        let adaptive: Vec<&super::ThemeSummary> = listed
            .iter()
            .filter(|theme| theme.id == super::ADAPTIVE)
            .collect();
        assert_eq!(adaptive.len(), 1, "{listed:?}");
        assert_eq!(adaptive[0].path, None, "the selection, not the file");
    }

    #[test]
    fn a_theme_the_operator_wrote_shadows_a_builtin_of_the_same_name() {
        let root = uze_testkit::temp::scratch("theme-shadow");
        let home = UzeHome::at(&root);
        std::fs::create_dir_all(home.themes_dir()).expect("themes dir");
        std::fs::write(home.themes_dir().join("default.json"), "{}").expect("theme file");
        let app = UzeApplication::new(home, Vec::new());

        let listed = app.themes().list(&["default"]).expect("listed");
        let shadowed: Vec<&super::ThemeSummary> = listed
            .iter()
            .filter(|theme| theme.id == "default")
            .collect();
        assert_eq!(shadowed.len(), 1, "listed twice: {listed:?}");
        assert_eq!(
            listed[0].id,
            super::ADAPTIVE,
            "the adaptive selection leads"
        );
        assert!(
            shadowed[0].path.is_some(),
            "the built-in won over their file"
        );
    }
}
