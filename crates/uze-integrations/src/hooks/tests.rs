use super::*;
use uze_core::hook::{CommandHandlerType, HookMatcher};

fn hook() -> PortableHook {
    PortableHook {
        id: "protect-env".into(),
        event: HookEvent::PreToolUse,
        matchers: vec![
            HookMatcher::Portable("shell".into()),
            HookMatcher::Native("Write".into()),
        ],
        handlers: vec![CommandHook {
            handler_type: CommandHandlerType::Command,
            command: uze_core::shell::ShellCommand::spelled(
                "${PLUGIN_ROOT}/check",
                "& \"${PLUGIN_ROOT}/check.ps1\"",
            ),
            timeout: 10,
        }],
        effect: HookEffect::Deny,
        order: 0,
    }
}

/// A group's native invocation through the generated wrapper, which is
/// what every entry below carries.
fn invocation(hook: &PortableHook) -> HookInvocation {
    HookInvocation::Line(wrapper_command_line(
        Path::new("/state/hooks/exec"),
        hook,
        Path::new("/pkg"),
    ))
}

#[test]
fn vendor_aliases_are_explicit() {
    assert_eq!(
        tool_names(crate::claude::HOOKS, &HookMatcher::Portable("shell".into())),
        ["Bash", "PowerShell"]
    );
    assert_eq!(
        tool_names(
            crate::antigravity::HOOKS,
            &HookMatcher::Portable("shell".into())
        ),
        ["run_command"]
    );
    assert_eq!(
        tool_names(crate::opencode::HOOKS, &HookMatcher::Native("Write".into())),
        ["Write"]
    );
    assert_eq!(
        vocabulary(crate::claude::HOOKS)
            .binding_for_native("Bash")
            .map(|binding| binding.alias),
        Some("shell"),
        "the reverse table must round-trip the forward one"
    );
}

const TARGETS: [HookTarget; 4] = [
    crate::claude::HOOKS,
    crate::codex::HOOKS,
    crate::antigravity::HOOKS,
    crate::opencode::HOOKS,
];

#[test]
fn every_alias_is_bound_on_every_harness_and_carries_its_portable_fields() {
    for target in TARGETS {
        let table = vocabulary(target);
        for alias in uze_core::hook::portable_tool_aliases() {
            let binding = table
                .binding(alias)
                .unwrap_or_else(|| panic!("{target} has no row for alias `{alias}`"));
            let promised = uze_core::hook::alias_fields(alias);
            let bound: Vec<&str> = binding.fields.iter().map(|(name, _)| *name).collect();
            assert_eq!(
                bound, promised,
                "{target}/{alias} must read exactly the fields the vocabulary promises"
            );
            if let Some(native) = binding.native_tool {
                assert_eq!(
                    table.binding_for_native(native).map(|entry| entry.alias),
                    Some(binding.alias),
                    "{target}/{alias} must round-trip through its native name"
                );
            }
        }
    }
}

#[test]
fn a_native_matcher_yields_no_portable_fields() {
    let table = vocabulary(crate::claude::HOOKS);
    assert!(table.binding_for_native("SomeVendorOnlyTool").is_none());
    assert_eq!(
        tool_names(crate::claude::HOOKS, &HookMatcher::Native("Write".into())),
        ["Write"]
    );
}

#[test]
fn the_shell_alias_reads_each_harnesss_own_command_field() {
    let field = |target: HookTarget| {
        vocabulary(target)
            .binding("shell")
            .and_then(|binding| binding.fields.first())
            .map(|(_, native)| *native)
    };
    assert_eq!(field(crate::claude::HOOKS), Some("command"));
    assert_eq!(field(crate::codex::HOOKS), Some("cmd"));
    assert_eq!(field(crate::antigravity::HOOKS), Some("CommandLine"));
    assert_eq!(field(crate::opencode::HOOKS), Some("command"));
}

#[test]
fn a_renamed_vendor_tool_still_normalizes_to_its_alias() {
    let alias = |native| {
        vocabulary(crate::codex::HOOKS)
            .binding_for_native(native)
            .map(|binding| binding.alias)
    };
    assert_eq!(alias("exec_command"), Some("shell"));
    assert_eq!(alias("Bash"), Some("shell"));
    assert_eq!(
        tool_names(crate::codex::HOOKS, &HookMatcher::Portable("shell".into())),
        ["exec_command", "Bash"],
        "the matcher intercepts every name this harness's shell tool answers to"
    );
}

/// A platform the `sh` template does not cover gets no hook, and the
/// plan says so. The wrapper is the only implementation of the
/// contract, so a delivery that cannot write one has nothing honest to
/// attach — an entry running something else would be a hook the author
/// never wrote.
fn hook_resource(package: &Path) -> Resource {
    Resource::from_package(
        uze_core::store::PackageId::from_plugin_name("demo", &package.join("plugin.json")).unwrap(),
        package.to_path_buf(),
        uze_core::capability::Capability {
            kind: uze_core::capability::CapabilityKind::Hook,
            path: package.join(HOOKS_FILE_NAME),
            payload: serde_json::to_vec(&hook()).unwrap(),
        },
    )
}

