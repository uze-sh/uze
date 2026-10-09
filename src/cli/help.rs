//! Help as UZE prints it: the root page, a command's page, and their examples.

use crate::*;

pub(crate) enum HelpTopic {
    Root,
    Setup,
    /// A command's own page, drawn from its clap definition, with the
    /// words that reach it (`config theme`).
    Command(Box<clap::Command>, String),
}

/// Whether clap reads `arguments` as a command to run — which makes a
/// `help` among them one of its values rather than a request for a page.
/// Asked only about a line ending in a bare `help`, so the second parse it
/// costs is one no ordinary invocation pays.
pub(crate) fn is_a_command(arguments: &[String]) -> bool {
    Cli::command()
        .try_get_matches_from(std::iter::once("uze".to_owned()).chain(arguments.iter().cloned()))
        .is_ok()
}

pub(crate) fn help_topic(arguments: &[String]) -> Option<HelpTopic> {
    let (path, requested) = match arguments {
        [command] if command == "help" || command == "--help" || command == "-h" => (&[][..], true),
        [command, path @ ..] if command == "help" && path.len() <= 1 => (path, true),
        [path @ .., command] if command == "--help" || command == "-h" => (path, true),
        // A trailing bare `help` asks for a page only where the line means
        // nothing else. `uze market help` asks about marketplaces, but
        // `uze market add help` asked to add a marketplace called `help`
        // and `uze remove help` to remove a plugin by that name — both of
        // which used to print a page and exit 0 having done nothing, which
        // a script reads as success.
        [path @ .., command] if command == "help" && !is_a_command(arguments) => (path, true),
        _ => (&[][..], false),
    };
    if !requested {
        return None;
    }
    match path {
        [] => Some(HelpTopic::Root),
        [command, ..] if command == "setup" => Some(HelpTopic::Setup),
        // As deep as the words name commands: `config theme set --help`
        // is about `set`, not about `config`. Hidden commands are found
        // too — `agent` is hidden from the root list, not from its reader.
        words => {
            let mut current = Cli::command();
            let mut reached = Vec::new();
            for word in words {
                let Some(next) = current.get_subcommands().find(|candidate| {
                    candidate.get_name() == word
                        || candidate.get_visible_aliases().any(|alias| alias == word)
                }) else {
                    break;
                };
                reached.push(next.get_name().to_owned());
                current = next.clone();
            }
            (!reached.is_empty()).then(|| HelpTopic::Command(Box::new(current), reached.join(" ")))
        }
    }
}

pub(crate) fn print_help(topic: HelpTopic) {
    match topic {
        HelpTopic::Root => print_root_help(),
        HelpTopic::Setup => print_setup_help(),
        HelpTopic::Command(command, path) => print_command_help(&command, &path),
    }
}

/// The subcommands a help page lists: every one clap would show.
pub(crate) fn visible_subcommands(
    command: &clap::Command,
) -> impl Iterator<Item = clap::Command> + '_ {
    command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
        .cloned()
}

/// A command's summary as one row has room for: its doc comment's first
/// sentence.
pub(crate) fn summary(command: &clap::Command) -> String {
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    let sentence = about.split(". ").next().unwrap_or_default();
    sentence.trim_end_matches('.').to_owned()
}

/// `name <arg> [arg]` — how a command is spelled with its positional
/// arguments, `<command>` where it only groups others, and `[command]`
/// where it also runs on its own.
pub(crate) fn spelling(command: &clap::Command) -> String {
    let mut spelled = command.get_name().to_owned();
    for argument in command.get_positionals() {
        let name = argument.get_id().as_str();
        if argument.is_required_set() {
            spelled.push_str(&format!(" <{name}>"));
        } else {
            spelled.push_str(&format!(" [{name}]"));
        }
    }
    if command.has_subcommands() {
        if command.is_subcommand_required_set() {
            spelled.push_str(" <command>");
        } else {
            spelled.push_str(" [command]");
        }
    }
    spelled
}

