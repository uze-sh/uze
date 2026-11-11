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
            args: None,
            interpreter: None,
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
        Roots::same(Path::new("/pkg")),
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

pub(super) const TARGETS: [HookTarget; 4] = [
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
    assert_eq!(field(crate::codex::HOOKS), Some("command"));
    assert_eq!(field(crate::antigravity::HOOKS), Some("CommandLine"));
    assert_eq!(field(crate::opencode::HOOKS), Some("command"));
}

#[test]
fn a_renamed_vendor_tool_still_normalizes_to_its_alias() {
    let alias = |native| {
        vocabulary(crate::claude::HOOKS)
            .binding_for_native(native)
            .map(|binding| binding.alias)
    };
    assert_eq!(alias("Bash"), Some("shell"));
    assert_eq!(alias("PowerShell"), Some("shell"));
    assert_eq!(
        tool_names(crate::claude::HOOKS, &HookMatcher::Portable("shell".into())),
        ["Bash", "PowerShell"],
        "the matcher intercepts every name this harness's shell tool answers to"
    );
}

/// A group naming only aliases a harness has no tool for would be
/// delivered with a matcher that never fires; it is reported instead, and
/// one that also names a tool the harness has is delivered as usual.
#[test]
fn a_group_that_could_never_fire_here_is_reported_not_delivered() {
    let package = uze_testkit::temp::scratch("unbound-only");
    fs::create_dir_all(&package).unwrap();
    let with = |matchers: Vec<HookMatcher>| {
        let mut group = hook();
        group.matchers = matchers;
        let mut resource = hook_resource(&package);
        resource.capability.payload = serde_json::to_vec(&group).unwrap();
        hook_plan(&resource, crate::codex::HOOKS, false, "evidence.", |_| {
            Some(ManagedArtifact::HookConfigEntry {
                config_file: package.join("hooks.json"),
                entry_name: "demo:protect-env".into(),
                event: HookEvent::PreToolUse,
                expected: "{}".into(),
                wrapper: package.join("exec"),
            })
        })
    };
    let unbound = with(vec![HookMatcher::Portable("file.read".into())]);
    assert_eq!(unbound.route, CompatibilityRoute::Unsupported);
    assert!(
        unbound.evidence.contains("`file.read`"),
        "{}",
        unbound.evidence
    );
    // `agent.spawn` fires on every platform; Codex's Windows shell does not.
    let mixed = with(vec![
        HookMatcher::Portable("file.read".into()),
        HookMatcher::Portable("agent.spawn".into()),
    ]);
    assert_ne!(
        mixed.route,
        CompatibilityRoute::Unsupported,
        "{}",
        mixed.evidence
    );
}

