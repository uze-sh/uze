use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::*;
use crate::package::acquisition::marketplace;

/// A process-scoped Git identity the scaffold's commit can use, isolated
/// from the ambient configuration the way every test fixture here is.
fn git_identity() -> uze_testkit::env::ProcessEnvGuard<'static> {
    let configuration = uze_testkit::temp::scratch("authoring-gitconfig").join("gitconfig");
    fs::write(
        &configuration,
        "[user]\n\tname = Test\n\temail = t@example.invalid\n",
    )
    .unwrap();
    let mut environment = uze_testkit::env::scope();
    environment.set("GIT_CONFIG_GLOBAL", &configuration);
    environment.set("GIT_CONFIG_SYSTEM", &configuration);
    environment
}

use uze_testkit::process::native;

fn scratch(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

/// The load-bearing invariant: **every layout this module writes passes its
/// own check** — every combination of capability flags, and a description
/// carrying what plain YAML would misread. The templates' commented field
/// documentation cannot drift from what the parsers accept, because a drift
/// is this test failing.
#[test]
fn every_scaffold_passes_its_own_check() -> Result<()> {
    let _git_identity = git_identity();
    let description = r#"Deploys: things "fast" — it's #1"#;
    for flags in 0..16_u8 {
        let caps = ScaffoldCapabilities {
            hook: flags & 1 != 0,
            mcp: flags & 2 != 0,
            agent: flags & 4 != 0,
            instructions: flags & 8 != 0,
        };
        let root = scratch(&format!("authoring-scaffold-{flags}"));
        let market = scaffold_marketplace("tools", Some("Test tools"), &root.join("market"))?;
        let plugin = scaffold_plugin(&market, "greet", Some(description), None, &caps)?;

        let plugin_report = check_plugin(&plugin)?;
        assert!(
            plugin_report.is_clean(),
            "{caps:?}: {:?}",
            plugin_report.findings
        );
        let standard = plugin_report
            .agent_plugins
            .as_ref()
            .expect("a plugin check judges the standard");
        assert!(
            standard.conformant && plugin_report.warnings.is_empty(),
            "{caps:?}: {:?} {:?}",
            standard.divergences,
            plugin_report.warnings
        );
        let market_report = check_marketplace(&market)?;
        assert!(
            market_report.is_clean(),
            "{caps:?}: {:?}",
            market_report.findings
        );
        assert!(
            market_report
                .agent_plugins
                .as_ref()
                .is_some_and(|standard| standard.conformant),
            "{caps:?}: {:?}",
            market_report.agent_plugins
        );
        assert!(market_report.delivers.iter().any(|d| d == "greet"));

        // The marketplace manifest the scaffold wrote parses by the same
        // rule an install-time catalogue read uses — and by now names the
        // plugin the scaffold added.
        let manifest = marketplace::parse_manifest(
            &fs::read(root.join("market/marketplace.json")).expect("the manifest is there"),
        )?;
        assert_eq!(manifest.plugins.len(), 1);
        fs::remove_dir_all(&root).expect("teardown");
    }
    Ok(())
}

#[test]
fn the_skill_description_survives_yaml_verbatim() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-description");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let description = r#"Deploys: things "fast" — it's #1"#;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        Some(description),
        None,
        &ScaffoldCapabilities::default(),
    )?;
    let skill = fs::read_to_string(plugin.join("skills/greet/SKILL.md")).unwrap();
    let head = skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split("\n---\n").next())
        .unwrap();
    let frontmatter: serde_yaml::Value =
        from_str_with_config(head, &ParserConfig::serde_yaml_compat()).unwrap();
    assert_eq!(
        frontmatter
            .get("description")
            .and_then(serde_yaml::Value::as_str),
        Some(description)
    );
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

/// Each field has one home: the description is the plugin's, so it lands
/// in `plugin.json`; the category is the catalogue's, so it lands on the
/// marketplace entry. Neither is written to the other file.
#[test]
fn each_field_is_written_to_the_one_file_that_owns_it() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-category");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        Some("Says hello"),
        Some("productivity"),
        &ScaffoldCapabilities::default(),
    )?;

    let raw = read_json(&market.join("marketplace.json"))?;
    let entry = &raw["plugins"][0];
    assert_eq!(entry["category"], "productivity");
    assert!(entry.get("description").is_none(), "{entry}");
    let plugin_json = read_json(&plugin.join("plugin.json"))?;
    assert_eq!(plugin_json["description"], "Says hello");
    assert!(plugin_json.get("category").is_none(), "{plugin_json}");
    assert!(check_marketplace(&market)?.warnings.is_empty());
    assert!(check_plugin(&plugin)?.is_clean());
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

