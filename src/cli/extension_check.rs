//! The extensions half of `uze agent plugin check`: each extension's
//! fixtures bound to its views by the engine the workspace draws with, and
//! every field a view names that its fixture lacks said as a warning; with
//! `--run`, each script run in the engine first, so what it answers is
//! what the views are bound to and the tests draw.

use std::collections::BTreeMap;
use std::path::Path;

use uze_application::{CheckedExtension, ExtensionDiagnostic, ExtensionRunner};
use uze_contract::{Answer, Clean, SourceKind};
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

/// One script `--run` ran, and what came of it.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Rehearsal {
    pub(crate) source: String,
    pub(crate) script: String,
    /// The checkout it was run against, as it was named.
    pub(crate) checkout: std::path::PathBuf,
    /// Whether it answered; a script that did not is also an error of its
    /// extension, said with why.
    pub(crate) answered: bool,
}

/// What `--run` ran, by extension id.
pub(crate) type Rehearsals = BTreeMap<String, Vec<Rehearsal>>;

/// Runs every script source, and every program source where this machine
/// confines programs, of every extension that compiled against the
/// checkout `checkout` sits in, and takes what each answered as that
/// source's data in place of its fixture: what the views are bound to and
/// what a test draws unless it gives the source data of its own. A script
/// that fails, or a checkout it may not read, is an error of the
/// extension, placed on its manifest.
pub(crate) fn rehearse_scripts(
    app: &uze_application::UzeApplication,
    extensions: &mut [CheckedExtension],
    checkout: &Path,
) -> Rehearsals {
    let runner = ExtensionRunner::new();
    let mut rehearsals = Rehearsals::new();
    for extension in extensions.iter_mut() {
        let Some(compiled) = extension.compiled.clone() else {
            continue;
        };
        for source in &compiled.manifest.sources {
            let (script, program) = match &source.kind {
                SourceKind::Script(script) => (script.clone(), false),
                SourceKind::Run(program) => (program.program().to_owned(), true),
                SourceKind::Fixture(_) | SourceKind::Events(_) | SourceKind::Agents => continue,
            };
            let script = &script;
            let Some(answer) = runner.rehearse(
                &app.extensions(),
                &extension.dir,
                &compiled.identity,
                &source.id,
                checkout,
            ) else {
                continue;
            };
            let failed = |why: String| {
                ExtensionDiagnostic::whole(
                    script,
                    format!("source `{}` did not answer: {why}", source.id),
                    None,
                )
            };
            let answered = match answer {
                Answer::Answered { envelope, .. } => {
                    let composed = source.output == uze_contract::Output::View;
                    match composed
                        .then(|| {
                            uze_extensions::declared::validate_composed(
                                &envelope.data,
                                &compiled.manifest,
                            )
                        })
                        .transpose()
                    {
                        Err(why) => {
                            extension
                                .errors
                                .push(failed(format!("its tree is refused: {why}")));
                            false
                        }
                        Ok(_) => {
                            extension.fixtures.insert(source.id.clone(), envelope.data);
                            true
                        }
                    }
                }
                Answer::Failed { reason, .. } => {
                    extension.errors.push(failed(reason));
                    false
                }
                // A program a machine that confines none could not run is
                // not the extension's fault: its recorded answers stand in.
                Answer::Withheld { reason } if program => {
                    extension.warnings.push(failed(reason));
                    false
                }
                Answer::Withheld { reason } => {
                    extension.errors.push(failed(reason));
                    false
                }
            };
            rehearsals
                .entry(extension.id.clone())
                .or_default()
                .push(Rehearsal {
                    source: source.id.clone(),
                    script: script.clone(),
                    checkout: checkout.to_path_buf(),
                    answered,
                });
        }
    }
    rehearsals
}

/// Each extension that is not clean, warns, has tests or ran a script, as
/// a block under its id; a clean one with nothing else to say is already
/// a row of what the plugin delivers.
pub(crate) fn render_extensions(
    extensions: &[CheckedExtension],
    runs: &uze::ui::extension_test::Runs,
    rehearsals: &Rehearsals,
) -> String {
    let mut text = String::new();
    for extension in extensions {
        let run = runs.get(&extension.id);
        let tested = run.is_some_and(|run| run.passed > 0 || !run.written.is_empty());
        let ran = rehearsals.get(&extension.id);
        if extension.errors.is_empty() && extension.warnings.is_empty() && !tested && ran.is_none()
        {
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
        for rehearsal in ran
            .into_iter()
            .flatten()
            .filter(|rehearsal| rehearsal.answered)
        {
            text.push_str(&format!(
                "{} {} answered `{}` {}\n",
                progress::success_icon(),
                Clean::label(&rehearsal.script).as_str(),
                Clean::label(&rehearsal.source).as_str(),
                progress::label(format!("in {}", progress::path(&rehearsal.checkout)))
            ));
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
