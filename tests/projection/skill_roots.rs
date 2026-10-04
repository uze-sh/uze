//! Skill roots: every loose Skill is a real directory in its own harness's
//! root, with one owner. Codex keeps `~/.agents/skills`, OpenCode its own
//! `~/.config/opencode/skills`; the supporting files are copies; an edit is
//! drift that UZE leaves alone; and an entry an earlier build linked into
//! the generated tier is replaced.

use std::{
    fs,
    path::{Path, PathBuf},
};

use uze_application::UzeApplication;
use uze_core::{
    PackageSource, UzeHome,
    capability::Resource as ProjectResource,
    exposure::{ExposurePlan, PackageExposurePlan},
    integration::{AttachmentInspection, AttachmentReceipt, HarnessDetection, IntegrationPort},
    provisioning::{ProcessResult, ProcessRunner, ProcessSpec},
    router::HarnessCapabilities,
    store::StoredPackage,
};
use uze_integrations::{codex::CodexIntegration, opencode::OpenCodeIntegration};

use uze_testkit::temp::scratch;

fn temp(label: &str) -> PathBuf {
    scratch(label)
}

fn user_only_body(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Explicit user action\ninvoke:\n  model: false\n  user: true\n---\n\nExplicit body.\n"
    )
}

/// Never spawns a process; the fake Codex (below) stands in only when a
/// generated envelope's `attach_package` shells out.
struct NoopProcessRunner;

impl ProcessRunner for NoopProcessRunner {
    fn run(&self, _spec: &ProcessSpec) -> uze_core::Result<ProcessResult> {
        Ok(ProcessResult {
            success: true,
            timed_out: false,
        })
    }
}

fn always_present<T: IntegrationPort>(integration: T) -> AlwaysPresent<T> {
    AlwaysPresent(integration)
}

struct AlwaysPresent<T: IntegrationPort>(T);

impl<T: IntegrationPort> IntegrationPort for AlwaysPresent<T> {
    fn id(&self) -> &'static str {
        self.0.id()
    }
    fn capabilities(&self) -> HarnessCapabilities {
        self.0.capabilities()
    }
    fn exposure_plan(&self, resource: &ProjectResource) -> ExposurePlan {
        self.0.exposure_plan(resource)
    }
    fn exposure_name_candidates(&self, resource: &ProjectResource) -> Vec<String> {
        self.0.exposure_name_candidates(resource)
    }
    fn skill_discovery_root(&self) -> Option<PathBuf> {
        self.0.skill_discovery_root()
    }
    fn package_exposure_plan(
        &self,
        package: &StoredPackage,
        resources: &[&ProjectResource],
    ) -> Option<PackageExposurePlan> {
        self.0.package_exposure_plan(package, resources)
    }
    fn detect(&self) -> HarnessDetection {
        HarnessDetection {
            present: true,
            version: Some("9.9.9".to_owned()),
        }
    }
    fn provision(
        &self,
        _runner: &dyn ProcessRunner,
    ) -> uze_core::Result<uze_core::provisioning::ProvisioningResult> {
        Ok(uze_core::provisioning::ProvisioningResult::verified(
            uze_core::provisioning::ProvisionAction::None,
            "test-always-present",
            self.detect(),
        ))
    }
    fn install(&self, home: &UzeHome, detection: &HarnessDetection) -> uze_core::Result<()> {
        self.0.install(home, detection)
    }
    fn status(&self, home: &UzeHome) -> uze_core::integration::IntegrationStatus {
        self.0.status(home)
    }
    fn attach(
        &self,
        resource: &ProjectResource,
    ) -> uze_core::Result<Option<uze_core::integration::ManagedArtifact>> {
        self.0.attach(resource)
    }
    fn attach_package(
        &self,
        package: &StoredPackage,
        plan: &PackageExposurePlan,
    ) -> uze_core::Result<Option<AttachmentReceipt>> {
        self.0.attach_package(package, plan)
    }
    fn attach_receipt(
        &self,
        resource: &ProjectResource,
    ) -> uze_core::Result<Option<AttachmentReceipt>> {
        self.0.attach_receipt(resource)
    }
    fn inspect_receipt(&self, receipt: &AttachmentReceipt) -> AttachmentInspection {
        self.0.inspect_receipt(receipt)
    }
    fn detach_receipt(
        &self,
        receipt: &AttachmentReceipt,
    ) -> uze_core::Result<AttachmentInspection> {
        self.0.detach_receipt(receipt)
    }
}