/// A platform the `sh` template does not cover gets no hook, and the
/// plan says so. The wrapper is the only implementation of the
/// contract, so a delivery that cannot write one has nothing honest to
/// attach — an entry running something else would be a hook the author
/// never wrote.
#[test]
fn a_platform_without_a_wrapper_template_delivers_no_hook() {
    let home = UzeHome::at(Path::new("/tmp/uze-home"));
    let resource = hook_resource(Path::new("/pkg"));
    let plan = crate::claude::HOOKS.entry_plan(
        &home,
        &resource,
        PathBuf::from("/config/settings.json"),
        "evidence.",
    );
    let ExposureMechanism::Managed(ManagedArtifact::HookConfigEntry {
        expected, wrapper, ..
    }) = plan.mechanism
    else {
        panic!("a harness with a template delivers: {:?}", plan.mechanism);
    };
    assert_eq!(wrapper, crate::claude::HOOKS.wrapper_path(&home));
    let entry: serde_json::Value = serde_json::from_str(&expected).unwrap();
    let (program, _) = uze_platform::shell::script(&wrapper.display().to_string());
    assert_eq!(entry["hooks"][0]["command"], program);

    assert!(
        wrapper_source(crate::opencode::HOOKS).is_none(),
        "a harness the template generator does not cover has no wrapper"
    );
}

/// The same answer one level up: the exposure plan reports Unsupported
/// with the reason, rather than an entry pointing at something that is
/// not there.
#[test]
fn a_hook_that_cannot_be_delivered_is_reported_unsupported() {
    let package = uze_testkit::temp::scratch("undeliverable");
    fs::create_dir_all(&package).unwrap();
    let resource = hook_resource(&package);
    let plan = hook_plan(
        &resource,
        &crate::claude::HOOKS.capabilities(),
        false,
        "evidence.",
        |_| None,
    );
    assert_eq!(plan.route, CompatibilityRoute::Unsupported);
    assert!(
        matches!(
            &plan.mechanism,
            ExposureMechanism::Unsupported { rationale } if rationale == NO_WRAPPER_TEMPLATE
        ),
        "nothing is attached: {:?}",
        plan.mechanism
    );
    assert!(plan.evidence.contains(NO_WRAPPER_TEMPLATE));
    let _ = fs::remove_dir_all(package);
}

#[test]
fn group_entry_omits_matcher_for_unmatch_all_and_reserves_native_timeout() {
    let mut hook = hook();
    let entry = group_entry(crate::claude::HOOKS, &hook, &invocation(&hook));
    assert_eq!(entry["matcher"], "Bash|PowerShell|Write");
    assert_eq!(entry["hooks"][0]["type"], "command");
    assert_eq!(
        entry["hooks"][0]["timeout"], 12,
        "each handler's own bound plus its kill grace, plus 1s to render"
    );
    hook.matchers = Vec::new();
    let entry = group_entry(crate::claude::HOOKS, &hook, &invocation(&hook));
    assert!(
        entry.get("matcher").is_none(),
        "no matcher key for a match-all group"
    );
}

/// The harness's own timeout is a backstop, and a backstop that fires
/// first defeats the purpose: a hook the harness kills is read as
/// non-blocking, so a `deny` group would be allowed through. The bound
/// therefore has to cover everything the wrapper can spend — and
/// `parse_manifest` is what keeps a manifest from asking for more than
/// the maximum, since clamping here would silently reintroduce the
/// problem.
#[test]
fn the_native_timeout_outlasts_everything_the_wrapper_can_spend() {
    let manifest = serde_json::json!({
        "hooks": {"PreToolUse": [{
            "id": "protect-env",
            "hooks": (1..=9).map(|_| serde_json::json!({
                "type": "command", "command": "check", "timeout": 30
            })).collect::<Vec<_>>(),
        }]}
    })
    .to_string();
    let hooks =
        uze_core::hook::parse_manifest(Path::new("hooks.json"), manifest.as_bytes()).unwrap();
    let entry = group_entry(crate::claude::HOOKS, &hooks[0], &invocation(&hooks[0]));
    let native = entry["hooks"][0]["timeout"].as_u64().unwrap();
    let spent: u64 = hooks[0]
        .handlers
        .iter()
        .map(|handler| u64::from(handler.timeout) + 1)
        .sum();
    assert!(
        native > spent,
        "the native backstop ({native}s) must outlast the wrapper's own worst case ({spent}s)"
    );
    assert!(u64::from(uze_core::hook::MAX_TIMEOUT_SECONDS) >= native);
}

