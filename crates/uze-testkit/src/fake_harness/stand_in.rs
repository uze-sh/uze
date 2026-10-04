//! A stand-in at work: the `uze-fake-harness` binary started under a
//! harness's name, answering by the table [`super::FakeHarnessBuilder`]
//! wrote beside it.
//!
//! Each action does in Rust what the vendor's CLI does to the files UZE reads
//! back, and nothing else: a stand-in is the side effect, never the vendor.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use super::{Action, MarketplaceVendor, McpNames, Pattern, Role, Rule, Step};

/// The directory beside the stand-ins that holds one table per name.
pub(crate) const TABLES: &str = ".fake";

/// Runs the stand-in `executable` names, with `arguments` as its argv.
pub fn run(executable: &Path, arguments: &[String]) -> ExitCode {
    let Some(name) = executable
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
    else {
        return fail("a stand-in has a name");
    };
    let table = executable
        .parent()
        .unwrap_or(Path::new("."))
        .join(TABLES)
        .join(format!("{name}.json"));
    let role: Role = match fs::read(&table)
        .map_err(|error| error.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|error| error.to_string()))
    {
        Ok(role) => role,
        Err(error) => return fail(&format!("{}: {error}", table.display())),
    };
    let joined = arguments.join(" ");
    match role {
        Role::Rules {
            log,
            shared_log,
            version_line,
            rules,
        } => {
            append_line(&log, &joined);
            if let Some(shared) = shared_log {
                append_line(&shared, &format!("{}|{joined}", executable.display()));
            }
            let version = Rule {
                pattern: Pattern::Exact("--version".to_owned()),
                action: Action::Stdout(version_line),
            };
            match std::iter::once(&version)
                .chain(&rules)
                .find(|rule| matches(&rule.pattern, &joined))
            {
                Some(rule) => perform(&rule.action, arguments),
                None => ExitCode::SUCCESS,
            }
        }
        Role::ScriptedAgent { log } => {
            append_line(&log, &joined);
            scripted_agent()
        }
    }
}

/// `--version` is answered first, as a vendor CLI answers it whatever else
/// it does; then the rules a test wrote, in order.
fn matches(pattern: &Pattern, joined: &str) -> bool {
    match pattern {
        Pattern::Exact(expected) => joined == expected,
        Pattern::Prefix(prefix) => joined.starts_with(prefix.as_str()),
        Pattern::Containing(token) => joined.contains(token.as_str()),
    }
}