pub(crate) fn command_rows(commands: impl Iterator<Item = clap::Command>) -> Vec<Vec<String>> {
    commands
        .map(|command| {
            let spelled = spelling(&command);
            let arguments = spelled
                .strip_prefix(command.get_name())
                .unwrap_or_default()
                .trim()
                .to_owned();
            vec![
                progress::accent(command.get_name()),
                progress::label(arguments),
                summary(&command),
            ]
        })
        .collect()
}

/// The root help's commands, flat and in the order a reader meets them:
/// the package manager, the workspace, then what looks after the machine.
/// A group is told apart by its hue and the blank line above it, never by a
/// heading: a heading such as "Project" would claim a scope the command
/// reports for itself (ADR-054). Each row is a name, an example of what
/// follows it, and a line short enough to fit an 80-column terminal; the
/// command's own `--help` holds the rest.
/// A root help row: the command, an example of what follows it, one line.
pub(crate) type RootCommand = (&'static str, &'static str, &'static str);

pub(crate) const ROOT_COMMANDS: &[(progress::CommandGroup, &[RootCommand])] = &[
    (
        progress::CommandGroup::Packages,
        &[
            (
                "install",
                "[plugin@market]",
                "Install a plugin, or everything declared here",
            ),
            ("update", "[plugin]", "Move plugins to their newest version"),
            (
                "remove",
                "<plugin> [-m]",
                "Take a plugin out of this project or machine",
            ),
            (
                "status",
                "[-m]",
                "What this project has, and what it still needs",
            ),
            ("inspect", "<plugin>", "How your agents receive a plugin"),
            ("market", "<command>", "Where plugins come from"),
        ],
    ),
    (
        progress::CommandGroup::Workspace,
        &[(
            "workspace",
            "[stop|allow]",
            "Open the terminal workspace where agents run",
        )],
    ),
    (
        progress::CommandGroup::Machine,
        &[
            ("setup", "[agent...]", "Install and set up your agents"),
            ("config", "<command>", "Appearance, icons and notifications"),
            ("doctor", "", "Diagnose, and repair what is safe to"),
            ("upgrade", "", "Install the latest uze"),
        ],
    ),
];

/// What the root help shows a newcomer typing, whole and copyable.
pub(crate) const ROOT_EXAMPLES: &[&str] = &[
    "uze lint@acme",
    "uze market add owner/repo",
    "uze config theme set dracula",
];

pub(crate) const DOCUMENTATION_URL: &str = "https://uze.sh/docs";
/// The same documentation as one plain-text index, the form an agent
/// reading this help can fetch and follow without rendering a site.
pub(crate) const DOCUMENTATION_FOR_AGENTS_URL: &str = "https://uze.sh/llms.txt";

pub(crate) fn print_root_help() {
    println!(
        "{} {}",
        progress::title("uze"),
        progress::label(env!("CARGO_PKG_VERSION"))
    );
    println!(
        "{}",
        progress::label("Agents come and go. Your work stays.")
    );
    println!();
    println!(
        "{}  uze <command> [args] [options]",
        progress::title("Usage")
    );
    println!();
    // One grid for every row on the page: with no heading over a group,
    // it is the columns running unbroken past the blank lines that make
    // the blank lines read as the separation.
    let mut groups: Vec<Vec<Vec<String>>> = ROOT_COMMANDS
        .iter()
        .map(|(group, commands)| {
            commands
                .iter()
                .map(|(name, arguments, line)| {
                    vec![
                        progress::command_name(name, *group),
                        progress::label(arguments),
                        (*line).to_owned(),
                    ]
                })
                .collect()
        })
        .collect();
    groups.push(vec![vec![
        progress::label("help"),
        progress::label("<command> --help"),
        "Help for one command".to_owned(),
    ]]);
    let mut rendered = progress::aligned_groups(groups);
    let help = rendered.pop().unwrap_or_default();
    for group in rendered {
        println!("{group}");
        println!();
    }
    println!("{help}");
    println!();
    println!("{}", progress::title("Examples"));
    for example in ROOT_EXAMPLES {
        println!("  {example}");
    }
    println!();
    println!(
        "{}",
        progress::aligned_rows(vec![
            vec![progress::label("docs"), DOCUMENTATION_URL.to_owned()],
            vec![
                progress::label("for agents"),
                DOCUMENTATION_FOR_AGENTS_URL.to_owned(),
            ],
        ])
    );
}

pub(crate) fn print_command_help(command: &clap::Command, path: &str) {
    let spelled = spelling(command);
    let arguments = spelled
        .strip_prefix(command.get_name())
        .unwrap_or_default()
        .trim();
    println!(
        "{} {}",
        progress::title(format!("uze {path}")),
        progress::label(arguments)
    );
    println!("{}.", summary(command));
    if command.has_subcommands() {
        println!();
        println!("{}", progress::title("Commands"));
        println!(
            "{}",
            progress::aligned_rows_wrapped(
                command_rows(visible_subcommands(command)),
                progress::terminal_width()
            )
        );
    }
    println!();
    println!("{}", progress::title("Options"));
    println!(
        "{}",
        progress::aligned_rows_wrapped(option_rows(command), progress::terminal_width())
    );
    if let Some((_, examples)) = COMMAND_EXAMPLES.iter().find(|(name, _)| *name == path) {
        println!();
        println!("{}", progress::title("Examples"));
        for example in *examples {
            println!("  {example}");
        }
    }
}

/// Whole command lines a page shows, for the commands whose arguments are
/// easiest to get wrong from their names alone.
pub(crate) const COMMAND_EXAMPLES: &[(&str, &[&str])] = &[
    (
        "install",
        &[
            "uze install",
            "uze install lint@acme",
            "uze install -m lint@acme",
        ],
    ),
    ("update", &["uze update", "uze update lint"]),
    ("remove", &["uze remove lint", "uze remove lint -m"]),
    ("status", &["uze status", "uze status -m"]),
    ("inspect", &["uze inspect lint"]),
    (
        "market",
        &[
            "uze market add acme/plugins",
            "uze market link acme ~/src/acme-plugins",
        ],
    ),
    (
        "config",
        &["uze config theme set dracula", "uze config icons nerd"],
    ),
];

/// A command's own flags as `-m, --machine   what it does`, then the help
/// flag every command takes. The global flags are the root's to list.
pub(crate) fn option_rows(command: &clap::Command) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = command
        .get_arguments()
        .filter(|argument| {
            !argument.is_positional()
                && !argument.is_hide_set()
                && !argument.is_global_set()
                && !matches!(
                    argument.get_id().as_str(),
                    "help" | "verbose" | "quiet" | "color"
                )
        })
        .map(|argument| {
            let mut spelled = Vec::new();
            if let Some(short) = argument.get_short() {
                spelled.push(format!("-{short}"));
            }
            if let Some(long) = argument.get_long() {
                spelled.push(format!("--{long}"));
            }
            let mut flag = spelled.join(", ");
            if argument.get_action().takes_values() {
                let value = argument
                    .get_value_names()
                    .and_then(|names| names.first().map(ToString::to_string))
                    .unwrap_or_else(|| argument.get_id().as_str().to_uppercase());
                flag.push_str(&format!(" <{}>", value.to_lowercase()));
            }
            let help = match argument.get_help() {
                Some(help) => help.to_string(),
                None if argument.get_id() == "format" => "Print as text or json".to_owned(),
                None => String::new(),
            };
            vec![progress::accent(flag), help]
        })
        .collect();
    rows.push(vec![
        progress::accent("-h, --help"),
        "Show this help".to_owned(),
    ]);
    rows
}
