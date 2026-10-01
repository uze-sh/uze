//! TUI — background worker threads: every product operation runs here
//! against a short-lived application facade, then reports back over the
//! channel so the render loop never blocks.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread,
    time::{Duration, Instant},
};

use uze_application::Preferences;

use uze_application::{
    Result, UzeApplication, UzeError, UzeHome,
    application::{
        ContextPlan, ContextReconciliationReport, InstallReport, MarketplaceRemovalReport,
        ProfileApplyResult, ProfilePreview, ProjectContextStatus, RemovePluginReport,
        UpdatePluginReport,
    },
};

use super::model::{Confirmation, Overlay, RefreshData, Status, TrustedRetry, TuiModel};
use super::orchestrator::answered_or;
use super::tui_application;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrustGrant {
    Ask,
    Granted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Intent {
    None,
    Quit,
    /// Closes the modal — the same action that opened it, or the close
    /// mark on its title.
    CloseModal,
    /// Write the operator's keyboard to `keys.json`. The keymap is already
    /// in force when this is sent — the screen swapped it between frames,
    /// which is what lets a rebinding be felt immediately. This is only
    /// the part that has to survive the process.
    PersistKeymap,
    /// Show what UZE can be drawn in.
    OpenThemePicker,
    /// Draw in this theme from now on, here and in the CLI.
    SelectTheme(String),
    /// Draw every mark from this glyph set from now on. Independent of the
    /// theme in both directions — neither call reads the other's half.
    SelectGlyphSet(String),
    /// Ring the bell for these finished agent turns from now on, in the
    /// running workspace as much as on the next launch.
    SelectChime(uze_application::Chime),
    /// Offer this extension in the workspace, or stop offering it — in the
    /// running workspace as much as on the next launch.
    SwitchExtension {
        id: String,
        name: String,
        enabled: bool,
    },
    /// Read the themes and the glyph sets the Settings screen chooses
    /// from. Sent on arriving there rather than per frame, so the list
    /// cannot change under the cursor between two frames.
    LoadSettings,
    Refresh,
    InspectPlugin(String),
    InspectMarketplacePlugin {
        name: String,
        marketplace: String,
    },
    Remove(String),
    Update(String, TrustGrant),
    Install {
        name: String,
        marketplace: String,
        grant: TrustGrant,
    },
    Setup(String),
    AddMarketplace(String),
    /// Take a marketplace off the machine, and every plugin it delivered.
    RemoveMarketplace(String),
    /// Hand a URL to the reader's own browser (the plugin drawer's Source
    /// card). Not a product operation — nothing is read or written — but
    /// it spawns a process, which is not something the render thread
    /// should be doing.
    OpenLink(String),
    /// Read the notes of this release for the modal already open on it —
    /// off the render thread, since reading them may reach the network.
    ReadReleaseNotes(String),
    /// Put the release notice about this version away, in both surfaces
    /// and every run after — a write, so not on the render thread.
    AcknowledgeRelease(String),
    ContextAnalyze(PathBuf),
    ContextApply(PathBuf),
    /// Reproduce the detected consumer workspace's `agents.lock` through
    /// the exact same Application use case `uze install` invokes.
    InstallProjectEnvironment(PathBuf),
    CreateProfile(String),
    DeleteProfile(String),
    /// Read what applying these preferences would write into each harness.
    PreviewProfile(super::model::PreviewQuestion),
    /// Fired on every Editor-panel value cycle — deliberately silent/no
    /// refresh (see `dispatch`'s arm), since the model already applied the
    /// new value optimistically and a status toast per keystroke would be
    /// noisy.
    UpdatePreferences {
        id: String,
        preferences: Preferences,
    },
    /// Carries the preferences on screen, written before applying: the
    /// editor persists each change on a thread of its own, and an apply
    /// that read the profile back from disk could overtake that write.
    ApplyProfile {
        id: String,
        preferences: Preferences,
        harness_ids: Vec<String>,
    },
}

impl Intent {
    /// The name a trace shows for this intent.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Quit => "quit",
            Self::CloseModal => "close_modal",
            Self::PersistKeymap => "persist_keymap",
            Self::OpenThemePicker => "open_theme_picker",
            Self::SelectGlyphSet(_) => "select_glyph_set",
            Self::SelectChime(_) => "select_chime",
            Self::SwitchExtension { .. } => "switch_extension",
            Self::LoadSettings => "load_settings",
            Self::SelectTheme(_) => "select_theme",
            Self::Refresh => "refresh",
            Self::InspectPlugin(_) => "inspect_plugin",
            Self::InspectMarketplacePlugin { .. } => "inspect_marketplace_plugin",
            Self::Remove(_) => "remove",
            Self::Update(..) => "update",
            Self::Install { .. } => "install",
            Self::Setup(_) => "setup",
            Self::AddMarketplace(_) => "add_marketplace",
            Self::RemoveMarketplace(_) => "remove_marketplace",
            Self::OpenLink(_) => "open_link",
            Self::ReadReleaseNotes(_) => "read_release_notes",
            Self::AcknowledgeRelease(_) => "acknowledge_release",
            Self::ContextAnalyze(_) => "context_analyze",
            Self::ContextApply(_) => "context_apply",
            Self::InstallProjectEnvironment(_) => "install_project_environment",
            Self::CreateProfile(_) => "create_profile",
            Self::DeleteProfile(_) => "delete_profile",
            Self::PreviewProfile(_) => "preview_profile",
            Self::UpdatePreferences { .. } => "update_preferences",
            Self::ApplyProfile { .. } => "apply_profile",
        }
    }
}

pub(crate) enum WorkerResult {
    Refreshed(std::result::Result<RefreshData, String>),
    /// A background pass that brought packages up to date, arriving after
    /// the screen already drew. Its own message because it reaches the
    /// network: chained ahead of the refresh, it would gate every screen on
    /// the slowest remote.
    AutoUpdated {
        applied: Vec<String>,
        data: std::result::Result<RefreshData, String>,
    },
    /// Each inspection carries the question it answers: the selection may
    /// have moved on by the time it lands, and an answer about a row the
    /// operator left is neither drawn nor reported.
    PluginInspected(
        Intent,
        std::result::Result<uze_application::application::PluginInspection, String>,
    ),
    MarketplaceInspected(
        Intent,
        std::result::Result<uze_application::application::MarketplacePluginDetail, String>,
    ),
    Mutated(std::result::Result<(String, RefreshData), String>),
    TrustRequired {
        plugin: String,
        detail: String,
        retry: TrustedRetry,
    },
    ContextAnalyzed(std::result::Result<(ProjectContextStatus, ContextPlan), String>),
    ContextApplied(std::result::Result<(String, ContextReconciliationReport), String>),
    ProfileApplied(std::result::Result<(String, Vec<ProfileApplyResult>, RefreshData), String>),
    ProfilePreviewed(
        super::model::PreviewQuestion,
        std::result::Result<ProfilePreview, String>,
    ),
    ReleaseNotesRead(String, Option<crate::self_update::ReleaseNotes>),
    /// A link handed to a browser, and the opener that took it.
    LinkOpened {
        url: String,
        opener: Option<String>,
    },
}

