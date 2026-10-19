use super::*;
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

/// The deterministic suites' inputs, shared with the Rust rule model.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/_fixtures/rules")
}

/// A checkout of its own: a Git repository with the rules and files given,
/// and a sessions directory beside it, both gone with the test.
struct Checkout {
    scratch: PathBuf,
    root: PathBuf,
    engine: PathBuf,
    sessions: PathBuf,
}

impl Checkout {
    fn new(label: &str) -> Self {
        let scratch = uze_testkit::temp::scratch(label);
        let root = scratch.join("work");
        fs::create_dir_all(&root).unwrap();
        let initialized = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("git runs");
        assert!(initialized.success());
        // The engine resolves the checkout from the physical path Git
        // answers with; the scratch directory may sit behind a link.
        let root = root.canonicalize().unwrap();
        let engine = scratch.join("engine");
        fs::write(&engine, engine::SOURCE).unwrap();
        fs::set_permissions(&engine, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            sessions: scratch.join("sessions"),
            scratch,
            root,
            engine,
        }
    }

    /// The checkout of the scenarios below: one rule of each trigger, the
    /// files they are about, and a file no rule governs.
    fn with_every_trigger(label: &str) -> Self {
        let checkout = Self::new(label);
        checkout.rule(
            "ui.md",
            "trigger: glob\nglobs: **/src/ui/**",
            "Use the design tokens.",
        );
        checkout.rule(
            "testing.md",
            "trigger: model_decision\ndescription: When writing or changing tests\nowner: nobody",
            "Tests sit beside the code.",
        );
        checkout.rule("style.md", "trigger: always_on", "Prefer small functions.");
        checkout.rule("reference.md", "trigger: manual", "Only when asked for.");
        for path in [
            "src/ui/button.ts",
            "src/ui/forms/field.ts",
            "src/core/math.ts",
        ] {
            checkout.file(path);
        }
        checkout
    }

    fn rule(&self, name: &str, frontmatter: &str, body: &str) {
        self.write(
            &format!(".agents/rules/{name}"),
            &format!("---\n{frontmatter}\n---\n{body}\n"),
        );
    }

    fn file(&self, path: &str) {
        self.write(path, "export const value = 1;\n");
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// A recorded payload, its placeholders filled.
    fn recorded(&self, name: &str, session: &str, path: &str) -> String {
        fs::read_to_string(fixtures().join("payloads").join(format!("{name}.json")))
            .unwrap()
            .replace("{root}", &self.root.display().to_string())
            .replace("{session}", session)
            .replace("{path}", path)
    }

    fn session_start(&self, harness: &str, session: &str, source: &str) -> String {
        self.recorded(&format!("{harness}-session-start"), session, "")
            .replace("{source}", source)
    }

    fn ask(&self, payload: &str) -> Answer {
        self.ask_with(payload, &[])
    }

    fn ask_with(&self, payload: &str, environment: &[(&str, &str)]) -> Answer {
        let mut command = Command::new(&self.engine);
        command
            .arg(&self.sessions)
            .current_dir(&self.scratch)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in environment {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("the engine starts");
        // An engine that answers before reading its stdin closes the pipe
        // first; that is an answer, not a failure of the test.
        let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "the engine always exits 0: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "the engine says nothing on stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Answer(String::from_utf8(output.stdout).unwrap())
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.scratch);
    }
}

/// What the engine wrote on stdout.
struct Answer(String);

impl Answer {
    fn is_silent(&self) -> bool {
        self.0.is_empty()
    }