/// A harness that announces only a new session never runs a group that
/// waits for a resume or a clear: it is reported, not delivered, while a
/// group that also waits for a new session is delivered as usual.
#[test]
fn a_session_start_the_harness_never_announces_is_reported_not_delivered() {
    let package = uze_testkit::temp::scratch("unannounced-source");
    fs::create_dir_all(&package).unwrap();
    let with = |target: HookTarget, sources: &[&str]| {
        let mut group = hook();
        group.event = HookEvent::SessionStart;
        group.effect = uze_core::hook::HookEffect::Observe;
        group.matchers = sources
            .iter()
            .map(|source| HookMatcher::Source((*source).to_owned()))
            .collect();
        let mut resource = hook_resource(&package);
        resource.capability.payload = serde_json::to_vec(&group).unwrap();
        hook_plan(&resource, target, false, "evidence.", |_| {
            Some(ManagedArtifact::HookConfigEntry {
                config_file: package.join("hooks.json"),
                entry_name: "demo:protect-env".into(),
                event: HookEvent::SessionStart,
                expected: "{}".into(),
                wrapper: package.join("exec"),
            })
        })
    };
    let resume_only = with(crate::antigravity::HOOKS, &["resume"]);
    assert_eq!(resume_only.route, CompatibilityRoute::Unsupported);
    assert!(
        resume_only.evidence.contains("`resume`"),
        "{}",
        resume_only.evidence
    );
    let with_startup = with(crate::antigravity::HOOKS, &["resume", "startup"]);
    assert_ne!(
        with_startup.route,
        CompatibilityRoute::Unsupported,
        "{}",
        with_startup.evidence
    );
    let resume_on_claude = with(crate::claude::HOOKS, &["resume"]);
    assert_ne!(
        resume_on_claude.route,
        CompatibilityRoute::Unsupported,
        "{}",
        resume_on_claude.evidence
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
    assert_eq!(
        wrapper,
        crate::claude::HOOKS.wrapper_path(&home, resource.package_id.as_str())
    );
    let entry: serde_json::Value = serde_json::from_str(&expected).unwrap();
    let (program, _) = uze_platform::shell::script(&wrapper.display().to_string());
    assert_eq!(entry["hooks"][0]["command"], program);

    assert!(
        wrapper_source(crate::opencode::HOOKS).is_none(),
        "a harness the template generator does not cover has no wrapper"
    );
}

/// A package read in place, outside the sources, is its own whole root:
/// its hook names a path where the package has it, never under the
/// carried directory a build keeps.
#[test]
fn a_package_read_in_place_names_its_hook_paths_where_it_has_them() {
    let home = UzeHome::at(Path::new("/tmp/uze-home"));
    let package = Path::new("/pkg");
    let mut hook = hook();
    hook.handlers[0].command = uze_core::shell::ShellCommand::spelled(
        "${PLUGIN_ROOT}/bin/check",
        "& \"${PLUGIN_ROOT}/bin/check.ps1\"",
    );
    let resource = Resource::from_package(
        uze_core::store::PackageId::from_plugin_name("demo", &package.join("plugin.json")).unwrap(),
        package.to_path_buf(),
        uze_core::capability::Capability {
            kind: uze_core::capability::CapabilityKind::Hook,
            path: package.join(HOOKS_FILE_NAME),
            payload: serde_json::to_vec(&hook).unwrap(),
        },
    );
    assert!(
        crate::claude::STORE.carried.contains(&"bin"),
        "the build of this harness carries `bin`"
    );
    let plan = crate::claude::HOOKS.entry_plan(
        &home,
        &resource,
        PathBuf::from("/config/settings.json"),
        "evidence.",
    );
    let ExposureMechanism::Managed(ManagedArtifact::HookConfigEntry { expected, .. }) =
        plan.mechanism
    else {
        panic!("a harness with a template delivers: {:?}", plan.mechanism);
    };
    assert!(
        !expected.contains(crate::shared::package_root::CARRIED_DIR),
        "{expected}"
    );
    assert!(expected.contains("bin/check"), "{expected}");
}

/// The same answer one level up: the exposure plan reports Unsupported
/// with the reason, rather than an entry pointing at something that is
/// not there.
#[test]
fn a_hook_that_cannot_be_delivered_is_reported_unsupported() {
    let package = uze_testkit::temp::scratch("undeliverable");
    fs::create_dir_all(&package).unwrap();
    let resource = hook_resource(&package);
    let plan = hook_plan(&resource, crate::claude::HOOKS, false, "evidence.", |_| {
        None
    });
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
            Roots::same(Path::new("/pkg")),
        ),
        "archive": agy_named_entry(crate::antigravity::HOOKS, &stop,
            Path::new("/state/hooks/exec"),
            Roots::same(Path::new("/pkg")),
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
    // Compared through the sealed line rather than read out of it: on
    // Windows that line is encoded against `cmd /c`.
    let arguments = wrapper_arguments(&stop, Roots::same(Path::new("/pkg")), &stop.handlers);
    assert_eq!(arguments[1..3], ["stop", "observe"]);
    assert_eq!(
        flat["command"],
        uze_platform::shell::sealed_script_line("/state/hooks/exec", &arguments),
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
        Roots::same(Path::new("/pkg")),
    );
    let document = serde_json::to_string(&entry).unwrap();
    assert_eq!(entry["PreToolUse"][0]["matcher"], "run_command|Write");
    assert!(
        entry.get("hooks").is_none(),
        "the named key holds the event map directly; a `hooks` wrapper is one dead hook"
    );
    // Compared through the sealed line rather than read out of it: on
    // Windows that line is encoded against `cmd /c`.
    let arguments = wrapper_arguments(&hook(), Roots::same(Path::new("/pkg")), &hook().handlers);
    assert_eq!(arguments[..3], ["/pkg", "pre_tool_use", "deny"]);
    assert_eq!(
        entry["PreToolUse"][0]["hooks"][0]["command"],
        uze_platform::shell::sealed_script_line("/state/hooks/exec", &arguments),
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
            Roots::same(Path::new("/pkg"))
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
        Roots::same(Path::new("/pkg")),
    );
    let expected = serde_json::to_string(&entry).unwrap();

    merge_named_entry(&config, name, &entry, &[]).unwrap();
    assert_eq!(
        inspect_named_entry(&config, name, &expected).state,
        AttachmentState::Matched
    );
    // Merging the same entry again changes nothing (idempotence).
    merge_named_entry(&config, name, &entry, &[]).unwrap();

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
        Roots::same(Path::new("/pkg")),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    merge_named_entry(&config, name, &entry, &[]).unwrap();

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
        Roots::same(Path::new("/pkg")),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    merge_named_entry(&config, name, &entry, &[]).unwrap();
    remove_named_entry(&config, name, &expected).unwrap();
    assert!(!config.exists(), "UZE removes the file it alone created");
}

/// A key UZE namespaces can still hold a hand edit, or an entry whose
/// receipt is gone; overwriting it would discard what nobody asked to lose.
/// Only this entry or a version a receipt recorded is replaced.
#[test]
fn a_named_entry_is_replaced_only_when_a_receipt_recorded_it() {
    let root = uze_testkit::temp::scratch("hooks-named-conflict");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let name = "pkg@market:protect-env";
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        Path::new("/state/hooks/exec"),
        Roots::same(Path::new("/pkg")),
    );
    let edited = serde_json::json!({ "PreToolUse": [{ "command": "hand-edited" }] });
    fs::write(&config, serde_json::json!({ name: edited }).to_string()).unwrap();

    assert!(matches!(
        merge_named_entry(&config, name, &entry, &[]),
        Err(UzeError::ManagedEntryConflict(_))
    ));
    let kept: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(kept[name], edited, "the conflicting entry is untouched");

    merge_named_entry(&config, name, &entry, &[edited.to_string()]).unwrap();
    let replaced: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(
        replaced[name], entry,
        "a recorded earlier version is replaced"
    );
    let _ = fs::remove_dir_all(root);
}