pub(crate) fn dispatch(
    intent: Intent,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    // Nothing is the commonest intent by a wide margin — it is what a tick
    // that resolved to no action answers — and it has nothing to dispatch,
    // clears no status and is worth no span: one around it is a record
    // that nothing happened, thousands of times an hour.
    if intent == Intent::None {
        return;
    }
    model.status_expires_at = None;
    // The key or click's span: every worker it starts captures this as its
    // parent, so a refresh's spans belong to the press that asked for it.
    let _span = tracing::info_span!("tui.intent", intent = intent.name()).entered();
    match intent {
        Intent::None | Intent::Quit | Intent::CloseModal => {}
        Intent::OpenThemePicker => open_theme_picker(home, model),
        Intent::SelectTheme(id) => match select_theme(home, &id) {
            Ok(()) => {
                model.status = Status::Success(format!("Drawing in {id}"));
                load_settings(home, model);
            }
            Err(error) => model.status = Status::Error(error),
        },
        Intent::SelectGlyphSet(id) => match select_glyph_set(home, &id) {
            Ok(()) => {
                model.status = Status::Success(format!("Drawing with the {id} glyphs"));
                load_settings(home, model);
            }
            Err(error) => model.status = Status::Error(error),
        },
        Intent::SelectChime(chime) => match select_chime(home, chime) {
            Ok(()) => {
                model.status = Status::Success(crate::ui::chime::outcome(chime).to_owned());
                model.settings_chime = chime;
            }
            Err(error) => model.status = Status::Error(error),
        },
        Intent::SwitchExtension { id, name, enabled } => {
            match switch_extension(home, &id, enabled) {
                Ok(()) => {
                    model.status =
                        Status::Success(crate::ui::extension_switch::outcome(&name, enabled));
                    model.disabled_extensions = crate::ui::extension_switch::disabled();
                }
                Err(error) => model.status = Status::Error(error),
            }
        }
        Intent::LoadSettings => load_settings(home, model),
        Intent::PersistKeymap => persist_keymap(home, model),
        Intent::Refresh => {
            if model.maintenance_in_flight {
                return;
            }
            model.status = Status::Working("Refreshing environment…".to_owned());
            model.maintenance_in_flight = true;
            spawn_refresh(home.clone(), sender.clone(), model.context_root.clone());
        }
        Intent::InspectPlugin(id) => inspect_plugin(id, home, sender, model),
        Intent::InspectMarketplacePlugin { name, marketplace } => {
            inspect_marketplace_plugin(name, marketplace, home, sender, model)
        }
        Intent::AcknowledgeRelease(version) => crate::self_update::acknowledge(home, &version),
        Intent::ReadReleaseNotes(version) => read_release_notes(version, home, sender),
        Intent::OpenLink(url) => {
            let sender = sender.clone();
            open_link(url, move |url, opener| {
                let _ = sender.send(WorkerResult::LinkOpened { url, opener });
            });
        }
        Intent::Remove(id) => {
            model.status = Status::Working(format!("Removing {id}…"));
            spawn_mutation(
                home.clone(),
                sender.clone(),
                model.context_root.clone(),
                move |app| app.plugins().remove(&id).map(remove_message),
            );
        }
        Intent::Update(id, grant) => {
            model.status = Status::Working(format!("Updating {id}…"));
            let retry_id = id.clone();
            spawn_trust_sensitive(
                home.clone(),
                sender.clone(),
                model.context_root.clone(),
                grant,
                id.clone(),
                move |app, authority| app.plugins().update(&id, authority).map(update_message),
                TrustedRetry::Update(retry_id),
            );
        }
        Intent::Install {
            name,
            marketplace,
            grant,
        } => install(name, marketplace, grant, home, sender, model),
        Intent::Setup(harness) => set_up(harness, home, sender, model),
        Intent::AddMarketplace(source) => add_marketplace(source, home, sender, model),
        Intent::RemoveMarketplace(name) => {
            model.status = Status::Working(format!("Removing marketplace {name}…"));
            spawn_mutation(
                home.clone(),
                sender.clone(),
                model.context_root.clone(),
                move |app| {
                    app.marketplace()
                        .remove(&name)
                        .map(marketplace_removal_message)
                },
            );
        }
        Intent::ContextAnalyze(root) => analyze_context(root, home, sender, model),
        Intent::InstallProjectEnvironment(root) => {
            install_project_environment(root, home, sender, model)
        }
        Intent::ContextApply(root) => apply_context(root, home, sender, model),
        Intent::CreateProfile(id) => {
            model.status = Status::Working(format!("Creating profile \"{id}\"…"));
            spawn_mutation(
                home.clone(),
                sender.clone(),
                model.context_root.clone(),
                move |app| {
                    app.profiles()
                        .create(&id, None, Preferences::default())
                        .map(|()| format!("Created profile \"{id}\""))
                },
            );
        }
        Intent::DeleteProfile(id) => {
            model.status = Status::Working(format!("Deleting profile \"{id}\"…"));
            spawn_mutation(
                home.clone(),
                sender.clone(),
                model.context_root.clone(),
                move |app| {
                    app.profiles()
                        .delete(&id)
                        .map(|()| format!("Deleted profile \"{id}\""))
                },
            );
        }
        Intent::PreviewProfile(question) => preview_profile(question, home, sender, model),
        Intent::UpdatePreferences { id, preferences } => update_preferences(id, preferences, home),
        Intent::ApplyProfile {
            id,
            preferences,
            harness_ids,
        } => apply_profile(id, preferences, harness_ids, home, sender, model),
    }
}

fn open_theme_picker(home: &UzeHome, model: &mut TuiModel) {
    // Cheap enough to read here rather than on a thread: a JSON
    // read and a directory listing, the same work `uze config theme list`
    // is budgeted for.
    let themes: Vec<(String, bool)> = tui_application(home.clone())
        .and_then(|app| app.themes().list(uze_theme::builtin_names()))
        .map(|themes| {
            themes
                .into_iter()
                .map(|theme| (theme.id, theme.active))
                .collect()
        })
        .unwrap_or_else(|_| {
            uze_theme::builtin_names()
                .iter()
                .map(|id| ((*id).to_owned(), false))
                .collect()
        });
    let selected = themes.iter().position(|(_, active)| *active).unwrap_or(0);
    model.overlay = crate::ui::model::Overlay::ThemePicker { themes, selected };
}

fn persist_keymap(home: &UzeHome, model: &mut TuiModel) {
    let file = uze_keys::difference_from_default(&uze_keys::active());
    let path = home.keymap_path();
    let written = if file.is_empty() {
        // An operator who put everything back leaves no file
        // behind: the default is not a thing to be written down.
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    } else {
        serde_json::to_string_pretty(&file)
            .map_err(std::io::Error::other)
            .and_then(|contents| std::fs::write(&path, contents + "\n"))
    };
    model.status = match written {
        Ok(()) => Status::Success("Keyboard saved".to_owned()),
        Err(error) => Status::Error(format!("{}: {error}", path.display())),
    };
}