    /// The context the answer hands the harness, checked to be in the shape
    /// every harness reads: `hookSpecificOutput.{hookEventName,
    /// additionalContext}` and nothing else.
    fn context(&self, event: &str) -> String {
        let document: serde_json::Value = serde_json::from_str(self.0.trim())
            .unwrap_or_else(|error| panic!("the answer is JSON ({error}): {}", self.0));
        assert_eq!(document["hookSpecificOutput"]["hookEventName"], event);
        assert_eq!(document.as_object().unwrap().len(), 1, "{document}");
        document["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("the context is a string")
            .to_owned()
    }
}

const UI_RULE: &str = "<rule path=\".agents/rules/ui.md\" trigger=\"glob\"";

#[test]
fn every_recorded_tool_shape_delivers_the_rule_for_the_file_it_touched() {
    let checkout = Checkout::with_every_trigger("rules-engine-shapes");
    for (index, shape) in [
        "claude-read",
        "claude-edit",
        "claude-bash",
        "codex-bash",
        "codex-exec-command",
        "codex-apply-patch",
        "opencode-read",
        "opencode-edit",
        "opencode-write",
    ]
    .into_iter()
    .enumerate()
    {
        let session = format!("session-{index}");
        let context = checkout
            .ask(&checkout.recorded(shape, &session, "src/ui/button.ts"))
            .context("PostToolUse");
        assert_eq!(
            context,
            "<rule path=\".agents/rules/ui.md\" trigger=\"glob\" matched=\"src/ui/button.ts\">\nUse the design tokens.\n</rule>",
            "{shape}"
        );
        let elsewhere = checkout.ask(&checkout.recorded(
            shape,
            &format!("{session}-elsewhere"),
            "src/core/math.ts",
        ));
        assert!(
            elsewhere.is_silent(),
            "{shape} never leaks the rule: {}",
            elsewhere.0
        );
    }
}

#[test]
fn a_session_start_hands_the_always_on_bodies_and_announces_the_on_demand_rules() {
    let checkout = Checkout::with_every_trigger("rules-engine-start");
    for harness in ["claude", "codex"] {
        let context = checkout
            .ask(&checkout.session_start(harness, "s", "startup"))
            .context("SessionStart");
        assert_eq!(
            context,
            "<rule path=\".agents/rules/style.md\" trigger=\"always_on\">\nPrefer small functions.\n</rule>\n\n\
             The project keeps these rules for particular tasks; read a rule's file when its description fits the task:\n\
             <rule path=\".agents/rules/testing.md\" trigger=\"model_decision\">When writing or changing tests</rule>",
            "{harness}"
        );
        for absent in [
            "Tests sit beside the code",
            "design tokens",
            "Only when asked",
        ] {
            assert!(!context.contains(absent), "{harness}: {absent}");
        }
    }
    let context = checkout
        .ask(&checkout.recorded("opencode-context", "ses_1", ""))
        .context("SessionStart");
    assert!(context.contains("Prefer small functions."));
}

#[test]
fn a_glob_rule_arrives_once_per_session_and_again_after_a_compaction() {
    let checkout = Checkout::with_every_trigger("rules-engine-dedup");
    let read =
        |session: &str, path: &str| checkout.ask(&checkout.recorded("claude-read", session, path));
    assert!(
        read("one", "src/ui/button.ts")
            .context("PostToolUse")
            .contains(UI_RULE)
    );
    assert!(
        read("one", "src/ui/forms/field.ts").is_silent(),
        "once per session"
    );
    assert!(
        read("two", "src/ui/forms/field.ts")
            .context("PostToolUse")
            .contains(UI_RULE),
        "another session is another record"
    );

    checkout.ask(&checkout.session_start("claude", "one", "resume"));
    assert!(
        read("one", "src/ui/button.ts").is_silent(),
        "a resume keeps the record"
    );
    for source in ["compact", "clear"] {
        checkout.ask(&checkout.session_start("claude", "one", source));
        assert!(
            read("one", "src/ui/button.ts")
                .context("PostToolUse")
                .contains(UI_RULE),
            "a {source} start forgets what was handed"
        );
    }
    let record = fs::read_to_string(checkout.sessions.join("one")).unwrap();
    assert_eq!(record, ".agents/rules/ui.md\n");
    assert_eq!(
        fs::metadata(&checkout.sessions)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "what a session was handed is this user's alone"
    );
}

#[test]
fn paths_are_read_relative_to_the_session_and_never_outside_the_checkout() {
    let checkout = Checkout::with_every_trigger("rules-engine-paths");
    let root = checkout.root.display().to_string();
    let read = |session: &str, cwd: &str, path: &str| {
        let payload = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": session,
            "cwd": cwd,
            "tool_name": "Read",
            "tool_input": {"file_path": path},
        });
        checkout.ask(&payload.to_string())
    };
    let matched = |answer: Answer| answer.context("PostToolUse").contains(UI_RULE);
    assert!(matched(read(
        "abs",
        &root,
        &format!("{root}/src/ui/button.ts")
    )));
    assert!(matched(read("rel", &root, "src/ui/button.ts")));
    assert!(matched(read("dot", &root, "./src/core/../ui/button.ts")));
    assert!(matched(read("sub", &format!("{root}/src"), "ui/button.ts")));
    assert!(matched(read(
        "subabs",
        &format!("{root}/src/core"),
        &format!("{root}/src/ui/button.ts")
    )));