/// A describing field left on an entry is a warning, never a finding, and
/// it says the one thing to do: move, delete or reconcile.
#[test]
fn market_check_says_what_to_do_with_a_describing_field_on_an_entry() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-describing-fields");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    for name in ["moved", "same", "differs"] {
        scaffold_plugin(
            &market,
            name,
            Some("Owned"),
            None,
            &ScaffoldCapabilities::default(),
        )?;
    }
    let mut plugin_json = read_json(&market.join("plugins/differs/plugin.json"))?;
    plugin_json["keywords"] = serde_json::json!(["owned"]);
    write_json(&market.join("plugins/differs/plugin.json"), &plugin_json)?;
    let mut manifest = read_json(&market.join("marketplace.json"))?;
    let entries = manifest["plugins"].as_array_mut().unwrap();
    entries[0]["keywords"] = serde_json::json!(["left"]);
    entries[1]["description"] = serde_json::json!("Owned");
    entries[2]["keywords"] = serde_json::json!(["left"]);
    write_json(&market.join("marketplace.json"), &manifest)?;

    let report = check_marketplace(&market)?;
    assert!(
        report.is_clean(),
        "a warning, never a finding: {:?}",
        report.findings
    );
    let warning = |prefix: &str| {
        report
            .warnings
            .iter()
            .find(|warning| warning.starts_with(prefix))
            .unwrap_or_else(|| panic!("{prefix}: {:?}", report.warnings))
            .clone()
    };
    assert!(warning("moved: `keywords`").contains("move it to"));
    assert!(warning("same: `description`").contains("delete it from the entry"));
    let differs = warning("differs: `keywords`");
    assert!(
        differs.contains(r#"["left"]"#) && differs.contains(r#"["owned"]"#),
        "both values are named: {differs}"
    );
    assert!(differs.contains("keep one of the two"), "{differs}");
    assert_eq!(report.warnings.len(), 3, "{:?}", report.warnings);
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn the_local_marketplace_is_the_project_itself() -> Result<()> {
    let root = scratch("authoring-local");
    // A project: its own repository is the marketplace's repository.
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    let (root, plugins) =
        scaffold_local_marketplace("tools", Some("Project tools"), &project, "plugins")?;
    assert!(root.join("marketplace.json").is_file());
    assert!(plugins.is_dir());

    // No Git was touched by the scaffold: the project's repository is the
    // identity, and the commit is the project's own flow's to make — the
    // unborn-HEAD state a fresh fixture starts in stays as it was.
    let manifest = fs::read_to_string(project.join("marketplace.json")).unwrap();
    assert!(manifest.contains("\"name\": \"tools\""));
    let no_commit = Command::new("git")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("-C")
        .arg(&project)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap();
    assert!(
        !no_commit.status.success(),
        "the scaffold fabricated a commit; the project's flow owns it"
    );

    // A project that already is a marketplace is refused, saying so.
    assert!(
        scaffold_local_marketplace("other", None, &project, "plugins").is_err(),
        "a second manifest would be two marketplaces in one repository"
    );

    // The renameable plugins directory: same manifest rule, other folder.
    let other = scratch("authoring-local-rename");
    let project_two = other.join("project");
    fs::create_dir_all(project_two.join(".git")).unwrap();
    let (_, plugins) = scaffold_local_marketplace("tools", None, &project_two, "tools-plugins")?;
    assert_eq!(plugins, project_two.join("tools-plugins"));
    fs::remove_dir_all(other).expect("teardown");
    fs::remove_dir_all(root).expect("teardown");
    Ok(())
}

#[test]
fn create_refuses_to_collide() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-refusal");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    // The same name again from a different directory is refused only at the
    // registration layer (the machine registry owns the name); what the
    // scaffold itself refuses is a target that already holds something.
    let occupied = root.join("occupied");
    fs::create_dir_all(occupied.join("something")).unwrap();
    assert!(scaffold_marketplace("other", None, &occupied).is_err());
    assert!(
        occupied.join("something").is_dir(),
        "the refusal wrote nothing into the directory"
    );

    // An existing plugin is refused, never overwritten.
    scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;
    assert!(
        scaffold_plugin(
            &market,
            "greet",
            None,
            None,
            &ScaffoldCapabilities::default()
        )
        .is_err()
    );
    // A name outside the rule is refused by the same rule an id is held to,
    // and told the name it meant.
    assert!(
        scaffold_plugin(
            &market,
            "-flag",
            None,
            None,
            &ScaffoldCapabilities::default()
        )
        .is_err()
    );
    let refused = scaffold_plugin(
        &market,
        "My_Plugin",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("try `my-plugin`"), "{refused}");
    assert!(!market.join("plugins/My_Plugin").exists());
    let refused = scaffold_marketplace("My Market", None, &root.join("named"))
        .unwrap_err()
        .to_string();
    assert!(refused.contains("try `my-market`"), "{refused}");
    assert!(!root.join("named").exists());
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn check_reports_what_install_would_refuse() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-check-fail");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;

    // A name the PackageId rule refuses is named before any install ran.
    fs::write(
        plugin.join("plugin.json"),
        r#"{ "name": "-flag", "description": "x" }"#,
    )
    .unwrap();
    let report = check_plugin(&plugin)?;
    assert!(!report.is_clean(), "an invalid name must be a finding");
    assert!(report.findings.iter().any(|f| f.contains("name")));
    fs::write(
        plugin.join("plugin.json"),
        r#"{ "name": "Greet", "description": "x" }"#,
    )
    .unwrap();
    let report = check_plugin(&plugin)?;
    assert!(
        report.findings.iter().any(|f| f.contains("try `greet`")),
        "an uppercase name is told the name it meant: {:?}",
        report.findings
    );
    fs::write(
        plugin.join("plugin.json"),
        r#"{ "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json", "name": "greet", "description": "x" }"#,
    )
    .unwrap();

    // A marketplace entry pointing outside itself is located by name.
    fs::write(
        root.join("market/marketplace.json"),
        r#"{ "name": "tools", "plugins": [ { "name": "escape", "source": "../outside" } ] }"#,
    )
    .unwrap();
    let market_report = check_marketplace(&root.join("market"))?;
    assert!(
        market_report
            .findings
            .iter()
            .any(|finding| finding.starts_with("escape")),
        "the finding locates the plugin that escapes: {:?}",
        market_report.findings
    );

    // A marketplace name and an entry name are held to the rule too.
    fs::write(
        root.join("market/marketplace.json"),
        r#"{ "name": "Tools", "plugins": [] }"#,
    )
    .unwrap();
    let market_report = check_marketplace(&root.join("market"))?;
    assert!(
        market_report
            .findings
            .iter()
            .any(|finding| finding.contains("`Tools`") && finding.contains("try `tools`")),
        "{:?}",
        market_report.findings
    );
    fs::write(
        root.join("market/marketplace.json"),
        r#"{ "name": "tools", "plugins": [ { "name": "Greet", "source": "./plugins/greet" } ] }"#,
    )
    .unwrap();
    let market_report = check_marketplace(&root.join("market"))?;
    assert!(
        market_report
            .findings
            .iter()
            .any(|finding| finding.starts_with("Greet") && finding.contains("try `greet`")),
        "{:?}",
        market_report.findings
    );
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn the_marketplace_check_covers_its_plugins() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-market-check");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities {
            hook: true,
            ..ScaffoldCapabilities::default()
        },
    )?;
    // A hook manifest violating the handler contract (out-of-bounds timeout)
    // is a finding located in that plugin.
    fs::write(
        plugin.join("hooks.json"),
        r#"{ "hooks": { "PreToolUse": [ { "id": "x", "matcher": "shell", "effect": "deny", "hooks": [ { "type": "command", "command": "${PLUGIN_ROOT}/scripts/guard", "timeout": 301 } ] } ] } }"#,
    )
    .unwrap();
    let report = check_marketplace(&market)?;
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.starts_with("greet") && finding.contains("between 1 and 300")),
        "an out-of-bounds handler timeout is found in its plugin, not delivered: {:?}",
        report.findings
    );
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn an_empty_at_directory_is_accepted() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-empty-at");
    let at = root.join("market");
    fs::create_dir_all(&at).unwrap();
    scaffold_marketplace("tools", None, &at)?;
    assert!(at.join("marketplace.json").is_file());
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn a_missing_git_identity_is_refused_before_any_write() {
    let mut environment = uze_testkit::env::scope();
    let empty = scratch("authoring-no-identity-config").join("gitconfig");
    fs::write(&empty, "").unwrap();
    environment.set("GIT_CONFIG_GLOBAL", &empty);
    environment.set("GIT_CONFIG_SYSTEM", &empty);
    let root = scratch("authoring-no-identity");
    let at = root.join("market");

    let refused = scaffold_marketplace("tools", None, &at);

    assert!(refused.is_err(), "no identity, no commit, no scaffold");
    assert!(!at.exists(), "the refusal wrote nothing");
    fs::remove_dir_all(&root).expect("teardown");
}