fn inspect_plugin(id: String, home: &UzeHome, sender: &Sender<WorkerResult>, model: &mut TuiModel) {
    let asked = Intent::InspectPlugin(id.clone());
    model.inspection_in_flight = Some(asked.clone());
    let (home, sender) = (home.clone(), sender.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        let result = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| app.plugins().inspect(&id))
                    .map_err(|error| error.to_string())
            },
            Err(format!("Inspecting {id} failed")),
        );
        let _ = sender.send(WorkerResult::PluginInspected(asked, result));
    });
}

fn inspect_marketplace_plugin(
    name: String,
    marketplace: String,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    let asked = Intent::InspectMarketplacePlugin {
        name: name.clone(),
        marketplace: marketplace.clone(),
    };
    model.inspection_in_flight = Some(asked.clone());
    let (home, sender) = (home.clone(), sender.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        let result = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| app.marketplace().inspect_plugin(&marketplace, &name))
                    .map_err(|error| error.to_string())
            },
            Err(format!("Inspecting {name} failed")),
        );
        let _ = sender.send(WorkerResult::MarketplaceInspected(asked, result));
    });
}

fn read_release_notes(version: String, home: &UzeHome, sender: &Sender<WorkerResult>) {
    let (home, sender) = (home.clone(), sender.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let notes = answered_or(|| crate::self_update::release_notes(&home, &version), None);
        let _ = sender.send(WorkerResult::ReleaseNotesRead(version, notes));
    });
}

fn set_up(harness: String, home: &UzeHome, sender: &Sender<WorkerResult>, model: &mut TuiModel) {
    model.status = Status::Working(format!("Setting up {harness}…"));
    spawn_mutation(
        home.clone(),
        sender.clone(),
        model.context_root.clone(),
        move |app| {
            app.setup(Some(&harness)).map(|results| {
                results
                    .into_iter()
                    .find(|r| r.integration == harness)
                    .map(|r| setup_outcome(&harness, &r))
                    .unwrap_or_else(|| format!("{harness} setup attempted"))
            })
        },
    );
}

/// What one `uze setup` did, in the words the CLI reports it with: the
/// action taken, the version verified, where an executable off `PATH` was
/// found, and why a harness that is not ready is not.
fn setup_outcome(harness: &str, result: &uze_application::application::SetupResult) -> String {
    if !result.configured {
        let reason = result
            .provisioning
            .reason
            .as_deref()
            .unwrap_or("executable was not verified");
        return format!("{harness} setup {:?}: {reason}", result.provisioning.status);
    }
    let action = format!("{:?}", result.provisioning.action).to_lowercase();
    let version = result.detection.version.as_deref().unwrap_or("unknown");
    let mut outcome = format!("{harness} ready ({action}; version {version})");
    if let Some(found) = &result.provisioning.located_outside_path {
        outcome.push_str(&format!(
            "; found at {}, open a new shell to run it by name",
            found.display()
        ));
    }
    outcome
}

fn add_marketplace(
    source: String,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.status = Status::Working(format!("Adding marketplace from {source}…"));
    spawn_mutation(
        home.clone(),
        sender.clone(),
        model.context_root.clone(),
        move |app| {
            app.marketplace().register(&source).map(|registration| {
                let identity = &registration.identity;
                let reads = registration.reads();
                if registration.added {
                    format!("Added marketplace from {identity}. {reads}")
                } else {
                    format!("Marketplace from {identity} is already added. {reads}")
                }
            })
        },
    );
}

fn analyze_context(
    root: PathBuf,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.status = Status::Working("Analyzing project context…".to_owned());
    let (home, sender) = (home.clone(), sender.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        let result = tui_application(home).and_then(|app| {
            let status = app.context().inspect(&root)?;
            let plan = app.context().plan(&root)?;
            Ok((status, plan))
        });
        let _ = sender.send(WorkerResult::ContextAnalyzed(
            result.map_err(|error| error.to_string()),
        ));
    });
}

fn install_project_environment(
    root: PathBuf,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.status = Status::Working("Installing project environment…".to_owned());
    spawn_mutation(
        home.clone(),
        sender.clone(),
        model.context_root.clone(),
        move |app| {
            // Same use case and same default (no trust flag) as the
            // CLI's `uze install`; the TUI adds no install logic.
            app.project()
                .install(&root, &uze_application::NoTrustAuthority)
                .and_then(|report| {
                    // Installed but not delivered everywhere is a failure
                    // to say, exactly as the CLI exits non-zero on it.
                    let missed: Vec<String> = report
                        .undelivered()
                        .map(|(plugin, delivery)| format!("{plugin} to {}", delivery.display_name))
                        .collect();
                    if missed.is_empty() {
                        Ok(report)
                    } else {
                        Err(uze_application::UzeError::DeliveryFailed(format!(
                            "installed, but not delivered: {}",
                            missed.join(", ")
                        )))
                    }
                })
                .map(|report| match report {
                    InstallReport::NoChanges => "Project environment already up to date".to_owned(),
                    InstallReport::NoProject => "No project here; nothing was declared".to_owned(),
                    InstallReport::Installed {
                        plugins,
                        reconciled,
                        ..
                    } => match (plugins.len(), reconciled) {
                        (0, _) => "Project context reconciled".to_owned(),
                        (count, _) => format!(
                            "Installed {count} plugin{}",
                            if count == 1 { "" } else { "s" }
                        ),
                    },
                })
        },
    );
}

fn apply_context(
    root: PathBuf,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.status = Status::Working("Applying context reconciliation…".to_owned());
    let (home, sender) = (home.clone(), sender.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        let result = tui_application(home)
            .and_then(|app| app.context().reconcile(&root))
            .map(|report| ("Context reconciled".to_owned(), report))
            .map_err(|error| error.to_string());
        let _ = sender.send(WorkerResult::ContextApplied(result));
    });
}

fn preview_profile(
    question: super::model::PreviewQuestion,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.profile_preview_asked = Some(question.clone());
    let (home, sender) = (home.clone(), sender.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        let result = tui_application(home)
            .map(|app| {
                app.profiles()
                    .preview(&question.preferences, &question.harness_ids)
            })
            .map_err(|error| error.to_string());
        let _ = sender.send(WorkerResult::ProfilePreviewed(question, result));
    });
}

fn update_preferences(id: String, preferences: Preferences, home: &UzeHome) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        if let Ok(app) = tui_application(home) {
            let _ = app.profiles().update_preferences(&id, preferences);
        }
    });
}

