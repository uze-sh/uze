//! The extensions half of `uze agent plugin check`: each extension's
//! fixtures bound to its views by the engine the workspace draws with, and
//! every field a view names that its fixture lacks said as a warning.

use uze_application::CheckedExtension;
use uze_contract::Clean;
use uze_extensions::declared::missing_fields;

use crate::*;

/// The locale a check binds lookups in: the one the workspace draws in
/// until it knows the operator's.
const LOCALE: &str = "en";

/// Binds each surface of `checked` to its source's fixture, under every
/// choice of its tabs, and adds a warning, placed in the view, for each
/// field the view reaches for that the fixture does not hold. The one
/// analysis the check and the language server both add to what the
/// application found, so they say the same thing.
pub(crate) fn bind_fixtures(checked: &mut CheckedExtension) {
    let Some(compiled) = checked.compiled.clone() else {
        return;
    };
    for missing in missing_fields(&compiled, &checked.fixtures, LOCALE) {
        checked.warn_at(
            &missing.view,
            missing.span.offset,
            format!(
                "`{}` is not in the fixture {}",
                missing.field, missing.fixture
            ),
        );
    }
}

/// Each extension that is not clean, warns or has tests, as a block under
/// its id; a clean one with nothing else to say is already a row of what
/// the plugin delivers.
pub(crate) fn render_extensions(
    extensions: &[CheckedExtension],
    runs: &uze::ui::extension_test::Runs,
) -> String {
    let mut text = String::new();
    for extension in extensions {
        let run = runs.get(&extension.id);
        let tested = run.is_some_and(|run| run.passed > 0 || !run.written.is_empty());
        if extension.errors.is_empty() && extension.warnings.is_empty() && !tested {
            continue;
        }
        text.push('\n');
        // An extension's id is its directory's name and its diagnostics
        // quote its files: both are the plugin's, cleaned before a terminal.
        text.push_str(&progress::report_section(&format!(
            "extension {}",
            Clean::label(&extension.id).as_str()
        )));
        for error in &extension.errors {
            let said = Clean::text(&error.to_string());
            text.push_str(&format!("{} {}\n", progress::error_icon(), said.as_str()));
        }
        for warning in &extension.warnings {
            let said = Clean::text(&warning.to_string());
            text.push_str(&format!("{} {}\n", progress::warning_icon(), said.as_str()));
        }
        if let Some(run) = run {
            for written in &run.written {
                text.push_str(&format!("{} wrote {written}\n", progress::success_icon()));
            }
            if run.passed > 0 {
                text.push_str(&format!(
                    "{} {} passed\n",
                    progress::success_icon(),
                    count(run.passed, "test")
                ));
            }
        }
    }
    text
}