/// The vendor's own docs split the two shapes: a tool event is grouped
/// with a matcher, while `Stop` is "flat (list of handler objects
/// directly)". A grouped `Stop` is parsed as invalid and silently
/// dropped — `plugin validate` says nothing and only `--log-file`
/// reports it (antigravity-cli#925, 1.1.24).
#[test]
fn a_stop_entry_is_flat_while_a_tool_event_stays_grouped() {
    let stop = PortableHook {
        id: "archive".into(),
        event: HookEvent::Stop,
        matchers: Vec::new(),
        effect: HookEffect::Observe,
        ..hook()
    };
    let value = serde_json::json!({
        "protect-env": agy_named_entry(crate::antigravity::HOOKS, &hook(),
            Path::new("/state/hooks/exec"),
            Path::new("/pkg"),
        ),
        "archive": agy_named_entry(crate::antigravity::HOOKS, &stop,
            Path::new("/state/hooks/exec"),
            Path::new("/pkg"),
        ),
    });

    let grouped = &value["protect-env"]["PreToolUse"][0];
    assert_eq!(grouped["matcher"], "run_command|Write");
    assert_eq!(grouped["hooks"][0]["type"], "command");

    let flat = &value["archive"]["Stop"][0];
    assert_eq!(
        flat["type"], "command",
        "a flat entry is the handler object itself: {flat}"
    );
    assert!(
        flat.get("hooks").is_none(),
        "a `hooks` group under Stop is dropped by the vendor's parser"
    );
    assert!(flat.get("matcher").is_none(), "Stop matches no tool");
    assert!(
        flat["command"]
            .as_str()
            .is_some_and(|command| command.contains("'stop' 'observe'")),
        "the flat entry still runs the wrapper with the group's arguments: {flat}"
    );
    assert_eq!(flat["timeout"], 12);
}

#[test]
fn agy_named_entry_carries_the_wrapper_and_is_deterministic() {
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        Path::new("/state/hooks/exec"),
        Path::new("/pkg"),
    );
    let document = serde_json::to_string(&entry).unwrap();
    assert_eq!(entry["PreToolUse"][0]["matcher"], "run_command|Write");
    assert!(
        entry.get("hooks").is_none(),
        "the named key holds the event map directly; a `hooks` wrapper is one dead hook"
    );
    assert!(
        document.contains("'/state/hooks/exec' '/pkg' 'pre_tool_use' 'deny'"),
        "the entry runs the shared wrapper with the group's own arguments: {document}"
    );
    assert!(
        !document.contains("hook-exec"),
        "nothing on the execution path may be the packager"
    );
    assert_eq!(
        entry,
        agy_named_entry(
            crate::antigravity::HOOKS,
            &hook(),
            Path::new("/state/hooks/exec"),
            Path::new("/pkg")
        ),
    );
}

/// Antigravity's shared `hooks.json` is a map of *named* hooks, so UZE
/// owns keys, not array members: a hand-written hook beside ours — and
/// any unrelated key — must survive attach, inspect and detach untouched.
#[test]
fn a_named_merge_leaves_every_foreign_hook_intact() {
    let root = uze_testkit::temp::scratch("hooks-named-merge");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    fs::write(
            &config,
            r#"{"my-own-guard":{"PreToolUse":[{"matcher":"run_command","hooks":[{"type":"command","command":"mine"}]}]},"notes":"kept"}"#,
        )
        .unwrap();
    let name = "pkg@market:protect-env";
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        Path::new("/state/hooks/exec"),
        Path::new("/pkg"),
    );
    let expected = serde_json::to_string(&entry).unwrap();

    merge_named_entry(&config, name, &entry).unwrap();
    assert_eq!(
        inspect_named_entry(&config, name, &expected).state,
        AttachmentState::Matched
    );
    // Merging the same entry again changes nothing (idempotence).
    merge_named_entry(&config, name, &entry).unwrap();

    let after: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(
        after["my-own-guard"]["PreToolUse"][0]["hooks"][0]["command"],
        "mine"
    );
    assert_eq!(after["notes"], "kept");
    assert_eq!(after[name], entry);

    let detached = remove_named_entry(&config, name, &expected).unwrap();
    assert_eq!(detached.state, AttachmentState::Missing);
    let survivors: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert!(survivors.get(name).is_none(), "UZE's own key is gone");
    assert_eq!(
        survivors["my-own-guard"]["PreToolUse"][0]["hooks"][0]["command"], "mine",
        "the foreign named hook is byte-identical: {survivors}"
    );
    assert_eq!(survivors["notes"], "kept");
}

/// Drift blocks removal: an edited entry is never silently rewritten,
/// and a file UZE cannot parse is never mutated at all.
#[test]
fn a_drifted_or_unreadable_named_entry_is_never_removed() {
    let root = uze_testkit::temp::scratch("hooks-named-drift");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let name = "pkg@market:protect-env";
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        Path::new("/state/hooks/exec"),
        Path::new("/pkg"),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    merge_named_entry(&config, name, &entry).unwrap();

    let mut edited: serde_json::Value =
        serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    edited[name]["PreToolUse"][0]["matcher"] = serde_json::json!("something-else");
    fs::write(&config, serde_json::to_vec_pretty(&edited).unwrap()).unwrap();
    assert_eq!(
        inspect_named_entry(&config, name, &expected).state,
        AttachmentState::Drifted
    );
    assert_eq!(
        remove_named_entry(&config, name, &expected).unwrap().state,
        AttachmentState::Drifted,
        "a drifted entry is reported, never removed"
    );

    fs::write(&config, "{not json").unwrap();
    assert_eq!(
        inspect_named_entry(&config, name, &expected).state,
        AttachmentState::Blocked
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), "{not json");
}