fn install(
    name: String,
    marketplace: String,
    grant: TrustGrant,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.status = Status::Working(format!("Installing {name}…"));
    let retry_name = name.clone();
    let retry_marketplace = marketplace.clone();
    let spec = format!("{name}@{marketplace}");
    spawn_trust_sensitive(
        home.clone(),
        sender.clone(),
        model.context_root.clone(),
        grant,
        name,
        move |app, authority| {
            app.marketplace()
                .install_plugin(&spec, authority)
                .and_then(|report| {
                    let missed: Vec<&str> = report
                        .undelivered()
                        .map(|delivery| delivery.display_name.as_str())
                        .collect();
                    if missed.is_empty() {
                        return Ok(format!("Installed {}", report.plugin.id));
                    }
                    Err(uze_application::UzeError::DeliveryFailed(format!(
                        "{} installed, but not delivered to {}",
                        report.plugin.id,
                        missed.join(", ")
                    )))
                })
        },
        TrustedRetry::Install {
            name: retry_name,
            marketplace: retry_marketplace,
        },
    );
}

fn apply_profile(
    id: String,
    preferences: Preferences,
    harness_ids: Vec<String>,
    home: &UzeHome,
    sender: &Sender<WorkerResult>,
    model: &mut TuiModel,
) {
    model.status = Status::Working(format!("Applying \"{id}\"…"));
    let (home, sender, context_root) = (home.clone(), sender.clone(), model.context_root.clone());
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.worker").entered();
        let result = tui_application(home.clone())
            .and_then(|app| {
                app.profiles().update_preferences(&id, preferences)?;
                app.profiles().set_active(&id)?;
                let results = app.profiles().apply(&id, &harness_ids)?;
                let data = load_refresh_data(home, &context_root)?;
                Ok({
                    let message = apply_message(&id, &results);
                    (message, results, data)
                })
            })
            .map_err(|error| error.to_string());
        let _ = sender.send(WorkerResult::ProfileApplied(result));
    });
}

pub(crate) fn spawn_refresh(home: UzeHome, sender: Sender<WorkerResult>, context_root: PathBuf) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.refresh").entered();
        let result = load_refresh_data(home, &context_root).map_err(|error| error.to_string());
        let _ = sender.send(WorkerResult::Refreshed(result));
    });
}

/// The one-time startup path, run in the background so the terminal takes
/// over instantly instead of sitting blank while default plugins are
/// seeded. Before this moved here, `main` ran `ensure_default_plugins`
/// synchronously — several harness-detection subprocess spawns — *before*
/// the alternate screen was even entered, so the terminal appeared frozen
/// for that whole stretch.
///
/// Started once per session by [`super::management::ManagementMemory::warming`],
/// at launch rather than on the first opening of the management modal:
/// bootstrap and refresh share one worker, so the one answer it composes
/// is normally waiting by the time that screen is asked for. Every
/// subsequent refresh (`Intent::Refresh`) goes through `spawn_refresh` and
/// is coalesced while a worker is in flight.
pub(crate) fn spawn_startup(home: UzeHome, sender: Sender<WorkerResult>, context_root: PathBuf) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.startup").entered();
        if let Ok(app) = tui_application(home.clone()) {
            let _ = app.ensure_default_plugins();
        }
        // The screen's own data goes first and on its own. Bringing
        // packages up to date reaches a remote, and chaining it ahead of
        // this made every screen wait on the slowest one — which is the
        // opposite of what a background pass is for.
        let refreshed =
            load_refresh_data(home.clone(), &context_root).map_err(|error| error.to_string());
        let _ = sender.send(WorkerResult::Refreshed(refreshed));

        // Opening uze is the explicit, interactive act the CLI's read-only
        // dispatch path deliberately isn't, so this is where a pending
        // update gets applied instead of only reported. Anything it cannot
        // settle alone — a revision asking to execute something new,
        // managed state it refuses to disturb — stays reported, and the
        // `u` action with its trust dialog is still the way through.
        let Ok(app) = tui_application(home.clone()) else {
            return;
        };
        let applied: Vec<String> = app
            .plugins()
            .auto_update()
            .into_iter()
            .filter(|outcome| outcome.applied)
            .map(|outcome| outcome.plugin)
            .collect();
        if applied.is_empty() {
            return;
        }
        // Only when something actually moved: a second refresh nothing
        // asked for is a screen that flickers for no reason.
        let data = load_refresh_data(home, &context_root).map_err(|error| error.to_string());
        let _ = sender.send(WorkerResult::AutoUpdated { applied, data });
    });
}

fn load_refresh_data(home: UzeHome, context_root: &std::path::Path) -> Result<RefreshData> {
    let app = tui_application(home)?;
    let snapshot = app.machine_snapshot(context_root)?;
    let mut plugins = snapshot.plugins;
    // Official plugins always lead the list — a stable sort keeps every
    // other ordering (whatever `list_plugins` returns) untouched within
    // each of the two groups.
    plugins.sort_by_key(|plugin| !plugin.source.starts_with("embedded:"));
    Ok(RefreshData {
        plugins,
        doctor: Some(snapshot.doctor),
        marketplace_plugins: snapshot.marketplace_plugins,
        marketplaces: snapshot.marketplaces,
        profiles: snapshot.profiles,
        context_status: snapshot.context_status,
        workspace: snapshot.workspace,
        // Only `spawn_startup` ever fills this in; an ordinary refresh
        // reports no auto-updates rather than re-raising old badges.
        auto_updated: Vec::new(),
    })
}

fn spawn_mutation(
    home: UzeHome,
    sender: Sender<WorkerResult>,
    context_root: PathBuf,
    operation: impl FnOnce(&UzeApplication) -> Result<String> + Send + 'static,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.mutation").entered();
        let result = tui_application(home.clone()).and_then(|app| {
            let message = operation(&app)?;
            let data = load_refresh_data(home, &context_root)?;
            Ok((message, data))
        });
        let _ = sender.send(WorkerResult::Mutated(
            result.map_err(|error| error.to_string()),
        ));
    });
}

/// Like `spawn_mutation`, but the operation is one that can cross the trust
/// boundary. With `TrustGrant::Ask` it runs non-interactively
/// (`NoTrustAuthority`) and, on `TRUST_REQUIRED`, surfaces a dialog rather
/// than failing silently or granting on the operator's behalf.
/// `TrustGrant::Granted` is only ever reached by that dialog's own explicit
/// confirmation re-dispatching the same action.
fn spawn_trust_sensitive(
    home: UzeHome,
    sender: Sender<WorkerResult>,
    context_root: PathBuf,
    grant: TrustGrant,
    package_hint: String,
    operation: impl FnOnce(&UzeApplication, &dyn uze_application::TrustAuthority) -> Result<String>
    + Send
    + 'static,
    retry: TrustedRetry,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let _span = tracing::info_span!("tui.trust_sensitive").entered();
        let outcome = tui_application(home.clone()).and_then(|app| {
            let result = match grant {
                TrustGrant::Ask => operation(&app, &uze_application::NoTrustAuthority),
                TrustGrant::Granted => operation(&app, &uze_application::AlwaysTrust),
            };
            result.map(|message| (message, ()))
        });
        match outcome {
            Ok((message, ())) => match load_refresh_data(home, &context_root) {
                Ok(data) => {
                    let _ = sender.send(WorkerResult::Mutated(Ok((message, data))));
                }
                Err(error) => {
                    let _ = sender.send(WorkerResult::Mutated(Err(error.to_string())));
                }
            },
            Err(UzeError::TrustRequired { package, detail }) => {
                let _ = sender.send(WorkerResult::TrustRequired {
                    plugin: if package.is_empty() {
                        package_hint
                    } else {
                        package
                    },
                    detail,
                    retry,
                });
            }
            Err(error) => {
                let _ = sender.send(WorkerResult::Mutated(Err(error.to_string())));
            }
        }
    });
}