/// A fake `codex` that answers every plugin CLI call successfully.
fn fake_codex_bin_dir(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = root.join("fake-bin");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("codex");
    fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    dir
}

fn with_fake_codex(root: &Path, f: impl FnOnce()) {
    let fake_bin = fake_codex_bin_dir(root);
    let mut scope = uze_testkit::env::scope();
    scope.set("PATH", uze_testkit::temp::path_prefixed(&fake_bin));
    f();
}

/// A `codex` fake that answers the two inspection commands with caller-built
/// JSON (via env vars) so integration-owned package receipts can be
/// inspected and detached truthfully in tests that exercise per-integration
/// detach/update.
fn with_truthful_fake_codex(
    root: &Path,
    marketplace_json: &str,
    plugins_json: &str,
    f: impl FnOnce(),
) {
    use std::os::unix::fs::PermissionsExt;
    let dir = root.join("fake-bin");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("codex");
    fs::write(
        &path,
        "#!/bin/sh\ncase \"$1 $2 $3\" in\n  \"plugin marketplace list\") echo \"$FAKE_CODEX_MARKETPLACES\" ;;\n  \"plugin list --json\") echo \"$FAKE_CODEX_PLUGINS\" ;;\n  *) exit 0 ;;\nesac\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    let mut scope = uze_testkit::env::scope();
    scope.set("PATH", uze_testkit::temp::path_prefixed(&dir));
    scope.set("FAKE_CODEX_MARKETPLACES", marketplace_json);
    scope.set("FAKE_CODEX_PLUGINS", plugins_json);
    f();
}

/// Codex and OpenCode against one machine: the OpenCode config file sits
/// at `<root>/opencode/opencode.json`, so its skills root is
/// `<root>/opencode/skills`.
fn codex_and_opencode(root: &Path) -> (UzeApplication, PathBuf, PathBuf, UzeHome) {
    let agents_home = root.join("agents-home");
    let uze_home = UzeHome::at(root.join("uze-home"));
    let application = UzeApplication::new_with_runner(
        uze_home.clone(),
        vec![
            Box::new(always_present(CodexIntegration::new(
                agents_home.clone(),
                uze_home.clone(),
            ))),
            Box::new(always_present(OpenCodeIntegration::new(
                agents_home.clone(),
                root.join("opencode/opencode.json"),
                uze_home.clone(),
            ))),
        ],
        Box::new(NoopProcessRunner),
    );
    (
        application,
        agents_home,
        root.join("opencode/skills"),
        uze_home,
    )
}

/// A `flow` package whose `review` skill is user-only and ships a script
/// and a reference beside its `SKILL.md`.
fn review_fixture(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let fixture_root = root.join("fixture");
    let skill = fixture_root.join("skills/review");
    fs::create_dir_all(skill.join("scripts")).unwrap();
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("SKILL.md"), user_only_body("review")).unwrap();
    fs::write(skill.join("references/guide.md"), "Guide.\n").unwrap();
    let script = skill.join("scripts/check.sh");
    fs::write(&script, "#!/bin/sh\necho ok\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        fixture_root.join("plugin.json"),
        r#"{"name":"flow","version":"1.0.0","description":"review fixture"}"#,
    )
    .unwrap();
    fixture_root
}

