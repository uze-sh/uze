//! The extensions half of `uze agent plugin check`: each extension's
//! fixtures bound to its views by the engine the workspace draws with, and
//! every field a view names that its fixture lacks said as a warning.

use uze_application::CheckedExtension;
use uze_extensions::declared::missing_fields;

use crate::*;

/// The locale a check binds lookups in: the one the workspace draws in
/// until it knows the operator's.
const LOCALE: &str = "en";

/// Binds each surface of `checked` to its source's fixture, under every
/// choice of its tabs, and adds a warning, placed in the view, for each
/// field the view reaches for that the fixture does not hold.
pub(crate) fn bind_fixtures(checked: &mut CheckedExtension) {
    let Some(compiled) = checked.compiled.clone() else {
        return;
    };
    for missing in missing_fields(&compiled, &checked.fixtures, LOCALE) {
        let place = checked
            .place(&missing.view, missing.span.offset)
            .map_or_else(String::new, |(line, column)| format!(":{line}:{column}"));
        checked.warnings.push(format!(
            "{}{place}: `{}` is not in the fixture {}",
            missing.view, missing.field, missing.fixture
        ));
    }
}

/// Each extension that is not clean, or warns, as a block under its id;
/// a clean one is already a row of what the plugin delivers.
pub(crate) fn render_extensions(extensions: &[CheckedExtension]) -> String {
    let mut text = String::new();
    for extension in extensions
        .iter()
        .filter(|extension| !extension.errors.is_empty() || !extension.warnings.is_empty())
    {
        text.push('\n');
        text.push_str(&progress::report_section(&format!(
            "extension {}",
            extension.id
        )));
        for error in &extension.errors {
            text.push_str(&format!("{} {error}\n", progress::error_icon()));
        }
        for warning in &extension.warnings {
            text.push_str(&format!("{} {warning}\n", progress::warning_icon()));
        }
    }
    text
}