/// Answers whether anything arrived.
pub(crate) fn drain_worker_results(
    model: &mut TuiModel,
    receiver: &std::sync::mpsc::Receiver<WorkerResult>,
) -> bool {
    let mut arrived = false;
    while let Ok(result) = receiver.try_recv() {
        arrived = true;
        match result {
            WorkerResult::ReleaseNotesRead(version, notes) => {
                if let Overlay::ReleaseNotes(modal) = &mut model.overlay {
                    modal.absorb(&version, notes);
                }
            }
            WorkerResult::LinkOpened { url, opener } => {
                model.status = match opener {
                    // Present tense on purpose: the opener took the
                    // address, which is all that can be known without
                    // waiting on it.
                    Some(opener) => Status::Success(format!("Opening {url} via {opener}")),
                    // The address is already on screen beside the glyph
                    // that was clicked, so a failure here costs the reader
                    // a copy-paste, not the link.
                    None => Status::Error(format!("No browser to open {url} with")),
                };
            }
            // A pass that moved something, arriving after the screen drew.
            // Carried into the same `refreshed` path so the rows and the
            // badge come from one answer rather than two.
            WorkerResult::AutoUpdated { applied, data } => match data {
                Ok(data) => {
                    let updated = applied.len();
                    model.refreshed(RefreshData {
                        auto_updated: applied,
                        ..data
                    });
                    model.status = Status::Success(format!(
                        "Updated {updated} plugin{}",
                        if updated == 1 { "" } else { "s" }
                    ));
                    model.status_expires_at = Some(Instant::now() + Duration::from_secs(5));
                }
                // The update happened; only re-reading the machine failed.
                // Saying so is better than a silent screen that is now one
                // refresh behind what is on disk.
                Err(error) => {
                    model.status =
                        Status::Error(format!("Updated, but could not refresh: {error}"));
                    model.status_expires_at = Some(Instant::now() + Duration::from_secs(5));
                }
            },
            WorkerResult::Refreshed(Ok(data)) => {
                let repaired = data
                    .doctor
                    .as_ref()
                    .map(|doctor| doctor.maintenance.repaired_count())
                    .unwrap_or_default();
                let updated = data.auto_updated.len();
                model.refreshed(data);
                model.maintenance_in_flight = false;
                // The badge only exists on the Plugins screen, and startup
                // lands on Overview — this is how an operator who never
                // opens Plugins still learns something changed under them.
                if updated > 0 {
                    model.status = Status::Success(format!(
                        "Updated {updated} plugin{}",
                        if updated == 1 { "" } else { "s" }
                    ));
                    model.status_expires_at = Some(Instant::now() + Duration::from_secs(5));
                } else if repaired > 0 {
                    model.status = Status::Success(format!(
                        "Synchronized {repaired} attachment{}",
                        if repaired == 1 { "" } else { "s" }
                    ));
                }
            }
            // The operator moved on before this landed. Were it absorbed, it
            // would displace the detail of the row they are on and have the
            // per-frame check ask for that one again; were it a failure, it
            // would report an error about a row nobody is looking at.
            WorkerResult::PluginInspected(asked, _)
            | WorkerResult::MarketplaceInspected(asked, _)
                if model.marketplace_inspect_intent() != asked =>
            {
                if model.inspection_in_flight.as_ref() == Some(&asked) {
                    model.inspection_in_flight = None;
                }
            }
            WorkerResult::PluginInspected(_, Ok(inspection)) => {
                model.plugin_resources.insert(
                    inspection.plugin.id.clone(),
                    inspection.capabilities.clone(),
                );
                model.plugin_detail = Some(inspection);
                model.inspection_in_flight = None;
            }
            WorkerResult::MarketplaceInspected(_, Ok(detail)) => {
                model.plugin_resources.insert(
                    model.marketplace_plugin_id(&detail.summary),
                    detail.capabilities.clone(),
                );
                model.marketplace_detail = Some(detail);
                model.inspection_in_flight = None;
            }
            WorkerResult::Mutated(Ok((message, data))) => {
                model.refreshed(data);
                // What was inspected before the change describes a plugin
                // that no longer stands as it did; the drawer asks again.
                model.plugin_detail = None;
                model.marketplace_detail = None;
                model.inspection_in_flight = None;
                model.status = Status::Success(message);
            }
            WorkerResult::TrustRequired {
                plugin,
                detail,
                retry,
            } => {
                model.overlay = Overlay::Confirm {
                    kind: Confirmation::Trust {
                        plugin,
                        detail,
                        retry,
                    },
                    focus: None,
                };
                model.status = Status::Idle;
            }
            WorkerResult::ContextAnalyzed(Ok((status, plan))) => {
                model.remembered.context_status = Some(status);
                model.remembered.context_plan = Some(plan);
                model.status = Status::Idle;
            }
            WorkerResult::ContextApplied(Ok((message, report))) => {
                model.status = Status::Success(message);
                let _ = report;
            }
            WorkerResult::ProfileApplied(Ok((message, results, data))) => {
                model.refreshed(data);
                model.profile_apply_results = results;
                model.status = Status::Success(message);
                model.status_expires_at = Some(Instant::now() + Duration::from_secs(5));
            }
            WorkerResult::ProfilePreviewed(question, result) => {
                model.profile_previewed(question, result);
            }
            WorkerResult::Refreshed(Err(error)) => {
                model.maintenance_in_flight = false;
                model.status = Status::Error(error);
            }
            // A failed inspection stays failed until the selection moves:
            // clearing the in-flight marker here would have the per-frame
            // check retry it forever, error after error.
            //
            // Not said over work in flight: an install or a removal changes
            // the very thing a read of it was asking about, so a read that
            // failed meanwhile is that work's consequence, not a fault — and
            // the line it would take is the one saying the work is running.
            WorkerResult::PluginInspected(_, Err(error))
            | WorkerResult::MarketplaceInspected(_, Err(error)) => {
                if !matches!(model.status, Status::Working(_)) {
                    model.status = Status::Error(error);
                }
            }
            WorkerResult::Mutated(Err(error))
            | WorkerResult::ContextAnalyzed(Err(error))
            | WorkerResult::ContextApplied(Err(error))
            | WorkerResult::ProfileApplied(Err(error)) => model.status = Status::Error(error),
        }
    }
    arrived
}