fn links_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_type = entry.file_type().unwrap();
            if file_type.is_symlink() {
                found.push(entry.path());
            } else if file_type.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    found
}

#[test]
fn opencode_gets_a_directory_of_its_own_with_its_supporting_files_copied() {
    use std::os::unix::fs::PermissionsExt;
    let root = temp("skill-root-opencode");
    with_fake_codex(&root, || {
        let (application, agents_home, opencode_skills, _) = codex_and_opencode(&root);
        application
            .plugins()
            .add(
                PackageSource::local(review_fixture(&root)),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("the skill installs for both harnesses");

        let entry = opencode_skills.join(uze_core::path::file_name_for("flow:review"));
        assert!(entry.is_dir() && !entry.is_symlink(), "a real directory");
        let skill = fs::read_to_string(entry.join("SKILL.md")).unwrap();
        assert!(skill.contains("opencode/autoinvoke: false"), "{skill}");
        assert!(
            !entry.join("agents/openai.yaml").exists(),
            "OpenCode's root carries OpenCode's encoding only"
        );
        assert_eq!(
            fs::read_to_string(entry.join("references/guide.md")).unwrap(),
            "Guide.\n"
        );
        let script = entry.join("scripts/check.sh");
        assert!(script.is_file() && !script.is_symlink());
        assert_ne!(
            fs::metadata(&script).unwrap().permissions().mode() & 0o111,
            0,
            "a copied script stays executable"
        );
        assert!(links_under(&opencode_skills).is_empty());
        assert!(
            !agents_home
                .join("skills")
                .join(uze_core::path::file_name_for("flow:review"))
                .exists(),
            "Codex's plugin covers the skill: nothing is written to its loose root"
        );
    });
    fs::remove_dir_all(&root).ok();
}

#[test]
fn codex_alone_keeps_the_user_only_skill_hidden_from_the_model() {
    let root = temp("skill-root-codex-only");
    with_fake_codex(&root, || {
        let agents_home = root.join("agents-home");
        let uze_home = UzeHome::at(root.join("uze-home"));
        let application = UzeApplication::new_with_runner(
            uze_home.clone(),
            vec![Box::new(always_present(CodexIntegration::new(
                agents_home.clone(),
                uze_home.clone(),
            )))],
            Box::new(NoopProcessRunner),
        );
        let fixture_root = review_fixture(&root);
        let store_bytes = fs::read(fixture_root.join("skills/review/SKILL.md")).unwrap();
        application
            .plugins()
            .add(
                PackageSource::local(&fixture_root),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("Codex-only user-only Skill installs");
        let envelope_skill = uze_home
            .runtime_dir()
            .join("attachments/codex/generated/flow@local/skills/review");
        assert_eq!(
            fs::read_to_string(envelope_skill.join("agents/openai.yaml")).unwrap(),
            "policy:\n  allow_implicit_invocation: false\n",
        );
        assert!(links_under(&envelope_skill).is_empty());
        assert_eq!(
            fs::read(fixture_root.join("skills/review/SKILL.md")).unwrap(),
            store_bytes,
            "Store bytes are never rewritten"
        );
    });
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_update_rebuilds_the_directory_and_the_receipt_still_matches() {
    let root = temp("skill-root-update");
    let (application, _, opencode_skills, uze_home) = codex_and_opencode(&root);
    let generated_root = uze_home.runtime_dir().join("attachments/codex/generated");
    let marketplaces = format!(
        r#"{{"marketplaces":[{{"name":"uze-store","root":"{}"}}]}}"#,
        generated_root.display()
    );
    let plugins = format!(
        r#"{{"installed":[{{"pluginId":"flow@uze-store","enabled":true,"installed":true,"marketplaceName":"uze-store","path":"{}"}}]}}"#,
        generated_root.join("flow@local").display()
    );
    with_truthful_fake_codex(&root, &marketplaces, &plugins, || {
        application
            .plugins()
            .add(
                PackageSource::local(review_fixture(&root)),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("initial install");
        let before = fs::read(
            opencode_skills
                .join(uze_core::path::file_name_for("flow:review"))
                .join("SKILL.md"),
        )
        .unwrap();
        application
            .plugins()
            .update("flow", &uze_core::trust::AlwaysTrust)
            .expect("update succeeds");
        assert_eq!(
            fs::read(
                opencode_skills
                    .join(uze_core::path::file_name_for("flow:review"))
                    .join("SKILL.md")
            )
            .unwrap(),
            before
        );
        let receipt = uze_core::state::receipts(&uze_home, Some("flow@local"))
            .unwrap()
            .into_iter()
            .find(|r| r.integration == "opencode")
            .unwrap();
        assert!(matches!(
            receipt.artifact,
            uze_core::integration::ManagedArtifact::GeneratedTree { .. }
        ));
        assert!(receipt.artifact.is_in_place());
    });
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_edited_skill_is_drift_and_is_left_as_the_operator_left_it() {
    let root = temp("skill-root-drift");
    with_fake_codex(&root, || {
        let (application, _, opencode_skills, uze_home) = codex_and_opencode(&root);
        application
            .plugins()
            .add(
                PackageSource::local(review_fixture(&root)),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("initial install");
        let skill = opencode_skills
            .join(uze_core::path::file_name_for("flow:review"))
            .join("SKILL.md");
        fs::write(&skill, "edited by hand\n").unwrap();

        let receipt = uze_core::state::receipts(&uze_home, Some("flow@local"))
            .unwrap()
            .into_iter()
            .find(|r| r.integration == "opencode")
            .unwrap();
        assert_eq!(
            receipt.artifact.inspect_standard().state,
            uze_core::integration::AttachmentState::Drifted
        );

        let report = application
            .plugins()
            .update("flow", &uze_core::trust::AlwaysTrust)
            .expect("one drifted skill does not fail the update");
        assert_eq!(fs::read_to_string(&skill).unwrap(), "edited by hand\n");
        let _ = report;
    });
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_entry_an_earlier_build_linked_is_replaced_by_a_directory() {
    let root = temp("skill-root-upgrade");
    with_fake_codex(&root, || {
        let (application, agents_home, opencode_skills, uze_home) = codex_and_opencode(&root);
        let fixture = review_fixture(&root);
        application
            .plugins()
            .add(
                PackageSource::local(&fixture),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("initial install");

        // What beta.3 left: OpenCode and Codex both holding one link in
        // `~/.agents/skills`, into a wrapper in the generated tier.
        let current = uze_core::state::receipts(&uze_home, Some("flow@local"))
            .unwrap()
            .into_iter()
            .find(|r| r.integration == "opencode")
            .unwrap();
        fs::remove_dir_all(opencode_skills.join(uze_core::path::file_name_for("flow:review")))
            .unwrap();
        let wrapper = uze_home
            .runtime_dir()
            .join("attachments/opencode/skills/flow/review");
        fs::create_dir_all(&wrapper).unwrap();
        fs::write(wrapper.join("SKILL.md"), "old wrapper\n").unwrap();
        let link = agents_home
            .join("skills")
            .join(uze_core::path::file_name_for("flow:review"));
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&wrapper, &link).unwrap();
        uze_core::state::forget_receipt(&uze_home, &current).unwrap();
        for integration in ["opencode", "codex"] {
            let mut old = current.clone();
            old.integration = integration.to_owned();
            old.artifact = uze_core::integration::ManagedArtifact::SymlinkReference {
                path: link.clone(),
                target: wrapper.clone(),
            };
            uze_core::state::record_receipt(&uze_home, old).unwrap();
        }

        application
            .plugins()
            .add(
                PackageSource::local(&fixture),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("the install after an upgrade succeeds");

        assert!(!link.exists() && !link.is_symlink(), "the old link is gone");
        assert!(
            opencode_skills
                .join(uze_core::path::file_name_for("flow:review"))
                .join("SKILL.md")
                .is_file()
        );
        let receipts = uze_core::state::receipts(&uze_home, Some("flow@local")).unwrap();
        assert!(
            receipts.iter().all(|r| !matches!(
                r.artifact,
                uze_core::integration::ManagedArtifact::SymlinkReference { .. }
            )),
            "no receipt in the earlier shape is left: {receipts:#?}"
        );
    });
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_name_somebody_else_holds_blocks_its_own_capability_and_no_other() {
    let root = temp("skill-root-blocked-one");
    with_fake_codex(&root, || {
        let (application, _, opencode_skills, _) = codex_and_opencode(&root);
        let fixture_root = root.join("two-skill-fixture");
        for skill in ["review", "commit"] {
            fs::create_dir_all(fixture_root.join(format!("skills/{skill}"))).unwrap();
            fs::write(
                fixture_root.join(format!("skills/{skill}/SKILL.md")),
                user_only_body(skill),
            )
            .unwrap();
        }
        fs::write(
            fixture_root.join("plugin.json"),
            r#"{"name":"flow","description":"two-skill fixture"}"#,
        )
        .unwrap();

        // Somebody else's directory at the name `review` needs.
        let theirs = opencode_skills.join(uze_core::path::file_name_for("flow:review"));
        fs::create_dir_all(&theirs).unwrap();
        fs::write(theirs.join("SKILL.md"), "theirs\n").unwrap();

        let report = application
            .plugins()
            .add(
                PackageSource::local(&fixture_root),
                &uze_core::trust::AlwaysTrust,
            )
            .expect("one held name does not fail the install");

        assert_eq!(
            fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
            "theirs\n",
            "the entry UZE does not own is untouched"
        );
        assert!(
            opencode_skills
                .join(uze_core::path::file_name_for("flow:commit"))
                .join("SKILL.md")
                .is_file(),
            "the capability whose name was free is delivered"
        );
        assert!(
            report
                .blocked
                .iter()
                .any(|one| one.capability.contains("review")),
            "the refusal names the capability, got {:?}",
            report.blocked
        );
        assert!(
            report
                .blocked
                .iter()
                .all(|one| !one.capability.contains("commit")),
            "nothing else is reported blocked"
        );
    });
    fs::remove_dir_all(&root).ok();
}

/// `${PLUGIN_ROOT}` in a delivered skill names the whole package, copied
/// out of the Store: the file it points at exists, and a write through it —
/// a hook building into its root — leaves the bytes the lock pins alone.
#[test]
fn the_plugin_root_a_skill_names_is_a_delivered_copy_never_the_store() {
    let root = temp("skill-root-plugin-root");
    with_fake_codex(&root, || {
        let (application, _, opencode_skills, uze_home) = codex_and_opencode(&root);
        let fixture = review_fixture(&root);
        fs::create_dir_all(fixture.join("phases")).unwrap();
        fs::write(fixture.join("phases/plan.md"), "Plan.\n").unwrap();
        fs::create_dir_all(fixture.join("skills/run")).unwrap();
        fs::write(
            fixture.join("skills/run/SKILL.md"),
            "---\nname: run\ndescription: Runs the plan\n---\nRead ${PLUGIN_ROOT}/phases/plan.md\n",
        )
        .unwrap();
        application
            .plugins()
            .add(PackageSource::local(fixture), &uze_core::trust::AlwaysTrust)
            .expect("installs");

        let skill = fs::read_to_string(
            opencode_skills
                .join(uze_core::path::file_name_for("flow:run"))
                .join("SKILL.md"),
        )
        .unwrap();
        let named = skill
            .lines()
            .find_map(|line| line.strip_prefix("Read "))
            .map(PathBuf::from)
            .expect("the body names the plan");
        assert!(
            named.starts_with(uze_home.runtime_dir()),
            "{}",
            named.display()
        );
        assert!(
            !named.starts_with(uze_home.store_dir()),
            "{}",
            named.display()
        );
        assert_eq!(fs::read_to_string(&named).unwrap(), "Plan.\n");

        fs::write(&named, "built over\n").unwrap();
        let stored = uze_home.plugins_dir().join("local/flow/phases/plan.md");
        assert_eq!(fs::read_to_string(stored).unwrap(), "Plan.\n");
    });
    fs::remove_dir_all(&root).ok();
}

/// An MCP entry an earlier build wrote with the Store as `${PLUGIN_ROOT}`
/// is UZE's own, by its receipt: the next install replaces it with the
/// delivered root rather than refusing it as somebody else's.
#[test]
fn an_mcp_entry_an_earlier_build_rooted_in_the_store_is_restated() {
    let root = temp("skill-root-mcp-restated");
    with_fake_codex(&root, || {
        let (application, _, _, uze_home) = codex_and_opencode(&root);
        let fixture = root.join("mcp-fixture");
        fs::create_dir_all(fixture.join("bin")).unwrap();
        fs::write(
            fixture.join("plugin.json"),
            r#"{"name":"served","version":"1.0.0","description":"mcp fixture"}"#,
        )
        .unwrap();
        fs::write(
            fixture.join("mcp.json"),
            r#"{"mcpServers":{"rtc":{"type":"stdio","command":"${PLUGIN_ROOT}/bin/server","args":[]}}}"#,
        )
        .unwrap();
        fs::write(fixture.join("bin/server"), "#!/bin/sh\n").unwrap();
        let install = || {
            application
                .plugins()
                .add(
                    PackageSource::local(fixture.clone()),
                    &uze_core::trust::AlwaysTrust,
                )
                .expect("installs")
        };
        install();

        let config_path = root.join("opencode/opencode.json");
        let receipt = uze_core::state::receipts(&uze_home, Some("served@local"))
            .unwrap()
            .into_iter()
            .find(|r| {
                r.integration == "opencode"
                    && matches!(
                        r.artifact,
                        uze_core::integration::ManagedArtifact::VendorConfigEntry { .. }
                    )
            })
            .expect("OpenCode holds the server as a config entry");
        let delivered = uze_home
            .runtime_dir()
            .join("packages/served@local")
            .to_string_lossy()
            .into_owned();
        let stored = uze_home
            .plugins_dir()
            .join("local/served")
            .to_string_lossy()
            .into_owned();
        let config = fs::read_to_string(&config_path).unwrap();
        assert!(config.contains(&delivered), "{config}");
        fs::write(&config_path, config.replace(&delivered, &stored)).unwrap();
        let mut earlier = receipt.clone();
        if let uze_core::integration::ManagedArtifact::VendorConfigEntry { command, .. } =
            &mut earlier.artifact
        {
            *command = PathBuf::from(command.to_string_lossy().replace(&delivered, &stored));
        }
        uze_core::state::record_receipt(&uze_home, earlier).unwrap();

        install();

        let config = fs::read_to_string(&config_path).unwrap();
        assert!(config.contains(&delivered), "restated: {config}");
        assert!(!config.contains(&stored), "{config}");
        let receipt = uze_core::state::receipts(&uze_home, Some("served@local"))
            .unwrap()
            .into_iter()
            .find(|r| {
                r.integration == "opencode"
                    && matches!(
                        r.artifact,
                        uze_core::integration::ManagedArtifact::VendorConfigEntry { .. }
                    )
            })
            .unwrap();
        let uze_core::integration::ManagedArtifact::VendorConfigEntry { command, .. } =
            &receipt.artifact
        else {
            unreachable!("found as a config entry");
        };
        assert!(
            command.starts_with(&delivered),
            "the ledger records the entry as written: {}",
            command.display()
        );
    });
    fs::remove_dir_all(&root).ok();
}