// A symbolic link, which Windows lets an ordinary account make only in developer mode.
#[cfg(unix)]
#[test]
fn an_emptied_config_behind_a_link_is_written_not_removed() {
    let root = uze_testkit::temp::scratch("hooks-named-linked");
    fs::create_dir_all(&root).unwrap();
    let managed = root.join("dotfiles-hooks.json");
    let config = root.join("hooks.json");
    let name = "pkg@market:protect-env";
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        Path::new("/state/hooks/exec"),
        Roots::same(Path::new("/pkg")),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    fs::write(&managed, "{}").unwrap();
    std::os::unix::fs::symlink(&managed, &config).unwrap();
    merge_named_entry(&config, name, &entry, &[]).unwrap();
    remove_named_entry(&config, name, &expected).unwrap();

    assert!(
        fs::symlink_metadata(&config)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the dotfile manager's link survives the last entry leaving"
    );
    let left: serde_json::Value = serde_json::from_slice(&fs::read(&managed).unwrap()).unwrap();
    assert_eq!(left, serde_json::json!({}));
    let _ = fs::remove_dir_all(root);
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
fn a_fail_closed_group_carries_the_harnesss_block_on_failure_marker() {
    for effect in [HookEffect::Deny, HookEffect::Ask, HookEffect::Transform] {
        let mut hook = hook();
        hook.effect = effect;
        let entry = group_entry(crate::claude::HOOKS, &hook, &invocation(&hook));
        assert_eq!(entry["hooks"][0]["onFailure"], "block", "{effect:?}");
    }
    let mut observed = hook();
    observed.effect = HookEffect::Observe;
    let entry = group_entry(crate::claude::HOOKS, &observed, &invocation(&observed));
    assert!(
        entry["hooks"][0].get("onFailure").is_none(),
        "an observational group must not block when it fails"
    );
    let codex = group_entry(crate::codex::HOOKS, &hook(), &invocation(&hook()));
    assert!(
        codex["hooks"][0].get("onFailure").is_none(),
        "a harness without the control gets no key"
    );
}

/// An entry an earlier build wrote without the marker is replaced by the
/// next attach, never left beside the new one.
#[test]
fn the_next_attach_adds_the_marker_to_an_entry_written_without_it() {
    let root = uze_testkit::temp::scratch("hooks-marker-upgrade");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("settings.json");
    let current = group_entry(crate::claude::HOOKS, &hook(), &invocation(&hook()));
    let mut earlier = current.clone();
    earlier["hooks"][0]
        .as_object_mut()
        .unwrap()
        .remove("onFailure");
    merge_event_entry(&config, HookEvent::PreToolUse, &earlier, &[]).unwrap();
    merge_event_entry(
        &config,
        HookEvent::PreToolUse,
        &current,
        &[serde_json::to_string(&earlier).unwrap()],
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    let entries = value["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0]["hooks"][0]["onFailure"], "block");
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
fn the_opencode_hooks_are_one_runtime_over_each_packages_document() {
    let document = opencode_hook_document(
        crate::opencode::HOOKS,
        &[&hook()],
        Path::new("/tmp/plugin root"),
    );
    let parsed: serde_json::Value = serde_json::from_str(&document).unwrap();
    assert_eq!(parsed["version"], OPENCODE_DOCUMENT_VERSION);
    assert_eq!(parsed["root"], "/tmp/plugin root");
    let group = &parsed["groups"][0];
    assert_eq!(group["event"], "pre_tool_use");
    assert_eq!(group["matchers"], serde_json::json!(["shell", "Write"]));
    assert_eq!(group["effect"], "deny");
    assert!(
        !document.to_lowercase().contains("uze"),
        "nothing in the package's delivered artifact names the packager"
    );
    assert_eq!(
        document,
        opencode_hook_document(
            crate::opencode::HOOKS,
            &[&hook()],
            Path::new("/tmp/plugin root")
        ),
        "generation is deterministic"
    );
    assert!(document_carries_groups(
        crate::opencode::HOOKS,
        &document,
        &[&hook()],
        Path::new("/tmp/plugin root"),
    ));

    // Each handler is carried for the runtime to run: a shell line with
    // the package root resolved, and a JavaScript script in OpenCode's
    // own Bun, from where the package is delivered.
    let line = &group["handlers"][0];
    let command = line["command"].as_str().expect("a shell line is carried");
    assert!(command.contains("check"), "{line}");
    assert!(!command.contains("${PLUGIN_ROOT}"), "{line}");
    assert_eq!(line["timeout"], 10);
    let mut watch = hook();
    watch.id = "watch".into();
    watch.handlers = vec![CommandHook {
        handler_type: CommandHandlerType::Command,
        command: "hooks/watch.js".into(),
        args: Some(vec!["--quiet".into()]),
        interpreter: None,
        timeout: 5,
    }];
    let with_script = opencode_hook_document(
        crate::opencode::HOOKS,
        &[&hook(), &watch],
        Path::new("/tmp/plugin root"),
    );
    let parsed: serde_json::Value = serde_json::from_str(&with_script).unwrap();
    let script = &parsed["groups"][1]["handlers"][0];
    assert_eq!(script["bun"], true, "{script}");
    let argv = script["argv"]
        .as_array()
        .expect("the script runs from its words");
    let delivered = crate::opencode::HOOKS
        .store
        .root_of(Path::new("/tmp/plugin root"))
        .join("hooks/watch.js");
    assert_eq!(argv[0], delivered.display().to_string(), "{script}");
    assert_eq!(argv[1], "--quiet");

    // Anything but exactly these groups, at this root and version, is not
    // the document this build writes.
    assert!(!document_carries_groups(
        crate::opencode::HOOKS,
        &document,
        &[&hook(), &watch],
        Path::new("/tmp/plugin root"),
    ));
    let mut moved: serde_json::Value = serde_json::from_str(&document).unwrap();
    moved["root"] = serde_json::json!("/tmp/elsewhere");
    assert!(!document_carries_groups(
        crate::opencode::HOOKS,
        &moved.to_string(),
        &[&hook()],
        Path::new("/tmp/plugin root"),
    ));
    let mut versioned: serde_json::Value = serde_json::from_str(&document).unwrap();
    versioned["version"] = serde_json::json!(OPENCODE_DOCUMENT_VERSION + 1);
    assert!(!document_carries_groups(
        crate::opencode::HOOKS,
        &versioned.to_string(),
        &[&hook()],
        Path::new("/tmp/plugin root"),
    ));

    let section = opencode_hooks_section(crate::opencode::HOOKS, Path::new("/state/hook-groups"));
    // V2 plugin API (spec: opencode.ai/v2/docs/build/plugins): the section
    // registers ctx.tool.hook callbacks and runs handlers in the harness's
    // own Bun.
    assert!(section.contains("\nasync function hooks(ctx) {\n"));
    assert!(section.contains("ctx.tool.hook(\"execute.before\""));
    assert!(section.contains("ctx.tool.hook(\"execute.after\""));
    assert!(section.contains("Bun.spawn"));
    assert!(section.contains("const STORE = \"/state/hook-groups\";"));
    assert!(section.contains("const DOCUMENT = \"hooks/uze-groups.json\";"));
    assert!(
        section.contains("shell: { tool: \"shell\", fields: (input) => ({ HOOK_COMMAND:"),
        "the alias table comes from the one vocabulary"
    );
    assert!(
        section.contains("code === 3"),
        "the decision channel is the exit code"
    );
    assert_eq!(
        section.matches("ctx.tool.hook(").count(),
        2,
        "no stop surface is ever claimed for OpenCode: the two tool hooks are all it registers"
    );
}

#[test]
fn an_earlier_releases_bridge_lived_in_the_global_plugin_directory() {
    let root = uze_testkit::temp::scratch("hooks-path");
    let bridge = crate::opencode::earlier::hook_bridge_path(&root, "demo");
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

/// Only a delivered hook asks the machine for the wrapper's programs: a
/// package whose delivered resources hold no hook needs nothing.
#[test]
fn a_package_without_hooks_needs_nothing_for_the_wrapper() {
    let scratch = uze_testkit::temp::TempDir::new("hooks-requirements-skill");
    let package = scratch.path().join("pkg");
    fs::create_dir_all(package.join("skills/commit")).unwrap();
    fs::write(package.join("plugin.json"), r#"{"name":"flow"}"#).unwrap();
    let skill = "---\nname: commit\ndescription: Commit.\n---\nBody.\n";
    fs::write(package.join("skills/commit/SKILL.md"), skill).unwrap();
    let stored = crate::shared::test_support::stored_at(&package, "flow");
    let mut resource = crate::shared::test_support::skill_resource(&stored);
    resource.capability.payload = skill.as_bytes().to_vec();
    let home = UzeHome::at(scratch.path().join("uze"));
    let claude = crate::claude::ClaudeIntegration::new(scratch.path().join("claude"), home.clone());
    // Setup has run, so the Skill is delivered rather than held back.
    uze_core::state::record(
        &home,
        uze_core::integration::IntegrationPort::id(&claude),
        uze_core::state::IntegrationRecord::default(),
    )
    .unwrap();
    assert!(
        !matches!(
            uze_core::integration::IntegrationPort::exposure_plan(&claude, &resource).mechanism,
            ExposureMechanism::Unsupported { .. }
        ),
        "the Skill is delivered"
    );
    assert!(generated_requirements(&claude, crate::claude::HOOKS, &[&resource]).is_empty());
}

/// A bridge that could not be removed is an error, never a success: it
/// stays, and would keep running a removed package's hooks.
// A directory that refuses a removal is a Unix file mode.
#[cfg(unix)]
#[test]
fn a_bridge_that_cannot_be_removed_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let root = uze_testkit::temp::TempDir::new("hooks-cleanup-locked");
    let plugins = root.path().join("plugins");
    let bridge = plugins.join("hooks-demo.ts");
    fs::create_dir_all(&plugins).unwrap();
    fs::write(&bridge, "// generated").unwrap();
    fs::set_permissions(&plugins, fs::Permissions::from_mode(0o555)).unwrap();
    // An account that writes past permissions (root) cannot see this case.
    if fs::write(plugins.join("probe"), "").is_err() {
        assert!(remove_bridge_file(&bridge).is_err());
        assert!(bridge.is_file(), "the bridge went after all");
    }
    fs::set_permissions(&plugins, fs::Permissions::from_mode(0o755)).unwrap();
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

/// The wrapper is a file of the package's build at the reserved path: the
/// build renders it when the package has hooks, every entry names its own
/// package's copy, and detaching an entry leaves it for the build, which
/// takes it when the package goes.
// Antigravity's named entries are served by the POSIX wrapper alone.
#[cfg(unix)]
#[test]
fn each_build_carries_its_own_wrapper_and_an_entry_leaves_it_to_the_build() {
    let root = uze_testkit::temp::scratch("hooks-build-wrapper");
    fs::create_dir_all(&root).unwrap();
    let home = UzeHome::at(root.join("home"));
    let wrapper = crate::antigravity::HOOKS.wrapper_path(&home, "pkg@market");
    assert_eq!(
        wrapper,
        crate::antigravity::STORE
            .build_dir(&home, "pkg@market")
            .join(WRAPPER_RELATIVE_PATH),
        "the wrapper sits inside the package's build"
    );
    assert_ne!(
        wrapper,
        crate::antigravity::HOOKS.wrapper_path(&home, "other@market"),
        "no wrapper is shared between packages"
    );

    let package_root = root.join("package");
    fs::create_dir_all(&package_root).unwrap();
    fs::write(package_root.join("plugin.json"), r#"{"name":"pkg"}"#).unwrap();
    let package = crate::shared::test_support::stored_at(&package_root, "pkg");
    let build = root.join("build");
    fs::create_dir_all(&build).unwrap();
    render_wrapper(crate::antigravity::HOOKS, &package, &build).unwrap();
    assert!(
        !build.join(WRAPPER_RELATIVE_PATH).exists(),
        "a package without hooks gets no wrapper"
    );
    fs::write(
        package_root.join(HOOKS_FILE_NAME),
        r#"{"hooks":{"PreToolUse":[{"id":"guard","matcher":"shell","effect":"deny","hooks":[{"type":"command","command":"${PLUGIN_ROOT}/guard","timeout":10}]}]}}"#,
    )
    .unwrap();
    render_wrapper(crate::antigravity::HOOKS, &package, &build).unwrap();
    assert!(build.join(WRAPPER_RELATIVE_PATH).is_file());

    let config = root.join("hooks.json");
    materialize_wrapper(
        &wrapper,
        &wrapper_source(crate::antigravity::HOOKS).unwrap(),
    )
    .unwrap();
    let entry = agy_named_entry(
        crate::antigravity::HOOKS,
        &hook(),
        &wrapper,
        Roots::same(Path::new("/pkg")),
    );
    let expected = serde_json::to_string(&entry).unwrap();
    let name = "pkg@market:protect-env";
    merge_named_entry(&config, name, &entry, &[]).unwrap();
    let detached = crate::antigravity::HOOKS
        .detach_entry(&HookEntry {
            config_file: &config,
            entry_name: name,
            event: HookEvent::PreToolUse,
            expected: &expected,
            wrapper: &wrapper,
        })
        .unwrap();
    assert_eq!(detached.state, AttachmentState::Missing);
    assert!(wrapper.exists(), "the wrapper is the build's to take");
    let _ = fs::remove_dir_all(root);
}

/// A build that changes the template must not strand every hook it
/// delivered before: an earlier build's wrapper is still UZE's, so its
/// entry still removes, and the next delivery renders the current one.
#[test]
fn a_wrapper_an_earlier_build_wrote_still_removes() {
    let root = uze_testkit::temp::scratch("hooks-stale-wrapper");
    fs::create_dir_all(&root).unwrap();
    let home = UzeHome::at(root.join("home"));
    let config = root.join("settings.json");
    let wrapper = crate::claude::HOOKS.wrapper_path(&home, "pkg@market");
    let earlier =
        format!("{WRAPPER_HEADER}, from a template this build no longer writes\nexit 0\n");
    materialize_wrapper(&wrapper, &earlier).unwrap();
    let entry = crate::claude::HOOKS.event_entry(&hook(), Roots::same(Path::new("/pkg")), &wrapper);
    let expected = serde_json::to_string(&entry).unwrap();
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    let delivered = HookEntry {
        config_file: &config,
        entry_name: "pkg@market:protect-env",
        event: HookEvent::PreToolUse,
        expected: &expected,
        wrapper: &wrapper,
    };

    let inspection = crate::claude::HOOKS.inspect_entry(&delivered);
    assert_eq!(inspection.state, AttachmentState::Matched);
    assert!(
        inspection.reason.contains("the next install rewrites it"),
        "a matched entry says its wrapper is an earlier build's: {}",
        inspection.reason
    );

    // An edited entry is drift, and that is all it says.
    let mut document: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    document["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"] = serde_json::json!(99);
    fs::write(&config, document.to_string()).unwrap();
    let drifted = crate::claude::HOOKS.inspect_entry(&delivered);
    assert_eq!(drifted.state, AttachmentState::Drifted);
    assert!(
        !drifted.reason.contains("the next install rewrites it"),
        "{}",
        drifted.reason
    );
    merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
    let mut document: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    document["hooks"]["PreToolUse"]
        .as_array_mut()
        .unwrap()
        .retain(|group| group["hooks"][0]["timeout"] != 99);
    fs::write(&config, document.to_string()).unwrap();

    assert_eq!(
        crate::claude::HOOKS.detach_entry(&delivered).unwrap().state,
        AttachmentState::Missing
    );
    assert!(!config.exists(), "the entry went with the receipt");
    let _ = fs::remove_dir_all(root);
}

/// A wrapper whose content differs from what this build writes is
/// rewritten, executable: a template upgrade lands on the next delivery.
#[test]
fn a_changed_wrapper_is_rewritten() {
    let root = uze_testkit::temp::TempDir::new("hooks-rewritten-wrapper");
    let wrapper = root.path().join("hooks").join("exec");
    fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    fs::write(&wrapper, "#!/bin/sh\nold\n").unwrap();
    let source = wrapper_source(crate::claude::HOOKS).expect("a wrapper runs here");
    materialize_wrapper(&wrapper, &source).unwrap();
    assert_eq!(fs::read_to_string(&wrapper).unwrap(), source);
    assert!(is_executable(&wrapper));
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
        let entry = target.event_entry(&hook(), Roots::same(Path::new("/pkg")), wrapper);
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

/// An entry that no longer starts the wrapper is not this delivery
/// changed, even when it still hands on the package's root: it is gone.
#[test]
fn an_entry_that_no_longer_starts_the_wrapper_is_missing_not_drift() {
    let root = uze_testkit::temp::scratch("hooks-replaced-wrapper");
    fs::create_dir_all(&root).unwrap();
    let wrapper = Path::new("/state/hooks/exec");
    for target in [crate::claude::HOOKS, crate::codex::HOOKS] {
        let config = root.join(format!("{target}.json"));
        let entry = target.event_entry(&hook(), Roots::same(Path::new("/pkg")), wrapper);
        let expected = serde_json::to_string(&entry).unwrap();
        merge_event_entry(&config, HookEvent::PreToolUse, &entry, &[]).unwrap();
        let mut document: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
        let handler = &mut document["hooks"]["PreToolUse"][0]["hooks"][0];
        // The wrapper sits in the command line or among its arguments,
        // depending on how this platform's shell starts a script.
        let spelled = handler.to_string();
        assert!(spelled.contains("/state/hooks/exec"), "{target}: {spelled}");
        *handler =
            serde_json::from_str(&spelled.replace("/state/hooks/exec", "/state/hooks/other"))
                .unwrap();
        fs::write(&config, document.to_string()).unwrap();

        assert_eq!(
            inspect_event_entry(&config, HookEvent::PreToolUse, &expected).state,
            AttachmentState::Missing,
            "{target}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

/// The previous versions of an entry are the ones this integration's
/// receipts recorded under that entry's name, and no other: another
/// group, or another harness's entry of the same name, is never replaced
/// as an earlier version.
#[test]
fn only_this_integrations_receipts_of_the_same_entry_are_previous_versions() {
    let scratch = uze_testkit::temp::TempDir::new("hooks-previous");
    let home = UzeHome::at(scratch.path().join("uze"));
    let record = |integration: &str, entry_name: &str, expected: &str| {
        uze_core::state::record_receipt(
            &home,
            uze_core::integration::AttachmentReceipt {
                package_id: "demo@local".into(),
                resource_identity: Some(format!("{integration}:{entry_name}")),
                integration: integration.into(),
                artifact: ManagedArtifact::HookConfigEntry {
                    config_file: scratch.path().join(format!("{integration}.json")),
                    entry_name: entry_name.into(),
                    event: HookEvent::PreToolUse,
                    expected: expected.into(),
                    wrapper: scratch.path().join("exec"),
                },
            },
        )
        .unwrap();
    };
    record("claude", "demo:a", r#"{"a":1}"#);
    record("claude", "demo:b", r#"{"b":1}"#);
    record("codex", "demo:a", r#"{"a":2}"#);
    assert_eq!(
        previous_hook_entry_content(&home, "claude", "demo:a").unwrap(),
        [r#"{"a":1}"#]
    );
}

/// The wrapper is shared by every package, so naming it is not enough:
/// another package's entry is not this receipt's edited one.
#[test]
fn another_packages_entry_through_the_same_wrapper_is_not_drift() {
    let root = uze_testkit::temp::scratch("hooks-other-package");
    fs::create_dir_all(&root).unwrap();
    let config = root.join("hooks.json");
    let wrapper = Path::new("/state/hooks/exec");
    let ours = crate::codex::HOOKS.event_entry(&hook(), Roots::same(Path::new("/pkg")), wrapper);
    let theirs =
        crate::codex::HOOKS.event_entry(&hook(), Roots::same(Path::new("/pkg-other")), wrapper);
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

/// Every harness a wrapper delivers to has a measured Windows entry form,
/// and so a Windows wrapper: Antigravity's arrived with the sealed line
/// that survives its `cmd /c`.
#[test]
fn every_wrapper_harness_has_a_windows_wrapper() {
    for target in [
        crate::claude::HOOKS,
        crate::codex::HOOKS,
        crate::antigravity::HOOKS,
    ] {
        assert!(PowerShellWrapper::source(target).is_some(), "{target}");
    }
}

/// An exec-form handler whose script nothing starts here is never
/// delivered, and the plan says why: a script that is neither executable
/// nor placed by its extension on POSIX, a `.sh` one on Windows. One the
/// table places is delivered as usual.
#[test]
fn an_exec_form_script_nothing_starts_here_is_reported_not_delivered() {
    let package = uze_testkit::temp::scratch("exec-form-plan");
    fs::create_dir_all(package.join("hooks")).unwrap();
    fs::write(package.join("hooks").join("guard"), "exit 0\n").unwrap();
    let with = |script: &str| {
        let mut group = hook();
        group.handlers = vec![CommandHook {
            handler_type: CommandHandlerType::Command,
            command: script.into(),
            args: Some(Vec::new()),
            interpreter: None,
            timeout: 10,
        }];
        let mut resource = hook_resource(&package);
        resource.capability.payload = serde_json::to_vec(&group).unwrap();
        hook_plan(&resource, crate::claude::HOOKS, false, "evidence.", |_| {
            Some(ManagedArtifact::HookConfigEntry {
                config_file: package.join("hooks.json"),
                entry_name: "demo:protect-env".into(),
                event: HookEvent::PreToolUse,
                expected: "{}".into(),
                wrapper: package.join("exec"),
            })
        })
    };
    let unplaced = with(uze_platform::shell::spelling(
        "hooks/guard",
        "hooks/guard.sh",
    ));
    assert_eq!(unplaced.route, CompatibilityRoute::Unsupported);
    assert!(
        unplaced.evidence.contains("not delivered on this platform"),
        "{}",
        unplaced.evidence
    );
    assert_ne!(
        with("hooks/guard.py").route,
        CompatibilityRoute::Unsupported,
        "a script the table places is delivered"
    );
    let _ = fs::remove_dir_all(package);
}

/// Core refuses a package shipping any path a build writes, and core cannot
/// name this crate: its list is the one source, and every path the hooks
/// write into a build has to be on it, or a package could ship its own.
#[test]
fn every_path_the_hooks_write_into_a_build_is_reserved_in_core() {
    for written in [
        <PosixWrapper as WrapperTemplate>::RELATIVE_PATH,
        <PowerShellWrapper as WrapperTemplate>::RELATIVE_PATH,
        OPENCODE_DOCUMENT,
        crate::shared::package_root::CARRIED_DIR,
    ] {
        assert!(
            uze_core::store::RESERVED_BUILD_PATHS.contains(&written),
            "{written} is written into a build but not reserved in uze_core::store::RESERVED_BUILD_PATHS"
        );
    }
}
