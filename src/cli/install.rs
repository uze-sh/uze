//! Installing a plugin into a project or onto the machine, and the trust prompts it asks.

use crate::*;

/// A plugin or marketplace name as a person typed it. Every name on record
/// is lowercase, so the case typed is forgiven before anything resolves.
pub(crate) fn typed_name(value: &str) -> std::result::Result<String, std::convert::Infallible> {
    Ok(uze_application::typed_name(value))
}

/// `uze <plugin>@<market>` — the project shorthand. Reached only from
/// `Command::External`; see that variant's doc comment for the precedence
/// argument. Semantically equivalent to `add_project_plugin`: this
/// function does no acquisition/reconciliation logic of its own, only
/// argument classification and rendering — the Application layer owns
/// desired-project-state → `agents.lock` → reconciliation → machine
/// delivery (see `docs/adr/019-...md`).
pub(crate) fn run_shorthand(app: &UzeApplication, args: Vec<String>, verbose: bool) -> Result<()> {
    // `external_subcommand` only ever fires with at least one token: the
    // unrecognized first argument that caused the fallback.
    let first = args[0].clone();
    let (plugin, marketplace) = match uze_application::parse_plugin_marketplace_spec(&first) {
        Ok(parsed) => parsed,
        // No `@` at all: this was never shorthand — it's an unrecognized
        // command. Reuse `clap`'s own error type/formatting/exit code
        // rather than a hand-rolled message, so this reads exactly like
        // any other "unrecognized subcommand" `clap` produces natively.
        Err(_) if !first.contains('@') => {
            // A near miss of a command is a typo; anything else may be a
            // plugin named without the marketplace it comes from.
            let hint = match nearest_command(&first) {
                Some(command) => format!("uze {command}"),
                None => format!(
                    "uze {first}@<market>  {}",
                    progress::label("a plugin is named with its marketplace")
                ),
            };
            progress::error(&format!("unknown command `{first}`"), Some(&hint));
            std::process::exit(2);
        }
        // Contains `@` but fails the shorthand's own validation (e.g. a
        // path segment, an empty half) — a real shorthand-input error, not
        // a missing command, so it flows through the ordinary `uze:
        // {error}` path like every other application error.
        Err(error) => return Err(error),
    };

    // Parses the *rest* of argv through `clap` — not a hand-rolled loop —
    // so an unrecognized flag is rejected with `clap`'s own error and exit
    // code instead of being silently ignored (the exact bug this replaces;
    // see docs/adr/019-...md).
    let shorthand = ShorthandArgs::try_parse_from(std::iter::once("uze".to_owned()).chain(args))
        .unwrap_or_else(|error| usage_error(error));

    if shorthand.verbose {
        progress::follow_steps(true);
    }
    install_package(
        app,
        PackageInstall {
            plugin: &plugin,
            marketplace: &marketplace,
            machine: shorthand.machine,
            authority: trust_authority(shorthand.trust).as_ref(),
            name_authority: name_collision_authority(shorthand.alias, shorthand.replace).as_ref(),
            format: shorthand.format,
            verbose: verbose || shorthand.verbose,
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

/// What `uze install <spec>` was handed: one package, or a project to
/// converge — the directory given, or the one it was run from.
pub(crate) enum InstallTarget {
    Package { plugin: String, marketplace: String },
    Project(Option<PathBuf>),
}

/// Reads the install positional. A `name@marketplace` spec is a package; a
/// project's root directory is the project to converge, which is what
/// `uze install <path>` meant before the package form shared its position.
/// Only a root: any other directory is far likelier a package's own
/// directory handed over as a direct source, which the marketplace
/// contract refuses — and converging whatever project happens to enclose
/// it would write into one nobody named.
pub(crate) fn install_target(
    app: &UzeApplication,
    positional: Option<String>,
    path: Option<PathBuf>,
) -> Result<InstallTarget> {
    let Some(positional) = positional else {
        return Ok(InstallTarget::Project(path));
    };
    if positional.contains('@') {
        let (plugin, marketplace) = uze_application::parse_plugin_marketplace_spec(&positional)?;
        return Ok(InstallTarget::Package {
            plugin,
            marketplace,
        });
    }
    if path.is_none() && app.project().is_root(Path::new(&positional)) {
        return Ok(InstallTarget::Project(Some(PathBuf::from(positional))));
    }
    Err(uze_application::UzeError::UnknownPackage(format!(
        "`{positional}` is not a `name@marketplace` spec; add its marketplace with \
         `uze market add <market>` first, then install with \
         `uze install {positional}@<market>`"
    )))
}

/// One package install as the caller asked for it — `uze install <spec>`
/// and the `uze <spec>` shorthand are the same operation with two
/// spellings, so they share this and its rendering.
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
        print!("{}", render_add_summary(&report));
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
            render_requirement_gaps(std::slice::from_ref(&report.plugin))
        );
    }
    let package = report.plugin.id.clone();
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
/// the machine is unchanged — `uze remove x -m && uze install y -m`
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
            // evidence printed to stdout left `uze install > install.log`
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
pub(crate) fn trust_evidence(request: &uze_application::TrustRequest) -> String {
    let mut text = String::from("\n");
    text.push_str(if request.previously_trusted {
        "This update introduces an executable capability the installed package did not have\n"
    } else {
        "This package requests an executable capability\n"
    });
    text.push_str(&format!("\nSource\n  {}\n", request.requested_source));
    if request.resolved_source != request.requested_source {
        text.push_str(&format!("\nResolved\n  {}\n", request.resolved_source));
    }
    text.push_str(&format!("\nPackage\n  {}\n", request.package_id));
    text.push_str("\nMCP\n");
    for capability in &request.executable {
        text.push_str(&format!(
            "  {} → {} {}\n",
            capability.name,
            capability.command,
            capability.arguments.join(" ")
        ));
        if let Some(directory) = &capability.working_directory {
            text.push_str(&format!("    cwd {directory}\n"));
        }
        for (key, value) in &capability.environment {
            text.push_str(&format!("    env {key}={value}\n"));
        }
    }
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