fn perform(action: &Action, arguments: &[String]) -> ExitCode {
    match action {
        Action::Stdout(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Action::Exit(code) => ExitCode::from(*code as u8),
        Action::TouchFile(path) => {
            touch(path);
            ExitCode::SUCCESS
        }
        Action::RecordLaunch { into } => {
            let mut record = format!("PID={}\n", std::process::id());
            for (name, value) in std::env::vars_os() {
                record.push_str(&format!(
                    "{}={}\n",
                    name.to_string_lossy(),
                    value.to_string_lossy()
                ));
            }
            let partial = into.with_extension("part");
            write(&partial, &record);
            let _ = fs::rename(&partial, into);
            std::thread::sleep(std::time::Duration::from_secs(60));
            ExitCode::SUCCESS
        }
        Action::PrintEnvironment => {
            for (name, value) in std::env::vars_os() {
                println!("{}={}", name.to_string_lossy(), value.to_string_lossy());
            }
            ExitCode::SUCCESS
        }
        Action::McpRegistry { state_dir, names } => mcp_registry(state_dir, *names, arguments),
        Action::StagePlugin { under_home } => {
            if let (Some(source), Some(home)) = (arguments.get(2), uze_platform::home::user_home())
            {
                stage_plugin(Path::new(source), &home.join(under_home));
            }
            ExitCode::SUCCESS
        }
        Action::Refuse { reason } => {
            eprintln!("{reason}");
            ExitCode::from(1)
        }
        Action::McpEntryMark(state_dir) => {
            if let Some(name) = mcp_entry_name(arguments) {
                touch(&state_dir.join(name));
            }
            ExitCode::SUCCESS
        }
        Action::CliPluginInstall { dest, arg_index } => {
            if let Some(source) = arg_index.checked_sub(1).and_then(|at| arguments.get(at)) {
                let source = Path::new(source);
                if let Some(base) = source.file_name() {
                    let _ = copy_tree(source, &dest.join(base));
                }
            }
            ExitCode::SUCCESS
        }
        Action::VendorMarketplace { state_dir, vendor } => {
            vendor_marketplace(state_dir, *vendor, arguments)
        }
        Action::VendorAgy { state_dir, dest } => vendor_agy(state_dir, dest, arguments),
        Action::InteractiveSession { banner } => hold_the_terminal(banner),
        Action::ConversationSession {
            transcripts_root,
            banner,
        } => {
            if let Some(id) = arguments.get(1) {
                let transcript = transcript(transcripts_root, id);
                append_line(&transcript, &json!({ "session": id }).to_string());
            }
            hold_the_terminal(banner)
        }
        Action::RecordConversation { transcripts_root } => {
            if let Some(id) = arguments.get(1) {
                write(&transcript(transcripts_root, id), "{}\n");
            }
            ExitCode::SUCCESS
        }
        Action::AsksOnTheTerminal => {
            ask_on_the_terminal();
            ExitCode::SUCCESS
        }
        Action::ForgeSsh { root } => forge_ssh(root, arguments),
    }
}

fn forge_ssh(root: &Path, arguments: &[String]) -> ExitCode {
    const REFUSED: u8 = 255;
    if arguments.first().is_some_and(|first| first == "-G") {
        return ExitCode::SUCCESS;
    }
    if let Some(host) = arguments
        .iter()
        .find(|argument| argument.ends_with(".invalid"))
    {
        let host = host.rsplit('@').next().unwrap_or(host);
        eprintln!("ssh: Could not resolve hostname {host}: Name or service not known");
        return ExitCode::from(REFUSED);
    }
    if std::env::var_os("SSH_AUTH_SOCK").is_none_or(|socket| socket.is_empty()) {
        eprintln!("git@forge: Permission denied (publickey).");
        return ExitCode::from(REFUSED);
    }
    // The remote command, last: `git-upload-pack '/<path>'`.
    let Some(command) = arguments.last() else {
        return ExitCode::from(REFUSED);
    };
    let path = command
        .split_once(' ')
        .map_or(command.as_str(), |(_, path)| path);
    let path = path.replace('\'', "");
    let path = path.trim_start_matches('/');
    let repository = Some(root.join(path))
        .filter(|repository| repository.is_dir())
        .unwrap_or_else(|| root.join(path.strip_suffix(".git").unwrap_or(path)));
    match crate::forge::upload_pack(&repository) {
        Some(code) => ExitCode::from(code as u8),
        None => ExitCode::from(REFUSED),
    }
}

fn mcp_registry(state_dir: &Path, names: McpNames, arguments: &[String]) -> ExitCode {
    let entry = |name: &str| state_dir.join(name);
    match arguments.get(1).map(String::as_str) {
        Some("get") => match arguments.get(2) {
            Some(name) if entry(name).exists() => ExitCode::SUCCESS,
            _ => ExitCode::from(1),
        },
        Some("remove") => {
            if let Some(name) = arguments.get(2) {
                let _ = fs::remove_file(entry(name));
            }
            ExitCode::SUCCESS
        }
        Some("add") => {
            let Some(name) = mcp_entry_name(arguments) else {
                return ExitCode::SUCCESS;
            };
            let refused = matches!(names, McpNames::Claude)
                && !name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character));
            if refused {
                eprintln!(
                    "Invalid name {name}. Names can only contain letters, numbers, hyphens, and \
                     underscores."
                );
                return ExitCode::from(1);
            }
            touch(&entry(name));
            ExitCode::SUCCESS
        }
        _ => ExitCode::SUCCESS,
    }
}