    let outside = checkout.scratch.join("elsewhere/src/ui");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("button.ts"), "").unwrap();
    assert!(
        read(
            "out",
            &root,
            &outside.join("button.ts").display().to_string()
        )
        .is_silent()
    );
    assert!(read("up", &root, "../elsewhere/src/ui/button.ts").is_silent());
    assert!(
        read("gone", &root, "src/ui/missing.ts").is_silent(),
        "a field naming no file is no path"
    );
}

#[test]
fn a_shell_command_is_read_word_by_word() {
    let checkout = Checkout::with_every_trigger("rules-engine-shell");
    let run = |session: &str, command: &str| {
        let payload = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": session,
            "cwd": checkout.root,
            "tool_name": "Bash",
            "tool_input": {"command": command, "description": "Show the file"},
        });
        checkout.ask(&payload.to_string())
    };
    for (session, command) in [
        ("quoted", "grep -n 'value' \"src/ui/button.ts\""),
        (
            "piped",
            "cat src/core/math.ts|head;wc -l <src/ui/forms/field.ts",
        ),
        ("flag", "prettier --write=src/ui/button.ts"),
        ("lines", "echo start\nless src/ui/button.ts"),
    ] {
        assert!(
            run(session, command)
                .context("PostToolUse")
                .contains(UI_RULE),
            "{command}"
        );
    }
    assert!(run("none", "ls src/core && echo src/ui/nothing").is_silent());
}

#[test]
fn a_patch_names_its_files_whether_or_not_they_still_exist() {
    let checkout = Checkout::with_every_trigger("rules-engine-patch");
    for (session, header) in [
        ("add", "*** Add File: src/ui/new.ts"),
        ("delete", "*** Delete File: src/ui/old.ts"),
        (
            "move",
            "*** Update File: src/core/math.ts\n*** Move to: src/ui/math.ts",
        ),
    ] {
        let payload = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": session,
            "cwd": checkout.root,
            "tool_name": "apply_patch",
            "tool_input": format!("*** Begin Patch\n{header}\n+x\n*** End Patch\n"),
        });
        assert!(
            checkout
                .ask(&payload.to_string())
                .context("PostToolUse")
                .contains(UI_RULE),
            "{header}"
        );
    }
    let body_only = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "body",
        "cwd": checkout.root,
        "tool_name": "apply_patch",
        "tool_input": {"command": "*** Begin Patch\n*** Update File: src/core/math.ts\n+import x from \"src/ui/button.ts\"\n*** End Patch\n"},
    });
    assert!(
        checkout.ask(&body_only.to_string()).is_silent(),
        "only a patch's headers name its files"
    );
}