/// A file that held nothing but UZE's own entry was created by UZE and
/// goes away with it; one holding anything else stays.
#[test]
fn a_named_config_that_uze_created_is_removed_with_its_last_entry() {
    let root = uze_testkit::temp::scratch("hooks-named-empty");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let name = "pkg@market:protect-env";
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        Path::new("/state/hooks/exec"),
        Path::new("/pkg"),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    merge_named_entry(&config, name, &entry).unwrap();
    remove_named_entry(&config, name, &expected).unwrap();
    assert!(!config.exists(), "UZE removes the file it alone created");
}

#[test]
fn merge_inspect_detach_preserve_foreign_entries_and_order() {
    let root = uze_testkit::temp::scratch("hooks-merge");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("settings.json");
    fs::write(
            &config,
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"foreign"}]}]},"theme":"dark"}"#,
        )
        .unwrap();
    let entry = group_entry(crate::claude::HOOKS, &hook(), &invocation(&hook()));
    let expected = serde_json::to_string(&entry).unwrap();
    let path = merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    assert_eq!(path, config);
    assert_eq!(
        inspect_event_entry(&config, HookEvent::PreToolUse, &expected).state,
        AttachmentState::Matched
    );
    // Idempotence: a second merge changes nothing.
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    assert_eq!(
        inspect_event_entry(&config, HookEvent::PreToolUse, &expected).state,
        AttachmentState::Matched
    );
    let after: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(after["theme"], "dark");
    let groups = after["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(
        groups.len(),
        2,
        "the foreign group stays and UZE's is appended"
    );
    assert_eq!(
        inspect_event_entry(&config, HookEvent::PostToolUse, &expected).state,
        AttachmentState::Missing,
        "an entry in the wrong event array is not matched"
    );
    assert_eq!(
        remove_event_entry(&config, HookEvent::PreToolUse, &expected)
            .unwrap()
            .state,
        AttachmentState::Missing
    );
    let after: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(
        after["hooks"]["PreToolUse"].as_array().unwrap().len(),
        1,
        "only UZE's entry went"
    );
    assert_eq!(
        after["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "foreign"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn merging_replaces_the_previous_version_of_the_same_group() {
    let root = uze_testkit::temp::scratch("hooks-replace");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let mut old = hook();
    old.handlers[0].timeout = 10;
    let old_entry = group_entry(crate::codex::HOOKS, &old, &invocation(&old));
    merge_event_entry(&config, HookEvent::PreToolUse, &old_entry, &[]).unwrap();
    let mut updated = hook();
    updated.handlers[0].timeout = 20;
    let new_entry = group_entry(crate::codex::HOOKS, &updated, &invocation(&updated));
    merge_event_entry(
        &config,
        HookEvent::PreToolUse,
        &new_entry,
        &[serde_json::to_string(&old_entry).unwrap()],
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    let entries = value["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        1,
        "the old version is replaced, not duplicated"
    );
    assert_eq!(entries[0]["hooks"][0]["timeout"], 22);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn drift_blocks_removal_and_an_empty_file_is_removed() {
    let root = uze_testkit::temp::scratch("hooks-drift");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let entry = group_entry(crate::codex::HOOKS, &hook(), &invocation(&hook()));
    let expected = serde_json::to_string(&entry).unwrap();
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    // A user rewrites the UZE group — removal must inspect first and refuse.
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    fs::write(
        &config,
        serde_json::to_string(&value)
            .unwrap()
            .replace("\"timeout\":12", "\"timeout\":99"),
    )
    .unwrap();
    assert_eq!(
        remove_event_entry(&config, HookEvent::PreToolUse, &expected)
            .unwrap()
            .state,
        AttachmentState::Drifted,
        "drift refuses detach and preserves the file"
    );
    assert!(config.exists());
    // Re-attach restores the exact entry beside the drifted user copy;
    // removal then deletes exactly the UZE entry and leaves the user's
    // edited copy untouched.
    merge_event_entry(
        &config,
        HookEvent::PreToolUse,
        &entry,
        std::slice::from_ref(&expected),
    )
    .unwrap();
    assert_eq!(
        remove_event_entry(&config, HookEvent::PreToolUse, &expected)
            .unwrap()
            .state,
        AttachmentState::Missing
    );
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    let groups = value["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "the drifted user copy survives removal");
    assert_eq!(groups[0]["hooks"][0]["timeout"], 99);
    // A UZE-created file holding nothing but UZE's own entry is removed
    // entirely once that entry goes.
    let solo = root.join("solo.json");
    merge_event_entry(&solo, HookEvent::PreToolUse, &entry, &[]).unwrap();
    assert_eq!(
        remove_event_entry(&solo, HookEvent::PreToolUse, &expected)
            .unwrap()
            .state,
        AttachmentState::Missing
    );
    assert!(!solo.exists(), "an empty UZE-only file is cleaned up");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_opencode_plugin_is_the_wrapper_with_the_packages_groups_as_data() {
    let plugin = opencode_bridge(
        crate::opencode::HOOKS,
        &[&hook()],
        Path::new("/tmp/plugin root"),
        "hook-demo",
    );
    // V2 plugin API (spec: opencode.ai/v2/docs/build/plugins): the
    // default export is the definition registering ctx.tool.hook
    // callbacks, with no import the harness would have to resolve.
    assert!(!plugin.contains("import "), "{plugin}");
    assert!(plugin.contains("export default {"));
    assert!(plugin.contains("id: \"hooks-hook-demo\""));
    assert!(plugin.contains("ctx.tool.hook(\"execute.before\""));
    assert!(plugin.contains("ctx.tool.hook(\"execute.after\""));
    assert!(
        plugin.contains("Bun.spawn"),
        "the harness's embedded Bun runtime executes handlers"
    );
    assert!(plugin.contains("\"event\":\"pre_tool_use\""));
    assert!(plugin.contains("\"matchers\":[\"bash\",\"Write\"]"));
    assert!(plugin.contains("\"effect\":\"deny\""));
    assert!(
        plugin.contains("bash: { tool: \"shell\", fields: (input) => ({ HOOK_COMMAND:"),
        "the alias table comes from the one vocabulary"
    );
    assert!(
        plugin.contains("code === 3"),
        "the decision channel is the exit code"
    );
    assert_eq!(
        plugin.matches("ctx.tool.hook(").count(),
        2,
        "no stop surface is ever claimed for OpenCode: the two tool hooks are all it registers"
    );
    assert!(
        !plugin.to_lowercase().contains("uze"),
        "nothing in the delivered artifact names the packager"
    );
    assert_eq!(
        plugin,
        opencode_bridge(
            crate::opencode::HOOKS,
            &[&hook()],
            Path::new("/tmp/plugin root"),
            "hook-demo"
        ),
        "generation is deterministic"
    );
}

#[test]
fn bridge_path_lives_in_the_auto_discovered_global_plugin_directory() {
    let root = uze_testkit::temp::scratch("hooks-path");
    let bridge = opencode_bridge_path(&root, "demo");
    assert_eq!(
        bridge,
        root.join("plugins/hooks-demo.ts"),
        "the single load source is the harness's global plugin directory"
    );
    assert!(
        !bridge.to_string_lossy().contains(".opencode"),
        "no legacy nested discovery path"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bridge_file_cleanup_removes_only_uzes_files() {
    let root = uze_testkit::temp::scratch("hooks-cleanup");
    let bridge = root.join("plugins/hooks-demo.ts");
    fs::create_dir_all(bridge.parent().unwrap()).unwrap();
    fs::write(&bridge, "// generated").unwrap();
    remove_bridge_file(&bridge).unwrap();
    assert!(!bridge.exists());
    assert!(
        !bridge.parent().unwrap().exists(),
        "an empty plugins dir left behind only by this file is removed"
    );
    // A foreign plugin file in the directory keeps it alive.
    fs::create_dir_all(bridge.parent().unwrap()).unwrap();
    fs::write(bridge.parent().unwrap().join("foreign.ts"), "// foreign").unwrap();
    fs::write(&bridge, "// generated").unwrap();
    remove_bridge_file(&bridge).unwrap();
    assert!(
        bridge.parent().unwrap().exists(),
        "a non-empty directory is preserved"
    );
    let _ = fs::remove_dir_all(root);
}

/// A merge rewrites the whole document, so the order the user's own
/// keys are in is UZE's to lose. A hand-organised `settings.json` must
/// come back in the order it was written, with the merged key appended
/// rather than sorted into the middle.
#[test]
fn a_merge_keeps_the_users_own_key_order() {
    let root = uze_testkit::temp::scratch("hooks-key-order");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("settings.json");
    fs::write(
        &config,
        r#"{"zed":{"nested":1,"already":2},"model":"opus","apiKeyHelper":"~/bin/key"}"#,
    )
    .unwrap();

    let entry = group_entry(crate::claude::HOOKS, &hook(), &invocation(&hook()));
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();

    let after = fs::read_to_string(&config).unwrap();
    let keys: Vec<&str> = after
        .lines()
        .filter_map(|line| line.strip_prefix("  \""))
        .filter_map(|line| line.split('"').next())
        .collect();
    assert_eq!(
        keys,
        ["zed", "model", "apiKeyHelper", "hooks"],
        "the user's keys keep their order and UZE's is appended: {after}"
    );
    assert!(
        after.contains("\"nested\": 1,"),
        "a nested foreign object keeps its own order too: {after}"
    );
    let _ = fs::remove_dir_all(root);
}

/// The executable bit is half the wrapper: `write_atomic` publishes
/// under the umask and chmods afterwards, so a crash between the two
/// leaves the right bytes unrunnable — exit 126, which a `deny` group
/// turns into a permanent block.
// Unix file modes, which Windows does not keep.
#[cfg(unix)]
#[test]
fn a_wrapper_that_lost_its_executable_bit_is_drift_and_is_repaired() {
    use std::os::unix::fs::PermissionsExt;

    let root = uze_testkit::temp::scratch("hooks-wrapper-mode");
    let wrapper = root.join("hooks").join("exec");
    let source = wrapper_source(crate::claude::HOOKS).unwrap();
    materialize_wrapper(&wrapper, &source).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o644)).unwrap();

    assert!(
        matches!(
            inspect_wrapper(crate::claude::HOOKS, &wrapper),
            WrapperState::Broken(AttachmentInspection {
                state: AttachmentState::Drifted,
                ..
            })
        ),
        "a wrapper the harness cannot execute is drift, not a match"
    );
    materialize_wrapper(&wrapper, &source).unwrap();
    assert_eq!(
        fs::metadata(&wrapper).unwrap().permissions().mode() & 0o777,
        0o755,
        "re-materializing repairs the mode even when the bytes match"
    );
    assert!(matches!(
        inspect_wrapper(crate::claude::HOOKS, &wrapper),
        WrapperState::Current
    ));
    assert_eq!(fs::read_to_string(&wrapper).unwrap(), source);
    let _ = fs::remove_dir_all(root);
}

fn hook_receipt(
    config: &Path,
    entry_name: &str,
    expected: &str,
    wrapper: &Path,
) -> uze_core::integration::AttachmentReceipt {
    uze_core::integration::AttachmentReceipt {
        package_id: "pkg@market".to_owned(),
        resource_identity: None,
        integration: "antigravity".to_owned(),
        artifact: uze_core::integration::ManagedArtifact::HookConfigEntry {
            config_file: config.to_path_buf(),
            entry_name: entry_name.to_owned(),
            event: HookEvent::PreToolUse,
            expected: expected.to_owned(),
            wrapper: wrapper.to_path_buf(),
        },
    }
}

/// The shared wrapper outlives every entry but the last one. The prune
/// runs inside a detach, while the ledger still lists the receipt being
/// detached — so "still used" has to be read from the harness's config,
/// not from the ledger, or the wrapper is never removed at all. Told on
/// Antigravity's named entries, which only the POSIX wrapper serves.
#[cfg(unix)]
#[test]
fn the_last_detached_hook_entry_takes_the_shared_wrapper_with_it() {
    let root = uze_testkit::temp::scratch("hooks-prune");
    fs::create_dir_all(&root).unwrap();
    let home = UzeHome::at(root.join("home"));
    let config = root.join("hooks.json");
    let wrapper = crate::antigravity::HOOKS.wrapper_path(&home);
    materialize_wrapper(
        &wrapper,
        &wrapper_source(crate::antigravity::HOOKS).unwrap(),
    )
    .unwrap();

    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        &wrapper,
        Path::new("/pkg"),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    let names = ["pkg@market:protect-env", "other@market:protect-env"];
    for name in names {
        merge_named_entry(&config, name, &entry).unwrap();
        uze_core::state::record_receipt(&home, hook_receipt(&config, name, &expected, &wrapper))
            .unwrap();
    }

    remove_named_entry(&config, names[0], &expected).unwrap();
    prune_shared_wrapper(&home, "antigravity", crate::antigravity::HOOKS);
    assert!(
        wrapper.exists(),
        "a wrapper another entry still runs is kept"
    );

    remove_named_entry(&config, names[1], &expected).unwrap();
    prune_shared_wrapper(&home, "antigravity", crate::antigravity::HOOKS);
    assert!(
        !wrapper.exists(),
        "the last detached entry takes the shared wrapper with it"
    );
    let _ = fs::remove_dir_all(root);
}

/// A ledger that cannot be read has not answered "nothing uses it"; it
/// has not answered at all. Deleting a wrapper live entries still run
/// leaves every one of them exiting 127 — which every harness reads as
/// non-blocking. Told on Antigravity's named entries, which only the POSIX
/// wrapper serves.
#[cfg(unix)]
#[test]
fn an_unreadable_ledger_keeps_the_shared_wrapper() {
    let root = uze_testkit::temp::scratch("hooks-prune-ledger");
    fs::create_dir_all(&root).unwrap();
    let home = UzeHome::at(root.join("home"));
    let config = root.join("hooks.json");
    let wrapper = crate::antigravity::HOOKS.wrapper_path(&home);
    materialize_wrapper(
        &wrapper,
        &wrapper_source(crate::antigravity::HOOKS).unwrap(),
    )
    .unwrap();

    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        &wrapper,
        Path::new("/pkg"),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    merge_named_entry(&config, "pkg@market:protect-env", &entry).unwrap();
    uze_core::state::record_receipt(
        &home,
        hook_receipt(&config, "pkg@market:protect-env", &expected, &wrapper),
    )
    .unwrap();

    let ledger = home.state_dir().join("attachments.json");
    assert!(ledger.exists(), "the receipt was recorded where it is read");
    fs::write(&ledger, b"{ this is not json").unwrap();

    prune_shared_wrapper(&home, "antigravity", crate::antigravity::HOOKS);
    assert!(
        wrapper.exists(),
        "an unreadable ledger blocks the destructive step, it does not authorize it"
    );
    let _ = fs::remove_dir_all(root);
}

/// "Still used" is about what runs the wrapper, not about what matches
/// the receipt. A hand-edited entry is drift — the harness still fires
/// it, and on an event-array config it reads as *absent*, so the
/// wrapper's own path in the file is what settles it.
#[test]
fn an_entry_that_drifted_still_counts_as_using_the_wrapper() {
    let root = uze_testkit::temp::scratch("hooks-prune-drift");
    fs::create_dir_all(&root).unwrap();
    let home = UzeHome::at(root.join("home"));
    let config = root.join("settings.json");
    let wrapper = crate::claude::HOOKS.wrapper_path(&home);
    materialize_wrapper(&wrapper, &wrapper_source(crate::claude::HOOKS).unwrap()).unwrap();

    let entry = group_entry(
        crate::claude::HOOKS,
        &hook(),
        &wrapper_exec(&wrapper, &hook(), Path::new("/pkg")),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    let mut receipt = hook_receipt(&config, "pkg@market:protect-env", &expected, &wrapper);
    receipt.integration = "claude".to_owned();
    uze_core::state::record_receipt(&home, receipt).unwrap();

    // The user edits the timeout: the entry no longer matches the
    // receipt, and still runs the wrapper on every tool call.
    let mut document: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    document["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"] = serde_json::json!(99);
    fs::write(&config, serde_json::to_string_pretty(&document).unwrap()).unwrap();
    assert_eq!(
        inspect_event_entry(&config, HookEvent::PreToolUse, &expected).state,
        AttachmentState::Drifted
    );

    prune_shared_wrapper(&home, "claude", crate::claude::HOOKS);
    assert!(
        wrapper.exists(),
        "a wrapper a live entry still names is never removed"
    );
    let _ = fs::remove_dir_all(root);
}

/// A build that changes the template must not strand every hook it
/// delivered before: the wrapper is generated tier, so an earlier
/// build's copy is still UZE's to remove, and the next attach
/// reproduces the current one.
#[test]
fn a_wrapper_an_earlier_build_wrote_still_removes() {
    let root = uze_testkit::temp::scratch("hooks-stale-wrapper");
    fs::create_dir_all(&root).unwrap();
    let home = UzeHome::at(root.join("home"));
    let config = root.join("settings.json");
    let wrapper = crate::claude::HOOKS.wrapper_path(&home);
    let earlier =
        format!("{WRAPPER_HEADER}, from a template this build no longer writes\nexit 0\n");
    materialize_wrapper(&wrapper, &earlier).unwrap();
    let entry = crate::claude::HOOKS.event_entry(&hook(), Path::new("/pkg"), &wrapper);
    let expected = serde_json::to_string(&entry).unwrap();
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    let delivered = HookEntry {
        config_file: &config,
        entry_name: "pkg@market:protect-env",
        event: HookEvent::PreToolUse,
        expected: &expected,
        wrapper: &wrapper,
    };

    assert_eq!(
        crate::claude::HOOKS.inspect_entry(&delivered).state,
        AttachmentState::Matched
    );
    assert_eq!(
        crate::claude::HOOKS
            .detach_entry(&home, "claude", &delivered)
            .unwrap()
            .state,
        AttachmentState::Missing
    );
    assert!(!config.exists(), "the entry went with the receipt");
    assert!(!wrapper.exists(), "the last entry took the wrapper with it");
    let _ = fs::remove_dir_all(root);
}

/// Only the header marks a wrapper as generated; a file somebody else
/// put there is still not UZE's to remove.
// Its stand-in programs are POSIX shell scripts.
#[cfg(unix)]
#[test]
fn a_wrapper_without_the_generated_header_is_drift() {
    let root = uze_testkit::temp::scratch("hooks-foreign-wrapper");
    let wrapper = root.join("hooks").join("exec");
    materialize_wrapper(&wrapper, "#!/bin/sh\nexit 0\n").unwrap();
    assert!(matches!(
        inspect_wrapper(crate::claude::HOOKS, &wrapper),
        WrapperState::Broken(AttachmentInspection {
            state: AttachmentState::Drifted,
            ..
        })
    ));
    let _ = fs::remove_dir_all(root);
}

/// Reading an edited entry as absent forgets its receipt, and the
/// edited entry goes on running the package's handlers after removal.
#[test]
fn an_edited_event_entry_is_drift_on_both_invocation_forms() {
    let root = uze_testkit::temp::scratch("hooks-edited-entry");
    fs::create_dir_all(&root).unwrap();
    let wrapper = Path::new("/state/hooks/exec");
    for target in [crate::claude::HOOKS, crate::codex::HOOKS] {
        let config = root.join(format!("{target}.json"));
        let entry = target.event_entry(&hook(), Path::new("/pkg"), wrapper);
        let expected = serde_json::to_string(&entry).unwrap();
        merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
        let mut document: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
        document["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"] = serde_json::json!(99);
        fs::write(&config, document.to_string()).unwrap();

        assert_eq!(
            inspect_event_entry(&config, HookEvent::PreToolUse, &expected).state,
            AttachmentState::Drifted,
            "{target}"
        );
        assert_eq!(
            remove_event_entry(&config, HookEvent::PreToolUse, &expected)
                .unwrap()
                .state,
            AttachmentState::Drifted,
            "{target}: drift refuses the detach"
        );
    }
    let _ = fs::remove_dir_all(root);
}

/// The wrapper is shared by every package, so naming it is not enough:
/// another package's entry is not this receipt's edited one.
#[test]
fn another_packages_entry_through_the_same_wrapper_is_not_drift() {
    let root = uze_testkit::temp::scratch("hooks-other-package");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let wrapper = Path::new("/state/hooks/exec");
    let ours = crate::codex::HOOKS.event_entry(&hook(), Path::new("/pkg"), wrapper);
    let theirs = crate::codex::HOOKS.event_entry(&hook(), Path::new("/pkg-other"), wrapper);
    merge_event_entry(&config, HookEvent::PreToolUse, &theirs, &[]).unwrap();
    assert_eq!(
        inspect_event_entry(
            &config,
            HookEvent::PreToolUse,
            &serde_json::to_string(&ours).unwrap()
        )
        .state,
        AttachmentState::Missing
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn an_unreadable_ledger_refuses_to_merge_rather_than_duplicate() {
    let root = uze_testkit::temp::scratch("hooks-previous-ledger");
    let home = UzeHome::at(root.join("home"));
    fs::create_dir_all(home.state_dir()).unwrap();
    fs::write(home.state_dir().join("attachments.json"), b"{ not json").unwrap();
    assert!(previous_hook_entry_content(&home, "claude", "pkg@market:protect-env").is_err());
    let _ = fs::remove_dir_all(root);
}

/// The Windows wrapper is generated and pinned on every platform, so a
/// change to it is reviewed where the suite runs, not first seen on a
/// Windows machine.
#[test]
fn the_powershell_wrapper_is_one_byte_identical_file_per_harness() {
    for target in [
        crate::claude::HOOKS,
        crate::codex::HOOKS,
        crate::antigravity::HOOKS,
    ] {
        let Some(source) = PowerShellWrapper::source(target) else {
            continue;
        };
        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("goldens")
            .join(format!("hooks-exec-{target}.ps1"));
        if std::env::var_os("UZE_REGENERATE_GOLDENS").is_some() {
            fs::write(&golden, &source).unwrap();
        }
        assert_eq!(
            fs::read_to_string(&golden).unwrap_or_default(),
            source,
            "{} is out of date; regenerate it with UZE_REGENERATE_GOLDENS=1",
            golden.display()
        );
        assert!(
            source.starts_with('\u{feff}'),
            "Windows PowerShell reads a script without a byte-order mark as ANSI"
        );
        assert!(
            !source.to_lowercase().contains("uze"),
            "nothing in a delivered artifact may name the packager"
        );
    }
}

/// A harness with no measured Windows entry form gets no Windows wrapper,
/// so its hooks are reported rather than delivered there.
#[test]
fn a_harness_without_a_windows_dialect_has_no_windows_wrapper() {
    assert!(PowerShellWrapper::source(crate::antigravity::HOOKS).is_none());
    assert!(PowerShellWrapper::source(crate::claude::HOOKS).is_some());
}

/// Codex on Windows runs a shell command without firing PreToolUse, which
/// only the Windows template declares, so a shell guard is reported there
/// and delivered everywhere else.
#[test]
fn codex_s_windows_shell_gap_is_declared_by_the_windows_template_alone() {
    let gap = PowerShellWrapper::unfired(crate::codex::HOOKS);
    assert!(
        gap.iter()
            .any(|(event, tool, why)| *event == HookEvent::PreToolUse
                && *tool == "shell"
                && why.contains("openai/codex/issues/24453")),
        "{gap:?}"
    );
    assert!(PosixWrapper::unfired(crate::codex::HOOKS).is_empty());
    assert!(PowerShellWrapper::unfired(crate::claude::HOOKS).is_empty());
}
