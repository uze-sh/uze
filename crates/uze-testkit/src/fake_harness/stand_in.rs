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

use super::{Action, MarketplaceVendor, Pattern, Role, Rule};

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
            version_line,
            rules,
        } => {
            append_line(&log, &joined);
            let version = Rule {
                pattern: Pattern::Exact("--version".to_owned()),
                action: Action::Stdout(version_line),
            };
            match rules
                .iter()
                .chain(std::iter::once(&version))
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

/// The rules a test wrote come first, so one of them may claim
/// `--version` before the stand-in's own answer does.
fn matches(pattern: &Pattern, joined: &str) -> bool {
    match pattern {
        Pattern::Exact(expected) => joined == expected,
        Pattern::Prefix(prefix) => joined.starts_with(prefix.as_str()),
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
    }
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
    let slug: String = working_directory()
        .to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    transcripts_root.join(slug).join(format!("{id}.jsonl"))
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

/// Asks on the terminal itself when there is one, whatever stdin is, and
/// waits for an answer nobody gives when no one is watching.
fn ask_on_the_terminal() {
    let (input, output) = uze_platform::stdio::TERMINAL;
    let (Ok(input), Ok(mut output)) = (
        OpenOptions::new().read(true).open(input),
        OpenOptions::new().write(true).open(output),
    ) else {
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
        // The real `agy` stages a plugin under its manifest's `name`, not
        // its directory's: a generated plugin lives in `<name>--<market>/`.
        let id = fs::read(root.join("plugin.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|manifest| manifest["name"].as_str().map(str::to_owned))
            .or_else(|| {
                root.file_name()
                    .map(|base| base.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        let _ = copy_tree(&root, &dest.join(&id));
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
        let file = super::step_file(&scripts, &slot, name);
        if file.is_file() {
            run_step(&file, &log);
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

/// A step, read by this platform's shell, its output appended to `log`.
fn run_step(file: &Path, log: &Path) {
    let output = || OpenOptions::new().create(true).append(true).open(log);
    let (Ok(stdout), Ok(stderr)) = (output(), output()) else {
        return;
    };
    let _ = uze_platform::shell::read_script(file)
        .stdout(stdout)
        .stderr(stderr)
        .status();
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