/// `Applied profile "default" to 3 harnesses · 1 approximation` — the one
/// concise status line the spec asks for; per-harness detail lives in the
/// Harnesses panel's badges (`model.profile_apply_results`), not here.
fn apply_message(id: &str, results: &[ProfileApplyResult]) -> String {
    use uze_application::PreferenceApplyOutcome;
    let approximated = results
        .iter()
        .filter(|result| {
            matches!(
                result.outcome,
                PreferenceApplyOutcome::AppliedWithApproximation { .. }
            )
        })
        .count();
    let failed = results
        .iter()
        .filter(|result| matches!(result.outcome, PreferenceApplyOutcome::Failed { .. }))
        .count();
    let mut message = format!(
        "Applied profile \"{id}\" to {} harness{}",
        results.len(),
        if results.len() == 1 { "" } else { "es" }
    );
    if approximated > 0 {
        message.push_str(&format!(
            " · {approximated} approximation{}",
            if approximated == 1 { "" } else { "s" }
        ));
    }
    if failed > 0 {
        message.push_str(&format!(" · {failed} failed",));
    }
    message
}

fn remove_message(report: RemovePluginReport) -> String {
    match report {
        RemovePluginReport::Removed { plugin, .. } => format!("Removed {plugin}"),
        RemovePluginReport::AlreadyAbsent { plugin } => {
            format!("No UZE state remains for {plugin}")
        }
        RemovePluginReport::Blocked { report, .. } => {
            format!(
                "{} changed outside UZE; managed state was preserved{}",
                report.package_id,
                what_blocked(&report)
            )
        }
    }
}

/// A marketplace that could not be emptied stays registered, so the
/// message says which case this was rather than claiming it is gone.
fn marketplace_removal_message(report: MarketplaceRemovalReport) -> String {
    if report.record_removed {
        return format!("Removed marketplace {}", report.marketplace);
    }
    let blocked = report
        .blocked
        .iter()
        .map(|package| package.package.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} is still registered: {blocked} changed outside UZE and was left in place",
        report.marketplace
    )
}

fn update_message(report: UpdatePluginReport) -> String {
    match report {
        UpdatePluginReport::Updated { plugin, .. } => format!("Updated {}", plugin.id),
        UpdatePluginReport::Blocked { report, .. } => {
            format!(
                "{} update blocked; managed state was preserved{}",
                report.package_id,
                what_blocked(&report)
            )
        }
    }
}

/// The first receipt that stood in the way, and why: "blocked" alone
/// leaves the operator nothing to act on.
fn what_blocked(report: &uze_application::ReconciliationReport) -> String {
    if let Some(error) = &report.ledger_error {
        return format!(": {error}");
    }
    report
        .receipts
        .iter()
        .find(|receipt| {
            !matches!(
                receipt.inspection.state,
                uze_application::AttachmentState::Matched
                    | uze_application::AttachmentState::Missing
            )
        })
        .map(|receipt| {
            format!(
                ": {} ({})",
                receipt.inspection.reason, receipt.receipt.integration
            )
        })
        .unwrap_or_default()
}

/// Hands `url` to the reader's browser on a thread of its own, and tells
/// `answered` which opener took it. Finding one spawns processes, which
/// the thread drawing the frame never waits on.
pub(crate) fn open_link(
    url: String,
    answered: impl FnOnce(String, Option<String>) + Send + 'static,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let opener = answered_or(|| open_in_browser(&url), None);
        answered(url, opener);
    });
}

/// Hands `url` to whatever this machine opens links with, and answers with
/// the name of the opener that accepted it.
///
/// Ordered by how directly each one speaks for the user: `$BROWSER` is
/// what they set themselves, `xdg-open` is what the desktop answers with,
/// `sensible-browser` is Debian's fallback, and `explorer.exe` answers on
/// WSL — where `xdg-open` is installed as somebody else's dependency,
/// accepts the address and exits 4 without opening a thing.
///
/// `wslview` is deliberately absent: it is the tool that case calls for,
/// but its project (`wslutilities/wslu`) was archived in March 2025, and
/// it only shells out to the Windows interop this list now reaches
/// directly. Anyone still running it names it in `$BROWSER`, which is
/// honoured ahead of everything here.
///
/// Every stream is closed: the alternate screen belongs to ratatui, and a
/// browser's startup chatter written into it lands in the middle of the
/// frame.
fn open_in_browser(url: &str) -> Option<String> {
    // A marketplace author writes the homepage, and an opener handed a
    // `file:` path or a custom scheme would launch whatever the desktop
    // associates with it.
    if !is_web_address(url) {
        return None;
    }
    // `$BROWSER` is a colon-separated list, and an entry may carry the URL
    // in a `%s` placeholder rather than as a trailing argument.
    let configured = std::env::var("BROWSER").unwrap_or_default();
    let preferred: Vec<&str> = configured.split(':').filter(|e| !e.is_empty()).collect();
    // `open` first, and only on macOS: it is the one opener there, and the
    // three below are all absent — so every link the workspace offered on a
    // Mac died as "no browser to open it with" unless the operator had set
    // `$BROWSER`. Written when Linux was the only reader, and found by
    // asking a second platform.
    let native: &[&str] = if cfg!(target_os = "macos") {
        &["open"]
    } else {
        &["xdg-open", "sensible-browser", "explorer.exe"]
    };
    for opener in preferred.into_iter().chain(native.iter().copied()) {
        let mut words = opener.split_whitespace();
        let Some(program) = words.next() else {
            continue;
        };
        let mut command = Command::new(program);
        let mut placed = false;
        for word in words {
            command.arg(word.replace("%s", url));
            placed |= word.contains("%s");
        }
        if !placed {
            command.arg(url);
        }
        let spawned = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            // A launcher hands the URL over and exits at once; nobody is
            // waiting on it, so reap it off-thread rather than leaving a
            // zombie behind for as long as the TUI runs.
            let parent = tracing::Span::current();
            thread::spawn(move || {
                let _parent = parent.enter();
                let _ = child.wait();
            });
            return Some(program.to_owned());
        }
    }
    None
}

fn is_web_address(url: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        url.get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
    })
}

/// Puts a theme in force and records it, so the next frame — and the next
/// `uze` command — are drawn in it. No session, pane or agent is touched:
/// changing what UZE looks like is not an event in the work it is hosting.
/// Records the glyph set and puts the re-resolved stack in force.
///
/// Resolved against whichever theme is active — including none, which
/// resolves the default — because the set is a layer beneath the theme
/// rather than a theme of its own.
fn select_glyph_set(home: &UzeHome, id: &str) -> std::result::Result<(), String> {
    let application = tui_application(home.clone()).map_err(|error| error.to_string())?;
    application
        .themes()
        .select_glyphs(id)
        .map_err(|error| error.to_string())?;
    let theme = application
        .themes()
        .active()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| uze_theme::builtin_names()[0].to_owned());
    let loaded =
        crate::theme::resolve(&application, home, &theme).map_err(|error| error.to_string())?;
    uze_theme::set_active(loaded.theme);
    Ok(())
}