#[test]
fn a_file_that_is_not_a_rule_is_skipped_and_the_rules_beside_it_still_answer() {
    let checkout = Checkout::with_every_trigger("rules-engine-invalid");
    checkout.write(".agents/rules/notes.md", "Just notes, no frontmatter.\n");
    checkout.rule("unknown.md", "trigger: sometimes", "UNKNOWN BODY");
    checkout.rule("bare.md", "trigger: glob", "BARE BODY");
    checkout.rule(
        "wide.md",
        "trigger: glob\nglobs: **/src/**/*.{ts,tsx}",
        "WIDE BODY",
    );
    checkout.rule(
        "mixed.md",
        "trigger: glob\nglobs: **/src/ui/**, **/src/{a,b}.ts",
        "MIXED BODY",
    );
    checkout.rule("silent.md", "trigger: model_decision", "SILENT BODY");
    checkout.write(
        ".agents/rules/open.md",
        "---\ntrigger: always_on\nOPEN BODY\n",
    );
    checkout.write(
        ".agents/rules/readme.txt",
        "---\ntrigger: always_on\n---\nTXT BODY\n",
    );
    checkout.rule(
        "twice.md",
        "trigger: always_on\ntrigger: always_on",
        "TWICE BODY",
    );
    checkout.rule(
        "block.md",
        "trigger: glob\nglobs: |\n  **/src/ui/**",
        "BLOCK BODY",
    );
    checkout.rule(
        "list.md",
        "trigger: glob\nglobs:\n  - **/src/ui/**",
        "LIST BODY",
    );
    checkout.rule(
        "folded.md",
        "trigger: model_decision\ndescription: When the\n  task is long",
        "FOLDED BODY",
    );
    let start = checkout
        .ask(&checkout.session_start("claude", "s", "startup"))
        .context("SessionStart");
    let tool = checkout
        .ask(&checkout.recorded("claude-read", "s", "src/ui/button.ts"))
        .context("PostToolUse");
    assert!(start.contains("Prefer small functions."));
    assert!(tool.contains(UI_RULE));
    for body in [
        "Just notes",
        "UNKNOWN BODY",
        "BARE BODY",
        "WIDE BODY",
        "MIXED BODY",
        "SILENT BODY",
        "OPEN BODY",
        "TXT BODY",
        "TWICE BODY",
        "BLOCK BODY",
        "LIST BODY",
        "FOLDED BODY",
        "When the",
    ] {
        assert!(!start.contains(body) && !tool.contains(body), "{body}");
    }
}

#[test]
fn quoting_crlf_and_a_byte_order_mark_are_read_as_an_editor_writes_them() {
    let checkout = Checkout::new("rules-engine-format");
    checkout.file("src/ui/button.ts");
    checkout.write(
        ".agents/rules/ui.md",
        "\u{feff}---\r\ntrigger: \"glob\" # the trigger\r\nglobs: '**/src/ui/**, **/docs/**'\r\n---\r\n\r\nBody line.\r\n\r\n",
    );
    let context = checkout
        .ask(&checkout.recorded("claude-read", "s", "src/ui/button.ts"))
        .context("PostToolUse");
    assert_eq!(
        context,
        "<rule path=\".agents/rules/ui.md\" trigger=\"glob\" matched=\"src/ui/button.ts\">\nBody line.\n</rule>"
    );
}

#[test]
fn a_checkout_without_rules_or_outside_git_pays_nothing() {
    let checkout = Checkout::new("rules-engine-none");
    checkout.file("src/ui/button.ts");
    assert!(
        checkout
            .ask(&checkout.session_start("claude", "s", "startup"))
            .is_silent()
    );
    assert!(
        checkout
            .ask(&checkout.recorded("claude-read", "s", "src/ui/button.ts"))
            .is_silent()
    );
    assert!(
        !checkout.sessions.exists(),
        "nothing is recorded where nothing was handed"
    );

    let outside = uze_testkit::temp::scratch("rules-engine-no-git");
    fs::create_dir_all(outside.join(".agents/rules")).unwrap();
    fs::write(
        outside.join(".agents/rules/style.md"),
        "---\ntrigger: always_on\n---\nOUTSIDE\n",
    )
    .unwrap();
    let payload = serde_json::json!({
        "hook_event_name": "SessionStart", "source": "startup", "session_id": "s", "cwd": outside,
    });
    assert!(checkout.ask(&payload.to_string()).is_silent());
    let _ = fs::remove_dir_all(outside);
}

#[test]
fn each_checkout_is_answered_with_its_own_rules() {
    let first = Checkout::new("rules-engine-own-a");
    let second = Checkout::new("rules-engine-own-b");
    first.rule("style.md", "trigger: always_on", "FIRST");
    second.rule("style.md", "trigger: always_on", "SECOND");
    let start = |checkout: &Checkout| {
        checkout
            .ask(&checkout.session_start("codex", "s", "startup"))
            .context("SessionStart")
    };
    assert!(start(&first).contains("FIRST") && !start(&first).contains("SECOND"));
    assert!(start(&second).contains("SECOND") && !start(&second).contains("FIRST"));
}