#[test]
fn a_name_the_manifest_already_carries_is_refused_before_any_write() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-ghost");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    fs::write(
        market.join("marketplace.json"),
        r#"{ "name": "tools", "plugins": [ { "name": "ghost", "source": "./elsewhere" } ] }"#,
    )
    .unwrap();

    let refused = scaffold_plugin(
        &market,
        "ghost",
        None,
        None,
        &ScaffoldCapabilities::default(),
    );

    assert!(refused.is_err());
    assert!(
        !market.join("plugins/ghost").exists(),
        "the refusal wrote nothing"
    );
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

/// The plugins directory a local marketplace chose is where every later
/// plugin goes — recorded in the manifest while it has no entry to say so.
#[test]
fn a_plugin_goes_where_the_marketplace_keeps_its_plugins() -> Result<()> {
    let root = scratch("authoring-plugins-dir");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    scaffold_local_marketplace("tools", None, &project, "tools-plugins")?;

    let first = scaffold_plugin(
        &project,
        "greet",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;
    assert_eq!(first, project.join("tools-plugins/greet"));

    // With an entry to read, the recorded key is no longer the only answer.
    let mut manifest = read_json(&project.join("marketplace.json"))?;
    manifest
        .as_object_mut()
        .unwrap()
        .remove(PLUGINS_DIRECTORY_KEY);
    write_json(&project.join("marketplace.json"), &manifest)?;
    let second = scaffold_plugin(
        &project,
        "wave",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;
    assert_eq!(second, project.join("tools-plugins/wave"));

    let report = check_marketplace(&project)?;
    assert!(report.is_clean(), "{:?}", report.findings);
    assert_eq!(report.delivers, ["greet", "wave"]);
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn check_reports_a_skill_a_harness_could_not_read() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-check-skill");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;
    let skill = plugin.join("skills/greet/SKILL.md");

    for (body, fault) in [
        ("Just prose, no frontmatter.\n", "no frontmatter"),
        (
            "---\nname: greet\ndescription: Deploys: things fast\n---\nbody\n",
            "not valid YAML",
        ),
        ("---\nname: greet\n---\nbody\n", "no `description`"),
        ("---\ndescription: x\n---\nbody\n", "no `name`"),
        (
            "---\nname: Greet_Skill\ndescription: x\n---\nbody\n",
            "try `greet-skill`",
        ),
        (
            "---\nname: hello\ndescription: x\n---\nbody\n",
            "differs from its directory `greet`",
        ),
    ] {
        fs::write(&skill, body).unwrap();
        let report = check_plugin(&plugin)?;
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.contains("SKILL.md") && finding.contains(fault)),
            "{fault}: {:?}",
            report.findings
        );
    }
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn check_reports_a_reference_outside_the_plugin() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-check-escape");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities {
            hook: true,
            mcp: true,
            ..ScaffoldCapabilities::default()
        },
    )?;

    for (file, body, reference) in [
        (
            "hooks.json",
            r#"{ "hooks": { "PreToolUse": [ { "id": "x", "matcher": "shell", "effect": "deny", "hooks": [ { "type": "command", "command": "${PLUGIN_ROOT}/../guard" } ] } ] } }"#,
            "${PLUGIN_ROOT}/../guard",
        ),
        (
            "hooks.json",
            r#"{ "hooks": { "PreToolUse": [ { "id": "x", "matcher": "shell", "effect": "deny", "hooks": [ { "type": "command", "command": "sh /opt/guard" } ] } ] } }"#,
            "/opt/guard",
        ),
        (
            "mcp.json",
            r#"{ "mcpServers": { "example": { "command": "python3", "args": ["../server.py"] } } }"#,
            "../server.py",
        ),
    ] {
        let original = fs::read(plugin.join(file)).unwrap();
        fs::write(plugin.join(file), body).unwrap();
        let report = check_plugin(&plugin)?;
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.contains(file) && finding.contains(reference)),
            "{reference}: {:?}",
            report.findings
        );
        fs::write(plugin.join(file), original).unwrap();
    }
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