/// What a theme card shows of a palette: the accent it leads with, then
/// the four states that carry meaning, then the brightest text. Six is what
/// a card has room for, and these six are the ones a theme is actually
/// judged on — a row of surfaces would be six shades of the same near-black.
const SWATCHES: &[uze_theme::Token] = &[
    uze_theme::Token::Accent,
    uze_theme::Token::StateSuccess,
    uze_theme::Token::StateWarning,
    uze_theme::Token::StateDanger,
    uze_theme::Token::StateInfo,
    uze_theme::Token::TextBright,
];

/// Reads both lists the Settings screen chooses from.
///
/// Cheap enough to read on this thread rather than a worker, for the same
/// reason the theme picker reads its own: a JSON read and a directory
/// listing, which is exactly the work `uze config theme list` is budgeted for.
fn load_settings(home: &UzeHome, model: &mut TuiModel) {
    model.settings_read = true;
    let Ok(application) = tui_application(home.clone()) else {
        return;
    };
    // The settings file is shared by every group here, so one that does
    // not parse fails all three reads the same way: said once, by name,
    // rather than showing a screen with nothing chosen and no reason.
    if let Err(error) = application.themes().active() {
        model.status = Status::Error(error.to_string());
    }
    if let Ok(themes) = application.themes().list(uze_theme::builtin_names()) {
        // Resolved here rather than per frame: each one is a file read, and
        // a screen that re-read the whole themes directory every tick would
        // be paying a directory walk to draw six coloured cells.
        model.settings_palettes = themes
            .iter()
            .filter_map(|theme| {
                let loaded = crate::theme::resolve(&application, home, &theme.id).ok()?;
                let colours = SWATCHES
                    .iter()
                    .map(|token| loaded.theme.color(*token))
                    .collect();
                Some((theme.id.clone(), colours))
            })
            .collect();
        model.settings_themes = themes;
    }
    if let Ok(sets) = application.themes().glyph_sets(uze_theme::glyph_sets()) {
        model.settings_glyph_sets = sets;
    }
    if let Ok(chime) = application.notifications().agent_finished() {
        model.settings_chime = chime;
    }
    model.settle_settings_selection();
}

/// Records the choice and puts it in force in this process — the
/// workspace under the modal reads it at the next finished turn.
fn select_chime(home: &UzeHome, chime: uze_application::Chime) -> std::result::Result<(), String> {
    tui_application(home.clone())
        .and_then(|app| app.notifications().set_agent_finished(chime))
        .map_err(|error| error.to_string())?;
    crate::ui::chime::set(chime);
    if chime != uze_application::Chime::Silent {
        crate::ui::chime::preview();
    }
    Ok(())
}

/// Records the choice and puts it in force in this process — the
/// workspace under the modal drops or restores the extension on its next
/// frame.
fn switch_extension(home: &UzeHome, id: &str, enabled: bool) -> std::result::Result<(), String> {
    tui_application(home.clone())
        .and_then(|app| app.extensions().set_enabled(id, enabled))
        .map_err(|error| error.to_string())?;
    crate::ui::extension_switch::set(id, enabled);
    Ok(())
}