#[test]
fn every_failure_lets_the_call_proceed_with_nothing_added() {
    let checkout = Checkout::with_every_trigger("rules-engine-fail-open");
    let read = checkout.recorded("claude-read", "s", "src/ui/button.ts");
    assert!(
        checkout
            .ask_with(&read, &[("HOOK_JQ", "/nonexistent/jq")])
            .is_silent(),
        "no jq"
    );
    for payload in [
        "",
        "not json",
        "[1, 2]",
        r#"{"hook_event_name": "PreToolUse", "cwd": "/"}"#,
        r#"{"hook_event_name": "PostToolUse", "cwd": "/nonexistent/place"}"#,
        r#"{"hook_event_name": "PostToolUse", "cwd": {"not": "a path"}}"#,
    ] {
        assert!(checkout.ask(payload).is_silent(), "{payload}");
    }
    let hostile = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "../../escape",
        "cwd": checkout.root,
        "tool_name": "Read",
        "tool_input": {"file_path": "src/ui/button.ts"},
    });
    assert!(
        checkout
            .ask(&hostile.to_string())
            .context("PostToolUse")
            .contains(UI_RULE)
    );
    assert!(
        !checkout.scratch.join("escape").exists() && !checkout.sessions.exists(),
        "a session id that is not a plain name records nothing"
    );
}

/// One row of the shared dialect table.
struct GlobCase {
    pattern: String,
    path: String,
    expected: String,
}

/// What the Rust model refuses as a whole file, the engine refuses too: a
/// rule `uze status` reports as not delivered never reaches a session.
#[test]
fn a_file_the_rust_model_refuses_is_never_delivered() {
    let checkout = Checkout::new("rules-engine-refused-files");
    checkout.rule(
        "empty-pattern.md",
        "trigger: glob\nglobs: **/x.rs,,**/src/c.rs",
        "EMPTY-PATTERN",
    );
    checkout.rule(
        "no-colon.md",
        "trigger: glob\nglobs: **/src/c.rs\nnot a key value",
        "NO-COLON",
    );
    checkout.rule(
        "commented.md",
        "# a comment the model skips\ntrigger: glob\nglobs: **/src/c.rs",
        "COMMENTED",
    );
    checkout.rule(
        "control.md",
        "trigger: glob\nglobs: **/src/c\u{b}.rs, **/src/c.rs",
        "CONTROL",
    );
    checkout.rule(
        "dash-key.md",
        "trigger: glob\nglobs: **/src/c.rs\n-x: an unknown key",
        "DASH-KEY",
    );
    checkout.file("src/c.rs");
    let answer = checkout.ask(
        &serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": "s",
            "cwd": checkout.root,
            "tool_name": "Read",
            "tool_input": {"file_path": "src/c.rs"},
        })
        .to_string(),
    );
    let text = answer.context("PostToolUse");
    assert!(
        !text.contains("CONTROL"),
        "a control character is refused: {text}"
    );
    assert!(
        text.contains("DASH-KEY"),
        "`-x:` is an unknown key, not a continuation: {text}"
    );
    for refused in ["EMPTY-PATTERN", "NO-COLON"] {
        assert!(!text.contains(refused), "{refused} delivered: {text}");
        let rule = format!(
            "---\n{}\n---\nbody\n",
            match refused {
                "EMPTY-PATTERN" => "trigger: glob\nglobs: **/x.rs,,**/src/c.rs",
                _ => "trigger: glob\nglobs: **/src/c.rs\nnot a key value",
            }
        );
        assert!(
            uze_core::rules::parse_rule(".agents/rules/r.md", rule.as_bytes()).is_err(),
            "the Rust model refuses {refused} too"
        );
    }
    assert!(
        text.contains("COMMENTED"),
        "a comment line is no fault: {text}"
    );
}

fn glob_table() -> Vec<GlobCase> {
    fs::read_to_string(fixtures().join("glob-dialect.tsv"))
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 3, "pattern, path, expected: {line:?}");
            assert!(
                ["match", "no-match", "invalid"].contains(&fields[2]),
                "{line:?}"
            );
            GlobCase {
                pattern: fields[0].to_owned(),
                path: fields[1].to_owned(),
                expected: fields[2].to_owned(),
            }
        })
        .collect()
}

