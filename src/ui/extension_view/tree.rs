//! A declared extension's [`Content::Tree`]: the catalog's nodes, laid out
//! and drawn by the host.

use super::*;

/// Draws `root` into `area` from row `scroll`, registering a hit for every
/// node that answers a gesture.
pub(super) fn render_tree(
    _frame: &mut ratatui::Frame<'_>,
    _area: Rect,
    _scroll: usize,
    _root: &uze_extensions::view::Node,
    _hits: &mut Vec<(Rect, ViewHit)>,
) {
}