/// A plugin directory staged under `plugins`, named by its manifest's
/// declared `name` where it has one — not its source directory's, which for
/// a generated envelope is the qualified package id.
fn stage_plugin(source: &Path, plugins: &Path) -> String {
    let id = fs::read(source.join("plugin.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|manifest| manifest["name"].as_str().map(str::to_owned))
        .or_else(|| {
            source
                .file_name()
                .map(|base| base.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let staged = plugins.join(&id);
    let _ = fs::remove_dir_all(&staged);
    let _ = copy_tree(source, &staged);
    id
}

/// `mcp add [--scope x] [--transport y] <name> -- <command>…`: the entry's
/// name is the last word that is no option, before `--`.
fn mcp_entry_name(arguments: &[String]) -> Option<&str> {
    let mut words = arguments.iter().skip(2);
    let mut name = None;
    while let Some(word) = words.next() {
        match word.as_str() {
            "--scope" | "--transport" => {
                words.next();
            }
            "--" => break,
            other => name = Some(other),
        }
    }
    name
}

/// Where a harness keeps the conversation `id` it was started in: one
/// directory per working directory, every character that is not
/// alphanumeric a hyphen — Claude Code's own naming.
fn transcript(transcripts_root: &Path, id: &str) -> PathBuf {
    transcripts_root
        .join(slug(&working_directory()))
        .join(format!("{id}.jsonl"))
}

/// Claude Code's name for a working directory's transcripts, as it was
/// observed to write it; the integration that reads them is pinned by the
/// same cases, so the stand-in and the product cannot drift apart.
fn slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

/// The directory as the process that started this one named it: `PWD`
/// when it still names where this process stands, as a shell reads it.
fn working_directory() -> PathBuf {
    let actual = std::env::current_dir().unwrap_or_default();
    std::env::var_os("PWD")
        .map(PathBuf::from)
        .filter(|named| {
            named
                .canonicalize()
                .is_ok_and(|named| actual.canonicalize().is_ok_and(|actual| named == actual))
        })
        .unwrap_or(actual)
}

fn hold_the_terminal(banner: &str) -> ExitCode {
    println!("{banner}");
    println!("cwd {}", working_directory().display());
    let _ = std::io::stdout().flush();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line == "/exit" {
            break;
        }
        println!("> {line}");
        let _ = std::io::stdout().flush();
    }
    ExitCode::SUCCESS
}

/// Asks on the terminal when it has one, as a program on this platform
/// finds it, and waits for an answer nobody gives when no one is watching.
fn ask_on_the_terminal() {
    let Some((input, mut output)) = uze_platform::stdio::terminal() else {
        return;
    };
    let _ = write!(output, "Start now? [y/N] ");
    let _ = output.flush();
    let mut answer = String::new();
    let _ = std::io::BufReader::new(input).read_line(&mut answer);
}

fn vendor_marketplace(
    state_dir: &Path,
    vendor: MarketplaceVendor,
    arguments: &[String],
) -> ExitCode {
    let _ = fs::create_dir_all(state_dir);
    let joined = arguments.join(" ");
    let root = read(&state_dir.join("root"));
    let name = read(&state_dir.join("name"));
    if joined.starts_with("plugin marketplace add") {
        let root = arguments.last().cloned().unwrap_or_default();
        write(&state_dir.join("root"), &root);
        let name = marketplace_name(Path::new(&root)).unwrap_or_else(|| "uze-store".to_owned());
        write(&state_dir.join("name"), &name);
    } else if joined.starts_with("plugin install") || joined.starts_with("plugin add") {
        if let Some(selector) = arguments.get(2).filter(|selector| !selector.is_empty()) {
            append_line(&state_dir.join("installed"), selector);
            if let Some(root) = root.as_deref() {
                let id = selector.split('@').next().unwrap_or(selector);
                let _ = fs::create_dir_all(Path::new(root).join(id));
            }
        }
    } else if joined.starts_with("plugin marketplace list") {
        let listed = match (vendor, &root) {
            (MarketplaceVendor::Claude, Some(root)) => {
                json!([{ "name": name.unwrap_or_default(), "path": root }])
            }
            (MarketplaceVendor::Claude, None) => json!([]),
            (MarketplaceVendor::Codex, Some(root)) => {
                json!({ "marketplaces": [{ "name": name.unwrap_or_default(), "root": root }] })
            }
            (MarketplaceVendor::Codex, None) => json!({ "marketplaces": [] }),
        };
        print!("{listed}");
    } else if joined.starts_with("plugin list") {
        let installed = lines(&state_dir.join("installed"));
        let listed = match vendor {
            MarketplaceVendor::Claude => Value::Array(
                installed
                    .iter()
                    .map(|selector| json!({ "id": selector, "enabled": true }))
                    .collect(),
            ),
            MarketplaceVendor::Codex => json!({
                "installed": installed
                    .iter()
                    .map(|selector| codex_installed(selector, root.as_deref(), name.as_deref()))
                    .collect::<Vec<_>>(),
            }),
        };
        print!("{listed}");
    } else if joined.starts_with(match vendor {
        MarketplaceVendor::Claude => "plugin uninstall",
        MarketplaceVendor::Codex => "plugin remove",
    }) && let Some(selector) = arguments.get(2)
    {
        drop_line(&state_dir.join("installed"), selector);
    }
    ExitCode::SUCCESS
}

/// What Codex reports for an installed `selector`: the entry its own
/// catalogue lists under the selector's active name, at that entry's
/// `source.path` — looked up in the catalogue UZE wrote, never derived from
/// the selector's shape (an alias can make the two differ).
fn codex_installed(selector: &str, root: Option<&str>, name: Option<&str>) -> Value {
    let active = selector.split('@').next().unwrap_or(selector);
    let root = root.unwrap_or_default();
    let relative = fs::read(Path::new(root).join(".agents/plugins/marketplace.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|catalog| {
            catalog["plugins"]
                .as_array()?
                .iter()
                .find(|entry| entry["name"] == active)
                .and_then(first_path)
        })
        .unwrap_or_default();
    json!({
        "pluginId": selector,
        "enabled": true,
        "installed": true,
        "marketplaceName": name.unwrap_or_default(),
        "path": format!("{root}/{}", relative.trim_start_matches("./")),
    })
}

/// The first `path` an entry carries, at any depth, in document order.
fn first_path(value: &Value) -> Option<String> {
    match value {
        Value::Object(fields) => fields.iter().find_map(|(key, value)| match value {
            Value::String(path) if key == "path" => Some(path.clone()),
            other => first_path(other),
        }),
        Value::Array(items) => items.iter().find_map(first_path),
        _ => None,
    }
}

/// The `name` a marketplace's catalogue declares, in the order a vendor
/// looks for the catalogue.
fn marketplace_name(root: &Path) -> Option<String> {
    [
        "marketplace.json",
        ".claude-plugin/marketplace.json",
        ".agents/plugins/marketplace.json",
    ]
    .iter()
    .find_map(|catalog| {
        let bytes = fs::read(root.join(catalog)).ok()?;
        let catalog: Value = serde_json::from_slice(&bytes).ok()?;
        catalog["name"].as_str().map(str::to_owned)
    })
}

fn vendor_agy(state_dir: &Path, dest: &Path, arguments: &[String]) -> ExitCode {
    let _ = fs::create_dir_all(state_dir);
    let joined = arguments.join(" ");
    let installed = state_dir.join("installed");
    if joined.starts_with("plugin install") {
        let Some(root) = arguments.get(2).map(PathBuf::from) else {
            return ExitCode::SUCCESS;
        };
        let id = stage_plugin(&root, dest);
        append_line(&installed, &id);
    } else if joined.starts_with("plugin list") {
        let imports: Vec<Value> = lines(&installed)
            .iter()
            .map(|id| json!({ "name": id }))
            .collect();
        print!("{}", json!({ "imports": imports }));
    } else if joined.starts_with("plugin uninstall")
        && let Some(id) = arguments.get(2)
    {
        drop_line(&installed, id);
        let _ = fs::remove_dir_all(dest.join(id));
    }
    ExitCode::SUCCESS
}

/// The scripted agent's loop (see [`super::FakeHarness::scripted_agent`]).
fn scripted_agent() -> ExitCode {
    let Some(scripts) = std::env::var_os("AGENT_SCRIPTS").map(PathBuf::from) else {
        return fail("AGENT_SCRIPTS must name the scripts directory");
    };
    let slot = working_directory()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let log = scripts.join(format!("{slot}.log"));
    let step = |name: &str| {
        let file = scripts.join(format!("{slot}.{name}.json"));
        if let Some(steps) = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Step>>(&bytes).ok())
        {
            let here = std::env::current_dir().unwrap_or_default();
            perform_steps(&steps, &here, &log);
        }
    };
    step("start");
    touch(&scripts.join(format!("{slot}.started")));
    let inbox = scripts.join(format!("{slot}.inbox"));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        append_line(&inbox, &line);
        let answered = if line.contains("rebase --continue") {
            Some(("conflict", "resolved"))
        } else if line.contains("checks failed") {
            Some(("gate", "fixed"))
        } else if line.contains("Open a pull request") || line.contains("Open a merge request") {
            Some(("request", "opened"))
        } else if line == "quit" {
            return ExitCode::SUCCESS;
        } else {
            None
        };
        if let Some((name, mark)) = answered {
            step(name);
            touch(&scripts.join(format!("{slot}.{mark}")));
        }
    }
    ExitCode::SUCCESS
}

/// `steps`, done in `here`, what each run says appended to `log`.
fn perform_steps(steps: &[Step], here: &Path, log: &Path) {
    for step in steps {
        match step {
            Step::Write { path, content } => write(&here.join(path), content),
            Step::Run {
                program,
                args,
                env,
                capture,
            } => {
                let output = std::process::Command::new(program)
                    .args(args)
                    .envs(env.iter().map(|(name, value)| (name, value)))
                    .current_dir(here)
                    .stdin(std::process::Stdio::null())
                    .output();
                let Ok(output) = output else { continue };
                match capture {
                    Some(capture) => write(
                        &here.join(capture),
                        &String::from_utf8_lossy(&output.stdout),
                    ),
                    None => append_line(log, String::from_utf8_lossy(&output.stdout).trim_end()),
                }
                append_line(log, String::from_utf8_lossy(&output.stderr).trim_end());
            }
            Step::Within { named_in, steps } => {
                let named = read(&here.join(named_in)).unwrap_or_default();
                perform_steps(steps, &here.join(named.trim()), log);
            }
        }
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(drop)
    }
}

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = OpenOptions::new().create(true).append(true).open(path);
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .filter(|text| !text.is_empty())
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, text);
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn append_line(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// Removes every line that is exactly `line`: a package id holding `/` is a
/// literal here, never a pattern.
fn drop_line(path: &Path, line: &str) {
    let kept: Vec<String> = lines(path)
        .into_iter()
        .filter(|kept| kept != line)
        .collect();
    let mut text = kept.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    write(path, &text);
}

fn fail(reason: &str) -> ExitCode {
    eprintln!("uze-fake-harness: {reason}");
    ExitCode::from(97)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same cases `uze-integrations`' Claude session reader is pinned to.
    #[test]
    fn a_working_directory_is_named_as_claude_names_it() {
        for (cwd, named) in [
            ("/home/x/.worktrees/y", "-home-x--worktrees-y"),
            (
                r"C:\uze-journeys\04-an-agent\projects\demo-app",
                "C--uze-journeys-04-an-agent-projects-demo-app",
            ),
        ] {
            assert_eq!(slug(Path::new(cwd)), named);
        }
    }
}