/// The engine answers every row of the table, with whichever `awk` the
/// directory `awk_dir` puts first (`None`: the system's own).
fn engine_answers_the_glob_table(awk_dir: Option<&Path>) {
    let table = glob_table();
    assert!(table.len() > 20, "the table is read: {}", table.len());
    let path_variable = awk_dir.map(|dir| {
        format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        )
    });
    let environment: Vec<(&str, &str)> = path_variable
        .as_deref()
        .map(|path| vec![("PATH", path)])
        .unwrap_or_default();
    let mut wrong = Vec::new();
    for (index, case) in table.iter().enumerate() {
        let checkout = Checkout::new(&format!("rules-engine-glob-{index}"));
        checkout.rule(
            "case.md",
            &format!("trigger: glob\nglobs: {}", case.pattern),
            "CASE",
        );
        checkout.file(&case.path);
        let payload = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": "s",
            "cwd": checkout.root,
            "tool_name": "Read",
            "tool_input": {"file_path": case.path},
        });
        let answer = checkout.ask_with(&payload.to_string(), &environment);
        let delivered = !answer.is_silent();
        if delivered != (case.expected == "match") {
            wrong.push(format!(
                "{} ~ {}: expected {}, delivered {delivered}",
                case.pattern, case.path, case.expected
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the engine disagrees with the table:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_engine_answers_the_shared_glob_table() {
    engine_answers_the_glob_table(None);
}

/// Debian and Ubuntu ship `mawk` as `awk`, macOS ships the one true awk:
/// the dialect must not depend on gawk's extensions.
#[test]
fn the_engine_answers_the_shared_glob_table_under_every_awk_here() {
    for awk in ["mawk", "nawk", "original-awk", "busybox"] {
        let Ok(found) = Command::new("sh")
            .args(["-c", &format!("command -v {awk}")])
            .output()
        else {
            continue;
        };
        let found = String::from_utf8_lossy(&found.stdout).trim().to_owned();
        if found.is_empty() {
            continue;
        }
        let shim = uze_testkit::temp::scratch(&format!("rules-engine-{awk}"));
        fs::create_dir_all(&shim).unwrap();
        let program = if awk == "busybox" {
            format!("#!/bin/sh\nexec {found} awk \"$@\"\n")
        } else {
            format!("#!/bin/sh\nexec {found} \"$@\"\n")
        };
        fs::write(shim.join("awk"), program).unwrap();
        fs::set_permissions(shim.join("awk"), fs::Permissions::from_mode(0o755)).unwrap();
        engine_answers_the_glob_table(Some(&shim));
        let _ = fs::remove_dir_all(shim);
    }
}

/// Whether Bun is here to load the generated plugin. CI installs it, so
/// there its absence is a broken job rather than a reason to skip.
fn bun_available() -> bool {
    let available = Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert!(
            std::env::var_os("CI").is_none(),
            "bun is not installed, and CI must run the OpenCode rules plugin check"
        );
        eprintln!("bun is not installed; the OpenCode rules plugin check is skipped");
    }
    available
}

/// The plugin, loaded by real Bun with a V2-shaped plugin context, asks
/// the real engine and hands its answer to the model: onto the request's
/// system parts before a request, onto the tool result after a call. The
/// context is the one OpenCode 2.0.32 hands a plugin
/// (`experiments/opencode/rules-directory`): the directory is the
/// instance's `location`, and `worktree` is an API, not a path.
#[test]
fn the_opencode_plugin_hands_the_engines_answer_to_the_model() {
    if !bun_available() {
        return;
    }
    let checkout = Checkout::with_every_trigger("rules-opencode-plugin");
    fs::write(
        checkout.scratch.join("rules.ts"),
        plugin::source(&checkout.engine, &checkout.sessions),
    )
    .unwrap();
    let root = serde_json::to_string(&checkout.root).unwrap();
    fs::write(
        checkout.scratch.join("drive.ts"),
        format!(
            r#"import plugin from "./rules.ts";
const hooks = {{}};
await plugin.setup({{
  location: {{ directory: {root}, workspaceID: null, project: {{ id: "global", directory: {root} }} }},
  worktree: {{ list: async () => [], create: async () => null }},
  session: {{ hook: async (name, fn) => {{ hooks[`session.${{name}}`] = fn; }} }},
  tool: {{ hook: async (name, fn) => {{ hooks[`tool.${{name}}`] = fn; }} }},
}});
const context = {{ sessionID: "ses_1", system: [{{ type: "text", text: "base" }}] }};
await hooks["session.context"](context);
const read = {{
  sessionID: "ses_1", tool: "read", input: {{ path: "src/ui/button.ts" }},
  result: {{ status: "completed", content: [{{ type: "text", text: "file" }}] }},
}};
await hooks["tool.execute.after"](read);
const again = {{
  sessionID: "ses_1", tool: "read", input: {{ path: "src/ui/forms/field.ts" }},
  result: {{ status: "completed", content: [] }},
}};
await hooks["tool.execute.after"](again);
const failed = {{
  sessionID: "ses_2", tool: "read", input: {{ path: "src/ui/button.ts" }},
  result: {{ status: "error", content: [] }},
}};
await hooks["tool.execute.after"](failed);
await hooks["tool.execute.after"]({{ sessionID: "ses_3", tool: "read", input: {{}} }});
console.log(JSON.stringify({{ system: context.system, read: read.result.content, again: again.result.content, failed: failed.result.content }}));
"#
        ),
    )
    .unwrap();
    let output = Command::new("bun")
        .args(["run", "drive.ts"])
        .current_dir(&checkout.scratch)
        .output()
        .expect("bun runs the driver");
    assert!(
        output.status.success(),
        "the plugin loads and runs: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let seen: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    let system = seen["system"].as_array().unwrap();
    assert_eq!(system.len(), 2, "{seen}");
    assert_eq!(
        system[1]["type"], "text",
        "pushed in the shape the first part has"
    );
    let start = system[1]["text"].as_str().unwrap();
    assert!(start.contains("Prefer small functions.") && start.contains("testing.md"));
    assert!(!start.contains("design tokens"));
    let read = seen["read"].as_array().unwrap();
    assert_eq!(read.len(), 2, "{seen}");
    assert_eq!(read[1]["type"], "text");
    assert!(read[1]["text"].as_str().unwrap().contains(UI_RULE));
    assert_eq!(seen["again"], serde_json::json!([]), "once per session");
    assert_eq!(
        seen["failed"],
        serde_json::json!([]),
        "only a completed call"
    );
}

/// The paths of the rules an answer hands over, in the order it hands them.
fn handed(answer: &Answer, event: &str) -> Vec<String> {
    if answer.is_silent() {
        return Vec::new();
    }
    answer
        .context(event)
        .split("<rule path=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap().to_owned())
        .collect()
}

/// `.agents/rules.json` read by the engine exactly as `uze-core` reads it:
/// every case of the shared table, the same rules in the same order.
#[test]
fn the_engine_reads_the_manifest_as_the_rust_model_does() {
    let cases = uze_testkit::fixtures::rules_manifest_cases();
    assert!(cases.len() >= 10, "the table is read: {}", cases.len());
    for case in cases {
        let checkout = Checkout::new(&format!("rules-engine-manifest-{}", case.name));
        case.stage(&checkout.root);
        let answer = checkout.ask(&checkout.session_start("claude", "s", "startup"));
        assert_eq!(
            handed(&answer, "SessionStart"),
            case.expected,
            "{}: {}",
            case.name,
            answer.0
        );
        let model: Vec<String> = uze_core::rules::discover(&checkout.root)
            .unwrap()
            .rules
            .into_iter()
            .map(|rule| rule.path)
            .collect();
        assert_eq!(model, case.expected, "{}: the Rust model agrees", case.name);
    }
}

#[test]
fn a_rule_in_a_registered_folder_answers_a_tool_call_like_a_flat_one() {
    let checkout = Checkout::new("rules-engine-manifest-glob");
    checkout.write(
        ".agents/rules/acme@corp/ui.md",
        "---\ntrigger: glob\nglobs: **/src/ui/**\n---\nUse the design tokens.\n",
    );
    checkout.write(
        ".agents/rules.json",
        r#"{"entries": [{"path": ".agents/rules/acme@corp"}]}"#,
    );
    checkout.file("src/ui/button.ts");
    let context = checkout
        .ask(&checkout.recorded("claude-read", "s", "src/ui/button.ts"))
        .context("PostToolUse");
    assert_eq!(
        context,
        "<rule path=\".agents/rules/acme@corp/ui.md\" trigger=\"glob\" matched=\"src/ui/button.ts\">\nUse the design tokens.\n</rule>"
    );
}