// A symbolic link, which Windows lets an ordinary account make only in developer mode.
#[cfg(unix)]
#[test]
fn check_reports_a_link_install_would_refuse() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-check-link");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;
    std::os::unix::fs::symlink("/etc", plugin.join("escape")).unwrap();

    let report = check_plugin(&plugin)?;

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.contains("not self-contained")),
        "{:?}",
        report.findings
    );
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn a_manifest_of_the_wrong_shape_is_refused_before_any_write() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-shape");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    for manifest in ["[]", r#"{ "name": "tools", "plugins": {} }"#] {
        fs::write(market.join("marketplace.json"), manifest).unwrap();

        let refused = scaffold_plugin(
            &market,
            "greet",
            None,
            None,
            &ScaffoldCapabilities::default(),
        );

        assert!(
            matches!(refused, Err(UzeError::MarketplaceScaffold(_))),
            "{manifest}: {refused:?}"
        );
        assert!(
            !market.join("plugins/greet").exists(),
            "the refusal wrote nothing"
        );
    }
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

#[test]
fn check_names_an_agent_a_harness_would_drop_or_rename() -> Result<()> {
    let _git_identity = git_identity();
    let root = scratch("authoring-agent-faults");
    let market = scaffold_marketplace("tools", None, &root.join("market"))?;
    let plugin = scaffold_plugin(
        &market,
        "greet",
        None,
        None,
        &ScaffoldCapabilities::default(),
    )?;
    fs::create_dir_all(plugin.join("agents/Review")).unwrap();
    fs::write(plugin.join("agents/bare.md"), "No frontmatter at all.\n").unwrap();
    fs::write(
        plugin.join("agents/Review/audit.md"),
        "---\nname: audit\n---\nBody.\n",
    )
    .unwrap();

    let report = check_plugin(&plugin)?;
    let bare = report
        .findings
        .iter()
        .find(|finding| finding.contains(&native("agents/bare.md")))
        .expect("an agent without frontmatter is reported");
    assert!(bare.contains("no frontmatter"), "{bare}");
    let nested: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.contains(&native("agents/Review/audit.md")))
        .collect();
    assert!(
        nested
            .iter()
            .any(|finding| finding.contains("no `description`")),
        "{nested:?}"
    );
    assert!(
        nested
            .iter()
            .any(|finding| finding.contains("`Review` in the agent's label `Review:audit`")),
        "{nested:?}"
    );
    Ok(())
}