fn select_theme(home: &UzeHome, id: &str) -> std::result::Result<(), String> {
    let application = tui_application(home.clone()).map_err(|error| error.to_string())?;
    let loaded =
        crate::theme::resolve(&application, home, id).map_err(|error| error.to_string())?;
    application
        .themes()
        .select(id)
        .map_err(|error| error.to_string())?;
    uze_theme::set_active(loaded.theme);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::mpsc};

    use uze_application::application::{
        DoctorReport, MaintenanceOutcome, MaintenanceReport, StoreHealth,
    };

    use super::*;

    fn refreshed_with_repair() -> RefreshData {
        RefreshData {
            doctor: Some(DoctorReport {
                uze_home: PathBuf::from("/home/uze"),
                store: StoreHealth::Ready,
                plugins: Vec::new(),
                harnesses: Vec::new(),
                attachments: Vec::new(),
                deliveries: Vec::new(),
                ledger_error: None,
                provisioning_state_error: None,
                leftovers: Default::default(),
                maintenance: MaintenanceReport {
                    outcomes: vec![MaintenanceOutcome::Repaired {
                        plugin: "fixture@local".to_owned(),
                        integration: "fixture".to_owned(),
                        receipt: "fixture:skill".to_owned(),
                    }],
                },
            }),
            ..RefreshData::default()
        }
    }

    #[test]
    fn repaired_maintenance_is_a_success_notification_not_a_problem() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(WorkerResult::Refreshed(Ok(refreshed_with_repair())))
            .unwrap();
        let mut model = TuiModel {
            maintenance_in_flight: true,
            ..TuiModel::default()
        };

        drain_worker_results(&mut model, &receiver);

        assert!(!model.maintenance_in_flight);
        assert_eq!(
            model.status,
            Status::Success("Synchronized 1 attachment".to_owned())
        );
    }

    fn drained(results: Vec<WorkerResult>, mut model: TuiModel) -> TuiModel {
        let (sender, receiver) = mpsc::channel();
        for result in results {
            sender.send(result).unwrap();
        }
        drain_worker_results(&mut model, &receiver);
        model
    }

    /// Clearing the marker on failure would have the per-frame check ask
    /// again at once, and again after that: a failed inspection stays
    /// failed until the selection moves.
    #[test]
    fn a_failed_inspection_keeps_its_marker_so_it_is_not_retried_every_frame() {
        let inspecting = Intent::InspectPlugin("one".to_owned());
        let model = drained(
            vec![WorkerResult::PluginInspected(
                inspecting.clone(),
                Err("unreadable".to_owned()),
            )],
            TuiModel {
                inspection_in_flight: Some(inspecting.clone()),
                ..browsing(&["one", "two"])
            },
        );

        assert_eq!(model.inspection_in_flight, Some(inspecting));
        assert_eq!(model.status, Status::Error("unreadable".to_owned()));
    }

    /// An answer about a row the operator already left is neither drawn
    /// nor reported, and does not release the inspection now running for
    /// the row they are on.
    #[test]
    fn an_answer_for_a_row_left_behind_is_dropped() {
        let current = Intent::InspectPlugin("two".to_owned());
        let mut moved_on = browsing(&["one", "two"]);
        moved_on.remembered.plugin_screen.selected = 1;
        moved_on.inspection_in_flight = Some(current.clone());
        moved_on.status = Status::Working("Inspecting two…".to_owned());

        let model = drained(
            vec![WorkerResult::PluginInspected(
                Intent::InspectPlugin("one".to_owned()),
                Err("index.lock exists".to_owned()),
            )],
            moved_on,
        );

        assert_eq!(model.inspection_in_flight, Some(current));
        assert_eq!(model.status, Status::Working("Inspecting two…".to_owned()));
    }

    /// Reading the row the pointer lands on says nothing on the status
    /// line: it is not work anybody asked for, and the line may already be
    /// saying that an install is running — which a read finishing, or one
    /// failing because the install changed what it read, must not erase.
    #[test]
    fn an_inspection_leaves_the_status_line_to_the_work_in_flight() {
        let installing = Status::Working("Installing two…".to_owned());
        let mut model = browsing(&["one", "two"]);
        model.status = installing.clone();
        let asked = model.marketplace_inspect_intent();
        let Intent::InspectPlugin(id) = asked.clone() else {
            panic!("an installed row is inspected as installed: {asked:?}");
        };
        model.inspection_in_flight = Some(asked.clone());

        let model = drained(
            vec![WorkerResult::PluginInspected(
                asked,
                Err(format!("unknown UZE package `{id}`")),
            )],
            model,
        );

        assert_eq!(model.status, installing);
    }

    fn browsing(ids: &[&str]) -> TuiModel {
        let mut model = TuiModel {
            focus: crate::ui::model::Focus::Content,
            route: crate::ui::model::Route::Plugins,
            ..TuiModel::default()
        };
        model.remembered.plugins = ids
            .iter()
            .map(|id| uze_application::application::PluginSummary {
                id: (*id).to_owned(),
                active_name: (*id).to_owned(),
                source: "embedded:example".to_owned(),
                store_path: PathBuf::from("/store/example"),
                commit: None,
                capability_count: 1,
                freshness: uze_application::application::Freshness::not_checked(),
                undelivered: Vec::new(),
            })
            .collect();
        model
    }

    #[test]
    fn a_failed_refresh_releases_maintenance_and_says_why() {
        let model = drained(
            vec![WorkerResult::Refreshed(Err("store unreadable".to_owned()))],
            TuiModel {
                maintenance_in_flight: true,
                ..TuiModel::default()
            },
        );

        assert!(!model.maintenance_in_flight, "a later refresh may run");
        assert_eq!(model.status, Status::Error("store unreadable".to_owned()));
    }

    /// An auto-update is announced even though its badge lives on a screen
    /// the operator may never open, and it outranks a repair in the one
    /// status line there is.
    #[test]
    fn updated_plugins_are_announced_ahead_of_repaired_attachments() {
        let data = RefreshData {
            auto_updated: vec!["a@m".to_owned(), "b@m".to_owned()],
            ..refreshed_with_repair()
        };
        let model = drained(vec![WorkerResult::Refreshed(Ok(data))], TuiModel::default());

        assert_eq!(
            model.status,
            Status::Success("Updated 2 plugins".to_owned())
        );
        assert!(
            model.status_expires_at.is_some(),
            "the notice goes away on its own"
        );
    }

    #[test]
    fn a_refusal_for_trust_asks_before_retrying() {
        let model = drained(
            vec![WorkerResult::TrustRequired {
                plugin: "flow@market".to_owned(),
                detail: "runs hooks".to_owned(),
                retry: TrustedRetry::Update("flow@market".to_owned()),
            }],
            TuiModel::default(),
        );

        assert!(matches!(
            model.overlay,
            Overlay::Confirm {
                kind: Confirmation::Trust { ref plugin, .. },
                focus: None,
            } if plugin == "flow@market"
        ));
        assert_eq!(model.status, Status::Idle);
    }

    #[test]
    fn the_apply_message_counts_harnesses_approximations_and_failures() {
        use uze_application::PreferenceApplyOutcome;
        let result = |outcome| ProfileApplyResult {
            integration: "fixture".to_owned(),
            outcome,
        };
        let results = vec![
            result(PreferenceApplyOutcome::Applied {
                changed_keys: Vec::new(),
            }),
            result(PreferenceApplyOutcome::AppliedWithApproximation {
                changed_keys: Vec::new(),
                notes: Vec::new(),
            }),
            result(PreferenceApplyOutcome::Failed {
                reason: "read-only".to_owned(),
            }),
        ];

        assert_eq!(
            apply_message("default", &results),
            "Applied profile \"default\" to 3 harnesses · 1 approximation · 1 failed"
        );
        assert_eq!(
            apply_message("default", &results[..1]),
            "Applied profile \"default\" to 1 harness"
        );
    }

    #[test]
    fn refresh_is_coalesced_while_maintenance_is_in_flight() {
        let (sender, receiver) = mpsc::channel();
        let home = UzeHome::at(uze_testkit::temp::scratch("worker-coalesce"));
        let mut model = TuiModel {
            maintenance_in_flight: true,
            ..TuiModel::default()
        };

        dispatch(Intent::Refresh, &home, &sender, &mut model);

        assert!(receiver.try_recv().is_err());
        assert!(model.maintenance_in_flight);
        assert_eq!(model.status, Status::Idle);
    }

    #[test]
    fn only_a_web_address_is_handed_to_an_opener() {
        assert!(is_web_address("https://example.com"));
        assert!(is_web_address("HTTP://example.com"));
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "vscode://open",
            "/usr/bin/xterm",
            "http:/",
            "",
        ] {
            assert!(!is_web_address(refused), "{refused}");
            assert_eq!(open_in_browser(refused), None, "{refused}");
        }
    }

    /// Opening a link spawns processes, so the press answers when the
    /// opener does rather than holding the frame until then.
    #[test]
    fn a_link_is_opened_off_the_thread_that_pressed_it() {
        let (sender, receiver) = mpsc::channel();
        let home = UzeHome::at(uze_testkit::temp::scratch("worker-open-link"));
        let mut model = TuiModel::default();

        dispatch(
            Intent::OpenLink("file:///etc/passwd".to_owned()),
            &home,
            &sender,
            &mut model,
        );
        assert_eq!(model.status, Status::Idle, "nothing waited on the opener");

        let answer = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let (replay, replayed) = mpsc::channel();
        replay.send(answer).unwrap();
        assert!(drain_worker_results(&mut model, &replayed));
        assert_eq!(
            model.status,
            Status::Error("No browser to open file:///etc/passwd with".to_owned())
        );
    }

    fn setup_result(
        provisioning: uze_application::ProvisioningResult,
    ) -> uze_application::application::SetupResult {
        uze_application::application::SetupResult {
            integration: "example".to_owned(),
            detection: provisioning.detection.clone(),
            configured: provisioning.status == uze_application::ProvisionStatus::Verified,
            provisioning,
            runtime_shim: None,
            attach_error: None,
            shim_error: None,
        }
    }

    #[test]
    fn a_setup_outcome_says_what_was_done_and_where_the_executable_is() {
        let verified = uze_application::ProvisioningResult::verified(
            uze_application::ProvisionAction::Install,
            "official-test-route",
            uze_application::HarnessDetection {
                present: true,
                version: Some("v1.2.3".to_owned()),
            },
        )
        .found_outside_path(Some(PathBuf::from("/home/u/.example/bin/example")));
        assert_eq!(
            setup_outcome("example", &setup_result(verified)),
            "example ready (install; version v1.2.3); found at \
             /home/u/.example/bin/example, open a new shell to run it by name"
        );

        let blocked = uze_application::ProvisioningResult::blocked(
            "install it by following https://example.invalid/install",
        );
        assert_eq!(
            setup_outcome("example", &setup_result(blocked)),
            "example setup Blocked: install it by following https://example.invalid/install"
        );
    }
}