/// A package uze installs today but a client of Agent Plugins 1.0 would
/// refuse or read short: every divergence is named, and none of them is a
/// finding, because uze's own format decides what installs.
#[test]
fn check_names_what_keeps_a_package_from_agent_plugins_without_refusing_it() -> Result<()> {
    let root = scratch("authoring-check-agent-plugins");
    let plugin = root.join("legacy");
    fs::create_dir_all(plugin.join("skills/review/deep/nested")).unwrap();
    fs::write(
        plugin.join("plugin.json"),
        r#"{"name":"legacy","author":"me","skills":"./skills","extensions":{"sh.uze":{"future":true,"requirements":[{"executable":"jq"}]}}}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Reviews\n---\nbody\n",
    )
    .unwrap();
    fs::write(
        plugin.join("skills/review/deep/nested/SKILL.md"),
        "---\nname: nested\ndescription: Nested\n---\nbody\n",
    )
    .unwrap();
    fs::write(
        plugin.join("mcp.json"),
        r#"{"//":"notes","mcpServers":{"s":{"command":"${PLUGIN_ROOT}/bin/s","args":["${PLUGIN_DATA}/x"]}}}"#,
    )
    .unwrap();

    let report = check_plugin(&plugin)?;
    assert!(report.is_clean(), "{:?}", report.findings);
    let standard = report.agent_plugins.expect("judged");
    assert!(!standard.conformant);
    let nested = format!(
        "{}: the standard discovers only `skills/<name>/SKILL.md`",
        native("deep/nested/SKILL.md")
    );
    for expected in [
        "plugin.json: no `$schema`",
        "plugin.json: `author` must be an object",
        "plugin.json: `skills` is not a manifest field",
        nested.as_str(),
        "mcp.json: no `$schema`",
        "mcp.json: `//` is not allowed",
        "server `s` has no `type`",
        "server `s` names `${PLUGIN_ROOT}/bin/s`",
    ] {
        assert!(
            standard.divergences.iter().any(|d| d.contains(expected)),
            "{expected}: {:?}",
            standard.divergences
        );
    }
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("requirements")),
        "`requirements` is a setting uze reads: {:?}",
        report.warnings
    );
    for expected in [
        "`extensions[\"sh.uze\"].future` is not a setting uze reads",
        "uze does not provide `${PLUGIN_DATA}` yet",
    ] {
        assert!(
            report.warnings.iter().any(|w| w.contains(expected)),
            "{expected}: {:?}",
            report.warnings
        );
    }
    fs::remove_dir_all(&root).expect("teardown");
    Ok(())
}

/// A handler written for one shell is reported before anything is
/// installed: on Windows a guard with no `windows` spelling refuses the
/// package, and an observing group is left out.
#[test]
fn check_names_a_handler_with_no_windows_spelling() {
    let market = scratch("check-windows-spelling");
    scaffold_marketplace("tools", None, &market).unwrap_or_else(|_| {
        // No Git identity here: the market directory alone is enough.
        fs::create_dir_all(market.join("plugins")).unwrap();
        market.clone()
    });
    let plugin = market.join("plugins/guarded");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("plugin.json"),
        r#"{"name":"guarded","version":"1.0.0","description":"Guarded"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("hooks.json"),
        r#"{"hooks":{"PreToolUse":[
            {"id":"guard","matcher":"shell","effect":"deny","hooks":[{"type":"command","command":"check"}]},
            {"id":"both","matcher":"shell","effect":"deny","hooks":[{"type":"command","command":{"posix":"check","windows":"check"}}]}
        ]}}"#,
    )
    .unwrap();

    let report = check_plugin(&plugin).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("hook `guard`")
                && warning.contains("no `windows` spelling")),
        "{:?}",
        report.warnings
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("hook `both`") && warning.contains("no `windows`")),
        "{:?}",
        report.warnings
    );
    let _ = fs::remove_dir_all(market);
}

/// An exec-form script is checked against both launcher tables before
/// anything is installed: a `.sh` script starts nowhere on Windows, and a
/// `.py` one starts everywhere.
#[test]
fn check_names_an_exec_form_script_a_platform_cannot_start() {
    let plugin = scratch("check-exec-form").join("plugins/guarded");
    fs::create_dir_all(plugin.join("hooks")).unwrap();
    fs::write(
        plugin.join("plugin.json"),
        r#"{"name":"guarded","version":"1.0.0","description":"Guarded"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("hooks.json"),
        r#"{"hooks":{"PreToolUse":[
            {"id":"shell-script","matcher":"shell","effect":"deny","hooks":[{"type":"command","command":"hooks/guard.sh","args":[]}]},
            {"id":"python-script","matcher":"shell","effect":"deny","hooks":[{"type":"command","command":"hooks/guard.py","args":["--strict"]}]}
        ]}}"#,
    )
    .unwrap();

    let report = check_plugin(&plugin).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("hook `shell-script`")
                && warning.contains("cannot run on Windows")
                && warning.contains("installing the package is refused there")),
        "{:?}",
        report.warnings
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("hook `python-script`")),
        "{:?}",
        report.warnings
    );
    let _ = fs::remove_dir_all(plugin.parent().unwrap().parent().unwrap());
}
